# Tmux-backed TUI harness

The tmux harness runs `jefe` inside a real terminal session and drives it with
keyboard input. It exists for deterministic end-to-end checks of behavior that is
hard to validate with reducer or component tests alone: focus changes, terminal
geometry, alternate-screen rendering, scrollback, process exit, and failure
artifacts.

The harness is intentionally split by side-effect boundary:

1. **Scenario model and macro expansion** parse JSON into typed Rust structs.
2. **Screen and scrollback matchers** evaluate captured text without I/O.
3. **Tmux driver** owns all `tmux -f /dev/null` process calls.
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

## Artifact layout

When an artifact directory is supplied, the runner may write:

- `final-screen.txt`: final screen capture on failure.
- `final-scrollback.txt`: final scrollback capture on failure.
- `error.txt`: structured failure context including step index, step kind, and
  reason.
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

- [`startup-quit.json`](./tmux-scenarios/startup-quit.json): waits for the
  dashboard keybind bar, captures the screen, quits, and waits for exit.
- [`help-modal.json`](./tmux-scenarios/help-modal.json): opens the help modal,
  verifies its stable title, captures it, closes it, then quits.
- [`scratch-pr-mode.json`](./tmux-scenarios/scratch-pr-mode.json): manual
  scratch scenario for PR-mode screen validation. It is intentionally not a CI
  gate because repository/GitHub configuration can vary by developer machine.

## Future regression scenarios

Once the scratch flows are stable, add scenarios for list viewport following,
filter controls, and inline composer caret behavior. Keep them local/manual until
they can be made deterministic with isolated config and predictable data.
