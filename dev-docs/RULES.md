# Development Rules for LLxprt Contributors (Rust)

This document is the operational rulebook for contributors and LLM agents.

These rules apply to all implementation work in this repository.

---

## 1) Core Rule: TDD Is Mandatory

Every production change must follow RED -> GREEN -> REFACTOR discipline.

- RED: write a failing test for behavior
- GREEN: implement minimal code to pass
- REFACTOR: improve design if it increases clarity/maintainability

No production-only commits without corresponding test intent.

---

## 2) Language and Quality Baseline

Primary language: Rust.

Required baseline:
- `cargo fmt --all --check` passes
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- `cargo test --workspace --all-features` passes

Project strictness from `project-standards.md` is binding.

---

## 3) Rust-Specific Coding Rules

### Type safety
- Prefer explicit domain types over primitive obsession.
- Use enums/structs for state and event contracts.
- Avoid stringly-typed control flow.

### Error handling
- Use `Result`/`Option` and typed errors.
- No `unwrap`/`expect` in production paths.
- Surface context-rich errors at module boundaries.

### Safety
- `unsafe` is forbidden unless explicitly approved and isolated.

### State and side effects
- Keep side effects at boundary modules.
- Keep reducers/state transition logic deterministic.

---

## 4) Architecture Rules

Must preserve module boundaries:
- UI layer renders and emits intent
- App state/event layer owns transitions
- Runtime layer owns tmux/PTY orchestration
- Persistence layer owns file I/O and schema/version validation
- Theme layer owns theme loading and fallback logic

Do not bypass boundaries with convenience calls.

Do not create parallel architecture variants (`*_v2`, `new_*`) unless explicitly approved.

---

## 5) Testing Rules

### What tests must verify
- Behavior (inputs -> outputs)
- State transitions and invariants
- Error and edge paths
- Integration across real module boundaries

### What tests must avoid
- Pure implementation-detail assertions as primary proof
- "exists/defined" assertions without behavioral value
- mock-only theater for integration claims

Regression tests are required for bug fixes.

If property-based testing is introduced for a module, keep it meaningful and deterministic.

---

## 6) Anti-Placeholder Rule

Implementation phases must not leave deferred-completion markers:

Forbidden in implementation code:
- TODO/FIXME/HACK placeholders
- "for now" or "will be implemented" comments
- empty/trivial return behavior used as final implementation

If a phase needs stub behavior, isolate that to explicit stub phases only.

---

## 7) Persistence Rules (v1)

Persistence is file-based only:
- `settings.toml`
- `state.json`

Requirements:
- schema/version aware
- validated reads
- atomic writes
- safe fallback on malformed/missing files

SQLite is not part of v1 scope.

---

## 8) Theme and UX Rules

- Green Screen is default and fallback theme.
- No bright/light default palette.
- Keyboard behavior must remain explicit and predictable.
- Terminal focus semantics (`F12`) must stay clear and reversible.

---

## 9) LLM-Specific Rules

LLM contributors must:
- follow existing architecture/style patterns,
- avoid speculative abstractions,
- avoid introducing weak typing,
- avoid disabling lint rules,
- include tests and docs updates with behavioral changes.

Before claiming completion, provide:
1. files changed,
2. behavior implemented,
3. tests added/updated,
4. validation commands and outcomes.

---

## 10) Verification Checklist Before Merge

- [ ] Tests prove behavior for changed requirements
- [ ] Lint/format/test gates all pass
- [ ] No prohibited placeholder/debt markers in implementation
- [ ] Boundaries remain intact
- [ ] Docs updated for contract changes
- [ ] No hidden regressions in keyboard/runtime behavior

If any box is unchecked, do not merge.
