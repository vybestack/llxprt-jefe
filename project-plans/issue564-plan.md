# Issue #564 — Rustc warnings only enforced by the single clippy CI job

> `unused_imports`, `dead_code`, `unused_variables`, `unused_mut` (rustc lints)
> are only denied by the clippy jobs that pass `-D warnings`. Every other path
> (`cargo build`, `cargo test`, `cargo check`) tolerates them, so a real unused
> import is green everywhere except one gate. Fix: deny rustc warnings at the
> lowest level via `[lints.rust] warnings = "deny"`.

## Acceptance matrix

| # | Launch path | Input | Success behavior | Failure behavior | Evidence |
|---|-------------|-------|------------------|------------------|----------|
| A1 | `cargo build` (local + CI `build`) | clean tree | green | n/a | full suite |
| A2 | `cargo build` (local + CI `build`) | source with unused import | **fails** with rustc `unused_imports` denied | exit != 0 | procedural verification |
| A3 | `cargo check` (local) | source with unused import | **fails** | exit != 0 | procedural verification |
| A4 | `cargo test` (local + CI `test`) | source with unused import | **fails** to compile | exit != 0 | procedural verification |
| A5 | `cargo clippy -- -D warnings` (CI `lint`/`windows_clippy`) | clean tree | green (no regression) | n/a | full suite |
| A6 | `[lints.clippy]` allow-list | clean tree | still suppresses intended clippy lints | n/a | clippy green + `lint_config_policy` guard |
| A7 | `[lints.rust]` config | Cargo.toml | `warnings = "deny"` + `unsafe_code = "forbid"` present | regression test fails if removed | `tests/lint_config_policy.rs` |

## Non-goals

- Changing clippy lint levels or complexity / source-size / architecture policies.
- Adding new clippy lints or changing `pedantic` / `nursery` levels.
- Adding a Makefile / justfile local runner (optional item 3 — out of scope).
- Enumerating individual rustc lints instead of blanket `warnings = "deny"`
  (optional item 2 — the primary recommendation is the blanket deny).
- Adding `-D warnings` to CI `build` / `test` jobs explicitly (optional item 4 —
  redundant once `[lints]` denies warnings).

## Vertical slice

1. **RED** — `tests/lint_config_policy.rs`: repository-text contract test
   (mirrors `tests/coderabbit_policy.rs` / `tests/ocr_review_policy.rs`) that
   parses `Cargo.toml`, asserts `[lints.rust]` contains `warnings = "deny"` and
   `unsafe_code = "forbid"`, and that the `[lints.clippy]` allow entries
   (`needless_pass_by_value`, `redundant_clone`) remain `allow`. Fails on the
   current tree (no `warnings` key).
2. **GREEN** — add `warnings = "deny"` to `[lints.rust]` in `Cargo.toml`. The
   tree is already clippy `-D warnings` clean, so this is a no-op on current
   code and only tightens the gate going forward. Test passes.

## Expected files

- `Cargo.toml` — `[lints.rust]` gains `warnings = "deny"` (1 line).
- `tests/lint_config_policy.rs` — new regression guard.

## Behavioral verification (procedural, not committed)

- Temporarily introduce an unused import in a source file; confirm
  `cargo build`, `cargo check`, and `cargo test` all fail; restore clean tree.
- Run full suite green on the clean tree.

## Scope ledger

- (none — within budget: 2 files, ~1 net source line + test)

## Review counters

- Local OCR: 0 / 2
- PR OCR: 0 / 2
