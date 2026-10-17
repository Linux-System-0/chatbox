//! Plugin manifest parsing and discovery.

use std::path::Path;

use chatbox_core::error::{Error, Result};
use chatbox_core::model::{ClientPluginManifest, ServerPluginInfo};
use serde::{Deserialize, Serialize};

/// A server plugin manifest on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerManifest {
    /// Plugin name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Tier; must be `server`.
    pub kind: String,
    /// Declared API version.
    pub api_version: u32,
    /// Relative path to the shared library.
    pub entry: String,
    /// Human description.
    #[serde(default)]
    pub description: String,
}

impl ServerManifest {
    /// Parse a manifest file.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(Error::storage)?;
        let manifest: ServerManifest = serde_json::from_str(&raw)
            .map_err(|e| Error::Invalid(format!("invalid plugin manifest: {e}")))?;
        if manifest.kind != "server" {
            return Err(Error::Invalid(format!(
                "manifest {} is not a server plugin",
                path.display()
            )));
        }
        Ok(manifest)
    }

    /// Convert into the public info type.
    pub fn info(&self, enabled: bool) -> ServerPluginInfo {
        ServerPluginInfo {
            name: self.name.clone(),
            version: self.version.clone(),
            api_version: self.api_version,
            description: self.description.clone(),
            enabled,
            builtin: false,
        }
    }
}

/// A client plugin manifest on disk, distinguished by a non-`server` `kind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientManifest(pub ClientPluginManifest);

impl ClientManifest {
    /// Parse a client manifest file.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(Error::storage)?;
        let manifest: ClientManifest = serde_json::from_str(&raw)
            .map_err(|e| Error::Invalid(format!("invalid client manifest: {e}")))?;
        if manifest.0.kind == "server" {
            return Err(Error::Invalid(format!(
                "manifest {} is a server plugin",
                path.display()
            )));
        }
        Ok(manifest)
    }
}
