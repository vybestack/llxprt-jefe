# Issue 423 delivery plan

## Issue and baseline

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/423
- Branch: `issue423`
- Base: `origin/main` at `edeff48`
- Reported behavior: contextual issue and pull-request composers reserve at most five rows, but Issues currently sizes the read-only document from logical line count while `ScrollableText` consumes wrapped display rows. A long logical line can therefore leave usable document rows blank.
- Origin: deferred intentionally from PR 418 / issue 408 so the contextual scroll/display ownership contract could change independently from New Issue textbox expansion.
- Discussion: the only issue comment is an automated planning placeholder and adds no behavioral requirements.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundaries | Target | Observable success | Observable failure / diagnostics | Permitted side effects | Persistence / compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | User opens an Issues New Comment or Reply composer | Narrow detail content; one logical document line wraps to multiple display rows; normal and tiny pane heights | Local TUI; platform-independent projection | The read-only document receives up to its wrapped display-row count within the contextual capacity; available rows are not discarded merely because logical line count is smaller | Existing banner/error paths remain unchanged; zero/tiny dimensions do not panic or overflow | Render only until existing submit/cancel action | Content-line scroll offsets, draft state, submit behavior, and persistence remain unchanged | updated real-TTY Issues scenario plus narrow-width issue projection/render regression |
| A2 | User opens a PR New Comment, conversation Reply, or review-thread Reply composer | Narrow and wide detail content; short, wrapped, normal, and tiny panes | Local TUI; all platforms | PR and Issues use the same contextual allocation policy: document rows are bounded by wrapped display rows and available capacity; composer remains directly below short context and never exceeds five rows | Existing PR error/loading paths remain unchanged | Render only until existing submit/cancel action | Existing PR draft, reply target, and mutation behavior remain unchanged | narrow-width PR projection/render regression and shared layout tests |
| A3 | User scrolls or opens a reply composer | Existing content-line scroll offset, bottom clamp, focused comment/thread reply anchor, wrapped content | All platforms | Scroll offsets remain in content-line units; reply anchor remains visible; state maximum offsets continue to agree with the renderer's contextual capacity for Issues and PRs | No new diagnostics; existing no-detail/no-anchor no-op behavior remains | Existing deterministic state transition only | No state schema or event change | existing issue/PR state focus regressions plus focused parity assertions around the shared capacity contract |
| A4 | User composes in any contextual comment/reply textbox | Normal pane, tiny pane, and content shorter/taller than capacity | All platforms | Composer receives remaining rows after contextual document allocation, preserves at least the existing document reservation on non-empty tiny panes, and is capped at five rows | Zero-row panes render no rows without panic | Render only | Existing caret-following TextBox contract unchanged | shared pure layout tests and issue/PR component tests |

## Explicit non-goals

- No change to New Issue fill-available guidance/textbox allocation from issue 408.
- No change to `ScrollableText` wrapping, terminal-cell measurement, selection mapping, scrollbar behavior, or content-line scroll-offset units.
- No manual display-row scroll offset, persisted geometry, new state event, or per-keystroke parent scrolling.
- No change to comment/reply submission, editing, pagination, GitHub I/O, keybindings, or persistence.
- No composer expansion beyond the existing five-row contextual cap.
- No dependency, manifest, workflow, agent-memory, `.llxprt/`, `.code_puppy/`, quality-gate, or harness-runner change.
- No migration of legacy scenarios to schema 1; update the existing issue detail wrap scenario because schema-1 direct PTY input is already tracked outside this issue.

## Bounded vertical slices

### Slice S1: shared contextual row allocation and Issues behavior

- Acceptance rows: A1, A3, A4.
- Architecture owner: pure layout/projection layer; integration boundary is `IssueDetailProjectionInputs -> DetailPaneProps`.
- Allowed production paths: `src/layout.rs`, `src/ui/components/issue_detail.rs`.
- Allowed evidence paths: `src/ui/components/issue_detail_render_tests.rs`, `dev-docs/tmux-scenarios/issues-detail-word-wrap.json`, this plan.
- RED: update the existing real-Jefe Issues detail scenario first; add a narrow-width projection/render test requiring wrapped document rows rather than logical-line rows.
- GREEN: introduce one shared pure contextual allocation value/function and use wrapped document rows in Issues.
- REFACTOR: keep wrapping in the existing `doc_wrap` projection and allocation in the pure shared layout helper; do not add state or component-local layout variants.
- Verification: focused layout and issue-detail tests, updated scenario, `make quick-check`.
- Stop conditions: new state events/fields, a wrapping subsystem, harness production changes, dependency changes, or paths outside this slice.

### Slice S2: shared wrapped scroll geometry and width-aware state

- Acceptance rows: A1-A4.
- Architecture owner: `domain::document_wrap` as the project-independent geometry layer; state consumes only domain geometry and boundary-supplied dimensions.
- Allowed production paths: `src/domain/document_wrap.rs`, `src/domain/mod.rs`, Issues/PR state type and operation modules, input orchestration, mouse detail routing, and the UI compatibility re-export.
- RED: state-level narrow-width tests require nonzero content-line offsets and visible composer help anchors where logical-line bounds returned zero; the strong real-TTY fixture must fail before implementation.
- GREEN: boundary routes store content width; state maximum offsets and reveal transitions consume the same wrapped rows the renderer consumes; offsets remain logical lines.
- REFACTOR: remove duplicate PR maximum-bound implementations and retain one shared geometry algorithm.
- Verification: focused geometry/state tests, strong 13-step real-TTY scenario, `make quick-check`.
- Stop conditions: new events, persisted geometry, changed offset units, new dependency, or unrelated UI behavior.

### Slice S3: PR policy parity and state-bound evidence

- Acceptance rows: A2, A3, A4.
- Architecture owner: PR pure detail projection consuming the S1 shared layout contract; state remains content-line based.
- Allowed production paths: `src/ui/components/pr_detail.rs`; `src/layout.rs` only for S1 contract refinement.
- Allowed evidence paths: `src/ui/components/pr_detail_render_tests.rs`, focused existing issue/PR state tests when parity needs an explicit regression, this plan.
- RED: add narrow-width PR tests requiring the same wrapped-row contextual allocation and five-row cap.
- GREEN: route PR detail projection through the shared allocation helper.
- REFACTOR: remove duplicated issue-only allocation logic; retain the existing state capacity helper as the single scroll-bound reservation contract.
- Verification: focused PR projection/render and composer-focus tests, `make quick-check`.
- Stop conditions: persisted geometry or new state events, changed scroll-offset units, reply-anchor relocation, pagination changes, or any unplanned public abstraction.

## Expected paths by layer

| Layer | Expected paths | Acceptance mapping |
| --- | --- | --- |
| Shared pure layout | `src/layout.rs` | A1-A4 |
| Issues pure projection | `src/ui/components/issue_detail.rs` | A1, A3, A4 |
| PR pure projection | `src/ui/components/pr_detail.rs` | A2-A4 |
| Issues component evidence | `src/ui/components/issue_detail_render_tests.rs` | A1, A3, A4 |
| PR component evidence | `src/ui/components/pr_detail_render_tests.rs` | A2-A4 |
| State evidence, only if existing tests do not prove parity | existing focused files under `src/state/` | A3 |
| Real-TTY evidence | `dev-docs/tmux-scenarios/issues-detail-word-wrap.json` | A1, A4 |
| Delivery record | `project-plans/issue423-plan.md` | all rows |

Approved expanded scope: at most 25 changed files and under 1,500 net changed lines. In addition to one typed contextual row-allocation contract in `layout`, the existing pure document wrapper is promoted below UI so state and rendering share width-aware geometry. The user explicitly approved this expansion after the stronger TUI RED exposed the state/render disagreement.

## Scope ledger

| Discovery | Disposition | Rationale / follow-up |
| --- | --- | --- |
| `ScrollableText` already exposes the canonical terminal-cell-aware `doc_wrap::wrap_document` projection | In-scope design constraint | Reuse it; do not duplicate wrapping or convert state offsets to display rows. |
| Issues shrinks contextual document rows by logical lines while PR always reserves full capacity | In-scope fix | Both projections should consume one wrapped-display-row-aware contextual allocation policy. |
| Strong TUI RED proved state lacked the content width needed to reveal wrapped tail anchors | Approved in-scope expansion | The user approved promoting the existing pure wrapper to `domain::document_wrap`, storing the latest Issues/PR content width, and using one geometry source for render allocation, state bounds, and anchor reveal while offsets remain content-line based. |
| Existing issue/PR reply-open tests cover anchor visibility with fixed contextual capacity | In-scope evidence reuse | Keep these tests green and add only focused parity evidence not already covered. |
| Existing issue detail wrap scenario is pre-schema and externally fixture-backed | In-scope evidence update | Update it in place rather than changing the schema-1 runner, which issue 408 already identified as a separate quality-tool concern. |
| RED/GREEN scenario with content whose wrapped height exceeded the fixed document capacity exposed an anchor-visibility gap | Approved in-scope expansion | Wrapped allocation alone could not preserve tail anchors. The approved shared domain geometry and width-aware state contract now close that gap. |
| Approved geometry expansion remained within 25-file target | In-scope | Scope review found 24 changed paths including the wrapper move, about 370 tracked insertions plus the moved 444-line module and plan; no hard-budget trigger. |

No unapproved scope discoveries are open.

## Review counters

- Pre-PR Open Code Review: 2 / 2 (both invocations terminated by external signal 15 with no output; no findings available to triage)
- Post-PR Open Code Review: 0 / 2

## Verification evidence

| Candidate head | Command / evidence | Result |
| --- | --- | --- |
| `edeff48` | source audit | Baseline confirmed: Issues uses logical line count for contextual rows; PR uses fixed document capacity; both state paths use content-line offsets and the shared fixed capacity helper |
| baseline working tree | focused allocation regressions | RED: Issues allocated 12 logical rows instead of 28 wrapped rows; PR allocated 28 fixed rows instead of 20 wrapped rows |
| baseline working tree | strong isolated `issues-detail-word-wrap.json` fixture | RED: long wrapped tail/new-comment anchor could not be revealed by logical-line state bounds |
| working tree | focused domain geometry, layout, Issues state, PR state, and projection tests | PASS |
| working tree | isolated fail-closed real-Jefe TUI scenario | PASS: 13 steps; wrapped tail and contextual New Comment anchor visible together |
| working tree | `make quick-check` and `git diff --check` | PASS |
| working tree | format, allow-policy, source-size, full Clippy, complexity Clippy, locked all-feature build | PASS; source-size emitted recommendation-only warnings |
| working tree | locked all-feature workspace tests with `--test-threads=1` | PASS; normal parallel gate repeatedly misclassified one one-second package-probe test under unrelated host cargo load |
| working tree | exact `cargo llvm-cov` workspace/all-feature coverage gate | PASS: 71.57% line coverage, above the required 30% floor |
| working tree | aggregate `make ci-check` attempts | Aggregate wrapper received external signal 15 during compilation; each required gate passed exactly and individually on the current source |

## Deferred findings and follow-ups

- None at planning time.
