//! Runtime configuration loaded from the environment.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};

use chatbox_storage::{BackendConfig, ShardConfig};

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
    /// MySQL shards, each with a primary and optional read replicas.
    pub mysql_shards: Vec<ShardConfig>,
    /// Single-node Redis URL.
    pub redis_url: Option<String>,
    /// Redis Cluster seed-node URLs. Takes precedence over `redis_url`.
    pub redis_cluster_nodes: Vec<String>,
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
    /// Maximum accepted upload size in bytes; `None` means unlimited and `0`
    /// disables uploads.
    pub max_upload_bytes: Option<u64>,
    /// Blocked words enforced by the built-in message guard.
    pub guard_words: Vec<String>,
    /// Deadline for one server-plugin hook call; zero disables it.
    pub plugin_hook_timeout_ms: u64,
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

        let mysql_shards = parse_shards(&std::env::var("MYSQL_SHARDS").unwrap_or_default())?;
        let redis_url = std::env::var("REDIS_URL").ok().filter(|s| !s.is_empty());
        let redis_cluster_nodes =
            parse_list(&std::env::var("REDIS_CLUSTER_NODES").unwrap_or_default());
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

        // Unset or empty means unlimited; `0` disables uploads.
        let max_upload_bytes = std::env::var("MAX_UPLOAD_BYTES")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse::<u64>()
                    .context("MAX_UPLOAD_BYTES must be an integer")
            })
            .transpose()?;

        let plugin_hook_timeout_ms = env_or("PLUGIN_HOOK_TIMEOUT_MS", "500")
            .parse::<u64>()
            .context("PLUGIN_HOOK_TIMEOUT_MS must be an integer")?;

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
            mysql_shards,
            redis_url,
            redis_cluster_nodes,
            redis_enabled,
            plugin_dir,
            object_dir,
            public_base_url,
            admin_user_ids,
            fanout_write_max_members,
            max_upload_bytes,
            guard_words,
            plugin_hook_timeout_ms,
        })
    }

    /// The hook deadline, or `None` when it is disabled.
    pub fn plugin_hook_timeout(&self) -> Option<std::time::Duration> {
        match self.plugin_hook_timeout_ms {
            0 => None,
            ms => Some(std::time::Duration::from_millis(ms)),
        }
    }

    /// Build the storage backend configuration.
    pub fn backend(&self) -> BackendConfig {
        BackendConfig {
            mysql_shards: self.mysql_shards.clone(),
            redis_url: self.redis_url.clone(),
            redis_cluster_nodes: self.redis_cluster_nodes.clone(),
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

/// Parse `MYSQL_SHARDS`.
///
/// Shards are separated by `;`. Within a shard, hosts are separated by `|`:
/// the first host is the writable primary and the rest are read replicas. For
/// example, two shards where the first has two replicas:
///
/// ```text
/// mysql://u:p@primary0/db|mysql://u:p@replica0a/db|mysql://u:p@replica0b/db;mysql://u:p@primary1/db
/// ```
fn parse_shards(raw: &str) -> Result<Vec<ShardConfig>> {
    let mut shards = Vec::new();
    for spec in raw.split(';') {
        let hosts: Vec<String> = spec
            .split('|')
            .map(|host| host.trim().to_string())
            .filter(|host| !host.is_empty())
            .collect();
        let Some((primary, replicas)) = hosts.split_first() else {
            continue;
        };
        shards.push(ShardConfig {
            primary: primary.clone(),
            replicas: replicas.to_vec(),
        });
    }
    Ok(shards)
}

/// Parse a comma-separated list, dropping empty entries.
fn parse_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_list, parse_shards};

    #[test]
    fn parses_single_shard_with_replicas() {
        let shards = parse_shards("mysql://p0/db|mysql://r0a/db|mysql://r0b/db").unwrap();
        assert_eq!(shards.len(), 1);
        assert_eq!(shards[0].primary, "mysql://p0/db");
        assert_eq!(shards[0].replicas, vec!["mysql://r0a/db", "mysql://r0b/db"]);
    }

    #[test]
    fn parses_multiple_shards() {
        let shards = parse_shards("mysql://p0/db;mysql://p1/db|mysql://r1/db").unwrap();
        assert_eq!(shards.len(), 2);
        assert!(shards[0].replicas.is_empty());
        assert_eq!(shards[1].primary, "mysql://p1/db");
        assert_eq!(shards[1].replicas, vec!["mysql://r1/db"]);
    }

    #[test]
    fn ignores_empty_shard_specs() {
        let shards = parse_shards("; mysql://p0/db ;; ").unwrap();
        assert_eq!(shards.len(), 1);
    }

    #[test]
    fn parses_cluster_nodes() {
        let nodes = parse_list("redis://h1:6379, redis://h2:6379,");
        assert_eq!(nodes, vec!["redis://h1:6379", "redis://h2:6379"]);
    }
}
