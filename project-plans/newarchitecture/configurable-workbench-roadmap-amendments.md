# Configurable Workbench Roadmap — Binding Amendments

> **Superseded:** the canonical issue bodies live in `configurable-workbench-github-issues/`, which carries the binding **no-shim policy**. This document's prescriptions of "LLxprt legacy adapter", "compatibility adapter" remote behavior, or any runtime shim kept alive across issues are void: superseded code is deleted at each capability's feature-complete, and the only permitted bridge is the one-way persistence schema migration.

These amendments are normative for `configurable-workbench-issue-roadmap.md` and resolve its sequencing/scope ambiguities.

## 1. Epic remains functional

The epic promises outcomes only:

- users customize Jefe without patching Rust;
- extension authors rely on stable documented contracts;
- operators can explain/recover invalid configuration without data loss;
- maintainers add agents/screens/plugins without duplicating product branches;
- installed compatible functionality is discoverable and predictable.

Architecture mechanisms are binding constraints referenced from the specification, not epic acceptance criteria.

## 2. Corrected foundation sequence

```text
CW-00 harness ───────────────────────────────┐
CW-01 agent contract ─> CW-03 diagnostics ─> CW-04 settings ─> CW-02 local agents ─> CW-02R remote agents
```

CW-02 remains the first major user-facing architecture slice; CW-00/01/03/04 are enabling foundations.

- CW-03 depends on CW-01, not CW-02.
- `agent-type list` belongs to CW-02.
- CW-02 depends on CW-00, CW-01, CW-03, and CW-04.
- CW-02 includes a minimal Agent Types status/enablement screen/panel, so all states are discoverable immediately.
- CW-14 integrates that existing agent surface into unified Settings; it does not first implement agent management.

## 3. Split first major slice without splitting the principle

### CW-02 — Definition-driven local agents

Includes shipped LLxprt, Code Puppy, and capability-verified Codex together through one generic registry/planner:

- local definition discovery;
- compatible/incompatible/not-found/probe-error status;
- explicit enablement;
- minimal status/enablement UI;
- typed creation forms;
- exact local launch plans;
- persisted type IDs/values;
- stale executable generation;
- LLxprt legacy adapter and idempotent migration.

Existing remote behavior remains on the compatibility adapter, so the application remains functional.

Additional EARS:

- **CW02-LOCAL-01 WHEN** a shipped LLxprt, Code Puppy, or Codex executable passes its pinned identity/capability fixture, **Jefe shall** classify that exact executable as InstalledCompatible.
- **CW02-LOCAL-02 WHEN** a valid `local.*` definition is discovered without an enable/reference entry, **Jefe shall** list it as inactive and exclude it from creation.
- **CW02-LOCAL-03 IF** an inactive local definition is invalid, **THEN Jefe shall** report a provider-free warning and continue startup.
- **CW02-LOCAL-04 IF** an active local definition is invalid, **THEN Jefe shall** fail candidate publication.
- **CW02-LOCAL-05 WHEN** Agent Types status opens, **Jefe shall** display the product of enablement and availability for every shipped/discovered definition.

Status projection fixture:

| Enablement | Availability | Display | Creation | Launch |
|---|---|---|---|---|
| enabled | compatible | Installed, enabled | yes | yes |
| disabled | compatible | Installed, disabled | no | no |
| enabled | incompatible | Enabled, incompatible: reason | no | no |
| disabled | incompatible | Installed, incompatible; disabled | no | no |
| enabled | not found | Enabled, not found | no | no |
| disabled | not found | Not installed; disabled | no | no |
| either | probe error | Probe error: reason | no | no |

### CW-02R — Definition-driven remote agents

**Value:** Remote repositories receive the same definition-driven agent support and precise diagnostics without risking local migration.

**Dependencies:** CW-02.

**Scope:** Remote capability probes, workdir fallbacks, target generation, typed launch-plan serialization, transport/not-found/incompatible distinction, LLxprt remote setup compatibility, and removal of temporary remote product branches.

**EARS:**

- **CW02R-01 WHERE** a repository target is remote, **Jefe shall** resolve compatibility using the definition's host-generated probe contract without requiring local installation.
- **CW02R-02 IF** SSH/auth/effective-user probing fails, **THEN Jefe shall** classify ProbeError rather than NotFound.
- **CW02R-03 IF** remote executable identity lacks required capability, **THEN Jefe shall** classify InstalledIncompatible and refuse launch.
- **CW02R-04 WHEN** remote launch begins, **Jefe shall** serialize the validated `OsString`-compatible plan through the single audited transport serializer.
- **CW02R-05 IF** repository target generation changes after planning, **THEN Jefe shall** reject the stale plan and reprobe.
- **CW02R-06 WHERE** legacy LLxprt remote setup is configured, **Jefe shall** preserve host-owned setup behavior until explicit migration.

**Harness:**

- `agent-definition-remote-compatible.json`
- `agent-definition-remote-not-found.json`
- `agent-definition-remote-probe-error.json`
- `agent-definition-remote-incompatible.json`
- `agent-definition-remote-stale-target.json`

## 4. Corrected contract ownership

- CW-09 depends on CW-05 and owns screen syntax plus opaque action/route/relationship references, ownership, duplicates, and shipped fixtures.
- CW-12 owns relationship DTO policy, graph validation, and propagation.
- CW-13 owns route activation schemas and navigation semantics.
- CW-16 owns plugin manifest/package schemas plus provider envelope/state/request matrices, limits, malformed fixtures, and config-migration envelopes.
- CW-19 only implements supervisor/lifecycle enforcement, online configuration/migration execution, cancellation, cleanup, generations, recovery, and environment/secret delivery.

## 5. Settings validation behavior

Enable is a draft-only trust decision. Enabling does not execute the provider.

- Save performs static manifest/schema/composition validation and atomically saves exact version plus config.
- Provider semantic validation runs during normal restart/startup.
- Failure produces provider-free recovery diagnostics and leaves the saved selection available for disable/rollback.
- Provider environment begins from a documented minimal allowlist; only owner-specific resolved secrets enter Configure and never logs/state/effective output.
- CW-18 owns migration preview/approve/cancel UI.
- CW-19 owns bounded migration request execution/validation.

## 6. EARS/test traceability rule

Every issue must assign stable IDs to criteria before creation. One criterion has one trigger/state and one observable response. Exact behavior references a named fixture/table. Tests cite criterion IDs.

Example decomposition for settings save:

- **CW04-SAVE-01 WHEN** the loaded settings hash differs from disk at save, **Jefe shall** reject overwrite and preserve both files.
- **CW04-SAVE-02 WHEN** edited paths validate and disk hash matches, **Jefe shall** patch only those syntax paths.
- **CW04-SAVE-03 WHEN** a patched document is committed, **Jefe shall** preserve comments/order/unknown/dormant nodes in the golden fixture.
- **CW04-SAVE-04 IF** staging, sync, rename, or parent-sync fails, **THEN Jefe shall** retain the original and expose retryable failure.

This mechanical pass is required when converting every CW outline into a GitHub issue.

## 7. UI scenario contract template

Every UI issue must list concrete filenames and, for each:

```text
Fixture: config/PATH/package/provider/files and dimensions
Steps: exact keys and typed harness actions
Wait: literal condition used by waitFor
Assert: positive and negative literal screen assertions
Restart/final: persisted file/invocation/visible state after restart
Mock: normal, focus, unavailable/error, dirty, and small geometry where applicable
CW-00 capabilities: env | file tree | process capture | resize | restart
```

All named scenarios in the roadmap use this template before issue creation.

Additional required concrete names:

- CW-06: `terminal-protected-recovery-macos.json`, `terminal-protected-recovery-linux.json`.
- CW-10: `shipped-screen-definition-parity.json`, `definition-dashboard-focus.json`, `definition-issues-focus.json`, `definition-pr-focus.json`, `definition-actions-focus.json`.
- CW-11: `custom-screen-enable-order.json`, `custom-screen-inactive-invalid.json`, `custom-screen-active-invalid.json`, `custom-screen-tiny-layout.json`.
- CW-12: `relationships-master-detail-immediate.json`, `relationships-master-detail-explicit.json`, `relationships-empty-retain.json`, `relationships-invalid-fanout.json`.
- CW-13: `typed-navigation-back.json`, `navigation-dirty-guard.json`, `navigation-local-unwind.json`, `navigation-depth-limit.json`.
- CW-14: `settings-general-appearance.json`, `settings-screens-order.json`, `settings-agent-types-integration.json`, `settings-diagnostics.json`, `settings-dirty-back.json`, `settings-lossless-save.json`, `settings-external-edit.json`.
- CW-15: `settings-keys-remap.json`, `settings-keys-conflict.json`, `settings-keys-unbind-reset.json`, `settings-keys-protected.json`.
- CW-17: `plugin-install-enable-restart.json`, `plugin-broken-unsupported.json`, `plugin-version-rollback.json`, `plugin-remove-enabled-rejected.json`.
- CW-18: `plugin-generated-settings-fields.json`, `plugin-settings-visibility-validation.json`, `plugin-secret-redaction.json`, `plugin-dormant-config.json`, `plugin-config-migration-approve.json`, `plugin-config-migration-cancel.json`, `plugin-config-migration-failure.json`.
- CW-19: `provider-crash-retry-generation.json`, `broken-provider-recovery-cli.json`.
- CW-20: `plugin-contributed-screen-panels.json`, `plugin-action-help-hint.json`, `plugin-invalid-outcome.json`.
- CW-21: `git-merger-install-config-merge.json`, `git-merger-head-changed.json`, `git-merger-cancel.json`, `git-merger-disabled.json`, `git-merger-secret-redaction.json`.
- CW-22: `release-clean-homebrew-layout.json`, `release-clean-linux-layout.json`, `author-example-custom-workbench.json`.

## 8. Cross-cutting UI/accessibility gate

Every UI issue must test applicable requirements:

- deterministic cell width and grapheme-safe clipping;
- no color-only status/focus/error;
- visible keyboard focus and reachable controls;
- modal focus trap/restoration;
- adjacent field validation plus summary;
- disabled/unavailable reason;
- protected recovery at reduced geometry.

## 9. Recovery/explainability allocation

- CW-03: `config path|validate|show-effective|edit|format`.
- CW-02/CW-02R: explain agent probe/compatibility.
- CW-06: explain action/binding.
- CW-11: explain screen/layout/provenance.
- CW-17: explain plugin package/contributions/version selection.
- CW-19: explain provider startup/request failure.

Every command has provider-free malformed-settings and hanging-provider tests where applicable.

## 10. Plugin update semantics

Installing a new exact version side-by-side is the v1 update operation. There is no separate `plugin update` command. Selection changes only through enable/switch/rollback after validation.

## 11. Corrected release dependency

```text
CW-15 + CW-21 -> CW-22
```

CW-22 is a release gate across all prior contract and feature issues, not merely the successor to the reference plugin.
