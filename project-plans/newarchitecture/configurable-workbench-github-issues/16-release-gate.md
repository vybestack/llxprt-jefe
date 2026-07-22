# CW-15: Final aggregation, installed layouts, and release gate

## Outcome and consumed contracts

Aggregate unchanged owner fixtures against clean installed macOS/Homebrew and Linux layouts. Consume every delivered capability, the exact Git Merger package, ownership guards, and author-kit hashes. This issue invents no contract, migration, fixture expectation, parser, retry, waiver, or user-data owner; a defect returns to its owner.

## Exact source/artifact responsibility inventory

| Source/path | Required aggregation responsibility |
|---|---|
| `.github/workflows/ci.yml` | continue unchanged source quality/test gates |
| `.github/workflows/release.yml` | build locked artifacts, install synthetic layouts, run release indexes and scans |
| `Cargo.toml` and `Cargo.lock` | locked feature-complete build; no release-only dependency |
| `Makefile::ci-check` | unchanged local/CI quality command |
| release asset/resource list | include binary, author kit, package resources, docs with recorded hashes/modes |
| `tests/fixtures/release/homebrew/` | exact Cellar/prefix/share/config/state synthetic tree |
| `tests/fixtures/release/linux/` | exact `/usr/local` and `/usr/share` synthetic tree |
| `tests/e2e/release.rs` | index validation, install launch/restart/recovery/secret/orphan/path scans |
| owner fixture indexes | immutable criterion/path/hash source; release never regenerates expected bytes |
| `docs/building.md` and `docs/getting-started.md` | installed locations, verification, recovery commands |

## Closed manifest and installed layouts

```text
ReleaseIndex={release_schema:1,artifact:{path,sha256,mode},platform:"macos"|"linux",
 resources:[{path,sha256,mode}],owner_fixtures:[{owner,criterion,path,sha256}],
 installed_layout:InstalledLayout,expected_exit:u8}
InstalledLayout={executable:AbsolutePath,share_root:AbsolutePath,config_root:AbsolutePath,
 state_root:AbsolutePath,plugin_roots:[AbsolutePath],author_kit_root:AbsolutePath}
```

Objects reject duplicate/unknown keys; records are unique and sorted by path then owner/criterion. Every delivered singular criterion appears exactly once; duplicate/missing/hash mismatch blocks release. Modes are octal integers 420, 448, or 493. Hash is lowercase SHA-256 of exact bytes.

Homebrew fixture installs executable at `/opt/homebrew/Cellar/jefe/1.0.0/bin/jefe`, resources at `/opt/homebrew/Cellar/jefe/1.0.0/share/jefe`, and prefix symlinks `/opt/homebrew/bin/jefe` and `/opt/homebrew/share/jefe`; physical Cellar resource identity is enumerated once. Plugin root order is user config plugins, prefix share plugins, Cellar share alias deduplicated physically. Linux fixture installs `/usr/local/bin/jefe`, `/usr/local/share/jefe`, and `/usr/share/jefe`; plugin order is user config, `/usr/local/share/jefe/plugins`, `/usr/share/jefe/plugins`. Author kit is `<share>/jefe/author-kit/v1`; Git Merger is `<share>/jefe/plugins/com.example.git-merger/1.0.0`. No repository-relative fallback or source path.

## Exact aggregation matrix

| Owner capability | Required release evidence |
|---|---|
| harness | real PTY, exact capture/interpolation/resize/restart/cleanup/redaction and every converted schema-1 scenario |
| configuration/state/effects | v1-to-v2, lossless hash/write/path/ambiguity/malformed provider-free recovery, stale effect |
| agents | all four pinned provenance/capability local+remote operation/target/preflight/plan/signature/generation cases |
| actions/keys | complete default inventory, contexts, protected keys, conflict/availability parity |
| descriptors/layout | five-screen normal/focus/unavailable/error/recovery/tiny parity and one geometry snapshot |
| custom screens/navigation | discovery/lowering/relationships, route generations, Back precedence and dirty choices |
| Settings/editors | draft/hash/reload/export and General/Appearance/Diagnostics/Agent/Screen/Key state matrices |
| package inventory | ordered roots, physical dedup/ambiguity, static validation/trust/archive/modes |
| providers | every wire payload/order/bound/environment, zero one-shot startup, persistent rollback, cleanup/confirmation |
| panels/config | every body/event/lifecycle/config field/migration/dormancy/secret state |
| Git Merger | relocation, exact view/merge argv, head invariant, cancel, no auto retry, dormancy |
| ownership audit | dependency guards and stale generation/effect-order permutations |
| author kit | all hashes, canonical/invalid/boundary/transcript examples from installed path |

All owner limits remain exact; release does not restate alternatives. Build uses locked dependencies and artifacts. Flow: build clean artifact; hash it; install synthetic tree; set contained HOME/PATH/config/state/plugin roots; run installed author kit; run owner fixtures; launch installed binary in real PTY at normal and tiny sizes; exercise terminal capture, Settings save/restart, Git Merger; run malformed-state recovery with a hanging provider trap; scan installed files/reports/logs/process table for secret, orphan, source/development path; compare durable files; emit release index report.

Any warning, contradiction, missing mapping, unapproved dependency, weakened gate, lint suppression, guessed external capability, product/plugin branch, compatibility shim/legacy adapter/dual code path or shim-token permutation outside the one-way persistence migration allowlist, surviving superseded symbol (`AgentKind`, `ScreenMode`, pre-registry dispatch/help/footer maps, schema-1 load/save outside migration, old-format harness parsing), secret, orphan, one-shot startup process, duplicate authority, source path, inaccessible recovery, or owner-fixture mutation blocks release. No waiver.

## Migration, recovery, and security

Release performs no new migration. It verifies owner migrations on copies in contained homes and preserves originals on failure. Recovery commands are exactly installed `jefe config path`, `validate`, `show-effective`, and `migrate-state`; with malformed state and a selected hanging provider they start zero provider/TUI processes. Artifacts are untrusted until hashes/modes/index pass. Environment is explicit, secrets are synthetic sentinels, no network credential is used, and every descendant is reaped.

## Distinct installed UI states

```text
NORMAL                         FOCUSED
+ Jefe installed -----------+ + Pull Requests ------------+
| Repositories  PRs        | |>>PR 42 Detail            |
| Git Merger available     | | Actions focused           |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Agent Types --------------+ + Provider -----------------+
| Claude incompatible      | | PLG-E502 bad envelope    |
| reason: missing resume   | | [Retry]                  |
+---------------------------+ +---------------------------+
```
```text
DIRTY                          RECOVERY
+ Save changes? ------------+ + Startup blocked ---------+
|>>Save  Discard  Cancel    | | CFG-E103; providers 0   |
+---------------------------+ | config validate command |
                               +---------------------------+
```
```text
SMALL
+Too small------+
|>>Back         |
| F12 terminal  |
| Ctrl-Q Exit   |
+---------------+
```

Normal, focused, unavailable, error, dirty, recovery, and small are separate installed runs on both platforms.

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW15-01 | WHEN a release index loads, it shall map every owner criterion exactly once with matching hash. | missing/duplicate/mutated index tests |
| CW15-02 | WHEN Homebrew layout installs, Jefe shall deduplicate Cellar/prefix identities and use installed resources. | relocated Homebrew scenario |
| CW15-03 | WHEN Linux layout installs, Jefe shall use exact ordered user/local/system roots. | relocated Linux scenario |
| CW15-04 | WHEN each compatible shipped CLI is installed without config, Jefe shall expose its pinned definition. | four executable fixtures |
| CW15-05 | WHEN aggregate runs, it shall execute unchanged owner expectations. | read-only fixture/hash guard |
| CW15-06 | IF startup is malformed, installed recovery shall start zero providers/TUI and preserve bytes. | hanging provider capture |
| CW15-07 | WHEN installed normal/tiny PTYs render, protected capture/Back/exit shall remain reachable. | two-platform UI state matrix |
| CW15-08 | WHEN Git Merger runs installed, it shall execute exact commands and remain relocatable. | package command/path captures |
| CW15-09 | WHEN artifacts/reports/processes are scanned, release shall contain no secret/orphan/development path. | exhaustive scan |
| CW15-10 | WHEN quality gates run, release shall use unchanged thresholds and locked all-feature artifacts. | workflow command assertion |
| CW15-11 | WHEN the release shim scan runs across source and installed artifacts, it shall find no shim-token permutation, deprecated re-export, compatibility delegate, dual code path, or superseded symbol outside the one-way persistence migration allowlist. | full-tree token-permutation scan, superseded-symbol absence assertions, and audited allowlist diff against the ownership-audit baseline |

RED indexes/layouts with deliberate missing hashes first; GREEN aggregation wiring only; REFACTOR runner invocation without changing expectations.

## Normative documentation and done

Update `docs/building.md`, `docs/getting-started.md`, `dev-docs/standards/testing-and-quality.md`, and release workflow comments with exact installed layouts, hash/index checks, owner aggregation, provider-free recovery, relocation/security scans, the shim scan, and no-waiver policy. Done requires all owner criteria exactly once on both clean layouts, a clean CW15-11 shim scan, and unchanged `make ci-check`: rustfmt; no clippy allows; source hard 1,000/warn 750; clippy all targets/features `-D warnings`; complexity 15 cognitive/60 lines/6 args/3 bools/type 250; coverage at least 30%; locked all-feature build/test. No unsafe, production unwrap/expect, arbitrary scenario shell, unapproved dependency, compatibility shim, warning, or waiver.