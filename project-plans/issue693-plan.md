# Issue #693 — New Issue form swallows the first line (the subject)

## Report

> First line of new issue is swallowed on windows. When I typed that subject the
> first time it went away.. I hit enter and typed it again. It shouldn't get
> swallowed the first time. I do not think this happens on Mac.
> So apparently the first line doesn't go away it just isn't visible after you
> type it...it does become the subject....but disappearing is confusing.

The reporter's own correction is the important part: the text is **not lost**, it
becomes the subject. It stops being **visible**.

## Root cause

The New Issue form has eight fields (`NewIssueFormState`), but the detail pane
renders only two things:

1. `build_new_issue_content` — a fixed four-line document
   (`"New Issue"`, a stale `Title: first line | Body: remaining lines` hint from
   before #407 split the draft into fields, a blank line, `[Composer input]`).
   It ignores the form entirely.
2. one embedded `TextBox` composer whose text comes from
   `new_issue_composer_from_form`, which renders **only the currently focused
   field**.

So the repro chain is:

- the form opens focused on Title (#454);
- the user types the subject — visible in the composer;
- the user presses Enter, which on Title dispatches `NewIssueFocusNext` (#480);
- focus becomes Body, so the composer now renders the empty `body_text`;
- the typed title vanishes from the screen even though `form.title_text` still
  holds it and it does become the created issue's subject.

This is a **rendering** defect and is platform-independent; nothing in the path
is Windows-specific. The "not on Mac" impression is incidental (it depends on
whether the user pressed Enter after the subject). Chasing a Windows input bug
is explicitly **out of scope** — no evidence supports one.

## Acceptance matrix

| # | Actor / input | Boundary cases | Success behavior | Failure behavior | Proof |
|---|---|---|---|---|---|
| A1 | User types a title, then moves focus off Title (Enter or Tab) | focus = Body, Labels, Milestone, Project, Assignees, Template, Type | the typed title text stays visible in the New Issue document | n/a (pure projection) | unit test on the content builder for every focus value |
| A2 | User types a body, then moves focus off Body | single-line and multi-line body; template scaffold body | the typed body stays visible, continuation lines indented under the label | n/a | unit test |
| A3 | Any focus | each of the 8 `NewIssueFormFocus` values | exactly one rendered field row is marked focused (`> `), all others `  ` | n/a | unit test |
| A4 | Unset optional fields | type/labels/milestone/project/assignees empty | render a stable `(none)` placeholder rather than a ragged empty row | n/a | unit test |
| A5 | Rendered rows | any field content | rows are ASCII-only (no emoji), consistent with the New PR composer | n/a | unit test mirroring `the_composer_text_is_emoji_free` |
| A6 | Detail pane render with the form open, focus = Body | title typed earlier | the title text appears in the rendered pane output | n/a | render test in `issue_detail_render_tests.rs` |
| A7 | TUI harness, Issues → `n` → type title → `tab` → type body | 120x40 pinned terminal | the title is still present in the frame after focus leaves Title | scenario assertion fails | `dev-docs/tmux-scenarios/issues-new-issue-typing.json` |
| A8 | State/UI line-count invariant | any form | the document the state layer counts is the same document the UI renders (one builder, one caller) | n/a | preserved by construction; `build_new_issue_content` keeps its single call site |

## Non-goals

- No keymap, event, message, or reducer changes. The focus/Enter semantics of
  #480 stay exactly as they are.
- No Windows-specific input handling (`KeyEventKind`, ConPTY) changes — no
  evidence of a platform defect.
- Not making the picker fields (Type/Labels/Milestone/Project/Assignees)
  editable, and not changing `new_issue_composer_from_form`'s picker→Title
  composer fallback.
- No new overlay, component, or public abstraction; no change to the New PR
  form; no `form.error` / `options_loading` surfacing (separate concern).
- No windowing/scrolling scheme for the New Issue document beyond the existing
  `new_issue_composer_rows` math.
- No dependency, workflow, quality-gate, or agent-memory changes.

## Slices

### S1 — the form becomes visible in the document (A1–A5)

- Owner: content projection (`src/issue_detail_content.rs`).
- RED: new `src/issue_detail_content_tests.rs` asserting the title survives a
  focus change, body continuation rows, focus marking, placeholders, ASCII.
- GREEN: `build_new_issue_content` takes `Option<&NewIssueFormState>` and emits
  one row per field in tab order, mirroring `new_pr_form_rows`, before the
  `[Composer input]` anchor.

### S2 — wire the form through the render path (A6, A8)

- Owner: UI component (`src/ui/components/issue_detail.rs`).
- RED: render test proving the typed title is present with focus on Body.
- GREEN: `new_issue_composer_content` accepts the form and forwards it; the
  existing `build_new_issue_content_renders_static_prompt_only` unit test is
  rewritten to the new contract.

### S3 — regression scenario (A7)

- Owner: TUI harness scenario.
- The existing `issues-new-issue-typing.json` types `HelloTitle`, presses `tab`,
  and never re-asserts the title. Add that assertion.

## Scope ledger

| File | Acceptance rows |
|---|---|
| `src/issue_detail_content.rs` | A1–A5, A8 |
| `src/issue_detail_content_tests.rs` (new, test-only) | A1–A5 |
| `src/ui/components/issue_detail.rs` | A6, A8 |
| `src/ui/components/issue_detail_render_tests.rs` | A6 |
| `dev-docs/tmux-scenarios/issues-new-issue-typing.json` | A7 |

## Review counters

- Local OCR runs: 1 / 2 (self review of the full production diff + new test file)
- PR OCR runs: 0 / 2

## Verification evidence

Candidate head, run on `issue693`:

- `cargo fmt --all --check` — exit 0.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — exit 0, no
  diagnostics.
- `cargo test --workspace --all-features --locked` — every unit/lib suite green; the
  integration suite reports `421 passed; 1 failed`. The single failure is
  `ui::dashboard_reorder_tui::guarded_dashboard_reorder_tui_scenario`
  (`HarnessError E006: frame did not contain 'alpha' within 15000ms`), reproduced on a
  clean `git stash -u` tree at the same merge base, so it is pre-existing and unrelated
  to this change.
- `cargo xtask ci` — fmt, check-clippy-allows, check-source-size (warnings only, all
  pre-existing >750-line files), check-architecture, check-multiplexer-surface, lint and
  complexity all pass; `coverage` exits 1 solely because the pre-existing failure above
  makes `cargo llvm-cov --fail-under-lines 30` exit 101.

Behavioral evidence per acceptance row lives in
`src/issue_detail_content_new_issue_tests.rs` (projection), 
`src/ui/components/issue_detail_render_tests.rs` (render path), and
`dev-docs/tmux-scenarios/issues-new-issue-typing.json` (TUI scenario).

## Deferred findings

- `ui::dashboard_reorder_tui::guarded_dashboard_reorder_tui_scenario` fails on main at
  this merge base. Out of scope for #693; needs its own issue rather than a drive-by fix
  here.
