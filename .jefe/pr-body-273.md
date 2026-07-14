## Summary

Eliminates the duplicated ad-hoc comment-pagination machinery on the Issue and PR detail screens, completing the unification started in #202 (PR #236).

Both `IssueDetail` and `PullRequestDetail` now store a shared `PaginatedList<IssueComment, CommentDetailIdentity>` instead of the parallel `(comments vec + has_more_comments + comments_cursor)` triple, and the per-screen `comments_page_pending` / `next_comments_page_request_id` state is gone — the container owns pending tracking + request-id correlation.

## What changed

- **New `CommentDetailIdentity { scope_repo_id, number }`** in `src/domain/pagination.rs` — one struct for both screens (issue_number / pr_number are both `u64`).
- **`IssueDetail.comments` and `PullRequestDetail.comments`** are now `PaginatedList<IssueComment, CommentDetailIdentity>`; `has_more_comments` and `comments_cursor` fields removed (derived from `next_page`).
- **Deleted** `IssueCommentsPagePending`, `PrCommentsPagePending`, and their `*_comments_page_pending_matches` staleness guards.
- **Comment load/accept/failure** now use `PaginatedList::begin_page` / `accept_page` / `accept_failure` with `LoadCorrelation::Page`.
- **`PaginatedList` moved** to `src/domain/paginated_list.rs` (with `src/state/pagination.rs` as a re-export shim) so the domain detail types can own it without reversing the architecture DAG (domain must not depend on state).
- **`loading.comments` retained** as the screen-level spinner flag (wired into nav/reset paths, independent of the container's `is_loading()`).
- GitHub parse layer seeds details with a settled `PaginatedList`.

## Why no serialization migration

`IssueDetail` and `PullRequestDetail` are transient runtime state — they do not derive `Serialize`/`Deserialize` and are not referenced anywhere in `src/persistence/`. `PersistedState::State` persists only repositories, agents, selection indices, and preferences. No migration needed.

## Behavior preserved

- The **BUG1 invariant holds for comments**: a failed comment page load leaves `next_page` intact so the user can retry load-more (guarded by `failed_comment_page_preserves_continuation_for_retry`). `next_page` only changes on a successful `accept_page`, never on `begin_page`.
- Stale comment pages (wrong request_id or wrong token) are rejected via `LoadCorrelation::Page`.
- Comment append + has-more semantics unchanged.

## Verification

All 9 gates pass: `cargo fmt --all --check`, `check-source-file-size.sh`, `check-architecture.sh`, `check-clippy-allows.sh`, strict clippy (`-D warnings`), complexity clippy gate, coverage (72.57%), build, and tests (**2560 passed, 0 failed**).

Closes #273.
