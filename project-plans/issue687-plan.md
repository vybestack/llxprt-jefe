# Issue #687 — Make namespace selection config-first and fail-closed

Supersedes the combined correctness scope of #683, #684, and #685.
Branch: `issue687` from `origin/main` at merge commit `07de7846`.

## 1. Invariant

A normal jefe process has exactly one authoritative installation identity. The
startup boundary establishes it once, fallibly, from the effective resolved
config/state location before any multiplexer, session, doctor namespace, or UI
consumer can observe it.

The namespace is never keyed by machine name, account name, elevation state,
working directory, or jefe/psmux version or commit. Rebuilding, upgrading, or
pulling a new binary must re-adopt the same sessions when the effective
config/state location is unchanged. Different effective locations remain
isolated unless the operator deliberately selects and can see an explicit
override.

There is no convenience path that may initialize the process-global identity
from ambient state, and no rejected override may silently fall back.

## 2. Acceptance matrix

| ID | Actor / launch path | Input / boundary | Success | Failure / diagnostic | Side effects before failure | Proof |
|---|---|---|---|---|---|---|
| N1 | Normal UI/server startup | Effective default config/state | Identity is established before multiplexer/session access and remains path-derived | Typed startup diagnostic and non-zero exit | No server or session creation | Startup unit + CLI/TUI scenario |
| N2 | Any startup consumer | Identity accessed before initialization | No self-initialization or ambient fallback | Typed not-initialized error reaches owning boundary; no panic | None; global cell stays empty | Installation + multiplexer unit tests |
| N3 | `--config <dir>` startup | Explicit config directory | Namespace derives from that directory's effective state location on every platform | Prior conflicting identity cannot switch silently | No server in wrong namespace | Startup/integration behavior |
| N4 | Startup with invalid `JEFE_NAMESPACE` | Empty, too long, separator, illegal character | N/A | Fails closed naming variable, rule, correction | No server/session creation | Unit + command behavior |
| N5 | Startup with valid `JEFE_NAMESPACE` | Deliberate recovery/A-B namespace | Uses override and makes provenance visible | N/A | Override never becomes installation default record | Startup + doctor behavior |
| N6 | Unix socket override | `JEFE_SOCKET_PATH` set | If retained, valid absolute value is typed and visibly deliberate | Invalid/unusable value fails closed; no derived fallback | No server/session creation | Unix unit/integration + Windows reachability contract |
| N7 | `jefe doctor --config <dir>` | Requested config differs from ambient | Namespace, origin, isolation rendering, and drift describe requested config without touching active identity or record | Resolution error appears as a report finding, never panic or ambient answer | Read-only | Doctor unit + CLI integration |
| N8 | Restart after rebuild/upgrade | Same config/state; different binary metadata | Exactly same namespace; existing sessions are discoverable/adopted | Binary difference may be reported but never isolates | No replacement server | Identity regression + TUI scenario |
| N9 | Machine/account/elevation/casing drift | Same installation under platform path semantics | Same namespace; known real drift names previous namespace | No silent replacement | Record retained until reconciliation | Cross-platform identity/drift tests |
| N10 | Separate worktrees/installations | Different config/state paths | Distinct namespaces on Windows, macOS, and Linux | Any deliberate collapse is reported as override | No accidental shared server | Unit + TUI scenario |

## 3. Architecture

### 3.1 Pure derivation remains unchanged

`src/runtime/namespace.rs` remains the sole owner of path-to-identity
derivation. It reads no environment and performs no I/O. Existing source
contracts continue to ban machine/account inputs; binary version and commit are
observability only and never enter the derivation.

### 3.2 Installation boundary is the only global writer

`src/runtime/installation.rs` owns the process-global `OnceLock` and the
fallible override read.

Planned contract:

```rust
pub fn initialize(
    state_path: &Path,
) -> Result<&'static InstallationIdentity, InstallationError>;

pub fn current() -> Result<&'static InstallationIdentity, IdentityUnavailable>;

pub(crate) fn resolve_identity(
    state_path: &Path,
) -> Result<InstallationIdentity, InstallationError>;
```

`current()` reads only. It never calls `get_or_init`, never resolves an ambient
path, and never swallows override errors. `initialize()` is the only production
writer. `resolve_identity()` is a side-effect-free boundary operation used by
both `initialize()` and read-only doctor collection so those paths cannot drift.

The unavailable error is typed. Production code does not panic, unwrap, or
invent a fallback identity.

### 3.3 Multiplexer rendering is explicit and fallible

`MultiplexerPlan::current()` already returns `Result`; it propagates an
identity-not-initialized error rather than creating identity. A crate-private
renderer accepts an explicit `InstallationIdentity` for doctor and other
read-only inspection, avoiding mutation of the active cell.

Binary path/version/commit remain evidence on the plan. They do not select the
namespace.

### 3.4 Doctor is config-scoped and read-only

Doctor uses the same persistence path resolver as startup, then
`resolve_identity(&paths.state.path)`, then explicit multiplexer rendering. It
never calls the global accessor and never reconciles the namespace record.
Every resolution error becomes a finding in the report.

### 3.5 Socket override is not allowed to masquerade as installation identity

`JEFE_SOCKET_PATH` is a rendering override, not a `NamespaceOrigin`. The
preferred outcome is to remove it from ordinary production selection if the
harness can express isolation with config/state identity. If subprocess harness
requirements make it necessary, it remains only as a typed, fail-closed,
provenance-complete override:

- absolute and within platform socket limits;
- error on relative/empty/overlong values;
- startup warning naming the override;
- doctor evidence naming the override;
- no silent fallback to an installation-derived socket.

Retirement versus constrained retention is decided by the S3 RED tests and
harness ownership analysis; either result must satisfy N6 completely in this
issue. It is not deferred.

## 4. Bounded vertical slices

### S1 — Initialization contract (N1, N2, N4; closes #684)

- Owners: installation boundary and multiplexer plan.
- Expected production paths: `src/runtime/installation.rs`,
  `src/runtime/multiplexer.rs`.
- Expected tests: `installation_tests.rs`, `multiplexer_tests.rs`, and affected
  adjacent test call sites.
- RED: pre-init access returns typed unavailable twice and leaves global state
  empty; pre-init multiplexer plan fails; invalid namespace never yields a
  derived identity.
- GREEN: delete self-initialization, propagate the typed error, migrate test-only
  convenience callers to explicit cell-free seams.
- Non-goal: no change to path hashing.

### S2 — Config-scoped doctor (N3, N7; closes #685)

- Owners: persistence path resolution, installation read-only resolution,
  doctor collection.
- Expected production paths: `src/doctor/collection.rs`,
  `src/runtime/installation.rs`, `src/runtime/multiplexer.rs`.
- RED: doctor for config B reports B while ambient/default is A; active cell is
  still empty afterward; resolution failures are report findings.
- GREEN: use startup's exact path resolver and explicit identity/plan rendering;
  inspect drift without writing.
- Non-goal: doctor does not start a server or reconcile records.

### S3 — Override closure (N5, N6; closes #683)

- Owners: Unix socket rendering, startup diagnostics, doctor evidence, harness
  isolation boundary.
- Expected paths: `src/runtime/socket.rs`, `src/runtime/multiplexer.rs`,
  `src/startup.rs`, `src/doctor/collection.rs`, and only the harness paths
  required to retire or constrain `JEFE_SOCKET_PATH`.
- RED: relative/empty/overlong socket values fail rather than fall through;
  valid override is visible; namespace override remains visible and unrecorded.
- GREEN: retire the ambient socket selector where possible; otherwise return a
  typed socket-override error and propagate it through plan/startup/doctor.
- Cross-platform proof: Unix tests execute behavior; a source contract proves
  the override cannot affect Windows.

### S4 — Session continuity and isolation (N8, N9, N10)

- Owners: behavioral tests and TUI harness.
- Scenario is written first and proven RED where current behavior is missing.
- New scenario proves two config locations cannot see each other's sessions and
  a restart/rebuilt binary using the same config re-adopts its sessions.
- Unit regressions prove binary metadata never keys identity and platform path
  equivalence remains correct.
- No production implementation beyond defects exposed by the scenario.

## 5. Expected call-site cutover

Production `installation::current()` callers in multiplexer and doctor are
converted: multiplexer propagates typed absence; doctor receives an explicit
resolved identity. `startup::build_persistence` remains the only production
initializer and runs before TUI/session access.

All production `MultiplexerPlan::current()` consumers already return `Result`
and propagate multiplexer failures; no silent fallback is added. Tests that
relied on implicit global initialization use existing cell-free test seams or
explicit identities, not competing production APIs.

## 6. Non-goals

- Keying namespace on jefe or psmux version/commit.
- Automatically migrating, killing, or reaping historical servers.
- Changing platform default config/state directory policy.
- Adding a second namespace mechanism.
- Weakening lint, architecture, coverage, source-size, or cross-platform gates.

Discovery of old namespaces and stable re-adoption when the config is unchanged
are in scope. Automatic migration of already-stranded historical servers is
not.

## 7. Scope ledger

| Item | Disposition |
|---|---|
| #683 socket override | Completed in S3: retained for harness ownership, typed, fail-closed, length-safe, and provenance-visible |
| #684 self-init + invalid override fallback | Completed in S1: removed; pre-init access is typed and non-mutating |
| #685 doctor config ordering | Completed in S2: config-scoped read-only resolution without record/global mutation |
| Binary identity | Observability only; explicit N8 regression and session-continuity scenario |
| Harness changes | S4 scenarios execute same-config re-adoption and different-config isolation on Unix |
| Existing namespace derivation | Preserve; no new inputs |
| Historical session migration | Out of scope; no automatic mutation/reaping |

Every changed file must map to N1–N10 or be recorded here before edit.

## 8. Review and verification ledger

- Pre-PR review: 1 / 2. No blockers; N2 direct multiplexer proof and N8/N10
  executable Unix scenarios added. A redundant error-message wording nit was
  rejected. `resolve_identity` remains `pub` inside the crate-private module
  because `pub(crate)` violates the repository's `redundant_pub_crate` lint and
  does not alter the external API.
- Post-PR review: 1 / 2. Approved with no blockers or substantive correctness
  findings. The one in-scope cosmetic finding was fixed by renaming the
  fail-closed scenario from namespace override to socket override, matching the
  `JEFE_SOCKET_PATH` behavior it executes. Pure repeated doctor resolution and
  isolation status wording were rejected as non-issues; Unix PTY execution
  remains assigned to compatible CI.
- Exact-head local gates: fmt, native and Linux-target Clippy, locked build,
  architecture, clippy-allow, multiplexer-surface, source-size, and Windows
  coverage floors pass. Full coverage passes at 71.42% lines.
- Scenario contracts: fail-closed override, same-config continuity, and
  different-config isolation all parse; executable Unix harness tests are wired
  into `harness_v1_fixtures`. This Windows host cannot execute Unix PTYs, so
  Linux/macOS CI is the execution authority.
- Required local candidate-head command: `cargo xtask ci`.
- Required PR evidence: exact-head Linux and native Windows CI, coverage,
  conflict-free ancestry, every finding triaged, and no unresolved correctness
  finding deferred from N1–N10.
