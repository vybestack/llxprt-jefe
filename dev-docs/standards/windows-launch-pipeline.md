# The agent launch pipeline and its declared gates

Issue #544, invariant **I1**.

> No gate in the launch pipeline may be an unconditional refusal. Every gate either
> succeeds, degrades to a defined fallback, or fails with a diagnostic that names the
> gate, the cause, and the remediation — surfaced to the user, at the point of failure,
> and copyable.

This document is the contract. `src/runtime/launch_gates.rs` is its executable form:
`LaunchGate` declares one variant per row below, and every accessor on it is an
exhaustive match, so **a new gate cannot be added without declaring its precondition,
its failure behaviour and its remediation — the build fails first**. A test in that
module additionally asserts that every declared gate id and every declared failure
behaviour appears in this file, so a gate cannot be added to the code without being
documented here.

## Why the pipeline needs this

macOS reaches a running agent in roughly four gates. Windows takes fifteen, seven of
them Windows-only: staging, the launch-plan transport, the PowerShell pane command,
Job-Object containment and the owner anchor have no macOS equivalent at all. Every one
of those was written as an unconditional refusal, and several failed with a diagnostic
naming no stage, so a user could not tell which of fifteen gates had stopped them.
#519 → #525 → #529 is the same class of defect filed three times; #530 sat latent for
seventeen days because nobody asked what a gate does when its input is wrong.

## Failure behaviours

| Behaviour | Meaning |
|---|---|
| `refuse` | The launch stops. The diagnostic **must** name the gate, the observed cause, and a remediation, and must reach the user at the point of failure. |
| `degrade` | The launch continues in a named, documented lesser mode, and the user sees a warning that names the mode. |

`degrade` is only correct where the lost property is not a safety property. Containment
is a cleanup guarantee — losing it leaves a process the user can still see and kill.
Ownership, authorization and working-directory correctness are not: losing them
produces a worker nobody owns, a worker built from stale evidence, or a worker running
against the wrong repository. Those stay `refuse`.

## The gates

Ordinal order is execution order.

| # | id | Precondition | Failure behaviour | Remediation given to the user |
|---|---|---|---|---|
| 0 | `launch-composition` | A complete launch request with the evidence captured for it | `refuse` | Reopen the agent and check its type, target, version selector and typed field values |
| 1 | `executable-discovery` | An immutable PATH/PATHEXT snapshot and a binary name | `refuse` | Install the agent, or correct the configured command so it resolves on PATH |
| 2 | `executable-strategy` | A resolved binary classified as a direct executable or a wrapper script | `refuse` | Reinstall the agent through its official installer so its launcher layout is complete |
| 3 | `identity-probe` | A resolved candidate and a monotonic probe generation | `refuse` | Run the agent's own version command by hand and fix what it reports |
| 4 | `managed-package-install` | A nonblank version selector, a writable cache root, and a working npm | `refuse` | Check network access to the npm registry and that Node.js and npm are installed |
| 5 | `capability-support` | A probed agent whose capabilities cover the requested operation | `refuse` | Choose an agent version that supports this operation, or change the operation |
| 6 | `execution-authorization` | Definition, executable, target, probe and activation evidence all still current | `refuse` | Reprobe the agent and launch again; the executable or configuration changed underneath |
| 7 | `sandbox-preflight` | The configured sandbox engine and image are present and inspectable | `refuse` | Start or install the sandbox engine, or turn the sandbox off for this agent |
| 8 | `prompt-assembly` | Exactly one typed prompt that fits the measured pane-command budget | `refuse` | Shorten the prompt, or send the issue reference instead of its full body |
| 9 | `session-host-staging` | Windows, a valid session name, and a readable host image | `refuse` | Free space in the jefe state directory and check that antivirus is not quarantining the staged host |
| 10 | `launch-plan-transport` | A private directory jefe owns and can write | `refuse` | Check that the jefe state directory exists and is writable |
| 11 | `pane-command` | A validated multiplexer and a pane command within the measured budget | `refuse` | Install the required psmux build, or shorten the launch so the pane command fits the budget |
| 12 | `worker-containment` | Windows, a Job Object the session host can create and own | `degrade` | None required; the agent runs uncontained and must be closed from its own pane |
| 13 | `owner-anchor` | Windows, an identifiable owning process for the session host | `refuse` | Launch jefe from a normal terminal; the current host does not expose an owner chain |
| 14 | `worker-spawn` | A consumable launch plan and an existing working directory | `refuse` | Check that the agent's working directory still exists and that the executable is runnable |

## Degraded mode: `uncontained-worker`

Gate `worker-containment` degrades when a Job Object cannot be created — most often
because jefe was started inside a pre-existing Job that does not permit breakaway, as
some IDE integrated terminals, CI runners and remote-session hosts do. jefe does not
control the environment it is launched from, so refusing there means the user can never
launch an agent at all.

In `uncontained-worker` mode the agent is spawned normally and behaves normally. What is
lost is only automatic cleanup: if the session host dies abnormally, the kernel will not
reap the agent's descendant tree, and the agent may survive as an orphan. jefe's
existing orphan detection and reaping (#332) still observes and can still clean up such
a worker, so the degradation is bounded and recoverable. The user is warned, by name,
at the point of launch.

## Adding a gate

1. Add the variant to `LaunchGate` in `src/runtime/launch_gates.rs`. The build now fails
   until you declare its id, precondition, failure behaviour and remediation.
2. Add it to `LaunchGate::ALL` in execution order.
3. Add its row to the table above. The registry test fails until you do.
4. Add a fault-injection test that forces the gate to fail and asserts the declared
   behaviour — a successful degradation, or a diagnostic naming that gate. A generic
   "spawn failed" is a test failure.
