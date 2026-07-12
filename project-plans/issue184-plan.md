# Issue #184: Add code_puppy as a first-class agent

## Overview

Add `code_puppy` (`mpfaffenberger/code_puppy`) as an alternative agent runtime
peer to LLxprt, governed per-repository via an `AgentKind` enum.

## Design

### AgentKind enum (domain layer)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    #[default]
    Llxprt,
    CodePuppy,
}
```

Methods: `binary_name()`, `label()`, `is_code_puppy()`.

### Fields added

- `Agent.agent_kind: AgentKind` — `#[serde(default)]`
- `LaunchSignature.agent_kind: AgentKind` — `#[serde(default)]`
- `Repository.default_agent_kind: AgentKind` — `#[serde(default)]`

### Detection module (`src/detection/mod.rs`)

PATH probe for `llxprt` and `code_puppy`, cached on `AppState.agent_availability`.

### Launch dispatch (runtime layer)

`launch_args()` branches on kind:
- Llxprt: existing behavior (profile-load, mode_flags, continue, sandbox)
- CodePuppy: `-i` interactive + mode_flags only (no profile/sandbox/continue)

`local_pane_command_args()` uses `kind.binary_name()`.

### Issue send flow

`prepare_issue_launch_signature()` branches on kind:
- Llxprt: push `-i "instruction"` to mode_flags (existing)
- CodePuppy: push instruction text to mode_flags (positional arg)

### StatusBar "(Kennel mode)"

When the selected agent's kind is CodePuppy, append " (Kennel mode)" to the
title in the status bar.

### Forms

- `AgentFormFields.agent_kind: AgentKind` + `AgentFormFocus::AgentKind`
- `RepositoryFormFields.default_agent_kind: AgentKind` + `RepositoryFormFocus::DefaultAgentKind`
- New agent modal defaults `agent_kind` from repo's `default_agent_kind`
- Form cycle (Space) cycles through *available* kinds only

## code_puppy CLI

- Binary: `code_puppy`
- Interactive: `code_puppy -i` (or no args)
- Single prompt: `code_puppy -p "prompt"` or positional
- No `--profile-load`, `--sandbox`, `--continue`, `--yolo`

## Files to modify

~30 files. See TDD approach: domain tests first, then runtime dispatch tests,
then form/state tests.
