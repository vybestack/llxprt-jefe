# Plan Template for Multi-Phase Features (Rust)

Use this template for new plans under `project-plans/<feature>/plan/`.

---

## Plan Header

```markdown
# Plan: [FEATURE NAME]

Plan ID: PLAN-YYYYMMDD-[FEATURE]
Generated: YYYY-MM-DD
Total Phases: [N]
Requirements: [REQ-IDs]

## Critical Reminders

Before implementing any phase:
1. Preflight verification is complete (Phase 0.5)
2. Integration points are explicitly listed
3. TDD cycle is defined per slice
4. Lint/test/coverage gates are declared
```

---

## Required Phase Template

````markdown
# Phase [NN]: [Phase Title]

## Phase ID
`PLAN-YYYYMMDD-[FEATURE].P[NN]`

## Prerequisites
- Required: Phase [NN-1] completed
- Verify previous phase markers/artifacts exist
- Expected files from previous phase: [list]

## Requirements Implemented (Expanded)

### REQ-XXX: [Requirement Title]
**Requirement text**: [full text, not shorthand]

Behavior contract:
- GIVEN: [precondition]
- WHEN: [action]
- THEN: [outcome]

Why it matters:
- [user/system value]

## Implementation Tasks

### Files to create
- `src/...` - [purpose]
  - marker: `@plan PLAN-...P[NN]`
  - marker: `@requirement REQ-XXX`

### Files to modify
- `src/...`
  - [exact change description]
  - marker: `@plan PLAN-...P[NN]`
  - marker: `@requirement REQ-XXX`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines: [X-Y]

## Verification Commands

```bash
# Structural gate
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

(Optional coverage gate if applicable)

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines [NN]
```

## Structural Verification Checklist
- [ ] Required files created/updated
- [ ] No skipped phases
- [ ] Plan/requirement traceability present
- [ ] Tests compile and run

## Semantic Verification Checklist (Mandatory)
- [ ] Feature behavior is present and reachable from real app flow
- [ ] Tests verify behavior, not only internals
- [ ] Error handling behavior matches requirement
- [ ] No placeholder/deferred implementation patterns remain
- [ ] Integration points validated end-to-end

## Deferred Implementation Detection (Mandatory)

```bash
# Reject if these appear in implementation code:
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] Requirement behavior demonstrated
- [ ] Verification commands pass
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: [git restore/checkout commands]
- blocking issues to resolve before next phase: [list]

## Phase Completion Marker
Create: `project-plans/[feature]/.completed/P[NN].md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary
````

---

## Preflight Verification Phase Template (Phase 0.5)

```markdown
# Phase 0.5: Preflight Verification

## Purpose
Verify assumptions before implementation.

## Toolchain Verification
- [ ] `cargo --version`
- [ ] `rustc --version`
- [ ] `cargo clippy --version`
- [ ] `cargo llvm-cov --version` (if coverage gate required)

## Dependency Verification
- [ ] Required crates present in `Cargo.toml`
- [ ] Required features flags confirmed

## Type/Interface Verification
- [ ] Referenced types/functions exist and signatures match assumptions
- [ ] Existing boundaries are reachable from intended call path

## Test Infrastructure Verification
- [ ] Existing test harness for affected modules confirmed
- [ ] If missing, explicit setup phase added before implementation

## Blocking Issues
[List any blockers. If non-empty, stop and revise plan first.]

## Gate Decision
- [ ] PASS: proceed
- [ ] FAIL: revise plan
```

---

## Integration Contract Template (Recommended for multi-component changes)

```markdown
# Integration Contract

## Existing Callers
- `src/...` -> [function path]
- `src/...` -> [function path]

## Existing Code Replaced/Removed
- `src/...` [what old behavior is replaced]

## User Access Path
- [Keybinding/command/screen/API path]

## Data/State Migration
- [state/config transformations required]

## End-to-End Verification
- [test name/command proving integrated flow]
```

---

## Plan Execution Tracker Template

```markdown
# Execution Tracker

| Phase | Status | Verified | Semantic Verified | Notes |
|------:|--------|----------|-------------------|-------|
| P00.5 | ⬜     | ⬜       | N/A               |       |
| P01   | ⬜     | ⬜       | ⬜                |       |
| P02   | ⬜     | ⬜       | ⬜                |       |
| P03   | ⬜     | ⬜       | ⬜                |       |
| ...   | ...    | ...      | ...               | ...   |
```

Update after each phase.
