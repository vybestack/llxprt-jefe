# Issue #467 — Make Windows agent sessions rebuild-safe and reliably reconnectable

## Problem

Native Windows psmux sessions use the currently running dashboard image as each
pane's long-lived launcher (`jefe.exe --jefe-internal-agent-launch`). Ctrl-Q and
dashboard crashes correctly leave those sessions alive, but the surviving
launchers lock the rebuild target. Killing the launchers to unblock a build kills
the panes and can strand Bun/Node descendants, making terminal reconnection
impossible. Issue #332/#416 added orphan recovery but not prevention.

## Architecture decision

On Windows, stage an immutable **per-session, content-addressed copy** of the Jefe
image below the resolved state root and use that copy as the psmux pane host:

```text
<state-root>/session-hosts/<sanitized-session>/<sha256>/jefe-session-host.exe
```

The copy—not the build/install target—runs the existing private launch-plan
entrypoint. The host creates and owns a kill-on-close Windows Job Object before
spawning the agent worker. The dashboard never owns the Job handle, so dashboard
quit/crash leaves the psmux host and worker alive. Host death closes the handle
and terminates the owned worker tree.

The session name makes artifact ownership deterministic, so no state schema
change is required: restart/kill/delete derive the session directory from the
existing `RuntimeBinding.session_name`. The binary digest allows old and new
host images to coexist without overwriting a running Windows image. Existing
#416 PID-identity-validated orphan reaping remains defense in depth.

The resolved state-file parent is passed to `TmuxRuntimeManager`; production,
`--config`, `JEFE_STATE_DIR`, and isolated tests therefore use the same path
authority without process-environment mutation.

### Dependency decision — approved

The existing `winsafe = 0.0.27` kernel feature does **not** expose Job Object
creation, limit configuration, or process assignment (confirmed against the
installed crate source). Project source forbids `unsafe`, so direct Win32 FFI is
not allowed. Completing AC6 requires adding the Windows-only safe wrapper:

```toml
win32job = "2.0.3"
```

This crate provides safe `Job::create`, `ExtendedLimitInfo::limit_kill_on_job_close`,
`Job::set_extended_limit_info`, and `Job::assign_current_process` APIs. The user
explicitly approved this Windows-target-only dependency on 2026-07-27; Unix and
remote execution paths remain unchanged.

## Acceptance matrix

| # | Path / boundary | Success | Failure / diagnostic | Proof |
|---|---|---|---|---|
| AC1 | Windows local launch | psmux pane command uses immutable copied host outside build target; argv/env/cwd unchanged | typed staging error names operation and safe path; no pane spawned | planner/staging/command tests + native psmux |
| AC2 | Ctrl-Q, one or multiple agents | dashboard exits; all original panes/workers remain healthy and attachable | no healthy binding is cleared | native lifecycle regression |
| AC3 | dashboard forced termination | same survival/reconnect behavior as Ctrl-Q | stale/dead pane is diagnosed, not called healthy | native lifecycle regression |
| AC4 | rebuild/replace after dashboard exit | source `jefe.exe` can be overwritten while sessions run | diagnostics identify source/host/pane when replacement fails | native image-replacement test |
| AC5 | restart | exact original pane is restored, exactly one worker, no duplicate `--continue` | typed restore result; binding retained when pane is live | manager/startup + native regression |
| AC6 | pane-host death | Job Object closes and worker descendants terminate; #416 reaper remains fallback | survivors are boundedly detected and reported | native Job Object process-tree test |
| AC7 | explicit kill/delete | target psmux session and its session-host directory are cleaned; unrelated sessions untouched | cleanup is best effort and retained for retry | cleanup truth table + runtime integration |
| AC8 | startup cleanup/legacy state | unreferenced, non-running staged versions and interrupted temp files are removed; legacy bindings load unchanged | live/ambiguous artifacts retained and logged | cleanup tests + existing serde suite |
| AC9 | Unix/remote | existing tmux/remote launch path unchanged | Windows host path never selected | structural tests + full CI |
| AC10 | end-to-end Windows | multiple agents → quit/crash → replace binary → restart → reconnect all; no duplicate, lock, dead binding, orphan, or unrelated cleanup | namespace-scoped transcript retained | required real-psmux regression |

## Non-goals

- Killing healthy agents when the dashboard exits.
- Reattaching to arbitrary Bun/Node processes after their PTY pane is dead.
- Changing remote process ownership or Unix/tmux lifecycle.
- Replacing existing #416 orphan safeguards.
- Adding a daemon/supervisor subsystem.
- Unrelated persistence, UI, or runtime refactoring.

## Vertical slices

### Slice 1 — RED: host staging and command contract

**Rows:** AC1, AC4, AC8, AC9.

**Expected paths:**
- `src/runtime/session_host.rs` (new)
- `src/runtime/session_host_tests.rs` (new)
- `src/runtime/multiplexer.rs`
- `src/runtime/multiplexer_tests.rs`
- `src/runtime/mod.rs`

**RED:** tests prove deterministic sanitized/content-addressed paths, copy rather
than hardlink, idempotent staging, atomic temp cleanup, typed errors, Windows pane
uses the copy, Unix uses its existing direct command, and a staged running-copy
fixture does not lock the source image.

**GREEN:** implement pure path planning and Windows staging. Use the repository's
dependency-free `domain::sha256::Sha256`; no hashing dependency.

### Slice 2 — RED: lifecycle root ownership and cleanup

**Rows:** AC5, AC7, AC8.

**Expected paths:**
- `src/runtime/manager.rs`
- `src/runtime/commands.rs`
- `src/runtime/session_host.rs`
- `src/main.rs`
- focused runtime tests

**RED:** explicit resolved state-root reaches local creation; reattach does not
stage/replace an existing host; kill removes only the target session directory;
startup cleanup retains referenced/live session directories and removes only
unreferenced/dead versions.

**GREEN:** add a manager constructor accepting the resolved session-host root,
thread the session name/root through local launch, and perform bounded,
agent-scoped cleanup. No `RuntimeBinding` schema change.

### Slice 3 — RED: host-owned Job Object containment

**Rows:** AC3, AC6.

**Expected paths:**
- `Cargo.toml` / `Cargo.lock` (only after explicit dependency approval)
- `src/runtime/job_object.rs` (new)
- `src/runtime/agent_launcher.rs`
- focused Windows test/fixture

**RED:** a host launches a long-lived child, host termination closes the Job, and
the child exits within the bound. Dashboard process lifetime is absent from this
ownership chain.

**GREEN:** create/configure/assign the Job from the private host entrypoint and
hold its safe handle for the full worker lifetime. Existing Unix code is
unchanged.

### Slice 4 — RED: native psmux quit/crash/rebuild/reconnect

**Rows:** AC2–AC7, AC10.

**Expected paths:**
- `tests/psmux_session_host.rs` (new or bounded extension of smoke suite)
- deterministic fixture under `tests/fixtures/`
- `Cargo.toml` fixture target if needed
- `dev-docs/testing/psmux-smoke.md`

**RED:** with a unique `-L` namespace, prove the current source launcher locks the
source and host death strands a child. Test owns cleanup via Drop and retains a
transcript.

**GREEN:** two or more staged hosts survive simulated dashboard quit/crash;
source executable is replaced; same pane PIDs/session names remain; exactly one
worker per pane; killing one host reaps only its tree; explicit cleanup removes
only its artifact directory.

## Scope ledger

| Date | Item | Disposition |
|---|---|---|
| 2026-07-27 | Issue #467 filed with complete prevention + containment scope | Accepted |
| 2026-07-27 | Branch `issue467` created from current `origin/main` (`d391efe`) | Accepted |
| 2026-07-27 | Deep architecture subagent timed out; GLM implementation analysis completed | Recorded |
| 2026-07-27 | Corrected proposed persistence expansion: session-name-derived ownership avoids a new persisted host-image field | Accepted, reduces scope |
| 2026-07-27 | Installed winsafe has no Job Object API; safe wrapper dependency is required because repository source forbids unsafe | **Approved by user; Windows target only** |
| 2026-07-27 | Mandatory scope count: 20 files, 2,693 net lines; complete lifecycle and native regression exceeded the 2,500-line hard threshold | **Approved by user to continue** |

## Expected budget

- Target: 12–18 files, approximately 700–1,200 net lines.
- Mandatory scope review above 25 files or 1,500 net lines.
- Hard stop without explicit approval above 40 files or 2,500 net lines.
- No `.llxprt/`, `.code_puppy/`, `.github/`, workflow, or quality-gate changes.

## Review counters

- OCR pre-PR: 1/2
- OCR post-PR: 0/2

## Verification evidence

### Slice 3 — host-owned Job Object containment (AC3, AC6)

**Status:** RED → GREEN complete for the in-crate containment boundary.

**Files (within Slice 3 allowlist):**
- `src/runtime/job_object.rs` (new) — narrow boundary owning every safe
  `win32job` call: `JobObjectError` (typed create/query/configure/assign),
  `JobContainment::enable_for_current_process` (create + configure
  `KILL_ON_JOB_CLOSE` + assign current process), `contain_handle` (cfg(test)
  helper to contain a spawned child without assigning the test runner).
- `src/runtime/job_object_tests.rs` (new, `cfg(all(test, windows))`) — three
  behavioral contracts.
- `src/runtime/agent_launcher.rs` — `run_launch_plan` establishes containment on
  Windows before `.status()` and holds the guard for the full worker lifetime;
  new `AgentLauncherError::ContainmentUnavailable` (cfg(windows)) refuses spawn
  on failure; Unix path byte-for-byte unchanged.
- `src/runtime/agent_executable_tests.rs` — contract test asserting the typed
  containment refusal diagnostic on Windows and the absence of any containment
  mention on Unix.
- `src/runtime/mod.rs` — module wiring (`#[cfg(windows)] mod job_object;` and
  the cfg-gated test module).
- `Cargo.toml` / `Cargo.lock` — `win32job = "2.0.3"` under
  `[target.'cfg(windows)'.dependencies]` (pre-approved dependency, unchanged).

**RED evidence:** the two behavioral tests
(`enabling_containment_for_current_process_yields_kill_on_job_close_guard`,
`native_kill_on_job_close_terminates_a_contained_descendant_within_bound`)
failed against the stub with
`failed to create windows job object` before the win32job-backed GREEN landed.

**GREEN evidence (native Windows, `CARGO_TARGET_DIR=target/issue467`):**
```
test runtime::job_object_tests::enabling_containment_for_current_process_yields_kill_on_job_close_guard ... ok
test runtime::job_object_tests::job_object_error_variants_are_typed_and_name_the_failing_operation ... ok
test runtime::job_object_tests::native_kill_on_job_close_terminates_a_contained_descendant_within_bound ... ok
test result: ok. 3 passed; 0 failed
```
The native containment proof spawns a long-lived `ping` child, contains ONLY the
spawned child (never the test runner — the guard's `enable_for_current_process`
path is invoked with `mem::forget` so its handle never closes mid-suite), then
drops the guard and asserts the child exits within a 5s bound — proving
`KILL_ON_JOB_CLOSE` reaps the descendant tree on handle release.

Full runtime lib suite green (339 passed, 0 failed) confirms the
`run_launch_plan` wiring introduced no regressions.

**AC3 ownership:** the dashboard process is absent from the Job ownership chain.
The Job handle lives only inside `run_launch_plan` (the private pane host
entrypoint), so a dashboard quit/crash cannot close it. AC3 native lifecycle
proof is owned by Slice 4.

**Safety:** no `unsafe` in Jefe source (crate-level `unsafe_code = "forbid"`);
all Win32 interaction is via the safe `win32job` wrapper.

**Native host-death proof (full AC6 end-to-end):** the in-crate test proves the
kernel mechanism (Job handle release → contained descendant termination within
bound). A dedicated subprocess fixture proving *host-process-exit* closes the
owned handle and reaps the worker tree is deferred to Slice 4, which owns the
native psmux regression; Slice 3 must not add a Cargo bin fixture outside its
allowlist.

### Slice 3 verification gap (pre-existing, not introduced here)

`cargo fmt --all --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings` do not
pass on the current working tree because of **pre-existing Slice 1/2** fmt diffs
and one clippy `too many arguments` error in `src/runtime/commands.rs` /
`session_host.rs` / `session_host_tests.rs` / `main.rs`. Slice 3 is forbidden
from modifying Slice 2 files, so those gates cannot be cleared from this slice.
Slice 3's own files (`job_object.rs`, `job_object_tests.rs`, `agent_launcher.rs`,
`agent_executable_tests.rs`, `mod.rs` additions) are individually fmt- and
clippy-clean.

Required exact-head gates:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

Native Windows proof:

```text
JEFE_REQUIRE_PSMUX=1 cargo test --features psmux-smoke --test psmux_session_host -- --nocapture
```

### Slice 4 and exact-head evidence

- Native Windows psmux lifecycle: `JEFE_REQUIRE_PSMUX=1 cargo test --features
  psmux-smoke --test psmux_session_host -- --nocapture` — **1 passed, 0 failed**.
- `cargo fmt --all --check` — passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — passed.
- `cargo build --workspace --all-features --locked` — passed with
  `CARGO_TARGET_DIR=target/issue467` so legacy running Jefe images do not lock
  the candidate build output.
- `cargo test --workspace --all-features --locked` — all issue-relevant and
  library targets passed; the pre-existing native `tests/psmux_attach.rs`
  input-byte contract fails in isolation with unrelated extra terminal input.
  No issue #467 file modifies that test or its attach/input route.

### Pre-PR review 1 triage

- **In-scope—Fix:** preserve staging failures as typed
  `RuntimeError::SessionHostStaging(SessionHostError)` — fixed.
- **In-scope—Fix:** remove unused no-Job fixture/test-driver scaffolding — fixed.
- **Reject:** Job-guard `mem::forget` in the isolated test and fixture — required
  to keep the kill-on-close handle alive until host process termination.
- **Reject:** startup/kill cleanup concurrency and scoping concerns — retained
  directories are live, referenced, unprobeable, or ambiguous; removal is only
  for missing unreferenced sessions and target-session explicit cleanup.

## Deferred findings

None. Every accepted behavior AC1–AC10 is in scope; no prevention work is deferred.
