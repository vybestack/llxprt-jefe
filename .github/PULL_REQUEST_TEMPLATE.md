<!-- Fill out the sections below when opening a PR. The checklist encodes the
     project's "must pass before pushing" gates and mirrors
     .github/workflows/ci.yml, which is the source of truth for CI. -->

## Summary

Describe the change at a high level. Link to the relevant issue or design notes
(e.g. `fixes #123`).

## Pre-push checklist

The fastest path to a green CI run is the single command that reproduces CI
locally:

```sh
make ci-check
```

`make ci-check` runs the same steps as CI (format, structural gates, lint,
complexity, coverage, build, tests). Confirm it passes before pushing.

For fast iteration during development:

```sh
make quick-check     # cargo fmt && cargo check -q && cargo test -q
```

The items below map 1:1 to the CI jobs in
[.github/workflows/ci.yml](workflows/ci.yml); `make ci-check` runs them all, but
they are listed so a failure can be traced to the matching gate:

- [ ] **Format** — `cargo fmt --all --check`
- [ ] **Clippy allow policy** — `scripts/check-clippy-allows.sh` (no tracked `#[allow(clippy::...)]` outside the allowlist)
- [ ] **Source file length** — `scripts/check-source-file-size.sh` (hard limit 1000 lines, warn at 750)
- [ ] **Lint** — `CLIPPY_CONF_DIR=.github/clippy rustup run stable cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] **Complexity** — `CLIPPY_CONF_DIR=.github/clippy rustup run stable cargo clippy --workspace --all-targets --all-features -- -D clippy::cognitive_complexity -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::type_complexity -D clippy::struct_excessive_bools`
- [ ] **Coverage** — `cargo llvm-cov --workspace --all-features --summary-only --ignore-filename-regex '(/vendor/|/tmp/|/rustc-)' --fail-under-lines 30` (requires `cargo-llvm-cov`; the [Makefile](../Makefile) wires up `LLVM_COV`/`LLVM_PROFDATA` for you)
- [ ] **Build** — `cargo build --workspace --all-features --locked`
- [ ] **Tests** — `cargo test --workspace --all-features --locked`

Optional (only if the change touches runtime/UI):

- [ ] **TUI smoke** — `cargo build --locked --all-features --bin jefe --bin jefe-tmux-harness`, then drive `target/debug/jefe-tmux-harness` with the scenario in `dev-docs/tmux-scenarios/startup-quit.json`

## Testing notes

Describe how this change was tested locally: commands run, important logs, smoke
scenarios, etc.

## Reviewers / Assignees

Tag reviewers or teams to review.

---

Keep this checklist in sync with the [Makefile](../Makefile) and
[.github/workflows/ci.yml](workflows/ci.yml).
