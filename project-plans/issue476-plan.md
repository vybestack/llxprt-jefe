# Issue 476 delivery plan

## Scope decision

Issue 476 cannot fit one Jefe pull request. It crosses the protocol/domain,
service/network, shell orchestration, reducer/messages, runtime identity, pure
projection, UI/input, and real-TTY evidence boundaries. That exceeds the
canonical three-owner and three-route split trigger and would exceed the target
of 25 files or 1,500 net changed lines.

Deliver the parent issue through ordered child or stacked pull requests. Each
intermediate Jefe pull request references issue 476 but does not close it. The
LLxprt Code producer and LLxprt Luther broker are external-repository slices;
they do not create implementation files in this repository.

## First Jefe slice: JSP/1 snapshot contract and compliance corpus

The first independently deliverable slice freezes the external semantic and
wire contract. It provides a strict typed snapshot validator and a portable,
language-neutral fixture corpus before either external implementation starts.
It does not connect Jefe to a server.

### Contract decisions requiring approval

1. JSP/1 uses a closed JSON envelope with `schema: 1` and a closed `kind`
   inventory. Unknown schemas, kinds, fields, duplicate fields, wrong types,
   trailing data, and non-integer numeric values fail. There is no legacy or
   unknown-version fallback.
2. Every record carries a Jefe agent ID, positive lifecycle generation, source
   epoch, and cursor. IDs are opaque safe ASCII strings of 1 through 128 bytes.
   Repository, path, agent kind, PID, display name, and native session metadata
   never participate in the live observation key.
3. Source epoch is producer/broker stream identity bound by registration to one
   Jefe agent and lifecycle generation. It is not added to `RuntimeSession`,
   `RuntimeBinding`, persistence, or `LivenessIdentity`. A producer stream can
   restart during one process generation; a Jefe relaunch invalidates the
   registered generation and requires a new source epoch.
4. Process liveness remains Jefe-runtime-owned. A producer reports typed process
   and lifecycle binding metadata, not whether the process is alive or dead.
   Observation health is Jefe-transport-owned. Native activity is source-owned.
   The three axes therefore remain orthogonal.
5. Each required snapshot field uses a closed state algebra: `unsupported`; or
   a supported provenance of `authoritative` or `inferred` combined with
   `unknown`, `known(value)`, or `degraded(last_value, as_of_ms,
   diagnostic_code)`. `stale` is a local observation-health overlay that keeps
   the last accepted field values; it is not producer capability or provenance.
6. Required fields cover source/native-session identity, process binding,
   native activity, current wait, current turn, todos, last displayed assistant
   message, last-created tool call, source terminal/error state, cursor, and
   bridge observation time. Optional current entities use known `null`. Todos
   use known full replacement `{revision, items}`, preserving the distinction
   between known-empty, unsupported, and unknown.
7. Waiting requires an explicit unresolved wait object with a typed reason:
   permission, question, elicitation, choice, user input, or other. Silence and
   elapsed time never create waiting. Last assistant message changes only at a
   native user-visible display or commit boundary. Drafts, hidden content,
   thinking, raw transcripts, tool arguments/output, and command bodies are
   absent from this proof-of-concept contract.
8. Last tool means the most recently created native tool item. Its phase is one
   of proposed, awaiting approval, scheduled, executing, succeeded, failed, or
   cancelled. Todos are full replacement with strictly increasing revision;
   patches are invalid.
9. Source/native timestamps are bounded UTC epoch milliseconds and diagnostic
   only. Source sequence is the ordering authority. Turn runtime carries an
   elapsed-millisecond anchor; Jefe later advances it from local monotonic
   receipt and never subtracts clocks across processes.
10. Ordering authority is `(source_epoch, sequence)`. Snapshot cursor C reflects
    all effects through C. Heartbeats carry C and do not consume a sequence.
    Events consume exactly C+1. Gaps, out-of-order records, old generations, and
    old epochs fail without state application.
11. Atomic replay is normative in this slice and implemented in the event/stream
    compliance slice. A fresh stream begins with an atomic snapshot at C. A
    resume request after N begins with the reconstructed snapshot at N followed
    by N+1 events. If epoch or replay is unavailable, the broker returns coded
    HTTP 409 `resync_required` before opening SSE; the client then makes a fresh
    request. A stream never mixes partial replay with fallback snapshot data.
12. Publisher and observer credentials are out-of-band HTTP authorization
    material, role-separated, loopback-only for JSP/1, and forbidden from
    protocol documents. Closed parsing rejects credential and control fields.
    Diagnostics contain stable code/path/location, never input values. The
    schema has no control operation.
13. Initial inclusive bounds are: snapshot document 256 KiB; IDs 128 bytes;
    todo list 256 entries; todo text 2 KiB; displayed assistant content 16 KiB;
    source diagnostic summary 2 KiB; tool label 256 bytes. Limit-plus-one input
    fails before a contract value is returned.
14. Initial stable error codes are `JSP-E001` closed JSON/syntax/shape,
    `JSP-E002` bound, `JSP-E003` unsupported version/kind, `JSP-E004`
    identity/binding, `JSP-E005` field state, and `JSP-E006` snapshot semantic
    invariant. Parsing returns a typed `Result` and performs no logging or I/O.

## First-slice acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure and diagnostics | Side effects before failure | Persistence and compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | Jefe or an external implementer runs `cargo test --test jsp_v1_snapshot_compliance`; production calls `jsp::v1::parse_snapshot` | Canonical full snapshot at exact allowed schema and field states, including known-empty todos and known-null optional entities | Pure Rust on macOS, Linux, and native Windows | Returns a typed observation snapshot with exact identity, cursor, semantic fields, provenance, and availability | Not applicable | None | Runtime value only; no state/settings write; JSP/1 only | Canonical full fixture and typed equality assertions |
| S2 | Same | Empty, truncated, non-UTF-8, trailing JSON, duplicate/unknown field, wrong type, schema 0/2, each maximum and maximum-plus-one | All targets | Every at-limit fixture is accepted | Returns deterministic `JSP-E001`, `JSP-E002`, or `JSP-E003` with a safe location and no echoed value | None; no partial contract object escapes | No fallback, coercion, dual format, or write | Closed-input and bound fixtures plus table-driven boundary tests |
| S3 | Same | Two LLxprt snapshots in the same repository/path with distinct agent/generation/epoch; zero generation; invalid binding | All targets | Distinct typed live keys remain distinct | Invalid identity returns `JSP-E004`; no snapshot is returned | None | Native metadata cannot collapse keys; no durable identity mutation | Same-worktree identity and invalid-identity fixtures |
| S4 | Same | Unsupported versus supported authoritative/inferred unknown/known/degraded; known-empty, unsupported, and unknown todos; producer-supplied stale state | All targets | Valid states remain distinct exhaustive enum values | Illegal combinations or producer stale state return `JSP-E005` at the field path | None | Unknown future variant requires a version change | Value-state and invalid semantic fixtures plus exhaustive assertions |
| S5 | Same | Explicit wait/activity consistency, elapsed anchor, todo revision, committed message, last-created tool phase, source terminal state; forbidden draft/raw/tool/control fields | All targets | Valid snapshot is accepted without excluded data | Contradiction or forbidden field returns `JSP-E001`/`JSP-E006`; diagnostics contain no payload text | None | No transcript/tool-output persistence; accepted text remains runtime-only in later slices | Full fixture and focused semantic failures |
| S6 | A protocol consumer reads the spec and runs the corpus from another repository/language; Jefe is the reference oracle | Manifest enumerates every fixture and expected result; credential/control fields are attempted in bodies | Language-neutral corpus; Jefe oracle on all targets | Manifest and all fixtures are consumed; auth roles and read-only inventory are explicit | Missing/unlisted fixture fails the integration test; credential/control fields fail closed | Fixture reads only | Corpus is versioned under JSP/1; external implementations must pass it | Manifest-enumeration and credential/control negative fixtures |

## First-slice non-goals

- Event parsing, stream state machine, heartbeat execution, replay execution,
  SSE, HTTP, batching, retry, timers, networking, registration lifecycle, or
  credential storage.
- AppState observation maps, messages/reducers, process-liveness changes,
  runtime-session changes, persistence changes, Split UI/input, terminal viewer
  behavior, or a TUI scenario.
- LLxprt Code or LLxprt Luther implementation and adapters for Claude Code,
  Codex, OpenCode, or Code Puppy.
- Generic remote transport/TLS, sockets, named pipes, WebSockets, late
  attachment, control/nudge/approval answering, subagent rows, or draft
  streaming.
- Refactoring `harness/v1` into shared infrastructure, adding generic JSON
  abstractions, exposing generic `serde_json::Value` maps, redaction fallback
  layers, new dependencies, lint allowances, quality-gate changes, or unrelated
  cleanup.

## Architecture boundaries

- `domain::observation` owns transport-neutral strongly typed semantic values.
  It has no project-internal dependency, I/O, clock, credential, or UI concern.
- `jsp::v1` is the external wire-boundary parser. It depends only on
  `domain::observation`, the standard library, and existing `serde`/
  `serde_json`. Private closed wire DTOs convert only after complete validation.
- `dev-docs/jsp/v1` owns the normative contract and language-neutral corpus.
- Integration and unit tests own observable parser/corpus behavior.
- The architecture standard records the new `jsp -> domain` direction. This
  slice does not modify architecture tooling or quality gates.

## Expected first-slice paths

Target no more than 22 files and 1,450 net changed lines:

- `dev-docs/jsp/v1/specification.md`
- `dev-docs/jsp/v1/fixtures/manifest.json`
- Up to seven focused JSON fixtures for full snapshot, same-worktree identity,
  field states, forbidden/auth fields, closed grammar, bounds, and
  identity/semantic failures
- `dev-docs/standards/architecture.md`
- `src/domain/mod.rs`
- `src/domain/observation.rs`
- `src/jsp/mod.rs`
- `src/jsp/v1/mod.rs`
- `src/jsp/v1/contract.rs`
- `src/jsp/v1/wire.rs`
- `src/jsp/v1/limits.rs`
- `src/jsp/v1/error.rs`
- `src/jsp/v1/validate.rs`
- `src/jsp/v1/parse.rs`
- `src/lib.rs`
- `tests/jsp_v1_snapshot_compliance.rs`

Every file remains below 750 lines, every function below 60 lines, and cognitive
complexity below 15. If complete behavior cannot remain within 25 files and
1,500 lines, split specification/corpus from parser rather than compressing
behavioral evidence or loosening gates.

## First-slice vertical steps

1. RED: add the typed fixture-manifest integration test and canonical fixtures;
   prove the test fails because JSP/1 parser/types do not exist.
2. GREEN: add domain types, private closed wire DTOs, document bound, schema
   gate, typed errors, and enough parsing for valid/closed/version/bound rows.
3. RED: add identity collision, illegal field-state, wait/todo/message/tool, and
   forbidden credential/control failures.
4. GREEN: validate the entire snapshot before conversion; return no partial
   value and echo no payload in diagnostics.
5. REFACTOR only in approved paths: split cohesive modules before 750 lines,
   keep the public API minimal, and remove duplication without extracting or
   modifying harness JSON code.
6. Commit coherent green behavior separately; do not commit a known-red slice.

Focused verification:

```text
cargo test --test jsp_v1_snapshot_compliance
cargo test --lib jsp::v1
cargo xtask quick
cargo xtask check source-size
cargo xtask check architecture
```

Exact-head pre-push verification:

```text
cargo xtask ci
```

Native Windows CI must pass. A TUI gate is not claimed for this non-UI slice.
Interrupted, partial, skipped, or stale-head verification is incomplete.

## Ordered parent-issue slices

1. **J1: snapshot contract/corpus.** This plan's first slice. Does not close
   issue 476.
2. **J2: event and stream semantic compliance.** Add normalized event parsing,
   deterministic stream state machine, exact sequence/epoch/revision rules,
   atomic snapshot-at-cursor replay, heartbeat semantics, 409 resync, and
   portable replay fixtures. No sockets, UI, or AppState. Target at most 20
   files and 1,450 lines. Completion gates external implementation work.
3. **External LLxprt Code child.** Add the non-blocking native producer and
   publisher with explicit native event boundaries and excluded-content rules.
   It must pass the J1/J2 producer corpus. No Jefe files.
4. **External LLxprt Luther child.** Add authenticated loopback POST/SSE broker,
   role separation, bounded event log, atomic snapshot/replay/resync,
   diagnostic endpoint, and whole-batch atomicity. It must pass the J1/J2 broker
   suite. No Jefe files.
5. **J3: runtime-only observation reducer.** Add the identity-keyed AppState
   projection and typed deterministic snapshot/event messages. Reject stale
   generation/epoch with no mutation. No persistence or I/O; do not alter
   `AgentStatus` or `LivenessIdentity`. Target at most 15 files and 1,250 lines.
6. **J4: loopback observer-client boundary.** Add bounded authenticated
   HTTP/SSE transport against a deterministic local fake server, with no
   AppState. Any manifest/lockfile or HTTP/TLS dependency change requires
   separate explicit approval. Do not hand-roll a broad HTTP stack as fallback.
   Target at most 18 files and 1,400 lines.
7. **J5: lifecycle orchestration and local health.** Add registration binding,
   cancellation, render-thread typed delivery, connecting/live/stale/
   disconnected/protocol-error transitions, monotonic freshness/elapsed ticks,
   and resync. Network and timers remain outside AppState. Target at most 20
   files and 1,450 lines.
8. **J6: compact status rows, Enter, and Split retirement.** Add a schema-1 TUI
   scenario first. Retire Split-only repository filter/grab/reorder while
   preserving Dashboard behavior. Add pure row projection, orthogonal
   indicators/headline, and Enter through the existing terminal attach/focus
   path. Target at most 25 files and 1,500 lines; split again if routes or budget
   exceed the policy.
9. **J7: selected-agent detail and freshness.** Add a TUI scenario first and a
   pure detail projection for todos, explicit wait, turn elapsed, last displayed
   reply, last-created tool, provenance, unsupported/unavailable/degraded/stale,
   and as-of age. Keep the iocraft renderer thin. Target at most 18 files and
   1,350 lines.
10. **Cross-repository qualification.** Run the exact J1/J2 corpus against the
    released Code producer and Luther broker, then run a real Jefe TUI scenario
    proving same-worktree identity separation, stale historical display,
    independent process death, and entry into the existing native UI. Issue 476
    closes only after every parent acceptance row has evidence and all exact-head
    delivery gates are complete.

## Scope ledger

| Item | Disposition | Notes |
| --- | --- | --- |
| Normative JSP/1 semantic/transport specification | Accepted for J1 | Written contract includes later transport semantics but no transport implementation |
| Transport-neutral observation domain values | Accepted for J1 | No process-liveness or persistence ownership |
| Snapshot parser/validator and fixture oracle | Accepted for J1 | Existing dependencies only |
| Event/replay implementation | Deferred to J2 | Normative semantics only in J1 |
| LLxprt Code producer | Deferred to external child | Must consume exact corpus revision |
| LLxprt Luther broker | Deferred to external child | Must consume exact corpus revision |
| AppState/reducer/network/UI | Deferred to J3-J7 | Outside J1 acceptance matrix |
| HTTP dependency selection | Deferred; approval required | No manifest/lockfile change in J1 |
| Source epoch in liveness/persistence | Rejected | Observation-stream identity is orthogonal to process liveness |
| Producer-owned process or observation health | Rejected | Violates axis ownership |
| Producer `stale` field state | Rejected | Stale is a local transport-health overlay |
| Accept-and-redact credential/control fields | Rejected | Closed parser rejects them at ingress |
| Terminal/log inference or shared harness refactor | Rejected | Violates issue invariants or J1 scope |
| J1 line budget | Discovered scope item | Target was <=22 files / 1,450 net lines. Actual: 22 impl files / 3,270 net lines (2,308 Rust code + 962 docs/fixtures). Code is under the 2,500 hard limit. Overage is from the normative specification and fixture corpus, which the plan explicitly identifies as splittable. No behavioral evidence compressed or deleted. All S1-S6 GREEN, all quality gates pass. If the target budget must be enforced, split specification/corpus (651 lines) into a separate doc-only PR. |

Newly discovered work must be added here before implementation. Stop if it
requires another production owner, a path outside the accepted list, Cargo or
lockfile changes, workflow/quality tooling, a new binary, a generic protocol
abstraction, or a budget breach.

## Review counters and finding policy

- Local Open Code Review: 0 of 2 used.
- Pull-request Open Code Review: 0 of 2 used.
- Independent review/remediation cycles: 0 of 2 used.

Each finding is classified as Blocker-Fix, In-scope-Fix, Reject, or Defer. A
reviewer suggestion does not authorize scope expansion.

## Verification evidence

J1 implementation is complete on branch `issue476` (from `4ed77d5`). RED was
captured first: the integration compliance test failed to compile with E0433
(`jefe::jsp` module missing) before any production code existed.

GREEN on the candidate head:

- `cargo test --test jsp_v1_snapshot_compliance` — 18 passed, 0 failed.
- `cargo test --lib jsp` — 13 passed, 0 failed.
- `cargo test --lib observation` — 8 passed, 0 failed.
- `cargo test --workspace --all-features --locked` — 0 failures across all
  targets.
- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — no
  warnings.
- `cargo xtask check source-size` — no new warning or failure; the largest new
  file is 612 lines.
- `cargo xtask check architecture` — pass.
- `cargo xtask check clippy-allows` — pass.

One integration TUI scenario (`ui::dashboard_reorder_tui`) failed once under
full-workspace parallel load and passed on isolated and repeated full-suite
runs. It exercises tmux timing and does not touch any JSP path.

Coordinator review corrections applied after the implementation pass:

- `process_binding` now uses the documented field-state algebra instead of
  silently collapsing an unsupported producer field into `None` values.
- `known` availability now rejects degraded-only members, closing a hole in the
  field-state algebra.
- The envelope schema/kind probe no longer parses the whole document into an
  untyped JSON tree.
- The oversized-document test now proves the byte bound fires, and an at-limit
  case proves the bound is inclusive.
- The specification's native-activity inventory, agent-kind type, process
  binding shape, todo revision rule, identifier grammar, and bounds table now
  match the enforced contract.

## Approval and stop conditions

Implementation begins only after approval of the fourteen J1 contract decisions,
the `jsp -> domain` public architecture boundary, the acceptance matrix, and the
expected paths.

Stop and request another decision if implementation needs:

- Cargo.toml/Cargo.lock, `.llxprt`, `.code_puppy`, `.github`, xtask, workflow,
  quality configuration, a new binary, or an unlisted production owner/path;
- a generic protocol framework, shared harness extraction, new dependency,
  weak public JSON value/map, unsafe code, unwrap/expect, lint suppression,
  threshold increase, placeholder, or compatibility fallback;
- process management, registration storage, timeout/cancellation/cleanup,
  credential vault, network server, or remote transport in J1;
- behavior outside S1-S6;
- more than 25 files or 1,500 net lines, or any approach toward the hard limit
  of 40 files or 2,500 net lines;
- integration with wrong ancestry, contract-set mainline drift, or required
  verification that cannot complete on the candidate head.

## Deferred findings and follow-ups

No review findings or follow-up issues have been recorded yet.
