# Issue 727 plan: restore the Edit Agent and Edit Repository presentation on the shared runtime

Issue: https://github.com/vybestack/llxprt-jefe/issues/727
Branch: `issue727`, created from `origin/main` at `4fbca0d7`

## Scope summary

The #706/#720 cutover replaced the bespoke Edit Agent and Edit Repository
renderers with the two retained shared-runtime projections
(`src/overlay_controls_agent_form.rs`,
`src/overlay_controls_repository_form.rs`). Deleting the bespoke renderers was
required #706 policy. Losing the operator-facing row presentation was not: the
generic `host_controls::project_form` body renderer emits `{label}: {value}`
rows, renders booleans as `true`/`false`, and shows a machine-facing
`submit: host.overlay-submit` row, while the old forms drew
`  {label:<16} [{value}]` rows with `[x]`/`[ ]` checkboxes, `(space toggles)`
hints, per-field contextual hints, spacer rows, and a dim one-row footer
(`tmp/forms-presentation-audit.md`, introduced at `394f59aa` on main).

This issue restores that pre-cutover presentation inside the retained
projections on the shared definition/control runtime. It must not resurrect the
deleted form screens (`src/ui/screens/new_agent.rs`,
`src/ui/screens/new_repository.rs`, `src/selection/form_content.rs`) or any
legacy routing path; `tests/issue706_cutover_contracts.rs` and
`dev-docs/testing/issue706-owner-evidence.json` stay green.

Functional audit result (`tmp/forms-functional-audit.md`): no functional
regression was proven for either form. The edit reducers, drafts, cursor
helpers, visibility rules, builders, ID-bound update paths, validation,
persistence, and submit/cancel branches are retained. This issue is therefore a
presentation restoration plus characterization coverage of that retained
behavior, not a functional repair.

The functional audit's four coverage gaps close here as characterization
tests: (1) repository projection/reducer order agreement around Default YOLO
and Default Agent, (2) one production key-route test per edit form,
(3) one TUI scenario per edit form covering invalid edit, correction, submit,
reopen/persistence, then a second canceled draft, (4) no mouse/typed
field-row adapter work (out of scope unless already intended; keyboard editing
is preserved and covered).

Resolved UI decisions (per the issue's acceptance matrix; restoration is the
default, one deliberate improvement allowed):

1. **Restore pre-cutover appearance** for both forms: two-space row indent,
   16-cell aligned label column, bracketed values (`[]` for empty), `[x]`/`[ ]`
   checkboxes with `  (space toggles)`, selector rows with their dynamic
   `effective_types_hint`, Sandbox Engine `(disabled)`/`space cycles:` state
   hint, the repository contextual hints (Issues/PRs fallback, transient-dir
   fallback, max-transient meaning), runtime-neutral `Version` /
   `Default Version` display labels, `SSH Options (space-separated)` label,
   blank spacer rows after the title and before the footer, and the indented
   plain-weight title line.
2. **Keep the shared definition/control runtime**: the fix is row text and row
   style metadata produced by the two retained projections
   (`bespoke_form_projection` pattern already used by
   `overlay_controls_generated_form.rs`), never a second renderer, a second
   draft owner, or a parallel routing path.
3. **One deliberate improvement: the footer fits one line at 120 columns.**
   The old agent footer was 117 display cells plus the two-space indent, which
   clipped after `Esc` at 120 columns. The restored footers keep the old
   double-space rhythm and wording but must fit the 116-cell content width at
   120 columns, so no footer wraps and no `cancel` orphan row appears. Footer
   fitting uses `fit_text_to_width` (the `push_list_item_row` pattern from
   #723) rather than `push_wrapped`.
4. **Focus styling**: restore the bright focused row (theme `rc.bright`) in
   addition to the retained caret, and restore dim styling for the remote
   SSH/identity fields while Remote Repository is unchecked. This requires a
   small additive extension to the shared shell (`HostControlRow` presentation
   hint + `HostControlOverlay` per-row color), which the issue's expected-path
   list already names (`src/host_controls.rs` only if shared row style
   metadata is required).
5. **Submit row**: no user-visible `submit:`/`host.overlay-submit` text. The
   `PanelHitTarget::Submit` hit target and the shared submit action contract
   (`action_affordances`) are retained; the visible affordance text is removed
   from the row stream (Enter submits, footer documents it), and the existing
   `agent_form_carries_the_shared_submit_affordance` style assertions move to
   the affordance contract.
6. **Validation error position**: restore the legacy position and emphasis,
   `Error: {message}` below the fields (after the spacer) and above the
   footer, rendered bright. The current top-prepend position
   (`prepend_detail_rows`) changes back.
7. **Long field values**: one row per field. The value is fitted/truncated
   inside the bracketed row (`fit_text_to_width`/ellipsis) so wrapping never
   changes the field-to-row mapping or focus indexing. The audit's wrap case
   (matrix #19) is settled as truncate-to-one-row.
8. **Repository order**: settle one order for Default YOLO and Default Agent
   and assert projection/reducer agreement (functional-audit gap 1). The
   pre-cutover duplicate YOLO row is not reproduced.

## Acceptance matrix

Decision-complete; each row names its proving artifact. "Old" means the
`652319329`/`f5826508` era frames in `tmp/forms-regression-evidence/`.

| ID | Surface / actor | Input and boundary | Required observable behavior | Failure behavior / diagnostic | Evidence |
|---|---|---|---|---|---|
| A1 | Edit Agent rows (operator opening the form from a selected agent) | 120x40 frame, fields populated from the persisted agent | Rows render `  {label:<16} [{value}]`: two-space indent, 16-cell label column, bracketed values; `[]` for empty text fields; blank spacer row after title and before footer; title line indented two spaces, plain weight | Any row renders as `Label: value`, ragged alignment, bare trailing colon, or missing spacer | Unit row-projection tests (`src/overlay_controls_agent_form_tests.rs`); `dev-docs/tmux-scenarios/issue727/edit-agent-presentation.json` frame assertions |
| A2 | Edit Agent booleans | `Pass --continue`, `Sandbox` toggled and at rest | `[x]`/`[ ]` plus `  (space toggles)`; never bare `true`/`false` | `Pass --continue: true` style row | Unit tests; scenario `Pass --continue  [x]  (space toggles)` and toggle-wait literals |
| A3 | Edit Agent selector + engine hints | Runtime selector with no available agents; sandbox off then on | `Agent Runtime    [core.llxprt]  (no available agents)` (or the real `space cycles:` list); `Sandbox Engine   [Podman]  (disabled)` when off, `space cycles:` list when on | Hint absent or value unbracketed | Unit tests (hint on/off variants); scenario Space-toggle sequence flipping the engine hint |
| A4 | Edit Agent labels | LLxprt agent | Display labels `Version` (not `LLxprt Version`) | Runtime-prefixed label appears | Unit label tests; scenario `Version          []` row |
| A5 | Edit Agent submit row | Any form state | No visible `submit: host.overlay-submit` (or any `submit:` text); Enter still submits; `PanelHitTarget::Submit` hit target and `action_affordances` contract intact | Machine action id visible, or submit affordance lost | Unit tests asserting no row text contains `submit:`/`overlay-submit` plus affordance assertions; scenario `absent: submit: host.overlay-submit` |
| A6 | Edit Agent footer | 120 columns | Exactly one footer row, double-space rhythm, ends with `Esc cancel` fully visible; fits 116-cell content width; no orphan `cancel` row | Footer wraps or `cancel` orphan row | Footer-length unit test (<= content width); scenario one-row footer literal + `absent` orphan `cancel` row check |
| A7 | Edit Agent focus | Focused field at rest and while typing | Focused row bright (`rc.bright`) in addition to the retained caret `▏` | Caret-only focus | Overlay rendering tests; scenario caret row (`Name             [beta▏]`) proves caret survives the format change |
| A8 | Edit Repository rows | 120x40 frame, fields populated from the persisted repository | Same aligned bracketed format; `Default Version` label (not `Default LLxprt Version`); `SSH Options (space-separated)` label; spacers after title and before footer; indented plain title | Inline rows, prefixed labels, shortened SSH label, missing spacers | Unit tests (`src/overlay_controls_repository_form_tests.rs`); `dev-docs/tmux-scenarios/issue727/edit-repository-presentation.json` |
| A9 | Edit Repository contextual hints | Issues/PRs blank vs set; transient dir blank vs set; max transient 0 vs set | `Issues / PRs Repo []  (blank uses GitHub Repo)` / `override issue/PR tracker`; `Transient Dir    []  (blank uses /tmp)`; `Max Transient    [0]  (0 = no limit)` | Hints absent | Unit tests per hint; scenario literals |
| A10 | Edit Repository booleans | `Remote Repository`, `Setup Env Default` | `[ ]`/`[x]` plus `  (space toggles)` | `Remote Repository: false` | Unit tests; scenario toggle waits |
| A11 | Edit Repository disabled remote fields | `remote_enabled = false` | Login User, Host/IP, SSH Port, Identity File, SSH Options, Run As User rows render dim; traversal still reaches them | Full-brightness disabled rows, or traversal lost | Overlay rendering tests (dim row style); retained traversal covered by route test R2 |
| A12 | Edit Repository footer | 120 columns | One row, fits 116 cells, `Space toggles remote options` wording, ends `Esc cancel` visible | Wrap or clipping | Unit footer test; scenario footer literal |
| A13 | Validation error presentation (both forms) | Submit with an invalid field (agent: whitespace version; repository: malformed GitHub repo) | `Error: {message}` bright, positioned below fields + spacer, above footer; form stays open; entity unchanged; correction and resubmit succeed | Error prepended above fields, uniform color, or form closes / entity mutates | Unit projection tests (error position); scenarios `issue727-edit-agent-lifecycle.json`, `issue727-edit-repository-lifecycle.json` |
| A14 | Long field values (both forms) | Value longer than the bracketed row width | One row; value fitted/truncated with ellipsis inside the brackets; stable field-to-row mapping | Wrapped extra rows changing geometry/focus indexing | Unit tests with an over-width value; retained `truncate_the_focused_row...` tests updated |
| A15 | Copy pane parity | Form open, selection/copy pane active | `agent_form_lines`/`repository_form_lines` produce the same rows the frame shows (single shared projection source preserved) | Copy pane diverges from frame | Unit tests asserting the shared projection is the single row source |
| A16 | Visibility gating (both forms) | Code Puppy agent; LLxprt agent; repository type switching | Runtime/type-gated fields follow the selected type exactly as today (retained behavior, no change) | Any visibility change | Existing retained tests (`agent_form_hides_llxprt_fields_for_code_puppy`, `repository_form_hides_type_gated_fields_like_the_legacy_renderer`) stay green |
| R1 | Edit Agent production key route | Open by AgentId, Tab/BackTab/Up/Down traversal, typing, Space cycling, Enter submit, reopen by same ID, Esc cancel | Every step resolves through registry/raw-key production paths and mutates only the intended entity by ID | Route regression or index-based update | New route characterization test (functional-audit gap 2) |
| R2 | Edit Repository production key route | Same, by RepositoryId | Same | Same | New route characterization test |
| R3 | Repository projection/reducer order | Each shipped runtime type | Projected visible row order and traversal order agree on one settled order around Default YOLO / Default Agent; no duplicate YOLO row | Order disagreement or duplicate row | New agreement test enumerating visible focus slots and projected rows (functional-audit gap 1) |
| F1 | Edit Agent full lifecycle scenario | Existing agent; mutate name; invalid submit; correct; submit; reopen; cancel second draft | Persisted value visible on reopen; canceled draft discarded (name unchanged); no launch/terminal-focus side effect | Wrong persisted value or canceled draft applied | `dev-docs/tmux-scenarios/issue727/edit-agent-lifecycle.json` |
| F2 | Edit Repository full lifecycle scenario | Existing repository; same sequence with a malformed GitHub repo as the invalid step | Same persistence/discard guarantees | Same | `dev-docs/tmux-scenarios/issue727/edit-repository-lifecycle.json` |
| C1 | Cutover contracts | Full test suite | `tests/issue706_cutover_contracts.rs` green; `issue706-owner-evidence.json` retained-module hashes re-pinned only by reproducing old hashes first; deleted paths stay deleted | Contract test fails or deleted path resurrected | Contract test run; owner-evidence pin graph section |
| C2 | Scenario sentinel re-sync | ~19 scenario files rewritten to inline form format by `394f59aa` | Sentinels re-synced to the restored bracketed format; manifest expectations (steps/assert counts) updated; all scenarios pass | Unrelated scenarios left red | Manifest + owner-evidence ledgers (GREEN-phase additions below) |

Persistence and compatibility: no state-schema, settings, definition, action,
or persistence change. The submit action id (`host.overlay-submit`) remains
the wire contract; only its operator-visible text disappears.

## Non-goals

- No reintroduction of deleted bespoke form screens, `form_content.rs` /
  `generated_form_content.rs`, old `SelectablePane` geometry for these forms,
  or any parallel form routing (guarded by `tests/issue706_cutover_contracts.rs`).
- No change to form state ownership, draft model, reducers, cursor helpers,
  builders, validation rules, or persistence semantics (functional audit found
  no regression there; changes require a failing parity test first).
- No redesign beyond the pre-cutover presentation; the only deliberate
  deviation is the one-row fitted footer (and the truncate-not-wrap decision
  for long values, which restores stable one-row geometry).
- No mouse or provider-style typed-field editing on form rows
  (functional-audit gap 4): adding a `FieldChanged` state adapter is separate
  work; keyboard editing is what is preserved and covered.
- No work on the #726 dashboard list-windowing fixture ambiguity or the #719
  scenario-startup instability.
- No dependency, `.github/`, `.llxprt/`, quality-gate, or lint-threshold
  changes.
- No coverage of `GeneratedAgent` / generated-form presentation beyond
  keeping it compiling and green; its projection already builds its own rows.

## Retained behavior constraints (must not regress)

From the functional audit's retain list, all preserved by this plan and
asserted by the route tests and scenarios:

1. Edit opens and submits by stable `AgentId`/`RepositoryId`, never list index.
2. Every persisted field populates on open, with text cursors at Unicode
   character counts (caret stays visible in the focused bracketed row).
3. Traversal skips type/runtime-gated fields; repository traversal keeps
   reaching remote fields while remote mode is off (they are dim, not removed).
4. Typing, paste, cursor movement, delete, backspace reach the existing
   reducers; the projections stay render-only, never a second draft owner.
5. Failed validation keeps the form open, the entity unchanged, and permits
   correction and resubmission.
6. Edit Agent submit never enters the new-agent launch branch or forces
   terminal focus.
7. Esc discards the draft and preserves screen, selection, and pane focus.
8. Definition-backed merge behavior (same-runtime edits preserve values
   outside the replaced declaration set; runtime changes seed from defaults).
9. Sandbox round trip accepts labels and CLI spellings and persists the engine.
10. Shared projection remains the single source for both the frame and the
    copy pane (`agent_form_lines`/`repository_form_lines`).
11. `PanelHitTarget::Field(id)` targets ride every row so mouse hit-testing
    and focus routing keep working.

## Bounded vertical slices

Each slice is independently testable, lands one coherent green behavior, and
stays inside the issue's expected-path list.

### S1: Edit Agent row restoration (A1, A2, A3, A4, A7-caret, A5, A13-agent, A14-agent)

- RED: exact row-projection tests in `src/overlay_controls_agent_form_tests.rs`
  (bracketed aligned rows, checkbox + hint, selector hint, engine hint off/on,
  `Version` label, no `submit:` row text, error below fields, one-row
  long-value) + `dev-docs/tmux-scenarios/issue727/edit-agent-presentation.json`
  (already RED, proven this run).
- GREEN: build rows directly in `src/overlay_controls_agent_form.rs` via the
  `bespoke_form_projection` pattern (no `host_controls::project_form` row
  text), neutral display labels for the runtime-version fields
  (projection-local label override, smaller blast radius than
  `InternalField::label()`), spacer rows, `fit_text_to_width` for values,
  footer constant shortened to fit 116 cells, submit text removed from the row
  stream while the affordance stays.
- Allowed paths: `src/overlay_controls_agent_form.rs`,
  `src/overlay_controls_agent_form_tests.rs`,
  `src/overlay_controls.rs` (only `REPOSITORY_FORM_FOOTER`/shared helpers if
  needed), scenario file + affected sentinel files for the agent form.

### S2: Edit Repository row restoration (A8, A9, A10, A12, A13-repository, A14-repository)

- RED: same test shape in `src/overlay_controls_repository_form_tests.rs` +
  `edit-repository-presentation.json` (already RED, proven this run).
- GREEN: same construction in `src/overlay_controls_repository_form.rs` with
  the repository hints (`blank uses GitHub Repo` / `override issue/PR tracker`,
  `blank uses /tmp`, `0 = no limit`), `SSH Options (space-separated)` label,
  `Default Version` label, `Space toggles remote options` footer fitted to one
  row.
- Allowed paths: `src/overlay_controls_repository_form.rs`,
  `src/overlay_controls_repository_form_tests.rs`, `src/overlay_controls.rs`
  (footer constant), scenario file + affected sentinels.

### S3: shared-shell row style metadata (A7 bright, A11 dim, A13 bright error)

- RED: overlay rendering tests for bright focused row, dim disabled remote
  rows, bright error line.
- GREEN: additive `HostControlRow` presentation hint (e.g. `focused`/`dim`/
  `bright` flags or a `RowStyle` enum) set by the two projections, mapped to
  `colors.bright`/`colors.dim` in `src/ui/components/host_control_overlay.rs`;
  `src/ui/orchestration.rs` passes row style through. No second shell, no
  screen-specific overlay component.
- Allowed paths: `src/host_controls.rs`, `src/ui/components/host_control_overlay.rs`,
  `src/ui/orchestration.rs` (style plumbing only), the two form modules, their
  tests.
- Stop condition: if the style extension turns out to require more than
  additive row metadata (e.g. per-row color plumbing across unrelated
  overlays), stop and present the alternatives rather than expanding the
  shared shell contract.

### S4: functional characterization (R1, R2, R3)

- RED: three new characterization tests; per the functional audit these are
  coverage gaps, not proven regressions, so where current behavior already
  satisfies the assertion the test lands green immediately (that is expected
  and is not a TDD violation for retained behavior; the presentation rows
  above are the true RED/GREEN pairs).
- GREEN: the route tests drive registry resolution + raw-key fallback +
  boundary submit/cancel for both forms, and the order-agreement test settles
  the repository order (with the projection adjusted if order disagreement is
  real, which is in-scope as functional-audit gap 1).
- Allowed paths: new `tests/` or in-crate test modules following the
  per-target budget rule; `src/overlay_controls_repository_form.rs` only if
  the order must move (acceptance row R3).

### S5: scenario corpus re-sync + ledger registration (C1, C2, F1, F2 GREEN)

- GREEN-only: promote the four scenarios from `tmp/issue727/red/` into
  `dev-docs/tmux-scenarios/issue727/` (they are authored there first, tracked
  but unregistered during RED), register them in
  `dev-docs/testing/scenario-execution-manifest.json` with exact step and
  assertion counts, add owner-evidence entries, and re-sync the affected
  sentinel files plus `scenario-owner-evidence.json` expectations to the
  restored format. S5a performs the scenario, manifest, and plan updates;
  owner-evidence updates remain for the separate old-hash reproduction task.
- Allowed paths: `dev-docs/tmux-scenarios/**`,
  `dev-docs/testing/scenario-execution-manifest.json`,
  `dev-docs/testing/scenario-owner-evidence.json`, #704/#705/#706
  owner-evidence ledgers as required by the pin graph, `project-plans/issue727-plan.md`.

Stopping conditions for all slices: any need for a new subsystem, unplanned
public abstraction, dependency change, behavior absent from the matrix, or a
change to `.llxprt/`, `.github/`, or quality-gate configuration.

## Expected path ledger

- `src/overlay_controls_agent_form.rs`: row construction, labels, hints,
  footer, submit-row text removal.
- `src/overlay_controls_agent_form_tests.rs`: exact row-projection tests.
- `src/overlay_controls_repository_form.rs`: same.
- `src/overlay_controls_repository_form_tests.rs`: same.
- `src/overlay_controls.rs`: `REPOSITORY_FORM_FOOTER` (and shared row/fit
  helpers if hoisted).
- `src/host_controls.rs`: `HostControlRow` presentation-hint fields only if
  S3 lands as planned.
- `src/ui/components/host_control_overlay.rs`: per-row color mapping.
- `src/ui/orchestration.rs`: row-style plumbing into the overlay shell.
- `src/domain/plugin/field.rs`: only if the label decision moves from
  projection-local overrides to `InternalField::label()` (default: local
  override; touching `field.rs` reprins unrelated sentinels and needs the
  pin-graph step).
- New characterization test modules for R1/R2/R3.
- `dev-docs/tmux-scenarios/issue727/{edit-agent-presentation,edit-repository-presentation,edit-agent-lifecycle,edit-repository-lifecycle}.json`
- The 13 existing scenario files with affected form observations:
  `code-puppy-chord-passthrough.json`, `code-puppy-version-fields.json`,
  `first-agent-tutorial.json`, `fork-issue-pr-repository.json`,
  `issue519/llxprt-launch-options.json`, `issue652/llxprt-sandbox-save.json`,
  `issue713/sandbox-launch-empty-ssh-agent.json`,
  `kennel-terminal-select.json`, `latest-version-fields.json`,
  `llxprt-version-fields.json`, `repo-github-field-focus.json`,
  `terminal-scrollback.json`, and `transient-agent-options.json`.
- `dev-docs/testing/scenario-execution-manifest.json`,
  `dev-docs/testing/scenario-owner-evidence.json`: registration + counts
  (GREEN phase).
- Owner-evidence ledgers as the pin graph requires (see below).
- `project-plans/issue727-plan.md`: this plan.

## Scope ledger

| Entry | Status |
|---|---|
| Presentation + lifecycle split into four scenarios instead of two combined ones. Combining the full mutation/invalid/correct/submit/reopen/cancel lifecycle with the exhaustive presentation assertion set made each run long and the failure localization poor; the directive explicitly allows this split when documented, with no weakened coverage. | Planned from the start; documented here |
| Truncate-not-wrap decision for long field values (A14). The old bespoke screens relied on flex clipping; the shared shell wraps. Restoration of stable one-row geometry requires an explicit decision; truncate-to-one-row is the pre-cutover observable behavior (one field row per field). | Resolved by decision 7 |
| Projection-local display-label overrides vs `InternalField::label()` change. Local override chosen (smaller blast radius); `field.rs` is listed in the issue's expected paths, so switching is in scope if tests force it, with the sentinel re-pin recorded. | Resolved by decision in S1/S2 |
| Repository Default YOLO / Default Agent order (R3). | Resolved in S2: projection order moved Default Agent before Default YOLO to match reducer traversal; the agreement test is green and no duplicate YOLO row remains. |
| Blank Edit Agent name closed the modal without a visible error after package probe. This violated A13/F1. | Discovered and fixed in S4 as an in-scope defect. Production paths: `src/state/form_submit_ops.rs`, `src/app_input/new_agent_submit.rs`; tests: `src/state/form_ops_tests.rs`, `src/app_input/new_agent_submit_tests.rs`. |
| S1-S4 focused verification | Green before S5. |
| S5 owner-evidence pin updates | Deferred to the separate pin task so old hashes can be reproduced before the chain is updated. |
| Anything else discovered during implementation | Requires a ledger entry and approval before implementation |

## Evidence ledger

RED evidence captured this run (before any production change), all under
`tmp/issue727/red/`:

- `edit-agent-presentation.json`, `edit-repository-presentation.json`,
  `edit-agent-lifecycle.json`, `edit-repository-lifecycle.json`: the four
  schema-1 scenario manifests (authored under `tmp/issue727/red/` during RED;
  promoted to `dev-docs/tmux-scenarios/issue727/` in S5, unchanged in
  substance).
- `old-era-{agent,repository}-{presentation,lifecycle}-report.json`: runs of
  the same four scenarios against the `652319329` pre-cutover binary
  (`../bisect-652319329/target/debug/jefe`), recorded where schema/config
  compatibility permits.
- `current-{agent,repository}-{presentation,lifecycle}-report.json`: runs
  against `target/debug/jefe` built from current main (`4fbca0d7`), proving
  RED on the presentation assertions with exact step, literal, and frame.
- `build-provenance.txt`: git and build provenance of the binary used for the
  current-main RED.

GREEN evidence through S5a:

- S1-S4 focused gates were green before the scenario re-sync. S4 found and
  fixed the blank Edit Agent name defect recorded in the scope ledger.
- S5a changed 96 form observations in the 13 existing scenario files listed in
  the expected path ledger. Dashboard/sidebar and generated-form observations
  were unchanged.
- The four macOS scenarios are registered with deterministic inventories:
  agent lifecycle 46 steps / 8 `assert-frame`; agent presentation 30 / 5;
  repository lifecycle 54 / 8; repository presentation 24 / 3. None captures.
- `target/debug/jefe`, `tmux_scenario`, `jefe-capture-shim`, harness probe, and
  JSP fixture were rebuilt serially. All four #727 scenarios passed on attempt
  1, as did `fork-issue-pr-repository.json`,
  `issue519/llxprt-launch-options.json`, `issue652/llxprt-sandbox-save.json`,
  `issue713/sandbox-launch-empty-ssh-agent.json`,
  `repo-github-field-focus.json`, and `terminal-scrollback.json`.
- Seven existing scenarios that depend on startup agent probes failed at the
  pre-form wait on attempts 1 and 2: `code-puppy-chord-passthrough.json`,
  `code-puppy-version-fields.json`, `first-agent-tutorial.json`,
  `kennel-terminal-select.json`, `latest-version-fields.json`,
  `llxprt-version-fields.json`, and `transient-agent-options.json`. Every
  failure was the known #719 missing Installed/Installed, enabled startup
  publication; no content-dependent failure was observed.
- Individual reports and driver logs are under `tmp/issue727/s5a/runs/`.
  `manifest_exactly_classifies_the_recursive_corpus` passed with the exact
  paths, platforms, operations, and assertion counts. Owner-evidence pins were
  not run or changed in S5a; the separate pin task remains.

## RED/GREEN expectations

RED (proven this run against current main `4fbca0d7`):

- Edit Agent presentation: fails at the first presentation assertion with the
  current inline rows (exact step, literal, and frame recorded in the report
  and summarized below).
- Edit Repository presentation: same shape of failure.
- Lifecycle scenarios: fail at their first restored-format wait literal
  (e.g. `Name             [alpha▏]`) while current main renders
  `Name: alpha▏`; the functional steps behind them are expected to pass at
  GREEN, which is exactly the characterization they provide.
- Unit row-projection tests (S1/S2 RED): to be written at implementation
  start against the same literals; they fail identically on current main.

GREEN criteria:

- All four scenarios pass end to end against the fixed binary.
- All unit/projection/rendering/route tests pass; the 13 re-synced existing
  scenarios pass; the manifest completion gate passes with exact counts.
- `cargo xtask ci` green on the candidate head; CI green on the exact head.

## Owner-evidence pin graph

Files whose content this issue changes and the ledgers that pin them:

- `src/overlay_controls_agent_form.rs`, `src/overlay_controls_repository_form.rs`,
  `src/overlay_controls.rs`, `src/host_controls.rs`,
  `src/ui/components/host_control_overlay.rs`, `src/ui/orchestration.rs`:
  pinned by `dev-docs/testing/issue706-owner-evidence.json` (retained-modules
  and repointed call-site records) and #704/#705 owner-evidence as those
  name the shared-runtime files. Re-pin by reproducing the old hashes first
  (the workflow requires old-hash reproduction before writing new ones).
- `dev-docs/tmux-scenarios/**` (the ~19 sentinel files): pinned by
  `dev-docs/testing/scenario-owner-evidence.json` (scenario hashes/counts) and
  the execution manifest (`expect` step/assertion counts). Every sentinel
  change re-pins its owner-evidence entry and updates the manifest's
  `expect.operations`/`assertions` numbers.
- `dev-docs/tmux-scenarios/issue727/*.json` (four new files): added to the
  execution manifest and owner-evidence in S5 only (GREEN phase), so no
  unrelated test goes red during RED. The manifest completion gate rejects
  missing evidence for any registered scenario, hence deferring registration
  until the binaries render the restored format.
- `tests/issue706_cutover_contracts.rs` and
  `dev-docs/testing/issue706-owner-evidence.json` deleted-path lists: must
  remain untouched in substance (no deleted path resurrected); if a contract
  test pins a row literal this issue changes, the pin is updated by
  reproducing the old value first, never by deleting the guard.

## Exact gates

Per slice: `cargo xtask quick` during iteration; focused
`cargo test --locked --lib overlay_controls` runs.

Before the PR (candidate head, serial cargo):

1. `cargo fmt --all --check`
2. `cargo xtask check clippy-allows`
3. `cargo xtask check source-size` (the two form modules and
   `host_controls.rs` approach limits; watch touched files)
4. `cargo xtask check architecture`
5. `CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features -- -D warnings`
6. Complexity clippy pass (the exact jefe gate command with `clippy::`-prefixed
   lint names)
7. `cargo build --workspace --all-features --locked`
8. `cargo test --workspace --all-features --locked`
9. `cargo xtask coverage` (>= 30% floor)
10. `scripts/run-scenario-manifest.py --platform macos` (all four new + all
    re-synced scenarios green; completion gate exact)
11. CI on the exact PR head (including windows_native and coverage)

## Stop conditions

Stop and ask for a decision when:

- the shared-shell style extension (S3) needs more than additive row metadata;
- any change would touch `.llxprt/`, `.github/`, dependency manifests, or
  quality-gate configuration;
- a sentinel re-sync uncovers behavior differences beyond presentation
  (functional drift found by the route tests);
- the repository order settlement (R3) contradicts both candidate orders;
- required verification cannot complete (build/test/scenario environment).

Stop successfully when the acceptance matrix has behavioral evidence, the
non-goals are untouched, exact-head gates and CI pass, the scenario corpus and
ledgers are consistent, OCR counters are within cap, and the scope ledger is
clean.

## Review counters

- Local OCR runs before PR: 0 / 2 used.
- OCR runs after PR opened: 0 / 2 used.

## RED evidence captured this run (current main `4fbca0d7`)

Binary provenance: `target/debug/jefe` built Sep 2 21:53 from `4fbca0d7`
(main, "Restore the dashboard chrome the provider runtime dropped (#723)
(#725)"); see `tmp/issue727/red/build-provenance.txt`.

Both presentation scenarios fail on current main at their first presentation
assertion, with the exact failing step/literal/frame recorded in
`tmp/issue727/red/current-*-report.json`. The one-row-footer and
`absent submit: host.overlay-submit` assertions are the ones that fail first
in each run; the aligned-row literals (`Shortcut (1-9)   [none]`,
`Name             [alpha▏]`) fail immediately after in the same assertion
step. Lifecycle scenarios fail at their first restored-format wait literal
(`Name             [▏]` / `Name             [alpha▏]`) while current main
renders `Name: ▏` / `Name: alpha▏`.

Old-era compatibility run (see `old-era-*.json` reports): the
`652319329` pre-cutover binary renders the restored format these scenarios
assert, subject to the compatibility caveats recorded with each report.

## Deferred findings

- (none yet)