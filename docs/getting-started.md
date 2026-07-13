# Getting started with LLxprt Jefe

This guide is for the common first-run workflow:

1. Create a repository in Jefe.
2. Create your first agent in that repository.
3. Start working without terminal-tab chaos.

---

## 1) Create a repository

From the dashboard, press `N` (capital N) to open **New Repository**.

![Create repository form](assets/jefe-create-repository.png)

### Repository fields

- **Name**
  - Friendly label shown in Jefe’s repository list.
  - Example: `LLxprt Code`, `payments-service`, `client-foo`.

- **Base Dir**
  - Think of this as a **parent directory** for this repo’s work.
  - A common pattern is: `~/projects/myreponame`
  - New agent work dirs are usually created under this path.
  - If you leave it empty, Jefe falls back to a temp path (`/tmp/<slug>`), but in practice you almost always want a real project path.

- **Default Profile**
  - Optional llxprt profile to prefill for new agents in this repository.
  - Leave blank to use llxprt defaults.

- **Default Version** (LLxprt repositories)
  - Optional npm version or tag to prefill for new LLxprt agents, such as `0.9.0` or `0.10.0-nightly.260712.21cb698b6`.
  - The value is copied when a new agent is created. Changing the repository default does not modify existing agents.
  - Leave blank to launch the directly installed `llxprt` executable.

### Submit / navigation

- `Tab` or `Down`: next field
- `Shift+Tab` or `Up`: previous field
- `Enter`: submit
- `Esc`: cancel

After submit, the repository is added and selected.

---

## 2) Create an agent

With your repository selected, press `n` (lowercase n) to open **New Agent**.

![Create agent form](assets/jefe-create-agent.png)

### Agent fields and what they mean

- **Shortcut (1-9)**
  - Optional quick-jump slot for `Alt+1..9`.
  - `0` clears the shortcut.

- **Name**
  - Agent label in the UI (required to create the agent).

- **Description**
  - Optional context note for you/team (what this agent is for).

- **Work Dir**
  - Filesystem path where llxprt runs.
  - A common pattern is: `~/projects/myreponame/somethingimdoing`
  - That `somethingimdoing` directory can be either:
    - a full checkout, or
    - a git worktree
  - For **new** agents, Jefe auto-generates this from repository base dir + agent name until you edit this field manually.

- **Profile**
  - llxprt profile name (`--profile-load`).
  - Blank means use llxprt default behavior.

- **Version** (LLxprt agents)
  - Optional npm version or tag. It starts with the repository's Default Version and can be overridden per agent.
  - Blank runs the directly installed `llxprt` executable.
  - A nonblank value runs the selected package through `npm exec`, so multiple LLxprt releases can run side by side without separate global installs.
  - `npm` must be available on the local machine for local agents or on the effective remote host for remote agents.
  - This field is hidden for Code Puppy agents.

- **Mode Flags**
  - Extra llxprt CLI flags.
  - The new-agent form pre-fills `--yolo`; clear it to run non-yolo. What you
    save is what is passed.

- **LLXPRT_DEBUG**
  - Optional debug env value for llxprt.
  - Leave blank unless you are debugging llxprt behavior.

- **Pass --continue** (checkbox)
  - When enabled, Jefe launches llxprt with `--continue`.

- **Sandbox** (checkbox)
  - Enables llxprt sandbox mode for this agent.
  - **Strong recommendation:** turn this on whenever your environment supports it.

- **Sandbox Engine**
  - Engine used for sandboxing (cycles with space in the form).
  - Typical options include `podman`, `docker`, and `sandbox-exec` depending on platform.

- **Sandbox Flags**
  - Resource limits/options passed via `SANDBOX_FLAGS`.
  - Jefe defaults to:
    - `--cpus=2 --memory=12288m --pids-limit=256`

### Submit / navigation

- `Tab` or `Down`: next field
- `Shift+Tab` or `Up`: previous field
- `Space`: toggle checkboxes / cycle sandbox engine
- `Enter`: submit
- `Esc`: cancel

After submit, the agent is created and selected.

---

## Sending a GitHub issue to an agent

**Send Issue** writes the selected issue's details to `.jefe/issue-prompt.md` and
launches the agent with Jefe's generic end-to-end delivery contract. The agent
is instructed to create an issue branch, implement and verify the change,
commit and push it, open a linked pull request, watch required workflows to
completion, and loop on failures and actionable review feedback. It must reply
in the relevant review threads and resolve addressed threads where the hosting
platform supports that operation.

This contract is the same for LLxprt, Code Puppy, and future runtimes; only the
runtime-specific command-line transport differs. Repository-local instructions
such as `AGENTS.md`, `.llxprt/LLXPRT.md`, or other agent memories may supplement
project conventions, but Jefe does not require them to provide the delivery
workflow.

If a workflow-watch command or shell invocation times out while checks are
pending, the agent is told to continue polling with a bounded delay. Pending
checks alone are not completion and should not cause the agent to return.

---

## Recommended baseline for most users

- Set a real repository **Base Dir**.
- Use a clear agent **Name** + short **Description**.
- Keep **Sandbox** enabled whenever possible.
- Start with default sandbox flags unless you know you need different limits.

If copy/paste from llxprt ever behaves oddly inside Jefe, check the tmux note in the main README.
