# Plugin System

ChatBox separates two plugin tiers that differ in **where they are installed**
and **how far their effects reach**. This distinction is deliberate: it keeps
the client fully user-controlled while allowing administrators to extend the
server.

## Client plugins

- Installed by the **user** on their own device.
- Applied **globally to that client** and only affect local behavior.
- **Never require any server change.**
- Examples: language packs (translations), UI themes, and other client-side
  features.
- Installing or removing one is entirely up to the user.

The backend only *serves manifests* that describe available client plugins
(`GET /api/v1/plugins/client`). A manifest is self-describing data; it cannot
change server behavior.

```json
{
  "name": "zh-CN",
  "version": "1.0.0",
  "kind": "language-pack",
  "entry": "translations/zh-CN.json",
  "description": "Simplified Chinese translations"
}
```

## Server plugins

- Installed by the **server administrator** on the backend.
- Run server-side and **must be deployed and enabled** before they have any
  effect.
- Clients cannot install them.
- May add or change behavior for all connected users.

The server runtime discovers a plugin from a **manifest** file, verifies it
declares the `server` tier and a supported API version, and exposes a set of
hooks. A plugin is only active after an administrator enables it.

```json
{
  "name": "audit-log",
  "version": "1.0.0",
  "kind": "server",
  "api_version": 2,
  "entry": "libaudit_log.so",
  "description": "Append an audit record for every message"
}
```

A manifest whose `api_version` differs from the host's
`PLUGIN_API_VERSION` (currently `2`) is skipped with a warning.

### Hook contract

Hooks exchange JSON: the host passes a [`HookContext`] document and a plugin
returns a JSON result. They are called in **registration order** — built-in
plugins first, then discovered plugins sorted by manifest file name — so the
chain is deterministic and testable.

| Hook | Event | Payload | Result |
|---|---|---|---|
| `on_message_before_persist` | `message.before_persist` | `sender_id`, `conversation_id`, `kind`, `content` | `allow`, `reject`, or `rewrite` |
| `on_message_after_persist` | `message.after_persist` | the persisted message | observed only |
| `on_user_register` | `user.register` | `user_id`, `handle` | observed only |
| `on_group_created` | `group.created` | `group_id`, `owner_id` | observed only |

`message.before_persist` is the only hook that can change the pipeline. Its
result is one of:

```json
{ "outcome": "allow" }
{ "outcome": "reject", "reason": "message contains 'forbidden'" }
{ "outcome": "rewrite", "content": "redacted text", "kind": "text" }
```

A `reject` aborts the send with `403 Forbidden` (HTTP) or a `rejected` error
frame (WebSocket). A `rewrite` replaces `content` and/or `kind` before the
message is persisted, and every later plugin in the chain sees the rewritten
payload. Multiple rewrites compose; the first rejection wins. `attachment_id`
and `client_msg_id` cannot be rewritten.

#### Isolation and deadlines

Each hook invocation is bounded by `PLUGIN_HOOK_TIMEOUT_MS` (default `500`,
`0` disables the deadline):

- Built-in plugins are polled with a Tokio timeout.
- Dynamically loaded plugins run on a blocking thread (`spawn_blocking`) with
  `catch_unwind`, so a panic never unwinds into the host. A timed-out call
  cannot be killed; the host logs it and stops waiting, but the thread keeps
  running until the plugin returns.
- A timeout, error, panic, or malformed result is logged and treated as
  **no opinion**. It never blocks message delivery.
- Plugin results are capped at 1 MiB; larger results are rejected and treated
  as no opinion.

### Host API

Dynamic plugins receive a [`HostApi`] table at initialisation and can call back
into the server from inside a hook through one generic entry point:

```c
char *call(void *ctx, const char *method, const char *params_json);
```

The response is a JSON envelope, `{"ok":true,"result":...}` or
`{"ok":false,"error":"..."}`. The host owns the returned string; release it
with the table's `free_string`. Built-in plugins run in-process against the
same stores directly and therefore do not need the callback table.

| Method | Params | Result |
|---|---|---|
| `log` | `level`, `message` | `null` |
| `now` | — | RFC 3339 timestamp |
| `user.get` | `user_id` | public user profile or `null` |
| `conversation.get` | `conversation_id` | conversation and members or `null` |
| `kv.get` | `key` | string or `null` |
| `kv.set` | `key`, `value`, `ttl_secs` (optional) | `null` |
| `kv.delete` | `key` | `null` |
| `kv.list` | `prefix` (optional) | `[{"key","value"}]` |

Host API rules:

- **Read-only by design.** Plugins cannot send messages, mutate users or
  conversations, or open new database writes. This keeps hooks re-entrant-safe
  and prevents a plugin from looping the delivery pipeline.
- **KV is namespaced per plugin.** The host prefixes every key with the plugin
  name, so a plugin can only read and write its own entries. Entries are
  persisted in the `plugin_kv` table and survive restarts; `ttl_secs` expires
  them automatically.
- Calls are only valid while a hook is running. The host passes an opaque
  context pointer that is invalid after the hook returns.

### ABI v2

A dynamic plugin is a shared library exporting `chatbox_plugin_init`:

```c
int32_t chatbox_plugin_init(uint32_t api_version,
                            struct PluginVTable *out,
                            const struct HostApi *host);
```

The host fills out a `PluginVTable` with the plugin name, version,
description, hook function pointers, and an optional `free_string` used to
release hook results. Hooks have the signature

```c
const char *hook(const char *context_json, const HostApi *host);
```

and return a NUL-terminated JSON document owned by the plugin, or `NULL` for
"no opinion". The host copies and frees it with `free_string`. The exported
name, version, and description override the manifest values when present, and
are otherwise taken from the manifest.

### Discovery and enablement

1. The administrator places a plugin manifest and its artifact under
   `plugins/` (configurable via `PLUGIN_DIR`).
2. At startup the runtime loads every manifest and registers it as
   **disabled**.
3. The administrator enables it with
   `POST /api/v1/plugins/server/{name}/enable`. The enabled set is persisted so
   it survives restarts.
4. Disabling a plugin unregisters its hooks immediately.

Built-in plugins are compiled into the server and registered the same way, so
administrators can toggle them without a filesystem change.
