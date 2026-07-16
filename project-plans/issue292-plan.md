# Issue #292 — Errors should have a dedicated component or panel

## Issue summary

Right now errors get printed above the selection panel, the layout shifts for
a second, then it goes away. Instead:

1. That an error happened should go to the title bar (where the WARN: ssh agent
   socket currently shows) — show the last error there.
2. A dedicated errors panel showing the last N errors.
3. Works like issues/PRs/actions — list by title, detail on selection.
4. Eager-loaded since errors are local, not remote.

## Acceptance matrix

| # | Actor / path | Input | Observable success | Failure / diagnostic | Test |
|---|---|---|---|---|---|
| A1 | Any code path that sets `error_message` | error string | `ErrorEntry` pushed to `errors_state` store, capped at N (50) | — | unit: push_error + cap |
| A2 | Any code path that sets per-mode `error` (issues/prs/actions) | error string | `ErrorEntry` pushed to `errors_state` store | — | unit: per-mode error → store |
| A3 | Status bar render | state with ≥1 error | center shows last error title (truncated) | — | unit + harness |
| A4 | Status bar render | state with 0 errors | center shows normal repo/running stats | — | unit |
| A5 | Status bar render | state with warning only (ssh) | center shows WARN (existing behavior) | — | existing tests |
| A6 | Dashboard key `e`/`E` | Dashboard mode | Enters errors mode (ScreenMode::DashboardErrors) | — | unit: key → event |
| A7 | Enter errors mode | from dashboard | screen_mode set, focus=list, errors_state.active=true, prior focus saved | — | unit: state transition |
| A8 | Errors mode render | ≥1 error | List pane shows error titles, detail pane shows selected error detail | — | harness scenario |
| A9 | Errors mode navigation | up/down in list | selected index moves, detail follows | — | unit |
| A10 | Errors mode Esc | any focus | exits to dashboard, prior focus restored | — | unit |
| A11 | Errors mode keybind bar | — | shows errors-mode hints | — | unit |

## Non-goals

- No persistence of error store (runtime-only, like issues/prs state).
- No filtering or search for errors (all errors shown).
- No mutation/editing of errors (read-only).
- No warning capture (only errors, per the issue wording).
- No replacing per-mode error banners in the existing issues/PRs/actions
  screens (those are per-mode errors; the global store is additive).
- No remote fetching (errors are local).

## Vertical slices

1. **Domain + State**: `ErrorEntry`, `ErrorStore`, `ErrorsState`, error capture
   in the reducer's error-setting transitions.
2. **Events + Messages**: `EnterErrorsMode`/`ExitErrorsMode`/navigation events,
   `ErrorsMessage` conversion.
3. **UI**: `ErrorsScreen` component, `ScreenMode::DashboardErrors`, keybind hint.
4. **Title bar**: last-error display in the status bar.
5. **Input**: `e`/`E` key from Dashboard to enter errors mode, Esc to exit.

## Architecture layers

- `src/domain/errors.rs` — ErrorEntry domain type
- `src/state/errors_types.rs` — ErrorsState
- `src/state/types.rs` — add errors_state field, ScreenMode variant
- `src/state/errors_ops.rs` — reducer operations
- `src/state/events.rs` — new events
- `src/state/mod.rs` — wire up apply, error capture
- `src/messages.rs` + `src/messages/errors_conversion.rs` — ErrorsMessage
- `src/ui/screens/errors.rs` — ErrorsScreen
- `src/ui/components/status_bar.rs` — last-error display
- `src/ui/components/keybind_bar.rs` — errors-mode hints
- `src/ui/orchestration.rs` — screen dispatch
- `src/app_input/normal.rs` — `e`/`E` key handler
- `src/selection/` — pane geometry for errors mode

## Scope ledger

(tracked during implementation)

## Review counters

- OCR pre-PR: 0/2
- OCR post-PR: 0/2

## Verification evidence

(to be filled during implementation)
