//! Repository ports (traits) and the [`Stores`] aggregate.
//!
//! Adapters in the `chatbox-storage` crate implement these traits over MySQL,
//! Redis, or in-memory state. The domain depends only on the traits.

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;
use crate::model::*;

/// User persistence.
#[async_trait]
pub trait UserRepo: Send + Sync {
    /// Insert a new user.
    async fn create(&self, user: &User) -> Result<()>;
    /// Fetch by internal id.
    async fn get(&self, id: i64) -> Result<Option<User>>;
    /// Fetch by custom public id.
    async fn get_by_user_id(&self, user_id: &str) -> Result<Option<User>>;
    /// Update a user's mutable fields.
    async fn update(&self, user: &User) -> Result<()>;
    /// Whether a custom id is taken.
    async fn exists_user_id(&self, user_id: &str) -> Result<bool>;
}

/// Session persistence.
#[async_trait]
pub trait SessionRepo: Send + Sync {
    /// Insert a session.
    async fn create(&self, session: &Session) -> Result<()>;
    /// Fetch a session by id.
    async fn get(&self, id: &str) -> Result<Option<Session>>;
    /// Update a session (e.g. revocation).
    async fn update(&self, session: &Session) -> Result<()>;
    /// List sessions for a user, newest first.
    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Session>>;
    /// Revoke every session bound to a device.
    async fn revoke_for_device(&self, device_id: i64) -> Result<()>;
}

/// Device persistence.
#[async_trait]
pub trait DeviceRepo: Send + Sync {
    /// Insert a device.
    async fn create(&self, device: &Device) -> Result<()>;
    /// Fetch a device by id.
    async fn get(&self, id: i64) -> Result<Option<Device>>;
    /// List devices for a user.
    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Device>>;
    /// Delete a device.
    async fn delete(&self, id: i64) -> Result<()>;
    /// Update the last-seen timestamp.
    async fn touch(&self, id: i64, at: Timestamp) -> Result<()>;
}

/// Conversation and membership persistence.
#[async_trait]
pub trait ConversationRepo: Send + Sync {
    /// Insert a conversation.
    async fn create(&self, conversation: &Conversation) -> Result<()>;
    /// Fetch a conversation.
    async fn get(&self, id: i64) -> Result<Option<Conversation>>;
    /// Find an existing direct conversation between two users.
    async fn find_direct(&self, a: i64, b: i64) -> Result<Option<Conversation>>;
    /// Add a member.
    async fn add_member(&self, member: &ConversationMember) -> Result<()>;
    /// Fetch a membership.
    async fn get_member(
        &self,
        conversation_id: i64,
        user_id: i64,
    ) -> Result<Option<ConversationMember>>;
    /// List members of a conversation.
    async fn list_members(&self, conversation_id: i64) -> Result<Vec<ConversationMember>>;
    /// List a user's memberships.
    async fn list_for_user(&self, user_id: i64) -> Result<Vec<ConversationMember>>;
    /// Advance a member's read cursor (monotonic).
    async fn set_read_cursor(&self, conversation_id: i64, user_id: i64, seq: u64) -> Result<()>;
    /// Remove a member.
    async fn remove_member(&self, conversation_id: i64, user_id: i64) -> Result<()>;
    /// Set a member's mute deadline.
    async fn set_mute(
        &self,
        conversation_id: i64,
        user_id: i64,
        until: Option<Timestamp>,
    ) -> Result<()>;
}

/// Message persistence, including the transactional outbox append.
#[async_trait]
pub trait MessageRepo: Send + Sync {
    /// Append a message and its outbox event atomically.
    async fn append(&self, message: &Message, outbox: &OutboxEvent) -> Result<()>;
    /// Fetch a message by id.
    async fn get(&self, id: i64) -> Result<Option<Message>>;
    /// Find a message by sender idempotency key.
    async fn find_by_client_id(
        &self,
        sender_id: i64,
        client_msg_id: &str,
    ) -> Result<Option<Message>>;
    /// List messages in a conversation, newest-anchored.
    ///
    /// When `before` is set, return messages with `seq < before`; when `after`
    /// is set, return messages with `seq > after`. The two are mutually
    /// exclusive. Results are ordered by `seq` ascending.
    async fn list(
        &self,
        conversation_id: i64,
        before: Option<u64>,
        after: Option<u64>,
        limit: u32,
    ) -> Result<Vec<Message>>;
    /// Execute a search query.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<Message>>;
    /// Total number of messages in a conversation (used by tests/admin).
    async fn count(&self, conversation_id: i64) -> Result<u64>;
    /// Number of messages with `seq > after` (used for unread counts).
    async fn count_after(&self, conversation_id: i64, after: u64) -> Result<u64>;
    /// Atomically reserve the next per-conversation sequence number.
    async fn next_seq(&self, conversation_id: i64) -> Result<u64>;
}

/// Contact-list persistence.
#[async_trait]
pub trait ContactRepo: Send + Sync {
    /// Create or replace a contact link.
    async fn upsert(&self, contact: &Contact) -> Result<()>;
    /// Fetch one contact link.
    async fn get(&self, owner_id: i64, contact_id: i64) -> Result<Option<Contact>>;
    /// List contacts for an owner.
    async fn list(&self, owner_id: i64) -> Result<Vec<Contact>>;
    /// Set the private remark fields.
    async fn set_remark(
        &self,
        owner_id: i64,
        contact_id: i64,
        remark: Option<String>,
        remark_avatar_url: Option<String>,
    ) -> Result<()>;
    /// Delete a contact link.
    async fn delete(&self, owner_id: i64, contact_id: i64) -> Result<()>;
}

/// Friend-request persistence.
#[async_trait]
pub trait FriendRequestRepo: Send + Sync {
    /// Insert a request.
    async fn create(&self, request: &FriendRequest) -> Result<()>;
    /// Fetch a request by id.
    async fn get(&self, id: i64) -> Result<Option<FriendRequest>>;
    /// Find a pending request between two users in either direction.
    async fn find_pending(&self, a: i64, b: i64) -> Result<Option<FriendRequest>>;
    /// List requests involving a user.
    async fn list_for_user(&self, user_id: i64) -> Result<Vec<FriendRequest>>;
    /// Update a request.
    async fn update(&self, request: &FriendRequest) -> Result<()>;
}

/// Group persistence.
#[async_trait]
pub trait GroupRepo: Send + Sync {
    /// Insert a group.
    async fn create(&self, group: &Group) -> Result<()>;
    /// Fetch a group.
    async fn get(&self, id: i64) -> Result<Option<Group>>;
    /// Fetch a group by its conversation.
    async fn get_by_conversation(&self, conversation_id: i64) -> Result<Option<Group>>;
    /// Update a group's name or invite policy.
    async fn update(&self, group: &Group) -> Result<()>;
    /// Delete a group.
    async fn delete(&self, id: i64) -> Result<()>;
    /// Add a member.
    async fn add_member(&self, member: &GroupMember) -> Result<()>;
    /// Fetch a member.
    async fn get_member(&self, group_id: i64, user_id: i64) -> Result<Option<GroupMember>>;
    /// List members.
    async fn list_members(&self, group_id: i64) -> Result<Vec<GroupMember>>;
    /// Update a member's role/mute.
    async fn update_member(&self, member: &GroupMember) -> Result<()>;
    /// Remove a member.
    async fn remove_member(&self, group_id: i64, user_id: i64) -> Result<()>;
    /// Set the owner.
    async fn set_owner(&self, group_id: i64, owner_id: i64) -> Result<()>;
}

/// Block-list persistence.
#[async_trait]
pub trait BlockRepo: Send + Sync {
    /// Record a block (idempotent).
    async fn add(&self, block: &Block) -> Result<()>;
    /// Remove a block.
    async fn remove(&self, owner_id: i64, blocked_id: i64) -> Result<()>;
    /// Whether `owner` has blocked `blocked`.
    async fn is_blocked(&self, owner_id: i64, blocked_id: i64) -> Result<bool>;
    /// List a user's blocks.
    async fn list(&self, owner_id: i64) -> Result<Vec<Block>>;
}

/// Attachment metadata persistence.
#[async_trait]
pub trait AttachmentRepo: Send + Sync {
    /// Insert an attachment.
    async fn create(&self, attachment: &Attachment) -> Result<()>;
    /// Fetch an attachment.
    async fn get(&self, id: i64) -> Result<Option<Attachment>>;
}

/// Outbox relay persistence.
#[async_trait]
pub trait OutboxRepo: Send + Sync {
    /// Fetch unpublished events in insertion order.
    async fn pending(&self, limit: usize) -> Result<Vec<OutboxEvent>>;
    /// Mark events published.
    async fn mark_published(&self, ids: &[i64], at: Timestamp) -> Result<()>;
}

/// Server plugin enablement persistence.
#[async_trait]
pub trait PluginRepo: Send + Sync {
    /// Whether a plugin is enabled.
    async fn is_enabled(&self, name: &str) -> Result<bool>;
    /// Persist a plugin's enabled state.
    async fn set_enabled(&self, name: &str, enabled: bool, at: Timestamp) -> Result<()>;
    /// List all persisted plugin states.
    async fn list(&self) -> Result<Vec<(String, bool)>>;
}

/// Server plugin key/value persistence.
///
/// Entries are namespaced by plugin name so a plugin can only reach its own
/// state. The host enforces key and value size limits before calling in.
#[async_trait]
pub trait PluginKvRepo: Send + Sync {
    /// Fetch a value, treating expired entries as absent.
    async fn get(&self, plugin: &str, key: &str) -> Result<Option<String>>;
    /// Insert or replace a value, optionally with an absolute expiry.
    async fn set(
        &self,
        plugin: &str,
        key: &str,
        value: &str,
        expires_at: Option<Timestamp>,
        at: Timestamp,
    ) -> Result<()>;
    /// Delete a value.
    async fn delete(&self, plugin: &str, key: &str) -> Result<()>;
    /// List live entries whose key starts with `prefix`, ordered by key.
    async fn list(&self, plugin: &str, prefix: &str) -> Result<Vec<(String, String)>>;
}

/// All repository ports bundled for injection into services.
#[derive(Clone)]
pub struct Stores {
    /// User repository.
    pub users: Arc<dyn UserRepo>,
    /// Session repository.
    pub sessions: Arc<dyn SessionRepo>,
    /// Device repository.
    pub devices: Arc<dyn DeviceRepo>,
    /// Conversation repository.
    pub conversations: Arc<dyn ConversationRepo>,
    /// Message repository.
    pub messages: Arc<dyn MessageRepo>,
    /// Contact repository.
    pub contacts: Arc<dyn ContactRepo>,
    /// Friend-request repository.
    pub friend_requests: Arc<dyn FriendRequestRepo>,
    /// Group repository.
    pub groups: Arc<dyn GroupRepo>,
    /// Block repository.
    pub blocks: Arc<dyn BlockRepo>,
    /// Attachment repository.
    pub attachments: Arc<dyn AttachmentRepo>,
    /// Outbox repository.
    pub outbox: Arc<dyn OutboxRepo>,
    /// Plugin repository.
    pub plugins: Arc<dyn PluginRepo>,
    /// Plugin key/value repository.
    pub plugin_kv: Arc<dyn PluginKvRepo>,
}
