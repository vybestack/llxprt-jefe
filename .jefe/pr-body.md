## Summary

Makes mouse-drag text selection + OSC 52 clipboard copy work on **every text-bearing surface** in the app, closing the gap where forms, chooser overlays, and confirm modals were silently non-selectable.

Closes #178.

## What was broken

Jefe already had a complete selection/copy stack (mouse routing, geometry hit-testing, content projections, OSC 52 writer), but these surfaces had no `SelectablePane` variant and no `pane_content_lines` projection, so mouse selection silently did nothing there:

- Agent definition form (NewAgentForm / EditAgent)
- Repository definition form (NewRepositoryForm / EditRepository)
- Agent chooser overlay
- Merge chooser overlay
- Confirm modal (delete agent/repo, kill agent, preflight, dirty copy)
- Help modal (was a variant but had an empty content projection — never actually worked)

## What changed

### New SelectablePane variants (`src/selection/text.rs`)
Added `NewAgentForm`, `NewRepositoryForm`, `AgentChooser`, `MergeChooser`, `ConfirmModal` to the enum.

### Overlay geometry + z-order (`src/selection/geometry.rs`)
- `pane_at` now checks for active overlays **before** the base layout.
- Full-screen modals (forms, help, confirm) check containment against their actual rendered bounds — points outside the modal return `None` (the modal replaced the screen, so there's no base pane to fall through to).
- In-screen overlays (choosers) check containment first; if the click misses the chooser, it falls through to the base screen layout (correct z-order).
- `OverlayPane` enum and `ScreenLayout.overlay` field carry overlay state through the pure selection layer (no iocraft types).

### Content projections (`src/selection/content.rs` + new modules)
- `src/selection/form_content.rs` — projections for agent and repository forms. Matches the exact rendered text including the caret character inserted into focused editable fields.
- `src/selection/overlay_content.rs` — projections for agent chooser, merge chooser, and all five confirm modal variants (ConfirmDeleteAgent, ConfirmDeleteRepository, ConfirmKillAgent, PreflightPrompt, ConfirmIssueDirtyCopy).
- Help modal projection now returns the actual help content lines (title + help_content_lines) instead of an empty Vec.

### PTY forwarding suppression (`src/mouse_routing.rs`)
- When any overlay is active, PTY mouse forwarding is suppressed so selection targets the top-most overlay instead of the terminal underneath.
- Help modal scroll offset is now read from AppState (mirrored from the app-shell hook state) so `scroll_offset_for_pane` and `effective_scroll_for_detail` correctly map screen coordinates to help content lines.

### Help scroll offset (`src/state/types.rs`, `src/app_input/mod.rs`)
- Added `help_scroll_offset: usize` to AppState.
- `handle_mode_help_key` mirrors the hook state to AppState at the end of each scroll operation.
- `effective_scroll_for_detail` suppresses scroll for the HelpModal title rows (first 2 content lines), matching how detail pane header rows work.

## Verification

- `cargo fmt --all --check` — pass
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` — pass (zero warnings)
- `cargo build --workspace --all-features --locked` — pass
- `cargo test --workspace --all-features --locked` — pass (1387 tests, 0 failures)
- All content projections verified against the actual iocraft component render code to ensure copied text matches what the user sees.

## Architecture

Follows the existing pattern and project standards:
- Pure selection/geometry/content logic stays in `src/selection/` (no iocraft types).
- View code stays render-only.
- The overlay state is carried through `ScreenLayout` (a plain-data descriptor), keeping `pane_at` pure and testable.
- The "is this pane selectable + how do I get its lines" contract is driven from one place (the enum + `pane_content_lines` mapping).
