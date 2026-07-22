# Issue 364 delivery plan

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/364
- Branch: `issue364`
- Base: `origin/main` at `113873a3b9bf3ae9d79a87f80ab239f14852cd02`
- Delivery: two stacked pull requests, with PR A on `issue364` and PR B on `issue364-embedded`.
- Source reuse: port behavior deliberately from PR #363 / `origin/issue361`; do not merge the stale branch or copy its old `Cargo.toml`/`Cargo.lock` versions.

## Summary

The persistent-shell lifecycle from #361 is on main, but the Terminal Manager portion is not. PR #363 was merged into `issue361` after PR #362 had already squash-merged that branch into main. GitHub therefore marks both PRs merged while current main contains only PR #362.

Deliver the Terminal Manager directly from current main with revised behavior:

- F7 opens a selectable list of every existing embedded shell.
- Browsing a row shows a throttled read-only preview in the lower pane.
- Enter focuses that shell as a live terminal in the same lower pane; it must not expand over the manager.
- F12 defocuses the live shell back to the manager list while preserving the shell.
- F10 resolves and resumes a shell from the selected repository context, including when its owning agent is not currently selected or attached.
- A hidden shell must not replace the entire dashboard footer with one hint. Normal context hints remain visible; the existing F10 token changes to indicate resume when the current repository has a resumable shell.

This follows #361 and supersedes the unlanded Terminal Manager delivery in PR #363. It does not undo the lifecycle behavior from PR #362.

## Verified current state

Current `origin/main` is `f502261`.

- PR #362 merged into main and provides F12 hide, F10 open/resume/close, runtime-only shell inventory, hidden-exit reconciliation, startup adoption/orphan cleanup, and best-effort shutdown cleanup.
- PR #363 is marked merged, but its base was `issue361`, not `main`. Its source commit `2ec7922` and merge commit `4609794` are not ancestors of current main.
- Current main has none of the Terminal Manager files or F7 wiring from PR #363.
- The exact delta from main to the advanced `issue361` branch is 26 files and 1,703 net lines.

The Terminal Manager design in PR #363 remains useful source material but is not the target behavior:

- It has a selectable shell list and targeted static preview.
- Enter attaches/selects the shell, leaves the manager, switches to Dashboard, and presents the shell as a full expanded overlay.
- F12 in the manager list exits to Dashboard.
- Its pending-focus flow is generation guarded, but navigation does not clear a pending request despite the state contract saying it does.
- Its row projection drops an inventory shell completely when its agent or repository record is missing.

## Current root causes and target deltas

| Concern | Current behavior / root cause | Required delta |
| --- | --- | --- |
| Terminal Manager availability | PR #363 did not reach main | Re-deliver the revised manager from a fresh branch based on current main; reuse validated PR #363 concepts rather than merging stale branch history wholesale |
| Interactive lower pane | PR #363 lower pane is a static capture; Enter calls the full Dashboard shell-overlay flow | Keep `DashboardTerminals` active after Enter and render the one attached viewer as a live terminal in the existing lower pane |
| F12 defocus | Manager-list F12 exits; a focused manager-embedded shell state does not exist | While the manager lower terminal owns input, intercept F12 before PTY forwarding, select window 0, preserve the shell, and return input to the manager list |
| Repository-scoped resume | Dashboard F10 resolves only `selected_agent()` and requires that exact owner already be attached | Resolve a shell through the selected repository, drive the existing attach scheduler when necessary, verify/select the existing shell, then resume without creating a duplicate |
| Multiple repositories | Inventory is correctly keyed by agent and already permits shells in many repositories | Keep agent ownership and project repository metadata into manager rows and repository-scoped resolution |
| Multiple shell-owning agents in one repository | No repository-to-shell selector exists | Prefer the selected agent's shell, otherwise the most recently focused shell in that repository; use deterministic AgentId ordering as an adoption/tie fallback |
| Footer collapse | `shell_resume_available` replaces every normal hint with `F10 resume shell` | Preserve the normal context hint set and replace only the F10 shell token with `F10 resume shell` when the selected repository has a resumable target |
| Stale asynchronous focus | PR #363 navigation changes selection without cancelling the pending generation | Navigation, manager exit, and superseding Enter invalidate the prior pending request; late results cannot move focus |
| Unknown/deleted owner | PR #363 `filter_map` hides the shell row | Keep the runtime-owned shell visible as unknown-owner, close-only, so it can be cleaned up |

## Architectural decisions

### Runtime ownership remains agent-based

Keep one fixed `jefe-shell` window per agent session. Do not re-key runtime shells by repository: the window physically belongs to an agent's multiplexer session, and a repository can have several agents.

Repository association is derived through the owning agent. The shell inventory remains runtime-only and is never persisted.

### Repository-to-shell resolution

Add a pure selector with this precedence for the selected repository:

1. If its selected agent owns a resumable shell, use that shell.
2. If exactly one Running agent in the repository owns a shell, use it.
3. If several Running agents own shells, use the most recently focused shell in that repository.
4. If focus recency is unavailable or tied after startup adoption, use deterministic AgentId ordering.
5. If no shell exists, retain today's F10 open-new-shell behavior for the selected Running local agent and its existing validation.

Track focus recency as a monotonic ordinal inside the runtime-only inventory. Preserve ordinals for surviving entries during reconciliation; startup-adopted entries begin with equal/zero recency and therefore use the deterministic tie rule.

Non-Running owners remain visible and close-only; repository F10 never resumes them.

### Shared generation-guarded focus orchestration

Generalize the useful PR #363 focus flow so both manager Enter and Dashboard F10 use one boundary:

1. Resolve an exact shell-owning AgentId and record a generation plus origin (`DashboardF10` or `ManagerEnter`).
2. Update ordinary repository/agent selection so the existing single-viewer attach scheduler targets the owner.
3. Wait for attach confirmation without blocking input.
4. Revalidate generation, expected owner, runtime inventory membership, and shell-window existence.
5. Select the existing `jefe-shell` window. Never create a shell during a resume/focus request.
6. Confirm presentation on the origin's surface.

Matching failures clear the pending request and warn. Unrelated attach outcomes do not cancel the request. Navigation, manager exit, or a newer focus request invalidate the old generation.

### One viewer, two shell surfaces

Represent where the visible shell is presented:

- Dashboard overlay: existing expanded behavior from PR #362.
- Manager embedded: live terminal in the manager's lower pane.

Both reuse the single attached viewer. The manager's browse preview remains a targeted static capture and never becomes a second viewer.

Manager list focus and manager terminal focus are distinct states:

- List focused: Up/Down/Home/End navigate; Enter focuses a Running shell; Ctrl-k closes; Esc/F12 exits the manager.
- Embedded shell focused: ordinary keys go to the PTY; F12 defocuses to the list and preserves the shell; F10 closes only that shell and returns to the list; natural `exit` returns to the list.

F12 defocus must select agent window 0 before changing state so a hidden shell cannot become the dashboard/dead-pane capture source.

## Acceptance matrix

| ID | Actor / inputs | Observable success | Failure and race behavior | Proof |
| --- | --- | --- | --- | --- |
| A1 | Dashboard user presses F7 | Terminal Manager opens from current main and lists every inventory shell with agent, repository, worktree, status, and close-only annotation; zero shells shows an empty state | No runtime mutation just from opening | UI projection tests and TUI scenario |
| A2 | Inventory contains a shell whose agent/repository record is missing | The row remains visible as unknown-owner and close-only; Ctrl-k can close it | It must not disappear from management | Projection and runtime-boundary tests |
| A3 | User navigates Up/Down/Home/End across shells in several repositories | Selection and metadata move; lower static preview clears and then refreshes for the selected shell | Preview results with stale owner/generation/selection are discarded | Reducer/correlation tests and scenario |
| A4 | User presses Enter on a Running shell whose owner is already attached | Manager remains visible; lower pane becomes the live shell at reduced geometry; typed input reaches that PTY | Shell is selected, not recreated | Routing/UI/runtime tests and scenario marker |
| A5 | User presses Enter on a Running shell owned by another agent/repository | Generation-guarded attach completes, exact existing shell is selected, and the live lower pane activates without expanding | Matching attach/select failure warns and restores list focus; unrelated outcomes do not cancel; no duplicate shell | Orchestration tests and two-repository scenario |
| A6 | User navigates/exits or presses Enter on a different row while focus is pending | Prior request is invalidated; late completion cannot jump focus | At most one current request | Reducer and orchestration race tests |
| A7 | Shell window exits externally between Enter and focus completion | Validation fails, warning is shown, and reconciliation removes stale inventory | Never creates a replacement shell | Stub-runtime orchestration test |
| A8 | F12 while manager-embedded shell owns input | F12 is intercepted before PTY forwarding; agent window 0 is selected; shell remains alive; manager stays open with list focus and static preview | Selection failure preserves live focus/state and warns | Routing/reducer tests and marker-survival scenario |
| A9 | F10 while manager-embedded shell owns input | Only selected `jefe-shell` is killed; inventory is removed; manager list remains; selection clamps safely | Kill failure preserves focus/state, warns, then reconciles | Tests and scenario |
| A10 | User types `exit` in manager-embedded shell | Natural-exit observer removes inventory and returns focus to manager list without marking the agent dead | Probe failure retains inventory and retries | Observer tests and scenario |
| A11 | Esc/F12 while manager list owns input | Manager exits to Dashboard and restores prior repository/agent focus; pending focus and preview are cleared | No shell process is killed | Reducer/routing tests |
| A12 | Dashboard F10 in repository context where selected agent owns a hidden shell | That exact shell resumes; current behavior remains compatible | Existing typed warnings remain on failure | Selector regression tests and scenario |
| A13 | Dashboard F10 in a repository with one hidden Running-owner shell but another/no agent selected | Resolver selects its owner, scheduler attaches, and that existing shell resumes | Attach/select failure warns without duplication | Selector/orchestration tests and scenario |
| A14 | Dashboard F10 in a repository with several hidden Running-owner shells | Most recently focused shell resumes; deterministic AgentId fallback applies when recency is tied; F7 remains available to choose another | Non-Running shells are ignored for resume | Pure selector tests |
| A15 | Dashboard F10 in a repository with no hidden shell | Existing open-new-shell behavior for the selected Running local agent is unchanged | Existing missing selection, remote, non-Running, and runtime warnings remain | Existing and focused regression tests |
| A16 | Dashboard footer has a repository-resumable shell | Full normal context hints remain visible; only the F10 shell token reads `F10 resume shell`; footer does not collapse | Width/hint tests continue to pass | Component render test and scenario |
| A17 | Manager list vs embedded shell vs Dashboard overlay footer/Help | Each surface documents its actual keys; Dashboard visible-shell footer remains `F12 hide shell | F10 close shell` | Stale or contradictory help fails tests | Keybind/help tests |
| A18 | Running owner becomes non-Running | Shell remains listed close-only; Enter is disabled with explanation; Ctrl-k and natural exit still work | No attempt to resume a non-Running owner | Projection/input tests |
| A19 | tmux and Windows psmux | New select/capture/kill/list operations use structured `MultiplexerPlan`; F12 never reaches the PTY while either shell surface is focused | Typed runtime errors reach warning paths | Command-shape tests and Windows CI |
| A20 | Restart, shutdown, and persistence | Existing startup adoption/orphan cleanup and shutdown close-all remain green; inventory, recency, surface, manager, preview, and pending focus are not persisted | Cleanup failures do not block quit | Existing lifecycle tests plus persistence round-trip tests |

## Explicit non-goals

- More than one embedded shell per agent.
- Re-keying runtime shell ownership from AgentId to RepositoryId.
- A second attached/live viewer or live-follow-selection while browsing.
- Persisting shell inventory, focus recency, manager state, preview, pending focus, or shell surface.
- Resuming shells owned by non-Running agents.
- Embedded shells for remote repositories.
- Mouse support inside Terminal Manager v1.
- Configurable keybindings (#185), F8 behavior, or agent launch/liveness redesign.
- Changes to startup adoption or shutdown cleanup beyond preserving their existing guarantees.
- Merging PR #363's stale branch history wholesale; reusable code may be ported deliberately onto current main.

## Expected ownership and paths

- Pure state and resolution: `src/state/shell_inventory_ops.rs`, `src/state/shell_focus_resolution.rs` (new), `src/state/terminal_manager_types.rs`, `src/state/terminal_manager_ops.rs`, `src/state/shell_overlay_ops.rs`, `src/state/types.rs`, `src/state/events.rs`, `src/state/mod.rs`
- Typed messages: `src/messages.rs`, `src/messages/terminal_manager.rs`, `src/messages/terminal_manager_conversion.rs`, `src/messages/event_conversion.rs`, `src/messages/names.rs`
- Input/side-effect orchestration: `src/app_input/terminal_manager.rs`, `src/app_input/shell_focus.rs` (new/shared), `src/app_input/shell_overlay.rs`, `src/app_input/normal.rs`, `src/app_input/mod.rs`, `src/app_shell.rs`, `src/app_shell_key_routing.rs`
- Runtime boundary: `src/runtime/shell_window.rs`, `src/runtime/shell_window_tests.rs`, `src/runtime/mod.rs`, and manager/stub seams only if needed
- UI/layout: `src/ui/screens/terminal_manager.rs`, `src/ui/screens/mod.rs`, `src/ui/orchestration.rs`, `src/ui/components/keybind_bar.rs`, `src/ui/components/terminal_view.rs`, `src/ui/modals/help.rs`, `src/layout.rs`, selection/mouse mode arms
- Behavioral proof: `dev-docs/tmux-scenarios/terminal-manager.json`, `scripts/issue361-manager-run-scenario.sh`, and the existing shell-overlay scenario when footer/resume assertions change

Reducers remain deterministic and I/O-free. Runtime/input boundaries perform attach/select/close/capture side effects before dispatching successful state transitions.

## Bounded test-first delivery

Deliver as two stacked PRs to stay within the project scope budget.

### PR A: recover manager browsing, repository resume, and footer composition

1. **Footer slice:** create the keybind render test first and prove RED on current main; preserve normal hints and alter only the F10 token.
2. **Manager browsing slice:** create/revise the TUI scenario first and prove F7 RED on current main; port the list, static preview, navigation, close, exit, typed state/messages, and psmux-safe capture from PR #363; include unknown-owner rows and pending cancellation.
3. **Repository-resume slice:** write pure precedence tests and attach-race tests first; add runtime-only recency and shared generation-guarded focus orchestration; prove two-repository F10 behavior in the scenario.

Expected magnitude: approximately 26–30 files and 1,500–1,900 net lines. This requires the mandatory scope review above the normal 25-file/1,500-line target and must remain below the 40-file/2,500-line hard stop.

### PR B: live manager-embedded shell surface

4. **Surface/geometry slice:** create reducer and layout RED tests; add the manager-embedded surface, reduced PTY geometry, F12 defocus, F10 close, and natural-exit return semantics.
5. **Live render/routing slice:** create overlay-first routing and snapshot-correlation RED tests; reuse the one attached viewer in the lower pane; extend the TUI scenario with in-place Enter, typed marker, F12 defocus/resume, close, and natural exit.

Expected magnitude: approximately 12–16 files and 600–950 net lines.

Stop for explicit approval before adding an unplanned subsystem/public abstraction, changing persistence/dependencies/workflows, exceeding 40 files or 2,500 net lines in either PR, or changing behavior outside this acceptance matrix.

## Overlap and dependencies

- #361 is the parent lifecycle/manager issue. Its lifecycle portion is on main; its Terminal Manager PR is not. This issue supersedes and refines the unlanded manager slice.
- #356 was already closed as superseded by #361.
- #306 and #332 are adjacent attach/restart and Windows teardown work; do not expand into them.
- #253 is a psmux compatibility constraint, not duplicate scope.
- #142 is the DRY view-layer epic; reuse shared list/detail/terminal components where appropriate.
- #185 configurable keybindings is explicitly out of scope.
- #164/#243, #222/#350, and #355/#360 are behavioral precedents for mode-aware F12 and the embedded-shell foundation.

No other open issue found by Terminal Manager, repository shell resume, F10/footer, or interactive embedded terminal searches covers this complete delta.


## Scope ledger

| Discovery | Classification | Disposition |
| --- | --- | --- |
| PR #363 is marked merged but its commits are absent from main | Blocker—Fix, in scope | Port only accepted manager behavior from `origin/issue361` onto current main; preserve version 0.0.30 and current main history |
| Current hidden-shell footer replaces all normal hints | In-scope defect | Fix first with component RED proof; augment the normal hint set instead of replacing it |
| PR #363 drops inventory rows with missing owner metadata | In-scope defect | Render unknown-owner close-only rows and retain cleanup action |
| PR #363 navigation leaves pending focus active | In-scope race | Invalidate pending generation on navigation, exit, and superseding focus |
| Terminal Manager live in-place shell crosses state, input, runtime, layout, and UI layers | Planned stacked PR B | Do not fold into PR A after browsing/repository focus is green |
| Current main is version 0.0.30 while `origin/issue361` predates the release bump | Integration constraint | Never port Cargo metadata from the stale branch |

### PR A mandatory scope review

PR A changes 32 paths and 2,229 net lines including the scenario, runner, plan, tests, and the format-stable theme-picker ownership extraction required to keep normal input routing below the source-size and strict function-size gates. This exceeds the 25-file/1,500-line target, so the mandatory scope review was performed. Every path maps to the accepted footer, manager browsing/preview/close/focus, repository-resolution, typed state/message, runtime capture, screen integration, behavioral proof, or required input-boundary size remediation. No dependency, persistence schema, workflow, `.llxprt/`, or PR B live-terminal changes are included. The candidate remains below the explicit 40-file/2,500-line hard stop, so delivery may continue without additional scope approval.

## Review counters

The issue-wide Open Code Review budget is four runs total: no more than two before a PR is opened and no more than two after PR opening across both stacked PRs.

- Issue-wide pre-PR Open Code Review: 2 attempted / 2; both runs were terminated before producing output, leaving zero-byte result files and no findings to triage.
- Issue-wide post-PR Open Code Review: 2 / 2; both workflow runs completed successfully on PR A, all first-run findings were dispositioned, replied to, and resolved, and the final run left zero unresolved threads. No OCR budget remains for PR B.

## Verification evidence

| Candidate | Command | Result |
| --- | --- | --- |
| base | `git rev-parse HEAD origin/main` | PASS: both `113873a3b9bf3ae9d79a87f80ab239f14852cd02` |
| PR A scenario RED | `scripts/issue364-manager-run-scenario.sh` | RED as intended on base `113873a`: step 2 timed out after F7 because Terminal Manager is absent |
| PR A focused tests | `cargo test -q terminal_manager`, `cargo test -q shell_focus_resolution`, `cargo test -q normal` | PASS: manager 14, selector 3, normal routing 77 across lib/bin targets |
| PR A quick suite | `make quick-check` | PASS: 2,258 library tests, 729 binary tests, all integration targets and doctests |
| PR A scenario GREEN | `scripts/issue364-manager-run-scenario.sh` | PASS: all 80 real-tmux steps; repository resume, two rows, preview navigation, close/clamp, focus/return, natural exit, and empty manager |
| PR A source-size gate | `./scripts/check-source-file-size.sh` after `cargo fmt --all` | PASS: no production source exceeds 1,000 lines |
| PR A exact candidate | `make ci-check` | PASS: format, Clippy policy, format-stable source-size, strict Clippy, coverage, locked all-feature build, and full workspace tests |
| PR B scenario RED | `scripts/issue364-manager-run-scenario.sh` | RED as intended on PR A head `face6c0`: step 73 timed out because Enter replaced the manager with the expanded Agent Shell instead of keeping the list visible |
| PR B focused tests | manager, shell-overlay, and manager-layout filters | PASS: manager state 15, shell-overlay lifecycle 21, and reduced lower-pane geometry coverage |
| PR B quick suite | `make quick-check` | PASS: 2,262 library tests, 729 binary tests, all integration targets and doctests |
| PR B scenario GREEN | `scripts/issue364-manager-run-scenario.sh` | PASS: all 84 real-tmux steps; in-place live shell, typed PTY marker, F12 list return/resume, and natural exit to empty manager |
| PR B exact candidate | `make ci-check` | PASS: format, Clippy policy, source-size, strict all-target/all-feature Clippy, coverage, locked build, full workspace tests, and doctests |

## Deferred findings and follow-ups

- None at implementation start.