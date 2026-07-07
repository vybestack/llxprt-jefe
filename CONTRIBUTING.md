# Contributing to jefe

`jefe` is a Rust TUI application. This guide is the single entry point for
contributors (human and LLM).

New here? Start with [`docs/getting-started.md`](docs/getting-started.md) for a
walkthrough, then [`docs/building.md`](docs/building.md) for build details.

## Build and run

```sh
cargo run
```

Requirements:

- Rust toolchain (edition 2024). See [`Cargo.toml`](Cargo.toml).
- `tmux` installed and available on `PATH`.
- `llxprt` (the agent CLI) installed and available on `PATH`.

## Fast iteration

For tight local loops:

```sh
make quick-check
# = cargo fmt && cargo check -q && cargo test -q
```

## Full pre-merge gate

Before pushing, reproduce the full CI gate locally:

```sh
make build
```

`make build` is an alias of `make ci-check`, which runs: format check,
clippy-allow policy, source-file-size policy, clippy complexity gates, the 30%
line-coverage gate, a workspace build, and the full test suite. See
[Testing and Quality](dev-docs/standards/testing-and-quality.md) for every job.

## Branch and PR conventions

- **One issue branch per issue** (e.g. `issue42`), branched from `main`.
- **Issue number in the PR title**, e.g. `Adds cat pictures to every UI screen (Fixes #123)`.
- **`Fixes #N` or `closes #N` in the PR body** so the linked issue auto-closes on
  merge.
- Squash-merge or rebase-merge to `main`. Delete the feature branch after merge.

## Standards

The authoritative standards live under [`dev-docs/standards/`](dev-docs/standards/):

- [Architecture Standards](dev-docs/standards/architecture.md) — module
  boundaries, the unidirectional data flow, the pure-views projection pattern,
  and the dependency-direction DAG.
- [Coding Standards](dev-docs/standards/coding-standards.md) — Rust conventions,
  lint config, complexity thresholds, source-file-length policy, DO/DON'T rules,
  documentation standards.
- [Testing and Quality](dev-docs/standards/testing-and-quality.md) — TDD, test
  layers, assertion style, coverage floor, the full verification suite, and the
  CI jobs.
- [Display and UI](dev-docs/standards/display-and-ui.md) — emoji-free policy,
  pure projections, screen/component structure, keybind footer, help modal,
  theme/UX rules.
- [Persistence and Runtime](dev-docs/standards/persistence-and-runtime.md) —
  versioned file persistence, atomic writes, safe fallback, runtime
  orchestration rules.

## Workflow

Multi-phase implementation follows a strict plan-and-coordinate discipline:

- [Planning Guide](dev-docs/workflow/PLAN.md) — how to create and execute robust
  multi-phase implementation plans.
- [Plan Template](dev-docs/workflow/PLAN-TEMPLATE.md) — reusable template for
  writing plans under `project-plans/<feature>/plan/`.
- [Coordinating Guide](dev-docs/workflow/COORDINATING.md) — how coordinators
  execute phase-by-phase work with subagents.

## Testing

TUI harness scenarios (real-TTY end-to-end checks) are documented in:

- [Tmux Harness Guide](dev-docs/testing/tmux-harness.md) — scenario JSON schema,
  step catalog, local execution, artifacts, and optional smoke checks.

## Further reading

- [Building](docs/building.md)
- [Getting Started](docs/getting-started.md)
- [Technical Overview](docs/technical-overview.md)
