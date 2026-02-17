#  Autonomous Plan-Creation Guide for LLxprt Workers (Rust)

This guide defines how to create reliable implementation plans for this repository.

It is designed to prevent common LLM failure modes:
- fake TDD,
- skipped integration,
- placeholder implementations,
- lint/test quality regressions,
- and plans that do not map to real code paths.

Use this with:
- [`PLAN-TEMPLATE.md`](./PLAN-TEMPLATE.md)
- [`RULES.md`](./RULES.md)
- [`project-standards.md`](./project-standards.md)

---

## Core Principles (Non-Negotiable)

1. **TDD is mandatory**
   - Production code must be written in response to a failing test.
2. **Sequential phases only**
   - Never skip plan phases.
3. **Integration-first thinking**
   - A feature is not complete unless reachable via real app flows.
4. **Modify, don’t fork**
   - Update existing modules; do not create parallel `*v2` implementations.
5. **Strong typing + explicit contracts**
   - No weakly-typed boundary shortcuts.
6. **Lint/test rigor is part of correctness**
   - Quality gates are requirements, not polish.
7. **No fake completion**
   - TODO/HACK/placeholder in implementation phases is a phase failure.

---

## Plan Identity and Phase Execution

Every plan must have an ID:

`PLAN-YYYYMMDD-<FEATURE-SLUG>`

Example:

`PLAN-20260216-PTY-REATTACH-RELIABILITY`

### Required execution order

If plan phases are `03..12`, execution must be:

`P03 -> verify -> P04 -> verify -> ... -> P12 -> verify`

Not acceptable:

`P03 -> P07 -> P12`

### Traceability markers

All substantial implementation artifacts should include traceability markers in Rust docs/tests:

```rust
/// @plan PLAN-20260216-PTY-REATTACH-RELIABILITY.P07
/// @requirement REQ-PTY-003
/// @pseudocode lines 24-39
pub fn ensure_attached(...) -> Result<(), PtyError> { ... }
```

Tests should include plan/requirement tags in the test name or nearby comment.

---

## Required Plan Directory Structure

```text
project-plans/<feature-slug>/
  specification.md
  analysis/
    domain-model.md
    pseudocode/
      component-001.md
      component-002.md
  plan/
    00-overview.md
    00a-preflight-verification.md
    01-analysis.md
    01a-analysis-verification.md
    02-pseudocode.md
    02a-pseudocode-verification.md
    03-<slice>-stub.md
    03a-<slice>-stub-verification.md
    04-<slice>-tdd.md
    04a-<slice>-tdd-verification.md
    05-<slice>-impl.md
    05a-<slice>-impl-verification.md
    ...
  .completed/
    P03.md
    P04.md
```

---

## Phase 0: Specification (Architectural Contract)

Create `specification.md` first.

Must include:
- Purpose/problem statement
- Explicit architectural boundaries
- Data contracts and invariants
- Integration points with existing modules
- Functional requirements (`REQ-*` identifiers)
- Error/edge case expectations
- Non-functional requirements (reliability/performance/operability)
- Testability requirements

No implementation timeline in this file.

---

## Phase 0.5: Preflight Verification (Mandatory)

Before any implementation phase, verify assumptions against the current codebase.

### 1) Dependencies/tools

```bash
cargo --version
rustc --version
cargo clippy --version
```

If coverage is required by plan, verify:

```bash
cargo llvm-cov --version
```

### 2) Type/interface existence

Verify all referenced structs/enums/functions actually exist and match assumptions.

### 3) Call-path feasibility

Verify every planned integration call path is real and reachable.

### 4) Test infrastructure

Verify tests exist (or create explicit phase to add test harness before implementation).

If any preflight check fails, update the plan first.

---

## Phase 1: Analysis

Produce domain and flow analysis artifacts before writing implementation code.

Expected outputs:
- entity/state transition notes,
- edge/error handling map,
- integration touchpoints list,
- explicit "old code to replace/remove" list.

Verification must confirm all requirements are represented.

---

## Phase 2: Pseudocode

Pseudocode must be algorithmic and numbered.

Required format:

```text
21: FUNCTION ensure_attached(target_idx)
22: VALIDATE target_idx in session map
23: IF attached_idx == target_idx AND viewer_alive RETURN Ok
24: TEARDOWN existing viewer
25: WAIT for reader thread termination (bounded)
26: ENSURE tmux session exists
27: SPAWN new attached viewer
28: STORE attached viewer and index
29: RETURN Ok
```

Pseudocode must include:
- validation points,
- error handling,
- ordering constraints,
- integration boundaries,
- side effects.

Implementation phases must reference line ranges.

---

## Implementation Cycle (Per Slice): Stub → TDD → Impl

For each feature slice:

### A) Stub Phase

Goal: compile-safe skeletons and call wiring.

Allowed in stub phases only:
- temporary `todo!()`/`unimplemented!()` where necessary.

Not allowed:
- fake success behavior,
- hidden production shortcuts,
- duplicate modules.

### B) TDD Phase

Write behavior-driven tests for real outcomes:
- input → output behavior,
- state transitions,
- integration behavior,
- error paths.

Never write tests that only assert implementation internals.

### C) Implementation Phase

Implement to satisfy tests and pseudocode steps.

Must:
- update existing modules or clearly justified new modules,
- keep contracts explicit,
- remove stub placeholders introduced earlier,
- keep lint/test/coverage gates passing.

---

## Integration Requirements (Mandatory)

A plan must include integration phases that answer:

1. **Who calls this new behavior?** (exact file/functions)
2. **What old behavior gets replaced?**
3. **How can a user trigger this end-to-end?**
4. **What state/config must migrate?**
5. **How is backward compatibility handled?**

If a feature can be "completed" without touching real integration points, the plan is incomplete.

---

## Verification Layers

Verification is both **structural** and **semantic**.

### Structural verification

- Correct files changed
- Phase markers present
- Tests compile and run
- No skipped phase IDs

### Semantic verification

- Behavior actually exists and is reachable
- Tests fail when behavior is broken
- Requirements are satisfied by real flow
- Integration path works end-to-end

### Required command baseline

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

If coverage gate is enabled:

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines 90
```

---

## Fraud/Failure Patterns to Detect

Reject phase completion if any are present in implementation phases:

- `TODO`, `FIXME`, `HACK`, placeholder comments
- Empty/trivial return behavior used as final implementation
- Tests that pass with non-functional implementation
- Tests asserting only mocks/interactions, not behavior
- Duplicate architecture (`foo_v2.rs`, `new_foo.rs`) instead of integration
- Skipped phase execution

---

## Phase Completion Marker

After each phase, create:

`project-plans/<feature>/.completed/PNN.md`

Include:
- phase ID,
- files created/modified,
- tests added/updated,
- verification command outputs,
- semantic verification summary,
- explicit pass/fail decision.

---

## Plan Evaluation Checklist (Gate Before Execution)

A plan is executable only if all are true:

- [ ] Uses plan ID + sequential phases
- [ ] Preflight verification defined
- [ ] Requirements are expanded and testable
- [ ] Integration points are explicit
- [ ] Legacy code replacement/removal is explicit
- [ ] Pseudocode line references present
- [ ] Verification phases include semantic checks
- [ ] Lint/test/coverage gates are defined
- [ ] No reliance on placeholder completion

If any item is missing, revise the plan before implementation.
