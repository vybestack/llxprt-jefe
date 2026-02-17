# Jefe — Project Standards

## Purpose

This document defines the coding standards, quality bar, and development practices for the Jefe project. These rules apply equally to human contributors and LLM-assisted code generation. Every pull request, every commit, and every generated patch is held to these standards without exception.

---

## Language and Toolchain

- **Language**: Rust, edition 2021.
- **Minimum Rust version**: 1.75.
- **Formatter**: `rustfmt` with project configuration (`rustfmt.toml`).
- **Linter**: `clippy` with project configuration (`clippy.toml`, `Cargo.toml [lints]`).
- **Build profiles**: `dev` (debug symbols), `release` (strip, LTO, single codegen unit).

All code must compile with zero warnings under the project's lint configuration. Warnings are not "things to fix later." They are blockers.

---

## Formatting Rules

The project `rustfmt.toml` enforces:

| Setting                      | Value  |
|------------------------------|--------|
| `edition`                    | 2021   |
| `max_width`                  | 100    |
| `tab_spaces`                 | 4      |
| `use_field_init_shorthand`   | true   |
| `use_try_shorthand`          | true   |

Run `cargo fmt --check` before every commit. No exceptions. No "format later" commits. Code that does not pass `cargo fmt --check` does not merge.

---

## Lint Configuration

### Cargo.toml Lint Levels

The following lint groups and individual lints are enforced in `Cargo.toml`:

| Category       | Level   | Notes                                    |
|----------------|---------|------------------------------------------|
| `unsafe_code`  | `forbid`| Absolute. No unsafe Rust. Ever.          |
| `missing_docs` | `warn`  | Public items should have doc comments.    |
| Clippy `correctness` | `deny` | Likely bugs are build failures.     |
| Clippy `suspicious`  | `warn` | Probable bugs flagged on every build. |
| Clippy `style`       | `warn` | Idiomatic Rust style enforced.       |
| Clippy `complexity`  | `warn` | Overly complex code flagged.         |
| Clippy `pedantic`    | `warn` | Strict quality checks enabled.       |
| Clippy `nursery`     | `warn` | Experimental useful lints enabled.   |

### Restricted Lints (Individual)

These lints are set to `warn` to catch dangerous patterns:

| Lint                        | Why                                                    |
|-----------------------------|--------------------------------------------------------|
| `unwrap_used`               | Production code must handle errors, not panic.         |
| `expect_used`               | Same as above. Use `?`, `map_err`, or `ok()`.          |
| `panic`                     | Application must not abort on recoverable conditions.  |
| `todo`                      | No placeholder code in committed code.                 |
| `unimplemented`             | Same as above.                                         |
| `dbg_macro`                 | Debug macros must not ship.                            |
| `print_stdout`              | TUI owns stdout. Use `eprintln!` for diagnostics.     |
| `print_stderr`              | Flagged so stderr usage is intentional and reviewed.   |
| `wildcard_enum_match_arm`   | Match arms must be explicit for maintainability.       |
| `string_to_string`          | Use `.clone()` on `String`, not `.to_string()`.        |
| `str_to_string`             | Use `.to_owned()` on `&str`, not `.to_string()`.       |
| `clone_on_ref_ptr`          | Use `Arc::clone(&x)` not `x.clone()` for clarity.     |
| `large_include_file`        | Embedded resources must be reasonably sized.           |

### Allowed Exceptions

| Lint                         | Status    | Rationale                             |
|------------------------------|-----------|---------------------------------------|
| `module_name_repetitions`    | `allow`   | Acceptable for clarity in mod paths.  |
| `must_use_candidate`         | `allow`   | Too noisy for internal helpers.       |

### Clippy Thresholds

Defined in `clippy.toml`:

| Threshold                    | Value | Meaning                                       |
|------------------------------|-------|-----------------------------------------------|
| `cognitive-complexity-threshold` | 15 | Functions exceeding this are flagged.         |
| `too-many-lines-threshold`   | 60    | Functions exceeding 60 lines are flagged.     |
| `too-many-arguments-threshold`| 6    | Functions with >6 args are flagged.           |
| `max-struct-bools`           | 3     | Structs with >3 bool fields are flagged.      |
| `type-complexity-threshold`  | 250   | Types exceeding this complexity are flagged.  |

---

## Explicit Do / Don't Rules

### DO

- **DO** handle all `Result` and `Option` values explicitly. Use `?`, `.map_err()`, `.unwrap_or()`, `.ok()`, or match. Never silently discard errors.
- **DO** use `eprintln!` for diagnostic output. stdout belongs to the TUI rendering pipeline.
- **DO** derive `Debug`, `Clone` on data types. Derive `Serialize`/`Deserialize` on types that touch persistence.
- **DO** write `#[must_use]` on pure functions that return computed values.
- **DO** keep functions under 60 lines. If a function exceeds this, decompose it.
- **DO** keep cognitive complexity under 15. If a function is too complex, extract helpers.
- **DO** use explicit match arms on enums. Add the new variant's handler when you add a variant.
- **DO** put module-level doc comments (`//!`) at the top of every module file.
- **DO** put doc comments (`///`) on every public function, struct, enum, and trait.
- **DO** use `to_owned()` for `&str → String` conversion and `.clone()` for `String → String`.
- **DO** use `Arc::clone(&x)` syntax when cloning `Arc` pointers, not `x.clone()`.
- **DO** run `cargo fmt --check && cargo clippy -- -D warnings && cargo test` before every commit.

### DON'T

- **DON'T** use `unsafe` code. The lint level is `forbid`. This is non-negotiable.
- **DON'T** use `.unwrap()` or `.expect()` in production code paths. These are acceptable ONLY in `#[cfg(test)]` blocks and in `const` contexts where the value is provably `Some`/`Ok`.
- **DON'T** use `panic!()`, `unreachable!()`, `todo!()`, or `unimplemented!()` in production code paths. If a code path is genuinely unreachable, refactor the types to make that structurally impossible.
- **DON'T** disable lints with `#[allow(...)]` without a code-review-approved comment explaining why. Every `allow` must have a justification. Blanket `#![allow(...)]` at the module level requires explicit approval and must be temporary.
- **DON'T** use `HashMap<String, serde_json::Value>` or equivalent weak typing. Define explicit structs with named, typed fields.
- **DON'T** use `dyn Any` or downcasting for type erasure. Use enums or generics.
- **DON'T** use wildcard imports (`use foo::*`) except for the `iocraft::prelude::*` import in component files.
- **DON'T** use `print!` or `println!` anywhere. The TUI owns stdout.
- **DON'T** use `dbg!()` in committed code.
- **DON'T** introduce new dependencies without justification. Every `[dependencies]` addition must be discussed and approved.
- **DON'T** use SQLite or any database. Persistence is flat JSON files. This is a deliberate design constraint.
- **DON'T** use bright, light, or high-saturation default color palettes. The default theme is Green Screen (monochrome green on black). All built-in themes must be dark.

---

## Theme and Visual Standards

### Mandatory Defaults

- The default theme is **Green Screen**: `#6a9955` foreground on `#000000` background.
- `#00ff00` (bright green) is reserved for high-emphasis elements only: the running-status indicator and focused borders. It must not be used as general-purpose text color.
- `#4a7035` is the dim/muted color for secondary text, inactive elements, and de-emphasized content.
- All shipped themes must have `"kind": "dark"`. No light themes. No bright default palettes.

### Theme Color Contracts

Every theme JSON must define all color slots listed in the theme file format (see Technical Specification). Missing color slots cause fallback to green-screen values, which may produce visual inconsistency in non-green themes. Theme authors must populate every slot.

### Terminal View Colors

The embedded terminal view remaps ANSI default/named colors to the active theme's palette. Explicit 256-color and RGB colors set by the child process are passed through unmodified. Only the 16 named ANSI colors and the logical Foreground/Background/Cursor colors follow the theme.

---

## Testing Standards

### Unit Tests

- Every module with logic (not just re-exports) must have a `#[cfg(test)] mod tests` block.
- Tests must cover: happy paths, edge cases, boundary values, and error conditions.
- Use `pretty_assertions` for struct/collection comparisons where available.
- Tests may use `.unwrap()` and `.expect()` freely — these are `#[cfg(test)]` contexts.
- Test function names must be descriptive: `test_compose_mode_defaults_to_yolo_and_continue_for_empty_input`, not `test1`.

### What to Test

| Component     | Test Coverage Expectations                                        |
|---------------|-------------------------------------------------------------------|
| `data/models` | Type construction, equality, serialization round-trip.            |
| `events/bus`  | `from_key` mapping, `is_quit`/`is_navigation`/`is_char` queries. |
| `app.rs`      | State transitions for every `AppEvent` variant. Navigation bounds. Split mode grab/ungrab. Form submit with valid/empty input. Delete with confirmation flow. Kill and relaunch status transitions. |
| `presenter/`  | Formatting output for all status variants, elapsed time edge cases, truncation behavior. |
| `theme/`      | Embedded theme loading, color parsing, hex edge cases, `ResolvedColors` fallbacks, `ThemeManager` set/cycle, external dir loading (empty, nonexistent). |
| `pty/`        | Key-event-to-bytes encoding for all key types. Mouse-event-to-bytes encoding. Color resolution for named, indexed, spec colors. Snapshot construction. |

### Integration Tests

- Integration tests that require tmux are acceptable but must be clearly marked and skippable in CI environments without tmux.
- Tests must not leave tmux sessions running. Use unique session names and clean up in teardown.

### Test Commands

```sh
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test module
cargo test --lib app::tests

# Run clippy as a test gate
cargo clippy -- -D warnings
```

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
//! Each agent gets its own **tmux session** (persistent backend terminal).
//! The UI maintains a single attached PTY viewer and re-attaches it to the
//! currently active agent's tmux session as selection changes.
```

### Public API Documentation

Every public struct, enum, function, and trait must have a `///` doc comment. The comment must describe:

- **What** it does (not how).
- **Parameters** and their constraints.
- **Return value** semantics.
- **Panics** (if any — which should be none in production code).

### Code Comments

- Comments explain **why**, not **what**. If the code needs a comment explaining what it does, the code should be refactored to be self-explanatory.
- Comments must not be used to talk to other developers or describe changes. Use commit messages for that.
- Dead/commented-out code is not acceptable. Delete it. Git history exists.
- TODO comments trigger a lint warning and must be resolved before merge.

---

## Code Organization Rules

### Module Structure

- One module per logical concern. Do not put unrelated types in the same file.
- Re-export key types from `mod.rs` for ergonomic imports.
- Keep `mod.rs` files minimal: declarations and re-exports only, not business logic.
- UI components go in `ui/components/`. Screen-level layouts go in `ui/screens/`. Modal overlays go in `ui/modals/`.

### File Size

- Source files should stay under ~400 lines. If a file grows beyond this, evaluate whether it should be split.
- `app.rs` is allowed to be larger because it is the central state machine, but individual methods within it must stay under 60 lines.

### Dependency Direction

```
main.rs → app.rs → data/ (models only)
main.rs → pty/
main.rs → theme/
main.rs → ui/ → presenter/ → data/
ui/ → theme/ (for ResolvedColors)
```

- `data/` depends on nothing project-internal.
- `events/` depends on nothing project-internal.
- `presenter/` depends on `data/` only.
- `ui/` depends on `data/`, `presenter/`, `theme/`.
- `pty/` depends on nothing project-internal (uses iocraft types for Color only).
- `app.rs` depends on `data/`, `events/`.
- `main.rs` wires everything together.

UI components must never call `PtyManager` methods. PTY interaction flows through the root component's event handler.

---

## Git and Commit Standards

### Commit Messages

- First line: imperative mood, ≤72 characters, describes the change.
- Body (if needed): explains **why** the change was made. Not a line-by-line diff summary.
- Reference issue numbers where applicable.

### Branch Hygiene

- Feature branches off `main`.
- Squash-merge or rebase-merge to `main`. No merge commits with trivial content.
- Delete feature branches after merge.

### Pre-commit Checklist

Every commit must pass:

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

No commit should introduce new warnings, failing tests, or formatting violations. CI enforces this. Local verification before push is expected.

---

## Review Quality Bar

### For Human Reviewers

- Verify lint compliance (CI catches this, but reviewers should understand the lint rationale).
- Verify new code has tests.
- Verify public API has doc comments.
- Verify no new `#[allow(...)]` without justification.
- Verify no `.unwrap()` / `.expect()` outside `#[cfg(test)]`.
- Verify the change does not break the module dependency DAG.
- Verify theme changes maintain dark-first, green-screen-default invariants.

### For LLM-Generated Code

All standards in this document apply identically. Additionally:

- LLM-generated code must not introduce crates not already in `Cargo.toml` without explicit human approval.
- LLM-generated code must not add `#[allow(...)]` annotations.
- LLM-generated code must not use `unwrap()`, `expect()`, `panic!()`, `todo!()`, or `unimplemented!()` in non-test paths.
- LLM-generated code must not generate placeholder implementations (empty function bodies, stub returns, hardcoded dummy values). If the implementation is incomplete, say so — do not ship a stub.
- LLM-generated code must follow the existing naming conventions, module structure, and architectural patterns in the codebase. Do not invent new patterns when existing ones apply.
- LLM-generated code must not disable or weaken any lint, clippy rule, or formatting check.
- LLM-generated test names must be descriptive of the behavior being tested, not generic (`test_it_works`).
