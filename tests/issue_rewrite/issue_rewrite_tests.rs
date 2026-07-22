//! Tests for the issue-draft rewrite instruction builder (issue #214).

use jefe::domain::build_rewrite_instruction;

#[test]
fn instruction_includes_the_draft_verbatim() {
    let draft = "Fix login bug\nThe login button does nothing on Safari.";
    let instruction = build_rewrite_instruction(draft, None);
    assert!(
        instruction.contains(draft),
        "instruction must embed the draft"
    );
}

#[test]
fn instruction_trims_trailing_whitespace_from_draft() {
    let instruction = build_rewrite_instruction("Title\nBody   \n\n", None);
    assert!(
        !instruction.ends_with("\n\n\n"),
        "trailing blank lines are trimmed"
    );
}

#[test]
fn instruction_constrains_output_format() {
    let instruction = build_rewrite_instruction("t", None);
    assert!(
        instruction.contains("FIRST line MUST be the issue title"),
        "must constrain the output to title-then-body"
    );
    assert!(
        instruction.contains("ONLY"),
        "must forbid surrounding commentary"
    );
}

#[test]
fn instruction_references_repo_when_provided() {
    let instruction = build_rewrite_instruction("t", Some("vybestack/llxprt-jefe"));
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
    let instruction = build_rewrite_instruction("t", None);
    assert!(
        !instruction.contains("source code in the"),
        "must not reference an unknown repository"
    );
}

#[test]
fn instruction_omits_repo_section_when_blank() {
    let instruction = build_rewrite_instruction("t", Some("   "));
    assert!(
        !instruction.contains("source code in the"),
        "a blank repo must be treated as absent"
    );
}
