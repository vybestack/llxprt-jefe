# Issue #337 — Add latest and latest nightly to llxprt code runners

## Goal
Allow users to type `latest` or `latest nightly` in any version field (Code
Puppy agent/default or LLxprt agent/default) and have it resolve at launch
time to the latest published release or nightly build. This is **in addition
to** providing an explicit version string — the sentinels do not replace
existing behavior.

## Background / registry facts

### LLxprt (npm: `@vybestack/llxprt-code`)
- npm dist-tag `latest` → `0.9.3` (latest stable release)
- npm dist-tag `nightly` → `0.10.0-nightly.260715.30de44adb` (latest nightly)
- npm natively resolves `@vybestack/llxprt-code@latest` and
  `@vybestack/llxprt-code@nightly`.
- The existing opaque selector already passes `latest` straight through to
  `@vybestack/llxprt-code@latest`, which npm resolves.
- The sentinel `latest nightly` must be mapped to the npm dist-tag `nightly`.

### Code Puppy (PyPI: `code-puppy`, launched via `uvx`)
- PyPI has no dist-tag concept and no nightly builds for `code-puppy`.
- `uvx --from code-puppy code-puppy` resolves to the latest published version.
- `uvx --from code-puppy==VERSION code-puppy` pins to an exact version.
- `code-puppy==latest` is **invalid** — uv rejects non-numeric version
  operands.
- Therefore both `latest` and `latest nightly` for Code Puppy must produce the
  bare package name `code-puppy` (no `==VERSION` suffix), letting uv resolve
  the latest available release.

## Decisions
- Sentinels are case-insensitive after trimming: `Latest`, `LATEST`,
  `Latest Nightly` all normalize to the same meaning.
- Sentinel recognition is a domain-layer concern; spec generation is
  centralized so launch planning, capability probes, and package probes all
  agree.
- `latest` maps to:
  - LLxprt npm spec: `@vybestack/llxprt-code@latest` (native dist-tag)
  - Code Puppy uvx spec: bare `code-puppy` (uv resolves to latest)
- `latest nightly` maps to:
  - LLxprt npm spec: `@vybestack/llxprt-code@nightly` (native dist-tag)
  - Code Puppy uvx spec: bare `code-puppy` (PyPI has no nightly channel)
- Explicit version strings (e.g. `0.0.361`, `0.10.0-nightly.260712.21cb698b6`)
  are unchanged: `code-puppy==VERSION` for uvx, `@vybestack/llxprt-code@VERSION`
  for npm.
- Persistence schema is unchanged — sentinels are stored as plain strings.
- Blank still means "direct binary launch" (no uvx/npm wrapper) for both
  runtimes.

## Acceptance matrix

| ID | Path/input | Target | Observable success | Failure/side effects | Evidence |
|---|---|---|---|---|---|
| A1 | LLxprt selector `latest` | Local/remote | npm spec is `@vybestack/llxprt-code@latest`; npm resolves it; launch and probe use the same spec | Existing diagnostics unchanged | Domain + npm launch tests |
| A2 | LLxprt selector `latest nightly` | Local/remote | npm spec maps to `@vybestack/llxprt-code@nightly`; launch and probe use the same spec | Existing diagnostics unchanged | Domain + npm launch tests |
| A3 | Code Puppy version `latest` | Local | uvx argv is `--from code-puppy code-puppy EXISTING_ARGS` (bare package, no `==`) | uvx must be present | Runtime command tests |
| A4 | Code Puppy version `latest nightly` | Local/remote | Same bare `code-puppy` argv; remote is safely escaped | uvx must be present | Runtime command + remote tests |
| A5 | Code Puppy version `latest` | Remote | uvx argv uses bare package; every argument is safely quoted | SSH/uvx errors identify the selected launch | Remote plan tests |
| A6 | Capability probe for Code Puppy `latest`/`latest nightly` | Local/remote | Probe invokes `uvx --from code-puppy code-puppy --help` | Resolution/import failure is faithfully reported | Capability tests |
| A7 | Explicit version `0.0.361` | Local/remote | Existing `code-puppy==0.0.361` behavior unchanged | Existing diagnostics unchanged | Regression tests |
| A8 | Explicit npm selector `0.10.0-nightly...` | Local/remote | Existing `@vybestack/llxprt-code@0.10.0-nightly...` unchanged | Existing diagnostics unchanged | Regression tests |
| A9 | Case-insensitive sentinels | Domain | `Latest`, `LATEST`, `Latest Nightly` all recognized | Trimmed but case-preserved for explicit versions | Domain tests |
| A10 | TUI form | TUI | User can type `latest` or `latest nightly` in version fields; value is persisted and restored | Existing form behavior unchanged | TUI scenario |

## Explicit non-goals
- Installing or updating uv/uvx/npm, PATH bootstrapping, retries, caching.
- Version resolution caching or pre-fetching.
- A version picker / dropdown UI.
- Persistence schema migration.
- Python version-spec validation in Jefe.
- Generic version-spec parsing or semver comparison.
- Changes to how blank versions behave (direct binary launch).

## Bounded vertical slices

### Slice 1 — Domain sentinel recognition and spec generation
Acceptance: A1-A2, A9. Add domain constants, a sentinel predicate, and update
`LlxprtNpmPackageSelector::package_spec()` to map `latest nightly` → `nightly`.

### Slice 2 — Code Puppy uvx spec generation
Acceptance: A3-A5, A7. Add a domain function that maps a Code Puppy version
string to its uvx `--from` spec. Update `launch_target_and_args` and
`code_puppy_help_probe` to use it.

### Slice 3 — Capability probe and package probe consistency
Acceptance: A6, A8. Verify the capability probe uses the centralized spec for
Code Puppy sentinels, and the npm package probe resolves LLxprt sentinels.

### Slice 4 — TUI scenario and full verification
Acceptance: A10. Add/update the TUI scenario to type `latest` in a Code Puppy
version field and verify it is persisted.

## Expected paths / scope ledger

Planned production ownership:
- `src/domain/llxprt_version.rs` — sentinel constants, predicate, spec mapping
- `src/runtime/commands.rs` — use centralized Code Puppy uvx spec
- `src/runtime/capabilities.rs` — use centralized Code Puppy uvx spec

Planned evidence:
- `src/domain/llxprt_version.rs` (inline tests)
- `src/runtime/commands_code_puppy_version_tests.rs`
- `src/runtime/npm_launch_tests.rs`
- `src/runtime/capabilities.rs` (inline tests)
- `src/runtime/package_probe_tests.rs`
- `tests/issue337_behavior.rs` (cross-layer evidence if needed)
- `dev-docs/tmux-scenarios/latest-version-fields.json`

Budget target: under 25 files / 1,500 net changed lines.

## Review counters
- Local OCR: 0/2
- PR OCR: 0/2
