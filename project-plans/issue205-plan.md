# Issue #205: Actions mode — filter by PR

## Goal
Add a "filter by PR" option to the Actions filter bar. When active, the run
list shows only runs whose `event` is `pull_request` and that correspond to a
specific PR number. Also add a cross-mode action: from the PR detail screen,
press `g`/`G` to jump to Actions mode pre-filtered to that PR's runs.

## Design

### Approach
- Add `pr_number: Option<u64>` and `head_sha: Option<String>` to
  `ActionsFilter`. `pr_number` is the user-facing display value; `head_sha`
  is the resolved SHA used for the GitHub API `head_sha=` query parameter.
  This mirrors the existing `workflow` (display) / `workflow_path` (API)
  pattern.
- Add `head_sha: String` to the `PullRequest` and `PullRequestDetail` domain
  types so the SHA can be resolved from loaded PR data. Update the GraphQL
  queries and JSON parsers to fetch `headRefOid`.
- When `pr_number` is set, `build_runs_api_path` appends
  `&event=pull_request&head_sha=<sha>` to the GitHub API query.
- Add a third filter field (`Pr`) to the Actions filter bar that cycles
  through PRs loaded in `prs_state.pull_requests()`.
- Cross-mode: pressing `g`/`G` in PR mode enters Actions mode pre-filtered
  to the currently selected PR (detail or list).

### Files to change (ordered by dependency layer)

#### 1. Domain layer
- `src/domain/actions.rs` — add `pr_number: Option<u64>` and
  `head_sha: Option<String>` to `ActionsFilter`.
- `src/domain/mod.rs` — add `head_sha: String` to `PullRequest` and
  `PullRequestDetail`.

#### 2. GitHub layer
- `src/github/parse_pr.rs` — add `headRefOid` to both GraphQL query strings;
  parse it in `parse_pr_from_node` and `parse_pull_request_detail_json`.
- `src/github/mod.rs` — add `headRefOid` to the `--json` field list in
  `get_pull_request_detail`.
- `src/github/actions.rs` — update `build_runs_api_path` to append
  `&event=pull_request&head_sha=<sha>` when `filter.pr_number` is `Some`.
  Add unit tests.

#### 3. State layer
- `src/state/types.rs` — add `Pr` variant to `ActionsFilterField`.
- `src/state/events.rs` — add `EnterActionsModeWithPrFilter { pr_number: u64,
  head_sha: String }` to `AppEvent`.
- `src/state/actions_ops.rs`:
  - Bump `ACTIONS_FILTER_FIELD_COUNT` to 3.
  - Add `cycle_pr_filter()` (cycles through `prs_state.pull_requests()`).
  - Update `update_draft_filter()` for `Pr` field.
  - Update `CycleFilterStatus` handler for index 2 (calls `cycle_pr_filter`).
  - Add `enter_actions_mode_with_pr_filter(pr_number, head_sha)` method.
  - Update `clear_filter`, `enter_actions_mode`, `reset_actions_for_repo_change`
    to reset `pr_number`/`head_sha` (covered by `ActionsFilter::default()`).
- `src/state/actions_load_ops.rs` — no changes (uses `committed_filter`).

#### 4. Messages layer
- `src/messages/actions.rs` — add `EnterModeWithPrFilter { pr_number, head_sha }`
  variant and `name()`.
- `src/messages/actions_conversion.rs` — add conversions for
  `EnterActionsModeWithPrFilter ↔ EnterModeWithPrFilter`.
- `src/messages/event_conversion.rs` — add `EnterActionsModeWithPrFilter` to
  `is_actions_event` list.

#### 5. Input layer
- `src/app_input/actions.rs` — update `resolve_filter_key_event`:
  - `ClearCurrent` at index 2 → clear `pr_number`/`head_sha`.
  - `CycleNext`/`CyclePrevious` at index 2 → `CycleFilterStatus` (already
    handled by the reducer's field-index dispatch).
- `src/app_input/prs.rs` — add `g`/`G` to `resolve_pr_global_key`:
  if `pr_detail` is loaded → `EnterActionsModeWithPrFilter { .. }`;
  else if a PR is selected in the list → resolve from list item;
  else → `EnterActionsMode`.
- `src/app_input/actions_orchestration.rs` — add
  `EnterModeWithPrFilter` arm to `dispatch_actions_message` (same as
  `EnterMode`: apply + list reload + workflows reload).

#### 6. UI layer
- `src/ui/components/filter_controls.rs` — add PR field to
  `actions_filter_fields` (index 2, label "pr", value `#N` or "any").
  Update `ACTIONS_FIELDS_PER_ROW` to 3.
- `src/ui/screens/actions.rs` — update `has_filters` check to include
  `pr_number.is_some()`.

#### 7. Tests
- Unit tests for `build_runs_api_path` with pr filter.
- Unit tests for `cycle_pr_filter`.
- Unit tests for `enter_actions_mode_with_pr_filter`.
- Unit tests for PR-mode `g`/`G` key → `EnterActionsModeWithPrFilter`.
- Unit tests for filter field navigation (3 fields).
- Update all `PullRequest`/`PullRequestDetail` construction sites in tests
  to include `head_sha`.
- TUI scenario for cross-mode navigation.

## TDD order (RED → GREEN → REFACTOR)
1. Write failing test: `build_runs_api_path` with pr filter → expect
   `&event=pull_request&head_sha=...`.
2. Implement: add fields + update `build_runs_api_path`. → GREEN.
3. Write failing test: `cycle_pr_filter` cycles through PRs.
4. Implement: add `cycle_pr_filter` + `Pr` field. → GREEN.
5. Write failing test: PR-mode `g`/`G` → `EnterActionsModeWithPrFilter`.
6. Implement: cross-mode action + event/message pipeline. → GREEN.
7. Write failing test: filter bar renders PR field.
8. Implement: UI updates. → GREEN.
9. Full `make ci-check` verification.
