# AGENTS.md — Development Guide

This document is the authoritative engineering guide for **ChatBox**. It applies
to every human contributor and to every automated agent (AI assistants, bots,
CI jobs) working in this repository. If a tool or agent conflicts with anything
written here, this document wins.

---

## 1. Language Policy — English Only

**English is the only language used in this repository's engineering
artifacts.** This keeps the project reviewable and maintainable by an
international community.

This applies to:

- Source code and identifiers (variables, functions, types, modules).
- Code comments and documentation comments (docstrings, Rustdoc, KDoc, JSDoc).
- Commit messages, branch names, and tag names.
- Files in `docs/` (design docs, ADRs, runbooks).
- Issue titles/bodies, pull-request titles/descriptions, and review comments.
- Configuration keys, log messages, and error messages.

Notes:

- Localized **end-user** content is the one exception: user-visible UI strings
  are provided through translation resources (language packs), not hard-coded in
  the source. Runtime translation files may contain non-English text.
- Do not commit machine-translated prose as a substitute for clear English.
  Write short, direct sentences.

---

## 2. Branching Model

ChatBox uses a lightweight Git-Flow-style model with one long-lived
integration branch (`develop`) and one stable branch (`main`). **Development
always happens on `feature/*` (and the other short-lived typed) branches;
`develop` only receives merges from them.**

### Branch types

| Pattern | Lifetime | Branches from | Merges into | Direct commits? |
|---|---|---|---|---|
| `main` | permanent | — | — | **No** — merge only, via review |
| `develop` | permanent | `main` | `main` (as a release) | **No** — merge only (integration branch) |
| `feature/*` | short-lived | `develop` | `develop` | Yes — this is where development happens |
| `fix/*` | short-lived | `develop` | `develop` | Yes |
| `hotfix/*` | short-lived | `main` | `main` **and** `develop` | Yes |
| `release/*` | short-lived | `develop` | `main` **and** back into `develop` | Yes |
| `docs/*` | short-lived | `develop` | `develop` | Yes |
| `refactor/*` | short-lived | `develop` | `develop` | Yes |
| `perf/*` | short-lived | `develop` | `develop` | Yes |
| `test/*` | short-lived | `develop` | `develop` | Yes |
| `chore/*` | short-lived | `develop` | `develop` | Yes |

> `develop` is the **integration branch**: it collects completed work from
> `feature/*` branches. Do not develop directly on `develop`; create a
> `feature/*` branch instead.

### Where development happens

- **All development happens on `feature/*` branches** (or the other typed
  short-lived branches above). You implement, test, and commit there.
- **`feature/*` and `fix/*`** are cut from `develop` and merged back into
  `develop` through a pull request. `develop` is the merge target, not a place to
  write code.
- **`release/*`** is cut from `develop` when preparing a release, used for final
  stabilization, then merged into `main` and merged back into `develop`.
- **`hotfix/*`** branches from `main` for urgent production fixes, is merged
  into `main` immediately, and **must also be merged back into `develop`** so the
  fix is not lost.
- **Never commit directly to `main` or `develop`.** They only receive reviewed
  merges.
- Do not create long-lived personal branches. Rebase/merge `develop` often to
  keep `feature/*` branches current.

### Branch naming

Create a typed branch from the correct base (see the table above):

```bash
# Normal feature work (branches from develop, merges back into develop)
git checkout develop
git pull --ff-only
git checkout -b feature/session-store

# Urgent production fix
git checkout main
git pull --ff-only
git checkout -b hotfix/token-expiry
```

Use these typed prefixes:

- `feature/` — new feature (the normal development branch)
- `fix/` — non-urgent bug fix
- `hotfix/` — urgent fix for a released version (branches from `main`)
- `release/` — release stabilization (branches from `develop`)
- `docs/` — documentation only
- `refactor/` — behavior-preserving change
- `perf/` — performance work
- `test/` — tests only
- `chore/` — tooling, deps, maintenance

Branch names are lowercase, hyphen-separated, and in English.

---

## 3. Commit Messages

ChatBox follows the **Conventional Commits** specification.

### Format

```text
<type>(<optional scope>): <short summary>

<body — what changed and why>

<footer — issue references, breaking changes>
```

- **Summary line:** imperative mood (`add`, not `added`/`adds`), lowercase
  after the colon, no trailing period, **≤ 72 characters**.
- **Body:** wrap at 72 columns. Explain **why** the change is needed and any
  non-obvious decisions. Do not narrate the diff line by line.
- **Footer:** reference issues (`Refs #42`, `Closes #42`) and call out breaking
  changes (`BREAKING CHANGE: ...`).
- Keep the message in **English**.

### Allowed types

`feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`,
`style`, `revert`.

### Examples

```text
feat(auth): verify passwords with argon2id

Replace the provisional verification stub with argon2id and add a
constant-time comparison to avoid leaking hash timing.

Closes #17
```

```text
docs(auth): plan session token lifecycle

Describe issue/refresh/revoke semantics before implementing the session
store so the interfaces can be reviewed first.

Refs #28
```

### Mandatory GPG signing

**Every commit and every tag MUST be GPG-signed.** Unsigned commits are
rejected in review and must not be merged.

One-time setup (per machine):

```bash
git config --global user.signingkey <YOUR_KEY_ID>
git config --global commit.gpgsign true
git config --global tag.gpgsign true
git config --global gpg.program gpg
```

Verify that signing works before you rely on it:

```bash
git config --get commit.gpgsign   # -> true
git log --show-signature -1       # -> "Good signature"
```

Rules:

- Do **not** use `git commit --no-gpg-sign` or `git tag -s` shortcuts that
  bypass signing.
- If a commit is made unsigned by mistake, amend it immediately
  (`git commit --amend --no-edit -S`) or rebase with signing enabled before
  pushing.
- Signed **annotated** tags are required for releases:
  `git tag -s v0.1.0 -m "v0.1.0"`.

---

## 4. When to Commit (Commit Timing)

Commit **early and often, but only coherent units of work.**

Commit when all of the following are true:

1. **The change is one logical unit.** One feature, one fix, one doc update,
   one refactor — not several unrelated edits bundled together.
2. **It builds.** The affected component compiles/bundles (e.g. `cargo check`,
   `npm run build`).
3. **Tests pass** for the affected area, and new behavior has tests where
   practical.
4. **It is self-contained.** The tree is left in a working state; reverting
   this one commit would not break unrelated work.

Do **not**:

- Commit broken, non-compiling, or knowingly failing code to `develop`, `main`, or
  any shared integration branch.
- Mix formatting-only changes with behavior changes.
- Bundle a large refactor with a feature change.
- Leave work uncommitted for days; large uncommitted trees cause painful
  merges.

Recommended checkpoints:

- After the design docs are updated and reviewed (see §6 — docs first).
- After each test that turns green.
- Before switching tasks, taking a break, or rebasing.
- Before opening a pull request.

---

## 5. Merging into `main`

`main` is the stable branch and is protected. Only three branch types may be
merged into it — `develop`, `release/*`, and `hotfix/*` — always after
review, and only when the branch is green.

### Pre-merge checklist

- [ ] All commits being merged are **GPG-signed** and verified.
- [ ] CI (build, lint, tests) is green for the full branch.
- [ ] Documentation and the roadmap are updated (see §6).
- [ ] At least one maintainer has reviewed and approved the change.
- [ ] The changelog/release notes are updated for user-visible changes.

### Merge procedure

Keep local `main` current, verify signatures, then merge with a signed merge
commit so the release history records the integration point:

```bash
git checkout develop
git pull --ff-only

# Verify every incoming commit is signed.
git log --show-signature main..develop

git checkout main
git pull --ff-only

# --no-ff keeps an explicit, signed merge commit.
git merge --no-ff --gpg-sign -m "chore(release): merge develop into main" develop
git push origin main
```

Notes:

- Sign the merge commit (`--gpg-sign`). Never use `--no-gpg-sign`.
- Prefer `--no-ff` (or a rebase-and-fast-forward of already-signed commits) so
  the history stays linear and auditable. Avoid squash-merging unless the
  resulting squashed commit is signed.
- Tag releases immediately with a **signed** annotated tag and push it:

  ```bash
  git tag -s v0.1.0 -m "v0.1.0"
  git push origin v0.1.0
  ```

- If `main` is managed through a hosting platform, ensure the platform performs
  a **signed** merge, or land the merge locally as shown above.

### Hotfix and release merges

`release/*` and `hotfix/*` follow the same signing and review rules, but with a
different base branch:

```bash
# Release branch (cut from develop)
git checkout develop && git pull --ff-only
git checkout -b release/v0.1.0
# ... stabilize, then merge into main as above, and back into develop.

# Hotfix (cut from main, for an urgent production fix)
git checkout main && git pull --ff-only
git checkout -b hotfix/token-expiry
# ... fix, then:
git checkout main
git merge --no-ff --gpg-sign -m "fix: merge hotfix/token-expiry into main" hotfix/token-expiry
git push origin main
git tag -s v0.0.1 -m "v0.0.1"   # only if a patch release is warranted
git push origin v0.0.1

# Do not forget to back-port the hotfix into the integration branch:
git checkout develop
git merge --no-ff --gpg-sign -m "fix: back-port hotfix/token-expiry" hotfix/token-expiry
git push origin develop
```

Key points:

- A `hotfix/*` must reach **both** `main` and `develop`; otherwise the next
  release from `develop` reintroduces the bug.
- A `release/*` branch is merged into `main`, then merged back into `develop` to
  keep them aligned.
- Delete short-lived branches after a successful merge.

### After the merge

- Sync the integration branch so it includes the merge:

  ```bash
  git checkout develop
  git merge --ff-only main
  git push origin develop
  ```

---

## 6. Documentation-First Workflow

**Update the plan/documentation before writing the code.** Design and intent
are reviewed in text first, so implementation has an agreed target.

For every change that adds or alters behavior:

1. **Update the docs first.** Edit the relevant design doc in `docs/`, the ADR
   (Architecture Decision Record) if the change is architectural, and the
   roadmap checkboxes in `README.md` / `README_zh.md`. Commit this as a
   `docs(...)` commit on your `feature/*` branch.
2. **Then implement the code** in a follow-up `feat(...)`/`fix(...)` commit that
   references the doc change.
3. **Keep docs and code in sync.** If implementation reveals the plan was
   wrong, update the doc in the same change — do not leave stale docs behind.

Rules:

- A pull request that introduces behavior without a documentation change should
  be sent back for revision (except trivial internal cleanups).
- Record non-obvious architectural decisions as an ADR under `docs/`.
- The roadmap is the shared plan: check items off only when the corresponding
  code and docs have both landed.
- Documentation is written in **English** (see §1).

---

## 7. Pull Requests

1. Push your topic branch and open a pull request **into `develop`** (not
   directly into `main`). The only exceptions are `hotfix/*` and
   `release/*`, which target `main`.
2. Describe the change, its motivation, and how it was tested.
3. Confirm the **GPG-signing** checkbox and that CI is green.
4. Address review comments with new signed commits — do not rewrite public
   history that others may have based work on.
5. A maintainer merges to `develop`; `main` is updated later via §5 (or, for
   `hotfix/*` and `release/*`, merged into `main` and back-ported to `develop`).

---

## 8. Testing, Style, and Tooling

- Run the project's formatters and linters before committing (e.g. `cargo fmt`,
  `cargo clippy`, and the web/android equivalents once they exist).
- Keep all `await`s/timeouts bounded, avoid blocking the async runtime, and
  respect the engineering constraints documented in `README.md`.
- Add tests for new behavior and bug fixes where practical.
- Never commit secrets, credentials, or local `.env` files (see `.gitignore`).

---

## 9. Quick Reference

```bash
# Start work
git checkout develop && git pull --ff-only
git checkout -b feature/my-change

# Commit (signed automatically via commit.gpgsign=true)
git add -p
git commit -m "feat(scope): concise English summary"

# Verify signatures
git log --show-signature

# Merge to main (maintainers only, after review)
git checkout main && git pull --ff-only
git merge --no-ff --gpg-sign -m "chore(release): merge develop into main" develop
git push origin main

# Urgent production fix
git checkout main && git pull --ff-only
git checkout -b hotfix/critical-bug
# ... fix and commit ...
git checkout main
git merge --no-ff --gpg-sign -m "fix: merge hotfix/critical-bug into main" hotfix/critical-bug
git push origin main
git checkout develop && git merge --no-ff --gpg-sign -m "fix: back-port hotfix/critical-bug" hotfix/critical-bug
git push origin develop
```
