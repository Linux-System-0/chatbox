//! Direct and group messaging, history, read cursors, and catch-up.

use std::collections::HashMap;

use super::{AuthContext, Services, normalize_text};
use crate::error::{Error, Result};
use crate::model::{
    Conversation, ConversationKind, ConversationMember, Message, MessageKind, OutboxEvent,
};

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
    /// Human title (group name or the other user's display name).
    pub title: String,
    /// The newest message, if any.
    pub last_message: Option<Message>,
    /// The caller's read cursor.
    pub last_read_seq: u64,
    /// Number of unread messages.
    pub unread: u64,
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
            if let Some(other) = members.iter().find(|m| m.user_id != ctx.user_id)
                && self.is_blocked_between(ctx.user_id, other.user_id).await?
            {
                return Err(Error::Forbidden("direct messaging is blocked".into()));
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
        self.require_member(conversation_id, ctx.user_id).await?;
        self.stores
            .conversations
            .set_read_cursor(conversation_id, ctx.user_id, seq)
            .await?;
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
            let title = self.conversation_title(&conversation, ctx.user_id).await?;
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
                title,
                last_message: last,
                last_read_seq: membership.last_read_seq,
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

    async fn conversation_title(&self, conversation: &Conversation, viewer: i64) -> Result<String> {
        match conversation.kind {
            ConversationKind::Group => Ok(self
                .stores
                .groups
                .get_by_conversation(conversation.id)
                .await?
                .map(|g| g.name)
                .unwrap_or_else(|| "Group".to_string())),
            ConversationKind::Direct => {
                let members = self
                    .stores
                    .conversations
                    .list_members(conversation.id)
                    .await?;
                for m in members {
                    if m.user_id != viewer
                        && let Some(user) = self.stores.users.get(m.user_id).await?
                    {
                        return Ok(user.effective_display_name().to_string());
                    }
                }
                Ok("Direct".to_string())
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
