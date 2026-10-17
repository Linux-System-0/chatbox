//! Group chat: roles, membership, permissions, and lifecycle.

use super::{AuthContext, Services, events::sync_frame, normalize_optional_text};
use crate::error::{Error, Result};
use crate::model::{
    Conversation, ConversationKind, ConversationMember, Group, GroupMember, GroupRole, Timestamp,
    User,
};
use crate::protocol::SyncScope;

/// A group member with profile and role.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupMemberView {
    /// Member profile.
    pub user: User,
    /// Role.
    pub role: GroupRole,
    /// Private alias this member uses for the group.
    ///
    /// Only populated when the member is the caller; another member's alias is
    /// private to them.
    pub remark: Option<String>,
    /// Mute deadline, if muted.
    pub muted_until: Option<Timestamp>,
    /// Join time.
    pub joined_at: Timestamp,
}

/// A group with its members.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupView {
    /// Group metadata.
    pub group: Group,
    /// Members ordered by role then join time.
    pub members: Vec<GroupMemberView>,
}

fn rank(role: GroupRole) -> u8 {
    match role {
        GroupRole::Owner => 3,
        GroupRole::Admin => 2,
        GroupRole::Member => 1,
    }
}

impl Services {
    /// Create a group. The creator becomes the owner.
    pub async fn create_group(
        &self,
        ctx: &AuthContext,
        name: &str,
        member_ids: &[String],
    ) -> Result<GroupView> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > self.config.max_group_name_len {
            return Err(Error::Invalid("invalid group name".into()));
        }
        let now = self.now();
        let conversation = Conversation {
            id: self.new_id(),
            kind: ConversationKind::Group,
            created_at: now,
        };
        self.stores.conversations.create(&conversation).await?;
        let group = Group {
            id: self.new_id(),
            conversation_id: conversation.id,
            name: name.to_string(),
            owner_id: ctx.user_id,
            invite_policy: GroupRole::Admin,
            created_at: now,
        };
        self.stores.groups.create(&group).await?;

        let mut member_user_ids = vec![ctx.user_id];
        for id in member_ids {
            if let Some(user) = self.stores.users.get_by_user_id(id).await?
                && !member_user_ids.contains(&user.id)
            {
                member_user_ids.push(user.id);
            }
        }
        for (index, user_id) in member_user_ids.iter().enumerate() {
            let role = if index == 0 {
                GroupRole::Owner
            } else {
                GroupRole::Member
            };
            self.add_member_internal(group.id, *user_id, role, now)
                .await?;
        }
        let view = self.get_group(ctx, group.id).await?;
        self.sync_group(&view, &[SyncScope::Conversations, SyncScope::Groups])
            .await;
        Ok(view)
    }

    async fn sync_group(&self, view: &GroupView, scopes: &[SyncScope]) {
        let user_ids: Vec<i64> = view.members.iter().map(|member| member.user.id).collect();
        let frame = sync_frame(
            scopes,
            Some(view.group.conversation_id),
            Some(view.group.id),
        );
        if let Err(e) = self.notify_users(&user_ids, &frame).await {
            tracing::warn!(group_id = view.group.id, error = %e, "group sync notification failed");
        }
    }

    async fn add_member_internal(
        &self,
        group_id: i64,
        user_id: i64,
        role: GroupRole,
        now: Timestamp,
    ) -> Result<()> {
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        self.stores
            .groups
            .add_member(&GroupMember {
                group_id,
                user_id,
                role,
                remark: None,
                muted_until: None,
                joined_at: now,
            })
            .await?;
        // Every group member is also a conversation member.
        if self
            .stores
            .conversations
            .get_member(group.conversation_id, user_id)
            .await?
            .is_none()
        {
            self.stores
                .conversations
                .add_member(&ConversationMember {
                    conversation_id: group.conversation_id,
                    user_id,
                    last_read_seq: 0,
                    muted_until: None,
                    joined_at: now,
                })
                .await?;
        }
        Ok(())
    }

    /// Fetch a group and its members. Only members may read it.
    pub async fn get_group(&self, ctx: &AuthContext, group_id: i64) -> Result<GroupView> {
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        self.require_member(group.conversation_id, ctx.user_id)
            .await?;
        let mut members = Vec::new();
        for m in self.stores.groups.list_members(group_id).await? {
            if let Some(user) = self.stores.users.get(m.user_id).await? {
                members.push(GroupMemberView {
                    user,
                    role: m.role,
                    remark: if m.user_id == ctx.user_id {
                        m.remark
                    } else {
                        None
                    },
                    muted_until: m.muted_until,
                    joined_at: m.joined_at,
                });
            }
        }
        members.sort_by(|a, b| {
            rank(b.role)
                .cmp(&rank(a.role))
                .then_with(|| a.joined_at.cmp(&b.joined_at))
        });
        Ok(GroupView { group, members })
    }

    async fn actor_member(&self, group_id: i64, user_id: i64) -> Result<GroupMember> {
        self.stores
            .groups
            .get_member(group_id, user_id)
            .await?
            .ok_or(Error::Forbidden("not a group member".into()))
    }

    /// Rename a group (owner or admin).
    pub async fn rename_group(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        name: &str,
    ) -> Result<Group> {
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        if !actor.role.is_admin() {
            return Err(Error::Forbidden("admin role required".into()));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > self.config.max_group_name_len {
            return Err(Error::Invalid("invalid group name".into()));
        }
        let mut group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        group.name = name.to_string();
        self.stores.groups.update(&group).await?;
        let view = self.get_group(ctx, group_id).await?;
        self.sync_group(&view, &[SyncScope::Conversations, SyncScope::Groups])
            .await;
        Ok(group)
    }

    /// Set who may invite members (owner or admin).
    pub async fn set_invite_policy(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        policy: GroupRole,
    ) -> Result<Group> {
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        if !actor.role.is_admin() {
            return Err(Error::Forbidden("admin role required".into()));
        }
        let mut group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        group.invite_policy = policy;
        self.stores.groups.update(&group).await?;
        let view = self.get_group(ctx, group_id).await?;
        self.sync_group(&view, &[SyncScope::Groups]).await;
        Ok(group)
    }

    /// Add members to a group, honoring the invite policy.
    pub async fn add_group_members(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        member_ids: &[String],
    ) -> Result<GroupView> {
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        if rank(actor.role) < rank(group.invite_policy) && actor.role != GroupRole::Owner {
            return Err(Error::Forbidden("not allowed to invite members".into()));
        }
        let now = self.now();
        for id in member_ids {
            let user = self.get_user_by_user_id(id).await?;
            if self
                .stores
                .groups
                .get_member(group_id, user.id)
                .await?
                .is_some()
            {
                continue;
            }
            self.add_member_internal(group_id, user.id, GroupRole::Member, now)
                .await?;
        }
        let view = self.get_group(ctx, group_id).await?;
        self.sync_group(&view, &[SyncScope::Conversations, SyncScope::Groups])
            .await;
        Ok(view)
    }

    /// Kick a member (owner or admin).
    pub async fn kick_group_member(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        user_id_str: &str,
    ) -> Result<()> {
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        let target = self.get_user_by_user_id(user_id_str).await?;
        let target_member = self
            .stores
            .groups
            .get_member(group_id, target.id)
            .await?
            .ok_or(Error::NotFound)?;
        if target_member.role == GroupRole::Owner {
            return Err(Error::Forbidden("cannot kick the owner".into()));
        }
        if rank(actor.role) <= rank(target_member.role) {
            return Err(Error::Forbidden(
                "cannot kick an equal or higher role".into(),
            ));
        }
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        self.remove_member_internal(group_id, target.id).await?;
        let mut audience: Vec<i64> = self
            .stores
            .conversations
            .list_members(group.conversation_id)
            .await?
            .into_iter()
            .map(|member| member.user_id)
            .collect();
        audience.push(target.id);
        let frame = sync_frame(
            &[SyncScope::Conversations, SyncScope::Groups],
            Some(group.conversation_id),
            Some(group_id),
        );
        if let Err(e) = self.notify_users(&audience, &frame).await {
            tracing::warn!(group_id, error = %e, "group sync notification failed");
        }
        Ok(())
    }

    /// Mute or unmute a member (owner or admin).
    pub async fn mute_group_member(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        user_id_str: &str,
        until: Option<Timestamp>,
    ) -> Result<()> {
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        let target = self.get_user_by_user_id(user_id_str).await?;
        let mut target_member = self
            .stores
            .groups
            .get_member(group_id, target.id)
            .await?
            .ok_or(Error::NotFound)?;
        if target_member.role == GroupRole::Owner {
            return Err(Error::Forbidden("cannot mute the owner".into()));
        }
        if rank(actor.role) <= rank(target_member.role) {
            return Err(Error::Forbidden(
                "cannot mute an equal or higher role".into(),
            ));
        }
        target_member.muted_until = until;
        self.stores.groups.update_member(&target_member).await?;
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        self.stores
            .conversations
            .set_mute(group.conversation_id, target.id, until)
            .await?;
        let view = self.get_group(ctx, group_id).await?;
        self.sync_group(&view, &[SyncScope::Groups]).await;
        Ok(())
    }

    /// Transfer ownership (owner only).
    pub async fn transfer_ownership(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        new_owner_id: &str,
    ) -> Result<Group> {
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        if actor.role != GroupRole::Owner {
            return Err(Error::Forbidden("owner role required".into()));
        }
        let new_owner = self.get_user_by_user_id(new_owner_id).await?;
        if new_owner.id == ctx.user_id {
            return Err(Error::Invalid("already the owner".into()));
        }
        let mut member = self
            .stores
            .groups
            .get_member(group_id, new_owner.id)
            .await?
            .ok_or(Error::NotFound)?;
        member.role = GroupRole::Owner;
        self.stores.groups.update_member(&member).await?;
        let mut old = actor;
        old.role = GroupRole::Admin;
        self.stores.groups.update_member(&old).await?;
        self.stores.groups.set_owner(group_id, new_owner.id).await?;
        let view = self.get_group(ctx, group_id).await?;
        self.sync_group(&view, &[SyncScope::Conversations, SyncScope::Groups])
            .await;
        self.stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)
    }

    /// Leave a group. The owner must transfer ownership or dissolve first.
    pub async fn leave_group(&self, ctx: &AuthContext, group_id: i64) -> Result<()> {
        let member = self.actor_member(group_id, ctx.user_id).await?;
        if member.role == GroupRole::Owner {
            let members = self.stores.groups.list_members(group_id).await?;
            if members.len() > 1 {
                return Err(Error::Forbidden(
                    "transfer ownership or dissolve the group before leaving".into(),
                ));
            }
        }
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        let mut audience: Vec<i64> = self
            .stores
            .groups
            .list_members(group_id)
            .await?
            .into_iter()
            .map(|member| member.user_id)
            .collect();
        self.remove_member_internal(group_id, ctx.user_id).await?;
        if !audience.contains(&ctx.user_id) {
            audience.push(ctx.user_id);
        }
        let frame = sync_frame(
            &[SyncScope::Conversations, SyncScope::Groups],
            Some(group.conversation_id),
            Some(group_id),
        );
        if let Err(e) = self.notify_users(&audience, &frame).await {
            tracing::warn!(group_id, error = %e, "group sync notification failed");
        }
        Ok(())
    }

    async fn remove_member_internal(&self, group_id: i64, user_id: i64) -> Result<()> {
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        self.stores.groups.remove_member(group_id, user_id).await?;
        self.stores
            .conversations
            .remove_member(group.conversation_id, user_id)
            .await?;
        Ok(())
    }

    /// Dissolve a group (owner only).
    pub async fn dissolve_group(&self, ctx: &AuthContext, group_id: i64) -> Result<()> {
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        if actor.role != GroupRole::Owner {
            return Err(Error::Forbidden("owner role required".into()));
        }
        let group = self
            .stores
            .groups
            .get(group_id)
            .await?
            .ok_or(Error::NotFound)?;
        let audience: Vec<i64> = self
            .stores
            .groups
            .list_members(group_id)
            .await?
            .into_iter()
            .map(|member| member.user_id)
            .collect();
        for member in self.stores.groups.list_members(group_id).await? {
            self.stores
                .conversations
                .remove_member(group.conversation_id, member.user_id)
                .await?;
        }
        self.stores.groups.delete(group_id).await?;
        let frame = sync_frame(
            &[SyncScope::Conversations, SyncScope::Groups],
            Some(group.conversation_id),
            Some(group_id),
        );
        if let Err(e) = self.notify_users(&audience, &frame).await {
            tracing::warn!(group_id, error = %e, "group sync notification failed");
        }
        Ok(())
    }

    /// Promote or demote a group administrator (owner only).
    pub async fn set_group_member_role(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        user_id_str: &str,
        role: GroupRole,
    ) -> Result<GroupView> {
        if role == GroupRole::Owner {
            return Err(Error::Invalid(
                "use ownership transfer to assign the owner role".into(),
            ));
        }
        let actor = self.actor_member(group_id, ctx.user_id).await?;
        if actor.role != GroupRole::Owner {
            return Err(Error::Forbidden("owner role required".into()));
        }
        let target = self.get_user_by_user_id(user_id_str).await?;
        let mut target_member = self
            .stores
            .groups
            .get_member(group_id, target.id)
            .await?
            .ok_or(Error::NotFound)?;
        if target_member.role == GroupRole::Owner {
            return Err(Error::Forbidden("cannot change the owner role".into()));
        }
        target_member.role = role;
        self.stores.groups.update_member(&target_member).await?;
        let view = self.get_group(ctx, group_id).await?;
        self.sync_group(&view, &[SyncScope::Groups]).await;
        Ok(view)
    }

    /// Set the caller's private alias for a group.
    pub async fn set_group_remark(
        &self,
        ctx: &AuthContext,
        group_id: i64,
        remark: Option<String>,
    ) -> Result<GroupView> {
        let mut member = self.actor_member(group_id, ctx.user_id).await?;
        member.remark = normalize_optional_text(remark, self.config.max_group_name_len)?;
        self.stores.groups.update_member(&member).await?;
        let view = self.get_group(ctx, group_id).await?;
        let frame = sync_frame(
            &[SyncScope::Conversations, SyncScope::Groups],
            Some(view.group.conversation_id),
            Some(group_id),
        );
        if let Err(e) = self.notify_users(&[ctx.user_id], &frame).await {
            tracing::warn!(group_id, error = %e, "group sync notification failed");
        }
        Ok(view)
    }

    /// The set of user ids in a group's conversation (used by fan-out).
    pub async fn group_recipients(&self, conversation_id: i64) -> Result<Vec<i64>> {
        Ok(self
            .stores
            .conversations
            .list_members(conversation_id)
            .await?
            .into_iter()
            .map(|m| m.user_id)
            .collect())
    }
}
