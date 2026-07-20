# Issue #214 — Agent rewrite the issue draft

**Repository:** vybestack/llxprt-jefe
**Issue:** `agent write the issue` (enhancement, OPEN)

## Problem statement

When composing a new GitHub issue in the Issues-mode inline composer, the
user has no way to ask the configured default agent (LLxprt or Code Puppy,
per the repository's `default_agent_kind`) to **rewrite / improve** the draft
before submitting it. Today the only agent interaction is "send this issue to
an agent" (launch an interactive session to *work on* the issue).

## Decision (interpretation of an exploratory issue)

The issue body is exploratory ("we might", "we could"). The clearest
actionable core is:

> In the new-issue composer, ask the configured default agent to rewrite the
> current draft **non-interactively** and replace the composer text with the
> result, leaving the composer open for review/edit.

Both `llxprt` and `code-puppy` support non-interactive print mode:
`llxprt -p/--prompt <prompt>` and `code-puppy -p/--prompt <prompt>` both run a
single prompt, print the response to stdout, and exit (verified via `--help`).

## Acceptance matrix

| # | Actor / path | Input / boundary | Success behavior | Failure behavior |
|---|---|---|---|---|
| A1 | New-issue composer (`InlineState::Composer{NewIssue}`), `Ctrl+R` | Non-empty draft | `RequestIssueRewrite` dispatched; `rewrite_pending=true`; default-agent runs non-interactively in the repo's working copy (if any) | — |
| A2 | Rewrite completes | Agent stdout non-empty | Composer text replaced with trimmed stdout; cursor→end; `rewrite_pending=false`; `draft_notice="Issue rewritten by agent"` | — |
| A3 | Rewrite fails | Agent missing / non-zero exit / empty stdout / panic | `rewrite_pending=false`; `draft_notice` carries the error | — |
| A4 | Composer not `NewIssue` | `Ctrl+R` while editing a comment / editor | No-op (request ignored) | — |
| A5 | Availability | Default-agent binary not installed | Fails fast with actionable error (reuses availability guard) | — |
| A6 | Pending guard | `Ctrl+R` while already rewriting | Ignored (idempotent) | — |

## Architecture (layers)

1. **Domain (pure):** `src/domain/issue_rewrite.rs` —
   `build_rewrite_instruction(draft, github_repo) -> String`. Produces the
   instruction: rewrite the issue, study the repo source, output ONLY the
   rewritten issue (first line = title). Fully unit-tested.
2. **Runtime (pure argv + boundary):** `src/runtime/non_interactive.rs` —
   `non_interactive_argv(signature) -> (AgentExecutableTarget, Vec<String>)`
   (pure; builds the `-p/--prompt <instruction>` argv, reusing the shared
   target resolution), and `run_non_interactive(signature, cwd, instruction)`
   (resolves the executable, runs it, captures stdout, trims).
3. **State:** `AppEvent::{RequestIssueRewrite, IssueRewriteSucceeded{text},
   IssueRewriteFailed{error}}`; `IssuesState.rewrite_pending: bool`; reducer
   `src/state/issues_rewrite_ops.rs`.
4. **Orchestration:** `src/app_input/issue_rewrite.rs` — resolve focused repo
   + work_dir + default-agent signature (`launch_signature_for_transient`),
   build instruction, availability guard, spawn via `gh_async`, apply
   success/failure events.
5. **Key + dispatch:** `Ctrl+R` in the inline composer; `IssuesMessage`
   round-trip; dispatch arm in `issues_dispatch.rs`.
6. **UI:** "Rewriting issue with agent…" banner while `rewrite_pending`, plus a
   composer key hint.

## Scope ledger / non-goals (deferred to follow-ups)

- Remote-target non-interactive execution (local only; remote needs SSH
  plumbing for `--prompt` capture).
- Choosing *which* agent (uses the repo default agent kind + defaults).
- PR-description rewrite (issues only).
- A separate "plan mode" toggle / coderabbit-style plan generation (the
  instruction asks for a well-structured, acceptance-criteria-rich issue,
  capturing the spirit).
- Cancellation UI beyond the runner's bounded timeout.

## Verification

`make quick-check` during iteration; `make ci-check` before push.
