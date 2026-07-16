# Issue #316 plan: selectable dirty-copy overwrite recovery

## Goal

When issue handoff cannot move an agent working copy to the repository default
branch because local tracked changes would be overwritten, treat that condition
like every other dirty working copy: show the safe-by-default selectable confirm
dialog, allow cancellation without mutation, and after explicit confirmation
clobber the blocking tracked state, remove non-owned untracked state, and reset
the worktree to the fetched default branch before launch.

The existing issue #228 control remains the shared interaction: Cancel is focused
by default, Left/Right/Tab changes focus, Enter activates the focused button,
and Escape or `n` cancels. Issue #316 closes the preparation gap where
jefe-owned tracked paths are intentionally omitted from ordinary dirty status but
can still make Git reject the default-branch checkout before that dialog appears.

## Acceptance matrix

| Row | Actor / launch path | Input and boundary | Observable success | Failure / side effects | Evidence |
| --- | --- | --- | --- | --- | --- |
| A1 | User sends an issue to an existing local agent | Non-owned tracked or untracked changes | Dirty Working Copy opens with Cancel focused and selectable Cancel/Confirm buttons | No cleanup, checkout, prompt write, or launch before confirmation | Existing modal/reducer tests plus updated TUI scenario |
| A2 | User cancels the dirty-copy dialog | Enter on default Cancel, Escape, or `n` | Dialog closes and handoff stops | Working copy remains byte-for-byte untouched | Existing confirm tests and TUI scenario |
| A3 | User sends an issue from a branch whose modified tracked `.jefe`/`.llxprt` path would be overwritten by default-branch checkout | Ordinary dirty parser ignores the owned path, but Git reports checkout overwrite protection | The checkout-protection result is classified as dirty and the selectable dialog opens instead of surfacing a raw Git error | Fetch/default-branch discovery may run; no destructive mutation or prompt write | New real-Git regression test |
| A4 | User selects Confirm for A1 or A3 | Explicit destructive opt-in | Blocking tracked changes are discarded, non-owned untracked paths are removed, owned untracked metadata is retained, and worktree HEAD/index are reset to fetched default branch before prompt write/launch | Cleanup or Git failures are surfaced and stop launch | New real-Git discard regression tests; existing preparation tests |
| A5 | User confirms from a local or remote target | Existing target-aware orchestration | Existing local/remote behavior and origin mismatch handling remain unchanged | No new shell or process abstraction; remote plan retains explicit reset/clean semantics | Existing local/remote preparation suite and full CI |

## Non-goals

- Do not delete the whole working-copy directory or force-reclone a correctly
  configured repository.
- Do not remove untracked `.jefe/` or `.llxprt/` metadata.
- Do not change confirmation behavior for delete, kill, preflight, or origin
  mismatch dialogs.
- Do not change dependencies, persistence schemas, workflow/quality tooling,
  public APIs, or agent configuration outside the target working copy.
- Do not attempt to preserve explicitly confirmed, tracked worktree edits: the
  requested Confirm action is the opt-in clobber operation needed to reach the
  fetched default branch cleanly.

## Vertical slices

### Slice 1: classify checkout overwrite protection as dirty

- Rows: A1-A3.
- Owner / boundary: `app_input::issue_git_prep` Git boundary and
  `app_input::issue_prep` orchestration.
- RED: real-Git test with a modified tracked owned path and divergent target
  branch expects `PrepOutcome::Dirty`, no checkout, and no prompt write.
- GREEN: preserve typed preparation semantics while recognizing Git's
  locale-stabilized checkout overwrite diagnostic as a dirty outcome.
- Allowed paths: `src/app_input/issue_git_prep.rs`,
  `src/app_input/issue_prep.rs`, `src/app_input/issue_prep_tests.rs`.

### Slice 2: confirmed clobber reaches fetched default branch

- Rows: A4-A5.
- Owner / boundary: ownership-scoped cleanup followed by existing Git prep.
- RED: real-Git test confirms the Discard policy removes the blocking tracked
  edit, retains untracked owned metadata, checks out/reset to origin default,
  writes the prompt last, and reports Ready.
- GREEN: extend confirmed cleanup only as needed for tracked blocking state;
  retain constrained per-path untracked deletion.
- Allowed paths: `src/app_input/issue_cleanup.rs`,
  `src/app_input/issue_git_prep.rs`, `src/app_input/issue_prep.rs`,
  `src/app_input/issue_prep_tests.rs`.

### Slice 3: UI scenario documents actual affirmative selection

- Rows: A1-A2 and A4.
- Owner / boundary: existing TUI scenario only; no new UI abstraction.
- RED: update the dirty-copy scenario before production edits so it selects
  Confirm and activates it rather than only moving focus back to Cancel.
- GREEN: scenario reaches the post-confirm handoff state with the modal gone.
- Allowed path: `dev-docs/tmux-scenarios/issue-dirty-copy-confirm.json`.

## Expected paths by layer

- Git/process boundary: `src/app_input/issue_git_prep.rs`.
- Cleanup boundary: `src/app_input/issue_cleanup.rs`.
- Issue preparation orchestration: `src/app_input/issue_prep.rs`.
- Behavioral regression tests: `src/app_input/issue_prep_tests.rs`.
- TUI behavior: `dev-docs/tmux-scenarios/issue-dirty-copy-confirm.json`.
- Delivery record: `project-plans/issue316-plan.md`.

## Scope ledger

| Discovery | Disposition | Reason |
| --- | --- | --- |
| Issue #228 already supplies keyboard focus and unified confirm handling | Reuse / no production UI change | Issue #316 is the preparation-path gap, not a second confirm control |
| Owned paths are intentionally ignored for ordinary dirty display | Preserve for initial fast path; classify only checkout-blocking state | Avoid prompting for routine owned metadata unless Git cannot reach the target branch |
| Confirmed cleanup currently preserves tracked owned edits | In-scope change | Those edits can make the requested clobber-and-reset operation impossible |

No unapproved scope additions.

## Review counters

- Local rustreviewer runs: 1
- Local OCR runs before PR: 1 / 2
- OCR runs after PR: 0 / 2
- GitHub review rounds: 0

## Verification evidence

- RED: real Git rejected checkout because local tracked `.llxprt/LLXPRT.md`
  would be overwritten.
- Focused issue tests: both independently named checkout-blocker tests pass.
- Full gate: `make ci-check` passed with matching Rust/Cargo/Clippy 1.97.
- Rustreviewer: test assertions and contracts strengthened; remote behavior and
  deterministic TUI fixture were deferred as scope expansions described below.
- OCR: reviewed all five changed implementation/test/scenario files and
  reported no findings.
- PR CI: pending.

## Deferred findings / follow-ups

- Remote issue preparation has a pre-existing broad checkout fallback that can
  reset the current default branch after an unclassified checkout failure. The
  issue #316 acceptance matrix explicitly preserves remote behavior; replacing
  that shell orchestration with typed remote checkout outcomes is a separate
  safety change requiring remote fixtures and broader cross-platform coverage.
- The dirty-copy TUI scenario still relies on configured issue/agent state. A
  deterministic scenario runner would require new fixture scripts and GitHub CLI
  shims beyond this issue's bounded local checkout-classification change.
- The initial Stop attempt fetches before checkout classification. Cancellation
  preserves the branch, HEAD, index, status, and working-copy bytes; fetch
  metadata is intentionally permitted by A3 so the eventual confirmation resets
  to the current remote default.
