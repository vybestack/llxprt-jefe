# Issue #311 Implementation Plan: Bound CodeRabbit review demand

## Issue

GitHub #311 — Add root CodeRabbit demand controls and throttle coverage
visibility.

The issue and its only comment were fetched on 2026-07-15. The comment is the
CodeRabbit issue-planner prompt and adds no acceptance requirements.

## Research and decisions

- The current CodeRabbit v2 schema is
  `https://coderabbit.ai/integrations/schema.v2.json` (JSON Schema draft
  2020-12).
- Current automatic-review documentation says draft PRs are skipped with
  `reviews.auto_review.drafts: false`, title keywords are case-insensitive
  exclusions, and `auto_pause_after_reviewed_commits` values of 1 or 2 are the
  recommended early pause for active branches.
- Current command documentation says `@coderabbitai review` is incremental,
  `@coderabbitai full review` starts from scratch, and each command consumes
  one PR review from the allowance when it runs.
- The root configuration is authoritative for this repository, subject to the
  vendor's documented higher-priority organization/workspace global overrides.
  Contributors can use `@coderabbitai configuration` to inspect the resolved
  source of each setting.
- Jefe has no local or vendored cross-repository measurement ledger. This
  change defines the immutable events and activation rule but does not invent a
  competing ledger. Until the external ledger is available, GitHub review
  comments, checks, and throttle responses remain source evidence rather than
  mutable local counters.
- No `.github/` change is planned. The issue authorizes root CodeRabbit
  configuration and contributor documentation, but not a CI/workflow change.
- This is repository policy, not user-visible TUI behavior; a TUI harness
  scenario would not exercise it and is out of scope.

## Acceptance matrix

| Row | Actor / launch path | Input and boundary cases | Observable success | Observable failure / diagnostic | Side effects and compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | CodeRabbit loads root configuration for a PR | Non-draft PR; draft PR; ready PR with `[WIP]`, `DO NOT MERGE`, or `[skip review]` title; `review-ready`, `wip`, or `do-not-review` label | Adding `review-ready` explicitly triggers review after verification; drafts and marked WIP PRs do not review | CodeRabbit review status/configuration response identifies the resolved setting | No change to organization allowance; global overrides can supersede repository YAML | Structural contract test asserts the nested opt-in controls; current vendor schema validates YAML |
| A2 | CodeRabbit receives pushes after an initial eligible review | One or more follow-up commits, including rapid pushes | Incremental review remains enabled and auto-pauses after two reviewed commits | CodeRabbit pause/status message is evidence; absence of throttle is not coverage proof | Counter semantics remain vendor-owned; cap starts at two | Contract test asserts `auto_incremental_review: true` and cap `2` |
| A3 | Reviewer deliberately requests another pass | Uncovered commits; broad rewrite; no commits since reviewed head; paused review | Incremental command is used for uncovered commits; full command only for broad/uncertain coverage; no duplicate exact-head request | Manual command response or throttle response is recorded as evidence | Every successful manual incremental or full command costs one allowance review | Documentation contract test asserts commands, cost, and exact-head guard |
| A4 | Contributor opens and advances a PR | Active work; local verification failure; exact-head verification success; final push after review | Work is kept draft/WIP; `review-ready` is added only after required local gates pass; final head's coverage is checked | PR review status/checks show pending, skipped, throttled, or stale reviewed head | Existing branch/PR and exact-head delivery rules remain authoritative | Documentation contract test asserts the explicit ready-label lifecycle |
| A5 | CodeRabbit reviews Rust production and test changes | `src/**/*.rs`, Rust tests under `src/` and `tests/`, generated/build output | Source and tests remain eligible; Rust instructions distinguish absence from typed fallible errors, forbid production unwrap/expect, preserve deterministic state, and require behavioral coverage | Missing file feedback is visible in review details; schema validation catches malformed entries | No broad Rust source/test exclusions or TypeScript-derived filters | Structural contract test rejects Rust-scope exclusions and asserts scoped Rust guidance |
| A6 | CodeRabbit reviews Jefe workflow changes | `.github/workflows/**/*.yml` or `.yaml` | Review focuses on untrusted PR boundaries, least privilege, pinned actions, secret handling, and exact-head gates | Review comments identify workflow safety concerns | Guidance only; no workflow file changes | Contract test asserts workflow path guidance |
| A7 | Measurement automation appends review lifecycle evidence once the external ledger is available | Automatic/manual request; completion; throttle; current and reviewed head observations | Append-only request, completion, throttle, and coverage events share request/PR/head identities; late events do not overwrite history | Missing/invalid events remain visible as an incomplete ledger sequence | No local mutable counters and no inference that no throttle means coverage | Documentation contract test asserts immutable event types and fields |
| A8 | Maintainer evaluates whether to tune demand controls | Complete and incomplete rolling windows; throttle without coverage; coverage without throttle; mixed or unknown configuration | Adjacent complete 28-day windows are compared after a fixed settling period; throttle rate and exact-head coverage are evaluated by resolved configuration | Partial, mixed, or unknown-configuration windows are non-comparable and zero denominators are not applicable | No mid-window tuning; no promise of zero throttling | Documentation contract test asserts cohort timestamps, adjacent windows, settling, and metric handling |

## Non-goals

- Do not change CodeRabbit organization/workspace allowance or promise zero
  throttling.
- Do not disable incremental review.
- Do not exclude production source or tests merely to lower demand.
- Do not treat the absence of a throttle message as proof of reviewed coverage.
- Do not add a local substitute for the separately owned publication ledger.
- Do not modify GitHub Actions, the PR template, dependencies, runtime code, or
  TUI behavior.
- Do not copy TypeScript, JavaScript, Node, mobile, or generated-file filters
  from vendor examples.

## Vertical slices

### Slice 1 — Executable repository-policy contract

- **Rows:** A1–A8.
- **Owner/boundary:** Repository integration test over version-controlled
  configuration and contributor policy.
- **Allowed file:** `tests/coderabbit_policy.rs`.
- **RED:** The test fails because `.coderabbit.yaml` and the demand policy do
  not exist.
- **GREEN:** The test verifies explicit review controls, preserved source/test
  scope, repository-specific path instructions, manual allowance semantics,
  immutable event definitions, and complete-window tuning rules.
- **Verification:**
  `cargo test --test coderabbit_policy -- --nocapture`.

### Slice 2 — Root demand controls

- **Rows:** A1, A2, A5, A6.
- **Owner/boundary:** CodeRabbit's repository-root configuration.
- **Allowed file:** `.coderabbit.yaml`.
- **GREEN:** Root YAML requires a positive `review-ready` opt-in, excludes
  drafts/WIP, keeps incremental review enabled, pauses after two reviewed
  commits, avoids Rust source/test exclusions, and supplies Rust/Jefe workflow
  path instructions.
- **External validation:** Convert YAML to JSON with Ruby's standard `yaml` and
  `json` libraries, then validate against a freshly fetched current v2 schema
  using AJV draft 2020 mode. Record schema URL and SHA-256 in verification
  evidence; do not vendor generated schema output.

### Slice 3 — Deliberate review lifecycle and measurement policy

- **Rows:** A3, A4, A7, A8.
- **Owner/boundary:** Contributor process documentation.
- **Allowed files:** `dev-docs/code-review-demand.md`, `CONTRIBUTING.md`.
- **GREEN:** Contributors have an explicit draft-to-ready-label lifecycle,
  manual rerun cost/use policy, exact-head coverage check, append-only ledger
  event contract, and reproducible adjacent rolling-window comparison rule.
- **Verification:** Focused contract test plus link validation by inspection.

## Expected files

| File | Acceptance rows | Purpose |
| --- | --- | --- |
| `project-plans/issue311-plan.md` | All | Decision-complete plan, scope ledger, and evidence |
| `tests/coderabbit_policy.rs` | All | Behavioral repository-policy contract |
| `.coderabbit.yaml` | A1, A2, A5, A6 | Authoritative vendor configuration |
| `dev-docs/code-review-demand.md` | A3, A4, A7, A8 | Team and automation lifecycle policy |
| `CONTRIBUTING.md` | A4 | Contributor entry-point link and concise ready rule |

## Scope ledger

| Item | Status | Decision |
| --- | --- | --- |
| Root CodeRabbit configuration | Accepted | Direct issue requirement |
| Repository policy integration test | Accepted | TDD evidence for config/documentation behavior |
| Contributor demand/measurement guide | Accepted | Direct issue requirement |
| Contributor-guide link | Accepted | Makes deliberate behavior discoverable |
| Repository `review-ready` label | Accepted | Required external trigger for the configured explicit opt-in lifecycle |
| GitHub Actions or PR-template enforcement | Excluded | `.github/` changes require separate explicit approval and are unnecessary for acceptance |
| Cross-repository ledger implementation | Excluded | Separately owned publication infrastructure is not present |
| Runtime or TUI changes/scenarios | Excluded | No user-visible application behavior changes |
| Dependency changes | Excluded | One-off schema validation uses available tools without manifest changes |

## Review counters

- Local Open Code Review runs before PR: 1 / 2.
- Open Code Review runs after PR: 0 / 2.
- Rust reviewer runs: 2.

## Verification evidence

The current vendor schema was fetched from the documented URL on 2026-07-15.
Its SHA-256 was
`d57478bfb748dbdf5d0509367f90d9c7fac65d8436639cfbec1cbeec77bf4b30`.

| Candidate head | Command / review | Result |
| --- | --- | --- |
| Worktree before implementation | `cargo test --test coderabbit_policy -- --nocapture` (RED) | Expected failure: all four tests reported missing config/policy files |
| Worktree before review fixes | `cargo test --test coderabbit_policy -- --nocapture` | Passed: 4 tests |
| Worktree before review fixes | Current CodeRabbit v2 JSON Schema validation via AJV draft 2020 | Passed: converted `.coderabbit.yaml` is valid |
| Worktree before review fixes | `make quick-check` | Passed |
| Worktree before review fixes | `make ci-check` with current rustup toolchain first on `PATH` | Passed; stale Homebrew Clippy 1.92 was shadowing rustup Clippy 1.97 |
| Worktree before review fixes | rustreviewer | Five actionable findings; all addressed in the current candidate |
| Worktree after strengthened contracts | `cargo test --test coderabbit_policy -- --nocapture` (RED) | Expected failure: explicit trigger, scoped Rust guidance, and reproducible measurement semantics were absent |
| Current candidate | Open Code Review | One medium finding fixed; one false finding rejected because `path_filters` is an optional section intentionally inspected for future exclusions; one parser-dependency suggestion rejected as an unapproved dependency expansion |
| Current candidate | `cargo test --test coderabbit_policy -- --nocapture` | Passed: 5 tests |
| Current candidate | Current CodeRabbit v2 JSON Schema validation via AJV draft 2020 | Passed |
| Current candidate | `make ci-check` with current rustup toolchain first on `PATH` | Passed after all review fixes |
| Current candidate | Live repository label lookup | `review-ready`, `wip`, and `do-not-review` exist with documented descriptions |

## Review finding triage

- **Blocker—Fix:** Replaced implicit ready behavior with the documented positive
  `review-ready` label trigger and disabled global automatic review.
- **In-scope—Fix:** Defined cohort timestamps, adjacent windows, settling,
  effective configuration fingerprints, eligibility snapshots, terminal states,
  zero denominators, late events, and non-comparable mixed configurations.
- **In-scope—Fix:** Strengthened tests to inspect nested YAML sections and
  path-scoped guidance, added descriptive assertions, and limited path-filter
  rejection to Rust production/test exclusions without adding a dependency.
- **In-scope—Fix:** Replaced bare `WIP` substring matching with `[WIP]`.
- **In-scope—Fix:** Clarified that `Option` models absence while `Result` with a
  typed error models fallible operations.
- **In-scope—Fix:** Changed Rust-scope filter matching to complete path
  components and added positive/negative regression cases.
- **Reject:** OCR interpreted the deliberate optional `path_filters` inspection
  as a typo for `path_instructions`; the test separately inspects path
  instructions and must guard future exclusions even though the current section
  is absent.
- **Reject:** A general YAML parser would add an unapproved dependency. The
  narrow section reader is paired with current-schema AJV validation and tests
  only repository-owned formatting.
- **Blocker—Fix:** Created the live `review-ready`, `wip`, and `do-not-review`
  repository labels required by the documented lifecycle.
- **In-scope—Fix:** Defined measurement cutoff `T`, publication `P = T + 7d`,
  the as-of boundary, and post-publication corrections.
- **In-scope—Fix:** Made terminal coverage membership depend on any historical
  qualifying ready/opt-in observation and deduplicated repeated readiness cycles.
- **In-scope—Fix:** Required the initial configuration to omit `path_filters`
  entirely and asserted every configured WIP title marker.
- **In-scope—Fix:** Made repository read failures name the missing path, parsed
  YAML keys and path entries without fixed quotes/indentation, and constrained
  documentation assertions to their owning Markdown sections.

## Deferred findings and follow-ups

None.
