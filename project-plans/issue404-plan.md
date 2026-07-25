# Issue #404: New repository auto-disappears when active-only mode is on

## Problem

When `hide_idle_repositories` (active-only / "v mode") is ON, creating a new
repository via `N` → New Repository form → Submit makes that repository
disappear immediately because a freshly created repository has no agents, so
`has_visible_agent_in_repository` returns false and the repo is filtered out.

Issue #116 introduced `sticky_dead_agent_ids` so that killing the selected
agent keeps it (and its repo) visible until the user navigates away. Issue
#404 asks for the same "sticky until navigation" treatment for a newly
created repository that has no agents.

## Desired Outcome

- A newly created repository stays visible in the dashboard repo list even
  when `hide_idle_repositories` is ON, so the user lands on it after the
  New Repository form closes.
- The stickiness is cleared by any selection-changing navigation (exactly
  the same set of messages that clears `sticky_dead_agent_ids`), after
  which normal active-only filtering resumes.
- A repository created while active-only is OFF is also recorded as sticky
  (mirroring kill behavior): toggling the filter is a display change, not
  navigation, so it must not clear the sticky set.

## Acceptance Matrix

| # | Actor / path | Input / boundary | Observable success | Observable failure | Persistence | Test |
|---|---|---|---|---|---|---|
| A1 | New Repository form submit, active-only ON | `hide_idle_repositories=true`; submit valid form for repo `r2` with no agents | `r2` appears in `visible_repository_indices()` and is the selected repo after submit | — | runtime-only, not persisted | `new_repository_stays_visible_when_active_only_on` |
| A2 | New repo sticky cleared by navigation | A1 state, then `NavigateDown` across repos | after navigation `r2` is filtered out (no running agents, no sticky) | — | — | `navigate_after_new_repo_filters_empty_repo` |
| A3 | New repo sticky survives filter toggle | create repo while filter OFF, then toggle filter ON | repo remains visible after toggle (toggle is not navigation) | — | — | `new_repo_with_filter_off_then_toggle_on_keeps_sticky` |
| A4 | New repo sticky cleared by `SelectRepository` | A1 state, then `SelectRepository(same_index)` | sticky cleared, empty repo filtered out | — | — | `sticky_cleared_on_select_repository_after_new_repo` |
| A5 | Multiple new repos all sticky | create two empty repos in succession | both remain visible until navigation | — | — | `multiple_new_repos_all_sticky` |
| A6 | Existing repo selection unchanged | active-only ON, select a repo with running agents, navigate | existing sticky-dead-agent behavior untouched | — | — | covered by existing #116 tests (regression) |
| A7 | Sticky is runtime-only | save/load round-trip | `sticky_empty_repository_ids` not persisted | — | must NOT appear in persisted DTO | `sticky_empty_repository_ids_not_persisted` |

## Non-Goals

- Persisting the sticky state across restarts (it is runtime-only, like
  `sticky_dead_agent_ids`).
- Changing which messages count as navigation (reuse the existing
  `prepare_ui_navigation` selection-change set).
- Altering the New Repository form validation or creation logic.
- Adding agents automatically to a new repository.
- Changing behavior when active-only is OFF (repos are already all visible;
  the sticky set is still populated for forward-compat but has no visible
  effect until the filter is on, exactly like kill).
- Touching `sticky_dead_agent_ids` semantics.

## Vertical Slices

### Slice 1 — Sticky empty-repository visibility (single slice)

- **Acceptance rows:** A1–A7
- **Architecture owner:** `src/state` reducer + selectors (deterministic,
  no I/O). No new module; mirrors the existing `sticky_dead_agent_ids`
  field on `AppState`.
- **Allowed files:**
  - `src/state/types.rs` — add `sticky_empty_repository_ids: HashSet<RepositoryId>`
  - `src/state/mod.rs` — clear it in `prepare_ui_navigation`
  - `src/state/selectors.rs` — include it in `has_visible_agent_in_repository`
  - `src/state/form_ops.rs` — insert the new repo id on submit
  - `tests/core/visibility_filter_contracts.rs` — behavioral tests
- **RED:** behavioral tests in `visibility_filter_contracts.rs` fail.
- **GREEN:** production changes make them pass.
- **REFACTOR:** keep naming/semantics parallel to `sticky_dead_agent_ids`.
- **Verification:** `make quick-check`, then `make ci-check`.

## Scope Ledger

| Item | Classification | Notes |
|---|---|---|
| `sticky_empty_repository_ids` field + selectors + submit hook + nav clear | In-scope | core fix |
| Behavioral tests (A1–A7) | In-scope | TDD coverage |
| Plan doc (this file) | In-scope | process artifact |

No newly discovered work. No dependency, workflow, agent-memory, or
quality-tool changes. No new public abstraction (mirrors an existing
private field). No unrelated refactor.

## Review Counters

- Local OCR runs before PR: 0 / 2
- OCR runs after PR opened: 1 / 2 (result: no findings)

## Verification Evidence

- `cargo test --test integration core::visibility_filter_contracts` → 20 passed
  (14 existing + 6 new A1–A6 behavioral tests).
- `cargo test --lib state::` → 762 passed.
- `cargo test --test integration` → 351 passed.
- `cargo fmt --all --check` → clean.
- `cargo build --workspace` → clean.
- Source-size gate: all touched files within limits (`mod.rs` at 1000, the
  hard-limit boundary `> 1000`).
- Pre-existing on main (not caused by this change): 4 `manual_is_multiple_of`
  clippy errors in `src/harness/v1/validate.rs` (newer clippy/rustc), and
  flaky real-process harness tests (`harness_v1::wait_timeout_escalates...`,
  `tmux_driver::real_jefe_session_uses_isolated_config...`) that pass in
  isolation but fail under heavy concurrent llvm-cov load.
