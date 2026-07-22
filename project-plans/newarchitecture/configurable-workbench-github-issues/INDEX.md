# Configurable Workbench GitHub issue-body index

Each numbered Markdown file is the complete exact GitHub issue body and sole implementation artifact for its capability. Bodies reproduce every consumed contract required for implementation; this index defines ordering and readiness only. Create issues in the order permitted by delivered capabilities, not by unresolved numeric GitHub references.

## Capability creation and implementation order

| File | Exact GitHub title | Consumed delivered capabilities | Body readiness |
|---|---|---|---|
| `00-epic.md` | Epic: Configurable Workbench v1 | none | paste-ready |
| `01-deterministic-tui-harness.md` | CW-00: Deterministic real-process TUI harness | none | paste-ready |
| `02-configuration-state-effects.md` | CW-01: Exact configuration/state migration, offline recovery, and closed effects | deterministic real-process harness | paste-ready after harness capability exists |
| `03-four-agent-cutover.md` | CW-02: Complete vertical four-agent definition cutover | deterministic harness; schema-2 persistence and closed effects | paste-ready; runtime compatibility is dynamic per installed release (probe-decided), with a Claude fixture-authoring capture required before implementation RED |
| `04-action-registry-keymaps.md` | CW-03: Action registry, source-derived default inventory, and single-chord keymaps | deterministic harness; schema-2 configuration and closed effects | unchanged; paste-ready after consumed capabilities exist |
| `05-screen-descriptors-layout.md` | CW-04: Sole internal screen descriptors and unified layout parity | deterministic harness; action registry | paste-ready after consumed capabilities exist |
| `06-custom-screens-relationships.md` | CW-05: External custom screens lowered to descriptors and typed relationships | configuration/effects; actions/keymaps; descriptors/layout | paste-ready after consumed capabilities exist |
| `07-navigation-dirty-lifecycle.md` | CW-06: Typed routes, local unwind, navigation, and dirty lifecycle | actions/keymaps; descriptors/layout; custom screens | paste-ready after consumed capabilities exist |
| `08-settings-shell.md` | CW-07: Core Settings shell and lossless draft UI | persistence/writer; descriptors/layout; navigation/dirty | paste-ready after consumed capabilities exist |
| `09-registry-editors.md` | CW-08: Agent Types, Screens/Layout, and Keys editors | four-agent registry; actions/keymaps; custom screens; Settings | paste-ready after consumed capabilities exist |
| `10-plugin-package-inventory.md` | CW-09: Package roots, manifest inventory, archive install, and explicit trust | persistence paths/settings; Settings; approved package-dependency decision | **blocked: only expected remaining blocker** |
| `11-provider-actions.md` | CW-10: One-shot and persistent action-provider lifecycle | actions/keymaps; delivered static package inventory | paste-ready; implementation waits for delivered package inventory |
| `12-provider-panels-config.md` | CW-11: Persistent host-rendered panels and plugin configuration migration | custom screens; navigation; Settings; package inventory; provider actions | paste-ready after consumed capabilities exist |
| `13-git-merger-package.md` | CW-12: Exact Git Merger reference package | registry editors; provider panels/config | paste-ready after consumed capabilities exist |
| `14-ownership-effect-audit.md` | CW-13: Ownership, stale-generation, and effect-order audit only | configuration/effects; agent/navigation generations; provider actions/panels | paste-ready after consumed capabilities exist |
| `15-authoring-kit.md` | CW-14: Authoring schemas and compatibility runner | all delivered owner contracts | paste-ready after consumed capabilities exist |
| `16-release-gate.md` | CW-15: Final aggregation, installed layouts, and release gate | registry editors; Git Merger; ownership audit; authoring kit | paste-ready after consumed capabilities exist |

A body being paste-ready means its contract is complete; it does not waive its consumed-capability entry condition. Independent bodies remain actionable even while package inventory is blocked.

## Sole remaining blocker

`10-plugin-package-inventory.md` is the only expected remaining blocker. Do not call that body paste-ready and do not create or implement its GitHub issue until a maintainer commits and approves `dev-docs/decisions/plugin-package-dependencies.md` with every exact crate, version, feature, license, provenance, checksum, and policy value enumerated in that body. Current Rust standard-library and approved project dependencies do not provide all required safe gzip/tar decoding, canonical SemVer, and SHA-256 behavior. Project policy forbids guessing or silently adding dependencies.

After approval, replace the dependency-gate preamble inside `10-plugin-package-inventory.md` with the approved concrete values, verify its lockfile/license consequences, and change only that row’s readiness. No other body requires that decision merely to remain a complete issue body; provider/package-dependent implementation waits on the delivered package-inventory capability.

## Body completeness ledger

| Body group | In-body implementation authority |
|---|---|
| epic | self-contained capability DAG, source ownership, startup/effect/migration/security contracts, aggregate UI states and release ledger |
| harness | one-time in-repo scenario conversion to schema 1 (old format deleted), process capture, interpolation, containment, PTY/restart, exits, redaction and limits |
| configuration/state | exact path identity, Settings/State schemas, migration, hash writer, recovery CLI and closed effects |
| descriptors/custom/navigation | executable layout allocation, five-screen parity, exact discovery/lowering/relationships, closed navigation/dirty intents |
| Settings/editors | draft identity/hash/revision, preview/reload/export, exact sparse payloads and complete layout-edit flow |
| providers/panels | complete JSONL direction/payload/state protocol, process supervision, host model/event DTOs, config/secrets/migration |
| Git Merger | exact installed files and destructive `gh` view/merge argv flow with head protection/no automatic retry |
| audit | exact source dependency/handle/authority guards and stale-owner permutations |
| author kit | exact installed paths, index and directory hashes, production parser reuse, CLI/exits and offline security |
| release | exact Homebrew/Linux installed manifests/layouts, immutable owner hashes, relocation/recovery/security scans |

## Global unchanged delivery gates

Every body uses test-first RED, smallest-owner GREEN, then parity-protected REFACTOR that **deletes** the superseded code. The epic's no-shim policy binds every body: no backward-compatibility shim, legacy adapter, facade, dual code path, deprecated re-export, or superseded type survives its capability's feature-complete; the only permitted bridge is the one-way persistence migration (settings/state schema 1 to 2 and its alias/value mapping). The ownership audit and release gate scan for shim-token permutations against a minimal audited allowlist. Every source implementation must pass the unchanged `make ci-check` gates: formatting; clippy allow scan; 1,000-line hard and 750-line warning source limits; all-target/all-feature clippy with `-D warnings`; cognitive/function/argument/boolean/type-complexity limits 15/60/6/3/250; line coverage at least 30%; locked all-feature build and tests. No body authorizes unsafe code, production panic/unwrap/expect, lint suppression, threshold increase, arbitrary scenario shell, guessed external capability, weak/mock-only evidence, unapproved dependency, or compatibility shim.

No issue body depends on this index for a type, algorithm, flow, test, UI state, migration, recovery, security, or documentation contract.