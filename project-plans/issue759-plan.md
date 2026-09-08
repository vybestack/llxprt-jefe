# Issue #759: Issue send clones GitHub repos over HTTPS instead of SSH

## Diagnosis

- `CloneIdentity::clone_url()` (`src/app_input/clone_identity.rs`) unconditionally
  synthesizes `https://github.com/{owner/repo}.git` (introduced in `cb5247d7` /
  PR #193). All send paths derive from it: `issues_send.rs`,
  `transient_issue_send.rs`, `transient_pr_send.rs`, force-reclone
  (`issue_prep.rs`), and remote prep (`issue_prep_remote.rs`).
- Surfaced when a brand-new fleet location (`/Volumes/XS1000/.../llxprt/branch-9`)
  exercised clone-if-missing for the first time; existing workdirs never clone.
- Environmental DNS failure was transient; the durable defects are the HTTPS
  transport (hard-fails with `GIT_TERMINAL_PROMPT=0` without a credential
  helper; SSH is the required default) and the doubled `git git clone` prefix
  in the failure diagnostic (`require_success()` already prefixes `git `).

## Acceptance matrix

| # | Actor / launch path | Input & boundary cases | Observable success | Observable failure | Test |
|---|---|---|---|---|---|
| 1 | CloneIdentity synthesis (all send paths) | valid `owner/repo`; rejection rules unchanged (URLs, whitespace, options still rejected) | `clone_url()` returns scp-form `git@github.com:owner/repo.git` | invalid inputs still yield `None` | `clone_identity.rs` unit tests |
| 2 | Local clone-if-missing (Issues screen send) | workdir absent → clone; workdir present w/ matching origin → no clone | `git clone` invoked with the SSH scp-form URL | clone failure surfaces stderr detail | `issue_git_prep.rs` / `issue_prep_tests.rs` |
| 3 | Force-reclone (origin-mismatch confirm) | identity required; URL resolved before removal | SSH URL resolved pre-removal | removal cannot precede URL resolution (type-enforced) | `issue_prep_tests.rs` |
| 4 | Remote prep (SSH target) | remote enabled; planned (not executed) commands | planned ssh argv contains `git clone -- git@github.com:owner/repo.git …` | plan mismatch fails test | `issue_prep_remote_tests.rs` |
| 5 | Origin-mismatch predicate (#190) | actual origin HTTPS, SSH scp-form, or case-varied host; non-github host | both transports match same `github.com` `owner/repo` | non-github host / different repo still mismatch | existing `origins_match` tests (unchanged behavior) |
| 6 | Clone failure diagnostic | clone exits nonzero with stderr | message reads `git clone <url> failed: <detail>` — single `git` prefix | — | unit test on the `clone_repository` error path |

## Non-goals

- No transport configuration knob. SSH is the default; git's
  `url.<base>.insteadOf` rewrite covers exotic needs. A knob requires a new
  explicit issue.
- No change to `Repository.github_repo` validation (bare `owner/repo` only).
- No change to origin-mismatch policy, dirty-copy confirm, or prep sequencing.
- No network/DNS handling, no retry/backoff subsystem.
- No UI/TUI change (no TUI scenario required — behavior is not UI-visible).
- Remote hosts cloning via SSH must have their own GitHub SSH keys; key
  provisioning is out of scope.

## Vertical slices

Single slice — the behavior is one mechanism (URL synthesis) plus one
diagnostic prefix, owned by `app_input`:

1. RED: update/add tests asserting scp-form `clone_url()`, SSH URL in local
   clone and remote prep plans, single-`git` diagnostic. Prove failure against
   current code.
2. GREEN: change `CloneIdentity::clone_url()` to scp form; fix the doubled
   `git` context in `clone_repository()`; update stale doc comments
   (`clone_identity.rs` module header, `issue_prep.rs` "HTTPS-only" mentions,
   `issue_git_prep.rs` `EXPECTED_GITHUB_HOST` comment) and affected test
   expectations.
3. REFACTOR: none anticipated beyond comment accuracy.

## Expected files

- `src/app_input/clone_identity.rs`
- `src/app_input/issue_git_prep.rs`
- `src/app_input/issue_prep.rs` (comments only, if at all)
- `src/app_input/issue_prep_remote.rs` (comments only, if at all)
- Tests: `src/app_input/issue_send_modal_tests.rs`,
  `src/app_input/issue266_tracker_tests.rs`,
  `src/app_input/issue_prep_tests.rs`,
  `src/app_input/issue_prep_remote_tests.rs`,
  `src/app_input/issue_prep_predicate_tests.rs` (as needed)

## Scope ledger

| Entry | Status |
|---|---|
| (none) | — |

## Review counters

- OCR pre-PR: 0 / 2 used
- OCR post-PR: 0 / 2 used

## Verification evidence

Recorded 2026-09-08 on branch `issue759` (raw logs under `tmp/issue759-red/`):

- RED (`cargo test --locked --bin jefe -- app_input::clone_identity
  app_input::issue_prep::remote::tests app_input::issue_prep::tests
  app_input::issue_git_prep::tests app_input::issue_send_modal_tests
  app_input::issue266_tracker_tests`, production behavior temporarily
  reverted to HEAD): `FAILED. 115 passed; 10 failed` — exactly the ten tests
  asserting the new behavior: `clone_identity::tests::{
  parses_valid_owner_repo, always_uses_ssh_scp_clone_url,
  from_repository_uses_github_repo_not_slug}`,
  `issue_prep::remote::tests::{plan_clone_when_missing,
  plan_uses_scp_url_regardless_of_remote_enabled,
  plan_force_reclone_resolves_url_before_rm}`,
  `issue_git_prep::tests::clone_failure_diagnostic_has_single_git_prefix`,
  `issue_send_modal_tests::{issue_send_info_carries_valid_clone_identity_only,
  code_puppy_issue_uses_identical_prep_and_fresh_no_resume_signature}`,
  `issue266_tracker_tests::issue_send_clone_identity_remains_fork_not_upstream`.
  All `parse()` rejection and `origins_match` tests passed in RED (unchanged
  behavior confirmed).
- GREEN (same command, fix applied): `ok. 125 passed; 0 failed`.
- Predicate/origin-matching suite (`... app_input::issue_prep::predicate_tests
  origins_match`): `ok. 48 passed; 0 failed`.
- `cargo fmt --all`: clean.
- Quick-check fallback (no Makefile in this repo): `cargo fmt --all --check`
  clean, `cargo check -q` exit 0, `cargo test -q` exit 0 — 81 test binaries,
  7650 passed, 0 failed (4m31s wall).
- Full workspace clippy/build gates deferred to the coordinator, per slice
  contract.

### Coordinator gates (exact candidate head)

- `cargo fmt --all --check`: pass (log: `tmp/issue759-gates/fmt2.log`).
- One clippy gate fix: the new diagnostic test initially used a `match` on
  `Result` that `clippy::manual-let-else` rejects; rewritten as `let...else`
  (behavior identical).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  pass (log: `tmp/issue759-gates/clippy2.log`).
- `cargo build --workspace --all-features --locked`: pass (log:
  `tmp/issue759-gates/build.log`).
- `cargo test --workspace --all-features --locked`: pass — 96 test binaries
  `ok`, 7764 passed, 0 failed, exit 0 (log: `tmp/issue759-gates/test.log`).
- Mainline drift check before PR: `origin/main` 0 commits ahead of branch
  point (`aa897813`) — no rebase needed.

## Deferred findings / follow-ups

- (none)
