# CW-02: Complete vertical four-agent definition cutover

## Parent, entry gates, dependencies, and user outcome

Parent: **Epic: Configurable Workbench v1**. Consumes the delivered deterministic real-process harness and schema-2 persistence/effect identity contracts. This issue is one vertical cutover: LLxprt, Code Puppy, Codex CLI, and Claude Code become peers through one registry from detection through create, restore, normal/resume/fresh-Issue/fresh-PR, local/remote planning, preflight, tmux, and Issue/PR Send.

**Claude entry gate:** before RED, the maintainer must acquire the exact supported Claude executable release artifact, record its version and SHA-256, and capture `claude --help` from that artifact. No local Claude executable was found during issue preparation. Implementation may use only mappings both listed in the official reference at <https://code.claude.com/docs/en/cli-reference> and reproduced by the acquired artifact. An unproven capability is `Unsupported`, visible and disabled. This gate has no value to fill into this body and cannot be waived by guessing.

End-to-end outcome: after restart, each enabled installed compatible definition appears with provenance; forms are generated from the definition; create/restore/send use one immutable launch plan; unsupported operations remain visible with a reason; stale probes/plans perform no filesystem, clone, prompt, SSH, tmux, or process side effect.

## Audited source responsibilities

| Existing source symbol | Current responsibility | Required final responsibility |
|---|---|---|
| `src/domain/mod.rs::AgentKind`, `Agent`, `Repository`, `LaunchSignature` | closed product identity and persisted/runtime values | replace generic `AgentKind` authority with `AgentTypeId`; retain a named schema-1 LLxprt migration adapter only |
| `src/agent_detection.rs` | PATH detection for LLxprt/Code Puppy | candidate resolution and probe adapter consuming registry definitions |
| `src/state/form_projection.rs`, `form_runtime.rs`, `form_ops.rs`, `form_build.rs`, `form_types.rs`, `modal_ops.rs` | product-branched forms | pure projections and typed form values from `AgentDefinition` |
| `src/selection/form_content.rs` | form presentation projection | render generated labels/status/reasons only |
| `src/ui/screens/new_agent.rs`, `new_repository.rs` | product-aware form UI | thin generated-form renderer emitting typed intent |
| `src/runtime/commands.rs` | product command assembly | execute only validated `AgentLaunchPlan`; no product match |
| `src/runtime/manager.rs`, `session.rs`, `preflight.rs` | tmux/PTY/preflight | own process/tmux/preflight execution, never definition policy |
| `src/app_input/availability.rs`, `preflight.rs`, `remote_probe.rs`, `fresh_prompt.rs` | product policy | invoke resolver/probe/planner and map typed results |
| `src/app_input/issue_send.rs`, `issues_send.rs`, PR send modules | fresh send orchestration | request `FreshIssue`/`FreshPullRequest` plan; no product branch |
| `src/app_init.rs` | restore and sandbox checks | reconcile signature, liveness, and current probe generation |
| `src/persistence/mod.rs` migration modules | schema-1 product serialization | map aliases/fields into typed values and dormant records |

New cohesive modules may be placed under `src/domain/` for contracts/validation, `src/agent_detection.rs` plus focused sibling modules for registry/resolution/probing/planning, and `src/runtime/` for local/SSH execution. The composition root wires adapters.

## Consumed dependency contracts

| Contract consumed | Exact use; this issue must not redefine it |
|---|---|
| Harness schema 1 | fixture file/process capture, exact argv/env/cwd/stdin, real PTY, resize, restart, bounded literal waits, secret redaction |
| Settings/State schema 2 | `agents.<type-id>.enabled`, typed values, stable IDs, dormant unknown owners, launch signature, lossless writes |
| `Correlation`/closed `Effect` | generation-bearing probe/preflight/runtime completions; effects execute only after committed state is released |
| Architecture standards | UI renders/emits intent; reducers are deterministic and I/O-free; runtime owns process/tmux; persistence owns file I/O |

## External CLI provenance and supported mapping

| Agent | Acquisition evidence | Proven mapping used by shipped definition | Explicitly unavailable without additional pinned evidence |
|---|---|---|---|
| Codex CLI | `/opt/homebrew/bin/codex`; `codex-cli 0.142.0`; usage `codex [OPTIONS] [PROMPT]` | executable `codex`; initial prompt positional; model `--model`; profile `--profile`; sandbox `--sandbox read-only|workspace-write|danger-full-access`; approval `--ask-for-approval untrusted|on-failure|on-request|never`; bypass `--dangerously-bypass-approvals-and-sandbox`; cwd `--cd`; resume subcommand `resume`, optional `--last` | remote/setup and any flag not in the pinned artifact capture |
| Claude Code | **not installed locally**; official reference URL above; exact supported-release artifact/version/SHA-256/help capture is mandatory entry gate | executable `claude`; initial interactive prompt positional; continue `--continue`/`-c`; resume `--resume`/`-r ID`; model `--model`; `--permission-mode` only with values documented by the acquired release; bypass `--dangerously-skip-permissions` | every capability not proven by both acquired supported artifact and official reference, including remote/setup assumptions |
| LLxprt | `/opt/homebrew/bin/llxprt`; `0.10.0-nightly.260712.21cb698b6` | profile `--profile-load`; interactive prompt `-i`/`--prompt-interactive`; sandbox `--sandbox`; engine; yolo; approval mode; continue; repository-local `<repo>/.llxprt/bin/llxprt` precedes PATH | mappings absent from pinned help/fixture |
| Code Puppy | `/Users/acoliver/.local/bin/code-puppy`; `0.0.634` | interactive `-i`; model; resume; quick-resume; yolo boolean `true|false` | mappings absent from pinned help/fixture |

Before implementation, fixtures store for every row: canonical executable artifact SHA-256, version output, complete help output, exact argv/env/cwd capture for each supported operation/target, raw probe streams, parsed identity/capabilities, and source URL/date for Claude. Hash or output drift makes that definition incompatible until re-pinned; it never silently broadens support.

## Closed contracts

```rust
pub struct AgentTypeId(String);
pub struct AgentDefinition {
    pub schema: u16,
    pub id: AgentTypeId,
    pub display_name: String,
    pub candidates: Vec<ExecutableCandidate>,
    pub probe: ProbeSpec,
    pub operations: OperationMatrix,
    pub targets: TargetMatrix,
    pub repository_fields: Vec<Field>,
    pub agent_fields: Vec<Field>,
    pub emitters: Vec<Emitter>,
}
pub enum Support { Supported, Unsupported { reason: String } }
pub enum Availability {
    NotFound,
    InstalledCompatible { identity: String, capabilities: Vec<String>, generation: u64 },
    InstalledIncompatible { reason: String, generation: u64 },
    ProbeError { code: ProbeErrorCode, reason: String, generation: u64 },
}
pub enum Operation { Normal, Resume, FreshIssue, FreshPullRequest }
pub enum Target { Local { canonical_cwd: PathBuf }, Remote(RemoteTarget) }
pub struct AgentLaunchPlan {
    pub type_id: AgentTypeId,
    pub operation: Operation,
    pub definition_sha256: [u8; 32],
    pub executable: PathBuf,
    pub argv: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
    pub target: Target,
    pub probe_generation: u64,
    pub target_generation: u64,
    pub preflight: Preflight,
    pub signature: LaunchSignature,
}
```

Serialized definition schema is closed:

```text
AgentDefinition={agent_type_schema:1,id,display_name,
 executable_candidates:[ExecutableCandidate 1..8],probe,
 operations:{normal:OperationSupport,resume:OperationSupport,
 fresh_issue:OperationSupport,fresh_pull_request:OperationSupport},
 targets:{local:TargetSupport,remote:TargetSupport},
 repository_fields:[Field 0..64],agent_fields:[Field 0..64],emitters:[Emitter 0..128]}
ExecutableCandidate={kind:"path-name",value:string}|{kind:"repository-llxprt",value:".llxprt/bin/llxprt"}
OperationSupport={supported:true,prompt:None|InitialPositional|InteractiveOption}|{supported:false,reason:string}
TargetSupport={supported:true}|{supported:false,reason:string}
Field={id,kind:Boolean|OptionalBoolean|String|Integer|Enum|Path|StringList,
 required,default?,minimum?,maximum?,choices:[string 0..64],visible_when?,launch_signature}
Emitter={kind:Fixed,value}|{kind:Flag,field}|{kind:Option,name,field}|
 {kind:BooleanOption,name,field,true_value,false_value?}|
 {kind:RepeatedOption,name,field}|{kind:Positional,field}|{kind:Environment,name,field}
```

There is no generic JSON value, shell template, token splitting, setup command, script, or raw-argument field. Active unknown fields fail. IDs are lowercase ASCII, 1–128 bytes, matching `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`. A sibling visibility graph must be acyclic.

Probe schema is closed:

```text
ProbeSpec={argv:[string 1..8],stream:Stdout|Stderr|Combined,
 framing:SingleJson|JsonLines|Utf8Text,
 identity:JsonPointer{pointer,anchored_pattern}|Line{prefix,anchored_pattern},
 capabilities:JsonArray{pointer}|PrefixedLines{prefix:"capability:"},
 required:[ID 0..32],timeout_ms:1..5000,max_bytes:1..65536}
```

RFC 6901 pointers are required. The implementation uses a bounded parser, not a new regex dependency: shipped anchored patterns are exact prefix/suffix/version-token recognizers represented as typed enums. Duplicate JSON keys, trailing bytes after single JSON, invalid UTF-8, overlong lines, duplicate/invalid capabilities, truncation, timeout, signal/nonzero exit, identity mismatch, or malformed framing is `ProbeError`. Valid identity lacking required capabilities is `InstalledIncompatible`. Capabilities sort bytewise.

## Deterministic algorithms and limits

1. Snapshot PATH once at startup. For each definition in ID order, inspect candidates in declaration order. Candidate values containing `/` are rejected except the typed repository-LLxprt candidate.
2. Canonicalize, open, and fingerprint `(canonical path, device/inode where available, size, mtime)` before probe. Read stdout and stderr concurrently, each capped at 65,536 bytes. Local timeout is 5 seconds; remote timeout is 20 seconds.
3. A successful probe records executable fingerprint, definition SHA-256, identity, sorted capabilities, and monotonically increasing generation.
4. Resolve operation and target support before preparation. Unsupported returns the declared reason and emits zero effects.
5. Validate typed values, visibility, bounds, and generations; emit argv element-by-element in declaration order. Environment starts empty and receives only `TERM`, `LANG`/`LC_*`, repository-declared safe names, fixed definition names, typed environment emitters, and verified operation values. tmux and unrelated ambient variables are excluded.
6. Preflight runs before clone/reset/prompt write/SSH/tmux/spawn. A sandbox engine is canonicalized/fingerprinted; an image is inspected by fixed argv without pull/build; only required environment names are reported. Failure returns `Unavailable` and performs no later effect.
7. Recheck executable, probe, target, and activation generations immediately before execution. Mismatch is `AGT-E203` and performs no side effect.
8. Local execution preserves `OsString` argv elements. Remote execution uses the existing audited SSH boundary with one POSIX single-quote serializer: each byte string is enclosed in single quotes and each embedded `'` is emitted as `'"'"'`; NUL and non-representable remote bytes are rejected. No shell syntax from a definition is accepted.

Artifact 1,048,576 bytes; data depth 16; map 256; array 1024; path 4096 bytes; fields 64 per scope/128 total form; emitters 128; candidates/probe argv 8; capabilities 32; probe stdout and stderr 65,536 bytes. Diagnostics and captures never contain secret values.

## Operation rules and migration

Fresh LLxprt forces continue false, removes only the typed continue emitter, and emits one `-i`/`--prompt-interactive` prompt according to pinned evidence. Fresh Code Puppy forces quick-resume false and emits one `-i` prompt according to pinned evidence. Codex and Claude emit only the rows proven above; unsupported fresh/resume/remote cells remain visible and disabled.

Schema-1 aliases map `llxprt` or missing kind to `core.llxprt`; `code_puppy`, `code-puppy`, and `codepuppy` map to `core.code-puppy`. Product fields become typed namespaced values. Existing LLxprt `mode_flags` and remote setup bytes are retained only in the named migration/runtime compatibility adapter. Unknown types/fields become dormant records. Signature version 1 hashes type ID, definition SHA-256, launch-signature fields, and target fingerprint; it excludes secrets and display-only fields. Restore requires matching signature and live tmux/process evidence; otherwise status is stopped/unknown.

## Architecture guard allowlist

The source guard rejects case-insensitive `llxprt`, `code puppy`, `code_puppy`, `codex`, `claude`, and the four stable type IDs in generic Rust source. Allowed locations are only:

* shipped definition data modules/files and their fixture hashes;
* `src/agent_detection.rs` repository-local LLxprt candidate adapter;
* the named schema-1 LLxprt/Code-Puppy migration adapter under `src/persistence/`;
* the named LLxprt legacy remote-setup compatibility adapter under `src/runtime/`;
* tests/fixtures that assert provenance, migration, parity, or guard failures.

The guard also rejects `match AgentKind`, generic `if type_id ==`, product-specific form branches, and product-specific Issue/PR send branches outside this allowlist.

## Failure and recovery

| Failure | Typed result | Durable state | Recovery |
|---|---|---|---|
| definition invalid | `AGT-E201` | source bytes and old published registry | fix active definition and restart |
| probe malformed/timed out | `ProbeError`/`AGT-E202` | definition and configured values | Retry creates a new generation |
| required capability absent | `InstalledIncompatible` | definition/config | install supported artifact or disable |
| executable/target changed | `AGT-E203` | current agent record | Reprobe; never execute stale plan |
| preflight unavailable | `Unavailable{reason}` | all state; no preparation mutation | correct engine/image/env and Retry |
| runtime/SSH failure | typed adapter error | definition, values, stopped runtime | explicit Retry/new generation |
| unknown migrated owner | dormant record | exact raw record | install exact owner/schema and restart |

## UI surfaces and state-specific mocks

Keys consume the action registry defaults: arrows or `j/k` select; Space toggles draft enablement; Enter opens/submits; Tab/Shift-Tab cycles; Esc/q backs out; `r` retries/reprobes; F12/`t` toggles terminal capture; Ctrl-Q and rapid bare `qqq` remain protected exits. Ordinary terminal keys and Ctrl-C pass through unchanged.

**Normal**
```text
+ Agent Types ---------------------------+
|  LLxprt       Installed, enabled       |
|  Code Puppy   Installed, enabled       |
|  Codex CLI    Installed, enabled       |
|  Claude Code  Not found, disabled      |
+ Space Toggle  Enter Details  q Back ---+
```

**Focused**
```text
+ Agent Types ---------------------------+
|> Codex CLI    Installed, enabled       |
|  path: /opt/homebrew/bin/codex         |
|  profile/model/sandbox available       |
+ focused row uses border and `>` text --+
```

**Unavailable**
```text
+ New Agent -----------------------------+
| Claude Code                            |
| Resume: Unsupported by Claude Code:    |
| supported artifact was not acquired    |
| [Create disabled] [Back]               |
+----------------------------------------+
```

**Error**
```text
+ Agent Types ---------------------------+
| Codex CLI  Probe error: invalid UTF-8  |
| AGT-E202  [Reprobe]                    |
+----------------------------------------+
```

**Dirty**
```text
+ Save agent enablement? ----------------+
| [Save]  [Discard]  [Cancel]            |
| Tab/Shift-Tab move; Enter choose       |
+----------------------------------------+
```

**Recovery**
```text
+ New Agent -----------------------------+
| Executable changed after probe.        |
| AGT-E203; no process was started.      |
| [Reprobe]  [Back]                      |
+----------------------------------------+
```

**Small terminal**
```text
+Agent Types----+
|>Codex: error  |
| invalid UTF-8 |
| r Retry q Back|
| Ctrl-Q Exit   |
+---------------+
```

**Terminal capture**
```text
+ Codex terminal ------------------------+
| child process output                   |
+ F12/t unfocus; Ctrl-Q protected -------+
```

Focus, status, error, and unavailable reason are textual, never color-only. Modals trap focus and restore the exact prior target. Clipping is grapheme-safe.

## Test-first acceptance ledger

Create each scenario, test, and embedded fixture before implementation and record the failing run.

| ID | Singular EARS criterion | Scenario | Test | Embedded fixture evidence |
|---|---|---|---|---|
| CW02-01 | WHEN candidates resolve, Jefe shall select the first physically valid candidate in declared order. | `agent-resolver-order.json` | `candidate_resolver_order` | repo LLxprt symlink tree plus PATH with two executable captures |
| CW02-02 | WHEN each pinned probe runs, Jefe shall reproduce its recorded identity and capabilities. | `agent-probe-parser.json` | `probe_parser_four_agents` | artifact SHA-256, version/help bytes, stdout/stderr bytes and expected sorted capabilities for all four rows |
| CW02-03 | IF probe framing, UTF-8, bounds, exit, identity, or capability validation fails, Jefe shall return `ProbeError`. | `agent-probe-negative.json` | `probe_negative_table` | one transcript per failure and exact `AGT-E202` correction |
| CW02-04 | IF a required capability is absent, Jefe shall show incompatible and emit zero launch effects. | `agent-incompatible-zero-spawn.json` | `capability_gate` | capture count zero and adjacent reason |
| CW02-05 | WHEN status renders, Jefe shall project every enablement/availability pair exactly once. | `agent-status-cartesian.json` | `status_projection` | 2×4 matrix with visible text and create-enabled boolean |
| CW02-06 | WHEN a supported local operation is submitted, Jefe shall produce the pinned argv/env/cwd plan. | `agent-local-operation-matrix.json` | `local_plan_golden` | all supported four-agent operation captures |
| CW02-07 | WHEN a supported remote operation is submitted, Jefe shall use the audited serializer and pinned remote transcript. | `agent-remote-operation-matrix.json` | `remote_plan_contract` | quote/NUL table and supported remote captures; unsupported cells assert zero SSH |
| CW02-08 | IF operation or target is unsupported, Jefe shall keep it visible with its exact reason and perform zero preparation. | `agent-unsupported-ui.json` | `operation_target_matrix` | full four-agent support matrix and process/file capture count zero |
| CW02-09 | IF sandbox preflight fails, Jefe shall perform no clone, prompt write, tmux, SSH, or agent spawn. | `agent-sandbox-preflight.json` | `preflight_order` | missing engine/image/env cases and all side-effect captures zero |
| CW02-10 | WHEN fresh Issue Send is confirmed, Jefe shall emit exactly one pinned fresh prompt after successful preflight. | `agent-fresh-issue.json` | `fresh_issue_ordering` | literal issue prompt bytes and argv for every supported agent |
| CW02-11 | WHEN fresh PR Send is confirmed, Jefe shall emit exactly one pinned fresh prompt after successful preflight. | `agent-fresh-pr.json` | `fresh_pr_ordering` | literal PR prompt bytes and argv for every supported agent |
| CW02-12 | IF any generation changes before execution, Jefe shall return `AGT-E203` and perform zero side effects. | `agent-stale-generation.json` | `generation_property` | old/new executable, probe, target, and activation generation tuples |
| CW02-13 | WHEN schema-1 records migrate, Jefe shall preserve known typed values and exact dormant unknown records. | `agent-legacy-migration.json` | `agent_migration_golden` | every current LLxprt/Code-Puppy field, aliases, invalid kind, and expected schema-2 JSON |
| CW02-14 | WHEN a matching live launch restores, Jefe shall attach through the existing tmux/PTY boundary. | `agent-terminal-compatibility.json` | `local_remote_tmux` | matching/mismatching signatures, live/dead sessions, resize and Ctrl-C/F12 captures |
| CW02-15 | WHEN the architecture guard scans source, Jefe shall find product tokens only in the explicit allowlist. | `agent-no-product-branches.json` | `agent_architecture_guard` | allowlisted paths plus one forbidden generic branch per pattern |
| CW02-16 | IF Claude artifact evidence is absent, Jefe shall publish Claude as unavailable and execute zero Claude process. | `agent-claude-evidence-gate.json` | `claude_entry_gate` | no local executable, official URL, empty acquired-artifact record, zero process capture |

GREEN builds one pipeline. REFACTOR removes product branches only after every parity fixture passes.

## Normative documentation updated by this issue

* `dev-docs/standards/architecture.md`: agent registry/planner dependency direction and the exact product-token allowlist.
* `dev-docs/standards/persistence-and-runtime.md`: `AgentTypeId`, launch signature migration, candidate/probe/preflight/plan/execution order, remote serialization, and environment rules.
* `dev-docs/standards/display-and-ui.md`: generated agent form/status/unavailable presentation and terminal-capture recovery.
* `dev-docs/standards/testing-and-quality.md`: pinned external artifact provenance, zero-side-effect stale/unsupported tests, and four-agent harness matrix.
* `docs/technical-overview.md`: composition-root wiring and end-to-end registry data flow.
* `docs/getting-started.md`: four-agent support/provenance limitations and visible unsupported behavior.

## Definition of done

All sixteen criteria pass; the Claude entry artifact is pinned or Claude is truthfully unavailable as required; all supported mappings are fixture-proven by embedded artifact evidence; no generic product branch, shell/raw args, guessed mapping, secret leak, stale side effect, `unsafe`, production panic/unwrap/expect, lint suppression, dependency addition, or weakened gate exists. Run unchanged `make ci-check` with Rust 2024, source hard limit 1000/warning 750, complexity 15/60/6/3/250, clippy `-D warnings`, coverage at least 30%, and locked all-feature build/tests.