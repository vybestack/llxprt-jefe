# Issue 657: Delete the --help capability probe

Issue: https://github.com/vybestack/llxprt-jefe/issues/657

## Summary

Every agent launch is gated by a second subprocess that scans `--help` for argument tokens the definition already authored. The scan cannot make a launch succeed that would otherwise fail; it can only fail a launch that would otherwise succeed. This deletes it.

Authored arguments are declared by the definition and assumed present. Each definition records a minimum supported version that is documented and displayed but never parsed, compared, or enforced. The identity probe is kept for path-name and repository-local candidates and skipped for package-runner candidates, where preparation already knows the resolved version.

## Evidence the gate does nothing

| Agent | Trusted before | `required` | What the `--help` run can change |
|---|---|---|---|
| LLxprt | yes (#535) | `prompt-interactive` | Nothing; already skipped |
| Code Puppy | no | `interactive` | One token, one agent |
| Codex CLI | no | empty | Nothing. Verdict is always compatible |
| Claude Code | no | empty | Nothing. 16 KB parsed and discarded |

`runtime/agent_plan.rs::resolve_flag_token` reads `definition.probe.capabilities.tokens` directly and never consults the probe result, so argv never depended on the scan (#534 root cause point 1).

## Acceptance matrix

| ID | Actor / path | Input and target | Observable success | Failure behavior / side effects | Persistence and compatibility | Proof |
|---|---|---|---|---|---|---|
| AC1 | Startup probes any shipped agent, local | Compatible installation on PATH | Exactly one subprocess spawned, with the identity argv | Identity failure still returns AGT-E202 with the exact reason | Availability shape changes; no user data migrates | Process-capture test asserting one spawn per candidate |
| AC2 | Agent whose help would fail the old gate | Help exits nonzero, times out, invalid UTF-8, or oversize | Agent is compatible and launchable | No probe error from help, because help never runs | No configuration change | Regression test converted from the old negative table |
| AC3 | Launch each of the four agents | Any supported operation and target | argv byte-identical to pre-change goldens for the same typed values | Unchanged | Unchanged | Golden argv tests, all four agents |
| AC4 | Definition authoring | A definition declaring a flag emitter | The emitter carries its own literal token; no capability id resolves argv | A reference to an undeclared field still fails validation at load | Closed schema rejects removed fields | Reader, canonicalizer, validation tests |
| AC5 | User views an agent type | Installed agent, declared minimum version | Resolved version and declared minimum both shown as text | A version below the minimum is not blocked, warned, or degraded | Minimums are documentation only | Status projection test asserting no gating |
| AC6 | Remote target | Remote probe for a supported agent | Remote path spawns identity only, never a remote help command | Identity failures behave as before | Remote serialization unchanged | Remote probe contract test |
| AC7 | Architecture and epic contract | Source scanned at feature-complete | No `CapabilityProbe`, required list, capability evaluation, or trusted flag remains, and no shim replaces them | n/a | n/a | Symbol-deletion proof plus existing shim-token scan |
| AC8 | Repository after the change | Fixtures and suite scanned | No help capture remains under `tests/fixtures/agent-definitions/`, and nothing reads one | n/a | Version and probe captures unchanged | Fixture inventory assertion |

## Non-goals

- Removing the identity probe for path-name and repository-local candidates, the executable fingerprint, generations, or the stale-plan guard (AGT-E203).
- Removing package preparation, the managed package cache, or the probe-to-plan invocation binding that fixes #571.
- Building a version comparator or enforcing any minimum version.
- Changing which arguments Jefe emits. AC3 asserts byte-identical argv.
- Changing operation/target support matrices, preflight, or the launch signature.
- Changing selector normalization, sentinels, cache layout, or the separate non-installing `npm view` availability probe.
- Any dependency, schema version bump, or quality-gate change.

## Slices

### Slice 1: Move authored tokens onto emitters

- Rows: AC3, AC4.
- Allowed paths: `src/domain/agent_definition/{fields,canonical,reader}.rs`, `shipped/`, `src/runtime/agent_plan.rs`, focused tests.
- RED: a flag emitter cannot carry its own token.
- GREEN: `Emitter::Flag { name, field }` end to end; every shipped definition declares its literal token; argv goldens unchanged.
- Stop condition: any golden argv byte changes.

### Slice 2: Delete the capability probe

- Rows: AC1, AC2, AC6, AC7.
- Allowed paths: `src/domain/agent_definition/{probe,types}.rs`, `src/runtime/agent_probe*.rs`, `agent_remote_probe.rs`, `src/state/generated_form*.rs`, `src/selection/generated_form_content.rs`, `src/app_input/availability.rs`, `src/agent_status_view.rs`, focused tests. `package_runtime.rs` only to surface the already-resolved version.
- RED: process-capture test asserts one spawn per candidate and observes two.
- GREEN: help never spawned; removed types gone; capability-based field disabling gone.
- Stop condition: deleting a type requires a compatibility branch to keep a consumer compiling.

### Slice 3: Minimum versions, fixtures, documentation

- Rows: AC5, AC8.
- Allowed paths: `src/domain/agent_definition/definition.rs` and `shipped/`, `src/agent_status_view.rs`, `tests/fixtures/agent-definitions/`, `dev-docs/standards/`, `docs/getting-started.md`.
- RED: a definition cannot declare a minimum version; help captures still checked in.
- GREEN: minimums declared and displayed without gating; every help capture deleted with no replacement check; #379 and #382 edited to record the amendment.
- Stop condition: displaying a minimum version pressures a comparator into existence.

## Scope ledger

| Discovery | Disposition |
|---|---|
| Codex and Claude parse help and discard the result | In-scope: core evidence |
| Code Puppy gates one token every release has | In-scope: same deletion |
| LLxprt already skips via the trusted flag | In-scope: flag becomes unconditional and is deleted |
| `resolve_flag_token` reads the definition, not the probe | In-scope: indirection dies with the probe |
| Capability-based field disabling in the generated form | In-scope: its only input is deleted |
| Package candidates spend a second subprocess on identity | Dropped: the premise was wrong. `prepare_managed_npm` returns `prefix: []`, so after preparation an npm identity probe executes the installed binary directly rather than re-entering `npm exec`. The 3.9s measured cost belongs to preparation, which stays either way. Only uvx stays runner-mediated, where the saving is marginal and the version string is a real signal. |
| Identity probe, fingerprint, generations, stale-plan guard | Out of scope: process safety, not argument verification |
| Managed package cache (#425 reproductions) | Out of scope: earned, untouched |
| Executable fingerprint is checked against a stored copy and never re-stat'd | Out of scope: #571 owns the remedy |

## Review counters

- Local Open Code Review: 0/2
- Post-PR Open Code Review: 0/2
- Independent review cycles: 0/2

## Verification evidence

Pending.

## Deferred findings and follow-ups

- Whether AGT-E202 retains enough distinct error codes once help-derived failures are gone.
