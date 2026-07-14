# GitHub Issue #202: Unify list pagination and lazy data-loading behind a common service/trait

**Repository:** jefe
**State:** open
**Labels:** enhancement

## Body

Follow-up to the Actions integration (#21, PR #132).

## Problem
List pagination and data loading is implemented ad-hoc per screen, and the behaviors diverge:

- **Issues mode** has on-demand pagination (PageUp/PageDown navigate pages via `list_page_pending`, `navigate_issue_list_page_up/down`).
- **PRs mode** has its own pagination state and comment-pagination path.
- **Actions mode** models pagination (`page`, `has_more`) but never wires a load-more path — it eagerly loads page 1 and the detail, with no way to fetch the next page. The user observed Actions scrolls/paginate eagerly and inconsistently compared to Issues/PRs.

Each mode re-implements the same idea: fetch a page of items from a GitHub data source, track `has_more` / current page / pending request, stale-response rejection by request_id, and loading flags. The data shape differs (issues vs PRs vs workflow runs) but the loading lifecycle is identical.

Agents and Repositories screens will likely need the same pattern (lazy load, paginate, refresh) for their data sources too, even though those sources differ from `gh`.

## Proposal
Introduce a common control/service/trait that owns the list-loading lifecycle shared by all screens:

- A generic `PaginatedList<T>` (or trait) that tracks: current page, `has_more`, loading flags, pending request with `request_id`, and stale-response rejection.
- A common `ListLoader` trait or service abstraction: `load_page(repo, filter, page) -> Result<Page<T>>`, `load_more()`, `reload()` — each screen implements the data-fetch (the `gh` call + parse) and the trait owns the state machine (pending tracking, request-id correlation, loading transitions).
- On-demand/lazy pagination as the default (load page 1 on entry, fetch next page on demand when the user scrolls past the end), consistent across Actions / Issues / PRs.
- Quota/caching hooks (TTL, dedup, coalescing) can live here too — see the related quota-evaluation issue (#201).

## Scope
- Design the trait/service (decide: trait object vs generic vs macro; where it lives in the layering — likely the state/runtime boundary since it bridges async `gh` fetches and the reducer).
- Migrate Actions to it first (it currently has no working load-more), then Issues, then PRs.
- Keep Agents/Repositories in mind as future consumers (different data source, same lifecycle).

## Related
- #201 (quota evaluation) — a common service is the natural home for quota protection.
- #194 (job inspection) — unrelated, but part of the Actions follow-ups.
