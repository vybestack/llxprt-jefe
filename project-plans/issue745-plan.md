# Issue #745 — Restore the parenthesized count form

Branch `issue745`. Authored on merged main `caf0d56d` ("Name the
composition-root screen by its screen name (#749)") and rebased onto merged
main `81ad67c1` ("Restore the preview branch, todo, elapsed and last message
(#751)") once that landed; see "Rebase onto `81ad67c1`" below.

## Problem

The shared list control appends a bracketed status suffix to every list item
that carries one:

```rust
// src/host_controls.rs:477-479
let status_suffix = item
    .status
    .as_deref()
    .map_or(String::new(), |value| format!(" [{value}]"));
```

Two host projections use `ListItem::status` to carry a *count* rather than a
status word, so the count renders in square brackets where the scenario corpus
pins round ones:

| projection | shipped row | corpus row |
| --- | --- | --- |
| `host_panel_models::repository_list` | `>> LLxprt Jefe [0]` | `LLxprt Jefe (0)` |
| `host_panel_models::workbench_status` | `>> [x] Needs you [1]` | `Needs you (1)` |

The form changed as collateral when #715 (`f5826508`) routed both onto the
shared list control. Nothing was lost in capability terms; only the bracket
shape drifted. The pre-cutover component still spells the sidebar row
`format!("{prefix}{} ({agent_count})", repository.name)`
(`src/ui/components/sidebar.rs:64`).

### Correction to the issue's premise about that component

The issue and its comment describe `src/ui/components/sidebar.rs` as having
"no live caller", so that its tests "pin a row the app does not render". That
is not what the tree shows. `Sidebar` is exported by
`src/ui/components/mod.rs:180` and rendered by four screens that
`src/ui/orchestration.rs` dispatches on `ScreenId`:

- `src/ui/screens/actions.rs:153`
- `src/ui/screens/issues.rs:267`
- `src/ui/screens/pull_requests.rs:285`
- `src/ui/screens/errors.rs:207`

What #715 removed was the *composition-root* screen's use of it, not every
use. So the app renders both forms today: `LLxprt Jefe [0]` on the
composition root through the shared list control, and `LLxprt Jefe (0)` on the
actions, issues, pull-requests and errors screens through the retained
component. Those component tests therefore cover a live path and are not
deleted or weakened. They are strengthened instead, so that the row form they
cover is pinned literally rather than incidentally, and the two rendering paths
are pinned to the same form.

## Decision

Restore `(N)`. The issue records the direction for this family: bring back the
pre-cutover presentation rather than bend the corpus to match drift.

Both projections get **their own suffix formatting**, exactly the way
`agent_type_availability` already composes its two-space rows
(`src/host_panel_models.rs:297-320`): the count is folded into the item
`label` and `status` becomes `None`. The shared `" [{value}]"` suffix is **not**
changed — the agent list (`Alpha One [Running]`), the session list, the
workbench cards (`[Working]`) and the generic control tests all depend on it.

### Consequence recorded, not hidden

`push_list_item_row` protects a `status` suffix from truncation: the label is
fitted to the row budget and the suffix always survives (#723). A folded label
is fitted as one unit, so on a pane too narrow for `name (N)` the count can be
truncated with the name instead of surviving it. This matches the pre-cutover
component, which fitted `name (N)` as one span, and matches
`agent_type_availability`, whose `[Create enabled]` tail is already inside its
label. The invariant that actually mattered in #723 — *the row never wraps into
a second row and never shifts later rows down* — is unchanged and is pinned by
a new test in this issue. The sidebar name budget is #732's subject and is a
non-goal here.

## Acceptance matrix

| # | Actor / launch path | Input & boundary | Observable success | Observable failure | Side effects | Persistence | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | Operator on the composition-root screen, repository sidebar | A repository with N visible agents, N = 0 and N > 0 | The sidebar row reads `<name> (N)`; the count is inside the item label and `status` is `None` | Row reads `<name> [N]` | none | none | unit: `repository_rows_carry_the_parenthesized_agent_count` |
| A2 | Same, selected row | The selected repository | The marker still leads the row: `>> <name> (N)` through the shared control | Marker or count missing | none | none | unit: `repository_row_renders_the_parenthesized_count_through_the_shared_control` |
| A3 | Operator on the workbench, Repositories STATUS block | One agent per bucket, default all-on mask | Bucket rows read `[x] Needs you (1)`, `[x] Working (1)`, `[x] Ready (1)`, `[x] Stale (1)`; `status` is `None` | Rows read `[x] Needs you [1]` | none | none | unit: `status_block_lists_four_buckets_in_filter_order_with_live_counts` |
| A4 | Same, a bucket toggled off | `Working` masked off | Row reads `[ ] Working (1)`: checkbox reflects the mask, count stays pre-filter | Count follows the filter, or bracket form returns | none | none | unit: `status_block_checkbox_reflects_the_mask_while_counts_stay_prefilter`, `status_block_activation_toggles_the_bucket_under_the_cursor` |
| A5 | Every other list control | Agent list, session list, workbench cards, generic snapshots | The shared `" [{value}]"` suffix is untouched: `Alpha One [Running]`, `[Working]` | Any of them flips to `(...)` | none | none | unit: existing `list_label_rows_truncate_instead_of_wrapping`, cards/session/agent-type suites |
| A6 | Narrow pane, sidebar row | Name longer than the row budget | The row still projects to exactly one row, visibly truncated, never wrapped | Two rows, later rows shift down | none | none | unit: `a_folded_repository_row_truncates_instead_of_wrapping` |
| A7 | Tutorial flow | `first-agent-tutorial`, step 41 | `LLxprt Jefe (0)` observed in the frame | `HAR-E005: literal 'LLxprt Jefe (0)' not observed` | none | none | scenario `first-agent-tutorial` |
| A8 | Workbench flows | `v1/workbench-cards` 33, `v1/workbench-cards-native` 37, `v1/workbench-attach` 48, `v1/workbench-sort` 48 | `Working (1)` / `Ready (1)` / `Needs you (1)` observed | `HAR-E005` on those literals | none | none | scenarios, subject to the #751 note below |
| A9 | Actions / issues / pull-requests / errors screens | The same repository list through the retained component | The row reads `<name> (N)`, pinned literally, so both rendering paths agree and no test pins a row the app does not render | The two paths disagree, or a row form is only pinned incidentally by a `contains` on the name | none | none | `src/ui/components/sidebar.rs` tests, strengthened to assert the full row |

## Non-goals

- The sidebar name budget and pane padding (#732), including the selection
  marker width.
- Agent row content (#730).
- The shared `" [{value}]"` list suffix and every control that depends on it.
- Deleting or rewriting the retained `src/ui/components/sidebar.rs` component
  itself; only its row-form test coverage is repointed.
- Restoring truncation protection for a folded count (see "Consequence"
  above); that lives with the name budget in #732.

## Slices

1. **RED** — run two required scenarios at unmodified `caf0d56d` and record the
   exact harness failures. Add unit tests for the sidebar row label and the
   STATUS row label; prove they fail for the intended reason.
2. **GREEN** — fold the count into the label in `repository_list` and
   `workbench_status`, set `status: None`, document why in each projection.
3. **Repoint** — move the row-form assertions off the uncalled component and
   onto the live projection; update `host_panel_models_status_tests.rs` to the
   rendered form.
4. **Gates + evidence** — full exact-head gate run, owner-evidence re-pin,
   scenario re-run, commit, push.

## Expected paths

| Layer | Path | Change |
| --- | --- | --- |
| Host projection | `src/host_panel_models.rs` | A1, A3: fold the count, `status: None`, doc comment; repoint the inline `dashboard_sidebar_rows_are_one_line_per_item` test |
| Host projection tests | `src/host_panel_models_status_tests.rs` | A3, A4: repoint label assertions to the rendered form |
| Host projection tests | `src/host_panel_models_sidebar_tests.rs` (new) | A1, A2, A6: sidebar row form through the projection and the shared control |
| Live component (4 screens) | `src/ui/components/sidebar.rs` | A9: strengthen the row-form assertions to pin `(N)` literally |
| Test registration | `src/lib.rs` or the existing test-module list | wire the new test module |
| Corpus | `dev-docs/tmux-scenarios/issue722/dashboard-arrow-navigation.json` | A10: re-pin the three `>> <name> [0]` assertions to `(0)` |
| Owner evidence | `dev-docs/testing/issue705-owner-evidence.json` | re-pin `src/host_panel_models.rs`, the two cascaded ledger pins, and the artifact-set fold |
| Owner evidence | `dev-docs/testing/scenario-owner-evidence.json` | re-pin the issue722 scenario |
| Owner evidence | `dev-docs/testing/issue704-owner-evidence.json` | cascade: re-pin `scenario-owner-evidence.json` |
| Plan | `project-plans/issue745-plan.md` | this file |

No production file outside `src/host_panel_models.rs` changes.

### A10, found during the final verification pass

`dev-docs/tmux-scenarios/issue722/dashboard-arrow-navigation.json` asserts
`">> Alpha Repo [0]"` twice and `">> Beta Repo [0]"` once. It is a required
macOS scenario and it drives the composition-root sidebar through the shared
list control, so the fold above turns those three assertions red.

That scenario is not counter-evidence against restoring `(N)`. It was authored
in `bb54dff2` ("route dashboard arrows by focused pane (#722) (#724)"), which
is *after* `f5826508` (#715) introduced the drift, so it froze the drifted form
rather than the pre-cutover one. Re-pinning it to `(0)` is the same restoration
this issue performs everywhere else, applied to the one corpus entry that
recorded the drift. It is a literal-only edit: the step count, operation set and
assertion count the execution manifest records are unchanged, which is why
`scenario-execution-manifest.json` stays byte-identical and the
`scenario_manifest_sha256` pin still reproduces.

Nothing else in the corpus or the test tree pins a bracketed count. The search
that established this covers the scenario corpus for any quoted literal ending
in ` [<digits>]` (three hits, all in this one file) and the Rust tree for
bracketed counts on repository and bucket rows (no hits outside the two suites
this issue repoints).

### Correction to this plan's own test-module doc

`src/host_panel_models_sidebar_tests.rs` was first written carrying the issue's
"no live caller" premise in its module doc. The tree contradicts it, as the
section above records, so the doc now states what is true: the retained
component is live on four screens and in `selection::content`, #715 repointed
only the composition root off it, and each path is pinned where it renders.

## Scope ledger

| Entry | Status | Reason |
| --- | --- | --- |
| Fold the count into `repository_list`'s label | in scope | A1, A2 |
| Fold the count into `workbench_status`'s label | in scope | A3, A4 |
| New sidebar projection test module | in scope | A1/A2/A6: the composition-root row form had no test of its own |
| Repoint `host_panel_models_status_tests.rs` | in scope | required by the issue comment; a stale pin would go green on a row the app no longer renders |
| Strengthen row-form assertions in `src/ui/components/sidebar.rs` tests | in scope | A9, adjusted: the component is live on four screens, so its tests are repointed by being made literal rather than deleted |
| Correct the doc comment on `agent_type_availability` | in scope | it cited `>> Alpha Repo [0]` as a shared-suffix example; that row no longer uses the shared suffix |
| Re-pin the three bracket assertions in `issue722/dashboard-arrow-navigation.json` | in scope | A10: the only corpus entry that froze the drifted form; leaving it ships a required scenario asserting a row the app does not render |
| Re-pin `issue705-owner-evidence.json` | in scope | mechanical consequence of touching a pinned source file |
| Re-pin `scenario-owner-evidence.json` and cascade through `issue704-owner-evidence.json` | in scope | mechanical consequence of A10 |
| Change the shared list suffix | rejected | the issue forbids it; other controls depend on it |
| Delete `src/ui/components/sidebar.rs` | rejected | out of scope, retained by the #706 decision |
| Sidebar name budget / marker width | deferred | #732 |

## Verification

Exact-head, strictly serial, one cargo invocation at a time:

`cargo fmt --all --check`; `cargo build --workspace --all-features --locked`;
`cargo test --workspace --all-features --locked`; `cargo xtask coverage`;
`cargo xtask check source-size`; `cargo xtask check architecture`;
`cargo clippy --workspace --all-targets --all-features -- -D warnings`; the
complexity gate with `CLIPPY_CONF_DIR` set and `clippy::`-prefixed lint names;
`scenario_manifest`; `issue704_owner_evidence`; `issue705_owner_evidence`;
`issue706_cutover_contracts`; `harness_authority`; `git diff --check`.

Scenarios: `first-agent-tutorial`, `dashboard-list-windowing`,
`pid-commit-corner`, `issue731/dashboard-focus-chrome`, and the four v1
workbench flows, subject to the #751 note below.

The evidence chain is re-derived by `tmp/issue745/repin745.py`, adapted from the
known-good `tmp/issue733/repin733.py`, and re-derived again from scratch against
the new base by `tmp/issue745/rebase/repin745-rebase.py`. A10 lengthens the
cascade that script walks: the scenario edit invalidates
`scenario-owner-evidence.json`, whose bytes are pinned by both
`issue704-owner-evidence.json` and `issue705-owner-evidence.json`, and
`issue704-owner-evidence.json`'s bytes are in turn pinned by
`issue705-owner-evidence.json`. The script derives that graph from the base
ledgers and fails if it does not match the declared write order, so the
ordering is proved rather than asserted.
`issue706-owner-evidence.json` and `scenario-execution-manifest.json` stay
byte-identical to the base throughout, and the `deleted_paths` sections are
checked as absence contracts rather than as reproducible hashes.

### `dashboard-list-windowing`, failing on main independently of this issue

`dashboard-list-windowing` stops at step 1 with
`HAR-E005: literal 'Repository 24' not observed within 30000 ms`. The same
failure, at the same step, with the same message, is recorded in the CI
artifacts for merged main at `3293a3d1`, at #747 and at #749
(`tmp/triage-corpus/artifacts-main-3293a3d1/`, `tmp/triage-corpus/a747/`,
`tmp/triage-corpus/a749/`, all `tui-scenarios-macos-2`).

That attribution is re-established locally against the rebase base rather than
carried over: `tmp/issue745/rebase/base-check.sh` detaches to unmodified
`81ad67c1`, builds the harness binaries there and runs the scenario, and it
fails with the same two executed steps, the same step index 1, the same `wait`
op and the same message
(`tmp/issue745/rebase/base-check/reports-dashboard-list-windowing/`). The
branch's own run at `7c8c8709` is identical to it, so nothing in this delta
moves that scenario in either direction.

The frame this branch captured at that failure
(`tmp/issue745/final/scenarios/reports-dashboard-list-windowing/`) shows why it
is not a count problem. The header reads `25 repos`, the selection is on
repository 24, and the sidebar lists `Repository 0` through `Repository 18`:

```
 0| LLxprt Jefe - 0.0.32                      25 repos | 0/25 running
 1|╔ ▶ Repositories ════╗╭ Agents ───────────────────────
 4|║    Repository 0 (… ║│   Agent 1 [Dead]
...
22|║    Repository 18 … ║│
```

The pane never windows to the selected item, so `Repository 24` is not on
screen in any row form. That is list windowing on the composition root, which
this issue does not touch.

The fold moves the row form in the direction that helps rather than hurts here:
the suffix costs four columns either way, but under `" [0]"` those columns were
reserved outside the fitted label, leaving an 11-column name budget that
truncates `Repository 0` to `Repository…`; folded, the name budget is 15 and
the frame above shows `Repository 0 (…`, with the name intact. Under the shipped
bracket form this scenario's literal could not have been observed even if the
windowing worked.

### #751 note

`v1/workbench-attach`, `v1/workbench-sort`, `v1/workbench-cards` and
`v1/workbench-cards-native` reach their `(N)` step only with PR #751's preview
work in the tree. On the original base `caf0d56d`, which does not contain it,
those four stopped earlier — at step 28 — for the preview reason, not the count
reason. That was reported as observed rather than worked around. #751 merged as
`81ad67c1`, and the rebase below moves this branch onto it, so the step 28
preview waits clear and the four count assertions become reachable.

## RED evidence, recorded at `caf0d56d` before any production change

Scenarios (`tmp/issue745/red/`, binaries built from the unmodified base):

| scenario | stopped at | harness error |
| --- | --- | --- |
| `first-agent-tutorial` | step 41 of 732 | `HAR-E005: literal 'LLxprt Jefe (0)' not observed within 15000 ms` |
| `v1/workbench-cards` | step 28 of 42 | `HAR-E005: literal 'JSP preview is wired' not observed within 30000 ms` |
| `v1/workbench-cards-native` | step 28 of 39 | `HAR-E005: literal 'Native LLxprt todo' not observed within 30000 ms` |
| `v1/workbench-attach` | step 28 of 55 | `HAR-E005: literal 'JSP preview is wired' not observed within 15000 ms` |
| `v1/workbench-sort` | step 28 of 52 | `HAR-E005: literal 'JSP preview is wired' not observed within 15000 ms` |

Only `first-agent-tutorial` reaches its `(N)` assertion. The other four stop at
step 28 on the preview literal, which is the #751 dependency described above,
not the count form. The frame captured at the tutorial's failure shows the
shipped bracket row directly:

```
╔ ▶ Repositories ════╗╭ Agent Types ──────────
║ >> LLxprt Jefe [0] ║│   no executable candid
```

Unit tests, added before the production edit
(`cargo test --lib --all-features --locked -- host_panel_models_sidebar_tests
host_panel_models_status_tests`, exit 101, `tmp/issue745/red/unit.log`):

```
test result: FAILED. 3 passed; 5 failed; 0 ignored; 5032 filtered out

failures:
    host_panel_models_sidebar_tests::repository_row_renders_the_parenthesized_count_through_the_shared_control
    host_panel_models_sidebar_tests::repository_rows_carry_the_parenthesized_agent_count
    host_panel_models_status_tests::status_block_activation_toggles_the_bucket_under_the_cursor
    host_panel_models_status_tests::status_block_checkbox_reflects_the_mask_while_counts_stay_prefilter
    host_panel_models_status_tests::status_block_lists_four_buckets_in_filter_order_with_live_counts

assertion `left == right` failed: the rendered sidebar rows are the corpus form
  left: [">> Repo one [0]", "   Repo two [2]"]
 right: [">> Repo one (0)", "   Repo two (2)"]
```

The three that already passed are the ones that do not depend on the row form:
selection identity, cursor movement through the host-panel input path, and the
#723 no-wrap invariant. `tmp/issue745/red/unit-production-diff.txt` records that
no production file was modified at that point.

## GREEN evidence, exact head, worktree `caf0d56d` + this delta

Gates (`tmp/issue745/final/`, `exits.txt`), strictly serial, all exit 0:

| gate | exit | result |
| --- | --- | --- |
| `cargo fmt --all --check` | 0 | clean |
| `cargo build --workspace --all-features --locked` | 0 | clean |
| `cargo test --workspace --all-features --locked` | 0 | 7736 passed, 0 failed, across 96 test binaries |
| `cargo xtask coverage` | 0 | 73.81% lines, 74.02% functions, 74.01% regions |
| `cargo xtask check source-size` | 0 | 130 pre-existing length warnings, no failure |
| `cargo xtask check architecture` | 0 | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | clean |
| complexity clippy, `CLIPPY_CONF_DIR` set, `clippy::`-prefixed | 0 | clean |
| `cargo test --test scenario_manifest` | 0 | 11 passed |
| `cargo test --test issue704_owner_evidence` | 0 | 6 passed |
| `cargo test --test issue705_owner_evidence` | 0 | 17 passed |
| `cargo test --test issue706_cutover_contracts` | 0 | 1 passed |
| `cargo test --test harness_authority` | 0 | 2 passed |
| `git diff --check` | 0 | clean |

Scenarios (`tmp/issue745/final/scenarios/`):

| scenario | result | attribution |
| --- | --- | --- |
| `first-agent-tutorial` | passed, 732/732 | A7 green; was RED at step 41 on `LLxprt Jefe (0)` at the base |
| `pid-commit-corner` | passed, 8/8 | unaffected |
| `issue731/dashboard-focus-chrome` | passed, 17/17 | unaffected |
| `issue722/dashboard-arrow-navigation` | passed, 10/10 | A10 green; the re-pinned assertions are what the app renders |
| `dashboard-list-windowing` | failed, step 1 | pre-existing on main, see the section above |
| `v1/workbench-cards` | failed, step 28 | `JSP preview is wired`, the #751 dependency |
| `v1/workbench-cards-native` | failed, step 28 | `Native LLxprt todo`, the #751 dependency |
| `v1/workbench-attach` | failed, step 28 | `JSP preview is wired`, the #751 dependency |
| `v1/workbench-sort` | failed, step 28 | `JSP preview is wired`, the #751 dependency |

Every step between 28 and the count assertions in those four is key input, text
entry, or a wait on `Shortcut`, `Agents`, `STATUS`, `Status: Ready`, a card
literal, `WAITING` or `permission`. The only assertions #745 owns are the
counts: `Working (1)` at cards 33, `[ ] Working (1)` at 37, `[x] Working (1)`
at 40, `Ready (1)` at cards-native 37, and `Needs you (1)` / `Working (1)` at
attach and sort 48-49. Those are the remaining #745 blockers in those four
flows, and they are unreachable on this branch for the preview reason, not the
count reason. A3 and A4 pin the same four labels at the projection instead.

## Rebase onto `81ad67c1`

#751 merged as `81ad67c1` after this branch was authored, and it touches two of
the same paths: `src/host_panel_models.rs` and
`dev-docs/testing/issue705-owner-evidence.json`. The branch was rebased onto it
rather than merged, so the delivery stays a single commit on top of merged main.
The record is `tmp/issue745/rebase/`.

**`src/host_panel_models.rs` — auto-merged, both intents kept.** The two deltas
are disjoint: #751 adds `AGENT_PREVIEW_DOCUMENT_WIDTH`, swaps the preview's
repository lookup for `dashboard_git_info::resolve_preview_git_info` and adds
`agent_preview_document`, which folds `preview_view`'s todo, elapsed and
last-reply rows into the projection's document; #745 folds the count into
`workbench_status` and `repository_list` and leaves the shared `" [{value}]"`
suffix alone. Neither is discarded, and that is proved rather than assumed:
`tmp/issue745/rebase/source-resolution.log` records both marker sets present in
the merged file, and records that applying this branch's original
`src/host_panel_models.rs` delta to main's version of the file byte-for-byte
reproduces the merged file, so the diff against `origin/main` is exactly #745's
delta and nothing else.

**`issue705-owner-evidence.json` — not hand-merged.** The conflict was two pins
moving under both branches: the `src/host_panel_models.rs` artifact hash and the
`artifact_set_sha256` fold that covers it. Resolving that by hand would write a
hash that no tree reproduces. Every ledger was reset to main's bytes with
`git checkout origin/main -- <path>` instead, and the #745 pins were re-derived
from scratch by `tmp/issue745/rebase/repin745-rebase.py` (65 proofs, 0
failures, `tmp/issue745/rebase/repin.log`). That script proves all 405 base pins
across the four ledgers reproduce from `81ad67c1` before it writes anything,
proves the stale set is exactly what the #745 delta explains
(`issue705` → `src/host_panel_models.rs`; `scenario-owner` → the issue722
scenario), derives the ledger-pins-ledger graph from the base bytes and checks
the declared write order against it, and re-verifies the chain afterwards. The
cascade it wrote:

| ledger | re-pinned |
| --- | --- |
| `scenario-owner-evidence.json` | `issue722/dashboard-arrow-navigation.json` |
| `issue704-owner-evidence.json` | `scenario-owner-evidence.json` |
| `issue705-owner-evidence.json` | `src/host_panel_models.rs`, `scenario-owner-evidence.json`, `issue704-owner-evidence.json`, then the `artifact_set_sha256` fold |

`issue706-owner-evidence.json` and `scenario-execution-manifest.json` are
byte-identical to `81ad67c1`, checked before, during and after the write;
`issue705`'s `base_revision` is untouched; the pinned path set of every ledger
is unchanged, so no pin was added or dropped; and the `deleted_paths` sections
are checked as absence contracts, all 37 entries still absent.

### Scenarios on the rebased head

The four v1 workbench flows were the four this delivery could not finish on the
old base. With #751 in the base they clear the step 28 preview wait and run to
completion, so the `(N)` assertions this issue owns — `Working (1)` at cards 33,
`[ ] Working (1)` at 37, `[x] Working (1)` at 40, `Ready (1)` at cards-native 37,
and `Needs you (1)` / `Working (1)` at attach and sort 48-49 — are reached and
observed rather than argued for at the projection alone.

| scenario | result | attribution |
| --- | --- | --- |
| `first-agent-tutorial` | passed, 732/732 | A7 |
| `pid-commit-corner` | passed, 8/8 | unaffected |
| `issue731/dashboard-focus-chrome` | passed, 17/17 | unaffected |
| `issue722/dashboard-arrow-navigation` | passed, 10/10 | A10 |
| `v1/workbench-cards` | passed, 42/42 | A8, now reachable |
| `v1/workbench-cards-native` | passed, 39/39 | A8, now reachable |
| `v1/workbench-attach` | passed, 55/55 | A8, now reachable |
| `v1/workbench-sort` | passed, 52/52 | A8, now reachable |
| `dashboard-list-windowing` | failed, step 1 | pre-existing on the base, proved below |

The exact-head gate run for the rebased head is `tmp/issue745/rebase/gates/`,
and the scenario run is `tmp/issue745/rebase/scenarios/`.

## Review counters

Local OCR runs before PR: 0 of 2. Post-PR OCR runs: not applicable, no PR is
opened for this delivery.

## Evidence

`tmp/issue745/`.
