//! Issue-draft rewrite instruction construction (issue #214).
//!
//! Pure helper that builds the natural-language instruction handed to the
//! configured default agent when the user asks it to rewrite a new-issue
//! draft non-interactively. The instruction is deliberately output-constrained
//! so the captured stdout can replace the composer text verbatim: the first
//! line is the issue title and the remaining lines are the body, with no
//! surrounding prose or code fences.
//!
//! No I/O lives here — the function is fully deterministic and unit-tested.

/// Build the non-interactive rewrite instruction for a new-issue draft.
///
/// `draft` is the raw composer text (first line treated as the title).
/// `github_repo`, when provided (e.g. `"owner/repo"`), tells the agent which
/// repository's source to study so the rewrite is grounded in the actual
/// codebase. When `None`, the agent rewrites from the draft alone.
#[must_use]
pub fn build_rewrite_instruction(draft: &str, github_repo: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("You are rewriting a GitHub issue draft to make it clearer, ");
    out.push_str("more complete, and better structured.\n\n");
    out.push_str("Improve the title to be concise and descriptive. Expand the body ");
    out.push_str("with a clear problem statement, relevant context, and concrete ");
    out.push_str("acceptance criteria where useful. Fix spelling and grammar. ");
    out.push_str("Preserve all of the author's original intent and technical detail; ");
    out.push_str("do not invent requirements that are not implied by the draft.\n\n");
    if let Some(repo) = github_repo.filter(|repo| !repo.trim().is_empty()) {
        use std::fmt::Write as _;
        let _ = write!(
            out,
            "This issue is for the repository {repo}. Study the source code in the \
             current working directory to ground the rewrite in the real codebase.\n\n"
        );
    }
    out.push_str("Output ONLY the rewritten issue. The FIRST line MUST be the issue ");
    out.push_str("title and every following line MUST be the body. Do not include ");
    out.push_str("any commentary, explanations, markdown code fences, or labels ");
    out.push_str("before or after the issue text.\n\n");
    out.push_str("Draft to rewrite:\n\n");
    out.push_str(draft.trim_end());
    out
}

