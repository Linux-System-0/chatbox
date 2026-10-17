# Operations

## Configuration

Configuration comes from environment variables (optionally loaded from
`deploy/.env`). See `deploy/.env.example` for the full list.

| Variable | Description | Default |
|---|---|---|
| `DATABASE_URL` | MySQL primary connection string | in-memory backend when unset |
| `DATABASE_REPLICA_URL` | MySQL read replica (optional) | — |
| `REDIS_URL` | Redis connection string | — |
| `REDIS_ENABLED` | Toggle Redis usage | `true` |
| `JWT_SECRET` | Secret for signing auth tokens | required in production |
| `HTTP_BIND` | HTTP listen address | `0.0.0.0:8080` |
| `RUST_LOG` | Log level (`error`…`trace`) | `info` |
| `PLUGIN_DIR` | Directory scanned for server plugin manifests | `plugins` |
| `OBJECT_STORAGE_DIR` | Local object storage root for development | `data/objects` |

When `DATABASE_URL` is unset the server starts with the in-memory storage
backend. This is intended for local development and tests only; it does not
persist across restarts.

## Storage backends

- **MySQL** — the required production datastore. Apply migrations from
  `deploy/sql/migrations` before starting a node.
- **Redis** — optional cache, presence store, connection registry, rate-limit
  counters, and fan-out transport. When `REDIS_ENABLED=false` the in-process
  adapters are used.
- **In-memory** — used by tests and by `DATABASE_URL`-less development.

## Local development

When `DATABASE_URL` is unset the server selects the **in-memory** backend, so
the API and WebSocket gateway run without MySQL or Redis:

```bash
cd backend
cargo run                      # listens on 0.0.0.0:8080, in-memory state
```

A Node-based smoke test exercises registration, WebSocket authentication, REST
and WebSocket sends, fan-out delivery, history, and search against a running
node:

```bash
cargo run &                    # start a node
node scripts/e2e-smoke.mjs http://127.0.0.1:8080
```

The Rust test suite covers the domain services over the in-memory backend:

```bash
cd backend
cargo test --workspace
```

## Database migrations

Migrations are plain SQL files named `NNNN_description.sql` and applied in
order. `sqlx::migrate!` drives them when the MySQL backend is selected; the
same files can be applied by hand or by an external migration tool.

## Health and observability

- `GET /api/v1/health/live` — process liveness; never touches dependencies.
- `GET /api/v1/health/ready` — dependency readiness (MySQL, Redis).
- Metrics follow the RED/USE model: request rate, errors, duration; resource
  utilization, saturation, errors. Structured logs are emitted with `tracing`.

## Backpressure policy

- Per-connection outbound queues are bounded. When a queue is full the gateway
  drops the oldest queued frame; if the condition persists it disconnects the
  slow client.
- Fan-out and outbox workers use bounded channels. Producers apply backpressure
  instead of growing memory unbounded.

## Deployment

`deploy/docker/docker-compose.yml` provisions MySQL, Redis, and an application
node for local evaluation. In production, run the stateless application and
gateway nodes behind an L7 load balancer and a separate WebSocket load
balancer, backed by a MySQL primary/replica cluster and a Redis cluster.
