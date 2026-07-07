# Coding Standards

This document defines the Rust language, lint, complexity, formatting, and
documentation standards for Jefe. It consolidates sections 3, 4, and 5 of the
former `dev-docs/project-standards.md` and the full detail of
`docs/project-standards.md`. All thresholds below are pulled verbatim from
`Cargo.toml`, `clippy.toml`, and the gate scripts.

Sibling standards:

- [Architecture Standards](./architecture.md)
- [Testing and Quality](./testing-and-quality.md)
- [Display and UI](./display-and-ui.md)
- [Persistence and Runtime](./persistence-and-runtime.md)

---

## Language and Toolchain

- **Language**: Rust.
- **Edition**: 2024 (per `Cargo.toml`).
- **Formatter**: `rustfmt`. Run `cargo fmt --all --check` before every commit.
- **Linter**: `clippy` with project configuration (`clippy.toml`,
  `.github/clippy/clippy.toml`, `Cargo.toml [lints]`).
- All code must compile with zero warnings under the project's lint
  configuration. Warnings are blockers, not "things to fix later."

---

## Safety and Correctness

| Rule | Detail |
|------|--------|
| `unsafe` | **Forbidden.** `Cargo.toml` sets `unsafe_code = "forbid"`. Non-negotiable. |
| Panic-driven control flow | No `panic!()`, `unreachable!()`, `todo!()`, `unimplemented!()` in production paths (`todo`/`unimplemented` are `deny`). If a path is genuinely unreachable, refactor the types to make it structurally impossible. |
| Error propagation | Use `Result`/`Option` and typed errors. `?`, `.map_err()`, or explicit `match`. Never silently discard an error (avoid `.ok()` unless silent error loss is explicitly intended). |

### `unwrap`/`expect` policy

- No `.unwrap()`/`.expect()` in production paths. The clippy lints
  `unwrap_used`/`expect_used` are `warn` and CI runs with `-D warnings`, so they
  are effectively denied in both production and test code.
- The only `const`-context exception is where the value is provably
  `Some`/`Ok`.
- In tests, use assertion macros or `let-else` extraction with clear `panic!`
  messages (see [Testing and Quality](./testing-and-quality.md)).

---

## Strong Typing

- Prefer explicit domain types (`enums`/`structs`) over primitive obsession.
- Do not use `HashMap<String, serde_json::Value>` or equivalent weak typing.
  Define explicit structs with named, typed fields.
- Do not use `dyn Any` or downcasting for type erasure. Use enums or generics.
- Do not use stringly-typed status or event channels when typed enums exist.
- Public module APIs must have explicit types and clear contracts.
- State and event contracts use enums/structs, not generic blobs.

---

## Lint Configuration (Cargo.toml)

The lint groups and individual lints are enforced in `Cargo.toml [lints]`:

| Category       | Level   | Notes                                |
|----------------|---------|--------------------------------------|
| `unsafe_code`  | `forbid`| Absolute. No unsafe Rust, ever.      |
| Clippy `all`   | `deny`  | (priority -1)                        |
| Clippy `pedantic` | `warn` | (priority -1)                      |
| Clippy `nursery`  | `warn` | (priority -1)                      |

Individual restricted lints:

| Lint                      | Level | Why                                                    |
|---------------------------|-------|--------------------------------------------------------|
| `unwrap_used`             | `warn`| Production code must handle errors, not panic.         |
| `expect_used`             | `warn`| Same as above.                                         |
| `print_stdout`            | `warn`| TUI owns stdout. Use `eprintln!` for diagnostics.      |
| `print_stderr`            | `warn`| Flagged so stderr usage is intentional and reviewed.   |
| `todo`                    | `deny`| No placeholder code in committed code.                 |
| `unimplemented`           | `deny`| Same as above.                                         |

Project-internal allowed lints (scoped exceptions, reviewed):

| Lint                       | Status  | Rationale                           |
|----------------------------|---------|-------------------------------------|
| `needless_pass_by_value`   | `allow` | Pragmatic for internal helpers.     |
| `redundant_clone`          | `allow` | Pragmatic.                          |
| `doc_markdown`             | `allow` | Pragmatic.                          |
| `missing_const_for_fn`     | `allow` | Pragmatic.                          |
| `missing_errors_doc`       | `allow` | Pragmatic.                          |
| `option_if_let_else`       | `allow` | Pragmatic.                          |

These allowed lints are reviewed periodically; they are not a license to add new
allows (see the zero-tolerance clippy-allow policy below).

---

## Complexity Thresholds (clippy.toml)

Defined in `clippy.toml` and mirrored in `.github/clippy/clippy.toml` (the CI
clippy config). The gate `scripts/check-clippy-allows.sh` verifies both files
keep identical thresholds so `CLIPPY_CONF_DIR` never silently falls back to
clippy defaults.

| Threshold                              | Value | Meaning                                       |
|----------------------------------------|-------|-----------------------------------------------|
| `cognitive-complexity-threshold`       | 15    | Functions exceeding this are flagged.         |
| `too-many-lines-threshold`             | 60    | Functions exceeding 60 lines are flagged.     |
| `too-many-arguments-threshold`         | 6     | Functions with >6 args are flagged.           |
| `max-struct-bools`                     | 3     | Structs with >3 bool fields are flagged.      |
| `type-complexity-threshold`            | 250   | Types exceeding this complexity are flagged.  |

These thresholds must not be raised. If a function exceeds a threshold, extract
helpers — do not loosen the rule. (See the lint-guardrail policy in
[Testing and Quality](./testing-and-quality.md).)

---

## Source-File Length Policy

Enforced by `scripts/check-source-file-size.sh` (the `source_file_size` CI job,
also run by `make ci-check`).

| Limit       | Value | Behavior                                              |
|-------------|-------|-------------------------------------------------------|
| `HARD_LIMIT`| 1000  | CI fails. Files over 1000 lines cannot merge.         |
| `WARN_LIMIT`| 750   | Warning only; address before the file hits the limit. |

Scan roots default to `src tests`. When a file approaches the warn limit,
evaluate splitting it — the pure-views pattern (see
[Architecture Standards](./architecture.md)) is the preferred way to extract
cohesive logic.

---

## Zero-Tolerance Clippy-Allow Policy

`scripts/check-clippy-allows.sh` (the `clippy_allow_policy` CI job) enforces a
zero-tolerance policy on clippy allow/expect attributes in first-party code:

- No `#[allow(clippy::...)]`, `#![allow(clippy::...)]`,
  `#[expect(clippy::...)]`, or `#[cfg_attr(..., allow(clippy::...))]` attributes
  in first-party code. `vendor/` is ignored.
- There is **no exception ledger**. If an exception is genuinely required, raise
  it as a design discussion — do not commit it as debt.
- The scanner is Rust-aware (it strips comments, string/char literals, and raw
  strings before matching, and tracks nested brackets), so hiding an allow
  inside a doc string or comment does not bypass it.
- The gate fails closed: if the scanner itself errors (git failure, missing
  files), the policy fails rather than reporting a clean result.

This mirrors the global lint-guardrail policy: never loosen lint or complexity
rules, never add error/type-suppression directives, never raise a complexity or
size threshold.

If a local non-clippy `#[allow]` is genuinely unavoidable:

1. scope it to the smallest possible item,
2. explain why in code,
3. reference a tracking issue.

---

## Formatting

- `rustfmt` is mandatory. Run `cargo fmt --all --check` before every commit.
  Code that does not pass `cargo fmt --all --check` does not merge.
- Prefer to keep formatting changes separate from logic changes when possible.
- Follow existing naming conventions and module organization.
- Keep functions focused; extract helpers when complexity grows.

---

## Explicit DO / DON'T

### DO

- **DO** handle all `Result` and `Option` values explicitly. Use `?`,
  `.map_err()`, `.unwrap_or()`, or `match`. Never silently discard errors
  (avoid `.ok()` unless intentional).
- **DO** use `eprintln!` for diagnostic output. stdout belongs to the TUI
  rendering pipeline.
- **DO** derive `Debug`, `Clone` on data types. Derive `Serialize`/`Deserialize`
  on types that touch persistence.
- **DO** write `#[must_use]` on pure functions that return computed values.
- **DO** keep functions under 60 lines. If a function exceeds this, decompose
  it.
- **DO** keep cognitive complexity under 15. If a function is too complex,
  extract helpers.
- **DO** use explicit match arms on enums. Add the new variant's handler when
  you add a variant.
- **DO** put module-level doc comments (`//!`) at the top of every module file.
- **DO** put doc comments (`///`) on every public function, struct, enum, and
  trait.
- **DO** run `cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace --all-features` before every commit (or `make ci-check`).

### DON'T

- **DON'T** use `unsafe` code. `unsafe_code = "forbid"`. Non-negotiable.
- **DON'T** use `.unwrap()` or `.expect()` anywhere in first-party code.
- **DON'T** use `panic!()`, `unreachable!()`, `todo!()`, or `unimplemented!()`
  in production code paths.
- **DON'T** add `#[allow(clippy::...)]`, `#![allow(clippy::...)]`,
  `#[expect(clippy::...)]`, or `#[cfg_attr(..., allow(clippy::...))]` in
  first-party code. Zero tolerance, no exception ledger.
- **DON'T** use `HashMap<String, serde_json::Value>` or equivalent weak typing.
- **DON'T** use `dyn Any` or downcasting for type erasure.
- **DON'T** use wildcard imports (`use foo::*`) except for the
  `iocraft::prelude::*` import in component files.
- **DON'T** use `print!` or `println!` anywhere. The TUI owns stdout.
- **DON'T** use `dbg!()` in committed code.
- **DON'T** introduce new dependencies without justification. Every
  `[dependencies]` addition must be discussed and approved.
- **DON'T** introduce parallel architecture variants (`*_v2`, `new_*`) unless
  explicitly approved.
- **DON'T** leave `TODO`/`FIXME`/`HACK` placeholders or "for now"/"will be
  implemented" comments in implementation code. If a phase needs stub behavior,
  isolate it to an explicit stub phase.

---

## Documentation Standards

### Module Documentation

Every `.rs` file must begin with a `//!` module-level doc comment explaining:

1. What this module is responsible for.
2. Key types or functions it provides.
3. Any important constraints or invariants.

Example:

```rust
//! PTY session management for embedded terminal views.
//!
//! Each agent gets its own tmux session (persistent backend terminal).
//! The UI maintains a single attached PTY viewer and re-attaches it to the
//! currently active agent's tmux session as selection changes.
```

### Public API Documentation

Every public struct, enum, function, and trait must have a `///` doc comment
describing:

- **What** it does (not how).
- **Parameters** and their constraints.
- **Return value** semantics.
- **Panics** (if any — which should be none in production code).

### Code Comments

- Comments explain **why**, not **what**. If the code needs a comment explaining
  what it does, the code should be refactored to be self-explanatory.
- Comments must not be used to talk to other developers or describe changes. Use
  commit messages for that.
- Dead/commented-out code is not acceptable. Delete it. Git history exists.
- `TODO` comments trigger a lint warning and must be resolved before merge.

---

## Review and Merge Quality Bar

A change is mergeable only when all are true:

1. Behavior aligns with functional/technical specs.
2. Module boundaries remain intact (see [Architecture Standards](./architecture.md)).
3. Lint, complexity, formatting, and source-file-length standards pass (see
   [Testing and Quality](./testing-and-quality.md) for the exact gates).
4. Tests cover the change and pass.
5. Documentation is updated when contracts change.
6. No prohibited patterns were introduced (`unwrap`/`expect`, `unsafe`,
   clippy-allow attributes, weak typing, parallel architectures, placeholder
   markers).
