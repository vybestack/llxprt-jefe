# Issue #261 plan: native Windows repository and GitHub workflows

## Goal

Make local repository, Git, GitHub, issue, and pull-request preparation use
native executable/argv and filesystem boundaries on Windows, while preserving
remote Linux shell plans and Unix behavior.

## RED

1. Add a deterministic TUI scenario for the local dirty-copy preparation and
   confirmation path, with no network operations.
2. Add executable-resolution tests for Windows `PATH`/`PATHEXT`, paths containing
   spaces and Unicode, explicit overrides, and missing executables.
3. Add real-Git integration tests rooted beneath spaces and Unicode that cover
   clone, refresh/default-branch preparation, linked worktrees, dirty detection,
   argv-like filenames, and cleanup.
4. Add pure path-policy tests for mixed/trailing separators, Windows
   case-insensitive comparison without mutating display values, long paths, and
   precise UNC rejection.
5. Add cleanup tests proving no `git clean` is planned, only enumerated untracked
   paths are removed, owned metadata is retained, and filesystem removal errors
   stop before tracked changes are reset.
6. Add GitHub command-boundary tests proving every argument remains a distinct
   `OsString` and the resolved `gh` executable is used.

## GREEN

1. Introduce a typed local-tool executable resolver shared by Git and GitHub CLI
   boundaries, including native Windows `PATHEXT` behavior.
2. Route local Git operations through a typed `GitCommand` boundary with explicit
   argv, noninteractive environment, typed spawn/exit failures, and no shell.
3. Route all GitHub CLI operations through a resolved `gh` command constructor;
   retain existing command-planning/parsing APIs and error categorization.
4. Introduce a local-path policy for comparison and supported-path validation.
   Preserve original `PathBuf` values for display and subprocess arguments;
   reject UNC work directories early with actionable text until they have a
   dedicated behavioral environment.
5. Replace `git clean` with `git ls-files --others --exclude-standard -z` plus
   individually constrained filesystem removals. Surface Windows lock failures
   and do not proceed to `reset --hard` after partial cleanup failure.
6. Keep `issue_prep_remote` Unix command strings unchanged and add regression
   assertions proving that boundary.

## REFACTOR and review

- Keep side effects in Git/GitHub/filesystem boundaries and state transitions
  deterministic.
- Verify repository registration, issue and PR handoff call the new boundaries.
- Audit deletion/reclone ownership and ensure no broad deletion is introduced.
- Run focused Windows and Unix checks, strict Clippy, locked build/test, and the
  full CI verification suite.
