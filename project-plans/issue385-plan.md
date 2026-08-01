# Issue #385 — CW-05: External custom screens lowered to descriptors and typed relationships

## 1. Outcome

Discover user screen files under the resolved definitions root, parse one closed TOML
syntax with spans and closed-object rules, lower each valid active file exactly once
into the internal `ScreenDescriptor` contract landed by CW-04 (#384), compose lowered
and compiled descriptors transactionally, and propagate typed same-screen port
relationships in bounded, pure reducer transitions.

## 2. Consumed contracts (already in the tree)

| Contract | Location |
|---|---|
| `ResolvedPaths::definitions` | `src/persistence/paths.rs` |
| `ScreenDescriptor` / `PanelDescriptor` / `LayoutNode` / `LayoutChild` / `Size` / `Axis` | `src/workbench/descriptor.rs` |
| `validate_descriptor`, `DescriptorError` | `src/workbench/validate.rs` |
| `ScreenRegistry`, `builtin_screens`, `RegistryError` | `src/workbench/screens.rs` |
| `resolve_layout`, `repair_focus`, `TooSmall`, `PanelState` | `src/workbench/resolve.rs` (standard collapse/focus algorithm) |
| `PanelId` / `PanelTypeId` / `RouteId` / `ScreenId` / `ScreenInstanceId` and the declared limits | `src/workbench/ids.rs` |
| `TypedMap` / `TypedValue` / `Id` / `ByteSpan` | `src/domain/config_contract.rs` |
| `CfgCode::{W004,E005,E006}`, `Diagnostic`, `Severity`, `DiagnosticPath`, `FILE_LIMIT`, `NESTING_LIMIT`, `MAP_LIMIT`, `ARRAY_LIMIT`, `STRING_LIMIT`, `PATH_LIMIT`, `FOLLOW_UP_LIMIT` | `src/persistence/diagnostic.rs` |
| `PublishedSettings` / `PublishedOwner::enabled` / `PublishedWorkbench::enabled_screens` (schema-2 owner activation/dormancy) | `src/persistence/settings_publish.rs` |
| `ActionId`, `ContextId`, `compiled_inventory()` | `src/domain/action_registry.rs`, `src/domain/input_context.rs`, `src/domain/default_action_inventory.rs` |
| Harness v1 scenario format | `src/harness/v1/contract.rs`, fixtures in `dev-docs/tmux-scenarios/v1/` |

## 3. Architecture decisions (recorded, because the issue text allows more than one shape)

**AD-1 — `ScreenId` stays a closed enum.** The runtime "which screen is active"
vocabulary is not widened. Widening it would force a rendering arm in
`ui/orchestration.rs` and navigation arms in `action_context.rs` /
`app_input/list_navigation.rs`, and the issue's non-goals forbid adding a renderer
or navigation stack. Instead the descriptor registry is keyed by a new closed
`ScreenIdentity` enum:

```rust
pub enum ScreenIdentity { Compiled(ScreenId), Custom(CustomScreenId) }
```

`ScreenDescriptor::id` becomes `ScreenIdentity`. `screen_descriptor(ScreenId)` keeps
its signature and looks up `ScreenIdentity::Compiled(id)`, so every existing caller
is unchanged. A lowered custom screen is present in the composed registry and is
resolvable and layout-resolvable, but is not yet routable — routing is CW-07.

**AD-2 — custom identifier text is interned to `'static`.** `PanelId`, `RouteId`,
and `PanelTypeId` are `Copy` newtypes over `&'static str` and the entire resolver,
allocator, and selection layer depend on that. Composition happens once, into a
`'static` registry, so lowered identifier text genuinely lives for the process. A
bounded, deduplicating interner (`src/workbench/intern.rs`) provides `&'static str`
for lowered identifiers. It is bounded by the declared screen/panel/port limits and
is the only place custom text becomes `'static`. Changing the newtypes to owned
strings instead would ripple through `resolve`, `allocate`, `screen_layout`, and
`selection` and is not in this issue's scope.

**AD-3 — modules live in `src/workbench/`, not `src/domain/workbench/`.** The issue's
inventory table predates #384, which landed the workbench at `src/workbench/`. New
pure modules join it there. Filesystem enumeration is a boundary concern and lives in
`src/persistence/screen_files.rs`; `src/workbench/` stays I/O-free.

**AD-4 — `SCR-E301` is a new closed code family in the workbench.** `CfgCode` is a
closed, serialized persistence vocabulary and is not widened. `ScrCode::E301` is
declared in `src/workbench/diagnostics.rs`; accompanying `CFG-W004`/`CFG-E005`/
`CFG-E006` diagnostics reuse `persistence::diagnostic::Diagnostic` unchanged.

## 4. Acceptance matrix

| ID | Actor / launch path | Input & boundary cases | Success behavior | Failure behavior + diagnostic | Side effects before failure | Persistence / compatibility | Proving test |
|---|---|---|---|---|---|---|---|
| CW05-01 | Startup discovery over `ResolvedPaths::definitions` | direct regular `<member>.screen.toml` where member is `[a-z][a-z0-9-]{0,62}`; subdirectory; symlink (to file and to dir); dotfile; `.screen.tml` / `.screen.toml.bak` / `.toml`; non-UTF-8 name; member 63 chars (accept) and 64 chars (reject); empty dir; missing dir; file at `FILE_LIMIT` bytes (accept) and `FILE_LIMIT + 1` (reject) | Only exact direct regular matches are candidates, ordered by canonical path bytes | Oversize file is rejected before parse with `SCR-E301`; unrecognised entries are silently not candidates | none (read-only) | missing directory is not an error | `persistence/screen_files_tests.rs` discovery matrix |
| CW05-02 | `parse_screen_file` + `lower_screen` | a valid `local.review` file | Parsed once, lowered once into `ScreenDescriptor`; `validate_descriptor` passes; no external DTO in the composed registry | — | none | descriptor golden is stable | `workbench/screen_lowering_tests.rs` golden; `custom-screen-enable.json` |
| CW05-03 | Composition with an inactive (dormant) owner whose file is invalid | `enabled = false` in settings for `local.broken` | Registry publishes without that screen; file bytes untouched | `CFG-W004` warning naming the file | none | bytes preserved on disk | `workbench/compose_tests.rs`; `custom-screen-inactive-invalid.json` |
| CW05-04 | Composition with an active owner whose file is invalid | unknown panel type; unresolvable action; unresolvable port ref; layout omits a panel; duplicate screen id across two files; `max < min` | Whole candidate registry is rejected; prior authority retained; no partial publication | `SCR-E301` plus `CFG-E005` (ownership/duplicate) or `CFG-E006` (reference/bound) | none | — | `workbench/compose_tests.rs` invalid matrix |
| CW05-05 | Reducer, immediate master-detail edge | source selection changes | Source and target both updated in one committed transition | — | — | — | `workbench/relationships_tests.rs` |
| CW05-06 | Reducer, explicit master-detail edge | selection changes, then declared activation action fires | Target unchanged until the declared action; staged source applied on activation | — | — | — | `workbench/relationships_tests.rs` |
| CW05-07 | Reducer, source becomes absent | `empty` ∈ {`show-none`,`show-all`,`retain`} for master-detail; {`detach`,`retain`} for session-target; input `retained` true/false | Each closed policy applied exactly: clear / typed all-value / keep prior / detach | — | — | — | `workbench/relationships_tests.rs` deletion policy table |
| CW05-08 | Graph validation and transition bound | cross-screen ref; self edge; 2-cycle; 3-cycle; output→output; input→input; type id mismatch; version mismatch; two incoming controlling edges on one target; duplicate `(source,kind)`; same-kind fan-out; relationships 64 (accept) / 65 (reject); follow-ups 64 (accept) / 65 (abort) | Rejected at validation, or transition aborts with no partial state | `SCR-E301` | none — transition is computed then committed once | — | `workbench/relationships_tests.rs` exhaustive invalid matrix |
| CW05-09 | Issues / Pull Requests screens | list selection moves up/down/page/home/end; repository change; empty list | Detail invalidation, comment cancellation, and scroll reset occur exactly as before, now driven by the declared bundled relationship | — | — | no state schema change | `state/issues_tests*.rs`, `state/prs_tests*.rs` parity (existing suites must stay green) plus `workbench/bundled_relationship_tests.rs` |
| CW05-10 | Layout resolution of a lowered custom screen | terminal too small for declared minima | The standard `resolve_layout` collapse ordering and `repair_focus` apply unchanged — no second geometry engine | `TooSmall` fallback | — | — | `workbench/screen_lowering_tests.rs` tiny-layout test; `custom-screen-tiny.json` |

## 5. Non-goals (explicitly out)

- No renderer for custom screens; `ui/orchestration.rs` gains no arm.
- No navigation stack, route table, or route change; custom screens are not reachable by key.
- No geometry engine; `resolve_layout` is the sole algorithm.
- No provider runtime, plugin loading, or effect execution from a custom file.
- No screen editor, authoring UI, or automatic file rewrite.
- No migration of an earlier custom-screen schema (none exists).
- No widening of `ScreenId`, `CfgCode`, or `TypedValue`.
- No change to settings syntax, keymap, or persisted state schema.

## 6. Vertical slices

| # | Behavior | Layer | New/changed files | RED |
|---|---|---|---|---|
| S1 | Interned identity + `ScreenIdentity` keying, registry unchanged for compiled screens | workbench | `intern.rs`, `ids.rs`, `descriptor.rs`, `screens.rs`, `validate.rs`, `mod.rs` | `intern_tests.rs`, existing workbench suites stay green |
| S2 | Deterministic bounded discovery | persistence boundary | `persistence/screen_files.rs` + tests | discovery matrix |
| S3 | Closed-syntax parser with spans and every bound at-limit/+1 | workbench | `screen_file.rs`, `screen_file_bounds.rs` + tests | parser matrix |
| S4 | Port graph validation + pure bounded propagation | workbench | `relationships.rs`, `relationship_graph.rs` + tests | graph/propagation matrices |
| S5 | `lower_screen` + transactional composition with owner activation | workbench | `screen_lowering.rs`, `compose.rs`, `diagnostics.rs` + tests | lowering golden, compose matrix |
| S6 | Startup wiring | app | `app_init.rs`, `startup.rs`, `main.rs`, `workbench/mod.rs` | harness fixtures |
| S7 | Bundled Issue/PR relationships replace embedded coupling | state | `screens.rs` (ports+relationships), `state/issues_ops.rs`, `state/prs_ops.rs` | parity suites |
| S8 | Documentation | docs | `dev-docs/standards/architecture.md`, `display-and-ui.md` | — |

All slices are complete. Deviations from the planned shape, and why:

- **Modules live in `src/workbench/`**, per AD-3. The issue's inventory names
  `src/domain/workbench/`, which predates #384.
- **The startup boundary is `src/startup_screens.rs`**, not `src/app_init.rs`.
  Composition needs resolved paths and published settings and must run before
  the TUI initializes; `app_init.rs` runs after, inside the binary crate.
- **Fixtures are `custom-screen-enable`, `custom-screen-inactive-invalid`, and
  `custom-screen-active-invalid`.** The issue names `custom-screen-tiny.json`
  for CW05-10, but a custom screen has no renderer in this issue, so a tiny
  terminal cannot show one. CW05-10 is proven by resolving a lowered descriptor
  through the standard `resolve_layout` at a tiny rect, which is the behavior
  the row asks for; the third harness fixture covers the refusal path instead.
- **Layout children carry no span.** Serde buffers the body of an internally
  tagged enum before dispatching on `type`, and a buffered value has lost the
  source positions a span needs. Layout violations name the structure instead.

## 7. Scope ledger

| Change | Justification |
|---|---|
| `src/workbench/ids.rs` — add `CustomScreenId`, `ScreenIdentity`, `VersionedTypeId`, custom-screen limits | acceptance CW05-02/04/08; AD-1 |
| `src/workbench/descriptor.rs` — `id: ScreenIdentity`, `PanelDescriptor::ports` | CW05-02, CW05-08 |
| `src/workbench/screens.rs` — registry keyed by `ScreenIdentity`; Issues/PRs ports + relationships | CW05-04, CW05-09 |
| `src/workbench/validate.rs` — validate ports alongside existing invariants | CW05-04 |
| `src/state/issues_ops.rs`, `src/state/prs_ops.rs` | CW05-09 cutover |
| `src/app_init.rs`, `src/startup.rs`, `src/main.rs` | CW05-02/03/04 startup composition |
| `dev-docs/standards/*.md` | issue "done" requirement |
| `dev-docs/tmux-scenarios/v1/custom-screen-*.json` | CW05-02/03/10 evidence |

No change to `.github/`, `Cargo.toml`, `clippy.toml`, `xtask/`, `.llxprt/`, or any quality gate.

## 8. Review counters

| Review | Cap | Used |
|---|---|---|
| Local OCR | 2 | 1 (one run; produced no parseable output) |
| PR OCR | 2 | 2 (automatic budget exhausted; run 1 failed to parse, run 2 produced 24 findings) |
| Design/code review cycles | 2 | 2 (local Rust review, PR review) |

## 8a. Evidence by acceptance row

| ID | Evidence |
|---|---|
| CW05-01 | `src/persistence/screen_files_tests.rs` (14 tests: type, name, symlink, order, size, UTF-8, repeatability) |
| CW05-02 | `src/workbench/compose_tests.rs` lowering tests; `src/workbench/screen_file_tests.rs`; harness `custom-screen-enable` |
| CW05-03 | `src/workbench/compose_tests.rs` dormant tests; `src/startup_screens_tests.rs`; harness `custom-screen-inactive-invalid` |
| CW05-04 | `src/workbench/compose_tests.rs` refusal matrix; `src/startup_screens_tests.rs`; harness `custom-screen-active-invalid` |
| CW05-05 | `src/workbench/relationship_propagation_tests.rs` immediate tests |
| CW05-06 | `src/workbench/relationship_propagation_tests.rs` explicit/activation tests |
| CW05-07 | `src/workbench/relationship_propagation_tests.rs` retained/empty policy table |
| CW05-08 | `src/workbench/relationships_tests.rs` (18 tests); propagation bound at 64 and 65 |
| CW05-09 | `src/state/screen_relationships_tests.rs`; the existing `issues_*`/`prs_*` suites stay green |
| CW05-10 | `src/workbench/compose_tests.rs::a_tiny_lowered_screen_falls_back_through_the_standard_resolver` |

## 9. Verification

`cargo xtask quick` during iteration; `cargo xtask ci` (fmt, clippy `-D warnings`,
clippy-allow policy, source-file size, architecture policy, complexity, coverage
≥30%, `--locked` build, tests) on the candidate head.

## 10. Review findings and triage

### Local review round 1 (Rust reviewer, plus one OCR run)

| # | Finding | Disposition | Action |
|---|---|---|---|
| H1 | `symlink_metadata` then `File::open` lets a swapped symlink be read | Blocker—Fix | The opened handle is compared against the enumerated entry (device/inode on Unix, type plus mtime elsewhere) and a mismatch is `Replaced`. An exactly named entry whose metadata cannot be read is now kept as an unreadable candidate instead of silently dropped. |
| H1b | Opening a name swapped for a FIFO could block | Reject | Requires an actor with write access to the config directory racing startup; such an actor can already rewrite the configuration outright. The identity re-check is what closes the exploitable half. |
| H2 | Aggregate startup memory unbounded by candidate count | Blocker—Fix | Discovery refuses a directory holding more than `MAX_SCREENS` candidates, before any file is read. |
| M3 | Diagnostics echoed values via `toml::de::Error::message()` | In-scope—Fix | Quoted runs are elided from the parser message. Identifiers (panel, port, type, schema version) are kept: they are structural names an author needs, drawn from a closed grammar. |
| M4 | Interner capacity ignored dormant and failed candidates | In-scope—Fix | Dormant candidates are parsed but never lowered, identifiers are grammar-checked before interning, and panel types resolve to compiled literals rather than interned text. |
| M5 | `serde(default)` widened the grammar | Partly—Fix | `values` became `Option`, so presence and emptiness stay distinguishable and `values = []` is rejected on any field. Defaults on `0..N` collections are Reject: the grammar states those bounds start at zero, and omitting an empty list is how TOML spells that. |
| M6 | Identifier bound not applied to layout leaves or relationship endpoints | In-scope—Fix | Both are checked during shape validation, before lowering. |
| M7 | `PortRef` ambiguous because identifiers may contain `.` | In-scope—Fix | A definition's panel and port identifiers may not contain `.`. |
| M8 | Follow-up bound miscounted and skipped explicit staging | In-scope—Fix | Follow-ups are counted as edge work, excluding the publication that caused the transition and including staging. |
| M9 | Issue/PR cutover diverged for duplicate subjects and stale indices | In-scope—Fix | The trigger is again row movement, exactly as before; the descriptor decides whether the screen couples list to detail and what the detail input receives. |

### PR review round (OpenCodeReview on the PR, 24 findings)

| Finding | Disposition | Action |
|---|---|---|
| `ScreenFileRejection` does not implement `Error` | In-scope—Fix | Implemented, matching `DefinitionsUnreadable` in the same module. |
| `ScreenStartupError` / `LoweringError` never chain a source | In-scope—Fix | Both implement `source()` for the variants that wrap a cause. |
| `compiled_inventory()` rebuilt per binding | In-scope—Fix | The inventory is built once per screen and every binding resolves against it. |
| Config-key failure discards the key | In-scope—Fix | The key is named. It is an identifier from a closed grammar, not a value. |
| Binding failure discards the action/context name | In-scope—Fix | The name is reported for the same reason. |
| Unknown panel type does not list what is available | In-scope—Fix | The refusal enumerates the definable panel types. |
| Zero extent silently coerced to one during lowering | In-scope—Fix | Lowering rejects zero rather than correcting it, so a parser regression cannot become a layout that does not match the file. |
| `ScreenSyntaxReason` `Display` could render empty for a new variant | In-scope—Fix | The second half matches exhaustively, so a new variant is a compile error. |
| `panel_types.rs` double dereference | In-scope—Fix | Uses `copied()`. |
| Test name promises two exit codes, asserts one | In-scope—Fix | Renamed to what it checks. |
| Hardcoded `core.dashboard` as the initial screen | In-scope—Fix | Derived from the compiled registry. |
| Test name/body mismatch on the empty enabled set | In-scope—Fix | Renamed to what it checks. |
| `unreachable!` messages omit the parse error | In-scope—Fix | The error is included. |
| Interner count test not self-contained | In-scope—Fix | Asserts an exact delta of one, then zero. |
| Multi-panel cycle tests use `matches!` | In-scope—Fix | Assert the exact reported panel, matching the self-edge test. |
| "Text fixtures" reads as a typo | In-scope—Fix | Reworded. |
| `drop(resident)` in `intern` is redundant | Reject | `clippy::significant_drop_tightening` requires it; removing it fails the lint gate. |
| `compose_fixtures` is not `#[cfg(test)]` gated | Reject | It is: `src/workbench/mod.rs` declares it under `#[cfg(test)]`. |
| `is_enabled` allocates per candidate | Reject | Composition runs once per process over at most 64 candidates. |
| No test for `InternExhausted` | Reject | Filling a 67,712-entry process-global table would permanently consume the interner for every other test in the binary. The bound is proven by construction and by the discovery limit that feeds it. |
| `PUBLISHED_REGISTRY` fallback is a test-ordering hazard | Reject | A premature read is not silent: publication then fails with `RegistryAlreadyPublished` and startup exits 78. `main.rs` reads the registry nowhere before `publish_screen_registry_or_exit`, and no test calls `compose_and_publish`. |

### Deferred

_(none)_
