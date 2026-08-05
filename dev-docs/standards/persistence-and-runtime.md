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
- The Settings draft is not persisted. The draft, its edits, its theme preview,
  its scheduled save revision, and the screen's selection all belong to the
  session looking at them. Persisting any of them would let a restart resurrect
  unsaved work over a file that has moved on.
- No background task scheduler state, no network server state.

---

## Editing the Settings Document

The settings document is edited losslessly. There is one parser
(`persistence::settings_document`), one patcher
(`settings_document::patch_assignment`), and one writer
(`persistence::writer::write`); the keymap editor and the Settings shell both go
through all three rather than owning a copy of any of them.

### Draft identity

A draft is bound to the exact bytes it was taken from, not to a copy of the
values in them. It holds:

- those bytes and their SHA-256, which becomes the writer's expected hash — an
  absent settings file binds to `ExpectedHash::Absent`, and its first save is
  the file's creation;
- the document revision it was read at;
- the exact syntax paths that were edited, which is a closed set of host-owned
  leaves rather than an open path space;
- the complete validated candidate, or the sorted diagnostics that block it.

A candidate that publishes is not yet a candidate the session could start from.
The registries composed out of it have rules of their own, so after every edit
the candidate is offered to the owners that compose it — the action/key resolver
and the descriptor/layout validator — and whatever they refuse is stored as
diagnostics. Those owners remain the only validators; the draft records their
answers and never forms its own.

A refused candidate is not a candidate that could not be built. Its values stay
on screen and Save stays blocked, because a screen that fell back to the file on
disk would report a conflict while showing syntax that does not conflict. Only a
document that cannot be read at all falls back to the base.

Editing mutates the candidate only. The published settings, the theme manager,
the composed keymap, and the screen registry are all unchanged until a save
succeeds.

### The editable leaves

A leaf carries the identity it names. There is no open path space and no
generic-map payload: an agent, a screen, or an action/context pair is decided at
runtime by what the registries hold, and those identity types have already
proved their own grammar, so an ill-formed path stays unrepresentable.

| Leaf | Written syntax | Removed by Reset |
|---|---|---|
| `appearance.theme` | the theme slug as a string | the assignment |
| `appearance.override_agent_theme` | `true` or `false` | the assignment |
| `workbench.initial_screen` | the screen id as a string | the assignment |
| `workbench.enabled_screens` | a replacement array of screen ids | the assignment |
| `workbench.screen_order` | a replacement array of screen ids | the assignment |
| `agents.<id>.enabled` | `true` or `false` | the assignment |
| `workbench.layout_overrides.<id>` | the whole layout tree as one inline value | the whole override |
| `keymap.<context>.<action>` | a replacement array of canonical chords | the assignment |

- An identity containing a `.` is written as a quoted key, so `core.llxprt` is
  one owner rather than an owner named `llxprt` inside a table named `core`.
- Membership and order are rewritten together from the projected rows, so every
  enabled screen appears exactly once and no disabled screen appears at all —
  because of how the arrays are built, not because something checks them after.
- Unbinding an action writes `[]`. That is a different statement from removing
  the assignment: an empty list says "this action has no chord", and a removed
  assignment says "inherit the compiled chords".
- A layout override is one whole tree, replaced or removed as a unit. It is
  written in the same grammar a screen definition file declares its layout in,
  so one shape of layout syntax is readable by anyone who has read either file.
  Whatever spelling the file used before — an inline value or its own `[table]`
  block — the replacement is written as one inline value, which is the one part
  of the document an override edit reshapes.
- Every leaf except the two appearance leaves is structural: it composes a
  registry that is built once, at startup, so a saved change applies at the next
  start and the running session is untouched.

### What the next start does with each leaf

Nothing here is written for its own sake. Every leaf the editors write is read
by exactly one owner, once, while that owner composes:

| Leaf | Read by | Effect at the next start |
|---|---|---|
| `agents.<id>.enabled` | the agent registry probe boundary | a disabled type is not offered and is not probed |
| `workbench.enabled_screens` | screen composition | a definition whose `local.<member>` identity is absent is dormant |
| `workbench.screen_order` | screen composition | the registry publishes the named screens first, in the order given |
| `workbench.layout_overrides.<id>` | screen composition | that screen's layout is the saved tree instead of the compiled one |
| `keymap.<context>.<action>` | action-registry composition | the action resolves from the saved chords |

- A screen the order does not name keeps the position it already had, so an
  order naming one screen moves that one and nothing else.
- A layout override is validated on its own, against the screen it names, by the
  same descriptor validator every registry publication goes through. An override
  it refuses is a startup **warning** and the compiled layout stands: an invalid
  layout is correctable only from inside the program, and refusing to start
  would leave the user unable to reach the screen that corrects it. The Settings
  Screens editor shows the same refusal against the same row.
- An override naming a screen that is not composed is reported for the same
  reason: settings that do nothing should say so rather than look applied.

### Save

A save requires zero validation errors. It moves the draft to
`Saving { revision }` with a strictly increasing per-session revision and emits
exactly one write of the whole candidate. The writer rereads the target,
compares the expected hash, replaces atomically through a mode-0600 temporary
file, and reports one of four outcomes:

| Outcome | Meaning | What the draft does |
|---|---|---|
| `Written` | the candidate is authoritative | adopts the new bytes, hash, and revision, and returns to clean |
| `Superseded` | a newer revision was scheduled first | returns to dirty; the newer save stands |
| `Conflict` (`CFG-E007`) | the target changed since the draft was bound | keeps the disk and the draft, and offers Reload, Export, Retry |
| `Failed` (`CFG-E104`) | nothing was written | keeps the draft, and offers Retry, Export, Discard |

Only the newest scheduled revision is answerable. A completion naming an older
one is a fact about work that has been superseded, not an instruction, and is
ignored.

### Schema 1

A schema-1 document is read through its in-memory migration view and never
rewritten by reading. An explicit save is the one moment it becomes schema 2:
the candidate is built from the migration's own schema-2 rendering, which
carries every unknown root assignment and table into `[extensions.schema1.*]`
rather than dropping it.

### Theme preview ownership

The preview token is a value the draft holds, not a mode the theme manager is
in. It names the theme being shown *and* the theme it replaced, so replacing a
preview keeps the theme the first one replaced, and reverting does not depend on
the manager having remembered anything. It is bound to the draft's generation,
so a token issued for a draft that has since been reloaded or discarded cannot
repaint the session. The boundary's only job is to make the manager wear the
theme the state names.

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
