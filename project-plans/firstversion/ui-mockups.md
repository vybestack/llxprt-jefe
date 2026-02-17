# Jefe v1 UI Mockups (Based on `toy1`)

These mockups are grounded in the current `toy1` implementation (`src/ui/**`, `src/app.rs`, `src/main.rs`).
They describe current behavior, not future/aspirational UI.

---

## Global Layout and Visual Rules

- **Top status bar**: one row
- **Bottom keybind bar**: one row
- Main content area in dashboard is 3 columns:
  - **Left** sidebar: `22` cols
  - **Middle** content: flexible width
  - **Right** preview: `36` cols
- Middle column in dashboard is split vertically:
  - **Top 25%**: agent list
  - **Bottom 75%**: terminal view
- Focused panes use **double border**; unfocused use **round border**.
- Default theme is **Green Screen**.

Theme hotkeys (when not in search/forms):
- `1` → `green-screen`
- `2` → `dracula`
- `3` → `default-dark`

---

## Icon Semantics (Current Code)

From `src/presenter/format.rs`:

### Agent status icons
- `o` Running
- `+` Completed
- `x` Errored
- `*` Waiting
- `#` Paused
- `-` Queued
- `!` Dead

### Todo icons
- `+` Completed
- `>` In progress
- `-` Pending

---

## 1) Dashboard (Default View)

```text
 Jefe  4 repos                                             3  running  9  total  [Green Screen]
╭──────────────────────╮╭──────────────────────────────────────────────╮╭──────────────────────────────╮
│ Repositories         ││ Agents: llxprt-code                          ││ #1872 Fix ACP socket timeout │
│                      ││                                              ││  Status:  o Running  00:42:17│
│ ▸ llxprt-code (3)    ││ ▸ o #1872 Fix ACP socket timeout  00:42:17  ││  Profile: default            │
│   starflight (2)     ││   o #1899 Refactor prompt loop    01:15:03  ││  Mode:    --yolo --continue  │
│   jefe (3)           ││   + #1902 Add tests               00:08:10  ││                              │
│   client-gable (1)   ││                                              ││  -- Todo --                  │
│                      │╰──────────────────────────────────────────────╯│  + Read issue                │
│                      │╭──────────────────────────────────────────────╮│  + Locate files              │
│                      ││ Terminal (F12 to focus)                      ││  > Implement timeout         │
│                      ││                                              ││  - Write tests               │
│                      ││  $ llxprt --profile-load default --yolo ...  ││                              │
│                      ││  ...PTY output...                             ││  -- Output --                │
│                      ││                                              ││  > Editing src/acp/socket.rs │
│                      ││                                              ││  Added timeout parameter...  │
│                      │╰──────────────────────────────────────────────╯│                              │
╰──────────────────────╯                                                ╰──────────────────────────────╯
 ^/v navigate  </> pane  r repo  a list  t terminal  s split  F12 focus/unfocus  k kill  d delete  l relaunch(dead)  q quit
```

### Behavior

- `↑/↓` navigates within focused pane.
- `←/→` cycles pane focus: sidebar → agent list → preview (and back).
- `r` focuses repository sidebar.
- `a` focuses agent list.
- `t` focuses terminal and enables terminal input mode.
- `d` opens delete confirmation:
  - from sidebar: delete repository
  - from agent list/preview: delete agent
- `k` kills selected agent session.
- `l` relaunches selected dead agent.
- `s` toggles split mode.

---

## 2) Dashboard Terminal Focus State (`F12`)

### Unfocused terminal

```text
╭──────────────────────────────────────────────╮
│ Terminal (F12 to focus)                      │
│ ...output visible...                         │
╰──────────────────────────────────────────────╯
```

### Focused terminal

```text
╔══════════════════════════════════════════════╗
║ Terminal (F12 to unfocus)                    ║
║ ...all keys forwarded to PTY...              ║
╚══════════════════════════════════════════════╝
```

### Behavior

- `F12` toggles terminal focus.
- When focused, keystrokes are forwarded to PTY (except F12 itself, used as escape hatch).
- Mouse events are also PTY-forwarded when terminal-focused and inside terminal bounds.

---

## 3) Split Mode Screen

```text
 Jefe  4 repos                                             3  running  9  total  [Green Screen]
╭──────────────────────╮╭──────────────────────────────────────────────────────────────────────────╮
│ Repositories         ││ SPLIT - Agents (3) [↑↓ select, enter grab, esc back]                    │
│                      ││                                                                          │
│ ▸ All (9)            ││ ╭──────────────────────────────────────────────────────────────────────╮ │
│   llxprt-code (3)    ││ │ ▸ o llxprt-code / #1872 Fix ACP socket timeout   00:42:17         │ │
│   starflight (2)     ││ │   Todo: > Implement timeout                                            │ │
│   jefe (3)           ││ │   Last: > Editing src/acp/socket.rs...                               │ │
│   client-gable (1)   ││ ╰──────────────────────────────────────────────────────────────────────╯ │
│                      ││ ╭──────────────────────────────────────────────────────────────────────╮ │
│                      ││ │   o starflight / #2001 TLS negotiation fix       00:12:08          │ │
│                      ││ │   Todo: > Reproduce race                                               │ │
│                      ││ │   Last: > Investigating handshake state...                           │ │
│                      ││ ╰──────────────────────────────────────────────────────────────────────╯ │
╰──────────────────────╯╰──────────────────────────────────────────────────────────────────────────╯
 a arm reorder  ↑/↓ move selected  enter unselect  m main+pty focus  esc main no pty focus
```

### Behavior

- `s` from dashboard enters split mode.
- Left pane includes `All` filter at cursor index `0`.
- Split focus modes:
  - `Repos` focus (`r`/default in split)
  - `Agents` focus (`a`)
- `Enter` in repo pane applies repo filter.
- `Enter` in agent pane toggles **grabbed** state for reordering.
- While grabbed, moving `↑/↓` swaps agent positions.
- `m` returns to dashboard and sets terminal focused.
- `Esc` behavior in split:
  - if grabbed: ungrab
  - else if focused on agents: move focus to repos
  - else: exit split to dashboard (terminal unfocused)

### Row visual states

- **Normal**: round border
- **Selected**: double border + selection marker `▸`
- **Grabbed**: double border + inverse colors + marker `≡`

---

## 4) New Agent / Edit Agent Form

```text
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ New Agent  (repo: llxprt-code)                                                                    │
│                                                                                                    │
│   Name             [Fix ACP socket timeout_]                                                      │
│   Description      [Handle reconnect timeout path]                                                 │
│   Work dir         [/Users/dev/worktrees/llxprt-code/fix-acp-socket-timeout]                     │
│   Profile          [default]                                                                       │
│   Mode             [--yolo]                                                                        │
│   Pass --continue  [x]  (space toggles)                                                           │
│                                                                                                    │
│   Tab/Down next  Shift+Tab/Up prev  Space toggle checkbox  Enter submit  Esc cancel              │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

`Edit Agent` uses the same form layout with title `Edit Agent  (repo: ...)`.

### Behavior

- `n` opens New Agent form.
- `Enter` on selected agent (dashboard list/preview) opens Edit Agent form.
- Field order: Name, Description, Work dir, Profile, Mode, Pass --continue checkbox.
- Name auto-generates work dir slug until work dir is manually edited.
- Empty name blocks submit.
- On submit, agent is created/updated and screen returns to dashboard.

---

## 5) New Repository / Edit Repository Form

```text
╭────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ New Repository                                                                                     │
│                                                                                                    │
│   Name         [llxprt-code_]                                                                      │
│   Base dir     [/Users/dev/projects/llxprt-code]                                                   │
│   Profile      [default]                                                                            │
│                                                                                                    │
│   Tab next field  Shift+Tab prev  Enter submit  Esc cancel                                        │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

`Edit Repository` uses same layout with title `Edit Repository`.

### Behavior

- `N` opens New Repository.
- `Enter` on selected repository (sidebar) opens Edit Repository.
- Empty repository name blocks submit.
- Base directory supports `~` expansion.
- New repo submit adds repo and selects it.

---

## 6) Help Modal

```text
                          ╭──────────────────────────────────────────────────────╮
                          │ Keyboard Shortcuts                                   │
                          │                                                      │
                          │ Navigation                                           │
                          │   ↑ / ↓         Navigate up / down                   │
                          │   ← / →         Switch pane focus                    │
                          │   Enter          Select / confirm                    │
                          │   Esc            Back / close modal                  │
                          │                                                      │
                          │ Pane Focus                                           │
                          │   r              Focus repository sidebar            │
                          │   a              Focus agent list                    │
                          │   t              Focus terminal pane                 │
                          │   m              Return to main view                 │
                          │                                                      │
                          │ Terminal                                             │
                          │   F12            Focus / unfocus terminal input      │
                          │                                                      │
                          │ Views                                                │
                          │   s              Toggle split mode                   │
                          │   /              Search / command palette            │
                          │   ? / h / F1     This help dialog                    │
                          │   q              Quit                                │
                          │                                                      │
                          │   ↑↓ scroll   Esc to close                           │
                          ╰──────────────────────────────────────────────────────╯
```

### Behavior

- Open via `?`, `h`, or `F1`.
- Scroll with `↑/↓` when content exceeds viewport.
- `Esc` closes modal.

---

## 7) Confirm Delete Modals

### Delete Repository

```text
╭──────────────────────────────────────────────────────────────────────╮
│ Delete Repository                                                    │
│                                                                      │
│   Delete repository 'llxprt-code' and all its agents?               │
│                                                                      │
│   [Enter] Confirm   [Esc] Cancel                                    │
╰──────────────────────────────────────────────────────────────────────╯
```

### Delete Agent

```text
╭──────────────────────────────────────────────────────────────────────╮
│ Delete Agent                                                         │
│                                                                      │
│   Delete agent #1872 Fix ACP socket timeout from repo 'llxprt-code'?│
│                                                                      │
│   [x] Also delete working directory:                                 │
│       /Users/dev/worktrees/llxprt-code/fix-acp-socket-timeout       │
│   (Space / d / ↑ / ↓ to toggle)                                      │
│                                                                      │
│   [Enter] Confirm   [Esc] Cancel                                    │
╰──────────────────────────────────────────────────────────────────────╯
```

### Behavior

- `d` in dashboard opens appropriate confirm modal based on focused pane.
- For agent delete, checkbox defaults true and toggles with:
  - `Space`
  - `d`
  - `↑`
  - `↓`
- `Enter` confirms, `Esc` cancels.

---

## 8) Command Palette / Search (Current Toy Behavior)

```text
(Visual layout is currently the same Dashboard component.)
```

### Behavior

- `/` toggles search mode (`Screen::CommandPalette`).
- Search query is captured in app state (`search_query`) while searching.
- Current toy implementation does **not** render a distinct search results list yet.
- Bottom keybind bar changes to command-palette hints:
  - `type to filter  ^/v navigate  enter select  esc close`

So today this is functionally a search state with keyboard behavior and state capture, but without a dedicated visual results panel.

---

## 9) Keybinding Summary (As Implemented)

### Global
- `q` / `Q` quit
- `?` / `h` / `F1` help
- `1` `2` `3` quick theme switch (outside search/forms)

### Dashboard / Main
- `↑/↓` navigate
- `←/→` switch pane
- `r` focus repository sidebar
- `a` focus agent list
- `t` focus terminal
- `F12` terminal focus toggle
- `n` new agent
- `N` new repository
- `d` delete selected (non-split)
- `k` kill agent (non-split)
- `l` relaunch dead agent (non-split)
- `s` toggle split mode
- `/` toggle search mode

### Split Mode
- `a` focus agents pane
- `r` focus repos pane
- `Enter` select (filter / grab toggle)
- `↑/↓` move cursor (or reorder if grabbed)
- `m` return to main + terminal focused
- `Esc` back behavior (ungrab → focus repos → exit split)

### Forms
- `Tab` / `Shift+Tab` next/previous field
- `↑/↓` also moves between fields
- `Backspace` edit field
- `Enter` submit
- `Esc` cancel
- In New/Edit Agent, checkbox toggles with `Space` when focused

---

## 10) Theme Baseline

- `ThemeManager` default active slug: `green-screen`
- Built-in embedded themes: `green-screen`, `dracula`, `default-dark`
- Optional external themes can be loaded via `JEFE_THEME_DIR`
- PTY fallback terminal colors are synchronized from active theme each render cycle

Green-screen default values are used as resilient fallback for unresolved theme colors.
