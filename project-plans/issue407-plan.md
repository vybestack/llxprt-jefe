# Issue #407: New-issue dialog supporting properties + templates (like GitHub)

## Problem

Today the new-issue composer (`OpenNewIssueComposer`) is a single inline
multiline text editor where the first line becomes the title and the
remaining lines become the body. It supports **no properties**: the user
cannot set labels, type, milestone, projects, priority, start/target dates,
effort, or relationships (parent / blocked-by / blocking / security alert)
from the create flow — all of those require creating the issue first and then
opening the per-property editors one at a time.

GitHub's web "New issue" experience shows a combo/template chooser (Bug,
Feature, Task, Blank, repo-defined templates), then a form with the title,
body, and all properties on the same screen. Issue #407 asks for the same
experience in the TUI: a combo-box up top that selects the template/type and
switches the layout, then all properties editable before submit, with
milestone and plan sticky (defaulting to whatever was last used in that
project), nothing required, and everything blankable.

## Desired Outcome

- Pressing `n`/`N` from the Issues list opens a **New Issue dialog** (a
  `ModalState::NewIssue` full-screen form modal, mirroring the existing
  `NewAgentForm`/`NewRepositoryForm` convention — not a new `ScreenMode`) that
  shows, on one screen:
  - a combo-box at the top to pick a **template** (Blank / Bug / Feature /
    Task / repo-defined issue templates). Switching the template prefills the
    title/body and the type (when the template maps to a type).
  - the **title** (single-line) and **body** (multiline) editors.
  - all **properties**: labels (multi), type (single), priority, milestone
    (single, sticky), projects (multi), start date, target date, effort, and
    relationships (add parent, blocked-by, blocking, security alert).
  - **none required**, all blankable.
  - **milestone and plan are sticky**: they default to whatever was last used
    in the current project (per-repo `RepoPreferences`).
- Submit (`Alt+Enter`, mirroring the existing composer) creates the issue via
  `gh api --method POST /repos/{owner}/{repo}/issues` with the title + body,
  then applies the selected properties (labels, type, milestone, etc.) using
  the existing `edit_properties` machinery against the created issue's node
  id. This keeps a single create surface and reuses the proven mutation
  helpers.

The issue author requested an **initial design / mockup** be shown to the
user for approval before implementation. The ASCII mockup is in the
"Mockup" section below.

## Architectural decisions that need user confirmation

These are surfaced explicitly per the bounded-delivery workflow (acceptance
language that permits materially different architectures must be resolved
before implementation).

1. **Dialog vs inline composer.** The issue says "This might mean adding a
   dialog for new or an initial like combo box that changes the layout ...
   Actually I think I like the combo ... we can switch if the template gets
   in the way (i.e. switch to blank)". This plan adopts a **full-screen form
   modal** (`ModalState::NewIssue` + `ui/screens/new_issue.rs`, mirroring the
   existing `NewAgentForm`/`NewRepositoryForm` convention — **not** a new
   `ScreenMode`, so `build_screen_element`/`ScreenMode` are untouched) because
   a single inline composer cannot host a multi-field form with combo boxes,
   multi-select option lists, and date inputs without a major re-architecture
   of the inline composer. This is the simplest reuse of the proven form-modal
   pattern and keeps the scope within the dialog approach the issue leans
   toward. **STOPPING HERE for user approval** of the form-modal approach.

2. **Templates.** GitHub repo issue templates come from
   `.github/ISSUE_TEMPLATE/*.md` (classic) plus the `issueTemplates` GraphQL
   connection (for the newer YAML front-matter templates with `type`/labels
   prefills). The plan proposes fetching the repo's issue templates via
   `gh api graphql` (`issueTemplates(first:50)`) and synthesizing the
   built-in Bug/Feature/Task/Blank presets client-side (matching the issue's
   "dialog for bug/feature/blank or whatever is easier"). Built-in presets
   prefill title prefix + body scaffolding and (when the repo has a matching
   issue type) set the type. **This is the larger unknown** — needs user
   confirmation that synthesizing Bug/Feature/Task client-side is acceptable
   vs. only listing repo-defined templates.

3. **Property support matrix.** Not every property has a clean `gh`/REST
   endpoint today. Concretely:
   - **Supported via existing `edit_properties` + `create_issue`** (Slice A):
     labels (multi, via `--add-label`), milestone (single, via
     `--milestone`), type (single, via `updateIssue` GraphQL after fetch of
     node id), assignees (multi, via `--add-assignee`).
   - **Supported via existing REST/GraphQL but NOT yet wired** (Slice B):
     projects (multi, via the `addProjectV2` GraphQL mutation — requires a
     project node id fetch).
   - **Not exposed by `gh`/REST in a simple way** (Slice C / deferred):
     priority, start date, target date, effort, and relationships
     (parent / blocked-by / blocking / security alert). These are GitHub
     Projects V2 item fields and the new sub-issue / dependency-tracking
     GraphQL mutations, which are not part of the issue REST API and would
     require a non-trivial GraphQL write surface (and, for relationships,
     the `addSubIssue`/`createIssueDepend` mutations). The plan proposes
     **deferring Slice C** to a follow-up issue and shipping Slice A (with
     optional Slice B for projects) as the first PR. **STOPPING HERE for
     user confirmation** of the slice split.

4. **Sticky "plan".** The issue says "milestone, and plan should be sticky".
   Jefe does not have a first-class "plan" entity today; the closest analog
   is the per-issue `project-plans/issueNNN-plan.md` document the workflow
   itself generates. The plan proposes interpreting "plan" as the **project**
   (Projects V2) selection — i.e. milestone + project are the two sticky
   per-repo defaults. **Needs user confirmation** that "plan" == "project",
   or whether a separate plan concept is intended.

5. **Per-repo stickiness.** `RepoPreferences` (issue #163) already persists
   per-repo filter/search/merge-method. The plan extends `RepoPreferences`
   with `last_new_issue_milestone: Option<String>` and
   `last_new_issue_project_ids: Vec<String>` (and, if Slice C is approved,
   date/effort/priority defaults). Defaults are restored when the dialog
   opens and remembered on submit.

## Mockup

The proposed New Issue dialog (modal screen). This is the screen the user
will see after pressing `n` from the Issues list.

```
┌─ New Issue ──────────────────────────────────────────────────────────────┐
│                                                                          │
│  Template        [ Bug          ]  (space cycles: Blank / Bug / Feature / │
│                                    Task / <repo templates>)              │
│  Type            [ Bug          ]  (space cycles repo issue types)        │
│                                                                          │
│  Title           [ Fix the flange gasket                                ] │
│                                                                          │
│  Body (Alt+Enter submit, multiline):                                    │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ ## What happened?                                                │   │
│  │                                                                  │   │
│  │ ## Steps to reproduce                                            │   │
│  │ 1.                                                               │   │
│  │ 2.                                                               │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  Labels          [ bug, ui ]                    (L to edit, multi)       │
│  Milestone       [ v1.2       ]  (sticky)      (M to edit, single)      │
│  Project         [ Backend    ]  (sticky)      (P to edit, multi)       │
│  Assignees       [ alice, bob ]                (A to edit, multi)       │
│                                                                          │
│  ── Deferred to follow-up (Slice C) ─────────────────────────────────── │
│  Priority        [        ]   Start date    [          ]                │
│  Target date     [        ]   Effort        [          ]                │
│  Parent          [        ]   Blocked by    [          ]                │
│  Blocking        [        ]   Security alert[          ]                │
│                                                                          │
│  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space cycles  │
│  L labels  M milestone  P project  A assignees  Alt+Enter submit  Esc    │
└──────────────────────────────────────────────────────────────────────────┘
```

Keybind footer (matches the project's emoji-free, keybind-bar convention):

```
Template/Type/Labels/Milestone/Project/Assignees  Tab/Up/Dn nav  Space cycle  Alt+Enter submit  Esc cancel
```

When the user presses `L` (Labels) while the dialog is open, the existing
`IssuePropertyEditorState` overlay is **not** reused directly (it is bound to
an existing issue number); instead the dialog's own multi-select picker
reuses the pure `build_property_editor_view` projection and the
`PropertyOption` model, but persists the selection into the dialog's draft
fields rather than mutating an existing issue. The same approach is used for
Milestone/Project/Assignees/Type pickers.

## Acceptance matrix

| # | Actor / path | Input | Success behavior | Failure behavior | Test |
|---|---|---|---|---|---|
| A1 | Issues list → `n` | `n`/`N` key | Opens the New Issue form modal (`ModalState::NewIssue`), template defaults to Blank, milestone/project restored from per-repo sticky prefs | n/a | state unit test asserting `modal == ModalState::NewIssue` after `OpenNewIssueDialog` |
| A2 | Dialog → template combo | `Space` on Template field | Cycles Blank → Bug → Feature → Task → repo templates; switching prefills title/body and (when mapped) the Type field | n/a | state unit test on `NewIssueTemplateNext` cycling + prefill |
| A3 | Dialog → type combo | `Space` on Type field | Cycles the repo's available issue types (fetched async); selecting one sets the draft type | Options-load failure surfaces a non-blocking error in the dialog footer | state unit test on type cycle; async load test |
| A4 | Dialog → title editor | chars / Home / End / Left / Right / Backspace / Delete | Edits the title text with the existing single-line cursor model | n/a | state unit test on `NewIssueTitleChar/Backspace/...` |
| A5 | Dialog → body editor | multiline keys | Edits the body using the existing inline-composer multiline model (chars, newline, backspace, delete, arrows, Home/End per issue #406) | n/a | state unit test on `NewIssueBodyChar/...` |
| A6 | Dialog → labels picker | `L` opens picker; `Space` toggles; `Enter` confirms; `Esc` cancels | Multi-select of repo labels; confirmed selection stored in dialog draft `labels: Vec<String>` | Options-load failure blocks confirm with a footer error | state unit test mirroring `issues_property_ops_tests` toggle/confirm |
| A7 | Dialog → milestone picker | `M` opens picker; single-select; `Enter` confirms | Selected milestone stored in draft; also remembered as the per-repo sticky default on submit | n/a | state unit test |
| A8 | Dialog → project picker | `P` opens picker; multi-select; `Enter` confirms | Selected project(s) stored in draft; remembered as sticky default on submit | If repo has no Projects V2, picker shows "(no projects)" and is blankable | state unit test |
| A9 | Dialog → assignees picker | `A` opens picker; multi-select; `Enter` confirms | Selected assignees stored in draft | n/a | state unit test |
| A10 | Dialog → submit (`Alt+Enter`) | title non-empty | Spawns the create flow: `create_issue` (title+body), then on success applies labels/milestone/type/assignees/projects against the created node id; on success returns to Issues list with the new issue selected and a "Created issue #N" notice | Empty title → footer error, no spawn; gh failure → footer error, dialog stays open | state unit test on `NewIssueSubmit` with empty title; integration test on the create-then-edit pipeline |
| A11 | Dialog → cancel (`Esc`) | `Esc` | Returns to Issues list, draft discarded, sticky prefs unchanged | n/a | state unit test |
| A12 | Sticky defaults restored on open | open dialog for a repo with stored `last_new_issue_milestone`/`last_new_issue_project_ids` | Milestone/Project fields pre-filled from `RepoPreferences` | n/a | state unit test on `RepoPreferences` restore |
| A13 | Sticky defaults remembered on submit | successful submit | `RepoPreferences.last_new_issue_milestone`/`last_new_issue_project_ids` updated for the current repo | n/a | state unit test on `remember_new_issue_preferences` |
| A14 | Repo change clears dialog | repo switch while dialog open | Dialog closes, draft discarded (mirrors property-editor reset on repo change) | n/a | state unit test |
| A15 | Key routing | `n`/`N`, `Alt+Enter`, `Esc`, `Space`, `Tab`, `L`/`M`/`P`/`A`, title/body editor keys | Routes to the corresponding `NewIssue*` events | n/a | `app_input` key-routing tests |

## Non-goals

- **Slice C properties (priority, start date, target date, effort,
  relationships: parent / blocked-by / blocking / security alert).** These
  require Projects V2 item-field writes and sub-issue/dependency mutations
  that are not part of the issue REST API and would materially expand the
  GraphQL write surface. They are deferred to a follow-up issue and rendered
  as disabled placeholders in the dialog. **Needs user confirmation.**
- **Replacing the inline comment/reply composer.** Only the new-issue path
  moves to a dialog; comment and reply stay inline.
- **Full template front-matter parsing.** Built-in Bug/Feature/Task
  templates are synthesized client-side; repo templates are listed by name
  and their body is used verbatim. Full YAML front-matter label/type
  auto-mapping from repo templates is a follow-up.
- **No new dependencies, no workflow/CI/quality-tool changes, no
  `.llxprt/`/`.github/`/dependency-manifest changes.**
- **No changes to the existing property-editor overlay for already-created
  issues.** The dialog reuses the pure projection (`property_editor_view`)
  but does not mutate the existing `IssuePropertyEditorState`.

## Vertical slices

### Slice A — Dialog skeleton + create pipeline (labels/milestone/type/assignees)

- **Acceptance rows**: A1, A2, A4, A5, A6, A7, A9, A10, A11, A12, A13, A14, A15.
- **Architecture owner**: `state` (new `NewIssueDialogState` + reducer ops),
  `app_input` (new `NewIssue*` events + key router), `github` (extend
  `create_issue` to optionally carry labels/milestone/assignees in a single
  `gh api` call where possible, then post-create type via `updateIssue`),
  `ui/screens/new_issue.rs` (new form-modal screen, mirroring
  `NewAgentForm`/`NewRepositoryForm`), `messages` (event conversion).
- **Allowed files** (target ≤ 25):
  - `src/state/types.rs` — add `NewIssueDialogState`, `NewIssueTemplate`,
    `NewIssueDialogFocus`, `NewIssueDialogCursor`, draft fields; add
    `ModalState::NewIssue` variant.
  - `src/state/events.rs` — add `OpenNewIssueDialog`, `NewIssueTemplateNext`,
    `NewIssueTypeNext`, `NewIssueTitleChar/Backspace/Delete/Cursor*`,
    `NewIssueBodyChar/...`, `NewIssueOpenLabelsPicker`,
    `NewIssueOpenMilestonePicker`, `NewIssueOpenAssigneesPicker`,
    `NewIssuePickerToggle`, `NewIssuePickerConfirm`, `NewIssuePickerCancel`,
    `NewIssuePickerOptionsLoaded/Failed`, `NewIssueSubmit`,
    `NewIssueSubmitSucceeded`, `NewIssueSubmitFailed`, `NewIssueCancel`.
  - `src/state/new_issue_dialog_ops.rs` — reducer cases (pure).
  - `src/state/new_issue_dialog_ops_tests.rs` — behavior tests.
  - `src/state/mod.rs` — module wire-up.
  - `src/state/modal_ops.rs` — `OpenNewIssueDialog`/`CloseModal` handling
    for `ModalState::NewIssue`.
  - `src/state/form_ops.rs` — `is_form_open`/focus/cursor helpers extended to
    the new modal (mirrors the `NewRepository`/`NewAgent` wiring).
  - `src/state/preferences_ops.rs` — `remember_new_issue_preferences` +
    restore-on-open helpers.
  - `src/domain/mod.rs` — extend `RepoPreferences` with
    `last_new_issue_milestone`, `last_new_issue_project_ids`.
  - `src/app_input/new_issue.rs` — key router for the dialog.
  - `src/app_input/mod.rs` — wire the new router.
  - `src/app_input/new_issue_submit.rs` — create-then-apply-properties
    orchestration (mirrors `issues_mutation.rs::create_issue`).
  - `src/app_input/issues.rs` — change `n`/`N` from
    `OpenNewIssueComposer` to `OpenNewIssueDialog` when on the Issues list.
  - `src/github/create_issue.rs` — extend `create_issue` to accept optional
    labels/milestone/assignees in the POST (or split into a follow-up edit).
  - `src/github/mod.rs` — re-exports.
  - `src/ui/screens/new_issue.rs` — new iocraft form-modal screen.
  - `src/ui/screens/mod.rs` — wire the screen.
  - `src/ui/orchestration.rs` — add `ModalState::NewIssue` arm to
    `build_modal_element` (mirrors the `NewAgentForm` arm; **does not** touch
    `build_screen_element` or `ScreenMode`).
  - `src/messages/*` — `NewIssue*` event conversions (issues-side).
  - `src/state/events.rs` / `src/messages/names.rs` / `message_names.rs` —
    name registrations.
- **RED**: `state/new_issue_dialog_ops_tests.rs` asserts `OpenNewIssueDialog`
  sets `screen_mode = NewIssue` and restores sticky milestone; fails before
  the state type exists.
- **GREEN**: implement the reducer + key router + screen + create pipeline.
- **Non-goals for Slice A**: projects picker, Slice C properties.
- **Verification**: `make quick-check` during iteration; `make ci-check`
  before push.
- **Stopping conditions**: any property that cannot be applied via the
  existing `edit_properties` helpers without a new GraphQL write subsystem
  → stop and propose a follow-up.

### Slice B — Projects picker (optional, may ship with Slice A or as a follow-up PR)

- **Acceptance rows**: A3, A8.
- **Architecture owner**: `github` (new `fetch_repo_projects` + `add_project_item`
  GraphQL helpers), `state/new_issue_dialog_ops.rs`, `ui/screens/new_issue.rs`.
- **Allowed files**: `src/github/projects.rs` (new), `src/github/mod.rs`,
  `src/state/new_issue_dialog_ops.rs`, `src/app_input/new_issue_submit.rs`.
- **RED**: test that the projects picker loads options and stores a selection.
- **GREEN**: implement the GraphQL fetch + post-create `addProjectV2` mutation.
- **Stopping conditions**: if the `addProjectV2` mutation requires a
  projectId not easily resolved, stop and confirm scope.

### Slice C — Deferred properties (follow-up issue)

- priority, start date, target date, effort, relationships (parent /
  blocked-by / blocking / security alert).
- Requires Projects V2 item-field writes + sub-issue/dependency mutations.
- Rendered as disabled placeholders in the dialog with a "(follow-up)" hint.
- **Needs user confirmation before a follow-up is filed.**

## Comment tracking (issue #407)

The issue author may add comments to issue #407 at any time during delivery.
Comments are part of the issue's decision-complete contract per the
bounded-delivery workflow and **must not be lost**.

- **Before starting each implementation slice**: re-fetch the issue and its
  comments with `gh issue view 407 --comments` (and the raw API
  `gh api repos/vybestack/llxprt-jefe/issues/407/comments --jq '.[].body'`).
- **Before opening the PR**: re-fetch comments again and reconcile any new
  requirements into the acceptance matrix / scope ledger. If a new comment
  materially changes the architecture or scope, stop for user approval.
- New comments are recorded in the Scope ledger below with the date and a
  one-line summary; any change to the acceptance matrix is recorded as an
  In-scope-Fix or a new non-goal.

As of 2026-07-25 the issue had **0 comments**; the only content is the issue
body, which itself requests the design/mockup-and-approve flow this plan
follows.

## Scope ledger

| Date | Item | Type |
|------|------|------|
| 2026-07-25 | Initial plan + mockup | — |
| 2026-07-25 | Added "Comment tracking" workflow step (user reminder) | In-scope-Fix |

## Review counters

- Local OCR: 0/2
- PR OCR: 0/2

## Verification

- `make quick-check` during iteration (cargo fmt + cargo check -q + cargo test -q).
- `make ci-check` before push (fmt check, clippy gates, coverage ≥ 30,
  build, test).
- TUI harness scenario: open the new-issue dialog, cycle templates, set
  labels, submit, and assert the created issue appears in the list (scenario
  script under `scripts/` + `tests/`).

## Stopping rules for this plan

Per the ISSUE-DELIVERY workflow, this plan **stops for user approval** before
implementation on the following open decisions:

1. **Dialog vs inline combo.** Confirm the full-screen form-modal approach
   (`ModalState::NewIssue`), reusing the existing form-modal pattern rather
   than introducing a new `ScreenMode`.
2. **Built-in Bug/Feature/Task templates synthesized client-side** vs.
   repo-templates-only.
3. **Slice split**: ship Slice A (labels/milestone/type/assignees) first,
   defer Slice C (priority/dates/effort/relationships) to a follow-up, and
   optionally include Slice B (projects) in the first PR.
4. **"plan" == "project"**: confirm that the sticky "plan" default is the
   Projects V2 selection, or whether a separate plan concept is intended.

No implementation will start until the user approves (or revises) these
decisions and the mockup.