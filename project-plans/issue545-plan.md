# Issue #545 — Windows CI must produce real Windows signal

Branch: `issue545` (cut from `origin/main` @ `09e1c9f6`, verified 0 ahead / 0 behind)

## Invariant being restored

> The `windows_native` job must produce real, complete Windows signal on every
> run, and a red Windows result must be trustworthy enough that nobody is
> tempted to dismiss it.

## Grounding evidence (reproduced locally on this branch)

| Claim in issue | Local reproduction | Result |
|---|---|---|
| main red on `cargo fmt` at `src/runtime/agent_probe.rs:336` | `cargo fmt --all --check` | CONFIRMED — diff at `agent_probe.rs:336` |
| main red on source-file-length policy | `cargo xtask check source-size` | CONFIRMED — `tests/issue382_behavior.rs` 1028 lines (max 1000) |
| portable checks gate native steps | `.github/workflows/ci.yml` L214-234 precede every native step | CONFIRMED |
| workspace-wide serialization | `RUST_TEST_THREADS: "1"` (L158) + `-- --test-threads=1` (L252) | CONFIRMED |
| coverage is Ubuntu-only | `coverage` job `runs-on: ubuntu-latest` (L110); no Windows coverage | CONFIRMED |

### Root cause of the parallelism hazard (why `--test-threads=1` was reached for)

psmux namespace generators are not uniformly collision-proof. On Windows the
system clock has coarse resolution (~0.5–15.6 ms), so a nanosecond timestamp is
**not** a unique value between two threads in the same tick:

| Generator | Composition | pid | atomic counter | Safe under parallelism |
|---|---|---|---|---|
| `tests/psmux_attach.rs` | `jefe-attach-{pid}-{nanos}-{seq}` | yes | yes | yes |
| `tests/psmux_server_loss.rs` | `jefe-issue493-{pid}-{nanos}-{seq}` | yes | yes | yes |
| `src/harness/psmux_driver.rs` | `jefe-harness-{pid}-{nanos}-{seq}` | yes | yes | yes |
| `tests/psmux_smoke.rs` | `jefe-psmux-{label}-{pid}-{nanos}` | yes | **no** | **no** |
| `tests/psmux_smoke_mouse.rs` | `jefe-psmux-{label}-{pid}-{nanos}` | yes | **no** | **no** |
| `tests/psmux_orphan_reap.rs` | `jefe-orph-{label}-{nanos}` | **no** | **no** | **no** |
| `tests/psmux_session_host.rs` | `jefe-467-{label}-{nanos}` | **no** | **no** | **no** |

The correct remedy is per-test namespace uniqueness (isolation), not job-wide
serialization. This is exactly what the issue demands and what V4 verifies.

## Acceptance matrix

| ID | Actor / launch path | Input & boundary | Platform | Observable success | Observable failure & diagnostic | Side effects before failure | Persistence / compat | Proving test |
|---|---|---|---|---|---|---|---|---|
| A1 | `windows_native` job, PR with formatting error | fmt violation present | Windows CI | every native step still executes and reports | native step failure reported on its own step | none | workflow only | `ci_windows_native_runs_no_portable_gate_before_native_steps` |
| A2 | `windows_native` job, PR with clippy / policy violation | clippy + policy violation | Windows CI | native steps still execute; portable checks reported by their own Ubuntu jobs | independent job red | none | workflow only | `ci_portable_checks_are_enforced_off_the_windows_native_job` |
| A3 | Windows-specific lint coverage | `cfg(windows)` lint gap | Windows CI | strict clippy runs as an independent job, never as a prefix | `windows_clippy` job red on its own | none | workflow only | `ci_windows_clippy_is_an_independent_job` |
| A4 | Windows test suite | default parallelism | Windows CI | no `RUST_TEST_THREADS` and no `--test-threads=1` anywhere in workflow | suite red on real races, not on serialization | none | workflow only | `ci_windows_native_does_not_serialize_the_workspace_suite` |
| A5 | Two concurrent psmux tests | same label, same clock tick | Windows + Unix | namespaces distinct; neither observes the other's sessions | assertion names both namespaces | psmux servers spawned then killed | per-test server lifetime | `concurrent_psmux_namespaces_are_distinct_and_mutually_invisible` |
| A6 | Namespace generators | 10k calls, same tick | any | all generated names unique | duplicate name reported | none | none | `psmux_namespace_generator_is_unique_under_same_tick_contention` |
| A7 | Coverage gate | Windows-only module below floor | Windows CI | build fails naming the module and its floor | module name + actual vs floor | none | floors are data, not code | `windows_coverage_floor_fails_when_module_regresses` |
| A8 | `windows_native` result consumer | job "green" because steps skipped | Windows CI | a completion gate fails when any native step did not execute | gate names the skipped step | none | required check name stable | `ci_windows_native_completion_gate_rejects_skipped_native_steps` |
| A9 | Scheduled main run | nightly | Windows CI | retrievable flake record artifact | record absent | none | artifact retention | `ci_schedules_a_main_flake_baseline_run` |
| A10 | `cargo fmt --all --check` on this branch | current tree | any | exits 0 | rustfmt diff | none | n/a | gate command |
| A11 | `cargo xtask check source-size` on this branch | current tree | any | exits 0 | file over hard limit | none | n/a | gate command |

## Non-goals (explicit)

- Not stabilising the intermittent Page-key/mouse ConPTY input-loss failure —
  tracked separately; must not be weakened here.
- Not raising any timeout as a remedy (`#456` precedent explicitly rejected).
- Not marking any test `#[ignore]`, `allow_failure`, or `continue-on-error`.
- Not keeping `--test-threads=1` "for now".
- Not removing portable checks from the repo — only removing their **gating**
  position in the Windows job; they remain enforced in dedicated Ubuntu jobs.
- Not refactoring psmux test behavior beyond namespace-uniqueness hardening.
- Not changing production runtime behavior of `job_object.rs`, `attach.rs`,
  `session_host.rs`, `agent_launcher.rs` — coverage floors observe them only.

## Vertical slices

### Slice 1 — Restore green main (A10, A11) — precondition for V7
- RED: `cargo fmt --all --check` and `cargo xtask check source-size` both fail.
- GREEN: both exit 0.
- Files: `src/runtime/agent_probe.rs`, `tests/issue382_behavior.rs` (split).
- Stop if: splitting the test file changes any assertion semantics.

### Slice 2 — Ungate native steps (A1, A2, A3)
- RED: contract tests in `tests/core/windows_ci_signal_contracts.rs`.
- GREEN: `ci.yml` — portable checks removed from `windows_native`; strict clippy
  relocated to an independent `windows_clippy` job.
- Stop if: any portable check turns out not to be enforced on Ubuntu.

### Slice 3 — De-serialize the Windows suite (A4, A5, A6)
- RED: A5/A6 isolation tests fail against current generators; A4 contract fails.
- GREEN: pid + process-wide atomic counter in every psmux namespace generator;
  `RUST_TEST_THREADS` and `--test-threads=1` removed.
- Stop if: a test proves genuinely un-isolatable (then serialize *that test*
  via a test-level mechanism, never the job).

### Slice 4 — Windows coverage enforcement (A7)
- RED: no Windows coverage gate exists.
- GREEN: per-module floors for Windows-only modules + `windows_coverage` job.

### Slice 5 — Skipped-vs-passed gate and flake baseline (A8, A9)
- RED: nothing distinguishes green-because-skipped.
- GREEN: completion gate job + scheduled main flake-baseline run.

## Scope ledger

| Change | Acceptance row | Approval |
|---|---|---|
| `.github/workflows/ci.yml` | A1–A4, A7–A9 | issue is *about* CI; user directed "fix it" |
| `src/runtime/agent_probe.rs` | A10 | issue deliverable 5 |
| `tests/issue382_behavior.rs` split | A11 | issue deliverable 5 |
| psmux namespace generators | A5, A6 | issue deliverable 2 |
| `xtask` coverage floors | A7 | issue deliverable 3 |
| new contract test file | A1–A4, A8, A9 | proving tests |

No file or line budget applies to this issue (stated explicitly in the issue
text). Scope is governed by the acceptance matrix and non-goals above.

## Items that CANNOT be completed by code in this PR

| ID | Why | Owner |
|---|---|---|
| V6 branch protection | GitHub repo admin setting, not repository content. This PR supplies the *required check* (`windows_native_complete`); enabling it in branch protection needs admin rights. | user / repo admin |
| V3 (10 consecutive green runs) | Observational, post-merge, needs wall-clock time | post-merge |
| V7 (main green 7 consecutive runs) | Observational, post-merge | post-merge |
| V8 evidence | Requires a scheduled run to actually fire post-merge | post-merge |

## Review counters

- OCR before PR: 0 / 2
- OCR after PR: 0 / 2

## Verification evidence

All commands run on native Windows (win32, 16 CPU) with `JEFE_REQUIRE_PSMUX=1`.

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | exit 0 |
| `cargo xtask check source-size` | exit 0 |
| `cargo xtask check architecture` | exit 0 |
| `cargo xtask check clippy-allows` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| complexity clippy gate | exit 0 |
| `cargo test --workspace --all-features --locked` | **62 targets, 0 failures** |
| psmux suite at default parallelism | 23 tests green; 6 consecutive `psmux_smoke` trials green; 1 trial green under a concurrent full rebuild |
| `js-yaml` parse of `ci.yml` | valid; `windows_native` has no `needs`, so nothing can skip it |

### A6 RED proof (root cause, measured not assumed)

With a timestamp-only namespace generator, on this Windows host:

```
namespace generator produced 7635 duplicates across 16000 concurrent calls
```

Duplicate namespace means a shared psmux server. This is the defect
`RUST_TEST_THREADS: 1` was masking. With pid + process-wide atomic counter the
same test yields zero duplicates.

### Defect surfaced by ungating (predicted by the issue)

`src/app_input/prs_diff_dispatch_tests.rs` fabricated an `ExitStatus` by
running `true` / `false`, which do not exist on Windows. Three tests failed
on Windows only. They had never run in CI because `windows_native` was
failing on portable checks before reaching `cargo test`. Fixed here; this is
exactly the class of defect the issue says is being hidden.

## Status by verification criterion

| ID | Status |
|---|---|
| V1 | Implemented — portable checks removed from `windows_native`; contract test enforces it. Needs a CI run on a deliberately-malformatted PR for final evidence. |
| V2 | Implemented — same mechanism; `windows_clippy` is independent. |
| V3 | `RUST_TEST_THREADS` and `--test-threads=1` removed and contract-enforced. Local: full suite green at default parallelism. The 10-consecutive-run evidence is post-merge. |
| V4 | **Done** — `tests/psmux_parallel_isolation.rs` proves distinct namespaces and mutual invisibility under real concurrency. |
| V5 | **Not started** — needs decision (see below). |
| V6 | Gate job `windows_native_complete` delivered. Enabling it in branch protection is an admin action. |
| V7 | Both red-main causes fixed, plus one further Windows-only defect. Consecutive-run evidence is post-merge. |
| V8 | Nightly schedule + `flake_baseline` artifact job delivered; evidence requires a scheduled run to fire. |
