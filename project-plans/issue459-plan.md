# Issue #459 — Replace portable shell and Make automation with a cross-platform Rust xtask

## Problem

Repository quality and developer automation is split across GNU Make, Bash,
embedded Python, Unix utilities, inline GitHub Actions shell, and direct Cargo
commands. The canonical local gate (`make ci-check`) is unavailable on a normal
Windows Rust installation, and policy checks (clippy-allow, source-size,
architecture) have different coverage by platform because they require Bash,
Python, and Unix text utilities. `tests/core/clippy_allow_policy.rs` directly
launches `bash`, making Windows test success depend accidentally on Git Bash.

## Decision (accepted approach)

**Clean cutover** to a conventional Rust `xtask` workspace package. Move all
generally applicable developer/quality automation into it. One canonical entry
point: `cargo xtask <command>` (via a Cargo alias in `.cargo/config.toml`).

- No Make wrapper, no Bash wrapper, no PowerShell port, no dual implementation.
- Update every active caller in the same change, then delete superseded files.
- `ci` is the aggregate; CI jobs call the narrow command matching the job so
  GitHub retains parallelism and job names.

## Acceptance matrix

| ID | Behavior | Evidence |
|----|----------|----------|
| A1 | `cargo xtask ci` runs the complete local CI-equivalent gate in order: fmt, clippy-allow policy, source-size policy, architecture policy, strict clippy, complexity clippy, coverage, locked all-feature build, locked all-feature tests. Fail-fast; nonzero exit propagates. | Command-progression test (fake-cargo plan) proves ordering + fail-fast + exit code. |
| A2 | `cargo xtask quick` = `cargo fmt`, `cargo check -q`, `cargo test -q`. | Command-plan unit test. |
| A3 | `cargo xtask trim-cache` = `cargo llvm-cov clean --workspace` + removal of `target/debug/incremental`, platform-aware `PathBuf`. | Fixture/path tests + dry-run/command-plan test (does not delete real cache). |
| A4 | Rust clippy-suppression scanner preserves zero-tolerance for outer/inner `allow`+`expect`, `cfg_attr`, whitespace variants, raw identifiers, multiline attributes, comments, strings/raw strings/byte strings, char literals/lifetimes, nested brackets. Scanner errors fail closed. | Migrate current positive + negative fixtures from `tests/core/clippy_allow_policy.rs` into xtask-owned tests; repository scan passes. |
| A5 | Clippy policy verifies the five complexity thresholds in root `clippy.toml` and `.github/clippy/clippy.toml` are present and equal. | Matched, missing, mismatched fixture tests. |
| A6 | Rust source-size policy preserves scan roots (`src tests`), 1000-line hard / 750-line warning limits, supports test overrides, stable relative-path diagnostics on Windows + Unix. | Clean, warning, failure, no-trailing-newline, nested-path, override fixture tests. |
| A7 | Architecture policy ported from `scripts/check-architecture.sh`: required message/state/input symbols, prohibited crate-wide clippy allowances (with the current exception ledger), handler-file discovery, 850-line default, `src/state/form_ops.rs` 955-line exception. | Positive repository test + focused negative fixtures. |
| A8 | Standalone xtask subcommands for each CI-visible gate: policy checks, strict lint, complexity, coverage, fmt, build, test. Args/env built with `std::process::Command`, not shell strings. | Unit tests assert command plans; CI invokes the standalone commands. |
| A9 | Coverage preserves stable-toolchain lookup, workspace/all-feature options, ignore regex, 30% line threshold, platform-aware `PathBuf`. | Command-plan tests for Unix + Windows path forms; Linux CI runs the real coverage gate. |
| A10 | `.github/workflows/ci.yml` invokes xtask for repeatable repository logic. Portable policy checks run on both Ubuntu and native Windows; host provisioning remains workflow-native. | Both CI platforms pass; clippy-allow, source-size, architecture policies run on Windows. |
| A11 | Active contributor, standards, and delivery-workflow docs name only `cargo xtask` commands. No active `make build`/`ci-check`/`quick-check`/`trim-cache` or migrated-script reference remains. | Documentation contract/search test; historical `project-plans/` untouched. |
| A12 | Superseded implementations deleted in same PR: `Makefile`, `scripts/check-clippy-allows.sh`, `scripts/check-source-file-size.sh`, `scripts/check-architecture.sh`. No `.sh`/`.ps1` shim forwards to xtask. | Paths absent; repository search finds no active references. |

## Required xtask command surface

```text
cargo xtask ci
cargo xtask quick
cargo xtask trim-cache
cargo xtask fmt
cargo xtask lint
cargo xtask complexity
cargo xtask coverage
cargo xtask build
cargo xtask test
cargo xtask check clippy-allows
cargo xtask check source-size
cargo xtask check architecture
```

## Expected files

- `Cargo.toml`, `Cargo.lock` — add xtask workspace member; retain root jefe
  package and normal default-member behavior.
- `.cargo/config.toml` — local `cargo xtask` alias.
- `xtask/Cargo.toml`
- `xtask/src/main.rs`
- `xtask/src/cli.rs` — argument parsing + dispatch.
- `xtask/src/process.rs` — `std::process::Command` plan helpers.
- `xtask/src/clippy_policy.rs` — suppression scanner + threshold sync.
- `xtask/src/source_size.rs` — source-size policy.
- `xtask/src/architecture.rs` — architecture policy.
- `xtask/src/toolchain.rs` — coverage/toolchain discovery.
- `xtask/tests/` — integration tests with temp fixtures.
- `tests/core/clippy_allow_policy.rs` — migrate to invoke xtask (or move into
  xtask tests) instead of spawning Bash.
- `tests/core/message_bus_contracts.rs` — remove/replace the Unix-only
  `architecture_boundary_script_passes_in_ci_tests` (A7) with an xtask-based
  equivalent.
- `.github/workflows/ci.yml` — invoke xtask; add Windows policy jobs.
- Docs: `README.md`, `CONTRIBUTING.md`, `docs/building.md`, `dev-docs/README.md`,
  `dev-docs/code-review-demand.md`, `dev-docs/standards/coding-standards.md`,
  `dev-docs/standards/testing-and-quality.md`,
  `dev-docs/workflow/ISSUE-DELIVERY.md`.
- Delete `Makefile`, `scripts/check-clippy-allows.sh`,
  `scripts/check-source-file-size.sh`, `scripts/check-architecture.sh`.

## Non-goals (from issue)

- Do not migrate/delete issue-specific scenario/shim/capture/tutorial scripts.
- Do not move OS/package-manager provisioning into xtask (apt, psmux download,
  artifact upload remain in workflow YAML + host shell).
- Do not rewrite release packaging or OCR workflow.
- Do not change lint levels, complexity thresholds, source-size limits,
  coverage %, scan scope, or architecture policy semantics while porting.
- Do not rewrite historical `project-plans/`.
- Do not modify `.llxprt/`.
- Do not refactor product/runtime architecture.

## Scope ledger

| File | Change | Est. net lines |
|------|--------|----------------|
| `Cargo.toml` | workspace member + default-members | +6 |
| `.cargo/config.toml` | xtask alias | +3 |
| `xtask/Cargo.toml` (NEW) | package manifest (std-only, tempfile dev) | +25 |
| `xtask/src/main.rs` (NEW) | entry + dispatch | +40 |
| `xtask/src/cli.rs` (NEW) | arg parsing | +120 |
| `xtask/src/process.rs` (NEW) | command-plan helpers | +110 |
| `xtask/src/clippy_policy.rs` (NEW) | scanner + threshold sync | +340 |
| `xtask/src/source_size.rs` (NEW) | source-size policy | +120 |
| `xtask/src/architecture.rs` (NEW) | architecture policy | +200 |
| `xtask/src/toolchain.rs` (NEW) | coverage/toolchain discovery | +90 |
| `xtask/tests/*.rs` (NEW) | integration/fixture tests | +700 |
| `tests/core/clippy_allow_policy.rs` | rewrite to call xtask | ~0 (rewrite) |
| `tests/core/message_bus_contracts.rs` | replace bash test | +15 / -15 |
| `.github/workflows/ci.yml` | xtask invocations + Windows policy jobs | +60 |
| `README.md` | doc updates | +5 / -5 |
| `CONTRIBUTING.md` | doc updates | +5 / -5 |
| `docs/building.md` | doc updates | +5 / -5 |
| `dev-docs/README.md` | doc updates | +1 / -1 |
| `dev-docs/code-review-demand.md` | doc updates | +1 / -1 |
| `dev-docs/standards/coding-standards.md` | doc updates | +2 / -2 |
| `dev-docs/standards/testing-and-quality.md` | doc updates | +10 / -10 |
| `dev-docs/workflow/ISSUE-DELIVERY.md` | doc updates | +1 / -1 |
| `Makefile` (DELETE) | remove | -30 |
| `scripts/check-clippy-allows.sh` (DELETE) | remove | -230 |
| `scripts/check-source-file-size.sh` (DELETE) | remove | -75 |
| `scripts/check-architecture.sh` (DELETE) | remove | -90 |

**Estimated total: ~28 files, ~1,900 net added lines** (gross added ~1,800,
gross deleted ~450 → net +1,350 in kept files; xtask is the bulk).

> ⚠️ **SCOPE REVIEW TRIGGER.** This crosses the 25-file soft target. The
> issue's own "Expected paths" require this footprint atomically (clean
> cutover forbids splitting the deletion of Make/scripts from the xtask
> replacement). Requesting explicit approval to proceed above the 25-file
> target but **within** the 40-file / 2,500-line hard budget.

## Review counters

- OCR before PR: 0 / 2
- OCR after PR: 0 / 2

## Vertical slices

1. **xtask skeleton + workspace wiring** — `xtask/Cargo.toml`, `main.rs`,
   `cli.rs`, `process.rs`, `.cargo/config.toml` alias, root `Cargo.toml`
   member. Commands `fmt`/`build`/`test`/`lint`/`complexity`/`coverage` as
   command-plan builders. RED: test that `cargo xtask` exists and dispatches.
2. **clippy-allow policy** (A4, A5) — port scanner + threshold sync with
   fixture tests migrated from `tests/core/clippy_allow_policy.rs`.
3. **source-size policy** (A6) — port with fixtures.
4. **architecture policy** (A7) — port with fixtures; replace
   `message_bus_contracts.rs` bash test.
5. **`ci` + `quick` + `trim-cache`** (A1, A2, A3) — aggregate + fast + cache.
6. **CI workflow cutover** (A10) — invoke xtask; add Windows policy jobs.
7. **Docs + deletions** (A11, A12) — update active docs, delete Make + 3
   scripts, prove clean active-reference search.

## Verification

- `cargo xtask ci` on the exact PR head (Linux + Windows where tools exist).
- `cargo fmt --all --check`, strict + complexity clippy, build, test.
- New xtask unit/integration tests pass.
- Repository-wide search: no active `make *` or `scripts/check-*.sh`
  references outside historical `project-plans/`.
