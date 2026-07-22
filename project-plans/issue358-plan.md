# Issue #358: Selecting an issue fails with Unknown JSON field state_reason due to wrong gh --json field name

## Root Cause

`GhClient::get_issue_detail` in `src/github/mod.rs` builds the `gh issue view
--json ...` argument list using the snake_case field name `state_reason`. The
`gh` CLI does **not** recognize `state_reason` — it expects the camelCase
`stateReason` for `--json` output. Pressing Enter on any issue in the issues
list triggers this command, which fails with `Unknown JSON field: "state_reason"`
and blocks the user from viewing any issue detail.

The parser (`parse_issue_state_reason` in `src/github/parse.rs`) already handles
both `stateReason` and `state_reason` JSON keys, so no parser change is needed.
Only the **request** (the `--json` field list) is wrong.

Introduced by PR #349 (commit `30402b5`), which added `state_reason` to the
field list by copying the REST API shape instead of the `gh` CLI shape.

## Acceptance Matrix

| # | Actor/Path | Input | Success Behavior | Failure Behavior | Test |
|---|-----------|-------|-----------------|-----------------|------|
| A1 | `gh issue view` field list | `ISSUE_DETAIL_JSON_FIELDS` constant | Contains `stateReason` (camelCase), not `state_reason` | — | unit |
| A2 | Issue detail `--json` command args | `build_issue_detail_json_args` builder output | Asserts `stateReason` token present, `state_reason` absent | — | unit |
| A3 | `get_issue_detail` uses the builder | Code review / same field constant | Single source of truth for the field list | — | — |
| A4 | Closed issue detail display | Closed issue with `COMPLETED`/`NOT_PLANNED`/`DUPLICATE` | Reason renders correctly (parser already handles both key spellings) | — | existing state_reason tests |
| A5 | Existing parsing unchanged | All existing state_reason parse tests pass | No regression | — | existing tests |

## Non-Goals

- Changing how state reason is parsed or displayed (already correct).
- Modifying the GraphQL issue-search query (already uses camelCase `stateReason`).
- Touching the close-issue mutation path.
- Changing the `gh issue view` invocation beyond the field name fix.

## Vertical Slices

### Slice 1: RED → GREEN (single commit)
- **Files**: `src/github/mod.rs`, `src/github/tests/issues.rs`
- **Change**:
  1. Extract the comma-separated `--json` field list from
     `get_issue_detail` into a named constant `ISSUE_DETAIL_JSON_FIELDS`.
  2. Fix `state_reason` → `stateReason` in that constant.
  3. Add `build_issue_detail_json_args` builder returning the `Vec<&str>` of
     field tokens (testable without a live `gh` call).
  4. Add a regression test asserting `stateReason` is present and
     `state_reason` is absent in the builder output.
- **Tests**: `src/github/tests/issues.rs` — `test_issue_detail_json_fields_use_camel_case`

## Scope Ledger

| Date       | Item          | Type |
|------------|---------------|------|
| 2026-07-22 | Initial plan  | —    |

## Review Counters
- Local OCR: 0/2
- PR OCR: 0/2

## Verification
- `make quick-check` during iteration
- `make ci-check` before push
