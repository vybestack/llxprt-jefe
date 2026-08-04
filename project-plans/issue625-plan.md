# Issue #625 — JSP/1 loses todo in-progress state, so consumers must guess which task is active

> JSP/1 publishes each todo as `{ text, completed: bool }`. The native task
> model has three states, and the producer collapses `pending` and
> `in_progress` into the same `false`. "What is this agent doing right now"
> therefore does not survive the wire, and every consumer that wants to answer
> it has to guess. This change carries the task state itself so nothing has to
> be inferred.

## Compatibility decision (taken up front)

The issue asks for an explicit decision between three options: dual-carry the
boolean beside a new field for one version, bump the schema version, or take a
clean break.

**Decision: clean break inside JSP/1.** `{ text, completed: bool }` becomes
`{ text, state: string }`. The envelope `schema` stays `1`.

Rationale:

- The producer (`vybestack/llxprt-code`) and this consumer ship together, which
  is the condition the issue names as making a clean break cheap.
- Dual carry would put two sources of truth for one fact on the wire, and
  nothing in the protocol could adjudicate `completed: false` alongside
  `state: "completed"`. JSP/1's whole discipline is one authoritative reading
  per field.
- A version bump would mean a parallel v2 specification, schema, fixture, and
  scenario tree for a one-field amendment — a far larger change than the issue
  contemplates, and it would leave a v1 tree nobody produces.

Consequence, accepted deliberately: a producer still emitting `completed` is
rejected closed with `JSP-E001` rather than silently downgraded. That is the
existing closed-shape contract for every other field and it fails loudly at
ingress instead of rendering a confidently wrong checklist.

## Wire shape

```json
{ "text": "<string>", "state": "<string>" }
```

- `state` is required; `additionalProperties: false` still applies.
- Recognized values: `pending`, `in_progress`, `completed`.
- The set is open-ended. Any other string parses successfully and degrades to
  "not completed and not active" — it is never guessed into one of the three.
  This matches the philosophy the issue quotes from `mapTodoCompleted`, and it
  is deliberately unlike the closed enums (`wait.reason`, `tool.phase`,
  `turn.outcome`) which reject unknown labels: a todo state is producer
  vocabulary, not protocol vocabulary.
- Bounded at 64 bytes, like every other string on the wire. Over-bound fails
  `JSP-E002`.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|
| A1 | Snapshot parse | `todos` known, item `state` is `pending`, `in_progress` or `completed` | `TodoItem.state` is the matching `TodoState` variant, so the three native states stay distinguishable | n/a | None | Those three labels are the recognized vocabulary | `tests/jsp_v1_snapshot_compliance.rs` |
| A2 | Snapshot parse | item `state` is any other string, e.g. `blocked`, or empty | Parses; state is `TodoState::Unrecognized`; neither completed nor active | n/a | None | New producer vocabulary never breaks an older observer | `tests/jsp_v1_snapshot_compliance.rs` |
| A3 | Snapshot parse | item carries the retired `completed` boolean, with or without `state`, or carries no `state` at all | `JSP-E001` naming `snapshot.todos.value`, echoing no payload value | Nothing is returned to the caller | The retired shape is rejected, not downgraded | `tests/jsp_v1_snapshot_compliance.rs` |
| A4 | Snapshot parse | `state` of exactly 64 bytes / 65 bytes | 64 accepted | 65 fails `JSP-E002` naming `items[i].state` and no value | None | Inclusive bound, like every other bound | `tests/jsp_v1_snapshot_compliance.rs` |
| A5 | Event parse (`todos.replaced`) | The same four cases as A1–A4 | Identical typed results on the event path; rejections are `JSP-E001` echoing no payload value | Identical | None | The two paths cannot drift | `tests/jsp_v1_event_compliance.rs` |
| A6 | Preview render, known todos | items in each of the four states | `[x]` completed, `[>]` in progress, `[ ]` pending, `[?]` unrecognized | n/a | None | The active item is authoritative, never derived from position | `tests/jsp_preview_projection.rs` |
| A9 | Status workbench card, known todos | active item late in the list; nothing active; several active; unrecognized state | `▸` marks exactly the items the producer published as in progress, and the checkbox uses the same four-state marker as the Preview | n/a | None | The workbench stops presenting "first unfinished" as the active item | `src/workbench_view_tests.rs`, `src/workbench_view_paging_tests.rs` |
| A7 | Preview render, degraded/unknown/unsupported todos | as today | Unchanged `[stale]`, `(unknown)`, `(unsupported)`, `(no tasks)` output | n/a | None | No regression | `tests/jsp_preview_projection.rs` |
| A8 | External implementation consuming the published corpus | Whole `dev-docs/jsp/v1` tree | Specification §8/§13, both executable schemas, schema cases, the 15 scenarios, the producer trace, and every fixture describe exactly the `state` shape | A stale artifact fails the compliance profiles | None | The corpus stays the single language-neutral contract | `tests/jsp_v1_compliance*.rs`, `tests/jsp_v1_*_compliance.rs` |

## Non-goals

- No `schema` version bump; no v2 tree.
- No dual carry of `completed`.
- No change to the producer in `vybestack/llxprt-code` (separate repository —
  recorded as a follow-up, and the reason the two halves must land together).
- No new workbench behavior beyond the active marker. The workbench landed on
  main in #640 while this was open, which turns the issue's step 4 — "only then
  let the Preview and the status workbench mark the active item as
  authoritative" — from hypothetical into required. Its layout, paging,
  sorting, filtering, buckets and windowing are untouched.
- No widening of the todo payload beyond task state — no tool arguments, no
  output, no nested detail.
- No change to todo revision/full-replacement semantics, to `TodoProjection`,
  or to the reducer's todo counters.
- No new derived "active item" anywhere; the marker is the producer's state or
  nothing.

## Slices

1. **Protocol core.** `limits.rs`, `wire.rs`, `domain/observation.rs`,
   `validate.rs`, `event.rs`. RED first via the snapshot and event compliance
   suites.
2. **Published corpus.** Specification §8 and §13, `snapshot.schema.json`,
   `event.schema.json`, schema cases, scenarios s05/s11/s12, the producer
   trace, the fixture corpus, and the harness producer fixture.
3. **Preview.** `preview_view.rs` renders the authoritative active item.

## Expected files by layer

- Protocol: `src/jsp/v1/{limits,wire,validate,event}.rs`
- Domain: `src/domain/observation.rs`
- View: `src/preview_view.rs`
- Published contract: `dev-docs/jsp/v1/**`
- Tests: `tests/jsp_v1_snapshot_compliance.rs`,
  `tests/jsp_v1_event_compliance.rs`, `tests/jsp_preview_projection.rs`,
  `tests/fixtures/jsp_llxprt_fixture.rs`, `src/jsp/v1/projection_tests.rs`,
  `src/jsp/v1/compliance/reducer_tests.rs`

## Scope ledger

| Change | Justification |
|---|---|
| New `MAX_TODO_STATE_BYTES` bound and specification §13 row | Every wire string in JSP/1 is bounded; an unbounded open-ended field would be the only hole |
| Compliance fixtures/scenarios/traces edited wholesale | The retired field appears in each of them; a half-converted corpus would fail its own profiles |
| `src/workbench_todo_window.rs` extracted from `src/workbench_view.rs` | Forced by the hard source-size gate, not chosen. The workbench arrived from main at 986 lines and the active-marker change pushed it to 1014, over the 1000 limit. The todo window is the cohesive piece this issue touches, so it moves whole into a `#[path]` child module — the pattern `app_init.rs` and `mouse_routing.rs` already use — with no public surface change. The two workbench test files were rebalanced for the same reason |
| The status workbench's active marker (`src/workbench_view.rs` and its tests) | #640 landed the workbench on main mid-flight, carrying the exact derived marker the issue names: `▸` on the first item that merely was not finished. It is step 4 of the issue's own direction, it cannot compile against the amended domain type anyway, and leaving a guess beside an authoritative field would be a half-finished cutover |
| ~~`src/app_input/prs_lifecycle_key_tests.rs` restored to a compiling state~~ | Withdrawn. Approved as an unrelated exception while `origin/main` at `6b6d9289` could not build its binary test target, then superseded: #645 fixed the same defect on main with `NavState::rooted`, which states the session's screen exactly instead of pushing it onto the dashboard. Integrating main resolved the conflict in main's favour, so the branch no longer changes this file at all |

## Review counters

- Local OCR runs: 1 / 2 (0 findings)
- Post-PR OCR runs: 2 / 2, both `complete_best_effort` with 0 findings
  (`90d9c13` and `851d0997`). The repository enforces the same cap, and its
  budget comment now reports it reached. The merge that followed resolves one
  conflict in main's favour and touches no reviewed code, so no on-demand run
  was requested.
- Rust review: 1 (one finding, triaged below)
- LLxprt PR Review walkthrough: title, description, linked issues and scope all
  pass. Its one finding — the description did not follow the repository PR
  template — was fixed by restructuring the PR body.
- CodeRabbit: skipped by the repository's label configuration.

## Review triage

| Finding | Disposition | Action |
|---|---|---|
| The retired-boolean and missing-state tests asserted only the error code, so the diagnostic they claimed was never checked | In-scope—Fix | Both paths now assert the exact snapshot diagnostic and that no producer value is echoed, using a sentinel |
| Acceptance row A3 claimed a diagnostic naming `snapshot.todos.items[i]`, which the parser has never produced | In-scope—Fix | Row corrected to the measured behavior: `JSP-E001` naming `snapshot.todos.value`, no echoed value |
| Structural rejection inside a payload should name the offending member, and an event should not label itself a snapshot | Defer | Protocol-wide and pre-existing: measured identically on `snapshot.current_wait.value`, and the event label comes from the shared entry point this change does not touch. Filed as #646 |
| Add a direct unit test for `TodoState::from_wire` | Reject | Every recognized mapping and the degrade are already proven through both public parse paths; a match-arm test would restate the implementation |
| Add a multibyte state case | Reject | The bound is a byte bound, `str::len()` is bytes, both paths exercise 64 and 65 bytes, and the schema oracle verifies `x-jsp-maxUtf8Bytes` separately |

## Verification evidence

- `cargo fmt --all --check` — clean.
- `cargo clippy --lib --test jsp_v1_snapshot_compliance --test jsp_v1_event_compliance
  --test jsp_preview_projection --all-features -- -D warnings` — clean.
- `cargo test --lib` — 3554 passed, 1 failed
  (`harness::signal_cleanup::tests::signal_delivery_triggers_cleanup_and_exit`,
  see the mainline blocker below).
- `cargo test --test jsp_v1_snapshot_compliance --test jsp_v1_event_compliance
  --test jsp_preview_projection --test jsp_v1_compliance
  --test jsp_v1_compliance_slice_b` — 128 passed, 0 failed.
- `cargo test --test jsp_host_socket --test jsp_two_instance_identity
  --test jsp_launch_lifecycle` — 23 passed, 0 failed.

- `cargo xtask ci` — every gate green on the candidate head: fmt,
  check-clippy-allows, check-source-size, check-architecture,
  check-multiplexer-surface, lint, complexity, coverage, build, test.
- CI on PR #647 at exact head `9fc747bd`, the merge that integrates main — 18
  checks pass, 3 skipping (the optional TUI smoke, the main-flake baseline
  record, and the OCR infrastructure-failure notifier, which skips because OCR
  succeeded). Native Windows, Windows Clippy, Windows coverage floors and the
  coverage gate are among the passing set. `mergeable: MERGEABLE`,
  `mergeStateStatus: CLEAN`. `cargo xtask ci` also passes locally on that head
  with zero test failures.

### Mainline blocker, fixed here and then superseded by main

`origin/main` at `6b6d9289` did not compile its binary test target:

    error[E0560]: struct `jefe::state::AppState` has no field named `screen`
      --> src/app_input/prs_lifecycle_key_tests.rs:24:9

PR #644 (issue #386) made `nav` the sole authority for screen identity and
removed `AppState::screen`; PR #643 (issue #183) landed a test that still
constructs `AppState { screen: ScreenId::PullRequests, .. }`. Neither CI run
saw the other. Reproduced on a clean stash of `origin/main` with no changes from
this branch applied.

That one defect blocked every gate: `cargo clippy --all-targets`, `cargo test
--workspace`, and `cargo xtask ci` on any branch cut from main. It was also the
cause of the signal-cleanup failure, which spawns a child `cargo test` and
reads its exit status, so it reported 101 instead of 143. It was fixed here
with explicit approval, which unblocked verification.

#645 then landed the same fix on main using
`NavState::rooted(ScreenId::PullRequests)`. That is the better statement — a key
test wants the session to *be* on the pull-requests screen, not to have pushed
it on top of the dashboard — so integrating main resolved the conflict in main's
favour and the branch stopped touching the file. The exception is withdrawn
rather than carried.

### One load-sensitive test, not a regression

The first `cargo xtask ci` run failed
`harness_v1_fixtures::llxprt_continue_field_fixture_sends_one_exact_issue_prompt`
("Issues Send must launch LLxprt once"). It passes alone, and the same
`cargo test --workspace --all-features --locked` command passed immediately
afterwards, as did a second complete `cargo xtask ci`. The test launches a real
multiplexer session and counts launches, so it is sensitive to load rather than
to anything in this change, which touches neither Issues Send nor the launcher.

## Deferred findings

- Producer amendment in `vybestack/llxprt-code`
  (`jspRedaction.ts` `buildTodoItems`/`mapTodoCompleted`, `jspDocuments.ts`) —
  filed as vybestack/llxprt-code#3003. It must land with this change.
- Protocol-wide diagnostic precision for structural rejections, and the event
  document that names itself a snapshot — filed as #646.
