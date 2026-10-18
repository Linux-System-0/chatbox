# HTTP/REST API (Control Plane)

Base path: `/api/v1`. All requests and responses are JSON (`application/json`)
unless noted. Authenticated endpoints require
`Authorization: Bearer <access_token>`.

Errors use a consistent envelope:

```json
{ "error": { "code": "invalid_credentials", "message": "..." } }
```

## Authentication

| Method | Path | Description |
|---|---|---|
| `POST` | `/auth/register` | Create an account with a custom ID + password |
| `POST` | `/auth/login` | Verify password, issue access + refresh tokens |
| `POST` | `/auth/refresh` | Exchange a refresh token for a new pair |
| `POST` | `/auth/logout` | Revoke the current session |
| `GET` | `/auth/sessions` | List active sessions/devices |

## Users

| Method | Path | Description |
|---|---|---|
| `GET` | `/users/me` | Current profile |
| `PATCH` | `/users/me` | Update custom ID, display name, or avatar |
| `GET` | `/users/{user_id}` | Look up a user by custom ID |

`PATCH /users/me` accepts any subset of `user_id`, `display_name`, and
`avatar_url`. A `null` or empty string clears `display_name` / `avatar_url`.
Changing `user_id` is rejected when the new ID is already taken. Avatars may be
an `http(s)` URL or a `data:image/...` URL up to 512 KiB; the backend validates
and enforces that limit, while the Web client downsizes uploads before sending.

### Presence

| Method | Path | Description |
|---|---|---|
| `GET` | `/presence?user_ids=1,2,3` | Current online state for internal user IDs |

Presence is also pushed over WebSocket as `presence` frames. The query endpoint
lets a freshly connected client bootstrap the state for contacts and open
conversations.

## Devices

| Method | Path | Description |
|---|---|---|
| `GET` | `/devices` | List devices |
| `DELETE` | `/devices/{id}` | Remove a device (revokes its sessions) |

## Contacts

| Method | Path | Description |
|---|---|---|
| `GET` | `/contacts` | List accepted contacts |
| `PATCH` | `/contacts/{user_id}` | Set a remark/nickname |
| `DELETE` | `/contacts/{user_id}` | Remove a contact |
| `POST` | `/contacts/requests` | Send a friend request by user ID |
| `GET` | `/contacts/requests` | List incoming/outgoing requests with profiles |
| `POST` | `/contacts/requests/{id}/accept` | Accept (recipient only) |
| `POST` | `/contacts/requests/{id}/reject` | Reject (recipient only) |

`GET /contacts` returns only accepted contacts that the caller has not blocked.
`PATCH /contacts/{user_id}` accepts `remark` and `remark_avatar_url`; a `null`
or empty string clears the field. Only the recipient of a pending request may
accept or reject it; outgoing requests are pending and expose no accept action.
A direct conversation can be created only after the contact link exists.

## Conversations & messages

| Method | Path | Description |
|---|---|---|
| `GET` | `/conversations` | List conversations with last-read cursors |
| `POST` | `/conversations/direct` | Get or create a direct conversation (contacts only) |
| `GET` | `/conversations/{id}/messages` | Page messages (`before`/`after`/`limit`) |
| `POST` | `/conversations/{id}/messages` | REST send (used by clients without WS) |
| `POST` | `/conversations/{id}/read` | Advance the read cursor (publishes a `read` frame) |

`ConversationView` includes `avatar_url`, `peer_id` (direct only), `group_id`
(group only), and `member_count`, so clients can render the title, avatar, and
presence state without extra lookups. Direct titles already apply the caller's
contact remark precedence. It also contains `read_cursors` — one
`{ "user_id", "last_read_seq" }` entry per member — which clients combine with
the live `read` WebSocket frame to render per-message read receipts.

## Groups

| Method | Path | Description |
|---|---|---|
| `POST` | `/groups` | Create a group (creator becomes owner) |
| `GET` | `/groups/{id}` | Group details + members |
| `PATCH` | `/groups/{id}` | Rename / change invite policy |
| `POST` | `/groups/{id}/members` | Add members |
| `DELETE` | `/groups/{id}/members/{user_id}` | Kick a member |
| `POST` | `/groups/{id}/members/{user_id}/mute` | Mute/unmute a member |
| `POST` | `/groups/{id}/members/{user_id}/role` | Promote/demote an admin (owner only) |
| `PATCH` | `/groups/{id}/remark` | Set the caller's private group remark |
| `POST` | `/groups/{id}/transfer` | Transfer ownership (owner only) |
| `POST` | `/groups/{id}/leave` | Leave the group |
| `DELETE` | `/groups/{id}` | Dissolve the group (owner only) |

Only the owner may call `/groups/{id}/members/{user_id}/role`; the body is
`{ "role": "admin" }` or `{ "role": "member" }`. Ownership itself changes
through `/groups/{id}/transfer`. `PATCH /groups/{id}/remark` accepts
`{ "remark": "..." }` and clears the caller's private alias with `null` or an
empty string. The alias is never exposed to other members.

## Block list

| Method | Path | Description |
|---|---|---|
| `GET` | `/blocks` | List blocked users |
| `POST` | `/blocks/{user_id}` | Block a user |
| `DELETE` | `/blocks/{user_id}` | Unblock a user |

Blocking does not delete the contact link, but the blocked user is hidden from
`GET /contacts` until they are unblocked.

## Files

Files are uploaded **directly to object storage** with a pre-signed URL.

| Method | Path | Description |
|---|---|---|
| `POST` | `/files/presign` | Request an upload URL + attachment ID |
| `PUT` | `/files/upload/{id}` | Upload bytes to the local development backend |
| `POST` | `/files/{id}/complete` | Confirm the upload and attach metadata |
| `GET` | `/files/{id}` | Get attachment metadata and a download URL |
| `GET` | `/files/{id}/content` | Download attachment bytes (no inline preview) |

## Search

`GET /search` with query parameters:

| Parameter | Description |
|---|---|
| `q` | Text query (content search) |
| `filename` | Filename wildcard (`*.mp4`) |
| `conversation_id` | Restrict to one conversation |
| `kind` | `text` or `file` |
| `from` / `to` | ISO-8601 timestamps, minute precision |
| `sort` | `asc` or `desc` (by `seq`) |
| `limit` / `cursor` | Pagination |

## Plugins

| Method | Path | Description |
|---|---|---|
| `GET` | `/plugins/client` | Client plugin manifests (public) |
| `GET` | `/plugins/server` | Server plugins (admin only) |
| `POST` | `/plugins/server/{name}/enable` | Enable a server plugin (admin only) |
| `POST` | `/plugins/server/{name}/disable` | Disable a server plugin (admin only) |

`GET /plugins/client` is discovery-only. Web and Android install client plugin
files locally for the signed-in user; the files are never uploaded to the
server (see [`plugin-system.md`](plugin-system.md)).

## Health

| Method | Path | Description |
|---|---|---|
| `GET` | `/health/live` | Process liveness |
| `GET` | `/health/ready` | Dependency readiness |
