//! Group chat: roles, membership, permissions, and lifecycle.

use super::{AuthContext, Services};
use crate::error::{Error, Result};
use crate::model::{
    Conversation, ConversationKind, ConversationMember, Group, GroupMember, GroupRole, Timestamp,
    User,
};

/// A group member with profile and role.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupMemberView {
    /// Member profile.
    pub user: User,
    /// Role.
    pub role: GroupRole,
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
            if let Some(user) = self.stores.users.get_by_user_id(id).await? {
                if !member_user_ids.contains(&user.id) {
                    member_user_ids.push(user.id);
                }
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
        self.get_group(ctx, group.id).await
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
        self.get_group(ctx, group_id).await
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
        self.remove_member_internal(group_id, target.id).await
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
        self.remove_member_internal(group_id, ctx.user_id).await
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
        for member in self.stores.groups.list_members(group_id).await? {
            self.stores
                .conversations
                .remove_member(group.conversation_id, member.user_id)
                .await?;
        }
        self.stores.groups.delete(group_id).await?;
        Ok(())
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
