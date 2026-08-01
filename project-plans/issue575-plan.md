# Issue #575 — Re-resolve blank-selector executable after an in-session upgrade

> A blank-selector local agent currently launches from the startup-captured
> `ResolvedCandidate`. Replacing the direct executable while Jefe remains open
> therefore makes every attempt reuse stale fingerprint evidence and fail with
> `AGT-E203`. Launch preparation must freshly resolve direct candidates while
> preserving the fail-closed probe-to-execution fingerprint boundary.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Targets | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | Existing local agent launch, relaunch, or send | Blank selector; executable unchanged since startup observation | Unix and Windows direct executable/wrapper forms | Existing launch behavior remains unchanged | Existing attributable probe diagnostics | Existing probe/preflight effects only | No persisted-agent mutation | Direct-launch regression test |
| A2 | Existing local agent launch, relaunch, or send | Blank selector; executable replaced after startup and stable/compatible during fresh probe | Unix and Windows | Same action freshly resolves, advances generation, probes, and launches replacement | No stale `AGT-E203` for superseded startup evidence | Fresh probe, then normal authorized launch | Durable agent configuration remains unchanged | Executable replacement behavior test plus TUI scenario proving success and no error |
| A3 | Existing local agent launch, relaunch, or send | Candidate changes after fresh probe evidence but before execution | Unix and Windows | None | `AGT-E203` remains attributable and fail-closed | Zero agent-launch side effects after mismatch | No stale plan is persisted or executed | Deterministic execution-boundary race test |
| A4 | Existing local agent launch, relaunch, or send | Replacement resolves but is incompatible or probe fails | Local | None | Current typed incompatibility/probe diagnostic | Probe only; no launch | Existing record remains recoverable | Incompatible/probe-failure behavior tests |
| A5 | Existing local agent launch, relaunch, or send | Direct executable removed or no current candidate resolves | Local | None | Current NotFound/unavailable diagnostic | No launch | Existing record remains intact | Removed-candidate behavior test |
| A6 | Explicit version-selector launch | Nonblank npm/uvx selector | Existing supported targets | Existing package resolution/materialization/probe/launch behavior | Existing package diagnostics | Existing contract | No selector migration | Package-selector regression tests |

## Non-goals

- Removing or weakening executable fingerprint checks.
- Trusting an incompatible or unprobeable replacement.
- Changing npm/uvx selector semantics or package-cache ownership.
- Changing remote candidate resolution.
- Mutating or pinning persisted selector data.
- Adding filesystem watching, polling, or a hot-reload subsystem.
- Redesigning availability UI or error history.

## Vertical slices

### Slice 1 — Direct replacement recovery

- **Rows:** A1, A2, A4, A5.
- **Owner / boundary:** runtime launch composition and candidate/probe generation reconciliation.
- **Allowed paths:** `src/runtime/launch_compose.rs`, its existing tests, a bounded issue behavior test, and the issue TUI scenario.
- **RED:** first update/add the TUI scenario so a stable executable replacement must proceed without `AGT-E203`; add a runtime behavior fixture that replaces a direct executable after startup evidence and currently reproduces stale `AGT-E203`.
- **GREEN:** direct launch preparation captures a fresh target-appropriate `PathSnapshot`, resolves once, computes generation from startup/current keys, and probes the fresh resolution. Current NotFound/incompatible/probe failures remain typed.
- **Non-goals:** no state publication, watcher, package, remote, or persistence changes.
- **Verification:** focused scenario/test, `cargo xtask quick`, and the required full gate.
- **Stop for approval:** any need for a new public abstraction/subsystem, state/persistence redesign, dependency/workflow/quality-tool change, or unrelated refactor.

### Slice 2 — Preserve post-evidence race safety and selector compatibility

- **Rows:** A2, A3, A6.
- **Owner / boundary:** immutable plan authorization and runtime execution guard.
- **Allowed paths:** existing runtime guard tests and package-selector tests; production guard code only if RED demonstrates a missing boundary.
- **RED:** deterministic replacement after fresh evidence must prove no stale candidate executes; retain package-selector regression coverage.
- **GREEN:** post-evidence replacement remains `AGT-E203` with zero launch side effects while stable direct replacement launches; explicit selectors remain unchanged.
- **Non-goals:** no new execution mechanism or package behavior.
- **Verification:** focused guard/package tests, issue TUI scenario, `cargo xtask quick`, full gate, and native Windows CI.
- **Stop for approval:** any new process-management abstraction or behavior absent from A1–A6.

## Expected paths / architectural layers

- `src/runtime/launch_compose.rs` — launch-time direct resolution and generation reconciliation.
- Existing runtime behavioral tests and/or `tests/issue575_behavior.rs` — replacement, current failure, and selector regression evidence.
- Existing execution-guard tests — post-probe fingerprint race evidence if not already sufficient.
- `dev-docs/tmux-scenarios/issue575/` and its fixture registration — UI-visible stable-upgrade/no-error proof.
- `src/runtime/agent_probe.rs` or `src/app_input/availability.rs` only if RED proves the existing contracts cannot carry the required evidence cleanly.

No new subsystem, public abstraction, dependency, workflow, quality-tool change,
or unrelated refactor is authorized.

## Scope ledger

| Entry | Status | Reason |
|---|---|---|
| Fresh direct-candidate resolution during launch preparation | In scope | A2 |
| Probe-generation reconciliation from startup and fresh keys | In scope | A2/A3 |
| Preserve post-evidence fingerprint mismatch rejection | In scope | A3 |
| Current-candidate typed failure diagnostics | In scope | A4/A5 |
| Explicit-selector regression protection | In scope | A6 |
| Filesystem watching/general hot reload | Rejected | Explicit non-goal |
| Package cache or remote resolver changes | Rejected | Different ownership and semantics |

## Review and verification ledger

- Local OCR: `1 / 2` — reviewed 21 supported changed files; seven findings triaged. Fixed signed/subsecond timestamp capture, fingerprint diagnostics, typed refresh gating and tests, discarded duplicate probe, recapture logging, and implicit fixture target mutation. Four files timed out in OCR subtasks and remain covered by the full reviewer cycle and local gates.
- PR OCR: `1 / 2` — six findings triaged. Fixed typed fingerprint capture errors and modification-time failure propagation. Rejected the diagnostic-format compatibility claim because `CandidateFingerprint::Display` is an internal diagnostic with no parser contract; rejected both cached-availability findings because A2/A4/A5 require the immediate authoritative launch probe to discover current installed, incompatible, failed, or removed state; rejected the private test-fixture interpolation warning because every value is a hardcoded trusted version literal.
- rustreviewer / DeepThinker: one clean full-review cycle requested over all changed and untracked files; review remediation used the complete available findings plus OCR.
- RED evidence: the issue TUI fixture failed before implementation because the upgraded agent became `Dead` instead of `Running`; the stable replacement and race-boundary runtime tests also established the intended behavior.
- Fast verification: `cargo xtask quick` passes after review remediation.
- TUI verification: `direct_upgrade_fixture_launches_replacement_without_stale_error` passes, including an empty Errors screen and unchanged persisted selector/version data.
- Windows structural verification: `cargo check --target x86_64-pc-windows-msvc --all-features` passes.
- Exact-head verification: `cargo xtask ci` passes after PR review remediation; line coverage remains above the 30% floor.
- Native Windows CI: passed on PR #582, including MSVC + psmux, Windows Clippy, and Windows coverage floors.
- Deferred findings: none

## Completion contract

Complete only when every row has behavioral evidence, a stable pre-launch
replacement creates no user-visible stale error, a post-evidence replacement
remains fail-closed, exact-head local verification and required CI (including
native Windows) pass, reviews are triaged within their counters, the PR is
conflict-free, and every changed file maps to this ledger.
