# CW-08: Agent Types, Screens/Layout, and Keys editors

## Outcome and consumed contracts

Add three Settings presenters and typed draft intents. Consume immutable agent status/provenance, screen descriptor/composition/layout validation, action/key availability/conflict/protected metadata, and Settings draft/hash/writer/reload/export behavior. Existing owners remain the only validators and serialization authorities; editors never start providers or mutate active registries.

## Exact source and responsibility inventory

| Source/symbol | Required responsibility |
|---|---|
| `src/state/settings.rs::reduce_settings` | receive the closed editor intents and update sparse draft paths |
| `src/ui/screens/settings.rs` | host section navigation and dirty/error summary |
| new `src/state/agent_types_editor.rs::project_agent_types` | pure rows from agent registry/probe snapshot |
| new `src/state/screens_editor.rs::project_screens` | pure rows/tree from descriptor registry and draft candidate |
| new `src/state/keys_editor.rs::project_keys` | pure rows from action/key resolver and candidate |
| agent registry validator | sole enablement/status validator |
| descriptor/layout validator | sole screen order/tree validator |
| action/key resolver | sole chord grammar/conflict/protected validator |
| lossless Settings writer | sole serializer and disk writer |

If equivalent modules already exist, extend them and update this inventory; no duplicate presenter/validator is permitted.

## Closed view/intents and exact payloads

```rust
struct AgentEditorRow { type_id: AgentTypeId, display_name: String, enabled: bool,
 availability: AgentAvailability, provenance: Provenance }
enum AgentAvailability { Compatible, Incompatible { reason: String }, NotFound,
 ProbeError { code: String, reason: String } }
enum AgentIntent { SetEnabled { type_id, enabled: bool }, Reset { type_id } }
struct ScreenEditorRow { screen_id: ScreenId, title: String, enabled: bool,
 order_index: u16, composition: CompositionStatus, provenance: Provenance }
enum CompositionStatus { Valid, Invalid { code: String, reason: String } }
enum ScreenIntent { SetEnabled { screen_id, enabled: bool }, MoveBefore { screen_id, anchor },
 MoveAfter { screen_id, anchor }, ReplaceLayout { screen_id, layout: LayoutNode }, ResetLayout { screen_id } }
struct KeyEditorRow { context: ContextId, action: ActionId, chords: Vec<Chord>,
 availability: ActionAvailability, protected: Option<String>, provenance: Provenance }
enum KeyIntent { CaptureSingleChord { context, action, chord },
 SetChords { context, action, chords: Vec<Chord> }, Unbind { context, action },
 Reset { context, action } }
```

Serialization is exact: agent intents patch/remove `agents.<id>.enabled`; screen enable/order writes replacement arrays `workbench.enabled_screens` and `workbench.screen_order`, each enabled ID exactly once and no disabled ID; layout writes/replaces/removes the whole `workbench.layout_overrides.<id>` tree; key set writes whole `keymap.<context>.<action>` array, Unbind writes `[]`, Reset removes syntax. Capture accepts exactly the next non-modifier key event as one canonical chord; Esc cancels capture, Ctrl-Q remains protected and is never captured. Maximum 8 chords/action-context and 2,048 effective bindings. Candidate is revalidated after each intent; `KEY-E401` includes chord, context, both action IDs, and provenance. Protected actions are read-only with exact reason.

Screen layout editor flow: select screen, Enter opens tree, j/k selects node, h/l selects parent/child, `a` adds a leaf/split through a closed chooser, `x` removes only when descriptor invariants remain satisfiable, `e` edits axis/size/min/max/collapsible/priority through typed fields, Enter applies candidate, Esc cancels node edit, `r` resets whole override. Invalid intermediate edit remains local to the node dialog; only a complete valid `LayoutNode` reaches `ReplaceLayout`. The standard resolver previews normal and small dimensions without changing active geometry.

Agent enablement may be drafted while unavailable, but whole candidate must obey owner publication rules and displays restart requirement. Unknown dormant owners remain byte-preserved and appear only as unavailable rows when inventory supplies identity. Reset returns inherited provenance.

## Complete flow, migration, errors, security

Immutable snapshot becomes rows; UI emits typed intent; Settings reducer patches candidate; authoritative validator returns diagnostics; Save follows common hash/atomic flow; restart composes new registries. Existing compiled defaults appear with provenance and no syntax. Existing user overrides round-trip unchanged. Conflict/write/reload/export use Settings recovery. No executable, provider, shell, secret value, generic map payload, or direct file I/O is available to editors.

Keys: `,` Settings; j/k rows; Tab controls; Space toggles; `K/J` or Alt-Up/Down reorders; Enter opens layout/capture; Delete unbinds; `r` resets; `s` saves; q/Esc Back/dirty guard; Ctrl-Q exit.

## Distinct state mocks for each editor

```text
NORMAL                         FOCUSED
+ Agent Types --------------+ + Screens ------------------+
| Claude Code [x] Compatible| |>>github.issues [x]       |
| Codex CLI   [ ] Not found | | K/J reorder Enter layout|
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Keys ---------------------+ + Keys ---------------------+
| Merge protected           | | Ctrl+M conflicts         |
| reason: emergency binding | | merge / move KEY-E401    |
+---------------------------+ +---------------------------+
```
```text
DIRTY                          RECOVERY
+ Save registry edits? -----+ + Screens ------------------+
|>>Save  Discard  Cancel    | | invalid override retained|
+---------------------------+ |>>Reset Export Back       |
                               +---------------------------+
```
```text
SMALL
+Layout----------+
|>>split H       |
| child: list    |
| ! min invalid  |
| q Back Ctrl-Q  |
+----------------+
```

Normal/focused/unavailable/error/dirty/recovery/small scenarios must run separately for Agent Types, Screens/Layout, and Keys; a shared mock is not sufficient.

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW08-01 | WHEN an agent toggle is drafted, Jefe shall serialize only the sparse enabled path and apply after restart. | agent status/provenance matrix |
| CW08-02 | WHEN screens reorder, Jefe shall serialize every enabled screen exactly once. | reorder permutations and duplicate/missing rejection |
| CW08-03 | WHEN layout editing completes, Jefe shall pass one complete tree to the sole validator and preview resolver. | add/remove/edit/cancel/reset flow scenario |
| CW08-04 | IF a layout intermediate is invalid, Jefe shall keep it local and block candidate application. | node dialog invalid matrix |
| CW08-05 | WHEN one chord is captured, Jefe shall canonicalize only the next eligible key. | modifiers/Esc/Ctrl-Q capture table |
| CW08-06 | IF a chord conflicts, Jefe shall identify both owners/context and block Save. | conflict/provenance fixture |
| CW08-07 | WHEN Unbind or Reset is chosen, Jefe shall write empty array or remove syntax respectively. | lossless TOML golden |
| CW08-08 | WHEN a protected action projects, Jefe shall expose read-only exact reason. | protected inventory fixture |
| CW08-09 | IF hash/write fails, Jefe shall retain draft and active registries. | common Settings recovery integration |
| CW08-10 | WHEN each editor renders each state, Jefe shall preserve accessibility and protected exit. | 21 distinct harness states |

RED fixtures and scenarios first; GREEN pure presenters/intents; REFACTOR removes editor-local validation.

## Normative documentation and done

Update `dev-docs/standards/display-and-ui.md` with exact editor controls/layout flow/state behavior and `dev-docs/standards/persistence-and-runtime.md` with exact sparse payload serialization/restart semantics. Done requires payload goldens, layout flow, all 21 UI state runs, active-registry immutability, and unchanged `make ci-check`; no validator/writer duplication, provider execution, dependency, or gate weakening.