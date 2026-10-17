//! Redis-backed infrastructure adapters.
//!
//! Redis holds the cache, presence markers, the connection registry, the
//! rate-limit counters, and the fan-out transport. Every key has a TTL so a
//! crashed node's registrations expire without manual cleanup.

use std::time::Duration;

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use chatbox_core::infra::{
    BusMessage, Cache, ConnectionInfo, ConnectionRegistry, MessageBus, PresenceStore, RateLimiter,
};
use chatbox_core::model::Timestamp;
use futures::StreamExt;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use tokio::sync::broadcast;

const CONN_TTL_SECS: u64 = 90;
const PRESENCE_TTL_SECS: u64 = 90;
const BUS_CHANNEL_PREFIX: &str = "chatbox:";

/// Infrastructure adapters backed by a single Redis deployment.
#[derive(Clone)]
pub struct RedisInfra {
    manager: ConnectionManager,
    bus_tx: broadcast::Sender<BusMessage>,
}

impl RedisInfra {
    /// Connect and start the pub/sub forwarding task.
    pub async fn connect(url: &str) -> Result<Self> {
        let client = Client::open(url).map_err(Error::storage)?;
        let manager = ConnectionManager::new(client.clone())
            .await
            .map_err(Error::storage)?;
        let (bus_tx, _) = broadcast::channel(4096);
        let tx = bus_tx.clone();
        tokio::spawn(async move {
            forward_pubsub(client, tx).await;
        });
        Ok(Self { manager, bus_tx })
    }

    fn conn(&self) -> ConnectionManager {
        self.manager.clone()
    }

    fn conn_key(user_id: i64, conn_id: &str) -> String {
        format!("conn:{user_id}:{conn_id}")
    }
}

async fn forward_pubsub(client: Client, tx: broadcast::Sender<BusMessage>) {
    loop {
        let mut pubsub = match client.get_async_pubsub().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "redis pubsub connect failed; retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        if let Err(e) = pubsub.psubscribe(format!("{BUS_CHANNEL_PREFIX}*")).await {
            tracing::warn!(error = %e, "redis psubscribe failed; retrying");
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let channel = msg.get_channel_name();
            // Strip the transport prefix so subscribers see the logical topic
            // (`delivery`, `presence`, ...) consistently with the in-memory bus.
            let topic = channel
                .strip_prefix(BUS_CHANNEL_PREFIX)
                .unwrap_or(channel)
                .to_string();
            let payload: String = msg.get_payload().unwrap_or_default();
            let _ = tx.send(BusMessage { topic, payload });
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[async_trait]
impl Cache for RedisInfra {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.conn();
        let value: Option<String> = conn.get(key).await.map_err(Error::storage)?;
        Ok(value)
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<()> {
        let mut conn = self.conn();
        let _: () = conn
            .set_ex(key, value, ttl.as_secs().max(1))
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.conn();
        let _: () = conn.del(key).await.map_err(Error::storage)?;
        Ok(())
    }
}

#[async_trait]
impl PresenceStore for RedisInfra {
    async fn set_online(&self, user_id: i64, online: bool) -> Result<()> {
        let mut conn = self.conn();
        let key = format!("presence:{user_id}");
        if online {
            let _: () = conn
                .set_ex(&key, "1", PRESENCE_TTL_SECS)
                .await
                .map_err(Error::storage)?;
        } else {
            let _: () = conn.del(&key).await.map_err(Error::storage)?;
        }
        Ok(())
    }

    async fn is_online(&self, user_id: i64) -> Result<bool> {
        let mut conn = self.conn();
        let key = format!("presence:{user_id}");
        let exists: bool = conn.exists(&key).await.map_err(Error::storage)?;
        Ok(exists)
    }

    async fn online_users(&self, users: &[i64]) -> Result<Vec<i64>> {
        let mut conn = self.conn();
        let mut online = Vec::new();
        for user in users {
            let key = format!("presence:{user}");
            let exists: bool = conn.exists(&key).await.map_err(Error::storage)?;
            if exists {
                online.push(*user);
            }
        }
        Ok(online)
    }
}

#[async_trait]
impl ConnectionRegistry for RedisInfra {
    async fn register(&self, user_id: i64, info: ConnectionInfo) -> Result<()> {
        let mut conn = self.conn();
        let key = Self::conn_key(user_id, &info.conn_id);
        let payload = serde_json::to_string(&info).map_err(Error::storage)?;
        let _: () = conn
            .set_ex(&key, payload, CONN_TTL_SECS)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn unregister(&self, user_id: i64, conn_id: &str) -> Result<()> {
        let mut conn = self.conn();
        let key = Self::conn_key(user_id, conn_id);
        let _: () = conn.del(&key).await.map_err(Error::storage)?;
        Ok(())
    }

    async fn heartbeat(&self, user_id: i64, conn_id: &str, at: Timestamp) -> Result<()> {
        let mut conn = self.conn();
        let key = Self::conn_key(user_id, conn_id);
        if let Some(mut info) = self
            .connections(user_id)
            .await?
            .into_iter()
            .find(|c| c.conn_id == conn_id)
        {
            info.last_seen = at;
            let payload = serde_json::to_string(&info).map_err(Error::storage)?;
            let _: () = conn
                .set_ex(&key, payload, CONN_TTL_SECS)
                .await
                .map_err(Error::storage)?;
        }
        Ok(())
    }

    async fn connections(&self, user_id: i64) -> Result<Vec<ConnectionInfo>> {
        let mut conn = self.conn();
        let pattern = format!("conn:{user_id}:*");
        let keys: Vec<String> = {
            let mut iter: redis::AsyncIter<String> =
                conn.scan_match(pattern).await.map_err(Error::storage)?;
            let mut keys = Vec::new();
            while let Some(key) = iter.next_item().await {
                keys.push(key);
            }
            keys
        };
        let mut out = Vec::new();
        for key in keys {
            let value: Option<String> = conn.get(&key).await.map_err(Error::storage)?;
            if let Some(value) = value {
                if let Ok(info) = serde_json::from_str::<ConnectionInfo>(&value) {
                    out.push(info);
                }
            }
        }
        Ok(out)
    }

    async fn connections_for(&self, user_ids: &[i64]) -> Result<Vec<(i64, Vec<ConnectionInfo>)>> {
        let mut out = Vec::with_capacity(user_ids.len());
        for user in user_ids {
            out.push((*user, self.connections(*user).await?));
        }
        Ok(out)
    }

    async fn prune_stale(&self, _before: Timestamp) -> Result<usize> {
        // Connection keys carry a TTL refreshed by heartbeats, so stale
        // registrations expire on their own.
        Ok(0)
    }
}

#[async_trait]
impl MessageBus for RedisInfra {
    async fn publish(&self, topic: &str, payload: &str) -> Result<()> {
        let mut conn = self.conn();
        let channel = format!("{BUS_CHANNEL_PREFIX}{topic}");
        let _: () = conn
            .publish(channel, payload)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<BusMessage> {
        self.bus_tx.subscribe()
    }
}

#[async_trait]
impl RateLimiter for RedisInfra {
    async fn check(&self, key: &str, limit: u32, window: Duration) -> Result<bool> {
        let mut conn = self.conn();
        let count: i64 = conn.incr(key, 1).await.map_err(Error::storage)?;
        if count == 1 {
            let _: () = conn
                .expire(key, window.as_secs().max(1) as i64)
                .await
                .map_err(Error::storage)?;
        }
        Ok(count <= limit as i64)
    }
}
