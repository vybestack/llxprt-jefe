# Windows support

Jefe runs natively on x86_64 Windows using the Windows ConPTY pseudo-console
and the [psmux](https://github.com/psmux/psmux) multiplexer. It does not use
WSL, Cygwin, MSYS2, Docker, Git Bash, or any Unix userland for its supported
runtime or install path.

## Prerequisites

- Windows 10 (build 1903+) or Windows 11, x86_64.
- Windows Terminal (recommended) or the classic ConHost console.
- PowerShell 5.1+ (for the install script).
- The `psmux` multiplexer (tmux-compatible).
- `git` and `gh` (optional but recommended for repository/issue features).
- The `llxprt` and/or `code-puppy` agent runtimes.

### Install psmux

psmux is installed separately from Jefe using the qualified Winget package id:

```powershell
winget install --id marlocarlo.psmux --exact
```

Jefe rejects tmux binaries resolved through WSL, Cygwin, MSYS2, Git Bash, or
any other compatibility layer. Only native `psmux.exe` (minimum version 3.3.7)
is accepted. If `psmux` is missing, too old, or resolved through a
compatibility layer, `jefe doctor` reports the exact remediation command
above.

### Install Jefe

Download the latest portable zip from
[GitHub Releases](https://github.com/vybestack/llxprt-jefe/releases/latest)
(`jefe-vX.Y.Z-x86_64-pc-windows-msvc.zip`), extract it, and run:

```powershell
.\jefe-install.ps1 -Action Install
```

The release zip contains only `jefe.exe`, its `jefe.exe.sha256` checksum,
`LICENSE`, and the first-party `jefe-install.ps1` script. The installer verifies
that checksum before launching the staged executable. It never bundles psmux or
third-party binaries.

## Supported terminals

Jefe supports any terminal that hosts the ConPTY pseudo-console API:

- **Windows Terminal** (recommended) — detected via the `WT_SESSION`
  environment variable.
- **ConHost** (classic console host) — used when Windows Terminal is not
  detected.

The diagnostic report identifies the detected terminal host and proves a
transient ConPTY allocation can open and close without launching a shell or
persistent session.

## First launch

After installing Jefe and psmux, open a new PowerShell window (so the updated
`PATH` takes effect) and run:

```powershell
jefe
```

On first launch Jefe creates its config/state directory under the platform
default (`%APPDATA%\jefe`). You can isolate an instance with `--config <dir>`:

```powershell
jefe --config "$env:LOCALAPPDATA\jefe-dev"
```

## Persistence and recovery

Jefe stores settings and state under the platform default config directory
(or an explicit `--config <dir>`). On startup it reconciles persisted agents
against live psmux sessions: agents whose psmux session no longer exists are
marked dead and can be relaunched. Persisted definitions survive restart;
an in-flight process that no longer has a live psmux session cannot be restored.

While Jefe is running, it also monitors the shared psmux server identity. If the
server disappears or is replaced, affected agents are shown as **Server Lost**
rather than individually dead. Jefe retains their runtime bindings and launch
settings for deliberate recovery; it does not silently relaunch agents. The
psmux panes themselves cannot survive an explicit `kill-server`.

Relevant environment overrides:

- `JEFE_SETTINGS_PATH` — path to `settings.toml`.
- `JEFE_CONFIG_DIR` — config directory (parent of `settings.toml`).
- `JEFE_STATE_PATH` — path to `state.json`.
- `JEFE_STATE_DIR` — state directory.

## Upgrade and uninstall preservation

The first-party install script supports `Upgrade` and `Uninstall`:

```powershell
.\jefe-install.ps1 -Action Upgrade
.\jefe-install.ps1 -Action Uninstall
```

- **Upgrade** replaces only package-owned files (`jefe.exe`,
  `jefe.exe.sha256`, `LICENSE`, and the ownership marker) and ensures the user
  `PATH` entry exists.
- **Uninstall** removes only package-owned files and the Jefe user `PATH`
  entry. It uses an ownership marker (`.jefe-installed`) before any recursive
  removal and refuses to touch a directory it did not install.

Both operations preserve your configuration, state, and any psmux sessions.
Configuration and state survive uninstall unless you explicitly delete them.

The installer serializes Jefe lifecycle operations for the same install path.
Each user `PATH` change reads one snapshot and performs at most one registry
write. Windows does not provide a compare-and-swap operation for this value, so
unrelated software editing the user `PATH` at the same time can still race with
the installer; avoid simultaneous external `PATH` edits.

On every lifecycle action, the installer removes validly owned sibling backup
directories that have been stale for at least seven days. Fresh backups and
unowned or malformed directories are preserved, and cleanup failures produce a
warning rather than hiding the requested install action.

## Firewall and antivirus

Jefe launches `psmux.exe` and agent runtimes as child processes. If firewall
or antivirus software prompts:

- Allow `psmux.exe` and `jefe.exe` to run and communicate locally.
- psmux uses a private named namespace (`-L <namespace>`) for isolation; no
  network listener is required for local sessions.

Remote Linux agents use OpenSSH; see the remote agents section below.

## PATH and PATHEXT

Jefe resolves `psmux`, `git`, `gh`, and agent runtimes through the native
Windows `PATH` and `PATHEXT`. A multiplexer resolved through a compatibility
layer (WSL, Cygwin, MSYS2, Git Bash) is rejected with its path and the exact
native remediation. `jefe doctor` reports the resolved paths.

## Long paths

Windows applies a default `MAX_PATH` (260) limit. Jefe warns when
`LongPathsEnabled` is disabled or absent and when a resolved config/state path
approaches the limit. To enable long paths (administrator):

```powershell
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
  -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
```

## Clipboard

Jefe uses OSC 52 clipboard sequences with psmux passthrough. If copy appears
not to work, ensure the terminal supports OSC 52 and psmux passthrough is
enabled.

## Remote Linux agents

Jefe diagnoses the local host only; it does not probe remote hosts. Remote
Linux setup is documented separately. Use `ssh` and a remote `tmux`/agent
runtime on the Linux host; Jefe orchestrates the remote session from the
local Windows host.

## Logs

Structured logging is opt-in via `JEFE_LOG_FILE` (output path) and `JEFE_LOG`
(filter directive). No log file is written unless these are set.

## Environment overrides

- `JEFE_PSMUX_BIN` — explicit path to the `psmux.exe` binary (overrides PATH
  resolution).
- `JEFE_GIT_BIN` — explicit path to `git`.
- `JEFE_GH_BIN` — explicit path to `gh`.
- `JEFE_SSH_BIN` — explicit path to `ssh`.
- `JEFE_WINDOWED=1` — disable fullscreen mode.
- `JEFE_LOG_FILE` / `JEFE_LOG` — structured logging.

## Diagnostics (`jefe doctor`)

Run `jefe doctor` to classify local readiness. It is read-only: it never
initializes configuration, mutates state, or creates a session. It reports:

- Jefe version, git commit, platform, and architecture.
- psmux path, version, capabilities, and private namespace isolation.
- ConPTY availability and the detected terminal host.
- Git, `gh`/auth, and agent runtime presence.
- Config/state writability and Windows long-path policy.

Exit codes:

- `0` — all required startup checks pass (warnings may be present).
- `2` — a required startup blocker failed (psmux missing/incompatible/
  untrusted, ConPTY unavailable, or persistence path unwritable).
- `1` — the diagnostic command itself could not complete.

### Safe artifact redirection and redaction

`jefe doctor` applies redaction to every evidence line before output, masking
usernames, home paths, raw SIDs, tokens, credentials, and prompts while
preserving structural labels (host names, tool names, section headers). To
save a report safely:

```powershell
jefe doctor > jefe-doctor.txt 2>&1
```

The redirected file contains the same redacted output shown in the terminal.
