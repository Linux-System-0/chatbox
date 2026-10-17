//! Contacts and friend requests.

use std::collections::HashSet;

use super::{AuthContext, Services, events::sync_frame, normalize_avatar, normalize_optional_text};
use crate::error::{Error, Result};
use crate::model::{Contact, FriendRequest, FriendRequestStatus, User};
use crate::protocol::SyncScope;

/// A contact together with the contact's profile.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContactView {
    /// The contact user.
    pub user: User,
    /// The per-contact remark.
    pub remark: Option<String>,
    /// The per-contact avatar chosen by the contact-list owner.
    pub remark_avatar_url: Option<String>,
    /// When the contact was added.
    pub created_at: crate::model::Timestamp,
}

/// A friend request with both participants' profiles.
///
/// The request fields stay flat in JSON so existing clients keep working; the
/// extra profiles let new clients render names instead of internal numeric ids.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FriendRequestView {
    /// The underlying request.
    #[serde(flatten)]
    pub request: FriendRequest,
    /// Requesting user's profile, when available.
    pub from_user: Option<User>,
    /// Target user's profile, when available.
    pub to_user: Option<User>,
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
        self.notify_contact_change(
            &[request.from_id, request.to_id],
            &[SyncScope::Requests],
            None,
        )
        .await;
        Ok(request)
    }

    async fn notify_contact_change(
        &self,
        user_ids: &[i64],
        scopes: &[SyncScope],
        conversation_id: Option<i64>,
    ) {
        let frame = sync_frame(scopes, conversation_id, None);
        if let Err(e) = self.notify_users(user_ids, &frame).await {
            tracing::warn!(error = %e, "contact sync notification failed");
        }
    }

    /// List requests sent to or by the caller, with participant profiles.
    pub async fn list_friend_requests(&self, ctx: &AuthContext) -> Result<Vec<FriendRequestView>> {
        let requests = self
            .stores
            .friend_requests
            .list_for_user(ctx.user_id)
            .await?;
        let mut views = Vec::with_capacity(requests.len());
        for request in requests {
            views.push(FriendRequestView {
                from_user: self.stores.users.get(request.from_id).await?,
                to_user: self.stores.users.get(request.to_id).await?,
                request,
            });
        }
        Ok(views)
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
                remark_avatar_url: None,
                created_at: now,
            })
            .await?;
        self.stores
            .contacts
            .upsert(&Contact {
                owner_id: request.to_id,
                contact_id: request.from_id,
                remark: None,
                remark_avatar_url: None,
                created_at: now,
            })
            .await?;
        self.notify_contact_change(
            &[request.from_id, request.to_id],
            &[SyncScope::Requests, SyncScope::Contacts],
            None,
        )
        .await;
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
        self.notify_contact_change(
            &[request.from_id, request.to_id],
            &[SyncScope::Requests],
            None,
        )
        .await;
        Ok(request)
    }

    async fn load_request(&self, _ctx: &AuthContext, request_id: i64) -> Result<FriendRequest> {
        self.stores
            .friend_requests
            .get(request_id)
            .await?
            .ok_or(Error::NotFound)
    }

    /// Whether two users are accepted contacts of each other.
    pub async fn are_contacts(&self, a: i64, b: i64) -> Result<bool> {
        Ok(self.stores.contacts.get(a, b).await?.is_some()
            && self.stores.contacts.get(b, a).await?.is_some())
    }

    /// List the caller's contacts with profiles.
    ///
    /// Contacts the caller has blocked are excluded; they are only visible in
    /// the block list until they are unblocked.
    pub async fn list_contacts(&self, ctx: &AuthContext) -> Result<Vec<ContactView>> {
        let contacts = self.stores.contacts.list(ctx.user_id).await?;
        let blocked: HashSet<i64> = self
            .stores
            .blocks
            .list(ctx.user_id)
            .await?
            .into_iter()
            .map(|block| block.blocked_id)
            .collect();
        let mut views = Vec::with_capacity(contacts.len());
        for contact in contacts {
            if blocked.contains(&contact.contact_id) {
                continue;
            }
            if let Some(user) = self.stores.users.get(contact.contact_id).await? {
                views.push(ContactView {
                    user,
                    remark: contact.remark,
                    remark_avatar_url: contact.remark_avatar_url,
                    created_at: contact.created_at,
                });
            }
        }
        Ok(views)
    }

    /// Set a per-contact remark and/or avatar.
    ///
    /// `None` leaves a field unchanged; `Some(None)` or an empty string clears
    /// it.
    pub async fn set_contact_remark(
        &self,
        ctx: &AuthContext,
        contact_user_id: &str,
        remark: Option<Option<String>>,
        remark_avatar_url: Option<Option<String>>,
    ) -> Result<ContactView> {
        let target = self.get_user_by_user_id(contact_user_id).await?;
        let mut contact = self
            .stores
            .contacts
            .get(ctx.user_id, target.id)
            .await?
            .ok_or(Error::NotFound)?;
        if let Some(value) = remark {
            contact.remark = normalize_optional_text(value, 64)?;
        }
        if let Some(value) = remark_avatar_url {
            contact.remark_avatar_url = normalize_avatar(value)?;
        }
        self.stores
            .contacts
            .set_remark(
                ctx.user_id,
                target.id,
                contact.remark.clone(),
                contact.remark_avatar_url.clone(),
            )
            .await?;
        self.notify_contact_change(
            &[ctx.user_id],
            &[SyncScope::Contacts, SyncScope::Conversations],
            None,
        )
        .await;
        Ok(ContactView {
            user: target,
            remark: contact.remark,
            remark_avatar_url: contact.remark_avatar_url,
            created_at: contact.created_at,
        })
    }

    /// Remove a contact in both directions.
    pub async fn remove_contact(&self, ctx: &AuthContext, contact_user_id: &str) -> Result<()> {
        let target = self.get_user_by_user_id(contact_user_id).await?;
        self.stores.contacts.delete(ctx.user_id, target.id).await?;
        self.notify_contact_change(
            &[ctx.user_id, target.id],
            &[SyncScope::Contacts, SyncScope::Conversations],
            None,
        )
        .await;
        Ok(())
    }
}
