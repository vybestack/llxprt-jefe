# Issue 317: repository defaults for transient-agent launch options

## Issue and decision

Issue: https://github.com/vybestack/llxprt-jefe/issues/317

The repository editor already exposes the transient Code Puppy YOLO setting, but it starts disabled. LLxprt transient agents are built with empty `mode_flags`, even though an individual LLxprt agent starts with `--yolo` in its editable Mode field. The issue asks for repository-configurable transient defaults matching the relevant individual-agent launch options and explicitly requires YOLO to default to enabled.

This plan implements the complete runtime-specific YOLO contract:

- LLxprt repositories gain an editable **Default Mode** value, represented as structured mode flags and defaulted to `--yolo`.
- Code Puppy repositories keep the existing **Default YOLO** checkbox, but new repositories default it to checked.
- Clearing Default Mode or unchecking Default YOLO is the explicit opt-out.
- Transient agent domain objects and launch signatures consume the persisted repository choice.

This is stronger than changing only the existing Code Puppy checkbox: that narrower change would leave the normal LLxprt transient path unable to configure or receive `--yolo`, contradicting the issue's comparison with individual-agent options.

## Acceptance matrix

| ID | Actor / launch path | Input and boundary | Observable success | Failure / diagnostic | Side effects before failure | Persistence / compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | User creating a local or remote LLxprt repository | Open New Repository with LLxprt selected | `Default Mode` is visible, editable, and initially `--yolo` | N/A; blank is a valid opt-out | None | New repository persists normalized whitespace-separated flags | TUI scenario; reducer/form test |
| A2 | User editing an LLxprt repository | Existing nonempty, empty, or legacy-missing mode flags | Edit form displays the persisted/effective mode; clearing it persists no flags | N/A; arbitrary whitespace normalizes to no flags | No runtime launch during edit | Legacy JSON missing the new field defaults to `--yolo`; an explicitly persisted empty vector remains empty | domain serde test; form create/update test |
| A3 | User creating a Code Puppy repository | Open New Repository with Code Puppy selected | `Default YOLO` is visible and checked by default; Space can disable it | N/A | None | Checked persists `Some(true)`; unchecked persists no YOLO override | reducer/form test; TUI scenario where runtime selection permits it |
| A4 | User editing a Code Puppy repository | Persisted `Some(true)` or `None` | Edit form reflects the saved checkbox and preserves opt-out | N/A | None | Existing explicit `None`/`null` stays disabled; genuinely missing legacy field uses the new default | form test; serde compatibility test |
| A5 | Issue/PR transient LLxprt launch, immediate or queued | Repository mode contains `--yolo` plus optional additional flags | Both transient `Agent` and `LaunchSignature` carry the repository flags, so fresh-prompt launch retains them | Existing launch diagnostics remain authoritative | Existing clone/preparation rules unchanged | Transient agents remain runtime-only | domain test; launch-signature test |
| A6 | Issue/PR transient LLxprt launch after opt-out | Repository mode is explicitly empty | No LLxprt mode flags, including no `--yolo`, are passed | Existing launch diagnostics remain authoritative | Existing clone/preparation rules unchanged | Explicit empty vector round-trips | domain/form/launch-signature tests |
| A7 | Issue/PR transient Code Puppy launch, immediate or queued | Repository YOLO is enabled or disabled | Enabled signature carries `Some(true)` and runtime capability logic emits YOLO; disabled signature carries `None` | Existing capability/launch diagnostics remain authoritative | Existing clone/preparation rules unchanged | Existing optional field remains schema-compatible | existing runtime tests plus launch-signature test |
| A8 | Keyboard and copy/selection form behavior | Tab/BackTab, character insertion, cursor movement, backspace/delete, runtime switching | Focus skips runtime-inapplicable fields and the Default Mode editor behaves like other text fields | No hidden field receives edits | None | Form-only state | focused reducer/projection tests |

## Explicit non-goals

- Adding repository defaults for LLxprt debug, pass-continue, sandbox engine/flags, or Code Puppy quick-resume. Those are not YOLO controls; pass-continue also conflicts with the one-shot transient invariant.
- Changing transient queueing, cloning, cleanup, prompt composition, capability probing, or process management.
- Persisting transient agents.
- Changing dependencies, workflows, quality gates, `.llxprt/`, or `.code_puppy/`.
- Migrating an explicitly saved Code Puppy opt-out (`null`/`None`) to enabled. The enabled default applies to new definitions and genuinely absent legacy fields, while explicit user state remains authoritative.
- Unrelated form-order or selection-content cleanup beyond what is required to expose and edit the new runtime-specific field correctly.

## Vertical slices

### Slice 1: persisted runtime-specific transient defaults

- Acceptance: A2, A4, A5, A6, A7.
- Owner: domain model and app-input launch adapter.
- Allowed paths: `src/domain/mod.rs`, `src/domain/tests.rs`, repository struct-literal test fixtures, `src/app_input/mod.rs`, a focused app-input test module.
- RED: domain tests for default/legacy/explicit-empty semantics and transient inheritance; launch-signature test for mode propagation.
- GREEN: add `default_llxprt_mode_flags`, safe serde defaults, domain constructor defaults, transient copying, and signature copying.
- Non-goals: runtime command changes and new orchestration.
- Verification: focused `cargo test` filters, then `make quick-check`.
- Stop: a schema-version bump, migration subsystem, or runtime command redesign becomes necessary.

### Slice 2: repository create/edit UI and keyboard behavior

- Acceptance: A1, A2, A3, A4, A8.
- Owners: deterministic state/form projection and thin UI rendering.
- Allowed paths: repository form types/cursor/delete/build/modal/projection/ops, `src/ui/screens/new_repository.rs`, focused state/UI tests, and one TUI scenario.
- RED: create TUI scenario first; add reducer/form tests for default values, persistence normalization, visibility/focus, editing, and opt-out.
- GREEN: render Default Mode for LLxprt, default it to `--yolo`, default Code Puppy YOLO checked for new repositories, and wire complete text-edit behavior.
- Non-goals: unrelated modal redesign or broad field reordering.
- Verification: focused state tests, TUI harness scenario, then `make quick-check`.
- Stop: a new public UI abstraction or changes outside the existing form architecture become necessary.

## Expected paths by layer

- Domain/persistence contract: `src/domain/mod.rs`, `src/domain/tests.rs`, existing repository fixture literals in `src/services/tests.rs`, `src/selection/content_tests.rs`, `src/persistence/tests.rs`, and `src/app_input/tracker_resolver.rs`.
- State/form: `src/state/form_types.rs`, `src/state/modal_ops.rs`, `src/state/form_build.rs`, `src/state/form_projection.rs`, `src/state/form_cursor.rs`, `src/state/form_delete_helpers.rs`, `src/state/form_ops.rs`, `src/state/form_ops_tests.rs`.
- UI: `src/ui/screens/new_repository.rs`.
- Launch adapter: `src/app_input/mod.rs` plus a focused test module registered there.
- TUI evidence: `dev-docs/tmux-scenarios/transient-agent-options.json`.
- Planning/evidence: this file.

## Test-first sequence

1. Add the TUI scenario asserting LLxprt `Default Mode [--yolo]`, editing it to blank, and reopening the repository to prove persistence; run it and record the expected pre-implementation failure.
2. Add domain tests for new defaults, legacy missing-field defaults, explicit-empty preservation, and transient inheritance; run and record RED.
3. Add launch-signature propagation tests; run and record RED.
4. Add state/form tests for new modal defaults, Code Puppy checked default, runtime-specific visibility/focus, create/update normalization, cursor editing, and opt-out; run and record RED.
5. Implement Slice 1 to GREEN, then Slice 2 to GREEN.
6. Refactor only within the accepted architecture and run focused tests plus `make quick-check`.
7. Run `make ci-check` on the candidate head.
8. Run rustreviewer and Open Code Review on the stable verified diff, triage every finding, and rerun exact-head verification after fixes.

## Scope ledger

| Discovered item | Disposition | Acceptance / reason |
| --- | --- | --- |
| Existing Code Puppy repository YOLO field defaults off | Planned | A3 |
| LLxprt transient `mode_flags` are hardcoded empty in both domain and launch signature | Planned | A1, A5, A6 |
| Repository focus visibility currently does not hide the Code Puppy YOLO focus for LLxprt | Planned | A8; required so hidden fields do not receive focus |
| Individual Code Puppy agent form itself defaults YOLO off | Reject for this issue | Issue owns repository-configured transient defaults, not persistent-agent default policy |
| Repository form selection/copy projection omits older transient controls | Defer | Pre-existing omission not required to render or operate the modal; follow-up only if review proves it blocks A1-A8 |
| Transient debug/sandbox/continue/quick-resume defaults | Reject for this issue | Explicit non-goal; not required by the YOLO-focused issue and some conflict with one-shot semantics |

## Review counters

- Local Open Code Review runs: 1 / 2
- Post-PR Open Code Review runs: 0 / 2
- Rust reviewer runs: 1

## Verification evidence

- Focused domain, form, launch-signature, queue-snapshot, persistence, and integration tests pass.
- The 30-step `transient-agent-options` TUI scenario passes.
- Formatting, Clippy allow policy, source-size policy, both Clippy gates, coverage (71.82% lines), locked build, and full locked tests pass.
- Public `git_info` behavior tests were split into an 88-test integration target while four private subprocess-timeout tests remain unit tests; all 92 pass and strict Clippy no longer reports a generated large stack array.

## Review findings and dispositions

| Reviewer | Finding | Disposition |
| --- | --- | --- |
| Rust reviewer | Queued agents could read edited repository defaults while launching the enqueue-time signature | In-scope fix: added immutable-signature construction and queued snapshot regression coverage for both runtimes |
| Rust reviewer | Positive Code Puppy default-enabled launch signature lacked focused coverage | In-scope fix: added a direct `Some(true)` launch-signature test |
| Open Code Review | Repository selection projection duplicated checkbox formatting | In-scope fix: reused the existing `render_checkbox` helper |
| Open Code Review | Suggested three-state Code Puppy YOLO form because `None` was interpreted as inheritance | Reject: in the established domain contract `None` is the explicit disabled/no-override state, missing serde fields use `default_code_puppy_yolo`, and tests prove explicit persisted `null` remains disabled after edit/save |

## Deferred findings / follow-ups

None filed. The scope ledger records the pre-existing selection/copy projection omission for possible later follow-up if it remains relevant.
