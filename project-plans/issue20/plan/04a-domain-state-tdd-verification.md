# Phase 04A — Domain & State TDD Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P04A
- **Prerequisites:** `.completed/P04.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the RED test suite is behavioral, marker-tagged, free of test smells, and actually fails
against the stub for the right reasons.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (test modules compile and
   register; every test carries `@plan/@requirement/@pseudocode` markers; ≥1 RED failure present).
2. **Behavioral code-reading evidence (file:line)** — cite each behavioral RED test by `file:line`
   and the precise assertion it makes (the "behavioral evidence" in a RED phase is the failing
   ASSERTION proving the missing behavior, not a placeholder/compile error). See Semantic checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": tests drive state via the real
   reducer entry (`apply`/`apply_message`), not private helpers bypassing the hub.
4. **Contradiction scan** — see "Contradiction Scan" (no test contradicts a documented invariant).
5. **Atomic verdict** — `Phase 04: PASS` (RED demonstrated for the right reasons) or `Phase 04:
   FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of RED tests for REQ-PR-001,003,005-011,014, NFR-002
- **Behavior contract:** GIVEN P04, WHEN verified, THEN ≥1 test fails because reducer behavior is
  unimplemented (not because of a compile error or a wrong assertion), and every listed REQ behavior
  has ≥1 dedicated test.

## Implementation Tasks
- **Files to create:** `.completed/P04A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

The verifier re-runs the COMPLETE baseline. Because P04 is a TDD(RED) phase, the RED exception
applies to exactly ONE command — `cargo test` — which MUST report ≥1 failure. All other gates
MUST pass (the RED tests must COMPILE; only their assertions may fail):
```bash
cargo fmt --all --check                                            # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                    # MUST pass (RED tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p04a.log  # EXPECTED to FAIL (RED)
# RED confirmation (inverted, finding #3): rg exits nonzero on no-match, so assert PRESENCE of a
# failing test result rather than relying on a bare "# expect" comment:
if ! rg -q "test result: FAILED" /tmp/p04a.log ; then
  echo "FAIL: no failing test found — RED not demonstrated"; exit 1
fi
# Vacuous-test HARD gate (inverted, finding #3): these constructs would mask a real assertion, so
# their PRESENCE must FAIL the phase (rg exits nonzero on no-match, so a bare grep here would wrongly
# fail a correct test suite). Scans the layout-helper RED tests too (they live in src/layout.rs):
if rg -n "assert!\(true\)|#\[ignore\]|\.unwrap\(\)|\.expect\(" src/state/prs_tests*.rs ; then
  echo "FAIL: vacuous/forbidden test construct in prs_tests*.rs"; exit 1
fi
rg -n "@requirement REQ-PR-" src/state/prs_tests*.rs
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
RED exception: only `cargo test` may fail, and only because the behavioral tests are unimplemented.
If fmt, clippy, `check-clippy-allows.sh`, or build fail, the phase is a FAIL (RED tests must compile
and the codebase must remain clippy/format clean). `check-clippy-allows.sh` is the AUTHORITATIVE
no-allow/no-expect hard gate (finding #6) and is enforced even in the RED phase.

## Structural Verification Checklist
- [ ] All test modules compile and are registered.
- [ ] Every test has `@plan/@requirement/@pseudocode` markers.
- [ ] ≥1 RED failure present.

## Semantic Verification Checklist (Mandatory)
- [ ] Each REQ-PR behavior maps to ≥1 named test (cite test → REQ table).
- [ ] Staleness, composer-focus, filter-draft, selection-following tests assert the precise behavior.
- [ ] The shared viewport-helper RED tests (finding #2) exist in `src/layout.rs`'s `#[cfg(test)] mod
  tests` (REQ-PR-006, component-001 lines 182-196): `test_list_first_visible_index_follows_selection_past_viewport`,
  `test_list_first_visible_index_clamps_at_top_and_short_lists`, and
  `test_list_visible_window_returns_exact_n_rows_and_bounds`. They are RED because the P03 stubs
  return wrong-but-total values (`list_first_visible_index` returns `0`; `list_visible_window`
  returns an empty slice), so the failures are ASSERTION mismatches, never panics. Cite the failing
  assertions.
- [ ] REQ-PR-012 open-in-browser reducer half is RED-covered: a test asserts `OpenInBrowser` is a PURE
  transition (sets opening notice with a PR selected / `NoSelectionToOpen` with none; no I/O), and a
  test asserts `OpenInBrowserFailed` sets a scoped error notice — both `handled == true`, never silent.
- [ ] Persistence backward-compat (REQ-PR-NFR-002, finding #2) is covered by all THREE tests, each
  grounded in the ACTUAL persistence mechanism (a separate `persistence::State` DTO +
  `to_persisted_state`, NOT an `AppState` serde round-trip): (a)
  `test_to_persisted_state_excludes_prs_state` proves `to_persisted_state(&AppState{prs_state.active
  = true, ..})` omits ALL PR data from the persisted DTO (no `prs`/`pull_request*` key in its JSON
  key set), so `prs_state` is NOT written to `state.json`; (b)
  `test_pre_pr_persisted_state_deserializes_without_pr_fields` proves a legacy `state.json` value
  with NO PR fields still deserializes into `persistence::State` with all prior fields intact
  (mirrors `test_issue_base_prompt_state_backward_compat`); (c)
  `test_app_state_default_has_inactive_prs_state` proves `AppState::default().prs_state ==
  PullRequestsState::default()` (inactive). Cite each test and confirm its assertion targets the
  real DTO/mapping, not a non-existent `AppState` serde path.
- [ ] No test smells (no `assert!(true)`, `#[ignore]`, unwrap/expect/panic).
- [ ] Failures are due to missing behavior, not compile/assertion errors (cite failure messages).

## Runtime-Path Reachability
- [ ] Tests drive state via `apply`/`apply_message` (the real reducer entry), not private helpers
  bypassing the hub.

## Contradiction Scan
- [ ] No test contradicts a documented invariant.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails (test files must carry no deferred
markers; RED status comes from unimplemented behavior, not from placeholder comments):
```bash
if rg -n "TODO|FIXME|HACK|placeholder|for now" src/state/prs_tests*.rs ; then
  echo "FAIL: deferred-implementation marker present in test files"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing Issues/other tests still pass (no regression introduced by new modules).

## Success Criteria
- `Phase 04: PASS` with RED evidence + test→REQ table, or `FAIL`.

## Failure Recovery
- Return to P04.

## Phase Completion Marker (`.completed/P04A.md`)
Phase ID, timestamp, RED list, test→REQ table, verdict.
