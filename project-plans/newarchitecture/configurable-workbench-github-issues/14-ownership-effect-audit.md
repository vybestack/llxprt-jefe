# CW-13: Ownership, stale-generation, and effect-order audit only

## Outcome and consumed contract

Audit and minimally repair ownership/effect violations after agent, navigation, provider-action, and provider-panel generation contracts exist. Consume exactly `Correlation`, `Transition`, eight closed effect families, `Completion`, `Never`, and `IdempotentQuery{max_attempts:1..3}`. This issue creates no store, queue, bus, vocabulary, retry policy, writer, geometry owner, or architecture rewrite.

## Exact audit inventory and required guards

| Area/source | Guard and invariant |
|---|---|
| `src/state/mod.rs::AppState::apply_message` | reducers are deterministic; at most 64 effects; no adapter call/handle |
| `src/messages.rs` and `src/messages/event_conversion.rs` | every completion carries typed owner/screen/activation/semantic identities; no generic JSON |
| `src/app_shell.rs` | commit state, release borrow/lock, then execute ordered effects |
| `src/app_input/` | emits typed intent only; no persistence/runtime/GitHub/provider call |
| `src/ui/` | renders projections and emits intent; no boundary I/O/import |
| `src/persistence/` | owns paths/documents/revisioned writers; imports no process/runtime/UI |
| `src/runtime/` | owns processes/tmux/PTY/supervisor; imports no `AppState` |
| `src/github/` and SSH adapters | return typed completion; own no navigation/UI state |
| agent invocation state | old invocation generation cannot update current runtime/session |
| navigation state | suspended/disposed generation cannot update current instance |
| provider requests/panels | old process/request/panel generation cannot update health/model/outcome |
| layout/action/package registries | exactly one geometry/availability/inventory authority |

Add `scripts/check-architecture.sh` rules based on Rust imports/type declarations and targeted symbol patterns, not comments or broad word matches. Add compile-fail tests where practical for handle-in-state/generic effect. The guard must reject direct `std::process`, file/socket/PTY/provider-handle fields in state/domain/UI; adapter imports from UI; `AppState` imports in runtime/persistence; untyped completion; boundary calls before release; and duplicate writer/path/layout/action/provider authorities. A checked allowlist contains exact existing composition-root paths only, with owner/rationale; adding an entry fails review unless this issue removes a false positive in the same change. No lint allow.

The guard additionally enforces the epic no-shim policy across the whole tree: it rejects case-insensitive shim-token permutations — `legacy`, `compat`, `shim`, `backward`, `bridge`, `fallback_v1`, `old_`, `_old`, `deprecated` — in Rust module names, type/function/field identifiers, `cfg`/feature names, and re-export paths; rejects `#[deprecated]` re-exports of removed surfaces; rejects any module pair implementing the same authority where one delegates to the other for "compatibility"; and rejects superseded symbols named by earlier capabilities as deleted (`AgentKind`, `ScreenMode`, pre-registry dispatch/help/footer maps, schema-1 load/save outside migration, old-format harness parsing). The shim allowlist contains only the one-way persistence migration modules, their tests/fixtures, and literal user-facing diagnostic strings; every entry carries owner/rationale, and an addition without a removed false positive fails review. Any violation found is repaired by deleting the shim and moving behavior to the sole owner — never by widening the allowlist.

Normative invariants: reducer either commits all transition state/effects or none; follow-up 64 is accepted, 65 rejects before commit with owner diagnostic; persistence completion older than latest pending revision changes nothing; stale agent/provider/screen/activation/panel identity changes nothing; first accepted terminal request result cannot be replaced; state has no OS handle; UI cannot execute effects; diagnostics include owner, retry policy, durable-data status, and recovery, max 256/origins 16; secrets are redacted.

## End-to-end, migration, failures, security

Run guards, then unit/property/integration/harness stale-result fixtures across each owner. For any found violation, first add a failing owner regression, move only the misplaced call/state to the existing owner, and retain behavior. There is no persisted format change; therefore migration is N/A. Durable settings/draft, newest state revision, agent definitions, package selection/config survive failures; provider/panel/progress models remain ephemeral. Recovery retries only declared policy or disables an owner offline. Guards never inspect generated secret values or execute providers.

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Recovery -----------------+ + Recovery -----------------+
| Provider failure durable | |>>Persistence CFG-E104    |
| Retry available          | | Enter details            |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Recovery -----------------+ + Recovery -----------------+
| Retry unavailable        | | owner: provider com.x    |
| reason: hash conflict    | | PLG-E502 generation      |
+---------------------------+ +---------------------------+
```
```text
DIRTY                          RECOVERY
N/A: audit dashboard has no    + Recovery -----------------+
draft or editor.               | stale generation ignored |
                               | current generation 4     |
                               +---------------------------+
```
```text
SMALL
+Recovery-------+
|>>PLG-E502     |
| data durable  |
| r Retry q Back|
+---------------+
```

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW13-01 | WHEN architecture guard runs, it shall reject each forbidden dependency/handle/effect pattern. | mutation fixtures for every guard rule |
| CW13-02 | WHEN an effect executes, Jefe shall have committed state and released access. | all eight effect-family probes |
| CW13-03 | IF screen/activation completion is stale, Jefe shall change no state. | active/suspended/disposed property |
| CW13-04 | IF persistence completion is older than pending revision, Jefe shall retain newest. | revisions 1/2 permutations |
| CW13-05 | IF agent generation is stale, Jefe shall retain current runtime/session. | generations 3/4 fixture |
| CW13-06 | IF provider request/process/panel generation is stale, Jefe shall reject health/model/outcome. | each provider owner fixture |
| CW13-07 | IF follow-ups reach 65, Jefe shall reject before partial commit. | 64/65 boundary test |
| CW13-08 | WHEN failures aggregate, Jefe shall show owner/retry/durability/recovery without secrets. | recovery dashboard state scenarios |
| CW13-09 | WHEN source authorities are scanned, Jefe shall contain one path/writer/layout/action/provider owner. | exact symbol inventory guard |
| CW13-10 | WHEN the shim guard scans the tree, Jefe shall contain no shim-token permutation, deprecated re-export, compatibility delegate, or superseded symbol outside the one-way persistence migration allowlist. | one seeded mutation fixture per token permutation and per superseded symbol; allowlist owner/rationale audit |

RED guard mutations and regressions first; GREEN only violations found — each repaired by deleting the shim/misplaced code into its sole owner; REFACTOR test helpers, never contracts.

## Normative documentation and done

Update `dev-docs/standards/architecture.md` with exact dependency/owner matrix, guards, and the no-shim policy; update `dev-docs/standards/persistence-and-runtime.md` with commit-release-execute/stale invariants; update `dev-docs/standards/testing-and-quality.md` with guard mutation tests including shim-token permutations. Done requires zero violations, all stale permutations, state byte-equivalence, UI recovery states, a clean shim scan with a minimal audited allowlist, and unchanged `make ci-check`; no new authority/dependency/suppression/threshold change.