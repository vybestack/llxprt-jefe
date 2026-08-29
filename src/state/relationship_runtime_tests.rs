use super::issues_test_fixtures::begin_issue_list_reload;
use super::prs_test_fixtures::{begin_pr_list_reload, test_pull_request};
use super::relationship_runtime::SelectedResourceKind;
use super::transition::TransitionExt;
use super::{
    AppEvent, AppState, ComposerTarget, InlineState, IssueLifecycleMutationPending,
    RelationshipCommand, RelationshipCommandError,
};
use crate::domain::{
    Id, Issue, IssueFilter, IssueState, PrFilter, Repository, RepositoryId, TypedPortValue,
    TypedValue,
};
use crate::workbench::{
    ISSUES_LIST_PANEL, PanelId, PanelInstanceId, PortId, PortRef, PortValue, SUBJECT_PORT,
    ScreenId, ScreenInstanceId, SourceIntent,
};
use std::path::PathBuf;

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| unreachable!("valid fixture id: {error}"))
}

fn resource(type_id: &str, number: &str) -> PortValue {
    PortValue::Typed(TypedPortValue {
        type_id: id(type_id),
        schema_version: 1,
        semantic_key: number.to_owned(),
        value: [(id("semantic-key"), TypedValue::String(number.to_owned()))].into(),
    })
}

fn issue(number: &str) -> PortValue {
    resource("github.issue", number)
}

fn pull_request(number: &str) -> PortValue {
    PortValue::Typed(TypedPortValue {
        type_id: id("github.pull-request"),
        schema_version: 1,
        semantic_key: number.to_owned(),
        value: [
            (id("semantic-key"), TypedValue::String(number.to_owned())),
            (id("head-sha"), TypedValue::String("sha123".to_owned())),
        ]
        .into(),
    })
}

fn port(panel: &'static str, port: &'static str) -> PortRef {
    PortRef {
        panel: PanelId::parse(panel)
            .unwrap_or_else(|error| unreachable!("valid fixture panel: {error}")),
        port: PortId::parse(port)
            .unwrap_or_else(|error| unreachable!("valid fixture port: {error}")),
    }
}

fn issue_selection(number: &str) -> SourceIntent {
    SourceIntent::Publish {
        port: port(ISSUES_LIST_PANEL, "selection"),
        value: issue(number),
    }
}

fn list_issue(number: u64) -> Issue {
    Issue {
        number,
        node_id: format!("I_{number}"),
        title: format!("Issue {number}"),
        state: IssueState::Open,
        author_login: "alice".to_owned(),
        updated_at: "2026-08-17T00:00:00Z".to_owned(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        state_reason: None,
        created_at: "2026-08-17T00:00:00Z".to_owned(),
        priority: None,
        linked_pr_numbers: Vec::new(),
    }
}

fn select_repository(state: &mut AppState) {
    let mut repository = Repository::new(
        RepositoryId("repo-1".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repository".to_owned(),
        "repo-1".to_owned(),
        PathBuf::from("/tmp/repo-1"),
    );
    repository.github_repo = "vybestack/llxprt-jefe".to_owned();
    state.repositories.push(repository);
    state.selected_repository_index = Some(0);
}
fn accept_issue_list(mut state: AppState, issues: Vec<Issue>) -> AppState {
    let request_id = begin_issue_list_reload(&mut state, "repo-1", IssueFilter::default());
    state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            filter: Box::new(IssueFilter::default()),
            request_id,
            issues,
            cursor: None,
            has_more: false,
        })
        .committed_pure()
}

fn state_with_issue_selection() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    select_repository(&mut state);
    let _ = state.switch_screen(ScreenId::Issues);
    let request_id = begin_issue_list_reload(&mut state, "repo-1", IssueFilter::default());
    state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            filter: Box::new(IssueFilter::default()),
            request_id,
            issues: vec![list_issue(42)],
            cursor: None,
            has_more: false,
        })
        .committed_pure()
}

fn state_with_pr_selection() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    select_repository(&mut state);
    let _ = state.switch_screen(ScreenId::PullRequests);
    let request_id = begin_pr_list_reload(&mut state, "repo-1", PrFilter::default());
    state
        .apply(AppEvent::PrListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            filter: Box::new(PrFilter::default()),
            request_id,
            pull_requests: vec![test_pull_request(42)],
            cursor: None,
            has_more: false,
        })
        .committed_pure()
}

fn current_subject(state: &AppState, detail_panel: &'static str) -> PortValue {
    let instance = state
        .nav
        .current()
        .relationships()
        .unwrap_or_else(|| unreachable!("published screen has a relationship runtime"));
    let subject = port(detail_panel, SUBJECT_PORT);
    let key = instance
        .port_key(&subject)
        .unwrap_or_else(|| unreachable!("detail panel has a runtime identity"));
    state.nav.current().relationship_state().value(&key)
}

fn current_issue_subject(state: &AppState) -> PortValue {
    current_subject(state, "issue-detail")
}

fn current_pr_subject(state: &AppState) -> PortValue {
    current_subject(state, "pr-detail")
}
#[test]
fn the_same_number_in_two_trackers_has_distinct_resource_identity() {
    let mut first = Repository::new(
        RepositoryId("first".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "First".to_owned(),
        "first".to_owned(),
        PathBuf::from("/tmp/first"),
    );
    first.github_repo = "example/first".to_owned();
    let mut second = first.clone();
    second.id = RepositoryId("second".to_owned());
    second.github_repo = "example/second".to_owned();

    assert_eq!(
        super::relationship_runtime::github_resource_key(Some(&first), Some(42)),
        Ok(Some("example/first#42".to_owned()))
    );
    assert_eq!(
        super::relationship_runtime::github_resource_key(Some(&second), Some(42)),
        Ok(Some("example/second#42".to_owned()))
    );
}

#[test]
fn issue_open_repository_search_and_filter_clears_publish_absence() {
    let mut state = state_with_issue_selection();
    state = state.apply(AppEvent::EnterIssuesMode).committed_pure();
    assert_eq!(current_issue_subject(&state), PortValue::Absent);

    let mut state = state_with_issue_selection();
    state.reset_issues_for_repo_change();
    assert_eq!(current_issue_subject(&state), PortValue::Absent);

    let mut state = state_with_issue_selection();
    state = state.apply(AppEvent::ApplySearch).committed_pure();
    assert_eq!(current_issue_subject(&state), PortValue::Absent);

    let mut state = state_with_issue_selection();
    state = state.apply(AppEvent::ApplyFilter).committed_pure();
    assert_eq!(current_issue_subject(&state), PortValue::Absent);
}

#[test]
fn pr_open_repository_search_and_filter_clears_publish_absence() {
    let mut state = state_with_pr_selection();
    state = state.apply(AppEvent::EnterPrsMode).committed_pure();
    assert_eq!(current_pr_subject(&state), PortValue::Absent);

    let mut state = state_with_pr_selection();
    state.reset_prs_for_repo_change();
    assert_eq!(current_pr_subject(&state), PortValue::Absent);

    let mut state = state_with_pr_selection();
    state = state.apply(AppEvent::PrApplySearch).committed_pure();
    assert_eq!(current_pr_subject(&state), PortValue::Absent);

    let mut state = state_with_pr_selection();
    state = state.apply(AppEvent::PrApplyFilter).committed_pure();
    assert_eq!(current_pr_subject(&state), PortValue::Absent);
}

#[test]
fn production_screen_instances_commit_typed_relationships_through_the_published_registry() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);

    let updates = state
        .apply_relationship_intent(issue_selection("42"))
        .unwrap_or_else(|error| unreachable!("published issue resource must commit: {error}"));

    assert_eq!(updates.len(), 2);
    assert_eq!(current_issue_subject(&state), issue("42"));
}

#[test]
fn replacing_navigation_with_a_durable_root_rebinds_relationships() {
    let mut state = AppState::new(crate::test_support::published_workbench());

    state.restore_navigation_root(ScreenId::Issues);

    let relationships = state
        .nav
        .current()
        .relationships()
        .unwrap_or_else(|| unreachable!("restored Issues screen has a relationship runtime"));
    assert_eq!(relationships.open_screen_id(), state.nav.current().id);
    assert!(
        relationships
            .port_key(&port(ISSUES_LIST_PANEL, "selection"))
            .is_some()
    );
}

#[test]
fn relationship_commands_reject_stale_or_forged_producers_without_mutation() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);
    let current = state.nav.current();
    let port = port(ISSUES_LIST_PANEL, "selection");
    let panel_instance = current
        .relationships()
        .and_then(|relationships| relationships.panel_instance_id(&port.panel))
        .unwrap_or_else(|| unreachable!("Issues source panel is instantiated"));
    let live_screen = current.id;
    let live_generation = current.generation;

    let cases = [
        (
            RelationshipCommand {
                open_screen_id: ScreenInstanceId::preview(),
                panel_instance_id: panel_instance,
                generation: live_generation,
                owner_id: id("github.issues"),
                intent: issue_selection("42"),
            },
            RelationshipCommandError::StaleScreen,
        ),
        (
            RelationshipCommand {
                open_screen_id: live_screen,
                panel_instance_id: panel_instance,
                generation: live_generation + 1,
                owner_id: id("github.issues"),
                intent: issue_selection("42"),
            },
            RelationshipCommandError::StaleGeneration,
        ),
        (
            RelationshipCommand {
                open_screen_id: live_screen,
                panel_instance_id: PanelInstanceId::from_u64(panel_instance.as_u64() + 10_000),
                generation: live_generation,
                owner_id: id("github.issues"),
                intent: issue_selection("42"),
            },
            RelationshipCommandError::WrongPanel,
        ),
        (
            RelationshipCommand {
                open_screen_id: live_screen,
                panel_instance_id: panel_instance,
                generation: live_generation,
                owner_id: id("github.pull-requests"),
                intent: issue_selection("42"),
            },
            RelationshipCommandError::WrongOwner,
        ),
    ];

    for (command, expected) in cases {
        assert_eq!(state.apply_relationship_command(command), Err(expected));
        assert_eq!(current_issue_subject(&state), PortValue::Absent);
    }
}

#[test]
fn suspended_instances_restore_their_own_relationship_state() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);
    state
        .apply_relationship_intent(issue_selection("41"))
        .unwrap_or_else(|error| unreachable!("first issue selection must commit: {error}"));
    let first_id = state.nav.current().id;

    let _ = state.enter_screen(ScreenId::Settings);
    let _ = state.enter_screen(ScreenId::Issues);
    state
        .apply_relationship_intent(issue_selection("42"))
        .unwrap_or_else(|error| unreachable!("second issue selection must commit: {error}"));
    let second_id = state.nav.current().id;

    assert_ne!(first_id, second_id);
    assert_eq!(current_issue_subject(&state), issue("42"));

    let _ = state.leave_screen();
    let _ = state.leave_screen();

    assert_eq!(state.nav.current().id, first_id);
    assert_eq!(current_issue_subject(&state), issue("41"));
}

#[test]
fn a_fresh_screen_instance_starts_without_inheriting_selection_and_preserves_restore() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);
    select_repository(&mut state);
    let request_id = begin_issue_list_reload(&mut state, "repo-1", IssueFilter::default());
    state = state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            filter: Box::new(IssueFilter::default()),
            request_id,
            issues: vec![list_issue(42)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();
    let first_id = state.nav.current().id;

    let _ = state.enter_screen(ScreenId::Settings);
    let _ = state.enter_screen(ScreenId::Issues);

    assert_ne!(state.nav.current().id, first_id);
    assert_eq!(current_issue_subject(&state), PortValue::Absent);

    state.publish_selected_resource(
        SelectedResourceKind::Issue,
        Some("vybestack/llxprt-jefe#43".to_owned()),
        None,
    );
    let _ = state.leave_screen();
    let _ = state.leave_screen();

    assert_eq!(state.nav.current().id, first_id);
    assert_eq!(
        current_issue_subject(&state),
        issue("vybestack/llxprt-jefe#42")
    );
}

#[test]
fn accepted_issue_loads_publish_and_clear_the_visible_selection() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);
    select_repository(&mut state);

    state = accept_issue_list(state, vec![list_issue(42)]);
    assert_eq!(
        current_issue_subject(&state),
        issue("vybestack/llxprt-jefe#42")
    );

    let _ = state.enter_screen(ScreenId::Settings);
    state = accept_issue_list(state, Vec::new());
    assert!(state.error_message.is_none());
    let _ = state.leave_screen();
    assert_eq!(
        current_issue_subject(&state),
        issue("vybestack/llxprt-jefe#42")
    );

    state = accept_issue_list(state, Vec::new());
    assert_eq!(current_issue_subject(&state), PortValue::Absent);

    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "title".to_owned(),
        cursor: 5,
    };
    state = state
        .apply(AppEvent::MutationSubmitted {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            mutation_id: 7,
            target: submitted_target,
        })
        .committed_pure();
    state = state
        .apply(AppEvent::IssueCreated {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            mutation_id: 7,
            issue: Box::new(list_issue(43)),
        })
        .committed_pure();
    assert_eq!(
        current_issue_subject(&state),
        issue("vybestack/llxprt-jefe#43")
    );

    state.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_owned()),
        mutation_id: 8,
        issue_number: 43,
        node_id: Some("I_43".to_owned()),
        close_reason: None,
        duplicate_of: None,
    });
    state = state
        .apply(AppEvent::IssueDeleted {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            issue_number: 43,
            mutation_id: 8,
        })
        .committed_pure();
    assert_eq!(current_issue_subject(&state), PortValue::Absent);
}

#[test]
fn accepted_pr_loads_publish_and_clear_the_visible_selection() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::PullRequests);
    select_repository(&mut state);

    let request_id = begin_pr_list_reload(&mut state, "repo-1", PrFilter::default());
    state = state
        .apply(AppEvent::PrListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            filter: Box::new(PrFilter::default()),
            request_id,
            pull_requests: vec![test_pull_request(42)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();
    assert_eq!(
        current_pr_subject(&state),
        pull_request("vybestack/llxprt-jefe#42")
    );

    let request_id = begin_pr_list_reload(&mut state, "repo-1", PrFilter::default());
    state = state
        .apply(AppEvent::PrListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            filter: Box::new(PrFilter::default()),
            request_id,
            pull_requests: Vec::new(),
            cursor: None,
            has_more: false,
        })
        .committed_pure();
    assert_eq!(current_pr_subject(&state), PortValue::Absent);
}

#[test]
fn provider_action_context_projects_exact_current_typed_resources() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);
    select_repository(&mut state);
    state = accept_issue_list(state, vec![list_issue(42)]);

    let context = super::provider_action_context::project_current_context(&state)
        .unwrap_or_else(|error| unreachable!("valid issue context: {error}"));
    assert_eq!(context.len(), 2);
    assert_eq!(
        context.get(&id("issue-detail.subject")),
        Some(&TypedValue::Map(
            [
                (
                    id("owner-id"),
                    TypedValue::String("github.issues".to_owned())
                ),
                (id("type-id"), TypedValue::String("github.issue".to_owned())),
                (id("schema-version"), TypedValue::Integer(1)),
                (
                    id("semantic-key"),
                    TypedValue::String("vybestack/llxprt-jefe#42".to_owned()),
                ),
                (
                    id("value"),
                    TypedValue::Map(
                        [(
                            id("semantic-key"),
                            TypedValue::String("vybestack/llxprt-jefe#42".to_owned()),
                        )]
                        .into(),
                    ),
                ),
            ]
            .into(),
        )),
    );
}

pub(super) struct DestructiveConfirmationFixture {
    pub(super) state: AppState,
    pub(super) owner: Id,
    pub(super) action_id: Id,
    pub(super) confirmation_id: Id,
    pub(super) original_key: crate::domain::effects::ProviderRequestKey,
    pub(super) retained_context: crate::domain::TypedMap,
}

pub(super) fn apply_provider(
    state: AppState,
    message: crate::messages::ProviderMessage,
) -> super::transition::Transition {
    let result = state.apply_message(crate::messages::AppMessage::Provider(Box::new(message)));
    let Ok(transition) = result else {
        panic!("provider test transition must succeed");
    };
    transition
}

pub(super) fn destructive_confirmation_fixture() -> DestructiveConfirmationFixture {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::Issues);
    select_repository(&mut state);
    state = accept_issue_list(state, vec![list_issue(42)]);
    stage_destructive_confirmation(state)
}

fn destructive_pr_confirmation_fixture() -> DestructiveConfirmationFixture {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.switch_screen(ScreenId::PullRequests);
    select_repository(&mut state);
    let request_id = begin_pr_list_reload(&mut state, "repo-1", PrFilter::default());
    state = state
        .apply(AppEvent::PrListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_owned()),
            filter: Box::new(PrFilter::default()),
            request_id,
            pull_requests: vec![test_pull_request(42)],
            cursor: None,
            has_more: false,
        })
        .committed_pure();
    stage_destructive_confirmation(state)
}

fn stage_destructive_confirmation(state: AppState) -> DestructiveConfirmationFixture {
    use crate::domain::effects::{Effect, ProviderEffect};
    use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
    use crate::messages::ProviderMessage;
    use crate::runtime::provider::protocol::Outcome;

    let owner = id("host");
    let action_id = id("provider.run");
    let confirmation_id = id("confirm.run");
    let policy = super::provider_requests::ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        true,
    );
    let Ok(retained_context) = super::provider_action_context::project_current_context(&state)
    else {
        panic!("resource context must be valid");
    };
    let invoked = apply_provider(
        state,
        ProviderMessage::Invoke {
            owner: owner.clone(),
            action_id: action_id.clone(),
            arguments: crate::domain::TypedMap::new(),
            policy,
        },
    );
    let original_key = match &invoked.effects[0].effect {
        Effect::Provider(ProviderEffect::InvokeAction { invocation }) => {
            assert_eq!(invocation.context_refs, retained_context);
            invocation.key.clone()
        }
        other => panic!("expected invoke effect, got {other:?}"),
    };
    let requested = apply_provider(
        invoked.next_state,
        ProviderMessage::Outcome {
            key: original_key.clone(),
            outcome: Outcome::RequestHostConfirmation {
                confirmation_id: confirmation_id.clone(),
                title: "Confirm".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Proceed".to_owned(),
                destructive: true,
                continuation_schema: Vec::new(),
            },
            now_epoch: 100,
        },
    );
    DestructiveConfirmationFixture {
        state: requested.next_state,
        owner,
        action_id,
        confirmation_id,
        original_key,
        retained_context,
    }
}

fn confirm_message(
    fixture: &DestructiveConfirmationFixture,
    now_epoch: u64,
) -> crate::messages::ProviderMessage {
    crate::messages::ProviderMessage::Confirm {
        owner: fixture.owner.clone(),
        action_id: fixture.action_id.clone(),
        generation: fixture.original_key.generation,
        confirmation_id: fixture.confirmation_id.clone(),
        values: crate::domain::TypedMap::new(),
        now_epoch,
    }
}

fn assert_confirmed_reinvocation(
    transition: &super::transition::Transition,
    fixture: &DestructiveConfirmationFixture,
) {
    use crate::domain::effects::{Effect, ProviderEffect};

    assert_eq!(transition.effects.len(), 1);
    match &transition.effects[0].effect {
        Effect::Provider(ProviderEffect::InvokeAction { invocation }) => {
            assert_eq!(
                invocation.key.generation,
                fixture.original_key.generation + 1
            );
            assert_eq!(invocation.context_refs, fixture.retained_context);
            assert!(invocation.continuation.is_some());
        }
        other => panic!("expected confirmed invoke effect, got {other:?}"),
    }
    assert_eq!(
        transition
            .next_state
            .provider_requests
            .pending_confirmation_count(),
        0
    );
}

#[test]
fn destructive_confirmation_rejects_changed_semantic_identity_without_consuming_intent() {
    let fixture = destructive_confirmation_fixture();
    let mut changed = fixture.state.clone();
    let overlay_before = changed.nav.current().overlays().clone();
    let request_count = changed.provider_requests.requests().len();
    assert_eq!(changed.provider_requests.pending_confirmation_count(), 1);
    changed.publish_selected_resource(
        SelectedResourceKind::Issue,
        Some("vybestack/llxprt-jefe#43".to_owned()),
        None,
    );

    let rejected = apply_provider(changed, confirm_message(&fixture, 101));
    assert!(rejected.effects.is_empty());
    assert_eq!(
        rejected.next_state.provider_requests.requests().len(),
        request_count
    );
    assert_eq!(
        rejected
            .next_state
            .provider_requests
            .pending_confirmation_count(),
        1
    );
    assert_eq!(
        rejected.next_state.nav.current().overlays(),
        &overlay_before
    );
    assert_eq!(
        rejected.next_state.error_message.as_deref(),
        Some("provider action context no longer matches the authorized intent")
    );

    let mut restored = rejected.next_state;
    restored.publish_selected_resource(
        SelectedResourceKind::Issue,
        Some("vybestack/llxprt-jefe#42".to_owned()),
        None,
    );
    let confirmed = apply_provider(restored, confirm_message(&fixture, 102));
    assert_confirmed_reinvocation(&confirmed, &fixture);
}

#[test]
fn destructive_pr_confirmation_rejects_changed_head_without_consuming_intent() {
    let fixture = destructive_pr_confirmation_fixture();
    let mut changed = fixture.state.clone();
    let overlay_before = changed.nav.current().overlays().clone();
    let request_count = changed.provider_requests.requests().len();
    changed.prs_state.list.items_mut()[0].head_sha = "force-pushed-head".to_owned();
    changed.sync_pr_selected_resource();

    let rejected = apply_provider(changed, confirm_message(&fixture, 101));
    assert!(rejected.effects.is_empty());
    assert_eq!(
        rejected.next_state.provider_requests.requests().len(),
        request_count
    );
    assert_eq!(
        rejected
            .next_state
            .provider_requests
            .pending_confirmation_count(),
        1
    );
    assert_eq!(
        rejected.next_state.nav.current().overlays(),
        &overlay_before
    );
    assert_eq!(
        rejected.next_state.error_message.as_deref(),
        Some("provider action context no longer matches the authorized intent")
    );

    let mut restored = rejected.next_state;
    restored.prs_state.list.items_mut()[0].head_sha = "sha123".to_owned();
    restored.sync_pr_selected_resource();
    let confirmed = apply_provider(restored, confirm_message(&fixture, 102));
    assert_confirmed_reinvocation(&confirmed, &fixture);
}
