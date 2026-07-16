# Issue 241: minimal first-agent tutorial capture

## Objective

Ship the existing first-agent tutorial with current-main evidence from one safe,
repeatable Unix-only run. The implementation is deliberately limited to one
scenario, one LLxprt-named local shim, one run-owned root, plain semantic
captures, and deterministic publication SVGs.

## Acceptance matrix

| Row | Actor / launch path | Input and boundaries | Observable success | Observable failure / diagnostics | Permitted side effects | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Documentation author runs `scripts/issue241-capture.sh capture` | Absolute, nonexistent run root; real Jefe and harness binaries | Root contains isolated home/config, local git fixture, socket directory, evidence, publication SVGs, and a success manifest with exact Jefe commit/version | Existing, relative, home-owned, or unsafe root is refused before creation; failed runs retain `private/diagnostic.txt`, harness failure artifacts, and a failed manifest | Create only beneath the requested root; no network or `gh` calls | Unix script contract tests plus two clean real runs and one final documented run |
| A2 | Existing tmux harness executes `first-agent-tutorial.json` | Fixed 100x32 terminal; literal typing must not append Enter | Real UI adds the repository, then creates the agent without premature form submission | Parser/driver failures identify the exact failed scenario step | Harness-owned tmux session and run-root Jefe state only | Scenario is added first and shown RED because current main has no `type` primitive; focused parser/runner/driver tests then GREEN |
| A3 | Jefe startup and runtime boundaries | Wrapper gives Jefe isolated HOME/config/socket and prepends one deterministic `llxprt` shim | Startup detects LLxprt by its normal PATH probe; launched agent prints ready text, accepts `hello from the tutorial`, and prints the expected response | Missing shim/runtime response fails a stable `waitFor`, preserving partial evidence | Local shim process and Jefe-private tmux socket under run root | Real scenario semantic checkpoints |
| A4 | Beginner follows the documented UI path | Dashboard -> New Repository -> New Agent -> focused terminal -> F12 return | Captures occur only after stable visible assertions and show final dashboard/agent state | No fixed wait is used as evidence; assertion failure records failing step | UI persistence only in isolated config | Scenario review plus successful evidence text |
| A5 | Documentation author publishes selected checkpoints | Six semantic captures with fixed dimensions/theme | Three selected deterministic SVGs have semantic filenames, no host/user/home/token/private-repo content, and are referenced with useful alt text | Publication validation rejects forbidden capture text before rendering/copying | Write run-root publication files; selected reviewed SVGs are committed under `docs/assets` | Script tests, content inspection, and tutorial link check |
| A6 | Documentation author previews or performs cleanup | Manifest and sentinel must identify this run; only exact contained owned paths qualify | Dry-run lists only owned config/home/socket/fixture/private paths; confirmed cleanup removes those and preserves evidence/publication/manifest | Missing sentinel, changed paths, symlinks, or containment failures refuse cleanup | Recursive removal only for exact manifest-listed immediate children beneath this run root; no force operations | Unix cleanup contract tests and real-run dry-run/cleanup checks |

## Explicit non-goals

- No GitHub fixture, authenticated `gh`, remote read/mutation, branch/PR lifecycle,
  or remote cleanup in the capture workflow.
- No real LLxprt or Code Puppy validation, runtime matrix, Windows/psmux capture
  behavior, or retry redesign.
- No general capture framework, process supervisor, publication system, report
  generator, ANSI parser, annotation system, or persistence/runtime/UI refactor.
- No application behavior beyond the current visible first-agent path. The real
  scenario demonstrated one narrow focus invariant missing after form submit;
  this issue fixes only that responsible state boundary plus the harness
  distinction between literal typing and line submit.
- No automatic modification of committed documentation assets. The issue run is
  editorially inspected and selected SVGs are copied into `docs/assets`.

## Vertical slices

### Slice 1: literal scenario typing (RED -> GREEN)

- Rows: A2, A4.
- Owner/boundary: typed harness scenario -> runner -> existing tmux/psmux drivers.
- RED: add the real tutorial scenario with `type` steps and prove current main
  rejects the primitive / cannot drive forms without implicit Enter.
- GREEN: add one `Step::Type` variant that sends tmux literal text without Enter;
  cover parsing, macro substitution, runner dispatch, and both driver argv paths.
- Allowed paths: `dev-docs/tmux-scenarios/first-agent-tutorial.json`,
  `src/harness/{step,expand,runner,tests,runner_tests,tmux_driver,tmux_driver_tests,psmux_driver,psmux_driver_tests}.rs`,
  `dev-docs/testing/tmux-harness.md`.
- Stop if reliable input requires app-state/runtime/persistence changes beyond a
  small responsible-boundary fix.

### Slice 2: bounded local capture and safety

- Rows: A1, A3, A5, A6.
- Owner/boundary: one private Unix shell workflow consuming the real binaries
  and existing harness CLI.
- RED: Unix contract tests for root refusal, failed-manifest truthfulness,
  publication redaction refusal, and manifest-scoped dry-run/cleanup.
- GREEN: add one issue-specific script that prepares the fixture/shim/wrapper,
  runs the scenario, records the exact revision/version and outcome, validates
  publication text, renders fixed SVGs, and performs exact manifest cleanup.
- Allowed paths: `scripts/issue241-capture.sh`, `tests/issue241_capture.rs`.
- Stop if this requires a reusable process, persistence, cleanup, or rendering
  subsystem.

### Slice 3: observed tutorial delivery

- Rows: A1-A5.
- Run the exact command twice from distinct clean roots, inspect semantic text
  and SVGs, then edit prose to describe only observed behavior and commit three
  to five selected images.
- Allowed paths: `docs/getting-started.md`, `docs/assets/first-agent-*.svg`.
- Stop if observed UI differs materially from the required narrative; adjust the
  bounded scenario/tutorial rather than adding infrastructure.

## Expected paths and architecture layers

- Harness model/orchestration boundary: existing `src/harness/*` files above.
- TUI acceptance: one JSON scenario and the existing real tmux CLI.
- Unix documentation-production boundary: one issue-specific script and one
  integration contract test.
- Documentation: one existing tutorial, harness guide update, and three selected SVGs.
- Plan/evidence ledger: this file.

Target: at most 20 changed files and below 1,500 net lines. Generated SVG lines
count toward the budget; reduce selected images before expanding the budget.

## Scope ledger

| Discovery | Disposition | Reason |
| --- | --- | --- |
| Current main includes issue 301 terminal focus/input responsiveness changes | In-scope fix at demonstrated boundary | Real run B routed the first prompt character to dashboard Help because form submit set `terminal_focused` without setting `pane_focus=Terminal`; a deterministic state regression and two clean real runs prove the narrow paired-state fix |
| Initial attach-failure hypothesis | Reject and remove | A focused unit change passed but real run C failed identically; the ineffective experiment was removed rather than retained as speculative hardening |
| Existing harness `line` always presses Enter and cannot fill a form field | In-scope fix | Smallest responsible harness boundary and the prior prototype's useful lesson |
| Harness outer socket uses a per-process `-L` namespace outside the run root | No change | Acceptance's run-root socket is the Jefe runtime socket; broad harness socket redesign is unnecessary because the harness namespace is private and cleaned by its existing guard |
| Prior PR 279 generalized capture, ANSI, GitHub, reports, and cleanup | Reject wholesale reuse | Explicitly outside this issue; only the literal `type` semantics inform Slice 1 |

No unapproved scope changes.

## Review counters

- Open Code Review before PR: 2 / 2 attempted; both external OCR invocations
  were terminated without producing output, so no findings were available to
  triage.
- Open Code Review after PR: 0 / 2.

## Verification evidence

Current implementation evidence (base revision `35cef9ddd4db24b32956de3369113e27b5f78139`):

- RED scenario: parser rejected the initially unsupported `type` step.
- Focused GREEN: 91 harness tests and four capture contract tests passed.
- Real diagnosis: run B exposed terminal input routed to dashboard Help; run C
  disproved the attach-failure hypothesis; run D proved the paired focus fix
  and exposed only a wrapped semantic assertion.
- Two clean capture roots: `/tmp/jefe-issue241-run-g` and
  `/tmp/jefe-issue241-run-h` both succeeded with the real Jefe/tmux path and
  success manifests; all selected SVGs compare byte-for-byte.
- Publication audit: selected output contains no local user/home/repository/token
  text; PID and tmux status content are deterministically redacted.
- `make quick-check`: passed (all workspace tests).
- Full CI-equivalent gates: format, policy, source-size, both Clippy passes,
  coverage (72.75% lines), locked build, and locked tests passed. Two monolithic
  invocations were externally interrupted during coverage compilation, so the
  same exact gates were completed individually to terminal success.
- Final documented capture: `/tmp/jefe-issue241-final-verified` succeeded after
  the full gates; all three committed SVGs compare byte-for-byte with its output.
- PR exact-head CI: pending.

## Deferred findings / follow-ups

None.
