# Multiplexer-backed TUI harness

The harness runs `jefe` inside a real terminal session and drives it with
keyboard input. It exists for deterministic end-to-end checks of behavior that is
hard to validate with reducer or component tests alone: focus changes, terminal
geometry, alternate-screen rendering, scrollback, process exit, and failure
artifacts.

Unix uses upstream tmux. Native Windows uses psmux and Windows ConPTY while
sharing the same typed scenario schema, matchers, runner, and artifact model.

The harness is intentionally split by side-effect boundary:

1. **Scenario model and macro expansion** parse JSON into typed Rust structs.
2. **Screen and scrollback matchers** evaluate captured text without I/O.
3. **Multiplexer driver** owns upstream tmux or native psmux process calls.
4. **Runner/orchestrator** composes the pure layers with the driver seam,
   bounded polling, and artifact capture.
5. **CLI and scenarios** provide local/manual entry points and opt-in smoke
   checks.

The production `jefe` binary does not contain test orchestration logic. The
separate `jefe-tmux-harness` tool starts a real `jefe` binary with an isolated
`--config` directory so developer state is never read or mutated.

## Scenario JSON schema

A scenario is a JSON object with `config`, optional `macros`, and `steps`.

```json
{
  "config": {
    "cols": 100,
    "rows": 32,
    "history_limit": 2000,
    "initial_wait_ms": 100,
    "out_dir": "target/tmux-harness/example",
    "keep_session": false,
    "assert_mode": "strict"
  },
  "macros": {
    "quit": {
      "params": [],
      "steps": [
        { "key": "q" },
        { "waitForExit": 3000 }
      ]
    }
  },
  "steps": [
    { "waitFor": "LLxprt Jefe" },
    { "macro": "quit", "args": {} }
  ]
}
```

### Config fields

- `cols` and `rows`: tmux pane geometry. Use fixed values for deterministic
  rendering.
- `history_limit`: retained scrollback lines.
- `initial_wait_ms`: optional startup pause before the first step.
- `wait_timeout_ms`: optional timeout for `waitFor` and `waitForNot`; zero or
  omission uses the platform default.
- `out_dir`: optional default artifact directory. The CLI `--out-dir` overrides
  it.
- `keep_session`: keep tmux alive after completion for manual debugging.
- `assert_mode`: `strict` aborts on first assertion failure; `soft` records
  assertion failures and continues.

## Step catalog

Each step object has one primitive key.

| Step | Example | Behavior |
| --- | --- | --- |
| `wait` | `{ "wait": 100 }` | Sleep for milliseconds. Prefer `waitFor` where possible. |
| `line` | `{ "line": "hello" }` | Type a full line and press Enter. |
| `key` | `{ "key": "?" }` | Send one tmux key token. |
| `keys` | `{ "keys": ["Tab", "Enter"] }` | Send a sequence of key tokens. |
| `waitFor` | `{ "waitFor": "Help" }` | Poll the screen until a literal appears. |
| `waitForNot` | `{ "waitForNot": "Loading" }` | Poll the screen until a literal disappears. |
| `expect` | `{ "expect": "new-agent" }` | Assert the current screen contains a literal. |
| `expectRightEdge` | `{ "expectRightEdge": "╮" }` | Assert a full-width line ends with a literal at the viewport's right edge. |
| `expectCount` | `{ "expectCount": "│", "count": 4 }` | Assert an exact literal count on screen. |
| `capture` | `{ "capture": "after-help" }` | Write `<label>.screen.txt` to the artifact dir. |
| `historySample` | `{ "historySample": "before" }` | Save scrollback and history size under a label. |
| `expectHistoryDelta` | `{ "expectHistoryDelta": "before" }` | Assert scrollback/history changed since the sample. |
| `copyMode` | `{ "copyMode": true }` | Enter or exit tmux copy mode. |
| `waitForExit` | `{ "waitForExit": 3000 }` | Poll `pane_dead` until the app exits. |
| `macro` | `{ "macro": "quit", "args": {} }` | Expand a named macro before execution. |

All screen matching is literal. If future scenarios need regular expressions,
add that as a typed matcher extension rather than smuggling dynamic behavior
through JSON.

## Running locally

Build the binary and harness, create an isolated config directory, and run a
scenario:

```bash
cargo build --workspace --all-features --locked
mkdir -p /tmp/jefe-harness-config
cargo run --bin jefe-tmux-harness -- \
  --scenario dev-docs/tmux-scenarios/startup-quit.json \
  --jefe-bin target/debug/jefe \
  --config /tmp/jefe-harness-config \
  --out-dir target/tmux-harness/startup-quit
```

To debug a failing scenario, add `--keep-session` and inspect the named tmux
session printed by the scenario file or CLI defaults.

### Native Windows with psmux

Install psmux 3.3.6 or newer, then run the same scenario JSON from PowerShell:

```powershell
cargo build --workspace --all-features --locked
$root = (Get-Location).Path
cargo run --bin jefe-tmux-harness -- `
  --scenario "$root\dev-docs\tmux-scenarios\startup-quit.json" `
  --jefe-bin "$root\target\debug\jefe.exe" `
  --config "$root\target\tmux harness Ω\config" `
  --out-dir "$root\target\tmux harness Ω\startup-quit"
```

Set `JEFE_PSMUX_BIN` when psmux is not on `PATH`. Each invocation creates a
unique `psmux -L <namespace>` namespace. Cleanup calls `kill-server` only with
that owned `-L` namespace; the harness never invokes bare `psmux kill-server`.
Use `--keep-session` to retain the isolated namespace. The CLI prints the
session and the path to `multiplexer.txt`, which records the executable,
qualified version, and namespace needed for safe inspection:

```powershell
psmux -L <namespace> list-sessions
psmux -L <namespace> capture-pane -p -S -200 -t <session>
psmux -L <namespace> kill-server
```

Missing or older psmux versions produce an actionable error identifying the
executable, minimum version, and `JEFE_PSMUX_BIN` override. Native-Windows
claims do not use WSL, Cygwin, MSYS2, Git Bash, Docker, or Unix shell wrappers.

## Artifact layout

When an artifact directory is supplied, the runner may write:

- `final-screen.txt`: final screen capture on failure.
- `final-scrollback.txt`: final scrollback capture on failure.
- `error.txt`: structured failure context including step index, step kind, and
  reason.
- `multiplexer.txt`: multiplexer executable/version and isolated namespace.
- `<label>.screen.txt`: named captures from `capture` steps.
- `<label>.history.txt`: named scrollback samples from `historySample` steps.

Artifact labels are sanitized before writing, so scenario names cannot escape the
artifact directory.

## Deterministic scenario guidance

- Pin `cols`, `rows`, and `history_limit`.
- Always pass an isolated `--config` directory.
- Prefer `waitFor`/`waitForNot`/`waitForExit` over fixed sleeps.
- Avoid spinner frames, elapsed-time text, network-backed GitHub lists, or local
  machine state unless the scenario explicitly sets up that state.
- Capture useful checkpoints before quitting so alternate-buffer teardown does
  not hide the interesting screen.
- Keep scratch/manual scenarios outside required CI until they prove stable.

## Tmux availability and skipping

Unit tests use fake drivers for deterministic runner behavior. Guarded real tmux
smoke tests skip cleanly when `tmux` is unavailable. The optional CI smoke job is
manual/opt-in and also skips when `tmux` cannot be installed or found.

## Included scenarios

- [`startup-quit.json`](../tmux-scenarios/startup-quit.json): waits for the
  dashboard keybind bar, captures the screen, quits, and waits for exit.
- [`help-modal.json`](../tmux-scenarios/help-modal.json): opens the help modal,
  verifies its stable title, captures it, closes it, then quits.
- [`fork-issue-pr-repository.json`](../tmux-scenarios/fork-issue-pr-repository.json):
  opens New Repository and verifies the optional Issues / PRs Repo override and
  its blank-fallback guidance for fork configurations.
- [`scratch-pr-mode.json`](../tmux-scenarios/scratch-pr-mode.json): manual
  scratch scenario for PR-mode screen validation. It is intentionally not a CI
  gate because repository/GitHub configuration can vary by developer machine.
- [`actions-mode.json`](../tmux-scenarios/actions-mode.json): uses the fail-closed
  `scripts/issue194-gh-shim.sh` fixture to load runs returned oldest-first from
  the API, asserts the newest run is selected first (issue #208), triggers
  load-more via `End` and checks the archived page-2 row appears, then enters
  job detail on the newest run, verifies jobs are collapsed by default, expands
  success and failure steps with their status glyphs, collapses with `Esc`,
  navigates job focus, and backs out to the run list. Run it end to end with
  `scripts/issue194-run-scenario.sh`; the runner seeds isolated state and audits
  that production performs only the expected read-only `gh` operations.
- [`code-puppy-chord-passthrough.json`](../tmux-scenarios/code-puppy-chord-passthrough.json):
  manual scenario that focuses an agent terminal and sends the Code Puppy
  shell-control chords (`Ctrl-X Ctrl-B`, `Ctrl-X Ctrl-X`, `Ctrl-C`) through
  jefe's embedded terminal. It is intentionally not a CI gate — it requires a
  configured repository, a running Code Puppy agent, and a live long-running
  foreground shell command inside the agent pane. The deterministic,
  CI-gated proof that these control bytes reach the child unchanged lives in
  the runtime unit tests (`prefix_passthrough_tests`), which drive a real
  `tmux attach-session` client on an isolated socket with the prefix disabled
  exactly as production does (#200).
- [`kennel-terminal-select.json`](../tmux-scenarios/kennel-terminal-select.json):
  manual scratch scenario for issue #197 — terminal text selection and copy for
  Code Puppy (Kennel mode) sessions. It focuses the terminal and captures the
  focused Kennel terminal screen. It is intentionally not a CI gate because it
  requires a configured repository with a running Code Puppy agent (which varies
  by developer machine), and the keyboard-only harness cannot drive mouse
  drag-select or assert OSC 52 clipboard contents. The behavioral contract
  (plain drag and shift-drag paint a Jefe selection and copy over the snapshot
  for Kennel agents; LLxprt keeps PTY forwarding when mouse reporting is active)
  is covered by unit tests in `tests/runtime/terminal_focus_routing.rs` and
  `src/selection/terminal_mouse_policy.rs`. Run the scenario manually:

  ```bash
  cargo run --bin jefe-tmux-harness -- \
    --scenario dev-docs/tmux-scenarios/kennel-terminal-select.json \
    --jefe-bin target/debug/jefe \
    --config /tmp/jefe-harness-config \
    --out-dir target/tmux-harness/kennel-terminal-select \
    --keep-session
  ```

  Then, in the kept session, drag across visible Code Puppy output: the
  selected cells should highlight (inverse video) and release should copy the
  highlighted text. Holding Shift while dragging must also highlight and copy
  (it is no longer a no-op).
- [`auth-dialog.json`](../tmux-scenarios/auth-dialog.json): manual scenario for
  issue #244 — the in-app device-code auth remediation dialog. It enters Issues
  mode, waits for the "Authenticate with GitHub" dialog to appear (when `gh` is
  unauthenticated), and cancels with Esc. It is intentionally **not** a CI gate
  because it requires an unauthenticated `gh` and a real browser authorization
  to complete the device-code flow. The deterministic proof that the dialog
  opens on a `NotAuthenticated` failure, that the one-time code + URL are
  parsed from `gh auth login --web` stderr, that the requested scopes are
  exactly `repo`, `read:org`, `gist`, and that the state machine transitions
  (idle → awaiting-code → confirming → success / failure / cancelled) live in
  unit tests (`github_tests::auth_device`, `state::auth_ops_tests`,
  `app_input::auth_remediation`, and `ui::modals::auth`).

## Future regression scenarios

Once the scratch flows are stable, add scenarios for list viewport following,
filter controls, and inline composer caret behavior. Keep them local/manual until
they can be made deterministic with isolated config and predictable data.

## Agent-runtime choice and process-start detection

The `agent-runtime-choice.json` scenario verifies the New Repository modal
shows a "Default Agent" field. It does **not** cycle the Default Agent kind
(pressing Space on that field) because the harness cannot deterministically
control which agent runtimes (`llxprt`, `code-puppy`) are installed on the
local PATH. The `AppState.installed_agent_kinds` snapshot is captured once at
process startup by probing the local PATH; the harness starts `jefe` with an
isolated `--config` directory but has no mechanism to inject a custom PATH or
seed the runtime snapshot. As a result, cycling is only deterministic when both
runtimes happen to be installed in the developer's environment, which is not
guaranteed across CI/local machines.

The cycling behavior itself is covered by unit tests:

- `form_runtime::next_installed_kind` — pure kind cycling logic.
- `form_ops` tests — Space on `AgentKind`/`DefaultAgentKind` focus cycles to
  the next installed kind.

Remote availability probing (issue #184 defects 2-4) is covered by
`remote_probe_tests.rs` — the pure classifier/planner seam tests prove
unavailable remote means no prep/prompt operation without needing a live SSH
connection.
