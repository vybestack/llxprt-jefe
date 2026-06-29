# PR Mode Implementation Plan — Overview

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Issue:** #20 (main GitHub PR integration); related closed design issue #46
- **Total Phases:** P00A / Phase 0.5 preflight is the PLAN-TEMPLATE PREFLIGHT VERIFICATION artifact —
  a pre-execution grounding GATE, NOT a worker/verifier pair. It has no `P00` worker counterpart by
  design: it only re-grounds the plan's assumptions against current `src/` and external tools (gh
  qualifiers, JSON shapes, line citations) and emits `.completed/P00A.md`. It does not write any
  production code, so the paired worker→verifier coordination rule does NOT apply to it. The PAIRED
  worker/verifier protocol BEGINS at P01/P01A: the 16 build phases `P01..P16` EACH have a paired
  `a`-verification counterpart (`P01`/`P01A` … `P16`/`P16A`), where a worker phase produces code and
  its `a`-counterpart independently verifies it. So the phase set is: 1 standalone P00A / Phase 0.5
  preflight gate + 16 worker phases (P01..P16) + 16 paired verifier phases (P01A..P16A).
- **Requirements range:** REQ-PR-001..REQ-PR-014, REQ-PR-NFR-001..REQ-PR-NFR-003
- **Exemplar mirrored:** `project-plans/issue15/` (Issues Mode, `PLAN-20260329-ISSUES-MODE`)

## Critical Reminders

1. **Plan documents only here.** Execution happens phase-by-phase by separate workers/verifiers.
2. **Mirror the CURRENT (post-fix) Issues Mode patterns**, not the original buggy ones. The
   codebase now has: a typed message bus (`src/messages.rs`), a typed layout module
   (`src/layout.rs`), off-thread gh I/O (`src/app_input/gh_async.rs`), the `ScrollableText`
   TEXT-region scroller (used by `issue_detail.rs`), and `assert!(matches!(...))` test hygiene.
   NOTE: the Issues list does NOT yet window its rows — `issue_list.rs` renders all rows with no
   offset and there is NO shared list-row selection-follow helper to reuse. PR Mode must BUILD a
   new shared list-viewport / selection-follow helper as an explicit deliverable (see REQ-PR-006,
   P12/P13/P14). `ScrollableText` is a TEXT-line scroller for the detail pane, NOT a row-list
   scroll abstraction.
3. **No clippy allows, no threshold overrides.** `scripts/check-clippy-allows.sh` fails CI on ANY
   first-party clippy allow/expect in every spelling (`#[allow(clippy`, `#![allow(clippy`,
   `cfg_attr(...allow(clippy`, and the `expect` equivalents) and ALSO asserts the two clippy configs
   stay in sync. The thresholds (cognitive-complexity 15, too-many-lines 60, too-many-arguments 6,
   type-complexity 250, max-struct-bools 3) live in BOTH `./clippy.toml` (local) and
   `./.github/clippy/clippy.toml` (CI, via `CLIPPY_CONF_DIR`) and must NOT be raised in either file.
   Split handlers/render fns so no function ever needs an override (see the P16 no-override gate).
4. **TDD is mandatory.** Every impl phase is preceded by a TDD phase whose RED tests must fail
   first. No `assert!(true)`, no `#[ignore]`, no mock theater.
5. **Additive integration.** PR Mode adds `prs_state` and new enum variants; existing flows are
   unaffected. `prs_state` is transient (excluded from persisted state).
6. **Symbol names are authoritative; cited line numbers are guidance.** Every `file:line` citation
   in this plan (e.g. `mod.rs L744-769`, `issues.rs L149-150`, component-pseudocode line refs) is a
   convenience pointer captured at planning time and may DRIFT as the codebase evolves. Before
   editing or asserting against any cited location, each worker and verifier MUST first locate the
   referenced item BY ITS SYMBOL NAME (function/type/field/variant name, marker, or
   `@pseudocode`/`@requirement` tag) and re-confirm/refresh the line number during preflight. If a
   citation has drifted, use the symbol-resolved location and note the corrected line; NEVER edit a
   line purely because a stale citation pointed at it. When a symbol cannot be found by name,
   treat it as a blocker and escalate rather than guessing from the line number.

## Global Coordination Protocol (binds to `dev-docs/COORDINATING.md`)

- Phases execute in strict numeric order. No skipping, no batching.
- Each phase has one worker and one verifier (the paired `NNa` document).
- Phase N+1 is blocked until Phase N's verification returns `PASS` and the prerequisite gate holds:
  `.completed/P(N-1).md` exists AND `.completed/P(N-1)A.md` exists with a PASS verdict AND the
  expected artifacts/files from N-1 are present.
- On `FAIL`: remediate within the same phase. After 3 failed attempts, escalate (deepthinker /
  user) — do not advance.
- A todo item is created for EVERY phase and EVERY verification before execution begins.

## Verifier Output Contract (every `NNa` phase)

A verifier MUST produce:
1. **Structural verification** — compiles; required new types/variants present; existing enums
   unchanged; all match arms updated; traceability markers present in every changed file.
2. **Behavioral code-reading evidence** — cited `file:line` proving each REQ behavior is realized
   (not just that a symbol exists).
3. **Runtime-path reachability** — trace key → route → `AppEvent` → `AppMessage` →
   `dispatch_app_message` → `AppState::apply_message` → `apply_prs_message` → render, citing each
   hop's `file:line`.
4. **Contradiction scan** — confirm no code path contradicts the requirement (e.g. a silent
   `None` arm dropping context, a duplicated constant, scroll math reading `crossterm::size()`).
5. **Atomic verdict** — `Phase NN: PASS` or `Phase NN: FAIL` with remediation steps.

> A verifier that writes "all checks passed" without cited `file:line` evidence is itself a FAIL.

## Mockup-Driven Layout Contract

PR Mode follows the Issues Mode two-column shell (see `mockups.md`):
- Column 1: repositories sidebar, width `PRS_SIDEBAR_WIDTH = LEFT_COL_WIDTH = 22` units.
- Column 2: PR workspace (flex), containing — top: optional error banner; optional filter band;
  PR list region (height from `prs_pane_rows`); bottom: unified scrollable detail view
  (metadata + body + reviews summary + checks summary + comments + composer).
- KeybindBar + StatusBar reused (shared components).

Seven placement acceptance checks (verified in P14A/P16):
1. Sidebar is exactly 22 units wide.
2. List region height equals `prs_pane_rows(...)` output (no clipping; selection-following).
3. Detail viewport height is passed as a prop derived from `prs_detail_viewport_rows(...)` — NOT
   read independently from `crossterm::size()`.
4. Error banner appears above the list only when an error is present.
5. Filter band appears only when filter controls are open.
6. Agent chooser overlay renders at the documented offset.
7. Long titles truncate with ellipsis by pane width.

## Global Mandates

- **Pseudocode line ranges.** Every impl phase cites `component-NNN lines X-Y` it realizes.
- **Per-file traceability markers.** Every changed/new fn, struct, enum, and module carries Rust
  doc-comment markers `@plan PLAN-20260624-PR-MODE.PNN`, `@requirement REQ-PR-NNN`,
  `@pseudocode component-NNN lines X-Y`. Grep-verified in verification phases.
- **Event-driven handlers.** Key handlers return `Option<AppEvent>`; transitions occur via
  `AppState::apply_message(...)` — never via direct `app_state.write()` mutation in handlers.
- **Async non-blocking gh I/O.** All `gh` calls run off-thread via `spawn_gh_task_with_panic`.
- **Reuse shared modules.** Reuse the layout module (no duplicated constants), the agent chooser,
  sidebar, status bar, keybind bar, `route_search_key`, `IssueComment`, `GhError`, and
  `ScrollableText` for the PR-detail TEXT region. Do NOT claim to reuse a list-row scroll pattern
  from `issue_list` — it has none (it renders all rows with no offset). The PR list's
  selection-follow viewport is a NEW shared helper added to `src/layout.rs` — stubbed in P03,
  RED-tested in P04, implemented in P05, and consumed by the PR list in P14 (see "New shared
  list-viewport helper" below).

## New Shared List-Viewport / Selection-Follow Helper (explicit deliverable)

The PR list selection-following requirement (REQ-PR-006, the #55 regression guard) maps to a CONCRETE,
NEW implementation — it does NOT reuse any existing list-scroll helper, because none exists:

- Current reality (grounded in `src/`): `src/ui/components/issue_list.rs` renders ALL rows via
  `props.issues.iter().enumerate()` (L137, verified) with no offset/viewport; `src/ui/screens/issues.rs`
  passes the full issue vec to `IssueList` with no scroll offset; `src/ui/components/scrollable_text.rs`
  windows TEXT LINES by `scroll_offset` (used only by `issue_detail.rs`) and is NOT a row-list
  abstraction. `src/layout.rs` has detail-pane viewport helpers but no list-row viewport helper.
- Deliverable: a new shared, pure pair of fns added to `src/layout.rs` (NOT a UI file — the STATE
  reducers consume them, so they must live in a module importable by both `state` and `ui` without a
  boundary violation; `src/layout.rs` is already that shared leaf module). They compute the
  first-visible row index from `(selected_index, loaded_len, viewport_rows)` so the selected row is
  always within `[first_visible, first_visible + viewport_rows)`; they clamp at both ends and never
  drop rows. There is NO `src/ui/components/list_viewport.rs`.
- Pseudocode: `analysis/pseudocode/component-001.md` "List Viewport / Selection-Follow Helper",
  lines 182-196 (`list_first_visible_index` lines 182-189, `list_visible_window` lines 190-196).
- Lifecycle: STUB in P03 (`src/layout.rs`, total wrong-value/clippy-clean) → RED pure-logic tests in
  P04 → IMPL in P05 → consumed by state reducers (P05) and by `pr_list.rs` (P14).
- Tests (P04 RED, pure-logic): selecting row 0 → offset 0; selecting the last row → offset keeps last
  row visible; navigating Down past the viewport bottom advances the offset by one; N loaded rows
  render exactly N rows when they fit (no clipping, no dropped rows — #54/#55).
- Both the PR state reducers and the PR list (`pr_list.rs`, consuming `crate::layout` helpers +
  `prs_pane_rows(...)`) use the SAME helpers; they are written so the issue list can later adopt them,
  but PR Mode is where they are first built and tested.

## Glossary / Terminology Mapping

| Spec term | Code construct |
|-----------|----------------|
| PR Mode | `ScreenMode::DashboardPullRequests` |
| PR pane focus | `PrFocus { RepoList, PrList, PrDetail }` |
| Detail subfocus | `PrDetailSubfocus { Body, Review(i), Check(i), Comment(i), NewComment }` |
| PR state aggregate | `PullRequestsState` (field of `AppState`) |
| PR list row | `domain::PullRequest` |
| PR detail | `domain::PullRequestDetail` |
| Review summary item | `domain::PrReview` |
| Check summary item | `domain::PrCheck` |
| PR filter | `domain::PrFilter` + `PrFilterState` |
| PR input modes | `InputMode::{PrsNormal,PrsInline,PrsSearch,PrsFilter,PrsChooser}` |
| PR message domain | `MessageDomain::PullRequests` + `PullRequestsMessage` |
| PR reducer hub | `AppState::apply_prs_message` |

## Baseline-to-Target Enum Evolution

### ScreenMode
```text
BEFORE: enum ScreenMode { Dashboard, Split, DashboardIssues }
AFTER : enum ScreenMode { Dashboard, Split, DashboardIssues, DashboardPullRequests }
                                                              ^^^^^^^^^^^^^^^^^^^^^^^ added
```

### Focus (new enum; PaneFocus + IssueFocus UNCHANGED)
```text
ADD: enum PrFocus { RepoList, PrList, PrDetail }
ADD: enum PrDetailSubfocus { Body, Review(usize), Check(usize), Comment(usize), NewComment }
```

### InputMode
```text
BEFORE: { Normal, TerminalCapture, Help, Search, Form, Confirm,
          IssuesNormal, IssuesInline, IssuesSearch, IssuesFilter, IssuesChooser }
AFTER : ...same... + PrsNormal, PrsInline, PrsSearch, PrsFilter, PrsChooser
```

### MessageDomain / AppMessage
```text
MessageDomain: ...existing... + PullRequests
AppMessage:    ...existing... + PullRequests(PullRequestsMessage)
```

### AppEvent (additive — existing variants UNCHANGED)
```text
+ Lifecycle:  EnterPrsMode, ExitPrsMode, RefocusPrList
+ Nav/Focus:  PrNavigate{Up,Down,PageUp,PageDown,Home,End}, PrListEnter,
              PrCycleFocus(Reverse), PrScrollDetail{Up,Down,PageUp,PageDown},
              PrDetailSubfocus{Next,Prev}
+ Data:       PrListLoaded/LoadFailed/PageLoaded, PrDetailLoaded/LoadFailed,
              PrCommentsPageLoaded/Failed
+ Filter/Search: PrOpen/CloseFilterControls, PrApply/ClearFilter, PrFilterNavigate{Next,Prev},
              PrCycleFilterState, PrCycleDraftFilter, PrUpdateDraftFilter,
              PrFocus/BlurSearchInput, PrSetSearchQuery, PrApply/ClearSearch
+ Inline:     PrOpenNewCommentComposer, PrOpenReplyComposer, PrInline*, PrCommentCreated,
              PrCommentCreateFailed, PrMutationFailed
+ Notice:     PrShowNotice(ReadOnlyHintKind)   // consumed no-op + non-blocking hint for invalid
                                               // r/c/e on read-only subfocus (REQ-PR-010/013);
                                               // reducer sets prs_state.draft_notice (no silent None)
+ Agent:      PrOpenAgentChooser, PrAgentChooserNavigate{Up,Down}, PrAgentChooserConfirm,
              PrAgentChooserCancel, PrSendToAgentCompleted, PrSendToAgentFailed
```

### AppState
```text
+ pub prs_state: PullRequestsState   // RUNTIME-ONLY: AppState derives only Debug/Default/Clone
                                     // (NOT Serialize/Deserialize), so there is NO #[serde(skip)].
                                     // It is simply omitted from the persisted-state DTO/mapping,
                                     // exactly as issues_state is (to_persisted_state never reads it).
```

### Persistence / Repository
```text
NO new persisted field. PR Mode reuses Repository.github_repo + Repository.issue_base_prompt.
```

## How Existing Behavior Is Preserved

1. Existing `ScreenMode`, `IssueFocus`, `PaneFocus`, `IssuesMessage`, and `AppEvent` variants are
   untouched; only additive variants are introduced.
2. Existing key routing for Dashboard/Issues/Split modes is unchanged; `p`/`P` only triggers when
   `screen_mode == Dashboard`.
3. `prs_state` defaults to inactive and is runtime-only; the persisted-state DTO mapping
   (`to_persisted_state`) never reads it (just as it never reads `issues_state`), so the on-disk
   format and backward-compat are unaffected (a backward-compat test asserts a pre-PR-mode persisted
   file still loads and yields default/inactive `prs_state`).
4. The reducer hub adds one `AppMessage::PullRequests` arm; all existing arms are preserved.

## REQ → Phase → Pseudocode Traceability Matrix (compact)

| REQ | Phases | Pseudocode |
|-----|--------|-----------|
| REQ-PR-001 mode entry/exit | P03–P05, P09–P11 | c001 L66–87, c003 L01–09, c004 L53–55,70 |
| REQ-PR-002 key routing & suppression (+ `AppEvent`↔`PullRequestsMessage` round-trip) | P03–P05 (conversion: stub P03, RED P04, GREEN P05), P09–P11 (key routing) | c003 L10–48, c004 L45–85 (round-trip invariant) |
| REQ-PR-003 pane focus & nav (repo-nav fix #47) | P03–P05, P09–P11 | c001 L99–162, c003 L49–56 |
| REQ-PR-004 Esc precedence | P09–P11 | c003 L92–98 |
| REQ-PR-005 exit-focus restoration | P03–P05 | c001 L77–87 |
| REQ-PR-006 PR list display/sort/scroll (#54,#55) | P03–P05 (shared list-viewport helper in `src/layout.rs`: STUB P03 → RED P04 → IMPL P05 → consumed P14), P06–P08, P12–P14 | c001 L177–223 + viewport-helper L182–196 (`list_first_visible_index` L182–189, `list_visible_window` L190–196), c002 L22–34,138–156,194–196 |
| REQ-PR-007 pagination/lazy-load (real GraphQL endCursor) | P06–P08, P12–P14 | c001 L108–118,224–229, c002 L35–58,102–107, c004 L127–155 |
| REQ-PR-008 filter & search (interactive #38/#40) | P03–P05, P09–P11, P12–P14 | c001 L249–291, c002 L59–73, c003 L134–146 |
| REQ-PR-009 PR detail (reviews+checks+branch+url) | P06–P08, P12–P14 | c001 L169–176,230–235, c002 L74–101,157–193 |
| REQ-PR-010 comments (composer focus/scroll #56) | P03–P11, P12–P14 | c001 L292–330, c002 L108–114, c003 L72–91 |
| REQ-PR-011 send-to-agent | P09–P14 | c002 L123–136 (`build_pr_send_payload` → `PrSendPayload`), c003 L120–126,147–187 |
| REQ-PR-012 `o` open-in-browser (real feature) + `external_url` display-only; in-app merge/approve/review-submit deferred to browser | P03–P14 | key routing c003 L68-69,88-89 (`o`→PrOpenInBrowser / NoSelectionToOpen) + dispatch c003 L190-228 (`dispatch_pr_open_in_browser`, `pr_open_in_browser_info`); reducer c001 L349-357,362-365 (`apply_pr_open_in_browser`/`_failed`, pure; notice applied BEFORE the async spawn); gh boundary c002 L115-122 (`open_pull_request_in_browser` → `gh pr view <n> --repo <o>/<n> --web`) + external_url field c002 L74-101; message bus c004 L32-34,63-65,82-83,113-115 (OpenInBrowser/OpenedInBrowser/OpenInBrowserFailed + dispatch arm; `apply_and_persist` BEFORE spawn); UI shows display-only external_url; spec REQ-PR-012 |
| REQ-PR-013 auth & error handling (no silent drops, slug validation) | P03–P05 (read-only notice reducer + ShowNotice round-trip), P06–P08, P09–P11, P12–P14 | c002 L09–11,20–21,157–165,174–193, c001 L344–348, c003 L83–89, c004 L27,62,81 |
| REQ-PR-014 empty states | P03–P14 | c001 L209–223,386–389 |
| REQ-PR-NFR-001 non-blocking I/O | P06–P16 | c002 (sync boundary), c004 L113–175 |
| REQ-PR-NFR-002 reliability (staleness) | P03–P11 | c001 L88–98,209–247 |
| REQ-PR-NFR-003 maintainability (no overrides) | all | project-standards thresholds |

## Codebase Integration Points

### Confirmed-exists files (current `src/`)

| File | Construct | Current line |
|------|-----------|--------------|
| `src/state/types.rs` | `enum ScreenMode` | L227 |
| `src/state/types.rs` | `struct AppState` (`issues_state`) | L247 (L277) |
| `src/state/types.rs` | `enum IssueFocus` / `DetailSubfocus` | L285 / L296 |
| `src/state/mod.rs` | `fn apply_message` | L342 |
| `src/state/mod.rs` | `fn select_repository_by_index` (resets issues) | L488 (L497) |
| `src/input.rs` | `enum InputMode` / `fn input_mode_for_state` / `route_search_key` | L9 / L45 / L89 |
| `src/messages.rs` | `enum MessageDomain` / `IssuesMessage` / `AppMessage` | L18 / L113 / L283 |
| `src/messages/issues_conversion.rs` | `IssuesMessage::from_app_event` | L17 (impl L11) |
| `src/app_input/mod.rs` | `fn dispatch_app_message` / `apply_and_persist` | L420 / L216 |
| `src/app_input/normal.rs` | `handle_normal_key_event` / `resolve_mode_key` / `handle_dashboard_issues_key` | L85 / L296 / L156 |
| `src/app_input/gh_async.rs` | `spawn_gh_task_with_panic` | L11 |
| `src/github/mod.rs` | `GhError` / `GhClient` / `list_comments` / `create_comment` | L31 / L95 / L211 / L315 |
| `src/layout.rs` | `ISSUES_SIDEBAR_WIDTH`, `issues_pane_rows`, `issues_detail_viewport_rows`; ADD shared selection-follow helpers `list_first_visible_index`/`list_visible_window` (finding #2) + `prs_pane_rows` | L206/L234/L264 |
| `src/ui/orchestration.rs` | `build_screen_element` | L75 |
| `src/ui/screens/issues.rs` | `IssuesScreen` (`IssuesScreenProps` L20) | L33 |

### New files to create

| File | Purpose |
|------|---------|
| `src/state/prs_ops.rs` | PR reducer (enter/exit/focus/nav/subfocus/scroll/filter/search) |
| `src/state/prs_load_ops.rs` | list/detail/page/comment loaded reducers (staleness guards) |
| `src/state/prs_inline_ops.rs` | inline composer reducers |
| `src/state/prs_mutation_ops.rs` | comment-create lifecycle reducers |
| `src/github/parse_pr.rs` | PR JSON parse + arg builders + sort |
| `src/messages/prs_conversion.rs` | `AppEvent`↔`PullRequestsMessage` conversion |
| `src/app_input/prs.rs` | PR key routing (8-level precedence) |
| `src/app_input/prs_dispatch.rs` | detail/comment loaders + prompt + agent launch |
| `src/app_input/prs_list_dispatch.rs` | list reload/fetch dispatch |
| `src/app_input/prs_filter.rs` | filter control key handling |
| `src/app_input/prs_mutation.rs` | inline submit → comment-create dispatch |
| `src/ui/components/pr_list.rs` | scroll-aware PR list (consumes the `crate::layout` selection-follow helpers + `prs_pane_rows`) |
| `src/ui/components/pr_detail.rs` | unified detail view (reuses `ScrollableText` for the body/comments TEXT region) |
| `src/ui/components/pr_filter_controls.rs` | interactive PR filter controls |
| `src/ui/screens/pull_requests.rs` | `PullRequestsScreen` |
| `src/state/prs_tests*.rs`, etc. | PR behavioral test modules |

## Integration Contract

### Existing callers
- `handle_normal_key_event` (normal.rs L85) gains a `DashboardPullRequests` branch and a `p`/`P`
  entry arm in `resolve_mode_key` (L296).
- `dispatch_app_message` (mod.rs L420) gains parallel `AppMessage::PullRequests` arms.
- `AppState::apply_message` (mod.rs L342) gains a `PullRequests` arm → `apply_prs_message`.
- `select_repository_by_index` (mod.rs L488) gains `if self.prs_state.active { reset_prs_... }`.
- `input_mode_for_state` (input.rs L45) gains a `DashboardPullRequests` block.
- `build_screen_element` gains a `DashboardPullRequests` → `PullRequestsScreen` arm.

### Key dispatch integration map (per-symbol)

```text
fn resolve_mode_key(key, screen_mode) -> KeyHandling          // normal.rs L296 (verified)
  role: emit EnterPrsMode on 'p'/'P' when Dashboard
  verify: rg "EnterPrsMode" src/app_input/normal.rs

fn handle_dashboard_prs_key(snapshot, key) -> KeyHandling      // normal.rs (new)
  role: delegate to prs::handle_prs_mode_key when DashboardPullRequests
  verify: rg "handle_prs_mode_key" src/app_input/normal.rs

fn dispatch_app_message(app_state, ctx, message)              // mod.rs L420 (verified)
  role: route AppMessage::PullRequests to dispatch helpers or apply_and_persist
  verify: rg "AppMessage::PullRequests" src/app_input/mod.rs

fn apply_message(self, message) -> Self                       // state/mod.rs L342
  role: route PullRequests domain to apply_prs_message + debug_assert(handled)
  verify: rg "apply_prs_message" src/state/mod.rs

fn select_repository_by_index(self, idx) -> Self              // state/mod.rs L488
  role: reset_prs_for_repo_change when prs active
  verify: rg "reset_prs_for_repo_change" src/state/mod.rs
```

### Full dispatch chain
See `analysis/pseudocode/component-004.md` "Full Dispatch Chain (PR Mode)".

### Existing code replaced/removed (and existing behaviors extended)
No code is removed and no logic is forked — no `*_v2`/`*_new`/`*_old` duplicates. However, "additive"
does NOT mean "untouched": the integration necessarily EXTENDS several existing `match`/dispatch
sites by adding new arms and changes a few previously-unhandled key behaviors. The exact existing
behaviors that are extended/changed (all by ADDING arms/branches, never deleting an existing one):

- **`AppState::apply_message`** (`src/state/mod.rs` L342): a new `AppMessage::PullRequests(_)` arm is
  added that routes to `apply_prs_message`; all existing arms are preserved unchanged.
- **`AppState::select_repository_by_index`** (`src/state/mod.rs` L488): a new
  `if self.prs_state.active { self.reset_prs_for_repo_change(); }` branch is added next to the
  existing `issues_state` branch (L496-498); existing behavior is preserved.
- **`dispatch_app_message`** (`src/app_input/mod.rs` L420): new `AppMessage::PullRequests(_)` dispatch
  arms are added alongside the existing domain arms.
- **`resolve_mode_key`** (`src/app_input/normal.rs` L296): the previously-unhandled dashboard `p`/`P`
  key is now handled — it emits `EnterPrsMode` when `screen_mode == Dashboard`. The `i`/`s` arms are
  unchanged. (Before this change `p` fell through to `None`.)
- **`handle_normal_key_event`** (`src/app_input/normal.rs` L85): a new `handle_dashboard_prs_key`
  intercept is inserted into the resolver chain BEFORE `resolve_mode_key`, mirroring the existing
  `handle_dashboard_issues_key` intercept.
- **`input_mode_for_state`** (`src/input.rs` L45): a new `DashboardPullRequests` block is added.
- **`build_screen_element`** (`src/ui/orchestration.rs`): a new `ScreenMode::DashboardPullRequests`
  arm is added that renders `PullRequestsScreen`.
- **Key suppression in PR mode:** while in `DashboardPullRequests`, dashboard/split/destructive
  bindings (`a` focus-agents, lowercase `s` split, split-mode `Esc`, `Ctrl-d`/`Ctrl-k`/`l`) are
  CONSUMED as no-ops by the PR handler so they never reach the dashboard handlers. This changes the
  effective behavior of those keys ONLY while PR mode is active; their dashboard behavior is
  untouched. `S` is repurposed to send-to-agent within PR detail (the dashboard split-on-`S` binding
  is suppressed in PR mode).

All of the above are additive at the enum/`match` level (new variants + new arms); no existing
variant, arm, or function body is deleted.

### User access path
Dashboard → press `p` → PR Mode opens scoped to the selected repository → repo sidebar / PR list /
PR detail; `Tab` cycles focus; `Esc` unwinds and exits.

### Data/state migration
`prs_state` is runtime-only and never written to the persisted DTO (`to_persisted_state` omits it,
as it does `issues_state`). No migration. Backward-compat asserted by a round-trip test on the
persisted-state DTO plus a load test of a pre-PR-mode persisted file.

### Backward-compat acceptance gate
Loading a pre-PR-mode persisted state file yields an `AppState` with `prs_state` at default
(inactive) and all prior fields intact.

### End-to-end verification
P16 e2e gate proves: entry reachable from real key flow; loading cannot stick; detail completion
path; scope switch invalidates + reloads; mockup placement; missing/invalid `github_repo` surfaces
inline config message; no clippy allows; no threshold overrides; full `make ci-check` green.

## Regression-Guard Cross-Reference (past Issues-Mode follow-on bugs)

| Past issue | Guard requirement | Phase + verification |
|-----------|-------------------|----------------------|
| #56 composer focus/scroll | REQ-PR-010 | P03–P05 (c001 L292–330), P10/P13 tests, P14A placement |
| #38/#40 filter not interactive | REQ-PR-008 | P09–P11 (c003 L134–146), P10 tests |
| #55 list clip/selection-follow | REQ-PR-006 | P03–P05 (NEW shared list-viewport helper in `src/layout.rs`: STUB P03 → RED P04 → IMPL P05) + P14 (UI consumption in `pr_list.rs`), P13 tests, P14A check 2 |
| #54 rows dropped | REQ-PR-006 | P05/P13 tests (N loaded → N rows), P14A |
| #47 repo nav vs pane_focus | REQ-PR-003 | P05 (c001 L125–153), P05A/P11A behavioral check |
| #37/#39 sync I/O, silent None, dup const, scroll heuristic, viewport size | REQ-PR-NFR-001, REQ-PR-009 | P06–P08 (async), P14 (layout prop), P08A/P14A contradiction scan |
| #68/#69/#70/#74/#25/#28/#32 no clippy allows / no overrides | REQ-PR-NFR-003 | every phase verification + P16 gate (`check-clippy-allows.sh`) |

Past-issue shorthand (one line each; each maps to the named regression-guard requirement text):
- **#56** — comment action `c` opened the composer but did not move detail subfocus to the composer,
  and the composer could render off-screen below the viewport → REQ-PR-010 (consistent composer
  focus + auto-scroll-into-view).
- **#38 / #40** — issue filter controls were not actually interactive (fields/Space/Apply/Clear did
  not work) → REQ-PR-008 (fully interactive filter controls).
- **#55** — the list clipped rows and the selection could move off-screen with no selection-following
  → REQ-PR-006 (scroll-aware list, selected row always kept visible).
- **#54** — loaded rows were silently dropped from rendering (N loaded rendered fewer than N) →
  REQ-PR-006 (N loaded → exactly N rendered when they fit).
- **#47** — repository Up/Down navigation depended on `pane_focus` and stopped reloading when focus
  was elsewhere → REQ-PR-003 (RepoList focus drives rescope/reload independent of `pane_focus`).
- **#37 / #39** — synchronous gh I/O on the UI thread, silent `None` match arms dropping
  unavailable-context cases, duplicated layout constants, heuristic scroll-length estimation, and
  scroll math reading `crossterm::size()` directly → REQ-PR-NFR-001 (non-blocking I/O) +
  REQ-PR-009/013 (viewport prop + real rendered length + no silent drops).
- **#68 / #69 / #70 / #74 / #25 / #28 / #32** — prior PRs that smuggled in `#[allow(clippy::…)]`
  attributes or raised `clippy.toml` thresholds to silence complexity lints instead of decomposing
  functions → REQ-PR-NFR-003 (zero allows, zero threshold overrides; split handlers instead).

## Verification Command Baseline (every phase)

EVERY worker and verifier phase — INCLUDING stub and TDD phases — runs the COMPLETE baseline below.
There are no partial baselines.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# or: make ci-check
```

### Coverage gate (`make ci-check` / `--fail-under-lines 30`)

The five-command list above is the PER-PHASE gate. `make ci-check` is the SUPERSET that ADDITIONALLY
runs the coverage gate (`cargo llvm-cov ... --fail-under-lines 30`) plus build/test under `--locked`.
The coverage gate is ENFORCED (and asserted explicitly) at the integration and end-to-end phases —
`P15` (integration hardening) and `P16`/`P16A` (E2E quality gate) — and again before opening the PR.
Worker/verifier phases SHOULD run `make ci-check` when llvm-cov is available; if it is absent locally
(see P00A / Phase 0.5 preflight check #1), the five-command baseline is the binding per-phase gate and the coverage
`--fail-under-lines 30` gate is satisfied at P15/P16 (where it is a hard FAIL if coverage < 30%).

### RED exception (TDD phases P04/P07/P10/P13 and their verifiers only)

In a TDD(RED) phase, exactly ONE command — `cargo test` — is EXPECTED to fail, because the newly
added behavioral tests assert behavior with no implementation yet. The RED tests must still
COMPILE, so `cargo build` MUST pass. `cargo fmt --all --check`, `cargo clippy ... -D warnings`, and
`bash scripts/check-clippy-allows.sh` MUST ALL pass even in the RED phase. No other command may
fail. In every non-TDD phase (stub, impl, integration, e2e, and all verifiers of GREEN phases),
ALL five commands MUST pass with no exception.

### No-override / no-threshold-raise (every phase)

`scripts/check-clippy-allows.sh` is part of the baseline above and is authoritative: it fails on ANY
first-party clippy allow/expect attribute in EVERY spelling (`#[allow(clippy::…)]`,
`#![allow(clippy::…)]`, `#[cfg_attr(…, allow(clippy::…))]`, and the `expect` forms) AND asserts that
the two clippy configs — `./clippy.toml` and `./.github/clippy/clippy.toml` — keep identical
thresholds (cognitive 15, fn-lines 60, args 6, type-complexity 250, struct-bools 3). Every phase
therefore re-asserts: no allows, no threshold raise, configs in sync; handlers are split to fit the
thresholds rather than overridden. P16/P16A additionally assert the EXACT threshold values
literally (see those phases).

**Cargo.toml `[lints.clippy]` no-weaken gate (every phase — finding #2).** Lint levels can ALSO be
weakened OUTSIDE `src/` attributes and the `clippy.toml` configs — via `Cargo.toml`'s top-level
`[lints.clippy]` table, which `scripts/check-clippy-allows.sh` does NOT inspect. The current
`Cargo.toml` already contains a `[lints.clippy]` table (`all = deny`, `pedantic/nursery = warn`,
`todo/unimplemented = deny`, plus SIX pre-existing `= "allow"` relaxations:
`needless_pass_by_value`, `redundant_clone`, `doc_markdown`, `missing_const_for_fn`,
`missing_errors_doc`, `option_if_let_else`). EVERY phase therefore additionally re-asserts that THIS
branch neither ADDS a new `allow` entry NOR DOWNGRADES an existing deny/warn to allow under the
`[lints]`/`[lints.clippy]` table. Removing/tightening an existing allow is permitted. The gate is a
HARD inverted check on `git diff main -- Cargo.toml` ADDED lines:
```bash
added_lints_allows="$(git diff main -- Cargo.toml \
  | grep -E '^\+' | grep -Ev '^\+\+\+' \
  | grep -E '=[[:space:]]*"allow"|level[[:space:]]*=[[:space:]]*"allow"')"
if [ -n "$added_lints_allows" ]; then
  echo "FAIL: this branch adds/weakens a Cargo.toml [lints.clippy] allow entry (finding #2)"
  printf '%s
' "$added_lints_allows"; exit 1
fi
```

## Analysis Artifacts Required

- `analysis/domain-model.md`
- `analysis/pseudocode/component-001.md` (state + reducer)
- `analysis/pseudocode/component-002.md` (gh client boundary)
- `analysis/pseudocode/component-003.md` (key routing + inline + chooser)
- `analysis/pseudocode/component-004.md` (message bus + dispatch routing)

## Execution Tracker

### Tracker / completion-marker consistency gate (machine-checkable — finding #4)

The coordinator MUST run this consistency check whenever a phase is marked complete (and at the
final audit). It enforces TWO properties: (1) EVERY phase in the canonical phase list has a tracker
ROW in the table below, and (2) every phase marked complete has a matching `.completed/PNN.md` /
`.completed/PNNA.md` marker (and vice-versa — no orphan marker without a row). The canonical phase
list is: `P00A` (standalone preflight gate) plus the 16 worker phases `P01..P16` and their 16 paired
verifiers `P01A..P16A`. A missing tracker row, a missing marker for a row marked complete, or an
orphan marker is a HARD FAIL.
```bash
set -euo pipefail
OVERVIEW=project-plans/issue20/plan/00-overview.md
COMPLETED=project-plans/issue20/.completed
fail=0

# Canonical phase list: P00A + P01..P16 + P01A..P16A.
PHASES=(P00A)
for n in $(seq -w 1 16); do PHASES+=("P$n" "P${n}A"); done

# (1) Every canonical phase has a tracker ROW in 00-overview.md.
for p in "${PHASES[@]}"; do
  # Match a table row that names the phase as a word (P05 must not match P05A, and vice-versa).
  if ! grep -Eq "^\|[[:space:]]*${p}([[:space:]/]|\b)" "$OVERVIEW"; then
    echo "TRACKER FAIL: no tracker row for $p in $OVERVIEW"; fail=1
  fi
done

# (2a) Every phase whose row is marked complete/verified has its .completed/PNN.md marker.
#      (Heuristic: a row is "complete" when its Status cell is not 'Pending' and not '—'.)
while IFS= read -r row; do
  p="$(printf '%s' "$row" | sed -E 's/^\|[[:space:]]*([P0-9A]+).*/\1/')"
  case " ${PHASES[*]} " in *" $p "*) : ;; *) continue ;; esac
  status="$(printf '%s' "$row" | awk -F'|' '{print $3}' | tr -d ' ')"
  if [ -n "$status" ] && [ "$status" != "Pending" ] && [ "$status" != "—" ]; then
    if [ ! -f "$COMPLETED/$p.md" ]; then
      echo "TRACKER FAIL: $p marked '$status' but $COMPLETED/$p.md missing"; fail=1
    fi
  fi
done < <(grep -E '^\|[[:space:]]*P[0-9A]' "$OVERVIEW")

# (2b) Every .completed/PNN.md marker corresponds to a canonical phase (no orphan markers).
if [ -d "$COMPLETED" ]; then
  for m in "$COMPLETED"/*.md; do
    [ -e "$m" ] || continue
    base="$(basename "$m" .md)"
    case " ${PHASES[*]} " in
      *" $base "*) : ;;
      *) echo "TRACKER FAIL: orphan completion marker $m has no canonical phase"; fail=1 ;;
    esac
  done
fi

if [ "$fail" -ne 0 ]; then echo "FAIL: tracker/marker consistency gate"; exit 1; fi
echo "tracker/marker consistency: OK"
```
This gate is advisory-free (no print-only branch): any inconsistency exits nonzero. P16A's "Phase
Completion Audit" is the final, authoritative run of the marker half of this check.

| Phase | Status | Verified | Semantic Verified | Notes |
|-------|--------|----------|-------------------|-------|
| P00A / Phase 0.5 preflight | Pending | — | — | |
| P01 analysis | Pending | — | — | |
| P01A | Pending | — | — | |
| P02 pseudocode | Pending | — | — | |
| P02A | Pending | — | — | |
| P03 domain+state stub | Pending | — | — | |
| P03A | Pending | — | — | |
| P04 domain+state TDD | Pending | — | — | |
| P04A | Pending | — | — | |
| P05 domain+state impl | Pending | — | — | |
| P05A | Pending | — | — | |
| P06 gh-client stub | Pending | — | — | |
| P06A | Pending | — | — | |
| P07 gh-client TDD | Pending | — | — | |
| P07A | Pending | — | — | |
| P08 gh-client impl | Pending | — | — | |
| P08A | Pending | — | — | |
| P09 msg-bus+key-routing stub | Pending | — | — | |
| P09A | Pending | — | — | |
| P10 msg-bus+key-routing TDD | Pending | — | — | |
| P10A | Pending | — | — | |
| P11 msg-bus+key-routing impl | Pending | — | — | |
| P11A | Pending | — | — | |
| P12 UI stub | Pending | — | — | |
| P12A | Pending | — | — | |
| P13 UI TDD | Pending | — | — | |
| P13A | Pending | — | — | |
| P14 UI+integration impl | Pending | — | — | |
| P14A | Pending | — | — | |
| P15 integration hardening | Pending | — | — | |
| P15A | Pending | — | — | |
| P16 e2e quality gate | Pending | — | — | |
| P16A final verification | Pending | — | — | |
