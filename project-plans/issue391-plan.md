Branch: `issue391` (exact `origin/main` ancestor `4c2979a4`).

## 1. Outcome

Jefe gains persistent-provider panel subscriptions with closed semantic snapshots
and events, host-owned rendering and interaction, generated plugin Settings
fields, and an explicit config migration transaction that runs before Configure.

`state::provider_panels` is the sole owner of panel lifecycle, instance/generation,
revision, accepted model, rate state, and bounded host-local presentation state.
`domain::plugin_config` is the sole owner of config declaration, value,
constraint, and visibility validation. Settings owns plugin drafts, references,
migration approval, and the existing expected-hash write. The provider runtime
owns process and delivery only.

Panels and panel-local state are never durable. Providers never send iocraft
objects, raw keys or mouse input, host effects, focus/scroll/theme/accessibility
policy, or durable models. One-shot and provider-free packages cannot declare
panels.

## 2. Consumed contracts and entry gate

| Contract | Existing owner | CW-11 use / gap |
|---|---|---|
| Strict provider envelope and framing | `src/runtime/provider/{framing,payload_reader,identifiers,dto,protocol}.rs` | Reuse one parser and envelope. Add closed top-level panel and migration payloads and panel-specific limits; do not add a second parser |
| Persistent process/session ownership | `src/runtime/provider/{persistent,persistent_session,coordinator,candidate,supervisor}.rs` | Reuse the sole process owner. Add asynchronous panel delivery and a provisional pre-Configure migration phase without handles in state |
| Provider request state | `src/state/provider_requests.rs` | Retain action request/progress/terminal/confirmation and health only; do not add panel model ownership |
| Static selected plugin manifests | `src/domain/plugin/*`, `src/persistence/plugin_inventory.rs`, `src/persistence/settings_publish.rs` | Consume selected exact package/provider/panel/config declarations; reject one-shot panels and bind only owner-declared panel types |
| Typed navigation instances/generations | `src/state/navigation.rs::{ScreenInstance,NavOutcome}`, `src/state/navigation_ops.rs` | Forward committed navigation outcomes to the panel reducer; refusals emit no panel effect |
| Descriptor registry and panel relationships | `src/workbench/*` | Bind a plugin declaration only within its contributed screen; never add dynamic plugin IDs to the global built-in panel registry |
| Settings draft and expected-hash writer | `src/state/settings*`, `src/persistence/settings_{edit,document,publish}.rs`, `src/persistence/writer.rs` | Extend one draft/writer for plugin config and migration approval; only matching `WriteOutcome::Authoritative` commits restart-applied target settings |
| Closed post-commit effects | `src/domain/effects.rs`, `src/state/transition.rs`, `src/services/*` | Panel and migration I/O occurs only after reducer commit and outside state borrows |
| Action registry / protected input | `src/domain/action_registry.rs`, `src/app_input/*` | Translate host input to declared semantic panel events through existing resolution; preserve Back/emergency exit |
| Pure host projections and TUI harness | `src/ui/components/*`, `src/harness/*`, `dev-docs/tmux-scenarios/*` | Render all closed body/config states through host primitives and prove visible behavior before implementation |

Parent/sequence context: #391 is the provider-panel/config feature-completion
cutover under epic #379. It directly consumes #385, #386, #387, #389, and #390;
#392 requires its generic panel/config behavior, #393 audits its ownership and
stale-generation rules, and #394 reuses its production parsers and fixtures.

Entry gate: **open**. The initiative's normative public contracts, canonical
issue body, restart-applied Settings rule, and no-shim policy resolve the contract
choices below. Several landed predecessor seams are intentionally intermediate;
their bounded correction is required in this PR rather than preserved behind
aliases or deferred to successors. No dependency, quality-gate, workflow, or new
process subsystem is planned.

## 3. Decisions fixed by initiative context

### D1 — normative panel event schema cutover

Replace the landed intermediate `event_kinds` grammar with the public contract's
`event_schema:[{kind,arguments:[Field;0..128]}]`. Event kinds are unique. Events
without a free typed map declare an empty argument list; `action.arguments` and
`submit.values` validate against their declaration, while `field-changed`
validates against the accepted Form model. IDs and tokens for the remaining event
kinds validate against the complete accepted snapshot. Remove `event_kinds`; do
not accept both grammars or create a general schema language.

### D2 — config field grammar cutover

Make #391 a hard manifest-schema-1 cutover from the landed
`kind/minimum/maximum` grammar to exact
`id,label,description?,type,required,default?,min?,max?,choices?,unique?,visible_when?,restart`.
There is no dual-key compatibility. `visible_when` remains the existing sibling
field ID gate for a present/truthy value and the graph must be acyclic. `unique`
is legal only for `string-list` and defaults false. String/list `min`/`max` are
inclusive lengths; integer/finite-number bounds are inclusive numeric values;
enum choices are unique and nonempty. `domain::plugin_config` becomes the sole
config schema/value/visibility validator. Constrained action/route/event/form
fields may reuse field vocabulary but do not become config validators.

### D3 — package selection, screens, and open navigation

One immutable settings-selected installed package version is the authority for
owner/schema validation, screen contributions, panel declarations, provider
composition, and migration source/target lookup. Selected package screen files
must pass through the existing sole parser/lowerer and compose transactionally.
Navigation accepts the composed open `ScreenIdentity` rather than refusing every
noncompiled target. A plugin panel resolves only within the same selected
package's contributed screen and requires a persistent provider; dynamic plugin
IDs never enter the global built-in panel-type registry.

### D4 — migration eligibility, identity, and restart timing

An enabled selected executable target enters migration when the Settings draft
target manifest's positive config schema version differs from the still-authoritative
prior selected installed manifest's schema version. First install and same-schema
package changes do not migrate. Provider-free packages cannot migrate; one-shot
and persistent binaries may run the provisional transaction, while only persistent
providers may own panels. Remove post-Configure `ConfigMigration` capability.

`from_version`/`to_version` are config schema versions. Request `config` is the
exact prior typed config containing references only; response `config` echoes it
exactly and `target_config` is the proposal. The existing positive `DraftToken`,
owner, process generation, and host-originated request ID must all match. Notes
are bounded display-only values and are not persisted. `migrated-config` is the
direct terminal response, not `Outcome::MigratedConfig`; request-origin validation
therefore depends on message role rather than stream direction.

Structural settings remain restart-applied as required by #379/#387. The
provisional provider performs hello/ack/migrate before the target is saved but is
never published or Configured. Approve uses the existing expected-hash writer;
only the exact matching authoritative completion commits target version/config.
The provisional process is then shut down/reaped, and normal startup Configures
the selected target after restart. Cancel/failure retains prior settings bytes.
The older undeclared `validate-config` table entry is not implemented: #391 has
no response DTO, ordering, or acceptance row for it, and host validation is exact.

### D5 — process, panel, and revision correlation

The envelope generation remains the fixed provider-process generation. Every
panel payload also carries a nested positive `generation` that is the panel
activation generation. Activate, resume, and Retry allocate a fresh panel
generation without requiring process restart; deactivate, event, and snapshot
must match it. `screen_instance_id` is the existing positive `ScreenInstanceId`
JSON u64. `panel_instance_id` is a new positive monotonic, never-reused,
session-only u64 allocated by `provider_panels` after navigation commits.
`panel_type` is the owner-qualified manifest `Id` resolved only in the selected
package/screen binding.

Snapshot size is the original UTF-8 byte length of the snapshot payload JSON,
excluding envelope bytes and LF. HostLocal size is its canonical JSON byte length.
Rate enforcement is a deterministic token bucket per panel instance/generation:
capacity and initial credit 40, refill 20 tokens per second from injected monotonic
elapsed time with carried fractional credit. A well-formed snapshot consumes one
token before model validation. No reducer reads wall-clock time.

## 4. Acceptance matrix

All protocol/model failures are `PLG-E502`. No failure below may partially apply
a model, write settings, Configure/publish a target provider, or persist panel
state unless the row explicitly permits that side effect.

| ID | Actor / launch path | Inputs and boundary cases | Observable success | Observable failure / diagnostic | Permitted side effects before failure | Persistence / compatibility | RED evidence |
|---|---|---|---|---|---|---|---|
| CW11-01 | Navigation activates a selected plugin-contributed screen | Exact owner/package/screen/panel binding; persistent provider only; foreign/unselected/undeclared/provider-free/one-shot declarations | Commit allocates one fresh panel instance per manifest-bound panel, then stages `activate-panel` | Invalid declaration refuses candidate/activation with owner-scoped diagnostic and zero provider delivery | Committed navigation only after complete target construction | Instance/model/lifecycle never persist; hard closed manifest cutover per D1/D2 | manifest and workbench binding matrix plus plugin-panel activation scenario |
| CW11-02 | Provider sends `panel-snapshot` to a live panel | Every body kind; revision 1 then exact +1; full snapshot including empty optional collections | Reducer atomically replaces the complete accepted model and renders the new body | Any invalid candidate leaves prior accepted model byte/field-equivalent and marks lifecycle Failed/stale where available | Framing/read and one rate token | No model persistence | every-kind protocol transcript and atomic replacement reducer table |
| CW11-03 | Protocol/parser and panel reducer validate a snapshot | Exact owner/instance/generation/revision/model schema/kind; 524,288-byte payload; 262,144-byte document; depth/map/array 16/256/1,024; body count N/N+1; rate 20/s burst 40 | Every at-limit value is accepted | Wrong identity, stale/late/gap/duplicate revision, schema/kind/bound/rate/size fails `PLG-E502`, applies no partial model, and may render literal `stale` | Bounded parse/read and rate accounting only | Ephemeral failure/model only | exhaustive negative and inclusive N/N+1 matrices with fake monotonic input |
| CW11-04 | Host action/input adapter sends semantic input | All nine event kinds; event declared by manifest; action/field/submit arguments validate; referenced IDs exist and affordance enabled | Exactly one closed `panel-event` names live instance/current revision | Raw key/mouse, undeclared kind, bad args, unknown/disabled/unafforded ID, stale panel/revision emits no provider or host effect | Host-local focus/selection may change only for a valid host-owned local movement | Events not persisted | event table, encoder round trip, undeclared/schema-invalid zero-effect tests, input scenario |
| CW11-05 | Navigation suspend/restore/replace/back, Retry, Disable, Dispose | Every legal and illegal lifecycle transition; HostLocal 65,536/N+1; late snapshot/event; unavailable process | Suspend sends reason suspend and retains only bounded HostLocal; restore/retry activates fresh generation; replace/dispose sends exact reason when possible and invalidates instance | Invalid transition/oversize HostLocal/late delivery mutates no disposed/current model; failed provider yields recovery state | Exact deactivate/activate delivery after committed navigation; targeted provider cleanup at runtime edge | HostLocal/model/lifecycle discarded on process/app exit | lifecycle state table driven by `NavOutcome`, delivery capture, suspend-resume-dispose scenario |
| CW11-06 | Settings opens selected plugin config | Boolean, string, integer, finite-number, enum, path, string-list, secret-reference; label/description/default/required/min/max/choices/unique/visibility/restart; regular/small geometry | Pure projection emits exact host control and set/unset secret reference state for each visible field | Invalid declaration prevents selected candidate publication; hidden controls do not edit or configure | Draft projection only | Draft persists only through normal explicit Save; references only | all-field projection golden, render test, generated-settings tmux scenario |
| CW11-07 | Settings edit/save/configure availability | Type mismatch, required missing, inclusive bounds, finite decimal, enum membership, duplicate list entry when unique, unknown field, visibility sibling/cycle | Valid candidate clears adjacent error and permits Save; matching saved candidate may Configure | Invalid candidate shows adjacent error plus summary and blocks Save, migration approval, and Configure | In-memory draft edit only | Invalid values are not written for an active owner | `plugin_config` declaration/value/cycle matrix plus reducer/UI blocked-operation tests |
| CW11-08 | Provider composition resolves a secret reference | Exact `{env:EnvName}`; set/unset; owning/foreign provider; migration/diff/export/diagnostic/state/debug observations | Resolved value appears only in owning Configure `secrets`; UI displays env name and set/unset | Missing/invalid reference is typed and redacted; no Configure on invalid active config | Environment lookup at Configure boundary only | Only `{env=...}` reference is durable; resolved bytes never are | sentinel all-observation scan and cross-owner provider transcript |
| CW11-09 | Settings publication sees absent or disabled owner | Arbitrary preserved owner subtree, including values invalid under an installed schema; disable/re-enable; owner later appears | Exact syntax/config survives dormant round trip without owner validation or process start | Enabling/selecting invokes exact schema validation before Save/Configure | Lossless document read and in-memory dormant record only | Exact comments/order/bytes retained until explicit owner edit | dormant absent/disabled round-trip and zero-spawn tests |
| CW11-10 | Selected upgrade needs config migration | Settings draft compares authoritative source and target manifests; provisional hello/ack then migrate before any Configure; exact schema versions, request ID, owner, process generation, draft token, source echo, target config; path-sorted redacted lossless diff; hash-writer outcomes | Valid response presents preview; Approve builds exact target selection/config candidate; matching expected-hash authoritative write commits it; provisional process reaps; restart then performs normal Configure/Ready/publication | Any mismatch/schema/bound/secret/diff/write fault retains prior config/selection and publishes no target | Provisional contained process, bounded delivery, preview; after approval only the normal atomic settings write and targeted reap | Exact target version/config becomes durable only on matching authoritative write and applies after restart | protocol/order transcript, diff golden, approve/write/reap capture, restart Configure scenario |
| CW11-11 | Operator cancels or migration/provider/write fails | Cancel at preview; error/timeout/EOF/wrong identity; conflict/superseded/failed write; provisional persistent or one-shot candidate | Cancel returns to exact prior selected version/config; recovery offers Retry/Disable/offline installed Rollback/provider-free config commands | No target Configure/Ready/publication and provisional process is reaped; typed redacted diagnostic | Provisional spawn/hello/migrate and targeted cleanup only | Prior settings bytes/hash remain authoritative; rollback is explicit selection only | cancellation/failure matrix, byte/hash equality, zero-Configure/publication capture, recovery scenario |
| CW11-12 | Production panel/config UI at all supported platforms/geometries | Normal, focused, unavailable, loading/progress, empty, failed+stale, dirty/migration, recovery, small; keyboard/mouse; Back/emergency exit | Host renders accessible literal state, visible focus, selection repair, wrapping/scroll/links/confirmation; protected actions remain reachable | Unavailable/stale/invalid action has zero provider effect and a visible reason; small layout remains usable | Host-local deterministic state and typed intents only | No visual/runtime panel state persists | component render/projection tests and real tmux scenarios for each distinct state |

## 5. Bounds and lifecycle invariants

- Closed envelope remains `{protocol:1,type,request_id,generation,payload}` and
  rejects unknown/duplicate keys and illegal direction/order.
- Panel snapshot revisions begin at 1 and increase exactly by 1 per panel
  instance/generation. Activate and Retry allocate fresh generations.
- Full candidate snapshots validate before one atomic model replacement. Failure
  may retain only the complete last accepted model, visibly marked `stale`.
- Affordance IDs and list item IDs are unique; selected IDs exist; referenced
  action IDs are declared/available; a disabled affordance has a nonempty reason.
- `total` implies `completed`; `completed <= total`. All count/byte/depth limits
  are inclusive.
- Panel lifecycle is exactly Declared, Activating, Active, Suspended, Failed,
  Disposing, Disposed. A disposed instance never returns.
- Durable data is limited to package selection/config/settings. Panel models,
  lifecycle, revisions, rate state, and HostLocal are session-only.
- Secret references are exact environment names; resolved values are temporary
  Configure-bound values and absent from every observation/export surface.
- Migration startup is provisional and never reaches Configure. The exact matching
  authoritative expected-hash write commits the target settings, the provisional
  process reaps, and normal startup Configures that target only after restart.

## 6. Bounded vertical slices (one branch and one PR)

### Slice A — selected package, screen, and closed manifest cutover

- Rows: CW11-01, declaration half of CW11-04, static inputs to CW11-06/07.
- Owners: `domain::plugin`, plugin inventory/publication, workbench composition,
  and the existing open `ScreenIdentity` navigation authority.
- Expected paths: constrained edits under
  `src/domain/plugin/{field,reader_parts,surface,manifest,mod}.rs`;
  `src/persistence/plugin_inventory.rs`; `src/startup_screens.rs`; focused
  workbench/navigation/render-dispatch/provider-composition tests; first RED tmux
  fixture/scenario edits.
- RED: exact selected version across every consumer, package screen parse/lower,
  open plugin navigation/rendering, `event_schema`, one-shot panel rejection, and
  owner/package/screen binding.
- GREEN: one immutable selected-package authority and one descriptor registry;
  selected persistent panels bind only to their owner's contributed screens.
- Non-goals: no runtime panel model, Settings save, or general navigation rewrite.
- Stop: compatibility requires dual grammar/schema migration or a dynamic global
  panel registry.

### Slice B — plugin config, generated Settings, dormancy, and secrets

- Rows: CW11-06 through CW11-09.
- Owners: new sole `domain::plugin_config` validator, existing Settings
  reducer/candidate/writer, package-aware owner catalog, persistence publication,
  and secret resolution only at provider Configure composition.
- Expected paths: new `src/domain/plugin_config.rs` and tests; focused
  `src/state/settings*`, `src/config_owners.rs`, `src/persistence/settings_*`,
  provider composition/environment tests, Settings projection/components, and the
  generated-settings tmux scenario.
- RED: all eight exact field types, declaration/value/visibility tables, typed
  edits, adjacent errors, blocked Save/Configure, exact `{env:EnvName}` reference,
  dormant round trip, selected-package owner catalog, and sentinel observation scan.
- GREEN: one validator and one Settings draft/writer; only references are durable,
  and only the owning Configure sees resolved secret bytes.
- Non-goals: keychain/store integration, plaintext secrets, general Settings
  redesign, owner validation of dormant data, or a second writer.
- Stop: lossless document replacement or another secret/config authority is needed.

### Slice C — closed panel wire, persistent delivery, and reducer

- Rows: CW11-02 through CW11-05.
- Owners: provider wire boundary, existing persistent owner thread, and new sole
  `state::provider_panels` lifecycle/model/revision/rate/HostLocal reducer.
- Expected paths: `src/runtime/provider/{dto,identifiers,payload_reader,encode,
  lifecycle,protocol,persistent_session,coordinator}.rs` and focused fixtures/tests;
  new `src/state/provider_panels.rs` and tests; smallest typed
  message/effect/navigation-outcome wiring.
- RED: every body/event transcript, message-role request IDs, process versus panel
  generation, inclusive bounds, atomic replacement, lifecycle/rate tables, late
  delivery, and zero-effect invalid event.
- GREEN: one strict parser, one asynchronous lane owned by the existing persistent
  session, and one pure panel reducer; no handles or I/O in state.
- Non-goals: no renderer, config writer, second stdout reader, or competing
  supervisor/channel architecture.
- Stop: delivery requires state-held handles or another effect/message bus.

### Slice D — host projection, rendering, and semantic input

- Rows: CW11-02, CW11-04 through CW11-07, CW11-12.
- Owners: pure projections plus thin `ui::components` renderers and existing
  action/input dispatch.
- Expected paths: new pure provider-panel/plugin-settings projections; all seven
  host body primitives, generated config controls and render tests; focused
  `app_input` routing; named schema-1 tmux scenarios authored before production UI.
- RED: visible scenarios first, all body/control goldens, focus/selection repair,
  scroll/form draft, unavailable/stale/loading/empty/small states, and protected
  Back/emergency-exit behavior.
- GREEN: descriptor-driven host rendering emits only declared semantic events;
  provider DTOs contain no iocraft or raw input.
- Non-goals: provider UI callbacks, alternate keymaps, or model persistence.
- Stop: provider-authored rendering/input or a second action registry is needed.

### Slice E — pre-Configure migration transaction and integration

- Rows: CW11-10, CW11-11, integration closure for CW11-01/CW11-05/CW11-12.
- Owners: existing supervisor/coordinator process boundary plus Settings state and
  exact expected-hash writer completion.
- Expected paths: focused provider candidate/persistent/session/coordinator
  modules and fixtures; typed effects/messages/workers; Settings migration state,
  preview/recovery UI, scenarios, and required standards docs.
- RED: provisional hello→ack→migrate→preview→approval→authoritative-write→reap
  transcript with zero Configure; restart Configure transcript; every identity,
  cancel/failure/write result; redacted sorted lossless diff and recovery scenarios.
- GREEN: provisional process remains runtime-owned and never publishes/Configures;
  matching authoritative write commits restart-applied settings; every failure
  retains exact prior authority and reaps the provisional process.
- Non-goals: automatic rollback/restart, remote package acquisition, general
  process orchestration rewrite, speculative `validate-config`, or successor work.
- Stop: a second supervisor, process handle in state, or migration behavior beyond
  the accepted contract is required.

## 7. Expected path ledger

| Layer | Planned paths | Acceptance rows |
|---|---|---|
| Domain | `src/domain/plugin_config.rs`, tests; hard cutover in `src/domain/plugin/{field,surface,reader_parts,manifest,values,mod}.rs` | 01, 04, 06-09 |
| Static package authority | `src/persistence/plugin_inventory.rs`, `src/startup_screens.rs`, `src/config_owners.rs`, `src/runtime/provider/composition.rs` and focused tests | 01, 06-10 |
| Wire/runtime | `src/runtime/provider/{dto,identifiers,payload_reader,encode,lifecycle,protocol,candidate,persistent,persistent_session,coordinator,supervisor}.rs` and focused tests/fixtures | 02-05, 08, 10-11 |
| State/effects/messages | new `src/state/provider_panels.rs`, tests; focused `navigation`, `settings`, message/effect/worker wiring | 01-07, 09-12 |
| Workbench/navigation | selected package screen discovery plus descriptor/panel binding/open `ScreenIdentity` composition/navigation tests | 01, 05, 12 |
| Persistence | focused plugin inventory and `settings_{edit,document,publish}.rs` tests using the existing lossless writer | 01, 06-11 |
| UI/input | `src/ui/orchestration.rs`, pure projections, host panel body components, generated Settings controls, focused input routing/tests | 01-07, 10-12 |
| Harness | provider fixture extensions and named schema-1 `dev-docs/tmux-scenarios/` scenarios authored before visible implementation | 01, 04-06, 10-12 |
| Documentation | `dev-docs/standards/display-and-ui.md`, `dev-docs/standards/persistence-and-runtime.md`; architecture only if open identity ownership text must be corrected | issue done criteria |
| Planning | `project-plans/issue391-plan.md` | workflow traceability |

No `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifest, quality-gate,
lint threshold, or unrelated test/documentation change is authorized.

## 8. Explicit non-goals

- Panels for one-shot or provider-free packages.
- Provider-authored rendering, iocraft values, raw terminal input, focus/scroll/
  wrap/selection/confirmation/link/theme/accessibility policy, PTY operations, or
  arbitrary host effects.
- Durable panel snapshots, lifecycle, revision, rate, or HostLocal data.
- Automatic provider restart, automatic config mutation, or automatic rollback.
- Remote acquisition/installation of a rollback version.
- A second JSON parser, config validator, Settings writer, action registry,
  message/effect bus, provider registry, supervisor, or process manager.
- General declarative migration language, scriptable visibility, plugin Settings
  UI callbacks, keychain integration, or additional field/body/event kinds.
- Adjacent cleanup, speculative hardening, successor Git Merger behavior, workflow
  changes, dependency changes, or quality-gate weakening.
- Stacked PRs. Slices are coherent internal commits in one issue branch and PR.

## 9. Scope ledger

| Item | Classification | Status |
|---|---|---|
| Closed top-level panel/migration DTO cutover from CW-10 placeholders | Required by issue | Accepted |
| `event_schema` and config/secret grammar hard cutover with all consumer conversion | Required by public contract/no-shim policy | Accepted |
| Exact selected installed package version as sole static authority | Required by CW11-01/06/10 | Accepted |
| Selected package screen parsing/lowering and composed open navigation/rendering | Required by CW11-01/12 and #392 handoff | Accepted |
| Persistent-only panel declaration and same-owner package-screen binding | Required by CW11-01 | Accepted |
| Navigation outcome forwarding to panel reducer | Small direct integration | Accepted |
| Package-aware Settings owner catalog and exact dormant preservation | Required by CW11-06/09 | Accepted |
| Provisional pre-Configure migration inside existing runtime owner, restart-applied | Required by CW11-10/11 and #379/#387 | Accepted |
| Asynchronous post-Ready panel delivery in existing persistent session | Required by CW11-02/04 | Accepted |
| Provider fixture and visible schema-1 tmux scenarios | Mandatory behavioral evidence | Accepted |
| Underspecified `validate-config` table entry | Out of scope | Reject speculative implementation |
| Git Merger/audit/author-kit/release/harness-successor issue work | Out of scope | Defer to #392-#395/#397 |
| Any dependency/gate/workflow/public subsystem not named above | Out of scope | Stop for approval |

## 10. Review counters and finding ledger

- Local Open Code Review: `2 / 2` used; the local cap is exhausted.
- PR Open Code Review: `0 / 2` used.
- Rust/DeepThinker review cycles: `2 / 2` implementation review cycles used.

All findings were classified as Blocker—Fix, In-scope—Fix, Reject, or Defer.
Accepted findings were remediated and focused regressions were added. Rejected
findings were either factually inapplicable to the closed protocol or conflicted
with the accepted no-shim/reap-before-save contract. No finding expanded scope.

## 11. Verification evidence

Latest remediation-tree evidence (to be rerun at the committed exact head):

1. `cargo fmt --all --check` and `git diff --check` pass.
2. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
   passes with no allowance or threshold changes.
3. `cargo build --workspace --all-targets --all-features --locked` passes.
4. All workspace library and binary unit-test targets pass. Segmented integration
   targets pass, including CW-10 76/76 and CW-11 5/5, plus the directly affected
   provider, Settings, package-composition, panel, migration, persistence,
   navigation, projection, renderer, and redaction suites.
5. `package-panel-lifecycle` passes 13/13 and
   `plugin-settings-generated-config` passes 14/14 in isolated real tmux-harness
   workspaces with absolute binary and config paths.
6. The 36-test harness fixture aggregate has 33 passes and only the three failures
   reproduced unchanged at base `4c2979a4`: `issue687` config isolation,
   `issue687` session continuity, and the unclean-prior-run fixture. Prior
   segmented coverage is 63.62%, above the 30% threshold.
7. The full aggregate also encounters a watchdog-visible starvation hang in
   `a_breadcrumb_recorded_while_the_run_heartbeats_survives_the_kill`. The exact
   test hangs identically at base `4c2979a4`; the other 23 issue-662 tests pass in
   the segmented run. Signal 15 from this external watchdog is not a test
   assertion failure and is unrelated to the CW-11 diff.
8. The repository has no Makefile. `xtask ci` ignores trailing `--scenario`
   arguments, so scenario-specific spellings invoke the same aggregate rather
   than the named scenario.
9. Final persistent-owner retirement regressions, persistent-session owner unit
   tests, CW-10, CW-11, package chrome, and provider renderer geometry were rerun
   after the last structural refactor and pass.

## 12. Deferred findings / follow-ups

None. Optional additional body kinds, event kinds, field kinds, persistence,
automatic rollback/restart, remote acquisition, and UI polish remain explicit
non-goals rather than follow-ups unless a concrete accepted need is discovered.
