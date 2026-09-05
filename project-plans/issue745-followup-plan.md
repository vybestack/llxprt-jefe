# Issue #745 follow-up — Keep STATUS counts visible in the dashboard split

Branch `issue745-followup`, cut from merged main `76478739` ("Restore the
parenthesized count form (#752)"). This follows #752, which closed #745 and was
reopened from dogfood evidence.

## What #752 delivered, and what it left open

#752 restored the round count form. Both dashboard projections stopped handing
their count to the shared list control as a `status` value and folded it into
the item `label` instead:

```rust
// src/host_panel_models.rs:75-80 at 76478739
label: format!(
    "{} {} ({})",
    if filter.allows(*bucket) { "[x]" } else { "[ ]" },
    bucket.label(),
    counts[bucket.as_index()]
),
status: None,
```

The shared control protects a `status` suffix from truncation and budgets the
label against it:

```rust
// src/host_controls.rs:519-542 at 76478739
/// A label plus its trailing count must never wrap: a wrapped sidebar row
/// shifts every later row down and reads as two items (issue #723). Only the
/// label portion is truncated to the row budget; the count always survives.
let budget =
    width.checked_sub(UnicodeWidthStr::width(marker) + UnicodeWidthStr::width(status_suffix));
```

Folding the count into the label moved it out from under that protection. The
`(N)` is now inside the span that `fit_text_to_width` truncates, so at the
shipped pane widths the count is the first thing to go.

The #752 plan recorded this as a deliberate non-goal:

> Restoring truncation protection for a folded count (see "Consequence"
> above); that lives with the name budget in #732.

The reopen overturns that call for the count itself. #732 still owns *how much
room the name gets* (name budget, pane padding, marker width). This issue owns
*the count surviving whatever budget the name is given*.

## Why the #752 tests and scenarios could not catch it

Every proof #752 shipped stops at or above the projection layer:

| proof | layer | width it sees |
| --- | --- | --- |
| `host_panel_models_status_tests` | `HostPanelModel` | none — asserts `ListItem::label` |
| `host_panel_models_sidebar_tests` | `HostPanelModel` | none, plus one control call at a synthetic width |
| `first-agent-tutorial` step 41 | rendered frame | one repository named `LLxprt Jefe`, `(0)` |
| `v1/workbench-*` steps 33/37/48 | rendered frame | one agent per bucket, `(1)` |

`ListItem::label` carries `[x] Needs you (1)` whatever the pane width is, so a
projection assertion can never fail on truncation. The scenarios do render, but
their synthetic states put a single-digit count under a short name, and at
those sizes the row happens to fit — `>> [x] Needs you (1)` is 20 cells wide in
a pane whose content rectangle is exactly 20 cells. The corpus was passing on
zero slack.

## The shipped geometry

`core.repositories` puts the sidebar and the STATUS block in a fixed
22-column rail (`src/workbench/screens.rs:97,672-688`) with
`LIST_PANE_CHROME = Insets::new(2, 1, 1, 1)`, so the resolver hands the panels:

| panel | chrome | content width | marker | label budget |
| --- | --- | --- | --- | --- |
| `status` | 22 | 20 | 3 | 17 |
| `repositories` | 22 (less list padding) | 18 | 3 | 15 |

Measured on this tree at `76478739`, at 120x40, 313x83 and 80x30 alike; the
rail is fixed, so terminal size does not change it.

With the count inside the label, the label budget must absorb the count too:

| row | label at count N | budget | first failing N |
| --- | --- | --- | --- |
| `[x] Needs you (N)` | 16 + digits | 17 | **N = 10** |
| `[x] Working (N)` | 14 + digits | 17 | N = 1000 |
| `[x] Ready (N)` / `[x] Stale (N)` | 12 + digits | 17 | N = 100000 |
| `<repository name> (N)` | name + 3 + digits | 15 | **name longer than 11 at N < 10** |

## Reproduced failures at `76478739`

Rendered through the real `ProviderScreen` iocraft path at 313x30 (the
operator's terminal is 313 columns; `ps -o lstart` puts PID 24067's start at
`Fri Sep 4 19:33:00 2026`, well after the 06:06 merge of #752, so this is not a
stale process):

```
|║ >> Repo one (12)   ║ |
|║    Repo llxprt-co… ║ |     <- sidebar count gone entirely
|║    Repo j (0)      ║ |
|╭ STATUS ────────────╮ |
|│>> [x] Needs you (1…│ |     <- STATUS count truncated mid-number
|│   [x] Working (0)  │ |
|│   [x] Ready (0)    │ |
|│   [x] Stale (0)    │ |
```

Both rows carry the correct count in `HostControlListItem`/`ListItem`; both
lose it in `push_list_item_row` at the shipped width. That is the missing
rendered-geometry case.

## Decision

Give the shared list control a **typed count** that it protects from
truncation, the same way it already protects a status suffix, and let the two
dashboard projections carry their count in it instead of in the label.

- `ListItem` gains `count: Option<usize>`.
- The shared control renders it as `" ({count})"`, subtracts it from the label
  budget, and appends it after the fitted label.
- The shared `" [{value}]"` status suffix is **unchanged**; nothing that
  renders `Alpha One [Running]` or a card's `[Working]` moves.
- `workbench_status` and `repository_list` set `label` to the semantic text and
  `count` to the number.

This is the invariant `push_list_item_row`'s own doc comment already claims
("the count always survives"); #752 removed the mechanism that made it true.
Restoring it as a typed field rather than a second string suffix keeps the
count unambiguous, keeps `status` free for status *words*, and keeps the fix
inside the shared definition/control runtime rather than in a bespoke renderer.

The provider wire is not widened: `LIST_ITEM_KEYS` stays at its five keys and
`read_list_item` sets `count: None`. A count is a host-projection affordance;
letting providers push one would need bounds, redaction and migration coverage
this issue does not own.

### Rejected alternatives

- **Change the shared `" [{value}]"` suffix to `" ({value})"`.** Rejected by
  #745's recorded decision and still wrong: it would rewrite every agent,
  session and card row.
- **Pre-truncate in the projection.** Impossible: `workbench_status(state)` and
  `repository_list(state)` are built with no width; the width only arrives in
  `project_model_rows`. This is precisely why the fix belongs in the control.
- **Teach `push_list_item_row` to find a trailing `(N)` inside the label.**
  Parsing presentation back out of a rendered string, and a global change to
  every list's truncation behavior with no test proving that general invariant.

## Acceptance matrix

| # | Actor / launch path | Input & boundary | Observable success | Observable failure | Side effects | Persistence | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- |
| B1 | Operator on `core.repositories`, STATUS block | 12 agents in `Needs you`, real rendered frame at the shipped 22-column rail | The painted row contains `Needs you` and `(12)`; no `…` inside the count | Row paints `[x] Needs you (1…` | none | none | render test `status_counts_survive_the_shipped_status_pane_width` |
| B2 | Same | `Working` and `Ready` populated, two-digit counts | Painted rows carry `Working (N)` and `Ready (N)` intact | Any bucket loses its count | none | none | same test, all four buckets asserted |
| B3 | Same | Empty workspace, every bucket zero | Painted rows carry `(0)` on all four buckets | A zero count is dropped | none | none | render test `status_counts_survive_when_every_bucket_is_empty` — a regression guard, green at RED (see below) |
| B4 | Operator on `core.repositories`, sidebar | A repository whose name overflows the 18-column pane, with agents | The painted row ends in `(N)`; the *name* is the part that shows `…` | Row paints `Repo llxprt-co…` with no count | none | none | render test `repository_counts_survive_a_name_that_overflows_the_sidebar` |
| B5 | Shared list control, any consumer | An item with `count` and a label longer than the budget | Exactly one row; row fits the width; row ends `" (N)"`; the label shows `…` | Wrapped row, overflowing row, or lost count | none | none | unit `list_count_survives_when_the_label_does_not` |
| B6 | Shared list control | An item with `status` and no `count` | Unchanged `" [{value}]"` suffix, still protected | Suffix form or protection changes | none | none | existing `list_label_rows_truncate_instead_of_wrapping`, unmodified |
| B7 | Shared list control | An item carrying both `count` and `status` | Deterministic order: `marker + label + " (count)" + " [status]"`, both protected | Non-deterministic or lossy composition | none | none | unit `list_renders_a_count_before_a_status_suffix` |
| B8 | Provider-fed list | A wire snapshot with the five documented list-item keys | Parses as today; `count` is `None`; a `count` key is still refused as unknown | Wire contract widened or narrowed | none | wire schema unchanged | existing `panel_reader` / `panel_wire` suites |
| B9 | Host projections | `workbench_status`, `repository_list` | `label` is the semantic text, `count` is `Some(n)`; the composed row still reads `[x] Needs you (1)` / `LLxprt Jefe (0)` | Label still carries the count, or the row form changes | none | none | repointed `host_panel_models_status_tests`, `host_panel_models_sidebar_tests` |
| B10 | Required scenarios | `first-agent-tutorial`, `v1/workbench-attach`, `v1/workbench-sort`, `v1/workbench-cards`, `v1/workbench-cards-native` | Their pinned literals still observed | `HAR-E005` on `LLxprt Jefe (0)` / `Needs you (1)` / `Working (1)` / `Ready (1)` | none | none | scenario runs |

## Non-goals

- The sidebar name budget, pane padding and selection-marker width (#732).
  This issue changes *what the budget protects*, not how wide it is.
- Agent row content (#730).
- The stale footer build metadata (#753).
- The shared `" [{value}]"` status suffix and every control that reads it.
- Widening the provider wire so a plugin can send a count.
- Repositories geometry (#735).

## Slices

1. **RED** — add the render-path tests (B1–B4) and the control unit tests
   (B5, B7) against unmodified `76478739`; capture the failures. B1/B2 and B4
   are the discriminating failures: at `76478739` they paint
   `[x] Needs you (1…` and a sidebar row with no count at all. B3 passes at
   RED and is a regression guard, not a discriminator — at a single-digit
   count every bucket label fits the 20-cell pane whole, which is exactly why
   the shipped corpus never caught this. It is kept because the zero-count row
   is the one a later budget change would silently drop.
2. **GREEN** — add `ListItem::count`, protect it in `push_list_item_row`, move
   the two projections onto it, thread `count: None` through every other
   construction site (B8, B9).
3. **Repoint** — update the projection assertions that pinned the folded label.
4. **Gates + evidence** — full exact-head gate run, deterministic ledger
   re-pin, scenario runs, commit, push, PR.

## Expected paths

| Layer | Path | Change |
| --- | --- | --- |
| Shared protocol model | `src/runtime/provider/panel_model.rs` | `ListItem::count` |
| Shared control | `src/host_controls.rs` | protect the count in `push_list_item_row` |
| Shared control tests | `src/host_controls_tests.rs` | B5, B7 |
| Host projections | `src/host_panel_models.rs` | B9: carry the count in the typed field |
| Host projection tests | `src/host_panel_models_status_tests.rs`, `src/host_panel_models_sidebar_tests.rs` | repoint |
| Render path tests (new) | `src/ui/components/provider_screen_count_render_tests.rs` | B1–B4 |
| Render test registration | `src/ui/components/provider_screen.rs` | `#[path]` module |
| Wire reader | `src/runtime/provider/panel_reader.rs` | `count: None`, wire keys unchanged |
| Redaction | `src/runtime/provider/redaction.rs` | carry the count through |
| Remaining `ListItem` sites | test modules listed by `rg 'ListItem \{'` | `count: None` |
| Owner evidence | `dev-docs/testing/issue705-owner-evidence.json` | re-pin touched artifacts and the set fold |
| Plan | `project-plans/issue745-followup-plan.md` | this file |

No scenario fixture changes: the render tests reach the geometry the corpus
cannot, and every pinned scenario literal must keep passing unchanged.

## Scope ledger

| entry | disposition |
| --- | --- |
| `ListItem` gains a field, so every construction site is touched | in scope: completing the contract change rather than leaving it half-done |
| `redaction.rs` is not in the #705 ledger but constructs a `ListItem` | in scope, mechanical |
| Sidebar row (B4) touches the same rail #732 owns | in scope for the count only; the name budget is untouched |

## Review counters

Open Code Review: 1 of 2 local (complete), 0 of 2 post-PR.

### Local review findings and disposition

Recorded in `tmp/issue745-followup/review/ocr.json` (3 findings, 21 files).

| # | Severity | Finding | Disposition |
| --- | --- | --- | --- |
| 1 | medium, bug | `push_list_item_row`'s underflow fallback sliced a suffix, so a narrow row could paint `(1…` or `[Runn…` — the defect this issue exists to remove, reachable through the same function | **Blocker-Fix.** `compose_list_item_row` now walks documented fallback rungs and never elides anything but the label. Suffixes are painted whole or dropped whole |
| 2 | low, test | `painted_row` bound to the first matching row anywhere in the rail, so a repository named after a bucket could answer for the bucket | **In-scope-Fix.** The search is scoped to one named pane and fails unless exactly one row matches; `a_repository_named_after_a_bucket_does_not_answer_for_the_bucket` is the collision fixture |
| 3 | low, maintainability | `RAIL_CELLS = 22` re-pins `SIDEBAR_COLUMNS`, which is `pub(super)` and cannot be imported here | **Defer.** The constant only narrows the failure message and the search window; widening the rail would fail these tests loudly, not silently. Re-exporting a workbench geometry constant for a UI test is a visibility change this issue does not own |

Finding 1 was fixed test-first: the four exact-form boundary tests failed
against the sliced fallback before the change
(`tmp/issue745-followup/resume2/red2-host-controls-prefix-behavior.log`), with
the recorded left-hand values `">>  (…"`, `">>  (1234…"`, `">>  …"` and
`">>  (2) [Runn…"`. The count-before-status order and the shared
`" [{value}]"` suffix are unchanged by both fixes.

## Verification commands

```
cargo fmt --all --check
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo xtask coverage
cargo xtask check source-size
cargo xtask check architecture
cargo clippy --workspace --all-targets --all-features -- -D warnings
CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features -- \
  -A clippy::all -A clippy::pedantic -A clippy::nursery \
  -D clippy::cognitive_complexity -D clippy::too_many_lines \
  -D clippy::too_many_arguments -D clippy::type_complexity -D clippy::struct_excessive_bools
cargo test --locked --all-features --test scenario_manifest
cargo test --locked --all-features --test issue704_owner_evidence
cargo test --locked --all-features --test issue705_owner_evidence
cargo test --locked --all-features --test issue706_owner_evidence
cargo test --locked --all-features --test harness_authority
git diff --check
```

## RED evidence

Recorded in `tmp/issue745-followup/red-*.log`; summarised in the PR. The
render-path run at `76478739` reads:

```
status_counts_survive_when_every_bucket_is_empty ... ok       <- B3, guard
repository_counts_survive_a_name_that_overflows_the_sidebar ... FAILED  <- B4
status_counts_survive_the_shipped_status_pane_width ... FAILED         <- B1/B2
test result: FAILED. 1 passed; 2 failed
```

So the discriminating RED failures are B1/B2 and B4; B3 was green at RED and
carries its weight as a regression guard over the zero-count row.

## GREEN evidence

Recorded in `tmp/issue745-followup/green-*.log` and the gate logs.
