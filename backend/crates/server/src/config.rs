//! Runtime configuration loaded from the environment.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};

use chatbox_storage::BackendConfig;

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP listen address.
    pub http_bind: String,
    /// JWT signing secret.
    pub jwt_secret: String,
    /// This node's stable identifier.
    pub node_id: String,
    /// This node's Snowflake number (0..1023).
    pub node_number: u64,
    /// MySQL primary URL.
    pub database_url: Option<String>,
    /// MySQL replica URL.
    pub database_replica_url: Option<String>,
    /// Redis URL.
    pub redis_url: Option<String>,
    /// Whether Redis is enabled.
    pub redis_enabled: bool,
    /// Directory scanned for plugin manifests.
    pub plugin_dir: PathBuf,
    /// Directory backing local object storage.
    pub object_dir: PathBuf,
    /// Public base URL used for local pre-signed URLs.
    pub public_base_url: String,
    /// Custom user ids allowed to manage server plugins.
    pub admin_user_ids: HashSet<String>,
    /// Groups larger than this fan out on read.
    pub fanout_write_max_members: u64,
    /// Blocked words enforced by the built-in message guard.
    pub guard_words: Vec<String>,
}

impl Config {
    /// Load configuration from the process environment.
    pub fn from_env() -> Result<Self> {
        let http_bind = env_or("HTTP_BIND", "0.0.0.0:8080");
        let jwt_secret = env_or("JWT_SECRET", "chatbox-development-secret");

        let node_id = env_or("NODE_ID", &format!("node-{}", std::process::id()));
        let node_number = env_or("NODE_NUMBER", "1")
            .parse::<u64>()
            .context("NODE_NUMBER must be an integer")?
            & 0x3ff;

        let database_url = std::env::var("DATABASE_URL").ok().filter(|s| !s.is_empty());
        let database_replica_url = std::env::var("DATABASE_REPLICA_URL")
            .ok()
            .filter(|s| !s.is_empty());
        let redis_url = std::env::var("REDIS_URL").ok().filter(|s| !s.is_empty());
        let redis_enabled = env_or("REDIS_ENABLED", "true")
            .parse::<bool>()
            .unwrap_or(true);

        let plugin_dir = PathBuf::from(env_or("PLUGIN_DIR", "plugins"));
        let object_dir = PathBuf::from(env_or("OBJECT_STORAGE_DIR", "data/objects"));
        let public_base_url = env_or("PUBLIC_BASE_URL", "http://127.0.0.1:8080");

        let admin_user_ids = std::env::var("ADMIN_USER_IDS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let fanout_write_max_members = env_or("FANOUT_WRITE_MAX_MEMBERS", "256")
            .parse::<u64>()
            .context("FANOUT_WRITE_MAX_MEMBERS must be an integer")?;

        let guard_words = std::env::var("MESSAGE_GUARD_WORDS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            http_bind,
            jwt_secret,
            node_id,
            node_number,
            database_url,
            database_replica_url,
            redis_url,
            redis_enabled,
            plugin_dir,
            object_dir,
            public_base_url,
            admin_user_ids,
            fanout_write_max_members,
            guard_words,
        })
    }

    /// Build the storage backend configuration.
    pub fn backend(&self) -> BackendConfig {
        BackendConfig {
            database_url: self.database_url.clone(),
            database_replica_url: self.database_replica_url.clone(),
            redis_url: self.redis_url.clone(),
            redis_enabled: self.redis_enabled,
            object_dir: self.object_dir.clone(),
            public_base_url: self.public_base_url.clone(),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}
