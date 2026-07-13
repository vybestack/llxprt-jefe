# Epic: Configurable Workbench v1

## Outcome

Deliver a restart-applied, lossless configurable workbench in which users can discover, explain, configure, create, resume, restore, and operate capability-compatible LLxprt, Code Puppy, Codex CLI, and Claude Code agents; remap host actions; compose bounded host-rendered screens; safely install and trust packages; run supervised provider actions/panels; and recover malformed configuration without starting untrusted processes or losing bytes. Extension authors receive versioned, machine-checkable contracts and an offline compatibility runner.

## Self-contained capability DAG

Capability names below are contracts, not GitHub issue-number placeholders. An arrow means the row consumes the named delivered capability.

| Capability | Exact delivered contract | Consumes |
|---|---|---|
| deterministic real-process harness | contained schema-1 filesystem/process/PTY/capture/interpolation/resize/restart/redaction runner plus legacy adapter | none |
| configuration, state, and effects | schema-2 lossless Settings/State migration, path identity, revisioned atomic writer, provider-free recovery, closed post-commit effects | deterministic harness |
| four-agent definitions | declarative LLxprt, Code Puppy, Codex CLI, Claude Code capability/probe/operation/target/preflight/plan contracts | harness; configuration/state/effects |
| actions and keymaps | immutable action/context/availability registry, source-derived defaults, canonical single chords, protected controls | harness; configuration/state/effects |
| descriptors and layout | sole internal screen/panel descriptor and executable resolved-layout snapshot with five-screen parity | harness; actions/keymaps |
| custom screens and relationships | exact local file discovery, parser/lowering, typed same-screen bounded relationship graph | configuration/state/effects; actions/keymaps; descriptors/layout |
| navigation and dirty lifecycle | typed routes/activations/instances/generations, exact Back precedence, Save/Discard/Cancel | actions/keymaps; descriptors/layout; custom screens |
| Settings shell | General/Appearance/Diagnostics lossless draft, SHA-256 conflict, reversible preview, reload/export/retry | configuration/state/effects; descriptors/layout; navigation/dirty |
| registry editors | Agent Types, Screens/Layout, Keys presenters and exact sparse payloads | four agents; actions/keymaps; custom screens; Settings |
| package inventory | ordered physical package roots, selected manifest/archive/trust authority | configuration/state/effects; Settings; separately approved package dependencies |
| provider actions | strict JSONL protocol, one-shot/persistent supervisor, confirmations, bounded outcomes/cleanup | actions/keymaps; package inventory |
| provider panels/config | host-rendered snapshots/events/lifecycle, generated config, secret references, explicit migration | custom screens; navigation; Settings; package inventory; provider actions |
| Git Merger package | relocatable package with exact `gh pr view`/`gh pr merge` destructive flow | registry editors; provider panels/config |
| ownership/effect audit | dependency guards and stale agent/screen/writer/provider generation proofs | configuration/state/effects; navigation; provider actions/panels |
| authoring kit | installed versioned schemas/tables/examples/hashes and production-parser compatibility runner | every owning contract above |
| release aggregation | unchanged owner fixtures on clean Homebrew/Linux layouts, relocation/recovery/security scans | registry editors; Git Merger; ownership audit; authoring kit |

Only package inventory has an unresolved prerequisite: maintainer approval of the exact archive/SemVer/SHA-256 dependency decision. That gate does not block remediation or implementation planning for independent capabilities; provider/package-dependent implementation starts only after the package inventory contract is delivered.

## Current source inventory and target authority

| Current source/symbol | Current responsibility | Target sole authority/parity |
|---|---|---|
| `src/domain/mod.rs::AgentKind` | closed LLxprt/Code Puppy product enum | migration adapter; immutable agent registry owns all definitions |
| `src/agent_detection.rs` | binary detection | definition-driven bounded probes with exact diagnostics |
| `src/state/form_projection.rs`, `form_runtime.rs`, `form_ops.rs`, `form_build.rs` | product-specific forms/runtime | constrained definition fields and typed operation planner |
| `src/runtime/commands.rs` | command assembly | definition operation adapters; no generic shell/raw args |
| `src/app_input/fresh_prompt.rs`, `preflight.rs`, `remote_probe.rs` | product policy | typed operation/target/preflight contracts |
| `src/persistence/mod.rs::{Settings,State}` | schema-1 paths/load/save | lossless schema-2 document/state/path/migration/writer authority |
| `src/state/types.rs::ScreenMode` | closed screens | migration only; descriptors and `NavState` own runtime identity |
| `src/ui/screens/dashboard.rs`, `split.rs`, `issues.rs`, `pull_requests.rs`, `src/actions_view.rs` | screen geometry/control | thin renderers over descriptor/ResolvedLayout snapshots |
| `src/layout.rs`, `src/mouse_routing.rs`, terminal/selection projections | duplicated geometry consumers | one resolver snapshot for render/hit/wrap/select/scroll/focus/PTY |
| `src/input.rs`, `src/app_input/`, `src/app_shell.rs`, Help/footer | distributed key/action behavior | action/key registry and typed intent dispatch |
| `src/harness/`, `dev-docs/tmux-scenarios/` | current real-TTY tests | closed deterministic harness plus behavior-compatible legacy lowering |
| `src/runtime/` | process/tmux adapters | also provider supervisor; never owns application state |
| absent package/provider modules | no plugin runtime | static inventory, strict supervisor, host panel/config reducers |

Dependency direction is pure domain/contracts, deterministic application reducers, then adapters/UI/composition. UI renders and emits typed intent; reducers perform no I/O; persistence owns files but no processes; runtime owns handles but no `AppState`; composition commits/releases state before effects; public DTOs import no UI/private message type. Registries publish atomically and become immutable until restart.

## Global closed grammar, bounds, diagnostics, and errors

IDs are lowercase ASCII 1–128 bytes matching `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`. Reserved namespaces are `core.*`, `github.*`, `local.*`, and `<plugin-id>.*`; plugin IDs have at least two labels and no reserved prefix. Duplicate IDs are fatal even when definitions are byte-identical. Active schemas reject unknown/duplicate fields; unknown owners are dormant only in explicitly lossless owner subtrees.

Inclusive bounds: artifact/manifest/schema/settings/state/provider line 1,048,576 bytes; string/document 262,144; path 4,096; description 4,096; data depth 16; map 256; array 1,024; diagnostics 256 and origins 16; enabled/discovered plugins 32/256; package entries/expanded bytes/path depth/path bytes 4,096/67,108,864/16/1,024; screens/panels/layout depth/split children 64/16/8/8; relationships/follow-ups 64/64; plugin actions 128; effective bindings 2,048 and chords/action-context 8; route/navigation depth 32/32; fields 128; provider requests/outbound/progress 16/64/256; snapshot 524,288; model rate sustained 20/s burst 40; probe output 65,536 per stream and local/remote timeout 5/20 seconds; provider handshake 5 seconds, action 1–600 seconds, shutdown/terminate/kill-drain 2 seconds each.

Diagnostics are `{code,severity,path,span?,owner?,owner_version?,provenance[],correction,redacted_detail}`, sorted error/warning/info then canonical path/span/code. Required families are `CFG-E001` through owner-specific configuration/migration/write errors including `CFG-W004`, `AGT-E201` and definition/probe/plan errors, `SCR-E301`, `NAV-E001`, `KEY-E401`, and `PLG-E501/PLG-E502`. Every failure identifies durable data and exact recovery. Secret values are absent from state, paths, hashes exposed to users, spans, provenance, effective/exported output, captures, logs, panels, diagnostics, and package artifacts.

## Authoritative startup and end-to-end flow

Resolve configured paths and physical identities; bounded-read Settings/State; perform only explicit permitted schema/path migration; load four shipped definitions, compiled core/GitHub descriptors, complete source-derived actions, local definitions/screens, then ordered selected package roots; parse static declarations without executing providers; merge defaults/user policy with provenance and dormant owners; validate ownership/references/bounds/agent capabilities/actions/keys/screens/layouts/routes/relationships/config/secrets; start required persistent providers in plugin-ID order and require all Ready; instantiate runtime state; atomically publish. One-shot providers start zero processes. Any active static or required persistent failure reaps candidates and publishes nothing.

A typed intent contains correlation ID, owner, screen instance/generation, activation generation, and semantic key. Reducer validates and commits state with at most 64 ordered closed effects; state access is released; adapter executes; completion echoes identity; stale completion changes nothing. Only persistence, agent probe/runtime, GitHub, SSH/tmux, provider, clipboard/URL, and timer effects exist. Retry is `Never` or `IdempotentQuery{max_attempts:1..3}`; destructive retries always require new explicit intent. There is no event bus/general queue.

Settings edits remain draft; SHA-256 mismatch preserves disk and exportable draft; structural changes apply after restart. Navigation stack, provider models, panel snapshots, progress, and process handles never persist. Offline recovery commands parse only needed static artifacts and start zero TUI/provider/probe/network processes.

## Migration, compatibility, and recovery matrix

| Source/failure | Required result | Recovery |
|---|---|---|
| Settings schema 1 | retain theme/override effective values and every unknown byte; write only explicitly | Save or `config format --migrate` |
| State schema 1 | stable IDs, mapped product IDs, signatures, dormant unknown records, reconciled liveness | `config migrate-state` |
| path aliases/multiple legacy files | physical aliases deduplicate; distinct sources never merge | inspect `config path`, resolve externally |
| malformed selected file | preserve all bytes; block normal startup; providers zero | `config validate` and exact migrate command |
| five current screens | visual/input/focus/mouse/wrap/selection/scroll/PTY/tiny parity | deterministic fallback and Back |
| existing actions | complete source-derived default inventory unchanged until override | Reset override |
| unknown owner | bytes dormant, no validation/publication/execution | install/enable owner or retain dormant |
| active invalid static contribution | publish nothing | fix/disable offline and restart |
| persistent startup failure | reap all candidate processes; preserve selection/config | Retry/Disable/Rollback offline |
| stale completion | current state unchanged | none; current generation continues |
| hash/write conflict | disk and draft retained | Reload/Export/Retry/Discard |

## V1 exclusions and security

No hot reload/self-restart, scripting/shell/setup templates/raw agent args, arbitrary plugin drawing/input/PTY/state/effects, workflow language, event bus/general queue, dynamic library, cross-screen relationship, same-kind fan-out, route/instance reuse, persisted navigation/provider model, marketplace/signature/sandbox security claim, auto-update/restart loop, Windows support, guessed CLI capability, or quality-gate exception. Provider execution has explicit trust but is not a sandbox. Paths are physically contained where required; child processes use explicit argv and environment; all groups are reaped.

## Distinct aggregate UI states

```text
NORMAL                         FOCUSED
+ Pull Requests ------------+ + Pull Requests ------------+
|  PR 41                    | |>>PR 42                   |
|  PR 42                    | | Detail / Actions focused |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Agent Types --------------+ + Provider -----------------+
| Claude incompatible      | | PLG-E502 bad sequence    |
| reason: capability absent| | [Retry]                  |
+---------------------------+ +---------------------------+
```
```text
DIRTY                          RECOVERY
+ Save changes? ------------+ + Startup blocked ---------+
|>>Save  Discard  Cancel    | | CFG-E103; providers 0   |
+---------------------------+ | config validate command |
                               +---------------------------+
```
```text
SMALL
+Too small------+
|>>Back         |
| F12 terminal  |
| Ctrl-Q Exit   |
+---------------+
```

Every implementing capability owns separate normal/focused/unavailable/error/dirty/recovery/small scenarios or an explicit state-specific N/A rationale. No combined mock satisfies multiple states. Focus/severity/status are textual, modals trap and restore focus, controls remain keyboard reachable, and clipping is grapheme-safe.

## Epic EARS and exhaustive aggregation ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| EPIC-01 | WHEN all delivered capabilities start, Jefe shall publish one immutable workbench through the stated DAG. | clean installed startup and registry inventory |
| EPIC-02 | WHEN valid schema-1 data migrates, Jefe shall preserve effective values, dormant bytes, and stable identity. | Settings/State migration hashes |
| EPIC-03 | IF startup cannot publish safely, Jefe shall preserve durable data and expose provider-free recovery. | malformed state plus hanging provider trap |
| EPIC-04 | WHEN typed work completes, Jefe shall accept only matching owner/generation/semantic completion after commit. | cross-owner stale/effect-order suite |
| EPIC-05 | WHEN four shipped CLIs are available, Jefe shall expose only pinned capability-verified operations. | four-agent provenance/probe/plan matrix |
| EPIC-06 | WHEN screens render at any tested size, all consumers shall use one resolved snapshot. | five-screen and custom-screen geometry suite |
| EPIC-07 | WHEN users edit Settings, Jefe shall retain lossless bytes/draft and apply structural changes only after restart. | hash/reload/export/editor/restart suite |
| EPIC-08 | WHEN a selected trusted provider runs, Jefe shall accept only closed declared bounded protocol data and reap it. | full wire/panel/config/process suite |
| EPIC-09 | WHEN Git Merger is approved, it shall recheck head and run the exact strategy command without automatic retry. | exact child argv and negative invariant matrix |
| EPIC-10 | WHEN author/release verification runs, it shall hash and execute unchanged owner fixtures from installed layouts. | Homebrew/Linux release indexes |
| EPIC-11 | IF any global bound is exceeded by one, Jefe shall fail at the owning boundary without partial publication. | complete at-limit/+1 index |
| EPIC-12 | WHEN all observations are scanned, Jefe shall expose no resolved secret or unreaped process. | artifact/report/log/frame/process scans |

Every capability begins with its named failing unit/property/integration/golden/real-TTY fixtures, implements the smallest owner, then removes obsolete branches only after parity. Harness scenarios use real process boundaries, contained roots, bounded literal synchronization, and no production hooks.

## Normative documentation and done

Update `dev-docs/standards/architecture.md`, `display-and-ui.md`, `persistence-and-runtime.md`, `testing-and-quality.md`, `dev-docs/RULES.md`, author-kit documentation, package README, and installed-user docs with the final owner contracts; generated descriptions never replace normative types/tables. Done requires every DAG capability and ledger row, no duplicate authority/product/plugin branch, no unresolved contract except the package dependency decision before package-inventory implementation, and unchanged `make ci-check`: formatting; no clippy allow; 1,000-line hard/750 warning source gate; all-target/all-feature clippy `-D warnings`; complexity 15 cognitive/60 lines/6 args/3 bools/type 250; line coverage at least 30%; locked all-feature build/tests. No unsafe, production panic/unwrap/expect, lint suppression, threshold increase, arbitrary shell, weak test, or unapproved dependency.