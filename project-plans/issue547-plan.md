# Issue #547 — The multiplexer namespace must identify the installation

Sub-issue of epic #539 (criterion E12). Labels: bug, windows.
Branch: `issue547` off `origin/main` @ `1c8912df`.

## 1. Scope

The issue body defines V1–V7. A later maintainer comment ("Scope addition:
multiplexer-binary isolation, approved by maintainer") adds V8–V12.

Both halves are in scope, but V8/V9 are delivered **inverted** from their
literal wording — see §3 D2. The upstream half (psmux/psmux#509, no stable
per-namespace server identity) is not ours and is not attempted here.

## 2. Behaviour that was wrong

The namespace was a hash of **machine identity**: on Windows
`format!("{host}\0{account}")` from `whoami`; on Unix the socket was
`jefe-<uid>.sock` from an `id -u` shell-out. Both were wrong, in mirror-image
ways:

- **Windows: an identity that should have been stable kept moving.** A machine
  rename, an account rename, a casing difference, or an elevation change
  produced a different hash, and every running agent vanished behind the old
  namespace.
- **Unix: an identity that should have varied never did.** One socket per
  account meant every worktree an operator had open shared a single tmux
  server.

Both symptoms have one cause: the namespace described *who was running jefe*
instead of *which installation was running*.

Evidence gathered while planning: every psmux server on the development machine
carried `-L jefe-76134a0ba22f56e9` while their start directories spanned six
worktrees across two unrelated projects (`jefe/branch-1..4`,
`llxprt/branch-1..2`). `platform_default_state_dir()` has no cwd component, so
one machine meant one namespace and one session pool for everything on it.

## 3. Design decisions

### D1 — A jefe instance is identified by the state location it was launched from

The namespace is a pure function of the **resolved state path**, normalized for
casing, separator style, and trailing separators. Not the host, not the user,
not the binary.

This is what makes every acceptance criterion fall out of one rule rather than
several:

- **V1** — renaming the machine or account does not move the state path, so the
  namespace does not move and sessions survive.
- **V3** — normalization absorbs casing drift in `%LOCALAPPDATA%`.
- **V4** — distinct accounts already have distinct home directories, therefore
  distinct state paths, therefore distinct namespaces. The `id -u` shell-out
  was redundant, and is now banned repo-wide by contract.
- **V5** — elevated and non-elevated share `%LOCALAPPDATA%`, so they share a
  namespace, which is the desired behaviour.
- Separate worktrees launched with `--config <dir>` get separate namespaces on
  **every** platform, closing the Unix half of the bug.

### D2 — Binary identity is reported, never keyed (V8/V9 inverted)

The scope addition asks that two psmux builds differing only by commit land in
different namespaces. Delivering that literally reproduces the exact failure
this issue exists to eliminate: a routine `cargo build` of psmux would hide
every running session across every worktree at once. The frequency here is
daily, not rare.

So the binary commit is **parsed, recorded and reported** (V10, V12), and a
mismatch is a **non-isolating warning**. Deliberate A/B isolation is available
through the explicit `JEFE_NAMESPACE` override (V11), which is loud by
construction because someone had to type it.

### D3 — Derivation is pure; environment and disk live at the boundary

- `src/runtime/namespace.rs` — pure. No env, no I/O. Owns `InstallationId`,
  `InstallationIdentity`, `NamespaceOrigin`, `NamespaceDrift`,
  `InstallationHistory`. Policed by
  `tests/core/namespace_derivation_contract.rs`, which fails the build if
  machine identity ever reappears in the derivation.
- `src/runtime/installation.rs` — boundary. Owns the write-once process
  identity, the `JEFE_NAMESPACE` read, and provenance.
- `src/runtime/namespace_record.rs` — boundary. Owns
  `<state_dir>/runtime-namespace.json`.

Keeping the env read out of the pure half is what makes derivation testable at
all: edition 2024 forbids `std::env::set_var`, so a derivation that read the
environment could not be exercised under test.

### D4 — Identity is fixed once, from the effective paths

`build_persistence` resolves it, so `--config <dir>` isolates the namespace as
well as persistence. Previously `--config` isolated storage while still
reaching into the ambient server.

The two failure modes get deliberately opposite answers:

- A **rejected `JEFE_NAMESPACE` is fatal**. Continuing would attach the operator
  to the exact namespace they asked to be separated from.
- A **conflicting second initialization is refused but survivable**. A server
  may already be running under the identity resolved first, and keeping it is
  what avoids orphaning those sessions.

### D5 — The record reports drift; it does not mint identity

`<state_dir>/runtime-namespace.json` records the namespace last run. It is
evidence, not authority — the namespace is always derived, never read back.
Four outcomes, and the distinctions are the point:

| Outcome | Meaning |
|---|---|
| `FirstRun` | New installation, nothing to strand. Silent. |
| `Stable` | Recorded value matches. Silent. |
| `PreviousNamespaceUnknown` | State exists but no record: an older build ran here and its namespace cannot be reproduced. |
| `Changed { previous }` | Namespace moved, and the abandoned one is named so the sessions can be found. |

`PreviousNamespaceUnknown` is the case that matters for existing users,
including the sessions stranded in `jefe-76134a0ba22f56e9`: a plain "first run"
would have been silent and useless. Overrides are never recorded and never
compared, because recording one would make the next ordinary launch report a
false drift.

Drift is never fatal. Startup warns and continues; `jefe doctor` inspects
without writing, so it cannot erase the evidence it exists to report.

## 4. Acceptance matrix

| ID | Behaviour | Test | Status |
|---|---|---|---|
| V1 | Sessions survive host/account rename across restart | `runtime::namespace_tests`, `namespace_record_tests` | Done |
| V2 | Empty vs changed namespace are distinct, reported outcomes | `namespace_tests`, `namespace_record_tests`, `startup` | Done |
| V3 | Casing/separator-only change is not drift | `namespace_tests` | Done |
| V4 | Two accounts stay isolated | `namespace_tests` | Done |
| V5 | Elevated vs non-elevated resolve identically | `namespace_tests` | Done |
| V6 | `jefe doctor` reports namespace, origin, state path, drift | `doctor::collection` tests | Done |
| V7 | Unix socket path unreachable on Windows, proven mechanically | `tests/core/unix_socket_gating_contract.rs` | Done |
| V8 | Differing psmux builds are distinguished | inverted — see D2 | Done as V12 |
| V9 | Fork build cannot silently pass as a release build | inverted — see D2 | Done as V12 |
| V10 | Recorded multiplexer identity includes the commit | `multiplexer.rs` unit | Done |
| V11 | Override works, is refused when unusable, and is visible | `namespace_tests`, `installation_tests`, `startup`, doctor | Done |
| V12 | Doctor reports binary path + version + commit; mismatch warns | `doctor::collection` tests | Done |

## 5. Slices

| # | Slice | Criteria | Commit |
|---|---|---|---|
| S0 | `#[cfg(unix)]`-gate the socket module; cfg-split `current_isolation` | V7 | `8047e80e` |
| S1 | Parse and carry the psmux commit; delete the duplicate parser | V10 | `f6bf52f3` |
| — | Discover psmux test files dynamically instead of a hardcoded roster | tooling | `b0f143af` |
| S2 | Re-key the namespace on the resolved state path | V1, V3–V5 | `c893e4ba` |
| S3 | Source-scan contract banning machine identity in the derivation | V1 | `83f221f2` |
| S4 | Pure `namespace.rs` + `installation.rs` boundary; unified platform rendering; `id -u` deleted and banned; startup wiring | V1–V5, V11 | `eb9e4817` |
| S5 | Doctor provenance: namespace, origin, originating state path, psmux commit | V6, V12 | `2f204690` |
| S6 | Namespace record + loud drift reporting | V2 | `1815d3d1` |
| S7 | Harness stops inheriting `JEFE_NAMESPACE` | isolation | `05d34942` |
| S8 | Fix two expectations only Linux could disprove | CI | `8a01c425` |
| S9 | Review response: remove the places messages could drift from the code | review | `5a2d6c6b` |
| S10 | Namespace record tests clean up after themselves | review | `a4dea5f4` |

Delivered as PR #686. Two failures in S8 were invisible from a Windows host:
cross-target clippy type-checks `cfg(unix)` code but never runs it, and
`tmux_driver_tests.rs` does not compile on Windows at all. Anything touching a
`cfg(unix)` path has to reach CI before it can be called verified.

## 6. Non-goals

- psmux/psmux#509 (stable per-namespace server identity) — upstream.
- Changing the `%LOCALAPPDATA%` state-dir location (#137).
- Fail-closed liveness (#539, separate sub-issue).
- Reaping or migrating sessions already stranded by past drift. They are now
  **reported**, which is the in-scope half; recovery tooling is not.

## 7. Scope ledger

| Item | Status |
|---|---|
| `MultiplexerIdentity` | In scope — mandated by the V8–V12 scope addition. `f6bf52f3`. |
| `src/runtime/namespace.rs` (pure) + `installation.rs` (boundary) | Delivered. Replaces `identity.rs`, which was deleted rather than left alongside. |
| `src/runtime/namespace_record.rs` + `runtime-namespace.json` | Delivered as reporting-only evidence, which is materially narrower than the authoritative-minting store originally proposed. |
| `JEFE_NAMESPACE` override | Reinstated after Q3, on the reasoning in §8. |
| Tightening `windows_ci_signal_contracts.rs` and the uid ban | Permitted: the standing rule forbids *loosening* quality tooling. |
| Harness env scrub | In scope — `--config` only isolates the namespace if the harness cannot inherit an override that outranks it. |
| Coverage/observability findings from this session | Split out as #662, #663, #664. |

## 8. Questions, resolved

- **Q1 — resolved by inverting V8/V9.** See D2. Neither original option was
  right: reporting a rebuild as `Rebound` costs a recovery step every build
  day, and re-keying silently defeats the isolation. Not keying on the binary
  at all removes the dilemma, and the explicit override covers the real A/B
  case.
- **Q2 — resolved.** Quality tooling may be tightened, only not loosened. The
  psmux roster is now derived from the filesystem, and the uid ban lost its
  exemption (verified by mutation, since the exemption covered exactly the file
  the shell-out would return to).
- **Q3 — resolved, reversing the earlier withdrawal.** The earlier reasoning
  said `JEFE_STATE_DIR` already selects the namespace so a second knob would
  contradict the first. That holds for *isolation*, but not for *recovery*: a
  stranded namespace cannot be reached by any state-dir value, because the
  namespace that stranded it was never a function of the state dir. The
  override is therefore kept, made fatal when unusable, and reported everywhere
  it applies — the stickiness objection is answered by visibility, not by
  removing the control.
