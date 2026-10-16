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
| `PATCH` | `/users/me` | Update display name |
| `GET` | `/users/{user_id}` | Look up a user by custom ID |

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
| `GET` | `/contacts/requests` | List incoming/outgoing requests |
| `POST` | `/contacts/requests/{id}/accept` | Accept |
| `POST` | `/contacts/requests/{id}/reject` | Reject |

## Conversations & messages

| Method | Path | Description |
|---|---|---|
| `GET` | `/conversations` | List conversations with last-read cursors |
| `POST` | `/conversations/direct` | Get or create a direct conversation |
| `GET` | `/conversations/{id}/messages` | Page messages (`before`/`after`/`limit`) |
| `POST` | `/conversations/{id}/messages` | REST send (used by clients without WS) |
| `POST` | `/conversations/{id}/read` | Advance the read cursor |

## Groups

| Method | Path | Description |
|---|---|---|
| `POST` | `/groups` | Create a group (creator becomes owner) |
| `GET` | `/groups/{id}` | Group details + members |
| `PATCH` | `/groups/{id}` | Rename / change invite policy |
| `POST` | `/groups/{id}/members` | Add members |
| `DELETE` | `/groups/{id}/members/{user_id}` | Kick a member |
| `POST` | `/groups/{id}/members/{user_id}/mute` | Mute/unmute a member |
| `POST` | `/groups/{id}/transfer` | Transfer ownership (owner only) |
| `POST` | `/groups/{id}/leave` | Leave the group |
| `DELETE` | `/groups/{id}` | Dissolve the group (owner only) |

## Block list

| Method | Path | Description |
|---|---|---|
| `GET` | `/blocks` | List blocked users |
| `POST` | `/blocks/{user_id}` | Block a user |
| `DELETE` | `/blocks/{user_id}` | Unblock a user |

## Files

Files are uploaded **directly to object storage** with a pre-signed URL.

| Method | Path | Description |
|---|---|---|
| `POST` | `/files/presign` | Request an upload URL + attachment ID |
| `POST` | `/files/{id}/complete` | Confirm the upload and attach metadata |
| `GET` | `/files/{id}` | Get a download URL (no inline preview) |

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

## Health

| Method | Path | Description |
|---|---|---|
| `GET` | `/health/live` | Process liveness |
| `GET` | `/health/ready` | Dependency readiness |
