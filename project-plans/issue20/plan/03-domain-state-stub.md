# Phase 03 — Domain & State Stub

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P03
- **Prerequisites:** `.completed/P02A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Introduce the PR-Mode DOMAIN types, STATE aggregate/focus enums/reducer-op stubs, INPUT modes,
`ScreenMode`/`AppEvent` variants, the typed MESSAGE surface (`MessageDomain::PullRequests`,
`PullRequestsMessage`, `AppMessage::PullRequests`, and the `prs_conversion` stub), and the NEW
shared selection-follow viewport helper stubs in `src/layout.rs` (finding #2) as compiling, TOTAL
stubs (signatures + structures). Update ALL exhaustive match arms so the workspace still compiles.
Every stub is total, deterministic, clippy-clean, and panic-free; its RETURNED VALUE is intentionally
WRONG/empty (no `todo!()`/`unimplemented!()` — clippy denies both) so the P04 RED tests fail by
behavioral assertion, not by panic. No real behavior yet.

**Scope boundary (single-owner file creation — finding #2):** P03 creates ONLY domain, state, and
message-conversion files. P03 does NOT create any `src/app_input/prs*.rs` files (those are created
in P09) and does NOT create any `src/ui` PR files (those are created in P12). P03 MODIFIES existing
shared files (`src/ui/orchestration.rs`, `src/state/mod.rs`, `src/messages.rs`, `src/input.rs`,
`src/state/types.rs`, `src/domain/mod.rs`, and `src/layout.rs` — the shared leaf that now hosts the
pure viewport helpers); later phases modify those same shared files further but never re-create
P03's new files.

**TOTAL-STUB rule (NO `todo!()`/`unimplemented!()` ANYWHERE — findings #1 & #4):** The repo's
`Cargo.toml` `[lints.clippy]` table DENIES both `todo` and `unimplemented` (`todo = "deny"` at
`Cargo.toml:63`, `unimplemented = "deny"` at `Cargo.toml:64`). Clippy is a STATIC lint: it fires on
the mere PRESENCE of a `todo!()`/`unimplemented!()` macro in source, regardless of whether the code
path is reachable. Because this phase's gate requires `cargo clippy --workspace --all-targets
--all-features -- -D warnings` to PASS (no RED exception in a stub phase), ANY `todo!()`/
`unimplemented!()` in P03-authored source would FAIL the phase. Therefore P03 introduces ZERO
`todo!()`/`unimplemented!()`. Every stub is TOTAL, deterministic, clippy-clean, and panic-free; its
RETURNED VALUE is intentionally WRONG/empty so the P04 behavioral tests fail by ASSERTION (a
wrong-value mismatch), NOT by panic. Concretely:
- Reducer stubs return safe defaults: mutating helpers (e.g. `reset_prs_for_repo_change`) have
  empty/no-op bodies returning `()`; `apply_prs_message` returns `true` from a TOTAL no-op `match`
  over every `PullRequestsMessage` variant (each arm does nothing and falls through to `true`), so
  the `apply_message` arm needs no `debug_assert!` and cannot debug-panic (finding #4). Because the
  stub mutates NO state, the P04/P11 behavioral reducer tests still fail by ASSERTION (observed state
  unchanged), not by panic. These helpers are reachable from `apply_message` and are panic-free.
  (In P03, `reset_prs_for_repo_change` exists only as a no-op SIGNATURE and is NOT yet called from
  `select_repository_by_index` — that wiring is GREEN/P05; finding #3.)
- The viewport helper stubs (`list_first_visible_index`, `list_visible_window`) return DELIBERATELY
  WRONG-BUT-TOTAL values (see "Files to modify → `src/layout.rs`" below): `list_first_visible_index`
  returns a constant `0` (ignoring selection-follow), and `list_visible_window` returns an EMPTY
  slice/`Vec`. These compile and never panic; the P04 selection-follow/exact-N-rows tests fail
  because the returned value is wrong, not because of a panic.
- The `build_screen_element` `DashboardPullRequests` arm renders a BENIGN placeholder element
  (e.g. `element! { View {} }`), NOT `todo!()` — panic-free whether or not it is ever rendered.
- The `prs_conversion` (`PullRequestsMessage` ↔ `AppEvent`) conversion bodies are TOTAL stubs that
  return a deterministic WRONG value (e.g. for `from_app_event` return `None`/a fixed non-matching
  variant; for `From<PullRequestsMessage> for AppEvent` return a fixed deterministic `AppEvent`
  that will NOT round-trip), so the P04 round-trip tests fail by assertion. They contain NO
  `todo!()`/`unimplemented!()`.

## Requirements Implemented (Expanded)

### REQ-PR-001 mode entry/exit, REQ-PR-003 focus, REQ-PR-006 list, REQ-PR-009 detail, REQ-PR-008 filter, REQ-PR-010 comments, REQ-PR-014 empty
- **Requirement:** The type surface for PR Mode must exist so later phases can implement behavior.
- **Behavior contract:**
  - GIVEN the current source, WHEN P03 lands, THEN `cargo build` succeeds, all new types/variants
    exist, no existing type/variant is renamed or removed, and all exhaustive matches compile.
- **Why it matters:** A compiling type skeleton lets TDD (P04) write RED tests against real
  signatures.

## Implementation Tasks

### Files to modify
- `src/state/types.rs`:
  - `enum ScreenMode` — add `DashboardPullRequests` (do NOT remove/rename existing). Markers:
    `@plan PLAN-20260624-PR-MODE.P03 @requirement REQ-PR-001 @pseudocode component-001 lines 66-76`.
  - `enum InputMode` is in `src/input.rs` — see below.
  - add `enum PrFocus { RepoList, PrList, PrDetail }`.
  - add `enum PrDetailSubfocus { Body, Review(usize), Check(usize), Comment(usize), NewComment }`.
  - add `struct PullRequestsState { ... }` mirroring `IssuesState` field set (active, pull_requests,
    selected_pr_index, pr_detail, committed_filter, draft_filter, search_query, loading,
    list_cursor, has_more, error, pr_focus, detail_subfocus, list_scroll_offset, list_viewport_rows,
    detail_scroll_offset, detail_viewport_rows, inline_state, agent_chooser, filter_ui,
    search_input_focused, prior_agent_focus, draft_notice, mutation_pending, next_mutation_id,
    list_reload_pending, next_list_request_id, list_page_pending, detail_pending,
    next_detail_request_id, comments_page_pending, next_comments_request_id) + pending guard structs
    + loading/filter-ui sub-structs + `impl Default`/`impl PullRequestsState`.
    (`list_scroll_offset`/`list_viewport_rows` back the NEW selection-follow helper now living in
    `src/layout.rs` — Finding-#2 relocation; component-001 lines 177-200, helpers at lines 182-189
    and 190-196.)
  - add `prs_state: PullRequestsState` field to `struct AppState`. NOTE: `AppState` derives only
    `Debug, Default, Clone` (NOT `Serialize`/`Deserialize`), so there is NO `#[serde(skip)]` to add.
    `prs_state` is RUNTIME-ONLY: it is simply omitted from the persisted-state DTO/mapping layer,
    exactly as `issues_state` is. Confirm by inspecting the persisted `struct State`
    (`src/persistence/mod.rs`) and `to_persisted_state` (`src/app_input/mod.rs`): neither references
    `issues_state`, and neither must reference `prs_state`.
  - add PR `AppEvent` variants (additive; see `00-overview.md` enum-evolution list). This INCLUDES
    `PrShowNotice(ReadOnlyHintKind)` — the consumed-no-op + non-blocking-hint event for invalid
    `r`/`c`/`e`/`o` actions (REQ-PR-010/012/013; component-003 lines 83-89). Add the
    CANONICAL `enum ReadOnlyHintKind { ReadOnlyReplyOnComment, ReadOnlyNoComment, ReadOnlyNotEditable,
    NoSelectionToOpen }` (Copy/Clone) — all FOUR variants, matching
    `analysis/domain-model.md` (canonical definition) — in `src/state/types.rs` (or `src/domain/mod.rs`,
    mirroring where issue focus/hint types live).
- `src/input.rs`:
  - `enum InputMode` — add `PrsNormal, PrsInline, PrsSearch, PrsFilter, PrsChooser`.
  - `fn input_mode_for_state` — add ONLY a minimal compile-only `DashboardPullRequests` arm that
    returns a fixed default (`InputMode::PrsNormal`) so the exhaustive `ScreenMode` match compiles
    (finding #3 — P03 is compile-only). The real precedence routing (Inline > Chooser > Search >
    Filter > Normal), mirroring the `DashboardIssues` block, is implemented in the GREEN phase (P11),
    guarded by its P10 RED `input_mode_for_state` tests. P03 does NOT introduce that branching
    behavior; the constant-`PrsNormal` arm is intentionally WRONG so the P10 routing tests fail by
    assertion, not by panic.
- `src/messages.rs`:
  - `enum MessageDomain` — add `PullRequests`.
  - add `enum PullRequestsMessage { ... }` (mirror `IssuesMessage`; `Box` large payloads). MUST
    include `ShowNotice(ReadOnlyHintKind)` (paired with the `PrShowNotice` AppEvent; component-004
    lines 26-27).
  - `enum AppMessage` — add `PullRequests(PullRequestsMessage)`; update `domain()`, `name()`,
    `route()`, and the `message_names!` invocation.
  - add `mod prs_conversion;` to `src/messages.rs` (the messages module is the single file
    `src/messages.rs`, NOT a `src/messages/mod.rs`). Place the `mod prs_conversion;` declaration
    directly next to the existing `mod issues_conversion;` declaration at `src/messages.rs:14`. The
    new submodule file is physically `src/messages/prs_conversion.rs`, mirroring the EXACT existing
    layout of `src/messages/issues_conversion.rs` (Rust resolves a `mod foo;` declared in
    `src/messages.rs` to a sibling file `src/messages/foo.rs`).
- `src/state/mod.rs`:
  - add `mod prs_ops; mod prs_load_ops; mod prs_inline_ops; mod prs_mutation_ops;`
  - `apply_message` — add `AppMessage::PullRequests(message) => { let _handled =
    self.apply_prs_message(message); }` WITHOUT a `debug_assert!(handled)` in P03 (finding #4). The
    stub `apply_prs_message` returns `true` from a TOTAL no-op match (see below), so the arm compiles
    and dispatch is panic-free regardless of reachability. The `debug_assert!(handled)` companion
    assertion is DEFERRED to the GREEN domain-state phase (P05) — which owns `src/state/mod.rs` and
    `apply_prs_message` — added there once `apply_prs_message` actually handles each
    `PullRequestsMessage` variant. Rationale: in a stub phase the no-op reducer cannot be made to
    "really handle" a message, so pairing a `false`/no-op stub with `debug_assert!(handled)` would
    debug-panic if a PR message were ever dispatched in a test/debug build — forbidden by the
    TOTAL/panic-free stub rule. P03 therefore neither asserts nor returns `false`.
  - `select_repository_by_index` (operates on `&mut self`): P03 does NOT wire any repo-scope reset
    behavior here (finding #3 — P03 is compile-only). The behavioral call
    `if self.prs_state.active { self.reset_prs_for_repo_change(); }` (mirroring the existing
    `if self.issues_state.active { self.reset_issues_for_repo_change(); }` at `src/state/mod.rs`
    L496-498) is ADDED in the GREEN phase (P05), guarded by the P04 RED test
    `test_select_repository_resets_pr_scope`. P03 only introduces the empty helper SIGNATURE
    `pub(super) fn reset_prs_for_repo_change(&mut self)` (mirroring the real
    `pub(super) fn reset_issues_for_repo_change(&mut self)` at `src/state/issues_ops.rs` L151) with a
    NO-OP body so the workspace compiles; it is not called from `select_repository_by_index` in P03.
- `src/ui/orchestration.rs`:
  - `build_screen_element` matches `ScreenMode` EXHAUSTIVELY with NO wildcard arm
    (`src/ui/orchestration.rs` L81-107), so the new `ScreenMode::DashboardPullRequests` variant
    REQUIRES a match arm for the workspace to compile. Add a `ScreenMode::DashboardPullRequests`
    arm that renders a BENIGN placeholder element (`element! { View {} }.into_any()` — `iocraft`'s
    `View` and the `element!` macro are already in scope via `iocraft::prelude::*` at L6), NOT
    `todo!()` and NOT the real `PullRequestsScreen` component. This keeps file ownership exclusive:
    P03 only MODIFIES `orchestration.rs` with a placeholder arm; P12 (the sole creator of
    `src/ui/screens/pull_requests.rs`) replaces this placeholder with the real `PullRequestsScreen(...)`
    wiring. The arm is unreachable at startup (`screen_mode` defaults to `Dashboard`) and renders a
    harmless empty view if ever entered (TOTAL-STUB rule above).
- `src/layout.rs` (shared low-level leaf module — finding #2):
  - Add the NEW shared, PURE selection-follow viewport helpers as STUBS here (NOT in any `src/ui`
    file, so the STATE layer can consume them without a state→ui boundary violation). `src/layout.rs`
    is already the documented shared module importable by BOTH state and ui (`src/layout.rs` L184-193:
    "Keeping them here avoids a dependency from the state layer into the UI layer"), and the
    pseudocode explicitly endorses "pure fns in layout.rs" (component-001 line 180).
  - `pub fn list_first_visible_index(selected_index: usize, len: usize, viewport_rows: usize) ->
    usize` — STUB returns a constant `0` (ignores selection-follow). Total, panic-free, clippy-clean.
    Markers: `@plan PLAN-20260624-PR-MODE.P03 @requirement REQ-PR-006 @pseudocode component-001 lines
    182-189`.
  - `pub fn list_visible_window<T>(rows: &[T], selected_index: usize, viewport_rows: usize) -> &[T]`
    — STUB returns an EMPTY slice (`&rows[0..0]`). Total, panic-free, clippy-clean. Markers:
    `@plan PLAN-20260624-PR-MODE.P03 @requirement REQ-PR-006 @pseudocode component-001 lines 190-196`.
  - These WRONG-BUT-TOTAL return values make the P04 pure-logic RED tests fail by ASSERTION
    (selection-follow / exact-N-rows mismatch), NEVER by panic. P05 replaces the stub bodies with the
    real algorithm; P14's `pr_list` consumes the SAME `crate::layout` helpers (no UI-layer copy).

### Files to create (P03-OWNED — domain, state, message-conversion ONLY)
- `src/domain/mod.rs` additions: `PullRequest`, `PullRequestDetail`, `PrReview`, `PrCheck`,
  `PrState`, `PrReviewState`, `PrCheckStatus`, `PrFilter`, `PrFilterState` (with `impl Default` for
  filter state = `Open`). Non-serde transient (mirror `Issue`/`IssueDetail`), reuse `IssueComment`.
- `src/state/prs_ops.rs`, `prs_load_ops.rs`, `prs_inline_ops.rs`, `prs_mutation_ops.rs` — stub
  reducer fns. Match the real Issues-Mode shapes: mutating reducers take `&mut self` and return `()`
  (e.g. `reset_prs_for_repo_change(&mut self)`, mirroring `reset_issues_for_repo_change`), and the
  hub is `fn apply_prs_message(&mut self, message: PullRequestsMessage) -> bool` returning `true`
  from a TOTAL no-op `match` over every `PullRequestsMessage` variant in the stub (finding #4 — so
  the `apply_message` dispatch arm needs no `debug_assert!` and cannot debug-panic; that
  `debug_assert!(handled)` companion on the `apply_message` arm in `src/state/mod.rs` is added in
  GREEN/P05, the domain-state phase that owns `src/state/mod.rs` and `apply_prs_message`). Per the
  TOTAL-STUB rule above, these reducer stub bodies are no-ops/safe
  defaults (NEVER `todo!()`/`unimplemented!()`, which clippy denies) because `apply_message` can
  reach `apply_prs_message`; the no-op mutates no state, so behavioral tests still fail by assertion.
  `reset_prs_for_repo_change` is a no-op SIGNATURE only and is NOT called from
  `select_repository_by_index` in P03 (finding #3 — that wiring is GREEN/P05). Do NOT use a
  `self -> Self` consuming signature.
- `src/messages/prs_conversion.rs` (physical sibling of `src/messages/issues_conversion.rs`) —
  `PullRequestsMessage::from_app_event` + `From<...> for AppEvent`. These conversion bodies are TOTAL
  stubs returning deterministic WRONG values (e.g. `from_app_event` returns `None`/a fixed
  non-matching variant; `From<PullRequestsMessage> for AppEvent` returns a fixed deterministic
  `AppEvent` that will NOT round-trip), so the P04 round-trip RED tests fail by assertion. They
  contain NO `todo!()`/`unimplemented!()` (clippy denies both).

Every new/changed item carries `@plan/@requirement/@pseudocode` markers.

### Files NOT created in P03 (single-owner boundary — finding #2)
- `src/github/parse_pr.rs` and the `GhClient` PR methods are created in **P06** (GitHub client
  stub), NOT here.
- `src/app_input/prs.rs`, `prs_dispatch.rs`, `prs_list_dispatch.rs`, `prs_filter.rs`,
  `prs_mutation.rs` (and their `src/app_input/mod.rs` `mod` registrations) are created in **P09**
  (message-bus & key-routing stub), NOT here.
- `src/ui/screens/pull_requests.rs`, `src/ui/components/pr_list.rs`, `pr_detail.rs`,
  `pr_filter_controls.rs` (and their `screens/mod.rs` / `components/mod.rs` registrations) are
  created in **P12** (UI stub), NOT here. P03's `build_screen_element` arm is a benign placeholder
  (above); P12 replaces it with the real `PullRequestsScreen`. NOTE (finding #2): there is NO
  `src/ui/components/list_viewport.rs`; the shared selection-follow helpers live in `src/layout.rs`
  (stubbed here in P03), consumed by both the state reducers (P05) and the UI `pr_list` (P14).

## Pseudocode Traceability
- c001 L01-09 (dispatch table), L62-92 (enter/exit/reset), state aggregate fields.
- c004 L01-37 (enum additions), L74-82 (reducer arm).

## Verification Commands

Run the COMPLETE baseline (all gates MUST pass — this is a stub/GREEN phase, no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# or: make ci-check
```
All five gates above MUST pass. Stub bodies compile and any provisional tests must already be
green; no command is permitted to fail in this phase.

## Structural Verification Checklist
- [ ] Workspace compiles.
- [ ] All new types/variants present.
- [ ] `ScreenMode`, `InputMode`, `AppEvent`, `IssuesMessage` existing variants unchanged.
- [ ] Every exhaustive match over `ScreenMode`/`AppMessage`/`MessageDomain`/`InputMode` updated.
- [ ] Markers present in every changed file.

## Semantic Verification Checklist (Mandatory)
- [ ] `prs_state` is runtime-only and NOT added to the persisted DTO: `to_persisted_state`
  (`src/app_input/mod.rs`) and `PersistedState`/`State` (`src/persistence/mod.rs`) reference neither
  `issues_state` nor `prs_state` (cite file:line). No `#[serde(...)]` attribute is added to `AppState`
  (it is not `Serialize`/`Deserialize`).
- [ ] NO new backward-compat persisted-state TEST is authored in P03 (finding #3 — P03 is
  compile-only and adds no behavioral tests). The backward-compat assertion (loading a pre-PR-mode
  persisted-state file deserializes successfully and `AppState::default().prs_state` is inactive,
  with all prior persisted fields intact) is authored as a RED test in P04
  (`test_pre_pr_persisted_state_loads_with_inactive_prs_state`) and made GREEN in P05. P03 only
  confirms structurally (above) that `prs_state` is absent from the persisted DTO.
- [ ] Existing key routing untouched (Dashboard/Issues/Split paths unchanged).
- [ ] Domain boundary stub (`src/domain`) does NOT import `crate::ui`/`crate::state`.
- [ ] STATE-layer boundary holds (finding #2): the new viewport helpers live in `src/layout.rs`
  (shared leaf), so `src/state/prs_*.rs` consume `crate::layout::list_first_visible_index` and do
  NOT import `crate::ui`. Confirm `src/state` contains NO `use crate::ui`.
- [ ] NO `todo!()`/`unimplemented!()` appears ANYWHERE in P03-authored source (findings #1 & #4):
  `Cargo.toml` denies both macros (`todo = "deny"` L63, `unimplemented = "deny"` L64) and clippy
  fires on PRESENCE regardless of reachability, so any occurrence would FAIL the clippy gate. The
  reducer stubs (`src/state/prs_*.rs`), the `layout.rs` viewport stubs, the `prs_conversion.rs`
  conversion stubs, and the `build_screen_element` placeholder arm are ALL total, panic-free, and
  return deterministic WRONG/empty values.
- [ ] Default `PrFilterState == Open`.

## Deferred Implementation Detection
```bash
# P03 owns ONLY domain/state/messages/layout files. Record (do NOT fail on) non-macro markers here.
# Record-only: append `|| true` so a no-match (rg exit 1) cannot abort the phase under `set -e`.
rg -n "TODO|FIXME|HACK|placeholder|for now|will be implemented" \
   src/domain src/state/prs_*.rs src/messages.rs src/messages/prs_conversion.rs src/input.rs src/layout.rs || true
# HARD inverted gate (findings #1 & #4): NO todo!()/unimplemented!() may appear in ANY P03-authored
# source — clippy denies both macros, so presence fails the phase. Fail (nonzero) if found anywhere:
if rg -n "todo!\(\)|unimplemented!\(\)" \
   src/state/prs_*.rs src/messages/prs_conversion.rs src/layout.rs src/ui/orchestration.rs ; then
  echo "FAIL: todo!()/unimplemented!() present (clippy denies todo/unimplemented; stubs must be total)"; exit 1
fi
```

## Success Criteria
- Compiles green; all stubs + markers present; no existing variant removed.

## Failure Recovery
Safe, surgical rollback — restore MODIFIED tracked files, and delete ONLY the specific new files
this phase created. Do NOT use `git clean` (it can wipe unrelated untracked files).
```bash
# 1. Revert in-place edits to existing tracked files:
git restore --staged --worktree -- \
   src/state/types.rs src/state/mod.rs src/input.rs \
   src/messages.rs src/domain/mod.rs src/ui/orchestration.rs src/layout.rs
# 2. Remove ONLY the P03-created new files, by exact path:
rm -f src/state/prs_ops.rs src/state/prs_load_ops.rs src/state/prs_inline_ops.rs \
      src/state/prs_mutation_ops.rs src/messages/prs_conversion.rs
```
(Removing the `mod prs_*;` / `mod prs_conversion;` declarations is covered by the `git restore` of
`src/state/mod.rs` and `src/messages.rs` in step 1. Do NOT touch `src/github`, `src/app_input`, or
`src/ui` PR files here — those belong to P06/P09/P12.)

## Phase Completion Marker (`.completed/P03.md`)
Phase ID, timestamp, files changed, build result, confirmation of ZERO `todo!()`/`unimplemented!()`
in P03-authored source, backward-compat result, semantic summary.
