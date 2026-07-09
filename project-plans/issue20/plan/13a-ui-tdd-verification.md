# Phase 13A — UI TDD Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P13A
- **Prerequisites:** `.completed/P13.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the UI render tests are behavioral (read the rendered buffer), cover every layout measurement
and rendering regression guard, and are RED for the right reasons — with cited evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (render-test modules
   compile and register; markers present; ≥1 RED failure present; every layout measurement covered).
2. **Behavioral code-reading evidence (file:line)** — cite each behavioral render RED test by
   `file:line` and the precise rendered-buffer assertion it makes (the RED "behavioral evidence" is
   the failing buffer assertion proving missing render behavior, never `assert!(true)`). See
   Semantic checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": tests render the real
   `PullRequestsScreen`/components with props derived from the layout helpers (not independent
   `crossterm::size()` reads).
4. **Contradiction scan** — see "Contradiction Scan" (no test contradicts a documented layout
   invariant; no test asserts clipped/dropped rows are acceptable — #54/#55).
5. **Atomic verdict** — `Phase 13: PASS` (RED demonstrated for the right reasons) or `Phase 13:
   FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of UI tests for REQ-PR-006,008,009,010,012,013,014
- **Behavior contract:** GIVEN P13, WHEN verified, THEN each mockup measurement + rendering
  regression has a dedicated buffer-reading test and failures stem from the unimplemented render.

## Implementation Tasks
- **Files to create:** `.completed/P13A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

The verifier re-runs the COMPLETE baseline. Because P13 is a TDD(RED) phase, the RED exception
applies to exactly ONE command — `cargo test` — which MUST report ≥1 failure. All other gates
MUST pass (the RED tests must COMPILE; only their assertions may fail):
```bash
cargo fmt --all --check                                              # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                  # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                      # MUST pass (RED tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p13a.log  # EXPECTED to FAIL (RED)
rg -n "test result: FAILED" /tmp/p13a.log                            # expect >=1 failure (RED confirmed)
rg -n "@requirement REQ-PR-" src/ui/
# No vacuous/ignored tests (finding #3 — invert the absence check; rg exits nonzero on no-match):
if rg -n "assert!\(true\)|#\[ignore\]" src/ui/ ; then
  echo "FAIL: vacuous assert!(true) or #[ignore] present in src/ui tests"; exit 1
fi
# No-threshold-raise assertion (finding #4) — MUST pass even in RED; both configs exact + unmodified:
for cfg in clippy.toml .github/clippy/clippy.toml; do
  echo "== $cfg =="
  grep -E '^[[:space:]]*cognitive-complexity-threshold[[:space:]]*=[[:space:]]*15([[:space:]]|#|$)'  "$cfg" || { echo "FAIL cognitive-complexity-threshold != 15 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-lines-threshold[[:space:]]*=[[:space:]]*60([[:space:]]|#|$)'        "$cfg" || { echo "FAIL too-many-lines-threshold != 60 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-arguments-threshold[[:space:]]*=[[:space:]]*6([[:space:]]|#|$)'     "$cfg" || { echo "FAIL too-many-arguments-threshold != 6 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*type-complexity-threshold[[:space:]]*=[[:space:]]*250([[:space:]]|#|$)'      "$cfg" || { echo "FAIL type-complexity-threshold != 250 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*max-struct-bools[[:space:]]*=[[:space:]]*3([[:space:]]|#|$)'                 "$cfg" || { echo "FAIL max-struct-bools != 3 in $cfg"; exit 1; }
done
if ! git diff --quiet -- clippy.toml .github/clippy/clippy.toml ; then
  echo "FAIL: clippy threshold config(s) modified in the working tree"; git diff -- clippy.toml .github/clippy/clippy.toml; exit 1
fi
# Cargo.toml [lints.clippy] no-weaken gate (finding #2) — FAIL if this branch ADDS an allow or
# downgrades an existing deny/warn to allow under the [lints] table (check-clippy-allows.sh does
# NOT inspect Cargo.toml). Removing/tightening an allow is permitted.
if git diff main -- Cargo.toml | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E '=[[:space:]]*"allow"|level[[:space:]]*=[[:space:]]*"allow"' ; then
  echo "FAIL: this branch adds/weakens a Cargo.toml [lints.clippy] allow entry"; exit 1
fi
```
RED exception: ONLY `cargo test` may fail, and only because the behavioral render tests are
unimplemented. If fmt, clippy, `check-clippy-allows.sh`, or build fail, the phase is a FAIL (RED
tests must compile and the codebase must remain clippy/format clean). `check-clippy-allows.sh` is the
AUTHORITATIVE no-allow/no-expect hard gate (finding #6) and is enforced even in the RED phase.

## Structural Verification Checklist
- [ ] Tests compile; ≥1 RED; markers present.

## Semantic Verification Checklist (Mandatory) — cite test names
- [ ] #54 all-rows, #55 selection-follow, #56 composer-visible, #37f overflow-from-length,
  #37g/#39 viewport-prop, #37h truncation — each has a test.
- [ ] `pr_list` render tests prove all loaded rows render (#54) and the selected row stays visible
  (#55) by consuming the `crate::layout` selection-follow helpers — cite test names. Confirm
  `ScrollableText` is NOT used for list rows. (The PURE helper algorithm — `list_first_visible_index`/
  `list_visible_window` — lives in `src/layout.rs` and its dedicated pure-logic RED tests are owned by
  P04, NOT re-tested here; finding #2.)
- [ ] Layout test asserts sidebar 22u + two-column.
- [ ] Empty-state + error-banner tests present.
- [ ] Tests assert on rendered buffer content (cite one).

## Runtime-Path Reachability
- [ ] Tests render the real `PullRequestsScreen`/components (not stand-ins).

## Contradiction Scan
- [ ] No test asserts a measurement contradicting mockups.md.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails. `Cargo.toml` `[lints.clippy]`
denies `todo`/`unimplemented`, so `todo!()`/`unimplemented!()` are NEVER allowed in these files
(findings #1 & #4), and this gate includes them:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" \
   src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
  echo "FAIL: deferred-implementation marker present in PR UI files"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing UI tests still pass.

## Success Criteria
- `Phase 13: PASS` with RED evidence + guard→test table, or `FAIL`.

## Failure Recovery
- Return to P13.

## Phase Completion Marker (`.completed/P13A.md`)
Phase ID, timestamp, RED list, guard→test table, verdict.
