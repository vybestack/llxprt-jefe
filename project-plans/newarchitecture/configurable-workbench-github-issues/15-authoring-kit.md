# CW-14: Authoring schemas and compatibility runner

## Outcome and consumed contracts

Publish machine-checkable copies of all delivered owner contracts and a provider-free compatibility runner. The kit reuses production parsers/validators; it must not reimplement semantics, spawn a provider for static validation, mutate input, or substitute an expectation when an owner fixture is missing. Consume exact agent, action/key, screen/layout/relationship/route, Settings/State, package/manifest/config, provider, panel, effect, diagnostic, and limit contracts.

## Exact paths, symbols, and ownership

| Path/symbol | Required responsibility |
|---|---|
| `author-kit/v1/index.json` | sole kit inventory of contracts, fixtures, examples and SHA-256 hashes |
| `author-kit/v1/schemas/` | closed JSON schemas for JSON-serializable owner DTOs |
| `author-kit/v1/tables/` | normative CSV/JSON tables for TOML duplicate/order, physical path, state machine, and limits not expressible in JSON Schema |
| `author-kit/v1/examples/custom-workbench/` | valid local screen/relationship/keymap example |
| `author-kit/v1/examples/local-agent/` | non-shipped declarative agent with no shell/raw args |
| `author-kit/v1/examples/static-package/` | non-executable `provider.mode=none` package |
| `author-kit/v1/fixtures/` | copied/linked immutable owner canonical and invalid fixtures |
| `src/bin/jefe_compat.rs::main` | exact CLI dispatch/exits |
| new `src/author_kit/mod.rs` | index/hash/layout orchestration only |
| production owner parser modules | imported and invoked directly; sole semantic authorities |
| release resource list | install `author-kit/v1` preserving relative layout |

Installed path is `<share>/jefe/author-kit/v1`; index paths are relative `/`-separated no-root/no-dot/no-symlink paths. Runner locates kit from executable/share installation or explicit `--kit-root PATH`; no repository fallback in installed tests.

## Closed index and runner contract

```text
KitIndex={kit_schema:1,host_api:string,contracts:[Contract],fixtures:[Fixture],examples:[Example]}
Contract={id:Id,version:u64,path:RelativePath,sha256:Sha256,parser:ParserId}
Fixture={id:Id,owner:Id,criterion:Id,path:RelativePath,sha256:Sha256,
 kind:"canonical"|"invalid"|"boundary"|"transcript"}
Example={id:Id,path:RelativePath,sha256:Sha256}
CompatibilityResult={result_schema:1,status:"success"|"warning"|"failure",
 diagnostics:[Diagnostic],checked:[{path,sha256}]}
```

Arrays are sorted by ID and IDs/paths unique. SHA-256 is lowercase 64 hex and covers exact file bytes; directory example hash is SHA-256 of sorted records `relative_path NUL mode-octal NUL file-sha256 LF`. Runner verifies every hash before parse. Missing/unindexed file, symlink, mode mismatch, duplicate owner criterion, unsupported exact version, or hash mismatch fails; no fallback.

CLI:

```text
jefe-compat [--kit-root PATH] validate PATH
jefe-compat [--kit-root PATH] validate-package ROOT
jefe-compat [--kit-root PATH] validate-transcript FILE
jefe-compat [--kit-root PATH] verify-kit
```

Success 0, contract failure 2, filesystem/hash 4, syntax/unknown option/missing operand 64. Output is deterministic error/warning/info then path/span/code; JSON output is not added in v1. Paths are argv elements and never shell-split.

Publish closed schema/table coverage for: ID regex/ownership; Settings 2 and State 2; all four pinned shipped agent provenance hashes plus local agent definition/probe/operation/target/preflight/plan; complete action/chord/context/default inventory; screen descriptor/external syntax/layout algorithm/relationship/route; package manifest/config/archive/installed layout; every provider envelope direction/payload/state/limit; every panel snapshot/body/event/lifecycle; effect/correlation/retry; diagnostics; all inclusive bounds. Canonical/invalid fixtures cover duplicate/unknown/owner/version, physical traversal/symlink, depth/map/array/bytes at-limit and +1, malformed/non-UTF-8, backpressure/crash/cancel/stale/cleanup, spans and redaction.

`validate-transcript` calls the production strict JSONL/framing/protocol/panel parsers in offline mode. It validates hello/ack/configure/ready, invoke/cancel/progress/outcome/error/shutdown, activate/deactivate/event/snapshot, config migration, confirmation two-invocation, direction/order/generation/rate/size. It executes no referenced executable and resolves no real secret. Secret placeholders serialize `${SECRET_REF:NAME}` and reports always redact them.

Bounds are exactly owner bounds: artifact 1,048,576; depth 16; map/array 256/1,024; IDs 128; path 4,096; package entries/expanded/depth/path 4,096/67,108,864/16/1,024; provider line/stderr/snapshot 1,048,576/262,144/524,288; requests/envelopes/progress 16/64/256; model rate 20/s burst 40.

## Flow, compatibility, recovery, security

Resolve installed kit, reject physical escape/symlink, parse closed index, hash every record, select production parser, validate, and emit immutable report. Versions coexist under separate directories; old kits are never rewritten. Parser/schema drift is detected by owner canonical and invalid fixtures in both production and kit. Failure leaves inputs untouched. Recovery is reinstall matching kit/artifact or correct authored input. Runner has bounded reads, empty execution capability, no shell/network/provider/process launch, and redacted diagnostics.

## UI applicability

No TUI is changed. Normal, focused, unavailable, dirty, recovery, and small-terminal TUI states are individually not applicable. Error is CLI-only. Required distinct CLI goldens are success, warning, contract error, hash/filesystem error, unsupported version, and narrow 80-column output; all are grapheme-safe and redacted.

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW14-01 | WHEN kit verifies, runner shall hash every indexed path with the exact file/directory algorithm. | valid/missing/extra/symlink/mode/hash fixtures |
| CW14-02 | WHEN custom example validates, runner shall compose it using production owner parsers. | custom workbench example scenario |
| CW14-03 | WHEN agent fixtures validate, runner shall verify exact four-agent provenance and local-agent grammar. | four-agent index and local example |
| CW14-04 | WHEN package validates, runner shall verify manifest/config/layout through production parsers. | static and provider package fixtures |
| CW14-05 | WHEN transcript validates, runner shall cover every direction/payload/state/body/event/outcome. | transcript coverage index |
| CW14-06 | IF any owner limit is exceeded by one, runner shall return that owner’s diagnostic. | complete boundary index |
| CW14-07 | WHEN static validation runs with executable traps, runner shall spawn zero processes. | provider-free capture |
| CW14-08 | IF an owner fixture is missing or parser behavior drifts, kit verification shall fail rather than substitute. | fixture deletion/mutation tests |
| CW14-09 | WHEN reports render, runner shall sort deterministically and redact all placeholders. | CLI golden/secret scan |
| CW14-10 | WHEN installed layout runs outside repository, runner shall use only installed paths. | relocated installation test |

RED index/hash/parser-reuse tests first; GREEN deterministic orchestration; REFACTOR generated schema copies only where hashes remain stable.

## Normative documentation and done

Create `dev-docs/authoring-kit.md` documenting exact installed paths, hash algorithm, CLI/exits, parser reuse, schemas/tables/examples/security/versioning; update `dev-docs/standards/testing-and-quality.md` with owner-fixture/hash drift gates and packaging requirements. Done requires complete indexed coverage, no duplicate parser, process spawn, input mutation, secret leak, repository fallback, and unchanged `make ci-check`; no new dependency or gate weakening.