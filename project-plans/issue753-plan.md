# Issue #753 — Footer commit stale after a fast-forward pull

Branch: `issue753`, cut from exact `origin/main` `20bb4bb0aa408197dc7b4be2b17b960e4a5a710c`
(PR #754 merge). Status: implemented on this branch (RED and GREEN recorded,
all pre-review gates green, local pre-PR review complete — see the review
record below); ready for PR.

## Problem

A binary built from one commit reports an older commit in the footer identity
slot after an ordinary fast-forward pull. `build.rs` emits a single
`cargo:rerun-if-changed=.git/HEAD`. `.git/HEAD` is a symbolic-ref file
(`ref: refs/heads/main`); a fast-forward moves `refs/heads/main` and leaves the
HEAD file byte-identical. Cargo sees no watched change, never re-runs the build
script, and `JEFE_GIT_COMMIT` keeps its previous value. The build script's own
comment admits this: "fast-forwards that do not touch any source file require a
clean rebuild to refresh the hash."

Consumers of the baked value, all read-only for this issue:
`src/lib.rs` `GIT_COMMIT` (via `option_env!`), `process_identity_label`
rendered by `src/ui/components/provider_screen.rs`, the issues, pull requests,
and actions screens, `src/selection/content.rs`, and
`src/doctor/{collection,report}.rs`.

## Root cause (confirmed from source and Git observation)

1. `build.rs` line 16: the only rerun trigger is `.git/HEAD`.
2. Fixture observation (throwaway repo under gitignored `tmp/`): across a
   branch move A→B, `.git/HEAD` stays `ref: refs/heads/main` with unchanged
   bytes and mtime while `refs/heads/main` changes to B. This is the same
   ref mutation a fast-forward pull performs.
3. Cargo-level reproduction on this host (cargo 1.98.0, micro crate in `tmp/`,
   jefe checkout untouched): build 1 bakes A; branch ref moves to B with no
   crate change; build 2 leaves the build-script `output` file untouched and
   the baked commit still A. The build script did not re-run. This is the
   issue's failure, reproduced exactly.

## Verified capability evidence (this host, cargo 1.98.0, git worktree layouts)

Probed in disposable fixtures under `tmp/issue753-setup/` before writing this
plan, so the design below rests on measured behavior:

- Detached HEAD: the HEAD file holds a raw sha and is rewritten on every
  movement (verified `80e912b6` → `b6edaf76` across a detached commit).
- Linked worktree: `.git` is a gitfile; `git rev-parse --git-dir` returns the
  per-worktree gitdir (holds HEAD), `git rev-parse --git-common-dir` returns
  the main `.git` (holds branch refs), `git symbolic-ref -q HEAD` returns the
  branch ref name. All three resolve every ordinary layout.
- `git pack-refs --all` removes the loose `refs/heads/<branch>` file; the next
  ref update re-creates the loose file, which shadows the stale packed entry
  (verified: packed-refs kept the old value while the loose file carried the
  new one, and `git rev-parse HEAD` resolved correctly throughout).
- Watching a currently-absent loose-ref path triggers a rerun when the file
  first appears (verified content-decisively: fingerprint taken while the
  branch was packed-only; a later ref update re-created the loose file and the
  rebuilt commit changed).
- Cargo emits no warning for a `rerun-if-changed` path that does not exist.
- Relative `rerun-if-changed` paths resolve against the build script's cwd,
  which is the package root (verified with `../.git/...` forms).

## Accepted implementation decisions

The issue names two acceptable mechanisms; this plan takes the first:

1. **Watch the resolved ref file as well as HEAD.** The watch list becomes
   exactly two files for an attached HEAD: `<git-dir>/HEAD` and
   `<common-dir>/<branch-ref>`. For a detached HEAD: `<git-dir>/HEAD` only
   (movement rewrites the watched file, verified above). Reading the commit
   stays `git rev-parse --short HEAD`, which already resolves every layout
   through git itself.
2. Resolve paths through git plumbing rather than by parsing `.git` by hand:
   `git rev-parse --git-dir`, `git rev-parse --git-common-dir`,
   `git symbolic-ref -q HEAD` (nonzero exit and empty stdout when detached).
   This keeps `.git`-directory, gitfile/worktree, packed-refs, and detached
   behavior correct without reimplementing git.
3. **Do not watch `packed-refs`.** Ref updates write loose refs; the
   absent-to-present transition triggers; the shadow rule keeps resolution
   correct. A third watch line adds noise and no measured benefit.
4. Fallback unchanged: git missing or not a repository keeps today's floor
   (`watch .git/HEAD`, bake `JEFE_GIT_COMMIT=unknown`, build succeeds).
5. **Pure build-support seam.** New `build_support/git_watch.rs` at repo root,
   included by `build.rs` via `#[path = "build_support/git_watch.rs"]` and by
   the integration test via `#[path = "../build_support/git_watch.rs"]`. It
   holds the pure HEAD classification and watch-path computation plus thin
   git invokers, so build.rs and the tests compile one source of truth. It is
   not exported from the lib, touches no `src/` module, adds no dependency,
   and requires no Cargo.toml change. The xtask architecture gate scans only
   `src` and `tests` (checked `xtask/src/architecture.rs`), so a new top-level
   build-support directory needs no gate change.
6. **RED proof is cargo-level and invokes the real `build.rs`.** The
   integration test writes a fixture crate inside a fixture git repository;
   the fixture manifest points the package's build script directly at the
   real file (`build = "<abs path to jefe build.rs>"`, the path baked as a
   literal at test runtime from `env!("CARGO_MANIFEST_DIR")`), so cargo
   compiles that exact unmodified file as the fixture's build script. The
   originally planned `include!("<abs path>")` form is rejected by rustc
   (E0753: the jefe `build.rs` starts with `//!` inner doc comments and macro
   expansion cannot produce inner attributes; probed on this host before
   implementation). Only cargo makes the actual rerun decision the issue is
   about, so only a cargo-level test can fail for the exact reason today.
   Direct `rustc` invocation was rejected as less faithful.

## Git layout coverage (degree required for a correct ordinary checkout)

| Layout | HEAD watched at | Branch ref watched at | Movement trigger |
|---|---|---|---|
| Ordinary repo, attached HEAD | `<pkg>/.git/HEAD` | `<pkg>/.git/refs/heads/<branch>` | Ref rewrite triggers; branch switch or detach rewrites HEAD and triggers |
| Branch currently packed (no loose file, e.g. after gc) | same | same path, absent at fingerprint time | Loose file creation on next ref update triggers (verified) |
| After `pack-refs --all` | same | same | Loose removal plus later re-creation triggers; packed-refs holds a stale shadowed value that resolution ignores (verified) |
| Detached HEAD | gitdir/HEAD holds raw sha | n/a | Every movement rewrites HEAD (verified) |
| Linked worktree (gitfile `.git`) | `<main>/.git/worktrees/<name>/HEAD` | `<main>/.git/refs/heads/<branch>` via common dir | Same rules; paths resolved by plumbing and emitted absolute when the common dir sits outside the package |
| Tarball or missing git | floor `.git/HEAD` only | n/a | Commit bakes `unknown`; build succeeds (today's behavior preserved) |

## Acceptance matrix

| ID | Actor / launch path | Input and boundary | Target | Observable success | Failure and diagnostic | Side effects permitted before failure | Persistence / compatibility | Behavioral proof |
|---|---|---|---|---|---|---|---|---|
| A1 | Developer or CI builds jefe after a fast-forward pull | Fixture repo at commit A; build-script metadata runs; branch fast-forwards to B without touching `.git/HEAD`; no source file changes; rebuild | Local build (all platforms with git + cargo) | Second build's baked `JEFE_GIT_COMMIT` equals B; matches `git rev-parse HEAD` | Stale A persists and the test names the stale value found in the build-script output file | Build-script execution writes only cargo's target artifacts | `option_env!` consumer contract unchanged; existing binaries unaffected | Cargo-level fixture test invoking the real `build.rs` twice around the ref move (RED today, GREEN after fix) |
| A2 | Build-script output inspection | Attached HEAD in an ordinary repo | Local build | Output file records `rerun-if-changed` for the resolved HEAD file and the resolved branch-ref file, package-relative when under the package root | Missing or wrong watch lines listed by the test | None beyond A1 | Watch lines are cargo directives; no runtime surface | Assertion on the fixture build's output file in the A1 test |
| A3 | Build in detached-HEAD state | Fixture repo checked out at a sha; movement rewrites HEAD | Local build (seam level) | Watch list is the resolved HEAD file only; computed list updates when HEAD content class changes | Wrong classification reported with HEAD contents | None | Detached builds bake the then-current sha | Seam unit tests against a real detached fixture repo |
| A4 | Build while branch is packed | Loose ref absent at fingerprint time | Local build (seam level plus verified cargo behavior) | Watched absent path still emitted; later ref update re-creates it and A1 mechanics observe the new commit | Absent path dropped from watch list would be reported | None | No schema or artifact change | Seam test asserting the branch-ref path is watched even when the loose file does not exist; cargo trigger verified empirically on this host and recorded above |
| A5 | Build inside a linked worktree | gitfile `.git`; per-worktree gitdir; shared common dir | Local build (seam level) | HEAD resolved in the worktree gitdir; branch ref resolved in the common dir; paths emitted absolute when outside the package | Resolution failure names the git command and stderr | None | Ordinary worktree builds keep a correct baked commit | Seam tests against a real `git worktree add` fixture |
| A6 | Build outside any repository or without git | Missing `.git`, missing git binary | Local build | `JEFE_GIT_COMMIT=unknown` emitted; build succeeds; floor watch `.git/HEAD` only | n/a (defined fallback) | None | Identical to today's fallback | Seam unit tests with resolution failing |
| A7 | Existing identity consumers | Current suite | All platforms | `tests/identity/identity_tests.rs`, `tests/doctor/report.rs`, and all other `GIT_COMMIT` consumers pass unchanged | Regression named by the failing existing test | None | Footer format `pid:{pid} {commit}` unchanged | Existing tests in the full gate run |

## Explicit non-goals

- Footer layout, clipping, and the identity label's position or format
  (#732 and the label's own formatting).
- Remote-tracking ref staleness: the baked identity is the HEAD tree's commit;
  `origin/*` tips are not watched and their movement must not trigger reruns.
- Watching `.git/packed-refs` (measured as unnecessary; see decisions).
- Default cargo rerun-on-anything semantics (rejected: mtime-fragile and
  wider than the accepted mechanism).
- Branch-name display, submodule pointer identity, or any new baked metadata
  beyond the existing `JEFE_GIT_COMMIT` and `JEFE_HOST_TRIPLE`.
- Changes to consumers (`src/lib.rs`, `src/doctor/`, `src/ui/`,
  `src/selection/`) or to Cargo.toml, Cargo.lock, xtask, or CI configuration.
- New dependencies, new public runtime API, or any parallel mechanism variant.

## Bounded vertical slices

### Slice 1 — RED evidence (test only, no production change)

- Acceptance rows: A1, A2 (as currently failing).
- Owner / boundary: integration test target `tests/identity/`; compiles the
  production `build.rs` unmodified through `include!` and cargo.
- Allowed files: `tests/identity.rs` (one `#[path]` module line),
  `tests/identity/build_script_fast_forward.rs` (new). The RED run needs only
  the fixture test; `build_support/git_watch.rs` lands in slice 2.
- RED criterion: `cargo test --test identity build_script_fast_forward`
  fails with a message naming the stale commit (A) still present in the
  build-script output after the fast-forward.
- RED criterion met (recorded 2026-09-05, `tmp/issue753/red.log`): exits 101
  with `rebuilt binary must report 0c7f3fc; JEFE_GIT_COMMIT stayed stale at
  3e82be8` (left "3e82be8", right "0c7f3fc").
- GREEN criterion for the slice: none; the slice exists to capture RED.
- Non-goals within the slice: no build.rs edit.
- Verification: the focused test command plus `cargo fmt --all --check` on
  touched files.
- Stop if: the fixture crate fails to build for an unrelated toolchain reason
  (offline registry, cargo home permissions), or include!-of-build.rs hits a
  compile context cargo rejects.

### Slice 2 — GREEN mechanism (production change + seam tests)

- Acceptance rows: A1, A2, A3, A4, A5, A6.
- Owner / boundary: build-script target plus the shared seam module; no `src/`
  changes; consumers untouched.
- Allowed files: `build.rs`, `build_support/git_watch.rs`, the slice 1 test
  file, `tests/identity.rs`.
- GREEN criterion: slice 1 test passes; the output file shows the commit B and
  both resolved watch lines; seam tests for detached, packed, worktree, and
  fallback pass.
- Refactor within scope: rewrite the stale build.rs comment that documents the
  old limitation so the file states the actual watch contract.
- Non-goals within the slice: packed-refs watching, consumer changes.
- Verification: focused identity tests, then the full gate list below.
- Stop if: any change beyond the allowed files appears necessary, or cargo
  behavior on Windows CI diverges from the recorded evidence.

### Slice 3 — Verification and evidence checkpoint

- Acceptance rows: A7 plus confirmation that A1–A6 hold on the exact head.
- Allowed files: none beyond evidence capture under `tmp/` or the issue/PR
  description.
- GREEN criterion: full gates pass on the candidate head; RED and GREEN
  outputs recorded; commits are one coherent green behavior each.
- Recorded so far (2026-09-05): focused GREEN at `tmp/issue753/green.log`
  (6 passed in 1.59s); mechanism evidence at `tmp/issue753/green-probe-*`.

## Expected paths by architectural layer

| Layer | File | Change |
|---|---|---|
| Build script | `build.rs` | Replace the single watch line with resolved watch lines from the seam; keep commit baking and fallback; rewrite the stale comment |
| Build support (new, outside the src DAG) | `build_support/git_watch.rs` | Pure head classification and watch-path computation; thin `git` invokers shared by build.rs and tests; no lib export |
| Integration tests | `tests/identity.rs`, `tests/identity/build_script_fast_forward.rs` | Cargo-level fast-forward RED/GREEN test; seam unit tests against real fixture repos following `tests/git_info` conventions (tempfile, `run_git` helper, `test_unwrap`) |

No other file changes are planned. The xtask architecture gate does not scan
`build_support/` (verified), and the source-size gate's scan roots are
unaffected by a small new file.

## RED/GREEN evidence plan

- RED command: `cargo test --test identity build_script_fast_forward -- --nocapture`
- RED expected failure: assertion reports the baked commit in the build-script
  output file equals A after the fast-forward to B (script did not re-run).
  Capture raw output under `tmp/issue753/` (gitignored) and quote it in
  the PR description.
- GREEN command: same command after slice 2.
- GREEN expected: test passes; assertions confirm the output file now carries
  `JEFE_GIT_COMMIT=<B>` plus `rerun-if-changed` lines for the resolved HEAD
  and branch-ref files.
- Micro-crate evidence already recorded on this host (cargo 1.98.0): today's
  watch list leaves the output file untouched across the ref move; the
  two-line watch list observed commits across plain, packed, and post-pack
  transitions; absent-to-present watch paths trigger; missing watched paths
  produce no warning. Fixture hashes and outputs live in
  `tmp/issue753/` for the duration of the effort.

### Recorded evidence (2026-09-05, this host)

- RED run: exits 101 with
  `assertion 'left == right' failed: rebuilt binary must report 0c7f3fc;
  JEFE_GIT_COMMIT stayed stale at 3e82be8` (left "3e82be8",
  right "0c7f3fc"); raw output at `tmp/issue753/red.log`. The preconditions
  held before the failure: the first binary baked A, `.git/HEAD` was
  byte-identical across the fast-forward, and A != B.
- GREEN run: `cargo test --test identity` exits 0; 10 passed in 1.59s (the
  four existing identity tests, the fast-forward test, and the attached,
  detached, packed, worktree, and non-repo seam tests); raw output at
  `tmp/issue753/green.log`. Both clippy gates pass (`-D warnings` and the
  namespaced complexity set; logs at `tmp/issue753/clippy*.log`), as do
  `cargo check --workspace --all-features --locked` and
  `cargo build --workspace --all-features --locked`.
- Mechanism probe (pre-implementation, `tmp/issue753/probe/`): `include!` of
  the jefe `build.rs` fails with E0753 (inner doc comments); `build = "<abs
  path>"` in the fixture manifest compiles the real file cleanly.
- GREEN mechanism evidence (`tmp/issue753/green-probe/`, results in
  `tmp/issue753/green-probe-result.txt` and
  `tmp/issue753/green-probe-output.txt`): build 1 baked `65fa2b6`; the
  fast-forward to `ae5ecb9` left `.git/HEAD` untouched; build 2 re-ran the
  build script and the rebuilt binary printed `ae5ecb9`. The output file
  shows the resolved package-relative watch lines
  `cargo:rerun-if-changed=.git/HEAD` and
  `cargo:rerun-if-changed=.git/refs/heads/test-main`, plus unchanged
  `JEFE_GIT_COMMIT` and `JEFE_HOST_TRIPLE` baking.
- Decision 3 (do not watch `packed-refs`) was not contradicted by GREEN
  evidence; the packed-branch seam test pins the absent loose-ref path on
  the watch list.
- Full pre-review gate run (2026-09-05, exact working-tree head under
  review, before the local OCR): all 14 gates green, exit 0 each, logs and
  exit codes at `tmp/issue753/gates1/g01..g14` — fmt, build, full workspace
  test (all-features, locked), coverage floor 30%, source-size,
  architecture, clippy `-D warnings`, namespaced complexity clippy,
  scenario_manifest, issue704/issue705/issue706 owner evidence,
  harness_authority, `git diff --check`.

### Pre-PR review record (local Open Code Review, run 1 of 2)

Run facts (raw output `tmp/issue753/review/ocr-result.json`): status
complete, workspace mode against base `20bb4bb0`; coverage selected 4/4 and
completed 4/4 — `build.rs`, `build_support/git_watch.rs`, `tests/identity.rs`,
`tests/identity/build_script_fast_forward.rs`. One comment raised (severity
low); no blocker, no major. Dispositions:

1. **Output-dir lookup nondeterminism — Reject, with evidence.** The reviewer
   proposed selecting the newest-by-mtime build-script `output` file,
   reasoning that unspecified `read_dir` order over multiple
   `jefe753fixture-*` dirs (fingerprint forked by differing emitted
   directives) could make the A2 watch-list assertion inspect a stale run.
   Rejected because the multiple-output-dir premise does not hold: cargo keys
   the build-script run directory by the compiled build-script fingerprint,
   and the fixture's `build.rs` is byte-identical across the A and B runs —
   emitted `cargo:` directives are recorded inside `output`, not in the
   directory hash. Measured on this host: the persistent fixture under
   `tmp/issue753/green-probe/` retained exactly one `output` file naming
   `JEFE_GIT_COMMIT` (dir `probe753-1ebf603b540fb444`) across builds at
   A=`65fa2b6` and B=`ae5ecb9`, overwritten in place (post-B contents are the
   two resolved watch lines plus `JEFE_GIT_COMMIT=ae5ecb9`); the sibling dir
   holds the compiled script, not an output. The A1 test's fixture target is
   additionally a fresh tempdir per test, so no cross-test staleness exists.
2. **Non-UTF-8 Git output — Defer to #755.** `plumbing_output` treats a
   non-UTF-8 stdout decode as absence, so a non-UTF-8 ref name or repository
   path component would silently reduce the watch list to today's
   `.git/HEAD` floor (or classify Detached). Real but out of scope by the
   non-goals ledger; the degradation equals the pre-issue fallback and never
   fails the build. Filed as vybestack/llxprt-jefe#755 ("Build metadata ref
   watching should handle non-UTF-8 Git output").
3. **Windows absolute watch path — pending native CI confirmation; no code
   finding unless CI fails.** Linked-worktree builds emit absolute watch
   paths; cargo accepts absolute paths and this host exercised only relative
   forms (plan ambiguity 2). Native-Windows CI on this PR settles it; a
   divergence there is the plan's stop condition, not a silent fallback.

## Scope ledger

- Every planned file maps to acceptance rows: `build.rs` → A1, A2, A6;
  `build_support/git_watch.rs` → A2–A6; `tests/identity*` → A1–A7.
- Mechanism adaptation recorded 2026-09-05: the A1 fixture points the
  fixture manifest's `build =` at the real `build.rs` instead of
  `include!`-ing it (E0753; see decision 6 and the recorded probe). Same
  file set, same proof strength: cargo compiles the real unmodified file as
  the fixture's build script.
- Pre-approved by the issue directive: the small pure build-support seam
  (`build_support/`, no dependencies, no public runtime abstraction).
- Confirmed unnecessary: xtask gate configuration changes (checked
  `xtask/src/architecture.rs` and `xtask/src/source_size.rs`).
- Discovered work not in this ledger becomes a follow-up issue; workflow,
  agent-memory, quality-tool, dependency, and `.github/` changes require
  explicit approval regardless of size.

## Review counters

- Open Code Review before PR: 1 of 2 used (run complete on the stable green
  checkpoint; record and dispositions below). At most 1 findings-only
  follow-up remains pre-PR.
- Open Code Review after PR: 0 of 2 used; plan spends at most 1.
- Cap per effort: four runs total, never more than two on either side; no run
  against known-broken code.

## Verification gates (exact, at the green checkpoint)

Corrects the earlier draft's gate list: the draft's aggregate named
`make ci-check`, but this repository has no Makefile (the aggregate is
`cargo xtask ci`), and it named a nonexistent `issue706_owner_evidence`
target. The real target is `tests/issue706_cutover_contracts.rs`. The full
required gates, in order:

1. `cargo fmt --all --check`
2. `cargo build --workspace --all-features --locked`
3. `cargo test --workspace --all-features --locked` (full tests)
4. Coverage gate: llvm-cov line floor 30% (`cargo xtask coverage`, the
   `coverage` step of `cargo xtask ci`)
5. `cargo xtask check source-size`
6. `cargo xtask check architecture`
7. Clippy warnings gate: `cargo xtask lint` (`rustup run stable cargo clippy
   --workspace --all-targets --all-features -- -D warnings` with
   `CLIPPY_CONF_DIR=.github/clippy`)
8. Namespaced complexity clippy: `cargo xtask complexity`
   (`-A clippy::all -A clippy::pedantic -A clippy::nursery
   -D clippy::cognitive_complexity -D clippy::too_many_lines
   -D clippy::too_many_arguments -D clippy::type_complexity
   -D clippy::struct_excessive_bools` with `CLIPPY_CONF_DIR=.github/clippy`;
   lint names keep the `clippy::` prefix, and clippy lints build-script
   targets, so the seam must be clean in both inclusion contexts)
9. `cargo test --test scenario_manifest`
10. `cargo test --test issue704_owner_evidence`
11. `cargo test --test issue705_owner_evidence`
12. `cargo test --test issue706_cutover_contracts`
13. `cargo test --test harness_authority`
14. `git diff --check`

Gates 1–8 run together as `cargo xtask ci` (fmt, clippy-allows,
source-size, architecture, multiplexer-surface, lint, complexity, coverage,
build, test; fail-fast in that order).

Iteration shortcuts: `cargo xtask quick`, and
`cargo test --test identity build_script_fast_forward` for the focused loop.

## Stop conditions

- A slice requires a file outside the planned set (Cargo.toml, xtask, CI,
  consumers) or any dependency or public-abstraction change: stop and get
  approval or file a follow-up.
- Cargo rerun semantics on CI (native Windows especially) diverge from the
  recorded host evidence, for example absolute-path rejection or fingerprint
  granularity flakiness: stop with evidence and narrow to content-only
  assertions before touching the design.
- The fixture test proves flaky under parallel execution despite
  `nested_cargo_lock`: stop rather than adding retry layers.
- Acceptance language admits a materially different architecture than the
  two-line watch list: stop and re-decide.
- Review counters exhausted with blockers outstanding: narrow or split the
  work; do not spend a fifth run.

## Ambiguities requiring approval

1. **Test runtime cost.** The A1 test runs two tiny `cargo build` invocations
   inside the test binary under the existing `nested_cargo_lock` convention
   (precedent: `tests/core/message_bus_contracts.rs`). Estimated single-digit
   seconds; measured 1.15s (RED) and 1.59s for all six focused tests
   (GREEN). The lock helper is a per-target copy inside the test file because
   the shared `tests/support/mod.rs` carries items the identity target never
   uses and the workspace denies dead code; the per-target support copy
   follows the `tests/git_info`/`tests/doctor` convention. The alternative,
   invoking `build.rs` under `rustc` directly, is faster but does not
   exercise cargo's rerun decision, which is the subject of the issue.
   Default: cargo-level. Flag if the test-time budget rejects it.
2. **Absolute watch paths in worktree builds.** When the common dir sits
   outside the package, the watch line must be absolute. Cargo accepts
   absolute paths and this host tolerates them, but only relative forms were
   exercised in the micro experiment. Native-Windows CI at the first slice
   that introduces the contract will confirm; a rejection there is a stop
   condition, not a silent fallback.
3. **build.rs comment rewrite.** The existing comment documents the stale
   behavior; updating it is a production-file edit deferred to the
   implementation phase per the plan-only directive.
