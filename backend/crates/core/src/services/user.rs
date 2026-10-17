//! User profile and device management.

use super::{AuthContext, Services};
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

    /// Update the caller's display name.
    pub async fn update_profile(
        &self,
        ctx: &AuthContext,
        display_name: Option<String>,
    ) -> Result<User> {
        let mut user = self.get_user(ctx.user_id).await?;
        if let Some(name) = display_name {
            let trimmed = name.trim();
            if trimmed.chars().count() > 64 {
                return Err(Error::Invalid("display name is too long".into()));
            }
            user.display_name = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
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
}
