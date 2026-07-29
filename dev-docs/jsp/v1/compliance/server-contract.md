# JSP/1 server adapter compliance contract

A JSP/1 server (broker) implementation proves compliance by emitting a
language-neutral **normalized HTTP/SSE adapter transcript** that the compliance
runner validates against the current-state JSP/1 transport contract. The runner
does **not** perform live networking: it validates the transcript the server
implementation recorded.

## Why a transcript, not live networking

JSP/1 has no HTTP dependency in this crate, and the server transport contract
is observer-owned. Validating a normalized transcript lets an external server
implementation (LLxprt Luther) prove the exact same invariants without the
runner depending on a network stack, a model, or a terminal. The transcript is
the language-neutral contract surface.

## Transcript shape

The transcript is a single JSON document (`server-transcript.json`) containing:

- `schema`: `1`
- `kind`: `server-transcript`
- `server_version`: opaque server label
- `challenge_nonce`: runner-supplied nonce that binds the transcript to the
  runner's challenge. A replayed transcript from a different nonce cannot pass
  because the observed result must incorporate the nonce.
- `trusted_credentials`: closed credential-handle/principal-handle records with
  trusted roles and optional publisher identity bindings. Handles are evidence
  references, never secret token values.
- `interactions`: an ordered array of normalized HTTP/SSE interactions. Each
  interaction has:
  - `name`: descriptive label only; the profile never trusts it for semantics.
  - `request`: a closed typed method/route/credential/principal/identity/body
    contract. The trusted inventory, not a request role label, authorizes it.
  - `response`: a closed typed status/result/body contract, or a typed SSE stream.
  - `assert`: descriptive label only; changing it cannot make a transcript pass.

The profile derives every invariant from request, response, parser result,
canonical reducer state, stream ordering, and fake-clock lease behavior.

## Normalized routes and roles

| Route | Method | Role | Purpose |
|---|---|---|---|
| `/jsp/1/register` | POST | publisher | Bind publisher credentials to one agent_id + lifecycle generation + source epoch |
| `/jsp/1/publish` | POST | publisher | Publish a snapshot/event/heartbeat |
| `/jsp/1/observe` | GET (SSE) | observer | Open a snapshot-first stream |
| `/jsp/1/heartbeat` | POST | publisher | Report source liveness |
| `/jsp/1/control` | POST | publisher | Role-separation probe; an observer attempt must be rejected |
| `/jsp/1/internal/lease_expired` | POST | server | Runner-owned lease expiration observation |
| `/jsp/1/internal/observation_digest` | GET | server | Runner-owned canonical-state digest proving a rejection caused no mutation |

The first four routes are the JSP/1 transport surface an external server must
implement. The final three are runner-owned internal routes: the compliance
runner drives them to observe canonical state, and only a trusted `server`
principal may answer the two `internal` routes.

Only a `publisher` may publish/heartbeat/register-as-publisher. Only an
`observer` may open an observation stream. A publisher may never observe an
unrelated session. An observer may never publish or control.

## Server profile invariants (validated against the transcript)

1. **Registration identity** — registration binds exactly one publisher to one
   `(agent_id, lifecycle_generation, source_epoch)`.
2. **Role separation** — observer publish/control attempts fail; publisher
   observes of unrelated session fail.
3. **Identity-class rejection** — valid identities are classified as unrelated
   agent, stale lifecycle generation, or stale source epoch. Every rejection
   remains pending until a later canonical observation proves no mutation.
4. **Canonical snapshot-first stream** — the first SSE item is exactly one
   atomic snapshot; subsequent items are contiguous events under current-state
   semantics. The reduced SSE native state must equal accepted canonical
   reducer state (no history, replay, resume, or cursor negotiation).
5. **Duplicates/out-of-order/gaps** — a duplicate sequence is a no-op; an
   out-of-order (lower) sequence is a no-op; a gap requires a fresh
   snapshot-first stream.
6. **Heartbeat/lease** — heartbeats preserve canonical native state while
   observation health changes independently; a missed lease marks observation
   health stale, never idle/dead native activity.
7. **Observation health** — observer-owned; transitions are computed from
   transport behavior, never producer-reported.
8. **Bounds** — the server enforces JSP document and payload bounds; a
   parser-proven limit-plus-one document is rejected with the exact status and
   typed reason.

## Credentials

Credentials never appear inside JSP documents or diagnostics. They are
out-of-band HTTP authorization material. The transcript carries opaque trusted
credential and principal handles, never token values. Requests do not
self-attest roles; authorization resolves the handle pair through the trusted
inventory and checks any publisher binding before route semantics run.

## Runner-owned challenge execution (Slice B)

The compliance runner may invoke an adapter command (`--adapter COMMAND`) or
use the built-in reference adapter (`--reference-adapter`) to execute the
challenge rather than replaying a self-attested transcript. The runner supplies:

- A nonce and expected checked adapter protocol version.
- The complete launch identity triple and process binding.
- A closed trusted credential/principal inventory. Every entry binds one opaque
  credential handle, principal handle, role, and complete identity triple.
- The same bounded operation schedule, fake clock, and source challenges used
  by the producer qualification protocol.

Producer/server qualification never accepts `--input` or a default static
transcript. Unknown handles are 401 `unknown_authentication`; a known principal
with the wrong complete binding is 403 `forbidden_binding`; an authenticated
wrong role is 403 `forbidden_role`. Security proof bits are credited only to
principals in the runner inventory. Every rejected duplicate, lower, gap,
forbidden, binding, or bounded operation is followed immediately by a trusted
server-principal observation digest; a second rejection cannot overwrite a
pending observation.

Strict SSE evidence includes both (1) an atomic snapshot at an earlier accepted
cursor followed by a real state-changing contiguous event tail to the current
canonical state and (2) a current snapshot-only stream. Heartbeats in an SSE
tail are optional and cannot substitute for a state-changing event. Dedicated
heartbeat times are monotonic. Lease output carries native activity value,
availability, and provenance exactly and never synthesizes idle from unknown,
degraded, or unsupported activity.
