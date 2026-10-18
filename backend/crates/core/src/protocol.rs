//! Realtime wire protocol shared by the gateway, workers, and clients.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{Message, MessageKind, Timestamp};

/// A frame sent by a client over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// Authenticate the connection and bind it to a device.
    Auth {
        /// Access token.
        token: String,
        /// Device id.
        device_id: i64,
    },
    /// Send a message to a conversation.
    Send {
        /// Client idempotency key.
        client_msg_id: String,
        /// Target conversation.
        conversation_id: i64,
        /// Payload kind.
        kind: MessageKind,
        /// Text body.
        #[serde(default)]
        content: Option<String>,
        /// Attachment id for file messages.
        #[serde(default)]
        attachment_id: Option<i64>,
    },
    /// Advance the read cursor.
    Ack {
        /// Conversation.
        conversation_id: i64,
        /// Highest read sequence.
        seq: u64,
    },
    /// Catch up after a reconnect.
    Resume {
        /// Per-conversation cursors.
        cursors: HashMap<i64, u64>,
    },
    /// Heartbeat.
    Ping,
}

/// A client-visible data area that changed elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncScope {
    /// The conversation list (new conversations, titles, membership).
    Conversations,
    /// The contact list (accepted/removed contacts, remarks, avatars).
    Contacts,
    /// Incoming and outgoing friend requests.
    Requests,
    /// Group metadata and membership (roles, mute, rename, dissolve).
    Groups,
}

/// A frame sent by the server over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    /// The connection is authenticated and ready.
    Ready {
        /// Server time when the frame was produced.
        server_time: Timestamp,
    },
    /// Acknowledgement of a send, carrying the assigned sequence.
    Ack {
        /// Echoed idempotency key.
        client_msg_id: String,
        /// Conversation.
        conversation_id: i64,
        /// Assigned per-conversation sequence.
        seq: u64,
        /// Assigned message id.
        message_id: i64,
    },
    /// A delivered message.
    Message {
        /// The message.
        message: Message,
    },
    /// A member advanced their read cursor.
    Read {
        /// Conversation.
        conversation_id: i64,
        /// Member whose cursor advanced.
        user_id: i64,
        /// Highest sequence the member has read.
        seq: u64,
    },
    /// A hint that a message exists; large groups pull by cursor.
    MessageHint {
        /// Conversation.
        conversation_id: i64,
        /// Sequence to pull from.
        seq: u64,
    },
    /// A presence change.
    Presence {
        /// User whose presence changed.
        user_id: i64,
        /// Whether the user is online.
        online: bool,
        /// Number of live devices.
        devices: usize,
    },
    /// One or more server-side areas changed and the client should refresh.
    Sync {
        /// Areas that changed.
        scopes: Vec<SyncScope>,
        /// Conversation affected by the change, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conversation_id: Option<i64>,
        /// Group affected by the change, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<i64>,
    },
    /// Heartbeat reply.
    Pong,
    /// An error.
    Error {
        /// Stable error code.
        code: String,
        /// Human message.
        message: String,
    },
}

/// A delivery routed to one exact gateway connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    /// The gateway node that owns the connection.
    pub node_id: String,
    /// Target user.
    pub user_id: i64,
    /// Target connection.
    pub conn_id: String,
    /// Frame to push.
    pub frame: ServerFrame,
}

/// Bus topic carrying [`Delivery`] envelopes.
pub const TOPIC_DELIVERY: &str = "delivery";

/// Bus topic carrying presence updates.
pub const TOPIC_PRESENCE: &str = "presence";
