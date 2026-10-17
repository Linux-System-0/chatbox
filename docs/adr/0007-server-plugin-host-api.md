# ADR 0007 — Server Plugin Host API and Isolated Hooks

- **Status:** Accepted
- **Date:** 2026-10-19

## Context

The first server-plugin runtime exposed four observe-only hooks. That was not
enough to build useful integrations: plugins could not inspect a user or a
conversation, could not keep a cursor between hook invocations, could only
return a status code (so a rejection carried no reason), and had no way to
rewrite a message. On the other side, the runtime promised isolation it did not
provide: dynamically loaded hooks were called synchronously on the async
worker with no deadline and no panic containment.

Extending the runtime raises three design questions: what a plugin may do, how
a plugin is isolated, and how both stay compatible as the API grows.

## Decision

### Hooks exchange JSON and may rewrite

Hook functions take the JSON context and return a JSON document. The
`message.before_persist` result is one of `allow`, `reject` (with a reason), or
`rewrite` (with `content` and/or `kind`). Rewrites compose in registration
order; the first rejection wins. `attachment_id` and `client_msg_id` are not
rewritable, so a plugin can redact a message but cannot retarget it.

The plugin API version is bumped to **2**; the host loads only plugins that
declare the exact version it implements. A v1 plugin must be rebuilt.

### Hooks are isolated

- Built-in hooks run as async futures under a Tokio deadline
  (`PLUGIN_HOOK_TIMEOUT_MS`, default 500 ms).
- Dynamic hooks run on the blocking pool with `catch_unwind` and the same
  deadline. Hooks use the `C-unwind` ABI so a Rust panic unwinds into the host
  instead of aborting the process.
- A timeout, error, panic, or malformed result is logged and treated as "no
  opinion", so a plugin cannot block message delivery.
- Hook results are capped at 1 MiB.

A timed-out blocking call cannot be killed; its thread keeps running until the
plugin returns. The host only stops waiting for it. This is documented, and it
is why plugins must keep hooks short.

### Host API is read-only and generic

Plugins receive one host callback table at initialisation and call back through
a single JSON entry point, `call(method, params)`. The API is read-only:

- `log`, `now`, `user.get`, `conversation.get` — inspection.
- `kv.get`/`set`/`delete`/`list` — plugin-scoped state with optional TTL,
  persisted in `plugin_kv`.

There is deliberately no `message.send` or mutation method: a hook that could
send a message would re-enter the delivery pipeline and invite recursion and
fan-out loops. New capabilities are added as new methods, which keeps the ABI
stable; the ABI version only changes for structural changes.

### Subscription scope

The host-side `ServerHost` runs one dedicated thread with a current-thread
Tokio runtime. Dynamic hooks execute on the blocking pool and cannot start a
nested runtime, so host calls are dispatched to that thread over a channel and
the caller blocks on the reply.

## Consequences

- Administrators can write audit, redaction, quota, and enrichment plugins
  without patching the server.
- A buggy plugin degrades gracefully instead of stalling or crashing the node.
- The host API stays small and auditable; anything not listed above requires a
  host change and a new ADR.
- Plugins are still trusted code: they run in-process with the server's
  privileges. The deadline and panic isolation reduce blast radius, not the
  need to deploy only reviewed artifacts.

## Alternatives considered

- **Subprocess-based plugins** — strongest isolation, but adds IPC, lifecycle,
  and serialization overhead. Rejected for now; the JSON contract keeps this
  path open.
- **WASM plugins** — sandboxed and portable, but needs a WASM runtime and a
  capability layer. Rejected as a larger change than the current tier needs.
- **Mutable host API (`message.send`)** — rejected: it turns a one-shot hook
  into a feedback loop and complicates ordering guarantees.
- **Unversioned ABI with tolerant loading** — rejected: silently running a
  plugin built against a different layout is unsafe.
