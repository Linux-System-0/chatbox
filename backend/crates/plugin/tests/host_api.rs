//! Host API tests over the in-memory backend.

use std::sync::Arc;

use chatbox_core::model::{Conversation, ConversationKind, ConversationMember, User};
use chatbox_core::port::Stores;
use chatbox_core::{Clock, SystemClock, Timestamp};
use chatbox_plugin::ServerHost;
use chatbox_storage::memory::MemoryStore;
use serde_json::{Value, json};

struct Fixture {
    host: ServerHost,
    stores: Stores,
}

fn fixture() -> Fixture {
    let memory = Arc::new(MemoryStore::new());
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
        plugin_kv: memory,
    };
    let host = ServerHost::new(stores.clone(), Arc::new(SystemClock));
    Fixture { host, stores }
}

fn call(host: &ServerHost, plugin: &str, method: &str, params: Value) -> Value {
    host.call_blocking(plugin, method, params).expect(method)
}

#[tokio::test]
async fn kv_is_namespaced_and_lists_by_prefix() {
    let f = fixture();
    call(
        &f.host,
        "alpha",
        "kv.set",
        json!({ "key": "user:1", "value": "a" }),
    );
    call(
        &f.host,
        "alpha",
        "kv.set",
        json!({ "key": "user:2", "value": "b" }),
    );
    call(
        &f.host,
        "alpha",
        "kv.set",
        json!({ "key": "other", "value": "c" }),
    );

    assert_eq!(
        call(&f.host, "alpha", "kv.get", json!({ "key": "user:1" })),
        json!("a")
    );
    // Another plugin cannot see alpha's entries.
    assert_eq!(
        call(&f.host, "beta", "kv.get", json!({ "key": "user:1" })),
        Value::Null
    );
    assert_eq!(
        call(&f.host, "alpha", "kv.list", json!({ "prefix": "user:" })),
        json!([
            { "key": "user:1", "value": "a" },
            { "key": "user:2", "value": "b" },
        ])
    );

    call(&f.host, "alpha", "kv.delete", json!({ "key": "user:1" }));
    assert_eq!(
        call(&f.host, "alpha", "kv.get", json!({ "key": "user:1" })),
        Value::Null
    );
}

#[tokio::test]
async fn kv_ttl_expires_immediately_at_zero() {
    let f = fixture();
    call(
        &f.host,
        "alpha",
        "kv.set",
        json!({ "key": "k", "value": "v", "ttl_secs": 0 }),
    );
    assert_eq!(
        call(&f.host, "alpha", "kv.get", json!({ "key": "k" })),
        Value::Null
    );
}

#[tokio::test]
async fn kv_enforces_limits() {
    let f = fixture();
    let long_key = "k".repeat(256);
    assert!(
        f.host
            .call_blocking("alpha", "kv.set", json!({ "key": long_key, "value": "v" }))
            .is_err()
    );
    let big_value = "v".repeat(64 * 1024 + 1);
    assert!(
        f.host
            .call_blocking("alpha", "kv.set", json!({ "key": "k", "value": big_value }))
            .is_err()
    );
    assert!(
        f.host
            .call_blocking(
                "alpha",
                "kv.set",
                json!({ "key": "k", "value": "v", "ttl_secs": -1 })
            )
            .is_err()
    );
}

#[tokio::test]
async fn reads_users_and_conversations() {
    let f = fixture();
    let now: Timestamp = SystemClock.now();
    f.stores
        .users
        .create(&User {
            id: 42,
            user_id: "alice".into(),
            display_name: Some("Alice".into()),
            password_hash: "hash".into(),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
    f.stores
        .conversations
        .create(&Conversation {
            id: 7,
            kind: ConversationKind::Group,
            created_at: now,
        })
        .await
        .unwrap();
    f.stores
        .conversations
        .add_member(&ConversationMember {
            conversation_id: 7,
            user_id: 42,
            last_read_seq: 0,
            muted_until: None,
            joined_at: now,
        })
        .await
        .unwrap();

    let user = call(&f.host, "alpha", "user.get", json!({ "user_id": "alice" }));
    assert_eq!(user["id"], 42);
    assert_eq!(user["display_name"], "Alice");
    // The password hash is never exposed to plugins.
    assert!(user.get("password_hash").is_none());

    let conversation = call(
        &f.host,
        "alpha",
        "conversation.get",
        json!({ "conversation_id": 7 }),
    );
    assert_eq!(conversation["kind"], "group");
    assert_eq!(conversation["members"], json!([42]));

    assert_eq!(
        call(&f.host, "alpha", "user.get", json!({ "user_id": "nobody" })),
        Value::Null
    );
    assert_eq!(
        call(
            &f.host,
            "alpha",
            "conversation.get",
            json!({ "conversation_id": 404 })
        ),
        Value::Null
    );
}

#[tokio::test]
async fn rejects_unknown_methods_and_bad_params() {
    let f = fixture();
    assert!(
        f.host
            .call_blocking("alpha", "message.send", json!({}))
            .is_err()
    );
    assert!(
        f.host
            .call_blocking("alpha", "user.get", json!({}))
            .is_err()
    );
    assert!(
        f.host
            .call_blocking("alpha", "kv.set", json!({ "key": "k" }))
            .is_err()
    );
}
