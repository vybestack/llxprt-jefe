# Phase 05 — Domain & State Impl (GREEN)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P05
- **Prerequisites:** `.completed/P04A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Implement the PR-Mode reducer AND the relocated shared viewport helpers (`src/layout.rs`) so all P04
RED tests turn GREEN. (P03's stubs are already total and clippy-clean — there are NO `todo!()` to
remove; P05 simply replaces the deterministic WRONG-value stub bodies with correct logic.) Reducer
remains pure (no I/O); side effects stay in the dispatch layer (later phases).

## Requirements Implemented (Expanded)

### REQ-PR-001 entry/exit, REQ-PR-003 focus/nav, REQ-PR-005 restore, REQ-PR-006 list, REQ-PR-007 pagination, REQ-PR-008 filter/search, REQ-PR-009 detail, REQ-PR-010 comments, REQ-PR-011 chooser, REQ-PR-012 open-in-browser (pure reducer half), REQ-PR-014 empty, NFR-002 staleness
- **Behavior contract:** GIVEN P04 RED tests, WHEN P05 lands, THEN all turn GREEN and existing tests
  stay green; reducer transitions match pseudocode exactly.
- **Why it matters:** This is the deterministic core; correctness here underpins all later layers.

## Implementation Tasks

### `src/state/prs_ops.rs` (markers per fn; pseudocode line refs)
- `enter_prs_mode` — c001 L66-76.
- `exit_prs_mode` (restore prior focus, bounds fallback) — c001 L77-87.
- `reset_prs_for_repo_change` (pub(super)) — c001 L88-98.
- `cycle_prs_focus` / `cycle_prs_focus_reverse` — c001 L154-162.
- `navigate_repo_up/down_in_prs_mode` (thin wrappers over the SHARED `move_repo_selection` helper,
  independent of pane_focus, #47) — c001 L146-153 (shared helper at c001 L134-145).
- `navigate_pr_list_{up,down,page_up,page_down,home,end}` (clamp `selected_pr_index`, then update
  `list_scroll_offset` via the NEW shared `crate::layout::list_first_visible_index` helper — c001
  L99-124, helper at c001 L182-189; the helper is a fresh build in `src/layout.rs` (finding #2), not
  a reuse of any existing list-scroll code, and the STATE layer imports it from the shared `layout`
  leaf — NOT from `crate::ui`); `invalidate_detail_requests_if_pr_selection_changed`.
- `detail_subfocus_next/prev` (Body/Review(i)/Check(i)/Comment(i)/NewComment) — c001 L201-208.
- `apply_pr_scroll_event` (bounded by rendered length, viewport prop; #37/#39) — c001 L169-176.
- `apply_prs_message(&mut self, PullRequestsMessage) -> bool` hub — special-case `ApplySearch`
  (commit trimmed query + reset) else `message => self.apply_prs_event(AppEvent::from(message))`
  (mutating `&mut self`, mirroring `apply_issues_message`; NOT a `self -> Self` consuming form) —
  c001 L366-372, c004 L70-83.
- `apply_prs_event(&mut self, AppEvent) -> bool` chained-OR over scroll/lifecycle/filter/inline-open/
  inline/mutation/agent/notice/open-browser/error appliers — c001 L373-385 (open-browser chained at
  c001 L383).
- `apply_pr_notice_event` → `apply_pr_show_notice(kind)` (REQ-PR-010/013): on `PrShowNotice(kind)`
  set `prs_state.draft_notice = Some(text_for(kind))` and return `true` — c001 L344-348. This is
  the no-silent-`None` surface for invalid `r`/`c`/`e` read-only actions and the `o`-with-no-selection
  (`NoSelectionToOpen`) case.
- `apply_pr_open_browser_event` → `apply_pr_open_in_browser` / `apply_pr_open_in_browser_failed`
  (REQ-PR-012; pure reducer half): on `PrOpenInBrowser` set a transient "opening in browser…" notice
  (the actual `gh pr view --web` launch is the dispatch layer's side effect, NOT done here); on
  `PrOpenedInBrowser` clear the opening notice; on `PrOpenInBrowserFailed{error}` set a scoped error
  notice (no silent drop) — c001 L349-357,362-365, hub chain c001 L383.

### `src/state/prs_load_ops.rs`
- `apply_pr_list_loaded` (validate scope+request_id, select 0, mark for detail load) — c001 L209-223.
- `apply_pr_list_page_loaded` (append, no reorder) — c001 L224-229.
- `apply_pr_detail_loaded` (subfocus=Body, clear loading) — c001 L230-235.
- `apply_pr_comments_page_loaded` (append older, stable) — c001 L236-241.
- `*_failed` handlers (clear loading, set scoped error, never silent) — c001 L242-247.

### `src/state/prs_inline_ops.rs`
- `apply_pr_inline_open_event` (OpenNewCommentComposer sets subfocus NewComment + follow; OpenReply
  prefill) — c001 L292-307.
- `apply_pr_inline_event` (char/newline/backspace/cursor/submit/cancel) — c001 L308-330.

### `src/state/prs_mutation_ops.rs`
- `apply_pr_mutation_event` (MutationSubmitted, CommentCreated append + follow viewport,
  CommentCreateFailed/MutationFailed scoped error) — c001 L316-327.
- `apply_pr_error_event` — c001 L242-247.

### `src/state/prs_ops.rs` (filter/search + agent)
- `apply_pr_filter_event` (FILTER_FIELD_COUNT wrap; CycleFilterState Open→Closed→Merged→All→Open;
  UpdateDraftFilter live; ApplyFilter commit + reset; ClearFilter) — c001 L249-274.
- `reload_pr_list_for_filter_change` — c001 L275-281.
- `apply_pr_agent_chooser_event` + `open_pr_agent_chooser` — c001 L331-340.

### `src/layout.rs` (shared viewport helpers — finding #2)
- Implement the two PURE, no-dependency selection-follow helpers (replacing the P03 WRONG-value
  stubs) so the P04 layout RED tests turn GREEN:
  - `pub fn list_first_visible_index(selected_index: usize, len: usize, viewport_rows: usize) ->
    usize` — component-001 lines 182-189. Returns the first visible row index that keeps `selected`
    on screen, clamped to `0..=len.saturating_sub(viewport_rows)`.
  - `pub fn list_visible_window<T>(rows: &[T], selected_index: usize, viewport_rows: usize) -> &[T]`
    — component-001 lines 190-196. Returns exactly `min(viewport_rows, rows.len())` rows starting at
    `list_first_visible_index(selected_index, rows.len(), viewport_rows)`, always including
    `selected_index`. (Signature matches the pseudocode and the P03 stub — `rows: &[T] -> &[T]`,
    NOT a `Range`.)
- These live in `src/layout.rs` (the shared leaf importable by BOTH `state` and `ui` without a
  boundary violation); the STATE reducers (above) and the UI `pr_list` (P14) consume the SAME
  functions. Carry `@plan PLAN-20260624-PR-MODE.P05 @requirement REQ-PR-006 @pseudocode component-001
  lines 182-196` markers.

### `src/state/mod.rs`
- `apply_message` `PullRequests` arm: replace the P03 compile-only `let _handled =
  self.apply_prs_message(message);` with `let handled = self.apply_prs_message(message);
  debug_assert!(handled, "unhandled PullRequestsMessage: {message:?}");` (finding #4 — the
  `debug_assert!(handled)` companion is ADDED HERE, in the GREEN domain-state phase that owns
  `src/state/mod.rs` and `apply_prs_message`, now that `apply_prs_message` truly handles every
  `PullRequestsMessage` variant — c004 L70-83). Mirrors the issues `apply_message` arm.
- Confirm `select_repository_by_index` resets PR state when active (wire the
  `if self.prs_state.active { self.reset_prs_for_repo_change(); }` deferred from P03; turns the P04
  RED `test_select_repository_resets_pr_scope` GREEN).
- **SHARED repo-nav helper extraction (Finding 5).** EXTRACT the repo selection-move logic that is
  currently duplicated in `src/state/issues_ops.rs::navigate_repo_up_in_issues_mode` (L122-131) and
  `navigate_repo_down_in_issues_mode` (L137-148) into ONE shared method
  `fn move_repo_selection(&mut self, direction: NavDir) -> bool` on `AppState` (`src/state/mod.rs`),
  built on the existing helpers `visible_repository_indices` (mod.rs L194),
  `selected_repository_visible_index` (mod.rs L207), `remember_selected_agent_for_current_repo`
  (mod.rs L130), `restore_selected_agent_for_current_repo` (mod.rs L152) — c001 L134-145. It returns
  whether the selection changed; the caller runs its own mode-specific reset.
- **REFACTOR Issues mode to call the shared helper (no duplication).** Rewrite
  `navigate_repo_up/down_in_issues_mode` so each body becomes
  `if self.move_repo_selection(dir) { self.reset_issues_for_repo_change() }` — c001 L152-153. PR
  mode's `navigate_repo_up/down_in_prs_mode` are thin wrappers calling the SAME helper
  (`if self.move_repo_selection(dir) { self.reset_prs_for_repo_change() }`); PR mode MUST NOT define
  a private copy of the selection-move logic. Existing issues repo-nav tests must remain green.

### `src/messages/prs_conversion.rs`
- Implement `PullRequestsMessage::from_app_event` and `From<PullRequestsMessage> for AppEvent`
  (total over PR variants) — c004 L43-72. This is the SINGLE GREEN implementation of the
  message↔event conversion; it turns the P04 RED conversion round-trip tests
  (`test_pr_show_notice_round_trips_and_sets_draft_notice`, `test_open_in_browser_events_round_trip`,
  `test_appevent_pullrequestsmessage_round_trip`) GREEN. The conversion is stubbed in P03, RED-tested
  in P04, and implemented here exactly once — it is NOT (re)implemented in P11 (finding #1).

## Pseudocode Traceability
- component-001 lines 66-389 (reducer transitions); component-001 lines 182-196 (shared
  `list_first_visible_index` / `list_visible_window` viewport helpers in `src/layout.rs` — finding
  #2); component-004 lines 45-85 (message↔event conversion).

## Verification Commands

This is a GREEN/impl phase — the COMPLETE baseline below MUST pass (no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# Forbidden-pattern gate: fail (nonzero) if any todo!()/unimplemented!() appears (clippy already
# denies both, but assert it explicitly across the impl surface including the layout helpers).
if rg -n "todo!\(\)|unimplemented!\(\)" \
   src/state/prs_*.rs src/messages/prs_conversion.rs src/layout.rs ; then
  echo "FAIL: todo!()/unimplemented!() in state/conversion/layout layer"; exit 1
fi
```

## Structural Verification Checklist
- [ ] All P04 RED tests now GREEN (including the `src/layout.rs` viewport-helper tests); existing
  tests green.
- [ ] No unmatched message variant (debug_assert(handled) never trips).
- [ ] No `todo!()`/`unimplemented!()` anywhere in the state/conversion/layout surface.
- [ ] `src/layout.rs` viewport helpers implemented and consumed by `src/state/prs_ops.rs`; STATE
  layer imports them from `crate::layout` (NOT `crate::ui`) — finding #2 boundary preserved.
- [ ] Per-fn markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] Transitions match pseudocode (spot-check 6 with cite).
- [ ] Stale responses discarded (scope/request_id).
- [ ] Repo-scope change resets list/detail/pending + preserves committed filter/search.
- [ ] Composer open sets subfocus NewComment + follow; comment-created follows viewport.
- [ ] Scroll bounded by rendered length using viewport prop, not `crossterm::size()`.
- [ ] No silent None arms; errors surfaced as scoped messages. Read-only `r`/`c`/`e` no-ops route
  through `PrShowNotice`/`apply_pr_show_notice` and populate `prs_state.draft_notice` (cite) — never
  a bare `None` drop (REQ-PR-010/013).
- [ ] No clippy allow / no threshold override; functions within limits.

## No-Placeholder / Deferred Detection
HARD inverted gate (finding #6) — absence passes, presence fails:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" \
   src/state/prs_*.rs src/messages/prs_conversion.rs src/layout.rs ; then
  echo "FAIL: deferred-implementation marker present in impl phase"; exit 1
fi
```

NOTE: this phase-local, file-scoped deferred-marker scan is a fast local guard ONLY; it does NOT
replace the global workspace-wide deferred-marker / no-placeholder gate enforced at P16
(16-e2e-quality-gate). Passing this local scan is necessary but not sufficient; the P16 gate remains
the authoritative final check.

## Success Criteria
- Full suite green; reducer pure; no placeholders; within complexity limits.

## Failure Recovery
- `git restore`/`git stash` the state layer; re-implement per pseudocode; if a test cannot pass,
  bisect between P04A and P05.

## Phase Completion Marker (`.completed/P05.md`)
Phase ID, timestamp, RED→GREEN list, clippy/fmt result, no-placeholder output, semantic summary.
