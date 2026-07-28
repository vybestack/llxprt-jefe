# Issue 448 delivery plan — disambiguate `doctor_alone_parses_as_doctor_command` test name

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/448
- Source: deferred from PR #444 (OCR post-PR run 2/2, finding #13, Defer).
- Branch: `issue448`
- Base: `origin/main`
- Review counters: OCR pre-PR 0/2, OCR post-PR 0/2
- Delivery shape: single-line test rename in one file, zero behavioral
  change, well under the 25-file / 1,500-line budget.

## Summary

`tests/doctor/cli.rs` has a test named `doctor_alone_parses_as_doctor_command`
(around line 30). The name is ambiguous because at least one sibling test in
the same module, `doctor_does_not_set_version_or_help_flags` (around line
146), also re-parses bare `doctor` (i.e. `parse(&["doctor"])`). The existing
name does not convey what *this specific* test proves versus the other
doctor-alone assertions.

The test body is correct and proves both that bare `doctor` is recognised as
the doctor subcommand **and** that it carries no `--config` dir. Only the name
is imprecise. This is a pure naming/readability change deferred from PR #444;
no parser behavior or test body change is permitted.

## Acceptance matrix

| Row | Actor / launch path | Input and boundary | Target | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| AC-01 | doctor CLI test module | renamed test `fn` in `tests/doctor/cli.rs` | local; platform-independent Rust test | the renamed identifier precisely conveys that bare `doctor` is recognised as the doctor subcommand **and** carries no `--config` dir, distinguishing it from `doctor_does_not_set_version_or_help_flags` | name review / ambiguity | none | no parser, public-API, or persisted-data change | `cargo test --test doctor` stays green |
| AC-02 | doctor CLI test module | test body of the renamed `fn` | local; platform-independent Rust test | the assertions (`is_doctor()` + `config_dir.is_none()`) are byte-for-byte unchanged | body diff review | none | none | `git diff` shows only the `fn` name line changed |
| AC-03 | quality gates | whole workspace | local CI | `cargo test --test doctor` green; `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean | gate failure | none | none | gate command exits 0 |

## Non-goals

- No change to the doctor parser behavior or its public API.
- No restructuring, merging, or splitting of tests in `tests/doctor/cli.rs`.
- No new doctor parsing coverage (the parsing contract is already covered).
- No rename of any sibling test unless its name is equally ambiguous and the
  rename is a single-line, in-scope edit (criterion AC-01 of the issue).
- No change to `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifests,
  or quality-gate scripts.

## Architectural decision

Single-line identifier rename in `tests/doctor/cli.rs`. The body stays
unchanged, so this is not TDD-shaped (no RED); the verification is that the
existing test stays green after the cosmetic rename. The new name must:

1. name the doctor subcommand recognition (what the body proves), and
2. name the no-`--config`-dir assertion (the part that distinguishes this test
   from `doctor_does_not_set_version_or_help_flags`, which only checks
   version/help flags).

Chosen name: `doctor_alone_parses_as_doctor_subcommand_with_no_config_dir`.

This precisely conveys both assertions in the body and is distinguishable from
`doctor_does_not_set_version_or_help_flags` (version/help flags) and from the
`doctor_with_*_config_carries_config_dir` family (config-dir present).

## Vertical slices

### S1 — Rename + verify

- Rows: AC-01, AC-02, AC-03.
- Owner/boundary: single test identifier in `tests/doctor/cli.rs`.
- Allowed file: `tests/doctor/cli.rs`.
- GREEN: rename the `fn`; keep body unchanged.
- Non-goals: per the issue non-goals.
- Verification: `cargo test --test doctor`; `cargo fmt --all --check`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- Stop for approval if a sibling rename or any non-name edit is required.

## Scope ledger

| Item | Classification | Disposition |
|---|---|---|
| Rename of `doctor_does_not_set_version_or_help_flags` | Not ambiguous (its name already conveys the version/help-flag assertion) | Reject — out of scope, name is already precise. |
| Any other sibling rename | Out of scope | Reject unless equally ambiguous and single-line. |
| Test body / assertion changes | Out of scope | Reject — issue requires no behavioral change. |

## Verification

- Focused: `cargo test --test doctor`.
- Full gate before push/PR: `cargo fmt --all --check` +
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` +
  `cargo test --workspace --all-features --locked` (or `make ci-check`).

## Stopping rules

- Stop if the rename touches more than the single `fn` line.
- Stop if any sibling test rename, body change, or module restructure is
  required.
- Stop if the hard scope budget (40 files / 2,500 lines) would be crossed.
