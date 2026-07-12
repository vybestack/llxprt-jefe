# Plan: In-app device-code auth dialog for unauthenticated `gh`

Plan ID: PLAN-20260712-AUTH-DIALOG
Generated: 2026-07-12
Issue: #244 — "Detect unauthenticated gh and offer in-app device-code auth dialog"
Total Phases: 4
Requirements: REQ-AUTH-001..006

## Summary

When `gh` is not authenticated, Jefe already detects the failure
(`GhError::NotAuthenticated` via `categorize_error` / `check_auth`). Today the
remediation is a static "run `gh auth login`" string that forces the user to
leave the TUI. This plan adds an in-app modal that drives the **device-code
OAuth flow** itself — surfacing the one-time code + URL, requesting exactly
the scopes Jefe needs, and detecting success/failure/cancellation — so the
user never leaves Jefe.

## Critical findings (from `gh` source `internal/authflow/flow.go`)

- `gh` ALWAYS requests minimum scopes `["repo", "read:org", "gist"]` — exactly
  the scopes the issue names. Passing them explicitly via `--scopes` makes the
  grant auditable and keeps a documented contract even if `gh`'s defaults
  change.
- The device code + URL are written to **stderr** (`IO.ErrOut`), NOT stdout:
  - `DisplayCode` → `! First copy your one-time code: XXXX-XXXX`
  - `BrowseURL` (non-interactive) → `Open this URL to continue in your web
    browser: https://github.com/login/device/...`
- `isInteractive` is `opts.IO.CanPrompt() && opts.Token == ""`. When stdin is
  NOT a TTY (piped/closed), `CanPrompt()` is false → **non-interactive path**:
  no "Press Enter" prompt, URL printed directly. This is what makes the flow
  usable from a TUI subprocess.
- To prevent `gh` from trying to spawn a browser on a headless/remote box, set
  `GH_BROWSER=/bin/true`. The user opens the browser themselves on any device.
- Exit 0 = success; non-zero = failure (code expired, network, denied).

## Architecture (module boundaries — must respect)

| Concern | Owner | File(s) |
|---|---|---|
| Auth state machine + reducer transitions | state layer | `src/state/auth_ops.rs`, `src/state/types.rs` (new `ModalState::Auth` + `AuthDialogState`), `src/state/events.rs` (new `AppEvent`s) |
| Pure scope/args builder + stderr parser | github boundary (pure, no I/O) | `src/github/auth_device.rs` |
| Spawning `gh auth login --web` + streaming | runtime layer | `src/runtime/gh_auth.rs` (new) — NOT tmux; this is a one-shot subprocess, not a PTY session. Lives in `runtime/` because the runtime layer "owns process orchestration". |
| Message routing | messages layer | `src/messages.rs` (`SystemMessage` / new `AuthMessage`), `src/messages/event_conversion.rs` |
| Key handling for the auth modal | app_input layer | `src/app_input/modal_handlers.rs` (`handle_mode_auth_key`) |
| Triggering on `NotAuthenticated` | app_input dispatch | `src/app_input/gh_async.rs` + the `persist_*_failed` sites — convert `NotAuthenticated` errors into opening the modal instead of a bare error string |
| Render-only modal | UI layer | `src/ui/modals/auth.rs` |

The UI receives the code/URL/status as plain data (render-only). The runtime
owns the subprocess. The state layer owns transitions. No boundary is crossed.

## Non-interactive invocation

```
GH_BROWSER=/bin/true gh auth login \
  --hostname github.com \
  --git-protocol https \
  --web \
  --scopes repo \
  --scopes read:org \
  --scopes gist
```
Run with **stdin closed** (`Stdio::null`) so `CanPrompt()` is false. Capture
stderr (where the code/URL are written) and stdout. Wait for exit.

## Phase 0.5: Preflight Verification

- [x] `cargo --version` / `rustc` available (project builds today)
- [x] `serde_json` already a dependency (used throughout `github/`)
- [x] `std::process::Command` already used by `GhClient` (no new deps)
- [x] `check_auth()` and `categorize_error()` exist as the detection source of truth
- [x] Existing modal pattern (`ModalState`, `handle_mode_*_key`, `input.rs::modal_input_mode`) confirmed
- [x] `spawn_gh_task_with_panic` confirmed for off-thread gh calls
- [x] `RuntimeManager` trait exists; auth subprocess is a one-shot (not a PTY session), so it does NOT go through `RuntimeManager` — it is a sibling helper in `runtime/`.

## Phase 1: Pure github boundary — scopes, args, stderr parser (TDD)

### Files to create
- `src/github/auth_device.rs`
  - `pub const AUTH_SCOPES: &[&str] = &["repo", "read:org", "gist"];`
  - `pub fn build_auth_login_args(scopes: &[&str]) -> Vec<String>` — returns the
    `gh auth login ... --web --scopes ...` argv (hostname/git-protocol fixed to
    github.com/https per issue).
  - `pub fn build_auth_login_env() -> Vec<(&'static str, &'static str)>` —
    returns `("GH_BROWSER", "/bin/true")` (and is unit-testable).
  - `pub struct DeviceCode { pub code: String, pub verification_url: String }`
  - `pub fn parse_device_code(stderr: &str) -> Option<DeviceCode>` — extracts the
    one-time code (`XXXX-XXXX`) and the verification URL from `gh`'s stderr
    output. Tolerant of ANSI color codes (strip them first).
- `src/github/tests/auth_device.rs` — unit tests for all of the above.

### Tests (RED first)
- `build_auth_login_args` produces exactly `["auth","login","--hostname","github.com","--git-protocol","https","--web","--scopes","repo","--scopes","read:org","--scopes","gist"]`.
- `AUTH_SCOPES` is exactly `["repo","read:org","gist"]`.
- `build_auth_login_env` includes `GH_BROWSER=/bin/true`.
- `parse_device_code` extracts `7701-C5F6` + `https://github.com/login/device` from the real `gh` stderr sample (`! First copy your one-time code: 7701-C5F6\nOpen this URL to continue in your web browser: https://github.com/login/device/...`).
- `parse_device_code` strips ANSI color escapes.
- `parse_device_code` returns `None` for output without a code.

### Wire into `src/github/mod.rs`
- `mod auth_device;` + `pub use auth_device::{AUTH_SCOPES, DeviceCode, build_auth_login_args, build_auth_login_env, parse_device_code};`

## Phase 2: State machine + reducer (TDD)

### Files to create/modify
- `src/state/auth_ops.rs` (new) — `AuthDialogState` + transitions.
- `src/state/types.rs` — add `ModalState::Auth { state: AuthDialogState }` and the `AuthDialogState` type.
- `src/state/events.rs` — add `AppEvent` variants:
  - `OpenAuthDialog`
  - `AuthCodeReceived { code: String, url: String }`
  - `AuthSucceeded`
  - `AuthFailed { error: String }`
  - `AuthCancelled`
  - `AuthRetry`
- `src/state/mod.rs` — `mod auth_ops;` + route new events through a new
  `SystemMessage::Auth*` or a dedicated `AuthMessage` domain channel.

### `AuthDialogState` + state machine
```
AuthDialogState:
  Idle                 // not shown (modal closed)
  AwaitingCode         // subprocess spawned, waiting for code parse
  Confirming { code, url }   // code+URL shown; user authorizes in browser; polling
  Success
  Failed { error }     // transient failure; retry offered
  Cancelled
```
Transitions (deterministic, in `auth_ops.rs`):
- `Idle --OpenAuthDialog--> AwaitingCode`
- `AwaitingCode --AuthCodeReceived{code,url}--> Confirming{code,url}`
- `AwaitingCode --AuthFailed{e}--> Failed{e}`
- `Confirming --AuthSucceeded--> Success` (modal closes → `ModalState::None`)
- `Confirming --AuthFailed{e}--> Failed{e}`
- `Failed --AuthRetry--> AwaitingCode`
- `* --AuthCancelled--> Cancelled` (modal closes → `ModalState::None`)

### Tests (RED first) — `src/state/auth_ops_tests.rs`
- Every transition above (GIVEN state + event → THEN next state).
- `OpenAuthDialog` while another modal is open does NOT clobber it (defense: only opens when `modal == None` OR an explicit override). Decision: auth is high-priority remediation, so it CAN preempt a non-auth modal — but it stores the prior modal so Esc-restore is possible. (Simpler v1: only open when `modal == None`; the dispatch layer closes any bare error first.)
- Success transitions clear the modal to `None`.
- Cancel transitions clear the modal to `None` and set `error_message` to a helpful "auth cancelled" string.

## Phase 3: Runtime subprocess + dispatch wiring (TDD)

### Files to create/modify
- `src/runtime/gh_auth.rs` (new) — `pub fn run_device_auth(client: &GhClient) -> AuthRunResult` that:
  - builds args via `build_auth_login_args(AUTH_SCOPES)`,
  - builds env via `build_auth_login_env()`,
  - spawns `gh` with `Stdio::null` for stdin, piped stderr/stdout,
  - waits for exit,
  - parses stderr with `parse_device_code`,
  - returns `AuthRunResult { code: Option<DeviceCode>, exit_success: bool, stderr: String }`.
  - This is a **blocking** call intended to run inside `spawn_gh_task_with_panic`.
- `src/app_input/gh_async.rs` (or a new `src/app_input/auth_remediation.rs`) — `spawn_auth_flow` that runs `run_device_auth` off-thread and delivers `AuthCodeReceived` / `AuthFailed` / `AuthSucceeded` events back to state.
- Trigger sites: in the `persist_*_failed` helpers (issues/prs/actions list+detail), when the error string indicates `NotAuthenticated` (detected via a new `is_not_authenticated_error(&str) -> bool` helper that reuses the `categorize_error` contract), open the auth dialog INSTEAD OF (or in addition to) the bare error. To keep detection as the single source of truth, expose `pub fn is_not_authenticated(error_text: &str) -> bool` from `github/parse.rs` (or reuse `GhError` directly at the call site where the typed error is still available).

### Tests (RED first)
- `src/runtime/gh_auth_tests.rs` — `run_device_auth` parses a captured stderr fixture (using a fake `gh`-emitting helper binary is non-deterministic; instead test the **pure** parse path via `parse_device_code`, and test the args/env assembly). The subprocess spawn itself is integration-tested via the TUI scenario (Phase 4) and is kept thin.
  - **Decision:** keep `run_device_auth` a thin wrapper; its correctness is proven by `parse_device_code` unit tests + the args/env assembly tests + the e2e scenario. No mock theater (per RULES.md).
- `src/app_input/auth_remediation_tests.rs` — `is_not_authenticated("gh is not authenticated. Run: gh auth login")` is true; `is_not_authenticated("network error")` is false.
- A state-level test proving: a `NotAuthenticated` failure on the issues list path opens `ModalState::Auth` (the dispatch layer calls `OpenAuthDialog`).

### Message routing
- New `SystemMessage` variants (or a small `AuthMessage` channel): `OpenAuthDialog`, `AuthCodeReceived{code,url}`, `AuthSucceeded`, `AuthFailed{error}`, `AuthCancelled`, `AuthRetry`.
- `event_conversion.rs`: map the new `AppEvent` variants to the auth channel.
- `input.rs::modal_input_mode`: `ModalState::Auth { .. } => Some(InputMode::Auth)`.
- `app_shell.rs::dispatch_mode_specific_key`: `InputMode::Auth => handle_mode_auth_key(...); true`.
- `modal_handlers.rs::handle_mode_auth_key`:
  - `Esc` → `AuthCancelled` (and if a retry is mid-flight, the spawned `gh` is orphaned — it exits on its own when stdin is closed / the user never authorizes; v1 does not kill it, documented).
  - `Enter`/`r` when `Failed` → `AuthRetry` (re-spawn the flow).
  - Other keys ignored (the flow is not text-editable).

## Phase 4: UI modal + TUI scenario

### Files to create/modify
- `src/ui/modals/auth.rs` (new) — `AuthModal` iocraft component, render-only.
  Renders title, the one-time code (prominent), the verification URL,
  instructions, and the current status (awaiting/confirming/failed/cancelled).
  No state mutation.
- `src/ui/modals/mod.rs` — `mod auth; pub use auth::{AuthModal, AuthModalProps};`
- `src/ui/` root — render `AuthModal` when `state.modal == ModalState::Auth { .. }`.
- `dev-docs/tmux-scenarios/auth-dialog.json` (new) — manual scenario (NOT a CI
  gate, because it requires an unauthenticated `gh` + a real browser). Covers:
  trigger a GitHub operation while unauthenticated → auth dialog appears →
  (manual) authorize → success. Plus a cancellation scenario.
- `dev-docs/testing/tmux-harness.md` — document the new scenario.

### Tests (RED first)
- `src/ui/modals/auth.rs` `#[cfg(test)]` — pure projection helper
  `auth_dialog_view(&AuthDialogState) -> AuthDialogView` (iocraft-free, per the
  pure-views pattern) returning the lines to render; unit-test the lines for
  each state.
- The TUI scenario JSON is manual (documented as non-CI).

## Acceptance criteria mapping (issue → plan)

- ✅ State machine transitions idle→awaiting-code→confirming→success/failure/cancelled/expired — Phase 2 tests.
- ✅ Runtime parses one-time code + URL from `gh auth login --web` stderr — Phase 1 `parse_device_code` test.
- ✅ Scopes exactly `repo`, `read:org`, `gist` — Phase 1 `AUTH_SCOPES`/`build_auth_login_args` test.
- ✅ TUI scenario: trigger op while unauthenticated → dialog → success → op proceeds — Phase 4 scenario.
- ✅ Cancellation + failed/expired retry scenario — Phase 4 scenario + Phase 2 cancel/fail tests.
- ✅ `gh auth login` never invoked interactively (stdin null, `--web`, `GH_BROWSER=/bin/true`) — Phase 3 invocation.
- ✅ `cargo fmt`, `clippy -D warnings`, build, full tests pass (`make ci-check`) — final verification.

## Verification Commands

```bash
cargo fmt --all --check
CLIPPY_CONF_DIR=.github/clippy rustup run stable cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
make ci-check   # fmt + clippy gates + coverage (--fail-under-lines 30) + build + test
```

## Failure Recovery
- `git restore <files>` for any phase that fails verification.
- The new modules are additive; reverting `mod.rs`/`types.rs`/`events.rs`/`input.rs`/`app_shell.rs` registrations restores prior behavior.

## Execution Tracker

| Phase | Status | Verified | Semantic Verified | Notes |
|------:|--------|----------|-------------------|-------|
| P00.5 | done   | yes      | N/A               | preflight |
| P01   | todo   | todo     | todo              | pure github boundary |
| P02   | todo   | todo     | todo              | state machine |
| P03   | todo   | todo     | todo              | runtime + dispatch |
| P04   | todo   | todo     | todo              | UI + scenario |
