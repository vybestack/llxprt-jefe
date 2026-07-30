# Issue 527 — Preserve live sessions across definition hash updates

## Acceptance matrix

| Row | Input / boundary | Success | Failure / safety | Evidence |
|---|---|---|---|---|
| A1 | Persisted Running local agent; stable tmux session alive; only definition hash changed | Register the existing process and retain its prior launch signature | No executable probe, fresh spawn, or session replacement | `live_session_survives_definition_hash_drift`; `observed_existing_session_returns_complete_authoritative_binding`; live test with 20 sessions |
| A2 | Persisted definition drift; signature version, typed values, or target differs | Do not reattach | Classify inconsistent and clear stale runtime state through existing startup handling | `durable_signature_distinguishes_definition_drift_from_value_and_target_changes` |
| A3 | Definition-only drift but binding names another session | Do not reattach | Preserve stable identity boundary | `binding_accepts_only_definition_drift_for_the_stable_session` |
| A4 | Definition-only drift with missing session or reused PID | Do not reattach | Missing is inconsistent; reused PID remains stale | `definition_drift_does_not_override_reused_pid_or_missing_session` |
| A5 | Multiple PTY harness fixtures run under the default parallel test runner | Fixtures execute without contending for shared terminal resources | No intermittent blank capture timeout | `cargo test --test harness_v1_fixtures --all-features --locked` |

## Non-goals

- Adopt sessions whose records were already persisted as stopped before startup.
- Relax value, target, binding, process, or remote-session validation.
- Change launch-plan authorization, package probing, or fresh-spawn behavior.
- Change the durable schema.

## Vertical slices

1. RED/GREEN startup reconciliation for definition-only drift.
2. Serialize PTY fixture execution after the full-suite parallel run demonstrated shared-resource contention.
3. Align persistence/runtime documentation and run exact-head gates.

## Expected paths

- `src/app_init.rs`
- `src/app_init_signature_reconcile.rs`
- `tests/harness_v1_fixtures.rs`
- `dev-docs/standards/persistence-and-runtime.md`
- `docs/technical-overview.md`
- `project-plans/issue527-plan.md`

## Scope ledger

| Discovery | Disposition |
|---|---|
| Default-parallel harness target repeatedly timed out with a blank capture; target passed serially and the failing test passed alone | In-scope gate fix after user approved proceeding: serialize resource-owning fixture runs |
| Existing untracked `.llxprt/settings.json`, `nohup.out`, and `toy1/` | Excluded; untouched |

## Review counters

- Local Rust reviewer: 0
- Local Open Code Review: 0/2
- Post-PR Open Code Review: 0/2

## Verification evidence

- RED: `cargo test --bin jefe app_init::tests::live_session_survives_launch_signature_drift -- --exact` failed with Inconsistent vs Running before the test was refined to `live_session_survives_definition_hash_drift`.
- GREEN: all focused `app_init::tests` passed.
- Live recovery: 20 persisted Running records matched 20 live panes; fixed release binary reattached all 20 sessions.
- Source size: hard gate passed; `src/app_init.rs` remains below 1,000 lines.
- Strict Clippy and all-features locked build passed.
- Parallel fixture target passed after serialization.

## Deferred findings

None.
