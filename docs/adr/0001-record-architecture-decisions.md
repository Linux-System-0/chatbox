# ADR 0001 — Record Architecture Decisions

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

ChatBox makes a number of non-obvious architectural choices (independently
scaled control/data planes, hybrid fan-out, transactional outbox, two-tier
plugins). Without a written record, the reasoning behind these choices is lost
and contributors may "simplify" them back into the naive designs the project
deliberately avoids.

## Decision

We record every architecturally significant decision as a numbered ADR under
`docs/adr/`. Each ADR states the context, the decision, the consequences, and
the alternatives considered. ADRs are immutable once accepted; a later decision
supersedes an earlier one with a new ADR rather than editing history.

## Consequences

- The rationale is reviewable before code is written (docs-first workflow).
- Superseding decisions are traceable.
- Contributors must read the relevant ADRs before changing core behavior.

## Alternatives considered

- **No formal record** — rejected; rationale would live only in chat logs.
- **Wiki pages** — rejected; they drift from the code and are not versioned
  with the change that motivated them.
