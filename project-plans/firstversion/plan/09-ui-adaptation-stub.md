# Phase 09: UI Adaptation Stub (toy1-pattern reuse)

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P09`

## Prerequisites
- Required: P08A completed.
- Verify previous markers: `.completed/P08.md`, `.completed/P08A.md`.
- Expected files: runtime/core contracts available.

## Requirements Implemented (Expanded)

### REQ-FUNC-002/003/004/006/008 + REQ-TECH-010
**Requirement text**: Establish UI composition skeleton that reuses toy1 layout and interaction patterns while consuming rebuilt core contracts.

Behavior contract:
- GIVEN rebuilt core boundaries
- WHEN UI stub adaptation is applied
- THEN screen/modals/split/help/search shells exist with typed hooks to core state/events.

Why it matters:
- Preserves operator familiarity while preventing old-core coupling.

## Implementation Tasks

### Files to create
- `src/ui/screens/dashboard.rs`
- `src/ui/screens/split.rs`
- `src/ui/modals/forms.rs`
- `src/ui/modals/help.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P09`
  - marker: `@requirement REQ-TECH-010`

### Files to modify
- `src/ui/mod.rs`
- `src/app.rs` (UI event bridge stubs)
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P09`
  - marker: `@requirement REQ-TECH-001`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-001: 13-37

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] UI adaptation stubs created.
- [ ] toy1-pattern layout semantics represented.
- [ ] UI uses typed core event/state interfaces.

## Semantic Verification Checklist (Mandatory)
- [ ] Focus/pane/split/search/help shells are coherent.
- [ ] No direct UI runtime/filesystem side effects.
- [ ] UI stub path matches ui-mockups expectations.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ui src/
```

## Success Criteria
- [ ] UI adaptation stubs ready for behavior TDD.

## Failure Recovery
- rollback steps: refactor UI shells to restore toy1 pattern alignment + core boundary use.
- blocking issues: missing screen/modal skeletons or boundary leaks.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P09.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary
