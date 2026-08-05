# Issue 662: jefe can die leaving zero diagnostic record anywhere, making termination undiagnosable

Issue: https://github.com/vybestack/llxprt-jefe/issues/662

## Summary

Jefe terminated twice and left no attributable record. The log stops mid-stream,
the panic hook never fired, the event log is empty, and cargo never observed the
child exiting. The defect delivered here is not the termination itself: it is
that a termination cannot be *attributed*. This issue makes the boundaries of a
jefe run observable so that the next occurrence is diagnosable from artifacts
that already exist on disk.

The delivered behavior is:

- an explicit run-start record and a typed run-end record in the log;
- a durable "run in progress" marker carrying the run's process identity, its
  last-seen heartbeat, and a breadcrumb of the in-flight operation;
- detection at the next start that a prior run ended with no recorded reason,
  reported in the log **and** surfaced in the UI;
- an explicit log flush on the exit paths jefe controls.

## Evidence from the issue

| Source | Result reported |
|---|---|
| `jefe.log` (28 MB) | No panic marker, no shutdown/exit record; log stops mid-stream. |
| Windows Application event log | No jefe entry, no WER report, no crash dump. |
| stderr / scrollback | Nothing; cargo emitted no `process didn't exit successfully` line. |
| Panic hook (`src/main.rs:256`) | Installed, never fired. |
| Health probe cadence | ~2.2 s lines stop in the same instant as the UI thread, then 6m10s of silence. |

Two facts constrain the design. First, the run died without executing any jefe
code, so the *only* mechanism that can attribute it is state written **before**
the death and interpreted **after** it — hence the marker. Second, nothing in the
current code writes a run boundary at all, so today even a clean exit is
indistinguishable from a kill.

## Current-state findings that shape the work

- `src/logging.rs` opens the log file with `OpenOptions::append` and hands the
  `File` to `tracing_subscriber::fmt().with_writer(..)`. Writes are unbuffered,
  but **no handle is retained and there is no flush entry point**, so "flush on
  exit" cannot currently be asserted by any test.
- After `init_diagnostics()` (`src/main.rs:325`) there is **no** `std::process::exit`
  call. Every controlled exit path returns through `run_tui` -> `main`, or unwinds
  through a panic. So the controlled exit surface is small and coverable.
- `jefe::runtime::process_liveness` / `capture_process_identity` already classify a
  recorded `ProcessIdentity` as `Alive` / `Dead` / `ReusedPid`, including PID reuse
  via `started_at`. Unclean-shutdown detection reuses this rather than inventing a
  liveness rule, which is what keeps a *concurrent* jefe from being misreported as
  a crashed one.
- `src/app_init.rs::append_warning` is the established route for putting a startup
  condition where the operator sees it (`state.warning_message`, rendered by the
  status bar).
- `src/harness/signal_cleanup.rs` (issue #375) is the structural precedent for
  platform termination handling: registration guard, detached handler thread,
  cleanup, then honoring the termination intent. Windows is a no-op there.

## Acceptance matrix

| ID | Actor / path | Input and target | Observable success | Failure behavior / side effects | Persistence and compatibility | Proof |
|---|---|---|---|---|---|---|
| A1 | `run_tui` after `init_diagnostics` | any start with `JEFE_LOG_FILE` set | one `run start` record in the log naming pid, `started_at` discriminator, jefe version, and wall-clock start time | log unavailable: start proceeds silently, no panic, no stderr noise | log only; no schema | integration test asserts the record in a child process's log |
| A2 | `run_tui` returning from `run_app` | clean quit, render-loop failure, or main-thread panic unwind | one `run end` record with a typed reason (`user-quit`, `render-failed`, `panic`, `unknown`) | marker removal failure is logged at warn and never aborts exit | log only | integration test asserts reason text per outcome |
| A3 | `run_diagnostics::begin_run` | resolved config dir | a marker file exists under `<config-dir>/runs/` containing the run's `ProcessIdentity`, version, start time, last-seen time | unwritable dir: begin_run returns no prior runs, logs at warn, start continues | new ephemeral file; absent-dir tolerated; unknown JSON fields ignored | integration test reads the marker back |
| A4 | next `begin_run` | a marker whose owner is `Dead` or `ReusedPid` | log record and UI warning naming the prior pid, its last-seen timestamp, and its breadcrumb; the consumed marker is deleted | unparseable marker is deleted and not reported | forward-compatible: unknown fields ignored | integration test + `init_app_state` test + TUI scenario |
| A5 | next `begin_run` | a marker whose owner is `Alive` | that marker is left untouched and **not** reported | indeterminate probe result: not reported, marker retained | concurrent instances share the dir safely (one marker per pid) | integration test seeding the test process's own identity |
| A6 | exit and panic paths | `JEFE_LOG_FILE` set | the last record written before `std::process::exit` and before a panic-hook return is present in the file | flush error ignored; never panics from the flush path | none | child-process test asserting the tail survives `process::exit` |
| A7 | attach/detach scheduler | an in-flight attach or detach | the marker's breadcrumb names the operation, and an unclean report repeats it | no breadcrumb recorded yet: report omits it rather than inventing one | breadcrumb is optional in the marker | integration test + `app_shell_attach` unit test |

## Non-goals

- Fixing the underlying cause of the termination (tracked separately).
- Crash-dump capture or any telemetry that leaves the machine.
- Log rotation, log size management, or changing the existing log format.
- A general-purpose event/audit subsystem. The marker records exactly the fields
  named above and nothing else.
- Any dependency, schema version bump, or quality-gate change.
- **A8 (Windows console control handler) is deferred pending an explicit user
  decision** — see "Blocked scope" below. Nothing in this plan installs a console
  control handler until that decision is recorded here.

## Blocked scope: A8, `SetConsoleCtrlHandler`

Issue item 4 asks for a console control handler so `CTRL_CLOSE_EVENT`,
`CTRL_LOGOFF_EVENT`, and `CTRL_SHUTDOWN_EVENT` are recorded. It cannot be
implemented under current project rules without a decision:

- `Cargo.toml` sets `unsafe_code = "forbid"` at package level, so jefe cannot
  register the FFI callback itself.
- The Windows dependencies already vendored expose no such API: `winsafe 0.0.27`
  contains no `SetConsoleCtrlHandler` (verified by grep), and `win32console 0.1.5`
  is code-page/output only. `windows-sys` appears only transitively.
- Therefore A8 requires **adding a dependency** (e.g. `ctrlc` with its
  `termination` feature, which covers CTRL_C/BREAK/CLOSE/LOGOFF/SHUTDOWN). A safe
  wrapper crate is the established policy here — the `win32console` entry in
  `Cargo.toml` states it exists so "all unsafe Win32 calls stay inside this crate
  so jefe source remains `unsafe`-free" — but a dependency change requires
  explicit approval under the delivery workflow.

Caveat to disclose with the decision: on Windows the handler routine runs on an
OS-injected thread with a hard time budget, and `ctrlc` signals an event and
returns immediately while the user closure runs elsewhere. The record is
therefore best-effort. It also would not have captured the deaths in this issue,
whose evidence shows *no* exit window was granted.

Options: (a) approve the dependency and implement A8; (b) defer A8 to a
follow-up issue. A4/A5 already attribute a window-less kill, which is the case
actually observed.

## Slices

### Slice 1 — pure run-record domain (delivered, `ad4f11c4`)

- Rows: A2 (reason type), A3 (marker shape), A4/A5 (classification rule), A7 (breadcrumb field).
- Allowed paths: `src/domain/run_record.rs`, `src/domain/mod.rs`, `tests/issue662_behavior.rs`.
- RED: classification tests for owner-alive / owner-gone / indeterminate, and for
  a marker with and without a breadcrumb.
- GREEN: `RunEndReason`, `RunMarker`, `PriorRunProbe`, `PriorRunDisposition`,
  `UncleanRun`, and a pure `classify_prior_run`. No I/O, no runtime dependency —
  the `ProcessLiveness` -> `PriorRunProbe` mapping happens at the caller.
- Stop condition: if the classification needs process probing inside `domain/`.

### Slice 2 — marker persistence (delivered, `5ac78df2`)

- Rows: A3, A4, A5.
- Allowed paths: `src/persistence/run_marker.rs`, `src/persistence/mod.rs`, `tests/issue662_behavior.rs`.
- RED: write/read round-trip; scan skips foreign files; unparseable marker is deleted; missing dir is tolerated.
- GREEN: `run_marker_dir`, `write_marker` (temp file + atomic replace), `read_markers`, `remove_marker`. One file per pid so concurrent instances never clobber each other.
- Stop condition: if this needs the revision/hash-gated `persistence::writer::write` contract.

### Slice 3 — log flush and run boundary records (delivered, `548b643a`)

- Rows: A1, A2, A6.
- Allowed paths: `src/logging.rs`, `src/run_diagnostics.rs`, `src/lib.rs`, `tests/issue662_behavior.rs`.
- RED: a child-process test asserting `run start` and `run end` records reach the
  log file and survive `std::process::exit`.
- GREEN: retain an `Arc<File>` in `logging` and add `logging::flush()`; add
  `run_diagnostics::begin_run` / `RunGuard::finish` / `heartbeat` / `record_breadcrumb`;
  flush from the panic hook and from run end.
- Stop condition: if flushing requires changing the subscriber's writer type or the log format.

### Slice 4 — wiring into the binary (delivered, `3d99a495`)

- Rows: A1, A2, A6, A7.
- Allowed paths: `src/main.rs`, `src/app_shell.rs`, `src/app_shell_attach.rs`, `src/panic_capture.rs`.
- RED: `app_shell_attach` unit test asserting a breadcrumb is recorded for an in-flight attach.
- GREEN: begin the run in `run_tui`; finish it with `user-quit` / `render-failed`;
  heartbeat from a dedicated `use_future`; breadcrumb at the attach/detach
  scheduler; flush from the panic hook.
- Stop condition: if the heartbeat needs a new worker/thread subsystem rather than an existing loop.

### Slice 5 — UI surfacing of an unclean prior run (delivered, `3d99a495`)

- Rows: A4, A5.
- Allowed paths: `src/main.rs` (context field), `src/app_init.rs`, `src/app_init_tests.rs`, `dev-docs/tmux-scenarios/v1/issue662-unclean-prior-run.json`, `tests/harness_v1_fixtures.rs`.
- RED: TUI scenario seeding a dead-owner marker and asserting the warning appears; `init_app_state` test asserting `warning_message` names the pid and last-seen time.
- GREEN: carry the detected unclean runs on `AppContext` and surface them through the existing `append_warning` route.
- Stop condition: if surfacing requires a new screen, message variant, or state field beyond `warning_message`.

### Slice 6 — A8, blocked

Not started. See "Blocked scope".

## Scope ledger

| Discovery | Disposition |
|---|---|
| `logging` retains no file handle, so no flush is possible | In scope (A6) — required by the accepted behavior. |
| `run_app` swallows render-loop errors, losing the exit reason | In scope (A2) — the typed reason needs it. |
| `ErrorSource::Startup` has display mappings but no producer | Defer — surfacing via `warning_message` satisfies A4; adding the first `Startup` error producer is adjacent scope. |
| `app_shell_liveness` early-returns when there are no local targets, so it is not a dependable heartbeat | In scope (A3) — heartbeat gets its own small `use_future` instead of changing liveness. |
| Windows console control handler needs a new dependency | Blocked — user decision required (A8). |

## Review counters

- Local OCR runs: 0 / 2
- Post-PR OCR runs: 0 / 2

## Verification evidence

Commits on `issue662`:

| Commit | Slices | Behavior landed |
|---|---|---|
| `ad4f11c4` | 1 | pure run-record domain types and `classify_prior_run` |
| `5ac78df2` | 2 | per-run marker persistence beside the durable state file |
| `548b643a` | 3 | `logging::flush()` and `run_diagnostics` begin/heartbeat/breadcrumb/finish |
| `3d99a495` | 4, 5 | binary wiring, typed end reason, breadcrumbs, UI surfacing, TUI scenario |

Acceptance rows to proof:

| Row | Proof | Result |
|---|---|---|
| A1 | `tests/issue662_behavior.rs::a_run_that_ends_for_a_reason_records_both_boundaries_and_retires_its_marker` (child process, real log file) | pass |
| A2 | same test plus `a_panicking_run_still_records_why_it_ended`; `run_app` now returns `RenderFailed` / `UserQuit` | pass |
| A3 | 8 persistence tests in `tests/issue662_behavior.rs` (round-trip, foreign files, unparseable, missing dir) | pass |
| A4 | `a_new_run_reports_and_clears_the_marker_of_a_prior_run_that_never_ended`; `src/app_init_tests.rs` x4; TUI scenario `issue662-unclean-prior-run.json` | pass locally; scenario runs on CI (harness is unix-only) |
| A5 | `a_run_killed_without_a_reason_leaves_its_marker_and_its_last_breadcrumb`; owner-alive classification tests | pass |
| A6 | child-process tests assert the tail survives process death; panic hook calls `logging::flush()` | pass |
| A7 | breadcrumb carried in the marker and repeated in the unclean report; attach/detach record breadcrumbs | pass |

Local gates on `3d99a495`:

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| `cargo test --all-features --locked --lib --bins` | 3734 + 870 + 12 passed, 0 failed |
| `cargo test --all-features --locked --tests` | all targets ok, 0 failed |
| `cargo xtask check architecture` | exit 0 |
| `cargo xtask check source-size` | exit 0 (warnings only, no file at or over the 1000-line limit) |

Note: `harness::tmux_driver::tests::real_psmux_runs_a_stable_native_process_when_available`
failed once under full-suite parallelism and passed in isolation both with and
without these changes; it is environment-dependent and untouched by this work.

## Deferred findings and follow-ups

- A8 console control handler, if deferred by the user decision.
- First producer for `ErrorSource::Startup`.
