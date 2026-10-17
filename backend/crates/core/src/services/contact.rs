//! Contacts and friend requests.

use super::{AuthContext, Services};
use crate::error::{Error, Result};
use crate::model::{Contact, FriendRequest, FriendRequestStatus, User};

/// A contact together with the contact's profile.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContactView {
    /// The contact user.
    pub user: User,
    /// The per-contact remark.
    pub remark: Option<String>,
    /// When the contact was added.
    pub created_at: crate::model::Timestamp,
}

impl Services {
    /// Send a friend request by custom user id.
    pub async fn send_friend_request(
        &self,
        ctx: &AuthContext,
        to_user_id: &str,
        message: Option<String>,
    ) -> Result<FriendRequest> {
        let target = self.get_user_by_user_id(to_user_id).await?;
        if target.id == ctx.user_id {
            return Err(Error::Invalid("cannot add yourself".into()));
        }
        if self.is_blocked_between(ctx.user_id, target.id).await? {
            return Err(Error::Forbidden("user is blocked".into()));
        }
        if self
            .stores
            .contacts
            .get(ctx.user_id, target.id)
            .await?
            .is_some()
        {
            return Err(Error::Conflict("already a contact".into()));
        }
        if self
            .stores
            .friend_requests
            .find_pending(ctx.user_id, target.id)
            .await?
            .is_some()
        {
            return Err(Error::Conflict("a request is already pending".into()));
        }
        let request = FriendRequest {
            id: self.new_id(),
            from_id: ctx.user_id,
            to_id: target.id,
            message: message.filter(|m| !m.trim().is_empty()),
            status: FriendRequestStatus::Pending,
            created_at: self.now(),
            responded_at: None,
        };
        self.stores.friend_requests.create(&request).await?;
        Ok(request)
    }

    /// List requests sent to or by the caller.
    pub async fn list_friend_requests(&self, ctx: &AuthContext) -> Result<Vec<FriendRequest>> {
        self.stores.friend_requests.list_for_user(ctx.user_id).await
    }

    /// Accept a pending request addressed to the caller.
    pub async fn accept_friend_request(
        &self,
        ctx: &AuthContext,
        request_id: i64,
    ) -> Result<FriendRequest> {
        let mut request = self.load_request(ctx, request_id).await?;
        if request.to_id != ctx.user_id {
            return Err(Error::Forbidden("only the recipient may accept".into()));
        }
        if request.status != FriendRequestStatus::Pending {
            return Err(Error::Conflict("request is no longer pending".into()));
        }
        let now = self.now();
        request.status = FriendRequestStatus::Accepted;
        request.responded_at = Some(now);
        self.stores.friend_requests.update(&request).await?;
        self.stores
            .contacts
            .upsert(&Contact {
                owner_id: request.from_id,
                contact_id: request.to_id,
                remark: None,
                created_at: now,
            })
            .await?;
        self.stores
            .contacts
            .upsert(&Contact {
                owner_id: request.to_id,
                contact_id: request.from_id,
                remark: None,
                created_at: now,
            })
            .await?;
        Ok(request)
    }

    /// Reject a pending request addressed to the caller.
    pub async fn reject_friend_request(
        &self,
        ctx: &AuthContext,
        request_id: i64,
    ) -> Result<FriendRequest> {
        let mut request = self.load_request(ctx, request_id).await?;
        if request.to_id != ctx.user_id {
            return Err(Error::Forbidden("only the recipient may reject".into()));
        }
        if request.status != FriendRequestStatus::Pending {
            return Err(Error::Conflict("request is no longer pending".into()));
        }
        request.status = FriendRequestStatus::Rejected;
        request.responded_at = Some(self.now());
        self.stores.friend_requests.update(&request).await?;
        Ok(request)
    }

    async fn load_request(&self, _ctx: &AuthContext, request_id: i64) -> Result<FriendRequest> {
        self.stores
            .friend_requests
            .get(request_id)
            .await?
            .ok_or(Error::NotFound)
    }

    /// List the caller's contacts with profiles.
    pub async fn list_contacts(&self, ctx: &AuthContext) -> Result<Vec<ContactView>> {
        let contacts = self.stores.contacts.list(ctx.user_id).await?;
        let mut views = Vec::with_capacity(contacts.len());
        for contact in contacts {
            if let Some(user) = self.stores.users.get(contact.contact_id).await? {
                views.push(ContactView {
                    user,
                    remark: contact.remark,
                    created_at: contact.created_at,
                });
            }
        }
        Ok(views)
    }

    /// Set a per-contact remark.
    pub async fn set_contact_remark(
        &self,
        ctx: &AuthContext,
        contact_user_id: &str,
        remark: Option<String>,
    ) -> Result<()> {
        let target = self.get_user_by_user_id(contact_user_id).await?;
        if self
            .stores
            .contacts
            .get(ctx.user_id, target.id)
            .await?
            .is_none()
        {
            return Err(Error::NotFound);
        }
        let clean = remark
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());
        self.stores
            .contacts
            .set_remark(ctx.user_id, target.id, clean)
            .await?;
        Ok(())
    }

    /// Remove a contact in both directions.
    pub async fn remove_contact(&self, ctx: &AuthContext, contact_user_id: &str) -> Result<()> {
        let target = self.get_user_by_user_id(contact_user_id).await?;
        self.stores.contacts.delete(ctx.user_id, target.id).await?;
        Ok(())
    }
}
