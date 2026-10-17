//! Domain models.
//!
//! Identifiers are 64-bit Snowflake values (see [`crate::ids`]) except for the
//! human-chosen `user_id`. Timestamps are UTC.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A UTC timestamp.
pub type Timestamp = DateTime<Utc>;

/// The public identifier a user chooses at registration.
pub type UserId = String;

/// The kind of a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    /// A one-to-one conversation.
    Direct,
    /// A group conversation.
    Group,
}

/// The kind of a message payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    /// A text message.
    Text,
    /// A file message referencing an attachment.
    File,
    /// A server-generated system message.
    System,
}

/// A group role, ordered from least to most privileged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    /// A regular member.
    Member,
    /// A group administrator.
    Admin,
    /// The single group owner.
    Owner,
}

impl GroupRole {
    /// Whether this role may rename a group, invite, mute, or kick.
    pub fn is_admin(self) -> bool {
        matches!(self, GroupRole::Owner | GroupRole::Admin)
    }
}

/// The lifecycle state of a friend request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendRequestStatus {
    /// Awaiting the recipient's response.
    Pending,
    /// Accepted; the contact link exists.
    Accepted,
    /// Rejected; no contact link is created.
    Rejected,
}

/// The platform a device runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// The SvelteKit web client.
    Web,
    /// The Kotlin Android client.
    Android,
    /// An unrecognized platform.
    Unknown,
}

/// A registered account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// The unique, human-chosen public ID.
    pub user_id: UserId,
    /// Optional display name; falls back to `user_id`.
    pub display_name: Option<String>,
    /// Optional avatar URL (absolute URL or `data:image/...` URL).
    pub avatar_url: Option<String>,
    /// Argon2id password hash. Never serialized to clients.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last profile update time.
    pub updated_at: Timestamp,
}

impl User {
    /// The name to show to other users.
    pub fn effective_display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.user_id)
    }
}

/// A device a user is signed in on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// Owning user.
    pub user_id: i64,
    /// Client-provided label.
    pub name: String,
    /// Platform.
    pub platform: Platform,
    /// Last heartbeat.
    pub last_seen_at: Timestamp,
    /// Registration time.
    pub created_at: Timestamp,
}

/// A login session bound to a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// JWT identifier (`jti`) for the access token.
    pub id: String,
    /// Owning user.
    pub user_id: i64,
    /// Bound device.
    pub device_id: i64,
    /// Hash of the refresh token.
    #[serde(skip_serializing)]
    pub refresh_hash: String,
    /// Issue time.
    pub issued_at: Timestamp,
    /// Expiry time.
    pub expires_at: Timestamp,
    /// Revocation time, if revoked.
    pub revoked_at: Option<Timestamp>,
}

impl Session {
    /// Whether the session is currently usable.
    pub fn is_active(&self, now: Timestamp) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }
}

/// A conversation, either direct or group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// Direct or group.
    pub kind: ConversationKind,
    /// Creation time.
    pub created_at: Timestamp,
}

/// A user's membership in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMember {
    /// Conversation.
    pub conversation_id: i64,
    /// Member user.
    pub user_id: i64,
    /// The highest sequence the member has read.
    pub last_read_seq: u64,
    /// Optional mute deadline.
    pub muted_until: Option<Timestamp>,
    /// Join time.
    pub joined_at: Timestamp,
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// Conversation.
    pub conversation_id: i64,
    /// Monotonic per-conversation sequence.
    pub seq: u64,
    /// Sender.
    pub sender_id: i64,
    /// Client idempotency key.
    pub client_msg_id: String,
    /// Payload kind.
    pub kind: MessageKind,
    /// Text body or system payload.
    pub content: Option<String>,
    /// Attachment for file messages.
    pub attachment_id: Option<i64>,
    /// Mentioned user IDs.
    pub mentions: Vec<i64>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Edit time, if edited.
    pub edited_at: Option<Timestamp>,
    /// Deletion time, if deleted.
    pub deleted_at: Option<Timestamp>,
}

/// An uploaded file's metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// Uploader.
    pub owner_id: i64,
    /// Original filename.
    pub filename: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type.
    pub mime: String,
    /// SHA-256 hex digest.
    pub sha256: String,
    /// Object storage key.
    pub storage_key: String,
    /// Upload time.
    pub created_at: Timestamp,
}

/// A contact link with an optional per-contact remark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    /// The owner of the contact list.
    pub owner_id: i64,
    /// The contact.
    pub contact_id: i64,
    /// Optional nickname.
    pub remark: Option<String>,
    /// Optional avatar chosen by the contact-list owner.
    pub remark_avatar_url: Option<String>,
    /// Creation time.
    pub created_at: Timestamp,
}

/// A friend request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequest {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// Requesting user.
    pub from_id: i64,
    /// Target user.
    pub to_id: i64,
    /// Optional introduction.
    pub message: Option<String>,
    /// Lifecycle state.
    pub status: FriendRequestStatus,
    /// Creation time.
    pub created_at: Timestamp,
    /// Response time, if answered.
    pub responded_at: Option<Timestamp>,
}

/// A group's metadata. `Group` is 1:1 with a `group`-kind conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// Backing conversation.
    pub conversation_id: i64,
    /// Group name.
    pub name: String,
    /// The single owner.
    pub owner_id: i64,
    /// Maximum role allowed to invite.
    pub invite_policy: GroupRole,
    /// Creation time.
    pub created_at: Timestamp,
}

/// A group member with a role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    /// Group.
    pub group_id: i64,
    /// Member user.
    pub user_id: i64,
    /// Role.
    pub role: GroupRole,
    /// Mute deadline, if muted.
    pub muted_until: Option<Timestamp>,
    /// Join time.
    pub joined_at: Timestamp,
}

/// A block entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// The user who created the block.
    pub owner_id: i64,
    /// The blocked user.
    pub blocked_id: i64,
    /// Creation time.
    pub created_at: Timestamp,
}

/// A row in the transactional outbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEvent {
    /// Internal Snowflake identifier.
    pub id: i64,
    /// The message this event delivers.
    pub message_id: i64,
    /// Conversation shard key.
    pub conversation_id: i64,
    /// JSON payload.
    pub payload: String,
    /// Publish time, once published.
    pub published_at: Option<Timestamp>,
    /// Creation time.
    pub created_at: Timestamp,
}

/// How search results are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SortOrder {
    /// Oldest first.
    #[default]
    Asc,
    /// Newest first.
    Desc,
}

/// A search query over messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    /// The caller; membership is enforced by the service.
    pub viewer_id: i64,
    /// Restrict to one conversation.
    pub conversation_id: Option<i64>,
    /// Restrict to text or file messages.
    pub kind: Option<MessageKind>,
    /// Full-text content query.
    pub content: Option<String>,
    /// Filename wildcard (e.g. `*.mp4`).
    pub filename: Option<String>,
    /// Inclusive lower time bound.
    pub from: Option<Timestamp>,
    /// Inclusive upper time bound.
    pub to: Option<Timestamp>,
    /// Result ordering.
    pub sort: SortOrder,
    /// Maximum results.
    pub limit: u32,
}

/// A client plugin manifest. Client plugins are installed by users and have no
/// server-side effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientPluginManifest {
    /// Plugin name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Plugin kind, e.g. `language-pack` or `theme`.
    pub kind: String,
    /// Relative entry point.
    pub entry: String,
    /// Human description.
    pub description: String,
}

/// A server plugin's registration state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPluginInfo {
    /// Plugin name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Declared plugin API version.
    pub api_version: u32,
    /// Human description.
    pub description: String,
    /// Whether the administrator has enabled it.
    pub enabled: bool,
    /// Whether it is a compiled-in built-in plugin.
    pub builtin: bool,
}
