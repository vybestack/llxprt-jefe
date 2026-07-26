# Issue #425 — npm exec version resolution is unreliable (managed install fix)

## Problem

`npm exec --yes --package=@vybestack/llxprt-code@VERSION -- llxprt ARGS` is
unreliable for local LLxprt launches:

- **A. Local `node_modules` shadows the `--package` spec** — npm exec loads the
  Arborist tree from the work_dir's `node_modules` and may run a local install
  instead of the requested version.
- **B. `_npx` cache lock contention** — concurrent same-spec launches race on
  `~/.npm/_npx/<hash>/concurrency.lock` → `ECOMPROMISED` / `ENOTEMPTY`.
- **C. `_npx` cache stores caret ranges** — drift risk once a `0.10.x` patch
  ships.

## Decision (accepted approach)

Replace local `npm exec` with a **jefe-managed version install cache**:

- Install the exact selector into `<cache_dir>/jefe/llxprt-versions/<slug>/`
  using `npm install` (no args) against a hand-written `package.json` that pins
  the exact selector (dist-tag or version, never a caret). `npm install` with
  no package args does NOT rewrite package.json.
- Execute `<install_dir>/node_modules/.bin/llxprt ARGS` directly (resolved via
  the existing `AgentExecutableResolver` scoped to the `.bin` dir, so Windows
  `.cmd` handling is reused).
- Per-launch process is `&mut self` (single-threaded runtime); concurrent
  launches from one jefe serialize on an in-process `Mutex`.

## Acceptance matrix

| # | Behavior | Evidence |
|---|----------|----------|
| AC1 | Local versioned LLxprt launch installs into the jefe cache dir, not the work_dir, and executes `<install_dir>/node_modules/.bin/llxprt` (no `npm exec` in the local pane command). | Unit test: `local_launch_plan` for a versioned signature produces `Agent(AgentKind::Llxprt)` + managed bin dir; `local_launch_command` resolves the bin from the managed dir. |
| AC2 | The package.json pin is the exact selector (`latest`/`nightly` dist-tag or explicit version), never a caret range. | Unit test on the pure install-spec + dir-name helpers. |
| AC3 | The install dir name is filesystem-safe and unique per effective selector (`latest`, `nightly`, explicit version). | Unit test mapping selectors → dir names. |
| AC4 | A repeated launch with the same selector hits the cache (marker + bin exist) and does not reinstall. | Unit test: marker-file check decides cache hit. |
| AC5 | `require_launch_package_available` for a local versioned launch installs (validating availability) and surfaces a typed error on failure; remote keeps the `npm view` probe. | Unit test on the local branch (fake npm fixture) + existing remote probe tests unchanged. |
| AC6 | The non-interactive (rewrite) path uses the managed bin dir for local versioned launches. | Unit test on `run_non_interactive` argv/executable resolution for the managed case. |
| AC7 | Remote versioned launches keep `npm exec` (out of scope for this PR). | Existing remote argv tests remain green. |
| AC8 | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build`, `cargo test` all pass. | `make ci-check`. |

## Vertical slices

1. **Domain helpers** — add `install_spec_value()` and `version_dir_name()` to
   `LlxprtNpmPackageSelector` (pure, unit-tested).
2. **Install module** — new `src/runtime/llxprt_install.rs`: cache-root, dir
   resolution, idempotent `ensure_installed(selector)` with in-process Mutex,
   typed `LlxprtInstallError`. Pure helpers + I/O boundary, unit-tested.
3. **Local launch wiring** — thread the managed bin dir through
   `local_launch_plan` → `local_launch_command`; resolve via a scoped
   resolver. Install step in `create_session`'s local path.
4. **Package-probe local branch** — `require_launch_package_available` for
   local NpmBacked calls `ensure_installed` instead of the `npm view` probe;
   remote unchanged.
5. **Non-interactive wiring** — `run_non_interactive` uses the managed bin dir
   for local versioned launches.
6. **Test updates** — local npm-launch tests move to the managed form; remote
   tests unchanged.

## Non-goals

- Remote versioned-launch managed install (keeps `npm exec`; follow-up).
- Cross-process file locking (in-process Mutex only; two concurrent jefe
  processes installing the same version can still race — noted as follow-up).
- Cache eviction / cleanup of old versions.
- Semver validation of the selector (npm handles resolution).
- Issue #403 (version sanitization) — already merged.

## Scope ledger

| File | Change | Net lines (est.) |
|------|--------|------------------|
| `src/domain/llxprt_version.rs` | +`install_spec_value`, `version_dir_name` + tests | +60 |
| `src/runtime/llxprt_install.rs` (NEW) | install cache module + tests | +280 |
| `src/runtime/mod.rs` | module decl + exports | +6 |
| `src/runtime/errors.rs` | `LlxprtInstall` variant | +8 |
| `src/runtime/commands.rs` | managed bin dir in `LocalLaunchPlan`/`local_launch_command`; install step | +70 |
| `src/runtime/package_probe.rs` | local branch → `ensure_installed` | +25 |
| `src/runtime/non_interactive.rs` | managed bin dir resolution | +30 |
| `src/runtime/npm_launch_tests.rs` | update local tests to managed form | +30 |
| `src/runtime/non_interactive_tests.rs` | managed argv test | +15 |

Estimated total: ~520 net lines, 9 files. Within budget (25 files / 1500 lines).
