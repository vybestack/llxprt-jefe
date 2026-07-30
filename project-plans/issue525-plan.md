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
- After wrapper normalization, the original five-second shared deadline advanced to `AGT-E202 probe timed out`. Raising that single shared deadline to ten seconds was not a reliable repair: the user's still-running Jefe process reports the same timeout, and the contract still gives the capability phase only whatever time remains after identity startup.
- Fresh measurements of the real installed npm wrapper are about 3.1 seconds for `--version` and 3.0 seconds for `--help` in isolation. Exact-head Jefe can complete under light load, but both sequential processes still share one ten-second deadline; cold startup or normal host contention can consume the remainder and fail the second phase.
- The required root fix is therefore per-process deadlines: identity and capability each receive the authored bounded timeout, with an explicit combined ceiling. This preserves fail-closed timeout handling without relying on another magic shared-timeout increase.

## Acceptance Matrix

| ID | Actor / path | Input and boundary | Observable success | Failure behavior / side effects | Behavioral proof |
|---|---|---|---|---|---|
| A1 | Windows command-wrapper probe | Canonical `\\?\C:\...\llxprt.cmd` plus fixed `--version`/`--help` argv | Wrapper path is converted to a launch-safe drive path before `cmd.exe` mediation; probe exits 0 and `core.llxprt` becomes available | Canonical fingerprint remains authoritative; genuine nonzero exits, malformed identity, timeout, and capability failures remain fail-closed | Native Windows command construction/process regression plus real dashboard capture |
| A2 | Windows PowerShell wrapper probe | Canonical `\\?\` `.ps1` path | The script path passed after `-File` is launch-safe without changing fixed argv | Shell selection and fail-closed probe behavior remain unchanged | Focused command-construction regression |
| A3 | Existing persisted state | Valid schema-2 state containing `branch-3` and no settings document | State loads without migration or repair and `branch-3` remains usable | Invalid state still fails through existing diagnostics; no automatic rewrite is added | `config validate`, effective-config output, and real-state launch proof |
| A4 | New isolated Jefe agent | Real installed LLxprt npm trio on Windows | Jefe creates an agent and launches a live `llxprt --yolo --continue` process | Failed probe prevents session creation as before | TUI scenario first, persisted active state, and live process evidence |
| A5 | Existing `branch-3` agent | User selects the persisted agent after availability refresh | Jefe opens the live LLxprt viewer with online status | No unrelated session is killed or rewritten | Real-config psmux capture after fix |
| A6 | Compatibility | Direct executables, normal wrapper paths, canonical fingerprint checks, Unix resolution, package invocation, pane launcher, and bounded local probes | Existing behavior remains green; both probe and actual launch use launch-safe wrapper paths; identity and capability each receive the authored per-process budget under an explicit combined ceiling | No public API, dependency, migration schema, candidate-order, or shell-policy changes; timeouts remain fail-closed | Focused tests and complete exact verification |
| A7 | Issue #518 compatibility | Current-main LLxprt defaults and emitters (`--yolo`, `--continue`) | Probe repair leaves merged #518 behavior intact and real launch argv retains both flags | No definition/form remapping in this PR | Existing #518 tests plus real process argv |
| A8 | Issue #515 lifecycle | Existing Windows psmux/session-host ownership behavior | No lifecycle behavior changes are bundled with the #519 usability repair | #515 remains independently open; no watchdog or orphan cleanup is added | Diff/scope review |
| A9 | Immediate local relaunch | Startup has resolved the LLxprt candidate, but its asynchronous availability probe is still pending | The pre-launch guard permits only this pending resolved evidence to reach the existing authoritative launch-time probe, allowing immediate use | Final NotFound, malformed pending evidence, incompatible, and probe-error observations remain rejected; the launch-time probe still fails closed | Focused RED/GREEN availability tests plus immediate populated-state TUI proof |

## Non-goals

- Do not modify the user's state or create a migration for already-valid schema-2 data.
- Do not weaken identity, capability, timeout, fingerprint, or nonzero-exit validation.
- Do not redesign command resolution, wrapper invocation, probe diagnostics, or process ownership.
- Do not change `.llxprt/`, workflows, dependencies, quality tooling, or unrelated agent definitions.
- Do not implement issue #515; its session-owner watchdog is a separate process-lifecycle subsystem and remains independently open.
- Do not revisit merged #518 launch-option semantics; preserve and regression-check its current-main `--yolo` and `--continue` behavior.

## Vertical Slice

### Slice 1: Launch-safe canonical Windows wrappers (A1-A6)

- Owners: shared wrapper command construction and the private Windows pane-launch boundary.
- Allowed production paths: `src/runtime/agent_probe.rs`, `src/runtime/agent_launcher.rs`, the closed probe timeout contract in `src/domain/agent_definition/{limits,probe}.rs`, and the existing `strip_verbatim_prefix` helper visibility in `src/runtime/agent_executable.rs`.
- Allowed test/evidence paths: focused tests in those modules, fixed migration/hash vectors, the typed issue #382 timeout contract, one issue TUI scenario, and this plan.
- RED 1 (complete): add the launch-level TUI scenario and native Windows tests showing canonical `\\?\` wrapper paths fail through both probe and pane-launch command construction.
- GREEN 1 (complete): convert only wrapper-script launch arguments to non-verbatim drive/UNC paths before shell mediation while retaining canonical fingerprints and direct-executable behavior.
- RED 2 (complete): use a deterministic two-phase probe fixture where identity completes within its authored timeout but consumes enough of the old shared deadline that capability times out; assert the overall probe must succeed when each process individually respects the authored bound.
- GREEN 2 (complete): provide a fresh authored deadline to each process while retaining a finite combined ceiling and existing timeout/nonzero/fingerprint failure behavior.
- RED 3 (complete): immediate relaunch while the resolved startup observation is still pending is rejected as `not installed on the local PATH`, before launch preparation can perform its authoritative probe.
- GREEN 3 (complete): permit only pending observations that retain resolved candidate evidence to continue into launch preparation; keep final or malformed NotFound evidence fail-closed.
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
| LLxprt identity plus capability startup is load-sensitive | Accept / in-scope | Fresh isolated measurements are ~3.1s + ~3.0s, but one shared 10s deadline leaves the second process only a remainder; replace that semantic with bounded per-process deadlines |
| Exact-head succeeds under light load while the user process times out | Accept / in-scope | Confirms a timing-margin defect rather than a persistent config or wrapper-resolution failure; the RED fixture must deterministically consume the shared remainder |
| Issue #518 is merged into current main | Accept as regression guard | Preserve `--yolo` and `--continue`; do not reopen its field/emitter design |
| Issue #515 remains open | Defer / out of scope | Its owner-watch subsystem is not required to repair identity/capability probe timing and would exceed this PR's bounded slice |
| CLI harness uses an isolated config while retaining ambient npm PATH | Accept as evidence | An isolated copy of real state lets exact-head probe the real wrapper without modifying user state; the scenario must prove usability, not merely process creation |
| Pending startup observations encode temporary `Availability::NotFound` | Accept / in-scope | The observation retains both `pending_generation` and resolved candidate evidence, so the generic NotFound guard currently emits a false local-PATH error before authoritative launch preparation |

## Review Counters

- Pre-PR OCR runs: 0 / 2
- Post-PR OCR runs: 2 / 2 (automatic budget exhausted; latest run at `df1eb721` reported no findings)

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
| Probe-budget RED / GREEN | Native Windows wrapper fixture gives identity and capability separate ~2s delays under a 3.5s authored bound; old shared-deadline code failed `AGT-E202 probe timed out`, while fresh per-process deadlines pass with a finite 7s combined ceiling |
| Real LLxprt availability | Exact-head isolated Jefe reports `core.llxprt` installed with identity `0.10.0`; generated forms enable supported operations after availability completes |
| Real agent launch | Exact-head isolated Jefe launched the copied `branch-3` agent through `cmd.exe /D /S /C C:\\Users\\acoli\\AppData\\Roaming\\npm\\llxprt.cmd --profile-load gpt56high --yolo --prompt-interactive --continue` |
| Interactive LLxprt proof | Through Jefe's focused terminal, the real npm-installed LLxprt received `Reply with exactly ISSUE525_INTERACTIVE_OK and no other text.` and returned exactly `ISSUE525_INTERACTIVE_OK`; capture retained under ignored `target/` evidence |
| TUI scenario GREEN | A populated-state harness run launched shortcut 5 immediately with no availability wait, focused the real terminal, sent the prompt, and captured exactly `ISSUE525_INTERACTIVE_OK`; user state and pre-existing sessions were untouched |
| Pending availability RED / GREEN | Focused test reproduced the false `local PATH` rejection for pending resolved evidence; the guard now passes only that state onward, while final and malformed NotFound evidence remain rejected |
| `cargo xtask quick` | Passes: 2695 library tests and all workspace/integration/doctest groups |
| Exact local gates | Formatting, strict all-target/all-feature Clippy, locked all-feature workspace build, and locked all-feature workspace tests pass |
| Exact-head CI / conflicts | Pending |

## Deferred Findings and Follow-ups

- Review confirmed the pending-resolved guard still reaches the authoritative launch probe. The Windows wrapper fixture is gated to Windows; defensive type-level encoding of the existing pending-resolution invariant is deferred as unnecessary hardening for this bounded fix.
- The real proof pane reports the requested branch-3 psmux directory, while LLxprt's own status line reports `C:\\Windows`; classify separately rather than expanding this PR into cwd/session-host behavior.
