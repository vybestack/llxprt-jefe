# Issue #406 — text boxes should support home/end/arrows/etc

## Problem

Text-editing surfaces in the TUI only handle a limited key set. The issue
author cannot use `Home`/`End` to jump to the start/end of the line, and the
"full common set" of line-editing keys is not uniformly supported across the
various composer/editor inputs.

The codebase already supports: chars, Backspace, Delete, Left, Right, Up,
Down (vertical movement). It is **missing**: `Home`, `End`, and the common
`Ctrl-A`/`Ctrl-E` Emacs-style line anchors (single-line text fields only —
the multiline composer maps `Ctrl-A` is unused today; we only add it to
single-line surfaces where Home/End map cleanly to "start/end of text").

## Acceptance matrix

For each text-editing surface, the **common set** must include the existing
keys (chars, Backspace, Delete, Left, Right, Up, Down where applicable) PLUS
`Home` and `End`.

The semantics of `Home`/`End` differ by surface kind:

| Surface | Home semantics | End semantics |
|---|---|---|
| Multiline inline composer/editor (issues + PRs) | Start of current line | End of current line |
| Single-line title editor (issues + PRs property Title) | Start of title text | End of title text |
| Form text fields (New/Edit Repo, Agent, WorkflowDispatch) | Start of field | End of field |
| Search input (issues + PRs, close-reason duplicate search) | Start of query | End of query |
| Filter text controls (issues + PRs) | Start of text field | End of text field |

### Rows

#### R1 — Multiline inline composer/editor (Issues)
- Actor: user typing in the Issues inline composer or inline editor.
- Input: `Home`, `End` keys (no modifiers).
- Success: `Home` moves the byte cursor to the start of the current logical
  line; `End` moves it to the end of the current logical line. Preserves
  char-boundary safety (multibyte).
- Failure: n/a (pure state transition).
- Test: `state/issues_inline_ops` tests assert byte cursor after Home/End on
  multiline text including a multibyte line.

#### R2 — Multiline inline composer/editor (PRs)
- Mirrors R1 for the PR inline composer/editor (NewComment, Reply).
- Test: `state/prs_tests_cursor_arrows` (or a sibling test module) asserts
  Home/End byte cursor on PR composer.

#### R3 — Single-line title editor (Issues property)
- Input: `Home`, `End`.
- Success: cursor → 0 (Home) or → title_text.len() (End).
- Test: `state/issues_property_ops_tests` asserts cursor position.

#### R4 — Single-line title editor (PRs property)
- Mirrors R3 for PR property Title editor.
- Test: `state/prs_property_ops_tests` asserts cursor position.

#### R5 — Form text fields (Repo/Agent/WorkflowDispatch)
- Input: `Home`, `End`.
- Success: cursor → 0 / → field length for the focused field.
- Test: `state/form_ops_tests` asserts Home/End on a representative field.

#### R6 — Key routing (modal handler)
- Input: `Home`/`End` keys dispatched to `handle_mode_form_key`.
- Success: `Home` → `FormMoveCursorStart`, `End` → `FormMoveCursorEnd`.
- Test: `app_input/modal_handlers_tests` (or a sibling) asserts the mapping.

#### R7 — Key routing (issues inline)
- Input: `Home`/`End` keys.
- Success: routes to `InlineCursorHome` / `InlineCursorEnd`.
- Test: `app_input/issues_key_tests` asserts the routing.

#### R8 — Key routing (PRs inline)
- Mirrors R7 for PR inline key router.
- Test: `app_input/prs_key_tests` asserts the routing.

#### R9 — Key routing (issues/PRs property Title editor)
- Input: `Home`/`End` keys while the property editor Title kind is open.
- Success: routes to `...TitleCursorHome` / `...TitleCursorEnd`.
- Test: `app_input/issues_property_key_tests`,
  `app_input/prs_property_key_tests`.

#### R10 — Key routing (search inputs)
- `Home`/`End` move within the query. For the single-line search inputs we
  model the cursor implicitly (they store the full query string and pop/append
  at the end today). This is out of scope to refactor to a real cursor; we
  treat Home/End as **no-op** for search inputs (consumed but no movement) to
  avoid changing the storage model. **NON-GOAL** — see Non-goals.

### Non-goals

- **No real cursor model for search inputs.** The search input stores a
  `query: String` and only supports append/pop at the end. Adding Home/End
  would require a cursor field and a wider refactor. Search inputs already
  support Backspace (pop) which is the common case; Home/End are left as
  no-ops for search.
- **No Emacs chords (Ctrl-A / Ctrl-E).** While common, adding them risks
  colliding with existing Ctrl-A semantics and is not requested by the issue
  body (which specifically names Home/End/arrows). The arrow keys already
  work; we add Home/End.
- **No word-movement (Ctrl-Left/Right, Alt-Left/Right).** Out of scope.
- **No mouse selection / clipboard integration.** Out of scope.
- **No changes to the close-reason duplicate-search digit-only input.** It
  only accepts ASCII digits; Home/End are not meaningful there.
- **No changes to filter control text fields beyond wiring Home/End to
  move within the field** — but the filter text fields ALSO use the
  append/pop model today, so they are treated like search (NON-GOAL).
- **No new dependencies, no workflow/CI/quality-tool changes.**

## Vertical slices

### Slice 1 — Multiline composer/editor Home/End (issues + PRs)
- Acceptance rows: R1, R2, R7, R8.
- Architecture owner: `state` (reducer) + `app_input` (key routing) +
  `messages` (event conversion for PRs).
- Allowed files:
  - `src/state/util.rs` (add `inline_cursor_line_start`/`_line_end`)
  - `src/state/issues_inline_ops.rs` (handle new events)
  - `src/state/prs_inline_ops.rs` (handle new events)
  - `src/state/events.rs` (add `InlineCursorHome`, `InlineCursorEnd`)
  - `src/messages.rs` (add `PrInlineMsg::CursorHome`, `CursorEnd`)
  - `src/messages/prs_conversion.rs` (wire conversion)
  - `src/messages/issues_conversion.rs` (wire conversion — Issues inline is
    a flat AppEvent today, so conversion may be a no-op; verify)
  - `src/messages/event_conversion.rs` (verify)
  - `src/messages/names.rs` / `message_names.rs` (names)
  - `src/app_input/issues.rs` (route Home/End)
  - `src/app_input/prs.rs` (route Home/End)
  - `src/messages/issues_dispatch.rs` (verify)
  - new/sibling test files for state assertions
- RED: state test asserting cursor moves to line start/end (fails: events
  don't exist).
- GREEN: add events + reducers + routing.

### Slice 2 — Title editor Home/End (issues + PRs)
- Acceptance rows: R3, R4, R9.
- Allowed files:
  - `src/state/issues_property_ops.rs`
  - `src/state/prs_property_ops.rs`
  - `src/state/events.rs` (add Title cursor Home/End variants)
  - `src/messages.rs` (property conversion)
  - `src/messages/*_property_conversion.rs`
  - `src/app_input/issues.rs` / `prs.rs` (route)
  - sibling test files.

### Slice 3 — Form text field Home/End
- Acceptance rows: R5, R6.
- Allowed files:
  - `src/state/util.rs` (`move_cursor_start`/`_end` already trivial; reuse
    `clamp_cursor`/`chars().count()`)
  - `src/state/form_cursor.rs` (per-field start/end)
  - `src/state/form_workflow_dispatch.rs`
  - `src/state/form_ops.rs` (handle_form_move_cursor_start/end)
  - `src/state/events.rs` (`FormMoveCursorStart`, `FormMoveCursorEnd`)
  - `src/messages.rs` (`ModalMessage` variants) + conversion
  - `src/app_input/modal_handlers.rs` (route Home/End)
  - `src/state/modal_op.rs` (dispatch)
  - sibling test files.

### Scope ledger

- All additions are within the text-editing subsystem; no new public
  abstraction, no new dependency, no workflow/CI/quality-tool change.
- Estimated magnitude: ~600–900 net LoC across ~18 files (within the 25-file
  / 1,500-line target).

### Review counters

- Local OCR runs: 0 / 2 (pre-PR).
- PR OCR runs: 0 / 2 (post-PR).

### Verification

- `make quick-check` during iteration.
- `make ci-check` before pushing the green checkpoint.
- Focused tests: `cargo test -p jefe --lib state::issues_inline`,
  `state::prs_tests_cursor_arrows`, `state::issues_property_ops_tests`,
  `state::prs_property_ops_tests`, `state::form_ops_tests`,
  `app_input::issues_key_tests`, `app_input::prs_key_tests`,
  `app_input::modal_handlers_tests`.
