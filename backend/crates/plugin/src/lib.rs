//! ChatBox server plugin runtime.
//!
//! The host registers server plugins, tracks which are enabled (persisted via
//! [`chatbox_core::port::PluginRepo`]), and dispatches hooks. Client plugins
//! never run here; the host only serves their manifests (see
//! `docs/plugin-system.md`).

pub mod abi;
mod builtin;
mod dynamic;
pub mod manifest;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use chatbox_core::model::{ClientPluginManifest, ServerPluginInfo};
use chatbox_core::port::PluginRepo;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub use abi::PLUGIN_API_VERSION;
pub use builtin::{AuditLogPlugin, MessageGuardPlugin};
pub use dynamic::LoadedPlugin;
pub use manifest::{ClientManifest, ServerManifest};

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
    pub payload: serde_json::Value,
}

impl HookContext {
    /// Build a hook context.
    pub fn new(event: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            event: event.into(),
            payload,
        }
    }
}

/// The result of a before-persist hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    /// The operation may proceed.
    Allow,
    /// The operation is rejected with the given reason.
    Reject(String),
}

/// A server plugin implementation.
#[async_trait]
pub trait ServerPlugin: Send + Sync {
    /// Plugin metadata.
    fn info(&self) -> PluginInfo;

    /// Called before a message is persisted. Returning [`HookOutcome::Reject`]
    /// aborts the send.
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
    plugins: HashMap<String, Arc<dyn ServerPlugin>>,
    enabled: HashSet<String>,
    client: Vec<ClientPluginManifest>,
}

/// The server plugin host.
pub struct PluginHost {
    repo: Arc<dyn PluginRepo>,
    inner: RwLock<Inner>,
}

impl PluginHost {
    /// Create a host that persists enablement through `repo`.
    pub fn new(repo: Arc<dyn PluginRepo>) -> Self {
        Self {
            repo,
            inner: RwLock::new(Inner::default()),
        }
    }

    /// Register a compiled-in plugin (disabled until enabled).
    pub async fn register(&self, plugin: Arc<dyn ServerPlugin>) {
        let name = plugin.info().name;
        self.inner.write().await.plugins.insert(name, plugin);
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
            if enabled && inner.plugins.contains_key(&name) {
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
    pub async fn discover_dir(&self, dir: &Path) -> Result<usize> {
        if !dir.exists() {
            return Ok(0);
        }
        let mut registered = 0;
        for entry in std::fs::read_dir(dir).map_err(Error::storage)? {
            let path = entry.map_err(Error::storage)?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).map_err(Error::storage)?;
            let value: serde_json::Value = serde_json::from_str(&raw)
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
                let plugin = unsafe { LoadedPlugin::load(&library_path, manifest_info) };
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

    /// List registered server plugins and their enablement.
    pub async fn list(&self) -> Vec<ServerPluginInfo> {
        let inner = self.inner.read().await;
        let mut infos: Vec<ServerPluginInfo> = inner
            .plugins
            .values()
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
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// Enable a plugin and persist the state.
    pub async fn enable(&self, name: &str) -> Result<()> {
        {
            let mut inner = self.inner.write().await;
            if !inner.plugins.contains_key(name) {
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
            if !inner.plugins.contains_key(name) {
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
            .enabled
            .iter()
            .filter_map(|name| inner.plugins.get(name).cloned())
            .collect()
    }

    /// Run the before-persist hook chain. The first rejection wins; a failing
    /// plugin is logged and skipped so it cannot block the pipeline.
    pub async fn on_message_before_persist(&self, ctx: &HookContext) -> Result<HookOutcome> {
        for plugin in self.enabled_plugins().await {
            match plugin.on_message_before_persist(ctx).await {
                Ok(HookOutcome::Reject(reason)) => return Ok(HookOutcome::Reject(reason)),
                Ok(HookOutcome::Allow) => {}
                Err(e) => {
                    tracing::warn!(plugin = %plugin.info().name, error = %e, "before_persist hook failed")
                }
            }
        }
        Ok(HookOutcome::Allow)
    }

    /// Run the after-persist hook chain.
    pub async fn on_message_after_persist(&self, ctx: &HookContext) -> Result<()> {
        for plugin in self.enabled_plugins().await {
            if let Err(e) = plugin.on_message_after_persist(ctx).await {
                tracing::warn!(plugin = %plugin.info().name, error = %e, "after_persist hook failed");
            }
        }
        Ok(())
    }

    /// Run the user-registration hook chain.
    pub async fn on_user_register(&self, ctx: &HookContext) -> Result<()> {
        for plugin in self.enabled_plugins().await {
            if let Err(e) = plugin.on_user_register(ctx).await {
                tracing::warn!(plugin = %plugin.info().name, error = %e, "user_register hook failed");
            }
        }
        Ok(())
    }

    /// Run the group-created hook chain.
    pub async fn on_group_created(&self, ctx: &HookContext) -> Result<()> {
        for plugin in self.enabled_plugins().await {
            if let Err(e) = plugin.on_group_created(ctx).await {
                tracing::warn!(plugin = %plugin.info().name, error = %e, "group_created hook failed");
            }
        }
        Ok(())
    }
}
