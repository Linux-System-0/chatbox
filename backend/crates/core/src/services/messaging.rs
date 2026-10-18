//! Direct and group messaging, history, read cursors, and catch-up.

use std::collections::HashMap;

use super::{AuthContext, Services, events::sync_frame, normalize_text};
use crate::error::{Error, Result};
use crate::model::{
    Conversation, ConversationKind, ConversationMember, Message, MessageKind, OutboxEvent,
};
use crate::protocol::{ServerFrame, SyncScope};

/// A send request from a client.
#[derive(Debug, Clone)]
pub struct SendRequest {
    /// Target conversation.
    pub conversation_id: i64,
    /// Client idempotency key.
    pub client_msg_id: String,
    /// Payload kind.
    pub kind: MessageKind,
    /// Text body.
    pub content: Option<String>,
    /// Attachment id for file messages.
    pub attachment_id: Option<i64>,
}

/// A conversation summary for the sidebar.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationView {
    /// Conversation id.
    pub id: i64,
    /// Direct or group.
    pub kind: ConversationKind,
    /// Human title (group name or the contact-aware direct title).
    pub title: String,
    /// Resolved avatar for a direct conversation.
    pub avatar_url: Option<String>,
    /// The other participant in a direct conversation.
    pub peer_id: Option<i64>,
    /// Backing group id for a group conversation.
    pub group_id: Option<i64>,
    /// Number of conversation members.
    pub member_count: u32,
    /// The newest message, if any.
    pub last_message: Option<Message>,
    /// The caller's read cursor.
    pub last_read_seq: u64,
    /// Read cursors for every member, used for per-message read receipts.
    pub read_cursors: Vec<ReadCursorView>,
    /// Number of unread messages.
    pub unread: u64,
}

/// A member's read position within a conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReadCursorView {
    /// Member user id.
    pub user_id: i64,
    /// Highest sequence the member has read.
    pub last_read_seq: u64,
}

struct ConversationIdentity {
    title: String,
    avatar_url: Option<String>,
    peer_id: Option<i64>,
    group_id: Option<i64>,
    member_count: u32,
}

impl Services {
    /// Get or create the direct conversation between the caller and another
    /// user.
    pub async fn get_or_create_direct(
        &self,
        ctx: &AuthContext,
        other_user_id: &str,
    ) -> Result<Conversation> {
        let other = self.get_user_by_user_id(other_user_id).await?;
        if other.id == ctx.user_id {
            return Err(Error::Invalid(
                "cannot start a direct chat with yourself".into(),
            ));
        }
        if !self.are_contacts(ctx.user_id, other.id).await? {
            return Err(Error::Forbidden(
                "add this user as a contact before starting a chat".into(),
            ));
        }
        if let Some(existing) = self
            .stores
            .conversations
            .find_direct(ctx.user_id, other.id)
            .await?
        {
            return Ok(existing);
        }
        let now = self.now();
        let conversation = Conversation {
            id: self.new_id(),
            kind: ConversationKind::Direct,
            created_at: now,
        };
        self.stores.conversations.create(&conversation).await?;
        for user_id in [ctx.user_id, other.id] {
            self.stores
                .conversations
                .add_member(&ConversationMember {
                    conversation_id: conversation.id,
                    user_id,
                    last_read_seq: 0,
                    muted_until: None,
                    joined_at: now,
                })
                .await?;
        }
        let frame = sync_frame(&[SyncScope::Conversations], Some(conversation.id), None);
        if let Err(e) = self.notify_users(&[ctx.user_id, other.id], &frame).await {
            tracing::warn!(conversation_id = conversation.id, error = %e, "conversation sync notification failed");
        }
        Ok(conversation)
    }

    /// Send a message. Idempotent on `(sender, client_msg_id)`.
    pub async fn send_message(&self, ctx: &AuthContext, req: SendRequest) -> Result<Message> {
        if req.client_msg_id.trim().is_empty() {
            return Err(Error::Invalid("client_msg_id is required".into()));
        }
        if let Some(existing) = self
            .stores
            .messages
            .find_by_client_id(ctx.user_id, &req.client_msg_id)
            .await?
        {
            return Ok(existing);
        }

        let conversation = self
            .stores
            .conversations
            .get(req.conversation_id)
            .await?
            .ok_or(Error::NotFound)?;
        let member = self
            .stores
            .conversations
            .get_member(req.conversation_id, ctx.user_id)
            .await?
            .ok_or(Error::Forbidden("not a member of this conversation".into()))?;

        if conversation.kind == ConversationKind::Direct {
            let members = self
                .stores
                .conversations
                .list_members(req.conversation_id)
                .await?;
            if let Some(other) = members.iter().find(|m| m.user_id != ctx.user_id) {
                if !self.are_contacts(ctx.user_id, other.user_id).await? {
                    return Err(Error::Forbidden(
                        "direct messaging requires an accepted contact".into(),
                    ));
                }
                if self.is_blocked_between(ctx.user_id, other.user_id).await? {
                    return Err(Error::Forbidden("direct messaging is blocked".into()));
                }
            }
        }

        let now = self.now();
        if let Some(until) = member.muted_until
            && until > now
        {
            return Err(Error::Forbidden(
                "you are muted in this conversation".into(),
            ));
        }

        let (content, mentions) = self.prepare_payload(ctx, &conversation, &req).await?;

        // A mutex-style lock per conversation keeps sequence assignment
        // contiguous under concurrent senders.
        let seq = self.stores.messages.next_seq(req.conversation_id).await?;
        let message = Message {
            id: self.new_id(),
            conversation_id: req.conversation_id,
            seq,
            sender_id: ctx.user_id,
            client_msg_id: req.client_msg_id,
            kind: req.kind,
            content,
            attachment_id: req.attachment_id,
            mentions,
            created_at: now,
            edited_at: None,
            deleted_at: None,
        };
        let payload = serde_json::to_string(&message).map_err(Error::storage)?;
        let outbox = OutboxEvent {
            id: self.new_id(),
            message_id: message.id,
            conversation_id: message.conversation_id,
            payload,
            published_at: None,
            created_at: now,
        };
        self.stores.messages.append(&message, &outbox).await?;
        // Sender implicitly has read their own message.
        self.stores
            .conversations
            .set_read_cursor(req.conversation_id, ctx.user_id, seq)
            .await?;
        Ok(message)
    }

    async fn prepare_payload(
        &self,
        ctx: &AuthContext,
        conversation: &Conversation,
        req: &SendRequest,
    ) -> Result<(Option<String>, Vec<i64>)> {
        match req.kind {
            MessageKind::Text => {
                let raw = req
                    .content
                    .as_deref()
                    .ok_or_else(|| Error::Invalid("text message requires content".into()))?;
                let text = normalize_text(raw, self.config.max_message_len)?;
                let mentions = if conversation.kind == ConversationKind::Group {
                    self.resolve_mentions(&text, conversation.id).await?
                } else {
                    Vec::new()
                };
                Ok((Some(text), mentions))
            }
            MessageKind::File => {
                let attachment_id = req
                    .attachment_id
                    .ok_or_else(|| Error::Invalid("file message requires an attachment".into()))?;
                let attachment = self
                    .stores
                    .attachments
                    .get(attachment_id)
                    .await?
                    .ok_or_else(|| Error::Invalid("unknown attachment".into()))?;
                if attachment.owner_id != ctx.user_id {
                    return Err(Error::Forbidden(
                        "attachment belongs to another user".into(),
                    ));
                }
                Ok((req.content.clone(), Vec::new()))
            }
            MessageKind::System => Err(Error::Forbidden(
                "system messages cannot be sent by clients".into(),
            )),
        }
    }

    /// Resolve `@user_id` mentions against the group's members.
    async fn resolve_mentions(&self, text: &str, conversation_id: i64) -> Result<Vec<i64>> {
        let mut mentioned = Vec::new();
        for token in text.split_whitespace() {
            if let Some(name) = token.strip_prefix('@') {
                let name = name.trim_end_matches(|c: char| {
                    !c.is_alphanumeric() && c != '_' && c != '-' && c != '.'
                });
                if name.is_empty() {
                    continue;
                }
                if let Some(user) = self.stores.users.get_by_user_id(name).await? {
                    let is_member = self
                        .stores
                        .conversations
                        .get_member(conversation_id, user.id)
                        .await?
                        .is_some();
                    if is_member && !mentioned.contains(&user.id) {
                        mentioned.push(user.id);
                    }
                }
            }
        }
        Ok(mentioned)
    }

    /// List message history anchored around a cursor.
    pub async fn list_messages(
        &self,
        ctx: &AuthContext,
        conversation_id: i64,
        before: Option<u64>,
        after: Option<u64>,
        limit: Option<u32>,
    ) -> Result<Vec<Message>> {
        self.require_member(conversation_id, ctx.user_id).await?;
        if before.is_some() && after.is_some() {
            return Err(Error::Invalid(
                "before and after are mutually exclusive".into(),
            ));
        }
        let limit = self.page_size(limit);
        self.stores
            .messages
            .list(conversation_id, before, after, limit)
            .await
    }

    /// Advance the caller's read cursor.
    pub async fn mark_read(&self, ctx: &AuthContext, conversation_id: i64, seq: u64) -> Result<()> {
        let member = self.require_member(conversation_id, ctx.user_id).await?;
        if seq <= member.last_read_seq {
            return Ok(());
        }
        self.stores
            .conversations
            .set_read_cursor(conversation_id, ctx.user_id, seq)
            .await?;
        // Notify every participant (including the reader's other devices) so
        // senders can flip their read receipts without polling.
        let frame = ServerFrame::Read {
            conversation_id,
            user_id: ctx.user_id,
            seq,
        };
        if let Err(e) = self.notify_conversation(conversation_id, &frame).await {
            tracing::warn!(conversation_id, error = %e, "read receipt notification failed");
        }
        Ok(())
    }

    /// Summaries of the caller's conversations.
    pub async fn list_conversations(&self, ctx: &AuthContext) -> Result<Vec<ConversationView>> {
        let memberships = self.stores.conversations.list_for_user(ctx.user_id).await?;
        let mut views = Vec::with_capacity(memberships.len());
        for membership in memberships {
            let conversation = match self
                .stores
                .conversations
                .get(membership.conversation_id)
                .await?
            {
                Some(c) => c,
                None => continue,
            };
            let title = self
                .conversation_identity(&conversation, ctx.user_id)
                .await?;
            let read_cursors = self
                .stores
                .conversations
                .list_members(conversation.id)
                .await?
                .into_iter()
                .map(|member| ReadCursorView {
                    user_id: member.user_id,
                    last_read_seq: member.last_read_seq,
                })
                .collect();
            let last = self
                .stores
                .messages
                .list(conversation.id, None, None, 1)
                .await?
                .into_iter()
                .next_back();
            let unread = self
                .stores
                .messages
                .count_after(conversation.id, membership.last_read_seq)
                .await?;
            views.push(ConversationView {
                id: conversation.id,
                kind: conversation.kind,
                title: title.title,
                avatar_url: title.avatar_url,
                peer_id: title.peer_id,
                group_id: title.group_id,
                member_count: title.member_count,
                last_message: last,
                last_read_seq: membership.last_read_seq,
                read_cursors,
                unread,
            });
        }
        views.sort_by(|a, b| {
            let a_ts = a.last_message.as_ref().map(|m| m.created_at);
            let b_ts = b.last_message.as_ref().map(|m| m.created_at);
            b_ts.cmp(&a_ts)
        });
        Ok(views)
    }

    async fn conversation_identity(
        &self,
        conversation: &Conversation,
        viewer: i64,
    ) -> Result<ConversationIdentity> {
        let members = self
            .stores
            .conversations
            .list_members(conversation.id)
            .await?;
        let member_count = members.len() as u32;
        match conversation.kind {
            ConversationKind::Group => {
                let group = self
                    .stores
                    .groups
                    .get_by_conversation(conversation.id)
                    .await?;
                let remark = match group.as_ref() {
                    Some(group) => self
                        .stores
                        .groups
                        .get_member(group.id, viewer)
                        .await?
                        .and_then(|member| member.remark)
                        .filter(|remark| !remark.trim().is_empty()),
                    None => None,
                };
                Ok(ConversationIdentity {
                    title: remark
                        .or_else(|| group.as_ref().map(|g| g.name.clone()))
                        .unwrap_or_else(|| "Group".to_string()),
                    avatar_url: None,
                    peer_id: None,
                    group_id: group.as_ref().map(|g| g.id),
                    member_count,
                })
            }
            ConversationKind::Direct => {
                for member in members {
                    if member.user_id == viewer {
                        continue;
                    }
                    if let Some(user) = self.stores.users.get(member.user_id).await? {
                        let contact = self.stores.contacts.get(viewer, user.id).await?;
                        let title = contact
                            .as_ref()
                            .and_then(|c| c.remark.clone())
                            .filter(|r| !r.trim().is_empty())
                            .unwrap_or_else(|| user.effective_display_name().to_string());
                        let avatar_url = contact
                            .and_then(|c| c.remark_avatar_url)
                            .or_else(|| user.avatar_url.clone());
                        return Ok(ConversationIdentity {
                            title,
                            avatar_url,
                            peer_id: Some(user.id),
                            group_id: None,
                            member_count,
                        });
                    }
                }
                Ok(ConversationIdentity {
                    title: "Direct".to_string(),
                    avatar_url: None,
                    peer_id: None,
                    group_id: None,
                    member_count,
                })
            }
        }
    }

    /// Catch up on missed messages after a reconnect.
    pub async fn resume(
        &self,
        ctx: &AuthContext,
        cursors: &HashMap<i64, u64>,
    ) -> Result<Vec<Message>> {
        let mut out = Vec::new();
        for (conversation_id, cursor) in cursors {
            let member = self
                .stores
                .conversations
                .get_member(*conversation_id, ctx.user_id)
                .await?;
            if member.is_none() {
                continue;
            }
            let mut batch = self
                .stores
                .messages
                .list(*conversation_id, None, Some(*cursor), self.page_size(None))
                .await?;
            out.append(&mut batch);
        }
        out.sort_by_key(|m| m.created_at);
        Ok(out)
    }

    /// Ensure the user is a member of the conversation.
    pub(crate) async fn require_member(
        &self,
        conversation_id: i64,
        user_id: i64,
    ) -> Result<ConversationMember> {
        self.stores
            .conversations
            .get_member(conversation_id, user_id)
            .await?
            .ok_or(Error::Forbidden("not a member of this conversation".into()))
    }

    /// The two participants of a direct conversation.
    pub async fn direct_participants(&self, conversation_id: i64) -> Result<Vec<i64>> {
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
