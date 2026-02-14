# Jefe — Design Session Notes

_Session started: 2/10/2026_

---

## Overview

**Jefe** ("the boss") — a project/task/agent orchestrator for managing multiple llxprt-code instances across projects and tasks. Currently the workflow involves ~10 terminal sessions with multiple tabs running llxprt-code agents manually. Jefe replaces that chaos with structured project/task/agent management backed by tmux, with a path toward richer agent communication.

---

## Problem Statement

- Running ~10 terminal sessions with multiple tabs of llxprt-code for various tasks
- Most common workflow: "fix issue #1234" with a LLXPRT.md guiding the agent's workflow
- 3-4 active projects at any time, each with multiple concurrent tasks
- No unified way to see status, organize, or manage all of this

## Core Concepts / Data Model

### Hierarchy: Projects → Tasks → Agents

- **Project**: a named workspace (e.g., "llxprt-code", "starflight-tls", "client-work-gable", "client-work-mariadb")
  - Has a base working directory
  - May have project-level config/defaults
- **Task**: a unit of work under a project (e.g., "fix issue #1234", "implement feature X")
  - Has a prompt (the `-i "..."` text)
  - **Gets its own working directory** (required — multiple concurrent tasks on the same project need separate dirs, e.g. separate git worktrees)
- **Agent**: an llxprt-code instance executing a task
  - Launched with specific configuration (--profile-load, --key, --keyfile, --provider, --model, --set, etc.)
  - Runs in --yolo mode (usually)
  - Lives in a tmux session/window
  - Clicking a running task brings the agent to the foreground in a fully functional terminal

## Agent Launch Details

- Agents are llxprt-code instances
- Launched with configurable flags:
  - `--profile-load <profile>`
  - `--key <key>` / `--keyfile <file>`
  - `--provider <provider>` / `--model <model>`
  - `--set <settings>`
  - `-i "the prompt"`
  - `--yolo` (usually)
- Each agent runs in its own tmux pane/window
- Working directory is task-specific (separate from project base dir)

## UI Options

Two separate axes: GUI vs TUI, and what framework within each.

### GUI Option: Native Rust with GPUI (Zed's UI framework)
- GPUI is a **GUI** framework (GPU-rendered, native windows), NOT a TUI
- Pros: Native performance, GPU-accelerated, could embed terminal views natively (like Zed does), Rust
- Cons: Tightly coupled to Zed's internals, barely documented as standalone, steep learning curve, heavy dependency
- Would give a desktop-app feel — think "Zed but for managing agents"

### TUI Option A: iocraft (Rust)
- React-like declarative API for Rust terminal UIs
- `element!` macro, `#[component]` macro, hooks (`use_state`, `use_future`), context, props by reference
- Flexbox layouts via `taffy`
- Fullscreen apps, event handling, key/mouse events
- Inspired by Dioxus and Ink (the JS terminal React renderer)
- Clean architecture: components are functions with hooks, not big trait impls
- Built-in: `View`, `Text`, `TextInput`, tables, forms, progress bars
- **Architecturally much cleaner than ratatui** — declarative vs imperative, component model vs manual draw loop
- 1.1k stars, 226 commits, actively maintained, 100% documented
- Async-native via futures/smol
- Deps: crossterm, taffy, futures, generational-box

### TUI Option B: ratatui
- More established (larger community), imperative API
- You manage the draw loop, state, and layout manually
- Has component/flux/elm architecture patterns but they're conventions, not built-in
- More boilerplate, less declarative

### TUI Option C: OpenTUI/Solid + Bun
- Bun avoids compile-to-JS pain, Solid reactivity model
- Still TypeScript-adjacent — user tires of TypeScript

**Leaning toward**: iocraft for TUI. Cleaner component architecture, React-like mental model without JS, Rust single binary. GPUI remains an option for a future GUI version.

### Terminal Embedding — Key Insight

**llxprt-code already embeds a PTY inside its own TUI.** The interactive shell feature (`shouldUseNodePtyShell`) does exactly this:

1. Uses `@lydell/node-pty` (or `node-pty`) to spawn shell commands in a PTY
2. Uses `@xterm/headless` to parse ANSI output into structured `AnsiOutput` (array of `AnsiLine` = array of `AnsiToken` with text, colors, bold, etc.)
3. The React-based TUI (`shellCommandProcessor.ts`) renders this parsed output in a scrollable view
4. Supports writing to the PTY (`writeToPty`), resizing (`resizePty`), scrolling (`scrollPty`), focus toggling (Ctrl+F)
5. Tracks active PTY IDs, manages lifecycle, handles abort/kill

**This means embedding llxprt-code's TUI inside Jefe's TUI is not just possible — the pattern already exists.** The question is whether we:

- **Option A**: Jefe spawns llxprt-code in a PTY and renders its output (same as how llxprt renders shell commands). Jefe would be a "terminal emulator for llxprt instances." Uses `node-pty` + `xterm-headless` (or Rust equivalents like `portable-pty` + `alacritty_terminal`/`vte`).
- **Option B**: Jefe uses tmux as the backend but renders the tmux pane content in its own TUI. Same idea, tmux as the PTY manager.
- **Option C**: Jefe IS the multiplexer — like a purpose-built Zellij/tmux but specifically for llxprt agents.

For a **Rust TUI (iocraft)** approach:
- `portable-pty` crate for PTY management
- `alacritty_terminal` or `vte` crate for terminal state parsing (equivalent of xterm-headless)
- iocraft component renders the parsed terminal state into a sub-view
- Input forwarding when focused on a specific agent pane

For a **GUI (GPUI)** approach:
- GPUI already has terminal embedding (Zed has a built-in terminal)
- Could potentially reuse Zed's terminal component
- More straightforward but heavier dependency

**Either way, tmux is NOT required.** Jefe can be the multiplexer itself.

## Phased Approach

### Phase 1: "Glorified Launcher" with tmux
- Organize projects/tasks/agents in a UI or CLI
- Launch llxprt-code instances in tmux sessions
- Click/select a running task → attach to that tmux pane (fully functional terminal)
- Track what's running, basic status (alive/dead)

### Phase 2: Rich Agent Communication
- Without tmux being attached in foreground, surface:
  - **Status**: running / completed / errored / waiting for input (red/green/yellow)
  - **Last message**: most recent agent output
  - **Current todo task**: what the agent thinks it's working on
  - **Todo list**: full task list from the agent
- This requires a communication channel between jefe and the llxprt-code instances

## Communication Channel Analysis

### ACP (Agent Communication Protocol) — Deep Dive

ACP is a JSON-RPC 2.0 protocol over stdin/stdout streams. Currently used for Zed↔llxprt-code integration.

**What ACP already provides (from schema.ts):**

| Capability | Schema Type | Direction | Notes |
|---|---|---|---|
| Initialize/handshake | `initialize` | client→agent | Protocol version, auth methods, capabilities |
| Authentication | `authenticate` | client→agent | Profile-based auth |
| Session management | `session/new`, `session/load` | client→agent | Create/resume sessions with cwd + MCP servers |
| Send prompts | `session/prompt` | client→agent | Content blocks: text, image, audio, resource |
| Cancel | `session/cancel` | client→agent | Abort in-progress work |
| Agent text streaming | `agent_message_chunk` | agent→client | Real-time text output |
| Agent thinking | `agent_thought_chunk` | agent→client | Thinking/reasoning output |
| Tool calls | `tool_call` / `tool_call_update` | agent→client | Status: pending/in_progress/completed/failed + diff content |
| **Plan/Todo updates** | `plan` | agent→client | **Already streams todo list with status (pending/in_progress/completed)!** |
| Permission requests | `request_permission` | agent→client | Tool approval flow |
| File system ops | `fs/read_text_file`, `fs/write_text_file` | agent→client | Delegated file I/O |

**What ACP gives us for Jefe's Phase 2 needs:**

- [OK] **Todo list + current task**: `plan` session update already streams `PlanEntry[]` with `{content, status}` — this is exactly the todo list!
- [OK] **Last message**: `agent_message_chunk` streams text in real-time
- [OK] **Error detection**: Tool calls report `failed` status, prompt responses report `stopReason` (end_turn, max_tokens, refusal, cancelled)
- [OK] **Running/idle detection**: Can infer from whether we're between prompt/response cycles
- WARNING: **Waiting for input**: `request_permission` tells us the agent is blocked waiting for tool approval, but there's no general "waiting for user input" signal
- [ERROR] **No "agent status" query**: ACP is push-based (notifications), no way to poll current state
- [ERROR] **Transport is stdin/stdout**: This is the big constraint — currently hardwired to stdio streams

**The Zed Compatibility Problem:**

ACP currently communicates over stdin/stdout. Zed launches llxprt-code with `--experimental-acp` and pipes stdin/stdout directly. For Jefe:

1. **We can't use stdin/stdout** because the agent also needs a terminal (tmux) for interactive use
2. **Options to solve this:**
   - **a) Unix domain socket / named pipe**: Add a second transport for ACP. Agent opens a socket in addition to its terminal. Jefe connects to the socket. Zed keeps using stdio. No Zed compat issue.
   - **b) Extended ACP mode**: `--experimental-acp --acp-transport=socket --acp-socket=/tmp/jefe-agent-{id}.sock` — new flag, stdio mode remains default for Zed
   - **c) tmux control mode + sideband file**: Agent writes status to a file/fifo, jefe reads it. Simpler but less structured.
   - **d) Embed ACP as a library**: Jefe imports the ACP client code and connects over socket. Reuse all the schemas.

**Recommendation**: Option (b) — extend ACP with a socket transport. This:
- Preserves 100% Zed compatibility (stdio remains default)
- Gives Jefe a structured, typed communication channel
- Reuses all existing ACP schemas (plan updates, tool calls, message streaming)
- Allows the agent to remain fully interactive in tmux (no stdout hijacking)
- Is a small, clean change to llxprt-code (add socket listener alongside the stdio one)

### A2A (Agent-to-Agent Protocol)
- Google's protocol for agent-to-agent discovery and communication
- More peer-oriented, built for agents discovering/talking to each other
- Heavier than needed for Jefe's hub-and-spoke model
- Could be relevant later if agents need to coordinate with each other (not just with Jefe)
- **Verdict**: Overkill for now, keep an eye on it

### Plugin Architecture (VSCode)
- Built for IDE integration, similar to ACP but different shape
- Not directly applicable to Jefe's use case
- **Verdict**: Not the right fit

### Antigravity (from gemini-cli)
- Being cherry-picked, still emerging
- Need to evaluate what it adds
- **Verdict**: TBD, monitor

---

## Proposed Architecture (Phase 1 + 2 path)

```
┌─────────────────────────────────────┐
│            JEFE UI                  │
│  (GPUI native / Rust TUI / TBD)    │
│                                     │
│  ┌─────────┐ ┌─────────┐ ┌──────┐  │
│  │Project A│ │Project B│ │Proj C│  │
│  │ Task 1 ●│ │ Task 1 ●│ │T1  ● │  │
│  │ Task 2 ●│ │ Task 2 ○│ │T2  ● │  │
│  │ Task 3 ○│ └─────────┘ └──────┘  │
│  └─────────┘                        │
│  ● = running  ○ = idle/done         │
│  [click task → fullscreen terminal] │
└──────────┬──────────────────────────┘
           │
     ┌─────┴──────┐
     │ tmux server │ (manages all terminal sessions)
     └─────┬──────┘
           │
    ┌──────┼──────────────┐
    │      │              │
    ▼      ▼              ▼
┌──────┐┌──────┐     ┌──────┐
│Agent ││Agent │ ... │Agent │  (llxprt-code instances)
│ tty  ││ tty  │     │ tty  │
│ +    ││ +    │     │ +    │
│ ACP  ││ ACP  │     │ ACP  │  (unix socket sideband)
│socket││socket│     │socket│
└──────┘└──────┘     └──────┘
```

**Phase 1**: Jefe just manages tmux sessions. Status = "is the process alive?"
**Phase 2**: Add ACP socket transport to llxprt-code. Jefe connects as ACP client to each agent's socket. Gets plan updates, message chunks, tool status, permission requests — all without touching the terminal.

---

## Open Questions

1. **UI framework final decision**: GPUI vs pure Rust TUI vs OpenTUI/Solid+Bun?
2. **Task working directory strategy**: Git worktrees? Cloned copies? Symlinked? User-managed?
3. **Configuration format**: YAML/TOML files per project? Single config DB?
4. **Task lifecycle**: What happens when an agent finishes? Auto-cleanup? Report? Stay around for review?
5. **Can a task have multiple agents** (e.g., parallel approaches to the same issue)?
6. **ACP socket path convention**: Where do socket files live? `/tmp/jefe/agent-{uuid}.sock`? Inside project dir?
7. **Should Jefe be able to send prompts to running agents?** (ACP supports `session/prompt` — Jefe could inject follow-up work)
8. **Permission handling in yolo mode**: If running with `--yolo`, permission requests shouldn't happen. But if they do (non-yolo), should Jefe surface them in the UI? Could be powerful.

---

## Raw Discussion Notes

- User currently has ~10 terminal sessions open with multiple tabs
- Common pattern: "fix issue #1234" with LLXPRT.md workflow guidance
- 3-4 active projects: llxprt-code, starflight-tls, client-work-gable, client-work-mariadb
- Wants project → task → agent hierarchy
- Tasks MUST get their own working directories (multiple issues on same project = separate dirs)
- Agents = llxprt-code instances with specific configs, launched in tmux
- Phase 1: tmux launcher, Phase 2: rich status without attaching
- ACP already has most of what we need for Phase 2 (plan/todo, message streaming, tool status)
- Key constraint: ACP uses stdio, but agents need terminals → need socket transport extension
- Zed compatibility must be preserved — socket transport is additive, not a replacement
- UI: leaning Rust. GPUI (Zed's framework) or pure Rust TUI. User tires of TypeScript. Bun+OpenTUI/Solid also on the table.
- Zed's UI framework is called **GPUI**


---

## Revised Data Model (2/12/2026)

After further discussion, the original three-level hierarchy (Project → Task → Agent)
was simplified. The "Task" layer added unnecessary complexity — in practice, each
llxprt-code instance _is_ the unit of work. The concept of "Project" was also renamed
to "Repository" to better match the mental model (a codebase you work on).

### New Hierarchy: Repositories → Agents

- **Repository** (was "Project"): A codebase with a name, slug, and base directory.
  Contains zero or more running agents.
- **Agent** (was "Task" + "Agent" flattened): A running llxprt-code instance working
  on a specific purpose within a repository.
  - `id` / `display_id`: unique identifier (e.g., "#1872")
  - `purpose`: what the agent is doing (was `task.name`, e.g., "Fix ACP socket timeout")
  - `work_dir`: worktree-style path (e.g., `~/projects/llxprt-code/branch-1/llxprt-code`)
  - `model`, `profile`, `mode`: agent launch configuration
  - `status`: Running / Completed / Errored / Waiting / Paused / Queued
  - `started_at`, `elapsed_secs`, `token_in`, `token_out`, `cost_usd`: metrics
  - `todos`: agent's plan/task list (from ACP in Phase 2)
  - `recent_output`: last N lines of agent output

### Plans / Specs (Deferred)

A "Plan" concept (structured specifications guiding agent work) was discussed but
explicitly deferred. Key design decisions for when it's revisited:

- Plans should NOT live inside the project repository (branch contamination risk)
- Plans will live in Jefe's own data directory (`~/.jefe/plans/`)
- Symlinked into worktrees as `.jefe/plan/` so agents can read them
- A plan may reference a GitHub issue, contain acceptance criteria, implementation
  notes, and test expectations
- Multiple agents may share a plan (parallel approaches to the same problem)
