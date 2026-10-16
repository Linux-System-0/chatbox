<div align="center">

# ChatBox

**A real-time, multi-user chat platform for Web and Android.**

[![Status](https://img.shields.io/badge/status-design%20%2F%20early--stage-orange)](#-project-status)
[![Web](https://img.shields.io/badge/Web-SvelteKit-FF3E00?logo=svelte&logoColor=white)](https://kit.svelte.dev/)
[![Android](https://img.shields.io/badge/Android-Kotlin-7F52FF?logo=kotlin&logoColor=white)](https://kotlinlang.org/)
[![Backend](https://img.shields.io/badge/Backend-Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Database](https://img.shields.io/badge/Database-MySQL-4479A1?logo=mysql&logoColor=white)](https://www.mysql.com/)
[![Cache](https://img.shields.io/badge/Cache-Redis-DC382D?logo=redis&logoColor=white)](https://redis.io/)
[![License](https://img.shields.io/badge/License-GPLv3-blue?logo=gnu&logoColor=white)](LICENSE)

English · [简体中文](README_zh.md)

</div>

---

## 📖 Overview

**ChatBox** is an open, extensible instant-messaging platform designed for both
**one-to-one** and **group** conversations. It targets a modern, cross-platform
experience: a **SvelteKit** web client, a native **Kotlin** Android client, and a
high-performance **Rust** backend that scales horizontally behind a cluster of
**MySQL** and **Redis** nodes.

The project is designed with production concerns from day one — concurrency,
timeouts, database locking, replication lag, failover, cache consistency, and
cache-avalanche protection are treated as first-class requirements rather than
afterthoughts.

> **Default language is English.** All code comments, identifiers, commit
> messages, and documentation are written in English to keep the project
> accessible to an international community.

---

## 🚧 Project Status

ChatBox is currently in the **design / early-development** stage. The
architecture, feature set, and engineering guidelines in this document describe
the target design; concrete components will land incrementally (see the
Roadmap section). Interfaces and directory layout may change before the
first stable release.

---

## ✨ Features

### 💬 Messaging
- **Single chat** — send either **text** or **files**.
- **No online preview** — every attachment is delivered as a downloadable file;
  the platform intentionally does not render file previews.
- **Offline messages** — messages are stored and delivered when the recipient
  reconnects.
- **Multi-device** — the same account can be signed in on several devices at the
  same time, with messages synchronized across all of them.
- **Message mentions** — `@` users inside group chats.

### 🔎 Search
- **Type search** — for files only, by filename with **wildcard** support
  (e.g. `*.mp4`).
- **Time search** — filter by time range, precise **to the minute**.
- **Content search** — full-text search across text messages.
- **Sorting** — results can be ordered by time **ascending or descending**.
- **Jump to message** — clicking a result returns to the conversation and
  automatically scrolls to and highlights that exact message.

### 👥 Contacts & Identity
- **ID login with password verification.**
- **Custom ID** — user-chosen, with uniqueness / conflict checking.
- **Custom display name** — falls back to the user ID when no name is set.
- **Remarks / nicknames** — per-contact aliases.
- **Add contacts by ID** — sends a friend request with an optional introduction
  message; the recipient must **accept** before the contact is added.

### 🗣️ Group chat
Three roles: **Owner**, **Admin**, and **Member**.

| Capability | Owner | Admin | Member |
|---|:---:|:---:|:---:|
| Send messages (unless muted) | ✅ | ✅ | ✅ |
| Rename group | ✅ | ✅ | ❌ |
| Restrict who can invite members | ✅ | ✅ | ❌ |
| Mute members | ✅ | ✅ | ❌ |
| Kick / add members | ✅ | ✅ | ❌ |
| Transfer ownership | ✅ | ❌ | ❌ |
| Dissolve group | ✅ | ❌ | ❌ |

- Only **one** owner per group at a time.
- The owner has all admin permissions **plus** ownership transfer and group
  dissolution.

### 🚫 Direct chat controls
- **Block / unblock** users in one-to-one conversations.

### 🔌 Extensibility
- **Plugin system** — extend ChatBox with new languages, features, and
  integrations without modifying the core.

---

## 🛠️ Tech Stack

| Layer | Technology | Notes |
|---|---|---|
| Web client | **SvelteKit** | SSR + SPA, WebSocket for realtime |
| Android client | **Kotlin** | Native Android |
| Backend | **Rust** | Async, memory-safe, horizontally scalable |
| Primary datastore | **MySQL** | **Required**; cluster-ready (primary/replica) |
| Cache / pub-sub | **Redis** | Cluster-ready; can be disabled in configuration |

---

## 🏗️ Architecture

ChatBox separates the **control plane** (HTTP/REST) from the **data plane**
(WebSocket realtime) so each scales independently and a slow client can never
stall request/response traffic. Application nodes are stateless; all shared
state lives in Redis, MySQL, and object storage.

```mermaid
graph TD
    subgraph Clients
        WEB[SvelteKit Web]
        AND[Kotlin Android]
    end

    WEB -->|HTTPS| LB[L7 API Gateway]
    AND -->|HTTPS| LB
    WEB -->|WSS| WSLB[WebSocket LB]
    AND -->|WSS| WSLB

    LB --> API1[API node 1]
    LB --> APIN[API node N]

    WSLB --> GW1[Gateway node 1]
    WSLB --> GWN[Gateway node N]

    API1 & APIN & GW1 & GWN --> REDIS[(Redis Cluster)]
    API1 & APIN --> MQ[(Durable Stream / Broker)]
    MQ --> FAN[Fan-out Workers]
    FAN --> GW1 & GWN
    GW1 & GWN --> MQ
    API1 & APIN & FAN --> PRIMARY[(MySQL Primary)]
    API1 & APIN & GW1 & GWN --> REPLICA[(MySQL Replicas)]
    PRIMARY -->|async replication| REPLICA
    API1 --> OBJ[(Object Storage)]
```

**Key ideas**

- **Independent scaling planes.** HTTP API and WebSocket gateways scale
  separately; a flash crowd of connections does not consume REST capacity.
- **Stateless nodes + connection registry.** Any node can serve any request
  (JWT / Redis sessions). Live connections are tracked in Redis
  (`conn:{user_id} -> {(node, device, conn_id, last_seen)}`) with TTL
  heartbeats, so a message is routed to the exact owning gateway instead of
  being broadcast to every node.
- **Send and deliver are decoupled.** Writes commit to MySQL through the
  transactional **outbox** pattern, then publish to a durable stream. Fan-out
  workers deliver to gateways. A failed or slow consumer never blocks the
  sender.
- **Durable, at-least-once delivery.** Every message carries a client-generated
  idempotency key (`client_msg_id`) and receivers de-duplicate. Exactly-once
  is achieved for *effects* through idempotent writers — not by pretending the
  network never fails.
- **Per-conversation ordering.** A monotonic sequence number per conversation
  lets clients order, detect gaps, and request a re-sync without any global
  lock.
- **Hybrid fan-out.** Small groups fan out on write (push); very large groups
  and channels fan out on read (shared timeline + per-device cursor) to avoid
  write amplification. Online members get a push hint; offline devices catch up
  by cursor on reconnect.
- **Write to primary, read from replicas** with read-your-writes routing and
  explicit replication-lag awareness.
- **Redis** holds cache, presence, the connection registry, rate-limit
  counters, and the fan-out transport (Streams); distributed locks are a last
  resort.
- **Direct-to-object-storage uploads** via pre-signed URLs keep large file
  transfers off the API nodes.

---

## 📁 Repository Layout (planned)

```text
chatbox/
├── backend/            # Rust workspace (API, realtime gateway, workers)
│   ├── crates/
│   │   ├── api/        # HTTP/REST handlers
│   │   ├── realtime/   # WebSocket gateway
│   │   ├── workers/    # Fan-out, outbox relay, background jobs
│   │   ├── core/       # Domain logic, services
│   │   ├── storage/    # MySQL / Redis data access
│   │   └── plugin/     # Plugin runtime & interfaces
│   └── Cargo.toml
├── web/                # SvelteKit web client
├── android/            # Kotlin Android client
├── deploy/             # Docker Compose, migrations, cluster configs
│   ├── docker/
│   └── sql/
├── docs/               # Design docs and ADRs
└── README.md
```

> ⚠️ The tree above describes the intended structure. The repository is in its
> design / early stage — see [Project Status](#-project-status).

---

## 🚀 Getting Started

### Prerequisites

| Tool | Version (recommended) |
|---|---|
| Rust | stable (2024 edition or newer) |
| Node.js | 20 LTS or newer |
| MySQL | 8.0+ (or compatible cluster) |
| Redis | 7.x+ (optional for local dev) |
| JDK + Android SDK | for building the Android client |

### 1. Clone

```bash
git clone https://github.com/Linux-System-0/chatbox.git
# 或 git clone https://codeberg.org/Linux-System-0/chatbox.git
cd chatbox
```

### 2. Configure

Copy the example environment file and edit it for your machine:

```bash
cp deploy/.env.example deploy/.env
```

| Variable | Description | Default |
|---|---|---|
| `DATABASE_URL` | MySQL primary connection string | `mysql://user:pass@127.0.0.1:3306/chatbox` |
| `DATABASE_REPLICA_URL` | MySQL read replica (optional) | — |
| `REDIS_URL` | Redis connection string | `redis://127.0.0.1:6379` |
| `REDIS_ENABLED` | Toggle Redis usage | `true` |
| `JWT_SECRET` | Secret for signing auth tokens | — |
| `HTTP_BIND` | HTTP listen address | `0.0.0.0:8080` |
| `RUST_LOG` | Log level (`error`…`trace`) | `info` |

### 3. Run the backend

```bash
cd backend
cargo run
```

### 4. Run the web client

```bash
cd web
npm install
npm run dev
```

### 5. Build the Android client

```bash
cd android
./gradlew assembleDebug
```

> The commands above reflect the intended developer workflow. They will become
> fully runnable as the corresponding components are implemented.

---

## ⚙️ Engineering Considerations

These non-functional requirements drive the architecture and are tracked as
first-class design concerns. The right-hand column is the **optimized** design;
it deliberately replaces naive approaches (such as locking a row for every read
receipt) with designs that stay correct under heavy concurrency.

| Concern | Failure mode at scale | Optimized approach |
|---|---|---|
| **Delivery semantics** | Retries create duplicate messages | At-least-once + `client_msg_id` idempotency key, dedup table / Redis `SETNX` with TTL, idempotent consumers |
| **Ordering** | Concurrent writers interleave a conversation | Per-conversation monotonic sequence (`INCR` / Snowflake + local order); clients detect gaps and re-sync |
| **Realtime fan-out** | Write amplification for huge groups | Hybrid push/pull: fan-out-on-write for small groups, fan-out-on-read cursor model for large ones |
| **Connection routing** | Broadcasting to every node wastes fan-out | Redis connection registry + targeted publish to the owning gateway |
| **Hot entities** | One popular group/user saturates a shard | Sharded counters, local caches, key replication, batched/coalesced writes, per-key work queues |
| **Database concurrency** | Lock waits, deadlocks, hot rows | Short transactions, ordered lock acquisition, optimistic version checks, no long-lived locks, avoid `SELECT ... FOR UPDATE` on hot paths |
| **Sharding & transactions** | Cross-shard 2PC destroys latency | Shard by `conversation_id` / `user_id`; keep invariants single-shard; use sagas/outbox across shards |
| **Replication lag** | Read-after-write returns stale data | Read-your-writes routing (sticky to primary briefly / LSN token), lag monitoring, fall back to primary on stale reads |
| **Replication health** | A silently stalled replica serves stale data | Heartbeats + lag thresholds; automatically eject lagging replicas from the read pool |
| **Failover** | Split-brain double writes | Automatic promotion with fencing tokens / leases; writers reject stale epochs |
| **Cache stampede** | Many misses regenerate one hot key at once | Single-flight / request coalescing per key, logical expiry + async refresh, per-key mutex |
| **Cache avalanche** | Mass simultaneous expiry overwhelms the DB | Jittered TTLs, warm-up on deploy, multi-level (in-process + Redis) cache, circuit breaker on the cache tier |
| **Cache penetration** | Queries for absent keys bypass the cache | Negative caching, bloom filter, validate before hitting storage |
| **Cache inconsistency** | DB and cache diverge | Cache-aside + invalidate-on-write (delete), delayed double-delete or CDC-based invalidation, versioned keys |
| **Cache hot keys** | One key exceeds a single Redis shard | Local cache in front, key replication, read from replicas, split hot keys |
| **Timeouts** | One slow dependency stalls the pool | Deadline propagation, per-hop budget, every `await` bounded, separate timeouts per dependency |
| **Retries** | Retry storms amplify an outage | Retry budget, exponential backoff with full jitter, only idempotent retries, jittered `Retry-After` |
| **Cascading failure** | One dependency takes down everything | Circuit breakers, bulkheads (isolated pools per dependency), load shedding, prioritized queues |
| **Rate limiting** | Traffic spikes exhaust capacity | Local token bucket + global Redis quota via Lua; lease quotas in chunks; per user/device/IP/conversation |
| **Rust concurrency** | Blocking the async reactor, lock contention | Tokio multi-thread; never block; actor-per-connection with bounded `mpsc`; sharded maps; `try_lock` with timeout; bounded channels for backpressure |
| **Memory / backpressure** | Slow consumers grow unbounded queues | Bounded per-connection outbound queues, drop-oldest / disconnect policy, end-to-end backpressure |
| **Observability** | Flying blind during incidents | RED/USE metrics, distributed tracing, latency histograms, saturation alarms, per-stage timers |

### Optimized message pipeline

1. **Accept** — the client sends with a `client_msg_id`; the API validates auth
   and rate limits.
2. **Persist** — one short transaction appends the message and an outbox row
   (same shard, no cross-shard work).
3. **Publish** — an outbox relay publishes to the durable stream; the sender
   gets an immediate ack with the assigned conversation `seq`.
4. **Fan-out** — workers resolve recipients (cached membership), apply the
   push/pull policy, and route via the connection registry.
5. **Deliver** — the owning gateways push over WebSocket with bounded queues;
   offline devices advance their cursor on reconnect.
6. **Ack** — per-device acks advance read cursors; receipts are coalesced and
   batched, never one row update per receipt.

```mermaid
sequenceDiagram
    participant C as Client
    participant API as API Node
    participant DB as MySQL Outbox
    participant MQ as Durable Stream
    participant F as Fan-out Worker
    participant GW as Gateway Node
    participant R as Redis Registry

    C->>API: send(client_msg_id)
    API->>DB: short tx: message + outbox
    API-->>C: ack(seq, client_msg_id)
    DB->>MQ: outbox relay publishes
    MQ->>F: delivery event
    F->>R: resolve live connections
    R-->>F: gateway + device + conn
    F->>GW: push to owning gateway
    GW-->>C: WebSocket frame
    C->>API: ack(device cursor)
```

---

## 🗺️ Roadmap

- [ ] Project scaffolding (backend workspace, web, android)
- [ ] Authentication: ID login, password verification, sessions, multi-device
- [ ] Direct messaging: text + file transfer, offline queue
- [ ] Contact system: add by ID, friend requests, remarks/nicknames
- [ ] Group chat: roles, permissions, `@` mentions, mute/kick/invite
- [ ] Search: type (wildcards), time range, content; sorting & jump-to-message
- [ ] Block list for direct chats
- [ ] Redis integration + clustering
- [ ] MySQL primary/replica + failover
- [ ] Plugin system (language packs, extensions)
- [ ] Hardening: observability, rate limiting, load testing

---

## 🤝 Contributing

Contributions are welcome once the initial scaffolding lands.

1. Fork the repository and create a feature branch:
   `git checkout -b feat/your-feature`.
2. Follow the **English-first** convention for code, comments, and docs.
3. Keep commits small and use clear, conventional messages.
4. Add tests where practical.
5. Open a pull request describing the change and its motivation.

---

## 📄 License

This project is licensed under the **GNU General Public License v3.0 only**
(GPL-3.0-only). See the [`LICENSE`](LICENSE) file for the full text.

---

## 🙏 Acknowledgements

Built with [SvelteKit](https://kit.svelte.dev/), [Kotlin](https://kotlinlang.org/),
[Rust](https://www.rust-lang.org/), [MySQL](https://www.mysql.com/), and
[Redis](https://redis.io/).