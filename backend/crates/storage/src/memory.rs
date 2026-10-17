//! In-memory storage and infrastructure adapters.
//!
//! These adapters implement every core port over shared, process-local state.
//! They power the test suite and the development backend selected when no
//! `DATABASE_URL` is configured. State is not persisted across restarts.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use chatbox_core::infra::{
    BusMessage, Cache, ConnectionInfo, ConnectionRegistry, MessageBus, ObjectStorage,
    PresenceStore, RateLimiter,
};
use chatbox_core::model::*;
use chatbox_core::port::*;
use tokio::sync::broadcast;

/// The shared, mutable state behind every in-memory adapter.
#[derive(Default)]
struct MemoryState {
    users: HashMap<i64, User>,
    user_ids: HashMap<String, i64>,
    devices: HashMap<i64, Device>,
    sessions: HashMap<String, Session>,
    conversations: HashMap<i64, Conversation>,
    conv_members: HashMap<(i64, i64), ConversationMember>,
    direct_index: HashMap<(i64, i64), i64>,
    messages: HashMap<i64, Message>,
    msg_client: HashMap<(i64, String), i64>,
    seqs: HashMap<i64, u64>,
    contacts: HashMap<(i64, i64), Contact>,
    friend_requests: HashMap<i64, FriendRequest>,
    groups: HashMap<i64, Group>,
    group_members: HashMap<(i64, i64), GroupMember>,
    blocks: HashSet<(i64, i64)>,
    attachments: HashMap<i64, Attachment>,
    outbox: BTreeMap<i64, OutboxEvent>,
    plugin_states: HashMap<String, bool>,
    cache: HashMap<String, (String, Instant)>,
    presence: HashSet<i64>,
    registry: HashMap<i64, Vec<ConnectionInfo>>,
    rate: HashMap<String, (Instant, u32)>,
    objects: HashMap<String, Vec<u8>>,
}

impl MemoryState {
    fn direct_key(a: i64, b: i64) -> (i64, i64) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
}

/// A process-local adapter implementing all repository and infrastructure
/// ports over one shared state cell.
pub struct MemoryStore {
    state: Mutex<MemoryState>,
    bus: broadcast::Sender<BusMessage>,
    base_url: String,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStore {
    /// Create a fresh in-memory store.
    pub fn new() -> Self {
        let (bus, _) = broadcast::channel(1024);
        Self {
            state: Mutex::new(MemoryState::default()),
            bus,
            base_url: String::new(),
        }
    }

    /// Set the public base URL used when generating local upload URLs.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemoryState> {
        self.state.lock().expect("memory state mutex poisoned")
    }
}

#[async_trait]
impl UserRepo for MemoryStore {
    async fn create(&self, user: &User) -> Result<()> {
        let mut st = self.lock();
        if st.user_ids.contains_key(&user.user_id) {
            return Err(Error::Conflict("user id is already taken".into()));
        }
        st.user_ids.insert(user.user_id.clone(), user.id);
        st.users.insert(user.id, user.clone());
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<User>> {
        Ok(self.lock().users.get(&id).cloned())
    }

    async fn get_by_user_id(&self, user_id: &str) -> Result<Option<User>> {
        let st = self.lock();
        Ok(st
            .user_ids
            .get(user_id)
            .and_then(|id| st.users.get(id))
            .cloned())
    }

    async fn update(&self, user: &User) -> Result<()> {
        let mut st = self.lock();
        st.user_ids.insert(user.user_id.clone(), user.id);
        st.users.insert(user.id, user.clone());
        Ok(())
    }

    async fn exists_user_id(&self, user_id: &str) -> Result<bool> {
        Ok(self.lock().user_ids.contains_key(user_id))
    }
}

#[async_trait]
impl SessionRepo for MemoryStore {
    async fn create(&self, session: &Session) -> Result<()> {
        self.lock()
            .sessions
            .insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<Session>> {
        Ok(self.lock().sessions.get(id).cloned())
    }

    async fn update(&self, session: &Session) -> Result<()> {
        self.lock()
            .sessions
            .insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Session>> {
        let st = self.lock();
        let mut sessions: Vec<Session> = st
            .sessions
            .values()
            .filter(|s| s.user_id == user_id)
            .cloned()
            .collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.issued_at));
        Ok(sessions)
    }

    async fn revoke_for_device(&self, device_id: i64) -> Result<()> {
        let mut st = self.lock();
        let now = chrono::Utc::now();
        for session in st.sessions.values_mut() {
            if session.device_id == device_id {
                session.revoked_at = Some(now);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl DeviceRepo for MemoryStore {
    async fn create(&self, device: &Device) -> Result<()> {
        self.lock().devices.insert(device.id, device.clone());
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Device>> {
        Ok(self.lock().devices.get(&id).cloned())
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<Device>> {
        let st = self.lock();
        Ok(st
            .devices
            .values()
            .filter(|d| d.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn delete(&self, id: i64) -> Result<()> {
        self.lock().devices.remove(&id);
        Ok(())
    }

    async fn touch(&self, id: i64, at: Timestamp) -> Result<()> {
        let mut st = self.lock();
        if let Some(device) = st.devices.get_mut(&id) {
            device.last_seen_at = at;
        }
        Ok(())
    }
}

#[async_trait]
impl ConversationRepo for MemoryStore {
    async fn create(&self, conversation: &Conversation) -> Result<()> {
        self.lock()
            .conversations
            .insert(conversation.id, conversation.clone());
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Conversation>> {
        Ok(self.lock().conversations.get(&id).cloned())
    }

    async fn find_direct(&self, a: i64, b: i64) -> Result<Option<Conversation>> {
        let st = self.lock();
        Ok(st
            .direct_index
            .get(&MemoryState::direct_key(a, b))
            .and_then(|id| st.conversations.get(id))
            .cloned())
    }

    async fn add_member(&self, member: &ConversationMember) -> Result<()> {
        let mut st = self.lock();
        st.conv_members
            .insert((member.conversation_id, member.user_id), member.clone());
        // Track direct pairs for fast lookup.
        let direct = st
            .conversations
            .get(&member.conversation_id)
            .map(|c| (c.kind, c.id));
        if let Some((ConversationKind::Direct, conv_id)) = direct {
            let members: Vec<i64> = st
                .conv_members
                .keys()
                .filter(|(c, _)| *c == member.conversation_id)
                .map(|(_, u)| *u)
                .collect();
            if members.len() == 2 {
                st.direct_index
                    .insert(MemoryState::direct_key(members[0], members[1]), conv_id);
            }
        }
        Ok(())
    }

    async fn get_member(
        &self,
        conversation_id: i64,
        user_id: i64,
    ) -> Result<Option<ConversationMember>> {
        Ok(self
            .lock()
            .conv_members
            .get(&(conversation_id, user_id))
            .cloned())
    }

    async fn list_members(&self, conversation_id: i64) -> Result<Vec<ConversationMember>> {
        let st = self.lock();
        Ok(st
            .conv_members
            .values()
            .filter(|m| m.conversation_id == conversation_id)
            .cloned()
            .collect())
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<ConversationMember>> {
        let st = self.lock();
        Ok(st
            .conv_members
            .values()
            .filter(|m| m.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn set_read_cursor(&self, conversation_id: i64, user_id: i64, seq: u64) -> Result<()> {
        let mut st = self.lock();
        if let Some(member) = st.conv_members.get_mut(&(conversation_id, user_id)) {
            if seq > member.last_read_seq {
                member.last_read_seq = seq;
            }
        }
        Ok(())
    }

    async fn remove_member(&self, conversation_id: i64, user_id: i64) -> Result<()> {
        let mut st = self.lock();
        st.conv_members.remove(&(conversation_id, user_id));
        st.direct_index.retain(|_, cid| *cid != conversation_id);
        Ok(())
    }

    async fn set_mute(
        &self,
        conversation_id: i64,
        user_id: i64,
        until: Option<Timestamp>,
    ) -> Result<()> {
        let mut st = self.lock();
        if let Some(member) = st.conv_members.get_mut(&(conversation_id, user_id)) {
            member.muted_until = until;
        }
        Ok(())
    }
}

#[async_trait]
impl MessageRepo for MemoryStore {
    async fn append(&self, message: &Message, outbox: &OutboxEvent) -> Result<()> {
        let mut st = self.lock();
        if st
            .msg_client
            .contains_key(&(message.sender_id, message.client_msg_id.clone()))
        {
            return Err(Error::Conflict("duplicate client_msg_id".into()));
        }
        st.msg_client.insert(
            (message.sender_id, message.client_msg_id.clone()),
            message.id,
        );
        st.messages.insert(message.id, message.clone());
        st.seqs
            .entry(message.conversation_id)
            .and_modify(|s| *s = (*s).max(message.seq))
            .or_insert(message.seq);
        st.outbox.insert(outbox.id, outbox.clone());
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Message>> {
        Ok(self.lock().messages.get(&id).cloned())
    }

    async fn find_by_client_id(
        &self,
        sender_id: i64,
        client_msg_id: &str,
    ) -> Result<Option<Message>> {
        let st = self.lock();
        Ok(st
            .msg_client
            .get(&(sender_id, client_msg_id.to_string()))
            .and_then(|id| st.messages.get(id))
            .cloned())
    }

    async fn list(
        &self,
        conversation_id: i64,
        before: Option<u64>,
        after: Option<u64>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        let st = self.lock();
        let mut messages: Vec<Message> = st
            .messages
            .values()
            .filter(|m| m.conversation_id == conversation_id)
            .filter(|m| before.map(|b| m.seq < b).unwrap_or(true))
            .filter(|m| after.map(|a| m.seq > a).unwrap_or(true))
            .cloned()
            .collect();
        messages.sort_by_key(|m| m.seq);
        if before.is_some() {
            let len = messages.len();
            let take = (limit as usize).min(len);
            messages = messages.split_off(len - take);
        } else {
            messages.truncate(limit as usize);
        }
        Ok(messages)
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<Message>> {
        let st = self.lock();
        let allowed: HashSet<i64> = st
            .conv_members
            .values()
            .filter(|m| m.user_id == query.viewer_id)
            .map(|m| m.conversation_id)
            .collect();
        let content_needle = query.content.as_deref().map(|c| c.to_lowercase());
        let filename_re = query
            .filename
            .as_deref()
            .map(chatbox_core::services::wildcard_to_regex)
            .transpose()?;
        let mut results: Vec<Message> = st
            .messages
            .values()
            .filter(|m| allowed.contains(&m.conversation_id))
            .filter(|m| {
                query
                    .conversation_id
                    .map(|c| m.conversation_id == c)
                    .unwrap_or(true)
            })
            .filter(|m| query.kind.map(|k| m.kind == k).unwrap_or(true))
            .filter(|m| {
                content_needle
                    .as_ref()
                    .map(|needle| {
                        m.content
                            .as_deref()
                            .map(|c| c.to_lowercase().contains(needle))
                            .unwrap_or(false)
                    })
                    .unwrap_or(true)
            })
            .filter(|m| {
                filename_re
                    .as_ref()
                    .map(|re| {
                        m.attachment_id
                            .and_then(|id| st.attachments.get(&id))
                            .map(|a| re.is_match(&a.filename))
                            .unwrap_or(false)
                    })
                    .unwrap_or(true)
            })
            .filter(|m| query.from.map(|f| m.created_at >= f).unwrap_or(true))
            .filter(|m| query.to.map(|t| m.created_at <= t).unwrap_or(true))
            .cloned()
            .collect();
        results.sort_by_key(|m| m.seq);
        if query.sort == SortOrder::Desc {
            results.reverse();
        }
        results.truncate(query.limit as usize);
        Ok(results)
    }

    async fn count(&self, conversation_id: i64) -> Result<u64> {
        Ok(self
            .lock()
            .messages
            .values()
            .filter(|m| m.conversation_id == conversation_id)
            .count() as u64)
    }

    async fn count_after(&self, conversation_id: i64, after: u64) -> Result<u64> {
        Ok(self
            .lock()
            .messages
            .values()
            .filter(|m| m.conversation_id == conversation_id && m.seq > after)
            .count() as u64)
    }

    async fn next_seq(&self, conversation_id: i64) -> Result<u64> {
        let mut st = self.lock();
        let next = st.seqs.get(&conversation_id).copied().unwrap_or(0) + 1;
        st.seqs.insert(conversation_id, next);
        Ok(next)
    }
}

#[async_trait]
impl ContactRepo for MemoryStore {
    async fn upsert(&self, contact: &Contact) -> Result<()> {
        self.lock()
            .contacts
            .insert((contact.owner_id, contact.contact_id), contact.clone());
        Ok(())
    }

    async fn get(&self, owner_id: i64, contact_id: i64) -> Result<Option<Contact>> {
        Ok(self.lock().contacts.get(&(owner_id, contact_id)).cloned())
    }

    async fn list(&self, owner_id: i64) -> Result<Vec<Contact>> {
        let st = self.lock();
        Ok(st
            .contacts
            .values()
            .filter(|c| c.owner_id == owner_id)
            .cloned()
            .collect())
    }

    async fn set_remark(
        &self,
        owner_id: i64,
        contact_id: i64,
        remark: Option<String>,
    ) -> Result<()> {
        let mut st = self.lock();
        if let Some(contact) = st.contacts.get_mut(&(owner_id, contact_id)) {
            contact.remark = remark;
        }
        Ok(())
    }

    async fn delete(&self, owner_id: i64, contact_id: i64) -> Result<()> {
        self.lock().contacts.remove(&(owner_id, contact_id));
        Ok(())
    }
}

#[async_trait]
impl FriendRequestRepo for MemoryStore {
    async fn create(&self, request: &FriendRequest) -> Result<()> {
        self.lock()
            .friend_requests
            .insert(request.id, request.clone());
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<FriendRequest>> {
        Ok(self.lock().friend_requests.get(&id).cloned())
    }

    async fn find_pending(&self, a: i64, b: i64) -> Result<Option<FriendRequest>> {
        let st = self.lock();
        Ok(st
            .friend_requests
            .values()
            .find(|r| {
                r.status == FriendRequestStatus::Pending
                    && ((r.from_id == a && r.to_id == b) || (r.from_id == b && r.to_id == a))
            })
            .cloned())
    }

    async fn list_for_user(&self, user_id: i64) -> Result<Vec<FriendRequest>> {
        let st = self.lock();
        Ok(st
            .friend_requests
            .values()
            .filter(|r| r.from_id == user_id || r.to_id == user_id)
            .cloned()
            .collect())
    }

    async fn update(&self, request: &FriendRequest) -> Result<()> {
        self.lock()
            .friend_requests
            .insert(request.id, request.clone());
        Ok(())
    }
}

#[async_trait]
impl GroupRepo for MemoryStore {
    async fn create(&self, group: &Group) -> Result<()> {
        self.lock().groups.insert(group.id, group.clone());
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Group>> {
        Ok(self.lock().groups.get(&id).cloned())
    }

    async fn get_by_conversation(&self, conversation_id: i64) -> Result<Option<Group>> {
        let st = self.lock();
        Ok(st
            .groups
            .values()
            .find(|g| g.conversation_id == conversation_id)
            .cloned())
    }

    async fn update(&self, group: &Group) -> Result<()> {
        self.lock().groups.insert(group.id, group.clone());
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<()> {
        let mut st = self.lock();
        st.groups.remove(&id);
        st.group_members.retain(|(g, _), _| *g != id);
        Ok(())
    }

    async fn add_member(&self, member: &GroupMember) -> Result<()> {
        self.lock()
            .group_members
            .insert((member.group_id, member.user_id), member.clone());
        Ok(())
    }

    async fn get_member(&self, group_id: i64, user_id: i64) -> Result<Option<GroupMember>> {
        Ok(self.lock().group_members.get(&(group_id, user_id)).cloned())
    }

    async fn list_members(&self, group_id: i64) -> Result<Vec<GroupMember>> {
        let st = self.lock();
        Ok(st
            .group_members
            .values()
            .filter(|m| m.group_id == group_id)
            .cloned()
            .collect())
    }

    async fn update_member(&self, member: &GroupMember) -> Result<()> {
        self.lock()
            .group_members
            .insert((member.group_id, member.user_id), member.clone());
        Ok(())
    }

    async fn remove_member(&self, group_id: i64, user_id: i64) -> Result<()> {
        self.lock().group_members.remove(&(group_id, user_id));
        Ok(())
    }

    async fn set_owner(&self, group_id: i64, owner_id: i64) -> Result<()> {
        let mut st = self.lock();
        if let Some(group) = st.groups.get_mut(&group_id) {
            group.owner_id = owner_id;
        }
        Ok(())
    }
}

#[async_trait]
impl BlockRepo for MemoryStore {
    async fn add(&self, block: &Block) -> Result<()> {
        self.lock()
            .blocks
            .insert((block.owner_id, block.blocked_id));
        Ok(())
    }

    async fn remove(&self, owner_id: i64, blocked_id: i64) -> Result<()> {
        self.lock().blocks.remove(&(owner_id, blocked_id));
        Ok(())
    }

    async fn is_blocked(&self, owner_id: i64, blocked_id: i64) -> Result<bool> {
        Ok(self.lock().blocks.contains(&(owner_id, blocked_id)))
    }

    async fn list(&self, owner_id: i64) -> Result<Vec<Block>> {
        let st = self.lock();
        Ok(st
            .blocks
            .iter()
            .filter(|(owner, _)| *owner == owner_id)
            .map(|(owner, blocked)| Block {
                owner_id: *owner,
                blocked_id: *blocked,
                created_at: chrono::Utc::now(),
            })
            .collect())
    }
}

#[async_trait]
impl AttachmentRepo for MemoryStore {
    async fn create(&self, attachment: &Attachment) -> Result<()> {
        self.lock()
            .attachments
            .insert(attachment.id, attachment.clone());
        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<Attachment>> {
        Ok(self.lock().attachments.get(&id).cloned())
    }
}

#[async_trait]
impl OutboxRepo for MemoryStore {
    async fn pending(&self, limit: usize) -> Result<Vec<OutboxEvent>> {
        let st = self.lock();
        Ok(st
            .outbox
            .values()
            .filter(|e| e.published_at.is_none())
            .take(limit)
            .cloned()
            .collect())
    }

    async fn mark_published(&self, ids: &[i64], at: Timestamp) -> Result<()> {
        let mut st = self.lock();
        for id in ids {
            if let Some(event) = st.outbox.get_mut(id) {
                event.published_at = Some(at);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl PluginRepo for MemoryStore {
    async fn is_enabled(&self, name: &str) -> Result<bool> {
        Ok(self
            .lock()
            .plugin_states
            .get(name)
            .copied()
            .unwrap_or(false))
    }

    async fn set_enabled(&self, name: &str, enabled: bool, _at: Timestamp) -> Result<()> {
        self.lock().plugin_states.insert(name.to_string(), enabled);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<(String, bool)>> {
        Ok(self
            .lock()
            .plugin_states
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect())
    }
}

#[async_trait]
impl Cache for MemoryStore {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut st = self.lock();
        if let Some((value, expires)) = st.cache.get(key) {
            if *expires > Instant::now() {
                return Ok(Some(value.clone()));
            }
            st.cache.remove(key);
        }
        Ok(None)
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<()> {
        self.lock()
            .cache
            .insert(key.to_string(), (value.to_string(), Instant::now() + ttl));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.lock().cache.remove(key);
        Ok(())
    }
}

#[async_trait]
impl PresenceStore for MemoryStore {
    async fn set_online(&self, user_id: i64, online: bool) -> Result<()> {
        let mut st = self.lock();
        if online {
            st.presence.insert(user_id);
        } else {
            st.presence.remove(&user_id);
        }
        Ok(())
    }

    async fn is_online(&self, user_id: i64) -> Result<bool> {
        Ok(self.lock().presence.contains(&user_id))
    }

    async fn online_users(&self, users: &[i64]) -> Result<Vec<i64>> {
        let st = self.lock();
        Ok(users
            .iter()
            .copied()
            .filter(|u| st.presence.contains(u))
            .collect())
    }
}

#[async_trait]
impl ConnectionRegistry for MemoryStore {
    async fn register(&self, user_id: i64, info: ConnectionInfo) -> Result<()> {
        let mut st = self.lock();
        let conns = st.registry.entry(user_id).or_default();
        conns.retain(|c| c.conn_id != info.conn_id);
        conns.push(info);
        Ok(())
    }

    async fn unregister(&self, user_id: i64, conn_id: &str) -> Result<()> {
        let mut st = self.lock();
        if let Some(conns) = st.registry.get_mut(&user_id) {
            conns.retain(|c| c.conn_id != conn_id);
        }
        Ok(())
    }

    async fn heartbeat(&self, user_id: i64, conn_id: &str, at: Timestamp) -> Result<()> {
        let mut st = self.lock();
        if let Some(conns) = st.registry.get_mut(&user_id) {
            if let Some(conn) = conns.iter_mut().find(|c| c.conn_id == conn_id) {
                conn.last_seen = at;
            }
        }
        Ok(())
    }

    async fn connections(&self, user_id: i64) -> Result<Vec<ConnectionInfo>> {
        Ok(self
            .lock()
            .registry
            .get(&user_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn connections_for(&self, user_ids: &[i64]) -> Result<Vec<(i64, Vec<ConnectionInfo>)>> {
        let st = self.lock();
        Ok(user_ids
            .iter()
            .map(|u| (*u, st.registry.get(u).cloned().unwrap_or_default()))
            .collect())
    }

    async fn prune_stale(&self, before: Timestamp) -> Result<usize> {
        let mut st = self.lock();
        let mut removed = 0;
        for conns in st.registry.values_mut() {
            let len = conns.len();
            conns.retain(|c| c.last_seen >= before);
            removed += len - conns.len();
        }
        Ok(removed)
    }
}

#[async_trait]
impl MessageBus for MemoryStore {
    async fn publish(&self, topic: &str, payload: &str) -> Result<()> {
        let _ = self.bus.send(BusMessage {
            topic: topic.to_string(),
            payload: payload.to_string(),
        });
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<BusMessage> {
        self.bus.subscribe()
    }
}

#[async_trait]
impl RateLimiter for MemoryStore {
    async fn check(&self, key: &str, limit: u32, window: Duration) -> Result<bool> {
        let mut st = self.lock();
        let now = Instant::now();
        let entry = st.rate.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, 1);
            return Ok(true);
        }
        if entry.1 >= limit {
            return Ok(false);
        }
        entry.1 += 1;
        Ok(true)
    }
}

#[async_trait]
impl ObjectStorage for MemoryStore {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        self.lock().objects.insert(key.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.lock().objects.get(key).cloned())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.lock().objects.contains_key(key))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.lock().objects.remove(key);
        Ok(())
    }

    async fn presign_put(&self, key: &str, _content_type: &str, _ttl: Duration) -> Result<String> {
        let id = key.rsplit('/').next().unwrap_or_default();
        Ok(format!("{}/api/v1/files/upload/{}", self.base_url, id))
    }

    async fn presign_get(&self, key: &str, _ttl: Duration) -> Result<String> {
        let id = key.rsplit('/').next().unwrap_or_default();
        Ok(format!("{}/api/v1/files/{}", self.base_url, id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn user_roundtrip_and_conflict() {
        let store = MemoryStore::new();
        let now = chrono::Utc::now();
        let user = User {
            id: 1,
            user_id: "alice".into(),
            display_name: None,
            password_hash: "x".into(),
            created_at: now,
            updated_at: now,
        };
        UserRepo::create(&store, &user).await.unwrap();
        assert_eq!(
            UserRepo::get_by_user_id(&store, "alice")
                .await
                .unwrap()
                .unwrap()
                .id,
            1
        );
        assert!(UserRepo::create(&store, &user).await.is_err());
    }

    #[tokio::test]
    async fn sequences_are_monotonic() {
        let store = MemoryStore::new();
        let a = MessageRepo::next_seq(&store, 1).await.unwrap();
        let b = MessageRepo::next_seq(&store, 1).await.unwrap();
        let c = MessageRepo::next_seq(&store, 2).await.unwrap();
        assert_eq!((a, b, c), (1, 2, 1));
    }

    #[tokio::test]
    async fn direct_index_is_order_independent() {
        let store = MemoryStore::new();
        let conv = Conversation {
            id: 9,
            kind: ConversationKind::Direct,
            created_at: chrono::Utc::now(),
        };
        ConversationRepo::create(&store, &conv).await.unwrap();
        for user in [5, 3] {
            ConversationRepo::add_member(
                &store,
                &ConversationMember {
                    conversation_id: 9,
                    user_id: user,
                    last_read_seq: 0,
                    muted_until: None,
                    joined_at: chrono::Utc::now(),
                },
            )
            .await
            .unwrap();
        }
        assert!(ConversationRepo::find_direct(&store, 3, 5)
            .await
            .unwrap()
            .is_some());
        assert!(ConversationRepo::find_direct(&store, 5, 3)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn rate_limit_enforced() {
        let store = MemoryStore::new();
        for _ in 0..3 {
            assert!(RateLimiter::check(&store, "k", 3, Duration::from_secs(60))
                .await
                .unwrap());
        }
        assert!(!RateLimiter::check(&store, "k", 3, Duration::from_secs(60))
            .await
            .unwrap());
    }
}
