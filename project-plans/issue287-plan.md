# Issue #287 Implementation Plan: Blocking liveness polling starves input processing

## Problem

`app_shell.rs` lines 149-164 runs a slow-poll liveness future every ~2s. It
calls `jefe::runtime::check_session_alive(&t.session_name)` **synchronously**
on the smol executor for every running agent. Each `check_session_alive` spawns
**two** tmux subprocesses (`has-session` + `list-panes`). With 15 running agents
that is 30 sequential subprocess spawns, each blocking the executor. Under
heavy host load this can stall keyboard processing for tens of seconds.

The current loop:

```rust
let dead_agents: Vec<AgentId> = targets
    .into_iter()
    .filter(|t| !jefe::runtime::check_session_alive(&t.session_name))  // BLOCKING
    .map(|t| t.agent_id)
    .collect();
```

## Solution

### 1. Batched snapshot liveness (replaces per-agent subprocess fan-out)

Add a new pure function in `src/runtime/liveness.rs`:

```rust
/// Parse tmux `list-sessions -F '#{session_name}'` output into a set of
/// live session names.
pub fn parse_alive_sessions(raw_output: &str) -> HashSet<String>;
```

Add a side-effecting function (also in `liveness.rs`, which is only 203 lines):

```rust
/// Query the tmux server once for all alive sessions, returning the set of
/// session names that exist and have at least one non-dead pane.
///
/// Uses a SINGLE `tmux list-sessions` subprocess instead of 2N subprocesses.
pub fn alive_session_set() -> HashSet<String>;
```

`alive_session_set()` runs `tmux list-sessions -F '#{session_name}'` (one
subprocess) and returns the set. For dead-pane detection we need the pane
status too, so we use `tmux list-sessions -F '#{session_name}:#{session_alive}'`
or check `list-panes` per session — BUT that re-introduces N calls.

**Better approach**: Use `tmux list-sessions -F '#{session_name}'` to get the
set of existing sessions (1 subprocess). Then check dead panes using a single
`tmux list-panes -a -F '#{session_name}:#{pane_dead}'` (1 subprocess for ALL
sessions at once). This gives us session existence + pane liveness in exactly
2 subprocesses total, regardless of agent count.

Parse function (pure, testable):

```rust
/// Reconcile which target sessions are alive from a set of existing session
/// names and pane-dead status lines (format: "session_name:0" or "session_name:1").
///
/// A session is alive if it exists AND has at least one non-dead pane.
pub fn reconcile_alive(
    targets: &[LivenessCheck],
    existing_sessions: &HashSet<String>,
    pane_dead_lines: &[String],
) -> Vec<AgentId>;  // returns dead agent_ids
```

### 2. Move liveness polling off the executor via `smol::unblock`

In `app_shell.rs`, wrap the batch liveness check in `smol::unblock` (same
pattern already used at lines 216 and 251 for attach/persist):

```rust
let dead_agents = smol::unblock(move || {
    let alive = jefe::runtime::alive_session_set();
    // reconcile targets against alive set
    // ...
}).await;
```

This moves the blocking tmux subprocess calls to a background OS thread, so
the smol executor remains free to process keyboard/input events.

### 3. Coalesce overlapping poll cycles

Add an `AtomicBool` or `Mutex<bool>` guard so a new poll cycle cannot start
while a previous one is still running. This prevents unbounded backlog.

Since the future is a single loop on one executor, the simplest guard is a
local `bool` flag: set `polling = true` before `smol::unblock`, set `polling =
false` after. If `polling` is true when the timer fires, skip that cycle.

### 4. Stale-result protection

The batch result is applied to app_state only if the agent is still running
(same guard as current code). No additional timestamp needed because the poll
loop is the sole writer of AgentStatusChanged → Dead events.

### 5. TUI harness scenario

Add `dev-docs/tmux-scenarios/liveness-responsiveness.json` — a scenario that
launches jefe, sends arrow keys, and verifies responsiveness. This is a manual
scenario (not CI-gated) because it requires configured state.

### 6. Behavioral tests

**Test 1** (pure, in `liveness.rs`): `reconcile_alive` correctly identifies
dead agents from session sets and pane-dead lines.

**Test 2** (pure, in `liveness.rs`): `parse_alive_sessions` correctly parses
tmux list-sessions output.

**Test 3** (integration, `tests/runtime/`): With a stub runtime that has a
delayed liveness check, input events are still processed promptly. This tests
that `smol::unblock` keeps the executor free.

**Test 4** (unit, `liveness.rs`): `alive_session_set` returns empty on a
nonexistent tmux server.

## File changes

| File | Change | Size impact |
|------|--------|------------|
| `src/runtime/liveness.rs` | Add `parse_alive_sessions`, `alive_session_set`, `reconcile_alive` + tests | +~150 lines (203 → ~350) |
| `src/runtime/mod.rs` | Re-export new functions | +3 lines |
| `src/app_shell.rs` | Replace per-agent sync loop with batch + `smol::unblock` + coalesce guard | Net -15 lines (998 → ~983) |
| `tests/runtime/liveness_batch_tests.rs` | New test file for batch liveness | New ~100 lines |
| `dev-docs/tmux-scenarios/liveness-responsiveness.json` | New manual scenario | New ~20 lines |

## Constraints

- `app_shell.rs` is 998 lines (HARD LIMIT 1000) — must NET REDUCE or stay same
- `liveness.rs` is 203 lines — has room for ~150 more
- No `unwrap`/`expect` in production paths
- No `unsafe`
- TDD: write failing tests first
- Follow existing `smol::unblock` pattern from app_shell.rs lines 216/251
- Follow existing pure-function extraction pattern from `pane_capture.rs`

## Acceptance criteria mapping

- "Input processing does not wait for per-agent tmux subprocesses" → `smol::unblock`
- "Liveness polling does not execute two sequential subprocesses per running agent" → batch (2 total)
- "Poll cycles cannot overlap" → coalesce guard
- "Responsiveness does not degrade linearly" → O(1) subprocess calls regardless of agent count
- "Automated coverage models delayed liveness responses" → stub runtime test
- "Stale liveness result cannot overwrite newer runtime state" → agent-still-running guard