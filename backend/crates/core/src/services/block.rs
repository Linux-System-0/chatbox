//! Direct-chat block list.

use super::{AuthContext, Services, events::sync_frame};
use crate::error::{Error, Result};
use crate::model::{Block, Timestamp, User};
use crate::protocol::SyncScope;

/// A blocked user with their profile.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockView {
    /// The blocked user.
    pub user: User,
    /// When the block was created.
    pub created_at: Timestamp,
}

impl Services {
    /// Block a user. Blocking is idempotent.
    pub async fn block_user(&self, ctx: &AuthContext, user_id_str: &str) -> Result<Block> {
        let target = self.get_user_by_user_id(user_id_str).await?;
        if target.id == ctx.user_id {
            return Err(Error::Invalid("cannot block yourself".into()));
        }
        let now = self.now();
        let block = Block {
            owner_id: ctx.user_id,
            blocked_id: target.id,
            created_at: now,
        };
        self.stores.blocks.add(&block).await?;
        self.infra
            .cache
            .delete(&block_key(ctx.user_id, target.id))
            .await?;
        self.notify_change(ctx.user_id).await;
        Ok(block)
    }

    async fn notify_change(&self, user_id: i64) {
        let frame = sync_frame(&[SyncScope::Contacts], None, None);
        if let Err(e) = self.notify_users(&[user_id], &frame).await {
            tracing::warn!(error = %e, "block sync notification failed");
        }
    }

    /// Unblock a user.
    pub async fn unblock_user(&self, ctx: &AuthContext, user_id_str: &str) -> Result<()> {
        let target = self.get_user_by_user_id(user_id_str).await?;
        self.stores.blocks.remove(ctx.user_id, target.id).await?;
        self.infra
            .cache
            .delete(&block_key(ctx.user_id, target.id))
            .await?;
        self.notify_change(ctx.user_id).await;
        Ok(())
    }

    /// List the caller's blocks with profiles.
    pub async fn list_blocked_users(&self, ctx: &AuthContext) -> Result<Vec<BlockView>> {
        let blocks = self.stores.blocks.list(ctx.user_id).await?;
        let mut views = Vec::with_capacity(blocks.len());
        for block in blocks {
            if let Some(user) = self.stores.users.get(block.blocked_id).await? {
                views.push(BlockView {
                    user,
                    created_at: block.created_at,
                });
            }
        }
        Ok(views)
    }

    /// List the caller's blocks.
    pub async fn list_blocks(&self, ctx: &AuthContext) -> Result<Vec<Block>> {
        self.stores.blocks.list(ctx.user_id).await
    }

    /// Whether direct messaging between two users is blocked in either
    /// direction. Results are cached briefly.
    pub async fn is_blocked_between(&self, a: i64, b: i64) -> Result<bool> {
        let key = block_key(a, b);
        if let Some(v) = self.infra.cache.get(&key).await? {
            return Ok(v == "1");
        }
        let blocked = self.stores.blocks.is_blocked(a, b).await?
            || self.stores.blocks.is_blocked(b, a).await?;
        self.infra
            .cache
            .set(
                &key,
                if blocked { "1" } else { "0" },
                std::time::Duration::from_secs(30),
            )
            .await?;
        Ok(blocked)
    }
}

fn block_key(a: i64, b: i64) -> String {
    // Order-independent so both directions share one cache entry.
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    format!("block:{lo}:{hi}")
}
