# Jefe UI Mockups — iocraft TUI

_Created: 2/12/2026_

These mockups represent the proposed TUI for Jefe built with iocraft (Rust).
All layouts use flexbox (via taffy). Box-drawing characters represent `View`
components with `border_style: BorderStyle::Round`. Colors noted in comments.

Terminal size assumed: **120×40** (responsive via flexbox).

---

## Screen 1: Dashboard (Main View)

This is the home screen. Three-column layout: project sidebar, task list, and
a task preview/detail pane. The sidebar is ~22 cols fixed, preview is ~38 cols
fixed, task list fills remaining space.

```
╭─ Jefe ──────────────────────────────────────────────────────────────────────────────────────────────── 3 projects ─╮
│╭─ Projects ─────────╮╭─ Tasks: llxprt-code ─────────────────────────────────╮╭─ Preview ──────────────────────╮│
││                     ││                                                      ││                                ││
││  ▸ llxprt-code  (3) ││  ● #1872 Fix ACP socket timeout    00:42:17  ██░░░░  ││  #1872 Fix ACP socket timeout  ││
││    starflight   (2) ││  ● #1899 Refactor prompt handler    01:15:03  ██████  ││                                ││
││    gable-work   (1) ││  ○ #1905 Add retry on 429           idle      ──────  ││  Status:  ● Running  00:42:17  ││
││    mariadb-cli  (0) ││                                                      ││  Agent:   claude-opus-4-6      ││
││                     ││                                                      ││  Profile: default              ││
││                     ││                                                      ││  Dir:     ~/worktrees/lx-1872  ││
││                     ││                                                      ││                                ││
││                     ││                                                      ││  ── Todo ───────────────────── ││
││                     ││                                                      ││  [OK] Read issue description      ││
││                     ││                                                      ││  [OK] Find relevant source files  ││
││                     ││                                                      ││  ▸ Implement socket timeout    ││
││                     ││                                                      ││  ○ Write tests                 ││
││                     ││                                                      ││  ○ Run CI checks               ││
││                     ││                                                      ││                                ││
││                     ││                                                      ││  ── Last Output ────────────── ││
││                     ││                                                      ││  Editing src/acp/socket.rs     ││
││                     ││                                                      ││  Added timeout parameter to    ││
││                     ││                                                      ││  connect() with default of     ││
││                     ││                                                      ││  30 seconds...                 ││
││                     ││                                                      ││                                ││
│╰─────────────────────╯╰──────────────────────────────────────────────────────╯╰────────────────────────────────╯│
│                                                                                                                 │
│ ↑↓ navigate  ←→ switch pane  enter open task  n new task  d delete  / search  ? help  q quit                    │
╰─────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### Layout Breakdown

```
View (flex_direction: Column, width: 100%, height: 100%)                 // outer frame
├── View (flex_direction: Row, flex_grow: 1.0)                           // main content area
│   ├── View (width: 22, border, padding: 1)                            // sidebar
│   │   └── ProjectList component (custom)
│   ├── View (flex_grow: 1.0, border, padding: 1)                       // task list
│   │   └── TaskTable component (custom)
│   └── View (width: 38, border, padding: 1)                            // preview
│       ├── TaskHeader
│       ├── TodoList
│       └── OutputPreview
└── View (height: 1, padding_left: 1)                                   // keybindings bar
    └── Text (keybindings, dim color)
```

### Colors

| Element                  | Color            |
|--------------------------|------------------|
| App title "Jefe"         | `Color::Cyan` bold |
| Selected project `▸`    | `Color::Yellow` bold |
| Unselected project       | `Color::White` |
| Task count `(3)`         | `Color::DarkGrey` |
| `●` running              | `Color::Green` |
| `●` errored              | `Color::Red` |
| `○` idle/completed       | `Color::DarkGrey` |
| `◉` waiting for input    | `Color::Yellow` |
| Progress bar filled      | `Color::Blue` |
| Progress bar empty       | `Color::DarkGrey` |
| Selected row background  | `Color::DarkBlue` (or ANSI 236) |
| Timer                    | `Color::DarkGrey` |
| Todo [OK]                   | `Color::Green` |
| Todo ▸ (in progress)     | `Color::Yellow` |
| Todo ○ (pending)         | `Color::DarkGrey` |
| Keybinding keys          | `Color::Cyan` bold |
| Keybinding descriptions  | `Color::DarkGrey` |
| Section headers          | `Color::Magenta` |

---

## Screen 2: Task Detail (Expanded View)

When the user presses `Enter` on a task from the dashboard, the preview pane
expands to fill the right ~65% and shows detailed agent information. The task
list collapses to a narrow strip. Press `Esc` to go back to dashboard.

```
╭─ Jefe ──────────────────────────────────────────────────────────────────────────────────────────────── 3 projects ─╮
│╭─ Tasks ────────────╮╭─ #1872 Fix ACP socket timeout ── ● Running ─────────────────────────────────────────────╮│
││                     ││                                                                                          ││
││  ● #1872 Fix ACP…   ││  Project:   llxprt-code                                                                 ││
││  ● #1899 Refactor…  ││  Agent:     claude-opus-4-6 via anthropic                                               ││
││  ○ #1905 Add retr…  ││  Profile:   default                                                                     ││
││                     ││  Directory: ~/worktrees/llxprt-code-1872                                                 ││
││  ── starflight ──   ││  Uptime:    00:42:17                                                                     ││
││  ● TLS renegotia…   ││  Mode:      --yolo                                                                      ││
││  ○ Cert rotation…   ││                                                                                          ││
││                     ││  ╭─ Todo List ──────────────────────────────────────────────────────────────────────────╮ ││
││  ── gable-work ──   ││  │  [OK] Read issue #1872 description and comments                                      │ ││
││  ● API migration…   ││  │  [OK] Search for ACP socket connection code in src/acp/                               │ ││
││                     ││  │  [OK] Find relevant source files: socket.rs, transport.rs                              │ ││
││                     ││  │  ▸ Implement configurable socket timeout in connect()                               │ ││
││                     ││  │  ○ Add timeout to ACP client handshake                                              │ ││
││                     ││  │  ○ Write unit tests for timeout behavior                                            │ ││
││                     ││  │  ○ Run existing test suite                                                          │ ││
││                     ││  │  ○ Open PR with changes                                                             │ ││
││                     ││  ╰──────────────────────────────────────────────────────────────────────────────────────╯ ││
││                     ││                                                                                          ││
││                     ││  ╭─ Recent Agent Output ──────────────────────────────────────────────── scrollable ↕ ──╮ ││
││                     ││  │  I'll now implement the configurable timeout. Looking at the connect()               │ ││
││                     ││  │  function in src/acp/socket.rs:                                                      │ ││
││                     ││  │                                                                                      │ ││
││                     ││  │  ● Tool: read_file  src/acp/socket.rs                              [OK] completed      │ ││
││                     ││  │  ● Tool: replace    src/acp/socket.rs:42-58                        [OK] completed      │ ││
││                     ││  │  ● Tool: replace    src/acp/transport.rs:15-20                     ▸ in progress    │ ││
││                     ││  │                                                                                      │ ││
││                     ││  │  Adding a timeout parameter with a default of 30 seconds. The                       │ ││
││                     ││  │  socket.connect() call now wraps with tokio::time::timeout...                        │ ││
││                     ││  ╰──────────────────────────────────────────────────────────────────────────────────────╯ ││
│╰─────────────────────╯╰──────────────────────────────────────────────────────────────────────────────────────────╯│
│                                                                                                                   │
│ ↑↓ navigate  esc back  t terminal  s send prompt  p pause agent  k kill  l logs  ? help                          │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### Key interactions from this view

| Key     | Action                                          |
|---------|------------------------------------------------|
| `t`     | Enter fullscreen terminal (Screen 3)            |
| `s`     | Open prompt input to send follow-up to agent    |
| `p`     | Pause/resume the agent                          |
| `k`     | Kill the agent (with confirmation)              |
| `l`     | View full scrollback log                        |
| `Esc`   | Return to dashboard                             |

---

## Screen 3: Agent Terminal (Fullscreen)

Pressing `t` from the task detail drops into a fullscreen embedded terminal
showing the actual llxprt-code TUI. A thin status bar at top and bottom
provides context. All input is forwarded to the agent's PTY.

The terminal content area is rendered by parsing the agent's PTY output through
`alacritty_terminal`/`vte` and painting it into an iocraft `View`.

```
╭─ llxprt-code ── #1872 Fix ACP socket timeout ── llxprt-code (3) ── ● Running 00:42:17 ── Ctrl+] detach ─╮
│                                                                                                           │
│  llxprt-code v1.42.0                                                                                     │
│                                                                                                           │
│  ╭─────────────────────────────────────────────────────────────────────────────────────────────────────╮  │
│  │  assistant                                                                                        │  │
│  │                                                                                                    │  │
│  │ I'll now add the timeout parameter to the transport layer. Let me update transport.rs:             │  │
│  │                                                                                                    │  │
│  │ ╭──────────────────────────────────────────────────────────────────────────────────────────────╮   │  │
│  │ │ replace src/acp/transport.rs                                                                │   │  │
│  │ │                                                                                              │   │  │
│  │ │ @@ -15,6 +15,10 @@                                                                         │   │  │
│  │ │ - pub async fn connect(addr: &str) -> Result<Self> {                                        │   │  │
│  │ │ + pub async fn connect(addr: &str, timeout: Duration) -> Result<Self> {                     │   │  │
│  │ │ +     let stream = tokio::time::timeout(                                                    │   │  │
│  │ │ +         timeout,                                                                          │   │  │
│  │ │ +         TcpStream::connect(addr)                                                          │   │  │
│  │ │ +     ).await.map_err(|_| Error::ConnectionTimeout)?;                                       │   │  │
│  │ │                                                                                              │   │  │
│  │ ╰──────────────────────────────────────────────────────────────────────────────────────────────╯   │  │
│  │                                                                                                    │  │
│  │ Now let me update the tests...                                                                     │  │
│  │                                                                                                    │  │
│  ╰────────────────────────────────────────────────────────────────────────────────────────────────────╯  │
│                                                                                                           │
│  > You can interact with the agent here. Type a message or use tool approval shortcuts.                  │
│                                                                                                           │
╰─ CPU 12% ── MEM 284MB ── Tokens: 42.1k in / 8.3k out ── Cost: $1.24 ─────────── Ctrl+] back to Jefe ───╯
```

### Notes

- **`Ctrl+]`** is the detach hotkey (inspired by tmux's `Ctrl+b` / screen's `Ctrl+a`).
  This is the _only_ key Jefe intercepts — everything else goes to the agent.
- The terminal view is a custom iocraft component wrapping a PTY + terminal state parser.
- The top bar shows: project name, task ID, task title, agent count, status, timer, detach hint.
- The bottom bar shows: resource usage (from `/proc` or `ps`), token count, cost estimate.
- Future: these metrics come from ACP in Phase 2.

---

## Screen 4: New Task Dialog

Pressing `n` from the dashboard opens a modal overlay for creating a new task.
This is an iocraft fullscreen overlay rendered on top of the dashboard, using
`View` with background color for the dimmed backdrop.

```
╭─ Jefe ──────────────────────────────────────────────────────────────────────────────────────────────── 3 projects ─╮
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░╭─ New Task ──────────────────────────────────────────────────────────╮░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  Project:     ┌──────────────────────────────────────────────┐       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │ llxprt-code                              ▾  │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               └──────────────────────────────────────────────┘       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  Task name:   ┌──────────────────────────────────────────────┐       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │ Fix issue #2010 - handle TLS cert renewal   │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               └──────────────────────────────────────────────┘       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  Prompt:      ┌──────────────────────────────────────────────┐       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │ Fix issue #2010. The TLS certificate renew  │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │ handler doesn't properly re-negotiate when  │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │ the cert is rotated. See the issue for det  │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │ ails and reproduction steps.                │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               └──────────────────────────────────────────────┘       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  Work dir:    ┌──────────────────────────────────────────────┐       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │ ~/worktrees/llxprt-code-2010    (auto)      │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               └──────────────────────────────────────────────┘       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  Profile:     [ default     ▾ ]    Model: [ claude-opus-4-6 ▾ ]     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  Mode:        (●) --yolo  ( ) interactive  ( ) supervised           │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  ── Advanced ──────────────────────────────────────────────────      │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│  Extra flags: ┌──────────────────────────────────────────────┐       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               │                                              │       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│               └──────────────────────────────────────────────┘       │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                          [ Cancel ]    [  Launch Task ]             │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░│                                                                     │░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░╰─────────────────────────────────────────────────────────────────────╯░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│ tab next field  shift+tab prev field  enter confirm  esc cancel                                                    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### Key Behaviors

- **Project dropdown**: Populated from configured projects.
- **Work dir**: Auto-generated from project base + task name/issue number.
  User can override. Jefe can auto-create a git worktree.
- **Profile/Model dropdowns**: Populated from llxprt-code's profile configs.
- **"Launch Task"** creates the worktree (if needed), spawns the agent in a
  PTY, and returns to the dashboard with the new task visible.

---

## Screen 5: Command Palette

Pressing `/` or `Ctrl+P` opens a fuzzy-finder command palette overlay.
This gives quick access to any task, project, or action without navigating
the hierarchy manually.

```
╭─ Jefe ──────────────────────────────────────────────────────────────────────────────────────────────── 3 projects ─╮
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░╭──────────────────────────────────────────────────────────────╮░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│   socket tim▌                                              │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░├──────────────────────────────────────────────────────────────┤░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│  ▸ ● llxprt-code / #1872 Fix ACP socket timeout   00:42:17  │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│    ● starflight / TLS renegotiation timeout        00:18:42  │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│                                                              │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│  ── Actions ─────────────────────────────────────────────     │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│    [ACTION] New task...                                             │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│    [ACTION] Add project...                                          │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│    [ACTION] Kill all agents                                         │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░│                                                              │░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░╰──────────────────────────────────────────────────────────────╯░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│ type to filter  ↑↓ navigate  enter select  esc close                                                              │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

### Search Behavior

- Results grouped: **Tasks** (fuzzy matched on project + task name) first, then **Actions**.
- Selecting a task navigates directly to its detail view.
- Selecting an action performs it (e.g., "New task..." opens Screen 4).

---

## Screen 6: Task Status Indicators (All States)

These show how different agent states appear in the task list:

```
╭─ Tasks: llxprt-code ─────────────────────────────────────────────────────────────────────╮
│                                                                                           │
│  ● #1872 Fix ACP socket timeout           Running          00:42:17  ▸ Implement timeo…  │
│  ● #1899 Refactor prompt handler           Running          01:15:03  ▸ Update tests       │
│  [OK] #1905 Add retry on 429                  Completed        00:28:41  PR #1912 opened     │
│   #1910 Fix race condition in pool        Errored          00:05:22  Exit code 1         │
│  ◉ #1915 Update auth flow                  Waiting          00:12:08  Permission needed   │
│  ◼ #1920 Migrate config format             Paused           00:33:50  Paused by user      │
│  ○ #1925 Upgrade dependencies              Queued           ──:──:──  Waiting for slot    │
│                                                                                           │
╰───────────────────────────────────────────────────────────────────────────────────────────╯
```

### Status Legend

| Icon | Color         | Status     | Meaning                                      |
|------|---------------|------------|----------------------------------------------|
| `●`  | Green         | Running    | Agent actively working                       |
| `[OK]`  | Bright Green  | Completed  | Agent finished successfully                  |
| ``  | Red           | Errored    | Agent crashed or task failed                 |
| `◉`  | Yellow        | Waiting    | Agent blocked (permission, input, etc.)      |
| `◼`  | Blue          | Paused     | User explicitly paused the agent             |
| `○`  | Dark Grey     | Queued     | Waiting for an agent slot (concurrency limit)|

---

## Screen 7: Send Prompt Overlay

Pressing `s` from the task detail view opens an inline prompt input, allowing
the user to send a follow-up message to a running agent via ACP (Phase 2).

```
╭─ #1872 Fix ACP socket timeout ── ● Running ──────────────────────────────────────────────────────────────────────╮
│                                                                                                                   │
│  ╭─ Recent Agent Output ──────────────────────────────────────────────────────────────────────────────────────╮   │
│  │                                                                                                            │   │
│  │  I've implemented the timeout parameter and updated the transport layer. The connect()                     │   │
│  │  function now accepts an optional Duration parameter. Should I proceed with the tests?                     │   │
│  │                                                                                                            │   │
│  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯   │
│                                                                                                                   │
│  ╭─ Send Prompt ──────────────────────────────────────────────────────────────────────────────────────────────╮   │
│  │                                                                                                            │   │
│  │  Yes, write the tests. Also make sure to test the case where the timeout is set to zero                   │   │
│  │  (should fail immediately) and the case where the server is slow but responds before the                   │   │
│  │  timeout expires.▌                                                                                         │   │
│  │                                                                                                            │   │
│  ╰────────────────────────────────────────────────────────────────────────────────────────── Ctrl+Enter send ─╯   │
│                                                                                                                   │
╰─ Ctrl+Enter send  esc cancel ────────────────────────────────────────────────────────────────────────────────────╯
```

---

## Screen 8: Confirmation Dialogs

Destructive actions get a small centered confirmation modal:

```
                    ╭─ Kill Agent ────────────────────────────────────╮
                    │                                                  │
                    │  Are you sure you want to kill the agent for:    │
                    │                                                  │
                    │  ● #1872 Fix ACP socket timeout                  │
                    │    llxprt-code · Running for 00:42:17            │
                    │                                                  │
                    │  The agent process will be terminated.            │
                    │  Any unsaved work may be lost.                    │
                    │                                                  │
                    │          [ Cancel ]    [ Kill Agent ]             │
                    │                                                  │
                    ╰──────────────────────────────────────────────────╯
```

---

## Navigation Model

```
                    ┌──────────────┐
                    │  Dashboard   │  (Screen 1)
                    │  Main View   │
                    └──────┬───────┘
                           │
              ┌────────────┼──────────────┐
              │            │              │
              ▼            ▼              ▼
     ┌────────────┐ ┌───────────┐ ┌─────────────┐
     │ New Task   │ │  Command  │ │ Task Detail  │
     │ (Screen 4) │ │  Palette  │ │ (Screen 2)   │
     │   modal    │ │ (Screen 5)│ │              │
     └────────────┘ └───────────┘ └──────┬───────┘
                                          │
                           ┌──────────────┼─────────────┐
                           │              │             │
                           ▼              ▼             ▼
                    ┌─────────────┐ ┌──────────┐ ┌───────────┐
                    │  Terminal   │ │  Send    │ │  Kill /   │
                    │ (Screen 3)  │ │  Prompt  │ │  Confirm  │
                    │ fullscreen  │ │ (Scr. 7) │ │ (Scr. 8)  │
                    └─────────────┘ └──────────┘ └───────────┘
```

**Navigation is modal but shallow** — you're never more than 2 levels deep from
the dashboard. `Esc` always goes back one level. `Ctrl+]` always escapes
from the terminal view.

---

## iocraft Component Hierarchy (Proposed)

```
App
├── StatusBar (top)
├── MainContent (flex row, flex_grow: 1)
│   ├── Sidebar
│   │   └── ProjectList
│   │       └── ProjectItem (repeated)
│   ├── TaskPane
│   │   └── TaskList
│   │       └── TaskRow (repeated)
│   └── DetailPane (conditional: preview vs expanded)
│       ├── TaskHeader
│       ├── TodoList
│       │   └── TodoItem (repeated)
│       ├── AgentOutput (scrollable)
│       │   └── OutputLine (repeated, includes ToolCallLine variant)
│       └── PromptInput (conditional, Screen 7)
├── KeybindBar (bottom, height: 1)
│
├── [Overlay: NewTaskModal] (conditional, Screen 4)
│   ├── DropdownField (project, profile, model)
│   ├── TextInputField (name, prompt, workdir, extra flags)
│   ├── RadioGroup (mode)
│   └── ButtonRow (cancel, launch)
│
├── [Overlay: CommandPalette] (conditional, Screen 5)
│   ├── SearchInput
│   └── ResultList
│       └── ResultItem (repeated)
│
├── [Overlay: ConfirmDialog] (conditional, Screen 8)
│
└── [Fullscreen: TerminalView] (conditional, Screen 3)
    ├── TerminalStatusBar (top)
    ├── TerminalCanvas (PTY renderer, flex_grow: 1)
    └── TerminalInfoBar (bottom)
```

---

## Responsive Behavior

| Terminal Width | Behavior                                                |
|----------------|---------------------------------------------------------|
| ≥ 120 cols     | Full 3-column layout (sidebar + tasks + preview)        |
| 90–119 cols    | 2-column: sidebar + tasks. Preview only on Enter.       |
| < 90 cols      | Single column: stack views. Sidebar collapses to icons. |

iocraft's taffy flexbox handles this naturally via `flex_shrink`, `flex_basis`,
and `min_width` on the panes.

---

## Theme System

Jefe uses the same general theme architecture as llxprt-code:

- **JSON-based theme definitions** in `themes/*.json`
- **Embedded themes** compiled into the binary via `include_str!`
- **ThemeManager** handles loading, switching, and cycling
- **Default theme: Green Screen** — monochrome green on black (`#6a9955` fg, `#00ff00` bright, `#000000` bg)
- **Theme cycling** via `T` key at runtime
- **Custom themes** loadable from external JSON files

### Built-in Themes

| Theme         | Kind | Background | Foreground | Notes                          |
|---------------|------|------------|------------|--------------------------------|
| Green Screen  | dark | `#000000`  | `#6a9955`  | Default. Monochrome green.     |
| Dracula       | dark | `#282a36`  | `#f8f8f2`  | Popular dark theme.            |
| Default Dark  | dark | `#1e1e1e`  | `#d4d4d4`  | VS Code-style dark.            |

### Theme Color Structure (ThemeColors)

Each theme defines ~30 color slots:
- `foreground`, `bright_foreground`, `dim_foreground` — text hierarchy
- `background`, `panel_bg` — backgrounds
- `border`, `border_focused` — container borders
- `status_running`, `status_completed`, `status_error`, `status_waiting`, `status_paused`, `status_queued`
- `accent_primary`, `accent_warning`, `accent_error`, `accent_success`
- `selection_fg`, `selection_bg` — selection highlighting
- `diff_added_*`, `diff_removed_*` — diff colors
- `input_bg`, `input_fg`, `input_placeholder` — form inputs
- `scrollbar_thumb`, `scrollbar_track`

### ResolvedColors Helper

To avoid verbose `Option<ThemeColors>` unwrapping in every component, the
`ResolvedColors` struct pre-extracts the 5 most common colors (fg, bright, dim,
border, border_focused) with green-screen fallbacks. Components call
`ResolvedColors::from_theme(props.colors.as_ref())` once at the top.

---

## Phase 1 vs Phase 2 Differences

| Feature                  | Phase 1                           | Phase 2 (ACP)                     |
|--------------------------|-----------------------------------|------------------------------------|
| Task status              | alive/dead (process check)        | Running/Completed/Errored/Waiting  |
| Todo list                | Not available                     | Live from `plan` ACP events        |
| Agent output             | Not available (use terminal)      | Streamed via `agent_message_chunk` |
| Tool calls               | Not available                     | Live via `tool_call` events        |
| Send prompt              | Not available                     | Via ACP `session/prompt`           |
| Preview pane             | Minimal (config only)             | Full live view                     |
| Terminal view             | Full PTY embed                   | Full PTY embed (same)              |

In Phase 1, the preview pane shows only static info (config, directory, uptime).
The primary interaction is "launch and attach to terminal." Phase 2 lights up
all the live data panels without needing to attach.
