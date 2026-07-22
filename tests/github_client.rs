//! GitHub client behavioral tests moved out of the lib target to stay under
//! the Clippy `large_stack_arrays` test-descriptor ceiling (issue #307).
//!
//! These integration tests exercise public `jefe::github` parsing, query
//! builders, and property-edit helpers against fixture JSON — the same
//! coverage previously registered on the lib test target via `#[path]` mods
//! in `src/lib.rs`.

#[path = "github_client/actions.rs"]
mod actions;
#[path = "github_client/auth_device.rs"]
mod auth_device;
#[path = "github_client/create_issue.rs"]
mod create_issue;
#[path = "github_client/issues.rs"]
mod issues;
#[path = "github_client/state_reason.rs"]
mod state_reason;
#[path = "github_client/tests_filters.rs"]
mod tests_filters;
#[path = "github_client/tests_pr.rs"]
mod tests_pr;
#[path = "github_client/tests_pr_detail.rs"]
mod tests_pr_detail;
#[path = "github_client/tests_pr_sort_reviews.rs"]
mod tests_pr_sort_reviews;
#[path = "github_client/tests_pr_threads.rs"]
mod tests_pr_threads;
#[path = "github_client/tests_timestamp_sort.rs"]
mod tests_timestamp_sort;
