# Issue #323 — Restart marks running agents Dead and clears bindings before restore can reattach them

## Problem

When jefe restarts, `init_app_state()` → `reconcile_running_agents()` runs
before `restore_runtime_sessions()`. The reconcile pass calls
`classify_agent_startup()`, which short-circuits to `Inconsistent` whenever
`binding_evidence()` returns `BindingEvidence::Inconsistent` — even when the
tmux session is provably `Alive`. Agents classified `Inconsistent` are added to
`dead_ids`, and `apply_dead_reconciliations()` marks them `Dead` AND clears
`runtime_binding = None`. When `restore_runtime_sessions()` runs next, it only
processes `Running` agents, so these agents — with their live tmux sessions —
are permanently lost from the UI.

## Acceptance Matrix

| # | Actor / Launch Path | Input / Boundary | Expected Behavior | Failure Behavior |
|---|---|---|---|---|
| AC1 | `classify_startup` | `session=Alive`, `binding=Inconsistent`, local, `process=Alive` | Returns `Running` (session is live, treat as reattachable) | — |
| AC2 | `classify_startup` | `session=Alive`, `binding=Inconsistent`, local, `process=Dead` | Returns `Running` (session is alive, process state is secondary) | — |
| AC3 | `classify_startup` | `session=Alive`, `binding=Inconsistent`, local, `process=ReusedPid` | Returns `Running` | — |
| AC4 | `classify_startup` | `session=Missing`, `binding=Inconsistent`, local | Returns `Inconsistent` (no session to rescue; existing behavior preserved) | — |
| AC5 | `classify_startup` | `session=Missing`, `binding=Inconsistent`, remote | Returns `Inconsistent` (no session to rescue; existing behavior preserved) | — |
| AC6 | `classify_startup` | `session=Alive`, `binding=Coherent`, local | Returns `Running` (existing behavior unchanged) | — |
| AC7 | `reconcile_running_agents` | Agent persisted `Running` with `Inconsistent` binding but live tmux session | NOT added to `dead_ids` → survives into `restore_runtime_sessions` | — |
| AC8 | `restore_one_agent` | Agent classified `Running` with `Inconsistent` binding | `Revived` — reattaches to live session and refreshes binding with current signature | — |
| AC9 | `reconcile_running_agents` | Agent persisted `Running` with `Inconsistent` binding, session gone | Added to `dead_ids` → `Dead` (existing behavior preserved) | — |

## Non-Goals

- Changing `binding_evidence()` itself — it still correctly identifies stale bindings.
- Merging reconcile and restore into a single pass (Option B from the issue) — out of scope for this fix.
- Adding automatic binding-refresh during normal (non-restart) operation.
- Remote session restore — remote agents with missing sessions stay `Inconsistent`/`Stopped`.
- Changing persistence format or adding new fields to `RuntimeBinding`.

## Implementation Plan

### Vertical Slice 1: classify_startup respects session liveness over binding inconsistency

**Architecture owner:** `src/app_init.rs` (`classify_startup` function, line 136)

**Allowed files:**
- `src/app_init.rs` — production fix + test updates

**RED:**
- Update existing test `live_session_with_mismatched_binding_is_never_reattached` to assert `Running` instead of `Inconsistent`.
- Add new test: `classify_startup` with `Alive + Inconsistent` across all process liveness states.
- Add test: `classify_startup` with `Missing + Inconsistent` still returns `Inconsistent` (negative case).

**GREEN:**
- In `classify_startup`, check session liveness BEFORE checking binding. When `session == Alive`, return `Running` regardless of binding evidence.
- This means `Inconsistent` binding agents with live sessions survive reconcile AND get reattached/refreshed in restore (because `Running` → `revive_agent_session` → `Revived` with fresh signature).

**REFACTOR:**
- Verify the test `startup_classification_covers_required_lifecycle_states` (line 733) doesn't need updates for the `Missing + Inconsistent` case.

**Verification:** `make quick-check`

## Scope Ledger

| Date | Item | Disposition |
|---|---|---|
| 2026-07-15 | Initial plan | Accepted |

## Review Counters

- OCR pre-PR: 0/2
- OCR post-PR: 0/2

## Verification Evidence

(to be filled during implementation)
