# Issue #432 — [Windows] Version=latest reports LLxprt unavailable

## Status context

Issue #432 was filed 2026-07-26 04:19 UTC. The proposed durable direction
(jefe-managed install cache) landed ~11 hours later in #431 (commit
`53b891c`, "Replace npm exec with jefe-managed LLxprt install cache for
local versioned launches (Fixes #425)").

**Reproduced on the reporting Windows machine (Node v24.18.0, npm 11.16.0,
`C:\Program Files\nodejs`):** Running exactly what `ensure_installed`
constructs — `node.exe npm-cli.js install` from a neutral temp dir against a
hand-written `package.json` pinning `@vybestack/llxprt-code: latest` —
succeeds (exit 0) and lands `llxprt.cmd` / `llxprt` / `llxprt.ps1` in
`node_modules/.bin`. So the managed-install fix (#431) resolves the
*original* user-facing failure on the reporting machine.

The remaining work for #432 is closing the **acceptance-criteria gaps that
#431 did not cover**, all of which are about Windows-specific evidence and
diagnostics — not about re-architecting the (now-correct) install path.

## Root-cause confirmation (UPDATED — real bug found via Slice 1 RED)

The Slice 1 Windows behavioral test surfaced the actual #432 root cause:

**`AgentExecutableResolver::canonical_script_launch_plan` uses
`std::fs::canonicalize()`, which on Windows returns `\\?\`-prefixed verbatim
paths. When that `\\?\C:\...\npm-cli.js` path is passed as a structured
argument to `node.exe`, Node's module loader `realpathSync`/`toRealPath`
mis-parses the verbatim prefix and degenerates the path to `'C:'` (treats it
as a drive-letter directory), failing with
`EISDIR: illegal operation on a directory, lstat 'C:'`.**

Reproduced deterministically: `node.exe \\?\C:\...\npm-cli.js` fails with
exactly that error; the same path without the `\\?\` prefix succeeds. This
affects both the npm install path (`node.exe npm-cli.js install`) AND the
official LLxprt launcher (`bun.exe index.ts`) since both flow through
`canonical_script_launch_plan`.

This is precisely the "Jefe-specific Windows execution context" failure #432
describes — the structured-argument contract from #258 is correct in shape but
the canonicalize-induced verbatim prefix breaks the consumer (`node.exe`/`bun.exe`).

### Fix (Slice 1, GREEN)

Strip the `\\?\` (and `\\?\UNC\`) verbatim prefix from canonicalized paths
inside `canonical_script_launch_plan` before they are stored in
`CanonicalScriptLaunchPlan`. The existence check still runs through
`canonicalize`, so the file is proven real; only the *stored* path used as a
structured argument is de-prefixed. No new dependency (narrow inline helper,
not a `dunce`-style general sandbox-escape surface). Updates the two existing
`agent_executable_tests` assertions that compared against raw `canonicalize`.



## Acceptance matrix

| # | Behavior | Evidence | Status after #431 | This PR |
|---|----------|----------|-------------------|---------|
| AC1 | On Windows, `Version=latest` launches the current published release. | Behavioral: `ensure_installed` against a staged canonical node.exe + npm-cli.js fixture installs `latest` and returns the `.bin` dir; marker written. | Path correct, no Windows test | **Add Windows install happy-path test** |
| AC2 | `latest nightly` and an explicit pinned version also work on Windows. | Same harness, selectors `nightly` + `0.9.0`. | Path correct, no Windows test | **Add Windows install test for nightly + pinned** |
| AC3 | macOS behavior remains unchanged. | Unix-gated install test stays green; no `cfg(unix)` path edited. | Covered | Verify unchanged |
| AC4 | A Windows integration test covers package resolution and launch with the canonical `node.exe` + `npm-cli.js` layout. | A `#[cfg(windows)]` test stages `node.exe` + `node_modules/npm/bin/npm-cli.js`, runs `ensure_installed_under`, and asserts the marker + bin dir. | **Missing** (existing happy-path is `#[cfg(unix)]`-only with an explicit comment that Windows needs the canonical layout) | **Add** |
| AC5 | A behavioral regression test covers the failing Jefe execution context rather than only asserting constructed argv. | The Windows test runs the real install subprocess from the neutral jefe-owned cwd, not just an argv assertion. | **Missing** | **Add** |
| AC6 | Package resolution/install does not depend on the repo worktree's `.npmrc`, `package.json`, or `node_modules`. | The Windows test runs from a temp cache root with no inherited npm config; the install cwd is the jefe-managed dir. | Path correct (cwd = install dir), no Windows test | Covered by AC4/AC5 test |
| AC7 | Concurrent agents selecting the same version do not race in npm's `_npx` cache. | No `_npx` involvement: install is into jefe-owned dir with in-process `Mutex`. | Covered by design + existing Unix test | Verify unchanged |
| AC8 | Windows npm execution continues to use direct structured arguments without `cmd.exe` mediation. | Existing `windows_npm_cmd_bypasses_cmd_and_preserves_adversarial_argv` test. | Covered | Unchanged |
| AC9 | Failures identify the resolution/install/launch phase and show bounded, redacted diagnostics. | `LlxprtInstallError` variants distinguish NpmMissing / InstallDir / InstallFailed; diagnostics bounded to 512 bytes; exit code in InstallFailed. | Largely covered | **Tighten**: ensure exit code + timeout phase appear in `InstallFailed` diagnostic |
| AC10 | Existing blank-Version behavior (direct resolved LLxprt executable) is unchanged. | `direct_local_plan_remains_exact` + `versioned_local_selector` returns None for blank. | Covered | Unchanged |

## Non-goals

- Changing the `latest` → `@latest` mapping (already correct).
- Adding `npx` support (Jefe uses `npm install` into a managed cache).
- Changing Code Puppy launch behavior.
- Reworking remote package resolution (keeps `npm exec`; #425 non-goal).
- Cross-process file locking (in-process `Mutex` only; documented follow-up).
- Cache eviction.
- Any change to the `latest`/`nightly` sentinel normalization.

## Vertical slices

### Slice 1 — Windows install happy-path behavioral test (AC1, AC2, AC4, AC5, AC6)

- **Owner:** `src/runtime/llxprt_install.rs` test module.
- **Allowed files:** `src/runtime/llxprt_install_tests.rs` (test additions only).
- **RED:** Add a `#[cfg(windows)]` test that stages the canonical Windows npm
  layout (`node.exe` + `node_modules/npm/bin/npm-cli.js`) as a *real* stub
  that creates `node_modules/.bin/llxprt.cmd` and exits 0, then calls
  `ensure_installed_under` against a temp cache root and asserts the marker
  + bin dir. The stub must be a real `node.exe`-executable script (a JS
  file that npm-cli.js would run is not viable; instead stage a fake
  `node.exe` that is a script interpreter is not possible on Windows
  without a real binary). **Resolution:** the stub `node.exe` cannot be a
  shell script on Windows. Instead, the test stages the canonical layout
  files (so `AgentExecutableResolver` produces the `node.exe` + `npm-cli.js`
  plan) and uses the **real** `node.exe` from `PATH`/`Program Files` to run
  a stub `npm-cli.js` that performs the install side effect (create the bin
  + marker). This is the same shape as the Unix shell-stub test but uses a
  JS stub run by the real node, proving the canonical Windows launch plan
  executes structured arguments end-to-end.
- **GREEN:** The test passes against the existing (already-correct)
  `ensure_installed` implementation. If the implementation has a residual
  Windows bug, this test surfaces it; otherwise it locks in the contract.
- **Non-goals:** no production code change in this slice unless the test
  reveals a bug.
- **Verification:** `cargo test --lib runtime::llxprt_install` on Windows;
  `cargo test --lib runtime::llxprt_install` on Unix must still pass (test
  is `#[cfg(windows)]`-gated).
- **Stop conditions:** if staging a real node-executable stub proves
  infeasible without a new test-infra dependency, stop and propose the
  alternative (e.g., a `#[cfg(windows)]` test that asserts the constructed
  `Command` program/args for the install path against the canonical layout
  — weaker but still closes the "Windows canonical layout" evidence gap).

### Slice 2 — Diagnostic phase/exit-code tightening (AC9)

- **Owner:** `src/runtime/llxprt_install.rs` (`run_npm_install`).
- **Allowed files:** `src/runtime/llxprt_install.rs`,
  `src/runtime/llxprt_install_tests.rs`.
- **RED:** Add a test asserting that an `InstallFailed` diagnostic for a
  nonzero exit includes the string `exited with status <code>` and that a
  timeout includes `timed out`. (The exit-code part already exists; verify
  the timeout phase surfaces distinctly from a generic capture error.)
- **GREEN:** If the timeout diagnostic does not already identify itself as
  the install phase, add a bounded phase label. Minimal change: the
  `run_command_capture_with_timeout` error already says
  `jefe llxprt install: timed out after Ns`; confirm `InstallFailed`
  surfaces that rather than collapsing it.
- **Non-goals:** no new error variant; no PII/redaction changes (diagnostics
  are already npm stderr only, no env values).
- **Verification:** `cargo test --lib runtime::llxprt_install`.

## Scope ledger (final)

| File | Change | Net lines (actual) |
|------|--------|--------------------|
| `src/runtime/agent_executable.rs` | +`strip_verbatim_prefix()` (Windows: strip `\\?\` / `\\?\UNC\`; Unix: identity); applied in `canonical_script_launch_plan` to `runtime` + `entrypoint` (root-cause fix); validate `is_file()` on canonical paths *before* stripping (OCR F1); OsStr-based UNC root (OCR F3); collapsed redundant branch (OCR F2) | +75 |
| `src/runtime/agent_executable_tests.rs` | +`expected_canonical()` helper (cross-platform); updated 2 npm/LLxprt canonical-path assertions to de-prefixed form | +35 |
| `src/runtime/npm_launch_tests.rs` | +`canonicalize_for_arg()` helper (`#[cfg(windows)]`); updated 2 Windows argv tests to de-prefixed form | +31 |
| `src/runtime/llxprt_install_tests.rs` | +3 `#[cfg(windows)]` canonical-install behavioral tests (hardlinked real node.exe + JS stub); +2 `#[cfg(unix)]` diagnostic phase tests (exit code + timeout); `require_node_install_or_skip` truthiness (OCR F4) | +274 |
| `project-plans/issue432-plan.md` | this plan | +plan |

Actual total: ~415 net source lines across 4 source files +1 plan file.
Well within budget (5 files ≤ 25; ~415 lines ≤ 1500). No new dependencies,
no workflow/agent-memory/quality-tool/manifest/.github changes.

## Review counters (final)

- Local OCR runs before PR: 0 / 2
- PR OCR runs: 2 / 2 (run 1 → 5 findings triaged; run 2 → 1 finding triaged)

### OCR triage outcome

Run 1 (6 inline comments on the initial PR head):
- **F1 [Blocker-Fix, applied]:** `is_file()` now runs on canonical (verbatim) paths *before* stripping — Win32 long-path (>260) installs validate correctly.
- **F2 [In-scope-Fix, applied]:** collapsed redundant `if let Component::Normal` branch to a single `push`.
- **F3 [Blocker-Fix, applied]:** VerbatimUNC root built with `OsString`/`OsStr` (no `to_string_lossy`) — non-UTF-8 filesystem bytes preserved.
- **F4 [In-scope-Fix, applied]:** `JEFE_REQUIRE_NODE_INSTALL` aligned with `JEFE_REQUIRE_PSMUX` truthiness convention (`== "1"`).
- **F5 [Reject] (×2 duplicate comments):** silent-skip visibility — matches the established `JEFE_REQUIRE_PSMUX` convention; `clippy::print_stderr` forbids `eprintln!` in the lib/test target; the `=1` gate is the loud-panic visibility mechanism.

Run 2 (1 inline comment on the OCR-fix head `364a341`):
- **[Reject]:** `require_node_install_or_skip` name implies infallibility but can panic — deliberately mirrors the established `JEFE_REQUIRE_PSMUX` `Option`-returning-helper-that-asserts pattern. Test-only naming preference; no change.

No Blocker-Fix or In-scope-Fix findings remain unresolved.

## Verification evidence (final head `364a341`)

Local (Windows, the reporting environment):
- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean (exit 0).
- `cargo build --workspace --all-features --locked` — clean (exit 0).
- `cargo test --lib --all-features --locked` — 2397 passed, 0 failed.
- `cargo test --lib runtime::` — 353 passed, 0 failed.

CI (PR #483, head `364a341`, all green, `mergeStateStatus: CLEAN`):
- Build ✅ | Test ✅ (Ubuntu) | Format ✅ | Lint (clippy) ✅ | Clippy allow policy ✅
- Complexity ✅ | Coverage gate ✅ | Source file length ✅ | Architecture boundary ✅
- Native Windows (MSVC + psmux) ✅ (lib tests 2397/0, incl. 3 new Windows canonical-install tests)
- Mergeability gate ✅ | Run LLxprt review ✅ | OpenCodeReview ✅ (2 runs, exit 0)

## Deferred findings / follow-ups

- Cross-process install locking (two concurrent jefe processes installing
  the same version) — documented in #425 plan, out of scope here.
- The flaky `psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys`
  ConPTY smoke test (appeared once on the first CI run, passed on rerun) is
  the same flake category tracked by #465/#468; not in scope for this issue.
