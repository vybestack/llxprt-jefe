# Issue #332 — Windows rebuild leaves dead psmux pane and orphaned LLxprt process

## Problem

On native Windows with psmux, exiting the Jefe dashboard and rebuilding/restarting
Jefe while an LLxprt agent remains running can kill the psmux **pane leader** (the
intermediate `jefe.exe --jefe-internal-agent-launch` supervisor) without terminating
the descendant LLxprt process tree. On restart:

- Jefe marks the agent `Dead`, clears its `runtime_binding` (`None`), and cannot reattach.
- Pressing `l` launches a **second** LLxprt with `--continue`, leaving the first process
  tree orphaned.
- The old psmux session and old LLxprt descendants persist until manually cleaned up.

Root cause: the psmux `pane_pid` captures the **launcher** (`jefe.exe`), not the real
worker, and there is no Job-Object / process-group binding grouping the worker tree to
the pane leader. So when the pane leader dies, the worker tree survives as an orphan,
and Jefe's liveness classifier treats a live descendant PID under a dead pane as
`Recoverable` instead of `Orphaned`.

This is the active orphan-recovery follow-up deferred by #121, distinct from the
live-session startup reconciliation (#323/#326) and healthy-session switching (#324).

## Acceptance Matrix

| # | Actor / Launch Path | Input / Boundary | Observable Success | Observable Failure / Diagnostic |
|---|---|---|---|---|
| AC1 | `classify_orphan_state` (pure) | pane=Dead, session=Exists, recorded worker identities=[], observed descendants=[] | `DeadPaneNoWorker` | — |
| AC2 | `classify_orphan_state` (pure) | pane=Dead, session=Exists, recorded identities=[X], observed descendants=[X alive, identity matches] | `DeadPaneWithOrphans` | — |
| AC3 | `classify_orphan_state` (pure) | pane=Alive, session=Exists, observed descendants=[X alive] | `NoOrphan` (healthy/reattachable) | — |
| AC4 | `classify_orphan_state` (pure) | pane=Dead, observed descendant PID alive but `ProcessIdentity` mismatches recorded anchor (PID reuse) | `DeadPaneNoWorker` (reuse rejected, NOT treated as orphan) | — |
| AC5 | `reap_orphan_tree` | validated identities only; non-matching PIDs skipped | terminates only validated members; returns `Ok`/typed `Err`; never panics | reap failure → typed `OrphanReapError`, logged warn, does not abort caller |
| AC6 | `reap_orphan_tree` | best-effort, agent-scoped | unrelated processes/sessions untouched | — |
| AC7 | `run_launch_plan` (Windows) | any wrapper kind | spawned child + descendants assigned to a Job Object with `KILL_ON_JOB_CLOSE`; pane-leader death reaps whole tree | Unix path unchanged |
| AC8 | `RuntimeBinding` (serde) | new `worker_identities: Vec<ProcessIdentity>` field, `#[serde(default)]` | old `state.json` without field loads with `[]` (backward compat) | — |
| AC9 | `spawn_session_internal` / reattach | after `pane_pid` captured | enumerates launch tree, stores resolved worker identities on `RuntimeSession`, surfaces into persisted `RuntimeBinding` | remote sessions: no local enumeration |
| AC10 | `classify_startup` (pure) | session=Missing/dead-pane + live **validated** descendants (orphan evidence) | `Orphaned` (NOT `Recoverable`) | — |
| AC11 | `classify_startup` (pure) | session=Alive + live worker | `Running` (binding preserved — #323/#324/#326 behavior unchanged) | — |
| AC12 | startup reconcile | agent classified `Orphaned` | `reap_orphan_tree` + `kill_session` invoked, then `Dead` + `runtime_binding=None` only AFTER reap attempted; best-effort, warn-don't-fail | reap/kill failure never aborts startup |
| AC13 | periodic liveness | dead pane whose recorded worker identities still alive | orphan verdict surfaced, guarded by `lifecycle_generation` staleness; remote agents excluded from local reaping | — |
| AC14 | `spawn_relaunch_session` | recorded orphan present, not yet reaped | relaunch BLOCKED — no duplicate `--continue` worker; typed `RuntimeError` + user-facing `error_message` | — |
| AC15 | `spawn_relaunch_session` | orphan reaped successfully | relaunch proceeds (single worker) | — |
| AC16 | `confirm_delete_agent` / `kill_agent_before_delete` | recorded worker identities present | invokes `reap_orphan_tree` + best-effort session kill, all non-fatal, before state removal | cleanup failure never blocks record removal |
| AC17 | `delete_selected_agent` | bogus/missing `work_dir` | agent record removed without error; opt-in `delete_work_dir` unchanged | — |
| AC18 | real-psmux regression (healthy) | unique `-L` namespace, deterministic fixture, Drop cleanup | exactly one worker remains attachable after simulated dashboard exit/restart | leaves no sessions/processes behind |
| AC19 | real-psmux regression (orphan) | fixture backgrounds long-lived child, marker-file PID, kill pane leader | reap terminates only validated descendants, removes only target session | no leaked sessions/processes |

## Non-Goals

- Restructuring the launcher to exec-replace the worker (Design Choice 1, Option 2) — rejected: Windows has no true exec; Job Object chosen.
- Re-deriving descendants at reap time via cmdline/cwd heuristics (Design Choice 2, Option 2) — rejected: persisted `ProcessIdentity` anchors chosen.
- Changing `binding_evidence()` semantics or merging reconcile/restore passes.
- Remote-agent local process reaping (remote agents stay excluded from `reap_orphan_tree`).
- Work-directory deletion on delete (remains opt-in via `delete_work_dir`).
- Automatic binding-refresh during normal (non-restart) operation.
- Tmux/Unix orphan recovery beyond structural parity for testability (Unix `/proc` walk kept minimal; the orphan scenario is Windows/psmux-specific).

## Implementation Plan (Bounded Vertical Slices)

> **Scope note:** This is a 5-phase effort touching >3 architectural layers
> (runtime/process, domain, app_init reconciliation, app_input relaunch/delete,
> tests). Per ISSUE-DELIVERY §3/§6 it is split into independently-testable slices.
> Each slice is one green commit. The full effort is projected near the soft
> 25-file / 1,500-line target; a mandatory scope review occurs after Phase 3.
> Hard stop without approval above 40 files / 2,500 net lines.

### Slice 1 — Phase 1: orphan classifier + reap primitive

**Architecture owner:** `src/runtime/orphan.rs` (new), `src/runtime/mod.rs`, `src/runtime/process.rs` (reuse)

**Allowed files:**
- `src/runtime/orphan.rs` (new) — `OrphanClassification` enum, pure `classify_orphan_state`, `#[cfg(windows)]` descendant enumerator (winsafe `CreateToolhelp32Snapshot`), `#[cfg(unix)]` `/proc` walk, `reap_orphan_tree` with PID-reuse guard
- `src/runtime/orphan_tests.rs` (new) — pure-classifier unit tests (AC1–AC4)
- `src/runtime/mod.rs` — `mod orphan;` + `pub use`

**RED:** `classify_orphan_state` truth-table tests fail (AC1–AC4) before implementation.
**GREEN:** implement pure classifier + thin OS probes + `reap_orphan_tree`.
**REFACTOR:** decompose into small helpers to stay under complexity limits.
**Verification:** `make quick-check` (orphan_tests focused).
**Stop-for-approval triggers:** any new public abstraction beyond `orphan.rs`; any dependency change.

### Slice 2 — Phase 2: Windows Job Object binding + persist worker identities

**Architecture owner:** `src/runtime/agent_launcher.rs`, `src/domain/mod.rs`, `src/runtime/manager.rs`

**Allowed files:**
- `src/runtime/agent_launcher.rs` — Windows Job Object (`KILL_ON_JOB_CLOSE`) wrapping spawn; Unix unchanged
- `src/domain/mod.rs` — `RuntimeBinding.worker_identities: Vec<ProcessIdentity>` (`#[serde(default)]`)
- `src/runtime/manager.rs::spawn_session_internal` — enumerate launch tree, store on `RuntimeSession` + binding
- `src/runtime/session.rs` — carry `worker_identities` field (if needed)
- threading helpers: `src/app_input/agent_runtime.rs`, `src/app_init.rs`, `src/app_input/relaunch.rs` (field propagation only)

**RED:** serde backward-compat test (AC8); Job-Object assignment test (AC7).
**GREEN:** implement binding + capture/persist.
**Verification:** `make quick-check`.
**Stop-for-approval triggers:** changing wrapper command construction; behavior absent from AC7–AC9.

### Slice 3 — Phase 3: startup + periodic liveness `Orphaned` classification

**Architecture owner:** `src/app_init.rs`, `src/runtime/liveness.rs`

**Allowed files:**
- `src/app_init.rs` — `StartupClassification::Orphaned` variant; extend `classify_startup`; reap-then-Dead in `reconcile_running_agents`/`apply_dead_reconciliations`/`restore_runtime_sessions`
- `src/runtime/liveness.rs` — orphan verdict in `batch_liveness_check_with_identity`/`reconcile_dead_agents_with_identity` (generation-guarded, remote-excluded)
- `src/app_init_shell_reconcile.rs` — pattern reference only (no change unless reap helper shared)

**RED:** extend `classify_startup` truth-table tests for `Orphaned` (AC10, AC11); prove dead-pane+orphans no longer `Recoverable`.
**GREEN:** wire classifier → reap → Dead; periodic liveness route.
**Verification:** `make quick-check`; **mandatory scope review here** (file/line tally).

### Slice 4 — Phase 4: relaunch guard + best-effort delete cleanup

**Architecture owner:** `src/app_input/relaunch.rs`, `src/app_input/modal_handlers.rs`, `src/state/state_ops.rs`, `src/runtime/errors.rs`

**Allowed files:**
- `src/app_input/relaunch.rs` — re-probe/reap before spawn; block on unreaped orphan (AC14/AC15)
- `src/runtime/errors.rs` — `RuntimeError` variant for cleanup-pending/blocked (or reuse `AlreadyRunning`)
- `src/runtime/stub_manager.rs` — failure-injection point for behavioral test
- `src/app_input/modal_handlers.rs` — `reap_orphan_tree` + session kill on delete (AC16)
- `src/state/state_ops.rs` — confirm missing work_dir tolerated (AC17)
- `src/app_input/relaunch_tests.rs`, `src/app_input/modal_handlers_tests.rs` — behavioral tests

**RED:** relaunch-blocked test (AC14); delete-with-bogus-workdir test (AC17).
**GREEN:** implement guard + resilient delete.
**Verification:** `make quick-check`.

### Slice 5 — Phase 5: real-psmux regressions + final verification

**Architecture owner:** `tests/` (psmux-smoke)

**Allowed files:**
- `tests/psmux_orphan_reattach.rs` (new, `#[cfg(all(windows, feature = "psmux-smoke"))]`) — AC18 (healthy reattach, single worker)
- `tests/psmux_orphan_reap.rs` (new) — AC19 (dead pane + descendants reap validated tree only)
- existing fixture `jefe-psmux-smoke-fixture` reused or small new fixture if needed

**Conventions:** unique `-L` namespace, `Drop` guard cleanup, `skip-unless-JEFE_REQUIRE_PSMUX` gating.
**Verification:** `make ci-check` (fmt, clippy `-D warnings`, build `--locked`, test `--locked`, coverage ≥30).

## Scope Ledger

| Date | Item | Disposition |
|---|---|---|
| 2026-07-25 | Initial plan (5 phases from CodeRabbit issue enrichment) | Accepted pending user approval |
| 2026-07-25 | Decision: stacked PRs (option b); Unix recovery = non-goal; new `OrphanBlocked` RuntimeError variant | Accepted (coordinator default) |
| 2026-07-25 | Slice 1 complete: `src/runtime/orphan.rs` + `orphan_tests.rs` (11 tests, AC1–AC4 + edge cases) | Green |
| 2026-07-25 | PRE-EXISTING clippy error `manual_is_multiple_of` at `src/harness/v1/validate.rs:114` (clippy-version drift, present on `main`) | Reject (out of scope; will block `make ci-check -D warnings` until fixed separately) |
| | Mandatory scope review after Slice 3 (Phase 3) | Pending |
| | Hard-budget check (40 files / 2,500 lines) | Pending |

## Review Counters

- OCR pre-PR: 0/2
- OCR post-PR: 0/2

## Verification Evidence

- **Slice 1 (orphan primitive):** `cargo test --lib orphan` → 11 passed / 0 failed. `cargo clippy --lib` on orphan code: clean (only pre-existing `validate.rs` error remains, see scope ledger). `cargo fmt --all -- --check`: clean.

## Open Questions for User (require decision before implementation)

1. **Scope/delivery shape:** The CodeRabbit plan is 5 phases. Options:
   - (a) Deliver all 5 phases in one PR against this plan (risk: near/over soft budget; needs scope review at Slice 3).
   - (b) Split into **stacked PRs**: Slice 1–2 (primitive + Job Object) as a foundation PR, Slice 3–5 (classification + guards + regressions) as a follow-up PR. Lower risk per PR.
   - (c) Deliver only Phase 1 (orphan primitive + pure classifier + unit tests) first as a vertical foundation, then re-plan the wiring.
2. **Unix structural parity:** The plan keeps the Unix `/proc` enumerator minimal (testability only) since the orphan scenario is Windows/psmux-specific. Confirm Unix is out of scope for actual orphan recovery behavior (non-goal above) — or should Unix liveness also gain `Orphaned` handling?
3. **`RuntimeError` variant vs reuse:** Add a new `CleanupPending`/`OrphanBlocked` variant (clearer) or reuse `AlreadyRunning` semantics (smaller diff)? Plan currently leaves this open in Slice 4.
