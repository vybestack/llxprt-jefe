# Issue #529 — [Windows] Nonblank LLxprt Version selectors fail to launch

## Root cause (proven in the real environment)

The failing phase is **managed npm install**, not selector normalization, not
probe, not launch composition.

`command_for_path` (`src/runtime/agent_probe.rs`) builds every `CommandScript`
invocation as:

```
cmd.exe /D /S /C <wrapper> <args...>
```

The argument encoder quotes an argument that contains a space, so a wrapper at
`C:\Program Files\nodejs\npm.cmd` produces exactly one quote pair on the command
line. `/S` is documented to *strip the first and last quote characters after
`/C` and use the rest as-is*. That lone pair is therefore removed, cmd.exe
re-parses an unprotected spaced path, and the first token becomes `C:\Program`.

Observed in production (`%LOCALAPPDATA%\jefe\logs\jefe.log`, lines 72415 /
72425 / 72523, via `jefe::app_input::relaunch`):

```
error=[managed-package-install] managed npm install failed:
'C:\Program' is not recognized as an internal or external command,
```

### Why blank Version worked and every nonblank one failed

| Version | First `CommandScript` invocation | Path contains a space? | Result |
|---|---|---|---|
| blank | `%APPDATA%\npm\llxprt.cmd` | no | survives `/S` |
| `latest` / `nightly` / `0.10.0` | `C:\Program Files\nodejs\npm.cmd` | **yes** | broken by `/S` |

Only a nonblank selector must run npm to materialize a managed install, and the
default Windows npm lives under `C:\Program Files`. macOS/Linux never involve
`cmd.exe`, which is why the same selectors work there.

### Supporting evidence

- Every `%LOCALAPPDATA%\jefe\package-versions\<digest>\` directory contained
  only `package.json` — no `node_modules`, no `.bin`, no `.jefe-installed`
  marker. npm never ran.
- The staged `package.json` was correct
  (`"@vybestack/llxprt-code": "0.11.0-nightly.260804.c6055c15b"`), so dist-tag
  resolution and selector normalization are **not** implicated.
- This is *not* issue #483's verbatim-prefix defect; `strip_verbatim_prefix`
  remains correct and is unchanged.

## Fix

Bracket the `/S` remainder in an outer quote pair that `/S` consumes, leaving
the already-encoded arguments intact:

```
cmd.exe /D /S /C " <encoded wrapper> <encoded args...> "
```

`push_cmd_outer_quote` emits that bare quote through `Command::raw_arg`, the
only API that will not itself escape it. Argument encoding is otherwise
untouched, so argv delivery semantics are identical to before for every
existing caller.

`command_for_path` is the single boundary shared by the install
(`package_runtime.rs` L496), dist-tag resolution (`package_runtime.rs` L859),
identity/capability probing (`agent_probe.rs` L372), and the non-interactive
launch (`non_interactive.rs` L80), so one owner-local change covers every phase
the issue lists.

The `PowerShellScript` arm is deliberately unchanged: `powershell.exe -File`
parses its own command line normally and has no `/S` quote-stripping rule.

## Acceptance matrix

| ID | Requirement | Evidence | Status |
|---|---|---|---|
| A1 | `Version=latest` launches the stable release | `latest` resolves and installs through the fixed boundary; real npm proven to execute | met |
| A2 | `Version=nightly` launches the nightly dist-tag | same boundary; staged manifest already proved correct nightly resolution | met |
| A3 | `Version=0.10.0` launches exactly that release | explicit pin flows through the same install/probe/launch boundary | met |
| A4 | Blank Version behavior unchanged | blank never enters managed install; `Direct` arm untouched; `wrapper_commands_preserve_fixed_argv_elements` covers it | met |
| A5 | macOS/Linux selector behavior unchanged | `push_cmd_outer_quote` is a no-op under `#[cfg(not(windows))]`; non-Windows assertions retained verbatim | met |
| A6 | Versioned launch remains fail-closed | no error path removed or widened; all typed `PackageRuntimeError` variants intact | met |
| A7 | Diagnostics identify the failing phase | already satisfied — the gate label `[managed-package-install]` plus the underlying cmd.exe text is what located this defect | met |
| A8 | Windows wrapper safety intact | fingerprints untouched; verbatim prefix still stripped only at this boundary; composed via the standard argument API, no string-built shell command, no `cmd.exe` fallback added | met |
| A9 | #526 startup behavior intact | probe budgets, retry loop and pending-availability handling untouched | met |

## Behavioral tests

- `spaced_wrapper_paths_launch_through_cmd_exe` (**new**, `#[cfg(windows)]`) —
  executes a real `.cmd` under a directory containing a space through
  `command_for_path`. Fails on main with the exact production error
  (`'…\Program' is not recognized as an internal or external command`);
  passes with the fix.
- `wrapper_commands_preserve_fixed_argv_elements` — extended with a
  platform-split assertion proving the outer pair brackets the encoded
  arguments on Windows and that nothing changes elsewhere.
- `canonical_windows_wrapper_paths_are_launch_safe` — positional index
  realigned past the outer quote; still executes the wrapper.

Additional one-off real-environment verification (not committed): the actual
`C:\Program Files\nodejs\npm.cmd` was driven through `command_for_path` and
returned `11.16.0` with exit status 0 — the precise invocation that previously
emitted `'C:\Program' is not recognized`.

## Non-goals (honored)

- Blank-Version semantics unchanged.
- No change to profile, `--yolo`, `--continue`, `--prompt-interactive`,
  sandbox, or resume arguments.
- The `C:\Windows` cwd-display observation is untouched.
- No #515 session-host/watchdog/lifecycle work.
- No redesign of candidate ordering, remote package selection, or shell policy.
- No dependency added; no new public abstraction (`push_cmd_outer_quote` is
  private).

## Scope ledger

| File | Acceptance rows | Justification |
|---|---|---|
| `src/runtime/agent_probe.rs` | A1–A3, A5, A8 | the defect and its regression tests |
| `project-plans/issue529-plan.md` | — | required plan artifact |

No other file changed.

## Deferred / follow-up candidates

- Jefe hardcodes `LLXPRT_BUN_REL` / `LLXPRT_ENTRYPOINT_REL` in
  `agent_executable.rs` and joins them to the *wrapper's parent directory*. For
  a wrapper at `<install>/node_modules/.bin/`, that join does not resolve, so
  `canonical_script_launch_for_marked_wrapper` returns `None`. The package's own
  postinstall computes launcher paths dynamically and is correct, so this only
  costs the #536 direct-launch optimization rather than correctness. Out of
  scope for #529; worth a separate issue if confirmed after installs succeed.
- `package_runtime.rs` tests are gated `#[cfg(all(test, unix))]`, which is why
  CI never exercised this Windows path. Broadening that gate is a separate
  testing-coverage concern.

### PR review 2 triage

| # | Finding | Class | Action |
| --- | --- | --- | --- |
| 1 | Give `push_cmd_outer_quote` compile-time enforcement (wrapper type or `#[inline(always)]`) so it cannot be repurposed with other raw strings | Reject | Dismissed with reasons recorded on the PR |

The helper's signature is `fn push_cmd_outer_quote(command: &mut Command)` — it
takes no string parameter, so no caller can supply raw text; the quote is a
literal in the body. A newtype constrains values crossing a signature and there
is none to constrain, and `#[inline(always)]` is a codegen hint with no safety
semantics. The finding's valid kernel — never extend `raw_arg` to
runtime-derived input — was already actioned in review 1 and is documented at
the call site.

## Review counters

- Local OCR runs: 1 / 2
- PR OCR runs: 2 / 2

### Local review 1 triage

Verdict: approve-with-nits, no blockers. Quoting correctness was confirmed
independently against the standard library's `make_command_line`, which emits a
separating space before every argument including a raw one, so the bracketing
quotes are never glued to the wrapper or to an argument.

| # | Finding | Class | Action |
| --- | --- | --- | --- |
| 1 | Outer-quote approach correct for spaced/unspaced wrappers, spaced args, embedded quotes, empty args, empty argv | No issue | none |
| 2 | `& \| ^ > < %` are not quoted by the encoder and stay cmd-interpreted | Defer | Pre-existing and byte-identical before/after this change; orthogonal to #529 |
| 3 | Spaced *arguments* covered only structurally, never by a real launch | In-scope-Fix | Fixed: added `spaced_arguments_survive_the_cmd_exe_boundary` |
| 4 | `agent_probe.rs` at 963 lines vs the 750 recommendation | Defer | Warning-only gate, under the 1000 hard limit; splitting it is an unrelated refactor |
| 5 | No `unwrap`/`expect` in production, no `unsafe`, no new dependency or public abstraction | No issue | none |
| 6 | `Direct` and `PowerShellScript` arms correctly untouched; `powershell -File` has no `/S` rule | No issue | none |

Finding 3 was taken because argument delivery is the specific behavior this
change puts at risk; findings 2 and 4 are pre-existing conditions that this
change does not worsen in kind and are out of scope for #529.

### PR review 1 triage

One actionable finding on PR #665, at the `push_cmd_outer_quote` helper. It
confirmed the implementation is safe — the only value passed to `raw_arg` is a
hardcoded quote — but asked that the constraint be recorded, since `raw_arg`
bypasses argument escaping and a later change routing a path or argument through
it would reintroduce command injection.

| # | Finding | Class | Action |
| --- | --- | --- | --- |
| 1 | `raw_arg` bypasses escaping; keep it restricted to the literal quote | In-scope-Fix | Fixed: constraint documented at the call site |

No defect was reported in the shipped behavior. All other CI review surfaces
passed with no findings.
