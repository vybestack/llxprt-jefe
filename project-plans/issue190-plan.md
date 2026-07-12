# Issue #190 — Origin-mismatch confirm prompt for issue send-to-agent

## Problem

Most of issue #190 is already implemented (#184 clone-if-missing, #166
dirty-copy guard, clone-URL derivation via `CloneIdentity`). The single
remaining gap is **point #6**:

> Do not auto-clone if `work_dir` exists but is a different/unrelated repo.
> If `work_dir` exists and is a git repo whose `origin` does not match the
> configured repository, surface a confirm prompt (default **no/halt**) rather
> than silently clobbering it.

Today, `ensure_workdir_cloned` (`src/app_input/issue_git_prep.rs`) returns
`Ok(())` as soon as `is_git_workdir(work_dir)` is true — it never inspects
`origin`. So a `work_dir` that is a git repo pointing at a *different* remote
silently proceeds into dirty-check → checkout+pull of the *configured* repo's
default branch on top of a foreign repo — a data/correctness hazard with no
confirm and a confusing downstream error.

## Scope

Issues mode only (`prepare_issue_target` / `prepare_local` / `prepare_remote`
in `src/app_input/issue_prep.rs` are used only by `issues_send.rs`). PR mode
does **not** use this orchestration (it only reuses `write_prompt_to_target`),
so it is out of scope.

## Design

### 1. Make `git_info::parse_origin_url` public

`src/git_info/mod.rs`: change `pub(crate) fn parse_origin_url` → `pub fn`.
It is already the canonical origin-URL normalizer (SSH/HTTPS/bare →
`owner/repo`). The binary crate (`app_input`) needs it; this is the DRY
choice (single source of truth for origin normalization).

**DONE** (already applied on this branch).

### 2. Local origin helpers in `issue_git_prep.rs`

Add pure + git-backed helpers:

- `pub(super) fn origin_shortform(work_dir: &Path) -> Option<String>` — runs
  `git -C <work_dir> remote get-url origin`, trims, normalizes via
  `jefe::git_info::parse_origin_url`. Returns `None` on git failure or when
  the URL is unparseable.
- `pub(super) fn origins_match(actual_url: &str, expected_owner_repo: &str)
  -> bool` — pure predicate: normalize `actual_url` via
  `parse_origin_url`, compare case-sensitively to the trimmed
  `expected_owner_repo`. (Both are already normalized `owner/repo`.)

### 3. Tri-state clone assurance

Extend the clone seam to return an origin-mismatch signal. Replace
`ensure_workdir_cloned` with a richer function (keep the old name for the
existing test seam but add an origin-aware variant):

```rust
pub(super) enum WorkdirAssurance {
    /// Already a git repo with matching origin (or no expected shortform
    /// to compare against). Proceed with the normal prep flow.
    Ready,
    /// Was missing and has just been cloned. Proceed (clone lands on the
    /// remote's default branch, so origin already matches).
    JustCloned,
    /// Exists and is a git repo, but origin does not match the configured
    /// repository. Caller must prompt the user before any destructive action.
    OriginMismatch { actual: String, expected: String },
}

pub(super) fn ensure_workdir_with_origin(
    work_dir: &Path,
    clone_url: Option<&str>,
    expected_shortform: Option<&str>,
) -> Result<WorkdirAssurance, String>
```

Logic:
1. If `is_git_workdir(work_dir)`:
   - If `expected_shortform` is `Some(expected)`:
     - read `origin_shortform(work_dir)`; if it is `Some(actual)` and
       `actual != expected` → return `OriginMismatch { actual, expected }`.
     - if origin is unreadable (no `origin` remote), treat as mismatch too
       (a git repo with no `origin` is not the configured repo).
   - Otherwise return `Ready`.
2. If `path_exists(work_dir)` → `Err` (existing non-git dir).
3. If missing: clone (reuse existing `clone_repository`), then `JustCloned`.

Keep `ensure_workdir_cloned` as a thin wrapper calling
`ensure_workdir_with_origin(work_dir, clone_url, None)` and mapping
`Ready|JustCloned → Ok(())`, so the existing `local_clone_when_missing_with_url`
test still passes unchanged.

### 4. New `PrepOutcome::OriginMismatch` variant

`src/app_input/issue_prep.rs`:

```rust
pub(super) enum PrepOutcome {
    Ready,
    Dirty,
    /// Workdir is a git repo whose origin does not match the configured
    /// repository. Caller must open the origin-mismatch confirm modal.
    OriginMismatch { actual: String, expected: String },
}
```

`prepare_local`: replace
```rust
ensure_workdir_cloned(work_dir, owned_url.as_deref())?;
```
with
```rust
let expected = identity.map(CloneIdentity::owner_repo);  // need to expose owner_repo
match ensure_workdir_with_origin(work_dir, owned_url.as_deref(), expected)? {
    WorkdirAssurance::Ready | WorkdirAssurance::JustCloned => {}
    WorkdirAssurance::OriginMismatch { actual, expected } => {
        return Ok(PrepOutcome::OriginMismatch { actual, expected });
    }
}
```

This requires `CloneIdentity::owner_repo()` to be callable from
`issue_prep.rs`. It is currently `pub(super)` within `clone_identity.rs`
(super = `app_input`). `issue_prep.rs` is also in `app_input`, so
`CloneIdentity::owner_repo` is already accessible via `use super::clone_identity::CloneIdentity`.
**Verify** the visibility works (it should — both modules share the same
parent `app_input`). If not, expose a new `pub(super) fn expected_shortform`
on `CloneIdentity`.

`prepare_remote`: mirror the same logic using a remote predicate probe
(`run_remote_check`) to read the origin URL on the remote host:

- After the existing "exists + is git worktree" detection, if identity is
  present, run a probe that prints the normalized origin shortform (or a
  sentinel when there is no `origin`), compare to `identity.owner_repo()`.
- On mismatch return `PrepOutcome::OriginMismatch`.

The `RemotePrepPlanner::plan` must also be extended to plan an origin-check
op (pure, for tests) — add an `origin_mismatch: bool` field to `PlanInputs`
and plan a no-op short-circuit when true (mirroring `Dirty`+Stop).

### 5. New modal `ConfirmIssueOriginMismatch`

`src/state/types.rs`:

```rust
ConfirmIssueOriginMismatch {
    agent_id: AgentId,
    work_dir: std::path::PathBuf,
    signature: LaunchSignature,
    payload: crate::github::SendPayload,
    actual: String,
    expected: String,
},
```

Mirrors `ConfirmIssueDirtyCopy` but carries the actual/expected origins for
the confirm message. Runtime-only (not in `PersistedState`).

### 6. Wire the modal end-to-end

- `src/app_input/issues_send.rs` `dispatch_agent_chooser_confirm`:
  in the `match prepare_issue_target(...)` add an arm:
  ```rust
  Ok(PrepOutcome::OriginMismatch { actual, expected }) => {
      prompt_origin_mismatch_confirm(
          app_state, ctx, &send_info.agent_id, &send_info.work_dir,
          launch_sig, send_info.payload.clone(), actual, expected,
      );
  }
  ```
  Same in `confirm_issue_dirty_copy_enter` (the Discard path can also hit
  origin-mismatch if the workdir changed between the first send and the
  confirm — though unlikely, handle it by re-prompting rather than failing).

- New `prompt_origin_mismatch_confirm(...)` in `issues_send.rs`: sets
  `state.modal = ModalState::ConfirmIssueOriginMismatch { ... }` and
  persists.

- New `confirm_issue_origin_mismatch_enter(...)` in `issues_send.rs`:
  called when the user presses Enter to opt in. It closes the modal,
  re-checks availability, resolves the target, then **re-runs prep with a
  force-reclone**: remove the mismatched workdir and re-clone from the
  configured identity, then proceed with the normal flow
  (dirty-check is a no-op on a fresh clone → checkout+pull → write prompt →
  launch). Use a new `DirtyPolicy::ForceReclone` OR a dedicated
  `prepare_issue_target_force_reclone` entry point that removes+reclones
  before delegating to the existing prep. Prefer the dedicated entry point
  to keep `DirtyPolicy` semantics clean (it is about dirty handling, not
  about origin).

- `src/app_input/modal_handlers.rs` `handle_mode_confirm_key`: add
  `ConfirmIssueOriginMismatch` to the confirm-key guard (Enter → proceed,
  Esc/n → halt), mirroring the `ConfirmIssueDirtyCopy` block. Extend
  `handle_confirm_enter` to dispatch the new modal to
  `confirm_issue_origin_mismatch_enter`.

- `src/input.rs` `InputMode` derivation: add
  `| ModalState::ConfirmIssueOriginMismatch { .. } => return InputMode::Confirm,`
  alongside `ConfirmIssueDirtyCopy`.

- `src/mouse_routing.rs`: add `ConfirmIssueOriginMismatch` to the two
  `ConfirmIssueDirtyCopy` match arms (lines ~734, ~754) so mouse selection
  is handled consistently.

- `src/ui/orchestration.rs`:
  - `derive_confirm_modal_data`: add an arm producing a
    `ConfirmModalData` with title "Wrong Repository" and a message like
    `"Working copy origin is {actual}, expected {expected}. Replace it with a fresh clone?"`.
  - `build_modal_element`: add `ConfirmIssueOriginMismatch` to the
    `confirm_data.map(...)` arm.

- `src/selection/overlay_content.rs`: the test at ~line 277 builds a
  `ConfirmIssueDirtyCopy`; the new modal will be exercised by a new test
  (no change needed to the existing test).

### 7. Force-reclone implementation (the opt-in action)

When the user opts in to replacing the mismatched repo:

1. `std::fs::remove_dir_all(&work_dir)` (local) or
   `rm -rf <work_dir>` over SSH (remote).
2. Re-clone via the existing clone seam (`ensure_workdir_cloned` with the
   configured `clone_url` — origin now matches by construction).
3. Proceed with `run_local_policy_and_prep` (dirty-check → checkout+pull →
   write prompt).

For remote, the `RemotePrepRunner` needs a `force_reclone` mode. Add a
method `run_force_reclone(work_dir, identity, prompt)` that does
`rm -rf && git clone && (dirty-check n/a) && checkout+pull && write prompt`
in the appropriate `ssh -T` ops.

To keep the entry point uniform, expose from `issue_prep.rs`:

```rust
pub(super) fn prepare_issue_target_force_reclone(
    target: &WorkTarget,
    work_dir: &Path,
    identity: Option<&CloneIdentity>,
    prompt: &str,
) -> Result<PrepOutcome, String>
```

It removes+reclones then delegates to the post-clone prep. No dirty-check
needed (fresh clone is clean). It must still do checkout+pull to sync to the
remote default branch and write the prompt.

## Tests (TDD: RED first, then GREEN)

### Pure unit tests (`issue_git_prep.rs` `tests` module)

- `origins_match` accepts HTTPS, SSH, bare forms of the same `owner/repo`.
- `origins_match` rejects a different `owner/repo`.
- `origins_match` rejects unparseable URLs (returns false when there is no
  expected to compare? — define: `origins_match` compares two already-
  normalized strings; the normalization happens in `origin_shortform`).

### `issue_prep_tests.rs` (local integration with temp repos)

- **origin-mismatch detected**: clone a bare origin A into `work`, then
  call `prepare_local` with an identity whose `owner_repo` is a *different*
  repo → must return `PrepOutcome::OriginMismatch` (NOT `Ready`), and the
  workdir must be untouched (no checkout/pull ran).
- **origin-match proceeds**: clone origin A, identity matches A →
  `PrepOutcome::Ready`.
- **no identity, existing repo**: `prepare_local` with `identity=None` and
  an existing git repo → `Ready` (no origin check when no expected
  shortform — preserves existing #166 behavior).
- **force-reclone replaces mismatched repo**: after an `OriginMismatch`,
  call the force-reclone entry point → workdir now has the configured
  origin, prompt is written, returns `Ready`.
- **happy-path delegation unchanged**: existing clone-when-missing and
  clean-prep tests still pass (regression-safe).

### Remote planner tests (`issue_prep_tests.rs`)

- `PlanInputs { origin_mismatch: true, .. }` → the planner short-circuits
  (no checkout/pull/prompt op planned), mirroring `Dirty`+Stop.
- The remote planner plans an origin-check predicate probe when
  `exists_is_git` and an identity is present.

### Modal/state tests

- New test in `app_input/app_input_tests.rs`: `ConfirmIssueOriginMismatch`
  routes to `InputMode::Confirm` (mirror
  `confirm_issue_dirty_copy_modal_routes_to_confirm_input_mode`).
- New test in `selection/overlay_content.rs`: confirm modal renders the
  actual/expected origin strings.

## Out of scope

- PR mode origin-mismatch (PR send uses a different path).
- Changing the clone URL protocol (HTTPS-only from #184 is preserved).
- Loosening any lint/complexity rule (forbidden; fix root causes instead).

## Verification

`make ci-check` (fmt check + clippy `-D warnings` + build + coverage ≥30% +
test). Then `rustreviewer` review, then PR with `Fixes #190`.
