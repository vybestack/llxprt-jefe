# Issue #424 — Symlink-aware path comparison for work-dir collision detection and delete guard

## Acceptance matrix

| ID | Actor / launch path | Input & boundary cases | Platform | Observable success | Observable failure | Side effects permitted | Persistence / compat | Behavioral test |
|----|---------------------|------------------------|----------|--------------------|--------------------|------------------------|----------------------|-----------------|
| A1 | `local_paths_equivalent` | Two paths that resolve to the same physical dir via symlink | Unix + Windows | Returns `true` | — | None (pure) | No contract change for non-symlink callers | Unit: create temp dir + symlink, assert equivalent |
| A2 | `local_paths_equivalent` | One path via symlink, one direct, both exist | Unix + Windows | Returns `true` | — | None | Backward compatible | Unit |
| A3 | `local_paths_equivalent` | One or both paths do not exist (unmaterialized work dir) | All | Falls back to string-based comparison | — | None | Backward compatible | Unit: nonexistent paths use string comparison |
| A4 | `local_paths_equivalent` | Canonicalize fails on one path | All | Falls back to string-based comparison | — | None | Backward compatible | Unit |
| A5 | Form validation (`check_agent_uniqueness`) | New work_dir collides with existing agent via symlink | All | Agent NOT created; error_message set | — | None | — | Covered by existing form_validation tests using the shared comparison |
| A6 | Delete guard (`work_dir_shared_by_sibling`) | Deleted agent's work_dir shared by sibling via symlink | All | `remove_dir_all` skipped; warn logged | — | None | — | Covered by existing state_ops tests using the shared comparison |

## Non-goals

- Changing `local_paths_equivalent` contract for callers that do not need symlink resolution — the function remains the single shared comparison; it simply canonicalizes when possible.
- Remote repository work dirs (paths refer to a remote host, cannot be canonicalized locally).
- Resolving relative paths that do not exist on disk (best-effort: only existing paths are canonicalized).

## Vertical slices

### Slice 1 — Canonicalizing comparison (single behavior)

- **Acceptance rows:** A1, A2, A3, A4
- **Architecture owner:** `src/services/normalize.rs` (app/domain boundary)
- **Allowed files:** `src/services/normalize.rs`, `src/services/mod.rs` (re-export if needed)
- **RED:** Unit test creating a temp dir, creating a symlink to it, asserting `local_paths_equivalent` returns `true` for the symlink path vs the real path. Also a test for the fallback when paths don't exist.
- **GREEN:** Add `canonicalize_for_comparison` helper using `std::fs::canonicalize`; in `local_paths_equivalent`, attempt to canonicalize both paths; if both succeed, compare canonical forms; otherwise fall back to existing string normalization.
- **Non-goals:** No changes to form validation or delete guard logic — they already use the shared comparison.
- **Verification:** `cargo test -p jefe local_paths_equivalent` + `make quick-check`

### Slice 2 — Update documentation comments (behavior proven)

- **Acceptance rows:** A5, A6 (documentation accuracy)
- **Allowed files:** `src/state/state_ops.rs` (delete doc comment), `src/state/form_validation_issue403.rs` (if needed)
- **GREEN:** Remove the "known limitation" language from doc comments now that symlink resolution is handled.
- **Verification:** `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `make quick-check`

## Scope ledger

| Item | Status | Disposition |
|------|--------|-------------|
| Initial scope | — | Slices 1–2 |

## Review counters

| Phase | Runs | Cap |
|-------|------|-----|
| Pre-PR OCR | 0 | 2 |
| Post-PR OCR | 0 | 2 |

## Verification evidence

- `cargo test --lib -- services::normalize::tests` — 27/27 pass (including `symlinked_paths_to_same_physical_directory_are_equivalent` and `nonexistent_paths_fall_back_to_string_comparison`)
- `cargo test --lib -- state::state_ops state::form_validation_issue403 state::form_build` — all caller tests pass
- `cargo fmt --all` — clean
- `cargo clippy --lib` — no new warnings from changed files (one pre-existing `manual_is_multiple_of` in unrelated `harness/v1/validate.rs` blocks the gate on this machine due to a newer clippy version; confirmed pre-existing on main)
- Full `cargo test`: 2127 pass; the only 4 failures are pre-existing `harness::*` integration tests that require a running psmux server (confirmed failing on main)
