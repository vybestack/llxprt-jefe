# Issue #263 plan: native Windows SSH transport for remote Linux agents

## Goal

Preserve controlled remote Unix command semantics while making every local SSH
process invocation explicit, shell-free, Windows-safe, typed, and diagnosable.

## RED

1. Add the repository-form TUI scenario first and prove SSH port, identity file,
   and option fields are absent.
2. Add domain/persistence tests for port, Unicode/space identity paths, and
   validated SSH options with backward-compatible defaults.
3. Add transport planning tests proving the resolved OpenSSH executable and each
   host/port/identity/option/remote-command value remain distinct argv entries.
4. Add failure-classification tests for missing executable, host-key failure,
   authentication failure, timeout, cancellation, missing remote tmux/runtime,
   and generic remote-command failure without exposing commands or credentials.
5. Add attach planning tests proving Windows remote attachment never invokes a
   local Unix shell and local psmux namespaces never enter remote argv.
6. Add a guarded disposable real-SSH test configured entirely by environment;
   skip with a diagnostic when no fixture is configured and clean only the
   run-owned remote session/path.

## GREEN

1. Extend `RemoteRepositorySettings` and repository forms with typed port,
   identity-file path, and validated options while preserving legacy state.
2. Extend `local_command` with explicit Windows OpenSSH resolution and an
   optional `JEFE_SSH_BIN` override.
3. Introduce one shared SSH transport planner/executor used by runtime,
   availability probes, prompt transfer, and remote repository preparation.
4. Keep remote Linux scripts as controlled single remote-command arguments;
   remove local `sh -lc` from remote attachment by spawning OpenSSH directly
   through `portable-pty`.
5. Return typed, actionable, redacted transport errors and retain bounded process
   teardown.

## REFACTOR and review

- Remove duplicated SSH argv construction and direct `Command::new("ssh")`.
- Audit LLxprt/Code Puppy discovery, preparation, launch, liveness, attach,
  detach, kill, and cleanup through the shared boundary.
- Confirm Unix behavior, quoting, effective-user wrapping, and upstream tmux
  semantics are unchanged.
- Run focused tests, the guarded fixture when configured, and all CI gates.
