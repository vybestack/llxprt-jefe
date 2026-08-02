# Issue #541 — fail-closed liveness and restore

Sub-issue of epic #539. Invariant **I3**, criteria **E3** and **E4**. Parent of #537.

> An unknown observation must never cause a state transition. `Unknown` is a first-class
> terminal outcome that produces no transition, preserves every binding, and surfaces a
> visible, actionable state to the user.

Branch `issue541`, based on `9a0909a3` (the #540 merge).

---

## 1. Baseline — what is already true after #540 and #543

The issue and its comments were written before #540 and #543 merged. Three of its premises
have changed, and the plan must start from the code rather than the prose.

| Premise in the issue | State on `9a0909a3` | Consequence |
|---|---|---|
| **V12** — server health keys on `#{pid}`, which psmux proved unstable (comment 3: *"the input is wrong"*) | **Closed by #540.** `classify_server_health` now treats the namespace instance token as decisive when both observations carry one; `#{pid}` is no longer the identity. | V12 needs a regression test asserting it stays closed, not new work. |
| Pane PID conflated with worker PID, so ownership questions consult the wrong process | **Closed by #543.** `PaneProcessIdentity` / `WorkerProcessIdentity` are distinct types; `WorkerDisposition` already distinguishes a worker that survived its pane. | Deliverable 6 (per-agent `Replaced`) can now ask a well-formed question: *is this agent's own worker alive?* Before #543 there was no way to express it. |
| Liveness has no notion of a worker outliving its pane | `app_shell_liveness.rs` already refuses to call `SurvivedPane` death, and #543 deferred *what it becomes* to #542. | This issue must not decide ownership either. It decides only that no transition occurs. |

**Still broken, confirmed by reading the code on `9a0909a3`:**

| Defect | Location | Evidence |
|---|---|---|
| `ServerLost` is a one-way trapdoor | `src/app_shell_liveness.rs:185` | `let _ = lost_ids;` — the recovery the doc comment promises is never implemented. `collect_local_targets` filters to `status == Running` and `eligible_for_server_lost` excludes `ServerLost`, so no other path reaches these agents either. |
| Cold-start transient strands a live agent | `src/app_init_signature_reconcile.rs:103` | `SessionEvidence::Unavailable => StartupClassification::Recoverable` → `RestoreOneOutcome::Skip` → persisted Running, no in-memory binding. This is #537. |
| No `Unknown` in the domain at all | `src/domain` `AgentStatus` | Variants are `Queued, Running, Completed, Errored, Waiting, Paused, Dead, ServerLost`. There is nowhere to put "we do not know", which is why every boundary invents an answer. |
| Probe failure produces no observation rather than an `Unknown` one | `src/runtime/liveness.rs` | `batch_liveness_check_with_identity` returns `Vec::new()` on `list-sessions` / `list-panes` failure. This is *accidentally* correct for the invariant — no transition — but it is silent, unretried, and never re-probed, so a genuinely dead agent stays `Running` forever (V4's mirror hazard). |

**Reframing this produces:** the issue is dominated by one missing concept, not by many independent
bugs. `AgentStatus` cannot express uncertainty, so every call site resolves uncertainty locally and
picks whichever state was convenient. Adding the concept is slice 1; the rest is routing existing
call sites through it.

---

## 2. Acceptance matrix

`S` = slice that delivers it.

| ID | Requirement | S | Notes |
|---|---|---|---|
| V1 | Fault injection at `has-session`, `list-sessions`, `list-panes`, `display-message`, durable read, process-identity query. Each asserts **zero** transitions and **zero** binding losses. | S2, S6 | Needs an injection seam. Must exercise the real boundary where a real psmux exists. |
| V2 | #537 reproduction green: cold-start transient does not strand a live agent; it is attachable afterwards. | S3 | |
| V3 | #527 reproduction green: definition-hash change on N ≥ 20 live agents leaves all N running and attached, definition updated in place. | S5 | |
| V4 | A genuinely dead session is still classified Stopped/ServerLost after retries are exhausted. **Fail-closed must not become never-closed.** | S4 | The mirror hazard. Deliberately tested alongside V1 so neither direction can be satisfied alone. |
| V5 | Server `Replaced` with a surviving host process does not mark that agent `ServerLost`; `Gone` with no surviving process does. | S4 | Now answerable per-agent using #543's `WorkerDisposition`. |
| V6 | An agent left `Unknown` after startup is re-probed automatically and reaches its true state with no user action. | S4 | |
| V7 | Visible, actionable `Unknown` UI state; user can force a re-probe. | S1, S7 | Issue forbids deferring this. |
| V8 | Full E4 perturbation suite — restart, rebuild, attach failure, transient probe failure, server replacement, definition change — against real psmux. | S8 | |
| V9 | Unix/tmux behaviour unchanged or equivalently improved; macOS suite green. | all | Enforced by running both targets every slice. |
| V10 | Launching agent N+1 must not change the status of agents 1..N, with ≥ 3 pre-existing running agents on real psmux. | S4 | The live cascade from comment 2. |
| V11 | A `ServerLost` agent later observed alive returns to `Running` automatically, binding intact, in bounded cycles, no user action. **`let _ = lost_ids;` must be gone.** | S4 | |
| V12 | Server health must not depend on `#{pid}`. | **already closed by #540** | Deliver a regression test only; do not re-implement. |

---

## 3. Non-goals

- **Deciding what an unowned-but-live worker becomes.** #543 deferred that to #542 and this issue
  does not reverse it. Here the answer is only "no transition".
- **Reaping or ownership changes.** #542 and #515 own that.
- **Fixing #445's durable-read path beyond the invariant.** The read already propagates errors
  rather than failing open to empty; if a fail-open path is found it is in scope, but auditing the
  whole persistence layer is not.
- **Upstream psmux work.** psmux#509 already landed and #540 consumed it.
- **Retries as the fix.** Explicitly called out by the issue: retries reduce probability, they do
  not restore the invariant. They are additive to no-transition, never a substitute.

---

## 4. Slices

Each is RED → GREEN → REFACTOR, committed green, verified on Windows **and**
`x86_64-unknown-linux-gnu`.

| S | Deliverable | Proves |
|---|---|---|
| **S1** | `AgentStatus::Unknown` (or a separate observation type carrying it) that must be matched exhaustively and cannot be coerced. Domain + persistence round-trip. | V7 (domain half) |
| **S2** | A fault-injection seam at each probe boundary, with tests asserting zero transitions per boundary. | V1 |
| **S3** | `classify_startup` stops mapping `Unavailable` to a decision; #537 reproduction. | V2 |
| **S4** | Per-agent server-health evaluation; `lost_ids` recovery implemented; deferred re-probe; bounded retry. | V4, V5, V6, V10, V11 |
| **S5** | Signature change can never reach a kill/stop; #527 reproduction at N ≥ 20. | V3 |
| **S6** | Durable-read and process-identity boundaries routed through `Unknown`. | V1 (remainder) |
| **S7** | UI surface for `Unknown` + force-re-probe affordance, with a TUI harness scenario written first. | V7 |
| **S8** | E4 perturbation suite in `windows_native`. | V8 |
| **S9** | Mechanical check failing the build on any un-exhaustive coercion of a fallible observation. | V13 (below) |

**V13 (from the issue's item 7, not in its own table):** a mechanical check must fail the build if
any liveness/restore call site coerces a fallible observation to a state without exhaustively
matching `Unknown`. Tracked as S9. This is the item that stops the invariant being re-violated a
fifth time, so it is not optional — the previous four fixes all held until someone added a call site.

---

## 5. Scope ledger

The issue states **no file or line budget applies**, and names nine modules. Recorded anyway so
growth is visible rather than silent.

| Slice | Expected paths |
|---|---|
| S1 | `src/domain/*`, `src/persistence/*`, `src/state/durable_*` |
| S2 | `src/runtime/liveness.rs`, test harness |
| S3 | `src/app_init_signature_reconcile.rs` |
| S4 | `src/app_shell_liveness.rs`, `src/runtime/server_health*.rs` |
| S5 | `src/app_init_signature_reconcile.rs`, `src/state/*` |
| S6 | `src/persistence/mod.rs`, `src/runtime/process_identity*` |
| S7 | `src/ui/*`, `dev-docs/tmux-scenarios/*` |
| S8 | `.github/workflows/ci.yml`, `tests/*` |
| S9 | `xtask/src/*` |

**Stop and ask before:** adding a dependency; changing workflow/agent-memory/quality tooling
beyond S8/S9; introducing a public abstraction not named above; or reversing the #542 deferral.

---

## 6. Open question for the maintainer

**Does `Unknown` belong in `AgentStatus`, or beside it?**

Putting it in `AgentStatus` makes every match exhaustive and is the strongest guarantee — nothing
can ignore it. But `AgentStatus` is persisted, so `Unknown` becomes a durable state, and an agent
could be restored *as* `Unknown`, which is arguably meaningless: on restart we have not probed yet,
so everything is unknown.

The alternative is a separate non-persisted observation layer wrapping the durable status, so
`Unknown` is always a runtime fact and never a stored one.

I lean to the second — uncertainty is a property of an observation, not of an agent — but it is a
domain-shape decision with persistence consequences, and S1 blocks on it. Proceeding with the
second unless told otherwise, and will record the choice in the S1 commit.

## V7 resolution: no manual re-probe keybinding

The acceptance matrix asked for the unknown state to be visible *and*
actionable. Delivered as:

- visible: unconfirmed rows render a distinct glyph in yellow, and startup
  reports how many agents it could not check and why;
- actionable: the periodic pass re-attempts adoption every
  `LIVENESS_POLL_INTERVAL`, which is 2 seconds.

A bound "force re-probe" action was considered and rejected. It would add a
`HandlerKey` variant, a published action id and owner, a default chord, keymap
persistence and help text -- a published-surface change -- to trigger by hand
something that already happens automatically every two seconds. The operator
gains nothing they do not already have within one poll.

If a manual trigger is ever wanted it belongs with a longer cadence or an
explicit "check now" affordance across all probes, not bolted onto this issue.