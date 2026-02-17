# Component 002 Pseudocode — Runtime Orchestration (tmux/PTY)

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

Requirements: REQ-FUNC-005,006,007,010; REQ-TECH-004,009

```text
01: FUNCTION ensure_session(agent, runtime_registry)
02:   IF runtime exists for agent.id -> return existing binding
03:   build launch_signature from profile/mode/pass_continue/work_dir
04:   create or recover tmux session name from stable agent identity
05:   persist binding in runtime_registry
06:   return binding

07: FUNCTION attach_viewer(agent_id)
08:   binding = runtime_registry.lookup(agent_id)
09:   IF missing -> return RuntimeMissing error
10:   teardown existing attached viewer safely
11:   wait bounded for prior reader termination
12:   spawn attach client for binding.session_name
13:   mark attached viewer metadata
14:   return success

15: FUNCTION forward_terminal_input(event)
16:   IF terminal not focused -> ignore
17:   IF no attached viewer -> return NoAttachedViewer error
18:   encode key/mouse event to PTY bytes
19:   write bytes to PTY writer
20:   on write failure emit RuntimeWriteFailure

21: FUNCTION kill_runtime(agent_id)
22:   binding = lookup(agent_id)
23:   IF missing -> return NoOp
24:   terminate tmux session/client according to policy
25:   verify liveness false (bounded probe)
26:   emit status Dead (or Errored with reason)

27: FUNCTION relaunch_runtime(agent_id)
28:   require current status Dead
29:   require launch_signature exists
30:   spawn runtime with preserved launch_signature
31:   on success emit Waiting/Running transition
32:   on failure keep Dead and emit surfaced error

33: FUNCTION liveness_refresh_tick()
34:   for each runtime binding probe liveness
35:   emit status transition events only on changes
```
