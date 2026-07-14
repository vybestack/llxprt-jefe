# Issue 230: Agent chooser identity and worktree status

## Issue intent

The issue/PR Send to Agent chooser must identify each eligible agent well enough to choose the intended runtime: show the agent name, runtime kind, and configured LLxprt profile or Code Puppy model. The chooser and dashboard agent list must also mark a dirty local working tree with `*`, alongside the branch information. Git probing must remain cached so rendering does not continuously spawn processes.

## Boundaries and design

- Keep chooser eligibility and selection in the state layer; introduce an explicit typed chooser entry rather than extending tuple-shaped state.
- Keep display formatting in pure, iocraft-free projection helpers and consume the same projection in the rendered chooser and selectable text projection.
- Keep local Git process execution in `git_info`; extend its short-TTL worktree metadata cache with dirty status. Remote repositories must not incur SSH worktree probes and must represent dirty status as unavailable/clean for display rather than claiming a remote tree is dirty.
- Preserve issue and PR send orchestration by selecting the target through the typed entry's `AgentId`.
- Use textual symbols only. Render the dirty marker adjacent to the displayed branch suffix, with deterministic handling of detached/non-Git/clean trees.
- Do not persist derived Git status.

## Test-first sequence

1. **RED — TUI scenario first**
   - Add `dev-docs/tmux-scenarios/send-to-agent-details.json` covering the user-visible chooser details and dirty marker expectation in the existing real-TTY harness style.
   - Validate its JSON/harness contract before production edits and record that the existing UI cannot satisfy the new expected labels.
2. **RED — pure chooser projection and state contracts**
   - Add behavioral tests for LLxprt entries (kind + configured profile), Code Puppy entries (kind + configured model), and explicit default text when the configured value is empty.
   - Add tests proving issue and PR choosers retain repository scoping/navigation and send orchestration selects the same `AgentId` after replacing tuples with typed entries.
   - Add selection-overlay parity tests so copied text exactly matches rendered chooser labels.
3. **RED — Git dirty status and agent-list projection**
   - Add `GitRepoInfo` formatting tests for clean and dirty branches and tests using a real temporary Git repository to prove tracked/untracked changes become dirty while clean worktrees do not.
   - Add dashboard agent-list projection/render tests proving `*` appears next to a dirty branch and is absent for clean/unknown status.
   - Add remote-resolution coverage proving no local worktree dirty claim is made for remote repositories.
4. **GREEN**
   - Introduce the typed chooser entry and pure label projection.
   - Wire both issue and PR chooser open/render/selectable-content/send paths to the typed entry.
   - Extend cached local Git metadata resolution with dirty status and include it in the agent-list branch suffix.
5. **REFACTOR and verification**
   - Keep formatting single-sourced, avoid duplicate issue/PR behavior, and preserve module boundaries.
   - Run focused tests during iteration, then `make quick-check`, then the complete `make ci-check` suite.
