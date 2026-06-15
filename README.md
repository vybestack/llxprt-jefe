<h1>
  <img src="docs/assets/jefe-logo.svg" alt="jefe logo" width="42" />
  <a href="https://vybestack.dev/jefe.html">LLxprt Jefe</a>
</h1>

**Problem:** "I have too many terminals open and can't keep track of which agent is working on what issue."

`jefe` gives you one terminal control plane for multiple [`LLxprt Code`](https://vybestack.dev/llxprt-code.html) agents across repositories.

 New here? Start with the step-by-step guide: [`docs/getting-started.md`](docs/getting-started.md)

![jefe screenshot](docs/assets/jefe-screenshot.png)

## What it does for you

- shows all your repos and agents in one place,
- keeps each agent in its own tmux-backed session,
- lets you quickly jump to the right agent and terminal,
- makes it easy to see status/output without tab chaos,
- supports day-to-day operations (create, edit, kill, relaunch, delete),
- persists your setup/state between runs.

## Main UI modes

- **Dashboard**: repositories, agents, terminal, preview.
- **Split view**: compact cross-agent operational view.
- **Form / Confirm / Search / Help** modals.
- **Terminal capture mode**: key/mouse input forwarded into selected running agent.

## Keyboard highlights

- `F12` / `t`: toggle terminal capture focus.
- `Alt+1..9` (plus macOS Option-symbol fallback): jump to agent shortcuts.
- `n` / `N`: new agent / new repository.
- `Ctrl-d`: delete selected.
- `Ctrl-k`: kill selected agent.
- `v`: toggle active-only visibility (running repositories + running agents).
- `l`: relaunch dead agent.
- `s`: split view.
- `?` / `h` / `F1`: help.

## Platform support matrix

| Platform | Status | Sandbox Engines |
|----------|--------|-----------------|
| macOS (arm64/x86_64) | **Supported** | Podman, Docker, Seatbelt |
| Linux (x86_64/aarch64) | **Supported** | Podman, Docker |
| Windows | **Not supported** | — |

Seatbelt (macOS `sandbox-exec`) is available only on macOS. If a persisted
configuration references an unsupported engine, `jefe` normalizes it to Podman
at startup with a warning.

## Install jefe

### Homebrew (macOS/Linux)

```bash
brew tap vybestack/tap https://github.com/vybestack/homebrew-tap
brew install jefe
```

### Linux `.deb` packages (Ubuntu/Debian)

Pick the latest release and architecture-specific asset from:

https://github.com/vybestack/llxprt-jefe/releases/latest

Then install:

```bash
sudo dpkg -i ./jefe-vX.Y.Z-x86_64-unknown-linux-gnu.deb
# or
sudo dpkg -i ./jefe-vX.Y.Z-aarch64-unknown-linux-gnu.deb
```

If dependencies are missing:

```bash
sudo apt-get install -f
```

### Linux `.rpm` packages (Fedora/RHEL/openSUSE)

Pick the latest release and architecture-specific asset from:

https://github.com/vybestack/llxprt-jefe/releases/latest

Then install:

```bash
sudo rpm -i ./jefe-vX.Y.Z-x86_64-unknown-linux-gnu.rpm
# or
sudo rpm -i ./jefe-vX.Y.Z-aarch64-unknown-linux-gnu.rpm
```

(If upgrading an existing install, use `sudo rpm -Uvh ...`.)

## Install llxprt (required)

`jefe` launches `llxprt` agents, so install `llxprt` separately and ensure it is on PATH.

### Homebrew

```bash
brew tap vybestack/tap https://github.com/vybestack/homebrew-tap
brew install llxprt
```

### npm

```bash
npm i -g @vybestack/llxprt-code
```

## Persistence and paths

By default, `jefe` resolves settings/state using platform paths, with env var overrides:

- `JEFE_SETTINGS_PATH`
- `JEFE_CONFIG_DIR`
- `JEFE_STATE_PATH`
- `JEFE_STATE_DIR`

### Running multiple instances with `--config`

Use the `--config <dir>` (short `-c <dir>`) runtime argument to point an
instance at an isolated config directory. Both `settings.toml` and `state.json`
live directly under that directory, and external themes are loaded from
`<dir>/themes/`:

```bash
# Default instance
jefe

# Isolated dev/test instance that won't touch your main config/state
jefe --config ~/.config/jefe-dev
```

This is handy when developing `jefe` with `jefe`: run a stable instance in one
tab while testing changes against a throwaway config in another. The directory
does not need to exist beforehand — it is created on first write. When
`--config` is supplied, it takes precedence over the `JEFE_*` path environment
variables.

Related runtime/env toggles:

- `JEFE_WINDOWED=1` to disable fullscreen mode.
- `JEFE_LOG_FILE` and `JEFE_LOG` for structured logging output/filtering.

## tmux clipboard note

If `llxprt` copy appears not to work inside `jefe`, verify tmux clipboard settings:

```bash
tmux set-option -g set-clipboard on
tmux set-option -g allow-passthrough on
tmux set-window-option -g allow-passthrough on
```

You can check current values with:

```bash
tmux show-options -g set-clipboard
tmux show-options -g allow-passthrough
tmux show-window-options -g allow-passthrough
```

## For contributors

Build/test/developer details moved to [`docs/building.md`](docs/building.md).

Local PR gate command (matches CI):

```bash
make build
```

(`make ci-check` is kept as an alias.)
