# Issue #400 — Bound and trust local Unix process probe subprocesses

## Problem

`src/runtime/process.rs` invokes the external `kill` and macOS `ps` utilities
synchronously via `std::process::Command::new("kill")` / `Command::new("ps")`.
Those invocations inherit executable resolution from `PATH` (no explicit
resolution) and impose no subprocess deadline (a hung probe blocks startup and
the render-path liveness poll indefinitely).

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Success behavior | Failure behavior | Test evidence |
|---|---|---|---|---|---|
| A1 | Unix `probe_process` (startup + render-path liveness) | live PID, trusted `kill` on PATH | `Running`/`Exited`/`Inaccessible` classified as today | resolution failure → `ProbeFailed` (fail open) | `local_command` Kill resolution test; `process_tests` resolution-failure test |
| A2 | Unix `probe_process` | hung / manipulated `kill` executable | n/a | spawn or timeout deadline fires → child killed+reaped → `ProbeFailed` | `local_command` `run_bounded` timeout test |
| A3 | macOS `unix_process_start_time` | live PID, trusted `ps` on PATH | UTC epoch seconds parsed as today | resolution/spawn/timeout failure → `None` (identity-less, fail open) | macOS structural command test updated; existing timezone-stability test covers end-to-end |
| A4 | `JEFE_KILL_BIN` / `JEFE_PS_BIN` override | explicit trusted executable path | override used verbatim | invalid override → typed `LocalToolError` → `ProbeFailed` | `local_command` override tests (Kill/Ps) |
| A5 | Windows `probe_process` | any PID | **unchanged** — native `OpenProcess`/`GetProcessTimes` | **unchanged** | existing Windows tests; `#[cfg(windows)]` branch untouched |
| A6 | structural command contracts | resolved executable + pid | args and controlled env (`LC_ALL=C`, `TZ=UTC`) preserved | n/a | `process_tests` structural tests updated for new builder signatures |

## Non-goals

- Changing issue #305 process classification semantics (`classify_unix_probe`,
  `classify_process_observation`, `ProcessObservation`, `ProcessLiveness`
  unchanged).
- Adding unsafe system calls (`unsafe_code = "forbid"` preserved).
- Changing remote SSH probing (`src/ssh.rs` execution path unchanged — no
  refactor to share the bounded-run helper; noted as a potential follow-up).
- Changing Windows native probe behavior.
- Adding new dependencies.

## Vertical slices

### Slice 1 — Extend `local_command` with Kill/Ps resolution and a bounded runner

**Files:** `src/local_command.rs`

- Add `LocalTool::Kill` (`"kill"`, `JEFE_KILL_BIN`) and `LocalTool::Ps`
  (`"ps"`, `JEFE_PS_BIN`).
- Add `BoundedRunError` (spawn / timeout / io / pipe-unavailable) and
  `pub fn run_bounded(command, timeout) -> Result<Output, BoundedRunError>`.
  Pattern mirrors `ssh.rs::execute_command`: spawn → piped stdout/stderr read
  on threads → `try_wait` poll with `Instant` deadline (25 ms sleep) → kill +
  reap on timeout. No stdin, no cancellation (simpler than ssh).

**RED tests** (in `local_command.rs` test module):
- Kill/Ps resolve from PATH and from explicit override (structural, like
  existing Git/Gh/Ssh tests).
- Invalid override → `InvalidOverride`.
- `run_bounded` returns `Output` for a fast-exiting command.
- `run_bounded` returns `Timeout` and reaps the child for a hanging command.

### Slice 2 — Route Unix/macOS probes through the bounded boundary

**Files:** `src/runtime/process.rs`, `src/runtime/process_tests.rs`

- Add `const PROBE_TIMEOUT: Duration = Duration::from_secs(5);`
- Split `unix_probe_command(pid)` into
  `unix_probe_command_for(executable: &Path, pid: u32) -> Command` (pure
  builder, preserves args + `LC_ALL=C`) and have `probe_process` resolve +
  run-bounded.
- Split `macos_start_time_command(pid)` into
  `macos_start_time_command_for(executable: &Path, pid: u32) -> Command` and
  have `unix_process_start_time` resolve + run-bounded.
- Map resolution, spawn, timeout, and pipe failures → `ProcessObservation::ProbeFailed`
  (Unix probe) / `None` (macOS start time).

**RED/updated tests** (in `process_tests.rs`):
- Structural command tests updated to call `*_for(Path, pid)` and verify args
  + env are unchanged.
- New test: empty-PATH resolution failure for Kill → `ProbeFailed` via
  `probe_process`.

## Scope ledger

| Item | Status |
|---|---|
| `src/local_command.rs` — Kill/Ps + `run_bounded` + trusted-path policy | Slices 1, 3, 4 |
| `src/runtime/process.rs` — resolve + bound probes | Slice 2 |
| `src/runtime/process_tests.rs` — updated structural + new boundary tests | Slice 2 |

Changed files: 4 (incl. plan). Net lines: +538 / -20 (well under 25-file and
1,500-line budgets).

## Review counters

- OCR local (pre-PR): 2 / 2 used
  - Run #1: 3 findings — 1 Blocker-Fix (trusted-path, fixed in Slice 3),
    2 Defer (macOS `.ok()?` observability — pre-existing pattern; shared
    `PROBE_TIMEOUT` — non-critical).
  - Run #2: 4 findings — 2 In-scope-Fix (Unix-only trust gate, `Stdio::null`
    stdin — fixed in Slice 4), 1 Defer (macOS `.ok()?` observability — same
    as run #1), 1 Reject (process-group kill — speculative for leaf probe
    tools, out of scope).
- OCR post-PR: 0 / 2
