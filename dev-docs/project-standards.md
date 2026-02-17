# Jefe Project Standards

This document defines mandatory development standards for Jefe.

Audience:
- human contributors,
- LLM contributors,
- reviewers.

These standards are normative. "Must" and "must not" are requirements.

---

## 1) Core Engineering Principles

1. Keep architecture modular and boundary-respecting.
2. Prefer strong typing and explicit domain modeling over shortcuts.
3. Keep runtime behavior deterministic and testable.
4. Optimize for maintainability over cleverness.
5. Treat lint/test/docs as part of the feature, not optional polish.

---

## 2) Architectural Discipline

Contributors must respect module ownership:

- UI modules render state and capture intent.
- App state/event modules own state transitions.
- Runtime orchestration modules own tmux/PTY behavior.
- Persistence modules own file I/O and schema handling.
- Theme modules own theme parsing/selection/fallback logic.

Do not move side effects into presentational UI components.
Do not bypass state/event contracts with ad-hoc mutation.

---

## 3) Rust Language Standards

## Safety and correctness
- `unsafe` is forbidden unless explicitly approved and isolated.
- Production paths must not rely on panic-driven control flow.
- Use `Result`/`Option` and typed error propagation.

## Strong typing
- Do not introduce weak/dynamic types where enums/structs are appropriate.
- Do not use stringly-typed status or event channels when typed enums exist.
- Public module APIs must have explicit types and clear contracts.

## Error handling
- No `unwrap`/`expect` in production paths.
- If an error is recoverable, handle it explicitly.
- If unrecoverable for a user action, return context-rich typed errors.

## Documentation
- Public structs/enums/functions in core modules must have clear doc comments.
- Update docs when contracts/behavior change.

---

## 4) Linting and Complexity Rules (Mandatory)

Jefe follows strict lint/complexity enforcement consistent with toy build standards.

Contributors must keep project lint config active and respected.

## Required lint posture
- `cargo clippy` must pass with project-configured strictness.
- Rust lint configuration in `Cargo.toml` must not be weakened.
- Clippy thresholds in `clippy.toml` must not be raised casually.

## Current complexity guardrails
- Cognitive complexity threshold: 15
- Function lines threshold: 60
- Function argument threshold: 6
- Type complexity threshold: 250
- Struct bool field cap guidance: 3

## Explicit prohibitions
- Do not disable lints globally.
- Do not add blanket `#[allow(...)]` at module/file scope to silence debt.
- Do not merge code that passes only by muting warnings.

If a local `#[allow]` is unavoidable:
1. scope it to the smallest possible item,
2. explain why in code,
3. reference a tracking issue.

---

## 5) Formatting and Style

- `rustfmt` config is mandatory.
- Formatting changes should be separate from logic changes when possible.
- Follow existing naming conventions and module organization.
- Keep functions focused; extract helpers when complexity grows.

---

## 6) Testing Standards

## Minimum requirement per change
Every meaningful change must include or update tests that verify behavior.

## Test layers
- Unit tests for pure logic (state transitions, parsing, normalization).
- Integration tests for module boundaries (runtime orchestration, persistence contracts).
- Regression tests for bug fixes.

## Test behavior standards
- Tests must verify externally meaningful behavior.
- Tests must be deterministic and non-flaky.
- Avoid over-mocking core logic; prefer realistic data shape coverage.

## Required validation commands
At minimum before merge:
- `cargo check`
- `cargo test`
- `cargo clippy` (project-configured strict mode)

---

## 7) Persistence Standards

Jefe v1 persistence is file-based only:
- `settings.toml`
- `state.json`

Standards:
- schemas are versioned,
- parse/validate before apply,
- writes are atomic,
- malformed files fail safely with clear operator feedback.

SQLite is out of scope for v1 and must not be introduced as hidden fallback.

---

## 8) Theme and UX Standards

- Green Screen is default and fallback theme.
- No bright/light default palettes.
- Theme behavior must stay consistent across all UI surfaces.
- Terminal focus/unfocus semantics must remain explicit and predictable.

---

## 9) Runtime Orchestration Standards

- Preserve stable agent/session identity mapping.
- Keep kill/relaunch semantics agent-scoped.
- Relaunch must respect saved profile/mode behavior.
- Runtime failure handling must not crash the app process.

Jefe should provide orchestration diagnostics only; deep runtime logs belong to `llxprt`.

---

## 10) LLM Contributor Rules

LLM-generated code must follow all rules above plus:

1. Do not invent alternate architecture patterns that violate module boundaries.
2. Do not silently drop strictness to make builds pass.
3. Do not replace typed models with generic maps/strings.
4. Do not add TODO-only stubs in production paths.
5. Do not ship speculative abstractions without active usage.

LLM outputs must include:
- explicit scope of change,
- tests added/updated,
- constraints/assumptions noted.

---

## 11) Review and Merge Quality Bar

A change is mergeable only when all are true:

1. Behavior aligns with functional/technical specs.
2. Module boundaries remain intact.
3. Lint, complexity, and formatting standards pass.
4. Tests cover the change and pass.
5. Documentation is updated when contracts change.
6. No prohibited patterns were introduced.

---

## 12) Non-Negotiable "Do Not" Summary

- Do not disable lints.
- Do not use weak typing for core contracts.
- Do not use `unwrap`/`expect` in production paths.
- Do not bypass architecture boundaries.
- Do not introduce SQLite in v1.
- Do not default to bright/light themes.
