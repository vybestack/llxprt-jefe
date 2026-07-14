# Issue #273 — Unify Issue and PR comment pagination onto a shared abstraction

## Goal
Replace the parallel ad-hoc comment-pagination state on the Issue and PR detail
screens with a single shared `PaginatedList<Comment, CommentDetailIdentity>`
container, eliminating the duplicated `IssueCommentsPagePending` /
`PrCommentsPagePending` structs, their staleness guards, and the per-screen
`next_comments_page_request_id` counters.

## Key finding (de-risks the whole change)
`IssueDetail` and `PullRequestDetail` do NOT derive Serialize/Deserialize and
are NOT referenced anywhere in `src/persistence/`. `PersistedState::State`
persisted fields are: repositories, agents, selection indices, pane_focus,
user_preferences — NO detail/comment state. **There is NO serialization
migration.** Detail/comment state is transient (rebuilt on screen entry). The
refactor is mechanical, not a persistence change.

## Current shape (the duplication)
Per screen, on the `*Detail` domain struct (transient):
- `comments: Vec<IssueComment>`
- `has_more_comments: bool`
- `comments_cursor: Option<String>`

Per screen, on the `IssuesState`/`PrsState`:
- `comments_page_pending: Option<{scope_repo_id, number, cursor, request_id}>`
  (two identical structs, differing only `issue_number` vs `pr_number`)
- `next_comments_page_request_id: u64`
- `loading.comments: bool`
- `*_comments_page_pending_matches(...)` staleness guard

Load lifecycle (identical on both screens):
1. detail-loaded seeds `comments` + `comments_cursor` + `has_more_comments`
2. dispatch `pr_comment_page_params`/`issue_comment_page_params` checks
   `!has_more_comments || loading.comments` → builds params from
   `detail.comments_cursor`
3. `mark_comments_page_loading_with_request_id` sets pending + loading flag
4. `apply_*_comments_page_loaded` extends vec + replaces cursor/has_more +
   clears pending, gated by the staleness match

## Target shape
A `PaginatedList<IssueComment, CommentDetailIdentity>` stored on the detail
struct (IssueDetail.comments, PullRequestDetail.comments), where:

- `items` ← the comments vec
- `next_page` ← Cursor(cursor) when has_more, Done otherwise (derived; no
  separate has_more_comments/comments_cursor fields)
- `pending` ← replaces comments_page_pending (a PendingLoad::Page)
- `last_request_id` ← replaces next_comments_page_request_id

New value type `CommentDetailIdentity { scope_repo_id: RepositoryId, number: u64 }`
in `src/domain/pagination.rs` (or a comment-specific module). Both screens use
it (issue_number / pr_number are both just `u64` — the identity is generic
over which detail type).

### loading.comments reconciliation
`loading.comments` is a screen-level flag shared with detail loading and is
set/cleared across many sites. Decision: KEEP `loading.comments` as the
screen-level visible-loading flag (it gates the spinner independently of the
container's `is_loading()`, and is wired into nav/reset paths). The
PaginatedList's pending marker is the staleness/request correlation; the flag
drives the spinner. They move together in the comment load/accept paths.

## New shared state-layer ops (in pagination.rs or a new comment_load_ops)
Mirror the list pattern but for the embedded comment list:
- `begin_comment_page(detail, identity, cursor_token, request_id)` →
  `list.begin_page(token, request_id)`
- `accept_comment_page(detail, identity, request_id, comments, next_cursor, has_more)`
  → staleness via `LoadCorrelation::Page`, then `list.accept_page(...)`
- `accept_comment_page_failure(detail, correlation)` → `list.accept_failure`
The screen-specific load_ops delegate to these.

## Scope of changes (mechanical)
1. `src/domain/pagination.rs` (or new): add `CommentDetailIdentity`.
2. `src/domain/issues.rs`, `src/domain/mod.rs`: change `IssueDetail.comments`
   and `PullRequestDetail.comments` to `PaginatedList<IssueComment,
   CommentDetailIdentity>`. Remove `has_more_comments` + `comments_cursor`
   fields (now derived from next_page).
3. `src/state/issues_types.rs`, `src/state/pr_types.rs`: remove
   `comments_page_pending`, `next_comments_page_request_id` (move into the
   container on the detail).
4. `src/state/issues_load_ops.rs`, `src/state/prs_load_ops.rs`: rewrite
   comment load/accept/match to use PaginatedList ops. Delete the two
   `*CommentsPagePending` structs + their match guards.
5. `src/app_input/prs_comments_dispatch.rs`, `src/app_input/issues_dispatch.rs`:
   update param gathering to read `detail.comments.next_page()` /
   `has_more()` instead of `comments_cursor`/`has_more_comments`; update
   pending marking.
6. `src/github/parse.rs`, `parse_pr.rs`, `mod.rs`: seed the detail with a
   `PaginatedList` (initial items + next_page from cursor/has_more) instead of
   setting the three fields directly.
7. UI/render + selection readers (`pr_detail_content.rs`,
   `issue_detail_content.rs`, `selection/content.rs`): read
   `detail.comments.items()` / `.has_more()` instead of the removed fields.
8. Update ~30 test/fixture construction sites (mechanical field replacement).

## TDD plan (RED first)
- Unit: `PaginatedList` comment-page accept appends + advances next_page
  (already covered by generic tests; add a CommentDetailIdentity-specific
  accept/stale test).
- State (issues): seeding detail with comments list + next_page; begin comment
  page; accept matching page appends + advances cursor; stale page rejected;
  failure restores continuation (already proven by BUG1 fix).
- State (prs): mirror.
- Integration (prs_integration_tests, issues_tests_detail_flow): the existing
  `comments_page_pending` assertions become `detail.comments.has_pending_request()`
  assertions; comment append + has_more semantics preserved.

## Verification (run each gate individually; combined make ci-check SIGTERMs during coverage)
1. cargo fmt --all --check
2. bash scripts/check-source-file-size.sh
3. bash scripts/check-architecture.sh
4. bash scripts/check-clippy-allows.sh
5. CLIPPY_CONF_DIR=.github/clippy rustup run stable cargo clippy --workspace --all-targets --all-features -- -D warnings
6. rustup run stable cargo clippy --workspace --all-targets --all-features -- -A clippy::all -A clippy::pedantic -A clippy::nursery -D clippy::cognitive_complexity -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::type_complexity -D clippy::struct_excessive_bools
7. rustup run stable cargo llvm-cov --workspace --all-features --summary-only --fail-under-lines 30
8. cargo build --workspace --all-features --locked
9. cargo test --workspace --all-features --locked
Note: `guarded_real_jefe_sticky_kill_scenario` is a flaky tmux test under load;
re-run isolated if it fails (unrelated).
