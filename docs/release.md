# Release Pipeline

This document describes how ChatBox turns a signed tag into published release
artifacts. It is the specification for the pipeline: the workflow file that
implements it is deliberately kept outside the repository, so this file is the
reference to review and to keep any external workflow in sync with
(see `AGENTS.md` §6).

## Triggers

| Trigger | Purpose | Tag used |
|---|---|---|
| `push` of a `v*` tag | Normal release | the pushed tag |
| `workflow_dispatch` (`tag` input) | Re-run a release after a transient failure | the supplied tag |

Both triggers require an **annotated, GPG-signed** tag (`AGENTS.md` §5). The
release job refuses to publish otherwise, so an unsigned or lightweight tag
fails loudly instead of producing an unverifiable release.

## Jobs

```text
verify ──┬─> backend ──┐
         └─> android ──┴─> release
```

`verify` gates everything else: a release is never built from a tree that does
not pass formatting, lint, tests, or the project's own guard scripts.

### `verify`

Cheap, fail-fast source checks:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked` (the domain services run against the
  in-memory stores, so no MySQL/Redis service is required)
- `npm ci && npm run check` in `web/` (Svelte check + language packs)
- `node scripts/check-android-network-policy.mjs` (source-level half of the
  network policy guard)
- `node scripts/check-language-packs.mjs`

### `backend`

Builds the server for both supported Linux targets and uploads them as
artifacts:

| Asset | Target | Builder |
|---|---|---|
| `chatbox-server-x86_64-unknown-linux` | `x86_64-unknown-linux-gnu` | `cargo build --release` |
| `chatbox-server-arm64-unknown-linux` | `aarch64-unknown-linux-gnu` | `cargo zigbuild --release` |

`cargo-zigbuild` cross-compiles to `aarch64` without a system toolchain, so the
ARM64 binary is produced on a plain `ubuntu-latest` runner. Both binaries are
stripped afterwards with the `llvm-strip` shipped by the Rust `llvm-tools`
component, which is target-agnostic and therefore also handles the zigbuild
artifact.

### `android`

Builds, signs, and verifies the release APK:

1. `./gradlew test` — JVM unit tests. Instrumented tests are **not** run here;
   they need an emulator and belong to a separate device-test workflow.
2. `./gradlew assembleRelease` — produces `app-release-unsigned.apk`; the
   Gradle build itself holds no signing configuration.
3. `zipalign -p -f 4` — required before signing so the APK can be mapped
   without relocation.
4. `apksigner sign` — reads the keystore from `RUNNER_TEMP`; every secret is
   passed through the environment, never interpolated into the shell script.
5. `apksigner verify --print-certs` — fails the job if the signature is invalid.
6. `node scripts/check-android-network-policy.mjs --require-apk --apk=<apk>` —
   inspects the packaged manifest and resources of the *signed* APK. The script
   defaults to the debug APK; the release pipeline points it at the release
   artifact explicitly.

The temporary keystore is deleted in an `if: always()` step. Android SDK
packages are pinned to the build's `compileSdk`/`buildToolsVersion`
(`android-35`, `build-tools;35.0.0`); bumping `compileSdk` in
`android/app/build.gradle.kts` requires bumping them here too.

### `release`

Downloads both artifact bundles, writes `SHA256SUMS.txt`, and publishes a GitHub
release with `gh release create --generate-notes --verify-tag`. Release notes
are generated from commit history by GitHub rather than assembled by hand.

## Required secrets

| Secret | Purpose |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | base64-encoded release keystore (`.jks`) |
| `ANDROID_KEY_ALIAS` | key alias inside the keystore |
| `ANDROID_KEYSTORE_PASSWORD` | keystore password |
| `ANDROID_KEY_PASSWORD` | key password |

`GITHUB_TOKEN` is provided by Actions and needs `contents: write`.

Missing secrets fail at the keystore step with an explicit `::error::` message
instead of a confusing `apksigner` exit code.

## Artifacts

| Asset | Contents |
|---|---|
| `chatbox-server-x86_64-unknown-linux` | x86_64 server binary |
| `chatbox-server-arm64-unknown-linux` | ARM64 server binary |
| `chatbox-android.apk` | signed release APK |
| `SHA256SUMS.txt` | checksums for the three assets above |

## Known gaps

- The `web/` client is **not** part of the release yet. The Svelte app is a
  standalone Node-based build (`@sveltejs/adapter-node`) and the server does not
  serve its output, so shipping it requires either a static-serve decision or a
  separate web artifact. Track this in the roadmap before claiming web support
  in a release.
- Docker images (`deploy/docker`) are built outside this workflow.

## Local dry run

Reproduce the gates before tagging:

```bash
cd backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

cd ../web && npm ci && npm run check
cd .. && node scripts/check-android-network-policy.mjs

cd android && ./gradlew test assembleRelease
```
