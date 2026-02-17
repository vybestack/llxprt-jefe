# Jefe v1 — Functional Specification

## Purpose

Jefe is a terminal application for managing multiple `llxprt` coding agents across multiple repositories from a single interface.

Jefe v1 exists to replace manual multi-terminal/tab workflows with one consistent control surface for:
- creating and configuring agents,
- launching and interacting with their terminal sessions,
- monitoring status,
- recovering dead sessions,
- and persisting operator preferences/state between runs.

This document defines **what Jefe v1 must do**. It does not define implementation steps.

---

## Product Scope

### In Scope

- Repository and agent management
- Embedded terminal interaction through managed runtime sessions
- Split-mode operator workflow
- Agent lifecycle controls (kill/relaunch dead)
- Theme selection with **Green Screen as default**
- Persistent local settings and state using files
- Local single-user operation

### Out of Scope

- Cloud synchronization
- Team/multi-user permissions
- Remote orchestration service
- Built-in issue tracker integration
- SQLite/database-backed persistence
- Deep execution logging in Jefe (detailed logs live in `llxprt`)

---

## Core Concepts

## Repository

A named codebase container in Jefe.

A repository includes:
- display name,
- slug,
- base directory,
- default profile,
- zero or more agents.

## Agent

The primary work unit in Jefe.

An agent includes:
- stable identifier,
- display id,
- name and description,
- working directory,
- profile,
- mode/flags,
- optional `--continue` behavior,
- lifecycle status,
- runtime session binding,
- runtime metrics and preview data.

## Runtime Session

A terminal runtime bound to an agent.

Jefe manages runtime sessions and allows the user to interact with the selected agent in an embedded terminal view.

---

## Functional Requirements

## 1) Startup and Persistence

1. Jefe must start with persisted settings/state when available.
2. Jefe must run with safe defaults when persistence files are missing.
3. Jefe must tolerate malformed persistence files with user-visible error feedback and safe fallback behavior.
4. Jefe must persist user-impacting changes during normal operation.

## 2) Main Workspace

1. Jefe must provide a primary dashboard view with:
   - repository list,
   - agent list,
   - selected-agent preview.
2. The dashboard must show selected context clearly.
3. Navigation must be keyboard-first and deterministic.
4. A keybind/status strip must always expose active controls.

## 3) Repository Management

1. Users must be able to create repositories.
2. Users must be able to edit repository metadata.
3. Users must be able to delete repositories through explicit confirmation.
4. Repository deletion must affect associated agents according to confirmation behavior.

## 4) Agent Management

1. Users must be able to create agents from a form.
2. Users must be able to edit agent configuration from a form.
3. Agent form fields must include:
   - name,
   - description,
   - work directory,
   - profile,
   - mode flags,
   - "pass --continue" toggle.
4. "pass --continue" must default to **enabled** for new agents.
5. Users must be able to delete agents through explicit confirmation.
6. Agent deletion confirmation must include whether to also delete the agent work directory.

## 5) Runtime Interaction

1. Jefe must provide an embedded terminal region for the selected agent.
2. Terminal input must use explicit focus semantics:
   - focused: keys go to terminal,
   - unfocused: keys control Jefe.
3. Focus/unfocus behavior must be clearly indicated in the UI.
4. The focus toggle key must behave consistently in all supported views.

## 6) Split Mode

1. Jefe must provide split mode for rapid multi-agent operational control.
2. Split mode must support:
   - repository filtering,
   - running-agent list focus,
   - row selection,
   - reorder workflow,
   - return to main view while preserving user context.

## 7) Lifecycle Controls

1. Users must be able to kill the selected agent runtime session.
2. Users must be able to relaunch dead agents.
3. Relaunch must preserve the agent’s configured runtime profile/mode behavior.
4. Agent status transitions must be reflected in the UI.
5. Dead status must be distinguishable from running/paused/waiting/completed states.

## 8) Search and Help

1. Jefe must provide a command palette/search workflow.
2. Jefe must provide an in-app help modal with current keybindings.
3. Help and search must be non-destructive and reversible.

## 9) Theme Behavior

1. Jefe must support multiple themes.
2. The default theme must be **Green Screen**.
3. Green Screen must be used as fallback when theme resolution fails.
4. Theme selection must persist across restarts.
5. Jefe must not default to bright/light palettes.

## 10) User Feedback and Error Handling

1. Operational errors must be surfaced clearly in context (form, modal, status area).
2. Jefe must remain interactive after recoverable runtime/persistence failures.
3. Jefe should only emit minimal orchestration diagnostics; deep tool/runtime logs are expected from `llxprt`.

---

## Persistence Requirements (Functional)

Jefe v1 persistence is file-based and local.

Jefe must persist at least:
- repositories and agents,
- selected theme,
- relevant UI/session context,
- agent runtime metadata required for restart continuity.

Persistence must use:
- `settings.toml` for user preferences/configuration,
- `state.json` for operational state.

Path contract (v1):

- `settings.toml` path resolution precedence:
  1. `JEFE_SETTINGS_PATH` (absolute file path override)
  2. `JEFE_CONFIG_DIR/settings.toml`
  3. platform default:
     - macOS: `~/Library/Application Support/jefe/settings.toml`
     - Linux: `${XDG_CONFIG_HOME:-~/.config}/jefe/settings.toml`
     - Windows: `%APPDATA%\jefe\settings.toml`

- `state.json` path resolution precedence:
  1. `JEFE_STATE_PATH` (absolute file path override)
  2. `JEFE_STATE_DIR/state.json`
  3. platform default:
     - macOS: `~/Library/Application Support/jefe/state.json`
     - Linux: `${XDG_STATE_HOME:-~/.local/state}/jefe/state.json`
     - Windows: `%LOCALAPPDATA%\jefe\state.json`

---

## Functional Acceptance Criteria

Jefe v1 is functionally complete when all of the following are true:

1. Repository and agent CRUD workflows are available and reliable from keyboard-driven UI.
2. Embedded terminal focus/unfocus behavior is explicit, consistent, and usable.
3. Split mode supports filtering, selection, and return-to-main without disorienting state loss.
4. Kill and relaunch workflows work per selected agent, including dead-agent recovery.
5. Green Screen is the default and remains the fallback if user theme is unavailable.
6. Settings/state persist between launches using `settings.toml` and `state.json`.
7. No SQLite dependency is required for persistence.
