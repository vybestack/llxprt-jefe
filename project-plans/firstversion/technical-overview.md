# Jefe v1 — Technical Specification

## Purpose

This document defines the architecture and technical contracts for Jefe v1.

It specifies **what the system architecture is and how modules are responsible for behavior**, without prescribing implementation sequencing.

---

## Architectural Goals

Jefe v1 architecture must be:
- modular,
- extensible,
- maintainable,
- testable,
- Rust-idiomatic,
- and resilient under runtime/session failure.

Primary technical focus:
- orchestrating many agent runtime sessions cleanly,
- preserving clear boundaries between UI, state, orchestration, and persistence,
- maintaining strong typing and predictable state transitions.

---

## Architecture Style

Jefe v1 uses a **layered modular architecture** with explicit contracts:

1. **Domain/Data Model Layer**
2. **Application State + Event/Reducer Layer**
3. **Runtime Orchestration Layer (tmux/PTY adapter)**
4. **Theme + Presentation Layer**
5. **UI Composition Layer (iocraft components/screens/modals)**
6. **Persistence Layer (file-based settings/state)**

The architecture is command/event driven at the application boundary and strongly typed internally.

---

## Module Boundaries

## 1) Domain/Data Model Layer

Defines core entities and enums, including:
- `Repository`
- `Agent`
- `AgentStatus`
- output/todo metadata types

Responsibilities:
- canonical data shapes,
- serialization contracts,
- value-level invariants.

Must not:
- perform UI rendering,
- call tmux/PTY directly,
- own terminal event mapping.

## 2) Application State + Event Layer

Defines:
- `AppState` (single source of UI/application truth in process),
- event enum(s) for operator/system actions,
- deterministic state transition handlers.

Responsibilities:
- map events to state transitions,
- maintain selection, modal, split, and form state,
- enforce UX rules for navigation/focus/form submission,
- coordinate lifecycle state updates.

Must not:
- parse terminal bytes,
- render UI directly,
- directly own filesystem persistence format logic.

## 3) Runtime Orchestration Layer

Defines and owns:
- `PtyManager` (runtime session orchestration),
- tmux session identity and lifecycle,
- embedded terminal snapshot generation,
- input forwarding,
- liveness checks,
- kill/relaunch operations.

Responsibilities:
- stable mapping between agent runtime identity and external session,
- reattach semantics when selection changes,
- session safety during teardown/reattach,
- terminal color defaults and rendering snapshots.

Must not:
- mutate business-level app state directly,
- persist settings/state files directly,
- decide UI behavior.

## 4) Theme + Presentation Layer

Defines:
- theme definitions,
- theme loading/selection contracts,
- resolved visual token helpers,
- small presentation format helpers.

Responsibilities:
- Green Screen baseline and fallbacks,
- typed theme model from JSON themes,
- stable color contract for UI consumers.

Must not:
- own application lifecycle logic,
- own runtime control logic.

## 5) UI Composition Layer

Contains:
- components,
- screens,
- modals.

Responsibilities:
- render read-only snapshots of current state,
- expose clear visual focus/selection state,
- keep view logic declarative and presentational.

Must not:
- contain orchestration policy,
- perform persistence I/O,
- define terminal lifecycle behavior.

## 6) Persistence Layer

Defines contracts for:
- loading settings and state,
- validating persisted content,
- writing changes atomically,
- versioning/migration boundaries.

Storage artifacts:
- `settings.toml`
- `state.json`

Path contract (v1):
- `settings.toml`:
  - `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` -> platform default
  - platform default:
    - macOS: `~/Library/Application Support/jefe/settings.toml`
    - Linux: `${XDG_CONFIG_HOME:-~/.config}/jefe/settings.toml`
    - Windows: `%APPDATA%\jefe\settings.toml`
- `state.json`:
  - `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` -> platform default
  - platform default:
    - macOS: `~/Library/Application Support/jefe/state.json`
    - Linux: `${XDG_STATE_HOME:-~/.local/state}/jefe/state.json`
    - Windows: `%LOCALAPPDATA%\jefe\state.json`

Both directories must be created on-demand with safe permissions before first write.

**SQLite is explicitly out of architecture scope for v1.**

---

## Data Contracts

## Repository Contract

Required fields include:
- name,
- slug,
- base directory,
- defaults/profile metadata,
- collection of agents.

## Agent Contract

Required fields include:
- stable id,
- display id,
- name/description,
- work dir,
- profile,
- mode,
- optional PTY slot/session link,
- status,
- temporal/runtime metrics,
- preview metadata.

## Runtime Session Contract

Runtime linkage must preserve:
- stable session identity,
- launch context (work dir/profile/mode),
- current liveness,
- current attachment state.

## Theme Contract

Theme definitions are JSON-backed with typed fallback behavior.

Contracts:
- default theme slug: `green-screen`,
- fallback to green-screen-compatible colors when missing/invalid,
- user overrides allowed via settings and optional external theme directory.

---

## Event Model and Control Flow Contracts

Jefe uses explicit typed events to separate intent from state mutation.

Event categories:
- navigation/focus events,
- modal/form events,
- lifecycle actions (kill/relaunch),
- terminal-focus toggle events,
- search/help events,
- system-driven state refresh events (e.g., liveness updates).

Control-flow contracts:
1. UI input becomes typed events.
2. `AppState` applies deterministic transitions.
3. Side-effecting operations (runtime/persistence) execute through their owning module contracts.
4. Resulting state changes are reflected in render snapshots.

No hidden side effects are allowed inside pure presentation components.

---

## Runtime Orchestration Contracts (tmux/PTY)

Jefe runtime architecture is a managed tmux/PTY orchestration model:

- One runtime session per agent identity.
- A single attached viewer can switch between agent sessions.
- Reattach must include safe teardown of previous viewer resources before spawning replacement.
- Input forwarding occurs only when terminal is focused.
- Liveness checks must support both attached and non-attached sessions.
- Relaunch recreates runtime using persisted launch profile/mode contract.

Failure behavior contract:
- runtime/session failures must be surfaced without crashing UI process,
- dead sessions must remain recoverable from operator controls.

---

## Persistence Contracts

Persistence is local and file-based.

## `settings.toml`

Stores user/system preferences such as:
- active theme,
- display/runtime preferences,
- default operator settings.

## `state.json`

Stores operational state such as:
- repositories/agents,
- selection and UI context,
- runtime linkage metadata,
- other restart-relevant session state.

Required persistence properties:
- explicit schema versioning,
- parse/validation before apply,
- atomic writes,
- recoverable fallback behavior when files are missing or malformed.

No relational database contract is present in v1.

---

## Extensibility Contracts

Jefe v1 must remain extensible for optional richer sideband integrations (e.g., ACP), without restructuring core boundaries.

Extensibility requirements:
- runtime adapter boundary remains swappable/expandable,
- state/event contracts remain stable under additional status sources,
- UI can incorporate richer metadata streams without collapsing architecture boundaries,
- persistence formats remain versioned and forward-compatible.

---

## Non-Functional Requirements

## Maintainability
- Clear module ownership and strict boundary discipline.
- Low coupling between UI rendering and runtime orchestration.
- Strong typing and explicit enums for lifecycle/focus/status.

## Reliability
- No silent corruption of session-to-agent mapping.
- No cross-agent side effects from single-agent control actions.
- Predictable behavior under runtime process death.

## Observability
- Minimal but actionable local diagnostics for orchestration and persistence failures.
- Detailed execution logs remain a concern of `llxprt` itself.

## Performance
- Interactive responsiveness in keyboard-driven workflows.
- Efficient terminal snapshot rendering for active session views.
- Persistence overhead must not block normal interaction patterns.

## UX Coherence
- Terminal focus/unfocus semantics must remain explicit.
- Green-screen-first visual baseline must remain stable and legible.

---

## Architecture Compliance Criteria

Jefe v1 architecture is compliant when:
1. Module responsibilities match the boundaries above.
2. Runtime orchestration is isolated behind the PTY/tmux layer contract.
3. State transitions are deterministic and event-driven.
4. UI components are presentational and side-effect free.
5. Persistence uses only `settings.toml` and `state.json` with validation/versioning/atomic writes.
6. Green Screen remains the default and fallback theme baseline.
