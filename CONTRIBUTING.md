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
cargo xtask quick
# = cargo fmt && cargo check -q && cargo test -q
```

## Full pre-merge gate

Before pushing, reproduce the full CI gate locally:

```sh
cargo xtask ci
```

`cargo xtask ci` runs: format check, clippy-allow policy, source-file-size
policy, architecture policy, clippy complexity gates, the 30% line-coverage
gate, a workspace build, and the full test suite. See
[Testing and Quality](dev-docs/standards/testing-and-quality.md) for every job.

## Reproduce the native Windows CI gate

Run the Windows gate from native PowerShell with the x86-64 MSVC toolchain. Do
not use WSL, Cygwin, MSYS2, Git Bash, Docker, or another Unix compatibility
layer for this qualification.

CI pins psmux 3.3.7 from the official release archive
`psmux-v3.3.7-windows-x64.zip` and verifies SHA-256
`60ff7b236f64184921cef3c1ff2611aa5a36fcc7ed8e2a58e968b8ded57f6028` before
extracting it. Local contributors may install the same qualified release with:

```powershell
winget install --id marlocarlo.psmux --version 3.3.7 --exact
psmux -V
```

Set `JEFE_PSMUX_BIN` if `psmux.exe` is not on `PATH`, then run the same required
commands as CI:

```powershell
$env:JEFE_REQUIRE_PSMUX = '1'
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo test --features psmux-smoke --test psmux_smoke -- --nocapture

# Schema-1 scenarios are Unix-PTY evidence and are not executed on Windows.
# Reproduce the installed-binary psmux lifecycle with the PowerShell commands
# in the windows_native job in .github/workflows/ci.yml.
```

The smoke suite and installed lifecycle own unique psmux namespaces and issue
only namespace-scoped cleanup. Failure diagnostics are written beneath
`target/psmux-smoke`.

## Branch and PR conventions

- **One issue branch per issue** (e.g. `issue42`), branched from `main`.
- **Issue number in the PR title**, e.g. `Adds cat pictures to every UI screen (Fixes #123)`.
- **`Fixes #N` or `closes #N` in the PR body** so the linked issue auto-closes on
  merge.
- Squash-merge or rebase-merge to `main`. Delete the feature branch after merge.

## Self Assigning Issues

To assign an issue to yourself, add a comment that contains **only** the text `/assign` (nothing else). A GitHub Action handles the request when:

1. The issue is not already assigned.
2. You are eligible: you have at least one **merged PR** in this repository, **or** you have a durable prior assignment (previously assigned to an issue here — current or past assignments in open or closed issues both qualify, recorded in a history index).
3. You currently have fewer than **3** open issues assigned to you (hard cap to limit spam / hoarding).

Bot accounts are ignored. On success the issue is labeled `auto-assigned` and a feedback comment is posted. Assignment may fail for several reasons (the issue was assigned by a concurrent request, the eligibility check could not be verified, GitHub API errors, or GitHub rejected or ignored the assignment because the account is unavailable or otherwise unassignable). The automation verifies each assignment and posts feedback reporting any failure.

Auto-assignments older than **2 weeks** with no qualifying linked PR activity are unassigned automatically. Only the login assigned by the `/assign` automation is removed — manual co-assignees are preserved. The maintainer `acoliver` is exempt from cleanup. Comment `/assign` again if you still intend to work on the issue.

## Code review demand

Keep active work in a draft PR (or use the documented WIP markers), run the
required exact-head local gate, and push that verified head before marking the
PR ready and adding the `review-ready` label. The
[CodeRabbit review-demand policy](dev-docs/code-review-demand.md) defines the
explicit trigger, automatic-review limits, deliberate manual reruns,
reviewed-head coverage, and immutable measurement events.

The [OpenCodeReview finding-evaluation process](dev-docs/code-review-process.md)
defines how to treat OCR output: classify every finding's validity
(valid/partial/invalid/unverifiable), record a disposition
(fix/explain/defer/user-judgment), report coverage honestly, and compare runs
only when their inputs match.

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
