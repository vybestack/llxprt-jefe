# Issue #389 — CW-09: Package roots, manifest inventory, archive install, and explicit trust

Branch: `issue389` (from `origin/main` @ `0b73a19a`).

## 1. Outcome

Jefe gains a **provider-free** plugin package subsystem. Ordered package roots are
scanned into an immutable inventory of physically distinct `(plugin id, canonical
version)` packages; a `tar.gz` archive can be validated and normalized atomically
into the user root; packages stay **disabled until explicit trust**; the exact
selected version and config persist losslessly through the CW-01 writer; and a
restart publishes **static declarations only**.

This issue starts **zero provider processes**, adds **no update command**, and
performs **no network access**.

## 2. Consumed owner contracts (prerequisite gate)

| Contract | Owning module | Status |
|---|---|---|
| Path authority / config root resolution | `src/persistence/paths.rs` (`ResolvedPaths.plugins` already resolves `<config>/plugins`) | present, **extended** (roots + physical identity) |
| Lossless settings document + sparse patch | `src/persistence/settings_document.rs` (`patch_assignment`), `src/persistence/settings_edit.rs` (`SyntaxPath`, `SettingsEdit`, `SettingsCandidate`) | present, **must be extended** |
| Settings publisher, `plugins.<owner>` owner root | `src/persistence/settings_publish.rs:112` (`parse_owner_root(.., OwnerKind::Plugin, ..)`) | present |
| Owner catalog / owner kinds | `src/config_owners.rs`, `src/domain/config_contract.rs` (`OwnerCatalog`, `OwnerDescriptor`, `OwnerKind::Plugin`) | present, **extended** (plugin owners registered from inventory) |
| Validated identifier | `src/domain/config_contract.rs::Id` (lowercase ASCII, `[a-z][a-z0-9]*([.-][a-z0-9]+)*`, ≤128 bytes) | present |
| Canonical SemVer + precedence | `src/domain/config_contract.rs::CanonicalSemver` | present |
| SHA-256 | `src/domain/sha256.rs::Sha256` | present, **extended** (incremental `update`/`finalize`) |
| Settings draft reducer / dirty lifecycle | `src/state/settings.rs::reduce_settings`, `src/state/settings_types.rs::SettingsDraft` | present, **must be extended** |
| Settings shell UI | `src/ui/screens/settings.rs` | present, **must be extended** |
| Harness schema 1 | `src/harness/v1/`, `dev-docs/tmux-scenarios/` | present |
| Closed-contract bounded JSON reader | `src/domain/agent_definition/bounded_json.rs` | present, **generalized** (see §4 D1) |
| Streaming gzip + tar decoding | — | **ABSENT — entry gate, see §3** |

## 3. Entry gate: the mandatory dependency decision record

The issue requires `dev-docs/decisions/plugin-package-dependencies.md`, maintainer
approved, before RED. Research resolves the gate's nine items as follows.

### 3.1 Items already satisfied by an already-present safe implementation

The gate's own escape clause admits approval that "names an already-present safe
implementation". Three of the four decision items qualify, so they require **no
new dependency**:

| Gate item | Already-present implementation | Why it satisfies CW-09 exactly |
|---|---|---|
| 2 — canonical SemVer parse/order | `crate::domain::CanonicalSemver` | Strict SemVer 2.0.0. Rejects leading-zero numeric identifiers, missing components, `v` prefix, whitespace, empty identifiers, repeated `+`. `precedence_cmp` compares major/minor/patch numerically then prerelease precedence and **excludes build metadata**; `as_str()` retains the exact original bytes, so two versions differing only by build metadata coexist and require exact selection — the precise CW-09 rule. It is already the version type in `OwnerCatalog`/`OwnerDescriptor`, so a second SemVer type would be a forbidden parallel architecture variant. |
| 4 — SHA-256 | `crate::domain::sha256::Sha256` | Safe, dependency-free, already the persistence wire-contract digest. Only exposes one-shot `Sha256::digest(&[u8])`; CW-09's bounded streaming digest needs an incremental `update`/`finalize` pair added to that same module. That is an extension of an existing in-tree implementation, **not** a new home-grown primitive. |
| 8 — process-group helper (for CW-10) | `src/runtime/command_capture.rs` + `nix`/`libc`/`winsafe`/`win32job` already in the lock | Process-group spawn and `-PGID` signalling already work. Record `command-group`/`process-wrap` as **rejected — redundant**. This issue starts no provider regardless. |

### 3.2 The one real decision: streaming gzip + tar

Nothing in `std`, in the existing dependencies, or in `vendor/` can decode gzip or
tar. Proposed (matching the maintainer's researched recommendation on #389):

```toml
flate2 = { version = "1.1.9", default-features = false, features = ["rust_backend"] }
tar = "0.4.46"
```

- Both `MIT OR Apache-2.0`; pure safe Rust on the pinned `rust_backend`, so our
  `unsafe_code = "forbid"` posture is unchanged and no C toolchain is required.
- All tier-1 targets including Windows.
- `flate2`'s `GzDecoder` is single-member, which is what makes "reject concatenated
  gzip members / trailing bytes" testable; `MultiGzDecoder` must not be used.
- `tar` 0.4.46 is past RUSTSEC-2026-0067 / RUSTSEC-2026-0068 (both fixed in
  0.4.45). CW-09 never calls the advisories' `unpack`/`unpack_in` path: it
  iterates `Archive::entries`, validates every header itself, and writes through
  its own contained staging writer.
- New lock entries: `flate2`, `miniz_oxide`, `adler2`, `crc32fast`, `tar`,
  `filetime`.

### 3.3 Gate status

**BLOCKED — awaiting maintainer approval of §3.2.** Dependency-manifest changes
require explicit approval under `dev-docs/workflow/ISSUE-DELIVERY.md` §2
independently of the issue's own gate. Slice 3 (§5) does not start until the
record is committed and approved. Slices 1, 2, 4, 5 need no new dependency and
proceed.

## 4. Acceptance matrix

Every row names the input and boundary cases, observable success and failure,
side effects permitted before failure, persistence expectations, and the
behavioral test that proves it.

### D. Package domain — identity, closed manifest, static validation (CW09-10, CW09-11)

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| D1 | The closed-contract bounded JSON reader is generalized to carry explicit bounds and a neutral error, and both `agent_definition` and the package domain consume the one reader | depth 16, duplicate key, trailing data, non-UTF-8, oversize artifact | one reader, two bound sets; `agent_definition` diagnostics unchanged | duplicate/unknown/oversize rejected identically to today | existing `agent_definition` bounded-JSON tests stay green; new plugin reader tests |
| D2 | `PluginId` parses lowercase ASCII 1–128 bytes matching `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*` with **at least two labels**, rejecting the `core.`, `github.`, and `local.` prefixes | 1 byte, 128 bytes, 129 bytes, one label, leading digit, trailing separator, double separator, uppercase, non-ASCII, each reserved prefix | typed value; `as_str()` returns the exact bytes | categorized reason variant per rule | limit / limit+1 unit table |
| D3 | Canonical package version is `CanonicalSemver`; selection compares precedence and build-metadata-only variants coexist under exact identity | `1.0.0`, `1.0.0-rc.1`, `1.0.0+a`, `1.0.0+b`, `01.0.0`, `v1.0.0`, `1.0`, `1.0.0 ` | precedence order matches SemVer 2.0.0; `+a` and `+b` are distinct packages | rejected spellings return `InvalidSemver` | precedence table (CW09-08 fixture) |
| D4 | `Manifest` and every nested DTO (`Provider`, `Action`, `Panel`, `Route`, `ScreenContribution`, `ConfigSchema`, `Field`, `SecretReference`, `PluginDefaults`, `Port`) deserialize from the closed schema, rejecting unknown fields and duplicate keys, with lower-kebab-case enum strings | one instance of every field; each enum spelling; `one-shot`, `host-before-invoke`, `field-changed`, `secret-reference` | typed immutable manifest | unknown field / duplicate key / wrong-case enum rejected with the offending name | CW09-10 fixture round trip |
| D5 | Every array bound, numeric range, and enum set is enforced at N and N+1 | actions 128/129, panels 32/33, routes 32/33, screens 32/33, config fields 128/129, contexts 1..32, arguments 0..128, choices 0..64, allowed outcomes 0..6, model kinds 1..7, event kinds 0..9, ports 0..32, `timeout_seconds` 0/1/600/601, `schema_version` 0/1 | at-limit accepted | limit+1 rejected with a bound diagnostic | CW09-11 one exact JSON fixture per rule |
| D6 | Finite numbers reject NaN and infinity; `SecretReference.env` matches `[A-Z_][A-Z0-9_]{0,127}` | `NaN`, `Infinity`, `1e999`, env of 1/128/129 bytes, leading digit, lowercase | accepted values are finite and canonical | rejected with the specific reason | boundary table |
| D7 | Static validation enforces cross-field rules and returns immutable declarations or diagnostics, **never starting a binary** | `provider=None` with handlers/binaries; owner id not prefixed `<plugin-id>.`; dangling reference; a screen id bound twice; visibility cycle; unknown host triple | declarations with every owner and reference bound exactly once | a categorized diagnostic per rule; zero process starts | CW09-10 + CW09-11, process-count assertion |
| D8 | Binary keys are exact build host triples; no matching triple yields a visible `Unsupported platform` with zero execution | exact host triple, near-miss triple, empty map | selected binary path is relative and contained | `UnsupportedPlatform` diagnostic, still listed | CW09-07 |
| D9 | `PLG-Ennn` diagnostics render stable codes through `Display` (`PLG-E501` ambiguity, `PLG-E503` indeterminate final sync) | every variant | code text is exact and stable | — | code golden per variant |

### R. Roots, physical identity, inventory (CW09-01, CW09-05, CW09-06, CW09-07)

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| R1 | The ordered low-to-high roots are exactly: canonical exe `../share/jefe/plugins`; macOS `/opt/homebrew/share/jefe/plugins` then `/usr/local/share/jefe/plugins`, Linux `/usr/local/share/jefe/plugins` then `/usr/share/jefe/plugins`; then `<config>/plugins/installed`. Missing roots are skipped. PATH and cwd are never inspected | all roots present, none present, only user root, each platform | exact ordered list with per-root writability | — | CW09-01, platform matrix |
| R2 | Only `<config>/plugins/installed` is writable; package-manager roots are read-only | write attempt against a system root | rejected before any filesystem mutation | typed read-only diagnostic | unit |
| R3 | Physical identity compares `(device, inode)` where available, otherwise the canonical absolute path; a missing final target uses canonical parent + basename | hardlinked tree, symlinked Cellar/prefix, absent final component, platform without inode | one physical row | — | CW09-05 |
| R4 | The first occurrence of one physical package wins; later aliases are recorded as provenance | Homebrew Cellar + prefix symlink pair | one row plus alias provenance | — | CW09-05 |
| R5 | Two **physically distinct** packages with the same `(plugin id, canonical version)` are ambiguous `PLG-E501` and **neither** is selected; precedence never resolves it | byte-equal duplicates, byte-different duplicates | `PLG-E501`, no selection, no publication | — | CW09-06 |
| R6 | Every selected package and file stays physically beneath its selected root; a final or intermediate symlink escape is rejected | symlink to `/etc`, `..` traversal, intermediate symlink | rejected | typed containment diagnostic | CW09-06 negative |
| R7 | Package layout is exactly `<root>/<plugin-id>/<canonical-semver>/plugin.json`; anything else is not a package | wrong depth, missing manifest, non-canonical version directory, non-directory | ignored or listed unavailable | — | CW09-01 |
| R8 | A malformed or unsupported **unselected** package is listed with its reason and does not block valid packages publishing | `manifest_schema: 9`, no matching host triple, valid neighbour | valid neighbour publishes | unavailable row carries the reason | CW09-07 |
| R9 | The inventory module is provider-free and depends only on the domain layer; it is reusable by `state/` and `ui/` | — | zero process starts during any scan | — | process-capture assertion |

### A. Archive install transaction (CW09-02, CW09-12) — **blocked on §3.3**

| # | Behaviour | Inputs / boundaries | Success | Failure | Evidence |
|---|---|---|---|---|---|
| A1 | Accepted input is one gzip stream containing one POSIX ustar/pax tar archive | single member; concatenated members; trailing bytes; non-gzip; encrypted; CRC failure | accepted | rejected before staging | CW09-12 |
| A2 | Forbidden entries are rejected: sparse, hard link, symlink, device, FIFO, socket, global pax header, GNU extension | one fixture each | — | rejected, installed tree hash unchanged | CW09-12 |
| A3 | Forbidden paths are rejected: absolute, backslash separator, NUL, empty / `.` / `..` component, duplicate normalized path, case-fold duplicate on a case-insensitive target, depth > 16, path > 1,024 bytes | each at limit and limit+1 | at-limit accepted | limit+1 rejected | CW09-12 |
| A4 | Bounds: > 4,096 entries, > 67,108,864 expanded regular-file bytes, any single manifest/resource > 1,048,576 bytes. Header-declared size is checked **before** body read; cumulative streamed bytes are checked **before each write** | at limit and limit+1; header size lying about body size | at-limit accepted | limit+1 rejected mid-stream, staging removed | CW09-12 incl. PAX size-mismatch |
| A5 | Archive root contains exactly one directory `<plugin-id>-<canonical-semver>/` whose `plugin.json` identity matches | zero, one, two root dirs; identity mismatch | accepted | rejected | CW09-02 negative |
| A6 | Staging is a unique mode-0700 directory under `<config>/plugins/.staging`; directories 0755, provider binaries 0755, resources 0644; setuid/setgid/sticky cleared; archive ownership and timestamps ignored | archive with 4777 / setuid entries | exact resulting modes | — | CW09-02 mode capture |
| A7 | Every byte is validated and SHA-256 computed **before** rename. Destination must not exist. Regular files and staging directories are fsynced, then the tree is atomically renamed to `<config>/plugins/installed/<id>/<version>`, then parents are fsynced | destination exists; failure before rename; failure after rename before final parent sync | committed exactly once | before rename: installed tree unchanged, only staging removed. After rename: `PLG-E503` indeterminate, rescan physical inventory, never overwrite | CW09-02 phase capture |
| A8 | Unexpected EOF or any limit failure deletes only the uncommitted staging directory | truncated archive | — | staging gone, nothing else touched | CW09-12 |
| A9 | `plugin install DIR --developer` applies identical containment, schema, mode, and hash checks, copies to staging, and never trusts source symlinks | dir with symlink, dir without manifest | same commit path | same rejections | CW09-02 developer variant |

### C. CLI, persistence, trust (CW09-03, CW09-04, CW09-08, CW09-09)

| # | Syntax | Exit and exact behaviour | Evidence |
|---|---|---|---|
| C1 | `jefe plugin list` | 0; provider-free; sorted by id, then SemVer precedence descending, then exact version bytes | CW09-01 |
| C2 | `jefe plugin inspect ID [--version VERSION]` | 0 selected/exact; 2 invalid or not found; 3 ambiguity | CW09-06 |
| C3 | `jefe plugin install ARCHIVE [--enable]` | 0 committed; 2 invalid; 3 identity/version conflict; 4 filesystem | CW09-02 |
| C4 | `jefe plugin install DIR --developer [--enable]` | same; a directory without `--developer` is usage **64** | CW09-02 |
| C5 | `jefe plugin enable ID [--version VERSION]` | 0 save; 2 invalid/not found; 3 ambiguous; 4 write | CW09-04 |
| C6 | `jefe plugin disable ID` | same exits; preserves config and version as a dormant selection | CW09-03 |
| C7 | `jefe plugin rollback ID --version VERSION` | selects an installed exact version; same exits; that version publishes after restart | CW09-08 |
| C8 | `jefe plugin remove ID --version VERSION` | 0 only if unselected and disabled; 2 not found or enabled; 4 filesystem. Removing an enabled/selected version changes nothing | CW09-09 |
| C9 | Install defaults to **disabled**; `--enable` and Settings Save display and persist explicit trust ("the provider will execute unsandboxed as the OS user after restart or invocation") | | CW09-03 |
| C10 | Save static-validates the complete candidate, stores exact version and config through the lossless writer, and executes **zero** provider processes | | CW09-04 (hanging provider executable, process count 0) |
| C11 | Versions are side by side; unknown, disabled, or absent owner syntax stays dormant and byte-preserved | | CW09-03 lossless golden |

### U. Settings Plugins UI (CW09-13)

| # | Behaviour | Evidence |
|---|---|---|
| U1 | Seven states render without colour-only cues: normal, focused, unavailable, error (`PLG-E501`), dirty trust confirm, recovery (broken selected package, "provider processes started: 0"), small terminal | `plugin-settings-all-states.json` goldens at 100x30 and 20x8 |
| U2 | Keys: `,` Settings, `j`/`k` select, Enter inspect, Space toggle trust draft, `i` install-path flow, `v` exact version chooser, `r` rollback, Delete remove, `s` Save, `q`/Esc dirty guard, Tab/Shift-Tab modal, Ctrl-Q exit | scenario steps |
| U3 | The UI is a pure projection over the immutable inventory plus the draft; it performs no scan, install, write, or process start | architecture gate + process-capture assertion |

## 5. Vertical slices

| Slice | Acceptance rows | Layer owner | Blocked? |
|---|---|---|---|
| 0 | §3 decision record + `Cargo.toml`/`Cargo.lock` + record contract test | docs / manifest | **yes — maintainer approval** |
| 1 | D1–D9 | `src/domain/plugin/` | no |
| 2 | R1–R9 | `src/persistence/` (roots, identity) + provider-free inventory module | no |
| 3 | A1–A9 | install adapter at the persistence boundary | **yes — needs slice 0** |
| 4 | C1–C11 | `src/cli.rs`, composition root, `SyntaxPath`/`SettingsEdit`, `reduce_settings` | partial (C3/C4 need slice 3) |
| 5 | U1–U3 | `src/state/` projection + `src/ui/screens/settings.rs` | no |
| 6 | Normative docs | `dev-docs/standards/*`, `docs/technical-overview.md` | no |

Order of execution: 1 → 2 → 5 → 4 (non-archive) → [0 → 3 → 4 archive] → 6.

## 6. Non-goals (explicit)

- Starting **any** provider process, one-shot or persistent (that is CW-10 / #390).
- Host-rendered panels and plugin configuration migration (CW-11 / #391).
- Any network access, registry, or `plugin update` command.
- Overwriting or upgrading an installed version in place; versions are side by side.
- Sandboxing the provider; trust is explicit and the provider runs unsandboxed.
- Retrofitting existing screens or the action registry to plugin ownership.
- Any change to the harness beyond adding CW-09 scenarios.

## 7. Scope ledger

| # | Discovered work | Disposition |
|---|---|---|
| S1 | `Sha256` has no incremental API; bounded streaming digest needs `update`/`finalize` | **In scope** — required by A7; extends the existing module, adds no dependency |
| S2 | `bounded_json` is hard-coupled to `DefinitionError` and to `agent_definition`'s limits | **In scope (D1)** — generalize once rather than fork a second JSON parser, which would be a forbidden parallel variant |
| S3 | `flate2` + `tar` dependency addition | **Blocked** — maintainer approval (§3.3) |
| S4 | Plugin owners must reach `OwnerCatalog` for `plugins.<id>` settings to validate | **In scope** — required by C10/C11 |
| S5 | Anything touching `.github/`, quality-gate scripts, or unrelated tests | **Out of scope** — follow-up issue |

## 8. Review counters

| Review | Budget | Used |
|---|---|---|
| Local OCR (pre-PR) | 2 | 0 |
| PR OCR | 2 | 0 |
| Subagent design/code review | 2 cycles total | 0 |

## 9. Verification

Per green checkpoint: `cargo xtask quick`.
Before push: `cargo xtask ci` (fmt, strict clippy, complexity, coverage floor,
clippy-allow policy, source-size policy, architecture policy, locked build,
locked test) on the exact candidate head.
