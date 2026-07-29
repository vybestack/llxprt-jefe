# Issue 477 delivery plan

## Scope decision

This issue is delivered as **one pull request**. The user explicitly approved
exceeding both the 25-file / 1,500-line target and the 40-file / 2,500-line
hard stop to deliver the complete executable JSP/1 compliance framework in a
single PR. The scope remains disciplined: every file maps to an acceptance row
and there is no speculative hardening, generic abstraction, or unrelated
refactor.

## Resolved design decisions

- **D1 (replay vs. snapshot refresh): current-state JSP/1.** No replay, no
  history, no resume, no `resync_required`. Gaps, reconnects, and epoch
  changes require a fresh snapshot-first stream. This matches the frozen
  contract from PR 495 (specification §16).
- **D2 (concurrent tool identity): headline-tool semantics.** JSP/1 projects
  only the most recently created tool. No tool-call IDs or wire-contract
  changes. The concurrent-tools scenario proves creation-order precedence and
  that an older call's phase change does not replace the headline tool.
- **D3 (executable adapter and HTTP/SSE contract): language-neutral normalized
  adapter transcript.** The producer profile validates a machine-readable
  producer trace. The server profile validates a language-neutral normalized
  HTTP/SSE adapter transcript rather than performing live networking. No
  dependency change. The adapter contracts are documented in
  `producer-contract.md` and `server-contract.md`.

## Acceptance matrix

| ID | Actor | Inputs | Target | Success | Failure | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | CLI / CI invokes schema oracle | Every snapshot/event/heartbeat variant | Machine-readable schemas + Rust oracle | All positive fixtures parse; all negatives reject with correct code | Stable finding names schema invariant, document kind, step | `schema.rs`, schema artifacts |
| C2 | Scenario runner | First snapshot, contiguous, duplicate, gap, identity mismatch, new epoch | Deterministic reducer | Exact-next applies once; duplicate/stale no-op; snapshot replaces atomically | Gap/out-of-order/identity error with offending cursor | `reducer.rs`, scenario fixtures |
| C3 | Projection oracle | Authoritative/inferred/unknown/degraded/empty/unsupported | Normalized projection | Provenance and field states remain distinct in projection | Malformed capabilities fail before mutation | `projection.rs`, S4/S15 |
| C4 | Scenario runner | Turn start/end/fail/cancel, wait open/resolve, todo replace, tool create/phase | Deterministic reducer | Explicit events alone drive state; todo revision increases | Stale revision or illegal transition reports invariant | S2-S8 |
| C5 | Health overlay | Heartbeat, disconnect, reconnect, stale/live | Reducer health | Process/observation/native vary independently | Invalid producer health rejected | `projection.rs`, S9-S12 |
| C6 | Corpus consumer | All 15 scenarios | Language-neutral JSON | Every scenario has expected projection per step | Missing scenario/step fails | `scenario.rs`, manifest |
| C7 | Compliance CLI | schema/reducer/producer/server/all profiles | All platforms | Exit 0 + JSON report on success; nonzero + JSON on failure | Nonzero exit; JSON includes invariant, scenario, step | `jefe-jsp-compliance.rs` |
| C8 | Producer adapter | Identity, events, redaction, bounds, gap | Producer trace | Producer emits bounded redacted docs; gap signal nonblocking | Missing invariant names producer finding | `profile.rs`, producer trace |
| C9 | Server adapter | Registration, publish, stale gen/epoch | Server transcript | Registration binds identity; contiguous publish updates state | Stale/duplicate/out-of-order cannot mutate | `server_profile.rs`, server transcript |
| C10 | SSE observer | Fresh subscription, snapshot-first | Server transcript | First item is atomic snapshot; events contiguous | Stream fails on mixed state | `server_profile.rs`, S9 |
| C11 | Lease | Heartbeat, missed lease, bounds | Server transcript | Heartbeat preserves state; lease marks stale | Lease failure typed; no idle/dead implication | `server_profile.rs` |
| C12 | Roles | Observer publish, publisher observe | Server transcript | Each role accesses only its route | Unauthorized reveals no credential | `server_profile.rs` |
| C13 | Qualification | External adapter | This artifact | Each profile independently passes | Failed profile blocks PoC | Documentation |

## Explicit non-goals

- Full model behavior, prompts, approvals, native terminal rendering, terminal
  scraping, semantic silence heuristics.
- Other agent adapters or subagent-level fixtures.
- Jefe AppState, message bus, production observer networking, status UI, or
  persistence integration.
- Generic HTTP/SSE/subprocess/test-framework/schema-generation abstractions.
- Dependency, workflow, architecture-gate, lint, complexity, source-size, or
  coverage changes.
- `.llxprt` or quality-gate configuration changes.

## TDD evidence (RED / GREEN)

### RED (tests written before production behavior)

All tests were written before or alongside the production modules they
exercise. The key RED evidence:

1. **Schema oracle (C1):** `schema_oracle_accepts_corpus_and_rejects_negatives`
   and `schema_manifest_lists_all_three_document_kinds` — written before
   `schema.rs` existed. RED: no schemas directory, no oracle function.

2. **Reducer (C2-C4):** `reducer_rejects_gap_without_partial_mutation` — written
   before `reducer.rs`. RED: no `ReferenceReducer` type, no `apply_event`
   returning `ReducerError::Gap`.

3. **Scenario corpus (C6):** `scenario_manifest_lists_exactly_fifteen_scenarios`
   and `every_scenario_passes_the_oracle` — written before scenario fixtures
   existed. RED: manifest missing or incomplete, no `ScenarioOracle`.

4. **Producer profile (C8):** `producer_trace_passes_profile` and
   `producer_trace_missing_gap_signal_fails` — written before `profile.rs`.

5. **Server profile (C9-C12):** `server_transcript_passes_profile` and
   `server_transcript_snapshot_first_stream_required` — written before
   `server_profile.rs`.

6. **CLI (C7):** `cli_all_profile_exits_zero_and_emits_pass_report`,
   `cli_schema_profile_exits_zero`, `cli_unknown_profile_exits_nonzero`.

### Corrective RED (remediation checkpoint)

The staged baseline first failed to compile because its new DTO used an invalid
serde rename rule. After restoring compilation, the focused integration target
reported 6 failures: producer timestamp semantics, malformed closed scenario
steps, three accepted negative event cases, a shallow stream mutation, and the
aggregate CLI. Strict Clippy then exposed large DTO variants, overlong semantic
functions, and optional-borrow issues. These failures were retained as concrete
regressions or corrected artifacts before final GREEN.

### First review-cycle remediation (Blocker/In-scope triage)

All convergent review findings were accepted as **Blocker—Fix** or
**In-scope—Fix** under the user's approved one-PR hard-scope expansion:

- **Blocker—Fix:** permissive/corrupt schemas, nested-document bound bypass,
  self-attested producer facts, server false-pass paths, reducer lifecycle and
  atomicity gaps, payload-reflective diagnostics, and non-UTF-8 argv handling.
- **In-scope—Fix:** complete projection parity, closed discriminator branches,
  trusted credential/principal evidence, all 15 extended scenarios, contract
  documentation, checked-in traces, and static-quality decomposition.
- **Reject:** replay/history/resume/resync and live sockets remain outside frozen
  current-state JSP/1.
- **Defer:** none.

Corrective RED tests mutated a canonical schema to permissive, supplied nested
raw documents and irrelevant union fields, altered challenge capacities/ranges,
mutated rejected server responses and SSE state, exercised event/heartbeat
before and after snapshot invalidation, attempted illegal transitions, and
passed a non-UTF-8 path. Each failed before its corresponding production fix.

### GREEN (verified remediation worktree)

```text
$ cargo test --lib jsp::v1::compliance -- --nocapture
running 22 tests
test result: ok. 22 passed; 0 failed

$ cargo test --test jsp_v1_compliance -- --nocapture
running 21 tests
test result: ok. 21 passed; 0 failed

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished successfully with no warnings

$ cargo xtask check source-size
Passed; issue production files are below 750 lines (repository baseline warnings only)

$ cargo xtask check architecture
Passed

$ cargo xtask check clippy-allows
Passed

$ cargo xtask quick
Passed: 2,601 library tests, 804 xtask tests, all integration targets and doctests
```

## Verification commands

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo xtask quick
```

## Review counters

- Local Open Code Review: 0 of 2 used.
- Pull-request Open Code Review: 0 of 2 used.
- Independent review/remediation cycles: 1 of 2 used.

## Scope ledger

| Item | Disposition | Notes |
| --- | --- | --- |
| Standard JSP/1 JSON Schemas and exact parser corpus | Accepted | Complete closed Draft 2020-12 schemas, canonical-byte semantic qualification, and package-local positive/negative parser cases with exact codes |
| Closed compliance DTO boundary | Accepted | Raw nested document bytes route only through authoritative parsers; closed fact/step branches reject irrelevant fields; no weak JSON maps |
| Deterministic harness utilities | Accepted | `FakeClock` and cursor-based `SequenceGenerator`, unit-tested and used by producer/server semantics |
| Pure reference reducer/projection | Accepted | Cursor C means last applied; provenance, seven todo states, and process liveness stay distinct |
| 15 normalized scenarios with per-step expected projections | Accepted | `scenarios/s01-s15.json` |
| Compliance CLI | Accepted | `src/bin/jefe-jsp-compliance.rs` |
| Producer adapter profile | Accepted | `profile.rs`, `producer-trace.json`, `producer-contract.md` |
| Server HTTP/SSE transcript profile | Accepted | `server_profile.rs`, `server-transcript.json`, `server-contract.md` |
| Stable machine-readable failure reports | Accepted | `report.rs` |
| Replay/resume/expiration | Rejected | Current-state JSP/1 (D1) |
| Concurrent tool IDs | Rejected | Headline-tool semantics (D2) |
| Live HTTP networking | Rejected | Normalized transcript (D3) |
| Dependency change | Accepted as unavoidable existing-dependency feature | Enabled serde_json `raw_value` to preserve exact nested document bytes for authoritative 256 KiB ingress checks; no new crate |
| `.llxprt` changes | Rejected | Untouched |
| Workflow / quality-gate changes | Rejected | Untouched |

## Deferred findings


## Slice B remediation: runner-owned challenge execution

Slice B replaces the replayable self-attested producer/server profiles with
**runner-owned challenge execution**. The runner supplies a nonce, challenge
parameters, and an adapter invocation; the adapter's observed output must bind
to the nonce and challenge parameters. An arbitrary `adapter_version`, a
missing chosen marker, fabricated queue arithmetic, or an uncaptured gap
cannot pass.

### New modules

- `challenge.rs`: Closed serializable runner challenge and pure deterministic
  verification. Supplies nonce/version, complete launch identity/process
  binding, redaction and S9 draft sources/markers, exact clock schedule, unique
  sink operation handles/deadline, captured drop interval/next publication, and
  trusted credential/principal inventory. All failure codes are payload-free.
- `reference_adapter.rs`: Built-in deterministic in-process adapter for
  checked-in self-test. Generates producer traces and server transcripts bound
  to the runner's nonce, marker, clock sequence, identity, and capacity
  parameters on each call. A replayed trace from a different nonce cannot
  pass because the nonce is embedded in the adapter's observed output.
- `adapter_invoker.rs`: Bounded subprocess invocation for external adapters.
  Writes challenge JSON to stdin, captures adapter output from stdout with
  size/deadline bounds, and returns payload-free diagnostic codes on failure.

### Modified modules

- `dto.rs`: Added `challenge_nonce: u64` to `ProducerTraceWire` and
  `ServerTranscriptWire`. Extended `ActivityValueWire` with `Unknown`,
  `Degraded`, `Unsupported` variants so lease evidence never maps
  unknown/degraded/unsupported to idle.
- `profile.rs`: Added complete `validate_producer_trace_with_challenge`
  qualification. It binds every observed operation to the runner challenge,
  reduces all captured documents, and requires the real post-drop publication.
- `server_profile*.rs`: Added complete challenge-bound server qualification,
  full identity-triple authentication, distinct unknown/binding/role failures,
  immediate trusted post-rejection digests, atomic SSE tail plus snapshot-only
  proofs, monotonic heartbeats, and complete lease activity
  availability/provenance.
- `src/bin/jefe-jsp-compliance.rs`: Added `--adapter`, `--reference-adapter`,
  and `--nonce` CLI flags for runner-owned challenge execution.

### Updated contracts and fixtures

- `producer-contract.md`: Documented `challenge_nonce` field and runner-owned
  challenge execution protocol.
- `server-contract.md`: Documented `challenge_nonce` field and runner-owned
  challenge execution protocol.
- `producer-trace.json`, `server-transcript.json`: Added `challenge_nonce: 0`
  for backward compatibility with the existing fixtures.

### Adversarial integration tests

`tests/jsp_v1_compliance_slice_b.rs` (34 tests) reproduces nonce binding,
fabricated adapter version/no process, arbitrary absent marker, fake
capacity/uncaptured gap publication, draft leakage, unknown handles versus
partial identity binding, unknown-auth response mutation, rejected-state
mutation or missing immediate digest, missing state-changing SSE tail, missing
snapshot-only evidence, unknown activity lease synthesis, bounded subprocess
failures, and checked reference adapter execution.

### Second review-cycle remediation (Blocker/In-scope triage)

The final bounded cleanup accepted the remaining reducer lifecycle, projection
algebra, scenario-oracle independence, and executable schema findings as
**Blocker—Fix**. API visibility/documentation and the `profile.rs` warning were
**In-scope—Fix**. Replay/history/resume/resync, new wire payloads, and quality
threshold changes remain **Reject** under D1 and the explicit non-goals.
No valid finding was deferred.

The reducer now preserves runtime liveness across source-epoch-only snapshots
and resets it for generation or agent-process identity changes; terminal native
session/tool transitions reject atomically. All 15 fixtures retain native
session/process-binding provenance, S3/S6 execute language-neutral terminal
negative steps, and S9 executes the runner-owned draft challenge. Schema
qualification recompiles mutable artifacts with Draft 2020-12 and independently
checks UTF-8 byte annotations, u32/u64 edges, exact inventories, and symlinks.
`profile.rs` is split at 734 lines.

### Final focused and gate verification

```text
$ cargo test --lib jsp::v1::compliance --locked
 test result: ok. 44 passed; 0 failed

$ cargo test --test jsp_v1_compliance --locked
 test result: ok. 33 passed; 0 failed

$ cargo test --test jsp_v1_compliance_slice_b --locked
 test result: ok. 34 passed; 0 failed

$ cargo fmt --all --check
Passed

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished successfully with no warnings

$ cargo xtask check source-size
Passed

$ cargo xtask check architecture
Passed

$ cargo xtask check clippy-allows
Passed

$ cargo build --workspace --all-features --locked
Finished successfully

$ cargo test --workspace --all-features --locked
Blocked in unrelated `harness_v1_fixtures`: 19 workspace-install failures because
system `/tmp` had only 128 MiB available (`No space left on device`). All focused
JSP suites passed before this environmental failure.
```

Exact-head commit and CI evidence remain for the coordinator.
