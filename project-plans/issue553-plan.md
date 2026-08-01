# Issue #553 — AGT-E202 probe timeout when sending an issue to an agent

## Problem

Sending an issue to a local LLxprt agent (version selector `nightly`) failed with:

    Error: spawn failed: probe error AGT-E202: probe timed out

The message is emitted by `agent_plan::validate_probe_evidence` after
`launch_compose::prepare_launch` runs the authoritative local probe
(`agent_probe::run_local_agent_probe`). Three separate defects make that
outcome both more likely and undiagnosable.

### D1 — package materialization is charged to the probe budget

`AgentProbeTarget::Local.total_timeout()` bounds each probe process at
`LOCAL_PROBE_TIMEOUT_MS` (10 s), which is a budget for *agent startup
responsiveness*.

For npm candidates that is correct: `package_runtime::prepare_managed_npm`
installs the package under `INSTALL_TIMEOUT` (300 s) **before** the probe
deadline is computed, so download cost is never charged to the probe.

For uvx candidates it is wrong. `package_invocation` returns the `uvx`
executable plus the structural prefix `--from <spec> <binary>`, so the probe
executes `uvx --from code-puppy==<version> code-puppy --version` **inside** the
10 s deadline, and uvx resolves and downloads the environment as part of that
process. Measured on a cold uv cache:

    uvx --from code-puppy==0.0.600 code-puppy --version   ->  24.27 s

That is a deterministic `AGT-E202 probe timed out` for any Code Puppy agent
whose selector is not already warm in the uv cache.

### D2 — AGT-E202 is undiagnosable

`ProbeFailure::into_availability` collapses every timeout to the bare string
`"probe timed out"`. The reported failure names no phase (identity vs
capability), no executable, no elapsed time and no budget, so a field report of
this failure cannot be attributed to a phase or a binary. The other failure
reasons are inconsistent: `Failed` carries a phase, `Evidence` and `Truncated`
carry none.

### D3 — one issue send runs the authoritative probe two or three times

`prepare_launch` is the authoritative probe boundary, and the send paths call it
more than once for a single user action:

- `issues_send::dispatch_agent_chooser_confirm` -> `prepare_launch_or_error`
- `issues_send::prepare_confirm_send_target` -> `prepare_launch_or_error`
  (dirty-copy and origin-mismatch confirm paths)
- `issues_send::spawn_and_attach_fresh_for_issue` -> `prepare_launch`

Each call executes a real agent subprocess synchronously on the input thread
(measured 2.5 s for LLxprt `nightly`, 6.5 s for Code Puppy). The pre-side-effect
calls exist to reject an unlaunchable configuration *before* prep destroys or
re-clones a working copy; they do not need probe evidence to do that. The result
is a multiplied stall and a multiplied timeout exposure per send.

### Out of scope, tracked elsewhere

- The managed package cache has no cross-process lock and its install is not
  atomic: **#555** (parent), **#556** (Unix), **#557** (Windows).
- Dist-tag selectors never refresh (`nightly` pinned forever): **#554**.

## Acceptance matrix

| ID | Actor / launch path | Input and boundary | Observable success | Observable failure | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|
| A1 | Local probe, package-runner-mediated candidate (uvx) | Runner invocation whose materialization exceeds the authored probe timeout | Probe completes and reports `InstalledCompatible` | Exceeding the materialization budget still yields `AGT-E202` | Probe process only | Definition and authored values unchanged; generation preserved | `runner_mediated_identity_probe_is_not_bounded_by_the_authored_probe_timeout` |
| A2 | Local probe, direct candidate | Direct executable exceeding the authored probe timeout | n/a | `AGT-E202` timeout, unchanged from today | Probe process only | Unchanged | `direct_identity_probe_remains_bounded_by_the_authored_probe_timeout` |
| A3 | Local probe, package-runner-mediated candidate | Capability phase after identity | Capability phase uses the ordinary probe budget | `AGT-E202` on capability timeout | Probe process only | Unchanged | `runner_mediated_capability_probe_uses_the_ordinary_probe_budget` |
| A4 | Local probe, any phase | Probe process exceeds its budget | n/a | Reason names phase, executable, elapsed and budget | Probe process only | Availability shape unchanged (`ProbeError { code, reason, generation }`) | `probe_timeout_reason_names_phase_executable_elapsed_and_budget` |
| A5 | Local probe, any phase | Non-timeout probe failures (truncated stream, invalid UTF-8, malformed framing, identity mismatch, bounds) | n/a | Reason names the phase and executable | Probe process only | Unchanged | `probe_failure_reasons_name_their_phase_and_executable` |
| A6 | Launch validation boundary | Request that is unsupported, has invalid values, an invalid remote, or no resolvable candidate | n/a | Typed `RuntimeError::SpawnFailed` with the existing message | **No probe process and no package install** | Unchanged | `validate_launch_rejects_without_executing_a_probe` |
| A7 | Launch validation boundary | Valid request whose candidate resolves | Validation succeeds | n/a | **No probe process and no package install** | Unchanged | `validate_launch_accepts_without_executing_a_probe` |
| A8 | Issue send, PR send, transient send | Send to a local agent | Every pre-side-effect guard is the probe-free validation boundary, so the authoritative probe runs once per send at spawn | Pre-side-effect rejection still precedes prep with the same message | Working copy untouched when validation rejects | Unchanged | A6 and A7 prove the boundary is probe-free; the five guards are routed to it. `AppStateHandle` is an `iocraft` hook state that cannot be constructed outside a component render, so the guards themselves are covered by compilation and the existing send suites rather than a direct unit test |
| A9 | All existing launch routes | Relaunch, restart, PR send, transient send, non-interactive | Behavior unchanged | Unchanged | Unchanged | Unchanged | Existing suites remain green |
| A10 | Dead module removal | `runtime::llxprt_install` | Build and public surface unchanged for live routes | n/a | None | `RuntimeError::LlxprtInstall` removed with its only producer | Compilation plus existing runtime error tests |

## Non-goals

- Do not add cross-process locking or atomic installation to the package cache (#556, #557).
- Do not change selector normalization, digest derivation, or cache-key refresh semantics (#554).
- Do not change `LOCAL_PROBE_TIMEOUT_MS` or `REMOTE_PROBE_TIMEOUT_MS`.
- Do not change candidate ordering, emitters, argv, env, or cwd composition.
- Do not change remote probe execution.
- Do not add a dependency, a public abstraction, or a new subsystem.
- Do not alter startup availability publication (#526 contract).
- Do not retry, soften, or otherwise make probe failures non-fail-closed.

## Vertical slices

### Slice A — materialization is not charged to the probe budget (A1, A2, A3)

- Owner boundary: `src/domain/agent_definition/limits.rs`, `src/runtime/agent_probe.rs`, `src/runtime/package_runtime.rs`.
- RED: fixture `uvx` runner that sleeps past a short authored `timeout_ms`; probe must succeed. Companion fixture with a direct candidate must still time out.
- GREEN: the identity phase of a runner-mediated invocation (non-empty structural prefix) is bounded by the shared package-materialization budget; every other phase keeps the authored/local budget.
- Stop condition: if this requires knowledge of uv or npm cache internals, stop and report.

### Slice B — AGT-E202 names its phase, executable, elapsed and budget (A4, A5)

- Owner boundary: `src/runtime/agent_probe.rs`.
- RED: assert the reason string content for a timing-out fixture and for a malformed-framing fixture.
- GREEN: carry phase and executable through `ProbeFailure`; render bounded diagnostics.
- Constraint: no secrets, bounded length, no stream contents beyond the existing excerpts.

### Slice C — one send, one authoritative probe (A6, A7, A8)

- Owner boundary: `src/runtime/launch_compose.rs`, `src/app_input/availability.rs`, `src/app_input/issues_send.rs`, `src/app_input/transient_issue_send.rs`, `src/app_input/transient_pr_send.rs`, `src/app_input/prs_orchestration.rs`.
- RED: a hanging-probe fixture proves the validation entry point returns without executing a probe.
- GREEN: extract `validate_launch` (definition, remote validity, support matrix, selector, target, field values, candidate resolution, preflight contract) and route every pre-side-effect guard to it; `prepare_launch` remains the single authoritative probe.
- Stop condition: if any pre-check genuinely needs probe evidence, stop and report.

### Slice D — remove the dead managed-install module (A10)

- Owner boundary: `src/runtime/llxprt_install.rs`, `src/runtime/mod.rs`, `src/runtime/errors.rs`.
- `llxprt_install` has no callers outside its own module and the `RuntimeError::LlxprtInstall` variant; it was superseded by `package_runtime` in #382 / PR #501. It carries a second `INSTALL_LOCK` and a second cache root.

## Scope ledger

| Layer | File | Change | Acceptance rows |
|---|---|---|---|
| domain | `src/domain/agent_definition/limits.rs` | Shared package-materialization budget constant | A1 |
| runtime | `src/runtime/agent_probe.rs` | Per-phase budget selection; phase/executable-aware diagnostics | A1-A5 |
| runtime | `src/runtime/package_runtime.rs` | Install timeout derives from the shared constant | A1 |
| runtime | `src/runtime/launch_compose.rs` | `validate_launch` extraction | A6, A7 |
| runtime | `src/runtime/mod.rs`, `src/runtime/errors.rs` | Remove dead module and its error variant | A10 |
| bin | `src/app_input/availability.rs` and send/relaunch call sites | Route pre-side-effect guards to validation | A8 |

Newly discovered work is recorded here before implementation. Any change outside
this table requires explicit approval.

## Review counters

| Review | Budget | Used |
|---|---|---|
| Open Code Review before PR | 2 | 1 |
| Open Code Review after PR | 2 | 0 |
| Independent review-and-remediation rounds | 2 | 1 |

### Open Code Review run 1 triage (`--from main --to issue553`)

| Finding | Disposition | Action |
|---|---|---|
| Capability phase loses a global probe bound | **Reject** the premise, **In-scope—Fix** the documentation | `origin/main` already re-based `Instant::now()` for the capability deadline, which is the per-process budget #525 deliberately introduced; the refactor preserves it. The combined ceiling is nevertheless larger for a runner-mediated probe, so `LOCAL_PROBE_TIMEOUT_MS` now documents it and `runner_mediated_probe_has_a_finite_combined_ceiling` asserts it |
| Materialization budget should be clamped to the authored probe timeout | **Reject** | Clamping restores the defect this issue fixes: the authored timeout budgets agent startup responsiveness, not a registry download. The resulting AGT-E202 names the budget it exceeded, so the larger bound is visible at the point of failure |
| Overflowing phase deadline collapses into an instant timeout | **In-scope—Fix** | An unrepresentable deadline falls forward instead of backward |
| Fixture `DELAY` placeholder substitution is fragile | **In-scope—Fix** | The delay marker carries the delay; no textual substitution remains |

## Verification evidence

`cargo xtask ci` at the pull-request head, rebased onto `origin/main` at
`de2682b` (#550): fmt, check-clippy-allows, check-source-size,
check-architecture, lint, complexity, coverage (71.78% lines against a 30%
floor), build, and test all pass; 128 test-target results, zero failures.

An earlier coverage run failed
`runtime::session_host_tests::concurrent_staging_of_the_same_image_is_idempotent`.
That test is untouched by this branch (`git diff main...HEAD -- src/runtime/session_host*`
is empty), passed five isolated re-runs, and passed both instrumented and
uninstrumented runs at the candidate head. Recorded as a pre-existing flake in
the deferred findings below rather than treated as a change regression.

## Deferred findings and follow-ups

- #555 / #556 / #557 — cross-process package-cache lock and atomic install.
- #554 — dist-tag selector refresh (in progress, expected to land first).
- Pre-existing flake in `concurrent_staging_of_the_same_image_is_idempotent`
  (session-host staging rename race): #561.
