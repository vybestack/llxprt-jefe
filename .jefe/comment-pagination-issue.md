Follow-up to the list-pagination unification (#202, PR #236). PR #236 unified the three top-level lists (Actions, Issues, PRs) onto the shared `PaginatedList<T, I>` state container and `ListLoader` lifecycle. The comment-pagination path on the Issue and PR **detail** screens was intentionally left out because it intersects the persisted domain model and needs a careful design pass.

## Problem

Issue and PR comment pagination is implemented ad-hoc per screen with parallel, structurally-identical machinery:

- `IssueCommentsPagePending` (`src/state/issues_types.rs`) and `PrCommentsPagePending` (`src/state/pr_types.rs`) are identical except the field name (`issue_number` vs `pr_number`, both `u64`).
- Each screen has its own `comments_page_pending: Option<...>`, `next_comments_page_request_id: u64` counter, `loading.comments: bool` flag, and `*_comments_page_pending_matches(...)` staleness guard.
- The comment data itself (`has_more_comments: bool`, `comments_cursor: Option<String>`) lives directly on the persisted domain types (`IssueDetail` in `src/domain/issues.rs`, `PullRequest` in `src/domain/mod.rs`).

This is exactly the per-screen divergence #202 set out to eliminate.

## Why it was split out

Unlike the top-level lists (where the state was already a free-standing container), comment state is embedded in the persisted domain types. Unifying it touches ~25 files:

- `has_more_comments` / `comments_cursor` are constructed in ~30 sites across tests, fixtures, the github parse layer (`src/github/parse.rs`, `parse_pr.rs`, `mod.rs`), UI render (`pr_detail_content.rs`, `issue_detail_content.rs`), and selection (`src/selection/content.rs`).
- Changing the domain-type shape is a serialization/migration concern (`IssueDetail` / `PullRequest` are persisted).
- `PaginatedList` models reload/page/silent-refresh; comments are append-only with no reload semantics, so the fit must be validated (it may warrant a thinner append-only variant).

Doing this blind in the same PR as the core unification would balloon scope and risk serialization + render regressions.

## Proposal

Unify Issue and PR comment pagination onto a shared abstraction. Decide up front between:

- **Full unification onto `PaginatedList<Comment, CommentIdentity>`** stored on the detail (replace `comments` vec + `comments_cursor` + `has_more_comments` + `comments_page_pending` + `next_comments_page_request_id` with one container). Cleanest end state; requires a serialization migration for the persisted detail types.
- **A thinner append-only comment list type** if `PaginatedList`'s reload/silent model is a poor fit.

Either way, the two parallel `*CommentsPagePending` structs and their staleness guards collapse to one.

## Scope

- Reconcile the `loading.comments` screen flag with the shared container's derived loading state.
- Update the github parse layer (it currently sets `comments_cursor`/`has_more_comments` directly).
- Migrate the ~30 test/fixture construction sites.
- Handle the persisted-state shape change (migration or version bump as needed).

## Related

- #202 — unified top-level list pagination (done in PR #236).
- #201 — quota evaluation (a common service is the natural home for quota protection).
