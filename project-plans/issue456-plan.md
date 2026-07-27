# Issue #456: Stabilize the Windows psmux mouse smoke test

## Problem and investigation result

The Windows-only integration test
`psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys` remains
non-deterministic on current `main` (`a38f763`) after issue #465 removed
psmux 3.3.7's `PageUp -> copy-mode -u` root binding.

A required local psmux 3.3.7 run on current main failed before Page-key input:
`AttachedViewer never observed mouse reporting after fixture advertised
1000/1002/1006`. The fixture emits those DEC private modes once, before the
detached session's ready marker; the test waits for that marker before creating
`AttachedViewer`. A fresh viewer starts with a blank terminal model, so whether
it observes the earlier one-shot mode advertisement depends on psmux/ConPTY
attach timing. The pane capture was healthy and contained
`PSMUX_SMOKE_READY`, and the failed run left no
`jefe-psmux-smoke-fixture` process.

Issue #465/#468 correctly fixed the separate PageUp interception failure. Its
single-write Page-key behavior remains part of this plan. Increasing the timeout
or re-injecting semantic Page keys cannot fix either a missed one-shot mode
advertisement or a key consumed by a root binding.

### Discovered root cause (post-DeepThinker)

The three-sequential-candidate architecture still failed to forward any probe
input on the original machine because Jefe itself was running **inside** an
existing psmux session. The inherited `PSMUX_SESSION`/`PSMUX_TARGET_SESSION`
environment variables made every psmux command appear nested, so psmux refused
setup with `psmux: sessions should be nested with care, unset PSMUX_SESSION to
force`, and the attaching client never established an input relay. The
one-shot mouse-mode timing was a symptom, not the cause.

The fix is a related production isolation change (authorized by the user's
demand to continue):

- `MultiplexerPlan::command()` scrubs `PSMUX_SESSION`/`PSMUX_TARGET_SESSION`
  on native Windows (Unix is unaffected), via a shared `pub(super)` constant
  `PSMUX_INHERITED_SESSION_VARS`.
- `attach_command` scrubs `TMUX`/`TMUX_PANE`/`TMUX_TMPDIR` plus the same two
  psmux session variables on every platform, and always emits the explicit
  `attach-session -t <session>` form (psmux 3.3.7 accepts it; the stale 3.3.6
  compatibility comment is removed).
- `PSMUX_CLAUDE_TEAMMATE_MODE` and `PSMUX_CONFIG_FILE` are intentionally
  retained (team mode is not session routing; the plan already carries `-f NUL`).

With both isolation changes, the real psmux smoke passes on the **first**
candidate in ~2.7s. The bounded three-candidate retry is retained as defense
against genuine attach-timing variance on loaded CI runners, but the candidate
deadline accounting is fixed so marker + mouse-mode are polled conjunctively
under one shared per-candidate ceiling (see AC1).

## Acceptance matrix

| ID | Actor / launch path | Input and boundary cases | Target | Observable success | Observable failure / diagnostic | Side effects permitted before failure | Persistence / compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| AC1 | `psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys` creates a detached fixture session, then attaches up to three sequential `AttachedViewer` candidates through `attach_viewer_until_input_and_mouse_ready` against the same established session | The fixture's startup mode advertisement may occur before any single viewer exists; a unique readiness probe (`j`/`k`/`l`) is the first confirmed post-attach child input for each candidate; the fixture emits the DEC private mouse modes *before* the probe's byte marker | Native Windows, psmux >= 3.3.7, `psmux-smoke` feature | A candidate's own unique marker (`PSMUX_BYTE_6A`/`6B`/`6C`) appears in capture **and** that same viewer reports `mouse_reporting_active`, polled conjunctively under one shared per-candidate ceiling (10s), within one shared 30-second overall deadline; the qualifier is kept for AC2 | The assertion lists per-candidate diagnostics distinguishing marker/mode/liveness; the namespace transcript remains under `target/psmux-smoke/<namespace>` | One unique psmux namespace, one fixture process, one temporary working directory, and test artifacts only | Related production isolation: `MultiplexerPlan::command` and `attach_command` scrub inherited `PSMUX_SESSION`/`PSMUX_TARGET_SESSION` (Windows/all-platform attach respectively) and always emit explicit `attach-session -t`; startup advertisement remains compatible with other fixture users | One-client RED recorded; DeepThinker-amended three-sequential-candidate architecture implemented; discovered inherited-PSMUX root cause fixed in production isolation; real psmux smoke GREEN on first candidate ~2.7s (see evidence) |
| AC2 | The same attached viewer writes PageUp/PageDown and SGR mouse bytes | Input readiness and mouse-mode synchronization have completed; Page keys are written once without retry re-injection | Native Windows, psmux >= 3.3.7 | Exact `CSI 5~`, `CSI 6~`, and `CSI <0;1;1M` constituent byte markers reach the child; `pane_in_mode` stays `0` | Existing bounded capture diagnostics identify the missing exact marker or unexpected pane mode | Only the accepted input bytes are sent to the isolated fixture | The issue #296 byte/mouse contract and issue #465 root-table workaround remain unchanged | Existing exact-byte assertions in `tests/psmux_smoke_mouse.rs` pass in each repeated run |
| AC3 | `PsmuxNamespace` teardown during normal return or panic unwinding | Success, assertion timeout, or command failure after namespace creation | Native Windows test harness only | Cleanup invokes namespace-scoped `kill-server` through the existing five-second bounded command collector, which waits for psmux 3.3.7 to terminate its owned fixture descendants; transcript collection completes | Cleanup command failure/timeout is recorded in the namespace transcript; teardown never contacts the default server | Only the unique `-L <namespace>` server and its descendants may be terminated | Relies on the already-qualified psmux 3.3.7 namespace-cleanup contract; no process-management subsystem or public API | RED run external process check shows no leaked fixture; focused repetitions complete with bounded teardown transcripts and no fixture survivors |

## Timing, retry, failure, and ownership decisions

- Keep the existing 30-second Windows poll ceiling as one shared overall
  deadline across all attach attempts. All waits return as soon as their
  condition holds. This test is Windows-only, so there is no Linux timeout
  branch to change. The 30-second ceiling is never restarted or made per-attempt.
- Synchronize on observable post-attach input delivery rather than sleeping or
  increasing a timeout.
- A candidate's unique readiness probe (`j`/`k`/`l`) may be sent through that
  candidate because it is an idempotent ASCII probe. At most three sequential
  candidates are attempted against the same established session, each capped at
  ten seconds; failed viewers are dropped before the next spawn. **Per-candidate
  deadline accounting:** marker delivery and mouse-mode observation are polled
  **conjunctively** under one shared per-candidate ceiling (`PER_CANDIDATE_TIMEOUT`)
  via `poll_marker_and_mouse_reporting` — the fixture emits the DEC private mouse
  modes before the probe's byte marker, so once the marker lands the modes have
  already traversed the PTY and the loop keeps polling mouse mode without
  restarting the deadline. The outer 30-second overall deadline is never
  restarted. PageUp/PageDown remain a single write with capture-only polling;
  semantic keys are never re-injected, and no retry or reattach occurs after
  semantic input starts.
- The fixture owns re-advertisement of the exact `MOUSE_MODE_BYTES` in response
  to any readiness probe (`READINESS_PROBES b"jkl"`), emitted before that
  probe's normal byte marker. Production `AttachedViewer` and runtime
  orchestration remain unchanged; `nudge_for_mode_recovery` is called once per
  candidate to mirror production but is never used as readiness.
- Namespace cleanup stays in the private test harness and reuses its existing
  bounded command collector. If full reaping requires a new process-management
  or cancellation subsystem, stop for approval instead of adding one.

## DeepThinker amendment record

- **DeepThinker count:** 1.
- **Trigger:** The one-viewer readiness-probe re-advertisement attempt
  (single `j` probe against a single attached viewer) remained RED: the pane
  forwarded fixture output but the probe input never reached the child within
  the full 30 seconds, even after `nudge_for_mode_recovery`.
- **User authorization:** The user explicitly authorized continuing issue #456
  on branch `issue456` and provided the amended bounded architecture below.
- **Selected architecture:** Replace the single readiness probe with
  `READINESS_PROBES b"jkl"`; the fixture re-advertises exact `MOUSE_MODE_BYTES`
  before its normal byte marker for any `j`/`k`/`l`. In the integration test,
  replace the one-shot viewer spawn plus separate readiness/mouse assertions
  with a private `attach_viewer_until_input_and_mouse_ready` helper that creates
  at most three sequential `AttachedViewer` candidates against the same
  established session, using unique probes `(j, PSMUX_BYTE_6A)`,
  `(k, PSMUX_BYTE_6B)`, `(l, PSMUX_BYTE_6C)`, one shared 30-second overall
  deadline and a maximum of ten seconds per candidate. A candidate qualifies
  only when its own unique marker appears in capture and that same viewer
  reports `mouse_reporting_active`. `nudge_for_mode_recovery` is called once
  per candidate to mirror production but is not used as readiness. Each failed
  viewer is dropped before the next spawn; the qualifier is kept for the
  existing PageUp/PageDown one-write/no-reinjection assertion and the exact SGR
  assertion. No retry or reattach after semantic input starts.
- **Scope impact:** Originally test-only; amended after the inherited-PSMUX
  root cause was discovered to include the related production isolation changes
  described above (`MultiplexerPlan::command`, `attach_command`). Allowed files
  now: `project-plans/issue456-plan.md`,
  `tests/fixtures/psmux_smoke_fixture.rs`, `tests/psmux_smoke_mouse.rs`,
  `src/runtime/multiplexer.rs`, `src/runtime/attach.rs`, plus their existing
  private test modules (`multiplexer_tests.rs`, `attach_tests.rs`).

## Explicit non-goals

- Do not change the asserted PageUp/PageDown or SGR mouse byte sequences.
- Do not delete, ignore, or gate the smoke test behind a CI skip variable.
- Do not increase the timeout or retry/re-inject semantic Page keys.
- Do not change psmux production **routing** behavior, public APIs,
  dependencies, workflow files, quality tooling, or `.llxprt/`. (The authorized
  env-scrub isolation fix and explicit `-t` are the only production changes.)
- Do not normalize the three private psmux test harnesses or move unrelated
  tests.
- Do not add a TUI scenario: this is stabilization of an existing native
  transport integration test, not a new UI-visible behavior.
- Do not implement issue #465's deferred P1/P3/P4/P6 harness or diagnostic
  hardening.

## Bounded vertical slices

### Slice 1 — deterministic post-attach mode synchronization

- **Acceptance rows:** AC1, preserves AC2.
- **Architecture owner:** native-Windows integration fixture/test boundary.
- **Allowed files:**
  - `tests/fixtures/psmux_smoke_fixture.rs`
  - `tests/psmux_smoke_mouse.rs`
  - `project-plans/issue456-plan.md`
- **RED (current main):** On current main with `JEFE_REQUIRE_PSMUX=1`,
  repetition 1 fails after 30 seconds because the attached viewer never
  observes mouse reporting.
- **RED (one-client amendment):** The single-`j`-probe/single-viewer
  re-advertisement path forwarded fixture output but the probe input never
  reached the child within the full 30 seconds, even after
  `nudge_for_mode_recovery`. This RED was the DeepThinker trigger.
- **GREEN (amended architecture):** The fixture exposes
  `READINESS_PROBES b"jkl"` and re-advertises exact `MOUSE_MODE_BYTES` before
  any probe's normal byte marker. The test attaches up to three sequential
  candidates, each with a unique probe and byte marker, keeping the first that
  both forwards its own unique marker and observes mouse reporting within one
  shared 30-second deadline (max 10s per candidate). Startup output and exact
  input assertions stay intact.
- **Verification:** fixture unit tests (all three probes + non-probes), focused
  unit/clippy, `cargo fmt --all --check`, and one real psmux smoke with
  `JEFE_REQUIRE_PSMUX=1`. Repeated runs and the full suite are coordinator-owned.
- **Stop conditions:** Stop if synchronization requires production runtime
  changes, a new public abstraction, a new fixture protocol subsystem, a timeout
  increase, or if all three candidates empirically fail.

### Slice 2 — bounded namespace cleanup fallback

- **Acceptance row:** AC3.
- **Architecture owner:** private `PsmuxNamespace` RAII test harness.
- **Allowed files:**
  - `tests/psmux_smoke_mouse.rs`
  - `project-plans/issue456-plan.md`
- **RED evidence:** The current `Drop` path calls unbounded `Command::output()`;
  source inspection proves it bypasses the harness's five-second collector.
  The reproduced timeout returned without a fixture survivor, confirming the
  namespace kill is the correct ownership boundary rather than permission to
  add a process-management subsystem.
- **GREEN:** `Drop` routes namespace-scoped `kill-server` through the existing
  bounded collector and persists its transcript.
- **Verification:** focused smoke repetitions complete; transcript contains the
  bounded cleanup command; external process check finds no fixture survivor.
- **Stop conditions:** Stop if psmux does not reap descendants after a completed
  namespace `kill-server`, or if proof requires a new process-management,
  timeout, cancellation, or cleanup subsystem.

## Expected paths and ownership layers

| Path | Layer / owner | Acceptance mapping |
| --- | --- | --- |
| `project-plans/issue456-plan.md` | Delivery record | Matrix, scope ledger, evidence, review triage |
| `tests/fixtures/psmux_smoke_fixture.rs` | Native psmux fixture | AC1 post-attach mode re-advertisement |
| `tests/psmux_smoke_mouse.rs` | Native psmux integration test/private harness | AC1 ordering, AC2 preserved assertions, AC3 bounded teardown |
| `src/runtime/multiplexer.rs` | Local multiplexer policy | AC1 isolation: scrub inherited `PSMUX_SESSION`/`PSMUX_TARGET_SESSION` on Windows `command()` via shared `PSMUX_INHERITED_SESSION_VARS` |
| `src/runtime/attach.rs` | Local attach command builder | AC1 isolation: scrub inherited tmux+psmux session vars on all attaches; explicit `attach-session -t` |
| `src/runtime/multiplexer_tests.rs` | Multiplexer policy unit tests | Regression coverage for the Windows scrub, base-args preservation, Unix non-scrub, retained vars |
| `src/runtime/attach_tests.rs` | Attach command unit tests | Regression coverage for the scrub helper and explicit `-t` argv on Windows/Unix plans |

The work crosses the test ownership boundary and the local multiplexer/attach
policy boundary (authorized related production isolation fix).

## Scope ledger

| Discovered item | Classification | Decision / mapping | Status |
| --- | --- | --- | --- |
| Current main already fixes PageUp root interception through issue #465/#468 | In-scope context | Preserve the root unbind and single-write Page-key assertion under AC2 | accepted, no new work |
| Current main locally misses the fixture's one-shot mouse modes before attach | Blocker-Fix | AC1 / Slice 1 | accepted |
| One-client readiness-probe path forwarded output but no probe input for full 30s even after nudge | Blocker-Fix | DeepThinker trigger; amended AC1 architecture selected (three sequential candidates, unique probes, one shared deadline) | accepted, DeepThinker count 1 |
| `READINESS_PROBES b"jkl"` replaces single probe; fixture re-advertises `MOUSE_MODE_BYTES` for any probe | In-scope-Fix | AC1 / Slice 1 (fixture) | implemented; unit tests cover all three probes + non-probes |
| Three sequential `AttachedViewer` candidates, unique probes/markers, one shared 30s deadline, max 10s per candidate | In-scope-Fix | AC1 / Slice 1 (test) | implemented; per-attempt diagnostics to transcript |
| Candidate deadline could spend up to `2*cap` (marker then mouse) | Blocker-Fix | AC1 / Slice 1: replaced `forward_probe_until_marker` + `wait_for_mouse_reporting` with conjunctive `poll_marker_and_mouse_reporting` under one shared per-candidate ceiling | implemented; function limits preserved |
| Inherited `PSMUX_SESSION`/`PSMUX_TARGET_SESSION` made Jefe appear nested inside a parent psmux session (root cause) | Blocker-Fix | Authorized related production isolation: `MultiplexerPlan::command` scrubs both on Windows; `attach_command` scrubs TMUX/TMUX_PANE/TMUX_TMPDIR + both psmux session vars on all platforms; shared `PSMUX_INHERITED_SESSION_VARS` constant | implemented; regression unit tests added |
| `attach_command` conditional `-t` (Unix-only) was a stale psmux 3.3.6 workaround | In-scope-Fix | Always emit `attach-session -t <session>` (psmux 3.3.7 minimum accepts it); removed stale comment | implemented; argv regression tests on both plans |
| `PSMUX_CLAUDE_TEAMMATE_MODE` / `PSMUX_CONFIG_FILE` retention | Decision | Retain both: team mode is not session routing, plan already carries `-f NUL` | accepted; regression test asserts retention |
| `PsmuxNamespace::drop` uses unbounded `Command::output()` | In-scope-Fix | AC3 / Slice 2; reuse existing bounded `run`/`run_os` collector only | implemented; cleanup failure recorded in transcript, never panics, transcript persisted after cleanup |
| All three candidates empirically failed before the isolation fix (single real run) | Empirical RED (superseded) | Was the stop condition; the inherited-PSMUX root-cause fix made the first candidate pass in ~2.7s | superseded by GREEN |
| Shared psmux harness, loader retry, readiness diagnostics, and expanded capture diagnostics | Defer | Already recorded as issue #465 P1/P3/P4/P6 follow-ups; not required for AC1-AC3 | deferred |
| Production attach recovery changes (nudge) | Reject | Existing production nudge is unrelated; only the authorized env-scrub isolation and explicit `-t` are in scope | rejected (except authorized isolation) |

## Scope budget

- Changed files: 7 (`project-plans/issue456-plan.md`,
  `tests/fixtures/psmux_smoke_fixture.rs`, `tests/psmux_smoke_mouse.rs`,
  `src/runtime/multiplexer.rs`, `src/runtime/attach.rs`,
  `src/runtime/multiplexer_tests.rs`, `src/runtime/attach_tests.rs`). The two
  test modules are existing files, not new modules.
- Net changed lines: approximately 845 including the delivery plan (production
  isolation + conjunctive polling + regression tests + plan amendments), below
  the 1,500-line scope-review trigger.
- Ownership layers: test fixture, integration test harness, local multiplexer
  policy, local attach command builder.
- Hard stop: more than 40 files or 2,500 net lines; mandatory scope review above
  25 files or 1,500 net lines.

## Review counters

- DeepThinker architecture analysis: 1.
- Local OCR: 2/2 (both actual CLI sessions were partial/aborted after four of
  six reviewable files; first reported two findings, second reported none).
- Independent GLM/zai local audit: pass; all seven changed paths reviewed.
- PR OCR: 0/2.

## Verification evidence

### RED on current main

- Head: `a38f763d885128955dab7ede309d76c13df5de45`.
- psmux: `3.3.7`.
- Command: `JEFE_REQUIRE_PSMUX=1 cargo test --features psmux-smoke --test psmux_smoke_mouse -- --nocapture --test-threads=1`.
- Result: failed on repetition 1 after 32.40 seconds at
  `tests/psmux_smoke_mouse.rs:102`:
  `AttachedViewer never observed mouse reporting after fixture advertised 1000/1002/1006`.
- Artifact:
  `target/psmux-smoke/jefe-psmux-mouse-mode-19372-18c628f7cfe19528/transcript.txt`
  shows a healthy pane containing `ALT_SCREEN` and `PSMUX_SMOKE_READY`.
- Post-failure process check found no `jefe-psmux-smoke-fixture` process.

### GREEN and exact-head evidence

To be filled after each completed slice. Interrupted, skipped, stale-head, or
partial commands do not count.

### Amended architecture implementation evidence

- **Branch:** `issue456`.
- **Allowed files changed:** `src/runtime/attach.rs`,
  `src/runtime/attach_tests.rs`, `src/runtime/multiplexer.rs`,
  `src/runtime/multiplexer_tests.rs`, `tests/fixtures/psmux_smoke_fixture.rs`,
  `tests/psmux_smoke_mouse.rs`, and `project-plans/issue456-plan.md`.
- **Related production changes:** local attach and native-Windows multiplexer
  commands scrub inherited psmux session-routing variables; attach uses the
  explicit psmux 3.3.7 `-t` target form.
- **RED artifacts preserved:** the one-client readiness-probe transcript
  (`target/psmux-smoke/jefe-psmux-mouse-mode-19372-18c628f7cfe19528/transcript.txt`)
  is unchanged working context; the three-candidate transcript is
  `target/psmux-smoke/jefe-psmux-mouse-mode-22432-18c632ded55cb744/transcript.txt`.

#### Fixture unit tests (GREEN)

- Command: `cargo test --features psmux-smoke --bin jefe-psmux-smoke-fixture`.
- Result: `test result: ok. 5 passed; 0 failed`, including
  `readiness_probe_emits_mouse_modes_before_marker` (covers `j`, `k`, `l` in
  1000/1002/1006 order) and `non_probe_input_emits_nothing_before_marker`.

#### Format and clippy (GREEN)

- `cargo fmt --all --check`: clean after `cargo fmt --all`.
- `cargo clippy --features psmux-smoke --test psmux_smoke_mouse -- -D warnings`:
  clean.
- `cargo clippy --features psmux-smoke --bin jefe-psmux-smoke-fixture -- -D warnings`:
  clean.

#### Real psmux smoke — three sequential candidates (RED, superseded by GREEN)

- psmux: `3.3.7`.
- Command: `JEFE_REQUIRE_PSMUX=1 cargo test --features psmux-smoke --test psmux_smoke_mouse -- --nocapture --test-threads=1`.
- Result (pre-isolation-fix): `FAILED. 0 passed; 1 failed` after 31.56 seconds.
- All three candidates failed to forward their own unique marker because Jefe
  itself ran inside a parent psmux session and inherited
  `PSMUX_SESSION`/`PSMUX_TARGET_SESSION`, so psmux refused the nested attach.
- This RED is **superseded** by the GREEN run after the authorized production
  isolation fix (see below). The inherited-PSMUX root cause was the real
  blocker; the three-candidate retry alone could not fix a refused attach.

### GREEN after authorized production isolation fix (first candidate)

- psmux: `3.3.7`.
- Command: `JEFE_REQUIRE_PSMUX=1 cargo test --features psmux-smoke --test psmux_smoke_mouse -- --nocapture --test-threads=1`.
- Result: `test result: ok. 1 passed; 0 failed` in **2.68s** (wall ~4.7s incl.
  compile). The first candidate qualified: marker + mouse reporting were
  observed conjunctively under the shared per-candidate ceiling, and the
  Page-key + SGR semantic assertions followed.
- AC3 teardown verified: post-run `Get-Process` found **no**
  `jefe-psmux-smoke-fixture` process, confirming the bounded `kill-server`
  reaped the fixture descendants. (Lingering `psmux` servers are pre-existing
  user-environment namespace servers, not test descendants; the test's own
  unique `-L` namespace is killed.)

### Focused unit tests (GREEN)

- `cargo test --lib multiplexer` → 18 passed (incl. 5 new
  `*_command_*`/`psmux_inherited_session_vars_*` regression tests).
- `cargo test --lib "runtime::attach::tests"` → 22 passed (incl. 3 new
  `scrub_helper_*`/`attach_command_*_plan_*` regression tests).
- `cargo test --features psmux-smoke --bin jefe-psmux-smoke-fixture` → 5 passed.

### Sibling psmux tests under JEFE_REQUIRE_PSMUX=1 (GREEN)

- `cargo test --features psmux-smoke --test psmux_attach` → 2 passed in 3.62s
  (exercises the same `attach_command` path with the new explicit `-t` and env
  scrubbing).
- `cargo test --features psmux-smoke --test psmux_orphan_reap` → 2 passed in
  3.68s (confirms the namespace reaping contract).

### Format and clippy (GREEN)

- `cargo fmt --all --check`: clean.
- `cargo clippy --lib -- -D warnings`: clean.
- `cargo clippy --tests -- -D warnings`: clean.
- `cargo clippy --features psmux-smoke --test psmux_smoke_mouse -- -D warnings`:
  clean.
- `cargo clippy --features psmux-smoke --bin jefe-psmux-smoke-fixture -- -D warnings`:
  clean.

### Ten consecutive native-Windows repetitions (GREEN)

- psmux: `3.3.7`.
- Command: loop ten invocations of
  `JEFE_REQUIRE_PSMUX=1 cargo test --features psmux-smoke --test psmux_smoke_mouse -- --nocapture --test-threads=1`.
- Result: **10/10 passed**, each in 2.52–2.84 seconds.
- Post-loop process check found no `jefe-psmux-smoke-fixture` survivor.

### Quick-check equivalent (GREEN)

- GNU `make` is unavailable on this Windows host, so the Makefile's exact
  `quick-check` commands were run directly: `cargo fmt --all`,
  `cargo check -q`, and `cargo test -q`.
- Result: all passed; primary test targets reported 2,330 and 760 passing tests
  with no failures, followed by all integration/doctest targets passing.

### Rebased-candidate full local verification (GREEN)

- Candidate base/head before issue commit: `4ed77d5218ff8b119a213f6d30569eb9fceeb640`
  (`origin/main`, after a clean autostash rebase from the original `a38f763`
  baseline; all seven issue files restored without conflict).
- Command: `cargo xtask ci` (current main replaced the former Makefile gate with
  the cross-platform xtask equivalent).
- Result: **passed** all 9 steps: format, clippy-allow policy, source-size,
  architecture, strict all-target/all-feature Clippy, complexity Clippy,
  coverage (**47.87%**, floor 30%), locked all-feature build, and locked
  all-feature workspace tests. Elapsed 178.6 seconds.
- The Windows psmux smoke target passed within the locked all-feature test step.

### Not yet claimed

- Exact committed-head verification and PR CI are not yet claimed.

## Review triage

- Local OCR 1, session `6f9364a1-45bd-4090-bd36-b0b7b9befef5`:
  **partial/aborted** after four of six reviewable files, with two findings.
  - **In-scope-Fix (valid):** the Windows attach test name/doc claimed to prove
    scrubbing while its assertions covered argv and `TERM`; renamed/documented
    it as explicit-target coverage and pointed to the dedicated scrub test.
  - **In-scope-Fix (valid):** Unix explicit-target coverage omitted the common
    `TERM=xterm-256color` assertion; added parity coverage.
- Local OCR 2, session `63e736f9-bc4e-4c86-8dc9-ee636bd9d505`:
  **partial/aborted** after the same four production/unit files, with no
  findings. It is not represented as clean full-diff coverage.
- Independent GLM/zai local audit: **pass** across all seven changed paths; no
  additional Blocker-Fix, In-scope-Fix, or Reject findings.
- **Defer:** duplicated private psmux smoke harness support is pre-existing and
  already tracked among issue #465 follow-ups; it is not required for AC1-AC3.

## Deferred findings / follow-ups

- Issue #465's P1/P3/P4/P6 items remain deferred and are not absorbed here.
- No optional hardening will be added after AC1-AC3 and required gates pass.
