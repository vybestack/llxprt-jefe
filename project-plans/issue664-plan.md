# Issue #664 — silent process-tree death during attach churn after an impossible `Replaced` observation

Branch: `issue664` (from `origin/main` @ `0b73a19a`).

## 1. Acceptance matrix

| # | Actor / launch path | Inputs and boundary cases | Observable success | Observable failure + diagnostic location | Side effects allowed | Behavioral proof |
|---|---|---|---|---|---|---|
| A1 | Windows liveness cycle → `observe_server_liveness` → `classify_observation` | Probe answers with a pid different from the pinned prior. Boundaries: new `started_at` strictly newer; equal; older; either side `None` | A differing identity whose creation time is **strictly newer** than the prior's is still `Replaced` | A differing identity that is **not** strictly newer yields the new `ServerLivenessObservation::ConflictingIdentity(observed)`; logged at `warn` from `server_health_io.rs` with prior/observed pid and `started_at` | None. No `exit-empty` remediation, no pin update | `src/runtime/server_health_io_tests.rs` — classification tests over the pure seam |
| A2 | `app_shell_liveness::handle_windows_cycle` | A `ConflictingIdentity` observation while agents are tracked | No agent transitions to `ServerLost`; the pinned prior identity is unchanged; the cycle continues | Conflict recorded; state untouched (same posture as `Unavailable`) | None | `src/app_shell_liveness.rs` in-file `mod tests` — `plan_server_cycle` reducer tests (`handle_windows_cycle` is private to the binary crate, so the proof lives beside it rather than in `tests/`) |
| A3 | Coverage of `src/runtime/server_health_io.rs` | `Replaced` branch first, then `Healthy`, `Gone`, unparseable stdout, non-`CommandCompleted` evidence, exit-empty target selection | The module's classification and exit-empty-selection logic execute under the default `cargo test` (no `psmux-smoke` feature, no live server) | n/a | None | `src/runtime/server_health_io_tests.rs` |
| A4 | jefe dashboard/TUI process (`src/main.rs` non-internal path) | Any startup that is **not** `--jefe-internal-agent-launch` | jefe's own process is never assigned to a job object it owns, so dropping a `JobContainment` handle can never terminate jefe itself | A change that makes `JobContainment::enable_for_current_process` reachable from a non-internal-launch path fails the contract test | None | `tests/core/job_self_containment_contract.rs` + `src/runtime/job_object_tests.rs` (drop of an owned containment kills only the contained child; the owner survives) |
| A5 | Both attach paths: background `perform_async_attach` → `build_viewer` and synchronous `RuntimeManager::attach` | A viewer teardown started by `drop_viewer_in_background` is still in flight when a new `AttachedViewer::spawn`/`spawn_remote` begins | The new spawn does not begin until the in-flight teardown(s) complete | A teardown that never completes must **not** deadlock the UI: the wait is bounded and, on expiry, logs a `warn` and proceeds | Bounded delay (≤ `VIEWER_TEARDOWN_WAIT`) on the attach path | `src/runtime/viewer_teardown_tests.rs` (gate semantics + ordering + bounded expiry) and an `AttachedViewer::spawn` wiring test proving the spawn observes the gate |

### Boundary decisions recorded

- The monotonicity guard lives in `server_health_io::classify_observation`, **not** in the pure `classify_server_liveness`. Only the I/O path has real creation timestamps; `parse_server_identity_output` hardcodes `started_at = 1` as a placeholder, so applying the guard to the pure path would classify every parsed identity as conflicting.
- When either side's `started_at` is `None` the comparison is unverifiable, so the guard **fails open** to today's behavior (`Replaced`). Converting uncertainty into a new outcome would be the same mistake the ownership model already forbids.
- `ConflictingIdentity` carries the observed identity so the next occurrence is attributable from the log alone.
- The attach gate waits inside `AttachedViewer::spawn_command`, the single funnel shared by `spawn`, `spawn_remote`, and `spawn_with_plan`, so both attach paths serialize at one point rather than each caller remembering to.

## 2. Non-goals

- **Acceptance item 5 of the issue (run-boundary breadcrumbs) is deferred to #662.** The issue's own "Non-goals" section names "the general observability work (#662)", and run-boundary records are #662's acceptance item 1. Implementing them here would duplicate that issue.
- Coverage-gate work (#663).
- Orphan reaping (#651) and psmux→ConPTY attach misbehaviour (#546).
- Making the identity probe authoritative under concurrent servers in general. In particular, `classify_observation` builds the current identity with `ServerIdentity::new`, which discards the parsed `#{server_instance}` token, so the token-decisive branch of `classify_server_health` never fires on this path. That is a real, separate defect; it is recorded as a follow-up rather than fixed here, because restoring the token changes classification semantics well beyond the accepted rows.
- Removing either attach path or unifying the background/synchronous attach architecture.

## 3. Slices

| Slice | Acceptance rows | Owner / boundary | Allowed paths |
|---|---|---|---|
| S1 | A1, A2 | Runtime classification + app-shell liveness reducer | `src/runtime/server_health.rs`, `src/runtime/server_health_io.rs`, `src/app_shell_liveness.rs` |
| S2 | A3 | Runtime classification tests | `src/runtime/server_health_io.rs` (test seam only), `src/runtime/server_health_io_tests.rs`, `src/runtime/mod.rs` (test module wiring) |
| S3 | A4 | Windows ownership boundary | `tests/core/job_self_containment_contract.rs`, `tests/core/mod.rs`, `src/runtime/job_object.rs` (doc only), `src/runtime/job_object_tests.rs` |
| S4 | A5 | Runtime attach boundary | `src/runtime/viewer_teardown.rs`, `src/runtime/viewer_teardown_tests.rs`, `src/runtime/attach.rs`, `src/runtime/manager.rs`, `src/runtime/mod.rs` |

## 4. Scope ledger

| Entry | Status | Justification |
|---|---|---|
| New crate-internal module `src/runtime/viewer_teardown.rs` | Accepted | Required by A5; the only way to serialize two independent attach paths at one point. Crate-internal, no new public API. |
| New `ServerLivenessObservation::ConflictingIdentity` variant | Accepted | Required by A1; the issue asks for an outcome distinct from `Replaced` and from `Unavailable`. `ServerHealth` is deliberately left alone: the guard sits above `classify_server_health`, on the only seam that has real creation timestamps. |
| Private `ServerCycleAction` reducer in `src/app_shell_liveness.rs` | Accepted | Required by A2; `handle_windows_cycle` is a private async fn that probes the OS, so the decision it makes had to be separable to be provable. Not public API. |
| Follow-up: `classify_observation` discards `ServerInstanceToken` | Deferred | Filed as a follow-up issue; see non-goals. |
| Follow-up: run-boundary breadcrumbs | Deferred to #662 | See non-goals. |

## 5. Review counters

- Local review runs used: 1 / 2
- Post-PR OCR runs used: 2 / 2 (cap reached)

### Local review 1 — triage

| Finding | Validity | Disposition | Reason |
|---|---|---|---|
| The plan named `tests/app_shell_liveness_conflicting_identity.rs` as the A2 proof, but that file does not exist | valid | **In-scope—Fix** | Fixed: the A2 row and slice S1 now name the real proof location. `handle_windows_cycle` is private to the binary crate, so the reducer tests live beside it. |
| `production_text`'s test-gate detection is a substring heuristic, so a future `#[cfg(feature = "integration-test")]` could be mis-stripped | valid but hypothetical | **Reject** | Not a defect in the current tree — every `#[cfg(...test...)]` in `src/` is a genuine test gate, and the scan carries three anti-vacuity guards. Writing a `cfg` parser to defend a config that does not exist is speculative hardening. |
| Test 3 of the containment contract does not filter `*_tests.rs` the way test 1 does | valid | **Reject** | Fails in the safe direction: a `_tests.rs` mentioning `run_launch_plan(` would add a caller and fail the test, never hide one. |
| The wiring test holds the process-global gate for ~150 ms, which can add latency to a concurrent attach test | valid | **Reject** | Bounded by `VIEWER_TEARDOWN_WAIT` and cannot deadlock or mis-assert; a global gate is the point of the fix. |
| Fail-open when `started_at` is `None` preserves the old `Replaced` for unverifiable comparisons | valid | **Reject** | Recorded design decision, not a defect. Manufacturing a conflict from absent evidence is the failure mode the ownership model forbids. |

Reviewer verdict: no blockers; all four accepted behaviors correctly and provably
implemented; the gate is free of deadlock, livelock, and UI wedging; no vacuous
tests remain.

### Post-PR OpenCodeReview 1 — triage

| Finding | Validity | Disposition | Reason |
|---|---|---|---|
| `production_text` strips `#[cfg(not(any(test, …)))]` and `#[cfg(not(all(test, windows)))]`, which are compiled **in** production, so a real call site could be hidden | valid | **In-scope—Fix** | Supersedes the local reviewer's hypothetical version of the same finding, which was rejected as speculative: OCR named concrete negated-`cfg` forms whose mis-stripping is a false *negative* on the #664 invariant. Fixed by keeping any negated `cfg`, so the scan errs toward a loud failure instead of silent blindness. The suggested `cargo-expand`/`syn` remedy is rejected — a new build dependency is out of scope. |
| `Mutex` poison recovery via `into_inner()` is silent in `viewer_teardown.rs` | partial | **Reject** | The protected state is a counter whose `TeardownGuard::drop` decrements during unwind, so the invariant survives poisoning by construction. Failing closed would wedge the attach path — the exact freeze this issue removes. The logging half is #662's scope. |
| `matches("establish_worker_containment()").count()` also counts comments | valid but immaterial | **Reject** | Fails in the loud direction only: a stray comment reddens the test, it can never hide a real caller. |
| `split_once("\nfn ")` body extraction is fragile | partial | **Reject** | `src/main.rs` has later top-level `fn`s, so the split terminates correctly today; the residual risk is a narrow over-capture that does not justify a parser dependency here. |
| `is_none_or` requires Rust 1.82, may break MSRV | invalid | **Reject** | `edition = "2024"` already requires ≥ 1.85, and CI compiled the file on every platform. |

### Post-PR OpenCodeReview 2 — triage

| Finding | Validity | Disposition | Reason |
|---|---|---|---|
| `a_wedged_teardown_expires_the_bound_instead_of_blocking_forever` passes a 50 ms bound but asserts only `waited < VIEWER_TEARDOWN_WAIT` (500 ms) | valid | **In-scope—Fix** | This test *is* the A5 evidence that a wedged teardown cannot freeze the UI, and the assertion did not exercise the argument: an implementation that ignored its bound and used a longer internal default would still have passed. Now asserts `waited >= BOUND` and `waited < BOUND * 4`, so the honoured value must be the one passed. |
| `replaces` fails open when either `started_at` is absent | valid, already covered | **Reject** | This is the recorded boundary decision (§1), and the requested justification comment is already present verbatim at `server_health_io.rs` L92–94: an absent discriminator makes the ordering *unverifiable*, not contradictory, so manufacturing a conflict from weak evidence would regress liveness on hosts that cannot supply creation times. |
| `classify_resolved_identity` clones `current` into each observation variant | valid, immaterial | **Reject** | `ServerIdentity` is a pid, an optional `u64` and a short version string, cloned once per 2 s liveness poll. Borrowing would push a lifetime through `ServerLivenessObservation` and every consumer for no measurable gain. |
| `production_text` brace counting ignores braces inside string literals and comments | unverifiable hypothesis | **Defer** | Not demonstrated against current sources, and mis-stripping is anchored loudly: `worker_containment_is_established_only_while_running_a_launch_plan` asserts exact `establish_worker_containment()` counts on the same extracted text, so a truncation reddens the suite. The proposed `syn` remedy is a new dependency and out of scope. |
| Global `viewer_teardown()` singleton makes the tests order-dependent | valid, immaterial | **Reject** | Four of the five tests construct isolated `ViewerTeardown::new()` instances; only the wiring proof touches the process-global, and it asserts a *lower* bound (`waited >= HOLD`), which concurrent teardown can only strengthen. |
| `is_none_or` requires Rust 1.70+, may break MSRV | invalid | **Reject** | Duplicate of an OCR 1 finding: `edition = "2024"` already requires ≥ 1.85 and CI compiles the file on every platform. |
| `a_viewer_spawn_waits_for_a_teardown_held_on_the_shared_gate` is a valid, non-vacuous wiring proof | — | **No action** | Reviewer confirmation, not a finding. |

## 6. Verification evidence

- [x] `cargo xtask fmt`, `check clippy-allows`, `check source-size`, `check architecture`, `check multiplexer-surface`, `lint`, `complexity`, `build` — all pass
- [x] `cargo test --workspace --all-features --locked` — 0 failures (3754 lib, 870 integration, all other targets green)
- [x] `cargo xtask coverage` — 71.21% lines against the 30% floor. A3 evidence: `src/runtime/server_health_io.rs` moved from **0% to 83.78%** lines; `src/runtime/viewer_teardown.rs` 97.92%
- [x] `cargo xtask coverage-windows` — every Windows-only floor met: `job_object.rs` 71.15% (floor 65), `app_shell_liveness.rs` 46.98% (floor 38), `server_health_io.rs` 33.33% (floor 0), `session_host.rs` 74.11% (floor 65), `agent_launcher.rs` 80.70% (floor 75), `attach.rs` 57.83% (floor 34)
- [x] CI watched to conclusion on every pushed head. Green shape is 18 pass / 0 fail / 3 skipped, including Native Windows (MSVC + psmux), Windows coverage floors, Coverage gate, Windows Clippy, Architecture boundary policy, Uncertain-observation coercion policy, and Mergeability gate. Skipped jobs are `Main flake baseline record` (main-only), `Optional TUI smoke (tmux)`, and `Notify OCR infrastructure failure` (fires only on OCR failure). The final head's result is recorded on PR #669
- [x] Blocker-Fix, self-found from CI on `76b12da5`: the OCR 2 lower-bound assertion flaked on Windows (`the wait returned after 49.5437ms, before its own 50ms bound elapsed`). A timed condvar wait and `Instant` do not share a clock source, so the wait can return one scheduler tick short. The assertion now allows 20 ms of timer slop, which still fails an immediate return and still fails a wait that ignores its argument for the 500 ms production default
- [x] Ancestry: `git rev-list --left-right --count origin/main...issue664` — 0 behind, 7 ahead; no conflicts
