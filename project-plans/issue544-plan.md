# Issue #544 — Remove no-fallback hard gates from the Windows launch pipeline

Branch: `issue544`. Base: `main` @ `52f8b6ed`.

Restores invariant **I1** / epic criterion **E7**:

> No gate in the Windows launch pipeline may be an unconditional refusal. Every gate
> either succeeds, degrades to a defined fallback, or fails with a diagnostic that names
> the gate, the cause, and the remediation — surfaced to the user, at the point of
> failure, and copyable.

Parent of #529, #435, #536 (#536 already CLOSED).

---

## 1. Baseline audit (read-only, completed before any edit)

Thirteen gates enumerated against `52f8b6ed`. Findings that drive the slices:

| # | Gate | File::fn | Names its gate today? | Fallback? |
|---|---|---|---|---|
| 1 | `prepare_launch` | `launch_compose.rs:75/:83` | **NO** — bare `spawn failed: {msg}` | none |
| 2 | `PathSnapshot::resolve_binary` | `agent_candidate_path.rs` | **NO** — `spawn failed: configured agent executable was not found` | `None` upstream |
| 3 | `AgentExecutableResolver` | `agent_executable.rs` | partial — `agent launch unavailable:` | none |
| 4 | `run_local_agent_probe` | `agent_probe.rs:167` | YES — `AGT-E201/E202/E203` | diagnostic-only for the list |
| 4b | managed npm install | `package_runtime.rs:562` | **NO** — folded into `AGT-E202` | volatile-version resolve falls back |
| 5 | capability / support matrix | `launch_compose.rs:278` | **NO** | diagnostic-only for the list |
| 6 | `authorize_execution` | `agent_execution_guard.rs` | YES — `AGT-E203` + dimension | none |
| 7 | `prepare_execution` preflight | `agent_preflight.rs` | **NO** | none |
| 8 | `prepare_fresh_send` | `agent_fresh_send.rs` | **NO** | prompt compaction/truncation |
| 9 | session-host staging | `session_host.rs:117` | YES — `session host staging failed:` | none |
| 10 | `write_launch_plan` | `agent_launcher.rs:63` | YES — `Windows agent launch plan preparation failed:` | none |
| 11 | psmux + `windows_pane_command_args` | `multiplexer.rs:649` | YES — `multiplexer dependency failed:` | none |
| 12 | Job Object + owner anchor | `job_object.rs`, `owner_anchor.rs` | YES | **none — hard refusal** |
| 13 | final spawn | `agent_launcher.rs:149` | **NO** — `agent process could not be started` | none |

Other confirmed facts:

- `pane_command_budget()` (`multiplexer_contract.rs:460`) is consumed **only** by
  `app_input/fresh_prompt.rs` for prompt sizing. `windows_pane_command_args`
  performs **no** length check. (V7 gap.)
- #435 is **entirely unimplemented**: zero source references. A launch failure is
  stashed into `state.error_message`, rendered as a ≤50-char `ERR:` status-bar line,
  and is copyable only after navigating to the Errors screen.
- A complete copyable-selection model already exists (`selection/text.rs`
  `SelectablePane`, `selection/errors_content.rs`, `selection/overlay_content.rs`
  `confirm_modal_lines`). The new surface reuses it rather than inventing one.
- #529: post-`27f6d21c` (#624, `npm view` now runs with `current_dir(cache_root)`)
  the reported root cause appears fixed. **No test forces a Windows nonblank-selector
  install end-to-end**, which is why it stayed unproven. Remaining plausible refusal
  is `run_npm_install` → `resolve_target(Npm)` not finding the official Node.js layout.
- No launch-continuing fallback exists anywhere in the pipeline today.

---

## 2. Decisions requiring explicit approval

**D1 — launch-plan transport (blocks Slice F).**
The issue asks for "an in-process transport". A true in-process transport on Windows
means a named pipe, which requires either a new dependency (`interprocess`/`tokio`) or
`unsafe`. `unsafe_code = "forbid"` is a crate-level lint and dependency additions need
explicit approval (workflow §2/§6). **Proposed instead:** move the plan out of the
shared `%TEMP%` into jefe's existing private per-user state directory
(`%LOCALAPPDATA%\jefe\launch-plans`, honouring `JEFE_STATE_DIR`), and replace the
`canonicalize == temp_dir()` check with containment under that owned directory. This
removes all three fatal modes named in the issue (world-readable shared dir, redirected
/junctioned `%TEMP%`, unwritable `%TEMP%`) and satisfies every V3 test, with no new
dependency, no new subsystem, and no `unsafe`. **Status: awaiting user approval.**

**D2 — owner anchor stays fail-closed.**
#544 names `ContainmentUnavailable` specifically. `OwnerAnchorUnavailable` is the
contract #542 deliberately introduced two commits ago ("no owner means no spawn").
Degrading it would reintroduce the unowned-survivor defect. Recorded as an explicit
**non-goal**; only Job-Object containment degrades.

**D3 — #529 is proven, not re-patched.**
The issue forbids a fourth point-patch. Slice E adds the missing end-to-end coverage
for all three selectors against the production install/resolve/probe/launch boundary.
A new fix is applied only if that coverage goes RED.

---

## 3. Acceptance matrix

| ID | Requirement | Behavioural proof | Slice |
|---|---|---|---|
| A1 | Every gate is declared with a name, precondition and failure behaviour in one authority | `LaunchGate` enum + `dev-docs/standards/windows-launch-pipeline.md` | A |
| A2 | Adding a gate without a declared failure behaviour fails the build | exhaustive `match` over `LaunchGate` in `describe`/`failure_behaviour` + a test asserting every variant is declared and unique | A |
| A3 | Every launch refusal carries its gate, cause and remediation | `LaunchGateFailure` threaded through `RuntimeError`; Display begins with the gate name | B |
| A4 | Gates 1, 2, 5, 7, 8, 13 no longer collapse to bare `spawn failed:` | per-gate Display assertions | B |
| A5 | A failed launch surfaces immediately in a dismissible surface, untruncated | reducer + render test; TUI scenario | C |
| A6 | That surface is copyable through the existing OSC-52 selection path | `SelectablePane::LaunchFailure` projection test | C |
| A7 | Transient issue/PR launchers stop discarding the real diagnostic | failure-path tests on both transient launchers | C |
| A8 | Errors ring buffer still receives the failure | existing `capture_runtime_errors` assertions retained | C |
| A9 | Job Object unavailable → agent still launches, degraded, with a visible warning | fault-injected containment failure + degraded-mode assertion | D |
| A10 | The degraded mode is documented and named in the warning | doc + Display assertion | D |
| A11 | All three nonblank selectors resolve, install and launch on Windows | end-to-end selector coverage at the production boundary | E |
| A12 | Blank selector behaviour unchanged | regression test | E |
| A13 | Launch-plan transport does not depend on a shared temp directory | redirected/junctioned `%TEMP%`, unwritable `%TEMP%`, concurrent second instance | F |
| A14 | A pane command over the measured budget fails with a specific diagnostic | over-budget construction test; never truncated | G |
| A15 | Every gate has a fault-injection test forcing its failure | per-gate test inventory test | H |
| A16 | macOS/Linux launch behaviour unchanged | existing suite green; no `cfg`-free behaviour change on Unix paths | all |

## 4. Non-goals

- `OwnerAnchorUnavailable` remains a hard refusal (D2).
- No redesign of candidate ordering, remote package selection, or shell policy.
- No new dependency; no `unsafe`.
- No change to `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifests or
  quality-gate configuration.
- Blank-Version semantics, LLxprt profile/`--yolo`/`--continue`/sandbox/resume
  arguments are untouched.
- Removing the PowerShell wrapper for `.cmd` targets is **deferred**: the wrapper is
  psmux's pane-command contract, not jefe's choice, and #619 (`4070551a`) just
  established the current framing. Recorded as a follow-up.

## 5. Slices

| Slice | Behaviour | Primary paths |
|---|---|---|
| A | Gate registry + declared failure behaviour + mechanical check | `src/runtime/launch_gates.rs` (new), `dev-docs/standards/windows-launch-pipeline.md` (new) |
| B | Every refusal names its gate | `runtime/errors.rs`, `launch_compose.rs`, `agent_preflight.rs`, `agent_fresh_send.rs`, `agent_launcher.rs`, `package_runtime.rs` |
| C | At-point-of-failure copyable surface (#435) | `state/`, `app_input/`, `selection/`, `ui/`, new TUI scenario |
| D | Job Object degradation (V2) | `runtime/job_object.rs`, `runtime/agent_launcher.rs` |
| E | #529 proof for all three selectors (V4) | `runtime/package_runtime*`, tests |
| F | Launch-plan transport off shared temp (V3) — **blocked on D1** | `runtime/agent_launcher.rs`, `persistence/paths.rs` |
| G | Pane-command budget enforcement (V7) | `runtime/multiplexer.rs` |
| H | Fault-injection test per gate (V1) | tests |

## 6. Scope ledger

| Entry | Status |
|---|---|
| Baseline audit | done, read-only |
| D1 transport choice | **awaiting approval** |

## 7. Review counters

- OCR before PR: 0 / 2
- OCR after PR: 0 / 2

## 8. Verification evidence

_(recorded per slice as it goes green)_
