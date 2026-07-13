# CW-05: External custom screens lowered to descriptors and typed relationships

## Outcome and consumed contracts

Discover user screen files deterministically, parse and validate one closed syntax, lower it once into the internal descriptor contract, and propagate typed same-screen relationships in bounded reducer transitions. Consume schema-2 lossless owner activation/dormancy and diagnostics, action IDs/contexts, and the exact descriptor/layout types and allocation algorithm. Do not add a renderer, geometry engine, navigation stack, provider runtime, or editor.

## Source and symbol inventory

| Source/symbol | Responsibility after change |
|---|---|
| `src/persistence/paths.rs::ResolvedPaths::definitions_dir` | sole root for user screen discovery |
| `src/app_init.rs` | request discovery/composition before atomic registry publication |
| `src/state/types.rs` | hold stable screen/instance/panel IDs, never external syntax |
| new `src/domain/workbench/screen_file.rs` | duplicate-key/unknown-field parser with spans |
| new `src/domain/workbench/screen_lowering.rs::lower_screen` | sole external-to-internal conversion |
| new `src/domain/workbench/relationships.rs` | port graph validation and pure bounded propagation |
| internal descriptor registry | compose lowered and compiled descriptors transactionally |
| `src/ui/screens/issues.rs` and `pull_requests.rs` | preserve list/detail semantics through declared bundled relationships |

Discovery examines direct regular files named `<local-member>.screen.toml` only, where member matches `[a-z][a-z0-9-]{0,62}`. No recursive scan, symlink, hidden file, extension alias, or non-UTF-8 name. Sort by canonical path bytes; duplicate IDs are fatal. File is bounded before parse.

## Closed syntax and lowering

```text
ScreenFile={screen_schema:1,id:local.*,title,route,activation:[Field;0..32],initial_focus,
 focus_order:[PanelId],panels:[Panel;1..16],layout:Layout,
 relationships:[Relationship;0..64],bindings:[BindingRef;0..256]}
Panel={id,type,config:TypedMap,focusable:bool,required:bool,ports:[Port;0..32]}
Port={id,direction:"input"|"output",type_id:VersionedTypeId,required:bool,retained:bool}
Layout={type:"leaf",panel}|{type:"split",axis:"horizontal"|"vertical",children:[Child;2..8]}
Child={node:Layout,size:{fixed:1..65535}|{weight:1..65535},min:1..65535,
 max?:1..65535,collapsible:bool,collapse_priority?:i32}
Relationship={kind:"scope",source:PortRef,target:PortRef}|
 {kind:"master-detail",source,target,activation:"immediate"|"explicit",
 empty:"show-none"|"show-all"|"retain"}|
 {kind:"session-target",source,target,empty:"detach"|"retain"}
BindingRef={context:ContextId,action:ActionId}
```

Objects are closed and reject duplicate keys. Activation fields use only boolean, optional-boolean, string, integer, enum, path, and string-list; no secret. Every panel occurs once in layout, every focusable panel once in focus order, initial focus is focusable, max >= min, and collapse priority is present exactly when collapsible. Port references are `<panel>.<port>` and require output-to-input exact type/version. Graph is same-screen, acyclic, one incoming controlling edge per target, one outgoing `(source,kind)`, and no same-kind fan-out. Limits: file 1,048,576 bytes; data/layout depth 16/8; maps/arrays 256/1,024; strings 262,144; IDs 128; paths 4,096; screens 64; panels 16; relationships/follow-ups 64.

`lower_screen` copies IDs/title/route/panels/focus/layout into the internal descriptor without semantic defaults. It resolves panel types and actions against immutable registries, preserving source span/provenance. No external DTO survives publication. Existing five screens remain compiled; bundled Issue/PR list-detail couplings are represented by equivalent internal relationships and parity-tested.

Propagation uses declaration order in the same committed transition. Immediate edges update target; explicit edges stage source until the declared activation action. Source deletion emits `None`. `show-none` clears target, `show-all` sets typed all-value, `retain` leaves prior target, and `detach` clears session attachment. Nonretained input clears on `None`; retained input follows relationship policy. Relationships never move focus. Attempted follow-up 65 aborts the transition with `SCR-E301`; no partial state.

Inactive invalid owner files emit `CFG-W004`, retain bytes, and do not lower. Active invalid files emit `SCR-E301` plus `CFG-E005/CFG-E006`, reject the whole candidate registry, start no provider, and retain prior authority. There is no earlier custom-screen schema to migrate and no automatic rewrite.

## End-to-end flow and security

Resolve definitions root; enumerate exact direct entries; read bounded bytes without following symlinks; parse all files with spans; determine active/dormant owner from settings; validate active panel/action/port/graph references; lower active files; compose with compiled descriptors; atomically publish. Selection produces semantic source intent; reducer computes all relationship updates, validates the bound, commits once, then renders through the standard resolver. Recovery is edit/disable the named file offline and restart. Syntax, paths, and diagnostics never contain secret values; custom files cannot request I/O, commands, raw drawing, input interception, PTY, cross-screen mutation, or arbitrary effects.

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Review --------+ Detail +   + Review --------+ Detail +
|  PR 41         | none    |   |>>PR 42         | #42     |
|  PR 42         |         |   | Enter activate |         |
+----------------+---------+   +----------------+---------+
```
```text
UNAVAILABLE                    ERROR
+ Screens ------------------+  + Startup blocked --------+
| local.review inactive     |  | SCR-E301 cycle at line 8|
| dependency unavailable    |  | fix/disable then restart|
+---------------------------+  +--------------------------+
```
```text
DIRTY                          RECOVERY
N/A: no draft/editor exists.   + Screens -----------------+
                               | invalid inactive omitted |
                               | CFG-W004 bytes preserved |
                               +--------------------------+
```
```text
SMALL
+Review----------+
|>>PR 42         |
| detail hidden  |
| q Back Ctrl-Q  |
+----------------+
```

Tab/Shift-Tab focuses; arrows/j/k select; Enter activates explicit relation; q/Esc Back; ? Help; Ctrl-Q exit. Focus/status are textual and clipping is grapheme-safe.

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW05-01 | WHEN files are discovered, Jefe shall accept only exact direct regular names in canonical order. | discovery type/name/symlink/order matrix |
| CW05-02 | WHEN a valid active file parses, Jefe shall lower it exactly once to the internal descriptor. | `custom-screen-enable.json`; syntax/descriptor golden |
| CW05-03 | IF an inactive file is invalid, Jefe shall preserve and omit it with `CFG-W004`. | `custom-screen-inactive-invalid.json` |
| CW05-04 | IF an active file is invalid, Jefe shall reject candidate publication without partial registry. | owner/reference/layout/type fixtures |
| CW05-05 | WHEN an immediate relationship changes, Jefe shall update source and target in one transition. | master-detail immediate reducer test |
| CW05-06 | WHEN an explicit relationship changes, Jefe shall wait for its declared action. | explicit activation scenario |
| CW05-07 | WHEN a source becomes absent, Jefe shall apply each closed empty/retained policy exactly. | deletion policy table |
| CW05-08 | IF graph scope, cycle, type, incoming, fan-out, or follow-up bound fails, Jefe shall reject it. | exhaustive graph invalid matrix |
| CW05-09 | WHEN bundled Issue/PR relationships run, Jefe shall preserve current list-detail behavior. | old/new parity scenarios |
| CW05-10 | WHEN a custom screen is tiny, Jefe shall use the standard collapse/focus algorithm. | `custom-screen-tiny.json` |

RED adds all fixtures first; GREEN parser/validator/lowerer/reducer; REFACTOR removes embedded list-detail coupling only after parity.

## Documentation and done

Update `dev-docs/standards/architecture.md` with discovery and lowering ownership and `dev-docs/standards/display-and-ui.md` with the complete screen/port/relationship grammar and policies. Done requires every parser bound at-limit/+1, all UI-applicable states, no second runtime/geometry, and unchanged `make ci-check`; no dependency or gate weakening.