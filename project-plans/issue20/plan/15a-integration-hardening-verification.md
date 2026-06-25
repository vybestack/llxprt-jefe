# Phase 15A — Integration Hardening Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P15A
- **Prerequisites:** `.completed/P15.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the integration suite genuinely exercises the full runtime chain for every requirement and
regression guard, that no seams remain, and `make ci-check` is green — with cited evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract" (GREEN
integration phase — every item is fully required, none N/A):
1. **Structural verification** — see "Structural Verification Checklist" (integration tests present
   and registered; markers present; `make ci-check` green incl. coverage ≥30).
2. **Behavioral code-reading evidence (file:line)** — cite `file:line` (test + production) proving
   each REQ and regression guard is exercised end-to-end through the real chain, no seams stubbed.
3. **Runtime-path reachability** — see "Runtime-Path Reachability (FULL)": trace the complete key →
   route → AppEvent → AppMessage → dispatch → apply → render chain for the integration scenarios.
4. **Contradiction scan** — see "Contradiction Scan" (no remaining seam/short-circuit; no silent
   drop; no sync gh I/O on the UI thread).
5. **Atomic verdict** — `Phase 15: PASS` or `Phase 15: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of end-to-end integration for REQ-PR-001..014, NFR-001..003
- **Behavior contract:** GIVEN P15, WHEN verified, THEN each requirement has an integration
  checkpoint driving the real chain and all regression guards are covered.

## Implementation Tasks
- **Files to create:** `.completed/P15A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

Run the COMPLETE verification baseline explicitly (this is a GREEN/integration phase — ALL commands
MUST pass; there is NO RED exception here). `make ci-check` is the superset gate and is also run:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p15a.log
make ci-check   # superset: fmt, clippy, coverage (--fail-under-lines 30), build, test
```

Then the phase-specific scans. The deferred-implementation check is a HARD inverted gate
— SCOPED to PR-owned changes only (finding #1): absence passes, presence fails. It must NOT grep all
of `src/`, because the current source contains PRE-EXISTING unrelated markers this PR does not own —
verified at `src/state/types.rs:211` (`TODO(issue #24)`), `src/state/types.rs:220`
(`Placeholder for future multi-issue handling`), and `src/persistence/mod.rs:559`
(`// For now, ...`). The gate scans the NEW PR-owned files in full and only the lines THIS branch
added to SHARED modified files via `git diff main`:
```bash
set -euo pipefail
# Presence-required HARD gate: the test log MUST contain a passing summary; absence ⇒ FAIL.
if ! rg -n "test result: ok" /tmp/p15a.log; then
  echo "FAIL: no passing 'test result: ok' summary found in /tmp/p15a.log"; exit 1
fi
PR_NEW_FILES=(
  src/github/parse_pr.rs
  src/state/prs_ops.rs src/state/prs_load_ops.rs src/state/prs_inline_ops.rs src/state/prs_mutation_ops.rs
  src/messages/prs_conversion.rs
  src/app_input/prs.rs src/app_input/prs_dispatch.rs src/app_input/prs_list_dispatch.rs src/app_input/prs_filter.rs src/app_input/prs_mutation.rs
  src/ui/components/pr_list.rs src/ui/components/pr_detail.rs src/ui/components/pr_filter_controls.rs
  src/ui/screens/pull_requests.rs
)
# NOTE (finding #2): the selection-follow viewport helpers live in src/layout.rs (a SHARED file),
# covered by the PR_SHARED_FILES added-line scan — there is NO list_viewport.rs.
PR_SHARED_FILES=(
  src/state/types.rs src/state/mod.rs src/input.rs src/messages.rs
  src/app_input/mod.rs src/app_input/normal.rs src/github/mod.rs src/layout.rs
  src/domain/mod.rs src/ui/orchestration.rs src/ui/mod.rs src/ui/components/mod.rs src/ui/screens/mod.rs src/lib.rs
)
DEFERRED_RE='TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now|will be implemented'
for f in "${PR_NEW_FILES[@]}"; do
  [ -f "$f" ] || continue
  if rg -n "$DEFERRED_RE" "$f" ; then echo "FAIL: deferred marker in new PR file $f"; exit 1; fi
done
for f in "${PR_SHARED_FILES[@]}"; do
  [ -f "$f" ] || continue
  if git diff main -- "$f" | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
    echo "FAIL: deferred marker ADDED by this branch in shared file $f"; exit 1
  fi
done
# Exact clippy-threshold assertion (finding #4) — both configs keep EXACT values + unmodified tree.
for cfg in clippy.toml .github/clippy/clippy.toml; do
  echo "== $cfg =="
  grep -E '^[[:space:]]*cognitive-complexity-threshold[[:space:]]*=[[:space:]]*15([[:space:]]|#|$)'  "$cfg" || { echo "FAIL cognitive-complexity-threshold != 15 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-lines-threshold[[:space:]]*=[[:space:]]*60([[:space:]]|#|$)'        "$cfg" || { echo "FAIL too-many-lines-threshold != 60 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-arguments-threshold[[:space:]]*=[[:space:]]*6([[:space:]]|#|$)'     "$cfg" || { echo "FAIL too-many-arguments-threshold != 6 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*type-complexity-threshold[[:space:]]*=[[:space:]]*250([[:space:]]|#|$)'      "$cfg" || { echo "FAIL type-complexity-threshold != 250 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*max-struct-bools[[:space:]]*=[[:space:]]*3([[:space:]]|#|$)'                 "$cfg" || { echo "FAIL max-struct-bools != 3 in $cfg"; exit 1; }
done
if ! git diff --quiet -- clippy.toml .github/clippy/clippy.toml ; then
  echo "FAIL: clippy threshold config(s) modified in the working tree"
  git diff -- clippy.toml .github/clippy/clippy.toml
  exit 1
fi
# Cargo.toml [lints.clippy] no-weaken gate (finding #2) — FAIL if this branch ADDS an allow or
# downgrades an existing deny/warn to allow under the [lints] table (check-clippy-allows.sh does
# NOT inspect Cargo.toml). Removing/tightening an allow is permitted.
if git diff main -- Cargo.toml | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E '=[[:space:]]*"allow"|level[[:space:]]*=[[:space:]]*"allow"' ; then
  echo "FAIL: this branch adds/weakens a Cargo.toml [lints.clippy] allow entry"; exit 1
fi
```

## Structural Verification Checklist
- [ ] All integration tests pass (including the finding #5 pagination/lazy-load checkpoint
  `it_pr_list_pagination_lazy_loads_appends_preserves_selection_and_discards_stale`).
- [ ] `make ci-check` green.
- [ ] Zero deferred markers in the PR-owned scope (new PR files + this branch's added lines in
  shared files); pre-existing unrelated markers are out of scope.

## Semantic Verification Checklist (Mandatory) — cite test names
- [ ] REQ→checkpoint table complete (every REQ-PR-001..014 + NFR mapped to ≥1 integration test).
- [ ] Each regression guard (#37/#39, #38/#40, #47, #54, #55, #56) mapped to ≥1 integration test.
- [ ] Stale/async/empty/auth/config-error checkpoints present.

## Runtime-Path Reachability (FULL)
- [ ] At least one checkpoint proves the full chain key→event→message→dispatch→reducer→render with
  cited hops.

## Contradiction Scan
- [ ] No integration test bypasses the dispatch layer (e.g. calling reducer directly to fake a
  journey).
- [ ] No regression in existing modes.

## No-Placeholder Verification
HARD inverted gate — SCOPED to PR-owned changes only (finding #1): absence passes, presence fails.
Re-run the identical SCOPED block from the "Verification Commands" section above (the `PR_NEW_FILES`
full scan + the `PR_SHARED_FILES` `git diff main` added-line scan). Do NOT grep all of `src/`: the
pre-existing unrelated markers at `src/state/types.rs:211`, `src/state/types.rs:220`, and
`src/persistence/mod.rs:559` are out of this PR's scope and must never be flagged or edited here.

## Integration Contract Acceptance Gates
- [ ] Backward compatibility: Dashboard/Issues/Split unchanged.
- [ ] Persistence schema unchanged; `prs_state` excluded.

## Success Criteria
- `Phase 15: PASS` with REQ→checkpoint + guard→checkpoint tables and a full runtime trace, or `FAIL`.

## Failure Recovery
- Return to P15 (or the owning layer's impl phase).

## Phase Completion Marker (`.completed/P15A.md`)
Phase ID, timestamp, REQ→checkpoint table, guard→checkpoint table, ci-check output, verdict.
