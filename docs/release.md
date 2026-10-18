# Release Pipeline

This document describes how ChatBox turns a signed tag into published release
artifacts. It is the specification for the pipeline; the implementation lives in
`.github/workflows/release.yml`. The verification gate it starts with is the
same workflow that runs as the push gate,
`.github/workflows/verify.yml`; it is also callable and manually dispatchable,
and the checks it runs are described in [`test.md`](test.md). When a workflow
and this document disagree, fix them in the same change (`AGENTS.md` §6).

## Triggers

| Trigger | Purpose | Tag used |
|---|---|---|
| `push` of a `v*` tag | Release | the pushed tag |

The trigger requires an **annotated, GPG-signed** tag (`AGENTS.md` §5). The
release job refuses to publish otherwise, so an unsigned or lightweight tag
fails loudly instead of producing an unverifiable release.

The workflow has no `workflow_dispatch` trigger or `tag` input. A transient
failure is retried with **Re-run failed jobs** in the Actions UI; pushing a new
signed tag is reserved for a new release. The verification workflow itself
(`verify.yml`) can be run manually to check a tag before publishing; see
[`test.md`](test.md#triggers).

Ordinary branch pushes and pull requests do not run `release.yml`; they are
covered by the CI gate in [`test.md`](test.md).

A per-tag `concurrency` group with `cancel-in-progress: false` queues overlapping
runs for the same tag instead of cancelling a run that may already be
publishing.

## Jobs

```text
verify ──┬─> backend ──┐
         └─> android ──┴─> release
```

`verify` gates everything else: a release is never built from a tree that does
not pass formatting, lint, tests, or the project's own guard scripts.

### `verify`

The job calls the same verification workflow as the push gate:

```yaml
verify:
  permissions:
    contents: read
    actions: write
  uses: ./.github/workflows/verify.yml
  with:
    ref: ${{ github.ref }}
```

It is deliberately not a copy of the checks. The release must be built from a
tree that passed the exact same gate the push workflow runs; a second
hard-coded list here would drift from [`test.md`](test.md) and quietly weaken
the release. The called workflow checks out the tag itself, so the caller has no
checkout step; `backend`, `android`, and `release` check out `github.ref` (the
pushed tag) directly.

What the workflow runs — formatting, lint, backend tests, the web check (which
includes the language-pack guard), and the source-level Android network policy
check — is specified in [`test.md`](test.md#jobs) and the
[reuse and manual runs section](test.md#reuse-and-manual-runs).

### `backend`

Builds the server for both supported Linux targets and uploads them as
artifacts:

| Asset | Target | Builder |
|---|---|---|
| `chatbox-server-x86_64-unknown-linux` | `x86_64-unknown-linux-gnu` | `cargo build --release --locked` |
| `chatbox-server-arm64-unknown-linux` | `aarch64-unknown-linux-gnu` | `cargo zigbuild --release --locked` |

`cargo-zigbuild` cross-compiles to `aarch64` without a system toolchain, so the
ARM64 binary is produced on a plain `ubuntu-24.04` runner. The jobs pin that
image instead of `ubuntu-latest`: the native x86_64 binary links the runner's
glibc, so a silent image rollover (GitHub moves the `-latest` alias to a new
Ubuntu release) would raise the minimum glibc for everyone using the released
binary. Adopting a newer image is therefore a deliberate change, made together
with the release notes. Both binaries are
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
packages are pinned to the build's `compileSdk`/`buildToolsVersion`/NDK
(`android-35`, `build-tools;35.0.0`, `27.0.12077973`); the command-line tools
archive is fixed by URL. Bumping `compileSdk` in
`android/app/build.gradle.kts` requires bumping these values here too.

### `release`

Downloads both artifact bundles and writes `SHA256SUMS.txt`. Before publishing,
it fetches tags and verifies that the tag is an annotated tag object containing a
PGP signature; an unsigned or lightweight tag fails the job. It then publishes a
GitHub release with `gh release create --generate-notes --verify-tag --latest`.
Release notes are generated from commit history by GitHub rather than assembled
by hand.

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
- There is no `workflow_dispatch` trigger or tag input on the release workflow;
  retries use the Actions **Re-run failed jobs** button. Re-releasing a tag with
  different content requires a new tag. The reusable verification workflow can
  be dispatched on its own, but it does not publish.
- Docker images (`deploy/docker`) are built outside this workflow.

## Local dry run

Reproduce the gates before tagging. The first block is exactly the CI gate
([`test.md`](test.md#reproducing-locally)); the second adds the release-only Android
steps:

```bash
cd backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

cd ../web && npm ci && npm run check
cd .. && node scripts/check-android-network-policy.mjs

cd android && ./gradlew test assembleRelease
```
