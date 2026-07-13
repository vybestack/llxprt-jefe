# Plan — Issue #164: F12 defocus across Issues/PR views + cross-mode `i`/`p` navigation

## Problem

1. **F12 is dashboard-centric.** `handle_f12_toggle` (in
   `src/app_input/modal_handlers.rs`) is invoked globally from
   `src/app_shell.rs` (`handle_key_event`) *before* any mode-specific key
   routing. It always routes to `PaneFocus::Agents`/`PaneFocus::Terminal`,
   which is meaningless in Issues/PR mode. Pressing F12 while viewing an
   issue or PR detail yanks `pane_focus` to `Agents` and is a confusing
   no-op.

2. **No cross-mode `i`/`p` navigation.** From the PR screen, `i` does not
   switch to Issues; from the Issues screen, `p` does not switch to PRs.
   Today `i`/`p` only enter their respective modes from `Dashboard`.

## Scope / non-goals

- **In scope:** make F12 mode-aware (dashboard unchanged, issues/PRs defocus
  terminal or return to list), and add `i`→Issues / `p`→PRs cross-mode keys.
- **Non-goal (explicitly deferred):** the issue body floats "a first-class
  state system for keys/modes instead of this spaghetti keyhandler garbage".
  That is a much larger architectural refactor. The maintainers' immediate
  requirement is the behavioral fix. The cross-mode key handling is
  implemented by extending the *existing* pure-resolver pattern
  (`resolve_*_key_event`) so the change is idiomatic and reviewable, not a
  wholesale rewrite. A note documenting the deferral is added in the PR body.
- **Non-goal:** focusing the terminal from the Issues/PR list via F12 when a
  running agent is selected (the issue calls this optional). The PR keeps F12
  in Issues/PRs strictly a "defocus / go back to list" affordance to avoid
  surprising pane-focus changes, matching the issue's primary expectation.

## Architecture decision

The F12 global interception in `app_shell.rs` is the root cause: it bypasses
mode-specific resolvers. The clean fix is to **stop intercepting F12 globally
for Issues/PR mode** and let each mode's pure key resolver own F12 semantics.

`handle_key_event` already reads `screen_mode` up front. We change the F12
block so that:
- `ScreenMode::Dashboard` / `Split` → existing `handle_f12_toggle` (unchanged).
- `ScreenMode::DashboardIssues` / `DashboardPullRequests` → fall through to
  the normal mode-specific dispatch (`dispatch_mode_specific_key` +
  `handle_normal_key_event`), which already routes to
  `handle_issues_mode_key` / `handle_prs_mode_key`.

Then `resolve_issues_key_event` / `resolve_prs_key_event` gain an F12 arm.

This keeps the change inside the established pure-resolver + reducer layer
(UI emits intent, state owns transitions), respecting the module boundaries
in `dev-docs/project-standards.md`.

## Test-first plan (RED → GREEN → REFACTOR)

### Unit tests (pure resolvers) — written FIRST, must fail before impl

Add to `src/app_input/issues_key_tests.rs`:

- `f12_in_issue_detail_returns_to_list`: state `IssueFocus::IssueDetail`,
  F12 → `Some(AppEvent::RefocusIssueList)`.
- `f12_in_issue_list_is_noop`: state `IssueFocus::IssueList`, F12 → `None`
  (already at list; no terminal focus, no detail to leave).
- `f12_while_terminal_focused_defocuses_and_returns_to_list`: state with
  `terminal_focused = true` and `IssueFocus::IssueDetail`, F12 →
  `Some(AppEvent::ToggleTerminalFocus)` (defocus), and a second resolver
  pass / reducer keeps `issue_focus` at list. (Detail is left because F12
  acts as "go back".) We assert the emitted event defocuses; the reducer's
  `ToggleTerminalFocus` only flips `terminal_focused`.
- `f12_does_not_fire_when_inline_composer_open`: inline composer active,
  F12 → `None` (composer owns the key; matches the matrix's "form input
  active" rule).
- `f12_does_not_fire_when_search_or_filter_open`: search/filter open, F12 →
  `None`.
- `p_from_issues_enters_prs_mode`: Issues mode, plain `p` →
  `Some(AppEvent::EnterPrsMode)`.
- `p_from_issues_does_not_fire_when_composer_open`: inline composer active,
  `p` → `None` (typed into composer).
- `i_from_issues_still_refocuses_list`: `i` in IssueDetail →
  `RefocusIssueList` (existing behavior preserved; `p` is added alongside).

Add to `src/app_input/prs_key_tests.rs` (mirror):

- `f12_in_pr_detail_returns_to_list` → `RefocusPrList`.
- `f12_in_pr_list_is_noop` → `None`.
- `f12_while_terminal_focused_defocuses` → `ToggleTerminalFocus`.
- `f12_does_not_fire_when_inline_composer_open` → `None`.
- `f12_does_not_fire_when_search_or_filter_open` → `None`.
- `i_from_prs_enters_issues_mode` → `EnterIssuesMode`.
- `p_from_prs_still_refocuses_list` (existing `RefocusPrList` preserved).

### Reducer / state tests (in `src/state/`)

- Verify `EnterIssuesMode` applied while in `DashboardPullRequests` switches
  `screen_mode` to `DashboardIssues` and restores/stashes focus correctly
  (the existing `enter_issues_mode` saves `prior_agent_focus`; this already
  works from any screen — add a regression test that starts from PR mode).
- Mirror for `EnterPrsMode` from `DashboardIssues`.
- These reuse the existing reducers; no new event variants are needed.

### TUI scenario (RED)

`dev-docs/tmux-scenarios/f12-cross-view-defocus.json` — drives the real
binary: enter Issues, F12 (noop at list), `p` → PRs, `i` → Issues, exit.

## Implementation steps (rustcoder)

1. **`src/app_shell.rs`** — make the F12 block mode-aware:
   ```rust
   if key_event.code == KeyCode::F(12)
       && matches!(screen_mode, ScreenMode::Dashboard | ScreenMode::Split)
   {
       handle_f12_toggle(app_state, &ctx.cloned());
       return;
   }
   ```
   (F12 in Issues/PR/Actions modes falls through to mode-specific dispatch.)
   Actions mode: F12 falls through to `resolve_actions_key_event`; leave its
   F12 handling as a no-op (`None`) — consistent with "go back" being
   mode-defined; Actions has no list/detail defocus semantics to add here, so
   F12 is simply not intercepted globally for it either. (Confirm during
   impl that this doesn't break an Actions expectation; if Actions needs F12
   for terminal defocus, scope it then.)

2. **`src/app_input/issues.rs`** — add F12 to `resolve_global_issues_key_event`:
   ```rust
   KeyCode::F(12) => Some(f12_event_for_issues(state)),
   ```
   where `f12_event_for_issues` returns:
   - `AppEvent::ToggleTerminalFocus` if `state.terminal_focused` (defocus),
   - `AppEvent::RefocusIssueList` if `issue_focus == IssueDetail`,
   - `None` otherwise (list, nothing to leave).
   Because `resolve_global_issues_key_event` runs *after* the inline/search/
   filter guards in `resolve_issues_key_event`, the matrix rules
   (composer/search/filter own the key) are satisfied automatically.
   Add `p` → `AppEvent::EnterPrsMode` in the same global arm (next to `i`).

3. **`src/app_input/prs.rs`** — mirror in `resolve_pr_global_key`:
   - F12 → defocus terminal / `RefocusPrList` from detail / `None` from list.
   - `i` → `AppEvent::EnterIssuesMode`.
   Keep existing `p`/`P` → `RefocusPrList`.

4. **`src/state/issues_ops.rs` / `prs_ops.rs`** — no new event variants; the
   existing `EnterIssuesMode`/`EnterPrsMode`/`RefocusIssueList`/
   `RefocusPrList`/`ToggleTerminalFocus` reducers already implement the
   transitions. Add regression tests that cross-entering modes from the other
   mode's screen works (saves prior focus, switches screen_mode).

5. **Tests** — add the unit + state tests above. Run
   `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo build --workspace --all-features --locked`,
   `cargo test --workspace --all-features --locked` (i.e. `make ci-check`).

6. **Help text** — verify keybind bar / terminal view "F12 unfocus" copy
   still accurate (no change needed; F12 now actually unfocuses in more
   places). If a keybind hint for `i`/`p` cross-mode is missing, add a brief
   one only if the existing hint infrastructure makes it trivial; do not
   expand scope.

## Lint/complexity guardrails

- No `eslint-disable`/`allow`/`ts-ignore` analogues (no `#[allow(...)]` for
  clippy in new code; fix the underlying issue instead).
- No new `unwrap`/`expect` in production paths.
- No loosening of complexity thresholds.

## Verification checklist

- [ ] New unit tests fail before impl (RED), pass after (GREEN).
- [ ] `make ci-check` green.
- [ ] TUI scenario passes locally (if tmux available).
- [ ] No regression in existing F12 dashboard tests
      (`tests/ui/dashboard_navigation.rs`).
- [ ] `i`/`p` still enter modes from Dashboard (existing behavior preserved).
