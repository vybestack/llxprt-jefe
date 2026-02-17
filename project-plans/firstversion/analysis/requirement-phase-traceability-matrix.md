# Requirement-to-Phase Traceability Matrix

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

This matrix is the authoritative requirement coverage map for planning and verification.

| Requirement | Requirement Summary | Primary Phases | Verification Phases | Primary Evidence Artifacts |
|---|---|---|---|---|
| REQ-FUNC-001 | Startup persistence load/fallback | P05, P12 | P05A, P12A, P14A | persistence-matrix.md, persistence tests |
| REQ-FUNC-002 | Dashboard workspace behavior | P09, P11 | P09A, P11A, P14A | ui-adaptation tests + dashboard contracts |
| REQ-FUNC-003 | Repository CRUD | P10, P11 | P10A, P11A, P14A | crud-validation-error-matrix.md |
| REQ-FUNC-004 | Agent CRUD | P10, P11 | P10A, P11A, P14A | crud-validation-error-matrix.md |
| REQ-FUNC-005 | Terminal interaction + F12 semantics | P07, P08, P10, P11 | P07A, P08A, P10A, P11A, P14A | f12-cross-view-consistency-matrix.md |
| REQ-FUNC-006 | Split mode behavior | P10, P11 | P10A, P11A, P14A | split mode tests and UI contracts |
| REQ-FUNC-007 | Kill/relaunch lifecycle controls | P07, P08 | P07A, P08A, P13A, P14A | runtime-lifecycle-acceptance-matrix.md |
| REQ-FUNC-008 | Search/help behavior | P10, P11 | P10A, P11A, P14A | search-help-acceptance-contract.md |
| REQ-FUNC-009 | Theme behavior, green-screen fallback | P05, P12 | P05A, P12A, P14A | theme-precedence-fallback-policy.md |
| REQ-FUNC-010 | Error visibility and recoverability | P05, P08, P11, P13 | P05A, P08A, P11A, P13A, P14A | integration-contract-completeness-matrix.md |
| REQ-TECH-001 | Layered architecture boundaries | P03, P05 | P03A, P05A, P13A, P14A | hybrid-strategy-compliance-matrix.md |
| REQ-TECH-002 | Strong typing | P03, P04, P05 | P03A, P04A, P05A, P14A | core domain/state tests |
| REQ-TECH-003 | Deterministic state transitions | P04, P05, P11 | P04A, P05A, P11A, P14A | reducer behavior tests |
| REQ-TECH-004 | Runtime isolation | P06, P08 | P06A, P08A, P13A, P14A | runtime boundary tests |
| REQ-TECH-005 | Persistence contract, no SQLite | P05, P12 | P05A, P12A, P14A | persistence-matrix.md |
| REQ-TECH-006 | Plan/code traceability | P01, P02, all impl phases | all `*A` phases | this matrix + pseudocode references |
| REQ-TECH-007 | Rust quality gates | all impl/test phases | all `*A` phases + P14 | command outputs in phase markers |
| REQ-TECH-008 | Anti-placeholder enforcement | all impl phases | all `*A` phases + P14 | grep checks in phase docs |
| REQ-TECH-009 | Integration reachability | P01, P10, P11, P13 | P01A, P10A, P11A, P13A, P14A | integration-contract-completeness-matrix.md |
| REQ-TECH-010 | Hybrid consistency (toy1 UI reuse + core rebuild) | P01, P03, P09, P11 | P01A, P03A, P09A, P11A, P14A | hybrid-strategy-compliance-matrix.md |

## Release Readiness Rule

A requirement is release-ready only when both conditions are true:
1. Its primary phase(s) are completed successfully.
2. Its mapped verification phase(s) are completed with semantic PASS.
