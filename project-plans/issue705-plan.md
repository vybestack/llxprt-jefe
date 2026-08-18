# Issue 705 plan — establish the shared screen/control runtime and migrate Dashboard

Issue: https://github.com/vybestack/llxprt-jefe/issues/705
Parent: #703
Prerequisite: #704, merged by PR #712 at `76e5d714`
Branch: `issue705`, created from that merged predecessor

## Outcome

Built-in, local, and package screen definitions instantiate through one open screen-instance runtime. Production render/input dispatch is selected only by a closed control kind and declared capability. Each screen instance retains its own panel/controller state, typed relationship values, focus, selection, viewport, drafts, activation, and generation.

Dashboard, Help, Search, and generic confirmation migrate completely to that runtime. Neutral local and package fixtures prove equivalent semantics for the nine public controls. Required-provider health may change generation-bound models and availability but cannot replace static declarations or control selection.

At close, the old Dashboard renderer/input/geometry/state tables, separate provider text renderer, stateless relationship adapter, separate migrated overlay authorities, and empty provider-action context path are deleted. The only residual closed built-in entries are exactly Repositories, Issues, PullRequests, Actions, Errors, Terminals, and Settings.

## Current ownership defects

- `ui::orchestration::build_screen_element` selects whole-screen implementations by compiled screen identity; non-built-ins use a separate `ProviderScreen`.
- `DEFINABLE_PANEL_TYPES` and `PanelTypeId` use product panel strings as behavior selectors.
- `provider_panel_view` flattens seven typed model kinds to text; Tree and StructuredDiff do not exist.
- `ScreenInstance` does not own full panel/controller/relationship/focus/viewport/draft state; Dashboard presentation state is global on `AppState`.
- `state::screen_relationships::detail_target_for` recreates empty relationship state per query, keys behavior to compiled `ScreenId`, and transports opaque `Subject(String)` values through one master-detail edge.
- Descriptor bindings lower and validate but do not participate in runtime action resolution.
- `navigation_unwind::resolve_back` exists, but shipped per-mode key chains still own Back precedence.
- Help, Search, and confirmation remain separate renderer/input/focus/navigation heads.
- Generic provider action dispatch constructs empty argument/context maps.
- Dashboard owns bespoke/fallback geometry rather than consuming its definition/control runtime.

## Approved design decisions

1. **Terminal control:** preserve the nine-value public `ControlKind`. Use an internal `HostControlKind::{Public(ControlKind), Terminal}` dispatch key; `Terminal` implements the same sealed host-control boundary but lowering accepts it only for host-owned definitions with the existing PTY capability. Package/local definitions cannot name it.
2. **Host-control shape:** use a sealed trait implemented by a closed set of host-owned factories. A factory receives only validated control declaration, model, retained control state, semantic input intent, and generation context. It receives no screen ID, origin, owner, package/product ID, or whole-screen delegate.
3. **Schema ownership:** place immutable resource/port schema definitions and validation under `workbench`, with pure value identifiers/typed values under `domain`. `PublishedWorkbench` owns the validated registry. Runtime state owns only values already validated against that registry.
4. **Relationship fan-out:** allow one source port to target bounded N distinct target ports. Reject duplicate exact edges, cycles, and multiple incoming writers to one target port. Apply targets in stable declaration order within one committed transition. Keep activation staging only as an internal atomic-transition mechanism, not a second relationship authority.
5. **Open identity compatibility:** add one validated open ID whose canonical string remains the existing persisted screen string. No persistence migration or alternate ID encoding. `ScreenIdentity` remains origin/seed metadata only; compiled-screen matches remain only inside explicit residual-seven adapters.
6. **Overlay grammar:** extend the screen definition/lowering contract now so built-in, local, and package definitions can declare host-owned Help, Search, and Confirmation overlays. Overlay kind is closed and host-owned; packages provide no drawing/input implementation.
7. **Model versions:** keep existing seven model contracts at schema version 1 and introduce Tree/StructuredDiff at version 1. The model-kind addition is additive; unknown kinds/fields/versions still fail fast. Do not reinterpret existing payloads or add a compatibility parser.
8. **Tool scope:** the published catalog contains exact package-relative provider/tool executable descriptors selected during #704 composition. #705 routes declared tool/provider effects through those descriptors and forbids ambient PATH guessing; agent executable detection remains outside this issue.


## Acceptance matrix

| ID | Actor/path and input | Observable success | Failure and permitted side effects | Evidence |
|---|---|---|---|---|
| CWR2-00 | Any built-in/local/package panel initializes or handles semantic input. | Dispatch is solely closed control kind to host-owned factory. | Unknown/forbidden kinds reject candidate publication; no fallback or whole-screen delegate executes. | Interface/call-path tests and seeded prohibited-signature/table mutations. |
| CWR2-01 | Equivalent declarations use list, tree, detail, structured-diff, form, status, progress, empty, or error across three origins. | Focus, projection, input, selection, and viewport semantics are identical. | Invalid declarations fail before runtime; runtime provider failure changes model status only. | Cross-origin properties/goldens and Linux/macOS TUI scenarios. |
| CWR2-01A | Neutral provider emits Tree/StructuredDiff snapshots and events. | Exact versioned DTO validates and renders through the same factories. | Unknown fields/kinds/versions, invalid topology/ranges/order, and exceeded bounds reject without partial model mutation. | Wire goldens, boundary/mutation fixtures, provider integration, TUI equivalence. |
| CWR2-02 | A validated screen instance activates. | All panel/controller/relationship/focus/selection/viewport/draft/activation/generation state is allocated under open screen and panel instance IDs. | Allocation/validation failure leaves no active partial instance. | Lifecycle and identity tests. |
| CWR2-03 | A source selection emits a schema-valid typed port value with N declared targets. | All targets update in stable order in one bounded committed transition. | Cycle, duplicate edge, multiple writer, type/version/owner/key mismatch rejects the whole transition. | Fan-out/order/cycle/deletion/type/version property tests. |
| CWR2-04 | Two instances of one definition diverge, suspend, and restore. | Each restores its own selection, viewport, typed values, draft, focus, and generation. | Stale generation cannot mutate either current instance. | Two-instance state test and TUI suspend/restore scenario. |
| CWR2-05 | A declared screen binding is pressed. | Published binding resolution, availability, and dispatch execute without screen-ID host code. | Protected/conflicting/unknown bindings reject candidate publication. | Conflict/static validation and runtime dispatch integration. |
| CWR2-06 | Back is pressed with overlay/local/dirty/route/global layers. | Sole navigation reducer applies declared precedence and one transition. | Dirty interception blocks navigation without losing state; no per-mode fallback chain executes. | Back precedence matrix, call-path guard, TUI overlay/route scenarios. |
| CWR2-07 | Declared resource context invokes a provider action. | Request carries only the closed schema-validated typed snapshot. | Unknown/extra/wrong owner/type/version/key values reject before invocation; no provider side effect. | Request golden, rejection fixtures, fail-if-invoked provider. |
| CWR2-08 | Resource semantic identity changes before destructive continuation. | Continuation revalidates current key/head and rejects stale intent. | Provider is not invoked; current state remains unchanged and diagnostic identifies staleness. | Stale semantic-key/head provider fixture. |
| CWR2-09 | Dashboard/local/package screens render and receive input. | Shared runtime executes; residual closed set is exactly seven. | No old Dashboard/provider/relationship/overlay dispatcher or geometry fallback is reachable. | Dashboard parity, neutral fixtures, call traces, strict residual/deletion/mutation guards. |
| CWR2-10 | Provider health fails after publication. | Static declarations and aggregate identity remain; model health becomes generation-bound unavailable/error; factory identity is unchanged. | Stale events are ignored; no declaration removal, fallback renderer, or automatic restart. | Crash/disable/recovery identity test and TUI scenario. |
| CWR2-11 | Exact-head evidence is checked. | Every criterion maps to immutable fixtures, symbols, commands, platforms, residual entries, and deleted authorities. | Missing/duplicate/stale/hash-mismatched evidence fails. | `issue705-owner-evidence.json` test with cross-platform canonical hashes. |

## RED → GREEN bounded vertical slices

Each slice crosses no more than three architecture ownership layers. UI-visible slices add/update the schema-1 TUI scenario first and record the expected RED result before production implementation.

### S1 — closed model vocabulary and exact Tree/Diff wire contracts (CWR2-01A)

Layers: domain/manifest → runtime/provider → state/workbench consumers.

RED unknown-field, kind, version, topology, range, ordering, and bound fixtures. Add Tree and StructuredDiff to `ModelKind::ALL`; introduce exact versioned DTOs and events; update manifest model-kinds, snapshot/event readers, bounds, and validation together. No UI rendering in this slice.

Bounds are inclusive: 1,000 Tree nodes; 256 StructuredDiff files; 1,024 hunks per file; 1,024 lines per hunk; and 262,144 UTF-8 content bytes per diff line. Provider framing retains its existing 1,048,576-byte line ceiling.

### S2 — typed resources and retained N-target relationships (CWR2-03)

Layers: domain/workbench → state.

RED typed-value and fan-out properties. Add typed port values and immutable schema validation; re-key relationship state by open screen/panel instance/port; permit bounded one-to-many source propagation; atomically reject bad transitions. Delete opaque `Subject(String)`, `master_detail_edge`, and per-query empty relationship state only when all consumers move.

### S3 — open IDs and complete screen-instance ownership (CWR2-02, CWR2-04)

Layers: workbench IDs/descriptors → state/navigation → persistence restore.

RED activation and two-instance restore behavior. Allocate complete state per instance, preserve canonical persisted IDs, and move Dashboard presentation state into instance ownership without altering residual-seven behavior.

### S4 — one host-control boundary and native projections (CWR2-00, CWR2-01, CWR2-01A)

Layers: state → UI → input intents.

RED cross-origin equivalence and factory call-path tests. Implement the sealed host-control contract and closed factories for all nine public kinds plus capability-restricted internal terminal. Route provider snapshots/events through the same projection/reducer boundary; remove text flattening for migrated controls.

### S5 — declared bindings and sole Back/navigation reducer (CWR2-05, CWR2-06)

Layers: workbench/action registry → state/navigation → app input.

RED binding conflicts/dispatch and Back matrix. Consume descriptor bindings in published action resolution; convert migrated Back/Esc routes to the sole reducer and remove Dashboard/local/package screen-ID decisions from shared host code.

### S6 — declared Help/Search/Confirmation layers (CWR2-06, CWR2-09)

Layers: workbench grammar/lowering → state/navigation → UI/input.

RED overlay declaration, focus trap, dirty interception, and Back scenarios. Lower closed host-owned overlays into screen instances and route rendering/input through host controls. Delete separate migrated modal authority only after parity passes.

### S7 — typed provider/tool action context and stale continuation rejection (CWR2-07, CWR2-08)

Layers: state context projection → app input → provider/tool catalog boundary.

RED request goldens and fail-if-invoked stale/invalid provider fixtures. Project exact typed resource snapshots, validate against the committed registry, revalidate semantic identity before destructive continuation, and resolve executables only from the published catalog.

### S8A — Dashboard definition/state/render cutover (CWR2-01, CWR2-02, CWR2-04, CWR2-09)

Layers: workbench definitions → state → UI.

RED Dashboard normal/focused/search/help/confirmation/empty/unavailable/error/shell/tiny/resize/mouse scenarios. Lower Dashboard panels, relationships, resources, overlays, and host-only terminal capability. Move all presentation state and projection to the instance/control runtime.

### S8B — Dashboard input/navigation/effect cutover (CWR2-05..09)

Layers: state semantic intents → app input → effect boundaries.

Route keyboard/mouse/focus/selection/search/grab/shell/action behavior through generic controls and navigation. Remove Dashboard branches from shared context/footer/help/hidden-panel dispatch after parity is green.

### S9 — neutral fixtures, deletions, residual ledger, docs, and evidence (CWR2-00..11)

Layers: tests/harness → production deletions → documentation/evidence.

Run neutral local/package definitions through all nine controls and typed relationships/actions. Delete superseded Dashboard/provider/relationship/overlay authorities. Assert the exact residual seven. Publish normative docs, scenario manifest entries, and immutable owner/deletion evidence.

## Expected path ledger

- Control/model contracts: `src/domain/plugin/surface.rs`, `src/runtime/provider/panel_model.rs`, provider reader/DTO/bounds modules, focused tests.
- Open IDs/descriptors/lowering: `src/workbench/{ids,descriptor,screen_file,screen_lowering,screens,panel_types}.rs` and focused tests/goldens.
- Resource/relationship contracts: focused `src/domain/**`, `src/workbench/{relationships,relationship_propagation,screens_ports}.rs`, `src/state/screen_relationships.rs` and tests.
- Instance/navigation: `src/state/{navigation,navigation_ops,navigation_layers,navigation_unwind,types}.rs`, focused state modules/tests.
- Host controls/UI: new focused host-control owner plus existing native controls; `src/provider_panel_view.rs`, `src/ui/components/provider_screen.rs`, `src/ui/orchestration.rs`.
- Dashboard cutover: `src/ui/screens/dashboard.rs`, Dashboard state/input/mouse/action projection owners, then deletion or residual-free reduction.
- Bindings/context/effects: `src/action_context.rs`, `src/domain/action_registry/**`, `src/app_input/**`, `src/app_shell_key_routing.rs`, provider request/effect owners.
- TUI/evidence/docs: `dev-docs/tmux-scenarios/issue705/**`, checked scenario manifests, `dev-docs/testing/issue705-owner-evidence.json`, normative architecture/runtime docs, `tests/issue705*`.

No dependency, `.github`, `.llxprt`, `.code_puppy`, quality-gate, lint threshold, or unrelated test change is planned.

## Exact deletion and residual ledger

Delete rather than wrap for migrated surfaces:

- Dashboard arm/whole-screen delegate in `build_screen_element` and bespoke Dashboard top-level renderer/input authority;
- Dashboard bespoke/fallback geometry and global presentation state;
- Dashboard product panel strings as renderer/input selectors;
- text-row provider projection/drawing for all nine shared controls;
- `detail_target_for`, per-query empty `RelationshipState`, `master_detail_edge`, and opaque `Subject(String)` transport;
- empty generic provider-action argument/resource construction;
- Help/Search/confirmation as separate renderer/input/focus/navigation authorities;
- Dashboard/local/package identity/origin branches in generic context, footer, help, hidden-panel, mouse, render, input, and navigation code.

Retain exactly these closed built-in adapters for later issues, with no fallback role:

- Repositories
- Issues
- PullRequests
- Actions
- Errors
- Terminals
- Settings

The panel-type registry itself remains only for those residual screens and is deleted by #708.

## Non-goals

- Migrating any of the residual seven built-ins.
- Git-specific product behavior or the #392 reference package.
- #706 sole-layout-generation and full PTY geometry cutover beyond Dashboard's migration needs.
- Arbitrary extension widgets, package-owned drawing/raw input/PTY, or provider protocol redesign.
- Hot reload, automatic provider restart, fallback rendering, compatibility facades, or dual authority.
- Agent executable discovery changes, persistence format migration, new dependencies, workflow/quality-tool changes, or optional hardening after done criteria pass.

## Hard stops

Stop for user direction if work requires a process/cancellation subsystem, an abstraction outside the approved design above, a provider protocol or persistence-version change, residual-seven behavior migration, visible shared-chrome change for residual screens, PTY manager changes beyond Dashboard geometry re-pointing, dependency/workflow/quality-tool change, or mainline integration exceeding the canonical drift bounds.

## Scope ledger

| Item | Status | Mapping |
|---|---|---|
| Nine public controls and exact Tree/Diff DTOs | Accepted | CWR2-00, CWR2-01, CWR2-01A |
| Internal capability-restricted terminal control | Accepted | Issue target; CWR2-00, CWR2-09 |
| Open ID and complete screen-instance state | Accepted | CWR2-02, CWR2-04 |
| Typed schema registry/value validation | Accepted | CWR2-03, CWR2-07, CWR2-08 |
| Retained bounded N-target relationships | Accepted | CWR2-03 |
| Binding and navigation reducer consumption | Accepted | CWR2-05, CWR2-06 |
| Declarative host-owned overlay grammar | Accepted | CWR2-06, CWR2-09 |
| Published provider/tool descriptor routing | Accepted | CWR2-07, CWR2-08 |
| Dashboard complete migration/deletion | Accepted | CWR2-09 |
| Provider health identity stability | Accepted | CWR2-10 |
| Schema-1 TUI and immutable evidence | Accepted | CWR2-01..11 |
| Unrelated local artifacts | Excluded | Pre-existing untracked workspace content |

## Review counters

- Open Code Review before PR: 1/2.
  - Local review 1 triage: fixed strict empty-hunk rejection, exact over-bound error assertions, populated no-legacy-fallback fixtures, and body-specific duplicate-ID diagnostics. The two weak-bound comments described the same fix. Rejected the informational event-list comment and the claimed redaction-coverage gap because the exhaustive supervisor redaction test already covers every new authored field. No findings deferred.
- Open Code Review after PR: 0/2.
- Planning/code analysis cycles: 2 (codebase planning analysis plus grounded HostControl cutover analysis; findings incorporated into the bounded slices).

## Verification ledger

- S1 RED (2026-08-17): `cargo test --locked --all-features --lib domain::plugin::surface::tests::model_and_event_kinds_use_the_exact_closed_wire_vocabulary` failed at compile time because the production model still exposed seven kinds and lacked `Tree`, `StructuredDiff`, and `ExpansionChanged`; the included provider contract tests also failed on the absent body/event variants.

- S2 relationship RED (2026-08-17): `cargo test --locked --all-features --lib may_` failed only the two new fan-out properties: the existing validator returned `DuplicateOutgoing` for one source port driving distinct targets and `SameKindFanOut` for distinct ports on one source panel.
- S2 typed-value RED (2026-08-17): the exact four-field domain test failed to compile because `domain::TypedPortValue` did not exist; the focused test is now green.
- S2 resource-schema RED (2026-08-17): the schema contract failed to compile because `workbench::resource_schemas` did not exist; strict type/version/owner/semantic-key/payload validation and duplicate/zero-version publication tests are now green.
- S2 instance-runtime RED (2026-08-17): typed source, atomic invalid-source, and two-instance isolation tests failed against the old opaque three-argument relationship API; all 20 focused propagation tests now pass with registry validation and `(OpenScreenId, PanelInstanceId, PortId)` state keys.
- S2 published-registry RED (2026-08-17): `the_published_workbench_owns_the_builtin_resource_schema_registry` failed to compile because `PublishedWorkbench::resource_schemas` did not exist; it is green after startup candidate publication and immutable ownership wiring.
- S2 focused GREEN (2026-08-17): 19 relationship graph tests, 3 resource-schema tests, 20 propagation tests, 68 provider-panel lifecycle/event tests, the published-registry test, and all 4,715 library tests pass.
- S1 supplemental RED (2026-08-17): `tree_nodes_and_diff_files_are_valid_selection_targets` returned zero effects for shared Tree/Diff selection, and `structured_diff_rejects_empty_hunks` accepted a zero-line hunk. Both focused tests are green after the minimal validator fixes.
- S2 slice GREEN (2026-08-17): `cargo xtask quick`, `cargo fmt --all --check`, and all-target/all-feature clippy with `-D warnings` passed after refreshing the affected immutable owner-evidence hashes.
- S1 focused GREEN (2026-08-17): exact Tree, StructuredDiff, ExpansionChanged, populated no-legacy-fallback, shared selection-target, and exhaustive provider-body redaction tests pass. `cargo xtask quick`, formatting, all-target/all-feature clippy, and a locked all-feature workspace build passed before review; exact-head gates are rerun after review fixes below.

- S2 review-fix RED (2026-08-17): focused resource and propagation tests failed to compile against the permissive schema/config reuse, unchecked source endpoint, infallible panel mapping, and screen-wide owner API. The resulting focused suites are green after exact resource-field semantics, checked semantic-key lookup, output-port validation, non-aliasing panel maps, and per-port immutable owners.
- S2 definition-schema RED (2026-08-17): closed `[[resources]]` parsing/lowering/composition tests initially failed because no definition-owned resource grammar or publication path existed. Schema 2 now carries bounded exact resources and explicit port owners; schema 1 remains accepted through a closed GitHub issue/PR owner migration and cannot claim schema-2 resources.
- S2 production-runtime RED (2026-08-17): real `ScreenInstance` values had no relationship runtime or retained state. Navigation now allocates independent checked panel identities, retains relationship state through suspend/restore, disposes replaced instances, validates against the published registry, and synchronizes issue/PR selections. A fresh-instance test first observed `Absent` and is green after push/replace initialization without restore overwrite.
- S2 review-fix GREEN (2026-08-17): 307 workbench tests and 1,434 state tests pass; strict library clippy passes. Immutable issue704 owner evidence was refreshed for the changed startup/state owners and its focused verification passes.

- S2 independent-review fixes (2026-08-17): tracker-qualified issue/PR semantic keys prevent repository-local number collisions; all seven production list-clear routes publish absence; schema-1 explicit owners must exactly match the closed historical mapping; navigation preflight is allocation-free and dirty Discard commits one prepared instance; direct startup-candidate tests prove enabled definition publication and atomic unknown-type/wrong-version/wrong-owner/duplicate refusal with configuration exit classification.
- S2 navigation allocation proof GREEN (2026-08-17): all 31 existing dirty-navigation tests pass, and the added thread-local allocation observation proves guard/cancel allocate no panel identities while Discard allocates exactly the entered screen's declared panel count. Strict library clippy passes.
- S2 restoration/identity GREEN (2026-08-17): durable-root restoration binds the current open-screen relationship runtime before commit; checked screen/panel allocators reject exhaustion; navigation preflight and guard/cancel consume no identities; focused restoration, allocator, and dirty-navigation tests pass.
- S2 authenticated-command RED/GREEN (2026-08-17): a producer-correlation test failed to compile before `RelationshipCommand`; the runtime now rejects stale screen IDs, stale generations, mismatched panel instances, and wrong owners before mutation while retaining pure `SourceIntent` inside propagation.
- S2 current gate GREEN (2026-08-17): `cargo fmt --all --check`, strict workspace/all-target/all-feature clippy, and `cargo xtask quick` pass with 4,752 library tests and the complete quick matrix after sequential owner-evidence refresh.


Focused tests and `cargo xtask quick` run per slice. Before push/PR, run exact-head:
- S1 exact-head GREEN (2026-08-17): `cargo fmt --all --check`, all-target/all-feature clippy with `-D warnings`, locked all-feature workspace build, `cargo xtask quick` (4,716 library tests plus the full quick matrix), and `cargo test --workspace --all-targets --all-features --locked` all passed. The captured full-suite log contained no `FAILED`, `test result: FAILED`, or leading `error:` markers.


```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo xtask ci
```

Run each new Linux/macOS schema-1 scenario through the sole `tmux_scenario` authority and checked execution manifest. Cross-platform contract/evidence hashes canonicalize checkout line endings. Executable/path/PTY changes receive Unix structural arguments, Windows resolver/wrapper, remote escaping, and native Windows CI evidence at the first affected slice.

## Deferred findings

None.
