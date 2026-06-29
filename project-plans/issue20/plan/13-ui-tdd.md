# Phase 13 — UI TDD (RED)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P13
- **Prerequisites:** `.completed/P12A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Write behavioral render tests for the PR components and screen, asserting the mockup layout contract
and the rendering regression guards (#54 all rows, #55 selection-following, #56 composer visibility,
#37/#39 overflow from rendered length + viewport prop). Tests must fail (RED) against the P12
skeleton.

## Requirements Implemented (Expanded)

### REQ-PR-006,008,009,010,012,013,014, NFR-003
- **Behavior contract:** GIVEN state fixtures, WHEN components render to a test buffer, THEN the
  output matches the layout contract and regression guarantees.

## Implementation Tasks

### Tests (component-local `#[cfg(test)]` or `src/ui/.../pr_*_tests.rs`)
Each test carries markers. Representative tests:

> NOTE (finding #2): the PURE selection-follow helpers `list_first_visible_index`/
> `list_visible_window` live in `src/layout.rs` (NOT the UI layer), and their pure-logic RED tests
> are owned by **P04** (against `crate::layout`), implemented in P05. P13 does NOT re-test the helper
> algorithm directly; it asserts the COMPONENT behavior that consumes them (rows render, selection
> stays visible) via `pr_list.rs`.

Component/screen render tests:
- `test_pr_list_renders_all_loaded_rows` (#54) — REQ-PR-006 / mockups list region.
- `test_pr_list_keeps_selected_row_visible_when_scrolled` (#55) — REQ-PR-006 (renders only the
  helper's visible window; selected row never clipped).
- `test_pr_list_truncates_long_title_with_ellipsis_by_pane_width` (#37h) — REQ-PR-006.
- `test_pr_list_shows_draft_and_review_decision_markers` — REQ-PR-006.
- `test_pr_detail_renders_metadata_body_review_summary_check_summary` — REQ-PR-009.
- `test_pr_detail_shows_branches_and_external_url` — REQ-PR-009,012 (asserts `external_url` is
  rendered display-only).
- `test_pr_keybind_bar_and_help_list_o_open_in_browser` — REQ-PR-012. Asserts the PR-mode keybind bar
  / help (`?`/`h`/`F1`) includes an `o = open PR in browser` label (no in-app merge/approve binding).
- `test_pr_detail_overflow_derived_from_rendered_length_not_heuristic` (#37f) — REQ-PR-009.
- `test_pr_detail_viewport_uses_prop_height_not_terminal_size` (#37g/#39) — REQ-PR-009.
- `test_pr_detail_composer_visible_within_viewport_when_active` (#56) — REQ-PR-010.
- `test_pr_filter_controls_render_all_fields_and_highlight_active` — REQ-PR-008.
- `test_pr_empty_state_renders_message_when_no_prs` — REQ-PR-014.
- `test_pr_screen_layout_sidebar_22u_and_two_column` — mockups measurements.
- `test_pr_screen_renders_error_banner_when_error_present` — REQ-PR-013.

## Pseudocode Traceability
- mockups.md measurements; component-001 state fields.

## Verification Commands

This is a **TDD(RED)** phase. Run the COMPLETE baseline. The RED exception applies to exactly ONE
command — `cargo test` — which is EXPECTED to fail (the new render/list-viewport/detail tests have
no implementation yet). Every other gate MUST pass (the test code must COMPILE; only assertions
may fail):
```bash
cargo fmt --all --check                                            # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                    # MUST pass (RED tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p13.log  # EXPECTED to FAIL (RED)
rg -n "test result: FAILED" /tmp/p13.log   # expect >=1 failure (RED confirmed)
```
RED exception: only `cargo test` may fail, and only because the behavioral tests are unimplemented.
`cargo build`, fmt, clippy, and `check-clippy-allows.sh` MUST all be green.

## Structural Verification Checklist
- [ ] Tests compile/registered; ≥1 RED; markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] Each rendering regression (#54,#55,#56,#37f,#37g,#37h) has a dedicated assertion.
- [ ] `pr_list` render tests prove all loaded rows render (#54) and the selected row stays visible
  (#55) by consuming the `crate::layout` helpers; `ScrollableText` is NOT used for list rows. (The
  helper algorithm's own pure-logic RED tests are owned by P04 against `crate::layout`.)
- [ ] Layout test asserts sidebar width + two-column split per mockups.
- [ ] Assertions read the rendered buffer (no `assert!(true)`); no `#[ignore]`/unwrap/expect.

## Deferred Implementation Detection
Inverted HARD gate (absence passes, presence fails) covering all four weak-test smells:
```bash
if rg -nP 'assert!\(true\)|#\[ignore\]|\.unwrap\(\)|\.expect\(' src/ui/; then
  echo "FAIL: deferred/weak test smell (assert!(true) | #[ignore] | .unwrap() | .expect())"; exit 1
fi
```

## Success Criteria
- RED confirmed; full layout + regression coverage.

## Failure Recovery
- Fix test compilation/fixtures.

## Phase Completion Marker (`.completed/P13.md`)
Phase ID, timestamp, test list, RED list, semantic summary.
