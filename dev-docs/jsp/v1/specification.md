# JSP/1 — Jefe Stream Protocol, Version 1

This is the normative specification for JSP/1, the Jefe Stream Protocol. It
freezes the external semantic and wire contract used by LLxprt Code (producer),
LLxprt Luther (broker), and Jefe (observer). J1 (this slice) defines the
snapshot document and the snapshot fixture corpus. Event, stream, heartbeat,
and replay semantics are described normatively here but are implemented in J2.

The canonical fixture corpus lives under `dev-docs/jsp/v1/fixtures/` and is
language-neutral: external implementations must consume the exact same corpus
and produce the same typed results as the Jefe reference oracle.

## 1. Envelope

Every JSP/1 document is a closed JSON object with exactly these top-level
fields for a snapshot:

| Field                   | Type    | Required | Notes                                              |
|-------------------------|---------|----------|----------------------------------------------------|
| `schema`                | integer | yes      | Must be exactly `1`. No fallback.                  |
| `kind`                  | string  | yes      | Must be `"snapshot"`.                              |
| `agent_id`              | string  | yes      | Opaque safe-ASCII ID, 1–128 bytes.                 |
| `lifecycle_generation`  | integer | yes      | Positive (`>= 1`).                                 |
| `source_epoch`          | string  | yes      | Opaque safe-ASCII stream identity, 1–128 bytes.    |
| `source_sequence`       | integer | yes      | Non-negative ordering sequence.                    |
| `cursor`                | integer | yes      | Non-negative; reflects all effects through cursor. |
| `bridge_observed_ms`    | integer | yes      | Non-negative UTC epoch milliseconds.               |
| `native_session`        | object  | yes      | See §3.                                            |
| `process_binding`       | field   | yes      | See §4.                                            |
| `native_activity`       | field   | yes      | See §5.                                            |
| `current_wait`          | field   | yes      | See §6.                                            |
| `current_turn`          | field   | yes      | See §7.                                            |
| `todos`                 | field   | yes      | See §8.                                            |
| `last_displayed_assistant_message` | field | yes | See §9.                                  |
| `last_created_tool_call`| field   | yes      | See §10.                                           |
| `source_terminal_state` | field   | yes      | See §11.                                           |
| `source_error_state`    | field   | yes      | See §12.                                           |

Unknown fields, duplicate fields, wrong types, trailing data, non-integer
numeric values, and non-object top-level values all fail with `JSP-E001`. This
applies at every level of the document, including inside a field state's
`value` and `last_value` payloads: a member that is not part of the closed
payload shape is rejected rather than ignored, and the same key sent twice is
rejected rather than resolved last-wins. There is no legacy or unknown-version
fallback.

### 1.1 Field-state algebra

Each semantic field uses one of two closed forms:

1. The bare string `"unsupported"` — the producer does not support this field.
2. A supported state object:
   ```json
   {
     "provenance": "authoritative" | "inferred",
     "availability": "known" | "unknown" | "degraded",
     "value": <field-specific>,
     "last_value": <field-specific>,
     "as_of_ms": <integer>,
     "diagnostic_code": "<string>"
   }
   ```

- `known` requires `value` and forbids `last_value`, `as_of_ms`,
  `diagnostic_code`.
- `unknown` forbids `value`, `last_value`, `as_of_ms`, `diagnostic_code`.
- `degraded` requires `last_value`, `as_of_ms`, `diagnostic_code`, and forbids
  `value`.

`stale` is **not** a producer field-state value. It is a local
observation-health overlay owned by the Jefe transport layer. A producer that
emits `stale` fails with `JSP-E005`.

`known` with `value: null` is valid for optional-entity fields
(`current_wait`, `source_terminal_state`) and means "known to be absent / not
applicable". This is distinct from `unknown` (not yet observed). For every
other field, `null` is not a valid known value: use `unsupported` or `unknown`.

`degraded` is not valid for the optional-entity fields `current_wait` and
`source_terminal_state`, because a wait or terminal state is either explicitly
observed or not observed. Supplying it fails with `JSP-E005`.

## 2. Identity and ordering

The live observation key is `(agent_id, lifecycle_generation, source_epoch)`.
Repository, path, agent kind, PID, display name, and native session metadata
are descriptive and never participate in the key.

- `lifecycle_generation` must be positive (`>= 1`). Zero fails with `JSP-E004`.
- Source epoch is producer/broker stream identity bound by registration to one
  agent and generation. A producer stream can restart during one process
  generation; a Jefe relaunch invalidates the generation and requires a new
  source epoch.
- Ordering authority is `(source_epoch, source_sequence)`. Snapshot cursor `C`
  reflects all effects through `C`. Heartbeats carry `C` and do not consume a
  sequence. Events consume exactly `C+1`. (J2 implements the stream state
  machine.)

## 3. Native session

```json
{
  "repository": "<string>",
  "path": "<string>",
  "agent_kind": "<string>",
  "pid": <integer>,
  "display_name": "<string>"
}
```

`agent_kind` is an opaque bounded label. JSP/1 does not close its inventory
because the producer set is expected to grow without a protocol version change.

All fields are descriptive metadata. They never collapse the observation key.

## 4. Process binding

The bare string `"unsupported"` or a supported state whose known value is:

```json
{ "pid": <integer>, "started_at_ms": <integer> }
```

Both members are required when the value is present. Process liveness remains
Jefe-runtime-owned. A producer reports typed process and lifecycle binding
metadata, not whether the process is alive or dead (decision 4).

## 5. Native activity

```json
{
  "provenance": "authoritative",
  "availability": "known",
  "value": { "state": "idle" | "thinking" | "acting" }
}
```

The state inventory is closed. Any other value fails with `JSP-E005`.

Native activity is source-owned. Observation health is Jefe-transport-owned.
The three axes (process liveness, observation health, native activity) remain
orthogonal.

## 6. Current wait

```json
{
  "provenance": "authoritative",
  "availability": "known",
  "value": null
}
```

`known` with `value: null` means "not waiting". `known` with a value object
means "waiting with an explicit unresolved reason":

```json
"value": { "reason": "permission" | "question" | "elicitation" | "choice" | "user_input" | "other" }
```

Silence and elapsed time never create waiting (decision 7). An explicit wait
object is required. `reason` is the only member of the wait object; any other
member fails with `JSP-E001`.

## 7. Current turn

```json
{
  "provenance": "authoritative",
  "availability": "known",
  "value": { "elapsed_ms": <integer> }
}
```

Turn runtime carries an elapsed-millisecond anchor. Jefe later advances it from
local monotonic receipt and never subtracts clocks across processes (decision
9).

## 8. Todos

```json
{
  "provenance": "authoritative",
  "availability": "known",
  "value": {
    "revision": <integer>,
    "items": [
      { "text": "<string>", "completed": <bool> }
    ]
  }
}
```

Todos are full replacement with strictly increasing revision; patches are
invalid (decision 8). `revision` must be positive (`>= 1`); zero fails with
`JSP-E005`. `known` with an empty `items` array preserves the distinction
between known-empty, unsupported, and unknown.

## 9. Last displayed assistant message

```json
{
  "provenance": "inferred",
  "availability": "known",
  "value": { "content": "<string>", "committed_ms": <integer> }
}
```

Last assistant message changes only at a native user-visible display or commit
boundary (decision 7). Drafts, hidden content, thinking, raw transcripts, tool
arguments/output, and command bodies are absent from this contract.

## 10. Last created tool call

```json
{
  "provenance": "authoritative",
  "availability": "known",
  "value": { "label": "<string>", "phase": "proposed" | "awaiting_approval" | "scheduled" | "executing" | "succeeded" | "failed" | "cancelled" }
}
```

Last tool means the most recently created native tool item (decision 8).

## 11. Source terminal state

```json
{
  "provenance": "authoritative",
  "availability": "known",
  "value": null
}
```

`known` with `value: null` means no terminal error state. A non-null value uses
the same closed payload as §12:

```json
"value": { "summary": <bounded string>, "code": <bounded string> }
```

## 12. Source error state

```json
{
  "provenance": "authoritative",
  "availability": "known",
  "value": { "summary": "<string>", "code": "<string>" }
}
```

Source/native timestamps are bounded UTC epoch milliseconds and diagnostic
only.

## 13. Bounds

All bounds are inclusive. Limit-plus-one input fails with `JSP-E002` before a
contract value is returned.

| Bound                              | Value      |
|------------------------------------|------------|
| Snapshot document                  | 256 KiB    |
| IDs (agent_id, source_epoch)       | 128 bytes  |
| Todo list entries                  | 256        |
| Todo text                          | 2 KiB      |
| Displayed assistant content        | 16 KiB     |
| Source diagnostic summary          | 2 KiB      |
| Tool label                         | 256 bytes  |
| Repository reference               | 256 bytes  |
| Path reference                     | 4 KiB      |
| Agent-kind label                   | 64 bytes   |
| Display name                       | 256 bytes  |
| Diagnostic code                    | 128 bytes  |
| Source error code                  | 128 bytes  |

Opaque identifiers (`agent_id`, `source_epoch`) additionally accept only ASCII
alphanumerics and `-`, `_`, `.`, and must be non-empty. Any other byte fails
with `JSP-E001`.

## 14. Error codes

| Code      | Meaning                                              |
|-----------|------------------------------------------------------|
| `JSP-E001`| Closed JSON / syntax / shape violation               |
| `JSP-E002`| Bound exceeded                                       |
| `JSP-E003`| Unsupported version or kind                          |
| `JSP-E004`| Identity / binding violation                         |
| `JSP-E005`| Field-state violation                                |
| `JSP-E006`| Snapshot semantic invariant violation                |

Parsing returns a typed `Result` and performs no logging or I/O (decision 14).
Diagnostics contain stable code/path/location and never echo input values
(decision 12).

## 15. Forbidden fields

Publisher and observer credentials are out-of-band HTTP authorization
material, role-separated, loopback-only for JSP/1, and forbidden from protocol
documents (decision 12). Closed parsing rejects credential and control fields
at ingress. The schema has no control operation.

Forbidden fields include (non-exhaustive): `publisher_token`, `observer_token`,
`raw_transcript`, `draft`, `control`, and any field not listed in §1.

## 16. Transport semantics (normative, J2-implemented)

- A fresh stream begins with an atomic snapshot at `C`.
- A resume request after `N` begins with the reconstructed snapshot at `N`
  followed by `N+1` events.
- If epoch or replay is unavailable, the broker returns coded HTTP 409
  `resync_required` before opening SSE; the client then makes a fresh request.
- A stream never mixes partial replay with fallback snapshot data (decision
  11).

These semantics are implemented in J2. J1 defines only the snapshot document
and corpus.

## 17. Fixture corpus

The fixture corpus lives under `dev-docs/jsp/v1/fixtures/` and is enumerated by
`manifest.json`. Each fixture declares an expected result (`ok` or `error`
with an `error_code`). External implementations must consume the corpus and
produce the same typed results.

| Fixture                          | Expected | Notes                                    |
|----------------------------------|----------|------------------------------------------|
| `snapshot_full.json`             | ok       | Canonical full snapshot (S1)             |
| `snapshot_identity_distinct.json`| ok       | Same worktree, distinct key (S3)         |
| `snapshot_field_states.json`     | ok       | Exhaustive field-state algebra (S4)      |
| `snapshot_bounds.json`           | ok       | At-limit ID bound (S2)                   |
| `snapshot_closed_grammar.json`   | error    | Unknown field, JSP-E001 (S2)             |
| `snapshot_forbidden_fields.json` | error    | Credential/control, JSP-E001 (S5/S6)     |
| `snapshot_semantic_failure.json` | error    | Producer stale state, JSP-E005 (S4)      |
