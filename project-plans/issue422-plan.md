# Issue 422 delivery plan

## Issue and baseline

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/422
- Branch: `issue422`
- Base: `origin/main` at `edeff48`
- Reported behavior: the shared `text_wrap` primitive budgets Unicode scalar values, while the terminal grid and the independently implemented `doc_wrap` projection budget terminal cells. TextBox can therefore place wide text or an end caret into its fixed right-side indicator gutter, and the advertised shared contract can drift from ScrollableText.
- Discussion: the only issue comment is an automated related-PR/planning placeholder and adds no requirements.
- Origin constraint: issue 408 explicitly deferred the shared wrapping change while accepting terminal-cell-aware prefix measurement and fixed-width TextBox indicators.

## Chosen contract

`text_wrap::wrap_text` remains the one public wrapping primitive and changes its width unit from Unicode scalar count to terminal display cells as measured by the existing `unicode-width` dependency. `WrapRow.start` and `WrapRow.end` remain half-open Unicode-scalar source offsets. This keeps editor and selection coordinates stable while making every displayed row fit the physical grid.

The existing ScrollableText behavior defines the narrow-glyph compatibility details to preserve while removing its duplicate wrapper:

- break at whitespace when the next word would exceed the cell budget;
- hard-break an overlong word at the largest scalar boundary that fits;
- retain zero-cell combining marks on the same row as a fitting base scalar;
- render a one-cell ellipsis for a glyph that cannot fit even on an empty nonzero-width row, while retaining its source range;
- preserve explicit-newline, blank-line, trailing-space range, and `width == 0` behavior.

TextBox caret positions remain Unicode-scalar source positions. A caret at the source end of a row whose rendered terminal width exactly fills the budget uses the existing trailing-caret-row rule when at least two viewport rows are available. ScrollableText selection remains content-line plus Unicode-scalar-column based; terminal-cell hit testing continues to map through each shared wrapped row.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure / diagnostics | Permitted side effects | Persistence / compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | Any pure wrapping consumer calls `text_wrap::wrap_text` | Narrow ASCII, CJK/wide scalars, combining marks, overlong words, an overwide glyph in a one-cell row, spaces, explicit newlines, zero width | All platforms; pure projection | Every nonzero-width display row occupies at most `width` terminal cells; narrow-text row text/ranges stay compatible; source ranges remain Unicode-scalar offsets and contiguous within each logical line; combining marks consume zero cells; an unrenderable wide scalar uses a finite one-cell placeholder and remains source-mappable | No panic or non-advancing loop; zero width returns the established empty projection | None | No dependency or public type addition; `WrapRow` fields retain their source-offset meaning | RED/GREEN `text_wrap` unit tests for CJK, combining, narrow compatibility, overwide fallback, row bounds, and `row_for_column` |
| A2 | User edits an Issues or PR composer rendered by TextBox | Wide glyph before/at a wrap boundary, combining sequence, caret inside a wide-text row, caret at the source end of an exact-cell-width row, one-row and multi-row viewports | Local TUI; platform-independent pure projection plus iocraft renderer | TextBox rows follow A1; caret maps to the shared row using source-scalar offsets; an exact-cell-width end caret follows the established trailing-row/single-row policy; wide body text does not enter the fixed two-cell indicator gutter and indicators stay at the parent right edge | Degenerate width suppresses the caret as before; no overflow/panic | Render only | Existing `TextBoxProps`, reducer cursor, viewport, and indicator behavior remain compatible | Updated real-Jefe textbox scenario is RED then GREEN; `text_box_view` CJK/combining/boundary tests; TextBox renderer right-gutter regression |
| A3 | User views or selects wrapped issue/PR/help text through ScrollableText | Same CJK, combining, narrow, whitespace, overwide, and newline fixtures as A1; terminal-cell clicks in and beyond wide/combining rows; visible scrollbar | All platforms; pure projection and iocraft renderer | `doc_wrap` delegates row formation to A1 rather than maintaining a second algorithm; content-line/scalar ranges and terminal-cell-to-selection mapping remain correct; wide rows do not consume the fixed scrollbar column | Unknown/past rows and cells retain current clamping behavior; no mapping panic | Render and selection projection only | Content-line scroll offsets and selection coordinate schema are unchanged | `doc_wrap` delegation/mapping tests and ScrollableText wide-row/right-scrollbar renderer regression |
| A4 | Existing non-UI consumer projects wrapped Actions detail text | Narrow and wide labels/status text | All platforms; pure projection | The existing consumer automatically receives A1 and emits cell-bounded rows without a parallel wrapper | No new failure path | None | Existing Actions APIs and status symbols remain unchanged | Shared primitive tests plus focused existing Actions projection suite |

## Explicit non-goals

- No grapheme-cluster cursor model, grapheme-aware editing commands, normalization, or dependency addition. Source and caret offsets remain Unicode-scalar based; caret positions adjacent to zero-cell combining marks may share one physical terminal boundary.
- No locale-dependent ambiguous-width or pictographic-emoji policy change; `unicode-width` remains authoritative and the existing emoji-free UI policy remains in force.
- No horizontal scrolling, gutter redesign, new wrap mode, hyphenation, markdown wrapping rewrite, or changed manual/vertical scrolling behavior.
- No state, reducer, keybinding, persistence, GitHub I/O, runtime, process, or terminal-driver changes.
- No public module/type addition. The existing public `wrap_text`, `WrapRow`, TextBox, ScrollableText, and selection contracts are updated in place.
- No dependency, manifest, lockfile, workflow, agent-memory, `.llxprt/`, `.code_puppy/`, quality-gate, or harness-runner changes.
- No unrelated refactor or test relocation.

## Bounded vertical slices

### Slice S1: one terminal-cell-aware shared wrapper

- Acceptance rows: A1, A3, A4.
- Architecture owner: iocraft-free `text_wrap` pure projection; integration boundary is existing consumers of `WrapRow`.
- Allowed production paths: `src/text_wrap.rs`, `src/ui/components/doc_wrap.rs`.
- Allowed evidence paths: unit tests in those modules and this plan.
- RED: add shared CJK/combining/overwide/cell-bound tests that fail against scalar budgeting; add a ScrollableText document assertion that rows equal the shared primitive while preserving line-local selection ranges.
- GREEN: move terminal-cell row formation into `text_wrap`; make `doc_wrap::wrap_document` adapt shared rows into content-line rows and retain only document/selection mapping concerns.
- REFACTOR: remove the duplicate wrapping algorithm and keep each function under project complexity/size limits.
- Verification: focused library tests for `text_wrap`, `doc_wrap`, mouse reverse mapping, Actions detail; `make quick-check`.
- Stop conditions: a new public abstraction/module, dependency change, selection schema change, or behavior outside A1/A3/A4.

### Slice S2: TextBox caret boundary and fixed gutters

- Acceptance rows: A2.
- Architecture owner: `text_box_view` pure projection; integration boundary is the thin TextBox renderer.
- Allowed production paths: `src/text_box_view.rs`; `src/ui/components/text_box.rs` only for renderer evidence or a minimal renderer correction proven necessary by A2.
- Allowed evidence paths: tests in those modules, `dev-docs/tmux-scenarios/issues-composer-textbox.json`, and this plan.
- RED: update the existing real-Jefe textbox scenario first with a CJK boundary fixture whose tail joins the wide glyph only under cell-aware wrapping; prove failure on the baseline. Add pure tests for CJK rows, combining source offsets, and exact-cell-width trailing caret, plus a renderer test that the wide body stays left of the fixed gutter.
- GREEN: use rendered terminal width for TextBox full-row/caret-boundary decisions; consume A1 without a TextBox-specific wrapper.
- REFACTOR: keep source-scalar caret mapping separate from terminal-cell capacity and preserve the existing parent-owned geometry.
- Verification: focused TextBox projection/renderer/integration tests, updated real-TTY scenario, `make quick-check`.
- Stop conditions: grapheme-aware input redesign, gutter/layout subsystem changes, state/reducer changes, or paths outside this slice.

## Expected paths by layer

| Layer | Expected paths | Acceptance mapping |
| --- | --- | --- |
| Shared pure wrapping contract | `src/text_wrap.rs` | A1-A4 |
| TextBox pure projection | `src/text_box_view.rs` | A2 |
| TextBox thin renderer/evidence | `src/ui/components/text_box.rs` | A2 |
| ScrollableText document/selection adapter | `src/ui/components/doc_wrap.rs` | A3 |
| ScrollableText thin renderer evidence | `src/ui/components/scrollable_text.rs` | A3 |
| Real-TTY evidence | `dev-docs/tmux-scenarios/issues-composer-textbox.json` | A2 |
| Delivery record | `project-plans/issue422-plan.md` | all rows |

Expected scope: at most 7 changed files and under 600 net changed lines. The change stays in one pure projection ownership layer plus the two existing thin UI adapters/evidence paths.

## Scope ledger

| Discovery | Disposition | Rationale / follow-up |
| --- | --- | --- |
| `doc_wrap` is already terminal-cell aware but duplicates row formation despite ScrollableText documentation saying it is built on `text_wrap` | In-scope fix | Promote its established cell-width semantics into the shared primitive, then delegate from `doc_wrap`; this directly satisfies the one-contract outcome. |
| TextBox prefix width and two-column indicator gutter are already terminal-cell aware from issue 408 | Preserve | Only body-row capacity and exact-width caret detection remain inconsistent; no gutter redesign is needed. |
| `WrapRow` source ranges and editor/selection models are Unicode-scalar based | In-scope constraint | Preserve source offsets while changing only the capacity unit to terminal cells. |
| `unicode-width` is already a direct dependency used by both relevant UI paths | Reuse | No dependency or manifest change. |
| Actions detail also consumes `text_wrap` | In-scope compatibility | It receives the corrected shared behavior automatically; no separate production path is planned. |
| Existing pre-schema textbox scenario is the established real-Jefe path because schema-1 input does not currently drive Jefe's iocraft stream | Preserve | Update and run the existing scenario; changing harness tooling remains out of scope. |
| Review reproduced a caret in source-only dropped whitespace on a full TextBox row | Blocker—Fix | Move the caret to the next shared display row, or a trailing empty row at the document end, so it cannot consume the fixed indicator gutter; add pure and renderer regressions. |
| Review reproduced a ScrollableText selection over source-only trimmed whitespace replacing the scrollbar with a synthetic blank | Blocker—Fix | Do not invent a display cell for an empty clipped span; retain the shared row and fixed scrollbar, with a renderer regression. |
| Review found ScrollableText inline-editor caret mapping returns terminal cells to a scalar-indexed renderer | Defer | Valid pre-existing inline-editor mismatch outside issue 422's accepted TextBox-caret and ScrollableText-selection contract; track it separately rather than expanding this PR. |
| OCR reported `row_for_column` could underflow before a row start | Reject | The reviewed implementation already uses `col.saturating_sub(row.start)`; the reported subtraction path is not present. |
| Physical-width docs and one Actions assertion still used character terminology/counts | In-scope—Fix | Update the touched contracts to terminal-cell terminology and measure the existing Actions projection with `UnicodeWidthStr`. |
| `src/text_box_view.rs` exceeds the 750-line warning threshold but remains below the 1000-line hard gate | Defer | Moving unrelated tests or splitting the established projection is outside issue 422 and requires a separately bounded refactor. |

No unapproved scope discoveries are open.

## Review counters

- Pre-PR Open Code Review: 2 / 2 (one explicit local run and one run observed by the independent Rust reviewer; no further local OCR runs permitted)
- Post-PR Open Code Review: 0 / 2
- Independent Rust/DeepThinker review cycles: 1 / 2 (DeepThinker and Rust reviewer completed the same stable checkpoint review cycle)

## Review triage

| Finding | Classification | Resolution |
| --- | --- | --- |
| TextBox caret can enter its fixed gutter at dropped wrap whitespace | Blocker—Fix | Remediated with next-row/trailing-row caret projection and pure/renderer tests. |
| ScrollableText can synthesize a selected blank over its scrollbar | Blocker—Fix | Remediated by rendering only actual selected row text and preserving the scrollbar in a renderer test. |
| ScrollableText wide inline-editor caret mixes terminal-cell and scalar offsets | Defer | Valid existing mismatch, but inline-editor caret rendering is outside this issue's accepted ScrollableText selection scope; follow-up issue records it. |
| Delivery plan lacked final evidence/triage | Blocker—Fix | This section and the verification table record factual results; interrupted commands are not recorded as passes. |
| Width terminology and Actions assertion remained character-based | In-scope—Fix | Updated touched API documentation and Actions physical-width assertion. |
| OCR `row_for_column` underflow | Reject | Factual mismatch with the saturating subtraction in the reviewed source. |
| Change source coordinates to grapheme/cell coordinates | Reject | Contradicts the accepted scalar source/caret/selection contract and issue non-goals. |
| Split oversized `text_box_view.rs` | Defer | Valid warning-level maintainability work outside this bounded behavior change. |

## Verification evidence

| Candidate head | Command / evidence | Result |
| --- | --- | --- |
| `edeff48` | source and history audit | Baseline: `text_wrap` counts scalars; `doc_wrap` independently counts cells; TextBox exact-width caret detection counts scalars. |
| scalar-wrap baseline | updated real-Jefe `issues-composer-textbox.json` | RED: the boundary row retained the preceding ASCII scalar before `界cell-tail`, so the expected CJK row prefix was absent. |
| implementation checkpoint | focused shared-wrapper, document mapping, TextBox, ScrollableText, and Actions tests | GREEN: CJK, combining marks, overwide fallback, scalar mappings, and fixed gutters passed. |
| `cedb3c5` | `make quick-check` | PASS: format, check, library/integration tests, and doctests. |
| `cedb3c5` | real `jefe-tmux-harness` with isolated config and fail-closed GitHub shim | PASS: all 31 steps; CJK begins the next physical row and the right-side gutter remains visible. |
| pre-remediation candidates | `make ci-check` attempts | INCOMPLETE: external SIGTERM while cargo/build locks were contended; not counted as verification passes. |
| working tree after review remediation | exact focused B1/B2 regressions | PASS. |
| working tree after review remediation | `make quick-check` | PASS: 2,167 library tests passed, one ignored; all integration suites and doctests passed. |
| working tree after review remediation | strict all-target/all-feature Clippy | Issue-touched code passed after refactoring; the command remains blocked by new Rust 1.97 `manual_is_multiple_of` diagnostics in unchanged `src/runtime/process.rs` and `src/harness/v1/validate.rs`. |
| working tree after review remediation | real `jefe-tmux-harness` with isolated config and fail-closed GitHub shim | PASS: all 31 steps. |

## Deferred findings and follow-ups

- ScrollableText inline-editor caret mapping returns terminal-cell offsets to a scalar-indexed renderer. This is valid but outside issue 422's accepted ScrollableText selection contract and is tracked by issue #429.
- `src/text_box_view.rs` is above the 750-line warning threshold but below the enforced 1000-line hard limit. A cohesive split would move unrelated tests and is deferred from issue 422.
