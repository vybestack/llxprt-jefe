# Issue 704 plan — publish one atomic workbench candidate

Issue: https://github.com/vybestack/llxprt-jefe/issues/704
Parent: #703
Prerequisite: #397, merged by PR #709 at `8c7a68b1555213f231fa2007d85c96c95a2e10ba`
Branch: `issue704`, created from that merged predecessor

## Outcome

Normal startup constructs every active static declaration and every required persistent provider as one unpublished candidate. It returns either a typed failure after complete candidate cleanup or one indivisible:

```rust
StartupCommit {
    workbench: Arc<PublishedWorkbench>,
    providers: ProviderCoordinator,
}
```

Only the composition root may consume that commit. `AppState`, renderer/input services, runtime PTYs, and the TUI are constructed afterward. Every declaration consumer reads the same immutable `Arc<PublishedWorkbench>` identity. Runtime provider health, model generations, and availability never replace static declarations.

## Current ownership defects

- `src/main.rs` publishes screens and constructs diagnostics, themes, JSP/runtime workers, and eager PTY geometry before provider startup completes.
- `src/workbench/mod.rs` owns `PUBLISHED_REGISTRY`; `screen_registry()` may synthesize compiled defaults.
- `src/startup_providers.rs` converts selected persistent-provider failure into warning/discard/continue behavior.
- `src/app_init.rs` rebuilds shipped agents, rescans packages, and may synthesize default Settings after startup.
- `StartupPersistence`, `AppContext`, `AppState`, the screen global, action/key snapshots, and `ProviderCoordinator` independently retain static authority.
- `ActionRegistrySnapshot::publish_availability` replaces a static registry snapshot when runtime health changes.
- initial runtime/PTY geometry is selected before a committed screen/frame boundary exists.

Reusable owners remain: pure screen/provider composition, strict JSONL supervision and bounds, `run_persistent_startup` rollback, provider process-tree cleanup, schema-2 lossless Settings ownership, and provider-free recovery commands.

## Fixed design decisions

1. Rename the existing settings-only `persistence::settings_publish::PublishedWorkbench` to `PublishedWorkbenchSettings`, with no alias. The cross-layer aggregate owns `PublishedWorkbench`.
2. `PublishedWorkbench` is data-only, has private fields, no `Default`, no global accessor, and no compatibility constructor. It owns effective Settings/provenance, validated agents/instances, one complete package inventory and exact selection, one static action/context/key registry, one validated screen registry, provider/tool descriptors and Ready publication metadata, runtime-availability ownership, and non-degrading static warnings.
3. Candidate construction is process-free. It reads/captures Settings and state without writing, scans packages exactly once, resolves selected owners exactly, validates every active static input, and composes all declarations before spawn.
4. A selected enabled persistent provider is required exactly when it owns any active validated `config`, `actions`, `panels`, `routes`, or `screens` declaration. Metadata/defaults alone do not make it required. Selected one-shot and declaration-empty persistent providers start zero startup processes.
5. Required providers are preflighted as a complete deterministic package/provider-ID-ordered set before the first spawn, then every provider must complete Configure and Ready. There is no degraded classification.
6. The prevalidated state-import write is the final fallible pre-commit operation. Provider ownership remains local until it succeeds. Failure preserves prior durable bytes and reaps recorded candidate descendants before returning.
7. `ProviderCoordinator` owns process/supervisor/request/health handles only. Immutable provider descriptors and Ready publication are part of the aggregate.
8. Runtime action availability is a generation-bound overlay in `AppState`; it may not replace the aggregate-owned static action graph. Permanent static unavailability cannot be overridden.
9. Provider-free config/plugin/doctor/binding recovery dispatch remains before normal startup. Read-only recovery preserves every durable byte; explicit disable may atomically change only its requested Settings input.
10. Runtime construction occurs only after commit. Initial PTY spawn/restore remains pending until the first committed nonzero descriptor-resolved terminal rectangle. Full #706 layout-generation work is not part of this issue.

## Acceptance matrix

| ID | Required behavior | Failure/side effects | Behavioral evidence |
|---|---|---|---|
| CWR1-00 | Exact selected enabled persistent owners of active declarations require Configure+Ready; one-shot/empty/disabled owners spawn zero. | Missing, ambiguous, unavailable, invalid, or uncontained active selection is fatal before publication. | Closed classification table, exact-version mutations, and executable spawn traps. |
| CWR1-01 | One complete aggregate and coordinator commit precedes runtime, `AppState`, renderer/input services, PTYs, and TUI. | No partial field or independently published snapshot is observable. | Aggregate census, constructor traps, `Arc::ptr_eq`, and success TUI scenarios. |
| CWR1-02 | Every active Settings/agent/action/screen/package/schema input validates before spawn. | Publish nothing, start no provider/runtime/TUI, and preserve durable bytes. | Malformed-fixture matrix, executable traps, and no-frame failure scenarios. |
| CWR1-03 | First/middle/last required providers all complete containment, spawn, Hello, Configure, Ready, and capability validation. | Every phase/EOF/timeout/crash/protocol failure reaps all recorded candidate descendants before typed recovery. | Ordered provider/phase matrix on Unix and native Windows; unrelated sentinel survives. |
| CWR1-04 | One-shot provider descriptors remain available for later invocation. | Startup process count is exactly zero. | Fail-if-spawned executable trap. |
| CWR1-05 | Candidate cleanup completes before `Err` becomes observable. | Root cause and cleanup evidence are retained; no candidate PID/descendant remains. | Resource/PID sentinel and final-import failure tests. |
| CWR1-06 | Production consumers hold the same aggregate identity for process lifetime; structural edits require restart. | No mutation/replacement/default API exists. | Pointer identity, save stability, restart process, and source guards. |
| CWR1-07 | Recovery commands are provider/TUI-free. | Read-only commands preserve all bytes; explicit disable only edits requested Settings atomically. | Contained-home subprocess matrix with provider/tmux traps and byte comparison. |
| CWR1-08 | Renderer, input, reducer, projector, and navigation declaration lookup begins at the committed aggregate. | No process-global/default registry or independently stored static snapshot remains. | Injected non-builtin behavior plus import/call/source guards. |
| CWR1-09 | Post-publication provider crash changes only generation-bound runtime health/model/availability. | Static declarations and aggregate identity remain; no fallback or automatic restart. | Ready-to-crash state/process test, stale generation test, and TUI scenario. |
| CWR1-10 | Immutable evidence maps CWR1-00..10 to fixtures, commands, platforms, production symbols, and exact deletions. | Stale/missing/duplicate evidence fails. | Checked issue704 owner/deletion manifest and semantic mutations. |

## RED → GREEN vertical slices

### S1 — static candidate and strict inventory (CWR1-00, CWR1-02)

Add classification/static-failure RED tests first. Introduce the private aggregate/candidate, mechanical settings type rename, exact package selection, one inventory scan, complete agent/screen/provider/action composition, and typed static failures. No process, durable write, or global publication is reachable here.

### S2 — required-provider transaction and cleanup (CWR1-00, CWR1-03, CWR1-04, CWR1-05)

Add first/middle/last phase/process tests and one-shot traps. Preflight all required candidates, reuse existing protocol bounds and persistent startup, retain temporary supervisor ownership through final fallible work, and remove warning/discard/catalog-only degradation. Do not create a new process subsystem.

### S3 — one commit and composition-root reorder (CWR1-01, CWR1-02, CWR1-05, CWR1-07)

Add construction and recovery traps. Return `StartupCommit`, make the validated import the final fallible step, render typed failure before terminal initialization, and move all normal runtime/TUI constructors after commit.

### S4 — declaration consumer cutover (CWR1-06, CWR1-08)

Add identity and injected-declaration RED tests. Give `AppContext` and `AppState` mandatory aggregate Arcs, migrate every declaration consumer, centralize explicit test-only fixtures, and delete global/default/split static authorities immediately after the last consumer moves.

### S5 — runtime availability and crash state (CWR1-06, CWR1-09)

Add generation/permanent-unavailability/provider-crash tests. Separate immutable action policy from runtime overlay, migrate key/mouse/help/footer/menu/provider surfaces, and prove health/model changes do not replace declarations.

### S6 — postcommit first-frame runtime boundary (CWR1-01, CWR1-02)

Add pending-manager/restore-deferral RED tests. Construct runtime after commit without initial PTY dimensions, configure from the first committed nonzero screen/frame rectangle, then perform restore/attach. Do not introduce #706's full layout-generation abstraction.

### S7 — TUI authority, deletion evidence, and docs (all; CWR1-10)

Add schema-1 macOS/Linux scenarios for atomic success, static failure, required-provider failure, provider crash, and restart publication through `tmux_scenario`. Add them once to the checked execution manifest, update #397 owner evidence hashes, add a separate CWR1 owner/deletion evidence file, and rewrite normative startup/runtime docs.

## Expected path ledger

- Aggregate/transaction: `src/published_workbench.rs`, `src/startup.rs`, focused startup candidate/commit/error/selection modules, `src/lib.rs`.
- Settings/inventory: `src/persistence/settings_publish.rs`, settings document/import/keymap/path/plugin inventory owners, `src/config_owners.rs`.
- Static workbench: `src/startup_screens.rs`, `src/workbench/**` only as required for composition and global deletion.
- Providers: `src/startup_providers.rs`, `src/runtime/provider/{composition,coordinator,persistent,candidate,process_tree}.rs`.
- Actions: `src/domain/action_registry.rs` and focused consumers/tests.
- Context/state: `src/main.rs`, `src/app_init.rs`, `src/state/**` declaration/availability/settings/navigation owners.
- Input/UI/runtime: relevant `src/app_input/**`, `src/services/**`, `src/app_shell*`, `src/screen_layout.rs`, `src/ui/**`, `src/runtime/manager.rs`.
- Tests/TUI/evidence: `tests/issue704*`, provider fixture support, `dev-docs/tmux-scenarios/issue704/**`, checked scenario manifests, `dev-docs/testing/issue704-owner-evidence.json`, normative docs.

No dependency, manifest grammar, persistence schema, `.github`, `.llxprt`, lint/complexity threshold, suppression, or quality-tool change is planned.

## Exact deletion ledger

Delete rather than wrap:

- `workbench::PUBLISHED_REGISTRY`, `publish_screen_registry`, `screen_registry`, `screen_descriptor`, `RegistryAlreadyPublished`, and lazy compiled publication;
- `startup_screens::compose_and_publish` and `main::publish_screen_registry_or_exit`;
- warning-and-continue provider publication, contribution discard, catalog-only failure construction, and empty-on-failure startup;
- duplicate immutable provider catalog/publication authority in `ProviderCoordinator`;
- fragmented `StartupPersistence` static authority and independent `AppContext`/`AppState` settings/package/action/key/screen/provider snapshots;
- `ActionRegistrySnapshot::publish_availability` and whole-snapshot runtime replacement;
- production shipped-agent/default-Settings reconstruction and startup package rescans outside explicit provider-free boundaries;
- Settings-save mutation of committed effective Settings;
- eager precommit PTY dimensions and independent initial panel geometry fallback.

Retain the process-local `ScreenInstanceId` allocator, pure composition, strict provider protocol and cleanup, one-shot invocation spawning, lossless Settings editing for next restart, and provider-free recovery.

## Non-goals

- Provider protocol redesign, new provider/tool catalog, hot reload, automatic provider disable/restart, or fallback runtime.
- Full #706 `LayoutGeneration`, general renderer/control redesign, or individual built-in screen migration.
- Package installation/manifest grammar, persistence schema, dependency, CI workflow, lint, complexity, or quality-tool changes.
- Optional hardening after accepted behavior and exact-head gates pass.

## Hard stops

Stop for user direction if work requires a new process/cancellation subsystem, protocol/schema/dependency/quality-tool change, broad process killing, full #706 geometry architecture, compensating rewrites of user bytes, unrelated renderer/built-in migration, or platform cleanup that cannot prove the recorded process tree is gone. Also stop on mainline drift under `ISSUE-DELIVERY.md`.

## Scope ledger

| Item | Status | Mapping |
|---|---|---|
| Atomic workbench candidate and commit | Accepted | CWR1-00..06, CWR1-08..09 |
| Exact package/provider requiredness | Accepted | CWR1-00, CWR1-02..04 |
| Deferred state import and cleanup-before-error | Accepted | CWR1-02, CWR1-05 |
| Global/default/split authority deletion | Accepted | CWR1-06, CWR1-08 |
| Runtime availability overlay | Accepted | CWR1-09 |
| Initial postcommit PTY boundary | Accepted | CWR1-01, CWR1-02; explicit issue outcome |
| Provider-free recovery | Accepted | CWR1-07 |
| Schema-1 TUI and native Windows evidence | Accepted | CWR1-01..05, CWR1-07, CWR1-09..10 |
| `.llxprt/LLXPRT.md` and local untracked artifacts | Excluded | Pre-existing unrelated workspace content |

## Review counters

- Open Code Review before PR: 0/2.
- Open Code Review after PR: 0/2.
- Independent code/design review cycles: 1/2 (DeepThinker architecture analysis; no findings yet to triage).

## Verification ledger

Focused tests and `cargo xtask quick` run per slice. Before push/PR, run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo xtask ci
```

Run each new macOS scenario through `scripts/run-scenario-manifest.py` and the sole `tmux_scenario` authority. Linux scenarios and native Windows process/cleanup evidence must pass exact-head CI. The issue text names `make ci-check`, but this repository has no Makefile; do not add quality tooling. `cargo xtask ci` plus the explicit required commands above is the repository gate.

## Deferred findings

None.
