// Test-module declarations lifted out of `mod.rs`, which sits at the
// source-size gate.
//
// Included textually rather than declared as a child module: the files
// these point at use `use super::...` to reach `crate::state`, so nesting
// them one level deeper would break every one of those imports.

#[cfg(test)]
#[path = "auth_ops_tests.rs"]
mod auth_ops_tests;
#[cfg(test)]
mod confirm_focus_tests;
#[cfg(test)]
mod errors_tests;
#[cfg(test)]
#[path = "form_home_end_tests.rs"]
mod form_home_end_tests;
#[cfg(test)]
#[path = "issues_tests_home_end.rs"]
mod issues_home_end_tests;
#[cfg(test)]
mod issues_test_fixtures;
#[cfg(test)]
#[path = "issues_tests.rs"]
mod issues_tests;
#[cfg(test)]
#[path = "issues_tests_close_delete.rs"]
mod issues_tests_close_delete;
#[cfg(test)]
#[path = "issues_tests_close_reason.rs"]
mod issues_tests_close_reason;
#[cfg(test)]
#[path = "issues_tests_components.rs"]
mod issues_tests_components;
#[cfg(test)]
#[path = "issues_tests_composer_focus.rs"]
mod issues_tests_composer_focus;
#[cfg(test)]
mod issues_tests_create;
#[cfg(test)]
#[path = "issues_tests_detail.rs"]
mod issues_tests_detail;
#[cfg(test)]
mod issues_tests_detail_content;
#[cfg(test)]
#[path = "issues_tests_detail_flow.rs"]
mod issues_tests_detail_flow;
#[cfg(test)]
#[path = "issues_tests_detail_nav.rs"]
mod issues_tests_detail_nav;
#[cfg(test)]
#[path = "issues_tests_esc.rs"]
mod issues_tests_esc;
#[cfg(test)]
#[path = "issues_tests_filter.rs"]
mod issues_tests_filter;
#[cfg(test)]
#[path = "issues_tests_inline_cursor.rs"]
mod issues_tests_inline_cursor;
#[cfg(test)]
#[path = "issues_tests_mutations.rs"]
mod issues_tests_mutations;
#[cfg(test)]
#[path = "issues_tests_repo_nav.rs"]
mod issues_tests_repo_nav;
#[cfg(test)]
#[path = "issues_tests_self_assignment.rs"]
mod issues_tests_self_assignment;
#[cfg(test)]
#[path = "issues_tests_send_agent_probe.rs"]
mod issues_tests_send_agent_probe;
#[cfg(test)]
#[path = "issues_tests_send_to_agent.rs"]
mod issues_tests_send_to_agent;
#[cfg(test)]
#[path = "issues_tests_sort.rs"]
mod issues_tests_sort;
#[cfg(test)]
#[path = "issues_tests_subfocus.rs"]
mod issues_tests_subfocus;
#[cfg(test)]
#[path = "preferences_tests.rs"]
mod preferences_tests;
#[cfg(test)]
#[path = "prs_integration_tests.rs"]
mod prs_integration_tests;
#[cfg(test)]
#[path = "prs_test_fixtures.rs"]
mod prs_test_fixtures;
#[cfg(test)]
#[path = "prs_tests.rs"]
mod prs_tests;
#[cfg(test)]
#[path = "prs_tests_bodyless_review_nav.rs"]
mod prs_tests_bodyless_review_nav;
#[cfg(test)]
#[path = "prs_tests_chooser_security.rs"]
mod prs_tests_chooser_security;
#[cfg(test)]
#[path = "prs_tests_close_delete.rs"]
mod prs_tests_close_delete;
#[cfg(test)]
#[path = "prs_tests_components.rs"]
mod prs_tests_components;
#[cfg(test)]
#[path = "prs_tests_composer_focus.rs"]
mod prs_tests_composer_focus;
#[cfg(test)]
#[path = "prs_tests_cursor_arrows.rs"]
mod prs_tests_cursor_arrows;
#[cfg(test)]
#[path = "prs_tests_detail.rs"]
mod prs_tests_detail;
#[cfg(test)]
#[path = "prs_tests_detail_flow.rs"]
mod prs_tests_detail_flow;
#[cfg(test)]
#[path = "prs_tests_filter.rs"]
mod prs_tests_filter;
#[cfg(test)]
#[path = "prs_tests_merge.rs"]
mod prs_tests_merge;
#[cfg(test)]
#[path = "prs_tests_new_form.rs"]
mod prs_tests_new_form;
#[cfg(test)]
#[path = "prs_tests_pagination.rs"]
mod prs_tests_pagination;
#[cfg(test)]
#[path = "prs_tests_repo_nav.rs"]
mod prs_tests_repo_nav;
#[cfg(test)]
#[path = "prs_tests_review_order.rs"]
mod prs_tests_review_order;
#[cfg(test)]
#[path = "prs_tests_review_threads.rs"]
mod prs_tests_review_threads;
#[cfg(test)]
#[path = "prs_tests_silent_refresh.rs"]
mod prs_tests_silent_refresh;
#[cfg(test)]
#[path = "prs_tests_sort.rs"]
mod prs_tests_sort;
#[cfg(test)]
#[path = "transient_agent_tests.rs"]
mod transient_agent_tests;
#[cfg(test)]
#[path = "transient_system_message_tests.rs"]
mod transient_system_message_tests;
#[cfg(test)]
#[path = "workbench_tests.rs"]
mod workbench_tests;
