# Issue #207 — Actions run-list status glyphs must match the detail screen

> The Actions run list renders bracketed text tags (`[OK]`, `[X]`, `[/]`, `[~]`,
> `[.]`, `[?]`) while the Actions detail pane renders bare single-codepoint
> glyphs (`✓`, `✗`, `⊘`, `~`, `.`, `?`). Two duplicated `status_glyph`
> implementations let the screens drift. Fix: one shared projection helper,
> detail-pane glyph style, no brackets in the run list.

## Accepted behavior

The run list adopts the detail pane's glyph vocabulary exactly, via a single
shared helper. Both screens call the same function, so the mapping is
necessarily identical.

| # | Run `status` / `conclusion` | Rendered glyph | Screens |
|---|-----------------------------|----------------|---------|
| A1 | `Completed` + `Success` | `✓` (U+2713) | run list + detail |
| A2 | `Completed` + `Failure` | `✗` (U+2717) | run list + detail |
| A3 | `Completed` + `TimedOut` / `ActionRequired` / `StartupFailure` | `✗` | run list + detail |
| A4 | `Completed` + `Cancelled` / `Skipped` / `Stale` / `Neutral` | `⊘` (U+2298) | run list + detail |
| A5 | `Completed` + `Unknown` or absent conclusion | `?` | run list + detail |
| A6 | `InProgress` | `~` | run list + detail |
| A7 | `Queued` / `Requested` / `Waiting` / `Pending` | `.` | run list + detail |
| A8 | `Unknown` status | `?` | run list + detail |
| A9 | Any run-list row | no `[` / `]` brackets around the status indicator | run list |
| A10 | Run-list title truncation | reserves chrome for the 1-cell glyph (prefix + glyph + space), so titles gain the 3 columns the bracketed tag used to consume | run list |

Boundary cases covered by A3/A4/A5: the run list previously collapsed
`TimedOut`, `ActionRequired`, `StartupFailure`, `Skipped`, `Stale`, `Neutral`
into `[?]` and mapped `Cancelled` to `[/]`. Unifying on the detail helper is
what makes the two screens agree; that reclassification is the point of the
issue ("the two screens can never drift again"), not an added feature.

Glyph policy: `dev-docs/standards/display-and-ui.md` explicitly permits `✓` and
other single-codepoint textual symbols; only pictographic emoji are banned.
`⊘` is already shipped in the detail pane. All three are `unicode-width` 1.

## Non-goals

- Changing the detail pane's glyph vocabulary (it is the accepted style).
- Colorizing glyphs, or any other run-list styling change.
- Touching the PR-list / PR-detail glyph helpers (`checks_status_glyph`,
  `review_status_glyph`) — different domain, different issue.
- Changing run ordering, windowing, layout modes, or meta-line content.

## Vertical slice (RED → GREEN → REFACTOR)

1. **RED (unit)** — `src/ui/components/actions_list.rs` tests assert `✓` / `✗`
   and the absence of `[` in the title line; a new parity test asserts the
   run-list projection and the detail projection emit the same glyph for the
   same status/conclusion pair. Fails on the current tree.
2. **RED (scenario)** — `dev-docs/tmux-scenarios/actions-mode.json` asserts
   `> ✗ Inspectable Actions fixture` instead of `> [X] …`. Fails on the current
   binary.
3. **GREEN** — move `status_glyph` to `src/actions_view.rs` (the shared pure
   Actions projection module) as a `pub` helper; delete both duplicates; call it
   from `actions_list.rs` and `actions_detail_view.rs`; drop `TITLE_CHROME` from
   7 to 4 to match the 1-cell glyph.

## Expected files

- `src/actions_view.rs` — gains the shared `status_glyph` helper + its tests.
- `src/actions_detail_view.rs` — drops its private copy, imports the shared one.
- `src/ui/components/actions_list.rs` — drops `run_status_glyph`, imports the
  shared helper, `TITLE_CHROME` 7 → 4, tests updated.
- `dev-docs/tmux-scenarios/actions-mode.json` — three literals updated.
- `project-plans/issue207-plan.md` — this plan.

## Behavioral verification

- `cargo test --workspace --all-features --locked`
- `make ci-check`
- `scripts/issue194-run-scenario.sh` (drives `actions-mode.json` end to end).

## Scope ledger

- (empty)

## Review counters

- Local OCR: 0 / 2
- PR OCR: 0 / 2
