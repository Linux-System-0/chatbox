# Continuous Integration

This document specifies the CI gate implemented by `.github/workflows/verify.yml`.
The same workflow is reused by the release pipeline (`workflow_call`) and can be
started manually (`workflow_dispatch`), so all three paths run exactly the same
checks instead of keeping copies that drift apart.

## Triggers

| Trigger | Scope | Purpose | Can it block a merge? |
|---|---|---|---|
| `push` | every branch and tag (no `branches` or `tags` filter) | Verifies the pushed commit; covers feature branches, `main`, `develop`, and release tags | **Indirectly** — branch protection can require the `Verify sources` check on a pull request's head commit |
| `workflow_call` | `.github/workflows/release.yml` | Gates a release on the same checks before any artifact is built | Not applicable |
| `workflow_dispatch` | manual, optional `ref` input | Runs the checks on demand against a branch, tag, or commit | **No** — see [Known gaps](#known-gaps) |

`workflow_dispatch` takes one optional input, `ref`, which defaults to the ref
selected in the Actions UI. It is a convenience for verifying a release tag or
an older commit without pushing a branch.

Manual runs are informational. They are not attached to a pull request, never
publish anything, and cannot satisfy a required status check; use them to
reproduce a failure or to inspect a tag before release.

### Push-only gate

`verify.yml` has no `pull_request` trigger. A pull request is gated indirectly:
pushing the head branch produces a `Verify sources` check run on that commit,
and branch protection can make that check required. The pull request cannot
merge while the check on its head commit is red or missing.

The trade-off is that the gate never sees GitHub's synthetic merge commit:

- Two pull requests can each be green on their own and still conflict
  semantically (a renamed function, a changed default, a moved migration) once
  both land. The `push` run on the target branch catches that only after the
  merge, when it can report the breakage but no longer prevent it.
- Fork pull requests do not run this repository's `push` workflow, so a required
  check waits for a run that never appears. See [Known gaps](#known-gaps).

Release tags also match the unfiltered `push` trigger, so the gate runs once as
a standalone `push` run. The release pipeline separately calls the same workflow
for the tag it is about to publish, so publishing starts only after the tag
passes the checks again.

## Jobs

### `Verify sources`

The only job. It checks out the requested ref and fails fast on cheap source
checks before anything expensive (cross-compilation, Android SDK provisioning)
is attempted:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked` — the domain services run against in-memory
  stores, so no MySQL/Redis service containers are needed.
- `npm ci && npm run check` in `web/` — `check` runs `svelte-kit sync`,
  `svelte-check`, and then `scripts/check-language-packs.mjs`, so the
  language-pack guard is covered by this step rather than repeated as a separate
  one.
- `node scripts/check-android-network-policy.mjs` — source-level half of the
  network policy guard. The packaged-APK half needs a built APK and therefore
  runs in the release pipeline, not here.

A direct `push` or `workflow_dispatch` run reports the job name, `Verify
sources`, as its status check. When `release.yml` calls the workflow, GitHub
prefixes the called job with the caller job, so the check there is named
`verify / Verify sources`. Branch protection only needs to require `Verify
sources`; the prefixed form exists only inside release runs.

The job pins `ubuntu-24.04` rather than `ubuntu-latest` so that a GitHub image
rollover cannot silently change the toolchain the gate approves — and, because
`release.yml` calls this same workflow, the image a release must pass in.

## Reuse and manual runs

The workflow is called with:

```yaml
jobs:
  verify:
    permissions:
      contents: read
      actions: write
    uses: ./.github/workflows/verify.yml
    with:
      ref: ${{ github.ref }}
```

Calling jobs grant `contents: read` and `actions: write` explicitly: a called
workflow does not inherit the caller's workflow-level permissions, and the Rust
cache needs to write to the Actions cache. The called workflow declares the same
permissions, which a caller may downgrade but not elevate.

The workflow checks out its target itself: `inputs.ref` when a caller or a
manual run provides one, `github.ref` otherwise. Callers therefore need no
checkout step before the call.

Keep every external `uses:` in the workflow pinned to a full commit SHA
(`AGENTS.md` §8). The local `./.github/workflows/verify.yml` references need no
SHA because the workflow is versioned with this repository.

## Reproducing locally

Run exactly what the workflow runs before pushing:

```bash
cd backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

cd ../web && npm ci && npm run check
cd .. && node scripts/check-android-network-policy.mjs
```

## Required repository settings

The workflow alone does not gate anything; GitHub has to be told to require it.
Under **Settings → Branches → Branch protection rules** for `main` (and
`develop`):

- [ ] **Require a pull request before merging** — blocks direct pushes
      (`AGENTS.md` §2).
- [ ] **Require status checks to pass before merging**, with `Verify sources`
      selected as required. For same-repository branches the check comes from
      the `push` run on the pull request's head commit; because there is no
      `pull_request` trigger, fork pull requests never report it. Optionally
      enable **Require branches to be up to date before merging**, but note that
      it does not create a check for the merge commit.
- [ ] **Require signed commits** — mirrors the GPG rule in `AGENTS.md` §5.
- [ ] **Do not allow bypassing the above settings** for the rule to apply to
      maintainers too.

Without the required-check entry, `Verify sources` merely reports on the branch
push.

## Concurrency

```yaml
group: ci-${{ github.workflow }}-${{ github.ref }}
cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```

`cancel-in-progress` is only true for `pull_request`, which this workflow does
not trigger. Every run therefore keeps going: pushes to the same ref queue
behind the active run instead of cancelling it. The expression keeps the
intended policy explicit for the day a `pull_request` trigger is added.

The group is scoped to this workflow and the triggering ref. A release run uses
the release workflow's separate `release-*` group; the called `verify` job runs
inside that run and is covered by it. Manual `workflow_dispatch` runs use the
push group and may queue behind a push run for the same ref; they publish and
gate nothing, so waiting is harmless.

## Known gaps

- **No `pull_request` trigger.** The gate runs on pushes only. The synthetic
  merge commit is never checked, and fork pull requests get no run at all.
  Adding `pull_request: branches: [main, develop]` would close both gaps.
- **Manual runs cannot gate anything.** `workflow_dispatch` runs are useful for
  inspection, but GitHub does not associate them with a pull request or a
  required check.
- **Android instrumented tests** are not run. They need an emulator and belong
  to a separate device-test workflow. The release pipeline runs `./gradlew test`
  (JVM) before assembling the APK; this gate does not touch Gradle at all, so a
  Kotlin compile break is first reported by a release run rather than by CI.
- **`./gradlew assembleRelease`** is not run by the gate, so a Gradle
  release-configuration break surfaces during a release, not during review.
- **`scripts/e2e-smoke.mjs`** is not part of the gate yet.
- No caching of the Android SDK; the release job provisions it from scratch.
