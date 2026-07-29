# JSP/1 producer adapter compliance contract

A JSP/1 producer implementation proves compliance by emitting a
language-neutral **producer trace** that the compliance runner validates. The
producer never runs a model or terminal during the trace; it emits a fixed,
deterministic sequence of JSP/1 documents and adapter facts.

## Adapter protocol

The compliance runner does **not** perform live networking or launch a real
agent process. It consumes a machine-readable trace emitted by the producer
implementation under test. External implementations emit the trace (for example
by running their adapter driver against a deterministic fake clock and a
bounded blocked sink) and the runner validates it independently.

The trace is a single JSON document (`producer-trace.json` shape) containing:

- `schema`: `1`
- `kind`: `producer-trace`
- `adapter_version`: opaque producer label
- `challenge_nonce`: runner-supplied nonce that binds the trace to the
  runner's challenge. An arbitrary `adapter_version`, absent chosen marker,
  fabricated queue arithmetic, or uncaptured gap cannot pass because the
  observed result must incorporate the nonce.
- `facts`: an ordered array of closed challenge/response records:
  - `clock_set` sets the deterministic fake clock; the following captured
    document must derive its `bridge_observed_ms` from that exact value.
  - `document` captures the exact nested JSP/1 bytes. The runner routes those
    bytes through the authoritative parser, including its 256 KiB ingress bound.
  - `redaction_challenge` names a captured document index and a forbidden test
    marker; the exact captured bytes must not contain the marker.
  - `bound_challenge` carries an at-limit and limit-plus-one event. The same
    parser must accept 16,384 content bytes and reject 16,385 with `JSP-E002`.
  - `nonblocking_challenge` records a blocked sink, bounded queue capacity,
    attempted/accepted counts, and elapsed/deadline milliseconds. Acceptance
    must be derived from the capacity and complete before the deadline.
  - `gap_challenge` records the last emitted sequence, exact dropped range, and
    next emitted sequence. Every endpoint is derived from the captured stream.

Fact kinds and fields are closed. Unknown, malformed, irrelevant, or
assertion-only fields fail before semantic evaluation; booleans claiming that
an operation was redacted or nonblocking are not evidence.

## Producer profile invariants

The producer trace must prove:

1. **Launch identity and epoch** — the first document is a snapshot with a
   valid `(agent_id, lifecycle_generation, source_epoch)` key.
2. **Monotonic ordering** — event `source_sequence` increases by exactly one
   within an epoch.
3. **Explicit transitions** — every activity, wait, turn, todo, message, and
   tool transition is an explicit event; silence never creates state.
4. **Complete todo replacement with revision** — `todos.replaced` carries a
   positive revision.
5. **Displayed message boundary** — `assistant_message.displayed` carries a
   commit timestamp; no draft appears.
6. **Tool-call creation and phase** — `tool_call.created` and
   `tool_call.phase_changed` carry a closed phase.
7. **Source-side redaction and payload limits** — captured bytes answer a
   marker challenge, and exact at-limit/limit-plus-one documents exercise the
   parser's inclusive bound.
8. **Nonblocking publication** — a bounded blocked-sink measurement derives
   accepted count from queue capacity and completes before its deadline.
9. **Nonblocking gap signaling** — the exact dropped interval is derived from
   the last and next emitted sequence rather than a producer-claimed gap flag.

## Non-negotiable rules

- The producer must never emit forbidden members (`publisher_token`,
  `observer_token`, `raw_transcript`, `draft`, `control`). The closed parser
  rejects them.
- The producer must never assert `stale` (it is a transport-owned overlay).
- The producer must never report observation health (it is observer-owned).

## Runner-owned challenge execution (Slice B)

The compliance runner may invoke an adapter command (`--adapter COMMAND`) or
use the built-in reference adapter (`--reference-adapter`) to execute the
challenge rather than replaying a self-attested trace. The runner supplies:

- A **nonce** (`--nonce N`) that the adapter's output must incorporate.
- Closed redaction and S9 draft source objects, each with a runner-owned source
  handle, source text, and marker. The source contains the marker; captured JSP
  documents and the final projection must not.
- A fake clock sequence that every captured document must consume exactly.
- The complete launch identity triple and process binding the adapter must
  observe.
- Bounded sink capacity/deadline plus unique runner operation handles.
- An exact drop interval and operation handle, followed by a captured next
  publication at `dropped_end + 1`.
- The expected checked adapter protocol version.

Producer/server qualification never accepts `--input` or a default static
trace. The runner writes one closed JSON challenge to the selected subprocess,
bounds input/output/deadline, and validates its response against that exact
challenge. A replayed trace, fabricated `adapter_version`, arbitrary absent
marker, invented queue arithmetic, or uncaptured gap publication fails. Every
captured producer document is parsed and reduced by `ReferenceReducer`; the
trace must prove all 11 event variants, legal transitions, exact launch
identity/epoch/generation, process binding, and draft exclusion.
