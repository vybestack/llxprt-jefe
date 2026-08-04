# Issue #642 — Orphan reaping never runs after a restart

<https://github.com/vybestack/llxprt-jefe/issues/642>

`RuntimeBinding.worker_identities` is never written to the durable document and is
hardcoded to `Vec::new()` on restore, so `orphan_evidence` always returns `NoOrphan`
after a restart. Startup then takes the plain `Stopped` path and `reap_orphaned_agent`
is never called, leaking a `jefe-session-host.exe` + `psmux server` pair (and ~26 MB of
staged binary) for every dead-pane agent.

A second defect found while shaping this plan: `app_init::restore_one_agent` maps
`StartupClassification::Orphaned` to `RestoreOneOutcome::Dead` **without reaping**,
unlike `reconcile_running_agents`, which does reap before Dead-marking. Persisting the
anchors alone would therefore still leak on the `restore_runtime_sessions` route.

## 1. Acceptance matrix

| # | Actor / launch path | Input & boundary cases | Target | Observable success | Observable failure & diagnostic | Side effects before failure | Persistence / compatibility | Proving test |
|---|---|---|---|---|---|---|---|---|
| AC1 | `state::durable_projection` (save) | Agent whose `RuntimeBinding.worker_identities` is non-empty; also the empty case | local | Schema-2 `RuntimeRecord` carries `worker_identities` in recorded order | Projection is pure and total; no failure mode introduced | none | Field omitted entirely when empty (`skip_serializing_if`), so existing documents and goldens are byte-identical | behavioral projection test |
| AC2 | `state::durable_restore::restore_agent` | `RuntimeRecord` **with** anchors; **without** the key (pre-#642 document); `session_id: None` | local | `RuntimeBinding.worker_identities` equals the recorded anchors | Missing key restores to empty rather than erroring | none | `#[serde(default)]`; `deny_unknown_fields` preserved | behavioral restore test |
| AC3 | Save → load round trip | Agent with anchors, saved then restored | local | Anchors survive the round trip unchanged | — | none | Revision semantics unchanged | round-trip test |
| AC4 | Startup classification after restart | Restored agent, session present, pane dead, validated descendants alive | local | `orphan_evidence` yields `DeadPaneWithOrphans` and `classify_startup` yields `Orphaned` (not `Stopped`) | Anchors that no longer match are `Dead`, so a stale record does not fabricate an orphan | none | n/a | behavioral test over restored binding |
| AC5 | `app_init::restore_one_agent` | `StartupClassification::Orphaned` | local | The orphan tree is reaped **before** the agent is Dead-marked and its binding cleared | Reap failures are logged and swallowed; startup is never aborted | reap is the intended effect | binding still present at reap time | behavioral test |
| AC6 | Remote repositories | Agent on a remote repository | remote | `orphan_evidence` still short-circuits to `NoOrphan`; no local reap is attempted | — | none | unchanged | existing + new assertion |

## 2. Non-goals

- Re-deriving descendants by walking the live process tree (the issue's alternative
  suggestion). Persisting the anchors is the accepted approach; the tree walk is not
  implemented here.
- Enabling `JEFE_LOG_FILE` by default. Tracked separately.
- Pruning superseded build-hash staging directories under
  `%LOCALAPPDATA%\jefe\session-hosts\`. Tracked separately.
- Changing `recover_server_lost_agents` so it revisits Dead+unbound agents.
- Making `worker_identity_from_pane` return a worker on Windows (the `BindingEvidence::Legacy`
  aggravating factor).
- Any change to reap mechanics inside `jefe::runtime::reap_orphan_session`.

## 3. Vertical slices

### Slice 1 — persist and restore the descendant anchors (AC1, AC2, AC3, AC6)

- Owner: durable state contract + projection/restore.
- Allowed paths: `src/domain/state_contract.rs`, `src/state/durable_projection.rs`,
  `src/state/durable_restore.rs`, and their existing test modules.
- RED: projection/restore/round-trip tests asserting anchors persist; a pre-#642
  document without the key still loads.
- GREEN: add the field, write it, restore it.
- Non-goals: no classification or reap changes.
- Stop if: the schema change appears to require a migration or a schema version bump.

### Slice 2 — reap on the restore route (AC4, AC5)

- Owner: `app_init` startup reconciliation.
- Allowed paths: `src/app_init.rs`, `src/app_init_orphan_reconcile.rs`,
  `src/app_init_tests.rs`.
- RED: a test proving `Orphaned` on the `restore_one_agent` route reaps before the
  binding is cleared.
- GREEN: split `Orphaned` out of the combined Dead arm and reap first.
- Non-goals: no new process-management subsystem; reuse `reap_orphaned_agent`.
- Stop if: reaping here needs new cancellation/timeout machinery.

## 4. Expected files by layer

| Layer | Files |
|---|---|
| Durable contract | `src/domain/state_contract.rs` |
| Pure projection | `src/state/durable_projection.rs`, `src/state/durable_restore.rs` |
| Startup boundary | `src/app_init.rs`, `src/app_init_orphan_reconcile.rs` |
| Tests | `src/state/durable_projection_tests.rs`, `src/app_init_tests.rs` |
| Plan | `project-plans/issue642-plan.md` |

## 5. Scope ledger

| Entry | Status |
|---|---|
| Persist `worker_identities` on `RuntimeRecord` | in scope (AC1) |
| Restore anchors instead of `Vec::new()` | in scope (AC2) |
| Reap on the `restore_one_agent` route | in scope (AC5) — discovered while shaping, same defect, recorded here |
| Default-on logging | deferred, follow-up |
| Staging-dir pruning per build hash | deferred, follow-up |
| `recover_server_lost_agents` revisiting Dead+unbound | deferred, follow-up |

No changes to `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifests, or
quality-gate configuration are planned.

## 6. Review counters

| Phase | Cap | Used |
|---|---|---|
| Local OCR before PR | 2 | 0 |
| OCR after PR opened | 2 | 0 |

## 7. Verification evidence

To be recorded on the candidate head: `make ci-check` (fmt, clippy `-D warnings`,
build, test, coverage `--fail-under-lines 30`) plus required CI including native
Windows.

## 8. Deferred findings

Recorded in the scope ledger above; follow-up issues to be filed rather than folded in.
