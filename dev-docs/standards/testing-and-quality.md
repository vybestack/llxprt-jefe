# Testing and Quality Standards

This document defines the testing discipline, test-layer expectations, assertion
style, coverage floor, and the full verification suite for Jefe. It consolidates
sections 1, 2, and 5 of the former `dev-docs/RULES.md`, section 6 of the former
`dev-docs/project-standards.md`, and the Testing Standards of
`docs/project-standards.md`. All commands and CI jobs below are pulled verbatim
from the `Makefile`, the gate scripts, and `.github/workflows/ci.yml`.

Sibling standards:

- [Architecture Standards](./architecture.md)
- [Coding Standards](./coding-standards.md)
- [Display and UI](./display-and-ui.md)
- [Persistence and Runtime](./persistence-and-runtime.md)

---

## TDD Is Mandatory

Every production change must follow RED -> GREEN -> REFACTOR discipline.

- **RED**: write a failing test for the behavior.
- **GREEN**: implement the minimal code to pass.
- **REFACTOR**: improve design if it increases clarity/maintainability.

No production-only commits without corresponding test intent. No production code
written before a failing test for the behavior it implements.

---

## Test Layers

| Layer                  | What it verifies                                          | Where                         |
|------------------------|-----------------------------------------------------------|-------------------------------|
| Unit                   | Pure logic: state transitions, parsing, normalization, projection functions. | `#[cfg(test)] mod tests` in the module, plus `tests/core/` |
| Integration            | Module boundaries (runtime orchestration, persistence contracts, event-to-message conversion, reducer dispatch). | `tests/core/`, `tests/integration/`, and focused `tests/*.rs` targets (e.g. `git_info`, `github_client`, `selection`) when the lib test budget is tight |
| Regression             | Bug fixes must include a regression test.                 | alongside the fix             |
| TUI harness scenarios  | Real-TTY end-to-end behavior (focus, geometry, alternate-screen, exit). | `dev-docs/testing/tmux-harness.md` + `dev-docs/tmux-scenarios/` |

New harness scenarios use the schema-1 contract (`dev-docs/tmux-scenarios/v1/`,
run by `tmux_scenario`): closed grammar, contained workspace, deterministic
environment, capture shims, secret redaction, and bounded literal `wait`
synchronization — never sleeps or unbounded polling. Legacy adapters,
compatibility shims, or dual-format detection are prohibited; pre-schema
scenarios migrate forward (issue #397). See the "Schema-1 deterministic
real-process harness" section of

### Generic agent-system acceptance

Changes to definitions, planning, or runtime execution require production-route
tests, not helper-only assertions. Cover every shipped definition, operation/
target support, typed defaults, selector normalization, candidate/probe evidence,
physical executable fingerprints, stale generation rejection, preflight zero
effects, immutable argv/env/cwd, local/remote serialization, durable migration,
and live-session restore. UI-visible changes additionally require deterministic
tmux scenarios. `scripts/check-architecture.sh` is a required negative gate: it
rejects legacy agent authority, product branching in runtime/persistence, and
positional shipped-definition authority in production code.
[`tmux-harness.md`](../testing/tmux-harness.md) for the grammar, `HAR-E001` -
`HAR-E007` diagnostics, exit codes, limits, containment, and redaction rules.

### Action-registry acceptance

**Inventory completeness is a gate, not a convention.** A generated golden
enumerates every compiled `(context, chord, action, handler)` row, and the
contract test fails in *both* directions: an inventory row whose handler is not
reachable from production dispatch, and a production handler that appears in no
inventory row. Adding a `HandlerKey` variant without an inventory row does not
compile. Do not "fix" this gate by deleting a row — either the dispatch or the
binding is genuinely missing, and one of them is the defect.

**Platform translation is captured, not assumed.** Scenario evidence records the
original platform event and the canonical chord as separate values, so a
platform-specific normalization (for example Option arriving as Alt) is visible
rather than inferred. Protected recovery is proven on both macOS and Linux.

**Mouse and PTY parity are asserted separately from resolution.** Forwarded
terminal input is asserted as exact PTY bytes taken from the encoder, never
reconstructed from the chord text, which is what makes "registry migration did
not turn forwarded input into an action" a testable claim. Mouse evidence
records frame, cell, resolved hit identity, and emitted action ID so a click and
a keyboard chord are provably the same action through the same availability and
handler path.

Availability assertions compare the reason string across dispatch, Help, footer,
menu, and the Keys editor for byte equality; a stale completion must change
nothing.

### Per-target test registration budget (issue #307)

Clippy's `large_stack_arrays` lint builds a stack array of every test function
pointer registered on each test target. Each target has a **hard ceiling of
2048** registrations and a **soft budget of ≤1900** so new tests can land
without immediately hitting the lint.

| Target kind | Examples | Budget |
|-------------|----------|--------|
| Lib unit tests | `cargo test --lib` | soft ≤1900, hard 2048 |
| Bin unit tests | `cargo test --bin jefe` | soft ≤1900, hard 2048 |
| Integration targets | each `tests/*.rs` file | soft ≤1900, hard 2048 |

When the lib target is near the soft budget, prefer a new coherent integration
target (`tests/<area>.rs` with `#[path]` submodules and `jefe::` imports) over
adding more `#[path]` registrations in `src/lib.rs`. See `tests/git_info.rs` and
`tests/github_client.rs` for the established pattern.

Do **not** raise the threshold or add lint suppressions — split suites instead
(issue #307).

### What tests must verify

- Behavior (inputs -> outputs).
- State transitions and invariants.
- Error and edge paths.
- Integration across real module boundaries (cited `file:line` evidence that the
  runtime path is reachable).

### What tests must avoid

- Pure implementation-detail assertions as primary proof.
- "exists/defined" assertions without behavioral value.
- Mock-only theater for integration claims. Tests that only assert mocks or
  interactions, not behavior, are non-compliant.
- Panic-based control flow for simple variant assertions; prefer
  `assert!(matches!(...))` (see Test Assertion Style below).

---

## Test Assertion Style

These rules apply to all test code. They are enforced by the same clippy lints
as production code (`unwrap_used`/`expect_used` are `warn` + `-D warnings`).

- Prefer `assert!(matches!(value, Pattern), "expected ..., got {value:?}")` over
  `match { Pattern => {} _ => panic!() }` for enum-variant discriminant checks.
- Use `assert_eq!`/`assert!` with descriptive messages for value comparisons.
- **Do NOT** use `assert_matches!` — it is nightly-only and not available on the
  stable toolchain this project uses.
- **Do NOT** use `.unwrap()`/`.expect()` in tests. Prefer `let-else` with a
  clear `panic!` for setup extraction, or assertion macros.
- Reserve bare `panic!` for unreachable extraction-failure branches in
  `let-else`/`match` where a value must be destructured.
- Test function names must be descriptive:
  `test_compose_mode_defaults_to_yolo_and_continue_for_empty_input`, not
  `test1`.
- Use `pretty_assertions` for struct/collection comparisons where available.

---

## What to Test (Coverage Expectations)

| Component     | Test Coverage Expectations                                        |
|---------------|-------------------------------------------------------------------|
| `domain/`     | Type construction, equality, serialization round-trip.            |
| `messages/`   | `AppEvent` <-> `AppMessage` conversion; routing per domain channel. |
| `state/`      | State transitions for every message; navigation bounds; split mode grab/ungrab; form submit with valid/empty input; delete with confirmation flow; kill and relaunch status transitions. |
| `theme/`      | Embedded theme loading, color parsing, hex edge cases, `ResolvedColors` fallbacks, `ThemeManager` set/cycle, external dir loading (empty, nonexistent). |
| `runtime/`    | PTY session lifecycle, key-event-to-bytes encoding, mouse-event-to-bytes encoding, color resolution for named, indexed, spec colors, snapshot construction. |
| `persistence/`| Atomic writes, schema/version validation, safe fallback on malformed/missing files, round-trip. |
| Pure views    | `build_text_box_view`, the action projections, etc. — viewport windowing, caret placement, multibyte safety, zero-width edge cases (see [Architecture Standards](./architecture.md)). |
| Action registry | Chord grammar and bounds, context resolution order, conflict/protection rejection, availability projection equality, override round-trip, offline explain output and exit codes. |

### Integration Tests

- Integration tests that require tmux are acceptable but must be clearly marked
  and skippable in CI environments without tmux.
- Tests must not leave tmux sessions running. Use unique session names and clean
  up in teardown.
- Deterministic and non-flaky. Pin terminal geometry and config directories.

---

## Coverage Floor

The CI coverage gate enforces a minimum line coverage of **30%** via
`cargo llvm-cov --fail-under-lines 30`. The coverage run ignores `vendor/`,
`tmp/`, and `rustc-` paths. This is a floor, not a ceiling — new behavior should
include tests that verify it.

---

## Verification Suite

### Fast iteration (`cargo xtask quick`)

For tight local loops (does not run the full CI gates):

```sh
cargo xtask quick
# = cargo fmt && cargo check -q && cargo test -q
```

### Full pre-merge gate (`cargo xtask ci`)

Run this before pushing. It reproduces every CI gate locally:

```sh
cargo xtask ci
```

`cargo xtask ci` runs, in order:

1. `cargo fmt --all --check`
2. `cargo xtask check clippy-allows` — zero-tolerance clippy-allow policy.
3. `cargo xtask check source-size` — source-file length policy.
4. `cargo xtask check architecture` — architecture boundary policy.
5. Strict clippy (`CLIPPY_CONF_DIR=.github/clippy rustup run stable cargo clippy --workspace --all-targets --all-features -- -D warnings`)
6. Complexity-only clippy pass (stable toolchain): `rustup run stable cargo clippy ... -A clippy::all -A clippy::pedantic -A clippy::nursery -D clippy::cognitive_complexity -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::type_complexity -D clippy::struct_excessive_bools`
7. Coverage gate (`cargo xtask coverage` — stable toolchain `cargo llvm-cov --workspace --all-features --summary-only --fail-under-lines 30`, ignoring `vendor/`, `tmp/`, `rustc-`).
8. `cargo build --workspace --all-features --locked`
9. `cargo test --workspace --all-features --locked`

The xtask runner pins the **stable** toolchain for all clippy and coverage steps
via `rustup run stable` so local results match CI exactly. The
`CLIPPY_CONF_DIR=.github/clippy` prefix points clippy at the CI config mirror.

### CI Jobs (`.github/workflows/ci.yml`)

The CI workflow runs these jobs on every pull request to `main`:

| Job                    | What it does                                                                                  |
|------------------------|-----------------------------------------------------------------------------------------------|
| `fmt`                  | `cargo fmt --all --check`                                                                     |
| `lint`                 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` (`CLIPPY_CONF_DIR=.github/clippy`) |
| `clippy_allow_policy`  | `cargo xtask check clippy-allows` — blocks first-party clippy allow/expect attributes.        |
| `source_file_size`     | `cargo xtask check source-size` — enforces the 1000-line hard / 750-line warn limits.         |
| `architecture_policy`  | `cargo xtask check architecture` — enforces architecture boundary policy.                     |
| `complexity`           | Complexity-only clippy pass denying `cognitive_complexity`, `too_many_lines`, `too_many_arguments`, `type_complexity`, `struct_excessive_bools`. |
| `coverage`             | `cargo xtask coverage` — `cargo llvm-cov --workspace --all-features --fail-under-lines 30`.   |
| `build`                | `cargo build --workspace --all-features --locked` (depends on all of the above).             |
| `test`                 | `cargo test --workspace --all-features --locked` (depends on `build`).                       |
| `windows_native`       | Native MSVC build + psmux smoke + portable policy checks (clippy-allow, source-size, architecture). |
| `tui_smoke` (optional) | Manual/opt-in tmux-backed TUI smoke scenario via `workflow_dispatch`. Skips if tmux unavailable. |

---

## Lint-Guardrail Policy

Never loosen lint or complexity rules, and never add suppression directives.
Specifically forbidden in a diff:

1. New `#[allow(clippy::...)]`, `#![allow(clippy::...)]`,
   `#[expect(clippy::...)]`, or `#[cfg_attr(..., allow(clippy::...))]`
   attributes in first-party code (see the zero-tolerance policy in
   [Coding Standards](./coding-standards.md)).
2. Clippy severity downgrade (`deny` to `warn` or `off`) or a new `allow` entry
   in `Cargo.toml [lints]`.
3. Any increase to a complexity/size threshold (`cognitive-complexity`,
   `too-many-lines`, `too-many-arguments`, `max-struct-bools`,
   `type-complexity`, or any `max:` option in `clippy.toml`).
4. Additions to an ignore block or `SCAN_ROOTS` exclusion that excludes source
   from linting or the source-file-size gate.

The `clippy_allow_policy` and `source_file_size` CI jobs enforce this
mechanically. Fix the underlying issue rather than silencing or loosening the
rule.

---

## Anti-Placeholder Rule

Implementation phases must not leave deferred-completion markers. Forbidden in
implementation code:

- `TODO`/`FIXME`/`HACK` placeholders.
- "for now" or "will be implemented" comments.
- Empty/trivial return behavior used as final implementation.

If a phase needs stub behavior, isolate that to explicit stub phases only (see
[Workflow: PLAN](../workflow/PLAN.md)).

---

## Verification Checklist Before Merge

- [ ] Tests prove behavior for changed requirements.
- [ ] Lint/format/test/coverage gates all pass (`cargo xtask ci`).
- [ ] Source files under 1000 lines (warnings reviewed under 750).
- [ ] No prohibited placeholder/debt markers in implementation.
- [ ] Boundaries remain intact (see [Architecture Standards](./architecture.md)).
- [ ] Docs updated for contract changes.
- [ ] No hidden regressions in keyboard/runtime behavior.

If any box is unchecked, do not merge.
