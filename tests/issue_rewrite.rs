//! Issue-draft rewrite instruction tests moved out of the lib target to stay
//! under the Clippy `large_stack_arrays` test-descriptor ceiling (issue #307).

#[path = "issue_rewrite/issue_rewrite_tests.rs"]
mod issue_rewrite_tests;
