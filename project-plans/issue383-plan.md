# Issue 383 delivery plan — CW-03 action registry and single-chord keymaps

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/383
- Branch: `issue383`
- Base: `origin/main` at `7b12edc`
- Status: **implementation approved; RED/GREEN slices in progress**
- Behavioral authorities: issue body, maintainer no-shim amendment, current source for existing behavior, and the canonical bounded workflow.
- User approval: on 2026-07-29, the user authorized proceeding after being told that D1–D13 and the one-PR hard-budget exception required explicit approval. The recommended D1–D13 decisions below are accepted.
- Review counters: local OCR 0/2; post-PR OCR 0/2; independent review cycles 0/2.

## Baseline finding

CW-03 cannot fit the normal or hard pull-request budget as currently accepted.
The required change spans domain values and resolution, schema-2 persistence,
startup and CLI, root and per-mode input orchestration, typed messages/state,
Help/footer/menu/Keys projections, a new Keys editor, mouse routing, strict
schema-1 harness capture, scenario conversion, tests, and five normative docs.
The maintainer's binding no-shim amendment also requires old harness scenarios to
be converted and the superseded parser deleted at feature-complete.

Expected scope is approximately 105–135 files and 11,000–18,000 changed lines
when that no-shim conversion is included. This exceeds the 25-file/1,500-line
target and the 40-file/2,500-line hard stop. Implementation therefore requires
explicit user approval for either one oversized issue PR or explicitly approved
stacked PRs. The project default is one issue per PR, so this plan recommends one
bounded PR with independently GREEN commits if the hard-budget exception is
approved.

Implementation is additionally blocked by contradictions between the ticket's
"source-derived" inventory and the current production route. Those decisions
are listed below. No production source or RED fixture may be written until they
are resolved, because the generated golden must be authoritative rather than a
guess.

## Current architectural authority

- Pure registry values and resolution belong in `src/domain/`; domain must not
  import state, persistence, UI, runtime, or harness.
- Raw input currently resolves through `src/app_shell.rs` and
  `src/app_shell_key_routing.rs` before `src/input.rs` and the per-mode
  `src/app_input/` resolvers.
- Typed behavior remains `AppEvent -> AppMessage -> AppState::apply_message`
  through `src/messages/event_conversion.rs`; side effects stay in app-input,
  persistence, and runtime boundaries.
- Schema-2 settings already publish raw keymaps through
  `src/persistence/settings_document.rs` and
  `src/persistence/settings_publish.rs`. CW-03 extends that authority rather
  than the older theme-only `persistence::Settings` serializer.
- Closed effects and five-field correlations already exist in
  `src/domain/effects.rs`; availability must reuse that contract, not create a
  parallel generation mechanism.
- Help and footer are currently separate static authorities in
  `src/ui/modals/help.rs` and `src/ui/components/keybind_bar.rs`.
- Current mouse routing supports selection, drag/copy, wheel, and PTY gestures,
  but no application click target emits an action ID.
- The strict schema-1 harness and an older harness coexist. Feature-complete
  no-shim delivery must leave one current-format parser and converted scenarios.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundaries | Observable success | Failure / diagnostic and permitted side effects | Persistence / compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- |
| CW03-01 | TUI keyboard user in every current context | Every generated `(context, chord, action, handler, availability, Help row, footer row)` and every production `KeyCode` path | With no override, each frozen row resolves and executes exactly once through one registry result with source-equivalent typed output or boundary effect | Completeness test fails on a missing dispatch or orphan row; validation failure performs no handler/effect | No settings write; existing frames and PTY bytes remain unchanged | `current-action-default-parity.json`, `every_current_default`, bidirectional generated golden, existing input regressions |
| CW03-02 | Pure resolver | Canonical chord; terminal special case; modal, editor/chooser, focused panel, screen, global contexts | Exactly one `Resolution`; explicit child binding shadows its parent | Same-context collision or implicit shadow rejects the whole candidate with `KEY-E401`; unresolved input is `Unbound`; zero handler/effect on failure | Prior snapshot remains authoritative | `contextual-keymap-override.json`, `context_resolution` across all six levels |
| CW03-03 | Dispatch, Help, footer, menu, and Keys editor | Available/unavailable action and exact reason; matching or stale closed-effect completion | All consumers project byte-identical status/reason from one immutable snapshot; unavailable dispatch emits notice only | Stale correlation is ignored; unavailable action performs zero handler/provider/runtime effect | Runtime snapshot changes only after authoritative completion; settings unchanged | `keymap-projection-consistency.json`, `availability_projection`, stale-completion tests |
| CW03-04 | Startup composition and Keys save validation | Same-context duplicate, implicit child shadow, canonical alias, duplicate chord, protected unbind/shadow | Valid candidate publishes atomically | Entire candidate rejected with `KEY-E401`; prior bytes/snapshot retained; Save disabled; zero write/publish/handler/effect | Lossless source bytes remain unchanged on rejection | `keymap-conflict-startup.json`, `conflict_validator`, byte-retention fixtures |
| CW03-05 | Keys editor user | Unbind `[]`, Reset removing override syntax, comments/order/dormant syntax, restart | Revision-gated atomic save reproduces exact effective bindings after restart | Invalid/conflicting candidate cannot write; write conflict/failure retains old bytes/snapshot and surfaces typed diagnostic | Unknown owner bytes, comments, and ordering remain lossless; no legacy whole-document rewrite | `keymap-unbind-reset.json`, `keymap_merge_roundtrip`, normal/focused/error/dirty/recovery/small-terminal scenarios |
| CW03-06 | `jefe explain binding` CLI user | Valid, invalid, unresolved, usage, explicit context, malformed settings, explicit config path | Prints normalized chord, searched contexts, winner, availability/reason, shadows, provenance; exit 0 when resolved | Exit 2 invalid/unresolved; 64 usage; no TUI/provider/probe/runtime/write | Reads current settings only; remains usable offline | `binding-explain-flow.json`, `explain_cli` output/exit goldens |
| CW03-07 | Terminal-capture user on macOS and Linux | Platform events for emergency exit and leave capture; protected override attempts | Protected recovery remains reachable under both platform normalizations | Invalid protected candidate rejected before publication; zero PTY write and zero handler | Previous bytes/snapshot retained; defaults recoverable | macOS/Linux protected scenarios and exact original/canonical/resolution captures |
| CW03-08 | Terminal-capture user and child PTY | Ordinary keys including Ctrl-C; scrollback-routing keys; modified keys; shell-overlay precedence | Ordinary input returns `ForwardToPty` and writes byte-identical encoding; current scroll interception remains exact | Existing typed PTY failure only; registry never turns forwarded input into an action | No persistence | `terminal-passthrough.json`, `terminal_input_parity`, exact original event/canonical class/PTY bytes |
| CW03-09 | Mouse user on each approved action-bearing hit surface | Left down/drag/up, zero-length click, non-empty selection drag, PTY reporting, wheel, modal/tiny geometry | Valid click emits assigned `ActionId` and executes the same availability/handler path as keyboard | No hit is a no-op; unavailable hit shows shared reason with zero handler; drag/copy and PTY gestures remain unchanged | No persistence except the clicked action's accepted behavior | `mouse-action-consistency.json`, `mouse_action_capture` frame/cell/hit/action tuples |
| CW03-10 | Parser, composer, and Keys editor | Exact 8/9 chords, 2048/2049 effective bindings, ID bounds, F1–F24, Unicode scalar, label cells, description bytes | Boundary values accepted | Owning path rejects over-limit complete candidate with typed diagnostic and zero write/publish/handler/effect | Previous bytes/snapshot retained | `keymap-resource-bounds.json`, `keymap_bounds` exact fixtures |

## Explicit non-goals

- No multi-chord sequences, sequence timeout, prefix/fallback semantics, or
  `PendingSequence`; the closed grammar rejects sequences.
- No `jefe settings keys list|set|reset`; only `jefe explain binding` and the
  required Keys editor are in scope.
- No new dependency, manifest change, workflow/quality-tool change, lint or
  complexity suppression/threshold change, unsafe, unwrap/expect workaround,
  or `.llxprt/` change.
- No dynamic plugin actions, closures in the registry, generic payload bus,
  arbitrary shell commands, or user-defined commands.
- No new process-management, timeout, cancellation, cleanup, geometry-registry,
  screen-descriptor, or availability-discovery subsystem without explicit
  approval.
- No use of the legacy theme `Settings` serializer as keymap authority and no
  rewrite of dormant/unknown schema-2 bytes.
- No unrelated runtime, state, test, or documentation refactor.
- No `ScreenMode` or `AgentKind` deletion in CW-03 unless separately approved;
  the no-shim amendment says those are deleted once their replacement
  descriptors land, and CW-03 does not define that descriptor subsystem.
- No optional hardening after all accepted behavior and required gates pass.

## Decision register — approved 2026-07-29

| ID | Decision | Source conflict | Accepted decision |
| --- | --- | --- | --- |
| D1 | Delivery shape and hard budget | Required no-shim scope is about 105–135 files / 11,000–18,000 changed lines | Approve one complete oversized PR with bounded GREEN commits, matching the repository's one-issue/one-PR default |
| D2 | Ticket rows versus actual defaults | Ticket says Dashboard `d/Delete`, kill `k`, restart `r`, Actions `a`, Split `q Back`; source uses Ctrl-D, Ctrl-K, Ctrl-R, `g`, and Split Esc while `q` feeds rapid-quit | Treat current source as authority and correct the generated inventory; do not add ticket-only aliases |
| D3 | Truly global help and terminal toggle | Ticket says `F1/?/h` and `F12/t` are global; current wrappers consume some, Help closes only Esc/?, and terminal capture forwards `t` | Preserve current behavior for parity except where protected recovery explicitly requires a new binding; enumerate any intentional new aliases separately |
| D4 | Modifier matching | Many current `KeyCode` arms ignore modifiers; exact `Chord` equality cannot represent that and omits crossterm META/HYPER | Define default inventory from intended exact chords and classify incidental modifier-insensitive matches as non-contract behavior, rather than adding an unplanned wildcard abstraction |
| D5 | Issues/PR inline and filter behavior | Ticket says Ctrl-C cancel/clear; Issues filter Ctrl-C exits Issues, shared clear is Ctrl-L, and PR inline behavior differs | Freeze current source behavior by context; do not normalize as an undocumented behavior change |
| D6 | Keys editor entry and ownership | No inventory row defines how Keys opens; example shows comma/Open Settings; modal/screen and dirty-close behavior are unspecified | Add global `,` -> `core.open-keys`; use a modal owned by schema-2 settings; Esc on dirty opens Save/Discard/Cancel, Esc there means Cancel |
| D7 | Availability predicate and reason golden | Shared reasons are required but no complete predicate/reason list exists | Freeze the existing capability checks and current user-facing reasons as the initial golden; unavailable actions stay visible |
| D8 | Malformed startup keymap | Current startup fails closed; issue requires safe defaults/prior snapshot and offline explain | On initial startup, retain malformed bytes, emit a typed diagnostic, and run compiled defaults; runtime edits retain prior snapshot |
| D9 | Harness observation channel | Strict harness is a separate process and has no canonical-resolution or mouse-hit observation channel | Approve one private, harness-only artifact protocol activated only by the contained schema-1 runner; no public runtime API or alternate input path |
| D10 | Old harness conversion/deletion | Binding no-shim amendment requires converted fixtures/scenarios and old parser deletion | Include complete conversion/deletion in this issue; this is part of D1's oversized scope approval |
| D11 | Mouse hit surfaces and click semantics | Current app has no action click targets, but CW03-09 requires captures | Limit action clicks to action-bearing controls introduced/owned by the registry (Keys rows/buttons and existing rendered modal buttons); zero-length release activates, non-empty drag remains selection |
| D12 | Protected Back and tiny layout | Current Back is context-specific and tiny-layout reachability is not enumerated | Protect the local-unwind Back action in every non-terminal modal/editor/chooser/screen context; tiny layout must always render and route one Back hit/chord plus Ctrl-Q |
| D13 | Additional public types | Snapshot and named closed types are approved; trace/capture/screen descriptors are not | Keep resolution trace, projections, persistence patches, and harness records private or `pub(crate)`; stop before any additional public abstraction |

The issue body and maintainer comment settle the no-shim rule. The user accepted
the recommended D1–D13 decisions and the oversized one-PR delivery shape on
2026-07-29, making the acceptance matrix decision-complete.

## Source-derived baseline inventory notes

The generated golden must expand every grouped row to concrete tuples and must
audit all production routes, not only the issue table.

### Outer routing and terminal

- Visible shell overlay precedes every other route: F12 hides, F10 closes, and
  other input forwards to PTY.
- Pre-mode handling currently includes F12 only on Dashboard/Split/Actions,
  Alt/Option+1..9, Dashboard F10 shell, and Dashboard F8 external terminal.
- Plain Dashboard attached terminal forwards exact Ctrl-C even when terminal
  capture is not focused.
- Terminal capture intercepts unmodified PageUp/PageDown/Home and conditional
  End/Up/Down scrollback controls; modified and ordinary keys forward raw.
- Exact Ctrl-Q quits in quit-eligible contexts; bare q/Q three times in the
  existing one-second window quits, with intervening input resetting the count.

### Modal and text contexts

- Help currently closes only with Esc or `?`; arrows, PageUp/PageDown/Home/End
  scroll it.
- Search, Dashboard search, confirm, forms, auth, theme picker, and shared filter
  controls each have distinct precedence and modifier behavior that must become
  explicit context rows.
- Shared filter clear-all is Ctrl-L. Issues filter has a preceding Ctrl-C exit
  route.

### Dashboard, Split, Errors, and Terminal Manager

- Normal navigation includes arrows, j/k, paging, Home/End, Left/Right, and Tab.
- Dashboard lifecycle uses Ctrl-D, Ctrl-K, Ctrl-R, and l/L; mode entry uses i/p/g/e/s
  and F7; help uses ?/h/H/F1; direct pane focus uses r/a/t; Enter is focus-dependent.
- Dashboard reorder uses Space, Up/Down, and Enter.
- Split exits with Esc and enters grab with g/G; current earlier normal tiers
  remain reachable.
- Errors and Terminal Manager have their own mode resolvers and must be in the
  golden even though the issue inventory omits them.

### Issues, pull requests, and Actions

- Issues precedence is property editor, close chooser, delete confirm, inline,
  agent chooser, search, filter, then list/detail normal handling.
- Pull Requests precedence is inline, agent chooser, merge chooser, property
  editor, search, filter, then list/detail/changes handling.
- Inline Unicode insertion and editing are raw editor operations, not actions;
  submit/cancel/rewrite controls require exact context rows.
- Actions precedence is search, filter, then focus-dependent workflow/run/detail
  resolution. Current static Help/footer text contains drift and is evidence of
  duplicate authority, not source dispatch authority.

## Planned production modules

| Path | Owner / purpose |
| --- | --- |
| `src/domain/keymap.rs` | Closed IDs, `Chord`, modifiers, key grammar, parse/format, constructor and resource bounds |
| `src/domain/action_registry.rs` | `Action`, `Binding`, `Availability`, `Resolution`, closed `HandlerKey`, immutable snapshot, composition, protected/conflict validation, pure resolution |
| `src/domain/input_context.rs` | Context IDs, parent declarations, ordered context stack; no state imports |
| `src/domain/default_action_inventory.rs` | Compiled source-derived actions/bindings and golden projection |
| `src/action_context.rs` | Pure selector from current app state/focus/modal/terminal state to domain contexts |
| `src/action_projection.rs` | Iocraft-free Help/footer/menu/Keys projection from one snapshot |
| `src/binding_explain.rs` | Provider-free explain service and private output model |
| `src/persistence/keymap_edit.rs` | Lossless schema-2 set/unbind/reset candidate patch and validation |
| `src/app_input/action_handlers.rs` | Closed handler-key execution into smallest typed message or existing boundary operation |
| `src/keys_view.rs` | Pure Keys-editor projection including focused/error/dirty/small layouts |
| `src/ui/modals/keys.rs` | Thin iocraft Keys editor |
| `src/harness/v1/action_capture.rs` | Private strict-harness original/canonical/resolution/mouse records, conditional on D9 |

Expected existing contract sets include `src/domain/mod.rs`, `src/lib.rs`,
`src/input.rs`, `src/app_shell.rs`, `src/app_shell_key_routing.rs`, the relevant
`src/app_input/` resolvers, typed message/state files, schema-2 persistence and
startup/CLI files, Help/footer/root UI files, selection geometry and mouse
routing, strict harness parser/runner/report files, integration test targets,
converted scenario files, and the five normative docs named by the issue.
Every changed file must be added to the scope ledger before editing.

## Dependency flow

```text
raw platform event
  -> input translation -> domain::Chord
  -> action_context(AppState snapshot -> ContextStack)
  -> domain::ActionRegistrySnapshot::resolve
  -> app_input::action_handlers
  -> smallest AppEvent/AppMessage -> deterministic reducer

SettingsDocument + owner catalog
  -> raw whole-list overrides
  -> pure candidate composition/validation
  -> immutable snapshot publication only after complete success

snapshot + context
  -> pure action_projection
  -> Help/footer/menu/Keys thin renderers

layout hit target -> ActionId -> same availability/handler path
```

## Bounded vertical commit slices in one PR

These are proposed internal GREEN commits if D1 is approved. They are not
separate PRs unless the user explicitly chooses stacked delivery.

| Slice | Rows | Owner / boundary | RED evidence | GREEN criterion | Stop conditions |
| --- | --- | --- | --- | --- | --- |
| S0 source inventory and chord translation | CW03-01, 07, 08 | pure domain values plus platform translation | generated completeness, grammar, platform canonicalization, PTY-byte fixtures | authoritative compiled inventory and canonical single-chord parser; runtime route not switched | D2–D5 unresolved; modifier behavior needs new abstraction |
| S1 composition, contexts, conflict/protection/bounds | CW03-02, 04, 10 | pure domain registry | six-level resolution and exact duplicate/implicit/protected/8-9/2048-2049 fixtures | immutable validated candidate and deterministic resolver | additional public trace/error abstraction required |
| S2 schema-2 overrides and explain | CW03-04, 05, 06, 10 | persistence candidate boundary and provider-free CLI | lossless unbind/reset/restart, no-write rejection, explain exit/output | schema-2 composition and offline explain use same snapshot | D6/D8 unresolved or schema rewrite required |
| S3 outer/Dashboard dispatch | CW03-01, 02, 07, 08 | root shell and typed handler executor | source-equivalent outputs and PTY bytes through production route | one resolver owns outer, terminal, quit, Dashboard, Split dispatch | old map must survive as fallback |
| S4 Issues/PR/Actions/special/modal dispatch | CW03-01, 02 | per-mode typed handlers | context-by-context current output fixtures | every mode consumes one resolution result; raw editor/PTY input remains raw | context absent from accepted golden |
| S5 availability and shared projections | CW03-03 | closed-effect completion plus pure views | byte-identical reason across five consumers and stale completion | one snapshot projects dispatch, Help, footer, menu, editor | D7 unresolved or new discovery subsystem needed |
| S6 Keys editor and lossless save | CW03-03, 04, 05 | typed reducer, pure Keys view, thin modal, persistence writer | required UI scenarios first, including dirty/error/recovery/small | edit/unbind/reset/save/restart operate atomically | D6/D12 unresolved or new screen subsystem needed |
| S7 mouse action identity | CW03-03, 09 | selection geometry and existing mouse boundary | click-vs-drag, unavailable, PTY, approved surfaces | hit target emits ActionId and uses keyboard handler path | D11 unresolved or new geometry subsystem needed |
| S8 strict harness capture and no-shim conversion | all | contained strict schema-1 runner and private capture protocol | parser/report/capture fixtures plus all issue scenarios | original/canonical/resolution/mouse/PTY evidence; all old scenarios converted; old parser deleted | D9/D10 unapproved or public runtime API required |
| S9 normative docs and final authority deletion | all | docs and architecture convergence | stale-contract scan | five docs match code; duplicate maps/parsers deleted; golden/source bidirectional gate clean | any compatibility fallback remains |

Each slice starts with the smallest failing behavioral test and, for visible UI,
a failing strict schema-1 TUI scenario. Focused tests and `cargo xtask quick` run
during iteration. Every GREEN checkpoint runs unchanged `make ci-check`. Main is
fetched before every slice; contract-file drift or more than five new main
commits pauses integration review.

## Scope ledger

| Discovery | Disposition |
| --- | --- |
| Closed action/chord/registry types and immutable snapshot | Accepted by issue, pending behavioral decisions |
| Full source-derived inventory including omitted Errors, Terminal Manager, shell, modal, and raw-input routes | Required for CW03-01; no invented rows |
| Existing schema-2 keymap parser/publisher and closed-effect correlation | Reuse; parallel authority prohibited |
| Keys editor | Required but blocked on D6 and D12 |
| Private app-to-harness capture artifact | Approval required under D9; public protocol prohibited without separate approval |
| Old harness parser/scenario conversion and deletion | Required by maintainer no-shim amendment; hard-budget approval required under D1/D10 |
| `ScreenMode`/`AgentKind` deletion | Deferred; replacement descriptors do not land in this issue |
| Mouse actions outside approved current/Keys/modal surfaces | Reject unless separately approved |
| Multi-chord sequences and settings-management CLI | Reject as outside the closed issue contract |
| Dependency/workflow/quality/lint/suppression/unsafe/unwrap/.llxprt changes | Prohibited |
| Unrelated cleanup, refactors, tests, or docs | Defer to follow-up |

No source implementation, tests, or scenarios have been started before S0. The
only prior working-tree change was this required issue plan.

## S0 execution ledger — source inventory and chord translation

S0 delivers CW03-01, CW03-07, and CW03-08, limited to pure domain values,
canonical single-chord parsing/formatting/platform translation, source-derived
default inventory/golden groundwork, and focused tests/fixtures. Production
runtime dispatch is **not** switched in S0 (per D2–D5 current source is the
behavior authority; the existing `input.rs`/`app_input`/`pty_encoding` routes
are untouched). No schema-1/2 change, no new dependency, no `.llxprt` change,
no lint/quality threshold change, no `unsafe`, no production `unwrap`/`expect`.

### S0 acceptance scope

- CW03-01 (S0 portion): closed `Action`/`Binding`/`Chord` value types and a
  source-derived frozen compiled inventory with a golden projection record
  tuple. Dispatch execution parity is deferred to S3/S4; S0 proves the
  inventory and value types are complete and self-consistent.
- CW03-07: canonical `crossterm::event::KeyEvent` -> `Chord` translation that
  preserves uppercase scalar and explicit Shift provenance; META/HYPER are
  unsupported and fail as typed errors (never silently accepted).
- CW03-08: terminal ordinary-input PTY-byte classification groundwork — a pure
  function classifying whether a canonical chord is `ForwardToPty` plus the
  encoded bytes, preserving the existing `pty_encoding::key_to_bytes` behavior.
  Runtime forwarding is unchanged in S0.

### S0 changed files (recorded before edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `src/domain/keymap.rs` | `Modifier`/`ModifierSet`, `Key`, `Chord`, parse/format, grammar rejection, resource bounds, `Chord::from_crossterm` canonical translation, PTY-byte classification | pure domain (new) |
| `src/domain/action_registry.rs` | `ActionId`, `HandlerKey`, `Provenance`, `Availability`, `Binding`, `Action` closed value types required by S0 inventory typing (no snapshot/resolver/composition yet) | pure domain (new) |
| `src/domain/input_context.rs` | `ContextId` newtype + ordered context stack/parent declarations required for inventory typing | pure domain (new) |
| `src/domain/default_action_inventory.rs` | Source-derived frozen compiled bindings and golden projection tuple groundwork | pure domain (new) |
| `src/domain/mod.rs` | Register the four new submodules and re-export the S0 surface | pure domain |
| `src/domain/keymap_tests.rs` | Grammar, formatting, bounds, crossterm translation, and PTY-byte tests (RED first) | focused test fixtures |
| `src/domain/action_registry_tests.rs` | `ActionId`/`HandlerKey`/`Binding` value-type tests (RED first) | focused test fixtures |
| `src/domain/input_context_tests.rs` | `ContextId` grammar and context-stack ordering tests (RED first) | focused test fixtures |
| `src/domain/default_action_inventory_tests.rs` | Inventory completeness, golden tuple, and protected-binding groundwork tests (RED first) | focused test fixtures |
| `src/state/actions_tests_sort.rs` | Repair pre-existing stale `Repository::new` test fixture so the branch's library tests compile and S0 GREEN evidence can run | required verification prerequisite |
| `src/state/prs_test_fixtures.rs` | Split a pre-existing 61-line test fixture so the required all-target Clippy gate can run under the current toolchain | required verification prerequisite |

### S0 RED evidence

- RED goal: `src/domain/keymap.rs`, `src/domain/action_registry.rs`,
  `src/domain/input_context.rs`, and `src/domain/default_action_inventory.rs`
  are absent, so the focused test modules fail to compile. The smallest
  failing behavioral/unit tests are written first and proven RED before any
  production value type exists.
- RED proof command and output are recorded in the S0 verification section
  below before the GREEN implementation.

### S0 non-goals (slice-local)

- No immutable snapshot, resolver, conflict/protection validation, or
  composition (S1). `default_action_inventory` exposes only the frozen compiled
  bindings and a golden projection tuple; it does not publish a runtime
  snapshot.
- No switch of production dispatch/input/app_input/harness/persistence/UI.
- No `pub use` of a `Resolution` enum, no trace/capture/screen descriptors
  (private or otherwise), no `dyn Any`, no closures, no generic payload.

### S0 verification record

**RED proof** (before production implementation). With the four production
modules absent but the test modules wired in `src/domain/mod.rs`, focused test
compilation fails for the intended reason:

```
$ cargo test --lib domain::keymap_tests
error[E0583]: file not found for module `keymap`
error[E0583]: file not found for module `action_registry`
error[E0583]: file not found for module `input_context`
error[E0583]: file not found for module `default_action_inventory`
```

**GREEN implementation and verification status** (2026-07-29):

- The approved stale `Repository::new` fixture prerequisite is repaired in
  `src/state/actions_tests_sort.rs`, so library tests compile.
- `prs_test_fixtures.rs::prs_state_with_detail` was split into the focused
  `detail_list_item` and `loaded_detail` builders so each function is <=60
  lines; test behavior is unchanged.
- The four new S0 test modules use small `let Ok(value) = ... else { panic!() };`
  / `let Some(value) = ... else { panic!() };` helpers and
  `unwrap_or_else(|err| panic!(...))`; no `unwrap`/`expect`/`expect_err` and no
  clone-on-copy remain in the new test code.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  (0 warnings; the prior `expect_used`, `clone_on_copy`, and
  `too_many_lines` findings are resolved without lint allows/suppressions or
  threshold changes).
- `cargo build --workspace --all-features --locked` — PASS.
- `cargo test --workspace --all-features --locked` — library 2803 passed / 1
  ignored / 0 failed; domain:: 392 passed / 0 failed; PR-mode state (consuming
  the split fixture) 235 passed / 0 failed. Five `tests/harness_v1_fixtures.rs`
  TUI-subprocess capture tests are non-deterministic `HAR-E005` empty-frame
  timeouts in this environment (that file is unmodified and out of S0 scope);
  they pass on a re-run of `--tests`.
- `scripts/check-architecture.sh` — PASS.
- `git diff --check` — PASS.

S0's compiled inventory was audited against the current keyboard authorities
listed in `AUDITED_DISPATCH_SOURCES`. The inventory intentionally excludes raw
editor character mutation and the rapid `qqq` sequence because neither is a
single-chord action. Runtime routing remains unchanged for S0.

## S1 execution ledger — composition, contexts, conflicts, protection, and bounds

S1 delivers CW03-02, CW03-04, and CW03-10 at the pure-domain boundary only.
Publication remains owned by the future application/startup boundary; S1 returns
an immutable snapshot only after complete validation and has no persistence,
runtime, state, UI, CLI, or harness side effects.

### S1 changed files (recorded before edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record S1 scope, RED/GREEN evidence, and exact verification results | delivery evidence |
| `src/domain/mod.rs` | Register the focused S1 composition test module and update S1 module ownership text | pure-domain module contract |
| `src/domain/action_registry_composition_tests.rs` | RED-first composition, resolution, conflict, protection, availability, and exact-boundary fixtures | focused pure-domain tests (new) |
| `src/domain/input_context_tests.rs` | RED-first validated standard/terminal context-stack behavior | focused pure-domain tests |
| `src/domain/default_action_inventory_tests.rs` | RED-first local-unwind protection inventory assertion | focused pure-domain tests |
| `src/domain/action_registry.rs` | Typed whole-list overrides, complete candidate validation, immutable snapshot, diagnostics, availability generation, and deterministic resolution | pure domain |
| `src/domain/input_context.rs` | Validated ordered standard and terminal-capture context stacks/parent declarations | pure domain |
| `src/domain/default_action_inventory.rs` | Mark accepted context-local unwind actions protected per D12 | pure domain |

### S1 RED evidence

Before any S1 production edit, `cargo test --lib domain::` failed with exit 101
for the intended reason: unresolved S1 contracts `ContextStack`,
`ContextStackError`, `BindingOverride`, `RegistryCandidate`,
`ActionRegistrySnapshot`, `Resolution`, `ActionAvailability`,
`AvailabilityGeneration`, and typed registry diagnostics. The existing S0
production modules had no composition or resolution API. An earlier malformed
multi-filter Cargo invocation exited 1 at argument parsing and is not counted as
RED evidence.

### S1 design boundaries

- A `RegistryCandidate` owns the complete validated-inventory inputs, typed
  whole-list overrides, declared context stacks, and one exact-correlated
  availability generation. `ActionRegistrySnapshot` is produced atomically;
  publication or prior-snapshot retention remains outside domain.
- Parent/child relationships are the adjacent entries in validated ordered
  `ContextStack` declarations. A child override is an explicit shadow. A parent
  override that newly collides with an unchanged child is implicit and invalid.
  Emergency/leave and protected-vs-unprotected collisions are invalid; nested
  protected local-unwind rows may share `Esc` because child-first resolution
  keeps the current local Back reachable while retaining its protected parent.
- `Shift+Tab` and `BackTab` are compared as one platform-normalized chord for
  validation and lookup without adding parser aliases or wildcard matching.
- Availability is immutable snapshot data stamped with the existing five-field
  `effects::Correlation`; S1 adds no effect family, queue, generation counter,
  handler execution, or publication mechanism.


### S1 GREEN and verification evidence

- Focused GREEN: `cargo test --lib domain::action_registry_composition_tests
  --no-fail-fast` — 10 passed; `domain::input_context_tests` — 10 passed;
  `domain::default_action_inventory_tests` — 7 passed.
- Complete pure-domain regression: `cargo test --lib domain:: --no-fail-fast`
  — 404 passed, 0 failed.
- `cargo fmt --all --check` — PASS.
- `cargo xtask check source-size` — PASS; `action_registry.rs` is exactly 1,000
  lines (warning threshold exceeded, hard limit respected).
- `cargo xtask check clippy-allows` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS.
- `cargo build --workspace --all-features --locked` — PASS.
- `cargo test --workspace --all-features --locked -q` — PASS: library 2,869
  passed / 1 ignored; bin 808 passed; every integration/doctest target passed.
- `scripts/check-architecture.sh` — PASS.
- `cargo xtask quick` — PASS on retry. The first run hit the known unmodified
  `tests/harness_v1_fixtures.rs` frame-width capture race (`themes` was below
  the captured viewport); the unchanged test passed on immediate full retry.
- `git diff --check` — PASS. No `.llxprt`, dependency, workflow, state,
  persistence, runtime, CLI, UI, or harness file changed; no commit or push.

## S2 execution ledger — schema-2 keymap overrides and binding explain

S2 delivers CW03-04, CW03-05, CW03-06, and CW03-10 at the lossless
schema-2 persistence boundary and provider-free CLI boundary. Runtime input
dispatch, UI/Keys editor, mouse routing, harness conversion, dependencies,
workflows, quality configuration, and later slices remain unchanged.

### S2 changed files (recorded before edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record S2 scope, RED/GREEN evidence, CLI examples, and exact verification | delivery evidence |
| `src/persistence/keymap_edit.rs` | Pure lossless set/unbind/reset candidate patching and complete registry composition | persistence/domain boundary (new) |
| `src/persistence/keymap_edit_tests.rs` | RED-first lossless roundtrip, rejection, bounds, and prior-authority fixtures | focused persistence tests (new) |
| `src/persistence/mod.rs` | Register keymap edit ownership/tests and expose the existing atomic revision-gated settings-byte write seam | persistence boundary |
| `src/persistence/settings_publish.rs` | Publish schema-2 `keymap.<context>.<action>` whole-list strings independently of config-owner catalog | lossless settings publication |
| `src/recovery_effective.rs` | Keep provider-free effective-settings rendering compatible with context/action string keymaps after the authoritative publication type change | provider-free recovery projection |
| `src/persistence/settings_document_tests.rs` | RED-first context/action publication and dormant-byte retention coverage | focused settings tests |
| `src/domain/action_registry.rs` | Retain validated effective binding/provenance data in the approved snapshot for private explain projection | pure domain snapshot |
| `src/domain/default_action_inventory.rs` | Closed source-derived complete context-stack declarations used by composition and explain | pure inventory/domain owner |
| `src/domain/default_action_inventory_tests.rs` | Closed context-stack declaration coverage and ordering assertions | focused pure-domain tests |
| `src/startup.rs` | Compose one startup keymap snapshot; retain malformed bytes, report KEY-E401, and use compiled defaults per D8 | startup boundary |
| `src/binding_explain.rs` | Provider-free read/compose/resolve service and private output projection | CLI service (new) |
| `src/binding_explain_tests.rs` | RED-first output, invalid/unresolved, malformed-settings, and no-write service tests | focused CLI-service tests (new) |
| `src/cli.rs` | Hand-rolled `explain binding CHORD [--context ID]` command parsing | CLI parsing |
| `tests/cli.rs` | RED-first usage/parser and explicit-config command coverage | CLI parser integration tests |
| `src/lib.rs` | Register the approved binding-explain service module | crate registration |
| `src/main.rs` | Dispatch explain before startup/TUI/provider/probe/runtime initialization and render its typed outcome | process entry boundary |
| `tests/binding_explain_cli.rs` | Real-process output/exit/config/offline/provider-free evidence | process integration tests (new) |

### S2 RED evidence

The smallest behavioral tests were added before S2 production behavior.
`cargo test --lib persistence::keymap_edit_tests --no-fail-fast` exited 101
with `E0583 file not found for module keymap_edit`, proving the intended missing
lossless candidate-edit contract. An earlier two-filter Cargo invocation exited
1 in Cargo argument parsing and is not counted as RED evidence. CLI RED is
recorded separately before its production implementation.
`cargo test --test cli explain_binding --no-fail-fast && cargo test --lib
binding_explain_tests --no-fail-fast` exited 101 with `E0583 file not found for
module binding_explain`, proving the intended missing provider-free service
before parser/service production edits. A later focused alias-shadow test also
failed with exit 101 because explain used exact `Chord::contains` while the
registry resolves `Shift+Tab` and `BackTab` canonically; production shadow
projection was then changed to query the same snapshot resolver.

### S2 GREEN evidence

- Lossless candidate behavior: `cargo test --lib
  persistence::keymap_edit_tests --no-fail-fast` — 7 passed. Whole-list set,
  explicit `[]` unbind, reset-to-inherit, comment/order/dormant-byte retention,
  snapshot-retained effective provenance, complete nested conflict/protection,
  malformed-keymap non-keymap retention, fatal malformed syntax, typed bounds
  rejection, and stale revision/expected-hash retention are covered.
- Explain service: `cargo test --lib binding_explain_tests --no-fail-fast` — 6
  passed. Output includes normalized chord, the complete source-derived nested
  search order, winner, resolution, availability/reason, shadows, and snapshot
  provenance; malformed keymaps report `KEY-E401` while resolving compiled
  defaults without writes, while malformed TOML remains fatal.
- CLI parser: `cargo test --test cli explain_binding --no-fail-fast` — 2 passed;
  valid context/config ordering and exit-64 usage failures are covered.
- Startup D8: `cargo test --lib
  malformed_initial_keymap_retains_bytes_and_uses_compiled_defaults
  --no-fail-fast` — 1 passed.
- Schema-2 publication: `cargo test --lib
  keymap_publishes_context_action_lists_without_config_owner_catalog
  --no-fail-fast` — 1 passed, including empty lists and dormant extension bytes.
- Real process: `cargo test --test binding_explain_cli --no-fail-fast` — 2
  passed with an empty `PATH`, unchanged settings bytes, no `state.json`, and
  exit codes 0/2/64.

A real resolved invocation printed `normalized chord: x`, searched
`dashboard -> global`, dispatched `dashboard.navigate-down` through
`NavigateDown`, and reported `settings:<selected settings.toml>` provenance.
The settings SHA-256 was identical before/after and no state file was created.
A malformed `Ctrl+` override printed `KEY-E401: unknown key in chord grammar` to
stderr, exited 0 because compiled `j` still resolved, reported `provenance:
compiled`, retained settings bytes, and created no state file.

### S2 verification record

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  after preserving the crate-private module boundary without lint suppression.
- `cargo build --workspace --all-features --locked` — PASS.
- `cargo test --workspace --all-features --locked` — PASS: library 2,886
  passed / 1 ignored; bin 808 passed; every integration target passed.
- `cargo test --doc --workspace --all-features --locked` — PASS: 2 doctests.
- `scripts/check-architecture.sh` and `cargo test architecture -- --nocapture`
  — PASS.
- `cargo xtask check source-size` and `cargo test source_size -- --nocapture`
  — PASS; `action_registry.rs` is 995 lines and no S2 file exceeds the hard
  limit (existing warning-only files remain).
- `cargo xtask check clippy-allows` and `cargo test clippy_allow_policy --
  --nocapture` — PASS.
- `cargo xtask quick` — PASS. Bare `cargo xtask check` is not an aggregate gate
  in this repository and correctly prints usage requiring one policy name; all
  three supported policy checks were run explicitly.
- `git diff --check` — PASS.

No schema version, dependency, runtime input dispatch, UI/Keys editor, mouse,
harness, workflow, quality configuration, or `.llxprt` file changed. No commit
or push was performed.

### S2 design boundaries

- `SettingsDocument` original bytes remain the formatting authority. Candidate
  edits patch only the selected assignment statement/value (or insert/remove
  that one owned assignment), then parse, publish, parse every chord, and
  compose the complete candidate before any write or publication.
- Empty chord arrays are explicit unbinds; absent assignments inherit compiled
  defaults. Rejection is one typed `KEY-E401` result and leaves the prior bytes
  and snapshot untouched.
- Startup and explain call the same persistence-owned composition function over
  the source-derived compiled inventory. Explain uses snapshot resolution and
  snapshot-retained provenance; it does not contain a binding table or a second
  resolver.
- No schema bump is required: schema 2 already owns `keymap`; S2 corrects that
  subtree from config-owner semantics to the approved context/action semantics.

## Verification and review contract

Per slice:

1. Record the intended RED failure before production behavior.
2. Run focused tests and the relevant strict schema-1 TUI scenario.
3. Run `cargo xtask quick` after GREEN/refactor.
4. Run unchanged `make ci-check` before each pushed GREEN checkpoint.
5. Preserve Unix/macOS behavior and add Windows structural coverage for platform
   event translation where the contract applies.

Before finalizing/pushing, run RustReviewer and detached Open Code Review with
`--timeout 20` on a stable verified checkpoint. Use no more than two review
cycles total and no more than two local OCR runs. Classify every finding as
Blocker-Fix, In-scope-Fix, Reject, or Defer; review output never authorizes scope
expansion.

Final exact-head readiness requires all ten criteria and converted scenarios,
source/golden bidirectional completeness, unchanged PTY/frame behavior, the
no-shim scan, unchanged `make ci-check`, required native CI, clean ancestry and
conflict status, completed review triage, and a clean scope ledger.

## Stop conditions

D1–D13 and the planned hard-budget exception were approved on 2026-07-29.
Stop before any unplanned subsystem/public abstraction, further unapproved
hard-budget expansion, wrong ancestry, incomplete verification, or behavior
outside CW03-01..10. Stop successfully when accepted behavior and all exact-head
gates are complete; do not continue optional hardening.

## S3 execution ledger — outer shell, terminal capture, and Dashboard-family dispatch

S3 delivers the runtime-dispatch portions of CW03-01, CW03-02, CW03-07, and
CW03-08 for shell-overlay precedence, terminal capture, global/pre-mode
shortcuts, Dashboard, Split, Errors, and Terminal Manager. Issues, Pull
Requests, Actions mode-specific routing, modal/editor routing, projections,
Keys UI, mouse routing, and harness conversion remain later-slice owners.

### S3 changed-file ledger (recorded before edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record S3 scope, RED/GREEN evidence, exact gates, and discovered inventory parity rows | delivery evidence |
| `src/action_context.rs` | Pure source-state selector for shell, terminal, S3 full-dispatch, and pre-mode-only context stacks | application selector (new, crate-private) |
| `src/action_context_tests.rs` | RED-first source-state/context precedence and focused-context fixtures | focused selector tests (new) |
| `src/input.rs` | Pure runtime `KeyEvent` to canonical `Chord` translation, including existing macOS Option-symbol normalization | input translation |
| `src/input_tests.rs` | RED-first Alt/Option canonicalization and exact modifier fixtures | focused input tests |
| `src/main.rs` | Move the one startup-composed immutable snapshot into root `AppContext` and register the selector | startup/root ownership |
| `src/app_shell.rs` | Replace migrated outer/pre-mode/terminal/S3 hardcoded routing with one registry-route invocation while retaining later-slice mode/editor routing | root input orchestration |
| `src/app_shell_key_routing.rs` | Resolve one canonical chord against one source-derived stack exactly once and execute the closed typed result | root dispatch boundary |
| `src/app_shell_key_routing_tests.rs` | RED-first Dashboard/Split/Errors/Terminal Manager/shell/terminal resolution parity | focused production-route tests (new) |
| `src/app_input/mod.rs` | Register/re-export the S3 handler executor and remove superseded global/terminal dispatch exports | app-input boundary |
| `src/app_input/action_handlers.rs` | Closed exhaustive `HandlerKey` executor into the smallest `AppEvent` or typed existing boundary operation | typed dispatch (new) |
| `src/app_input/action_handlers_tests.rs` | RED-first dynamic Dashboard/Errors/terminal-scroll/Terminal Manager handler output fixtures | focused executor tests (new) |
| `src/app_input/normal.rs` | Remove migrated Dashboard/Split/Errors/global maps; retain rapid-`qqq`, raw Dashboard search, and later-slice Issues/PR/Actions delegation | legacy-slice boundary |
| `src/app_input/dashboard_search.rs` | Remove migrated special/grab routing while retaining raw search and later-slice mode delegation | raw editor/shared plumbing |
| `src/app_input/list_navigation.rs` | Expose the existing pure Dashboard/Split page-capacity calculation to the typed executor without duplicating geometry | shared pure calculation |
| `src/app_input/errors.rs` | Delete the superseded hardcoded Errors key dispatch map after registry parity | migrated duplicate authority |
| `src/app_input/terminal_manager.rs` | Replace the key map with typed close/focus boundary functions; retain runtime orchestration | runtime boundary |
| `src/app_input/shell_overlay.rs` | Replace F-key dispatch maps with typed hide/close/open boundary functions | runtime boundary |
| `src/app_input/modal_handlers.rs` | Expose the existing theme-picker boundary without retaining an F9 key map | shared S3 boundary |
| `src/app_input/split_mode_key_tests.rs` | Move Split assertions from deleted hardcoded resolver to registry/executor behavior | focused parity tests |
| `src/app_input/prs_key_tests.rs` | Move the existing Dashboard `p` entry assertion off the deleted Dashboard resolver while preserving PR-mode dispatch ownership | focused later-slice regression test |
| `src/app_input/prs_integration_tests.rs` | Route the existing Dashboard-to-PR integration checkpoint through the registry result instead of the deleted Dashboard resolver | focused later-slice integration test |
| `src/app_input/pty_passthrough_tests.rs` | Construct root context with the single immutable snapshot and retain exact Ctrl-C contention behavior | PTY regression tests |
| `src/domain/default_action_inventory.rs` | Add audited missing Split/Errors/pre-mode rows and register an internal S3 spec split without crossing the source-size hard limit | pure inventory |
| `src/domain/default_action_inventory_s3.rs` | Hold only the newly audited S3 parity rows because the 906-line inventory owner cannot safely grow past 1,000 lines | pure inventory split (new, internal) |
| `src/domain/default_action_inventory_tests.rs` | RED-first assertions for every audited missing S3 row | focused inventory tests |
| `src/binding_explain_tests.rs` | Extend the existing all-context BackTab override fixture for the newly audited Split and Errors cycle-pane rows | existing S2 composition regression fixture |

`src/domain/action_registry.rs` remains unchanged at 995 lines: the audit found
that the existing closed handlers are sufficient when the executor receives the
canonical chord and selected source context. No new public abstraction or
handler variant is required.

Ledger correction: the three shared/test rows for `list_navigation.rs`,
`prs_key_tests.rs`, and `prs_integration_tests.rs` were discovered while
deleting the final Dashboard/Split fallback references and are recorded here
before final GREEN verification. They neither expand production behavior nor
migrate PR-mode dispatch; they connect existing pure geometry and tests to the
already-approved S3 route. Full verification then exposed the existing
all-context BackTab explain fixture's missing explicit Split and Errors
overrides after the new source-audited cycle-pane rows landed; that focused S2
fixture update is recorded above before the corrective edit.

### S3 RED contract

The first tests cover only observable pure seams: source state selects the
correct ordered context; existing macOS Option symbols become the canonical
Alt-digit chord; one production-route resolution chooses the expected handler;
and the closed executor produces the current smallest event/boundary result,
including conditional terminal forwarding. They are wired before those modules
or functions exist and must fail to compile for the intended missing S3
contracts. Inventory parity RED is added separately when the first Split/Errors
fixture exposes a source-valid row absent from the S0 inventory.

### S3 slice boundaries

- `AppContext` owns exactly one immutable `ActionRegistrySnapshot`, moved from
  the startup result. Event routing borrows it under the existing root context
  lock, derives one stack, calls `snapshot.resolve` once, drops the guard, and
  executes that one typed result.
- Shell and terminal raw forwarding remain boundary effects and continue using
  the unchanged `pty_encoding::key_to_bytes` path. Unsupported canonical input
  forwards in PTY-owned contexts and never creates a fallback action route.
- Rapid `qqq` remains the approved state machine outside the single-chord
  registry. Raw Dashboard search and all Issues/PR/Actions/modal/editor behavior
  remain on their current later-slice paths; only their inherited global/pre-mode
  shortcut seam uses the registry in S3.
- No old dispatch map or compatibility fallback remains for a migrated S3
  context. `core.open-keys` remains behaviorally inert until the approved S6
  Keys UI exists; it does not create a second dispatch authority.


### S3 RED evidence

Before any S3 production module existed, `cargo test --bin jefe
action_context_tests --no-fail-fast` exited 101 with `E0583` for missing
`src/action_context.rs` and `src/app_input/action_handlers.rs`, plus `E0432` for
the missing single `resolve_registry_key` route. This is the intended first RED:
the tests were wired and compiled against absent S3 selector, executor, and
one-resolution contracts. A preceding run caught a test-module declaration
inserted inside an existing expression; that wiring error was repaired and is
not counted as behavioral RED evidence.

### S3 GREEN implementation and verification evidence

- Runtime ownership: `StartupPersistence::keymap_snapshot` is moved into the
  single root `AppContext`; the production key route derives one
  `ActionContext`, canonicalizes one `KeyEvent`, invokes
  `ActionRegistrySnapshot::resolve` exactly once, releases the state/context
  guards, and executes the closed typed `HandlerKey` result. Only the test-only
  helper composes a fresh compiled snapshot.
- Duplicate authority deletion: the old shell-overlay, terminal-capture,
  global/pre-mode, Dashboard/Split navigation/lifecycle/mode, Errors, and
  Terminal Manager key selectors were removed. `src/app_input/errors.rs` was
  deleted; a final source scan found no migrated selector names.
- Focused S3 behavior: action-context 4 passed; typed action handlers 3 passed;
  root registry routing 3 passed; Split routing 2 passed; PR Dashboard entry 1
  passed; Option-digit canonicalization 1 passed; terminal control inventory 1
  passed; PTY passthrough 6 passed. The Errors executor fixture proves `j`/`k`
  scroll only in detail and remain consumed/no-op outside detail, while arrows
  still navigate.
- Exact PTY bytes: `ctrl_c_maps_to_etx_byte`,
  `function_keys_use_expected_xterm_sequences`,
  `modified_arrow_keys_use_xterm_sequences`,
  `modified_edit_keys_use_xterm_sequences`, and
  `modified_function_keys_use_xterm_sequences` each passed.
- Full locked regression: `cargo test --workspace --all-features --locked -q`
  passed: library 2,889 passed / 1 ignored, binary 809 passed, and every
  integration/doctest target passed. The first full run exposed the existing
  all-context BackTab explain fixture's missing explicit Split/Errors overrides;
  the fixture was corrected and its focused test passed before the complete
  rerun.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  with zero warnings.
- `cargo build --workspace --all-features --locked` — PASS.
- `scripts/check-architecture.sh`, `cargo test architecture -- --nocapture`,
  `cargo xtask check source-size`, `cargo test source_size -- --nocapture`,
  `cargo xtask check clippy-allows`, and `cargo test clippy_allow_policy --
  --nocapture` — PASS. The near-limit files remain below the 1,000-line hard
  limit: `action_registry.rs` 995, `default_action_inventory.rs` 915,
  `prs_key_tests.rs` 998, and `prs_integration_tests.rs` 993.
- `cargo xtask quick` and `git diff --check` — PASS.
- Strict TUI: `dev-docs/tmux-scenarios/errors-mode.json` passed all 9 steps
  through the current production binary. The applicable Terminal Manager
  script was run twice and consistently reached its unchanged New Repository
  form but timed out at step 12 waiting for repository creation after Enter;
  the captured final frame remained in that modal. The failure occurs in the
  later-slice modal/form submission leg before the scenario reaches Terminal
  Manager shell behavior; all Rust Terminal Manager registry/executor and full
  regressions pass. No harness, modal key map, or scenario was changed because
  those owners are explicitly outside S3.

No dependency, workflow, quality configuration, public abstraction, UI
projection, mouse route, harness conversion, `.llxprt` content, or generic
payload was added. No commit or push was performed.