# CW-11: Persistent host-rendered panels and plugin configuration migration

## Outcome and consumed contracts

Deliver persistent-provider panel snapshots/events, host rendering, generated plugin Settings fields, and explicit pre-Configure config migration. Consume custom descriptors/ports/relationships, typed navigation instances/generations, Settings drafts/writer, selected static package manifests, and the exact provider envelope/handshake/supervisor. One-shot providers cannot declare panels. Providers never send iocraft objects, raw keys/mouse, arbitrary effects, or durable models.

## Source/symbol inventory

| Source/symbol | Required responsibility |
|---|---|
| provider protocol parser | add closed panel activate/deactivate/event/snapshot and migration payloads |
| `src/state/provider_requests.rs` | retain provider health only; no panel model ownership |
| new `src/state/provider_panels.rs` | sole panel lifecycle/model/revision/host-local reducer |
| new `src/domain/plugin_config.rs` | sole config schema/value/visibility validator |
| `src/ui/components/` host primitives | render list/detail/form/status/progress/empty/error DTOs |
| descriptor registry | bind declared panel type to plugin screen only |
| Settings reducer/writer | own plugin draft, secret references, migration approval/save |
| supervisor | own persistent process and delivery only |

## Closed panel wire DTOs

Envelope remains `{protocol:1,type,request_id,generation,payload}` with duplicate/unknown rejection. Add:

```text
activate-panel H->P {panel_instance_id,screen_instance_id,panel_type,activation,prior_host_local?}
deactivate-panel H->P {panel_instance_id,reason:"suspend"|"dispose"|"replace"}
panel-event H->P {panel_instance_id,revision,event:PanelEvent}
panel-snapshot P->H PanelSnapshot
PanelSnapshot={model_schema:1,panel_instance_id,generation,revision:u64,kind:BodyKind,
 title,description?:string,loading:bool,action_affordances:[Affordance;0..64],body:Body}
Affordance={id,label,action_id,arguments?:TypedMap,enabled:bool,unavailable_reason?:string}
Body={kind:"list",items:[ListItem;0..1000],selected_id?:Id,next_page_token?:string}|
 {kind:"detail",document:string,metadata:[Metadata;0..256],actions:[Id]}|
 {kind:"form",fields:[Field;0..128],values:TypedMap,field_errors:[FieldError],submit_action:ActionId}|
 {kind:"status",rows:[StatusRow;0..256]}|
 {kind:"progress",message,completed?:u64,total?:u64,cancellable:bool}|
 {kind:"empty",message,action?:Id}|{kind:"error",code,message,retryable:bool,retry_action?:Id}
ListItem={id,label,description?:string,status?:string,actions:[Id;0..64]}
Metadata={label,value}; StatusRow={label,value,state:"normal"|"warning"|"error"}
PanelEvent={kind:"selected",id}|{kind:"activated",id}|{kind:"action",id,arguments:TypedMap}|
 {kind:"field-changed",field_id,value}|{kind:"submit",values:TypedMap}|
 {kind:"page-requested",token}|{kind:"retry"}|{kind:"cancel"}|{kind:"link-selected",link_id}
```

Snapshot revision begins 1 and increases exactly by 1 for each panel instance/generation. IDs are unique/stable; selected ID exists; total implies completed and completed<=total; actions/affordances are declared and available; disabled affordance has reason. Manifest declares allowed event kinds and argument schemas; host emits only those. Full snapshots atomically replace accepted model. Snapshot max 524,288 bytes, document 262,144, host-local 65,536, model rate sustained 20/s burst 40, nesting/map/array 16/256/1,024. Invalid model moves lifecycle to Failed and may display the last accepted model with literal `stale`; it never partially applies.

```text
PanelLifecycle="declared"|"activating"|"active"|"suspended"|"failed"|"disposing"|"disposed"
HostLocal={focus_target?:Id,scroll_offset:u32,selected_id?:Id,form_draft?:TypedMap}
```

Activate/Retry increments generation. Suspend sends deactivate, unsubscribes, and retains bounded HostLocal only. Resume activates fresh generation. Dispose sends deactivate when possible, invalidates instance, and rejects late snapshots. Models and lifecycle never persist. Host exclusively owns focus, key/mouse translation, wrap, scroll, selection repair, confirmation, links, theme, and accessibility.

## Closed plugin config and migration

Manifest config declaration is exactly `{schema_version:u64,fields:[Field;0..128]}`. Field has `{id,label,description?,type,required,default?,min?,max?,choices?,unique?,visible_when?,restart}`. Types are boolean, string, integer, finite-number, enum, path, string-list, secret-reference. `restart` is `none|provider|host`. Constraints valid only for their type; choices are unique; visibility references siblings and forms an acyclic graph. Secret reference serializes exactly `{env:EnvName}`; UI displays set/unset and persists only reference. Resolved secret enters only owning Configure and is absent from state/model/log/diagnostic/effective/export.

Migration occurs before Configure:

```text
migrate-config H->P {from_version,to_version,config,draft_token}
migrated-config P->H {from_version,to_version,config,draft_token,target_config,notes:[string;0..64]}
```

Host validates exact identity, target schema, bounds, and absence of resolved secrets; computes a redacted path-sorted lossless diff. Approve writes exact target version/config through Settings hash writer then configures provider. Cancel/failure keeps old selection/config and starts no provider. Absent/disabled owner config is dormant and not owner-validated. Rollback is selecting a still-installed prior package version offline, never automatic mutation.

## Complete flow, recovery, security

Screen activation commits panel instance then sends activate effect; provider snapshot passes framing/owner/generation/revision/schema/bounds; panel reducer atomically replaces model; pure host projection renders; semantic input becomes only a declared event. Config open projects host controls; edit validates candidate; migration runs provider before Configure; approval saves; startup continues. Invalid event/outcome executes zero host effect. Durable data is package selection/config/settings; ephemeral models are discarded on failure. Recovery offers Retry, Disable, offline Rollback, and provider-free config commands.

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Plugin panel -------------+ + Plugin panel -------------+
|  item A                   | |>>item B                   |
|  item B                   | | Enter activate            |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Plugin panel -------------+ + Plugin panel stale -------+
| provider unavailable      | | PLG-E502 revision 3/4     |
| reason: handshake failed  | | last model [Retry]        |
+---------------------------+ +---------------------------+
```
```text
DIRTY/MIGRATION                RECOVERY
+ Approve migration --------+ + Provider recovery -------+
| old: redacted             | | config version 2 retained|
| new: redacted             | |>>Retry Disable Rollback  |
|>>Approve Cancel           | +---------------------------+
+---------------------------+
```
```text
SMALL
+Plugin panel---+
|>>item A       |
| stale         |
| r Retry q Back|
+---------------+
```

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW11-01 | WHEN a plugin screen activates, Jefe shall create only manifest-declared panel instances. | declaration/binding scenario |
| CW11-02 | WHEN a valid next snapshot arrives, Jefe shall atomically replace the full model. | every body kind transcript |
| CW11-03 | IF ownership/generation/revision/schema/rate/size is invalid, Jefe shall fail without partial model. | exhaustive negative/at-limit matrix |
| CW11-04 | WHEN host input occurs, Jefe shall emit only a declared semantic event DTO. | every event kind and undeclared rejection |
| CW11-05 | WHEN panel suspends/resumes/disposes, Jefe shall enforce lifecycle and bounded host-local retention. | lifecycle state table |
| CW11-06 | WHEN config schema renders, Jefe shall generate exact host controls for every field type. | all-field projection golden |
| CW11-07 | IF config/visibility is invalid, Jefe shall show adjacent errors and block Save/Configure. | constraints/cycle matrix |
| CW11-08 | WHEN a secret resolves, Jefe shall expose it only to owning Configure. | all-observation scan |
| CW11-09 | WHILE owner is absent/disabled, Jefe shall preserve config without owner validation. | dormant round-trip |
| CW11-10 | WHEN valid migration is approved, Jefe shall hash-save exact target before Configure. | approve transcript/write capture |
| CW11-11 | WHEN migration cancels/fails/has wrong identity, Jefe shall retain exact prior config and start no provider. | cancellation/negative matrix |
| CW11-12 | WHEN each panel/config state renders, Jefe shall preserve host focus/accessibility/protected exit. | distinct UI scenarios |

RED all DTO/lifecycle/config/migration fixtures; GREEN strict reducers/presenters; REFACTOR only constrained field grammar reuse.

## Documentation and done

Update `dev-docs/standards/display-and-ui.md` with complete DTO rendering/input/state rules and `dev-docs/standards/persistence-and-runtime.md` with lifecycle/config/migration/secrets/recovery. Done requires all DTO fields/limits, state transitions, secret scans, UI states, and unchanged `make ci-check`; no provider rendering/persistence, dependency or gate weakening.