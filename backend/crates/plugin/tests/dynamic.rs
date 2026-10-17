//! Loads a real shared library to exercise the C ABI end to end.
//!
//! The fixture is compiled with the system C compiler. When no compiler is
//! available the test is skipped so minimal CI images still pass.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatbox_core::SystemClock;
use chatbox_core::error::Result as CoreResult;
use chatbox_core::model::Timestamp;
use chatbox_core::port::{PluginRepo, Stores};
use chatbox_plugin::{
    HookContext, HookOutcome, LoadedPlugin, PLUGIN_API_VERSION, PluginHost, PluginInfo, ServerHost,
    ServerPlugin,
};
use chatbox_storage::memory::MemoryStore;
use serde_json::json;

/// A minimal C plugin: it calls the host `now` method and rewrites the message.
const PLUGIN_SOURCE: &str = r#"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef char *(*hook_fn)(const char *context, const void *host);

struct host_api {
    uint32_t api_version;
    void *ctx;
    char *(*call)(void *ctx, const char *method, const char *params);
    void (*free_string)(char *value);
};

struct plugin_vtable {
    uint32_t api_version;
    const char *name;
    const char *version;
    const char *description;
    hook_fn on_message_before_persist;
    hook_fn on_message_after_persist;
    hook_fn on_user_register;
    hook_fn on_group_created;
    void (*free_string)(char *value);
};

static void release(char *value) { free(value); }

static char *before(const char *context, const void *host_ptr) {
    const struct host_api *host = (const struct host_api *)host_ptr;
    if (host == NULL || host->call == NULL) {
        return NULL;
    }
    char *now = host->call(host->ctx, "now", NULL);
    if (now == NULL || strstr(now, "\"ok\":true") == NULL) {
        return NULL;
    }
    host->free_string(now);
    char *result = malloc(48);
    strcpy(result, "{\"outcome\":\"rewrite\",\"content\":\"redacted\"}");
    return result;
}

int32_t chatbox_plugin_init(uint32_t api_version,
                            struct plugin_vtable *vtable,
                            const struct host_api *host) {
    (void)host;
    if (api_version != 2 || vtable == NULL) {
        return 1;
    }
    vtable->api_version = 2;
    vtable->name = "c-rewriter";
    vtable->version = "1.0.0";
    vtable->description = "C test plugin";
    vtable->on_message_before_persist = before;
    vtable->free_string = release;
    return 0;
}
"#;

fn compile(dir: &Path) -> Option<PathBuf> {
    let source = dir.join("plugin.c");
    std::fs::write(&source, PLUGIN_SOURCE).ok()?;
    let library = dir.join("libtest_plugin.so");
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-O0", "-o"])
        .arg(&library)
        .arg(&source)
        .status()
        .ok()?;
    status.success().then_some(library)
}

#[derive(Default)]
struct Repo(Mutex<HashSet<String>>);

#[async_trait]
impl PluginRepo for Repo {
    async fn is_enabled(&self, name: &str) -> CoreResult<bool> {
        Ok(self.0.lock().unwrap().contains(name))
    }

    async fn set_enabled(&self, name: &str, enabled: bool, _at: Timestamp) -> CoreResult<()> {
        let mut states = self.0.lock().unwrap();
        if enabled {
            states.insert(name.to_string());
        } else {
            states.remove(name);
        }
        Ok(())
    }

    async fn list(&self) -> CoreResult<Vec<(String, bool)>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|name| (name.clone(), true))
            .collect())
    }
}

fn stores() -> Stores {
    let memory = Arc::new(MemoryStore::new());
    Stores {
        users: memory.clone(),
        sessions: memory.clone(),
        devices: memory.clone(),
        conversations: memory.clone(),
        messages: memory.clone(),
        contacts: memory.clone(),
        friend_requests: memory.clone(),
        groups: memory.clone(),
        blocks: memory.clone(),
        attachments: memory.clone(),
        outbox: memory.clone(),
        plugins: memory.clone(),
        plugin_kv: memory,
    }
}

#[tokio::test]
async fn loads_and_runs_a_c_plugin() {
    let dir = std::env::temp_dir().join(format!("chatbox-plugin-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let Some(library) = compile(&dir) else {
        eprintln!("skipping: no working C compiler");
        return;
    };

    let plugin_host = Arc::new(ServerHost::new(stores(), Arc::new(SystemClock)));
    let info = PluginInfo {
        name: "c-rewriter".into(),
        version: "1.0.0".into(),
        api_version: PLUGIN_API_VERSION,
        description: "C test plugin".into(),
        builtin: false,
    };
    // SAFETY: the test just compiled this library from the source above.
    let plugin = unsafe { LoadedPlugin::load(&library, info, Some(plugin_host.clone())) }.unwrap();
    assert_eq!(plugin.info().name, "c-rewriter");

    let ctx = HookContext::new(
        "message.before_persist",
        json!({ "conversation_id": 1, "kind": "text", "content": "secret" }),
    );
    let outcome = plugin.on_message_before_persist(&ctx).await.unwrap();
    assert_eq!(
        outcome,
        HookOutcome::Rewrite(chatbox_plugin::MessagePatch {
            content: Some("redacted".into()),
            kind: None,
        })
    );

    // The same library works through the host wrapper, including the deadline.
    let host = PluginHost::new(Arc::new(Repo::default()))
        .with_host(plugin_host)
        .with_timeout(Some(std::time::Duration::from_secs(1)));
    host.register(Arc::new(plugin)).await;
    host.enable("c-rewriter").await.unwrap();
    assert_eq!(
        host.on_message_before_persist(&ctx).await.unwrap(),
        HookOutcome::Rewrite(chatbox_plugin::MessagePatch {
            content: Some("redacted".into()),
            kind: None,
        })
    );

    let _ = std::fs::remove_dir_all(&dir);
}
