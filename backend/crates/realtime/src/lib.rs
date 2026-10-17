//! WebSocket realtime gateway.
//!
//! The gateway authenticates a connection, registers it in the connection
//! registry, and pushes outbound frames from a bounded per-connection queue.
//! Fan-out workers publish [`Delivery`](chatbox_core::protocol::Delivery)
//! envelopes on the bus; the [`Hub`] forwards those addressed to this node.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use chatbox_core::error::Error;
use chatbox_core::infra::{ConnectionInfo, MessageBus};
use chatbox_core::protocol::{ClientFrame, Delivery, ServerFrame, TOPIC_DELIVERY, TOPIC_PRESENCE};
use chatbox_core::services::{AuthContext, SendRequest, Services};
use chatbox_plugin::{HookContext, HookOutcome, PluginHost};
use futures::{SinkExt, StreamExt};
use tokio::sync::{RwLock, mpsc};

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const OUTBOUND_QUEUE: usize = 256;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Tracks live connections owned by this gateway node.
pub struct Hub {
    node_id: String,
    senders: RwLock<HashMap<String, Connection>>,
}

struct Connection {
    user_id: i64,
    tx: mpsc::Sender<ServerFrame>,
}

impl Hub {
    /// Create a hub for this node.
    pub fn new(node_id: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            node_id: node_id.into(),
            senders: RwLock::new(HashMap::new()),
        })
    }

    /// The node id this hub serves.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Register a connection's outbound queue.
    pub async fn register(&self, conn_id: String, user_id: i64, tx: mpsc::Sender<ServerFrame>) {
        self.senders
            .write()
            .await
            .insert(conn_id, Connection { user_id, tx });
    }

    /// Remove a connection, returning its user.
    pub async fn unregister(&self, conn_id: &str) -> Option<i64> {
        self.senders
            .write()
            .await
            .remove(conn_id)
            .map(|c| c.user_id)
    }

    /// Number of locally owned connections for a user.
    pub async fn user_connection_count(&self, user_id: i64) -> usize {
        self.senders
            .read()
            .await
            .values()
            .filter(|c| c.user_id == user_id)
            .count()
    }

    /// Push a frame to a local connection. Returns `false` when the connection
    /// is unknown or its queue is closed.
    pub async fn send_to_conn(&self, conn_id: &str, frame: ServerFrame) -> bool {
        let tx = {
            let senders = self.senders.read().await;
            match senders.get(conn_id) {
                Some(conn) => conn.tx.clone(),
                None => return false,
            }
        };
        // `try_send` implements the bounded-queue backpressure policy: if the
        // client is too slow we drop the frame rather than grow memory.
        match tx.try_send(frame) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(conn_id, "dropping frame for slow client");
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    /// Spawn the task that forwards bus deliveries addressed to this node.
    pub fn spawn_delivery_listener(self: &Arc<Self>, bus: Arc<dyn MessageBus>) {
        let hub = self.clone();
        let mut rx = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(message) => {
                        if message.topic != TOPIC_DELIVERY && message.topic != TOPIC_PRESENCE {
                            continue;
                        }
                        match serde_json::from_str::<Delivery>(&message.payload) {
                            Ok(delivery) if delivery.node_id == hub.node_id => {
                                hub.send_to_conn(&delivery.conn_id, delivery.frame).await;
                            }
                            Ok(_) => {}
                            Err(e) => tracing::warn!(error = %e, "undecodable delivery"),
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "delivery listener lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}

/// The gateway state shared by every WebSocket handler.
#[derive(Clone)]
pub struct Gateway {
    /// Domain services.
    pub services: Services,
    /// Server plugin host.
    pub plugins: Arc<PluginHost>,
    /// Local connection hub.
    pub hub: Arc<Hub>,
}

impl Gateway {
    /// Build the gateway router (`GET /ws`).
    pub fn router(self) -> Router {
        Router::new().route("/ws", get(ws_handler)).with_state(self)
    }

    async fn broadcast_presence(&self, user_id: i64, online: bool, devices: usize) {
        let frame = ServerFrame::Presence {
            user_id,
            online,
            devices,
        };
        let conversations = match self
            .services
            .stores
            .conversations
            .list_for_user(user_id)
            .await
        {
            Ok(c) => c,
            Err(_) => return,
        };
        let mut peers = Vec::new();
        for membership in conversations {
            if let Ok(members) = self
                .services
                .stores
                .conversations
                .list_members(membership.conversation_id)
                .await
            {
                for member in members {
                    if member.user_id != user_id && !peers.contains(&member.user_id) {
                        peers.push(member.user_id);
                    }
                }
            }
        }
        if let Ok(conns) = self.services.infra.registry.connections_for(&peers).await {
            for (peer, connections) in conns {
                for conn in connections {
                    let delivery = Delivery {
                        node_id: conn.node_id.clone(),
                        user_id: peer,
                        conn_id: conn.conn_id.clone(),
                        frame: frame.clone(),
                    };
                    if let Ok(payload) = serde_json::to_string(&delivery) {
                        let _ = self
                            .services
                            .infra
                            .bus
                            .publish(TOPIC_PRESENCE, &payload)
                            .await;
                    }
                }
            }
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(gateway): State<Gateway>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, gateway))
}

async fn handle_socket(socket: WebSocket, gateway: Gateway) {
    let (mut sender, mut receiver) = socket.split();

    // 1. Authenticate.
    let ctx = match authenticate(&mut receiver, &gateway.services).await {
        Ok(ctx) => ctx,
        Err(e) => {
            let _ = send_frame(
                &mut sender,
                ServerFrame::Error {
                    code: e.code().to_string(),
                    message: e.to_string(),
                },
            )
            .await;
            return;
        }
    };

    // 2. Register local queue and global registry.
    let (tx, mut rx) = mpsc::channel::<ServerFrame>(OUTBOUND_QUEUE);
    let conn_id = uuid::Uuid::new_v4().to_string();
    gateway
        .hub
        .register(conn_id.clone(), ctx.user_id, tx.clone())
        .await;
    let _ = gateway
        .services
        .infra
        .registry
        .register(
            ctx.user_id,
            ConnectionInfo {
                node_id: gateway.hub.node_id().to_string(),
                device_id: ctx.device_id,
                conn_id: conn_id.clone(),
                last_seen: gateway.services.now(),
            },
        )
        .await;
    let devices = gateway.hub.user_connection_count(ctx.user_id).await;
    let _ = gateway
        .services
        .infra
        .presence
        .set_online(ctx.user_id, true)
        .await;
    gateway.broadcast_presence(ctx.user_id, true, devices).await;

    let _ = tx
        .send(ServerFrame::Ready {
            server_time: gateway.services.now(),
        })
        .await;

    // 3. Writer task.
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if send_frame(&mut sender, frame).await.is_err() {
                break;
            }
        }
    });

    // 4. Reader loop.
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        if handle_client_frame(&gateway, &ctx, &tx, &text).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Ping(_))) => {
                        let _ = tx.try_send(ServerFrame::Pong);
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = heartbeat.tick() => {
                let _ = gateway
                    .services
                    .infra
                    .registry
                    .heartbeat(ctx.user_id, &conn_id, gateway.services.now())
                    .await;
            }
        }
    }

    // 5. Cleanup.
    gateway.hub.unregister(&conn_id).await;
    let _ = gateway
        .services
        .infra
        .registry
        .unregister(ctx.user_id, &conn_id)
        .await;
    writer.abort();
    let remaining = gateway.hub.user_connection_count(ctx.user_id).await;
    if remaining == 0 {
        let _ = gateway
            .services
            .infra
            .presence
            .set_online(ctx.user_id, false)
            .await;
        gateway.broadcast_presence(ctx.user_id, false, 0).await;
    }
}

async fn authenticate(
    receiver: &mut futures::stream::SplitStream<WebSocket>,
    services: &Services,
) -> Result<AuthContext, Error> {
    let deadline = tokio::time::sleep(AUTH_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return Err(Error::Unauthorized),
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        match serde_json::from_str::<ClientFrame>(&text) {
                            Ok(ClientFrame::Auth { token, .. }) => {
                                return services.authenticate(&token).await;
                            }
                            _ => return Err(Error::Unauthorized),
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => return Err(Error::Unauthorized),
                    Some(Ok(_)) => {}
                    Some(Err(_)) => return Err(Error::Unauthorized),
                }
            }
        }
    }
}

async fn handle_client_frame(
    gateway: &Gateway,
    ctx: &AuthContext,
    tx: &mpsc::Sender<ServerFrame>,
    text: &str,
) -> Result<(), Error> {
    let frame: ClientFrame =
        serde_json::from_str(text).map_err(|e| Error::Invalid(format!("invalid frame: {e}")))?;
    match frame {
        ClientFrame::Auth { .. } => {}
        ClientFrame::Ping => {
            let _ = tx.try_send(ServerFrame::Pong);
        }
        ClientFrame::Ack {
            conversation_id,
            seq,
        } => {
            gateway
                .services
                .mark_read(ctx, conversation_id, seq)
                .await?;
        }
        ClientFrame::Resume { cursors } => {
            let messages = gateway.services.resume(ctx, &cursors).await?;
            for message in messages {
                let _ = tx.try_send(ServerFrame::Message { message });
            }
        }
        ClientFrame::Send {
            client_msg_id,
            conversation_id,
            kind,
            content,
            attachment_id,
        } => {
            send_message(
                gateway,
                ctx,
                tx,
                SendRequest {
                    conversation_id,
                    client_msg_id,
                    kind,
                    content,
                    attachment_id,
                },
            )
            .await?;
        }
    }
    Ok(())
}

async fn send_message(
    gateway: &Gateway,
    ctx: &AuthContext,
    tx: &mpsc::Sender<ServerFrame>,
    mut request: SendRequest,
) -> Result<(), Error> {
    let hook_ctx = HookContext::new(
        "message.before_persist",
        serde_json::json!({
            "sender_id": ctx.user_id,
            "conversation_id": request.conversation_id,
            "kind": request.kind,
            "content": request.content,
        }),
    );
    match gateway.plugins.on_message_before_persist(&hook_ctx).await? {
        HookOutcome::Allow => {}
        HookOutcome::Reject(reason) => {
            let _ = tx.try_send(ServerFrame::Error {
                code: "rejected".into(),
                message: reason,
            });
            return Ok(());
        }
        HookOutcome::Rewrite(patch) => {
            if let Some(kind) = patch.kind {
                request.kind = kind;
            }
            if let Some(content) = patch.content {
                request.content = Some(content);
            }
        }
    }
    let message = gateway.services.send_message(ctx, request).await?;
    let after_ctx = HookContext::new(
        "message.after_persist",
        serde_json::to_value(&message).unwrap_or_default(),
    );
    if let Err(e) = gateway.plugins.on_message_after_persist(&after_ctx).await {
        tracing::warn!(error = %e, "after-persist hook failed");
    }
    let _ = tx.try_send(ServerFrame::Ack {
        client_msg_id: message.client_msg_id.clone(),
        conversation_id: message.conversation_id,
        seq: message.seq,
        message_id: message.id,
    });
    Ok(())
}

async fn send_frame(
    sender: &mut futures::stream::SplitSink<WebSocket, WsMessage>,
    frame: ServerFrame,
) -> Result<(), ()> {
    let text = serde_json::to_string(&frame).map_err(|_| ())?;
    sender
        .send(WsMessage::Text(text.into()))
        .await
        .map_err(|_| ())
}
