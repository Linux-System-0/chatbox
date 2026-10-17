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
