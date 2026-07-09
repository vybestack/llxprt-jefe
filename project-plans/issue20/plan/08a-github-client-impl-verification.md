# Phase 08A — GitHub Client Impl Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P08A
- **Prerequisites:** `.completed/P08.md` exists.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Verify the PR gh client parses correctly, maps errors, stays sync + isolated, and is
placeholder/override-free — with cited evidence and a contradiction scan for the #37/#39 silent-None
regression.

## Verifier Output Contract (complete — finding #3)

This verifier MUST produce ALL FIVE items of the `00-overview.md` "Verifier Output Contract" (GREEN
implementation phase — every item is fully required, none N/A):
1. **Structural verification** — see "Structural Verification Checklist" (parse/arg/error fns
   present and boundary-isolated; markers present; complexity within thresholds).
2. **Behavioral code-reading evidence (file:line)** — cite `file:line` in `src/github/` proving each
   parse/arg/error behavior is realized (real GraphQL shape parsed, errors mapped to `GhError`). See
   Semantic checklist.
3. **Runtime-path reachability** — see "Runtime-Path Reachability": the sync boundary is reachable
   only off-thread via the dispatch helpers (`spawn_gh_task_with_panic`); cite the boundary entry.
4. **Contradiction scan** — see "Contradiction Scan": NO silent `None` arm dropping an
   unavailable-context case (#37/#39), no sync gh call on the UI thread, boundary imports clean.
5. **Atomic verdict** — `Phase 08: PASS` or `Phase 08: FAIL` with remediation (see Success Criteria).

## Requirements Implemented (Expanded)

### Verification of client impl for REQ-PR-006,007,009,010,012,013, NFR-001,003
- **Behavior contract:** GIVEN P08, WHEN verified, THEN parsing/arg/error behaviors are correct, no
  silent drops occur, and the boundary remains isolated and sync.

## Implementation Tasks
- **Files to create:** `.completed/P08A.md`.
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
deferred-implementation check is a HARD inverted gate — SCOPED to PR-owned changes (finding #1):
absence passes, presence fails. New file `src/github/parse_pr.rs` is scanned in full; shared file
`src/github/mod.rs` is scanned only for markers THIS branch ADDED (`git diff main` added lines), so
pre-existing unrelated text is ignored:
```bash
DEFERRED_RE='TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now'
if [ -f src/github/parse_pr.rs ] && rg -n "$DEFERRED_RE" src/github/parse_pr.rs ; then
  echo "FAIL: deferred-implementation marker present in new file src/github/parse_pr.rs"; exit 1
fi
if git diff main -- src/github/mod.rs | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
  echo "FAIL: deferred-implementation marker ADDED by this branch in src/github/mod.rs"; exit 1
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

## Semantic Verification Checklist (Mandatory) — cite file:line
- [ ] `list_pull_requests`/`get_pull_request_detail` follow the error idiom.
- [ ] `list_pr_comments` GraphQL query targets `repository.pullRequest(number:).comments` and NOT
  `repository.issue(number:)`; `get_pull_request_detail` sources its first comment page from
  `list_pr_comments` (Finding 1 / P00A §2d) — cite the method + the GREEN
  `test_list_pr_comments_query_targets_pull_request_not_issue`.
- [ ] `create_pr_comment` posts to the issues REST endpoint (valid for a PR number).
- [ ] Reviews/checks summaries parsed from `reviewDecision`/`statusCheckRollup`.
- [ ] `external_url` captured for display, and `open_pull_request_in_browser` spawns
  `gh pr view <number> --repo <owner>/<name> --web` (REQ-PR-012) — cite the method.
- [ ] Malformed/partial review or check entries are preserved as displayable degraded records
  (e.g. "(unknown reviewer)"/"(unparseable check)"), never silently dropped (finding #5; #37/#39).
- [ ] Methods sync; the NEW PR boundary modules do NOT import
  `crate::state`/`crate::ui`/`crate::app_input`; they MAY use `crate::domain`, `serde_json`,
  `std::process`, and sibling `crate::github` types (`GhError`, response/parse structs) — mirroring
  `src/github/parse.rs`.

## Runtime-Path Reachability
- [ ] Confirm these methods are invoked only via `spawn_gh_task_with_panic` callers (grep callers
  added in later phases; here confirm no direct UI-thread call exists yet).

## Contradiction Scan
- [ ] No `_ => None`/`_ => {}` arm silently discards an unavailable-context case (cite each match).
- [ ] No function exceeds clippy thresholds (clippy green).

## No-Placeholder Verification
HARD inverted gate — SCOPED to PR-owned changes (finding #1): absence passes, presence fails. New
file `src/github/parse_pr.rs` scanned in full; shared file `src/github/mod.rs` scanned only for
markers THIS branch ADDED (`git diff main` added lines):
```bash
DEFERRED_RE='TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now'
if [ -f src/github/parse_pr.rs ] && rg -n "$DEFERRED_RE" src/github/parse_pr.rs ; then
  echo "FAIL: deferred-implementation marker present in new file src/github/parse_pr.rs"; exit 1
fi
if git diff main -- src/github/mod.rs | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
  echo "FAIL: deferred-implementation marker ADDED by this branch in src/github/mod.rs"; exit 1
fi
```

## Integration Contract Acceptance Gates
- [ ] Existing issue client tests still green.

## Success Criteria
- `Phase 08: PASS` with cited proofs, or `FAIL`.

## Failure Recovery
- Return to P08.

## Phase Completion Marker (`.completed/P08A.md`)
Phase ID, timestamp, test counts, cited proofs, contradiction-scan result, verdict.
