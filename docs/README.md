# ChatBox Design Documentation

This directory holds the design documents and Architecture Decision Records
(ADRs) for ChatBox. Per `AGENTS.md` §6, documentation is written **before** the
behavior it describes and kept in sync with the code.

## Index

| Document | Description |
|---|---|
| [`architecture.md`](architecture.md) | Component overview, scaling planes, and request flow |
| [`data-model.md`](data-model.md) | Domain entities, identifiers, and storage layout |
| [`api.md`](api.md) | HTTP/REST control-plane contract |
| [`realtime-protocol.md`](realtime-protocol.md) | WebSocket data-plane framing and lifecycle |
| [`plugin-system.md`](plugin-system.md) | Client vs. server plugin model |
| [`web-client.md`](web-client.md) | Web UI structure, design tokens, themes, and accessibility |
| [`android-network-security.md`](android-network-security.md) | Android server setup, host matching, and runtime TLS trust |
| [`operations.md`](operations.md) | Configuration, deployment, migrations, and health |

## Architecture Decision Records

| ADR | Title | Status |
|---|---|---|
| [0001](adr/0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](adr/0002-rust-axum-sqlx-backend.md) | Rust + Axum + SQLx backend | Accepted |
| [0003](adr/0003-hybrid-fanout.md) | Hybrid push/pull fan-out | Accepted |
| [0004](adr/0004-transactional-outbox.md) | Transactional outbox for delivery | Accepted |
| [0005](adr/0005-plugin-tiers.md) | Two-tier plugin model | Accepted |
| [0006](adr/0006-mysql-sharding.md) | MySQL sharding, replicas, and Redis Cluster | Accepted |
| [0007](adr/0007-server-plugin-host-api.md) | Server plugin host API and isolated hooks | Accepted |
| [0008](adr/0008-android-runtime-network-trust.md) | Runtime network trust in the Android client | Accepted |

## Conventions

- English only (see `AGENTS.md` §1).
- Code identifiers in the implementation match the names used here.
- When a document and the code disagree, fix the document in the same change.
