# Issue 446 — Windows startup rejects legacy local `~` paths

## Issue and regression context

- Issue: <https://github.com/vybestack/llxprt-jefe/issues/446>
- Launch path: normal `jefe` startup on Windows after updating from `main`.
- User-visible failure: schema-1 state containing `~/projects/jefe` is rejected with `CFG-E001` / `path has no existing ancestor` before the TUI starts.
- Regression: commit `92b1456` made schema-1 migration resolve local repository and agent paths through physical identity. A leading `~` remains a relative path at that boundary, so ancestor discovery eventually exhausts the relative path instead of resolving it against the user's home directory.
- The reported `jefe.exe` access-denied symptom was reproducible alongside psmux 3.3.6 process leaks. Upgrading the host to psmux 3.3.7 immediately restored detached session creation and cleanup; the upstream release includes process-teardown, one-shot command reliability, and atomic-server fixes.
- No CodeRabbit research response was present when this plan was shaped.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundaries | Target | Observable success | Failure and diagnostic | Side effects before failure/success | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| A1 | User starts `jefe`; startup validates a selected schema-1 `state.json` | A local repository `base_dir` is exactly `~` or begins with the host-supported home shorthand (`~/`; Windows also accepts `~\`) | Local Windows; equivalent host-native behavior on Unix | Startup validation succeeds when the home directory is available; the legacy path is resolved against the current user's home before physical identity is calculated | If no usable home is available, retain the existing typed `CFG-E001` path failure; do not guess a different root | Read and validate only; normal startup does not rewrite schema-1 bytes or create the represented repository path | Schema-1 bytes remain authoritative and byte-identical until a later normal save | Startup-boundary unit test seeds schema-1 bytes with a local `~` repository and proves startup succeeds without rewriting them |
| A2 | Schema-1 migration restores local repository and agent state | Local repository `base_dir` and local agent `work_dir` use home shorthand; the agent suffix need not exist | Local Windows and Unix | Migrated schema-2 repository location and local agent `work_dir` typed value contain the canonical home-based OS path; launch hashes are derived from the same effective path | Physical-identity errors remain typed persistence diagnostics | No directory creation and no source-byte mutation | Prevents startup success followed by restoration of an unusable literal `~` agent path | Migration test asserts canonical repository location, canonical typed agent work directory, and absence of created suffix directories |
| A3 | Schema-1 migration handles a remote repository and agent | Remote `base_dir` / `work_dir` begin with `~/` | Remote target on any local host | Remote tilde syntax remains remote syntax and is not expanded against the local user's home | Existing remote validation/diagnostics remain unchanged | No local filesystem access for remote paths | Existing remote identity and typed-value semantics remain compatible | Migration regression test proves remote targets retain remote tilde paths |
| A4 | User starts with ordinary local schema-1 paths | Absolute paths or relative paths without a leading standalone `~` component | All supported local platforms | Existing nearest-ancestor physical identity behavior is unchanged | Existing `CFG-E001` diagnostics are unchanged | Existing read-only migration behavior | No schema-version or dependency changes | Existing migration and path-identity suites remain green |
| A5 | Windows user starts Jefe or the native harness through psmux | Installed psmux is 3.3.6 or older versus 3.3.7+ | Native Windows | Preflight accepts 3.3.7+, detached `new-session` returns, and namespace cleanup terminates the owned server without leaking processes that lock `jefe.exe` | 3.3.6 is rejected with existing unsupported-version guidance naming the 3.3.7 minimum | Qualification may probe `psmux -V`; no session is started for an unsupported binary | Native Windows CI pins and checksums 3.3.7; local docs and smoke policy agree | Version-policy RED/GREEN tests plus the real psmux session test on upgraded 3.3.7 |

## Explicit non-goals

- Adding a new process-management or cleanup subsystem; the psmux slice uses the upstream lifecycle fix and rejects the known-broken release.
- Expanding `~user`, environment variables, or arbitrary shell syntax.
- Changing remote path semantics; remote `~/...` paths are interpreted by the remote environment.
- Rewriting state during normal startup, changing the schema version, or adding a migration command side effect.
- Retrofitting tilde expansion into all persistence target/config paths or moving the existing state/services path architecture.
- Normalizing unrelated repository defaults such as `transient_agent_dir`.
- Adding dependencies, public APIs, production modules, UI behavior, or TUI scenarios.

## Vertical slices

### Slice S1 — Restore legacy local home-relative state

- Acceptance rows: A1–A4.
- Architecture owner: `persistence::migration`; integration boundary is startup's existing `migrate_state` validation call.
- Allowed files:
  - `project-plans/issue446-plan.md`
  - `src/startup.rs`
  - `src/persistence/migration.rs`
  - `src/persistence/migration_tests.rs`
- RED:
  1. Add a startup-boundary test showing schema-1 local `~` state currently returns `CFG-E001`.
  2. Add migration tests for canonical local repository/agent paths and unchanged remote tilde paths.
  3. Run only those focused tests and record the expected failure before production edits.
- GREEN:
  - Expand only leading local home shorthand at the schema-1 migration boundary using the host's existing home-variable convention (`USERPROFILE` before `HOME` on Windows, `HOME` on Unix).
  - Resolve repository identity and local agent work target from that effective path.
  - Store the same effective local agent path in the migrated typed values so restore and launch fingerprints cannot diverge.
  - Preserve remote source values and all unrelated migration behavior.
- REFACTOR:
  - Keep the helper private to migration, small, typed with `Path`/`PathBuf`, and free of environment mutation.
- Verification:
  - Focused RED/GREEN test filters.
  - `make quick-check` during iteration.
  - Exact-head full required suite before check-in: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo build --workspace --all-features --locked`, and `cargo test --workspace --all-features --locked` (or `make ci-check`).
- Stop for approval if the fix requires a dependency, public abstraction/module, schema change, persistence write on startup, UI/TUI work, unrelated test movement/refactor, or files outside the allowed set.

### Slice S2 — Require the psmux lifecycle fix

- Acceptance row: A5.
- Architecture owners: runtime and native-Windows harness version qualification; CI/docs pin the same tested release.
- Approved expansion: the reporter explicitly identified stale psmux/Jefe processes as part of the bug and authorized restarting/upgrading psmux and doing what was needed to make the launch path work.
- RED: runtime, harness, smoke, and docs/CI contract tests reject the existing 3.3.6 minimum and pin when changed to require 3.3.7.
- GREEN: raise the runtime/harness/smoke minimum to 3.3.7, update install guidance, pin the official x64 archive at SHA-256 `60ff7b236f64184921cef3c1ff2611aa5a36fcc7ed8e2a58e968b8ded57f6028`, and verify a real detached session starts and cleans up on 3.3.7.
- Non-goal: implementing a competing Jefe process-kill workaround for defects fixed upstream.

## Expected architecture paths

| Layer | Expected path | Reason |
|---|---|---|
| Startup boundary test | `src/startup.rs` | Proves the reported normal-launch behavior and no-write contract |
| Persistence behavior tests | `src/persistence/migration_tests.rs` | Proves schema-1 to schema-2 local/remote path compatibility |
| Persistence migration | `src/persistence/migration.rs` | Owns interpretation and canonicalization of legacy schema-1 local paths |
| Delivery record | `project-plans/issue446-plan.md` | Acceptance, scope, review, and verification ledger |
| Runtime qualification | `src/runtime/multiplexer.rs`, `src/runtime/multiplexer_tests.rs` | Rejects psmux 3.3.6 before sessions can leak and gives 3.3.7 upgrade guidance |
| Harness qualification | `src/harness/psmux_driver.rs`, `src/harness/psmux_driver_tests.rs`, `tests/psmux_smoke.rs`, `tests/psmux_orphan_reap.rs` | Keeps real native-Windows tests on the same supported lifecycle release |
| Native Windows CI/docs | `.github/workflows/ci.yml`, `CONTRIBUTING.md`, `dev-docs/testing/{psmux-smoke,tmux-harness}.md`, `tests/core/tmux_harness_docs_contracts.rs` | Pins/checksums 3.3.7 and prevents policy drift |

## Scope ledger

| Discovery | Classification | Decision |
|---|---|---|
| psmux 3.3.6 leaves processes behind and hangs isolated `new-session` commands | Blocker—Fix (approved expansion) | Raise every product/test minimum to 3.3.7, pin that release in native Windows CI, and rely on the upstream lifecycle fix rather than adding a parallel cleanup subsystem |
| Migrated local agent typed values currently retain the source path while the launch fingerprint uses a canonical work target | Blocker—Fix | In scope under A2 because otherwise startup can pass but restored execution still receives literal `~` and its typed-value hash describes a different path |
| Existing `services::expand_tilde` cannot be imported by persistence without reversing the architecture DAG | In-scope—Fix | Keep a bounded private legacy-migration helper rather than introducing cross-layer coupling or a new shared public abstraction |
| Remote `~/...` must not use the local home | Blocker—Fix | Preserve by branching on repository locality and prove A3 |

Current scope: 15 files, 383 added / 39 deleted lines including this plan; no scope-budget review trigger. `src/persistence/migration.rs` is 751 lines, above the warning threshold but below the enforced 1,000-line hard limit.

## Review counters

- Pre-PR Open Code Review runs: 0 / 2.
- Post-PR Open Code Review runs: 2 / 2 (both PR-head workflows completed successfully; no issue-specific findings were emitted). The cap is reached; no additional OCR review may be requested.

## Verification evidence

| Check | Result | Evidence |
|---|---|---|
| Current-main baseline | Passed | `main` matched `origin/main`; branch `issue446` created from `cd76557` |
| Issue/comments fetched with `gh` | Passed | Issue body and one CodeRabbit research request read; no response yet |
| RED focused tests | Passed | The startup regression test was copied into an isolated `HEAD` archive and failed with `CFG-E001`, path `~/projects/jefe`, detail `path has no existing ancestor`; the temporary archive was removed afterward |
| GREEN focused tests | Passed | Startup no-rewrite, local canonical repository/agent paths, and remote tilde preservation each pass with `--all-features` |
| psmux S2 RED/GREEN | Passed | Runtime, harness, smoke, and docs/CI contracts first failed against the 3.3.6 minimum/pin, then passed with 3.3.7. A real detached session that timed out on 3.3.6 passed in 1.46 seconds after upgrading to 3.3.7. |
| Quick verification | Passed | GNU Make was unavailable on this Windows host; direct equivalent `cargo fmt --all`, `cargo check -q`, and `cargo test -q` passed (3,812 tests across targets/doctests) |
| Full required suite | Passed | Exact-head format, strict all-target/all-feature Clippy, locked all-feature build, locked all-feature workspace tests with `JEFE_REQUIRE_PSMUX=1`, and `git diff --check` all exited 0 on psmux 3.3.7. |
| First PR CI attempt | Blocker remediated | Native Windows proved psmux 3.3.7 may terminate a detached descendant itself when the pane leader dies; the legacy integration test incorrectly required that child to survive. The test now accepts direct psmux cleanup or applies Jefe's validated reap if a descendant remains, while always requiring no leaked target process/session and an untouched bystander. The focused test passes five consecutive native runs. |
| Second PR CI attempt | Blocker remediated | The orphan target passed, then the psmux smoke retry fixture still returned the obsolete supported version `tmux 3.3.6`; its expected successful recovery is updated to the required `tmux 3.3.7`. |
| Third PR CI attempt | Blocker remediated | Native Windows twice observed the fixture's output/mouse modes before psmux's attached input relay accepted Page-key bytes. The unchanged focused test passed locally, confirming a load-dependent transport-readiness race. The real-transport test now establishes input readiness with a child-echoed probe before asserting PageUp/PageDown and SGR sequences. |
| Bounded review | Passed | GLM review found no Blocker or Reject findings; A1–A4 and planned scope were confirmed. Both permitted post-PR OCR workflows reported success with no policy or infrastructure failure and emitted no issue-specific findings. |

## Review triage and deferred findings

| Finding | Disposition | Decision |
|---|---|---|
| Home-equipped behavior tests silently skipped when no home variable existed | In-scope—Fix | Tests now fail clearly when their required host fixture is unavailable instead of vacuously passing |
| Windows extended-length canonical paths are platform-dependent | Reject | Existing `physical_identity` behavior; expected and migrated paths use the same canonical representation |
| MSYS-style Windows `HOME` guard lacked rationale | In-scope—Fix | Added a focused why-comment without changing behavior |
| Canonical work targets unify empty-id agents expressed with tilde versus absolute home paths | Reject | Intended physical-identity behavior and consistent with repository migration semantics |
| Four real-psmux harness tests time out before creating sessions on psmux 3.3.6 | Blocker—Fix | Reclassified after the reporter identified it as part of issue #446 and authorized the expansion; psmux 3.3.7 fixes the real-session test and is now required |
| Native Windows CI expected the detached fixture child to survive pane-leader termination | Blocker—Fix | psmux 3.3.7 correctly cleaned the child directly, making the old orphan-only premise obsolete. The test now verifies the accepted A5 outcome across both valid implementations: direct psmux cleanup, or Jefe classification/reaping when a validated descendant survives. |
| Loader-retry smoke fixture still recovered to psmux 3.3.6 | Blocker—Fix | Update the success fixture and assertion to 3.3.7 so the retry behavior is tested against the supported minimum rather than intentionally failing qualification. |
| Native Windows mouse smoke sent semantic input immediately after observing output modes | Blocker—Fix | psmux can publish pane output before its attached input relay is accepting bytes under CI load. Establish a child-echoed input-readiness barrier first, then retain the exact Page-key and SGR byte assertions. |
