# ADR 0006 — MySQL Sharding, Replicas, and Redis Cluster

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

The backend must scale horizontally. A single MySQL primary caps write
throughput, and a single Redis node caps cache, registry, and pub/sub capacity.
The roadmap calls for MySQL sharding, multiple read replicas, and Redis
clustering. The design must keep the domain code (`chatbox-core`) free of
database concerns and keep the existing repository ports unchanged.

## Decision

### MySQL shards

- A **shard** is one primary plus zero or more read replicas. Every shard
  carries the **full schema**; rows are distributed, not tables.
- Shards are configured with `MYSQL_SHARDS`: `;`-separated shard specs, each a
  `|`-separated host list whose first entry is the primary.
- A store operation routes to exactly one shard when it knows the row's shard
  key; otherwise it fans out.

Routing keys:

| Repository | Shard key |
|---|---|
| `users` | `id` (public-id lookups fan out) |
| `sessions` | session `id` |
| `devices` | device `id` |
| `conversations`, `conversation_members`, `conversation_sequences`, `messages`, `outbox` | `conversation_id` |
| `contacts`, `blocks` | `owner_id` |
| `friend_requests` | request `id` |
| `groups`, `group_members` | `group_id` |
| `attachments` | attachment `id` |
| `plugin_states` | shard 0 (global, low volume) |

A message and its outbox row always share the conversation shard, so the
transactional outbox still commits in one transaction (ADR 0004).

Keys are **mixed** (murmur finalizer) before modulo. Snowflake ids keep their
low bits zero for the first id of each millisecond, so `id % n` would hot-spot
on shard 0. String keys use FNV-1a.

Fan-out reads run every shard concurrently and merge:

- "first match" lookups (`get_by_user_id`, `find_direct`, `find_by_client_id`,
  `get_by_conversation`) return the first hit.
- List queries (`list_for_user`, `pending`) concatenate and re-sort by their
  documented order, then truncate.
- Fan-out writes (`revoke_for_device`, `mark_published`) are idempotent and
  apply to every shard.

Filename search normally joins `messages` to `attachments`, but those tables
have different shard keys. The dispatcher first collects matching attachment
ids from every shard, then filters messages by `attachment_id IN (...)` so the
message query stays shard-local.

### Read replicas

Reads are spread round-robin across a shard's replicas and fall back to the
primary when none are configured. Writes always target the primary. Read-your-
writes is preserved by reading through the primary for the write path; automatic
lag detection and replica ejection remain future work.

### Redis Cluster

- `REDIS_URL` selects a single node; `REDIS_CLUSTER_NODES` selects a cluster
  (comma-separated seed URLs) and takes precedence.
- Commands run over a `RedisConn`, a small enum over `ConnectionManager` and
  async `ClusterConnection`, so every port implementation is shared.
- The connection registry stores one **hash per user** (`conn:{user_id}`) rather
  than per-connection keys. `SCAN` cannot enumerate a cluster reliably, while a
  single hash is served by one slot. Stale fields are filtered by heartbeat
  time.
- Pub/sub always runs over a plain connection to one seed node. A Redis Cluster
  forwards published messages to every node, so a subscriber on one node sees
  cluster-wide traffic; `PUBLISH` itself uses the cluster connection.

## Consequences

- The number of shards is fixed at deployment. Changing it remaps every key and
  requires an offline resharding job; this is acceptable at the current stage.
- Fan-out queries cost one round trip per shard. They are confined to lookups
  without a routing key and are documented above.
- Cross-shard list ordering is done in the application; per-shard `LIMIT`
  results merge into the global order.
- No foreign keys or distributed transactions span shards. Invariants are kept
  within a shard; cross-shard effects rely on the outbox and idempotency.

## Alternatives considered

- **Table partitioning inside one MySQL server** — rejected; it does not add
  write capacity across machines.
- **A routing directory service** — rejected for now; it adds a critical
  dependency and its own consistency problem before it is needed.
- **Consistent hashing with virtual nodes** — deferred; modulo over a fixed
  shard count is simpler and sufficient until online resharding exists.
- **Redis Cluster only** — rejected; single-node Redis stays supported for
  development and small deployments.
