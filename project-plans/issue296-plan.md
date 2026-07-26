# Issue #296 — [Windows] LLxprt mouse input is lost and Page keys arrive as arrows

Branch: `issue296`
Plan owner: LLxprt Code
Workflow: `dev-docs/workflow/ISSUE-DELIVERY.md`

## Problem summary

On native Windows (Microsoft Windows App, macOS client), LLxprt Code inside
Jefe loses mouse input and Page keys arrive as arrows:

- Mouse wheel does nothing.
- Clicking/dragging LLxprt's visible scrollbar starts Jefe text selection.
- Page Up / Page Down (`fn`+Up / `fn`+Down on macOS) behave as plain arrows.

Related: #245 / PR #249 (original ownership fix), #253 (native Windows epic),
#260 / PR #284 (native Windows attachment validation), #262 / PR #291
(persistent Windows agent lifecycle parity).

## Codebase findings (evidence)

1. **Encoder is correct.** `src/pty_encoding.rs::nav_key_bytes` maps
   `PageUp`→`\x1b[5~`, `PageDown`→`\x1b[6~`, `Up`→`\x1b[A`, `Down`→`\x1b[B`.
   Tests already lock this (`modified_edit_keys_use_xterm_sequences`).
   ⇒ Page-key loss is **upstream (client translation)** or
   **downstream (transport)**, not in `key_to_bytes`.

2. **Mouse-reporting mode is derived from the live PTY stream.**
   `src/runtime/attach.rs::AttachedViewer::mouse_reporting_active()` reads the
   embedded `alacritty_terminal::Term` mode bits (MOUSE_MODE / SGR_MOUSE /
   UTF8_MOUSE). `spawn_command` always builds a **fresh** `Term` with cleared
   bits. Mode is only recovered if the child re-emits DEC private mouse mode
   sequences (`\x1b[?1000h`, `?1002h`, `?1006h`) *through the attach PTY stream*
   *after* attach.

3. **No post-attach mode-recovery exists.** Neither
   `TmuxRuntimeManager::attach` (`src/runtime/manager.rs`) nor
   `TmuxRuntimeManager::apply_attach_result` (`src/runtime/async_attach.rs`)
   nudges the child to re-advertise modes. If ConPTY (Windows 10 / older
   Windows 11) consumes the DEC private mode sequences before psmux/Jefe
   observe them, the Jefe `Term` stays in non-reporting fallback and the
   gesture machine routes left-drag/wheel to Jefe selection.

4. **Mouse routing depends on `mouse_reporting_active()`.**
   `src/mouse_routing.rs::terminal_target_info` reads the flag once under a
   single lock; `route_terminal_gesture` feeds it to the gesture state machine.
   When false, a non-kennel (LLxprt) child's left gesture goes to Jefe
   selection and its wheel is Noop'd to detail scroll.

5. **psmux-smoke harness exists.** `tests/fixtures/psmux_smoke_fixture.rs`
   already emits `\x1b[?1000h\x1b[?1006h\x1b[?2004h\x1b[?1049h` on startup and
   records received bytes via `PSMUX_BYTE_<hex>`. `tests/psmux_smoke.rs` has
   the `PsmuxNamespace` / `psmux-smoke` feature-gated pattern, gated by
   `windows_native` CI and `JEFE_REQUIRE_PSMUX`.

## Acceptance matrix

Each row identifies actor/launch path, input, expected behavior, and the
behavioral test that proves it.

| # | Actor / path | Input | Expected (observable) | Failure / diagnostic | Test / scenario |
|---|---|---|---|---|---|
| A1 | LLxprt agent (non-kennel), native Windows, after attach | Mouse wheel over terminal pane | Wheel forwards to PTY as SGR mouse bytes (scrolls LLxprt) | Wheel Noop'd to detail scroll when `mouse_reporting_active()` false | psmux-smoke: fixture advertises 1000/1002/1006 ⇒ Jefe `mouse_reporting_active()` true |
| A2 | LLxprt agent, native Windows | Unmodified left click + drag over terminal | Routes to PTY (`ForwardToPty`), LLxprt scrollbar works | Falls through to Jefe selection when reporting false | Unit: reporting-active LLxprt gesture ⇒ `ForwardToPty` (regression in `gesture_tests`) |
| A3 | LLxprt agent, native Windows | Shift + left drag | Jefe text selection (ownership contract from #249 preserved) | — | Existing `shift_mouse_events_are_not_forwarded_to_pty` + `gesture_tests` |
| A4 | Any agent | Physical/native PageUp / PageDown key | Reaches child as `CSI 5~`; PageDown as `CSI 6~` (not arrows) | Encoder bug (none found) | psmux-smoke: forward PageUp/PageDown ⇒ fixture records `5~`/`6~` bytes |
| A5 | macOS client → Windows App | `fn`+Up / `fn`+Down | Diagnosed & documented: client translates to arrow before Jefe; alternative key documented; no `fn` inference in Jefe | — | Dev-docs entry + encoder unit tests stay green |
| A6 | LLxprt agent, native Windows, attach with ConPTY consuming DEC private modes | Attach completes | Post-attach recovery re-establishes observed mouse-reporting mode (resize/refresh nudge or replay) so `mouse_reporting_active()` reflects child's true state | Stuck in non-reporting fallback | Unit: post-attach recovery hook sets mode-observation; psmux-smoke regression |
| A7 | Code Puppy (kennel) agent | Wheel over terminal | Jefe scrollback viewport owns wheel (#249 contract preserved) | — | `wheel_intercept_active_for_agent` + existing wheel-intercept tests |

## Non-goals

- **No `fn` inference in Jefe.** The macOS `fn`+Arrow → arrow translation
  happens upstream of Jefe; we will not add heuristics to reverse it.
- **No refactor of the gesture state machine or mouse router.** Scope is
  strictly mode-observation recovery + diagnostics + docs.
- **No cross-platform parsing-logic changes.** ConPTY-consumption mitigation
  stays Windows-aware / boundary-level.
- **No new process-management, timeout, cancellation, or cleanup subsystem.**
- **No dependency or workflow/agent-memory/quality-tool changes.**
- **No re-architecture of `AttachedViewer` lifecycle.**
- Renaming, cosmetic, or unrelated test relocation.

## Vertical slices

### Slice 1 — Diagnostic tracing (RED → GREEN)

**Acceptance rows:** informs A1, A4, A6 (no behavioral change; observability).
**Architecture owner:** runtime + app_input boundary.
**Allowed files:**
- `src/runtime/attach.rs` (tracing only, no behavior change)
- `src/runtime/manager.rs` (tracing only)
- `src/runtime/async_attach.rs` (tracing only)
- `src/app_input/pty_passthrough.rs` (tracing only)
- `src/pty_encoding.rs` (tracing only, if needed)

**Behavior:** Add `tracing` at the three ticket checkpoints:
1. mouse-mode bit toggles in the reader/advance path of `AttachedViewer`;
2. lifecycle boundaries that reset/affect mode (spawn, resize, alt-screen);
3. raw `KeyEvent` (code+modifiers) + resulting encoded bytes for nav/page keys,
   distinguishing tilde form (`CSI 5~`/`6~`) from letter form (`CSI A`/`B`).

**Tests:** tracing-only; no behavior change. A unit test may assert a helper
exists / is callable. Keep behind existing `tracing` levels.

**Stop conditions:** if adding tracing requires a new public abstraction or
cross-module plumbing beyond the boundary, stop.

### Slice 2 — psmux-smoke fixture + Windows-gated real-transport test

**Acceptance rows:** A1, A4, A6.
**Architecture owner:** tests + runtime attach.
**Allowed files:**
- `tests/fixtures/psmux_smoke_fixture.rs` (already emits `?1000h ?1006h`; add
  `?1002h`, keep recording `INPUT_HEX`/`PSMUX_BYTE` for `5~`/`6~` and SGR mouse)
- `tests/psmux_smoke.rs` (new feature-gated test through
  `AttachedViewer`/ConPTY/psmux path)
- `dev-docs/testing/psmux-smoke.md` (compatibility-matrix rows)

**Behavior:**
- Fixture advertises 1000/1002/1006 and records Page-key + SGR mouse bytes.
- New test asserts (a) after fixture advertises modes, Jefe's terminal model
  reports `mouse_reporting_active()` true; (b) forwarded PageUp/PageDown
  arrive at the child as `CSI 5~`/`CSI 6~` (not arrows); (c) SGR mouse input
  reaches the child intact.
- Reuse `PsmuxNamespace`, version gating, timeouts, RAII cleanup; honor
  `JEFE_REQUIRE_PSMUX`.

**Stop conditions:** if the test cannot drive input through the real
`AttachedViewer` without a new test-only public API surface beyond the existing
`spawn_with_plan` / `psmux-smoke` feature, stop and report.

### Slice 3 — Post-attach mouse-mode recovery + ownership regression

**Acceptance rows:** A1, A2, A6, A7.
**Architecture owner:** runtime attach (recovery) + selection tests.
**Allowed files:**
- `src/runtime/attach.rs` (recovery hook, scoped to mode observation)
- `src/runtime/manager.rs` (attach completion calls recovery)
- `src/runtime/async_attach.rs` (attach completion calls recovery)
- `src/mouse_routing_wheel_intercept_tests.rs` (regression assertion)
- `src/selection/gesture_tests.rs` (regression assertion: reporting-active
  LLxprt ⇒ `ForwardToPty`)
- `tests/runtime/terminal_focus_routing.rs` (if present, regression assertion)

**Behavior:** A recovery mechanism so a freshly spawned `AttachedViewer` does
not leave LLxprt stuck in non-reporting fallback — e.g., a post-attach
resize/refresh nudge that prompts the TUI to re-emit DECSET, so
`mouse_reporting_active()` reflects the child's true state. Scope strictly to
restoring correct mode observation; do not refactor the gesture machine or
router. If Phase 2 confirms ConPTY consumes the modes, add a Windows-aware
guard at the attach boundary and document it (do not change cross-platform
parsing).

**Tests:** RED test proving that with no recovery, `mouse_reporting_active()`
stays false after attach-without-re-emission; GREEN proving recovery flips it.
Plus regression assertions: reporting-active LLxprt ⇒ `ForwardToPty`; Shift+drag
⇒ Jefe selection; kennel wheel ⇒ Jefe scrollback.

**Stop conditions:** if the recovery requires a new process-management /
timeout / cancellation subsystem or an unplanned public abstraction, stop.

### Slice 4 — macOS `fn` Page-key documentation

**Acceptance rows:** A5.
**Architecture owner:** dev-docs.
**Allowed files:**
- `dev-docs/testing/psmux-smoke.md` (or adjacent Windows testing doc)

**Behavior:** Record the finding (client translates `fn`+Arrow to arrow before
Jefe; encoder stays correct for physical PageUp/PageDown) and a verified
alternative key combination. Explicitly no `fn`-inference logic in Jefe.

**Stop conditions:** none (documentation only).

## Scope ledger

Newly discovered work is recorded here with a disposition (Blocker-Fix /
In-scope-Fix / Reject / Defer). Starts empty; updated per slice.

| Item | Disposition | Notes |
|------|-------------|-------|
| `src/harness/v1/validate.rs:114` clippy `manual_is_multiple_of` | Blocker-Fix (gate-enabling prerequisite) | Pre-existing on `origin/main`; newer clippy (1.97) flags `input.len() % 4 != 0`. One-line mechanical fix to `!input.len().is_multiple_of(4)` required for the clippy gate to pass. |
| `src/bin/jefe-capture-shim.rs:163` clippy `duration_suboptimal_units` | Blocker-Fix (gate-enabling prerequisite) | Pre-existing on `origin/main`; clippy 1.97 flags `Duration::from_secs(3600)`. One-line fix to `Duration::from_hours(1)` (stable since Rust 1.95; project has no MSRV pin). Required for the clippy gate to pass. |

## Review counters (Open Code Review cap: 4 per effort)

- Pre-PR OCR runs: 0 / 2
- Post-PR OCR runs: 0 / 2

## Verification evidence

(Updated as slices complete.)

- `cargo fmt --all --check`: **PASS** (exit 0)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: **PASS** (exit 0)
- `cargo build --workspace --all-features --locked`: **PASS** (exit 0)
- `cargo test --workspace --all-features --locked`: **PASS** (exit 0, no failures)
- `windows_native` CI (psmux-smoke): pending — the new
  `psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys` test
  runs under the existing `windows_native` job with no new gating.
- coverage ≥ 30%: pending CI.

## Deferred findings / follow-ups

(None yet.)
