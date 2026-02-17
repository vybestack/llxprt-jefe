# Search and Help Acceptance Contract (v1)

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

## Scope

This contract defines minimum acceptable v1 behavior for help and search workflows.

## Help Contract

1. Open via `?`, `h`, or `F1`.
2. Displays current active keybindings and navigation hints.
3. Is non-destructive and reversible with `Esc`.
4. Supports scrolling when content exceeds viewport.

## Search Contract (v1 minimum)

1. Open/close via `/`.
2. Captures query in app state.
3. Provides deterministic keyboard behavior while active.
4. Closing search returns to previous non-destructive state.
5. Search must render a visible filtered result list (commands and/or entities) while active.
6. Search must support deterministic selection behavior:
   - Up/Down changes highlighted result,
   - Enter applies highlighted result action,
   - Esc cancels search and restores prior view context.
7. If no results match, UI must render explicit "no results" state without mutating core state.

## Acceptance Criteria

- HELP-001: Help open/close behavior verified across contexts.
- HELP-002: Keybinding list rendered and scrollable.
- SRCH-001: Search mode toggles and query state persists during mode.
- SRCH-002: Search mode exits safely with no destructive state mutation.
- SRCH-003: Search key routing does not conflict with terminal focus semantics.
- SRCH-004: Result list is rendered and updates as query changes.
- SRCH-005: Up/Down/Enter/Esc behavior is deterministic and test-covered.
- SRCH-006: No-results behavior is explicit and non-destructive.

## Verification Mapping

- P10 (TDD): HELP-*/SRCH-* tests authored.
- P11 (impl): behaviors implemented.
- P14 (quality gate): no regressions.
