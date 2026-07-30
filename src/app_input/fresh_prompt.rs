//! Shared kind-specific fresh-prompt launch-signature construction.

use jefe::domain::{AgentLaunchRequest, Id, TypedValue};

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

/// Runtime-neutral, repo-neutral delivery contract appended to every fresh
/// Send Issue instruction. Issue-specific content remains in the prompt file;
/// this text defines how an agent must carry that issue through review and CI.
///
/// It is deliberately self-contained: it references no project-specific file
/// (the canonical process lives in jefe's own `dev-docs/workflow/`, which does
/// not exist in target repositories) and uses scope adherence rather than
/// hard-coded file/line counts as the scope guardrail.
pub(super) const ISSUE_DELIVERY_WORKFLOW: &str = concat!(
    "Before implementing, shape the issue into clear acceptance criteria: the ",
    "behavior to deliver, the relevant inputs and boundary cases, and the tests ",
    "that will prove it. Implement only the accepted behavior and keep every change ",
    "strictly within the issue's scope; do not expand scope to add adjacent cleanup ",
    "or speculative hardening, and stop for approval before adding an unplanned ",
    "subsystem or public abstraction, making a workflow, agent-memory, quality-tool, ",
    "or dependency change, moving an unrelated refactor or test into scope, or ",
    "implementing behavior outside the issue's scope. Classify every review finding ",
    "as Blocker-Fix, In-scope-Fix, Reject, or Defer; reviewer suggestions do not ",
    "authorize scope expansion. Limit Open Code Review to two local and two PR OCR ",
    "reviews per issue/PR effort. Declare completion only when every accepted behavior ",
    "has behavioral evidence, local verification and CI pass on the candidate head, ",
    "reviews are complete and triaged, all Blocker-Fix and In-scope-Fix findings are ",
    "resolved, and the PR is conflict-free with correct ancestry. Stop successfully ",
    "when accepted behavior is complete and all required gates pass. Do not continue ",
    "optional hardening or cleanup, and do not weaken architecture, TDD, lint, ",
    "complexity, source-size, safety, coverage, cross-platform, or CI requirements."
);

pub(super) fn fresh_prompt_instruction(
    prompt_kind: FreshPromptKind,
    prompt_content: &str,
) -> String {
    let truncated = truncate_prompt_content(prompt_content);
    let label = prompt_kind.label();
    let base = format!("Read and work on the following GitHub {label}.\n\n{truncated}");
    match prompt_kind {
        FreshPromptKind::Issue => format!("{base}\n\n{ISSUE_DELIVERY_WORKFLOW}"),
        FreshPromptKind::PullRequest => base,
    }
}

/// Maximum prompt content length (in bytes) before truncation.
///
/// Cross-platform safe bound that stays well under the smallest OS
/// command-line length limit (Windows CreateProcess: ~32 KB). The
/// `ISSUE_DELIVERY_WORKFLOW` appendix adds ~1.3 KB for issues.
///
/// Note: the binding constraint on Unix is **tmux's pane-command limit**
/// (~16,340 bytes on tmux 3.x), not the OS `ARG_MAX`. Prompt content that
/// exceeds [`PROMPT_COMPACTION_THRESHOLD_BYTES`] is compacted to a preview +
/// `gh` fetch reference well before this ceiling, so truncation is a
/// last-resort safety net only (issue #409).
pub(super) const MAX_PROMPT_CONTENT_BYTES: usize = 24_000;

/// Prompt content length (in bytes) above which the body is compacted to a
/// short preview + `gh issue/pr view --comments` fetch reference instead of
/// being inlined verbatim.
///
/// Sized so that the compacted prompt — including metadata, base prompt, the
/// `ISSUE_DELIVERY_WORKFLOW` appendix (~1.3 KB for issues), and the
/// instruction framing — stays comfortably under tmux's pane-command limit
/// ([`TMUX_PANE_COMMAND_LIMIT_BYTES`]).
///
/// The agent runs in a checked-out git repo with `gh` available and
/// authenticated, so it can fetch the full live issue/PR content itself —
/// strictly better than a truncated copy (issue #409).
pub(super) const PROMPT_COMPACTION_THRESHOLD_BYTES: usize = 10_000;

/// Measured tmux pane-command length limit on tmux 3.7b (macOS): 16,330 bytes
/// OK, 16,340 bytes TOO LONG. We use 16,000 as a conservative safety margin.
///
/// This is the binding constraint on Unix — not the OS `ARG_MAX` (macOS:
/// 1,048,576). tmux imposes this internal limit on the total pane command
/// string length, which includes env-scrub prefix, executable, mode flags,
/// and the inlined prompt instruction.
///
/// Only referenced by tests (the production compaction threshold is sized
/// with enough headroom that this limit is never checked at runtime).
#[cfg(test)]
pub(super) const TMUX_PANE_COMMAND_LIMIT_BYTES: usize = 16_000;

/// Maximum number of bytes of preview content to show before the fetch
/// reference in a compacted prompt.
const COMPACTION_PREVIEW_BYTES: usize = 2_000;

/// Compact prompt content that exceeds [`PROMPT_COMPACTION_THRESHOLD_BYTES`]
/// to a short preview + a `gh` fetch instruction, so the full prompt stays
/// within tmux's pane-command length limit.
///
/// The agent runs `gh` natively in the checked-out repo, so the `fetch_command`
/// tells it exactly how to retrieve the full, live content. Content at or
/// below the threshold passes through unchanged.
///
/// # Caller responsibility
///
/// The `fetch_command` is interpolated verbatim into the prompt text the agent
/// reads. Callers MUST construct it from validated components only: the
/// `repository` should be a GitHub-validated `owner/repo` slug (enforced by
/// [`GitHubRepoRef::parse`](crate::domain::GitHubRepoRef::parse)) and the
/// number must be an integer. Never pass raw user/editor input as
/// `fetch_command`.
///
/// Example: `gh issue view 42 --repo owner/repo --comments`.
#[must_use]
pub(super) fn compact_prompt_content(content: &str, fetch_command: &str) -> String {
    if content.len() <= PROMPT_COMPACTION_THRESHOLD_BYTES {
        return content.to_owned();
    }
    // Find the last char boundary at or before the preview limit.
    let mut preview_end = COMPACTION_PREVIEW_BYTES.min(content.len());
    while preview_end > 0 && !content.is_char_boundary(preview_end) {
        preview_end -= 1;
    }
    let preview = &content[..preview_end];
    let omitted = content.len().saturating_sub(preview_end);
    format!(
        "{preview}

\
         [... {omitted} more bytes omitted — run the command below to fetch the full content ...]

\
         Fetch the full content with: {fetch_command}

\
         This compact reference was generated because the original content exceeded the \
         command-line length limit."
    )
}

/// Truncate prompt content if it exceeds the byte budget.
///
/// Truncation adds a visible `[... truncated ...]` marker so the agent knows
/// the content was cut. The cut happens at a character boundary to avoid
/// splitting multi-byte UTF-8.
///
/// This is a last-resort safety net: normal prompts are compacted by
/// [`compact_prompt_content`] at the formatter layer (issue #409) well before
/// this ceiling is reached.
fn truncate_prompt_content(content: &str) -> String {
    if content.len() <= MAX_PROMPT_CONTENT_BYTES {
        return content.to_owned();
    }
    // Find the last char boundary at or before the byte limit.
    let mut end = MAX_PROMPT_CONTENT_BYTES;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = content[..end].to_owned();
    truncated.push_str("\n\n[... prompt truncated to stay within command-line length limits ...]");
    truncated
}

/// Transform a base signature into a fresh, non-resuming prompt launch.
///
/// The full prompt content is inlined directly into the instruction string
/// (issue #315), eliminating the `.jefe/issue-prompt.md` / `.jefe/pr-prompt.md`
/// file write that previously got in the way of git operations.
#[must_use]
pub(super) fn prepare_fresh_prompt_signature(
    mut request: AgentLaunchRequest,
    prompt_kind: FreshPromptKind,
    prompt_content: &str,
) -> AgentLaunchRequest {
    request.operation = match prompt_kind {
        FreshPromptKind::Issue => jefe::domain::agent_definition::Operation::FreshIssue,
        FreshPromptKind::PullRequest => jefe::domain::agent_definition::Operation::FreshPullRequest,
    };
    if let Ok(prompt_id) = Id::parse("prompt") {
        request.values.insert(
            prompt_id,
            TypedValue::String(fresh_prompt_instruction(prompt_kind, prompt_content)),
        );
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::agent_definition::{AgentDefinition, AgentTypeId, Operation};
    use jefe::domain::canonical_values::typed_field;
    use jefe::domain::{RemoteRepositorySettings, TypedMap, TypedValue};
    use std::path::PathBuf;

    fn shipped_type(display_name: &str) -> AgentTypeId {
        AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.display_name == display_name)
            .map_or_else(
                || panic!("missing shipped definition {display_name}"),
                |definition| definition.id,
            )
    }

    fn base_request(display_name: &str) -> AgentLaunchRequest {
        AgentLaunchRequest {
            type_id: shipped_type(display_name),
            values: TypedMap::new(),
            work_dir: PathBuf::from("/tmp/work"),
            remote: RemoteRepositorySettings::default(),
            operation: Operation::Resume,
        }
    }

    fn prompt_value(request: &AgentLaunchRequest) -> &str {
        match typed_field(&request.values, "prompt") {
            Some(TypedValue::String(value)) => value,
            other => panic!("expected typed prompt string, got {other:?}"),
        }
    }

    #[test]
    fn issue_delivery_workflow_is_repo_neutral_and_scope_focused() {
        // Scope-first: the agent must shape acceptance criteria and stay in scope.
        for required in [
            "acceptance criteria",
            "Implement only the accepted behavior",
            "within the issue's scope",
        ] {
            assert!(
                ISSUE_DELIVERY_WORKFLOW.contains(required),
                "issue delivery workflow must require {required}"
            );
        }
        // Repo-neutral: no jefe-only doc path and no hard-coded file/line budgets.
        for forbidden in [
            "dev-docs/workflow/ISSUE-DELIVERY.md",
            "net changed lines",
            "hard scope budget",
        ] {
            assert!(
                !ISSUE_DELIVERY_WORKFLOW.contains(forbidden),
                "issue delivery workflow must not include jefe-only artifact: {forbidden}"
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
            "unrelated refactor or test",
            "outside the issue's scope",
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
    fn issue_delivery_workflow_defines_completion_readiness() {
        for required in [
            "behavioral evidence",
            "local verification",
            "CI",
            "candidate head",
            "reviews are complete and triaged",
            "Blocker-Fix and In-scope-Fix findings are resolved",
            "correct ancestry",
            "conflict-free",
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
    fn fresh_issue_is_declaration_driven_for_all_supported_types() {
        for display_name in ["LLxprt", "Code Puppy"] {
            let request = prepare_fresh_prompt_signature(
                base_request(display_name),
                FreshPromptKind::Issue,
                "issue content body",
            );
            assert_eq!(request.operation, Operation::FreshIssue);
            assert_eq!(
                prompt_value(&request),
                fresh_prompt_instruction(FreshPromptKind::Issue, "issue content body")
            );
        }
    }

    #[test]
    fn fresh_pr_uses_typed_prompt_and_preserves_existing_values() {
        let mut request = base_request("LLxprt");
        let yolo = Id::parse("yolo").unwrap_or_else(|error| panic!("valid yolo field: {error}"));
        request.values.insert(yolo.clone(), TypedValue::Bool(true));
        let prepared = prepare_fresh_prompt_signature(
            request,
            FreshPromptKind::PullRequest,
            "pr content body",
        );
        assert_eq!(prepared.operation, Operation::FreshPullRequest);
        assert_eq!(prepared.values.get(&yolo), Some(&TypedValue::Bool(true)));
        assert_eq!(
            prompt_value(&prepared),
            fresh_prompt_instruction(FreshPromptKind::PullRequest, "pr content body")
        );
    }

    #[test]
    fn prompt_content_is_inlined_as_one_typed_value() {
        let unique_content = "UNIQUE_MARKER_42adb: this is the prompt body";
        let result = prepare_fresh_prompt_signature(
            base_request("LLxprt"),
            FreshPromptKind::Issue,
            unique_content,
        );
        let inlined = prompt_value(&result);
        assert!(inlined.contains(unique_content));
        assert!(!inlined.contains(".jefe/"));
    }

    #[test]
    fn truncate_preserves_short_content_unchanged() {
        let short = "short prompt";
        assert_eq!(truncate_prompt_content(short), short);
    }

    #[test]
    fn truncate_adds_marker_and_stays_within_budget() {
        let large = "x".repeat(MAX_PROMPT_CONTENT_BYTES + 5000);
        let truncated = truncate_prompt_content(&large);
        assert!(truncated.len() <= MAX_PROMPT_CONTENT_BYTES + 200);
        assert!(truncated.contains("[... prompt truncated"));
    }

    #[test]
    fn truncate_cuts_at_char_boundary_for_multibyte_utf8() {
        let filler = "a".repeat(MAX_PROMPT_CONTENT_BYTES - 2);
        let content = format!("{filler}🎉🎉");
        let truncated = truncate_prompt_content(&content);
        assert!(truncated.contains('a'));
        assert!(truncated.contains("[... prompt truncated"));
    }

    #[test]
    fn adversarial_shell_metacharacters_remain_data() {
        let adversarial = "'; rm -rf /; echo '\n$(whoami)\n`backtick`";
        let result = prepare_fresh_prompt_signature(
            base_request("Code Puppy"),
            FreshPromptKind::Issue,
            adversarial,
        );
        let prompt = prompt_value(&result);
        assert!(prompt.contains("rm -rf"));
        assert!(prompt.contains("whoami"));
        assert!(prompt.contains("backtick"));
    }
}
