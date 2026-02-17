# F12 Cross-View Consistency Matrix

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

F12 semantics must be explicit and consistent across all supported views.

| View/Context | F12 Pressed While Unfocused | F12 Pressed While Focused | Other Key Routing | Expected Indicator |
|---|---|---|---|---|
| Dashboard terminal pane | enter terminal-focused mode | return to app-focused mode | focused -> PTY; unfocused -> app navigation | border/style + status hint text |
| Split mode | if terminal subview active, same toggle semantics | same | never ambiguous key ownership | visible focus marker |
| Modal open | F12 should not steal modal focus unless explicitly allowed | same rule | modal keys remain modal-owned | modal remains authoritative |
| Form input active | same as modal rule | same | form field editing keys remain form-owned | form focus indicator stable |
| Help modal | F12 ignored or non-destructive (policy-defined) | same | help navigation unaffected | help overlay unchanged |
| Search/command mode | F12 does not break search input semantics | same | search input owns typing unless terminal focused by explicit transition | search mode hint stable |

## Required Policy

1. F12 is the only global terminal-focus escape/toggle key.
2. Non-terminal contexts must define whether F12 is ignored or routed through a guarded transition.
3. Key routing ownership must always be unambiguous and visible.

## Verification Mapping

- P07 runtime TDD: core routing and session-focused edge behavior.
- P10 UI TDD: add context-specific behavior tests.
- P11 UI implementation: enforce consistent routing.
- P13 integration hardening: verify routing ownership remains deterministic through cross-layer flows.
- P14 quality gate: no regression across contexts.

## Canonical Acceptance Checklist

- F12-001: Dashboard focused/unfocused semantics PASS.
- F12-002: Split-mode semantics PASS.
- F12-003: Modal/form/help/search contexts PASS with explicit policy behavior.
- F12-004: No ambiguous key ownership in any supported view.
