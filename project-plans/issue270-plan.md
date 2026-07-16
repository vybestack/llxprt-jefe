# Issue #270 — Launch selectable Code Puppy versions with uvx

## Goal
Add a persisted, runtime-specific Code Puppy package version to agents and a
copy-on-create repository default. Blank retains direct `code-puppy`; nonblank
uses structural argv equivalent to
`uvx --from code-puppy==VERSION code-puppy EXISTING_ARGS` locally and safely
shell-escaped argv remotely.

## Decisions
- Versions are trimmed opaque strings. Jefe does not duplicate Python package
  version parsing; `uvx` owns validation.
- Blank is the only direct-launch sentinel.
- Hidden runtime-specific drafts survive Agent/Default Agent switching.
- Repository defaults are copied into new persistent and transient Code Puppy
  agents. Later repository edits never mutate existing agents or bindings.
- A pinned launch requires `uvx`, not a globally installed `code-puppy`.
- Capability probes invoke the exact selected package with `--help`; uv
  resolution followed by import/execution failure remains a launch failure.
- Persistence remains schema v1 through serde defaults for missing fields.

## Acceptance matrix

| ID | Path/input | Target | Observable success | Failure/side effects | Evidence |
|---|---|---|---|---|---|
| A1 | Agent Version blank | Local/remote | Existing direct `code-puppy` argv and detection are unchanged | Existing diagnostics unchanged | Runtime command/probe regression tests |
| A2 | Agent Version nonblank | Local | Structural `uvx`, `--from`, `code-puppy==VERSION`, `code-puppy`, then existing arguments | Missing uvx or child/package failure is surfaced | Launch-plan and capability tests |
| A3 | Agent Version nonblank | Remote | Global Code Puppy is not required; uvx is required; every argument is safely quoted | SSH/uvx/package errors identify the selected launch | Remote plan/probe tests |
| A4 | Model, YOLO, quick resume, interactive and fresh prompt args | Local/remote | Wrapper prefix changes only; all existing Code Puppy args remain ordered and intact | No LLxprt args leak | Table-driven argv tests |
| A5 | Restart/reattach/relaunch/issue send/PR send | Local/remote | Runtime binding and every derived/fresh signature retain the selected version | Existing mismatch/failure handling remains | Binding, relaunch and fresh-signature tests |
| A6 | New/Edit Code Puppy agent form | TUI | Version is visible, editable, persisted and restored | Invalid existing required fields behave unchanged | State/projection/UI tests and harness |
| A7 | New/Edit LLxprt agent form | TUI | Code Puppy Version is hidden and skipped; LLxprt Version remains visible | Hidden draft is retained, never launched | Focus/runtime-switch tests and harness |
| A8 | Repository Default Agent Code Puppy | TUI | Default Version is visible, editable and persisted | Existing validation remains | State/projection/UI tests and harness |
| A9 | Repository Default Agent LLxprt | TUI | Code Puppy default is hidden/skipped and retained dormant | No uvx probe for LLxprt | Focus/runtime-switch tests |
| A10 | New persistent/transient Code Puppy agent | State/send | Repository default is copied once | Later default edits do not mutate the agent | Creation/transient tests |
| A11 | Legacy state missing fields | Persistence | Missing values deserialize blank and round-trip | No migration/schema bump | Compatibility tests |
| A12 | Pinned capability probe | Local/remote | Exact selected package receives `--help` | Resolution/import/execution nonzero is faithfully reported | Capability/probe tests |
| A13 | Version containing whitespace/metacharacters | Local/remote | Outer whitespace is trimmed; value remains one opaque package argument and is safely escaped remotely | uvx may reject invalid syntax | Structural and hostile-value tests |

## Explicit non-goals
- Installing or updating uv/uvx, PATH bootstrapping, retries, caching, registry
  browsing, or a package-manager UI.
- A generic public one-shot package-runner framework.
- Python version-spec validation in Jefe.
- Mutating existing agents when repository defaults change.
- Persistence schema migration, dependency, workflow, quality-tool, or unrelated
  refactor changes.

## Bounded vertical slices

### Slice 1 — TUI, forms, persistence, copy-on-create
Acceptance: A6-A11. Add the TUI scenario first and prove RED. Add serde-defaulted
domain/form fields, runtime-specific focus/rendering, create/edit mapping, and
copy-on-create behavior. Allowed layers: domain, state forms, pure selection,
existing UI screens, persistence tests, one harness scenario. Stop if this
requires a schema migration or generic form framework.

### Slice 2 — Canonical launch planning
Acceptance: A1-A4, A13. Add failing command-plan tests, then extend existing
launch-target planning with uvx while preserving `code_puppy_launch_args` as the
inner argument builder. Prove structural local/Windows transport and remote
escaping. Stop if a new public process abstraction is needed.

### Slice 3 — Availability, capabilities and diagnostics
Acceptance: A2, A3, A12. Add failing exact-version help and remote availability
tests. Reuse existing probe/preflight boundaries: pinned requires uvx and exact
package execution; blank keeps global Code Puppy behavior. Stop before adding
installation, retry, timeout, or cache subsystems.

### Slice 4 — Lifecycle/send propagation
Acceptance: A5, A10-A12. Prove startup coherence, relaunch, issue/PR fresh
signatures, and transient sends retain the pin. Run the harness and complete
verification.

## Expected paths / scope ledger

Planned production ownership:
- `src/domain/mod.rs`
- `src/state/form_types.rs`, `form_projection.rs`, `form_ops.rs`, `form_build.rs`,
  `form_runtime.rs`, `modal_ops.rs`
- `src/selection/form_content.rs`
- `src/ui/screens/new_agent.rs`, `new_repository.rs`
- `src/services/mod.rs`, `src/app_init.rs`
- `src/runtime/commands.rs`, `capabilities.rs`
- existing app-input availability/probe/preflight/send modules only as required

Planned evidence:
- Existing sibling unit/integration test modules, `tests/issue270_behavior.rs`
  if cross-layer evidence is clearer there
- `dev-docs/tmux-scenarios/code-puppy-version-fields.json`

Conditional and approval-required:
- Touch executable resolution/package-probe internals only if their existing
  private contracts cannot represent uvx.
- Any new dependency, public abstraction, migration, workflow/tool change, or
  unrelated refactor requires approval.

Budget target was at most 25 files / 1,500 net lines. The mandatory scope review
found that adding the required public `LaunchSignature` field forces fixture
updates across 27 literal-bearing files, projecting 50-55 changed files. On
2026-07-15 the owner explicitly approved one PR and lifted the 40-file limit for
this issue. The 2,500-net-line hard stop and all architecture/non-goal boundaries
remain in force.

## Review counters
- Local OCR: 1/2
- PR OCR: 1/2

## Verification evidence
- TUI RED: scenario added before Slice 1 production behavior and failed on the missing fields.
- Focused tests: `cargo test --test issue270_behavior` passed 7/7; runtime capability tests passed independently.
- `make quick-check`: passed after rebasing onto `origin/main`.
- `make ci-check`: passed with the coherent rustup 1.97 toolchain after correcting local Homebrew tool shadowing.
- TUI GREEN: `code-puppy-version-fields.json` passed all 54 steps.
- CI/native Windows: exact-head PR checks passed, including Build, Test,
  Coverage, strict Clippy, complexity, source length, formatting, allow policy,
  and native Windows; optional TUI smoke skipped by workflow policy.

## Scope review
The candidate contains 60 files after splitting the cohesive pinned-command test
module to satisfy the 1,000-line hard source limit. Net changed lines remain
below both the 1,500-line target and 2,500-line hard stop. The file-count growth
is fixture propagation from the approved persisted `LaunchSignature` field plus
the required TUI plan/scenario/evidence. One atomic green commit is required:
splitting the public struct change from its Rust literals would create knowingly
unbuildable intermediate commits.

## Review triage
Local OCR run 1:
- In-scope—Fix: added whitespace-only blank-sentinel coverage, replaced a weak
  shell-string substring assertion with exact structural argv evidence, and
  restored the no-`cd` remote-probe rationale.
- Reject: claims that POSIX single quoting permits dollar/backtick/newline
  execution; the real-shell hostile-value test proves exact argv and no side
  effect.
- Reject: duplicate version labels; runtime visibility makes the two rows
  mutually exclusive.
- Reject: pinned-without-YOLO probing as a regression; A12 requires exact
  selected-package execution independently of YOLO validation.
- Reject: Windows remote-shell coverage; SSH targets the remote POSIX shell,
  while local Windows wrapper/structural argv behavior has dedicated coverage.
- Reject: hard-index style nit after replacing it with full-vector equality.

PR OCR run 1:
- Reject: three duplicate claims of an incomplete package-probe rename; the
  launch-aware wrapper intentionally delegates LLxprt to the existing npm probe,
  and exact-head all-target CI compiles successfully.
- Reject: raw form version propagation; availability trims at its predicate and
  launch/persistence boundaries normalize independently.
- Reject: remote uvx fallback request; A3/A12 require uvx and the exact package,
  with pre-side-effect availability and capability probes.
- Reject: two duplicate transport-abstraction requests; shared structural argv
  already feeds direct local execution and required SSH shell serialization.
- Reject: private remote-field helper exhaustiveness concern; its exhaustive
  caller owns focus routing and fails compilation when variants are added.
- Reject: explicit initialization of all default form fields; canonical Default
  plus runtime-specific overrides avoids duplicated type policy.
- Reject: trimming the legacy model merely to match the new version field;
  version trimming is explicit acceptance behavior and model changes are out of
  scope.
- All ten threads received in-thread replies and were resolved.

## Deferred findings / follow-ups
None.
