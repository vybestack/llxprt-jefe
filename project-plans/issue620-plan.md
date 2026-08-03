# Issue #620 — Deliver multi-line prompts to npm-shimmed agents on Windows

Follow-up to #536 / PR #619. Goal: **sending an issue to Claude Code and Codex
on Windows must actually work**, not fail with a diagnostic.

## Empirical findings (gathered before design)

1. **Both CLIs take the prompt as an initial positional argument**, proven by the
   captured help fixtures — no inference:
   - `tests/fixtures/agent-definitions/claude/2.1.212/help.stdout`:
     `Usage: claude [options] [command] [prompt]`, `Arguments: prompt  Your prompt`,
     "starts an interactive session by default".
   - `tests/fixtures/agent-definitions/codex/0.142.0/help.stdout`:
     `Usage: codex [OPTIONS] [PROMPT]`, `[PROMPT] Optional user prompt to start the session`.
   So `PromptShape::InitialPositional` is fixture-verified for both.

2. **Neither CLI accepts a prompt from a file or from stdin.** Grepping both help
   fixtures for `stdin|from a file|--prompt|prompt-file` yields only
   `--prompt-suggestions` and `--replay-user-messages` (Claude) and nothing
   (Codex). Temp-file and stdin delivery are therefore *not options* — argv is
   the only supported route, and it must be made to work.

3. **PowerShell does not truncate a multi-line argument.** The `0x0A` limit is
   specific to `cmd.exe`. Verified directly on this machine by invoking
   `powershell.exe -NoLogo -NoProfile -NonInteractive -File dump.ps1 "<4-line prompt>"`
   through `CreateProcess` (.NET `ProcessStartInfo.ArgumentList`, same quoting
   rules Rust's `std::process::Command` uses). Result: `COUNT=1` with all four
   lines intact.

4. **npm always installs a `.ps1` beside the `.cmd`.** `npm prefix -g` on this
   machine holds `llxprt.cmd`/`llxprt.ps1` and `ocr.cmd`/`ocr.ps1`. npm's
   `cmd-shim` emits the `.cmd`, `.ps1`, and extensionless trio together for the
   same command. Selecting the sibling `.ps1` is therefore **not** the "guess a
   neighbouring runtime" that #619 rejected — it is the same installer's shim
   for the same binary, not a different program.

5. **`.cmd` wins resolution today.** `windows_extensions` uses default PATHEXT
   `.COM;.EXE;.BAT;.CMD` and appends `.ps1` *after*; `resolve_windows` returns
   the first hit. So npm agents land on `CommandScript`.

## Acceptance matrix

| Row | Given | When | Then |
|---|---|---|---|
| A1 | Unmarked `.cmd` with a sibling `.ps1` | argv contains CR/LF | launch through `powershell -File <sibling>.ps1`, full argv byte-intact |
| A2 | Unmarked `.cmd` with **no** sibling `.ps1` | argv contains CR/LF | still `CommandScriptArgumentUnsupported`; never truncate |
| A3 | Unmarked `.cmd`, newline-free argv | launch | existing `cmd.exe` contract unchanged, byte for byte |
| A4 | Marked llxprt wrapper | multi-line prompt | unchanged from #619 — runtime + entrypoint, no `cmd.exe` |
| A5 | `core.claude-code` | fresh-issue send | supported, prompt delivered as initial positional |
| A6 | `core.codex` | fresh-issue send | supported, prompt delivered as initial positional |
| A7 | `Direct` / `PowerShellScript` wrappers | any argv | unchanged |

## Non-goals

- Changing PATHEXT resolution order or preferring `.ps1` generally.
- Parsing `.cmd` shim contents to extract a runtime + entrypoint.
- Remote/setup targets for Claude and Codex — still not fixture-verified.
- Revisiting the execution-guard fingerprint blind spot for shims (noted in
  #620, wider than this change).

## Slices

1. **Launcher**: sibling-`.ps1` fallback in `write_launch_plan` (A1–A4, A7).
2. **Definitions**: flip fresh-issue / fresh-PR to supported with
   `PromptShape::InitialPositional` for Claude and Codex (A5, A6).

## Expected paths

- `src/runtime/agent_launcher.rs` — `powershell_sibling_for`, fallback in
  `write_launch_plan`, behavioral tests.
- `src/domain/agent_definition/shipped/claude.rs`
- `src/domain/agent_definition/shipped/codex.rs`
- `src/domain/agent_definition/shipped/common.rs` — replace
  `unsupported_only_operations` with a positional-prompt operation matrix.
- `tests/generated_form_ui.rs` — expectation currently asserts the Claude
  fresh-issue "not fixture-verified" reason.

## Scope ledger

Target well under budget. Deferred: stdin/temp-file prompt delivery (unsupported
by both CLIs, so not implementable); guard fingerprinting of shim targets.

## Risks

- `-NonInteractive` risk: **CLOSED with evidence.** Piping input through
  `powershell.exe -NoLogo -NoProfile -NonInteractive -File child.ps1` showed the
  child process still received stdin (`CHILD_GOT="hello-from-user"`) and
  `[Environment]::UserInteractive` remained `True`. The flag suppresses only
  PowerShell's own prompts (`Read-Host`, confirmations), which is desirable in a
  pane: it prevents the host from hanging on a prompt the user cannot see. The
  npm shim execs the agent directly, so the agent stays fully interactive.
- Flipping the operation matrices changes each definition's SHA-256, which the
  execution guard compares. Expect fixture/golden updates.

## TUI scenario coverage

The tmux harness runs only under tmux: every shipped scenario declares
`platform` of `macos` (85) or `linux` (21), and even the `windows-*` scenarios
are `linux`. A Windows `.cmd`-shim scenario is therefore not implementable in
this harness — there are no `.cmd`/`.ps1` shims on the harness platforms — and
the `-NonInteractive` question it was meant to answer is now closed directly by
the evidence above.

The UI-visible half of this change (Claude/Codex no longer refuse fresh-issue)
is covered by updating `dev-docs/tmux-scenarios/issue382/agent-unsupported-ui.json`:
the stale `Fresh Issue: Unsupported: Claude fresh-issue prompt is not
fixture-verified` assertion becomes an `absent` assertion, so the scenario now
proves the refusal is gone. That scenario keeps its original purpose — Resume,
Remote, and Model remain unsupported there and still assert `[Create disabled]`.

## Review / verification ledger

- RED evidence: sibling-`.ps1` launcher test failed at `agent_launcher.rs:405`
  (`11 passed; 1 failed`) before the fix, `12 passed; 0 failed` after.
- Local gates: `cargo fmt --all` clean; `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean (EXIT=0, no warnings); full
  `cargo test --workspace --all-features --no-fail-fast` = 81 suites,
  0 failures (including the usually-flaky `psmux_attach`).
- Local OCR: 0/2. PR OCR: 0/2.
