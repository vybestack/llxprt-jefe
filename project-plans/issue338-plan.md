# Issue #338: Sending issues still fails on dirty workspaces

## Problem

When sending an issue to an agent (`S` → `AgentChooserConfirm`), the workspace
prep logic has two gaps:

1. **No warning when not on the default branch.** If the agent's work_dir is
   clean but checked out on a non-default branch, jefe silently switches to
   the default branch and launches — no warning, no confirmation. The user is
   surprised when their feature-branch worktree is moved to `main` under them.

2. **Dirty cleanup reliability.** The dirty-confirm modal exists, but the
   issue reports cleanup still doesn't take effect even after confirming. The
   local discard path is surgical and well-tested, but there's no integration
   test for the combined "dirty AND not-on-default-branch" scenario.

## Root cause

`run_local_policy_and_prep` / `RemotePrepRunner::run` only check `is_workdir_dirty`
before deciding whether to return the `Dirty` (needs-confirm) outcome. The
"current branch ≠ default branch" condition is never evaluated as a trigger
for confirmation — the switch happens unconditionally inside
`prepare_issue_workdir` / the checkout script.

## Desired behavior (acceptance matrix)

| # | Condition | Stop (initial send) | Discard (after confirm) |
|---|-----------|--------------------|--------------------------|
| 1 | On default branch, clean | Ready → launch | n/a |
| 2 | On default branch, dirty | Dirty → modal | clean + launch |
| 3 | Not on default branch, clean | Dirty → modal (warn) | switch to default + pull + launch |
| 4 | Not on default branch, dirty | Dirty → modal (warn) | clean + switch to default + pull + launch |

- `.jefe/` and `.llxprt/` paths are never reported as dirty and never removed.
- Default branch is resolved dynamically (`origin/HEAD`), never hardcoded.
- The issue-driven launch never passes `--continue`.
- The confirm modal default is no/halt (Cancel).

## Implementation plan

### Step 1 — `issue_git_prep.rs`: branch-detection helper

Add `is_on_default_branch(work_dir) -> Result<bool, String>` that compares
`current_branch_name` with `resolve_default_branch`.

### Step 2 — `issue_prep.rs`: local prep checks branch + dirty

`run_local_policy_and_prep` returns `Dirty` when EITHER dirty OR not on the
default branch (Stop policy). Discard policy: discard changes if dirty, then
checkout+pull the default branch (existing logic already handles this).

### Step 3 — `issue_prep_remote.rs`: remote prep checks branch + dirty

Mirror the local change: after the dirty check, the checkout script already
switches to the default branch. Add the branch check as a Stop trigger so the
remote path also warns when not on the default branch.

### Step 4 — Modal message

Update the `ConfirmIssueDirtyCopy` modal text to cover the not-on-main case.

### Step 5 — Tests (RED first)

- Local: not-on-default-branch + clean → Stop returns Dirty
- Local: not-on-default-branch + dirty → Discard → Ready, on default branch
- Local: on-default-branch + dirty → Stop returns Dirty (regression)
- Local: on-default-branch + clean → Ready (regression)
- Remote planner: not-on-default-branch → Stop emits no destructive op

## Non-goals

- Changing the `--continue` override (already correct).
- Changing origin-mismatch handling.
- Changing the dirty-detection porcelain logic.
- Changing transient send paths.
