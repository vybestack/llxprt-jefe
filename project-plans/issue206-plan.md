# Issue #206 — Actions workflow filter returns no runs (full path sent to API)

## Root cause (verified empirically)

`build_runs_api_path` in `src/github/actions.rs` builds the filtered runs endpoint
using `percent_encode_path(&filter.workflow_path)`. `percent_encode_path` deliberately
keeps `/` unescaped, so for `workflow_path = ".github/workflows/ocr-review.yml"` the
produced URL path is:

    repos/{owner}/{repo}/actions/workflows/.github/workflows/ocr-review.yml/runs?...

The literal slashes split the path into extra segments, so GitHub routes it as a
different resource and returns **HTTP 404 "Not Found"** (confirmed: `gh api` exits 1,
stderr `gh: Not Found (HTTP 404)`). The reducer's `fail_runs_load` then sets
`actions_state.error` and leaves `runs` empty, producing the observed "no runs".

The GitHub REST API `actions/workflows/{workflow_id_or_filename}/runs` endpoint
accepts the **bare workflow filename** (e.g. `ocr-review.yml`) — verified it returns
200 with 376 runs for the OCR Review workflow.

The "message flashes at the top" is the 404 error banner rendered from
`actions_state.error` while the empty list renders "No workflow runs match filters".
Fixing the filename eliminates the 404, so the flash disappears. The error-handling
plumbing (`fail_runs_load` / `complete_runs_load` clearing `error`) is already sound
and persists errors until a successful reload — no separate flash bug to fix.

## Scope (single file: `src/github/actions.rs`)

### Production change — `build_runs_api_path`

Extract the **bare filename** (last path segment of `filter.workflow_path`) and use
that as the `{filename}` path segment. Keep `workflow_path` semantics everywhere else
(state, cycling, committed filter) unchanged — only the API path construction changes.

Add a small pure helper `workflow_filename(path: &str) -> &str` that returns the
substring after the last `/` (or the whole string if there is no `/`). Use it inside
`build_runs_api_path` so the encoded segment is the filename only.

The `percent_encode_path` helper no longer needs to keep `/` verbatim for the workflow
segment (since we now pass only the filename), but it is also used nowhere else and
MUST NOT be loosened/tightened in a way that affects correctness — keep its behavior,
just feed it the filename. (The `/` passthrough becomes dead for this call site but is
harmless; leave the function as-is to avoid scope creep.)

### Test changes — same file's `#[cfg(test)] mod tests`

1. **Update** `build_runs_api_path_uses_workflow_path_not_name`: this test currently
   asserts the buggy behavior (`path.contains(".github/workflows/ci.yml")`). Change it
   to assert the FIX: the API path contains `/workflows/ci.yml/runs` (bare filename)
   and does NOT contain `.github/` or an encoded `%2F`. Rename to
   `build_runs_api_path_uses_workflow_filename_not_full_path` for clarity.

2. **Add** `build_runs_api_path_uses_filename_for_nested_workflow_path`: with
   `workflow_path = ".github/workflows/ocr-review.yml"`, assert the path contains
   `ocr-review.yml` and does NOT contain `.github` or `workflows/ocr-review.yml`
   (i.e. only the last segment is used).

3. **Add** `build_runs_api_path_filename_without_directory_separator`: when
   `workflow_path` has no `/` (defensive: a workflow path that is already just a
   filename), the whole value is used unchanged.

4. Keep existing `build_runs_api_path_no_workflow_filter` and
   `build_runs_api_path_with_status_filter` passing unchanged.

### Out of scope
- No state-layer changes (`workflow_path` stays the full path for cycling/matching).
- No UI/error-banner changes (the 404 is eliminated at the source).
- No changes to `percent_encode_path`/`percent_encode_query` behavior.

## TDD sequence
1. RED: update test (1) and add tests (2), (3) first; `cargo test build_runs_api_path`
   fails because the current code emits the full path.
2. GREEN: add `workflow_filename` helper and use it in `build_runs_api_path`; tests pass.
3. REFACTOR: keep the helper next to `percent_encode_path` with a doc comment explaining
   the GitHub API requires the filename, not the full path.

## Verification
`make ci-check` (fmt --check, clippy `-D warnings`, coverage >=30, build, test).
