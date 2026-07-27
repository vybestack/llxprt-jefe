# Issue #429 — Align ScrollableText inline caret offsets with renderer indexing

## Problem

The ScrollableText inline-editor caret projection reports terminal-cell
offsets for wrapped rows, while the renderer indexes row text by Unicode
scalar position. Wide glyphs can therefore shift the rendered inline caret.

### Root cause

`src/domain/document_wrap.rs::caret_row_for_line_col` is the projection used
by the inline-editor caret placement. Its second return value
(`col_within_row`) is currently computed as a **terminal display-cell width**
via `UnicodeWidthStr::width(prefix.as_str())`.

Its single consumer, `src/ui/components/scrollable_text.rs::cursor_row_element`,
indexes the row text by **Unicode scalar position** via
`chars.iter().take(cursor_col)` / `skip(cursor_col)`. It then renders the
caret cell, and the caret's `Box` cell consumes whatever scalar the renderer
sliced out.

These two coordinate spaces diverge for two glyph classes:

- **Wide (CJK/emoji) glyphs** — display width 2, scalar width 1. The
  projection counts 2 cells per glyph; the renderer slices 1 scalar per
  index unit. The caret is painted too far right.
- **Zero-width (combining marks) glyphs** — display width 0, scalar width 1.
  The projection skips them; the renderer still slices one scalar. The caret
  drifts by one for each combining mark preceding it.

The renderer is the authority: `cursor_row_element` slices `chars` by scalar
position to paint the exact glyph under the caret. The projection must
therefore return the **char offset relative to the row's `line_char_start`**,
matching the renderer's scalar indexing. Wrapping and selection mapping stay
terminal-cell-aware and char-based respectively — neither changes.

## Decision (accepted approach)

**Contract:** `caret_row_for_line_col` returns
`(global_row_index, char_offset_within_row)`, where `char_offset_within_row`
is the 0-based Unicode scalar offset of the caret column relative to the
row's `line_char_start`. This matches the renderer's
`chars.iter().take(cursor_col)` indexing in `cursor_row_element` exactly.

This is the minimal, contract-aligning fix. It:

- keeps wrapping terminal-cell-aware (`wrap_document` / `wrap_text` are
  unchanged);
- keeps selection mappings char-based (`row_highlight` / `clip_range_to_row`
  are unchanged);
- keeps the renderer's scalar-based `cursor_row_element` slicing as-is;
- changes only the projection's second return value from a cell width to a
  char offset, plus adds a doc comment naming the contract.

## Non-goals

- Changing issue 422's TextBox caret or ScrollableText selection contract
  (explicit non-goal from the issue text).
- Changing `cursor_row_element`'s scalar-based rendering (it is the
  coordinate authority).
- Changing `display_cell_to_char_offset`, `viewport_cell_to_content`, or any
  other terminal-cell→char reverse-map used by mouse selection.
- Changing `inline_cursor_line_start`/`inline_cursor_line_end` (state-layer
  byte-cursor movement, separate from the render projection).
- Changing the `DetailContent.cursor` coordinate contract (it stays
  `(content_line, char_column)` — already scalar-based).
- Adding new modules, dependencies, public abstractions, or production
  helpers beyond the one-line projection fix and its doc.
- Touching `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifests,
  quality-gate scripts, or unrelated tests/docs.

## Acceptance matrix

| # | Behavior | Evidence |
|---|----------|----------|
| AC1 | `caret_row_for_line_col` returns a **char offset** relative to the row's `line_char_start` for an ASCII/wrapping case (regression guard). | Pure unit test in `document_wrap.rs` asserts `(row, char_rel)` for the existing "alpha bravo charlie" case. |
| AC2 | For a wrapped CJK row, the caret at a column inside the wide-glyph row maps to the **char offset** (not the cell width). CJK glyph (display width 2) at caret position maps to char offset, not 2× scalar. | Pure unit test: `wrap_document("甲乙丙", 4)` → caret at col 2 → row 1 ("丙"), char offset 0 (NOT cell width 0 coincidentally; use a row with mixed content). |
| AC3 | For a row containing a combining mark, the caret column maps to a **char offset** that accounts for the combining mark as a scalar. A combining mark (`e\u{301}`) preceding the caret adds 1 to the char offset (display width 0 but scalar 1). | Pure unit test on a constructed row with a combining mark. |
| AC4 | The rendered caret cell in `cursor_row_element` lands on the correct glyph for CJK content (renderer-level regression guard for AC2). | Renderer test in `scrollable_text.rs` (the `#[cfg(test)]` module) renders a CJK line with an inline caret and asserts the inverse-video cell covers the intended glyph via the existing ANSI-stripping helper. |
| AC5 | No regression to ASCII caret placement (existing `caret_row_for_line_col_finds_wrapped_subrow` expectations stay correct because char offset == cell width for ASCII). | The existing ASCII test continues to pass unchanged (its assertions are `(1,2)`, `(0,0)`, `(3,2)` — these are char offsets, which equal cell widths for ASCII). |
| AC6 | No change to selection mapping or wrapping (`row_highlight`, `clip_range_to_row`, `wrap_document` untouched). | Diff inspection: only `caret_row_for_line_col` body + doc change in `document_wrap.rs`; no change to selection/wrap functions. Existing `non_overlapping_selection_paints_no_highlight_on_row` + CJK wrap tests stay green. |
| AC7 | No change to the `ScrollableTextProps` `cursor_col`/`cursor_line` public prop types or the `DetailContent.cursor` coordinate contract. | Diff inspection: no prop/struct type changes. |

## Vertical slices

This is a single-slice change: one pure-projection function contract
alignment + its doc, with RED→GREEN tests in two existing test modules. It
does not cross multiple architectural ownership layers (it lives entirely in
the `domain` projection + its renderer consumer, which already shares the
`doc_wrap` module as the single source of truth).

1. **Projection contract alignment** — `caret_row_for_line_col` returns char
   offset instead of cell width; add contract doc comment. Implements AC1,
   AC2, AC3, AC5. RED tests added in `document_wrap.rs` test module.
2. **Renderer regression guard** — add a CJK caret renderer test in
   `scrollable_text.rs` test module proving the caret paints on the correct
   glyph (AC4). (No production change; the renderer is already correct once
   AC1-AC3 land.)

## Expected paths / files

- `src/domain/document_wrap.rs` — fix `caret_row_for_line_col` to return char
  offset + add contract doc (production change, ~1-3 net lines).
- `src/ui/components/scrollable_text.rs` — add CJK/combining-mark caret
  renderer regression test(s) in the existing `#[cfg(test)]` module (test-only).
- `project-plans/issue429-plan.md` — this plan (documentation only).

## Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test -p <crate> document_wrap` (focused)
- `cargo test -p <crate> scrollable_text` (focused)
- `make ci-check` (full gate: fmt, clippy allow policy, source size,
  complexity, coverage ≥30, build, test) before pushing the green checkpoint.

## Scope ledger

| Date | Item | Disposition |
|------|------|-------------|
| 2026-07-26 | Initial scope: align `caret_row_for_line_col` char-offset contract + add CJK/combining-mark coverage. | Accepted |
| 2026-07-26 | Change `cursor_row_element` renderer slicing to be cell-aware instead of scalar-based. | Rejected — renderer scalar indexing is the coordinate authority; changing it would move the bug rather than fix it and would also touch the selection contract (out of scope). |
| 2026-07-26 | Change `display_cell_to_char_offset` / `viewport_cell_to_content` (mouse selection reverse-map). | Rejected — issue explicitly preserves the selection contract; mouse→content mapping is selection-side. |

## Review counters (OCR)

- Local OCR runs before PR: 1 / 2
- OCR runs after PR opened: 1 / 2

(Cap: two local + two PR per issue/PR effort.)

## OCR review triage

### Local run (pre-PR, head `46276b3`)

0 findings ("Looks good to me").

### PR run (head `2528d32`)

1 finding (`src/ui/components/scrollable_text.rs:706-711`, maintainability/medium):
the new CJK caret regression test hard-coded the ANSI SGR literals
`\u{1b}[48` / `\u{1b}[7m`, duplicating the same detection already present in
the existing `non_overlapping_selection_paints_no_highlight_on_row` test;
suggested extracting a shared helper.

**Disposition: In-scope-Fix.** Factually correct: both tests live in the
same module touched by this PR and both encoded the renderer color-encoding
assumption inline. Extracted `contains_highlight_sgr(line)` and routed both
the caret test and the selection test through it so the format assumption
lives in one place. Remediated in commit `2528d32`.

## Verification evidence

Local (candidate head `2528d32`):

- `cargo fmt --all --check` — pass
- `scripts/check-clippy-allows.sh` — pass
- `scripts/check-source-file-size.sh` — pass (warnings only, pre-existing)
- clippy `-D warnings` — pass
- clippy complexity gate (`-D cognitive_complexity -D too_many_lines …`) — pass
- coverage `--fail-under-lines 30` — pass (72.11% lines)
- `cargo build --workspace --all-features --locked` — pass
- `cargo test --lib` — pass (all 3 new tests green; no regressions)

PR #450 CI (exact head `2528d32`):

- Format (rustfmt) — SUCCESS
- Lint (clippy) — SUCCESS
- Clippy allow policy — SUCCESS
- Source file length checks — SUCCESS
- Complexity checks — SUCCESS
- Coverage gate — SUCCESS (72.11%)
- Build — SUCCESS
- Test — SUCCESS
- OpenCodeReview (CI job) — SUCCESS
- Native Windows (MSVC + psmux) — FAILURE (pre-existing flake; see below)
- CodeRabbit — skipped (excluded by label configuration)
- PR state: `mergeable: MERGEABLE`, `mergeStateStatus: UNSTABLE`. Branch
  `main` is NOT protected (no required status checks), so the Windows job is
  a signal-only check, not a merge gate.

### Native Windows (MSVC + psmux) — proven pre-existing flake

The Windows job failed across 5 consecutive runs, each on a **different**
process/timing-sensitive smoke test (no two runs failed the same test):

| Run | Failing test | Timeout |
|-----|--------------|---------|
| 1 | `psmux_four_recording_agents_remain_independent_and_scoped` | 30s spawn readiness |
| 2 | `psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys` | 30s byte echo |
| 3 | `guarded_dashboard_reorder_tui_scenario` | 15s `waitFor` |
| 4 | `psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys` | 30s byte echo |
| 5 | `guarded_real_dashboard_lists_window_fixture_rows` + `guarded_real_jefe_qqq_quits` | 30s `waitFor` |

All failures are `waitFor`/spawn-readiness timeouts in `tests/psmux_smoke*.rs`
and `src/harness/runner_tests.rs` — harness tests that spawn real jefe/tmux/psmux
subprocesses and wait for them to reach a state within a fixed budget. None of
them import or touch `document_wrap.rs` or `scrollable_text.rs` (the only
files this PR changes).

Cross-branch evidence: the same `Native Windows (MSVC + psmux)` job failed on
unrelated branches `issue264` (run 30217009450) and `issue407` (run
30210215055) on the same family of timing tests. The failing test changes
every run — the signature of runner-resource-contention flakiness, not a
deterministic code defect. This matches the documented pre-existing flake set
(the `harness::runner::tests::guarded_real_*` and psmux spawn tests).

The Unix-side `Test` job (which runs the full lib + integration suite on
Linux) passes on the candidate head, and `cargo test --lib` passes locally.

## Deferred findings / follow-ups

- The `Native Windows (MSVC + psmux)` job's process/timing smoke tests are a
  repository-wide flakiness concern (affects multiple branches) and is out of
  scope for this PR. A follow-up could raise the `waitFor` budgets or add
  retry/backoff to the psmux/harness smoke tests, but that is a testing-
  infrastructure change, not part of the caret-alignment acceptance matrix.

## Stopping conditions

Stop and ask before: adding any new module/abstraction/dependency, changing
selection/wrap/state-layer contracts, moving tests, or exceeding the hard
scope budget (40 files / 2,500 net lines).
