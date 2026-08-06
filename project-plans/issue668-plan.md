# Issue #668 — server identity classification discards the psmux `server_instance` token

Branch: `issue668` (from `origin/main` @ `0b39bd8c`).

`parse_server_identity_output` already returns `ServerIdentity::with_instance(...)`
when psmux answers the `#{server_instance}|#{pid}|#{version}` probe with a
namespace token. `server_health_io::classify_observation` then rebuilds the
current identity with `ServerIdentity::new`, which hardcodes `instance: None`,
so the token never reaches `classify_server_health`. Its token-decisive branch
is therefore dead on the production Windows path and every verdict falls
through to the weaker `pid + started_at` comparison — the comparison that
produced the impossible `Replaced` recorded in #664.

## 1. Acceptance matrix

| # | Actor / launch path | Inputs and boundary cases | Observable success | Observable failure + diagnostic location | Side effects allowed | Behavioral proof |
|---|---|---|---|---|---|---|
| A1 | Windows liveness cycle → `observe_server_liveness` → `classify_observation` | Probe stdout carries a non-empty `#{server_instance}` field; the answering pid is live so the OS supplies a real creation discriminator. Boundary: blank instance field (a multiplexer predating psmux#509) | The observed identity carries **both** the parsed `ServerInstanceToken` and the OS-resolved creation discriminator | A blank instance field yields `instance: None` and the pre-existing process-identity evidence only — never a synthesised token | None beyond the existing probe | `src/runtime/server_health_io_tests.rs` — pure `resolve_observed_identity` cases plus a Windows live-pid `classify_observation` case |
| A2 | `classify_resolved_identity` (pure seam, token now present) | Same token + different pid; same token + different pid *and older* creation discriminator | `Healthy` — psmux answers from whichever per-session server of the namespace replied, so a different answering pid within one namespace is not a restart (#540) | A `Replaced`/`ConflictingIdentity` verdict here is the phantom-agent defect: agents under a live namespace would be declared `ServerLost` | None. The identity is pinned as healthy | `src/runtime/server_health_io_tests.rs` |
| A3 | `classify_resolved_identity` (pure seam, token now present) | Different token + same pid, at the same creation discriminator and at a strictly newer one | A strictly newer process under a different namespace token is `Replaced` even though the pid was reused | An identical process (same pid, same creation discriminator) reporting a *different* token is contradictory evidence → `ConflictingIdentity`, warned from `server_health_io.rs` | None on the conflicting case; pin update on the genuine replacement | `src/runtime/server_health_io_tests.rs` |
| A4 | `classify_resolved_identity` (pure seam, token on one side only) | Token on the prior only; token on the current only. Each with matching and with differing process identity | The comparison falls back to `pid + started_at`: matching → `Healthy`, differing → the #664-guarded `Replaced`/`ConflictingIdentity` | A one-sided token must never be treated as decisive in either direction (neither an automatic match nor an automatic mismatch) | None | `src/runtime/server_health_io_tests.rs` |
| A5 | `classify_resolved_identity` — interaction of the two rules | Different tokens with a *non-monotonic* creation discriminator (the #664 shape, now on the token path) | The #664 monotonicity guard still refuses it: `ConflictingIdentity`, not `Replaced` | The token rule must not promote a non-monotonic answer to `Replaced`, and the guard must not demote a same-token answer away from `Healthy` | None | `src/runtime/server_health_io_tests.rs` |

### Boundary decisions recorded

- The token is preserved at the point the OS-resolved process identity is
  merged with the parsed answer. The parser owns the namespace token (the OS
  cannot know it); the OS owns the creation discriminator (the parser
  hardcodes a placeholder `1`). Keeping only one half is what made the
  decisive branch dead.
- The two rules compose in one direction only, and deliberately:
  `classify_server_health` decides *whether* the identities differ (token
  first, process identity as fallback), and the #664 guard in
  `classify_resolved_identity` then decides whether a claimed difference is
  *orderable*. A same-token answer never reaches the guard, so an older
  sibling server under one namespace stays `Healthy`.
- A blank `#{server_instance}` field remains the capability signal for a build
  predating psmux#509 and still yields `instance: None`. No token is invented.

## 2. Non-goals

- Making the `display-message` identity probe authoritative under concurrent
  servers. Choosing *which* server answers is a separate problem from
  correctly classifying the answer (issue #668 non-goal, related #540).
- Changing `classify_server_health`, `parse_server_identity_output`, or the
  `ServerInstanceToken` type. All three are already correct; the defect is
  strictly the dropped field on the I/O path.
- Changing `ServerLivenessObservation`, `plan_server_cycle`, or any agent
  state transition. The verdict set is unchanged; only the evidence feeding it
  is restored.
- Persisting the namespace token across jefe restarts.
- Coverage-gate work (#663) and observability work (#662).

## 3. Slices

| Slice | Acceptance rows | Owner / boundary | Allowed paths |
|---|---|---|---|
| S1 | A1 | Runtime server-health I/O boundary | `src/runtime/server_health_io.rs`, `src/runtime/server_health_io_tests.rs` |
| S2 | A2–A5 | Runtime classification (pure seam) | `src/runtime/server_health_io_tests.rs` |

Both slices stay inside one architectural layer and one orchestration route,
so no child split is required.

## 4. Scope ledger

| Entry | Status | Justification |
|---|---|---|
| Crate-internal `resolve_observed_identity` helper in `server_health_io.rs` | Accepted | Required by A1. It is the exact expression the issue names as defective, lifted to a `pub(super)` pure function so token preservation has behavioral evidence on every platform rather than only where a live-pid probe succeeds. Mirrors the existing #664 split of `classify_resolved_identity`; no new public API. |
| `project-plans/issue668-plan.md` | Accepted | Required by the delivery workflow. |

## 5. Review counters

- Local review runs used: 1 / 2
- Post-PR OCR runs used: 1 / 2

### Local run 1 — `ocr review --from 0b39bd8c --to dd974734` (v1.8.8, stepfun/step-3.7-flash)

Coverage: `complete_best_effort` — 2 selected, 2 completed, 0 failed, 0 waived.

| Finding | Validity | Disposition |
|---|---|---|
| `server_health_io_tests.rs:274` (maintainability, low): the `observed` test helper panics through `let ... else` instead of `expect` | Partial — the panic is real, but the reviewer itself records it as acceptable and names no correctness or coverage defect | Reject — `expect` panics identically with a strictly less informative message, whereas `panic!("...got {stdout:?}")` names the offending probe answer; threading `Result` through a test helper adds noise without changing observable behavior. The no-`unwrap`/`expect` rule governs production paths, which this helper is not. |

### Post-PR run 1 — `ocr review --from 0b39bd8c --to 1803da92` (v1.8.8, stepfun/step-3.7-flash)

Coverage: `complete_best_effort` — 2 selected, 2 completed, 0 failed, 0 waived. **0 findings**, so nothing to triage. This run covers the final head including the test refactor, which means the sole finding from local run 1 was not re-raised. Its disposition stands unchanged.

CI-side reviews on the same head — OpenCodeReview, CodeRabbit and the LLxprt review — all reported success, and `pr.reviews --actionable` returns no unresolved threads.

## 6. Verification evidence

- [x] `cargo xtask` gates — `fmt`, `lint` (strict `clippy --workspace --all-targets --all-features -D warnings`), `check clippy-allows`, `check source-size`, `check architecture`, `complexity`, `build` all clean.
- [x] `cargo test --workspace --all-features --locked` — 0 failures.
- [x] coverage gates — `cargo xtask coverage`: total 71.21% lines against the 30% floor; `src/runtime/server_health_io.rs` rises to 84.75% lines / 82.16% regions from the documented 0/82 baseline. The first attempt aborted on `harness::tmux_driver::tests::real_psmux_runs_a_stable_native_process_when_available`, a real-psmux session start that timed out under llvm-cov instrumentation; it passes both unmutated and on the clean rerun, so it is environmental and unrelated to this change.
- [x] `cargo xtask coverage-windows` per-module floors — every configured module clears its floor, no violations. `src/runtime/server_health_io.rs` reports 37.28% (44/118 lines) against its floor of 0 under the narrower Windows-only selection.
- [x] CI on the exact head — PR #673 @ `1803da92`: 19 checks pass, 0 fail, 2 skipped (the tmux smoke and the main flake baseline, neither of which runs on a PR). The first push failed `Lint (clippy)`, `Complexity checks` and `Coverage gate` on a single Linux-only diagnostic — `constant NAMESPACE_B is never used`, because the new tests re-spelled the tokens as literals so that constant had only a `#[cfg(windows)]` reader. Fixed at the source by rendering probe answers from the constants through an `answer` helper rather than by suppressing the lint.
- [x] Ancestry / conflict check — `git merge-base --is-ancestor origin/main issue668` held at `0b39bd8c`; `origin/main` has since advanced to `8538fdd1`, and `git merge-tree --write-tree origin/main issue668` reports no conflicts. The branch is cut directly from `main`, not stacked on another PR.

The Windows floor for `src/runtime/server_health_io.rs` stays at 0 on purpose: raising it is quality-tooling work owned by #663 and needs approval. This change only adds coverage, so the gate cannot regress.
