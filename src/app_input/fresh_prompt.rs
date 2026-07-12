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
const ISSUE_DELIVERY_WORKFLOW: &str = "Complete the issue end to end: start from the latest base branch without deleting unrelated or agent-configuration files; create a dedicated issue branch before changing code; use gh to fetch the issue and all comments; research the codebase and make a test-first implementation plan; implement the change and run the repository's complete verification suite; commit and push the branch; create a detailed pull request linked to the issue; watch every pull-request workflow until terminal completion, continuing to poll with a bounded delay even when a watch command or shell invocation times out; inspect failed workflow logs, fix failures, rerun verification, commit, push, and watch again; collect all project review feedback, including ordinary reviews, inline threads, and automated review comments; address every actionable finding, reply in the corresponding review thread with the fix or why it does not apply, and resolve addressed threads where supported; repeat the checks, reviews, fixes, replies, and workflow watches until all required checks pass and no actionable unresolved review feedback remains. Do not return merely because workflows are pending; report only completion or a genuine external blocker.";

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
            code_puppy_yolo: Some(false),
            mode_flags: vec!["--stale".to_owned()],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: String::new(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: kind,
        }
    }

    #[test]
    fn issue_delivery_workflow_is_exact_and_runtime_neutral() {
        let expected = concat!(
            "Read and work on the GitHub issue described in .jefe/issue-prompt.md. ",
            "Complete the issue end to end: start from the latest base branch without deleting unrelated or agent-configuration files; create a dedicated issue branch before changing code; use gh to fetch the issue and all comments; research the codebase and make a test-first implementation plan; implement the change and run the repository's complete verification suite; commit and push the branch; create a detailed pull request linked to the issue; watch every pull-request workflow until terminal completion, continuing to poll with a bounded delay even when a watch command or shell invocation times out; inspect failed workflow logs, fix failures, rerun verification, commit, push, and watch again; collect all project review feedback, including ordinary reviews, inline threads, and automated review comments; address every actionable finding, reply in the corresponding review thread with the fix or why it does not apply, and resolve addressed threads where supported; repeat the checks, reviews, fixes, replies, and workflow watches until all required checks pass and no actionable unresolved review feedback remains. Do not return merely because workflows are pending; report only completion or a genuine external blocker."
        );

        assert_eq!(
            fresh_prompt_instruction(FreshPromptKind::Issue, ".jefe/issue-prompt.md"),
            expected
        );
        assert!(!ISSUE_DELIVERY_WORKFLOW.contains("OCR"));
        assert!(!ISSUE_DELIVERY_WORKFLOW.contains("CodeRabbit"));
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
    }
}
