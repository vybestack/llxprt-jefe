# Phase 14A — UI Integration Impl Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P14A
- **Prerequisites:** `.completed/P14.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the PR render logic matches the mockup layout contract and all rendering regression guards,
stays isolated, and is placeholder/override-free — with cited evidence and the 7 mockup placement
acceptance checks.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract" (GREEN
implementation phase — every item is fully required, none N/A):
1. **Structural verification** — see "Structural Verification Checklist" (render logic present and
   isolated; markers present; complexity within thresholds; shared layout constants reused).
2. **Behavioral code-reading evidence (file:line)** — cite `file:line` proving each render behavior
   and the 7 mockup placement acceptance checks are realized (sidebar 22u, list height from
   `prs_pane_rows`, detail viewport prop, banners/bands conditionality, selection-following). See
   Semantic checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": `build_screen_element` →
   `PullRequestsScreen` → list/detail components render from state-derived props; cite each hop.
4. **Contradiction scan** — see "Contradiction Scan": no row clipping/drop (#54/#55), no
   `crossterm::size()` read for scroll math (#37/#39), UI imports clean.
5. **Atomic verdict** — `Phase 14: PASS` or `Phase 14: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of UI impl for REQ-PR-006,008,009,010,012,013,014, NFR-003
- **Behavior contract:** GIVEN P14, WHEN verified, THEN render output satisfies the mockup
  measurements and #54/#55/#56/#37f/#37g/#37h guards.

## Implementation Tasks
- **Files to create:** `.completed/P14A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

Run the COMPLETE verification baseline (this is a GREEN/impl phase — ALL commands MUST pass; there
is NO RED exception here):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

Then the phase-specific placeholder/override/terminal-size gates. This is an impl-verifier phase, so
the deferred-implementation check is a HARD inverted gate (finding #6) — absence passes, presence
fails:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" \
   src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
  echo "FAIL: deferred-implementation marker present after impl phase"; exit 1
fi
# Forbidden-API HARD gate (finding #3 — rg exits nonzero on no-match, so invert: fail ONLY when the
# forbidden pattern is FOUND). PR components take viewport rows as props, not from terminal size:
if rg -n "crossterm::terminal::size" src/ui/components/pr_*.rs ; then
  echo "FAIL: PR components must not read crossterm::terminal::size (use viewport-rows props)"; exit 1
fi
# Selection-follow helpers live in crate::layout (finding #2) — there must be NO list_viewport.rs:
if [ -f src/ui/components/list_viewport.rs ]; then
  echo "FAIL: src/ui/components/list_viewport.rs must not exist (helpers live in crate::layout)"; exit 1
fi
# No-allow gate (finding #6). `bash scripts/check-clippy-allows.sh` (run in the baseline above) is
# AUTHORITATIVE. The block below is the SAME inverted multi-pattern defense-in-depth gate used in
# P16 — it FAILS (nonzero) if ANY first-party allow/expect attribute exists in ANY spelling; ZERO
# matches passes. (The old single-pattern `# expect none` comment was non-inverted and is replaced.)
for pat in \
  '#\[allow\(clippy'          \
  '#!\[allow\(clippy'         \
  'cfg_attr\(.*allow\(clippy' \
  '#\[expect\(clippy'         \
  '#!\[expect\(clippy'        \
  'cfg_attr\(.*expect\(clippy' ; do
  if rg -n "$pat" src/ ; then
    echo "FAIL: forbidden clippy allow/expect attribute found ($pat)"; exit 1
  fi
done
```

Exact clippy-threshold assertion (finding #4) — the two configs `./clippy.toml` and
`./.github/clippy/clippy.toml` MUST keep the EXACT values and MUST NOT be modified in the working
tree. HARD inverted gates (nonzero exit on any violation):
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
- [ ] Suite green; no placeholders; markers present.

## Mockup Placement Acceptance Checks (cite test/file:line for each)
- [ ] Sidebar width = 22u.
- [ ] Two-column layout (sidebar + workspace), not three.
- [ ] PR list region height = `prs_pane_rows` list portion.
- [ ] Unified detail view (metadata+body+reviews+checks+comments) scrolls as one region.
- [ ] Filter band appears only when filter controls open.
- [ ] Agent chooser overlays at expected position.
- [ ] Keybind bar shows PR-mode binds.

## Semantic Verification Checklist (Mandatory) — cite file:line
- [ ] #54 all rows; #55 selection-follow; #56 composer visible; #37f overflow-from-length;
  #37g/#39 viewport-prop; #37h truncation.
- [ ] The #54/#55 guards are realized by the `crate::layout` selection-follow helpers
  (`list_first_visible_index`/`list_visible_window`, component-001 lines 182-196; cite file:line).
  Confirm `pr_list.rs` consumes them and does NOT use `ScrollableText` for list rows, that no claim of
  reusing a pre-existing list-scroll helper remains (none exists: `issue_list.rs` still renders all
  rows with no offset), and that NO `src/ui/components/list_viewport.rs` file exists (finding #2).
- [ ] UI isolation (no github/app_input imports).

## Runtime-Path Reachability
- [ ] `DashboardPullRequests` → `PullRequestsScreen` → components consume `prs_state` (cite).

## Contradiction Scan
- [ ] No component reads terminal size for scroll math (grep empty).
- [ ] No function exceeds clippy thresholds.

## No-Placeholder Verification
HARD inverted gate (finding #6) — absence passes, presence fails:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" \
   src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs ; then
  echo "FAIL: deferred-implementation marker present after impl phase"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing screens unchanged.

## Success Criteria
- `Phase 14: PASS` with 7 placement checks + guard citations, or `FAIL`.

## Failure Recovery
- Return to P14.

## Phase Completion Marker (`.completed/P14A.md`)
Phase ID, timestamp, placement checks, guard citations, verdict.
