//! Tests for the issue-draft rewrite instruction builder (issue #214 / #359).

use jefe::domain::build_rewrite_instruction;
use std::path::Path;

fn out_path() -> &'static Path {
    Path::new("/tmp/jefe-rewrite-out.md")
}

#[test]
fn instruction_includes_the_draft_verbatim() {
    let draft = "Fix login bug\nThe login button does nothing on Safari.";
    let instruction = build_rewrite_instruction(draft, None, out_path());
    assert!(
        instruction.contains(draft),
        "instruction must embed the draft"
    );
}

#[test]
fn instruction_trims_trailing_whitespace_from_draft() {
    let instruction = build_rewrite_instruction("Title\nBody   \n\n", None, out_path());
    assert!(
        !instruction.ends_with("\n\n\n"),
        "trailing blank lines are trimmed"
    );
}

#[test]
fn instruction_constrains_output_to_temp_file() {
    let instruction = build_rewrite_instruction("t", None, out_path());
    assert!(
        instruction.contains(out_path().to_string_lossy().as_ref()),
        "must name the output file path"
    );
    assert!(
        instruction.contains("FIRST line in that file MUST be the issue title"),
        "must constrain the file contents to title-then-body"
    );
    assert!(
        instruction.contains("only handoff"),
        "must forbid relying on stdout for the issue text"
    );
}

#[test]
fn instruction_references_repo_when_provided() {
    let instruction = build_rewrite_instruction("t", Some("vybestack/llxprt-jefe"), out_path());
    assert!(
        instruction.contains("vybestack/llxprt-jefe"),
        "must name the repository so the agent studies the right source"
    );
    assert!(
        instruction.contains("source code"),
        "must direct the agent to study the source"
    );
}

#[test]
fn instruction_omits_repo_section_when_none() {
    let instruction = build_rewrite_instruction("t", None, out_path());
    assert!(
        !instruction.contains("source code in the"),
        "must not reference an unknown repository"
    );
}

#[test]
fn instruction_omits_repo_section_when_blank() {
    let instruction = build_rewrite_instruction("t", Some("   "), out_path());
    assert!(
        !instruction.contains("source code in the"),
        "a blank repo must be treated as absent"
    );
}
