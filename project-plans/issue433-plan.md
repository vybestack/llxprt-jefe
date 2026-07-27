# Issue 433 delivery plan

## Issue and baseline

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/433
- Branch: `issue433`
- Base: `origin/main` at `20f6e764433946c0916bb4bf5878ff47212705cc`
- Concrete reproduction: send issue #425 (8,092 UTF-8 body bytes) from the Issues screen to direct LLxprt on native Windows with psmux 3.3.6; the launched pane reports `line too long`.
- Related work: #409/#410 compact large Unix tmux prompts; #253 owns native Windows parity; #277 already transports Windows agent argv/environment out of band through a consumed launch-plan file.
- Discussion: the user approved generalizing the existing canonical script launch abstraction to launch the official LLxprt bundled Bun runtime and `index.ts` entrypoint directly. Generic command-script compaction/error handling, PowerShell wrappers, and #425 package-version behavior remain deferred.

## Corrected root cause and approved architecture

Current main does not pass the full prompt through psmux. `MultiplexerPlan::agent_pane_command_args_with_launcher` writes the full argv/environment to a private temporary plan and gives psmux only `jefe.exe --jefe-internal-agent-launch <plan-path>`.

The private launcher later reconstructs a `CommandScript` as `cmd.exe /D /S /C llxprt.cmd <args>`. The official installed LLxprt wrapper then invokes its bundled Bun runtime and `index.ts`, but the complete #425-sized instruction has already crossed `cmd.exe`'s approximately 8,191-character command-line boundary.

Approved fix:

- recognize only the official wrapper marker `LLXPRT_NATIVE_LAUNCHER owned by @vybestack/llxprt-code`;
- validate and canonicalize the documented bundled Bun and `index.ts` paths relative to the wrapper;
- generalize the existing typed canonical script launch plan from npm-specific Node/CLI names to runtime/entrypoint names;
- have the consumed private launch plan execute the runtime plus entrypoint and preserve every original argument as a structural process argument;
- preserve unrecognized `.cmd`/`.bat` wrappers and all Unix paths unchanged.

This removes the binding `cmd.exe` shell from the supported LLxprt path rather than lowering prompt thresholds or changing psmux.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure / diagnostics | Permitted side effects | Compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | User presses uppercase `S`, chooses direct LLxprt, and Jefe launches the resolved agent | #425-sized instruction (8,092-byte body plus metadata/workflow), quotes and shell metacharacters | Native Windows, psmux 3.3.6, official `llxprt.cmd` | Session starts and the full instruction reaches LLxprt as one unchanged argv value; no `line too long` | Resolver/launcher returns a prompt-free typed diagnostic if the marked official layout is incomplete | Existing private launch-plan file is created and consumed; owned psmux namespace/session only | Existing issue prompt and LLxprt flags remain unchanged | resolver/launcher regression plus real psmux smoke with official-layout fixture |
| A2 | Windows resolver examines a marked official LLxprt command wrapper | Bundled Bun and `index.ts` both exist; one is missing; wrapper marker absent | Native Windows | Complete marked layout yields a canonical runtime/entrypoint plan | Marked but incomplete layout fails safely with reinstall guidance; unmarked wrapper retains existing `CommandScript` behavior | Read-only wrapper/layout inspection | Other agents and third-party wrappers are unchanged | deterministic resolver unit tests |
| A3 | Private launcher consumes a canonical script launch plan | Long prompt, empty args, Unicode, quotes, shell metacharacters | Windows process boundary | Runtime executable is the process program; entrypoint is the first child argument; original args remain separate and ordered | Existing safe `AgentLauncherError` behavior remains prompt-free | Plan file is removed before child execution as today | Existing canonical npm launch remains behaviorally identical after type generalization | command structure tests and existing npm tests |
| A4 | Unix/macOS or Windows pinned npm launch executes | Direct Unix LLxprt; canonical Windows npm; selector-backed LLxprt | Unix/macOS and Windows | Existing executable selection, prompt compaction, and npm direct launch remain unchanged | Existing diagnostics unchanged | No new side effects | Serialized persistence schema and public CLI are unchanged | existing suites plus focused regression guards |

## Explicit non-goals

- No psmux pane-command or namespace changes.
- No lower platform-specific prompt-compaction threshold and no change to #409/#410 behavior.
- No generic `.cmd`/`.bat` command-length budgeting, compaction, or new fallback error subsystem.
- No PowerShell-wrapper generalization without evidence of an LLxprt `.ps1` failure.
- No change to issue #425's npm version selection, cache, locking, or install strategy.
- No change to issue/PR content, keybindings, reducer behavior, persistence, dependencies, workflow/agent-memory files, `.llxprt/`, `.code_puppy/`, `.github/`, or quality-gate configuration.
- No broad parser for arbitrary command scripts; only the explicit official marker and canonical package-relative layout are accepted.

## Bounded vertical slice

### S1: canonical official LLxprt script launch

- Acceptance rows: A1-A4.
- Architecture owner: runtime executable resolution and private process launcher; integration boundary is `ResolvedAgentExecutable -> AgentLaunchPayload -> Command`.
- Allowed production paths: `src/runtime/agent_executable.rs`, `src/runtime/agent_launcher.rs`, plus directly required public re-export/exhaustive error consumers in `src/runtime/mod.rs` and `src/runtime/package_probe.rs`.
- Allowed evidence paths: `src/runtime/agent_executable_tests.rs`, `src/runtime/npm_launch_tests.rs`, `tests/psmux_smoke.rs`, `tests/fixtures/psmux_smoke_fixture.rs`, `dev-docs/testing/psmux-smoke.md`, and this plan.
- Planned approved public-contract change: rename/generalize `CanonicalNpmLaunchPlan` to a canonical script-runtime plan with `runtime` and `entrypoint`, and generalize the corresponding private serialized payload. No new module or subsystem.
- RED: first add deterministic resolver/launcher assertions and a real psmux smoke using an official-layout fixture with an 8,092-byte prompt. Current main must resolve the wrapper without a canonical plan and fail the real launch at the command-shell boundary.
- GREEN: the official marker plus complete canonical layout resolves to bundled Bun + `index.ts`; the private launcher bypasses `cmd.exe`; the native psmux smoke records the full prompt.
- REFACTOR: keep wrapper recognition/layout validation in the resolver boundary and process construction in the launcher boundary; retain safe prompt-free errors.
- Verification: focused runtime unit tests; `cargo test --features psmux-smoke --test psmux_smoke -- --nocapture`; `make quick-check`; exact-head `make ci-check`.
- Stop conditions: parsing arbitrary wrapper syntax, a new process/cleanup subsystem, prompt-layer changes, dependencies, quality-tool changes, paths outside the allowed set, or scope above 15 files/800 lines for the slice.

## TUI evidence decision

The observable UI route from uppercase `S` through issue prompt construction and `spawn_session_fresh` is existing covered behavior; this issue changes only the post-resolution Windows process boundary. New schema-1 scenarios cannot target Windows because the closed grammar permits only `macos|linux`, and changing that harness is an unapproved quality-tool expansion. Adding a new legacy scenario is prohibited by the schema-1 migration contract. Therefore the bounded native proof is the established real-psmux smoke suite using production resolver, private launcher, and session creation, while existing app-input tests retain the uppercase-`S` route. A full Windows schema-1 TUI scenario is deferred to the harness Windows-platform effort rather than weakening either contract.

## Expected paths by layer

| Layer | Expected paths | Acceptance mapping |
| --- | --- | --- |
| Windows executable resolution | `src/runtime/agent_executable.rs` | A1, A2, A4 |
| Private launch-plan execution | `src/runtime/agent_launcher.rs` | A1, A3, A4 |
| Runtime public re-export | `src/runtime/mod.rs` | A3, A4 |
| Exhaustive probe error classification | `src/runtime/package_probe.rs` | A2, A4 |
| Resolver behavior evidence | `src/runtime/agent_executable_tests.rs` | A2, A4 |
| Launcher/npm regression evidence | `src/runtime/npm_launch_tests.rs` | A3, A4 |
| Native Windows process scenario | `tests/psmux_smoke.rs` | A1, A3 |
| Native fixture support, only if needed | `tests/fixtures/psmux_smoke_fixture.rs` | A1 |
| Compatibility documentation | `dev-docs/testing/psmux-smoke.md` | A1 |
| Delivery record | `project-plans/issue433-plan.md` | all rows |

Expected scope: at most 14 changed files and under 700 net changed lines. Two files are user-approved one-line prerequisite Clippy corrections; the native regression was split into a child module and a test-module declaration was reordered to satisfy the repository's 1,000-line hard gate.

## Scope ledger

| Discovery | Disposition | Rationale / follow-up |
| --- | --- | --- |
| PR #277 already removes full argv from the psmux pane command | In-scope correction | Do not alter psmux or add final-pane-command budgeting; diagnose the subsequent wrapper boundary. |
| Official installed `llxprt.cmd` carries an explicit native-launcher marker and invokes bundled Bun + `index.ts` | In-scope fix | Use the marker and canonical package-relative paths; do not parse arbitrary shell syntax. |
| Existing canonical npm plan already bypasses `cmd.exe` | In-scope refactor | Generalize the type/payload while preserving npm behavior. User explicitly approved this public abstraction change. |
| Generalized public type and new typed error require runtime re-export and exhaustive probe classification updates | In-scope fix | `src/runtime/mod.rs` and `src/runtime/package_probe.rs` are direct compile-time consumers of the approved contract, not new behavior or scope expansion. |
| Current Rust/Clippy flags `input.len() % 4 != 0` in `src/harness/v1/validate.rs` | Blocker-Fix | User approved the one-line prerequisite correction so the PR has no Clippy failure; behavior is unchanged. |
| Current Rust/Clippy flags a one-hour capture-shim sleep expressed as 3,600 seconds | Blocker-Fix | Use `Duration::from_hours(1)` in `src/bin/jefe-capture-shim.rs`; user required clearing all PR Clippy failures and behavior is unchanged. |
| Issue #425 concerns npm package-version reliability | Reject | #425 supplies the size fixture only; its install/cache/version requirements remain separate. |
| Generic unrecognized command wrappers may retain the same command-shell limit | Defer | No evidence for other agents in #433; generic compaction/error handling would add a broader subsystem. |
| PowerShell wrappers use a different process boundary | Defer | No reproduced failure; preserve existing behavior. |
| Schema-1 harness excludes Windows and new legacy scenarios are prohibited | Defer | Changing the quality tool requires separate approval/issue; use the established real psmux smoke at the changed boundary. |
| Adding the native regression made `tests/psmux_smoke.rs` exceed the 1,000-line hard gate | In-scope fix | Keep behavior unchanged and place the new regression in `tests/psmux_smoke/official_llxprt.rs` as a child module. |
| Current main has `src/runtime/manager.rs` at 1,002 lines | Blocker-Fix | Reorder the two private test-module declarations without behavior change, removing separator lines so the exact-head hard gate passes at 1,000 lines. |
| Pre-PR audit: resolver contracts were Windows-gated, wrapper marker read was unbounded, and marked-corrupt fallback intent was implicit | In-scope fix | Run pure injected-platform resolver tests on all hosts, cap marker reads at 8 KiB with regression coverage, and document authoritative marked-wrapper failure behavior. |
| Pre-PR audit: fixture filename sniffing could become fragile if reused | Defer | Test-only behavior is bounded to the new native regression; revisit if the fixture gains callers. |
| Pre-PR audit: marker trust model | Reject | PATH is already trusted and package-relative runtime/entrypoint paths are canonicalized; no privilege boundary is added. |

## Review counters

- Local OCR runs before PR: 1 / 2 (GLM Rust audit; pass, three low in-scope findings addressed, two informational findings deferred).
- OCR runs after PR: 0 / 2.

## Verification evidence

| Check | Result | Head |
| --- | --- | --- |
| Baseline branch/status/main drift | `issue433`, clean, `main...origin/main` 0/0 | `20f6e76` |
| RED focused tests | Reconstructed against isolated base source: `cargo test --lib issue433_red_official_llxprt_wrapper_requires_canonical_launch_plan -- --nocapture` failed at the behavioral assertion because the official LLxprt wrapper had no canonical direct plan and would use `cmd.exe` | `20f6e76` |
| GREEN focused tests | Resolver contracts: 11 passed on injected Windows policy, including oversized-marker rejection. `cargo test --lib windows_official_llxprt_script_plan_launches_bun_with_entrypoint_first_argument -- --nocapture`: 1 passed, runtime/entrypoint structural argv preserved | working tree |
| Native psmux smoke | New #425-sized regression passed through a WMI-created process outside the current nested psmux pseudo-console: 1 passed, full 8,092-byte prompt recorded; exact all-feature test gate also passed all 13 psmux smoke tests | working tree |
| `make quick-check` | `make` is unavailable on this native Windows host; direct Makefile equivalent `cargo fmt --all; cargo check -q; cargo test -q` passed from a WMI-created process outside nested psmux (all listed suites passed) | working tree |
| `make ci-check` | Exact constituent gates passed: fmt; all-target/all-feature Clippy `-D warnings`; complexity Clippy; source-size hard gate; locked all-feature build; locked all-feature tests outside nested psmux; coverage 69.63% lines (minimum 30%). Clippy-allow script could not invoke Python on this host; equivalent first-party scan found no attributes and config-threshold comparison passed. | working tree |
| Exact-head scope/ancestry/conflict review | 14 files, 618 additions / 68 deletions / +550 net lines including untracked plan and child test module; `git diff --check` clean, `main...HEAD` 0/0; within approved 14-file/700-line budget. Remote conflict check is deferred until PR creation. | working tree |

## Deferred findings and follow-ups

- Generic long-argument handling for unrecognized Windows command/PowerShell wrappers, if reproduced.
- Windows support in the schema-1 real-process TUI harness.
- Issue #425's jefe-managed exact LLxprt package installation/version strategy.
- Replace the native fixture's `index.ts` filename sniffing with an explicit protocol marker if the fixture gains more callers.
