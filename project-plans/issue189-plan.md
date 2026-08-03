# Issue #189 — Dedicated composer submit key with Enter reserved for newlines

> Issues #265, #383, and #480 already established registry-owned
> `Alt+Enter` / `Ctrl+Enter` submit actions and bare-Enter text behavior. Issue
> #189 closes the remaining acceptance gap: the active composer must expose its
> effective, user-remappable submit binding through the generated keybind footer,
> without retaining a stale hardcoded default. The issue comment is interpreted
> as requiring the same composer behavior on Pull Requests.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Target | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | User opens New Issue from Issues mode | Bare Enter in Body; bare Enter in Title | Local TUI on supported terminals | Body Enter inserts a newline; Title Enter advances focus and never submits | None | Draft/focus mutation only; no GitHub mutation | Existing draft/form behavior remains compatible | Pure input tests plus New Issue TUI scenario with two visible body lines |
| A2 | User composes an issue comment or reply | Bare Enter versus dedicated submit chord | Issues detail composer | Enter inserts a newline; default `Alt+Enter` or `Ctrl+Enter` resolves to `InlineSubmit` | Existing blank-draft validation remains visible | No GitHub mutation for blank/invalid draft | Existing `OpenNewIssueComposer` / `MutationSubmitted` flow is unchanged | Existing pure input and mutation tests plus focused production-route regression |
| A3 | User composes a PR comment, reply, or review-thread comment | Bare Enter versus dedicated submit chord | Pull Requests detail/changes composer | Enter inserts a newline; default `Alt+Enter` or `Ctrl+Enter` resolves to `PrInlineSubmit` | Existing blank-draft validation remains visible | No GitHub mutation for blank/invalid draft | Existing PR mutation flow is unchanged | Existing pure input tests, focused production-route regression, and deterministic PR TUI scenario |
| A4 | User overrides or unbinds a composer submit action in settings schema 2 | Override `issues.new-submit`, `issues.inline-submit`, or `prs.inline-submit` | Issues and Pull Requests composer contexts | Dispatch uses the effective chord; generated active-composer footer shows that effective chord, or `Unbound submit` when explicitly unbound | Invalid keymap continues to fail through existing settings diagnostics | No new side effects | Existing keymap format and action IDs remain authoritative | Pure projection tests for override/unbind, production registry-route tests, TUI scenarios with non-default submit chords |
| A5 | User opens any accepted composer with default settings | Active composer footer and embedded content | Issues and Pull Requests | Footer includes discoverable `submit` text generated from the action snapshot; embedded composer content contains no stale hardcoded submit chord | None | Display only | Emoji-free existing theme behavior | Pure projection/component tests and TUI frame assertions |

## Boundary decisions

- New Issue Title is a single-line field: bare Enter advances to Body rather than inserting a title newline; it still never submits.
- New Issue Body and all issue/PR comment bodies retain bare Enter as newline.
- Both `Alt+Enter` and terminal-portable `Ctrl+Enter` remain compiled defaults; this issue does not choose a new chord.
- Empty/blank submit behavior, mutation pending guards, retries, and GitHub diagnostics stay exactly as implemented.
- The issue comment's word “merge” is read in context as composer parity on the PR screen. PR merge-chooser confirmation is not a composer action and is an explicit non-goal.

## Non-goals

- Changing PR merge-chooser bindings or merge behavior.
- Changing mutation payloads, GitHub API calls, retry/pending semantics, or draft persistence.
- Adding a new input, footer, keymap, or process subsystem.
- Renaming public action IDs or changing settings schema.
- Refactoring unrelated static shortcut text outside issue/PR composer surfaces.
- Changing editor (as opposed to composer) behavior.
- Adding dependencies, workflow/quality configuration, or `.llxprt/` changes.

## Vertical slice — generated, remappable composer submit discovery

- **Rows:** A1–A5.
- **Owners / boundaries:** canonical action display inventory -> pure footer projection -> existing Issues/PR screen rendering; existing action registry -> existing input handlers. This is one bounded cutover across three ownership layers.
- **Allowed paths:** the existing action display/projection and keybind-bar modules; Issues/PR screen and composer-content modules with their existing tests; focused action-routing tests; issue-scoped schema-1 TUI scenario/runner files; this plan.
- **RED:** first make the New Issue scenario require a remapped submit hint while preserving two body lines; add a deterministic PR composer scenario requiring its own remapped submit hint and multiline text; add pure projection and production-route tests for effective composer chords. The current hardcoded `Alt+Enter` composer text must fail those remapped-hint assertions.
- **GREEN:** select composer-specific groups from the existing footer mode mechanism, render effective snapshot chords, remove only the superseded hardcoded submit text on accepted composer surfaces, and preserve existing input/mutation routing.
- **REFACTOR:** only remove duplication directly superseded by generated composer hints; do not clean adjacent shortcut text.
- **Verification:** focused Rust tests, both issue-scoped TUI scenarios, `cargo xtask quick`, reviewers/OCR, then exact-head `cargo xtask ci`.
- **Stop for approval:** a new subsystem or production module, a new public abstraction instead of extending the existing footer mode contract, mutation-flow changes, dependency/workflow/quality changes, or behavior outside A1–A5.

## Expected paths / architectural layers

- `src/domain/default_action_inventory_display.rs` — canonical composer footer groups referencing existing action IDs.
- `src/action_projection.rs` — pure selection/projection of composer footer modes and effective bindings.
- `src/ui/components/keybind_bar.rs` — pass the existing footer-mode selection into the pure projection.
- `src/ui/screens/issues.rs`, `src/ui/screens/pull_requests.rs` — select an existing composer footer mode from current state.
- `src/issue_detail_content.rs`, `src/pr_detail_content.rs`, `src/ui/components/pr_diff.rs` — remove only stale hardcoded submit chords superseded by the generated footer while retaining stable composer anchors.
- Existing adjacent unit/component tests and `src/app_shell_key_routing_tests.rs` — behavioral dispatch/projection evidence.
- `dev-docs/tmux-scenarios/issues-new-issue-typing.json` and an issue-scoped PR composer scenario/runner — real-TTY evidence for both screens.

No new subsystem, production module, dependency, public action abstraction,
workflow/quality-tool change, or unrelated refactor is authorized.

## Scope ledger

| Entry | Status | Reason |
|---|---|---|
| Composer-specific generated footer groups using existing action IDs | In scope | A4/A5 |
| Effective override/unbind projection and dispatch evidence | In scope | A4 |
| Remove hardcoded submit chords only from accepted composer surfaces | In scope | A5; prevents remap drift |
| Issues multiline and dedicated-submit evidence | In scope | A1/A2 |
| PR detail/changes composer parity | In scope | A3 and issue comment |
| PR merge chooser changes | Rejected | Explicit non-goal |
| Mutation/message-flow redesign | Rejected | Existing flow already satisfies A1–A3 |
| Adjacent static shortcut cleanup | Rejected | Outside accepted composer surfaces |

## Review and verification ledger

- Local OCR: `1 / 2`
- PR OCR: `0 / 2`
- Rust reviewer / DeepThinker: cycle 1 complete
- RED evidence: passed — the PR scenario, applied alone to unmodified HEAD `67511917`, failed with `HAR-E006: frame did not contain 'F9 submit' within 15000ms`; the baseline rendered its hardcoded default instead.
- Focused verification: projection override/unbind tests and state-derived production-route override tests pass; strict Clippy and source-size policy pass; existing issue/PR submit and bare-Enter tests remain in the suite.
- TUI verification: New Issue schema-1 scenario passes with `F8 submit` and two body lines; PR scenario passes with `F9 submit`, two body lines, bounded synchronization, and read-only `gh` audit.
- Review triage: fixed the reviewer findings for focused projection-test extraction, effective-snapshot production-route proof, deterministic PR scenario synchronization, and retained runner diagnostics. Rejected a proposed `issues.new-rewrite` action because no such canonical action exists and adding one would expand submit scope. OCR medium findings for temporary shim-audit cleanup and fail-closed zero/one-argument handling were fixed and retested; OCR low findings were rejected as non-defects or redundant shared-path coverage.
- Exact-head verification: passed with `cargo xtask ci` after `make ci-check` reported that this repository has no such Make target. The CI-equivalent command completed all format, policy, strict/complexity Clippy, coverage (67.98% lines), locked build, and locked workspace/all-feature tests successfully.
- CI / native Windows: pending
- Deferred findings: none

## Completion contract

Complete only when A1–A5 each have behavioral evidence, exact-head local gates
and required CI pass, review findings are explicitly triaged within the review
caps, the PR has correct ancestry and no conflicts, and every changed file maps
to this ledger. Stop at that point without optional cleanup or hardening.
