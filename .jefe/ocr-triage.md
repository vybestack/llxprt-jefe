## OCR Findings Triage — PR #236 (head 790d224)

Evaluated all 29 inline findings + 1 summary finding against the actual source. Summary by category:

### Invalid — reference fields/code that no longer exist (16 findings)
These findings ask to restore or check fields removed by the PaginatedList migration. List loading is now **derived** from `PaginatedList::is_loading()`/`has_pending_request()`; the old `loading.list`, `list_reload_pending`, `list_page_pending`, and `list_cursor` fields are gone by design.

- `actions_orchestration.rs:274` — asks to restore `actions_state.loading.list = false`. `ActionsLoadingState` now has only `detail`; list loading is derived. The no-repo path is hit before any reload starts (owner/repo empty check), so no pending exists to clear. No stuck spinner possible.
- `actions_load_tests.rs:324`, `:374` — ask to restore `list_reload_pending = None`. That field is gone; the tests verify reload via `list_pending()` assertions (which IS the equivalent check). Tests are correct.
- `issues_tests_detail_flow.rs:387` — removed `list_cursor.is_none()` assertion. `list_cursor` is gone; `has_more_issues()` (asserted false) implies the continuation is `Done`/None. Equivalent coverage preserved.
- `pagination.rs:437`, `:464` — claim duplicated matching logic / an `accept_failure_proxy`. **Factual error**: a single private `pending_matches` helper (pagination.rs:450) is the sole source of truth; both `accept_failure` (430) and `is_stale` (441) delegate to it. No duplication exists.
- `app_input/mod.rs:917` — asks to restore `loading.list = true` in `reset_issue_list_for_repo_change`. Field is gone; loading is derived from the pending reload that the dispatch layer begins (as the reducer test comments document).

### Invalid — misread correct tests / architecture (6 findings)
- `issues_tests_detail_flow.rs:385` — asks the pure-reducer test to verify the full SelectRepository→reload→loaded chain. Per `dev-docs/project-standards.md`, the reducer is pure and the dispatch layer owns I/O; the test correctly asserts the reducer clears stale state and documents that dispatch begins the reload (covered in app_input integration tests). Correct module-boundary separation.
- `issues_tests_filter.rs:181`, `:6` — claim reload verification was removed. The tests assert `list_loading()`/`list_pending()` after filter/search apply, which verifies reload is triggered.
- `actions_load_tests.rs:355` — removed explanatory comments. Minor; the test names and assertions remain self-documenting. Not restoring prose.
- `prs_tests_detail_flow.rs:212`, `prs_tests_pagination.rs:25` — style nits on variable shadowing/unused binding. No functional impact; not changing test style in this PR.

### Out of scope — proposed API expansion (5 findings)
- `pagination.rs:120` — remove `Eq` derive. `Eq` is intentional (correlation by equality); all item/identity types are `Eq`. Keeping it.
- `pagination.rs:308` — add `Superseded` variant to `BeginOutcome`. Callers correlate via request-id (the design's contract). Not expanding the API.
- `pagination.rs:324` — add `NotInitialized` variant. Same reasoning; `TokenMismatch` is sufficient.
- `pagination.rs:391` — rename/split `AcceptOutcome::Empty`. Callers treat `Empty`/`Applied` identically by design; documented behavior. Not changing.
- `issues_ops.rs:252`, `:263` — extract a shared "page-size" constant for the `10` in page-up/down. The `10` is a **viewport jump** (visible rows), not the async fetch batch size (30); they are independent concerns. The `10` predates this PR. Out of scope and the premise is incorrect.

### Low-priority cleanliness, no functional bug (4 findings)
- `prs_list_dispatch.rs:180`, `:85` — request-id allocated before `begin_page` outcome is known; a not-started page load "wastes" a monotonic id. This is **safe by design**: the id counter is monotonic and never recycles (recycling would be the real bug). A wasted id merely advances the counter; u64 exhaustion requires ~10^19 events. No fix — the alternative (rolling back the counter) would violate the monotonic-no-recycle invariant.
- `actions_load_ops.rs:137`, `:143` — `begin_actions_reload` silently returns on `RequestIdExhausted`. Same astronomical-improbability reasoning; the old `saturating_add` silently stopped at MAX too. No user-facing improvement available for an impossible condition without over-engineering error plumbing.
- `actions_load_ops.rs:49`, `:92` — `bool` return values always `true` / stale errors dropped. The `bool` is a reducer-handler convention (handled=true); stale errors are intentionally dropped (a superseded request's error must not clobber the current view). Correct by design.

### Conclusion
No High or Medium-severity functional issues found. All findings are invalid, out-of-scope API proposals, or low-priority cleanliness with no safe simple fix. No code changes warranted. CI is green (Lint/Build/Test/Coverage/Format/Clippy/Complexity/Size/Arch all pass); 2434 tests pass; coverage 72.68%.
