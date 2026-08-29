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

1. **Terminal control:** preserve the nine-value public `ControlKind` and route them through the sealed, identity-free host-control factories. Keep PTY terminal rendering on a private capability-gated host path that uses the shared panel layout and renderer; package/local definitions cannot declare it, and no second public control-kind authority exists.
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
- Independent S4 Rust review triage:
  - Fixed: split newly touched hard-limit sources; reject snapshot/body kind mismatch before commit; repair stale List/Diff activation; require an exact enabled retry affordance; submit only current Form fields by merging live drafts over provider values through an enabled submit affordance; validate exact provider events; and keep persistent-session health from consuming legal panel snapshots.
  - Rejected for S4: production Terminal/PTTY wiring, because the approved slice requires only a private capability-gated host boundary and explicitly excludes PTY/geometry integration.
  - Layered authority retained: factories own live control semantics; provider-panel state remains the sole owner of manifest event declarations, process/panel generation, correlation, and revision. `undeclared_action_kind_stages_no_provider_effect` proves fail-closed state rejection.
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

- S4 HostControl RED (2026-08-18): exact public-control/factory tests failed before `ControlKind`, sealed factories, native Tree/StructuredDiff projection, and shared semantic intents existed; integrated Tree/Diff mouse tests initially produced no typed event through the shared boundary.
- S4 HostControl GREEN (2026-08-18): all 12 factory tests pass, including enabled/disabled Retry and Cancel, stale List/Diff selection repair, displayed Form-value submission, capability-restricted Terminal, and exact nine-kind dispatch. Exact binary-target tests pass for Tree expansion, StructuredDiff selection/activation, and undeclared-action atomic rejection. Native Tree/Diff projection and snapshot/body-kind mismatch tests pass. Provider wire and panel-state suites remain green after cohesive source-size splits (88 and 45 tests respectively).
- S4 persistent-health RED/GREEN (2026-08-18): repeated scenario failures proved owner health was calling the pre-session `candidate_health`, consuming a queued legal panel snapshot as `ProtocolFault { evidence: Frame }` before the owner stdout service could deliver it. The focused classifier test failed before a session-specific no-stdout health path existed; it and all four persistent-session tests pass after health was limited to sticky faults and `try_wait`, preserving the generic pre-session illegal-stdout tests.
- S4 persistent-health ordering fix (2026-08-18): review identified that a snapshot queued behind an owner exit could arrive after failure publication and reactivate the dead owner. RED failed because no failed-owner delivery gate existed. GREEN tracks immutable failed owners in the worker and rejects their late snapshots before panel-state mutation; the focused binary test passes.
- S4 boundary review RED/GREEN (2026-08-18): negotiated capabilities now fail closed for panel commands and both idle/active panel snapshots, while action-only providers retain their valid action subset. Provider-authoritative List/Tree/StructuredDiff selection is reconciled through a cloned host-local candidate and rejected atomically with `HostLocalTooLarge` before retained state or model mutation. Focused capability, exact-bound, reducer, and persistent-session suites pass; strict all-target Clippy and the source-size gate pass.
- S4 review triage (2026-08-18): independent review APPROVED with no blockers. In-scope fixes: an over-bound authoritative selection now marks the panel Failed immediately so Retry is available, public HostControl variants retain API documentation, and the factory-test count is exact. Defer: the 998-line input-test file must be split before its next substantive addition. Reject as non-defect: the bounded failed-owner recovery interval matches the no-auto-restart lifecycle; selective staging remains commit hygiene rather than product behavior.
- S4 review RED/GREEN (2026-08-18): the production-path Form regression first submitted stale copied provider data after editing one field and accepting a newer snapshot. `update_panel_host_field` now retains only explicit edits; projection and submission merge that sparse draft over the latest accepted provider values. The focused regression passes.
- S4 review RED/GREEN (2026-08-18): the List/Tree/StructuredDiff regression first activated an optimistic host selection after a provider accepted a different valid selection. Snapshot acceptance now reconciles host-local selector state from the authoritative body, and later activation targets the provider-selected item/node/file. The focused cross-control regression passes.
- S4 package-provider scenario (2026-08-19): after removing every diagnostic and pacing sleep, the authoritative macOS schema-1 scenario passed all 33 steps twice consecutively against exact working-tree `jefe`, `jefe-capture-shim`, and `tmux_scenario` binaries, including provider interaction, declared Help open/close, and Dashboard return with no stale, unavailable, or error state. It proves package navigation, collapsed/expanded native Tree rows, provider-authoritative child selection/activation, native StructuredDiff selection/activation, and clean provider shutdown. The provider validates the exact process/panel generations, revision, and event payload before each snapshot. Because the direct PTY harness deliberately sends adjacent Escape bytes to disambiguate a terminal Escape, the scenario checks the complete ten-record `action-capture.jsonl` before exit: nine dispatch records, exactly one `Esc` / `workbench.back` / `WorkbenchBack` transition, and the second disambiguating Escape recorded as unbound with PTY byte 27 rather than as a second application action. Linux parity carries the identical checked artifact and remains pending Linux CI.
- S4 source-size GREEN (2026-08-18): `cargo xtask check source-size` passes after extracting cohesive Tree/Diff reader functions and the live-event test; all new and touched Rust sources remain below the 1,000-line hard limit.
- S4 fresh-review blocker closure (2026-08-19): focused RED tests first admitted commands after owner failure and retained a queued snapshot after unavailability. GREEN makes `PersistentSessionOwner::send_panel` reject unavailable exact sessions before enqueue, makes `PersistentSessionOwner::drain_panel_deliveries` discard queued unavailable-owner snapshots, and leaves generation-bound `ProviderPanelState::accept_snapshot` as the sole reducer admission boundary. The focused persistent-session, provider-worker, and provider-panel groups pass, strict all-target/all-feature Clippy passes, and `cargo xtask check source-size` passes with `provider_panel_input_tests_core.rs` at 998 lines and `state/types.rs` split to 849 lines. The exact-built macOS Tree/StructuredDiff fixture passed twice with one Back dispatch and one unbound PTY Escape. Dedicated 12-step Linux/macOS provider-health fixtures now prove a provider that exits after Ready remains unavailable across repeated action invocation, with no retry/cancel or automatic restart; the macOS fixture passes locally and the checked manifest requires the Linux fixture in its native CI shard. Final scenario hashes, scenario-owner evidence, and nested issue704 evidence are refreshed sequentially, and the focused evidence validators pass.
- S5 declared-binding RED/GREEN (2026-08-18): candidate publication initially accepted a declared `prs.open` binding that collided with host `Enter`, and an active lowered screen resolved declared `prs.list-browser` / `o` as `Unbound`. The immutable action snapshot now validates exact `(context, action)` requests after effective Settings overrides, rejects protected, unbound, duplicate, declared/declared, host-conflicting, and protected-host-shadowing requests, and resolves only each active descriptor's requested pairs before the unchanged host stack. The focused publication/runtime tests pass, an undeclared same-context `Ctrl+S` action remains unreachable, strict all-target/all-feature Clippy passes, and `cargo xtask quick` reached the immutable-owner evidence gate with 4,773 library and 924 binary tests green; the refreshed owner-evidence test passes.
- S5 declared-binding review (2026-08-18): OCR identified a terminal-capture bypass; RED showed declared `o` dispatched instead of forwarding to the PTY, and GREEN now leaves terminal capture under the registry's existing terminal resolver. In-scope fixes also align declared conflict checks with existing same-action overlap semantics, ignore empty effective entries, and prefer protected-host diagnostics. Reject: publication intentionally validates the fixed lowered-screen host fallback while declared capabilities precede caller stacks, so introducing a new public fallback-stack abstraction is neither required nor safe in this slice. Existing lowering coverage already proves unknown-action refusal; candidate validation still independently fails unknown/context-mismatched typed pairs if it receives them.
- S5 Settings-owner RED/GREEN (2026-08-18): unbinding a declared action in a Settings draft initially passed the Settings candidate even though startup would refuse the same effective keymap. Settings registry validation now reuses the startup screen-binding validator against the candidate-effective immutable action snapshot; the focused regression proves the complete stable `DeclaredUnbound` diagnostic.
- S5 availability and terminal proof (2026-08-18): the active-definition integration test now declares universally executable `(dashboard, dashboard.open-help)`, resolves only that exact pair, leaves undeclared `dashboard.open-errors` unreachable, applies a mutable availability generation as `Resolution::Unavailable`, and preserves terminal `ForwardToPty`. Typed-context compiled handlers such as PR browser remain in the later executable-context slice rather than serving as a misleading S5 fixture.
- S5 production TUI proof (2026-08-18): both schema-1 package scenarios declare `(dashboard, dashboard.open-help)` without screen-ID host routing. The exact macOS production binary dispatches `h` as `dashboard.open-help` / `OpenHelp`, displays `Help - Keyboard Shortcuts`, dispatches `help.close`, returns to the provider screen, and then performs exactly one Back transition. The first run exposed only an inaccurate expected Crossterm rendering (`Char(h)` versus `Char('h')`); after correcting that artifact, the 33-step/6-frame scenario passed. Linux parity carries the same checked artifact for CI.
- S5 independent-review triage (2026-08-18): fixed the Settings-owner gap and strengthened runtime-unavailability plus production dispatch evidence. Provider-surface raw Enter/Esc remains a transient request/confirmation authority rather than ordinary screen fallback: declaration publication rejects collisions with host Enter and protected Esc. Raw Tab is intercepted only while provider confirmation is pending, where overlay focus has intentional precedence. These paths therefore do not create a second declared-binding key authority.
- S5 independent-review blocker fixes (2026-08-18): declarations now run only as exact fallback capabilities on the typed generic screen scope, so terminal capture, shell-overlay ownership, and active modal contexts resolve first. A focused regression proves `Shift+?` closes Help rather than reopening it, an ordinary shell key remains available for PTY forwarding, and `F12` remains shell-owned. Lowering now preserves well-formed typed pairs without consulting the premature compiled-only inventory; final compiled+provider+Settings composition owns unknown-action, context-membership, protection, effective-binding, and collision validation. Provider-action publication/dispatch and post-composition unknown/context-mismatch regressions pass.
- S5 immutable-candidate authority (2026-08-18): startup captures local definition candidates and every manifest-declared package screen file once, retaining either bytes or the typed read refusal. Both initial publication and Settings revalidation call one process-free full candidate composer with authoritative exact package selection, original host/containment, captured screens, resources/ports, providers, actions, exact declarations, and layouts. The Settings reducer performs no filesystem reads, uses no permissive package selector or synthetic containment, and validates only the candidate version's active descriptors. Production-path regressions prove changed invalid v2 descriptors refuse, valid v2 descriptors replace incompatible v1 state, newly introduced screen IDs are validated, captured bytes remain authoritative after disk mutation, missing/ambiguous/unavailable exact versions refuse, parse failures retain stable path/span/detail diagnostics, active-screen unbinds refuse, and disabling the declaring package permits the same compiled-action unbind. A real published package screen navigates through the production reducer, dispatches its declared provider action through `HandlerKey::ProviderAction`, and honors mutable runtime unavailability. Direct typed-lowering regressions reject malformed context and action identifiers. Startup tests were extracted from the production source to preserve the source-size gate.

Focused tests and `cargo xtask quick` run per slice. Before push/PR, run exact-head:
- S1 exact-head GREEN (2026-08-17): `cargo fmt --all --check`, all-target/all-feature clippy with `-D warnings`, locked all-feature workspace build, `cargo xtask quick` (4,716 library tests plus the full quick matrix), and `cargo test --workspace --all-targets --all-features --locked` all passed. The captured full-suite log contained no `FAILED`, `test result: FAILED`, or leading `error:` markers.

- S5 direct-review fixes (2026-08-18): package-screen bytes are now captured by the persistence I/O boundary and workbench composition accepts only immutable captured sources. Settings candidate validation composes screens once from retained exact authorities, retains that candidate registry for independent layout-owner validation, then accumulates, sorts, and deduplicates layout and later full-workbench refusals; the simultaneous `DeclaredUnbound` plus invalid Dashboard layout regression passes. `ContextStack::allows_screen_declarations` is crate-private. Declaration validation/resolution and candidate-diagnostic logic were split into cohesive modules so both newly crossed production sources returned below the 750-line recommendation; strict architecture, Clippy-allow, source-size, and all-target/all-feature Clippy gates pass.
- S5 direct-review triage (2026-08-18): in-scope multi-owner RED returned the resource-schema refusal plus two layout refusals but hid the independent declared-action refusal; GREEN asks each applicable post-screen owner from one retained screen stage and returns all four stable, sorted, deduplicated diagnostics. The shared Dashboard/overlay cutover, typed provider action context with stale semantic-identity rejection, and final CWR2 owner/deletion/scenario evidence are accepted issue-level blockers assigned to planned S6-S9 rather than S5 scope. The 760-line HostControl advisory is deferred until its next substantive growth; the hard source-size gate passes.
- S5 sole-Back RED/GREEN (2026-08-18): shipped Escape handlers previously selected mode-specific mutations without consulting the existing typed precedence resolver. `AppEvent::Back` now enters one shared navigation reducer that resolves exactly one owner across host confirmation, dirty guard, chooser, editor, search, filter controls, overlay, panel transient, and leave. Chord-aware routing preserves multiplexed non-Escape behavior, while provider request/confirmation, terminal capture, and Settings-owned transient editors retain higher input precedence. Generic screen leave commits provider deactivate/resume effects before releasing state access, then schedules persistence and effects outside the state guard; Issue/PR detail refocus retains post-commit list refresh. Focused state and production-dispatch regressions prove one-owner transitions, dirty interception, Repositories one-key exit, Issue/PR refresh selection, and provider effect handoff. Independent review found and the implementation corrected persistence freshness ordering by scheduling the Back snapshot before follow-up adapters that can synchronously persist newer failures. Strict formatting, architecture, source-size, Clippy-allow, all-target/all-feature Clippy, build, library, and binary gates pass; immutable owner evidence was refreshed after the cutover.
- Final regression remediation (2026-08-21): the exact-instance presentation proof initially exceeded the cognitive-complexity gate and was split into named setup/fresh/restored assertions without reducing coverage. Native Tree/StructuredDiff then exposed package Tree Enter being misrouted through Dashboard activation because shared Dashboard Help was mistaken for Dashboard ownership. The preserved failing action capture and corrected 33-step scenario established RED/GREEN; later independent review replaced the temporary composition-root inference with the exact sealed declaration authority recorded below.
- Final native scenario evidence (2026-08-21): the production macOS binary passed Dashboard parity (22 steps), provider health (12 steps), Tree/StructuredDiff (33 steps), and same-definition workbench runtime restoration (23 steps), each with app exit 0. The checked Linux mirrors remain required native Linux CI evidence, and the immutable pair validator proves exact semantic parity after only name/platform normalization.
- Pre-remediation exact-head GREEN (2026-08-21): from `a020ea6edf3f2b71d8cad1f7895850b5b8c96eb9`, immutable issue704 evidence passed 6/6, issue705 evidence passed 15/15, scenario-manifest validation passed 11/11, and the exact same-definition restoration filter passed 1/1. `cargo fmt --all --check`, `git diff --check`, Windows MSVC all-target/all-feature checking, source-size, `cargo xtask quick`, and `cargo xtask ci` passed; CI completed formatting, Clippy-allow, source-size, architecture, multiplexer-surface, strict lint, complexity, 72.74% line coverage, locked build, and locked tests. These results became stale when the independent-review remediations below changed source and evidence.
- Independent whole-diff review remediation: product panel spellings no longer grant host model/input authority. Exact compiled panel leaves carry sealed `HostPanelCapability { model_source, control_kind }`; local/package lowering cannot construct it, and the four Dashboard product leaves are absent from `DEFINABLE_PANEL_TYPES`. Rendering, input, layout, grab, provider binding, and ownership consume the typed capability.
- Independent whole-diff review remediation: full Dashboard action context and footer authority no longer derive from initial-screen ordering or one Dashboard-context binding. Only the compiled Dashboard descriptor carries sealed `DashboardActionContext` and `DashboardFooter` capabilities. Package-first and local imitation regressions preserve only each descriptor's exact declared action pair.
- Independent whole-diff review remediation: generic List, Tree, and StructuredDiff selection projects declaration-owned semantic-key-only typed values through exact resource schemas. Provider event acceptance and stable ordered relationship fan-out commit through one cloned candidate or refuse atomically; real lowered package regressions cover invalid values, two-target order, suspend/restore, stale generation, and exact lifecycle/relationship instance identities.
- Independent whole-diff review remediation: key bindings to provider actions with declared arguments are refused by both startup and Settings candidate publication with a stable diagnostic. The runtime dispatch boundary independently fails closed and a panic-on-dispatch regression proves no provider invocation occurs.
- Independent whole-diff review remediation: confirmation execution consumes the displayed token's immutable owner/action/generation binding, same public IDs replace only the exact immutable binding, and regressions execute queued tokens in presentation order while proving exact cross-instance closure and suspended-owner restoration.
- Independent whole-diff review remediation: mirrored schema-1 semantic-continuation scenarios now prove exact three-resource invocation A, deterministic two-target projection, semantic head change, stale destructive-continuation refusal, absence of invocation B, and clean shutdown on macOS/Linux. Focused runtime artifacts, rather than the scenario, own wrong/extra-resource injection refusal. CWR2-03/07/08 mappings and all nested hashes were refreshed to match those actual responsibilities.
- Final independent-review history (2026-08-28): the initial whole-diff review returned `REQUEST_CHANGES` with seven in-scope findings. The RED regressions and GREEN changes in the preceding six entries close string-derived host authority, Dashboard authority inferred from ordering or one binding, non-semantic generic selection, mutable confirmation ownership, same-ID confirmation replacement across immutable bindings, zero-argument key binding of provider actions that require arguments, and incomplete semantic-continuation evidence.
- Rust 1.98 gate compatibility (2026-08-28): the rolling stable toolchain introduced strict-Clippy diagnostics in existing and changed code. Mechanical fixes used `const fn`, fixed-size `as_chunks`, `unwrap_or`, method references, named grouped parameters, and cohesive helper extraction. No lint setting, exception, suppression, or quality threshold changed. Formatting and strict workspace/all-target/all-feature Clippy pass.
- Final candidate validation (2026-08-28): `git diff --check`, source-size, issue704 evidence 6/6, issue705 evidence 17/17, scenario-manifest evidence 11/11, the locked all-feature build, and Windows MSVC all-target/all-feature checking pass. `cargo xtask ci` exits 0 after its full formatting, policy, lint, complexity, 72.78% line-coverage, build, test, fixture, and doctest sequence. The separate locked workspace/all-target/all-feature test rerun exits 0 with 4,902 library tests and every later target green.
- Validation race record (2026-08-28): two earlier all-target attempts and one quick run reached an unchanged 500 ms process-marker timeout in `driver_timeout_terminates_its_process_group_and_aborts_the_shard`; the test passes alone, in its 11-test suite, during `cargo xtask ci`, and in the final all-target run. A later quick rerun reached a different unchanged 1,500 ms identity-probe timeout in `identity_mismatch_reasons_name_their_phase_and_executable`; that same test passes in the final all-target run. Neither test file changed, and no unrelated timing-infrastructure change was made.
- Final native macOS evidence (2026-08-28): the manifest driver passed Dashboard parity (22 steps), provider health (12 steps), semantic continuation (20 steps), Tree/StructuredDiff (33 steps), and same-definition workbench restoration (23 steps). The first isolated semantic-continuation attempt timed out after 10 observed steps when its generation-2 fixture process stopped after Ready; retained action capture proves the expected F2/Down/Up/F3/Down dispatch sequence, but no artifact exposed an exact provider exit code. No source or scenario changed, and the fresh isolated rerun passed all 20 steps. Both records are retained; the final five reports have exit 0, `passed` status, complete expected steps, and completion evidence.
- Second and final whole-diff review (2026-08-29): `ocr review` completed after 7h35m across 268 files and returned 268 findings (21 high / 108 medium / 139 low) without a bare approval verdict. Because the final cycle retains findings, independent approval is not established by the review alone. All 268 comments were read and classified against source, acceptance evidence, and the issue-705 scope; dispositions are recorded in the ignored triage artifact. Accepted findings were remediated RED-first in six dependency clusters: overlay capacity/hit geometry; confirmation input ownership and typed decision identity; Dashboard focus/navigation authority; provider/host panel projection and input; preflight and auth admission boundaries; and residual inventory, sizing, footer projection, required-panel hiding, encoder round-trip, and mechanical cleanup. Rejected or deferred items (unreachable compiled config legs, warning-only invariant handling, authoring duplication, and time-sensitive or already-covered composites) stayed out of scope without broadening the runtimes.
- Remediated exact candidate final validation (2026-08-29): after every source/test/evidence change, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo build --workspace --all-features --locked`, and `cargo test --locked --workspace --all-features` all exit 0 with 7,580 tests passed and none failed across all binaries. scene and owner gates pass: issue704 owner evidence 6/6, issue705 owner evidence 17/17, and scenario-manifest validation 11/11 after CRLF-normalized artifact-hash refreshes. The five sequential macOS scenarios then passed exactly once against the exact-built binaries: dashboard parity 22 steps, provider health 12, semantic continuation 20, Tree/StructuredDiff 33, and same-definition workbench restoration 23, each with app exit 0, `passed` status, and zero redactions.
- Approval/review-cap state (2026-08-29): the independent-approval/review-cap conflict is unresolved. The final review cycle is exhausted (no third review may run), yet its verdict carries retained findings, so automatic approval is not claimed. Committing requires an explicit user direction resolving that conflict; the branch is never pushed or merged before that direction.


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
