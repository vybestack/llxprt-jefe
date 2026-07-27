# Issue #480 — New issue now submits on enter

## Problem

In the inline New Issue form (issue #407), pressing plain Enter while the
**Title** field is focused dispatches `NewIssueSubmit`, immediately creating
the issue on GitHub. Every other field in the form advances focus on Enter
(Template/Type/Labels/Milestone/Project/Assignees → `NewIssueFocusNext`;
Body → `NewIssueBodyNewline`), and the form's own rendered hint advertises
`Alt+Enter submit`. Only Alt+Enter (and the Ctrl+Enter compatibility key) is
supposed to submit. This is a regression from the pre-#407 composer where
bare Enter always inserted a newline.

## Root cause

`src/app_input/issues.rs::resolve_new_issue_enter` maps
`NewIssueFormFocus::Title => AppEvent::NewIssueSubmit`. Plain Enter in the
title submits instead of advancing to the next field (Body). The Alt+Enter /
Ctrl+Enter submit path is already handled earlier in
`resolve_new_issue_inline_key_event` and is unaffected.

## Decision (accepted approach)

Change **only** the `Title` arm of `resolve_new_issue_enter` to dispatch
`NewIssueFocusNext` (advance to Body), matching every other selection/text
field. Alt+Enter and Ctrl+Enter continue to submit via the existing
modifier-aware early return. This restores the "Enter never submits" contract
that the rest of the form and the inline editor already honor.

## Acceptance matrix

| ID | Behavior | Evidence |
|----|----------|----------|
| A1 | Plain Enter (no modifiers) while the New Issue form Title field is focused dispatches `NewIssueFocusNext` (advances to Body), **not** `NewIssueSubmit`. | Behavioral key-routing test. |
| A2 | Alt+Enter while the New Issue form Title field is focused still dispatches `NewIssueSubmit` (regression guard). | Behavioral key-routing test. |
| A3 | Ctrl+Enter while the New Issue form Title field is focused still dispatches `NewIssueSubmit` (regression guard, terminal-portable compat). | Behavioral key-routing test. |
| A4 | Plain Enter while the New Issue form Body field is focused still dispatches `NewIssueBodyNewline` (unchanged). | Behavioral key-routing test. |
| A5 | Plain Enter on selection fields (Template/Type/Labels/Milestone/Project/Assignees) still dispatches `NewIssueFocusNext` (unchanged). | Behavioral key-routing test. |

## Non-goals

- Do not change the Alt+Enter / Ctrl+Enter submit contract.
- Do not change the inline editor / inline composer (non-new-issue) Enter
  behavior — that path is already correct (issue #265).
- Do not change the rendered hint text (`Alt+Enter submit` is accurate).
- Do not add a new event type or public abstraction.
- Do not touch the submit pipeline, property-apply pipeline, or persistence.
- Do not modify `.llxprt/`, dependencies, quality-gate scripts, or CI config.
- Do not refactor unrelated key-routing code.

## Vertical slices

1. **Title Enter no longer submits** (A1–A5) — single RED→GREEN slice:
   - Add behavioral tests in `src/app_input/issues_key_tests.rs` proving
     plain Enter in Title advances focus (not submit), and that Alt/Ctrl+Enter
     and Body/selection-field Enter are unchanged.
   - Fix the `Title` arm of `resolve_new_issue_enter` to return
     `AppEvent::NewIssueFocusNext`.

## Expected files

| File | Change | Est. net lines |
|------|--------|----------------|
| `src/app_input/issues.rs` | Title arm → `NewIssueFocusNext` | +1 / -1 |
| `src/app_input/issues_key_tests.rs` | new behavioral tests + helper | ~+90 |

**Estimated total: 2 files, ~90 net added lines.** Well under the 25-file /
1,500-line target; no scope-review trigger.

## Scope ledger

| File | Change | Reason |
|------|--------|--------|
| `src/app_input/issues.rs` | 1-line fix in `resolve_new_issue_enter` | A1 root cause |
| `src/app_input/issues_key_tests.rs` | add New Issue form Enter tests | A1–A5 evidence |

No newly discovered work. No out-of-scope files.

## Review counters

- OCR before PR: 0 / 2
- OCR after PR: 0 / 2

## Verification

- `cargo xtask quick` (fmt + check + test) during iteration.
- Full `cargo xtask ci` (fmt check, clippy-allow policy, source-size,
  architecture, strict + complexity clippy, coverage ≥ 30%, locked
  all-feature build + test) on the green checkpoint.
- New behavioral tests pass; existing `issues_key_265_tests.rs` and
  `new_issue_form_ops_tests.rs` still pass.
