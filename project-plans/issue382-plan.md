# Issue 382 delivery plan — CW-02: complete vertical four-agent definition cutover

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/382
- Branch: `issue382`
- Base: `origin/main` at `53b891c`
- Status: **approved for Claude evidence capture and RED implementation**
- Behavioral authority: issue body, its dynamic-installed-version amendment, and the user's one-PR scope decision.
- Review counters: local OCR 0/2; post-PR OCR 0/2 for the entire issue/PR effort.

## Baseline and delivery-shape finding

This issue cannot fit the normal pull-request budget. The current tree contains more than 570 `AgentKind` references across approximately 90 Rust files, while the issue requires that type to be absent at feature-complete. Product-specific behavior spans domain, persistence, state/reducers, app-input orchestration, selection projections, UI, runtime, harness scenarios, and documentation. It also crosses detection, create, restore, normal, resume, fresh-Issue, fresh-PR, local, remote, preflight, and tmux routes.

The issue therefore crosses more than three ownership layers and more than three orchestration routes. The user explicitly rejected stacked PRs and approved exceeding the 40-file / 2,500-net-line hard stop for one complete PR. The bounded slices below are independently GREEN commits inside that PR; they are not partial PRs, and no slice may ship as a half-complete alternative authority.

Three target files are already near the source limit: `src/domain/mod.rs`, `src/runtime/commands.rs`, and `src/state/form_ops.rs`. Slices touching them must extract cohesive modules rather than append or weaken a gate.

## Decision register

| ID | Approved decision | Consequence / stop condition |
|---|---|---|
| D1 | Deliver the complete issue in one PR; the user explicitly approved exceeding the normal and hard scope budgets and rejected stacked PRs | Use coherent GREEN commits inside the PR. Scope outside the acceptance matrix, an unplanned subsystem/public abstraction, or unrelated cleanup still requires a stop |
| D2 | Install a then-current official Claude Code release and capture SHA-256, version, complete help, probe streams, source URL, and capture date before RED | No mapping may be guessed. If acquisition fails or the reference and installed release disagree, stop before the affected definition mapping |
| D3 | Generalize the Jefe-managed exact-install cache for local npm candidates and retain audited `npm exec` only for remote execution | This preserves issue #425's stronger local architecture while satisfying package-backed selection. Stop if a candidate cannot use the generic managed cache without redesigning it |
| D4 | The issue mandate approves the required `scripts/check-architecture.sh` change | Never raise, bypass, or loosen a quality threshold |
| D5 | Apply the OCR cap to the entire one-PR effort: 0/2 local and 0/2 post-PR | Spend reviews only on stable, fully verified checkpoints |
| D6 | Close identity recognizers during domain-contract RED using exact, prefix/suffix, and version-token typed variants only | Stop if captured outputs cannot be represented without a new dependency or generic regex/string template |

## Acceptance matrix

| Row | Actor / launch path | Inputs and boundary cases | Target / platform | Observable success | Observable failure and diagnostic | Permitted side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| CW02-01 | startup candidate resolver | declared candidate order; symlinks; missing/non-executable entries; path-name containing `/`; blank/nonblank selector; absent runner | local macOS/Linux plus Windows resolver coverage; repository-local typed candidate | first physically valid declared candidate is selected and fingerprinted from one PATH snapshot | typed skipped/rejected reason; no probe when unresolved | metadata/canonicalization only | selected candidate and generation are derived, not persisted as permanent compatibility | `agent-resolver-order.json`; `candidate_resolver_order` |
| CW02-02 | probe adapter/parser | all four captured version/help/probe streams and exact framing | local process; remote parser contract where applicable | recorded identity and bytewise-sorted capabilities reproduced | malformed evidence is not accepted | bounded probe process only | fixture release is provenance, never an allow-list | `agent-probe-parser.json`; `probe_parser_four_agents` |
| CW02-03 | probe adapter/parser | duplicate keys, trailing bytes, UTF-8, line/byte bounds, duplicates, timeout, signal/nonzero exit, identity mismatch, malformed framing | local 5-second and remote 20-second limits; cross-platform process behavior | valid bounded stream parses | `ProbeError` with `AGT-E202` correction | probe process only; no launch/preparation | configured values retained; retry creates a new generation | `agent-probe-negative.json`; `probe_negative_table` |
| CW02-04 | availability gate before launch | compatible identity lacking each required capability | all operations and targets | incompatible status and exact missing-capability reason are visible | `InstalledIncompatible`; launch disabled | zero launch/preparation effects | definition and values retained | `agent-incompatible-zero-spawn.json`; `capability_gate` |
| CW02-05 | status projection/UI | enabled/disabled crossed with NotFound, Compatible, Incompatible, ProbeError | normal, focused, error, small-terminal states | each pair appears exactly once with textual status and create-enabled state | adjacent reason/error code visible | none | durable enablement remains separate from observed availability | `agent-status-cartesian.json`; `status_projection` |
| CW02-06 | supported local submit | all fixture-supported operations, typed values, env allowlist, `OsString` elements | local macOS/Linux plus Windows argv/resolver tests | immutable fixture-golden argv/env/cwd plan | typed validation/support failure before execution | zero runtime effects while planning | plan carries definition/probe/target/signature generations | `agent-local-operation-matrix.json`; `local_plan_golden` |
| CW02-07 | supported remote submit | quotes, empty strings, NUL, non-representable bytes, supported/unsupported cells | existing audited SSH boundary; POSIX remote serialization | one fixture-golden remote transcript using the one serializer | typed rejection or declared unsupported reason | zero SSH for rejected/unsupported input | same definition/generation/signature contracts as local | `agent-remote-operation-matrix.json`; `remote_plan_contract` |
| CW02-08 | generated form and submit UI | every operation/target support cell and exact declared reason | normal/focused/unavailable/small terminal | unsupported choices remain visible and disabled | exact adjacent reason; no hidden fallback | zero preparation | typed values remain durable and dormant unknown values remain untouched | `agent-unsupported-ui.json`; `operation_target_matrix` |
| CW02-09 | preflight boundary | missing engine, image, required env, changed engine fingerprint | local sandbox and remote target paths | successful fixed-argv inspection permits later preparation | `Unavailable { reason }` | zero clone/reset/prompt/SSH/tmux/spawn before success | all durable state retained | `agent-sandbox-preflight.json`; `preflight_order` |
| CW02-10 | confirmed fresh Issue Send | every supported agent; exact issue prompt bytes; failed preflight | supported local/remote cells | exactly one prompt and fixture-golden plan after preflight | typed unsupported/preflight failure | zero prompt/process preparation on failure | fresh policy is declaration-driven, not a product branch | `agent-fresh-issue.json`; `fresh_issue_ordering` |
| CW02-11 | confirmed fresh PR Send | every supported agent; exact PR prompt bytes; failed preflight | supported local/remote cells | exactly one prompt and fixture-golden plan after preflight | typed unsupported/preflight failure | zero prompt/process preparation on failure | fresh policy is declaration-driven, not a product branch | `agent-fresh-pr.json`; `fresh_pr_ordering` |
| CW02-12 | execution boundary | stale executable, probe, target, or activation generation | local and remote | exact current generations execute | `AGT-E203` and recovery action | zero filesystem/clone/prompt/SSH/tmux/spawn side effects | record retained for reprobe | `agent-stale-generation.json`; `generation_property` |
| CW02-13 | schema-1 migration | all current fields, aliases, invalid/unknown kinds and fields, version selectors | local persistence only | known values become typed schema-2 values; unknown raw records are byte-exact dormant data | malformed migration diagnostic without runtime fallback | read/migration only until normal authoritative save | one-way migration imports no runtime type; no dual path | `agent-legacy-migration.json`; `agent_migration_golden` |
| CW02-14 | startup restore and attach | matching/mismatching signatures; live/dead tmux/process evidence; resize/Ctrl-C/F12 | local and remote tmux/PTY paths | matching live launch attaches through existing boundary | stopped/unknown for mismatch or dead evidence | liveness/attach checks only | signature v1 excludes secrets/display-only values | `agent-terminal-compatibility.json`; `local_remote_tmux` |
| CW02-15 | architecture gate | allowed definition/migration/fixture tokens; each forbidden branch and shim-token permutation | source tree and seeded guard fixtures | only explicit allowlist passes and `AgentKind` is absent | gate identifies forbidden path/token | none | schema-1 terminology remains only in one-way migration | `agent-no-product-branches.json`; `agent_architecture_guard` |
| CW02-16 | startup with no Claude candidate | empty PATH for `claude` and blank selector | all platforms | Claude appears once as NotFound and disabled | no fabricated probe or compatibility | zero Claude process effects | definition remains available for a future installed compatible release | `agent-claude-evidence-gate.json`; `claude_entry_gate` |
| CW02-17 | package-backed candidate resolver/planner | selectors for all four definitions; blank selector; sentinels; absent runner; selector change | local managed npm cache / uvx and remote audited runner argv; Windows wrapper coverage | exact package execution plan and new probe generation | typed runner/selector failure before launch | package availability/install boundary only after support/generation validation | existing LLxprt/Code Puppy selectors migrate losslessly | `agent-version-selector.json`; `package_runner_selector` |

## Explicit non-goals

- No new dependency, manifest/workflow change, lint suppression, unsafe code, panic-driven production path, shell template, raw argument field, setup command, or generic JSON escape outside dormant migration records.
- No replacement SSH transport, process manager, timeout/cancellation subsystem, tmux/PTY architecture, provider/plugin system, or persistence authority.
- No runtime schema-1 adapter, alias facade, old/new switch, permanent bridge, or guessed Claude mapping.
- No behavior outside CW02-01..17 and the issue's closed contracts, failure table, UI states, normative documentation mandate, and architecture guard.
- No optional cleanup after accepted evidence, exact-head gates, review triage, conflict checks, and CI are complete.

## Bounded vertical commit slices in one PR

Each production slice starts with the named RED evidence, becomes independently GREEN, runs focused checks and `make ci-check`, and lands as one coherent local commit in the single issue PR. Main is fetched before every slice, and contract-file drift pauses implementation for rebase or true-merge review. No intermediate PR is created.

| Slice | Rows | Owner / integration boundary | Allowed paths | RED evidence | GREEN criterion | Stop conditions |
|---|---|---|---|---|---|---|
| S0 Claude provenance gate | fixture prerequisite | fixture capture only | `tests/fixtures/agent-definitions/claude/**`; this plan | provenance validation fails until capture exists | SHA-256, version, complete help, raw streams, official source URL/date captured from one real release | stop if no real release can be acquired or mapping is absent from either source |
| S1 closed definition contract | enabling: `AGT-E201` failure row | pure `src/domain/` | `agent_type_id*`, `agent_definition*`, existing `sha256.rs`, minimal module wiring | ID/schema/bounds/duplicate/visibility/emitter table tests | strict closed schema and typed diagnostics pass with no I/O | new dependency, weak typing, or unclosed recognizer required |
| S2 candidate resolution | CW02-01 | filesystem/PATH boundary in detection siblings | `src/agent_detection.rs`; focused resolver/registry modules/tests; resolver scenario | `candidate_resolver_order` | deterministic order, typed skips, physical fingerprint, one PATH snapshot | any process spawn required for resolution |
| S3 probe engine | CW02-02,03 | pure parser in domain plus runtime process adapter | focused probe modules/tests; four-agent fixtures; two scenarios | parser and negative table fail for missing behavior | bounded concurrent capture, framing, identity, capabilities, errors, generations pass | S0 incomplete; dependency or new process subsystem required |
| S4 availability/status | CW02-04,05,16 | app-input result mapping, pure projection, thin UI | focused availability/state/projection/screen modules; three scenarios | zero-spawn/cartesian/NotFound UI scenarios fail | all statuses visible once; incompatible/NotFound emit zero launch effects | new modal subsystem required |
| S5 generated form model | enabling typed field/visibility contract | deterministic I/O-free state projection | extracted form type/build/projection/runtime modules/tests | all field-kind/bounds/visibility/order tests fail | definition creates typed forms with no product policy | submission/UI/runtime changes required |
| S6 generated form UI | CW02-08 | app-input intent -> reducer -> pure projection -> thin UI | bounded form/modal/content/screen modules; unsupported scenario | support matrix and focus/reason scenario fail | visible disabled support, exact reasons, trapped/restored focus, zero preparation | source-size gate would need raising; argv planning required |
| S7 local planner | CW02-06 | domain plan value plus runtime assembly | extracted plan/signature/commands modules/tests; local scenario | local golden matrix fails | one immutable plan, ordered element emitters, strict env allowlist, no product match | append to near-limit files; remote/preflight/send behavior required |
| S8 generation guard | CW02-12 | runtime execution boundary using consumed effect contracts | focused launcher/reconciliation modules/tests; stale scenario | generation property table fails | immediate recheck returns `AGT-E203` with zero side effects | new cancellation/process subsystem required |
| S9 remote planner/execution | CW02-07 | existing SSH boundary plus runtime adapter | `src/ssh.rs`; focused target/remote modules/tests; remote scenario | quoting/NUL/zero-SSH table fails | one audited serializer, byte rejection, no definition shell syntax | bypass/new SSH transport required |
| S10 preflight ordering | CW02-09 | runtime preflight plus app-input typed mapping | focused preflight/sandbox modules/tests; scenario | all zero-side-effect failure cases fail | preflight precedes every preparation effect | network pull/build or sandbox semantics expansion required |
| S11 fresh sends | CW02-10,11 | app-input orchestration requests plans | bounded fresh/issues/PR send modules/tests; two scenarios | exact prompt/argv/order scenarios fail | one post-preflight prompt; no product send branch | prompt content changes or new send behavior required |
| S12 package selectors | CW02-17 and selector part of CW02-13 | generic domain selector plus runtime package boundary | selector module; current LLxprt selector/install/probe/capability modules; scenario | four-agent runner/sentinel/fallback/generation matrix fails | one generic selector; local exact managed installs; remote audited runner; lossless selector migration | managed-cache redesign or literal local `npm exec` conflict unresolved |
| S13 one-way migration | CW02-13 | persistence only | migration/schema-1/value/fixture/test modules; migration scenario | full-field aliases/dormant golden fails | lossless typed values and exact dormant records; no runtime import/adapter | runtime type or dual path required |
| S14 restore/tmux compatibility | CW02-14 | app-init reconciliation plus existing runtime boundary | bounded app-init/runtime/session/restore modules/tests; terminal scenario | signature/liveness/resize/input matrix fails | matching live signature attaches; mismatch/dead becomes stopped/unknown | tmux/PTY or process-management redesign required |
| S15 architecture convergence | CW02-15 | mechanical cutover and gate | directory-scoped commit steps for `src/state/**`, `src/app_input/**`, `src/runtime/**`, `src/ui/**` + `src/selection/**`; definition/migration allowlist; `scripts/check-architecture.sh`; scenario | seeded forbidden patterns pass before guard implementation | `AgentKind` absent; product/shim tokens only in exact allowlist | gate weakening, behavior changes, or an intermediate commit that does not compile |
| S16 normative docs | issue documentation mandate | docs only | four `dev-docs/standards/*.md`; `docs/technical-overview.md`; `docs/getting-started.md` | contract comparison identifies stale text | final data flow, support/provenance limits, UI, runtime/persistence, and testing contracts match code | any new behavior proposed by docs |

S15 uses directory-scoped GREEN commits inside the same PR so the mechanical convergence remains reviewable without creating partial pull requests. Every commit must compile and pass its accepted evidence; no commit may leave a second authority or user-selectable compatibility path.

## Verification contract

Per slice:

1. Run the focused test target and relevant TUI scenario, first recording the intended RED failure.
2. Run `cargo fmt --all --check` and focused Clippy/tests during implementation.
3. Run `make quick-check` after GREEN/refactor.
4. Before each green checkpoint is pushed, run unchanged `make ci-check`.
5. For executable/process/shell changes, include Unix structural argv tests, Windows resolver/wrapper coverage, remote escaping tests, and native Windows CI.

Final exact-head evidence:

- all 17 named tests and scenarios pass;
- `scripts/check-architecture.sh` passes with the new guard;
- `make ci-check` passes unchanged;
- no unapproved files are in the diff;
- branch ancestry contains current `origin/main`, and the final PR is conflict-free;
- all local/OCR/CodeRabbit findings are classified Blocker-Fix, In-scope-Fix, Reject, or Defer and dispositioned;
- GitHub checks pass on the exact pushed head.

## Scope ledger

| Date | Discovery | Disposition |
|---|---|---|
| 2026-07-26 | Issue crosses eight ownership areas and more than ten orchestration routes and exceeds the hard scope budget | User explicitly approved one complete PR above the limits and rejected stacked PRs; use bounded GREEN commits inside it |
| 2026-07-26 | More than 570 `AgentKind` references exist across approximately 90 Rust files | CW02-15 convergence uses directory-scoped GREEN commits; no partial PRs |
| 2026-07-26 | `claude` was absent from PATH while the issue requires real release evidence before RED | User approved installing Claude Code; S0 captures evidence before RED and no mapping is guessed |
| 2026-07-26 | Current main includes issue #425's Jefe-managed local npm install cache | Preserve the stronger architecture by generalizing managed local installs; retain `npm exec` only at the remote boundary |
| 2026-07-26 | CW02-15 mandates a quality-gate script change | Approved by the issue mandate and user approval of the whole cutover; no gate may be weakened |
| 2026-07-26 | S1, S5, and S16 are enabling work rather than numbered ledger rows | Approved scope: they own the issue's AGT-E201, generated typed-form, and normative-documentation contracts |
| 2026-07-26 | No dependency change is needed: SHA-256 and bounded parser precedents already exist | Reuse/extract existing pure primitives; stop if a new crate appears necessary |
| 2026-07-26 | S2 candidate resolution adds three new generic sibling modules (`agent_candidate`, `agent_candidate_path`, `agent_candidate_fingerprint`) plus `agent_registry`; each is under the 1000-line source hard limit | Approved S2 scope (CW02-01); no other layer modified |
| 2026-07-26 | S2 `agent_candidate` carries a bounded `Box::leak` selector interner so the resolver can borrow `&'static str` selectors without a lifetime parameter | Bounded by SELECTOR_BYTE_LIMIT and captured once at startup; full interning belongs to S12 (package selectors) |

## Review and deferred-finding ledger

| Item | State |
|---|---|
| Local OCR runs | 0/2 |
| Post-PR OCR runs | 0/2 |
| General review findings | none yet |
| CodeRabbit findings | none yet |
| Deferred findings / follow-up issues | none yet |
| Verification evidence | branch created from `origin/main` at `53b891c`; RED run recorded below |

## RED phase evidence (2026-07-26)

Deliverables completed for the RED phase without any production-source change:
the three missing scenarios, the seventeen-test integration target, the four
captured fixture provenance directories, and a structural-validation gate for
every scenario. Production source, dependencies, scripts, docs, `.github`, and
`.llxprt` were not modified.

### Scenario ledger (all seventeen structurally valid)

Three missing scenarios added under `dev-docs/tmux-scenarios/issue382/`:

- `agent-no-product-branches.json` (CW02-15 architecture guard)
- `agent-claude-evidence-gate.json` (CW02-16 Claude NotFound gate)
- `agent-version-selector.json` (CW02-17 package-runner selector)

The prior interrupted attempt's fourteen scenarios carried two structural
defects that the shipped schema-1 harness parser rejects. Both were corrected
to make all seventeen scenarios conform to the closed grammar:

1. Executable fixture files used mode `365` (`0o555`); the parser only accepts
   file modes `384|420|448|493`. All executable scripts were changed to
   `493` (`0o755`), matching the convention in `dev-docs/tmux-scenarios/v1/`.
2. Five scenarios declared both a fixture file and a `capture` at the same
   path, which the semantic rule rejects as contradictory (a capture
   materializes the capture-shim over any fixture content). Each conflict was
   resolved by following the established `harness-capture.json` convention:
   captures that record process invocations now register the executable and the
   conflicting static fixture file was removed; misplaced post-launch captures
   that duplicated a static identity script were removed in favor of the
   fixture file. Affected: `agent-resolver-order`, `agent-local-operation-matrix`,
   `agent-incompatible-zero-spawn`, `agent-remote-operation-matrix`,
   `agent-version-selector`.

Structural validation (directive 4) was confirmed by parsing every scenario
through `jefe::harness::v1::parse_scenario_v1`: all seventeen parse with no
fixture/capture collisions, mode violations, or step-order errors, and the
on-disk directory contains exactly the seventeen acceptance-matrix scenarios.

### Integration test target

`tests/issue382_behavior.rs` plus helper module `tests/issue382/{mod,fixtures}.rs`
contain exactly the seventeen acceptance-ledger test names plus an
`all_seventeen_scenarios_structurally_valid` gate:

`candidate_resolver_order`, `probe_parser_four_agents`, `probe_negative_table`,
`capability_gate`, `status_projection`, `local_plan_golden`,
`remote_plan_contract`, `operation_target_matrix`, `preflight_order`,
`fresh_issue_ordering`, `fresh_pr_ordering`, `generation_property`,
`agent_migration_golden`, `local_remote_tmux`, `agent_architecture_guard`,
`claude_entry_gate`, `package_runner_selector`.

Each behavioral test (a) parses its scenario through the shipped harness
parser, (b) asserts the exact captured fixture bytes (probe/version streams
contain the recorded identity token; provenance records the typed id,
SHA-256, and verified mappings), and (c) exercises the closed production
contract. The fixture-byte assertions were proven truthful against the real
captured bytes (code-puppy's raw stream interleaves terminal palette control
sequences, so the identity token is matched as a substring the parser
recognizes, per the provenance note). No test is ignored, placeholder,
unconditionally panicking, or fake.

### RED run

Smallest dedicated command and exact output (exit 101):

```text
$ cargo test --test issue382_behavior --no-run
   Compiling jefe v0.0.32 (.../branch-1)
error[E0432]: unresolved import `jefe::domain::agent_definition`
  --> tests/issue382_behavior.rs:30:19
   |
30 | use jefe::domain::agent_definition::{
   |                   ^^^^^^^^^^^^^^^^ could not find `agent_definition` in `domain`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `jefe` (test "issue382_behavior") due to 1 previous error
```

The single compile error is the intended RED: the closed production contract
module `jefe::domain::agent_definition` (`AgentTypeId`, `AgentDefinition`,
`ExecutableCandidate`, `ProbeSpec`, `AgentLaunchPlan`, `Availability`,
`Support`, `Operation`, `Target`, `OperationMatrix`, `Preflight`,
`ProbeErrorCode`, `RemoteTarget`, `CandidateKind`) does not exist on
`origin/main`. GREEN must add the typed domain contract; once present every
test body compiles and asserts the accepted observable contracts. `cargo build
--workspace` passes unchanged, confirming no production code was modified.

## S2 candidate resolution / immutable registry (2026-07-26)

### S2 RED evidence

S2 strengthens the `candidate_resolver_order` integration test
(`tests/issue382_behavior.rs`) to exercise the S2 boundary: it publishes the
shipped `AgentTypeRegistry`, confirms the LLxprt definition's first declared
candidate is the typed `RepositoryLlxprt` kind, builds a repository-local
symlink tree, and asserts the generic resolver selects declared index 0 with a
canonical absolute path and fingerprint against an empty PATH. The focused
unit suites (`agent_candidate_tests`, `agent_candidate_path_tests`,
`agent_candidate_fingerprint_tests`, `agent_registry_tests`) assert each
deterministic property: declaration order, typed skips, repository-local
symlink tree, PATH snapshot, platform/PATHEXT, missing/non-executable
candidates, slash rejection, package-runner blank/nonblank selector
participation, absent-runner typed skip, and canonical-path +
(dev/inode where available, size, mtime) fingerprint.

Before the S2 production modules existed, the strengthened integration and
unit tests failed to compile because `jefe::agent_candidate`,
`jefe::agent_candidate_path`, `jefe::agent_candidate_fingerprint`, and
`jefe::agent_registry` were absent. That compile failure is the S2 RED.

### S2 GREEN evidence

- `cargo test --lib agent_candidate`: 31 passed.
- `cargo test --lib agent_registry`: 10 passed.
- `cargo test --test issue382_behavior`: 18 passed (all seventeen ledger
  scenarios plus the structural gate, with `candidate_resolver_order`
  strengthened to exercise the S2 boundary).
- `cargo fmt --all --check`: clean.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`:
  zero new lints; the only remaining warning is the known pre-existing
  `src/runtime/llxprt_install.rs` `Duration::from_secs(300)` lint (S0-era,
  unrelated to S2).
- `cargo check -q`: clean.
- `cargo test`: full workspace green.

### S2 production surface

New generic sibling modules (no other layer modified):

- `src/agent_registry.rs` — immutable `AgentTypeRegistry` published once at
  the composition boundary; validates every definition, stores them in
  canonical ID order, exposes `definitions()`, `get(id)`, `at(index)`, and
  rejects duplicate type ids. Owns no `AppState`/PATH/process.
- `src/agent_candidate_path.rs` — captured `PathSnapshot` plus the audited
  platform launchable-file policy (Unix executable bit; Windows PATHEXT
  extension order and `.ps1` fallback) reused from the existing
  `AgentExecutableResolver` semantics. Pure filesystem read; never spawns.
- `src/agent_candidate_fingerprint.rs` — `CandidateFingerprint` carrying
  canonical path, device/inode where available, size, and mtime.
- `src/agent_candidate.rs` — generic `AgentCandidateResolver` that consumes
  closed `AgentDefinition` values plus a borrowed `PathSnapshot` and resolves
  the first physically valid candidate in declared order, returning typed
  `CandidateSkip` reasons and a `CandidateFingerprint`. It never spawns and
  owns no mutable registry.

Wiring: `src/lib.rs` declares the four new public modules. `src/domain/mod.rs`
already declared `agent_definition` (S1); no further domain wiring was needed.

### S2 non-goals respected

- No process spawn, probe, planning, UI, persistence, orAppState.
- No new dependency, clippy allow, unsafe, production unwrap/expect/panic, or
  product branch in generic source.
- Product knowledge lives only in the shipped definition data and the typed
  `RepositoryLlxprt` candidate kind; the new modules contain no product tokens.
- No fake `AgentDefinition::resolve_candidate` filesystem method existed to
  remove (the pre-S2 tree had none); the real boundary is the generic resolver.
- Package-runner plan argv belongs to S12; S2 only proves the runner resolves
  and (when requested) fingerprints it.

## S3c/S3d runtime probe execution (2026-07-26)

### S3c/S3d RED evidence

The production-connected `issue382_behavior` target now discovers and executes
all four retained release fixtures through the runtime boundary, and focused
fake-executable tests cover concurrent stdout/stderr draining, typed local and
remote timeout contracts, local timeout, nonzero exit, independent stream
truncation, invalid UTF-8, overlong lines, malformed framing, identity mismatch,
required versus optional capabilities, deterministic stream selection,
NotFound zero-spawn behavior, executable fingerprint change, and requested
probe generations.

Before the runtime adapter existed, the exact focused compile run failed as
intended:

```text
$ cargo test --test issue382_behavior --no-run
error[E0432]: unresolved import `jefe::runtime::AgentProbeTarget`
error[E0432]: unresolved imports `jefe::runtime::AgentProbeResult`,
               `jefe::runtime::run_local_agent_probe`
error: could not compile `jefe` (test "issue382_behavior")
```

This is the S3c/S3d RED: fixture playback and failure-path tests are connected
to the missing production runtime contract rather than a test-only parser.

### S3c/S3d GREEN evidence

- `cargo test -q runtime::agent_probe` passed.
- `cargo test -q --test issue382_behavior` passed all 29 production-connected issue tests.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passed.
- `make quick-check` passed the full quick suite.
- `make ci-check` passed unchanged, including format, policy, source-size, strict stable Clippy, architecture, coverage, locked build, and all tests.

The runtime adapter remains product-agnostic and bounded. It executes only validated definition argv, captures stdout and stderr concurrently with independent limits, applies typed local/remote timeout contracts, rechecks executable fingerprints, and returns generation-bearing availability without touching reducer state or any launch side effect.


## S4 availability/status projection (2026-07-26)

### RED and GREEN evidence

The production-connected status tests initially failed because startup exposed only the two legacy installed kinds and had no four-definition availability projection. S4 now publishes all registry definitions immediately, schedules process probes through the existing post-commit effect worker, applies only matching generation results, and renders a pure textual status projection on the dashboard. Missing definitions remain visible as NotFound; required-capability failures remain visible and disabled with the exact reason; optional capability absence does not disable the agent.

Regression evidence proves startup renders before a hanging probe completes and stale probe completions do not apply. The previously failing real TUI startup, restart, and sticky-kill scenarios pass after moving probe execution out of app initialization. `cargo test -q --test issue382_behavior` passes 30 tests, strict workspace Clippy passes, `make quick-check` passes, and unchanged `make ci-check` passes.

## S5 generated typed form model (2026-07-26)

### S5 RED evidence

A focused integration target, `tests/generated_form_model.rs`, now exercises the
production form-generation boundary for all seven field kinds, declaration
order, defaults and metadata, typed reducer edits/toggles, visibility and focus,
validation, active/signature value projection, unknown IDs, and typed disabled
capability reasons. Before production implementation, the exact focused run
failed as intended:

```text
$ cargo test --test generated_form_model --no-run
error[E0432]: unresolved import `jefe::state::generated_form`
 --> tests/generated_form_model.rs:6:18
  |
6 | use jefe::state::generated_form::{
  |                  ^^^^^^^^^^^^^^ could not find `generated_form` in `state`
error: could not compile `jefe` (test "generated_form_model") due to 1 previous error
```

This is the S5 RED: tests are connected to a missing production projection and
reducer module rather than duplicating form policy in test helpers.

### S5 GREEN evidence

`GeneratedForm` now projects validated definition fields in declaration order into typed drafts, preserves hidden values while excluding them from active/signature projections, derives visible focus order, exposes unavailable capability reasons without hiding fields, and applies typed edits without I/O or product branches. `cargo test -q --test generated_form_model` passes all focused tests, the issue behavior target remains green, strict workspace Clippy passes, and both `make quick-check` and unchanged `make ci-check` pass.

## S6 generated New Agent UI (2026-07-26)

### S6 RED evidence

`tests/generated_form_ui.rs` and the schema-1 real-PTY scenario
`dev-docs/tmux-scenarios/issue382/agent-unsupported-ui.json` were authored before
the production UI route. The typed tests initially failed because no
`GeneratedAgent` modal state or generated form intent path existed. After the
state route was added, the real scenario still failed waiting for `New Agent`:
trace evidence proved Enter emitted `OpenAgentTypeForm` and the reducer committed
`ModalState::GeneratedAgent`, but `ui::orchestration::build_modal_element` had no
render arm for that modal. That isolated the intended rendering-boundary RED.

### S6 GREEN evidence

The dashboard now opens one definition-generated modal through typed input,
message conversion, and deterministic reducer intent. A pure iocraft-free
projection is the textual authority for operation/target support, all typed
fields, exact unavailability reasons, and Create/Back actions; a thin generated
screen renders it with selection support and bounds body rows to keep disabled
actions visible in a 54x16 terminal. Unsupported focus and activation are inert,
perform no third Claude invocation or preparation write, and Esc restores the
exact prior Agent Types focus/index. Backward navigation now dispatches the same
generated-form intent path as forward navigation. The real scenario passes from
startup probe through wide/narrow rendering and focus restoration. Focused UI
tests pass, `make quick-check` passes, and strict workspace/all-target Clippy
passes. Full exact-head `make ci-check` is recorded below after this ledger
update.

## S7 immutable local launch planning (2026-07-26)

### S7 RED evidence

`tests/agent_local_plan.rs` and the production-connected `local_plan_golden`
acceptance test were added against the missing `runtime::agent_plan` boundary.
The focused target initially failed to compile because no typed local planner,
request, field-value collection, or outcome contract existed.

### S7 GREEN evidence

A definition-generic pure planner now validates current compatible probe evidence
and generations, resolves local operation/target support before preparation,
canonicalizes the local working directory, validates typed field values, and
emits `OsString` argv/env elements in definition declaration order. The
environment starts empty and accepts only explicit typed environment emitters;
unsupported or invalid input returns a typed zero-effect outcome. Every plan is
stamped with type ID, definition SHA-256, executable, probe/target generations,
preflight contract, typed-value hash, target fingerprint, and signature. The
focused 17-test planner matrix and production `local_plan_golden` pass for all
four shipped definitions; strict workspace/all-target Clippy and `make
quick-check` pass. Remote serialization, execution, stale recheck, preflight
side effects, fresh-send orchestration, migration, and package-cache
specialization remain outside this slice.