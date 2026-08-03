# Issue #627 — Enter submits as a newline and Ctrl+Enter steering is unreachable in the embedded agent terminal

> jefe's embedded agent terminal is a terminal emulator, and it currently
> violates two parts of that contract. It never answers the child's identity and
> mode queries, so a child can never negotiate the kitty keyboard protocol and
> can never see a distinguishable `Ctrl+Enter`; and it flushes a whole batch of
> queued key events into the PTY in one instant, so the child sees a submit
> `Enter` arrive in the same burst as the character before it. Agent TUIs use a
> short burst window to tell a pasted CR from a pressed Enter, so both defects
> land on the same user-visible symptom: Enter inserts a newline and steering
> never fires.

## Root cause summary

| Defect | Location | Effect |
|---|---|---|
| `TermEvent::PtyWrite` replies dropped | `src/runtime/attach.rs` `RuntimeListener` | Child capability queries (DA1, kitty flags, XTVERSION, `modifyOtherKeys`, DSR) are never answered; children stall on detection and fall back to legacy key encoding |
| `Ctrl+Enter` encoded as bare LF | `src/pty_encoding.rs` | `0x0A` is indistinguishable from `Ctrl+J`; readline-style parsers report `Ctrl+J`, which agent TUIs bind to "insert newline", so the steer chord is unaddressable |
| Batched key events written with no separation | `src/app_input/pty_passthrough.rs` -> `AttachedViewer::write_input`, driven by iocraft's drain-all `poll_change` | The submit Enter lands in the same instant as the preceding keystroke and is reclassified as pasted content |

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Targets | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | Attached agent child writes a terminal query (`CSI c`, `CSI ? u`, `CSI > q`, `CSI > 4 ; ? m`, DSR) into the embedded terminal model | Any query `alacritty_terminal` answers with `Event::PtyWrite` | All platforms | The reply bytes are written to that child's PTY input, unchanged and in order | Write failure is logged at `warn` with the error; the session keeps running | None beyond the attempted write | No stored state; wire behavior only | Unit test feeding query bytes through the terminal parser and asserting the exact reply bytes reach a capture writer |
| A2 | Same path | `TermEvent::ClipboardStore` (OSC 52) | All platforms | Still forwarded to the host clipboard boundary exactly as today, never to the child's input | Unchanged `warn` diagnostic | Unchanged | Unchanged | Existing injected-boundary clipboard tests stay green; the clipboard and query-reply paths remain distinct functions with distinct sinks. The OSC 52 host boundary opens `/dev/tty`, so no test drives a real clipboard event through the parser |
| A3 | Same path | Any other `TermEvent` (bell, title, wakeup, ...) | All platforms | Nothing is written into the child's PTY input | n/a | None | Unchanged | Unit test asserting the capture writer stays empty |
| A4 | User presses `Ctrl+Enter` with the agent terminal focused, child has pushed kitty keyboard flags (`CSI > 1 u`) | `KeyCode::Enter` + `CONTROL` | All platforms | The child receives `ESC [ 13 ; 5 u` | n/a | None | Legacy encodings untouched for children that never enable the protocol | Encoder unit test plus a `TermMode` test proving the pushed flags are observable |
| A5 | Same, `Shift+Enter` | `Enter` + `SHIFT` | All platforms | The child receives `ESC [ 13 ; 2 u` | n/a | None | The backslash-CR compatibility form remains for non-protocol children | Encoder unit test for both modes |
| A6 | Same, `Alt+Enter` | `Enter` + `ALT` | All platforms | The child receives `ESC [ 13 ; 3 u` (no extra `ESC` prefix) | n/a | None | as above | Encoder unit test asserting no double escape |
| A7 | Same, plain `Enter` | `Enter`, no modifiers | All platforms | The child receives `CR` (`0x0D`), because flag 1 disambiguation leaves unmodified Enter legacy | n/a | None | Unchanged from today | Encoder unit test |
| A8 | Any key with the agent terminal focused, child has **not** enabled the kitty keyboard protocol | Every key currently encoded | All platforms | Byte-for-byte identical to today, including `Ctrl+Enter` -> `LF` and `Shift+Enter` -> `\` + `CR` | n/a | None | Full backward compatibility for children that do not negotiate | Existing encoder tests stay green unchanged |
| A9 | Any caller of the encoder | Choosing an encoding | All platforms | The encoding is chosen only from the observed child state, with `Legacy` as the default, so no caller can silently pick a third behavior | n/a | None | Unchanged | Unit test over `PtyKeyEncoding::for_child` and its default |
| A10 | User presses `Enter` (any modifiers) after another keystroke was written to the same child less than the guard interval earlier | Batched key drain, worst case zero elapsed time | All platforms | jefe delays the Enter write so the child observes at least `SUBMIT_GAP` between the previous input byte and the Enter | n/a | The preceding keys are already written; only the Enter is held | No stored state | Pure-function tests over the pacing state, plus a runtime test proving an Enter write is separated from the previous write |
| A11 | Same, when the previous write to that child was already older than the guard interval | Typing at human speed | All platforms | No delay at all is added | n/a | None | Unchanged | Pure-function test asserting a zero delay |
| A12 | Any non-Enter write (characters, navigation, mouse reports, pastes, query replies) | Any | All platforms | Never delayed, and every write updates the "last input byte" instant used by A10 | n/a | None | Unchanged | Pure-function tests covering the record/observe contract |
| A13 | Runtime with no attached viewer | Any key | All platforms | Protocol interrogation reports "not enabled" and the write path behaves exactly as today | Existing `RuntimeError::WriteFailed` path unchanged | None | Unchanged | Stub-manager test |

## Non-goals

- Changing any agent's client-side heuristics. jefe owns the emulator contract;
  the child is not modified and is not required to change.
- Full kitty keyboard protocol coverage. Only the Enter chord family is switched
  to CSI-u, because that is the chord family this issue's behavior depends on.
  Letters, navigation keys, and function keys keep their current encodings even
  when the protocol is active.
- Honouring individual kitty flag bits beyond "disambiguation is on". jefe does
  not implement `REPORT_EVENT_TYPES`, `REPORT_ALTERNATE_KEYS`,
  `REPORT_ALL_KEYS_AS_ESC`, or `REPORT_ASSOCIATED_TEXT`.
- Replacing iocraft's drain-all event delivery or introducing a dedicated PTY
  writer thread/queue subsystem. The guard is applied at the existing write
  boundary.
- Guaranteeing the child *parses* the Enter in a separate read. If the child's
  own event loop is blocked longer than the guard interval it can still coalesce
  two separated writes into one read. jefe's obligation, and what is tested
  here, is that jefe stops destroying the separation it was given.
- Answering queries `alacritty_terminal` does not answer, or adding new query
  support to the terminal model.
- Any UI, keymap, action-registry, or settings change. No jefe-owned keybinding
  changes, so no keybind-bar or keymap projection updates.

## Vertical slices

### Slice 1 — Answer the child's terminal queries (A1–A3)

- **Owner / boundary:** `src/runtime` PTY-attachment boundary. The listener is
  the only new writer into the child's input, and it reuses the viewer's
  existing writer handle.
- **Allowed paths:** `src/runtime/attach.rs`, a new
  `src/runtime/attach_listener_tests.rs` if the in-file tests would push the
  file past the source-size gate, `project-plans/issue627-plan.md`.
- **RED:** a test drives `CSI ? u` and `CSI c` through a `Term<RuntimeListener>`
  built over a capture writer and asserts the reply bytes were written. Fails
  today because `RuntimeListener` discards everything but `ClipboardStore`.
- **GREEN:** `RuntimeListener` carries the viewer's writer and forwards
  `TermEvent::PtyWrite` payloads to it, logging write failures at `warn`.
  `ClipboardStore` handling is unchanged; all other events are still dropped.
- **Verification:** `cargo test --lib runtime::attach`, `cargo xtask quick`.
- **Stop for approval:** any need to change the reader thread, spawn sequencing,
  or the `RuntimeManager` trait shape beyond this slice.

### Slice 2 — CSI-u Enter encoding when the child negotiated the protocol (A4–A9, A13)

- **Owner / boundary:** `src/pty_encoding.rs` owns byte encoding;
  `src/runtime` owns the observed terminal mode; `src/app_input` joins them.
- **Allowed paths:** `src/pty_encoding.rs`, `src/pty_encoding_tests.rs` (test
  module extracted to keep the file inside the source-size gate),
  `src/app_input/pty_passthrough.rs`, `src/action_capture_emit.rs`,
  `src/runtime/attach.rs`, `src/runtime/manager.rs`,
  `src/runtime/stub_manager.rs`.
- **RED:** encoder tests assert `ESC [ 13 ; 5 u` / `; 2 u` / `; 3 u` for
  Ctrl/Shift/Alt Enter under an active protocol and `CR` for plain Enter; a
  runtime test asserts the pushed flags are observable after the child writes
  `CSI > 1 u`.
- **GREEN:** `key_to_bytes` takes a `PtyKeyEncoding` describing passthrough and
  protocol state instead of a bare `bool`; the Enter arm emits CSI-u only when
  the protocol is active and passthrough is off. `AttachedViewer` exposes the
  observed mode the same way it already exposes bracketed paste, and
  `forward_key_to_pty` reads it before encoding.
- **Verification:** `cargo test --lib pty_encoding runtime`, `cargo xtask quick`.
- **Stop for approval:** any request to extend CSI-u beyond the Enter family.

### Slice 3 — Keep the submit Enter out of the preceding keystroke's burst (A10–A12)

- **Owner / boundary:** `src/runtime` write boundary owns the pacing state
  (per attached PTY, no UI state, so it cannot itself trigger re-renders);
  `src/app_input` classifies the key.
- **Allowed paths:** `src/runtime/key_pacing.rs` (+ tests),
  `src/runtime/mod.rs`, `src/runtime/attach.rs`, `src/runtime/manager.rs`,
  `src/runtime/stub_manager.rs`, `src/app_input/pty_passthrough.rs`.
- **RED:** pure tests over the pacing state assert a full guard delay when the
  previous write was simultaneous, a partial delay part-way through the window,
  and no delay once the window has elapsed; a runtime-level test asserts an
  Enter write is separated from an immediately preceding write.
- **GREEN:** every PTY input write records its instant; an Enter write first
  waits out the remainder of the guard interval. The delay is bounded by the
  guard interval and is only ever paid once per Enter press.
- **Verification:** `cargo test --lib runtime::key_pacing`, `cargo xtask quick`.
- **Stop for approval:** any need for a writer thread, async timer, or event-loop
  restructure.

## Expected paths / architectural layers

- `src/runtime/attach.rs` — listener query replies, observed keyboard mode,
  paced writes.
- `src/runtime/key_pacing.rs` — pure pacing state and its tests.
- `src/runtime/manager.rs`, `src/runtime/stub_manager.rs`, `src/runtime/mod.rs` —
  runtime boundary plumbing for the two new observations.
- `src/pty_encoding.rs`, `src/pty_encoding_tests.rs` — Enter chord encoding.
- `src/app_input/pty_passthrough.rs` — reads the negotiated mode, classifies the
  Enter write.
- `src/action_capture_emit.rs` — call-site update for the encoder signature.
- `project-plans/issue627-plan.md` — this plan.

No new subsystem, dependency, workflow change, or quality-tool change is
authorized.

## Scope ledger

| Entry | Reason | Status |
|---|---|---|
| Extract `pty_encoding`'s test module into `src/pty_encoding_tests.rs` | The file is already above the 750-line recommended limit; slice 2 adds production code and tests to it, and the hard limit is 1000 | Accepted — required to keep the source-size gate green, same-file tests only |
| `RuntimeListener` becomes a struct with a writer handle instead of a unit struct | Required by A1; it is a private-to-runtime construction detail of `AttachedViewer` | Accepted — inside slice 1's boundary |
| `key_to_bytes` second parameter becomes `PtyKeyEncoding` instead of `bool` | Two independent booleans at one call site is exactly the primitive obsession the standards forbid | Accepted — mechanical call-site update, no behavior change when the protocol is off |
| The `passthrough_enter` encoder flag is removed with its two tests | It had no production caller: every call site passed `false`. Keeping an unreachable third way to encode Enter next to the new negotiated one reintroduces exactly the ambiguity this issue is about, and the dead-code gate rejects an unreachable variant. Its only unique coverage (legacy `Alt+Enter` keeping its ESC prefix) is preserved by a new legacy test | Accepted — inside slice 2's file and contract |
| `domain::keymap::pty_bytes_for_chord` doc updated | It named the removed `passthrough_enter` parameter | Accepted — one stale doc line. Deduplicating that second encoder against `pty_encoding` is recorded as a deferred finding, not done here |

## Review counters

- Open Code Review before PR: 0 of 2 used.
- Open Code Review after PR: 0 of 2 used.
- Subagent design/code review cycles: 0 of 2 used.

## Verification evidence

To be filled at the green checkpoint.

## Deferred findings

- `domain::keymap::pty_bytes_for_chord` is a second, independent implementation
  of the legacy PTY key encoding with no production caller (only its own tests).
  It now silently disagrees with `pty_encoding` for a negotiated child.
  Deduplicating or deleting it is a separate change.
- `harness/v1/pty.rs` documents that it forwards kitty keyboard-protocol replies
  but builds its terminal model with `TermConfig::default()`, where alacritty
  gates the protocol off. The claim is currently unreachable. The harness hosts
  jefe itself, which does not negotiate the protocol, so nothing observable
  depends on it today.

## TUI scenario note

No jefe-rendered output changes, so no automated tmux scenario asserts a new
frame. The PTY key transport is proven by the manual chord-passthrough family
already documented in `dev-docs/testing/tmux-harness.md`, which requires a live
agent; this issue extends that documented manual procedure rather than adding a
frame assertion that could not observe child-side key interpretation.
