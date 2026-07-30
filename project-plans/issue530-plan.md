# Issue 530 Plan: Preserve the Requested Windows Agent Working Directory

## Scope

Fix the native Windows interactive launch boundary so the staged private session host starts the actual agent process in the immutable launch plan's requested working directory. Preserve psmux pane `-c` metadata, canonical executable identity, wrapper mediation, launch argv/environment, non-interactive behavior, and fail-closed launch semantics.

## Diagnosis

- `AgentLaunchPlan.cwd` contains the requested repository/work directory.
- `src/runtime/commands.rs::local_launch_command` passes that value to `psmux new-session -c`.
- On Windows, psmux starts a staged `jefe-session-host.exe --jefe-internal-agent-launch <plan.json>`.
- `AgentLaunchPayload` serializes executable, wrapper, script launch, argv, and environment, but not cwd.
- `run_launch_plan` builds the agent `Command` and calls `status()` without `Command::current_dir`, so the worker inherits the session-host process cwd. In the reported real path, LLxprt reports `C:\Windows` while psmux reports the requested repository directory.
- The non-interactive boundary already applies `.current_dir(&plan.cwd)`, establishing the expected runtime contract.
- The defect predates #526; recent Windows fixes restored launches and exposed it. Reverting #526/#528/#521 would not add cwd propagation and would reintroduce separate failures.

## Acceptance Matrix

| ID | Actor / path | Input and boundary | Observable success | Failure behavior / permitted side effects | Compatibility / proof |
|---|---|---|---|---|---|
| A1 | Windows private launch-plan writer | Requested absolute cwd containing spaces or non-ASCII characters | Payload serializes and consumes the exact `PathBuf` without lossy conversion | Missing cwd makes the payload malformed; no worker starts | Payload round-trip and command projection tests |
| A2 | Staged Windows session host | Host process cwd differs from requested payload cwd | Actual child process starts with OS cwd equal to the requested directory | Missing/non-directory cwd returns a typed launcher failure before worker spawn | Native Windows child reporter test |
| A3 | Real interactive LLxprt launch | Configured repository is outside `C:\Windows` | LLxprt reports the configured repository directory immediately after focus | Launch must not silently fall back to `C:\Windows`, home, or Jefe startup cwd | Issue #530 psmux TUI scenario RED then GREEN; user performs final real launch before push |
| A4 | Local multiplexer command | Existing `AgentLaunchPlan.cwd` | `psmux new-session -c <cwd>` remains unchanged and the same cwd reaches the private payload | Existing multiplexer errors remain typed | Structural command/multiplexer tests |
| A5 | Windows wrapper launch | Direct, `.cmd`, `.bat`, `.ps1`, and structured script launch | Existing #526 canonical fingerprint and launch-safe wrapper handling remain intact | Existing launch/probe failures remain fail-closed | Existing #525 tests plus focused launcher tests |
| A6 | LLxprt launch options | Profile/mode/resume/sandbox configuration | Existing profile, `--yolo`, `--prompt-interactive`, and `--continue` argv remain unchanged | No field/default remapping | Existing #518/#525 tests and real-process argv evidence |
| A7 | Non-interactive, Unix/tmux, and remote launches | Existing launch plans | Existing cwd behavior remains unchanged | No new fallback or shell interpolation | Cross-platform build/tests and existing scenarios |
| A8 | Lifecycle ownership | Existing session host and Job containment | Cwd correction does not change process containment, recovery, or teardown | No #515 watchdog/reaper behavior is added | Diff review and existing lifecycle tests |

## Non-goals

- Do not change work-directory derivation, normalization, repository persistence, or agent selection.
- Do not remove or rely solely on psmux `-c`; pane metadata and actual process cwd must agree.
- Do not use an LLxprt-specific `--cd` argument instead of setting the OS process cwd.
- Do not change canonical fingerprints, wrapper selection, candidate order, launch flags, remote quoting, or package selection.
- Do not implement #515 lifecycle/watchdog work or #529 version-selector work.
- Do not modify `.llxprt/`, workflows, dependencies, quality tooling, or unrelated tests.
- Do not push or open a PR until the user launches a real local agent and confirms its actual directory.

## Vertical Slice

### Slice 1: Private payload owns actual child cwd (A1-A8)

- Owner: runtime launch-plan transport and private session-host spawn boundary.
- Allowed production paths: `src/runtime/agent_launcher.rs`, `src/runtime/multiplexer.rs`, `src/runtime/session_host.rs`, `src/runtime/commands.rs` only as required to thread the already-owned `AgentLaunchPlan.cwd`.
- Allowed test/evidence paths: focused runtime tests, one issue #530 TUI scenario, and this plan.
- RED: add the issue TUI scenario first and show current main focuses LLxprt with `C:\Windows`; add payload/child-process tests requiring the requested cwd.
- GREEN: serialize cwd in `AgentLaunchPayload`, apply it with `Command::current_dir` before worker spawn, and reject a missing/non-directory cwd before containment/spawn.
- REFACTOR: keep cwd application at one private-launch command boundary and preserve existing structured argv/environment construction.
- Stop for approval if this requires a psmux patch, new process-management subsystem, public launch abstraction, persistence migration, dependency, workflow/tooling change, or any #515/#529 behavior.

## Expected Paths

- `project-plans/issue530-plan.md`
- `dev-docs/tmux-scenarios/issue530/windows-agent-working-directory.json`
- `src/runtime/agent_launcher.rs`
- `src/runtime/multiplexer.rs`
- `src/runtime/session_host.rs`
- `src/runtime/commands.rs`
- Existing focused tests in those modules, unless a small dedicated integration test is necessary for the child-reported cwd

## Scope Ledger

| Discovery | Disposition | Reason |
|---|---|---|
| `plan.cwd` and psmux `-c` are already correct | Accept as evidence | The defect is payload/spawn propagation, not work-dir derivation |
| LLxprt reports `C:\Windows` while psmux reports the repository | Accept / in scope | Proves logical pane cwd and actual worker cwd diverge |
| Private payload lacks cwd | Accept / in scope | This is the narrow transport gap |
| Non-interactive launch already sets `current_dir` | Accept as reference | Establishes parity contract without a new abstraction |
| #526 restored wrappers and exposed this gap | Accept as context | Revert would not fix cwd and would regress launchability |
| #515 session-host owner watchdog | Defer / out of scope | Separate lifecycle subsystem |
| #529 version-selector managed install | Defer / out of scope | Separate package-runtime failure |
| Probe process cwd | Defer unless RED evidence requires it | Identity/help probes do not establish interactive agent cwd |

## Review Counters

- Pre-PR OCR runs: 0 / 2
- Post-PR OCR runs: 0 / 2

## Verification Evidence

| Check | Result |
|---|---|
| Baseline | `issue530` from `origin/main` at `cae19a72` |
| Issue/comments fetched | Complete via `gh issue view 530` |
| TUI RED | Baseline scenario exited 1 while waiting for the focused agent to show `branch-3`; current-main LLxprt had reported `C:\\Windows` in the same native launch path |
| Focused runtime RED/GREEN | Six payload/cwd tests pass; native psmux child reporter passes and records the requested directory |
| Native fixed TUI evidence | Isolated exact-head LLxprt status line shows `~\\projects\\jefe\\branch-3`; proof-owned processes were removed with command-line ownership guards |
| `cargo fmt --all --check` | Pass |
| Strict Clippy | Pass: workspace, all targets/features, warnings denied |
| Locked all-feature build | Pass using isolated target directory |
| Locked all-feature tests | Pass using isolated target directory |
| Isolated exact-head binary | `target/issue530/debug/jefe.exe` built successfully |
| User real-launch cwd confirmation | Pass: user confirmed the isolated exact-head binary launches an actual agent in the actual configured directory |

## Deferred Findings and Follow-ups

- None yet.
