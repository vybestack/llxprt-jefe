# Issue #390 — CW-10: One-shot and persistent action-provider lifecycle

Branch: `issue390` (from current `origin/main` at `8538fdd1`).

## 1. Outcome

Jefe gains a strict, closed JSONL action-provider protocol, a handle-free request
reducer, and a runtime supervisor that solely owns provider processes, process
groups, pipes, environment construction, timeouts, and reaping. One-shot and
persistent lifecycle semantics remain distinct. Provider effects execute only
after the state transition commits and releases state. Provider outcomes are
validated against immutable action declarations and the current owner/context
before the host executes one of the closed operations.

No provider draws UI, emits arbitrary effects, owns navigation, accesses host
state, persists runtime request state, or places a process handle in application
state.

## 2. Consumed contracts and entry gate

| Contract | Owning symbol | Status and CW-10 use |
|---|---|---|
| Closed post-commit effects and exact correlation | `src/domain/effects.rs::{Effect::Provider, ProviderEffect, ProviderResponse, Correlation, IssuedEffect, EffectCompletion}` | Delivered by CW-01; extend the existing closed provider variants rather than add a second effect model |
| Commit, release, then execute | `src/state/transition.rs`, `src/services/effect_executor.rs`, `src/app_input/agent_lifecycle_ops.rs` | Delivered; the provider adapter follows this seam and never runs under a state guard |
| Immutable action/availability authority | `src/domain/action_registry.rs::{ActionRegistrySnapshot, ActionAvailability, Availability, Resolution}` | Delivered by CW-03; all visible availability reasons come from this snapshot |
| Static provider/action declarations | `src/domain/plugin/action.rs::{Action, ActionConfirmation, ActionOutcome}` | Delivered by CW-09; owns contexts, timeout, destructive policy, confirmation mode, handler, and allowed outcomes |
| Package/provider authority | `src/domain/plugin/{manifest.rs,provider.rs,field.rs,values.rs}` | Delivered by CW-09; owns exact package identity/version, selected binary, `ProviderMode`, config schema, secret references, and host triple |
| Physical selected package | `src/runtime/package_runtime.rs`, `src/persistence/plugin_inventory.rs` | Delivered; supervisor resolves the selected contained binary from the immutable inventory |
| Process-group and bounded-capture conventions | `src/runtime/command_capture.rs`, Windows `src/runtime/job_object.rs` and `owner_anchor.rs` | Delivered; CW-10 adds live incremental framing and staged shutdown without introducing a dependency |
| Deterministic process evidence | `src/harness/` and `dev-docs/tmux-scenarios/` | Delivered; CW-10 adds provider fixtures and UI scenarios |

Entry gate: **open**. CW-01, CW-03, and CW-09 are present on current main. No
contract shim and no new dependency is permitted.

## 3. Decisions fixed by the issue contract

1. **Secret value source.** A `secret-reference` is exactly an environment name.
   The supervisor resolves only manifest-declared references from the host
   process environment, places the resolved value only in the owning Configure
   payload, and does not inherit the host environment. An environment binding is
   populated only when the declaration explicitly names that secret binding.
   No secret store or dependency is introduced.
2. **Closed Outcome vocabulary.** The wire DTO recognizes all seven Outcome
   kinds named by the issue. CW-10 executes only an outcome declared by the
   action and valid for the current owner/context. Panel and config-migration
   outcomes therefore parse as closed values but fail ownership/context
   validation until their owning later slice exists.
3. **Persistent publication.** Persistent candidate startup and all-or-nothing
   publication are explicit CW10-03/CW10-04 requirements and are in scope. The
   orchestration belongs at the existing startup composition edge and delegates
   all handles to the supervisor; it must not create a competing registry or
   process manager.
4. **Effect shape.** Extend `ProviderEffect` and `ProviderResponse`, and use the
   existing typed completion path. Progress is a typed provider message; no
   generic JSON event, queue, bus, or second effect family is introduced.
5. **Delivery shape.** The slices below are coherent internal commits in one
   issue branch and one PR. They are not stacked PRs.

## 4. Acceptance matrix

All protocol failures below use `PLG-E502`. Selection and configuration remain
durable after runtime failure; provider request/progress/confirmation state is
session-only and has no migration.

| ID | Actor / launch path | Inputs and boundaries | Observable success | Observable failure and diagnostic | Side effects permitted before failure | Persistence / compatibility | RED evidence |
|---|---|---|---|---|---|---|---|
| CW10-01 | Startup composition with an enabled `ProviderMode::OneShot` package | Any valid selected one-shot manifest; local and platform-selected binary | Static action metadata publishes and process capture remains zero | Static validation/selection failure leaves action unavailable with its shared reason; no spawn | None | Exact selected package/config retained; no provider model persisted | startup executable trap and `provider-oneshot-zero-startup` scenario |
| CW10-02 | Action dispatch through post-commit provider effect | Fresh positive generation; selected local binary; hello/ack/configure/ready; invoke; 0..256 progress; one terminal; shutdown/ack/EOF | Exact fresh lifecycle transcript and process reaped | Any crash, EOF, timeout, wrong ordering, or framing error marks only that generation unavailable with typed diagnostic | One contained spawn and bounded drains, followed by full reap | Package selection/config retained; request state not persisted | Rust fixture binary and exact transcript/process capture |
| CW10-03 | Startup composition with required persistent providers | Providers sorted by plugin ID; each handshake stage <=5 s; Ready capabilities subset of declaration | Every required candidate is Ready before one atomic registry publication | Any candidate failure routes to CW10-04 | Candidate processes may exist before publication | Previous published snapshot retained until commit | ordered two-provider startup transcript |
| CW10-04 | Persistent candidate startup failure | Spawn, hello-ack, configure, ready, timeout, or capability mismatch for any required candidate | All candidate processes and descendants stop/reap; nothing publishes | `PLG-E502` for protocol failure or typed unavailable runtime failure | Candidate spawns only; rollback reaps all | Existing selection/config and previous registry remain | failure fixture for every handshake phase and rollback capture |
| CW10-05 | Pure framing/protocol parser | Exactly one UTF-8 JSON object plus LF; exact envelope; every closed payload and Outcome | Canonical table parses to strongly typed DTOs in legal order | Unknown/missing/extra/wrong-type field, unknown payload, wrong direction/ID/generation/order fails | None | Wire schema 1 only | per-payload canonical table and lifecycle state-machine table |
| CW10-06 | Framing, protocol state machine, request reducer | CRLF, BOM, blank, duplicate key, trailing data, non-UTF-8, >1,048,576 bytes; host/provider ID rules; positive fixed generation; queue 64; concurrent 16 | Every value at its limit succeeds | Every limit+1 or invalid framing/field/direction/generation/order/rate fails the generation with `PLG-E502` | Bounded read/drain only | No partial provider state survives failure | exhaustive negative table including each boundary at N/N+1 |
| CW10-07 | Provider progress reducer | Sequence begins 1, increments exactly 1, max 256; total implies completed; completed<=total; completed/total never decrease | Bounded progress state updates deterministically | Gap, duplicate, decrease, missing completed, completed>total, or event 257 fails generation | None | Progress is session-only | progress property/table tests |
| CW10-08 | Provider continuation plus shared host confirmation modal | Declared provider continuation; owner/action/context/generation-bound ID; 5-minute TTL; exact declared field schema | Confirm consumes ID once and starts fresh invocation B/full one-shot handshake with exact continuation; Cancel starts none | Forged, expired, reused, owner/action/context/generation mismatch, or undeclared destructive policy starts no continuation | Bounded handle-free pending confirmation only | Confirmation is not persisted | two-invocation transcript, cancel capture, expiry/reuse table, TUI scenario |
| CW10-09 | Cancel/terminal reducer | Both event orderings; exactly one outcome/error terminal | First terminal result remains authoritative | Every later byte is `PLG-E502` but cannot replace first result | Best-effort cancel envelope when still live | Session-only | both orderings property test |
| CW10-10 | Explicit Retry | Old generation output races a new positive generation | Retry spawns a fresh generation; all old completions/lines change nothing | Current-generation failure is visible; stale output is ignored | Old generation is reaped before replacement publication | Selection/config retained | stale-line/completion generation property |
| CW10-11 | Supervisor shutdown and host exit | Continuous stdout/stderr drain; shutdown 2 s; stdin close/group terminate 2 s; kill/reap descendants and final drain 2 s; Unix/Windows | Child and grandchild are gone and reaped | Expired stage escalates deterministically; no orphan remains | Bounded termination signals and drain | None | child/grandchild hang fixture with PID liveness capture |
| CW10-12 | Offline recovery/config CLI | Malformed config plus selected hanging provider | Command reports its normal recovery result and starts zero providers | Config diagnostic only; provider executable is untouched | None | Recovery retains/repairs only its documented durable target | executable trap around recovery command |
| CW10-13 | TUI projection and input dispatch | NORMAL, FOCUSED, UNAVAILABLE, ERROR, DIRTY/CONFIRMATION, RECOVERY, SMALL; protected exit; accessible focus | Distinct states render; reason is byte-identical to action-registry availability; focus is visible without colour and trapped/restored | Unavailable action dispatches no effect; tiny viewport remains usable | UI intent messages only | No UI runtime state persisted | TUI harness scenarios created and proven RED before UI implementation plus pure projection tests |
| CW10-14 | Supervisor Configure/environment construction and every provider-owned observation surface | Empty base env; provider dir + system bins PATH; contained HOME/TMPDIR; locale; declared nonsecret values; declared secret references; stderr max 262,144 | Provider receives only allowed names; secret value appears only in owning Configure or explicitly declared secret environment binding | Missing declaration/value is typed and redacted; no secret appears in state/log/stderr/report/diagnostic | Contained directories and bounded redacted capture | Config stores only references, never values | fixture records argv/env/cwd/stdin and scans Configure, env, state, stderr, report, diagnostics |

## 5. Bounds and lifecycle invariants

- Request IDs are `h-` or `p-` plus 6–20 ASCII digits; host/provider direction
  is checked for every payload.
- One process has one fixed positive generation.
- Handshake is exactly hello, hello-ack, configure, ready; each stage has 5 s.
- Invocation timeout is 60 s by default and the manifest range is 1–600 s.
- Maximum active requests is 16; maximum queued outbound envelopes is 64.
- Maximum stderr retained is 262,144 bytes; stdout and stderr drain continuously.
- First terminal wins. Data after terminal is fatal but cannot change that result.
- One-shot starts no process at startup and performs a fresh full lifecycle for
  each invocation, including continuation invocation B.
- Persistent processes start only during candidate startup in plugin-ID order;
  all required candidates reach Ready before publication; no auto-restart.
- Retry is operator-explicit and allocates a new generation.

## 6. Bounded vertical slices

### Slice A — closed protocol and framing

- Rows: CW10-05, CW10-06, protocol half of CW10-07.
- Owner: pure wire boundary under `src/runtime/provider/`; no process or state.
- Allowed paths: `src/runtime/provider/{mod,error,framing,protocol}.rs`, their
  tests, and the module declaration in `src/runtime/mod.rs`.
- RED: canonical payload/order tests, malformed framing table, request-ID and
  progress boundary tests fail because the module/behavior is absent.
- GREEN: exact closed DTOs, duplicate-key rejection, bounded incremental UTF-8
  JSONL framing, lifecycle state machine, and progress validator pass.
- Non-goals: no spawn, reducer, effect wiring, UI, or persistence.
- Verification: focused tests, `cargo xtask quick`.
- Stop: a second JSON/effect architecture or a dependency would be required.

### Slice B — handle-free reducer, confirmations, and UI projection

- Rows: CW10-01, CW10-07 through CW10-10, CW10-13.
- Owner: `src/state/provider_requests.rs` plus typed message/effect contracts;
  pure projection feeds thin UI components.
- Allowed paths: provider request state/reducer tests; `src/domain/effects.rs`;
  typed `src/messages*`; reducer routing; action projection/thin UI; named TUI
  scenarios.
- RED: TUI scenario files are authored first and fail for absent provider states;
  reducer tables fail for progress, confirmation, terminal race, and generation.
- GREEN: reducer owns bounded values only, reuses shared availability reason,
  emits closed effects, and produces all distinct accessible UI projections.
- Non-goals: no process handle, timer handle, direct I/O, or alternate action
  registry.
- Verification: scenarios, focused state/projection tests, `cargo xtask quick`.
- Stop: confirmation requires a scheduler in state or a second message/effect bus.

### Slice C — supervisor, environment, process fixtures

- Rows: CW10-02 through CW10-04, CW10-11, CW10-12, CW10-14.
- Owner: `src/runtime/provider/supervisor.rs`; sole handle/process owner.
- Allowed paths: runtime provider supervisor and contained helpers, existing
  startup composition edge, Rust fixture binary/integration tests, and recovery
  command test seam where needed.
- RED: one-shot and persistent transcripts, each persistent failure phase,
  hanging descendant cleanup, zero-spawn recovery, and observation-surface secret
  scans fail before production implementation.
- GREEN: one-shot and persistent semantics remain separate; atomic publication,
  bounded environment, timeout, drain, escalation, and reap contracts pass on
  supported platform paths.
- Non-goals: no shell fixture, sandbox, auto-restart, ambient inheritance, or
  new package inventory.
- Verification: focused integration tests and `cargo xtask quick`.
- Stop: a runtime dependency, unsafe code, or process ownership outside the
  supervisor is required.

### Slice D — post-commit integration and normative documentation

- Rows: end-to-end closure of all rows, especially CW10-02, CW10-08, CW10-10,
  CW10-13, and CW10-14.
- Owner: application composition/dispatch boundary and standards docs.
- Allowed paths: `src/app_shell*`, smallest `src/app_input/` handlers, existing
  navigation/refresh/notice adapters, provider UI renderers, named scenarios,
  `dev-docs/standards/{persistence-and-runtime,architecture}.md`.
- RED: end-to-end dispatch/progress/terminal/confirmation/retry scenarios fail
  before adapter integration.
- GREEN: state commits and releases before supervisor execution; only declared,
  current, closed outcomes execute; scenarios and docs match the implementation.
- Non-goals: panel/config-migration behavior, Git Merger package, broad ownership
  audit, or quality-gate changes.
- Verification: TUI harness, focused tests, then exact-head `make ci-check`.
- Stop: integration needs an unplanned public abstraction or navigation behavior
  beyond the issue contract.

## 7. Expected paths by architectural layer

| Layer | Expected paths |
|---|---|
| Closed domain/effect contracts | `src/domain/effects.rs`, existing `src/domain/plugin/*` consumers |
| Wire/runtime | `src/runtime/provider/{mod,error,framing,protocol,supervisor}.rs`, `src/runtime/mod.rs` |
| Pure state | `src/state/provider_requests.rs` and focused test modules, smallest reducer registration |
| Typed messages | `src/messages.rs` and its existing split conversion/name modules |
| Post-commit composition | `src/app_shell.rs`, `src/app_shell_workers.rs`, smallest `src/app_input/` modules |
| Pure UI projection / thin render | existing action projection plus cohesive provider view/component modules |
| Behavioral evidence | Rust unit/integration fixture modules and `dev-docs/tmux-scenarios/` provider scenarios |
| Normative docs | `dev-docs/standards/persistence-and-runtime.md`, `dev-docs/standards/architecture.md` |
| Delivery record | `project-plans/issue390-plan.md` |

A fixture-only `[[bin]]` entry is allowed if the established fixture convention
requires it; no runtime dependency or feature is added.

## 8. Explicit non-goals

- Provider panel activation/events/snapshot rendering or config migration
  request execution (CW-11). The closed Outcome DTO may represent panel and
  migrated-config results, but current ownership/context validation rejects them.
- The Git Merger reference package (CW-12).
- A cross-architecture ownership/effect hardening sweep (CW-13). CW10-14 covers
  every provider-owned surface required by this issue.
- Auto-restart, provider state persistence, runtime-request migration, arbitrary
  commands/URLs/clipboard/PTY/shell/private host messages/raw UI.
- Sandboxing trusted providers.
- New dependencies, shell fixtures, unsafe code, production unwrap/expect,
  suppression attributes, lint/complexity threshold changes, or gate weakening.
- Changes to `.llxprt/`, `.code_puppy/`, `.github/`, unrelated tests, or quality
  tooling.

## 9. Scope ledger

| # | Discovered or anticipated work | Disposition |
|---|---|---|
| S1 | Live incremental line framing does not exist in `command_capture.rs` | **In scope (CW10-05/06)** — isolate in pure framing module and reuse only drain/process conventions |
| S2 | Staged 2 s / 2 s / 2 s shutdown is stricter than existing capture teardown | **In scope (CW10-11)** — isolate in supervisor and prove with descendant fixture |
| S3 | Existing `ProviderEffect`/`ProviderResponse` contain only availability operations | **In scope** — extend the existing closed CW-01 family; do not add another effect model |
| S4 | Persistent candidate startup must gate publication | **In scope (CW10-03/04)** — add orchestration at existing startup composition edge; handles stay in supervisor |
| S5 | Secret reference supplies an environment name but no secret store exists | **In scope (CW10-14)** — resolve only declared names from host environment; no new store/dependency |
| S6 | Panel/config-migration execution, Git Merger, or broad ownership audit | **Out of scope** — later configurable-workbench issues |
| S7 | Any `.github/`, quality-gate, dependency, `.llxprt/`, or `.code_puppy/` change | **Out of scope / approval required** |

New discoveries are appended before implementation expands.

## 10. Review counters

| Review | Budget | Used |
|---|---|---|
| Subagent design/code review cycles total | 2 | 0 |
| Local OCR before PR | 2 | 0 |
| OCR after PR | 2 | 0 |
| CodeRabbit | Ready-head review plus one bounded remediation cycle | 0 |

DeepThinker was requested for issue shaping but its transport closed before a
result; the successful evidence-grounded shaping analysis came from the
code-analysis subagent. A fresh DeepThinker review remains required in one code
review cycle.

## 11. Verification evidence

Per green slice: focused RED/GREEN evidence and `cargo xtask quick`.

Before push and on every candidate exact head:

```text
make ci-check
```

This includes format, strict Clippy, architecture, policy, source-size,
coverage, locked build, and locked tests. Process changes also require the PR's
native Windows CI. UI-visible behavior requires the tmux harness before the PR.

Evidence ledger:

- [x] Slice A RED / GREEN / quick gate — missing protocol module produced the intended compile RED; 45 focused framing/protocol tests pass; full-workspace strict Clippy and source-size gate pass; every provider production file is below 750 lines
- [ ] Slice B TUI RED first / reducer GREEN / quick gate
- [ ] Slice C supervisor fixtures GREEN / quick gate
- [ ] Slice D end-to-end TUI GREEN / docs complete
- [ ] Local Rust review and OCR triaged within counters
- [ ] Exact-head `make ci-check`
- [ ] PR ancestry and conflict check
- [ ] Exact-head CI including Windows and coverage
- [ ] CodeRabbit findings triaged, commented, and resolved
- [ ] Scope ledger clean

## 12. Deferred findings and follow-ups

None at shaping time beyond the explicit later configurable-workbench issues in
§8. Valid review findings outside this matrix will be recorded here and filed as
follow-ups instead of expanding this PR.
