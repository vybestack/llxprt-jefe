# Issue #388 — CW-08: Agent Types, Screens/Layout, and Keys editors

Branch: `issue388` (from `origin/main` @ `1b576d7b`).

## 1. Outcome

The Settings screen delivered by CW-07 (issue #387) gains three more sections —
Agent Types, Screens/Layout, and Keys. Each section is a **pure presenter** over
an immutable snapshot plus the live draft candidate, and each emits **closed
typed intents** that the Settings reducer turns into **sparse** edits of the
lossless settings document. The existing owners stay the only validators and the
only serializer; the editors never start a provider and never mutate an active
registry.

## 2. Consumed owner contracts (prerequisite gate — all present)

| Contract | Owning module | Status |
|---|---|---|
| Agent type registry / definitions / availability | `src/agent_registry.rs`, `src/domain/agent_definition/{definition,types}.rs` (`AgentTypeId`, `AgentDefinition`, `Availability`, `ProbeErrorCode`) | present |
| Agent probe observation snapshot | `src/agent_status_view.rs::AgentAvailabilityObservation`, `AppState::agent_type_availability` | present |
| Agent enablement consumption at startup | `src/app_init.rs::agent_type_enabled` (reads `settings.agents.<id>.enabled`) | present |
| Screen descriptor registry + layout value types | `src/workbench/{screens,descriptor,ids}.rs` (`ScreenRegistry`, `ScreenDescriptor`, `LayoutNode`, `LayoutChild`, `Axis`, `Size`, `ScreenIdentity`) | present |
| Descriptor/layout validator | `src/workbench/validate.rs::validate_descriptor` (`DescriptorError`) | present |
| Layout preview resolver | `src/workbench/resolve.rs::resolve_layout` (`ResolvedLayout`, `TooSmall`) | present |
| `enabled_screens` consumption at startup | `src/startup_screens.rs::compose` → `src/workbench/compose.rs::compose_screens` | present |
| Action/key resolver | `src/domain/action_registry.rs` (`ActionRegistrySnapshot`, `Action`, `Binding`, `ActionId`, `Availability`, `Provenance`), `src/domain/input_context.rs::ContextId`, `src/domain/keymap.rs::Chord` | present |
| Keymap composition/conflict validator | `src/persistence/keymap_edit.rs::compose_published`, `RegistryCandidate::compose` (`KEY-E401`) | present |
| Lossless settings publisher (already understands every target path) | `src/persistence/settings_publish.rs` (`agents.<id>.enabled`, `workbench.enabled_screens`, `workbench.screen_order`, `workbench.layout_overrides.<id>`, `keymap.<ctx>.<action>`) | present |
| Lossless settings writer / candidate / draft / recovery | `src/persistence/settings_edit.rs` (`SyntaxPath`, `SettingsEdit`, `SettingsCandidate`), `src/persistence/settings_document.rs::patch_assignment`, `src/state/settings.rs::reduce_settings`, `src/state/settings_types.rs::SettingsDraft` | present, **must be extended** |

### Inventory correction (issue table update)

The issue's inventory names three *new* modules. Two are new; one already
exists and is extended rather than duplicated:

- `src/state/agent_types_editor.rs::project_agent_types` — **new**.
- `src/state/screens_editor.rs::project_screens` — **new**.
- `src/state/keys_editor.rs::project_keys` — **extends the existing file**.
  `src/state/keys_editor.rs` today holds `KeysEditorState`, the state machine of
  the standalone Keys **modal**. That modal is the duplicate presenter the issue
  forbids, so it is retired and the file becomes the pure Settings presenter.

## 3. Acceptance matrix

Every row names the actor path, inputs and boundaries, success and failure
behaviour, side effects, persistence, and the test that proves it.

### A. Sparse edit vocabulary (persistence owner extension)

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| A1 | `SyntaxPath` gains parameterised leaves: `AgentEnabled(Id)`, `EnabledScreens`, `ScreenOrder`, `LayoutOverride(Id)`, `Keymap(ContextId, ActionId)` | ids already validated by their own parsers | each leaf renders its exact dotted path and table header | an id that does not parse is unrepresentable (typed constructor) | `settings_edit_tests` path/segment goldens |
| A2 | `SettingsEdit::AgentEnabled { id, enabled }` writes exactly `agents.<id>.enabled = <bool>` and touches nothing else | existing unrelated syntax, comments, dormant owners | byte diff limited to that assignment | inline-ancestor table → `E006` diagnostic, draft retained | lossless TOML golden |
| A3 | `SettingsEdit::EnabledScreens(Vec<Id>)` / `ScreenOrder(Vec<Id>)` write replacement arrays | empty vec, single, many | whole array replaced in place; each id exactly once | duplicate or unknown id rejected by the publisher, candidate blocked | golden + publisher round trip |
| A4 | `SettingsEdit::LayoutOverride { screen, layout }` writes/replaces the whole `workbench.layout_overrides.<id>` tree; `Reset` removes it | leaf, nested split, absent override | whole sub-table replaced or removed, neighbours preserved | inline ancestor → `E006` | lossless TOML golden |
| A5 | `SettingsEdit::Keymap { context, action, chords }` writes the whole `keymap.<ctx>.<action>` array; empty vec writes `[]`; `Reset` removes the assignment | 0, 1, 8 chords | exact array text, quoted context/action keys | — | lossless TOML golden (CW08-07) |
| A6 | Every new leaf is `structural()` — a saved change applies at the next start | — | Save surfaces the existing `RESTART_NOTICE` | — | reducer test |
| A7 | Existing user overrides round-trip byte-unchanged when nothing is edited, and dormant unknown syntax is preserved | file with `[extensions]`, unknown agent owner | bytes identical | — | golden |

### B. Agent Types editor

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| B1 | `project_agent_types(&snapshot, &draft) -> Vec<AgentEditorRow>` is pure: no I/O, no provider start, no registry mutation | empty registry, registry with every availability variant | one row per known type, in registry order | — | purity/row contract test |
| B2 | `AgentAvailability` projects `Compatible`, `Incompatible { reason }`, `NotFound`, `ProbeError { code, reason }` from the probe snapshot | all four upstream `Availability` variants | exact reason text preserved | — | availability matrix test (CW08-01) |
| B3 | `Provenance` distinguishes compiled default from a settings override, and `Reset` returns the inherited value | drafted, saved, absent | row reports the draft's effective value with its origin | — | provenance matrix test |
| B4 | `AgentIntent::SetEnabled` may be drafted for an **unavailable** type | `NotFound`, `ProbeError` | draft accepts, row keeps its unavailable status, candidate still validates | — | reducer test |
| B5 | `AgentIntent::Reset` removes the assignment | drafted then reset | edit removed; draft clean when it matches base | — | reducer + golden |
| B6 | Unknown dormant owners appear only as unavailable rows when the inventory supplies identity, and their bytes survive | settings naming an agent id with no definition | row shown `NotFound`, bytes preserved | — | golden + row test |

### C. Screens / Layout editor

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| C1 | `project_screens(&registry, &draft) -> Vec<ScreenEditorRow>` is pure | compiled-only registry, registry with a custom screen | one row per descriptor, ordered by the draft's `screen_order` then registry order | — | purity/order test |
| C2 | `CompositionStatus` is taken from `validate_descriptor` applied to the candidate descriptor (override applied); the editor performs no validation of its own | valid override, override naming an unknown panel, override breaking a descriptor invariant | `Valid` / `Invalid { code, reason }` with the validator's own text | — | delegation test (CW08-03) |
| C3 | `ScreenIntent::SetEnabled` rewrites both `enabled_screens` and `screen_order` so every enabled id appears exactly once and no disabled id appears | enable, disable, disable-last | replacement arrays satisfy the invariant | — | permutation + duplicate/missing rejection (CW08-02) |
| C4 | `ScreenIntent::MoveBefore` / `MoveAfter` reorder `screen_order` | move to head, tail, onto itself, onto an unknown anchor | order permuted, membership unchanged; unknown anchor is a no-op | — | reorder permutation test |
| C5 | `ScreenIntent::ReplaceLayout` accepts only a complete `LayoutNode`; the layout dialog keeps invalid intermediates local | partial node edits | no intent emitted while the node is incomplete/invalid | node dialog shows the reason and blocks apply | node-dialog invalid matrix (CW08-04) |
| C6 | `ScreenIntent::ResetLayout` removes the whole override | override present / absent | removed / no-op | — | golden |
| C7 | The layout preview uses `resolve_layout` at normal and small dimensions and never writes `AppState::resolved_layout` | 100x24 and 40x10 | preview rows reported; active geometry unchanged | `TooSmall` reported as a preview note | preview + active-geometry immutability test (CW08-03) |

### D. Keys editor

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| D1 | `project_keys(&snapshot, &draft) -> Vec<KeyEditorRow>` is pure and carries context, action, chords, availability, protected reason, provenance | bound and unbound actions | one row per action/context pair, deterministic order | — | purity/row contract test |
| D2 | A protected action projects read-only with the **exact** reason from the inventory | `core.emergency-exit` | `protected: Some(reason)`; every mutating intent for it is refused | refusal carries the same reason | protected inventory fixture (CW08-08) |
| D3 | `KeyIntent::CaptureSingleChord` accepts exactly the next non-modifier key event as one canonical chord | bare modifier press, Esc, `Ctrl-Q`, ordinary chord | modifier-only events ignored; Esc cancels; `Ctrl-Q` never captured; anything else becomes one chord | — | capture table (CW08-05) |
| D4 | `KeyIntent::SetChords` / `Unbind` / `Reset` write the whole array / `[]` / remove the assignment | 0..=8 chords, 9 chords | ≤8 accepted; 9 refused **by the owner's limit** | refusal reported, draft retained | golden + limit test (CW08-07) |
| D5 | A conflicting chord is reported by the composer as `KEY-E401` naming the chord, the context, **both** action ids, and provenance; Save is blocked | two actions bound to one chord in one context | candidate blocked with that diagnostic | — | conflict/provenance fixture (CW08-06) |
| D6 | The standalone Keys modal is retired; `core.open-keys` opens Settings on the Keys section | `core.open-keys` chord | Settings opens focused on Keys | — | action routing test + harness scenario |

### E. Settings screen host, keys, and recovery

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| E1 | Sections list gains Agent Types, Screens/Layout, Keys; the dirty/error summary counts every section's diagnostics | clean, dirty, blocked | counts and summary match the projections | — | `settings_view` tests |
| E2 | Keys: `j`/`k` rows, `Tab` controls, `Space` toggles, `K`/`J` and Alt-Up/Down reorder, `Enter` opens layout/capture, `Delete` unbinds, `r` resets, `s` saves, `q`/`Esc` Back with dirty guard, `Ctrl-Q` exit | each key in each section | exactly the documented intent | keys with no meaning in a section are ignored | app-input tests + keybind bar |
| E3 | On hash conflict or write failure the draft **and** the active registries are retained | conflict, write failure | existing recovery choices offered; `AppState` registries byte-identical | — | recovery integration (CW08-09) |
| E4 | Each editor renders in all seven states preserving accessibility markers and the protected `Ctrl-Q` exit | NORMAL, FOCUSED, UNAVAILABLE, ERROR, DIRTY, RECOVERY, SMALL | state-specific literal markers present | — | 21 tmux scenarios (CW08-10) |

## 4. Explicit non-goals

1. **Runtime consumption of `workbench.screen_order` and `workbench.layout_overrides`.** Both are published today and consumed by nothing. CW-08 owns the *editors*: it writes the exact sparse syntax, validates the candidate through the descriptor/layout owner, and previews with the standard resolver. Making the startup composition honour them is a separate workbench slice (follow-up issue).
2. No change to the agent probe, no provider execution, no shell, no secret exposure.
3. No new dependency, no new quality gate, no gate relaxation.
4. No change to `.github/`, `.llxprt/`, `.code_puppy/`, `Cargo.toml` dependency set.
5. No generic-map or path-string intent payloads; every intent is closed and typed.
6. No new theme, no change to the CW-07 General/Appearance/Diagnostics sections beyond adding sections to the list.

## 5. Vertical slices

| Slice | Acceptance rows | Owner boundary | Allowed paths |
|---|---|---|---|
| S1 | A1–A7 | persistence (lossless writer) | `src/persistence/settings_edit*.rs`, `src/persistence/settings_document.rs` |
| S2 | B1–B6 | state presenter + reducer | `src/state/agent_types_editor*.rs`, `src/state/settings*.rs`, `src/messages/settings.rs` |
| S3 | C1–C7 | state presenter + reducer | `src/state/screens_editor*.rs`, `src/state/settings*.rs`, `src/messages/settings.rs` |
| S4 | D1–D5 | state presenter + reducer | `src/state/keys_editor*.rs`, `src/state/settings*.rs`, `src/messages/settings.rs` |
| S5 | D6, E1–E3 | UI + app-input cutover | `src/ui/screens/settings.rs`, `src/ui/modals/`, `src/app_input/`, `src/state/modal_ops.rs`, `src/messages*` |
| S6 | E4 | harness + docs | `dev-docs/tmux-scenarios/`, `dev-docs/testing/tmux-harness.md`, `dev-docs/standards/*.md` |

## 6. Scope ledger

| Entry | Reason | Disposition |
|---|---|---|
| `SyntaxPath` becomes a data-carrying enum (loses `Copy`) | required by A1; callers in `settings.rs`, `settings_view.rs`, `messages/settings.rs` follow | in scope |
| Retire the standalone Keys modal | the issue forbids a duplicate presenter; D6 | in scope |
| `screen_order` / `layout_overrides` runtime consumption | not required by any EARS row | non-goal, follow-up |

## 7. Review counters

- Local OCR runs: 0 / 2
- PR OCR runs: 0 / 2
- Design/code review cycles: 0 / 2

## 8. Verification

`make ci-check` (fmt, clippy gates, coverage, build, test) plus
`scripts/check-architecture.sh` and the `tests/core` contract suite, on the exact
candidate head.
