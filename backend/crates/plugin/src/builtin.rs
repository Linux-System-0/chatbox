//! Built-in server plugins.
//!
//! Built-ins are compiled into the host and toggled by an administrator just
//! like discovered plugins, so operators can enable or disable them without a
//! filesystem change.

use async_trait::async_trait;
use chatbox_core::error::Result;

use crate::{HookContext, HookOutcome, PluginInfo, ServerPlugin};

/// Appends an audit record for every persisted message.
pub struct AuditLogPlugin;

#[async_trait]
impl ServerPlugin for AuditLogPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "audit-log".into(),
            version: "1.0.0".into(),
            api_version: crate::PLUGIN_API_VERSION,
            description: "Append an audit record for every message".into(),
            builtin: true,
        }
    }

    async fn on_message_after_persist(&self, ctx: &HookContext) -> Result<()> {
        tracing::info!(
            target: "chatbox::audit",
            payload = %ctx.payload,
            "message persisted"
        );
        Ok(())
    }

    async fn on_user_register(&self, ctx: &HookContext) -> Result<()> {
        tracing::info!(target: "chatbox::audit", payload = %ctx.payload, "user registered");
        Ok(())
    }

    async fn on_group_created(&self, ctx: &HookContext) -> Result<()> {
        tracing::info!(target: "chatbox::audit", payload = %ctx.payload, "group created");
        Ok(())
    }
}

/// Rejects text messages containing any configured blocked word.
pub struct MessageGuardPlugin {
    words: Vec<String>,
}

impl MessageGuardPlugin {
    /// Create a guard with the given blocked words (matched case-insensitively).
    pub fn new(words: Vec<String>) -> Self {
        Self {
            words: words.into_iter().map(|w| w.to_lowercase()).collect(),
        }
    }
}

#[async_trait]
impl ServerPlugin for MessageGuardPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "message-guard".into(),
            version: "1.0.0".into(),
            api_version: crate::PLUGIN_API_VERSION,
            description: "Reject messages that contain blocked words".into(),
            builtin: true,
        }
    }

    async fn on_message_before_persist(&self, ctx: &HookContext) -> Result<HookOutcome> {
        let content = ctx
            .payload
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_lowercase();
        for word in &self.words {
            if content.contains(word) {
                return Ok(HookOutcome::Reject(format!("message contains '{word}'")));
            }
        }
        Ok(HookOutcome::Allow)
    }
}
