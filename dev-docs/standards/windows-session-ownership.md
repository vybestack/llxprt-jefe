# Windows Session Ownership Model

Status: normative. Issue #542 (invariant **I4**, epic criterion **E5**).
Supersedes the implicit, undocumented models assumed by #332, #467 and #493.

> **Invariant I4.** A Windows session process tree has exactly one
> owner-lifetime anchor. No process in the tree can outlive its owner, and no
> owner can die leaving a reachable-but-unowned tree.

This document is the specification. `src/runtime/owner_anchor.rs`,
`src/runtime/agent_launcher.rs`, `src/runtime/job_object.rs` and
`src/runtime/session_host.rs` implement it and reference it by name.

## 1. Why this document exists

The same defect class has been closed three times and reopened four. Every
previous fix moved the ownership anchor without writing down where it now was,
so the next change moved it again:

| Fix | Anchor after the fix | Gap it left |
|---|---|---|
| #332 `ca7e4bd4` | dashboard image, plus after-the-fact orphan reaping | reaping is a mitigation, not ownership |
| #467 `eaba9f2a` | staged `jefe-session-host.exe` owning a `KILL_ON_JOB_CLOSE` Job | containment is *host*-lifetime, not *owner*-lifetime — the host contains its worker, nothing contained the host. Created #515 |
| #493 `c51bd65d` | unchanged; added the `ServerLost` tri-state | explicitly does not reap surviving trees |

The tree is `psmux server -> pane process -> session host -> worker`, and
containment had only ever been established at one link at a time.

## 2. The tree

```text
  jefe dashboard                     (NOT an owner — see §4)
        │ starts, then forgets
        ▼
  psmux server            ── L2 owner ──┐
        │ spawns pane                   │
        ▼                               │  owner chain of
  pane process (pwsh)     ── L1 owner ──┤  the session host
        │ spawns launcher               │
        ▼                               │
  session host            ◀── ANCHOR HOLDER
        │ owns a KILL_ON_JOB_CLOSE Job
        ▼
  worker (bun / node / agent)  ── contained
```

## 3. Roles, owners, and death semantics

| Process | Owned by | Its death means | What must be reaped |
|---|---|---|---|
| **jefe dashboard** | the user | the UI is gone; agents keep running and stay reconnectable | nothing |
| **psmux server** | the user / the multiplexer namespace | every session it hosted is gone | every pane process, session host and worker below it |
| **pane process** (`pwsh`) | the psmux server | that one pane is gone | the session host and worker below it |
| **session host** | the pane process, transitively the psmux server | the Job handle closes | the worker tree, by the kernel, unconditionally |
| **worker** | the session host, via the Job | the agent finished or died | nothing; the host exits with it |

Reading the table downward gives the containment guarantee; reading it upward
gives the anchor rule: **the session host must not outlive any process above it
in its owner chain.**

## 4. The anchor rule

The **session host is the anchor holder**. It is the only process that owns a
kernel object (`KILL_ON_JOB_CLOSE` Job) whose closure reaps the tree.

Its **owner chain** is exactly two links, captured at launch:

- **L1 — the pane process**: the session host's direct parent.
- **L2 — the psmux server**: the session host's grandparent.

Capture is deliberately capped at depth 2. Climbing further would reach the
Jefe dashboard, and anchoring on the dashboard is exactly the #332 model that
#467 had to undo: it makes a dashboard quit or a rebuild kill live agents.
Capping at L2 is what keeps #467's acceptance criteria (dashboard exit and
mid-run rebuild both leave the tree alive and reconnectable) true.

### Rules

1. **Capture happens before the spawn.** The owner chain is captured *before*
   the worker process is created. A late lookup cannot distinguish "my owner
   is alive" from "my owner already died and something else holds its PID".
2. **The anchor is a `ProcessIdentity`, never a PID.** `ProcessIdentity` is
   PID plus process creation time. **PID reuse** must not be able to spoof
   ownership: an ancestor whose PID still resolves but whose creation time
   differs is a different process, and is treated as owner death.
3. **An ancestor cannot be younger than its descendant.** At capture time, a
   candidate ancestor whose creation time is later than the session host's own
   is rejected — it is a recycled PID, not a real ancestor.
4. **Owner death releases the tree.** When *any* captured link is confirmed
   dead or replaced, the session host exits. Exiting closes the Job handle and
   the kernel terminates the contained worker tree. Because every process above
   the host must outlive it, any confirmed ancestor death is an ownership
   violation, whichever link it happens at and whatever the termination order.
5. **Uncertainty must never terminate.** Termination is irreversible; a
   transient probe failure is not. `Inaccessible` and `ProbeFailure`
   observations **fail open**: the host keeps running and emits a bounded
   diagnostic. Only a positive observation of death or replacement releases the
   tree.
6. **If ownership cannot be established, nothing is spawned.** A launch that
   cannot capture an owner chain fails with a typed error instead of creating a
   worker that nothing owns.

## 5. What this model is not

- **It is not a sweeper.** A periodic janitor that reaps orphans after the fact
  is defence in depth, not an ownership model. Invariant I4 must hold without
  one, and the tests prove it does. Startup reconciliation
  (`app_init_orphan_reconcile`, `startup_cleanup_session_hosts`) exists to
  resolve trees left by *pre-model* builds and by terminations that bypassed
  every user-space path; it is not the mechanism.
- **It is not liveness reporting.** #493's `ServerLost` tri-state describes
  what the dashboard *displays* when a psmux server vanishes. It deliberately
  does not reap. This model reaps, from inside the tree, without the dashboard
  being involved or even running.
- **It is not PID-based cleanup.** No decision in this model is taken on an
  executable name, command line, working directory, or bare PID.

## 6. Staged host lifecycle across rebuild

`session_host.rs` stages a content-addressed host per session at
`<root>/<session>/<sha256>/jefe-session-host.exe`. A rebuild changes the hash,
so the next launch stages a new digest directory while the running host keeps
its own image file-locked.

Ownership status of a superseded digest directory: it belongs to a *previous
host generation*. It is removable exactly when no process holds its image open.
Windows enforces this for free — `remove_dir_all` fails on a locked image — so
staging prunes superseded digests for that session on a best-effort basis and
silently retains anything still in use. Pruning never fails a launch.

## 7. Termination-order matrix

The expected surviving tree after each termination, graceful or `taskkill /F`:

| Terminated | Expected survivors |
|---|---|
| jefe dashboard | psmux server, pane, session host, worker — all alive and reconnectable |
| worker | nothing below it; the session host exits with its child |
| session host | worker reaped by the kernel via the Job |
| pane process | session host observes L1 loss, exits, kernel reaps the worker |
| psmux server | every pane, session host and worker under that server exits; other servers' trees are untouched |

No survivors outside this table, in any order, at any level of abruptness.
