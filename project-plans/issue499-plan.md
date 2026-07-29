# Issue 499 delivery plan

## Problem statement

`vybestack/llxprt-code` has a GitHub Action for the `/assign` self-assignment
convention (the `assign.yml` + `assign-stale-cleanup.yml` workflows backed by
`assign-issue.sh`, `assign-assignment-history.sh`, `unassign-stale-issues.sh`,
and the shared `assign-constants.sh`). `vybestack/llxprt-jefe` has no such
automation. Issue #499 asks to port the `/assign` automation to this project so
contributors can self-assign issues by commenting exactly `/assign`.

The port is a CI/automation deliverable: no product (Rust) code, no
dependencies, no agent-memory or quality-tool changes. The automation scripts
are bash + `gh` + `jq`, already proven in llxprt-code. This plan ports them
verbatim in semantics while adapting project-scoped identifiers (feedback
marker, stale-cleanup repo guard) to the jefe convention.

## Acceptance matrix

| # | Actor / path | Input / boundary | Observable success | Observable failure / diagnostic | Behavioral test |
|---|---|---|---|---|---|
| A1 | `assign.yml` → `assign` job, `issue_comment` (created) on an **open issue** | Comment body is exactly `/assign` (LF/CRLF/trailing-tab only); commenter is not a bot | Commenter is assigned to the issue; `auto-assigned` label added; sticky feedback comment posted referencing jefe marker | Already assigned / cap exceeded / ineligible / closed → sticky feedback, no assignment | Structural: workflow `if:` matches exact `/assign` (+LF/CRLF/tab) on issue comments, excludes PR comments and bots |
| A2 | `assign.yml` → `assign` job | Eligibility: >=1 merged PR **or** prior durable assignment history | Assignment proceeds | Ineligible → feedback explaining eligibility | Structural: `assign-issue.sh` present and referenced; eligibility helpers present |
| A3 | `assign.yml` → `assign` job | Open-issue cap: <3 open assigned issues | Assignment proceeds; post-mutation cap enforced with rollback | Cap reached → feedback, no assignment | Structural: `MAX_ASSIGNMENTS=3` and cap-check logic present in script |
| A4 | `assign.yml` → `assign` job | Concurrency contention | Deterministic winner election; losers roll back verified | Rollback failure → nonzero exit, state diagnosable | Structural: election/rollback helpers present |
| A5 | `assign.yml` → `record-history` job, `issues` (assigned) | Any assignment event | Per-user history label `asnhist--LOGIN` created/verified (exact definition; collision fails) | API error → nonzero | Structural: job triggers on `issues: assigned`; calls `record-assignment-history.sh` |
| A6 | `assign-stale-cleanup.yml` → `cleanup` job, schedule + `workflow_dispatch` | Open issues with `auto-assigned` label, assignment >14 days, no qualifying linked PR | Stale auto-assignee + label removed; co-assignees preserved; exempt login never removed | API/timeline failure → preserve state, nonzero exit | Structural: `if:` restricts to `vybestack/llxprt-jefe`; script referenced |
| A7 | Shared marker convention | Feedback comments | Use `<!-- jefe-assign-feedback -->` (jefe convention, mirroring `<!-- jefe-ocr-review -->`) | (n/a) | Structural: marker in `assign-issue.sh` is jefe-prefixed |
| A8 | CONTRIBUTING.md | Reader looking for how to self-assign | "Self Assigning Issues" section documents exact `/assign`, eligibility, cap, stale cleanup | (n/a) | Structural: CONTRIBUTING.md contains the section |

## Non-goals

- No changes to Rust product code, `Cargo.toml`, `Cargo.lock`, or dependencies.
- No changes to required product CI (`ci.yml`), the mergeability gate, OCR, or
  PR-review workflows.
- No changes to agent-memory, quality-tool, or `.llxprt/` configuration.
- No backfill file (`.github/assignment-history.txt`) shipped — eligibility via
  the existing per-user label and merged-PR paths is sufficient for the port;
  a backfill is an llxprt-code-specific historical artifact.
- No porting of the llxprt-code JS-based assign harness tests (`assign-*.test.js`).
  Those are JS/vitest artifacts tied to llxprt-code's Node test runner; jefe's
  test layer is Rust. Behavioral coverage here is structural workflow-contract
  tests in the established `tests/core/*_workflow_contracts.rs` pattern.
- No fork-safety changes: `assign.yml` uses `issue_comment`/`issues` triggers
  with `github.token` (the GITHUB_TOKEN), which is safe for self-assignment and
  matches the upstream design. The automation runs from the base-branch
  workflow definition (the code checked out is the default branch tree).

## Planned vertical slices

This is a single cohesive automation port; splitting it across multiple PRs
would ship non-functional fragments (a workflow that calls a non-existent
script). It is delivered as one vertical slice.

### Slice 1 (only slice): port /assign automation + contract test

- **Acceptance rows:** A1–A8
- **Architecture owner:** CI/automation (`.github/workflows/`,
  `.github/scripts/`, `CONTRIBUTING.md`, `tests/core/`).
- **Allowed files:**
  - `.github/scripts/assign-constants.sh` (new)
  - `.github/scripts/assign-issue.sh` (new)
  - `.github/scripts/record-assignment-history.sh` (new)
  - `.github/scripts/unassign-stale-issues.sh` (new)
  - `.github/workflows/assign.yml` (new)
  - `.github/workflows/assign-stale-cleanup.yml` (new)
  - `CONTRIBUTING.md` (new "Self Assigning Issues" section)
  - `tests/core/assign_workflow_contracts.rs` (new)
  - `tests/core/mod.rs` (one module declaration line)
  - `project-plans/issue499-plan.md` (this file)
- **RED:** new contract test fails on current main because the workflows,
  scripts, and CONTRIBUTING section do not exist.
- **GREEN completion criteria:** contract test passes; all six automation
  files present and shellcheck-clean; CONTRIBUTING documents `/assign`;
  `cargo xtask quick` green.
- **Non-goals for this slice:** everything in the issue-level non-goals above.
- **Verification commands:**
  - `shellcheck .github/scripts/*.sh`
  - `cargo test --test integration assign_workflow_contracts`
  - `cargo xtask quick` (fmt, check, test)
  - `make ci-check` (full gate)
- **Stopping conditions:** any requirement to touch `ci.yml`, product Rust
  code, dependencies, OCR/PR-review workflows, agent-memory, or quality-tool
  configuration requires user approval.

## Adaptations from llxprt-code to jefe (project-scoped identifiers)

The automation scripts are ported **verbatim in logic**. Only project-scoped
identifiers change:

1. **Feedback marker** (`assign-issue.sh`): `<!-- llxprt-assign-feedback -->`
   → `<!-- jefe-assign-feedback -->` (mirrors the existing
   `<!-- jefe-ocr-review -->` marker convention in `ocr-review.yml`).
2. **Stale-cleanup repo guard** (`assign-stale-cleanup.yml` `if:`): the
   scheduled-run guard changes from `github.repository == 'vybestack/llxprt-code'`
   to `github.repository == 'vybestack/llxprt-jefe'` so cleanup only runs on the
   canonical upstream jefe repo (not forks).
3. **Feedback stale-cleanup mention** (`unassign-stale-issues.sh` comment):
   none — the exempt-login and threshold constants are identical. The
   `EXEMPT_LOGIN='acoliver'` is the same maintainer for both repos.

Everything else (label names `auto-assigned`, `asnhist--LOGIN`, colors,
descriptions, cap `3`, stale threshold `14`, election algorithm, rollback
logic, concurrency groups) is identical to the upstream proven implementation.

## Scope ledger

- `.github/scripts/assign-constants.sh` — new (A2, A5).
- `.github/scripts/assign-issue.sh` — new (A1–A4, A7).
- `.github/scripts/record-assignment-history.sh` — new (A5).
- `.github/scripts/unassign-stale-issues.sh` — new (A6).
- `.github/workflows/assign.yml` — new (A1–A5).
- `.github/workflows/assign-stale-cleanup.yml` — new (A6).
- `CONTRIBUTING.md` — new "Self Assigning Issues" section (A8).
- `tests/core/assign_workflow_contracts.rs` — new structural test (A1–A8).
- `tests/core/mod.rs` — one module declaration line (test wire-up).
- `project-plans/issue499-plan.md` — this plan.

No newly discovered out-of-scope work. No dependency, agent-memory, or
quality-tool changes. Within the 25-file / 1,500-net-line target.

## Review counters

- Open Code Review (local, before PR): 0 / 2
- Open Code Review (after PR): 0 / 2
- CodeRabbit: pending PR
