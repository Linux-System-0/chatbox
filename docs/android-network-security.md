# Android Network Configuration and Trust

Status: **implemented** — the Android client enforces the policy described here.
This document is the specification and is kept in sync with the code.

The Android client must work against arbitrary self-hosted ChatBox backends:
plain HTTP on a LAN address, HTTPS with a private CA, or a public HTTPS
deployment. The server address is therefore chosen by the user before login, and
the trust policy is configured at runtime instead of being compiled in.

This document defines the input rules, host matching, TLS trust model, consent
flow, and storage for that feature. Protocol contracts live in
[`api.md`](api.md) and [`realtime-protocol.md`](realtime-protocol.md); the
decision record is [ADR 0008](adr/0008-android-runtime-network-trust.md).

## 1. Goals

- One Android build works for every deployment; there is **no debug/release
  policy split**.
- The user enters `host[:port]` and picks `http` or `https` before logging in.
- The user controls the trust anchors and the cleartext allowlist at runtime.
- Insecure modes are explicit, informed, visible, and reversible.
- Every network path (REST, WebSocket, probe) is governed by one policy.

## 2. Non-goals

- Defending against a rooted device, a compromised OS, or a malicious
  accessibility service.
- Per-request trust prompts or a general-purpose proxy configuration.
- WebView or third-party SDK traffic; those do not exist in the client today.
- Hiding the policy from the user for "security"; the user is the trust
  decision maker.

## 3. Platform constraint

`android:networkSecurityConfig` points at a **compile-time XML resource**. It
cannot be changed at runtime, and Android ignores
`android:usesCleartextTraffic` when a network security config is present. A
runtime-configurable client therefore cannot rely on it.

Consequences:

- The manifest sets `android:usesCleartextTraffic="true"` and the
  `networkSecurityConfig` attribute plus `res/xml/network_security_config.xml`
  are removed. The platform no longer blocks cleartext; the app does.
- All policy enforcement moves into the shared OkHttp client used by Retrofit
  and the WebSocket. A request that bypasses that client bypasses the policy.
- CI must reject new raw `OkHttpClient()` constructions so future networking
  cannot silently skip the policy (see §18).

This trades platform defense-in-depth for runtime flexibility. The trade is
deliberate and recorded in the ADR.

## 4. Threat model

Assets: the account password, access and refresh tokens, message content, the
contact list, and the identity of the server the user believes they are talking
to.

Adversaries considered:

| Adversary | Example |
|---|---|
| Passive observer | Someone on the same Wi-Fi reading cleartext HTTP |
| Active MITM | Hostile proxy or a CA the device trusts |
| Lookalike server | Typo-squatted domain or a different LAN host |
| Social engineering | A user persuaded to import an attacker CA or disable verification |

Out of scope: rooted devices, compromised operating systems or keyboards,
malicious apps holding the user's unlocked device.

Design responses: HTTPS with system CAs by default, one-time consent before any
cleartext host is allowed, a first-connection certificate fingerprint
confirmation, visible CA fingerprints, a guarded "disable verification" switch,
and an always-visible server origin.

## 5. Server address setup

### 5.1 State machine

```text
EnterAddress
  → Validate (format, normalization)
  → ApplyPolicy (scheme; cleartext consent if needed)
  → Probe (health endpoint)
  → FingerprintConfirm (HTTPS only, first connection to this origin)
  → Login
```

The address and policy are persisted. Returning users skip straight to login,
but the active origin stays visible on the login and chat screens. Changing the
server logs the user out and clears the stored session (§13).

The settings and CA dialogs are reachable from this screen, because a private
CA must be importable before a probe can succeed.

### 5.2 Input grammar

```text
address := host [ ":" port ]
host    := reg-name | ipv4 | "[" ipv6 "]"
```

- The port is optional: `80` for `http`, `443` for `https`.
- The scheme is chosen with an explicit `http` / `https` toggle.
- If the pasted text starts with `http://` or `https://`, the prefix is stripped
  and the toggle is synced to it.
- Rejected: path, query, fragment, userinfo (`user@host`), whitespace, empty
  host, or a port outside `1..65535`.
- The normalized form `scheme://host[:port]` is displayed below the field so the
  user can see exactly what will be contacted.

Normalization (applied to the stored server address and to every host match):

- lowercase;
- IDNA/punycode for internationalized names;
- strip a trailing dot;
- canonical bracket form for IPv6 (`[::1]`, `[::1]:8080`; a bare IPv6 literal
  is accepted and bracketed, but a bare IPv6 literal cannot carry a port).

## 6. Host matching

The cleartext allowlist holds two kinds of entries: DNS patterns and IP
addresses/CIDR blocks. Matching is on the host only; the port is never part of a
match.

### 6.1 DNS patterns

- `*` matches **one label**: one or more characters that are not `.`.
- `**` matches **one or more labels**.
- Matching is anchored to the whole host, case-insensitive, and ignores a
  trailing dot.
- A pattern must contain at least one literal label; patterns consisting only of
  wildcards (`*`, `**`) are rejected, because they are equivalent to allowing
  all cleartext and must not be expressible as a host entry.
- Wildcards are never valid in IP entries.

| Pattern | Host | Match |
|---|---|---|
| `*.example.com` | `a.example.com` | yes |
| `*.example.com` | `a.b.example.com` | no |
| `*.example.com` | `example.com` | no |
| `**.example.com` | `a.example.com` | yes |
| `**.example.com` | `a.b.example.com` | yes |
| `**.example.com` | `example.com` | no |
| `chat-*.example.com` | `chat-eu.example.com` | yes |
| `*.example.com` | `evil-example.com` | no |

The last row is the reason matching is label-based and anchored: a suffix or
substring comparison would allow `evil-example.com`.

### 6.2 IP and CIDR

- Exact IPv4 and IPv6 literals are matched literally.
- CIDR blocks are supported for IPv4 and IPv6 (`10.0.0.0/8`, `fd00::/8`).
  Host bits are masked automatically: `10.0.0.5/8` is stored as `10.0.0.0/8`.
- A CIDR entry matches **literal IP hosts only**. It is never applied to the
  resolved address of a domain name. Resolving at request time would make the
  policy depend on DNS and expose a DNS-rebinding bypass; a user who wants to
  allow a name adds a DNS pattern instead.
- At entry time the user chooses between an exact IP and a CIDR block; both are
  explicit user actions.

## 7. Policy resolution

The policy is evaluated for every request and every redirect hop:

```text
1. verification disabled
     → allow any scheme, any host, any certificate, any hostname

2. cleartext (http / ws)
     → allowed only when the host matches the allowlist

3. TLS (https / wss)
     → chain validated against (system CAs if enabled) ∪ custom CAs
     → hostname verification required
```

Defaults: system CAs **on**, custom CA list empty, cleartext allowlist empty,
verification **on**.

## 8. TLS implementation

- Custom CAs are loaded into a `KeyStore`; a delegating `X509TrustManager` tries
  the system trust manager and the custom one and fails only when both reject
  the chain. Exceptions are never swallowed to "make it work".
- Disabling verification installs a no-op trust manager **and** a permissive
  hostname verifier; replacing only one still fails on name mismatches.
- The peer chain is captured even when validation fails so the fingerprint
  dialog can show what was presented.
- Imported certificates: PEM or DER, parsed with `CertificateFactory`, at most
  10 certificates and 64 KiB per file. A file containing a private key
  (`PRIVATE KEY`) is rejected. Both CA certificates and self-signed server
  certificates are accepted as anchors.
- The list shows subject, issuer, validity window, and SHA-256 fingerprint.

## 9. First-connection confirmation

Applies only when verification is enabled and the scheme is `https`.

- First connection to an origin: show the presented certificate's SHA-256
  fingerprint, subject, issuer, and validity, and require an explicit confirm.
  The dialog is not dismissible by tapping outside.
- The confirmed fingerprint is stored per origin.
- If the fingerprint later changes for the same origin, show a warning that
  distinguishes certificate rotation from a possible MITM and require a new
  confirmation.
- If the chain cannot be validated, the failure screen offers *Cancel*, *Trust
  this certificate* (adds it to the custom CA list, then retries), and *Open
  network settings*.
- With verification disabled, no fingerprint prompt is shown; the user already
  accepted the risk when disabling verification.

## 10. Cleartext consent

Selecting `http` for an origin that is not in the allowlist shows a one-time
confirmation. On confirm, the host is added to the allowlist and the flow
continues; subsequent connections to that host do not ask again.

The dialog states that credentials, tokens, and message content will travel
unencrypted, and that `ws://` inherits the same exposure. With verification
disabled, no dialog is shown because everything is already permitted.

Allowlist entries are only ever created by explicit user action.

## 11. Redirects

Redirects are allowed. Because OkHttp follows them automatically, the policy
check runs in a **network interceptor**, so every hop is evaluated:

- a redirected `http` hop must match the allowlist;
- a redirected `https` hop uses the configured trust anchors;
- an `https → http` downgrade is blocked unless the target host is allowlisted;
- `Authorization` must not be forwarded across origins (covered by a test).

## 12. Connectivity probe

`GET {origin}/api/v1/health/live` — unauthenticated, returns
`{"status":"ok"}`. It uses the same client and the same policy as the rest of
the app, so a freshly imported CA or allowlist entry is reflected immediately.

- 5 second timeout, cancellable.
- Failures are classified: invalid input, DNS failure, connection refused,
  timeout, TLS handshake failure, certificate trust failure, unexpected HTTP
  status.
- Each class maps to a specific hint (for example, a trust failure links to the
  CA import dialog).

## 13. Session and origin binding

- `Session` gains an `origin` field, stored with the tokens.
- If the stored origin differs from the current one, the session is cleared and
  the user must log in again.
- Sessions written by older builds have no origin and are discarded once; the
  user logs in again.
- The current origin is displayed on the login and chat screens.

## 14. Storage

| Data | Location |
|---|---|
| Server address, session | `chatbox` preferences |
| Policy (switches, allowlist, custom CAs, known fingerprints) | `chatbox_net` preferences |
| Insecure-mode timestamp | `chatbox_net` preferences |

Policy is device-level and survives logout; the session does not. Only public
certificates are stored, never private keys.

`android:allowBackup="false"` prevents the policy and server address from being
restored onto another device or edited through a backup.

## 15. UI

- **Server screen** (first launch, or "Change server"): address field, scheme
  toggle, normalized URL preview, *Connect*, and a *Network settings* entry.
- **Network settings** (also reachable before login):
  - *Trust system CAs* (default on);
  - *Allow cleartext* list with add/remove (DNS pattern, IP, CIDR);
  - *Custom CAs* list with import/remove and fingerprint details;
  - *Disable security verification* with the confirmation described in §16.
- **Fingerprint dialog** as described in §9.
- **Chat screen**: a persistent red banner while verification is disabled,
  showing when it was disabled; the origin is always visible.
- Enabling "disable verification" asks for confirmation; re-enabling it does
  not.

## 16. Security guardrails

- The disable-verification confirmation lists what is lost: certificate and
  hostname checks, cleartext restrictions; that tokens, passwords, and messages
  can be read or modified; and that this is intended for debugging. The
  confirming button is destructive and explicit.
- The red banner while disabled cannot be dismissed.
- Custom CA and fingerprint dialogs always show the SHA-256 fingerprint, so the
  user can compare it out of band.
- First-connection confirmation cannot be skipped accidentally.
- No "remember password" option is added.

## 17. Platform and build changes

- `AndroidManifest.xml`: remove `android:networkSecurityConfig`, add
  `android:usesCleartextTraffic="true"` and `android:allowBackup="false"`.
- Delete `app/src/main/res/xml/network_security_config.xml`.
- Remove `BuildConfig.API_BASE_URL`; `ApiClient.create(baseUrl, client)` and
  `ChatSocket(baseUrl, session, client)` take a runtime base URL and the shared
  policy client, so only the source of that value changes. `10.0.2.2:8080`
  becomes a prefill hint, never a trusted default.
- New strings are added to the language-pack dictionaries; nothing is
  hard-coded in the UI.

## 18. Testing

Unit tests:

- DNS pattern matching: single-level `*`, multi-level `**`, bare domain,
  lookalike suffix, case, trailing dot, wildcard-only rejection.
- CIDR parsing and masking for IPv4 and IPv6, including invalid prefixes.
- Address normalization: pasted scheme, missing port, IDN, IPv6 brackets,
  rejected paths and ports.
- Policy resolution order, including verification-disabled short-circuit.
- Trust manager composition: system-only, custom-only, both, self-signed,
  expired, wrong hostname, empty chain.

Instrumented tests with `MockWebServer` and `HandshakeCertificates`:

- cleartext allowed and denied;
- `https → http` redirect blocked unless allowlisted;
- fingerprint change triggers a new confirmation;
- probe uses the active policy.

Manual matrix: `http`/`https` × allowlisted/not × system CA/self-signed/expired
× verification on/off × port present/absent × IPv6.

Artifact checks: the APK contains no network security config resource and the
merged manifest sets `usesCleartextTraffic="true"`.

CI guard: `node scripts/check-android-network-policy.mjs --require-apk` fails if
networking code constructs `OkHttpClient` directly instead of using the shared
factory, if the manifest or resource regresses, or if the packaged APK does not
match. The source-level check is part of the verification workflow
(`.github/workflows/verify.yml`, the push gate described in
[`test.md`](test.md#jobs)); the packaged-APK check runs in the release pipeline
(`.github/workflows/release.yml`, described in
[`release.md`](release.md#android)).
Pass `--apk=<path>` to inspect a specific artifact instead of the debug build;
the release pipeline uses it to check the signed APK. Run the JVM tests with
`./gradlew :app:testDebugUnitTest` and the instrumented tests with
`./gradlew :app:connectedDebugAndroidTest`.

## 19. Migration

All steps below are implemented.

1. Remove the manifest attribute and the resource file.
2. Introduce the policy store and the shared client factory; route
   `ApiClient.create` and `ChatSocket` through it.
3. Add the server screen and gate `ChatBoxRoot` on a configured origin.
4. Add the `origin` field to `Session` and drop origin-less sessions.
5. Update `android/README.md` (the build section currently documents
   `BuildConfig.API_BASE_URL`).

## 20. Future work

- Per-origin scoping of custom CAs (today the CA list is global).
- Optional certificate pinning per origin.
- A server capability/version endpoint so the client can warn about
  incompatible backends.
- Import/export of the policy, and QR-code server setup.
