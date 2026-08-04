# JSP/1 — Jefe Stream Protocol, Version 1

This is the normative specification for JSP/1, the Jefe Stream Protocol. It
freezes the external semantic and wire contract used by LLxprt Code (producer),
LLxprt Luther (broker), and Jefe (observer).

JSP/1 answers four questions about a running coding agent: what is it doing
now, how far along is it, is it blocked waiting for input and why, and is the
status source itself healthy. It is observation-only. It carries no control
operation, cannot answer a permission or question request, and never replaces
the agent's native terminal UI.

This document defines the complete protocol: the snapshot document, the event
inventory, heartbeats, stream semantics, and the language-neutral fixture
corpus that every implementation must satisfy.

The canonical fixture corpus lives under `dev-docs/jsp/v1/fixtures/` and is
language-neutral: external implementations must consume the exact same corpus
and produce the same typed results as the Jefe reference oracle.

### Schema byte-length keyword

The executable schemas use JSON Schema Draft 2020-12 and the custom annotation
`x-jsp-maxUtf8Bytes`. A conforming JSP validator **must** implement this keyword
for every annotated string by measuring the number of bytes in the string's
UTF-8 encoding. Draft 2020-12 `maxLength` counts Unicode code points and is not
a substitute. Inputs that satisfy a character-count limit but exceed the
annotated UTF-8 byte limit are invalid.

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
(`current_wait`, `current_turn`, `source_terminal_state`) and means "known to be
absent / not applicable". This is distinct from `unknown` (not yet observed).
For every other field, `null` is not a valid known value: use `unsupported` or
`unknown`.

`degraded` is not valid for the optional-entity fields `current_wait`,
`current_turn`, and `source_terminal_state`, because a wait, turn, or terminal
state is either explicitly observed or not observed. Supplying it fails with
`JSP-E005`.

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
  reflects all effects through `C`. Heartbeats carry no sequence and do not
  consume one. Events consume exactly `C+1`. (J2 implements the stream state
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
      { "text": "<string>", "state": "<string>" }
    ]
  }
}
```

Todos are full replacement with strictly increasing revision; patches are
invalid (decision 8). `revision` must be positive (`>= 1`); zero fails with
`JSP-E005`. `known` with an empty `items` array preserves the distinction
between known-empty, unsupported, and unknown.

`state` carries the producer's own task state so an observer never has to infer
which item is active. JSP/1 recognizes `pending`, `in_progress`, and
`completed`. Unlike `current_wait.reason`, `last_created_tool_call.phase`, and
`turn.ended.outcome`, this vocabulary is open: a label outside the recognized
set is accepted and read as "not completed and not active". It is never mapped
onto one of the recognized states, because a guess presented as state is the
defect this field exists to remove. `state` is required; omitting it fails with
`JSP-E001`.

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
| Todo task state                    | 64 bytes   |
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

### 15.1 Embedded observer-host profile

A Jefe-launched local producer may receive an owner-only bootstrap file whose
closed fields are `schema`, `protocol`, `endpoint`, `registration_id`,
`publisher_credential`, `agent_id`, and `lifecycle_generation`. The endpoint is
IPv4 loopback HTTP and exposes `POST /jsp/1/register`, `/jsp/1/publish`, and
`/jsp/1/heartbeat`. Every request supplies the credential as `Authorization:
Bearer ...` and the matching opaque registration ID as `Jsp-Registration-Id`.
Jefe binds both values to the authorized agent and positive generation before
child creation. The route state is explicitly `reserved` until one successful
`register`; registration is accepted exactly once. `publish` and `heartbeat`
are rejected before registration. Missing, unknown, or mismatched authority
never mutates current observation state. A registered producer renews a 15-second
lease through accepted snapshots, events, or heartbeats; lease expiry changes
observer health to `stale` without changing producer-owned native state.

## 16. Stream semantics

JSP/1 answers "what is this agent doing now". It is a live status protocol, not
a history or visualization protocol. The broker keeps exactly one current
snapshot per source epoch and streams live events. It stores no event history.

- A stream always begins with a `snapshot` document. This removes the race in
  which a separately fetched snapshot is already stale before the stream opens.
  It is not a cursor negotiation: the client cannot request a position.
- Subsequent items are `event` documents (§18) and `heartbeat` documents (§19).
- `source_sequence` increases by exactly one per event within an epoch. It
  exists for **gap detection only**.
- On a detected gap, the observer rejects the event's native mutation, preserves
  the last accepted native state as historical context, changes observer health
  to `stale`, and rejects subsequent events and heartbeats until a fresh snapshot
  atomically replaces that state. An epoch change or reconnect likewise requires
  registration and a fresh snapshot. There is no replay, no resume-after-N
  request, and no `resync_required` negotiation, because no history is retained
  to replay.

Deliberately excluded: replay buffers, cursor negotiation, replay expiration,
out-of-order event reordering, and event history. Stale status is refreshed by
re-reading current state, which is always cheaper and always correct.

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

## 18. Event documents

An event document is a closed JSON object reporting one authoritative native
transition. Events carry only what a status view needs; they never carry raw
tool arguments, command bodies, tool output, model thinking, or transcripts.

| Field                  | Type    | Required | Notes                                          |
|------------------------|---------|----------|------------------------------------------------|
| `schema`               | integer | yes      | Must be exactly `1`.                           |
| `kind`                 | string  | yes      | Must be `"event"`.                             |
| `agent_id`             | string  | yes      | Opaque safe-ASCII ID, 1–128 bytes.             |
| `lifecycle_generation` | integer | yes      | Positive (`>= 1`).                             |
| `source_epoch`         | string  | yes      | Opaque safe-ASCII stream identity.             |
| `source_sequence`      | integer | yes      | Increases by exactly one per event in an epoch.|
| `bridge_observed_ms`   | integer | yes      | Non-negative UTC epoch milliseconds.           |
| `event`                | object  | yes      | Closed payload, discriminated by `type`.       |

The identity triple must match the stream's snapshot. An event whose
`agent_id`, `lifecycle_generation`, or `source_epoch` does not match the
observed live instance is rejected with `JSP-E004` and never applied.

### 18.1 Event inventory

The `event` object is discriminated by a closed `type` field. Unknown types
fail with `JSP-E001`; there is no forward-compatible ignore rule, because
silently dropping a state transition would leave a status view wrong rather
than visibly unknown.

| `type`                 | Payload members            | Meaning                                    |
|------------------------|----------------------------|--------------------------------------------|
| `activity.changed`     | `state`                    | Native activity (§5) changed.              |
| `wait.opened`          | `reason`                   | An explicit blocking request opened (§6).  |
| `wait.resolved`        | none                       | The open wait was answered natively.       |
| `turn.started`         | none                       | A turn began; elapsed anchors at zero.     |
| `turn.ended`           | `outcome`                  | Turn finished: `completed`/`failed`/`cancelled`. |
| `todos.replaced`       | `revision`, `items`        | Full replacement of the todo list (§8).    |
| `tool_call.created`    | `label`, `phase`           | A tool call was created (§10).             |
| `tool_call.phase_changed` | `label`, `phase`        | The most recently created tool changed phase. |
| `assistant_message.displayed` | `content`, `committed_ms` | A completed reply became user-visible (§9). |
| `source.error`         | `summary`, `code`          | The source reported an error state (§12).  |
| `session.ended`        | none                       | The native session ended.                  |

Payload members reuse the exact types, closed inventories, and bounds defined
for the corresponding snapshot fields. `todos.replaced` obeys §8 in full,
including the positive-revision rule and the 256-entry and 2 KiB text bounds.

### 18.2 Semantics

- **Waiting requires an explicit event.** Only `wait.opened` without a matching
  `wait.resolved` puts an agent in `waiting_for_input`. Silence, elapsed time,
  and process age never imply waiting.
- **Turn elapsed is anchored, not transmitted.** `turn.started` anchors elapsed
  time at zero; Jefe advances the display from its own monotonic clock. A
  snapshot's `current_turn` carries an elapsed anchor for late subscribers.
- **Todos are full replacement.** There are no patches in JSP/1. A replacement
  whose `revision` does not exceed the applied revision is ignored as stale.
- **Last message updates only on commit.** `assistant_message.displayed` fires
  when the native UI has shown completed content. Streaming drafts are not
  represented in JSP/1 and can never replace a committed message.
- **Last tool is by creation order.** `tool_call.created` sets the current tool.
  `tool_call.phase_changed` updates that tool's phase and never reorders by
  completion time.
- **Producers cannot assert stale.** `stale` is an observer-side transport
  overlay (§20). A producer that sends it fails with `JSP-E005`.

## 19. Heartbeat documents

| Field                  | Type    | Required | Notes                                   |
|------------------------|---------|----------|-----------------------------------------|
| `schema`               | integer | yes      | Must be exactly `1`.                    |
| `kind`                 | string  | yes      | Must be `"heartbeat"`.                  |
| `agent_id`             | string  | yes      | Opaque safe-ASCII ID.                   |
| `lifecycle_generation` | integer | yes      | Positive (`>= 1`).                      |
| `source_epoch`         | string  | yes      | Opaque safe-ASCII stream identity.      |
| `bridge_observed_ms`   | integer | yes      | Non-negative UTC epoch milliseconds.    |

A heartbeat carries no `source_sequence`: it reports liveness of the status
source, not a state transition, and must not advance or gap the sequence.

Heartbeats exist so a healthy-but-quiet source is distinguishable from a hung
one. A missed heartbeat means observation health is `stale` — never that the
agent is idle, ready, or dead.

### 19.1 Heartbeat cadence and the observer lease

An observer holds a **lease** of 15000 ms: it marks observation health `stale`
once that much time passes with no accepted snapshot, event, or heartbeat.

A producer MUST heartbeat at an interval no greater than one third of the
lease, so two consecutive heartbeats can be lost before the observer declares
the source stale.

Choosing an interval equal to the lease is non-conforming even though it looks
correct: expiry then races scheduling jitter and the observation flickers
between `live` and `stale` for a source that is perfectly healthy. Both sides
of this repository pair previously defaulted to 15000 ms independently, which
is exactly that race.

## 20. Observation health is observer-owned

Observation health (`unsupported`, `connecting`, `live`, `stale`,
`disconnected`, `protocol_error`) is computed by the observer from transport
behavior and heartbeat timing. It is never a wire field, and a producer cannot
report it. This keeps the three axes orthogonal: process liveness is owned by
the Jefe runtime, observation health by the transport, and native activity by
authoritative producer events.
