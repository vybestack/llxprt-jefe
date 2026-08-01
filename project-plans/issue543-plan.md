# Issue #543 — Separate pane/host process identity from worker process identity

Sub-issue of the Windows runtime remediation epic #539 (invariant I4, criterion E6).

## Root cause

`src/runtime/manager.rs:606` asserts a platform invariant verbatim:

> jefe launches `llxprt` directly (no shell/wrapper in the pane), so the pane PID
> *is* the worker PID.

That is true on Unix and **false on Windows since PR #467**. The real chains are:

| Platform | Chain | `#{pane_pid}` is |
|---|---|---|
| Unix / macOS | `jefe → tmux server → [pane = llxprt worker]` | the worker |
| Windows | `jefe → psmux server → pwsh (pane leader) → jefe-session-host.exe → agent worker` | the **pwsh pane leader**, an ancestor two hops above the worker |

The divergence is established at `multiplexer.rs:197-208`, which dispatches to
`unix_pane_command_args` (direct exec) or `windows_pane_command_args` (a
PowerShell string wrapping `<launcher> --jefe-internal-agent-launch <plan>`).

`manager.rs:634-639` writes that single `#{pane_pid}` value into three fields
whose names all claim to describe the worker:

- `pid` — pane/host PID, named for the worker.
- `process_identity` — pane/host identity, named for the worker.
- `worker_identities` — the one field that is genuinely worker-correct, because
  `orphan::capture_worker_identities` enumerates the *descendant subtree*.

Nothing in the type system prevents the substitution, so the conflation is
invisible at every call site.

## Two defects the issue text does not name

1. **`app_input/agent_runtime.rs:29` hardcodes `worker_identities: Vec::new()`.**
   Every launch/relaunch persistence path discards the only worker-correct
   anchors the runtime captured. Orphan reaping silently loses its evidence
   after the first rebind.
2. **`app_shell_liveness.rs:214` reads pane death as worker death.** The
   background poll consults neither `process_identity` nor `worker_identities`;
   it marks the agent `Dead` and clears the binding on `#{pane_dead}` alone. On
   Windows a pwsh/host crash therefore strands a live worker and drops the
   anchors needed to find it. The startup path layers orphan detection on top;
   the background poll does not.

## The worker identity is not merely mislabelled — it is unknown

`agent_launcher.rs::run_launch_plan` spawns the worker with `command.status()`,
which blocks and **discards the `Child`**. `.id()` is never read. The launch-plan
file is deleted before the spawn. No file, env var, stdout record, or Job Object
handle carries anything back to jefe. The host does not know its own child.

The mechanism to fix this already exists in-repo and is already tested:
`tests/fixtures/psmux_session_host_fixture.rs:107-114` uses `spawn()`, reads
`child.id()`, and writes a `HostMarker { host_pid, worker_pid, host_owned_job,
started_at }` plus a `.worker.json` sidecar. Production never adopted it. Slice 6
promotes that proven fixture mechanism into `run_launch_plan`.

## Acceptance matrix

| # | Criterion | Behavior | Test |
|---|---|---|---|
| V1 | Type-level separation | `PaneProcessIdentity`, `WorkerProcessIdentity`, `ServerProcessIdentity` are mutually non-substitutable; passing one where another is expected is a **compile error**, proven by a `trybuild`-style or doc-test negative case | `identity_roles_are_not_substitutable` |
| V2 | Host dies, worker survives | The disagreement is observable as a first-class classification, not collapsed into `Dead`; the resulting action follows the ownership model | `host_death_with_live_worker_is_not_agent_death` |
| V3 | Worker dies, pane survives | Reported as worker death even though `#{pane_dead}=0` | `worker_death_behind_live_pane_is_detected` |
| V4 | PID reuse cannot spoof ownership | Every identity check compares `started_at`; a recycled PID with a different creation token never validates | `recycled_pid_does_not_match_recorded_anchor` |
| V5 | jefe owns the identity check | Tree teardown is never delegated to psmux's reaper (upstream psmux#447 has the same PID-reuse bug) | audit statement + `reap_orphan_tree` coverage |
| V6 | Repository-wide audit | Every consumer of the three fields classified by the identity it actually needs; recorded in the PR | audit document |
| V7 | Unix is an explicit case | "pane process *is* the worker" is modelled as a stated equality, not an absent distinction | `unix_pane_identity_equals_worker_identity` |
| V8 | Anchors survive rebind | `worker_identities` is no longer cleared by the launch/relaunch persistence path | `relaunch_preserves_worker_anchors` |

## Non-goals

- Defining the ownership model itself (that is #542 / W3). See open decision below.
- Changing kill/teardown semantics. `manager.rs:779` reaps via the host's
  kill-on-close Job Object and reads none of these fields; it is correct by
  containment and stays as-is.
- Remote/SSH sessions. `pane_pid` is local-only and remote keeps the tmux/SSH
  path unchanged.
- Reworking the schema-1 → schema-2 migration. Old files must keep loading;
  recovering identities from schema-1 `evidence` maps is out of bounds.
- UI changes. No render path reads these fields.

## Open decision (raised to the maintainer)

V2 requires the resulting action to be "the one specified by the ownership model
(W3)". W3 is #542, which is scheduled **after** #543 and does not exist yet. The
epic forbids deferring a criterion to a follow-up. Options put to the maintainer:

- **(a)** #543 delivers the *distinction* and makes "worker alive but unowned" a
  first-class tested observation with no state transition; the *action* lands in
  #542, with V2 amended to say so. **Recommended** — deciding the action here
  pre-empts the audit #542 exists to perform.
- **(b)** Pull the minimal ownership decision forward into #543.
- **(c)** Reorder: #542 before #543.

Slices 1–4 and 6–8 are unaffected by this ruling; only slice 5 is gated.

## Vertical slices

Each slice leaves the workspace compiling with non-target tests green.

1. **Newtypes.** `PaneProcessIdentity` / `WorkerProcessIdentity` /
   `ServerProcessIdentity` over the existing `{pid, started_at}` pair, with
   accessors and explicit conversions. Purely additive.
2. **Retype the fields.** `RuntimeSession` / `RuntimeBinding` carry
   `pane_identity: Option<PaneProcessIdentity>` and
   `worker_identity: Option<WorkerProcessIdentity>`; `worker_identities` becomes
   `Vec<WorkerProcessIdentity>`. Serde keeps accepting the legacy `pid` /
   `process_identity` keys, routing them to `pane_identity` — which is what they
   always actually held.
3. **Capture.** `manager.rs:619-639` and `manager_existing.rs` record the pane
   identity from `#{pane_pid}` and resolve the worker identity separately. Unix
   states the equality explicitly (V7).
4. **Consumers.** Retype per the audit inventory; fix the `worker_identities`
   erasure at `agent_runtime.rs:29` (V8).
5. **Pane death ≠ worker death.** Route `app_shell_liveness.rs` through the
   orphan classifier. **Gated on the open decision.**
6. **Host reports its child.** `run_launch_plan` switches `status()` → `spawn()`,
   records the worker identity, writes the marker sidecar; jefe reads it (V2/V3).
7. **Persistence.** Persist both identities with `#[serde(default)]`; schema-1
   and schema-2 files keep loading.
8. **Server identity.** `ServerIdentity.process` becomes `ServerProcessIdentity`.

Plus: delete the false invariant comment at `manager.rs:606` and the matching
claim in `pane_capture.rs:122`.

## Scope ledger

Epic #539 **suspends the file/line scope budget** for this issue and its
siblings; citing a size budget to defer a criterion is a process error. Estimated
blast radius ≈ 18–22 source files, ≈ 900–1,300 net lines, ≈ 6 test files.

| Item | Status |
|---|---|
| `src/domain/mod.rs` (newtypes + `RuntimeBinding` reshape) | planned |
| `src/runtime/session.rs` (fields + constructors) | planned |
| `src/runtime/manager.rs` (capture 619-639, accessors 476-489, comment 606) | planned |
| `src/runtime/manager_existing.rs` (reattach capture + projection) | planned |
| `src/runtime/agent_launcher.rs` (spawn + marker sidecar) | planned |
| `src/runtime/pane_capture.rs` (doc claim at :122) | planned |
| `src/runtime/orphan.rs`, `process.rs`, `liveness.rs` (types/docs) | planned |
| `src/runtime/server_health{,_io}.rs` (server newtype) | planned |
| `src/app_init.rs`, `app_init_signature_reconcile.rs`, `app_init_orphan_reconcile.rs` | planned |
| `src/app_input/agent_runtime.rs` (V8 erasure fix), `relaunch.rs`, `mod.rs`, `modal_handlers.rs` | planned |
| `src/app_shell_liveness.rs` | **gated** on the open decision |
| `src/state/durable_{projection,restore}.rs`, `src/domain/state_contract.rs` | planned |
| Test files encoding the conflated contract | planned |
| Ownership model / action on unowned worker | **deferred to #542** pending ruling |

Review counters: OCR pre-PR 0/2, OCR post-PR 0/2.
