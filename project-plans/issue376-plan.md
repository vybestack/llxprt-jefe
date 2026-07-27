# Issue 376 Delivery Plan — PR Delta Review

## Status

- Issue: <https://github.com/vybestack/llxprt-jefe/issues/376>
- Branch: `issue376`
- Baseline: `834862e` (`origin/main` at branch creation)
- Plan state: **Approved for implementation as one PR.**
- User decision: keep the existing PR list/detail screen unchanged, add Changes
  as an optional drill-down, default it to Deltas Only, and deliver the complete
  feature in this PR. Expansion beyond the soft 25-file/1,500-line target is
  approved for that purpose; stop for renewed approval before crossing 40 files
  or 2,500 net lines.

## Grounded constraints and corrected API facts

- The feature remains inside `ScreenMode::DashboardPullRequests`; it is a PR-detail drill-down, not a new top-level screen.
- GitHub's `GET /repos/{owner}/{repo}/pulls/{number}/files` returns unified `patch` hunks, not whole-file text. It can satisfy deltas-only mode but cannot satisfy full-file mode alone.
- Every file response includes an immutable Git blob `sha`. A live check against removed file `src/github/tests/mod.rs` in PR 368 proved that `GET /repos/{owner}/{repo}/git/blobs/{sha}` returns the deleted file's prior full bytes. Therefore the smallest exact full-file architecture is a lazy blob lookup by the selected file's own SHA. It works for added, modified, renamed, and removed files without adding `base_sha` to `PullRequestDetail`.
- Blob text should be fetched through GitHub GraphQL by immutable OID with `byteSize`, `isBinary`, `isTruncated`, and `text`, or an equivalently bounded typed GitHub-client call. Binary/truncated/unavailable content must be explicit; it must not be misrepresented as an empty file.
- Existing review-thread data has only `path` and `line`. Correct LEFT/RIGHT and multi-line anchoring requires the GraphQL thread fields `diffSide`, `startDiffSide`, `startLine`, and original-line data for outdated threads. Path plus line alone is ambiguous for deleted lines.
- `t` is not available for view toggling: the app-shell consumes it globally as terminal-focus toggle before PR input dispatch. The planned view toggle is `v`.
- All view state is session-only. No persistence schema or settings change is planned.
- New feature scenarios must use schema 1 under `dev-docs/tmux-scenarios/v1/` and run through `tests/harness_v1_fixtures.rs`.
- The existing PR list/detail screen remains unchanged. Changes is an optional
  drill-down entered by the user, and its default view is Deltas Only.
- To reduce GitHub API quota use, a local repository may satisfy a lazy full-file
  request with read-only `git cat-file` when the immutable blob already exists.
  A missing local object falls back to the GitHub blob read. This path never
  fetches, checks out, updates refs, or mutates the repository; remote-configured
  repositories skip the local probe.

## Proposed keys

| Context | Key | Action |
|---|---|---|
| PR detail | `d` | Enter Changes for the loaded PR and start the changed-files read |
| Changed-files list | Up/Down, PageUp/PageDown, Home/End | Move file selection |
| Changed-files list | Enter or Tab | Focus the selected file's content |
| File content | Up/Down, PageUp/PageDown, Home/End | Move the selected display row and derived viewport |
| File content | BackTab | Return focus to the changed-files list |
| Changes | `v` | Toggle Full File / Deltas Only; full content is fetched once per selected file and cached for this visit |
| Commentable diff row | `c` | Open a new line-review composer |
| Existing thread | `r` | Reply using the existing review-thread composer flow |
| Existing thread | `R` | Resolve or unresolve using the existing thread mutation flow |
| Changes content | Esc | Return to changed-files list |
| Changed-files list | Esc | Return to the same PR detail |
| Composer | Alt+Enter | Submit using the existing portable composer shortcut |
| Composer | Esc | Cancel without mutation |

`Left`/`Right` are not used as back keys inside Changes because they already mean pane cycling throughout PR mode. Esc and Tab/BackTab preserve predictable ownership.

## Screen flow

```text
PR list
  └─ Enter ─> PR detail
                └─ d ─> Changes / file-list focus
                            ├─ Enter or Tab ─> Changes / content focus
                            │                    ├─ v ─> Full File <-> Deltas Only
                            │                    ├─ c ─> new line-comment composer
                            │                    ├─ r/R ─> reply or resolve selected thread
                            │                    └─ Esc/BackTab ─> file-list focus
                            └─ Esc ─> same PR detail
```

A PR selection/detail change invalidates the prior Changes identity, cached blobs, selected file/row, load correlations, and inline target.

## State map

```text
PrChangesState
  identity: { scope_repo_id, pr_number, head_sha }
  load:
    idle | loading(request_id) | loaded | failed(error)
  files:
    ordered Vec<PrFileChange>
    selected_file: Option<usize>
    truncated_by_github_limit: bool
  focus:
    FileList | Content
  view_mode:
    DeltasOnly (default) | FullFile
  selected_row:
    Option<DiffRowIndex>
  blobs:
    per-file idle | loading(request_id) | text | binary | truncated | failed(error)
  inline:
    existing PR InlineState and composer target, reused rather than duplicated
```

The pure `pr_diff_content` projection derives viewport rows, line numbers, display colors/roles, commentability, and thread-row mappings from the domain state. The reducer stores semantic selection, not wrapped terminal rows.

## Fixed-width mockups

### Loading

```text
┌ Repositories ─────────┬ Changes — PR 376 ─────────────────────────────────────┐
│ > llxprt-jefe         │ Changed files                                         │
│                       │ Loading changed files…                                │
│                       ├───────────────────────────────────────────────────────┤
│                       │ Select a file to review                               │
└───────────────────────┴───────────────────────────────────────────────────────┘
 d changes  Esc back
```

### Loaded / full-file mode

```text
┌ Repositories ─────────┬ Changes — PR 376 — Full File ─────────────────────────┐
│ > llxprt-jefe         │ > M src/app.rs                         +12  -3         │
│                       │   A src/diff.rs                        +84  -0         │
│                       │ - D docs/old.md                         +0 -20         │
│                       ├───────────────────────────────────────────────────────┤
│                       │    41     fn render() {                               │
│                       │ -  42         old_call();                             │
│                       │ +  42         new_call();                             │
│                       │    43     }                                            │
│                       │           ┌ review by alex · unresolved ───────────┐  │
│                       │           │ Can this preserve the old fallback?    │  │
│                       │           └────────────────────────────────────────┘  │
└───────────────────────┴───────────────────────────────────────────────────────┘
 ↑↓ line  Tab files  v deltas  c comment  r reply  R resolve  Esc back
```

Added and removed rows use resolved theme success/error accents, never hard-coded RGB values. A removed file is prefixed `-` in the top list and its full prior blob is rendered as removed rows.

### Deltas-only mode

```text
┌ Repositories ─────────┬ Changes — PR 376 — Deltas Only ───────────────────────┐
│ > llxprt-jefe         │ > M src/app.rs                         +12  -3         │
│                       │ - D docs/old.md                         +0 -20         │
│                       ├───────────────────────────────────────────────────────┤
│                       │ @@ -41,3 +41,3 @@                                     │
│                       │    41     fn render() {                               │
│                       │ -  42         old_call();                             │
│                       │ +  42         new_call();                             │
│                       │    43     }                                            │
└───────────────────────┴───────────────────────────────────────────────────────┘
 ↑↓ line  Tab files  v full file  c comment  Esc back
```

### Blob or patch unavailable

```text
│ Binary or truncated file content cannot be displayed.                        │
│ File: assets/image.bin                                                        │
│ Delta metadata is also unavailable for this file.                            │
│ Press r to retry the content read, v for the available view, or Esc to back.  │
```

If patch metadata is absent but a text blob is available, Full File still displays the text with an explicit `Delta highlighting unavailable` banner. It never invents highlights.

### Changed-files failure

```text
│ Failed to load changed files: <diagnostic>                                    │
│ No GitHub mutation was attempted. Press r to retry or Esc to return.          │
```

The full diagnostic is also recorded through the existing error-store boundary.

### New line-review composer

```text
│ +  42         new_call();                                                     │
│ ┌ New review comment — src/app.rs:42 RIGHT ────────────────────────────────┐  │
│ │ Preserve the fallback here.                                             │  │
│ └──────────────────────────────────────────────────────────────────────────┘  │
│ Alt+Enter submit  Esc cancel                                                  │
```

## Decision-complete acceptance matrix

| ID | Actor / launch path | Inputs and boundaries | Target | Observable success | Failure and diagnostic | Permitted side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| A1 | User on loaded PR detail | `d`; no loaded detail; PR changes while request is in flight | Local TUI, GitHub via `gh`, macOS/Linux and native-Windows-compatible Rust paths | Enters Changes with file-list focus; starts one correlated files read; stale completion is ignored | No detail produces a notice; auth/network/parse failure renders retryable failure and Error Store entry | Read-only GitHub calls only | Transient; existing PR behavior unchanged | Key-routing/reducer tests plus schema-1 scenario |
| A2 | User viewing changed-files list | Added/modified/removed/renamed/unknown status; 0 files; pagination through GitHub's 3,000-file endpoint limit | GitHub REST files endpoint | Ordered rows show status and counts; removed rows begin with `-`; selection remains valid as pages arrive | Unknown status degrades to a typed fallback; truncation is disclosed; malformed page fails without partial semantic corruption | Read-only files-page calls | No persistence | GitHub parser/pagination tests, list projection tests, scenario |
| A3 | User opens selected file in Deltas Only | Multi-hunk patch; additions, deletions, context, `No newline` marker; absent patch | Pure domain/projection + TUI | Correct old/new line numbers and hunk rows; theme roles distinguish additions/removals | Missing patch displays an explicit unavailable state, not an empty diff | None | No persistence | Unified-diff parser and projection tests |
| A4 | User opens selected file in Full File | File status added/modified/renamed/removed; text/binary/truncated blob; path with spaces/unicode; local object present/absent; local or remote-configured repository | Local read-only Git blob with GitHub immutable-blob fallback | Lazy exact blob read; local object avoids API quota; full text shown; patch rows are mapped inline; removed file shows its full prior content as removed | Missing local object falls back without notice; binary/truncated/GitHub failure is explicit and retryable; available deltas remain usable; Error Store records terminal failure | Bounded read-only local `git cat-file` probe when applicable, then at most one read-only GitHub blob call per uncached selected file; no fetch/ref/worktree mutation and no eager reads | Cache is transient and identity-scoped | Local-present/local-missing/remote-skip Git command tests, blob argument/parser tests, full-file merge projection tests, scenario |
| A5 | User toggles view | `v` before/after blob completion; repeated toggles | Local TUI | Switches Deltas Only/Full File without re-fetching an already cached blob; default is Deltas Only and opening Changes does not fetch full blobs | If full text unavailable, Full File shows the explicit limitation and retry action | At most a lazy read when Full File first needs the blob | Transient default; no settings change | Reducer/key tests and scenario |
| A6 | User navigates Changes | File-list/content focus; small/zero terminal sizes; wrapping; selection at bounds | All supported terminals | Up/Down/Page/Home/End act in the focused area; Tab/BackTab transitions; pure projection keeps selected semantic row visible | Degenerate geometry renders a bounded empty/placeholder viewport without panic | None | Existing PR pane navigation outside Changes remains unchanged | Reducer/projection/layout tests |
| A7 | User returns | Esc in composer/content/file-list; stale loads pending | Local TUI | Esc cancels composer, then content→file-list, then file-list→same PR detail; late completions cannot reopen/change the exited view | None; back is always available during failure/loading | Cancel only; no mutation on navigation | No persistence | Key/reducer tests and scenario |
| A8 | User reads existing inline reviews | Current LEFT/RIGHT single/multi-line threads; resolved; outdated; path/anchor missing; thread beyond current file | GitHub GraphQL thread data and TUI | Threads render at matching rows with author/body/resolved/outdated state; ranges show their anchor; unmatched/outdated threads remain visible in a per-file `Unmapped reviews` section rather than being dropped | Thread-fetch degradation is disclosed while diff remains usable | Read-only thread reads already owned by PR detail/refresh flow | Existing PR-detail thread rendering/reply/resolve remains compatible | Thread parsing, anchor projection, and rendering tests |
| A9 | User creates a new line comment | `c` on commentable RIGHT or LEFT diff row; unchanged full-file row; no selection; concurrent composer | GitHub REST review-comment mutation | Existing composer opens with typed `{path,line,side,commit_id}` target; Alt+Enter creates comment; refreshed thread appears inline | Non-commentable row/no selection shows notice; mutation failure preserves draft and reports through existing mutation/Error Store path | Exactly one mutation after explicit submit; no mutation on open/cancel | Composer text transient; existing comment flows unchanged | Reducer/key/API argument/mutation tests and schema-1 scenario with fail-closed `gh` fixture |
| A10 | User replies/resolves existing thread | `r`/`R` on selected thread; resolved/unresolved; stale/missing thread | Existing GitHub thread mutations | Reuses current reply and resolve/unresolve behavior from PR detail; success refreshes thread state inline | Existing typed mutation failures preserve composer/state and remain diagnosable | Existing explicit thread mutation only | No new persistence | Existing flow regression tests plus diff-focus tests |
| A11 | User views contextual help/footer | Changes file-list/content/composer focus; terminal-focused global state | Local TUI | Footer advertises only valid keys for current focus; help includes Changes keys; `t` remains global terminal focus | None | None | Existing key semantics preserved | Pure hint tests and scenario frame assertion |

## Recorded implementation decisions

This is one cohesive addition to the existing PR screen, not a new product
subsystem or a separate top-level screen. The architectural detail only records
how the UI reads data without violating the project's existing ownership
boundaries.

1. **One PR:** all accepted behavior is delivered on `issue376` in one PR.
2. **Default and full-file loading:** Changes defaults to Deltas Only. Full File
   first tries an applicable local repository's immutable object with bounded,
   read-only `git cat-file`, then lazily reads the immutable GitHub blob when the
   object is absent. The result is cached for this visit. No Git fetch or local
   mutation is permitted.
3. **Review boundary:** create single-line LEFT/RIGHT review comments and reuse
   existing reply/resolve behavior. Existing multi-line threads render, but
   authoring new multi-line ranges and submitting overall Approve/Request
   Changes decisions remain non-goals.
4. **Keys:** `d` enters Changes and `v` toggles views. `t` remains the existing
   global terminal-focus shortcut.

## Explicit non-goals

- Side-by-side diff, syntax highlighting, word-level diff, blame, commit-by-commit review, and local working-tree comparison.
- Multi-file selection, bulk actions, staging, checkout, merge, or any local Git mutation from Changes.
- Authoring multi-line review ranges unless explicitly approved at the gate.
- Approve, Request Changes, Dismiss Review, or pending-review batching unless explicitly approved at the gate.
- Commenting on unchanged full-file lines outside GitHub's patch-addressable rows; GitHub's review-comment API does not accept arbitrary unchanged lines outside the diff.
- Fetching every full blob eagerly; running `git fetch`; changing refs, index, or
  worktree; probing a configured remote repository over SSH; durable blob
  caching; or persistence/settings changes.
- New crates or changes to `Cargo.toml`/`Cargo.lock`.
- Changes to `.llxprt/`, `.code_puppy/`, `.github/`, quality gates, workflows, or agent memory.
- Unrelated PR-mode refactors, test moves, or broad redesign of the existing message bus.
- New legacy tmux scenarios or shell-based scenario runners; the accepted feature scenario is schema 1.

## Bounded vertical slices

### Slice 1 — File list and deltas-only drill-down

- Acceptance: A1, A2, A3, A6 (file list), A7, initial A11.
- Owner/boundary: typed diff domain + GitHub files reads + PR reducer/input + thin UI/pure projection.
- RED first: schema-1 scenario enters PR detail, presses `d`, observes removed `-` row, opens a patch, and returns; parser/reducer/projection/key tests fail for missing behavior.
- GREEN: files load is correlated and paginated, patch parsing is total, deltas-only rendering is themed, back navigation works.
- Non-goals: blob reads, thread rendering, or any mutation.
- Stop: a dependency/workflow change, new process subsystem, path outside the expected set, or projected PR over the hard budget.

### Slice 2 — Lazy full-file rendering

- Acceptance: A4, A5, remaining A6/A11.
- Owner/boundary: immutable blob read at GitHub boundary; pure full-file/patch merge projection; transient state cache.
- RED first: removed-file full-content, modified-file interleaving, binary/truncated/failure, toggle-cache, and scenario full/delta assertions.
- GREEN: all statuses render exact full text when available, including removed files; unavailable text is explicit; `v` does not duplicate cached reads.
- Non-goals: inline thread rendering or mutation.
- Stop: eager global cache, persistence, dependency, or public abstraction not listed below.

### Slice 3 — Existing inline threads

- Acceptance: A8 and read-only part of A10.
- Owner/boundary: GitHub thread anchor enrichment + pure thread-to-diff projection; reuse existing PR thread model/refresh ownership.
- RED first: LEFT/RIGHT/range/outdated/unmapped parser and projection cases.
- GREEN: no valid thread is silently dropped; mapped threads appear inline; unmatched threads appear in the explicit section.
- Non-goals: authoring a comment.
- Stop: moving thread ownership to a second subsystem or unrelated detail-view redesign.

### Slice 4 — New comments, replies, and resolve

- Acceptance: A9, mutation part of A10, final A11.
- Owner/boundary: typed composer target + existing mutation orchestration + REST line-review call.
- RED first: key/reducer/API argument/failure tests and schema-1 fail-closed `gh` mutation path.
- GREEN: single-line LEFT/RIGHT comment creation, reply, and resolve work from Changes; drafts survive failure; refresh is stale-safe.
- Non-goals: new multi-line ranges and overall review decisions.
- Stop: any additional review workflow, batching subsystem, or unplanned mutation.

## Expected paths for the single PR

Exact path use may shrink after RED. Any path not listed here must enter the scope ledger and be approved when required.

### Slices 1–2 expected paths

| Layer | Expected paths | Rows |
|---|---|---|
| Plan/evidence | `project-plans/issue376-plan.md`; `dev-docs/tmux-scenarios/v1/pr-delta-review.json`; `tests/harness_v1_fixtures.rs` | A1–A7, A11 |
| Domain | new focused module under `src/domain/` for changed-file, blob, hunk/line/anchor types; `src/domain/mod.rs` module declaration only | A2–A5 |
| GitHub boundary | new focused `src/github/pr_diff.rs`; `src/github/mod.rs` declaration/export only | A1–A5 |
| State | `src/state/pr_types.rs`; new `src/state/prs_diff_ops.rs`; `src/state/events.rs`; `src/state/mod.rs` declarations/routing | A1–A7 |
| Messages | `src/messages.rs`; `src/messages/prs_conversion.rs`; `src/messages/message_names.rs`; `src/messages/names.rs` | A1–A7 |
| Orchestration/input | `src/app_input/prs.rs`; `src/app_input/prs_orchestration.rs`; new focused `src/app_input/prs_diff_dispatch.rs`; `src/app_input/mod.rs` declaration | A1–A7 |
| Pure view | new `src/pr_diff_content.rs`; `src/lib.rs` declaration | A2–A6 |
| UI/theme | new `src/ui/components/pr_diff.rs`; `src/ui/components/mod.rs`; `src/ui/components/scrollable_text.rs`; `src/ui/components/selectable_list.rs`; `src/ui/screens/pull_requests.rs`; `src/ui/components/keybind_bar.rs`; `src/theme/mod.rs`; `src/layout.rs` only if existing geometry helpers cannot express the layout | A2–A7, A11 |
| Co-located tests | Existing/new `*_tests.rs` only adjacent to the owners above; no unrelated test moves | Corresponding rows |

Projected slices 1–2: 22–28 files, 1,250–1,700 net lines.

### Slices 3–4 expected paths

| Layer | Expected paths | Rows |
|---|---|---|
| Domain/GitHub threads | `src/domain/mod.rs` or the accepted focused thread-anchor type module; `src/github/parse_pr.rs`; `src/github/pr_threads.rs`; affected thread parser tests | A8–A10 |
| State/messages | `src/state/types.rs`; `src/state/prs_diff_ops.rs`; existing thread/inline ops only where reuse requires it; `src/state/events.rs`; `src/messages.rs`; PR conversion/name modules | A8–A10 |
| Orchestration/input | `src/app_input/prs.rs`; `src/app_input/prs_mutation.rs`; `src/app_input/prs_orchestration.rs` only as required by existing routing | A9–A10 |
| Projection/UI | `src/pr_diff_content.rs`; `src/ui/components/pr_diff.rs`; `src/ui/components/detail_pane.rs` only if the existing composer target match must be made exhaustive; `src/ui/components/keybind_bar.rs` | A8–A11 |
| Scenario/evidence | `dev-docs/tmux-scenarios/v1/pr-delta-review.json`; `tests/harness_v1_fixtures.rs`; this plan's evidence ledger | A8–A11 |

Projected slices 3–4: 12–20 files, 650–1,100 net lines. The combined
PR is expected to exceed the soft target, which the user approved to deliver the
complete feature together. A mandatory scope review is required as soon as the
working diff exceeds 25 files or 1,500 net lines. Adding anchor fields directly
to `PrReviewThread` may touch many literal fixtures; prefer a cohesive typed
anchor design over fixture churn or a test-only workaround. Stop and request
renewed approval before exceeding 40 files or 2,500 net lines.

## Scope ledger

| Entry | Discovered work | Classification | Decision / authorization | Paths | Status |
|---|---|---|---|---|---|
| S-001 | PR files `patch` is hunk-only; full file needs another read | In-scope requirement | Lazy immutable blob lookup accepted; Deltas Only is the default | GitHub/domain/state/projection paths | Approved |
| S-002 | Removed files still expose their prior immutable blob SHA | In-scope correction | Render prior full file as removed; no placeholder-only shortcut | Same as S-001 | Planned |
| S-003 | `t` is consumed globally before PR dispatch | In-scope UX constraint | Use `v`; no global shortcut change | PR input/footer/help | Planned |
| S-004 | Existing thread `path` + `line` cannot disambiguate LEFT/RIGHT/ranges | In-scope correctness | Enrich thread anchors in slices 3–4; single-line authoring boundary recorded | Domain/GitHub/parser/projection | Approved |
| S-005 | New feature scenarios must use schema 1 | Required existing policy | Add v1 fixture and CI fixture test; no harness changes planned | `dev-docs/tmux-scenarios/v1`, test fixture ledger | Planned |
| S-006 | Combined feature likely exceeds soft PR target | Delivery guardrail | User requires one PR and approves soft-target expansion; mandatory scope review retained | All | Approved |
| S-007 | Full-file GitHub reads consume quota even when the blob is already local | In-scope optimization requested by user | Try bounded read-only local `git cat-file` for local repositories, fall back to GitHub on absence; never fetch or mutate; skip remote-configured repositories | Existing local-command seam plus diff dispatch tests | Approved |
| S-008 | Working scope reached 27 files / 1,753 net lines during Slice 1 | Mandatory soft-budget review | Reviewed all 27 paths: each maps to A1–A7/A11 plan, scenario, boundary, state, input, or projection evidence; no dependency/workflow/persistence/unrelated change. Continue the approved one-PR delivery while pruning duplication and remeasure before each slice | Current issue diff | Reviewed |
| S-009 | Complete accepted behavior reached 49 files / 3,407 net lines including plan, schema-1 fixture, fail-closed shim, and focused tests | Hard-budget review | User explicitly instructed completion in one PR and renewed authorization with “do it”; all paths map to A1–A11. Source-size and strict Clippy gates pass after cohesive extraction; no dependency, workflow, persistence, or unrelated production change was introduced | Complete issue diff | Approved and reviewed |
| S-010 | Final review remediation and mutation scenario evidence reached 51 files / 4,075 net lines | Mandatory renewed scope review | The previously authorized one-PR hard-budget exception remains explicit. The two added paths are focused Changes key tests and the fail-closed fixture; growth is tests, plan evidence, and accepted correctness remediation only. No dependency/workflow/persistence/quality configuration or unrelated production path was added. | Complete issue diff | Approved authorization retained; reviewed |
| S-011 | Final pre-commit scope is 52 files / 4,509 additions / 94 deletions / 4,415 net lines | Exact-head mandatory scope review | Reviewed all 37 tracked modifications and 15 new files against A1–A11 and the approved combined-PR exception. The additional growth is the completed plan/OCR ledger, restored module provenance documentation, deterministic loaded-suite scenario synchronization, and focused remediation coverage; all production paths remain in the planned domain, GitHub, message, state, orchestration, projection, and UI layers. No dependency, workflow, persistence, quality configuration, `.llxprt`, or unrelated production change is present. | Complete candidate diff | Approved authorization retained; clean |

No unapproved production paths have been changed.

### Local OCR run 1 triage

Run `20260727T001340Z-48abb059` completed with 44 comments across 48 reviewed files and `complete_best_effort` coverage. Its checksum ledger validated and stderr was empty, but the manifest recorded provider/model `zai-anthropic` / `glm-5.2`, not required StepFun `step-3.7-flash`; the workspace also changed afterward. It therefore counts as local run 1 and supplies finding hypotheses, but is not the final required review.

| Findings | Classification | Disposition | Source-checked outcome |
|---|---|---|---|
| 3, 4, 16, 18, 44 | Blocker-Fix | Fixed | Content navigation, file switches, and view toggles now clamp against the same threaded document rendered by the UI. |
| 1 | Blocker-Fix | Fixed | `c`/`r`/`R` require Changes Content focus; stale content selection cannot act from the file list. |
| 5, 6 | Blocker-Fix / In-scope-Fix | Fixed | Review-comment/reply success no longer enters top-level issue comments; ordinary comments preserve existing append/scroll behavior after composer closure. |
| 9, 26, 27 | Blocker-Fix | Fixed | Local non-UTF-8 falls through to authoritative GitHub blob metadata; local content-read errors retain stderr instead of silently degrading. |
| 13, 14, 36 | In-scope-Fix | Fixed | Changes conversion consumes values without unrelated cloning/boxing and uses `ControlFlow` to satisfy strict Clippy. |
| 17 | In-scope-Fix | Fixed | Blob cache is bounded to eight entries with oldest-first eviction and duplicate in-flight suppression. |
| 24, 25 | Blocker-Fix / Reject | Fixed / Dismissed | Fixture POST routing matches production argv; the single-object REST response shape was already correct. |
| 28, 30–33, 37–42 | In-scope-Fix | Fixed | Added focused parser/projection/reducer/key/window/empty-selection tests; retained trailing removed rows and bounded list/document windows. |
| 34, 35 | In-scope-Fix | Fixed | Thread anchors use an indexed ordered map rather than repeated reverse scans; shadowed line naming was clarified. |
| 43 | In-scope-Fix | Fixed | Composer uses stable user-facing target labels instead of debug output. |
| 2, 7, 8, 10–12, 15 | In-scope-Fix | Fixed | Corrected displaced/misleading documentation, unnecessary clones, and formatting in touched issue paths. |
| 19, 20 | Reject | Explained/dismissed | Changes is an intentional drill-down, not a fourth global pane; local Tab/BackTab is consumed and Escape performs the documented unwind. |
| 21, 22 | In-scope-Fix | Fixed where active | Changes projection is bounded and only mounted when active; the PR screen's existing snapshot clone contract is unchanged. |
| 23 | Reject | Explained/dismissed | A fixture-specific schema-1 shim install is bounded evidence, not authorization for a new harness registration subsystem. |
| 29 | Reject with test | Explained/dismissed | The reducer already guards missing selection; a focused no-selection key safety test now locks the behavior. |
| 39, 40 | In-scope-Fix duplicates | Fixed | Duplicate cleanup/test observations are covered by the grouped fixes above. |

Additional source-check findings found during triage were fixed in scope: threaded-document selection/anchor consistency, full-file patch-addressability, full-file loads on file navigation, correlated repository-resolution failures, actual page-count navigation, and refreshed thread preservation while Changes remains active.

### Local OCR run 2 triage

Run `20260727T041909Z-eb4f5ebd` completed successfully with exit 0 and
`complete_best_effort` coverage. It used Open Code Review `v1.7.16
(a0b49d5b) darwin/arm64`, the StepFun OpenAI-compatible protocol with model
`step-3.7-flash`, and arguments `--audience agent --format json --concurrency 2
--timeout 20`. It reviewed 50 files and emitted 21 comments;
`comparison_eligible` was false. Every `sha256.txt` entry validated, and the
worktree matched `manifest.pre.json` immediately after the run. `stderr.log`
contains one failed attempt to read nonexistent `src/messages/mod.rs`; the run
continued and completed successfully. This consumes the second and final local
OCR slot.

| Finding | Validity | Workflow classification / disposition | Source-checked outcome |
|---:|---|---|---|
| 1 | Partial | Blocker-Fix — fixed | Removed the pre-reducer `PrInlineDraft` capture. Submission now clones body, target, mutation identity, repository scope, and PR number from one post-reducer read snapshot, rejects composer/pending target mismatches, and has focused matching/mismatch coverage. |
| 2 | Valid | In-scope-Fix — fixed | Changes PageUp/PageDown now use viewport-derived `PageItemCount`; focused small/large-terminal coverage proves the prior fixed size 10 was wrong. |
| 3 | Invalid | Reject — explained/dismissed | Hunk headers already support the optional heading suffix. Exact no-newline markers are handled as rows and do not affect hunk-header parsing. |
| 4 | Valid | In-scope-Fix — fixed | Restored the domain module's existing `@plan` and `@requirement` provenance annotations. |
| 5 | Invalid | Reject — explained/dismissed | Review-comment fields are passed to `gh api` as distinct `-f`/`-F` argv entries; `gh` owns form serialization, so caller-side percent encoding would corrupt literal values. |
| 6 | Partial | In-scope-Fix — fixed | Added focused RIGHT-side argv coverage with spaces, `&`, `=`, Unicode, and a newline, proving body/path/commit/line/side remain distinct literal arguments. |
| 7 | Valid | In-scope-Fix — fixed | Aligned the two Changes blob message-name arms with the surrounding match. |
| 8 | Invalid | Reject — explained/dismissed | `PrChanges` is an intentional PR-detail drill-down, not a fourth global focus. It locally owns Tab/BackTab and Escape by the accepted screen flow. |
| 9 | Invalid | Reject — explained/dismissed | Review-comment success intentionally waits for authoritative detail/thread refresh rather than appending a REST review comment to top-level issue comments. Reducer coverage and the schema-1 scenario prove the refreshed inline thread appears. |
| 10 | Partial | In-scope-Fix — fixed | The schema-controlled marker was already fixture-only; it now additionally requires an absolute workspace and the exact contained `${workspace}/comment-created` path, failing closed with exit 64 otherwise. |
| 11 | Partial | In-scope-Fix — fixed | The PR screen now avoids cloning Changes state and projecting/cloning review threads while Changes is inactive. The existing whole-screen snapshot contract remains unchanged. |
| 12 | Valid | Defer — explained | The existing PR-detail `m` assertion is adjacent-route test organization, not accepted Changes behavior. Moving unrelated coverage is explicitly out of scope and no production defect is present. |
| 13 | Partial | Reject — explained/dismissed | Local object absence and non-UTF-8 content deliberately return a local miss and fall through to the authoritative bounded GitHub blob read. Local successful text and oversized blobs retain their optimized paths; no fetch or repository mutation occurs. |
| 14 | Valid | Blocker-Fix — fixed | Unified-diff parsing now accepts only context/add/remove rows and the exact `\\ No newline at end of file` marker; unknown prefixes produce a typed malformed result. |
| 15 | Valid | In-scope-Fix — fixed | Focused tests prove the no-newline marker advances neither side and preserves Removed LEFT line 1 and Added RIGHT line 1 addressing. |
| 16 | Valid | In-scope-Fix — fixed | Outdated thread labels now use `original_line` when `original_start_line` and current line data are absent, matching anchor projection. |
| 17–19 | Valid duplicate root | Blocker-Fix — fixed | Selection, refresh success, and refresh failure now clear obsolete blob pending/error activity; same-blob duplicate suppression remains, cached selection clears old activity, and focused reducer tests cover each transition. |
| 20–21 | Invalid duplicate root | Reject — explained/dismissed | File/document entries are enumerated before slicing, so indices remain absolute after bounded windowing; no window-relative index is stored or dispatched. |

No second-OCR finding authorizes behavior or paths beyond A1–A11. Finding 12
remains outside implementation scope; finding 13 documents the accepted local
optimization/fallback boundary. All valid in-scope roots are remediated.

## Review counters

| Review type | Allowed | Used | Remaining |
|---|---:|---:|---:|
| Local Open Code Review before PR | 2 | 2 | 0 |
| Open Code Review after PR | 2 | 0 | 2 |

Each finding will be recorded as `Blocker-Fix`, `In-scope-Fix`, `Reject`, or `Defer`. Reviewer output does not authorize a path or behavior outside this plan.

## Verification evidence

| Check | Candidate SHA | Result | Evidence |
|---|---|---|---|
| TUI scenario RED | `834862e` + test changes | RED as intended | Schema-1 real-process fixture reached loaded PR detail, sent `d`, and timed out waiting for `Changes — PR 376 — Deltas Only`; captured frame remained on PR detail |
| Focused unit/integration tests | working tree | GREEN | Domain/parser/projection/reducer/message/key tests pass, including viewport page counts, strict patch rows/no-newline numbering, outdated-thread labels, blob activity cleanup, literal RIGHT-side argv, and coherent post-reducer inline-submit snapshots. |
| `make quick-check` | working tree | PASS | Fresh post-OCR run passed format/check and all workspace tests: library 2,394 passed / 1 ignored, binary 764 passed, and every integration/doctest target passed. |
| Required exact gates | working tree | PASS | Fresh post-OCR exact constituent gates passed: format; Clippy allow/source-size/complexity checks; strict workspace/all-target/all-feature Clippy; coverage at 71.36% lines; locked all-feature workspace build; and locked all-feature workspace tests (2,394 library passed / 1 ignored, 764 binary passed, all integration/doctest targets passed). A silent combined `make ci-check` confirmation was externally terminated during coverage, so it is recorded as incomplete rather than passed; every exact constituent gate was run directly. `src/domain/mod.rs` is exactly at the 1,000-line hard limit. |
| Schema-1 `pr-delta-review` scenario | working tree | PASS | Fresh post-OCR standalone real-process run passed all 32 steps. The same fixture passed in the locked workspace suite after its bounded post-mutation refresh wait was raised from 15,000 to 30,000 ms for loaded-suite determinism. It opened Deltas Only, rendered changed/removed lines, lazily loaded Full File, submitted RIGHT-side `src/app.rs:3`, asserted the contained marker, refreshed/rendered the inline thread, and returned through file list to unchanged PR detail. |
| Local OCR | working tree | 2/2 complete and triaged | Both permitted pre-PR runs are recorded above. Run 2 used StepFun `step-3.7-flash`; every one of its 21 comments is source-checked and disposed. No additional pre-PR OCR run is permitted. |
| PR OCR | — | 0/2 used | Awaiting PR |
| CI / conflict / ancestry | — | Not run | Awaiting PR |

## Deferred findings / follow-ups

None. Valid out-of-scope review findings will be recorded here with a follow-up issue rather than silently expanding issue 376.

## Exact-head completion checklist

- Every accepted A-row has behavioral evidence on the candidate head.
- Non-goals remain absent.
- Every changed file maps to an A-row and an approved slice.
- Scope ledger has no pending/unapproved implementation item.
- Required local verification and schema-1 scenario pass on exact head.
- CI passes on exact head, including native Windows/coverage gates.
- Correct ancestry and conflict-free PR state are verified.
- Required reviews are complete; every finding is triaged and all `Blocker-Fix`/`In-scope-Fix` findings are resolved.
- OCR counters remain within 2 local / 2 PR runs.
- Delivery stops when these conditions are met; no optional hardening follows.
