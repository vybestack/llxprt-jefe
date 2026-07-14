# Issue 194: Inspectable GitHub Actions jobs

## Problem and contract

Actions run details must expose jobs as a compact, keyboard-navigable inline
sub-focus. Jobs begin collapsed. The focused job is visibly distinct, and its
steps retain their status glyphs when expanded.

The deterministic input contract is:

- `Enter` on a selected run moves focus from `RunList` to `Detail`.
- `Up`/`Down` in `Detail` moves the focused job and keeps its row in view.
- `Enter`/`Right` expands the focused job and never collapses it.
- `Left` collapses the focused job and otherwise remains in `Detail`.
- `Esc` in `Detail` collapses the focused job when expanded; when already
  collapsed, it returns focus to `RunList`. `Esc` from `RunList` exits Actions.
- Run changes and detail reloads clear expansions; accepted detail loads focus
  the first job, if present.

## Architecture

Use the existing inline job sub-focus in `ActionsState` rather than a fourth
pane. The reducer owns focus, expansion, collapse, and scroll-following. The
Actions pure view projects the focused job and computes its rendered line. The
UI only renders the projection and emits typed intent. Runtime, GitHub, and
persistence boundaries remain unchanged.

The expand operation receives a dedicated `ActionsMessage::ExpandJob` and
`AppEvent::ActionsExpandJob`; using the existing toggle would violate the
expand-only key contract. Collapse remains a separate typed intent.

A fourth pane is rejected because it would duplicate run-detail context, require
new layout and selection geometry, and diverge from the established PR-thread
inline inspection model without adding behavior required by this issue.

## Test-first sequence

1. Update the fixed-geometry Actions TUI scenario first. It asserts the new
   `Enter detail` hint and proves `Tab` into Detail followed by `Esc` refocuses
   the run list instead of exiting Actions.
2. Add key-resolution tests for RunList `Enter`, Detail expand-only keys,
   collapse keys, and contextual `Esc` behavior.
3. Add reducer tests for idempotent expand, collapse/refocus semantics, default
   collapse, and job-navigation scroll-following across collapsed and expanded
   predecessors.
4. Add pure projection/UI tests proving the focused job row is identified,
   visible after windowing, and rendered with a stable focus marker while
   per-step success/failure glyphs remain intact.
5. Add message conversion/classification contract tests for the new expand-only
   intent.
6. Implement the smallest production changes through AppEvent -> ActionsMessage
   -> reducer -> pure projection -> UI, then refactor only where needed to meet
   complexity and source-size gates.
7. Run targeted tests, `cargo fmt --all`, `make quick-check`, and `make ci-check`.

## Invariants and edge cases

- No detail or no jobs: expand/collapse/navigation are safe no-ops; Detail Esc
  refocuses RunList.
- A stale focused index is clamped before projection or transition.
- A zero-row viewport does not underflow and renders no content.
- Expanding/collapsing clamps the stored scroll offset after line-count changes.
- Job navigation follows rendered job-row positions, including step rows from
  expanded preceding jobs.
