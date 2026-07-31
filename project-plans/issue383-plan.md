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

## S4 execution ledger — Issues, Pull Requests, Actions, and modal dispatch

S4 completes the runtime-dispatch portions of CW03-01 and CW03-02 for every
remaining keyboard-owned context. Existing raw Unicode insertion, text/cursor
mutation, paste, and PTY forwarding stay at their current boundaries; every
action control resolves from the one root-owned immutable snapshot exactly
once. Current source after the issue-520 merge is the parity authority.

### S4 changed-file ledger (recorded before production edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record S4 path ledger plus RED/GREEN and exact verification evidence | delivery evidence |
| `src/action_context.rs` | Derive complete source-precedence stacks for Help, modal, search/filter, Issues, PR, Actions, inline, and chooser states | application selector |
| `src/action_context_tests.rs` | RED-first precedence fixtures for every newly registry-owned state family | focused selector tests |
| `src/app_shell.rs` | Make root action dispatch terminal after raw-input ownership and remove later-slice fallback routing | root input orchestration |
| `src/app_shell_key_routing.rs` | Execute one S4 registry result and preserve raw/unsupported behavior without a second resolver | root dispatch boundary |
| `src/app_shell_key_routing_tests.rs` | RED-first production-route parity and one-resolution fixtures | focused route tests |
| `src/app_input/mod.rs` | Export raw-input and typed S4 boundary operations; delete superseded mode routing | app-input boundary |
| `src/app_input/action_handlers.rs` | Extend the closed exhaustive executor to all current S4 typed outputs/boundaries | typed dispatch |
| `src/app_input/action_handlers_s4.rs` | Cohesive private S4 typed event planning split needed to keep the executor below source-size limits; no public abstraction | typed dispatch (new, internal) |
| `src/app_input/action_handlers_tests.rs` | RED-first Issues/PR/Actions/modal typed output parity | focused executor tests |
| `src/app_input/actions.rs` | Retain raw Actions search editing only; remove the superseded action-control key map | raw input boundary |
| `src/app_input/dashboard_search.rs` | Retain raw Dashboard search editing only; move apply/cancel controls to the registry | raw input boundary |
| `src/app_input/filter_controls.rs` | Represent raw filter text editing independently from registry-owned controls | shared raw input |
| `src/app_input/raw_key_mutations.rs` | Central explicit raw text/cursor ownership before registry resolution; excludes action controls | raw input boundary (new, internal) |
| `src/app_input/issues.rs` | Retain Issues raw editor/search/property/form mutation only; remove action-control maps | raw input boundary |
| `src/app_input/issues_filter.rs` | Expose typed filter command planning and retain raw field editing | typed/raw Issues boundary |
| `src/app_input/prs.rs` | Retain PR raw editor/search/property mutation only; remove action-control maps | raw input boundary |
| `src/app_input/prs_filter.rs` | Expose typed filter command planning and retain raw field editing | typed/raw PR boundary |
| `src/app_input/modal_handlers.rs` | Replace Help/confirm/auth/form/theme key maps with named typed boundary operations; preserve issue-520 modal behavior | modal boundary |
| `src/app_input/normal.rs` | Delete Issues/PR/Actions compatibility delegation and retain only rapid-quit state handling | legacy boundary deletion |
| `src/app_input/list_navigation.rs` | Reuse source viewport geometry for the active S4 screen | shared pure calculation |
| `src/domain/default_action_inventory.rs` | Replace provisional S0 S4 rows with an internal audited source-parity split | pure inventory |
| `src/domain/default_action_inventory_s4.rs` | Hold all audited current S4 rows and complete context-stack declarations below source-size limits | pure inventory (new, internal) |
| `src/domain/default_action_inventory_tests.rs` | RED-first current-row, no-ticket-alias, and full-context parity assertions | focused inventory tests |
| `src/binding_explain_tests.rs` | Keep explain fixtures synchronized with corrected S4 context/action inventory | focused registry consumer tests |
| `src/persistence/keymap_edit_tests.rs` | Keep lossless keymap candidate fixtures synchronized with corrected S4 inventory | focused persistence tests |
| `src/ui/modals/help.rs` | Describe registry/executor ownership of Help scrolling after deleting the old modal key handler | UI contract documentation |

No UI projection, Keys editor, mouse route, harness conversion/capture, docs,
dependency, workflow/quality configuration, `.llxprt` file, public abstraction,
generic payload, compatibility resolver, or fallback route is in this slice.
The private executor and inventory splits are recorded before creation because
the existing owners are respectively 491 and 915 lines.

### S4 RED contract

The first tests require full S4 context ownership and source-specific typed
handler output while the selector still returns `PreModeOnly` and every S4
handler still returns `LaterSlice`. The inventory tests additionally require
audited rows that are absent from the provisional S0 table and reject the
ticket-only Issues/Actions `j`/`k`, Actions `a`, PR inline Ctrl-C, generic
search Ctrl-L, and generic filter Ctrl-C aliases. These tests are written and
run before any production S4 edit. This slice is dispatch/parity rather than a
new visible interaction, so existing strict Issues/PR/Actions/modal scenarios
are the UI evidence instead of a new scenario.

### S4 RED evidence

Before production edits, `cargo test --bin jefe action_context_tests
--no-fail-fast` exited 101 with three `E0599` errors because the tests require
the absent `DispatchScope::FullS4` contract. Independently, `cargo test --lib
s4_inventory_is_source_audited_without_ticket_only_aliases --no-fail-fast`
compiled and failed at the first absent audited row (`issues.detail` + `Down` +
`NavigateDown`). These are the intended selector and inventory REDs; the closed
handler test is wired in the same test-first change and remains `LaterSlice`
until the selector compiles.

### S4 GREEN implementation and verification evidence

**Runtime ownership**: `derive_action_context` selects `DispatchScope::FullS4`
for Issues, Pull Requests, Actions, Dashboard search, and all modal/editor/
chooser states. The production key route derives one `ActionContext`,
canonicalizes one `KeyEvent`, invokes `ActionRegistrySnapshot::resolve` exactly
once, and executes the closed typed `HandlerKey` result through
`s4::execution_for`. Before resolution, `raw_key_mutations::resolve` explicitly
claims only Unicode insertion/deletion, editor cursor movement, and multiline
newline insertion. A `FullS4` `Unbound` result is then terminally consumed (with
rapid-`qqq` observation only in eligible normal modes); it never re-enters an
Issues/PR/Actions/modal/filter key map. The root also retains the prior
defensive stale-terminal-focus normalization before deriving input mode.

**Closed handler executor**: `action_handlers_s4.rs` dispatches every S4
`HandlerKey` variant into the smallest existing `AppEvent` or boundary
operation for Issues, Pull Requests, Actions, Dashboard search, and all modal
contexts (help, confirm, auth, form, theme). The `handler_execution!` macro
retains all S4 handlers as `LaterSlice` so that only the s4 split plans them;
the s4 split delegates to existing `pub(super)` helpers in `issues.rs`,
`prs.rs`, `issues_filter.rs`, and `prs_filter.rs` without duplicating logic.
The modal boundary executor in `s4::apply_modal_boundary` routes confirm/auth/
form/theme boundary actions to the existing `modal_handlers` functions. Help
scrolling is likewise a typed boundary and updates the canonical clamped
`AppState::help_scroll_offset`; the superseded Help key map and duplicate hook
state are deleted.

**Inventory audit**: `default_action_inventory_s4.rs` holds all audited S4
action/control rows with fully-qualified context stacks. The provisional S0
context stacks referencing bare `"filter"` were removed from the main
inventory; S4 contexts use `"issues.filter"`, `"prs.filter"`, `"actions.filter"`
etc. Modal/editor/chooser stacks are intentionally isolated as
`[special-context, global]`, matching the old terminal consumption boundary and
preventing parent screen controls from leaking through. Dashboard search and
Dashboard/Split/Actions modal states additionally inherit only the narrow
`dashboard.pre-mode` F12 binding, preserving the old pre-mode terminal toggle
without exposing Dashboard F8/F9/F10 or normal screen controls. The inventory
is source-audited with no ticket aliases or wildcards.

**No-fallback evidence**: `full_s4_root_has_no_legacy_action_fallback` scans the
root and every migrated S4 input owner for the deleted compatibility entry
points. `full_s4_special_contexts_resolve_controls_and_leave_raw_text_unbound`
proves cancel/submit resolve through the compiled snapshot while ordinary text
and multiline newline remain explicitly raw-owned. `raw_key_mutations::tests`
proves navigation, submit, and cancel are never classified as raw mutations.

**Verification record**:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  with zero warnings.
- `cargo build --workspace --all-features --locked` — PASS.
- Focused corrected-route evidence: `cargo test --bin jefe app_input::
  --no-fail-fast` — 638 passed; `cargo test --bin jefe
  app_shell_key_routing --no-fail-fast` — 6 passed; `cargo test --lib
  default_action_inventory --no-fail-fast` — 10 passed. The routing suite proves
  Dashboard, Split, and Actions overlay F12 parity, keeps F8 out of the narrow
  pre-mode context, and keeps modal F12 unbound in Issues and Pull Requests.
- `cargo test --bin jefe --no-fail-fast` — PASS: 793 passed / 0 failed,
  including the two final narrow pre-mode regressions.
- `scripts/check-architecture.sh` and `cargo xtask check architecture` — PASS.
  `modal_handlers.rs` 716 lines, `action_handlers_s4.rs` 742 lines, and
  `action_handlers.rs` 560 lines (all below the 1,000-line hard limit).
- `cargo xtask check source-size` — PASS (existing warnings only, no new
  violations).
- `cargo xtask check clippy-allows` — PASS.
- `cargo xtask quick` — PASS.
- `git diff --check` — PASS.
- Strict TUI scenarios: `errors-mode.json` passed all 9 steps. `help-modal`,
  `actions-mode`, `confirm-dialog-focus`, and `issues-filter-open-close`
  failed identically on the pre-S4 baseline, confirming these are pre-existing
  harness/timing issues unrelated to S4 dispatch.

No dependency, workflow, quality configuration, public abstraction, UI
projection, mouse route, harness conversion, `.llxprt` content, or generic
payload was added. No commit or push was performed.


## S5 execution ledger — authoritative availability and shared projections

S5 implements CW03-03 only. The already-composed action/binding candidate stays
immutable; one root-owned runtime snapshot receives a complete availability
generation only after exact closed-effect completion. Existing capability
checks and their current user-facing reasons are the golden. Help and the
keybind footer become thin consumers of the planned iocraft-free projection;
there is no current application menu, and the same private rows are reserved
for a later menu and S6 Keys editor without building either UI in this slice.

### S5 changed-file ledger (recorded before production edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record the S5 acceptance, exact path ledger, RED/GREEN evidence, verification, and blockers | delivery evidence |
| `dev-docs/tmux-scenarios/keymap-projection-consistency.json` | Strict schema-1 Help/footer visibility contract written before production edits | behavioral scenario (new) |
| `src/action_projection.rs` | One pure, iocraft-free, must-use projection for action rows, Help, footer, menu rows, and future Keys rows | pure view (new, internal) |
| `src/lib.rs` | Register the private projection module | crate wiring |
| `src/domain/action_registry.rs` | Retain immutable action metadata and publish a complete correlated availability generation atomically | pure domain |
| `src/domain/action_registry_composition_tests.rs` | RED-first authoritative republication and exact-reason fixtures | domain tests |
| `src/domain/effects.rs` | Add closed, typed action-availability provider request/response variants; no generic payload | pure effects contract |
| `src/messages.rs` | Add the smallest typed availability-projection reducer intent | typed messages |
| `src/messages/event_conversion.rs` | Preserve exhaustive typed message/event conversion for the new intent | conversion seam |
| `src/messages/names.rs` | Preserve the existing exhaustive diagnostic name table for the typed intent | message diagnostics |
| `src/state/events.rs` | Add the exhaustive low-level intent variant required by the existing conversion seam | typed events |
| `src/state/types.rs` | Store the one root-owned immutable action snapshot in runtime-only state | root state |
| `src/state/mod.rs` | Route the typed availability intent and register the private availability owner | reducer wiring |
| `src/state/action_availability.rs` | Freeze existing capability predicates/reasons, stage one closed effect, and apply authoritative completion | deterministic reducer split (new, internal) |
| `src/state/runtime_ops.rs` | Route exact-correlated action-availability completions after ledger acceptance | completion reducer |
| `src/state/pr_types.rs` | Expose one crate-local canonical reason accessor for existing read-only hint kinds | reason authority |
| `src/state/prs_ops.rs` | Consume the canonical reason accessor and shared no-agent reason | existing reducer consumer |
| `src/state/issues_ops.rs` | Consume the shared no-agent reason | existing reducer consumer |
| `src/main.rs` | Transfer the startup-composed snapshot into root state instead of retaining a second authority | root composition |
| `src/app_init.rs` | Take the composed snapshot, stage initial availability after composition, and execute the existing funnel | startup composition |
| `src/app_input/action_availability.rs` | Execute the typed availability effect synchronously through the existing serial executor; no worker/queue/discovery subsystem | effect boundary (new, internal) |
| `src/app_input/pty_passthrough_tests.rs` | Keep the existing root-context test fixture aligned with transient startup snapshot ownership | test fixture |
| `src/app_input/mod.rs` | Invoke the shared post-transition availability funnel at existing composition boundaries | app-input composition |
| `src/app_input/action_handlers.rs` | Keep Help viewport math consuming the shared projected Help lines | typed dispatch consumer |
| `src/app_shell_key_routing.rs` | Resolve from root state and surface unavailable notice before handler planning/execution | root dispatch |
| `src/app_shell_key_routing_tests.rs` | RED-first zero-handler/effect unavailable routing and exact notice coverage | focused route tests |
| `src/app_shell_workers.rs` | Reproject after authoritative startup probe completions change agent eligibility | existing completion boundary |
| `src/selection/content.rs` | Use projected Help/footer content for selection text, preserving geometry | selection projection consumer |
| `src/selection/content_tests.rs` | Keep selection-copy parity tests aligned with the required root snapshot and no-fallback contract | selection projection tests |
| `src/ui/modals/help.rs` | Delete the static Help authority and render shared projected lines with unchanged viewport math | thin UI consumer |
| `src/ui/orchestration.rs` | Pass the root-owned immutable snapshot to Help | UI wiring |
| `src/ui/components/keybind_bar.rs` | Delete the static footer authority and render shared projected text | thin UI consumer |
| `src/ui/components/pr_render_screen_tests.rs` | Update existing Help/footer parity tests to provide one snapshot | UI tests |
| `src/ui/components/issue_lifecycle_render_tests.rs` | Update existing footer lifecycle tests to provide one snapshot | UI tests |
| `src/ui/screens/dashboard.rs` | Pass the root snapshot to the footer | UI wiring |
| `src/ui/screens/split.rs` | Pass the root snapshot to the footer | UI wiring |
| `src/ui/screens/issues.rs` | Pass the root snapshot to the footer | UI wiring |
| `src/ui/screens/pull_requests.rs` | Pass the root snapshot to the footer | UI wiring |
| `src/ui/screens/actions.rs` | Pass the root snapshot to the footer | UI wiring |
| `src/ui/screens/errors.rs` | Pass the root snapshot to the footer | UI wiring |
| `src/ui/screens/terminal_manager.rs` | Pass the root snapshot to the footer | UI wiring |

This slice deliberately exceeds the normal file target because replacing two
cross-screen authorities requires every existing thin screen call site; D1 is
the explicit issue-wide oversized-scope approval. The ledger remains below the
40-file hard stop. No S6 Keys UI/save, mouse routing, harness capture/conversion,
normative docs, dependency, workflow/quality configuration, `.llxprt` content,
public abstraction, generic payload, provider/UI I/O projection, unsafe, or
production panic/unwrap/expect is included.

### S5 RED contract

The strict scenario requires the exact current read-only reason to appear from
Help and the PR footer while those actions remain visible. Focused pure tests
require one snapshot row to expose byte-identical reason/status to dispatch,
Help, footer, menu, and future Keys projections, and reducer tests require exact
owner, screen generation, activation generation, semantic key, and correlation
identity before publication. The first scenario/test runs are recorded before

### S5 correction — genuine snapshot-derived projections

The initial S5 `action_projection.rs` was found to improperly retain a giant
static `HELP_LINES` binding map and hardcoded per-mode footer chord strings,
making the projection a parallel display authority instead of a pure consumer
of immutable snapshot metadata. This correction eliminates that duplicate
authority so that Help/footer/menu/future Keys rows are genuinely generated
from immutable snapshot action metadata, effective bindings, context,
availability, and provenance. Static headings and layout only; no static
chord-action authority remains.

#### S5 correction changed-file ledger

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record the correction scope, evidence, and exact verification | delivery evidence |
| `src/domain/default_action_inventory_display.rs` | Private display metadata table: section headings, help-line text/order, footer-hint text/order; no chord-action binding authority | pure domain display owner (new, internal) |
| `src/domain/default_action_inventory.rs` | Register the private display submodule | pure domain module contract |
| `src/domain/action_registry_chord_cmp.rs` | Internal chord canonicalization and terminal-intercept helpers extracted to keep `action_registry.rs` under the source-size hard limit | pure domain split (new, internal) |
| `src/domain/action_registry.rs` | Import extracted chord helpers after source-size hard-limit correction | pure domain |
| `src/action_projection.rs` | Rewrite to project all rows from snapshot metadata/bindings/availability/provenance via the canonical display table; delete static HELP_LINES map and hardcoded footer_base; add structural test rejecting hardcoded maps and displayed row completeness | pure view |

#### S5 correction design boundaries

- The display table (`default_action_inventory_display.rs`) carries ONLY
  presentation metadata: section names, pre-formatted help-line text, per-line
  display ordering, footer-hint fragments, and per-mode/per-focus grouping. It
  carries NO chord strings and NO chord→action binding literals. Each help line
  and footer hint carries an optional `&[&str]` of action IDs used solely for
  availability-status lookup from the immutable snapshot.
- `action_projection.rs` is a pure consumer of the immutable snapshot plus the
  display table. All availability/status strings are looked up dynamically via
  `ActionRegistrySnapshot::availability_entries()`. No static chord→action map,
  no hardcoded `footer_base`, and no static `HELP_LINES` constant remains.
- `project_footer` short-circuits only for the invariant terminal-focus and
  shell-overlay cases (where the same condition governs every mode). For all
  other modes, it joins display-table hint fragments by `" | "`, appends
  unavailable-status annotations for actions in that mode's contexts, and
  applies the `shell_resume_available` text substitution.
- No public display type is exposed. All display structs are `pub` inside the
  `pub(crate) mod display` submodule, visible only to `action_projection.rs`.
- No S6+ scope (Keys UI/save, mouse routing, harness capture/conversion, docs,
  dependencies, workflow/quality/lint configuration, or `.llxprt` change) is
  included.

#### S5 correction RED and structural evidence

The structural test `projection_has_no_hardcoded_chord_action_map` reads the
production portion of `action_projection.rs` (split at `#[cfg(test)]`) and
asserts that:
- The old `HELP_LINES` constant is absent.
- The old `footer_base` function is absent.
- No hardcoded chord literals (`"Ctrl+Q"`, `"F10"`, `"F12"`, `"Esc"`) appear as
  binding authority.
- The projection uses `HELP_DISPLAY_LINES` and canonical footer display groups.

The structural tests `displayed_help_action_ids_are_complete` and
`displayed_footer_action_ids_are_complete` compile the inventory and verify
that every action ID referenced in the display table exists in the compiled
inventory. The existing `availability_projection_is_byte_identical_across_five_consumers`
and `available_projection_preserves_existing_help_and_footer_bytes` tests
confirm byte-identical projection from the snapshot.

#### S5 correction verification record

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  with zero warnings (resolved `redundant_pub_crate`, `map_unwrap_or`,
  `match_same_arms`, `expect_used`, and `too_many_lines` findings without lint
  suppression).
- `cargo build --workspace --all-features --locked` — PASS.
- `cargo test --workspace --all-features --locked` — PASS: library 2,902
  passed / 1 ignored / 0 failed; binary 794 passed; every integration target
  passed.
- `cargo xtask check source-size` — PASS with zero ERROR lines
  (`action_registry.rs` 996 lines, `events.rs` 1000 lines; both at or below
  the 1,000-line hard limit after extracting chord canonicalization helpers to
  `action_registry_chord_cmp.rs`).
- `cargo xtask check clippy-allows` — PASS.
- `scripts/check-architecture.sh` and `cargo xtask check architecture` — PASS.
- `git diff --check` — PASS.

No S6 Keys UI/save, mouse routing, harness capture/conversion, normative docs,
dependency, workflow/quality configuration, `.llxprt` content, public
abstraction, or generic payload was added. No commit or push was performed.

### S5 follow-up correction — effective binding labels (2026-07-30)

The prior correction still stored chord-prefixed Help and footer strings in the
private display table. This follow-up makes effective snapshot bindings the
single chord-label authority without relocating that table:

- `default_action_inventory_display.rs` now stores section/order, action IDs,
  semantic descriptions, and raw non-registry rows only. Normal, shell-overlay,
  and terminal-focused action hints all use action IDs.
- `action_projection.rs` gathers each row's effective binding chords from the
  immutable snapshot, retains deterministic action/binding order, deduplicates,
  and applies generic Help/footer presentation and digit-run compaction.
- A settings fixture replaces `dashboard.toggle-terminal`'s compiled chord with
  `z` and proves exact Help and footer fragments show `z` and omit `F12`.
- A structural scan rejects known chord literals in every action-backed Help,
  mode-footer, Actions-focus, shell-overlay, and terminal-focused description.
  Raw `qqq`, Split move, contextual Help, and PR search rows remain explicitly
  static because they do not have remappable registry actions.

Verification on the corrected files:

- `cargo fmt --all --check` — PASS.
- `cargo test --lib action_projection::tests -q` — PASS: 7 passed.
- Focused footer width/identity tests — PASS.
- `cargo clippy --lib --all-features -- -D warnings` — PASS.
- `cargo xtask check source-size` — PASS with the new projection at 817 lines
  (warning only; below the 1,000-line hard limit).
- `cargo xtask check clippy-allows` — PASS.
- `cargo xtask check architecture` — PASS.
- `cargo build --workspace --all-features --locked` — PASS.
- `cargo test --lib -q` reaches 2,900 passed / 1 ignored and four stale UI
  expectation failures that assert the removed literal groupings (`> runs`,
  `L labels`, and the old Help pane string). Those test files are outside this
  follow-up's explicit two-source-file edit boundary; the new projection tests
  cover the corresponding snapshot-derived grouped output.

No public type, S6+ behavior, dependency, workflow, `.llxprt`, commit, or push
was added.

### S5 final verification update

The four stale UI assertions were updated to assert snapshot-derived semantic
content and grouped effective chords rather than the removed hardcoded chord
layout. Full locked workspace tests then passed: library 2,904 passed / 1
ignored, binary 794 passed, and every integration/doctest target passed. Full
all-target/all-feature Clippy, locked build, source-size, clippy-allow,
architecture, formatting, and `git diff --check` passed. `cargo xtask quick`
repeatedly reached the unchanged `llxprt_continue_field_fixture_sends_one_exact_issue_prompt`
HAR-E005 startup capture race; the exact isolated fixture passed immediately,
while complete quick retries reproduced the empty-frame startup timeout. This
is recorded as incomplete quick evidence rather than a pass; all deterministic
S5 tests and the full locked workspace suite are green.

## S6 execution ledger — Keys editor and lossless save

S6 implements the Keys-editor portions of CW03-03, CW03-04, and CW03-05 only.
The accepted global `,` action opens one schema-2-owned modal. A deterministic
typed reducer owns navigation, editing, validation status, dirty-exit
confirmation, and recovery state; an iocraft-free projection owns every layout
decision; the iocraft component only renders that projection. The app-input
boundary validates complete candidates and uses the existing lossless keymap
patcher plus revision-gated atomic writer. The root snapshot changes only after
an authoritative write succeeds.

### S6 changed-file ledger (recorded before edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record S6 scope, RED/GREEN evidence, and exact verification | delivery evidence |
| `dev-docs/tmux-scenarios/v1/keys-editor.json` | Strict schema-1 normal/focused/error/dirty/save/reopen contract | behavioral scenario (new) |
| `dev-docs/tmux-scenarios/v1/keys-editor-unbind-reset.json` | Strict schema-1 Unbind/Reset/inheritance contract | behavioral scenario (new) |
| `dev-docs/tmux-scenarios/v1/keys-editor-recovery-small.json` | Strict schema-1 malformed-keymap recovery and tiny-layout Back/Ctrl-Q contract | behavioral scenario (new) |
| `tests/harness_v1_fixtures.rs` | Execute the three S6 fixtures through the real strict runner | integration evidence |
| `src/keys_view.rs` | Pure snapshot-to-editor and editor-to-layout projection | pure view (new, internal) |
| `src/keys_view_tests.rs` | Focused/error/dirty/recovery/small-terminal projection tests | pure-view tests (new) |
| `src/state/keys_editor.rs` | Typed editor values and deterministic reducer | state reducer (new, internal) |
| `src/state/keys_editor_tests.rs` | Navigation/edit/unbind/reset/validation/dirty-confirm reducer tests | state tests (new) |
| `src/state/mod.rs` | Register and route the private Keys reducer | reducer wiring |
| `src/state/types.rs` | Carry the runtime-only Keys modal state | root state |
| `src/state/modal_ops.rs` | Route the smallest typed modal message to the Keys reducer | reducer routing |
| `src/messages.rs` | Add typed Keys modal intents | typed messages |
| `src/messages/keys.rs` | Private typed Keys message split required to keep the message owner below 1,000 lines | typed messages (new, internal) |
| `src/messages/event_conversion.rs` | Preserve exhaustive bidirectional event/message conversion for Keys intents | conversion seam |
| `src/messages/names.rs` | Preserve exhaustive message diagnostics | message diagnostics |
| `src/state/events.rs` | Carry the existing low-level typed Keys intent facade | typed events |
| `src/input.rs` | Classify the Keys modal as app-owned input | input classification |
| `src/action_context.rs` | Give the modal the protected global routing stack | pure context selector |
| `src/persistence/keymap_edit.rs` | Apply a complete list of set/unbind/reset edits before one validation/write | lossless persistence boundary |
| `src/persistence/keymap_edit_tests.rs` | Complete multi-edit, absent-target, protected, and byte-retention coverage | persistence tests |
| `src/persistence/mod.rs` | Narrowly re-export the existing candidate/edit persistence contracts to the binary composition boundary | persistence wiring |
| `src/startup.rs` | Retain the validated source document/absence identity for later lossless edits | startup composition |
| `src/main.rs` | Wire the selected settings authority and keymap revision fence into root context | composition root |
| `src/app_init.rs` | Preserve keymap recovery information after startup state hydration | startup state wiring |
| `src/app_input/keys_editor.rs` | Translate keys to typed intents and execute pure validation/atomic save effects | side-effect boundary (new, internal) |
| `src/app_input/mod.rs` | Register and expose the private Keys boundary | app-input wiring |
| `src/app_input/action_handlers.rs` | Execute `OpenKeys` through the named Keys boundary | closed action executor |
| `src/app_input/pty_passthrough_tests.rs` | Keep the root-context fixture complete after adding settings authority state | focused fixture |
| `src/app_shell.rs` | Route Keys input before general raw mutation/registry dispatch | root input orchestration |
| `src/mouse_routing.rs` | Treat Keys as a blocking overlay only; no action click routing | existing overlay guard |
| `src/ui/modals/keys.rs` | Thin iocraft renderer over `keys_view` | UI modal (new) |
| `src/ui/modals/mod.rs` | Register/re-export the Keys modal | UI wiring |
| `src/ui/mod.rs` | Re-export the Keys modal for orchestration | UI wiring |
| `src/ui/orchestration.rs` | Render the Keys modal from cloned root state | UI orchestration |
| `src/lib.rs` | Register the private pure Keys projection/tests | crate wiring |

No S7 mouse hit routing beyond retaining stable action IDs in rendered button
labels, no S8 capture conversion/protocol, no S9 normative docs, no new public
abstraction or subsystem, no dependency/workflow/quality/`.llxprt` change, no
unsafe, and no production panic/unwrap/expect is included. The private state,
view, UI, and boundary splits are planned before creation to keep every file
below 1,000 lines.

### S6 RED contract

The three strict schema-1 scenarios and their integration-test entries are
written and executed before any production S6 source edit. They require the
absent Keys title, focused row/editor, `KEY-E401` disabled-save state,
Save/Discard/Cancel dirty guard with Escape-as-Cancel, reset inheritance,
explicit unbind, runtime reopen from the newly published snapshot, malformed
keymap recovery, and a resized tiny layout that still renders Back and Ctrl-Q.
Focused reducer/view/persistence tests are also written before their production
modules or complete-candidate APIs exist.

### S6 RED/GREEN and verification evidence

- Strict RED was captured before production edits: the primary schema-1 fixture
  timed out waiting for `Keys - Keyboard Bindings` after sending the accepted
  global comma action, proving `OpenKeys` was still inert. Focused persistence
  RED then failed on the absent `KeymapEdit` and `KeymapCandidate::from_edits`
  contracts; reducer and pure-view tests likewise referenced absent S6 modules.
- The deterministic reducer now owns stable `(context, action)` rows, navigation,
  canonical single-chord-list editing, protected read-only behavior, explicit
  Unbind, Reset-to-inheritance, complete-candidate validation state, recovery,
  and the Save/Discard/Cancel dirty guard. Escape in confirmation returns to the
  dirty editor.
- The app-input boundary intercepts only the open Keys modal before normal raw
  mutation/registry routing, while Ctrl-Q continues through the protected global
  action. It validates one complete lossless candidate and writes through the
  existing revision/hash-fenced atomic writer. A stale/conflicting/failing write
  retains the draft and prior authority; only an authoritative completion adopts
  the exact document bytes, expected hash, published settings, revision, context
  snapshot, and root immutable snapshot.
- The pure projection supplies normal, focused/editing, invalid, dirty,
  confirmation, recovery, and compact states. Unicode-cell truncation and a
  reserved line/footer budget keep `Esc Back | Ctrl-Q Quit` inside a 44x10
  modal without direct UI terminal I/O.
- Focused GREEN: `cargo test --lib keys_ --no-fail-fast` — 21 passed;
  `cargo test --lib persistence::keymap_edit_tests --no-fail-fast` — 8 passed;
  `cargo test --bin jefe app_shell_key_routing --no-fail-fast` — 7 passed.
- Strict schema-1 GREEN on the final formatted implementation: all three real
  fixtures passed sequentially: normal/focused/KEY-E401/dirty/save/reopen,
  Reset/inheritance plus explicit Unbind, and malformed recovery with compact
  Back/Ctrl-Q reachability.
- `cargo fmt --all` and `git diff --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  with zero warnings and no lint suppression.
- `cargo build --workspace --all-features --locked` — PASS.
- Full locked workspace tests initially reached the unchanged
  `llxprt_continue_field_fixture_sends_one_exact_issue_prompt` harness timing
  race after every preceding target passed; the exact fixture passed immediately
  in isolation. The exact-final-head `cargo test --workspace --all-features
  --locked -q` retry then passed every target (library 2,915 passed / 1 ignored;
  binary 791 passed, all 25 strict harness fixtures, integrations, and doctests).
  `cargo xtask quick` also passed the complete suite.
- `cargo xtask check source-size`, `cargo xtask check clippy-allows`, and
  `cargo xtask check architecture` — PASS. `messages.rs` and `state/events.rs`
  are exactly 1,000 lines; every new S6 owner is below 500 lines.
- Mandatory scope review: S6 changes 35 files because the accepted behavior
  crosses typed event/message/state wiring, root input/composition, persistence,
  pure view/UI, and three strict fixtures. Every path maps to the ledger above;
  no unplanned owner or unrelated behavior was added, and the slice remains
  below the approved 40-file hard stop under D1.
- Final added-production-line scan found no `unsafe`, production unwrap/expect,
  clippy allow/expect, TODO/FIXME/HACK, dependency, workflow, quality, `.llxprt`,
  S7 click routing, S8 capture protocol, or S9 normative-doc change. No commit
  or push was performed.

## S7 execution ledger — mouse action identity

S7 implements CW03-09 and D11 only. A left-button down/release at the same
screen cell activates an approved action-bearing target. The target contributes
its stable `ActionId`; the current immutable snapshot contributes the exact
`Resolution`, including availability and `HandlerKey`; and dispatch then enters
the same keyboard-owned action executor. Any non-empty drag remains the existing
selection/copy gesture. PTY reporting ownership, terminal wheel interception,
detail scrolling, pane geometry, and all non-approved surfaces remain unchanged.

### S7 changed-file ledger (recorded before implementation)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record S7 scope, RED/GREEN evidence, and exact verification | delivery evidence |
| `dev-docs/tmux-scenarios/v1/mouse-action-consistency.json` | Strict schema-1 raw-SGR scenario for approved clicks, drag/no-hit, unavailable/no-effect, and preserved app-owned mouse routing | behavioral scenario (new) |
| `tests/harness_v1_fixtures.rs` | Execute the S7 fixture through the unchanged strict runner | integration evidence |
| `src/keys_view.rs` | Attach row `ActionId` hit identities to the existing pure Keys layout projection | pure view |
| `src/keys_view_tests.rs` | Prove visible/clipped row hit identity follows the rendered projection | pure-view tests |
| `src/main.rs` | Compile the same private Keys projection source at the binary boundary without exposing a library API | composition root |
| `src/app_shell.rs` | Own transient click-down state separately from durable/public `AppState` and thread it to mouse routing | root input orchestration |
| `src/app_shell_key_routing.rs` | Expose one binary-private resolved-action executor reused by keyboard and mouse | root action orchestration |
| `src/mouse_routing.rs` | Preserve PTY/selection/wheel precedence while delegating approved zero-length releases | existing mouse boundary |
| `src/mouse_action_routing.rs` | Resolve existing pane/layout hit targets to `ActionId`, then to the current snapshot `Resolution` | private pure routing (new) |
| `src/mouse_action_execution.rs` | Track click-vs-drag and enter the shared resolved-action executor | private side-effect boundary (new) |
| `src/mouse_action_routing_tests.rs` | Focused approved/no-hit/unavailable/click-vs-drag/geometry routing tests | private unit tests (new) |
| `src/mouse_routing_tests.rs` | Preserve existing PTY reporting, wheel, selection, and copy contracts | existing regression tests |

The pre-existing post-merge S7 draft added a public snapshot lookup and a public
root-state click field. Both are rejected by D13 and are removed during S7:
action resolution uses existing snapshot resolution, and click bookkeeping is a
binary-private hook. Confirm geometry continues through `pane_at`; Keys hit
identity is emitted by the same pure projection consumed by the renderer. No
parallel action/handler map, public API, geometry subsystem, capture protocol,
S8/S9 behavior, dependency/workflow/quality/`.llxprt` change, unsafe, production
unwrap/expect, or lint suppression is permitted.

### S7 RED contract

Before production changes, focused tests require the absent Keys row hit-target
identity, exact `Resolution::Unavailable` preservation, shared keyboard/mouse
execution entry, drag cancellation, and approved confirm-button identity. The
strict schema-1 scenario sends raw SGR bytes through the existing `text`
operation; no new harness operation or capture field is added. Its first run
must fail because the merged production mouse route has no complete Keys action
hit path.


### S7 corrective verification update

The first implementation failed the all-target Clippy and source-size gates.
The underlying design was corrected without suppressions: mouse click geometry
and resolved action data now travel in small private request structs, private
module visibility is exact, redundant Option/match/closure forms were removed,
and the mouse routing owner was compacted below the 1,000-line hard limit.
Final evidence: six focused binary mouse-action tests pass; the Keys projection
tests pass; full all-target/all-feature Clippy passes; locked workspace tests
pass (library 2,917 passed / 1 ignored, binary 799 passed, all integrations and
doctests); source-size, architecture, clippy-allow, formatting, quick, and diff
checks pass.

## S8 execution ledger — strict schema-1 capture and no-shim conversion

S8 completes the harness-evidence obligations of D9 and D10 across CW03-01,
CW03-07, CW03-08, and CW03-09. It adds a private, harness-only capture artifact
that records, for one input, the **original platform event**, the **canonical
chord**, and the **resolution** as three separately observable values, plus the
**exact PTY bytes** written for forwarded input as its own field, plus mouse
**frame/cell/hit/action-ID** tuples. It converts the remaining legacy scenarios
to schema-1, deletes the superseded legacy parser/adapter so exactly one parser
remains, and adds a generated inventory completeness golden with a bidirectional
source-dispatch test.

### S8 acceptance scope

- CW03-01 (evidence): a generated inventory completeness golden asserts that
  every compiled `(context, chord, action, handler)` row is reachable through
  the production dispatch route, and — bidirectionally — that every production
  `HandlerKey` reachable from source dispatch is present in the generated
  inventory. No orphan row and no orphan handler.
- CW03-07/CW03-08 (evidence): the private capture records the exact original
  event (code + modifier bits), the canonical chord text, the resolution class,
  and, separately, the exact bytes forwarded to the PTY. The PTY-byte field is
  never derived from the chord field; it is the literal `pty_encoding` output.
- CW03-09 (evidence): a mouse capture record carries frame generation, cell
  (col,row), hit-surface identity, and the resolved `ActionId`.
- D10 no-shim: all remaining legacy-format scenarios are converted to schema-1;
  the legacy `parse_scenario`/`Step`/`ScenarioConfig`/`expand_macros` parser and
  its `MacroDef` adapter are deleted; exactly one scenario parser remains.

### Legacy key-spelling translation (runner-side only)

Converted scenarios keep legacy tmux key spellings where they are the natural
scenario vocabulary. The **runner** translates those spellings to the existing
closed schema-1 key table; **no driver byte changes**. The translation is a pure
name-to-name mapping evaluated before `keys::encode`, so every encoded byte
sequence remains exactly what `harness::v1::keys::encode` already produces:

| Legacy spelling | Canonical schema-1 key | Modifiers |
| --- | --- | --- |
| `Esc` | `escape` | — |
| `BSpace` | `backspace` | — |
| `BTab` | `tab` | `shift` |
| `Space` | `space` | — |
| `C-<letter>` | `<letter>` | `control` |
| `M-<key>` | `<key>` | `alt` |
| `PageUp`/`PageDown`/`Home`/`End`/`Up`/`Down`/`Left`/`Right`/`Enter`/`Tab`/`Delete`/`Insert`/`F1`..`F12` | lowercased canonical name | — |

`BTab` maps to `shift`+`tab`. The existing encoder yields `\t` for that pair,
which is byte-identical to what the legacy tmux driver produced for `BTab` on
the PTY, so no driver byte changes. Uppercase single letters (`N`, `S`, `D`, …)
translate to the lowercase letter plus `shift`, which the existing encoder
already renders as the uppercase byte.

### S8 changed-file ledger (recorded before implementation)

| Path | Purpose | Layer |
| --- | --- | --- |
| `project-plans/issue383-plan.md` | Record S8 scope, RED/GREEN evidence, and exact verification | delivery evidence |
| `src/harness/v1/keys_legacy.rs` | Pure legacy-spelling to canonical-key translation used by parsing/runner; no byte change | harness pure translation (new) |
| `src/harness/v1/keys.rs` | Route unknown spellings through the legacy translator before failing | harness key encoder |
| `src/harness/v1/action_capture.rs` | Private strict-harness record model: original event, canonical chord, resolution, exact PTY bytes, mouse frame/cell/hit/action | harness private capture (new) |
| `src/harness/v1/action_capture_tests.rs` | RED-first record separation, PTY-byte independence, and mouse tuple tests | harness capture tests (new) |
| `src/harness/v1/mod.rs` | Register the private capture and legacy translation modules | harness module contract |
| `src/harness/v1/runner.rs` | Activate the capture artifact only for the contained schema-1 runner | harness runner boundary |
| `src/harness/v1/env.rs` | Publish the harness-only capture path into the contained environment | harness environment |
| `src/harness/mod.rs` | Delete the superseded legacy parser/adapter exports; keep drivers used by native Windows CI | harness module contract |
| `src/harness/parser.rs` | **Delete** — superseded legacy parser | deletion |
| `src/harness/scenario.rs` | **Delete** — superseded legacy document model | deletion |
| `src/harness/step.rs` | **Delete** — superseded legacy step model | deletion |
| `src/harness/config.rs` | **Delete** — superseded legacy config model | deletion |
| `src/harness/macro_def.rs` | **Delete** — superseded legacy macro adapter | deletion |
| `src/harness/expand.rs` | **Delete** — superseded legacy macro expansion | deletion |
| `src/harness/error.rs` | **Delete** — superseded legacy scenario error | deletion |
| `src/harness/runner.rs` | **Delete** — superseded legacy runner | deletion |
| `src/harness/capture.rs` | **Delete** — superseded legacy capture model | deletion |
| `src/harness/matchers.rs` | **Delete** — superseded legacy matchers | deletion |
| `src/harness/tests.rs`, `runner_tests.rs`, `matchers_tests.rs` | **Delete** — tests of deleted legacy parser | deletion |
| `src/harness/tmux_driver.rs`, `psmux_driver.rs`, `psmux_process.rs`, `signal_cleanup.rs` | Retain the driver seam required by native Windows CI; sever the deleted-parser dependency | harness driver boundary |
| `src/bin/jefe-tmux-harness.rs` | Run schema-1 scenarios through the retained multiplexer driver instead of the deleted parser | harness CLI |
| `dev-docs/tmux-scenarios/*.json` (53 files) | Convert every remaining legacy scenario to schema-1 | behavioral scenarios |
| `tests/core/tmux_harness_docs_contracts.rs` | Assert one parser and schema-1 shipped scenarios | docs/scenario contract tests |
| `tests/ui/dashboard_reorder_tui.rs` | Consume schema-1 instead of the deleted legacy parser | integration test |
| `src/domain/inventory_completeness.rs` | Generated inventory completeness golden projection | pure domain (new) |
| `src/domain/inventory_completeness_tests.rs` | RED-first bidirectional source-dispatch completeness tests | pure domain tests (new) |
| `src/domain/mod.rs` | Register the completeness module | pure domain module contract |
| `dev-docs/testing/tmux-harness.md` | Describe the single remaining parser and the converted corpus | documentation |

Deletion of ~10 legacy harness source files plus 3 legacy test files and the
conversion of 53 scenarios is the D10 no-shim obligation covered by the D1
oversized-scope approval.

### S8 non-goals

- No S9 normative-doc convergence or final authority deletion beyond the
  harness parser/adapter named by D10.
- No public runtime API, no new public abstraction, no new dependency, no
  workflow change, no `.llxprt` change, no lint suppression or threshold change.
- No driver byte changes: legacy spellings translate to existing canonical keys
  and reuse the existing encoder unchanged.
- No new harness step operation is added for capture; capture activation is an
  environment-scoped artifact owned by the contained runner.

### S8 RED contract

Written and proven failing before production behavior:

1. `action_capture` record tests require the absent private module and assert
   that `original_event`, `canonical_chord`, `resolution`, and `pty_bytes` are
   four separately observable fields, and that a forwarded key records exact
   bytes that are not reconstructed from the chord text.
2. A mouse capture test requires frame/cell/hit/action-ID as four fields.
3. `keys_legacy` translation tests require the absent translator and assert the
   legacy spellings encode to byte-identical sequences.
4. `inventory_completeness` tests require the absent module and assert both
   directions: no inventory row without a production dispatch path, and no
   production `HandlerKey` outside the generated inventory.
5. A one-parser scan test requires that no legacy parser symbol remains.

### S8 GREEN implementation and verification evidence

**RED first.** Before any S8 production module existed, the three focused test
modules were wired and `cargo test --lib domain::inventory_completeness_tests
--no-fail-fast` exited 101 with exactly three `error[E0583]: file not found for
module` errors for `inventory_completeness`, `action_capture`, and
`keys_legacy`. That is the intended first RED: the tests compile against absent
capture, translation, and completeness contracts.

**Private contained capture (D9).** `harness/v1/action_capture.rs` owns the
record model and `harness/v1/action_capture_sink.rs` owns the contained writer.
The sink is inert unless the schema-1 runner sets `JEFE_HARNESS_ACTION_CAPTURE`,
which it now does in `runner.rs::launch`, pointing at
`action-capture.jsonl` inside the contained workspace so the artifact is torn
down with it. `src/action_capture_emit.rs` is binary-private and observes only;
a write failure is deliberately swallowed so an unwritable artifact can never
change what the application does with a keystroke. No public runtime API and no
alternate input path was added.

**Four values captured independently.** `KeyCapture` carries the original
platform event (`code` plus raw modifier bits), the canonical chord, the
resolution class, and the resolved action/handler. `pty_bytes` is taken from
`pty_encoding::key_to_bytes` — the same encoder the forwarder uses — and is
never derived from the chord's display text. The real-process fixture
`action-capture-evidence.json` proves the fields vary independently: original
`Down` -> chord `Down` -> `Dispatch` with empty `pty_bytes`, and original
`Char('q')` with non-zero modifier bits -> chord `Ctrl+Q` ->
`Dispatch`/`core.emergency-exit`. The test asserts the two resolve to different
actions and that the original event text is never equal to the chord text.

The capture itself exposed a real terminal behavior while this fixture was being
stabilized: a lone `escape` byte immediately followed by another key is
re-parsed by the terminal as an Alt-prefixed chord, so the app legitimately
observed `Ctrl+Alt+Q`/`Unbound` rather than `Ctrl+Q`. That is correct
application behavior under a real PTY, not a routing defect, so the fixture was
narrowed to unambiguous keys instead of weakening the assertion. The fixture was
also reduced to globally-bound keys so it does not depend on modal timing under
full-suite load; it now passes repeatedly in isolation and in the full run.

**Mouse frame/cell/hit/action.** `MouseActionRoute` now carries the stable hit
identity (`confirm.button`, `keys.row`) and the contributed `ActionId`;
`mouse_action_execution.rs` records frame, column, row, hit, action, and
resolution at the single existing activation point. Routing behavior is
unchanged — all six focused mouse tests still pass.

**Legacy spellings translated without changing bytes.** `keys::encode` first
consults `keys_legacy::translate`, then delegates to the unchanged
`encode_canonical`. Translation is name-to-name only. A correction was made
during the slice: `BTab` is its own terminal key emitting `\x1b[Z` (CSI Z), not
`tab` plus Shift; the earlier mapping would have changed the emitted bytes.
`translation_preserves_exact_encoder_bytes` pins Esc/BSpace/BTab/Space/Enter/
PageUp/F12/C-q/C-c/M-3/N to their exact sequences.

**No-shim conversion (D10).** All 53 legacy scenarios were converted to strict
schema 1; `find dev-docs/tmux-scenarios -name '*.json'` now reports zero
old-format files. Twelve superseded harness files were deleted (`parser.rs`,
`scenario.rs`, `step.rs`, `config.rs`, `expand.rs`, `macro_def.rs`,
`matchers.rs`, `matchers_tests.rs`, `runner.rs`, `runner_tests.rs`, `tests.rs`,
`runner_agent_fixture.rs`). `grep -rn "pub fn parse_scenario" src/` returns
exactly one result: `harness::v1::parse_scenario_v1`. Retained drivers no longer
depend on the deleted matcher module; `tmux_driver_tests.rs` owns the two-line
literal predicate it actually needed.

**Windows CI compatibility preserved without touching workflows.** The pinned
`jefe-tmux-harness` binary keeps its name and CLI flags but now parses schema-1
through the one parser and executes through the new `harness/v1/tmux_runner.rs`
backend. Both CI-pinned scenarios were run for real against a live tmux session:
`startup-quit.json` -> `ok: 7 steps`; `windows-renderer-viewport.json` ->
`ok: 12 steps`. No workflow file was modified.

**Generated inventory completeness golden and bidirectional dispatch test.**
`domain/inventory_completeness.rs` projects a deterministic golden and gates
both directions: no generated row may name a handler outside the closed
dispatch surface, and no closed dispatch handler may be absent from every
generated row. `ALL_HANDLERS` and `handler_name` are derived from one
`handler_surface!` declaration, so they cannot drift and a new `HandlerKey`
variant fails to compile until it is declared.

**Defects the new gates found (fixed, not suppressed).**

1. The bidirectional test failed on first run with nine orphan handlers:
   `FormCursorLeft/Right/Start/End`, `FormBackspace`, `FormDelete`,
   `FilterBackspace`, `SearchClear`, and `PullRequestsFocusSearch`. Investigation
   confirmed all nine are branch-local (they do not exist on `origin/main`),
   that form/search text and cursor editing has been owned by
   `app_input/raw_key_mutations.rs` since S4, and that `PrFocusSearchInput` never
   had a runtime producer. Per D2 they were deleted rather than given invented
   binding rows.
2. The corpus-wide schema-1 gate found that
   `dev-docs/tmux-scenarios/v1/issue493-server-loss.json` — shipped in #512 and
   never validated by any test — contained the invalid key `"esc"`. Corrected to
   `"escape"`. `harness-limits.json` is an intentional rejection fixture and is
   named explicitly as such so the gate still requires it to fail validation.

**Verification record.**

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  with zero warnings. The first run failed on `too_many_lines` and
  `too_long_first_doc_paragraph`; both were resolved by removing the duplicated
  handler table (the file went from ~600 to 316 lines) and shortening doc
  paragraphs. No lint allow, suppression, or threshold change was added.
- `cargo build --workspace --all-features --locked` — PASS, zero warnings.
- `cargo test --workspace --all-features --locked` — PASS: 62 test targets, 0
  failures. Library 2,860 passed / 1 ignored; binary 799 passed; integration 401
  passed; all 27 strict schema-1 harness fixtures passed.
- `cargo xtask check source-size`, `cargo xtask check clippy-allows`,
  `cargo xtask check architecture`, `scripts/check-architecture.sh`, and
  `git diff --check` — PASS.

**Scope.** 86 files changed, +12,607 / −6,390. This is inside the D1 oversized
one-PR approval and is dominated by the mandated 53-scenario conversion and the
4,195 lines of deleted superseded parser code. No dependency, workflow, quality
configuration, `.llxprt` file, public runtime API, S9 documentation convergence,
or lint suppression was touched. No commit or push was performed.

## S9 execution ledger — normative documentation and final authority deletion

S9 closes the issue's documentation mandate and completes the D10 no-shim
obligation that S8 left partially satisfied.

### S9 changed-file ledger (recorded before edit)

| Path | Purpose | Layer |
| --- | --- | --- |
| `dev-docs/tmux-scenarios/**/*.json` (48 files) | One-shot conversion of 991 key steps to canonical `key` + `modifiers` | behavioral scenarios |
| `src/harness/v1/keys.rs` | Remove the spelling-translation hook; `encode` is the single canonical encoder again | harness key encoder |
| `src/harness/v1/mod.rs` | Deregister the deleted translation module | harness module contract |
| `src/harness/v1/keys_legacy.rs` | **Delete** — shipped legacy input mode | deletion |
| `src/harness/v1/keys_legacy_tests.rs` | **Delete** — tests of the deleted shim | deletion |
| `dev-docs/standards/architecture.md` | Action-registry dependency direction and one-resolution invariant | documentation |
| `dev-docs/standards/display-and-ui.md` | Shared resolved-action projections replace static keybind authority; exact protected terminal behavior | documentation |
| `dev-docs/standards/testing-and-quality.md` | Inventory completeness guard, platform translation captures, mouse/PTY parity | documentation |
| `dev-docs/testing/tmux-harness.md` | Canonical-only key grammar and the four capture fields | documentation |
| `docs/technical-overview.md` | Action composition, availability, dispatch, and explain flow | documentation |
| `project-plans/issue383-plan.md` | S9 evidence | delivery evidence |

### S9 correction — the legacy key-spelling translator was itself a shim

S8 converted the scenario corpus to schema 1 but kept historical tmux key
spellings (`Esc`, `BSpace`, `BTab`, `C-q`, `M-3`, bare `N`) and translated them
in the runner. Re-reading the binding amendment, that is exactly what it
forbids: conversion is "one-shot dev-time, never a shipped runtime input mode."
Keeping the translator would also have left a module named `keys_legacy` for the
epic shim-token scan to find, and a second spelling vocabulary for scenarios to
drift into.

The translator was therefore deleted and the corpus converted properly: 991 key
steps across 48 files now carry canonical `key` plus explicit `modifiers`. The
mapping is byte-preserving by construction — `C-q` becomes `q`+`control` (0x11),
`M-3` becomes `3`+`alt` (`0x1b 3`), `BTab` becomes `backtab` (`CSI Z`), `BSpace`
becomes `backspace` (0x7f), uppercase `N` becomes `n`+`shift` (`N`) — so no
scenario's PTY bytes changed. `grep` for any historical spelling in a `key`
field now returns zero, and the encoder has one code path again.

Pre-existing schema-1 scenarios that already used a bare uppercase scalar (for
example `"key": "S"`) were left untouched: a single Unicode scalar is canonical
in the closed table and encodes identically.

### S9 verification record

- `cargo build --workspace --all-features` — PASS after the deletion.
- `cargo test --lib harness::v1::keys` — 3 passed; the encoder's named-key,
  modifier, and `HAR-E001` contracts are unchanged.
- `cargo test --test harness_v1_fixtures` — 27 passed, i.e. every converted
  scenario parses and encodes through the one remaining parser.
- `cargo test --test integration tmux_harness_docs` — 5 passed, including
  `the_superseded_scenario_parser_is_absent` and
  `every_shipped_tmux_scenario_is_strict_schema_1`.
- Corpus checks: all scenario JSON parses; zero historical key spellings remain.
- Exact-head `cargo xtask ci` evidence is recorded below.

### S9 documentation convergence

- `architecture.md` gains a normative **Action Registry** section stating the
  one-resolution invariant (exactly one of Dispatch/Unavailable/ForwardToPty/
  Unbound), the prohibition on any second dispatch/help/footer table, atomic
  candidate publication with `KEY-E401`, protected-binding guarantees, and the
  registry's dependency direction. The old "global-shortcut seam" bullet, which
  described the pre-registry early-return shortcuts, is replaced by the
  resolution seam, and the DAG table gains the two pure projection modules.
- `display-and-ui.md` replaces the hand-maintained per-mode hint-string list
  with the snapshot projection contract, documents that unavailable actions stay
  visible with one shared reason, and states exactly what terminal capture
  intercepts versus forwards byte-for-byte.
- `testing-and-quality.md` documents the bidirectional inventory completeness
  gate (and that deleting a row is not how you fix it), platform translation
  capture, and separate mouse/PTY parity assertions.
- `tmux-harness.md` now documents a canonical-only key grammar and the four
  independently observable capture fields.
- `technical-overview.md` documents composition, availability, dispatch, and the
  offline explain flow with its exit codes.

### S9 defect found by the full suite: a non-discriminating scenario wait

The first two full runs failed, and neither failure was noise.

1. `tests/ui/dashboard_reorder_tui.rs` embedded its scenario inline in Rust and
   still used `Space`, `Down`, and `C-q`. Deleting the translator correctly made
   it `HAR-E001: unknown key 'Space'`. Converted to canonical keys; the stale
   comment claiming spellings are "translated by the runner" was removed.

2. `keys-editor.json` step 27 waited for the modal's bottom border
   (`╰────…`) after cancelling the dirty-close prompt. That literal is present in
   the prompt frame too, so under coverage-instrumented full-suite load the wait
   could be satisfied while the prompt was still open; the following `down` then
   landed on the prompt (which does not handle it) and the run stalled until
   timeout. It passed in isolation, which is exactly how this class of race
   hides.

   Fixed by waiting on `actions.run-down`, a row the prompt overlay pushes out
   of the frame and which therefore only appears once the prompt has closed.
   This is a real synchronization fix, not a timeout increase: no `timeout_ms`
   was raised and no retry was added.

### S9 exact-head verification

`cargo xtask ci` — PASS end to end on the candidate head:

- `fmt`, `check-clippy-allows`, `check-source-size`, `check-architecture` — PASS.
- strict `lint` and the `complexity` pass — PASS, zero warnings.
- `coverage` — PASS at **70.83%** line coverage (floor 30%).
- `build` (locked, all features) — PASS.
- `test` (workspace, all features, locked) — PASS, zero failures, including all
  27 real-process schema-1 harness fixtures and both doctests.

No dependency, workflow, quality-gate, `.llxprt`, lint-suppression, or threshold
change was made in this slice.

## Review cycle 1 — findings and triage

Reviews run on the S9 head: one Rust architecture/code review and one local
Open Code Review (`ocr --from origin/main --to HEAD`, 177 files, 66 comments).
Counters after this cycle: local OCR **1/2**, post-PR OCR 0/2, review cycles
**1/2**.

The Rust review reported 0 blocking findings and confirmed the accepted
invariants directly against source: clean `domain/` boundary, one resolution per
input with no surviving parallel dispatch/help/footer authority, atomic
candidate publication with prior bytes retained on rejection, protected-binding
enforcement, byte-faithful PTY passthrough, no production `unwrap`/`expect`/
`panic`/`unsafe`, typed errors, behavioral tests, and no dependency, workflow,
lint, or threshold change.

### Blocker-Fix

| Finding | Evidence | Resolution |
| --- | --- | --- |
| `ModalState::Keys` derived the context `global`, which repeats the stack's own tail; `ContextStack` rejects duplicates, so opening the Keys editor made context derivation fail. The editor deliberately lets `Ctrl+Q` fall through, so the **protected emergency exit was swallowed** and replaced with a warning. | Reproduced by a new RED test: `DuplicateContext(ContextId("global"))` on every screen mode | The Keys modal now derives its own `keys` context. `keys_modal_context_keeps_the_protected_exit_reachable` covers all seven screen modes and is GREEN. |
| `record_untranslatable` labelled dropped input as `ForwardToPty` with zero bytes, so the capture claimed a PTY write that never happened — corrupting the exact evidence CW03-08 exists to provide. | `src/action_capture_emit.rs` | Non-forwarded input now records `Unbound`; only genuinely forwarded input records `ForwardToPty` plus its bytes. |

### In-scope-Fix

| Finding | Evidence | Resolution |
| --- | --- | --- |
| `compact_digit_run` split chord labels on the last **byte** via `split_at`, which panics when a label ends in a multi-byte scalar (a char-key override can produce one). | `src/action_projection.rs` | Splits on the last character and parses the digit through `char::to_digit`, so a non-ASCII label returns `None` instead of panicking. |
| Footer status de-duplication used `part.contains(&status)`, a substring test, so a status could be suppressed by an unrelated longer one. | `src/action_projection.rs` | Compares for equality. |
| Two absence assertions in `agent-shell-overlay.json` sampled the frame immediately after the key that closes the overlay — the same instantaneous-assert race the keys-editor fixture hit. | scenario steps 26–29 and 36–39 | Each assertion now runs after the adjacent waits that already prove the overlay closed. No new literal was invented and no timeout was raised. |

### Reject

| Finding | Why |
| --- | --- |
| PR filter maps `FilterPreviousChoice` and `FilterNextChoice` to the same forward event, "breaking" reverse cycling | Not a regression: `origin/main` maps `CycleNext` and `CyclePrevious` to the same forward event too. CW03-01 requires behavioral parity, so changing it here would be an unrequested behavior change. Recorded as a follow-up instead. |
| `workspace_stack` drops focused/screen levels while a special editor/chooser is active | Matches current behavior, where an open property editor/chooser consumes input before panel and screen handling. Adding those levels would newly expose bindings that do not fire today. |
| `is_quit_key` guard "weakens a quit safety net" | Intentional: `Ctrl+Q` is the registry's protected emergency exit, and the rapid `q q q` sequence is a separate mechanism. Feeding the exit chord into the sequence counter would double-handle it. |
| Chord parsing fails for two or more modifiers plus a literal `+` | Not reproducible: `parse` strips the `++` suffix before splitting modifiers, so `Ctrl+Alt++` parses. The keymap suite covers the grammar. |

### Defer

- ~23 further scenarios assert frame absence immediately after a key press. The
  class is real, but those scenarios are not executed by CI or the test suite,
  and each fix needs a destination literal that cannot be verified without
  running them. The two automatically-exercised cases were fixed. The durable
  answer is a bounded "wait until absent" operation, which is harness-grammar
  scope rather than this issue.
- Perf items (quadratic lookups in validation/resolution, repeated document
  parsing in `apply_edits`, snapshot cloning per render) are real but concern
  small, bounded collections and are outside the accepted behavior.
- Style, doc-wording, and temp-directory-cleanup-on-failure suggestions in
  tests.

One-shot conversion and scan scripts used during this work were run from a
scratch directory and deliberately not committed: conversion tooling is
dev-time, never a shipped input mode.

### Review-cycle-1 verification and one unrelated flake

`cargo xtask ci` on the remediated head — PASS end to end: fmt, clippy-allow
policy, source size, architecture, strict lint, complexity, coverage at
**70.82%**, locked all-feature build, and the full workspace test suite with all
27 real-process harness fixtures.

Two clippy errors introduced by the remediation itself (`items_after_statements`
for the new char-split helper, and `manual_contains`) were fixed at the source by
lifting the helper to module scope and using `Vec::contains`. No allow attribute
or threshold change was added.

One failure in this cycle was **not** caused by this branch:
`v1/config-path-precedence.json` asserts five keys are visible in a 100x30
frame, but the printed JSON contains absolute temp paths whose length varies per
harness run, so the last key (`themes`) intermittently wraps off the bottom of
the frame. The fixture is unchanged since #419 and had passed in earlier runs on
this same branch. Rather than dismiss it as unrelated, the viewport was widened
to 60 rows so the whole document fits regardless of path length; every `contains`
and `absent` assertion is unchanged, and the fixture now passes repeatedly.

### Main integration: converting the issue #530 Windows scenario

Merging current `origin/main` brought in
`dev-docs/tmux-scenarios/issue530/windows-agent-working-directory.json`, authored
from a base predating this branch and therefore still in the old format. The
corpus gate correctly failed on it.

Before touching it, its relevance was checked against a current Windows
`state.json` supplied by the maintainer. `Alt+5` resolves by `shortcut-slot`,
not list position, and slot 5 is `branch-3` at
`C:\Users\acoli\projects\jefe\branch-3` — so the scenario's `Alt+5` -> wait
`branch-3` sequence still matches the live setup exactly. It is current
evidence, not a stale artifact, so deleting it was rejected.

It was converted to strict schema 1 following the precedent S8 already
established for its sibling native-Windows scenario
(`issue525/windows-npm-wrapper-launch.json`): same contained `config/` +
`work/` workspace, same launch, and the identical step sequence with canonical
keys (`Alt+5`, `l`, `Ctrl+C`, `F12`, `Ctrl+Q`). The old screen-`capture` step
was dropped, matching how every other converted scenario handled it, because
schema-1 `capture` denotes subprocess capture rather than a screen artifact.

### Two defects inherited from main, fixed rather than dismissed

Merging `origin/main` at `09e1c9f` brought in two failures that this branch did
not cause. Both were proved pre-existing with a clean worktree of unmodified
`origin/main`, and both were fixed here because the PR cannot be green
otherwise.

1. `tests/issue382_behavior.rs` was 1,028 lines on main, over the 1,000-line
   source-size hard limit. Issue #534's two new definition tests were moved into
   the existing `tests/issue382/agent_probe_runtime.rs` probe submodule, where
   they sit beside the other probe coverage. The parent target returns to 993
   lines; no test was dropped and no threshold was touched.
2. `probe_parser_four_agents` failed on unmodified `origin/main`: #534 made the
   LLxprt capability probe trusted, so it no longer spends a second `--help`
   invocation, but the shared fixture assertion still demanded
   `["--version", "--help"]` for all four agents. The assertion now derives the
   expected invocations from the probe's `trusted` flag, so trusted definitions
   are asserted to probe once and untrusted ones twice — which is the behavior
   #534 intended.

### Final exact-head verification

`cargo xtask ci` — PASS across all nine stages on the merged head: fmt,
clippy-allow policy, source size, architecture, strict lint, complexity,
coverage at **70.82%**, locked all-feature build, and the full workspace test
suite including every real-process schema-1 harness fixture and both doctests.

### CI-only failure: a title assertion competing with the startup warning banner

`real_jefe_session_uses_isolated_config_when_binary_available` passed locally and
on `origin/main`'s runner but failed twice on this PR's runner. The captured
frame showed the app had started and rendered its dashboard normally; only the
title was truncated, because the top row carries both the title and the startup
warning banner, and on that runner an optional startup probe failed
(`could not reconcile shell windows: capability probe failed: tmux list-`). The
banner took the width and left `LLxprt` without `Jefe`. `wait_for_screen_literal`
returns the last capture after its 10-second deadline, so the assertion then ran
against a frame whose title had never appeared.

Neither the status bar nor the startup reconcile path is touched by this branch,
so the banner's presence is a property of the host, not of this change. The test
name and intent are about the isolated config being used and the app rendering,
which is why the assertion now waits on `Agent Types` in the dashboard body
instead of a title that any startup warning can displace. Nothing was skipped,
weakened, or given a longer timeout.
