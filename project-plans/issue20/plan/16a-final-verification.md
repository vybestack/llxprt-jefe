# Phase 16A — Final Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P16A
- **Prerequisites:** `.completed/P16.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Final behavioral audit of the whole PR-Mode feature: prove every requirement is reachable and
correct via cited runtime paths, confirm every phase completed, verify enum preservation and the
absence of overrides, and render the atomic plan-level verdict.

## Verifier Output Contract (complete — finding #3)

This final verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract"
at whole-feature scope (every item fully required, none N/A):
1. **Structural verification** — enum preservation gate, no-override gate, phase-completion audit,
   and the full `make ci-check` baseline (see "Enum-Preservation Gate", "No-Override Gate", "Phase
   Completion Audit", "Final Verification Commands").
2. **Behavioral code-reading evidence (file:line)** — see "Final Verifier Behavioral Audit (cited
   proofs required)": every audit item carries a cited `file:line` proof of realized behavior.
3. **Runtime-path reachability** — each audit item traces its key → route → AppEvent → AppMessage →
   dispatch → apply → render path (cited), proving reachability end-to-end.
4. **Contradiction scan** — see the "Contradiction scan" audit item (no conflicting transition/
   render path).
5. **Atomic verdict** — the plan-level `PLAN-20260624-PR-MODE: PASS`/`FAIL` (see Success Criteria).

## Requirements Implemented (Expanded)

### Final acceptance of REQ-PR-001..014, REQ-PR-NFR-001..003
- **Behavior contract:** GIVEN all phases complete, WHEN the final auditor runs, THEN each
  requirement is demonstrably satisfied with cited proofs and no contradiction remains.

## Implementation Tasks
- **Files to create:** `.completed/P16A.md`.
- **Files to modify:** `plan/00-overview.md` execution tracker (mark all phases verified).

## Final Verifier Behavioral Audit (cited proofs required)
- [ ] **Entry reaches mode:** `p` from Dashboard → `DashboardPullRequests` rendered (cite key→render).
- [ ] **Loading can't be stuck:** every loader has a failure/panic path clearing the loading flag
  (cite each).
- [ ] **Detail completion:** Enter → detail load → render with reviews+checks summary (cite).
- [ ] **Scope switch invalidates + reloads:** repo change resets + reloads, stale discarded (cite).
- [ ] **Composer focus + follow:** `c` sets subfocus NewComment, composer visible, new comment
  follows (#56) (cite).
- [ ] **Filter interactivity:** every field mutates draft; Apply reloads (#38/#40) (cite).
- [ ] **Repo nav:** Up/Down in RepoList changes repo independent of pane_focus (#47) (cite).
- [ ] **List integrity:** N loaded → N rendered; selection stays visible (#54/#55) (cite).
- [ ] **Non-blocking I/O:** no synchronous gh call on UI thread (#37/#39, NFR-001) (cite).
- [ ] **No silent drops:** every failure surfaces a scoped error/log (cite).
- [ ] **Deferred ops / open-in-browser:** in-app merge/approve/review-submit are absent; `o` opens
  the selected PR in the browser by spawning `gh pr view <number> --repo <owner>/<name> --web` via
  `spawn_gh_task_with_panic` (off-thread), with `NoSelectionToOpen` notice when nothing is selected
  and `PrOpenInBrowserFailed` on error (no silent drop); `external_url` remains display-only
  (REQ-PR-012) (cite the routing arm, dispatch helper, gh boundary call).
- [ ] **Missing config inline error:** no slug → scoped message, no spawn (cite).
- [ ] **Mockup placement:** sidebar 22u + two-column + regions (cite layout test).
- [ ] **Contradiction scan:** no conflicting transition/render path found.

## Phase Completion Audit
```bash
for p in P00A P01 P01A P02 P02A P03 P03A P04 P04A P05 P05A P06 P06A P07 P07A \
         P08 P08A P09 P09A P10 P10A P11 P11A P12 P12A P13 P13A P14 P14A \
         P15 P15A P16; do
  test -f "project-plans/issue20/.completed/$p.md" && echo "$p OK" || echo "$p MISSING"
done
```

## Enum-Preservation Gate
```bash
rg -n "DashboardPullRequests" src/state/types.rs
rg -n "MessageDomain::PullRequests|PullRequestsMessage|AppMessage::PullRequests" src/messages.rs
# existing variants intact:
rg -n "Dashboard,|Split,|DashboardIssues" src/state/types.rs
```

## No-Override Gate

Enforce exact thresholds in BOTH clippy configs, zero allow/expect attributes in every spelling,
and decomposition over override.
```bash
# (a) EXACT thresholds unchanged in BOTH the root and CI clippy configs.
for cfg in clippy.toml .github/clippy/clippy.toml; do
  grep -E '^[[:space:]]*cognitive-complexity-threshold[[:space:]]*=[[:space:]]*15([[:space:]]|#|$)'  "$cfg" || { echo "FAIL cognitive != 15 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-lines-threshold[[:space:]]*=[[:space:]]*60([[:space:]]|#|$)'        "$cfg" || { echo "FAIL lines != 60 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-arguments-threshold[[:space:]]*=[[:space:]]*6([[:space:]]|#|$)'     "$cfg" || { echo "FAIL args != 6 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*type-complexity-threshold[[:space:]]*=[[:space:]]*250([[:space:]]|#|$)'      "$cfg" || { echo "FAIL type-complexity != 250 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*max-struct-bools[[:space:]]*=[[:space:]]*3([[:space:]]|#|$)'                 "$cfg" || { echo "FAIL struct-bools != 3 in $cfg"; exit 1; }
done
# (b) Zero first-party clippy allow/expect attributes. `bash scripts/check-clippy-allows.sh` is
# AUTHORITATIVE (it also asserts the two configs sync). The inverted multi-pattern block below is the
# SAME defense-in-depth gate used in P16 — it FAILS (nonzero) if ANY first-party allow/expect
# attribute exists in ANY spelling; ZERO matches passes (finding #6 — no non-inverted greps).
bash scripts/check-clippy-allows.sh
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
# (c) No threshold edits in the working tree — HARD gate (finding #1). ANY modification to EITHER
# clippy config file fails the phase (mirrors the P16 gate; not a report-only `git diff --stat`).
git diff --quiet -- clippy.toml .github/clippy/clippy.toml || { echo "FAIL: clippy config modified"; exit 1; }
# (d) Cargo.toml [lints.clippy] no-weaken gate (finding #2). check-clippy-allows.sh does NOT inspect
# Cargo.toml's `[lints.clippy]` table, so a PR could relax lint levels HERE. Cargo.toml currently has
# a top-level `[lints.clippy]` table with SIX pre-existing `= "allow"` relaxations (baseline). FAIL
# if THIS branch ADDS a new `allow` or DOWNGRADES an existing deny/warn to allow (scoped to added
# lines; removing/tightening an allow is permitted and never fails).
added_lints_allows="$(git diff main -- Cargo.toml \
  | grep -E '^\+' | grep -Ev '^\+\+\+' \
  | grep -E '=[[:space:]]*"allow"|level[[:space:]]*=[[:space:]]*"allow"')"
if [ -n "$added_lints_allows" ]; then
  echo 'FAIL: this branch ADDS/weakens a Cargo.toml lint to allow (finding #2):'
  echo "$added_lints_allows"
  exit 1
fi
```
Any threshold raise, any allow/expect attribute, or any Cargo.toml `[lints.clippy]` allow-weakening
is a hard FAIL; handlers/render fns must be split to fit `too-many-lines-threshold = 60` and
`cognitive-complexity-threshold = 15` instead.

## Final Verification Commands

Run the COMPLETE baseline explicitly (all gates MUST pass — this is the final GREEN audit, no RED
exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
make ci-check   # superset: fmt, clippy, coverage (--fail-under-lines 30), build, test
```

### Final deferred-implementation gate — SCOPED to PR-owned changes only (finding #1)

HARD inverted gate; absence passes, presence fails. It must NOT grep all of `src/`: the current
source contains PRE-EXISTING unrelated markers this PR does not own — verified present at
`src/state/types.rs:211` (`TODO(issue #24)`), `src/state/types.rs:220`
(`Placeholder for future multi-issue handling`), and `src/persistence/mod.rs:559`
(`// For now, ...`). No `todo!()`/`unimplemented!()`/placeholder introduced by THIS PR may survive
the final audit, but pre-existing unrelated markers are out of scope and must not be flagged/edited.
```bash
set -euo pipefail
PR_NEW_FILES=(
  src/github/parse_pr.rs
  src/state/prs_ops.rs src/state/prs_load_ops.rs src/state/prs_inline_ops.rs src/state/prs_mutation_ops.rs
  src/messages/prs_conversion.rs
  src/app_input/prs.rs src/app_input/prs_dispatch.rs src/app_input/prs_list_dispatch.rs src/app_input/prs_filter.rs src/app_input/prs_mutation.rs
  src/ui/components/pr_list.rs src/ui/components/pr_detail.rs src/ui/components/pr_filter_controls.rs
  src/ui/screens/pull_requests.rs
)
# NOTE (finding #2): viewport helpers live in src/layout.rs (a SHARED file scanned for ADDED markers
# below) — there is NO list_viewport.rs.
PR_SHARED_FILES=(
  src/state/types.rs src/state/mod.rs src/input.rs src/messages.rs
  src/app_input/mod.rs src/app_input/normal.rs src/github/mod.rs src/layout.rs
  src/domain/mod.rs src/ui/orchestration.rs src/ui/mod.rs src/ui/components/mod.rs src/ui/screens/mod.rs src/lib.rs
)
DEFERRED_RE='TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now'
for f in "${PR_NEW_FILES[@]}"; do
  [ -f "$f" ] || continue
  if rg -n "$DEFERRED_RE" "$f" ; then echo "FAIL: deferred marker in new PR file $f"; exit 1; fi
done
for f in "${PR_SHARED_FILES[@]}"; do
  [ -f "$f" ] || continue
  if git diff main -- "$f" | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
    echo "FAIL: deferred marker ADDED by this branch in shared file $f"; exit 1
  fi
done
```
Every command above MUST pass. No RED exception applies at the final audit; any failure ⇒ `FAIL`.

## Success Criteria
- Atomic verdict `PLAN-20260624-PR-MODE: PASS` ONLY if every audit item has a cited proof, all
  phase markers exist, enums preserved, no overrides, and `make ci-check` is green. Any missing
  proof ⇒ `FAIL` with remediation routed to the owning phase.

## Failure Recovery
- Route each failing audit item to its owning phase; re-run from there; re-verify.

## Phase Completion Marker (`.completed/P16A.md`)
Phase ID, timestamp, full audit with cited proofs, phase-completion audit output, enum/override gate
results, ci-check output, atomic plan verdict.
