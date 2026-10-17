//! MySQL repository adapters.
//!
//! Queries are runtime-checked (see ADR 0002) and the schema lives in
//! `deploy/sql/migrations`. Rows are mapped by column name to keep the crate
//! free of compile-time database checking.
//!
//! Every configured shard carries the full schema. Rows are routed by a stable
//! shard key; a single logical row therefore always lives on one shard. Lookups
//! and list queries that do not carry their routing key fan out across every
//! shard and merge the results. See `docs/adr/0006-mysql-sharding.md`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use chatbox_core::model::*;
use chatbox_core::port::*;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::{MySql, MySqlPool, QueryBuilder, Row};

use crate::backend::ShardConfig;

/// One physical MySQL shard: a primary plus zero or more read replicas.
///
/// Reads are spread across the replica pool in round-robin order; with no
/// replica configured the primary serves reads too. Writes always target the
/// primary.
pub struct MySqlShard {
    pool: MySqlPool,
    replicas: Vec<MySqlPool>,
    replica_cursor: AtomicUsize,
}

impl MySqlShard {
    async fn connect(primary_url: &str, replica_urls: &[String]) -> Result<Self> {
        let mut replicas = Vec::with_capacity(replica_urls.len());
        for url in replica_urls {
            replicas.push(connect_pool(url).await?);
        }
        Ok(Self {
            pool: connect_pool(primary_url).await?,
            replicas,
            replica_cursor: AtomicUsize::new(0),
        })
    }

    /// The primary pool (writes and read-your-writes).
    pub fn pool(&self) -> &MySqlPool {
        &self.pool
    }

    /// Round-robin across replicas, falling back to the primary.
    fn read_pool(&self) -> &MySqlPool {
        if self.replicas.is_empty() {
            return &self.pool;
        }
        let index = self.replica_cursor.fetch_add(1, Ordering::Relaxed) % self.replicas.len();
        &self.replicas[index]
    }
}

async fn connect_pool(url: &str) -> Result<MySqlPool> {
    MySqlPoolOptions::new()
        .max_connections(16)
        .connect(url)
        .await
        .map_err(Error::storage)
}

/// Sharded MySQL store sharing one or more shard pools.
///
/// The store owns every shard and routes each operation to the shard that owns
/// the row. Operations without a routing key fan out and merge.
#[derive(Clone)]
pub struct MySqlStore {
    shards: Arc<Vec<MySqlShard>>,
}

impl MySqlStore {
    /// Connect a single-shard store.
    pub async fn connect(primary_url: &str) -> Result<Self> {
        Self::connect_shards(vec![ShardConfig::primary(primary_url)]).await
    }

    /// Connect every configured shard, opening each primary and its replicas.
    pub async fn connect_shards(shards: Vec<ShardConfig>) -> Result<Self> {
        if shards.is_empty() {
            return Err(Error::storage("at least one MySQL shard is required"));
        }
        let mut built = Vec::with_capacity(shards.len());
        for shard in &shards {
            built.push(MySqlShard::connect(&shard.primary, &shard.replicas).await?);
        }
        Ok(Self {
            shards: Arc::new(built),
        })
    }

    /// Number of configured shards.
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// The first shard's primary pool. Retained for tooling and tests.
    pub fn pool(&self) -> &MySqlPool {
        self.shards[0].pool()
    }

    /// The primary pool of every shard, in configuration order.
    pub fn pools(&self) -> Vec<&MySqlPool> {
        self.shards.iter().map(MySqlShard::pool).collect()
    }

    /// Route by an integer key (Snowflake id, conversation id, ...).
    fn shard_for(&self, key: i64) -> &MySqlShard {
        // Mixing is essential: Snowflake ids keep their low bits zero for the
        // first id of every millisecond, so `id % shards` would hot-spot on
        // shard 0. The murmur-style finalizer spreads every bit.
        self.shard_for_hash(mix64(key as u64))
    }

    /// Route by a stable string key (session id, request id, ...).
    fn shard_for_str(&self, key: &str) -> &MySqlShard {
        self.shard_for_hash(fnv1a(key))
    }

    fn shard_for_hash(&self, hash: u64) -> &MySqlShard {
        &self.shards[shard_index(hash, self.shards.len())]
    }

    /// Bundle this store into the [`Stores`] aggregate.
    pub fn stores(self) -> Stores {
        let store = std::sync::Arc::new(self);
        Stores {
            users: store.clone(),
            sessions: store.clone(),
            devices: store.clone(),
            conversations: store.clone(),
            messages: store.clone(),
            contacts: store.clone(),
            friend_requests: store.clone(),
            groups: store.clone(),
            blocks: store.clone(),
            attachments: store.clone(),
            outbox: store.clone(),
            plugins: store.clone(),
            plugin_kv: store,
        }
    }
}

/// FNV-1a hash used to map string shard keys onto a shard.
fn fnv1a(value: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A murmur-style 64-bit finalizer. Every input bit affects every output bit,
/// which keeps sequential or low-bit-sparse ids spread across shards.
fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Map a hash onto `[0, shard_count)`.
fn shard_index(hash: u64, shard_count: usize) -> usize {
    (hash % shard_count as u64) as usize
}

fn ts(row: &MySqlRow, col: &str) -> Result<Timestamp> {
    let value: NaiveDateTime = row.try_get(col).map_err(Error::storage)?;
    Ok(DateTime::from_naive_utc_and_offset(value, Utc))
}

fn opt_ts(row: &MySqlRow, col: &str) -> Result<Option<Timestamp>> {
    let value: Option<NaiveDateTime> = row.try_get(col).map_err(Error::storage)?;
    Ok(value.map(|v| DateTime::from_naive_utc_and_offset(v, Utc)))
}

fn naive(value: Timestamp) -> NaiveDateTime {
    value.naive_utc()
}

fn opt_naive(value: Option<Timestamp>) -> Option<NaiveDateTime> {
    value.map(naive)
}

fn seq(row: &MySqlRow, col: &str) -> Result<u64> {
    // Sequence and size columns are declared BIGINT UNSIGNED, which SQLx only
    // decodes into u64.
    row.try_get(col).map_err(Error::storage)
}

fn bool_of(row: &MySqlRow, col: &str) -> Result<bool> {
    let value: i8 = row.try_get(col).map_err(Error::storage)?;
    Ok(value != 0)
}

fn conversation_kind(value: &str) -> Result<ConversationKind> {
    match value {
        "direct" => Ok(ConversationKind::Direct),
        "group" => Ok(ConversationKind::Group),
        other => Err(Error::storage(format!("unknown conversation kind {other}"))),
    }
}

fn message_kind(value: &str) -> Result<MessageKind> {
    match value {
        "text" => Ok(MessageKind::Text),
        "file" => Ok(MessageKind::File),
        "system" => Ok(MessageKind::System),
        other => Err(Error::storage(format!("unknown message kind {other}"))),
    }
}

fn group_role(value: &str) -> Result<GroupRole> {
    match value {
        "owner" => Ok(GroupRole::Owner),
        "admin" => Ok(GroupRole::Admin),
        "member" => Ok(GroupRole::Member),
        other => Err(Error::storage(format!("unknown group role {other}"))),
    }
}

fn platform(value: &str) -> Result<Platform> {
    match value {
        "web" => Ok(Platform::Web),
        "android" => Ok(Platform::Android),
        _ => Ok(Platform::Unknown),
    }
}

fn request_status(value: &str) -> Result<FriendRequestStatus> {
    match value {
        "pending" => Ok(FriendRequestStatus::Pending),
        "accepted" => Ok(FriendRequestStatus::Accepted),
        "rejected" => Ok(FriendRequestStatus::Rejected),
        other => Err(Error::storage(format!("unknown request status {other}"))),
    }
}

fn user_from_row(row: &MySqlRow) -> Result<User> {
    Ok(User {
        id: row.try_get("id").map_err(Error::storage)?,
        user_id: row.try_get("user_id").map_err(Error::storage)?,
        display_name: row.try_get("display_name").map_err(Error::storage)?,
        avatar_url: row.try_get("avatar_url").map_err(Error::storage)?,
        password_hash: row.try_get("password_hash").map_err(Error::storage)?,
        created_at: ts(row, "created_at")?,
        updated_at: ts(row, "updated_at")?,
    })
}

fn device_from_row(row: &MySqlRow) -> Result<Device> {
    let platform_raw: String = row.try_get("platform").map_err(Error::storage)?;
    Ok(Device {
        id: row.try_get("id").map_err(Error::storage)?,
        user_id: row.try_get("user_id").map_err(Error::storage)?,
        name: row.try_get("name").map_err(Error::storage)?,
        platform: platform(&platform_raw)?,
        last_seen_at: ts(row, "last_seen_at")?,
        created_at: ts(row, "created_at")?,
    })
}

fn session_from_row(row: &MySqlRow) -> Result<Session> {
    Ok(Session {
        id: row.try_get("id").map_err(Error::storage)?,
        user_id: row.try_get("user_id").map_err(Error::storage)?,
        device_id: row.try_get("device_id").map_err(Error::storage)?,
        refresh_hash: row.try_get("refresh_hash").map_err(Error::storage)?,
        issued_at: ts(row, "issued_at")?,
        expires_at: ts(row, "expires_at")?,
        revoked_at: opt_ts(row, "revoked_at")?,
    })
}

fn conversation_from_row(row: &MySqlRow) -> Result<Conversation> {
    let kind_raw: String = row.try_get("kind").map_err(Error::storage)?;
    Ok(Conversation {
        id: row.try_get("id").map_err(Error::storage)?,
        kind: conversation_kind(&kind_raw)?,
        created_at: ts(row, "created_at")?,
    })
}

fn member_from_row(row: &MySqlRow) -> Result<ConversationMember> {
    Ok(ConversationMember {
        conversation_id: row.try_get("conversation_id").map_err(Error::storage)?,
        user_id: row.try_get("user_id").map_err(Error::storage)?,
        last_read_seq: seq(row, "last_read_seq")?,
        muted_until: opt_ts(row, "muted_until")?,
        joined_at: ts(row, "joined_at")?,
    })
}

fn message_from_row(row: &MySqlRow) -> Result<Message> {
    let kind_raw: String = row.try_get("kind").map_err(Error::storage)?;
    let mentions_raw: Option<serde_json::Value> = row.try_get("mentions").ok();
    let mentions = mentions_raw
        .and_then(|value| serde_json::from_value::<Vec<i64>>(value).ok())
        .unwrap_or_default();
    Ok(Message {
        id: row.try_get("id").map_err(Error::storage)?,
        conversation_id: row.try_get("conversation_id").map_err(Error::storage)?,
        seq: seq(row, "seq")?,
        sender_id: row.try_get("sender_id").map_err(Error::storage)?,
        client_msg_id: row.try_get("client_msg_id").map_err(Error::storage)?,
        kind: message_kind(&kind_raw)?,
        content: row.try_get("content").map_err(Error::storage)?,
        attachment_id: row.try_get("attachment_id").map_err(Error::storage)?,
        mentions,
        created_at: ts(row, "created_at")?,
        edited_at: opt_ts(row, "edited_at")?,
        deleted_at: opt_ts(row, "deleted_at")?,
    })
}

fn contact_from_row(row: &MySqlRow) -> Result<Contact> {
    Ok(Contact {
        owner_id: row.try_get("owner_id").map_err(Error::storage)?,
        contact_id: row.try_get("contact_id").map_err(Error::storage)?,
        remark: row.try_get("remark").map_err(Error::storage)?,
        remark_avatar_url: row.try_get("remark_avatar_url").map_err(Error::storage)?,
        created_at: ts(row, "created_at")?,
    })
}

fn friend_request_from_row(row: &MySqlRow) -> Result<FriendRequest> {
    let status_raw: String = row.try_get("status").map_err(Error::storage)?;
    Ok(FriendRequest {
        id: row.try_get("id").map_err(Error::storage)?,
        from_id: row.try_get("from_id").map_err(Error::storage)?,
        to_id: row.try_get("to_id").map_err(Error::storage)?,
        message: row.try_get("message").map_err(Error::storage)?,
        status: request_status(&status_raw)?,
        created_at: ts(row, "created_at")?,
        responded_at: opt_ts(row, "responded_at")?,
    })
}

fn group_from_row(row: &MySqlRow) -> Result<Group> {
    let policy_raw: String = row.try_get("invite_policy").map_err(Error::storage)?;
    Ok(Group {
        id: row.try_get("id").map_err(Error::storage)?,
        conversation_id: row.try_get("conversation_id").map_err(Error::storage)?,
        name: row.try_get("name").map_err(Error::storage)?,
        owner_id: row.try_get("owner_id").map_err(Error::storage)?,
        invite_policy: group_role(&policy_raw)?,
        created_at: ts(row, "created_at")?,
    })
}

fn group_member_from_row(row: &MySqlRow) -> Result<GroupMember> {
    let role_raw: String = row.try_get("role").map_err(Error::storage)?;
    Ok(GroupMember {
        group_id: row.try_get("group_id").map_err(Error::storage)?,
        user_id: row.try_get("user_id").map_err(Error::storage)?,
        role: group_role(&role_raw)?,
        muted_until: opt_ts(row, "muted_until")?,
        joined_at: ts(row, "joined_at")?,
    })
}

fn attachment_from_row(row: &MySqlRow) -> Result<Attachment> {
    Ok(Attachment {
        id: row.try_get("id").map_err(Error::storage)?,
        owner_id: row.try_get("owner_id").map_err(Error::storage)?,
        filename: row.try_get("filename").map_err(Error::storage)?,
        size: seq(row, "size")?,
        mime: row.try_get("mime").map_err(Error::storage)?,
        sha256: row.try_get("sha256").map_err(Error::storage)?,
        storage_key: row.try_get("storage_key").map_err(Error::storage)?,
        created_at: ts(row, "created_at")?,
    })
}

fn outbox_from_row(row: &MySqlRow) -> Result<OutboxEvent> {
    Ok(OutboxEvent {
        id: row.try_get("id").map_err(Error::storage)?,
        message_id: row.try_get("message_id").map_err(Error::storage)?,
        conversation_id: row.try_get("conversation_id").map_err(Error::storage)?,
        payload: row.try_get("payload").map_err(Error::storage)?,
        published_at: opt_ts(row, "published_at")?,
        created_at: ts(row, "created_at")?,
    })
}

#[async_trait]
impl UserRepo for MySqlShard {
    async fn create(&self, user: &User) -> Result<()> {
        sqlx::query(
            "INSERT INTO users (id, user_id, display_name, avatar_url, password_hash, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user.id)
        .bind(&user.user_id)
        .bind(&user.display_name)
        .bind(&user.avatar_url)
        .bind(&user.password_hash)
        .bind(naive(user.created_at))
        .bind(naive(user.updated_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<User>> {
        let row = sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(user_from_row).transpose()
    }

    async fn get_by_user_id(&self, user_id: &str) -> Result<Option<User>> {
        let row = sqlx::query("SELECT * FROM users WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(user_from_row).transpose()
    }

    async fn update(&self, user: &User) -> Result<()> {
        sqlx::query(
            "UPDATE users SET user_id = ?, display_name = ?, avatar_url = ?, password_hash = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&user.user_id)
        .bind(&user.display_name)
        .bind(&user.avatar_url)
        .bind(&user.password_hash)
        .bind(naive(user.updated_at))
        .bind(user.id)
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn exists_user_id(&self, user_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(self.read_pool())
            .await
            .map_err(Error::storage)?;
        Ok(count > 0)
    }
}

#[async_trait]
impl SessionRepo for MySqlShard {
    async fn create(&self, session: &Session) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (id, user_id, device_id, refresh_hash, issued_at, expires_at, revoked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(session.user_id)
        .bind(session.device_id)
        .bind(&session.refresh_hash)
        .bind(naive(session.issued_at))
        .bind(naive(session.expires_at))
        .bind(opt_naive(session.revoked_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query("SELECT * FROM sessions WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(session_from_row).transpose()
    }

    async fn update(&self, session: &Session) -> Result<()> {
        sqlx::query("UPDATE sessions SET refresh_hash = ?, revoked_at = ? WHERE id = ?")
            .bind(&session.refresh_hash)
            .bind(opt_naive(session.revoked_at))
            .bind(&session.id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Session>> {
        let rows = sqlx::query("SELECT * FROM sessions WHERE user_id = ? ORDER BY issued_at DESC")
            .bind(user_id)
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter().map(session_from_row).collect()
    }

    async fn revoke_for_device(&self, device_id: i64) -> Result<()> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = NOW(6) WHERE device_id = ? AND revoked_at IS NULL",
        )
        .bind(device_id)
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }
}

#[async_trait]
impl DeviceRepo for MySqlShard {
    async fn create(&self, device: &Device) -> Result<()> {
        sqlx::query(
            "INSERT INTO devices (id, user_id, name, platform, last_seen_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(device.id)
        .bind(device.user_id)
        .bind(&device.name)
        .bind(platform_str(device.platform))
        .bind(naive(device.last_seen_at))
        .bind(naive(device.created_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Device>> {
        let row = sqlx::query("SELECT * FROM devices WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(device_from_row).transpose()
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Device>> {
        let rows = sqlx::query("SELECT * FROM devices WHERE user_id = ? ORDER BY created_at DESC")
            .bind(user_id)
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter().map(device_from_row).collect()
    }

    async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM devices WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn touch(&self, id: i64, at: Timestamp) -> Result<()> {
        sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
            .bind(naive(at))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }
}

fn platform_str(platform: Platform) -> &'static str {
    match platform {
        Platform::Web => "web",
        Platform::Android => "android",
        Platform::Unknown => "unknown",
    }
}

#[async_trait]
impl ConversationRepo for MySqlShard {
    async fn create(&self, conversation: &Conversation) -> Result<()> {
        sqlx::query("INSERT INTO conversations (id, kind, created_at) VALUES (?, ?, ?)")
            .bind(conversation.id)
            .bind(match conversation.kind {
                ConversationKind::Direct => "direct",
                ConversationKind::Group => "group",
            })
            .bind(naive(conversation.created_at))
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Conversation>> {
        let row = sqlx::query("SELECT * FROM conversations WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(conversation_from_row).transpose()
    }

    async fn find_direct(&self, a: i64, b: i64) -> Result<Option<Conversation>> {
        let row = sqlx::query(
            "SELECT c.* FROM conversations c
             JOIN conversation_members m1 ON m1.conversation_id = c.id AND m1.user_id = ?
             JOIN conversation_members m2 ON m2.conversation_id = c.id AND m2.user_id = ?
             WHERE c.kind = 'direct' LIMIT 1",
        )
        .bind(a)
        .bind(b)
        .fetch_optional(self.read_pool())
        .await
        .map_err(Error::storage)?;
        row.as_ref().map(conversation_from_row).transpose()
    }

    async fn add_member(&self, member: &ConversationMember) -> Result<()> {
        sqlx::query(
            "INSERT INTO conversation_members
             (conversation_id, user_id, last_read_seq, muted_until, joined_at)
             VALUES (?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE joined_at = VALUES(joined_at)",
        )
        .bind(member.conversation_id)
        .bind(member.user_id)
        .bind(member.last_read_seq)
        .bind(opt_naive(member.muted_until))
        .bind(naive(member.joined_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get_member(
        &self,
        conversation_id: i64,
        user_id: i64,
    ) -> Result<Option<ConversationMember>> {
        let row = sqlx::query(
            "SELECT * FROM conversation_members WHERE conversation_id = ? AND user_id = ?",
        )
        .bind(conversation_id)
        .bind(user_id)
        .fetch_optional(self.read_pool())
        .await
        .map_err(Error::storage)?;
        row.as_ref().map(member_from_row).transpose()
    }

    async fn list_members(&self, conversation_id: i64) -> Result<Vec<ConversationMember>> {
        let rows = sqlx::query("SELECT * FROM conversation_members WHERE conversation_id = ?")
            .bind(conversation_id)
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter().map(member_from_row).collect()
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<ConversationMember>> {
        let rows = sqlx::query("SELECT * FROM conversation_members WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter().map(member_from_row).collect()
    }

    async fn set_read_cursor(&self, conversation_id: i64, user_id: i64, seq: u64) -> Result<()> {
        sqlx::query(
            "UPDATE conversation_members SET last_read_seq = ?
             WHERE conversation_id = ? AND user_id = ? AND last_read_seq < ?",
        )
        .bind(seq)
        .bind(conversation_id)
        .bind(user_id)
        .bind(seq)
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn remove_member(&self, conversation_id: i64, user_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM conversation_members WHERE conversation_id = ? AND user_id = ?")
            .bind(conversation_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn set_mute(
        &self,
        conversation_id: i64,
        user_id: i64,
        until: Option<Timestamp>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE conversation_members SET muted_until = ? WHERE conversation_id = ? AND user_id = ?",
        )
        .bind(opt_naive(until))
        .bind(conversation_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }
}

#[async_trait]
impl MessageRepo for MySqlShard {
    async fn append(&self, message: &Message, outbox: &OutboxEvent) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::storage)?;
        sqlx::query(
            "INSERT INTO messages
             (id, conversation_id, seq, sender_id, client_msg_id, kind, content, attachment_id, mentions, created_at, edited_at, deleted_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(message.id)
        .bind(message.conversation_id)
        .bind(message.seq)
        .bind(message.sender_id)
        .bind(&message.client_msg_id)
        .bind(message_kind_str(message.kind))
        .bind(&message.content)
        .bind(message.attachment_id)
        .bind(serde_json::Value::Array(
            message.mentions.iter().map(|m| serde_json::json!(m)).collect(),
        ))
        .bind(naive(message.created_at))
        .bind(opt_naive(message.edited_at))
        .bind(opt_naive(message.deleted_at))
        .execute(&mut *tx)
        .await
        .map_err(Error::storage)?;
        sqlx::query(
            "INSERT INTO outbox (id, message_id, conversation_id, payload, published_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(outbox.id)
        .bind(outbox.message_id)
        .bind(outbox.conversation_id)
        .bind(&outbox.payload)
        .bind(opt_naive(outbox.published_at))
        .bind(naive(outbox.created_at))
        .execute(&mut *tx)
        .await
        .map_err(Error::storage)?;
        tx.commit().await.map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Message>> {
        let row = sqlx::query("SELECT * FROM messages WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(message_from_row).transpose()
    }

    async fn find_by_client_id(
        &self,
        sender_id: i64,
        client_msg_id: &str,
    ) -> Result<Option<Message>> {
        let row = sqlx::query("SELECT * FROM messages WHERE sender_id = ? AND client_msg_id = ?")
            .bind(sender_id)
            .bind(client_msg_id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(message_from_row).transpose()
    }

    async fn list(
        &self,
        conversation_id: i64,
        before: Option<u64>,
        after: Option<u64>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        let rows = if let Some(before) = before {
            // Take the `limit` newest messages strictly before the cursor, then
            // return them in ascending order.
            sqlx::query(
                "SELECT * FROM (
                    SELECT * FROM messages WHERE conversation_id = ? AND seq < ?
                    ORDER BY seq DESC LIMIT ?
                 ) t ORDER BY seq ASC",
            )
            .bind(conversation_id)
            .bind(before)
            .bind(limit as i64)
            .fetch_all(self.read_pool())
            .await
        } else {
            sqlx::query(
                "SELECT * FROM messages WHERE conversation_id = ? AND seq > ?
                 ORDER BY seq ASC LIMIT ?",
            )
            .bind(conversation_id)
            .bind(after.unwrap_or(0))
            .bind(limit as i64)
            .fetch_all(self.read_pool())
            .await
        }
        .map_err(Error::storage)?;
        rows.iter().map(message_from_row).collect()
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<Message>> {
        // Filename matching is resolved by the dispatcher before the query is
        // sent to a shard, because attachments and messages may live on
        // different shards. Direct callers get the no-index behavior.
        self.search_messages(query, None).await
    }

    async fn count(&self, conversation_id: i64) -> Result<u64> {
        let value: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE conversation_id = ?")
                .bind(conversation_id)
                .fetch_one(self.read_pool())
                .await
                .map_err(Error::storage)?;
        Ok(value as u64)
    }

    async fn count_after(&self, conversation_id: i64, after: u64) -> Result<u64> {
        let value: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE conversation_id = ? AND seq > ?",
        )
        .bind(conversation_id)
        .bind(after)
        .fetch_one(self.read_pool())
        .await
        .map_err(Error::storage)?;
        Ok(value as u64)
    }

    async fn next_seq(&self, conversation_id: i64) -> Result<u64> {
        // `LAST_INSERT_ID()` is scoped to a single MySQL connection, so the
        // increment and the read must share the same acquired connection.
        let mut conn = self.pool.acquire().await.map_err(Error::storage)?;
        sqlx::query(
            "INSERT INTO conversation_sequences (conversation_id, seq)
             VALUES (?, LAST_INSERT_ID(1))
             ON DUPLICATE KEY UPDATE seq = LAST_INSERT_ID(seq + 1)",
        )
        .bind(conversation_id)
        .execute(&mut *conn)
        .await
        .map_err(Error::storage)?;
        let value: u64 = sqlx::query_scalar("SELECT LAST_INSERT_ID()")
            .fetch_one(&mut *conn)
            .await
            .map_err(Error::storage)?;
        Ok(value)
    }
}

fn message_kind_str(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Text => "text",
        MessageKind::File => "file",
        MessageKind::System => "system",
    }
}

/// Shard-local helpers that are not part of a repository port.
impl MySqlShard {
    /// Run a message search on this shard.
    ///
    /// When `attachment_ids` is `Some`, the query filters by those ids instead
    /// of joining the (possibly remote) attachments table; an empty list
    /// therefore matches nothing. Content and time filters stay in SQL.
    async fn search_messages(
        &self,
        query: &SearchQuery,
        attachment_ids: Option<&[i64]>,
    ) -> Result<Vec<Message>> {
        let mut qb = QueryBuilder::<MySql>::new(
            "SELECT m.* FROM messages m
             JOIN conversation_members cm
               ON cm.conversation_id = m.conversation_id AND cm.user_id = ",
        );
        qb.push_bind(query.viewer_id);
        qb.push(" WHERE 1 = 1");
        if let Some(conversation_id) = query.conversation_id {
            qb.push(" AND m.conversation_id = ")
                .push_bind(conversation_id);
        }
        if let Some(kind) = query.kind {
            qb.push(" AND m.kind = ").push_bind(message_kind_str(kind));
        }
        if let Some(content) = &query.content {
            qb.push(" AND LOWER(m.content) LIKE ")
                .push_bind(format!("%{}%", content.to_lowercase()));
        }
        if let Some(ids) = attachment_ids {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            qb.push(" AND m.attachment_id IN (");
            let mut separated = qb.separated(", ");
            for id in ids {
                separated.push_bind(*id);
            }
            separated.push_unseparated(")");
        }
        if let Some(from) = query.from {
            qb.push(" AND m.created_at >= ").push_bind(naive(from));
        }
        if let Some(to) = query.to {
            qb.push(" AND m.created_at <= ").push_bind(naive(to));
        }
        match query.sort {
            SortOrder::Asc => qb.push(" ORDER BY m.seq ASC"),
            SortOrder::Desc => qb.push(" ORDER BY m.seq DESC"),
        };
        qb.push(" LIMIT ").push_bind(query.limit as i64);
        let rows = qb
            .build()
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter().map(message_from_row).collect()
    }

    /// Attachment ids on this shard whose filename matches a SQL pattern.
    async fn find_attachment_ids(&self, pattern: &str) -> Result<Vec<i64>> {
        let rows = sqlx::query_scalar::<_, i64>("SELECT id FROM attachments WHERE filename LIKE ?")
            .bind(pattern)
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        Ok(rows)
    }
}

#[async_trait]
impl ContactRepo for MySqlShard {
    async fn upsert(&self, contact: &Contact) -> Result<()> {
        sqlx::query(
            "INSERT INTO contacts (owner_id, contact_id, remark, remark_avatar_url, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE remark = VALUES(remark),
                remark_avatar_url = VALUES(remark_avatar_url)",
        )
        .bind(contact.owner_id)
        .bind(contact.contact_id)
        .bind(&contact.remark)
        .bind(&contact.remark_avatar_url)
        .bind(naive(contact.created_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, owner_id: i64, contact_id: i64) -> Result<Option<Contact>> {
        let row = sqlx::query("SELECT * FROM contacts WHERE owner_id = ? AND contact_id = ?")
            .bind(owner_id)
            .bind(contact_id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(contact_from_row).transpose()
    }

    async fn list(&self, owner_id: i64) -> Result<Vec<Contact>> {
        let rows =
            sqlx::query("SELECT * FROM contacts WHERE owner_id = ? ORDER BY created_at DESC")
                .bind(owner_id)
                .fetch_all(self.read_pool())
                .await
                .map_err(Error::storage)?;
        rows.iter().map(contact_from_row).collect()
    }

    async fn set_remark(
        &self,
        owner_id: i64,
        contact_id: i64,
        remark: Option<String>,
        remark_avatar_url: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE contacts SET remark = ?, remark_avatar_url = ?
             WHERE owner_id = ? AND contact_id = ?",
        )
        .bind(remark)
        .bind(remark_avatar_url)
        .bind(owner_id)
        .bind(contact_id)
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn delete(&self, owner_id: i64, contact_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM contacts WHERE owner_id = ? AND contact_id = ?")
            .bind(owner_id)
            .bind(contact_id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }
}

#[async_trait]
impl FriendRequestRepo for MySqlShard {
    async fn create(&self, request: &FriendRequest) -> Result<()> {
        sqlx::query(
            "INSERT INTO friend_requests
             (id, from_id, to_id, message, status, created_at, responded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(request.id)
        .bind(request.from_id)
        .bind(request.to_id)
        .bind(&request.message)
        .bind(request_status_str(request.status))
        .bind(naive(request.created_at))
        .bind(opt_naive(request.responded_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<FriendRequest>> {
        let row = sqlx::query("SELECT * FROM friend_requests WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(friend_request_from_row).transpose()
    }

    async fn find_pending(&self, a: i64, b: i64) -> Result<Option<FriendRequest>> {
        let row = sqlx::query(
            "SELECT * FROM friend_requests
             WHERE status = 'pending'
               AND ((from_id = ? AND to_id = ?) OR (from_id = ? AND to_id = ?))
             LIMIT 1",
        )
        .bind(a)
        .bind(b)
        .bind(b)
        .bind(a)
        .fetch_optional(self.read_pool())
        .await
        .map_err(Error::storage)?;
        row.as_ref().map(friend_request_from_row).transpose()
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<FriendRequest>> {
        let rows = sqlx::query(
            "SELECT * FROM friend_requests WHERE from_id = ? OR to_id = ? ORDER BY created_at DESC",
        )
        .bind(user_id)
        .bind(user_id)
        .fetch_all(self.read_pool())
        .await
        .map_err(Error::storage)?;
        rows.iter().map(friend_request_from_row).collect()
    }

    async fn update(&self, request: &FriendRequest) -> Result<()> {
        sqlx::query("UPDATE friend_requests SET status = ?, responded_at = ? WHERE id = ?")
            .bind(request_status_str(request.status))
            .bind(opt_naive(request.responded_at))
            .bind(request.id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }
}

fn request_status_str(status: FriendRequestStatus) -> &'static str {
    match status {
        FriendRequestStatus::Pending => "pending",
        FriendRequestStatus::Accepted => "accepted",
        FriendRequestStatus::Rejected => "rejected",
    }
}

#[async_trait]
impl GroupRepo for MySqlShard {
    async fn create(&self, group: &Group) -> Result<()> {
        sqlx::query(
            "INSERT INTO chat_groups
             (id, conversation_id, name, owner_id, invite_policy, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(group.id)
        .bind(group.conversation_id)
        .bind(&group.name)
        .bind(group.owner_id)
        .bind(group_role_str(group.invite_policy))
        .bind(naive(group.created_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Group>> {
        let row = sqlx::query("SELECT * FROM chat_groups WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(group_from_row).transpose()
    }

    async fn get_by_conversation(&self, conversation_id: i64) -> Result<Option<Group>> {
        let row = sqlx::query("SELECT * FROM chat_groups WHERE conversation_id = ?")
            .bind(conversation_id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(group_from_row).transpose()
    }

    async fn update(&self, group: &Group) -> Result<()> {
        sqlx::query("UPDATE chat_groups SET name = ?, invite_policy = ? WHERE id = ?")
            .bind(&group.name)
            .bind(group_role_str(group.invite_policy))
            .bind(group.id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM chat_groups WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn add_member(&self, member: &GroupMember) -> Result<()> {
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id, role, muted_until, joined_at)
             VALUES (?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE role = VALUES(role)",
        )
        .bind(member.group_id)
        .bind(member.user_id)
        .bind(group_role_str(member.role))
        .bind(opt_naive(member.muted_until))
        .bind(naive(member.joined_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get_member(&self, group_id: i64, user_id: i64) -> Result<Option<GroupMember>> {
        let row = sqlx::query("SELECT * FROM group_members WHERE group_id = ? AND user_id = ?")
            .bind(group_id)
            .bind(user_id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(group_member_from_row).transpose()
    }

    async fn list_members(&self, group_id: i64) -> Result<Vec<GroupMember>> {
        let rows = sqlx::query("SELECT * FROM group_members WHERE group_id = ?")
            .bind(group_id)
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter().map(group_member_from_row).collect()
    }

    async fn update_member(&self, member: &GroupMember) -> Result<()> {
        sqlx::query(
            "UPDATE group_members SET role = ?, muted_until = ? WHERE group_id = ? AND user_id = ?",
        )
        .bind(group_role_str(member.role))
        .bind(opt_naive(member.muted_until))
        .bind(member.group_id)
        .bind(member.user_id)
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn remove_member(&self, group_id: i64, user_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
            .bind(group_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn set_owner(&self, group_id: i64, owner_id: i64) -> Result<()> {
        sqlx::query("UPDATE chat_groups SET owner_id = ? WHERE id = ?")
            .bind(owner_id)
            .bind(group_id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }
}

fn group_role_str(role: GroupRole) -> &'static str {
    match role {
        GroupRole::Owner => "owner",
        GroupRole::Admin => "admin",
        GroupRole::Member => "member",
    }
}

#[async_trait]
impl BlockRepo for MySqlShard {
    async fn add(&self, block: &Block) -> Result<()> {
        sqlx::query(
            "INSERT INTO blocks (owner_id, blocked_id, created_at) VALUES (?, ?, ?)
             ON DUPLICATE KEY UPDATE created_at = VALUES(created_at)",
        )
        .bind(block.owner_id)
        .bind(block.blocked_id)
        .bind(naive(block.created_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn remove(&self, owner_id: i64, blocked_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM blocks WHERE owner_id = ? AND blocked_id = ?")
            .bind(owner_id)
            .bind(blocked_id)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn is_blocked(&self, owner_id: i64, blocked_id: i64) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE owner_id = ? AND blocked_id = ?")
                .bind(owner_id)
                .bind(blocked_id)
                .fetch_one(self.read_pool())
                .await
                .map_err(Error::storage)?;
        Ok(count > 0)
    }

    async fn list(&self, owner_id: i64) -> Result<Vec<Block>> {
        let rows = sqlx::query("SELECT * FROM blocks WHERE owner_id = ?")
            .bind(owner_id)
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter()
            .map(|row| {
                Ok(Block {
                    owner_id: row.try_get("owner_id").map_err(Error::storage)?,
                    blocked_id: row.try_get("blocked_id").map_err(Error::storage)?,
                    created_at: ts(row, "created_at")?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl AttachmentRepo for MySqlShard {
    async fn create(&self, attachment: &Attachment) -> Result<()> {
        sqlx::query(
            "INSERT INTO attachments
             (id, owner_id, filename, size, mime, sha256, storage_key, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(attachment.id)
        .bind(attachment.owner_id)
        .bind(&attachment.filename)
        .bind(attachment.size)
        .bind(&attachment.mime)
        .bind(&attachment.sha256)
        .bind(&attachment.storage_key)
        .bind(naive(attachment.created_at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Attachment>> {
        let row = sqlx::query("SELECT * FROM attachments WHERE id = ?")
            .bind(id)
            .fetch_optional(self.read_pool())
            .await
            .map_err(Error::storage)?;
        row.as_ref().map(attachment_from_row).transpose()
    }
}

#[async_trait]
impl OutboxRepo for MySqlShard {
    async fn pending(&self, limit: usize) -> Result<Vec<OutboxEvent>> {
        let rows =
            sqlx::query("SELECT * FROM outbox WHERE published_at IS NULL ORDER BY id ASC LIMIT ?")
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(Error::storage)?;
        rows.iter().map(outbox_from_row).collect()
    }

    async fn mark_published(&self, ids: &[i64], at: Timestamp) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut qb = QueryBuilder::<MySql>::new("UPDATE outbox SET published_at = ");
        qb.push_bind(naive(at));
        qb.push(" WHERE id IN (");
        let mut separated = qb.separated(", ");
        for id in ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");
        qb.build()
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }
}

#[async_trait]
impl PluginRepo for MySqlShard {
    async fn is_enabled(&self, name: &str) -> Result<bool> {
        let value: Option<i8> =
            sqlx::query_scalar("SELECT enabled FROM plugin_states WHERE name = ?")
                .bind(name)
                .fetch_optional(self.read_pool())
                .await
                .map_err(Error::storage)?;
        Ok(value.unwrap_or(0) != 0)
    }

    async fn set_enabled(&self, name: &str, enabled: bool, at: Timestamp) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_states (name, enabled, updated_at) VALUES (?, ?, ?)
             ON DUPLICATE KEY UPDATE enabled = VALUES(enabled), updated_at = VALUES(updated_at)",
        )
        .bind(name)
        .bind(enabled as i8)
        .bind(naive(at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<(String, bool)>> {
        let rows = sqlx::query("SELECT name, enabled FROM plugin_states")
            .fetch_all(self.read_pool())
            .await
            .map_err(Error::storage)?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get("name").map_err(Error::storage)?,
                    bool_of(row, "enabled")?,
                ))
            })
            .collect()
    }
}

#[async_trait]
impl PluginKvRepo for MySqlShard {
    async fn get(&self, plugin: &str, key: &str) -> Result<Option<String>> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT v FROM plugin_kv
             WHERE plugin = ? AND k = ? AND (expires_at IS NULL OR expires_at > ?)",
        )
        .bind(plugin)
        .bind(key)
        .bind(naive(Utc::now()))
        .fetch_optional(self.read_pool())
        .await
        .map_err(Error::storage)?;
        Ok(value)
    }

    async fn set(
        &self,
        plugin: &str,
        key: &str,
        value: &str,
        expires_at: Option<Timestamp>,
        at: Timestamp,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO plugin_kv (plugin, k, v, expires_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE v = VALUES(v), expires_at = VALUES(expires_at),
                                     updated_at = VALUES(updated_at)",
        )
        .bind(plugin)
        .bind(key)
        .bind(value)
        .bind(expires_at.map(naive))
        .bind(naive(at))
        .execute(&self.pool)
        .await
        .map_err(Error::storage)?;
        Ok(())
    }

    async fn delete(&self, plugin: &str, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM plugin_kv WHERE plugin = ? AND k = ?")
            .bind(plugin)
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(Error::storage)?;
        Ok(())
    }

    async fn list(&self, plugin: &str, prefix: &str) -> Result<Vec<(String, String)>> {
        let pattern = format!(
            "{}%",
            prefix
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let rows = sqlx::query(
            "SELECT k, v FROM plugin_kv
             WHERE plugin = ? AND k LIKE ? AND (expires_at IS NULL OR expires_at > ?)
             ORDER BY k LIMIT 1000",
        )
        .bind(plugin)
        .bind(pattern)
        .bind(naive(Utc::now()))
        .fetch_all(self.read_pool())
        .await
        .map_err(Error::storage)?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get("k").map_err(Error::storage)?,
                    row.try_get("v").map_err(Error::storage)?,
                ))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Sharded dispatcher
//
// The implementations below forward to the owning shard when a routing key is
// available and fan out when it is not. Fan-out reads merge and order the
// per-shard results; fan-out writes apply to every shard (idempotently).
// Calls use fully-qualified trait syntax because a shard implements every
// repository trait and the method names collide.
// ---------------------------------------------------------------------------

#[async_trait]
impl UserRepo for MySqlStore {
    async fn create(&self, user: &User) -> Result<()> {
        UserRepo::create(self.shard_for(user.id), user).await
    }

    async fn get(&self, id: i64) -> Result<Option<User>> {
        UserRepo::get(self.shard_for(id), id).await
    }

    async fn get_by_user_id(&self, user_id: &str) -> Result<Option<User>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| UserRepo::get_by_user_id(shard, user_id)),
        )
        .await;
        for result in results {
            if let Some(user) = result? {
                return Ok(Some(user));
            }
        }
        Ok(None)
    }

    async fn update(&self, user: &User) -> Result<()> {
        UserRepo::update(self.shard_for(user.id), user).await
    }

    async fn exists_user_id(&self, user_id: &str) -> Result<bool> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| UserRepo::exists_user_id(shard, user_id)),
        )
        .await;
        for result in results {
            if result? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[async_trait]
impl SessionRepo for MySqlStore {
    async fn create(&self, session: &Session) -> Result<()> {
        SessionRepo::create(self.shard_for_str(&session.id), session).await
    }

    async fn get(&self, id: &str) -> Result<Option<Session>> {
        SessionRepo::get(self.shard_for_str(id), id).await
    }

    async fn update(&self, session: &Session) -> Result<()> {
        SessionRepo::update(self.shard_for_str(&session.id), session).await
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Session>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| SessionRepo::list_for_user(shard, user_id)),
        )
        .await;
        let mut sessions = Vec::new();
        for result in results {
            sessions.extend(result?);
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.issued_at));
        Ok(sessions)
    }

    async fn revoke_for_device(&self, device_id: i64) -> Result<()> {
        // A device id does not route sessions, so revoke on every shard.
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| SessionRepo::revoke_for_device(shard, device_id)),
        )
        .await;
        for result in results {
            result?;
        }
        Ok(())
    }
}

#[async_trait]
impl DeviceRepo for MySqlStore {
    async fn create(&self, device: &Device) -> Result<()> {
        DeviceRepo::create(self.shard_for(device.id), device).await
    }

    async fn get(&self, id: i64) -> Result<Option<Device>> {
        DeviceRepo::get(self.shard_for(id), id).await
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Device>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| DeviceRepo::list_for_user(shard, user_id)),
        )
        .await;
        let mut devices = Vec::new();
        for result in results {
            devices.extend(result?);
        }
        devices.sort_by_key(|device| std::cmp::Reverse(device.created_at));
        Ok(devices)
    }

    async fn delete(&self, id: i64) -> Result<()> {
        DeviceRepo::delete(self.shard_for(id), id).await
    }

    async fn touch(&self, id: i64, at: Timestamp) -> Result<()> {
        DeviceRepo::touch(self.shard_for(id), id, at).await
    }
}

#[async_trait]
impl ConversationRepo for MySqlStore {
    async fn create(&self, conversation: &Conversation) -> Result<()> {
        ConversationRepo::create(self.shard_for(conversation.id), conversation).await
    }

    async fn get(&self, id: i64) -> Result<Option<Conversation>> {
        ConversationRepo::get(self.shard_for(id), id).await
    }

    async fn find_direct(&self, a: i64, b: i64) -> Result<Option<Conversation>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| ConversationRepo::find_direct(shard, a, b)),
        )
        .await;
        for result in results {
            if let Some(conversation) = result? {
                return Ok(Some(conversation));
            }
        }
        Ok(None)
    }

    async fn add_member(&self, member: &ConversationMember) -> Result<()> {
        ConversationRepo::add_member(self.shard_for(member.conversation_id), member).await
    }

    async fn get_member(
        &self,
        conversation_id: i64,
        user_id: i64,
    ) -> Result<Option<ConversationMember>> {
        ConversationRepo::get_member(self.shard_for(conversation_id), conversation_id, user_id)
            .await
    }

    async fn list_members(&self, conversation_id: i64) -> Result<Vec<ConversationMember>> {
        ConversationRepo::list_members(self.shard_for(conversation_id), conversation_id).await
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<ConversationMember>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| ConversationRepo::list_for_user(shard, user_id)),
        )
        .await;
        let mut members = Vec::new();
        for result in results {
            members.extend(result?);
        }
        Ok(members)
    }

    async fn set_read_cursor(&self, conversation_id: i64, user_id: i64, seq: u64) -> Result<()> {
        ConversationRepo::set_read_cursor(
            self.shard_for(conversation_id),
            conversation_id,
            user_id,
            seq,
        )
        .await
    }

    async fn remove_member(&self, conversation_id: i64, user_id: i64) -> Result<()> {
        ConversationRepo::remove_member(self.shard_for(conversation_id), conversation_id, user_id)
            .await
    }

    async fn set_mute(
        &self,
        conversation_id: i64,
        user_id: i64,
        until: Option<Timestamp>,
    ) -> Result<()> {
        ConversationRepo::set_mute(
            self.shard_for(conversation_id),
            conversation_id,
            user_id,
            until,
        )
        .await
    }
}

#[async_trait]
impl MessageRepo for MySqlStore {
    async fn append(&self, message: &Message, outbox: &OutboxEvent) -> Result<()> {
        // The message and its outbox row must commit in one transaction, so
        // they share the conversation shard.
        MessageRepo::append(self.shard_for(message.conversation_id), message, outbox).await
    }

    async fn get(&self, id: i64) -> Result<Option<Message>> {
        // A message id does not encode its conversation shard; probe all.
        let results =
            futures::future::join_all(self.shards.iter().map(|shard| MessageRepo::get(shard, id)))
                .await;
        for result in results {
            if let Some(message) = result? {
                return Ok(Some(message));
            }
        }
        Ok(None)
    }

    async fn find_by_client_id(
        &self,
        sender_id: i64,
        client_msg_id: &str,
    ) -> Result<Option<Message>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| MessageRepo::find_by_client_id(shard, sender_id, client_msg_id)),
        )
        .await;
        for result in results {
            if let Some(message) = result? {
                return Ok(Some(message));
            }
        }
        Ok(None)
    }

    async fn list(
        &self,
        conversation_id: i64,
        before: Option<u64>,
        after: Option<u64>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        MessageRepo::list(
            self.shard_for(conversation_id),
            conversation_id,
            before,
            after,
            limit,
        )
        .await
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<Message>> {
        // Resolve filename matches first: attachments can live on other shards,
        // so the per-shard query filters by id rather than joining.
        let attachment_ids = match query.filename.as_deref() {
            Some(filename) => {
                let pattern = filename.replace('*', "%").replace('?', "_");
                let results = futures::future::join_all(
                    self.shards
                        .iter()
                        .map(|shard| shard.find_attachment_ids(&pattern)),
                )
                .await;
                let mut ids = Vec::new();
                for result in results {
                    ids.extend(result?);
                }
                Some(ids)
            }
            None => None,
        };

        let mut rows = if let Some(conversation_id) = query.conversation_id {
            self.shard_for(conversation_id)
                .search_messages(query, attachment_ids.as_deref())
                .await?
        } else {
            let results = futures::future::join_all(
                self.shards
                    .iter()
                    .map(|shard| shard.search_messages(query, attachment_ids.as_deref())),
            )
            .await;
            let mut all = Vec::new();
            for result in results {
                all.extend(result?);
            }
            all
        };
        match query.sort {
            SortOrder::Asc => rows.sort_by_key(|message| message.seq),
            SortOrder::Desc => rows.sort_by_key(|message| std::cmp::Reverse(message.seq)),
        }
        rows.truncate(query.limit as usize);
        Ok(rows)
    }

    async fn count(&self, conversation_id: i64) -> Result<u64> {
        MessageRepo::count(self.shard_for(conversation_id), conversation_id).await
    }

    async fn count_after(&self, conversation_id: i64, after: u64) -> Result<u64> {
        MessageRepo::count_after(self.shard_for(conversation_id), conversation_id, after).await
    }

    async fn next_seq(&self, conversation_id: i64) -> Result<u64> {
        MessageRepo::next_seq(self.shard_for(conversation_id), conversation_id).await
    }
}

#[async_trait]
impl ContactRepo for MySqlStore {
    async fn upsert(&self, contact: &Contact) -> Result<()> {
        ContactRepo::upsert(self.shard_for(contact.owner_id), contact).await
    }

    async fn get(&self, owner_id: i64, contact_id: i64) -> Result<Option<Contact>> {
        ContactRepo::get(self.shard_for(owner_id), owner_id, contact_id).await
    }

    async fn list(&self, owner_id: i64) -> Result<Vec<Contact>> {
        ContactRepo::list(self.shard_for(owner_id), owner_id).await
    }

    async fn set_remark(
        &self,
        owner_id: i64,
        contact_id: i64,
        remark: Option<String>,
        remark_avatar_url: Option<String>,
    ) -> Result<()> {
        ContactRepo::set_remark(
            self.shard_for(owner_id),
            owner_id,
            contact_id,
            remark,
            remark_avatar_url,
        )
        .await
    }

    async fn delete(&self, owner_id: i64, contact_id: i64) -> Result<()> {
        ContactRepo::delete(self.shard_for(owner_id), owner_id, contact_id).await
    }
}

#[async_trait]
impl FriendRequestRepo for MySqlStore {
    async fn create(&self, request: &FriendRequest) -> Result<()> {
        FriendRequestRepo::create(self.shard_for(request.id), request).await
    }

    async fn get(&self, id: i64) -> Result<Option<FriendRequest>> {
        FriendRequestRepo::get(self.shard_for(id), id).await
    }

    async fn find_pending(&self, a: i64, b: i64) -> Result<Option<FriendRequest>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| FriendRequestRepo::find_pending(shard, a, b)),
        )
        .await;
        for result in results {
            if let Some(request) = result? {
                return Ok(Some(request));
            }
        }
        Ok(None)
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<FriendRequest>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| FriendRequestRepo::list_for_user(shard, user_id)),
        )
        .await;
        let mut requests = Vec::new();
        for result in results {
            requests.extend(result?);
        }
        requests.sort_by_key(|request| std::cmp::Reverse(request.created_at));
        Ok(requests)
    }

    async fn update(&self, request: &FriendRequest) -> Result<()> {
        FriendRequestRepo::update(self.shard_for(request.id), request).await
    }
}

#[async_trait]
impl GroupRepo for MySqlStore {
    async fn create(&self, group: &Group) -> Result<()> {
        GroupRepo::create(self.shard_for(group.id), group).await
    }

    async fn get(&self, id: i64) -> Result<Option<Group>> {
        GroupRepo::get(self.shard_for(id), id).await
    }

    async fn get_by_conversation(&self, conversation_id: i64) -> Result<Option<Group>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| GroupRepo::get_by_conversation(shard, conversation_id)),
        )
        .await;
        for result in results {
            if let Some(group) = result? {
                return Ok(Some(group));
            }
        }
        Ok(None)
    }

    async fn update(&self, group: &Group) -> Result<()> {
        GroupRepo::update(self.shard_for(group.id), group).await
    }

    async fn delete(&self, id: i64) -> Result<()> {
        GroupRepo::delete(self.shard_for(id), id).await
    }

    async fn add_member(&self, member: &GroupMember) -> Result<()> {
        GroupRepo::add_member(self.shard_for(member.group_id), member).await
    }

    async fn get_member(&self, group_id: i64, user_id: i64) -> Result<Option<GroupMember>> {
        GroupRepo::get_member(self.shard_for(group_id), group_id, user_id).await
    }

    async fn list_members(&self, group_id: i64) -> Result<Vec<GroupMember>> {
        GroupRepo::list_members(self.shard_for(group_id), group_id).await
    }

    async fn update_member(&self, member: &GroupMember) -> Result<()> {
        GroupRepo::update_member(self.shard_for(member.group_id), member).await
    }

    async fn remove_member(&self, group_id: i64, user_id: i64) -> Result<()> {
        GroupRepo::remove_member(self.shard_for(group_id), group_id, user_id).await
    }

    async fn set_owner(&self, group_id: i64, owner_id: i64) -> Result<()> {
        GroupRepo::set_owner(self.shard_for(group_id), group_id, owner_id).await
    }
}

#[async_trait]
impl BlockRepo for MySqlStore {
    async fn add(&self, block: &Block) -> Result<()> {
        BlockRepo::add(self.shard_for(block.owner_id), block).await
    }

    async fn remove(&self, owner_id: i64, blocked_id: i64) -> Result<()> {
        BlockRepo::remove(self.shard_for(owner_id), owner_id, blocked_id).await
    }

    async fn is_blocked(&self, owner_id: i64, blocked_id: i64) -> Result<bool> {
        BlockRepo::is_blocked(self.shard_for(owner_id), owner_id, blocked_id).await
    }

    async fn list(&self, owner_id: i64) -> Result<Vec<Block>> {
        BlockRepo::list(self.shard_for(owner_id), owner_id).await
    }
}

#[async_trait]
impl AttachmentRepo for MySqlStore {
    async fn create(&self, attachment: &Attachment) -> Result<()> {
        AttachmentRepo::create(self.shard_for(attachment.id), attachment).await
    }

    async fn get(&self, id: i64) -> Result<Option<Attachment>> {
        AttachmentRepo::get(self.shard_for(id), id).await
    }
}

#[async_trait]
impl OutboxRepo for MySqlStore {
    async fn pending(&self, limit: usize) -> Result<Vec<OutboxEvent>> {
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| OutboxRepo::pending(shard, limit)),
        )
        .await;
        let mut events = Vec::new();
        for result in results {
            events.extend(result?);
        }
        events.sort_by_key(|event| event.id);
        events.truncate(limit);
        Ok(events)
    }

    async fn mark_published(&self, ids: &[i64], at: Timestamp) -> Result<()> {
        // Outbox ids do not route, so mark on every shard.
        let results = futures::future::join_all(
            self.shards
                .iter()
                .map(|shard| OutboxRepo::mark_published(shard, ids, at)),
        )
        .await;
        for result in results {
            result?;
        }
        Ok(())
    }
}

#[async_trait]
impl PluginRepo for MySqlStore {
    // Plugin state is global and low-volume; it lives on the first shard.
    async fn is_enabled(&self, name: &str) -> Result<bool> {
        PluginRepo::is_enabled(&self.shards[0], name).await
    }

    async fn set_enabled(&self, name: &str, enabled: bool, at: Timestamp) -> Result<()> {
        PluginRepo::set_enabled(&self.shards[0], name, enabled, at).await
    }

    async fn list(&self) -> Result<Vec<(String, bool)>> {
        PluginRepo::list(&self.shards[0]).await
    }
}

#[async_trait]
impl PluginKvRepo for MySqlStore {
    // Plugin state is global and low-volume; it lives on the first shard.
    async fn get(&self, plugin: &str, key: &str) -> Result<Option<String>> {
        PluginKvRepo::get(&self.shards[0], plugin, key).await
    }

    async fn set(
        &self,
        plugin: &str,
        key: &str,
        value: &str,
        expires_at: Option<Timestamp>,
        at: Timestamp,
    ) -> Result<()> {
        PluginKvRepo::set(&self.shards[0], plugin, key, value, expires_at, at).await
    }

    async fn delete(&self, plugin: &str, key: &str) -> Result<()> {
        PluginKvRepo::delete(&self.shards[0], plugin, key).await
    }

    async fn list(&self, plugin: &str, prefix: &str) -> Result<Vec<(String, String)>> {
        PluginKvRepo::list(&self.shards[0], plugin, prefix).await
    }
}

#[cfg(test)]
mod tests {
    use super::{fnv1a, mix64, shard_index};

    #[test]
    fn fnv1a_is_stable() {
        // The exact values pin the routing so shard assignments cannot drift
        // silently across releases.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("chatbox"), fnv1a("chatbox"));
        assert_ne!(fnv1a("a"), fnv1a("b"));
    }

    #[test]
    fn shard_index_is_in_range_and_balanced() {
        for key in 0..1000u64 {
            let index = shard_index(key, 4);
            assert!(index < 4);
        }
        let mut seen = [false; 4];
        for key in 0..4u64 {
            seen[shard_index(key, 4)] = true;
        }
        assert!(seen.iter().all(|hit| *hit));
    }

    #[test]
    fn mix_spreads_snowflake_shaped_ids() {
        // Snowflake ids generated in distinct milliseconds end in many zero
        // bits. Without mixing these would all land on one shard.
        let mut seen = [false; 4];
        for step in 0..64u64 {
            let snowflake = step << 12;
            seen[shard_index(mix64(snowflake), 4)] = true;
        }
        assert!(
            seen.iter().all(|hit| *hit),
            "mixed shard ids should cover every shard"
        );
    }
}
