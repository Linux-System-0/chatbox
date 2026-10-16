# ADR 0002 — Rust + Axum + SQLx Backend

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

The backend must handle many concurrent WebSocket and HTTP connections without
blocking, stay memory-safe under adversarial input, and scale horizontally.
MySQL is the required primary datastore and Redis provides cache and pub/sub.

## Decision

- Implement the backend as a Rust workspace with a strict dependency direction
  (`core ← storage`, `core ← api/realtime/workers`).
- Use **Tokio** as the async runtime and **Axum** for HTTP and WebSocket
  termination.
- Use **SQLx** with runtime-checked queries against MySQL. Queries are not
  compile-time checked against a live database so the workspace builds in CI
  without a provisioned database; the in-memory adapters and integration tests
  cover behavior.
- Define storage behind **ports (traits)** in `core` so the in-memory, MySQL,
  and Redis adapters are interchangeable.

## Consequences

- A single language across the backend simplifies tooling and review.
- The domain is testable without a database.
- Runtime-checked SQL means schema/query mismatches surface in integration
  tests rather than at compile time; migrations and integration tests are
  therefore mandatory.

## Alternatives considered

- **Compile-time-checked SQLx macros** — rejected for CI friction; can be
  adopted later behind `SQLX_OFFLINE` cache.
- **A blocking ORM** — rejected; blocking the reactor is a first-class
  anti-pattern for this workload.
