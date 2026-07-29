# Issue 436 delivery plan

## Issue and baseline

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/436
- Branch: `issue436`
- Branch-start base: `main` at `4faf76ab`
- Reported behavior: PR #427 (issue #422) introduced two avoidable copies and
  one redundant scalar traversal while extracting TextBox wrapped-row
  projection helpers:
  1. `project_line_segments` borrows a local `Vec<WrapSegment>` and clones
     every segment `String` into `TextBoxRow` even though the segment vector
     is immediately discarded.
  2. `project_line_segments` already computes `rendered_chars`, but
     `caret_col_for_segment` recounts the same segment
     (`seg.text.chars().count()`) when clamping the caret.
  3. The later visible-row step clones `display_rows[disp_idx].clone()` into
     the padded viewport even though `display_rows` could be consumed in place.
- Origin: deferred OpenCodeReview findings from PR #427 / issue #422. The two
  OCR comments about the scalar recount are duplicate reports of the same
  expression and are handled as one root cause.
- Constraint: these are performance and ownership-quality improvements, NOT
  correctness defects. No behavior change is permitted.

## Chosen contract

`project_line_segments` consumes `Vec<WrapSegment>` by value (moving each
segment `text` into the produced `TextBoxRow`) instead of borrowing and
cloning. The already-computed `rendered_chars` scalar is threaded into the
caret-clamp path so `caret_col_for_segment` no longer recomputes
`seg.text.chars().count()`. The flat `display_rows` buffer is consumed
in place (drain/truncate/pad) when building the fixed-size viewport so the
final visible rows are moved rather than cloned.

All public APIs (`build_text_box_view`, `TextBoxView`, `TextBoxRow`,
`TextCaret`, `WrapRow`, `wrap_text`), text/caret/viewport/wrap-boundary/gutter
behavior, dependencies, lint/complexity/safety/coverage, and CI remain
unchanged. No new public abstraction, dependency, unsafe code, or behavior
change.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure / diagnostics | Permitted side effects | Persistence / compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | `build_text_box_view` pure projection (all existing callers) | Empty text; short line; long line that wraps; CJK/wide glyphs; combining marks; exact-cell-width trailing caret; one-row and multi-row viewports; trailing newline; zero width; zero viewport | All platforms; pure projection | Identical `TextBoxView` output (rows, caret columns, `first_visible_row`, `total_lines`, `total_display_rows`) to the pre-change baseline for every input | Any difference in row text, caret column, viewport anchor, or row count | None | Public API, types, and behavior unchanged | All pre-existing `text_box_view` unit tests pass unchanged; regression snapshot of representative fixtures |
| A2 | TextBox iocraft renderer (`ui::components::text_box`) and its callers | Wide body before gutter; wrap-boundary caret; wide prefix; scroll indicators | Local TUI; iocraft renderer | Rendered output identical to baseline (gutter visible, caret reversed, no overflow) | Any rendered difference | Render only | Existing `TextBoxProps` and reducer behavior unchanged | Pre-existing TextBox renderer tests pass unchanged |
| A3 | Integration `text_box_layout` suite | Multi-line scrolling, wrapped rows, zero viewport | Library integration | Viewport anchoring and total counts unchanged | Any count/anchor difference | None | Integration contract unchanged | `tests/text_box_layout.rs` passes unchanged |
| A4 | Ownership quality | `project_line_segments` input segment vector | Pure projection | `WrapSegment` values are consumed (moved) into `TextBoxRow`; no `.clone()` of segment `String` in the projection | A remaining `.clone()` on segment text in `project_line_segments` | None | N/A | Source inspection: no `seg.text.clone()` remains in `project_line_segments` |
| A5 | Scalar reuse | `caret_col_for_segment` clamp | Pure projection | The clamp uses the already-computed `rendered_chars` instead of recomputing `seg.text.chars().count()` | A remaining `seg.text.chars().count()` in `caret_col_for_segment` | None | N/A | Source inspection: no `chars().count()` recount in `caret_col_for_segment` |
| A6 | Visible-row ownership | `display_rows` -> fixed viewport | Pure projection | The visible rows are moved (drain/truncate/pad) rather than `.clone()`d | A remaining `display_rows[disp_idx].clone()` | None | N/A | Source inspection: no `display_rows[…].clone()` remains |

## Explicit non-goals

- No behavior change of any kind: identical projection output for every input.
- No public API, type, dependency, manifest, lockfile, workflow,
  agent-memory, `.llxprt/`, `.code_puppy/`, quality-gate, or harness change.
- No new public abstraction, module, or trait.
- No unsafe code (the crate forbids it; `[lints.rust] unsafe_code = "forbid"`).
- No lint suppression or complexity/size threshold change.
- No grapheme/cell coordinate model change; source/caret offsets remain scalar.
- No unrelated refactor or test relocation. The deferred
  `src/text_box_view.rs` > 750-line warning split (issue #422 scope ledger)
  remains out of scope.
- No change to `text_wrap::wrap_text` or `WrapRow`.
- No horizontal scrolling, gutter redesign, or wrap-mode change.

## Bounded vertical slices

This issue is a single cohesive ownership refactor of one pure projection
module. It does not cross more than three architectural ownership layers (it
touches only `src/text_box_view.rs`). Per ISSUE-DELIVERY §3 it is delivered as
one slice because the three changes are inseparable within the same ownership
flow (consuming the segment vector is what enables the in-place viewport
build).

### Slice S1 (only slice): preserve ownership through TextBox projection

- **Acceptance rows:** A1, A2, A3, A4, A5, A6
- **Architecture owner:** iocraft-free `text_box_view` pure projection.
  Integration boundary: the private `project_line_segments`,
  `caret_col_for_segment`, and `build_text_box_view` viewport-build steps.
- **Allowed production paths:** `src/text_box_view.rs` only.
- **Allowed evidence paths:** existing unit tests in `src/text_box_view.rs`,
  `src/ui/components/text_box.rs`, `tests/text_box_layout.rs`, and this plan.
- **RED:** The refactor preserves behavior, so the existing comprehensive
  suite IS the regression net. Before refactoring, capture a golden snapshot
  of `build_text_box_view` output for representative inputs (empty, short,
  wrapped, CJK, combining, exact-width trailing caret, multi-line scroll,
  trailing newline, zero width, zero viewport). After refactoring, assert the
  output is byte-identical. If any fixture diverges, that is RED.
- **GREEN:** (1) Change `project_line_segments` to take `Vec<WrapSegment>` by
  value and move each `seg.text` into the `TextBoxRow`. (2) Thread
  `rendered_chars` into `caret_col_for_segment` and remove the
  `seg.text.chars().count()` recount. (3) Replace the
  `display_rows[disp_idx].clone()` viewport build with an in-place
  drain/truncate/pad that moves rows. All existing tests pass unchanged.
- **REFACTOR:** Keep each function under project complexity/size limits; no
  signature expansion beyond threading the already-computed scalar.
- **Verification:** focused `text_box_view`, TextBox renderer, and
  `text_box_layout` suites; `cargo fmt --all --check`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `cargo build --workspace --all-features --locked`;
  `cargo test --workspace --all-features --locked`.
- **Stop conditions:** any behavior difference; a new public
  abstraction/module; a dependency/workflow/agent-memory/quality-tool change;
  a path outside `src/text_box_view.rs`; unsafe code; lint suppression.

## Expected paths by layer

| Layer | Expected paths | Acceptance mapping |
| --- | --- | --- |
| TextBox pure projection | `src/text_box_view.rs` | A1, A4, A5, A6 |
| TextBox thin renderer (evidence only) | `src/ui/components/text_box.rs` | A2 |
| Integration contract (evidence only) | `tests/text_box_layout.rs` | A3 |
| Delivery record | `project-plans/issue436-plan.md` | all rows |

Expected scope: 1 changed production file (`src/text_box_view.rs`) plus this
plan. Well under the 25-file / 1,500-line targets.

## Scope ledger

| Discovery | Disposition | Rationale / follow-up |
| --- | --- | --- |
| `src/text_box_view.rs` is above the 750-line warning threshold but below the 1000-line hard gate (deferred from issue #422) | Preserve / Defer | Splitting the module is unrelated to this ownership refactor and remains a separate bounded follow-up. |
| The two OCR comments about the scalar recount are duplicates of the same expression | Reject (duplicate) | Handled as one root cause per the issue origin note. |
| `caret_at_full_width_end` and `caret_in_hidden_suffix` already receive `rendered_chars`/`rendered_width` and do not recount | Preserve | Only `caret_col_for_segment` has the redundant recount. |
| The visible-row clone requires consuming `display_rows` in place | In-scope fix | Drain the visible window, truncate/pad to `viewport_rows`. |

No unapproved scope discoveries are open.

## Review counters

- Pre-PR Open Code Review: 1 / 2 (local run completed: 0 comments, "Looks good to me")
- Post-PR Open Code Review: 1 / 2 (CI OpenCodeReview job completed: 1 inline comment, triaged below)
- Independent Rust/DeepThinker review cycles: 0 / 2

## Review triage

| Finding | Classification | Resolution |
| --- | --- | --- |
| CI OCR: `project_line_segments` signature changed from `&[WrapSegment]` to `Vec<WrapSegment>`, future callers passing a reference will fail to compile | Reject | This is the intended design of the fix, not a defect. (1) `project_line_segments` is a private function with no public API surface, no submodules, and no external callers — the OCR search itself confirmed only one call site. (2) The by-value signature IS what issue #436 requires: consuming the segment vector is what enables each segment String to be moved into TextBoxRow instead of cloned. (3) The Rust compiler enforcing ownership at compile time is a safety feature. Comment replied to and resolved. |

## Verification evidence

| Candidate head | Command / evidence | Result |
| --- | --- | --- |
| `4faf76ab` (main) | baseline `text_box_view` (31), TextBox renderer (4), `text_box_layout` (5) | PASS: all 40 baseline tests green before any change. |
| issue436 working tree | focused `text_box_view` (31), TextBox renderer (4), `text_box_layout` (5) | PASS: identical counts after refactor. |
| issue436 working tree | source inspection: no `seg.text.clone()`, no `chars().count()` recount, no `display_rows[…].clone()` | PASS: all three clones removed. |
| issue436 working tree | `cargo fmt --all --check` | PASS. |
| issue436 working tree | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS. |
| issue436 working tree | `cargo build --workspace --all-features --locked` | PASS. |
| issue436 working tree | `cargo test --workspace --all-features --locked` | PASS: 2574 lib tests + all integration suites; one flaky `settings_edit_fixture` fails only under full-workspace parallel load and passes in isolation on both main and this branch (pre-existing fixture-parallelism flake, unrelated to the pure-projection change). |

## Deferred findings and follow-ups

- `src/text_box_view.rs` > 750-line warning split: deferred from issue #422,
  still out of scope here.
