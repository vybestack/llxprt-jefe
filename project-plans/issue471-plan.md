# Issue 471 delivery plan

Release workflow hardening deferred from PR #444 (OCR run 4). All five
findings target pre-existing lines in `.github/workflows/release.yml` that the
Windows-support PR did not modify. No Rust production code and no release
artifact layout or checksum contract changes are in scope.

## Scope decision

This is a single, narrowly scoped CI/supply-chain hardening slice. It touches
one workflow file plus a contract test that follows the existing
`read_repo_text` pattern already used by `tests/core/windows_support_contracts.rs`
and `tests/core/ocr_workflow_contracts.rs`. It crosses one ownership layer
(release CI) and one orchestration route (the tag-push release pipeline), so it
does not trigger the three-owner/three-route split rule.

## Decisions

1. **Pin style.** Match the precedent already established in
   `.github/workflows/ci.yml` (`windows_native` job, lines 162-167):
   `uses: <owner>/<action>@<40-char-sha> # <tag>`. Reuse the exact SHAs already
   pinned in `ci.yml` for the three shared actions so the two workflows stay in
   lockstep:
   - `actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4`
   - `dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30 # stable`
   - `Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2`
   For the remaining third-party actions not yet pinned anywhere in the repo,
   pin to the commit SHA their current `v*` tag resolves to:
   - `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4`
     (matches the SHA already used in `ocr-review.yml` / `pr-review.yml`)
   - `actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4`
     (matches the SHA already used in `ocr-review.yml`)
   - `softprops/action-gh-release@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65 # v2`
     (current `v2` tag commit)
   Pinned SHAs are verified against the GitHub API in the delivery notes. A SHA
   is "full" (40 hex chars); the contract test enforces the full-length shape.

2. **`timeout-minutes` values.** Choose conservative ceilings that bound hangs
   without prematurely cancelling known-slow legitimate runs, consistent with
   the existing 60-minute `windows_native`/build ceilings and the 15-minute
   smoke ceiling in `ci.yml`:
   - `build-release-binaries`: 60 (cross-compilation + packaging; matches the
     existing build ceiling in `ci.yml`).
   - `publish-release-assets`: 15 (download + gh-release upload; bounded I/O).
   - `update-homebrew-tap`: 15 (clone + jq parse + git push; bounded I/O).

3. **Permissions scoping.** Remove the workflow-level
   `permissions: contents: write`. Add a workflow-level least-privilege default
   `permissions: contents: read`. Declare `permissions: contents: write` only on
   the `publish-release-assets` job, which is the only job that calls
   `softprops/action-gh-release`. `build-release-binaries` needs only `read`
   (checkout + build). `update-homebrew-tap` needs only `read` for the GitHub
   API release-metadata fetch and pushes to a *separate* repository via
   `HOMEBREW_TAP_TOKEN`.

4. **Explicit `jq` install.** Add a dedicated step at the top of
   `update-homebrew-tap` that installs `jq` via `apt-get` (the job runs on
   `ubuntu-latest`). This removes the implicit dependence on the runner image
   having `jq` preinstalled. (`jq` is also used by `pr-review.yml` and
   `ocr-review.yml` without an explicit install; hardening the release workflow
   is the requested scope, not touching the other workflows.)

5. **Dynamic default branch.** Replace the hardcoded `git push origin main` with
   a step that resolves the tap repository's default branch via the GitHub API
   and pushes to that ref. Use the existing `HOMEBREW_TAP_TOKEN` to call
   `GET /repos/vybestack/homebrew-tap` and read `default_branch`. This keeps the
   push correct if the tap's default branch is ever renamed. It does not change
   the source repository's default-branch assumption (the source repo default is
   not referenced by the release workflow).

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure and diagnostics | Side effects before failure | Persistence and compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | TDD contract test reads `.github/workflows/release.yml` | Third-party action references | CI (all platforms) | Every `uses:` for a third-party action references a 40-char commit SHA | A mutable tag reference (`@v2`, `@stable`) is present and fails the contract test | None | Workflow YAML only; no artifact/checksum contract change | `release_workflow_contracts.rs` assertions on full-SHA pinning |
| A2 | Same | Each job in the release workflow | CI | `build-release-binaries`, `publish-release-assets`, and `update-homebrew-tap` each declare a `timeout-minutes` | A job lacks `timeout-minutes` and fails the contract test | None | None | Contract assertions enumerating the three job keys with a `timeout-minutes` line |
| A3 | Same | Workflow and job `permissions:` blocks | CI | The workflow declares a read-only default; only `publish-release-assets` elevates to `contents: write` | Workflow-level `contents: write` or a `contents: write` on a non-publish job fails the contract test | None | No change to what the publishing job can do | Contract assertions scoping `contents: write` to the publish job |
| A4 | Same | The tap job step sequence | CI | `update-homebrew-tap` contains an explicit `jq` install step before the formula generation step | Absence of an explicit `jq` install fails the contract test | None | None | Contract assertion that the tap job installs `jq` |
| A5 | Same | The tap push step | CI | The push resolves the tap default branch dynamically (no literal `git push origin main`) | A hardcoded `origin main` push fails the contract test | None | Tap push target remains the tap repo's real default | Contract assertion forbidding `git push origin main` and requiring a default-branch resolution step |
| A6 | Release pipeline triggered by a `v*` tag push | Real tag push on the candidate head | GitHub Actions release workflow | The workflow YAML parses and all three jobs are dispatchable; the existing Windows packaging/checksum steps (issue #264) are unchanged | YAML syntax error breaks dispatch and fails CI | None | Release artifact layout and checksum contract unchanged | Existing `windows_support_contracts.rs` AC-10/AC-11 still pass; `release.yml` unchanged in the Windows-specific regions |

## Non-goals

- No change to the Windows packaging steps, asset layout, or checksum contract
  added by PR #444 / issue #264.
- No change to the `pr-review.yml`, `ocr-review.yml`, or `ci.yml` pinning of
  `jq`/actions. Those are out of scope; the issue targets `release.yml` only.
- No upgrade of pinned action versions beyond the commit the current tag points
  to. Pinning only freezes the current mutable reference; it does not move it.
- No new release artifact, signing, provenance (SLSO), or attestation step.
- No change to the Homebrew formula content, the `HOMEBREW_TAP_TOKEN` mechanism,
  or the tap repository URL.
- No new Rust production code, dependency, or manifest/lockfile change.
- No new xtask subcommand or quality-gate change.

## Architecture boundaries

- `.github/workflows/release.yml` owns the release CI pipeline. This slice
  edits only that file's action pins, `permissions`, `timeout-minutes`, `jq`
  install, and default-branch push. It does not add jobs or change triggers.
- `tests/core/release_workflow_contracts.rs` owns the durable hardening
  contract. It reads the workflow as text (same pattern as the existing
  `windows_support_contracts.rs` and `ocr_workflow_contracts.rs`) and asserts
  observable properties. No YAML parser dependency is added; the assertions are
  scoped to the specific hardened regions to avoid brittleness.
- `tests/core/mod.rs` registers the new test module.
- No production Rust module, no Cargo change, no xtask change.

## Expected paths

Target: 3 changed files, well under the 25-file / 1,500-line budget.

- `.github/workflows/release.yml` (hardening edits)
- `tests/core/release_workflow_contracts.rs` (new contract test)
- `tests/core/mod.rs` (register the new module)
- `project-plans/issue471-plan.md` (this plan)

## Vertical slice

A single vertical slice: RED contract test → GREEN hardening edits → verify.

1. **RED:** add `tests/core/release_workflow_contracts.rs` with assertions for
   A1-A6 and register it in `tests/core/mod.rs`. Run the test and confirm it
   fails for the intended reason (current workflow violates each hardening
   property).
2. **GREEN:** edit `.github/workflows/release.yml` to satisfy A1-A5. Confirm the
   Windows-specific regions (issue #264) are untouched so A6 holds.
3. **REFACTOR:** none expected; the edit is small and direct.
4. Commit one coherent green behavior.

Focused verification:

```text
cargo test --test core release_workflow_contracts
cargo test --test core windows_support_contracts
cargo xtask quick
```

Exact-head pre-push verification:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

Native Windows CI must pass (the contract test is platform-independent text
assertion; the release workflow is exercised only on tag push). An interrupted,
partial, skipped, or stale-head verification is incomplete.

## Scope ledger

| Item | Disposition | Notes |
| --- | --- | --- |
| Pin third-party actions in release.yml | Accepted (A1) | Reuse ci.yml SHAs where shared |
| Add job timeouts | Accepted (A2) | 60/15/15 minutes |
| Scope `contents: write` to publish job | Accepted (A3) | Workflow-level `contents: read` default |
| Explicit `jq` install | Accepted (A4) | ubuntu-latest apt-get step |
| Dynamic tap default branch | Accepted (A5) | GitHub API default_branch resolution |
| Contract test for the hardening | Accepted | New test module, read-as-text pattern |
| Pin `jq`/actions in other workflows | Rejected (out of scope) | Issue targets release.yml only |
| Upgrade action versions | Rejected | Pin current, do not move |
| SLSA/provenance/signing | Rejected | Not requested; separate effort |
| New xtask workflow-yaml lint | Rejected | Adds tooling; not required by the issue |

Newly discovered work must be added here before implementation. Stop if it
requires another production owner, a Cargo/lockfile change, an xtask/quality-gate
change, a new dependency, or a budget breach.

## Review counters and finding policy

- Local Open Code Review: 0 of 2 used.
- Pull-request Open Code Review: 0 of 2 used.

Each review finding receives one disposition (Blocker-Fix, In-scope-Fix, Reject,
Defer). A reviewer suggestion does not authorize scope expansion.

## Verification evidence

GREEN on the candidate head (branch `issue471`):

- RED captured first: all six hardening contract tests failed for the intended
  reasons before any workflow edit; the A6 Windows regression guard passed
  throughout (proving the test correctly distinguishes in-scope from out-of-
  scope regions).
- `cargo fmt --all --check` — clean (one rustfmt adjustment applied to the new
  test file).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — no
  warnings.
- `cargo build --workspace --all-features --locked` — clean.
- `cargo test --test integration` — 360 passed, 0 failed. This includes all 7
  `release_workflow_contracts` tests (A1-A6) and all 10
  `windows_support_contracts` (issue #264) regression guards.
- `cargo test --workspace --all-features --locked` — 801 passed; the only 3
  failures are pre-existing and environmental, in
  `app_input::prs_diff_dispatch::tests` (`classify_local_size_probe_*`), which
  spawn the Unix-only `true`/`false` programs via `std::process::Command`. They
  were confirmed to fail identically on the stashed base tree (without this
  change) on this Windows host and are unrelated to the release workflow. They
  are exercised under the repository's native Windows CI separately.

Action SHA provenance (resolved via the GitHub API at delivery time):

- `actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5` — already pinned
  in `ci.yml` (`windows_native`).
- `dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30` — already
  pinned in `ci.yml` (`windows_native`).
- `Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32` — already
  pinned in `ci.yml` (`windows_native`).
- `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02` — already
  used in `ocr-review.yml` / `pr-review.yml`.
- `actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093` — already
  used in `ocr-review.yml`.
- `softprops/action-gh-release@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65` —
  current `v2` tag commit.
