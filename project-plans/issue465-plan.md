# Issue #465: psmux smoke tests fail non-deterministically on Native Windows CI — bare PageUp root binding swallows Page keys

## Problem

The Native Windows (MSVC + psmux) CI job fails non-deterministically. A
different psmux integration test fails each run, the same test passes on the
next rerun, and the failures occur on PRs that do not touch any
psmux-related source or test files. The dominant failure (~60%) is
`psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys`, whose
root cause is a **psmux 3.3.7 compatibility defect**: psmux 3.3.7 ships a
default root-table binding `PageUp → copy-mode -u` that consumes bare PageUp
events before they reach the pane child. The test re-injects the same
Page-key sequence every ~110ms via `write_input_until_captured`, but once
copy mode is active every retry is consumed by copy mode — retries cannot
recover.

Secondary failure classes (already resolved by the 3.3.7 upgrade or by
prior fixes) are documented in the issue but are out of scope for this PR
except where the CI duplicate invocation amplifies churn.

## Root cause (dominant failure)

- psmux 3.3.7 ships a default root-table binding: `PageUp` → `copy-mode -u`.
- `configure_prefix_for_passthrough` in `src/runtime/commands.rs` only
  manages `prefix` and `prefix2`; it does **not** unbind `PageUp` from the
  root table or disable `scroll-enter-copy-mode`.
- ConPTY/crossterm decodes `CSI 5~` as a semantic `PageUp` event; psmux's
  root binding fires `copy-mode -u` **before** the key is forwarded to the
  pane. Once copy mode is active, `PageDown` is also consumed.
- The test's retry loop re-sends the same sequence, but retries are
  consumed by copy mode and cannot recover.
- Non-determinism comes from ConPTY event batching and runner scheduling:
  if `PageDown` is dispatched before the copy-mode transition propagates,
  it follows the normal passthrough path and the test passes.

## Acceptance matrix

| ID  | Actor / launch path                          | Boundary / input                                          | Target            | Observable success                                                                                 | Observable failure / diagnostic                                                                              | Side effects before failure                       | Persistence / compatibility                       | Behavioral test / scenario                                                                         |
| --- | -------------------------------------------- | --------------------------------------------------------- | ----------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| AC1 | `configure_prefix_for_passthrough` (Windows) | psmux 3.3.7 root table                                    | Native Windows    | Jefe-owned psmux sessions unbind `PageUp` from the root table (or disable `scroll-enter-copy-mode`) | `list-keys -T root` no longer binds `PageUp`; `show-options -gv scroll-enter-copy-mode` is `off`             | None (option apply is idempotent)                 | Backward-compatible; Unix path unchanged                                          | Unit test asserting the Windows branch emits the unbind/disable command                            |
| AC2 | Mouse smoke test                              | `\x1b[5~\x1b[6~` written in a single `write_input` call   | Native Windows CI | `PSMUX_BYTE_7E` (and the rest of the CSI 5~/6~ byte markers) appear in the fixture capture          | Timeout diagnostic includes `pane_pid`, `pane_dead`, `scroll-enter-copy-mode`, and root key table state     | None (single write, no re-injection)             | Test contract unchanged (AttachedViewer path is the behavior being verified) | `psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys` (rewritten)                   |
| AC3 | CI workflow                                   | `windows_native` job                                      | `.github/workflows/ci.yml` | psmux smoke suite runs exactly once per job (no duplicate `cargo test --features psmux-smoke --test psmux_smoke`) | Duplicate step removed; `cargo test --workspace --all-features --locked` remains the single execution        | None                                              | CI timing/resource change only                     | `ci.yml` has exactly one psmux smoke execution step                                               |

## Non-Goals

- **Do not** simply increase the 30-second timeout — a PageUp consumed by a
  root binding will still be consumed after 5 minutes.
- **Do not** add retry loops that re-inject semantic key sequences — this
  mutates psmux state and cannot recover once copy mode is active.
- **Do not** restructure the tests to avoid AttachedViewer — the attached
  input path is the behavior being verified (issue #296).
- **Do not** pin to a newer psmux version until one is released with the
  upstream fix; configure around the defect instead.
- **Do not** normalize the three divergent test harnesses (P1) in this PR —
  that is a follow-up. This PR only fixes the dominant failure and removes
  the CI duplicate.
- **Do not** add explicit readiness-stage diagnostics (P3), loader retry
  consistency (P4), bounded cleanup waits (P5), or capture diagnostics (P6)
  in this PR — they are follow-ups. This PR is scoped to the P0 production
  fix, the P0 test fix, and the P2 CI duplicate removal.
- **Do not** change the Unix `configure_prefix_for_passthrough` path.

## Vertical slices

### Slice 1 — P0 production: unbind PageUp from psmux root table

- **Acceptance rows:** AC1
- **Architecture owner:** `src/runtime/commands.rs` (Windows branch of
  `configure_prefix_with`); `src/runtime/commands_finalize.rs` calls it
  unchanged.
- **Allowed files:**
  - `src/runtime/commands.rs` (Windows branch only; add the unbind/disable
    command after the prefix options)
  - `src/runtime/commands_tests.rs` (new unit test for the Windows branch)
- **RED:** Unit test asserting the Windows `configure_prefix_with`
  closure emits `unbind-key -T root PageUp` (or
  `set-option -g scroll-enter-copy-mode off`). The test must fail before the
  production change.
- **GREEN:** Add the command to the Windows branch of
  `configure_prefix_with`.
- **Non-goals:** No Unix change; no remote change; no new module.
- **Verification:** `cargo test -p jefe --lib runtime::commands_tests`
  (Unix-host cross-compile check via `cargo check --target
  x86_64-pc-windows-msvc` is not available on this host; the Windows branch
  is unit-tested by asserting the command sequence the closure builds, not
  by executing psmux).
- **Stop conditions:** If the unbind command is rejected by psmux 3.3.7 in
  a way that requires a different option name, stop and record the
  alternative.

### Slice 2 — P0 test: rewrite the mouse Page-key assertion

- **Acceptance rows:** AC2
- **Architecture owner:** `tests/psmux_smoke_mouse.rs`
- **Allowed files:**
  - `tests/psmux_smoke_mouse.rs`
- **RED:** The existing test already fails non-deterministically; the
  rewrite is the GREEN. A TDD RED is not separately provable on this host
  (no psmux); the behavioral contract is locked by the new assertion
  structure.
- **GREEN:** Replace `write_input_until_captured` for Page keys with a
  single `write_input` call followed by a bounded poll of `capture-pane`
  (no re-injection). Assert each Page-key byte marker independently.
  Assert psmux did not enter copy mode after bare PageUp
  (`display-message -p -t <session> "#{pane_in_mode}"` == 0).
- **Non-goals:** No production change here; no harness normalization; no
  new shared module.
- **Verification:** `cargo test --features psmux-smoke --test
  psmux_smoke_mouse` (skipped on non-Windows / no psmux).
- **Stop conditions:** If the single-write approach proves to lose bytes
  on loaded runners in a way that requires a bounded retry, stop and
  record the evidence.

### Slice 3 — P2 CI: remove duplicate psmux smoke invocation

- **Acceptance rows:** AC3
- **Architecture owner:** `.github/workflows/ci.yml`
- **Allowed files:**
  - `.github/workflows/ci.yml`
- **RED:** `grep -n "psmux_smoke" .github/workflows/ci.yml` shows two
  invocations before the change.
- **GREEN:** Remove the explicit
  `cargo test --features psmux-smoke --test psmux_smoke` step; the
  `cargo test --workspace --all-features --locked` step already includes
  it.
- **Non-goals:** No CI restructuring beyond removing the duplicate; no
  change to the TUI scenario steps.
- **Verification:** `grep` confirms exactly one psmux smoke execution
  path remains.
- **Stop conditions:** None expected.

## Scope ledger

| File                                | Slice | Reason                                   | Approved |
| ----------------------------------- | ----- | ---------------------------------------- | -------- |
| `src/runtime/commands.rs`           | 1     | AC1: Windows unbind                      | yes      |
| `src/runtime/commands_tests.rs`      | 1     | AC1: unit test                           | yes      |
| `tests/psmux_smoke_mouse.rs`         | 2     | AC2: single-write Page-key assertion     | yes      |
| `.github/workflows/ci.yml`          | 3     | AC3: remove duplicate smoke invocation   | yes      |
| `project-plans/issue465-plan.md`     | —     | This plan                                 | yes      |

## Review counters

- OCR before PR: 0/2
- OCR after PR: 0/2
- CodeRabbit cycles: 0
- DeepThinker cycles: 0

## Verification evidence

(to be filled in after implementation)

## Deferred findings / follow-up issues

- P1: Normalize the three divergent test harness wrappers into a shared
  private support module.
- P3: Add explicit readiness stages and fail-fast diagnostics.
- P4: Make loader retry consistent across all psmux invocations.
- P5: Strengthen cleanup with bounded waits.
- P6: Improve capture diagnostics (debug-escaped capture, row count).