# Hybrid Strategy Compliance Matrix

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

Objective: enforce the agreed strategy — **reuse toy1 UI composition patterns** while **rebuilding non-UI core layers**.

## Compliance Rules

| Rule ID | Rule | Allowed | Forbidden | Phases |
|---|---|---|---|---|
| HYB-001 | Reuse toy1 UI composition and interaction model | layout structure, pane focus patterns, modal UX, split mode interaction, keybinding mental model | copy-paste of old non-UI orchestration into new architecture | P09, P10, P11 |
| HYB-002 | Rebuild non-UI core layers | new typed domain/state/runtime/persistence/theme boundaries | preserving ad-hoc direct side effects from UI into runtime/filesystem | P03..P08, P12 |
| HYB-003 | Boundary isolation | UI emits typed intents only | UI directly calling tmux process orchestration or persistence I/O | P03, P05, P11, P13 |
| HYB-004 | No architecture forks | evolve canonical modules | creating `*v2` parallel architecture trees as shortcut | all phases |
| HYB-005 | Cross-layer validation | integration tests prove reachability through boundaries | mocked-only integration claims with no user-path coverage | P13, P14 |

## Measurable Checks

1. UI layer imports only event/state interfaces, not runtime/persistence internals.
2. Runtime boundary is invoked through app event handlers, not direct UI actions.
3. Persistence writes originate from persistence boundary only.
4. Theme resolution and fallback are isolated to theme boundary.
5. Integration tests demonstrate user flows traversing boundaries end-to-end.

## Verification Mapping

- Structural checks: P03A, P05A, P09A, P11A
- Semantic checks: P13A, P14A
