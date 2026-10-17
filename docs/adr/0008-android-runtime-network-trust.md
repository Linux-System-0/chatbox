# ADR 0008 — Runtime Network Trust in the Android Client

- **Status:** Accepted
- **Date:** 2026-10-17

## Context

The Android client was built around a compile-time network security config that
trusted the system CAs and allowed cleartext only to `10.0.2.2`, `localhost`,
and `127.0.0.1`. The backend address was a `BuildConfig` constant. That works
for emulator development and nothing else: self-hosted deployments use LAN
addresses or private CAs, and users need to point the app at their own server.

Android's network security config cannot help here. It is a build-time XML
resource, and `usesCleartextTraffic` is ignored whenever it is present. A
runtime-configurable trust policy therefore requires the opposite arrangement:
let the platform permit cleartext and validate TLS in the application's own
networking stack. The feature also must not fork into debug and release
behavior, because the same APK is expected to work in both roles.

## Decision

1. Remove `android:networkSecurityConfig` and
   `res/xml/network_security_config.xml`; set
   `android:usesCleartextTraffic="true"`.
2. Move all trust decisions into one shared OkHttp client used by Retrofit, the
   WebSocket, and the connectivity probe:
   - trust anchors: system CAs (toggle) plus a user-managed CA list;
   - cleartext: allowed only for user-listed DNS patterns, IPs, or CIDR blocks;
   - "disable verification": no chain check, no hostname check, any cleartext
     host. This is an explicit, confirmed, visibly flagged mode.
3. Resolve policy in one order: verification disabled → allow everything;
   otherwise `http` requires an allowlist match and `https` requires a valid
   chain and hostname.
4. Ask the user for the server as `host[:port]` plus an `http`/`https` choice
   before login, validate and normalize it, probe
   `/api/v1/health/live` with the active policy, and show a one-time
   certificate fingerprint confirmation on the first HTTPS connection to an
   origin.
5. Bind sessions to their origin; changing the server logs out.
6. Keep a single policy for all build types. No debug-only relaxation.

Exact input rules, matching grammar, dialogs, and storage are specified in
[`android-network-security.md`](../android-network-security.md).

## Consequences

Positive:

- One APK serves emulator development, LAN testing, and private-CA production
  deployments.
- Users can see and control exactly what is trusted; insecure modes require
  informed consent and stay visible while active.
- The policy is testable in isolation (matcher, trust manager composition,
  policy order) instead of being an opaque build artifact.

Negative:

- Platform-level cleartext blocking is gone. Enforcement depends on every
  request going through the shared client; a new raw `OkHttpClient` would
  bypass the policy. CI checks and code review must guard this.
- The client now contains trust-all code paths. They are guarded by
  confirmation and a persistent warning, but they exist in release builds by
  design.
- First connection to an HTTPS origin needs a fingerprint confirmation, which
  adds friction that a compiled-in CA would not have.
- `Session` becomes origin-scoped, so existing sessions are invalidated once.

## Alternatives considered

- **Build-type source sets (debug permissive, release strict).** Rejected: it
  cannot support a user-configured self-hosted server in a release build, and it
  reintroduces the debug/release split this decision removes.
- **Manifest placeholder selecting one of two XML policies.** Rejected: still
  compile-time, still requires a rebuild per deployment, and the effective
  policy becomes invisible in review.
- **Keep the static config and require users to rebuild the APK.** Rejected:
  the client is distributed as a binary; requiring a toolchain to change servers
  is not acceptable.
- **Trust only user-installed CAs via `src="user"`.** Rejected: it still cannot
  express cleartext host rules or a per-origin fingerprint confirmation, and it
  silently trusts every CA the device owner installed.
- **Certificate pinning only.** Rejected as the sole mechanism: self-hosted
  deployments rotate certificates, and pinning does not address cleartext or
  first-contact trust. It remains a possible future option.
