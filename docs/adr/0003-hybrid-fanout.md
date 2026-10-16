# ADR 0003 — Hybrid Push/Pull Fan-out

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

Fan-out-on-write (push) gives the lowest delivery latency for small groups but
write amplification grows with membership: a 100k-member group would require
100k writes per message. Fan-out-on-read (pull) avoids write amplification but
adds latency and complicates delivery for online users.

## Decision

Use a **hybrid** policy keyed on group size:

- **Small groups** fan out on write. The sender's commit produces one delivery
  event; workers resolve recipients from the cached membership and push.
- **Large groups/channels** use a shared timeline with a **per-device cursor**.
  Members receive a lightweight push hint and pull the timeline by cursor. On
  reconnect, offline devices catch up by cursor.

The size threshold is configuration (`FANOUT_WRITE_MAX_MEMBERS`, default 256).

## Consequences

- Small groups keep push latency.
- Large groups bound write amplification independent of membership.
- Clients must support both a live push path and a cursor-based catch-up path.

## Alternatives considered

- **Push-only** — rejected; write amplification is unbounded.
- **Pull-only** — rejected; online delivery latency suffers and presence is
  harder to exploit.
