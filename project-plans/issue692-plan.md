# Issue #692 — `Ctrl+Enter` nudges never reach the agent when jefe runs on Windows

> `Ctrl+Enter` is llxprt's "nudge"/steering chord. It works for llxprt launched
> directly in PowerShell, and it works for llxprt hosted by jefe on macOS and
> Linux. Only the Windows-hosted-by-jefe combination loses it. That shape — one
> platform, one transport — points at the one hop the platforms do not share:
> the byte encoding jefe writes into the multiplexer client's PTY.

## Topology

A keystroke reaches a hosted agent through three hops:

| Hop | Windows | Unix |
|---|---|---|
| 1. host terminal → jefe | Windows console API via crossterm 0.28 | escape sequences via crossterm 0.28 |
| 2. jefe → multiplexer attach client | bytes written to a ConPTY (`portable_pty`) | bytes written to a Unix PTY |
| 3. multiplexer → pane child | psmux writes a **console input record** into the pane's ConPTY | tmux writes **escape-sequence bytes** into the pane's PTY |

Hop 3 is where the two platforms genuinely diverge, and it is why an encoding
chosen for tmux does not automatically survive on psmux.

## Measured behavior of the transport

Recorded on Windows against the supported multiplexer (`psmux 3.3.7`, tmux
3.3.7 fork) in jefe's own topology — an outer ConPTY running
`psmux attach-session`, with the probe target in the pane, inside a throwaway
`-L` namespace. Reproduction scripts are throwaway probes. Two independent
measurements were taken for each candidate encoding: what the *attach client*
recognises (read back through a `bind-key -n` that stores the key name in a
server option), and what the *pane child* actually receives (a
`[Console]::ReadKey($true)` logger reporting key, modifiers and char code).

| Bytes jefe writes to the client PTY | Key the client recognises | What the pane child receives |
|---|---|---|
| `CR` (`\r`) | `Enter` | `key=Enter mods=None char=13` |
| `LF` (`\n`) | **`C-Enter`** | **`key=Enter mods=Control char=10`** |
| `CSI 13 ; 5 u` — *what jefe sends today* | **none — silently discarded** | **nothing at all** |
| `CSI 27 ; 5 ; 13 ~` (modifyOtherKeys form) | none — silently discarded | nothing at all |
| win32-input-mode (`CSI 13;28;10;1;8;1_` + key-up) | `C-Enter` | `key=Enter mods=Control char=10` |

Supporting observations:

| Question | Measured answer |
|---|---|
| Does `set-option -s extended-keys on` (issue #627) change any row above? | **No.** The table is identical with the option `on` and `off`. On Windows the pane child is fed a console input record, not an escape sequence, so the option that governs tmux's escape-sequence downgrade has nothing to act on |
| Does the attach client ask for a richer encoding? | Yes — it emits `CSI ? 9001 h` (win32-input-mode) on attach, which jefe's embedded terminal currently ignores |
| What does llxprt see for `Ctrl+Enter` in a straight PowerShell? | `VK_RETURN` with the Ctrl modifier and `uChar = 0x0A` — byte-for-byte the record the `LF` row produces |
| Does jefe already emit `LF` for something else? | Yes: `Ctrl+J` has always encoded to `0x0A`. On Windows `Ctrl+J` therefore already arrives as `Ctrl+Enter`, before and after this change |

The third row is the decisive one: `LF` does not approximate the native
behavior, it reproduces it exactly. The working case (llxprt in PowerShell) and
the fixed case (llxprt under jefe) deliver the identical console record, which
is why the agent cannot tell them apart.

### Client registration is not client readiness

CI reported the transport test failing on the exact bytes the table above says
work (`[10]`, observed as `nothing`). Investigation ruled out the encoding and
found two facts worth recording, because both would mislead the next reader.

First, `psmux -V` is not a version. The binary CI installs (the pinned v3.3.7
release, build `05cc5d4`, 2026-07-20) and the locally built binary the table was
measured against (`cb098c0`, 2026-08-03) both print `psmux 3.3.7`, and they do
not behave identically. Any future transport measurement should record the build
hash, not the version string.

Second, the real fault was in the test, not the product. `list-clients` reports
a client as soon as it registers with the server, which is earlier than the
moment its terminal begins delivering keystrokes; bytes written into that window
are discarded. The release build leaves that window open long enough to lose the
chord whenever the machine is loaded, which is why the failure appeared only in
CI and only in the full parallel suite:

| psmux build | test alone | full parallel suite |
|---|---|---|
| `cb098c0` (local) | passes | passes |
| `05cc5d4` (CI release) | passes | **loses the chord** |

The test now writes a probe `CR` repeatedly until it is *observed*, which is the
only evidence the path under test is live, then clears the record and waits for
it to stay clear so an in-flight probe cannot be misread as the chord's result.
Re-running with the encoder reverted to `CSI 13 ; 5 u` still fails against the
release build, so the gate did not make the assertion vacuous — silence is now
attributable to the encoding rather than to startup timing.

This is a test-fidelity fix, not a product concession: in production the chord
is sent in response to a keypress in a session the user is already attached to,
so the startup window does not exist.

## Root cause

| # | Defect | Location | Effect |
|---|---|---|---|
| 1 | `Ctrl+Enter` is encoded as `CSI 13 ; 5 u` on every platform | `src/pty_encoding.rs` `enter_bytes` | psmux's input parser has no CSI-u branch, so on Windows the sequence is consumed and dropped. The chord is not downgraded, not mis-delivered — it vanishes between jefe and the pane, and the agent's nudge binding can never fire |

Issue #627 chose CSI-u correctly *for tmux*, where it is parsed as a modified
Enter and re-encoded per-pane. The defect is that the choice was made
unconditionally for a transport that was never measured.

## Why `LF` rather than win32-input-mode

Both encodings were measured working end-to-end. `LF` is chosen because:

- it is one byte and needs no handshake, whereas win32-input-mode is only
  legitimate once jefe honours the client's `CSI ? 9001 h` request — a
  negotiation jefe's embedded terminal does not implement, and implementing it
  would change how *every* key is written, not just this one;
- it produces exactly the console record the native path produces, so the fix
  is verifiable against a known-good reference rather than against a spec;
- the ambiguity it reintroduces (`Ctrl+J` ≡ `Ctrl+Enter`) already exists on
  Windows today via the `Ctrl+J` path and is not made worse. On Unix, where the
  ambiguity was the actual complaint in #627, nothing changes.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Targets | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | User presses `Ctrl+Enter` with the agent terminal focused | `KeyCode::Enter` + `CONTROL` | Windows | jefe writes `LF`; the attach client recognises `C-Enter`; the pane child receives `Enter` with the Control modifier and char `0x0A` | Chord silently absent at the pane, as today | None | Matches what the child receives from a native Windows console, so no agent needs to change | Encoder unit test, plus a real-psmux integration test that reads the chord back at the pane |
| A2 | Same | `KeyCode::Enter` + `CONTROL` | macOS / Linux | `CSI 13 ; 5 u`, byte-for-byte unchanged from #627 | n/a | None | Unchanged | Encoder unit test |
| A3 | Same | `CONTROL` combined with `ALT` | Windows | `ESC` + `LF` — the Alt prefix jefe applies to every other unencoded chord | n/a | None | Consistent with `Alt+Enter` | Encoder unit test |
| A4 | Same | `CONTROL` combined with `ALT` | macOS / Linux | `CSI 13 ; 7 u`, unchanged | n/a | None | Unchanged | Encoder unit test |
| A5 | Same, plain `Enter` | `Enter`, no modifiers | All platforms | `CR`, unchanged | n/a | None | Unchanged | Encoder unit test |
| A6 | Same, `Shift+Enter` / `Alt+Enter` / `Shift+Alt+Enter` | `Enter` + those modifiers | All platforms | Byte-for-byte unchanged, so the issue #1 contract holds | n/a | None | Unchanged | Encoder unit test |
| A7 | Every other key — characters, control chars, navigation, function keys, mouse | Any | All platforms | Byte-for-byte unchanged | n/a | None | Unchanged | Existing whole-family encoder tests |
| A8 | `Ctrl+J` | `KeyCode::Char('j')` + `CONTROL` | All platforms | `0x0A`, unchanged | n/a | None | On Windows it continues to reach the pane as `Ctrl+Enter`, exactly as before this change | Encoder unit test |
| A9 | Windows CI / developer machine without psmux installed | Integration test run | Windows | The real-transport test skips cleanly | With `JEFE_REQUIRE_PSMUX=1` the same condition is a hard failure naming the missing binary | None | Matches the existing conformance-test convention | Test gating asserted by construction |

## Non-goals

- Implementing the win32-input-mode handshake (`CSI ? 9001 h`) in jefe's
  embedded terminal. It is the more expressive encoding and it was measured
  working, but it changes the encoding of every key and is a subsystem of its
  own. Recorded as a follow-up, not folded in here.
- Disambiguating `Ctrl+J` from `Ctrl+Enter` on Windows. That collision predates
  this issue, is not what #692 reports, and cannot be resolved without the
  handshake above.
- Implementing the kitty keyboard protocol, which the multiplexer ignores on
  both platforms (#627 non-goal, still true).
- Changing `set-option -s extended-keys on`. It was measured irrelevant on
  Windows but it is load-bearing on tmux, so it stays exactly as #627 left it.
- Changing the `Shift+Enter` backslash-`CR` compatibility form (issue #1).
- Changing hop 1. crossterm 0.28 already delivers `KeyCode::Enter` +
  `CONTROL` on Windows; that hop was verified healthy and is untouched.

## Slices

1. **RED — encoder.** Extend `src/pty_encoding_tests.rs` with the
   platform-conditional expectation for `Ctrl+Enter` and `Ctrl+Alt+Enter`
   (A1–A4), leaving A5–A8 asserting the unchanged encodings. Fails on Windows
   against today's unconditional CSI-u.
2. **RED — real transport.** `src/pty_encoding_transport_tests.rs`, a
   Windows-gated test that owns a throwaway `-L` namespace (modelled on
   `src/runtime/multiplexer_conformance_io.rs`), attaches a real psmux client
   through `portable_pty`, writes the bytes jefe's encoder produces for
   `Ctrl+Enter`, and asserts the client recognises the chord. Skips without
   psmux unless `JEFE_REQUIRE_PSMUX=1` (A1, A9). Fails today because nothing
   arrives.

   It sits beside the encoder in the binary crate rather than under
   `src/runtime/`, because the subject under test is `key_to_bytes` itself and
   that lives in the binary crate — a copy of the expected bytes would test
   nothing, since the whole defect was jefe confidently emitting bytes nobody
   consumed. The cost is that the attach command is rebuilt from the plan's own
   `executable()` and `base_args()` instead of calling the private production
   builder. The isolation flags come from the plan itself, and the scrub list is
   restated to match `scrub_inherited_multiplexer_env` variable for variable, so
   the rebuild can be checked against the original; what is asserted is the byte
   payload, not the argv. Unifying the two lists behind one exported helper is a
   follow-up, because the production builder is not reachable from this crate.
3. **GREEN.** Make the `CONTROL` branch of `enter_bytes` platform-conditional:
   `cfg!(windows)` yields `LF` with Alt-prefixing left to the caller; every
   other platform keeps the CSI-u form. Document the measured reason inline.
4. **Verify.** `cargo fmt --all --check`, clippy gates, build, full test run.
