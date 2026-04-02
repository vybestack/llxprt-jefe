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
5. Coordination follows `dev-docs/COORDINATING.md` exactly: no skipped phases, no batching, one worker + one verifier per phase, fail-blocked progression only

## Global Coordination Protocol (COORDINATING.md Binding)

This plan is executable only if coordinator behavior satisfies `dev-docs/COORDINATING.md`.

- **Strict order only**: execute `P00A, P01, P01A, ... P16, P16A` in exact numeric order.
- **One phase at a time**: never run multiple implementation phases in one worker launch.
- **Atomic verification**: each verifier must emit `Phase NN: PASS` or `Phase NN: FAIL` with concrete remediation items.
- **Failure loop mandatory**: on FAIL, perform same-phase remediation and re-verify; do not continue.
- **Prerequisite gate**: before each phase, verify:
  1. `project-plans/issue15/.completed/P(N-1).md` exists,
  2. corresponding verification phase PASS exists,
  3. required artifacts listed by phase are present.
- **Todo discipline**: create/maintain todo entries for every phase and verification phase before execution starts.

### Verifier Output Contract (Behavioral, Not Checklist-Only)

Every verification phase MUST include evidence from real code behavior, not only checklist ticks.
Verifier must provide:

1. **Structural checks** (files, tests, markers, compilation)
2. **Behavioral code-reading checks** with cited file/line evidence, proving the expected runtime path exists.
3. **Runtime-path checks** showing key flows are reachable from actual dispatch chain, e.g.:
   - key event source -> key routing -> `AppEvent` emission -> `dispatch_app_event` side effect -> reducer (`AppState::apply`) -> render condition in UI.
4. **Contradiction scan**: identify places where tests pass but production path is missing (example: reducer handles `IssueListLoaded` but no side-effect emits it).
5. **Atomic verdict**:
   - `Phase NN: PASS` only when structural + behavioral checks both pass.
   - `Phase NN: FAIL` with exact remediation items and blocking reason.

A verifier output that only states "all checks passed" without cited behavioral evidence is non-compliant and must be treated as FAIL.

## Mockup-Driven Layout Contract (Issue #16)

Issue #16 mockups are normative for UI placement and composition checks in P14/P14A/P15/P15A/P16/P16A.
Source: `gh issue view 16` and supplemental comment link in issue body.

### Required layout measurements and placement checkpoints

- **Baseline dashboard shell (preserved)**
  - Repositories sidebar fixed width `22u32` (left, full height)
  - Middle column split: top `25%` agent list, bottom `75%` terminal
  - Preview pane fixed width `36u32` (right, full height)

- **Issues mode shell (two-column layout)**
  - Repositories sidebar remains visible, left, full height, same fixed width (`22u32`) — identical placement to baseline dashboard
  - Issues workspace occupies remaining width to the right (single right column, NOT a third column)
  - Issues workspace is vertically split into:
    1. filter/search band (top),
    2. issue list region (top portion, selection drives detail),
    3. unified detail+comments view (bottom portion — one scrollable view containing: issue metadata, body, comments timeline, new comment field, inline controls)
  - Issue detail and comments are **one unified scrollable view**, not separate panes or regions
  - Layout matches mockup focus states (list-focused and detail-focused) from Issue #16

- **Placement acceptance checks** (must be explicitly verified):
  1. repo pane is persistent in issues mode, full screen height, and responds to focus/nav,
  2. layout is two columns (repos sidebar + issues workspace) — NOT three columns,
  3. issue list and unified detail view are both present within the issues workspace with deterministic sizing,
  4. detail+comments is one scrollable view with a single scroll indicator (not split into separate regions),
  5. send-to-agent panel is anchored to issues workspace (not detached global placement),
  6. inline error surfaces are visible in-pane where failures occur,
  7. keybind text reflects issues-mode bindings and suppression rules.

Any deviation from the #16 mockup contract requires explicit documented rationale and user sign-off.

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

### `ScreenMode` (src/state/types.rs L221-225)

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

### `PaneFocus` (src/state/types.rs L229-234)

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

### `AppEvent` (src/state/types.rs L268-338)

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
  IssuesScrollDetailUp, IssuesScrollDetailDown,
  IssueDetailSubfocusNext, IssueDetailSubfocusPrev,
  OpenFilterControls, CloseFilterControls, ApplyFilter, ClearFilter,
  FocusSearchInput, BlurSearchInput, SetSearchQuery { ... }, ApplySearch, ClearSearch,
  UpdateDraftFilter { ... },
  OpenNewCommentComposer, OpenReplyComposer { ... }, OpenInlineEditor { ... },
  InlineChar(char), InlineBackspace, InlineSubmit, InlineCancelOrEsc,
  CommentCreated { ... }, CommentCreateFailed { ... },
  IssueBodyUpdated { ... }, CommentUpdated { ... }, MutationFailed { ... },
  OpenAgentChooser, AgentChooserNavigateUp, AgentChooserNavigateDown,
  AgentChooserConfirm, AgentChooserCancel,
  SendToAgentCompleted, SendToAgentFailed { ... },
```

### `AppState` (src/state/types.rs L238-264)

Existing fields are **PRESERVED**. New field added:

```
NEW field (added):
  pub issues_state: IssuesState,   ← aggregates all issues-mode state
```

### `ModalState` (src/state/types.rs L161-217)

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
3. **Key routing**: Existing `handle_normal_key_event()` (L61 in src/app_input/normal.rs) handles `ScreenMode::Dashboard` and `ScreenMode::Split`. A new branch dispatches `ScreenMode::DashboardIssues` to a new `handle_issues_mode_key()` function BEFORE the existing handler, so existing code paths are not modified.
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

### Structural Note: Module Split Pattern

The codebase uses a split-module pattern. Implementation must follow these conventions:

- **Type definitions** (`ScreenMode`, `PaneFocus`, `AppEvent`, `AppState`, `ModalState`, form structs) → `src/state/types.rs` (re-exported via `pub use types::*` in `src/state/mod.rs`)
- **`AppState::apply()` match arms** → `src/state/mod.rs` (L233)
- **Form operations** → `src/state/form_ops.rs`
- **Key routing** → `src/app_input/` directory:
  - `mod.rs` — dispatch, shared utilities, module declarations
  - `normal.rs` — normal-mode key handler (`handle_normal_key_event` at L61)
  - `preflight.rs` — preflight prompt handler
  - **NEW: `issues.rs`** — issues-mode key handler (to be created, following the existing submodule pattern)
- **App initialization** → `src/app_init.rs` (extracted from `main.rs`)
- **`AppContext`** → `src/main.rs` L99 (struct definition); init in `src/app_init.rs`

The following file paths are confirmed to exist in the source tree at plan creation time:

| Source File | Confirmed | Integration |
|-------------|-----------|-------------|
| `src/state/mod.rs` | [OK] | Contains `AppState::apply()` (L233); re-exports all types from `types.rs` |
| `src/domain/mod.rs` | [OK] | Contains `Repository` (L196), `RepositoryId`, `AgentId`, `Agent` |
| `src/input.rs` | [OK] | Contains `InputMode` enum (L9), `input_mode_for_state()` (L30), `route_search_key()` (L54) |
| `src/app_input/mod.rs` | [OK] | Contains `dispatch_app_event()` (L359); delegates to `normal.rs` and `preflight.rs` |
| `src/app_input/normal.rs` | [OK] | Contains `handle_normal_key_event()` (L61) — all normal-mode key bindings |
| `src/app_input/preflight.rs` | [OK] | Contains `handle_preflight_prompt_enter()` (L8) |
| `src/state/types.rs` | [OK] | Contains `ScreenMode` (L221), `PaneFocus` (L229), `AppEvent` (L268), `AppState` (L238), `ModalState` (L161) — re-exported via `src/state/mod.rs` |
| `src/state/form_ops.rs` | [OK] | Contains form field input handling (951 lines) — `RepositoryFormFields`/cursor ops |
| `src/state/util.rs` | [OK] | Contains state utility functions |
| `src/app_init.rs` | [OK] | App initialization logic extracted from main.rs |
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

| New File | Purpose | Created In |
|----------|---------|------------|
| `src/github/mod.rs` | GitHub client boundary (`GhClient`, `GhError`, response types) | P03 (skeleton), P06 (full signatures), P08 (implementation) |
| `src/app_input/issues.rs` | Issues-mode key handler (`handle_issues_mode_key()` and sub-handlers), declared as `mod issues;` in `src/app_input/mod.rs` | P09 (stub), P11 (implementation) |
| `src/ui/screens/issues.rs` | Issues mode screen layout (two-column: repos sidebar + issues workspace) | P12 (stub), P14 (implementation) |
| `src/ui/components/issue_list.rs` | Issue list pane component | P12 (stub), P14 (implementation) |
| `src/ui/components/issue_detail.rs` | Unified issue detail+comments scrollable view (metadata, body, comments timeline, inline controls) | P12 (stub), P14 (implementation) |
| `src/ui/components/filter_controls.rs` | Filter controls component | P12 (stub), P14 (implementation) |
| `src/ui/components/agent_chooser.rs` | Send-to-agent agent chooser overlay | P12 (stub), P14 (implementation) |

## Integration Contract

### Existing Callers
- `src/main.rs` — app bootstrap, event loop, terminal event dispatch; uses `SharedContext` as the runtime context object
- `src/app_input/normal.rs` — key routing via `handle_normal_key_event()` (L61)
- `src/app_input/mod.rs` — event dispatch via `dispatch_app_event()` (L359)
- `src/input.rs` — `input_mode_for_state()` resolution; currently returns `Normal`/`TerminalCapture`/`Help`/`Search`/`Form`/`Confirm`
- `src/state/mod.rs` — `AppState::apply()` event reducer (L233); types re-exported from `src/state/types.rs`
- `src/domain/mod.rs` — entity types: `Repository` { id, name, slug, base_dir, default_profile, remote, agent_ids }
- `src/persistence/mod.rs` — `State` struct serialization with `repositories: Vec<Repository>`, `agents: Vec<Agent>`
- `src/ui/screens/dashboard.rs` — dashboard layout rendering
- `src/ui/components/sidebar.rs` — repository list pane
- `src/ui/components/keybind_bar.rs` — keybinding display

### Key Dispatch Integration Map

This subsection names the exact existing functions and their verified line numbers that form the dispatch chain. Every implementation phase that touches key routing or event handling MUST integrate with these functions — not introduce parallel paths.

#### `handle_normal_key_event` — `src/app_input/normal.rs` L61

```
pub fn handle_normal_key_event(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> Option<AppEvent>
```

- **Role**: Top-level entry point for all keyboard events when `InputMode::Normal` is active. Takes a mutable `AppStateHandle`, a quit handle, the shared context, the raw `KeyEvent`, and the current `ScreenMode`. Returns `Option<AppEvent>` — `Some(event)` to dispatch, or `None` when the key was consumed without emitting an event (e.g., direct state mutation for focus changes).
- **Current behavior**: Checks `screen_mode` and routes to per-mode handlers. Currently handles `ScreenMode::Dashboard` and `ScreenMode::Split`.
- **Integration point for Issues Mode**: A new guard `if screen_mode == ScreenMode::DashboardIssues` is added **before** the existing `Dashboard`/`Split` routing. When matched, it calls the new `handle_issues_mode_key()` function and returns early. The existing routing is never reached when in issues mode, preserving all existing behavior exactly.
- **Module pattern**: Following the existing split (`normal.rs`, `preflight.rs`), the new issues mode handler should live in a new `src/app_input/issues.rs` submodule, declared in `src/app_input/mod.rs`.
- **Verification**: `grep -n "handle_normal_key_event" src/app_input/normal.rs` must show at L61. Any issues-mode routing added here must not alter the return path for `Dashboard` or `Split` modes.

#### `dispatch_app_event` — `src/app_input/mod.rs` L359

```
pub fn dispatch_app_event(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
```

- **Role**: Synchronous event dispatch function. Takes a mutable `AppStateHandle`, the shared context, and a single `AppEvent`. Applies side-effects (I/O, async task spawning) and mutates state directly. Returns nothing (`()`) — follow-up state changes are applied internally via `app_state.write()` and `AppState::apply()`.
- **Current behavior**: Matches on `AppEvent` variants to perform I/O (agent launch, file system ops) and apply state transitions.
- **Integration point for Issues Mode**: New `AppEvent` variants (`EnterIssuesMode`, `IssueListLoaded`, `CommentCreated`, etc.) are added as new match arms in this function. The new arms call `GhClient` methods (synchronously or by spawning background tasks) and apply results to state. No existing arms are modified.
- **Verification**: `grep -n "dispatch_app_event" src/app_input/mod.rs` must show exactly one `pub fn` definition at L359. New issues-mode arms must be additive (no modification of existing arms).

#### `AppState::apply` — `src/state/mod.rs` L233

```rust
pub fn apply(mut self, event: AppEvent) -> Self {
```

- **Role**: Pure state reducer. Takes the current `AppState` and an `AppEvent`, returns the next `AppState`. No I/O, no side effects. This is the single source of truth for all state transitions.
- **Current behavior**: Matches on `AppEvent` to compute next state. Currently handles all existing variants (navigation, agent lifecycle, form, persistence, etc.).
- **Integration point for Issues Mode**: New `AppEvent` variants for issues mode are handled by new match arms added to this function. The new arms update `state.issues_state` fields. The existing arms for non-issues events are untouched. The function signature and ownership model (consuming `self`, returning `AppState`) are unchanged.
- **Verification**: `grep -n "pub fn apply" src/state/mod.rs` must show exactly one definition at L233. All new issues-mode event arms must return a complete `AppState` (not partial/default). No existing arm may be removed or modified.

#### Full Dispatch Chain for a Key Press in Issues Mode

```
KeyEvent (from terminal)
  │
  ▼
handle_normal_key_event(app_state, should_quit, ctx, key_event, screen_mode) — L61
  │  [guard: screen_mode == DashboardIssues]
  │  returns Option<AppEvent>
  ▼
handle_issues_mode_key(app_state, ctx, key_event)  ← src/app_input/issues.rs (NEW, P09/P11)
  │  [determines AppEvent based on focus domain + key]
  │  returns Option<AppEvent>
  ▼
dispatch_app_event(app_state, ctx, evt)            ← src/app_input/mod.rs L359
  │  [performs I/O, spawns tasks; mutates state directly]
  │  calls app_state.write() and AppState::apply() internally
  ▼
AppState::apply(self, event) -> Self               ← src/state/mod.rs L233
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
- Key routing in `src/app_input/` extended: add issues-mode branch before/within `handle_normal_key_event()`.
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

### `ScreenMode` — `src/state/types.rs` L221–225

Current source (verified):

```rust
// L221 (in src/state/types.rs)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreenMode {
    #[default]
    Dashboard,   // L223 — default variant; represents Agents Mode
    Split,       // L224 — split-view mode
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
- The new `DashboardIssues` arm is handled by the new `handle_issues_mode_key()` function in `src/app_input/issues.rs` (called from `mod.rs` before the existing handler via `mod issues;` declaration), so no existing arm is disturbed.
- When `screen_mode == Dashboard`, the `PaneFocus` cycle (`Repositories → Agents → Terminal`) operates exactly as today.
- When `screen_mode == Dashboard`, the `s`/`S` → `EnterSplitMode` binding (src/app_input/normal.rs L148) fires normally because the guard is `screen_mode == ScreenMode::Dashboard`.

### `PaneFocus` — `src/state/types.rs` L229–234

Current source (verified):

```rust
// L229 (in src/state/types.rs)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PaneFocus {
    #[default]
    Repositories,  // L231 — repository list pane
    Agents,        // L232 — agent list pane
    Terminal,      // L233 — terminal pane
}
```

**`PaneFocus` is NOT modified by this plan.** Issues mode introduces a separate `IssueFocus` enum:

```rust
// NEW — added in src/state/types.rs (re-exported via src/state/mod.rs)
pub enum IssueFocus {
    RepoList,    // repo sidebar while in Issues Mode
    IssueList,   // issue list pane
    IssueDetail, // issue detail/comments pane
}
```

When `screen_mode == DashboardIssues`, the active focus domain is read from `AppState.issues_state.issue_focus: IssueFocus`, not from `AppState.pane_focus`. The `PaneFocus` value during Issues Mode is an implementation detail (held at its last Agents-mode value or restored on exit).

### `AppEvent` — `src/state/types.rs` L268–346

Current variants (all PRESERVED — verified line range L268–338):

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
IssuesScrollDetailUp, IssuesScrollDetailDown,
IssueDetailSubfocusNext, IssueDetailSubfocusPrev,
IssueListLoaded { ... }, IssueListPageLoaded { ... }, IssueListLoadFailed { ... },
IssueDetailLoaded { ... }, IssueDetailLoadFailed { ... },
IssueCommentsPageLoaded { ... }, IssueCommentsPageFailed { ... },
OpenFilterControls, CloseFilterControls, ApplyFilter, ClearFilter,
FocusSearchInput, BlurSearchInput, SetSearchQuery { ... }, ApplySearch, ClearSearch,
UpdateDraftFilter { ... },
OpenNewCommentComposer, OpenReplyComposer { ... }, OpenInlineEditor { ... },
InlineChar(char), InlineBackspace, InlineSubmit, InlineCancelOrEsc,
CommentCreated { ... }, CommentCreateFailed { ... },
IssueBodyUpdated { ... }, CommentUpdated { ... }, MutationFailed { ... },
OpenAgentChooser, AgentChooserNavigateUp, AgentChooserNavigateDown,
AgentChooserConfirm, AgentChooserCancel,
SendToAgentCompleted, SendToAgentFailed { ... },
```

---

## Terminology Glossary

This glossary maps every plan-level name used in the specification and pseudocode to its concrete Rust identifier (current or planned).

| Plan / Spec Term | Rust Identifier | Status | File | Notes |
|------------------|----------------|--------|------|-------|
| `dashboard_agents` | `ScreenMode::Dashboard` | **Existing** | `src/state/types.rs` L223 | The current default mode. NOT renamed. |
| `dashboard_issues` | `ScreenMode::DashboardIssues` | **New** | `src/state/types.rs` | New variant added to `ScreenMode` (re-exported via mod.rs). |
| `split_mode` | `ScreenMode::Split` | **Existing** | `src/state/types.rs` L224 | Unchanged. |
| Agents Mode | `ScreenMode::Dashboard` + `PaneFocus::{Repositories,Agents,Terminal}` | **Existing** | `src/state/types.rs` L221–234 | The current dashboard with agent management. |
| Issues Mode | `ScreenMode::DashboardIssues` + `IssueFocus::{RepoList,IssueList,IssueDetail}` | **New** | `src/state/types.rs` | New mode; uses separate focus enum. |
| `repo_list` focus | `IssueFocus::RepoList` | **New** | `src/state/types.rs` | Focus on the repository sidebar within Issues Mode. |
| `issue_list` focus | `IssueFocus::IssueList` | **New** | `src/state/types.rs` | Focus on the issue list pane. |
| `issue_detail` focus | `IssueFocus::IssueDetail` | **New** | `src/state/types.rs` | Focus on the detail/comments pane. |
| focus domain | `AppState.issues_state.issue_focus: IssueFocus` | **New** | `src/state/types.rs` | Active variant determines key dispatch branch. |
| inline control | `InlineState::{Composer, Editor}` | **New** | `src/state/types.rs` | At most one active; exclusivity invariant enforced. |
| scope change | Repository selection change while `screen_mode == DashboardIssues` | Behavioral | `src/app_input/issues.rs` / `src/state/mod.rs` | Triggers `handle_repo_scope_change_in_issues_mode` (component-003 L128–135). |
| `issue_base_prompt` | `Repository::issue_base_prompt: String` | **New** | `src/domain/mod.rs` L196+ | New field on existing struct; `#[serde(default)]` for compat. |
| `IssuesState` | `AppState.issues_state: IssuesState` | **New** | `src/state/types.rs` | Aggregate struct for all issues-mode runtime state. |
| `GhClient` | `crate::github::GhClient` | **New** | `src/github/mod.rs` | `gh` CLI wrapper; synchronous; isolated boundary. |
| `GhError` | `crate::github::GhError` | **New** | `src/github/mod.rs` | Error enum (component-002 L75–82). |
| `SendPayload` | `crate::github::SendPayload` | **New** | `src/github/mod.rs` | Built by `build_send_payload` (component-002 L62–74). |
| `dispatch_issues_event` | `AppState::apply()` — L233. |
| `route_issues_mode_key` | `handle_issues_mode_key()` | **New** | `src/app_input/issues.rs` | New function in `issues.rs` submodule (declared as `mod issues;` in `mod.rs`); called from `handle_normal_key_event()` when `screen_mode == DashboardIssues`. |
| `handle_normal_key_event` — L61 | Entry point for normal-mode key dispatch; gains issues branch. |
| `dispatch_app_event` | `pub fn dispatch_app_event(...)` | **Existing** | `src/app_input/mod.rs` L359 | Event dispatch entry; unchanged in signature. |
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
 via boundary isolation |
d structurally via boundary isolation |
 via boundary isolation |
