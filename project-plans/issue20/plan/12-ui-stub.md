# Phase 12 — UI Stub

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P12
- **Prerequisites:** `.completed/P11A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Add the PR-Mode UI surface as compiling stubs: the `PullRequestsScreen`, the `PrList`, `PrDetailView`,
`PrFilterControls` components, layout helpers, and the `build_screen_element` arm — rendering a
minimal skeleton. UI renders/emits intent only (no I/O, no AppState mutation).

**Single-owner file creation (finding #2):** P12 is the SOLE creator of every `src/ui` PR file
(`screens/pull_requests.rs`, `components/pr_list.rs`, `pr_detail.rs`, `pr_filter_controls.rs`).
P03 created NONE of these; P03 only left a benign placeholder arm in `build_screen_element`. P12
REPLACES that placeholder arm with the real `PullRequestsScreen` wiring (it MODIFIES the existing
`src/ui/orchestration.rs`, it does not re-create P03 files).

**Selection-follow viewport helpers live in `src/layout.rs`, NOT in the UI layer (finding #2):**
The PURE selection-follow helpers `list_first_visible_index`/`list_visible_window` are NOT a UI
file. Because the STATE reducers (P05 `navigate_pr_list_*`) consume them, they MUST live in a
state-neutral shared leaf module. They are placed in `src/layout.rs` (already the shared geometry
module imported by both `state` and `ui` — see `src/layout.rs` L184-193; the pseudocode endorses
"pure fns in layout.rs" at component-001 line 180). They are STUBBED in P03, RED-tested in P04, and
IMPLEMENTED in P05 — BEFORE P12 consumes them. P12 therefore does NOT create a
`src/ui/components/list_viewport.rs` file; `pr_list.rs` imports the helpers from `crate::layout`.
(This keeps the state→ui boundary intact: state never imports ui.)

**TOTAL-STUB rule (NO `todo!()`/`unimplemented!()` ANYWHERE — findings #1 & #4):** `Cargo.toml`
`[lints.clippy]` DENIES `todo` (L63) and `unimplemented` (L64); clippy fires on their mere PRESENCE
regardless of reachability, and this stub phase requires `cargo clippy ... -- -D warnings` to PASS.
Therefore NO `todo!()`/`unimplemented!()` may appear in any render body or component prop handling —
they must render benign empty/skeleton element trees and return safe defaults. There is no permitted
`todo!()` anywhere in P12's files.

## Requirements Implemented (Expanded)

### REQ-PR-006 list, REQ-PR-009 detail, REQ-PR-008 filter, REQ-PR-014 empty, NFR-003 maintainability
- **Behavior contract:** GIVEN the state/dispatch layers, WHEN P12 lands, THEN
  `ScreenMode::DashboardPullRequests` renders `PullRequestsScreen` with sidebar + list + detail
  regions; components compile with prop signatures matching the mockup measurements.
- **Why it matters:** Establishes the render surface for the UI TDD phase.

## Implementation Tasks

### `src/layout.rs`
- Add PR layout constants/helpers WITHOUT duplicating existing ones (#37/#39 dup-const guard):
  `PRS_SIDEBAR_WIDTH = LEFT_COL_WIDTH`, `PR_DETAIL_HEADER_ROWS` (incl review+check summary rows),
  `prs_pane_rows(term_rows, error_visible, filter_controls_open) -> (usize, usize)`,
  `prs_detail_pane_rows`, `prs_detail_viewport_rows`, `pr_list_content_width`. Reuse shared
  constants (`OUTER_BARS_HEIGHT`, `LEFT_COL_WIDTH`).

> NOTE (finding #2): the selection-follow helpers `list_first_visible_index`/`list_visible_window`
> are NOT created here — they live in `src/layout.rs` (stub P03 → RED P04 → impl P05;
> component-001 lines 182-196). There is NO `src/ui/components/list_viewport.rs` file in this plan.
> `pr_list.rs` imports the helpers from `crate::layout`.

### Files to create (stub components — render minimal element trees, no `todo!()` in render)
- `src/ui/components/pr_list.rs` — `PrListProps { pull_requests, selected_index, list_scroll_offset,
  list_pane_rows, focused, loading, layout }`; renders ONLY the visible window via the
  `crate::layout` selection-follow helpers (`list_first_visible_index`/`list_visible_window`,
  component-001 lines 182-196); selection-following + truncation (#54/#55) — wired in P14, skeleton
  here. Must NOT use `ScrollableText` for list rows.
- `src/ui/components/pr_detail.rs` — `PrDetailViewProps { detail, subfocus, scroll_offset,
  viewport_rows, comments_loading, focused, inline_state }` — unified scrollable detail (metadata,
  body, review summary, check summary, comments, composer).
- `src/ui/components/pr_filter_controls.rs` — `PrFilterControlsProps { draft_filter, field_index,
  draft_labels_text }`.
- `src/ui/screens/pull_requests.rs` — `PullRequestsScreen` (mirror `IssuesScreen`): StatusBar +
  Row{ Sidebar(PRS_SIDEBAR_WIDTH) + Column{ error banner, filter band, PrList, PrDetailView, agent
  chooser overlay } } + KeybindBar(DashboardPullRequests). Reuse Sidebar/StatusBar/KeybindBar/
  AgentChooser; reuse `ScrollableText` ONLY for the PR-detail body/comments TEXT region (the same
  role it plays in `issue_detail.rs`). The PR LIST uses the `crate::layout` selection-follow helpers,
  never `ScrollableText`.

### Files to modify
- `src/ui/components/mod.rs`, `src/ui/screens/mod.rs` — register the new PR modules (`pr_list`,
  `pr_detail`, `pr_filter_controls`, `pull_requests`). (No `list_viewport` module is registered — the
  helpers live in `crate::layout`.)
- `src/ui/orchestration.rs` — REPLACE the P03 benign placeholder arm
  (`ScreenMode::DashboardPullRequests => element! { View {} }.into_any()`) with the real
  `ScreenMode::DashboardPullRequests => PullRequestsScreen{...}.into_any()` arm. This MODIFIES the
  existing arm (P03 owns the placeholder; P12 owns the real wiring) — it does not add a second arm.
- `src/ui/components/keybind_bar.rs` — add `DashboardPullRequests` keybind set.

Markers on every item.

## Pseudocode Traceability
- mockups.md measurements; component-001 (state fields consumed by render).

## Verification Commands

Run the COMPLETE baseline (all gates MUST pass — this is a stub/GREEN phase, no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# or: make ci-check
# Boundary-isolation HARD gate (finding #3 — rg exits NONZERO on no-match, so an absence check must
# be inverted to fail ONLY when a forbidden import is FOUND):
if rg -n "use crate::github|use crate::app_input" src/ui/ ; then
  echo "FAIL: src/ui imports a forbidden layer (github/app_input)"; exit 1
fi
rg -n "PullRequestsScreen|PrList|PrDetailView|PrFilterControls" src/ui/
```
All gates above MUST pass. Stub bodies compile; no command is permitted to fail in this phase.

## Structural Verification Checklist
- [ ] Build green; components + screen + layout helpers present.
- [ ] `build_screen_element` arm added; existing arms unchanged.
- [ ] No duplicated layout constants.
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] UI imports no `crate::github`/`crate::app_input` (renders/emits only).
- [ ] Viewport rows + list_pane_rows are PROPS (not read via `crossterm::size()` inside components)
  (#37/#39).
- [ ] Reuses Sidebar/StatusBar/KeybindBar/AgentChooser; reuses `ScrollableText` ONLY for the
  detail TEXT region (NOT for list rows).
- [ ] `pr_list.rs` consumes the `crate::layout` selection-follow helpers
  (`list_first_visible_index`/`list_visible_window`) and does NOT use `ScrollableText` for rows; NO
  `src/ui/components/list_viewport.rs` file exists (the helpers live in `crate::layout`, a new build
  — no existing list-scroll abstraction exists to reuse).
- [ ] No `todo!()`/`unimplemented!()` ANYWHERE in P12 files (findings #1 & #4 — clippy denies both
  macros). HARD gate:
  ```bash
  if rg -n "todo!\(\)|unimplemented!\(\)" src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
    echo "FAIL: todo!()/unimplemented!() in a PR render path (clippy denies both)"; exit 1
  fi
  ```
- [ ] The `build_screen_element` `DashboardPullRequests` arm now wires the real `PullRequestsScreen`
  (replacing P03's placeholder), and no second/duplicate arm was added (cite).

## Deferred Implementation Detection
```bash
# Stub phase: todo!()/unimplemented!() are FORBIDDEN (gated above; clippy denies both). Record other
# deferred markers; these become hard gates in P14.
# Record-only: append `|| true` so a no-match (rg exit 1) cannot abort the phase under `set -e`.
rg -n "TODO|FIXME|HACK|placeholder|for now" src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs || true
```

## Success Criteria
- Compiles; DashboardPullRequests renders skeleton; isolation preserved.

## Failure Recovery
Restore the modified tracked files and delete ONLY the new files this phase created. Do NOT use
`git clean`. (Restoring `src/ui/orchestration.rs` returns the `DashboardPullRequests` arm to P03's
benign placeholder.)
```bash
git restore --staged --worktree -- \
   src/ui/components/mod.rs src/ui/screens/mod.rs src/ui/orchestration.rs \
   src/ui/components/keybind_bar.rs src/layout.rs
rm -f src/ui/components/pr_list.rs src/ui/components/pr_detail.rs \
      src/ui/components/pr_filter_controls.rs \
      src/ui/screens/pull_requests.rs
```

## Phase Completion Marker (`.completed/P12.md`)
Phase ID, timestamp, files changed, build result, isolation check, semantic summary.
