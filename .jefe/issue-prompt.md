# GitHub Issue #200: Preserve Code Puppy shell-control chords through the embedded tmux terminal

**Repository:** jefe
**State:** open
**Labels:** bug, enhancement

## Body

## Summary

Code Puppy's in-flight shell control shortcuts do not work reliably through Jefe's embedded terminal:

- Ctrl+X Ctrl+B should background all running shell commands.
- Ctrl+X Ctrl+X should kill all running shell commands.
- Ctrl+C should kill running shells and cancel the agent run.

These shortcuts work when Code Puppy runs directly in a regular terminal but do not appear to take effect in Jefe.

## Reproduction

1. Launch a Code Puppy agent in Jefe and focus its embedded terminal.
2. Ask it to run a long foreground command such as pytest.
3. While the command is running, press Ctrl+X followed by Ctrl+B.
4. Repeat with Ctrl+X followed by Ctrl+X.
5. Separately test Ctrl+C.

## Expected behavior

- Ctrl+X Ctrl+B backgrounds the running command and Code Puppy continues interactively.
- Ctrl+X Ctrl+X kills the running shell command without killing the Code Puppy session.
- Ctrl+C kills running shells and cancels the active agent operation, matching native Code Puppy behavior.
- Code Puppy's chord hint/status feedback remains visible in the embedded terminal.

## Actual behavior

The chords do not appear to trigger when Code Puppy runs through Jefe.

## Source analysis

Jefe correctly encodes Ctrl+X as byte 0x18, Ctrl+B as 0x02, and Ctrl+C as 0x03 before writing to the attached PTY. However, the attached viewer itself runs an interactive tmux client. Jefe starts tmux with `-f /dev/null`, which leaves tmux's default prefix as Ctrl+B. Therefore the second key in Code Puppy's Ctrl+X Ctrl+B chord is consumed by tmux's client key table instead of reaching Code Puppy. This is a concrete collision between Jefe's transport layer and Code Puppy's application-level chord.

Code Puppy confirms the intended protocol in its source:

- `command_runner._register_shell_chords` binds 0x18 to kill shells and 0x02 to background shells while commands are active.
- The first Ctrl+X arms the line editor's chord state.
- The follow-up control byte dispatches the registered action.

Ctrl+X Ctrl+X and Ctrl+C are not tmux-prefix collisions, so their failure needs an end-to-end byte/terminal-mode investigation rather than assuming the same root cause. Verify whether bytes are written once, survive the tmux client, reach Code Puppy's controlling tty, and are consumed by its active key-listener/editor.

## Requested behavior

The embedded agent terminal must transparently carry application control chords.

- Jefe's internal tmux transport must not reserve Ctrl+B or any other ordinary agent shortcut while terminal capture is active.
- Prefer disabling the tmux prefix for Jefe's private, programmatically managed sessions/viewer or moving all Jefe tmux control behind APIs that do not consume child input.
- Do not solve this with a Code Puppy-specific byte rewrite; preserve raw PTY semantics for all runtimes.
- Keep Jefe's own terminal escape/focus mechanism (F12) working.
- Diagnose Ctrl+X Ctrl+X and Ctrl+C independently and ensure key Press/Repeat/Release handling does not drop or duplicate control bytes.
- Remote and local sessions must behave identically.

## Acceptance criteria

- A PTY/tmux integration test proves 0x18 0x02 reaches the child unchanged and in order.
- A test proves 0x18 0x18 reaches the child unchanged and in order.
- A test proves 0x03 reaches the child once.
- A Code Puppy TUI harness scenario starts a long-running shell and verifies background, shell-kill, and cancel behavior separately.
- LLxprt control-key passthrough remains covered.
- No user-facing raw tmux prefix is required to control Jefe's managed session.

