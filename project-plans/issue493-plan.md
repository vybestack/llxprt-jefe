# Issue #493 — Detect and recover from native-Windows psmux server loss

## Problem

On native Windows, all local agents share one stable per-user/per-host psmux
namespace. If that psmux server exits while Jefe remains alive, every pane and
worker hosted by that server is lost together. The current liveness path fails
open when `list-sessions` itself fails, but it can treat a replacement server's
successful empty inventory as proof that every individual agent died. The app
then marks all affected agents `Dead`, clears their bindings, and loses the
information needed for deliberate batch recovery.

Issue #467 made healthy Windows sessions independent of the rebuildable Jefe
image. It did not make psmux server state survive server termination and does
not conflict with this issue.

## Accepted implementation decisions

The user delegated implementation-level choices after the issue's materially
different architectures were identified. The following bounded decisions are
therefore the accepted basis for Stack A and the planned Stack B:

1. **Explicit public state:** add `AgentStatus::ServerLost`. It is a transient,
   recoverable state, preserves `runtime_binding` and the runtime launch
   signature, renders as a red/error status, and projects to
   `LastKnownRuntime::Running` rather than `Stopped`. This requires approving a
   public domain-enum change and a small public runtime server-observation
   contract.
2. **No persistent client subsystem:** reuse the existing two-second liveness
   observer as a PID-pinned watchdog. On Windows it probes
   `display-message -p "#{pid}|#{version}"` and applies the capability-verified
   server option `set-option -s exit-empty off` once per observed server
   identity. No long-lived control-client child process is added.
3. **Explicit recovery, never automatic:** show a confirmation action after
   server loss. On confirmation, relaunch every currently `ServerLost` local
   agent from retained runtime launch signatures. Successful agents become
   `Running`; failures remain `ServerLost`; a summary notice and detailed logs
   identify partial failure. No automatic relaunch occurs.
4. **Bounded delivery:** use two stacked PRs if the complete diff would exceed
   the target of 25 files or 1,500 net lines. The first PR owns observation,
   classification, status, prevention, startup diagnostics, docs, and the
   native regression. The second owns the user-confirmed batch-recovery flow.
   The final PR closes #493 after both stacks pass.

Empirical capability evidence on the development host (psmux 3.3.7) before
implementation:

- `display-message -p '#{pid}|#{version}'` returned the server PID and `3.3.7`;
- `show-options -s exit-empty` returned `exit-empty on`;
- `set-option -s exit-empty off` succeeded;
- all probes used a unique `jefe-issue493-*-probe` namespace and cleanup killed
  only that disposable server.

A persistent client or `exit-empty off` cannot preserve sessions after an
explicit `kill-server`: the server and its in-memory panes are already gone.
The accepted behavior is prevention of idle exit where supported, prompt
server-identity diagnosis, retention of recovery metadata, and deliberate
recreation—not transparent survival.

## Acceptance matrix

| ID | Actor / launch path | Input and boundary | Target | Observable success | Failure and diagnostic | Side effects permitted before failure | Persistence / compatibility | Behavioral proof |
|---|---|---|---|---|---|---|---|---|
| A1 | App startup dependency probe | Windows, no psmux server; executable present at 3.3.7+ | Local Windows | Executable preflight succeeds without creating a server or changing agent state | Missing, malformed, or <3.3.7 executable produces one user-visible warning and a structured log naming resolved executable, detected/minimum version where known, and remediation | Read-only `psmux -V`; no server/session creation or state write | Existing state remains loaded; startup remains usable for diagnostics | Pure preflight-result test plus native no-server probe |
| A2 | App startup server probe | Windows, existing psmux server below 3.3.7 or server probe unavailable | Local Windows | Existing supported server identity/version is logged; no session is disturbed | Unsupported server version produces a user-visible warning; unavailable server is informational when no running local binding needs it | Read-only server probe only | Live bindings and sessions are retained; no schema change | Startup diagnostic reducer test and isolated psmux probe |
| A3 | Periodic liveness | Same observed server identity; all tracked sessions/panes present | Local Windows | All agents remain `Running`; existing two-second cadence and bounded subprocess behavior remain responsive | Probe failure is logged and fails open for that cycle | Read-only queries; no agent mutation | No durable write for no-op cycles | Pure server-observation classification and app-shell integration test |
| A4 | Periodic liveness | Previously observed server is unavailable while one or more local agents are `Running` | Local Windows | Result is `ServerGone`; every affected agent becomes `ServerLost`; bindings and launch signatures remain available; no agent becomes `Dead` | Warning contains executable, version if known, namespace, prior server PID/identity, command/status/stderr, and affected count/IDs using existing redaction rules | Status transition and one coalesced durable projection; no relaunch, kill, or binding clear | `ServerLost` projects as last-known running; persisted schema version remains unchanged | Pure classifier test, state transition test, app-shell test, real-psmux regression |
| A5 | Periodic liveness | Current server identity differs from the pinned identity, whether its inventory is empty or contains unrelated sessions | Local Windows | Result is `ServerReplaced`; agents bound to the prior server become `ServerLost`, preserving bindings/signatures; replacement server sessions are not treated as the old sessions | Same structured warning as A4 with prior/current PID and creation identity | Status transition only; no session mutation | Same as A4 | Pure PID/creation-token replacement matrix and real-psmux replacement regression |
| A6 | Periodic liveness | Same server identity; one target session is missing or has no live pane while peers remain healthy | Local Windows and existing Unix behavior | Existing per-agent death path runs only for that target: preview captured where possible, status `Dead`, binding cleared, dead signature retained | Existing session-scoped liveness diagnostics | Existing per-agent mutation/save only | Existing persistence behavior unchanged | Regression proving same-server individual death remains `Dead` and peers remain `Running` |
| A7 | Windows server retention | First observation of each supported psmux server identity | Local Windows | Jefe applies `exit-empty off` once for that identity; periodic PID observation detects later loss/replacement | Unsupported option or command failure is capability-classified, logged, and does not crash or mark agents dead | One namespace-scoped `set-option`; no persistent child and no retry storm | Runtime-only identity cache; no schema change | Pure once-per-identity planner test and isolated native psmux option test |
| A8 | Recovery action | One or more `ServerLost` agents; user opens and confirms batch recovery | Local Windows | Jefe recreates each affected session from its retained signature; successes receive fresh bindings and become `Running`; failures remain `ServerLost`; unrelated agents/sessions are untouched | Confirmation/result notice reports success/failure counts; per-agent typed runtime logs include agent/session and operation | Only confirmed affected sessions may be recreated; best-effort batch continues after an individual failure | Existing launch signature and binding formats remain compatible; one coalesced save after final reducer results | TUI scenario first, reducer/message tests, manager batch integration test, real-psmux kill-and-recover regression |
| A9 | Recovery cancellation/retry | Recovery prompt canceled, stale confirmation, no affected agents, or partial prior failure | Local Windows | Cancel performs no runtime action; stale/no-op request is rejected deterministically; a later explicit action retries only agents still `ServerLost` | User-visible no-op/stale or partial-failure notice; detailed logs at runtime boundary | None before valid confirmation; retry touches only still-affected agents | Bindings/signatures remain retained | Pure reducer tests and orchestration stale-result test |
| A10 | Unix liveness and launch | tmux on macOS/Linux, including missing individual session | Local Unix | Current liveness, namespace, startup, and relaunch behavior is unchanged | Existing diagnostics remain | No Windows server option/probe path executes | Existing persisted state unchanged | Unix structural command tests and existing full suite |
| A11 | Native end-to-end regression | Isolated namespace, at least two long-running sessions, explicit server termination, bounded wait, confirmed recovery | Native Windows / psmux 3.3.7+ | Server loss is diagnosed within the accepted poll bound; agents are never mass-marked `Dead`; bindings/signatures remain; confirmed batch relaunch creates live replacement sessions; bystander namespace is untouched | Retained transcript identifies namespace, server PID/version, commands, statuses, stderr, agent/session IDs, and cleanup result | Test may kill only its unique owned namespace and must clean all created sessions/processes | No production namespace or user state touched | Feature-gated `psmux-smoke` integration target plus schema-1 TUI scenario for visible status/recovery |
| A12 | Documentation user | Windows installation and troubleshooting | Docs | Public docs consistently require psmux 3.3.7+ and explain Server Lost/recovery semantics | N/A | Documentation only | No compatibility change | Focused text assertion/review plus docs build checks in normal gates |

## Explicit non-goals

- Preserving an existing psmux pane or worker after an explicit `kill-server`;
  recovery creates replacements from retained signatures.
- Automatically relaunching agents without a user confirmation.
- Adding a persistent psmux control client, daemon, service, new timeout loop,
  or separate process supervisor.
- Randomizing the stable host/user namespace or changing cross-instance session
  sharing; that is a separate architectural decision.
- Preventing a fully privileged agent shell from invoking psmux or killing
  processes; Jefe detects and recovers from the resulting server loss.
- Proving whether the historical trigger was idle exit, RDP teardown, AV/EDR,
  or an agent-issued psmux command. Diagnostics are added for future evidence.
- Recovering after Jefe itself exits between observing server loss and the user
  confirming recovery; issue #493 is the mid-run path while Jefe remains alive.
- Reattaching to arbitrary orphaned Bun/Node processes after their PTY server is
  gone.
- Changing remote-agent ownership, liveness, relaunch, or persistence.
- Changing Unix/tmux server options or lifecycle behavior.
- Catch-unwind hardening for the liveness future, broad runtime cleanup, or
  unrelated test relocation.
- Dependency, workflow, `.github/`, `.code_puppy/`, `.llxprt/`, quality-gate,
  lint-policy, or threshold changes.

## Bounded vertical slices

### Slice 1 — RED server-loss contracts and scenarios

**Rows:** A3-A6, A8-A11.

**Owner / boundary:** deterministic runtime observation contract plus existing
native psmux and schema-1 TUI harness boundaries.

**Allowed paths:**

- `project-plans/issue493-plan.md`
- `src/runtime/server_health.rs` (new only after public-contract approval)
- `tests/runtime_server_health.rs` or focused module tests
- `tests/psmux_server_loss.rs`
- `dev-docs/tmux-scenarios/v1/issue493-server-loss.json`
- existing harness documentation only if needed to register the scenario

**RED:** create/update the TUI scenario first and prove the visible
server-loss/recovery expectation fails. Add pure classification cases for
unavailable server, replaced identity, same-server individual death, probe
failure, and stale observations. Add a real-psmux regression that fails because
current code cannot retain/recover affected bindings.

**GREEN:** tests compile against the smallest typed contract; production remains
unchanged until the RED reason is recorded in Verification Evidence.

**Stop:** if the existing schema-1 grammar cannot express the scenario without a
new harness command or quality/workflow change, stop for approval rather than
extending the harness.

### Slice 2 — Server observation, diagnostics, and `ServerLost`

**Rows:** A3-A6, A10.

**Owner / boundary:** runtime performs psmux I/O; pure classification returns a
typed observation; app-shell applies generation-checked typed state messages.

**Allowed paths:**

- `src/runtime/server_health.rs`
- `src/runtime/liveness.rs`
- `src/runtime/mod.rs`
- `src/app_shell_liveness.rs` (new, direct extraction required because
  `src/app_shell.rs` is already 997 lines)
- `src/app_shell.rs`
- `src/domain/mod.rs`
- `src/messages.rs` / existing smallest runtime message module
- `src/state/runtime_ops.rs`
- `src/state/durable_projection.rs`
- `src/state/terminal_manager_types.rs`
- `src/ui/components/agent_list.rs` and focused existing tests

**RED:** pure and reducer tests prove A3-A6 fail on current main. In particular,
a successful empty inventory from a changed server identity must not enter
`reconcile_dead_agents_with_identity`.

**GREEN:** `ServerGone`/`ServerReplaced` are generation-checked server-wide
observations; affected agents become `ServerLost`; only `Healthy` same-server
observations can generate per-agent `Dead` identities.

**Refactor:** move only the directly touched liveness future out of the
near-limit `app_shell.rs`; do not move unrelated tests or observers.

**Stop:** any need for a second state store, schema-version change, process
enumerator, or unrelated app-shell refactor.

### Slice 3 — Windows PID-pinned retention

**Rows:** A3, A5, A7, A10-A11.

**Owner / boundary:** Windows psmux command planning/execution in runtime; pure
once-per-server-identity decision state in the liveness observer.

**Allowed paths:**

- `src/runtime/server_health.rs`
- `src/runtime/multiplexer.rs`
- `src/runtime/liveness.rs`
- `src/app_shell_liveness.rs`
- focused runtime/native psmux tests

**RED:** command-shape tests require structured `display-message` and
namespace-scoped `set-option -s exit-empty off`; state tests require one option
application per identity and reapplication after replacement.

**GREEN:** the existing observer performs bounded periodic identity checks and
applies the option once per supported server. Failures are typed/fail-open and
never trigger per-agent death.

**Stop:** any persistent child/control connection, new process manager, new
background loop, dependency, unsafe code, or unbounded retry.

### Slice 4 — Startup version diagnostics and docs

**Rows:** A1-A2, A12.

**Owner / boundary:** existing multiplexer preflight and app initialization;
public Windows documentation.

**Allowed paths:**

- `src/app_init.rs` and existing focused app-init tests
- `src/runtime/multiplexer.rs` only if server-version parsing cannot live in the
  approved server-health module
- `docs/windows-support.md`
- `docs/technical-overview.md`

**RED:** tests prove executable/server <3.3.7 yields a non-blocking startup
warning without creating/killing a server or clearing bindings; docs assertion
fails on the current 3.3.6 claims.

**GREEN:** startup checks the installed executable and any already-running
server best-effort, surfaces one actionable warning, and docs consistently state
3.3.7+.

**Stop:** blocking app startup, modifying doctor architecture, or probing a
server in a way that creates one.

### Slice 5 — User-confirmed batch recovery

**Rows:** A8-A9, A11.

**Owner / boundary:** UI emits intent; state/reducer owns confirmation and stale
request semantics; app-input orchestration invokes a typed runtime batch
operation; runtime recreates only affected sessions.

**Allowed paths:**

- `src/state/types.rs` / focused recovery state module
- `src/state/modal_ops.rs` and focused tests
- `src/messages.rs` / event conversion
- `src/app_input/agent_runtime.rs` and focused tests
- `src/runtime/manager.rs` or a focused manager recovery module
- `src/ui/orchestration.rs`
- `src/ui/modals/confirm.rs` only if the existing generic modal cannot render
  the approved variant without change
- `src/ui/components/keybind_bar.rs` and help text for discoverability
- the Slice 1 TUI/native scenarios

**RED:** the TUI scenario is updated first for prompt, cancel, confirm, partial
failure, and successful summary; reducer/runtime tests prove no automatic action
and no loss of failed signatures.

**GREEN:** confirmation dispatches one generation/correlation-guarded batch;
results reduce deterministically; successful and failed agents receive the
states in A8.

**Stop:** automatic relaunch, retry scheduler, parallel task subsystem, changes
to `MAX_DEAD_SIGNATURES`, or recovery behavior for agents not in `ServerLost`.

## Delivery split and expected path budget

### Stack A — observation, status, prevention, diagnostics

Expected 14-18 files, approximately 850-1,200 net lines:

- plan;
- runtime server-health/liveness/multiplexer modules and tests;
- direct app-shell liveness extraction;
- domain/status, runtime message/reducer/projection/display matches;
- app-init tests;
- Windows docs;
- real-psmux regression and visible-status TUI scenario.

### Stack B — confirmed batch recovery

Expected 8-12 files, approximately 450-750 net lines:

- recovery modal/message/state/input/runtime orchestration;
- keybind/help discoverability;
- focused reducer/orchestration tests;
- TUI and real-psmux scenario completion.

Each PR independently targets no more than 25 files / 1,500 net lines. A
mandatory scope review occurs before crossing either target. Work stops without
explicit approval above 40 files or 2,500 net lines in either PR. If both stacks
fit under the target as one coherent diff, a single PR is permitted only after
a recorded scope review confirms every file maps to A1-A12.

## Scope ledger

| Date | Discovery / proposed change | Acceptance mapping | Disposition |
|---|---|---|---|
| 2026-07-28 | Add public `AgentStatus::ServerLost` and public typed server observation | A3-A6, A8-A10 | Accepted; Stack A implemented |
| 2026-07-28 | Choose periodic PID-pinned observer plus `exit-empty off`, not persistent control client | A3, A5, A7, A10-A11 | Accepted; Stack A implemented |
| 2026-07-28 | Choose user-confirmed best-effort batch relaunch, not automatic relaunch | A8-A9, A11 | Accepted for Stack B; not implemented in Stack A |
| 2026-07-28 | Extract only the liveness future because `src/app_shell.rs` was 997 lines | A3-A6 | In-scope; reduced `app_shell.rs` to 878 lines |
| 2026-07-28 | Installed psmux 3.3.7 exposes server PID/version and supports `exit-empty off` in an isolated probe | A2, A5, A7 | Verified capability; no repository change |
| 2026-07-28 | Namespace randomization would break cross-instance sharing | Non-goal | Reject for #493; separate decision/follow-up only |
| 2026-07-28 | Liveness `catch_unwind` hardening is adjacent | Non-goal | Defer; do not implement in #493 |
| 2026-07-28 | Stack A scope measured at 21 files / 1,570 net lines including this 333-line plan | A1-A7, A10-A12 | Mandatory scope review complete: implementation is 1,237 net lines excluding the required plan; below 25 files and far below hard stop |
| 2026-07-28 | Schema-1 visible-status scenario cannot execute on native Windows (`HAR-E005`) | A11 | Scenario fixture added and syntax/build path validated; native diagnosis is proven by real-psmux test; execute TUI fixture on Unix CI |

Every changed file must be added to this ledger or map to an allowed path in a
slice. Reviewer suggestions do not authorize expansion.

## Review counters

| Review phase | Used | Cap |
|---|---:|---:|
| Local Open Code Review | 2 | 2 |
| Post-PR Open Code Review | 0 | 2 |

Every finding will be recorded below as **Blocker-Fix**, **In-scope-Fix**,
**Reject**, or **Defer** before code changes are made.

## Verification evidence

| Candidate / slice | Command or scenario | Result |
|---|---|---|
| Planning baseline | `git fetch origin main`; `main...origin/main` | 0 ahead / 0 behind |
| Capability probe | isolated psmux 3.3.7 `display-message`, `show-options`, `set-option`; owned `kill-server` cleanup | Pass |
| Slice 1 RED | `cargo test --test runtime_server_health` before production contract | Failed as expected: unresolved `ServerHealth`, `ServerIdentity`, and classifier |
| Focused GREEN | runtime server health (20), app-shell liveness (4), durable projection (1) | Pass |
| `cargo xtask quick` | Initial native-Windows run lacked Unix `true`/`false`; rerun with existing Git-for-Windows shims on `PATH` | Pass: 2,506 lib + 801 bin tests and all integration targets |
| `JEFE_REQUIRE_PSMUX=1 cargo test --features psmux-smoke --test psmux_server_loss -- --nocapture` | Isolated namespace: Healthy, once-per-identity option, Gone, Replaced | Pass: 1/1 |
| Schema-1 TUI scenario | `target/debug/tmux_scenario --scenario dev-docs/tmux-scenarios/v1/issue493-server-loss.json` | Not executable on win32: expected `HAR-E005` Unix-PTY requirement; fixture retained for Unix CI |
| Exact local gates | fmt, strict Clippy, locked all-feature build/test; Git-for-Windows shims on `PATH` for three existing Unix-command fixtures | Pass |
| Source-size policy | `cargo xtask check source-size` | Pass; only existing advisory warnings, changed files below hard limit |
| Required CI exact candidate head | Pending after PR |
| Ancestry / conflict check | Pending before each PR | Not run |

## Review findings and deferred follow-ups

Local OCR #1 findings and dispositions:

- **Blocker-Fix — resolved:** missing native diagnosis evidence; added
  `tests/psmux_server_loss.rs` with an isolated owned namespace.
- **Blocker-Fix — resolved:** binding-preservation test directly assigned status;
  replaced with a reducer-driven transition assertion.
- **In-scope-Fix — resolved:** identity-capture failure could compare a synthetic
  creation token; the I/O boundary now fails open as `Unavailable`.
- **In-scope-Fix — resolved:** stale liveness results could transition a rebound
  agent; session name, generation, and current `Running` status are revalidated.
- **In-scope-Fix — resolved:** `ServerLost` agents could re-enter individual Dead
  reconciliation; only currently `Running` targets are reconciled.
- **In-scope-Fix — resolved:** stale/no-op server-loss cycles staged unnecessary
  saves; saves now occur only after at least one transition.

Local OCR #2 reported zero Blocker-Fix or In-scope-Fix findings and judged Stack A
coherent. The running-server startup version refinement and confirmed batch
recovery remain accepted Stack B work, not optional expansion of this checkpoint.
Namespace randomization and liveness-future panic containment remain explicit
non-goals.

## Completion conditions

Stop successfully only after A1-A12 each have behavioral evidence, exact-head
local and required CI gates pass, review output is fully classified and all
Blocker-Fix/In-scope-Fix findings are resolved, ancestry is correct, the final
PR is conflict-free, and the scope ledger contains no unapproved change. Do not
continue optional hardening after those conditions are met.
