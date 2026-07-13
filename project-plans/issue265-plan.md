# Issue 265: Linux keyboard behavior

## Intent

Fix the Linux terminal keyboard regressions reported in GitHub issue #265 without pulling the configurable-keybinding work from #185 into this change.

Acceptance behavior:

- In every Issues and Pull Requests inline composer/editor, bare Enter inserts a newline.
- Alt+Enter is the discoverable, terminal-portable submit/save binding.
- Ctrl+Enter remains accepted when a terminal can encode it distinctly.
- Uppercase `S` in Issue Detail always expresses the Send to Agent intent.
- When the selected repository has no eligible agents, `S` produces visible `No agents available` feedback rather than a silent no-op.
- Existing Send to Agent behavior remains unchanged when an eligible agent exists.

## Root cause evidence

On Linux with tmux 3.6:

- `tmux send-keys S` emits byte `0x53`, so tmux transports uppercase `S` correctly.
- `tmux send-keys C-Enter` emits `0x0a`, exactly the same byte as bare Enter. The application therefore cannot reliably distinguish Ctrl+Enter from Enter on this terminal path.
- `tmux send-keys M-Enter` emits `0x1b 0x0a`, so Alt+Enter is distinguishable.

The Issues input resolver currently accepts only Control+Enter for submit. It also suppresses `S` before the typed event pipeline when the global agent list is empty. Repository eligibility is actually known only by the reducer, whose current no-eligible-agent path silently does nothing. `IssuesState::draft_notice` exists, but the Issues screen does not currently project it into its banner.

The existing Unix tmux driver already runs on Linux. No Linux port is required unless the real scenario exposes a separate decoder defect.

## Test-first plan

### RED

1. Add or update pure input tests before production changes:
   - Issues Alt+Enter -> `InlineSubmit`.
   - PR Alt+Enter -> `PrInlineSubmit`.
   - Preserve Ctrl+Enter compatibility tests.
   - Assert bare Enter -> the corresponding newline event in both modes.
   - Change the no-agent Issue Detail `S` expectation from `None` to `OpenAgentChooser`.
   - Preserve the precedence test proving inline-active `S` inserts the character.
2. Add reducer tests:
   - no agents -> no chooser plus `No agents available` notice;
   - an agent exists only for another repository -> the same notice;
   - an eligible selected-repository agent with a stale notice -> clear the notice and open the chooser;
   - the no-eligible-agent path clears any stale chooser.
3. Add pure projection/render tests:
   - every Issue/PR submit and save hint advertises Alt+Enter;
   - Issues banner uses error precedence, otherwise `draft_notice`;
   - a notice-only banner reserves the same layout row as an error banner.
4. Add a strict shipped tmux scenario using literal `S` and `M-Enter`.
5. Run focused tests and the real Linux scenario against unchanged production code; retain failure artifacts as RED evidence.

### GREEN

1. Extend Issues and PR inline key resolvers to submit on Alt+Enter or Ctrl+Enter while leaving unmodified Enter as newline.
2. Remove the global-agent guard from the Issue Detail `S` route so intent always enters the typed event/reducer pipeline.
3. Make the Issues reducer authoritative for repository-scoped eligibility:
   - clear stale chooser and set `No agents available` when no eligible agents exist;
   - clear stale notice and open the chooser when eligible agents exist.
4. Derive one Issues banner value with `error` precedence over `draft_notice`; use it both for rendering and pane sizing.
5. Update all Issues/PR inline submit/save hints to advertise Alt+Enter.
6. Re-run focused tests and the real tmux scenario.

### REFACTOR / VERIFY

- Keep input routing pure, transitions in the reducer, rendering in UI projections, and tmux/process I/O in harness boundaries.
- Do not add platform-specific key maps or user-remappable settings in this issue.
- Run `make quick-check`, then `make ci-check`.

## Safe Linux tmux scenario

Run Jefe with an isolated config and deterministic fixture repository state. Use a fail-closed `gh` shim in an isolated PATH that returns fixture issue list/detail data, records every invocation, and rejects all unknown or mutation commands, especially issue-creation POSTs.

Scenario:

1. Enter Issues mode and wait for fixture issue `#265`.
2. Open detail, press literal tmux key `S`, and wait for/capture `No agents available`.
3. Return to the list, open New Issue, and wait for `Alt+Enter submit`.
4. Press literal tmux key `M-Enter` on an empty draft and wait for/capture `Issue title cannot be empty`.
5. Cancel, quit, and wait for exit.
6. Assert the shim audit contains no mutation and the isolated tmux session is cleaned up.

If fixture setup cannot be made deterministic without expanding the production harness API, keep this as a documented manual scenario plus a test-only runner script/wrapper under the existing harness boundary. Do not use live GitHub mutation behavior as the proof.
