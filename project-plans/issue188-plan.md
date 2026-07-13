# Issue #188 — Close issues with a reason (done/not-planned/invalid/duplicate) incl. duplicate-by-number search

## Goal

Extend the existing close-issue flow (#175/#182) so that closing an issue carries
a **state reason** (`COMPLETED` / `NOT_PLANNED` / `REOPENED` is not a close
reason — we use `COMPLETED`, `NOT_PLANNED`, and a `DUPLICATE` + `INVALID` path),
mirroring GitHub's close-reason UX. When the reason is **Duplicate**, the user
can type/search an issue number to mark as duplicate-of, the way GitHub's UI does.

This builds on the single close path already present
(`CloseIssue` → `handle_issue_close` → `GhClient::close_issue`).

## Design overview

### New flow (replaces the bare `CloseIssue` key)

Today: `Shift-C` → `CloseIssue` → reducer sets `close_mutation_pending` →
dispatch calls `gh issue close` → `IssueClosed`.

New: `Shift-C` → `OpenCloseReasonChooser` → reducer opens a **close-reason
chooser overlay** (like the PR merge chooser / agent chooser) listing:
`Completed`, `Not planned`, `Duplicate`, `Invalid`. The user navigates with
Up/Down, presses Enter to choose. For `Duplicate`, a **duplicate-number search
sub-state** is entered (a text input seeded from the repo's issues, filtered as
the user types a number). Confirming dispatches the close **with the reason**
(and the duplicate-of number for `Duplicate`).

### Layer-by-layer changes

#### 1. Domain layer (`src/domain/issues.rs`)

Add a `CloseReason` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Completed,
    NotPlanned,
    Duplicate,
    Invalid,
}
```

With:
- `label() -> &'static str` → `"Completed"`, `"Not planned"`, `"Duplicate"`,
  `"Invalid"` (emoji-free, plain text).
- `graphql_state_reason() -> Option<&'static str>` →
  `"COMPLETED"`, `"NOT_PLANNED"`, `None` (Duplicate uses markIssueAsDuplicate),
  `"NOT_PLANNED"` for Invalid (GitHub has no `INVALID` state reason; the issue
  body is invalid semantics — map Invalid → `NOT_PLANNED` since that's the
  closest GitHub state reason; document this).
- A `CLOSE_REASONS: &[CloseReason]` const slice (like `MERGE_METHODS`) for the
  chooser iteration.

**Tests:** `label` values, `graphql_state_reason` mapping, `CLOSE_REASONS`
contents, `From`/round-trip if needed.

#### 2. GitHub client (`src/github/issue_lifecycle.rs`)

Extend the close path to carry a reason. Two new builders (pure, unit-tested):

- `build_close_issue_with_reason_args(owner, repo, number, reason,
  duplicate_of: Option<u64>) -> Vec<String>`:
  - For `Completed`/`NotPlanned`/`Invalid`: use
    `gh issue close NUM --repo OWNER/REPO --reason REASON` (the `gh` CLI
    supports `--reason completed|not planned|reopened`). For `Invalid` map to
    `not planned` (no `invalid` reason in gh CLI).
  - For `Duplicate`: `gh issue close NUM --repo OWNER/REPO --reason "not
    planned"` AND additionally mark as duplicate via GraphQL
    `markIssueAsDuplicate` (or the REST duplicate endpoint). This requires the
    issue's **node id**, so the close-with-duplicate path needs the node id
    (already resolvable via `focused_issue_node_id`).

  Actually the cleanest approach that matches the issue notes: use GraphQL
  `updateIssueV2` with `state: CLOSED` and `stateReason`. But `gh issue close
  --reason` is simpler and already supported by the CLI. We'll use:
  - `gh issue close` with `--reason` for non-duplicate.
  - For duplicate: `gh issue close --reason "not planned"` + GraphQL
    `markIssueAsDuplicate` mutation (requires the canonical issue node id and
    the duplicate-of issue's node id).

  To keep the duplicate path robust and avoid a second node-id lookup for the
  duplicate-of issue, we use the REST `POST /repos/{owner}/{repo}/issues/{number}`
  with `state_reason` where supported, plus `markIssueAsDuplicate` GraphQL.

  **Decision (simplest, matches `gh` CLI capabilities):**
  - Non-duplicate: `gh issue close NUM --repo OWNER/REPO --reason <reason>`.
  - Duplicate: `gh issue close NUM --repo OWNER/REPO --reason "not planned"`
    then `gh api graphql -f query=...markIssueAsDuplicate... -F
    canonicalId=<duplicate-of-node-id> -F duplicateId=<this-issue-node-id>`.

  For the duplicate-of node id, we need to resolve the typed issue number to a
  node id. Add `build_issue_node_id_args(owner, repo, number)` →
  `gh api graphql ... issue(number:) { id }` and a parser, OR reuse the
  existing search/list machinery. Simplest: a new
  `GhClient::resolve_issue_node_id(owner, repo, number) -> Result<String,
  GhError>` via a small GraphQL query, and a pure
  `parse_issue_node_id_json(&str) -> Result<String, GhError>`.

- `build_mark_duplicate_args(canonical_node_id, duplicate_node_id) ->
  Vec<String>` — the GraphQL `markIssueAsDuplicate` mutation args.
- `build_issue_node_id_args(owner, repo, number) -> Vec<String>` — GraphQL
  query args.
- `parse_issue_node_id_json(&str) -> Result<String, GhError>`.

Add `GhClient` methods:
- `close_issue_with_reason(owner, repo, number, reason, duplicate_of:
  Option<u64>, this_node_id: Option<&str>) -> Result<(), GhError>`
- `resolve_issue_node_id(owner, repo, number) -> Result<String, GhError>`

Keep the existing `close_issue`/`build_close_issue_args` intact (back-compat;
other callers / tests reference them).

**Tests (pure):** each builder constructs the exact expected args; the parser
extracts the node id from a sample JSON; reason→`--reason` mapping per variant.

#### 3. State types (`src/state/issues_types.rs`)

Add the close-reason chooser overlay state:

```rust
/// Close-reason chooser overlay state (issue #188). Mirrors the merge chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueCloseReasonChooserState {
    pub issue_number: u64,
    pub selected_index: usize,
    /// When the chosen reason is Duplicate, the user types a number here.
    pub duplicate_search: Option<IssueDuplicateSearchState>,
    /// Two-step confirm like delete-confirm (avoids accidental close).
    pub awaiting_confirmation: bool,
}

/// Duplicate-by-number search sub-state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IssueDuplicateSearchState {
    pub query: String,
    /// Issues seeded from the repo's loaded issue list (number + title).
    pub candidates: Vec<(u64, String)>,
    pub selected_index: usize,
}
```

Add field to `IssuesState`:
```rust
pub close_reason_chooser: Option<IssueCloseReasonChooserState>,
```

#### 4. State events (`src/state/events.rs`)

Replace the single `CloseIssue` key-layer event's role (keep `CloseIssue` for
back-compat as the "quick close with default Completed reason" path? No — the
issue says the reason choice should be offered). **Decision:** `Shift-C` now
opens the reason chooser instead of closing immediately. Keep `CloseIssue` as
an internal event used by the chooser's "quick confirm" but make the key open
the chooser.

New events:
- `OpenCloseReasonChooser` — open the chooser overlay.
- `CloseReasonNavigateUp` / `CloseReasonNavigateDown` — move selection.
- `CloseReasonSelect` — confirm the selected reason (Enter). If Duplicate,
  enters duplicate-search sub-state. Otherwise arms confirmation.
- `CloseReasonDuplicateSearchChar(char)` / `CloseReasonDuplicateSearchBackspace`
  / `CloseReasonDuplicateSearchNavigateUp` /
  `CloseReasonDuplicateSearchNavigateDown` — type/select the duplicate number.
- `CloseReasonConfirm` — second Enter: dispatches the actual close with reason.
- `CloseReasonCancel` — Esc: close the chooser without closing the issue.

The reducer (`issues_close_delete_ops.rs` or a new `issues_close_reason_ops.rs`)
owns these transitions deterministically. The `close_mutation_pending` record is
extended to carry the reason + duplicate_of:

```rust
pub struct IssueLifecycleMutationPending {
    // ...existing...
    pub close_reason: Option<CloseReason>,        // None = legacy plain close
    pub duplicate_of: Option<u64>,
}
```

#### 5. App input / dispatch (`src/app_input/`)

- `issues.rs` key handler: when `close_reason_chooser.is_some()`, route keys to
  `resolve_close_reason_chooser_key_event`. Change `Shift-C` from
  `CloseIssue` to `OpenCloseReasonChooser` (in both list + detail focus).
- `issues_dispatch.rs` / `mod.rs`: add the new lifecycle messages to the
  dispatch arms. `CloseReasonConfirm` calls a new
  `issues_lifecycle::handle_issue_close_with_reason` that:
  - reads the pending (reason + duplicate_of + node_id),
  - spawns the gh task calling `close_issue_with_reason`,
  - delivers `IssueClosed` (carrying the reason, so the reducer can update a
    `state_reason` display field) / `MutationFailed`.
- The duplicate-search candidates are seeded from the already-loaded
  `issues_state.issues` (number + title) when Duplicate is selected — no
  network call, consistent with "seeded from the repo's issues".

#### 6. Messages (`src/messages.rs` + `issues_conversion.rs`)

Add the new `IssuesMessage` variants mirroring the new `AppEvent`s, and the
conversion arms. Update `event_conversion.rs` routing.

#### 7. UI (`src/ui/components/`)

New component `close_reason_chooser.rs` (mirrors `merge_chooser.rs`): lists the
four reasons with selection markers, shows the duplicate search input + filtered
candidate list when in duplicate mode, shows the confirm hint. Pure projection
helpers for the candidate filtering (testable without iocraft).

Wire it into `src/ui/screens/issues.rs` as an overlay (like delete_confirm +
agent_chooser).

Update `keybind_bar.rs`: the `C close` hint stays (now opens the reason
chooser). Optionally update hint text to `C close (reason)` — keep concise.

#### 8. Selection / mouse routing

Add `CloseReasonChooser` to `OverlayPane` / `SelectablePane` if the chooser
needs text selection / mouse routing (mirror `MergeChooser`). At minimum,
ensure `active_overlay` returns the chooser when open so mouse clicks don't
fall through. (If the merge chooser is handled, follow its exact pattern.)

### Test plan (TDD, RED first)

**Pure unit (no network, no tmux):**
1. `CloseReason::label` / `graphql_state_reason` / `CLOSE_REASONS`.
2. `build_close_issue_with_reason_args` for each reason (Completed, NotPlanned,
   Invalid → `--reason` value; Duplicate → `--reason "not planned"`).
3. `build_mark_duplicate_args` constructs the GraphQL mutation args.
4. `build_issue_node_id_args` + `parse_issue_node_id_json`.
5. Reducer: `OpenCloseReasonChooser` opens the chooser (when issue focused +
   open), is blocked when another overlay/mutation is active, shows notice when
   no issue focused or already closed.
6. Reducer: navigate up/down bounds within `CLOSE_REASONS`.
7. Reducer: `CloseReasonSelect` for non-duplicate arms confirmation; for
   Duplicate enters duplicate-search sub-state seeded from loaded issues.
8. Reducer: duplicate search char/backspace updates query; candidate filtering
   (pure projection) filters by number-prefix.
9. Reducer: `CloseReasonConfirm` sets `close_mutation_pending` with reason +
   duplicate_of; `CloseReasonCancel` clears the chooser.
10. Reducer: `IssueClosed` with reason updates the issue's state (and a
    `state_reason` field if we add one to the domain `Issue`/`IssueDetail`).
11. Key tests: `Shift-C` resolves to `OpenCloseReasonChooser`; chooser keys
    route correctly; Esc cancels.
12. Message conversion round-trip for every new variant.

**State tests** go in `src/state/issues_tests_close_delete.rs` (or a new
`issues_tests_close_reason.rs`).

### Scope guardrails

- Do NOT touch the PR mode close path (none exists).
- Keep the existing `close_issue`/`build_close_issue_args` (still used /
  referenced) — extend, don't break.
- No `unwrap`/`expect` in production paths. No `unsafe`. No clippy allows.
- Files under 1000 lines (warn 750); functions under 60 lines; cognitive
  complexity under 15.
- Emoji-free UI.
- Reuse `*SilentRefreshed` machinery: after a successful close-with-reason,
  dispatch a silent refresh of the issue list (so the closed issue's
  state_reason reflects server truth) — mirror how the existing close path
  handles post-close (currently it does optimistic update only; the issue
  notes say "reuse the existing *SilentRefreshed machinery to refresh after
  the close"). Add a `RefocusIssueList`-style silent refresh if an issues
  silent-refresh path exists; otherwise follow the existing close path's
  approach (optimistic update) and document.

### Verification

`make ci-check` (fmt, clippy allows, source-file-size, clippy, complexity,
coverage ≥30%, build, test) must pass clean.
