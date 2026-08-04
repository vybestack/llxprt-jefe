# Issue #621 — Send selected work items to agents from list views

> Issues and pull requests currently expose Send to Agent only from their
> detail contexts. Add registry-owned list actions that load the selected
> item's complete detail in place and then open the existing chooser without
> moving visible focus out of the list.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Target | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | Issues user in `issues.list` | Selected issue; default `Ctrl+S` | Local TUI with GitHub-backed repository | Full selected issue detail loads in place, then the existing Send to Agent chooser opens while `IssueList` focus and the selected row remain unchanged | Detail-load failures use the existing Issues diagnostic/auth-remediation path; no chooser opens | Detail fetch only; no prep or launch | No focus/selection persistence mutation; existing chooser contract retained | Schema-1 Issues TUI scenario plus registry/input/orchestration/reducer tests |
| A2 | Pull Requests user in `prs.list` | Selected PR; default `Ctrl+S` | Local TUI with GitHub-backed repository | Full selected PR detail loads in place, then the existing chooser opens while `PrList` focus and the selected row remain unchanged | Detail-load failures use the existing PR diagnostic/auth-remediation path; no chooser opens | Detail fetch only; no prep or launch | No focus/selection persistence mutation; existing chooser contract retained | Schema-1 PR TUI scenario plus registry/input/orchestration/reducer tests |
| A3 | Issues or PR user confirming the chooser | Existing configured or transient agent | Existing local/remote targets | Confirmation resolves the newly loaded full detail and continues through the existing issue/PR payload, preparation, preflight, and launch pipeline | Existing typed preparation/preflight/launch diagnostics | Exactly the existing confirmed-send side effects | Payload, assignment, repository preparation, prompt, and launch semantics unchanged | Existing send-flow regressions plus send-info tests using list-preserved focus |
| A4 | Issues or PR user canceling the chooser | `Esc` after direct list invocation | TUI | Chooser closes and the original list focus and selected row remain | None | No fetch beyond the completed detail load; no prep or launch | Existing chooser cancel behavior retained | Pure reducer/input test and both TUI scenarios |
| A5 | Issues or PR user with an empty list | Default or remapped list-send chord | TUI | No chooser, fetch, preparation, or launch occurs | Registry's shared unavailable diagnostic identifies the missing selected issue/PR | None | No state mutation except the shared unavailable notice | Availability/resolution tests and empty-list input test |
| A6 | User editing Keys bindings | Remap or unbind either list-send action | Settings-backed effective registry snapshot | Dispatch, footer, Help, Actions/Keys projections all reflect the effective binding or unbound state | Existing keymap diagnostics for invalid/conflicting edits | Existing settings persistence only | One registry snapshot remains authoritative; no parallel shortcut path | Registry projection/remap/unbind tests |
| A7 | Existing detail-view user | `Enter` from list, then `Shift+S` in detail | Issues and PRs | Existing detail navigation/load and chooser behavior remains unchanged | Existing diagnostics | Existing detail fetch and confirmed-send side effects | Existing action IDs and bindings remain compatible | Existing detail input tests plus focused regressions |

## Non-goals

- Changing issue or PR mutation behavior, assignment, repository preparation,
  payload formatting, prompt construction, preflight, or agent launch semantics.
- Replacing the agent chooser, auto-selecting an agent, or bypassing confirmation.
- Changing composer submit, issue close/delete, PR browser, or PR merge behavior.
- Adding a dependency, public abstraction, subsystem, or parallel shortcut/
  dispatch authority.
- Navigating to or simulating Issue Detail, PR Detail, or PR Changes focus.
- Fetching comments separately from the existing full-detail request contract.

## Vertical slices

### Slice 1 — Direct issue-list send

- **Rows:** A1, A3, A4, A5, A6, A7 for Issues.
- **Owner / boundary:** compiled action registry → existing Issues input handler →
  Issues detail-fetch orchestration → existing chooser reducer/send pipeline.
- **Allowed paths:** issue list action inventory/display projection, existing
  Issues action/input/availability tests, `src/app_input/issues_dispatch.rs`,
  existing Issues chooser reducer tests, and one schema-1 TUI scenario/fixture.
- **RED:** add the Issues schema-1 scenario first and prove `Ctrl+S` does not
  open the chooser from list focus; then add focused registry/input,
  no-selection availability, list-focus chooser/cancel, and full-detail send
  context tests.
- **GREEN:** `issues.list-send-agent` defaults to `Ctrl+S`, resolves through
  `HandlerKey::IssuesSendToAgent`, requests the existing full issue detail
  without applying `IssuesEnter`, opens the existing chooser only after a
  current correlated success, and preserves list focus/selection through Esc.
- **Non-goals:** no new public event family, chooser, send-info, payload, prep,
  or launch route. Typed events inside the existing Issues message family own
  correlated list-send continuation state.
- **Verification:** focused Rust tests, issue TUI scenario, JSON/shell
  validation, `git diff --check`, `cargo xtask quick`.
- **Stop for approval:** a new public abstraction/subsystem, dependency,
  workflow/quality-tool change, unrelated refactor/test relocation, or behavior
  outside A1/A3–A7.

### Slice 2 — Direct PR-list send

- **Rows:** A2, A3, A4, A5, A6, A7 for Pull Requests.
- **Owner / boundary:** compiled action registry → existing PR input handler →
  PR detail-fetch orchestration → existing chooser reducer/send pipeline.
- **Allowed paths:** PR list action inventory/display projection, existing PR
  action/input/availability tests, `src/app_input/prs_dispatch.rs` and its
  existing orchestration integration point, existing PR chooser reducer tests,
  and one schema-1 TUI scenario/fixture.
- **RED:** add the PR schema-1 scenario first and prove `Ctrl+S` does not open
  the chooser from list focus; then add focused registry/input,
  no-selection availability, list-focus chooser/cancel, and full-detail send
  context tests.
- **GREEN:** `prs.list-send-agent` defaults to `Ctrl+S`, resolves through
  `HandlerKey::PullRequestsSendToAgent`, requests the existing full PR detail
  without applying `PrListEnter`, opens the existing chooser only after a
  current correlated success, and preserves list focus/selection through Esc.
- **Non-goals:** no new public event family, chooser, send-info, payload, prep,
  or launch route. Typed events inside the existing PR message family own
  correlated list-send continuation state.
- **Verification:** focused Rust tests, PR TUI scenario, JSON/shell validation,
  `git diff --check`, `cargo xtask quick`.
- **Stop for approval:** a new public abstraction/subsystem, dependency,
  workflow/quality-tool change, unrelated refactor/test relocation, or behavior
  outside A2–A7.

## Expected paths / architectural layers

- `src/domain/default_action_inventory_s4.rs` — distinct list-context actions
  with `Ctrl+S`; preserve detail action IDs and `Shift+S`.
- `src/domain/default_action_inventory_display.rs` — Help/footer references
  include list and detail action IDs from the same snapshot.
- `src/state/action_availability.rs` — selection-first unavailable reasons for
  the two list actions, then existing agent eligibility.
- `src/app_input/action_handlers_s4.rs` and existing key tests — the new list
  bindings reuse existing handler keys and chooser event types.
- `src/app_input/issues_dispatch.rs` and the existing Issues reducer/message
  modules — start and cancel an exact reducer-owned continuation, reduce the
  full-detail result, and consume the one-shot ready event without changing
  `IssueFocus`.
- `src/app_input/prs_dispatch.rs`, the existing PR reducer/message modules, and,
  where required by existing file-size ownership, `src/app_input/prs_orchestration.rs`
  — equivalent PR completion.
- Existing Issues/PR reducer/send tests — permit list-focus chooser ownership,
  preserve cancellation state, and prove send info consumes full loaded detail.
- `dev-docs/tmux-scenarios/issue621/` plus existing fixture/shim registration
  paths — schema-1 direct-list behavior for both screens.

No new public event family, subsystem, dependency, workflow, quality-tool
change, `.llxprt/` change, or unrelated refactor is authorized.

## Scope ledger

| Entry | Status | Reason |
|---|---|---|
| Separate `issues.list-send-agent` and `prs.list-send-agent` registry actions | In scope | A1/A2/A6; list defaults differ from compatible detail defaults |
| Reuse existing send handler keys and chooser implementation | In scope | A1/A2/A7 |
| Full-detail fetch with a reducer-owned exact continuation and unchanged focus | In scope | A1/A2/A3; required to reject stale callbacks permanently |
| Typed begin/cancel/ready/auth events in existing Issue/PR message families | In scope | Reducer ownership and auth-remediation ordering required by A1/A2 |
| Selection-specific registry unavailability | In scope | A5 |
| Help/footer projection references for list actions | In scope | A6 |
| Existing detail action or payload/launch redesign | Rejected | Explicit non-goal |
| New public async/cancellation subsystem | Rejected | Existing reducers own typed continuation state |
| Dependency, workflow, quality-gate, or `.llxprt/` changes | Rejected | Outside issue scope |

## Review and verification ledger

- Local OCR: `2 / 2`; the first detached workspace review selected 40 files
  and returned 14 findings with four files incomplete due its per-file budget.
  Valid findings in projection tests, interaction-state ownership, scenario
  cleanup, and shim query classification were remediated; performance-only
  rebuild caching and incorrect settings-provenance advice were rejected. The
  second review selected 42 files, including tests and scenarios, but completed
  with internal invalid-line-range errors and no structured findings; session
  recovery confirmed no emitted code comments.
- PR OCR: `0 / 2`.
- Rust reviewer / DeepThinker: one comprehensive cycle each completed; blocker
  findings around reducer ownership, paired cancellation, auth ordering, stale
  ordinary-detail correlation, source size, and test coverage were remediated.
- RED evidence: stale reversible-context and same-index replacement regressions
  failed before reducer-owned continuation remediation.
- Focused verification: 14 binary list-send tests pass, reducer guard tests pass,
  strict Clippy passes, shell syntax and `git diff --check` pass.
- TUI verification: both schema-1 scenarios pass all 14 steps through
  `scripts/issue621-run-scenarios.sh`.
- Exact-head verification: every `cargo xtask ci` stage passed; the aggregate
  wrapper was externally terminated during long-running stages, so the
  remaining `complexity`, `coverage`, locked `build`, and locked `test` stages
  were also run directly and passed at the same worktree state.
- Native CI: pending PR checks.
- Deferred findings / follow-up issues: dispatch files remain above the 750-line
  recommendation but below the enforced hard limit; extraction is unrelated to
  accepted behavior and deferred.

## Completion contract

Complete only when A1–A7 have behavioral evidence, both list actions remain
registry-owned and remappable, no-selection paths have zero external side
effects, direct list invocation loads full correlated detail without changing
visible focus, existing detail sends remain compatible, both TUI scenarios pass,
exact-head local and required CI gates pass, reviews are triaged within their
counters, the PR is conflict-free with correct ancestry, and every changed file
maps to this ledger.
