# Phase 09A — Message Bus & Key Routing Stub Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P09A
- **Prerequisites:** `.completed/P09.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the routing/dispatch surface compiles, wires `p` entry + mode delegation, preserves existing
key routing, and carries markers — with cited evidence and a runtime-path skeleton trace.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract":
1. **Structural verification** — see "Structural Verification Checklist" (routing/dispatch surface
   present; existing key routing intact; markers present).
2. **Behavioral code-reading evidence (file:line)** — full REQ-behavior code-reading is **N/A —
   stub phase** (handlers are inert delegators that assert no routing behavior yet). The analogous
   evidence is cited `file:line` proof the `p` entry arm and mode-delegation skeleton exist with the
   correct signatures and inert bodies.
3. **Runtime-path reachability** — see "Runtime-Path Reachability (skeleton)": the key → route →
   delegate skeleton compiles and is wired; cite each stubbed hop. (No live behavior yet.)
4. **Contradiction scan** — see "Contradiction Scan" (no existing routing arm altered/dropped; `p`
   only intercepted in `Dashboard`; no duplicated dispatch).
5. **Atomic verdict** — `Phase 09: PASS` or `Phase 09: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of routing/dispatch surface for REQ-PR-001,002,003,004,010,011,012
- **Behavior contract:** GIVEN P09, WHEN verified, THEN the key→event→dispatch skeleton exists and
  existing Dashboard/Issues routing is untouched.
- **REQ-PR-012 stub presence:** the `o`/open-in-browser routing skeleton MUST exist as compiling
  signatures: `handle_pr_list_key`/`handle_pr_detail_key` reference the `PrOpenInBrowser` /
  `PrShowNotice{ kind: NoSelectionToOpen }` event paths, the `PullRequests(OpenInBrowser)` dispatch
  arm routes to `prs_dispatch::dispatch_pr_open_in_browser`, and `dispatch_pr_open_in_browser` /
  `pr_open_in_browser_info` signatures exist with BENIGN NO-OP stub bodies (return without side
  effect / `Err(RepoContextError::NoSelection)` or similar safe default) — NEVER `todo!()`/
  `unimplemented!()` (clippy denies both macros, so P09 forbids them in ALL `src/app_input/prs*.rs`).
  Behavior is verified later (P10 RED → P11 GREEN); P09A only proves the surface compiles, is wired,
  and is panic-free.

## Implementation Tasks
- **Files to create:** `.completed/P09A.md`.
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

Then the phase-specific structural greps (each is a presence GATE — fail nonzero if the required
symbol is absent):
```bash
rg -n "Char\('p'\|'P'\).*EnterPrsMode" src/app_input/normal.rs
rg -n "handle_dashboard_prs_key|handle_prs_mode_key" src/app_input/normal.rs src/app_input/prs.rs
rg -n "AppMessage::PullRequests" src/app_input/mod.rs
# Traceability-marker HARD gate (finding #1): every NEW P09 deliverable file MUST carry ALL THREE
# marker types (@plan/@requirement/@pseudocode). Missing ANY one in ANY file is a hard FAIL.
PLAN_RE='@plan PLAN-20260624-PR-MODE\.P[0-9]+'
REQ_RE='@requirement REQ-PR-(NFR-)?[0-9]+'
PSEUDO_RE='@pseudocode component-[0-9]+ lines [0-9]+-[0-9]+'
marker_fail=0
for f in \
  src/app_input/prs.rs src/app_input/prs_dispatch.rs src/app_input/prs_list_dispatch.rs \
  src/app_input/prs_filter.rs src/app_input/prs_mutation.rs ; do
  [ -f "$f" ] || { echo "$f: MISSING FILE"; marker_fail=1; continue; }
  miss=""
  rg -q "$PLAN_RE"   "$f" || miss="$miss @plan"
  rg -q "$REQ_RE"    "$f" || miss="$miss @requirement"
  rg -q "$PSEUDO_RE" "$f" || miss="$miss @pseudocode"
  [ -n "$miss" ] && { echo "$f: MISSING MARKER(S):$miss"; marker_fail=1; } || echo "$f: all three markers present"
done
[ "$marker_fail" -eq 0 ] || { echo "FAIL: one or more P09 deliverable files missing required markers"; exit 1; }

# REQ-PR-012 stub-presence GATES — these MUST be present (exit nonzero if any is missing):
for pat in \
  "PrOpenInBrowser" \
  "NoSelectionToOpen" \
  "dispatch_pr_open_in_browser" \
  "pr_open_in_browser_info" ; do
  if ! rg -q "$pat" src/app_input/ ; then
    echo "FAIL: REQ-PR-012 stub symbol missing: $pat"; exit 1
  fi
done
# The 'o' open-in-browser routing skeleton must live in the key handlers:
rg -n "PrOpenInBrowser|NoSelectionToOpen" src/app_input/prs.rs
# The dispatch arm must route OpenInBrowser to the dispatch helper:
if ! rg -q "PullRequests\(.*OpenInBrowser.*\)" src/app_input/mod.rs ; then
  echo "FAIL: REQ-PR-012 dispatch arm for OpenInBrowser missing"; exit 1
fi
```

## Structural Verification Checklist
- [ ] Build green; entry + delegation present (cite).
- [ ] Existing `i`/`s`/Esc/grab arms unchanged (cite).
- [ ] Dispatch PR arms exhaustive/compiling.
- [ ] REQ-PR-012 stub surface present (cite): `PrOpenInBrowser` + `NoSelectionToOpen` paths in
  `prs.rs` key handlers, the `PullRequests(OpenInBrowser)` dispatch arm in `mod.rs`, and the
  `dispatch_pr_open_in_browser`/`pr_open_in_browser_info` signatures with BENIGN NO-OP bodies
  (never `todo!()`/`unimplemented!()` — clippy denies both macros).
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] `p`/`P` gated on `screen == Dashboard` (cite).
- [ ] Delegation to `handle_prs_mode_key` only when `DashboardPullRequests` (cite).
- [ ] No handler mutates AppState directly (cite return types).
- [ ] No `todo!()`/`unimplemented!()` ANYWHERE in `src/app_input/prs*.rs` (findings #1 & #4 — clippy
  denies both macros; consistent with P09). Key handlers return safe
  `Option<AppEvent>` values, and the `PullRequests(...)` arms (including `OpenInBrowser`) route to
  BENIGN NO-OP stub bodies that return without side effect — NEVER `todo!()`/`unimplemented!()`. The
  dispatch helpers (`dispatch_pr_open_in_browser`, `pr_open_in_browser_info`, etc.) are likewise
  panic-free no-ops/safe defaults, not `todo!()`. HARD gate (scans ALL `prs*.rs` files — exit
  nonzero on ANY match):
  ```bash
  if rg -n "todo!\(\)|unimplemented!\(\)" src/app_input/prs*.rs ; then
    echo "FAIL: todo!()/unimplemented!() present in src/app_input/prs*.rs"; exit 1
  fi
  ```

## Runtime-Path Reachability (skeleton)
- [ ] Trace: key `p` → `resolve_mode_key` → `EnterPrsMode` → `.into()` →
  `AppMessage::PullRequests(EnterMode)` → `dispatch_app_message` PR arm (cite each hop; the arm body
  is a NO-OP, never `todo!()`, so dispatching it at stub time cannot panic).
- [ ] Confirm NO `src/app_input/prs*.rs` body contains `todo!()`/`unimplemented!()` at all (clippy
  denies both macros): every dispatch arm AND every dispatch helper is a benign NO-OP / safe default,
  so no dispatched `PullRequests(...)` message (and no startup path) can ever reach a `todo!()`.
  Behavior is filled in by the P10 RED → P11 GREEN cycle; the stubs remain panic-free no-ops.

## Contradiction Scan
- [ ] No existing routing arm altered.
- [ ] No duplicate handler for the same key/precedence level.

## Deferred Implementation Detection
Stub phase: `todo!()`/`unimplemented!()` are FORBIDDEN in ALL `src/app_input/prs*.rs` (clippy denies
both macros; consistent with P09), so the `todo!()` scan is a HARD inverted gate here (NOT
record-only); the other deferred markers are also a HARD inverted gate:
```bash
# Hard inverted gate for todo!()/unimplemented!() — absence passes, presence fails:
if rg -n "todo!\(\)|unimplemented!\(\)" src/app_input/prs*.rs ; then
  echo "FAIL: todo!()/unimplemented!() present in src/app_input/prs*.rs (must be benign no-ops)"; exit 1
fi
# Hard inverted gate for non-todo!() markers — absence passes, presence fails:
if rg -n "TODO|FIXME|HACK|placeholder|for now" src/app_input/prs*.rs ; then
  echo "FAIL: stray deferred marker (non-todo!()) in P09 stub"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Dashboard/Issues/Split key routing unaffected (their tests green).

## Success Criteria
- `Phase 09: PASS` with cited skeleton trace, or `FAIL`.

## Failure Recovery
- Return to P09.

## Phase Completion Marker (`.completed/P09A.md`)
Phase ID, timestamp, cited evidence, verdict.
