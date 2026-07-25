# Issue 408 delivery plan

## Issue and baseline

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/408
- Branch: `issue408`
- Base: `origin/main` at `462cb13`
- Reported behavior: the multiline textbox only exposes five rows in the New Issue flow, does not visibly indicate hidden rows above or below its caret-following viewport, and should remain reusable at parent-selected dimensions.
- Discussion: the only issue comment links the merged Issues-to-TextBox migration; it adds no requirements.

## Usage audit and screen proposal

The reusable `TextBox` is rendered only by `ui::components::detail_pane`. Its current callers are:

1. Issues New Issue.
2. Issues New Comment and Reply.
3. Pull Requests New Comment and Reply.

`TextBoxProps` already receives explicit `viewport_rows` and `content_width`; there is no internal five-row default. The fixed behavior comes from the detail parents reserving `DETAIL_COMPOSER_VIEWPORT_ROWS` for every active composer. The detail screens already derive width from their parent pane.

Accepted proposal:

- **Adjust Issues New Issue:** preserve its four static guidance rows when space permits and allocate every remaining detail-body row to the textbox, while guaranteeing one editable row on constrained non-empty viewports.
- **Keep Issues comment/reply at five rows:** the surrounding issue body/comment and reply anchor are required context, so this parent intentionally restricts its textbox.
- **Keep PR comment/reply at five rows:** the surrounding PR/review context and reply anchor are likewise more useful than an expanded draft by default.
- **Apply scroll indicators to all TextBox uses:** every contextual composer benefits when its parent-selected viewport hides wrapped or logical rows.
- **Keep parent-selected width for all uses:** reserve the indicator gutter inside the supplied content width so wrapping and right-edge placement remain deterministic.

The schema-1 runner was evaluated first as required for new scenarios, but its direct-PTY input reached the harness probe and did not reach Jefe's iocraft terminal-event stream after initialization. Changing the runner would be a quality-tool change outside this issue. This issue therefore **updates the existing pre-schema textbox scenario** (rather than adding a new scenario) and runs it through the established multiplexer-backed real Jefe harness with isolated state and the existing fail-closed GitHub shim.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure / diagnostics | Permitted side effects | Persistence / compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | User enters Issues and opens New Issue | Tall detail pane; draft from empty through more than five logical or wrapped rows | Local TUI; platform-independent projection/layout | Static guidance remains visible when space permits; textbox occupies all remaining detail-body rows, so a tall pane shows substantially more than five draft rows | Existing Issues error/banner behavior is unchanged; no new failure path | Render only until existing submit action | No schema change; draft and submit semantics unchanged | updated multiplexer-backed real-process scenario asserts the first and eighth draft rows are simultaneously visible; pure projection test asserts exact row allocation |
| A2 | Any Issues or PR detail composer | Text above viewport, text below viewport, both directions available, no overflow, one-row viewport, wrapped text, zero rows/width | All platforms | A textual up arrow is right-aligned when rows exist above; a down arrow is right-aligned when rows exist below; both are represented when both directions are available; no arrows when all content fits | No panic or overflow at zero/tiny dimensions | Render only | Existing caret-following and text content remain unchanged | `text_box_view` unit tests for display-row scrollability; component render tests for visible/right-aligned arrows and fit case |
| A3 | Parent embeds TextBox through a detail pane | Different explicit row counts and widths, including narrow widths | All platforms | Parent-selected row count is rendered exactly; supplied content width bounds prefix, wrapped text, caret, and indicator gutter; there is no horizontal scrolling | Tiny widths degrade to blank text cells without panic | Render only | Existing public props remain source-compatible; no new dependency | detail-pane/component render tests and existing zero-width/wrap tests |
| A4 | User composes an issue comment/reply or PR comment/reply | Tall and constrained panes | All platforms | Contextual composers remain parent-restricted to at most five rows, continue reserving a read-only document/anchor row where available, and gain A2 indicators | Existing document scrolling/focus diagnostics remain unchanged | Render only | Existing scroll-bound and focus behavior remains unchanged | focused layout/state tests plus existing issue/PR composer regressions |

## Explicit non-goals

- No horizontal scrolling or left/right indicators.
- No visual marker solely for line wrapping.
- No manual textbox scroll-offset state, mouse wheel ownership, or new reducer events; the existing caret-following local viewport remains authoritative.
- No expansion of issue comment/reply or PR comment/reply composers beyond their intentional five-row parent policy.
- No change to issue/PR submit, editing, focus, pagination, persistence, GitHub I/O, or keybindings.
- No dependency, workflow, agent-memory, `.llxprt/`, `.code_puppy/`, quality-gate, or persistence-schema changes.
- No refactor of `ScrollableText` or property editors, which do not use `TextBox`.
- No migration of the pre-schema harness or addition/change to schema-1 runner code; the existing textbox scenario is updated in place because changing the runner is outside scope.

## Bounded vertical slices

### Slice S1: parent-sized New Issue composer

- Acceptance rows: A1, A3, A4.
- Architecture owner: pure Issues detail projection; integration boundary is `IssueDetailProjectionInputs -> DetailPaneProps`.
- Allowed production paths: `src/ui/components/issue_detail.rs`; `src/layout.rs` only if a shared row-allocation helper is required by tests/state consistency.
- Allowed evidence paths: `tests/text_box_layout.rs`, `src/ui/components/issue_detail_render_tests.rs`, `dev-docs/tmux-scenarios/issues-composer-textbox.json`, and this plan.
- RED: update the existing real-Jefe textbox scenario first; add a projection/render test requiring an eight-row draft to remain simultaneously visible in a tall New Issue pane and exact small-pane allocation.
- GREEN: New Issue assigns guidance rows plus all remaining rows to its textbox; contextual composers retain five rows.
- REFACTOR: keep row-allocation decisions pure and parent-owned; do not add stateful layout behavior.
- Verification: focused issue-detail tests, feature scenario, `make quick-check`.
- Stop conditions: new layout subsystem, public abstraction not listed here, state-event changes, or paths outside this slice.

### Slice S2: display-row-aware textbox scroll indicators

- Acceptance rows: A2, A3.
- Architecture owner: `text_box_view` pure projection; integration boundary is the thin `ui::components::text_box` renderer.
- Allowed production paths: `src/text_box_view.rs`, `src/ui/components/text_box.rs`.
- Allowed evidence paths: `tests/text_box_layout.rs`, unit tests in the thin renderer module, and this plan.
- Planned public contract addition: expose total wrapped display-row count and pure `can_scroll_up` / `can_scroll_down` queries on `TextBoxView`; no new module or subsystem.
- RED: add projection tests for up/down/both/fit and renderer tests for arrow visibility at the right edge within the parent width.
- GREEN: a fixed two-column indicator gutter is included inside `content_width`; top/bottom arrows reflect hidden wrapped display rows without changing caret-following.
- REFACTOR: keep symbols/rendering in the component and all visibility decisions iocraft-free.
- Verification: focused text-box and detail-pane tests, `make quick-check`.
- Stop conditions: stored/manual scrolling, reducer input changes, horizontal scrolling, dependency changes, or an unplanned public abstraction.

## Expected paths by layer

| Layer | Expected paths | Acceptance mapping |
| --- | --- | --- |
| Pure textbox projection | `src/text_box_view.rs` | A2, A3 |
| Thin textbox renderer | `src/ui/components/text_box.rs` | A2, A3 |
| Issues parent layout projection | `src/ui/components/issue_detail.rs` | A1, A4 |
| Shared layout contract, only if needed | `src/layout.rs` | A1, A4 |
| Behavioral projection evidence | `tests/text_box_layout.rs` | A1-A4 |
| Thin renderer evidence | unit tests in `src/ui/components/text_box.rs` | A2, A3 |
| Real-TTY evidence | `dev-docs/tmux-scenarios/issues-composer-textbox.json` | A1 |
| Delivery record | `project-plans/issue408-plan.md` | all rows |

Expected scope: at most 7 changed files and under 500 net changed lines. The plan deliberately stays within UI projection/render and test-evidence layers.

## Scope ledger

| Discovery | Disposition | Rationale / follow-up |
| --- | --- | --- |
| TextBox already has explicit row and width props | In-scope design clarification | Preserve that composable contract; correct the parent allocation rather than inventing a replacement component. |
| Five rows originate in shared detail layout, not a TextBox default | In-scope fix | New Issue receives a distinct fill-available policy; contextual comment/reply parents retain five rows. |
| TextBox viewport follows the caret and stores no manual offset | In-scope constraint | Indicators describe hidden display rows around the caret-derived window; manual independent scrolling is explicitly out of scope. |
| Width wrapping uses text width after the prefix | In-scope fix | Reserve the indicator gutter before projection so total output remains bounded by the parent width. |
| Direct schema-1 PTY input did not drive Jefe's iocraft event stream | Defer | Updating the harness would be a quality-tool change. Update the existing textbox scenario and use the multiplexer-backed runner; schema-1 Jefe input support remains with the harness migration effort. |
| Existing `issues-composer-textbox.json` depended on live issue data | In-scope evidence fix | Reuse the established isolated config and fail-closed issue-list shim so the updated New Issue flow has no network or developer-state dependency. |
| Property editor has its own editor projection and does not embed TextBox | Reject | Outside the audited component's usage and issue scope. |
| PR OCR: prefix width used scalar count instead of terminal-cell width | In-scope—Fix | Prefix width is part of A3's parent width budget. Use the established `unicode-width` dependency and add a wide-prefix renderer regression. The shared text wrap remains intentionally scalar-count based by its documented contract. |
| PR OCR: arrow right-edge assertions use scalar indices | In-scope—Fix | The shipped arrows and ASCII fixture are one-cell symbols, but the new wide-prefix regression now exercises the variable-width boundary that the original assertions do not cover. |
| PR OCR: New Issue ignored a contextual reservation argument | In-scope—Fix | Separate fill-available and contextual row-allocation helpers so New Issue does not compute or accept the irrelevant fixed-composer reservation. The duplicate fourth finding shares this disposition. |
| Exact-head PR OCR: New Issue guidance height used logical lines | In-scope—Fix | A1 preserves guidance when space permits, including narrow widths. Allocate from the shared wrapped-document display-row count and add a RED/GREEN narrow-width integration test. |
| Exact-head PR OCR: wide body text can exceed a scalar-count wrap budget | Defer | The shared `text_wrap` contract intentionally uses Unicode scalar values and is shared by TextBox and ScrollableText. Changing it is a broader public behavior shift outside A1-A4; follow-up issue #422 records the shared design/test work. |

No unapproved scope discoveries are open.

## Review counters

- Pre-PR Open Code Review: 2 / 2 (both invocations terminated by signal 15 without output; no findings available to triage)
- Post-PR Open Code Review: 2 / 2 (four findings on `a069823` and two findings on `50910ff`; all classified, in-scope findings fixed, and the shared wide-body contract deferred to #422)

## Verification evidence

| Candidate head | Command / evidence | Result |
| --- | --- | --- |
| `462cb13` | source audit | Baseline: New Issue, Issues comments/replies, and PR comments/replies all inherit a five-row detail-composer allocation; TextBox has no scroll-direction indicators |
| `462cb13` | attempted schema-1 New Issue scenario | Blocked before target assertion: direct-PTY key steps did not reach Jefe's iocraft input stream; no harness code changed |
| `462cb13` | updated multiplexer-backed New Issue scenario | RED: step 21 could not find `row-one-visible` after the eighth row moved the fixed five-row viewport |
| working tree | updated multiplexer-backed New Issue scenario with isolated state and fail-closed `gh` shim | PASS: 27 steps; rows one and eight were simultaneously visible in the 160x40 New Issue pane |
| working tree | `cargo test --test text_box_layout`; focused TextBox, issue-detail, Issues composer, and PR composer tests | PASS: parent row allocation, wrapped-display direction queries, right-edge arrows, and unchanged contextual composer behavior |
| working tree | `make quick-check` | PASS |
| `a069823` | `make ci-check` | PASS: format, policy, source size, Clippy/complexity, coverage, locked all-feature build, and full tests |
| `a069823` | PR CI | PASS: build, test, format, lint, policy, source size, complexity, coverage, and native Windows; optional TUI smoke skipped by design |
| working tree | focused tests after PR OCR fixes | PASS: TextBox renderer (including wide prefix) and `text_box_layout` integration tests |
| `50910ff` | PR CI | PASS: build, test, format, lint, policy, source size, complexity, coverage, and native Windows; optional TUI smoke skipped by design |
| working tree | narrow-guidance RED/GREEN and focused tests | RED on logical-line allocation; PASS after shared wrapped-display-row allocation (4 layout tests, 2 renderer tests) |
| working tree | `make ci-check` after PR OCR fixes | PASS: format, policy, source size, Clippy/complexity, coverage, locked all-feature build, and full tests |

## Deferred findings and follow-ups

- Schema-1 runner input compatibility with Jefe's iocraft terminal-event stream remains with the existing harness migration effort; no harness production or quality-tool code changed in this issue.
- Shared terminal-cell-aware body wrapping is tracked by #422; it requires a coordinated `text_wrap`, TextBox caret, and ScrollableText selection contract rather than an issue-408-only patch.
