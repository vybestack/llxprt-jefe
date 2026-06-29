# Phase 11A — Message Bus & Key Routing Impl Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P11A
- **Prerequisites:** `.completed/P11.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify routing/dispatch/async I/O is correct, non-blocking, regression-guarded, placeholder/
override-free — with cited evidence and a full runtime-path trace.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract" (GREEN
implementation phase — every item is fully required, none N/A):
1. **Structural verification** — see "Structural Verification Checklist" (routing/dispatch/async
   helpers present; markers present; complexity within thresholds; existing routing intact).
2. **Behavioral code-reading evidence (file:line)** — cite `file:line` proving each routing/dispatch
   behavior is realized (precedence, suppression, round-trip, off-thread spawn). See Semantic
   checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability (FULL)": trace key → route →
   AppEvent → AppMessage → `dispatch_app_message` → `apply_message` → `apply_prs_message` (+ async
   spawn) → result, citing each hop's `file:line`.
4. **Contradiction scan** — see "Contradiction Scan": no synchronous gh call on the UI thread
   (NFR-001), no silent drop, repo-nav independent of `pane_focus` (#47).
5. **Atomic verdict** — `Phase 11: PASS` or `Phase 11: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of routing/dispatch impl for REQ-PR-001-004,008,010,011,012,013, NFR-001,003
- **Behavior contract:** GIVEN P11, WHEN verified, THEN the full key→event→message→dispatch→reducer
  chain is reachable, all gh I/O is off-thread, and regression guards hold.

## Implementation Tasks
- **Files to create:** `.completed/P11A.md`.
- **Files to modify:** `plan/00-overview.md` tracker.

## Verification Commands

Run the COMPLETE baseline (this is a GREEN verifier phase — every gate MUST pass, no RED exception):
```bash
cargo fmt --all --check                                              # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
cargo build --workspace --all-features --locked                      # MUST pass
cargo test --workspace --all-features --locked                       # MUST pass
bash scripts/check-clippy-allows.sh                                  # MUST pass
# Impl-verifier deferred-implementation HARD inverted gate (finding #6) — absence passes, presence fails:
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" src/app_input/prs*.rs ; then
  echo "FAIL: deferred-implementation marker present after impl phase"; exit 1
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
# Ensure no synchronous GhClient PR call on the UI thread (incl. open-in-browser, REQ-PR-012):
if rg -n "list_pull_requests|get_pull_request_detail|create_pr_comment|open_pull_request_in_browser" \
     src/app_input/ \
   | rg -v "spawn_gh_task_with_panic|prs_dispatch|prs_list_dispatch|prs_mutation" ; then
  echo "FAIL: synchronous GhClient PR call reachable on UI thread"; exit 1
fi
# REQ-PR-012: confirm the open-in-browser launch is wired off-thread via the dispatch helper.
rg -n "open_pull_request_in_browser" src/app_input/prs_dispatch.rs
rg -n "spawn_gh_task_with_panic" src/app_input/prs_dispatch.rs
# Exact clippy-threshold assertion (finding #4) — both configs keep EXACT values + unmodified tree.
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

## Semantic Verification Checklist (Mandatory) — cite file:line
- [ ] Each PR loader uses `spawn_gh_task_with_panic` with on_panic clearing loading (NFR-001).
- [ ] No UI-thread blocking gh call (cite grep result = empty).
- [ ] Async-side-effect TESTS assert OBSERVABLE STATE, not spawn recording (finding #5). The
  dispatch-ordering tests (`test_open_in_browser_sets_opening_notice_and_marks_pending`,
  `test_open_in_browser_no_selection_sets_notice_and_no_pending`,
  `test_pr_agent_chooser_confirm_applies_reducer_before_side_effects`) verify the synchronously
  pre-spawn state (notice + loading/pending flag) and filesystem effects (the written
  `{work_dir}/.jefe/pr-prompt.md`) — NOT a recorded/counted `spawn_gh_task_with_panic` invocation
  (no such recorder seam exists in the codebase; one must NOT be added). Confirm the tests contain no
  spawn-recorder/spawn-count construct:
  ```bash
  if rg -n "recorded.*spawn|spawn.*count|TaskSpawner|spawn_calls" src/app_input/app_input_tests.rs ; then
    echo "FAIL: dispatch-ordering tests rely on a non-existent spawn recorder (finding #5)"; exit 1
  fi
  ```
- [ ] 8-level precedence honored (cite handler ordering).
- [ ] `handle_dashboard_prs_key` is wired BEFORE `resolve_mode_key` in `handle_normal_key_event`;
  `p`/`P` in `DashboardPullRequests` resolves to `RefocusPrList` (never re-`EnterPrsMode`) — cite
  the chain ordering `file:line` (REQ-PR-001).
- [ ] Read-only `r`/`c`/`e` arms on review/check subfocus are CONSUMED and return
  `Some(PrShowNotice{kind})`; the reducer sets `prs_state.draft_notice` — cite each arm; confirm NO
  bare `None` drop (finding #4; REQ-PR-010/013).
- [ ] `o` open-in-browser: the reducer arm is PURE (sets a transient notice only), and the dispatch
  layer spawns `gh pr view <number> --repo <owner>/<name> --web` via `spawn_gh_task_with_panic`
  (NEVER on the UI thread); `NoSelectionToOpen` notice when no PR is selected; success →
  `PrOpenedInBrowser`, failure → `PrOpenInBrowserFailed{error}` (no silent drop) — cite the routing
  arm, the dispatch helper, and the gh boundary call (REQ-PR-012).
- [ ] Esc unwind order correct.
- [ ] #56 composer focus, #38/#40 filter interactivity, #47 repo nav, #37/#39 viewport prop — each
  cited.
- [ ] Suppressed keys consumed.

## Runtime-Path Reachability (FULL)
- [ ] Trace `p` → EnterPrsMode → AppMessage → dispatch (reload list) → spawn → PrListLoaded →
  apply_message → render (cite each hop).
- [ ] Trace Enter on PR → detail load → PrDetailLoaded → render.
- [ ] Trace `c` → composer → submit → spawn → PrCommentCreated → reducer append + follow.
- [ ] Trace `o` → PrOpenInBrowser → AppMessage::PullRequests(OpenInBrowser) → dispatch
  `dispatch_pr_open_in_browser` → spawn `gh pr view … --web` → PrOpenedInBrowser/PrOpenInBrowserFailed
  → reducer notice (cite each hop).

## Contradiction Scan
- [ ] No silent None arm drops a load failure (each failure delivers a scoped error).
- [ ] No function exceeds clippy thresholds (clippy green).

## No-Placeholder Verification
HARD inverted gate (finding #6) — absence passes, presence fails:
```bash
if rg -n "TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now" src/app_input/prs*.rs ; then
  echo "FAIL: deferred-implementation marker present after impl phase"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Dashboard/Issues/Split routing + dispatch unchanged (their tests green).

## Success Criteria
- `Phase 11: PASS` with full runtime traces + async evidence, or `FAIL`.

## Failure Recovery
- Return to P11.

## Phase Completion Marker (`.completed/P11A.md`)
Phase ID, timestamp, test counts, runtime traces, async evidence, verdict.
