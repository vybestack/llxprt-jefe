# Issue 522 delivery plan

## Scope decision

Deliver the accepted Jefe half and final cross-repository proof in one issue-linked pull request with bounded internal commits. The accepted scope includes the embedded JSP host, secure local LLxprt bootstrap, production-owned reducer/state, existing Preview projection, protocol/schema clarifications, and tmux proof. The Split status workbench remains excluded.

The plan crosses more than three ownership layers because the user's accepted behavior is itself an end-to-end vertical slice. No stacked PRs will be created. Scope is governed by the accepted behavior and explicit non-goals below.

## Resolved decisions

- Jefe is the proof-of-concept embedded broker and observer; LLxprt Luther is not required.
- The embedded profile uses authenticated loopback registration, publication, and heartbeat. Jefe reduces accepted documents directly; no self-SSE route is required.
- JSP/1 remains current-state and snapshot-first with no replay, history, resume, cursor negotiation, or resync-required behavior.
- A gap preserves native data, marks observer health stale, and requires a fresh snapshot.
- The pure JSP stream reducer is production-owned and reused by compliance.
- AgentStatus remains process-only. Observation health and native activity are separate runtime-only state.
- Local fresh LLxprt launch is supported. Remote and already-running reattach render telemetry unsupported.
- The JSON Schema dialect remains the standards-defined Draft 2020-12. JSP's custom UTF-8-byte annotation is mandatory to enforce.
- No new third-party dependency is planned: the existing smol dependency supplies loopback networking. If a safe focused host cannot be built without a new HTTP dependency, stop for approval rather than hand-roll a general parser.

## Acceptance matrix

| ID | Actor / target | Inputs / boundaries | Success | Failure / side effects | Evidence |
| --- | --- | --- | --- | --- | --- |
| J1 | Jefe startup | Local runtime; bind availability | Authenticated 127.0.0.1 ephemeral endpoint is ready before instrumented spawn | Typed failure; zero instrumented child side effects | Real-socket tests |
| J2 | Local LLxprt launch | Agent ID, positive generation, owner-only runtime dir/file | Bootstrap path enters authorized environment; credential absent from argv/logs | Atomic cleanup and revocation | Launch-plan, permission, canary tests |
| J3 | Registration/auth | Valid, unknown, wrong-role, wrong-binding, conflicting credentials | Token binds one agent/generation/epoch | 401/403/409 and no canonical mutation | Raw request matrix |
| J4 | Publication | Snapshot, +1, duplicate, lower, gap, stale identity, heartbeat | Atomic snapshot; +1 applies; duplicate/lower no-op; heartbeat changes health only | Gap preserves native state and requires fresh snapshot; stale identity no-op | Reducer and socket tests |
| J5 | Runtime state | Parsed payloads and typed transport state | Payload-preserving observation keyed by agent/generation/epoch | No I/O and no persistence | State/persistence tests |
| J6 | Preview | Known/empty/unsupported/unknown/degraded/stale todos and message | Real todos and last committed assistant reply render at finite width | Drafts excluded; deterministic clipping | Pure view and component tests |
| J7 | Status | Process, health, activity, wait, turn, tool, terminal facts | Truthful Starting/Unsupported/Connecting/Stale/Disconnected/Protocol error/Waiting/Working/Ready/Failed/Ended/Unknown | No fabricated Ready or Dead | Exhaustive precedence tests |
| J8 | Turn elapsed | Producer elapsed anchor plus local ticks | Local monotonic advancement | No cross-process clock subtraction | Fake-clock tests |
| J9 | Lifecycle | Failed launch, exit, kill, relaunch | Credentials/file revoked; old generation/epoch rejected | Agent-scoped bounded cleanup | Lifecycle tests |
| J10 | Unsupported paths | Remote or attached existing process | Explicit telemetry unsupported | No local path/token injection | Remote/restore tests |
| J11 | Protocol/schema | Embedded profile, gap health, known-null turn, agent binding, multibyte bounds | Rust and TypeScript agree while existing documents/scenarios remain compatible | Parser/schema mismatch fails the oracle | Schema/compliance tests |
| X1 | TUI-first scenario | Current Preview before production wiring | Scenario exists first and fails on missing real JSP data | Failure is the intended missing behavior | Preserved RED output |
| X2 | Real process proof | Real Jefe + real native LLxprt + deterministic fake responses | Preview shows actual todo, committed reply, and truthful live status | No model/network credential; no semantic terminal scraping | Green tmux scenario/report |
| X3 | Identity proof | Same-directory instances and relaunch | No collision; delayed old identity cannot update replacement | Cross-contamination fails | Unit/integration/harness evidence |

## Preview status precedence

1. Confirmed exit: Dead.
2. Queued/spawning: Starting.
3. Alive + unsupported observation: Running — telemetry unsupported.
4. Alive + connecting/stale/disconnected/protocol-error: render health explicitly; historical native values remain visibly stale.
5. Live + explicit unresolved wait: Waiting with reason.
6. Live + terminal source state/session end: Failed or Ended.
7. Live + active turn/thinking/acting/nonterminal headline tool: Working.
8. Live + known idle/no wait/no turn/no terminal state: Ready.
9. Otherwise: Unknown.

source.error remains diagnostic because JSP/1 has no event that clears it.

## Vertical slices

### S1 — TUI scenario and protocol profile

- Acceptance: X1, J11.
- Paths: `dev-docs/tmux-scenarios/v1/`, `dev-docs/jsp/v1/`, schema manifest/cases and focused tests.
- RED: scenario cannot find real todo/reply/status in current Preview.
- GREEN: specification and executable fixtures express the accepted embedded/current-state semantics.
- Stop: wire-shape change beyond known-null current turn or a new protocol version.

### S2 — Production-owned reducer and runtime state

- Acceptance: J4, J5, J8.
- Paths: `src/jsp/v1/`, `src/domain/observation.rs`, `src/messages.rs`, `src/state/`, focused tests.
- RED: typed message cannot populate state; identity/gap cases fail.
- GREEN: one pure reducer supplies compliance and product state.
- Stop: second state-machine implementation or persisted observation.

### S3 — Pure Preview status projection

- Acceptance: J6, J7.
- Paths: iocraft-free view module, `src/ui/components/preview.rs`, dashboard selection plumbing, tests.
- RED: hard-coded no-tasks Preview omits accepted fields.
- GREEN: finite-width truthful projection.
- Stop: Split/new-screen/layout redesign.

### S4 — Loopback host and launch lifecycle

- Acceptance: J1-J3, J9, J10.
- Paths: focused service/runtime boundary, launch authorization/environment, messages, real-socket tests.
- RED: authentication/bootstrap/lifecycle matrices fail.
- GREEN: safe local embedded host with typed state delivery and cleanup.
- Stop: dependency change, remote transport, general HTTP abstraction, or unplanned process subsystem.

### S5 — Cross-repository proof

- Acceptance: X2, X3.
- Paths: scenario, focused wrapper/script if required to invoke the real package-tree LLxprt launcher, tests/docs.
- RED: fixture/real producer data absent.
- GREEN: real Jefe + real LLxprt proof and redacted report.
- Stop: standalone LLxprt packaging subsystem or semantic terminal scraping.

## Expected paths

- `dev-docs/jsp/v1/specification.md`
- `dev-docs/jsp/v1/compliance/{server-contract.md,schemas/}`
- `dev-docs/tmux-scenarios/v1/jsp-llxprt-preview.json`
- `dev-docs/tmux-scenarios/v1/jsp-llxprt-preview-native.json`
- `src/jsp/v1/`
- `src/domain/observation.rs`
- `src/messages.rs`
- `src/state/`
- focused service/runtime host/bootstrap modules
- `src/domain/agent_definition/shipped/llxprt.rs`
- `src/ui/components/preview.rs` and a pure projection module
- focused unit/integration/TUI tests

## Explicit non-goals

- Split status workbench, all-agent rows, Split retirement, new screen, or second terminal viewer.
- LLxprt producer implementation.
- Luther, remote/TLS/Unix-socket/named-pipe transport, or late attachment.
- Replay/history/resume/cursor negotiation/resync-required.
- Control, prompts, approvals, steering, terminal scraping, or key injection.
- Other agents, subagent rows, nested tasks, raw transcripts/thinking/tool payloads.
- Persistence of observation or credentials.
- Dependency/workflow/quality-gate changes, suppressions, unsafe, or production unwrap/expect.

## Scope ledger

| Item | Disposition | Notes |
| --- | --- | --- |
| Embedded Jefe host | Accepted | Required to make the two-repository proof complete without Luther |
| Existing Preview rendering | Accepted | Explicit user-visible proof |
| Protocol/schema clarifications | Accepted | Required interoperability/root-cause fixes |
| Split workbench | Rejected | Explicitly deferred by user |
| New dependency | Not approved | Stop if existing stack cannot safely deliver the focused host |
| `.llxprt` changes | Rejected | Protected and untouched |

## Review counters

- Independent review/remediation cycles: 0 of 2. Subagent reviewers were
  unavailable for this run: every configured profile returned a provider error
  (usage limit reached, missing API key, or load-balancer exhaustion). This work
  has had no independent Rust reviewer.
- Local OCR: 1 of 2.
- PR OCR: 0 of 2.

## Review dispositions

Addressed:

- Scenario socket collision. Each scenario now uses its own socket path. The
  path stays short deliberately: it backs the tmux server socket and the
  platform `sun_path` limit is 104 bytes, so a workspace-relative path exceeds
  it and the agent dies before observation starts. This was verified by
  observing `Status: Dead` with the longer path.
- Racy `drain_messages` assertions replaced with bounded polling, because the
  worker publishes after writing the HTTP response.
- Weak `assert_ne!` turn-elapsed assertion replaced with the exact rendered
  value.
- `TempDir` guard retained so the JSP runtime directory is reclaimed.

- Reducer activity synthesis. `apply_turn_ended` inferred an idle activity when
  a turn ended. That contradicts current-state semantics, where activity is
  authoritative from the producer, and compliance scenario S6 fails against it.
  The synthesis was also unnecessary: the native producer emits its own
  `activity.changed`, and the native proof still renders `Status: Ready` with it
  removed. The unit test that encoded the wrong behavior was rewritten.

- Startup fail-closed on the JSP host. Jefe exited when the local host could
  not bind. Observation is optional telemetry, so a host that cannot start must
  degrade to unsupported telemetry rather than prevent Jefe from running. The
  host is now optional; agents launch uninstrumented when it is absent.

- Preview status labels. Completed and Errored were both rendered as "Dead",
  conflating a successful exit with a failure, and Waiting/Paused collapsed
  into "Running" when no observation existed. The labels now match the ones the
  rest of the application already renders.
- Teardown revoked launches with `?`, so one failure stranded credentials for
  every remaining agent. Revocation now attempts all launches and reports the
  first error.
- The bootstrap environment variable was appended unconditionally, so a plan
  reused across a relaunch could carry two entries. It is now replaced in
  place.

Dismissed with reason:

- "`turn_observed_at` is never populated." It is: `src/jsp_host/mod.rs` sets it
  from the publisher's turn anchor when an observation is delivered. The
  reducer is not the only writer.
- "Remove the duplicated `prepare_current` call." The first preflight runs
  against the unmodified plan so an unspawnable agent fails before any
  credential material is written, which is acceptance J1. The second preflight
  is required because JSP instrumentation changes the plan. Documented instead.
- "Fold the token validation into the route handler's own `mutate` call." The
  registry is only touched by the single worker thread, so the described TOCTOU
  window does not exist, and the split keeps authorization ahead of routing.

## Verification evidence

### RED

- Command: `target/debug/tmux_scenario --scenario dev-docs/tmux-scenarios/v1/jsp-llxprt-preview.json --install jefe=target/debug/jefe`.
- Exit: 4 (`HAR-E006`).
- Intended failure: `frame does not contain 'Status: Working'`.
- Captured frame showed the existing Preview with `Status: Queued`, `Todo:`, and `(no tasks)`, proving that real JSP status/todos/reply were absent before implementation.
- Report retained locally at `target/issue522-red.json`.

### Focused GREEN

- `cargo test --locked production_reducer_preserves_preview_payloads --lib`: passed.
- `cargo test --locked jsp::v1 --lib`: passed.
- `cargo test --locked --test jsp_preview_projection`: passed.
- `cargo test --locked --test jsp_host_socket`: passed (10 tests, including route phase, lease expiry, protocol mutation forwarding, cleanup safety, and coalescing).
- `cargo test --locked --test harness_v1_fixtures -q -- --test-threads=1`: passed (21 tests).
- The fixture-backed tmux scenario passed (`status: passed`, exit 0) with `Status: Working`, monotonic `Turn elapsed`, the expected todo `[ ] Implement issue 522`, and `Last reply: JSP preview is wired`, with `(no tasks)` asserted absent.
- The real native LLxprt tmux proof passed (`status: passed`, exit 0) with `Status: Ready`, the completed native todo `[x] Native LLxprt todo`, and `Last reply: Native LLxprt JSP r…`, with `(no tasks)` asserted absent.
- Both proofs render the Preview from reduced JSP documents only; terminal text is never used as observation input.
- Wire evidence: the real producer's first request is `POST /jsp/1/register` carrying `Authorization: Bearer pub-…`, `jsp-registration-id: reg-…`, and a complete snapshot-first document (`kind: snapshot`, `source_sequence: 0`, fresh `source_epoch`, full provenance/availability envelopes), confirming the frozen current-state contract on the wire rather than by inference.

### Reproducible cross-process proof

Build the exact binaries used by the deterministic fixture scenario:

```sh
cargo build --locked --features psmux-smoke \
  --bin jefe --bin tmux_scenario --bin jefe-jsp-llxprt-fixture
```

Run the fixture producer through the real Jefe process and real PTY harness:

```sh
target/debug/tmux_scenario \
  --scenario dev-docs/tmux-scenarios/v1/jsp-llxprt-preview.json \
  --install jefe=target/debug/jefe \
  --install llxprt=target/debug/jefe-jsp-llxprt-fixture \
  --install tmux="$(command -v tmux)"
```

The native LLxprt proof uses a dedicated scenario because it asserts the
*terminal* state of a completed turn (`Status: Ready`, completed todo) rather
than the fixture's mid-turn state. Substitute only the installed producer path,
retaining the same Jefe binary, isolated scenario workspace, and deterministic
fake-response profile:

```sh
target/debug/tmux_scenario \
  --scenario dev-docs/tmux-scenarios/v1/jsp-llxprt-preview-native.json \
  --install jefe=target/debug/jefe \
  --install llxprt=<jsp-capable llxprt launcher> \
  --install tmux="$(command -v tmux)"
```

The installed producer must be a JSP-capable LLxprt launcher configured with the
deterministic fake-response profile (`LLXPRT_FAKE_RESPONSES`) so the run consumes
the fixed three-step sequence: todo `in_progress`, todo `completed`, then the
committed text reply. No model or network credential is used.

The native scenario's waits are set to the harness maximum of 30000 ms. The
15000 ms waits used by the fixture scenario are sufficient for the in-process
fixture producer but not for real LLxprt, which must boot its TUI, execute two
tool turns, and commit a reply before the assertion can hold. An under-sized
wait surfaces as `Status: Stale` with `(no tasks)` — an observation-health
timeout, not a protocol failure.

Two operational notes for anyone reproducing this:

- The scenarios drive the New Agent form by a fixed number of `tab` steps, so
  adding or removing a form field invalidates them. The symptom is a timeout
  waiting for `> [Create enabled]` while the cursor rests on the last field.
- Immediately after a dependency install or a full TypeScript rebuild, the
  first native run can exceed the 30000 ms ceiling because the runtime
  re-transpiles the whole workspace on that first start. This presents as
  `Status: Stale` and clears once the cache is warm. It is a harness ceiling,
  not a producer defect: the producer's registration was confirmed
  independently against a raw listener on both the interactive and
  non-interactive paths.


### Scope review
- Every changed path maps to the accepted Jefe host, lifecycle, reducer/state, Preview, protocol, test, plan, or harness behavior.
- The reducer and projection moved from compliance-only ownership to production ownership so compliance and the runtime share one state machine.
- No quality configuration, workflow, Split workbench, remote transport, persistence, or `.llxprt` change was made.

### Verification status

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo build --workspace --all-features --locked`: passed.
- Focused JSP host, reducer, state ingress, Preview, and serial harness targets: passed.
- The ordinary parallel full workspace test run reached the pre-existing real-PTY harness target and reported six timing/contention failures; rerunning that target serially passed all 21 tests. Two attempts to run the entire workspace serially were terminated externally while that same target was in progress after every preceding target had passed, so no false full-suite pass is recorded.

## Deferred findings

- Windows ACL behavior is implemented fail-closed using the operating system's `icacls.exe` under the existing Windows dependency set, but this macOS run cannot execute the Windows-native ACL test path. Cross-target compilation is covered by normal Windows CI.
