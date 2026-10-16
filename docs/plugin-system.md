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
declares the `server` tier and a compatible API version, and exposes a set of
hooks. A plugin is only active after an administrator enables it.

```json
{
  "name": "audit-log",
  "version": "1.0.0",
  "kind": "server",
  "api_version": 1,
  "entry": "libaudit_log.so",
  "description": "Append an audit record for every message"
}
```

### Hooks

| Hook | Fired when |
|---|---|
| `on_message_before_persist` | Before a message is written; may reject or rewrite |
| `on_message_after_persist` | After commit, before fan-out |
| `on_user_register` | A new account is created |
| `on_group_created` | A group is created |

Hooks are asynchronous, bounded by a deadline, and isolated: a failing hook
never blocks the core pipeline. The runtime reports hook errors to the
observability pipeline and continues.

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
