# Issue #547 — Runtime namespace identity must survive identity-material change

Sub-issue of epic #539 (criterion E12). Labels: bug, windows.
Branch: `issue547` off `origin/main` @ `1c8912df`.

## 1. Scope as actually stated

The issue body defines V1–V7. A later maintainer comment ("Scope addition:
multiplexer-binary isolation, approved by maintainer") adds V8–V12 and states
it should land **early, ahead of the broader hostname-drift work**, because it
is self-contained and gates #540 V12.

Both halves are in scope. The upstream half (psmux/psmux#509 — no stable
per-namespace server identity) is **not** ours and is not attempted here.

## 2. Current behaviour (grounded)

- `src/runtime/identity.rs:36-42` — `current_identity_material()` on Windows is
  `format!("{host}\0{account}")` from `whoami`, hashed by
  `namespace_for_identity()` (FNV-1a) into `jefe-<16 hex>`.
- `namespace_for_identity(identity: &[u8])` is already **generic over bytes**,
  so adding material requires no new hashing primitive.
- `unique_namespace_for_identity()` already composes `process::id()` +
  `AtomicU64::fetch_add` + nanos — the collision-resistant generator required by
  `tests/core/windows_ci_signal_contracts.rs::psmux_test_namespaces_never_depend_on_a_timestamp_alone`.
- `src/runtime/multiplexer.rs:352-376` — `preflight()` runs `-V` and passes the
  output to `classify_probe`, whose success type is `MultiplexerVersion`.
  **The commit is parsed away and discarded.** `ProbeObservation::Output`
  (:394-400) still carries the full `stdout`, so the commit is available at the
  classification boundary today.
- `resolve_executable()` (:853) honours `JEFE_PSMUX_BIN` / `JEFE_TMUX_BIN`, but
  the resolved binary is **not** an input to the namespace.
- `src/harness/psmux_driver.rs:446-506` has a *second*, independent
  `PsmuxVersion` parser. The issue requires the version/commit parse be built
  once and shared, not implemented twice.

## 3. Design decisions

### D1 — The namespace has two axes plus an override

| Axis | Purpose | Criteria |
|---|---|---|
| Identity material (host + user, normalized) | isolation between concurrent users on one machine | V3, V4, V5 |
| Multiplexer binary identity (version **+ commit**) | stop a fork build from joining a release build's namespace | V8, V9, V10 |
| Explicit override | deliberate, loud isolation for A/B runs | V11 |

### D2 — Persistence is authoritative; derivation is only used to mint

`<state_dir>/runtime-namespace.json`, written through `persistence::writer`
(`AtomicWrite` + `BackupPolicy`). Once a namespace is minted it is **never
re-derived**. This is what makes hostname drift survivable (V1).

### D3 — The record is a map keyed by binary-identity fingerprint

```
{ version, entries: [ { binary_fingerprint, namespace, identity_fingerprint } ] }
```

This reconciles the two axes, which otherwise conflict:

- **Zero-strand upgrade.** On a machine with no record, the first slot is minted
  with the *legacy* formula `fnv(host\0user)`, so an existing user with live
  sessions lands on the namespace those sessions already occupy.
- **Binary isolation.** Any *subsequent, different* binary identity mints a new
  slot whose material includes the binary fingerprint → different namespace
  (V8, V9).
- **Drift immunity.** Host/user change does not change the key, so the
  persisted namespace still wins (V1).

### D4 — Normalized fingerprint material

Trim + ASCII-lowercase before fingerprinting, so casing-only change is not
drift (V3). V4 holds because distinct users have distinct `%LOCALAPPDATA%` →
distinct state dirs → distinct records. V5 holds because elevated and
non-elevated share `%LOCALAPPDATA%` → same record → `Stable`.

### D5 — Resolution is a total, explicit enum

```rust
enum NamespaceResolution {
    Minted   { namespace, .. },
    Stable   { namespace },
    Drifted  { namespace, recorded_fingerprint, current_fingerprint, would_derive },
    Rebound  { namespace, previous_namespace, previous_binary, current_binary },
    Overridden { namespace, source },
}
```

`Drifted.namespace` is the **persisted** value, so sessions stay findable while
the user is warned. This is the V2 requirement: "no sessions" and "we changed
where we were looking" are different values, not the same empty list.

### D6 — A legitimate psmux upgrade must be loud, not silent

**This is a consequence of the scope addition that the issue does not spell
out.** If the namespace keys on binary identity, then a routine psmux upgrade
(3.3.7 → 3.4.0) selects a different slot and every running session becomes
invisible — reproducing the exact symptom this issue exists to eliminate.
`Rebound` therefore exists to report it explicitly, and the previous namespace
is retained in the record so recovery is possible. **See open question Q1.**

## 4. Acceptance matrix

| ID | Behaviour | Test |
|---|---|---|
| V1 | Sessions survive hostname change and username change across restart | `tests/core/namespace_persistence.rs` |
| V2 | Empty-namespace vs changed-namespace are distinct outcomes | same |
| V3 | Casing-only change in user/host is not drift | `src/runtime/namespace.rs` unit |
| V4 | Two different users stay isolated | `tests/core/namespace_persistence.rs` |
| V5 | Elevated vs non-elevated resolve identically | same |
| V6 | `jefe doctor` reports namespace, derivation, drift | `tests/doctor/` |
| V7 | Unix socket path incl. `id -u` unreachable on Windows, proven mechanically | `tests/core/multiplexer_socket_scoping_contract.rs` |
| V8 | Two psmux binaries differing only by commit → different namespaces | `src/runtime/namespace.rs` unit + e2e |
| V9 | Fork build cannot list/attach/modify/reap a release build's sessions | `tests/psmux_*` |
| V10 | Recorded multiplexer identity includes the commit | `multiplexer.rs` unit |
| V11 | Override works and is visible in UI + doctor | unit + `tests/doctor/` |
| V12 | Doctor reports binary path + version + commit | `tests/doctor/` |

### Test-isolation constraint

The V1/V4/V5 tests must **not** mint via the real `fnv(host\0user)`, because
that value is identical for every concurrent test process on the machine and
would collide on a shared psmux server — the precise hazard behind contract A6.
These tests inject unique identity material seeded from
`process::id()` + `fetch_add` (reusing `unique_namespace_for_identity`'s
existing approach), then mutate it to prove the persisted namespace holds.

## 5. Slices (RED → GREEN → REFACTOR each)

| # | Slice | Criteria | Notes |
|---|---|---|---|
| S0 | `#[cfg(unix)]`-gate the socket module | V7 | Independent. Blockers: `multiplexer.rs:197` calls `super::socket::jefe_tmux_socket_path()` inside the `Unix` arm; `commands_tests.rs:640` calls it in an `else` branch. Fix by cfg-splitting `current_isolation()`. |
| S1 | Parse and carry psmux **commit**; unify the duplicate parser | V10 | Widen `classify_probe`'s success type to a `MultiplexerIdentity { version, commit }`. Share with `harness/psmux_driver.rs`. Overlaps #540 V1/V2. |
| S2 | Binary identity becomes namespace input | V8, V9 | |
| S3 | Explicit, loud namespace override | V11 | |
| S4 | Persistence + drift resolution | V1–V5 | `src/runtime/namespace.rs` (pure) + `src/runtime/namespace_store.rs` (I/O). |
| S5 | Doctor provenance | V6, V12 | Extend `record_namespace_isolation` (`src/doctor/collection.rs:89`). Fingerprints hashed, never raw material. |

Maintainer sequencing puts S1–S3 ahead of S4. S0 is independent and lands first
because it is cheap and removes a live foot-gun.

## 6. Non-goals

- psmux/psmux#509 (stable per-namespace server identity) — upstream, not fixable here.
- Changing `%LOCALAPPDATA%` state-dir location (#137).
- The fail-closed liveness work (separate sub-issue of #539).
- Reaping or migrating sessions stranded by *past* drift on existing machines.

## 7. Scope ledger

| Item | Status |
|---|---|
| New public abstractions: `NamespaceResolution`, `MultiplexerIdentity`, namespace override env var | **Awaiting approval** |
| New modules `src/runtime/namespace.rs`, `namespace_store.rs` | **Awaiting approval** |
| New persisted artifact `runtime-namespace.json` | **Awaiting approval** |
| Touching `harness/psmux_driver.rs` to de-duplicate the version parser | In scope per issue ("built once and shared") |
| Adding a new psmux test file to the hardcoded list in `windows_ci_signal_contracts.rs` | **Question Q2** — that file is quality tooling |
| Coverage/observability findings from this session | Split out as #662, #663, #664 |

## 8. Open questions for the maintainer

- **Q1.** On a legitimate psmux *upgrade*, should sessions (a) be reported as
  `Rebound` with the old namespace offered for recovery, or (b) carry over by
  re-keying the existing slot to the new binary identity? (b) is friendlier but
  weakens V8's guarantee. Plan currently plans (a).
- **Q2.** May I add the new psmux test file to the allowlist in
  `tests/core/windows_ci_signal_contracts.rs`? It is quality tooling, which
  normally requires approval before modification.
- **Q3.** Override mechanism: env var, CLI flag, or both? Issue says "env var
  or CLI flag".
