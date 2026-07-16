//! Shared kind-specific fresh-prompt launch-signature construction.

use jefe::domain::{AgentKind, LaunchSignature};

/// Workflow represented by a prompt file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FreshPromptKind {
    Issue,
    PullRequest,
}

impl FreshPromptKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PullRequest => "PR",
        }
    }
}

/// Runtime-neutral delivery contract appended to every fresh Send Issue
/// instruction. Issue-specific content remains in the prompt file; this text
/// defines how an agent must carry that issue through review and CI.
const ISSUE_DELIVERY_WORKFLOW: &str = concat!(
    "Follow the canonical bounded issue-delivery policy in ",
    "dev-docs/workflow/ISSUE-DELIVERY.md. Before implementation, shape a decision-complete ",
    "acceptance matrix and record explicit non-goals, bounded vertical slices, expected paths, and a ",
    "scope ledger. Agents must stop for approval before adding an unplanned subsystem or public ",
    "abstraction, ",
    "making a workflow, agent-memory, quality-tool, or dependency change, moving an unrelated ",
    "refactor or test move into scope, implementing behavior outside the acceptance matrix, or ",
    "exceeding the hard scope budget. Target no more than 25 files or 1,500 net changed lines, perform ",
    "a mandatory scope review above either threshold, and stop without approval above 40 files or ",
    "2,500 net changed lines. Classify every review finding as Blocker-Fix, In-scope-Fix, Reject, or ",
    "Defer; reviewer suggestions do not authorize scope expansion. Limit Open Code Review to two ",
    "local and two PR OCR reviews per issue/PR effort. Declare exact-head completion only when every ",
    "accepted behavior has behavioral evidence, local verification and CI pass on the candidate head, ",
    "reviews are complete and triaged, all Blocker-Fix and In-scope-Fix findings are resolved, correct ",
    "ancestry is confirmed, the PR is conflict-free, and the scope ",
    "ledger is clean. Stop successfully when accepted behavior and all required gates are complete. ",
    "Do not continue optional hardening or cleanup, and do not weaken architecture, TDD, lint, ",
    "complexity, source-size, safety, coverage, cross-platform, or CI requirements."
);

fn fresh_prompt_instruction(prompt_kind: FreshPromptKind, prompt_relative_path: &str) -> String {
    let base = format!(
        "Read and work on the GitHub {} described in {prompt_relative_path}",
        prompt_kind.label()
    );
    match prompt_kind {
        FreshPromptKind::Issue => format!("{base}. {ISSUE_DELIVERY_WORKFLOW}"),
        FreshPromptKind::PullRequest => base,
    }
}

/// Transform a base signature into a fresh, non-resuming prompt launch.
#[must_use]
pub(super) fn prepare_fresh_prompt_signature(
    mut sig: LaunchSignature,
    prompt_kind: FreshPromptKind,
    prompt_relative_path: &str,
) -> LaunchSignature {
    sig.pass_continue = false;
    sig.code_puppy_quick_resume = false;
    let instruction = fresh_prompt_instruction(prompt_kind, prompt_relative_path);
    match sig.agent_kind {
        // LLxprt keeps the agent's persisted mode flags (e.g. `--yolo`) and
        // appends the fresh instruction. Replacing them here dropped `--yolo`,
        // starting every issue/PR-driven LLxprt session in non-yolo mode (#210).
        //
        // `--continue` is stripped because continuation is owned by
        // `pass_continue`, which fresh prompts force off: a stale persisted
        // `--continue` must not resume a prior session on a fresh launch.
        AgentKind::Llxprt => {
            sig.mode_flags.retain(|flag| flag != "--continue");
            sig.mode_flags.push("-i".to_owned());
            sig.mode_flags.push(instruction);
        }
        // CodePuppy does not consume any LLxprt flags (#184): the instruction
        // is the sole positional argument.
        AgentKind::CodePuppy => sig.mode_flags = vec![instruction],
    }
    sig
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{RemoteRepositorySettings, SandboxEngine};
    use std::path::PathBuf;

    fn base_sig(kind: AgentKind) -> LaunchSignature {
        LaunchSignature {
            work_dir: PathBuf::from("/tmp/work"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: vec!["--stale".to_owned()],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: kind,
            llxprt_version: None,
        }
    }

    #[test]
    fn issue_delivery_workflow_references_policy_and_shapes_accepted_scope() {
        for required in [
            "dev-docs/workflow/ISSUE-DELIVERY.md",
            "decision-complete acceptance matrix",
            "explicit non-goals",
            "bounded vertical slices",
            "expected paths",
            "scope ledger",
        ] {
            assert!(
                ISSUE_DELIVERY_WORKFLOW.contains(required),
                "issue delivery workflow must require {required}"
            );
        }
        assert!(!ISSUE_DELIVERY_WORKFLOW.contains("CodeRabbit"));
    }

    #[test]
    fn issue_delivery_workflow_stops_unplanned_scope_expansion() {
        for required in [
            "stop for approval",
            "unplanned subsystem",
            "public abstraction",
            "workflow, agent-memory, quality-tool, or dependency change",
            "unrelated refactor or test move",
            "behavior outside the acceptance matrix",
            "25 files or 1,500 net changed lines",
            "40 files or 2,500 net changed lines",
        ] {
            assert!(
                ISSUE_DELIVERY_WORKFLOW.contains(required),
                "issue delivery workflow must include scope guardrail: {required}"
            );
        }
    }

    #[test]
    fn issue_delivery_workflow_bounds_and_triages_review() {
        for required in [
            "Blocker-Fix",
            "In-scope-Fix",
            "Reject",
            "Defer",
            "two local and two PR OCR reviews",
        ] {
            assert!(
                ISSUE_DELIVERY_WORKFLOW.contains(required),
                "issue delivery workflow must include review rule: {required}"
            );
        }
        assert!(!ISSUE_DELIVERY_WORKFLOW.contains("address every actionable finding"));
    }

    #[test]
    fn issue_delivery_workflow_defines_exact_head_success() {
        for required in [
            "exact-head",
            "behavioral evidence",
            "local verification",
            "CI",
            "reviews are complete and triaged",
            "Blocker-Fix and In-scope-Fix findings are resolved",
            "correct ancestry",
            "conflict-free",
            "scope ledger is clean",
            "Stop successfully",
            "Do not continue optional hardening",
        ] {
            assert!(
                ISSUE_DELIVERY_WORKFLOW.contains(required),
                "issue delivery workflow must include completion rule: {required}"
            );
        }
    }

    #[test]
    fn llxprt_and_code_puppy_project_the_identical_issue_contract() {
        let llxprt = prepare_fresh_prompt_signature(
            base_sig(AgentKind::Llxprt),
            FreshPromptKind::Issue,
            ".jefe/issue-prompt.md",
        );
        let code_puppy = prepare_fresh_prompt_signature(
            base_sig(AgentKind::CodePuppy),
            FreshPromptKind::Issue,
            ".jefe/issue-prompt.md",
        );

        assert_eq!(llxprt.mode_flags.last(), code_puppy.mode_flags.first());
        assert_eq!(llxprt.mode_flags[llxprt.mode_flags.len() - 2], "-i");
        assert_eq!(code_puppy.mode_flags.len(), 1);
    }

    #[test]
    fn llxprt_issue_uses_fresh_issue_instruction() {
        let result = prepare_fresh_prompt_signature(
            base_sig(AgentKind::Llxprt),
            FreshPromptKind::Issue,
            ".jefe/issue-prompt.md",
        );
        assert!(!result.pass_continue);
        // Persisted mode flags are preserved and the instruction is appended.
        assert_eq!(
            result.mode_flags,
            vec![
                "--stale".to_owned(),
                "-i".to_owned(),
                fresh_prompt_instruction(FreshPromptKind::Issue, ".jefe/issue-prompt.md")
            ]
        );
    }

    #[test]
    fn code_puppy_pr_uses_only_fresh_pr_instruction() {
        let result = prepare_fresh_prompt_signature(
            base_sig(AgentKind::CodePuppy),
            FreshPromptKind::PullRequest,
            ".jefe/pr-prompt.md",
        );
        assert!(!result.pass_continue);
        assert_eq!(
            result.mode_flags,
            vec!["Read and work on the GitHub PR described in .jefe/pr-prompt.md"]
        );
    }

    #[test]
    fn code_puppy_issue_uses_only_fresh_issue_instruction() {
        let result = prepare_fresh_prompt_signature(
            base_sig(AgentKind::CodePuppy),
            FreshPromptKind::Issue,
            ".jefe/issue-prompt.md",
        );
        assert!(!result.pass_continue);
        assert_eq!(
            result.mode_flags,
            vec![fresh_prompt_instruction(
                FreshPromptKind::Issue,
                ".jefe/issue-prompt.md"
            )]
        );
    }

    #[test]
    fn llxprt_preserves_existing_mode_flags_including_yolo() {
        // Regression for issue #210: the fresh-prompt path for LLxprt agents
        // must not drop the agent's persisted mode flags (e.g. `--yolo`).
        // Appending the instruction while keeping `--yolo` is what makes the
        // fresh session launch in yolo mode, matching a normal launch.
        let mut sig = base_sig(AgentKind::Llxprt);
        sig.mode_flags = vec!["--yolo".to_owned()];
        let result =
            prepare_fresh_prompt_signature(sig, FreshPromptKind::Issue, ".jefe/issue-prompt.md");
        assert!(!result.pass_continue);
        assert_eq!(
            result.mode_flags,
            vec![
                "--yolo".to_owned(),
                "-i".to_owned(),
                fresh_prompt_instruction(FreshPromptKind::Issue, ".jefe/issue-prompt.md")
            ]
        );
    }

    #[test]
    fn llxprt_fresh_prompt_does_not_add_yolo_to_empty_mode() {
        // The other half of #210: an agent whose mode was cleared (non-yolo)
        // must stay non-yolo on a fresh-prompt launch. --yolo is never
        // synthesized here; only the instruction is appended.
        let mut sig = base_sig(AgentKind::Llxprt);
        sig.mode_flags.clear();
        let result =
            prepare_fresh_prompt_signature(sig, FreshPromptKind::Issue, ".jefe/issue-prompt.md");
        assert!(!result.pass_continue);
        assert_eq!(
            result.mode_flags,
            vec![
                "-i".to_owned(),
                fresh_prompt_instruction(FreshPromptKind::Issue, ".jefe/issue-prompt.md")
            ]
        );
    }

    #[test]
    fn llxprt_fresh_prompt_strips_persisted_continue() {
        // Fresh launches must never resume a prior session. `--continue` is
        // owned by `pass_continue` (forced off here), so a stale persisted
        // `--continue` in the mode string must be dropped, not forwarded.
        let mut sig = base_sig(AgentKind::Llxprt);
        sig.mode_flags = vec!["--yolo".to_owned(), "--continue".to_owned()];
        let result =
            prepare_fresh_prompt_signature(sig, FreshPromptKind::PullRequest, ".jefe/pr-prompt.md");
        assert!(!result.pass_continue);
        assert_eq!(
            result.mode_flags,
            vec![
                "--yolo",
                "-i",
                "Read and work on the GitHub PR described in .jefe/pr-prompt.md"
            ]
        );
    }

    #[test]
    fn llxprt_pr_uses_fresh_pr_instruction() {
        let result = prepare_fresh_prompt_signature(
            base_sig(AgentKind::Llxprt),
            FreshPromptKind::PullRequest,
            ".jefe/pr-prompt.md",
        );
        assert!(!result.pass_continue);
        // Persisted mode flags are preserved and the instruction is appended.
        assert_eq!(
            result.mode_flags,
            vec![
                "--stale",
                "-i",
                "Read and work on the GitHub PR described in .jefe/pr-prompt.md"
            ]
        );
        issue_and_pr_fresh_signatures_retain_exact_selector();
    }

    fn issue_and_pr_fresh_signatures_retain_exact_selector() {
        let selector =
            jefe::domain::LlxprtNpmPackageSelector::normalize("0.10.0-nightly.260712.21cb698b6");
        for (kind, path) in [
            (FreshPromptKind::Issue, ".jefe/issue-prompt.md"),
            (FreshPromptKind::PullRequest, ".jefe/pr-prompt.md"),
        ] {
            let mut signature = base_sig(AgentKind::Llxprt);
            signature.llxprt_version = selector.clone();
            signature.code_puppy_version = "0.0.361-rc1".to_owned();
            let prepared = prepare_fresh_prompt_signature(signature, kind, path);
            assert_eq!(prepared.llxprt_version, selector);
            assert_eq!(prepared.code_puppy_version, "0.0.361-rc1");
        }
    }
}
