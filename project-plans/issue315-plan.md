# Issue #315: Inline issue prompt into agent launch instruction

## Problem

Issue prompts are written to `.jefe/issue-prompt.md` on the agent's working
copy before launch, then the launch instruction references that file path.
This file gets in the way of git operations (it shows up in `git status`,
clone/cleanup flows must explicitly ignore `.jefe/`, etc.). The issue asks us
to determine whether modern llxprt-code and codepuppy can accept the entire
prompt content inline via the `-i` flag, and if so, eliminate the file write.

## Decision

**Inline the prompt content directly into the agent launch instruction.** The
`-i` flag already passes the instruction string as a positional argument; we
simply pass the full issue/PR prompt markdown content instead of a short
"Read and work on the GitHub issue described in .jefe/issue-prompt.md" string
that references a file. This eliminates the on-disk prompt file write for both
issues and PRs (consistency: both flows use `prepare_fresh_prompt_signature`).

## Acceptance Matrix

| # | Actor / Launch Path | Input | Observable Success | Observable Failure | Test |
|---|---|---|---|---|---|
| A1 | `prepare_fresh_prompt_signature` (Issue, LLxprt) | prompt content string | mode_flags end with `-i` + content+workflow instruction; no file path reference | n/a | `fresh_prompt.rs` unit test |
| A2 | `prepare_fresh_prompt_signature` (Issue, CodePuppy) | prompt content string | mode_flags = [content+workflow instruction]; no file path | n/a | `fresh_prompt.rs` unit test |
| A3 | `prepare_fresh_prompt_signature` (PR, LLxprt) | prompt content string | mode_flags end with `-i` + "Read and work on the GitHub PR described in..." preamble + content | n/a | `fresh_prompt.rs` unit test |
| A4 | `prepare_fresh_prompt_signature` (PR, CodePuppy) | prompt content string | mode_flags = [PR instruction with content] | n/a | `fresh_prompt.rs` unit test |
| A5 | `prepare_issue_target` (Local, Stop) | no prompt param needed | returns Ready without writing `.jefe/issue-prompt.md` | n/a | `issue_prep_tests.rs` |
| A6 | `prepare_issue_target` (Local, Discard) | no prompt param needed | resets workdir, returns Ready, no `.jefe/issue-prompt.md` written | n/a | `issue_prep_tests.rs` |
| A7 | `prepare_issue_target_force_reclone` (Local) | no prompt param needed | re-clones, returns Ready, no prompt file written | n/a | `issue_prep_tests.rs` |
| A8 | RemotePrepRunner::run (Remote) | no prompt param needed | runs SSH prep (clone/checkout/dirty-guard) without writing prompt | n/a | `issue_prep_remote_tests.rs` planner |
| A9 | `issues_send::dispatch_agent_chooser_confirm` | issue payload | launch_sig.mode_flags contains inline prompt content, not file path | n/a | `issue_send_modal_tests.rs` |
| A10 | PR send path | PR payload | launch_sig.mode_flags contains inline PR prompt content | n/a | `app_input_tests.rs` |

## Non-Goals

- Changing the `.jefe/` ignore logic in `git_info/dirty_status.rs` — `.jefe/`
  may still contain other runtime files and must remain ignored.
- Changing how `write_prompt_to_target` / `write_pr_prompt_to_target` work —
  these may become dead code after the refactor and will be removed if so,
  but the safe-path-validation logic is preserved if still referenced.
- Changing the PR prompt *formatting* (`format_pr_prompt`) or issue prompt
  formatting (`format_issue_prompt`) — only the *delivery mechanism* changes.
- Changing the `.jefe/pr-prompt.md` constant for PRs that still use
  `write_pr_prompt_to_target` separately.

## Vertical Slices

### Slice 1: Change `prepare_fresh_prompt_signature` to accept prompt content
- **Files**: `fresh_prompt.rs` (impl + tests)
- **Change**: Third parameter changes from `prompt_relative_path: &str` to
  `prompt_content: &str`. The instruction becomes:
  - Issue: `"Read and work on the following GitHub issue.\n\n{content}\n\n{ISSUE_DELIVERY_WORKFLOW}"`
  - PR: `"Read and work on the following GitHub PR.\n\n{content}"`
- **RED**: New tests asserting content appears in instruction, file path does not.
- **GREEN**: Update `fresh_prompt_instruction` to inline content.

### Slice 2: Remove prompt file write from issue prep
- **Files**: `issue_prep.rs` (impl + tests)
- **Change**: Remove `prompt: &str` parameter from `prepare_issue_target`,
  `prepare_local`, `run_local_policy_and_prep`, `prepare_issue_target_force_reclone`,
  `force_reclone_local_with_url`. Remove `write_prompt_local`. Remove the
  prompt write step from prep sequences.
- **RED**: Tests asserting `.jefe/issue-prompt.md` is NOT written after prep.
- **GREEN**: Remove the write calls.

### Slice 3: Remove prompt file write from remote prep
- **Files**: `issue_prep_remote.rs` (impl + tests)
- **Change**: Remove `prompt: &str` from `RemotePrepRunner::run`,
  `run_force_reclone`. Remove the prompt-write step (step 5) from remote
  sequences.
- **RED**: Planner tests asserting no `cat >` / `.jefe/issue-prompt.md` in
  planned ops.
- **GREEN**: Remove the prompt steps from the runner and planner.

### Slice 4: Update all callers
- **Files**: `issues_send.rs`, `transient_issue_send.rs`,
  `transient_queue_ops.rs`, `prs_orchestration.rs`, `transient_pr_send.rs`
- **Change**: Pass prompt content to `prepare_fresh_prompt_signature` instead
  of file path. Remove prompt content from `prepare_issue_target` calls (the
  prompt is now inlined into the launch signature, not written to disk).
- **Tests**: Update `issue_send_modal_tests.rs`, `app_input_tests.rs`,
  `runtime/commands_tests.rs`.

## Scope Ledger

| Item | Status |
|---|---|
| `fresh_prompt.rs` | Planned (impl + tests) |
| `issue_prep.rs` | Planned (impl + tests) |
| `issue_prep_remote.rs` | Planned (impl + tests) |
| `issue_prep_tests.rs` | Planned (tests) |
| `issue_prep_remote_tests.rs` | Planned (tests) |
| `issues_send.rs` | Planned (caller update) |
| `transient_issue_send.rs` | Planned (caller update) |
| `transient_pr_send.rs` | Planned (caller update) |
| `transient_queue_ops.rs` | Planned (caller update) |
| `prs_orchestration.rs` | Planned (caller update) |
| `issue_send_modal_tests.rs` | Planned (test update) |
| `app_input_tests.rs` | Planned (test update) |
| `remote_probe_tests.rs` | May need update |
| `prs_integration_tests.rs` | May need update |
| `runtime/commands_tests.rs` | May need update |
| `issue_prep_predicate_tests.rs` | May need update |

## Review Counters

- OCR pre-PR: 0 / 2
- OCR post-PR: 0 / 2
