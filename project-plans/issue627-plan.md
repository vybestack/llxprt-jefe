# Issue #627 — Enter submits as a newline and Ctrl+Enter steering is unreachable in the embedded agent terminal

> A keystroke reaches a hosted agent through four hops: the host terminal, jefe,
> the multiplexer, and finally the agent's pane. `Ctrl+Enter` was lost at three
> of them, and a submit `Enter` was corrupted at the fourth. Fixing only one hop
> proves nothing, so each hop is established by measurement rather than by
> assumption.

## Measured behavior of the transport

Recorded against tmux 3.7b with an isolated configuration (`-f /dev/null`), in
jefe's own topology — an outer PTY running `tmux attach-session`, with the agent
in the pane. Reproduction scripts are throwaway probes, and the results they
established are:

| Question | Measured answer |
|---|---|
| Does the multiplexer forward a modified Enter to the pane? | Only when `extended-keys` is on. Its default is `off`, and then **every** modified Enter reaches the pane as a bare `CR` |
| Does it honour a pane's kitty keyboard request (`CSI > 1 u`)? | No. It is ignored entirely |
| Does it honour a pane's `modifyOtherKeys` request (`CSI > 4 ; 2 m`)? | Yes, when `extended-keys` is on |
| What does the pane then receive for `Ctrl+Enter`? | `CSI 27 ; 5 ; 13 ~` |
| Does the multiplexer parse `CSI 13 ; 5 u` from its own terminal as `Ctrl+Enter`? | Yes |
| What reaches a pane that asked for nothing? | `CR` — the multiplexer downgrades per pane |
| Does the multiplexer ask its own terminal to enable extended keys? | No. There is no signal jefe can observe |
| Which queries does the multiplexer answer for its pane? | XTVERSION and DA1 only; not the kitty query, not `modifyOtherKeys` |
| Which queries does the multiplexer send to its own terminal? | DA1, DA2, XTVERSION, OSC 10, OSC 11 — none of which jefe answered |

The last row explains the startup stall, and the "no observable signal" row is
why the encoding cannot be gated on anything jefe can see.

## Root causes

| # | Defect | Location | Effect |
|---|---|---|---|
| 1 | The host terminal is never asked to disambiguate key chords | `vendor/iocraft/src/terminal.rs` pushed only `REPORT_EVENT_TYPES` | `Ctrl+Enter` arrives at jefe as a bare `Enter`, or as `Ctrl+J`. Jefe cannot forward a chord it never receives |
| 2 | `Ctrl+Enter` was encoded as a bare `LF` | `src/pty_encoding.rs` | `LF` is byte-identical to `Ctrl+J`, which agent TUIs bind to "insert newline". There was no byte sequence that could express the chord |
| 3 | Extended-key reporting was left at the multiplexer's default | `src/runtime/commands_root_keys.rs` | The multiplexer collapsed every modified Enter to `CR` before the pane saw it, whatever jefe sent |
| 4 | Terminal query replies were discarded | `src/runtime/attach.rs` `RuntimeListener` | The multiplexer client's DA1/DA2/XTVERSION/OSC queries went unanswered, so it identified its terminal by timeout |
| 5 | Batched key events are written with no separation | `src/app_input/pty_passthrough.rs` -> `AttachedViewer`, driven by iocraft's drain-all `poll_change` | The submit Enter lands in the same instant as the preceding keystroke and is reclassified downstream as pasted content |

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Targets | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | User presses `Ctrl+Enter` with the agent terminal focused | `KeyCode::Enter` + `CONTROL` | All platforms | The multiplexer client receives `ESC [ 13 ; 5 u`, which it parses as a modified Enter | n/a | None | Children that ask for nothing still receive `CR` from the multiplexer | Encoder unit test, plus the measured multiplexer behavior above |
| A2 | Same | Any combination of `CONTROL` with other modifiers | All platforms | The modifiers accumulate into the one CSI-u parameter | n/a | None | as above | Encoder unit test |
| A3 | Same, plain `Enter` | `Enter`, no modifiers | All platforms | `CR`, unchanged | n/a | None | Unchanged | Encoder unit test |
| A4 | Same, `Shift+Enter` / `Alt+Enter` | `Enter` + `SHIFT` or `ALT` | All platforms | Byte-for-byte unchanged from today, so the issue #1 contract holds | n/a | None | Unchanged | Encoder unit test |
| A5 | Every other key | Any | All platforms | Byte-for-byte unchanged | n/a | None | Unchanged | Encoder unit test over the whole key families |
| A6 | jefe attaches to its own multiplexer server | Session setup | All platforms | Extended-key reporting is enabled, scoped to jefe's own server, so a pane that asks for extended keys receives the modified chord | Setup failure surfaces exactly as the existing prefix-configuration failures do | The other session options already applied | A pane that asks for nothing is unaffected | Unit tests over the emitted configuration commands |
| A7 | jefe's own terminal | Raw-mode setup | All platforms with a terminal that supports it | Escape-code disambiguation is requested, so modified Enter chords are delivered to jefe at all | Terminals without support are untouched, exactly as before | None | Terminals without keyboard-enhancement support behave as today | Vendored-iocraft change; the flag is only pushed when the terminal advertises support |
| A8 | Hosted client writes a terminal query | DA1, DA2, DSR, and anything else the model answers | All platforms | The reply reaches the client's PTY input, in order | Write failure is logged at `warn`; the session keeps running | None beyond the attempted write | No stored state | Unit tests feeding queries through the parser |
| A9 | Same | The parser is mid-parse, holding the terminal model | All platforms | The reply is queued, not written, and goes out only once the model is released | Queue-lock failure is logged at `warn` | None | n/a | Unit test asserting nothing is written during parsing |
| A10 | Same | `TermEvent::ClipboardStore` | All platforms | Still routed to the host clipboard boundary, never to the client's input | Unchanged `warn` | Unchanged | Unchanged | Existing injected-boundary tests; the two paths have distinct sinks |
| A11 | Same | Any other terminal event | All platforms | Nothing is written to the client | n/a | None | Unchanged | Unit test asserting the sink stays empty |
| A12 | User presses `Enter` (any modifiers) after another byte was written to the same child more recently than the guard interval | Batched key drain, worst case zero elapsed time | All platforms | The child observes at least `ENTER_INPUT_GAP` between the previous byte and the Enter | n/a | The preceding bytes are already written | No stored state | Pure pacing tests plus a writer-level test measuring the real separation |
| A13 | Same, previous write already older than the guard | Typing at human speed | All platforms | No delay is added | n/a | None | Unchanged | Pure pacing test and a writer-level test |
| A14 | Any non-Enter write — characters, navigation, mouse reports, pastes, query replies | Any | All platforms | Never delayed, and every one of them updates the mark the next Enter measures against | n/a | None | Unchanged | Pure pacing tests plus a writer-level test where a query reply lands between two keystrokes |
| A15 | Runtime with no attached viewer | Any key | All platforms | Unchanged `RuntimeError::NoAttachedViewer` behavior | Unchanged | None | Unchanged | Stub-manager path unchanged |

## Non-goals

- Implementing the kitty keyboard protocol. The multiplexer ignores it, so jefe
  neither advertises nor implements it; the terminal model keeps alacritty's
  default configuration rather than claiming a protocol it would not honour.
- Extending CSI-u encoding beyond `Ctrl+Enter`. That is the one chord whose
  legacy encoding was ambiguous; every other key keeps its bytes.
- Removing the `Shift+Enter` backslash-CR compatibility form (issue #1).
- Replacing iocraft's drain-all event delivery or adding a PTY writer thread.
- Guaranteeing the child *parses* the Enter in a separate read. If the child's
  own event loop is blocked longer than the guard interval it can still coalesce
  two separated writes into one read. Jefe's obligation, and what is tested here,
  is that it stops destroying the separation it was given.
- Wiring up `src/runtime/commands_tests.rs`, which is orphaned (see deferred
  findings).

## Vertical slices

### Slice 1 — Answer the hosted client's terminal queries (A8–A11)

- **Owner / boundary:** `src/runtime` PTY-attachment boundary.
- **GREEN:** the terminal-model listener queues `TermEvent::PtyWrite` payloads;
  the reader thread writes them once it has released the model.
- **Verification:** `cargo test --lib runtime::attach`.

### Slice 2 — Express `Ctrl+Enter` at all (A1–A5)

- **Owner / boundary:** `src/pty_encoding.rs` owns byte encoding.
- **GREEN:** `Ctrl+Enter` is `ESC [ 13 ; <mods> u`; every other key is unchanged.
- **Verification:** `cargo test --bin jefe pty_encoding`.

### Slice 3 — Keep the submit Enter out of the preceding write's burst (A12–A14)

- **Owner / boundary:** `src/runtime` write boundary owns the pacing state,
  behind the same lock as the writer; `src/app_input` classifies the key.
- **GREEN:** every PTY input write records its instant; an Enter waits out the
  remainder of the guard interval first.
- **Verification:** `cargo test --lib runtime::key_pacing runtime::attach`.

### Slice 4 — Carry the chord across the remaining two hops (A6, A7)

- **Owner / boundary:** `src/runtime/commands_root_keys.rs` owns multiplexer
  session configuration; `vendor/iocraft` owns jefe's own terminal setup.
- **GREEN:** extended-key reporting is enabled on jefe's own multiplexer server,
  and jefe asks its host terminal for escape-code disambiguation.
- **Verification:** `cargo test --lib commands_root_keys`, plus the measured
  transport behavior recorded above.

## Expected paths / architectural layers

- `src/runtime/attach.rs`, `src/runtime/attach_listener.rs` — query replies and
  the single paced write path.
- `src/runtime/key_pacing.rs` — pacing state and the paced PTY writer.
- `src/runtime/commands_root_keys.rs` — multiplexer extended-key configuration.
- `src/runtime/manager.rs`, `src/runtime/stub_manager.rs`, `src/runtime/mod.rs` —
  runtime boundary plumbing.
- `src/pty_encoding.rs` — Enter chord encoding.
- `src/app_input/pty_passthrough.rs` — classifies the Enter write.
- `vendor/iocraft/src/terminal.rs` — host-terminal keyboard enhancement flags.
- `project-plans/issue627-plan.md` — this plan.

## Scope ledger

| Entry | Reason | Status |
|---|---|---|
| Extract `pty_encoding`'s test module into `src/pty_encoding_tests.rs` | The file was already above the recommended limit and the slice adds to it | Accepted — same-file tests only |
| Extract the terminal-model event sink into `src/runtime/attach_listener.rs` with its clipboard tests | `attach.rs` crossed the 1000-line hard limit; the sink is exactly the code these slices changed | Accepted |
| The unreachable `passthrough_enter` encoder flag is removed | It had no production caller: every call site passed `false`. Its only unique coverage is preserved by a new legacy `Alt+Enter` test | Accepted |
| `vendor/iocraft` keyboard-enhancement flags | A6/A7 cannot be met without it, and the vendored copy already carries jefe-specific input-policy changes | Accepted — one flag added, gated on the terminal advertising support |
| `domain::keymap::pty_bytes_for_chord` doc updated | It named the removed `passthrough_enter` parameter | Accepted — one stale doc line |

## Review counters

- Open Code Review before PR: 0 of 2 used.
- Open Code Review after PR: 0 of 2 used.
- Subagent design/code review cycles: 1 of 2 used.
- CI OpenCodeReview passes: 2, both triaged (fixed, declined with reasoning, or
  deferred with a recorded follow-up).

## Deferred findings

- `src/runtime/commands_tests.rs` — 883 lines of tests — is not declared by any
  module and is therefore never compiled or run. Wiring it in is a separate
  change with its own risk of surfacing long-dormant failures.
- `domain::keymap::pty_bytes_for_chord` is a second implementation of the PTY
  key encoding with no production caller. It now disagrees with `pty_encoding`
  for `Ctrl+Enter`. Deduplicating or deleting it is a separate change.
- `harness/v1/pty.rs` documents that it forwards kitty keyboard-protocol replies
  but builds its terminal model with the protocol disabled, so that part of the
  claim is unreachable. The harness hosts jefe itself, which does not negotiate
  the protocol, so nothing observable depends on it.
- The residual newline case: if the child's event loop stalls longer than the
  guard interval, two separated writes can still be read together. The complete
  remedy is for the child to distinguish pasted text by provenance rather than
  by arrival timing.
- `RuntimeListener` still forwards OSC 52 clipboard stores synchronously from
  inside the terminal parser, which holds the model lock, and the host boundary
  opens `/dev/tty`. That is the same liveness hazard this change fixes for query
  replies, but it predates the change and moving it needs its own care around
  clipboard ordering and coverage.
- The Enter separation is a bounded blocking wait on the caller. It is only ever
  the remainder of the interval since jefe's own last write, so it is zero at
  human typing speed, but a dedicated pacing thread would remove even that. It
  is a new subsystem with its own ordering guarantees and is a stated non-goal
  here.

## Verification evidence

- `cargo xtask ci` on the candidate head: fmt, check-clippy-allows,
  check-source-size, check-architecture, check-multiplexer-surface, lint,
  complexity, coverage and build all pass.
- `cargo test --workspace --all-features --locked` on the candidate head: all 81
  test targets pass.
- `tests/jsp_host_socket.rs::production_host_generates_unique_credentials_delivers_and_revokes`
  failed in two runs that overlapped with other heavy work on the machine, and
  passed on an idle machine on both this branch and `main`. The test drives 100
  rapid requests at a single-threaded unix-socket worker and its own source
  documents the accept-backlog contention that produces exactly this
  "connection reset by peer". It shares no code with this change, which touches
  only the PTY input path. Treated as a pre-existing load-sensitive flake, not
  as a regression.
- Transport behavior measured directly against tmux 3.7b with an isolated
  configuration, in jefe's own attach topology; results recorded in the
  measured-behavior table above.
