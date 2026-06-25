# Phase 10 — Message Bus & Key Routing TDD (RED)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P10
- **Prerequisites:** `.completed/P09A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Write behavioral tests for PR key routing (8-level precedence), Esc unwind precedence, suppression,
focus-domain key handling, filter-controls interactivity, inline submit, and agent chooser. Tests
must fail (RED) against P09 stubs. The `AppEvent`↔`PullRequestsMessage` round-trip CONVERSION tests
are NOT in this phase — they belong to the domain-state slice (P04 RED → P05 GREEN), because the
conversion is stubbed in P03 and the P05 reducer hub already depends on it (finding #1).

## Requirements Implemented (Expanded)

### REQ-PR-001,002,003,004,008,010,011,012,013
- **Behavior contract:** GIVEN the P09 stub handlers, WHEN routing tests run, THEN they fail because
  the handlers do not yet emit the right events.
- **RED cause (findings #1 & #4):** P09 handlers are TOTAL and clippy-clean (NO
  `todo!()`/`unimplemented!()`, which clippy denies) — they return safe `None`/NO-OP values. So every
  RED failure is a BEHAVIORAL assertion mismatch (handler returned `None`/wrong event instead of the
  expected `Some(event)`), NEVER a panic; fmt/clippy/build/`check-clippy-allows.sh` stay GREEN.

## Implementation Tasks

### Tests (`src/app_input/app_input_tests.rs` or dedicated `prs_input_tests.rs`)
Each test carries markers. Representative tests:
- `test_p_from_dashboard_emits_enter_prs_mode` — REQ-PR-001 / c003 L01-09.
- `test_p_ignored_when_not_dashboard` — REQ-PR-001 / c003 L01-09.
- `test_p_in_prs_mode_refocuses_pr_list` — REQ-PR-001 / c003 L24-25. Asserts that with
  `screen_mode == DashboardPullRequests`, pressing `p`/`P` emits `Some(RefocusPrList)` (NOT a
  second `EnterPrsMode`). This proves the dashboard-PRs intercept handles `p` before
  `resolve_mode_key` can re-enter.
- `test_handle_dashboard_prs_key_runs_before_resolve_mode_key` — REQ-PR-001,002 / c003 L05-09,18.
  Ordering proof: drives `handle_normal_key_event` with `screen_mode == DashboardPullRequests` and
  `p`, asserting the emitted event is `RefocusPrList` (the PR intercept consumed it) and that
  `resolve_mode_key`'s Dashboard-only `EnterPrsMode` arm was NOT taken. Mirrors the issues-mode
  guarantee that `i` does not re-fire while already in `DashboardIssues`.
- `test_a_exits_prs_mode_from_global_level` — REQ-PR-001 / c003 L10-20.
- `test_input_mode_for_state_routes_dashboard_pull_requests_by_precedence` — REQ-PR-002,004 /
  c003 L07,51 (finding #3 — behavior MOVED here from the P03 compile-only stub). Asserts
  `input_mode_for_state` returns the correct `InputMode` for a `DashboardPullRequests` screen by
  precedence Inline > Chooser > Search > Filter > Normal — i.e. `PrsInline` when an inline composer
  is active, `PrsChooser` when the agent chooser is open, `PrsSearch` when the search input is
  focused, `PrsFilter` when filter controls are open, and `PrsNormal` otherwise (mirroring the
  `DashboardIssues` precedence). RED because the P03 stub returns a constant `PrsNormal` for ALL
  `DashboardPullRequests` states; GREEN in P11.
- `test_tab_cycles_panes_from_every_pane` — REQ-PR-003 (issue #46) / c003 L14-20. Asserts
  `Tab`/`Shift+Tab` resolve to `PrCycleFocus`/`PrCycleFocusReverse` in ALL THREE panes — `RepoList`,
  `PrList`, AND `PrDetail` (Tab cycles panes from detail too; it is NOT consumed for subfocus there).
- `test_jk_moves_subfocus_in_pr_detail` — REQ-PR-003 (issue #46) / c003 L81-82. Asserts `j`/`k`
  resolve to `PrDetailSubfocusNext`/`PrDetailSubfocusPrev` in `PrDetail`, and that `Tab`/`Shift+Tab`
  do NOT resolve to subfocus there.
- `test_left_arrow_optional_reverse_cycle_in_pr_detail` — REQ-PR-003 / c003 L83-85. Asserts `Left`
  resolves to `PrCycleFocusReverse` in `PrDetail` (optional parity; not the sole escape).
- `test_repo_focus_up_down_changes_repo_not_pane_focus` — REQ-PR-003 (#47) / c003 L49-56.
- `test_inline_composer_consumes_keys_before_global` — REQ-PR-002,010 / c003 L10-18.
- `test_agent_chooser_consumes_keys_before_search` — REQ-PR-002,011 / c003 L10-18.
- `test_search_input_routes_chars_to_query` — REQ-PR-002,008 / c003 L127-133.
- `test_filter_controls_tab_space_text_enter_clear_esc` — REQ-PR-008 (#38/#40) / c003 L134-146.
- `test_filter_field_cycling_wraps_through_all_eight_fields` — REQ-PR-008 (#38/#40) / c003 L134-138.
  Asserts `Tab` advances `filter_ui.field_index` through ALL EIGHT fields in order — state, draft,
  review-decision, checks-status, author, assignee, reviewer, labels — and WRAPS from the last back
  to the first; `Shift+Tab` reverses and wraps the other way. This is the updated field count after
  adding the two issue #20 signal filters (was 6, now 8).
- `test_space_cycles_review_decision_filter_draft_state` — REQ-PR-008 (issue #20 review signal) /
  c003 L139-140. With filter controls open and `field_index` on the review-decision field, asserts
  `Space` emits `Some(PrCycleReviewFilter)` and that the reducer cycles
  `draft_filter.review_decision` through `Any -> Approved -> ChangesRequested -> ReviewRequired ->
  None -> Any` (wrap), updating DRAFT state only (committed filter unchanged until Apply).
- `test_space_cycles_checks_status_filter_draft_state` — REQ-PR-008 (issue #20 workflow signal) /
  c003 L139-140. With `field_index` on the checks-status field, asserts `Space` emits
  `Some(PrCycleChecksFilter)` and the reducer cycles `draft_filter.checks_status` through `Any ->
  Success -> Failing -> Pending -> Any` (wrap), updating DRAFT state only.
- `test_apply_commits_review_and_checks_filters_and_triggers_reload` — REQ-PR-008 (issue #20
  signals) / c003 L143. After cycling the review-decision and checks-status DRAFT fields, asserts
  `Enter`/Apply emits `Some(PrApplyFilter)`, the reducer copies `draft_filter -> committed_filter`
  (so `committed_filter.review_decision`/`checks_status` reflect the new selections), and a list
  reload is triggered (new `list_reload_pending` request id) — proving the new controls update draft
  state AND that Apply commits + reloads.
- `test_esc_precedence_inline_then_chooser_then_search_then_filter_then_exit` — REQ-PR-004 /
  c003 L92-98.
- `test_c_opens_comment_composer_only_from_detail_subfocus` — REQ-PR-010 (#56) / c003 L72-82.
- `test_c_on_review_or_check_emits_show_notice_not_none` — REQ-PR-010,013 / c003 L83-85. Asserts `c`
  on a `Review(i)`/`Check(i)` subfocus returns `Some(PrShowNotice{ ReadOnlyNoComment })` (NOT
  `None`/no composer) and that `apply_*` sets `prs_state.draft_notice = Some(_)`.
- `test_r_replies_only_on_comment_subfocus` — REQ-PR-010,013 / c003 L86-87. Asserts `r` on a comment
  opens the reply composer; `r` on body/review/check/new-comment returns
  `Some(PrShowNotice{ ReadOnlyReplyOnComment })` (NOT `None`) and populates `draft_notice`.
- `test_e_on_pr_detail_emits_show_notice_not_none` — REQ-PR-010,013 / c003 L83-89. Asserts `e` in PR
  detail returns `Some(PrShowNotice{ ReadOnlyNotEditable })` (NOT a silent `None`) and that the
  reducer surfaces a non-blocking `draft_notice` (regression guard against silent-`None` drops).
- `test_capital_s_opens_agent_chooser_from_detail` — REQ-PR-011 / c003 L72-82.
- `test_o_on_loaded_pr_emits_open_in_browser` — REQ-PR-012 / c003 L68-69,88-89. Asserts `o` (from PR-list
  focus and from PR-detail focus) with a PR present returns `Some(PrOpenInBrowser)`.
- `test_o_with_no_selection_emits_show_notice_not_none` — REQ-PR-012 / c003 L68-69,88-89. Asserts `o` with
  no PR loaded returns `Some(PrShowNotice{ NoSelectionToOpen })` (consumed + hint, never `None`).
- `test_suppressed_keys_ctrl_d_ctrl_k_l_consumed_noop` — REQ-PR-002 / c003 L10-48
  (Consumed-no-op semantics). Asserts `s`/`Ctrl-d`/`Ctrl-k`/`l` resolve to `KeyHandling::Handled(None)`
  — i.e. CONSUMED at the outer layer (no fallthrough to dashboard/destructive handlers) with NO
  emitted `AppEvent` and NO state change. This is the "consumed + silently ignored" outcome and is
  explicitly DISTINCT from the read-only `r`/`c`/`e`/`o` "consumed + notice" outcome (which returns
  `Some(PrShowNotice{kind})`); the test proves a bare `None` never carries a user-visible effect and
  is never used for the read-only cases. (Mirrors `test_s_key_suppressed_in_issues_mode` etc.)

> NOTE (TDD sequencing — finding #1): the `AppEvent`↔`PullRequestsMessage` round-trip CONVERSION
> tests live in the domain-state slice (P04 RED → P05 GREEN), NOT here. The conversion is
> stubbed in P03 (`src/messages/prs_conversion.rs` — TOTAL wrong-value stub, NEVER `todo!()`),
> RED-tested in P04, and implemented exactly once in P05 — because `apply_prs_message` (the P05
> reducer hub) already depends on `AppEvent::from(message)`. This phase only RED-tests KEY ROUTING
> (`handle_prs_mode_key` →
> `Option<AppEvent>`); it does NOT re-test or re-implement the conversion. The `o`-key tests above
> assert the EMITTED `AppEvent` (routing), not the message conversion.

## Pseudocode Traceability
- component-003 lines 01-232 (key routing only; conversion lines c004 L45-85 are owned by P04/P05).

## Verification Commands

This is a **TDD(RED)** phase. Run the COMPLETE baseline. The RED exception applies to exactly ONE
command — `cargo test` — which is EXPECTED to fail (the new key-routing/no-op-hint tests have no
implementation yet). Every other gate MUST pass (the test code must COMPILE; only assertions may
fail):
```bash
cargo fmt --all --check                                            # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                    # MUST pass (RED tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p10.log  # EXPECTED to FAIL (RED)
rg -n "test result: FAILED" /tmp/p10.log   # expect >=1 failure (RED confirmed)
```
RED exception: only `cargo test` may fail, and only because the behavioral tests are unimplemented.
`cargo build`, fmt, clippy, and `check-clippy-allows.sh` MUST all be green.

## Structural Verification Checklist
- [ ] Tests compile/registered; ≥1 RED; markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] Precedence tests assert ordering (a higher level consumes before a lower one).
- [ ] `p`-in-PR-mode ordering test asserts `handle_dashboard_prs_key` consumes `p`/`P` as
  `RefocusPrList` BEFORE `resolve_mode_key` (never a second `EnterPrsMode`).
- [ ] Suppression tests assert keys are consumed (no fallthrough to dashboard actions).
- [ ] No `AppEvent`↔`PullRequestsMessage` round-trip test is (re)added here — that conversion is
  owned and tested by the domain-state slice (P04 RED → P05 GREEN); this phase asserts EMITTED
  `AppEvent`s only (finding #1).
- [ ] No `assert!(true)`, `#[ignore]`, unwrap/expect.

## Deferred Implementation Detection
Inverted HARD gate (absence passes, presence fails) covering all four weak-test smells:
```bash
if rg -nP 'assert!\(true\)|#\[ignore\]|\.unwrap\(\)|\.expect\(' src/app_input/; then
  echo "FAIL: deferred/weak test smell (assert!(true) | #[ignore] | .unwrap() | .expect())"; exit 1
fi
```

## Success Criteria
- RED confirmed; full key-routing/precedence/suppression/no-op-hint coverage.

## Failure Recovery
- Fix test compilation.

## Phase Completion Marker (`.completed/P10.md`)
Phase ID, timestamp, test list, RED list, semantic summary.
