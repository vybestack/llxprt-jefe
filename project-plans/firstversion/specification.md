# Jefe Firstversion — Specification

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

## Purpose

Build Jefe v1 as a Rust/iocraft terminal application that preserves the proven interaction model from `toy1` while replacing non-UI internals with cleanly bounded, strongly typed core layers.

## Strategy Contract

1. **Reuse UI patterns from toy1**
   - Screen composition, modal/form UX, keybinding semantics, focus signaling, split mode behavior, and terminal focus/unfocus interaction patterns.
2. **Rebuild non-UI core layers**
   - Domain model, app state/event contracts, runtime orchestration boundary, persistence boundary, and theme loading/fallback boundary.
3. **No architecture fork**
   - Do not create parallel `*v2` code trees; evolve toward the target boundary model directly.

---

## Functional Requirements

### REQ-FUNC-001 Startup & persistence fallback
- Load settings/state from `settings.toml` + `state.json` at startup when present.
- Missing files must fall back to safe defaults.
- Malformed files must produce clear user-visible error and safe fallback behavior.

### REQ-FUNC-002 Dashboard workspace
- Provide dashboard with repository list, agent list, terminal region, selected-agent preview, top status bar, bottom keybind bar.
- Keyboard-first navigation and deterministic selection/focus.

### REQ-FUNC-003 Repository CRUD
- Create/edit/delete repository workflows with explicit confirmation for destructive actions.

### REQ-FUNC-004 Agent CRUD
- Create/edit/delete agent workflows with required fields and delete-workdir confirmation option.
- New-agent default: `--continue` enabled.

### REQ-FUNC-005 Runtime interaction
- Embedded terminal region for selected agent.
- Explicit terminal focus semantics (`F12`):
  - focused -> terminal input forwarding,
  - unfocused -> Jefe navigation shortcuts.

### REQ-FUNC-006 Split mode
- Enter split mode with repository filtering, running-agent list, row selection, reorder workflow, and return behavior.

### REQ-FUNC-007 Lifecycle controls
- Kill selected runtime session.
- Relaunch dead session preserving profile/mode/continue behavior.
- Reflect lifecycle status transitions clearly.

### REQ-FUNC-008 Search/help
- Command/search workflow.
- Help modal with active keybindings.
- Non-destructive reversible behavior.

### REQ-FUNC-009 Theme behavior
- Multiple themes supported.
- **Green Screen** is default and fallback.
- Theme selection persists across restarts.
- No bright/light default palette.

### REQ-FUNC-010 Error handling
- Surface operational errors in context.
- Keep UI interactive after recoverable runtime/persistence failures.

---

## Technical Requirements

### REQ-TECH-001 Architectural boundaries
Enforce layered boundaries:
1. Domain/Data model
2. App state + event/reducer
3. Runtime orchestration (tmux/PTY)
4. Theme/presentation
5. UI composition
6. Persistence

### REQ-TECH-002 Strong typing
Use explicit structs/enums for entities, events, statuses, and contracts. Avoid stringly control logic.

### REQ-TECH-003 Deterministic state transitions
App state mutation must be event-driven and deterministic; side effects occur only through owned boundaries.

### REQ-TECH-004 Runtime isolation
Runtime manager owns attach/reattach, input forwarding, liveness checks, kill/relaunch, and session identity mapping.

### REQ-TECH-005 Persistence contract
- File-based only (`settings.toml`, `state.json`), no SQLite.
- Exact path resolution rules:
  - `settings.toml`: `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` ->
    - macOS: `~/Library/Application Support/jefe/settings.toml`
    - Linux: `${XDG_CONFIG_HOME:-~/.config}/jefe/settings.toml`
    - Windows: `%APPDATA%\jefe\settings.toml`
  - `state.json`: `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` ->
    - macOS: `~/Library/Application Support/jefe/state.json`
    - Linux: `${XDG_STATE_HOME:-~/.local/state}/jefe/state.json`
    - Windows: `%LOCALAPPDATA%\jefe\state.json`
- Version-aware schema validation.
- Atomic writes.
- Safe fallback on malformed/missing files.

### REQ-TECH-006 Traceability
Implementation artifacts include:
- `@plan PLAN-20260216-FIRSTVERSION-V1.PNN`
- `@requirement REQ-*`
- pseudocode line references in implementation phases.

### REQ-TECH-007 Rust quality gates
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- optional coverage gate via `cargo llvm-cov`

### REQ-TECH-008 Anti-placeholder rule
No TODO/FIXME/HACK/placeholder completion in implementation phases.

### REQ-TECH-009 Integration reachability
Every requirement must map to concrete user flows and integration paths.

### REQ-TECH-010 Hybrid consistency
Plan must explicitly encode toy1 UI reuse + core-layer rebuild split, without boundary violations.

---

## Data Contracts and Invariants

- **Repository**: stable identity, unique slug, base directory, profile defaults, owned agent list.
- **Agent**: stable id + display id, name/description, work dir, profile, mode, continue toggle, status, runtime linkage.
- **Runtime binding**: stable session identity, launch signature, liveness metadata, attachment metadata.
- **Theme**: typed palette with guaranteed Green Screen fallback token resolution.
- **Persistence objects**: validated before apply; invalid payload never mutates canonical in-memory state.

---

## Integration Contract

- UI emits typed events to AppState/event layer.
- Event layer invokes runtime/persistence/theme boundaries for side effects.
- Runtime boundary emits status/liveness updates back via typed events.
- Persistence boundary owns serialization/validation/atomicity.
- Theme boundary owns load/resolve/fallback behavior.

---

## Non-Functional Requirements

- Maintainability via strict module ownership.
- Reliability under runtime death/reattach cases.
- Observable operator-facing errors with minimal orchestration diagnostics.
- Responsive keyboard-first UX.
- Green-screen-first visual coherence.
