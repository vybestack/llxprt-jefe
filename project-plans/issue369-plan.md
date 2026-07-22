# Issue #369: v0.0.30: GitHub repository field cannot receive focus or input

## Root Cause

The repository form's focus-navigation chain (`RepositoryFormFocus::next`/`prev`
in `src/state/form_types.rs`) diverges from the visual rendering order
(`src/ui/screens/new_repository.rs` and `src/selection/form_content.rs`).

When issue #213 (transient agent support) was merged, the fields
`TransientAgentDir` and `TransientMaxConcurrent` were inserted into the focus
chain **between `DefaultLlxprtVersion` and `GitHubRepo`**. However, the UI
renders them at the **bottom of the form** (after `SetupEnvDefault`), far below
the visually-adjacent `GitHubRepo` field.

Result: when a user Tabs forward from `DefaultLlxprtVersion`, focus jumps to
`TransientAgentDir` (rendered at the bottom), then `TransientMaxConcurrent`,
then `GitHubRepo` (rendered near the middle). The caret disappears from the
visible area between `DefaultLlxprtVersion` and `GitHubRepo`, making it appear
that `GitHubRepo` is unreachable. The user perceives the GitHub Repo field as
unfocusable.

### Visual render order (LLxprt default)
1. Name, BaseDir, DefaultProfile
2. DefaultAgentKind
3. DefaultLlxprtMode, DefaultLlxprtVersion
4. **GitHubRepo** ← visually here
5. IssuePrRepo
6. RemoteEnabled, LoginUser, Host, SshPort, IdentityFile, SshOptions, RunAsUser
7. SetupEnvDefault
8. TransientAgentDir, TransientMaxConcurrent ← rendered here (bottom)

### Focus next() order (broken)
... DefaultLlxprtVersion → **TransientAgentDir** → TransientMaxConcurrent → GitHubRepo ...

### Expected focus next() order (match visual)
... DefaultLlxprtVersion → **GitHubRepo** → IssuePrRepo → RemoteEnabled → ... → SetupEnvDefault → TransientAgentDir → TransientMaxConcurrent → Name

## Acceptance Matrix

| # | Actor/Path | Input | Success Behavior | Failure Behavior | Test |
|---|-----------|-------|-----------------|-----------------|------|
| A1 | Tab from DefaultLlxprtVersion | FormNextField | Focus → GitHubRepo (visually adjacent) | — | unit |
| A2 | Shift+Tab from GitHubRepo | FormPrevField | Focus → DefaultLlxprtVersion | — | unit |
| A3 | Tab from GitHubRepo | FormNextField | Focus → IssuePrRepo | — | unit |
| A4 | Shift+Tab from IssuePrRepo | FormPrevField | Focus → GitHubRepo | — | unit |
| A5 | GitHubRepo focused, type "owner/repo" | FormChar | fields.github_repo = "owner/repo" | — | unit |
| A6 | Tab from SetupEnvDefault | FormNextField | Focus → TransientAgentDir (bottom) | — | unit |
| A7 | Tab from TransientMaxConcurrent | FormNextField | Focus → Name (wrap) | — | unit |
| A8 | Shift+Tab from Name | FormPrevField | Focus → TransientMaxConcurrent (wrap) | — | unit |
| A9 | CodePuppy default kind: Tab from DefaultProfile | FormNextField | Focus → DefaultCodePuppyModel | — | unit |
| A10 | TUI scenario: navigate to GitHubRepo, type repo | Tab keys + chars | "GitHub Repo" shows typed value with caret | — | tmux scenario |
| A11 | Existing repository nav/submission still works | — | unchanged behavior | regression | existing tests |

## Non-Goals

- Redesigning the repository form layout.
- Changing which fields are visible per agent kind.
- Adding mouse-click focus support for form fields.
- Changing the EditRepository form (it shares the same code paths and is fixed by the same change).

## Vertical Slices

### Slice 1: Fix focus chain order (RED → GREEN)
- **Files**: `src/state/form_types.rs`
- **Change**: Reorder `RepositoryFormFocus::next()` and `prev()` so the focus
  chain matches the visual render order. Specifically, move
  `TransientAgentDir`/`TransientMaxConcurrent` to the end of the chain (after
  `SetupEnvDefault`), and connect `DefaultLlxprtVersion`/`DefaultCodePuppyVersion`
  directly to `GitHubRepo`.
- **Tests**: `src/state/form_ops_tests.rs`, `src/state/form_projection.rs` (inline tests)

### Slice 2: Add/fix unit tests asserting the corrected order
- **Files**: `src/state/form_ops_tests.rs`, `tests/ui/forms_and_modals.rs`
- **Change**: Update the existing test (`repository_checkbox_toggle_updates_remote_fields`)
  that encodes the old (broken) order, and add new tests for A1–A9.

### Slice 3: Add TUI scenario
- **Files**: `dev-docs/tmux-scenarios/repo-github-field-focus.json`
- **Change**: Scenario that opens New Repository, Tabs to GitHubRepo, types a repo value, and verifies the caret appears.

## Scope Ledger

| Date | Item | Type |
|------|------|------|
| 2026-07-22 | Initial plan | — |

## Review Counters
- Local OCR: 0/2
- PR OCR: 0/2

## Verification
- `make quick-check` during iteration
- `make ci-check` before push
