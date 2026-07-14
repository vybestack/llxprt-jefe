Closes #202.

## Summary

Issue #202 asked for one common list-loading lifecycle shared by every screen, because Actions, Issues, and PRs each re-implemented the same idea (fetch a page, track has_more/continuation/pending, reject stale responses by request id, manage loading flags) with diverging behavior.

This PR delivers that for all three screens in a single change:

- A generic, deterministic `PaginatedList<T, I>` state container (`src/state/pagination.rs`) plus `PageToken` / `ListRequestId` value types (`src/domain/pagination.rs`).
- `PageToken { Cursor, PageNumber, Done }` unifies GraphQL-cursor (Issues/PRs) and REST-page (Actions) pagination. `has_more` is derived from the token, never stored, so contradictory state is impossible.
- Exactly one pending operation at a time (`PendingLoad<I>` enum), with stale rejection by kind + identity + request id + requested token living in one place.
- Zero bool fields on the container; loading visibility is derived from the pending kind + `ReloadVisibility`.
- Removed the legacy `request_id == 0` special-case.

### Per-screen migration
- **Actions** (e4377b8): `ActionsState.list: PaginatedList<WorkflowRun, ActionsListIdentity>`. Actions gains a working load-more for the first time.
- **Issues**: `IssuesState.list: PaginatedList<Issue, IssueListIdentity>`. Reload-replace + empty-detail-clear behavior preserved.
- **PRs**: `PullRequestsState.list: PaginatedList<PullRequest, PrListIdentity>`. By-number selection follow + scroll clamp on silent refresh (issue #128) preserved.

### Staleness/safety hardening (review-driven)
- PR page results validate the event scope against the list's stored identity scope before appending (a wrong-repo response is rejected).
- PR list failures try both reload and page correlations so a page failure clears the pending marker instead of leaving the list permanently loading.
- PR reload/silent-refresh failures validate the event scope before cancelling the current pending request.
- Dispatch adapters stop spawning GitHub I/O when `begin_page` returns `Busy`/`Exhausted`/`TokenMismatch`, and surface no request when the request-id space is exhausted (id 0 stays reserved as "no ids allocated yet").

## Verification
- cargo fmt --check
- CLIPPY_CONF_DIR=.github/clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
- complexity gate (cognitive_complexity, too_many_lines, too_many_arguments, type_complexity, struct_excessive_bools)
- scripts/check-source-file-size.sh, check-architecture.sh, check-clippy-allows.sh
- cargo build --workspace --all-features --locked
- cargo test --workspace --all-features --locked (2130 tests pass)
- cargo llvm-cov --fail-under-lines 30 (71.11%)
- Open Code Review (ocr) + rustreviewer subagent review completed; all High/Medium findings addressed.

## Notes
- One known pre-existing flaky test (`guarded_real_jefe_sticky_kill_scenario`, tmux harness) can fail under parallel coverage load but passes in isolation; it is unrelated to this change.
