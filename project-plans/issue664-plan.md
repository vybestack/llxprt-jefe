# Issue #664 — silent process-tree death during attach churn after an impossible `Replaced` observation

Branch: `issue664` (from `origin/main` @ `0b73a19a`).

## 1. Acceptance matrix

| # | Actor / launch path | Inputs and boundary cases | Observable success | Observable failure + diagnostic location | Side effects allowed | Behavioral proof |
|---|---|---|---|---|---|---|
| A1 | Windows liveness cycle → `observe_server_liveness` → `classify_observation` | Probe answers with a pid different from the pinned prior. Boundaries: new `started_at` strictly newer; equal; older; either side `None` | A differing identity whose creation time is **strictly newer** than the prior's is still `Replaced` | A differing identity that is **not** strictly newer yields the new `ServerLivenessObservation::ConflictingIdentity(observed)`; logged at `warn` from `server_health_io.rs` with prior/observed pid and `started_at` | None. No `exit-empty` remediation, no pin update | `src/runtime/server_health_io_tests.rs` — classification tests over the pure seam |
| A2 | `app_shell_liveness::handle_windows_cycle` | A `ConflictingIdentity` observation while agents are tracked | No agent transitions to `ServerLost`; the pinned prior identity is unchanged; the cycle continues | Conflict recorded; state untouched (same posture as `Unavailable`) | None | `tests/app_shell_liveness_conflicting_identity.rs` |
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
| S1 | A1, A2 | Runtime classification + app-shell liveness reducer | `src/runtime/server_health.rs`, `src/runtime/server_health_io.rs`, `src/app_shell_liveness.rs`, `tests/app_shell_liveness_conflicting_identity.rs` |
| S2 | A3 | Runtime classification tests | `src/runtime/server_health_io.rs` (test seam only), `src/runtime/server_health_io_tests.rs`, `src/runtime/mod.rs` (test module wiring) |
| S3 | A4 | Windows ownership boundary | `tests/core/job_self_containment_contract.rs`, `tests/core/mod.rs`, `src/runtime/job_object.rs` (doc only), `src/runtime/job_object_tests.rs` |
| S4 | A5 | Runtime attach boundary | `src/runtime/viewer_teardown.rs`, `src/runtime/viewer_teardown_tests.rs`, `src/runtime/attach.rs`, `src/runtime/manager.rs`, `src/runtime/mod.rs` |

## 4. Scope ledger

| Entry | Status | Justification |
|---|---|---|
| New crate-internal module `src/runtime/viewer_teardown.rs` | Accepted | Required by A5; the only way to serialize two independent attach paths at one point. Crate-internal, no new public API. |
| New `ServerHealth::Conflicting` / `ServerLivenessObservation::ConflictingIdentity` variants | Accepted | Required by A1; the issue asks for an outcome distinct from `Replaced` and from `Unavailable`. |
| Follow-up: `classify_observation` discards `ServerInstanceToken` | Deferred | Filed as a follow-up issue; see non-goals. |
| Follow-up: run-boundary breadcrumbs | Deferred to #662 | See non-goals. |

## 5. Review counters

- Local OCR runs used: 0 / 2
- Post-PR OCR runs used: 0 / 2

## 6. Verification evidence

- [ ] `make ci-check` on candidate head
- [ ] CI (including native Windows and coverage) green on exact head
