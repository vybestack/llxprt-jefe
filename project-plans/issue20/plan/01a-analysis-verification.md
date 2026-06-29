# Phase 01A — Analysis Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P01A
- **Prerequisites:** `.completed/P01.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the domain model is complete, internally consistent, grounded in current source, and free of
contradictions, with cited evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract"; for a
documentation-only analysis phase, the two code-level items are explicitly marked **N/A — documentation
phase** with the reason (no production code exists yet to read or to trace at runtime), NOT silently
omitted:
1. **Structural verification** — see "Structural Verification Checklist" (entities/state/events/
   ownership present and grouped; matches `00-overview.md`).
2. **Behavioral code-reading evidence (file:line)** — **N/A — documentation phase.** P01 produces
   `analysis/domain-model.md` only; there is no production `src/` code to cite. The analogous
   evidence is each REQ → entity/state/event citation INTO the analysis doc (Semantic checklist).
3. **Runtime-path reachability** — provided at ANALYSIS level only (see "Runtime-Path Reachability
   (analysis-level)"): the intended key → AppEvent → AppMessage → dispatch → apply_prs_message →
   render chain is described and consistent with `component-004.md`. Real per-hop `src/` `file:line`
   tracing is **N/A — documentation phase** (no code yet).
4. **Contradiction scan** — see "Contradiction Scan" (no entity/field contradicts an invariant; no
   ownership concern assigned to two layers).
5. **Atomic verdict** — `Phase 01: PASS` or `Phase 01: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of analysis for REQ-PR-001..014, NFR-001..003
- **Behavior contract:** GIVEN `domain-model.md`, WHEN verified, THEN every REQ maps to named
  entities/state/events/ownership with no ambiguity, and every claim about existing code matches the
  current source (cited file:line).

## Implementation Tasks
- **Files to create:** `.completed/P01A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

Run the COMPLETE verification baseline (P01 is documentation-only, so the workspace must remain
green — ALL commands MUST pass; there is NO RED exception here):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

Then the phase-specific analysis greps:
```bash
rg -n "PullRequestsState|PrFocus|PrDetailSubfocus" project-plans/issue20/analysis/domain-model.md
rg -n "issues_state" src/state/types.rs        # confirm aggregate template still at cited line
rg -n "IssueComment" src/domain/mod.rs         # confirm reuse target exists
```

## Structural Verification Checklist
- [ ] All entities/state/events present and grouped.
- [ ] Ownership table covers reducer/boundary/dispatch/key/UI/persistence.
- [ ] New-files-to-create list matches `00-overview.md`.

## Semantic Verification Checklist (Mandatory)
- [ ] Each REQ-PR-NNN is traceable to ≥1 entity/state-field/event (spot-check 5 REQs with cite).
- [ ] No entity duplicates an existing type that should be reused (`IssueComment`, `GhError`).
- [ ] No persisted-schema change introduced.
- [ ] Edge/error model surfaces (never silently drops) unavailable-context cases.
- [ ] Regression guards explicitly cross-referenced.

## Runtime-Path Reachability (analysis-level)
- [ ] The intended chain key → AppEvent → AppMessage → dispatch → apply_prs_message → render is
  described and consistent with `component-004.md`.

## Contradiction Scan
- [ ] No entity/state field contradicts an invariant (e.g. read-only review yet an edit event).
- [ ] No ownership concern assigned to two layers.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails (the analysis deliverable must carry
no unfinished markers):
```bash
if rg -n "TODO|FIXME|HACK|placeholder|will be implemented" project-plans/issue20/analysis/domain-model.md ; then
  echo "FAIL: deferred-implementation marker present in analysis deliverable"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Additive-only: no existing entity removed/renamed in the model.
- [ ] `prs_state` excluded from persisted mapping is stated.

## Success Criteria
- `Phase 01: PASS` with cited evidence, or `FAIL` with remediation.

## Failure Recovery
- Return to P01; amend `domain-model.md`.

## Phase Completion Marker (`.completed/P01A.md`)
Phase ID, timestamp, cited evidence, verdict, semantic summary.
