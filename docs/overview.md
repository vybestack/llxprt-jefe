# Jefe — Functional Specification

## What Jefe Is

Jefe is a terminal-native orchestrator for managing multiple AI coding agent instances across repositories. It replaces the manual workflow of maintaining numerous terminal sessions, tabs, and windows running separate `llxprt` (or equivalent CLI agent) processes. Jefe provides a single unified TUI to launch, monitor, organize, and interact with every agent across every repository the user works on.

The name means "the boss." Jefe is the boss of the agents.

---

## Core Concepts

### Repository

A **repository** represents a codebase the user works on. Each repository has:

- A display name (e.g., "llxprt-code", "starflight-tls").
- A URL-safe slug derived from the name.
- A base directory path on disk (e.g., `/Users/alice/projects/llxprt-code`).
- A default profile for new agents launched against this repository (may be empty, meaning "use agent CLI defaults").
- Zero or more agents.

Repositories are persistent. They survive application restarts.

### Agent

An **agent** is a single running (or previously-run) instance of an AI coding CLI (e.g., `llxprt`) working in a directory under a repository. Each agent has:

- A unique ID and a human-readable display ID (e.g., `#42`).
- A short name describing its task (e.g., "Fix ACP socket timeout").
- A longer description of the work being done.
- A working directory, typically a subdirectory or worktree under the repository's base directory.
- A profile name (inherited from the repository default, overridable per agent).
- A mode string controlling agent behavior flags (e.g., `--yolo`, `--yolo --continue`).
- A PTY session slot linking it to a live terminal backend.
- A lifecycle status (see Agent Lifecycle below).
- Runtime telemetry: elapsed time, token counts (input/output), estimated cost.
- A todo list reflecting the agent's own task breakdown.
- Recent output lines including tool call activity.

Agents are the primary unit of work in Jefe. There is no intermediate "task" entity between repositories and agents. The agent name and description carry the semantic role that a task layer would otherwise provide.

### Agent Lifecycle

Every agent is in exactly one of these states:

| Status        | Meaning                                                              |
|---------------|----------------------------------------------------------------------|
| **Running**   | Agent process is alive and actively working.                         |
| **Completed** | Agent finished its work successfully.                                |
| **Errored**   | Agent encountered an error or its process exited with failure.       |
| **Waiting**   | Agent is blocked on user input, permission, or an external resource. |
| **Paused**    | Agent was explicitly paused by the user.                             |
| **Queued**    | Agent is waiting for a concurrency slot to open.                     |
| **Dead**      | Agent process terminated and its PTY session is no longer alive.     |

Status transitions are derived from the underlying PTY session state. A Running agent whose tmux session process exits transitions to Dead. A Dead agent can be relaunched.

---

## User Interface

Jefe is a fullscreen TUI application. It operates in the terminal's alternate screen mode with mouse event capture. The UI is dark-themed by default, with **Green Screen** (monochrome green on black) as the mandatory default palette.

### Dashboard (Main View)

The dashboard is a three-column layout:

1. **Repository Sidebar** (fixed width, left): Lists all repositories with agent counts. The selected repository is highlighted with inverse-video selection. Arrow keys navigate; the selection indicator is `▸`.

2. **Agent List + Terminal** (flexible width, center): The top portion (~25% of height) shows agents for the selected repository with status icons, display IDs, names, and elapsed timers. The bottom portion (~75% of height) is a live embedded terminal view rendering the active agent's PTY output with full color, bold, underline, and cursor fidelity.

3. **Preview Pane** (fixed width, right): Shows detail for the selected agent — status, profile, mode, todo list with completion markers, and recent output lines. When no agent is selected, a static placeholder preserves layout consistency.

The top status bar shows the application name, repository count, running/total agent counts, and the active theme name. The bottom keybinding bar shows context-sensitive keyboard shortcuts.

### Split View

Split view shows one row per running agent across all (or a filtered subset of) repositories. Each row displays the agent's repository, display ID, name, elapsed time, current in-progress todo, and last output line. The sidebar acts as a repository filter (with an "All" option at the top).

Split view supports **grab-and-reorder**: pressing Enter on a selected agent "grabs" it (shown with inverse-video and `≡` marker), and arrow keys swap its position relative to other running agents. Pressing Enter again releases the grab.

### Terminal Focus Mode

Pressing F12 toggles terminal focus. When focused, all keyboard input is forwarded to the agent's PTY. The terminal view border changes to double-line style to indicate capture. F12 is the only key Jefe intercepts in focused mode. Mouse events are forwarded when the child application has mouse reporting enabled.

### Agent Creation Form

Pressing `n` opens a form to launch a new agent under the currently selected repository. Fields:

| Field              | Behavior                                                                     |
|--------------------|------------------------------------------------------------------------------|
| **Name**           | Required. Short task description. Auto-derives the working directory slug.   |
| **Description**    | Optional. Longer explanation of the agent's purpose.                         |
| **Work dir**       | Auto-populated from repo base dir + name slug. User-editable.                |
| **Profile**        | Inherited from repository default. Editable. Empty means agent CLI defaults. |
| **Mode**           | Defaults to `--yolo`. Free-text for arbitrary flags.                         |
| **Pass --continue**| Checkbox (default on). Appends `--continue` to the mode flags.               |

On submit, Jefe creates the working directory on disk, spawns a tmux session running the agent CLI in that directory with the configured profile and mode flags, allocates a PTY slot, and returns to the dashboard with the new agent selected.

### Repository Creation Form

Pressing `N` opens a form to register a new repository. Fields: Name, Base directory, Default profile. On submit the base directory is created on disk if it does not exist.

### Agent and Repository Editing

Pressing Enter on a selected agent or repository opens the same form pre-populated with existing values. Changes to name, description, working directory, profile, and mode are applied immediately.

### Deletion

Pressing `d` triggers a confirmation dialog. For agents, the dialog offers the option to also delete the agent's working directory from disk. For repositories, all agents are removed along with the repository entry. The delete-working-directory checkbox defaults to checked and is toggleable via Space, `d`, or arrow keys within the confirmation modal.

### Kill and Relaunch

- **Kill (`k`)**: Terminates the agent's tmux session and marks it Dead.
- **Relaunch (`l`)**: Re-creates the tmux session for a Dead agent from its original working directory, profile, and mode. The agent returns to Running.

### Help Modal

Pressing `?`, `h`, or F1 opens a scrollable keyboard shortcut reference overlay. The modal supports up/down scrolling when content exceeds the available terminal height.

### Confirmation Dialogs

Destructive actions (delete agent, delete repository, kill agent) require explicit confirmation via a centered modal with `[Enter] Confirm` / `[Esc] Cancel`.

---

## Keyboard Shortcuts

### Dashboard

| Key        | Action                                        |
|------------|-----------------------------------------------|
| `↑` / `↓` | Navigate within current pane                  |
| `←` / `→` | Switch pane focus (sidebar → list → preview)  |
| `r`        | Focus repository sidebar                      |
| `a`        | Focus agent list                              |
| `t`        | Focus terminal pane (no input capture)        |
| `F12`      | Toggle terminal input capture                 |
| `n`        | New agent                                     |
| `N`        | New repository                                |
| `e`/Enter  | Edit selected agent or repository             |
| `d`        | Delete selected item (with confirmation)      |
| `k`        | Kill running agent                            |
| `l`        | Relaunch dead agent                           |
| `s`        | Toggle split view                             |
| `/`        | Open search/command palette                   |
| `?`/`h`/F1 | Help                                         |
| `1`/`2`/`3`| Switch theme (Green Screen / Dracula / Dark) |
| `q`        | Quit                                          |

### Split View

| Key        | Action                                        |
|------------|-----------------------------------------------|
| `↑` / `↓` | Navigate repos or agents depending on focus   |
| `r`        | Focus repository filter sidebar               |
| `a`        | Focus agent rows                              |
| `Enter`    | Grab/ungrab selected agent for reorder        |
| `m`        | Return to dashboard with terminal focused     |
| `Esc`      | Return to dashboard without terminal focus    |

### Forms

| Key            | Action                                    |
|----------------|-------------------------------------------|
| `Tab` / `↓`   | Next field                                |
| `Shift+Tab`/`↑`| Previous field                           |
| `Space`        | Toggle checkbox                           |
| `Enter`        | Submit form                               |
| `Esc`          | Cancel and return to dashboard            |

---

## Theme System

Jefe ships with three embedded themes. The **Green Screen** theme is the default and is always available.

| Theme        | Background | Foreground | Character                   |
|--------------|------------|------------|-----------------------------|
| Green Screen | `#000000`  | `#6a9955`  | Monochrome green. Default.  |
| Dracula      | `#282a36`  | `#f8f8f2`  | Popular dark multi-color.   |
| Default Dark | `#1e1e1e`  | `#d4d4d4`  | VS Code-style neutral dark. |

All built-in themes are dark. No bright or light default palettes exist. The default theme uses green-on-black with `#00ff00` reserved for high-emphasis elements (e.g., the running-status indicator) and `#4a7035` for muted/secondary text.

Themes define colors for: foreground hierarchy (primary, bright, dim), backgrounds, borders (default and focused), selection (inverse-video), status indicators per lifecycle state, accent colors, diff highlights, input fields, and scrollbar elements.

Theme switching is available at runtime via `1`/`2`/`3` number keys. External themes are loadable from JSON files in a user-configurable directory (set via `JEFE_THEME_DIR` environment variable).

The PTY terminal view applies theme color defaults to ANSI color rendering. When the child process uses default foreground/background colors (rather than explicit ANSI codes), the terminal view maps them to the active theme's palette. The full 256-color and RGB palette are supported; only the logical default/named colors follow the theme.

---

## Persistence

### Settings File

Jefe persists user settings to `~/.config/jefe/settings.json` (or the platform-appropriate config directory). Settings include:

- Active theme slug.
- Window preferences (if applicable).
- Default profile overrides.

### State File

Jefe persists repository and agent metadata to `~/.local/share/jefe/state.json` (or the platform-appropriate data directory). The state file contains:

- All repository definitions (name, slug, base_dir, default_profile).
- All agent definitions per repository (name, description, work_dir, profile, mode, display_id).
- Agent runtime status is NOT persisted — it is re-derived from tmux session liveness on startup.

On startup, Jefe loads the state file, reconciles agent records against live tmux sessions (marking agents whose sessions no longer exist as Dead), and renders the dashboard.

On every mutation (agent created, repository added, agent edited, item deleted), the state file is written to disk.

---

## Agent Execution Model

### tmux as the PTY Backend

Every agent runs inside a dedicated tmux session. Jefe creates tmux sessions named `jefe-{slot}` where `{slot}` is the agent's PTY slot index. The tmux session runs the agent CLI directly (not a shell) — when the agent CLI process exits, the tmux session terminates, and Jefe detects this as a Dead agent.

Jefe maintains a single **attached viewer** PTY at a time. This viewer is a `tmux attach-session` process whose output is parsed through an Alacritty terminal emulator model for cell-accurate rendering. Switching the selected agent triggers a viewer teardown-and-reattach cycle: the old viewer child is killed, its reader thread is joined (with a bounded timeout to prevent hangs), and a new viewer is spawned against the target agent's tmux session.

### PTY Input Forwarding

When terminal focus is active, Jefe writes raw keystrokes to the attached viewer's PTY master. This includes special keys, control sequences, and printable characters. Mouse events are forwarded when the child application reports mouse-mode capability via the terminal mode flags.

### PTY Rendering

The terminal view renders a cell grid derived from the Alacritty terminal model. Each cell carries a character, foreground color, background color, bold flag, and underline flag. Cells with identical styles are coalesced into text runs to minimize rendering overhead. Theme color defaults are applied to cells using logical named/indexed ANSI colors. Trailing whitespace runs are trimmed per row.

### Resize Handling

When the host terminal resizes, Jefe recalculates the layout geometry, resizes the PTY master, and resizes the Alacritty terminal model to match. The PTY dimensions account for UI chrome (borders, headers, status bars).

---

## ACP Sideband (Extensibility)

Jefe's v1 agent monitoring is PTY-based: agent status is derived from process liveness, and agent output is rendered from the terminal stream. A richer monitoring channel is available as an optional extension via the **Agent Communication Protocol (ACP)**.

ACP is a JSON-RPC 2.0 protocol. When the agent CLI supports a socket transport (`--acp-transport=socket`), Jefe can connect to the agent's Unix domain socket and receive structured events:

- **Plan/todo updates**: Streamed task lists with per-item status (pending, in-progress, completed).
- **Message chunks**: Real-time agent text output.
- **Tool call status**: Tool invocations with pending/in-progress/completed/failed status and diff content.
- **Permission requests**: Agents blocked waiting for user approval.

This sideband is additive. It does not replace the PTY-based interaction model. It enriches the preview pane with structured data that the PTY stream alone cannot provide (e.g., parsed todo items rather than raw terminal text).

The ACP socket transport preserves compatibility with other ACP clients (e.g., IDE integrations that use stdio transport). Jefe's use of socket transport is orthogonal to the terminal session.
