# Issue 437 delivery plan

## Issue

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/437
- Branch: `issue437`
- Base: `origin/main` at `53b891c`
- Related closed issue: https://github.com/vybestack/llxprt-jefe/issues/351
- Reported behavior: while the Issues workspace is visible, a `smol::unblock` worker prints a `generational-box` panic directly into the terminal UI. The reporter wants recoverable background failures retained on the Errors screen without immediately replacing or overlaying the active workspace.

## Grounded diagnosis

The reported dependency line is `generational-box` 0.5.6 `MemoryLocationBorrowInfo::borrow_error`, where diagnostic code unwraps the recorded mutable-borrow location after a conflicting iocraft-state borrow. This differs from issue 351's reported line and demonstrates concurrent access to iocraft hook state rather than a GitHub empty-list parsing failure.

Issue 351 migrated only issue-list fetches to the existing root-owned `BackgroundGhDelivery` queue. Current main still has 31 production calls to `spawn_gh_task_with_panic`; those calls copy `State<AppState>` into `smol::unblock` and read or write it from a blocking worker while the iocraft executor may render or process input. Three of those routes are issue-detail reads in `issues_dispatch.rs`, but the issue report contains no action, backtrace, or task identifier that distinguishes them from an earlier Issues mutation, PR refresh, Actions request, authentication request, or other legacy GitHub task completing after the visible screen changed.

`catch_unwind` does not suppress Rust's panic hook: the default hook writes the panic to stderr before the payload is returned. Because a legacy panic callback then accesses the same off-thread iocraft state, it also cannot reliably append to the Errors store. Hiding that output without removing the off-thread state access would conceal the architecture defect and is not an acceptable root fix.

## Exact race mechanism

`GenerationalBox::try_read` fails while another thread holds the data write lock, then builds a diagnostic through `MemoryLocationBorrowInfo::borrow_error`, which evaluates `self.borrowed_mut_at.read().unwrap()`. The writer's `GenerationalRefBorrowMutGuard::drop` clears that same field with `borrowed_mut_at.write().take()`. When the writer releases between the failed data read and the diagnostic read, the `Option` is already `None` and the unwrap panics at `lib.rs:357:58`.

`SyncStorage` uses `parking_lot` locks, and `debug_assertions` compile this diagnostic code into dev/test builds. The panic is therefore reachable only when one `State<AppState>` is borrowed from two threads at once. `spawn_gh_task_with_panic` invokes `work(app_state, ctx)` inside `smol::unblock`, so all 31 production call sites read and write iocraft state from a `blocking-N` worker thread, exactly matching the reported thread name. Every other `smol::unblock` site awaits the blocking call first and touches state on the executor thread, so those are not implicated.

## Resolved decision gate

Approved boundary: migrate every legacy GitHub route to state-free blocking work with root-executor application, then delete the unsafe helper. Partial migration was rejected because the report cannot attribute the panic to a specific route and because a second partial fix would repeat the outcome of issue 351.

The migration is made mechanical and self-enforcing by changing the helper signature so blocking work never receives an `AppStateHandle`:

- `work: FnOnce(&SharedContext) -> T` runs on the blocking thread and may only perform GitHub I/O.
- `apply: FnOnce(&mut AppStateHandle, &SharedContext, T)` and the panic continuation run on the render thread through the existing root-owned `BackgroundGhDelivery` queue.

No migrated call site passes a state handle into a worker: the helper's blocking closure receives only the shared context, so reintroducing the old shape requires deliberately capturing state rather than merely following the existing signature.

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
| CodeRabbit marked issue 437 as a possible duplicate of issue 351 | Not a duplicate | Issue 351 intentionally migrated only issue-list fetch; 31 legacy stateful worker calls remain. The current panic is at a different generational-box line indicating a conflicting borrow. |
| Issue report provides no triggering action or backtrace | Decision required | Visible Issues mode does not identify the worker that panicked; a task from another mode may complete after navigation. Route coverage changes architecture and PR size materially. |
| `catch_unwind` still runs the default panic hook | In-scope only for selected state-free recoverable request boundary | Required to meet the no-TUI-dump acceptance without hiding uncontained panics. A global arbitrary-panic ingestion queue remains out of scope. |
| Current panic callbacks access the same copied iocraft state after a worker panic | Root-cause defect | Error-page recording cannot be reliable until the selected worker routes become state-free. |
| Existing Errors capture is additive to inline mode errors | Accepted behavior change for worker panics only | Caught worker panic delivery must append directly and clear pending state without setting the inline mode error slot, matching the reporter's non-immediate preference. Ordinary `GhError` behavior remains unchanged. |
| A 32nd call site (`new_issue_submit.rs`, issue 407) merged to main while this branch was open | In-scope-Fix | Deleting the helper turned an otherwise silent semantic conflict into a CI compile error, which is the intended guardrail. The new route was migrated to `spawn_gh_work` during rebase. |
| Remaining legacy routes outside the selected scope | Resolved by full migration | All legacy production callers (31 at branch point, 32 after integrating issue 407) now use `spawn_gh_work`; `spawn_gh_task_with_panic` is deleted, so the old signature no longer exists to copy. |
| No deterministic way to trigger a real worker panic from the TUI | Rejected adding a production panic-injection hook | A `JEFE_*` panic-injection env var would be test-only behavior inside the shipped binary, which this plan lists as an explicit stop condition. The panic-to-Errors path is instead proven by `silent_route_panic_is_recorded_without_leaving_the_active_screen`, which drives a real panicking worker through the real delivery queue in an iocraft mock terminal and asserts both the recorded entry and the unchanged active screen. The real Errors screen is separately proven by the `errors-mode.json` TUI scenario. |

## Review counters

- Pre-PR Open Code Review: 1 / 2 (22 files, 7 comments; a first attempt with a `A..B` range reviewed 0 files and was not counted as coverage)
- Post-PR Open Code Review: 2 / 2 budgeted; CI re-triggers OCR automatically on every push, producing 4 total runs and 52 inline comments. All triaged; no additional review was requested.
- Independent Rust review: 1
- Review/remediation cycles total: 2 / 2 (limit reached; no further review rounds)

## Review triage

| # | Source | Finding | Disposition | Action |
| --- | --- | --- | --- | --- |
| 1 | Rust review | `spawn_gh_work` does not structurally forbid capturing an `AppStateHandle` in the worker closure | Reject (claim), Defer (hardening) | Correct that a `FnOnce` can still capture state, so the barrier is a strong convention rather than a proof. The plan's wording was corrected to claim only that the helper no longer *passes* a state handle. Converting the seam to a non-capturing `fn` pointer plus an owned input DTO would touch all 31 routes again and is a separate design change, so it is recorded as a follow-up rather than taken here. |
| 2 | Rust review | A completion could reach a *newly installed* root handler after the original owner is torn down | Reject | Not reachable: the root `App` is mounted exactly once in `src/main.rs:227` and is never remounted, so no second owner can install into the same `AppContext`. The pre-existing late-delivery contract from issue 351 still covers teardown with no replacement. |
| 3 | Rust review | Panic continuations produce an inline banner and two Errors entries | Reject (as characterized) | Measured with a temporary instrumented probe driving a real panic through the real queue on a non-silent route: the result was exactly one Errors entry and no banner, because the route's failure event is correlation-rejected. Behavior for a *matching* correlated failure is the route's ordinary, pre-existing visible-error semantics, which this issue explicitly does not change. |
| 4 | Rust review | `capture_worker_panic` shifts the selected Errors entry when the user is browsing | Reject | Index-stable-on-insert is pre-existing `ErrorsState::push` behavior from issue 292, is documented on that method, and is locked by an existing test asserting the index is preserved when `snap_to_newest` is false. Changing it would alter issue-292 behavior for every caller and is outside this scope. |
| 5 | Rust review | No test proves panic output is suppressed or that uncontained panics still report | Blocker-Fix | Added `contained_panics_are_silent_and_uncontained_panics_still_report`, which re-executes itself in a child process so a sentinel hook can be installed ahead of the process-global one, then asserts the delegate is called zero times for a contained panic and exactly once for an uncontained one. Added `concurrent_containment_keeps_locations_independent` for two threads panicking behind a barrier. |
| 6 | Rust review | `delivery_handle_or_report` succeeds whenever `AppContext` exists, even with no handler installed | Defer | Real but unreachable today: every dispatch runs from terminal-event handling, which happens only after the root installs the handler on its first render. Narrowing the check requires the same owner-token redesign as finding 1 and is recorded with it. |
| 7 | Rust review | A stale panic location could be attributed to a later payload | In-scope-Fix | `contain` now clears the location slot on entry and reads it once after the boundary. Added `a_location_from_earlier_work_is_not_reused` (resumed payload keeps no earlier site) and `the_reported_location_is_the_escaping_panic`. |
| 8 | Rust review | New tests use synthetic probes rather than real migrated routes | Defer | A route-level behavioral matrix across ~31 routes is a large test-only expansion beyond this issue's acceptance rows. Route semantics are preserved mechanically (each route keeps its own typed failure event), and the existing per-route reducer suites still pass. Recorded as a follow-up. |
| 9 | Rust review | Plan says "32 production calls" | In-scope-Fix | Verified with `git grep` on the base: 31 production call sites plus one definition and one test reference. Plan corrected. |
| 10 | OCR | Property-edit routes return without UI feedback when the delivery queue is missing | Reject | `delivery_handle_or_report` invokes the supplied reporter before returning `None`; both property-edit routes pass `property_edit_abandoned`/`options_abandoned`, so the editor always receives a typed failure. |
| 11 | OCR | `options_abandoned` / `pr_options_abandoned` discard the message and hardcode "panicked" | In-scope-Fix | Both now include the message and are reworded to "Options fetch abandoned", which is accurate for a missing queue as well as a panic. |
| 12 | OCR | Silent issue-detail refresh discards the panic message | Reject | Intentional and required: this route must not surface a visible error (issue 175). The diagnostic is not lost — `record_worker_panic` records the message and its source location on the Errors screen before the route's silent handler runs, which is exactly what `silent_route_panic_is_recorded_without_leaving_the_active_screen` proves. |
| 13 | OCR | Test couples to the literal file name `worker_panic.rs` | Reject | The assertion is paired with a `starts_with("... (at ")` check and pins that the location resolves to this module's panic site rather than a caller's. A rename is a deliberate edit that should update its own test. |
| 14 | OCR | Nested `abandoned` helpers are inconsistent with the shared module-level pattern | In-scope-Fix | All remaining nested helpers hoisted to module scope with descriptive names (`pr_thread_resolve_abandoned`, `pr_property_edit_abandoned`, `merge_abandoned`, `open_in_browser_abandoned`, `list_abandoned`), which also removes the `items_after_statements` lint. |

## Verification evidence

| Candidate head | Command / evidence | Result |
| --- | --- | --- |
| `53b891c` | `cargo tree -i generational-box@0.5.6` | `generational-box` is used through vendored iocraft 0.5.3 |
| `53b891c` | inspect dependency line 357 | panic site is conflicting-borrow diagnostic state, not issue parsing |
| `53b891c` | production caller inventory | 31 production `spawn_gh_task_with_panic` call sites still move iocraft AppState into blocking workers; issue list alone uses the state-free delivery helper |
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

## Post-PR review triage (CI OpenCodeReview)

| # | Finding | Disposition | Action |
| --- | --- | --- | --- |
| 15 | Abandoned handlers say "task panicked" but also run when the delivery queue is unavailable, so an availability failure is reported as a panic | In-scope-Fix | Correct and systemic. Reworded all 15 route messages to "abandoned", which is accurate for both a contained panic and a missing queue; the message itself still carries the specific cause. The Errors-screen title "Background task panicked" is unchanged because that path only ever runs for a real panic. |
| 16 | Terminology is inconsistent between the PR and issue property-edit paths | In-scope-Fix | Resolved by the same sweep; every route now uses one verb. |
| 17 | `AssertUnwindSafe` has no documented rationale | In-scope-Fix | Added the reasoning: `work` is consumed by the call, its captures are dropped with the closure on unwind, the payload is converted to a `String`, no borrow crosses the boundary, and callers pass owned request data. |
| 18 | `page_size: 30` is a magic number | In-scope-Fix | Extracted `COMMENT_PAGE_SIZE`. |
| 19 | `Apply` continuations run on the render thread without panic protection; wrap them in `catch_unwind` or document the contract | Reject (wrap), In-scope-Fix (document) | Wrapping would contradict the project's fail-fast preference: a panic in a reducer continuation is a genuine state bug, and swallowing it would hide exactly the class of defect this issue exists to surface. Containment is deliberately scoped to external `gh` work. Documented the contract on `spawn_gh_work` instead. |
| 20 | `comment_page_params` can forward `cursor: None` when `has_more()` is true but the token is a `PageNumber` | Reject | Unreachable for issue comments. That list's `next_page` is only ever built by `PageToken::from_cursor`, which yields `Cursor` or `Done`, and `has_more()` is false for `Done`; `PageNumber` is produced only by the REST helper, which comments never use. The `issue_comment_cursor` match is retained as an explicit total-match guard, locked by its two unit tests. |
| 21 | `drop(state)` in `comment_page_params` is redundant | Reject | Removing it fails the build: `clippy::significant_drop_tightening` requires the explicit drop because the caller immediately acquires a write guard. Verified by making the change and observing the lint error. Added a comment recording why it is required. |
| 22 | Breaking signature change: `handle_pr_thread_resolve` and other dispatchers now take `&mut AppStateHandle` | Reject | Not a breaking change to any external contract: these are crate-internal dispatch functions with no downstream consumers, and every call site is updated in this PR. `AppStateHandle` is `Copy`, so the mutable reference expresses the write intent rather than enabling new aliasing. A missed call site would be a compile error, and the workspace builds clean. |
| 23 | `params` is cloned twice in `pr_options_abandoned` setup | Reject | Both clones are required: one is consumed by the reporter passed to `delivery_handle_or_report`, the other by the panic handler given to `spawn_gh_work`. Because each closure owns its copy, neither can be elided without making one handler unconstructible. |

## Scope review (mandatory: 26 files vs the 25-file target)

The PR is 26 changed files and +1,052 net lines against its merge base, one file over the 25-file review trigger and well inside the 1,500-line target and the 40-file / 2,500-line hard stop.

Cause: two files were added after the branch opened, both forced by integrating `origin/main` rather than by discretionary scope. `src/app_input/new_issue_submit.rs` is the issue-407 route that merged mid-flight and had to be migrated to compile. `src/app_input/issues_comments_dispatch.rs` exists because absorbing that route pushed `issues_dispatch.rs` to 865 lines, past the 850-line handler boundary; the comment-pagination route was moved out instead of relaxing the limit.

Reviewability is unaffected: 21 of the 26 files are mechanical single-route conversions to the same helper, and the extracted module is a move with no behavior change. Continuing without splitting.

## Post-PR review triage, round 2 (CI OpenCodeReview on the docs-only head)

The review budget (2 cycles) is spent; this round ran automatically because CI re-triggers OCR on every push. All 16 comments were still triaged.

| # | Finding | Disposition | Action |
| --- | --- | --- | --- |
| 24 | `mark_comment_failure_pending` drops the failure event when `begin_issue_comment_page` returns `None` | Reject (pre-existing) | Verified against base `c14a223`: the `?` on `begin_issue_comment_page` and the `Option` return predate this PR. The extraction moved the function verbatim; no behavior changed. Recorded as a follow-up candidate rather than smuggled in here. |
| 25 | Merge-methods load has an empty panic handler, so users silently fall back to "all available" | Reject (pre-existing, intentional) | Verified against base: the chooser's graceful fallback is the established behavior and the code comment already states it. Changing it is a UX decision outside this issue. `record_worker_panic` still records the panic on the Errors screen, so it is no longer invisible. |
| 26 | `COMMENT_PAGE_SIZE` could drift from the issue-list page size | Defer | Fair, but adding `static_assertions` is a dependency change requiring approval, and the two constants are three lines apart with a comment binding them. |
| 27 | `MISSING_DETAIL_REPO_MSG` widened to `pub(super)` leaks a string | Defer | Narrow visibility widening forced by the 850-line boundary extraction; `pub(super)` is still crate-internal. Not worth a further refactor at this stage of the PR. |
| 28 | `assignment_abandoned` defined after its call site | Reject | This is the deliberate convention adopted across all 32 routes in this PR after clippy's `items_after_statements` finding; helpers sit below the dispatcher they serve. Consistency beats local reordering. |
| 29 | Wrap `apply`/`on_panic` in `catch_unwind` | Reject (repeat of 19) | Same reasoning: containment is scoped to external `gh` work on purpose so reducer bugs stay loud. |
| 30 | Breaking API change to `&mut AppStateHandle`; add a compat shim | Reject (repeat of 22) | Crate-internal functions, no external consumers; the suggested shim would reintroduce the aliasing this PR removes. |
| 31 | Double clone in the PR property-edit paths | Reject (repeat of 23) | Both clones are consumed by distinct closures. |
| 32 | Audit all callers of the property-edit dispatchers | Reject | Already guaranteed: a missed call site is a compile error, and the workspace builds clean on this head. |
| 33 | Extra `owner_repo` clone in `issues_send` | Reject | Same ownership constraint; two closures each need an owned copy. |
| 34 | Document that "abandoned" now covers both panics and queue-unavailable | In-scope-Fix | Accepted and recorded here: the abandoned wording deliberately spans both causes, and the appended message carries the specific one. |
| 35 | `install_hook` captures the previous hook once; later hooks are not delegated to | Defer | Correct in principle. Jefe installs no other hook, and the `Once` guard is what makes delegation deterministic under concurrency. A dynamic strategy is a design change, not a fix. |
| 36 | Threads spawned inside `work` do not inherit containment | Defer | Accurate limitation. No migrated route spawns threads inside `work`; all blocking work is a direct `gh` call. |
| 37-39 | Three comments confirming the `spawn_gh_work` split and early-return path are correct | Acknowledged | No action needed. |

## Post-PR review triage, rounds 3-4 (automatic CI OpenCodeReview re-runs)

CI re-triggers OCR on every push, so two further rounds ran after the budget was spent. All 21 comments were triaged; several made falsifiable claims that were checked against the code rather than accepted.

| # | Finding | Disposition | Evidence |
| --- | --- | --- | --- |
| 40 | `describe` takes `&Box<dyn Any + Send>`, an unnecessary indirection | In-scope-Fix | Correct. Changed to `&dyn Any`; clippy then required `&*payload` at the call site, confirming the coercion was real. |
| 41 | "New issue submit abandoned" uses a verb as a noun | In-scope-Fix | Correct for a user-facing string. Changed to "submission". |
| 42 | Dead code: `let repo_id = repo.id.clone();` in `dispatch_workflow_run` | Reject | No such binding exists. The function uses the `scope_repo_id` parameter throughout; the only clones are `scope_repo_id.clone()` and `panic_scope_repo_id`. Clippy's `unused_variables` would fail the build if it existed. |
| 43 | `resume_unwind` invokes the panic hook, so `a_location_from_earlier_work_is_not_reused` will fail | Reject | Factually wrong. `std/src/panic.rs:362` documents `resume_unwind` as "Triggers a panic without invoking the panic hook." The named test passes; the whole module is 10/10 green. |
| 44 | `spawn_device_auth_flow` requires `&mut` but its caller holds a shared reference, so it "will fail to compile" | Reject | The sole caller (`modal_handlers.rs:137`) already holds `&mut` — the preceding line calls `apply_and_persist(app_state, ...)`, which requires it. The workspace compiles clean. |
| 45 | Auth panics are double-reported (Errors screen + dialog) | Reject | Intentional and distinct. `record_worker_panic` writes the diagnostic to the Errors screen; `report_auth_failed` unsticks the modal so the user can retry. Removing either leaves the dialog hung or the panic undiagnosable. |
| 46-52 | Double clone of params/action/scope/dispatched across seven routes; suggestions to borrow, use `Cow`, or clone lazily | Reject | `delivery_handle_or_report` takes `report: impl FnOnce(...)`, so it consumes its reporter; the panic handler passed to `spawn_gh_work` must be a separate `'static` `FnOnce`. Two independent `FnOnce` values cannot share one owned capture. The suggested lazy construction would require the reporter to outlive the call, which the signature forbids. These are one small clone per user-initiated request, not a hot path. |
| 53 | Revert the property-edit and `prs_mutation` dispatchers to `&AppStateHandle` | Reject | Repeat of 22/30. The `&mut` is what makes the render-thread ownership explicit; `AppStateHandle` is `Copy`, so this constrains nothing at runtime. |
| 54 | Verify no test or log parser matches the old "task panicked" wording | Reject (already verified) | `grep` over `src/`, `tests/`, `dev-docs/`, `docs/` finds no remaining match except the Errors-screen title `"Background task panicked"`, which is deliberate and asserted at `gh_async.rs:494`. |
| 55 | Preserve "panicked" for real panics and reserve "abandoned" for queue unavailability | Reject | This is the inverse of finding 15, which was accepted for being correct. One verb covers both causes and the appended message carries the specific one; the Errors entry title still distinguishes a genuine panic. |
| 56 | `INSTALL.call_once` costs an atomic check per `contain` call | Reject | Micro-optimization on a path that spawns a subprocess and performs network I/O. The `Once` is what makes hook installation race-free. |

## Exact-head completion

Candidate head `issue437` rebased onto `ff9b6e3`, PR https://github.com/vybestack/llxprt-jefe/pull/452, base `main`.

| Gate | Result |
| --- | --- |
| Acceptance rows A1-A4 | behavioral evidence present (render-thread affinity, silent-route panic capture, hook delegation, late-delivery teardown) |
| Local verification | rustfmt, both clippy gates, clippy-allow policy, architecture boundary, source-size, coverage, build, and full workspace suite all pass |
| `errors-mode` TUI scenario | re-run on this head: ok, 9 steps |
| Required CI | 12 of 12 checks pass, including Native Windows, Coverage gate, and OpenCodeReview; the optional tmux smoke job is skipped by design |
| Ancestry / conflicts | rebased onto `ff9b6e3` after mainline drift; `origin/main` is an ancestor of the head; merge state CLEAN, MERGEABLE |
| Known-flaky suite note | Two real-process harness tests (`harness_v1_fixtures::settings_edit_fixture_executes_configured_editor_as_argv`, `harness::tmux_driver::tests::real_jefe_session_uses_isolated_config_when_binary_available`) fail intermittently under full parallel workspace load. Reproduced on unmodified `origin/main` (1 of 6 full runs), so this is pre-existing environmental flakiness in the multiplexer harness, not a regression from this PR. Neither test is touched by this change. |
| Reviews | 2 of 2 cycles used; every finding fixed, rejected with evidence, or deferred |
| Scope ledger | clean; the 26-file scope review is recorded above |

Stopping here per the workflow: accepted behavior is proven and all required gates pass. The two deferred items below are follow-ups, not remaining work on this issue.

## Deferred findings and follow-ups

- Make the worker seam structurally unable to capture iocraft state (non-capturing `fn` pointer plus an owned input DTO). Touches all migrated routes again, so it belongs in its own change.
- Add per-route panic tests beyond the boundary-level proofs.
- Any valid review improvement outside the selected acceptance rows will be recorded here and proposed as a follow-up rather than implemented automatically.
