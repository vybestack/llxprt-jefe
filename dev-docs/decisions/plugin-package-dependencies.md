# Decision: dependencies for plugin package inventory and archive install

Required by the entry gate of issue #389 (CW-09: package roots, manifest
inventory, archive install, and explicit trust).

| Field | Value |
|---|---|
| Approval date | 2026-08-05 |
| Approver | `acoliver` (repository maintainer) |
| Approval discussion | https://github.com/vybestack/llxprt-jefe/issues/389#issuecomment-5051771285 |
| Toolchain at approval | rustc 1.97.1 (8bab26f4f 2026-07-14), edition 2024 |
| Advisory scan | `cargo deny`/RUSTSEC review performed 2026-08-05; results per crate below |

## 1. Summary of the decision

CW-09 named four capabilities as requiring a dependency decision: canonical
Semantic Versioning, streaming gzip, tar decoding, and SHA-256. Three of the
four are **satisfied by an already-present safe implementation**, which the
gate's own escape clause permits provided the implementation is named. Only
gzip and tar require new crates.

| Capability | Decision |
|---|---|
| Canonical SemVer parse and precedence | **No new dependency.** Use `crate::domain::CanonicalSemver`. |
| SHA-256, one-shot and streaming | **No new dependency.** Use `crate::domain::sha256`. |
| Streaming gzip | **Approve `flate2` 1.1.9**, backend pinned to `rust_backend`. |
| Tar decoding | **Approve `tar` 0.4.46**, default features off. |
| Process-group helper for CW-10 | **Reject as redundant.** |

## 2. Capabilities satisfied in-tree

### 2.1 Canonical SemVer — `crate::domain::CanonicalSemver`

Defined in `src/domain/config_contract.rs`. It implements SemVer 2.0.0 exactly
as CW-09 specifies:

- rejects leading-zero numeric identifiers, missing components, whitespace, a
  `v` prefix, empty prerelease or build sections, and repeated `+`;
- `precedence_cmp` compares major/minor/patch numerically and then prerelease
  precedence, **excluding** build metadata;
- equality and `as_str` retain the exact original bytes, so two versions
  differing only by build metadata have equal precedence yet remain distinct
  identities that coexist and require exact selection.

That is the whole of the CW-09 rule. It is also already the version type used by
`OwnerCatalog`/`OwnerDescriptor`, so adding the `semver` crate here would create
a second semantic-version type for the same concept — a parallel architecture
variant that `dev-docs/standards/architecture.md` forbids. Proof of behaviour:
`src/domain/plugin/coordinate_tests.rs`.

Note: `semver 1.0.27` is already present transitively (via the
`wasm-metadata`/`wit-parser` chain). Making it direct would not add a
supply-chain party, but it would add a competing domain type, which is the
reason for rejection.

### 2.2 SHA-256 — `crate::domain::sha256`

Defined in `src/domain/sha256.rs`: a safe, dependency-free implementation that
is already the digest for persistence wire contracts. CW-09 additionally needs a
*streaming* digest so a 64 MiB archive is never buffered whole in memory merely
to be hashed; `Sha256Hasher` provides `update`/`finalize` over the same single
compression implementation.

This is an extension of an existing in-tree primitive, not a new home-grown one.
The module is pinned to the published FIPS 180-4 / RFC 6234 known-answer vectors
and to streaming/one-shot equivalence across chunk sizes straddling the 64-byte
block and the 56-byte padding threshold: `src/domain/sha256_tests.rs`.

### 2.3 Process-group helper for CW-10 — rejected as redundant

`src/runtime/command_capture.rs` already spawns into its own process group via
`std::os::unix::process::CommandExt::process_group` and signals `-PGID`;
`nix` and `libc` are already in the lockfile, and Windows job-object needs are
covered by the existing `winsafe` and `win32job` dependencies. `command-group`
and `process-wrap` are therefore rejected. This issue starts no provider
process at all.

## 3. Approved dependencies

### 3.1 `flate2` 1.1.9 — streaming gzip

| Fact | Value |
|---|---|
| Exact version / checksum | `1.1.9` / `843fba2746e448b37e26a819579957415c8cef339bf08564fe8b7ddbd959573c` |
| SPDX license | `MIT OR Apache-2.0` |
| Repository | https://github.com/rust-lang/flate2-rs |
| MSRV | 1.67.0 |
| Direct runtime deps (as configured) | 2 — `miniz_oxide`, `crc32fast` |
| Transitive additions | `adler2`, `simd-adler32` |
| Advisory scan | no current advisories (2026-08-05) |
| Maintenance | maintained under the `rust-lang` GitHub organisation; ~572M downloads, ~5.8k dependents |
| Platforms | pure Rust on the pinned backend; all tier-1 targets, no C toolchain |

The backend is pinned with `default-features = false, features =
["rust_backend"]`. flate2 documents that its default backend may one day become
`zlib-rs`, which uses `unsafe`; pinning `miniz_oxide` keeps the decode path
100% safe Rust so the crate-level `unsafe_code = "forbid"` posture is preserved.

### 3.2 `tar` 0.4.46 — tar decoding

| Fact | Value |
|---|---|
| Exact version / checksum | `0.4.46` / `3f6221d9a6003c78398e3b239969f352578258df48c8eb051caadae0015bc840` |
| SPDX license | `MIT OR Apache-2.0` |
| Repository | https://github.com/containers/tar-rs |
| MSRV | 1.63 |
| Direct runtime deps (as configured) | 1 — `filetime` |
| Transitive additions | none beyond `filetime` (default `xattr` feature disabled) |
| Advisory scan | RUSTSEC-2026-0067 and RUSTSEC-2026-0068 both fixed in 0.4.45; 0.4.46 is the current patched line and carries no open advisory (2026-08-05) |
| Maintenance | maintained in the composefs/containers organisation (Colin Walters) specifically to keep it maintained; ~192M downloads, ~2.6k dependents; the crate Cargo itself uses for package extraction |
| Platforms | all tier-1 targets including Windows |

Default features are disabled, which drops `xattr`. The install transaction
ignores archive ownership, timestamps, and extended attributes and writes
explicit modes, so reading xattrs would be dead weight on the trusted path.

Both 2026 advisories concern `unpack`/`unpack_in`, the convenience extraction
path. CW-09 never calls it. The contract mandates iterating `Archive::entries`,
validating every header directly — rejecting links, devices, sparse files,
absolute paths, duplicates, excess depth and size — and writing regular files
through jefe's own contained staging writer with explicit modes. The vulnerable
path is out of scope by construction rather than by version alone.

### 3.3 Exact manifest and lockfile entries

`Cargo.toml`:

```toml
flate2 = { version = "1.1", default-features = false, features = ["rust_backend"] }
tar = { version = "0.4.46", default-features = false }
```

`Cargo.lock` additions:

| Package | Version | Checksum |
|---|---|---|
| `flate2` | 1.1.9 | `843fba2746e448b37e26a819579957415c8cef339bf08564fe8b7ddbd959573c` |
| `tar` | 0.4.46 | `3f6221d9a6003c78398e3b239969f352578258df48c8eb051caadae0015bc840` |
| `miniz_oxide` | 0.8.9 | `1fa76a2c86f704bdb222d66965fb3d63269ce38518b83cb0575fca855ebb6316` |
| `crc32fast` | 1.5.0 | `9481c1c90cbf2ac953f07c8d4a58aa3945c425b7185c9154d67a65e4230da511` |
| `adler2` | 2.0.1 | `320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa` |
| `simd-adler32` | 0.3.10 | `3a219298ac11a56ea9a6d2120044824d6f01aeb034955e7af7bc16858527deea` |
| `filetime` | 0.2.29 | `5c287a33c7f0a620c38e641e7f60827713987b3c0f26e8ddc9462cc69cf75759` |

## 4. Rejected alternatives

### 4.1 Implementing gzip, tar, SemVer, or SHA-256 locally

Rejected for gzip and tar. Archive-format parsing is precisely where a
home-grown implementation creates silent vulnerabilities: RUSTSEC-2026-0068 is
itself a PAX header parser-divergence bug, which is the failure mode a fresh
ustar/pax/DEFLATE implementation would be most likely to reproduce. A correct
and complete implementation would also be large enough that keeping it within
the repository's complexity and source-size gates would require either
suppressing those gates or splitting the codec across many modules for reasons
unrelated to the domain.

SemVer and SHA-256 are a different case and are *not* rejected: they are already
implemented in-tree, reviewed, and tested, so using them is reuse rather than a
new home-grown primitive.

### 4.2 Invoking external `tar`, `gzip`, or `openssl`

Rejected. Shelling out violates the repository's no-arbitrary-shell rule. Beyond
that, host binaries diverge — bsdtar on macOS, GNU tar on Linux, frequently
neither in a minimal container — which reintroduces the exact parser-divergence
vulnerability class the decision above is trying to avoid, and makes behaviour
depend on the user's `PATH`. An argv-only policy would still not bound expanded
bytes or filter entries without reading the archive ourselves, so the external
process would buy nothing while costing determinism.

## 5. Testability of the required rejections (gate item 7)

Each condition CW-09 requires to be provable is reachable through the selected
APIs:

| Required proof | Mechanism |
|---|---|
| Concatenated gzip members / trailing bytes | `GzDecoder` decodes a single member; the reader then asserts the underlying stream is exhausted. `MultiGzDecoder` is deliberately not used, since it would silently accept appended members. |
| Duplicate archive paths, case-fold duplicates | `Entry::path_bytes` per entry, normalized and accumulated in a set before any write. |
| Links, devices, FIFO/socket, sparse, GNU extensions, global pax headers | `Entry::header().entry_type()` exposes each type distinctly, and `link_name_bytes` exposes link targets, so each is a distinct rejection. |
| Header-declared size before body read | `Header::size()` is available before the entry body is streamed. |
| Streaming expanded-byte bounds | `Entry` implements `Read`, so cumulative bytes are checked before each write rather than after a full expansion. |
| PAX size-header divergence | tar 0.4.45+ honours PAX size overrides consistently with other parsers, so a fixture whose PAX size disagrees with its ustar size is rejected rather than interpreted differently by jefe and by the authoring tool. |
| Canonical SemVer rejection | `CanonicalSemver::parse` returns `InvalidSemver` for each non-canonical spelling. |
| Digest known-answer vectors | `Sha256::digest` and `Sha256Hasher` are pinned to the published FIPS 180-4 / RFC 6234 vectors. |

## 6. Constraints this decision does not relax

Adding these crates does not permit `unsafe` in jefe, does not raise any
complexity, coverage, or source-size threshold, and does not introduce any lint
suppression. The archive path remains subject to every gate in
`dev-docs/standards/testing-and-quality.md`.
