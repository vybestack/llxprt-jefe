# Issue #496 — Global panic capture and executor-owned app state

## Status

**Accepted for implementation.** The user delegated the bounded architecture and
test-boundary decisions to the implementer and requested completion without
additional approval prompts. The decisions below are therefore authoritative;
the normal hard scope and quality stop conditions still apply.

Planning base: `30344e2bec5c72b30bcbe422ef03bc2811e8e3ae` (current
`origin/main` when the issue branch was created).

## Problem and validated root cause

Jefe installs a file-backed tracing subscriber when configured, but it does not
replace Rust's default process panic hook before entering the TUI. The default
hook writes panic diagnostics to stderr, which corrupts the alternate-screen UI.

The concrete `blocking-N` trigger is
`app_input::terminal_manager::complete_pending_shell_focus`: it copies
`AppStateHandle` into `smol::unblock`, then calls `state.try_read()` from the
blocking worker through `select_pending_runtime_shell` and
`pending_focus_matches`. The complete audit found this is the only non-GitHub
`smol::unblock` closure that accesses `AppStateHandle`/`State<AppState>`.

A panic hook controls diagnostics but does not catch an unwind. Therefore it can
permanently prevent the default hook from printing raw panic text, but it cannot
promise that an arbitrary uncontained panic continues execution long enough for
the render loop to show the queued report. Awaiting a panicked `smol::unblock`
can resume the unwind on the executor.

## Accepted implementation decisions

1. **Capture, not broad recovery.** Install a global hook that never delegates to
   the default hook, logs and queues every panic, and lets normal Rust unwind
   semantics continue. Panics contained by an existing safe boundary can be
   drained into Errors; an uncontained fatal panic is still captured/logged and
   emits no raw terminal text, but is not promised to render in-app. Remove the
   known off-thread state trigger rather than adding broad `catch_unwind` around
   runtime work with potentially partial external side effects.
2. **Configured logging contract.** "Recorded in the log file" means recorded
   when Jefe's existing file tracing sink is configured and initialized. Do not
   create a new default log location or change logging privacy/storage policy.
3. **Private ownership.** Keep `panic_capture` private to the binary; do not add
   a public library API. Install it after `logging::init()` and before terminal
   initialization / `smol::block_on`.
4. **Silent semantics.** A silent panic entry is retained in the full Errors
   ring and never becomes the status banner or changes screen/focus/error-list
   selection/scroll. It does not erase an older unrelated visible error banner.
5. **Bounded queue.** Cap undrained panic reports at the existing 50-entry error
   capacity and retain the newest reports. Do not add a panic-recovery or
   redaction subsystem.
6. **No debugger delegation.** Debugger-specific delegation is a non-goal because
   the previous/default hook may write to the terminal.
7. **TUI evidence boundary.** The real app has no deterministic legal input that
   intentionally panics. A required real-TTY test cannot be added without a new
   test boundary. Approve a narrowly scoped, non-shipping integration fixture
   that exercises the same panic-capture-to-Errors projection in the schema-1
   TUI harness, with child-process tests separately proving real startup hook
   ordering, blocking-pool capture, configured logging, and empty stderr. The
   fixture design must be reviewed before implementation and must not add a
   production panic trigger, public API, dependency, or quality-rule exception.

If every arbitrary panic must recover and render in-app, or every run must create
a log file, stop this bounded plan and approve a separate recovery/logging
architecture first.

## Decision-complete acceptance matrix (conditional on the decisions above)

| ID | Actor / launch path | Input and boundaries | Target | Observable success | Observable failure / diagnostics | Side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| A1 | Normal Jefe TUI startup after logging initialization | Any panic whose hook runs after installation; string and non-string payloads; optional source location; named or unnamed thread | Local/remote terminal; macOS, Linux, Windows | Global hook emits no stdout/stderr, extracts payload/location/thread metadata, traces the report, and queues it | Uncontained unwind retains Rust fail-fast semantics; report is available to configured tracing and queue before unwind continues | Queue append and configured trace event only | No durable-state schema change; CLI paths completed before hook installation remain unchanged | Fresh child process installs the real hook, triggers a caught panic, and proves stdout/stderr do not contain the payload and one report drains |
| A2 | Blocking-pool worker with survivable containment | Panic inside `smol::unblock`, caught at the test boundary after the hook runs | macOS/Linux/Windows | Report includes worker identity and payload, configured log contains it, queue drains exactly once | Logging is absent only when the existing sink is unconfigured/failed; queue remains the in-process diagnostic | Same as A1 | Existing logging opt-in behavior is preserved | Cross-platform child-process test with unique configured log file; no exact `blocking-N` suffix assertion |
| A3 | App-shell poll loop | Zero, one, or multiple queued reports; queue overflow retains newest 50 | TUI executor thread | Drain converts reports into silent `ErrorSource::Panic` entries | Poisoned queue ownership is recovered inside the hook boundary without terminal output; no app-state access occurs in the hook | Errors ring mutation on executor only | Errors remain runtime-only | State/integration test proves ordered exactly-once drain and capacity behavior |
| A4 | Status bar and Errors screen | Silent panic after no visible error and after an older visible error | All TUI platforms | Panic is listed and selectable/copyable on Errors; it never becomes `ERR:` and does not change screen, focus, selection, or scroll | Older visible error remains the visible banner until existing behavior clears/replaces it | Append to Errors ring only | `silent` deserializes as false when absent for compatibility | State tests plus projection tests for `Source: Panic`; approved schema-1 fixture scenario verifies frame behavior |
| A5 | Pending shell-focus completion | Pending owner/generation stable, stale before select, stale after select, runtime success/failure | tmux and psmux paths | Worker receives only plain snapshot/context runtime inputs; multiplexer select is off-thread; executor revalidates and commits/compensates | Typed runtime error follows existing reducer behavior; stale result is not committed and existing compensation runs | Multiplexer select may occur before executor detects staleness, matching current contract | No persistence format change | Focused orchestration regression plus existing generation/reducer tests; no source-text-only proof |
| A6 | Existing GitHub worker containment | Contained and uncontained caught worker panics after global hook installation | All platforms | `worker_panic::contain` still attributes contained panic site and suppresses delegation; uncontained panic delegates to global capture | Existing GitHub worker failure behavior remains unchanged | Existing Errors behavior for contained GitHub failures | No route migration | Child-process composition test installs global hook first, then exercises contained and uncontained-caught cases |
| A7 | Exact candidate head | All accepted paths and boundaries | Local gates plus native Windows CI | Format, policy, architecture, strict/complexity Clippy, coverage, locked build, tests, scenario, CI, ancestry, and conflict checks pass | Any interrupted/skipped/stale-SHA gate is incomplete | None beyond test fixtures | No compatibility regression | `cargo xtask ci`, approved TUI fixture gate, PR CI and review evidence on exact SHA |

## Explicit non-goals

- No migration of the 31 `gh_async::spawn_gh_work` routes; their centralized
  `worker_panic::contain` path remains unchanged.
- No broad panic recovery or generic task-supervision subsystem.
- No claim that `set_hook` catches unwind, handles abort/signals/faults, or can
  render after a fatal executor unwind.
- No default log-file policy, logging documentation expansion, or payload
  redaction subsystem.
- No iocraft/generational-box/vendor changes.
- No project-wide `unwrap`/`expect` cleanup.
- No debugger-specific previous-hook delegation.
- No production panic trigger, hidden production test command, public panic API,
  dependency change, quality-gate change, lint suppression, threshold increase,
  unrelated refactor, or unrelated test move.
- No change that clears an older visible error banner when a silent panic arrives.
- No defensive `catch_unwind` around runtime operations that already return
  typed errors.

## Bounded vertical slices

### Slice 1 — Silent Panic domain/state/UI semantics (A3, A4)

- **Owners:** domain model, deterministic state, pure projection, thin UI label.
- **Allowed paths:** `src/domain/errors.rs`, `src/state/errors_types.rs`,
  `src/state/selectors.rs`, `src/state/errors_tests.rs`,
  `src/selection/errors_content.rs`, `src/selection/content_tests.rs`,
  `src/ui/screens/errors.rs`.
- **RED:** silent insertion/visible-selector/focus-selection-scroll/serde tests
  and Panic source projection test fail for missing behavior.
- **GREEN:** silent entries remain in the ring while status projection skips them.
- **Stop:** any durable schema migration, new public abstraction, or unrelated UI
  redesign becomes necessary.
- **Focused verification:** relevant lib tests and `cargo xtask quick`.

### Slice 2 — Global capture, logging, queue drain, and hook composition (A1-A4, A6)

- **Owners:** binary startup and app-shell orchestration boundary.
- **Allowed paths:** new private `src/panic_capture.rs`, `src/main.rs`,
  `src/app_shell.rs`, and test-only changes in `src/app_input/worker_panic.rs`.
- **RED:** child-process blocking-worker test proves current default-hook stderr;
  hook-composition test proves global capture is absent before implementation.
- **GREEN:** private hook installs after logging, queues bounded reports, traces to
  configured sink, drains on executor, and composes with existing containment.
- **Stop:** app-shell exceeds 1,000 lines, hook needs a public library module,
  logging policy must change, or broad recovery is required.
- **Focused verification:** child tests, app-shell/state tests, source-size and
  architecture checks, then `cargo xtask quick`.

### Slice 3 — Executor-owned pending focus state (A5)

- **Owner:** terminal-manager orchestration boundary.
- **Allowed paths:** `src/app_input/terminal_manager.rs` and an existing adjacent
  test module if required.
- **RED:** behavioral orchestration test proves stale pending focus is not
  confirmed and compensation remains correct while select work is off-thread.
- **GREEN:** no `AppStateHandle` enters the worker; pre/post validation and typed
  runtime handling pass.
- **Stop:** a new public runtime trait, process subsystem, vendor change, or
  unrelated terminal-manager refactor is required.
- **Focused verification:** terminal-manager/reducer tests and `cargo xtask quick`.

### Slice 4 — TUI evidence and exact-head delivery (A4, A7)

- **Owner:** schema-1 non-shipping test fixture only.
- **Allowed paths:** to be finalized after approval and design inspection;
  expected under `dev-docs/tmux-scenarios/v1/` plus the smallest existing fixture
  registration/test boundary.
- **RED:** scenario shows missing Panic entry or status suppression before the
  behavior exists.
- **GREEN:** real-TTY frame excludes payload/`ERR:` while Errors projection shows
  Panic; child test remains the strict stderr-channel proof.
- **Stop:** production panic orchestration, dependency/manifest change, public
  abstraction, or more than the approved fixture files is required.
- **Verification:** scenario target and full `cargo xtask ci` on exact head.

## Expected files by architectural layer

| Layer | Expected path | Acceptance |
|---|---|---|
| Plan | `project-plans/issue496-plan.md` | workflow evidence |
| Binary boundary | `src/panic_capture.rs` (new) | A1-A3 |
| Startup | `src/main.rs` | A1, A6 |
| App orchestration | `src/app_shell.rs` | A3 |
| Runtime orchestration | `src/app_input/terminal_manager.rs` | A5 |
| Worker composition test | `src/app_input/worker_panic.rs` | A6 |
| Domain | `src/domain/errors.rs` | A3, A4 |
| State | `src/state/errors_types.rs`, `src/state/selectors.rs` | A3, A4 |
| State tests | `src/state/errors_tests.rs` | A3, A4 |
| Projection | `src/selection/errors_content.rs` | A4 |
| Projection tests | `src/selection/content_tests.rs` | A4 |
| UI | `src/ui/screens/errors.rs` | A4 |
| TUI evidence | `dev-docs/tmux-scenarios/v1/panic-capture-errors.json`, `src/bin/jefe-harness-probe.rs`, `tests/harness_v1_fixtures.rs` | A4, A7 |
| Typed capture route | `src/messages/errors.rs`, `src/messages/errors_conversion.rs`, `src/messages/event_conversion.rs`, `src/state/events.rs`, `src/state/errors_ops.rs` | A3, A4 |
| Executor drain boundary | `src/app_shell_panic.rs` | A3 |

Final scope is 22 files and 881 net added lines after rebasing onto current main,
below the 25-file / 1,500-line target. The mainline liveness extraction reduced
`src/app_shell.rs` below its former 1,000-line limit; the issue integration does
not add a second orchestration path.

## Off-thread audit ledger

| Site | App state in worker? | Disposition |
|---|---:|---|
| `src/app_shell_workers.rs` liveness closure | No | Leave unchanged |
| `src/app_shell_workers.rs` persistence closure | No | Leave existing typed/caught callback boundary unchanged |
| `src/app_shell_workers.rs` history closure | No | Leave unchanged |
| `src/app_input/terminal_manager.rs` preview closure | No | Leave unchanged |
| `src/app_input/terminal_manager.rs` pending-focus closure | **Yes** | Fix in Slice 3 |
| `src/app_input/shell_overlay.rs` existence closure | No | Leave unchanged |
| `src/app_input/shell_overlay.rs` inventory closure | No | Leave unchanged |
| `src/app_shell.rs` liveness closure | No | Leave unchanged |
| `src/app_shell.rs` attach closure | No | Leave unchanged |
| `src/app_shell.rs` persistence scheduling closure | No | Leave unchanged |
| `src/app_input/gh_async.rs` centralized worker closure | No direct state; contained | Explicitly excluded with all 31 routes |

No other non-GitHub `smol::unblock` state capture was found. Reviewer suggestions
to add broad containment are outside the accepted architecture unless concrete
source evidence changes this ledger.

## Scope ledger

| Status | File / discovery | Mapping / disposition |
|---|---|---|
| Complete | `project-plans/issue496-plan.md` | mandatory bounded workflow record |
| Complete | production/domain/state/projection paths listed above | A1-A6 |
| Complete | adjacent state/projection/worker tests | behavioral evidence for A1-A6 |
| Complete | schema-1 scenario, probe command, and fixture registration | non-shipping real-PTY evidence for A4/A7 |
| Complete | typed Errors message/event/reducer route | preserves unidirectional state ownership for A3/A4 |
| Reject | all other audited `smol::unblock` closures | no app state access; no defect to fix |
| Defer | unconditional default log file | separate logging/privacy policy |
| Defer | generic panic recovery/task supervision | separate architecture with consistency semantics |

Every changed file must be entered here and map to an acceptance row before edit.
Stop for approval above 25 files or 1,500 net changed lines; stop unconditionally
without explicit approval above 40 files or 2,500 net changed lines.

## Review counters and finding dispositions

- DeepThinker planning analysis: complete.
- Local Rust/DeepThinker review cycles: 1 / 2, all findings triaged.
- Local CodeRabbit review: complete, 2 / 2 findings **In-scope-Fix** and resolved.
- OCR before PR: 1 / 2, 3 / 3 findings **In-scope-Fix** and resolved.
- OCR after PR: 0 / 2.
- CodeRabbit PR findings: pending PR.

Review dispositions: preserve selected error identity; revalidate stale
same-owner generations; drain unconditionally on the executor; route capture
through typed reducer messages; add real-PTY evidence; assert blocking-worker
stdout/stderr, log, capacity, non-string payload, and exact-once behavior; retain
the newest silent report at full capacity; use `VecDeque` for bounded queue
eviction; and isolate fixture capture from stale reports. The proposed
hook-reentrancy wrapper was **Reject** because hook code cannot safely recover a
subscriber panic, and the accepted fail-fast contract avoids layered panic
recovery.

## Verification evidence

| Candidate SHA | Command / evidence | Result |
|---|---|---|
| `4a976a22` | Slice RED compile/tests | failed for missing Panic/silent/hook contracts as intended |
| `4a976a22` | strict Clippy, format, clippy-allow, source-size, architecture, locked workspace build | pass after rebase |
| `4a976a22` | blocking hook, worker composition, silent state/selection/capacity, stale focus regressions | pass |
| `4a976a22` | `panic_capture_fixture_projects_silent_error_without_raw_terminal_output` | pass in schema-1 real PTY |
| pre-rebase reviewed head | `cargo xtask ci` through format/policy/architecture/strict+complexity Clippy/coverage | pass; locked all-test phase exposed only the existing editor-fixture timeout |
| pre-rebase reviewed head | locked all-feature Jefe suite with editor fixture isolated serially | pass |
| current `origin/main` | strict Clippy/all-test gate | blocked by `actions_tests_sort.rs` using the obsolete four-argument `Repository::new`; same source exists on origin/main and is outside issue scope |
| `4a976a22` | DeepThinker/Rust review, local CodeRabbit, OCR 1/2 | complete and triaged |
| `4a976a22` | ancestry | rebased: one issue commit ahead, zero behind `origin/main` |
| pending PR head | native Windows / PR CI / PR CodeRabbit / conflict check | pending |

## Deferred findings / follow-ups

No follow-up issue has been created. If approval requires unconditional panic
recovery or default logging, create separate decision-complete issues rather
than expanding issue #496 automatically.
