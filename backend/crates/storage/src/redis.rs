//! Redis-backed infrastructure adapters.
//!
//! Redis holds the cache, presence markers, the connection registry, the
//! rate-limit counters, and the fan-out transport. Every key has a TTL so a
//! crashed node's registrations expire without manual cleanup.
//!
//! Both a single node and a Redis Cluster are supported. Commands go through a
//! [`RedisConn`], which is a single-node connection manager or an async cluster
//! connection. Pub/sub always runs over a plain connection to one seed node:
//! a Redis Cluster forwards published messages to every node, so a subscriber
//! attached to any one node observes the whole cluster's traffic.

use std::time::Duration;

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use chatbox_core::infra::{
    BusMessage, Cache, ConnectionInfo, ConnectionRegistry, MessageBus, PresenceStore, RateLimiter,
};
use chatbox_core::model::Timestamp;
use chrono::Utc;
use futures::StreamExt;
use redis::aio::{ConnectionLike, ConnectionManager};
use redis::cluster::ClusterClient;
use redis::cluster_async::ClusterConnection;
use redis::{AsyncCommands, Client, Cmd, Pipeline, RedisFuture, Value};
use tokio::sync::broadcast;

const CONN_TTL_SECS: u64 = 90;
const PRESENCE_TTL_SECS: u64 = 90;
const BUS_CHANNEL_PREFIX: &str = "chatbox:";

/// A connection to either a single Redis node or a Redis Cluster.
#[derive(Clone)]
enum RedisConn {
    // Both variants are boxed so the enum stays pointer-sized.
    Single(Box<ConnectionManager>),
    Cluster(Box<ClusterConnection>),
}

impl ConnectionLike for RedisConn {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, Value> {
        match self {
            RedisConn::Single(conn) => conn.req_packed_command(cmd),
            RedisConn::Cluster(conn) => conn.req_packed_command(cmd),
        }
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a Pipeline,
        offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<Value>> {
        match self {
            RedisConn::Single(conn) => conn.req_packed_commands(cmd, offset, count),
            RedisConn::Cluster(conn) => conn.req_packed_commands(cmd, offset, count),
        }
    }

    fn get_db(&self) -> i64 {
        match self {
            RedisConn::Single(conn) => conn.get_db(),
            RedisConn::Cluster(conn) => conn.get_db(),
        }
    }
}

/// Infrastructure adapters backed by a single Redis node or a Redis Cluster.
#[derive(Clone)]
pub struct RedisInfra {
    conn: RedisConn,
    bus_tx: broadcast::Sender<BusMessage>,
}

impl RedisInfra {
    /// Connect to a single Redis node and start the pub/sub forwarding task.
    pub async fn connect(url: &str) -> Result<Self> {
        let client = Client::open(url).map_err(Error::storage)?;
        let manager = ConnectionManager::new(client)
            .await
            .map_err(Error::storage)?;
        Ok(Self::with_conn(
            RedisConn::Single(Box::new(manager)),
            url.to_string(),
        ))
    }

    /// Connect to a Redis Cluster from a list of seed-node URLs.
    pub async fn connect_cluster(nodes: &[String]) -> Result<Self> {
        if nodes.is_empty() {
            return Err(Error::storage("redis cluster requires at least one node"));
        }
        let client = ClusterClient::new(nodes.iter().cloned()).map_err(Error::storage)?;
        let conn = client
            .get_async_connection()
            .await
            .map_err(Error::storage)?;
        Ok(Self::with_conn(
            RedisConn::Cluster(Box::new(conn)),
            nodes[0].clone(),
        ))
    }

    fn with_conn(conn: RedisConn, pubsub_seed: String) -> Self {
        let (bus_tx, _) = broadcast::channel(4096);
        let tx = bus_tx.clone();
        tokio::spawn(async move {
            forward_pubsub(pubsub_seed, tx).await;
        });
        Self { conn, bus_tx }
    }

    fn conn(&self) -> RedisConn {
        self.conn.clone()
    }
}

async fn forward_pubsub(seed: String, tx: broadcast::Sender<BusMessage>) {
    loop {
        let client = match Client::open(seed.as_str()) {
            Ok(client) => client,
            Err(e) => {
                tracing::warn!(error = %e, "redis pubsub client failed; retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
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
        let key = conn_key(user_id);
        let payload = serde_json::to_string(&info).map_err(Error::storage)?;
        let _: () = conn
            .hset(&key, &info.conn_id, payload)
            .await
            .map_err(Error::storage)?;
        let _: () = conn
            .expire(&key, CONN_TTL_SECS as i64)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn unregister(&self, user_id: i64, conn_id: &str) -> Result<()> {
        let mut conn = self.conn();
        let key = conn_key(user_id);
        let _: () = conn.hdel(&key, conn_id).await.map_err(Error::storage)?;
        Ok(())
    }

    async fn heartbeat(&self, user_id: i64, conn_id: &str, at: Timestamp) -> Result<()> {
        let mut conn = self.conn();
        let key = conn_key(user_id);
        let existing: Option<String> = conn.hget(&key, conn_id).await.map_err(Error::storage)?;
        if let Some(raw) = existing
            && let Ok(mut info) = serde_json::from_str::<ConnectionInfo>(&raw)
        {
            info.last_seen = at;
            let payload = serde_json::to_string(&info).map_err(Error::storage)?;
            let _: () = conn
                .hset(&key, conn_id, payload)
                .await
                .map_err(Error::storage)?;
            let _: () = conn
                .expire(&key, CONN_TTL_SECS as i64)
                .await
                .map_err(Error::storage)?;
        }
        Ok(())
    }

    async fn connections(&self, user_id: i64) -> Result<Vec<ConnectionInfo>> {
        let mut conn = self.conn();
        let key = conn_key(user_id);
        // A per-user hash keeps every registration in one cluster slot; SCAN
        // cannot enumerate a cluster reliably. Stale fields are filtered by
        // heartbeat time, and the whole hash expires after the TTL.
        let raw: std::collections::HashMap<String, String> =
            conn.hgetall(&key).await.map_err(Error::storage)?;
        let cutoff = Utc::now() - chrono::Duration::seconds(CONN_TTL_SECS as i64);
        let mut out = Vec::new();
        for value in raw.into_values() {
            if let Ok(info) = serde_json::from_str::<ConnectionInfo>(&value)
                && info.last_seen >= cutoff
            {
                out.push(info);
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
        // Registration hashes carry a TTL refreshed by heartbeats, so stale
        // registrations expire on their own; reads also filter by last-seen.
        Ok(0)
    }
}

fn conn_key(user_id: i64) -> String {
    format!("conn:{user_id}")
}

#[async_trait]
impl MessageBus for RedisInfra {
    async fn publish(&self, topic: &str, payload: &str) -> Result<()> {
        let mut conn = self.conn();
        let channel = format!("{BUS_CHANNEL_PREFIX}{topic}");
        // `PUBLISH` is forwarded to every node of a cluster, so a subscriber on
        // any single node receives the message.
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
