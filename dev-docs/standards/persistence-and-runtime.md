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

### The durable state document

State is stored as a schema-2 document (`domain::StateV2`). It is the single
persistence authority: there is no second in-memory shape that also knows how
to reach disk.

- **One projection each way.** `state::durable_projection::to_durable_state`
  builds the document from `AppState`; `state::durable_restore::from_durable_state`
  restores it. Nothing else translates between runtime and durable form, so the
  two directions cannot drift.
- **Saves are staged, not called.** The reducer stages a single bounded
  `PersistenceEffect::PersistState` carrying the projected candidate, the
  revision it claims, and its correlation. Staging again supersedes the pending
  save by semantic key, so a burst of edits coalesces into one write of the
  latest state rather than a queue of stale ones.
- **`durable_revision` is an acknowledged-write watermark.** It advances only
  when a write is confirmed authoritative. A superseded write is normal
  coalescing rather than a user-facing failure, and a completion whose
  correlation no longer matches changes nothing.

### Agent launch persistence and restore

Schema-2 stores the generic agent type ID and typed values directly. Its
`LaunchSignatureV1` is the canonical definition/value/target digest shared by
projection, migration, planning, runtime binding, and startup reconciliation.
Only fields declared `launch_signature` contribute to the value digest.

Schema-1 product aliases are migrated one way into typed values; unknown legacy
records are retained as dormant raw records rather than guessed. Startup may
register an already-live tmux session without package or agent-executable effects
after the persisted signature matches a freshly projected signature and live
session/process evidence agrees. For local sessions only, a definition-only hash
drift may use a reattach-only registration path when the persisted binding still
matches the prior signature and the signature version, typed values, target, and
stable session identity remain compatible. That path performs a final local
session check, captures the current pane process identity, and either registers
the existing process or fails; it cannot create a session. The binding retains
the prior launch signature as process provenance while the session remains
running. A stale value, target, session binding, confirmed-dead process, or remote
definition drift is non-executable and never reaches runtime registration.

Fresh execution always uses a finalized immutable `AgentLaunchPlan`. Package
selection finalizes wrapper, structural prefix, executable, and physical
fingerprint before planning. Runtime receives the plan plus separately validated
remote transport settings and does not reinterpret agent type identity.
- **Reads never rewrite.** Schema-1 documents are migrated in memory on load;
  the legacy bytes stay authoritative until a save replaces them, and that
  replacement first retains the originals in a content-addressed sibling
  because the one-way migration is otherwise unrecoverable.
- **Identity is stable across migration.** An agent's tmux session name is
  derived from its id, so rewriting ids orphans live sessions: startup liveness
  looks for a session that no longer exists and demotes a healthy running agent
  to dead. Migration therefore preserves any id that is already a valid
  identifier and mints one only when the source id cannot be reused. Because
  schema-1 ids carry no uniqueness guarantee, the first claimant keeps its id
  and later duplicates fall back to the minted, collision-disambiguated form.
- **Legacy writing is test-only.** Production never writes schema 1. The typed
  schema-1 helpers exist behind the `schema1-fixtures` feature so tests can
  author legacy input through the real serialized shape; hand-written JSON
  fixtures silently omit fields and have already hidden a defect that made
  jefe refuse to start on state files it wrote itself.

### What is not persisted

- Agent lifecycle status is not restored as fact. The document records only
  what an agent was *last known* to be doing, which startup reconciles against
  actual tmux liveness. The hint still matters: an agent last known to be
  running is the one startup must check for an orphaned session, so discarding
  it silently strands live sessions.
- Navigation is not persisted. The document records which screen the session was
  last on and nothing else: the navigation stack, screen instances and their
  generations, drafts and their dirty guard, subscriptions, and modal state are
  all runtime-only. A restored session therefore comes back as exactly one clean
  instance on the migrated screen, with an empty stack and no guard — a restored
  stack would point at screens whose data is long gone, and a restored guard
  would ask about a draft that no longer exists.
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

### Startup and PID liveness policy

Process-instance evidence follows one conservative policy across startup
reconciliation and the local PID-only recovery probe:

| Outcome | Startup with no live session | PID-only recovery probe |
|---------|------------------------------|-------------------------|
| `Alive` | keep the agent recoverable/running | alive |
| `Dead` | stop the agent and clear its binding | dead |
| `ReusedPid` | reject the stale binding | not applicable without an expected identity; false if classified |
| `Inaccessible` | keep the agent recoverable | alive (fail open) |
| `MalformedIdentity` | reject inconsistent binding evidence | not applicable without an expected identity; false if classified |
| `ProbeFailure` | keep the agent recoverable | alive (fail open) |

A live multiplexer session remains ground truth during startup even when
persisted binding metadata is inconsistent. Without a live session, invalid
session names, launch signatures, or PID/identity pairings are rejected before
process liveness is considered. For coherent bindings, only a confirmed exit,
PID reuse, or malformed identity rejects the expected process. Permission
denial and probe failure are uncertainty, not proof of death.

A PID-only probe has no persisted creation token to compare, so it can produce
`Alive`, `Dead`, `Inaccessible`, or `ProbeFailure`, but cannot independently
produce `ReusedPid` or `MalformedIdentity`. PID-only and identity-aware startup
both route their final classifications through the same recoverability policy.
Unix probes force the C locale before interpreting `kill -0` diagnostics;
macOS creation tokens come from UTC, C-locale `ps` output; Windows retains its
native creation `FILETIME`.

During restore, PID and `ProcessIdentity` are selected as one observation. Fresh
runtime evidence never borrows a missing field from persisted state, and a
stored identity is only written with its matching PID. Legacy PID-only bindings
remain readable and are probed by PID until a successful runtime refresh adds a
platform creation token. Legacy identities with a missing creation token also
remain compatible: a matching live PID is accepted, and fully tokenized future
observations resume PID-reuse protection.
