# Issue #555 / #556 — managed package-cache installs are unsafe across jefe processes and are not atomic

Parent: **#555**. Delivered slice: **#556** (cross-process lock + atomic install,
implemented and verified on Unix). The Windows slice is **#557** and is
explicitly *not* delivered here — it must be developed and verified on native
Windows. This PR keeps the tree working on all platforms.

## Root cause

`src/runtime/package_runtime.rs::prepare_managed_npm` is the sole production
install path for managed npm agents (`launch_compose::prepare_launch →
local_candidate → finalize_local_invocation`, and `agent_probe →
prepare_local_probe`). It has three defects:

1. **No cross-process mutual exclusion.** The install directory
   `<cache_root>/<selector-digest>/` is *per machine*, but the only guard is a
   `static INSTALL_LOCK: Mutex<()>`, which is *per process*. Two jefe processes
   that both miss the same digest run `npm install` concurrently into the same
   directory. This is #425 Problem B (`ENOTEMPTY … rename …/node_modules/…`)
   reproduced one level up, inside jefe's own directory. The `Mutex` was never
   intended as directory-level exclusion — it is an intra-process invariant
   guard for the runtime/capture-worker split, and it stays for that purpose.
2. **Installation is not atomic.** `write_package_json` → `run_npm_install` →
   (only on success) `.jefe-installed` marker, all mutating the *final* path.
   An interrupted install leaves `package.json` with no `node_modules` and no
   marker; a concurrent reader can observe a half-built `node_modules` tree.
3. **The cache-hit check is an unprotected two-part read** (marker contents plus
   binary resolution) with nothing preventing a concurrent writer from rewriting
   `node_modules` between the two parts.

#554 (landed as `509661a`) converts this from dormant to live: volatile
selectors now reinstall on TTL expiry, so installs are routine rather than
once-ever.

## Fix

### Cross-process advisory lock

A new private runtime module `src/runtime/package_install_lock.rs` owns the
protocol. The lock file is `<cache_root>/<digest>.lock` — a **sibling** of the
install directory, not inside it, because the install directory is now replaced
wholesale by `rename`.

The lock is a **kernel advisory lock**: `std::fs::File::try_lock` /
`File::unlock`, which are `flock` on Unix and `LockFileEx` on Windows — exactly
what the issue's proposed direction asked for. They are stable std since Rust
1.89, so this needs no new dependency and no `unsafe`.

Because the lock lives in the kernel and is attached to the open file
description, the operating system releases it when the holder exits for any
reason, including `SIGKILL`. That removes the entire class of problems a
userspace lock would have to solve:

- a legitimate install is **never** declared stale however long it runs, which
  is precisely the mistake npm's five-second stale threshold made;
- a crashed holder needs no recovery, so there is no window in which two
  processes could each decide to recover the same lock;
- there is no on-disk ownership record, so no pid identity, no liveness
  probing, no file mtime and no wall-clock reasoning enters the protocol;
- the lock file is never unlinked, so no process can remove a lock another
  process holds.

Waiting polls `try_lock` and is bounded by `INSTALL_TIMEOUT + LOCK_WAIT_GRACE`.
Exceeding the ceiling is a typed, fail-closed error — never a forced steal. The
lock file is a zero-byte sentinel, created if absent and never truncated,
because another process may already hold it.

Lock scope: acquired **before** the cache-hit check and held through the marker
write, binary resolution, and fingerprint capture, closing defect 3.

Ordering: a **per-digest** intra-process guard is taken first, then the file
lock. The `static INSTALL_LOCK` from #382 becomes a keyed
`BTreeMap<digest, Arc<Mutex<()>>>` because preparation now waits on another
process: a process-global guard would let one contended digest block every
unrelated one.

This serializes processes on one machine against one local cache directory,
which is what `dirs::cache_dir()` provides. Advisory locking over a network
filesystem is emulated at best; jefe does not support a shared network package
cache, and the module says so.

### Atomic install

`npm install` never touches the final path again:

1. `<cache_root>/.staging-<digest>/` — any leftover is removed first, so staging
   always starts empty.
2. `package.json` written into staging; `npm install` runs with staging as cwd.
3. The binary is resolved **in staging** and must exist, else
   `InstalledBinaryMissing` — a broken tree is never promoted.
4. The `.jefe-installed` marker is written **into staging**, so the promoted
   directory is complete the instant it appears. There is no window in which
   `node_modules` exists without its marker.
5. Promotion: if the install directory exists, `rename` it to
   `<cache_root>/.retired-<digest>/`; `rename` staging into place; then remove
   the retired tree. Neither `rename` ever targets an existing directory, so
   both are valid on Unix and Windows. If publication fails after the previous
   entry was retired, that entry is restored, and a restore that itself fails
   is reported rather than hidden.

A reader that goes through preparation observes either the complete previous
tree or the complete new tree — never a partial one — because it holds the same
lock. An interrupted install leaves only staging; the final path is untouched
and the next attempt starts from a clean staging dir.

Publication is two renames, so a process killed between them leaves the
published path absent with a complete tree at the retired path.
`restore_interrupted_promotion` reconciles that under the lock **before** the
cache is consulted: a retired tree with no published entry is republished, and
a retired tree alongside a published entry is discarded. A crash therefore never
strands a valid cache entry or forces a network install to recover.

Because staging always starts empty, a re-install can never see a previous
`package-lock.json`, which **subsumes** #554's explicit volatile lockfile
removal. That now-unreachable removal is deleted and #554's behavioral guarantee
("an expired volatile re-resolve must not let npm see a prior lockfile") is
retained as an assertion in the existing #554 test.

### Typed, bounded, redacted diagnostics

Two new `PackageRuntimeError` variants: `InstallLockUnavailable` and
`InstallPromotionFailed`. Every detail string is truncated to
`MAX_DETAIL_CHARS` (256) and carries the selector digest plus the
`io::ErrorKind` — never an absolute cache path (which embeds the user's home
directory and therefore the account name).

The issue's proposed direction also asked for a distinct stale-lock-recovery
variant. Under a kernel advisory lock there is no stale-lock recovery step to
fail, so that variant would be unreachable and is deliberately not added.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Success behavior | Failure behavior + diagnostic location | Side effects permitted before failure | Persistence / compatibility | Behavioral proof |
|---|---|---|---|---|---|---|---|
| A1 | Two concurrent OS processes calling `finalize_local_invocation_at` (production preparation boundary) for the same digest | same cache root, same selector, cold cache; npm stub sleeps to widen the window | both succeed; `npm install` runs **exactly once**; both resolve the identical executable path | n/a | second process waits on the lock | on-disk cache layout unchanged for readers | `concurrent_processes_install_once_and_agree` (re-execs the test binary twice) |
| A2 | `finalize_local_invocation_at`, cache miss | npm stub asserts, from inside the install, that the *final* install directory holds no `node_modules` | install is built in `.staging-<digest>` and renamed into place; final path never holds a partial tree | n/a | staging directory created | final directory only ever complete | `install_is_staged_and_promoted_atomically` |
| A3 | `finalize_local_invocation_at`, cache miss, npm fails mid-install | stub creates a partial `node_modules` then exits non-zero | typed `InstallFailed`; final install directory has **no marker** and no promoted tree; a subsequent run with a healthy stub installs successfully | `PackageRuntimeError::InstallFailed` | staging left behind, reclaimed by the next run | no cache hit is ever produced from an interrupted install | `interrupted_install_leaves_no_cache_hit_and_retries_cleanly` |
| A3b | Preparation after a process was killed between the two publication renames | published path absent, complete tree at the retired path | the previous entry is republished under the lock; no npm install is required | `InstallPromotionFailed` if the reconciling rename fails | none | a crash never strands a valid cache entry | `a_promotion_interrupted_after_retiring_restores_the_previous_install`, `a_completed_promotion_discards_a_leftover_retired_tree` |
| A4a | A second OS process takes the lock and is **killed** without releasing it | holder dies with the lock held | the kernel releases the lock; the next acquisition succeeds with no recovery step and no timeout | n/a | none | — | `a_lock_whose_holder_was_killed_is_available_without_recovery` |
| A4b | A second OS process holds the lock for the whole (shortened, injected) ceiling | holder alive throughout | never taken over; once the holder releases, the waiter acquires | `PackageRuntimeError::InstallLockUnavailable` | none | — | `a_lock_held_by_a_live_process_is_never_taken_over` |
| A4c | `prepare_managed_npm_with_lock_policy` while the digest is held | held for longer than the injected ceiling | fails closed | `InstallLockUnavailable`; no install directory is created | none — no install, no staging | — | `a_live_installer_blocks_preparation_with_a_typed_redacted_error` |
| A5 | Both new failure variants | unopenable lock, expired wait, failed publication | — | each variant is distinct, `Display` is bounded, carries the selector digest, and contains no path separator, cache path, or home directory | none | — | `a_lock_that_cannot_be_opened_is_typed_bounded_and_redacted`, `a_wait_timeout_is_typed_bounded_and_redacted`, `a_failed_promotion_is_typed_bounded_and_redacted` |
| A6 | Existing single-process npm behavior | existing package-runtime, #554 TTL, probe, and launch-compose tests | unchanged and green; no timing-dependent assertions added | — | — | marker format unchanged | existing `package_runtime_tests.rs`, `tests/issue382/package_selector.rs` |
| A7 | uvx preparation | uvx candidate | unchanged: no lock, no staging, structural `--from` prefix | — | — | — | `local_uvx_is_a_closed_structural_prefix`, `structural_uvx_probe_executes_the_selected_agent_invocation` |

## Non-goals

- Selector normalization, digest derivation, cache-key semantics (#554).
- Probe budgets / probe phase semantics (#553).
- Any new dependency (`libc`, `rustix`, `fs2`, `fd-lock`, …). `unsafe` stays
  forbidden.
- Background installer, daemon, or install queue.
- Windows-specific locking work and native-Windows verification (#557).
- Garbage-collecting the package cache, cache size limits, or eviction.
- `src/runtime/llxprt_install.rs` (superseded, no production callers — already
  a recorded #554 follow-up).

## Vertical slices

1. **Lock protocol** — `src/runtime/package_install_lock.rs`: identity-based
   staleness, `create_new` acquisition, verified rename takeover, bounded wait,
   RAII release. RED: A4a/A4b/A4c/A5.
2. **Atomic install + lock integration** — `src/runtime/package_runtime.rs`:
   staging/promotion, marker-in-staging, lock held across cache-hit check
   through fingerprint capture, new typed variants. RED: A1/A2/A3/A5.

Both slices live in one architectural layer (`src/runtime`) and one orchestration
route (managed npm preparation), so no stacked split is required.

## Expected paths

| Layer | Path | Change |
|---|---|---|
| runtime | `src/runtime/package_install_lock.rs` | new (private module) |
| runtime | `src/runtime/package_runtime.rs` | modified |
| runtime | `src/runtime/mod.rs` | register private module |
| tests | `src/runtime/package_runtime_lock_tests.rs` | new |
| tests | `src/runtime/package_runtime_tests.rs` | witness helper adapted to an absolute path (staging makes an in-directory witness unobservable across installs); #554 assertions preserved |
| tests | `src/runtime/package_install_lock_tests.rs` | new |
| quality tooling | `clippy.toml`, `.github/clippy/clippy.toml` | `msrv` corrected (approved, S6) |
| collateral | 11 files under `src/` and `tests/` | mechanical `collapsible_if` / `unnecessary_map_or` rewrites newly required by the corrected `msrv` (S7) |
| docs | `project-plans/issue555-plan.md` | this plan |

Budget: 24 files, +1,684 net lines. Inside the 25-file target; above the
1,500-line target, so the mandatory scope review below applies. Far below the
40-file / 2,500-line hard stop.

## Mandatory scope review (net lines above target)

Every changed file maps to an acceptance row or to an approved scope-ledger
entry, and nothing was added that is not required by them:

| Group | Files | Net | Justification |
|---|---|---|---|
| Production behavior | `package_install_lock.rs`, `package_runtime.rs`, `mod.rs` | ~+340 | Acceptance rows A1–A5 and scope-ledger S1, S8, S9. |
| Behavioral tests | `package_install_lock_tests.rs`, `package_runtime_lock_tests.rs`, `package_runtime_tests.rs` | ~+830 | One test per acceptance row plus the two-process harness A1 requires. Test coverage is the largest group and is explicitly not treated as scope expansion. |
| Plan and triage record | `project-plans/issue555-plan.md` | ~+310 | Required by the workflow (acceptance matrix, non-goals, ledger, review counters, triage). Documentation only. |
| Approved MSRV correction and its collateral | `clippy.toml`, `.github/clippy/clippy.toml`, 11 mechanical source files | ~+40 | Scope-ledger S6 (user-approved) and S7 (unavoidable, machine-applied, semantically identical). |

No new subsystem, public abstraction, dependency, or behavior outside the
acceptance matrix was introduced. The overage is documentation and behavioral
tests, not production surface, so the work is not split.

## Scope ledger

| # | Discovered work | Disposition |
|---|---|---|
| S1 | New private module `package_install_lock.rs` instead of growing `package_runtime.rs` | **In scope.** `package_runtime.rs` is already 503 lines against a 750-line warn / 1000-line hard gate. Same layer, same route, `pub(super)` only — an internal split, not a new public abstraction. |
| S2 | #554's explicit `package-lock.json` removal becomes unreachable under fresh staging | **In scope.** Removing unreachable code is required by the accepted behavior; #554's behavioral assertion is preserved in its existing test. |
| S3 | #554 witness helper moves from `<install_dir>/.jefe-lock-witness` to an absolute path supplied by env | **In scope.** Forced by A2 (staging starts empty), not an unrelated test move. Assertions unchanged. |
| S4 | Re-exec of the unit-test binary to obtain a genuine second OS process for A1 | **In scope.** Uses `std::env::current_exe`, an established pattern in this repo (`process_tests.rs`, `harness_v1.rs`). No new `[[bin]]`, no manifest change. |
| S5 | Lock-wait ceiling injected via a private `LockPolicy` seam so A4b/A4c do not sleep for the production 5-minute ceiling | **In scope.** Test seam only; the public preparation boundary keeps its signature and production defaults. |
| S6 | `clippy.toml` / `.github/clippy/clippy.toml` `msrv` raised from `1.75` to `1.89` | **Approved by the user.** Required to use `std::fs::File::try_lock`, which is the only correct lock available without a new dependency or `unsafe`. The old value was already impossible: `edition = "2024"` needs Rust ≥ 1.85, `Cargo.toml` declares no `rust-version`, and every CI job uses the stable toolchain. Not a loosening — no threshold was raised and no lint was downgraded or suppressed. |
| S7 | 16 pre-existing `collapsible_if` / `unnecessary_map_or` sites across 11 files became lint errors once the `msrv` was corrected (let-chains and `is_none_or` are ≥ 1.88) | **In scope as unavoidable collateral of S6.** Every edit is clippy's own machine-applicable rewrite and semantically identical. The alternative — suppression attributes — is forbidden by the lint-guardrail policy. |
| S8 | `INSTALL_LOCK` changed from one process-global mutex to a per-digest map | **In scope (review finding).** Preparation now waits on another process while holding the intra-process guard, so a global guard would newly let one contended digest block every unrelated one. Preserves the #382 invariant at the correct granularity. |
| S9 | `restore_interrupted_promotion` added | **In scope (review finding).** Publication is two renames; without reconciliation a crash between them strands a valid cache entry and forces a network install to recover. |

## Review counters

- Local OCR runs (cap 2): 1 — commit-scoped, `complete_best_effort`, artifacts under
  `~/Library/Logs/llxprt-code/opencodereview/runs/20260801T055530Z-ce3f71ee/`
  (plus corroborating runs launched inside the two review subagents).
- PR OCR runs (cap 2): 2 — both automatic, on PR #572. The bot reports
  "Automatic OCR review budget: 2 of 2 used". No further review runs are
  requested; remaining work is remediation only.

## Review triage

Cycle 1: `rustreviewer` and `deepthinker` (independent, full-scope) plus OCR.

| Finding | Disposition |
|---|---|
| Stale takeover (`rename` + read-back) is not compare-and-swap; two recoverers can both own the lock, and an earlier guard can unlink a successor's lock | **Blocker — Fixed.** Replaced the entire userspace protocol with `std::fs::File::try_lock` (kernel `flock`/`LockFileEx`). No takeover, no unlink, no ownership record. |
| Create-then-write window plus wall-clock `content_grace` allows stealing a live lock; read errors read as an empty body; clock jumps break both directions | **Blocker — Fixed.** No lock body, no mtime and no wall clock remain in the protocol. |
| Crash between the two publication renames strands the previous good tree; rollback failure is swallowed; retired tree deleted while the published path is absent | **Blocker — Fixed.** `restore_interrupted_promotion` reconciles under the lock before the cache is consulted; a failed restore is reported as its own diagnostic. |
| `remove_tree` treats every metadata error as absence, so staging may not actually be reset | **In-scope — Fixed.** Only `NotFound` counts as absent; `Path::exists` replaced by a fallible `exists`. |
| Degraded pid-only self identity defeats pid-reuse detection and is cached for the process lifetime | **Fixed by design.** No identity is recorded at all. |
| One process-global install mutex held across a cross-process wait blocks unrelated digests | **In-scope — Fixed.** Per-digest guards (S8). |
| Promotion diagnostics do not carry the selector digest the plan claims | **In-scope — Fixed.** Promotion shares the lock module's bounded/redacted formatter and asserts the digest. |
| Timeout diagnostic asserts the holder is live when liveness was only not disproven | **Fixed by design.** The kernel answer is definitive; the message now states the lock is still held. |
| Orphaned `.claim-*` files accumulate after a crash | **Fixed by design.** No claim files exist. |
| The two-process test has no start barrier, so a loaded machine could serialize the children and pass against an unserialized implementation; children are not reaped and stderr is discarded | **In-scope — Fixed.** Children publish readiness and park until the parent releases a start marker; stderr goes to per-child files; an RAII guard always kills and reaps. |
| Lock tests only exercise one in-process recoverer | **In-scope — Fixed.** Contended and killed-holder cases now use a real second OS process. |
| Publication briefly leaves the path absent, so a consumer that already resolved an executable but has not spawned it — or a running agent loading files by path — is not protected. Fix requires generation-addressed install directories, a pointer swap, retention of referenced generations, and re-fingerprinting before spawn | **Defer.** Real, but it is a property of the fixed-path cache design that predates this change (in-place `npm install` was strictly worse), and the remedy changes cache-key/path semantics — an explicit non-goal here. Follow-up issue filed. |
| Cache hits now require a writable cache root, so a read-only or full cache fails a launch that previously succeeded | **Reject (accepted trade-off).** The issue's stated direction requires the lock to be taken before the cache-hit check. Removing it safely needs the deferred generation-addressed design. |
| Add `fs2`/`fd-lock`, or an audited OS-lock implementation | **Reject.** Unnecessary: std provides the same primitive. Adding a dependency is an explicit non-goal. |
| No `fsync`, so power-loss durability is not guaranteed | **Reject.** Power-loss durability is not claimed; the documentation says process crash. |
| Shared network/NFS or cross-PID-namespace caches are unsound | **Reject (documented).** jefe's cache is the local per-user `dirs::cache_dir()`; the module states that a shared network cache is unsupported. |
| Volatile marker stamps `now` from before the wait, slightly shortening the next TTL | **Reject.** `now` is injected for determinism; the shift is bounded by the install duration and immaterial against a 12 h TTL. |

Cycle 2: Open Code Review on PR #572 (2 automatic runs, 17 inline findings).

| Finding | Disposition |
|---|---|
| Reconciliation discards the retired tree whenever the published path merely *exists*, so a corrupt published entry destroys the last complete copy | **Blocker — Fixed.** The retired tree is discarded only once the published entry is structurally complete (marker present, selected binary resolvable); otherwise the unusable published directory is removed and the retired one republished. The check is structural rather than `cache_hit`, because a merely TTL-expired entry is still a better offline fallback than none. New test `an_unusable_published_install_is_replaced_by_the_retired_tree`. |
| `INSTALL_LOCKS` grows without bound (reported twice) | **In-scope — Fixed.** `install_guard_for` drops guards the map alone references before inserting. Safe because a preparer keeps its `Arc` alive across the whole critical section, so two preparers of one digest can never receive different mutexes. |
| `HolderProcess::release` calls `wait` with no timeout, turning a hung holder into a stalled CI run | **In-scope — Fixed.** `wait_bounded` polls `try_wait` against a 30 s deadline and kills the child on overrun. No `wait-timeout` dependency added. |
| `HolderProcess` borrows the `TempDir` instead of owning it | **In-scope — Fixed.** It now owns the `TempDir`, so the compiler enforces the lifetime. |
| Test file inherits `Duration`/`Instant` from the parent module's glob | **In-scope — Fixed.** Explicit imports added. |
| Generated shell stubs splice paths into single quotes and could break on an embedded quote (reported three times, across both test files and the `settle` argument) | **In-scope — Fixed.** Every interpolated value goes through the repository's existing `runtime::commands::shell_escape_single`, and the scripts bind them to shell variables. |
| Diagnostic length assertion used `MAX_DETAIL_CHARS * 2`, which is slack rather than a derived bound | **In-scope — Fixed.** A named `MAX_DIAGNOSTIC_CHARS = MAX_DETAIL_CHARS + 64` (the fixed `Display` prefix) is used by every diagnostic assertion. |
| The two-process test protocol has no summary documentation | **In-scope — Fixed.** Module-level doc block describing the re-execution contract, the environment channel, the barrier file naming, the report format, and child ownership. |
| `observed_candidate` resolves the candidate twice | **Reject (documented).** Not redundant: the first resolution is the only way to derive the published install directory the stub must be told about, and the second must follow the real stub so the candidate fingerprints the npm that actually runs. Comment added. |
| Guards are held across `npm install`; drop and reacquire, or stage per process and lock only for promotion | **Reject.** Spanning the install is the point of the lock, and per-process staging would let N processes each run a full install of the same package — the duplicated work A1 forbids. The claim that unrelated digests are blocked is incorrect: both guards are keyed by digest. Tradeoff now stated in the code. |
| Add a runtime assertion or wrapper enforcing lock-acquisition order to prevent a future ABBA deadlock | **Reject.** Both acquisitions are adjacent lines of one private function and `package_install_lock::acquire` has no other production caller. Thread-local lock-order tracking would be a new subsystem guarding a hypothetical future edit. |
| The barrier plus the 1 s stub delay makes the two-process test timing-sensitive on loaded CI | **Reject.** The barrier is what removes the timing sensitivity: both children are proved to be inside preparation before either is released, so overlap does not depend on spawn order or scheduling. The 60 s deadline is a hang guard that fails the test, not a synchronization assumption. |
| `is_none_or` equivalence confirmation | **No action.** Informational. |

## Deferred findings

- Generation-addressed managed installs so a consumer that has already resolved
  an executable, or a running agent that loads files by path, is insulated from
  a replacement — plus the read-only-cache fast path that becomes possible once
  published generations are immutable. Filed as **#571**.
