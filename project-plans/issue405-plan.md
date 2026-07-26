# Issue #405: Add a search mode for agents and repositories

## Problem

Power users accumulate many repositories and agents. The dashboard sidebar
(repo list) and the middle agent pane have no text search; the only list
filtering today is `hide_idle_repositories` (`v` active-only). There is no way
to quickly narrow the repo sidebar or agent list by typing a name fragment.
Additionally, when a filter is in effect there is no on-screen indication that
the visible list is filtered, so a user can stare at a short list without
realizing it is filtered.

The issue asks for a "filter but lite" search (a single text field, not the
structured multi-field filter form used by Issues/PRs) for **both** repos and
agents, plus a clear visual signal that a filtered view is active.

## Design decision

Mirror the established Issues/PRs/Actions "search lite" pattern: a focused
single-line text input (`search_input_focused`) backed by a persistent query
string, live-filtering the visible lists as the user types. This is exactly
the "filter lite" the issue describes and keeps the architecture consistent
(each screen owns its search state).

Key choices:

- **Reuse `/` as the entry key** in Dashboard normal mode. Today `/` opens the
  shared `ModalState::Search` (which is display-only on the dashboard and only
  meaningfully used by SplitScreen). For the Dashboard, `/` is repurposed to
  focus a dedicated **dashboard search input**. SplitScreen's `/`/modal path is
  untouched (different `ScreenMode`, different key resolver).
- **Persistent query, live filtering.** `dashboard_search_query` persists after
  the input blurs so the filtered view remains until explicitly cleared — the
  same model Issues/PRs use. This is why a dedicated field is used instead of
  the modal (the modal clears its query on `CloseModal`).
- **Two independent filters, one query.** The query filters the repo sidebar
  (by repo `name`) AND the agent pane (by agent `name`) independently. A repo
  need not match the query for its agents to be filtered — the two lists are
  filtered separately against the same query.
- **Case-insensitive substring** on the trimmed query against the displayed
  `name` field. Empty/blank query = no search filtering.
- **AND-composed with active-only.** Search is an additional predicate on top
  of `hide_idle_repositories`. A repo is visible iff active-only passes AND the
  search predicate passes; same for agents.
- **Runtime-only (not persisted).** A stale search filter on startup would be
  confusing and contradicts "make it obvious you are filtered." Like the sticky
  sets, the query is runtime-only. (Contrast: `hide_idle_repositories` is a
  stable preference and persists.)
- **Filtered-view indicator.** The dashboard shows an on-screen indicator
  whenever ANY dashboard filter reduces the visible set — both the new search
  query and the existing active-only mode. This directly addresses the issue's
  "make it obvious you're looking at a filtered view" for the dashboard. A
  general indicator for Issues/PRs/Actions filter modes is recorded as a
  follow-up (out of scope).

## Acceptance Matrix

| # | Actor / path | Input / boundary | Observable success | Observable failure | Persistence | Test |
|---|---|---|---|---|---|---|
| A1 | Dashboard `/` focuses search | `ScreenMode::Dashboard`, press `/` | `dashboard_search_input_focused == true` | — | runtime-only | `slash_focuses_dashboard_search` |
| A2 | Typing filters repos live | 3 repos `[alpha, beta, gamma]`, focus search, type `al` | `visible_repository_indices()` returns only the `alpha` repo | — | runtime-only | `search_filters_repositories_by_name` |
| A3 | Repo match is case-insensitive | repos `[Alpha, BETA]`, query `eT` | both `Alpha` and `BETA` visible | — | runtime-only | `repo_search_is_case_insensitive` |
| A4 | Empty query disables search filter | A2 state, clear query to empty | all 3 repos visible again | — | runtime-only | `empty_query_disables_search_filter` |
| A5 | Search AND-composes with active-only | active-only ON, query matches only an idle repo | matched idle repo NOT visible (active-only wins); query matching a repo with running agents IS visible | — | runtime-only | `search_composes_with_active_only` |
| A6 | Search filters agents in selected repo | selected repo has agents `[zig, cargo, rustc]`, query `ru` | `visible_agents_for_repository()` returns only `rustc` | — | runtime-only | `search_filters_agents_by_name` |
| A7 | Backspace pops last char | query `ab`, focused, Backspace | query becomes `a`, lists refilter | — | runtime-only | `backspace_pops_search_char` |
| A8 | Esc clears non-empty query then blurs | query `ab`, focused, Esc | query cleared, input blurred, all lists restored | — | runtime-only | `esc_clears_nonempty_search` |
| A9 | Esc on empty query blurs only | query empty, focused, Esc | input blurred, query stays empty, no crash | — | runtime-only | `esc_on_empty_query_blurs_only` |
| A10 | Enter blurs, keeps query (filter persists) | query `ab`, focused, Enter | input blurred, query `ab` retained, lists stay filtered | — | runtime-only | `enter_blurs_keeps_query` |
| A11 | Selection normalizes when filter shrinks set | selected repo index points to a now-filtered-out repo, query applied | selected repo clamps to a visible repo (no panic, no dangling index) | — | runtime-only | `selection_normalizes_when_filtered` |
| A12 | `/` is dashboard-only (split unaffected) | `ScreenMode::Split`, press `/` | split's existing `OpenSearch`/modal path unchanged | — | — | regression (existing split tests) |
| A13 | Filtered-view indicator shown when search active | query non-empty | dashboard renders an indicator naming the active query | — | runtime-only | `search_active_renders_indicator` |
| A14 | Filtered-view indicator shown when active-only on | `hide_idle_repositories == true`, search empty | dashboard renders an active-only indicator | — | runtime-only | `active_only_renders_indicator` |
| A15 | No indicator when unfiltered | search empty AND active-only off | no filter indicator rendered | — | runtime-only | `unfiltered_renders_no_indicator` |
| A16 | Dashboard query not persisted | set query, save/load round-trip | `dashboard_search_query` absent from persisted DTO | must NOT persist | round-trip test | `dashboard_search_query_not_persisted` |

## Non-Goals

- Persisting the dashboard search query across restarts (runtime-only, like the
  sticky sets).
- Changing which messages count as navigation (reuse existing normalization).
- Altering SplitScreen's `/`/`ModalState::Search` path (different screen mode).
- Multi-field / structured filter form for repos or agents (the issue asks for
  "lite" single-text search).
- Server-side / GitHub search — this is purely client-side name filtering of
  already-loaded repos and agents.
- Matching on fields other than `name` (slug, github_repo, description) — keep
  the predicate minimal and predictable; expand later if requested.
- A general filtered-view indicator for Issues/PRs/Actions modes — recorded as
  a follow-up. This issue delivers the dashboard indicator only.
- Fuzzy matching, regex, or saved searches.

## Vertical Slices

### Slice 1 — Dashboard search: selectors + state + key routing + render (single slice)

- **Acceptance rows:** A1–A16
- **Architecture owner:** `src/state` reducer + selectors (deterministic, no
  I/O) for the filtering predicate; `src/app_input` for key routing;
  `src/ui` for the input line + indicator (pure projection → thin component).
- **Allowed files:**
  - `src/state/types.rs` — add `dashboard_search_query: String` and
    `dashboard_search_input_focused: bool` (runtime-only, NOT in persisted DTO)
  - `src/state/events.rs` — add `FocusDashboardSearch`, `BlurDashboardSearch`,
    `SetDashboardSearchQuery { query }`, `ClearDashboardSearch`
  - `src/messages.rs` (+ `event_conversion.rs` / `names.rs`) — corresponding
    `UiNavigationMessage` variants + conversion + stable names
  - `src/state/selectors.rs` — AND the search predicate into
    `visible_repository_indices()` and the agent visibility path
    (`is_agent_visible_with_idle_filter` / agent selectors); add pure helpers
    `dashboard_search_matches_repo` / `dashboard_search_matches_agent`
  - `src/state/mod.rs` (or a small new `src/state/dashboard_search_ops.rs` if
    `mod.rs` would exceed the size limit) — reducer handlers for the 4 events,
    including selection normalization after a query change
  - `src/app_input/normal.rs` — repurpose dashboard `/` to
    `FocusDashboardSearch`; add a focused-search key resolver (chars →
    `SetDashboardSearchQuery`, Backspace → pop, Enter → blur, Esc →
    clear/blur). NB `normal.rs` is at 980 lines (hard limit 1000) — if the new
    resolver would push it over, extract it into a sibling module
    `src/app_input/dashboard_search.rs` rather than loosening the limit.
  - `src/input.rs` — `InputMode::DashboardSearch` variant +
    `input_mode_for_state` mapping when the dashboard search input is focused
  - `src/ui/screens/dashboard.rs` — render the search input line (when focused
    or query non-empty) and pass filter-active flags down
  - `src/ui/components/status_bar.rs` (or sidebar header) — filtered-view
    indicator covering search + active-only
  - `src/ui/components/keybind_bar.rs` — add `/ search` to the Dashboard hint
  - `tests/core/dashboard_search_contracts.rs` — behavioral tests (A1–A16)
- **RED:** behavioral tests in `dashboard_search_contracts.rs` fail.
- **GREEN:** production changes make them pass.
- **REFACTOR:** keep the search predicate pure and `#[must_use]`; keep key
  routing parallel to the Issues/PRs search-input resolver.
- **Verification:** `make quick-check`, then `make ci-check`.

## Scope Ledger

| Item | Classification | Notes |
|---|---|---|
| `dashboard_search_query` + `dashboard_search_input_focused` fields | In-scope | core state |
| 4 events + messages + conversion + names | In-scope | typed pipeline |
| Selector predicates (repo + agent) | In-scope | filtering |
| Reducer handlers + selection normalization | In-scope | state transitions |
| Dashboard `/` repurpose + focused-input key resolver | In-scope | key routing |
| `InputMode::DashboardSearch` | In-scope | input mode |
| Search input render + filtered-view indicator | In-scope | UX clarity |
| Keybind hint `/ search` | In-scope | discoverability |
| Behavioral tests A1–A16 | In-scope | TDD coverage |
| Plan doc (this file) | In-scope | process artifact |
| General filter indicator for Issues/PRs/Actions | Defer | follow-up issue |
| Persisting dashboard query | Reject (non-goal) | runtime-only |
| SplitScreen `/`/modal | Reject (out of scope) | untouched |

No newly discovered work yet. No dependency, workflow, agent-memory, or
quality-tool changes. No new public abstraction beyond typed events/messages
that mirror the existing Issues/PRs search pattern. No unrelated refactor.

## Review Counters

- Local OCR runs before PR: 0 / 2
- OCR runs after PR opened: 0 / 2

## Verification Evidence

(to be filled during implementation)
