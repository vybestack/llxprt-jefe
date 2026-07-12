//! Generic, runtime-neutral PR-completion delivery contract for issue sends.
//!
//! This module is iocraft-free and side-effect-free. It owns the single source
//! of truth for the end-to-end delivery workflow that Jefe injects into every
//! Send Issue prompt. Keeping the contract here — rather than relying on
//! repository-local agent memories (`.llxprt/LLXPRT.md`), `AGENTS.md`, Code
//! Puppy defaults, or model memory — makes correct delivery behavior
//! runtime- and machine-independent (issue #227).
//!
//! The contract is intentionally generic across Code Puppy, LLxprt, and future
//! runtimes. It names `gh` because GitHub integration is always active for an
//! issue send (the issue context itself is sourced via `gh`), but it prescribes
//! no runtime-specific argv, no project-specific instruction files, and no
//! model memory. Only the argv *transport* differs across runtimes, and that is
//! owned by `fresh_prompt::prepare_fresh_prompt_signature`.

/// Build the generic end-to-end PR-completion delivery contract that Jefe
/// appends to every Send Issue prompt.
///
/// The returned string is a self-contained, capability-neutral workflow. It is
/// identical for every runtime (Code Puppy, LLxprt, future runtimes); only the
/// argv transport differs, which is constructed separately. The contract:
///
/// - Starts from the target repository's latest base branch without deleting
///   unrelated or agent-configuration files.
/// - Creates a dedicated issue branch before changing code.
/// - Fetches the issue and all comments with `gh`.
/// - Researches the codebase and creates a test-first implementation plan.
/// - Implements and runs the repository's *complete* verification suite.
/// - Commits, pushes, and opens a detailed PR linked to the issue.
/// - Watches all PR workflows to terminal completion and loops on failures.
/// - Collects all PR reviews, inline review threads, and automation comments
///   (including OCR and CodeRabbit findings).
/// - Addresses actionable findings, reruns verification, pushes, and replies
///   in-thread explaining each fix or why a finding does not apply, resolving
///   addressed threads where supported.
/// - Repeats the checks/reviews/fix/reply cycle until required workflows pass
///   and no actionable unresolved review threads remain.
/// - Never stops merely because workflows are pending; polls with a bounded
///   delay until completion or reports a genuine external blocker.
#[must_use]
pub(super) fn issue_delivery_contract() -> &'static str {
    // Keep this as one continuous, ordered, numbered contract. The exact
    // wording is asserted by unit tests so all runtimes receive identical
    // instructions. Returned as a `&'static str` because the text is a fixed
    // literal; there is no per-call allocation.
    "\
## Delivery Workflow

Follow this end-to-end workflow to deliver this issue autonomously. This \
contract is supplied by Jefe and is the same for every runtime; repository-\
local agent memories or instruction files may supplement it but are not \
required and must not be relied on for these delivery semantics.

1. Start from the target repository's latest base branch. Do not delete \
unrelated files, agent-configuration files, or version-controlled tooling \
configuration.
2. Create a dedicated issue branch before changing any code.
3. Fetch the issue and all of its comments with `gh` so you have the complete \
context.
4. Research the relevant codebase and create a test-first implementation plan \
before writing production code.
5. Implement the change, then run the repository's complete verification suite \
(format, lint, build, and tests) — not merely a convenient subset.
6. Commit and push the branch.
7. Create a detailed pull request linked to this issue (for example with \
`closes #N` or `fixes #N` in the body).
8. Watch all pull request workflows until they reach a terminal state; do not \
stop merely because checks are pending. If a watch or poll command times out \
while workflows remain pending, invoke it again and keep polling rather than \
returning control before the pull request is green.
9. When a workflow fails, inspect its logs, remediate the failure, rerun the \
verification suite, commit, push, and watch again.
10. Fetch all pull request reviews, inline review threads, and automation \
comments — including automated code-review bot findings (for example Open Code \
Review and CodeRabbit).
11. For each actionable finding: address it, rerun the verification suite, \
commit, push, and watch again. Reply in the corresponding review thread \
explaining each fix or why a finding does not apply, and resolve addressed \
threads where the host supports it.
12. Repeat the checks / reviews / fix / reply cycle until all required \
workflows pass and no actionable unresolved review threads remain.
13. If a genuine external blocker prevents completion (for example an \
infrastructure outage or a missing secret), report it explicitly rather than \
stopping silently while checks are still pending. Otherwise continue polling \
with a bounded delay until completion."
}

#[cfg(test)]
mod tests {
    use super::issue_delivery_contract;

    /// The contract is non-empty and is a markdown section.
    #[test]
    fn contract_is_a_markdown_section() {
        let contract = issue_delivery_contract();
        assert!(
            contract.starts_with("## Delivery Workflow"),
            "contract must open with the Delivery Workflow heading; got:\n{contract}"
        );
    }

    /// The contract must name `gh` (GitHub integration is always active for an
    /// issue send) while remaining runtime-neutral.
    #[test]
    fn contract_names_gh_for_github_operations() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("`gh`"),
            "contract must reference gh for fetching issue/comments; got:\n{contract}"
        );
    }

    /// The contract must NOT depend on runtime-specific instruction files or
    /// model memory.
    #[test]
    fn contract_is_runtime_and_memory_neutral() {
        let contract = issue_delivery_contract();
        assert!(
            !contract.contains(".llxprt/LLXPRT.md"),
            "contract must not reference .llxprt/LLXPRT.md"
        );
        assert!(
            !contract.contains("AGENTS.md"),
            "contract must not reference AGENTS.md"
        );
        assert!(
            !contract.contains("Code Puppy"),
            "contract must be runtime-neutral (no Code Puppy mention)"
        );
        assert!(
            !contract.contains("LLxprt"),
            "contract must be runtime-neutral (no LLxprt mention)"
        );
    }

    /// Acceptance: the contract must instruct the agent to create a branch and
    /// a pull request.
    #[test]
    fn contract_requires_branch_and_pull_request() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("issue branch"),
            "contract must instruct creating a dedicated issue branch"
        );
        assert!(
            contract.to_lowercase().contains("pull request"),
            "contract must instruct creating a pull request"
        );
    }

    /// Acceptance: the contract must require watching workflows through
    /// completion and looping on failures.
    #[test]
    fn contract_requires_watching_workflows_and_looping_on_failure() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("terminal state"),
            "contract must require watching workflows to terminal state"
        );
        assert!(
            contract.contains("do not stop merely because checks are pending"),
            "contract must forbid stopping while checks are pending"
        );
        assert!(
            contract.contains("logs, remediate the failure"),
            "contract must require inspecting logs and remediating failures"
        );
        assert!(
            contract.contains("watch again"),
            "contract must require re-watching after a fix"
        );
    }

    /// Acceptance: the contract must require collecting automated reviews and
    /// ordinary inline review threads.
    #[test]
    fn contract_requires_collecting_reviews_and_automation_comments() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("inline review threads"),
            "contract must require collecting inline review threads"
        );
        assert!(
            contract.contains("automation"),
            "contract must require collecting automation comments"
        );
        assert!(
            contract.contains("Open Code Review"),
            "contract must name OCR (Open Code Review) findings"
        );
        assert!(
            contract.contains("CodeRabbit"),
            "contract must name CodeRabbit findings"
        );
    }

    /// Acceptance: the contract must require replying in-thread and resolving
    /// addressed review threads.
    #[test]
    fn contract_requires_in_thread_replies_and_resolving_threads() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("Reply in the corresponding review thread"),
            "contract must require replying in-thread"
        );
        assert!(
            contract.contains("resolve addressed threads"),
            "contract must require resolving addressed threads where supported"
        );
    }

    /// Acceptance: the contract must require repeating until checks pass and
    /// actionable review feedback is exhausted.
    #[test]
    fn contract_requires_repeat_until_exhausted() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("Repeat the checks / reviews / fix / reply cycle"),
            "contract must require repeating the cycle"
        );
        assert!(
            contract.contains("no actionable unresolved review threads remain"),
            "contract must require exhausting actionable review feedback"
        );
    }

    /// Acceptance: the contract must call for the *complete* verification
    /// suite, not a subset.
    #[test]
    fn contract_requires_complete_verification_suite() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("complete verification suite"),
            "contract must require the complete verification suite"
        );
        assert!(
            contract.contains("not merely a convenient subset"),
            "contract must forbid running only a subset"
        );
    }

    /// Acceptance: the contract must require a test-first implementation plan.
    #[test]
    fn contract_requires_test_first_plan() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("test-first implementation plan"),
            "contract must require a test-first implementation plan"
        );
    }

    /// The contract is deterministic: two calls produce identical bytes. This
    /// is what makes the behavior identical across fresh launches, retries,
    /// relaunches, and restored workflows.
    #[test]
    fn contract_is_deterministic() {
        let a = issue_delivery_contract();
        let b = issue_delivery_contract();
        assert_eq!(a, b, "contract must be byte-identical across calls");
    }

    /// The contract must preserve unrelated/agent-configuration files when
    /// starting from the base branch.
    #[test]
    fn contract_preserves_unrelated_and_config_files() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("Do not delete unrelated files"),
            "contract must forbid deleting unrelated or config files"
        );
    }

    /// The contract must handle the external-blocker case explicitly rather
    /// than stopping silently.
    #[test]
    fn contract_reports_external_blockers() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("genuine external blocker"),
            "contract must instruct reporting a genuine external blocker"
        );
    }

    /// The contract must require re-issuing a watch/poll command when it times
    /// out while workflows are still pending, rather than returning control
    /// before the pull request is green (reference behavior step 14).
    #[test]
    fn contract_requires_re_polling_on_watch_timeout() {
        let contract = issue_delivery_contract();
        assert!(
            contract.contains("times out"),
            "contract must address a watch/poll command timing out"
        );
        assert!(
            contract.contains("invoke it again"),
            "contract must require re-issuing the watch/poll command on timeout"
        );
        assert!(
            contract.contains("before the pull request is green"),
            "contract must forbid returning before the pull request is green"
        );
    }
}
