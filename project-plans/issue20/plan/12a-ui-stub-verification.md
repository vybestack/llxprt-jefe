# Phase 12A — UI Stub Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P12A
- **Prerequisites:** `.completed/P12.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the PR UI surface compiles, renders for `DashboardPullRequests`, is isolated (renders/emits
only), reuses shared components, avoids duplicated layout constants, and carries markers — with cited
evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (UI components/screen
   present and registered; reuse shared components; no duplicated layout constants; markers present).
2. **Behavioral code-reading evidence (file:line)** — full rendered-behavior code-reading is **N/A
   — stub phase** (components render an inert skeleton, asserting no measured layout yet). The
   analogous evidence is cited `file:line` proof each component/screen exists with the correct
   props/signature and an inert render body that emits only (no state/github imports).
3. **Runtime-path reachability** — see "Runtime-Path Reachability": `build_screen_element` reaches
   `PullRequestsScreen` for `DashboardPullRequests`; cite the stubbed arm. (No live layout yet.)
4. **Contradiction scan** — see "Contradiction Scan" (UI does NOT import github/app_input; no
   duplicated layout constant; no scroll math reading `crossterm::size()` directly).
5. **Atomic verdict** — `Phase 12: PASS` or `Phase 12: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of UI surface for REQ-PR-006,008,009,014, NFR-003
- **Behavior contract:** GIVEN P12, WHEN verified, THEN the screen + components exist with
  mockup-aligned prop signatures and the orchestration arm renders them.

## Implementation Tasks
- **Files to create:** `.completed/P12A.md`.
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
rg -n "DashboardPullRequests" src/ui/orchestration.rs src/ui/components/keybind_bar.rs
rg -n "PRS_SIDEBAR_WIDTH|prs_pane_rows|prs_detail_viewport_rows|pr_list_content_width" src/layout.rs
# Selection-follow helpers live in crate::layout (finding #2), NOT in a UI file. Confirm presence in
# layout.rs and that pr_list.rs consumes them:
rg -n "list_first_visible_index|list_visible_window" src/layout.rs
rg -n "list_first_visible_index|list_visible_window" src/ui/components/pr_list.rs
# There must be NO src/ui/components/list_viewport.rs file (finding #2). HARD gate:
if [ -f src/ui/components/list_viewport.rs ]; then
  echo "FAIL: src/ui/components/list_viewport.rs must not exist (helpers live in crate::layout)"; exit 1
fi
# Boundary-isolation HARD gate (finding #3 — invert the absence check; rg exits nonzero on no-match):
if rg -n "use crate::github|use crate::app_input" src/ui/ ; then
  echo "FAIL: src/ui imports a forbidden layer (github/app_input)"; exit 1
fi
# Traceability-marker HARD gate (finding #1): every NEW P12 deliverable file MUST carry ALL THREE
# marker types (@plan/@requirement/@pseudocode). Missing ANY one in ANY file is a hard FAIL.
PLAN_RE='@plan PLAN-20260624-PR-MODE\.P[0-9]+'
REQ_RE='@requirement REQ-PR-(NFR-)?[0-9]+'
PSEUDO_RE='@pseudocode component-[0-9]+ lines [0-9]+-[0-9]+'
marker_fail=0
for f in \
  src/ui/components/pr_list.rs src/ui/components/pr_detail.rs \
  src/ui/components/pr_filter_controls.rs src/ui/screens/pull_requests.rs ; do
  [ -f "$f" ] || { echo "$f: MISSING FILE"; marker_fail=1; continue; }
  miss=""
  rg -q "$PLAN_RE"   "$f" || miss="$miss @plan"
  rg -q "$REQ_RE"    "$f" || miss="$miss @requirement"
  rg -q "$PSEUDO_RE" "$f" || miss="$miss @pseudocode"
  [ -n "$miss" ] && { echo "$f: MISSING MARKER(S):$miss"; marker_fail=1; } || echo "$f: all three markers present"
done
[ "$marker_fail" -eq 0 ] || { echo "FAIL: one or more P12 deliverable files missing required markers"; exit 1; }

# Single-owner file boundary (finding #2): P12 is the SOLE creator of the PR UI files. Assert each
# exists and carries a P12 marker (they must NOT have been created by P03):
for f in \
  src/ui/screens/pull_requests.rs \
  src/ui/components/pr_list.rs \
  src/ui/components/pr_detail.rs \
  src/ui/components/pr_filter_controls.rs ; do
  if [ ! -f "$f" ]; then echo "FAIL: P12-owned UI file missing: $f"; exit 1; fi
done
# P12 MODIFIES (does not re-create) the existing orchestration.rs, REPLACING the P03 benign
# placeholder arm with the real PullRequestsScreen. The placeholder must be gone and the real
# screen wired:
if rg -n "ScreenMode::DashboardPullRequests" src/ui/orchestration.rs | rg -q "View \{ \}" ; then
  echo "FAIL: P03 placeholder element still present in DashboardPullRequests arm"; exit 1
fi
rg -n "PullRequestsScreen" src/ui/orchestration.rs   # the real screen must be wired in the arm
```

## Structural Verification Checklist
- [ ] Build green; components/screen/layout helpers present (cite).
- [ ] Orchestration arm added; existing arms unchanged.
- [ ] No duplicated layout constants (cite reuse of LEFT_COL_WIDTH/OUTER_BARS_HEIGHT).
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] UI isolation confirmed (no github/app_input imports) — cite import blocks.
- [ ] Viewport/list rows are props — cite prop structs.
- [ ] Shared components reused — cite imports.
- [ ] NO `todo!()`/`unimplemented!()` anywhere in the PR UI files (findings #1 & #4): `Cargo.toml`
  `[lints.clippy]` denies both macros (`todo`/`unimplemented` = "deny"), and clippy fires on their
  mere presence, so every render body MUST return a real (possibly empty/skeleton) element/value —
  never `todo!()`. HARD gate (absence passes, presence fails):
  ```bash
  if rg -n "todo!\(|unimplemented!\(" \
     src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
    echo "FAIL: todo!()/unimplemented!() in a PR render path (clippy denies both)"; exit 1
  fi
  ```

## Runtime-Path Reachability
- [ ] `build_screen_element(DashboardPullRequests)` returns the real `PullRequestsScreen` (NOT the
  P03 placeholder) and every component it mounts renders a real (possibly empty) tree — no panic on
  first render (cite). (findings #1 & #4)

## Contradiction Scan
- [ ] No component reads terminal size for scroll math.
- [ ] No new sidebar/statusbar/keybind re-implementation.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails (render paths are reachable, so
`todo!(`/`unimplemented!(` are included):
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" \
   src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
  echo "FAIL: deferred-implementation marker present in reachable PR UI files"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Dashboard/Issues/Split screens render unchanged.

## Success Criteria
- `Phase 12: PASS` with cited evidence, or `FAIL`.

## Failure Recovery
- Return to P12.

## Phase Completion Marker (`.completed/P12A.md`)
Phase ID, timestamp, cited evidence, verdict.
