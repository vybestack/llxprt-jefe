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

## Reproduce the native Windows CI gate

Run the Windows gate from native PowerShell with the x86-64 MSVC toolchain. Do
not use WSL, Cygwin, MSYS2, Git Bash, Docker, or another Unix compatibility
layer for this qualification.

CI pins psmux 3.3.6 from the official release archive
`psmux-v3.3.6-windows-x64.zip` and verifies SHA-256
`a56a890ea0829567818b9a368f16dcbd39c087f27328573df17c10dd39618947` before
extracting it. Local contributors may install the same qualified release with:

```powershell
winget install --id marlocarlo.psmux --version 3.3.6 --exact
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

$workspace = (Get-Location).Path
$config = Join-Path $workspace 'target/windows local/config with spaces'
$working = Join-Path $workspace 'target/windows local/working with spaces'
$artifacts = Join-Path $workspace 'target/tmux-harness/windows-local'
New-Item -ItemType Directory -Force $config, $working, $artifacts | Out-Null
& (Join-Path $workspace 'target/debug/jefe-tmux-harness.exe') `
  --scenario (Join-Path $workspace 'dev-docs/tmux-scenarios/startup-quit.json') `
  --jefe-bin (Join-Path $workspace 'target/debug/jefe.exe') `
  --config $config `
  --working-dir $working `
  --session 'jefe-windows-local' `
  --out-dir $artifacts
```

The smoke suite and harness own unique psmux namespaces and only issue
namespace-scoped cleanup. Failure diagnostics are written beneath
`target/psmux-smoke` and `target/tmux-harness`.

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
