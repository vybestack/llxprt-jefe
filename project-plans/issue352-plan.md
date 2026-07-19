# Issue 352: filled-in getting-started happy path

## Objective

Replace the reference-style in-product portion of `docs/getting-started.md` with one
verified, imperative walkthrough that uses a single internally consistent example
from repository creation through issue handoff and pull-request merge. Generate the
matching publication-safe screenshots through the supported first-agent capture
workflow, using only deterministic local runtime and GitHub fixtures.

## Acceptance matrix

| Row | Actor / launch path | Inputs and boundaries | Observable success | Observable failure / diagnostics | Permitted side effects and compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | New user follows `docs/getting-started.md` | One example named `LLxprt Jefe`, local path `~/projects/llxprt-jefe`, tracker `vybestack/llxprt-jefe`; no alternate path in the main flow | Each section says what to press, what value to enter, what appears, and what to do next, through merge completion | Missing prerequisite or likely focus/auth problem is explained at the step where it matters; advanced choices link to reference docs | Documentation only; no application behavior or persistence format changes | Prose review against the acceptance matrix and current keybinding/form code |
| A2 | Maintainer runs the supported first-agent regeneration command | Absolute nonexistent run root; isolated HOME/config/socket/PATH; real current Jefe and harness binaries; deterministic local LLxprt, Code Puppy, GitHub, and git fixtures | Scenario fills the repository, LLxprt, and Code Puppy forms with the tutorial values and reaches stable semantic checkpoints | Scenario drift or an unrecognized fixture command fails closed and retains private diagnostics | Reads no normal Jefe state, credentials, or remote GitHub resources; writes only run-owned paths and selected docs assets | TUI scenario first; capture and regeneration contract tests; real regeneration run |
| A3 | New user creates the example repository and agents | Repository form has realistic base/tracker paths; LLxprt form uses profile `tutorial`; Code Puppy form uses model `gpt-5.6-sol`; fields irrelevant to the happy path retain safe defaults | Published form screenshots visibly match the prose and show both runtime choices in the same repository | Missing or inconsistent values cause scenario assertions or documentation contract assertions to fail | Existing form semantics only; no runtime option or detection changes | Filled-form scenario captures plus committed SVG/content assertions |
| A4 | New user opens Issues and sends issue 352 to an agent | Deterministic local GitHub fixture serves issue 352; chooser contains the tutorial LLxprt and Code Puppy agents; LLxprt working copy is clean and on its default branch | Issues workspace and send chooser are captured; confirming the selected agent launches it with issue 352 context and produces a stable visible shim result | Unknown GitHub calls, unsafe git state, or launch failure stops the scenario with diagnostics; no network fallback | Local fixture git fetch/checkout and local shim launch only; assignment is handled by the local fixture | Real TUI `waitFor` checkpoints, GitHub fixture audit, issue/send captures |
| A5 | New user opens Pull Requests and merges the resulting PR | Deterministic local fixture serves open, mergeable PR 353 for issue 352, approved with passing checks | PR detail and merge chooser are captured; confirming the documented merge method produces the visible merged result and fixture state refresh | Unknown/incorrect merge command is rejected and the scenario fails without remote mutation | Mutation is confined to a run-root fixture state file; no authenticated GitHub access | Real TUI checkpoints, fixture audit, PR/merge captures |
| A6 | Maintainer promotes and verifies tutorial screenshots | Fixed allowlist of internally consistent semantic captures; 100x32 publication grid and existing redaction/geometry rules | All selected SVG bytes and provenance are promoted transactionally; `check` detects source or asset drift | Missing, unsafe, clipped, stale, or partially promoted assets fail with actionable diagnostics | Replace only allowlisted `docs/assets/first-agent-*` files and provenance | Capture/regeneration tests, committed geometry tests, regeneration `check` |

## Explicit non-goals

- No application behavior, form layout, keybinding, persistence, runtime, or GitHub
  client changes solely for documentation.
- No real LLxprt Code, Code Puppy, GitHub authentication, network request, remote
  mutation, or runtime-version matrix in the capture workflow.
- No exhaustive form option, keybinding, runtime combination, error-state, agent
  lifecycle, installation, or troubleshooting catalog in the main happy path.
- No generalized capture framework, GitHub mocking platform, CMS, automatic prose
  generation, or resurrection of closed PR 279.
- No changes to `.llxprt/`, `.github/`, dependency manifests, quality gates, or
  unrelated tests and documentation.

## Vertical slices

### Slice 1: deterministic filled-in end-to-end capture (RED -> GREEN)

- Rows: A2-A6.
- Owners/boundaries: existing TUI scenario -> issue-specific capture shell boundary
  -> supported regeneration/promotion wrapper. No production Rust architecture.
- RED: update `first-agent-tutorial.json` first with filled forms, both agent kinds,
  Issues send, PR detail, and merge checkpoints; prove the existing capture fixture
  cannot satisfy it. Add contract assertions for the expanded selected asset set and
  fail-closed local GitHub fixture before implementation.
- GREEN: extend the existing bounded capture environment with deterministic local
  Code Puppy, GitHub, and git fixtures; publish and transactionally promote only the
  selected captures; keep the existing redaction, geometry, diagnostics, cleanup,
  and provenance contracts.
- Allowed paths:
  - `dev-docs/tmux-scenarios/first-agent-tutorial.json`
  - `scripts/issue241-capture.sh`
  - `scripts/regenerate-first-agent-tutorial.sh`
  - `scripts/first-agent-tutorial-gh-shim.sh`
  - `tests/issue241_capture.rs`
  - `tests/first_agent_tutorial_regeneration.rs`
  - `tests/first_agent_tutorial_gh_shim.rs`
  - `tests/fixtures/first_agent_tutorial/fake-capture.sh`
  - `dev-docs/testing/tmux-harness.md`
- Verification: focused three integration-test targets, shell syntax checks, one real
  regeneration, and regeneration `check`.
- Stop for approval if the real flow needs production changes, authenticated access,
  a new public abstraction, or behavior not listed in A2-A6.

### Slice 2: imperative tutorial and generated evidence

- Rows: A1, A3-A6.
- Owner/boundary: documentation consuming only observed scenario behavior and
  generated publication assets.
- RED: the Slice 1 scenario/assets establish that current prose and three mostly
  blank screenshots do not cover the accepted path.
- GREEN: rewrite the in-product guide as sequential instructions; link advanced
  material to `docs/overview.md`; promote the verified form, runtime, Issues, send,
  PR, merge, and result screenshots with accurate alt text.
- Allowed paths:
  - `docs/getting-started.md`
  - selected `docs/assets/first-agent-*.svg`
  - `docs/assets/first-agent-tutorial.provenance`
  - this plan
- Verification: links/assets exist, prose identifiers match capture assertions,
  real regeneration output equals committed assets, `make quick-check`, then
  `make ci-check` on the exact candidate head.
- Stop if the observed current UI materially contradicts the accepted walkthrough;
  report the mismatch rather than changing production behavior.

## Expected paths and scope budget

- TUI acceptance: one existing scenario.
- Bounded documentation-production boundary: two existing scripts plus one private
  fail-closed fixture script.
- Behavioral contracts: three integration-test files plus the existing fake-capture fixture.
- Maintainer documentation: one existing harness guide section.
- User documentation: one existing getting-started guide.
- Generated evidence: eight selected SVGs plus one provenance file.
- Delivery ledger: this plan.

Target: no more than 20 changed files and 1,500 net changed lines, including generated
SVG rows. A mandatory scope review occurs before crossing either repository target;
work stops without approval above 40 files or 2,500 net lines.

## Scope ledger

| Discovery | Disposition | Reason |
| --- | --- | --- |
| Issue 343's supported workflow intentionally used one LLxprt shim and no GitHub fixture | In-scope extension | Issue 352 explicitly requires Code Puppy, Issues handoff, PR detail, and merge screenshots through that workflow; deterministic local fixtures preserve its isolation boundary |
| Existing issue 230 fixture proves fail-closed exact GitHub command matching and two-runtime chooser rendering | Reuse pattern, not implementation | The tutorial needs different issue/PR/mutation vectors and must remain self-contained; no live GitHub fallback is permitted |
| Issue sending performs default-branch git preparation and post-launch self-assignment | In-scope fixture behavior | A local bare origin and exact viewer/assignment fixture calls can exercise the current happy path without changing production behavior |
| PR detail fetches metadata, comments, and review threads concurrently and refreshes after merge | In-scope fixture behavior | The fixture must serve the exact current read/mutation vectors and persist merged state only under the run root |

No unapproved scope changes.

## Review counters

- Open Code Review before PR: 2 / 2 attempted; both CLI runs were terminated before emitting output.
- Independent clean pre-PR review: 1; all ten findings triaged and addressed in scope.
- Open Code Review after PR: 0 / 2.

## Verification evidence

- Branch: `issue352`, created from current `origin/main` at `e2ce0fb`.
- RED evidence: the expanded regeneration test retained the old Code Puppy asset, and the real scenario failed when the former capture boundary could not provide the Code Puppy/GitHub path.
- Focused GREEN: GitHub fixture, regeneration, and capture integration targets pass (22 tests total).
- Real tutorial regeneration: completed from isolated local runtime, git, and GitHub fixtures; all eight assets promoted.
- Publication safety: no normal home, temporary run-root, SSH-agent warning, credential-like token, hostname, or real username appears in committed SVGs.
- Regeneration `check`: passes against the recorded source fingerprint and asset object IDs.
- `make quick-check`: passed.
- `make ci-check`: passed before review remediation; rerun required on final exact head.
- Scope: 20 files including generated assets, under 1,500 net lines and within the accepted budget.

## Deferred findings / follow-ups

- The terminal component can emit a non-fatal generational-box panic while rapidly stopping two fixture agents. Application stderr is retained under the run-owned private diagnostics directory so it cannot corrupt publication assets. Production lifecycle correction is outside this documentation issue.
