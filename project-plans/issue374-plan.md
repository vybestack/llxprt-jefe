# Issue 374 delivery plan: partition blocking runtime operations from AppContext

## Issue and decisions

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/374
- Branch: `issue374`
- Base: `origin/main` at `529085a`
- Delivery: one bounded PR.
- Architecture decision: use concrete immutable runtime snapshots and operation-specific stale guards. Do not introduce a generic `RuntimeOperation` framework because attach, capture, persistence, shell-overlay, and pending-focus operations have different ownership and generation identities.
- Input decision: F10/F12 remain synchronous. Multiplexer subprocesses execute without an `AppContext` guard, while existing success/failure ordering remains unchanged.
- Stale-owner decision: an open/select result whose attached owner changed is discarded and best-effort window-0 compensation restores the hidden-shell invariant.
- Preview decision: remove dead-pane multiplexer capture from the render path by capturing once during off-lock liveness detection and storing runtime-only preview lines.
- Cache decision: mouse history reads use existing cache data and preserve geometry on cache miss; no synchronous capture is performed while an `AppContext` guard is held.

## Acceptance matrix

| ID | Actor / path | Boundary case | Observable success | Failure and permitted side effects | Persistence / compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Visible-shell exit observer | Overlay owner/generation may change while probing | Shell-existence subprocess runs without an `AppContext` guard; matching natural exit restores the correct surface | Probe error warns and retries; stale result is discarded | Runtime-only; Unix and psmux commands unchanged | snapshot/guard unit tests, observer seam test, existing shell scenarios |
| A2 | Hidden-shell inventory observer | Batched probe failure or concurrent inventory change | Observation runs without an `AppContext` guard and removes only confirmed missing entries | Any probe error retains inventory and warns | Existing runtime-only inventory | observer seam and reconciliation tests |
| A3 | Dashboard F10 open/resume | Background attach changes owner during operation | Snapshot is taken under a short lock, subprocess runs off-lock, matching owner opens/resumes and resizes | Runtime error preserves UI and warns; stale owner does not dispatch success and selects window 0 best-effort | Existing key semantics and single viewer | stale-owner decision tests and issue364 scenario |
| A4 | Visible-shell F10 close / F12 hide | Multiplexer operation fails | Kill/select-0 executes off-lock and reducer transition occurs only after success | Existing warning; overlay remains retryable | Existing shortcuts and lifecycle | operation seam tests and issue222/364 scenarios |
| A5 | Terminal Manager Enter | Pending generation or attached owner changes | Select-existing-only operation runs off-lock and confirms only the matching pending request | Stale result is discarded/compensated; failure warns and clears matching request | No shell recreation or second viewer | focus decision tests and issue364 scenario |
| A6 | Startup and shutdown | Unknown/orphan shells or close failures | Stateless observe/close-all operations run without `AppContext`; startup classification and best-effort shutdown remain intact | Failures retain diagnostic behavior and never kill agent sessions | Existing tmux/psmux plans unchanged | reconciliation/runtime tests |
| A7 | Dead-agent render preview | Dead pane selected for repeated frames | Pane capture occurs once in off-lock liveness work; render uses runtime-only cached lines and never shells out | Capture failure logs and leaves preview empty; stale liveness result is discarded | Preview is not persisted | liveness reducer/projection tests |
| A8 | Mouse scroll/selection | Context contention or cold cache | Cached history is read without a multiplexer subprocess; geometry is preserved on miss | Empty fallback only where existing contention already permits it | Existing selection behavior | cache/geometry contention tests |
| A9 | Existing attach path | Attach completes after target changes | Existing snapshot/build/apply path rejects stale attachment and preserves one viewer | Existing typed failure behavior | No attach architecture change | existing attach tests and scenarios; new snapshot semantics tests only |

## Explicit non-goals

- No cloneable runtime manager, second live viewer, weakened owner/generation checks, or persisted runtime preview/inventory.
- No generic operation trait/framework.
- No asynchronous pending state machine for F10/F12.
- No multiplexer argument, executable-resolution, dependency, workflow, quality-tool, `.llxprt/`, or `.github/` changes.
- No broad refactor of attach/capture/persist workers that already follow the accepted pattern.
- No unrelated launch/kill/relaunch input-path refactor.
- No poison-recovery hardening beyond behavior required by issue 374.
- No removal of legacy runtime trait methods merely because call sites become unused.

## Bounded vertical slices

### S1: immutable shell-operation snapshot boundary

- Acceptance: A1-A6, A9.
- Owner: runtime shell-window boundary.
- Allowed paths: `src/runtime/shell_window.rs`, `src/runtime/mod.rs`, `src/runtime/shell_window_tests.rs`, `src/runtime/manager_tests.rs`.
- RED: tests require typed local snapshot inputs, remote/missing rejection, and attached-owner matching.
- GREEN: execute functions accept owned snapshot fields rather than a manager guard; command construction is unchanged.
- Stop: any generic operation framework or `RuntimeManager` trait expansion.

### S2: shell observers and lifecycle operations

- Acceptance: A1-A6.
- Owner: shell input/orchestration boundary plus deterministic stale guards.
- Allowed paths: `src/app_input/shell_overlay.rs`, `src/app_input/terminal_manager.rs`, `src/app_init_shell_reconcile.rs`, `src/state/shell_overlay_ops.rs`, focused tests.
- RED: closure seams prove subprocess execution can acquire the context lock; stale owner/generation decisions reject apply.
- GREEN: short-lock snapshot, off-lock execution, revalidation/compensation, unchanged failure ordering.
- Stop: new user-visible state or async shortcut machinery.

### S3: cache-only selection history

- Acceptance: A8.
- Owner: existing capture worker/cache boundary.
- Allowed paths: `src/app_shell_workers.rs`, `src/mouse_routing.rs`, `src/mouse_routing_tests.rs`, existing focused tests.
- RED: cold-cache geometry remains unchanged and contention returns without blocking.
- GREEN: no mouse path invokes `RuntimeManager::capture_history` under a guard.
- Constraint: `src/mouse_routing.rs` must remain at or below 1,000 lines.

### S4: capture dead preview at liveness boundary

- Acceptance: A7.
- Owner: existing off-lock liveness worker and deterministic state event.
- Allowed paths: `src/app_shell.rs`, `src/app_shell_workers.rs`, `src/runtime/capture_ops.rs`, state event/type/reducer files, focused tests.
- RED: repeated dead snapshot projection requires cached lines and no runtime context.
- GREEN: off-lock liveness result carries preview lines, stale apply is rejected, render is pure/cache-only.
- Stop: a new worker subsystem or persisted preview.

## Expected paths and scope

Expected: 16-20 files and fewer than 1,200 net changed lines. Target remains 25 files / 1,500 net lines; stop for approval above 40 files or 2,500 net lines.

## Scope ledger

| Discovery | Classification | Disposition |
| --- | --- | --- |
| Shell open/close/hide/select/observe/close-all and pane capture execute subprocesses | In-scope | Partition concrete call sites using immutable inputs |
| Resize, PTY writes, snapshots, dirty flags, generations, and attached-owner reads are in-memory/PTY operations | Reject | Do not refactor |
| Attach/capture/persist already snapshot and stale-guard | In-scope evidence | Reuse patterns; do not rewrite |
| Explicit launch/kill/relaunch paths also hold the lock across work | Defer | Separate follow-up; not background terminal operations accepted here |
| Poison-recovery tests | Defer | Existing unrelated hardening |
| `manager.rs` is near 1,000 lines; `mouse_routing.rs` is at 1,000 | Blocker constraint | Add runtime code in shell/capture modules; keep mouse edit net non-positive |

## Review counters

- Pre-PR Open Code Review: 1 completed full review; 1 OCR invocation terminated without output / 2 permitted successful runs.
- Post-PR Open Code Review: 0 / 2.

## Verification evidence

| Candidate | Command | Result |
| --- | --- | --- |
| Base | architecture/source inspection | confirmed blocking subprocesses under `AppContext` in shell observers, shell shortcuts, manager focus, dead preview, and mouse history |
| RED | focused snapshot/locking tests during implementation | failed before concrete shell inputs, lifecycle guards, and cache-only geometry were implemented |
| GREEN | `cargo test --lib shell_window::snapshot_tests -- --nocapture` | PASS: 10 typed snapshot, lifecycle-stale, remote, and pane-target tests |
| GREEN | focused shell-overlay/dead-preview/terminal-manager tests | PASS: 1 F10 route, 22 overlay reducer, 17 manager, and dead-preview cache tests |
| GREEN | `scripts/issue222-run-scenario.sh` | PASS: 50 real-tmux steps |
| GREEN | `scripts/issue364-manager-run-scenario.sh` | PASS: 84 real-tmux steps |
| GREEN | `make ci-check` plus exact fmt/clippy/build/test rerun | PASS after strict Clippy remediation |
| Scope | working-copy review | 15 files, below 25-file / 1,500-line target |

## Deferred findings

- None created yet. Any valid out-of-scope discovery will be recorded as a follow-up rather than expanding this PR.
