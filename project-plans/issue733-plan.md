# Issue #733 — restore the dashboard preview's real branch, todo block, turn elapsed and last message

Branch: `issue733`, based on `origin/main` `caf0d56dc` (`Name the composition-root
screen by its screen name (#749)`).
Issue: #733 (`Dashboard preview always shows Branch: (unknown) and lost its todo,
elapsed and last-message rows`). Related: #715, #723/#725, #731, #734.

Two defects, one pane. Both are wiring: the data still exists, the retained
projection still computes it, and the shipped panel does not ask for it.

---

## 1. Established evidence this plan builds on

Nothing below is re-derived; each row cites the artifact it came from.

| Fact | Source |
|---|---|
| `host_panel_models::agent_preview` builds git info with `GitRepoInfo::from_configured_origin`, whose constructor hardcodes `branch: None, dirty: None` | `src/host_panel_models.rs:360-362`, `src/git_info/mod.rs:72-83` |
| `preview_metadata` therefore resolves `Branch` to the `(unknown)` sentinel on every repository | `src/preview_view.rs:48-55` |
| `GitRepoInfo::resolve` is the branch/dirty probe the pre-cutover dashboard used; it is intact, cached, and still called from the selection/copy path only | `src/git_info/mod.rs:93-113`, `src/dashboard_git_info.rs:15-34`, `tmp/issue731-agentlist/FINDINGS.md` §2.3 |
| `build_preview_view_at` still computes the turn-elapsed row, the `Todo:` block and `Last reply:`; `agent_preview` consumes only `preview_metadata` and never reaches them | `src/preview_view.rs:60-92`, `src/host_panel_models.rs:368-375` |
| `src/ui/components/preview.rs` is the only caller of `build_preview_view`, and it has no live caller of its own | `tmp/issue731-agentlist/FINDINGS.md` §2.2 |
| The loss is attributable: #715 (`f5826508`) dropped the whole body; #723/#725 (`4fbca0d7`) restored the five metadata rows with the origin-only constructor | `tmp/issue731-agentlist/FINDINGS.md` §2.2 |
| Six v1 scenarios wait on the agent's reply text and are red on merged main | issue #733 comment, reproduced below in §5 |
| The shared Detail control renders `document`, then `metadata`, then `actions`, wrapping the document at the pane's real width | `src/host_controls.rs:600-625`, `src/host_controls/intent.rs:321-330` |
| The dashboard preview pane is 36 fixed columns with `Insets::new(3, 1, 2, 2)`, so its content width is exactly 32 cells | `src/workbench/screens.rs:99,156,616-621` |
| `Last reply: JSP preview is wired` is exactly 32 characters, so it fits that pane on one row without wrapping | measured; scenario `dev-docs/tmux-scenarios/v1/jsp-llxprt-preview.json` step 30 |

---

## 2. Acceptance matrix

Every row is decision-complete: one observable success, one observable failure,
one named proof.

| # | Actor / launch path | Input and boundary cases | Success behavior (observable) | Failure behavior and diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|
| A1 | `dashboard_git_info::resolve_preview_git_info(state)` | selected agent whose `work_dir` is a real local git work tree on a named branch | Returns `Some(GitRepoInfo)` whose `branch` is that branch name | assertion prints the resolved `GitRepoInfo` | one cached `git rev-parse`/`git status` probe per work dir | reuses the existing `GitRepoInfo::resolve` cache and TTL; no new probe, no new cache | `src/host_panel_models_chrome_tests.rs` |
| A2 | same | no selected agent, or the agent's repository is not in state | Returns `None`; the preview keeps its `No agent selected` body | assertion prints `Some(..)` | none | unchanged from today | `src/host_panel_models_chrome_tests.rs` |
| A3 | same | repository with `remote.enabled == true` | `branch` and `dirty` stay `None`; the row reads `(unknown)` | assertion prints the probed branch | none — remote probing is skipped, no SSH round trip | matches `GitRepoInfo::resolve`'s documented remote contract | `src/host_panel_models_chrome_tests.rs` |
| A4 | `project_host_panel(state, AgentPreview)` | selected agent in a real local git work tree | The `Branch` metadata row carries the real branch name, not `(unknown)` | assertion prints the metadata rows | one cached git probe | the five-row metadata set and the per-value width budget from #723 are unchanged | `src/host_panel_models_chrome_tests.rs` |
| A5 | same | agent whose `work_dir` is not a git tree | `Branch` still reads `(unknown)`; `Repo` still reads the configured origin | assertion prints the metadata rows | one failed probe, no panic | the `(unknown)` sentinel is preserved as the only unknown spelling | `src/host_panel_models_chrome_tests.rs` |
| B1 | `project_host_panel(state, AgentPreview)` | observation with an active turn, three todos and a last message | The projected rows contain `Turn elapsed: 5s`, a `Todo:` header, one row per todo, and `Last reply: <content>` | assertion prints every projected row | none (pure over the observation) | rows come from `preview_view`, so the marker vocabulary (`[x] [>] [ ] [?]`) and the stale/unsupported/unknown arms are unchanged | `src/host_panel_models_chrome_tests.rs` |
| B2 | same | no observation at all | The `Todo:` header is present with `  (telemetry unavailable)` beneath it; no elapsed row, no `Last reply:` row | assertion prints every projected row | none | reproduces `append_todos`' no-observation arm exactly | `src/host_panel_models_chrome_tests.rs` |
| B3 | same | agent selected, `agent.description` empty | The document still opens with one empty row, exactly as it does today, and the restored rows follow it | assertion prints the document | none | the leading description line added by #723/#725 is preserved, not reverted (explicit non-goal) | `src/host_panel_models_chrome_tests.rs` |
| B4 | same | no agent selected | Body stays the `No agent selected` detail with empty metadata | assertion prints the body | none | unchanged from today | existing `src/host_panel_models_chrome_tests.rs` coverage |
| B5 | same | a last reply wider than the pane (`Native LLxprt JSP reply`, 35 cells with its label) | The row is truncated to one 32-cell row ending in `…`; the reply never wraps onto a row of its own | assertion prints every projected row | none | reproduces the pre-cutover `fit_text_to_width` budget the corpus reads; two native scenarios wait on a 19-character prefix a wrap would split | `src/host_panel_models_chrome_tests.rs` |
| C1 | **Guard.** `project_control_body` over the shipped `AgentPreview` body | fixture with elapsed, todos and a last message, projected at the pane's real width | *Every* line `preview_view::build_preview_view_at` produces for the same inputs appears among the projected rows | assertion names the first line that did not reach the projection and prints both sets | none | if a future change stops routing through `preview_view`, this fails; the module cannot silently become unreachable again | `src/host_panel_models_chrome_tests.rs` |
| D1 | Real PTY, 110x32, JSP fixture producer | `dev-docs/tmux-scenarios/v1/jsp-llxprt-preview.json` | Step 29 observes `JSP preview is wired`; step 30 asserts `Status: Working`, `Implement issue 522`, `Last reply: JSP preview is wired` present and `(no tasks)` absent | `HAR-E005: literal 'JSP preview is wired' not observed within 15000 ms`, exit 124 | none | required-corpus scenario, unmodified | scenario run under `scripts/run-scenario-manifest.py` |
| D2 | same | `jsp-llxprt-preview-native`, `workbench-attach`, `workbench-cards`, `workbench-cards-native`, `workbench-sort` | Each runs past step index 28 and passes every assertion about the preview's reply and todo rows | same `HAR-E005` at step index 28 | none | required-corpus scenarios, unmodified | scenario runs, §7 and §11 |
| D3 | same | `pid-commit-corner`, `issue731/dashboard-focus-chrome` | Stay green: the change adds rows to one panel body and does not move chrome | scenario failure report | none | regression guard for the panes this change sits beside | scenario runs, §7 |

---

## 3. Non-goals

Each stays unchanged and is reported rather than silently altered.

1. **The pane title.** `Agent preview` stays; it is not reverted to `Preview`.
2. **The leading description line.** The document still opens with
   `agent.description`. Restoring the pre-cutover order (metadata first) would
   mean either reverting that line or reordering the *shared* Detail control for
   every provider panel; see §4.3 for the recorded decision.
3. **Telemetry acquisition.** No change to how observations are produced,
   published or aged.
4. **The agent list row.** The status glyph, `[N]` shortcut badge, `origin @
   branch` suffix, dirty marker and `↕` grab marker are issue A of
   `tmp/issue731-agentlist/FINDINGS.md` §4.1 and are not touched here.
5. **`GitRepoInfo` probing, caching and TTL.** Reused exactly as-is. No new
   probe type, no cache change, no new call into the render loop beyond the one
   the pre-cutover dashboard already made.
6. **`src/preview_view.rs` and `src/ui/components/preview.rs`.** Both are pinned
   retained modules in `dev-docs/testing/issue706-owner-evidence.json`. They stay
   byte-identical: everything this change needs is already public on them. They
   are consumed, not edited, and not resurrected as a second renderer.
7. **A new schema-1 scenario for `Branch:`.** See §4.4; a hermetic scenario
   cannot assert a real branch today, and asserting a shimmed one would be
   vacuous. The branch behaviour is proven against a real git work tree in a
   unit test instead.
8. **Footer chrome, panel focus, Repositories geometry.** Tracked separately
   (#734 part two, #730, issue D of FINDINGS §4.1).

---

## 4. Design decisions

### 4.1 Branch: use the probe, through the module that owns dashboard git

`agent_preview` swaps `GitRepoInfo::from_configured_origin(&repo.github_repo)`
for a new `dashboard_git_info::resolve_preview_git_info(state)`, which calls
`GitRepoInfo::resolve(&repository.github_repo, repository.remote.enabled,
&agent.work_dir)` — the same three arguments `resolve_dashboard_git_info` passes
today. The resolver lives in `src/dashboard_git_info.rs` because that module
already owns "how the dashboard resolves git display data", and because the
preview needs one work dir rather than the whole visible list: calling
`resolve_dashboard_git_info` would probe every visible agent to use one result.

Cost is bounded by the existing process cache (`cached_branch_and_dirty`,
`BRANCH_TTL`), which is the same cache the pre-cutover dashboard hit on every
frame.

### 4.2 The restored rows come *from* `preview_view`, not from a copy

`agent_preview` asks `preview_view::build_preview_view` for the whole Preview and
drops the leading rows `preview_metadata` already supplied, so the tail — turn
elapsed, blank, `Todo:`, the todo rows, blank, `Last reply:` — is literally the
retained module's output. Nothing is recomputed and no formatting is duplicated,
so the todo markers, the stale/unsupported/unknown arms and the elapsed format
cannot drift from the module that owns them.

The rows are budgeted to the pane's content width, 32 cells: `PREVIEW_COLUMNS`
(36) less the two columns `PREVIEW_CHROME` spends on each side. That is the
pre-cutover behaviour — `build_preview_view` fits every row with
`fit_text_to_width`, so an over-wide row ends in an ellipsis — and it is what
the corpus reads. Both alternatives fail an existing scenario, measured rather
than predicted:

- Handing the rows over untruncated lets the shared Detail control wrap them.
  `Last reply: Native LLxprt JSP reply` (35 cells) breaks into
  `Last reply: Native LLxprt JSP` and `reply`, and `jsp-llxprt-preview-native`
  waits on the 19-character prefix `Native LLxprt JSP r`, which then appears on
  no row. Evidence: `tmp/issue733/green/reports-jsp-llxprt-preview-native/`.
- Reusing the metadata block's narrower 30-cell budget cuts
  `Last reply: JSP preview is wired` to `Last reply: JSP preview is wi…`, and
  `jsp-llxprt-preview` asserts the full 32-cell line.

The metadata block keeps its own 30-cell budget: that one applies to the value
after the label split, which is #723's accepted contract and has its own test.

### 4.3 Why the metadata block now renders after the document

`project_detail` renders `document`, then `metadata`, then `actions`, for every
Detail panel in the app. The restored rows go in the document, so the pane reads:

```
<description>
Turn elapsed: 5s

Todo:
  [>] Implement issue 522

Last reply: JSP preview is wired
Name: LLxprt
Status: Working
Repo: acme/widgets
Branch: feature/agent-cards
Dir: ./llxprt
```

Pre-cutover the metadata came first. Restoring that order would require one of:
reverting the leading description line (an explicit non-goal), reordering the
shared control for every provider Detail panel (a change to shared semantics for
one panel's benefit), or extending the provider wire protocol with a trailing
section (a new public abstraction, and a stopping condition under
`dev-docs/workflow/ISSUE-DELIVERY.md` §5).

All three cost more than the ordering is worth, and none of the six scenarios
asserts row order. Recorded here as a deliberate, visible deviation rather than
an accident. Restoring the pre-cutover order belongs with the pane-title and
description-line questions the issue already defers.

### 4.4 Why no new scenario asserts `Branch:`

The harness environment is closed: `PATH` is `${workspace}/bin` and nothing else
(`src/harness/v1/env.rs:17-29`). A scenario asserting a real branch would need
either a `git` shim in that directory — in which case it asserts the shim, not
the probe — or a `PATH` override reaching the host toolchain, which would break
the hermetic contract the harness exists to provide. The harness also builds
workspaces from declared dirs and files only, so it cannot create a real git work
tree to probe.

The branch behaviour is therefore proven against a **real** git work tree, on a
named branch, with real `git init`/`git commit`, in a unit test — the same
technique `tests/git_info/real_repository.rs` already uses. A hermetic scenario
for `Branch:` is recorded as a deferred follow-up, not delivered vacuously.

---

## 5. RED evidence recorded before any production change

Head `caf0d56dc0e4b1a5e30b9b5d0b6c0a5e56cbb0d1` (`origin/main`), worktree clean
apart from the user-owned `.llxprt/LLXPRT.md`.

| Scenario | Result | Diagnostic |
|---|---|---|
| `dev-docs/tmux-scenarios/v1/jsp-llxprt-preview.json` | exit 124, `failed`, 29 of 31 steps | step index 28: `HAR-E005: literal 'JSP preview is wired' not observed within 15000 ms` |
| `dev-docs/tmux-scenarios/v1/workbench-cards.json` | exit 124, `failed`, 29 of 42 steps | step index 28: `HAR-E005: literal 'JSP preview is wired' not observed within 30000 ms` |

The captured frame (`tmp/issue733/red/frame-jsp-llxprt-preview.txt`) shows the
`Agent preview` pane carrying its five metadata rows, `Branch: (unknown)`, and
twenty-one blank rows where the todo block, elapsed and last message belong.

Unit-level RED is added in the same commit sequence and fails first for the
intended reason (A4, B1, C1 all fail against unmodified `agent_preview`).

---

## 6. Slices

One behaviour, one slice; the two defects share a single function and a single
test file, and splitting them would leave the pane half-restored between commits.

**Slice 1 — the preview carries its branch and its live document.**

1. Acceptance rows: A1–A5, B1–B4, C1, D1–D3.
2. Architecture owner: `host_panel_models` (host-owned panel models) with its
   git-resolution boundary in `dashboard_git_info`. The shared control runtime,
   the provider wire protocol and the retained `preview_view` /
   `ui::components::preview` modules are read-only.
3. Allowed paths:
   - `src/host_panel_models.rs`
   - `src/dashboard_git_info.rs`
   - `src/host_panel_models_chrome_tests.rs`
   - `project-plans/issue733-plan.md`
   - `dev-docs/testing/issue705-owner-evidence.json` (pin re-derivation only)
4. RED: the two scenarios above, plus the new unit tests.
5. GREEN: every assertion about the preview's reply, todo block and elapsed row
   passes in all six v1 scenarios; `jsp-llxprt-preview`,
   `jsp-llxprt-preview-native`, `pid-commit-corner` and
   `issue731/dashboard-focus-chrome` pass outright; every gate in §7 exits 0.
   §11 records the four scenarios a second, out-of-scope regression still
   blocks.
6. Non-goals: §3.
7. Stop and report if the work appears to require a protocol change, a new
   control kind, an edit to a pinned retained module, or a change to the shared
   `project_detail` ordering.

---

## 7. Required verification

Strictly serial, exact head, as background jobs with sentinels.

- `cargo fmt --all --check`
- `cargo build --workspace --all-features --locked`
- `cargo test --workspace --all-features --locked`
- `cargo xtask coverage`
- `cargo xtask check source-size`
- `cargo xtask check architecture`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets
  --all-features -- -A clippy::all -A clippy::pedantic -A clippy::nursery
  -D clippy::cognitive_complexity -D clippy::too_many_lines
  -D clippy::too_many_arguments -D clippy::type_complexity
  -D clippy::struct_excessive_bools`
- `cargo test --test scenario_manifest --all-features --locked`
- `cargo test --test issue704_owner_evidence --all-features --locked`
- `cargo test --test issue705_owner_evidence --all-features --locked`
- `cargo test --test issue706_cutover_contracts --all-features --locked`
- `cargo test --test harness_authority --all-features --locked`
- `git diff --check`
- Scenarios: the six v1 scenarios named on the issue, plus
  `dev-docs/tmux-scenarios/pid-commit-corner.json` and
  `dev-docs/tmux-scenarios/issue731/dashboard-focus-chrome.json`.

### Evidence pins

`src/host_panel_models.rs` is pinned as artifact 22 of
`dev-docs/testing/issue705-owner-evidence.json`, so that ledger is re-derived
from worktree bytes after the source change, and its
`validation.artifact_set_sha256` fold recomputed. Nothing hash-pins
`issue705-owner-evidence.json` itself, so the cascade terminates there — proved,
not assumed. `dev-docs/testing/issue706-owner-evidence.json` stays byte-identical
because neither pinned retained module is edited. The re-pin proves every old
hash reproduces from `HEAD` before writing anything, and treats the
`deleted_paths` sections as an absence contract rather than a reproduction one.

---

## 8. Scope ledger

| Entry | Disposition |
|---|---|
| `src/host_panel_models.rs` — git source and document rows | A1–A5, B1–B4, C1 |
| `src/dashboard_git_info.rs` — single-agent preview resolver | A1–A3 |
| `src/host_panel_models_chrome_tests.rs` — new tests | A1–A5, B1–B4, C1 |
| `dev-docs/testing/issue705-owner-evidence.json` — pin re-derivation | required gate, mechanical, no semantic change |
| `project-plans/issue733-plan.md` | this plan |
| *No* change to `.llxprt/`, `.code_puppy/`, `.github/`, `Cargo.toml`, `Cargo.lock`, any scenario, any manifest, any gate script | — |

No unapproved entries.

---

## 9. Review counters

Open Code Review runs used: 0 of 2 pre-PR. This effort does not open a PR.

---

## 10. Deferred findings

| Finding | Why deferred |
|---|---|
| **The STATUS block spells its counts `[N]` where the corpus reads `(N)`**, so `workbench-attach`, `workbench-cards`, `workbench-cards-native` and `workbench-sort` still fail after every #733 assertion in them passes. See §11 | A second, independent regression, in `host_panel_models::workbench_status` on the Repositories screen: #715/#720 moved the count onto the shared list control's `" [{status}]"` suffix. #733 names "Repositories geometry, tracked separately" as a non-goal, and `tmp/issue731-agentlist/FINDINGS.md` §4.1 issue D names the `(N)` → `[N]` count form as a non-goal of *that* issue too, so nobody owns it yet. Changing it would alter a different pane's accepted behaviour with no acceptance row here |
| No hermetic scenario asserts a real `Branch:` value | The harness cannot build a git work tree and its `PATH` is closed; a shim would make the assertion vacuous (§4.4) |
| The metadata block renders after the live document rather than before it | Restoring the pre-cutover order needs a decision the issue already defers (pane title, leading description line) or a shared-control change (§4.3) |
| The dashboard agent *row* is still missing its glyph, badge, git suffix and dirty marker | Issue A of `tmp/issue731-agentlist/FINDINGS.md` §4.1 |

---

## 11. Scenario outcome at the delivered head

Reports under `tmp/issue733/green2/`.

| Scenario | Before (`caf0d56d`) | After | Remaining failure |
|---|---|---|---|
| `v1/jsp-llxprt-preview` | failed, step index 28 | **passed**, 31/31 | — |
| `v1/jsp-llxprt-preview-native` | failed, step index 28 | **passed**, 34/34 | — |
| `v1/workbench-attach` | failed, step index 28 | 48 of 49 steps pass, including step index 28 | index 48, `HAR-E005: literal 'Needs you (1)' not observed`; the frame renders `[x] Needs you [1]` |
| `v1/workbench-cards` | failed, step index 28 | 33 of 34 steps pass, including index 32's `assert-frame` for `Implement issue 522` and `JSP preview is wired` | index 33, `HAR-E006: frame does not contain 'Working (1)'`; the frame renders `[x] Working [1]` |
| `v1/workbench-cards-native` | failed, step index 28 | 37 of 38 steps pass, including index 36's `assert-frame` for `[x] Native LLxprt todo` and `Native LLxprt JSP r` | index 37, `HAR-E006: frame does not contain 'Ready (1)'`; the frame renders `[x] Ready [1]` |
| `v1/workbench-sort` | failed, step index 28 | 48 of 49 steps pass, including step index 28 | index 48, `HAR-E005: literal 'Needs you (1)' not observed` |
| `pid-commit-corner` | passed | **passed**, 8/8 | — |
| `issue731/dashboard-focus-chrome` | passed | **passed**, 17/17 | — |

Every assertion in the four still-failing scenarios that concerns the preview's
reply, todo block or elapsed row now passes; each stops on the STATUS block's
count form instead. That count is *correct* in every frame — `[1]`, `[1]`,
`[1]` — and only its spelling differs, and the code producing it
(`workbench_status`, `host_controls::project_list`) is untouched by this change.
The issue comment's expectation that all six would go green rested on the
step-28 failure being the only one; it was not, and the second cause is out of
this issue's scope. Recorded rather than fixed.
