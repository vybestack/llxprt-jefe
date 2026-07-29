# Issue #461 — Harden the Windows installer

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/461
- Branch: `issue461`
- Base: `origin/main` at `f08fa31`
- Issue state: open
- Delivery shape: one bounded issue-closing pull request
- Review counters: OCR pre-PR 0/2, OCR post-PR 1/2
- Status: candidate implementation complete; local verification and review evidence are recorded below

## Decisions required before implementation

| ID | Decision | Proposed bounded choice | Status |
|---|---|---|---|
| D-01 | Workflow changes | Update `.github/workflows/release.yml` so the portable zip includes an inner `jefe.exe.sha256`, while retaining the existing checksum beside the zip. Update `.github/workflows/ci.yml` so the Windows package fixture includes that checksum and the native Windows job runs the Pester harness with the hosted runner's preinstalled Pester 5.9.0. | **APPROVED 2026-07-28 by user continuation instruction** |
| D-02 | External PATH edits | Treat the existing per-install-path named mutex as the atomicity boundary for concurrent Jefe invocations. Each add/remove operation performs one user-PATH read, computes from that snapshot, and performs at most one user-PATH write. Document that Windows exposes no compare-and-swap for this value, so unrelated software can still race with the read/write pair and should not edit the user PATH concurrently with the installer. | **APPROVED 2026-07-28 by user continuation instruction** |
| D-03 | Backup retention | Sweep only installer-owned sibling directories matching `<InstallDir>.backup-*` whose last-write time is at least seven days old. Run the sweep after acquiring the install mutex. Preserve fresh, unowned, or malformed directories; warn and continue when a candidate cannot be validated or removed. | **APPROVED 2026-07-28 by user continuation instruction** |
| D-04 | PowerShell test compatibility | Run the repository Pester script with the Windows runner's preinstalled Pester 3.4.0 under Windows PowerShell 5.1, matching the installer's documented minimum host. Do not add, vendor, install, or pin a new module dependency. | **APPROVED 2026-07-28; corrected after exact CI evidence on 2026-07-29** |

## Acceptance matrix

| Row | Actor / launch path | Input and boundary cases | Target | Observable success | Observable failure / diagnostic | Permitted side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| AC-01 | Windows user running `Install`, `Upgrade`, or `Uninstall` | Empty PATH; trailing semicolon; trailing directory separators; case variants; duplicate owned entries; near-limit PATH; two Jefe processes for the same normalized install path | Native Windows, local | Each PATH mutation reads once and writes at most once; all matching owned entries are removed on uninstall; the named mutex serializes Jefe operations | PATH-length rejection names the safe Windows limit and leaves PATH unchanged | Current-user PATH only; no system PATH mutation | Existing `.jefe-installed.pathAdded` ownership semantics remain compatible | Pester PATH fixtures plus deterministic two-process mutex/install fixture; existing Rust installer contracts |
| AC-02 | Unrelated Windows software editing user PATH while Jefe runs | External edit between Jefe's one read and optional one write | Native Windows, local | Installer minimizes the race window and documents its concurrency boundary accurately | Documentation states that unrelated simultaneous PATH edits can still be overwritten because Windows has no compare-and-swap environment-variable API | Current-user PATH only | No registry schema or machine-wide coordination mechanism is introduced | Windows support documentation contract and Pester proof that each operation uses one snapshot |
| AC-03 | Installer startup under its per-target mutex | Fresh and stale sibling backups; owned, unowned, malformed, locked, and removal-failing candidates | Native Windows, local | Owned backups at least seven days old are removed; fresh and unsafe candidates remain; cleanup failures warn without hiding the requested lifecycle action | Warning identifies a skipped/unremovable backup; action continues | Deletion is limited to stale sibling backup directories with valid Jefe ownership metadata | Restorable recent backups and foreign directories are preserved; retained rollback backups become cleanable after the threshold | Pester filesystem fixtures for stale/fresh/foreign backups and cleanup failures |
| AC-04 | Installer staging a portable package | Checksum absent; valid bare hash; valid conventional `<hash>  jefe.exe`; malformed hash; mismatch | Native Windows, local and release package | When `jefe.exe.sha256` exists, staged `jefe.exe` SHA256 is verified before `--version`; packages without a checksum remain accepted for compatibility | Malformed or mismatched checksum aborts before staged executable launch and names expected/actual context without publishing the stage | Package-owned stage directory only; normal failure cleanup removes it | Existing extracted packages without an inner checksum still install; new releases include the checksum | Pester deterministic fixtures proving absent/valid/malformed/mismatch behavior; release and CI package contracts |
| AC-05 | Release maintainer packaging Windows portable assets | Windows MSVC release artifact | GitHub Actions release workflow | Zip contains `jefe.exe`, `jefe.exe.sha256`, `LICENSE`, and `jefe-install.ps1`; existing outer zip `.sha256` is still emitted | Workflow/package contract fails if the checksum is absent or contents drift | Release staging/assets only | Portable-release distribution decision and third-party exclusion remain unchanged | Rust workflow contract plus exact-head release-workflow/package inspection where available |
| AC-06 | Installer implementation resolving its binary | Package source and stage paths | Native Windows | `$BinaryName` is derived once from `$AppName` and reused for source validation, copy, checksum, execution, and diagnostics | Missing derived binary produces the existing source error | None | Command/file name remains `jefe.exe` | Pester source-validation fixture and Rust textual contract |
| AC-07 | Contributor validating the installer | PATH, concurrency, rollback, uninstall, checksum, and backup fixtures | Local Windows and native Windows CI | Pester suite passes deterministically without changing the real user PATH or requiring downloaded modules | Pester failure names the behavior and fixture; CI uploads existing Windows diagnostics | Temporary fixture directories and child PowerShell processes only | PowerShell 5.1+ installer remains supported; tests run on local and hosted Pester 3.4.0 under Windows PowerShell 5.1 | RED then GREEN Pester run; CI workflow contract; exact-head Windows job |
| AC-08 | Upgrade encountering a publish move failure; uninstall encountering removal failure | Existing owned install, backup created, move/remove failure | Native Windows | Original install is restored on publish failure; uninstall restores only a PATH entry owned by metadata; temporary stage is removed | Combined rollback failure retains and names the backup; original failure remains observable otherwise | Package-owned stage/backup/install paths and owned current-user PATH entry only | Existing ownership marker and rollback behavior are preserved | Pester deterministic rollback and uninstall fixtures plus existing Rust safety contracts |

## Explicit non-goals

- No change to the portable GitHub Release distribution decision from issue #264.
- No MSI, WiX, `cargo-dist`, Winget publication, installer framework, or new dependency.
- No machine-wide PATH edit, system registry edit, privileged install, or global lock against unrelated software.
- No promise of atomic compare-and-swap behavior against unrelated PATH editors; Windows does not expose that operation through the current API.
- No cleanup of `.stage-*`, backups younger than seven days, backups without a valid Jefe ownership marker, arbitrary sibling directories, configuration, state, or psmux sessions.
- No mandatory checksum for historical/external package fixtures; verification is required when `jefe.exe.sha256` is present.
- No signature/authenticode system, network checksum retrieval, archive downloader, or package-format change.
- No Rust product/runtime, TUI, persistence, or agent-lifecycle change.
- No unrelated refactor, test relocation, dependency manifest, agent-memory, or `.llxprt/` change.

## Bounded vertical slices

### S1 — Atomic Jefe PATH mutation and deterministic lifecycle tests

- Rows: AC-01, AC-02, AC-07, AC-08.
- Owner/boundary: first-party PowerShell installer and its Pester behavior suite.
- Allowed paths: `scripts/jefe-install.ps1`, `tests/powershell/jefe-install.Tests.ps1`, `docs/windows-support.md`, `tests/core/windows_support_contracts.rs`, this plan.
- RED: Pester fixtures fail because add performs duplicate reads and rollback/uninstall behavior lacks harness evidence.
- GREEN: add/remove consume one PATH snapshot and perform at most one write; concurrency, rollback, and uninstall fixtures pass without touching the real user PATH.
- Verification: local Pester, PowerShell parser check, focused Windows support Rust contracts.
- Stop if a global registry lock, public module/API, external test dependency, or files outside the allowed set are required.

### S2 — Owned stale-backup cleanup

- Rows: AC-03, AC-07.
- Owner/boundary: installer filesystem lifecycle under the existing named mutex.
- Allowed paths: `scripts/jefe-install.ps1`, `tests/powershell/jefe-install.Tests.ps1`, `docs/windows-support.md`, this plan.
- RED: stale/fresh/foreign fixture behavior fails before a sweep exists.
- GREEN: only stale, validly owned sibling backups are removed; warnings are bounded and the requested action continues.
- Verification: focused Pester backup contexts and parser check.
- Stop if a background service, scheduler, recursive discovery outside sibling backups, or configurable retention subsystem is required.

### S3 — Staged binary checksum and derived binary name

- Rows: AC-04, AC-05, AC-06, AC-07.
- Owner/boundary: package staging, release packaging, and native Windows package gate.
- Allowed paths after D-01 approval: `scripts/jefe-install.ps1`, `tests/powershell/jefe-install.Tests.ps1`, `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `tests/core/windows_support_contracts.rs`, `docs/windows-support.md`, this plan.
- RED: Pester mismatch/malformed fixtures and package contract fail; staged execution is observable before checksum rejection.
- GREEN: optional inner checksum is copied and verified before execution; release/CI packages contain it; `$BinaryName` owns every binary reference.
- Verification: local Pester, focused Rust workflow contracts, native Windows CI.
- Stop before editing workflows without D-01 approval or introducing downloads, signing, dependencies, or a package-format change.

### S4 — Exact-head qualification and bounded delivery

- Rows: AC-07 plus all preceding evidence.
- Owner/boundary: repository verification/review/PR process; no new product behavior.
- Allowed paths: plan evidence only unless a finding is classified Blocker—Fix or In-scope—Fix and maps to an existing acceptance row.
- GREEN: focused tests, `cargo xtask quick`, `cargo xtask ci`, bounded review, ancestry/conflict checks, and exact-head required CI pass.
- Stop on mainline contract drift, unapproved scope, hard-budget breach, incomplete required verification, or exhausted OCR limits.

## Expected paths and scope budget

| Path | Acceptance mapping | Planned change | Estimated net lines |
|---|---|---|---:|
| `project-plans/issue461-plan.md` | all | Decision matrix, scope ledger, review/verification evidence | +180 |
| `scripts/jefe-install.ps1` | AC-01, AC-03, AC-04, AC-06, AC-08 | PATH snapshot mutation, stale sweep, checksum verification, derived name, testable dispatch | +100 / -35 |
| `tests/powershell/jefe-install.Tests.ps1` (new) | AC-01, AC-03, AC-04, AC-06, AC-07, AC-08 | Deterministic Pester behavior harness | +350 |
| `tests/core/windows_support_contracts.rs` | AC-02, AC-05, AC-07 | Package/workflow/docs contracts | +45 / -10 |
| `.github/workflows/release.yml` | AC-05 | Generate and package inner binary checksum | +5 / -1 |
| `.github/workflows/ci.yml` | AC-05, AC-07 | Run Pester and package checksum fixture | +15 / -3 |
| `docs/windows-support.md` | AC-02, AC-03, AC-04, AC-05 | Package contents, PATH race boundary, backup retention | +20 / -5 |

Expected total: 7 files and approximately 661 net added lines, below the 25-file / 1,500-line target. No hard-scope approval is expected.

## Scope ledger

| Date | Discovery | Disposition |
|---|---|---|
| 2026-07-28 | `main` and `origin/main` both pointed to `f08fa31`; no working-tree changes existed | Accepted baseline; created `issue461` from `origin/main` |
| 2026-07-28 | Issue has one bot planning comment and no additional human requirements | Issue body is acceptance authority; bot comment adds no scope |
| 2026-07-28 | Current release workflow emits a checksum beside the zip, not an executable checksum inside the extracted package | AC-04 cannot protect staged execution from that outer checksum; propose inner `jefe.exe.sha256` under D-01 |
| 2026-07-28 | Current CI package contents are exactly `jefe-install.ps1,jefe.exe,LICENSE` | D-01 must update fixture generation and expected contents together |
| 2026-07-28 | Hosted `windows-latest` image documents preinstalled Pester 3.4.0 and 5.9.0; local host has Pester 3.4.0 | D-04 avoids a dependency/tool installation while permitting local and CI behavior evidence |
| 2026-07-28 | Windows user environment variables expose get/set, not compare-and-swap | D-02 bounds atomicity to concurrent Jefe invocations and requires accurate external-editor documentation |
| 2026-07-28 | `.github` edits are required for released checksums and exact-head Pester evidence | Approved and implemented within the bounded D-01 workflow scope |
| 2026-07-29 | Native Windows CI proved Pester 5.9 discovery under PowerShell 7 cannot compile the executable fixture with `Add-Type -OutputType ConsoleApplication` | **In-scope—Fix:** run the harness with preinstalled Pester 3.4 under Windows PowerShell 5.1, which matches the documented installer minimum and passes all 17 tests without a dependency or fixture rewrite |

## Review triage

A focused GLM-backed pre-PR review completed after the full local gate.

- **Blocker—Fix:** none.
- **In-scope—Fix:** remove the stale generated `tests/powershell/red-results.xml` artifact. Resolved; only the intended `.Tests.ps1` remains.
- **In-scope—Fix:** the first Native Windows run failed during Pester 5.9 discovery because PowerShell 7 does not support `Add-Type -OutputType ConsoleApplication`. Resolved by running the unchanged harness with hosted Pester 3.4 under Windows PowerShell 5.1, the installer's supported minimum host.
- **Reject:** PATH, metadata rollback, uninstall, seven-day cleanup, checksum ordering/format, release/CI package, and Rust contract concerns were inspected and found correct against the acceptance matrix.
- **Reject (post-PR OCR inline 3670097408):** replace or explain the test fixture's `31990` PATH length. The test intentionally demonstrates that appending an ordinary absolute install path exceeds the fixed 32,000-character production guard; deriving the value from production text would couple behavioral evidence to implementation detail, while the current boundary remains stable and failed correctly before the guard existed.
- OCR counters remain pre-PR 0/2 and post-PR 1/2.

## Verification evidence

- Baseline: clean `main` at `f08fa31`, equal to `origin/main` at intake.
- Branch: `issue461` created from `origin/main`; merge-base ancestry check passed.
- Issue/comments: fetched with `gh`; one non-authoritative CodeRabbit planning comment.
- Environment: Windows PowerShell 5.1 / local Pester 3.4.0; hosted Windows 2025 image lists both Pester 3.4.0 and 5.9.0.
- RED: local Pester 3.4 run produced 12 intended failures before production changes; workflow/package contracts produced 3 intended failures; the docs hardening contract produced 1 intended failure. The first post-PR Native Windows run also failed before test discovery because PowerShell 7 rejected the executable fixture's `Add-Type -OutputType ConsoleApplication` call.
- GREEN: local Windows PowerShell 5.1 / Pester 3.4 suite passes 17/17 after changing CI to the same supported host; focused Windows support Rust contracts pass 10/10.
- Parser: production installer and Pester harness parse without PowerShell syntax errors.
- `cargo xtask quick` passed after adding the existing Git test utilities to this Windows process PATH; the first attempt reproduced the repository's Windows-only missing `true`/`false` fixture issue without that PATH entry.
- `cargo xtask ci` passed with the same existing Git test utilities on PATH, including format, policy, source-size, architecture, strict/complexity Clippy, coverage, locked build, and locked tests.
- Focused GLM pre-PR review found no blockers; its one in-scope generated-artifact finding was fixed.
- Exact-head verification will be rerun after commits and before push/PR creation.

## Deferred findings and follow-ups

None at intake.
