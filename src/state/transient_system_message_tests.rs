//! Tests for the transient-agent SystemMessage handling in the reducer
//! (issue #213). Extracted from `mod.rs` to keep that file under the
//! source-file-size hard limit.

use crate::domain::{Repository, RepositoryId};
use crate::messages::{AppMessage, SystemMessage};
use crate::state::AppState;

use std::path::PathBuf;

fn state_with_repo() -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_owned()),
        "Test".to_owned(),
        "test".to_owned(),
        PathBuf::from("/tmp/test"),
    ));
    state
}

#[test]
fn transient_agent_queued_sets_draft_notice_position_zero() {
    let mut state = state_with_repo();
    state.issues_state.draft_notice = None;
    state.prs_state.draft_notice = None;

    let after = state.apply_message(AppMessage::System(SystemMessage::TransientAgentQueued {
        queue_position: 0,
    }));

    assert_eq!(
        after.issues_state.draft_notice.as_deref(),
        Some("Transient agent queued — launching next…"),
    );
}

#[test]
fn transient_agent_queued_sets_draft_notice_with_position() {
    let state = state_with_repo();

    let after = state.apply_message(AppMessage::System(SystemMessage::TransientAgentQueued {
        queue_position: 3,
    }));

    assert_eq!(
        after.issues_state.draft_notice.as_deref(),
        Some("Transient agent queued (position 3)"),
    );
    assert_eq!(
        after.prs_state.draft_notice.as_deref(),
        Some("Transient agent queued (position 3)"),
    );
}

#[test]
fn transient_agent_dequeued_clears_draft_notice() {
    let mut state = state_with_repo();
    state.issues_state.draft_notice = Some("Transient agent queued (position 1)".to_owned());
    state.prs_state.draft_notice = Some("Transient agent queued (position 1)".to_owned());

    let after = state.apply_message(AppMessage::System(SystemMessage::TransientAgentDequeued));

    assert!(
        after.issues_state.draft_notice.is_none(),
        "TransientAgentDequeued must clear issues draft_notice"
    );
    assert!(
        after.prs_state.draft_notice.is_none(),
        "TransientAgentDequeued must clear prs draft_notice"
    );
}
