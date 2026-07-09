# Phase 16 — End-to-End Quality Gate

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P16
- **Prerequisites:** `.completed/P15A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Run the comprehensive quality gate: architecture audits, requirement-traceability gate, runtime-path
proof gate, traceability-marker validation, mockup E2E layout gate, and the full `make ci-check`.
This phase blocks merge until every gate passes.

## Requirements Implemented (Expanded)

### REQ-PR-001..014, REQ-PR-NFR-001..003 (whole feature acceptance)
- **Behavior contract:** GIVEN the implemented feature, WHEN the quality gate runs, THEN every
  architectural, traceability, runtime-path, layout, and lint/complexity gate passes with no
  overrides.

## Implementation Tasks (audit scripts/checks — no production code change expected)

### A. Architecture audits

Forbidden-pattern checks are INVERTED gates: they exit nonzero ONLY when a forbidden match is found
(absence of matches passes; presence fails).
```bash
# No forbidden imports (boundary isolation) — FAIL if any match is found.
if rg -n "use crate::github|use crate::app_input" src/ui/ ; then
  echo "FAIL: ui/ imports a forbidden boundary (github/app_input)"; exit 1
fi
if rg -n "use crate::ui|use crate::state|use crate::app_input" src/github/ ; then
  echo "FAIL: github/ imports a forbidden boundary (ui/state/app_input)"; exit 1
fi
# No forked/parallel architecture — FAIL if any match is found.
if rg -n "pull_requests_v2|prs_new|prs_old|pr_list_v2" src/ ; then
  echo "FAIL: forked/parallel PR architecture detected"; exit 1
fi
# Enum preservation (new variants added, none removed) — these MUST be present (FAIL if missing).
if ! rg -q "DashboardPullRequests" src/state/types.rs ; then
  echo "FAIL: ScreenMode::DashboardPullRequests missing"; exit 1
fi
if ! rg -q "PullRequests" src/messages.rs ; then
  echo "FAIL: MessageDomain/AppMessage PullRequests missing"; exit 1
fi
```

### B. Requirement traceability gate (REQ → Phase → Pseudocode consistency, not just presence)

Superficial marker presence is INSUFFICIENT (finding #3). This gate cross-checks the
`REQ → Phase → Pseudocode` traceability matrix in `plan/00-overview.md` and enforces THREE
properties for every functional REQ-PR-NNN:
1. it appears in at least one PRODUCTION file marker (a non-test first-party `src/` file), AND
2. it appears in at least one BEHAVIORAL TEST marker (a test file/module), AND
3. every `@pseudocode component-NNN lines X-Y` range cited anywhere in `src/` actually EXISTS —
   the cited `analysis/pseudocode/component-NNN.md` file exists and has at least `Y` lines.

A failure of ANY property is a hard FAIL (nonzero exit), not a printed warning.

```bash
trace_fail=0

# Helper: classify a path as a TEST file (behavioral test marker source) vs PRODUCTION file.
# Test sources in this repo are the external `*tests*.rs`/`*_tests.rs` modules and any file region
# under `#[cfg(test)]`. We treat files matching the test-name glob OR containing `#[cfg(test)]` as
# test sources; everything else first-party in src/ is production.
is_test_file() {
  case "$1" in
    *tests*.rs|*_test.rs) return 0 ;;
  esac
  rg -q "#\[cfg\(test\)\]" "$1"
}

# (a)+(b) Per-REQ production AND behavioral-test marker coverage.
for r in 001 002 003 004 005 006 007 008 009 010 011 012 013 014 \
         NFR-001 NFR-002 NFR-003 ; do
  prod=0; test=0
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    if is_test_file "$f"; then test=1; else prod=1; fi
  done < <(rg -l -- "@requirement REQ-PR-$r\b" src/ 2>/dev/null)
  # NFR markers may legitimately live only in production/integration scope; functional REQs (001-014)
  # require BOTH a production marker AND a behavioral-test marker.
  case "$r" in
    NFR-*)
      if [ "$prod" -eq 0 ] && [ "$test" -eq 0 ]; then
        echo "REQ-PR-$r: MISSING (no marker in src/)"; trace_fail=1
      else
        echo "REQ-PR-$r: present (prod=$prod test=$test)"
      fi ;;
    *)
      if [ "$prod" -eq 0 ] || [ "$test" -eq 0 ]; then
        echo "REQ-PR-$r: INSUFFICIENT coverage (prod=$prod test=$test) — needs BOTH a production and a behavioral-test marker"; trace_fail=1
      else
        echo "REQ-PR-$r: production+test markers present"
      fi ;;
  esac
done

# (c) Every cited @pseudocode component-NNN lines X-Y range must EXIST in the analysis file.
PSEUDO_DIR=project-plans/issue20/analysis/pseudocode
while IFS= read -r citation; do
  # citation looks like: component-001 lines 66-87
  comp="$(printf '%s' "$citation" | grep -oE 'component-[0-9]+')"
  hi="$(printf '%s'   "$citation" | grep -oE 'lines [0-9]+-[0-9]+' | grep -oE '[0-9]+-[0-9]+' | cut -d- -f2)"
  cfile="$PSEUDO_DIR/$comp.md"
  if [ ! -f "$cfile" ]; then
    echo "PSEUDO FAIL: cited $comp has no file at $cfile (from '$citation')"; trace_fail=1; continue
  fi
  have="$(wc -l < "$cfile" | tr -d ' ')"
  if [ "${hi:-0}" -gt "${have:-0}" ]; then
    echo "PSEUDO FAIL: '$citation' cites line $hi but $cfile has only $have lines"; trace_fail=1
  fi
done < <(rg -oN "component-[0-9]+ lines [0-9]+-[0-9]+" src/ | sort -u)

if [ "$trace_fail" -ne 0 ]; then
  echo "FAIL: requirement-traceability gate — REQ production+test coverage and/or pseudocode line ranges are inconsistent"; exit 1
fi
```

> This gate is the executable counterpart of the `REQ → Phase → Pseudocode Traceability Matrix` in
> `plan/00-overview.md`: it proves each functional REQ is realized in production AND guarded by a
> behavioral test, and that every cited pseudocode line range is real (no dangling citations).

### C. No-override / no-allow gate

This gate enforces FOUR things: (a) clippy.toml thresholds are unchanged in BOTH the root and CI
configs, (b) zero first-party clippy allow/expect attributes in ANY spelling, (c) handlers and
render fns are SPLIT to satisfy thresholds rather than overridden, and (d) the `Cargo.toml`
`[lints.clippy]` table is not weakened by THIS branch (finding #2).

> Current `Cargo.toml` lints-table facts (grounded at plan time): `Cargo.toml` has a top-level
> `[lints.rust]` table (`unsafe_code = "forbid"`) and a top-level `[lints.clippy]` table that sets
> `all = deny`, `pedantic/nursery = warn`, `unwrap_used/expect_used/print_stdout/print_stderr =
> warn`, `todo/unimplemented = deny`, and SIX pre-existing relaxations marked `= "allow"`
> (`needless_pass_by_value`, `redundant_clone`, `doc_markdown`, `missing_const_for_fn`,
> `missing_errors_doc`, `option_if_let_else`) under a comment "Relaxed for stub phase - will tighten
> in impl phases". `scripts/check-clippy-allows.sh` covers `src/` attributes and the two
> `clippy.toml` config files, but it does NOT inspect this `Cargo.toml` table — so lint levels could
> be weakened HERE without tripping any existing gate. Gate (d) closes that loophole. Removing/
> tightening existing allows (deny/warn) is ALLOWED and must NOT fail; only ADDING a new `allow` or
> DOWNGRADING an existing deny/warn to allow fails.

```bash
# (a) Assert EXACT threshold values are unchanged in BOTH config files.
# The repo keeps two clippy configs that MUST stay in sync (verified by the script below):
#   ./clippy.toml                  (root, used by local `cargo clippy`)
#   ./.github/clippy/clippy.toml   (CI, referenced via CLIPPY_CONF_DIR)
for cfg in clippy.toml .github/clippy/clippy.toml; do
  echo "== $cfg =="
  grep -E '^[[:space:]]*cognitive-complexity-threshold[[:space:]]*=[[:space:]]*15([[:space:]]|#|$)'   "$cfg" || { echo "FAIL cognitive-complexity-threshold != 15 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-lines-threshold[[:space:]]*=[[:space:]]*60([[:space:]]|#|$)'         "$cfg" || { echo "FAIL too-many-lines-threshold != 60 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*too-many-arguments-threshold[[:space:]]*=[[:space:]]*6([[:space:]]|#|$)'      "$cfg" || { echo "FAIL too-many-arguments-threshold != 6 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*type-complexity-threshold[[:space:]]*=[[:space:]]*250([[:space:]]|#|$)'       "$cfg" || { echo "FAIL type-complexity-threshold != 250 in $cfg"; exit 1; }
  grep -E '^[[:space:]]*max-struct-bools[[:space:]]*=[[:space:]]*3([[:space:]]|#|$)'                  "$cfg" || { echo "FAIL max-struct-bools != 3 in $cfg"; exit 1; }
done

# (b) Zero first-party clippy allow/expect attributes (covers every spelling).
# scripts/check-clippy-allows.sh is the authoritative gate: it ALSO asserts the two configs
# above stay in sync (so CLIPPY_CONF_DIR cannot silently fall back to clippy defaults).
bash scripts/check-clippy-allows.sh
# Belt-and-suspenders INVERTED gate covering every allow/expect spelling — FAIL (nonzero) if ANY
# first-party match is found; ZERO matches passes.
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

# (c) No threshold edits sneaked in via the working tree — FAIL if either config is modified.
if ! git diff --quiet -- clippy.toml .github/clippy/clippy.toml ; then
  echo "FAIL: clippy threshold config(s) modified in the working tree"
  git diff -- clippy.toml .github/clippy/clippy.toml
  exit 1
fi

# (d) Cargo.toml [lints.clippy] no-weaken gate (finding #2) — close the loophole that lint levels
# can be relaxed via Cargo.toml's `[lints.clippy]` table (which check-clippy-allows.sh does NOT
# inspect). FAIL if THIS branch ADDS a new `allow` entry under the lints table, or DOWNGRADES an
# existing deny/warn to allow. Scoped to `git diff main` ADDED lines, so the SIX pre-existing
# allows are baseline (not flagged), and REMOVING/tightening an allow is permitted (never fails).
# An added line that sets a lint to allow looks like `+ <lint> = "allow"` or
# `+ <lint> = { level = "allow", ... }`. We restrict matching to ADDED lines inside Cargo.toml.
added_lints_allows="$(git diff main -- Cargo.toml \
  | grep -E '^\+' | grep -Ev '^\+\+\+' \
  | grep -E '=[[:space:]]*"allow"|level[[:space:]]*=[[:space:]]*"allow"')"
if [ -n "$added_lints_allows" ]; then
  echo 'FAIL: this branch ADDS/weakens a Cargo.toml lint to allow (finding #2):'
  echo "$added_lints_allows"
  exit 1
fi
# Defense-in-depth: also fail if a `[lints]`/`[lints.clippy]` table HEADER is newly introduced where
# none existed (a brand-new table could smuggle allows past the line-level check above).
if git diff main -- Cargo.toml | grep -E '^\+[[:space:]]*\[lints(\.|])' | grep -Ev '^\+\+\+' ; then
  # A new [lints*] header on this branch is suspicious only if it adds allows; the line-level gate
  # above already catches added allows, so this is informational — but if the baseline had NO lints
  # table at all and one appears, surface it for manual review.
  echo "NOTE: a [lints*] table header was added/modified by this branch — confirm no allow weakening above."
fi
```

Decomposition requirement (no overrides): every changed/added handler and render fn that would
otherwise exceed `too-many-lines-threshold = 60` or `cognitive-complexity-threshold = 15` MUST be
split into smaller helper functions. Raising any threshold or adding any `allow`/`expect` to silence
the lint is a hard FAIL — there is no exception ledger.

### D. Traceability-marker validation (ALL THREE marker types — hard FAIL on any missing)

Every new PR-deliverable file MUST carry ALL THREE marker types (finding #1):
`@plan PLAN-20260624-PR-MODE.PNN`, `@requirement REQ-PR-NNN`, and
`@pseudocode component-NNN lines X-Y`. A file missing ANY one of the three is a hard FAIL (nonzero
exit), not just a printed warning. The single-marker (`@plan`-only) check is INSUFFICIENT.

The marker scan MUST enumerate EVERY new PR-deliverable file explicitly. NOTE (finding #2): the pure
selection-follow viewport helpers (`list_first_visible_index`/`list_visible_window`) do NOT live in a
new UI file — they are added to the SHARED module `src/layout.rs` (so the STATE reducers can consume
them without importing the UI layer), and their traceability markers are validated by the shared-file
added-line gate (D2 below), NOT by this new-file list. There is NO `src/ui/components/list_viewport.rs`.
This list is the AUTHORITATIVE complete NEW-PR-deliverable set (15 files) and is kept in sync with
`plan/00-overview.md` "New files to create".

For EACH of the 15 files the gate greps ALL THREE patterns and exits nonzero if ANY is missing:
- `@plan PLAN-20260624-PR-MODE\.P[0-9]+`  (phase-qualified plan marker)
- `@requirement REQ-PR-(NFR-)?[0-9]+`     (requirement marker)
- `@pseudocode component-[0-9]+ lines [0-9]+-[0-9]+`  (pseudocode line-range marker)
```bash
marker_fail=0
PR_DELIVERABLE_FILES=(
  src/github/parse_pr.rs
  src/state/prs_ops.rs src/state/prs_load_ops.rs src/state/prs_inline_ops.rs src/state/prs_mutation_ops.rs
  src/messages/prs_conversion.rs
  src/app_input/prs.rs src/app_input/prs_dispatch.rs src/app_input/prs_list_dispatch.rs src/app_input/prs_filter.rs src/app_input/prs_mutation.rs
  src/ui/components/pr_list.rs src/ui/components/pr_detail.rs src/ui/components/pr_filter_controls.rs
  src/ui/screens/pull_requests.rs
)
PLAN_RE='@plan PLAN-20260624-PR-MODE\.P[0-9]+'
REQ_RE='@requirement REQ-PR-(NFR-)?[0-9]+'
PSEUDO_RE='@pseudocode component-[0-9]+ lines [0-9]+-[0-9]+'
for f in "${PR_DELIVERABLE_FILES[@]}"; do
  if [ ! -f "$f" ]; then echo "$f: MISSING FILE"; marker_fail=1; continue; fi
  miss=""
  rg -q "$PLAN_RE"   "$f" || miss="$miss @plan"
  rg -q "$REQ_RE"    "$f" || miss="$miss @requirement"
  rg -q "$PSEUDO_RE" "$f" || miss="$miss @pseudocode"
  if [ -n "$miss" ]; then
    echo "$f: MISSING MARKER(S):$miss"; marker_fail=1
  else
    echo "$f: all three markers present"
  fi
done
if [ "$marker_fail" -ne 0 ]; then
  echo "FAIL: one or more PR deliverable files are missing required @plan/@requirement/@pseudocode markers"; exit 1
fi
```

#### D2. Shared-file added-line marker validation (finding #7)

PR-added blocks in SHARED modified integration files must ALSO carry at least a `@plan`/`@requirement`
marker on each substantial new PR block. This is scoped to lines THIS branch ADDED (`git diff main`),
so pre-existing code and trivial single-line registrations (e.g. a one-line `mod pr_list;` or a single
`use`/enum-variant registration) are NOT required to carry a marker — only substantial added blocks
(a heuristic threshold of ≥ 5 added non-trivial lines per file's PR hunk-set) must be annotated. The
gate FAILS if a shared file has a substantial PR-added block but NO `@plan` or `@requirement` marker
among its added lines.
```bash
shared_marker_fail=0
PR_SHARED_FILES=(
  src/state/types.rs src/state/mod.rs src/input.rs src/messages.rs
  src/app_input/mod.rs src/app_input/normal.rs src/github/mod.rs src/layout.rs
  src/domain/mod.rs
  src/ui/components/mod.rs src/ui/screens/mod.rs src/ui/orchestration.rs
)
for f in "${PR_SHARED_FILES[@]}"; do
  [ -f "$f" ] || continue
  added="$(git diff main -- "$f" | grep -E '^\+' | grep -Ev '^\+\+\+')"
  # Count non-trivial added lines (ignore blank lines and lone closing braces/registration lines).
  substantive="$(echo "$added" | grep -Ev '^\+[[:space:]]*$|^\+[[:space:]]*[})];,]*[[:space:]]*$' | wc -l | tr -d ' ')"
  if [ "${substantive:-0}" -ge 5 ]; then
    if echo "$added" | grep -Eq '@plan PLAN-20260624-PR-MODE|@requirement REQ-PR-'; then
      echo "$f: substantial PR-added block carries a marker"
    else
      echo "$f: FAIL — substantial PR-added block ($substantive lines) has NO @plan/@requirement marker"
      shared_marker_fail=1
    fi
  fi
done
if [ "$shared_marker_fail" -ne 0 ]; then
  echo "FAIL: one or more shared files have a substantial PR-added block with no traceability marker"; exit 1
fi
```

### E. Mockup E2E layout gate
- Render `PullRequestsScreen` at a known size; assert sidebar 22u, two-column, list/detail regions,
  filter band conditionality, keybind bar — matching mockups.md.

### F. Full verification suite

Do NOT rely solely on `make ci-check`. Run the canonical five-command baseline
EXPLICITLY and verbatim FIRST (each MUST exit 0 — this is the final GREEN gate, no RED exception),
THEN run `make ci-check` as the superset that ADDITIONALLY enforces the coverage gate
(`--fail-under-lines 30`):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
make ci-check   # superset: fmt, clippy, coverage (--fail-under-lines 30), build, test
```

## Pseudocode Traceability
- All components (audit only).

## Structural Verification Checklist
- [ ] All audits A-F pass.
- [ ] The five-command baseline ran explicitly and all five passed (`cargo fmt --all --check`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `bash scripts/check-clippy-allows.sh`; `cargo build --workspace --all-features --locked`;
  `cargo test --workspace --all-features --locked`).
- [ ] `make ci-check` green (superset adds coverage ≥30; fmt, clippy -D warnings, build --locked,
  test --locked).

## Semantic Verification Checklist (Mandatory)
- [ ] No forbidden imports; no forked architecture; enums preserved.
- [ ] Every functional REQ-PR-001..014 has BOTH a production-file marker AND a behavioral-test
  marker; every NFR appears in `src/`; every cited `@pseudocode component-NNN lines X-Y` range
  exists in the analysis file (no dangling citation).
- [ ] No clippy allow/expect (any spelling) in first-party `src/`; both `clippy.toml` and
  `.github/clippy/clippy.toml` retain EXACT thresholds (15/60/6/250/3) and remain in sync.
- [ ] `Cargo.toml` `[lints.clippy]` table is not weakened by this branch — no newly-added `allow`
  and no deny/warn→allow downgrade (`git diff main -- Cargo.toml`).
- [ ] Handlers/render fns are split to satisfy thresholds — no override anywhere.
- [ ] Every one of the 15 new PR-deliverable files carries ALL THREE markers
  (`@plan`/`@requirement`/`@pseudocode`); every substantial PR-added block in a shared file carries
  at least a `@plan`/`@requirement` marker (the shared `src/layout.rs` viewport helpers among them).
- [ ] Mockup layout gate passes.

## Deferred Implementation Detection

Inverted gate — SCOPED to PR-owned changes only (finding #1): FAIL (nonzero) if ANY deferred marker
is found in a NEW PR-owned file, or was ADDED by this branch to a SHARED modified file. It must NOT
grep all of `src/`: the current source contains PRE-EXISTING unrelated markers this PR does not own
— verified present at `src/state/types.rs:211` (`TODO(issue #24)`), `src/state/types.rs:220`
(`Placeholder for future multi-issue handling`), and `src/persistence/mod.rs:559` (`// For now, ...`).
A whole-`src/` grep would false-FAIL on these or pressure editing of unrelated files.
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
DEFERRED_RE='TODO|FIXME|HACK|placeholder|for now|will be implemented'
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

## Success Criteria
- All gates A-F green; zero deferred markers in the PR-owned scope (new PR files + this branch's
  added lines in shared files; pre-existing unrelated markers excluded); zero overrides.

## Failure Recovery
- Route each failing gate to its owning impl phase; re-run quality gate.

## Phase Completion Marker (`.completed/P16.md`)
Phase ID, timestamp, audit outputs, ci-check output, traceability table, semantic summary.
