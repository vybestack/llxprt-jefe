# Jefe TUI - Data Layer, Event Bus, and App State

## Overview

This document describes the data layer, event bus, and application state for the Jefe TUI application. All components have been built, tested, and are ready for UI integration.

## Components Built

### 1. Data Models (`src/data/models.rs`)

Core data structures with full documentation and tests:

- **`AgentStatus`** - Execution status (Running, Completed, Errored, Waiting, Paused, Queued)
- **`TodoStatus`** - Todo completion status (Pending, InProgress, Completed)
- **`TodoItem`** - Individual todo with content and status
- **`Agent`** - AI agent instance with model, profile, mode, status, tokens, and cost
- **`OutputKind`** - Type of output (Text, ToolCall)
- **`ToolStatus`** - Tool execution status (InProgress, Completed, Failed)
- **`OutputLine`** - Single line of agent output
- **`Task`** - Task with agent, todos, and output
- **`Project`** - Project containing multiple tasks

All models include:
- Clone, Debug, Serialize, Deserialize derives
- Comprehensive doc comments
- Unit tests

### 2. Mock Data Generator (`src/data/mock.rs`)

Generates realistic fake data for development with 4 projects:

1. **llxprt-code** (3 tasks)
   - #1872 Fix ACP socket timeout (Running, 42min, 5 todos)
   - #1899 Refactor prompt handler (Running, 1hr15min, 6 todos)
   - #1905 Add retry on 429 (Completed, 28min)

2. **starflight-tls** (2 tasks)
   - #42 TLS renegotiation fix (Running, 18min)
   - #38 Cert rotation handler (Completed, 45min)

3. **gable-work** (1 task)
   - #156 API migration v3 (Running, 2hr)

4. **mariadb-cli** (0 tasks)

### 3. Event Bus (`src/events/bus.rs`)

Simple synchronous event bus with `AppEvent` enum supporting:

- Navigation (Up, Down, Left, Right)
- Actions (Select, Back, NewTask, DeleteTask)
- UI (OpenSearch, OpenHelp, OpenTerminal, ViewLogs, CycleTheme)
- Agent control (SendPrompt, PauseAgent, KillAgent)
- Text input (Char)

Includes:
- `from_key()` - Convert char to event
- `is_quit()`, `is_navigation()`, `is_char()` - Helper methods
- Comprehensive tests

### 4. App State (`src/app.rs`)

Central application state with:

**Enums:**
- `ActivePane` - Sidebar, TaskList, Preview
- `Screen` - Dashboard, TaskDetail, CommandPalette
- `ModalState` - None, NewTask, ConfirmKill, Help

**AppState struct:**
- Projects collection
- Selected project/task indices
- Active pane, screen, modal state
- Search query and state

**Methods:**
- `new()` - Initialize with projects
- `current_project()` - Get selected project
- `current_task()` - Get selected task
- `task_count()` - Total tasks across projects
- `running_count()` - Count of running tasks
- `handle_event()` - Process user events
- Navigation methods (up, down, left, right)
- UI state management methods

## Test Coverage

[OK] **42 tests passing**

Coverage includes:
- All data models
- Mock data generation
- Event handling
- App state navigation
- UI state transitions
- Project/task selection

## Usage

```rust
use jefe_toy1::app::AppState;
use jefe_toy1::data::mock::generate_mock_data;
use jefe_toy1::events::AppEvent;

// Initialize with mock data
let projects = generate_mock_data();
let mut app_state = AppState::new(projects);

// Navigate
app_state.handle_event(AppEvent::NavigateDown);
app_state.handle_event(AppEvent::Select);

// Get current context
let project = app_state.current_project();
let task = app_state.current_task();
let running = app_state.running_count();
```

## Next Steps

With the data layer complete, the next phase is to build:

1. **Presenter Layer** - Format data for display (elapsed time, status icons, etc.)
2. **Theme System** - Color schemes and styling
3. **UI Components** - Sidebar, task list, preview pane, status bar
4. **Screens** - Dashboard, task detail, command palette
5. **Modals** - Help, new task, confirmations

## Notes

- All data is mock/fake for prototype development
- Dead code warnings are expected until UI is built
- Event handlers are stubbed but structure is complete
- State management is fully functional and tested
