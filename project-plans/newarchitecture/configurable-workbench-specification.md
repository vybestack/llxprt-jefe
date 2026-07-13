# Configurable Jefe Workbench Specification

## 1. Normative status and product promise

This is the normative configurable-workbench v1 product and architecture specification. `configurable-workbench-public-contracts.md` supplies the exact serialized/wire/parser/limit contracts and is incorporated in full. `configurable-workbench-issue-roadmap.md` supplies delivery and test ownership. A contradiction among them blocks release; no issue may locally weaken a contract.

After restart, users can discover, explain, create, resume, restore, and use capability-compatible LLxprt, Code Puppy, Codex CLI, and Claude Code agents locally or remotely; send fresh Issue/PR work through the same typed operation path; remap actions; compose supported host-rendered screens; edit one lossless master configuration; install and explicitly trust plugins; and recover without data loss. The four shipped agents are a binding first slice. Capability mappings for Codex and Claude Code must be pinned and verified; unverified mappings are omitted, but Claude Code is never removed.

V1 excludes hot reload/self-restart, scripting/shell/setup commands/raw agent args, arbitrary plugin UI/drawing/input/PTY/state/effects, workflow language, event bus/general queue, dynamic libraries, cross-screen relationships, same-kind fan-out, route reuse, persisted navigation/provider models, marketplace/signatures/sandbox security claims, automatic update/restart loops, Windows, and quality-gate exceptions.

## 2. Architecture and authoritative responsibility

Dependencies point inward: domain/contracts are I/O-free; application reducers depend on them; persistence/runtime/GitHub/SSH/tmux/provider are adapters; pure projections are iocraft-free; UI renders snapshots and emits typed intent; composition root wires concrete ports. UI does not call adapters, state owns no process handles, runtime owns no application state, public DTOs import no UI/private message type, and registries are immutable after transactional publication.

The public-contract responsibility map is authoritative even when implementations are split into cohesive files. File layout may vary, but a second validator, geometry source, action availability calculation, command assembler, provider state owner, path selector, or persistence writer is forbidden.

The minimal state/effect contract is a foundation, not late hardening: reducers commit state and bounded follow-ups, release state access, then execute closed typed effects carrying correlation/owner/generation/semantic key. Completions echo those identities and stale results are ignored. Only bounded idempotent queries retry. There is no event bus or general queue. The later hardening issue only audits this contract.

## 3. Startup, publication, and recovery

Normal startup order is:

1. resolve selected/legacy paths and physical identities;
2. parse the lossless Settings document and State schema 1/2;
3. migrate state in memory/import only under the exact decision table;
4. load compiled core/GitHub descriptors and four shipped agent definitions;
5. enumerate local definitions and ordered platform plugin roots, physically deduplicating aliases;
6. resolve exact plugin versions and parse all selected static manifests/schemas/resources;
7. merge defaults/user policy with provenance and retain unknown-owner data dormant;
8. validate IDs, owners, bounds, agent definitions, actions, screens, routes, ports, relationships, keys, recovery reachability, binary selection, and owner config;
9. start **persistent providers only**, sorted by plugin ID, and require Hello/Configure/Ready from all;
10. create initial screen instances/subscriptions and atomically publish one registry/effective config.

One-shot providers execute no startup process and cannot gate publication. They perform a fresh spawn/Hello/Configure/Ready/action/terminal/shutdown/exit cycle per invocation. Any selected static or required persistent-provider failure stops/reaps candidates and publishes nothing. No enabled content is silently skipped, disabled, substituted, or partly registered.

Provider-free CLI commands use the exhaustive syntax/result table in public contracts. They start neither TUI nor providers; probes run only when explicitly requested. Malformed/unsupported selected state or path ambiguity prevents normal TUI, preserves every byte, and prints exact recovery commands. Valid state containing unknown type/owner records opens normal TUI with unavailable dormant records. Offline validation states that provider semantic validation was not run.

## 4. Persistence, settings, and state

Settings schema 2, State schema 2, complete schema-1 migrations, current platform path migration, runtime signatures, unknown/dormant behavior, numeric/nesting bounds, lossless merge, physical identity, and transaction phases are exactly public-contract section 3. No reader silently writes. Settings draft and State snapshot writers are independent and revisioned; state coalesces to the newest full snapshot. Hash conflict retains disk and draft. An atomic phase failure leaves the prior authority intact and retryable.

Settings merge is compiled defaults, selected plugin defaults in ID/version order, user definitions in canonical-path order, sparse settings. Missing inherits; maps recursively merge except defined atomic subtrees; scalars/lists replace; bindings replace by context/action and `[]` unbinds; reset removes syntax; layouts replace whole trees. Unknown active known-owner fields fail. Unknown owners remain dormant and lossless and do not publish.

The Settings UI edits this same document through typed drafts. Common shell owns section navigation, dirty/hash/save/discard/Save-and-Exit, validation summary, reversible theme preview, restart messaging, conflict/write recovery, and Save/Discard/Cancel guard. Structural saves never mutate active registries and v1 never self-executes. Secret fields persist references and display only set/unset.

## 5. Four-agent generic capability architecture

The sole path is:

```text
AgentDefinition -> CandidateResolver -> ProbeParser -> CapabilitySet
 -> Operation/Target Resolver -> Preflight -> AgentLaunchPlan -> local/remote adapter
```

All exact contracts are public-contract section 4. Product knowledge exists only in shipped definitions, pinned fixtures, and the named legacy LLxprt adapter. Generic domain, forms, state, persistence, Issue/PR orchestration, and runtime contain no product ID/name branch.

Resolver behavior includes repository-local `.llxprt/bin/llxprt` before PATH for LLxprt, both locally and in the canonical remote workdir. Every candidate is canonicalized/fingerprinted. The exact stream/framing/parser/duplicate-key/UTF-8/size/time/error rules produce Compatible, Incompatible, NotFound, or ProbeError. Capability extraction is deterministic and sorted.

Operations are normal, resume, fresh-issue, and fresh-pr; targets are local and remote. Every definition explicitly marks each combination supported or supplies a reason. Unsupported controls stay visible but disabled with the same reason used by Help/explain; invoking performs zero preparation or spawn. This type-driven matrix replaces product branches.

Local and remote plans preserve typed argv/environment boundaries. Remote probes are read-only, effective-user probes and require no local installation. Transport/auth/effective-user failure is ProbeError, absent executable is NotFound, and missing capability is Incompatible. One audited SSH serializer is used.

Preflight is a closed typed plan. LLxprt sandbox preflight resolves/fingerprints engine, verifies an already-present image using fixed argv (no build/pull), checks declared environment names, and returns Ready/Unavailable before repository mutation/spawn. Code Puppy does not inherit stale LLxprt fields. Environment begins empty and only the exact allowlist/typed emitters enter it; tmux variables are scrubbed. Generation checks precede clone/reset/prompt write/preflight/spawn.

Shipped fixtures pin successful local and supported remote normal/resume/fresh Issue/fresh PR transcripts. Codex and Claude unsupported operations remain explicit if not verified. Migration maps current LLxprt and Code Puppy values/signatures exactly and preserves unknowns dormant. Restoration reconciles runtime liveness and never invents a live process.

## 6. Actions, descriptors, screens, layout, relationships, navigation

One action registry and availability resolver drive dispatch, Help, hints, menus, Settings, and explain. The complete current source-derived behavior/default inventory is frozen before cutover, including terminal capture, global/modal/editor behavior, rapid emergency behavior, mouse paths, Option/Alt, Ctrl-Q/Ctrl-C, F12, and footer omissions. Configurable bindings are single chords with the exact grammar/precedence/shadow/unbind/conflict contract. Protected recovery is host policy and remains reachable on macOS/Linux and tiny terminals.

CW-04 introduces the **sole internal** screen/panel descriptor and layout contract and converts all five shipped screens to it. CW-05 only parses external screen/manifest declarations and lowers them to that contract; it does not add a second representation. `ResolvedLayout` alone controls rendering, mouse, selection, focus, viewport, and PTY dimensions. Allocation follows the exact public pseudocode, including fixed-first, weighted remainder in declaration order, clamp/redistribute, collapse ordering, first-required tiny fallback, and nonzero PTY content.

Screen definitions declare activation, route, panel instances, focus, Split/Leaf layout, relationships and bindings. Plugin manifests fully declare action/panel/route/screen bindings before execution. Ownership and duplicate policy is exact; provider handshakes cannot register undeclared behavior.

Ports use exact versioned DTO identity. Scope, MasterDetail and SessionTarget are same-screen, acyclic, bounded reducer follow-ups with one target controller and no same-kind source fan-out. They never move focus. Navigation Push/Replace/Back validates before mutation, uses fresh instances, max depth 32, and no persistence/reuse. Back applies the exact local unwind precedence before stack navigation. Dirty exits all use one host Save/Discard/Cancel policy.

## 7. Plugins, trust, packages, and declarations

The approved dependency decision record described in public contracts is a required CW-09 entry artifact, not an implementation note. It records exact versions or standard-library boundaries, approval, rationale, and tests before RED implementation/Cargo changes.

Package roots and alias/dedup/ambiguity rules are exact and platform-ordered. Package-manager roots are read-only; only user root is writable. Install is static, atomic and disabled unless explicit `--enable`; no provider is executed. Versions are side-by-side; enable/switch/rollback selects an exact version after static validation; enabled versions cannot be removed; there is no update command.

Enabling a provider means trusting it to execute as the OS user; there is no sandbox claim. Settings displays Installed-disabled, Enabled-version, Incompatible, Broken, Unsupported Platform, and Ambiguous with reasons. Enable is a draft trust decision. Save performs static validation and records exact selection/config, but does not execute it; semantic validation occurs at next persistent startup or per one-shot invocation. Dormant plugin syntax survives disable/uninstall.

Manifest action, panel, route and screen declarations, screen bindings, argument/activation/event schemas, outcomes, timeouts, confirmation policy, model kinds, ports and handlers are exhaustive and closed as specified publicly. Plugins cannot add arbitrary controls to Settings or screens.

## 8. Provider protocol and host confirmation

Provider JSONL envelopes, complete payloads, state machines, bounds, environment/secrets, cancellation, cleanup, and outcome validation are exactly public-contract sections 7–9. stdout is protocol; stderr is bounded/redacted. Runtime owns process groups and handles; application state owns only health/generation/request DTOs.

One-shot and persistent modes share protocol messages but not startup semantics. One-shot is configured and readied per invocation, returns one terminal result, then shuts down/exits. Persistent alone can gate registry publication, remains Ready for action/panel cycles, and requires explicit Retry/new generation after failure. No restart loop exists.

Host-confirm continuation is two invocations. A provider's terminal confirmation request causes the host modal; cancel executes nothing further; confirm creates a fresh typed continuation request bound to owner/action/context/generation and single-use ID. A one-shot provider is restarted/reconfigured for continuation. The provider cannot self-assert confirmation.

Closed outcomes translate at host adapters to private messages/effects only after ownership/generation/declaration validation. No provider emits internal messages, commands, shell, URL launch, persistence, PTY operations, renderer objects, or arbitrary effects.

## 9. Host-rendered panel lifecycle and plugin config migration

Persistent providers may back only the exact List, Detail, Form, Status, Progress, Empty and Error snapshot models. Snapshots are bounded full replacements with generation and monotonically increasing revision. Events are closed semantic DTOs from declared schemas. The host exclusively owns controls, rendering, focus, input translation, wrapping, scrolling, selection repair, confirmations, links, theme and accessibility.

Panel lifecycle and host-local state are exact: Declared/Activating/Active/Suspended/Failed/Disposing/Disposed. Suspension deactivates subscriptions and retains bounded local focus/scroll/selection/form draft; snapshots/provider state never persist. Invalid models produce a host error/retry view; stale last model may remain visibly stale in memory. Retry creates a generation.

Plugin config uses host-generated fields from constrained schemas. Resolved secrets enter only owning Configure and never diagnostics/effective output/state/models. Migration executes only in a bounded candidate pre-Configure lifecycle, validates target schema, and presents redacted diff. Approve atomically changes exact version/config; Cancel/failure leaves old values and provider-free disable/rollback available.

## 10. UI state requirements

Every user-visible issue must implement and fixture its roadmap matrix. Applicable states are normal, focused, unavailable/disabled, validation/protocol error, tiny geometry, dirty modal, stale/retry, and provider-free recovery. No state is color-only. Focus is visible and keyboard reachable; modals trap and restore focus; errors are adjacent and summarized; disabled controls display reasons; cell width/clipping/wrapping is deterministic and grapheme-safe; protected exit remains visible at reduced geometry.

Status projections use exact common authorities: agent enablement × availability; action binding × availability; screen enabled × composition; plugin installed/selected/platform/static/provider state; panel lifecycle/revision. Views never infer status from labels or process handles.

## 11. Testing, compatibility, and release

Development is RED -> GREEN -> REFACTOR. Every criterion is singular: one trigger/state and one observable response. Every criterion maps explicitly to a named scenario, test family, and fixture in the roadmap; tests cite the ID. UI scenarios pin config/state/HOME/PATH/package/provider roots, platform and dimensions and use typed file/env/mode/process/resize/restart operations with bounded literal waits—never arbitrary shell or a production test hook.

Unit tests cover pure parser/merge/migration/layout/graph/resolver/reducer/state-machine logic. Contract/golden tests cover all tables and all four shipped pinned products. Integration tests cover atomic files, executable/SSH/tmux boundaries, archives, framing/backpressure/process cleanup, one-shot zero-startup behavior, persistent startup rollback, and installed roots. Property tests cover containment, deterministic merge/layout, argv preservation, migration idempotence and bounded propagation. Every nested/resource bound has at-limit and limit-plus-one fixtures with owning diagnostic.

The compatibility kit is owner-created incrementally; final release only aggregates it. It includes Settings/State schema 1→2, current action inventory, five shipped screens, custom relationships/navigation, four agents including Claude Code, package roots/Cellar-link dedup, manifests/declarations, every provider payload/state, panel models/events, migration, malformed/unknown/bounds/redaction/cleanup, and Homebrew/Linux installed layouts.

All existing repository quality gates remain unchanged: formatting, clippy/allow policy with `-D warnings`, architecture/source-size checks, locked all-feature build/tests, and coverage floor. No lint suppression, threshold change, test weakening, unapproved dependency, unresolved contract contradiction, special-case product/plugin branch, leaked secret, unreaped provider, or inaccessible recovery path may release.
