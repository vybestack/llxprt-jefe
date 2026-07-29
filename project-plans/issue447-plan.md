# Issue #447 — doctor report: replace O(kinds x findings) scan in `ordered_section_kinds` with single-pass grouping

**Source:** Deferred from PR #444 (OCR post-PR review run 2/2, finding #9, Defer).
**Branch:** `issue447` (from `main`).
**Type:** Single-file internal refactor. No behavioral change, no new public API.

## Acceptance matrix

| # | Acceptance row | Expected behavior | Test |
|---|---|---|---|
| A1 | `ordered_section_kinds` no longer performs O(kinds × findings) work | A single pass over findings (or an equivalent sub-quadratic grouping) yields the kinds present | Unit test in `src/doctor/report.rs` asserting the returned ordering matches the canonical order for shuffled/duplicate/multi-kind inputs, including all 10 variants |
| A2 | Canonical section ordering preserved | Sections still render in canonical order regardless of input finding order | `report_renders_sections_in_canonical_order_regardless_of_input` (existing, integration) |
| A3 | Non-empty sections only | Empty kinds never produce a header; only present kinds render | `report_groups_multiple_findings_of_one_kind_under_one_header`, new dedup unit test |
| A4 | No observable output change | Rendered bytes identical for any findings input | Snapshot-style: render a representative multi-kind report and assert exact string equality (regression guard) |
| A5 | Gates pass | fmt, clippy `-D warnings`, `cargo test --workspace --all-features --locked` | CI / local |

## Non-goals

- Do NOT change the canonical `FindingKind` ordering or the set of reported kinds.
- Do NOT add any new finding kind or doctor section.
- Do NOT change how diagnostics are collected or rendered (text, redaction, status markers).
- Do NOT introduce a new public API, dependency, or module boundary.
- Do NOT relocate existing tests.

## Vertical slice(s)

**Slice 1 (only slice): single-pass grouping in `src/doctor/report.rs`.**

- Files allowed: `src/doctor/report.rs` only.
- RED: add a unit test in `report.rs` (`mod tests`) that pins the single-pass grouping contract — canonical order for shuffled/missing/duplicate/multi-kind inputs (including all 10 variants), and a snapshot test asserting exact rendered bytes for a representative multi-kind report.
- GREEN: replace the `canonical.iter().filter(|k| findings.iter().any(...))` scan with a single pass that records, for each finding in canonical-kind order, whether its kind has already been emitted. Reuse the grouped result to drive `write_kind_findings` so each kind no longer re-filters the slice.
- Refactor within scope: keep `ordered_section_kinds` as the single source of canonical ordering; `write_kind_findings` may accept a pre-grouped iterator instead of re-filtering, but this stays internal and private.
- Verification: `cargo test -p jefe --lib doctor::`, `cargo test --test doctor`, full gate suite.

## Scope ledger

| Item | Disposition |
|---|---|
| Single-pass grouping in `ordered_section_kinds` | In-scope (the issue) |
| Reuse grouping in `write_kind_findings` to remove the second per-kind pass | In-scope (issue text explicitly notes this; bounded, single-file, no API change) |
| Snapshot test for exact rendered bytes | In-scope (regression guard for "no observable change") |
| Anything else | Reject / follow-up |

**Hard scope budget check:** expected ~1 file changed, < 150 net lines. Well under 25-file / 1,500-line target.

## Review counters

- OCR local (pre-PR): 0 / 2
- OCR post-PR: 0 / 2

## Verification evidence

(to be filled: commit SHA, command outputs, CI run links)
