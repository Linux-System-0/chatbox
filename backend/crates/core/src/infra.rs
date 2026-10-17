//! Infrastructure ports: cache, presence, connection registry, message bus,
//! and rate limiting. These are implemented by `chatbox-storage` and consumed
//! by the API and realtime gateway.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::error::Result;
use crate::model::Timestamp;

/// A live connection record in the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// The gateway node that owns the connection.
    pub node_id: String,
    /// Device bound to the connection.
    pub device_id: i64,
    /// Unique connection id.
    pub conn_id: String,
    /// Last heartbeat.
    pub last_seen: Timestamp,
}

/// Cache port (cache-aside with TTL).
#[async_trait]
pub trait Cache: Send + Sync {
    /// Read a value.
    async fn get(&self, key: &str) -> Result<Option<String>>;
    /// Write a value with a TTL.
    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<()>;
    /// Delete a value.
    async fn delete(&self, key: &str) -> Result<()>;
}

/// Presence port.
#[async_trait]
pub trait PresenceStore: Send + Sync {
    /// Mark a user online or offline.
    async fn set_online(&self, user_id: i64, online: bool) -> Result<()>;
    /// Whether a user is online.
    async fn is_online(&self, user_id: i64) -> Result<bool>;
    /// Filter the given users down to those online.
    async fn online_users(&self, users: &[i64]) -> Result<Vec<i64>>;
}

/// Connection registry port.
#[async_trait]
pub trait ConnectionRegistry: Send + Sync {
    /// Register a live connection.
    async fn register(&self, user_id: i64, info: ConnectionInfo) -> Result<()>;
    /// Remove a connection.
    async fn unregister(&self, user_id: i64, conn_id: &str) -> Result<()>;
    /// Refresh a connection's last-seen time.
    async fn heartbeat(&self, user_id: i64, conn_id: &str, at: Timestamp) -> Result<()>;
    /// List a user's live connections.
    async fn connections(&self, user_id: i64) -> Result<Vec<ConnectionInfo>>;
    /// List live connections for several users.
    async fn connections_for(&self, user_ids: &[i64]) -> Result<Vec<(i64, Vec<ConnectionInfo>)>>;
    /// Drop connections whose heartbeat is older than `before`.
    async fn prune_stale(&self, before: Timestamp) -> Result<usize>;
}

/// A message on the bus.
#[derive(Debug, Clone)]
pub struct BusMessage {
    /// Topic name.
    pub topic: String,
    /// JSON payload.
    pub payload: String,
}

/// Durable-ish publish/subscribe transport. Implementations may use Redis
/// Streams; the in-memory implementation uses a broadcast channel.
#[async_trait]
pub trait MessageBus: Send + Sync {
    /// Publish a payload to a topic.
    async fn publish(&self, topic: &str, payload: &str) -> Result<()>;
    /// Subscribe to all topics. Callers filter by `topic`.
    fn subscribe(&self) -> broadcast::Receiver<BusMessage>;
}

/// Rate limiter port.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Return `true` if the action is allowed under `limit` per `window`.
    async fn check(&self, key: &str, limit: u32, window: Duration) -> Result<bool>;
}

/// Object storage port. Files are uploaded directly by clients using
/// pre-signed URLs; the API never proxies large payloads.
#[async_trait]
pub trait ObjectStorage: Send + Sync {
    /// Store an object.
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;
    /// Read an object.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    /// Whether an object exists.
    async fn exists(&self, key: &str) -> Result<bool>;
    /// Delete an object.
    async fn delete(&self, key: &str) -> Result<()>;
    /// Produce a time-limited URL clients may PUT to.
    async fn presign_put(&self, key: &str, content_type: &str, ttl: Duration) -> Result<String>;
    /// Produce a time-limited URL clients may GET from.
    async fn presign_get(&self, key: &str, ttl: Duration) -> Result<String>;
}

/// All infrastructure ports bundled for injection.
#[derive(Clone)]
pub struct Infra {
    /// Cache.
    pub cache: Arc<dyn Cache>,
    /// Presence.
    pub presence: Arc<dyn PresenceStore>,
    /// Connection registry.
    pub registry: Arc<dyn ConnectionRegistry>,
    /// Message bus.
    pub bus: Arc<dyn MessageBus>,
    /// Rate limiter.
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// Object storage.
    pub objects: Arc<dyn ObjectStorage>,
}
