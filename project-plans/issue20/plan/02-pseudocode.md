# Phase 02 — Pseudocode

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P02
- **Prerequisites:** `.completed/P01A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Produce numbered, algorithmic pseudocode for all four PR-Mode components, with validation, error
handling, ordering, and side-effect annotations. Impl phases cite these line ranges.

## Requirements Implemented (Expanded)

### REQ-PR-001..014, NFR-001..003 (algorithmic level)
- **Behavior contract:** GIVEN the domain model, WHEN pseudocode completes, THEN every reducer
  transition, gh boundary method, key route, and message-bus conversion is specified line-by-line
  with explicit validation/error/ordering/side-effects.

## Implementation Tasks
- **Files to create/confirm:**
  - `analysis/pseudocode/component-001.md` — state + event reducer.
  - `analysis/pseudocode/component-002.md` — gh client boundary.
  - `analysis/pseudocode/component-003.md` — key routing + inline + chooser.
  - `analysis/pseudocode/component-004.md` — message bus + dispatch routing.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

P02 is pseudocode/documentation-only — NO production code is added. The COMPLETE workspace baseline
below MUST still pass to prove the tree remains green (all five commands apply even though no Rust
changed; there is NO RED exception in this phase):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
```

Then the phase-specific documentation checks:
```bash
for n in 001 002 003 004; do test -f project-plans/issue20/analysis/pseudocode/component-$n.md; done
rg -n "^[0-9]+:" project-plans/issue20/analysis/pseudocode/component-001.md | head
```

## Structural Verification Checklist
- [ ] All four components present with zero-padded numbered lines.
- [ ] c001 covers dispatch table, enter/exit, nav, focus, subfocus, loaded/failed, filter/search,
      inline, mutation, agent, reset-for-repo-change, reducer-hub wiring.
- [ ] c002 covers list/detail/comments/create + parse helpers + GhError mapping (sync boundary).
- [ ] c003 covers 8-level routing, inline, submit dispatch, chooser, search, filter, send-info.
- [ ] c004 covers enum additions, conversions (both directions), reducer arm, dispatch arms,
      loaders, viewport-prop update, full chain diagram, round-trip invariant.

## Semantic Verification Checklist (Mandatory)
- [ ] Staleness validation (scope + request_id) appears in every async-loaded handler.
- [ ] Max-scroll derived from real rendered length, viewport as prop (#37/#39).
- [ ] Repo nav independent of pane_focus (#47).
- [ ] Composer opens + sets subfocus NewComment + auto-scroll (#56).
- [ ] Filter controls update draft live + Apply reloads (#38/#40).
- [ ] List loaded → all rows preserved/rendered; selection-following (#54/#55).
- [ ] No silent None drops; parse failures logged.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails (the produced pseudocode deliverable
must carry no unfinished markers):
```bash
if rg -n "TODO|FIXME|HACK|placeholder|will be implemented" project-plans/issue20/analysis/pseudocode/ ; then
  echo "FAIL: deferred-implementation marker present in pseudocode deliverable"; exit 1
fi
```

## Success Criteria
- All four components complete; impl phases can cite concrete line ranges.

## Failure Recovery
- Amend the affected component; re-run P02A.

## Phase Completion Marker (`.completed/P02.md`)
Phase ID, timestamp, component list, line counts.
