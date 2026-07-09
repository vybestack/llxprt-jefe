# Phase 02A — Pseudocode Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P02A
- **Prerequisites:** `.completed/P02.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the pseudocode is complete, numbered, internally consistent, and covers every REQ behavior
and every regression guard, with cited line references.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract"; for a
documentation-only pseudocode phase, the two code-level items are explicitly marked **N/A —
documentation phase** with the reason (no production code exists yet to read or to trace at runtime),
NOT silently omitted:
1. **Structural verification** — see "Structural Verification Checklist" (continuous numbered lines;
   cross-component event-name consistency; round-trip invariant stated in c004).
2. **Behavioral code-reading evidence (file:line)** — **N/A — documentation phase.** P02 produces
   `analysis/pseudocode/component-00N.md` only; there is no production `src/` code to cite. The
   analogous evidence is each REQ → cited pseudocode line range (Semantic checklist).
3. **Runtime-path reachability** — provided at PSEUDOCODE level only: c004's full chain is verified
   to match the `00-overview.md` dispatch map (see "Runtime-Path Reachability"). Real per-hop `src/`
   `file:line` tracing is **N/A — documentation phase** (no code yet).
4. **Contradiction scan** — see "Contradiction Scan" (no event handled two contradictory ways across
   components; no pseudocode mutates AppState from the gh boundary).
5. **Atomic verdict** — `Phase 02: PASS` or `Phase 02: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of pseudocode for REQ-PR-001..014, NFR-001..003
- **Behavior contract:** GIVEN the four components, WHEN verified, THEN every REQ behavior has a
  cited pseudocode line range and every regression guard is realized in the algorithm.

## Implementation Tasks
- **Files to create:** `.completed/P02A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

Run the COMPLETE verification baseline (P02 is documentation-only, so the workspace must remain
green — ALL commands MUST pass; there is NO RED exception here):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

Then the phase-specific pseudocode greps:
```bash
rg -n "^[0-9]+:" project-plans/issue20/analysis/pseudocode/component-002.md | tail
rg -n "scope_repo_id|request_id" project-plans/issue20/analysis/pseudocode/component-001.md
rg -n "viewport|prs_detail_viewport_rows" project-plans/issue20/analysis/pseudocode/component-004.md
```

## Structural Verification Checklist
- [ ] Each component uses continuous numbered lines.
- [ ] Cross-references between components are consistent (event names match across c001/c003/c004).
- [ ] Round-trip conversion invariant stated in c004.

## Semantic Verification Checklist (Mandatory)
- [ ] Trace each REQ-PR-NNN to a cited pseudocode line range (table in verdict).
- [ ] Each regression guard (#37/#39/#47/#54/#55/#56, #38/#40) cited to a specific line.
- [ ] Sync gh boundary + async dispatch separation is unambiguous.
- [ ] Reducer is pure (no I/O); side effects live only in dispatch helpers.

## Runtime-Path Reachability
- [ ] c004 full chain matches `00-overview.md` dispatch map (cite both).

## Contradiction Scan
- [ ] No event handled in two contradictory ways across components.
- [ ] No pseudocode mutates AppState from the gh boundary.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails (the pseudocode deliverable must
carry no unfinished markers):
```bash
if rg -n "TODO|FIXME|HACK|placeholder|will be implemented" project-plans/issue20/analysis/pseudocode/ ; then
  echo "FAIL: deferred-implementation marker present in pseudocode deliverable"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Conversions are bidirectional and total over PR variants.
- [ ] Reducer-hub arm returns bool + debug_assert pattern (mirrors Issues).

## Success Criteria
- `Phase 02: PASS` with REQ→line table, or `FAIL` with remediation.

## Failure Recovery
- Return to P02; amend components.

## Phase Completion Marker (`.completed/P02A.md`)
Phase ID, timestamp, REQ→line table, verdict, semantic summary.
