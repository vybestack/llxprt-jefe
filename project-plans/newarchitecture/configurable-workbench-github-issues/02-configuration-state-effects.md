# CW-01: Exact configuration/state migration, offline recovery, and closed effects

## Outcome and consumed capability

Deliver one path/document/migration/writer authority and a minimal typed effect boundary. Consume the deterministic harness contract: schema-1 contained files, explicit empty environment, capture assertions, real-PTY restart, bounded literal waits, physical containment, and redacted reports. No other issue or planning document defines required behavior.

## Exact source and responsibility inventory

| Source/symbol | Current responsibility | Required ownership/parity |
|---|---|---|
| `src/persistence/mod.rs::{Settings,State,SETTINGS_SCHEMA_VERSION,STATE_SCHEMA_VERSION}` | schema-1 DTO/load/save | facade over schema-2 document/state authorities; preserve schema-1 effective values |
| `src/persistence/tests.rs` | persistence tests | retain every old golden and add migration/lossless/write-phase matrices |
| `src/app_init.rs` and `src/startup.rs` | startup path/load | consume `ResolvedPaths` and provider-free diagnostics; never select paths independently |
| `src/cli.rs` and `src/main.rs` | command dispatch | own exact recovery syntax/exits below without TUI/provider startup |
| `src/state/mod.rs::AppState::apply_message` | transitions | return `Transition`; never execute an adapter while state is borrowed |
| `src/messages.rs` and `src/messages/event_conversion.rs` | messages/events | carry typed intents/completions and correlation, never generic JSON effects |
| `src/app_shell.rs` | composition | commit, release state access, execute ordered effects, deliver completion |

Create cohesive `src/persistence/{paths,settings_document,state_v2,migration,writer,diagnostic}.rs` modules and I/O-free effect DTOs in the domain/application boundary. These names are normative unless an existing module already owns the exact concern; update this inventory if reused.

## Paths and physical identity

`--config DIR` selects `DIR/settings.toml`, `DIR/state.json`, `DIR/definitions`, `DIR/plugins`, `DIR/themes` and ignores path env. Otherwise each explicit `JEFE_SETTINGS_PATH`/`JEFE_STATE_PATH` wins for its file, then `JEFE_CONFIG_DIR/settings.toml` and `JEFE_STATE_DIR/state.json`, then macOS `~/Library/Application Support/jefe/{settings.toml,state.json}`, or Linux `${XDG_CONFIG_HOME:-$HOME/.config}/jefe/settings.toml` and `${XDG_STATE_HOME:-$HOME/.local/state}/jefe/state.json`. Empty/non-UTF-8 path variables are `CFG-E001`.

`PhysicalIdentity` is canonical existing path plus `(device,inode)` where supported; a missing leaf is canonical no-follow parent plus basename. Aliases deduplicate. If target is absent: no legacy source means empty in-memory state; one valid physically distinct source is copied atomically and retained; malformed source exits 2; multiple distinct sources exit 3. If target exists, any distinct legacy source exits 3 even when bytes match. Nothing merges or deletes.

## Closed serialization and migration

Settings is lossless TOML with required `settings_schema = 2`; allowed roots are `appearance`, `workbench`, `agents`, `keymap`, `plugins`, `extensions`. Appearance has only `theme?:string` and `override_agent_theme?:bool`. Workbench has only `initial_screen?:Id`, `enabled_screens?:[Id]`, `screen_order?:[Id]`, `layout_overrides?:map`. Agent entries have only `enabled?:bool`, `repository_defaults?:typed-map`; keymap leaves are chord arrays; plugin entries have only `enabled?:bool`, `version?:canonical-semver`, `config?:typed-map`. Active known owners reject unknown fields. Unknown owner subtrees are dormant byte-preserved syntax and never publish. `extensions` is always dormant. Maps merge recursively; enabled/order/chord arrays and layout trees replace wholly; reset removes the syntax node.

The document retains original bytes, token spans, comments, key order, quoting, provenance, and SHA-256. Read never writes. Save validates the complete candidate, rereads and compares SHA-256, patches only edited syntax paths, writes a unique same-directory mode-0600 file, flushes and syncs it, atomically renames it, then syncs the parent when supported. Only successful rename changes authority. Conflict is `CFG-E007`; phase failure is `CFG-E104`; disk and exportable draft remain intact.

State JSON rejects duplicate/unknown fields:

```text
StateV2={state_schema:2,revision:u64,repositories:[Repository],agents:[Agent],selection:Selection,
 last_selected_agent_by_repo:{RepositoryId:AgentId},preferences:Preferences,dormant_records:[Dormant]}
Repository={id,location:{local_path:string}|{remote_target:string},display_name,
 agent_defaults:{type_id:Id,values:TypedMap}}
Agent={id,repository_id,type_id,values,launch_signature:{version:1,definition_hash:Sha256,
 typed_value_hash:Sha256,target_fingerprint:Sha256},runtime:{session_id?:string,
 invocation_generation:u64,last_known:"stopped"|"running"|"unknown"}}
Selection={repository_id?:Id,agent_id?:Id,screen_id?:Id}
Preferences={hide_idle_repositories:bool,pane_focus:string,terminal_focused:bool}
Dormant={kind:string,stable_id?:Id,raw_schema:u64,reason:string,raw_value:JsonValue}
```

References and IDs are unique; no index persists. Schema-1 settings requires `schema_version=1`, `theme`, optional default-false `override_agent_theme`; migrate effective values in memory and preserve unknown legacy syntax until explicit save/`format --migrate`. Schema-1 state maps stable IDs from canonical repository identity plus collision ordinal and legacy agent identity; valid indices become IDs, invalid indices become absent plus `CFG-W004`; absent/`llxprt` becomes `core.llxprt`; `code_puppy`, `code-puppy`, and `codepuppy` become `core.code-puppy`; all product values remain typed; unknown records become dormant. Retain session ID but set liveness unknown. Revision increments exactly once. Reapplying migration to schema 2 is a semantic no-op.

Bounds are inclusive: file 1,048,576 bytes; nesting 16; map 256; array 1,024; string 262,144; path 4,096; ID 128; diagnostics 256; provenance origins 16; effects/follow-ups 64. IDs match `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`. Diagnostics are `{code,severity,path,span?,owner?,owner_version?,provenance[],correction,redacted_detail}` sorted severity error/warning/info, canonical path, span, code. Use `CFG-E001` path, `CFG-E002` syntax, `CFG-E003` type, `CFG-W004` dormant/repair, `CFG-E005` ownership, `CFG-E006` reference, `CFG-E007` conflict, `CFG-E008` limit, `CFG-E102` unsupported settings schema, `CFG-E103` unsupported/malformed state, `CFG-E104` write.

## Effects contract

```rust
pub struct Correlation { correlation_id: CorrelationId, owner: OwnerId,
    screen_generation: u64, activation_generation: u64, semantic_key: SemanticKey }
pub struct Transition { next_state: AppState, effects: Vec<Effect> }
pub enum Effect { Persistence(PersistenceEffect), AgentProbe(ProbeEffect), Runtime(RuntimeEffect),
    GitHub(GitHubEffect), SshTmux(SshTmuxEffect), Provider(ProviderEffect),
    ClipboardUrl(ClipboardUrlEffect), Timer(TimerEffect) }
pub struct Completion<T> { correlation: Correlation, result: Result<T, EffectError> }
pub enum RetryPolicy { Never, IdempotentQuery { max_attempts: NonZeroU8 } }
```

`max_attempts` is 1–3. Reducer validates and commits at most 64 ordered effects; the shell releases all state access before execution. Completion must exactly match owner, screen generation, activation generation, and semantic key; otherwise it changes nothing. Persistence writes are revisioned and only newest pending revision can become authoritative. No closure, service, handle, generic payload, bus, or queue enters state.

## Recovery CLI and complete flow

| Command | Side effects | Exit |
|---|---|---|
| `jefe config path [--config DIR]` | print selected, legacy, canonical, physical identity, provenance | 0/2/3 |
| `jefe config validate [--config DIR]` | static parse/migration only; print skipped semantic owners | 0/2/3 |
| `jefe config show-effective [--config DIR] [--provenance]` | redacted output, no write | 0/2/3 |
| `jefe config edit [--config DIR]` | execute configured editor as argv, never shell | 0/4 |
| `jefe config format [--config DIR] [--check] [--migrate]` | check is read-only; otherwise owned-node patch | 0/2/4 |
| `jefe config migrate-state [--config DIR]` | apply path decision and atomic target write only | 0/2/3/4 |

Unknown option/missing operand exits 64. Recovery initializes no TUI, provider, probe, network, tmux, or PTY. Startup resolves paths, reads bounded bytes, migrates in memory or performs only the decision-table import, validates, and hands immutable candidates to later composition. Malformed selected state blocks normal startup while preserving bytes and printing exact validate/migrate commands. Secrets never appear in state, hash input diagnostics, effective output, provenance, logs, or harness capture.

## UI applicability

No TUI is introduced. Normal, focused, unavailable, dirty, and small-terminal states are individually not applicable. Error and recovery are CLI-only and require distinct goldens: malformed state, ambiguous source, hash conflict, and write failure. This is not a combined-state waiver.

## Test-first EARS ledger

| ID | Singular requirement | Scenario/test evidence |
|---|---|---|
| CW01-01 | WHEN paths resolve, Jefe shall apply the exact precedence and physical identity rules. | `config-path-precedence.json`; macOS/Linux/override matrix |
| CW01-02 | WHEN exactly one valid legacy source exists, Jefe shall atomically import it and retain its source. | `config-legacy-import.json`; phase capture |
| CW01-03 | IF distinct candidates are ambiguous, Jefe shall exit 3 without modifying any file. | `config-ambiguity.json`; byte-equal/different fixture |
| CW01-04 | WHEN schema-1 settings migrate, Jefe shall preserve effective values and all dormant syntax without writing. | `settings-v1-lossless.json`; byte/token golden |
| CW01-05 | WHEN schema-1 state migrates, Jefe shall produce stable schema-2 IDs, signatures, selections, and dormant records. | `state-v1-v2.json`; complete field golden |
| CW01-06 | WHEN migration repeats, Jefe shall produce semantically identical state. | migration idempotence property |
| CW01-07 | WHEN a matching-hash candidate saves, Jefe shall patch only edited paths through every atomic phase. | `settings-lossless-save.json`; phase fault matrix |
| CW01-08 | IF disk hash differs, Jefe shall retain disk and draft and report `CFG-E007`. | `settings-hash-conflict.json` |
| CW01-09 | WHEN recovery commands run, Jefe shall start zero providers and zero TUI processes. | `config-provider-free.json`; hanging provider capture |
| CW01-10 | WHEN a reducer emits an effect, Jefe shall commit and release state before execution. | `effect-after-commit.json`; all effect variants |
| CW01-11 | IF completion identity is stale, Jefe shall leave current state byte-equivalent. | generation property and old-correlation fixture |
| CW01-12 | IF every bound is exceeded by one, Jefe shall reject at the owning parser. | depth/map/array/file/string/path/diagnostic matrix |

RED adds these fixtures first; GREEN adds owners; REFACTOR removes old selectors/writers and under-borrow effects only after parity.

## Normative documentation and done

Update `dev-docs/standards/persistence-and-runtime.md` with path precedence, physical identity, schemas, migration, writer phases, effects ordering, stale completion, and recovery exits; update `dev-docs/RULES.md` with lossless/no-reader-write and provider-free recovery requirements. Done requires old persistence tests plus every ledger row and unchanged `make ci-check`; no new dependency, unsafe, production unwrap/expect, shell command, secret leak, threshold or lint suppression.