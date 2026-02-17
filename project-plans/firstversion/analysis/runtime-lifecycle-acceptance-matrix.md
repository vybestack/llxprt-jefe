# Runtime Lifecycle Acceptance Matrix

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

This matrix defines mandatory runtime/session transition acceptance behavior.

| Scenario ID | Initial State | Trigger | Expected Transition | Required Assertions | Phases |
|---|---|---|---|---|---|
| RT-001 | No session | launch selected agent | NoSession -> Waiting/Running | session created, launch signature stored | P07, P08, P08A |
| RT-002 | Running | kill action (`k`) | Running -> Dead | runtime terminated, status updated, UI indicates Dead | P07, P08, P08A |
| RT-002A | Dead | kill action (`k`) again | Dead -> Dead | operation is idempotent, no corruption, user feedback remains coherent | P07, P08, P08A |
| RT-003 | Dead | relaunch action (`l`) | Dead -> Waiting/Running | relaunch uses preserved profile/mode/continue | P07, P08, P08A |
| RT-003A | Running | relaunch action (`l`) | Running -> Running (guarded no-op or explicit restart policy) | deterministic behavior + clear feedback | P07, P08, P08A |
| RT-004 | Attached session A | select agent B + terminal focus | attached(A) -> attached(B) | safe detach/reattach semantics, no crash | P07, P08, P13 |
| RT-005 | Terminal unfocused | normal keys | state unchanged | keys handled by Jefe navigation, not PTY | P07, P10, P11 |
| RT-006 | Terminal focused | key press | PTY input forwarded | keys routed to PTY channel except escape toggle | P07, P08, P11 |
| RT-007 | Runtime failure | liveness refresh tick | Running -> Errored/Dead | non-fatal UI behavior, recoverable relaunch path | P08, P13 |
| RT-008 | Persisted runtime metadata exists | app restart | metadata restored to known state | stale sessions handled safely, no invalid attach | P12, P13, P14 |

## Required Test Coverage

- Unit-level runtime transition tests (`tests/runtime/*`)
- Integration-level recovery and operator flow tests (`tests/integration/*`)

## Acceptance Gate

All `RT-*` scenarios must be covered and PASS before P14A final PASS decision.
