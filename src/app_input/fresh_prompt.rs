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

/// Transform a base signature into a fresh, non-resuming prompt launch.
#[must_use]
pub(super) fn prepare_fresh_prompt_signature(
    mut sig: LaunchSignature,
    prompt_kind: FreshPromptKind,
    prompt_relative_path: &str,
) -> LaunchSignature {
    sig.pass_continue = false;
    let instruction = format!(
        "Read and work on the GitHub {} described in {prompt_relative_path}",
        prompt_kind.label()
    );
    sig.mode_flags = match sig.agent_kind {
        AgentKind::Llxprt => vec!["-i".to_owned(), instruction],
        AgentKind::CodePuppy => vec![instruction],
    };
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
    fn llxprt_issue_uses_fresh_issue_instruction() {
        let result = prepare_fresh_prompt_signature(
            base_sig(AgentKind::Llxprt),
            FreshPromptKind::Issue,
            ".jefe/issue-prompt.md",
        );
        assert!(!result.pass_continue);
        assert_eq!(
            result.mode_flags,
            vec![
                "-i",
                "Read and work on the GitHub issue described in .jefe/issue-prompt.md"
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
            vec!["Read and work on the GitHub issue described in .jefe/issue-prompt.md"]
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
        assert_eq!(
            result.mode_flags,
            vec![
                "-i",
                "Read and work on the GitHub PR described in .jefe/pr-prompt.md"
            ]
        );
    }
}
