# Issue #387 — CW-07: Core Settings shell and lossless draft UI

Branch: `issue387` (from `origin/main` @ `6b6d9289`)

## 1. Ground truth established before shaping

Verified in the tree, not assumed:

- **CW-01 is landed.** `src/persistence/settings_document.rs` holds the lossless
  document (`original_bytes`, `sha256`, `node(path) -> SyntaxNode` with
  `key_span`/`value_span`/`statement_span`, `table_span`, `comment_spans`,
  `apply_patches`). `src/persistence/settings_publish.rs` publishes the closed
  typed model `PublishedSettings` (this *is* the issue's `TypedSettings`).
  `src/persistence/writer.rs` performs the revisioned, hash-gated, mode-0600
  atomic write and returns `WriteOutcome::{Authoritative,Stale}` /
  `WriteError` carrying `CfgCode::E007` on hash conflict and `CfgCode::E104` on
  write failure. `src/persistence/diagnostic.rs` already defines `Diagnostic`,
  `CfgCode` (the `CFG-E###` namespace), `Severity`, and every bound the issue
  lists (`FILE_LIMIT` 1 048 576, `NESTING_LIMIT` 16, `MAP_LIMIT` 256,
  `ARRAY_LIMIT` 1 024, `STRING_LIMIT` 262 144, `PATH_LIMIT` 4 096).
  `crate::domain::sha256::Sha256` is the digest type.
  **Therefore CW-07 introduces no diagnostic taxonomy, no digest type, and no
  writer.**
- **CW-01's schema-1 settings view is landed.**
  `persistence::migration::{migrate_settings, format_migrated_settings,
  SettingsMigration}` already reads a schema-1 document into a schema-2
  `PublishedSettings` in memory and can format an explicit schema-2 candidate
  that preserves dormant syntax. CW-07 consumes it; it does not re-migrate.
- **CW-04 is landed.** `src/workbench/` holds the descriptor registry
  (`screen_registry`, `screen_descriptor`, `ScreenId`, `PanelId`,
  `ScreenDescriptor`) and `resolve_layout`. `PanelState::hiding` is the
  supported way an application hides a declared panel, and
  `src/screen_layout.rs::hidden_panel_ids` is the single place that decides it.
- **CW-06 is landed.** `src/state/navigation.rs::reduce_navigation` is the sole
  screen-change authority; `src/state/navigation_dirty.rs` already owns
  `DraftToken`, `SaveIntent`, `DirtyState`, `DirtyChoice`, `GuardPhase`,
  `DirtyGuard`, and `DraftAction`. `AppState::{enter_screen, leave_screen,
  mark_screen_dirty, mark_screen_clean, resolve_dirty}` are the verbs.
  `navigation_dirty.rs`'s own module docs name "the settings shell that follows
  this capability" as the owner of its draft, writer, and completion.
  **Therefore CW-07 writes no navigation or dirty-guard logic.**
- **A closed lossless editor precedent already exists.**
  `src/persistence/keymap_edit.rs::{KeymapEdit, KeymapCandidate}` builds a
  complete candidate from typed edits by patching only the selected syntax spans
  and re-validating the whole document, and
  `src/app_input/keys_editor.rs` executes the write at the boundary through
  `FilePersistenceManager::save_keymap_candidate_revisioned`. CW-07's settings
  candidate follows this exact shape rather than inventing a second one.
- **The current theme picker is the settings-destroying path CW-07 replaces.**
  `src/app_input/modal_handlers.rs::apply_theme_picker_selection` persists by
  `PersistenceManager::load_settings()` (legacy `Settings` struct) →
  `save_settings_to` → `toml::to_string_pretty(Settings)`. That rewrites the
  whole file as `schema_version`/`theme`/`override_agent_theme` at the root —
  which is not even valid schema 2 — destroying comments, ordering, keymap, and
  every dormant subtree. The issue's inventory line "migrate existing theme
  choices/keys into Appearance presenter" is therefore a cutover, not an
  addition: leaving both authorities in place would keep a live data-loss path.
- **`,` is currently bound to `core.open-keys`** in the `global` context
  (`src/domain/default_action_inventory.rs`), and `F9` is bound to
  `dashboard.open-theme-picker` in the `dashboard` context. The issue's key
  table assigns `,` to Settings. Duplicate chords in one effective context are a
  composition error, so delivering `,` requires relocating `core.open-keys`.
  Retiring the theme picker frees `F9`, which is the chord `core.open-keys`
  moves onto. Net default-binding count is unchanged and no chord is lost.
- **Layering.** `domain` may not depend on `workbench` or `state`;
  `persistence` depends on `domain`; `state` depends on both; `ui`/`app_input`
  sit above. So the candidate builder belongs in `persistence`, the draft
  reducer in `state`, the projection in `state`, and the renderer in `ui`.

## 2. Acceptance matrix

| # | Ledger | Behavior | Inputs / boundary cases | Failure behavior | Evidence |
|---|---|---|---|---|---|
| A1 | CW07-01 | Opening Settings binds one draft to the exact loaded bytes, their SHA-256, and the current document revision, and clones one `PublishedSettings` snapshot | schema-2 document; schema-1 document; absent document | no document ⇒ draft binds `ExpectedHash::Absent` and a defaults snapshot | `src/state/settings_open_tests.rs` |
| A2 | CW07-01 | A freshly opened draft is `DraftStatus::Clean`, has no edited paths, no preview, and no diagnostics | — | — | same |
| B1 | CW07-02 | An `Edit` mutates only the draft candidate and records the exact syntax path; the published/active settings, theme manager, keymap registry, and screen registry are unchanged while the draft is unsaved | General scalar; Appearance theme; Appearance override toggle | — | `settings-structural-draft.json` scenario + `src/state/settings_edit_tests.rs` |
| B2 | — | `Edit` is refused for any path outside the section's ownership (General = host scalars present in schema; Appearance = `theme` and `override_agent_theme`; Diagnostics read-only) | unowned path; Diagnostics write | refused ⇒ state byte-identical, `CFG-E005` diagnostic | same |
| B3 | — | `Reset { path }` removes the source assignment so the compiled default is inherited, and drops the path from `edited_paths` | reset an edited path; reset an unedited path | — | same |
| B4 | — | `edited_paths` is bounded at 256; the 257th distinct edit is refused with `CFG-E008` and changes nothing | 256 ok, 257 refused | — | bound test |
| C1 | CW07-03 | An Appearance theme edit applies exactly one reversible preview; a second theme edit replaces the preview theme and retains the **original** prior theme | one edit; two edits; edit back to prior | — | `src/theme/preview_tests.rs` + `src/state/settings_preview_tests.rs` |
| C2 | CW07-03 | Cancel, Discard, confirmed Reload, and a failed Save each restore the exact prior theme and clear the token | each of the four paths | — | preview matrix test |
| C3 | CW07-03 | A successful Save adopts the preview as the active theme and clears the token | — | — | same |
| C4 | CW07-03 | A preview token whose generation is not the live generation is ignored by apply/adopt/revert | stale generation | state unchanged | preview generation test |
| D1 | CW07-04 | A valid, matching-hash Save patches only the edited syntax spans: comments, key order, quoting style, and dormant subtrees are byte-preserved | comment before/after, single vs double quotes, `[extensions.*]`, unknown owner subtree | — | `src/persistence/settings_edit_tests.rs` golden |
| D2 | CW07-04 | An edited path with no existing assignment is inserted into its owning table, or the table is created, without disturbing other bytes | missing key in existing table; missing table | — | same |
| D3 | CW07-10 | Saving a schema-1 view explicitly writes schema 2 and preserves every dormant subtree; schema 1 is never written and never rewritten on load | schema-1 doc with dormant subtree | — | migration-save golden |
| E1 | CW07-05 | Validation runs over the **complete** candidate; any error leaves the draft intact, focuses the first diagnostic in sort order, and performs no write | wrong type for `theme`; wrong type for `override_agent_theme`; unowned root; over-limit string | Save refused, `DraftStatus` stays `Dirty` | `src/state/settings_validate_tests.rs` |
| E2 | CW07-05 | Diagnostics are sorted error → warning → info, then path, span, code, are bounded at 256, and never contain a secret value or a raw user value | mixed severities; secret-ref field | — | sort/redaction test |
| F1 | — | Save requires zero validation errors, moves the draft to `Saving { revision }` with a strictly increasing revision, and emits exactly one writer request carrying candidate bytes, base hash, draft token and revision | clean draft; dirty valid draft; dirty invalid draft | invalid ⇒ no request emitted | `src/state/settings_save_tests.rs` |
| F2 | CW07-09 | A completion whose revision is older than the newest pending revision is ignored; the newest pending revision is retained | in-order; out-of-order; duplicate | — | revision property test |
| F3 | — | A matching successful completion adopts the new bytes/hash/revision, clears `edited_paths`, and returns the draft to `Clean` | — | — | same |
| F4 | — | `SaveAndExit` leaves the screen only after a matching successful completion; a failure keeps the user on Settings with the draft intact | success; failure; stale | — | same |
| G1 | CW07-06 | A `CFG-E007` hash mismatch preserves both the disk bytes and the draft and moves to `Conflict { disk_hash }`, offering Reload, Export, Retry and Back | external edit between open and Save | no write performed | `settings-external-edit.json` + conflict tests |
| G2 | — | A `CFG-E104` atomic failure moves to `Failed { code }` and offers Retry, Export and Discard, retaining the draft | injected writer-phase failure | — | failure test |
| G3 | — | Retry reruns validation and the hash check and never blind-overwrites | retry after conflict; retry after failure | still conflicting ⇒ `Conflict` again | retry test |
| H1 | CW07-07 | Reload rereads the exact current disk bytes and rebuilds the draft from them, losing no disk bytes | clean draft; dirty draft | dirty ⇒ reload only after explicit confirmation | reload tests |
| I1 | CW07-08 | Export writes a redacted canonical TOML representation of the draft to an explicitly selected **contained** relative path, mode 0600, leaving base hash, revision and dirty status unchanged; secret references stay references | contained path; `..` escape; absolute path; existing file | escape/absolute ⇒ refused with `CFG-E101`, draft unchanged | export tests |
| I2 | CW07-08 | Export failure retains the draft and reports a redacted diagnostic | unwritable target | — | same |
| J1 | CW07-11 | The Settings screen renders NORMAL, FOCUSED, UNAVAILABLE, ERROR, DIRTY, RECOVERY and SMALL, preserving section focus, the adjacent first error, the modal trap, and the protected `Ctrl-Q` exit | wide and small terminals | — | `src/state/settings_view_tests.rs` + tmux scenarios |
| J2 | — | An unavailable (not installed) theme is shown as `unavailable: not installed` and is not silently substituted | settings naming a missing theme | — | view test + `settings-unavailable-theme.json` |
| J3 | — | A structural save displays exactly `Restart Jefe to apply structural changes`, and nothing hot-reloads or self-executes | structural edit saved | — | view test |
| K1 | — | Back from a dirty Settings screen raises the **existing** navigation dirty guard with a `SaveIntent::Owner` naming `core.settings`, and Save/Discard/Cancel resolve through `reduce_navigation` | dirty; clean | clean ⇒ no guard | `src/state/settings_navigation_tests.rs` + `settings-dirty-back.json` |
| L1 | — | The theme picker modal is retired: `,` opens Settings, `F9` opens the Keys editor, and theme selection/override are edited only through Settings Appearance over the lossless writer | every previous theme-picker entry point | — | inventory tests + `theme-override-toggle.json` update |

## 3. Explicit non-goals

- Agent, Screen/Layout, Keys, and Plugin editors inside Settings (CW-09/CW-11).
  The existing standalone Keys editor keeps its current behavior and simply
  moves chord.
- Hot reload, self-restart, self-exec, provider/process/network start, or
  applying structural changes without a restart.
- A second parser, writer, digest, diagnostic taxonomy, or navigation authority.
- Persisting draft, preview, or Settings UI state.
- New dependencies, `unsafe`, `unwrap`/`expect` in production paths,
  suppression directives, or any weakened lint/complexity/size/coverage/CI gate.
- Any change to `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifests, or
  quality-gate scripts.

## 4. Planned vertical slices

1. **S1 — Lossless settings candidate** (`src/persistence/settings_edit.rs`):
   `SyntaxPath`, `SettingsEdit`, `SettingsCandidate`,
   `save_settings_candidate_revisioned`, schema-1 migration-save.
   RED: D1–D3, B4.
2. **S2 — Reversible theme preview** (`src/theme/preview.rs` + `ThemeManager`):
   `ThemeId`, `PreviewId`, `ThemePreviewToken`, apply/adopt/revert.
   RED: C1–C4.
3. **S3 — Draft reducer** (`src/state/settings.rs`, `src/state/settings_types.rs`,
   `src/messages/settings.rs`): `SettingsSection`, `SettingsDraft`,
   `DraftStatus`, `SettingsIntent`/`SettingsMessage`, `reduce_settings`.
   RED: A1–A2, B1–B3, E1–E2, F1–F4, G1–G3, H1, I1–I2.
4. **S4 — Projection and renderer** (`src/state/settings_view.rs`,
   `src/ui/screens/settings.rs`): section rows, field rows, theme rows migrated
   from `theme_picker_view`, diagnostic rows, the seven UI states.
   RED: J1–J3.
5. **S5 — Wiring and theme-picker retirement**: `ScreenId::Settings` +
   descriptor + `core.settings` owner, action inventory
   (`core.open-settings` on `,`, `core.open-keys` to `F9`, the `settings`
   context actions), `src/app_input/settings.rs` write boundary, dirty-guard
   integration, deletion of `ModalState::ThemePicker` and its handlers.
   RED: K1, L1.
6. **S6 — Evidence and docs**: tmux scenarios for the distinct UI states;
   `dev-docs/standards/display-and-ui.md` and
   `dev-docs/standards/persistence-and-runtime.md`.

## 4a. Slice progress

| Slice | State | Evidence |
|---|---|---|
| S1 candidate | pending | |
| S2 preview | pending | |
| S3 reducer | pending | |
| S4 view/renderer | pending | |
| S5 wiring/cutover | pending | |
| S6 evidence/docs | pending | |

## 5. Scope ledger

| Entry | Why it is in scope | Acceptance row |
|---|---|---|
| Move `core.open-keys` from `,` to `F9` | The issue's key table assigns `,` to Settings; duplicate chords in one effective context are a composition error, so the move is forced by the accepted behavior | L1 |
| Delete `ModalState::ThemePicker` and its handlers/screen | The issue's source inventory requires migrating the theme choices/keys into the Appearance presenter; leaving the modal keeps a second theme authority whose save path destroys the lossless document | L1, C1–C3 |
| `settings-unavailable-theme.json` etc. new scenarios | CW07-11 requires each distinct UI state to have rendering evidence | J1–J3 |

## 6. Review counters

| Review | Budget | Used |
|---|---|---|
| Local OCR (pre-PR) | 2 | 0 |
| PR OCR | 2 | 0 |
| Subagent review cycles | 2 | 0 |

## 7. Verification evidence

_(filled in as slices land)_
