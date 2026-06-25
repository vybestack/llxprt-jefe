# Phase 15 — Integration Hardening

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P15
- **Prerequisites:** `.completed/P14A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Add end-to-end integration tests that drive the full PR-Mode user journey through the real
key→event→message→dispatch→reducer→render chain, and harden every regression guard with an
integration-level regression test. Close any seams between the unit-tested layers.

## Requirements Implemented (Expanded)

### REQ-PR-001..014, REQ-PR-NFR-001..003 (end-to-end)
- **Behavior contract:** GIVEN a configured repo with a `github_repo` slug, WHEN a user presses `p`
  and interacts (navigate repos/PRs, filter, search, open detail, page comments, compose comment,
  send-to-agent, open-in-browser, exit), THEN each step transitions state correctly and renders the
  expected output, with no blocking I/O and correct staleness handling.

## Implementation Tasks

### Integration test suite (e.g. `tests/prs_mode_integration.rs` or in-crate integration module)
Numbered hard-gate checkpoints, each with a named test:
1. `it_enter_prs_mode_from_dashboard_loads_list` — entry → list reload spawned → `PrListLoaded`
   renders rows (REQ-PR-001,006).
2. `it_repo_nav_switches_scope_and_reloads` — repo Up/Down resets list + reloads (#47; REQ-PR-003).
3. `it_filter_apply_reloads_and_updates_list` — interactive filter → Apply → reload (#38/#40;
   REQ-PR-008).
4. `it_search_commit_reloads_with_query` — `/` → type → Enter → reload (REQ-PR-008).
5. `it_select_pr_loads_detail_with_reviews_and_checks` — Enter → `PrDetailLoaded` renders summaries
   (REQ-PR-009).
6. `it_scroll_detail_paginates_comments` — scroll near bottom → `load_more_pr_comments` →
   `PrCommentsPageLoaded` appends (REQ-PR-010).
7. `it_compose_comment_follows_viewport_and_appends` — `c` → composer subfocus + visible → submit →
   `PrCommentCreated` appends + follows (#56; REQ-PR-010).
8. `it_send_to_agent_writes_prompt_and_launches` — `S` → chooser → confirm → `.jefe/pr-prompt.md`
   written + agent launched (REQ-PR-011).
9. `it_open_in_browser_spawns_gh_pr_view_web` — `o` on a loaded PR (list or detail) emits
   `PrOpenInBrowser` → dispatch spawns `gh pr view <number> --repo <owner>/<name> --web` off-thread
   (asserted via the async wrapper, never a UI-thread call); with no PR selected, `o` is consumed and
   surfaces a `NoSelectionToOpen` notice (no silent drop); `external_url` is rendered display-only and
   no in-app merge/approve/review-submit keybinding exists (REQ-PR-012).
10. `it_esc_precedence_unwinds_then_exits` — full Esc chain (REQ-PR-004).
11. `it_exit_restores_prior_dashboard_focus` — `a`/Esc-exit restores focus (REQ-PR-005).
12. `it_stale_response_discarded_after_repo_switch` — late `PrListLoaded` for old scope ignored
   (NFR-002).
13. `it_missing_github_repo_shows_inline_config_error` — no slug → scoped config message, no spawn
   (REQ-PR-013,014).
14. `it_not_authenticated_shows_auth_error` — gh unauth → auth message (REQ-PR-013).
15. `it_empty_pr_list_shows_empty_state` — zero PRs → empty-state render (REQ-PR-014).
16. `it_no_blocking_gh_call_on_ui_thread` — assert loaders go through async wrapper (NFR-001).
17. `it_persisted_state_excludes_prs_state` — `to_persisted_state(&state)` maps only the persisted
    DTO fields and never reads `prs_state` (which is not `Serialize`); loading a pre-PR-mode
    persisted file yields default/inactive `prs_state` with all prior fields intact (NFR-003).
18. `it_dashboard_and_issues_modes_unaffected` — regression on existing modes.
19. `it_pr_list_pagination_lazy_loads_appends_preserves_selection_and_discards_stale` —
    end-to-end PR list pagination / lazy-load (REQ-PR-007). Drives the REAL chain: load first page
    (30 rows, `has_more = true`, stored `endCursor` per component-001 L213-214
    `apply_pr_list_loaded` setting `list_cursor`/`has_more_prs`); navigate selection DOWN to the last
    loaded row so the lazy-load trigger fires (component-001 L114-115: when
    `selected_pr_index == pull_requests.len() - 1 AND has_more_prs AND no list_page_pending`, emit
    `request_pr_list_page(scope, list_cursor, new request_id)`); deliver `PrListPageLoaded` and assert
    `apply_pr_list_page_loaded` (component-001 L224-229) APPENDS the new rows (list grows 30→60),
    PRESERVES existing rows + the current selection index, and recomputes
    `list_scroll_offset = list_first_visible_index(...)` (component-001 L182-189) so the selected
    row and scroll position stay on-screen (no jump, no clipping — #54/#55). Then deliver a STALE /
    out-of-order page response (wrong scope_id or stale request_id, i.e. a cursor/request mismatch)
    and assert it is DISCARDED by the `VALIDATE scope + request_id ELSE discard` guard
    (component-001 L225) — rows are NOT duplicated/appended and selection is unchanged.
    REQ-PR-007 / c001 L114-115,182-189,213-214,224-229; c002 L35-58,102-107; c004 L127-155.

## Pseudocode Traceability
- component-001..004 (full chain).

## Verification Commands

Run the COMPLETE baseline explicitly (all gates MUST pass — this is an integration/GREEN phase,
no RED exception; every integration test must be green):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
make ci-check   # superset: fmt, clippy, coverage (--fail-under-lines 30), build, test
```

### Deferred-implementation HARD inverted gate — SCOPED to PR-owned changes only (finding #1)

CRITICAL SCOPING RULE: do NOT grep all of `src/`. The current source contains PRE-EXISTING,
unrelated deferred-style markers that this PR does not own and must not be pressured to edit —
verified present at `src/state/types.rs:211` (`TODO(issue #24)`), `src/state/types.rs:220`
(`Placeholder for future multi-issue handling`), and `src/persistence/mod.rs:559`
(`// For now, we accept older versions ...`). A whole-`src/` grep would either false-FAIL on these
or create pressure to touch unrelated files. The gate is therefore scoped to (a) the NEW PR-owned
files in full, and (b) the lines THIS branch introduced into SHARED modified files via `git diff`.

```bash
set -euo pipefail
# (a) NEW PR-owned deliverable files — scan in full (these are 100% owned by this plan):
PR_NEW_FILES=(
  src/github/parse_pr.rs
  src/state/prs_ops.rs src/state/prs_load_ops.rs src/state/prs_inline_ops.rs src/state/prs_mutation_ops.rs
  src/messages/prs_conversion.rs
  src/app_input/prs.rs src/app_input/prs_dispatch.rs src/app_input/prs_list_dispatch.rs src/app_input/prs_filter.rs src/app_input/prs_mutation.rs
  src/ui/components/pr_list.rs src/ui/components/pr_detail.rs src/ui/components/pr_filter_controls.rs
  src/ui/screens/pull_requests.rs
)
# NOTE (finding #2): the selection-follow viewport helpers live in src/layout.rs (a SHARED file), so
# they are covered by the PR_SHARED_FILES added-line scan below — there is NO list_viewport.rs.
DEFERRED_RE='TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now|will be implemented'
for f in "${PR_NEW_FILES[@]}"; do
  [ -f "$f" ] || continue
  if rg -n "$DEFERRED_RE" "$f" ; then
    echo "FAIL: deferred-implementation marker in new PR file $f"; exit 1
  fi
done
# (b) SHARED modified files — only flag markers THIS branch ADDED (added '+' lines vs main),
#     so pre-existing unrelated markers (types.rs:211/220, persistence/mod.rs:559) are ignored:
PR_SHARED_FILES=(
  src/state/types.rs src/state/mod.rs src/input.rs src/messages.rs
  src/app_input/mod.rs src/app_input/normal.rs src/github/mod.rs src/layout.rs
  src/domain/mod.rs src/ui/orchestration.rs src/ui/mod.rs src/ui/components/mod.rs src/ui/screens/mod.rs src/lib.rs
)
for f in "${PR_SHARED_FILES[@]}"; do
  [ -f "$f" ] || continue
  # added lines only ('^+' but not the '+++' header), restricted to this branch's diff vs main:
  if git diff main -- "$f" | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
    echo "FAIL: deferred-implementation marker ADDED by this branch in shared file $f"; exit 1
  fi
done
```
All gates above MUST pass; no command is permitted to fail in this phase.

## Structural Verification Checklist
- [ ] All integration tests pass (the numbered checkpoints below, including the pagination/lazy-load
  test added per finding #5).
- [ ] Zero deferred markers in the PR-owned scope (new PR files + this branch's added lines in
  shared files); pre-existing unrelated markers are out of scope.
- [ ] `make ci-check` green (fmt, clippy, coverage ≥30, build, test).

## Semantic Verification Checklist (Mandatory)
- [ ] Each checkpoint drives the REAL dispatch chain (not a shortcut).
- [ ] Staleness, async, composer-follow, repo-nav, filter-interactivity guards each have an
  integration test.
- [ ] Existing modes regression test passes.

## Deferred Implementation Detection
HARD inverted gate — SCOPED to PR-owned changes only (finding #1): absence passes, presence fails.
This uses the SAME scoping as the "Verification Commands" gate above — it scans the NEW PR-owned
files in full and only the lines THIS branch added (via `git diff main`) in shared modified files,
so the pre-existing unrelated markers (`src/state/types.rs:211`, `src/state/types.rs:220`,
`src/persistence/mod.rs:559`) are never flagged. Run the identical scoped block from the
"Deferred-implementation HARD inverted gate — SCOPED to PR-owned changes only" section above
(`PR_NEW_FILES` full scan + `PR_SHARED_FILES` `git diff main` added-line scan).

## Success Criteria
- All integration tests green (including the finding #5 pagination/lazy-load test); `make ci-check`
  green; zero deferred markers in the PR-owned scope (new PR files + this branch's added lines in
  shared files).

## Failure Recovery
- Identify the failing seam; fix in the owning layer's module; re-run from the affected phase.

## Phase Completion Marker (`.completed/P15.md`)
Phase ID, timestamp, integration test results, ci-check output, zero-deferred output, semantic summary.
