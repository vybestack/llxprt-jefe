# Issue 652: Preserve LLxprt sandbox settings when saving agents on macOS

Issue: https://github.com/vybestack/llxprt-jefe/issues/652

## Summary

On macOS, enabling Sandbox in the LLxprt agent form does not survive Save. Reopening the agent shows Sandbox disabled, so Jefe cannot launch that agent with the requested sandbox configuration.

## Regression analysis

The definition-driven agent cutover in PR 501 moved persisted launch configuration into typed definition values, but the shipped LLxprt definition does not declare sandbox_enabled, sandbox_engine, or sandbox_flags. The creation service only copies form values for fields declared by the active definition, so all three sandbox form inputs are dropped on save.

The Edit Agent projection independently hard-codes sandboxing to disabled with the default engine and flags rather than reading typed agent values. The LLxprt definition also lacks emitters for the sandbox CLI options and SANDBOX_FLAGS environment variable, so manually migrated values cannot fully affect a launch.

## Acceptance matrix

| ID | Actor / path | Input and target | Observable success | Failure behavior / side effects | Persistence and compatibility | Proof |
|---|---|---|---|---|---|---|
| AC1 | User creates a local LLxprt agent on macOS | Sandbox enabled; supported engine selected; flags entered | Saved agent retains enabled state, canonical engine, and flags; launch planning emits the sandbox option, engine option, and raw SANDBOX_FLAGS value | Invalid typed values fail before runtime launch effects | Values survive durable-state projection and reload; existing schema-2 values remain readable | Reducer/service behavioral test plus definition-driven launch-plan test |
| AC2 | User edits an existing sandbox-enabled LLxprt agent | Open Edit Agent without changing sandbox fields, then Save | Form initially reflects the persisted enabled state, engine, and flags; save preserves them | Missing optional legacy values project to current unsandboxed defaults without mutating the agent merely by opening the form | Migrated sandbox values remain editable and are not reset | Form projection and submit regression test |
| AC3 | User saves an unsandboxed LLxprt agent | Sandbox disabled | Agent remains unsandboxed and launch emits no sandbox option, engine option, or SANDBOX_FLAGS | No sandbox preflight or runtime side effects | Current default remains unchanged | Negative launch-plan test |
| AC4 | Unsupported/non-LLxprt agent path | Sandbox values absent or stale | Other agent definitions do not gain LLxprt sandbox behavior | No unrelated definition or launch behavior changes | Existing typed values are preserved according to current generic persistence rules | Existing definition and cross-agent suites |

## Non-goals

- Changing which sandbox engines macOS or Linux supports.
- Adding a new sandbox engine or runtime.
- Redesigning the New/Edit Agent form.
- Changing LLxprt sandbox semantics beyond restoring the existing Sandbox, Sandbox Engine, and Sandbox Flags controls.
- Changing dependencies, persistence schema version, workflow tooling, or quality gates.

## Planned vertical slices

### Slice 1: Typed save and edit projection

- Acceptance rows: AC1, AC2, AC3, AC4.
- Owners: shipped definition metadata, agent-form state projection, and the existing canonical creation/update boundary.
- Allowed paths: src/domain/agent_definition/shipped/llxprt.rs, src/services/mod.rs, src/services/tests.rs, src/state/form_build.rs, src/state/modal_ops.rs, and focused state tests.
- RED: service and reducer tests show enabled/engine/flags are discarded and Edit Agent reopens disabled.
- GREEN: LLxprt declares all three typed fields; New Agent and Edit Agent save and reload their values; disabled remains the default; other agent definitions remain unchanged.
- Non-goals: no new form framework or persistence schema.
- Focused verification: targeted Rust tests, cargo fmt, cargo xtask quick.
- Stop condition: a new public abstraction, schema change, or fourth ownership layer is required.

### Slice 2: Effective definition-driven launch

- Acceptance rows: AC1, AC3, AC4.
- Owners: shipped LLxprt definition and pure local launch planner.
- Allowed paths: src/domain/agent_definition/shipped/llxprt.rs and tests/agent_local_plan.rs, with a helper change only if the existing closed emitter model cannot express the accepted behavior.
- RED: enabled typed values do not emit the LLxprt sandbox argv/env contract.
- GREEN: enabled values emit --sandbox, --sandbox-engine with the canonical value, and raw SANDBOX_FLAGS; disabled values emit none of them.
- Non-goals: no launch planner redesign and no changes to sandbox preflight ordering.
- Focused verification: agent_local_plan tests, cargo xtask quick.
- Stop condition: conditional behavior requires changing the generic emitter schema or another orchestration route.

### Slice 3: UI scenario and compatibility verification

- Acceptance rows: AC1, AC2, AC3.
- Owners: deterministic TUI harness and generic durable projection tests.
- Allowed paths: dev-docs/tmux-scenarios/issue652/, a focused issue652 behavior test, and existing persistence tests if needed.
- RED: scenario saves an enabled LLxprt sandbox and Edit Agent shows it disabled.
- GREEN: scenario observes enabled sandbox after save/reopen; typed values survive state projection/restore.
- Non-goals: no visual redesign.
- Verification: tmux harness scenario, full make ci-check, OCR, rustreviewer, and exact-head checks.
- Stop condition: fixture changes outside issue652 or quality-tool changes are required.

## Expected paths by ownership layer

- Definition/domain: src/domain/agent_definition/shipped/llxprt.rs and focused definition/plan tests.
- Service/state projection: src/services/mod.rs, src/services/tests.rs, src/state/form_build.rs, src/state/modal_ops.rs, and focused state tests.
- UI behavior: dev-docs/tmux-scenarios/issue652/ plus focused behavior-test registration.
- Persistence: tests only unless behavioral evidence identifies a generic projection defect; typed maps are already serialized generically.

## Scope ledger

| Discovery | Disposition |
|---|---|
| LLxprt sandbox fields omitted from the shipped definition | In-scope: root cause |
| Edit Agent hard-codes sandbox disabled | In-scope: required to preserve Save/Edit behavior |
| LLxprt sandbox emitters omitted | In-scope: required for the persisted setting to be usable |
| Startup sandbox normalization is currently a no-op | Out of scope: no acceptance test showed it blocks this fix |
| The flag emitter resolves capability IDs from normalized field names | In-scope: rename the LLxprt capability ID to sandbox-enabled and update preflight lookup |
| LLxprt definition changes alter migration launch-signature hashes | In-scope: update the fixed definition and typed-value hash vectors |
| Disabled legacy records could contain stale nonempty engine/flag values | Reject expansion: accepted save paths clear these values; conditional generic emitters remain an explicit non-goal |

## Review counters

- Local Open Code Review: 1/2
- Post-PR Open Code Review: 0/2
- Independent review cycles: 1/2

## Verification evidence

- Focused state, service, launch-plan, migration-vector, durable round-trip, atomic rejection, and scenario-grammar tests pass.
- macOS tmux scenario llxprt-sandbox-save passes all 24 steps in a fresh isolated workspace after review remediation.
- cargo xtask quick passed before review; exact-head focused tests, format, and strict Clippy pass after remediation.
- Detached coverage passes at 71.80% lines, above the 30% floor; locked all-feature build and tests pass.
- Final exact-head cargo xtask ci passes all stages; coverage is 71.80% lines. Evidence is recorded in /tmp/issue652-ci-final.log.
- DeepThinker and rustreviewer findings were classified and remediated; local OCR reviewed 16 files and its three findings were fixed (two duplicate atomic-edit findings and one disabled-migration assertion gap).

## Deferred findings and follow-ups

None.