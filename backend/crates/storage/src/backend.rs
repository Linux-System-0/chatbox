//! Backend selection and construction.

use std::path::PathBuf;
use std::sync::Arc;

use chatbox_core::error::Result;
use chatbox_core::infra::{
    Cache, ConnectionRegistry, MessageBus, ObjectStorage, PresenceStore, RateLimiter,
};
use chatbox_core::port::Stores;
use chatbox_core::Infra;

use crate::memory::MemoryStore;
use crate::mysql::MySqlStore;
use crate::object::LocalObjectStorage;
use crate::redis::RedisInfra;

/// Configuration selecting the storage backend.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// MySQL primary URL. When `None`, the in-memory backend is used.
    pub database_url: Option<String>,
    /// Optional MySQL read replica URL.
    pub database_replica_url: Option<String>,
    /// Redis URL.
    pub redis_url: Option<String>,
    /// Whether to use Redis when a URL is available.
    pub redis_enabled: bool,
    /// Directory backing local object storage.
    pub object_dir: PathBuf,
    /// Public base URL used to build local pre-signed URLs.
    pub public_base_url: String,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            database_url: None,
            database_replica_url: None,
            redis_url: None,
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
        if let Some(url) = config.database_url.clone() {
            let store = MySqlStore::connect(&url).await?;
            let store = match &config.database_replica_url {
                Some(replica) => store.with_replica(replica).await?,
                None => store,
            };
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
