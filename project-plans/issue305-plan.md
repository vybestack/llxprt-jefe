# Issue #305: cross-platform process identity and restore coherence

## Root Cause and Decisions

The process lifecycle contract is already typed, but three boundaries bypass it:

1. Unix `kill -0` treats every nonzero status as `Exited`, so an unsignalable
   live process can be reported dead.
2. macOS records no process creation discriminator; using ambient-timezone
   `ps -o lstart=` text directly would also make one process appear to change
   identity when `TZ` changes.
3. startup restore independently falls back PID and identity fields, allowing a
   fresh PID to be paired with a persisted identity from a different process.

This issue will preserve the existing `ProcessObservation` and
`ProcessLiveness` contracts rather than add another process-management system.
Unix will keep the unsafe-free external `kill` probe, force `LC_ALL=C`, capture
stderr, and classify only known permission-denied and missing-process messages;
unknown output fails open as `ProbeFailed`. macOS will invoke `ps` with
`TZ=UTC` and `LC_ALL=C` and parse its fixed `lstart` form into UTC epoch seconds.
Linux `/proc` and Windows `OpenProcess`/creation `FILETIME` identity capture stay
unchanged. All platforms will use one pure final liveness policy: only `Alive`,
`Inaccessible`, and `ProbeFailure` retain liveness; `Dead`, `ReusedPid`, and
`MalformedIdentity` do not establish that the expected process is alive.

Restore will represent PID plus `ProcessIdentity` as one private observation and
choose a fresh observation or persisted observation atomically. A partial fresh
observation never borrows a missing field from persistence. Internally
inconsistent observations are discarded rather than persisted as a mixed pair.

## Acceptance Matrix

| ID | Actor / path | Inputs and boundaries | Target | Success behavior | Failure behavior / diagnostic | Side effects before failure | Persistence / compatibility | Behavioral proof |
|----|--------------|-----------------------|--------|------------------|-------------------------------|-----------------------------|-----------------------------|------------------|
| A1 | Typed process probe | successful `kill -0`; C-locale `EPERM`/permission-denied; C-locale `ESRCH`; unknown stderr; spawn failure | Unix | success becomes `Running`; permission denial becomes `Inaccessible`; missing PID becomes `Exited`; unknown/spawn failure becomes `ProbeFailed` | unknown probe evidence is not promoted to confirmed death; existing typed error carries the classification | one argument-structured local probe only; no shell interpolation | no schema change | pure classifier tests, structural `Command` test, and live/dead process regression |
| A2 | Process identity capture | same live process observed by helpers whose parent `TZ` values differ; valid and malformed `ps` output | macOS | both environments produce the same nonempty `started_at`; fixed UTC text parses to epoch seconds | malformed/failed `ps` returns no token and downstream comparison fails open as `ProbeFailure` when a persisted token exists | one local `ps` query; no mutation of parent environment | existing serialized `Option<u64>` remains compatible; newly captured macOS bindings gain `Some(token)` | parser tests, structural `Command` test, and same-live-PID cross-timezone subprocess test |
| A3 | Persisted process comparison | same PID/same token; same PID/different token; missing/malformed identity | Linux, macOS, Windows | matching instance is `Alive`; a changed token is `ReusedPid` and rejected | malformed evidence remains `MalformedIdentity`/`ProbeFailure`, never `Dead` | probe only | legacy PID-only state remains readable; no migration | pure six-outcome classification tests including reused PID |
| A4 | PID-only/background liveness API | `Running`, `Exited`, `Inaccessible`, `ProbeFailed`; final six-state policy | Unix and Windows, with shared platform-neutral seam | inaccessible and probe failures fail open; only confirmed exit is false for an unbound PID probe | probe failures remain observable as typed policy before conversion to bool | probe only | no persistence | shared policy tests cover all six final states; existing current/nonexistent PID tests exercise native path; Windows CI runs the shared contract and native implementation |
| A5 | Startup runtime restoration | coherent fresh pair; fresh PID only; fresh identity only; no fresh observation; mismatched fresh or persisted pair | local startup | fresh evidence is chosen atomically; identity-only evidence derives its own PID; no fresh evidence falls back to one coherent persisted observation | mismatched pair is discarded instead of mixed or persisted | no process side effect in resolver | persisted PID-only legacy binding remains PID-only; restored binding always has `pid == process_identity.pid` when identity exists | private pure binding-observation tests and startup restore integration path |
| A6 | Startup classification | `Alive`, `Dead`, `ReusedPid`, `Inaccessible`, `MalformedIdentity`, `ProbeFailure` with live/missing session evidence | local startup, including Windows | alive or uncertain process evidence preserves recoverability; a live session remains ground truth for otherwise malformed binding metadata | confirmed dead/reused/malformed evidence with no live session is stopped/stale/inconsistent and binding is cleared | no launch before classification | existing binding schema and remote session-only behavior preserved | existing startup matrix plus focused reused/inaccessible/probe-failure regressions |
| A7 | Operator / maintainer contract | startup and PID-only liveness outcomes | all supported platforms | one documented table states the fail-open policy and identifies outcomes unavailable to an unbound PID probe | no ambiguous inference that access denial means death | none | documents current schema and compatibility semantics | documentation review plus tests mapped to every table row |

## Explicit Non-Goals

- No new dependency, unsafe code, direct libc/syscall use, or process-management
  subsystem.
- No change to Windows `FILETIME` capture, Linux `/proc/<pid>/stat` parsing,
  tmux session ownership, launch/relaunch behavior, or remote SSH probing.
- No remote escaping test: these probes are local-only, pass PID as one
  structured argument, and never construct a remote command or shell string.
- No persistence schema/version migration; legacy missing `started_at` remains
  readable and is handled conservatively.
- No UI behavior or TUI scenario. The issue changes a non-visual runtime
  boundary and is proven below the UI layer.
- No active orphan adoption/reclaim behavior or changes to batch tmux polling.
- No quality-gate, workflow, dependency-manifest, `.llxprt/`, or `.code_puppy/`
  changes.

## Planned Vertical Slices

### Slice 1: Typed cross-platform probe policy (A1, A3, A4)

- **Owner / boundary:** `src/runtime/process.rs` owns process observation and
  classification; `src/runtime/liveness.rs` consumes that typed service rather
  than implementing a second platform probe.
- **Allowed files:** `src/runtime/process.rs`, `src/runtime/process_tests.rs`,
  `src/runtime/liveness.rs`.
- **RED:** pure Unix classifier and six-state fail-open policy tests do not
  compile before their seams exist; native liveness regression demonstrates the
  public API contract.
- **GREEN:** controlled-locale Unix results preserve `Inaccessible` vs `Exited`,
  unknown failures fail open, PID-only liveness delegates to the typed service,
  and Windows retains native probe behavior through the shared policy.
- **Structural/platform evidence:** assert executable, separate PID arguments,
  and `LC_ALL=C`; native Windows CI compiles and tests the shared contract.
- **Non-goals:** no macOS token or startup restore changes in this slice.
- **Verify:** focused runtime tests, `cargo fmt --all --check`, then
  `make quick-check`.
- **Stop:** adding a dependency, changing public enums, or requiring a shell or
  remote command needs approval.

### Slice 2: Stable macOS process token (A2, A3)

- **Owner / boundary:** `src/runtime/process.rs` owns local process I/O and a
  pure fixed-format UTC parser.
- **Allowed files:** `src/runtime/process.rs`, `src/runtime/process_tests.rs`,
  `src/domain/mod.rs` only for correcting the identity contract documentation.
- **RED:** parser, command-structure, malformed-input, and same-live-PID
  cross-timezone tests fail before capture exists.
- **GREEN:** macOS stores UTC epoch seconds from controlled `ps`; Linux and
  Windows implementations are unchanged.
- **Non-goals:** no portable BSD promise beyond macOS; other non-Linux Unix
  targets retain an absent start token.
- **Verify:** focused runtime tests on macOS and `make quick-check`.
- **Stop:** a new time/date dependency or platform API requires approval.

### Slice 3: Atomic restore observation (A5, A6)

- **Owner / boundary:** startup orchestration selects one process observation;
  the runtime remains the source of fresh PID/identity evidence.
- **Allowed files:** `src/app_init.rs` and planned private module
  `src/app_init_process_binding.rs`. The private module is required because
  `app_init.rs` is already at the source-file hard limit boundary and owns the
  pure atomic resolver without creating a public abstraction.
- **RED:** resolver tests fail before the private observation type and atomic
  selection exist.
- **GREEN:** restore cannot combine fresh and persisted fields, legacy PID-only
  evidence remains supported, and every produced identity agrees with its PID.
- **Non-goals:** no runtime-manager redesign or unrelated startup test movement.
- **Verify:** focused binary tests and `make quick-check`.
- **Stop:** changing runtime traits/public APIs or requiring another production
  module needs approval.

### Slice 4: Contract documentation and exact-head delivery (A7)

- **Owner / boundary:** runtime standards document the implemented behavior.
- **Allowed files:** `dev-docs/standards/persistence-and-runtime.md` and this
  plan's evidence/ledger sections.
- **GREEN:** table covers all six outcomes for startup and PID-only polling and
  explains that unbound polling cannot classify identity reuse/malformation.
- **Verify:** `make ci-check`, scope/ancestry review, OCR, then exact-head CI.

## Expected Files by Architectural Layer

| Layer | Expected paths | Acceptance rows |
|-------|----------------|-----------------|
| Domain contract docs | `src/domain/mod.rs` | A2, A3 |
| Runtime boundary and tests | `src/runtime/process.rs`, `src/runtime/process_tests.rs`, `src/runtime/liveness.rs` | A1-A4 |
| Startup orchestration | `src/app_init.rs`, `src/app_init_process_binding.rs` | A5, A6 |
| Runtime standards | `dev-docs/standards/persistence-and-runtime.md` | A7 |
| Delivery record | `project-plans/issue305-plan.md` | A1-A7 |

Planned maximum: 8 changed files and substantially below 1,500 net lines.

## Scope Ledger

| Date | Item | Disposition | Approval / follow-up |
|------|------|-------------|----------------------|
| 2026-07-22 | Initial bounded plan from issue and comments | In scope | Issue #305 |
| 2026-07-22 | Private startup binding resolver module needed to keep `app_init.rs` under 1,000 lines | In scope planned module | No public API or subsystem |
| 2026-07-22 | Remote escaping coverage | Not applicable | Local structured arguments only; explicit non-goal |

## Review Counters

- Local Open Code Review: 2/2 attempted; both tool runs were terminated before
  producing output
- Independent Rust review: 1 complete
- Post-PR Open Code Review: 0/2
- CodeRabbit: not requested until verified PR head is ready

## Verification Evidence

- Baseline: process, native PID, and 19 startup tests passed on 2026-07-22.
- Slice 1 RED: `cargo test runtime::process_tests` failed on missing Unix
  classifier/command and shared policy symbols.
- Slice 1 GREEN: 9 process tests and both native PID tests passed; full
  `make quick-check` passed (2,280 library tests plus all binary/integration and
  doc-test targets).
- Slice 2 RED: `cargo test runtime::process_tests` failed on missing macOS
  parser and command-construction seams.
- Slice 2 GREEN: 13 process tests passed on macOS, including a same-live-PID
  fixture queried from UTC, Pacific, and Tokyo parent environments; full
  `make quick-check` passed with 2,284 library tests.
- Slice 3 RED: the focused binding test failed to compile when the private
  observation/resolver seam was removed (`E0432`/`E0433`/`E0425`).
- Slice 3 GREEN: 5 atomic observation tests, all 19 startup tests, and full
  `make quick-check` passed; `app_init.rs` remained under the 1,000-line hard
  limit.
- Pre-review `make ci-check` passed on `0e523674da8f6f21ea4fca171cc07ac374620cd0`.
- Review remediation focused tests passed: 14 process tests, 7 atomic
  binding/application tests, and 10 startup tests.
- Post-remediation `make quick-check` passed with 2,285 library tests and 727
  binary tests plus all integration and doc-test targets.
- Final `make ci-check` exact head: pending
- Native Windows CI exact head: pending
- PR conflict and ancestry check: pending

## Review Triage

| Finding | Disposition | Action |
|---------|-------------|--------|
| Legacy macOS tokenless identities became malformed | Blocker-Fix | Tokenless expected identity now accepts the same live PID; fully tokenized mismatches still reject reuse. |
| Windows test bypassed Win32 error mapping | In-scope-Fix | Extracted a pure stage/error classifier, routed production errors through it, and added native-Windows API-error tests. |
| Atomic restore lacked application-boundary proof | In-scope-Fix | Added tests that resolve partial fresh observations and verify the resulting `RuntimeBinding`. |
| Test-only dead-decision model contradicted production | In-scope-Fix | Removed the duplicate helpers/tests and covered the real startup classifier/PID binding path. |
| Runtime policy documentation did not match all binding checks | In-scope-Fix | Documented binding validation separately and routed startup through the shared recoverability predicate. |
| PR/Windows readiness remained pending | Reject as code finding | Correct delivery-state observation; retained as a required pending delivery gate. |
| UTC calendar conversion lacked boundary tests | In-scope-Fix | Added epoch, rollover, leap-century, invalid-date, and pre-epoch fixtures. |

## Deferred Findings / Follow-ups

None.
