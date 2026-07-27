# Native Windows psmux compatibility smoke suite

The psmux smoke suite qualifies the real native-Windows multiplexer behavior
that Jefe's runtime and TUI harness depend on. It does not use WSL, Cygwin,
MSYS2, Git Bash, Docker, or a Unix shell.

## Supported version

The minimum qualified version is **psmux 3.3.7**. Version 3.3.6 can leak
processes and hang one-shot `new-session` commands; 3.3.7 contains the upstream
process-teardown and command-reliability fixes. Install it with:

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

| Jefe contract | Command exercised | Qualified 3.3.7 behavior |
| --- | --- | --- |
| Version policy | `psmux -V` | Emits `tmux 3.3.7`; parsed and minimum-enforced |
| Isolated server | `psmux -L <name> ...` | Namespaces cannot see or terminate each other's sessions |
| Session creation and geometry | `new-session -d -s <session> -x 100 -y 32 -c <dir> <fixture>` | Starts a native process in the explicit Unicode/space-containing directory |
| Session liveness | `has-session -t <session>` | Success while the owned session exists |
| Session enumeration | `list-sessions -F '#{session_name}'` | Reports only sessions in the selected namespace |
| Pane discovery | `list-panes -t <session> -F <format>` | Reports session/window/pane indexes, PID, dead state, dimensions, and history size |
| Runtime formats | `display-message -p -t <session> '#{...}'` | Reports pane dead state, dimensions, and history size |
| Prefix passthrough | `set-option -t <session> prefix None` and `prefix2 None` | Accepts the options used for transparent control-key forwarding |
| Dead-pane retention | `set-option -t <session> remain-on-exit on` | Retains exited panes and reports `pane_dead=1` |
| Clipboard passthrough | `set-option -g set-clipboard on`; global/pane `allow-passthrough on` | Accepts Jefe's clipboard and escape-passthrough options |
| Harness history | `set-option -wt <session> history-limit 2000`; `#{history_size}` | Accepts the configured capacity; detached 3.3.7 reports `history_size=0` while bounded `capture-pane -S` still returns pane output |
| Literal and named input | `send-keys -l ...`; `send-keys ... Enter Escape Tab Up Down C-c C-d` | Delivers literal UTF-8, Enter, Tab, Ctrl-C, and Ctrl-D; accepts Escape/arrows but detached 3.3.7 does not forward those keys to the raw fixture |
| Screen/history capture | `capture-pane -p -S <start> -E - -t <session>` | Returns visible output and bounded scrollback |
| Resize request | `resize-window -t <session> -x 90 -y 28` | Command succeeds; detached 3.3.7 retains its initial `100x32` geometry until an attached client supplies size |
| Session cleanup | `kill-session -t <session>` | Terminates only the selected session |
| Namespace cleanup | `psmux -L <name> kill-server` | Terminates only the explicitly named namespace |
| Mouse-mode advertisement observation (#296) | `AttachedViewer::spawn_with_plan` over a fixture emitting `\x1b[?1000h ?1002h ?1006h` | Jefe's embedded terminal model observes the advertised DEC private mouse modes and reports `mouse_reporting_active() == true` after attach |
| Page-key byte delivery (#296) | `AttachedViewer::write_input(b"\x1b[5~")` / `b"\x1b[6~")` | The exact `CSI 5~` / `CSI 6~` byte sequences reach the child (not arrow sequences `CSI A` / `CSI B`) |
| SGR mouse byte delivery (#296) | `AttachedViewer::write_input(b"\x1b[<0;1;1M")` | The SGR mouse sequence reaches the child intact |

The detached resize result is recorded rather than overstated: the command is
accepted, but psmux 3.3.7 continues to report the creation geometry without an
attached client. Interactive resize through ConPTY belongs to the attachment
qualification work.

## macOS `fn`+Arrow Page-key translation (issue #296)

When a macOS client connects through Microsoft Windows App and presses
`fn`+Up / `fn`+Down (the macOS gesture that normally yields PageUp / PageDown),
the Microsoft Windows App input-mapping layer translates the gesture to a plain
arrow key **before** it reaches Jefe. Jefe therefore receives `KeyCode::Up` /
`KeyCode::Down` (encoded correctly as `CSI A` / `CSI B`) and never sees a
`PageUp` / `PageDown` event to encode as `CSI 5~` / `CSI 6~`.

This is a client-side translation, not a Jefe bug, and Jefe intentionally does
**not** infer `fn` intent: there is no reliable way to distinguish a real arrow
key from a translated page-key once the OS has collapsed them, and any
heuristic would mis-route genuine arrow input.

Jefe's encoder remains correct for any true physical PageUp / PageDown key
(`KeyCode::PageUp` → `CSI 5~`, `KeyCode::PageDown` → `CSI 6~`), locked by the
`modified_edit_keys_use_xterm_sequences` and `nav_key_bytes` unit tests.

**Verified alternative for macOS + Windows App users:** use the PageUp /
PageDown keys available on a full keyboard (e.g. an external keyboard with
dedicated Page keys), or remap the gesture at the Microsoft Windows App /
macOS client level so PageUp / PageDown are delivered as distinct keys rather
than `fn`+Arrow.

## Failure artifacts

A failed command reports and writes:

- exact executable, namespace, and argv;
- exit status, stdout, and stderr;
- psmux version and minimum policy;
- the owned namespace name;
- command transcript and last captured pane state.

Artifacts are scoped to the repository `target` directory and contain no cleanup
commands for unrelated namespaces.
