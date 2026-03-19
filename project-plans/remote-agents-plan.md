# Plan: Remote Repository SSH Support for Jefe

Plan ID: `PLAN-20260313-REMOTE-AGENTS-V1`
Generated: 2026-03-13
Status: Updated to match implemented repository-owned design
Scope: Add first-class SSH-backed repository execution to the existing local-agent workflow without breaking local agents or current tmux-backed behavior.

## Problem Statement

Jefe originally assumed every agent runs locally and that `work_dir` exists on the local filesystem. The implemented direction is to let a repository carry optional remote SSH settings while preserving the same operational model for its agents:

- create/edit repositories with remote settings,
- create/edit agents with the same agent form as before,
- keep `work_dir` derivation behavior unchanged,
- spawn/relaunch/kill agents from the same dashboard,
- attach to tmux-backed terminal sessions,
- optionally bootstrap the remote environment when `llxprt` resolution fails.

## Approved UX / Ownership Model

Remote settings belong to `Repository`, never to `Agent`.

### Repository form fields

Repository form labels must remain exactly:

- `Name`
- `Base Dir`
- `Default Profile`
- `Remote Repository`
- `Login User`
- `Host / IP`
- `Run As User`
- `Setup Env Default`

### Agent form rules

The agent form remains visually and semantically unchanged.
There are no remote-specific agent fields.

Agent `work_dir` behavior also remains unchanged:

- default `work_dir = repository.base_dir + "/" + slugified(agent.name)`
- user may still edit `work_dir` manually
- for remote repositories, both `base_dir` and `work_dir` are remote paths and must remain meaningful raw strings on the local host
- remote `work_dir` must not be expanded locally or created with local filesystem APIs

## Remote Execution Model

The remote model separates transport identity from execution identity.

### Transport identity

SSH connects as:

- `login_user`

### Effective execution user

The effective execution user is:

- `run_as_user`, when non-empty
- otherwise `login_user`

This supports common setups such as logging in as `ubuntu` and executing as `acoliver`.

### Required privilege wrapper

When `run_as_user` differs from `login_user`, commands should run as:

- `sudo -n su - <run_as_user> -c '<command>'`

`sudo -n` is required so jefe fails fast instead of hanging on a password prompt.

All remote operations that must reflect the real execution environment should run as the effective execution user, including:

- llxprt resolution,
- remote path creation,
- tmux spawn/attach/kill/liveness commands,
- setup-env installs,
- remote capture-pane reads.

## Runtime Requirements

Remote lifecycle parity must cover:

- spawn
- attach
- kill
- relaunch
- liveness
- capture

### llxprt resolution order

Resolve `llxprt` on the remote host as the effective execution user in this order:

1. `llxprt`
2. `<remote_path>/node_modules/.bin/llxprt`
3. `npx --yes @vybestack/llxprt-code`

### Setup-env behavior

Setup-env is only relevant when all of the following are true:

- the repository is remote-enabled,
- `Setup Env Default` is enabled,
- llxprt resolution fails.

When setup-env runs, prefer a path-local install under the remote path for the effective execution user.

## Persistence / Compatibility

Backward compatibility requirements:

- `Repository.remote` must deserialize with serde defaults
- `LaunchSignature.remote` must deserialize with serde defaults
- existing persisted state files must continue to load cleanly

Remote context can continue to ride on `RuntimeSession.launch_signature.remote` rather than requiring a separate transport field.

## Safety Rules

### Remote work directories

For remote repositories:

- do not locally expand the path,
- do not locally create the path,
- do not locally delete the path when `delete_work_dir` is checked.

If remote deletion is later implemented, it must be done explicitly over SSH. Silent local deletion is forbidden.

### Attach

Remote attach remains in MVP.

The attach command must be shell-escaped carefully and, when an execution user override is present, wrapped with:

- `sudo -n su - ... -c ...`

### Shell safety

All host/user/path/session inputs must be shell-quoted defensively.
No extra shell helper crate is required; project conventions currently favor `std::process::Command` plus local helper functions.

## Implemented Runtime Shape

The current implementation threads repository-owned remote settings through launch signatures and runtime bindings, and adds SSH-backed support for:

- remote tmux session creation,
- remote tmux attach,
- remote tmux kill,
- remote tmux liveness checks,
- remote tmux capture-pane output,
- startup restore using repository remote context,
- remote-safe delete-workdir behavior.

## Verification Expectations

Minimum verification for this feature set:

- `cargo check`
- `cargo test`
- repository remote persistence tests
- agent create/edit semantics for remote repositories
- runtime lifecycle tests remain green for local behavior

## Follow-up Work

Useful future hardening items:

- stronger remote runtime unit coverage around command construction,
- remote-specific integration tests with mocked SSH command execution,
- optional explicit remote delete support,
- richer remote preflight diagnostics,
- doc updates in user-facing help/README if remote workflows become broadly exposed.
