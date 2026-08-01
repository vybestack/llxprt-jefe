# Issue #556 — Serialize managed package installs across jefe processes and make installation atomic (Unix)

Sub-issue of the managed package-cache concurrency parent (#425, Problem B).

## Goal

Give the managed npm package cache real cross-process mutual exclusion and an
atomic install, implemented and behaviorally verified on Unix. Windows locking
behavior is **not** changed here (separate Windows sub-issue); the tree must
keep building/working on all platforms.

## Design (dependency-free, std-only)

- **Cross-process lock** = a per-digest lock file created with
  `OpenOptions::create_new(true)` (`O_EXCL`). `create_new` is an atomic,
  exclusive, cross-process AND cross-thread primitive — exactly one contender
  wins. No new crate needed (rejects the non-goal "do not add a dependency").
- **Stale recovery (A4)** = the lock file records `pid` + install epoch. A
  contender that finds an existing lock reads the pid and checks liveness via
  the portable `kill -0 <pid>` shell built-in (`std::process::Command`, no
  crate). A dead pid is recovered **immediately** (no fixed timeout). A live
  pid is never declared stale → the contender waits.
- **Atomic install (A2)** = build in a sibling temp dir
  `cache_root/.<digest>.building-<pid>-<nonce>`, write the marker there, then
  swap into place: remove any stale final dir (we hold the lock, safe) and
  `rename(temp → final)`. The final path is therefore either absent or
  complete — never a partial tree.
- **Interrupted install (A3)** = if the process dies before the rename, the
  final dir never exists (no marker, no partial tree) → the next call is a
  clean cache miss + retry. Stale `.building-*` dirs for the held digest are
  swept at install start (we hold the lock, so they are abandoned).
- **Waiter loop** = double-checked locking: check `cache_hit`; if miss, acquire
  lock; under lock re-check `cache_hit` (another may have finished); else
  install + swap + release. Losers observe a complete cache hit (A1).
- **Mutex kept** as the existing intra-process guard (A6: single-process
  behavior unchanged); the file lock adds the missing cross-process dimension.
- **Platform split**: new lock + atomic-swap path is `#[cfg(unix)]`; the
  `#[cfg(not(unix))]` path keeps the current direct-write behavior so Windows
  is unchanged.

## Acceptance matrix

| ID | Requirement | Behavioral proof (test) | Location |
|----|-------------|--------------------------|----------|
| A1 | Two concurrent installers of the same digest serialize; one installs, the other observes a complete cache hit | Two-**process** test: spawn the helper bin twice in parallel against a counting npm stub; assert exactly one npm invocation and both resolve to the identical executable | `tests/issue556_behavior.rs` + `src/bin/jefe-issue556-installer.rs` |
| A2 | A reader never observes a partially built tree | Install builds in a sibling temp dir and is renamed into place; test asserts the final path is never resolvable with a partial state (bin without marker) while building in temp | `package_cache_lock_tests.rs` (atomic swap) + `package_runtime_atomic_tests.rs` |
| A3 | An interrupted install leaves no cache hit and is retried cleanly | npm fails (exit≠0) mid-install; assert no marker, no final dir, then a succeeding retry installs cleanly | `package_runtime_atomic_tests.rs` |
| A4 | Stale lock from a dead process recovered without a fixed timeout; a live long install never declared stale | Unit-test the staleness decision: dead pid (exited child) → recover; live pid (`sleep` child) → not stale | `package_cache_lock_tests.rs` |
| A5 | Lock, stale-recovery, rename failures are typed, bounded, redacted | Distinct `PackageRuntimeError` variants (`CacheLock`, `InstallRename`); assert bounded length and no secret leakage | `package_cache_lock_tests.rs` + variant tests |
| A6 | Existing single-process behavior unchanged | Existing `package_runtime_tests.rs` + launch/probe tests remain green; no new timing-dependent assertions | existing tests |
| A7 | uvx path retains current semantics | uvx preparation unchanged; behavioral assertion that uvx still resolves the structural prefix | `tests/issue556_behavior.rs` |

## Non-goals (enforced)

- No selector normalization / digest / cache-key changes (#554).
- No probe-budget / probe-phase changes (#553).
- No new dependency (no `libc`/`fs2`/`fs4`/`nix`/`rustix`).
- No background installer / daemon / queue.
- No Windows locking behavior change (Windows sub-issue).

## Files (scope ledger)

| File | Change | Acceptance |
|------|--------|------------|
| `src/runtime/package_cache_lock.rs` | NEW — cross-process lock + atomic dir swap | A1,A2,A3,A4,A5 |
| `src/runtime/package_cache_lock_tests.rs` | NEW — lock/stale/swap unit tests | A2,A3,A4,A5 |
| `src/runtime/package_runtime.rs` | wire lock + atomic install into `prepare_managed_npm`; add error variants; keep Mutex | A1-A6 |
| `src/runtime/package_runtime_atomic_tests.rs` | NEW — integrated atomic-install + interrupted-retry tests | A2,A3 |
| `src/runtime/mod.rs` | declare new module | — |
| `src/bin/jefe-issue556-installer.rs` | NEW — helper bin driving the production finalize boundary | A1 |
| `tests/issue556_behavior.rs` | NEW — two-process serialization + uvx-unaffected | A1,A7 |
| `Cargo.toml` | add `[[bin]]` helper entry (test fixture, no dependency) | A1 |

Review counters: OCR pre-PR 0/2, OCR post-PR 0/2.

## Verification

`cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings &&
cargo build --workspace --all-features --locked && cargo test --workspace --all-features --locked`
plus `cargo xtask check source-size` / `architecture`.
