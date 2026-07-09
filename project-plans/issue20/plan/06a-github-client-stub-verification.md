# Phase 06A — GitHub Client Stub Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P06A
- **Prerequisites:** `.completed/P06.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the PR gh client surface compiles, is isolated to the boundary, reuses existing
error/response patterns, and carries markers — with cited evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (gh client surface present
   and boundary-isolated; reuses `GhError`/response patterns; markers present).
2. **Behavioral code-reading evidence (file:line)** — full REQ-behavior code-reading is **N/A —
   stub phase** (the boundary stubs are inert and assert no parse/error behavior yet). The analogous
   evidence is cited `file:line` proof each stubbed fn exists with the correct signature and an inert
   body.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": the boundary is wired enough to
   compile and is reachable only via the (future) dispatch helpers; cite the stubbed entry points.
   (No live behavior yet — stub phase.)
4. **Contradiction scan** — see "Contradiction Scan" (boundary does NOT import ui/state/app_input;
   no synchronous call wired onto the UI thread; no duplicated error type).
5. **Atomic verdict** — `Phase 06: PASS` or `Phase 06: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of client surface for REQ-PR-006,007,009,010,012,013
- **Behavior contract:** GIVEN P06, WHEN verified, THEN all PR methods/helpers exist with correct
  signatures, the boundary imports nothing from UI/state, and existing methods are intact.

## Implementation Tasks
- **Files to create:** `.completed/P06A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

Run the COMPLETE verification baseline (this is a GREEN/stub phase — ALL commands MUST pass; there
is NO RED exception here):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

No-allow authoritative gate (finding #6): `bash scripts/check-clippy-allows.sh` above is the
AUTHORITATIVE no-allow/no-expect hard gate — it fails on ANY first-party clippy allow/expect
attribute in EVERY spelling and asserts the two clippy configs stay in sync. This phase runs it as a
hard gate (a nonzero exit fails the phase); no non-inverted `# expect none` greps are relied upon.

No-threshold-raise assertion (finding #4) — both configs keep the EXACT values and stay unmodified
in the working tree. HARD inverted gates (nonzero exit on any violation):
```bash
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

Then the phase-specific structural greps:
```bash
rg -n "list_pull_requests|get_pull_request_detail|list_pr_comments|create_pr_comment|open_pull_request_in_browser|build_pr_send_payload" src/github/mod.rs
# Finding 1 — PR comment FETCH object-path gate: list_pr_comments MUST target repository.pullRequest,
# NOT repository.issue. Confirm the new method exists and references pullRequest (inverted: fail if
# the PR comments fetcher queries repository.issue). The issue list_comments stays repository.issue.
rg -n "fn list_pr_comments" src/github/mod.rs   # expect the new PR-specific comments fetcher present
rg -n "parse_pull_requests_json|parse_pull_request_detail_json|parse_pr_review|parse_pr_check|sort_pull_requests|build_pr_search_args|build_pr_search_query|parse_checks_rollup" src/github/parse_pr.rs
# Boundary isolation HARD gate (finding #3 — rg exits NONZERO on no-match, so an absence check must be
# inverted to fail ONLY when a forbidden import is FOUND):
if rg -n "use crate::ui|use crate::state|use crate::app_input" src/github/ ; then
  echo "FAIL: src/github imports a forbidden layer (ui/state/app_input)"; exit 1
fi
# Traceability-marker HARD gate (finding #1): the new PR deliverable file MUST carry ALL THREE
# marker types (@plan/@requirement/@pseudocode). Missing ANY one is a hard FAIL.
PLAN_RE='@plan PLAN-20260624-PR-MODE\.P[0-9]+'
REQ_RE='@requirement REQ-PR-(NFR-)?[0-9]+'
PSEUDO_RE='@pseudocode component-[0-9]+ lines [0-9]+-[0-9]+'
marker_fail=0
for f in src/github/parse_pr.rs ; do
  [ -f "$f" ] || { echo "$f: MISSING FILE"; marker_fail=1; continue; }
  miss=""
  rg -q "$PLAN_RE"   "$f" || miss="$miss @plan"
  rg -q "$REQ_RE"    "$f" || miss="$miss @requirement"
  rg -q "$PSEUDO_RE" "$f" || miss="$miss @pseudocode"
  [ -n "$miss" ] && { echo "$f: MISSING MARKER(S):$miss"; marker_fail=1; } || echo "$f: all three markers present"
done
[ "$marker_fail" -eq 0 ] || { echo "FAIL: PR deliverable file missing required markers"; exit 1; }
```

## Boundary Import Scope (Finding #7 — enforced HERE, not P03A)
The NEW PR gh-client boundary modules (`src/github/parse_pr.rs` and the PR methods/helpers added to
`src/github/mod.rs`) must NOT import `crate::state`, `crate::ui`, or `crate::app_input`. They MAY use
`crate::domain`, `serde_json`, `std::process` (the `gh` CLI transport), and sibling `crate::github`
types (`GhError`, `IssueListResponse`, `IssueComment`, `SendPayload`, and the new PR response/parse
structs) — mirroring the current `src/github/parse.rs` import block (`crate::domain`,
`serde_json::Value`, `super::{GhError, IssueListResponse}`). The grep above asserts only the three
forbidden imports (`crate::ui`/`crate::state`/`crate::app_input`) are absent from `src/github/`.
(This is the correctly-scoped version of the boundary check; P03A does not perform it because P03
introduces no gh-client code.)

## Structural Verification Checklist
- [ ] Build green; all signatures present (cite).
- [ ] `PrListResponse` fields match `IssueListResponse` shape.
- [ ] Existing methods unchanged (cite).
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] Boundary isolation confirmed (no UI/state imports) — cite import block.
- [ ] `create_pr_comment` documents issues-endpoint reuse (REST, valid for PR numbers) — cite doc.
- [ ] `list_pr_comments` exists and is documented as the PR-specific comments fetcher targeting
  `repository.pullRequest(number:).comments` (NOT `repository.issue`); cite doc comment. The issue
  `list_comments` is NOT reused for PR comment FETCH (Finding 1 / P00A §2d).
- [ ] `GhError` reused, not duplicated — cite return types.
- [ ] NO `todo!()`/`unimplemented!()` anywhere in the PR gh-client stub bodies (findings #1 & #4 —
  clippy denies both macros). The stub methods/helpers return TOTAL deterministic values
  (`Ok(Default::default())` / a deterministic `Err` / empty `Vec`/`String` / degraded records) so the
  P07 RED tests fail by BEHAVIORAL assertion, never a panic. HARD gate (exit nonzero on ANY match):
  ```bash
  if rg -n "todo!\(\)|unimplemented!\(\)" src/github/parse_pr.rs src/github/mod.rs ; then
    echo "FAIL: todo!()/unimplemented!() present in PR gh-client stubs"; exit 1
  fi
  ```

## Runtime-Path Reachability
- [ ] These methods are called only from `prs_dispatch`/`prs_list_dispatch` via
  `spawn_gh_task_with_panic` (documented; enforced in later phases) — NOT from startup, input, render,
  or any P06 test. Their TOTAL wrong-value stub bodies are panic-free and clippy-clean; the P07 RED
  tests / P08 impl drive the real behavior (findings #1 & #4).

## Contradiction Scan
- [ ] No method mutates AppState or returns UI types.

## Deferred Implementation Detection
Stub phase: `todo!()`/`unimplemented!()` are FORBIDDEN in ALL PR gh-client stubs (findings #1 & #4 —
clippy denies both macros), so the macro scan is a HARD inverted gate here (NOT record-only); the
other deferred markers are also a HARD inverted gate:
```bash
# Hard inverted gate for todo!()/unimplemented!() — absence passes, presence fails:
if rg -n "todo!\(\)|unimplemented!\(\)" src/github/parse_pr.rs src/github/mod.rs ; then
  echo "FAIL: todo!()/unimplemented!() present in PR gh-client stubs"; exit 1
fi
# Hard inverted gate for non-macro markers — absence passes, presence fails:
if rg -n "TODO|FIXME|HACK|placeholder|for now" src/github/parse_pr.rs ; then
  echo "FAIL: stray deferred marker (non-macro) in P06 stub"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing issue client behavior unchanged (its tests green).

## Success Criteria
- `Phase 06: PASS` with cited evidence, or `FAIL`.

## Failure Recovery
- Return to P06.

## Phase Completion Marker (`.completed/P06A.md`)
Phase ID, timestamp, cited evidence, verdict.
