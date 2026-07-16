//! Public behavior tests for git repository parsing, status, and repository probing.

#[path = "git_info/dirty_status.rs"]
mod dirty_status;
#[path = "git_info/parsing.rs"]
mod parsing;
#[path = "git_info/real_repository.rs"]
mod real_repository;
#[path = "git_info/support.rs"]
mod support;
