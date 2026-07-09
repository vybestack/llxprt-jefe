# Phase 11 — Message Bus & Key Routing Impl (GREEN)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P11
- **Prerequisites:** `.completed/P10A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Implement PR key routing, dispatch, and async gh I/O so all P10 RED tests turn GREEN — replacing the
P09 TOTAL NO-OP stub bodies with real routing/dispatch logic. (There is no `todo!()` to remove: P09
stubs were already total and clippy-clean — findings #1 & #4.) All gh I/O runs off the UI thread via
`spawn_gh_task_with_panic` (NFR-001).

## Requirements Implemented (Expanded)

### REQ-PR-001,002,003,004,008,010,011,012,013, NFR-001 (non-blocking I/O)
- **Behavior contract:** GIVEN P10 RED tests, WHEN P11 lands, THEN routing emits the correct events
  per precedence, dispatch routes them, and loaders run asynchronously; the UI thread never blocks
  on `gh`.
- **REQ-PR-012 (open-in-browser) — dispatch/side-effect half implemented HERE:** WHEN `o` is pressed
  with a PR selected/loaded (PrList or PrDetail focus), THEN `handle_pr_list_key`/`handle_pr_detail_key`
  emit `Some(PrOpenInBrowser)`; the `PullRequests(OpenInBrowser)` dispatch arm routes to
  `prs_dispatch::dispatch_pr_open_in_browser`, which resolves the selected PR's scope/owner/name/number
  and spawns `GhClient::open_pull_request_in_browser` (`gh pr view <number> --repo <owner>/<name>
  --web`) via `spawn_gh_task_with_panic` (OFF the UI thread), delivering `PrOpenedInBrowser` on
  success and `PrOpenInBrowserFailed{scope,number,error}` on `Err`/panic — NEVER a UI-thread call,
  NEVER a silent drop. WHEN `o` is pressed with no PR selected, THEN the handler emits
  `Some(PrShowNotice{ kind: NoSelectionToOpen })` (consumed key + non-blocking notice). The reducer
  half (`apply_pr_open_in_browser`/`_failed`) was implemented in P05; this phase owns the routing
  emission + the off-thread launch. The reducer notice is applied+persisted BEFORE the async spawn
  (mirrors the issues send-to-agent precedent `dispatch_agent_chooser_confirm`, mod.rs L744-769)
  (c003 L88-89,190-228; c004 L113-115; c002 L115-122).
  **Why it matters:** open-in-browser is the deliberate handoff for deferred merge/approve/review
  operations; it must never block the UI or silently swallow failures.

## Implementation Tasks

### `src/app_input/prs.rs` (markers + c003 refs per fn)
- `handle_prs_mode_key` — 8-level precedence resolver — c003 L10-48.
- `handle_pr_repo_key` (Up/Down → repo nav events) — c003 L49-56.
- `handle_pr_list_key` (Up/Down/PageUp/PageDown/Home/End/Enter) — c003 L57-71.
- `handle_pr_detail_key` (scroll Up/Down, subfocus `j`/`k` -> PrDetailSubfocusNext/Prev, `c` comment
  from detail subfocus (#56), `r` reply on comment subfocus, `S` chooser, `o` open-in-browser) —
  c003 L72-91. `Tab`/`Shift+Tab` are NOT handled here: they fall through to the P7 pane-cycle so
  Tab cycles panes from `PrDetail` too (issue #46); `Left` is an optional reverse pane-cycle. The `o` key
  resolves to `Some(PrOpenInBrowser)` when a PR is present and `Some(PrShowNotice{ kind:
  NoSelectionToOpen })` otherwise (consume + hint, never silent) — c003 L88-89 (REQ-PR-012). The
  read-only paths (`r` off-comment, `c`/`e` on review/check) MUST return `Some(PrShowNotice{ kind })`
  (consumed key + non-blocking notice) — NEVER a bare `None` (finding #4; REQ-PR-010/013; c003
  L83-89, no silent-`None` drops). The `kind` is the matching `ReadOnlyHintKind` variant.
- `handle_pr_inline_key` — c003 L99-108.
- `handle_pr_agent_chooser_key` — c003 L120-126.
- `handle_pr_search_input_key` (reuse `route_search_key`) — c003 L127-133.
- `handle_pr_filter_controls_key` (Tab/Space/text/Enter/Ctrl-c/Esc — fully interactive #38/#40) —
  c003 L134-146.
- `handle_esc_in_prs_mode` (inline→chooser→search→filter→exit) — c003 L92-98.

### `src/input.rs` (finding #3 — real routing, GREEN)
- `input_mode_for_state` — REPLACE the P03 compile-only constant-`PrsNormal` arm with the real
  `DashboardPullRequests` precedence routing, mirroring the `DashboardIssues` block: Inline >
  Chooser > Search > Filter > Normal (inspect `prs_state.inline_state`/`agent_chooser`/
  `search_input_focused`/`filter_ui` to return `PrsInline`/`PrsChooser`/`PrsSearch`/`PrsFilter`,
  else `PrsNormal`). This turns the P10 RED test
  `test_input_mode_for_state_routes_dashboard_pull_requests_by_precedence` GREEN — c003 L07,51.

### `src/app_input/prs_list_dispatch.rs`
- `dispatch_pr_list_reload` / `dispatch_pr_list_fetch` / `request_pr_list_reload` — validate slug,
  check auth, set loading, `spawn_gh_task_with_panic` → deliver `PrListLoaded`/`PrListLoadFailed`
  — c004 L127-137.

### `src/app_input/prs_dispatch.rs`
- `load_pr_detail_for_selection`, `load_more_pr_comments` — c004 L138-155; `preview_pr_from_list`,
  `format_pr_prompt(&PrSendPayload)` (renders structured payload → markdown, mirrors
  `format_issue_prompt`) — c003 L176-187.
- `dispatch_pr_open_in_browser` + `pr_open_in_browser_info` — EXACT ordering (mirror the issues
  side-effecting precedent `dispatch_agent_chooser_confirm`, mod.rs L744-769): the reducer
  `apply_pr_open_in_browser` (c001 L349-357) has ALREADY applied the "opening in browser…" notice
  when `PullRequests(OpenInBrowser)` was dispatched and persisted in the `mod.rs` arm (c004 L113-115:
  `apply_and_persist(...)` BEFORE the spawn); this dispatch fn then resolves the selected PR's
  scope/number and, only for a valid repo+selection, `spawn_gh_task_with_panic` calls
  `GhClient::open_pull_request_in_browser` (`gh pr view <number> --repo <owner>/<name> --web`);
  deliver `PrOpenedInBrowser` on success and `PrOpenInBrowserFailed{scope,number,error}` on
  `Err`/panic. The `NoSelection` path NEVER reaches this dispatch (the handler emits
  `PrShowNotice{NoSelectionToOpen}` instead), so no-selection sets the notice and does NOT spawn —
  NEVER a UI-thread call, NEVER a silent drop (REQ-PR-012; c003 L190-228, c002 L115-122).

### `src/app_input/prs_mutation.rs`
- `handle_pr_inline_submit` — submit comment via `spawn_gh_task_with_panic` →
  `PrCommentCreated`/`PrCommentCreateFailed` — c003 L109-119.

### `src/app_input/normal.rs`
- Keep `handle_dashboard_prs_key` wired into `handle_normal_key_event` BEFORE `resolve_mode_key`
  (immediately after the `handle_dashboard_issues_key` call at L94-98). This ordering makes `p`/`P`
  in `DashboardPullRequests` resolve to `Some(RefocusPrList)` via the PR intercept and prevents
  `resolve_mode_key` (Dashboard-only `EnterPrsMode` arm) from re-entering the mode — c003 L05-09,18,
  L24-25 (mirrors the issues `i` guarantee).

### `src/app_input/mod.rs`
- `dispatch_app_message` PR arms wired to the dispatch fns, including the
  `PullRequests(OpenInBrowser)` arm → `prs_dispatch::dispatch_pr_open_in_browser` (REQ-PR-012) —
  c004 L97-118,113-115.
- `dispatch_prs_navigation`, `refresh_prs_navigation`, `refresh_repo_scope_if_changed_prs`,
  `reset_pr_list_for_repo_change`, `refresh_pr_preview_if_changed` — c004 L119-126.
- `update_pr_detail_viewport_rows` (viewport prop from `jefe::layout::prs_detail_viewport_rows`;
  #37/#39) — c004 L156-159.
- `update_pr_list_viewport_rows` (reads `crossterm::size()` ONCE at the dispatch boundary and writes
  `jefe::layout::prs_pane_rows(...)` into `prs_state.list_viewport_rows`, mirroring the existing
  `update_detail_viewport_rows`; the reducer/selection-follow helper read the stored value so no
  reducer touches crossterm) — c001 L177-200 (helper algorithms at L182-196), #55.
- send-to-agent: `dispatch_pr_agent_chooser_confirm`, `pr_send_info`, `focused_pr_comment`,
  `write_pr_prompt(work_dir, payload)` (calls `prs_dispatch::format_pr_prompt(&PrSendPayload)` then
  writes `{work_dir}/.jefe/pr-prompt.md`), `launch_pr_agent`, `attach_pr_agent`,
  `persist_pr_agent_launch_success`, `apply_pr_send_to_agent_failed` — mirror the issue send
  machinery EXACTLY (mod.rs L744-869): `pr_send_info` reads `work_dir`+`signature` from the chosen
  AGENT (NOT from the payload) and builds the payload via `GhClient::build_pr_send_payload` →
  `PrSendPayload` (structured fields; NO `prompt_markdown`/`work_dir`/`signature`); ordering is read
  send-info → `apply_and_persist(PrAgentChooserConfirm)` → `write_pr_prompt` → `launch_pr_agent`
  (c002 L123-136, c003 L147-187).

### Dispatch-ordering behavioral tests (P11-owned; in `src/app_input/app_input_tests.rs`)
These prove the EXACT reducer-before-spawn ordering required by finding #3, mirroring the issues
side-effecting precedent (`dispatch_agent_chooser_confirm`, mod.rs L744-769). The async `gh` call is
exercised through the existing dispatch test harness with a `None`/test `SharedContext` so no real
`gh` runs and no real browser launches.

**Async-side-effect verification = OBSERVABLE STATE, not spawn recording (finding #5).**
`spawn_gh_task_with_panic` (`src/app_input/gh_async.rs` L11) is a concrete free function with NO
injected recorder/spawn-count seam — and the codebase has no such seam anywhere (no `TaskSpawner`
trait, no spawn counter). The EXISTING issues async-dispatch tests
(`src/app_input/app_input_tests.rs`, e.g. the `dispatch_issue_list_fetch` /
`dispatch_agent_chooser_confirm` paths) therefore assert OBSERVABLE STATE that the synchronous
pre-spawn portion of the dispatch writes — the loading/pending flag and the persisted notice set
BEFORE `spawn_gh_task_with_panic` — NOT a count of spawn calls. These PR tests mirror that exactly:
they assert the synchronously-applied state (notice + the loading/pending flag the dispatch sets,
e.g. via the same `mark_*_loading` precedent that sets `loading.*`/`*_pending` + a monotonic
request id before the spawn). They MUST NOT attempt to record or count `spawn_gh_task_with_panic`
invocations (no such seam exists; do NOT add one).
- `test_open_in_browser_sets_opening_notice_and_marks_pending` — REQ-PR-012 / c004 L113-115,
  c003 L190-215. GIVEN a valid repo + selected/loaded PR, WHEN `PullRequests(OpenInBrowser)` is
  dispatched, THEN the synchronous pre-spawn portion is OBSERVABLE in state: (1) the reducer FIRST
  sets `prs_state.draft_notice == Some("Opening pull request in browser…")` (apply_pr_open_in_browser,
  c001 L349-357) AND (2) `dispatch_pr_open_in_browser` marks the open-in-browser pending/loading flag
  the dispatch sets before spawning. Assert BOTH observable mutations hold (notice first, then the
  pending flag) — NOT a recorded spawn.
- `test_open_in_browser_no_selection_sets_notice_and_no_pending` — REQ-PR-012 / c003 L200-201,
  c001 L353-354. GIVEN no PR selected, WHEN the `o` path runs, THEN the handler emits
  `PrShowNotice{ NoSelectionToOpen }` so `prs_state.draft_notice` is the no-selection message AND the
  open-in-browser pending/loading flag is NOT set (the `NoSelection` path never reaches the dispatch
  that would set it). Assert the observable state: notice == no-selection message AND pending flag
  unset — NOT a spawn count of zero.
- `test_pr_agent_chooser_confirm_applies_reducer_before_side_effects` — REQ-PR-011 / c003 L147-156,
  mod.rs L744-769. Asserts the OBSERVABLE reducer-before-side-effect ordering: after the confirm
  dispatch, the agent chooser is closed and the send is recorded in persisted state
  (`apply_and_persist(PrAgentChooserConfirm)` ran) BEFORE the launch, the prompt file
  `{work_dir}/.jefe/pr-prompt.md` was written (assert the file exists / its contents) before the
  agent-launch state mutation, and the `PrSendPayload` carries the structured fields. Verify ordering
  through these observable state/filesystem effects, mirroring how the issue send-to-agent test
  asserts state + the written prompt file — NOT by recording `write_pr_prompt`/`launch_pr_agent`
  calls.

## Pseudocode Traceability
- component-003 lines 10-232; component-004 lines 97-175.

## Verification Commands

This is a GREEN/impl phase — the COMPLETE baseline below MUST pass (no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# Forbidden-pattern HARD gate: fail (nonzero) if any todo!()/unimplemented!() appears (clippy denies
# both; none should ever have existed in these files — findings #1 & #4).
if rg -n "todo!\(\)|unimplemented!\(\)" src/app_input/prs*.rs ; then
  echo "FAIL: todo!()/unimplemented!() in app_input/prs*.rs"; exit 1
fi
# Positive presence check: every async loader spawns off-thread.
rg -n "spawn_gh_task_with_panic" src/app_input/prs_dispatch.rs src/app_input/prs_list_dispatch.rs src/app_input/prs_mutation.rs
```

## Structural Verification Checklist
- [ ] All P10 RED tests GREEN; existing tests green.
- [ ] No `todo!()`/`unimplemented!()` in `prs*.rs` (clippy denies both).
- [ ] Markers per fn.

## Semantic Verification Checklist (Mandatory)
- [ ] Every gh call wrapped in `spawn_gh_task_with_panic` (cite each loader); on_panic clears
  loading + delivers failure (NFR-001).
- [ ] No blocking `GhClient` call on the UI thread (grep shows only async-wrapped calls).
- [ ] `c` from detail sets composer + subfocus NewComment + follow (#56).
- [ ] `handle_dashboard_prs_key` runs before `resolve_mode_key`; `p`/`P` in PR mode emits
  `RefocusPrList` (never a second `EnterPrsMode`) — cite the resolver-chain ordering.
- [ ] Read-only `r`/`c`/`e` no-op paths return `Some(PrShowNotice{kind})` (consumed + hint), never a
  bare `None` (REQ-PR-010/013, no silent drops) — cite each arm.
- [ ] Filter controls fully interactive (#38/#40).
- [ ] Repo nav independent of pane_focus (#47).
- [ ] Viewport rows passed as prop, not read via `crossterm::size()` in scroll math (#37/#39).
- [ ] Suppressed keys consumed (no fallthrough).
- [ ] No clippy allow / no override; functions within limits (split handlers as needed).

## No-Placeholder / Deferred Detection
HARD inverted gate (finding #6) — absence passes, presence fails:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" src/app_input/prs*.rs ; then
  echo "FAIL: deferred-implementation marker present in impl phase"; exit 1
fi
```

NOTE: this phase-local, file-scoped deferred-marker scan is a fast local guard ONLY; it does NOT
replace the global workspace-wide deferred-marker / no-placeholder gate enforced at P16
(16-e2e-quality-gate). Passing this local scan is necessary but not sufficient; the P16 gate remains
the authoritative final check.

## Success Criteria
- Suite green; async I/O; precedence correct; no placeholders; within limits.

## Failure Recovery
- `git restore` app_input; re-implement per pseudocode; bisect P10A↔P11.

## Phase Completion Marker (`.completed/P11.md`)
Phase ID, timestamp, RED→GREEN list, async-wrap evidence, clippy/fmt result, semantic summary.
