# Issue 613: Conformance teardown is not crash-safe and leaks psmux servers

Issue: https://github.com/vybestack/llxprt-jefe/issues/613

## Summary

Windows startup probes multiplexer contract conformance in a throwaway namespace named `jefe-conformance-<jefe pid>-<invocation>`. `qualify_multiplexer` creates that namespace, runs the probes, and then issues `kill-server` as straight-line code. Any unwind, early return, or process death between session creation and that final call permanently strands the psmux server pair for the namespace: nothing else in Jefe ever revisits a conformance namespace, and psmux servers outlive their launching parent.

Field evidence in the issue: 36 live `psmux.exe` processes where only 7 were legitimate, 23 orphans spread across 9 conformance namespaces, every owning Jefe pid already dead, and each namespace still holding a complete registry (`.key`, `.pid`, `.port`, `.sid`). The orphans were healthy servers, not wedged ones, so they never expire on their own.

The fix is the pair the issue asks for: make teardown run on unwind, and reclaim leftovers that a previous crash already stranded.

## Acceptance matrix

| ID | Actor / path | Input and target | Observable success | Failure behavior / side effects | Persistence and compatibility | Proof |
|---|---|---|---|---|---|---|
| AC1 | Conformance qualification unwinds after the scratch session exists | Panic raised while probes are in flight | `kill-server` is still issued against the scratch namespace before the unwind escapes `qualify_multiplexer` | The panic still propagates unchanged; teardown failure is swallowed, never converted into a second panic | No new persisted artifact | Unwind test asserting the recorded teardown command (Unix fake binary) and namespace death (Windows real psmux) |
| AC2 | Conformance qualification completes normally | Real multiplexer, successful probes | Scratch namespace is torn down exactly once and the returned report is byte-identical to today's | Probe failures continue to yield the existing report, still followed by teardown | Existing qualification and divergence output unchanged | Existing conformance/qualification suites plus real-binary tests |
| AC3 | Windows startup after an earlier crash | Registry holds `jefe-conformance-<dead pid>-<n>` with a live recorded server | The leftover namespace is killed during startup and its registry entries disappear | Startup continues and still qualifies the multiplexer even when a reclaim fails | Jefe writes no new state; psmux owns and removes its own registry files | Real-binary Windows sweep test using a reaped pid |
| AC4 | Startup with namespaces that must survive | Live Jefe namespace `jefe-<hex>`, conformance namespace owned by a running pid, malformed names, and registry entries whose recorded server is gone or pid-reused | None of these are killed and no `kill-server` is spawned for them | Ambiguous liveness (inaccessible, probe failure, malformed identity) retains the namespace | Untouched registry entries | Pure classifier tests plus a live-owner survival assertion in the Windows sweep test |
| AC5 | Startup on a host without a readable registry | Missing home directory, missing `.psmux` directory, unreadable or truncated entries | Sweep yields no candidates, logs at most a warning, and startup proceeds | No panic, no error surfaced to the user, qualification still runs | No compatibility surface | Discovery unit tests over temporary directories |

## Non-goals

- No Ctrl+C or Windows console-control handler. The issue lists it as optional and redundant once teardown is crash-safe and startup reclaims leftovers.
- No sweep for Unix socket isolation. The reported leak is Windows/psmux and the RAII guard already covers both platforms; the pure classifier stays platform-neutral so a future Unix discovery can reuse it.
- No direct deletion or rewriting of psmux registry files. Jefe only issues `kill-server`; psmux owns its registry lifecycle.
- No change to which namespaces the existing orphan reaper owns, and no reclaim of real (non-conformance) Jefe namespaces.
- No change to probe verbs, conformance reports, divergence rendering, or the startup warning text.
- No new dependency, no workflow/quality-tool change, no persistence schema change.

## Planned vertical slices

### Slice 1: Crash-safe scratch teardown

- Acceptance rows: AC1, AC2.
- Owners: conformance I/O edge only.
- Allowed paths: `src/runtime/multiplexer_conformance_io.rs` and its in-module tests.
- RED: a test that unwinds while the scratch namespace is held shows no teardown command was issued.
- GREEN: a private RAII guard owns the scratch plan and issues `kill-server` from `Drop`; `qualify_multiplexer` keeps its current report semantics.
- Non-goals: no public API addition, no change to probe sequencing.
- Focused verification: runtime conformance tests, `cargo fmt`, `cargo xtask quick`.
- Stop condition: teardown cannot be expressed without a new public abstraction or a probe-sequence change.

### Slice 2: Pure leftover discovery and reclaim decision

- Acceptance rows: AC4, AC5.
- Owners: a new runtime module holding only pure parsing and classification plus directory-scoped discovery.
- Allowed paths: `src/runtime/multiplexer_conformance_sweep.rs`, `src/runtime/multiplexer_conformance_sweep_tests.rs`, `src/runtime/mod.rs` (module and test wiring only).
- RED: no parser exists for `jefe-conformance-<pid>-<n>` and no reclaim decision exists.
- GREEN: namespace parsing, recorded-server identity parsing, directory discovery over an injected registry path, and a total classifier that reclaims only when the owner is dead and the recorded server is alive.
- Non-goals: no process probing inside the pure layer, no filesystem mutation.
- Focused verification: unit tests over temporary directories, `cargo xtask quick`.
- Stop condition: a decision cannot be made without probing processes from the pure layer.

### Slice 3: Startup reclaim wiring and real-binary proof

- Acceptance rows: AC3, AC4, AC5.
- Owners: conformance I/O edge and the existing Windows startup qualification entry point.
- Allowed paths: `src/runtime/multiplexer_conformance_sweep.rs`, `src/runtime/multiplexer_conformance_io.rs`, and the in-module Windows real-binary test.
- RED: a stranded conformance namespace owned by a reaped pid survives `qualify_multiplexer_for_startup`.
- GREEN: startup qualification first reclaims dead-owner leftovers through the existing bounded probe path; failures log and never abort startup; live-owner namespaces survive.
- Non-goals: no retry loop, no background thread, no telemetry surface.
- Verification: Windows real-binary test under `JEFE_REQUIRE_PSMUX=1`, full `cargo xtask ci`, OCR, exact-head checks.
- Stop condition: reclaim requires a scheduler, new persistence, or a change to the startup warning contract.

## Expected paths by ownership layer

- Conformance I/O edge: `src/runtime/multiplexer_conformance_io.rs`.
- New pure/edge sweep module: `src/runtime/multiplexer_conformance_sweep.rs` and `src/runtime/multiplexer_conformance_sweep_tests.rs`.
- Module wiring: `src/runtime/mod.rs` (declaration and `#[cfg(test)]` test module only).
- Plan: `project-plans/issue613-plan.md`.

## Scope ledger

| Discovery | Disposition |
|---|---|
| `kill-server` runs as straight-line code and is skipped on unwind | In-scope: root cause |
| Already-stranded namespaces are never revisited | In-scope: the issue's second requested fix |
| psmux records `<pid>:<creation filetime>` in each `.pid` registry entry, matching `ProcessIdentity` | In-scope: gives a pid-reuse-safe bound on which leftovers are worth killing |
| psmux exposes no registry-path environment override and no server-listing verb | In-scope constraint: discovery must read the user-profile registry directory |
| A `__warm__` session is auto-created alongside the scratch session | No action: `kill-server` already terminates the whole namespace server |
| `src/harness/signal_cleanup.rs` documents the false assumption that psmux dies with its parent | Defer: comment-only correction belongs with a signal-handling issue, not this fix |
| Unix socket-isolation leftovers after a hard kill | Reject expansion: unevidenced and explicitly a non-goal; the guard still covers Unix unwind |

## Review counters

- Local Open Code Review: 1/2
- Post-PR Open Code Review: 0/2

## Local review triage

OCR run 1 (session `de66fce8`, range `b18c6e15..2c1fd0c9`) reported six comments that
deduplicate to three findings, all in `src/runtime/multiplexer_conformance_io_tests.rs`
and none in production code.

| Finding | Validity | Disposition |
|---|---|---|
| The Unix unwind test asserts the recorded `kill-server` invocation instead of observing that the namespace is gone | partial | explain: the Unix test drives a recorder binary precisely because no real server exists to observe, and its Windows sibling already asserts the observable death of the server; `kill-server` is the contract's own teardown verb |
| `observed.lock().ok()` hides a poisoned mutex behind the "never recorded" panic message | valid | fix: consume the mutex with `into_inner` and report poisoning distinctly |
| `is_ok_and(|value| value == "1")` in the `JEFE_REQUIRE_PSMUX` gate is verbose | invalid | dismiss: it is the existing repo convention, character for character, in `tests/runtime_multiplexer_real_binary.rs` |

## Verification evidence

- Behavioral RED proved for both halves: the guard test failed before the RAII guard existed, and the startup test failed with "startup must reclaim jefe-conformance-19584-0, whose jefe is gone" while the sweep call was stubbed out.
- Real-world proof on the development machine: a genuine pre-existing orphan pair, `jefe-conformance-14296-0` (servers 3428 and 19672), was reclaimed by the new sweep; the live workspace namespace and its eight servers were untouched.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `check-clippy-allows`, `check-source-size`, `check-architecture`, `check-multiplexer-surface`, and the complexity gate all pass.
- Coverage passes at 71.19% lines, far above the 30% floor.
- `cargo build --workspace --all-features --locked` and `cargo test --workspace --all-features --locked` pass with `JEFE_REQUIRE_PSMUX=1`, so the Windows real-binary tests run rather than skip.
- A full-suite run caught the reclaim test borrowing a freshly reaped PID: under a busy suite Windows hands that PID out again, the "departed" owner reads as alive, and the sweep correctly retains the namespace. The test now names an owner the kernel cannot allocate and waits a bounded interval for the server to finish dying, so it stays a statement about the sweep rather than about host load.
- `cargo xtask ci` passes end to end on the exact head.

## Deferred findings and follow-ups

- `src/harness/signal_cleanup.rs` still documents the assumption that psmux processes die with their parent, which this issue disproves. Correcting that comment belongs with signal handling, not here.
