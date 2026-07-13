# Tutorial Capture Workflow

The tutorial-capture workflow is a reusable, agent-driven documentation
production system built around the existing tmux harness. It enables a
documentation author (or automated agent) to exercise Jefe's real TUI through
the same happy-path interactions taught by the tutorial, capture semantic
checkpoints, and produce reproducible monochrome preview artifacts.

This is a documentation-production workflow, not a replacement for deterministic
unit, integration, or TUI regression tests.

## Platform support

This workflow is **Unix-only**. It requires:

- `tmux` for terminal session management.
- `git` for fixture repository provisioning.
- POSIX shell (`/bin/sh`) for shim scripts.
- Unix file permissions (`chmod`) for executable shims.

On non-Unix platforms (Windows), the workflow will fail fast at the `prepare`
step with a clear error message. Use WSL or a Unix container on Windows.

All path handling uses `std::path::PathBuf` and `std::path::Path` with
`join()` for portability within Unix. No raw string concatenation is used for
path construction.

## Architecture

The workflow preserves the existing harness side-effect boundaries:

| Layer | Module | Owns |
|-------|--------|------|
| Typed scenario / pure planning | `tutorial_capture::manifest`, `tutorial_capture::allowlist` | Manifest types, run-ID validation, allowlist/refusal rules, mutation planning |
| Runtime shim planning | `tutorial_capture::path_shim` | Shim script content, controlled PATH computation |
| Persistence boundary | `tutorial_capture::persistence` | Versioned manifest DTO, atomic writes, canonical run-root containment, exclusive creation, cleanup with containment |
| Orchestration | `tutorial_capture::orchestration` | Setup/teardown, git provisioning, manifest persistence delegation, recursive redaction |
| Redaction | `tutorial_capture::redaction` | Full token value redaction, recursive file scrubbing, fixture/private repo redaction |
| Report rendering | `tutorial_capture::report` | Pure Markdown generation from manifest data (Markdown-escaped) |
| GitHub executor | `tutorial_capture::github_executor` | Tier-B command planning, allowlist gating, live execution, scoped cleanup, explicit PR-number merge |
| SVG rendering | `tutorial_capture::svg_render` | Deterministic monochrome preview SVG from screen text with metadata |
| Color SVG rendering | `tutorial_capture::ansi_svg` | Color-preserving SVG from ANSI escape sequences (16/256/RGB + bold/underline) |
| Tmux driver | `harness::tmux_driver` | Owns all tmux process I/O (unchanged, with safe PATH escaping) |
| Scenario runner | `harness::runner` | Scenario execution, polling, artifact capture (unchanged) |

The production `jefe` binary does not contain tutorial-capture logic. The
separate `jefe-tutorial-capture` tool orchestrates runs.

## Safety model

### Manifest-scoped cleanup

Cleanup never trusts arbitrary paths from the manifest. Every owned path
must pass containment validation before removal:

1. **NUL-byte rejection**: paths containing NUL bytes are rejected (path
   injection defense).
2. **Canonical containment**: paths are lexically canonicalized (resolving
   `.` and `..`) and must be contained within the run root.
3. **Exact expected sub-directory**: paths must match one of the known
   resource-kind names (`config`, `artifacts`, `shims`, `fixture-repo`,
   `fixture-clone`). Any other path — even if syntactically contained — is
   refused.
4. **Symlink rejection**: no component of the path may be a symlink,
   preventing symlink-based traversal.
5. **Duplicate rejection**: duplicate paths are rejected.
6. **Production checkout refusal**: if the run root is inside a git repository
   whose `origin` remote points to a known production repository
   (`vybestack/jefe` or `vybestack/llxprt-jefe`), creation is refused. Parent
   directory symlinks (e.g. macOS `/tmp` → `/private/tmp`) are resolved via
   canonical existing-ancestor validation.

### Atomic versioned persistence

The manifest is written atomically (temp file + rename) with a schema
version. On load, the schema version is checked and unknown versions are
rejected. This prevents a stale or attacker-crafted manifest from being
misinterpreted by cleanup.

### Exclusive run-root creation

The run root is created exclusively: if it already exists, an error is
returned. A sentinel file (`.jefe-tutorial-claimed`) is written to mark
ownership. This prevents two runs from sharing the same directory.

### Evidence preservation

By default, cleanup preserves the artifact directory (evidence). Use
`--purge-evidence` to also remove artifacts. The manifest records whether
cleanup was completed and which paths were removed, retained, or already
absent.

### Creation-allowlist provenance (Finding #3)

The manifest records which repositories were in the allowlist when GitHub
resources were created (`creation_allowlist`). Cleanup revalidates every
resource against this creation-time allowlist — if a resource's repo was not
allowlisted at creation time, it is **skipped** and cleanup is **not** marked
complete. This prevents resources created under one allowlist from being
cleaned under a different one.

Skipped or failed cleanup resources **block** cleanup completion and evidence
purge. The manifest is only marked `cleanup_completed` when all resources are
successfully cleaned (or treated as idempotent success).

Already-closed issues and already-deleted branches are treated as
**idempotent success**: if `gh` reports that the resource is already
closed/deleted/not-found, cleanup records it as `Cleaned` rather than `Failed`.

### Full token redaction

Redaction scrubs **full token values**, not just prefixes. The redaction
engine matches token prefixes (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`,
`github_pat_`) followed by 20+ alphanumeric characters and replaces the
entire token with `<token>`. Redaction is recursive across all text files
in the artifact directory and fails typed on I/O errors (fail-closed).
Fixture/private repository names are also redacted from all artifacts.

Redaction is applied on **both success and failure paths**: if a scenario
run fails, artifacts are still redacted before the manifest outcome is set
to `failed`, preventing publication of unredacted artifacts. Symlinks
within the artifact directory are rejected during recursive redaction
(fail-closed) to prevent symlink-based traversal attacks.

### Hostname and timestamp privacy

The tmux harness disables the status bar (`tmux set-option status off`)
for all capture sessions so the hostname, session name, and live clock
never appear in screen captures. As defense-in-depth, the redaction engine
also scrubs known hostnames, common macOS machine names (MacBook, iMac),
and ISO-8601 timestamps from all text artifacts.

### Shell injection prevention

The tmux driver's PATH export uses proper shell escaping
(`shell_escape_single`) rather than raw single-quote interpolation. NUL bytes
are rejected in session names, command argv, and the PATH value. Regression
tests verify that quotes, backticks, and newlines are neutralized.

## CLI usage

The `jefe-tutorial-capture` binary provides subcommands:

### prepare

Create isolated local fixtures and manifest:

```bash
cargo run --bin jefe-tutorial-capture -- prepare \
  --run-id my-tutorial-run \
  --base-dir /tmp/jefe-tutorial-capture \
  --scenario tutorial-capture-local \
  --runtime-profile shim \
  --shim-availability both
```

The default `--base-dir` is `/tmp/jefe-tutorial-capture` (external to the
repository) so artifacts are never written inside the project tree.

The `--shim-availability` flag controls which agent runtime shims are
installed when using the `shim` runtime profile:

- `llxprt-only` — install only the `llxprt` shim; `code-puppy` is not detected.
- `code-puppy-only` — install only the `code-puppy` shim; `llxprt` is not detected.
- `both` — install both shims (default).

For real-runtime profiles (`real-llxprt`, `real-code-puppy`), the detection
PATH uses **curated PATH projection**: the launched process sees only a
curated bin directory containing symlinks to required system tools (sh, git,
tmux, env, id, kill for Tier A; additionally gh for Tier B) and the selected
runtime binary (via shim or real symlink). No inherited PATH directories are
used, so Jefe's startup detection sees only the selected runtime.

**Finding #1**: Required system tools (sh, git, tmux, env, id, kill) are
projected by executable path — even if the source directory also contains
agent binaries, the named tool is symlinked. Agent runtimes are never
projected as system tools. `prepare` fails if sh, git, tmux, env, id, or kill
are not found on PATH. `gh` is required only for Tier B
(`plan-github --confirm-disposable`). The additional tools beyond the
original three (sh, git, tmux) are needed because Jefe's runtime code uses
them when running with a curated PATH: `env` for the agent pane command
prefix (`env -u TMUX ...`), `id` for socket UID resolution, and `kill` for
PID-based liveness checks.

Runtime profiles:
- `shim` — deterministic shim that exposes stable terminal text (default).
- `real-llxprt` — use the real `llxprt` binary if available on the host PATH.
  The executable is validated before the run starts; if it is not found, an
  error is returned rather than silently falling back.
- `real-code-puppy` — use the real `code-puppy` binary if available.
  The executable is validated before the run starts; if it is not found, an
  error is returned rather than silently falling back.

Unknown runtime profile values are an error, not a silent default to `shim`.

The manifest records which profile was used so artifacts are labeled
correctly as shim-backed or real-runtime-backed.

### capture-local

Run the deterministic tmux capture scenario:

```bash
cargo run --bin jefe-tutorial-capture -- capture-local \
  --manifest /tmp/jefe-tutorial-capture/my-tutorial-run/run-manifest.json \
  --scenario dev-docs/tmux-scenarios/tutorial-capture-local.json \
  --jefe-bin target/debug/jefe
```

This launches the real Jefe binary with an isolated config directory and a
controlled PATH (shim directory prepended), drives the scenario, and saves
semantic checkpoint captures to the artifact directory.

The scenario uses `type` steps for form field input (no Enter) and `key Enter`
to submit forms. This prevents premature form submission.

### plan-github

Plan opt-in GitHub fixture mutations (allowlist-gated):

```bash
cargo run --bin jefe-tutorial-capture -- plan-github \
  --fixture-repo fixture/test-repo \
  --run-id my-tutorial-run \
  --dry-run
```

The `--fixture-repo` must match the allowlist. Production repositories are
always refused, even when explicitly listed. The allowlist is built from
independent sources (CLI `--allow-repo` flags, `JEFE_TUTORIAL_FIXTURE_ALLOWLIST` env var,
`--allowlist-file` file) so the target repo can never self-allow. The
`--fixture-repo` target is checked against the allowlist but is never
automatically added to it.

Flags:
- `--fixture-repo <REPO>` — GitHub `owner/repo` for the fixture (required).
- `--run-id <ID>` — Run identifier for unique resource naming (required).
  Must be 1-64 ASCII alphanumeric or hyphen characters. For non-dry-run
  execution, the manifest's run ID must exactly match this value.
- `--allow-merge` — Allow merging the fixture PR (optional).
- `--dry-run` — Plan only; print the mutation plan without executing.
- `--confirm-disposable` — Confirm the fixture is disposable. **Required**
  for actual execution. Without either `--dry-run` or `--confirm-disposable`,
  the command exits with an error.
- `--allowlist-file <FILE>` — Path to an allowlist file (one `owner/repo`
  per line, `#` comments). Merged with CLI and env sources.
- `--allow-repo <REPO>` — Explicitly allow a fixture repo. May be repeated
  to allow multiple repos. Merged with env and file sources. The target
  (`--fixture-repo`) is never auto-allowed; it must appear in at least one
  of these independent sources.
- `--clone-dest <DIR>` — Destination for `gh repo clone`. Must be exactly
  `<run-root>/fixture-clone` (validated with lexical canonicalization). An
  arbitrary external path is **rejected before any mutation** to prevent
  cloning into unmanaged locations.

**Non-dry-run execution uses the typed GitHub executor** — it calls the real
`gh` CLI to create issues, clone, create branches, write changed files,
commit, push, and create PRs. Each created resource is immediately recorded
in the manifest with an atomic save. Optional explicit merge is supported
with `--allow-merge`. **Manual command printing is never used for
non-dry-run execution.**

**No live GitHub mutation during tests**: the typed executor is tested
with an injected fake command runner. Live execution is explicitly opt-in
(requires `--confirm-disposable` and authenticated `gh` context).

### capture-github

Run the opt-in GitHub Issues/PR capture scenario (requires a prepared manifest
and the real jefe binary). The dedicated scenario is at
`dev-docs/tmux-scenarios/tutorial-capture-github.json`:

```bash
cargo run --bin jefe-tutorial-capture -- capture-github \
  --manifest /tmp/jefe-tutorial-capture/my-tutorial-run/run-manifest.json \
  --scenario dev-docs/tmux-scenarios/tutorial-capture-github.json \
  --jefe-bin target/debug/jefe
```

This drives Jefe's Issues and Pull Requests modes against the fixture
repository, capturing semantic checkpoints to the artifact directory. The
local capture scenario (`tutorial-capture-local.json`) does **not** include
Issues/PR checkpoints because the fixture repo has no GitHub configuration
and those modes would show configuration errors. Issues/PR screenshots are
captured only in this dedicated GitHub scenario where the fixture has
proper GitHub setup.

### Tier B state seeding

Before the GitHub capture scenario, the workflow **seeds isolated Jefe state**
via the `state_seed` module: it writes a `settings.toml` and `state.json`
in the run's isolated config directory containing a pre-registered
`Repository` (pointing at the cloned fixture-clone path with the GitHub
repo set) and a `TutorialAgent`. This is **safe setup-boundary seeding**
(the agent and repository exist so the scenario can interact with them),
while the actual tutorial actions (sending issues/PRs to the agent, driving
the merge chooser) are still driven through the Jefe UI via tmux.

The scenario JSON files:
- `tutorial-capture-github.json` — confirm issue and PR send-to-agent,
  assert visible outcomes/agent state. Uses uppercase `S` for send-to-agent,
  with macro-substituted unique agent names and specific chooser/post-send
  assertions.
- `tutorial-capture-github-merge.json` — drive merge chooser/confirm/result
  when merge capture is enabled. This is a separate scenario variant that
  requires two `Enter` keys (first selects confirm, second executes) and
  asserts the merge result is visible.

### Merge authorization

Merge authorization is persisted in the manifest from `plan-github
--allow-merge` as `merge_authorized: true`. The `capture-github` merge
scenario requires this flag to be set — if the scenario file name contains
"merge" and the manifest does not have `merge_authorized`, the command
errors with a clear message. Setup (`plan-github` without `--allow-merge`)
never merges.

### capture-github fixture identity requirement

`capture-github` requires the manifest to have a `fixture_github_repo` set
(from `plan-github --fixture-repo`). If the manifest is missing the fixture
GitHub repository identity, `capture-github` errors with a clear message
directing the user to run `plan-github --fixture-repo` first.

### Capture metadata overwrite

Every capture invocation (`capture-local`, `capture-github`) overwrites the
manifest's capture metadata for the current invocation:
- Binary hash (SHA-256 of the jefe binary under test).
- Scenario hash (SHA-256 of the scenario file).
- Scenario name (from the scenario file stem).
- Theme (preserved if explicitly set, defaults to "dark").

This ensures re-runs with a different binary or scenario are accurately
reflected in the manifest, rather than inheriting stale values from a
prior invocation.

### ANSI capture and artifacts

Each screen capture produces both a plain-text (`.screen.txt`) and an
ANSI escape-sequence (`.screen.ansi`) artifact. ANSI capture and write
errors are fatal — they fail the checkpoint and the run, so missing color
data is never silently swallowed.

The `render` subcommand produces both monochrome SVG and color-preserving
SVG (from ANSI data). Both are registered in the manifest as artifacts
(`Visual` kind for mono SVG, `ColorSvg` kind for color SVG).

### Shim availability selection

The `prepare` subcommand accepts a `--runtime-profile` that controls which
agent runtime shims are created:
- `shim` — creates both `llxprt` and `code-puppy` deterministic shims
  (default). The `ShimAvailability` selection determines which shims are
  injected: `Both`, `LlxprtOnly`, or `CodePuppyOnly`.
- `real-llxprt` — uses the real `llxprt` binary from PATH (no shims).
- `real-code-puppy` — uses the real `code-puppy` binary from PATH (no shims).

### Step::Type macro substitution

The harness macro expansion now substitutes `$param` placeholders in
`Step::Type { text }` steps, not just `Step::Line`, `Step::Key`, and
`Step::Keys`. This allows macro-parameterized text input in scenario JSON.

### Cleanup fail-closed behavior

GitHub cleanup fails closed when GitHub resources exist but the manifest's
creation allowlist provenance is empty — cleanup is refused rather than
risking deletion of resources whose origin is uncertain. Merged PRs and
auto-deleted branches are recognized as idempotent success (treated as
`Cleaned` rather than `Failed`). Partial cleanup outcomes are preserved in
the manifest and local cleanup is not marked complete when any GitHub
resource cleanup fails or is skipped.

### render

Convert screen captures to deterministic SVG images:

```bash
cargo run --bin jefe-tutorial-capture -- render \
  --manifest /tmp/jefe-tutorial-capture/my-tutorial-run/run-manifest.json
```

Reads all `ScreenCapture` artifacts registered in the manifest, renders each
as an SVG with embedded metadata (cols, rows, theme, jefe version, scenario
hash), and writes them to the `artifacts/svg/` sub-directory.

### report

Generate a Markdown evidence report:

```bash
cargo run --bin jefe-tutorial-capture -- report \
  --manifest /tmp/jefe-tutorial-capture/my-tutorial-run/run-manifest.json
```

### cleanup

Remove only manifest-owned resources:

```bash
cargo run --bin jefe-tutorial-capture -- cleanup \
  --manifest /tmp/jefe-tutorial-capture/my-tutorial-run/run-manifest.json \
  --purge-evidence \
  --confirm
```

Cleanup executes manifest-scoped GitHub resource cleanup (closing/deleting
only issues, PRs, and branches recorded in the manifest) before local path
cleanup. The `--confirm` flag is required to perform actual cleanup; without
it, use `--dry-run` to preview what would be cleaned.

Without `--purge-evidence`, the artifact directory is preserved as evidence.

### validate-runtime

Validate a real-runtime profile by launching the real Jefe TUI via tmux
with a curated PATH containing only the selected real runtime. The scenario
opens New Agent (key `n`), waits for "New Agent" and "Agent Runtime", and
asserts the runtime chooser shows the expected runtime label text
(`LLxprt` or `code_puppy`). This is a **real launch+detection validation**,
not just an executable availability check.

```bash
cargo run --bin jefe-tutorial-capture -- validate-runtime \
  --manifest /tmp/jefe-tutorial-capture/my-tutorial-run/run-manifest.json \
  --jefe-bin target/debug/jefe
```

This subcommand is for real-runtime profiles (`real-llxprt` or
`real-code-puppy`). It generates a validate-runtime scenario from the
manifest's runtime profile, launches Jefe with the curated detection PATH,
and records semantic evidence (the validated runtime name) in the manifest
only if the scenario succeeds and the expected chooser artifact is captured.

**Finding #3**: Validation evidence is recorded only after ExitCode success
AND the expected chooser artifact (`validate-runtime-chooser`) is registered
in the manifest. On failure, no success evidence is recorded.

The `capture-local` subcommand is scoped to the shim profile; a real-runtime
profile will be **refused** (error exit) if used with `capture-local`.

## TUI scenario

The deterministic local capture scenario is at
`dev-docs/tmux-scenarios/tutorial-capture-local.json`. It is intentionally not
a CI gate because it requires the real Jefe binary and tmux.

The scenario follows the tutorial's single coherent happy path:

1. Wait for the dashboard and orient.
2. Open the New Repository form and capture the checkpoint.
3. Type the fixture repo name, Tab to the path field, type the path, and
   press Enter to submit.
4. Open the New Agent form and capture the runtime choice.
5. Type the agent name and press Enter to create it.
6. Enter terminal capture and interact with the agent shim.
7. Return to the dashboard.
8. Quit Jefe safely.

Issues and Pull Requests mode checkpoints are **not** included in the local
scenario because the fixture repository has no GitHub configuration and
those modes would show configuration errors. They are captured in the
dedicated GitHub scenario (`tutorial-capture-github.json`).

Each checkpoint follows the pattern: perform input, wait for stable text,
assert expected state, capture with a semantic label.

## Visual artifacts

Two SVG rendering modes are provided:

### Monochrome preview (`tutorial_capture::svg_render`)

A **reproducible monochrome preview** from captured screen text. It is not
color-preserving — it provides a stable monochrome visual artifact that can
be used as a reference for documentation. This provides:

- Stable filenames tied to semantic checkpoint labels.
- Consistent dimensions and dark theme (monochrome: single foreground color
  on a dark background).
- Fixed geometry: cell size (8×16px), padding (16px), font size (14px).
- Fixed font stack: `Courier New, DejaVu Sans Mono, Menlo, monospace`.
- Declared rows from metadata (not actual line count) so the SVG height is
  deterministic even when the capture has fewer lines.
- Geometry, theme, tool version, and scenario hash metadata embedded in
  the SVG as `<desc>` and a comment.

### Color-preserving SVG (`tutorial_capture::ansi_svg`)

A **publication candidate** color SVG that preserves terminal colors from
ANSI escape sequences captured via `tmux capture-pane -e`. This preserves:

- 16-color foreground/background (normal + bright variants).
- 256-color palette (6×6×6 RGB cube + 24-step grayscale).
- 24-bit RGB truecolor (`\e[38;2;R;G;Bm` / `\e[48;2;R;G;Bm`).
- Bold attribute (`\e[1m`) and underline attribute (`\e[4m`).
- Fixed geometry and font stack (same as monochrome).
- Embedded metadata for reproducibility.

The harness captures both plain text (`.screen.txt`) and ANSI-escaped
(`.screen.ansi`) versions at each capture checkpoint. The `render` command
generates both a monochrome SVG and a color SVG when ANSI data is available.

**The color SVG is a publication candidate requiring editorial review.** A
documentation author must verify color fidelity against the original
terminal, choose the clearest images, write transitions, and confirm
accessibility/alt text before publishing.

## Runtime control

The workflow controls agent runtime availability via the `RuntimeProfile`
setting:

- **Shim profile**: A deterministic interactive shim (`llxprt` and
  `code-puppy` executable names) is written to the shim directory. The shim
  exposes stable terminal text and accepts a small known interaction. The
  shim directory is prepended to the inherited PATH, so the launched Jefe
  process sees the shims first.
- **Real-runtime profiles** (`real-llxprt`, `real-code-puppy`): No shims are
  injected. The real runtime executable is validated to exist on the host
  PATH before the run starts. The shim directory is still created and
  prepended to PATH (for system tool discovery), but it contains no shims.
  This means Jefe will detect whichever real runtime is installed.
- The `RuntimeProfile` in the manifest records whether the run was shim-backed
  or real-runtime-backed.
- The binary hash of the Jefe binary under test is recorded in the manifest
  for identity verification.

The shim profile exercises the real Jefe launch/tmux/PTY path. It does not
validate runtime-specific behavior that only the real runtime can provide.
Real-runtime profiles require the actual runtime to be installed.

## Manifest metadata

The run manifest records:

- Run ID (validated: 1-64 ASCII alphanumeric/hyphen characters).
- Jefe version and git commit.
- Scenario name and scenario digest (SHA-256 of the scenario file).
- Terminal geometry (cols, rows).
- Runtime profile (shim / real-llxprt / real-code-puppy).
- Binary digest (SHA-256 of the jefe binary under test — not a
  non-cryptographic hash).
- Theme name.
- Tool versions (tmux, git, gh) as a JSON object.
- Fixture repository paths (local and GitHub).
- Owned local paths (with containment validation).
- Created GitHub resources (issues, branches, PRs).
- Produced artifacts (with semantic labels).
- Observed actions (keybindings and descriptions).
- Discrepancies (unexpected text or state during the run).
- Run outcome (pending / success / failed / partial).
- Cleanup status.

## Manual smoke workflow (Tier B, live GitHub)

This workflow is **opt-in** and requires explicit confirmation. Never run it
against a production repository.

1. Create a disposable fixture repository on GitHub.
2. Add it to the allowlist.
3. Run `plan-github` with `--confirm-disposable` (no `--dry-run`).
4. Run `capture-github` to drive Jefe's Issues and PR modes.
5. Optionally merge the fixture PR with `--allow-merge`.
6. Run `cleanup` to remove only manifest-owned GitHub resources.

All created resources are recorded in the manifest before any later mutation
or cleanup. If any step fails, artifacts and the manifest are preserved.

## Testing strategy

The workflow follows test-first development:

- **Unit tests**: manifest ownership, run-ID naming, allowlist/refusal rules,
  mutation planning, redaction (full token value), artifact naming, cleanup
  selection with containment validation, PATH computation, shim content,
  report rendering, SVG rendering, persistence (atomic writes, schema
  version, containment, symlink/traversal/duplicate/NUL rejection).
- **Orchestration tests**: setup and teardown against temporary local git
  repositories using `tempfile`.
- **CLI tests**: argument parsing for all subcommands including
  `--runtime-profile` and `--purge-evidence`.
- **Adversarial tests**: path traversal, symlink injection, NUL byte
  injection, duplicate paths, production checkout refusal, shell injection
  with quotes/metacharacters/newlines.
- **Guarded tmux integration test**: a real tmux + real Jefe binary test
  that is guarded (skips if tmux or the binary is unavailable).
- **Live GitHub tests**: opt-in only; command-planning tests run in CI, and
  a documented manual smoke workflow against the fixture repository is
  described above.

## Acceptance criteria mapping

| Criterion | Implementation |
|-----------|----------------|
| One command sequence can prepare and run an isolated local documentation capture | `prepare` + `capture-local` subcommands |
| Create repository and agent through the real Jefe UI | `tutorial-capture-local.json` scenario drives the real forms |
| Runtime availability is controllable | `path_shim` module + `env_path` on `TmuxStartRequest` + `--runtime-profile` CLI |
| Opt-in, allowlisted GitHub fixture flow | `plan-github` + `capture-github` subcommands + `allowlist` module + `github_executor` |
| Every path and GitHub resource recorded in a manifest | `RunManifest` with `OwnedPath` and `GitHubResource` entries, atomic persistence, artifact registration after capture |
| Cleanup is manifest-scoped | `cleanup_with_containment()` validates and removes only manifest-owned paths |
| Text captures, visual artifacts, and Markdown report | `capture` steps + `render` subcommand (SVG) + `report` subcommand |
| Artifacts scrubbed of credentials | `redaction` module with full token value redaction |
| Distinguishes shim-backed from real-runtime | `RuntimeProfile` in manifest + binary hash |
| Tutorial author can select a subset | Semantic checkpoint labels in artifact index |
| Workflow and safety model documented | This document |
