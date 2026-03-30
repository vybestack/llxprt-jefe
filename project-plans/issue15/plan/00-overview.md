# Plan: Issues Mode

Plan ID: `PLAN-20260329-ISSUES-MODE`
Generated: 2026-03-29
Total Phases: 16 core phases (+ paired verification phases)
Requirements: REQ-ISS-001..014, REQ-ISS-NFR-001..003

## Critical Reminders

Before implementing any phase:
1. Preflight verification is complete (P00A)
2. Integration points are explicitly listed
3. TDD cycle is defined per slice
4. Lint/test/coverage gates are declared

## Global Mandate: Pseudocode Line Ranges in All Implementation Phases

Every implementation phase (P03, P05, P06, P08, P09, P11, P12, P14, P15) MUST cite concrete pseudocode line ranges for each file edit — not just component names. Verification phases MUST confirm that pseudocode references are present. The only exception is UI layout/rendering code that has no algorithmic pseudocode counterpart, in which case the phase MUST reference the state-layer pseudocode conditions that drive rendering decisions (e.g., component-001 lines that define the state fields the UI reads).

## Global Mandate: Per-File Traceability Markers

Every file edit in every implementation phase MUST include explicit traceability markers in the code:
- `@plan PLAN-20260329-ISSUES-MODE.PNN` — which phase introduced the change
- `@requirement REQ-ISS-NNN` — which requirement(s) the change satisfies
- `@pseudocode component-NNN lines X-Y` — which pseudocode lines the change implements

These markers MUST appear as Rust doc comments on the function, struct, enum, or module being changed. Verification phases MUST confirm markers are present via `grep` checks.

## Global Mandate: Event-Driven Issues Mode Handlers

All issues-mode key handlers MUST return `Option<AppEvent>` rather than directly mutating state via `app_state.write()`.

All issues-mode state transitions MUST go through `AppState::apply()`.

This differs from some existing dashboard handlers (e.g., `r`, `a`, `n` keys) which directly mutate state via `app_state.write()`. The issues-mode code intentionally uses the pure event-driven pattern for testability and state machine clarity.

The existing direct-mutation handlers will be migrated to event-driven in a future effort.

Note: `handle_normal_key_event` takes `&mut AppStateHandle` and returns `Option<AppEvent>`. The issues-mode early guard should return the event without calling `app_state.write()`.

## Plan Scope

Add GitHub Issues Mode to Jefe: issue browsing/filtering/searching per repository, inline comment creation/editing/reply, send-to-agent with issue context, and `issue_base_prompt` repository config field. Extends existing architecture without forking.

## Glossary / Terminology Mapping

The specification uses logical role names that map to concrete code constructs as follows:

| Specification Term | Current Code Construct | Notes |
|--------------------|----------------------|-------|
| `dashboard_issues` | `ScreenMode::DashboardIssues` (new variant) | Logical mode name in spec; implemented as new enum variant |
| `dashboard_agents` | `ScreenMode::Dashboard` (existing) | The current `Dashboard` variant IS the agents mode. NOT renamed. |
| "Agents Mode" | `ScreenMode::Dashboard` + `PaneFocus::{Repositories, Agents, Terminal}` | Current default behavior |
| "Issues Mode" | `ScreenMode::DashboardIssues` + `IssueFocus::{RepoList, IssueList, IssueDetail}` | New mode added |
| `issue_list` focus | `IssueFocus::IssueList` (new type) | Not a `PaneFocus` variant; separate enum for issues mode |
| `issue_detail` focus | `IssueFocus::IssueDetail` (new type) | Subfocus within issues mode |
| `repo_list` focus (in issues mode) | `IssueFocus::RepoList` (new type) | Reuses repo sidebar UI but focus is tracked by `IssueFocus` |
| `split_mode` | `ScreenMode::Split` (existing) | Unchanged |
| "focus domain" | The active `IssueFocus` variant in issues mode | Determines which key handler receives input |
| "inline control" | `InlineState::{Composer, Editor}` | At most one active at a time (exclusivity invariant) |
| "scope change" | User navigates to a different repository while in issues mode | Invalidates all issue state; discards unsent drafts |

**Key invariant**: Existing `ScreenMode` variants (`Dashboard`, `Split`) and `PaneFocus` variants (`Repositories`, `Agents`, `Terminal`) are **NOT renamed or removed**. Issues mode extends these enums with new variants and introduces new focus-tracking types alongside them.

## Baseline-to-Target Enum Evolution

### `ScreenMode` (src/state/mod.rs L228-233)

```
BASELINE (current):              TARGET (after plan):
┌──────────────────────┐         ┌──────────────────────┐
│ pub enum ScreenMode  │         │ pub enum ScreenMode  │
│ {                    │         │ {                    │
│   Dashboard,   // ←  │  ──→   │   Dashboard,         │  ← PRESERVED (agents mode)
│   Split,       // ←  │  ──→   │   Split,             │  ← PRESERVED (split mode)
│ }                    │         │   DashboardIssues,   │  ← NEW (issues mode)
│                      │         │ }                    │
└──────────────────────┘         └──────────────────────┘
```

- `Dashboard` remains the default and represents agents-mode behavior. NOT renamed.
- `Split` remains unchanged.
- `DashboardIssues` is added as a new variant for issues mode.
- **All existing `match ScreenMode` arms must be updated** to handle `DashboardIssues`.

### `PaneFocus` (src/state/mod.rs L236-242)

```
BASELINE (current):              TARGET (after plan):
┌──────────────────────┐         ┌──────────────────────┐
│ pub enum PaneFocus   │         │ pub enum PaneFocus   │
│ {                    │         │ {                    │
│   Repositories, // ← │  ──→   │   Repositories,      │  ← PRESERVED
│   Agents,       // ← │  ──→   │   Agents,            │  ← PRESERVED
│   Terminal,     // ← │  ──→   │   Terminal,           │  ← PRESERVED
│ }                    │         │ }                    │
└──────────────────────┘         └──────────────────────┘
```

- `PaneFocus` is **NOT modified**. All three existing variants are preserved.
- Issues mode uses a **separate** `IssueFocus` enum for its internal focus tracking:

```
NEW TYPE (added):
┌──────────────────────────┐
│ pub enum IssueFocus      │
│ {                        │
│   RepoList,              │  ← repo sidebar focus in issues mode
│   IssueList,             │  ← issue list pane focus
│   IssueDetail,           │  ← issue detail pane focus
│ }                        │
└──────────────────────────┘
```

### `InputMode` (src/input.rs L9-16)

```
BASELINE (current):              TARGET (after plan):
┌──────────────────────┐         ┌──────────────────────┐
│ pub enum InputMode   │         │ pub enum InputMode   │
│ {                    │         │ {                    │
│   Normal,       // ← │  ──→   │   Normal,            │  ← PRESERVED
│   TerminalCapture,←  │  ──→   │   TerminalCapture,   │  ← PRESERVED
│   Help,         // ← │  ──→   │   Help,              │  ← PRESERVED
│   Search,       // ← │  ──→   │   Search,            │  ← PRESERVED
│   Form,         // ← │  ──→   │   Form,              │  ← PRESERVED
│   Confirm,      // ← │  ──→   │   Confirm,           │  ← PRESERVED
│ }                    │         │   IssuesNormal,      │  ← NEW
│                      │         │   IssuesInline,      │  ← NEW
│                      │         │   IssuesSearch,      │  ← NEW
│                      │         │   IssuesFilter,      │  ← NEW
│                      │         │   IssuesChooser,     │  ← NEW
│                      │         │ }                    │
└──────────────────────┘         └──────────────────────┘
```

### `AppEvent` (src/state/mod.rs L276-346)

All existing variants are **PRESERVED**. New issue-specific variants are **added**:

```
NEW AppEvent variants (added, not replacing anything):
  EnterIssuesMode,
  ExitIssuesMode,
  RefocusIssueList,
  IssuesNavigateUp, IssuesNavigateDown,
  IssuesNavigatePageUp, IssuesNavigatePageDown,
  IssuesNavigateHome, IssuesNavigateEnd,
  IssuesEnter,
  IssuesCycleFocus, IssuesCycleFocusReverse,
  IssueListLoaded { ... }, IssueListPageLoaded { ... },
  IssueListLoadFailed { ... },
  IssueDetailLoaded { ... }, IssueDetailLoadFailed { ... },
  IssueCommentsPageLoaded { ... }, IssueCommentsPageFailed { ... },
  OpenFilterControls, CloseFilterControls,
  ApplyFilter { ... }, ClearFilter,
  FocusSearchInput, BlurSearchInput,
  ApplySearch { ... }, ClearSearch,
  OpenNewCommentComposer, OpenReplyComposer { ... },
  OpenInlineEditor { ... },
  InlineChar(char), InlineBackspace, InlineSubmit, InlineCancelOrEsc,
  CommentCreated { ... }, CommentCreateFailed { ... },
  IssueBodyUpdated, CommentUpdated { ... }, MutationFailed { ... },
  OpenAgentChooser, AgentChooserNavigateUp, AgentChooserNavigateDown,
  AgentChooserConfirm, AgentChooserCancel,
  SendToAgentCompleted, SendToAgentFailed { ... },
```

### `AppState` (src/state/mod.rs L246-272)

Existing fields are **PRESERVED**. New field added:

```
NEW field (added):
  pub issues_state: IssuesState,   ← aggregates all issues-mode state
```

### `ModalState` (src/state/mod.rs L171-225)

**NOT modified**. Issues mode uses inline controls and overlays tracked in `IssuesState`, not `ModalState`.

### `Repository` (src/domain/mod.rs)

Existing fields are **PRESERVED**. New field added:

```
NEW field (added):
  #[serde(default)]
  pub issue_base_prompt: String,   ← empty string default for backward compat
```

## How Existing Behavior Is Preserved

1. **Agents mode**: `ScreenMode::Dashboard` is unchanged. All existing `match` arms continue to work. `PaneFocus` cycling between `Repositories`/`Agents`/`Terminal` is untouched.
2. **Split mode**: `ScreenMode::Split` is unchanged. `s`/`S` entry and `Esc` exit continue to work when NOT in issues mode (issues mode suppresses `s` key).
3. **Key routing**: Existing `handle_normal_key_event()` (L858 in app_input.rs) handles `ScreenMode::Dashboard` and `ScreenMode::Split`. A new branch dispatches `ScreenMode::DashboardIssues` to a new `handle_issues_mode_key()` function BEFORE the existing handler, so existing code paths are not modified.
4. **Persistence**: `issue_base_prompt` uses `#[serde(default)]` so existing JSON deserializes cleanly.

## REQ→Phase→Pseudocode Traceability Matrix

| REQ ID | Description | Phases | Pseudocode Reference |
|--------|-------------|--------|---------------------|
| REQ-ISS-001 | Mode Entry and Exit | P03 (stub), P04 (TDD), P05 (impl), P09 (key stub), P10 (key TDD), P11 (key impl), P12 (UI stub), P14 (UI impl), P15 (integration) | component-001 lines 33-51 (enter/exit mode); component-003 lines 128-137 (scope change, reply prefill) |
| REQ-ISS-002 | Key Routing and Suppression | P03 (input.rs stub), P09 (key stub), P10 (key TDD), P11 (key impl), P15 (integration) | component-003 lines 01-38 (priority chain, suppression rules) |
| REQ-ISS-003 | Pane Focus and Navigation | P04 (TDD), P05 (impl), P10 (key TDD), P11 (key impl) | component-001 lines 52-82 (navigation/focus cycling); component-003 lines 39-72 (list/detail key handlers) |
| REQ-ISS-004 | Esc Precedence Chain | P04 (TDD), P05 (impl), P10 (key TDD), P11 (key impl), P15 (integration) | component-001 lines 115-127 (6-level Esc chain) |
| REQ-ISS-005 | Exit-Focus Restoration | P04 (TDD), P05 (impl), P15 (integration) | component-001 lines 41-51 (exit restore logic) |
| REQ-ISS-006 | Issue List Display and Sorting | P03 (domain types stub), P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl) | component-001 lines 83-96 (list loaded, selection); component-002 lines 09-25 (list_issues parsing/sorting) |
| REQ-ISS-007 | Pagination and Lazy Loading | P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P15 (integration) | component-001 lines 97-102 (page loaded append); component-002 lines 33-43 (comments pagination) |
| REQ-ISS-008 | Filtering and Search | P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P10 (key TDD), P11 (key impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl) | component-001 lines 22-29 (filter events), 158-165 (selection after filter); component-002 lines 09-25 (filter args); component-003 lines 112-127 (search/filter keys) |
| REQ-ISS-009 | Issue Detail and Comments | P03 (domain types stub), P07 (client TDD), P08 (client impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl) | component-002 lines 26-43 (detail/comments parsing) |
| REQ-ISS-010 | Inline Create/Edit | P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P10 (key TDD), P11 (key impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl), P15 (integration) | component-001 lines 129-157 (detail subfocus, inline state); component-003 lines 73-101 (inline key handler/submit), 138-141 (exclusivity guard) |
| REQ-ISS-011 | Send-to-Agent | P06 (client stub), P07 (client TDD), P08 (client impl), P10 (key TDD), P11 (key impl), P12 (UI stub), P14 (UI impl), P15 (integration) | component-002 lines 62-74 (build_send_payload); component-003 lines 102-111 (agent chooser keys) |
| REQ-ISS-012 | Repository Config `issue_base_prompt` | P03 (domain field stub), P04 (TDD), P05 (impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl), P15 (integration) | component-003 lines (implicit in payload—component-002 lines 62-74) |
| REQ-ISS-013 | Authentication and Error Handling | P03 (github stub), P06 (client stub), P07 (client TDD), P08 (client impl), P11 (key impl), P15 (integration) | component-002 lines 04-08 (auth), 75-82 (error enum) |
| REQ-ISS-014 | Empty States | P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl), P15 (integration) | component-001 lines 90-95 (empty list state) |
| REQ-ISS-NFR-001 | Responsiveness | P15 (integration), P16 (quality gate) | N/A (non-algorithmic, verified behaviorally) |
| REQ-ISS-NFR-002 | Reliability | P15 (integration), P16 (quality gate) | N/A (non-algorithmic, verified behaviorally) |
| REQ-ISS-NFR-003 | Maintainability | P00A (preflight), P01 (analysis), P16 (quality gate) | N/A (architectural constraint, verified structurally) |

## Analysis Artifacts Required by This Plan

- `analysis/domain-model.md`
- `analysis/pseudocode/component-001.md` (state + event reducer)
- `analysis/pseudocode/component-002.md` (GitHub client boundary)
- `analysis/pseudocode/component-003.md` (key routing + inline mutation + agent chooser)

## Codebase Integration Points (Verified Against Source Tree)

The following file paths are confirmed to exist in the source tree at plan creation time:

| Source File | Confirmed | Integration |
|-------------|-----------|-------------|
| `src/state/mod.rs` | [OK] | Contains `ScreenMode` (L229), `PaneFocus` (L237), `AppEvent` (L276), `AppState` (L246) |
| `src/domain/mod.rs` | [OK] | Contains `Repository` (L103), `RepositoryId`, `AgentId`, `Agent` |
| `src/input.rs` | [OK] | Contains `InputMode` enum (L9), `input_mode_for_state()` (L30), `route_search_key()` (L54) |
| `src/app_input.rs` | [OK] | Contains `dispatch_app_event()` (L359), `handle_normal_key_event()` (L858) |
| `src/persistence/mod.rs` | [OK] | Contains `State` struct with `repositories: Vec<Repository>` |
| `src/lib.rs` | [OK] | Module declarations; currently has `domain`, `input`, `logging`, `persistence`, `runtime`, `state`, `theme`, `ui` |
| `src/main.rs` | [OK] | Binary crate entry point, imports from `jefe::state`, uses `AppStateHandle` and `SharedContext` |
| `src/ui/screens/dashboard.rs` | [OK] | Dashboard layout rendering |
| `src/ui/screens/new_repository.rs` | [OK] | Repository create/edit form |
| `src/ui/components/sidebar.rs` | [OK] | Repository list pane |
| `src/ui/components/keybind_bar.rs` | [OK] | Keybinding display |
| `src/ui/components/mod.rs` | [OK] | UI component module declarations |
| `src/ui/screens/mod.rs` | [OK] | UI screen module declarations |
| `src/ui/modals/help.rs` | [OK] | Help modal content |

### New Files to Create

| New File | Purpose |
|----------|---------|
| `src/github/mod.rs` | GitHub client boundary (`GhClient`, `GhError`, response types) |
| `src/ui/screens/issues.rs` | Issues mode screen layout (three-pane) |
| `src/ui/components/issue_list.rs` | Issue list pane component |
| `src/ui/components/issue_detail.rs` | Issue detail pane (body, comments, inline controls) |
| `src/ui/components/filter_controls.rs` | Filter controls component |
| `src/ui/components/agent_chooser.rs` | Send-to-agent agent chooser overlay |

## Integration Contract

### Existing Callers
- `src/main.rs` — app bootstrap, event loop, terminal event dispatch; uses `SharedContext` (not `AppContext`)
- `src/app_input.rs` — key routing via `handle_normal_key_event()` (L858) and event dispatch via `dispatch_app_event()` (L359)
- `src/input.rs` — `input_mode_for_state()` resolution; currently returns `Normal`/`TerminalCapture`/`Help`/`Search`/`Form`/`Confirm`
- `src/state/mod.rs` — `AppState::apply()` event reducer, `ScreenMode` (`Dashboard`/`Split`), `PaneFocus` (`Repositories`/`Agents`/`Terminal`)
- `src/domain/mod.rs` — entity types: `Repository` { id, name, slug, base_dir, default_profile, remote, agent_ids }
- `src/persistence/mod.rs` — `State` struct serialization with `repositories: Vec<Repository>`, `agents: Vec<Agent>`
- `src/ui/screens/dashboard.rs` — dashboard layout rendering
- `src/ui/components/sidebar.rs` — repository list pane
- `src/ui/components/keybind_bar.rs` — keybinding display

### Key Dispatch Integration Map

This subsection names the exact existing functions and their verified line numbers that form the dispatch chain. Every implementation phase that touches key routing or event handling MUST integrate with these functions — not introduce parallel paths.

#### `handle_normal_key_event` — `src/app_input.rs` L858

```
pub fn handle_normal_key_event(key: KeyEvent, state: &AppState, ctx: &SharedContext) -> Vec<AppEvent>
```

- **Role**: Top-level entry point for all keyboard events when `InputMode::Normal` is active. Receives the raw `KeyEvent` and the current `AppState`, and returns a list of `AppEvent`s to dispatch.
- **Current behavior**: Checks `state.screen_mode` and routes to per-mode handlers. Currently handles `ScreenMode::Dashboard` and `ScreenMode::Split`.
- **Integration point for Issues Mode**: A new guard `if state.screen_mode == ScreenMode::DashboardIssues` is added **before** the existing `Dashboard`/`Split` routing. When matched, it calls the new `handle_issues_mode_key()` function and returns early. The existing routing is never reached when in issues mode, preserving all existing behavior exactly.
- **Verification**: `grep -n "handle_normal_key_event" src/app_input.rs` must show exactly one `pub fn` definition at L858. Any issues-mode routing added here must not alter the return path for `Dashboard` or `Split` modes.

#### `dispatch_app_event` — `src/app_input.rs` L359

```
pub fn dispatch_app_event(event: AppEvent, state: &AppState, ctx: &SharedContext) -> Vec<AppEvent>
```

- **Role**: Synchronous event dispatch function. Receives a single `AppEvent`, applies any side-effects (I/O, async task spawning), and returns zero or more follow-up events to re-dispatch.
- **Current behavior**: Matches on `AppEvent` variants to perform I/O (agent launch, file system ops) or emit chained events.
- **Integration point for Issues Mode**: New `AppEvent` variants (`EnterIssuesMode`, `IssueListLoaded`, `CommentCreated`, etc.) are added as new match arms in this function. The new arms call `GhClient` methods (synchronously or by spawning background tasks) and emit follow-up events. No existing arms are modified.
- **Verification**: `grep -n "dispatch_app_event" src/app_input.rs` must show exactly one `pub fn` definition at L359. New issues-mode arms must be additive (no modification of existing arms).

#### `AppState::apply` — `src/state/mod.rs` L561

```
pub fn apply(self, event: AppEvent) -> AppState
```

- **Role**: Pure state reducer. Takes the current `AppState` and an `AppEvent`, returns the next `AppState`. No I/O, no side effects. This is the single source of truth for all state transitions.
- **Current behavior**: Matches on `AppEvent` to compute next state. Currently handles all existing variants (navigation, agent lifecycle, form, persistence, etc.).
- **Integration point for Issues Mode**: New `AppEvent` variants for issues mode are handled by new match arms added to this function. The new arms update `state.issues_state` fields. The existing arms for non-issues events are untouched. The function signature and ownership model (consuming `self`, returning `AppState`) are unchanged.
- **Verification**: `grep -n "pub fn apply" src/state/mod.rs` must show exactly one definition at L561. All new issues-mode event arms must return a complete `AppState` (not partial/default). No existing arm may be removed or modified.

#### Full Dispatch Chain for a Key Press in Issues Mode

```
KeyEvent (from terminal)
  │
  ▼
handle_normal_key_event(key, state, ctx)          ← src/app_input.rs L858
  │  [guard: screen_mode == DashboardIssues]
  ▼
handle_issues_mode_key(key, state, ctx)            ← src/app_input.rs (NEW, P09/P11)
  │  [determines AppEvent(s) based on focus domain + key]
  ▼
dispatch_app_event(event, state, ctx)              ← src/app_input.rs L359
  │  [performs I/O, spawns tasks, emits follow-up events]
  ▼
AppState::apply(state, event)                      ← src/state/mod.rs L561
  │  [pure state transition; returns new AppState]
  ▼
New AppState (rendered by UI on next frame)
```

This chain must be complete and unbroken. Any issues-mode key that does not reach `AppState::apply` via `dispatch_app_event` is a wiring defect, not a test defect.

### Existing Code Replaced/Removed
- No code is removed; this is additive.
- `ScreenMode` enum extended with `DashboardIssues` variant (currently has `Dashboard`, `Split`).
- `PaneFocus` behavior extended when in issues mode (currently has `Repositories`, `Agents`, `Terminal`). **NOT modified** — issues mode uses separate `IssueFocus` enum.
- `InputMode` extended with issues-mode variants (currently has `Normal`, `TerminalCapture`, `Help`, `Search`, `Form`, `Confirm`).
- Key routing in `app_input.rs` extended: add issues-mode branch before/within `handle_normal_key_event()`.
- `Repository` domain type extended with `issue_base_prompt: String` field with `#[serde(default)]`.
- Repository form in `new_repository.rs` extended with `issue_base_prompt` multiline field.

### User Access Path
- `i` enters Issues Mode from dashboard.
- `a` or `Esc` (when no inner control active) exits Issues Mode.
- `Tab`/`Shift+Tab` cycles focus between repo list, issue list, issue detail.
- `Up/Down/PageUp/PageDown/Home/End` navigates issue list.
- `Enter` focuses selected issue detail.
- `f` opens filter controls; `/` focuses search input.
- `e` edits focused issue body or comment; `r` replies to focused comment.
- `S` opens send-to-agent chooser.
- Repository config screen gains `issue_base_prompt` field.

### Data/State Migration
- `issue_base_prompt` field added to `Repository` with `#[serde(default)]` for backward compatibility.
- Existing `state.json` files deserialize cleanly with empty `issue_base_prompt`.
- No schema version bump required (additive field with default).

### Backward Compatibility Acceptance Gate
- Existing `state.json` without `issue_base_prompt` MUST deserialize without error.
- Existing dashboard/agents mode keyboard flows MUST be unaffected.
- Existing agent lifecycle operations (Ctrl-d, Ctrl-k, l) MUST work outside Issues Mode.
- Existing split mode entry/exit MUST be unaffected outside Issues Mode.

### End-to-End Verification
- Issues mode entry/exit keyboard flow tests.
- Issue list loading/selection/pagination state tests.
- Filter/search composition tests.
- Inline editor/composer exclusivity tests

---

## Baseline-to-Target Enum Mapping

This section shows the exact current state of key enums as found in source (with verified line numbers), what new variants are added, and how existing behavior is preserved.

### `ScreenMode` — `src/state/mod.rs` L228–233

Current source (verified):

```rust
// L228
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreenMode {
    #[default]
    Dashboard,   // L231 — default variant; represents Agents Mode
    Split,       // L232 — split-view mode
}
```

After this plan, the enum gains one new variant. **No existing variant is renamed or removed.**

```rust
pub enum ScreenMode {
    Dashboard,        // PRESERVED — Agents Mode (existing default)
    Split,            // PRESERVED — split-view mode
    DashboardIssues,  // NEW — Issues Mode entry point
}
```

How `Dashboard` behavior is preserved when Issues Mode is inactive:

- `ScreenMode::Dashboard` remains the `#[default]` value.
- All existing `match screen_mode` arms covering `Dashboard` and `Split` continue to execute unchanged when the mode is either of those two values.
- The new `DashboardIssues` arm is handled by the new `handle_issues_mode_key()` dispatch path added in `app_input.rs` (before the existing handler), so no existing arm is disturbed.
- When `screen_mode == Dashboard`, the `PaneFocus` cycle (`Repositories → Agents → Terminal`) operates exactly as today.
- When `screen_mode == Dashboard`, the `s`/`S` → `EnterSplitMode` binding (app_input.rs L945) fires normally because the guard is `screen_mode == ScreenMode::Dashboard`.

### `PaneFocus` — `src/state/mod.rs` L236–241

Current source (verified):

```rust
// L236
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaneFocus {
    #[default]
    Repositories,  // L239 — repository list pane
    Agents,        // L240 — agent list pane
    Terminal,      // L241 — terminal pane
}
```

**`PaneFocus` is NOT modified by this plan.** Issues mode introduces a separate `IssueFocus` enum:

```rust
// NEW — added in src/state/mod.rs (issues module)
pub enum IssueFocus {
    RepoList,    // repo sidebar while in Issues Mode
    IssueList,   // issue list pane
    IssueDetail, // issue detail/comments pane
}
```

When `screen_mode == DashboardIssues`, the active focus domain is read from `AppState.issues_state.issue_focus: IssueFocus`, not from `AppState.pane_focus`. The `PaneFocus` value during Issues Mode is an implementation detail (held at its last Agents-mode value or restored on exit).

### `AppEvent` — `src/state/mod.rs` L276–346

Current variants (all PRESERVED — verified line range L276–346):

```
NavigateUp, NavigateDown, NavigateLeft, NavigateRight,
SelectRepository(usize), SelectAgent(usize), JumpToAgentByShortcut(u8),
CyclePaneFocus, ToggleTerminalFocus, ToggleHideIdleRepositories,
EnterSplitMode, ExitSplitMode,
EnterGrabMode, ExitGrabMode, GrabMoveUp, GrabMoveDown, SetSplitFilter(Option<RepositoryId>),
OpenHelp, OpenSearch, CloseModal, SubmitForm,
FormChar(char), FormBackspace, FormDelete, FormMoveCursorLeft, FormMoveCursorRight,
FormNextField, FormPrevField, FormToggleCheckbox,
OpenNewRepository, OpenEditRepository(RepositoryId), OpenDeleteRepository(RepositoryId),
OpenNewAgent(RepositoryId), OpenEditAgent(AgentId), OpenDeleteAgent(AgentId),
ToggleDeleteWorkDir,
KillAgent(AgentId), RelaunchAgent(AgentId), AgentStatusChanged(AgentId, AgentStatus),
PersistenceLoadSuccess, PersistenceLoadFailed(String),
PersistenceSaveSuccess, PersistenceSaveFailed(String),
SetTheme(String), ThemeResolveFailed(String),
Quit, ClearError, ClearWarning,
```

New variants to be **added** (not replacing anything):

```
EnterIssuesMode, ExitIssuesMode, RefocusIssueList,
IssuesNavigateUp, IssuesNavigateDown,
IssuesNavigatePageUp, IssuesNavigatePageDown,
IssuesNavigateHome, IssuesNavigateEnd,
IssuesEnter, IssuesCycleFocus, IssuesCycleFocusReverse,
IssueListLoaded { ... }, IssueListPageLoaded { ... }, IssueListLoadFailed { ... },
IssueDetailLoaded { ... }, IssueDetailLoadFailed { ... },
IssueCommentsPageLoaded { ... }, IssueCommentsPageFailed { ... },
OpenFilterControls, CloseFilterControls, ApplyFilter { ... }, ClearFilter,
FocusSearchInput, BlurSearchInput, ApplySearch { ... }, ClearSearch,
OpenNewCommentComposer, OpenReplyComposer { ... }, OpenInlineEditor { ... },
InlineChar(char), InlineBackspace, InlineSubmit, InlineCancelOrEsc,
CommentCreated { ... }, CommentCreateFailed { ... },
IssueBodyUpdated, CommentUpdated { ... }, MutationFailed { ... },
OpenAgentChooser, AgentChooserNavigateUp, AgentChooserNavigateDown,
AgentChooserConfirm, AgentChooserCancel,
SendToAgentCompleted, SendToAgentFailed { ... },
```

---

## Terminology Glossary

This glossary maps every plan-level name used in the specification and pseudocode to its concrete Rust identifier (current or planned).

| Plan / Spec Term | Rust Identifier | Status | File | Notes |
|------------------|----------------|--------|------|-------|
| `dashboard_agents` | `ScreenMode::Dashboard` | **Existing** | `src/state/mod.rs` L231 | The current default mode. NOT renamed. |
| `dashboard_issues` | `ScreenMode::DashboardIssues` | **New** | `src/state/mod.rs` | New variant added to `ScreenMode`. |
| `split_mode` | `ScreenMode::Split` | **Existing** | `src/state/mod.rs` L232 | Unchanged. |
| Agents Mode | `ScreenMode::Dashboard` + `PaneFocus::{Repositories,Agents,Terminal}` | **Existing** | `src/state/mod.rs` L229–241 | The current dashboard with agent management. |
| Issues Mode | `ScreenMode::DashboardIssues` + `IssueFocus::{RepoList,IssueList,IssueDetail}` | **New** | `src/state/mod.rs` | New mode; uses separate focus enum. |
| `repo_list` focus | `IssueFocus::RepoList` | **New** | `src/state/mod.rs` | Focus on the repository sidebar within Issues Mode. |
| `issue_list` focus | `IssueFocus::IssueList` | **New** | `src/state/mod.rs` | Focus on the issue list pane. |
| `issue_detail` focus | `IssueFocus::IssueDetail` | **New** | `src/state/mod.rs` | Focus on the detail/comments pane. |
| focus domain | `AppState.issues_state.issue_focus: IssueFocus` | **New** | `src/state/mod.rs` | Active variant determines key dispatch branch. |
| inline control | `InlineState::{Composer, Editor}` | **New** | `src/state/mod.rs` (issues submodule) | At most one active; exclusivity invariant enforced. |
| scope change | Repository selection change while `screen_mode == DashboardIssues` | Behavioral | `src/app_input.rs` / `src/state/mod.rs` | Triggers `handle_repo_scope_change_in_issues_mode` (component-003 L128–135). |
| `issue_base_prompt` | `Repository::issue_base_prompt: String` | **New** | `src/domain/mod.rs` L103+ | New field on existing struct; `#[serde(default)]` for compat. |
| `IssuesState` | `AppState.issues_state: IssuesState` | **New** | `src/state/mod.rs` | Aggregate struct for all issues-mode runtime state. |
| `GhClient` | `crate::github::GhClient` | **New** | `src/github/mod.rs` | `gh` CLI wrapper; synchronous; isolated boundary. |
| `GhError` | `crate::github::GhError` | **New** | `src/github/mod.rs` | Error enum (component-002 L75–82). |
| `SendPayload` | `crate::github::SendPayload` | **New** | `src/github/mod.rs` | Built by `build_send_payload` (component-002 L62–74). |
| `dispatch_issues_event` | `AppState::apply()` issues arm | **New** | `src/state/mod.rs` | Issues events dispatched through existing `apply()` at L561. |
| `route_issues_mode_key` | `handle_issues_mode_key()` | **New** | `src/app_input.rs` | New function; called from `handle_normal_key_event()` when `screen_mode == DashboardIssues`. |
| `handle_normal_key_event` | `pub fn handle_normal_key_event(...)` | **Existing** | `src/app_input.rs` L858 | Entry point for normal-mode key dispatch; gains issues branch. |
| `dispatch_app_event` | `pub fn dispatch_app_event(...)` | **Existing** | `src/app_input.rs` L359 | Event dispatch entry; unchanged in signature. |
| `input_mode_for_state` | `pub fn input_mode_for_state(state: &AppState) -> InputMode` | **Existing** | `src/input.rs` L30 | Gains issues-mode detection before existing `Normal` fallback. |

---

## REQ-to-Phase-to-Pseudocode Traceability Matrix

Each row covers one requirement from `specification.md`. Phases are listed in implementation order. Pseudocode line ranges are from the `analysis/pseudocode/` artifacts.

| REQ ID | Requirement Title | Phase(s) | Pseudocode Component | Pseudocode Lines |
|--------|-------------------|----------|---------------------|-----------------|
| REQ-ISS-001 | Mode Entry and Exit | P03 (state stub), P04 (state TDD), P05 (state impl), P09 (key stub), P10 (key TDD), P11 (key impl), P12 (UI stub), P14 (UI impl), P15 (integration) | component-001 | Lines 33–51 (enter/exit mode state transitions) |
| REQ-ISS-001 | Mode Entry and Exit (scope change) | P11 (key impl), P15 (integration) | component-003 | Lines 128–137 (scope change handler, reply prefill) |
| REQ-ISS-002 | Key Routing and Suppression | P03 (input.rs stub), P09 (key stub), P10 (key TDD), P11 (key impl), P15 (integration) | component-003 | Lines 01–38 (priority chain, suppression rules) |
| REQ-ISS-003 | Pane Focus and Navigation | P04 (TDD), P05 (impl), P10 (key TDD), P11 (key impl) | component-001 | Lines 52–82 (navigate up/down, focus cycling) |
| REQ-ISS-003 | Pane Focus and Navigation (key handlers) | P10 (key TDD), P11 (key impl) | component-003 | Lines 39–72 (issue list and detail key handlers) |
| REQ-ISS-004 | Esc Precedence Chain | P04 (TDD), P05 (impl), P10 (key TDD), P11 (key impl), P15 (integration) | component-001 | Lines 115–127 (6-level Esc chain) |
| REQ-ISS-005 | Exit-Focus Restoration | P04 (TDD), P05 (impl), P15 (integration) | component-001 | Lines 41–51 (exit_issues_mode restore logic) |
| REQ-ISS-006 | Issue List Display and Sorting | P03 (domain types stub), P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl) | component-001 | Lines 83–96 (list loaded, selection, empty state) |
| REQ-ISS-006 | Issue List Display and Sorting (gh client) | P07 (client TDD), P08 (client impl) | component-002 | Lines 09–25 (list_issues, filter args, sort, pagination) |
| REQ-ISS-007 | Pagination and Lazy Loading (list) | P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P15 (integration) | component-001 | Lines 97–102 (page loaded append) |
| REQ-ISS-007 | Pagination and Lazy Loading (comments) | P07 (client TDD), P08 (client impl) | component-002 | Lines 33–43 (list_comments pagination) |
| REQ-ISS-008 | Filtering and Search (state) | P04 (TDD), P05 (impl) | component-001 | Lines 22–29 (filter/search events), 158–165 (selection after filter) |
| REQ-ISS-008 | Filtering and Search (gh client) | P07 (client TDD), P08 (client impl) | component-002 | Lines 09–25 (filter args to gh CLI) |
| REQ-ISS-008 | Filtering and Search (key handlers) | P10 (key TDD), P11 (key impl) | component-003 | Lines 112–127 (search/filter key handlers) |
| REQ-ISS-009 | Issue Detail and Comments | P03 (domain types stub), P07 (client TDD), P08 (client impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl) | component-002 | Lines 26–43 (get_issue_detail, list_comments parsing) |
| REQ-ISS-010 | Inline Create/Edit (state) | P04 (TDD), P05 (impl) | component-001 | Lines 129–157 (detail subfocus, inline state machines) |
| REQ-ISS-010 | Inline Create/Edit (key handlers + submit) | P10 (key TDD), P11 (key impl) | component-003 | Lines 73–101 (inline key handler, handle_inline_submit) |
| REQ-ISS-010 | Inline Create/Edit (exclusivity guard) | P04 (TDD), P05 (impl) | component-003 | Lines 138–141 (exclusivity_guard) |
| REQ-ISS-011 | Send-to-Agent (payload) | P06 (client stub), P07 (client TDD), P08 (client impl) | component-002 | Lines 62–74 (build_send_payload) |
| REQ-ISS-011 | Send-to-Agent (key handlers) | P10 (key TDD), P11 (key impl) | component-003 | Lines 102–111 (handle_agent_chooser_key) |
| REQ-ISS-012 | Repository Config `issue_base_prompt` | P03 (domain field stub), P04 (TDD), P05 (impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl), P15 (integration) | component-002 | Lines 62–74 (payload includes issue_base_prompt) |
| REQ-ISS-013 | Authentication and Error Handling (auth) | P03 (github stub), P06 (client stub), P07 (client TDD), P08 (client impl) | component-002 | Lines 04–08 (check_auth) |
| REQ-ISS-013 | Authentication and Error Handling (errors) | P07 (client TDD), P08 (client impl), P15 (integration) | component-002 | Lines 75–82 (GhError enum) |
| REQ-ISS-014 | Empty States | P04 (TDD), P05 (impl), P07 (client TDD), P08 (client impl), P12 (UI stub), P13 (UI TDD), P14 (UI impl), P15 (integration) | component-001 | Lines 90–95 (empty list/detail empty state handling) |
| REQ-ISS-NFR-001 | Responsiveness | P15 (integration), P16 (quality gate) | N/A | Non-algorithmic; verified behaviorally in integration tests |
| REQ-ISS-NFR-002 | Reliability | P15 (integration), P16 (quality gate) | N/A | Non-algorithmic; verified behaviorally (error non-crash) |
| REQ-ISS-NFR-003 | Maintainability | P00A (preflight), P01 (analysis), P16 (quality gate) | N/A | Architectural constraint; verified structurally via boundary isolation |
