# CW-09: Package roots, manifest inventory, archive install, and explicit trust

> **Issue-creation blocker — not yet paste-ready:** repository policy requires approval before adding dependencies, and the current `Cargo.toml` contains `serde`, `serde_json`, `toml`, `iocraft`, `crossterm`, `taffy`, `unicode-width`, `portable-pty`, `alacritty_terminal`, `smol`, `thiserror`, `tracing`, `tracing-subscriber`, `dirs`, `lru`, and `comrak`, but no dependency that can safely decode `tar.gz`, provide canonical Semantic Versioning precedence, or compute SHA-256. Rust `std` and those existing dependencies do not provide all three. The maintainer must commit the completed decision record below before this text is created as a GitHub issue. Blank decision values are deliberately not represented by placeholders; until an approved record exists, this body is candidly blocked rather than falsely paste-ready.

## Mandatory maintainer-owned decision record

The committed record must be `dev-docs/decisions/plugin-package-dependencies.md`, owned by the repository maintainer, approved before RED, and contain all of these concrete values:

1. approval date, approver GitHub identity, and linked approval discussion;
2. exact crate name and exact lockfile version for canonical Semantic Versioning parse/order;
3. exact crate names and exact lockfile versions for streaming gzip and tar decoding;
4. exact crate name and exact lockfile version for SHA-256;
5. each crate's SPDX license, repository URL, minimum supported Rust version, direct/transitive dependency count, latest advisory scan result/date, and maintenance rationale;
6. rejection rationale for implementing gzip, tar, SemVer, or SHA-256 locally and for invoking external `tar`, `gzip`, `openssl`, or shell commands;
7. proof that duplicate archive paths, links/special files, streaming expanded-byte bounds, canonical SemVer rejection, and digest known-answer vectors are testable through the selected APIs;
8. approval or rejection of a process-group helper dependency for the later provider-supervisor issue; this issue itself starts no provider;
9. the exact resulting `Cargo.toml` lines and `Cargo.lock` package/version/checksum entries.

If approval rejects any required dependency without naming an already-present safe implementation, this issue must be split or cancelled; implementation must not substitute a home-grown parser/cryptographic primitive or host command.

## Parent, dependencies, and complete outcome

Parent: **Epic: Configurable Workbench v1**. Consumes schema-2 path/settings/lossless-writer contracts and the delivered Settings shell. End-to-end: provider-free discovery lists every physically distinct installed package/version; archive installation validates and normalizes into the user root atomically; packages remain disabled until explicit trust; exact version/config selection persists losslessly; restart publishes static declarations only. This issue starts zero provider processes and adds no update command.

## Source and symbol responsibility table

| Source/module | Final responsibility |
|---|---|
| `src/persistence/` path authority | expose canonical config plugin root and physical identity service; never parse manifests |
| new cohesive package-domain modules under `src/domain/` | closed manifest/config/declaration DTOs, IDs, canonical SemVer value, static validation |
| new package inventory module outside UI/state | ordered root scan, alias/dedup/ambiguity, selected immutable package snapshot |
| new package install adapter at persistence boundary | bounded archive stream, staging modes, digest, atomic rename/remove |
| Settings draft reducer | typed enable/disable/version/config intents only |
| thin Settings Plugins UI | render immutable inventory and emit intents; no scan/install/write/process |
| CLI/composition root | wire provider-free list/inspect/install/enable/disable/rollback/remove commands |

## Consumed dependency contracts

| Contract | Exact use |
|---|---|
| persistence path/physical identity | canonical roots, `(device,inode)` where available, missing-final-parent identity, mode-0600 settings writer |
| Settings schema 2 | `plugins.<id>={enabled,version,config}`; unknown absent owners remain dormant/lossless |
| Settings dirty lifecycle | Save/Discard/Cancel and hash-conflict/write recovery |
| action/screen declaration contracts | validate manifest declarations but do not execute or render them |
| harness schema 1 | physical root trees, archive bytes/modes, process-capture assertion of zero provider starts, restart durability |

## Ordered roots and identity algorithm

Low-to-high discovery order is:

1. canonical executable directory's `../share/jefe/plugins`;
2. macOS `/opt/homebrew/share/jefe/plugins`, then `/usr/local/share/jefe/plugins`; Linux `/usr/local/share/jefe/plugins`, then `/usr/share/jefe/plugins`;
3. `<config>/plugins/installed`.

Skip missing roots. Never inspect PATH or cwd. Canonicalize each existing root and package; compare `(device,inode)` where available, otherwise canonical absolute path. The first occurrence of one physical package wins and later aliases are recorded. Two physically distinct packages with identical `(plugin ID, canonical version)` are ambiguous `PLG-E501`; precedence never resolves that collision. Every selected package and file must remain physically beneath its selected root; a final/intermediate symlink escape is rejected. Package-manager roots are read-only. Only `<config>/plugins/installed` is writable.

Package layout is exactly `<root>/<plugin-id>/<canonical-semver>/plugin.json`. IDs are lowercase ASCII 1–128 bytes matching `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`; plugin IDs have at least two labels and must not start `core.`, `github.`, or `local.`. Canonical SemVer is ASCII `MAJOR.MINOR.PATCH` plus optional dot-separated prerelease/build identifiers under SemVer 2.0.0; leading zero numeric identifiers, missing components, whitespace, `v` prefix, and normalization-changing spellings are rejected. Selection compares major/minor/patch numerically, then SemVer prerelease precedence; build metadata does not affect precedence but remains part of exact identity, so two versions differing only by build metadata coexist and require exact selection.

## Closed manifest and DTOs

```text
Manifest={manifest_schema:1,id,version,display_name,host_api:{minimum,maximum},
 protocol:1,provider:Provider,config:ConfigSchema?,actions:[Action 0..128],
 panels:[Panel 0..32],routes:[Route 0..32],screens:[ScreenContribution 0..32],defaults:PluginDefaults?}
Provider={mode:None,binaries:{}}|{mode:OneShot|Persistent,binaries:{HostTriple:RelativePath}}
Action={id,label,description,category,contexts:[ID 1..32],arguments:[Field 0..128],
 timeout_seconds:1..600,destructive:bool,confirmation:None|HostBeforeInvoke|ProviderContinuation,
 handler,allowed_outcomes:[NavigateDeclaredRoute|RefreshCurrentResource|Notice|ReplaceOwnedPanel|RequestHostConfirmation|CloseOwnedPanel 0..6]}
Panel={id,model_kinds:[List|Detail|Form|Status|Progress|Empty|Error 1..7],
 event_kinds:[Selected|Activated|Action|FieldChanged|Submit|PageRequested|Retry|Cancel|LinkSelected 0..9],handler,ports:[Port 0..32]}
Route={id,activation_fields:[Field 0..32],target_screen}
ScreenContribution={path:RelativePath,screen_ids:[ID 1..32]}
ConfigSchema={schema_version:u32>=1,fields:[Field 0..128]}
Field={id,kind:Boolean|String|Integer|FiniteNumber|Enum|Path|StringList|SecretReference,
 required:bool,default?,minimum?,maximum?,choices:[Scalar 0..64],visible_when?,restart:None|Provider|Host}
SecretReference={env:string matching [A-Z_][A-Z0-9_]{0,127}}
PluginDefaults={actions_enabled:[ID],screens_enabled:[ID],config:ConfigObject}
```

Serialized JSON uses lower-kebab-case enum strings (`one-shot`, `host-before-invoke`, `field-changed`, `secret-reference`). Every object is closed and rejects duplicate keys. `provider=None` forbids handlers and binaries. Binary keys are exact build host triples; no matching triple means visible `Unsupported platform` and zero execution. Owner IDs must start `<plugin-id>.`; references must resolve; every contributed screen ID is bound exactly once; visibility graphs are acyclic; finite numbers reject NaN/infinity; handshake cannot add declarations. Static validation returns immutable declarations or diagnostics and never starts a binary.

## Archive schema, limits, and transaction

Accepted input is one gzip stream containing one POSIX ustar/pax tar archive. Reject concatenated gzip members, trailing bytes, encrypted/non-gzip input, checksum failure, sparse files, hard/symbolic links, devices, FIFO/socket, global pax headers, GNU extensions, absolute paths, backslash separators, NUL, empty/`.`/`..` components, duplicate normalized paths, case-fold duplicates on a case-insensitive target, path depth over 16, path over 1,024 bytes, more than 4,096 entries, or expanded regular-file bytes over 67,108,864. Each individual manifest/resource is at most 1,048,576 bytes. Header-declared size is checked before body read; streaming cumulative bytes are checked before each write. Unexpected EOF or limit failure deletes only the uncommitted staging directory.

Archive root contains exactly one directory `<plugin-id>-<canonical-semver>/`; its `plugin.json` identity must match. Stage in a unique mode-0700 directory beneath `<config>/plugins/.staging`; create directories/provider binaries/resources as 0755/0755/0644, clearing setuid/setgid/sticky and ignoring archive ownership/timestamps. Validate every byte and compute SHA-256 before rename. Destination must not exist. Fsync regular files and staging directories, atomically rename to `<config>/plugins/installed/<id>/<version>`, then fsync parents. A failure before rename leaves installed state unchanged; a failure after rename but before final parent sync reports indeterminate `PLG-E503`, rescans physical inventory, and never overwrites.

Directory developer install accepts only `plugin install DIR --developer`; it applies identical containment/schema/mode/hash checks, copies to staging, and never trusts source symlinks.

## CLI, persistence, and trust

| Syntax | Exit and exact behavior |
|---|---|
| `jefe plugin list` | 0; provider-free sorted ID then SemVer precedence descending then exact version bytes |
| `jefe plugin inspect ID [--version VERSION]` | 0 selected/exact; 2 invalid/not found; 3 ambiguity |
| `jefe plugin install ARCHIVE [--enable]` | 0 committed; 2 invalid; 3 identity/version conflict; 4 filesystem |
| `jefe plugin install DIR --developer [--enable]` | same; directory without `--developer` is usage 64 |
| `jefe plugin enable ID [--version VERSION]` | 0 save; 2 invalid/not found; 3 ambiguous; 4 write |
| `jefe plugin disable ID` | same exits; preserves config/version as dormant selection |
| `jefe plugin rollback ID --version VERSION` | select installed exact version; same exits |
| `jefe plugin remove ID --version VERSION` | 0 only if unselected/disabled; 2 not found/enabled; 4 filesystem |

Install defaults disabled. `--enable` and Settings Save display and persist explicit trust: the provider will execute unsandboxed as the OS user after restart or invocation. Save static-validates the complete candidate, stores exact version/config through the lossless writer, and executes zero provider processes. Versions are side-by-side; there is no network/update command. Removing an enabled/selected version changes nothing. Unknown/disabled/absent owner syntax remains dormant and byte-preserved.

## UI states

Keys: Settings `,`; `j/k` select; Enter inspect; Space toggle trust draft; `i` install-path flow; `v` exact version chooser; `r` rollback; Delete remove; `s` Save; q/Esc dirty guard; Tab/Shift-Tab modal; Ctrl-Q exit.

**Normal**
```text
+ Plugins -----------------------------+
|  Git Merger 1.0.0  installed disabled|
|  versions: 1.0.0, 0.9.0             |
+ Space Trust  Enter Inspect ----------+
```
**Focused**
```text
+ Plugins -----------------------------+
|> Git Merger 1.0.0 disabled           |
|  root: user; provider not started     |
+--------------------------------------+
```
**Unavailable**
```text
+ Plugins -----------------------------+
| BadPkg 2.0.0 Unsupported platform    |
| no binary for aarch64-apple-darwin    |
+--------------------------------------+
```
**Error**
```text
+ Plugins -----------------------------+
| Dup 1.0.0 Ambiguous PLG-E501         |
| two physical package paths [Details] |
+--------------------------------------+
```
**Dirty**
```text
+ Trust and save? ---------------------+
| Provider runs unsandboxed as you.    |
| [Save] [Discard] [Cancel]            |
+--------------------------------------+
```
**Recovery**
```text
+ Broken selected package -------------+
| provider processes started: 0        |
| [Disable] [Rollback] [Inspect]        |
+--------------------------------------+
```
**Small terminal**
```text
+Plugins--------+
|>Git Merger    |
| disabled      |
| q Back Ctrl-Q |
+---------------+
```

## Failure table

| Failure | Durable result | Recovery |
|---|---|---|
| root alias | one physical row plus alias provenance | none |
| distinct ID/version collision | no selection/publication | remove one package offline |
| malformed/unsupported unselected package | listed unavailable; other packages publish | inspect/fix/remove |
| archive validation/write before rename | installed tree unchanged | correct archive and retry |
| final fsync indeterminate | rescan; never overwrite | inspect physical result then retry only if absent |
| settings hash/write failure | disk and draft retained; zero provider starts | Reload/Export/Retry |

## Test-first criterion ledger

| ID | Singular EARS criterion | Scenario | Test | Fixture evidence |
|---|---|---|---|---|
| CW09-01 | WHEN roots scan, Jefe shall list every physical version in exact root order. | `plugin-installed-inventory.json` | `package_root_order` | macOS/Linux trees, missing roots, aliases and expected provenance |
| CW09-02 | WHEN a valid archive installs, Jefe shall atomically normalize it into the user root. | `plugin-install-archive.json` | `archive_transaction` | exact tar.gz bytes, SHA-256, modes and fsync/rename phase captures |
| CW09-03 | WHEN install omits `--enable`, Jefe shall leave the package disabled. | `plugin-install-disabled.json` | `disabled_default` | settings before/after and provider invocation count zero |
| CW09-04 | WHEN trust saves, Jefe shall persist exact version/config and execute zero providers. | `plugin-enable-provider-free.json` | `static_enable` | hanging provider executable plus process count zero |
| CW09-05 | WHEN aliases identify one package, Jefe shall retain only the first physical occurrence and record aliases. | `plugin-cellar-link-dedup.json` | `physical_dedup` | Cellar/prefix inode tree |
| CW09-06 | IF distinct packages share ID/version, Jefe shall emit `PLG-E501` and select neither. | `plugin-package-ambiguity.json` | `package_ambiguity` | two physical packages with byte-equal and byte-different variants |
| CW09-07 | IF an unselected package is malformed or unsupported, Jefe shall list its reason without blocking valid packages. | `plugin-broken-unsupported.json` | `unselected_failure_isolation` | schema 9, no host triple, valid neighbor |
| CW09-08 | WHEN rollback selects an installed exact version, Jefe shall publish that version after restart. | `plugin-version-rollback.json` | `version_selection` | release/prerelease/build precedence table |
| CW09-09 | IF remove targets selected/enabled version, Jefe shall leave files/settings unchanged. | `plugin-remove-enabled-rejected.json` | `remove_transaction` | byte/hash/mode snapshots before/after |
| CW09-10 | WHEN declarations parse, Jefe shall bind every owner/reference exactly once. | `plugin-manifest-declarations.json` | `manifest_contract` | one instance of every closed DTO field and expected lowered IDs |
| CW09-11 | IF unknown/duplicate/owner/reference/bound validation fails, Jefe shall reject the selected manifest. | `plugin-manifest-negative.json` | `manifest_negative` | one exact JSON fixture per rule at limit and limit plus one |
| CW09-12 | IF an archive contains any forbidden entry/path/header/limit, Jefe shall reject before publication. | `plugin-adversarial-archives.json` | `archive_negative` | each forbidden tar/gzip case and unchanged installed-tree hash |
| CW09-13 | WHEN Plugins renders, Jefe shall expose normal, focus, unavailable, error, dirty, recovery, and small states without color-only cues. | `plugin-settings-all-states.json` | `plugin_ui_goldens` | exact frame blocks represented above at 100x30 and 20x8 |

## Normative documentation updated

* `dev-docs/standards/architecture.md`: package inventory ownership and provider-free static phase.
* `dev-docs/standards/persistence-and-runtime.md`: roots, physical identity, archive transaction, settings selection/trust, modes, recovery.
* `dev-docs/standards/display-and-ui.md`: Plugins inventory/trust/disabled/error/recovery states.
* `dev-docs/standards/testing-and-quality.md`: adversarial archive, physical alias, zero-process and atomic phase tests.
* `dev-docs/decisions/plugin-package-dependencies.md`: approved exact dependency decision required by the entry gate.
* `docs/technical-overview.md`: package discovery, selection, static composition, and restart flow.

## Definition of done

The maintainer decision record is complete and approved; all thirteen criteria pass; archive/SemVer/SHA-256 behavior is provided only by approved exact dependencies; all static commands and Settings paths start zero providers; roots, aliases, ambiguity, modes, trust, limits, and lossless persistence are exact. `make ci-check` remains unchanged; no shell command, home-grown cryptography/archive codec, suppression, unsafe, unwrap/expect, or gate weakening exists.