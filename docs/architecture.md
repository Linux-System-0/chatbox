# Architecture

ChatBox separates the **control plane** (HTTP/REST) from the **data plane**
(WebSocket realtime) so each scales independently and a slow client can never
stall request/response traffic. Application nodes are stateless; all shared
state lives in Redis, MySQL, and object storage.

## Components

```text
backend/
├── crates/
│   ├── core/       # Domain models, ports (traits), services
│   ├── storage/    # MySQL, Redis, and in-memory adapters
│   ├── plugin/     # Server plugin runtime & registry
│   ├── api/        # HTTP/REST handlers (control plane)
│   ├── realtime/   # WebSocket gateway (data plane)
│   ├── workers/    # Outbox relay + fan-out workers
│   └── server/     # Composition root; builds and runs a node
```

- **`core`** owns the domain. It defines models, the repository/service ports
  (traits), and the business rules. It has no dependency on a database driver.
- **`storage`** implements the ports over MySQL, Redis, and an in-memory backend
  used by tests and single-process development.
- **`plugin`** implements the server plugin runtime. Client plugins are shipped
  to clients as manifests and never touch the backend (see `plugin-system.md`).
- **`api`** exposes REST handlers and the JWT auth middleware.
- **`realtime`** terminates WebSocket connections, owns the connection
  registry, and pushes outbound frames.
- **`workers`** runs the outbox relay and fan-out loop.
- **`server`** wires the layers together, loads configuration, and selects the
  storage backend.

## Scaling planes

- The HTTP API and the WebSocket gateway scale separately. A flash crowd of
  connections does not consume REST capacity.
- Nodes are stateless. JWT access tokens and Redis-backed sessions let any node
  serve any request.
- Live connections are tracked in the **connection registry**
  (`conn:{user_id} -> {(node, device, conn_id, last_seen)}`) with TTL
  heartbeats, so a message is routed to the owning gateway instead of being
  broadcast to every node.

## Storage scaling

- **MySQL shards.** A deployment has one or more shards. Each shard is a
  primary plus zero or more read replicas and carries the full schema; rows are
  distributed across shards by a stable shard key (`conversation_id` for the
  message plane, `user_id`/`owner_id`/`id` for the rest). Operations that know
  their key hit one shard; lookups and list queries without a routing key fan
  out across shards and merge. See ADR 0006 for the routing table.
- **Read replicas.** Reads round-robin across a shard's replicas and fall back
  to the primary; writes always go to the primary. Read-your-writes is handled
  by reading the primary on the write path, with lag-aware routing as future
  work.
- **Redis.** A single node or a Redis Cluster can back the cache, presence,
  registry, rate limiter, and pub/sub. The registry uses one hash per user so it
  never needs `SCAN`; pub/sub subscribes to one seed node because a cluster
  forwards published messages to every node.

## Message pipeline

1. **Accept** — the client sends with a `client_msg_id`; the API validates auth
   and rate limits.
2. **Persist** — one short transaction appends the message and an outbox row to
   the same shard (no cross-shard work).
3. **Publish** — an outbox relay publishes to the durable stream; the sender
   gets an immediate ack with the assigned conversation `seq`.
4. **Fan-out** — workers resolve recipients (cached membership), apply the
   push/pull policy, and route via the connection registry.
5. **Deliver** — the owning gateways push over WebSocket with bounded queues;
   offline devices advance their cursor on reconnect.
6. **Ack** — per-device acks advance read cursors; receipts are coalesced.

## Dependency direction

```text
server ──▶ api ─────────┐
       ──▶ realtime ────├──▶ core ◀── storage (implements core ports)
       ──▶ workers ─────┘
       ──▶ plugin ──────▶ core
```

`core` never imports a concrete driver. `storage` never imports `api` or
`realtime`. This keeps the domain testable with the in-memory adapters.

## Failure handling

- **Delivery** is at-least-once. Every message carries a client-generated
  idempotency key (`client_msg_id`); writers are idempotent so the *effect* is
  exactly-once.
- **Ordering** is a monotonic per-conversation sequence number. Clients detect
  gaps and re-sync.
- **Timeouts** are bounded at every hop; retries use exponential backoff with
  full jitter and a retry budget.
- **Isolation** uses bulkheads (separate pools per dependency), circuit
  breakers, and load shedding.
- **Caching** is cache-aside with invalidate-on-write, single-flight to prevent
  stampedes, jittered TTLs to prevent avalanches, and negative caching to
  prevent penetration.
