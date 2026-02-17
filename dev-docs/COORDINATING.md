# Subagent Coordination Guide (Rust / Jefe)

This document explains how the coordinator should execute multi-phase plans using subagents.

The goal is to prevent:
- skipped phases,
- invalid batching,
- missing verification gates,
- and context-loss errors in long implementations.

---

## Critical Execution Rules

1. **Never skip phase numbers**
   - If phases are `03..12`, execute exactly `03, 04, 05, ... 12`.
2. **One phase at a time**
   - Each phase has its own worker execution and its own verification.
3. **Verify before proceeding**
   - Phase `N+1` cannot start until Phase `N` verification is PASS.
4. **No multi-phase batching**
   - "Do phases 09-14 together" is not allowed.
5. **Block on failure**
   - Failed phase must be remediated and re-verified before moving on.

---

## Coordination Pattern

```text
Phase N Worker -> Phase N Output -> Phase N Verifier -> PASS/FAIL
  PASS -> Phase N+1
  FAIL -> Phase N remediation -> re-verify
```

Repeat until final phase is complete.

---

## TodoList Requirements (Mandatory)

At plan start, create TODO entries for **every phase** and **every verification phase**.

Example shape:

- `P03` phase work
- `P03a` phase verification
- `P04` phase work
- `P04a` phase verification
- ...

Each todo should include intended subagent assignment.

Do not start execution without a complete phase todo list.

---

## Suggested Subagent Mapping (This Repo)

Use available subagents according to phase type:

- Rust implementation phases: `rustcoder`
- Rust verification/review phases: `rustreviewer`
- Architecture/debug investigation phases: `deepthinker`
- UI copy/layout docs: `uiexpert` or `docwriter`

If a selected subagent times out/fails, retry once or switch to the nearest equivalent specialist.

---

## Worker Launch Template

For each phase, run a worker with:

- explicit phase ID
- prerequisite checks
- strict scope (only that phase)
- concrete deliverables
- forbidden actions (no skipping ahead)

Then run a verifier phase with:
- specific pass/fail checklist,
- structural checks,
- semantic checks,
- explicit PASS/FAIL output.

---

## Phase Prerequisite Check

Before Phase `N`, verify Phase `N-1` is complete.

At minimum, check:

1. Expected artifacts from `N-1` exist
2. Corresponding verification phase passed
3. `.completed/P(N-1).md` has pass evidence

If prerequisite is missing, **stop** and remediate.

---

## Verification Must Be Atomic

Verification output must be phase-local and explicit:

- `Phase 05: PASS`
- or `Phase 05: FAIL` with concrete remediation items

Not acceptable:
- aggregate "phases 03-06 look fine"
- ambiguous status

---

## Failure and Remediation Loop

If phase verification fails:

1. Do not proceed.
2. Create remediation task for same phase.
3. Re-run verification.
4. Repeat until PASS or blocked by external dependency.

If blocked, document blocker and pause plan progression.

---

## Anti-Patterns to Avoid

- Skipping test phases because they look "obvious"
- Jumping to final integration phase early
- Combining implementation and verification in one step
- Modifying future-phase scope in current phase
- Ignoring failed verification and moving forward anyway

---

## Minimum Coordinator Checklist

- [ ] Todos created for all phase and verification steps
- [ ] Phases executed strictly in numeric order
- [ ] Each phase has separate worker and verifier
- [ ] Verification PASS recorded before next phase
- [ ] Failure triggers remediation loop
- [ ] No batching, no skipping
- [ ] Completion markers updated per phase

If any checklist item is false, coordination is not compliant.
