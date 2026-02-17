# Component 001 Pseudocode — AppState + Event Reducer

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

Requirements: REQ-FUNC-002,003,004,005,006,007,008,010; REQ-TECH-001,002,003,009

```text
01: FUNCTION dispatch(event, state)
02:   MATCH event.type
03:     CASE Navigation -> apply_navigation(event, state)
04:     CASE Focus -> apply_focus(event, state)
05:     CASE FormInput -> apply_form_input(event, state)
06:     CASE FormSubmit -> validate_then_emit_side_effect(event, state)
07:     CASE RuntimeIntent -> emit_runtime_request(event)
08:     CASE RuntimeResult -> apply_runtime_status_transition(event, state)
09:     CASE SplitIntent -> apply_split_rules(event, state)
10:     CASE ThemeIntent -> emit_theme_request(event)
11:     CASE PersistenceResult -> apply_persistence_result(event, state)
12:   RETURN new_state + side_effects

13: FUNCTION apply_focus(event, state)
14:   IF event == ToggleTerminalFocus(F12)
15:     state.terminal_focused = NOT state.terminal_focused
16:   ELSE IF state.terminal_focused
17:     emit TerminalInputForward(event)
18:   ELSE
19:     update pane focus / selection indices with bounds checks

20: FUNCTION validate_then_emit_side_effect(event, state)
21:   IF form == NewAgent AND name empty -> set form error, STOP
22:   IF form == NewRepo AND name empty -> set form error, STOP
23:   IF submit valid -> emit typed command (CreateRepo/CreateAgent/Update/...)
24:   set optimistic UI state only where policy allows

25: FUNCTION apply_split_rules(event, state)
26:   IF entering split -> preserve return context
27:   IF grab toggle -> flip grabbed row state
28:   IF grabbed + move up/down -> swap rows deterministically
29:   IF esc in split -> ungrab OR focus repos OR exit split (ordered)

30: FUNCTION apply_runtime_status_transition(event, state)
31:   map lifecycle outcomes to AgentStatus
32:   ensure Dead remains recoverable for relaunch path
33:   preserve selected context where possible
```
