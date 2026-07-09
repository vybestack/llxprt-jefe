# Phase 03A — Domain & State Stub Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P03A
- **Prerequisites:** `.completed/P03.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the stub compiles, introduces the full PR type surface, preserves all existing variants and
the persisted-state schema, and carries traceability markers — with cited evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (new type surface present;
   existing variants intact; persisted schema unchanged; traceability markers present).
2. **Behavioral code-reading evidence (file:line)** — full REQ-behavior code-reading is **N/A —
   stub phase** (stubs are total/wrong-value and assert NO behavior yet). The analogous evidence is
   cited `file:line` proof that each stubbed symbol exists with the correct signature and that the
   bodies are inert (no logic), per the "Compile-Only / No-Behavioral-Change Gate".
3. **Runtime-path reachability** — see "Runtime-Path Reachability": the skeleton chain is wired
   enough to compile; cite each stubbed hop. (No live behavior yet — stub phase.)
4. **Contradiction scan** — see "Contradiction Scan" (no stub silently alters existing behavior; no
   duplicated constant/macro override).
5. **Atomic verdict** — `Phase 03: PASS` or `Phase 03: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of type surface for REQ-PR-001,003,006,008,009,010,014
- **Behavior contract:** GIVEN P03, WHEN verified, THEN every new type/variant exists, every
  existing variant is intact, the persisted schema is unchanged, and the workspace is green.

## Implementation Tasks
- **Files to create:** `.completed/P03A.md`.
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
rg -n "DashboardPullRequests" src/state/types.rs src/input.rs src/ui/orchestration.rs
rg -n "PullRequestsState|PrFocus|PrDetailSubfocus|prs_state" src/state/types.rs
rg -n "PullRequest\b|PullRequestDetail|PrReview|PrCheck|PrState|PrFilter" src/domain/mod.rs
rg -n "MessageDomain::PullRequests|PullRequestsMessage|AppMessage::PullRequests" src/messages.rs
rg -n "apply_prs_message|reset_prs_for_repo_change" src/state/mod.rs
# Traceability-marker HARD gate (finding #1): every NEW P03 deliverable file MUST carry ALL THREE
# marker types (@plan/@requirement/@pseudocode). Missing ANY one in ANY file is a hard FAIL.
PLAN_RE='@plan PLAN-20260624-PR-MODE\.P[0-9]+'
REQ_RE='@requirement REQ-PR-(NFR-)?[0-9]+'
PSEUDO_RE='@pseudocode component-[0-9]+ lines [0-9]+-[0-9]+'
marker_fail=0
for f in \
  src/state/prs_ops.rs src/state/prs_load_ops.rs src/state/prs_inline_ops.rs src/state/prs_mutation_ops.rs \
  src/messages/prs_conversion.rs ; do
  [ -f "$f" ] || { echo "$f: MISSING FILE"; marker_fail=1; continue; }
  miss=""
  rg -q "$PLAN_RE"   "$f" || miss="$miss @plan"
  rg -q "$REQ_RE"    "$f" || miss="$miss @requirement"
  rg -q "$PSEUDO_RE" "$f" || miss="$miss @pseudocode"
  [ -n "$miss" ] && { echo "$f: MISSING MARKER(S):$miss"; marker_fail=1; } || echo "$f: all three markers present"
done
[ "$marker_fail" -eq 0 ] || { echo "FAIL: one or more P03 deliverable files missing required markers"; exit 1; }
```

## Structural Verification Checklist
- [ ] Build green.
- [ ] All new identifiers present (cite file:line for each).
- [ ] Existing `ScreenMode`/`InputMode`/`AppEvent`/`IssuesMessage` variants unchanged (cite).
- [ ] Markers present in every changed file (grep count > 0 per file).

## Semantic Verification Checklist (Mandatory)
- [ ] Persisted-state struct does not gain `prs_state` (cite the persisted mapping).
- [ ] `IssueComment` reused for PR comments (no new comment type) — cite `PullRequestDetail`.
- [ ] NO `todo!()`/`unimplemented!()` appears ANYWHERE in P03-authored source — clippy denies both
  macros (`Cargo.toml:63-64`), so the stub phase's clippy gate would fail on their presence. Every
  stub (reducers, the `src/layout.rs` viewport helpers, the `prs_conversion` bodies, the
  `build_screen_element` placeholder arm) is TOTAL, clippy-clean, and panic-free. The viewport/
  conversion stubs return WRONG/empty values; `apply_prs_message` returns `true` from a no-op match
  but mutates NO state — so the P04 RED tests fail by assertion (wrong value or unchanged state),
  NOT by panic (findings #1 & #4).
- [ ] STATE-layer boundary holds (finding #2): the relocated viewport helpers live in `src/layout.rs`
  (shared leaf), so `src/state/prs_*.rs` consume `crate::layout::list_first_visible_index` /
  `list_visible_window`; the state layer never imports `crate::ui`, and no
  `src/ui/components/list_viewport.rs` file exists.
- [ ] Default `PrFilterState == Open` (cite `impl Default`).

## Single-Owner File Boundary (finding #2)
- [ ] P03 did NOT create any `src/app_input/prs*.rs` file (those are P09-owned). Confirm absence:
  ```bash
  if ls src/app_input/prs*.rs 2>/dev/null ; then echo "FAIL: P03 created app_input PR files (P09-owned)"; exit 1; fi
  ```
- [ ] P03 did NOT create any `src/ui` PR file (`pr_list.rs`/`pr_detail.rs`/`pr_filter_controls.rs`/
  `screens/pull_requests.rs` are P12-owned). There is NO `src/ui/components/list_viewport.rs` file in
  this plan (finding #2 — the viewport helpers live in `src/layout.rs`). Confirm absence:
  ```bash
  if ls src/ui/components/pr_*.rs src/ui/screens/pull_requests.rs 2>/dev/null ; then
    echo "FAIL: P03 created UI PR files (P12-owned)"; exit 1; fi
  if [ -f src/ui/components/list_viewport.rs ]; then
    echo "FAIL: src/ui/components/list_viewport.rs must not exist (helpers live in crate::layout)"; exit 1; fi
  ```
- [ ] P03 did NOT create `src/github/parse_pr.rs` (P06-owned). Confirm absence:
  ```bash
  if ls src/github/parse_pr.rs 2>/dev/null ; then echo "FAIL: P03 created github PR file (P06-owned)"; exit 1; fi
  ```
- [ ] The `build_screen_element` `DashboardPullRequests` arm is a BENIGN placeholder
  (`element! { View {} }`), NOT the real `PullRequestsScreen` (which is P12-owned). Cite the arm.

> NOTE: The gh-client boundary import constraint (the new PR gh-client modules must NOT import
> `crate::state`/`crate::ui`/`crate::app_input`) is NOT in scope for P03 — P03 adds domain/state/
> message stubs, not the gh-client. That boundary check is enforced in P06A against the new
> `src/github/parse_pr.rs` + PR methods in `src/github/mod.rs`.

## Compile-Only / No-Behavioral-Change Gate (finding #3)
- [ ] `select_repository_by_index` does NOT call `reset_prs_for_repo_change` in P03 (the repo-scope
  reset wiring is GREEN/P05). Confirm absence in the P03 diff:
  ```bash
  if git diff main -- src/state/mod.rs | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E 'reset_prs_for_repo_change' ; then
    echo "FAIL: P03 wired reset_prs_for_repo_change into select_repository_by_index (defer to P05)"; exit 1
  fi
  ```
  (`reset_prs_for_repo_change` MUST still exist as a no-op SIGNATURE in `src/state/prs_ops.rs`; it is
  simply not yet called.)
- [ ] `input_mode_for_state`'s `DashboardPullRequests` arm is compile-only: it returns a fixed
  default (`InputMode::PrsNormal`) with NO Inline/Chooser/Search/Filter precedence branching (that
  routing is GREEN/P11). Cite the arm and confirm it contains no `inline_state`/`agent_chooser`/
  `search_input_focused`/`filter_ui` inspection.
- [ ] NO new backward-compat persisted-state TEST is authored in P03 (finding #3). That test
  (`test_pre_pr_persisted_state_loads_with_inactive_prs_state`) is a P04 RED test; P03 only confirms
  structurally that `prs_state` is absent from the persisted DTO. Confirm P03 added no such test.

## Runtime-Path Reachability
- [ ] `build_screen_element` has a `DashboardPullRequests` arm (cite) — render reachable once impl
  lands; the P03 arm is a benign empty view, never `todo!()`.
- [ ] `apply_message` has a `PullRequests` arm (cite) — reducer reachable; it calls
  `apply_prs_message`, whose P03 stub returns `true` from a TOTAL no-op match (safe default, NOT
  `todo!()`). The arm has NO `debug_assert!(handled)` in P03 (finding #4 — deferred to the GREEN
  domain-state phase P05, which owns `src/state/mod.rs` and `apply_prs_message`).
  Confirm the P03 `apply_message` arm does not contain `debug_assert!`:
  ```bash
  if git diff main -- src/state/mod.rs | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E 'debug_assert!' ; then
    echo "FAIL: P03 added debug_assert! to the PullRequests apply_message arm (defer to P05)"; exit 1
  fi
  ```
- [ ] Because the stub `apply_prs_message` returns `true` and adds no `debug_assert!`, dispatching a
  `PullRequests(...)` message is panic-free in debug/test builds. P03 tests still assert only on
  structure/compilation (no behavioral dispatch is required to avoid a panic).
- [ ] NO `todo!()`/`unimplemented!()` appears ANYWHERE in P03-authored source (findings #1 & #4 —
  clippy denies both macros, so their mere presence would fail the stub phase's clippy gate). HARD
  inverted gate (scans ALL P03 PR surfaces, including the `prs_conversion` bodies and the
  `src/layout.rs` helper stubs — absence passes, presence fails):
  ```bash
  if rg -n "todo!\(\)|unimplemented!\(\)" \
       src/state/prs_*.rs src/messages/prs_conversion.rs src/layout.rs src/ui/orchestration.rs ; then
    echo "FAIL: todo!()/unimplemented!() present (clippy denies both; P03 stubs must be total)"; exit 1
  fi
  ```

## Contradiction Scan
- [ ] No exhaustive match left non-updated (build would fail otherwise — confirm clean build).
- [ ] No existing variant silently changed.

## Deferred Implementation Detection
Stub phase: P03 introduces ZERO `todo!()`/`unimplemented!()` (findings #1 & #4 — clippy denies both
macros), so the macro scan is itself a HARD inverted gate (run above in Runtime-Path Reachability).
The NON-macro deferred markers in P03-owned PR code are an additional HARD inverted gate (finding
#6 — absence passes, presence fails). NEW PR files are scanned in full; SHARED modified files
(`src/domain/mod.rs`, `src/messages.rs`, `src/input.rs`, `src/state/types.rs`, `src/state/mod.rs`,
`src/layout.rs`) are scanned ONLY for markers THIS branch ADDED (git diff main added lines), so
pre-existing unrelated markers (e.g. `src/state/types.rs:211/220`, `src/persistence/mod.rs:559`) are
never flagged:
```bash
DEFERRED_RE='TODO|FIXME|HACK|placeholder|for now|will be implemented'
for f in src/messages/prs_conversion.rs src/state/prs_ops.rs src/state/prs_load_ops.rs \
         src/state/prs_inline_ops.rs src/state/prs_mutation_ops.rs; do
  [ -f "$f" ] || continue
  if rg -n "$DEFERRED_RE" "$f" ; then echo "FAIL: stray deferred marker in new PR file $f"; exit 1; fi
done
for f in src/domain/mod.rs src/messages.rs src/input.rs src/state/types.rs src/state/mod.rs src/layout.rs; do
  [ -f "$f" ] || continue
  if git diff main -- "$f" | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
    echo "FAIL: deferred marker ADDED by this branch in shared file $f"; exit 1
  fi
done
```

## Integration Contract Acceptance Gates
- [ ] Old behavior preserved: Dashboard/Issues/Split flows unaffected (no diff to their arms).
- [ ] Backward-compat: persisted schema unchanged (structural — the behavioral round-trip TEST is
  authored in P04, not P03; finding #3).

## Success Criteria
- `Phase 03: PASS` with cited evidence, or `FAIL` with remediation.

## Failure Recovery
- Return to P03; fix stubs.

## Phase Completion Marker (`.completed/P03A.md`)
Phase ID, timestamp, cited evidence, verdict, semantic summary.
