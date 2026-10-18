//! End-to-end tests for the domain services over the in-memory backend.

use std::sync::Arc;
use std::time::Duration;

use chatbox_core::error::Error;
use chatbox_core::infra::{ConnectionInfo, Infra};
use chatbox_core::model::{
    FriendRequestStatus, GroupRole, MessageKind, Platform, SearchQuery, SortOrder,
};
use chatbox_core::port::Stores;
use chatbox_core::protocol::{Delivery, ServerFrame};
use chatbox_core::services::{
    AuthContext, AuthTokens, ConversationView, LoginInput, RegisterInput, SendRequest,
    ServiceConfig, Services,
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
    harness_with_config(ServiceConfig::default())
}

fn harness_with_config(config: ServiceConfig) -> Harness {
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
    )
    .with_config(config);
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

async fn befriend(services: &Services, from: &AuthContext, to_name: &str, to: &AuthContext) {
    let request = services
        .send_friend_request(from, to_name, None)
        .await
        .expect("friend request");
    services
        .accept_friend_request(to, request.id)
        .await
        .expect("accept friend request");
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
    befriend(&h.services, &alice, "bob", &bob).await;

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

    // Conversation summary reports unread for Alice and Bob's read cursor.
    let conversations: Vec<ConversationView> = h.services.list_conversations(&alice).await.unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].unread, 1);
    let bob_cursor = conversations[0]
        .read_cursors
        .iter()
        .find(|cursor| cursor.user_id == bob_id)
        .expect("bob read cursor");
    assert_eq!(bob_cursor.last_read_seq, second.seq);
}

#[tokio::test]
async fn reading_a_message_publishes_a_read_receipt() {
    let h = harness();
    let (alice_id, _, alice) = register(&h.services, "alice").await;
    let (bob_id, _, bob) = register(&h.services, "bob").await;
    befriend(&h.services, &alice, "bob", &bob).await;
    let conv = h
        .services
        .get_or_create_direct(&alice, "bob")
        .await
        .unwrap();
    let message = send(&h.services, &alice, conv.id, "m1", "hello bob").await;

    h.services
        .infra
        .registry
        .register(
            alice_id,
            ConnectionInfo {
                node_id: "node-1".into(),
                device_id: 1,
                conn_id: "alice-conn".into(),
                last_seen: h.services.now(),
            },
        )
        .await
        .unwrap();
    let mut bus = h.services.infra.bus.subscribe();
    h.services
        .mark_read(&bob, conv.id, message.seq)
        .await
        .unwrap();

    let mut receipt = None;
    while let Ok(event) = bus.try_recv() {
        let delivery: Delivery = serde_json::from_str(&event.payload).unwrap();
        if let ServerFrame::Read {
            conversation_id,
            user_id,
            seq,
        } = delivery.frame
        {
            receipt = Some((conversation_id, user_id, seq));
            break;
        }
    }
    assert_eq!(receipt, Some((conv.id, bob_id, message.seq)));
}

#[tokio::test]
async fn block_prevents_direct_messages() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;
    befriend(&h.services, &alice, "bob", &bob).await;
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
        .set_contact_remark(&alice, "bob", Some(Some("Bobby".into())), None)
        .await
        .unwrap();
    let contacts = h.services.list_contacts(&alice).await.unwrap();
    assert_eq!(contacts[0].remark.as_deref(), Some("Bobby"));

    h.services
        .set_contact_remark(
            &alice,
            "bob",
            None,
            Some(Some("data:image/png;base64,AAAA".into())),
        )
        .await
        .unwrap();
    let contacts = h.services.list_contacts(&alice).await.unwrap();
    assert_eq!(
        contacts[0].remark_avatar_url.as_deref(),
        Some("data:image/png;base64,AAAA")
    );

    // Blocking hides the contact from the contact list but keeps the block.
    h.services.block_user(&alice, "bob").await.unwrap();
    assert!(h.services.list_contacts(&alice).await.unwrap().is_empty());
    assert_eq!(
        h.services.list_blocked_users(&alice).await.unwrap().len(),
        1
    );
    h.services.unblock_user(&alice, "bob").await.unwrap();
    assert_eq!(h.services.list_contacts(&alice).await.unwrap().len(), 1);

    h.services.remove_contact(&alice, "bob").await.unwrap();
    assert!(h.services.list_contacts(&alice).await.unwrap().is_empty());
}

#[tokio::test]
async fn direct_chat_requires_an_accepted_contact() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;

    let err = h
        .services
        .get_or_create_direct(&alice, "bob")
        .await
        .unwrap_err();
    assert!(matches!(err, chatbox_core::Error::Forbidden(_)));

    befriend(&h.services, &alice, "bob", &bob).await;
    assert!(h.services.get_or_create_direct(&alice, "bob").await.is_ok());
}

#[tokio::test]
async fn profile_can_change_id_display_name_and_avatar() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;

    let updated = h
        .services
        .update_profile(
            &alice,
            Some("alice-new".into()),
            Some(Some("Alice A.".into())),
            Some(Some("data:image/png;base64,AAAA".into())),
        )
        .await
        .unwrap();
    assert_eq!(updated.user_id, "alice-new");
    assert_eq!(updated.display_name.as_deref(), Some("Alice A."));
    assert_eq!(
        updated.avatar_url.as_deref(),
        Some("data:image/png;base64,AAAA")
    );
    assert!(h.services.get_user_by_user_id("alice").await.is_err());
    assert!(h.services.get_user_by_user_id("alice-new").await.is_ok());

    // Taking another user's id is rejected.
    let err = h
        .services
        .update_profile(&alice, Some("bob".into()), None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, chatbox_core::Error::Conflict(_)));

    // Empty values clear the optional profile fields.
    let cleared = h
        .services
        .update_profile(&alice, None, Some(Some("".into())), Some(Some("".into())))
        .await
        .unwrap();
    assert!(cleared.display_name.is_none());
    assert!(cleared.avatar_url.is_none());
    let _ = bob;
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

    // The owner can promote and demote administrators.
    let promoted = h
        .services
        .set_group_member_role(&alice, group.group.id, "bob", GroupRole::Admin)
        .await
        .unwrap();
    assert!(
        promoted
            .members
            .iter()
            .any(|m| m.user.user_id == "bob" && m.role == GroupRole::Admin)
    );
    h.services
        .rename_group(&bob, group.group.id, "team-by-admin")
        .await
        .unwrap();
    // Only the owner may change roles.
    assert!(
        h.services
            .set_group_member_role(&bob, group.group.id, "carol", GroupRole::Admin)
            .await
            .is_err()
    );
    h.services
        .set_group_member_role(&alice, group.group.id, "bob", GroupRole::Member)
        .await
        .unwrap();
    assert!(
        h.services
            .rename_group(&bob, group.group.id, "nope")
            .await
            .is_err()
    );

    // A group remark overrides the group name only for its owner.
    h.services
        .set_group_remark(&bob, group.group.id, Some("bob's team".into()))
        .await
        .unwrap();
    let bob_conversations = h.services.list_conversations(&bob).await.unwrap();
    let bob_group = bob_conversations
        .iter()
        .find(|conversation| conversation.id == group.group.conversation_id)
        .unwrap();
    assert_eq!(bob_group.title, "bob's team");
    let alice_conversations = h.services.list_conversations(&alice).await.unwrap();
    let alice_group = alice_conversations
        .iter()
        .find(|conversation| conversation.id == group.group.conversation_id)
        .unwrap();
    assert_eq!(alice_group.title, "team-by-admin");
    // Another member's private group remark is not exposed.
    let alice_view = h.services.get_group(&alice, group.group.id).await.unwrap();
    assert!(
        alice_view
            .members
            .iter()
            .find(|member| member.user.user_id == "bob")
            .unwrap()
            .remark
            .is_none()
    );
    h.services
        .set_group_remark(&bob, group.group.id, None)
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
    befriend(&h.services, &alice, "bob", &bob).await;
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
    let (bob_id, _, bob) = register(&h.services, "bob").await;
    befriend(&h.services, &alice, "bob", &bob).await;
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
async fn group_changes_push_sync_frames() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (bob_id, _, _bob) = register(&h.services, "bob").await;
    let group = h
        .services
        .create_group(&alice, "team", &["bob".into()])
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
    h.services
        .set_group_member_role(&alice, group.group.id, "bob", GroupRole::Admin)
        .await
        .unwrap();

    let message = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out")
        .expect("bus closed");
    let delivery: chatbox_core::protocol::Delivery =
        serde_json::from_str(&message.payload).unwrap();
    assert_eq!(delivery.user_id, bob_id);
    assert!(matches!(
        delivery.frame,
        chatbox_core::protocol::ServerFrame::Sync { .. }
    ));
}

#[tokio::test]
async fn file_upload_round_trip() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;
    let (_, _, bob) = register(&h.services, "bob").await;
    befriend(&h.services, &alice, "bob", &bob).await;
    let conv = h
        .services
        .get_or_create_direct(&alice, "bob")
        .await
        .unwrap();

    let upload = h
        .services
        .presign_attachment(&alice, "report.pdf", "application/pdf", 4)
        .await
        .unwrap();
    h.services
        .store_upload(&alice, upload.attachment_id, b"data")
        .await
        .unwrap();
    let attachment = h
        .services
        .complete_attachment(
            &alice,
            upload.attachment_id,
            "report.pdf",
            "application/pdf",
        )
        .await
        .unwrap();
    assert_eq!(attachment.filename, "report.pdf");
    assert_eq!(attachment.size, 4);

    let message = h
        .services
        .send_message(
            &alice,
            SendRequest {
                conversation_id: conv.id,
                client_msg_id: "file-1".into(),
                kind: MessageKind::File,
                content: Some("report.pdf".into()),
                attachment_id: Some(attachment.id),
            },
        )
        .await
        .unwrap();
    assert_eq!(message.attachment_id, Some(attachment.id));

    let (fetched, url) = h
        .services
        .get_attachment(&bob, attachment.id)
        .await
        .unwrap();
    assert_eq!(fetched.sha256, attachment.sha256);
    assert!(url.contains(&attachment.id.to_string()));
}

#[tokio::test]
async fn upload_limit_is_unlimited_by_default() {
    let h = harness();
    let (_, _, alice) = register(&h.services, "alice").await;

    // Presign accepts any size while no limit is configured.
    h.services
        .presign_attachment(
            &alice,
            "huge.bin",
            "application/octet-stream",
            600 * 1024 * 1024 * 1024,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn upload_limit_rejects_oversized_files() {
    let h = harness_with_config(ServiceConfig {
        max_upload_bytes: Some(1024),
        ..Default::default()
    });
    let (_, _, alice) = register(&h.services, "alice").await;

    let error = h
        .services
        .presign_attachment(&alice, "big.bin", "application/octet-stream", 1025)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::PayloadTooLarge(_)));

    let upload = h
        .services
        .presign_attachment(&alice, "ok.bin", "application/octet-stream", 1024)
        .await
        .unwrap();
    h.services
        .store_upload(&alice, upload.attachment_id, &[0u8; 1024])
        .await
        .unwrap();

    let error = h
        .services
        .store_upload(&alice, upload.attachment_id, &[0u8; 1025])
        .await
        .unwrap_err();
    assert!(matches!(error, Error::PayloadTooLarge(_)));
}

#[tokio::test]
async fn upload_limit_zero_disables_uploads() {
    let h = harness_with_config(ServiceConfig {
        max_upload_bytes: Some(0),
        ..Default::default()
    });
    let (_, _, alice) = register(&h.services, "alice").await;

    let error = h
        .services
        .presign_attachment(&alice, "small.bin", "application/octet-stream", 1)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Forbidden(_)));

    let error = h
        .services
        .presign_attachment(&alice, "empty.bin", "application/octet-stream", 0)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Forbidden(_)));

    let error = h.services.store_upload(&alice, 1, b"").await.unwrap_err();
    assert!(matches!(error, Error::Forbidden(_)));
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
