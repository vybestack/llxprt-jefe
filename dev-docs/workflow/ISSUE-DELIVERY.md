# Bounded Issue Delivery Workflow

This document is the canonical process for delivering GitHub issues in Jefe. It preserves the project's architecture, TDD, lint, safety, cross-platform, and CI standards while preventing unbounded scope and review loops.

## 1. Shape the issue before implementation

Convert the issue and its comments into a decision-complete acceptance matrix. Each row must identify:

- actor or launch path;
- relevant input and boundary cases;
- local, remote, and platform target where applicable;
- observable success behavior;
- observable failure behavior and diagnostic location;
- side effects permitted before failure;
- persistence and compatibility expectations;
- behavioral test or scenario that will prove the result.

Do not start implementation while a requirement permits materially different architectures. Resolve ambiguous timing, failure, cleanup, retry, or ownership semantics with the user, or explicitly record the stronger behavior as a non-goal.

## 2. Record scope and non-goals

The issue plan must contain:

- the acceptance matrix;
- explicit non-goals;
- planned vertical slices;
- expected files or modules by architectural layer;
- a scope ledger for newly discovered work;
- review counters;
- verification evidence;
- deferred findings and follow-up issues.

Every changed file must map to an acceptance row or an approved scope change. Changes to `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifests, quality-gate scripts or configuration, and unrelated tests or documentation require explicit approval or a separate prerequisite/follow-up change.

## 3. Plan bounded vertical slices

Each slice must deliver one independently testable behavior through the required layers. Avoid assigning the entire issue to one implementation step.

If the planned work crosses more than three architectural ownership layers or more than three orchestration routes, split it into child slices or stacked pull requests before coding.

Each slice defines:

1. acceptance rows implemented;
2. architecture owner and integration boundary;
3. allowed files or modules;
4. RED behavioral test or TUI scenario;
5. GREEN completion criteria;
6. explicit non-goals;
7. verification commands;
8. conditions that require stopping for approval.

## 4. Use auditable TDD

For every slice:

1. Add the smallest behavioral test that fails for the intended reason.
2. For UI-visible work, add or update the TUI harness scenario first and prove RED.
3. Implement only enough production behavior to make the slice GREEN.
4. Refactor within the accepted architecture and scope.
5. Run focused tests and `make quick-check` during iteration.
6. Run the complete required verification before the green checkpoint is pushed.
7. Commit one coherent green behavior.

Tests must verify observable behavior rather than mock call counts. Do not combine unrelated scenarios into a single test merely to avoid test-target or tooling pressure.

## 5. Bound implementation agents

An implementation-agent prompt must include the slice's acceptance rows, allowed paths, tests, architecture boundary, non-goals, and stopping conditions.

The agent must stop and report instead of expanding when the slice appears to require:

- a new process-management, timeout, cancellation, or cleanup subsystem;
- an unplanned public abstraction or production module;
- a workflow, agent-memory, quality-tool, or dependency change;
- unrelated refactoring or test relocation;
- behavior absent from the acceptance matrix;
- exceeding the hard scope budget.

The coordinator presents the alternatives and obtains approval or creates a separate prerequisite/follow-up issue.

## 6. Scope and drift guardrails

These are reviewability triggers, not permission to weaken engineering standards.

### Pull-request budget

- Target: no more than 25 changed files or 1,500 net changed lines, including tests.
- Mandatory scope review above either target.
- Hard stop without explicit approval above 40 files or 2,500 net changed lines.

### Commit budget

- One green behavior per commit.
- Target: no more than 15 files or 800 net changed lines.
- Larger commits require an explicit explanation for renames, generated artifacts, or inseparable behavior.

### Mainline drift

Fetch before each slice and before opening the PR. Pause to rebase or perform a true merge when:

- `origin/main` is more than five commits ahead; or
- main changed a file in the active slice's contract set.

A commit described as a merge must have two parents. Verify that merge-base ancestry advances. If integration introduces more than five issue-unrelated files, stop and restart from current main by applying only the green issue commits. Never reconcile main by copying a snapshot into a one-parent commit.

## 7. Apply quality gates early

Do not loosen lint, complexity, safety, source-size, architecture, coverage, cross-platform, or test requirements.

During each slice, check touched files before they approach the source-size and complexity limits. Before pushing a green checkpoint, run the repository-required format, policy, Clippy, build, test, coverage, architecture, and relevant TUI gates. Cross-platform executable, process, or shell changes require Unix structural-argument tests, Windows resolver/wrapper coverage, remote escaping tests, and native Windows CI at the first slice that introduces the contract.

An interrupted, skipped, stale-SHA, or partial verification command is incomplete, not passed.

## 8. Use bounded review and explicit triage

Open Code Review is capped per issue/PR effort:

- no more than two local OCR runs before the PR;
- no more than two OCR runs after the PR is opened.

Record the counters in the issue plan. Spend runs only on stable, verified checkpoints—not known-broken or rapidly changing code.

Every review finding receives one disposition:

- **Blocker—Fix:** demonstrated correctness, security, data-loss, acceptance, architecture, or mandatory-gate failure;
- **In-scope—Fix:** maintainability required to implement the accepted behavior safely;
- **Reject:** incorrect, duplicative, contradicted by project behavior, or already covered;
- **Defer:** valid improvement outside the accepted issue scope, recorded as a follow-up when appropriate.

A reviewer suggestion is not, by itself, scope authorization. Do not implement speculative hardening, adjacent cleanup, or new behavior merely to exhaust review output. Do not request a fifth OCR run if blockers remain; narrow or split the work, use targeted deterministic evidence, or keep the PR unmerged.

## 9. Exact-head PR readiness

A PR is ready only when:

- every acceptance row has behavioral evidence;
- explicit non-goals remain out of the implementation;
- the scope ledger contains no unapproved change;
- required local verification passes on the candidate head;
- required CI, including native Windows and coverage, passes on the exact head;
- the PR is conflict-free and ancestry is correct;
- all required reviews have completed;
- review output has been read and every finding is fixed, rejected, or deferred;
- OCR counters remain within the cap;
- superseded implementation PRs are closed or clearly marked superseded.

A successful review workflow means the tool executed; it does not mean that the review reported no findings. Rate-limited, canceled, pending, or untriaged review is not approval.

## 10. Stopping rules

Stop and ask for a decision when:

- acceptance language allows materially different architectures;
- a slice requires a new subsystem or unplanned abstraction;
- the hard scope budget is crossed;
- integration ancestry is wrong or unrelated files flood the diff;
- a subagent leaves its approved contract or file boundary;
- required verification cannot complete.

Stop remediation when all accepted correctness, security, architecture, and gate blockers are resolved and remaining findings are explicitly rejected or deferred.

Stop successfully when the acceptance matrix is complete, exact-head verification and CI pass, required reviews are complete and triaged, the PR is conflict-free, and the scope ledger is clean. Do not continue optional hardening or cleanup after these conditions are met.
