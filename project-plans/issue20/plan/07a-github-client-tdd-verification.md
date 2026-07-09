# Phase 07A — GitHub Client TDD Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P07A
- **Prerequisites:** `.completed/P07.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the parse/arg/error tests are behavioral, fixture-grounded, smell-free, and RED for the right
reasons, with cited evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (test modules compile and
   register; markers present; ≥1 RED failure present).
2. **Behavioral code-reading evidence (file:line)** — cite each behavioral parse/arg/error RED test
   by `file:line` and the precise fixture-grounded assertion it makes (the RED "behavioral evidence"
   is the failing assertion proving missing parse/error behavior). See Semantic checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": tests exercise the real boundary
   parse/arg/error entry points (not private shims), so the GREEN impl will be reached the same way.
4. **Contradiction scan** — see "Contradiction Scan" (no test contradicts a documented boundary
   invariant; no test asserts a silent-None drop is acceptable).
5. **Atomic verdict** — `Phase 07: PASS` (RED demonstrated for the right reasons) or `Phase 07:
   FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of client tests for REQ-PR-006,007,009,010,012,013
- **Behavior contract:** GIVEN P07, WHEN verified, THEN each parse behavior has a dedicated test
  using realistic fixtures, and failures are due to unimplemented parsing.

## Implementation Tasks
- **Files to create:** `.completed/P07A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

The verifier re-runs the COMPLETE baseline. Because P07 is a TDD(RED) phase, the RED exception
applies to exactly ONE command — `cargo test` — which MUST report ≥1 failure. All other gates
MUST pass (the RED tests must COMPILE; only their assertions may fail):
```bash
cargo fmt --all --check                                              # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                  # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                      # MUST pass (RED tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p07a.log  # EXPECTED to FAIL (RED)
# RED confirmation (inverted, finding #3): rg exits nonzero on no-match, so assert PRESENCE:
if ! rg -q "test result: FAILED" /tmp/p07a.log ; then
  echo "FAIL: no failing test found — RED not demonstrated"; exit 1
fi
rg -n "@requirement REQ-PR-" src/github/
# Vacuous/forbidden test construct HARD gate (inverted, finding #3) — presence FAILS:
if rg -n "assert!\(true\)|#\[ignore\]|\.unwrap\(\)|\.expect\(" src/github/ ; then
  echo "FAIL: vacuous/forbidden test construct in src/github tests"; exit 1
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
RED exception: ONLY `cargo test` may fail, and only because the behavioral parse/arg/error tests are
unimplemented. If fmt, clippy, `check-clippy-allows.sh`, or build fail, the phase is a FAIL (RED
tests must compile and the codebase must remain clippy/format clean). `check-clippy-allows.sh` is the
AUTHORITATIVE no-allow/no-expect hard gate (finding #6) and is enforced even in the RED phase.

## Structural Verification Checklist
- [ ] Tests compile; ≥1 RED.
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] Each parse/arg/error behavior maps to ≥1 test (cite test→REQ table).
- [ ] Fixtures match `gh --json` field names (cite a fixture).
- [ ] Malformed-JSON test asserts ParseError (cite).
- [ ] Finding 1 — PR comments object-path test present: `test_list_pr_comments_query_targets_
  pull_request_not_issue` asserts the emitted query contains `pullRequest(` and NOT `issue(number:`,
  selects `comments(first:` + `pageInfo{hasNextPage endCursor}`, and passes `after` through unchanged
  (cite test). The PR comments FETCH must NOT reuse the issue `list_comments` query path.
- [ ] No test smells.

## Runtime-Path Reachability
- [ ] Tests call the real parse/arg functions (not reimplemented logic).

## Contradiction Scan
- [ ] No test asserts behavior contradicting the boundary contract (e.g. expecting AppState change).

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails. INCLUDES `todo!(`/`unimplemented!(`:
the P06 GhClient/parse_pr stub bodies are TOTAL and clippy-clean — they contain NO
`todo!()`/`unimplemented!()` (findings #1 & #4; clippy denies both macros), so this gate trips on any
occurrence across the github surface:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" src/github/tests.rs src/github/parse_pr.rs src/github/mod.rs ; then
  echo "FAIL: deferred-implementation marker present in github files"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing github tests still pass.

## Success Criteria
- `Phase 07: PASS` with RED evidence + test→REQ table, or `FAIL`.

## Failure Recovery
- Return to P07.

## Phase Completion Marker (`.completed/P07A.md`)
Phase ID, timestamp, RED list, test→REQ table, verdict.
