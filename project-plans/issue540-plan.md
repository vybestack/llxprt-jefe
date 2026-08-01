# Issue #540 — Versioned, behavioural psmux compatibility contract

Sub-issue of epic #539. Restores invariant **I2**, epic criteria **E1/E2**.

> jefe must never execute against a multiplexer binary whose behaviour it has not
> verified, and every semantic jefe depends on must be asserted against that binary
> rather than assumed from tmux.

## Grounding evidence gathered before planning

Measured on 2026-08-01 against psmux `3.3.7 (05cc5d4)` (installed, WinGet) and a local
build of `acoliver/psmux@1a8b6d5` (which carries the merged fixes for psmux#509/#510/#443):

| Probe | installed 05cc5d4 | fork 1a8b6d5 |
|---|---|---|
| `-L <ns> display-message -p '#{pid}'` | **`22440` for *every* namespace**, including one with no server at all | resolves within the namespace, but still changes per session: `9008` → `17784` → `3832` |
| `-L <ns> display-message -p '#{server_instance}'` | unsupported | **stable**: `883b25f5…` across three sessions; differs per namespace; errors cleanly when no server |
| server processes per `-L` namespace | 4 | 4 |

This is a live instance of exactly the class #540 exists to prevent. jefe pins
`SERVER_IDENTITY_FORMAT = "#{pid}|#{version}"` (`server_health_io.rs:13`) on the tmux
assumption that `#{pid}` identifies *the server for this socket*. On psmux there is one
server **process per session**, so `#{pid}` names whichever server answered. Adding a
session therefore looks like a server restart. Upstream's fix (psmux#509) deliberately
does **not** change `#{pid}`; it adds `#{server_instance}` as the stable namespace token.

Consequence in production today: jefe's persisted state claims three agents `running`
while only one session exists. The two phantoms are unreachable and un-relaunchable.

This is recorded here because it is the strongest available justification for V5 (a
mechanical check on format strings): no version gate would have caught it, and the
symptom surfaced three layers away from the cause.

## Non-goals (explicit)

- **Not bundling or vendoring psmux.** Maintainer decision D = REJECT. The refusal path
  is therefore the only path when the contract is unmet, which is why the diagnostic
  quality in V2/V3 is a deliverable and not a nicety.
- **Not a CI version matrix.** Decision C = REJECT (runner queue is the bottleneck). CI
  continues to qualify one pinned version; cross-version work happens locally via the
  `JEFE_PSMUX_BIN` override (decision A = ADOPT).
- **Not moving past psmux 3.3.7.** That evaluation moved to #547 (which owns
  multiplexer-binary isolation, deliverables 7–9).
- **Not per-agent `Replaced` evaluation.** #541 V5 owns "a `Replaced` server with a
  surviving host process must not mark that agent `ServerLost`". #540 supplies the
  *correct identity input*; #541 decides what to do with a genuine replacement.
- **Not the ownership anchor.** #542.
- **Not removing the launch-pipeline hard gates.** #544.

## Contract surface, as measured

Verbs issued from production `src/` (occurrence counts):
`set-option` 14, `send-keys` 10, `capture-pane` 10, `has-session` 6, `list-windows` 6,
`list-sessions` 5, `list-panes` 5, `display-message` 5, `new-session` 5, `kill-server` 5,
`attach-session` 4, `kill-session` 4, `unbind-key` 3, `select-window` 3,
`show-options` 1, `new-window` 1.

Format strings: `#{number}` 38, `#{session_name}` 11, `#{pane_pid}` 10, `#{window_name}` 9,
`#{pane_dead}` 7, `#{version}` 3, `#{pid}` 3, `#{window_index}` 3, `#{next_display_index}` 2,
`#{history_size}` 2, `#{pane_dead_signal}` 1, `#{pane_index}` 1.
(`#{pr_number}`, `#{issue_number}`, `#{n}`, `#{thread_idx}`, `#{dup_num}` are jefe's own
template syntax, not multiplexer formats — the V5 checker must not confuse the two.)

Known behavioural divergences already patched ad hoc, which V6 must fold into the contract:
`exit-empty off` (`server_health_io.rs:120`, `app_shell_liveness.rs:59`), F12 prefix
override (`action_projection.rs:647,651`), PageUp root-table unbind
(`action_projection.rs:151,733`), `remain-on-exit` (`psmux_driver.rs:292`).

V4's tmux-derived constant: `PROMPT_COMPACTION_THRESHOLD_BYTES = 10_000`
(`fresh_prompt.rs:87`), referenced by 12 assertions in `prompt_compaction_tests.rs` and
paired with `TMUX_PANE_COMMAND_LIMIT_BYTES`.

## Acceptance matrix

| ID | Criterion | Slice | Proof |
|---|---|---|---|
| V1 | Conformance suite passes every contract item against a qualified psmux | S2 | suite green in `windows_native` |
| V2 | Unqualified binary (wrong version; and a stub that answers `-V` correctly but violates one item) ⇒ refusal naming the failing item | S3 | stub-binary tests |
| V3 | No qualified psmux ⇒ failure at **startup**, before session-creation code is reachable | S3 | test asserts ordering |
| V4 | Pane-command byte limit **measured** against the live binary; threshold derived; tmux constant deleted | S4 | probe test + constant absent |
| V5 | Build fails if production code uses a verb/format not declared in the contract | S5 | `cargo xtask` check |
| V6 | Each retired workaround has a conformance assertion, or removal evidence | S6 | per-item assertion |
| V7 | Binary-provenance mismatch under a running jefe is detected | S7 | SHA256 manifest test |

## Bounded vertical slices

- **S1 — contract surface module.** One enumerated table: verb / format / option / limit,
  each with expected response shape. Pure data + types, no I/O. Includes
  `#{server_instance}` as a *capability-gated* item (present on psmux ≥ #509, absent on
  3.3.7 release).
- **S2 — conformance runner (V1).** Executes S1's surface against the resolved binary,
  one assertion per item. Boundary module; deterministic classification of results.
- **S3 — startup qualification + refusal (V2, V3).** Moves the gate from session creation
  to startup. Refusal names binary path, observed version, failing item, and remedy.
  Must run against a `JEFE_PSMUX_BIN` override (decision A) and still enforce.
- **S4 — measured pane-command budget (V4).** Probe the live limit; derive the threshold;
  delete `PROMPT_COMPACTION_THRESHOLD_BYTES`; re-express the 12 dependent assertions
  against the measured value.
- **S5 — mechanical surface check (V5).** `xtask` lint: every multiplexer verb/format in
  production code must appear in S1. Must distinguish jefe's own `#{…}` template syntax.
- **S6 — retire workarounds into the contract (V6).**
- **S7 — provenance (V7).** SHA256 manifest; detect the binary changing under a running jefe.
- **S8 — server identity migration.** Adopt `#{server_instance}` where the contract says
  it is available, keep `#{pid}` only as a documented-unstable fallback for pre-#509
  builds, and make "namespace has no server" a distinct observation from "identity
  changed". Directly fixes the phantom-agent failure above.

Order: S1 → S2 → S3 → S5 → S8 → S6 → S4 → S7.
S8 sits after S5 so the checker exists before the format string is swapped.

## Scope ledger

Epic #539 suspends the file/line budget. Expected paths:
`src/runtime/{contract*,server_health*.rs,commands.rs,multiplexer.rs,psmux_driver.rs,liveness.rs}`,
`src/app_init*.rs`, `src/…/fresh_prompt.rs`, `xtask/src/`, `tests/`.

## Open question for the maintainer (not blocking S1–S3)

S8 changes which format string jefe trusts for server identity. On a pre-#509 psmux
(including the currently installed 3.3.7 release) `#{server_instance}` does not exist, so
the stable identity is simply **unavailable**. Per #541's fail-closed direction, the
honest reading is "identity unknown" — which must not be treated as a restart. That
implies jefe on an old psmux stops attempting server-replacement detection rather than
continuing to guess from `#{pid}`. Flagging because it is a deliberate capability
reduction on unfixed binaries, and the alternative (keep guessing) is what produced the
phantom agents.
