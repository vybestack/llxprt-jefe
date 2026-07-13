# Configurable Workbench: Epic and Implementation Issues

## Status, execution rules, and epic

This is the implementation-ready configurable-workbench v1 roadmap. Product/public behavior is exactly `configurable-workbench-specification.md` and `configurable-workbench-public-contracts.md`; this document assigns delivery, RED-first proof, and ownership. `MUST`/`MUST NOT` are binding.

**CW-EPIC outcome:** after restart, users can use capability-compatible LLxprt, Code Puppy, Codex CLI, and Claude Code through one definition architecture; explain/edit actions, screens and settings; install/trust provider plugins; and recover without silent data loss. Generic layers gain no product/plugin branch.

Every issue follows RED -> GREEN -> REFACTOR, creates every named fixture/scenario before implementation, and records a failing run. One EARS ID has one trigger/state and one observable response. Every ID below maps to a scenario, test family and fixture; tests cite IDs. Harness scenarios pin config/state/HOME/PATH/package/provider roots, dimensions and platform, use typed file/env/mode/process/resize/restart operations and bounded literal waits, and contain no arbitrary shell or production test hook.

All existing gates remain unchanged: formatting, clippy/allow policy with `-D warnings`, architecture/source-size checks, locked all-feature build/tests, and coverage floor. No suppressions, threshold changes, test weakening, speculative scripting/shell, hot reload, event bus/general queue, arbitrary plugin UI, workflow language, or unapproved dependency.

## Acyclic issue DAG

Arrows point to prerequisites:

```text
CW-00 harness
CW-01 config/state + minimal effects -> CW-00
CW-02 four-agent cutover -> CW-00 + CW-01
CW-03 actions/keymaps -> CW-00 + CW-01
CW-04 sole descriptors/layout + shipped parity -> CW-00 + CW-03
CW-05 external screens/relationships lowering -> CW-01 + CW-03 + CW-04
CW-06 navigation/dirty lifecycle -> CW-03 + CW-04 + CW-05
CW-07 Settings shell -> CW-01 + CW-04 + CW-06
CW-08 core registry editors -> CW-02 + CW-03 + CW-05 + CW-07
CW-09 package/dependency/manifest inventory -> CW-01 + CW-07
CW-10 provider actions -> CW-03 + CW-09
CW-11 provider panels/config migration -> CW-05 + CW-06 + CW-07 + CW-09 + CW-10
CW-12 Git Merger -> CW-08 + CW-11
CW-13 ownership/effect audit -> CW-02 + CW-06 + CW-10 + CW-11
CW-14 author kit -> CW-02 + CW-03 + CW-05 + CW-09 + CW-10 + CW-11
CW-15 release -> CW-08 + CW-12 + CW-13 + CW-14
```

CW-01 introduces the closed correlation/commit-before-execute contract. CW-13 is audit/hardening only and cannot introduce a store, queue, bus, or competing effect model. CW-04 introduces the sole internal descriptor/layout model; CW-05 lowers external definitions into it.

## Authoritative responsibility map

| Issue | Sole introduced authority | Compliant file splits allowed; forbidden duplicate |
|---|---|---|
| CW-00 | harness schema/runner | parser/driver/report files; no production hook |
| CW-01 | paths, Settings/State schemas/migrations, lossless document, writers, minimal closed effects | cohesive config/persistence/application files; no UI/startup copy |
| CW-02 | agent definitions/probes/plans/status | parser/probe/planner/runtime adapters; no product branch |
| CW-03 | actions/availability/bindings | parser/resolver/projections; no Help/footer maps |
| CW-04 | internal descriptor and ResolvedLayout | descriptors/resolver/adapters; no controller geometry |
| CW-05 | external screen parser/lowering/relationship policy | parser/compiler/graph; no second descriptor |
| CW-06 | route/navigation/dirty lifecycle | contracts/reducer; no panel-owned navigation |
| CW-07 | Settings shell/draft | state/projection/UI; persistence remains CW-01 |
| CW-08 | host editors | separate section presenters; validators stay owners |
| CW-09 | dependency artifact/package/manifest selection | inventory/parser/install UI; no process execution |
| CW-10 | provider supervisor/action protocol | framing/process/outcome adapters; no state process handles |
| CW-11 | panel DTO lifecycle/config migration | model/event/config modules; host rendering only |
| CW-12 | reference package | package may split files; no host special case |
| CW-13 | audit/guards only | no new authority |
| CW-14 | compatibility runner/docs | cannot alter owner fixtures |
| CW-15 | aggregation/packaging | cannot invent contracts |

## UI matrix legend

Each user-visible issue includes a matrix. `N` normal, `F` focused, `U` unavailable/disabled, `E` error, `T` tiny, `D` dirty, `R` recovery/stale. Each marked state requires a golden assertion within the mapped scenario: visible focus, keyboard reachability, no color-only meaning, adjacent+summary validation, disabled reason, modal trap/restore, grapheme-safe clipping, and protected exit as applicable.

---

## CW-00 — Deterministic real-process TUI harness

**Value/scope:** Add typed create-file/directory, exact bytes/mode/env, executable capture, bounded stdin/stdout/stderr, resize, restart and mutation steps in a mode-0700 contained workspace. Capture argv as elements, allowlisted env, cwd, frames, exit/signal and invocation number. Reject traversal/symlink escape, duplicate path, unsupported mode, unbounded operation and arbitrary commands. This changes harness only.

| ID | Singular EARS response | Scenario | Test family | Fixture |
|---|---|---|---|---|
| CW00-01 | WHEN valid schema parses, produce deterministic workspace plan | `harness-file-process-capture.json` | `harness-schema` | `valid-all-steps.json` |
| CW00-02 | WHEN fixture executes, capture exact argv/env/cwd/frames | same | `capture-boundary` | `capture-executable` |
| CW00-03 | WHEN terminal resizes, capture redraw at dimensions | `harness-resize-restart.json` | `real-tty-delivery` | `100x30-70x18` |
| CW00-04 | WHEN app restarts, retain only durable artifacts | same | `restart-workspace` | `durability-map.json` |
| CW00-05 | IF path escapes physically, reject before launch | `harness-path-containment.json` | `containment-property` | `symlink-traversal-tree` |
| CW00-06 | IF operation exceeds bound, terminate and identify step | `harness-timeout-redaction.json` | `capture-bounds` | `hanging-executable` |
| CW00-07 | WHEN report emits, redact declared secrets | same | `report-redaction` | `secret-streams` |

Failure owner is runner validator/process owner; workspace/report is retained; rerun starts a clean invocation. Done when all capabilities are documented, old scenarios remain compatible, and gates pass.

---

## CW-01 — Exact configuration/state migration, offline recovery, and minimal closed effects

**Value/scope:** Implement public-contract sections 2, 3 and 9: exact platform/path/package-root inputs needed by config, Settings schema 2, State schema 2, complete schema-1 migrations including runtime signatures and dormant records, lossless merge/provenance, revisioned atomic writers, exhaustive recovery CLI, malformed-state behavior, diagnostic ownership/bounds, and the foundational closed effect/correlation/commit-before-execute contract. No agent/screen/plugin semantics or Settings UI.

The effect contract is part of this predecessor: reducer commits bounded transitions, releases state, then typed persistence/probe/runtime/GitHub/SSH-tmux/provider/clipboard-URL/timer effects execute with correlation/owner/generation/key. No event bus/queue. State/path migration fixtures include physical aliases, multiple legacy files, malformed target, invalid indices, unknown kinds/fields, live-session reconciliation, runtime signature hashes and idempotence.

| ID | Singular EARS response | Scenario | Test family | Fixture |
|---|---|---|---|---|
| CW01-01 | WHEN paths resolve, apply exact precedence and physical provenance | `config-path-precedence.json` | `path-resolution` | `mac-linux-path-table` |
| CW01-02 | WHEN one valid legacy source exists and target absent, import atomically and retain source | `config-legacy-state-import.json` | `state-path-decision` | `schema1-single-source` |
| CW01-03 | IF distinct target/legacy or multiple sources exist, exit 3 without merge | `config-state-path-conflict.json` | `physical-dedup-property` | `distinct-byte-equal-and-different` |
| CW01-04 | WHEN aliases identify one file, deduplicate once | `config-physical-alias-dedup.json` | `physical-dedup-property` | `symlink-inode-tree` |
| CW01-05 | IF selected state is malformed, preserve it and block normal TUI with recovery syntax | `config-malformed-state-recovery.json` | `state-parser` | `truncated-state.json` |
| CW01-06 | WHEN schema-1 migrates, produce exact schema-2 entities/signatures/dormant records | `state-schema1-schema2.json` | `migration-golden` | `all-current-fields-unknown-live.json` |
| CW01-07 | WHEN migration repeats, output remains semantically identical | same | `migration-idempotence-property` | `schema2-first-output.json` |
| CW01-08 | WHEN schema-1 settings load, preserve effective theme and unknown syntax without write | `settings-schema1-lossless.json` | `lossless-migration` | `comments-dormant.toml` |
| CW01-09 | IF disk hash changes, preserve disk and draft and emit `CFG-E007` | `config-lossless-external-edit.json` | `document-patch` | `external-edit.toml` |
| CW01-10 | IF any write phase fails, original authority remains and Retry is exposed | `config-write-phase-failure.json` | `revision-writer` | `stage-sync-rename-parent-sync` |
| CW01-11 | WHEN recovery CLI runs, execute zero providers/TUI processes | `config-provider-free.json` | `cli-syntax-exit-table` | `hanging-provider` |
| CW01-12 | WHEN reducer emits effect, execute only after committed/released state | `effect-after-commit.json` | `closed-effect-reducer` | `correlation-transcript` |
| CW01-13 | IF completion identity is stale, leave current state unchanged | `effect-stale-correlation.json` | `generation-property` | `old-correlation.json` |
| CW01-14 | IF nested/aggregate limit+1 occurs, diagnostic belongs to parsing boundary | `config-nested-bounds.json` | `all-resource-bounds` | `depth17-map257-array1025` |

No TUI matrix (commands only); CLI output has normal/warning/error/malformed tables. Done when every CLI syntax/exit row, schema field, migration decision, limit and diagnostic golden passes provider-free.

---

## CW-02 — Complete vertical four-agent definition cutover

**Value/non-splittable end state:** LLxprt, Code Puppy, capability-verified Codex CLI and capability-verified Claude Code are shipped peers through one registry from resolver/probe/status/enablement, generated repository/agent forms, normal/resume/fresh-issue/fresh-pr, local/remote targets, sandbox preflight/environment, persistence/signatures/restoration, tmux and Issue/PR Send. Claude Code MUST NOT be removed. Generic code has no product branches; only definitions, pinned fixtures and named legacy LLxprt adapter contain product knowledge.

Before RED, commit reproducible release SHA-256, exact raw probe streams, executable fixture, parsed identity/capabilities, supported/unsupported operation-target matrix, exact argv/env/cwd, remote transcript and fresh prompts for all four. Freeze current LLxprt/Code Puppy behavior: repository-local LLxprt first; continue/quick-resume fresh rules; LLxprt image/engine/env preflight; tmux scrub/input; Code Puppy model/yolo; remote setup legacy payload. Unverified Codex/Claude options are explicitly unsupported, not guessed.

| ID | Singular EARS response | Scenario | Test family | Fixture |
|---|---|---|---|---|
| CW02-01 | WHEN candidate resolves, retain exact canonical path/fingerprint/generation | `agent-resolver-order.json` | `candidate-resolver` | `repo-llxprt-path-symlink` |
| CW02-02 | WHEN exact probe stream parses, produce sorted identity/capabilities | `agent-probe-parser.json` | `probe-parser-table` | `four-raw-streams` |
| CW02-03 | IF probe timeout/framing/UTF8/duplicate/truncation fails, classify ProbeError | same | `probe-negative-bounds` | `probe-malformed-set` |
| CW02-04 | IF identity exists without required capability, classify Incompatible | `agent-definition-discovery-status.json` | `availability-product` | `four-missing-capability` |
| CW02-05 | WHEN Agent Types opens, show enablement × availability for every definition | same | `status-projection` | `all-status-cartesian` |
| CW02-06 | WHEN supported local operation plans, preserve exact argv/env/cwd | `agent-definition-create-all-shipped.json` | `plan-golden` | `four-local-operation-matrix` |
| CW02-07 | WHEN supported remote operation plans, use effective remote resolver/serializer | `agent-definition-remote-matrix.json` | `remote-contract` | `absence-auth-incompatible-effective-user` |
| CW02-08 | IF operation/target unsupported, disable it with exact reason and zero prep/spawn | `agent-unsupported-operation-ui.json` | `operation-target-matrix` | `four-support-matrices` |
| CW02-09 | WHEN LLxprt sandbox enabled, verify engine/image/env before mutation | `agent-sandbox-preflight.json` | `preflight-state-machine` | `ready-image-missing-env-engine-change` |
| CW02-10 | WHEN Issue Send confirms, deliver exact fixture-authorized fresh Issue plan | `agent-definition-issue-pr-prompts.json` | `fresh-prompt-ordering` | `four-issue-prompts` |
| CW02-11 | WHEN PR Send confirms, deliver exact fixture-authorized fresh PR plan | same | `fresh-prompt-ordering` | `four-pr-prompts` |
| CW02-12 | IF definition/probe/target changes, perform zero prep/spawn and request reprobe | `agent-definition-stale-generation.json` | `generation-property` | `mutated-executable-target` |
| CW02-13 | WHEN compatible persisted agent restarts, restore by stable signature and liveness | `agent-definition-legacy-migration.json` | `restore-migration` | `schema1-four-types-live-dead` |
| CW02-14 | WHEN terminal attaches, preserve input/resize/scroll/detach/kill/relaunch parity | `agent-definition-terminal-compatibility.json` | `local-remote-tmux` | `four-terminal-transcripts` |
| CW02-15 | WHEN architecture guard runs, reject product branches outside approved locations | `agent-no-product-branches.json` | `architecture-guard` | `forbidden-symbol-patterns` |

UI matrix:

| Surface | N | F | U | E | T | D | R | Scenario |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| Agent Types | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ | `agent-definition-discovery-status.json`, `agent-definition-focused-error-tiny.json` |
| New Agent | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | `agent-definition-create-all-shipped.json`, `agent-unsupported-operation-ui.json` |
| Issue/PR Send | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | `agent-definition-issue-pr-prompts.json`, stale scenario |
| Terminal | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ | terminal compatibility scenario |

Failure owner: registry/probe/planner/runtime according to boundary; values remain durable; retry increments generation; stale output is ignored. Done only when all four products pass verified subsets and no generic product branch remains.

---

## CW-03 — Action registry, exact current inventory, single-chord keymaps

**Scope:** Freeze every current resolver/footer/Help/modal/editor/mouse/global/terminal/AppEvent behavior, including Ctrl-Q, rapid `qqq`, F12/`t`, Alt/Option 1–9, scrollback/text controls/Ctrl-C. Implement one action/availability/binding resolver, exact chord grammar, context precedence, explicit parent shadow, list replacement/`[]`, conflict policy, protected recovery, provider-free explain. Handler keys are typed composition keys, not service closures.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW03-01 | WHEN frozen inventory runs, dispatch every recorded default | `current-action-default-parity.json` | `every-current-default` | `current-action-default-bindings-v1` |
| CW03-02 | WHEN chord arrives, resolve at most one candidate by context order | `contextual-keymap-override.json` | `context-resolution` | `parent-shadow.toml` |
| CW03-03 | WHEN availability changes, dispatch/Help/hints project one result | `keymap-help-hint-consistency.json` | `projection-golden` | `available-unavailable` |
| CW03-04 | IF user conflict/implicit shadow occurs, fail `KEY-E401` | `keymap-conflict-startup.json` | `conflict-validator` | `all-conflict-classes` |
| CW03-05 | WHEN unbind/reset occurs, omit or inherit exactly | `keymap-unbind-reset.json` | `merge-roundtrip` | `unbind-reset.toml` |
| CW03-06 | WHEN explain runs, report normalized resolution/provenance provider-free | `binding-explain-flow.json` | `explain-cli` | `alias-context-cases` |
| CW03-07 | WHILE terminal capture active, preserve Leave/Emergency paths | `terminal-protected-recovery-macos.json`, `terminal-protected-recovery-linux.json` | `real-tty-protected` | `platform-events` |
| CW03-08 | WHILE terminal capture active, forward ordinary/Ctrl-C parity | same | `terminal-input-parity` | `capture-inputs` |

UI matrix: Keys/Help surfaces require N/F/U/E/T/D/R in `keys-focused-error-tiny.json`, conflict/unbind scenarios; terminal requires N/F/U/E/T/R in platform scenarios. Registry validation owns error; source remains durable and unpublished until correction/restart.

---

## CW-04 — Sole internal screen descriptors and unified layout parity

**Scope:** Introduce the one internal `ScreenDescriptor`/`PanelDescriptor`/`LayoutNode` contract and convert Dashboard, repositories/Split, Issues, Pull Requests and Actions. Introduce sole `ResolvedLayout`, exact allocation pseudocode, screen-instance/panel-state keys and screen-enum migration. No external syntax, relationships, navigation stack or Settings.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW04-01 | WHEN no override exists, instantiate five screens with parity | `shipped-screen-definition-parity.json` | `five-screen-golden` | `compiled-descriptors-v1` |
| CW04-02 | WHEN focus moves, follow declared order | `definition-dashboard-focus.json`, `definition-split-focus.json`, `definition-issues-focus.json`, `definition-pr-focus.json`, `definition-actions-focus.json` | `focus-repair` | five focus fixtures |
| CW04-03 | WHEN geometry changes, every consumer uses one snapshot | `unified-layout-resize.json` | `render-hit-selection-pty` | `geometry-snapshots` |
| CW04-04 | WHEN allocation clamps/remainders/collapses, follow exact pseudocode | `unified-layout-allocation.json` | `allocation-property` | `fixed-weight-min-max-collapse` |
| CW04-05 | IF required minima fail, show first required focus panel and notice | `shipped-screen-tiny-recovery.json` | `tiny-fallback` | `1x1-through-80x24` |
| CW04-06 | WHEN PTY resolves, never request zero content | same | `nonzero-pty-property` | `terminal-leaf-bounds` |

UI matrix: each five-screen fixture covers N/F/T; parity scenario covers U/E/R where existing screen supports it. Tiny scenario asserts protected Back. Layout resolver owns failure; persisted screen ID stays durable; no duplicate geometry survives.

---

## CW-05 — External custom screens lowered to descriptors and typed relationships

**Scope:** Parse complete `local.*` `screen_schema=1`, activation, panel/action/route references, ownership/duplicates, focus/layout/bindings and Scope/MasterDetail/SessionTarget. Lower once into CW-04 descriptors. Implement exact graph/cardinality/empty/retention/activation/bounds policy through bounded reducer follow-ups; no second layout/runtime model.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW05-01 | WHEN valid inactive screen is enabled, lower/compose it in order | `custom-screen-enable-order.json` | `screen-parser-lowering` | `local-review.toml` |
| CW05-02 | IF inactive screen invalid, warn and omit | `custom-screen-inactive-invalid.json` | `active-policy` | `unknown-field-panel` |
| CW05-03 | IF active screen invalid, reject publication | `custom-screen-active-invalid.json` | `registry-transaction` | `ownership-duplicate-reference` |
| CW05-04 | WHEN optional layout does not fit, collapse deterministically | `custom-screen-tiny-layout.json` | `external-layout-parity` | `review-tiny.toml` |
| CW05-05 | WHEN immediate source changes, propagate in same bounded transition | `relationships-master-detail-immediate.json` | `relationship-reducer` | `pr-detail-ports` |
| CW05-06 | WHEN explicit source changes, wait for activation action | `relationships-master-detail-explicit.json` | `relationship-reducer` | `explicit-activation` |
| CW05-07 | WHEN retained source becomes None, retain last target | `relationships-empty-retain.json` | `empty-policy` | `delete-refresh-cases` |
| CW05-08 | IF graph/type/cardinality/scope/bound fails, reject registry | `relationships-invalid-fanout.json`, `relationships-cycle-type-errors.json` | `graph-property` | `all-invalid-graphs` |

UI matrix: custom screen N/F/U/E/T/R through the eight scenarios; no independent dirty state. Composer owns failures; syntax durable, no partial publication.

---

## CW-06 — Typed routes, local unwind, navigation and dirty lifecycle

**Scope:** Implement declared activation schemas, Push/Replace/Back, max 32, exact local unwind precedence, subscriptions, instance restoration, stale activation rejection, and shared Clean/Dirty Save/Discard/Cancel. No reuse/persisted stack.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW06-01 | WHEN Push succeeds, suspend and create fresh target | `typed-navigation-back.json` | `route-reducer` | `push-activation` |
| CW06-02 | WHEN Replace succeeds, dispose old only after validation | same | `atomic-navigation` | `replace-failure` |
| CW06-03 | WHEN Back reaches stack, restore exact prior instance | same | `instance-restore` | `two-instance-state` |
| CW06-04 | WHILE dirty, complete Save/Discard/Cancel before mutation | `navigation-dirty-guard.json` | `dirty-transition` | `save-discard-cancel` |
| CW06-05 | WHEN Back has local UI, unwind first exact layer only | `navigation-local-unwind.json` | `unwind-precedence-table` | `all-layers-stacked` |
| CW06-06 | IF depth would exceed 32, notice and no mutation | `navigation-depth-limit.json` | `stack-bound-property` | `depth32-33` |
| CW06-07 | IF completion names stale instance/activation, ignore it | `navigation-stale-completion.json` | `generation-property` | `suspended-disposed-results` |

UI matrix: navigation N/F/U/E/T/D/R in `navigation-focused-dirty-tiny.json` plus scenarios. Reducer owns failures; domain data unchanged; retries retain current instance.

---

## CW-07 — Core Settings shell and lossless draft UI

**Scope:** Introduce `core.settings` General/Appearance/Diagnostics and common section navigation, draft/hash/save/discard/Save-and-Exit, reversible preview, restart text, conflict/write recovery. Consume CW-01 persistence; do not implement agent/screen/key/plugin editors.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW07-01 | WHILE structural draft unsaved, leave active registries unchanged | `settings-general-appearance.json` | `draft-reducer` | `structural-draft` |
| CW07-02 | WHEN preview cancels, restore exact prior theme | same | `preview-token` | `theme-preview` |
| CW07-03 | WHEN Settings renders, show focus and diagnostics | `settings-diagnostics.json` | `settings-projection` | `diagnostic-list` |
| CW07-04 | WHEN Back is dirty, trap Save/Discard/Cancel | `settings-dirty-back.json` | `dirty-modal` | `modal-focus` |
| CW07-05 | WHEN valid matching hash saves, patch edited paths and show restart | `settings-lossless-save.json` | `whole-candidate-save` | `comments-order-dormant.toml` |
| CW07-06 | IF hash differs, retain disk/draft and refuse overwrite | `settings-external-edit.json` | `hash-conflict` | `two-versions` |
| CW07-07 | IF write fails, retain draft/active state and expose Retry | `settings-write-retry.json` | `write-revision` | `phase-failures` |

UI matrix: Settings shell N/F/U/E/T/D/R in `settings-shell-focused-error-tiny.json` and mapped scenarios. Settings owns draft state, persistence owns writes.

---

## CW-08 — Agent Types, Screens/Layout and Keys editors

**Scope:** Integrate existing registries into Settings through typed presenter/edit intents; validators remain CW-02/03/05. Agent enablement applies on restart; screens enable/order and whole-layout replacement; Keys exact capture/conflict/reset. No Plugins.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW08-01 | WHEN agent enablement saves, preserve status and apply after restart | `settings-agent-types-integration.json` | `editor-integration` | `agent-statuses.toml` |
| CW08-02 | WHEN screen order saves, include enabled screens exactly once | `settings-screens-order.json` | `screen-editor` | `screen-order.toml` |
| CW08-03 | WHEN key remap saves, dispatch after restart | `settings-keys-remap.json` | `key-editor` | `remap.toml` |
| CW08-04 | IF capture conflicts, show owner/context and block save | `settings-keys-conflict.json` | `key-editor` | `conflict.toml` |
| CW08-05 | WHEN Reset chosen, remove override and show provenance | `settings-keys-unbind-reset.json` | `reset-lossless` | `unbind-reset.toml` |
| CW08-06 | WHEN protected binding selected, render read-only reason | `settings-keys-protected.json` | `protected-editor` | `protected-actions` |
| CW08-07 | WHEN editor tiny, keep focus/error/recovery reachable | `settings-registry-focused-error-tiny.json` | `editor-goldens` | `all-editor-states` |

UI matrix: all three editors N/F/U/E/T/D/R via listed scenarios. Draft is durable in memory; active registries remain unchanged until restart.

---

## CW-09 — Dependency approval, package roots, manifest declarations, inventory and trust

**Entry artifact before RED:** commit approved decision containing path/date/approver, semver/tar.gz/schema-regex/process-group/framing need, exact crate/version or std boundary, license/security rationale and tests. No Cargo edit before approval.

**Scope:** Implement exact ordered roots, canonical physical dedup/ambiguity, package/archive transaction/modes/limits, exhaustive manifest action/panel/route/screen declarations and screen bindings, config schema, ownership/unknown rules, static Settings inventory, explicit unsandboxed trust, side-by-side exact versions. Execute zero providers. Install defaults disabled; enable/switch/rollback statically save exact version; enabled removal rejected; no update command.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW09-01 | WHEN roots scan, list every exact version in declared root order | `plugin-installed-inventory.json` | `roots-installed-layout` | `mac-linux-root-trees` |
| CW09-02 | WHEN archive installs, atomically normalize into user root | `plugin-install-enable-restart.json` | `adversarial-archive` | `valid-package.tar.gz` |
| CW09-03 | WHEN install lacks enable, leave disabled | same | `selection-policy` | `disabled-default` |
| CW09-04 | WHEN Enable saves, persist exact version/config with zero execution | same | `static-enable` | `hanging-provider-package` |
| CW09-05 | WHEN aliases identify one package, deduplicate first physical occurrence | `plugin-cellar-link-dedup.json` | `physical-dedup-property` | `cellar-prefix-alias` |
| CW09-06 | IF distinct package shares ID/version, mark `PLG-E501` | `plugin-package-ambiguity.json` | `selection-determinism` | `two-physical-packages` |
| CW09-07 | IF unselected package broken/unsupported, list reason without blocking | `plugin-broken-unsupported.json` | `manifest-negative` | `schema9-no-triple` |
| CW09-08 | WHEN rollback saves installed version, select next restart | `plugin-version-rollback.json` | `version-selection` | `v1-v2` |
| CW09-09 | IF removal targets enabled version, change nothing | `plugin-remove-enabled-rejected.json` | `remove-transaction` | `enabled-v1` |
| CW09-10 | WHEN declarations parse, bind every action/panel/route/screen exactly once | `plugin-manifest-declarations.json` | `manifest-contract` | `all-declarations.json` |
| CW09-11 | IF unknown/owner/duplicate/bound invalid, reject selected manifest | same | `manifest-negative-limits` | `declaration-negative-set` |

UI matrix: Plugins N/F/U/E/T/D/R in `plugin-static-settings-focused-error-tiny.json` plus scenarios. Package inventory owns failure; installed bytes/selection remain durable; no generation exists.

---

## CW-10 — One-shot/persistent action provider lifecycle and supervision

**Scope:** Implement exact envelopes/payloads/state machines/environment, one-shot zero-startup/per-invocation lifecycle, persistent-only startup gate, action progress/terminal, host-confirm two-invocation continuation, cancellation, outcome validation, process-group drain/escalation/reap, no restart loop, Retry generation and offline zero-spawn. Manifest remains authority.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW10-01 | WHEN startup composes one-shot actions, publish with zero process invocation | `provider-oneshot-zero-startup.json` | `startup-mode-contract` | `one-shot-capture` |
| CW10-02 | WHEN one-shot invoked, run Configure/Ready/action/terminal/shutdown/exit | `provider-oneshot-invocation.json` | `protocol-state-machine` | `one-shot-transcript` |
| CW10-03 | WHEN persistent startup succeeds, require Hello then Configure then Ready | `provider-persistent-handshake.json` | `protocol-state-machine` | `persistent-transcript` |
| CW10-04 | IF required persistent provider fails, reap candidates and publish nothing | same | `startup-rollback` | `second-provider-fails` |
| CW10-05 | WHEN action runs, accept bounded ordered progress and one terminal | `provider-action-progress-outcome.json` | `request-reducer` | `all-terminal-types` |
| CW10-06 | WHEN confirmation requested, bind single-use ID and show host modal | `provider-host-confirm-continuation.json` | `confirmation-state-machine` | `request-a-confirm` |
| CW10-07 | WHEN host confirms, invoke fresh typed continuation; on cancel invoke none | same | `confirmation-state-machine` | `confirm-and-cancel` |
| CW10-08 | WHEN cancel races terminal, first terminal wins | `provider-cancel-race.json` | `race-reducer` | `both-orderings` |
| CW10-09 | IF crash/EOF occurs, mark action unavailable without host crash | `provider-crash-retry-generation.json` | `supervisor-failure` | `crash-phases` |
| CW10-10 | WHEN Retry occurs, increment generation and reject old output | same | `generation-property` | `late-old-lines` |
| CW10-11 | IF framing/order/rate/size invalid, report `PLG-E502` | `provider-protocol-bounds.json` | `protocol-negative` | `every-payload-and-bound` |
| CW10-12 | WHEN shutdown bounds expire, escalate and reap group | `provider-process-group-cleanup.json` | `process-cleanup` | `child-grandchild-hang` |
| CW10-13 | WHEN recovery CLI runs, invoke zero providers | `broken-provider-recovery-cli.json` | `offline-zero-spawn` | `hanging-provider` |
| CW10-14 | WHEN action unavailable, Help/hints use shared reason | `plugin-action-help-hint.json` | `action-projection` | `provider-unavailable` |

UI matrix: action/progress/confirmation N/F/U/E/T/D/R in `provider-action-error-recovery-tiny.json`, confirmation and crash scenarios. Supervisor owns runtime failure; exact selection/config durable; state contains no handles.

---

## CW-11 — Persistent host-rendered panels and plugin config migration

**Scope:** Implement every exact snapshot/model/event schema, declaration binding, panel state machine, host-local state, suspension/disposal/retry, generated config controls, secret delivery/redaction, semantic validation and migration preview/approve/cancel/failure. Only persistent providers contribute panels. No arbitrary rendering/input/state/effects.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW11-01 | WHEN plugin screen activates, create only declared panel instances | `plugin-contributed-screen-panels.json` | `screen-panel-binding` | `declared-screen.json` |
| CW11-02 | WHEN valid newer snapshot arrives, atomically replace owned full model | same | `snapshot-parser-reducer` | `all-model-kinds.jsonl` |
| CW11-03 | WHEN panel renders, host owns focus/input/wrap/selection/confirmation | same | `host-render-golden` | `all-model-kinds` |
| CW11-04 | WHEN semantic event occurs, send only declared event DTO | `plugin-panel-events.json` | `event-schema` | `all-event-kinds.jsonl` |
| CW11-05 | IF model/revision/size/rate invalid, reject and show host recovery | `plugin-panel-model-bounds.json` | `model-negative-property` | `nested-limit-plus-one` |
| CW11-06 | WHEN screen suspends, deactivate subscription and retain bounded host-local state | `plugin-panel-suspend-resume.json` | `panel-state-machine` | `active-suspend-dispose` |
| CW11-07 | WHEN config renders, generate host controls from schema | `plugin-generated-settings-fields.json` | `config-projection` | `all-field-types.json` |
| CW11-08 | IF config field invalid, show adjacent error and block save | `plugin-settings-visibility-validation.json` | `config-validator` | `bounds-cycle-invalid` |
| CW11-09 | WHEN secret resolves, deliver only owning Configure | `plugin-secret-redaction.json` | `environment-redaction` | `all-observation-surfaces` |
| CW11-10 | WHILE owner disabled/absent, preserve dormant config without schema validation | `plugin-dormant-config.json` | `dormant-roundtrip` | `unknown-owner.toml` |
| CW11-11 | WHEN migration output validates and approved, atomically save target | `plugin-config-migration-approve.json` | `migration-state-machine` | `v2-v3-valid` |
| CW11-12 | WHEN preview cancels, retain exact old selection/config | `plugin-config-migration-cancel.json` | `migration-state-machine` | `cancel-diff` |
| CW11-13 | IF migration fails, retain old values and offline rollback | `plugin-config-migration-failure.json` | `migration-negative` | `wrong-version-secret-bound` |
| CW11-14 | IF outcome undeclared/invalid, execute zero effect | `plugin-invalid-outcome.json` | `outcome-validator` | `all-invalid-outcomes` |

UI matrix: plugin panels/config/migration N/F/U/E/T/D/R in `plugin-panel-focused-error-tiny-recovery.json` and mapped scenarios. Host panel/supervisor/settings draft own failures per public table; provider models never persist.

---

## CW-12 — Exact Git Merger reference package

**Scope:** Ship relocatable `com.example.git-merger` with contextual typed merge request `{request_id,repository_ref,pull_request_ref,strategy,expected_head_oid}`. Host validates current head and owns confirmation; provider may use Git/`gh` internally, emits bounded progress and declared refresh/notice only. No host branch on ID/name.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW12-01 | WHEN enabled/restarted, expose action only in declared PR context | `git-merger-install-config-merge.json` | `package-e2e` | `git-merger-package` |
| CW12-02 | WHEN selected, construct only typed current PR request | same | `request-golden` | `merge-request.json` |
| CW12-03 | WHEN confirmation succeeds, provider emits bounded progress/terminal | same | `provider-transcript` | `confirm-continuation.jsonl` |
| CW12-04 | WHEN merge succeeds, refresh known PR | same | `closed-outcome` | `success-outcome` |
| CW12-05 | IF head differs, fail without success | `git-merger-head-changed.json` | `head-invariant` | `changed-head` |
| CW12-06 | WHEN confirmation cancels, invoke no continuation | `git-merger-cancel.json` | `cancel-capture` | `zero-invocation` |
| CW12-07 | WHEN disabled/restarted, remove contribution but retain dormant syntax | `git-merger-disabled.json` | `dormant-integration` | `disabled-settings` |
| CW12-08 | WHEN artifacts inspected, contain no resolved secret | `git-merger-secret-redaction.json` | `redaction-scan` | `all-artifacts` |

UI matrix: merge action/modal/progress/error N/F/U/E/T/D/R in `git-merger-error-tiny.json` and scenarios. No automatic destructive retry.

---

## CW-13 — Ownership, stale-generation, and effect-order audit only

**Scope:** Audit the already-introduced CW-01 closed effect contract and every ownership/failure table. Add architecture guards for UI I/O, state process handles, effect under borrow/lock, untyped completion, duplicate authority and >64 follow-ups. No new store, effect vocabulary, event bus, queue, retry policy, persistence authority or architecture rewrite.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW13-01 | IF screen completion stale, reducer changes nothing | `stale-completion-cross-screen.json` | `audit-generation` | `old-instance` |
| CW13-02 | IF persistence completion predates pending revision, retain newer | `persistence-newer-revision-wins.json` | `audit-writer` | `revisions-1-2` |
| CW13-03 | IF provider generation stale, reject output | `provider-old-generation-ignored.json` | `audit-supervisor` | `generation-3-4` |
| CW13-04 | IF follow-ups reach 64, stop with owning diagnostic | `bounded-followups.json` | `audit-bound` | `cycle-defense` |
| CW13-05 | WHEN effect executes, state is committed/released | `effect-after-commit.json` | `architecture-guard` | `all-effect-kinds` |
| CW13-06 | WHEN failures aggregate, show owner/retry/durability and exits | `failure-recovery-dashboard-tiny.json` | `failure-matrix-golden` | `all-failure-owners` |

UI matrix applies only to recovery dashboard N/F/U/E/T/R. Done when audit finds no duplicate authority and all earlier owner tables are executable tests.

---

## CW-14 — Authoring schemas and compatibility runner

**Scope:** Publish machine-readable schemas or exhaustive tables, canonical/negative/bound fixtures already owned by introducing issues, four-agent pinned provenance, action inventory, sample local screen/agent/non-executable package, provider transcript checker and installed-layout runner. It cannot alter behavior or defer fixtures.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW14-01 | WHEN custom example validates, compose without undocumented fields | `author-example-custom-workbench.json` | `example-e2e` | `examples/custom-workbench` |
| CW14-02 | WHEN agent runner executes, verify four pinned hashes/transcripts | `compatibility-agent-fixtures.json` | `kit-agent` | `four-agent-index.json` |
| CW14-03 | WHEN package fixture runs, verify package/manifest/config/layout | `compatibility-plugin-package.json` | `kit-package` | `package-index.json` |
| CW14-04 | WHEN provider transcript runs, verify every payload/state/model/outcome | `compatibility-provider-transcripts.json` | `kit-provider` | `transcript-index.json` |
| CW14-05 | WHEN each limit+1 runs, reject with owner diagnostic | `compatibility-negative-limits.json` | `kit-limits` | `limit-index.json` |
| CW14-06 | WHEN static author validation runs, spawn zero providers | `author-provider-free-validation.json` | `kit-offline` | `hanging-provider` |

No TUI matrix; runner output has success/warning/failure goldens. Reports do not mutate source/package/settings.

---

## CW-15 — Final aggregation, installed layouts, and release gate

**Scope:** Aggregate only. Build clean locked artifacts and execute owner-created fixtures for Settings/State 1→2, all bounds/diagnostics, four agents including Claude Code, operation/target/preflight matrices, complete action inventory, sole layout/five screens/custom relationships/navigation, all Settings UI matrices, exact ordered roots and dedup, declarations, one-shot zero-startup, persistent startup rollback, all provider payload/state/model/event/migration failures, Git Merger, and author kit.

| ID | EARS response | Scenario | Test | Fixture |
|---|---|---|---|---|
| CW15-01 | WHEN Homebrew layout installed, dedup Cellar/link and pass kit | `release-clean-homebrew-layout.json` | `installed-release` | `homebrew-cellar-prefix` |
| CW15-02 | WHEN Linux layout installed, resolve ordered roots and pass kit | `release-clean-linux-layout.json` | `installed-release` | `usr-local-usr-share` |
| CW15-03 | WHEN each compatible shipped CLI installed without config, expose verified definition | `release-four-agent-no-config.json` | `installed-four-agent` | `four-cli-bin` |
| CW15-04 | IF normal startup broken, recovery commands remain provider-free | `release-provider-free-recovery.json` | `installed-recovery` | `malformed-state-broken-provider` |
| CW15-05 | WHEN aggregate runner executes, use unchanged owner expectations | `release-contract-aggregation.json` | `fixture-hash-index` | `compatibility-index` |
| CW15-06 | WHEN tiny/terminal capture renders, protected recovery remains reachable | `release-tiny-protected-recovery.json` | `installed-real-tty` | `mac-linux-events` |

UI matrix aggregates every prior marked cell; the release scenario additionally covers N/F/U/E/T/D/R across installed artifacts. Any missing fixture/ID mapping, contradiction, warning, quality failure, unapproved dependency, weakened gate, lint suppression, unverified shipped mapping, product/plugin branch, leaked secret, unreaped provider, one-shot startup invocation, duplicate authority or inaccessible recovery blocks release. CW-15 owns no migration or user data; defects return to the introducing issue.
