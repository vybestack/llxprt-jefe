# Issue #396: Native Windows CI flaky: guarded_real_dashboard_lists_window_fixture_rows times out on waitFor

## Problem

The `Native Windows (MSVC + psmux)` CI check is flaky. The
`cargo test --workspace --all-features --locked` step intermittently fails on
`harness::runner::tests::guarded_real_dashboard_lists_window_fixture_rows`. The
test panics on the very first step (`waitFor "LLxprt Jefe"` … actually the first
scenario step `waitFor "Repository 24"`) with a timeout, meaning jefe never
rendered its first screen within the scenario wait budget.

This failure is **not caused by recent code changes** — it reproduces on
unrelated PRs and on already-merged PRs on main.

Root cause: the affected test parses
`dev-docs/tmux-scenarios/dashboard-list-windowing.json` directly and inherits
only the platform default `wait_timeout_ms` (0 → `DEFAULT_WAIT_TIMEOUT`, 15s on
Windows). Unlike its sibling guarded tests (`guarded_real_jefe_runner_scenario_*`,
`guarded_real_jefe_qqq_quits`) which explicitly request 30s via
`scenario_with_wait_timeout`, this — the heaviest guarded test (25-repo/25-agent
fsync'd state write + synchronous parse/reconcile + 9 steps) — gets only the bare
default. On slow Windows CI runners the cold start exceeds 15s.

## Desired Outcome

- Remove the Windows startup-render flake for the heaviest guarded scenario by
  granting it an explicit, generous `wait_timeout_ms` via the existing
  per-scenario override (matching the 30s sibling real-binary tests), with no
  production code change required (the test parses the file directly and
  `effective_wait_timeout` already honors a non-zero value).
- Preserve Windows list-windowing coverage (do NOT quarantine / `#[ignore]`).
- Preserve regression sensitivity for other scenarios (do NOT raise the global
  Windows default).
- Make any future timeout recurrence self-diagnosing by enriching the
  `waitFor`/`waitForNot` timeout failure reason with the configured budget and
  the actual elapsed wait, so CI-uploaded logs (`target/tmux-harness/` /
  `target/windows-ci/`) show real timing instead of a bare "condition did not
  become true before timeout".

## Non-Goals

- Quarantining (`#[ignore]` / `#[cfg(not(windows))]`) the dashboard list test.
- Raising the global `DEFAULT_WAIT_TIMEOUT` constant on either platform.
- Changing the Windows psmux lifecycle, ConPTY path, or signal handling (those
  are tracked separately under #253 / #332).
- Changing CI workflow YAML, dependency manifests, or quality-gate scripts.
- Altering `POLL_INTERVAL` cadence or any success-path behavior of the harness.
- Adding new public abstractions or production modules.

## Architecture

### Current State

- `src/harness/runner.rs`:
  - `DEFAULT_WAIT_TIMEOUT` = 5s Unix / 15s Windows.
  - `effective_wait_timeout(wait_timeout_ms)` is a pure function: 0 → default,
    non-zero → explicit ms override.
  - `poll_until(...)` polls a predicate against a `Duration` deadline; on timeout
    it builds a `RunnerFailure` with reason
    `"condition did not become true before timeout"`.
- `dev-docs/tmux-scenarios/dashboard-list-windowing.json`: `config` block has
  `cols/rows/history_limit/initial_wait_ms/assert_mode` but **no**
  `wait_timeout_ms`, so it gets the platform default.
- `src/harness/runner_tests.rs::guarded_real_dashboard_lists_window_fixture_rows`
  parses the scenario file directly via `include_str!` and does NOT apply
  `scenario_with_wait_timeout` (unlike its siblings).
- `dev-docs/testing/tmux-harness.md` already documents `wait_timeout_ms` as a
  JSON-only config field; no doc change is needed.
- `tests/core/tmux_harness_docs_contracts.rs` asserts only CI-YAML text and that
  shipped scenarios parse/expand — it does NOT assert scenario JSON contents, so
  adding a field won't break the contract.
- Windows CI job (`runs-on: windows-latest`) has `timeout-minutes: 60`
  (3600s), so a 30s budget is comfortably bounded.

### Proposed Design

**Phase 1 — scenario budget (data-only, no code change):**

Add `"wait_timeout_ms": 30000` to the `config` object in
`dev-docs/tmux-scenarios/dashboard-list-windowing.json`. The test inherits the
budget automatically because `effective_wait_timeout` honors a non-zero value.
Passing runs are unaffected (`waitFor` returns as soon as the condition matches);
the larger budget only adds headroom for slow Windows cold starts.

**Phase 2 — self-diagnosing timeout failure messages:**

In `src/harness/runner.rs`, extend `poll_until`'s timeout failure construction to
report the configured budget (`timeout`) and the actual elapsed wait (derived
from the existing deadline arithmetic). Scope strictly to the failure path — do
not touch `POLL_INTERVAL` or success-path behavior.

## Acceptance Matrix

| # | Actor/Path | Input | Success Behavior | Failure Behavior | Test |
|---|-----------|-------|-----------------|------------------|------|
| A1 | dashboard-list scenario parsed by the guarded test | cold start on slow Windows runner; 25-repo/25-agent fsync'd state | `waitFor "Repository 24"` succeeds within the scenario's explicit 30s budget; test runs all 9 steps | (no longer times out at the 15s platform default) | existing `guarded_real_dashboard_lists_window_fixture_rows` (now 30s-backed) |
| A2 | `dashboard-list-windowing.json` parse + expand | JSON with new `wait_timeout_ms` field | parses and expands; `effective_wait_timeout(30000) == 30s` | parse/expand error | existing `shipped_tmux_scenarios_parse_and_expand` contract + `effective_wait_timeout_*` unit test |
| A3 | `waitFor`/`waitForNot` predicate never matches within budget | predicate always returns false; small budget | (n/a) | failure reason includes configured budget (ms) and actual elapsed wait (ms) | new unit test `wait_for_timeout_reports_budget_and_elapsed` |
| A4 | `waitForExit` predicate never matches within budget | pane never dies; tiny `timeout_ms` | (n/a) | failure reason includes configured budget and actual elapsed (consistent with A3 — `waitForExit` also goes through `poll_until`) | existing `timeout_failure_uses_failing_step_context` (unchanged shape; still asserts `step_index`, `step_kind`, contains "timeout") |
| A5 | successful `waitFor`/`waitForNot` | predicate matches quickly | returns immediately; no behavior change | (n/a) | existing `wait_for_succeeds_when_later_capture_matches` (unchanged) |

## Vertical Slices

### Slice 1: Give the dashboard-list scenario an explicit 30s wait budget
- **Acceptance rows**: A1, A2
- **Files**: `dev-docs/tmux-scenarios/dashboard-list-windowing.json`
- **Change**: add `"wait_timeout_ms": 30000` to the `config` object.
- **RED**: (informational only — the flake is environment-dependent and cannot
  be reproduced deterministically on this host; the change is data-only and
  proven by the contract test that ships scenarios must parse/expand).
- **GREEN**: contract test `shipped_tmux_scenarios_parse_and_expand` still
  passes; `effective_wait_timeout_*` proves 30000 → 30s.
- **Verification**: `cargo test --test core tmux_harness_docs_contracts` and
  `cargo test -p jefe harness::runner::tests::effective_wait_timeout`.

### Slice 2: Enrich waitFor/waitForNot/waitForExit timeout reporting with timing
- **Acceptance rows**: A3, A4, A5
- **Files**: `src/harness/runner.rs`, `src/harness/runner_tests.rs`
- **Change**: in `poll_until`, when the deadline is reached, compute the elapsed
  duration and include both the configured budget and the elapsed wait in the
  failure reason. Add a focused unit test asserting the enriched reason
  contains budget and elapsed information.
- **RED**: add the new unit test first (asserts reason contains budget/elapsed);
  it fails against the current reason string.
- **GREEN**: extend `poll_until` failure construction; new test passes; existing
  `timeout_failure_uses_failing_step_context` and `wait_for_succeeds_when_later_capture_matches`
  remain green.
- **Verification**: `cargo test -p jefe harness::runner::tests`.

## Scope Ledger

| Date | Item | Type |
|------|------|------|
| 2026-07-25 | Initial plan | — |
| 2026-07-25 | Investigated two clippy1.97 lints (`manual_is_multiple_of`, `duration_suboptimal_units`). Confirmed they are **false positives**: with the project MSRV (1.75, set in `.github/clippy/clippy.toml`) the suggested replacements (`is_multiple_of` 1.87, `from_hours` 1.85 — also non-std) violate MSRV. The original `%4 != 0` / `from_secs(3600)` code is MSRV-correct and passes CI clippy. No change needed; `validate.rs` and `jefe-capture-shim.rs` left untouched. (Earlier iteration incorrectly applied the replacements and broke both Linux + Windows clippy gates — reverted.) | Reject |
| 2026-07-25 | `runner_tests.rs` was at991 lines (under the1000 hard limit). Adding a standalone timing test breached1000. Folded the budget/elapsed assertion into the existing `timeout_failure_uses_failing_step_context` test instead of adding a new function; file stays at994. | In-scope-Fix |

## Review Counters
- Local OCR: 0/2
- PR OCR: 1/2 (run #1 on commit `2fbb2cf`; 1 inline finding on `jefe-capture-shim.rs` — correctly flagged the non-std `from_hours` introduced in that revision; resolved by reverting to the MSRV-correct original)

## Verification
- `make quick-check` during iteration
- `make ci-check` before push
- Windows timeout budget sanity-checked against `timeout-minutes: 60` (3600s)
  on the `windows-latest` job in `.github/workflows/ci.yml`.
- Exact-head local gates (rebased onto `origin/main`): `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo build --workspace --all-features --locked`, and
  `cargo test --workspace --all-features --locked` all pass.
- PR: https://github.com/vybestack/llxprt-jefe/pull/415 — MERGEABLE, no conflicts.
