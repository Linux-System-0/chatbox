//! Backend selection and construction.

use std::path::PathBuf;
use std::sync::Arc;

use chatbox_core::Infra;
use chatbox_core::error::Result;
use chatbox_core::infra::{
    Cache, ConnectionRegistry, MessageBus, ObjectStorage, PresenceStore, RateLimiter,
};
use chatbox_core::port::Stores;

use crate::memory::MemoryStore;
use crate::mysql::MySqlStore;
use crate::object::LocalObjectStorage;
use crate::redis::RedisInfra;

/// One MySQL shard: a writable primary plus zero or more read replicas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardConfig {
    /// Primary connection string.
    pub primary: String,
    /// Read-replica connection strings served round-robin.
    pub replicas: Vec<String>,
}

impl ShardConfig {
    /// A shard with no replicas.
    pub fn primary(primary: impl Into<String>) -> Self {
        Self {
            primary: primary.into(),
            replicas: Vec::new(),
        }
    }
}

/// Configuration selecting the storage backend.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// MySQL shards. When empty, the in-memory backend is used.
    pub mysql_shards: Vec<ShardConfig>,
    /// Single-node Redis URL.
    pub redis_url: Option<String>,
    /// Redis Cluster seed-node URLs. Takes precedence over `redis_url`.
    pub redis_cluster_nodes: Vec<String>,
    /// Whether to use Redis when a URL or cluster node is available.
    pub redis_enabled: bool,
    /// Directory backing local object storage.
    pub object_dir: PathBuf,
    /// Public base URL used to build local pre-signed URLs.
    pub public_base_url: String,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            mysql_shards: Vec::new(),
            redis_url: None,
            redis_cluster_nodes: Vec::new(),
            redis_enabled: false,
            object_dir: PathBuf::from("data/objects"),
            public_base_url: String::new(),
        }
    }
}

/// The constructed store and infrastructure ports.
#[derive(Clone)]
pub struct Backend {
    /// Repository ports.
    pub stores: Stores,
    /// Infrastructure ports.
    pub infra: Infra,
    /// The selected backend name (`memory` or `mysql`).
    pub kind: &'static str,
}

impl Backend {
    /// Connect every adapter described by the configuration.
    pub async fn connect(config: BackendConfig) -> Result<Self> {
        if !config.mysql_shards.is_empty() {
            let store = MySqlStore::connect_shards(config.mysql_shards.clone()).await?;
            let stores = store.stores();
            let infra = build_infra(&config).await?;
            Ok(Self {
                stores,
                infra,
                kind: "mysql",
            })
        } else {
            let memory = Arc::new(MemoryStore::new().with_base_url(config.public_base_url.clone()));
            let stores = Stores {
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
            };
            let infra = Infra {
                cache: memory.clone(),
                presence: memory.clone(),
                registry: memory.clone(),
                bus: memory.clone(),
                rate_limiter: memory.clone(),
                objects: memory,
            };
            Ok(Self {
                stores,
                infra,
                kind: "memory",
            })
        }
    }
}

async fn build_infra(config: &BackendConfig) -> Result<Infra> {
    let objects: Arc<dyn ObjectStorage> = Arc::new(LocalObjectStorage::new(
        config.object_dir.clone(),
        config.public_base_url.clone(),
    )?);

    if config.redis_enabled {
        if !config.redis_cluster_nodes.is_empty() {
            let redis = Arc::new(RedisInfra::connect_cluster(&config.redis_cluster_nodes).await?);
            return Ok(Infra {
                cache: redis.clone() as Arc<dyn Cache>,
                presence: redis.clone() as Arc<dyn PresenceStore>,
                registry: redis.clone() as Arc<dyn ConnectionRegistry>,
                bus: redis.clone() as Arc<dyn MessageBus>,
                rate_limiter: redis.clone() as Arc<dyn RateLimiter>,
                objects,
            });
        }
        if let Some(url) = &config.redis_url {
            let redis = Arc::new(RedisInfra::connect(url).await?);
            return Ok(Infra {
                cache: redis.clone() as Arc<dyn Cache>,
                presence: redis.clone() as Arc<dyn PresenceStore>,
                registry: redis.clone() as Arc<dyn ConnectionRegistry>,
                bus: redis.clone() as Arc<dyn MessageBus>,
                rate_limiter: redis.clone() as Arc<dyn RateLimiter>,
                objects,
            });
        }
    }

    let memory = Arc::new(MemoryStore::new());
    Ok(Infra {
        cache: memory.clone(),
        presence: memory.clone(),
        registry: memory.clone(),
        bus: memory.clone(),
        rate_limiter: memory.clone(),
        objects,
    })
}
