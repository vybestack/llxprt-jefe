# Configurable Workbench Public Contracts

## 1. Authority, compatibility, identity, and ownership

This is the normative configurable-workbench v1 contract. `MUST`, `MUST NOT`, `SHOULD`, and `MAY` have RFC 2119 meanings. The specification and roadmap incorporate these contracts; contradiction blocks release. Public schemas reject unknown fields in active artifacts unless a table below says otherwise. Versions are exact integers; breaking changes increment the owning major.

IDs are lowercase ASCII, 1–128 bytes, matching `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`. `core.*` is host-owned, `github.*` bundled-GitHub-owned, `local.*` user-owned, and `<plugin-id>.*` plugin-owned. Plugin IDs have at least two labels and cannot begin with a reserved namespace. Local member IDs match `[a-z][a-z0-9-]{0,62}`. Duplicate IDs are fatal even when definitions are byte-identical. An owner may split implementation across compliant files; file location never transfers responsibility.

Authoritative responsibility map:

| Contract/data | Sole authority | Consumers that MUST NOT redefine it |
|---|---|---|
| paths, lossless document, provenance, atomic writes | config/persistence | UI, providers, domain |
| settings/state migration | versioned migration modules | startup/UI/runtime |
| agent definition/probe/plan | agent registry and planner | forms, runtime, product adapters |
| screen/panel descriptor | workbench descriptor registry | UI controllers, plugins |
| resolved geometry | layout resolver | renderer, mouse, selection, PTY |
| actions/availability/bindings | action registry/resolver | Help, hints, Settings |
| routes/dirty/navigation | navigation reducer | panels/providers |
| package selection/static contributions | package inventory/composer | supervisor/UI |
| process lifecycle | runtime supervisor | reducers/domain/UI |
| durable entities and transitions | application reducers | runtime/UI/providers |
| diagnostics | failure's owning boundary; config aggregates | views and unrelated adapters |

No event bus, general queue, workflow language, arbitrary plugin UI, shell template, hot reload, or product-specific branch in generic code is part of v1.

## 2. Closed limits and diagnostics

All limits are inclusive. Nested totals count decoded UTF-8 bytes and all descendants, not only top-level entries. Static active violations reject the candidate; dormant violations are retained losslessly and diagnosed when selected; runtime violations fail only the owning request/provider generation.

| Resource | Limit |
|---|---:|
| artifact/schema/manifest/settings file | 1,048,576 bytes each |
| nesting depth: TOML/JSON/config values | 16 |
| map keys / array elements in any value | 256 / 1024 |
| string / path / description | 262,144 / 4096 / 4096 bytes |
| ID / label | 128 bytes / 128 cells |
| diagnostics retained; origins per diagnostic | 256; 16 |
| enabled/discovered plugins | 32 / 256 |
| package entries/expanded bytes/path depth/path length | 4096 / 67,108,864 / 16 / 1024 bytes |
| enabled screens/panels per screen/layout depth/split children | 64 / 16 / 8 / 8 |
| relationships/follow-ups per transition | 64 / 64 |
| action declarations per plugin/effective bindings/chords per action-context | 128 / 2048 / 8 |
| routes/panels/screens per plugin | 32 / 32 / 32 |
| navigation depth | 32 |
| fields per agent scope/config/form | 64 / 128 / 128 |
| emitters/candidates/probe argv/capabilities | 128 / 8 / 8 / 32 |
| provider line/stderr/snapshot bytes | 1,048,576 / 262,144 / 524,288 |
| concurrent requests/outbound envelopes/progress per request | 16 / 64 / 256 |
| list items/metadata rows/affordances | 1000 / 256 / 64 |
| document bytes/model update rate | 262,144 / 20 s⁻¹ burst 40 |
| probe stdout/stderr/time | 65,536 each / 5 s local, 20 s remote |
| Hello/Configure/Ready | 5 s each |
| action timeout | 60 s default, declared 1–600 s |
| shutdown/terminate/final flush | 2 s / 2 s / 2 s |

Every diagnostic is `{code,severity,path,span?,owner?,owner_version?,provenance[],correction,redacted_detail}`. The boundary detecting a failure assigns code/span/owner and redacts it; config only sorts/retains. Ordering is severity(error, warning, info), canonical path, byte span, code. Required codes: `CFG-E001` syntax, `CFG-E002` schema, `CFG-E003` active invalid, `CFG-W004` inactive invalid, `CFG-E005` duplicate/ownership, `CFG-E006` bound, `CFG-E007` external edit, `CFG-E008` protected recovery, `CFG-E102` physical ambiguity, `CFG-E103` malformed state, `CFG-E104` write; `AGT-E201` incompatible, `AGT-E202` probe, `AGT-E203` stale; `SCR-E301` composition; `KEY-E401` conflict; `PLG-E501` package ambiguity, `PLG-E502` protocol, `PLG-E503` unavailable. Secrets are absent from paths, spans, details, provenance, logs, captures, state, and effective output.

## 3. Paths, Settings schema 2, State schema 2, recovery CLI

### 3.1 Paths and package roots

Mutable path precedence is: (1) `--config DIR` ignores path environment and uses `DIR/{settings.toml,state.json,definitions,plugins,themes}`; (2) file variables `JEFE_SETTINGS_PATH`/`JEFE_STATE_PATH`; (3) root variables `JEFE_CONFIG_DIR`/`JEFE_STATE_DIR`; (4) platform defaults. macOS config/state is `~/Library/Application Support/jefe`; Linux config is `${XDG_CONFIG_HOME:-$HOME/.config}/jefe`, state `${XDG_STATE_HOME:-$HOME/.local/state}/jefe`.

Plugin roots are scanned in this exact low-to-high precedence order, and listings retain origin order:

| Platform | Ordered roots |
|---|---|
| all | canonical executable `../share/jefe/plugins` |
| macOS | `/opt/homebrew/share/jefe/plugins`, `/usr/local/share/jefe/plugins` |
| Linux | `/usr/local/share/jefe/plugins`, `/usr/share/jefe/plugins` |
| all | `<config>/plugins/installed` |

Nonexistent roots are skipped. Roots are not discovered through PATH/cwd. Canonicalize every existing root/package, then compare `(device,inode)` where available; the first occurrence of one physical directory wins and later symlink/Cellar aliases are recorded as aliases. Two distinct physical directories with one `(id,version)` are ambiguous, never precedence-overridden. A missing final target compares canonical parent plus basename. A final symlink escaping its selected root is rejected.

### 3.2 Settings schema 2

Top-level TOML is exactly:

| Key | Type/default | Merge/unknown rule |
|---|---|---|
| `settings_schema` | integer `2`, required | exact |
| `appearance` | table | recursive map merge |
| `workbench` | table | recursive except lists below |
| `agents` | table keyed type ID | recursive; unknown type dormant |
| `keymap` | table keyed context | `(context,action)` list replacement; `[]` unbind |
| `plugins` | table keyed plugin ID | recursive; absent owner dormant |
| `extensions` | table | reserved lossless dormant content |

`appearance={theme?:string,override_agent_theme?:bool}`. `workbench={initial_screen?:ID,screen_order?:[ID],enabled_screens?:[ID],layout_overrides?:map}`; order lists replace and contain unique enabled IDs exactly once, and a layout override replaces the whole tree. `agents.<id>={enabled?:bool,repository_defaults?:typed-map}`. `plugins.<id>={enabled?:bool,version?:canonical-semver,config?:typed-map}`. Missing inherits compiled defaults then selected plugin defaults (plugin ID/version order), user definitions (canonical-path order), then settings. Scalar/list replacement is total. Reset removes the syntax node. Unknown top-level keys outside `extensions`, unknown keys owned by an active known table, and wrong types are fatal. Unknown owner IDs and all descendants are dormant: preserve comments/order/bytes semantically, do not owner-schema-validate, do not publish. Sparse files may omit every optional key.

The lossless document retains comments, ordering, quote style, unknown/dormant nodes and loaded SHA-256. Save validates the complete candidate, rereads and compares hash, patches only edited syntax paths, writes a unique same-directory mode-0600 temporary, fsyncs, atomically renames, fsyncs parent where supported, and updates authority only after success. Hash conflict retains disk plus in-memory/exportable draft. Reads never rewrite.

Schema-1 migration accepts required `schema_version=1`, `theme:string`, optional/default-false `override_agent_theme`, and unknown syntax. Effective mapping is `appearance.theme` and `appearance.override_agent_theme`; unknown syntax moves nowhere and remains at its original syntax location as dormant legacy content. Migration occurs only on an explicit successful schema-2 save/`config format --migrate`; read/validate/show do not write. The migration is sequential, idempotent, preserves comments/order, and removes the three schema-1 keys only in the committed patch.

### 3.3 State schema 2 and complete schema-1 migration

State JSON schema 2 is closed:

```text
{state_schema:2, revision:u64,
 repositories:[RepositoryRecord], agents:[AgentRecord],
 selection:{repository_id?:ID,agent_id?:ID,screen_id?:ID},
 last_selected_agent_by_repo:{RepositoryId:AgentId},
 preferences:{hide_idle_repositories:bool,pane_focus:string,terminal_focused:bool},
 dormant_records:[DormantRecord]}
RepositoryRecord={id,location:LocalPath|RemoteTarget,display_name,agent_defaults:{type_id,values}}
AgentRecord={id,repository_id,type_id,values,launch_signature:{version:1,definition_hash,typed_value_hash,target_fingerprint},runtime:{session_id?,invocation_generation:u64,last_known:stopped|running|unknown}}
DormantRecord={kind,stable_id?,raw_schema,reason,raw_value}
```

IDs and references are unique/valid; indices never persist. Runtime signatures include type ID, definition hash, only fields marked `launch_signature`, target identity/generation fingerprint, and signature version; secrets and non-signature UI values are excluded. Unknown type/field records are retained in `dormant_records`, shown unavailable, never substituted/deleted, and rehydrate only after exact owner/schema validation. Runtime `running` is observational and MUST be reconciled with tmux/process liveness; migration never invents a live process.

Schema-1 accepts `schema_version`, `repositories[]`, `agents[]`, nullable selected indices, default-false `hide_idle_repositories`, default-empty index map, `pane_focus`, default-false `terminal_focused`, and default `user_preferences`. Migration: assign stable deterministic IDs from canonical repository identity plus collision ordinal and agent legacy identity; map valid indices to IDs; invalid indices become absent plus warning; missing/`llxprt` -> `core.llxprt`; `code_puppy|code-puppy|codepuppy` -> `core.code-puppy`; product defaults/fields become namespaced typed values; raw LLxprt `mode_flags` and remote setup remain exact legacy-adapter payload; unknown kinds become dormant records. Live session IDs are retained but `last_known=unknown` until reconciliation. Result revision is source revision/default 0 plus one. Golden migration is idempotent and source bytes remain retained during path import.

Current legacy state is `dirs::data_local_dir()/jefe/state.json` (macOS usually aliases target; Linux normally `${XDG_DATA_HOME:-$HOME/.local/share}/jefe/state.json`). If target absent: no source means empty in memory; exactly one valid distinct source is atomically copied and retained; malformed source exits 2; multiple sources exit 3. If target exists: physical aliases dedup; any distinct legacy file exits 3 even if bytes equal; malformed target exits 2. No merge/default overwrite occurs.

Normal TUI behavior is exact: malformed/unsupported selected state or ambiguous paths prevent TUI and print `config validate`/`migrate-state` recovery instructions; valid state with dormant records opens normal TUI and shows unavailable records. Recovery commands parse only what they need, never start TUI/providers, and never delete malformed bytes.

### 3.4 Exhaustive CLI syntax and results

| Syntax | Providers/probes | Writes | Success/result |
|---|---|---|---|
| `jefe config path [--config DIR]` | none | none | selected/legacy/canonical/physical/provenance; 0 |
| `config validate [--config DIR]` | none | none | static all-artifact diagnostics, notes semantic validation skipped; 0 or 2/3 |
| `config show-effective [--config DIR] [--provenance]` | none | none | redacted effective candidate; 0 or 2/3 |
| `config edit [--config DIR]` | none | editor only | exact argv editor launch; 0 or 4 |
| `config format [--check] [--migrate]` | none | only without `--check` | lossless canonical owned nodes; 0/2/4 |
| `config migrate-state [--config DIR]` | none | target only | decision table import/migrate; 0/2/3/4 |
| `agent-type list|explain ID [--target ID]` | probes only when `--probe` supplied | none | status/provenance; 0, 2, or 5 |
| `explain binding CHORD [--context ID]` | none | none | resolution/provenance; 0/2/64 |
| `explain action|screen ID` | none | none | declaration/availability/layout; 0/2 |
| `plugin list|inspect ID [--version V]` | none | none | exact versions/state; 0/2/3 |
| `plugin install ARCHIVE [--enable]` | none | package/settings transaction | disabled by default; 0/2/3/4 |
| `plugin install DIR --developer [--enable]` | none | package/settings | directory otherwise usage 64 |
| `plugin enable|disable ID [--version V]` | none | settings | static validated selection; 0/2/3/4 |
| `plugin rollback ID --version V` | none | settings | selected installed exact version; 0/2/3/4 |
| `plugin remove ID --version V` | none | package | enabled rejected; 0/2/4 |

Unknown option/missing operand is 64. Exit 0 success/warnings; 2 parse/schema/composition; 3 ambiguity; 4 filesystem/editor transaction; 5 explicitly requested online probe/provider validation failed. Offline commands execute zero providers; only `agent-type ... --probe` executes bounded probes.

## 4. Agent definition, probe, operation, target, preflight, environment

Four binding shipped peers are `core.llxprt`, `core.code-puppy`, `core.codex`, and `core.claude-code`. Each needs a pinned release identity, SHA-256, exact probe stdout/stderr, executable fixture, capability set, and successful local/remote transcript. Unsupported unverified mappings are omitted, never guessed; Claude Code remains present with its verified subset.

Definition schema 1 requires `{agent_type_schema=1,id,display_name,executable_candidates,probe,operations,targets,repository_fields,agent_fields,launch}`. Unknown fields are fatal for active definitions. Fields use boolean/optional-boolean/string/integer/enum/path/string-list with exact type default, min/max, choices, sibling acyclic `visible_when`, and `launch_signature`.

Probe is exact:

```text
probe={argv:[fixed tokens 1..8], stream:stdout|stderr|combined,
 framing:single-json|json-lines|utf8-text, identity:{source:pointer|line,match:anchored-regex},
 capabilities:{source:pointer|json-array|prefixed-lines,pointer_or_prefix},
 required:[ID<=32], timeout_ms:1..5000, max_bytes:1..65536}
```

The host resolves candidates from one startup PATH snapshot left-to-right; a candidate containing `/` is forbidden except the shipped LLxprt resolver may first test repository-local `<repository>/.llxprt/bin/llxprt`, then PATH candidates. Canonical absolute executable is opened/stat-fingerprinted before probe. Capture raw stdout/stderr concurrently to separate byte bounds; `combined` parses deterministic tagged chunks in read sequence, not OS timing concatenation. Nonzero/signal, timeout, invalid UTF-8, truncation, framing error, duplicate JSON key, trailing bytes for single JSON, line over max, identity mismatch, or capability shape error is `ProbeError`. A valid identity lacking required capabilities is `InstalledIncompatible`. JSON pointers use RFC 6901; capability arrays contain unique public IDs; prefixed lines are exact `capability:<ID>`, trim only CRLF, reject duplicates/invalid IDs, then sort. Probe success stores canonical path, file fingerprint, identity, sorted capabilities, definition hash, and monotonically increasing probe generation.

Operations are a closed map with keys `normal`, `resume`, `fresh-issue`, `fresh-pr`; each is `supported=true` plus fixture-authorized emitter/prompt policy, or `{supported=false,reason}`. Targets are `local` and `remote`, each similarly supported/reason. Unsupported combinations remain visible: form submit/Send is disabled, inline text is `Unsupported by <agent>: <reason>`, Help/explain shows the same reason, and no prep/probe/spawn occurs. This avoids product branches.

Launch emitters are only fixed, flag, option, boolean-option, repeated-option, positional, and env; no templates/splitting/shell/raw args. `AgentLaunchPlan={type_id,operation,definition_hash,absolute_executable,argv:OsString[],env:(OsString,OsString)[],cwd,target,probe_generation,target_generation,preflight,signature}`. Local execution preserves elements. Remote uses one audited serializer and rejects NUL/unrepresentable values.

Target resolver: Local canonicalizes repository cwd and increments generation when identity changes. Remote is `{remote_target_id,host,user?,port?,canonical_workdir}`; SSH/auth/effective-user failure is ProbeError, command-not-found is NotFound, identity/capability mismatch is Incompatible. Remote probe is host-generated, read-only, run as effective target user, and requires no local executable. Repository-local LLxprt resolution on remote tests `<canonical_workdir>/.llxprt/bin/llxprt` before remote PATH. No definition may supply setup commands. Existing LLxprt remote setup remains only in the named legacy adapter.

Preflight is closed: `none` or `sandbox {engine, image, required_env:[names], image_probe_argv}`. It runs after all generation checks but before repository mutation/spawn. The sandbox contract resolves engine by canonical executable/fingerprint, verifies image using fixed argv without pull/build, verifies required environment names are present, and returns typed `Ready{engine,image_digest,env_names}|Unavailable{reason}`. Provider/agent environment begins empty and receives only `TERM`, locale (`LANG`,`LC_*`), repository-declared safe entries, definition fixed entries, typed field env emitters, and operation-specific verified entries; tmux variables and undeclared ambient variables are removed. LLxprt sandbox preflight runs only when its typed sandbox setting is enabled; Code Puppy never receives stale LLxprt settings.

Generation checks precede clone/reset/prompt writes/preflight/spawn. Normal/resume/fresh operations all use the same resolver/planner. Fresh LLxprt forces no continue, removes only `--continue`, preserves other flags, appends `-i` and exact fixture prompt. Fresh Code Puppy disables quick resume and uses exactly one prompt positional. Codex/Claude use only pinned operation mappings. Runtime signatures are section 3.3. Unknown values remain dormant.

## 5. Sole descriptor/layout contract, screens, routes, and navigation

CW-04 introduces the sole internal `ScreenDescriptor`/`PanelDescriptor`/`LayoutNode` contract. Shipped screens compile directly to it. CW-05 introduces external `screen_schema=1` and lowers it once into that same contract; it MUST NOT add a parallel runtime descriptor or geometry model.

External screen fields are exact: `screen_schema,id,title,route,activation[0..32],initial_focus,focus_order,panels[1..16],layout,relationships[0..64],bindings[0..256]`. Panel is `{type,config}`; every panel occurs once in layout and every focusable panel once in focus order. Activation uses agent field types without secrets. Unknown/duplicate/owner-invalid references fail.

Layout is `leaf{panel}` or `split{axis:horizontal|vertical,children[2..8]}`; child is `{node,size:fixed-positive|weight-positive,min>=1,max?,collapsible=false,collapse_priority?}`. Allocation pseudocode is normative:

```text
resolve(node, rect):
  remove no chrome here (caller supplies content rect)
  if leaf: emit chrome=rect; content=inset(rect); return
  children := declaration order; available := axis length - internal borders
  while sum(required minima) > available and collapsible remains:
    hide minimum by (collapse_priority ascending, depth-first index ascending)
  if required minima still exceed available:
    show only first required panel in focus_order in rect plus TooSmall; return
  allocate fixed := clamp(fixed,min,max)
  weighted_pool := available - sum(fixed)
  give each weight floor(pool*w/sum_w); give remainder one cell in declaration order
  repeatedly clamp below min/above max and redistribute only among unclamped weighted children
  derive nonoverlapping child rects in declaration order; recurse
  repair focus to first visible panel at/after old focus in cyclic focus_order
```

Global chrome is removed once before this algorithm. Borders/titles are inside rectangles. Hidden panels receive no hit/selection/viewport region and PTYs are never resized to zero. `ResolvedLayout` is sole input to render, mouse, selection, focus, scrolling, and PTY sizing.

Ports are `{id,direction,type_id,required,retained}` with exact version equality. Relationships are Scope, MasterDetail, SessionTarget; same screen only, acyclic, one incoming target, one outgoing per `(source,kind)`, no same-kind fan-out, max 64. Deterministic declaration-order reducer follow-up emits None on deletion, applies declared show-none/show-all/retain or detach/retain, never moves focus.

Manifest routes declare `{id,activation_schema,target_screen}`. Push/Replace/Back validate before mutation; stack max 32, no reuse/persistence. Local unwind precedence is: host confirmation modal; dirty Save/Discard/Cancel; focused chooser; focused editor; search; filter; non-dirty overlay; panel-local transient; then navigation Back. Only the first applicable layer handles the key. Push suspends subscriptions and creates fresh instance; Replace disposes after successful validation; Back restores exact prior instance. Stale instance/activation results are ignored.

## 6. Manifest contributions and package trust

A documented dependency approval record is an entry artifact for CW-09: repository path, date/approver, each need (semver, tar.gz, schema/restricted regex, process groups, framing), exact crate/version or standard-library boundary, license/security rationale, and tests. RED implementation and Cargo edits cannot begin without the approved artifact.

Package path is `<root>/<plugin-id>/<canonical-semver>/plugin.json`; archives reject links, special files, traversal, duplicates, bounds, privilege bits. Staging 0700; directories/providers/resources 0755/0755/0644. User root only is writable.

Manifest schema 1 closed top-level fields: `manifest_schema,id,version,display_name,host_api{min,max},protocol=1,provider,config?,actions[],panels[],routes[],screens[],defaults?`. Provider is `{mode:none|one-shot|persistent,binaries:{triple:path}}`; `none` forbids handlers/binaries. Platform triples supported are exact host build triples; absent is Unsupported Platform.

Action declaration is `{id,label,description,category,contexts[1..32],argument_schema?,timeout_seconds:1..600,destructive,confirmation:none|host-before-invoke|provider-continuation,handler,allowed_outcomes[]}`. Panel declaration is `{id,model_kinds[],event_schema,handler,ports[]}`. Route declaration is `{id,activation_schema,target_screen}`. Screen declaration is `{path,bindings?:[screen IDs]}` where bindings explicitly name screens contributed by that file; every external screen ID is bound exactly once. Handshake cannot add declarations.

Config schema is `{schema_version,fields[]}` with boolean/string/integer/number/enum/path/string-list/secret-reference, typed default, required, numeric min/max (finite numbers only), string/list min/max, unique choices, sibling acyclic visibility, and restart none/provider/host. Secret reference is exactly `{"env":"[A-Z_][A-Z0-9_]{0,127}"}`.

## 7. Provider lifecycle, payloads, environment, outcomes

Every UTF-8 JSONL envelope is closed: `{protocol:1,type,request_id,generation,payload}`. Request IDs match `[hp]-[0-9]{6,20}`; generation is positive u64 fixed for process. Unknown fields/types, duplicate keys/IDs, wrong generation/order, post-terminal data, malformed/non-UTF-8/oversize lines are fatal `PLG-E502`.

**One-shot providers execute no process at startup.** Static declarations publish from the manifest. Each invocation independently does `spawn -> hello/hello-ack -> configure/ready -> invoke-action -> progress* -> terminal outcome/error -> shutdown/shutdown-ack -> EOF/reap`. It cannot gate startup publication. **Persistent providers alone** start in sorted plugin-ID order during candidate startup and each does `spawn -> hello/ack -> configure/ready`; all Ready responses are required before publication. Request cycles follow; shutdown always reaps. No automatic restart.

Payloads are exact:

| Type | Sender/state | Payload |
|---|---|---|
| `hello` | host/Spawned | `{host_api,protocol,plugin_id,plugin_version}` |
| `hello-ack` | provider/HelloSent | `{provider_name,protocol}` |
| `configure` | host/HelloAck | `{config_version,config,secrets,environment}` |
| `ready` | provider/Configured | `{capabilities:[]}` (must be declared subset) |
| `invoke-action` | host/Ready | `{invocation_id,action_id,arguments,context,continuation?}` |
| `activate-panel` | host/Ready | `{panel_instance_id,panel_type_id,config,activation}` |
| `deactivate-panel` | host/Ready | `{panel_instance_id}` |
| `panel-event` | host/Ready | `{panel_instance_id,event}` |
| `validate-config` | host/pre-Configure candidate | `{config_version,config}` |
| `migrate-config` | host/pre-Configure candidate | `{from_version,to_version,config,draft_token}` |
| `cancel` | host/request active | `{target_request_id}` |
| `progress` | provider/request active | `{sequence,message,completed?,total?}` |
| `outcome` | provider/request active | `{kind,...kind payload}` |
| `error` | provider/phase or request | `{code,message,retryable,field_errors[]}` |
| `shutdown`/`shutdown-ack` | host/provider | `{reason}` / `{}` |

Each request has progress sequence starting 1 and increasing, then exactly one terminal. Cancel is best effort; first terminal wins. Configure environment starts empty and includes only `PATH` fixed to provider directory plus platform system bins, `HOME`, `TMPDIR`, locale, and declared non-secret names; `secrets` contains only resolved owner references. No ambient inheritance.

Closed outcomes: navigate declared route; refresh current known resource; bounded notice; replace owned live panel snapshot; request host confirmation; close owned panel; migrated-config only for migration. No command/URL/private message/persistence/PTY/arbitrary effect.

Host confirmation continuation is exact: provider returns `request-host-confirmation{confirmation_id,title,body,confirm_label,destructive,continuation_schema}` as terminal for invocation A. Host validates declaration and shows modal. Cancel ends with no new provider request. Confirm creates invocation B with fresh request/invocation IDs and `continuation={confirmation_id,approved:true,values}` validated by declared schema; one-shot starts a fresh process/configure cycle, persistent uses Ready process. A confirmation ID is single-use, bound to owner/action/context/generation and expires after 5 minutes. Provider cannot claim confirmation without this continuation.

Process owner continuously drains stdout/stderr, redacts stderr, isolates Unix process group/session, closes stdin then shutdown/terminate/kill/reap within bounds. Retry creates a generation; stale output never mutates state.

## 8. Panel snapshots, models, events, and state machine

`PanelSnapshot={model_schema:1,panel_instance_id,generation,revision,kind,title,description?,loading,action_affordances[],body}`. Full snapshots only; revision starts 1 and strictly increases per instance/generation. Common strings obey limits/control-character rules. Affordance is `{id,label,action_id,arguments?,enabled,unavailable_reason?}` and action must be declared/available.

Bodies are exact:

| Kind | Body |
|---|---|
| list | `{items:[{id,label,description?,status?,actions:[]}],selected_id?,next_page_token?}` unique stable IDs |
| detail | `{document,metadata:[{label,value}],actions:[]}` |
| form | `{fields:[constrained field],values,field_errors:[{field_id,message}],submit_action}` |
| status | `{rows:[{label,value,state:normal|warning|error}]}` |
| progress | `{message,completed?,total?,cancellable}` with `0<=completed<=total` |
| empty | `{message,action?}` |
| error | `{code,message,retryable,retry_action?}` |

Panel events are closed semantic DTOs: `selected{id}`, `activated{id}`, `action{id,arguments}`, `field-changed{field_id,value}`, `submit{values}`, `page-requested{token}`, `retry{}`, `cancel{}`, `link-selected{link_id}`. Manifest event schema lists allowed event kinds and argument schemas; unknown events are never sent. Host owns focus, keys/mouse translation, wrapping, scroll offset, selected-item repair, confirmation, links, theme, accessibility.

State machine: Declared -> Activating -> Active(revision) -> Suspended -> Active, or Failed -> Activating on explicit Retry; any live state -> Disposing -> Disposed. Activate/Retry uses new generation. Suspension sends Deactivate and stops subscription, retains only host-local `{focus_target,scroll_offset,selected_id,form_draft}` bounded to 64 KiB; snapshots/provider state are never persisted. Disposal sends Deactivate if needed and invalidates IDs. Invalid snapshot enters Failed; last accepted model may display stale in memory.

## 9. Closed effect/correlation contract and migration

The minimal contract is introduced by CW-01 and used by every later issue: an intent has `correlation_id`, owner, screen/activation generation, and semantic key; reducer validates and commits state plus a bounded vector of closed effects; state borrow/lock is released; then effects execute. Effects are only persistence, agent probe/runtime, GitHub, SSH/tmux, provider, clipboard/URL, and timer adapters with typed payloads. Completion echoes correlation/owner/generation/key. Stale completion is ignored. `Never` retries only explicitly; `IdempotentQuery{max_attempts<=3}` may retry. No event bus or general queue. CW-13 only audits/enforces this contract and cannot introduce another store/effect model.

Plugin config migration is legal pre-Configure only. Request includes exact versions/config/draft token; `migrated-config` echoes all and supplies target config/notes. Host validates bounds, target schema, absence of secret values, and shows redacted lossless diff. Approve atomically writes exact version/config; cancel/failure changes neither.

## 10. Fixtures, traceability, and UI states

Every parser/introducing issue owns canonical valid, unsupported-version, unknown-field, duplicate/ownership, escape, every limit and limit+1 (including nested depth/aggregate bytes), round-trip/lossless, malformed/partial/non-UTF-8/backpressure/crash/cancel/stale/cleanup, and source-span/redaction fixtures. Tests cite EARS IDs. Four-agent fixtures include LLxprt, Code Puppy, capability-verified Codex CLI, and capability-verified Claude Code.

For each user-visible issue the roadmap's UI matrix is mandatory: Normal, Focused, Unavailable/Error, Tiny, and Dirty/Recovery where applicable. Every cell maps to a named scenario and asserts visible focus, keyboard reachability, no color-only meaning, adjacent plus summary validation, unavailable reason, modal trap/restore, grapheme-safe clipping, and protected exit. Quality gates remain unchanged.
