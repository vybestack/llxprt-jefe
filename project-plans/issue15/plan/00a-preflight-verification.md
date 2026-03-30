# Phase 00A: Preflight Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P00A`

## Prerequisites
- Required: specification finalized (`project-plans/issue15/specification.md`).
- Required: PLAN.md and PLAN-TEMPLATE.md reviewed.
- Verify existing docs: `specification.md`, `analysis/domain-model.md` (if exists).
- Expected files from previous phase: none (this is the first phase).

## Requirements Implemented (Expanded)

### REQ-ISS-NFR-003: Maintainability — Toolchain and Boundary Feasibility
**Requirement text**: GitHub client boundary is isolated and testable. Event/reducer pattern is followed. Verify Rust toolchain is available and target module boundaries exist for integration.

Behavior contract:
- GIVEN the Jefe workspace at plan start
- WHEN preflight verification commands are executed
- THEN Rust toolchain is confirmed available, `gh` CLI is installed and authenticated, all target source file paths are validated to exist, key enums/structs match expected signatures, and call paths are reachable.

Why it matters:
- Prevents planning against unavailable tools or non-existent integration paths. A failed preflight check must halt plan execution and trigger plan revision.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P00A.md` -- preflight evidence log
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P00A`
  - marker: `@requirement REQ-ISS-NFR-003`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- tracker status update
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P00A`

### Pseudocode traceability (if impl phase)
- N/A (preflight phase)

## Preflight Checklists

### 1) Toolchain Verification (Enforceable)
- [ ] `cargo --version` exits 0
- [ ] `rustc --version` exits 0
- [ ] `cargo clippy --version` exits 0
- [ ] `cargo llvm-cov --version` exits 0 (if coverage gate required by plan)

```bash
cargo --version
rustc --version
cargo clippy --version
```

### 2) `gh` CLI Verification (Enforceable)
- [ ] `gh --version` exits 0
- [ ] `gh auth status` exits 0 (authenticated)
- [ ] `gh issue list --repo <test-repo> --limit 1` succeeds (API reachability)

```bash
gh --version
gh auth status
```

### 3) Interface Existence Verification (Enforceable)
- [ ] `src/state/mod.rs` exists
- [ ] `src/domain/mod.rs` exists
- [ ] `src/input.rs` exists
- [ ] `src/app_input/mod.rs` exists
- [ ] `src/persistence/mod.rs` exists
- [ ] `src/lib.rs` exists
- [ ] `src/ui/components/` directory exists
- [ ] `src/ui/screens/` directory exists
- [ ] `src/ui/modals/` directory exists

```bash
test -f src/state/mod.rs && echo "state/mod.rs: OK" || echo "MISSING: state/mod.rs"
test -f src/domain/mod.rs && echo "domain/mod.rs: OK" || echo "MISSING: domain/mod.rs"
test -f src/input.rs && echo "input.rs: OK" || echo "MISSING: input.rs"
test -f src/app_input/mod.rs && echo "app_input/mod.rs: OK" || echo "MISSING: app_input/mod.rs"
test -f src/persistence/mod.rs && echo "persistence/mod.rs: OK" || echo "MISSING: persistence/mod.rs"
test -f src/lib.rs && echo "lib.rs: OK" || echo "MISSING: lib.rs"
test -d src/ui/components && echo "ui/components/: OK" || echo "MISSING: ui/components/"
test -d src/ui/screens && echo "ui/screens/: OK" || echo "MISSING: ui/screens/"
test -d src/ui/modals && echo "ui/modals/: OK" || echo "MISSING: ui/modals/"
```

### 4) Concrete File-Level Signature Verification (Enforceable)

Verify exact types, function signatures, and enum variants in target files.

#### 4a) `src/state/types.rs` — Enum and Struct Signatures (re-exported via `src/state/mod.rs`)

- [ ] `ScreenMode` enum exists at ~L221 (in src/state/types.rs) with exactly: `Dashboard` (default), `Split`
- [ ] `PaneFocus` enum exists at ~L229 (in src/state/types.rs) with exactly: `Repositories` (default), `Agents`, `Terminal`
- [ ] `AppEvent` enum exists at ~L268 (in src/state/types.rs) with variants including: `NavigateUp`, `NavigateDown`, `NavigateLeft`, `NavigateRight`, `SelectRepository(usize)`, `SelectAgent(usize)`, `CyclePaneFocus`, `ToggleTerminalFocus`, `EnterSplitMode`, `ExitSplitMode`, `OpenHelp`, `OpenSearch`, `Quit`
- [ ] `AppState` struct exists at ~L238 (in src/state/types.rs) with fields: `repositories: Vec<Repository>`, `agents: Vec<Agent>`, `selected_repository_index: Option<usize>`, `selected_agent_index: Option<usize>`, `screen_mode: ScreenMode`, `pane_focus: PaneFocus`, `terminal_focused: bool`, `modal: ModalState`
- [ ] `ModalState` enum exists at ~L171 (in src/state/types.rs) with variants: `None` (default), `Help`, `Search { query }`, `NewRepository { fields, focus, cursor }`, `EditRepository { ... }`, `ConfirmDeleteRepository { ... }`, `NewAgent { ... }`, `EditAgent { ... }`, `ConfirmDeleteAgent { ... }`, `ConfirmKillAgent { ... }`, `PreflightPrompt { ... }`
- [ ] `AppState::apply()` method exists in `src/state/mod.rs` and is the event reducer

```bash
# Verify exact ScreenMode variants
echo "--- ScreenMode ---"
grep -A5 "pub enum ScreenMode" src/state/types.rs

# Verify exact PaneFocus variants
echo "--- PaneFocus ---"
grep -A6 "pub enum PaneFocus" src/state/types.rs

# Verify AppState struct fields
echo "--- AppState fields ---"
grep -A20 "pub struct AppState" src/state/types.rs

# Verify AppEvent exists and sample variants
echo "--- AppEvent sample ---"
grep -n "pub enum AppEvent" src/state/types.rs
grep -c "NavigateUp\|NavigateDown\|CyclePaneFocus\|EnterSplitMode\|ExitSplitMode\|OpenHelp\|Quit" src/state/types.rs

# Verify ModalState variants
echo "--- ModalState ---"
grep -A3 "pub enum ModalState" src/state/types.rs

# Verify apply() method
echo "--- apply() ---"
grep -n "fn apply" src/state/mod.rs | head -5
```

#### 4b) `src/input.rs` — InputMode and Function Signatures

- [ ] `InputMode` enum exists at L9 with exactly: `Normal`, `TerminalCapture`, `Help`, `Search`, `Form`, `Confirm`
- [ ] `fn input_mode_for_state(state: &AppState) -> InputMode` exists at L30
- [ ] `fn route_search_key(key: &KeyEvent) -> SearchKeyRoute` exists at L54
- [ ] `input_mode_for_state` currently returns `Normal` when `modal == None` and `terminal_focused == false`

```bash
# Verify exact InputMode variants
echo "--- InputMode ---"
grep -A8 "pub enum InputMode" src/input.rs

# Verify function signatures
echo "--- input_mode_for_state ---"
grep -n "pub fn input_mode_for_state" src/input.rs

echo "--- route_search_key ---"
grep -n "pub fn route_search_key" src/input.rs

# Verify input_mode_for_state returns Normal as fallback
echo "--- Normal fallback ---"
grep -A3 "InputMode::Normal" src/input.rs
```

#### 4c) `src/app_input/mod.rs` — Key Dispatch Signatures

- [ ] `fn handle_normal_key_event` — L61
- [ ] `fn dispatch_app_event(...)` exists and is the event dispatch entry point
- [ ] Key routing for `i` key is NOT currently bound (available for issues mode)
- [ ] Key routing for `a`/`A` currently sets `pane_focus = PaneFocus::Agents` (L174)
- [ ] Key routing for `s`/`S` currently maps to `EnterSplitMode` when `screen_mode == Dashboard` (L148-150)
- [ ] Key routing for `Ctrl-d` currently maps to `OpenDeleteAgent`/`OpenDeleteRepository` (L129-137)
- [ ] Key routing for `Ctrl-k` currently maps to `KillAgent` (L140-142 in src/app_input/normal.rs)
- [ ] Key routing for `l`/`L` currently maps to `RelaunchAgent` (L145 in src/app_input/normal.rs)

```bash
# Verify handle_normal_key_event signature
echo "--- handle_normal_key_event ---"
grep -n "pub fn handle_normal_key_event" src/app_input/normal.rs

# Verify dispatch_app_event
echo "--- dispatch_app_event ---"
grep -n "fn dispatch_app_event" src/app_input/mod.rs

# Verify 'i' key is NOT currently bound
echo "--- 'i' key check ---"
grep -rn "Char('i'" src/app_input/ || echo "OK: 'i' not bound (available)"

# Verify current 'a' binding (focus agents) — in normal.rs
echo "--- 'a' key binding ---"
grep -n "Char('a'" src/app_input/normal.rs

# Verify current 's' binding (split mode) — in normal.rs
echo "--- 's' key binding ---"
grep -n "Char('s'" src/app_input/normal.rs

# Verify Ctrl-d binding (delete) — in normal.rs
echo "--- Ctrl-d binding ---"
grep -n "Char('d'" src/app_input/normal.rs

# Verify Ctrl-k binding (kill) — in normal.rs
echo "--- Ctrl-k binding ---"
grep -n "Char('k'" src/app_input/normal.rs

# Verify 'l' binding (relaunch) — in normal.rs
echo "--- 'l' binding ---"
grep -n "Char('l'" src/app_input/normal.rs
```

#### 4d) `src/domain/mod.rs` — Repository Struct

- [ ] `pub struct Repository` exists with fields: `id: RepositoryId`, `name: String`, `slug: String`, `base_dir: PathBuf`, `default_profile: String`
- [ ] `Repository` derives `Serialize, Deserialize` (for persistence)
- [ ] `Repository` does NOT currently have `issue_base_prompt` field

```bash
# Verify Repository struct
echo "--- Repository ---"
grep -A15 "pub struct Repository" src/domain/mod.rs | head -20

# Verify serde derives
echo "--- serde ---"
grep -B2 "pub struct Repository" src/domain/mod.rs

# Verify issue_base_prompt not yet present
echo "--- issue_base_prompt absence ---"
grep "issue_base_prompt" src/domain/mod.rs && echo "UNEXPECTED: field already exists" || echo "OK: field not yet present"
```

#### 4e) UI Module Structure

- [ ] `src/ui/components/mod.rs` exists and declares component modules
- [ ] `src/ui/screens/mod.rs` exists and declares screen modules
- [ ] `src/ui/screens/dashboard.rs` exists (dashboard layout)
- [ ] `src/ui/screens/new_repository.rs` exists (repo form)
- [ ] `src/ui/components/keybind_bar.rs` exists
- [ ] `src/ui/modals/help.rs` exists
- [ ] No `issue_list.rs`, `issue_detail.rs`, `filter_controls.rs`, `agent_chooser.rs` exist yet in `src/ui/components/`
- [ ] No `issues.rs` exists yet in `src/ui/screens/`
- [ ] No `src/github/` directory exists yet

```bash
echo "--- UI module structure ---"
cat src/ui/components/mod.rs
echo "---"
cat src/ui/screens/mod.rs
echo "---"
test -f src/ui/screens/dashboard.rs && echo "dashboard.rs: OK" || echo "MISSING"
test -f src/ui/screens/new_repository.rs && echo "new_repository.rs: OK" || echo "MISSING"
test -f src/ui/components/keybind_bar.rs && echo "keybind_bar.rs: OK" || echo "MISSING"
test -f src/ui/modals/help.rs && echo "help.rs: OK" || echo "MISSING"

echo "--- New files should NOT exist yet ---"
test -f src/ui/components/issue_list.rs && echo "UNEXPECTED: issue_list.rs already exists" || echo "OK: not present"
test -f src/ui/components/issue_detail.rs && echo "UNEXPECTED: issue_detail.rs already exists" || echo "OK: not present"
test -f src/ui/components/filter_controls.rs && echo "UNEXPECTED: filter_controls.rs already exists" || echo "OK: not present"
test -f src/ui/components/agent_chooser.rs && echo "UNEXPECTED: agent_chooser.rs already exists" || echo "OK: not present"
test -f src/ui/screens/issues.rs && echo "UNEXPECTED: issues.rs already exists" || echo "OK: not present"
test -d src/github && echo "UNEXPECTED: github/ already exists" || echo "OK: not present"
```

### 5) Call-Path Feasibility Verification (Enforceable)

- [ ] All existing `match ScreenMode` arms enumerated — count total match sites to know how many need updating
- [ ] `CyclePaneFocus` handler in `apply()` verified — must understand current cycling logic to avoid breaking it
- [ ] `input_mode_for_state()` currently returns `Normal` as the default — issues mode detection will be added before this fallback

```bash
# Count all ScreenMode match sites across codebase
echo "--- ScreenMode match sites ---"
grep -rn "ScreenMode::" src/ | grep -v "test\|///\|//!" | wc -l
grep -rn "screen_mode ==" src/ | wc -l

# Enumerate match sites by file
echo "--- ScreenMode by file ---"
grep -rn "ScreenMode::" src/ | grep -v "test\|///\|//!" | cut -d: -f1 | sort | uniq -c | sort -rn

# Verify CyclePaneFocus handler
echo "--- CyclePaneFocus ---"
grep -n "CyclePaneFocus" src/state/types.rs src/state/mod.rs

# Verify input_mode_for_state default path
echo "--- input_mode_for_state default ---"
grep -A5 "InputMode::Normal" src/input.rs
```

### 6) Test Infrastructure Verification (Enforceable)
- [ ] Inline `#[cfg(test)]` modules exist in at least one `src/` file
- [ ] `cargo test --workspace --all-features` exits 0 (baseline green)

```bash
grep -rn "#\[cfg(test)\]" src/ | head -10
cargo test --workspace --all-features
```

### 7) Workspace Quality Gate Baseline (Enforceable)
- [ ] `cargo fmt --all --check` exits 0
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits 0
- [ ] `cargo test --workspace --all-features` exits 0

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Blocker Gate Decision
- [ ] **PASS**: All 7 checklists above pass. Proceed to P01.
- [ ] **FAIL**: One or more checks failed. Stop and revise plan before proceeding.

If any check fails, document:
- Which check failed
- Root cause
- Remediation action taken
- Whether plan assumptions need revision

## Structural Verification Checklist
- [ ] Rust toolchain commands execute successfully.
- [ ] `gh` CLI is installed and authenticated.
- [ ] All target source files exist at expected paths.
- [ ] `ScreenMode` has exactly `Dashboard`, `Split` variants (no more, no less).
- [ ] `PaneFocus` has exactly `Repositories`, `Agents`, `Terminal` variants.
- [ ] `InputMode` has exactly `Normal`, `TerminalCapture`, `Help`, `Search`, `Form`, `Confirm` variants.
- [ ] `AppState` struct fields match expected schema.
- [ ] `handle_normal_key_event()` signature matches expected parameters.
- [ ] `input_mode_for_state()` signature matches expected parameters.
- [ ] `Repository` struct exists without `issue_base_prompt` field.
- [ ] `i` key is not currently bound in `handle_normal_key_event()`.
- [ ] All target key bindings (`a`, `s`, `Ctrl-d`, `Ctrl-k`, `l`) have verified current behavior.
- [ ] Key dispatch entry points (`dispatch_app_event`, `handle_normal_key_event`) are identified with line numbers.
- [ ] Test infrastructure (inline `#[cfg(test)]` modules) is confirmed.
- [ ] Quality gate baseline passes (fmt + clippy + test).
- [ ] New files/directories (`src/github/`, UI components) do NOT already exist.

## Semantic Verification Checklist (Mandatory)
- [ ] Plan file path assumptions map to real files/modules in the codebase.
- [ ] `gh` CLI can query issues for at least one accessible repository.
- [ ] `ScreenMode` enum is simple enough to extend with a new variant without breaking match arms (verify all match statements enumerated and counted).
- [ ] `PaneFocus` enum extension is NOT needed — plan uses separate `IssueFocus` enum (verified).
- [ ] `InputMode` enum can be extended with 5 new variants — all match sites enumerated.
- [ ] `i` key is available (not bound) for mode entry.
- [ ] `a` key's current behavior (focus agents pane) will need conditional override in issues mode — current binding location verified.
- [ ] Feature behavior is reachable from real app flow: `i` key can be wired into `handle_normal_key_event()`.
- [ ] No blocker remains unresolved.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" project-plans/issue15/plan/
```

## Success Criteria
- [ ] All preflight checks pass.
- [ ] Blocker gate decision is PASS.
- [ ] Any identified concern has explicit remediation path documented.

## Failure Recovery
- rollback steps: Stop phase progression; patch plan targets and assumptions before proceeding.
- blocking issues to resolve before next phase: unavailable toolchain, missing `gh` CLI, non-existent source paths, failing baseline quality gate, signature mismatches.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P00A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P00A`
- timestamp
- toolchain versions recorded
- all 7 checklist results
- signature verification evidence (exact enum variants, function signatures)
- blocker gate decision (PASS/FAIL)
- any remediation actions taken

---

## Concrete Signature Verification

This section lists the actual function signatures, enum variants, and struct fields—with verified line numbers—that the plan depends on. These were read directly from source at plan authoring time and must be re-confirmed during P00A execution.

### `src/state/types.rs` — Verified Signatures (re-exported via `src/state/mod.rs`)

#### `ScreenMode` — L221–225 (in src/state/types.rs)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreenMode {
    #[default]
    Dashboard,   // L223
    Split,       // L224
}
```

Verification command: `grep -n "pub enum ScreenMode" src/state/types.rs`

#### `PaneFocus` — L229–234 (in src/state/types.rs)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaneFocus {
    #[default]
    Repositories,  // L231
    Agents,        // L232
    Terminal,      // L233
}
```

Verification command: `grep -n "pub enum PaneFocus" src/state/types.rs`

#### `AppState` struct — L238–264 (in src/state/types.rs)

```rust
#[derive(Debug, Default, Clone)]
pub struct AppState {
    // Data
    pub repositories: Vec<Repository>,
    pub agents: Vec<Agent>,

    // Selection
    pub selected_repository_index: Option<usize>,
    pub selected_agent_index: Option<usize>,
    pub last_selected_agent_by_repo: Vec<(RepositoryId, AgentId)>,

    // View state
    pub screen_mode: ScreenMode,
    pub pane_focus: PaneFocus,
    pub terminal_focused: bool,
    pub hide_idle_repositories: bool,

    // Modal/form state
    pub modal: ModalState,

    // Split mode state
    pub split_filter: Option<RepositoryId>,
    pub split_grab_index: Option<usize>,

    // Errors/warnings
    pub error_message: Option<String>,
    pub warning_message: Option<String>,
}
```

Verification command: `grep -n "pub struct AppState" src/state/types.rs`

#### `AppEvent` enum — L268–338

All variants present at plan authoring time (verified):

```
NavigateUp, NavigateDown, NavigateLeft, NavigateRight,
SelectRepository(usize), SelectAgent(usize), JumpToAgentByShortcut(u8),
CyclePaneFocus, ToggleTerminalFocus, ToggleHideIdleRepositories,
EnterSplitMode, ExitSplitMode,
EnterGrabMode, ExitGrabMode, GrabMoveUp, GrabMoveDown,
SetSplitFilter(Option<RepositoryId>),
OpenHelp, OpenSearch, CloseModal, SubmitForm,
FormChar(char), FormBackspace, FormDelete,
FormMoveCursorLeft, FormMoveCursorRight,
FormNextField, FormPrevField, FormToggleCheckbox,
OpenNewRepository, OpenEditRepository(RepositoryId),
OpenDeleteRepository(RepositoryId),
OpenNewAgent(RepositoryId), OpenEditAgent(AgentId),
OpenDeleteAgent(AgentId), ToggleDeleteWorkDir,
KillAgent(AgentId), RelaunchAgent(AgentId),
AgentStatusChanged(AgentId, AgentStatus),
PersistenceLoadSuccess, PersistenceLoadFailed(String),
PersistenceSaveSuccess, PersistenceSaveFailed(String),
SetTheme(String), ThemeResolveFailed(String),
Quit, ClearError, ClearWarning,
```

All of these are PRESERVED. New issues-mode variants are appended.

Verification command: `sed -n '268,338p' src/state/types.rs`

#### `AppState::apply()` — L233

```rust
pub fn apply(mut self, event: AppEvent) -> Self {
```

Full signature: takes ownership of `self` and the event, returns new `Self`. The issues-mode events extend the `match` arm in this function.

Verification command: `grep -n "pub fn apply" src/state/mod.rs`

### `src/app_input/mod.rs` — Verified Signatures

#### `dispatch_app_event` — L359

```rust
pub fn dispatch_app_event(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
```

Parameters:
- `app_state: &mut AppStateHandle` — locked mutable handle to app state
- `ctx: &SharedContext` — shared runtime context (terminal handles, etc.)
- `evt: AppEvent` — the event to dispatch

Verification command: `sed -n '359,361p' src/app_input/mod.rs`

#### `handle_normal_key_event` — L61–864

```rust
pub fn handle_normal_key_event(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> Option<AppEvent> {
```

Parameters:
- `app_state: &mut AppStateHandle` — mutable state handle
- `should_quit: &mut QuitHandle` — quit signal
- `ctx: &SharedContext` — shared context
- `key_event: &KeyEvent` — the crossterm key event
- `screen_mode: ScreenMode` — current screen mode (passed in as discriminator)

Returns: `Option<AppEvent>` — the event to dispatch, or `None` if key was consumed without emitting an event (e.g., direct state mutation for focus changes).

The new `handle_issues_mode_key()` function will have the same parameter shape. It will be called when `screen_mode == ScreenMode::DashboardIssues` before the existing handler logic runs, so the `ScreenMode::Dashboard` and `ScreenMode::Split` paths remain unmodified.

Verification command: `grep -n "pub fn handle_normal_key_event" src/app_input/normal.rs`

#### Key bindings currently occupying relevant keys (verified in `handle_normal_key_event`):

| Key | Current behavior | Source line(s) | Plan action |
|-----|-----------------|----------------|-------------|
| `i` / `I` | **Not bound** — `grep -rn "Char('i'" src/app_input/` returns no match | — | Bind to `EnterIssuesMode` |
| `a` / `A` | Sets `pane_focus = PaneFocus::Agents` directly (no event emitted) | ~L174 | Suppress / redirect to `ExitIssuesMode` when in `DashboardIssues` |
| `s` / `S` | `EnterSplitMode` when `screen_mode == Dashboard` | ~L148 | Explicit no-op when `screen_mode == DashboardIssues` |
| `Esc` | `ExitSplitMode` when `screen_mode == Split` | ~L151 | In issues mode: 6-level precedence chain (component-001 L115–127) |
| `Ctrl-d` | `OpenDeleteAgent` / `OpenDeleteRepository` | ~L129–137 in normal.rs | Suppress (no-op) in issues mode |
| `Ctrl-k` | `KillAgent` | ~L140-142 in normal.rs | Suppress (no-op) in issues mode |
| `l` / `L` | `RelaunchAgent` | ~L145 | Suppress (no-op) in issues mode |
| `r` / `R` | Sets `pane_focus = PaneFocus::Repositories` directly | ~L168–173 | Suppress in issues mode (`r` → inline reply; focus-repo not applicable) |

Verification commands:
```bash
grep -n "Char('d'\|Char('k'\|Char('l'\|Char('s'" src/app_input/normal.rs    # Ctrl-d, Ctrl-k, l, s bindings
grep -n "Char('r'\|Char('a'" src/app_input/normal.rs    # r, a pane-focus bindings
grep -rn "Char('i'" src/app_input/   # confirm 'i' unbound
```

### `src/domain/mod.rs` — Verified Signatures

#### `Repository` — L196–206

```rust
/// A repository is a named codebase container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: RepositoryId,
    pub name: String,
    pub slug: String,
    pub base_dir: PathBuf,
    pub default_profile: String,
    #[serde(default)]
    pub remote: RemoteRepositorySettings,
    pub agent_ids: Vec<AgentId>,
}
```

- Derives `Serialize, Deserialize` — serde-safe for persistence.
- Does **not** currently have `issue_base_prompt`. Plan adds it as:
  ```rust
  #[serde(default)]
  pub issue_base_prompt: String,
  ```
  The `#[serde(default)]` attribute ensures existing `state.json` files (which lack the field) deserialize cleanly with an empty string.

Verification command: `grep -A15 "pub struct Repository" src/domain/mod.rs`

### Summary Table — Signatures the Plan Depends On

| Identifier | Kind | File | Line | Plan Dependency |
|-----------|------|------|------|----------------|
| `ScreenMode::Dashboard` | Enum variant | `src/state/types.rs` | L223 | Default preserved; issues mode adds `DashboardIssues` alongside |
| `ScreenMode::Split` | Enum variant | `src/state/types.rs` | L224 | Preserved; suppression guard `screen_mode == Dashboard` at L148 still fires correctly |
| `PaneFocus::Repositories` | Enum variant | `src/state/types.rs` | L231 | Preserved; issues mode uses `IssueFocus` instead |
| `PaneFocus::Agents` | Enum variant | `src/state/types.rs` | L232 | Preserved; `a` key redirected only when `screen_mode == DashboardIssues` |
| `PaneFocus::Terminal` | Enum variant | `src/state/types.rs` | L233 | Preserved; unaffected by issues mode |
| `AppState` — `src/state/types.rs` L238 | Gains `issues_state: IssuesState` field |
| `AppState::apply()` — L233 | Issues events handled in new `match` arms added here |
| `AppEvent` — `src/state/types.rs` L268–346 | All preserved; new issues variants appended |
| `dispatch_app_event(app_state, ctx, evt)` | Function | `src/app_input/mod.rs` | L359 | Signature unchanged; issues events flow through this |
| `handle_normal_key_event` — L61 | Gains issues-mode branch; existing arms untouched |
| `Repository` — L196–206 | All preserved; `issue_base_prompt: String` added with `#[serde(default)]` |
