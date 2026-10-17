//! Realtime change notifications for the control plane.
//!
//! HTTP mutations (contacts, groups, profiles) publish a small `sync` frame to
//! every affected live connection so clients never need a manual refresh. The
//! frames travel through the same delivery bus and connection registry as chat
//! messages, so they work across gateway nodes.

use super::Services;
use crate::error::Result;
use crate::protocol::{Delivery, ServerFrame, SyncScope, TOPIC_DELIVERY};

/// Build a `sync` frame for the given scopes.
pub(crate) fn sync_frame(
    scopes: &[SyncScope],
    conversation_id: Option<i64>,
    group_id: Option<i64>,
) -> ServerFrame {
    ServerFrame::Sync {
        scopes: scopes.to_vec(),
        conversation_id,
        group_id,
    }
}

impl Services {
    /// Push a frame to every live connection owned by `user_ids`.
    ///
    /// Offline users receive nothing here; their next fetch returns the new
    /// state. Notification failures are returned to the caller, which normally
    /// treats them as best effort.
    pub async fn notify_users(&self, user_ids: &[i64], frame: &ServerFrame) -> Result<()> {
        if user_ids.is_empty() {
            return Ok(());
        }
        let connections = self.infra.registry.connections_for(user_ids).await?;
        for (user_id, conns) in connections {
            for conn in conns {
                let delivery = Delivery {
                    node_id: conn.node_id.clone(),
                    user_id,
                    conn_id: conn.conn_id.clone(),
                    frame: frame.clone(),
                };
                let payload = serde_json::to_string(&delivery).map_err(crate::Error::storage)?;
                self.infra.bus.publish(TOPIC_DELIVERY, &payload).await?;
            }
        }
        Ok(())
    }

    /// Push a frame to every live connection of a conversation's members.
    pub async fn notify_conversation(
        &self,
        conversation_id: i64,
        frame: &ServerFrame,
    ) -> Result<()> {
        let members = self
            .stores
            .conversations
            .list_members(conversation_id)
            .await?;
        let user_ids: Vec<i64> = members.into_iter().map(|member| member.user_id).collect();
        self.notify_users(&user_ids, frame).await
    }
}
