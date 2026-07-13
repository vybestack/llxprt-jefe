# Native Windows psmux compatibility smoke suite

The psmux smoke suite qualifies the real native-Windows multiplexer behavior
that Jefe's runtime and TUI harness depend on. It does not use WSL, Cygwin,
MSYS2, Git Bash, Docker, or a Unix shell.

## Supported version

The minimum qualified version is **psmux 3.3.6**. Install it with:

```powershell
winget install marlocarlo.psmux
```

The suite uses `psmux.exe` by default. Set `JEFE_PSMUX_BIN` to test a specific
binary. Local runs print a skip reason when psmux is unavailable. Environments
that promise psmux availability must set `JEFE_REQUIRE_PSMUX=1`, which turns an
unavailable or incompatible binary into a test failure.

## Run the suite

```powershell
cargo test --features psmux-smoke --test psmux_smoke -- --nocapture
```

Each test owns a unique `-L` namespace. Cleanup always targets that namespace;
the suite never contacts the default server and never invokes bare
`kill-server`. Diagnostics are retained under `target/psmux-smoke/<namespace>`.

## Compatibility matrix

| Jefe contract | Command exercised | Qualified 3.3.6 behavior |
| --- | --- | --- |
| Version policy | `psmux -V` | Emits `tmux 3.3.6`; parsed and minimum-enforced |
| Isolated server | `psmux -L <name> ...` | Namespaces cannot see or terminate each other's sessions |
| Session creation and geometry | `new-session -d -s <session> -x 100 -y 32 -c <dir> <fixture>` | Starts a native process in the explicit Unicode/space-containing directory |
| Session liveness | `has-session -t <session>` | Success while the owned session exists |
| Session enumeration | `list-sessions -F '#{session_name}'` | Reports only sessions in the selected namespace |
| Pane discovery | `list-panes -t <session> -F <format>` | Reports session/window/pane indexes, PID, dead state, dimensions, and history size |
| Runtime formats | `display-message -p -t <session> '#{...}'` | Reports pane dead state, dimensions, and history size |
| Prefix passthrough | `set-option -t <session> prefix None` and `prefix2 None` | Accepts the options used for transparent control-key forwarding |
| Dead-pane retention | `set-option -t <session> remain-on-exit on` | Retains exited panes and reports `pane_dead=1` |
| Clipboard passthrough | `set-option -g set-clipboard on`; global/pane `allow-passthrough on` | Accepts Jefe's clipboard and escape-passthrough options |
| Harness history | `set-option -wt <session> history-limit 2000`; `#{history_size}` | Accepts the configured capacity; detached 3.3.6 reports `history_size=0` while bounded `capture-pane -S` still returns pane output |
| Literal and named input | `send-keys -l ...`; `send-keys ... Enter Escape Tab Up Down C-c C-d` | Delivers literal UTF-8, Enter, Tab, Ctrl-C, and Ctrl-D; accepts Escape/arrows but detached 3.3.6 does not forward those keys to the raw fixture |
| Screen/history capture | `capture-pane -p -S <start> -E - -t <session>` | Returns visible output and bounded scrollback |
| Resize request | `resize-window -t <session> -x 90 -y 28` | Command succeeds; detached 3.3.6 retains its initial `100x32` geometry until an attached client supplies size |
| Session cleanup | `kill-session -t <session>` | Terminates only the selected session |
| Namespace cleanup | `psmux -L <name> kill-server` | Terminates only the explicitly named namespace |

The detached resize result is recorded rather than overstated: the command is
accepted, but psmux 3.3.6 continues to report the creation geometry without an
attached client. Interactive resize through ConPTY belongs to the attachment
qualification work.

## Failure artifacts

A failed command reports and writes:

- exact executable, namespace, and argv;
- exit status, stdout, and stderr;
- psmux version and minimum policy;
- the owned namespace name;
- command transcript and last captured pane state.

Artifacts are scoped to the repository `target` directory and contain no cleanup
commands for unrelated namespaces.
