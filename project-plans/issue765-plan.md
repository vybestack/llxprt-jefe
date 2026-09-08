# Issue #765 delivery plan — semantic-continuation-macos intermittent CI failure

## Corrected diagnosis (code-verified)

The observed CI signature `exit=124; report step count=10, expected=20` is
`validate_report` output in `scripts/run-scenario-manifest.py` after a
well-formed report was received and written to the reports directory.

- `exit=124` is the schema-1 harness's own bounded wait-timeout exit code
  (`src/harness/v1/error.rs`: `HarnessError::wait_timeout`), not the manifest
  runner's outer timeout (that path kills the process group and raises
  `scenario exceeded its outer timeout` with no report) and not a job timeout.
- A 10-step report means steps 0–9 recorded; the harness records every executed
  step (passed or failed) and stops at the first failure
  (`src/harness/v1/runner.rs::execute`). The failed step is index 9:
  `wait frame "Confirm alpha deployment?" timeout_ms 15000`.
- Therefore this is not a hang. The confirmation modal (gated on a
  provider-continuation round trip to the persistent `/bin/sh` provider after
  `key f3`) did not render within 15 s. The harness honored its bounds,
  emitted a complete report (including the frame observed at timeout via
  `wait_for` → `record_frame`), and exited 124.
- The report is written to the shard reports directory before validation and
  the evidence upload step runs `if: always()` (it succeeded in run
  34279566901 attempt 1). The triage gap the issue hit is that a failed-jobs
  rerun overwrites `tui-scenarios-macos-5` (attempt 2 replaces attempt 1), and
  the report content never reaches the job log.

## Acceptance matrix

| Row | Actor / launch path | Input & boundary cases | Observable success behavior | Observable failure behavior & diagnostic location | Side effects permitted | Persistence / compatibility | Behavioral proof |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | `scripts/run-scenario-manifest.py` shard execution (CI shard job or local driver invocation) | A scenario report that fails `validate_report` for any reason (exit mismatch, step-count mismatch, step identity, capture mismatch) | The driver's per-scenario stderr failure line embeds triage evidence from the report: failed step index/op/error and the lines of the final observed frame | Malformed runner output keeps today's `missing report` / `invalid report JSON` errors; no traceback; validation rules unchanged | Log/stderr text only | Report file format, directory layout, and validation semantics unchanged | New case in `tests/scenario_manifest/driver_tests.rs`: stub runner emits a failing report (exit 124, truncated steps, step error text, frame lines); assert driver stderr contains the step error text and a frame line (RED before the change) |
| A2 | ci.yml `tui_scenarios_linux` / `tui_scenarios_macos` shard jobs | A shard job whose "Execute exact required … scenario shard" step fails (any failure, incl. the #765 signature) | An additional artifact `scenario-failures-<platform>-<shard>-run<run_id>-attempt<run_attempt>` uploads the shard reports directory, so failure evidence survives failed-jobs reruns | Upload skipped on success (`if: failure()`); `if-no-files-found: ignore` covers failures that occur before any report exists; the completion gate's download patterns (`tui-scenarios-<platform>-*`) do not match the new name, so gate semantics are unchanged | One artifact upload on failure | Existing `tui-scenarios-*` artifacts, `_completion.json` protocol, and `--verify-completion` inventory checks untouched | YAML review + exact-head PR CI (success path proves non-interference; name-prefix disjointness argued above) |
| A3 | Issue #765 record | n/a | Issue comment corrects the diagnosis (no hang; step-9 bounded wait timeout; exit-code semantics) and records rejected/deferred directions with reasoning | n/a | none | n/a | Comment content approved by coordinator |
| A4 | App provider/confirmation path | Conditional: only if the deep analysis identifies a concrete intermittent-stall mechanism in `src/runtime/provider/` or the confirmation dispatch path | Targeted minimal fix with a unit test naming the mechanism | If no concrete mechanism is identified: recorded as pending-evidence (next CI failure now produces diagnosable logs/artifacts via A1+A2); no speculative change | none | Per fix | Rust unit test (RED → GREEN) |

## Non-goals

- No change to scenario wait budgets (`15000 ms`) — no evidence yet that the
  modal is slow-but-eventual rather than delayed by a real defect; the issue's
  own criteria require reproduction first.
- No harness "blocking tmux call" bounding — rejected: the harness performs no
  tmux calls in this path (the app-under-test does), and the observed failure
  is already bounded (complete report + exit 124). The only unbounded harness
  operations (`frame_lines`/`assert_frame` lock reads) would produce the
  outer-timeout, no-report signature, which was not observed.
- No renaming of the existing `tui-scenarios-*` artifacts — skipped shards do
  not re-upload on failed-jobs reruns, and the completion gate verifies an
  exact report inventory per attempt, so per-attempt names for the primary
  artifacts would break rerun gating.
- No app-behavior change without an identified mechanism (A4 gate).
- No Windows/psmux, unrelated scenario, or quality-gate changes.

## Vertical slices

1. **Slice 1 (A1)** — driver failure-evidence logging.
   - Owner: harness tooling (`scripts/run-scenario-manifest.py`,
     `tests/scenario_manifest/driver_tests.rs`).
   - RED: stub-runner test asserting stderr carries failed-step error + frame
     lines; fails on current driver (stderr has only the validation error).
   - GREEN: append report-derived evidence to the validation failure message.
   - Stop if: the change would alter report writing, validation rules, or
     exit codes.
2. **Slice 2 (A2)** — failure-evidence artifact in ci.yml (both platforms).
   - Authorized by the issue's own suggested next step #1 ("Upload scenario
     reports (with frames) for failed scenarios in the shard jobs").
   - Stop if: the gate's download patterns or the `--verify-completion`
     inventory semantics would need to change.
3. **Slice 3 (A3)** — issue comment after PR opens (diagnosis correction +
   evidence instructions for the next occurrence).
4. **A4 decision** — after the provider-path deep analysis returns: implement
   only with a concrete mechanism; otherwise record as deferred.

## Expected files

- `scripts/run-scenario-manifest.py` (A1)
- `tests/scenario_manifest/driver_tests.rs` (A1 test)
- `.github/workflows/ci.yml` (A2)
- issue #765 comment (A3)
- conditional: `src/runtime/provider/**` or confirmation dispatch (A4)

## Scope ledger

| Entry | Status |
| --- | --- |
| A1 driver evidence logging | approved (this plan) |
| A2 failure artifact upload | approved (requested by issue) |
| A4 app-side fix | blocked on investigation outcome; requires coordinator approval |

## Review counters

- OCR pre-PR: 0 / 2 used
- OCR post-PR: 0 / 2 used

## Verification evidence

- Sandbox constraint: this Linux sandbox has no Rust toolchain (`target/`
  holds macOS artifacts) and no tmux; local Rust gates cannot run here.
- Local (available): `python3 -m py_compile scripts/run-scenario-manifest.py`;
  manual driver invocation with stub runners to prove A1 RED/GREEN behavior
  (same fixture strategy as `driver_tests.rs`).
- Exact-head: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo build --workspace
  --all-features --locked`, `cargo test --workspace --all-features --locked`
  via PR CI (includes `tests/scenario_manifest`, macOS shards 0–5, linux
  shards 0–1, completion gate, coverage, and Windows jobs).

## Deferred findings / follow-ups

- Root cause of the >15 s modal delay: pending evidence. With A1+A2 the next
  main-branch occurrence yields the timeout frame and failed-step diagnostics
  in the job log plus a rerun-proof artifact.
- A4 investigation lead (from the scenario's provider script, recorded for
  verification): the `request-host-confirmation` outcome is gated on the app
  forwarding a second `selected`-beta panel-event (`read -r selected_beta`)
  after the step-8 `down` key. The deploy action's invocation timeout is 60 s
  and the provider script has no internal timeout, so a dropped or coalesced
  repeat-selection event under load would wedge the round trip well past the
  15 s harness wait. Verify in the app's panel-event delivery path whether a
  repeat selection (alpha→beta, then alpha→beta again with revision bump) can
  ever be suppressed.
