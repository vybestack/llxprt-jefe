# Issue #259 — Native process liveness and runtime identity

## Behavioral goal

Replace Windows command-line liveness and environment-derived namespace identity with typed platform services. Persist a process-instance discriminator so startup reconciliation cannot mistake a reused Windows PID for the original agent. Keep remote SSH/Unix probes unchanged.

## Test-first sequence

1. **RED — classification contracts**
   - Add pure tests for alive, exited, inaccessible, reused PID, malformed persisted identity, and probe failure.
   - Assert probe failure never classifies as alive.
2. **RED — native process transitions**
   - Spawn the Rust test executable as short- and long-lived child processes.
   - Prove running, normal exit, and forced termination transitions through the production process service.
3. **RED — namespace identity**
   - Test deterministic namespace hashing, separation for distinct identities, privacy-safe output, and collision-resistant test suffixes.
4. **GREEN — platform services**
   - Add a runtime process service. Windows uses the safe `winsafe` wrapper around `OpenProcess`, `GetExitCodeProcess`, and `GetProcessTimes`; Unix retains supported local semantics.
   - Add a Windows identity source using the maintained `whoami` crate and hash identity material before constructing psmux namespace names.
5. **GREEN — persistence/reconciliation integration**
   - Add a backward-compatible optional process identity to runtime bindings and sessions.
   - Capture it with pane PID creation and use it during startup reconciliation.
   - Preserve remote tmux/SSH liveness behavior exactly.
6. **REFACTOR/VERIFY**
   - Remove Windows `tasklist` and environment-username identity assumptions.
   - Run focused tests, independent review, and the full CI verification suite.

## Boundaries

- Runtime owns native process and namespace side effects.
- Domain owns serializable process-instance data.
- Persistence remains backward compatible through `#[serde(default)]`.
- No UI behavior changes; no TUI scenario is required.
