# Issue #536 — Windows send-to-agent delivers only the first line of the prompt

> On Windows the agent's argv is delivered through `cmd.exe /D /S /C <wrapper.cmd> …`
> whenever the resolved executable is a `.cmd`/`.bat` wrapper. `cmd.exe` terminates
> its command line at the first `0x0A`, so a multi-line issue prompt is silently cut
> down to `Read and work on the following GitHub issue.` and the issue number, body,
> comments and delivery-workflow appendix never reach the agent. The cmd.exe bypass
> that already exists for this exact reason (`CanonicalScriptLaunchPlan`, issue #258)
> is never populated on the production launch path.

## Root cause (reproduced)

1. `PathSnapshot::resolve_binary` (the generic resolver used by `AgentCandidateResolver`)
   classifies `llxprt.cmd` as `AgentWrapperKind::CommandScript` and carries no
   canonical script-launch plan. The product-specific `AgentExecutableResolver`,
   which *does* compute one, is no longer on the agent-candidate path.
2. `write_launch_plan` hard-codes `script_launch: None`, so the `Some` branch of
   `base_command_for_payload` — which spawns `runtime + entrypoint + args` with no
   `cmd.exe` — is unreachable in production.
3. `base_command_for_payload` therefore runs `cmd.exe /D /S /C <wrapper> <argv>` and
   the prompt is truncated at its first newline.

`5c1d5fc9` did not introduce the truncation; it made Windows launches succeed where
the `--help` probe previously failed them, which is what exposed the pre-existing defect.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Targets | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | Windows pane launch of a `CommandScript` wrapper carrying the official native-launcher marker and a complete bundled layout | Multi-line prompt argv | Windows local | Worker is spawned as `runtime entrypoint <argv>` with the argv byte-identical to the requested one; no `cmd.exe` in the process chain | n/a | Plan file written and consumed exactly once | Plan JSON keeps its existing shape; `script_launch` is populated instead of `null` | Launcher behavior test asserting program + full multi-line argv |
| A2 | Windows pane launch of a `CommandScript` wrapper with **no** canonical layout | Argv containing `\n` or `\r` | Windows local | none | Typed `CommandScriptArgumentUnsupported`, surfaced through `MultiplexerError::AgentLaunchPlan` → `SendToAgentFailed` | No plan file is left behind; no agent process is started | No partial/truncated launch is ever performed | Behavior test asserting the typed error and that no truncated command is built |
| A3 | Windows pane launch of a `CommandScript` wrapper with no canonical layout | Newline-free argv (`--version`, normal-mode prompts) | Windows local | Existing `cmd.exe /D /S /C` behavior is unchanged, including verbatim-prefix stripping (#525) | Existing diagnostics | Unchanged | Unchanged | Existing `canonical_command_script_payload_uses_launch_safe_path` test stays green |
| A4 | Windows pane launch of a `Direct` or `PowerShellScript` executable | Any argv, including multi-line | Windows local | Existing direct/PowerShell spawn is unchanged and already newline-safe | Existing diagnostics | Unchanged | Unchanged | Direct-wrapper multi-line argv test |
| A5 | Unix pane launch | Any argv, including multi-line | Unix local | Unchanged `execve` argv delivery | Unchanged | Unchanged | Unchanged | Existing Unix pane-command tests stay green |
| A6 | Issue send prompt construction | Issue with number, title, body, comments | All | The composed prompt still contains the issue number, body and delivery-workflow appendix at the point it enters the launch plan | n/a | none | Unchanged | Prompt-composition test proving the content reaching `write_launch_plan` is complete |

## Non-goals

- Changing prompt composition, compaction thresholds, or the pane-command byte budget.
- Re-plumbing `ResolvedCandidate` / `PackageInvocation` / `CandidateEvidence` /
  `AgentLaunchPlan` / `AgentPaneLaunch` to carry a script-launch plan end to end.
- Changing the capability-probe path (`command_for_path`), whose argv is short and
  newline-free, or reverting anything from `5c1d5fc9`.
- Changing remote/SSH launch composition or its quoting.
- Changing candidate resolution order, fingerprinting, or the execution guard.
- Adding a general shell-escaping or argv-encoding subsystem.

## Vertical slices

### Slice 1 — Derive the cmd.exe bypass at the launch-plan boundary

- **Rows:** A1, A3, A4, A5.
- **Owner / boundary:** the Windows private pane launcher and the platform-owned
  executable resolver that already owns canonical-layout knowledge.
- **Allowed paths:** `src/runtime/agent_executable.rs`, `src/runtime/agent_launcher.rs`,
  their existing test modules, and a bounded issue behavior test.
- **RED:** a launcher test spawning a marked wrapper fixture with a multi-line argv
  must show the argument arriving truncated at the first newline.
- **GREEN:** `write_launch_plan` asks the resolver for the wrapper's canonical
  runtime + entrypoint and stores it in the payload's existing `script_launch` field.
  The derivation is pure and reuses the audited `#258`/`#432` layout logic, including
  verbatim-prefix stripping.
- **Non-goals:** no changes to candidate resolution, probing, or plan transport shape.
- **Verification:** focused launcher tests plus `make quick-check`.
- **Stop for approval:** any need to widen `AgentPaneLaunch`/`AgentLaunchPlan`, change
  the probe path, or introduce a new subsystem.

### Slice 2 — Never deliver a silently truncated argv

- **Rows:** A2, A6.
- **Owner / boundary:** the same launch-plan boundary.
- **Allowed paths:** `src/runtime/agent_launcher.rs` and its tests, plus the bounded
  issue behavior test.
- **RED:** an unmarked `.cmd` wrapper with a multi-line argv currently produces a
  truncating `cmd.exe` command instead of an error.
- **GREEN:** that combination returns a typed `CommandScriptArgumentUnsupported`
  naming the gate, leaves no plan file behind, and starts no process.
- **Non-goals:** no fallback that partially delivers the prompt.
- **Verification:** focused tests, `make quick-check`, then the full gate.
- **Stop for approval:** any request to soften the guard into best-effort truncation.

## Expected paths / architectural layers

- `src/runtime/agent_executable.rs` — expose the existing canonical-layout derivation
  for an already-resolved wrapper path.
- `src/runtime/agent_launcher.rs` — populate `script_launch`, add the typed
  no-silent-truncation gate, its `Display` text, and the Windows behavior tests that
  carry a multi-line prompt across the real launch-plan boundary.
- A6 is already covered by `src/app_input/fresh_prompt.rs::prompt_content_is_inlined_as_one_typed_value`,
  which proves the composed prompt reaching the launch signature contains the issue body verbatim.

No new subsystem, public abstraction beyond one pure resolver function, dependency,
workflow, quality-tool change, or unrelated refactor is authorized.

## Scope ledger

| Entry | Status | Reason |
|---|---|---|
| Canonical runtime/entrypoint derivation for a resolved wrapper path | In scope | A1 |
| Populating the existing `script_launch` payload field | In scope | A1 |
| Typed refusal for newline argv with no canonical layout | In scope | A2 |
| Preserving `cmd.exe` behavior for newline-free argv | In scope | A3 |
| Direct/PowerShell/Unix regression protection | In scope | A4/A5 |
| Prompt-content completeness evidence | In scope | A6 |
| End-to-end `script_launch` plumbing through candidate/plan structs | Deferred | Same observable behavior at far higher blast radius; follow-up if probe parity is later required |
| Probe-path (`command_for_path`) cmd.exe removal | Deferred | Probe argv is short and newline-free; not part of this defect |
| Remote/SSH argv quoting | Rejected | Different target and ownership |

## Review and verification ledger

- RED evidence: `marked_wrapper_delivers_multiline_prompt_without_cmd_exe` and
  `unmarked_command_script_refuses_a_newline_argument_instead_of_truncating` both failed
  before implementation (`AgentLauncherError::CommandScriptArgumentUnsupported` did not
  exist, and every written plan carried `script_launch: None`, so the marked wrapper was
  still routed through `cmd.exe`).
- Empirical repro: spawning `%COMSPEC% /D /S /C wrapper.cmd "<multi-line prompt>"` on this
  machine returned `ARG1=[Read and work on the following GitHub issue.]`, reproducing the
  reported symptom exactly. The reporting machine's `llxprt.cmd` carries the native-launcher
  marker and the complete bun/entrypoint layout, so the bypass is applicable there.
- Local verification: `cargo fmt --all` clean; `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean; `cargo test --workspace --all-features
  --no-fail-fast` reports zero failures attributable to this change.
- Known-flaky exclusion: `psmux_attach::native_psmux_attachment_preserves_terminal_contract_and_session`
  failed once on scrollback delivery and passed on immediate re-run. It exercises psmux/ConPTY
  input delivery (issues #438, #546) and touches no launcher code.
- Local OCR: `0 / 2`
- PR OCR: `0 / 2`
- RED evidence: pending
- Fast verification: pending
- Exact-head verification: pending
- Native Windows CI: pending
- Deferred findings: see scope ledger

## Completion contract

Complete only when a multi-line issue prompt provably reaches the spawned Windows
worker byte-intact, no configuration can silently deliver a truncated prompt,
newline-free and non-`CommandScript` launches are unchanged, exact-head local
verification and required CI (including native Windows) pass, reviews are triaged
within their counters, the PR is conflict-free, and every changed file maps to
this ledger.
