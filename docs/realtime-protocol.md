# Realtime Protocol (Data Plane)

The realtime gateway terminates WebSocket connections at `GET /ws`. Frames are
JSON text messages with a `type` discriminator. Binary frames are reserved for
future use.

## Lifecycle

1. **Connect** — the client opens `wss://.../ws`.
2. **Authenticate** — the client sends `auth` with an access token and a
   `device_id`. The gateway validates the token, registers the connection in
   the connection registry, and replies `ready` with the server time.
3. **Exchange** — the client sends `send` and `ack` frames; the server pushes
   `message` frames.
4. **Heartbeat** — the client sends `ping` periodically; the server replies
   `pong` and refreshes the registry TTL.
5. **Resume** — after a reconnect the client sends `resume` with its per-
   conversation cursors; the server replays the gap from the durable store.

## Client → Server

```jsonc
{ "type": "auth", "token": "<jwt>", "device_id": 123 }
{ "type": "send", "client_msg_id": "uuid", "conversation_id": 42,
  "kind": "text", "content": "hello @bob" }
{ "type": "send", "client_msg_id": "uuid", "conversation_id": 42,
  "kind": "file", "attachment_id": 777 }
{ "type": "ack", "conversation_id": 42, "seq": 128 }
{ "type": "resume", "cursors": { "42": 100, "43": 57 } }
{ "type": "ping" }
```

## Server → Client

```jsonc
{ "type": "ready", "server_time": "2026-01-01T00:00:00Z" }
{ "type": "ack", "client_msg_id": "uuid", "conversation_id": 42, "seq": 129,
  "message_id": 9001 }
{ "type": "message", "message": { /* Message */ } }
{ "type": "presence", "user_id": 7, "online": true, "devices": 2 }
{ "type": "pong" }
{ "type": "error", "code": "rate_limited", "message": "..." }
```

## Ordering and delivery guarantees

- Every `message` carries the conversation `seq`. Clients apply frames in `seq`
  order and request a re-sync when they detect a gap.
- Delivery is **at-least-once**. `client_msg_id` makes retries idempotent.
- Outbound queues are **bounded per connection**. Under backpressure the
  gateway drops the oldest queued frame or disconnects the slow client rather
  than growing memory without limit.
- Offline messages are persisted and delivered on reconnect via `resume`.

## Routing

The gateway maintains the connection registry in Redis:

```text
conn:{user_id} = { (node, device_id, conn_id, last_seen) }
```

Fan-out workers resolve the set of live `(node, conn_id)` targets and publish a
targeted delivery event, so a message is never broadcast to every node.
