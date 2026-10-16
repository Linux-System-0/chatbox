# ADR 0005 — Two-Tier Plugin Model

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

Users want to customize their client (translations, themes) without asking an
administrator, while operators want to extend server behavior (audit logging,
integrations) for everyone. Conflating the two would let a client-side install
change server behavior, which is both a security and a governance problem.

## Decision

Split plugins into two explicit tiers:

- **Client plugins** are user-installed, applied globally on that client, and
  have **no server impact**. The backend only serves their manifests.
- **Server plugins** are administrator-installed, must be deployed **and
  enabled** on the server, and may affect all users. Clients cannot install
  them.

The tiers are distinguished in the manifest by `kind` and are validated by the
respective runtimes.

## Consequences

- The threat model is clear: client plugins cannot escalate to the server.
- Operators retain full control over server behavior.
- Two runtimes must be documented and kept distinct (see
  `plugin-system.md`).

## Alternatives considered

- **Single unified plugin model** — rejected; it would require granting clients
  server-level capability or silently ignoring the distinction, both unsafe.
- **Server-only plugins** — rejected; users could not install language packs or
  themes on their own.
