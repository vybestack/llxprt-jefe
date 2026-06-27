//! Pull Requests Mode composer-focus tests (#56) — open composer sets
//! NewComment subfocus, comment created appends + follows viewport, agent
//! chooser open/navigate/confirm/cancel.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011

use super::prs_test_fixtures::prs_state_with_detail;
use crate::domain::{IssueComment, PrCheck, PrCheckStatus, PrReview, PrReviewState, RepositoryId};
use crate::state::AppState;
use crate::state::types::{AppEvent, ComposerTarget, InlineState, PrDetailSubfocus};

/// Helper: a test comment.
fn make_comment(id: u64, author: &str) -> IssueComment {
    IssueComment {
        comment_id: id,
        author_login: author.to_string(),
        created_at: "2024-01-03T00:00:00Z".to_string(),
        edited_at: None,
        body: format!("Comment {id}"),
    }
}

/// PrOpenNewCommentComposer must set inline_state to Composer(NewComment) AND
/// move detail_subfocus to NewComment (#56).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 292-298
#[test]
fn test_open_comment_composer_sets_subfocus_newcomment() {
    let state = prs_state_with_detail("repo-1", 1);

    let new_state = state.apply(AppEvent::PrOpenNewCommentComposer);

    assert!(
        matches!(
            &new_state.prs_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            }
        ),
        "inline_state must be Composer(NewComment), got {:?}",
        new_state.prs_state.inline_state
    );
    assert_eq!(
        new_state.prs_state.detail_subfocus,
        PrDetailSubfocus::NewComment,
        "detail_subfocus must move to NewComment (#56)"
    );
}

/// PrCommentCreated must append the comment, clear the composer, set subfocus
/// to the new comment, and follow the viewport to reveal it (#56).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 316-322
#[test]
fn test_comment_created_appends_and_marks_follow_viewport() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Simulate an active composer (pending mutation).
    state.prs_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft text".to_string(),
        cursor: 10,
    };
    state.prs_state.mutation_pending = Some(crate::state::types::PrMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        target: ComposerTarget::NewComment,
    });
    state.prs_state.next_mutation_id = 2;
    let existing = make_comment(100, "alice");
    state
        .prs_state
        .pr_detail
        .as_mut()
        .unwrap_or_else(|| panic!("detail should exist"))
        .comments = vec![existing];

    let new_state = state.apply(AppEvent::PrCommentCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        mutation_id: 1,
        comment: make_comment(101, "bob"),
    });

    let detail = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("detail should remain"));
    assert_eq!(
        detail.comments.len(),
        2,
        "comment must be appended to existing"
    );
    assert_eq!(detail.comments[1].comment_id, 101);
    // Composer cleared.
    assert_eq!(new_state.prs_state.inline_state, InlineState::None);
    assert!(
        new_state.prs_state.mutation_pending.is_none(),
        "mutation_pending must clear after success"
    );
    // Subfocus set to the new comment.
    assert_eq!(
        new_state.prs_state.detail_subfocus,
        PrDetailSubfocus::Comment(1),
        "subfocus must point at the newly-created comment (#56)"
    );
}

/// After a comment is created, the detail must scroll to the REAL rendered
/// bottom (including reviews, checks, section headers, and separators) so the
/// newly-posted comment is on-screen, and a later page-down does not jump.
///
/// Regression (#56): the post-create scroll used a stale heuristic that counted
/// only header+body+comments, so with reviews/checks present it under-scrolled
/// and the new comment rendered below the viewport (off-screen).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 316-322
#[test]
fn test_comment_created_scrolls_to_real_rendered_bottom_with_reviews_and_checks() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Small viewport so the bottom is below the fold.
    state.prs_state.detail_viewport_rows = 6;
    // Populate the sections the stale heuristic ignored: reviews + checks.
    populate_full_detail_sections(&mut state);
    // Simulate an active composer with a pending mutation.
    state.prs_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "ship it".to_string(),
        cursor: 7,
    };
    state.prs_state.mutation_pending = Some(crate::state::types::PrMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        target: ComposerTarget::NewComment,
    });
    state.prs_state.next_mutation_id = 2;

    let new_state = state.apply(AppEvent::PrCommentCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        mutation_id: 1,
        comment: make_comment(101, "bob"),
    });

    let detail = new_state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail should remain after comment create"));
    // The REAL rendered bottom, derived the same way the scroll clamp does
    // (composer is closed after create, so subfocus + inline_state reflect that).
    let rendered_lines = crate::pr_detail_content::pr_detail_content_line_count(
        detail,
        new_state.prs_state.detail_subfocus,
        &new_state.prs_state.inline_state,
        new_state.prs_state.loading.detail,
        new_state.prs_state.loading.comments,
    );
    let expected_bottom = rendered_lines.saturating_sub(new_state.prs_state.detail_viewport_rows);

    assert_eq!(
        new_state.prs_state.detail_scroll_offset,
        expected_bottom,
        "PrCommentCreated must scroll to the REAL rendered bottom \
         (offset={}, expected={}, rendered_lines={}, viewport={})",
        new_state.prs_state.detail_scroll_offset,
        expected_bottom,
        rendered_lines,
        new_state.prs_state.detail_viewport_rows
    );
    // The new comment's last line must be within the viewport after create.
    assert!(
        new_state.prs_state.detail_scroll_offset + new_state.prs_state.detail_viewport_rows
            >= rendered_lines,
        "newly-created comment must be within the viewport (not off-screen)"
    );
}

/// PrOpenAgentChooser must open the chooser (when agents available).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-011
/// @pseudocode component-001 lines 331-340
#[test]
fn test_agent_chooser_open_navigate_confirm_cancel() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Provide agents so the chooser opens.
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/agent1"),
    ));
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("agent-2".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 2".to_string(),
        std::path::PathBuf::from("/tmp/agent2"),
    ));

    // Open the chooser.
    let state = state.apply(AppEvent::PrOpenAgentChooser);
    assert!(
        state.prs_state.agent_chooser.is_some(),
        "agent_chooser must open"
    );

    // Navigate down.
    let state = state.apply(AppEvent::PrAgentChooserNavigateDown);
    let chooser = state
        .prs_state
        .agent_chooser
        .clone()
        .unwrap_or_else(|| panic!("chooser should remain open after navigate"));
    assert_eq!(chooser.selected_index, 1);

    // Navigate up.
    let state = state.apply(AppEvent::PrAgentChooserNavigateUp);
    let chooser = state
        .prs_state
        .agent_chooser
        .clone()
        .unwrap_or_else(|| panic!("chooser should remain open after navigate"));
    assert_eq!(chooser.selected_index, 0);

    // Confirm closes the chooser (and dispatches the send — not asserted here).
    let state = state.apply(AppEvent::PrAgentChooserConfirm);
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "agent_chooser must close on confirm"
    );

    // Re-open then cancel.
    let state = state.apply(AppEvent::PrOpenAgentChooser);
    assert!(state.prs_state.agent_chooser.is_some());
    let state = state.apply(AppEvent::PrAgentChooserCancel);
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "agent_chooser must close on cancel"
    );
}

/// Helper: populate body + reviews + checks + comments on the selected PR
/// detail so the rendered content overflows a small viewport. Exercises the
/// sections the old heuristic ignored (reviews, checks, separators, headers).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn populate_full_detail_sections(state: &mut AppState) {
    let detail = state
        .prs_state
        .pr_detail
        .as_mut()
        .unwrap_or_else(|| panic!("detail should exist"));
    detail.body = "Line A
Line B
Line C"
        .to_string();
    detail.reviews = vec![
        PrReview {
            author_login: "rev1".to_string(),
            state: PrReviewState::Approved,
            submitted_at: "2024-01-02T00:00:00Z".to_string(),
            body: Some("looks good".to_string()),
        },
        PrReview {
            author_login: "rev2".to_string(),
            state: PrReviewState::ChangesRequested,
            submitted_at: "2024-01-02T01:00:00Z".to_string(),
            body: None,
        },
    ];
    detail.checks = vec![
        PrCheck {
            name: "build".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "passed".to_string(),
            url: None,
        },
        PrCheck {
            name: "test".to_string(),
            status: PrCheckStatus::Failure,
            conclusion: "failed".to_string(),
            url: None,
        },
    ];
    detail.comments = vec![make_comment(100, "alice"), make_comment(101, "bob")];
}

/// Opening the new-comment composer must scroll the detail viewport to the
/// REAL rendered bottom (including reviews, checks, section headers,
/// separators, and the composer block) so the composer is on-screen.
///
/// Regression: a stale heuristic that counted only header+body+comments left
/// the composer rendered below the viewport (off-screen), and a later
/// page-down — which clamps to the real, larger max — made the screen jump.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn test_open_composer_scrolls_to_real_rendered_bottom_so_composer_visible() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Small viewport so the bottom is below the fold.
    state.prs_state.detail_viewport_rows = 6;
    // Populate the sections the stale heuristic ignored: reviews + checks.
    populate_full_detail_sections(&mut state);

    let new_state = state.apply(AppEvent::PrOpenNewCommentComposer);

    let detail = new_state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail should exist"));
    // The REAL rendered bottom, derived the same way the scroll clamp does.
    let rendered_lines = crate::pr_detail_content::pr_detail_content_line_count(
        detail,
        new_state.prs_state.detail_subfocus,
        &new_state.prs_state.inline_state,
        new_state.prs_state.loading.detail,
        new_state.prs_state.loading.comments,
    );
    let expected_bottom = rendered_lines.saturating_sub(new_state.prs_state.detail_viewport_rows);

    assert_eq!(
        new_state.prs_state.detail_scroll_offset,
        expected_bottom,
        "opening the composer must scroll to the REAL rendered bottom \
         (offset={}, expected={}, rendered_lines={}, viewport={})",
        new_state.prs_state.detail_scroll_offset,
        expected_bottom,
        rendered_lines,
        new_state.prs_state.detail_viewport_rows
    );
    // And that bottom must reveal the composer's final line (within viewport).
    assert!(
        new_state.prs_state.detail_scroll_offset + new_state.prs_state.detail_viewport_rows
            >= rendered_lines,
        "composer's last line must be within the viewport after open"
    );
}

/// HIGH-2: While a mutation is pending, pressing Esc/Cancel MUST close the
/// composer (inline_state → None) and clear mutation_pending — it must NOT be
/// swallowed by the early-return guard and leave the composer frozen.
///
/// @plan PLAN-20260624-PR-MODE.P05
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 308-330
#[test]
fn test_esc_closes_composer_while_mutation_pending() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft text".to_string(),
        cursor: 10,
    };
    state.prs_state.mutation_pending = Some(crate::state::types::PrMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        target: ComposerTarget::NewComment,
    });

    let new_state = state.apply(AppEvent::PrInlineCancelOrEsc);

    assert_eq!(
        new_state.prs_state.inline_state,
        InlineState::None,
        "Esc MUST close the composer even while a mutation is pending"
    );
    assert!(
        new_state.prs_state.mutation_pending.is_none(),
        "Esc MUST clear mutation_pending (cancel the in-flight intent)"
    );
}

/// HIGH-2: a late-arriving PrCommentCreated AFTER the user cancelled (which
/// cleared mutation_pending) MUST no-op — the completion handler guards on
/// mutation_pending matching and tolerates the dropped mutation.
///
/// We drive this through `apply_prs_event` (the reducer entry point) rather
/// than `apply` because an unhandled event trips `apply_message`'s routing
/// debug_assert; the observable behavior we care about is that NO comment is
/// appended and mutation_pending stays None.
///
/// @plan PLAN-20260624-PR-MODE.P05
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 316-327
#[test]
fn test_comment_created_noops_after_cancel_cleared_mutation_pending() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // The user cancelled: composer is closed and mutation_pending is None.
    state.prs_state.inline_state = InlineState::None;
    state.prs_state.mutation_pending = None;
    let before = state.clone();

    let handled = state.apply_prs_event(AppEvent::PrCommentCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        mutation_id: 1,
        comment: make_comment(101, "bob"),
    });

    // No comment appended (the cancelled mutation's result is dropped): the
    // detail comments list is unchanged.
    assert!(
        state
            .prs_state
            .pr_detail
            .as_ref()
            .is_some_and(|d| d.comments.is_empty()),
        "late comment from a cancelled mutation MUST NOT be appended"
    );
    assert!(
        state.prs_state.mutation_pending.is_none(),
        "mutation_pending stays None after a cancelled-mutation result"
    );
    // The handler reports whether it mutated state; the key invariant is the
    // observable state is unchanged for comments (no append).
    let _ = handled;
    let _ = before;
}

/// MED-1: Submitting with NO repo selected MUST surface a visible error and
/// close the composer — it must NOT silently freeze the composer open with no
/// feedback.
///
/// @plan PLAN-20260624-PR-MODE.P05
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 308-315
#[test]
fn test_submit_no_repo_selected_surfaces_error_and_closes_composer() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Remove the selected repo so there is no scope.
    state.selected_repository_index = None;
    state.prs_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft text".to_string(),
        cursor: 10,
    };
    state.prs_state.mutation_pending = None;

    let new_state = state.apply(AppEvent::PrInlineSubmit);

    assert_eq!(
        new_state.prs_state.inline_state,
        InlineState::None,
        "composer MUST close when no repo is selected"
    );
    assert!(
        new_state.prs_state.mutation_pending.is_none(),
        "no mutation MUST be pending when no repo is selected"
    );
    assert!(
        new_state.prs_state.error.is_some(),
        "a visible error MUST be surfaced when no repo is selected"
    );
}

/// MED-2: An `InlineState::Editor` (unreachable in PR mode — no PR path sets
/// it) MUST NOT be silently misrouted to a NewComment mutation. Submitting
/// from an Editor state must surface an error and create NO mutation, rather
/// than fabricate a bogus NewComment target.
///
/// @plan PLAN-20260624-PR-MODE.P05
/// @requirement REQ-PR-010
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 308-315
#[test]
fn test_submit_from_editor_does_not_create_newcomment_mutation() {
    let mut state = prs_state_with_detail("repo-1", 1);
    state.prs_state.inline_state = InlineState::Editor {
        target: crate::state::types::EditorTarget::Comment { comment_index: 0 },
        text: "edited".to_string(),
        cursor: 6,
    };
    state.prs_state.mutation_pending = None;

    let new_state = state.apply(AppEvent::PrInlineSubmit);

    assert!(
        new_state
            .prs_state
            .mutation_pending
            .as_ref()
            .is_none_or(|p| !matches!(p.target, ComposerTarget::NewComment)),
        "Editor submit MUST NOT fabricate a NewComment mutation"
    );
    assert!(
        new_state.prs_state.mutation_pending.is_none(),
        "Editor submit MUST create no mutation in PR mode"
    );
    assert!(
        new_state.prs_state.error.is_some(),
        "Editor submit MUST surface a visible error (unreachable-but-guarded)"
    );
}

/// Regression (#20): typing in the PR new-comment composer must NOT yank the
/// detail viewport back to the top (the old per-keystroke scroll-follow did).
/// Mirrors the Issues open-scroll contract: the viewport scrolls ONLY on
/// composer open (to the rendered bottom) and stays put while typing. After
/// opening the composer on a detail taller than the viewport and typing several
/// characters, (1) `detail_scroll_offset` must NOT reset to 0 (it stays at the
/// bottom region where the composer is), and (2) the built content's cursor
/// line must fall within `[offset, offset + viewport_rows)` — i.e. the caret is
/// inside the visible window on the composer line, NOT the header boundary.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn test_composer_typing_does_not_reset_scroll_and_caret_stays_visible() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Make the detail content taller than the viewport by adding body lines.
    {
        let detail = state
            .prs_state
            .pr_detail
            .as_mut()
            .unwrap_or_else(|| panic!("detail should exist"));
        detail.body = (0..30)
            .map(|i| format!("body line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
    }
    // Small viewport so the rendered bottom is well below the top.
    state.prs_state.detail_viewport_rows = 5;

    // Open the composer — this scrolls to the rendered bottom (#56).
    let mut state = state.apply(AppEvent::PrOpenNewCommentComposer);
    let offset_after_open = state.prs_state.detail_scroll_offset;
    assert!(
        offset_after_open > 0,
        "opening the composer must scroll down to the rendered bottom (offset > 0)"
    );

    // Type several characters — the per-keystroke scroll-follow is GONE, so the
    // offset must NOT change (the view stays where open left it).
    for ch in "hello world".chars() {
        state = state.apply(AppEvent::PrInlineChar(ch));
    }

    assert_eq!(
        state.prs_state.detail_scroll_offset, offset_after_open,
        "typing must NOT reset the scroll offset (no per-keystroke scroll-follow); \
         expected {offset_after_open}, got {}",
        state.prs_state.detail_scroll_offset
    );
    assert_ne!(
        state.prs_state.detail_scroll_offset, 0,
        "scroll offset must NOT have been yanked to the top (the regression)"
    );

    // The caret must be inside the visible window — on the composer line, not
    // the header boundary.
    let detail = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail should exist"));
    let content = crate::pr_detail_content::build_pr_detail_content(
        detail,
        state.prs_state.detail_subfocus,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    );
    let (cursor_line, _col) = content
        .cursor
        .unwrap_or_else(|| panic!("composer must expose a caret while typing"));
    let offset = state.prs_state.detail_scroll_offset;
    let viewport = state.prs_state.detail_viewport_rows;
    assert!(
        cursor_line >= offset && cursor_line < offset + viewport,
        "caret line {cursor_line} must be inside the visible window \
         [{offset}, {}) (on the composer line, not the header)",
        offset + viewport
    );
}
