# Phase 01 — Domain & Architecture Analysis

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P01
- **Prerequisites:** `.completed/P00A.md` exists with Blocker Gate PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Produce the authoritative domain model and integration analysis for PR Mode, grounded in the
current source, so that pseudocode (P02) and all impl phases have a single source of truth.

## Requirements Implemented (Expanded)

### REQ-PR-001..014 (analysis-level)
- **Requirement:** Every functional requirement must map to concrete domain entities, state fields,
  events, message-bus surface, and integration touchpoints.
- **Behavior contract:**
  - GIVEN the spec, WHEN analysis completes, THEN `analysis/domain-model.md` enumerates every new
    entity (with invariants), the `PullRequestsState` aggregate, the event taxonomy, the message-bus
    surface, transition/side-effect ownership, the edge/error model, and integration touchpoints —
    each traceable to a REQ.
- **Why it matters:** Prevents primitive-obsession and silent gaps; locks ownership boundaries.

## Implementation Tasks

- **Files to create/confirm:** `analysis/domain-model.md` (entities, state aggregate, event
  taxonomy, message-bus surface, ownership table, edge/error model, existing-code-to-modify,
  new-code-to-create, integration touchpoints).
- **Files to modify:** `plan/00-overview.md` tracker (mark P01).

## Verification Commands

P01 is analysis/documentation-only — NO production code is added. The COMPLETE workspace baseline
below MUST still pass to prove the tree remains green BEFORE any impl phase begins (all five commands
apply even though no Rust changed; there is NO RED exception in this phase):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
```

Then the phase-specific documentation checks:
```bash
test -f project-plans/issue20/analysis/domain-model.md
rg -n "PullRequest|PullRequestDetail|PrReview|PrCheck|PrState|PrFilter|PullRequestsState" \
   project-plans/issue20/analysis/domain-model.md
```

## Structural Verification Checklist
- [ ] Every new domain entity listed with fields + invariants.
- [ ] `PullRequestsState` mirrors `IssuesState` field-for-field where applicable.
- [ ] Event taxonomy groups lifecycle/nav/data/filter/inline/agent events.
- [ ] Message-bus surface (`MessageDomain::PullRequests`, `PullRequestsMessage`) described.
- [ ] Ownership table assigns every concern to exactly one layer.

## Semantic Verification Checklist (Mandatory)
- [ ] `IssueComment` reuse is explicit (no duplicate comment type).
- [ ] No new persisted `Repository` field; reuse of `github_repo` + `issue_base_prompt` stated.
- [ ] Staleness model (scope_repo_id + request_id) present on all async responses.
- [ ] Read-only nature of reviews/checks stated; deferred ops via `external_url`.
- [ ] Regression guards (#37/#39/#47/#54/#55/#56) referenced in edge/error or ownership notes.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails (the produced analysis deliverable
must carry no unfinished markers):
```bash
if rg -n "TODO|FIXME|HACK|placeholder|will be implemented" project-plans/issue20/analysis/domain-model.md ; then
  echo "FAIL: deferred-implementation marker present in analysis deliverable"; exit 1
fi
```

## Success Criteria
- `domain-model.md` complete; every REQ traceable to entities/state/events/ownership.

## Failure Recovery
- Amend `domain-model.md`; re-run P01A.

## Phase Completion Marker (`.completed/P01.md`)
Phase ID, timestamp, file list, REQ→entity coverage summary.
