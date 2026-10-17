//! End-to-end tests for the domain services over the in-memory backend.

use std::sync::Arc;
use std::time::Duration;

use chatbox_core::infra::{ConnectionInfo, Infra};
use chatbox_core::model::{
    FriendRequestStatus, GroupRole, MessageKind, Platform, SearchQuery, SortOrder,
};
use chatbox_core::port::Stores;
use chatbox_core::services::{
    AuthContext, AuthTokens, ConversationView, LoginInput, RegisterInput, SendRequest, Services,
};
use chatbox_core::tokens::TokenConfig;
use chatbox_core::{Snowflake, SystemClock, TokenManager};
use chatbox_plugin::{HookContext, HookOutcome, MessageGuardPlugin, PluginHost};
use chatbox_storage::memory::MemoryStore;
use chatbox_workers::{Fanout, OutboxRelay};

struct Harness {
    services: Services,
    plugins: Arc<PluginHost>,
}

fn harness() -> Harness {
    let memory = Arc::new(MemoryStore::new().with_base_url("http://localhost"));
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
        plugin_kv: memory.clone(),
    };
    let infra = Infra {
        cache: memory.clone(),
        presence: memory.clone(),
        registry: memory.clone(),
        bus: memory.clone(),
        rate_limiter: memory.clone(),
        objects: memory,
    };
    let tokens = Arc::new(TokenManager::new(TokenConfig {
        secret: "test-secret".into(),
        ..Default::default()
    }));
    let services = Services::new(
        stores,
        infra,
        Arc::new(Snowflake::new(1)),
        tokens,
        Arc::new(SystemClock),
    );
    let plugins = Arc::new(PluginHost::new(services.stores.plugins.clone()));
    Harness { services, plugins }
}

async fn register(services: &Services, id: &str) -> (i64, AuthTokens, AuthContext) {
    let (user, tokens) = services
        .register(RegisterInput {
            user_id: id.into(),
            password: "password123".into(),
            display_name: None,
            device_name: "test-device".into(),
            platform: Platform::Unknown,
        })
        .await
        .expect("register");
    let ctx = services
        .authenticate(&tokens.access_token)
        .await
        .expect("authenticate");
    (user.id, tokens, ctx)
}

async fn send(
    services: &Services,
    ctx: &AuthContext,
    conv: i64,
    key: &str,
    text: &str,
) -> chatbox_core::model::Message {
    services
        .send_message(
            ctx,
            SendRequest {
                conversation_id: conv,
                client_msg_id: key.into(),
                kind: MessageKind::Text,
                content: Some(text.into()),
                attachment_id: None,
            },
        )
        .await
        .expect("send")
}

#[tokio::test]
async fn registration_login_and_duplicate_id() {
    let h = harness();
    let (_, _, _) = register(&h.services, "alice").await;
    // Duplicate custom id is rejected.
    let err = h
        .services
        .register(RegisterInput {
            user_id: "alice".into(),
            password: "password123".into(),
            display_name: None,
            device_name: "d".into(),
            platform: Platform::Web,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, chatbox_core::Error::Conflict(_)));

    // Login with the correct and incorrect password.
    let (user, _) = h
        .services
        .login(LoginInput {
            user_id: "alice".into(),
            password: "password123".into(),
            device_name: "web".into(),
            platform: Platform::Web,
        })
        .await
        .expect("login");
    assert_eq!(user.user_id, "alice");
    assert!(
        h.services
            .login(LoginInput {
                user_id: "alice".into(),
                password: "wrong-password".into(),
                device_name: "web".into(),
                platform: Platform::Web,
            })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn direct_messaging_is_idempotent_and_searchable() {
    let h = harness();
    let (alice_id, _, alice) = register(&h.services, "alice").await;
    let (bob_id, _, bob) = register(&h.services, "bob").await;

    let conv = h
        .services
        .get_or_create_direct(&alice, "bob")
        .await
        .expect("direct");
    // Same pair returns the same conversation.
    assert_eq!(
        h.services
            .get_or_create_direct(&bob, "alice")
            .await
            .unwrap()
            .id,
        conv.id
    );

    let first = send(&h.services, &alice, conv.id, "m1", "hello bob").await;
    assert_eq!(first.seq, 1);
    // Idempotent retry.
    let retry = send(&h.services, &alice, conv.id, "m1", "hello bob").await;
    assert_eq!(retry.id, first.id);
    assert_eq!(h.services.stores.messages.count(conv.id).await.unwrap(), 1);

    let second = send(&h.services, &bob, conv.id, "m2", "hi alice").await;
    assert_eq!(second.seq, 2);

    // Bob reads and lists.
    h.services
        .mark_read(&bob, conv.id, second.seq)
        .await
        .unwrap();
    let history = h
        .services
        .list_messages(&bob, conv.id, None, None, Some(10))
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].sender_id, alice_id);
    assert_eq!(history[1].sender_id, bob_id);

    // Content search.
    let results = h
        .services
        .search(
            &alice,
            &SearchQuery {
                viewer_id: 0,
                conversation_id: Some(conv.id),
                content: Some("hello".into()),
                sort: SortOrder::Asc,
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, first.id);

    // Conversation summary reports unread for Alice.
    let conversations: Vec<ConversationView> = h.services.list_conversations(&alice).await.unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].unread, 1);
}

#[tokio::test]
async fn block_prevents_direct_messages() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;
    let conv = h
        .services
        .get_or_create_direct(&alice, "bob")
        .await
        .unwrap();

    h.services.block_user(&bob, "alice").await.unwrap();
    let err = h
        .services
        .send_message(
            &alice,
            SendRequest {
                conversation_id: conv.id,
                client_msg_id: "m1".into(),
                kind: MessageKind::Text,
                content: Some("hi".into()),
                attachment_id: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, chatbox_core::Error::Forbidden(_)));

    h.services.unblock_user(&bob, "alice").await.unwrap();
    assert!(
        h.services
            .send_message(
                &alice,
                SendRequest {
                    conversation_id: conv.id,
                    client_msg_id: "m2".into(),
                    kind: MessageKind::Text,
                    content: Some("hi".into()),
                    attachment_id: None,
                },
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn friend_request_flow_creates_contact() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;

    let request = h
        .services
        .send_friend_request(&alice, "bob", Some("hi".into()))
        .await
        .unwrap();
    assert_eq!(request.status, FriendRequestStatus::Pending);
    // Duplicate request rejected.
    assert!(
        h.services
            .send_friend_request(&alice, "bob", None)
            .await
            .is_err()
    );

    let accepted = h
        .services
        .accept_friend_request(&bob, request.id)
        .await
        .unwrap();
    assert_eq!(accepted.status, FriendRequestStatus::Accepted);

    let contacts = h.services.list_contacts(&alice).await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].user.user_id, "bob");

    h.services
        .set_contact_remark(&alice, "bob", Some("Bobby".into()))
        .await
        .unwrap();
    let contacts = h.services.list_contacts(&alice).await.unwrap();
    assert_eq!(contacts[0].remark.as_deref(), Some("Bobby"));

    h.services.remove_contact(&alice, "bob").await.unwrap();
    assert!(h.services.list_contacts(&alice).await.unwrap().is_empty());
}

#[tokio::test]
async fn group_roles_and_lifecycle() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;
    register(&h.services, "carol").await;

    let group = h
        .services
        .create_group(&alice, "team", &["bob".into(), "carol".into()])
        .await
        .unwrap();
    assert_eq!(group.group.owner_id, group.members[0].user.id);
    assert_eq!(group.members.len(), 3);

    // A member cannot rename.
    assert!(
        h.services
            .rename_group(&bob, group.group.id, "hacked")
            .await
            .is_err()
    );

    // Rename and mute by the owner.
    h.services
        .rename_group(&alice, group.group.id, "team-renamed")
        .await
        .unwrap();
    let until = h.services.now() + chrono::Duration::minutes(5);
    h.services
        .mute_group_member(&alice, group.group.id, "bob", Some(until))
        .await
        .unwrap();

    // Muted member cannot send.
    assert!(
        h.services
            .send_message(
                &bob,
                SendRequest {
                    conversation_id: group.group.conversation_id,
                    client_msg_id: "g1".into(),
                    kind: MessageKind::Text,
                    content: Some("hello".into()),
                    attachment_id: None,
                },
            )
            .await
            .is_err()
    );

    // Mentions are resolved to member ids.
    h.services
        .mute_group_member(&alice, group.group.id, "bob", None)
        .await
        .unwrap();
    let message = send(
        &h.services,
        &alice,
        group.group.conversation_id,
        "g2",
        "hi @bob",
    )
    .await;
    assert_eq!(message.mentions.len(), 1);

    // Transfer and dissolve.
    h.services
        .transfer_ownership(&alice, group.group.id, "bob")
        .await
        .unwrap();
    let group = h.services.get_group(&bob, group.group.id).await.unwrap();
    assert_eq!(
        group
            .members
            .iter()
            .find(|m| m.role == GroupRole::Owner)
            .unwrap()
            .user
            .user_id,
        "bob"
    );
    h.services
        .dissolve_group(&bob, group.group.id)
        .await
        .unwrap();
    assert!(h.services.get_group(&alice, group.group.id).await.is_err());
}

#[tokio::test]
async fn offline_messages_are_delivered_on_resume() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;
    let conv = h
        .services
        .get_or_create_direct(&alice, "bob")
        .await
        .unwrap();

    // Bob is offline; Alice sends three messages.
    for i in 1..=3 {
        send(
            &h.services,
            &alice,
            conv.id,
            &format!("m{i}"),
            &format!("msg {i}"),
        )
        .await;
    }

    let mut cursors = std::collections::HashMap::new();
    cursors.insert(conv.id, 0u64);
    let caught_up = h.services.resume(&bob, &cursors).await.unwrap();
    assert_eq!(caught_up.len(), 3);

    // Catching up again from the last seq returns nothing new.
    cursors.insert(conv.id, 3u64);
    assert!(h.services.resume(&bob, &cursors).await.unwrap().is_empty());
}

#[tokio::test]
async fn fanout_routes_to_registered_connections() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (bob_id, _, _) = register(&h.services, "bob").await;
    let conv = h
        .services
        .get_or_create_direct(&alice, "bob")
        .await
        .unwrap();

    h.services
        .infra
        .registry
        .register(
            bob_id,
            ConnectionInfo {
                node_id: "node-1".into(),
                device_id: 1,
                conn_id: "conn-1".into(),
                last_seen: h.services.now(),
            },
        )
        .await
        .unwrap();

    let mut rx = h.services.infra.bus.subscribe();
    send(&h.services, &alice, conv.id, "m1", "ping").await;

    let fanout = Fanout::new(
        h.services.stores.clone(),
        h.services.infra.clone(),
        "node-1",
        256,
    );
    let relay = OutboxRelay::new(h.services.stores.clone(), fanout);
    relay.tick().await.unwrap();

    let message = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out")
        .expect("bus closed");
    let delivery: chatbox_core::protocol::Delivery =
        serde_json::from_str(&message.payload).unwrap();
    assert_eq!(delivery.user_id, bob_id);
    assert_eq!(delivery.conn_id, "conn-1");
    assert!(matches!(
        delivery.frame,
        chatbox_core::protocol::ServerFrame::Message { .. }
    ));
}

#[tokio::test]
async fn plugin_guard_rejects_and_toggles() {
    let h = harness();
    h.plugins
        .register(Arc::new(MessageGuardPlugin::new(vec!["forbidden".into()])))
        .await;
    h.plugins.bootstrap().await.unwrap();
    h.plugins.register_builtin_client_manifests().await;

    // Disabled plugins have no effect.
    let ctx = HookContext::new(
        "message.before_persist",
        serde_json::json!({ "content": "this is forbidden" }),
    );
    assert_eq!(
        h.plugins.on_message_before_persist(&ctx).await.unwrap(),
        HookOutcome::Allow
    );

    // Enabling persists and activates the hook.
    h.plugins.enable("message-guard").await.unwrap();
    assert_eq!(
        h.plugins.on_message_before_persist(&ctx).await.unwrap(),
        HookOutcome::Reject("message contains 'forbidden'".into())
    );

    // Client plugin manifests are served (informational only).
    let client = h.plugins.client_manifests().await;
    assert!(client.iter().any(|m| m.name == "zh-CN"));

    // The enabled set is persisted and reloads.
    let persisted = h
        .services
        .stores
        .plugins
        .is_enabled("message-guard")
        .await
        .unwrap();
    assert!(persisted);
}
