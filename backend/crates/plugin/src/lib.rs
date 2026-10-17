//! ChatBox server plugin runtime.
//!
//! The host registers server plugins, tracks which are enabled (persisted via
//! [`chatbox_core::port::PluginRepo`]), and dispatches hooks. Client plugins
//! never run here; the host only serves their manifests (see
//! `docs/plugin-system.md`).
//!
//! Hooks run in registration order, are bounded by an optional deadline, and
//! are isolated: a failing, panicking, or slow plugin is logged and skipped so
//! it cannot block the delivery pipeline.

pub mod abi;
mod builtin;
mod dynamic;
pub mod host;
pub mod manifest;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use chatbox_core::model::{ClientPluginManifest, MessageKind, ServerPluginInfo};
use chatbox_core::port::PluginRepo;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::RwLock;

pub use abi::PLUGIN_API_VERSION;
pub use builtin::{AuditLogPlugin, MessageGuardPlugin};
pub use dynamic::LoadedPlugin;
pub use host::ServerHost;
pub use manifest::{ClientManifest, ServerManifest};

/// Default hook deadline when the host is not configured otherwise.
pub const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_millis(500);

/// Metadata for a registered server plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Declared plugin API version.
    pub api_version: u32,
    /// Human description.
    pub description: String,
    /// Whether the plugin is compiled into the host.
    pub builtin: bool,
}

/// The data passed to a hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// Event name, e.g. `message.before_persist`.
    pub event: String,
    /// Event payload.
    pub payload: Value,
}

impl HookContext {
    /// Build a hook context.
    pub fn new(event: impl Into<String>, payload: Value) -> Self {
        Self {
            event: event.into(),
            payload,
        }
    }
}

/// Fields a before-persist hook may replace.
///
/// `attachment_id` and `client_msg_id` are deliberately not part of the patch:
/// plugins can redact or retype a message, not retarget it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePatch {
    /// Replacement message body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Replacement message kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MessageKind>,
}

/// The result of a before-persist hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    /// The operation may proceed unchanged.
    Allow,
    /// The operation is rejected with the given reason.
    Reject(String),
    /// The message is rewritten before it is persisted.
    Rewrite(MessagePatch),
}

/// Maximum reason length returned to a client after a rejection.
const MAX_REJECT_REASON: usize = 500;

/// Parse the JSON result of a before-persist hook.
pub fn parse_before_result(value: &Value) -> Result<HookOutcome> {
    #[derive(Deserialize)]
    struct Raw {
        outcome: String,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        kind: Option<MessageKind>,
    }

    let raw: Raw = serde_json::from_value(value.clone())
        .map_err(|e| Error::Invalid(format!("invalid plugin result: {e}")))?;
    match raw.outcome.as_str() {
        "allow" => Ok(HookOutcome::Allow),
        "reject" => {
            let reason = raw
                .reason
                .unwrap_or_else(|| "rejected by a server plugin".to_string());
            Ok(HookOutcome::Reject(
                reason.chars().take(MAX_REJECT_REASON).collect(),
            ))
        }
        "rewrite" => {
            if raw.content.is_none() && raw.kind.is_none() {
                return Err(Error::Invalid(
                    "rewrite result must set content, kind, or both".into(),
                ));
            }
            Ok(HookOutcome::Rewrite(MessagePatch {
                content: raw.content,
                kind: raw.kind,
            }))
        }
        other => Err(Error::Invalid(format!("unknown outcome '{other}'"))),
    }
}

/// A server plugin implementation.
#[async_trait]
pub trait ServerPlugin: Send + Sync {
    /// Plugin metadata.
    fn info(&self) -> PluginInfo;

    /// Called before a message is persisted. Returning [`HookOutcome::Reject`]
    /// aborts the send; [`HookOutcome::Rewrite`] replaces fields before the
    /// message is stored.
    async fn on_message_before_persist(&self, _ctx: &HookContext) -> Result<HookOutcome> {
        Ok(HookOutcome::Allow)
    }

    /// Called after a message is persisted.
    async fn on_message_after_persist(&self, _ctx: &HookContext) -> Result<()> {
        Ok(())
    }

    /// Called after a user registers.
    async fn on_user_register(&self, _ctx: &HookContext) -> Result<()> {
        Ok(())
    }

    /// Called after a group is created.
    async fn on_group_created(&self, _ctx: &HookContext) -> Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct Inner {
    // Registration order is part of the contract, so plugins live in a Vec and
    // the hash map only indexes it.
    plugins: Vec<Arc<dyn ServerPlugin>>,
    index: HashMap<String, usize>,
    enabled: HashSet<String>,
    client: Vec<ClientPluginManifest>,
}

/// The server plugin host.
pub struct PluginHost {
    repo: Arc<dyn PluginRepo>,
    host: Option<Arc<ServerHost>>,
    timeout: Option<Duration>,
    inner: RwLock<Inner>,
}

impl PluginHost {
    /// Create a host that persists enablement through `repo`.
    pub fn new(repo: Arc<dyn PluginRepo>) -> Self {
        Self {
            repo,
            host: None,
            timeout: Some(DEFAULT_HOOK_TIMEOUT),
            inner: RwLock::new(Inner::default()),
        }
    }

    /// Attach the host API used by dynamically loaded plugins.
    pub fn with_host(mut self, host: Arc<ServerHost>) -> Self {
        self.host = Some(host);
        self
    }

    /// Set the per-hook deadline; `None` disables it.
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    /// Register a compiled-in plugin (disabled until enabled).
    pub async fn register(&self, plugin: Arc<dyn ServerPlugin>) {
        let name = plugin.info().name;
        let mut inner = self.inner.write().await;
        if let Some(index) = inner.index.get(&name).copied() {
            inner.plugins[index] = plugin;
        } else {
            let position = inner.plugins.len();
            inner.index.insert(name, position);
            inner.plugins.push(plugin);
        }
    }

    /// Register a client plugin manifest to be served to clients.
    pub async fn register_client(&self, manifest: ClientPluginManifest) {
        let mut inner = self.inner.write().await;
        if !inner.client.iter().any(|m| m.name == manifest.name) {
            inner.client.push(manifest);
        }
    }

    /// Load the persisted enabled set after all plugins are registered.
    pub async fn bootstrap(&self) -> Result<()> {
        let states = self.repo.list().await?;
        let mut inner = self.inner.write().await;
        for (name, enabled) in states {
            if enabled && inner.index.contains_key(&name) {
                inner.enabled.insert(name);
            }
        }
        Ok(())
    }

    /// Register the built-in client language packs.
    pub async fn register_builtin_client_manifests(&self) {
        for (name, entry, description) in [
            ("en", "translations/en.json", "English (built-in)"),
            (
                "zh-CN",
                "translations/zh-CN.json",
                "Simplified Chinese translations",
            ),
        ] {
            self.register_client(ClientPluginManifest {
                name: name.into(),
                version: "1.0.0".into(),
                kind: "language-pack".into(),
                entry: entry.into(),
                description: description.into(),
            })
            .await;
        }
    }

    /// Scan a directory for plugin manifests and register what loads.
    ///
    /// Manifests are processed in file-name order so the hook chain is
    /// deterministic across machines and filesystems.
    pub async fn discover_dir(&self, dir: &Path) -> Result<usize> {
        if !dir.exists() {
            return Ok(0);
        }
        let mut paths = std::fs::read_dir(dir)
            .map_err(Error::storage)?
            .map(|entry| entry.map(|entry| entry.path()).map_err(Error::storage))
            .collect::<Result<Vec<_>>>()?;
        paths.sort();
        let mut registered = 0;
        for path in paths {
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).map_err(Error::storage)?;
            let value: Value = serde_json::from_str(&raw)
                .map_err(|e| Error::Invalid(format!("invalid manifest: {e}")))?;
            let kind = value.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if kind == "server" {
                let manifest = ServerManifest::load(&path)?;
                if manifest.api_version != PLUGIN_API_VERSION {
                    tracing::warn!(
                        plugin = %manifest.name,
                        "skipping plugin with unsupported api_version"
                    );
                    continue;
                }
                let library_path = dir.join(&manifest.entry);
                if !library_path.exists() {
                    tracing::warn!(
                        plugin = %manifest.name,
                        path = %library_path.display(),
                        "plugin artifact is missing; not registered"
                    );
                    continue;
                }
                let manifest_info = PluginInfo {
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    api_version: manifest.api_version,
                    description: manifest.description.clone(),
                    builtin: false,
                };
                // SAFETY: the manifest declares a ChatBox plugin; loading is an
                // explicit administrator action.
                let plugin =
                    unsafe { LoadedPlugin::load(&library_path, manifest_info, self.host.clone()) };
                match plugin {
                    Ok(plugin) => {
                        self.register(Arc::new(plugin)).await;
                        registered += 1;
                    }
                    Err(e) => {
                        tracing::error!(plugin = %manifest.name, error = %e, "failed to load plugin")
                    }
                }
            } else if let Ok(client) = ClientManifest::load(&path) {
                self.register_client(client.0).await;
                registered += 1;
            }
        }
        Ok(registered)
    }

    /// List registered server plugins and their enablement, in hook order.
    pub async fn list(&self) -> Vec<ServerPluginInfo> {
        let inner = self.inner.read().await;
        inner
            .plugins
            .iter()
            .map(|plugin| {
                let info = plugin.info();
                let enabled = inner.enabled.contains(&info.name);
                ServerPluginInfo {
                    name: info.name,
                    version: info.version,
                    api_version: info.api_version,
                    description: info.description,
                    enabled,
                    builtin: info.builtin,
                }
            })
            .collect()
    }

    /// Enable a plugin and persist the state.
    pub async fn enable(&self, name: &str) -> Result<()> {
        {
            let mut inner = self.inner.write().await;
            if !inner.index.contains_key(name) {
                return Err(Error::NotFound);
            }
            inner.enabled.insert(name.to_string());
        }
        self.repo
            .set_enabled(name, true, chrono::Utc::now())
            .await?;
        Ok(())
    }

    /// Disable a plugin and persist the state.
    pub async fn disable(&self, name: &str) -> Result<()> {
        {
            let mut inner = self.inner.write().await;
            if !inner.index.contains_key(name) {
                return Err(Error::NotFound);
            }
            inner.enabled.remove(name);
        }
        self.repo
            .set_enabled(name, false, chrono::Utc::now())
            .await?;
        Ok(())
    }

    /// Client plugin manifests (informational only; no server effect).
    pub async fn client_manifests(&self) -> Vec<ClientPluginManifest> {
        self.inner.read().await.client.clone()
    }

    async fn enabled_plugins(&self) -> Vec<Arc<dyn ServerPlugin>> {
        let inner = self.inner.read().await;
        inner
            .plugins
            .iter()
            .filter(|plugin| inner.enabled.contains(&plugin.info().name))
            .cloned()
            .collect()
    }

    /// Await one hook call with panic isolation and the configured deadline.
    async fn guarded<T, F>(&self, future: F) -> Result<T>
    where
        F: Future<Output = Result<T>>,
    {
        let future = AssertUnwindSafe(future).catch_unwind();
        let future = async {
            match future.await {
                Ok(result) => result,
                Err(_) => Err(Error::Internal("plugin panicked".into())),
            }
        };
        match self.timeout {
            Some(timeout) => match tokio::time::timeout(timeout, future).await {
                Ok(result) => result,
                Err(_) => Err(Error::Internal(format!(
                    "plugin hook exceeded the {timeout:?} deadline"
                ))),
            },
            None => future.await,
        }
    }

    /// Run the before-persist hook chain.
    ///
    /// Rewrites compose in registration order: every later plugin sees the
    /// payload produced by earlier rewrites. The first rejection wins; a
    /// failing plugin is logged and skipped.
    pub async fn on_message_before_persist(&self, ctx: &HookContext) -> Result<HookOutcome> {
        let mut payload = ctx.payload.clone();
        let mut patch: Option<MessagePatch> = None;
        for plugin in self.enabled_plugins().await {
            let name = plugin.info().name;
            let hook_ctx = HookContext::new(ctx.event.clone(), payload.clone());
            let outcome = match self
                .guarded(plugin.on_message_before_persist(&hook_ctx))
                .await
            {
                Ok(outcome) => outcome,
                Err(e) => {
                    tracing::warn!(plugin = %name, error = %e, "before_persist hook failed");
                    continue;
                }
            };
            match outcome {
                HookOutcome::Allow => {}
                HookOutcome::Reject(reason) => return Ok(HookOutcome::Reject(reason)),
                HookOutcome::Rewrite(rewrite) => {
                    if let Some(content) = &rewrite.content {
                        payload["content"] = json!(content);
                    }
                    if let Some(kind) = rewrite.kind {
                        payload["kind"] = json!(kind);
                    }
                    let merged = patch.get_or_insert_with(MessagePatch::default);
                    if rewrite.content.is_some() {
                        merged.content = rewrite.content;
                    }
                    if rewrite.kind.is_some() {
                        merged.kind = rewrite.kind;
                    }
                }
            }
        }
        Ok(match patch {
            Some(patch) => HookOutcome::Rewrite(patch),
            None => HookOutcome::Allow,
        })
    }

    /// Run the after-persist hook chain.
    pub async fn on_message_after_persist(&self, ctx: &HookContext) -> Result<()> {
        for plugin in self.enabled_plugins().await {
            let name = plugin.info().name;
            if let Err(e) = self.guarded(plugin.on_message_after_persist(ctx)).await {
                tracing::warn!(plugin = %name, error = %e, "after_persist hook failed");
            }
        }
        Ok(())
    }

    /// Run the user-registration hook chain.
    pub async fn on_user_register(&self, ctx: &HookContext) -> Result<()> {
        for plugin in self.enabled_plugins().await {
            let name = plugin.info().name;
            if let Err(e) = self.guarded(plugin.on_user_register(ctx)).await {
                tracing::warn!(plugin = %name, error = %e, "user_register hook failed");
            }
        }
        Ok(())
    }

    /// Run the group-created hook chain.
    pub async fn on_group_created(&self, ctx: &HookContext) -> Result<()> {
        for plugin in self.enabled_plugins().await {
            let name = plugin.info().name;
            if let Err(e) = self.guarded(plugin.on_group_created(ctx)).await {
                tracing::warn!(plugin = %name, error = %e, "group_created hook failed");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use chatbox_core::error::Result;
    use chatbox_core::model::{MessageKind, Timestamp};
    use chatbox_core::port::PluginRepo;

    use super::*;

    #[derive(Default)]
    struct TestRepo(Mutex<HashSet<String>>);

    #[async_trait]
    impl PluginRepo for TestRepo {
        async fn is_enabled(&self, name: &str) -> Result<bool> {
            Ok(self.0.lock().unwrap().contains(name))
        }

        async fn set_enabled(&self, name: &str, enabled: bool, _at: Timestamp) -> Result<()> {
            let mut states = self.0.lock().unwrap();
            if enabled {
                states.insert(name.to_string());
            } else {
                states.remove(name);
            }
            Ok(())
        }

        async fn list(&self) -> Result<Vec<(String, bool)>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|name| (name.clone(), true))
                .collect())
        }
    }

    async fn host(timeout: Option<Duration>) -> PluginHost {
        PluginHost::new(Arc::new(TestRepo::default())).with_timeout(timeout)
    }

    /// Appends a suffix to the text content.
    struct Suffix(&'static str);

    #[async_trait]
    impl ServerPlugin for Suffix {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: format!("suffix{}", self.0),
                version: "1.0.0".into(),
                api_version: PLUGIN_API_VERSION,
                description: "append a suffix".into(),
                builtin: false,
            }
        }

        async fn on_message_before_persist(&self, ctx: &HookContext) -> Result<HookOutcome> {
            let content = ctx.payload["content"].as_str().unwrap_or_default();
            Ok(HookOutcome::Rewrite(MessagePatch {
                content: Some(format!("{content}{}", self.0)),
                kind: None,
            }))
        }
    }

    /// Records the payload it observed, then leaves it unchanged.
    struct Observer {
        seen: Arc<Mutex<Option<String>>>,
    }

    #[async_trait]
    impl ServerPlugin for Observer {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "observer".into(),
                version: "1.0.0".into(),
                api_version: PLUGIN_API_VERSION,
                description: "observe the chain".into(),
                builtin: false,
            }
        }

        async fn on_message_before_persist(&self, ctx: &HookContext) -> Result<HookOutcome> {
            *self.seen.lock().unwrap() = ctx.payload["content"].as_str().map(str::to_string);
            Ok(HookOutcome::Allow)
        }
    }

    struct KindSetter;

    #[async_trait]
    impl ServerPlugin for KindSetter {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "kind".into(),
                version: "1.0.0".into(),
                api_version: PLUGIN_API_VERSION,
                description: "retype".into(),
                builtin: false,
            }
        }

        async fn on_message_before_persist(&self, _ctx: &HookContext) -> Result<HookOutcome> {
            Ok(HookOutcome::Rewrite(MessagePatch {
                content: None,
                kind: Some(MessageKind::System),
            }))
        }
    }

    struct Rejecter;

    #[async_trait]
    impl ServerPlugin for Rejecter {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "rejecter".into(),
                version: "1.0.0".into(),
                api_version: PLUGIN_API_VERSION,
                description: "reject".into(),
                builtin: false,
            }
        }

        async fn on_message_before_persist(&self, _ctx: &HookContext) -> Result<HookOutcome> {
            Ok(HookOutcome::Reject("blocked".into()))
        }
    }

    struct Panicker;

    #[async_trait]
    impl ServerPlugin for Panicker {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "panicker".into(),
                version: "1.0.0".into(),
                api_version: PLUGIN_API_VERSION,
                description: "panic".into(),
                builtin: false,
            }
        }

        async fn on_message_before_persist(&self, _ctx: &HookContext) -> Result<HookOutcome> {
            panic!("boom");
        }
    }

    struct Sleeper(Duration);

    #[async_trait]
    impl ServerPlugin for Sleeper {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                name: "sleeper".into(),
                version: "1.0.0".into(),
                api_version: PLUGIN_API_VERSION,
                description: "sleep".into(),
                builtin: false,
            }
        }

        async fn on_message_before_persist(&self, _ctx: &HookContext) -> Result<HookOutcome> {
            tokio::time::sleep(self.0).await;
            Ok(HookOutcome::Reject("too late".into()))
        }
    }

    fn ctx(content: &str) -> HookContext {
        HookContext::new(
            "message.before_persist",
            json!({ "conversation_id": 1, "kind": "text", "content": content }),
        )
    }

    #[tokio::test]
    async fn rewrites_compose_in_registration_order() {
        let plugins = host(Some(Duration::from_secs(1))).await;
        let seen = Arc::new(Mutex::new(None));
        plugins.register(Arc::new(Suffix("!"))).await;
        plugins
            .register(Arc::new(Observer { seen: seen.clone() }))
            .await;
        plugins.register(Arc::new(KindSetter)).await;
        for name in ["suffix!", "observer", "kind"] {
            plugins.enable(name).await.unwrap();
        }

        let outcome = plugins.on_message_before_persist(&ctx("hi")).await.unwrap();
        assert_eq!(
            outcome,
            HookOutcome::Rewrite(MessagePatch {
                content: Some("hi!".into()),
                kind: Some(MessageKind::System),
            })
        );
        // The second plugin observed the first plugin's rewrite.
        assert_eq!(*seen.lock().unwrap(), Some("hi!".to_string()));

        // The original context is never mutated.
        assert_eq!(ctx("hi").payload["content"], "hi");
    }

    #[tokio::test]
    async fn first_rejection_wins() {
        let plugins = host(Some(Duration::from_secs(1))).await;
        plugins.register(Arc::new(Suffix("!"))).await;
        plugins.register(Arc::new(Rejecter)).await;
        for name in ["suffix!", "rejecter"] {
            plugins.enable(name).await.unwrap();
        }
        assert_eq!(
            plugins.on_message_before_persist(&ctx("hi")).await.unwrap(),
            HookOutcome::Reject("blocked".into())
        );
    }

    #[tokio::test]
    async fn disabled_plugins_are_skipped() {
        let plugins = host(Some(Duration::from_secs(1))).await;
        plugins.register(Arc::new(Suffix("!"))).await;
        assert_eq!(
            plugins.on_message_before_persist(&ctx("hi")).await.unwrap(),
            HookOutcome::Allow
        );
    }

    #[tokio::test]
    async fn panicking_plugin_is_skipped() {
        let plugins = host(Some(Duration::from_secs(1))).await;
        plugins.register(Arc::new(Panicker)).await;
        plugins.register(Arc::new(Suffix("!"))).await;
        for name in ["panicker", "suffix!"] {
            plugins.enable(name).await.unwrap();
        }
        assert_eq!(
            plugins.on_message_before_persist(&ctx("hi")).await.unwrap(),
            HookOutcome::Rewrite(MessagePatch {
                content: Some("hi!".into()),
                kind: None,
            })
        );
    }

    #[tokio::test]
    async fn slow_plugin_is_skipped() {
        let plugins = host(Some(Duration::from_millis(20))).await;
        plugins
            .register(Arc::new(Sleeper(Duration::from_millis(200))))
            .await;
        plugins.enable("sleeper").await.unwrap();
        assert_eq!(
            plugins.on_message_before_persist(&ctx("hi")).await.unwrap(),
            HookOutcome::Allow
        );
    }

    #[test]
    fn parses_hook_results() {
        assert_eq!(
            parse_before_result(&json!({ "outcome": "allow" })).unwrap(),
            HookOutcome::Allow
        );
        assert_eq!(
            parse_before_result(&json!({ "outcome": "reject", "reason": "no" })).unwrap(),
            HookOutcome::Reject("no".into())
        );
        assert_eq!(
            parse_before_result(&json!({ "outcome": "rewrite", "content": "safe" })).unwrap(),
            HookOutcome::Rewrite(MessagePatch {
                content: Some("safe".into()),
                kind: None,
            })
        );
        assert!(parse_before_result(&json!({ "outcome": "rewrite" })).is_err());
        assert!(parse_before_result(&json!({ "outcome": "explode" })).is_err());
        assert!(parse_before_result(&json!({})).is_err());
    }
}
