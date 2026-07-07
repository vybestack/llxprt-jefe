# Dev Docs Index

This directory contains the working rules for planning and implementation in
Jefe. The single entry point for contributors is
[`CONTRIBUTING.md`](../CONTRIBUTING.md) at the repo root.

## Standards

Normative standards live under [`standards/`](./standards/):

- [`architecture.md`](./standards/architecture.md) — module boundaries, the
  unidirectional data flow, the pure-views projection pattern, dependency DAG.
- [`coding-standards.md`](./standards/coding-standards.md) — Rust conventions,
  lint/complexity thresholds, source-file-length policy, DO/DON'T rules,
  documentation standards.
- [`testing-and-quality.md`](./standards/testing-and-quality.md) — TDD, test
  layers, assertion style, coverage floor, the verification suite, CI jobs.
- [`display-and-ui.md`](./standards/display-and-ui.md) — emoji-free policy,
  pure projections, screen/component structure, keybind footer, help modal,
  theme/UX rules.
- [`persistence-and-runtime.md`](./standards/persistence-and-runtime.md) —
  versioned file persistence, atomic writes, safe fallback, runtime
  orchestration.

## Workflow

Multi-phase planning and coordination under [`workflow/`](./workflow/):

- [`PLAN.md`](./workflow/PLAN.md) — how to create and execute robust multi-phase
  implementation plans.
- [`PLAN-TEMPLATE.md`](./workflow/PLAN-TEMPLATE.md) — reusable template for
  writing plans under `project-plans/<feature>/plan/`.
- [`COORDINATING.md`](./workflow/COORDINATING.md) — how coordinators execute
  phase-by-phase work with subagents.

## Testing

TUI harness scenarios (real-TTY end-to-end checks) under [`testing/`](./testing/):

- [`tmux-harness.md`](./testing/tmux-harness.md) — scenario JSON schema, step
  catalog, local execution, artifacts, optional smoke checks.

## Redirect stubs

The following files are now short redirect stubs pointing at the standards docs
above:

- [`RULES.md`](./RULES.md) — redirects to coding-standards, testing-and-quality,
  architecture.
- [`project-standards.md`](./project-standards.md) — redirects to all standards
  docs.

## Recommended Reading Order

1. [`standards/architecture.md`](./standards/architecture.md)
2. [`standards/coding-standards.md`](./standards/coding-standards.md)
3. [`standards/testing-and-quality.md`](./standards/testing-and-quality.md)
4. [`workflow/PLAN.md`](./workflow/PLAN.md)
5. [`workflow/PLAN-TEMPLATE.md`](./workflow/PLAN-TEMPLATE.md)
6. [`workflow/COORDINATING.md`](./workflow/COORDINATING.md)

## Practical Workflow

1. Define or update feature spec under `project-plans/...`.
2. Create/refresh plan using [`workflow/PLAN-TEMPLATE.md`](./workflow/PLAN-TEMPLATE.md).
3. Use [`workflow/COORDINATING.md`](./workflow/COORDINATING.md) to execute
   phases in strict sequence.
4. Ensure all quality gates pass (`make ci-check`), covering:
   - `cargo fmt --all --check`
   - `scripts/check-clippy-allows.sh`
   - `scripts/check-source-file-size.sh`
   - clippy complexity gates
   - coverage (`--fail-under-lines 30`)
   - `cargo build --workspace --all-features --locked`
   - `cargo test --workspace --all-features --locked`
