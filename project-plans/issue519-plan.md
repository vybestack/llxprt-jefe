# Issue 519 Plan: Restore LLxprt Availability and Launch Defaults

## Scope

Deliver one bounded regression PR for issues #519 and #518. Both regressions were introduced by the definition-driven launch change in PR #501 and prevent a normal LLxprt workflow. Issue #515 is explicitly deferred because it requires a separate Windows process-ownership subsystem and native lifecycle suite.

## Diagnosis

- The installed LLxprt resolves on this Windows machine and both `llxprt --version` and `llxprt --help` exit successfully from PowerShell and `cmd.exe`.
- `cargo run --locked --bin jefe -- doctor` detects LLxprt, but doctor only proves candidate resolution; the dashboard availability worker executes the definition probe.
- `run_probe_process` pipes stdout/stderr but leaves stdin inherited. Under a detached psmux/session-host context, a Windows npm wrapper can receive an unusable inherited console handle and exit non-zero. Sibling non-interactive process boundaries explicitly use `Stdio::null()`.
- The probe failure is converted to `identity probe exited with status 1`, marks `core.llxprt` unavailable, and blocks every local launch.
- The LLxprt definition independently lost the default `yolo = true`, declares no `continue` field/emitter, and maps legacy `pass_continue` to the unrelated `prompt_interactive` field.

## Acceptance Matrix

| ID | Actor / path | Input and boundary | Observable success | Failure behavior / side effects | Behavioral proof |
|---|---|---|---|---|---|
| A1 | Local definition probe | Probe is launched from a parent with readable/inherited stdin, including detached Windows wrappers | Probe child receives EOF on stdin, captures bounded stdout/stderr, and valid LLxprt evidence becomes `InstalledCompatible` | Existing timeout, truncation, non-zero exit, fingerprint, and identity failures remain fail-closed; no launch occurs | Deterministic process-boundary regression plus existing probe suite |
| A2 | Windows LLxprt wrapper | Direct selector resolves the npm `.cmd`/`.ps1` wrapper | `--version` and `--help` probes complete without console-input dependence | A genuine non-zero exit still returns `AGT-E202`; identity checks are not weakened | Focused Windows wrapper/probe test and local source-tree evidence |
| A3 | Generated LLxprt form | New agent with no manual YOLO edit | `Yolo` is visible and defaults to checked; committed typed value is `true` | Other definitions and explicit false remain unchanged | TUI scenario first, generated-form model/submit test |
| A4 | LLxprt launch plan | Committed `yolo = true` or `false` | True emits `--yolo`; false omits it | No product branching in planner | Golden local-plan test |
| A5 | Generated and legacy LLxprt forms | Continue enabled/disabled independently from prompt-interactive | Definition exposes a `continue` boolean and true emits `--continue`; false omits it | Missing capability disables only the corresponding field; unsupported launch remains fail-closed | Generated-form projection and golden local-plan tests |
| A6 | Legacy edit/submit bridge | Existing `pass_continue` checkbox is submitted | Value maps to typed `continue`, not `prompt_interactive` | No Code Puppy or migration behavior changes | Focused state/form regression test |
| A7 | Compatibility | Existing Code Puppy, Codex, Claude, remote, package-selector, and persisted explicit values | Existing behavior remains green | No new dependencies, schema, emitter kind, or public subsystem | Focused regressions plus complete CI suite |

## Non-goals

- Do not accept non-zero probe exits, weaken identity recognition, retry arbitrary CLI failures, or bypass capability requirements.
- Do not change the global generated-form empty value; LLxprt owns its own YOLO default.
- Do not change Code Puppy YOLO or continuation semantics.
- Do not migrate already-persisted explicit `yolo = false` values.
- Do not remove or redesign `prompt_interactive`; make continuation independent.
- Do not add diagnostics unless the stdin fix fails to restore the detached probe and captured stderr is needed for further diagnosis.
- Do not implement issue #515 in this PR. It introduces ancestor identity discovery, an owner watchdog, launcher wiring, and native psmux lifecycle tests across unrelated ownership layers.

## Vertical Slices

### Slice 1: Non-interactive probe stdin (A1-A2)

- Owner: runtime process boundary.
- Allowed paths: `src/runtime/agent_probe_process.rs` and its focused tests.
- RED: a nested probe test supplies parent stdin and asserts the probe child observes EOF.
- GREEN: explicitly configure probe stdin as `Stdio::null()`.
- Stop if deterministic proof requires a new detached-process or multiplexer subsystem, or if the one-line fix does not restore the reported context.

### Slice 2: LLxprt defaults and continuation (A3-A7)

- Owner: shipped definition data and the legacy form-to-typed-value bridge.
- Allowed paths: LLxprt shipped definition, focused form/plan tests, and one issue TUI scenario.
- RED: add the TUI scenario first, then tests proving default checked YOLO, independent Continue, and emitted argv.
- GREEN: set LLxprt's field default, declare the continuation field, add the existing flag emitter, and correct the legacy bridge.
- Stop if this requires a global generated-form change, a new emitter type, persistence migration, or product-specific planner branch.

## Expected Paths

- `src/runtime/agent_probe_process.rs`
- `src/domain/agent_definition/shipped/llxprt.rs`
- `src/state/form_build.rs`
- `src/state/modal_ops.rs` only if loading the corrected legacy `continue` value cannot be handled without it
- `tests/issue382/agent_probe_runtime.rs` or a focused sibling test
- `tests/agent_local_plan.rs`
- `tests/generated_form_model.rs`
- `tests/generated_form_submit.rs`
- focused state form tests
- `dev-docs/tmux-scenarios/issue519/llxprt-launch-options.json`

## Scope Ledger

| Discovery | Disposition | Reason |
|---|---|---|
| Probe drops stderr when formatting non-zero status | Defer | A1 is green; diagnostic expansion is not required to restore operation |
| Existing explicit false YOLO values remain false | Accept / non-goal | Preserve user-authored durable values |
| Definition and typed-value fixed vectors changed | Accept / in-scope | The LLxprt schema default/field and corrected `pass_continue -> continue` bridge intentionally change both canonical hashes |
| Local Windows coverage instrumentation exits while compiling `jefe` without a diagnostic | Defer to exact-head CI | The exact fmt, Clippy, locked build, and locked test gates pass locally; do not change quality tooling in this issue |
| Existing Windows tests require Git `true`/`false` on `PATH` | Reject as issue scope | The same test-only environment requirement exists on `main`; quick/exact tests pass when Git's Unix tools are on `PATH` |
| Issue #515 Windows owner watchdog | Defer to its own PR | New subsystem and unrelated lifecycle acceptance matrix |

## Review Counters

- Pre-PR OCR runs: 0 / 2
- Post-PR OCR runs: 0 / 2

## Verification Evidence

| Check | Result |
|---|---|
| Baseline branch | `issue519`, rebased onto `origin/main` at `88005aba` |
| Installed `llxprt --version` / `--help` | Both exit 0 in attached PowerShell/cmd context |
| Current source `jefe doctor` | Passes and reports LLxprt detected |
| TUI scenario RED | Failed at `Yolo: [x]` before implementation; the form showed unchecked YOLO and no Continue row |
| Probe process RED | Exact nested Windows test failed because the probe consumed the inherited stdin line when `Stdio::null()` was temporarily absent |
| Focused GREEN tests | Probe stdin exact test; 18 local-plan tests; 7 generated-form model tests; 9 generated-form submit tests; 5 issue #317 tests; 25 issue #382 tests |
| TUI scenario GREEN | `llxprt-launch-options.json`: `ok: 15 steps` against the final built binary |
| `cargo xtask quick` | Passes with Git's `usr/bin` on `PATH` for existing Windows tests that invoke `true`/`false` |
| Exact required local gates | `cargo fmt --all --check`, strict workspace Clippy, locked all-feature workspace build, and locked all-feature workspace tests all pass |
| `cargo xtask ci` | Format, policy, source-size, architecture, lint, and complexity pass; local Windows coverage instrumentation exits while compiling `jefe` without a rustc diagnostic, including after canonical cache cleanup and serial compilation |
| Scope | 14 issue files, 464 insertions / 34 deletions including this plan and the scenario; below project limits |
| Exact-head CI / conflicts | Pending PR CI and mergeability check |

## Deferred Findings and Follow-ups

- Issue #515 remains open for a dedicated Windows session-owner watchdog PR.
- Exact-head CI owns the coverage proof because local Windows `cargo-llvm-cov` exits during instrumented `jefe` compilation without a compiler diagnostic; no quality-gate or dependency changes are authorized in this PR.
- If exact-head native Windows evidence shows the stdin fix does not restore issue #519, expose bounded probe stderr before selecting any further behavior change.
