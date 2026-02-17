# Integration Contract Completeness Matrix

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

This matrix ensures integration contract quality is explicit and verifiable.

| Contract Area | Required Detail | Evidence Artifact | Phases |
|---|---|---|---|
| Existing callers | exact entry points and dispatch paths | plan/00-overview.md | P00, P13, P13A |
| Replaced/removed behavior | old direct coupling paths to remove | hybrid-strategy-compliance-matrix.md | P03, P05, P11 |
| User access path | keyboard/operator flows end-to-end | ui-mockups.md + P13 tests | P10, P11, P13 |
| Data/state migration | startup/load/save migration handling | persistence-matrix.md | P12, P13 |
| Backward-safe behavior | non-destructive fallback and recovery | runtime-lifecycle + persistence matrices | P08, P12, P13 |
| E2E proof | integration and quality gate tests | P13 + P14 artifacts | P13, P14 |

## Integration Completeness Gate

Integration contract is complete only when each row above has:
1. explicit phase ownership,
2. explicit verification phase,
3. observable evidence artifact.
