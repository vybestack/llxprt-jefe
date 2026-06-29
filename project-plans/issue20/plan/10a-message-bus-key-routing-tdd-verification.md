# Phase 10A — Message Bus & Key Routing TDD Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P10A
- **Prerequisites:** `.completed/P10.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify routing/precedence/suppression/round-trip tests are behavioral, cover every precedence level
and regression guard, and are RED for the right reasons — with cited evidence.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (test modules compile and
   register; markers present; ≥1 RED failure present; every precedence level has a test).
2. **Behavioral code-reading evidence (file:line)** — cite each behavioral routing/precedence/
   suppression/round-trip RED test by `file:line` and its precise assertion (the RED "behavioral
   evidence" is the failing assertion proving missing routing behavior). See Semantic checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": tests drive the real key-routing
   entry and the `AppEvent`↔`PullRequestsMessage` round-trip, not private shims.
4. **Contradiction scan** — see "Contradiction Scan" (no test contradicts a documented precedence or
   suppression invariant).
5. **Atomic verdict** — `Phase 10: PASS` (RED demonstrated for the right reasons) or `Phase 10:
   FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of routing tests for REQ-PR-001,002,003,004,008,010,011
- **Behavior contract:** GIVEN P10, WHEN verified, THEN each precedence level + regression (#38/#40,
  #47, #56) has a dedicated test and failures stem from unimplemented handlers.

## Implementation Tasks
- **Files to create:** `.completed/P10A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

The verifier re-runs the COMPLETE baseline. Because P10 is a TDD(RED) phase, the RED exception
applies to exactly ONE command — `cargo test` — which MUST report ≥1 failure. All other gates
MUST pass (the RED tests must COMPILE; only their assertions may fail):
```bash
cargo fmt --all --check                                              # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                  # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                      # MUST pass (RED tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p10a.log  # EXPECTED to FAIL (RED)
# RED confirmation (inverted, finding #3): rg exits nonzero on no-match, so assert PRESENCE:
if ! rg -q "test result: FAILED" /tmp/p10a.log ; then
  echo "FAIL: no failing test found — RED not demonstrated"; exit 1
fi
rg -n "@requirement REQ-PR-" src/app_input/
# Vacuous/ignored test HARD gate (inverted, finding #3) — presence FAILS:
if rg -n "assert!\(true\)|#\[ignore\]" src/app_input/ ; then
  echo "FAIL: vacuous assert!(true) or #[ignore] present in src/app_input tests"; exit 1
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
RED exception: ONLY `cargo test` may fail, and only because the behavioral routing/precedence tests
are unimplemented. If fmt, clippy, `check-clippy-allows.sh`, or build fail, the phase is a FAIL (RED
tests must compile and the codebase must remain clippy/format clean). `check-clippy-allows.sh` is the
AUTHORITATIVE no-allow/no-expect hard gate (finding #6) and is enforced even in the RED phase.

## Structural Verification Checklist
- [ ] Tests compile; ≥1 RED; markers present.

## Semantic Verification Checklist (Mandatory) — cite test names
- [ ] Each of the 8 precedence levels has ≥1 test.
- [ ] #47 repo-nav test asserts selection change independent of pane_focus.
- [ ] #38/#40 filter test asserts each field mutates draft + Apply reloads.
- [ ] #56 composer test asserts subfocus → NewComment on open.
- [ ] `o` open-in-browser test asserts the key emits `PrOpenInBrowser` from pr_list/pr_detail and a
  `NoSelectionToOpen` notice when no PR is selected (REQ-PR-012) — never a silent no-op.
- [ ] Read-only `e`/`r`/`c` on review/check subfocus assert the key is CONSUMED and a notice is
  surfaced (no silent `None`) — finding #4.
- [ ] **Consumed-no-op vs consumed-with-notice are distinct and both proven** (c003 "Consumed-no-op
  semantics", L40-42, 219-265):
  - The read-only `r`/`c`/`e` tests and `test_o_with_no_selection_emits_show_notice_not_none` assert
    the **consumed-with-notice** outcome: return value is `Some(PrShowNotice{kind})` AND (paired with
    P04's `test_show_notice_sets_draft_notice_for_each_readonly_hint_kind`) `draft_notice` is
    populated — NEVER a bare `None`.
  - `test_suppressed_keys_ctrl_d_ctrl_k_l_consumed_noop` asserts the **consumed-and-silently-ignored**
    outcome: `s`/`Ctrl-d`/`Ctrl-k`/`l` resolve to `KeyHandling::Handled(None)` — consumed (no
    fallthrough), NO emitted event, NO state change. There is NO `AppEvent::Noop` sentinel; `None`
    means `Handled(None)` and is used ONLY for intentionally-inert keys, never for the read-only
    `r`/`c`/`e`/`o` cases.
  - Confirm no test treats a suppressed-key `None` as carrying a user-visible effect, and no
    read-only `r`/`c`/`e`/`o` test accepts `None`.
- [ ] Esc-precedence test asserts full unwind order.
- [ ] NO `AppEvent`↔`PullRequestsMessage` round-trip CONVERSION test is present in this phase — that
  conversion is owned by the domain-state slice (RED in P04, GREEN in P05) per finding #1. Confirm
  the `o`/notice tests here assert EMITTED `AppEvent`s (routing), not message conversion.

## Runtime-Path Reachability
- [ ] Tests drive the real `handle_prs_mode_key` (not reimplemented routing).

## Contradiction Scan
- [ ] No test asserts a key both suppressed and acted-upon.

## Deferred Implementation Detection
HARD inverted gate (finding #6) — absence passes, presence fails. INCLUDES `todo!(`/`unimplemented!(`:
P09 stub bodies (handlers AND dispatch helpers) are TOTAL and clippy-clean — they contain NO
`todo!()`/`unimplemented!()` (findings #1 & #4; clippy denies both macros), so this gate trips on any
occurrence:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" src/app_input/prs*.rs ; then
  echo "FAIL: deferred-implementation marker present in app_input PR files"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing app_input tests still pass.

## Success Criteria
- `Phase 10: PASS` with RED evidence + level→test table, or `FAIL`.

## Failure Recovery
- Return to P10.

## Phase Completion Marker (`.completed/P10A.md`)
Phase ID, timestamp, RED list, level→test table, verdict.
