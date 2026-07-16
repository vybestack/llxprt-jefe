# Issue #208: Actions runs newest-first

## Goal

Workflow runs in Actions mode must appear reverse-chronological by
`created_at` (newest first), independent of GitHub API / pagination order.

## Acceptance matrix

| ID | Actor / path | Input / boundary | Observable success | Observable failure | Evidence |
| --- | --- | --- | --- | --- | --- |
| A1 | Actions list reload | API returns runs out of chronological order | List shows newest `created_at` first; selection lands on newest | N/A (pure reorder) | Reducer `runs_loaded_sorts_newest_created_at_first_and_empty_timestamps_last`; TUI `actions-mode.json` |
| A2 | Actions load-more | Page 2 contains a run chronologically between page-1 runs | After append, full list is still newest-first; selection follows run id | Stale page ignored (existing) | Reducer `runs_page_loaded_resorts_interleaved_appends_and_preserves_selection`; discriminating TUI load-more navigation in `actions-mode.json` |
| A3 | Equal timestamps | Two runs share `created_at` | Deterministic order by `id` descending | N/A | Pure + reducer equal-timestamp tests |
| A4 | Missing timestamps | Empty `created_at` | Empty timestamps sort after dated runs; id tie-break among empties | N/A | Reducer `runs_loaded_sorts_newest_created_at_first_and_empty_timestamps_last` |

## Non-goals

- Changing GitHub API query parameters or `per_page`.
- Sorting by `updated_at` or conclusion.
- Re-sorting inside the pure viewport projection on every paint.
- UI chrome / filter / job-detail behavior beyond run-list order.

## Architecture

The Actions state owner sorts at commit time in its load reducers (`RunsLoaded`
/ `RunsPageLoaded`) via a private comparator and
`resort_actions_runs_preserving_selection`. Projection stays order-preserving,
so `state` does not depend on view logic. In-place crate-private
`PaginatedList::sort_by` avoids cloning on page append without expanding the
public API.

## Vertical slices

1. **State-owned comparator + behavioral tests** — already landed.
2. **Reducer reload + page-append sort with selection-by-id** — already landed.
3. **TUI scenario** — multi-run fixture returned oldest-first; assert newest title
   is selected first; navigate to end to trigger page-2; assert post-append order.
4. **Plan / ledger + main integration + exact-head gates** — this document.

## Scope ledger

| Change | Disposition | Maps to |
| --- | --- | --- |
| `src/actions_view.rs` order-preserving projection | In scope | A1–A4 display contract |
| `src/state/actions_load_ops.rs` private comparator and sort on load/page | In scope | A1–A4 |
| `src/domain/paginated_list.rs` crate-private `sort_by` | In scope | A2 (in-place) |
| `tests/issue208_behavior.rs` / paginated-list tests | In scope | A1–A4 |
| `src/state/actions_tests.rs` extraction | In-scope maintainability | Keep touched source at 823 lines and reducer coverage focused |
| `scripts/issue194-gh-shim.sh` multi-run + interleaved page-2 fixture | In scope | A1–A2 TUI |
| `dev-docs/tmux-scenarios/actions-mode.json` order asserts | In scope | A1–A2 TUI |
| `scripts/issue194-run-scenario.sh` audit expectations | In scope | A1–A2 TUI |
| `project-plans/issue208-plan.md` | In scope | delivery ledger |
| Merge `vybestack/main` | Required readiness | mainline drift |

## Review counters

- CodeRabbit: allocation concern fixed earlier; exact-head re-review still required after this push.
- OCR: two post-PR runs completed (cap reached); valid helper-identity finding fixed, fixture-field findings rejected as contradicted by the serde parser.
- LLxprt Code review (`4709293462`): blockers addressed by this remediation commit set.

## Process deviation

The contributor committed production and reducer behavior before adding the
required TUI scenario. The mutation-based RED evidence below demonstrates that
the scenario detects both initial-order and load-more regressions, but it does
not satisfy scenario-first TDD retroactively. History was not rewritten. Merge
readiness therefore requires an explicit maintainer decision accepting this
recorded process deviation for this contribution.

## TUI regression RED → GREEN (slice 3)

Do not rewrite prior implementation history. For the TUI ordering slice:

1. **RED:** with the multi-run oldest-first fixture and newest-first scenario
   asserts, temporarily disable production sort (`sort_workflow_runs_newest_first`
   no-op / skip resort). Scenario must fail because the first selected title is
   the oldest API row.
2. **GREEN:** restore sort; scenario passes; first selected title is newest;
   after End/load-more, Up selects the interleaved page-2 run, proving global
   ordering and selection remapping rather than append-only behavior.
3. Record commands and outcomes under Verification below.

## Verification

```bash
cargo fmt --all --check
cargo test --test issue208_behavior --locked
CARGO_TARGET_DIR=$PWD/target scripts/issue194-run-scenario.sh
make quick-check
# before push / merge readiness:
make ci-check
```

### TUI regression RED → GREEN evidence (2026-07-16)

Fixture returns page-1 runs oldest-first; scenario step asserts
`> [X] Inspectable Actions fixture` as the initially selected row.

1. **RED:** temporarily no-op'd `sort_workflow_runs_newest_first` and
   `resort_actions_runs_preserving_selection`.
   `scripts/issue194-run-scenario.sh` exited 1 with:
   `step 4 failed: expected screen to contain '> [X] Inspectable Actions fixture'`
2. **GREEN:** restored sort helpers. Same scenario exited 0:
   `ok: 35 steps` / `PASS: issue 194/208 Actions scenario (newest-first + jobs)...`
   Page-2 audit entry `actions/runs?page=2&per_page=30` present after `End`.


### Discriminating load-more regression evidence (2026-07-16)

The page-2 fixture timestamp falls between the two page-1 timestamps. After
`End` triggers pagination, `Up` must select `Interleaved Actions fixture run`;
append-only behavior leaves the rows in a different order.

1. **RED:** temporarily removed the post-append
   `resort_actions_runs_preserving_selection` call. The scenario exited 1 at
   step 9 because the interleaved row never became selected.
2. **GREEN:** restored post-append sorting. The same scenario exited 0 with
   `ok: 35 steps` and the read-only gh audit passed.

Current main was integrated with a true merge before final verification.
Do not rewrite earlier implementation commits to simulate TDD; this TUI slice
was proven with a genuine temporary RED patch that was discarded after GREEN.
