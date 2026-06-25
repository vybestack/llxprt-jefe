# Phase 04 — Domain & State TDD (RED)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P04
- **Prerequisites:** `.completed/P03A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Write behavioral tests for the PR-Mode state reducer (enter/exit, focus, navigation, subfocus,
scroll, list/detail/comment loading with staleness, filter/search, inline composer, comment
lifecycle, agent chooser, repo-scope reset). At least one test MUST fail (RED) before P05.

## Requirements Implemented (Expanded)

### REQ-PR-001,002,003,005,006,007,008,009,010,011,012,013,014, NFR-002
- **Behavior contract:** GIVEN the TOTAL P03 stubs (reducer bodies returning safe defaults, the
  `layout.rs` viewport helpers returning WRONG-but-total values, and the `prs_conversion` bodies
  returning deterministic WRONG values — NONE of them `todo!()`/`unimplemented!()`, which clippy
  denies), WHEN behavioral tests run, THEN they fail by ASSERTION because the stubs return wrong
  values/state — NOT because of any panic. This proves the tests exercise real behavior, not stubs.
- **RED cause (findings #1 & #4):** Every RED failure here is a BEHAVIORAL assertion mismatch
  (wrong returned value or wrong state), never a `todo!()`/`unimplemented!()` panic. The P03 stubs
  are total and clippy-clean precisely so that this phase's `cargo build`/`clippy`/fmt/
  `check-clippy-allows.sh` gates stay GREEN while only `cargo test` is RED.
- **Why it matters:** RED-first prevents tests that trivially pass against placeholder code. The
  `AppEvent`↔`PullRequestsMessage` conversion (REQ-PR-002 round-trip invariant) is RED-tested here
  because it is stubbed in P03 (wrong-value total stub) and the P05 reducer hub depends on it
  (finding #1).

## Implementation Tasks

### Test modules to create (mirror `issues_tests*.rs`)
- `src/state/prs_tests.rs`, `prs_tests_detail.rs`, `prs_tests_filter.rs`, `prs_tests_repo_nav.rs`,
  `prs_tests_composer_focus.rs`, `prs_tests_detail_flow.rs`, `prs_tests_components.rs`; register via
  `#[path = ...]` in `src/state/mod.rs` test section.

Each test fn carries `@plan/@requirement/@pseudocode` markers. Representative tests:

- `test_enter_prs_mode_sets_active_and_saves_prior_focus` — REQ-PR-001 / c001 L62-71.
- `test_enter_prs_mode_default_committed_filter_is_open` — REQ-PR-008 / c001 L74. Asserts that after
  `enter_prs_mode`, `prs_state.committed_filter.state == Some(PrFilterState::Open)` (default scope =
  OPEN PRs) and all other structured criteria (author/assignee/reviewer/is_draft/labels/query_text)
  are unset/empty.
- `test_clear_committed_filter_resets_state_to_open` — REQ-PR-008 / c001 L270-274. Asserts that Clear
  (`clear_committed_filter`) resets `committed_filter` to the default with
  `state == Some(PrFilterState::Open)` (not empty) and all other criteria cleared.
- `test_exit_prs_mode_restores_prior_focus_with_bounds_fallback` — REQ-PR-005 / c001 L77-87.
- `test_cycle_focus_repo_to_list_to_detail_and_wrap` — REQ-PR-003 / c001 L154-162.
- `test_navigate_repo_in_prs_mode_changes_selection_independent_of_pane_focus` — REQ-PR-003 (#47) /
  c001 L146-153.
- `test_repo_scope_change_resets_pr_list_detail_and_pending` — REQ-PR-003 / c001 L88-98.
- `test_select_repository_resets_pr_scope` — REQ-PR-003 (finding #3 — behavior MOVED here from the
  P03 stub) / c001 L88-98. Exercises `select_repository_by_index` while `prs_state.active == true`
  and asserts it invokes `reset_prs_for_repo_change` (cleared pr list/detail/pending, reset
  selection/cursors). RED because P03 does NOT wire `reset_prs_for_repo_change` into
  `select_repository_by_index` (the no-op signature exists but is uncalled); GREEN in P05.
#### Persistence backward-compat tests (REQ-PR-NFR-002 — `prs_state` transient/excluded; finding #2)

GROUNDED PERSISTENCE FACTS (verified in `src/`, the test design MUST match these — do NOT invent an
AppState serde round-trip that does not exist):
- Persistence does NOT serialize `AppState`. `AppState` derives only `Debug/Default/Clone` (NOT
  `Serialize`/`Deserialize`), so there is no `#[serde(skip)]` to add.
- The on-disk state is a SEPARATE DTO `persistence::State` (`src/persistence/mod.rs`, struct `State`
  — `#[derive(Serialize, Deserialize, Default)]`), and `to_persisted_state(state: &AppState) ->
  PersistedState` (`src/app_input/mod.rs`, fn `to_persisted_state`) EXPLICITLY enumerates the
  persisted fields (`repositories`, `agents`, `selected_repository_index`,
  `selected_agent_index`, `hide_idle_repositories`, `last_selected_agent_by_repo`). It already omits
  `issues_state`; it likewise must omit `prs_state`. Adding `prs_state` to `AppState` therefore
  CANNOT reach disk unless someone adds it to this explicit mapping — these tests lock that.
- The established backward-compat test pattern is `serde_json::from_value(legacy_json)` against
  `persistence::State` (see `src/persistence/tests.rs`, fn `test_issue_base_prompt_state_backward_compat`),
  and the persisted round-trip pattern is `mgr.save_state(&State{..})` then `mgr.load_state()` (see
  `src/persistence/tests.rs`, fn `file_persistence_roundtrip_state`). The new tests MIRROR these.

These tests live in the domain/state TDD slice (P04) and assert the transient/excluded invariant.
They are RED-or-locking per their nature (the `to_persisted_state` field-set assertion is RED until
P05 wires `prs_state` into `AppState` without adding it to the mapping; the deserialize/default
assertions document and lock the backward-compat invariant). Place them in
`src/state/prs_tests.rs` (the `to_persisted_state` assertion, alongside the existing
`to_persisted_state_carries_hide_idle_toggle` precedent) and, where they exercise the on-disk DTO,
mirror the `src/persistence/tests.rs` precedents.

- `test_to_persisted_state_excludes_prs_state` — REQ-PR-NFR-002 / finding #2. GIVEN an `AppState`
  whose `prs_state.active == true` (and with some non-default PR list/detail/filter content), WHEN
  `to_persisted_state(&state)` is called, THEN the returned `PersistedState`/`persistence::State`
  has NO field carrying any PR data and serializing it to JSON (`serde_json::to_value`) yields a
  payload whose key set is EXACTLY the legacy persisted-state keys (no `prs`/`pull_request*` key
  appears). This is the proof that `prs_state` is excluded from persistence (mirrors
  `to_persisted_state_carries_hide_idle_toggle`).
- `test_pre_pr_persisted_state_deserializes_without_pr_fields` — REQ-PR-NFR-002 / finding #2. GIVEN
  a legacy `state.json` value (a `serde_json::json!` object with NO PR fields whatsoever — the same
  shape used by `test_issue_base_prompt_state_backward_compat`), WHEN it is deserialized into
  `persistence::State`, THEN it deserializes successfully and ALL prior persisted fields are intact
  (mirrors the established backward-compat deserialize test). This proves an OLD state file (written
  before PR mode existed) still loads.
- `test_app_state_default_has_inactive_prs_state` — REQ-PR-NFR-002 / finding #2. Asserts
  `AppState::default().prs_state` is the `PullRequestsState::default()` (inactive: `active == false`,
  empty list/detail, default committed filter), i.e. on a load that produced default/legacy state,
  the resulting `AppState` has `prs_state` inactive. This proves load yields
  `PullRequestsState::default()` / inactive PR state.

> Backward-compat acceptance summary (the three assertions above, in finding-#2 terms):
> (a) `prs_state` is NOT written to `state.json` (`to_persisted_state` omits all PR data);
> (b) an OLD `state.json` without any PR fields still deserializes successfully with prior fields
> intact; (c) the resulting `AppState` has `prs_state == PullRequestsState::default()` (inactive).
>
> Test-hygiene note: where the deserialize test needs to unwrap a `Result`/`Option`, use the
> established `value_or_panic(context)` test helper (defined in `src/startup.rs`, used by the
> `test_issue_base_prompt_state_backward_compat` precedent) — NOT `.unwrap()`/`.expect()`, which the
> P04A vacuous-test gate forbids in `prs_tests*.rs`. Prefer `assert!(matches!(...))` for the
> field-set/inactive assertions.
- `test_list_navigation_keeps_selection_in_bounds` — REQ-PR-006 / c001 L99-118.
- `test_list_loaded_renders_all_rows_including_first_and_last` — REQ-PR-006 (#54) / c001 L209-220.
- `test_list_loaded_discards_stale_scope_or_request_id` — NFR-002 / c001 L209-223.
- `test_list_page_loaded_appends_without_reordering` — REQ-PR-007 / c001 L224-229.
- `test_detail_loaded_sets_subfocus_body_and_clears_loading` — REQ-PR-009 / c001 L230-235.
- `test_detail_loaded_discards_stale_pr_number_or_request_id` — NFR-002 / c001 L230-235.
- `test_comments_page_loaded_appends_older_stable_order` — REQ-PR-010 / c001 L236-241.
- `test_open_comment_composer_sets_subfocus_newcomment` — REQ-PR-010 (#56) / c001 L292-298.
- `test_comment_created_appends_and_marks_follow_viewport` — REQ-PR-010 (#56) / c001 L316-322.
- `test_filter_navigate_and_update_draft_changes_draft_only` — REQ-PR-008 (#38/#40) / c001 L254-264.
- `test_apply_filter_commits_and_resets_for_reload` — REQ-PR-008 / c001 L265-269.
- `test_cycle_filter_state_open_closed_merged_all_open` — REQ-PR-008 / c001 L259-261.
- `test_apply_search_commits_trimmed_query_and_resets` — REQ-PR-008 / c001 L282-286.
- `test_clear_search_blurs_and_reloads` — REQ-PR-008 / c001 L287-291.
- `test_scroll_detail_down_bounded_by_rendered_length` — REQ-PR-009 (#37/#39) / c001 L169-176.
- `test_agent_chooser_open_navigate_confirm_cancel` — REQ-PR-011 / c001 L331-340.
- `test_show_notice_sets_draft_notice_for_each_readonly_hint_kind` — REQ-PR-010,013 / c001 L344-348.
  Asserts `apply_prs_message(ShowNotice(kind))` sets `prs_state.draft_notice = Some(_)` (non-empty)
  for each `ReadOnlyHintKind` variant and returns `handled == true` (no-silent-`None` proof).
- `test_open_in_browser_reducer_is_pure_sets_opening_notice` — REQ-PR-012 / c001 L349-357.
  Asserts `apply_prs_message(OpenInBrowser)` on a state WITH a selected/loaded PR sets a transient
  "opening…" notice (NOT a `NoSelectionToOpen` notice) and performs NO I/O / no list/detail mutation
  (the reducer half is pure; the launch is the dispatch layer's job). With NO PR selected it sets the
  `NoSelectionToOpen` notice. `handled == true` in both cases (never silent).
- `test_open_in_browser_failed_sets_scoped_error_notice` — REQ-PR-012,013 / c001 L362-365.
  Asserts `apply_prs_message(OpenInBrowserFailed{..})` sets a scoped error notice (no silent drop).
- `test_empty_pr_list_shows_empty_state_not_panic` — REQ-PR-014 / c001 L218-220.

#### Message↔event conversion round-trip tests (finding #1 — owned by the domain-state slice)

These RED tests target `src/messages/prs_conversion.rs`, which is stubbed in P03 (a TOTAL stub
returning deterministic WRONG values — NOT `todo!()`, which clippy denies) and implemented in P05.
They MUST be RED here because the stub conversion returns values that do NOT round-trip, so the
assertions fail (by mismatch, never by panic). They live in this slice (NOT P10) because the P05
reducer hub `apply_prs_message` already
depends on `AppEvent::from(message)`; the conversion is therefore stubbed→RED→GREEN exactly once
within P03→P04→P05. Place them in `src/state/prs_tests_components.rs`.

- `test_pr_show_notice_round_trips_and_sets_draft_notice` — REQ-PR-013 / c004 L27,62,81. Asserts
  `PrShowNotice(kind)` ↔ `PullRequestsMessage::ShowNotice(kind)` round-trips and `apply_prs_message`
  sets `draft_notice` (no-silent-drop pipeline proof).
- `test_open_in_browser_events_round_trip` — REQ-PR-012 / c004 L32-34,63-65,82-83. Asserts
  `PrOpenInBrowser`/`PrOpenedInBrowser`/`PrOpenInBrowserFailed` ↔ the matching
  `PullRequestsMessage::OpenInBrowser/OpenedInBrowser/OpenInBrowserFailed` round-trip.
- `test_appevent_pullrequestsmessage_round_trip` — REQ-PR-002 / c004 round-trip invariant. Asserts
  `AppEvent::from(PullRequestsMessage::from_app_event(e)) == e` for sampled PR `AppEvent`s.

#### Shared viewport-helper pure-logic tests (finding #2 — `src/layout.rs`)

The selection-follow viewport helpers were relocated from the (now-nonexistent)
`src/ui/components/list_viewport.rs` into the shared leaf `src/layout.rs` (finding #2) so BOTH the
state reducers (P05) and the UI `pr_list` (P14) can consume the SAME pure functions without a
state→ui boundary violation. Their RED tests therefore belong to THIS domain/state slice (the first
consumer is the P05 reducer), placed in `src/layout.rs`'s own `#[cfg(test)] mod tests` (the
established home for layout helper tests). The P03 stubs return WRONG-but-total values
(`list_first_visible_index` returns `0`; `list_visible_window` returns an empty slice) so
these tests fail by assertion, never by panic.

- `test_list_first_visible_index_follows_selection_past_viewport` — REQ-PR-006 (#55) /
  component-001 lines 182-189. Asserts the first visible index advances to keep `selected` on screen
  once `selected >= viewport_rows` (e.g. selected=12, len=30, viewport_rows=10 ⇒ first_visible=3).
- `test_list_first_visible_index_clamps_at_top_and_short_lists` — REQ-PR-006 (#55) /
  component-001 lines 182-189. Asserts first_visible==0 when `selected < viewport_rows` or when
  `len <= viewport_rows`, and never exceeds `len.saturating_sub(viewport_rows)`.
- `test_list_visible_window_returns_exact_n_rows_and_bounds` — REQ-PR-006 (#54) /
  component-001 lines 190-196. Asserts the returned slice (`list_visible_window(&rows, selected,
  viewport_rows)`) contains exactly `min(viewport_rows, rows.len())` rows, starts at
  `list_first_visible_index(...)`, and includes `rows[selected]`.

## Pseudocode Traceability
- component-001 lines 62-295 (all reducer transitions); component-001 lines 182-196 (the relocated
  shared `list_first_visible_index` / `list_visible_window` viewport helpers — finding #2);
  component-004 lines 43-72 (the `AppEvent`↔`PullRequestsMessage` conversion — RED here, GREEN in P05).

## Verification Commands

This is a **TDD(RED)** phase. Run the COMPLETE baseline. The RED exception applies to exactly ONE
command — `cargo test` — which is EXPECTED to fail because the newly added behavioral tests assert
behavior that is not yet implemented. Every other gate MUST pass (the test code must still
COMPILE; only assertions may fail):
```bash
cargo fmt --all --check                                            # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                    # MUST pass (tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p04.log  # EXPECTED to FAIL (RED)
rg -n "FAILED|test result: FAILED" /tmp/p04.log   # expect >=1 failure (RED confirmed)
```
RED exception rationale: only `cargo test` is allowed to report failures, and only because the
RED behavioral tests have no implementation yet. `cargo build` must still succeed (the new tests
must compile). fmt, clippy, and `check-clippy-allows.sh` MUST be green even in the RED phase.

## Structural Verification Checklist
- [ ] All test modules registered and compiling.
- [ ] Each test carries markers.
- [ ] ≥1 required test FAILS before impl (RED confirmed).

## Semantic Verification Checklist (Mandatory)
- [ ] Tests assert behavior (state transitions), not symbol existence.
- [ ] Staleness tests use mismatched scope/request_id and assert no-op.
- [ ] Composer-focus tests assert subfocus == NewComment + follow flag.
- [ ] Filter tests assert draft-only updates then Apply reload.
- [ ] No `assert!(true)`, no `#[ignore]`, no unwrap/expect/panic in tests (use `assert!(matches!)`).

## Deferred Implementation Detection
Inverted HARD gate (absence passes, presence fails). Scans all P04-authored RED test files,
including the shared viewport-helper tests placed in `src/layout.rs`:
```bash
if rg -nP 'assert!\(true\)|#\[ignore\]|\.unwrap\(\)|\.expect\(' src/state/prs_tests*.rs src/layout.rs; then
  echo "FAIL: deferred/weak test smell (assert!(true) | #[ignore] | .unwrap() | .expect())"; exit 1
fi
```

## Success Criteria
- Tests compile; ≥1 fails RED; coverage of every listed REQ behavior.

## Failure Recovery
- Fix test compilation; if a test passes against stub, strengthen it to assert real behavior.

## Phase Completion Marker (`.completed/P04.md`)
Phase ID, timestamp, test list, RED failure list, semantic summary.

