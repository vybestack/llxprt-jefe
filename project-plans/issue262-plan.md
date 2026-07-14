# Issue #262 plan: persistent Windows multi-agent lifecycle parity

## Scope and existing foundation

Issues #259–#261 already provide native psmux process identity, ConPTY attachment,
agent executable resolution, and Windows-safe repository preparation. This issue
closes lifecycle gaps rather than introducing a second runtime architecture.

## RED

1. Create the Windows TUI lifecycle scenario first. It assumes an isolated
   configuration seeded with four deterministic agents (two LLxprt, two Code
   Puppy), verifies switching and selected-agent kill, and initially fails before
   fixture setup/state exists.
2. Add pure startup-reconciliation tests covering explicit Running, Stopped,
   Stale, Recoverable, and Inconsistent classifications from:
   - live, missing, or unavailable psmux evidence;
   - local versus remote runtime;
   - coherent, legacy, or inconsistent persisted binding;
   - alive, exited, inaccessible, reused-PID, malformed, and failed process probes.
3. Add regression tests proving a live session with a mismatched persisted
   session/signature is never reattached and a Windows identity without a
   creation discriminator is malformed rather than accepted as alive.
4. Add persistence tests for Unicode/spaces, both runtime kinds, launch flags,
   process identity round-trip, and legacy identity omission.
5. Add a real psmux integration test with four independently interactive
   recording fixtures in two repositories. Prove scoped kill, attach-client
   teardown, restart-style reattachment, dead-pane detection, and namespace
   isolation. Guard real LLxprt/Code Puppy checks on installation.

## GREEN

1. Introduce explicit deterministic startup evidence/classification domain types
   at the application orchestration seam; keep OS/psmux probing in runtime.
2. Validate persisted runtime bindings against the stable derived session name,
   current launch signature, and internally consistent PID identity before
   reattachment.
3. Map classifications conservatively:
   - Running -> register/reattach;
   - Recoverable -> preserve metadata without destructive action;
   - Stopped/Stale/Inconsistent -> mark Dead and clear stale binding.
4. Preserve remote tmux-only behavior and legacy Unix state where no process
   identity was ever persisted.
5. Complete deterministic fixture/harness setup without shell-only Windows
   assumptions or global psmux cleanup.

## REFACTOR and verification

- Keep state classification pure and injectable; no mock-only lifecycle claims.
- Keep kill/delete scoped to one stable AgentId and Jefe's private namespace.
- Verify relaunch retains runtime kind, model/profile, work directory, flags,
  sandbox, and continue/quick-resume semantics.
- Review Windows reboot, process access denial, probe failure, partial launch,
  malformed state, and concurrent agent behavior.
- Run focused tests, format, strict Clippy, locked build/tests, and all CI gates.
- Independently review the complete diff before commit, then monitor PR checks
  and review threads to green.
