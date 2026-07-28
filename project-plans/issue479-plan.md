# Issue #479 Plan: delete the working copy dialog is no-op

## Problem

When an agent has a dirty working copy and the user sends an issue, the
`ConfirmIssueDirtyCopy` modal appears. The user reports:

1. The confirm action is a no-op (nothing happens).
2. It should **delete the working copy in the local git clone** and then send
   the issue to the agent.

## Root Cause

`confirm_issue_dirty_copy_enter` (in `src/app_input/issues_send.rs`) calls
`prepare_issue_target(..., DirtyPolicy::Discard)`. The Discard policy only
runs `reset --hard` + `clean -fd` — it does NOT delete the working copy.
The user explicitly wants a full delete + re-clone (guaranteed clean state),
matching the existing `ConfirmIssueOriginMismatch` force-reclone path.

The "no-op" perception stems from the default focus being `Cancel`: pressing
Enter (the natural "yes" key) dismisses the modal without action. The default
is already `Cancel` (correct per the issue's "it should be cancel").

## Acceptance Matrix

| # | Actor / Path | Input / Boundary | Success Behavior | Failure Behavior | Test |
|---|---|---|---|---|---|
| AC1 | Dirty-copy confirm, local target, valid clone identity | Dirty working copy, user cycles to Confirm + Enter | Working copy directory is deleted and re-cloned from configured identity; issue is sent to agent (launch + self-assignment) | N/A | `confirm_dirty_copy_enter_force_reclones_local` |
| AC2 | Dirty-copy confirm, no valid clone identity | Agent's repo has no valid `github_repo` | N/A (no deletion) | `SendToAgentFailed` with clear error; working copy NOT deleted | `confirm_dirty_copy_enter_without_identity_fails_without_deleting` |
| AC3 | Dirty-copy confirm, default Cancel focus | User presses Enter without cycling | Modal dismisses; no side effects (no deletion, no launch) | N/A | existing `issue_send_modal_tests` |
| AC4 | Dirty-copy confirm dialog text | Modal rendered | Title/message reflects "delete and re-clone" semantics | N/A | `overlay_content` confirm render test |
| AC5 | Dirty-copy confirm, Esc / `n` | User presses Esc | Modal dismisses; no side effects | N/A | existing tmux scenario |

## Non-Goals

- Changing the default focus (already `Cancel`).
- Changing the origin-mismatch confirm path (already force-reclones).
- Changing the initial `Stop`-policy detection (still opens the modal).
- Merging the dirty-copy and origin-mismatch modal variants (different
  triggers, different text).
- Preserving uncommitted `.jefe/`/`.llxprt/` metadata across the delete
  (a fresh clone restores tracked owned metadata; uncommitted owned metadata
  is lost, consistent with an explicit "delete" choice).
- Remote-target behavior changes (the remote force-reclone path already
  exists and is reused as-is).

## Vertical Slices

### Slice 1: Change confirm action to force-reclone (core fix)

- **Acceptance rows**: AC1, AC2
- **Architecture owner**: `app_input` orchestration layer
- **Allowed files**:
  - `src/app_input/issues_send.rs` (change `confirm_issue_dirty_copy_enter`)
- **RED test**: `confirm_dirty_copy_enter_force_reclones_local` — proves the
  confirm path calls force-reclone (working copy is replaced), not Discard.
  And `confirm_dirty_copy_enter_without_identity_fails_without_deleting`.
- **GREEN**: Replace the `prepare_issue_target(..., Discard)` call with the
  force-reclone sequence (validate identity → `prepare_issue_target_force_reclone`).
- **Non-goals**: dialog text, tmux scenario.
- **Verification**: `cargo test -p jefe --lib dirty_copy_confirm`
- **Stop conditions**: if force-reclone requires touching `issue_prep.rs`
  internals (it should not — the function already exists).

### Slice 2: Update dialog text

- **Acceptance rows**: AC4
- **Architecture owner**: `ui` orchestration layer
- **Allowed files**:
  - `src/ui/orchestration.rs` (update `ConfirmKind::IssueDirtyCopy` text)
  - `src/selection/overlay_content.rs` (update render assertion if needed)
- **RED test**: assertion that the rendered message contains "delete" /
  "re-clone".
- **GREEN**: update the title/message strings.
- **Verification**: `cargo test -p jefe --lib confirm_modal_renders`

### Slice 3: Update tmux scenario

- **Acceptance rows**: AC5 (regression)
- **Allowed files**:
  - `dev-docs/tmux-scenarios/issue-dirty-copy-confirm.json`
- **Change**: update expected dialog text to match new wording.

## Scope Ledger

| Item | Type | Status |
|---|---|---|
| `confirm_issue_dirty_copy_enter` → force-reclone | In-scope | done |
| Dialog text update ("delete and re-clone") | In-scope | done |
| tmux scenario text update | In-scope | done |
| Default focus change | Non-goal | already Cancel |
| Remove dead `DirtyPolicy` enum (consequence of fix) | In-scope-Fix | done |
| Remove dead `issue_cleanup.rs` module (consequence) | In-scope-Fix | done |
| Refactor `RemotePrepRunner::run` (extract `dirty_check_and_checkout`) | In-scope-Fix | done (complexity gate) |

## Review Counters

- OCR (pre-PR): 1 / 2 (1 finding: self-contradictory Dirty error message — fixed)
- OCR (post-PR): 0 / 2

## Verification Evidence

- `cargo xtask quick`: PASS
- `cargo xtask ci`: PASS
  - fmt check: PASS
  - architecture: PASS
  - clippy (`-D warnings`): PASS
  - complexity: PASS
  - coverage: 70.82% (threshold 30%): PASS
  - build: PASS
  - test: 92 groups, 0 failures: PASS
- Behavioral tests:
  - `local_force_reclone_replaces_dirty_working_copy` — proves force-reclone
    replaces a dirty worktree (untracked + stale feature commit removed).
  - `confirm_modal_dirty_copy_has_content` — asserts dialog text mentions
    "delete" and "clone".
  - `confirm_issue_dirty_copy_modal_routes_to_confirm_input_mode` — modal
    routing unchanged (Cancel default).
