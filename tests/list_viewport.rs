//! List viewport and text-fitting tests moved out of the lib target to stay
//! under the Clippy `large_stack_arrays` test-descriptor ceiling (issue #307).

#[path = "list_viewport/list_viewport_tests.rs"]
mod list_viewport_tests;
