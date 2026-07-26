# Issue 264 delivery plan — native Windows diagnostics, packaging, installation, and support

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/264
- Branch: `issue264`
- Base: `origin/main` at `20f6e76`
- Issue state: open
- Review counters: OCR pre-PR 0/2, OCR post-PR 0/2
- Delivery shape: stacked pull requests are required because the issue crosses
  the CLI, diagnostic/process, persistence, Windows runtime, release automation,
  external distribution, documentation, and CI ownership boundaries.
- Status: implementation is blocked on the decisions recorded below.

## Summary

Deliver a native x86-64 Windows support path only after three independently
reviewable capabilities are complete:

1. a redacted, read-mostly `jefe doctor` command that classifies local readiness;
2. a reproducible package/install/upgrade/uninstall path with a clean-Windows
   installation and isolated real-psmux smoke gate; and
3. accurate support and troubleshooting documentation whose public Windows
   support claim is gated on the preceding capabilities.

The CodeRabbit comment is research, not acceptance authority. This plan derives
its rows from the human-authored issue body and verifies proposed implementation
paths against current repository primitives.

## Decisions required before implementation

| ID | Decision | Recommended bounded choice | Why implementation must wait |
|---|---|---|---|
| D-01 | Supported Jefe distribution channel | Publish a portable `x86_64-pc-windows-msvc` zip on GitHub Releases and document a PowerShell per-user installation under `%LOCALAPPDATA%`; install psmux separately with qualified package `marlocarlo.psmux`. Defer a Jefe Winget listing until package identity and `microsoft/winget-pkgs` submission ownership are established. | A Jefe Winget identifier, publisher, repository/fork, credentials, and submission owner do not exist in this repository. Portable release and Jefe Winget are materially different release architectures. |
| D-02 | Workflow changes | Approve bounded edits to `.github/workflows/ci.yml` and `.github/workflows/release.yml` for the Windows package, clean-install doctor gate, and isolated psmux smoke. | The canonical workflow requires explicit approval before changing `.github/`. These edits are required by issue acceptance but cannot be inferred as routine implementation detail. |
| D-03 | License | Confirm Apache-2.0 is the intended project license and approve adding the standard top-level `LICENSE` text matching `Cargo.toml`. | Packaging must contain license metadata, but adding legal text needs maintainer confirmation. |
| D-04 | Doctor exit contract | Exit 0 when all required startup checks pass; exit 2 when psmux is missing/incompatible/untrusted, ConPTY cannot open on Windows, or configured persistence paths are not writable; report missing Git, unauthenticated/missing `gh`, and absent agent runtimes as warnings because they disable features but do not prevent Jefe from starting. Exit 1 only when the diagnostic command itself cannot complete. | The issue requires classifications but does not define which findings make the command fail. CI and user scripts depend on this contract. |
| D-05 | Diagnostic filesystem effects | Do not initialize configuration or mutate state. For an existing config/state directory, create and remove a uniquely named writability probe; for a missing directory, probe the nearest existing parent and report that the application directory is absent. Never touch persistent sessions. | Existing `validate_config_dir` creates a missing directory, so direct reuse would make `doctor` initialize user state. |
| D-06 | CLI/report surface | Add a typed `Doctor` command to the hand-written CLI parser and dispatch it in `main` before logging/TUI initialization. Ship human-readable redacted output only; safe artifacts are collected with shell redirection. Defer `--json` and `--copy`, which the issue does not require. | A parser subcommand and a `main` special case are materially different ownership models. JSON/clipboard would add unaccepted surface and test scope. |
| D-07 | Delivery stack | Use PR 1 for doctor + Windows probes + native CI diagnostic smoke, PR 2 for license/release package + clean package-install scenario, and PR 3 for support docs/status after a package is available. | The canonical workflow mandates splitting work that crosses more than three ownership layers or orchestration routes. |

## Acceptance matrix

| Row | Actor / launch path | Input and boundary | Target | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| AC-01 | Windows user following the supported install path | Clean x86-64 Windows host with PowerShell and Winget; no Jefe or psmux preinstalled | native Windows, local | documented commands install the packaged Jefe executable and qualified `marlocarlo.psmux`, and `jefe --version` launches | command names the failed download/package/PATH step and leaves no partially installed Jefe-owned files | only package-owned staging/install paths | existing config and persistent psmux sessions are untouched | clean-Windows CI installation test executes the documented command path against the built package |
| AC-02 | Windows user launching Jefe | Host has no WSL, Cygwin, MSYS2, Docker, Git Bash, or Unix userland | native Windows, local | Jefe and psmux start through native executables only | compatibility-layer executable resolution is rejected with its path and native remediation | transient process probes only | no runtime fallback behavior is added | Windows CI PATH isolation plus executable-resolution tests |
| AC-03 | Windows user upgrading or uninstalling | Existing package and optionally existing config/sessions | native Windows, local | documented upgrade replaces package-owned files; uninstall removes only package-owned files and PATH registration | refusal or actionable error on in-use/permission failures; no config/session deletion | package-owned files and user PATH entry only | `%APPDATA%`/configured state and psmux sessions survive unless the user explicitly removes them | package install/upgrade/uninstall contract test and support-doc contract |
| AC-04 | User running `jefe doctor` | normal invocation and version metadata built with or without an exact commit | all platforms, local | report includes Jefe version/commit, OS/platform, and architecture | unavailable commit is explicitly reported as unavailable, not fabricated | none | no TUI, logging, config initialization, or session changes | CLI parsing/dispatch tests and report snapshot assertions |
| AC-05 | User diagnosing multiplexer startup | psmux/tmux present, missing, too old, incapable, overridden, or resolved through an unsupported compatibility layer | Windows required; Unix compatibility | report includes resolved path, version, capabilities, and private namespace/socket readiness | blocking status includes exact supported install/upgrade command; rejected PATH/PATHEXT resolution explains the offending path | bounded version/capability subprocesses only | existing multiplexer selection and minimum-version contracts remain authoritative | injected resolver/classification tests plus native psmux doctor smoke |
| AC-06 | Windows user diagnosing terminal support | ConPTY available/unavailable and supported/unknown terminal host environment | native Windows | report identifies terminal host evidence and proves a transient ConPTY allocation can open and close | blocking ConPTY diagnostic includes actionable OS/terminal guidance | one transient pseudo-console allocation, always released | no shell or persistent session is launched | pure classification tests plus Windows-native probe smoke |
| AC-07 | User diagnosing feature prerequisites | Git present/missing; `gh` present/missing/authenticated/unauthenticated; LLxprt Code and Code Puppy present/missing | all platforms, local | each tool/runtime receives a typed pass/warn/fail classification with redacted path/version evidence | missing or unauthenticated optional tools explain the disabled feature and remediation | bounded `--version`/auth subprocesses | no credentials or prompts are modified | injected probe-result tests and native command smoke |
| AC-08 | User diagnosing persistence readiness | explicit/default config and state paths exist, are missing, long, writable, or unwritable | all platforms; Windows long-path detail | report shows structural redacted locations, writability, and relevant Windows long-path policy/limitations | blocking unwritable configured path names the structural path and remedy; long-path risk is a warning | temporary writability file in an existing target/ancestor, removed before return | no settings/state creation or mutation | temp-directory boundary tests and Windows long-path classification tests |
| AC-09 | User attaching a diagnostic report to an issue | home paths, usernames, raw SIDs, credentials, token-shaped values, prompts, URLs with userinfo | all platforms, local | actionable evidence remains while sensitive values are replaced with stable redaction markers | any raw sensitive fixture in output fails the test | none beyond already-authorized probes | same redaction applies before terminal/file output | redaction corpus tests over human report output |
| AC-10 | Release maintainer publishing a version | tagged release with Windows MSVC target | GitHub release automation | release publishes versioned x86-64 Windows package containing Jefe, license, and first-party install metadata only | workflow fails before publication if metadata, checksum, or expected package contents are wrong | release artifacts only | psmux and other third-party binaries are not bundled | release contract test plus workflow artifact inspection |
| AC-11 | CI validating the supported package | clean Windows runner installs package then executes doctor and isolated startup/quit scenario | native Windows CI | packaged executable passes preflight and real psmux startup/quit, then cleanup removes only package-owned files | diagnostic/harness artifacts are uploaded; timeout/failure is bounded and named | isolated config, working directory, namespace, and package staging/install paths | no user config/session cleanup; test namespace is always cleaned | clean-install workflow job using existing schema-1 startup/quit scenario |
| AC-12 | Windows user reading support docs | installation, first launch, terminals, persistence/recovery, antivirus/firewall, paths, clipboard, remote Linux agents, logs, diagnostics | documentation | README/getting-started/building/support/technical overview agree on exact supported behavior and commands | docs contract points to missing or contradictory required section | none | support statement flips only when AC-01 through AC-11 gates are operational | cross-platform docs contract test |
| AC-13 | Contributor qualifying candidate head | all changed source, tests, workflows, package metadata, and docs | Windows and Unix CI | format, policy, Clippy, coverage, build, tests, docs contracts, native Windows install, and psmux smoke pass on exact head | failing gate names the unmet acceptance row | build/test artifacts only | no weakened gates or skipped platform checks | `make ci-check` locally plus required exact-head GitHub checks |

## Explicit non-goals

- No WSL, Cygwin, MSYS2, Docker, Git Bash, or Unix runtime fallback.
- No bundling psmux, Git, GitHub CLI, LLxprt Code, Code Puppy, or MSVC runtime
  binaries inside the Jefe package.
- No MSI, WiX, `cargo-dist`, installer framework, or new dependency unless the
  distribution decision explicitly selects it.
- No Jefe Winget publication in the recommended portable-release path; that is a
  follow-up after package identity and external submission ownership exist.
- No `doctor --json`, clipboard copy, telemetry, automatic repair, package
  installation, authentication mutation, config initialization, or session
  mutation.
- No remote-host probing. Remote Linux setup is documented; doctor diagnoses the
  local Jefe host only.
- No persistence schema, runtime session, TUI, agent lifecycle, or user-keymap
  change.
- No unrelated refactors, test relocation, dependency/quality-gate changes, or
  changes under `.llxprt/` or `.code_puppy/`.

## Planned stacked vertical slices

### PR 1 / S1 — Redacted doctor and native readiness probes

- Rows: AC-04 through AC-09 and the doctor portion of AC-11/AC-13.
- Owners/boundaries: typed CLI parsing; diagnostic domain/orchestrator; existing
  command/runtime/persistence boundary adapters; native Windows CI.
- Allowed paths:
  - `src/cli.rs` and its existing/new focused test target;
  - `src/main.rs`, `src/lib.rs`;
  - new cohesive `src/doctor/` modules and focused tests;
  - narrowly exposed read-only/probe contracts in `src/local_command.rs`,
    `src/runtime/multiplexer.rs`, `src/persistence/`, `src/github/`,
    `src/agent_detection.rs`, or `src/runtime/agent_executable.rs` only when the
    doctor cannot consume an existing typed contract;
  - `.github/workflows/ci.yml` after D-02 approval;
  - `project-plans/issue264-plan.md`.
- RED: parser/dispatch, classification, redaction, persistence-side-effect,
  long-path, and report tests; CI contract asserts doctor invocation before the
  workflow edit.
- GREEN: `jefe doctor` emits the accepted redacted report, returns the accepted
  exit code, creates no config/state/session, and passes on the configured native
  Windows runner.
- Verification: focused doctor/CLI tests, `make quick-check`, then full candidate
  gate and native Windows CI.
- Stop if a new dependency, process-management subsystem, public abstraction,
  TUI route, persistence mutation, or files outside the allowed set are needed.

### PR 2 / S2 — Portable package, installation lifecycle, and clean smoke

- Rows: AC-01 through AC-03, AC-10, AC-11, AC-13.
- Owners/boundaries: release packaging, package-owned PowerShell install boundary,
  existing native psmux harness scenario.
- Allowed paths:
  - `LICENSE` after D-03 approval;
  - `.github/workflows/release.yml` and `.github/workflows/ci.yml` after D-02;
  - a small first-party PowerShell install/upgrade/uninstall script only if D-01
    selects the portable per-user architecture;
  - package/workflow contract tests under the existing integration-test layout;
  - existing `dev-docs/tmux-scenarios/startup-quit.json` without semantic change;
  - plan evidence.
- RED: package-content/install lifecycle contract and workflow contract fail
  before release/install automation exists.
- GREEN: clean native install, doctor, isolated real-psmux startup/quit, and
  package-only cleanup pass; release artifact excludes third-party binaries.
- Verification: PowerShell contract tests, package inspection, full gate, and
  exact-head Windows CI.
- Stop before external Winget publication, a new installer framework, privileged
  system-wide mutation, or deletion outside the package-owned path.

### PR 3 / S3 — Windows support documentation and support-status gate

- Rows: AC-12, AC-13; documents all preceding rows.
- Owners/boundaries: public support docs and docs contract tests.
- Allowed paths: `README.md`, `docs/getting-started.md`, `docs/building.md`,
  `docs/technical-overview.md`, new `docs/windows-support.md`, and the focused
  docs contract test/registration.
- RED: docs contract fails on every required section or command absent from the
  current docs.
- GREEN: all documents agree with the verified package and diagnostic behavior;
  Windows is marked supported only after PR 1/PR 2 exact-head gates and an
  installable release path exist.
- Verification: focused docs contract, `make quick-check`, full gate.
- Stop if docs must claim an unpublished package identifier, unverified terminal,
  or support behavior not proven by preceding slices.

## Expected file budget

| Stack | Expected files | Estimated net lines | Budget state |
|---|---:|---:|---|
| PR 1 doctor | 10–16 | 700–1,200 | target; mandatory review if an existing owner needs broad API expansion |
| PR 2 package | 5–9 | 300–700 plus standard license text | target; `.github` approval required |
| PR 3 docs | 6–8 | 350–650 | target; gated on operational distribution |
| Whole issue | 21–33 distinct | 1,350–2,350 plus license text | must not be collapsed into one PR; mandatory scope review before any stack exceeds 25 files or 1,500 net lines; hard stop above 40 files or 2,500 net lines without approval |

## Scope ledger

| Date | Discovery | Disposition |
|---|---|---|
| 2026-07-26 | Issue #264 is open and authored by `acoliver`; the long CodeRabbit comment is non-authoritative research | In scope: acceptance rows come from the issue body, not optional bot additions |
| 2026-07-26 | No Jefe Winget identity or `microsoft/winget-pkgs` publication owner exists in the repository | Decision D-01: recommend portable GitHub release; defer Jefe Winget publication |
| 2026-07-26 | `.github` changes are required by the clean-install/release acceptance criteria but require explicit workflow approval | Decision D-02 pending |
| 2026-07-26 | `Cargo.toml` declares Apache-2.0 but the repository has no top-level license file | Decision D-03 pending; do not infer legal intent from metadata alone |
| 2026-07-26 | Existing `validate_config_dir` creates a missing directory | Decision D-05: add/use a read-mostly diagnostic probe rather than initialize config |
| 2026-07-26 | The issue does not require JSON or clipboard output | Reject optional expansion; human output redirected to a file is the safe-artifact path |
| 2026-07-26 | The issue crosses more than three ownership layers and orchestration routes | Split into PR 1 doctor, PR 2 package, PR 3 docs per canonical workflow |

## Review triage

No review runs have been spent and no findings are open.

## Verification evidence

- Branch `issue264` was created from current `origin/main` at `20f6e76`.
- GitHub issue #264 is open and has no related implementation PR.
- Planning audit verified existing multiplexer, local-tool, GitHub-auth,
  agent-runtime, persistence-path, logging, identity/redaction, clipboard,
  version/commit, native Windows CI, and schema-1 harness primitives.
- RED/GREEN evidence: pending decisions and implementation.
- Exact-head local gate: pending.
- Exact-head GitHub CI: pending.

## Deferred findings and follow-ups

- A first-class Jefe Winget package is deferred unless D-01 selects it and the
  maintainer supplies the package identifier, publisher identity, external
  submission mechanism, and publication owner.
- MSI/registered installer behavior is deferred; the issue can be satisfied by a
  verified portable per-user package if D-01 is approved.
