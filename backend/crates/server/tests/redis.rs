//! Redis infrastructure integration test.
//!
//! Skipped unless a Redis is configured. Two forms are accepted:
//!
//! - `TEST_REDIS_URL` — a single node.
//! - `TEST_REDIS_CLUSTER_NODES` — a comma-separated seed list for a cluster.
//!
//!   TEST_REDIS_URL=redis://127.0.0.1:6379 \
//!   cargo test -p chatbox-server --test redis
//!   TEST_REDIS_CLUSTER_NODES=redis://127.0.0.1:7000,redis://127.0.0.1:7001 \
//!   cargo test -p chatbox-server --test redis

use std::time::Duration;

use chatbox_core::infra::{
    Cache, ConnectionInfo, ConnectionRegistry, MessageBus, PresenceStore, RateLimiter,
};
use chatbox_storage::redis::RedisInfra;

const USER: i64 = 42;

#[tokio::test]
async fn redis_single_node_roundtrip() {
    let Ok(url) = std::env::var("TEST_REDIS_URL") else {
        eprintln!("TEST_REDIS_URL not set; skipping Redis test");
        return;
    };
    let redis = RedisInfra::connect(&url).await.expect("connect to Redis");
    exercise(&redis).await;
}

#[tokio::test]
async fn redis_cluster_roundtrip() {
    let Ok(raw) = std::env::var("TEST_REDIS_CLUSTER_NODES") else {
        eprintln!("TEST_REDIS_CLUSTER_NODES not set; skipping Redis cluster test");
        return;
    };
    let nodes: Vec<String> = raw
        .split(',')
        .map(|node| node.trim().to_string())
        .filter(|node| !node.is_empty())
        .collect();
    let redis = RedisInfra::connect_cluster(&nodes)
        .await
        .expect("connect to Redis cluster");
    exercise(&redis).await;
}

/// Run the same assertions against any Redis deployment.
async fn exercise(redis: &RedisInfra) {
    // Cache.
    redis
        .set("chatbox:test:key", "value", Duration::from_secs(30))
        .await
        .expect("set");
    assert_eq!(
        redis.get("chatbox:test:key").await.unwrap().as_deref(),
        Some("value")
    );
    redis.delete("chatbox:test:key").await.expect("delete");
    assert_eq!(redis.get("chatbox:test:key").await.unwrap(), None);

    // Presence.
    redis.set_online(USER, true).await.expect("online");
    assert!(redis.is_online(USER).await.expect("is online"));
    assert_eq!(
        redis
            .online_users(&[USER, USER + 1])
            .await
            .expect("online users"),
        vec![USER]
    );
    redis.set_online(USER, false).await.expect("offline");

    // Connection registry. The registry is a per-user hash so it can be served
    // by a Redis Cluster without SCAN.
    let key = format!("conn:{USER}");
    let now = chrono::Utc::now();
    let info = ConnectionInfo {
        node_id: "node-1".into(),
        device_id: 7,
        conn_id: "conn-1".into(),
        last_seen: now,
    };
    redis.register(USER, info.clone()).await.expect("register");
    let conns = redis.connections(USER).await.expect("connections");
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0].conn_id, "conn-1");

    redis
        .heartbeat(USER, "conn-1", now + chrono::Duration::seconds(1))
        .await
        .expect("heartbeat");
    redis.unregister(USER, "conn-1").await.expect("unregister");
    assert!(
        redis
            .connections(USER)
            .await
            .expect("connections")
            .is_empty()
    );
    redis.delete(&key).await.ok();

    // Rate limiter.
    let rl = "chatbox:test:rate";
    redis.delete(rl).await.ok();
    assert!(redis.check(rl, 2, Duration::from_secs(30)).await.unwrap());
    assert!(redis.check(rl, 2, Duration::from_secs(30)).await.unwrap());
    assert!(!redis.check(rl, 2, Duration::from_secs(30)).await.unwrap());
    redis.delete(rl).await.ok();

    // Pub/sub. Publishing to one node of a cluster reaches every subscriber.
    let mut sub = redis.subscribe();
    let published = tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..50 {
            redis
                .publish("test-topic", "payload")
                .await
                .expect("publish");
            if let Ok(msg) = tokio::time::timeout(Duration::from_millis(100), sub.recv()).await {
                let msg = msg.expect("bus message");
                if msg.topic == "test-topic" && msg.payload == "payload" {
                    return true;
                }
            }
        }
        false
    })
    .await
    .expect("pub/sub timed out");
    assert!(published, "published payload was not received");
}
