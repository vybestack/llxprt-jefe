//! Issue-draft rewrite instruction construction (issue #214 / #359).
//!
//! Pure helper that builds the natural-language instruction handed to the
//! configured default agent when the user asks it to rewrite a new-issue
//! draft non-interactively. The instruction tells the agent to write the
//! rewritten issue to a known output path so thinking/tool/session noise on
//! stdout cannot pollute the draft (issue #359).
//!
//! No I/O lives here — the function is fully deterministic and unit-tested.

use std::fmt::Write as _;
use std::path::Path;

/// Build the non-interactive rewrite instruction for a new-issue draft.
///
/// `draft` is the raw composer text (first line treated as the title).
/// `github_repo`, when provided (e.g. `"owner/repo"`), tells the agent which
/// repository's source to study so the rewrite is grounded in the actual
/// codebase. When `None`, the agent rewrites from the draft alone.
/// `output_path` is the absolute file the agent must overwrite with ONLY the
/// rewritten issue text (title on the first line, body after).
#[must_use]
pub fn build_rewrite_instruction(
    draft: &str,
    github_repo: Option<&str>,
    output_path: &Path,
) -> String {
    let mut out = String::new();
    out.push_str("You are rewriting a GitHub issue draft to make it clearer, ");
    out.push_str("more complete, and better structured.\n\n");
    out.push_str("Improve the title to be concise and descriptive. Expand the body ");
    out.push_str("with a clear problem statement, relevant context, and concrete ");
    out.push_str("acceptance criteria where useful. Fix spelling and grammar. ");
    out.push_str("Preserve all of the author's original intent and technical detail; ");
    out.push_str("do not invent requirements that are not implied by the draft.\n\n");
    if let Some(repo) = github_repo.filter(|repo| !repo.trim().is_empty()) {
        let _ = write!(
            out,
            "This issue is for the repository {repo}. Study the source code in the \
             current working directory to ground the rewrite in the real codebase.\n\n"
        );
    }
    let _ = write!(
        out,
        "Write ONLY the rewritten issue text to this file (overwrite it completely):\n\
         {}\n\n\
         The FIRST line in that file MUST be the issue title and every following line \
         MUST be the body. Do not write thinking, tool output, session metadata, \
         commentary, explanations, markdown code fences, or labels to the file. \
         Do not rely on stdout for the issue text — the file is the only handoff.\n\n",
        output_path.display()
    );
    out.push_str("Draft to rewrite:\n\n");
    out.push_str(draft.trim_end());
    out
}

#[cfg(test)]
#[path = "issue_rewrite_tests.rs"]
mod tests;
