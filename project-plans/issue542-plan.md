# Issue #542 — A single owner-lifetime anchor for Windows session process trees

Invariant **I4**, epic criterion **E5**. Parent of #515, #306, #445.

> A Windows session process tree has exactly one owner-lifetime anchor. No
> process in the tree can outlive its owner, and no owner can die leaving a
> reachable-but-unowned tree.

## 1. Why the three previous closures did not hold

| Prior fix | What it moved | What it left open |
|---|---|---|
| #332 (`ca7e4bd4`) | Added orphan recognition and reaping; anchor stayed the dashboard image | Reaping is after-the-fact, not ownership |
| #467 (`eaba9f2a`) | Anchor moved from dashboard to a staged, content-addressed `jefe-session-host.exe` with a `KILL_ON_JOB_CLOSE` Job | Containment is *host*-lifetime, not *owner*-lifetime — the host contains its worker, nothing contains the host. This **created #515** |
| #493 (`c51bd65d`) | Added the `ServerLost` tri-state for a vanished psmux server | Explicitly does not reap surviving trees |

The structural cause: containment was established at exactly one link of
`psmux server -> pane process (pwsh) -> jefe-session-host.exe -> worker`, never
along the whole chain. There was no single answer to "who owns this worker, and
what happens when that owner dies?"

## 2. The ownership model this issue establishes

Named, documented, and merged as
`dev-docs/standards/windows-session-ownership.md` (V8). Summary:

```text
psmux server ──owns──> pane process (pwsh) ──owns──> session host ──owns──> worker
      ^                        ^                          ^                    ^
   anchor L2               anchor L1              anchor holder          contained
```

- The session host is the **anchor holder**. It holds the kill-on-close Job that
  contains the worker.
- Its **owner chain** is exactly its parent (the pane process) and its
  grandparent (the psmux server) — captured as `ProcessIdentity` (PID plus
  creation time) **before** the worker is spawned. Capture is capped at depth 2
  so the Jefe dashboard is never an anchor; that is what keeps #467's
  dashboard-exit and rebuild-survival guarantees intact.
- Any **confirmed** death or PID-reuse of any captured ancestor is an ownership
  violation: the host exits, closing the Job, and the kernel reaps the worker.
- Uncertainty is never a death sentence. `Inaccessible` / `ProbeFailure`
  observations fail open.

## 3. Acceptance matrix

| ID | Actor / launch path | Input & boundary | Observable success | Observable failure & diagnostic | Side effects before failure | Persistence / compat | Behavioral proof |
|---|---|---|---|---|---|---|---|
| A1 | Session host, Windows, pre-spawn | `psmux -> pwsh -> host` | Captures parent + grandparent `ProcessIdentity` before spawning the worker | Unresolvable chain returns typed `OwnerUnavailable`; no worker spawned | Launch plan consumed; nothing spawned | None | `owner_anchor` unit tests + `run_launch_plan` refusal test |
| A2 | Owner watchdog | Every captured ancestor alive | Host and worker untouched; no status or process mutation | n/a | none | none | Pure decision tests |
| A3 | Owner watchdog | Any captured ancestor confirmed `Dead` | Host exits; kill-on-close Job reaps the worker within a bounded interval | Cleanup failure is diagnostic; cannot panic the dashboard | none | none | Native isolated psmux regression (V2) |
| A4 | Owner watchdog | Ancestor PID reused with a different creation identity | Treated as owner death; ownership never re-attached to the impostor | n/a | none | none | PID-reuse unit test (V5) |
| A5 | Owner watchdog | Probe `Inaccessible` / `ProbeFailure` | Fails open — agent stays alive; bounded diagnostic only | Uncertainty is never converted into termination | none | none | Pure classifier tests |
| A6 | Dashboard exits, psmux healthy | — | Host and worker survive and stay reconnectable | n/a | none | none | #467 dashboard-exit regression stays green |
| A7 | Jefe binary rebuilt mid-run, psmux healthy | — | Running staged host and worker survive; new build is not file-locked | n/a | none | none | #467 rebuild regression + A8 |
| A8 | Staging, rebuild changes the digest | `<root>/<session>/<digest>/` | Superseded digest directories for that session are pruned at stage time; the in-use one is retained because Windows locks a running image | Prune failure is silent retention, never a launch failure | new digest staged | staged tree only | `session_host` prune tests (V6) |
| A9 | Kill matrix | Kill each of dashboard / psmux server / pane / host / worker, graceful and `taskkill /F` | Resulting tree exactly matches the documented expected tree; no survivors outside the model | n/a | none | none | Native kill-matrix regression (V1) |
| A10 | Abrupt jefe termination then restart | Kill bypassing `Drop` | Every tree resolves to a defined startup state; live agents re-bound, dead ones reaped | Classification is total — no evidence combination is undefined | none | durable state unchanged | Startup-classification totality test (V3) |
| A11 | Restart / deliberately failed attach | — | Ownership preserved; binding re-established against the same tree by identity, not PID alone | Attach failure never marks a live agent dead | none | binding preserved | #306 contracts stay green (V4) |
| A12 | CI, `windows_native` | After the suite | Asserts **zero** surviving processes in the jefe namespace and fails the build if any remain | Step fails the job rather than recording and exiting 0 | none | none | Workflow contract test (V7) |
| A13 | Unix / tmux / remote | — | Behavior structurally unchanged; Windows-only code not compiled | n/a | none | none | Cross-platform clippy/build/test |

## 4. Non-goals

- A dashboard-wide process supervisor, always-on cleanup daemon, or periodic
  sweeper as the **primary** mechanism. The invariant must hold without one and
  the tests must prove that.
- Killing agents from an uncertain `Unavailable` psmux probe.
- Automatic relaunch after server loss; `ServerLost` UX and its recovery
  confirmation are unchanged.
- Persisting process anchors in the durable state schema.
- Heuristic cleanup keyed on executable name, command line, cwd, or PID alone.
- Terminating the historical pre-watchdog orphans described in #515.
- Unix/tmux or remote-agent lifecycle changes.
- Re-opening #306's reducer/attachment policy: it merged in `88a3fd2d`
  (PR #616) and is carried here as a regression guard only.

## 5. Vertical slices

| Slice | Acceptance rows | Owner layer | Allowed paths | RED proof |
|---|---|---|---|---|
| S1 Ownership model document | A-all, V8 | docs | `dev-docs/standards/windows-session-ownership.md`, `tests/core/windows_ownership_model_contracts.rs`, `tests/core/mod.rs` | Contract test asserts doc exists, names every tree role, and that code references it |
| S2 Owner anchor: pure classifier + chain capture | A1, A2, A4, A5 | runtime (pure + thin platform seam) | `src/runtime/owner_anchor.rs`, `src/runtime/owner_anchor_tests.rs`, `src/runtime/mod.rs` | Classifier and PID-reuse tests fail before the module exists |
| S3 Enforce the anchor in the session host | A1, A3, A6, A7, A13 | runtime boundary | `src/runtime/agent_launcher.rs`, `src/runtime/mod.rs` | `run_launch_plan` refuses an unowned spawn; native owner-death regression |
| S4 Staged-host lifecycle across rebuild | A8 | runtime boundary | `src/runtime/session_host.rs`, `src/runtime/session_host_tests.rs` | Prune test fails: superseded digests accumulate |
| S5 Kill matrix + abrupt-teardown totality | A9, A10, A11 | tests | `tests/psmux_owner_lifetime.rs`, `src/app_init_tests.rs` | Kill matrix and totality assertions |
| S6 CI zero-survivor gate | A12 | CI (issue-authorized) | `.github/workflows/ci.yml`, `tests/core/windows_ci_signal_contracts.rs` | Contract test asserts the step throws instead of `exit 0` |

## 6. Scope ledger

| Change | Authorization |
|---|---|
| `.github/workflows/ci.yml` | Explicitly required by V7 in the issue body |
| `dev-docs/standards/windows-session-ownership.md` (new) | Explicitly required by V8 |
| New module `src/runtime/owner_anchor.rs` | Required by deliverable 2; the issue names the ownership model as spanning these modules and states no file/line budget applies |

No dependency, agent-memory, or quality-tooling change. `.llxprt/` and
`.code_puppy/` untouched.

## 7. Review counters

- OCR before PR: 0 / 2
- OCR after PR: 0 / 2

## 8. Verification evidence

| Slice | Command | Result |
|---|---|---|
| S1 | `cargo test --test integration windows_ownership_model` | RED 5 failed → GREEN 5 passed |
| S2/S3 | `cargo test --lib runtime::` | GREEN 380 passed |
| S4 | `cargo test --lib runtime::session_host` | RED (superseded generations accumulate) → GREEN |
| S6 | `cargo test --test integration windows_ci_signal` | RED 1 failed → GREEN 10 passed |
| S5 | `JEFE_REQUIRE_PSMUX=1 cargo test --features psmux-smoke --test psmux_session_host` | **RED 1 failed → GREEN 2 passed** (see §9) |
| gate | `cargo fmt --all --check` | exit 0 |
| gate | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| gate | `cargo build --workspace --all-features --locked` | exit 0 |
| gate | `cargo test --workspace --all-features --locked --no-fail-fast` | exit 0 — 81 binaries, 5286 passed, 0 failed |

## 9. How V1/V2 were actually proven

The first version of `killing_the_owning_psmux_server_reaps_the_whole_owned_tree`
was **not a regression guard**: it stayed green with `spawn_owner_watchdog`
deleted from the fixture. Measured against real psmux 3.3.7 with the watchdog
disabled, killing the psmux server reaped the tree in 1.64 s and killing the
pane `pwsh` reaped it in 4.23 s. The pane's ConPTY teardown was destroying the
console-attached session host, so the test measured Windows, not Jefe. Shipping
that as V1/V2 evidence would have repeated the exact failure mode (#332, #467,
#493) this issue exists to stop.

#515's field evidence has the opposite topology: the psmux server was gone while
the pane `pwsh` was **still alive**, holding a surviving host. Those hosts were
never console-cascaded.

Fix: the `--pane-launcher` fixture mode spawns the session host with
`DETACHED_PROCESS`, using the safe `CommandExt::creation_flags` path the worker
already used. The host now has real ancestors in the psmux tree but no shared
console, matching #515. `unsafe_code = "forbid"` is preserved — no FFI was
added, and no production code changed for the sake of the test.

Discrimination proof:

- **RED** — with `spawn_owner_watchdog(anchor)` replaced by `let _ = &anchor;`,
  the test fails at `tests/psmux_session_host.rs:393`: "session host (pid 18856)
  survived the death of the process that owned it" (21.85 s).
- **GREEN** — with the watchdog restored, 2 passed in 3.25 s.

The test kills only `owner_links[0].pid`, a process the test itself spawned
inside its own uniquely-named psmux namespace, with `taskkill /F` and no `/T`,
so nothing cascades and no ambient psmux server is touched.

## 10. Deferred findings

None.
