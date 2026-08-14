# Issue 397 delivery plan — execute every schema-1 TUI scenario through one runner

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/397
- Branch: `issue397`
- Base: `origin/main` at `c985c4cf`
- Parent: #703; prerequisite: #380
- Review counters: OCR pre-PR 0/2, OCR post-PR 0/2, independent design review 1/2
- Delivery shape: one branch and one PR; bounded green commits, no stacked PRs

## 1. Audited current state

The recursive corpus contains 157 JSON files, and all 157 already declare schema
1. The issue body's earlier count of 88 is the number of root-level files, not the
current recursive schema-1 count. Scenario conversion is therefore a proved no-op.
The remaining defect is execution and ownership:

- `src/harness/v1/runner.rs::run` executes the complete schema-1 grammar through
  a real Unix PTY.
- `src/harness/v1/tmux_runner.rs::run_tmux_v1` is a second authority. It ignores
  launch argv/cwd and rejects write, mkdir, remove, resize, assert-file, restart,
  capture, assert-capture, and non-frame waits.
- `src/bin/tmux_scenario.rs` owns the strict 0/2/4/124 schema-1 CLI but is Unix
  only.
- `src/bin/jefe-tmux-harness.rs`, selected tests, scripts, docs, and CI still use
  `run_tmux_v1`.
- 136 scenarios declare `macos`; 21 declare `linux`; schema 1 admits no Windows
  platform.
- Current Windows scenario steps run macOS/Linux-declared files through the
  partial runner without checking platform. They are not valid schema-1 platform
  evidence. Native psmux product coverage is independently owned by the
  `tests/psmux_*.rs` suites over `src/runtime/**`.
- No Makefile exists; repository aggregate gates are `cargo xtask ...`.

## 2. Fixed architecture decisions

### D1 — sole scenario authority

Required scenario evidence is exactly:

```text
checked execution manifest
  -> real tmux_scenario subprocess
    -> parse_scenario_v1
      -> harness::v1::runner::run
        -> v1 workspace + real PTY/process group + capture + report
```

`run_tmux_v1`, `tmux_runner`, and `jefe-tmux-harness` are deleted. In-process
`run()` tests may retain report-internal assertions; they call the same sole
runner and are not an alternate execution authority.

### D2 — platform truth

A manifest entry is required on exactly the platform declared by its scenario.
The other Unix platform is explicitly unsupported because the scenario declares a
different platform. Windows is explicitly unsupported because schema 1 admits
only macOS/Linux and the surviving full-grammar runner requires a Unix PTY.

Native Windows psmux behavior remains required through its actual owners:
`tests/psmux_*.rs`, the native Windows workspace test, package lifecycle, owned
namespace cleanup, and survivor assertions. The installed startup/quit CI step is
retained by name and rewritten to drive a contained psmux namespace directly; it
does not become a second scenario runner.

### D3 — required CI execution

The Linux subset executes in the required Ubuntu test job. A required
`macos-latest` job executes the macOS subset, with a separate completion gate that
rejects skipped/cancelled/non-successful execution. Optional/manual smoke execution
is not evidence and is removed.

### D4 — complete checked manifest

`dev-docs/testing/scenario-execution-manifest.json` is outside the scenario tree
and lists every recursive scenario exactly once in byte-sorted path order. Each
entry closes:

- path and scenario schema;
- acceptance criterion IDs;
- required OS and explicit reasons for both unsupported OSes;
- exact `tmux_scenario` command/install set;
- required CI job;
- bounded timeout;
- expected exit/report status and declared operation/capture/assertion evidence.

Validation fails for missing, extra, duplicate, unsorted, unsupported-without-
reason, nonexistent install, wrong CI job/platform, stale assertion inventory, or
old-runner command entries.

### D5 — interruption and cleanup

The surviving runner records only identities it owns: the launched PTY process
group and a workspace-contained application multiplexer socket. Signal cleanup is
re-owned under `harness::v1`; SIGINT/SIGTERM cleanup targets only those recorded
identities. Tests retain unrelated sentinel process groups and multiplexer sockets
and prove they survive success, assertion failure, timeout, interruption, resize,
restart, and child failure.

### D6 — import-graph deletion

The retained set is every transitive dependency of `parse_scenario_v1`,
`runner::run`, reports, capture shims, workspace containment, PTY execution, and
owned-socket cleanup. Delete the old binary/runner and any top-level harness driver,
pane-capture, process, error, or signal module whose only remaining consumers are
itself/tests after migration. Do not delete similarly named `src/harness/v1/*`
modules used by the surviving runner.

## 3. Acceptance matrix

| ID | Actor/path | Success | Failure and prohibited side effects | Evidence |
|---|---|---|---|---|
| CW00B-01 | Manifest validator | Exact recursive path set, sorted and uniquely classified with all closed fields | Missing/extra/duplicate/unsorted/stale fields fail before execution | manifest mutations and directory diff |
| CW00B-02 | Required Linux/macOS CI job | Executes each platform subset through a real `tmux_scenario` subprocess and reports exact subset count | Assertion failure, missing binary, skipped job, or count mismatch is red | per-entry report plus completion gate |
| CW00B-03 | Runner cleanup | Success/failure/timeout/interruption/resize/restart/child failure reap only recorded identities | No broad kill; unrelated process/socket remains alive | targeted outcome matrix |
| CW00B-04 | Parser/runner/report | Existing interpolation, capture, and redaction semantics remain exact | Invalid/secret fixture never leaks or silently passes | retained fixture tests plus executable entries |
| CW00B-05 | Platform classifier | Required platform is exact; every exclusion has deterministic reason | Silent skip, platform rewrite, or parse-only pass fails | platform mutations and no-skip guards |
| CW00B-06 | Ownership guard | One parser, one runner, one required CLI; import graph fully classified | Old binary, symbol, invocation, alias, or proved predecessor fails source guard | import/source/path mutation tests |
| CW00B-07 | Package/provider scenarios | Real package install/startup/Ready/navigation/model/event/crash/recovery/cleanup behavior executes | Parsing alone is not evidence | manifest-required package/provider scenarios |
| CW00B-08 | Downstream owner evidence | Criteria map to immutable scenario hashes, commands, platforms, surviving symbols, and deletions | Stale hash/path/symbol fails | checked owner-evidence manifest |

## 4. Bounded vertical slices

### S1 — RED ownership and manifest contracts

- Rows: CW00B-01, CW00B-05, CW00B-06.
- Add manifest DTO/validator tests and executable import/source classification.
- Check in the complete generated manifest; generation is a development aid only,
  not a runtime compatibility path.
- Initial RED proves the old binary/runner/invocations exist and incomplete
  inventory is rejected.

### S2 — GREEN required execution

- Rows: CW00B-02, CW00B-04, CW00B-07.
- Spawn `tmux_scenario` for the current platform subset using exact installs.
- Add required macOS execution/completion jobs and Linux execution to the required
  test path.
- Execute and triage every scenario. Scenario defects may be corrected while
  preserving intent. Product defects are recorded as blockers/follow-ups; product
  behavior is not changed in this issue.

### S3 — RED/GREEN targeted interruption cleanup

- Row: CW00B-03.
- Add outcome/sentinel matrix first.
- Re-own signal cleanup against recorded PTY process group and contained socket.
- No broad process command or second runner.

### S4 — REFACTOR deletion and invocation cutover

- Rows: CW00B-05, CW00B-06.
- Migrate the dashboard reorder scenario into the checked corpus.
- Migrate/delete old scripts after their install semantics are represented in the
  manifest.
- Rewrite native Windows installed startup/quit around an owned psmux namespace.
- Delete old runner/binary and import-proved predecessors.
- Update docs, PR template, CONTRIBUTING, and contract tests without weakening
  Windows or quality gates.

### S5 — owner evidence and exact-head verification

- Row: CW00B-08 and all done criteria.
- Publish immutable owner/deletion evidence.
- Run required real TUI subsets, policy gates, and full exact-head verification.
- Complete bounded review/remediation and rerun exact-head gates.

## 5. Expected path ledger

| Layer | Expected paths | Rows |
|---|---|---|
| Plan/evidence | `project-plans/issue397-plan.md`, checked manifest and owner evidence under `dev-docs/testing/` | all |
| Sole runner | `src/harness/v1/{runner,pty,app_socket,signal_cleanup,mod}.rs`, `src/bin/tmux_scenario.rs` | 02-04,06 |
| Deletion | `src/bin/jefe-tmux-harness.rs`, `src/harness/v1/tmux_runner.rs`, import-proved top-level `src/harness/*` predecessors | 06 |
| Manifest tests | new focused integration test modules under `tests/` | 01-08 |
| Scenarios | `dev-docs/tmux-scenarios/**` only when preserving a red scenario's existing intent or extracting dashboard reorder | 02,04,07 |
| CI | `.github/workflows/ci.yml` and equal-strength workflow contract tests | 02,03,05 |
| Scripts | old issue scenario launchers and tutorial/capture callers | 02,06 |
| Docs | `dev-docs/testing/tmux-harness.md`, PR template, CONTRIBUTING, related docs contracts | 01,02,05,06 |
| Multiplexer contract | only now-unused declarations surfaced by the unchanged bidirectional policy | 06 |

## 6. Scope ledger

| Discovery | Classification | Disposition |
|---|---|---|
| All 157 scenarios are already schema 1 | Current fact | Conversion step is a no-op |
| Schema 1 has no Windows platform and full runner is Unix-only | Required honest platform classification | Windows explicitly unsupported for scenario execution; native psmux suite remains required |
| Current Windows harness runs foreign-platform scenarios through partial runner | Defect in scope | Delete misleading steps; preserve direct native psmux evidence |
| No required macOS CI job exists | Required by CW00B-02 | Add macOS execution and completion jobs |
| Old top-level multiplexer drivers may become import-orphans | Required deletion | Delete only after executable import classification; keep policy unchanged |
| Existing scenarios may fail when first executed | Required triage | Fix scenario intent only; product behavior requires separate issue/decision |
| `.github` changes | Explicitly required by issue body and user instruction to implement #397 | Accepted |
| Dependencies, lint thresholds, suppressions, production UI/workbench behavior | Out of scope | Do not change |

## 7. Non-goals and stopping conditions

No schema redesign, Windows scenario backend, adapter, alias, fallback runner,
dual parser, arbitrary scenario shell, product behavior change, broad process kill,
secret weakening, lint suppression, dependency, or quality-gate reduction.

Stop for user decision only if implementation requires a new general process
subsystem, a schema change, a dependency, product behavior changes to satisfy a
stale scenario, or deletion of a multiplexer contract still used by production.

## 8. Verification

Focused during slices:

```sh
cargo test --locked --all-features --test scenario_manifest
cargo test --locked --all-features --test scenario_cleanup
cargo test --locked --all-features --test harness_ownership
cargo xtask check multiplexer-surface
cargo xtask quick
```

Exact head:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo xtask ci
```

Review: one rustreviewer/DeepThinker cycle and one detached Open Code Review run,
then bounded remediation and final exact-head verification.