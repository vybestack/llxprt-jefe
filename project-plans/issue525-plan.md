# Issue 525 Plan: Make Canonical Windows Wrappers Launchable

## Scope

Fix the confirmed Windows LLxprt identity-probe regression in issue #525. The real user configuration is valid and must remain untouched; the defect is at the boundary between canonical candidate fingerprinting and Windows script-wrapper invocation.

## Diagnosis

- Current main reproduces `core.llxprt (identity probe exited with status 1)` against the real default Jefe state.
- `jefe config validate` reports state schema 2, revision 2403, `migrated_in_memory = false`, and no diagnostics; no settings file is required.
- The persisted `branch-3` agent still uses the shipped `core.llxprt` definition and valid typed launch options.
- The installed npm directory contains `llxprt`, `llxprt.cmd`, and `llxprt.ps1`; `llxprt.cmd --version` exits 0 with attached and null stdin.
- `PathSnapshot` correctly resolves the Windows npm install to `llxprt.cmd`. Candidate fingerprinting then stores `std::fs::canonicalize` output as the executable path, which is `\\?\C:\...\llxprt.cmd` on Windows.
- The probe and launcher pass that canonical verbatim path to `cmd.exe /D /S /C`. `cmd.exe` exits 1 for the `\\?\` path (`The system cannot find the path specified`) while the same command with `C:\...\llxprt.cmd` exits 0.
- The runtime already has audited `strip_verbatim_prefix` handling for canonical Bun/Node arguments; the missing behavior is applying the same launch-safe conversion at wrapper mediation boundaries while retaining the canonical fingerprint for stale-candidate checks.
- After wrapper normalization, the real probe advanced to `AGT-E202 probe timed out`. The installed LLxprt takes about 2.6 seconds for `--version` and 2.7 seconds for `--help`; because both commands share one five-second deadline, normal warm startup consistently exceeds the budget. A ten-second local budget preserves bounded fail-closed behavior while covering sequential identity and capability startup.

## Acceptance Matrix

| ID | Actor / path | Input and boundary | Observable success | Failure behavior / side effects | Behavioral proof |
|---|---|---|---|---|---|
| A1 | Windows command-wrapper probe | Canonical `\\?\C:\...\llxprt.cmd` plus fixed `--version`/`--help` argv | Wrapper path is converted to a launch-safe drive path before `cmd.exe` mediation; probe exits 0 and `core.llxprt` becomes available | Canonical fingerprint remains authoritative; genuine nonzero exits, malformed identity, timeout, and capability failures remain fail-closed | Native Windows command construction/process regression plus real dashboard capture |
| A2 | Windows PowerShell wrapper probe | Canonical `\\?\` `.ps1` path | The script path passed after `-File` is launch-safe without changing fixed argv | Shell selection and fail-closed probe behavior remain unchanged | Focused command-construction regression |
| A3 | Existing persisted state | Valid schema-2 state containing `branch-3` and no settings document | State loads without migration or repair and `branch-3` remains usable | Invalid state still fails through existing diagnostics; no automatic rewrite is added | `config validate`, effective-config output, and real-state launch proof |
| A4 | New isolated Jefe agent | Real installed LLxprt npm trio on Windows | Jefe creates an agent and launches a live `llxprt --yolo --continue` process | Failed probe prevents session creation as before | TUI scenario first, persisted active state, and live process evidence |
| A5 | Existing `branch-3` agent | User selects the persisted agent after availability refresh | Jefe opens the live LLxprt viewer with online status | No unrelated session is killed or rewritten | Real-config psmux capture after fix |
| A6 | Compatibility | Direct executables, normal wrapper paths, canonical fingerprint checks, Unix resolution, package invocation, pane launcher, and bounded local probes | Existing behavior remains green; both probe and actual launch use launch-safe wrapper paths and sequential identity/capability startup has a ten-second shared budget | No public API, dependency, migration schema, candidate-order, or shell-policy changes; timeouts remain fail-closed | Focused tests and complete exact verification |

## Non-goals

- Do not modify the user's state or create a migration for already-valid schema-2 data.
- Do not weaken identity, capability, timeout, fingerprint, or nonzero-exit validation.
- Do not redesign command resolution, wrapper invocation, probe diagnostics, or process ownership.
- Do not change `.llxprt/`, workflows, dependencies, quality tooling, or unrelated agent definitions.
- Do not implement issue #515 or revisit the merged #518 launch-option behavior.

## Vertical Slice

### Slice 1: Launch-safe canonical Windows wrappers (A1-A6)

- Owners: shared wrapper command construction and the private Windows pane-launch boundary.
- Allowed production paths: `src/runtime/agent_probe.rs`, `src/runtime/agent_launcher.rs`, the closed probe timeout contract in `src/domain/agent_definition/{limits,probe}.rs`, and the existing `strip_verbatim_prefix` helper visibility in `src/runtime/agent_executable.rs`.
- Allowed test/evidence paths: focused tests in those modules, fixed migration/hash vectors, the typed issue #382 timeout contract, one issue TUI scenario, and this plan.
- RED: add the launch-level TUI scenario, prove current main cannot select LLxprt, add native Windows tests showing canonical `\\?\` wrapper paths fail through both probe and pane-launch command construction, then prove the five-second shared deadline is shorter than LLxprt's sequential identity/capability startup.
- GREEN: convert only wrapper-script launch arguments to non-verbatim drive/UNC paths before shell mediation, retain canonical executable fingerprints and direct-executable behavior, and raise the bounded local shared probe budget to ten seconds.
- Stop if the fix requires a new resolver abstraction, wrapper policy change, migration, dependency, or process-management subsystem.

## Expected Paths

- `src/runtime/agent_probe.rs`
- `src/runtime/agent_launcher.rs`
- `src/runtime/agent_executable.rs` only to reuse the existing private helper across runtime siblings
- focused module tests
- `dev-docs/tmux-scenarios/issue525/windows-npm-wrapper-launch.json`
- `project-plans/issue525-plan.md`

## Scope Ledger

| Discovery | Disposition | Reason |
|---|---|---|
| Default settings file is absent | Reject as defect | Defaults are valid and recovery validation reports no diagnostic |
| Real state is schema 2 revision 2403 | Accept as evidence | Proves no migration or config repair is needed |
| `jefe` is not installed on the no-profile tool PATH | Reject as root cause | The exact current source binary reproduces the reported probe failure against the real state |
| npm installs extensionless, `.cmd`, and `.ps1` siblings | Accept as evidence | `PathSnapshot` already selects `.cmd`; the defect occurs after canonical fingerprinting |
| Canonical fingerprints use Windows `\?` paths | Accept / in-scope | Preserve the fingerprint, but strip the prefix only for shell-wrapper launch arguments |
| Probe stderr remains summarized on nonzero exit | Defer | Launch-safe wrapper mediation restores operation; diagnostic redesign is outside #525 |
| LLxprt identity plus capability startup is ~5.36 seconds | Accept / in-scope | The two commands share one five-second local deadline; extend the bounded local contract to ten seconds and update its typed/hash vectors |
| CLI harness uses an isolated environment without ambient npm PATH | Reject as product defect | Its scenario preserves the behavioral RED artifact, while real-state psmux and live process evidence prove the production launch path |

## Review Counters

- Pre-PR OCR runs: 0 / 2
- Post-PR OCR runs: 0 / 2

## Verification Evidence

| Check | Result |
|---|---|
| Baseline | `issue525` from `origin/main` at `869f5064` |
| Real config validation | Valid schema 2, revision 2403, no diagnostics or in-memory migration |
| Installed wrapper | `llxprt.cmd --version` exits 0 with attached and null stdin |
| Real Jefe RED | Current main resolves `llxprt.cmd`, canonicalizes it to `\?\...`, and shows identity probe status 1 |
| Exact shell boundary | Normal `C:\...\llxprt.cmd` exits 0 through `cmd.exe`; canonical `\?\C:\...\llxprt.cmd` exits 1 with path-not-found |
| TUI scenario RED | Fails at create step; all LLxprt operations show `no executable candidate resolved` and Create is disabled |
| Focused wrapper RED / GREEN | Both probe and private pane-launch tests failed on canonical `\\?\` argv before implementation and pass with launch-safe wrapper paths; the normalized `.cmd` fixture executes successfully |
| Probe-budget RED / GREEN | Typed budget test failed at 5s versus required 10s; real warm `--version` + `--help` measured 5361ms; test and runtime are green at 10s |
| Real LLxprt availability | Fixed Jefe advanced beyond identity status 1 and the five-second timeout; real-state forms no longer mark `core.llxprt` unavailable |
| Real agent launch | Jefe launched `cmd.exe /D /S /C C:\\Users\\acoli\\AppData\\Roaming\\npm\\llxprt.cmd --profile-load gpt56high --yolo`, with live Bun/LLxprt child processes observed |
| TUI scenario GREEN | Harness remains isolated from ambient npm PATH, so its initial RED is retained as evidence; product launch is proven against the real state and installed wrapper instead of weakening harness isolation |
| `cargo xtask quick` | Passes: 2695 library tests and all workspace/integration/doctest groups |
| Exact local gates | Formatting, strict all-target/all-feature Clippy, locked all-feature workspace build, and locked all-feature workspace tests pass |
| Exact-head CI / conflicts | Pending |

## Deferred Findings and Follow-ups

- None yet.
