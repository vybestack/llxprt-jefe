//! Process identity tests moved out of the lib target (issue #307).

#[path = "identity/build_script_fast_forward.rs"]
mod build_script_fast_forward;
#[path = "../build_support/git_watch.rs"]
mod git_watch;
#[path = "identity/identity_tests.rs"]
mod identity_tests;
