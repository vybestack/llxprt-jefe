# Issue #403: Duplicate agent names, multiline version strings, silent launch failures

## Problem

Three related bugs make agents with duplicate names and/or malformed version
strings silently unlaunchable:

1. **No duplicate agent name validation** — agents in the same repository can
   share a name, producing a work-dir collision.
2. **Multiline/whitespace-polluted version strings are silently accepted** —
   `LlxprtNpmPackageSelector::normalize` only `str::trim()`s, preserving
   embedded newlines/control chars. The bogus version causes silent npm
   resolution failures.
3. **Launch failures from bad versions do not surface** — `mark_launch_failed`
   writes `error_message` directly to `AppState`, bypassing the reducer's
   `capture_runtime_errors`. The error never enters the durable Errors ring
   buffer, so the status bar (which reads `last_error_title` from the ring
   buffer) shows nothing. The agent just goes "Dead".
4. **Delete can destroy a shared work directory** — `delete_selected_agent`
   `remove_dir_all`s the work dir even when a sibling agent in the same repo
   still references it.

## Root Causes (verified against source)

### Bug 2 — `normalize()` only trims surrounding whitespace
`src/domain/llxprt_version.rs` `LlxprtNpmPackageSelector::normalize`:
`raw.trim()` strips leading/trailing whitespace but preserves embedded
`\n`, `\r`, `\t`, and control characters. Same gap in
`code_puppy_uvx_from_spec` / `code_puppy_requires_uvx` (they also only
`.trim()`).

### Bug 1 — No per-repository name/work-dir uniqueness
`src/state/form_ops.rs` `submit_new_agent` / `submit_edit_agent` never scan
`self.agents` for a same-repo name collision or a work-dir collision before
pushing/updating. The `enforce_shortcut_uniqueness` pattern exists for
shortcuts but is not applied to names/work-dirs.

### Bug 3 — Launch failures bypass the error-capture reducer
`src/app_input/mod.rs` `mark_launch_failed` writes
`state.error_message = Some(...)` directly via `app_state.write()`, then calls
`persist_state`. It does NOT go through `apply_message` → `finalize_message` →
`capture_runtime_errors`. The dashboard status bar renders
`last_error_title()` (from the Errors ring buffer), not `error_message`
directly. Since the launch error is never captured into the ring buffer, it
is invisible on the dashboard. The agent form screen also does not render
`error_message` (only `new_repository.rs` does).

### Bug 4 — Delete unconditionally removes shared work dirs
`src/state/state_ops.rs` `delete_selected_agent` checks
`delete_work_dir && !repository_remote_enabled && removed_agent.work_dir.exists()`
but never checks whether a sibling agent still references the same `work_dir`.

## Acceptance Matrix

| # | Actor/Path | Input | Success Behavior | Failure Behavior | Test |
|---|-----------|-------|-----------------|-----------------|------|
| A1 | `LlxprtNpmPackageSelector::normalize` | version with embedded `\n`, `\r`, `\t`, spaces, control chars | All whitespace stripped; single-token selector returned | — | unit (`llxprt_version.rs` inline tests) |
| A2 | `LlxprtNpmPackageSelector::normalize` | whitespace-only or empty after stripping | `None` returned | — | unit |
| A3 | `code_puppy_uvx_from_spec` / `code_puppy_requires_uvx` | version with embedded whitespace | Whitespace stripped before building uvx spec | — | unit |
| A4 | `normalize` | shell metacharacters (non-whitespace) | Preserved as data (no semver validation) | — | unit (updated existing test) |
| B1 | `submit_new_agent` | name collides (case-insensitive, trimmed) with existing same-repo agent | Agent NOT created; `error_message` set; modal stays open | — | state unit test |
| B2 | `submit_new_agent` | work-dir collides with existing same-repo agent (via `local_paths_equivalent`) | Agent NOT created; `error_message` set; modal stays open | — | state unit test |
| B3 | `submit_new_agent` | distinct name + distinct work-dir | Agent created; `error_message` cleared; modal closes | — | state unit test |
| B4 | `submit_edit_agent` | rename to a name that collides with another agent | Agent NOT updated; `error_message` set; modal stays open | — | state unit test |
| B5 | `submit_edit_agent` | same agent's own name (no collision) | Agent updated normally | — | state unit test |
| B6 | `submit_new_agent` / `submit_edit_agent` | `llxprt_version` or `code_puppy_version` field contains internal whitespace after trim | `error_message` set; modal stays open; no agent created/updated | — | state unit test |
| B7 | New Agent form render | `error_message` is Some | "  Error: {msg}" line rendered before hint line | — | (mirrors existing `new_repository.rs` pattern) |
| C1 | `mark_launch_failed` | any `RuntimeError` | `error_message` captured into the Errors ring buffer; `last_error_title()` returns the error | — | app_input unit test |
| C2 | Status bar | launch error captured | `ERR: {truncated}` shown in center area | — | existing status-bar rendering (no change) |
| D1 | `delete_selected_agent` | two agents share `work_dir`; delete one with `delete_work_dir=true` | Agent removed from state; shared directory NOT removed | — | state unit test |
| D2 | `delete_selected_agent` | sole-owner agent; `delete_work_dir=true` | Agent removed; directory removed (existing behavior) | — | state unit test |

## Non-Goals

- Full semver validation of version strings (npm handles resolution; we only
  sanitize whitespace).
- Preventing same-named agents across *different* repositories (only same-repo
  collisions matter).
- Changing the agent ID generation scheme.
- Adding a new npm availability probe on field blur (Phase 4 of the issue's
  proposed plan — deferred; out of scope for this PR).
- Adding a new diagnostic panel UI (the existing Errors screen ring buffer
  already stores full detail).
- Changing the error-dismissal UX (no new keybinding for explicit dismiss);
  errors remain dismissable via the existing Errors-mode clear and the
  `ClearError` system message.

## Vertical Slices

### Slice 1: Version field sanitization (Bug 2) — RED → GREEN
- **Acceptance rows**: A1, A2, A3, A4
- **Files**: `src/domain/llxprt_version.rs`
- **Change**: Update `normalize()` to strip ALL whitespace (not just
  surrounding). Update `code_puppy_uvx_from_spec` and
  `code_puppy_requires_uvx` to strip internal whitespace before building the
  spec. Document the "no whitespace" invariant. Update/extend inline tests.
- **RED test**: add failing tests for embedded `\n`/`\r`/`\t`/space/control
  chars in `normalize` and the Code Puppy helpers BEFORE changing the impl.
- **GREEN**: implement the whitespace stripping.
- **Verification**: `cargo test -p jefe --lib domain::llxprt_version`

### Slice 2: Duplicate agent name + work-dir collision prevention (Bug 1) — RED → GREEN
- **Acceptance rows**: B1, B2, B3, B4, B5, B6
- **Files**: `src/state/form_ops.rs` (validation in submit_new_agent /
  submit_edit_agent), `src/ui/screens/new_agent.rs` (render error line),
  new test file `src/state/form_ops_issue403_tests.rs`
- **Change**: Add pre-submit checks in `submit_new_agent` and
  `submit_edit_agent`:
  1. Version-field whitespace check (B6) — if `llxprt_version` or
     `code_puppy_version` contains internal whitespace after trim, set
     `error_message` and return early.
  2. Name uniqueness (B1/B4) — scan same-repo agents for a trimmed
     case-insensitive name match; set `error_message` and return early.
  3. Work-dir collision (B2) — scan same-repo agents for a
     `local_paths_equivalent` work-dir match; set `error_message` and return
     early.
  On success, clear `error_message` (matching the repository-form pattern).
  Add the "  Error: {msg}" render line to `new_agent.rs` before the hint.
- **RED test**: failing tests asserting error_message is set and agent count
  is unchanged for collisions.
- **GREEN**: implement the checks.
- **Verification**: `cargo test -p jefe --lib state::form_ops`

### Slice 3: Launch error capture into ring buffer (Bug 3) — RED → GREEN
- **Acceptance rows**: C1, C2
- **Files**: `src/app_input/mod.rs` (mark_launch_failed),
  `src/state/errors_ops.rs` (expose capture for direct-write paths)
- **Change**: After `mark_launch_failed` sets `error_message`, call the
  error-capture logic so the error enters the durable ring buffer and
  `last_error_title()` returns it. Expose `capture_runtime_errors` as a
  `pub(crate)` method on `AppState` (or add a thin `capture_errors()` wrapper)
  so the app_input layer can invoke it after a direct state write.
- **RED test**: failing test asserting `last_error_title()` returns the error
  after `mark_launch_failed` (requires a test harness for the app_input
  function, or asserting the captured state directly).
- **GREEN**: wire the capture call.
- **Verification**: `cargo test -p jefe --lib app_input`

### Slice 4: Delete work-dir sharing guard (Bug 4) — RED → GREEN
- **Acceptance rows**: D1, D2
- **Files**: `src/state/state_ops.rs` (delete_selected_agent + inline tests)
- **Change**: Before `remove_dir_all`, scan remaining `state.agents` for any
  other agent whose `work_dir` is equivalent (via `local_paths_equivalent`).
  If a sibling shares it, skip removal and log a warning.
- **RED test**: failing test that two agents share a dir, delete one with
  `delete_work_dir=true`, assert dir still exists.
- **GREEN**: implement the guard.
- **Verification**: `cargo test -p jefe --lib state::state_ops`

## Architecture & Integration Boundaries

- **Domain layer** (`src/domain/llxprt_version.rs`): pure normalization.
  No new dependencies. The "no whitespace" invariant is documented on the
  type.
- **State layer** (`src/state/form_ops.rs`, `src/state/state_ops.rs`,
  `src/state/errors_ops.rs`): form validation and delete guards are
  state-internal; no new public abstractions. `capture_runtime_errors` is
  exposed `pub(crate)` for the app_input direct-write path only.
- **App-input layer** (`src/app_input/mod.rs`): `mark_launch_failed` gains a
  capture call after setting `error_message`. No new subsystem.
- **UI layer** (`src/ui/screens/new_agent.rs`): adds the error render line,
  mirroring `new_repository.rs` exactly. No new component.

## Scope Ledger

| Date | Item | Type |
|------|------|------|
| 2026-07-25 | Initial plan (4 slices, ~6 files) | — |

## Review Counters
- Local OCR: 0/2
- PR OCR: 0/2

## Verification
- `make quick-check` during iteration
- `make ci-check` before push (fmt, clippy -D warnings, coverage ≥30, build, test)
