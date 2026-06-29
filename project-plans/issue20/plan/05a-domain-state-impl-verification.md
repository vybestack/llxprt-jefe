# Phase 05A — Domain & State Impl Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P05A
- **Prerequisites:** `.completed/P05.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the reducer behavior is correct, pure, placeholder-free, within complexity limits, and that
all P04 tests pass for the right reasons — with cited evidence and a runtime-path trace.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract" (this
is a GREEN implementation phase — every item is fully required, none N/A):
1. **Structural verification** — see "Structural Verification Checklist" (reducers/helpers present;
   markers present; complexity within thresholds; existing enums/arms intact).
2. **Behavioral code-reading evidence (file:line)** — cite `file:line` in `src/` proving each
   REQ-PR behavior is realized by the reducer logic (not merely that a symbol exists). See Semantic
   checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": trace key → route → AppEvent →
   AppMessage → `dispatch_app_message` → `apply_message` → `apply_prs_message` → result, citing each
   hop's `file:line`.
4. **Contradiction scan** — see "Contradiction Scan" (no silent `None` drop, no I/O in the reducer,
   no duplicated constant, no scroll math reading `crossterm::size()`).
5. **Atomic verdict** — `Phase 05: PASS` or `Phase 05: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of reducer for REQ-PR-001,003,005-011,014, NFR-002,003
- **Behavior contract:** GIVEN P05, WHEN verified, THEN every reducer transition matches pseudocode,
  staleness/composer/filter/selection behaviors hold, and no placeholder/override remains.

## Implementation Tasks
- **Files to create:** `.completed/P05A.md`.
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

Then the phase-specific placeholder/override gates. This is an impl-verifier phase, so the
deferred-implementation check is a HARD inverted gate (finding #6) — absence passes, presence fails.
`src/layout.rs` is included because P05 implements the shared viewport helpers there (finding #2);
because `layout.rs` is a SHARED pre-existing file, scan only the lines this branch ADDED (via
`git diff main`) so pre-existing markers elsewhere in the file are not flagged:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" src/state/prs_*.rs src/messages/prs_conversion.rs ; then
  echo "FAIL: deferred-implementation marker present after impl phase"; exit 1
fi
if git diff main -- src/layout.rs | grep -E '^\+' | grep -Ev '^\+\+\+' \
   | grep -E "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" ; then
  echo "FAIL: deferred-implementation marker added to src/layout.rs viewport helpers"; exit 1
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
- [ ] Suite green; debug_assert(handled) present and never trips.
- [ ] No `todo!()`/`unimplemented!()` in state/conversion.
- [ ] Markers per fn (cite a few).

## Semantic Verification Checklist (Mandatory) — cite file:line for each
- [ ] `enter_prs_mode`/`exit_prs_mode` save+restore prior focus with bounds fallback.
- [ ] `reset_prs_for_repo_change` clears list/detail/pending; committed filter/search preserved.
- [ ] Repo nav (#47) changes selection via `PrFocus::RepoList`, not `pane_focus`.
- [ ] SHARED repo-nav helper (Finding 5): `AppState::move_repo_selection` exists in `src/state/mod.rs`
  and is called by BOTH `navigate_repo_up/down_in_issues_mode` AND `navigate_repo_up/down_in_prs_mode`.
  PR mode does NOT define a private copy of the selection-move logic. Verify with:
  `rg -n "move_repo_selection" src/state/` (expect: one definition in mod.rs + call sites in
  issues_ops.rs and prs_ops.rs) and confirm `navigate_repo_*_in_prs_mode` bodies are thin wrappers
  (`if self.move_repo_selection(dir) { self.reset_prs_for_repo_change() }`) with NO duplicated
  remember/move/restore logic. Existing issues repo-nav tests remain green.
- [ ] `apply_pr_list_loaded` renders all N rows (#54) and discards stale (NFR-002).
- [ ] Composer open → subfocus NewComment + follow (#56); comment-created follows viewport.
- [ ] Filter draft updates live; Apply commits + reloads (#38/#40); CycleFilterState order correct.
- [ ] Scroll bounded by rendered length using viewport prop (#37/#39).
- [ ] SHARED viewport helpers (Finding 2): `list_first_visible_index`/`list_visible_window` are
  implemented in `src/layout.rs` (NOT a UI file; there is NO `src/ui/components/list_viewport.rs`),
  carry `@plan/@requirement REQ-PR-006/@pseudocode component-001 lines 182-196` markers, and match
  the pseudocode (selection stays on-screen; window length == `min(viewport_rows, len)` and always
  contains `selected_index`). The P04 RED helper tests now PASS. The state reducers consume them via
  `crate::layout::list_first_visible_index` and the state layer imports NO `crate::ui`. Cite
  `src/layout.rs:line` and the now-green test names.
- [ ] No silent None arms (cite error-surfacing arms).
- [ ] `apply_pr_open_in_browser`/`_failed` (REQ-PR-012) are PURE: `OpenInBrowser` sets the opening (or
  `NoSelectionToOpen`) notice and `OpenInBrowserFailed` sets a scoped error notice — NO `gh`/process
  launch in the reducer (the `gh pr view --web` side effect belongs to the dispatch layer). Cite the
  hub chaining (`apply_pr_open_browser_event`, c001 L383) and confirm no I/O.

## Runtime-Path Reachability
- [ ] Trace: `AppEvent::PrListEnter` → `AppMessage::PullRequests(Enter)` →
  `apply_message` PullRequests arm → `apply_prs_message` → reducer transition (cite each hop).
- [ ] The message↔event conversion round-trip is now GREEN (finding #1): the P04 RED tests
  `test_pr_show_notice_round_trips_and_sets_draft_notice`, `test_open_in_browser_events_round_trip`,
  and `test_appevent_pullrequestsmessage_round_trip` PASS against the P05 `prs_conversion.rs`
  implementation. Confirm the conversion is implemented exactly once here (not in P11) — cite the
  test names and `src/messages/prs_conversion.rs:line`.

## Contradiction Scan
- [ ] No transition mutates I/O or spawns tasks (reducer is pure).
- [ ] No function exceeds clippy thresholds (clippy green proves it).

## No-Placeholder Verification
HARD inverted gate (finding #6) — absence passes, presence fails:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" src/state/prs_*.rs src/messages/prs_conversion.rs ; then
  echo "FAIL: deferred-implementation marker present after impl phase"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing Issues/dashboard reducer behavior unchanged (their tests green).
- [ ] Persisted schema unchanged (serde round-trip test green).

## Success Criteria
- `Phase 05: PASS` with cited proofs + runtime trace, or `FAIL` with remediation.

## Failure Recovery
- Return to P05; bisect between P04A and P05 if needed.

## Phase Completion Marker (`.completed/P05A.md`)
Phase ID, timestamp, test pass counts, cited proofs, no-placeholder output, verdict.
