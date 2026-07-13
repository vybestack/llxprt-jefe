# CW-07: Core Settings shell and lossless draft UI

## Outcome and consumed contracts

Deliver `core.settings` with General, Appearance, and Diagnostics sections over the lossless schema-2 document/writer and common navigation dirty guard. Consume loaded document bytes/SHA-256/provenance, closed persistence effects, descriptor/layout snapshots, and Save/Discard/Cancel navigation. Do not duplicate parsing/writing or add Agent, Screen, Key, or Plugin editors.

## Source/symbol inventory

| Source/symbol | Required responsibility/parity |
|---|---|
| `src/persistence/mod.rs::{Settings,State}` | expose typed snapshot, lossless document revision, loaded SHA-256, writer port |
| `src/theme/mod.rs` | apply/adopt/revert a generation-bound `ThemePreviewToken` |
| `src/ui/screens/theme_picker.rs` | migrate existing theme choices/keys into Appearance presenter |
| `src/state/theme_picker_view.rs` | preserve theme labels/availability as pure projection |
| new `src/state/settings.rs::reduce_settings` | sole Settings draft/pending save/reload/export state owner |
| new `src/ui/screens/settings.rs` | thin renderer and typed intent emitter |
| descriptor registry | declare `core.settings` and General/Appearance/Diagnostics panels |
| navigation reducer | own Back dirty confirmation and focus restoration |

## Closed state and algorithms

```rust
enum SettingsSection { General, Appearance, Diagnostics }
struct SettingsDraft { token: DraftToken, base_hash: Sha256, base_document_revision: u64,
 edited_paths: BTreeSet<SyntaxPath>, candidate: TypedSettings,
 validation: Vec<Diagnostic>, preview: Option<ThemePreviewToken>, status: DraftStatus }
enum DraftStatus { Clean, Dirty, Saving { revision: u64 },
 Conflict { disk_hash: Sha256 }, Failed { code: DiagnosticCode } }
struct ThemePreviewToken { id: PreviewId, generation: u64, prior_theme: ThemeId,
 preview_theme: ThemeId }
enum SettingsIntent { Edit { path, value }, Reset { path }, Save, SaveAndExit,
 Discard, Cancel, ReloadDisk, ExportDraft { path: RelativePath }, Retry }
```

Open reads one typed snapshot and lossless document identity. Edit mutates candidate only, adds exact syntax path, validates the complete candidate, and never changes structural registries. Appearance theme edit applies one reversible preview; another edit replaces preview while retaining the original prior theme. Cancel/Discard/reload/failed Save reverts exact prior theme; successful Save adopts preview and clears token.

Save requires no validation errors; emits full candidate plus edited paths, base hash, draft token, and monotonically increasing revision. Writer rereads disk, compares SHA-256, patches only edited nodes, performs mode-0600 atomic phases, and returns new bytes/hash. A matching completion updates base and clean state; stale completion is ignored. SaveAndExit exits only after success. `CFG-E007` conflict preserves disk and draft. Reload rereads disk only after explicit confirmation when dirty, rebuilds draft, and loses no disk bytes. Export writes a redacted canonical TOML representation of the draft to an explicitly selected contained path, mode 0600, without changing base/hash/dirty status; secret references remain references. Retry reruns validation/hash check, never blind-overwrites.

Consumed settings grammar is required `settings_schema=2` with only `appearance`, `workbench`, `agents`, `keymap`, `plugins`, `extensions`; active known owners are closed; unknown owner subtrees and extensions are dormant/lossless; reset removes syntax. Bounds: file 1,048,576, depth 16, map 256, array 1,024, string 262,144, path 4,096, diagnostics 256, origins 16, edited paths 256. Diagnostics sorted error/warning/info then path/span/code and never contain secrets.

General edits host scalar settings already present in schema. Appearance edits theme and override-agent-theme only. Diagnostics is read-only and shows code, severity, path/span, owner/version, provenance, correction, and redacted detail. Structural saves display exactly `Restart Jefe to apply structural changes`; v1 never hot reloads or self-executes.

## End-to-end, migration, recovery, security

Open Settings, clone snapshot identity, edit/preview, validate, choose Save, commit pending state, release state access, execute writer, then adopt completion. Schema-1 settings use the in-memory migration view and are written as schema 2 only on explicit Save. Validation failure focuses first error without write. Hash conflict offers Reload, Export, Retry, and Back. Atomic failure offers Retry/Export/Discard. Export failure retains draft. No provider/process/network starts; no secret value appears in diagnostics, preview, export, logs, or state.

Keys: `,` opens; j/k or arrows select; Tab/Shift-Tab focuses; Enter/Space activates; Left/Right selects; `s` Save; `S` SaveAndExit; `r` Reset; q/Esc Back; ? Help; Ctrl-Q exit.

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Settings -----------------+ + Settings -----------------+
| General                   | |>>Appearance               |
| Appearance                | | Theme: green-screen      |
| Diagnostics (2)           | | s Save                   |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Appearance ---------------+ + Settings -----------------+
| Theme missing-theme       | | theme has wrong type     |
| unavailable: not installed| | CFG-E003 Save blocked    |
+---------------------------+ +---------------------------+
```
```text
DIRTY                          RECOVERY
+ Save changes? ------------+ + External edit detected --+
|>>Save  Discard  Cancel    | | disk and draft preserved |
+---------------------------+ |>>Reload Export Retry     |
                               +---------------------------+
```
```text
SMALL
+Settings--------+
|>>Appearance    |
| theme: green   |
| ! 1 error      |
| q Back Ctrl-Q  |
+----------------+
```

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW07-01 | WHEN Settings opens, Jefe shall bind draft to exact bytes/hash/revision. | settings-open identity test |
| CW07-02 | WHILE a structural draft is unsaved, Jefe shall leave active registries unchanged. | `settings-structural-draft.json` |
| CW07-03 | WHEN theme preview changes/cancels, Jefe shall retain original prior theme and restore it exactly. | preview replacement/cancel matrix |
| CW07-04 | WHEN a valid matching-hash draft saves, Jefe shall patch only edited syntax paths. | comments/order/quotes/dormant golden |
| CW07-05 | IF validation fails, Jefe shall retain draft, focus first error, and perform no write. | all General/Appearance invalid types |
| CW07-06 | IF disk hash differs, Jefe shall retain disk/draft and offer Reload/Export/Retry. | `settings-external-edit.json` |
| CW07-07 | WHEN Reload is confirmed, Jefe shall rebuild from exact current disk bytes. | dirty/clean reload scenarios |
| CW07-08 | WHEN Export succeeds or fails, Jefe shall preserve draft/base and redact observations. | export mode/path/failure fixtures |
| CW07-09 | IF writer completion is stale, Jefe shall retain the newest pending revision. | revision property |
| CW07-10 | WHEN schema-1 view saves, Jefe shall explicitly create schema 2 without losing dormant syntax. | migration-save golden |
| CW07-11 | WHEN each UI state renders, Jefe shall preserve focus, adjacent error, modal trap, and protected exit. | distinct state scenarios |

RED all scenarios first; GREEN reducer/presenter/UI; REFACTOR only toward existing persistence/navigation authority.

## Normative documentation and done

Update `dev-docs/standards/display-and-ui.md` with Settings keys, state mocks, accessibility, reload/export behavior; update `dev-docs/standards/persistence-and-runtime.md` with draft/hash/revision/preview ownership and save flow. Done requires lossless byte goldens, every writer phase, all UI states, and unchanged `make ci-check`; no duplicate writer/parser, active structural mutation, self-restart, new dependency, or gate weakening.