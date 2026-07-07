# Persistence and Runtime Standards

This document defines the versioned file-persistence contract and the runtime
orchestration rules for Jefe. It consolidates sections 7 and 9 of the former
`dev-docs/project-standards.md` and the persistence/runtime detail in
`docs/project-standards.md` and `docs/technical-overview.md`.

Sibling standards:

- [Architecture Standards](./architecture.md)
- [Coding Standards](./coding-standards.md)
- [Testing and Quality](./testing-and-quality.md)
- [Display and UI](./display-and-ui.md)

---

## Persistence Standards (v1)

Jefe v1 persistence is **file-based only**. SQLite and any other database are
out of scope for v1 and must not be introduced, even as a hidden fallback. This
is a deliberate design constraint.

### Persistence files

| File            | Purpose                                              | Format |
|-----------------|------------------------------------------------------|--------|
| `settings.toml` | User preferences not tied to a repository/agent (e.g. active theme slug). | TOML   |
| `state.json`    | The complete set of repository and agent definitions. | JSON   |

### Path resolution order

- `settings.toml`: `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` ->
  platform default.
- `state.json`: `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` -> platform
  default.

The `--config <dir>` (short `-c <dir>`) runtime argument points an instance at
an isolated config directory; both `settings.toml` and `state.json` live
directly under it, and external themes load from `<dir>/themes/`. When supplied,
it takes precedence over the `JEFE_*` path environment variables.

### Standards

- **Versioned schemas.** The persistence layer (`src/persistence/`) carries a
  schema version and surfaces `SchemaVersionMismatch { expected, found }` when
  the on-disk version does not match. Version bumps require explicit migration.
- **Parse/validate before apply.** Reads parse and validate before any state is
  mutated; a malformed file never corrupts in-memory state.
- **Atomic writes.** Writes are atomic so a crash mid-write cannot leave a
  truncated file.
- **Safe fallback on malformed/missing files.** A missing or unparseable file
  fails safely with clear operator feedback (typed `PersistenceError`), not a
  crash. The app can still start.
- **Invalid config directory.** An explicit `--config` directory that cannot be
  used (not a directory, unwritable) is surfaced fail-fast at startup via
  `PersistenceError::InvalidConfigDir` so silent data loss cannot occur
  mid-session.

### What is not persisted

- Agent lifecycle status (Running/Dead/etc.) is re-derived from tmux session
  liveness on startup; the state file stores agent definitions, runtime status
  is ephemeral.
- No background task scheduler state, no network server state.

---

## Runtime Orchestration Standards

The runtime layer (`src/runtime/`, the PTY manager) owns tmux/PTY behavior. The
following rules are binding.

### Agent/session identity

- **Stable agent/session identity mapping.** Each agent maps to one tmux
  session whose name is derived from its `AgentId`:
  `RuntimeSession::session_name_for(agent_id)` produces `jefe-{sanitized_id}`
  (see `src/runtime/session.rs`). Sessions are stored in a
  `HashMap<AgentId, RuntimeSession>` keyed by `AgentId`, not by slot index. The
  mapping is stable across attach/detach cycles.
- A single attached viewer exists at any time. There is no multi-viewer mode.

### Kill and relaunch

- **Agent-scoped kill/relaunch.** `kill_session(idx)` destroys exactly one tmux
  session and tears down the attached viewer if it is current. It never touches
  other agents' sessions.
- **Relaunch respects saved profile/mode.** `relaunch_session(idx)` kills and
  re-creates the tmux session from the agent's stored metadata (work directory,
  profile, mode). If no slot exists, `add_session` creates one and the slot is
  assigned. Relaunch resets error state and re-attaches if the agent is
  current.

### Failure handling

- **Runtime failure must not crash the app process.** `PtyManager` never panics.
  All tmux failures are captured as `Result<(), String>` or logged to stderr.
  tmux fork failures trigger exactly one automatic server reset retry before
  propagating the error.
- **Orchestration diagnostics only.** Jefe provides orchestration diagnostics
  (session liveness, attach/teardown errors). Deep runtime logs belong to
  `llxprt` — jefe does not own or parse child-process internal logs.

### Threading model

- One reader thread per attached viewer, running a blocking `read()` loop on the
  PTY master's reader fd, feeding bytes into the Alacritty terminal model under
  lock.
- The main thread (render path) locks the `Term` briefly to extract snapshots.
  Lock contention is minimal because snapshot extraction is fast.
- Reader thread join uses a 500ms bounded timeout to prevent indefinite hangs on
  viewer teardown.
- `PtyManager` fields use `Mutex` (not `RwLock`); contention is low enough that
  `Mutex` suffices.

### Liveness polling

On every render cycle, the root component checks all agents with
`status == Running`. For each, if the slot is no longer alive (`is_alive(slot)`
returns false), status is set to `Dead`. This check only writes to `AppState`
when changes are detected, avoiding infinite render loops.
