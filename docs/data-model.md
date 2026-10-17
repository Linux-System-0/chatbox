# Data Model

All identifiers are **53-bit Snowflake** values (time-ordered and collision-free
across nodes) except where a human-chosen string is required. 53 bits keep every
id within JavaScript's `Number.MAX_SAFE_INTEGER`, so the web and Android clients
can round-trip them without precision loss. Timestamps are stored in UTC.

## Identifiers

- `id` — internal Snowflake identifier used by foreign keys.
- `user_id` — the **custom public ID** a user chooses at registration. It is
  unique, case-sensitive, and used for login and for adding contacts.
- `client_msg_id` — a client-generated idempotency key (string, unique per
  sender).
- `seq` — a monotonic per-conversation sequence assigned by the backend.

## Entities

### User

| Field | Type | Notes |
|---|---|---|
| `id` | Snowflake | Primary key |
| `user_id` | string(64) | Unique custom ID |
| `display_name` | string(64)? | Falls back to `user_id` when unset |
| `password_hash` | string | Argon2id |
| `created_at` | timestamp | |
| `updated_at` | timestamp | |

### Device

A user may be signed in on multiple devices. Sessions are bound to a device.

| Field | Type | Notes |
|---|---|---|
| `id` | Snowflake | Primary key |
| `user_id` | Snowflake | FK → User |
| `name` | string(64) | Client-provided label |
| `platform` | enum(`web`,`android`,`unknown`) | |
| `last_seen_at` | timestamp | Heartbeat |
| `created_at` | timestamp | |

### Session

| Field | Type | Notes |
|---|---|---|
| `id` | string | JWT `jti` |
| `user_id` | Snowflake | |
| `device_id` | Snowflake | |
| `refresh_hash` | string | Hash of the refresh token |
| `issued_at` / `expires_at` | timestamp | |
| `revoked_at` | timestamp? | Soft revocation |

### Conversation

Unifies one-to-one and group conversations.

| Field | Type | Notes |
|---|---|---|
| `id` | Snowflake | Primary key |
| `kind` | enum(`direct`,`group`) | |
| `created_at` | timestamp | |

`ConversationMember` links users to conversations and stores the per-user read
cursor (`last_read_seq`) and optional mute deadline.

### Message

| Field | Type | Notes |
|---|---|---|
| `id` | Snowflake | Primary key |
| `conversation_id` | Snowflake | Shard key |
| `seq` | u64 | Per-conversation, unique with `conversation_id` |
| `sender_id` | Snowflake | |
| `client_msg_id` | string(64) | Unique per sender |
| `kind` | enum(`text`,`file`,`system`) | |
| `content` | text? | Text body or system payload |
| `attachment_id` | Snowflake? | FK → Attachment |
| `mentions` | json | List of mentioned user IDs |
| `created_at` | timestamp | |
| `edited_at` / `deleted_at` | timestamp? | |

### Attachment

| Field | Type | Notes |
|---|---|---|
| `id` | Snowflake | Primary key |
| `owner_id` | Snowflake | Uploader |
| `filename` | string(255) | Original filename |
| `size` | u64 | Bytes |
| `mime` | string(127) | Content type |
| `sha256` | string(64) | Integrity |
| `storage_key` | string(255) | Object storage key |
| `created_at` | timestamp | |

### Contact & FriendRequest

- `Contact` (`owner_id`, `contact_id`, `remark`) is created only after a
  friend request is **accepted**.
- `FriendRequest` (`id`, `from_id`, `to_id`, `message?`, `status`, timestamps)
  has status `pending`, `accepted`, or `rejected`.

### Group

`Group` is 1:1 with a `group`-kind conversation.

| Field | Type | Notes |
|---|---|---|
| `id` | Snowflake | Primary key |
| `conversation_id` | Snowflake | Unique |
| `name` | string(64) | |
| `owner_id` | Snowflake | Exactly one owner |
| `created_at` | timestamp | |

`GroupMember` stores `role ∈ {owner, admin, member}`, `muted_until?`, and join
time. Role permissions are described in `README.md`.

### Block

`Block` (`owner_id`, `blocked_id`, `created_at`) suppresses direct messages in
both directions.

## Search

Search is scoped by conversation and supports:

- **Type search** — filename wildcards (`*.mp4`) for file messages.
- **Time search** — inclusive `[from, to]` range with minute precision.
- **Content search** — text body matching.
- **Sorting** — `seq` ascending or descending.
- **Jump-to-message** — the result carries `conversation_id` + `seq` so the
  client can scroll to and highlight the exact message.

## Storage layout

- Messages are sharded by `conversation_id`; the uniqueness constraint on
  `(conversation_id, seq)` and `(sender_id, client_msg_id)` lives on the same
  shard.
- The `outbox` table lives next to `messages` so the append and the publish
  intent commit in one transaction.
- Read replicas serve read-only queries; writers read their own writes by
  routing briefly to the primary.
