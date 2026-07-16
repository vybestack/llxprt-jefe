# Issue 342: first-agent tutorial SVG geometry

## Objective

Correct the bounded issue 241 publication renderer so its fixed 100-column,
32-row tutorial captures retain the complete terminal grid inside an SVG with
explicit horizontal padding. Regenerate the three published tutorial assets
from the unchanged semantic capture scenario without changing the tutorial
narrative or publication redaction.

## Acceptance matrix

| Row | Actor / launch path | Input and boundaries | Observable success | Observable failure / diagnostics | Permitted side effects | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Documentation reader opens `docs/getting-started.md` on GitHub | Any of the three existing first-agent images generated from the fixed 100x32 scenario | The complete 100-column terminal grid, including rightmost borders and final-column text, is visible with intentional left/right padding | A renderer geometry contract fails when the final terminal column reaches or crosses the SVG edge | None at read time | Deterministic integration test over all committed tutorial SVGs plus rendered-image inspection |
| A2 | Documentation author runs `scripts/issue241-capture.sh capture` | Existing six semantic checkpoints, 100 columns, 32 rows, publication redaction enabled | Every generated SVG uses one fixed cross-image width/height and an explicit 100-column text extent contained between equal horizontal padding | Contract test identifies missing or inconsistent width, text extent, padding, or final-column containment | Existing run-root publication files only | RED/GREEN capture workflow contract test |
| A3 | Documentation author republishes selected checkpoints | Unchanged first-agent scenario and tutorial narrative | The three affected assets are regenerated, retain safe redacted content, and preserve useful existing Markdown alt text | Existing publication validation rejects unsafe content; asset contract rejects stale geometry | Replace only the three selected SVG assets | Real capture run, byte/content inspection, and unchanged tutorial diff |

## Explicit non-goals

- No generalized image-rendering platform, dynamic terminal-size support, ANSI
  renderer, dependency, application UI behavior, or harness scenario change.
- No change to the tutorial prose, image selection, filenames, alt text,
  publication redaction policy, capture isolation, or cleanup behavior.
- No attempt to redesign issue 241's fixed 100x32 publication contract.

## Vertical slice

### Slice 1: fixed terminal-grid geometry (RED -> GREEN)

- Rows: A1-A3.
- Owner/boundary: the existing issue-specific publication renderer and its Unix
  integration contract.
- RED: add deterministic checks proving the current 800-pixel viewport cannot
  contain the declared 100-column text extent with horizontal padding and that
  all committed assets carry the same safe geometry.
- GREEN: give every rendered row an explicit fixed terminal-grid text extent,
  widen the fixed SVG/background to contain it with equal padding, and
  regenerate the three selected assets from the unchanged semantic scenario.
- Allowed paths: `scripts/issue241-capture.sh`, `tests/issue241_capture.rs`,
  `docs/assets/first-agent-{new-repository,new-agent,result}.svg`, and this plan.
- Verification: focused issue 241 capture tests, real issue 241 capture,
  independent SVG rasterization/inspection, `make quick-check`, and
  `make ci-check`.
- Stop if deterministic containment requires a new renderer/dependency, a
  scenario or tutorial narrative change, or files outside the allowed paths.

## Expected paths and architecture layers

- Unix documentation-production boundary: `scripts/issue241-capture.sh`.
- Behavioral contract: `tests/issue241_capture.rs`.
- Generated documentation assets: the three existing `docs/assets/first-agent-*.svg` files.
- Plan/evidence ledger: this file.

Target: 6 changed files and fewer than 250 net changed lines. Generated line
replacements count toward the file budget but do not add a new subsystem.

## Scope ledger

| Discovery | Disposition | Reason |
| --- | --- | --- |
| The existing 800-pixel viewport is narrower than 100 monospace glyphs at 14px plus padding | Blocker—Fix | Direct reproduction of issue 342 |
| Publication redaction changes some row string lengths | In-scope geometry handling | Unicode-aware publication normalization preserves redaction markers while restoring each semantic row to the fixed 100-column grid |
| Generic SVG text-length support is inconsistent across renderers | In-scope fallback geometry | The explicit text extent proves the contract, while a wider viewport and normalized rows also keep all glyphs visible when a renderer ignores `textLength` |
| OCR found that consuming whitespace before `pid:` skipped PIDs at column zero or after punctuation | Blocker—Fix | The redaction again matches `pid:` in any position and tests leading, whitespace-prefixed, and parenthesized forms |
| Unicode-aware row normalization uses Perl | Reject portability finding | POSIX shell and the available awk count UTF-8 bytes on supported macOS; Perl is already available in the bounded Unix capture environment and preserves the required Unicode character-column contract without a new project dependency |
| PR 339 already pins height to 32 rows and 594 pixels | No change | Issue 342 is horizontal clipping only; preserving fixed height is required |

No unapproved scope changes.

## Review counters

- Open Code Review before PR: 1 / 2. StepFun `step-3.7-flash`, OCR workspace scope,
  `complete_best_effort`; two duplicate high-impact findings identified a
  leading-PID redaction regression and were fixed with RED/GREEN coverage. One
  low-impact greedy-marker finding was also fixed. The Perl portability concern
  was rejected because preserving Unicode character columns is required, the
  existing capture contract is explicitly bounded Unix tooling, and POSIX
  shell/awk on supported macOS count UTF-8 bytes rather than characters.
- Open Code Review after PR: 0 / 2.

## Verification evidence

- Reproduction: all three current assets declare an 800-pixel viewport while
  100 glyphs at the renderer's 14px monospace settings exceed the available
  784 pixels between the existing 8-pixel side margins; independent
  `rsvg-convert` rasterization retains the clipped 800x594 canvas.
- RED focused regressions: generated SVG rows lacked `textLength`; committed
  assets lacked containment geometry; publication rows were not normalized to
  100 columns; and a narrower viewBox was not detected.
- GREEN focused regression: all 11 issue 241 capture contracts pass, including
  final-column containment, actual viewBox-width agreement, fixed row width,
  redaction, and committed-asset borders.
- Real semantic capture and asset regeneration:
  `/private/tmp/jefe-issue342-publication-20260716` completed successfully using
  the unchanged scenario; all three selected assets were copied from that run.
- Independent rasterization: all three assets render at 880x594 with visible
  content ending at x=856, leaving 24 pixels before the viewport edge in the
  local fallback renderer.
- Publication audit: every selected asset contains 32 rows of exactly 100
  Unicode characters; no local path, identity, or credential-like content was
  found; tutorial prose and scenario remain unchanged.
- Local OpenCodeReview artifact:
  `/Users/acoliver/Library/Logs/llxprt-code/opencodereview/runs/20260716T190441Z-81d2ad9f`;
  OCR reported `complete_best_effort` coverage over the changed script and test.
- OCR remediation: leading, whitespace-prefixed, and parenthesized PID redaction
  pass RED/GREEN focused coverage; the status-marker split uses the first marker;
  malformed geometry is checked before subtraction; focused Clippy passes.
- `make quick-check`: passed on the completed implementation.
- `make ci-check`: passed after correcting focused Clippy findings; format,
  policy, source-size, both Clippy passes, 73.39% line coverage, locked build,
  and the complete locked test suite all completed successfully.
- PR exact-head CI: pending.

## Deferred findings / follow-ups

None.
