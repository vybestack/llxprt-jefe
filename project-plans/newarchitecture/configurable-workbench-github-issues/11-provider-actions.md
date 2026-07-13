# CW-10: One-shot and persistent action-provider lifecycle

## Outcome and consumed contracts

Deliver a strict JSONL provider protocol, request reducer, and process supervisor for statically declared actions. Consume immutable action declarations/availability, selected static package manifests, closed post-commit provider effects, generation identities, Settings-selected config references, and harness process capture/cleanup. Package inventory must already be delivered with approved dependencies; this body does not choose those dependencies. No provider can draw UI, emit arbitrary effects, own navigation, access host state, or place process handles in application state.

## Source and symbol inventory

| Source/symbol | Required responsibility |
|---|---|
| `src/app_shell.rs` | execute `ProviderEffect` only after state commit/release |
| `src/messages.rs` | typed provider effects/completions; no generic JSON event |
| `src/runtime/mod.rs` | expose supervisor adapter without application state |
| new `src/runtime/provider/supervisor.rs` | sole process-group/handle/lifecycle/environment owner |
| new `src/runtime/provider/framing.rs` | bounded UTF-8 JSONL read/write and duplicate-key rejection |
| new `src/runtime/provider/protocol.rs` | closed envelope/payload DTO parser and state machine |
| new `src/state/provider_requests.rs` | health/request/progress/outcome reducer with no handles |
| action registry | declaration/context/availability authority |
| package inventory | executable/manifest/trust/config declaration authority |

## Wire protocol

Each UTF-8 line contains exactly one JSON object and `\n`; CRLF, BOM, blank lines, duplicate keys, unknown keys, trailing non-whitespace, non-UTF-8, and lines above 1,048,576 bytes are fatal `PLG-E502`. Envelope is exactly `{protocol:1,type:string,request_id:string,generation:u64,payload:object}`. Request IDs match `h-[0-9]{6,20}` host or `p-[0-9]{6,20}` provider; generation is fixed and positive for one process. Unknown type, wrong direction/ID/generation/order, or data after terminal is fatal.

Closed payloads:

```text
hello H->P {host_api:string,plugin_id:Id,plugin_version:SemVer}
hello-ack P->H {provider_name:string,protocol:1}
configure H->P {config_version:u64,config:TypedMap,secrets:{name:EnvName},environment:{name:string}}
ready P->H {capabilities:["actions"|"panels"|"config-migration"]}
invoke-action H->P {invocation_id:Id,action_id:ActionId,arguments:TypedMap,
 context:{screen_id,screen_instance,resource_refs:TypedMap},continuation?:Continuation}
cancel H->P {target_request_id:string}
progress P->H {sequence:u16,message:string,completed?:u64,total?:u64}
outcome P->H Outcome
error P->H {code:string,message:string,retryable:bool,field_errors:[{path,message};0..128]}
shutdown H->P {reason:"completed"|"cancelled"|"host-exit"|"failure"}
shutdown-ack P->H {}
Continuation={confirmation_id:Id,approved:true,values:TypedMap}
Outcome={kind:"navigate",route_id:RouteId,activation:TypedMap}|
 {kind:"refresh",resource_ref:TypedMap}|{kind:"notice",severity:"info"|"warning",message:string}|
 {kind:"replace-panel",panel_instance_id:Id,snapshot:PanelSnapshot}|
 {kind:"request-host-confirmation",confirmation_id:Id,title:string,body:string,
 confirm_label:string,destructive:bool,continuation_schema:[Field]}|
 {kind:"close-panel",panel_instance_id:Id}|{kind:"migrated-config",migration:MigratedConfig}
```

An outcome kind must be declared by the action and valid for current owner/context. `navigate` uses a declared route; refresh only a current known resource; replace/close only an owned live panel; migrated-config only a migration request. Command, URL, clipboard, persistence, PTY, shell, private host message, raw UI, and unknown outcomes are impossible.

Handshake is host hello, provider hello-ack, host configure, provider ready. Each stage has 5 seconds. Ready capabilities must be a subset of manifest declarations. One-shot providers start zero processes at startup; invocation performs spawn, handshake, invoke, zero or more progress, exactly one outcome/error, shutdown/ack, EOF, reap. Persistent providers start in plugin-ID order during candidate startup; all required providers must reach Ready before atomic publication, otherwise every candidate process is stopped/reaped and nothing publishes. No auto-restart.

Progress sequence starts at 1 and increases exactly by 1, max 256; when total exists, completed exists and `completed<=total`; values never decrease. First terminal event wins cancel/terminal race; later bytes are protocol failure but cannot change result. Default timeout 60 s, manifest 1–600 s. Concurrent requests 16, queued outbound envelopes 64, stderr 262,144 bytes. Retry is explicit and creates a new generation.

Host confirmation is two invocations. Terminal A returns request-host-confirmation. Host validates declared destructive policy and stores a single-use owner/action/context/generation-bound ID for 5 minutes. Cancel starts no continuation. Confirm starts fresh invocation B (and full one-shot handshake) with exact continuation. Provider cannot forge/reuse/extend confirmation.

Environment starts empty and includes provider directory plus fixed platform system-bin PATH, contained HOME/TMPDIR, locale, and manifest-declared nonsecret names. Secrets resolve only declared references into Configure and never environment unless declaration explicitly names that secret environment binding; they never appear in state/logs/stderr/report/diagnostic. Drain stdout/stderr continuously. Shutdown closes new requests, sends shutdown and waits 2 s, closes stdin/terminates group and waits 2 s, kills/reaps descendants and drains 2 s.

## End-to-end, migration, failures, security

Static composition publishes action metadata; dispatch creates request/generation; state commits; supervisor executes; framing validates each line; reducer validates ownership/order/context and updates bounded progress or terminal outcome; host adapter executes only closed validated outcome. Crash/EOF/protocol/timeout marks this generation unavailable and preserves package selection/config. Offline recovery starts zero providers. Provider state/models never persist; no migration exists for runtime requests.

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Action progress ----------+ + Actions ------------------+
| Checks 2/4                | |>>Git Merger              |
| Cancel                    | | Enter invoke             |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Actions ------------------+ + Provider failed ---------+
| Git Merger unavailable    | | PLG-E502 bad sequence    |
| reason: provider stopped  | | [Retry] generation 4     |
+---------------------------+ +---------------------------+
```
```text
DIRTY/CONFIRMATION             RECOVERY
+ Confirm action? ----------+ + Provider recovery -------+
| destructive              | | config remains durable   |
|>>Confirm  Cancel         | |>>Retry  Disable  Back     |
+---------------------------+ +---------------------------+
```
```text
SMALL
+Confirm?-------+
| destructive  |
|>>Confirm      |
| Cancel        |
| Ctrl-Q Exit   |
+---------------+
```

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW10-01 | WHEN one-shot actions compose, Jefe shall start zero provider processes. | startup capture with executable trap |
| CW10-02 | WHEN one-shot action runs, Jefe shall execute the complete fresh lifecycle. | exact transcript/process capture |
| CW10-03 | WHEN persistent startup succeeds, Jefe shall complete ordered handshakes before publication. | two-provider startup transcript |
| CW10-04 | IF any required persistent provider fails, Jefe shall reap all candidates and publish nothing. | each handshake phase failure |
| CW10-05 | WHEN every payload parses in valid order, Jefe shall accept its exact closed fields. | per-payload canonical table |
| CW10-06 | IF framing, field, direction, generation, order, rate, or bound is invalid, Jefe shall fail that generation with `PLG-E502`. | exhaustive negative table |
| CW10-07 | WHEN progress arrives, Jefe shall enforce sequence/count/total monotonicity. | progress property |
| CW10-08 | WHEN confirmation is accepted/cancelled, Jefe shall create one fresh bound continuation or no invocation. | two-invocation/cancel captures |
| CW10-09 | WHEN cancel races terminal, Jefe shall retain the first terminal result. | both orderings |
| CW10-10 | WHEN Retry occurs, Jefe shall reject all old-generation output. | stale-line property |
| CW10-11 | WHEN shutdown bounds expire, Jefe shall kill and reap every descendant. | child/grandchild hang fixture |
| CW10-12 | WHEN recovery CLI runs, Jefe shall start zero providers. | malformed config plus hanging provider |
| CW10-13 | WHEN each UI state renders, Jefe shall use the shared availability reason and accessible focus. | distinct state scenarios |
| CW10-14 | WHEN secrets/ambient names are scanned, Jefe shall expose neither outside owning Configure. | every observation surface scan |

RED transcripts/captures first; GREEN state machines/supervisor; REFACTOR framing only without merging one-shot/persistent semantics.

## Documentation and done

Update `dev-docs/standards/persistence-and-runtime.md` with full wire tables, lifecycle, environment, cleanup, bounds, confirmation, and failure ownership; update `dev-docs/standards/architecture.md` with supervisor/state separation. Done requires all payload/order/limit/process/UI tests and unchanged `make ci-check`; no new dependency beyond the separately approved package inventory, no shell, secret leak, orphan, suppression, or gate weakening.