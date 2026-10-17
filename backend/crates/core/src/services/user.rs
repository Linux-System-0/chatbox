//! User profile and device management.

use std::collections::HashSet;

use super::{AuthContext, Services, normalize_avatar, normalize_optional_text};
use crate::error::{Error, Result};
use crate::model::{Device, User};

impl Services {
    /// Fetch a user by internal id.
    pub async fn get_user(&self, id: i64) -> Result<User> {
        self.stores.users.get(id).await?.ok_or(Error::NotFound)
    }

    /// Fetch a user by custom public id.
    pub async fn get_user_by_user_id(&self, user_id: &str) -> Result<User> {
        self.stores
            .users
            .get_by_user_id(user_id)
            .await?
            .ok_or(Error::NotFound)
    }

    /// Update the caller's custom id, display name, and/or avatar.
    ///
    /// `None` leaves a field unchanged; `Some(None)` (or an empty string) clears
    /// it. `user_id` cannot be cleared.
    pub async fn update_profile(
        &self,
        ctx: &AuthContext,
        user_id: Option<String>,
        display_name: Option<Option<String>>,
        avatar_url: Option<Option<String>>,
    ) -> Result<User> {
        let mut user = self.get_user(ctx.user_id).await?;

        if let Some(next_id) = user_id {
            let next_id = next_id.trim();
            self.validate_user_id(next_id)?;
            if next_id != user.user_id {
                if self.stores.users.exists_user_id(next_id).await? {
                    return Err(Error::Conflict("user id is already taken".into()));
                }
                user.user_id = next_id.to_string();
            }
        }

        if let Some(name) = display_name {
            user.display_name = normalize_optional_text(name, 64)?;
        }
        if let Some(avatar) = avatar_url {
            user.avatar_url = normalize_avatar(avatar)?;
        }

        user.updated_at = self.now();
        self.stores.users.update(&user).await?;
        Ok(user)
    }

    /// List the caller's devices.
    pub async fn list_devices(&self, user_id: i64) -> Result<Vec<Device>> {
        self.stores.devices.list_for_user(user_id).await
    }

    /// Delete a device and revoke its sessions.
    pub async fn delete_device(&self, ctx: &AuthContext, device_id: i64) -> Result<()> {
        let device = self
            .stores
            .devices
            .get(device_id)
            .await?
            .ok_or(Error::NotFound)?;
        if device.user_id != ctx.user_id {
            return Err(Error::Forbidden("device belongs to another user".into()));
        }
        self.stores.sessions.revoke_for_device(device_id).await?;
        self.stores.devices.delete(device_id).await?;
        Ok(())
    }

    /// The effective display name of a user, falling back to the custom id.
    pub fn display_name_of(user: &User) -> String {
        user.effective_display_name().to_string()
    }

    /// Current online state for the given internal user ids.
    pub async fn presence_for(&self, user_ids: &[i64]) -> Result<Vec<(i64, bool)>> {
        let online = self.infra.presence.online_users(user_ids).await?;
        let online: HashSet<i64> = online.into_iter().collect();
        Ok(user_ids
            .iter()
            .map(|user_id| (*user_id, online.contains(user_id)))
            .collect())
    }
}
