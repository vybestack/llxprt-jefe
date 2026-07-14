# Issue #301 Implementation Plan: Complete input responsiveness work

## Issue
GitHub #301 — Complete input responsiveness work left by PR #290.

PR #290 batched liveness but left synchronous persistence, capture, and
attachment on the input/render hot paths. Under production-scale state this
starves keyboard input. The issue also identifies a stale-liveness correctness
defect and a missing responsiveness acceptance test.

## Root causes (verified against current source)

1. **Synchronous fsync per key.** `apply_and_persist` → `persist_state` →
   `FilePersistenceManager::save_state` serializes + `sync_all` + rename on
   every navigation key. Production state ≈114 KB; that fsync dominates.
2. **Synchronous `tmux capture-pane` from render.** `capture_history` shells
   out with blocking `Command::output()` during the render body (app_shell
   render) and from scroll/refresh paths.
3. **Synchronous `runtime.attach` on F12.** `handle_f12_toggle` locks
   `AppContext` and calls `runtime.attach` inline.
4. **Background attach holds the ctx mutex** (`perform_async_attach` locks
   `ctx` for the whole attach duration, blocking PTY input/paste).
5. **Liveness has no stale-result guard.** `batch_liveness_check` returns dead
   agent IDs that are applied unconditionally — a rebound/restarted agent can
   be marked dead by a stale observation.
6. **No responsiveness behavioral test.**
7. **Harness servers not deterministically torn down** on every exit path.

## Architecture

Introduce typed async request/result flows at the runtime boundary so input
and render never spawn subprocesses or fsync. State transitions stay
deterministic; external side effects happen on background workers that
snapshot, release the lock, do work, reacquire, validate identity/generation,
and apply.

### New boundary module: `src/services/`

A new `services` crate module (already referenced in `src/lib.rs`) hosts the
coalescing persistence worker, the capture request/result flow, and the
liveness observation identity. These are pure-domain orchestration types; the
actual I/O stays in `persistence/` and `runtime/`.

## Phases (RED → GREEN → REFACTOR)

### Phase 1 — Coalescing persistence worker

**Design.** A single background future owns a `Mutex<Option<PersistedState>>`
"pending" slot and a generation counter. `schedule_persist(snapshot)` stores
the latest snapshot under the lock (microseconds) and bumps generation. A
worker loop drains the slot via `smol::unblock` and calls
`FilePersistenceManager::save_state`. An `AtomicU64` records the applied
generation so a late write whose snapshot predates the latest schedule is
skipped (newest-wins under reordered completions). Shutdown flushes the final
slot synchronously.

**Files.**
- `src/services/persist_worker.rs` — `PersistWorker`, `PersistRequest`,
  `PersistHandle` (the `Arc` shared with the input path).
- Replace `persist_state(&ctx, &persisted)` call sites with
  `persist_handle.schedule(persisted)` (no I/O on the input path).
- `app_shell.rs` owns one `PersistHandle`, created at startup and threaded
  into `app_input`.

**Tests (RED first).**
1. `persist_worker_orders_newest_wins_under_reordered_completions` — schedule
   N then N+1; arrange for N to complete last; assert durable state == N+1.
2. `persist_worker_coalesces_rapid_schedules` — schedule 100 times rapidly;
   assert at most a bounded number of durable writes (coalescing).
3. `persist_worker_reports_failure_without_blocking` — a failing
   `save_state` surfaces an error future without panicking; subsequent
   schedules still succeed.
4. `persist_worker_shutdown_flushes_final_snapshot` — drop the handle, assert
   the last scheduled snapshot is durable.

### Phase 2 — Async capture request/result flow

**Design.** `RuntimeManager::capture_history` stays as the I/O primitive but
is no longer called from render. Instead a `CaptureWorker` owns a
`Mutex<Option<CaptureRequest>>` where the render path records a request keyed
by `(agent_id, output_generation)`. A background `smol::unblock` drains the
request, calls the existing `capture_pane_history`, and stores the result +
generation in the existing `HistoryCache` (under a short lock). The renderer
reads the cache only (already non-blocking via `get`). Dedup: a new request
with the same `(agent_id, generation)` as the in-flight request is a no-op.
Stale results whose generation predates the current attached session/generation
are not stored.

**Files.**
- `src/services/capture_worker.rs` — `CaptureWorker`, `CaptureRequest`,
  `CaptureHandle`.
- `app_shell.rs` render body calls `capture_worker.request(...)` (cheap) then
  reads the cache; no `capture-pane` subprocess on the render thread.
- `refresh_terminal_scroll_geometry` likewise requests rather than captures.

**Tests (RED first).**
1. `render_completes_from_cache_while_capture_blocked` — block the capture
   worker; assert a render call returns within a bounded budget using cached
   state.
2. `capture_deduplicates_per_generation` — request the same generation twice;
   assert only one capture subprocess.
3. `stale_capture_does_not_overwrite_newer_view` — request gen A; move
   selection to gen B; A completes; assert B's cached view is untouched.
4. `transient_capture_failure_preserves_last_good` — first capture succeeds;
   second fails; cache still serves the first.

### Phase 3 — Async attach without holding AppContext

**Design.** `AttachScheduler` already debounces; `perform_async_attach` is the
problem because it locks `ctx` for the whole `runtime.attach` duration. Split
it: snapshot the minimal attach inputs (session_name, remote, rows, cols)
under the lock; release the lock; run `AttachedViewer::spawn` (or
`spawn_remote`) on `smol::unblock`; reacquire the lock; validate the desired
target still matches (the scheduler's `in_flight` guard already prevents
duplicate performs, but we also guard against the agent being killed while
attach was in flight); apply the viewer into the runtime.

This requires exposing a `RuntimeManager::apply_attach_result` method that
takes a pre-built viewer + agent_id and installs it, plus a
`RuntimeManager::attach_inputs(agent_id) -> Option<AttachInputs>` that
snapshots the data needed to build a viewer without holding the lock.

**Files.**
- `src/runtime/manager.rs` — `AttachInputs`, `attach_inputs()`,
  `apply_attach_result()`.
- `src/app_shell.rs::perform_async_attach` — rewrite to snapshot/release/work/
  reacquire/validate/apply.

**Tests (RED first).**
1. `terminal_input_processed_while_background_attach_blocked` — start a
   background attach that blocks; send terminal bytes; assert they are
   forwarded without waiting for the attach.
2. `stale_attach_does_not_resurrect_stopped_agent` — request attach A; kill
   agent; A completes; assert the agent stays dead and the viewer is not
   installed.
3. `attach_superseded_by_newer_target` — request attach A; before it
   completes, set desired B; A completes; assert B is authoritative.

### Phase 4 — Liveness stale-result protection

**Design.** Add a `LivenessGeneration` (u64) to `LivenessCheck` and to the
result flow. The app_shell liveness future snapshots the current
`runtime_binding` session name + a per-agent generation (incremented on
spawn/relaunch/kill/rebind). `batch_liveness_check` returns
`Vec<(AgentId, observed_session_name, request_generation)>`. Before marking an
agent dead, verify the agent's current `runtime_binding` still references
`observed_session_name` and the request_generation matches the agent's current
lifecycle generation. A mismatch means the agent was rebound/restarted; skip.

**Files.**
- `src/runtime/manager.rs` — `LivenessCheck` gains `generation: u64` and
  `binding_session_name: Option<String>` (snapshot of the current binding).
- `src/runtime/liveness.rs` — `reconcile_dead_agents` returns identity triple.
- `src/app_shell.rs` — liveness future validates before applying.
- `src/state` — agent lifecycle generation (or store it in the runtime
  binding).

**Tests (RED first).**
1. `liveness_ignores_stale_result_after_rebind` — check session A; rebind to
   B; stale "A missing" result arrives; agent stays Running on B.
2. `liveness_ignores_stale_result_after_restart` — check; restart; stale
   result arrives; agent stays Running.
3. `batch_command_count_constant_with_agent_count` — (already a property;
   add a deterministic test asserting two subprocesses regardless of N).

### Phase 5 — F12 as intent, not synchronous attach

**Design.** `handle_f12_toggle` currently calls `attach_for_f12` inline (locks
ctx, calls `runtime.attach`). Replace with: update `pane_focus` /
`terminal_focused` deterministically (immediate state transition) and set
the `AttachScheduler` desired target (already debounced). The background
attach future (Phase 3) performs the actual attach. F12 returns immediately.

**Files.**
- `src/app_input/modal_handlers.rs::handle_f12_toggle` — remove
  `attach_for_f12`; set desired only.
- `src/app_shell.rs` — the render body already sets desired from
  `selected_running_agent_id`; F12 now just flips focus intent.

**Tests (RED first).**
1. `f12_updates_focus_intent_while_attach_blocked` — block the attach worker;
   send F12; assert `terminal_focused` / `pane_focus` change within a bounded
   budget without waiting for attach.
2. `f12_terminal_input_after_toggle` — F12 then immediate key; assert the key
   is forwarded (attach may still be pending).

### Phase 6 — Harness lifecycle ownership

**Design.** Add a `TmuxSessionGuard` RAII that owns a `TmuxSession` + a
`TmuxDriver` reference and kills the session + server namespace on drop. The
runner uses the guard for every scenario path (success, assertion failure,
timeout, panic). Add a test that asserts teardown after each exit path. Forbid
bare default-server commands in harness code (already largely enforced by
`harness_socket_name`).

**Files.**
- `src/harness/runner.rs` — replace manual `cleanup_session` with a guard.
- `src/harness/tmux_driver.rs` — guard type.

**Tests (RED first).**
1. `harness_teardown_on_success` — run a passing scenario; assert the session
   socket is gone.
2. `harness_teardown_on_assertion_failure` — run a failing scenario; assert
   teardown.
3. `harness_teardown_on_timeout` — scenario with an unsatisfiable `waitFor`;
   assert teardown.
4. `harness_teardown_on_launch_failure` — request a non-existent binary;
   assert no server leaked.

### Phase 7 — Strict TUI responsiveness scenario

Update `dev-docs/tmux-scenarios/liveness-responsiveness.json` to a strict,
assertion-based scenario with a production-scale fixture and controlled delayed
boundaries. This is the acceptance test that input stays responsive while
liveness/persistence/capture/attach are deliberately delayed.

## Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- complexity-only clippy pass
- `cargo llvm-cov --workspace --all-features --fail-under-lines 30`
- `cargo build --workspace --all-features --locked`
- `cargo test --workspace --all-features --locked`
- No loosened lint/complexity/coverage gates.
- No `unwrap`/`expect` in production paths.
- No `unsafe`.

## Non-goals (from issue)

- No silent input drops.
- No global durability weakening.
- No arbitrary sleeps as synchronization.
- No reliance on host load.
- No new global lock held across external work.
- No unbounded tmux subprocess parallelism.
- Screenshots/no-panic smoke tests are not sufficient.