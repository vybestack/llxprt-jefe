# Issue 713 plan: restore the host sandbox preflight on every launch path

Issue: https://github.com/vybestack/llxprt-jefe/issues/713
Branch: `issue713`, created from `origin/main` at `76e5d714`

## Outcome

`preflight_or_prompt()` is the single gate every launch path crosses
(`modal_handlers.rs`, `relaunch.rs`, `issues_send.rs`, `prs_orchestration.rs`,
`transient_pr_send.rs`). Since `7b12edcde` its body has been a
`validate_launch_request()` call followed by `return true`, so
`runtime::sandbox_preflight()` has had no production caller. A sandbox-enabled
LLxprt agent therefore launches with a forwarded but empty SSH agent, and every
in-container git operation over SSH fails with `Permission denied (publickey)`.

After this change a sandbox-enabled local LLxprt launch consults the host
sandbox preflight again. A returned `PreflightIssue` opens the existing
`ModalState::PreflightPrompt`, whose `SshAgentNoIdentities` copy and `SshAdd`
remediation are already implemented and already covered by rendering tests.
Confirming the prompt runs the remediation and then re-checks, so a host that is
still not ready re-prompts instead of launching.

## Fixed design decisions

1. The gate is expressed as one pure predicate over the launch request rather
   than restored as an inline condition, so every launch path and the
   post-remediation re-check share one rule and the rule is directly testable.
2. Gating is definition-driven, not agent-kind-driven. The predicate consults
   the active `AgentDefinition`: only a definition that declares a
   `sandbox_enabled` field participates. This preserves the intent the deleted
   `should_run_sandbox_preflight` documented (stale `sandbox_enabled` values
   persisted for a non-sandbox agent must not trigger sandbox preflight)
   without reintroducing the removed `AgentKind` enum.
3. Remote launches are not gated. `sandbox_preflight` inspects the local
   container daemon and the local SSH agent; neither describes the remote host
   that a remote launch actually runs on, so gating a remote launch on local
   host state would block launches for an irrelevant reason.
4. The runtime check keeps its process boundary. Only the pure decision that
   reads `ssh-add -l` output is extracted so the empty-agent recognition is
   provable without mutating the developer's or CI's real SSH agent.
5. Injection stays local: the decision function takes the host check as a
   parameter, and production passes `runtime::sandbox_preflight`. No new public
   trait, module, or subsystem is introduced.

## Acceptance matrix

| ID | Launch path / actor | Input | Observable success | Observable failure and diagnostic location | Evidence |
|---|---|---|---|---|---|
| P1 | every path through `preflight_or_prompt` | local LLxprt request, `sandbox_enabled = true`, host check reports `SshAgentNoIdentities` | the decision yields that issue, so `open_preflight_prompt` sets `ModalState::PreflightPrompt` and the caller aborts the immediate launch | n/a; no launch side effect precedes the prompt | `launch_preflight_issue` unit test with a recording host check |
| P2 | same | same request, host check reports no issue | decision yields `None`; the launch proceeds unchanged | n/a | unit test |
| P3 | gating | `sandbox_enabled` absent or `false` | host check is never consulted; launch proceeds | n/a | recording stub asserts zero consultations |
| P4 | gating | definition declares no `sandbox_enabled` field while values carry a stale `sandbox-enabled = true` | host check is never consulted | n/a | unit test over a non-sandbox shipped definition |
| P5 | gating | `remote.enabled = true` with sandbox enabled | host check is never consulted | n/a | unit test |
| P6 | validation precedence | request that fails `validate_launch_request` | `PreflightIssue::UnsupportedRuntimeOption` is prompted and the host check is never consulted | prompt carries the runtime diagnostic string | unit test |
| P7 | prompt confirmation | user confirms `SshAdd` remediation | the whole `launch_preflight_issue` decision re-runs against the signature as remediation left it; a still-unready host produces the next prompt, a cleared host resumes the launch | remediation error closes the modal and sets `error_message` (existing behavior) | unit tests on the shared decision plus the reachability contract on the re-check call |
| P8 | host observation | `ssh-add -l` exits zero printing `The agent has no identities.` | treated as no identities, which is what produces `SshAgentNoIdentities` | non-zero exit is also treated as no identities | pure unit tests over the extracted listing decision |
| P11 | user creating a sandbox-enabled LLxprt agent, host agent forwarded but empty | new-agent form with Sandbox on, `ssh-add -l` reports "The agent has no identities." | the launch is gated by the SSH-agent prompt before any runtime effect; Esc dismisses it and returns to the dashboard | the prompt names the condition and the remediation | `dev-docs/tmux-scenarios/issue713/sandbox-launch-empty-ssh-agent.json`, proven to time out at `76e5d714` and pass with the fix |
| P10 | gating | sandbox enabled but `sandbox_engine` is unknown or absent | the launch is refused with `UnsupportedRuntimeOption` quoting what the request named; no engine is guessed and no host check runs | the prompt names the field, and no sandboxed agent starts against a host nothing checked | unit tests for the unknown and absent cases |
| P9 | regression class | the launch gate stops handing the host check to its decision, stops re-checking after remediation, or `sandbox_preflight` loses every production use | the build fails the reachability contract naming the lost call | contract failure message names the gate and the consequence | `tests/core/sandbox_preflight_reachability_contracts.rs`, proven to fail at `76e5d714` and pass after the fix |

Persistence and compatibility: no durable schema, no `ModalState` variant, no
`PreflightIssue` variant, and no `PreflightAction` variant changes. Existing
persisted `PreflightPrompt` modals continue to load.

## Non-goals

- No change to the definition-driven `runtime::agent_preflight` engine/image/env
  inspection boundary. It answers a different question (is the declared sandbox
  image inspectable) and remains the ordered pre-effect gate.
- No new `PreflightIssue` variants, remediation actions, or prompt copy.
- No new public trait, inspector, or module; no process/cancellation subsystem.
- No change to how the prompt renders or how its focus behaves. That surface
  already has coverage in `src/selection/overlay_content.rs` and
  `src/app_input/modal_handlers_tests.rs`.
- `runtime::sandbox_ssh_agent_warning` stays as-is. It is a separate non-blocking
  advisory that this issue does not ask to wire up or remove.

## Vertical slices

### S1: restore the gate on the launch path (P1..P7)

RED: `src/app_input/preflight_tests.rs` asserting the gate decision for each
row. GREEN: add the pure `sandbox_preflight_engine` predicate and the
`launch_preflight_issue` decision to `src/app_input/preflight.rs`, call it from
`preflight_or_prompt`, and restore the post-remediation re-check in
`handle_preflight_prompt_enter`.

Allowed paths: `src/app_input/preflight.rs`, `src/app_input/preflight_tests.rs`,
`src/app_input/mod.rs` (test module declaration only).

### S2: provable empty-agent recognition (P8)

RED: unit tests in `src/runtime/preflight.rs` over the extracted listing
decision. GREEN: extract the pure decision from `ssh_agent_has_identities` and
call it from the process boundary.

Allowed paths: `src/runtime/preflight.rs`.

Stopping conditions for both slices: any need for a new public abstraction, a
new issue/action variant, a dependency change, or behavior outside the matrix.

## Expected path ledger

- `src/app_input/preflight.rs`: gate predicate, decision, restored call sites.
- `src/app_input/preflight_tests.rs`: new behavioral tests (P1..P7).
- `src/app_input/mod.rs`: test module declaration only.
- `src/runtime/preflight.rs`: extracted listing decision and its tests (P8).
- `tests/core/sandbox_preflight_reachability_contracts.rs`: reachability contract (P9).
- `tests/core/mod.rs`: module declaration only.
- `dev-docs/tmux-scenarios/issue713/sandbox-launch-empty-ssh-agent.json`: new scenario (P11).
- `scripts/harness-podman-ready-shim.sh`, `scripts/harness-ssh-add-empty-shim.sh`,
  `scripts/harness-ssh-add-loaded-shim.sh`: hermetic host fixtures for those scenarios.
- `dev-docs/tmux-scenarios/issue652/llxprt-sandbox-save.json`: ready-host fixture only; steps and assertions unchanged.
- `dev-docs/testing/scenario-execution-manifest.json`, `dev-docs/testing/scenario-owner-evidence.json`: scenario registration and evidence.
- `dev-docs/testing/issue704-owner-evidence.json`: the two hashes it records for those files.
- `project-plans/issue713-plan.md`: this plan.

No dependency, manifest, `.github`, `.llxprt`, persistence-schema, or
quality-gate change is planned.

## Scope ledger

| Entry | Status |
|---|---|
| S3, source reachability contract (P9) in `tests/core/`, beyond the two slices originally planned. Added because the unit tests prove the gate decides correctly but cannot prove the launch paths still consult it, and losing that one call is the entire defect. Follows the existing precedent in `tests/core/attach_ownership_contracts.rs`, which asserts an equivalent one-line invariant in source for the same reason. | Accepted; no production behavior added |
| S4, TUI scenario coverage (P11) plus the harness fixtures it needs, and a fixture change to `dev-docs/tmux-scenarios/issue652/llxprt-sandbox-save.json`. The original plan recorded "no TUI scenario" on the belief that the gate depends on uncontrollable host state. That was wrong: `src/harness/v1/env.rs` gives every scenario a hermetic environment with `PATH` rooted in the workspace and no `SSH_AUTH_SOCK`, so both the container runtime and the SSH agent are exactly what a scenario installs. The #652 scenario, whose subject is sandbox-value persistence rather than the sandbox host, is restored to its original steps and assertions by installing a ready host. | Accepted; required by the project rule that UI-visible behavior carries scenario coverage, and by the #652 scenario legitimately observing the restored gate |

## Review counters

- Local OCR runs before PR: 1 / 2
- OCR runs after PR opened: 2 / 2 (both run automatically by the repository's
  OCR workflow; its budget comment records 2 of 2 used)

### OCR run 1 triage (local, `76e5d714..4b83a1e5`)

| Finding | Disposition | Action |
|---|---|---|
| `sandbox_preflight_engine` normalized an unrecognized or absent `sandbox_engine` to the `Podman` default, so the gate could inspect one runtime for a request naming another. Confirmed reachable: `validate_launch_request` fingerprints typed values without checking enum membership, so an unknown engine is only refused later, at plan building. | In-scope, fix | Parse strictly and return `None` when the engine does not resolve. New rows P10 and two unit tests. |
| `handle_preflight_prompt_enter` re-ran only the host-check half of the gate while its doc comment claimed it ran the same gate, and `apply_preflight_action` takes the signature mutably between the two. | In-scope, fix | Re-check through `launch_preflight_issue`, so the claim is structural. Reachability contract updated to assert the shared call. |

### Automated OCR triage (PR head `27e6945c`, 9 inline threads)

| Finding | Disposition | Reasoning |
|---|---|---|
| A sandbox-enabled request whose engine does not resolve bypasses preflight entirely, because `validate_launch_request` does not pair `sandbox_enabled` with a valid `sandbox_engine`. | Blocker, fix | Correct, and it is this issue's own defect class. An absent engine in particular is not refused by plan building, so a sandboxed agent could start against a host nothing checked. Replaced `Option<SandboxEngine>` with a three-state `SandboxGate`; an unresolvable engine now produces `UnsupportedRuntimeOption` quoting what the request named. Row P10 rewritten; two tests updated. |
| The podman fixture answers "ready" to any `podman info`, so a change to the arguments preflight sends would leave it silently agreeing. | In-scope, fix | The fixture is new in this change and the failure mode is a scenario that keeps passing while the check it stands for has moved. Now matches the exact argv. |
| `shipped_agent_type(3)` identifies the sandbox-capable definition by registry position. | In-scope, fix | The test's premise is that the definition declares `sandbox_enabled`, so it now finds it by that field, mirroring its sibling test. |
| `uses_check_symbol` skips `use` items, so an aliased import would not be detected as a caller. | Reject | It fails closed. An alias makes the contract report no production caller, which breaks the build and is investigated, rather than passing while the call is gone. |
| The first two contract assertions use substring rather than whole-word matching. | Reject | They match complete call expressions including the argument list, not bare identifiers; the argument list already carries the precision. |
| The empty-agent fixture prints `The agent has no identities.` while the Rust check looks for lowercase text without a period. | Reject | `listing_reports_identities` lowercases before matching and `contains` ignores the trailing period. Covered by a casing unit test and proven end to end by the scenario. |
| `printf ' %s' "$@"` prints a leading space when the argument list is empty. | Reject | Cosmetic, in a diagnostic, and copied from the existing shim convention in `scripts/harness-agent-availability-shim.sh`. |
| `AgentDefinition::shipped()` is scanned on every launch. | Reject | Four elements on a user-initiated launch that is about to spawn a process. |
| The non-sandbox-definition test panics if the premise ever fails. | Reject | A panic with that message is the test failing, which is the intended outcome when its premise dies. |

## Verification evidence

Local, on the candidate head, logs under `tmp/verify713/`:

- `cargo fmt --all --check`: clean.
- `cargo xtask check clippy-allows | source-size | architecture | observation-coercion`: all exit 0.
- `cargo build --workspace --all-features --locked`: clean.
- `cargo test --workspace --all-features --locked`: 7343 passed, 0 failed.
- `cargo xtask coverage`: total line coverage 72.06% against the 30% floor.
- `CLIPPY_CONF_DIR=.github/clippy cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  five findings in `src/domain/sha256.rs`, `src/domain/agent_definition/sha256.rs`,
  `src/workbench/compose.rs` and three in `xtask/src/clippy_policy.rs`. All are
  reproducible on unmodified `76e5d714` (verified by stashing this change) and
  come from a newer local clippy than the pinned CI stable. No finding touches
  a file in this change. CI runs the pinned toolchain.

Scenarios, `scripts/run-scenario-manifest.py --platform macos`:

- `dev-docs/tmux-scenarios/issue713/sandbox-launch-empty-ssh-agent.json`: passes.
- `dev-docs/tmux-scenarios/issue652/llxprt-sandbox-save.json`: passes.
- `dev-docs/tmux-scenarios/paste-enter-escape.json`: passes locally; its CI
  failure on the first PR run was a timeout in the terminal-passthrough
  assertion of a scenario that enables no sandbox, which this change cannot
  reach.

Regression proofs:

- P9: with `src/app_input/preflight.rs` restored to `76e5d714`, all three
  reachability contracts fail; with the fix applied, all three pass.
- P11: with the same file restored to `76e5d714`, the new scenario times out at
  step 20 with "literal 'SSH agent has no identities' not observed"; with the
  fix applied it passes.

## Deferred findings

- (none yet)
