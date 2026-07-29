# Issue #520 — Fix malformed LLxprt fresh-send prompt arguments

## Status

Accepted for implementation. The user requested that issue #520 be filed and driven through a complete fix and pull request without additional approval for bounded work.

Planning base: `88005aba` (current `origin/main` when branch `issue520` was created).

Upstream symptom: https://github.com/vybestack/llxprt-code/issues/2851
Regressing Jefe change: PR #501 / commit `7b12edcd`

## Validated root cause

The shipped LLxprt definition declares `prompt_interactive` as a Boolean field with a flag emitter. The legacy form's **Pass --continue** checkbox writes that field. A fresh Issue or Pull Request plan therefore emits a bare `--prompt-interactive` before the fresh-send boundary appends the real `-i <prompt>` pair.

The resulting structural argv is:

    --profile-load glm --yolo --prompt-interactive -i <prompt>

LLxprt defines `--prompt-interactive` and `-i` as aliases of one string-valued option. Its parser combines the malformed duplicate into an array, which later fails at `initialPrompt.trim()`. Before PR #501, Jefe emitted exactly one prompt-valued option. The Jefe correction belongs in the definition and deterministic operation projection, not in the process executor.

## Decision-complete acceptance matrix

| ID | Actor / launch path | Inputs and boundaries | Observable success | Failure / side effects | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|
| A1 | Local LLxprt Fresh Issue | profile `glm`; YOLO true; continuation true; exact multi-line issue prompt | Exact final argv is `--profile-load`, `glm`, `--yolo`, `-i`, prompt; empty plan env; exact cwd | No bare/duplicate prompt alias or continuation reaches runtime | Post-501 obsolete `prompt-interactive` value cannot affect the plan | Full shipped-definition `prepare_launch` regression and TUI form scenario |
| A2 | Local LLxprt Fresh Pull Request | Same matrix with exact PR prompt | Same fresh-session contract with PR bytes | Same zero-malformed-argv rule | Same | Full shipped-definition regression |
| A3 | Remote LLxprt Fresh Issue / Fresh Pull Request | Same typed values and remote target | Structural agent argv has the A1/A2 semantics and audited remote serialization | Unsupported/invalid plan performs no SSH/process effect | Same | Remote full-plan/transcript tests |
| A4 | LLxprt Normal and Resume | continuation true and false | True emits fixture-proven `--continue`; false emits no continuation | No normal/resume plan emits bare `--prompt-interactive` | New form saves typed `continue` | Local/remote plan goldens and form tests |
| A5 | Existing schema-2 values created by PR #501 | obsolete `prompt-interactive` present, `continue` absent | Launch projection copies only declared fields, so obsolete data cannot emit malformed argv; editing replaces it with current generated values | Runtime does not filter product arguments or coerce prompt values | No state schema bump; unknown durable bytes remain lossless until the form is explicitly saved | Launch/form tests |
| A6 | Issues screen Send confirmation through the real TUI | schema-1 LLxprt agent with profile `glm`, YOLO true, and `pass_continue=true`; issue #230 | One captured launch has `--profile-load glm --yolo -i <full issue prompt>` and the Issues UI remains usable | No duplicate prompt alias or continuation reaches the process | Schema-1 migration supplies canonical `continue`, then FreshIssue projection omits it | Real-PTY harness scenario with deterministic GitHub, Git, tmux, and process captures |

## Accepted implementation decisions

1. Replace the LLxprt Boolean `prompt_interactive` field/emitter with Boolean `continue`, whose flag token is resolved from the existing fixture-proven `continue -> --continue` capability.
2. Make launch-value projection accept the operation rather than a prompt-only Boolean. Fresh Issue and Fresh Pull Request omit both `prompt` and `continue`; fresh-send assembly remains the sole prompt owner.
3. Update legacy form visibility/build/edit projection to read/write `continue` while retaining the user-facing **Pass --continue** label and existing default.
4. Do not add a runtime argv filter, LLxprt-specific executor branch, prompt coercion, dependency, schema bump, or broad migration subsystem.
5. Existing post-501 `prompt-interactive` keys are not declared after the correction. `launch_values` already projects only declared fields before strict plan validation, so they are retained durably but cannot affect execution. Explicitly editing an agent rewrites the current generated values.

## Non-goals

- Fixing LLxprt's independent scalar-option validation bug in this repository.
- Changing prompt text, Issue/PR selection, preflight order, environment inheritance, tmux behavior, or remote escaping.
- Adding compatibility fallbacks, process-boundary argument scanning, or defensive prompt normalization.
- Redesigning generated forms or the agent-definition schema.
- Dependency, quality-gate, workflow, `.llxprt/`, `.code_puppy/`, or unrelated documentation changes.
- Moving unrelated tests or cleaning up adjacent issue #382 architecture.

## Bounded vertical slices

### Slice 1 — UI contract and semantic definition (A4, A6)

- **Owners:** shipped definition, deterministic form projection, thin existing UI.
- **Allowed paths:** `dev-docs/tmux-scenarios/v1/llxprt-continue-field.json`, `tests/harness_v1_fixtures.rs`, `src/domain/agent_definition/shipped/llxprt.rs`, `src/state/form_projection.rs`, `src/state/form_build.rs`, `src/state/modal_ops.rs`, and focused existing form tests.
- **RED:** real-PTY scenario and form tests require the declared `continue` field behind **Pass --continue**; current definition exposes `prompt_interactive` instead.
- **GREEN:** field visibility/default/save/edit behavior uses `continue`; normal/remote goldens emit `--continue`.
- **Stop:** a state schema migration, public abstraction, new fixture subsystem, or UI redesign becomes necessary.
- **Verification:** focused form/definition/plan tests and `cargo xtask quick`.

### Slice 2 — Fresh operation contract (A1-A3, A5)

- **Owners:** deterministic launch composition and existing fresh-send boundary.
- **Allowed paths:** `src/runtime/launch_compose.rs`, adjacent runtime tests, `tests/agent_local_plan.rs`, `tests/issue382_behavior.rs`, and `tests/issue382/fresh_send.rs`.
- **RED:** full shipped-definition tests require exact complete local/remote argv with continuation true and prove obsolete `prompt-interactive` cannot emit.
- **GREEN:** fresh projection omits prompt and continuation; fresh-send adds one exact prompt-valued option.
- **Stop:** a new runtime layer, process filter, remote serializer change, persistence schema bump, or unrelated route is required.
- **Verification:** focused runtime/integration tests, updated scenario, and `cargo xtask quick`.

### Slice 3 — Exact-head delivery (A1-A6)

- **Owner:** verification/review/PR workflow only.
- **Allowed paths:** no new production paths; only in-scope review fixes mapped in the ledger.
- **GREEN:** tmux scenario, `make ci-check`, Rust/DeepThinker review, Open Code Review, PR CI, CodeRabbit triage, ancestry and conflict checks pass on exact head.
- **Stop:** hard scope budget, unrelated mainline conflict, or required-gate blocker needs an unplanned subsystem.

## Expected files by architectural layer

| Layer | Expected path | Acceptance |
|---|---|---|
| Plan | `project-plans/issue520-plan.md` | workflow evidence |
| TUI evidence | `dev-docs/tmux-scenarios/v1/llxprt-continue-field.json`, `tests/harness_v1_fixtures.rs` | A6 |
| Shipped policy | `src/domain/agent_definition/shipped/llxprt.rs` | A1-A4 |
| Form projection/build/edit | `src/state/form_projection.rs`, `src/state/form_build.rs`, `src/state/modal_ops.rs` | A4-A6 |
| Form tests | existing focused generated/form test modules as required | A4-A6 |
| Launch composition | `src/runtime/launch_compose.rs` and adjacent tests | A1-A5 |
| Local/remote goldens | `src/runtime/agent_plan_tests.rs`, `src/runtime/agent_remote_plan_tests.rs`, `tests/agent_local_plan.rs`, `tests/issue382_behavior.rs` | A1-A4 |
| Fresh-send acceptance | `src/runtime/agent_fresh_send_tests.rs`, `tests/issue382/fresh_send.rs` if required | A1-A3 |

Target: no more than 15 changed files and 800 net lines for the green implementation; repository hard workflow thresholds remain 25 files / 1,500 net lines before mandatory review and 40 files / 2,500 net lines before an approval stop.

## Scope ledger

| Status | File / discovery | Mapping / disposition |
|---|---|---|
| Complete | GitHub issue #520, branch, and this plan | requested workflow |
| Complete | shipped definition and form projection/build/edit paths | A4-A6 |
| Complete | operation-aware composition and extracted focused tests | A1-A5; extraction keeps generic production source within architecture policy |
| Complete | schema-1 migration and fixed-vector updates | A5-A6; `pass_continue` maps one way to canonical `continue` |
| Complete | local/remote plan goldens and normal/resume matrix | A1-A4 |
| Complete | real-PTY Issues Send scenario, capture assertion, and deterministic GH/Git/tmux shims | A6; captures the actual structural LLxprt invocation after repository preparation |
| Complete | stale Issue Send comments/test name correction | review maintainability finding; documents operation-owned continuation omission |
| Reject | OCR claim that stale `prompt-interactive` test data is dead | the seeded obsolete value is required evidence that undeclared schema-2 data cannot execute |
| Reject | runtime argument filtering | hides invalid definition and violates ownership |
| Defer | LLxprt CLI scalar validation | upstream llxprt-code issue #2851 |

Final bounded scope is 22 paths, below the repository target of 25 files and 1,500 net lines. It exceeds the issue's initial 15-file implementation target because review-required migration, explicit-edit, architecture extraction, and real process-capture evidence were added. A mandatory scope review found every path maps directly to A1-A6; no new public abstraction, production subsystem, dependency, quality rule, or unrelated behavior was added.

## Review counters

- DeepThinker root-cause investigation: complete (read-only).
- Local Rust/DeepThinker review cycles: 1 / 2, findings triaged and remediated.
- OCR before PR: 1 / 2; two comments evaluated, with stale-data premise rejected and architecture extraction/remediation completed.
- OCR after PR: 0 / 2.
- CodeRabbit PR findings: pending.

Finding dispositions: **Blocker-Fix** architecture token violation fixed by extracting tests; **In-scope-Fix** schema-1 canonical continuation, explicit-edit stale-value removal, full operation matrix, captured Issues Send flow, and stale comments/test name; **Reject** removal of the stale schema-2 test value because it is A5 evidence.

## Verification evidence

| Candidate | Command / evidence | Result |
|---|---|---|
| `88005aba` | root-cause source/history investigation | malformed duplicate prompt option confirmed |
| RED working tree | operation-aware compile, migration, and explicit-edit tests | failed for the intended missing contracts |
| GREEN working tree | `runtime::launch_compose::tests` | 3 passed: fresh Issue/PR exact argv plus local/remote normal/resume true/false |
| GREEN working tree | schema-1 fixed vectors and explicit-edit regression | passed after canonical mapping and declared-value projection |
| GREEN working tree | `llxprt_continue_field_fixture_sends_one_exact_issue_prompt` | passed; one captured LLxprt invocation, one `-i`, full prompt, no continuation |
| GREEN working tree | architecture and source-size gates | passed; `launch_compose.rs` is 625 lines |
| exact working head | format, strict Clippy, locked all-feature build | passed |
| exact working head | locked all-feature library tests | 2,794 passed, 1 ignored |
| exact working head | locked all-feature integration tests | all test binaries passed, including all 22 harness scenarios |
| exact working head | architecture, source-size, shell syntax, JSON parse, diff check | passed; only existing source-size warnings |
| exact working head | `make ci-check` | unavailable because this checkout has no Make target; exact component gates above were run instead |
| pending PR head | CI, CodeRabbit, ancestry, conflict check | pending |

## Deferred findings / follow-ups

LLxprt should independently reject a yargs array for repeated scalar prompt aliases before React startup. That remains tracked by https://github.com/vybestack/llxprt-code/issues/2851 and is not implemented in this Jefe issue.
