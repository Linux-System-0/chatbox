# ADR 0004 — Transactional Outbox for Delivery

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

Writing a message to MySQL and publishing a delivery event to Redis Streams are
two different systems. A naive "write then publish" can lose messages when the
publish fails after the commit, or publish events for writes that rolled back.

## Decision

Use the **transactional outbox** pattern:

1. In one short transaction, append the message row and an `outbox` row that
   records the delivery intent.
2. A relay process reads unpublished outbox rows in order, publishes them to
   the durable stream, and marks them published.
3. The relay is idempotent: consumers de-duplicate by message id, and the
   `client_msg_id` makes client retries idempotent.

The outbox row lives on the same shard as the message, so no cross-shard
two-phase commit is required.

## Consequences

- No lost or phantom delivery events.
- The sender's ack does not depend on the broker being available.
- At-least-once delivery is explicit; exactly-once is achieved for effects via
  idempotent writers.

## Alternatives considered

- **Two-phase commit across MySQL and the broker** — rejected; latency and
  operational complexity destroy availability.
- **Publish directly, no outbox** — rejected; loses the atomicity guarantee.
