# Issue 437 delivery plan

## Issue

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/437
- Branch: `issue437`
- Base: `origin/main` at `53b891c`
- Related closed issue: https://github.com/vybestack/llxprt-jefe/issues/351
- Reported behavior: while the Issues workspace is visible, a `smol::unblock` worker prints a `generational-box` panic directly into the terminal UI. The reporter wants recoverable background failures retained on the Errors screen without immediately replacing or overlaying the active workspace.

## Grounded diagnosis

The reported dependency line is `generational-box` 0.5.6 `MemoryLocationBorrowInfo::borrow_error`, where diagnostic code unwraps the recorded mutable-borrow location after a conflicting iocraft-state borrow. This differs from issue 351's reported line and demonstrates concurrent access to iocraft hook state rather than a GitHub empty-list parsing failure.

Issue 351 migrated only issue-list fetches to the existing root-owned `BackgroundGhDelivery` queue. Current main still has 32 production calls to `spawn_gh_task_with_panic`; those calls copy `State<AppState>` into `smol::unblock` and read or write it from a blocking worker while the iocraft executor may render or process input. Three of those routes are issue-detail reads in `issues_dispatch.rs`, but the issue report contains no action, backtrace, or task identifier that distinguishes them from an earlier Issues mutation, PR refresh, Actions request, authentication request, or other legacy GitHub task completing after the visible screen changed.

`catch_unwind` does not suppress Rust's panic hook: the default hook writes the panic to stderr before the payload is returned. Because a legacy panic callback then accesses the same off-thread iocraft state, it also cannot reliably append to the Errors store. Hiding that output without removing the off-thread state access would conceal the architecture defect and is not an acceptable root fix.

## Exact race mechanism

`GenerationalBox::try_read` fails while another thread holds the data write lock, then builds a diagnostic through `MemoryLocationBorrowInfo::borrow_error`, which evaluates `self.borrowed_mut_at.read().unwrap()`. The writer's `GenerationalRefBorrowMutGuard::drop` clears that same field with `borrowed_mut_at.write().take()`. When the writer releases between the failed data read and the diagnostic read, the `Option` is already `None` and the unwrap panics at `lib.rs:357:58`.

`SyncStorage` uses `parking_lot` locks, and `debug_assertions` compile this diagnostic code into dev/test builds. The panic is therefore reachable only when one `State<AppState>` is borrowed from two threads at once. `spawn_gh_task_with_panic` invokes `work(app_state, ctx)` inside `smol::unblock`, so all 32 production call sites read and write iocraft state from a `blocking-N` worker thread, exactly matching the reported thread name. Every other `smol::unblock` site awaits the blocking call first and touches state on the executor thread, so those are not implicated.

## Resolved decision gate

Approved boundary: migrate every legacy GitHub route to state-free blocking work with root-executor application, then delete the unsafe helper. Partial migration was rejected because the report cannot attribute the panic to a specific route and because a second partial fix would repeat the outcome of issue 351.

The migration is made mechanical and self-enforcing by changing the helper signature so blocking work never receives an `AppStateHandle`:

- `work: FnOnce(&SharedContext) -> T` runs on the blocking thread and may only perform GitHub I/O.
- `apply: FnOnce(&mut AppStateHandle, &SharedContext, T)` and the panic continuation run on the render thread through the existing root-owned `BackgroundGhDelivery` queue.

Removing `AppStateHandle` from the blocking closure turns the defect from a convention into a compile error, so no call site can regress.

## Candidate acceptance matrix

The rows below are complete except for the route coverage selected at the decision gate.

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure / diagnostics | Permitted side effects | Persistence / compatibility | Behavioral proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | User starts one of the approved GitHub routes while iocraft continues rendering/input | success may arrive during render, after navigation, after a newer correlated request, or after root teardown | all platforms; Unix TUI scenario for terminal evidence | blocking GitHub work never reads or writes iocraft `State<AppState>`; a live matching result applies on the root executor; stale/late results are ignored by existing correlation/lifecycle rules; no generational-box panic reaches the terminal | ordinary `GhError` follows the route's existing visible failure semantics and existing Errors capture | approved GitHub command only; state mutation only on root executor | no persistence schema or public compatibility change | focused lifecycle/concurrency tests plus a fail-closed real-TUI scenario for the selected route set |
| A2 | Approved state-free GitHub worker panics inside its recoverable request boundary | string, owned-string, and unknown panic payloads; active workspace is not Errors | all platforms | panic payload is converted to one bounded Errors entry with the owning source; active workspace and focus remain unchanged; no inline mode banner/modal is opened | full copyable detail is available on Errors; structured tracing remains available when configured; no default panic-hook bytes are written into the TUI | error-store append and correlated loading/pending cleanup only | Errors remain runtime-only; no automatic screen switch | focused panic-boundary test and TUI scenario asserting active workspace stability, absence of raw panic text, then presence of the entry after opening Errors |
| A3 | Approved request finishes after root delivery ownership has ended | success and panic completion | all platforms | delivery never dereferences dropped iocraft state and exits cleanly | post-teardown result is intentionally discarded with existing structured diagnostic behavior | completed external I/O only | existing issue-351 lifecycle contract retained | existing late-delivery regression plus route-specific delivery test |
| A4 | User is already browsing Errors when another approved worker panic arrives | non-newest selection and nonzero detail scroll | all platforms | new entry is retained without stealing current Errors selection or scroll | bounded ring eviction remains the existing oldest-first policy | error-store append only | existing Errors semantics retained | reducer/delivery integration test using current `ErrorsState::push` behavior |

## Explicit non-goals

- No parser workaround or patch to vendored `iocraft` / `generational-box`; the panic is evidence of invalid ownership at the caller boundary.
- No process-wide recovery for arbitrary main-thread, renderer, runtime, persistence, PTY, or third-party panics unless option 3 is explicitly approved.
- No automatic navigation to Errors, modal, toast, or inline issue/PR/action banner for a caught worker panic.
- No change to ordinary GitHub command failure visibility, authentication remediation, retry behavior, filtering, pagination, mutation semantics, or post-mutation refresh semantics.
- No persistence of the Errors ring buffer and no state-schema migration.
- No dependency, workflow, `.github/`, `.llxprt/`, `.code_puppy/`, quality-tool, lint, complexity, source-size, safety, or coverage change.
- No unrelated refactor or test relocation.
- No claim that unselected legacy routes are safe.

## Bounded vertical slices

### Slice S1: selected state-free GitHub delivery routes

- Acceptance rows: A1, A3 for the route set approved at the decision gate.
- Architecture owner: existing `app_input::gh_async` root-lifecycle delivery boundary.
- Integration boundary: worker owns only immutable request data, `SharedContext`, and result construction; root handler alone reads/writes iocraft state.
- RED: add route behavioral tests that block completion while the root renders/navigates and prove current worker/state coupling violates the intended delivery contract.
- GREEN: selected callers use `spawn_gh_request_with_panic`; typed outcomes apply only through `apply_background_gh_delivery`; existing correlation semantics remain intact.
- Non-goals: no panic-output policy or Errors-only presentation in this slice.
- Verification: focused tests, `make quick-check`.
- Stop conditions: a fourth orchestration route in one slice, a new delivery subsystem, public API, dependency/workflow/tooling edit, unrelated refactor, or hard-budget risk.

### Slice S2: recoverable request panic presentation

- Acceptance rows: A2, A4, limited to the state-free request boundary delivered by S1.
- Architecture owner: recoverable worker boundary plus existing Errors aggregate.
- Integration boundary: a private panic-hook policy suppresses default stderr only while a panic is inside the explicit `catch_unwind` boundary; the typed panic delivery clears the matching pending state and appends directly to Errors on the root executor without populating a mode error slot.
- RED: first add a deterministic panic test and UI scenario that show current default-hook output / immediate banner behavior.
- GREEN: no panic text appears in the TUI; one copyable entry appears in Errors; active mode/focus remains unchanged; ordinary uncaught panics retain the prior hook.
- Non-goals: no global arbitrary-panic queue, recovery, retries, or persistence.
- Verification: focused tests, selected fail-closed TUI scenario, `make quick-check`.
- Stop conditions: process-wide panic queue, cross-thread AppState access, a public panic API, test-only production command path, or behavior outside A2/A4.

## Expected paths by layer

The exact route files depend on the decision gate. The bounded Issues-detail option is expected to use:

| Layer | Expected paths | Acceptance mapping |
| --- | --- | --- |
| Delivery boundary | `src/app_input/gh_async.rs`, `src/app_input/mod.rs` | A1-A4 typed result and panic delivery |
| Issues orchestration | `src/app_input/issues_dispatch.rs` and, only if source-size requires an approved extraction, one adjacent private module | A1-A3 three read routes |
| Deterministic state behavior | existing `src/state/errors_types.rs` API; tests may live in existing app-input/state test modules | A2, A4 |
| TUI evidence | existing `dev-docs/tmux-scenarios/errors-mode.json` re-run against the migrated build | A2/A4 Errors screen remains correct under the new dispatch boundary |
| Delivery record | `project-plans/issue437-plan.md` | scope, evidence, reviews, findings |

The all-routes option must replace this section with child-slice/stacked-PR contracts before implementation; no child may contain more than three orchestration routes.

## Scope ledger

| Discovery | Disposition | Rationale / follow-up |
| --- | --- | --- |
| CodeRabbit marked issue 437 as a possible duplicate of issue 351 | Not a duplicate | Issue 351 intentionally migrated only issue-list fetch; 32 legacy stateful worker calls remain. The current panic is at a different generational-box line indicating a conflicting borrow. |
| Issue report provides no triggering action or backtrace | Decision required | Visible Issues mode does not identify the worker that panicked; a task from another mode may complete after navigation. Route coverage changes architecture and PR size materially. |
| `catch_unwind` still runs the default panic hook | In-scope only for selected state-free recoverable request boundary | Required to meet the no-TUI-dump acceptance without hiding uncontained panics. A global arbitrary-panic ingestion queue remains out of scope. |
| Current panic callbacks access the same copied iocraft state after a worker panic | Root-cause defect | Error-page recording cannot be reliable until the selected worker routes become state-free. |
| Existing Errors capture is additive to inline mode errors | Accepted behavior change for worker panics only | Caught worker panic delivery must append directly and clear pending state without setting the inline mode error slot, matching the reporter's non-immediate preference. Ordinary `GhError` behavior remains unchanged. |
| Remaining legacy routes outside the selected scope | Resolved by full migration | All 32 legacy callers now use `spawn_gh_work`; `spawn_gh_task_with_panic` is deleted, so the unsafe pattern cannot be reintroduced without a compile error. |
| No deterministic way to trigger a real worker panic from the TUI | Rejected adding a production panic-injection hook | A `JEFE_*` panic-injection env var would be test-only behavior inside the shipped binary, which this plan lists as an explicit stop condition. The panic-to-Errors path is instead proven by `silent_route_panic_is_recorded_without_leaving_the_active_screen`, which drives a real panicking worker through the real delivery queue in an iocraft mock terminal and asserts both the recorded entry and the unchanged active screen. The real Errors screen is separately proven by the `errors-mode.json` TUI scenario. |

## Review counters

- Pre-PR Open Code Review: 0 / 2
- Post-PR Open Code Review: 0 / 2
- Review/remediation cycles total: 0 / 2

## Verification evidence

| Candidate head | Command / evidence | Result |
| --- | --- | --- |
| `53b891c` | `cargo tree -i generational-box@0.5.6` | `generational-box` is used through vendored iocraft 0.5.3 |
| `53b891c` | inspect dependency line 357 | panic site is conflicting-borrow diagnostic state, not issue parsing |
| `53b891c` | production caller inventory | 32 `spawn_gh_task_with_panic` calls still move iocraft AppState into blocking workers; issue list alone uses the state-free delivery helper |
| `53b891c` | issue/PR history | issue 351 and PR 354 explicitly deferred all non-list GitHub routes and process-level panic capture |
| candidate | `CLIPPY_CONF_DIR=.github/clippy rustup run stable cargo clippy --workspace --all-targets --all-features -- -D warnings` (the exact `make ci-check` invocation) | clean. A bare `cargo clippy` without `CLIPPY_CONF_DIR` reports pre-existing `clippy::duration_suboptimal_units` at `src/runtime/llxprt_install.rs:33`; that is a config artifact, not an issue-437 regression: it reproduces identically on stashed base `53b891c`, and `.github/clippy/clippy.toml` sets `msrv = "1.75"`, under which the suggested `Duration::from_mins` does not exist, so the CI configuration correctly does not raise it. No lint was suppressed to reach this state. |
| candidate | complexity gate (`-D cognitive_complexity -D too_many_lines -D too_many_arguments -D type_complexity -D struct_excessive_bools`) | clean; every lint introduced by this change (unused `mut`, `too_many_arguments`, `items_after_statements`, `single_match_else`, `field_reassign_with_default`, `expect_err`) is fixed at the source rather than suppressed |
| candidate | `scripts/check-architecture.sh`, `scripts/check-clippy-allows.sh`, `scripts/check-source-file-size.sh` | all pass; no file crosses the 1000-line hard limit |
| candidate | `cargo fmt --all --check` | clean |
| candidate | `cargo test --workspace --all-features --locked` | 2354 passed, 1 failed: `harness::tmux_driver::tests::real_jefe_session_uses_isolated_config_when_binary_available` |
| candidate vs base | 20x `cargo test --lib -- harness::` on each tree | pre-existing timing flake, not a regression: base `53b891c` failed 2/20, the candidate failed 1/20. The test gives a real spawned `jefe` process a hard 3-second deadline (`wait_for_screen_literal`) and asserts on an all-blank capture when the process has not rendered yet. Passes deterministically in isolation on both trees. |
| candidate | `jefe-tmux-harness --scenario dev-docs/tmux-scenarios/errors-mode.json` against the rebuilt binary | `ok: 9 steps` — the real Errors screen still opens, renders, and exits under the migrated dispatch boundary |
| candidate | `make ci-check` | fmt, clippy-allow policy, source-size, both Clippy gates, and coverage all pass (coverage 71.98% lines against a 30% floor). The run ended on `settings_edit_fixture_executes_configured_editor_as_argv`. |
| candidate vs base | 12x `cargo test --test harness_v1_fixtures` on base | the same fixture is a pre-existing real-PTY timing flake: base `53b891c` failed 1/12 with the identical `E005 ... not observed within 15000 ms` blank-frame signature. Passes deterministically in isolation on the candidate (19/19). Both flaky tests share one cause — a real spawned process missing a fixed deadline under parallel load — and neither touches the GitHub dispatch boundary this change modifies. |

## Deferred findings and follow-ups

- Pending route-coverage decision.
- Any valid review improvement outside the selected acceptance rows will be recorded here and proposed as a follow-up rather than implemented automatically.
