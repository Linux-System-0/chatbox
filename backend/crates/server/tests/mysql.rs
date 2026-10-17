//! MySQL-backed integration test.
//!
//! Skipped unless `TEST_DATABASE_URL` points at a running MySQL with the
//! ChatBox schema available; the test applies the migration itself (idempotent
//! `CREATE TABLE IF NOT EXISTS`) so a fresh database is enough.
//!
//!   TEST_DATABASE_URL=mysql://chatbox:chatbox@127.0.0.1:3306/chatbox \
//!     cargo test -p chatbox-server --test mysql

use chatbox_core::model::{
    Conversation, ConversationKind, ConversationMember, Message, MessageKind, OutboxEvent,
};
use chatbox_core::port::{ConversationRepo, MessageRepo, UserRepo};
use chatbox_core::{IdGenerator, Snowflake};
use chatbox_storage::mysql::MySqlStore;

const MIGRATION: &str = include_str!("../../../../deploy/sql/migrations/0001_init.sql");

#[tokio::test]
async fn mysql_roundtrip_and_sequence() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL not set; skipping MySQL integration test");
        return;
    };
    let store = MySqlStore::connect(&url).await.expect("connect to MySQL");
    sqlx::raw_sql(MIGRATION)
        .execute(store.pool())
        .await
        .expect("apply schema");

    let ids = Snowflake::new(1);
    let now = chrono::Utc::now();
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

    // Regression: the sequence must start at 1 and increment atomically, and
    // the BIGINT UNSIGNED column must decode back into u64.
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

    // Read cursor round-trips through the unsigned column.
    ConversationRepo::set_read_cursor(&store, conversation_id, sender_id, 1)
        .await
        .expect("set read cursor");
    let member = ConversationRepo::get_member(&store, conversation_id, sender_id)
        .await
        .expect("get member")
        .expect("member exists");
    assert_eq!(member.last_read_seq, 1);

    // Register a user to cover the unique-id path.
    let user_id = format!("mysqluser{}", ids.next_id());
    let user = chatbox_core::model::User {
        id: ids.next_id(),
        user_id: user_id.clone(),
        display_name: None,
        password_hash: "not-a-real-hash".into(),
        created_at: now,
        updated_at: now,
    };
    UserRepo::create(&store, &user).await.expect("create user");
    assert!(UserRepo::exists_user_id(&store, &user_id).await.unwrap());
}
