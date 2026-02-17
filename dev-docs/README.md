# Dev Docs Index

This directory contains the working rules for planning and implementation in Jefe.

## Documents

- [`RULES.md`](./RULES.md)
  - Day-to-day development rules for contributors and LLMs.
  - Read this first before coding.

- [`project-standards.md`](./project-standards.md)
  - Project-wide coding, lint, complexity, testing, and review standards.
  - Treat this as policy.

- [`PLAN.md`](./PLAN.md)
  - How to create and execute robust multi-phase implementation plans.
  - Includes preflight, traceability, integration, and verification requirements.

- [`PLAN-TEMPLATE.md`](./PLAN-TEMPLATE.md)
  - Reusable template for writing plans under `project-plans/<feature>/plan/`.

- [`COORDINATING.md`](./COORDINATING.md)
  - How coordinators execute phase-by-phase work with subagents.
  - Covers strict sequencing, verification gating, and remediation loops.

## Recommended Reading Order

1. `RULES.md`
2. `project-standards.md`
3. `PLAN.md`
4. `PLAN-TEMPLATE.md`
5. `COORDINATING.md`

## Practical Workflow

1. Define or update feature spec under `project-plans/...`.
2. Create/refresh plan using `PLAN-TEMPLATE.md`.
3. Use `COORDINATING.md` to execute phases in strict sequence.
4. Ensure all quality gates pass:
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - `cargo test --workspace --all-features`
