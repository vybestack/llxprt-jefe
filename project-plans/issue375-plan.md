# Issue #375: Make TUI harness signal cleanup private-socket aware

## Problem

The TUI scenario runners rely on RAII cleanup inside the harness for success,
assertion failure, timeout, and panic paths. An abrupt process signal (SIGINT,
SIGTERM, SIGHUP, SIGQUIT) can bypass Rust Drop and leave a private tmux
server/session alive.

A generic shell EXIT trap is insufficient because the harness uses a private
tmux socket (`-L jefe-harness-<pid>`) and intentionally retains artifact
directories for diagnostics.

## Desired Outcome

- Introduce shared, private-socket-aware signal cleanup for real-tmux harness
  runs.
- Preserve diagnostic artifacts (no artifact deletion).
- Keep ordinary cleanup ownership centralized in `signal_cleanup.rs` rather
  than duplicated across issue-specific runner scripts.
- Cover Unix signals supported by the harness (SIGINT, SIGTERM, SIGHUP,
  SIGQUIT) and document Windows behavior (no-op: psmux-based harness does not
  use a long-lived server process).
- Add behavioral tests proving cleanup targets only the harness-owned
  server/session and never touches unrelated tmux servers or sessions.

## Non-Goals

- Deleting captured artifacts.
- Killing user tmux servers or unrelated sessions.
- Adding bespoke cleanup logic independently to each issue runner script.
- Changing the Windows psmux harness lifecycle (psmux processes die with their
  parent; no server-to-kill exists).

## Architecture

### Current State

- `src/harness/tmux_driver.rs` resolves a per-process private socket name via
  `harness_socket_name()` (a `OnceLock<String>` with `jefe-harness-<pid>`).
- `TmuxSessionGuard` provides RAII cleanup on Drop — but Drop is bypassed by
  abrupt signals.
- The harness binary (`src/bin/jefe-tmux-harness.rs`) calls
  `run_tmux_scenario()` which constructs the guard, runs the scenario, and
  drops the guard.
- Runner scripts (issue230, issue265, issue351) each set their own EXIT traps.

### Proposed Design

New module: `src/harness/signal_cleanup.rs`

- `SignalCleanupGuard`: an RAII guard that, **on Unix**, registers SIGINT,
  SIGTERM, SIGHUP, and SIGQUIT handlers via the `signal-hook` crate before the
  tmux session starts. When any registered signal arrives, the handler kills
  **only** the harness-owned tmux server (`tmux -L <harness_socket> -f /dev/null
  kill-server`). On Drop, the guard unregisters the handlers.
- On Windows, `SignalCleanupGuard` is a zero-sized no-op (psmux processes die
  with the parent process; there is no persistent server to kill).
- The guard captures the harness socket name string so the signal handler can
  issue a targeted `kill-server` on exactly that socket.

Integration point: `run_tmux_scenario` in `runner.rs` constructs the guard
before starting the session. The guard's Drop runs after the session guard's
Drop (reverse construction order), ensuring normal-path RAII still works and
signal-path cleanup is also covered.

`TmuxDriver` gains a `kill_harness_server()` method that kills the server on
the harness socket — used by both the signal handler and available for
explicit teardown.

## Acceptance Matrix

| # | Actor/Path | Input | Success Behavior | Failure Behavior | Test |
|---|-----------|-------|-----------------|-----------------|------|
| A1 | Unix: signal handler fires on harness socket | SIGINT/SIGTERM/SIGHUP/SIGQUIT | Harness tmux server killed (no sessions remain on `jefe-harness-<pid>` socket) | — | unit (signal simulation) |
| A2 | Unix: signal cleanup only kills harness server | SIGTERM | Only `jefe-harness-<pid>` sessions killed; sessions on other sockets unaffected | — | behavioral (real tmux) |
| A3 | Unix: artifacts preserved after signal | SIGTERM | Artifact directory untouched (not deleted) | — | behavioral (real tmux) |
| A4 | Unix: normal-path (no signal) Drop still works | Normal scenario completion | TmuxSessionGuard kills session as before | — | existing tests (unchanged) |
| A5 | Windows: guard is no-op | Any signal | No tmux interaction (psmux lifecycle handles cleanup) | — | compile-check (cfg gate) |
| A6 | Unix: kill_harness_server is idempotent | kill-server on already-dead socket | Ok (no error) | — | unit |
| A7 | Unix: guard unregisters on Drop | Construct guard, then drop | Signal handlers unregistered; subsequent signals use default disposition | — | unit |

## Non-Goals (reiterated)

- No artifact deletion (signal cleanup must NEVER touch the filesystem).
- No killing of non-harness sessions/servers.
- No per-script duplication — centralized in `signal_cleanup.rs`.

## Vertical Slices

### Slice 1: Add `kill_harness_server` to TmuxDriver (RED → GREEN)
- **Acceptance rows**: A6
- **Files**: `src/harness/tmux_driver.rs`
- **Change**: Add `pub fn kill_harness_server(&self) -> Result<(), TmuxDriverError>`
  that runs `tmux -L <socket> kill-server`, treating "no server running" as
  success (idempotent).
- **Tests**: `src/harness/tmux_driver_tests.rs`

### Slice 2: Create `signal_cleanup.rs` module with tests (RED → GREEN)
- **Acceptance rows**: A1, A7
- **Files**: `src/harness/signal_cleanup.rs`, `src/harness/signal_cleanup_tests.rs`
- **Change**: Implement `SignalCleanupGuard` with Unix signal handler
  registration via `signal-hook`. On signal, call `TmuxDriver::kill_harness_server()`.
  On Windows, guard is a no-op.
- **Tests**: Unit tests for guard construction, drop, and signal-triggered
  cleanup targeting the harness socket only.

### Slice 3: Integrate guard into `run_tmux_scenario` (GREEN)
- **Acceptance rows**: A2, A3, A4
- **Files**: `src/harness/runner.rs`
- **Change**: Construct `SignalCleanupGuard` before starting the tmux session
  in `run_tmux_scenario`. Guard drops after `TmuxSessionGuard` (reverse order).
- **Tests**: Behavioral tests proving the guard only kills the harness socket.

## Scope Ledger

| Date | Item | Type |
|------|------|------|
| 2026-07-22 | Initial plan | — |
| 2026-07-22 | Added `#[must_use]` to `SignalCleanupGuard` (OCR finding) | In-scope-Fix |
| 2026-07-22 | Added signal-delivery integration test (OCR finding) | In-scope-Fix |
| 2026-07-22 | Added `catch_unwind` around `perform_cleanup` (OCR blocker) | Blocker-Fix |
| 2026-07-22 | Replaced fixed 200ms sleeps with `poll_until_dead` (OCR finding) | In-scope-Fix |
| 2026-07-22 | Added `SocketGuard` RAII for test cleanup on panic (OCR finding) | In-scope-Fix |
| 2026-07-22 | Changed `kill_harness_server` from `run_tmux_capture` to `run_tmux` (OCR finding) | In-scope-Fix |
| 2026-07-22 | Added PID + AtomicU64 counter to `unique_suffix` (OCR finding) | In-scope-Fix |
| 2026-07-22 | Documented detached thread intent and `handler_count` purpose (OCR finding) | In-scope-Fix |
| 2026-07-22 | Set `LC_ALL=C` on raw tmux test helpers (OCR finding) | In-scope-Fix |

## Review Counters
- Local OCR: 2/2 (completed)
- PR OCR: 2/2 (completed)

## Verification
- `make quick-check` during iteration
- `make ci-check` before push
- Windows compilation verified via `cargo check` (no Windows CI runner
  available locally, but cfg gates ensure the module compiles to a no-op)
