//! MySQL-backed integration test.
//!
//! Skipped unless a database is configured. Two forms are accepted:
//!
//! - `TEST_DATABASE_URL` — a single shard.
//! - `TEST_MYSQL_SHARDS` — one or more shards, `;`-separated, each a `|`-
//!   separated primary/replica list, matching `MYSQL_SHARDS`.
//!
//! The ChatBox schema is applied to every shard's primary (idempotent
//! `CREATE TABLE IF NOT EXISTS`), so a fresh database is enough.
//!
//!   TEST_MYSQL_SHARDS='mysql://chatbox:chatbox@127.0.0.1:3306/chatbox0;mysql://chatbox:chatbox@127.0.0.1:3306/chatbox1' \
//!     cargo test -p chatbox-server --test mysql

use chatbox_core::model::{
    Attachment, Conversation, ConversationKind, ConversationMember, Message, MessageKind,
    OutboxEvent, SearchQuery, Session, SortOrder, User,
};
use chatbox_core::port::{AttachmentRepo, ConversationRepo, MessageRepo, SessionRepo, UserRepo};
use chatbox_core::{IdGenerator, Snowflake};
use chatbox_storage::ShardConfig;
use chatbox_storage::mysql::MySqlStore;

const MIGRATION: &str = include_str!("../../../../deploy/sql/migrations/0001_init.sql");

/// Parse the shard configuration accepted by the test.
fn shard_config() -> Option<Vec<ShardConfig>> {
    if let Ok(raw) = std::env::var("TEST_MYSQL_SHARDS")
        && !raw.trim().is_empty()
    {
        let mut shards = Vec::new();
        for spec in raw.split(';') {
            let hosts: Vec<String> = spec
                .split('|')
                .map(|host| host.trim().to_string())
                .filter(|host| !host.is_empty())
                .collect();
            if let Some((primary, replicas)) = hosts.split_first() {
                shards.push(ShardConfig {
                    primary: primary.clone(),
                    replicas: replicas.to_vec(),
                });
            }
        }
        if !shards.is_empty() {
            return Some(shards);
        }
    }
    std::env::var("TEST_DATABASE_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .map(|url| vec![ShardConfig::primary(url)])
}

#[tokio::test]
async fn mysql_roundtrip_sequence_and_fanout() {
    let Some(shards) = shard_config() else {
        eprintln!("TEST_DATABASE_URL/TEST_MYSQL_SHARDS not set; skipping MySQL test");
        return;
    };
    let store = MySqlStore::connect_shards(shards)
        .await
        .expect("connect to MySQL");
    assert_eq!(store.shard_count(), store.pools().len());
    for pool in store.pools() {
        sqlx::raw_sql(MIGRATION)
            .execute(pool)
            .await
            .expect("apply schema");
    }

    let ids = Snowflake::new(1);
    let now = chrono::Utc::now();

    // --- A conversation and its sequence live on one shard. -----------------
    let conversation_id = ids.next_id();
    let sender_id = ids.next_id();
    ConversationRepo::create(
        &store,
        &Conversation {
            id: conversation_id,
            kind: ConversationKind::Direct,
            created_at: now,
        },
    )
    .await
    .expect("create conversation");
    ConversationRepo::add_member(
        &store,
        &ConversationMember {
            conversation_id,
            user_id: sender_id,
            last_read_seq: 0,
            muted_until: None,
            joined_at: now,
        },
    )
    .await
    .expect("add member");

    // Regression: the sequence starts at 1 and increments atomically, and the
    // BIGINT UNSIGNED column decodes back into u64.
    let first = MessageRepo::next_seq(&store, conversation_id)
        .await
        .expect("first seq");
    let second = MessageRepo::next_seq(&store, conversation_id)
        .await
        .expect("second seq");
    assert_eq!((first, second), (1, 2));

    let message = Message {
        id: ids.next_id(),
        conversation_id,
        seq: first,
        sender_id,
        client_msg_id: format!("mysql-{conversation_id}"),
        kind: MessageKind::Text,
        content: Some("hello from MySQL".into()),
        attachment_id: None,
        mentions: vec![sender_id],
        created_at: now,
        edited_at: None,
        deleted_at: None,
    };
    let outbox = OutboxEvent {
        id: ids.next_id(),
        message_id: message.id,
        conversation_id,
        payload: serde_json::to_string(&message).unwrap(),
        published_at: None,
        created_at: now,
    };
    MessageRepo::append(&store, &message, &outbox)
        .await
        .expect("append message");

    let stored = MessageRepo::get(&store, message.id)
        .await
        .expect("get message")
        .expect("message exists");
    assert_eq!(stored.seq, 1);
    assert_eq!(stored.mentions, vec![sender_id]);

    let history = MessageRepo::list(&store, conversation_id, None, None, 10)
        .await
        .expect("list messages");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content.as_deref(), Some("hello from MySQL"));

    ConversationRepo::set_read_cursor(&store, conversation_id, sender_id, 1)
        .await
        .expect("set read cursor");
    let member = ConversationRepo::get_member(&store, conversation_id, sender_id)
        .await
        .expect("get member")
        .expect("member exists");
    assert_eq!(member.last_read_seq, 1);

    // --- Users are found by public id even when sharded by internal id. -----
    let user_id = format!("mysqluser{}", ids.next_id());
    let user = User {
        id: ids.next_id(),
        user_id: user_id.clone(),
        display_name: None,
        avatar_url: None,
        password_hash: "not-a-real-hash".into(),
        created_at: now,
        updated_at: now,
    };
    UserRepo::create(&store, &user).await.expect("create user");
    assert!(UserRepo::exists_user_id(&store, &user_id).await.unwrap());
    let found = UserRepo::get_by_user_id(&store, &user_id)
        .await
        .expect("get by public id")
        .expect("user exists");
    assert_eq!(found.id, user.id);

    // --- Fan-out search across shards and cross-shard attachment join. ------
    let mut conversation_ids = vec![conversation_id];
    let mut attachment_ids = Vec::new();
    // A second conversation is placed on the next shard when several exist,
    // because consecutive Snowflake ids hash to different shards.
    for _ in 0..2 {
        let cid = ids.next_id();
        let uid = ids.next_id();
        ConversationRepo::create(
            &store,
            &Conversation {
                id: cid,
                kind: ConversationKind::Direct,
                created_at: now,
            },
        )
        .await
        .expect("create conversation");
        ConversationRepo::add_member(
            &store,
            &ConversationMember {
                conversation_id: cid,
                user_id: uid,
                last_read_seq: 0,
                muted_until: None,
                joined_at: now,
            },
        )
        .await
        .expect("add member");

        let attachment = Attachment {
            id: ids.next_id(),
            owner_id: uid,
            filename: format!("clip-{cid}.mp4"),
            size: 42,
            mime: "video/mp4".into(),
            sha256: "0".repeat(64),
            storage_key: format!("objects/{cid}"),
            created_at: now,
        };
        AttachmentRepo::create(&store, &attachment)
            .await
            .expect("create attachment");
        attachment_ids.push(attachment.id);

        let seq = MessageRepo::next_seq(&store, cid).await.expect("next seq");
        let file_message = Message {
            id: ids.next_id(),
            conversation_id: cid,
            seq,
            sender_id: uid,
            client_msg_id: format!("file-{cid}"),
            kind: MessageKind::File,
            content: None,
            attachment_id: Some(attachment.id),
            mentions: Vec::new(),
            created_at: now,
            edited_at: None,
            deleted_at: None,
        };
        let event = OutboxEvent {
            id: ids.next_id(),
            message_id: file_message.id,
            conversation_id: cid,
            payload: serde_json::to_string(&file_message).unwrap(),
            published_at: None,
            created_at: now,
        };
        MessageRepo::append(&store, &file_message, &event)
            .await
            .expect("append file message");
        conversation_ids.push(cid);
    }

    let viewer = sender_id;
    // Every created conversation belongs to a distinct user; add `viewer` as a
    // member so the search membership join matches.
    for cid in &conversation_ids {
        ConversationRepo::add_member(
            &store,
            &ConversationMember {
                conversation_id: *cid,
                user_id: viewer,
                last_read_seq: 0,
                muted_until: None,
                joined_at: now,
            },
        )
        .await
        .expect("add viewer member");
    }

    let query = SearchQuery {
        viewer_id: viewer,
        kind: Some(MessageKind::File),
        filename: Some("clip-*.mp4".into()),
        sort: SortOrder::Asc,
        limit: 100,
        ..Default::default()
    };
    let results = MessageRepo::search(&store, &query)
        .await
        .expect("search by filename");
    let expected = attachment_ids.len() as u32;
    assert!(
        results.len() as u32 >= expected,
        "expected at least {expected} file messages, got {}",
        results.len()
    );

    // --- Sessions route by id and revoke by device across shards. -----------
    let session_id = format!("session-{}", ids.next_id());
    let session = Session {
        id: session_id.clone(),
        user_id: user.id,
        device_id: ids.next_id(),
        refresh_hash: "hash".into(),
        issued_at: now,
        expires_at: now + chrono::Duration::hours(1),
        revoked_at: None,
    };
    SessionRepo::create(&store, &session)
        .await
        .expect("create session");
    let loaded = SessionRepo::get(&store, &session_id)
        .await
        .expect("get session")
        .expect("session exists");
    assert_eq!(loaded.id, session_id);
    SessionRepo::revoke_for_device(&store, session.device_id)
        .await
        .expect("revoke session");
    let revoked = SessionRepo::get(&store, &session_id)
        .await
        .expect("get session")
        .expect("session exists");
    assert!(revoked.revoked_at.is_some());
}
