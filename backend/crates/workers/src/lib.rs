//! Background workers: the transactional outbox relay and message fan-out.

use std::time::Duration;

use chatbox_core::error::Result;
use chatbox_core::infra::Infra;
use chatbox_core::model::Message;
use chatbox_core::port::Stores;
use chatbox_core::protocol::{Delivery, ServerFrame, TOPIC_DELIVERY};
use tokio::sync::watch;

/// Resolves message recipients and routes deliveries to owning gateways.
#[derive(Clone)]
pub struct Fanout {
    stores: Stores,
    infra: Infra,
    node_id: String,
    /// Groups larger than this fan out on read (hint + cursor pull).
    write_max_members: u64,
}

impl Fanout {
    /// Create a fan-out helper for the current node.
    pub fn new(
        stores: Stores,
        infra: Infra,
        node_id: impl Into<String>,
        write_max_members: u64,
    ) -> Self {
        Self {
            stores,
            infra,
            node_id: node_id.into(),
            write_max_members,
        }
    }

    /// The node id used for produced deliveries.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Deliver one message to every live connection of every member.
    ///
    /// Returns the number of delivery envelopes published. Offline members
    /// receive nothing here; they catch up by cursor on reconnect.
    pub async fn deliver_message(&self, message: &Message) -> Result<usize> {
        let members = self
            .stores
            .conversations
            .list_members(message.conversation_id)
            .await?;
        let recipient_ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
        let pull = recipient_ids.len() as u64 > self.write_max_members;

        let connections = self.infra.registry.connections_for(&recipient_ids).await?;
        let mut delivered = 0;
        for (user_id, conns) in connections {
            let frame = if pull {
                ServerFrame::MessageHint {
                    conversation_id: message.conversation_id,
                    seq: message.seq,
                }
            } else {
                ServerFrame::Message {
                    message: message.clone(),
                }
            };
            for conn in conns {
                let delivery = Delivery {
                    node_id: conn.node_id.clone(),
                    user_id,
                    conn_id: conn.conn_id.clone(),
                    frame: frame.clone(),
                };
                let payload =
                    serde_json::to_string(&delivery).map_err(chatbox_core::Error::storage)?;
                self.infra.bus.publish(TOPIC_DELIVERY, &payload).await?;
                delivered += 1;
            }
        }
        Ok(delivered)
    }
}

/// Reads the outbox and publishes each message exactly once (at-least-once on
/// the wire; consumers de-duplicate).
pub struct OutboxRelay {
    stores: Stores,
    fanout: Fanout,
    batch: usize,
    interval: Duration,
}

impl OutboxRelay {
    /// Create a relay with the default batch size and poll interval.
    pub fn new(stores: Stores, fanout: Fanout) -> Self {
        Self {
            stores,
            fanout,
            batch: 256,
            interval: Duration::from_millis(200),
        }
    }

    /// Override the batch size.
    pub fn with_batch(mut self, batch: usize) -> Self {
        self.batch = batch;
        self
    }

    /// Override the poll interval.
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Run until `shutdown` flips to `true`.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(self.interval) => {}
            }
            match self.tick().await {
                Ok(0) => {}
                Ok(n) => tracing::debug!(count = n, "outbox relay published events"),
                Err(e) => tracing::error!(error = %e, "outbox relay tick failed"),
            }
        }
    }

    /// Process one batch. Exposed for tests and for a manual drain.
    pub async fn tick(&self) -> Result<usize> {
        let events = self.stores.outbox.pending(self.batch).await?;
        if events.is_empty() {
            return Ok(0);
        }
        let mut published = Vec::with_capacity(events.len());
        for event in &events {
            match serde_json::from_str::<Message>(&event.payload) {
                Ok(message) => {
                    if let Err(e) = self.fanout.deliver_message(&message).await {
                        tracing::warn!(message_id = event.message_id, error = %e, "fan-out failed");
                    }
                }
                Err(e) => {
                    tracing::warn!(message_id = event.message_id, error = %e, "undecodable outbox payload");
                }
            }
            // Mark regardless so a poison payload cannot stall the relay.
            published.push(event.id);
        }
        self.stores
            .outbox
            .mark_published(&published, chrono::Utc::now())
            .await?;
        Ok(published.len())
    }
}
