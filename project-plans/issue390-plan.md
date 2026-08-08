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
- Delivered: pure startup composition (`runtime/provider/composition.rs`), the
  single start site (`startup_providers.rs`), provider actions lowered into the
  one `ActionRegistrySnapshot` under `HandlerKey::ProviderAction`, the
  edge-owned `ProviderCoordinator` in `AppContext` shut down before host exit,
  post-commit background execution through `services/provider_effect_worker` and
  `app_shell_workers::run_provider_worker`, real progress payload delivery, and
  the package Help section quoting the snapshot's own unavailable reason.
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
| S7 | `src/harness/v1/tmux_runner.rs` matched the scenario launch step as `Step::Launch { .. } => Ok(true)`, discarding its argv, cwd **and environment**. Jefe's tmux socket is derived from the uid, not from `--config`, so every tmux scenario silently attached to whatever jefe server the operator already had running — it enumerated their live agent sessions and reported them as unmatched | **In scope (fixed)** — `TmuxStartRequest` now carries an app environment applied to the `exec`'d command after the tmux scrub, and `contained_app_env` forces a workspace-local `JEFE_SOCKET_PATH` unless the scenario names its own. Verified: the "N live jefe session(s)" warning disappears and the scenario runs on its own server. No operator session was harmed (all 22 verified alive) |
| S12 | `availability_entries` recomputed availability for **every** action in the snapshot, including provider actions, from a reason table that describes compiled actions only. A provider action startup composition had marked `Unavailable { "no binary for ..." }` was silently republished as `Available` on the next refresh — offering the operator an action whose provider cannot run | **In scope (fixed)** — a package action now keeps the availability the authority that knows about it published; compiled actions are still recomputed from host state |
| S13 | Help's scroll clamp used `content_lines - viewport`, but `ScrollableText` word-wraps content and the offset is in *content-line* units. The clamp therefore stopped short by however many lines wrapped, leaving the tail of Help permanently unreachable. Invisible while the compiled table was all of Help; it hid the entire package section the moment anything was appended. The clamp also derived its viewport from raw terminal rows while the renderer used the app's (smaller) render rows | **In scope (fixed)** — `help_max_scroll` is one wrap-aware function both call sites share, returning the first content line whose own first display row still fills the viewport |
| S8 | CW-09 never registers installed packages as configuration owners, so `plugins.<id>` trust published as *dormant* and every package read as untrusted no matter what the operator chose. CW-10 cannot select trusted packages without this | **In scope (prerequisite)** — added `config_owners::owner_catalog_with_packages` and used it at the startup boundary, and moved the package scan ahead of settings publication. `plugin_command` already worked around the gap by string-scanning the raw document |
| S9 | The settings **editor** still builds its catalog from `builtin_owner_catalog()`, so toggling package trust in the UI remains unpublishable | **Deferred follow-up** — pre-existing CW-09 defect, not introduced here, and out of CW-10's acceptance matrix. Startup now reads trust correctly; the editor path needs the same package-aware catalog |
| S10 | `run_one_shot` retained only `TranscriptEntry::Progress(sequence)`, so the worker was fabricating empty progress (`message: ""`, no counts) — progress an operator cannot read, contradicting CW10-07 | **In scope (fixed)** — `LifecycleTranscript` now also retains the provider's own ordered `ProgressPayload`s, redacted against resolved secrets, and the worker forwards them verbatim |
| S11 | Any `.github/`, quality-gate, dependency, `.llxprt/`, or `.code_puppy/` change | **Out of scope / approval required** |

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
- [x] Slice B reducer GREEN / strict Clippy / source-size / architecture — 65 focused reducer+projection tests pass (24 acceptance CW10-06–10, 41 remediation RED-first across two batches); full-workspace strict Clippy, source-size, and architecture gates pass; `src/messages.rs` held at the 750-line warn boundary and `src/state/types.rs` trimmed below 998; every provider production file is below 750 lines; placeholder TUI scenarios deleted and deferred to Slice D RED-first with deterministic provider fixtures
- [x] Slice C supervisor fixtures GREEN / quick gate — 108 focused provider unit tests (framing, protocol, encode, environment, outbound queue 64/65 boundary, line reader, supervisor) plus 22 fixture-driven integration tests (CW10-02 happy/progress-256/provider-error/never-ready/crash/generation-drift; CW10-09 first-terminal; CW10-11 hang-shutdown staged reap, descendant-hang, strict shutdown-ack lifecycle; CW10-14 secret redaction across every provider-owned surface, environment isolation, caller-secret rejection) and the CW10-12 recovery zero-spawn executable-trap test all pass; full-workspace strict Clippy (`-D warnings`), `clippy-allows`, source-size, and architecture gates pass; locked all-feature workspace build succeeds; every new provider production file is below 750 lines (`supervisor.rs` largest at 725, `drains.rs` 197); the full lib suite (4270 tests) is green with no regressions
- [x] Slice C2 persistent lifecycle GREEN / strict Clippy / source-size / architecture — 20 pure persistent-helper unit tests (8 fail-fast health-classification, 2 signal-delivery-evidence) plus 22 fixture-driven integration tests (incl. shutdown-frame write-failure) pass (CW10-03 ordered reverse-input→plugin-ID startup, all-ready atomic publication, duplicate-plugin-id rejection; CW10-04 spawn/hello-ack-timeout/ready-timeout/protocol-fault/crash-after-ack/undeclared-capability failures with rollback reap, second-candidate rollback of the first, no auto-restart after a ready exit; CW10-11 explicit host-shutdown reap-all, bounded `Drop` reap-all with PID-liveness check, wrong/missing/eof-before-ack/data-after-ack cleanup failures while still reaping, lingering-descendant `DrainTimeout` cleanup evidence with `process_reaped=false`); the C1 one-shot supervisor integration tests (22) and the CW10-12 recovery zero-spawn test remain green with no regression; full-workspace strict Clippy (`-D warnings`), `clippy-allows`, source-size, and architecture gates pass; locked all-feature workspace build succeeds; the full lib suite (4290 passed) is green; every new provider production file is below 750 lines (`supervisor.rs` 749, `persistent.rs` 748, `candidate.rs` 551)
- [x] Slice D composition/integration GREEN — 7 composition tests (one-shot publishes with zero startup spawn, untrusted contributes nothing, unsupported platform publishes the shared reason, no-provider package contributes nothing, persistent candidates sorted by plugin id with fixed positive generation, binary contained under its package directory, failed persistent publication marks only those actions unavailable and withdraws them from the catalog); 4 keymap provider-lowering tests (provider actions join the single snapshot with availability, exact unavailable reason retained, a provider colliding with a compiled action id is refused, no-provider composition equals the compiled snapshot); 4 startup publication tests (one-shot publishes and starts nothing, untrusted never reaches the snapshot, a persistent candidate that cannot start publishes one shared reason with no supervisor and nothing invocable, no packages leaves the base snapshot untouched); 5 provider Help projection tests; 9 provider worker integration tests including RED-first real progress payload delivery. Full workspace suite `cargo test --workspace --all-features --locked`: **6730 passed, 0 failed**. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo xtask check source-size`, and `cargo xtask check architecture` all pass with no suppression and no threshold change; `supervisor.rs` was split into `supervisor.rs` (577) + `outcome.rs` (219) to stay under the 750-line warn boundary
- [x] Slice D docs complete — `dev-docs/standards/architecture.md` gains an action-provider ownership table and the two load-bearing rules (single start site; commit-then-execute); `dev-docs/standards/persistence-and-runtime.md` gains the full bounds table, `PLG-E502`/`PLG-E503` split, environment/secret contract, and cleanup contract
- [x] Slice D TUI scenario evidence — both fixture-backed scenarios pass against a real multiplexer: `provider-action-published.json` and `provider-action-unavailable.json` each report `ok: 11 steps`, asserting the live frame shows `Packages:` with `Ship release`, and `Run alien task  (Unavailable: no binary for aarch64-apple-darwin)` respectively. Getting there required fixing three real defects found by the attempt (S7, S12, S13)
- [ ] Local Rust review and OCR triaged within counters
- [ ] Exact-head `make ci-check`
- [ ] PR ancestry and conflict check
- [ ] Exact-head CI including Windows and coverage
- [ ] CodeRabbit findings triaged, commented, and resolved
- [ ] Scope ledger clean

## 12. Deferred findings and follow-ups

### Slice B pre-commit remediation (this commit)

Seven source-grounded fixes were applied before the Slice B commit, each with
RED-first tests:

1. **Placeholder TUI scenarios deleted.** The seven JSON files under
   `dev-docs/tmux-scenarios/issue390/` only asserted the pre-existing quit text
   and proved no provider behavior. Meaningful live TUI scenarios must be
   authored RED-first in Slice D before any thin renderer/live UI integration,
   when deterministic provider fixtures exist. TUI scenario validation is not
   behavioral RED; a schema-parser pass is not a scenario RED.

2. **Duplicate outbound queue removed.** `OutboundEnvelope`, `OutboundKind`,
   `MAX_QUEUED_ENVELOPES`, and all queue methods/constants were removed from the
   pure state. Staged closed `ProviderEffect` is the single post-commit outbound
   model; the real 64-envelope queue belongs inside the Slice C supervisor.

3. **Confirmation made exact.** `ActiveRequest` now retains original context
   refs, arguments, and an immutable `ActionPolicy`. The `RequestHostConfirmation`
   outcome is the sole confirmation-request path: it validates that the action
   declared `ProviderContinuation`, declared `RequestHostConfirmation`, and the
   destructive flag matches policy before persisting the exact title/body/confirm
   label/schema plus owner/action/context/generation for UI. Confirm validates
   exact owner/action/context/generation/id/TTL, consumes once, and returns a
   fresh `ProviderInvocation B` carrying original arguments/context and exact
   continuation values. The AppState handler stages `ProviderEffect::InvokeAction`
   for invocation B (not a separate `ConfirmContinuation` effect). The
   `ConfirmContinuation` and `Confirmed` variants were removed.

4. **Outcome acceptance validates declared kind.** `record_outcome` validates
   the outcome kind against the immutable action policy before terminal commit.
   Panel/migrated-config outcomes (`ReplacePanel`, `ClosePanel`, `MigratedConfig`)
   are rejected as `UnsupportedOutcome` in CW-10. Navigate/refresh/notice are
   checked against declared allowed outcomes.

5. **Post-terminal PLG-E502 observable.** Later bytes after a terminal result
   are reported as a typed `PostTerminal` (`PLG-E502`) protocol violation
   rather than silently ignored; the first terminal result is preserved. A
   progress monotonicity violation is reported as `ProgressFault` (`PLG-E502`)
   while marking the generation unavailable.

6. **Generation exhaustion fails typed.** The pure `next_generation` helper
   returns `GenerationExhausted` on u64::MAX rather than saturating/reusing.
   Tested directly through the pure helper with no test-only production backdoor.

7. **`provider_requests.rs` split.** Data types moved to
   `provider_request_model.rs` (375 lines); the reducer is 552 lines; every new
   production file is below 750. `ProviderMessage` boxed in `AppEvent`/
   `AppMessage` to avoid `large_enum_variant`.

No other findings at this time. Valid review findings outside this matrix
will be recorded here and filed as follow-ups instead of expanding this PR.

### Slice B post-review atomicity/view remediation (uncommitted)

Six additional RED-first fixes close the remaining atomicity, confirmation,
view, cancel, retry, and source-size gaps before the Slice B commit:

8. **Confirm consumes the token before allocating a generation.** `confirm()`
   now validates by immutable lookup first (owner/action/context/generation +
   confirmation id), then checks TTL, then computes the next generation
   without mutating, and only commits generation + request + token removal
   atomically. An invalid or expired confirm no longer increments the
   generation counter; generation exhaustion preserves the single-use token.

9. **TTL boundary is `>=` 300 and expired tokens are fail-fast single-use.**
   Expiration is now `elapsed >= CONFIRMATION_TTL_SECONDS` (300 exactly is
   expired, not valid). An expired token is consumed once on the failing
   attempt, so a repeated expired attempt sees `ConfirmationNotFound` rather
   than probing/reusing the token.

10. **`PendingConfirmationView` feeds the pure projection.** The private
    title/body/confirm_label/continuation_schema fields are now exposed as a
    read-only `PendingConfirmationView<'a>` via
    `ProviderRequestState::latest_pending_confirmation_view()`. The
    `ProviderViewMode::Confirmation` carries the exact declared
    title/body/confirm_label/continuation_schema and defaults keyboard focus to
    `ConfirmFocus::Cancel`; projection tests prove byte-exact content and the
    default Cancel focus.

11. **Cancel after a terminal request stages no effect.** `CancelOutcome` is
    now `Cancelled { key } | AlreadyTerminal { key }`. A cancel that arrives
    after the request already reached a terminal state returns
    `AlreadyTerminal`; the AppState handler stages no `CancelRequest` effect,
    consistent with first-terminal semantics. A reducer test and an
    AppState-level test prove no effect.

12. **Retry of an unknown old key returns `UnknownGeneration`.** `retry()`
    now requires the old key to match an active request; an unknown old key
    returns `UnknownGeneration` instead of silently starting a new request.

13. **Pre-existing files trimmed.** `src/messages.rs` is held at the 750-line
    warn boundary (no new warning) and the `provider_requests` field doc in
    `src/state/types.rs` is condensed so verbose field docs no longer push it
    to 998. No unrelated behavior was refactored.

### Slice C1 delivery (this commit)

Issue #390 Slice C1 lands the one-shot provider process supervisor, the closed
host-to-provider JSONL encoder, the isolated environment constructor with
secret resolution and redaction, the bounded 64-envelope outbound queue, and the
cross-platform Rust fixture binary. One-shot semantics are isolated; persistent
startup/publication (CW10-03/04) remains Slice C2 work.

- **Supervisor (`src/runtime/provider/supervisor.rs`).** Sole owner of the
  provider `Child`, its process group, its pipes, two continuous drain threads,
  the outbound queue, and the staged reaper. Drives the fresh one-shot lifecycle
  (spawn → hello/hello-ack → configure/ready → invoke-action → 0..256 progress
  → exactly one outcome/error → shutdown/shutdown-ack → EOF/reap) and returns
  only typed domain values (`OneShotResult`/`OneShotOutcome`/`LifecycleTranscript`);
  no `Child` or handle reaches `AppState`. `SupervisorBounds::PRODUCTION` holds
  the exact bounds (5 s handshake stage, 60 s invocation, 2 s shutdown-ack,
  2 s stdin-close, 2 s final-drain) and tests inject small values.
- **Staged shutdown/reap (CW10-11).** Closes new requests, waits 2 s for
  graceful exit; closes stdin and terminates the process group (Unix SIGTERM on
  the group / Windows `taskkill /T`), waits 2 s; force-kills and reaps the tree,
  waits 2 s. Stage C issues kill signals **without** an unbounded `child.wait()`
  (removed): `process_tree::terminate_process_tree` and `kill_process_tree`
  return typed `io::Result`s, and the bounded poll in `staged_shutdown` is the
  sole reap authority. EOF is recorded in the transcript **only** when the
  bounded final stdout drain observes an actual channel disconnection; a
  descendant that survives the leader's reap (holding an inherited pipe) surfaces
  as `CleanupFailure::DrainTimeout`, and clean cleanup requires both stdout EOF
  and a closed stderr drain — descendants are never assumed reaped merely because
  the leader reaped. stderr is drained continuously and retained ≤ 262 144 bytes.
- **Leak-proof schema redaction (CW10-14).** `redact_field` returns
  `Option<Field>`: a confirmation-schema field whose redacted scalars cannot
  re-validate (for example two enum choices that both redact to the same
  `[REDACTED]` placeholder) is **omitted** (`None`), never rebuilt as the
  original secret-bearing declaration.
- **Encoder (`encode.rs`), environment (`environment.rs`), queue
  (`outbound.rs`), line reader (`line_reader.rs`), process tree
  (`process_tree.rs`), drains (`drains.rs`).** No new dependency, no `unsafe`,
  no `serde_json::Value`, no production `unwrap`/`expect`/`panic`/`#[allow]`.
- **CW10-14 environment.** Base env begins empty (`env_clear`); the process
  receives only the provider directory + fixed platform system-bins PATH, a
  contained HOME/TMPDIR, the locale, manifest-declared nonsecret names, and
  explicitly bound secret environment bindings. Configure secret sources resolve
  only declared host-env references and land only in the owning `Configure`
  payload; `ProcessEnv` does not derive `Debug`; resolved secret values are
  redacted from retained stderr and from outcome/error strings and never appear
  in any diagnostic.
- **CW10-12.** Recovery/doctor remain provider-free; an executable-trap
  integration test places a canary provider on PATH and proves `jefe config
  validate` and `jefe doctor` never spawn it.
- **Fixture.** `tests/fixtures/provider_fixture.rs` (a feature-gated `[[bin]]
  jefe-provider-fixture` behind the `provider-fixtures` feature, never in the
  shipping/no-feature build) speaks the closed JSONL protocol in eleven scenario
  modes driven by the real supervisor. The `provider-fixtures` feature is the
  dev-dependency self-feature that integration tests opt into.

### Slice C1 final correctness remediation (RED-first, no commit)

Four source-grounded fixes close the remaining leak, EOF, reaping, and shutdown
correctness gaps before the Slice C1 commit, each proven RED-first:

1. **Leak-proof schema redaction (`redaction.rs`).** `redact_field` previously
   rebuilt a field from a redacted draft and fell back to the *original
   unredacted* `Field` on `Field::parse` failure (`Field::parse(draft).unwrap_or(field)`),
   which leaks a secret when two distinct enum choices redact to the same
   `[REDACTED]` placeholder and fail duplicate-choice revalidation. It now
   returns `Option<Field>`: a field whose redacted scalars cannot revalidate is
   **omitted** (`None`), and the caller `filter_map`s it out of the
   continuation schema. A RED test proves two distinct secret choices that both
   redact to the placeholder cause the field to be omitted and that **no
   original secret remains anywhere** in the rebuilt schema.
2. **Observed-only EOF and bounded final stdout drain (`supervisor.rs`,
   `drains.rs`, `driver.rs`).** The transcript recorded `TranscriptEntry::Eof`
   unconditionally in the normal-cleanup and drain-spawn-failure paths; it now
   records EOF only when the bounded final stdout drain (`final_stdout_drain`,
   moved to `drains.rs`) observes an actual channel disconnection. After process
   exit/kill the drain detects disconnection (clean EOF), rejects a remaining
   frame (data-after-ack → `CleanupFailure::ShutdownAck`), reports a non-frame
   fault (`CleanupFailure::ShutdownAck`), and returns
   `CleanupFailure::DrainTimeout` when EOF is not observed within the bound.
   RED tests cover valid EOF, lingering inherited stdout (timeout), and data
   buffered after ack.
3. **`observe_shutdown_ack` defers EOF to the final drain (`driver.rs`).** A
   valid ack no longer reads a follow-up event whose `Timeout` was treated as
   success; after a valid ack the method returns `None` and the bounded final
   stdout drain alone decides whether EOF was actually observed.
4. **Nonblocking Stage C reaping (`process_tree.rs`, `supervisor.rs`).**
   `kill_process_tree`/`force_kill_tree` no longer call `child.wait()`
   (unbounded); they issue kill signals and return typed `io::Result`s, and
   `staged_shutdown`'s bounded poll is the sole reap authority.
   `terminate_process_tree` likewise returns `io::Result`. A clean cleanup
   requires stdout EOF **and** a closed stderr drain (`compose_cleanup_failure`),
   so descendants are never assumed reaped merely because the leader reaped.

Files: `redaction.rs`, `supervisor.rs`, `supervisor_tests.rs`, `drains.rs`,
`driver.rs`, `process_tree.rs`, `Cargo.toml` (already feature-gated). No new
dependency, no `unsafe`, no production `unwrap`/`expect`/`panic`/`#[allow]`, no
unbounded wait, no secret-bearing fallback; every provider production file is
below 750 lines. Slice C2 is not implemented.

### Slice C2 delivered (persistent candidate lifecycle)

CW10-03 (ordered persistent candidate startup) and CW10-04 (atomic
all-or-nothing publication with rollback reap) are implemented. One-shot and
persistent state machines remain distinct: persistent processes never reuse
`run_one_shot` and are solely owned by `PersistentSupervisor` (a
runtime/provider type), never `AppState`. No process handle leaves the
supervisor; only typed readiness/health/publication values do.

- **Persistent supervisor (`src/runtime/provider/persistent.rs`, 527 lines).**
  Defines the atomic candidate/publication boundary: `run_persistent_startup`
  returns `PersistentStartupResult::Started { supervisor, publication }` (a
  `PersistentSupervisor` owning every ready process plus a separate, data-only
  `PersistentPublication` snapshot) or `PersistentStartupFailure { failure,
  rollback }` (typed failure/rollback evidence and no handles/publication).
  Candidates are sorted by canonical `PluginId` text and started in that order;
  duplicate plugin IDs are rejected before any spawn. Each candidate performs
  hello/hello-ack/configure/ready with the per-stage handshake bound; a `Ready`
  capability set that is not a subset of the manifest-declared capabilities is
  rejected (`Capability` phase, `PLG-E502`) before publication. No publication
  data is returned until every required candidate is `Ready`.
- **Atomic rollback/reap (CW10-04).** Any spawn/hello-ack/configure/ready/
  protocol/capability/timeout failure reaps every previously started and the
  failing candidate (best-effort `shutdown` frame then the existing staged
  process-tree/drain mechanics) and returns the per-candidate reap evidence;
  no publication is returned. No auto-restart: a ready process that exits is
  observed as `CandidateHealth::Exited` and is never respawned.
- **Explicit bounded shutdown (CW10-11).** `PersistentSupervisor::shutdown`
  sends a best-effort `shutdown` frame and runs the staged reap for every
  candidate, returning per-candidate `process_reaped` evidence; it is idempotent.
  `Drop` performs a bounded exact-PID/group cleanup (the staged reaper; bounded
  by the worst-case shutdown window) so a dropped supervisor cannot orphan a
  candidate process. `Drop` invokes `reap_all` explicitly (never silenced with
  `let _ =`); it cannot return evidence, but the bounded staged reap always runs.
- **Per-candidate startup (`src/runtime/provider/candidate.rs`, 414 lines).**
  Isolates spawn, drain spawn, and the closed handshake to `ready` (reusing C1
  framing/environment/redaction/process-tree helpers without merging the
  one-shot and persistent state machines). The owned candidate is boxed in the
  `StartOutcome` to keep the rollback `Result` small (no clippy allow), and the
  owned process carries only the fields the supervisor reads (the publication
  snapshot carries `plugin_version` separately, so no dead-code suppression is
  needed on the owned process).
- **Fixtures/tests.** `tests/fixtures/provider_fixture.rs` gained persistent
  modes (ready, hello-hang, ready-hang, protocol-drift, crash-after-ack,
  undeclared-cap, ready-then-exit, plus remediation modes: secret-protocol,
  illegal-bytes, descendant-hang, ack-wrong-kind, ack-missing, ack-eof-before,
  ack-data-after). `persistent_tests.rs` (8 fail-fast health-classification
  tests) and `supervisor_tests.rs` (2 signal-delivery-evidence tests, plus the
  `Io` cleanup-code assertion) cover `classify_health` precedence
  (illegal > try-wait error > exited-wins-over-closed > running+closed > ready),
  `signal_cleanup_evidence` (benign ESRCH filtered, real signal error preserved),
  and the `CleanupFailure::Io` runtime-unavailable code.
  `tests/issue390_persistent_providers.rs` is split via `#[path]` into
  `support`/`lifecycle`/`remediation` modules (each below 750 lines; 22
  integration tests) and cover: two
  candidates in reverse input order starting in plugin-ID order; all-ready
  atomic publication; spawn failure returning no publication and reaping
  nothing; failure at each handshake phase (hello-ack timeout, ready timeout,
  protocol fault, crash-after-ack, undeclared capability) with reap evidence;
  second-candidate failure rolling back the first; no auto-restart after a
  ready process exits; explicit host shutdown reaping all; `Drop` reaping all
  (PID liveness check); and duplicate plugin-ID rejection before any spawn; plus
  the remediation scenarios below, including a healthy candidate whose stdin
  closed before shutdown surfacing a `CleanupFailure::Io` write-failure while
  still being reaped.
- **Reuse without merging.** `staged_shutdown`, `wait_for_exit`,
  `collect_retained_stderr`, `compose_cleanup_failure`, environment/secret
  resolution, redaction, encode, and process-tree helpers are reused; the
  one-shot lifecycle driver and `run_one_shot` are untouched. The one-shot
  `staged_shutdown` signature now accepts `Option<ChildStdin>` (a persistent
  process may have already closed/dropped its stdin) with no behavior change for
  the one-shot caller, which still passes `Some`.

### Slice C2 persistent-correctness remediation (RED-first, no commit)

Five source-grounded fixes close the remaining secret-leak, cleanup-evidence,
strict-ack, fail-fast-health, and post-exit-pipe-closure gaps before the Slice
C2 commit, each proven RED-first:

1. **Persistent startup failures are redacted like one-shot.**
   `prepare_environment` now resolves and preserves a `Redactor` (cloned from
   the constructed `ProcessEnv`), and it is threaded through spawn, drain-spawn,
   handshake, and capability validation. Every `CandidateFailure`/`StartupFailure`
   diagnostic returned to the host is run through
   `redaction::redact_supervisor_failure` so a configure secret echoed in a
   malformed startup protocol error (`UnknownValue { value }`) or an I/O/spawn
   diagnostic never reaches an operator surface. A RED integration test scans
   `Debug`/`Display` of the startup failure for a declared secret echoed as an
   invalid capability and asserts no secret remains.
2. **Clean tree cleanup requires leader reaped AND bounded stdout EOF AND
   bounded stderr completion.** `reap_owned` no longer discards the
   `final_stdout_drain`/`collect_retained_stderr` outcomes: they feed
   `compose_cleanup_failure`, and `ReapedCandidate`/`CandidateShutdown` carry a
   typed `cleanup_failure: Option<CleanupFailure>` analogous to the one-shot
   result. `reaped`/`process_reaped` now report a *clean* tree cleanup
   (`cleanup_failure.is_none()`); a lingering descendant that survives the
   leader's reap and holds an inherited pipe makes `process_reaped=false` with a
   `DrainTimeout`, never a clean reap. A RED persistent fixture
   (`persistent-descendant-hang`, an escaping descendant via `process_group(0)`
   on Unix) proves `process_reaped=false` with a `DrainTimeout`.
3. **Strict shutdown-ack/EOF validation for healthy persistent shutdown and
   rollback-after-Ready.** A shared closed observer
   (`driver::await_shutdown_ack`) was extracted from the one-shot
   `Driver::observe_shutdown_ack` (behavior-preserving) and is reused by the
   persistent `observe_healthy_shutdown`: a healthy candidate's explicit
   shutdown and rollback-after-Ready advance the lifecycle through the outbound
   `shutdown` and strictly observe the `shutdown-ack`. A wrong kind, a missing
   (timeout) ack, an EOF before the ack, or data-after-ack each produce a
   `CleanupFailure::ShutdownAck` while still killing/reaping the tree. An
   unhealthy candidate (failed before `ready`) remains best-effort reaped with no
   ack expected. RED fixtures cover all four ack-fault modes.
4. **`health()` fails fast.** `candidate_health` probes the bounded stdout
   channel first (`try_recv`): an unexpected frame, a non-frame fault, or a
   closed-while-alive pipe each produce a sticky `CandidateHealth::ProtocolFault`
   (illegal stdout after Ready marks the candidate unavailable and triggers no
   restart) before `try_wait`. A `try_wait` OS error is reported as
   `CandidateHealth::ProbeFailed`, never `Ready`. Once observed, a `ProtocolFault`
   is sticky (the evidence is retained on the candidate) so it does not clear when
   the bytes are drained from the channel. Unit tests cover Ready, Exited,
   ProbeFailed, ProtocolFault (frame/fault/closed), and precedence.
5. **An already-exited candidate still collects bounded pipe closure.** The
   early `if owned.exited { return reaped=true }` shortcut is removed: when
   `health()` observed an exit, the leader is force-killed/terminated for any
   lingering descendant, and the bounded final stdout/stderr drains still run to
   confirm both pipes closed within the bound before a clean reap is claimed. A
   RED test drives a ready candidate to self-exit and asserts its subsequent
   explicit shutdown still collects the drains rather than short-circuiting.

No new dependency, no `unsafe`, no production `unwrap`/`expect`/`panic`/`todo`/
`unimplemented`, no suppression/clippy allow, no unbounded wait, no broad kill,
no ambient env, no generic bus/queue, no handle outside the supervisor, no
secret-bearing `Debug`. Every new provider production file is below 750 lines.

### Slice C2 final cleanup remediation (RED-first, no commit)

Five final cleanup items close the test-organization, health-ordering,
shutdown-write, signal-evidence, and `Drop`-silencing gaps before the Slice C2
commit, each proven RED-first where behavior was newly exercised:

6. **Oversize integration test target split.** The 900-line
   `tests/issue390_persistent_providers.rs` is split via the established
   `#[path = ...]` module organization (matching `tests/doctor.rs`) into a thin
   entry plus `support` (shared `Scene`/env/bounds/poll helpers), `lifecycle`
   (CW10-03/04 ordered startup, atomic publication, rollback reap, no
   auto-restart, explicit shutdown, duplicate-id), and `remediation`
   (CW10-11/14 cleanup-evidence/redaction/strict-ack/illegal-bytes/post-exit/
   shutdown-write) modules, each below the 750-line source-size gate. Only
   persistent-provider tests were moved; no unrelated tests were relocated.

7. **`classify_health` priority: process exit wins over a normal closed
   channel.** The classification is refactored into an explicit flat priority
   cascade — illegal buffered stdout > `try_wait` OS error (`ProbeFailed`) >
   process exit (`Exited`) > running-with-closed-stdout
   (`ProtocolFault(closed-while-alive)`) > idle (`Ready`) — so a normally-exited
   process whose stdout channel has disconnected is `Exited`, not a
   closed-while-alive protocol fault. A RED unit test pins
   exited+closed=>`Exited`; the existing running+closed=>`ProtocolFault` test
   is retained.

8. **`send_shutdown_frame` returns typed I/O evidence.** The write/flush errors
   it silently discarded (`let _ =`) now return `io::Result<()>`, and a healthy
   candidate's failed shutdown write/flush (its stdin closed or it exited before
   the host signalled) is incorporated as a typed `CleanupFailure::Io`
   (`PLG-E503`, redacted) while the staged reap still escalates and reaps. An
   unhealthy rollback candidate's signal remains best-effort (the bounded reap
   is authoritative). A RED integration test drives a healthy candidate to
   self-exit, leaves it marked alive (no `health()` probe so shutdown attempts
   the write), and asserts `CleanupFailure::Io` with the process still reaped.

9. **`reap_owned` preserves terminate/force-kill signal errors.** The
   `let _ =` discards on `terminate_process_tree`/`force_kill_tree` (in both the
   exited branch and the shared `staged_shutdown`) are replaced with captured
   `io::Error`s. A pure `signal_cleanup_evidence` helper preserves the first
   non-benign error as typed `CleanupFailure::Io` runtime evidence when the reap
   and drains are otherwise clean; a benign ESRCH (the target was already
   reaped) is filtered so a clean cleanup is never dirtied. On non-Unix the
   equivalent "process already gone" condition is not a stable errno, so the
   bounded reap/drains remain authoritative. The one-shot `staged_shutdown`
   signature now returns `(ShutdownOutcome, Vec<io::Error>)`; the one-shot
   caller destructures and ignores the signal errors (out of scope for C2).
   RED unit tests prove ESRCH is filtered and a real signal error is preserved.

10. **`Drop` no longer silences `reap_all`.** `let _ = reap_all(...)` is replaced
    with an explicit `reap_all(...)` invocation that runs the bounded staged
    cleanup (the return evidence cannot escape `Drop`, but the reap is never
    silenced and each candidate is reaped within the shutdown bounds).

No new dependency, no `unsafe`, no production `unwrap`/`expect`/`panic`/`todo`/
`unimplemented`, no suppression/clippy allow, no unbounded wait, no broad kill,
no ambient env, no generic bus/queue, no handle outside the supervisor, no
secret-bearing `Debug`. Every new and changed provider production and test file
is below 750 lines (`persistent.rs` 748, `supervisor.rs` 749). Slice D is not
implemented.
