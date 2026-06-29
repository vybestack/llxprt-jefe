# Phase 14 — UI Integration Impl (GREEN)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P14
- **Prerequisites:** `.completed/P13A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Implement the PR-Mode render logic so all P13 RED tests turn GREEN. Wire the PR list to the shared
selection-follow helpers (`list_first_visible_index`/`list_visible_window`), which already live in
`src/layout.rs` and were implemented in P05 (finding #2 — they are NOT a UI file, because the state
reducers consume them too). Build the unified detail/checks/reviews/comments view with overflow
derived from rendered length, composer visibility, filter controls, empty/error states, and the
keybind bar — all per mockups. UI renders/emits only.

## Requirements Implemented (Expanded)

### REQ-PR-006,008,009,010,012,013,014, NFR-003
- **Behavior contract:** GIVEN P13 RED tests, WHEN P14 lands, THEN all components render per the
  mockup layout contract and regression guards pass; UI consumes `prs_state` props and emits intent
  events only.

## Implementation Tasks

> NOTE (finding #2): the pure selection-follow helpers `list_first_visible_index`/
> `list_visible_window` already exist in `src/layout.rs` (stub P03 → RED P04 → impl P05;
> component-001 lines 182-196). They are NOT created or modified here. They are written generically
> so the issue list can adopt them later; do NOT modify `issue_list.rs` here. There is NO
> `src/ui/components/list_viewport.rs` file.

### `src/ui/components/pr_list.rs` (markers + refs)
- Render N rows for N loaded PRs (#54); compute the visible window via the `crate::layout`
  selection-follow helpers (`list_first_visible_index`/`list_visible_window`, component-001 lines
  182-196) using `viewport_rows = prs_pane_rows(...)` so the selected row is always on-screen (#55);
  do NOT use `ScrollableText` for list rows; truncate long titles with ellipsis by
  `pr_list_content_width` (#37h); draft/review-decision/check markers.

### `src/ui/components/pr_detail.rs`
- Unified scrollable view: header (title+metadata+branches), body, review summary, check summary,
  comments, composer. Overflow/max-scroll derived from ACTUAL rendered content length (#37f);
  viewport height from `viewport_rows` PROP (#37g/#39); composer rendered within viewport when
  active (#56); display-only `external_url`.

### `src/ui/components/pr_filter_controls.rs`
- Render all filter fields, highlight active field, show draft values.

### `src/ui/screens/pull_requests.rs`
- Compose StatusBar + Sidebar(22u) + workspace(error banner, filter band, PrList[list_pane_rows],
  PrDetailView[flex], agent chooser overlay) + KeybindBar; pass `prs_pane_rows`/
  `prs_detail_viewport_rows` results as props.

### `src/ui/components/keybind_bar.rs`
- Populate `DashboardPullRequests` keybinds (Tab, ↑↓, Enter, f, /, c, r, S, a, Esc, `o` = open PR
  in browser) — the `o` label MUST read "open in browser" (REQ-PR-012), consistent with help text.

## Pseudocode Traceability
- mockups.md; component-001 state fields.

## Verification Commands

Run the COMPLETE baseline (all gates MUST pass — this is a GREEN impl phase, no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# or: make ci-check
# Deferred-implementation HARD inverted gate (finding #6) — absence passes, presence fails:
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" \
   src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
  echo "FAIL: deferred-implementation marker present after impl phase"; exit 1
fi
# Forbidden-API HARD gates (finding #3 — rg exits nonzero on no-match, so invert: fail ONLY when the
# forbidden pattern is FOUND):
if rg -n "crossterm::terminal::size" src/ui/components/pr_*.rs ; then
  echo "FAIL: PR components must take viewport rows as props, not read crossterm::terminal::size"; exit 1
fi
if rg -n "ScrollableText" src/ui/components/pr_list.rs ; then
  echo "FAIL: PR list rows must NOT use ScrollableText (use the crate::layout viewport helpers)"; exit 1
fi
```
All gates above MUST pass; no command is permitted to fail in this phase.

## Structural Verification Checklist
- [ ] All P13 RED tests GREEN; existing tests green.
- [ ] No `todo!()`/`unimplemented!()` (clippy denies both); markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] All loaded rows render (#54); selection stays visible (#55) via the `crate::layout`
  selection-follow helpers (`pr_list.rs` consumes them and does NOT use `ScrollableText` for rows;
  no `src/ui/components/list_viewport.rs` file exists).
- [ ] Overflow from rendered length (#37f); viewport from prop (#37g/#39); composer visible (#56).
- [ ] Titles truncated by pane width (#37h).
- [ ] Filter controls render interactively (state mirrors draft).
- [ ] Empty + error states render.
- [ ] UI imports no github/app_input; emits intent only.
- [ ] No clippy allow / no override; functions within limits (split render helpers as needed).

## No-Placeholder / Deferred Detection
HARD inverted gate (finding #6) — absence passes, presence fails:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" \
   src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
  echo "FAIL: deferred-implementation marker present in impl phase"; exit 1
fi
```

NOTE: this phase-local, file-scoped deferred-marker scan is a fast local guard ONLY; it does NOT
replace the global workspace-wide deferred-marker / no-placeholder gate enforced at P16
(16-e2e-quality-gate). Passing this local scan is necessary but not sufficient; the P16 gate remains
the authoritative final check.

## Success Criteria
- Suite green; mockup-aligned render; regression guards pass; no placeholders; within limits.

## Failure Recovery
- `git restore` UI; re-implement per mockups; bisect P13A↔P14.

## Phase Completion Marker (`.completed/P14.md`)
Phase ID, timestamp, RED→GREEN list, clippy/fmt result, no-placeholder output, semantic summary.
