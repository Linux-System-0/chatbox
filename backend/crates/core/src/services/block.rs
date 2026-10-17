//! Direct-chat block list.

use super::{AuthContext, Services};
use crate::error::{Error, Result};
use crate::model::Block;

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
        Ok(block)
    }

    /// Unblock a user.
    pub async fn unblock_user(&self, ctx: &AuthContext, user_id_str: &str) -> Result<()> {
        let target = self.get_user_by_user_id(user_id_str).await?;
        self.stores.blocks.remove(ctx.user_id, target.id).await?;
        self.infra
            .cache
            .delete(&block_key(ctx.user_id, target.id))
            .await?;
        Ok(())
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
