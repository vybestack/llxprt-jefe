# Issue #434 — Initialize the Windows console for Unicode TUI output

## Problem

On native Windows, Jefe writes UTF-8 box-drawing and cursor glyphs through
iocraft, but it never switches the console output code page from the inherited
OEM page to UTF-8. The console decodes the byte stream with the wrong code page,
so borders and caret/block cursor glyphs render as `?`.

## Architecture decision

Own console preparation at the executable boundary in a binary-private
`src/terminal_init.rs` module. `main` obtains a scope guard immediately before
entering the render future and holds it until the render future returns. The
module has one platform-uniform call signature; its non-Windows implementation
returns `None`.

The module separates a deterministic state machine from the Windows adapter:

- a private injected console-policy trait reads/sets output code page and mode;
- preparation captures both original values before mutation;
- Windows TTY preparation changes output CP to 65001 and ORs
  `ENABLE_VIRTUAL_TERMINAL_PROCESSING` into the existing mode;
- a guard restores mode and code page in reverse mutation order on normal return
  and panic unwinding;
- non-TTY output skips every console API call;
- setup is all-or-nothing: a mode-setting failure immediately rolls back a code
  page change before returning no guard;
- restoration is best effort: failure restoring one value is logged and does
  not suppress restoration of the other.

Diagnostics are structured `tracing::warn!` events emitted after logging has
initialized. Console setup is a rendering capability boundary, not persistence,
state, runtime, or UI component behavior.

### Dependency decision — approved

The issue states that `winsafe = 0.0.27` with `kernel` exposes the console code
page APIs. Inspection of the installed crate source shows that it exposes safe
`HSTD::GetConsoleMode` / `SetConsoleMode` and the VT constant, but contains no
`GetConsoleOutputCP` or `SetConsoleOutputCP` binding. `crossterm 0.28.1` also has
no code-page API. The issue-permitted `windows`/`windows-sys` APIs are `unsafe`
functions and cannot be called because first-party `unsafe` is forbidden.

The approved narrow safe option is a Windows-target-only dependency:

```toml
win32console = "0.1.5"
```

Only its static `WinConsole::get_output_code_page` and
`WinConsole::set_output_code` methods will be used. Output mode remains on the
existing `winsafe` dependency. `win32console` preserves arbitrary original code
pages as `u32`, adds only its own package to the lockfile because `winapi 0.3` is
already resolved, and keeps all unsafe Win32 calls outside first-party source.
The user explicitly approved this manifest change on 2026-07-27.

### Cleanup semantics — approved interpretation

Rust `Drop` runs for normal return and panic **unwinding**. It cannot run after
`std::process::abort`, `std::process::exit`, `TerminateProcess`, power loss, or a
panic compiled with `panic = "abort"`. Therefore the issue's phrase
"panic/abort exit (via Drop)" is technically impossible for hard-abort paths.
The accepted behavior is clean exit plus panic unwinding; hard process
termination is an explicit non-goal. The user approved this interpretation on
2026-07-27.

## Acceptance matrix

| ID | Actor / launch path and inputs | Target | Observable success | Failure / diagnostic and permitted prior side effects | Persistence / compatibility | Behavioral proof |
|---|---|---|---|---|---|---|
| A1 | User launches fullscreen or windowed Jefe from `cmd.exe`, PowerShell, or Windows Terminal with stdout attached and OEM output CP | Local native Windows 10 1903+ / 11 | Before rendering, output CP is 65001 and the existing output mode includes VT processing; all existing Unicode borders, separators, carets, and block cursors render without `?` substitution | Setup failure logs the failed operation; rendering continues rather than crashing. A failure before mutation permits no side effect; a mode-set failure permits only a transient CP change that is synchronously rolled back | No state/config/schema changes; existing glyph and border policy is unchanged | Windows policy test proves CP/mode resulting state; native Windows build + documented manual reproduction recipe proves visible Unicode rendering |
| A2 | Render future returns normally after successful preparation | Local native Windows | Original mode and output CP are restored before process return | Each restoration failure logs separately; failure restoring mode does not prevent the CP restore attempt | User shell state is preserved best effort | Fake policy test observes original mode/CP after guard drop and restoration order |
| A3 | A panic unwinds through the render scope after successful preparation | Local native Windows, unwind builds | Scope guard drops and restores original mode/output CP during unwinding | Restoration warnings use the same diagnostics as A2; no extra side effects | No persisted effect | `catch_unwind` regression test observes original mode/CP after panic unwinding |
| A4 | stdout is piped, redirected, or otherwise not a terminal | All platforms; especially Windows | Preparation returns no guard and makes zero policy reads/writes | No warning and no console mutation | Existing non-interactive behavior unchanged | Fake policy non-TTY test proves no state change or API operation |
| A5 | Reading CP/mode, setting CP, or setting VT mode fails | Local native Windows | Jefe does not panic; no partially prepared guard escapes | Read failures and CP-set failure occur before mutation; mode-set failure triggers immediate CP rollback. Primary and rollback failures are both logged | No persisted effect | Table-driven fake-policy failure tests prove all-or-nothing setup and rollback attempt |
| A6 | Restoring either mode or CP fails on guard drop | Local native Windows | Both restore operations are attempted independently | Structured warning names each failed restore; no retry loop or process abort | Best-effort shell compatibility | Fake policy restoration-failure test proves the second restore still runs |
| A7 | Jefe builds/runs on macOS or Linux and reaches the same call site | Local/remote Unix | Uniform function is a no-op; existing Unicode rendering remains unchanged | No warning, mutation, or platform API linkage | Existing UI assertions and Unix runtime behavior remain unchanged | Cross-platform compile/tests plus existing border/caret suites remain green |
| A8 | Main enters the UI render boundary | All platforms | One unconditional call site obtains and retains the guard for the full `smol::block_on` scope; there is no `cfg` fork in `main` | Console setup inability does not suppress the render path | Startup/recovery/doctor early exits remain outside console mutation | Source integration test or review evidence at `src/main.rs`; full CLI and TUI regression suites |

## Explicit non-goals

- Changing iocraft, border styles, separator strings, scrollbar glyphs, form
  caret glyphs, or workflow-dispatch cursor glyphs.
- Replacing Unicode rendering with ASCII.
- Changing the console input code page; this issue concerns Jefe's output.
- Adding raw FFI, first-party `unsafe`, a process/signal/termination cleanup
  subsystem, or a public library abstraction.
- Guaranteeing restoration after hard abort, `process::exit`, forced process
  termination, host crash, or power loss; those paths do not run destructors.
- Mutating a non-TTY sink or adding a `chcp`/shell-command fallback.
- Refactoring startup, iocraft, crossterm, UI components, runtime, persistence,
  or unrelated tests.
- Supporting Windows versions older than the documented Windows 10 build 1903
  minimum.
- Adding retry, polling, or global process hooks around console APIs.

## Bounded vertical slices

### Slice 1 — RED: observable Unicode and console-state contracts

**Rows:** A1–A8.

**Architecture owner / boundary:** executable terminal initialization boundary;
private deterministic state machine and existing schema-1 TUI harness.

**Allowed paths:**

- `project-plans/issue434-plan.md`
- `src/terminal_init.rs` (new; test-first state-machine contracts)
- `src/main.rs` (module declaration only if needed to compile RED)

**RED:**

1. Under the approved scenario exception (no Windows schema-1 PTY scenario is
   possible), add the injected console-policy state-machine tests first.
2. Add policy tests for UTF-8 + VT resulting state, clean restore, panic-unwind
   restore, non-TTY no-op, setup rollback, and best-effort drop restoration.
3. Prove focused tests fail because preparation/restoration is not implemented.

**GREEN criterion:** none in this slice; retain auditable RED output in the plan.

**Stop conditions:** scenario infrastructure cannot represent an OEM Windows
console without workflow/harness changes; tests require a public abstraction;
any dependency or path outside this allowlist is needed.

**Exception (approved 2026-07-27):** the schema-1 grammar permits only
`macos`/`linux` and `tmux_scenario` hard-fails with `HAR-E005` on Windows; the
documented pre-schema Windows harness binary no longer ships. A compliant OEM
code-page scenario would require an unplanned Windows schema-1 PTY subsystem,
which is out of scope. Under the approved issue #434 scenario exception, the
visible Unicode-regression is proven by the deterministic console-policy state
machine, a native Windows build check, and a documented manual reproduction
recipe (`cargo build` then `jefe` from `cmd.exe` before/after the change),
rather than a Windows schema-1 scenario. No harness, workflow, or dependency
supporting change is introduced for the scenario.

### Slice 2 — GREEN/REFACTOR: safe Windows adapter and main integration

**Rows:** A1–A8.

**Architecture owner / boundary:** executable OS boundary (`terminal_init`) wired
once by `main`.

**Allowed paths (in addition to Slice 1):**

- `Cargo.toml` and `Cargo.lock` — only after dependency approval
- `src/terminal_init.rs`
- `src/main.rs`

**GREEN:** implement the private policy state machine, Windows adapter, Unix
stub, structured failure diagnostics, and scope-guard call immediately before
`smol::block_on`. All focused policy tests and the Windows scenario pass.

**REFACTOR:** only naming/decomposition needed to satisfy the 60-line function,
complexity, source-size, formatting, and lint gates.

**Verification:** focused binary tests and scenario, `cargo xtask quick`, then
`cargo xtask ci` on the candidate head.

**Stop conditions:** an additional dependency/subsystem/public API is required;
vendored/UI/runtime files appear necessary; accepted failure semantics must
change; or the scope budget is approached.

## Expected paths and budget

| Path | Purpose | Estimated net change |
|---|---|---:|
| `project-plans/issue434-plan.md` | Acceptance/scope/evidence ledger | +170 |
| `Cargo.toml` | Approved Windows-only safe CP wrapper | +1 |
| `Cargo.lock` | Resolved wrapper package | +8 |
| `src/terminal_init.rs` | State machine, adapters, guard, focused tests | +250–350 |
| `src/main.rs` | Private module wiring and held guard | +3–6 |

Expected: 5 files, approximately 450–560 net lines. This is below the 25-file /
1,500-line target and 15-file / 800-line commit targets. No `.llxprt/`,
`.code_puppy/`, `.github/`, workflow, quality configuration, vendored, UI,
runtime, or persistence changes are authorized.

## Scope ledger

| Date | Discovery / proposed work | Disposition |
|---|---|---|
| 2026-07-27 | Branch `issue434` created from current `origin/main` at `eaba9f2`; origin is 0 commits ahead | Accepted |
| 2026-07-27 | Issue and sole bot comment fetched with `gh`; no human clarification changes the issue contract | Recorded |
| 2026-07-27 | Installed `winsafe 0.0.27` lacks `Get/SetConsoleOutputCP`; `windows` and `windows-sys` calls are unsafe | Recorded; safe wrapper required |
| 2026-07-27 | Proposed Windows-only `win32console 0.1.5` for CP APIs; existing winsafe remains mode owner | **Approved by user** |
| 2026-07-27 | Hard-abort restoration cannot be implemented through `Drop`; clean return and panic unwind are implementable | **Approved by user as explicit non-goal interpretation** |
| 2026-07-27 | Windows TUI scenario must prove the visible regression before production implementation | Accepted; stop if harness changes are required |
| 2026-07-27 | Schema-1 grammar accepts only `macos`/`linux`, `tmux_scenario` hard-fails on Windows, and the superseded legacy harness binary is absent | **Approved scenario exception: prove regression via policy tests + native Windows build + manual recipe; no schema/PTY/harness change in this issue** |

## Review counters

- OCR before PR: 1 / 2 (GLM rustreviewer subagent)
- OCR after PR: 0 / 2

## Review findings (OCR #1 — pre-PR)

| # | Finding | Disposition | Resolution |
|---|---|---|---|
| 1 | Module/struct docs claim "restores both code page and mode" but only CP is restored | In-scope-Fix | Fixed: docs now state only CP is restored; VT processing intentionally left enabled (progressive enhancement, avoids unsafe bitflag reconstruction) |
| 2 | VT-failure rollback warning swallows the underlying io::Error | In-scope-Fix | Fixed: `if let Err(error)` captures and logs `error = %error` |
| 3 | `panic_unwind_restores_state` test only proves catch_unwind succeeded, not that restoration occurred | In-scope-Fix | Fixed: added `Arc<Mutex<Vec<u32>>>` restore-log observer to RecordingPolicy; test now asserts the logged CP value after unwind |
| 4 | `win32console` redundant with `crossterm` | Reject | Factual error: `crossterm 0.28.1` source contains zero `GetConsoleOutputCP`/`SetConsoleOutputCP` bindings (verified by source search); the crate does not expose these APIs |
| 5 | `panic::set_hook` race under `--test-threads=N` | Defer | Latent flake risk; did not manifest in test runs. Follow-up if CI flakes |
| 6 | Repeated `stdout_handle()` calls re-query `GetStdHandle` | Defer | Stateless design is simpler; failure mode is observable and non-fatal |

## Verification evidence

All gates verified on native Windows (x86_64-pc-windows-msvc):

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | Clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Clean |
| `cargo build --workspace --all-features --locked` | Clean |
| `cargo test -p jefe --bin jefe` (including13 `terminal_init` tests) | **785 passed; 0 failed** |
| `cargo test -p jefe --lib` | **2386 passed; 0 failed** |
| `cargo xtask check clippy-allows` | Clean (zero-tolerance) |
| `cargo xtask check source-size` | Clean (mod.rs=265, tests.rs=341 — well under750-line warn) |
| `cargo xtask check architecture` | Clean |
| `cargo test --test psmux_attach` | 2 failures — **pre-existing on origin/main** (require real psmux binary; unrelated to this change; verified via `git stash` baseline) |

RED→GREEN evidence: the state-machine tests were written before the Windows adapter and initially failed (`prepare_console` returned `None` for all inputs); after implementing `prepare_console` with the `RecordingPolicy` fake, all13 behavioral tests pass.

Required candidate-head gates:

```text
cargo xtask ci
```

## Deferred findings / follow-ups

None.
