# Issue #534 — Trusted capability probe for LLxprt

## Problem

Jefe cannot reliably launch agents on Windows. The definition-driven agent probe
(#382) runs two subprocesses per probe: `--version` (identity) and `--help`
(capability verification). The `--help` gate is the dominant failure point on
Windows — it exits nonzero, times out, or returns unexpected framing where
`--version` succeeds.

The capability tokens used for argv composition are read from the **shipped
definition**, not from the `--help` result. The only runtime effect of the
`--help` gate is to reject installations missing a *required* capability token.
For LLxprt the sole required capability is `prompt-interactive`, and every
shipped LLxprt release supports every argument Jefe emits. The `--help` gate
therefore adds launch fragility without rejecting any genuinely incompatible
installation.

## Solution

Add an opt-in `trusted` flag to `CapabilityProbe`. When `trusted: true`, the
runtime probe skips the `--help` subprocess and reports every authored token as
present. The definition's capability tokens remain the authority for argv
composition (`resolve_flag_token`).

LLxprt's definition marks its capability probe `trusted: true`. Codex, Claude,
and Code Puppy retain their existing `--help` verification (unchanged).

## Acceptance matrix

| ID | Requirement | Proof |
|---|---|---|
| A1 | Trusted probe runs only `--version`, never `--help` | `trusted_probe_runs_only_version_and_reports_authored_capabilities` |
| A2 | Trusted probe reports authored token ids as present | Same test asserts `capabilities == authored_capability_ids()` |
| A3 | LLxprt definition marks probe trusted | `shipped_llxprt_definition_marks_capability_probe_trusted` |
| A4 | Codex/Claude/Code Puppy remain untrusted | `shipped_non_llxprt_definitions_remain_untrusted` |
| A5 | Authored tokens still resolve for argv composition | `resolve_flag_token` unchanged; existing launch tests green |
| A6 | Remote probe honors trusted mode | `agent_remote_probe.rs` skips `--help` when trusted |
| A7 | Identity failures remain fail-closed | `--version` nonzero/timeout/mismatch still AGT-E202 |
| A8 | Canonical form includes `trusted` | `capability_probe_to_json` serializes `trusted` |
| A9 | Reader accepts `trusted` | `read_trusted` parses bool, rejects unknown types |

## Non-goals

- Do not change blank-Version semantics.
- Do not change LLxprt profile/yolo/continue/prompt-interactive/sandbox arguments.
- Do not change Codex/Claude/Code Puppy probe behavior.
- Do not address #515 session-host/watchdog/lifecycle work.
- Do not add a dependency or public abstraction.

## Scope ledger

| Layer | File | Change |
|---|---|---|
| domain | `src/domain/agent_definition/probe.rs` | Add `trusted: bool` to `CapabilityProbe`; add `authored_capability_ids()` helper |
| domain | `src/domain/agent_definition/canonical.rs` | Serialize `trusted` in `capability_probe_to_json` |
| domain | `src/domain/agent_definition/reader.rs` | Deserialize `trusted` via `read_trusted` |
| domain | `src/domain/agent_definition/shipped/common.rs` | Add `trusted_capability_probe` helper |
| domain | `src/domain/agent_definition/shipped/llxprt.rs` | Mark LLxprt capability probe `trusted: true` |
| runtime | `src/runtime/agent_probe.rs` | Skip `--help` when trusted; report authored tokens |
| runtime | `src/runtime/agent_remote_probe.rs` | Skip remote `--help` when trusted |
| test | `src/domain/agent_definition/probe_tests.rs` | Domain tests for `trusted` default and `authored_capability_ids` |
| test | `src/domain/agent_definition/definition_tests.rs` | Update struct literal |
| test | `src/domain/agent_definition/validation_tests.rs` | Update struct literal |
| test | `src/persistence/migration_tests.rs` | Update LLxprt definition hash fixed vector |
| test | `tests/issue382/agent_probe_runtime.rs` | Runtime test: trusted probe runs only `--version` |
| test | `tests/issue382_behavior.rs` | Definition trust tests (cross-platform) |

13 files, +204 / -5 lines.

## Verification

- `cargo fmt --all --check` — pass
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` — pass
- `cargo build --workspace --all-features --locked` — pass
- `cargo test --lib` — 2709 passed, 0 failed
- `cargo test --test issue382_behavior` — 28 passed, 0 failed
- Pre-existing unrelated failures (`prs_diff_dispatch`, git not on PATH) unchanged

## Review counters

- OCR before PR: 0
- OCR after PR: 0
