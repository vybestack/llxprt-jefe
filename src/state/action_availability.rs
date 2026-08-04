//! Deterministic action-availability reduction and publication.
//!
//! Current capability predicates are evaluated once into a complete closed
//! effect request. Only an exact correlated completion may replace the one
//! root-owned immutable registry snapshot.

use crate::domain::Id;
use crate::domain::action_registry::{ActionAvailability, Availability, AvailabilityGeneration};
use crate::domain::effects::{
    Correlation, Effect, EffectFamily, ProviderEffect, RetryPolicy, SemanticKey,
};
use crate::domain::{IssueState, PrState};

use super::{AppState, NO_AGENTS_AVAILABLE, PrDetailSubfocus, PrFocus, ReadOnlyHintKind};

const ACTION_AVAILABILITY_OWNER: &str = "core.keymap";
const ACTION_AVAILABILITY_SUBJECT: &str = "action-availability";

impl AppState {
    /// Record a refused action in the global warning and the active work-item
    /// screen's existing notice band.
    pub fn record_unavailable_action(&mut self, reason: String) {
        match self.screen() {
            super::ScreenId::Issues => self.issues_state.draft_notice = Some(reason.clone()),
            super::ScreenId::PullRequests => self.prs_state.draft_notice = Some(reason.clone()),
            super::ScreenId::Dashboard
            | super::ScreenId::Repositories
            | super::ScreenId::Actions
            | super::ScreenId::Errors
            | super::ScreenId::Terminals => {}
        }
        self.warning_message = Some(reason);
    }

    pub(super) fn stage_action_availability_projection(&mut self) {
        let Some(snapshot) = self.action_registry_snapshot.as_ref() else {
            return;
        };
        let entries = availability_entries(self, snapshot.actions());
        if authoritative_entries_match(snapshot, &entries) {
            return;
        }
        let Ok(owner) = Id::parse(ACTION_AVAILABILITY_OWNER) else {
            self.error_message =
                Some("BUG: builtin action availability owner is invalid".to_owned());
            return;
        };
        let effect = Effect::Provider(ProviderEffect::ProjectActionAvailability { entries });
        let semantic_key = SemanticKey::new(EffectFamily::Provider, ACTION_AVAILABILITY_SUBJECT);
        if let Err(error) =
            self.register_pending_effect(owner, semantic_key, effect, RetryPolicy::Never)
        {
            self.error_message = Some(error.to_string());
        }
    }

    pub(super) fn publish_action_availability(
        &mut self,
        correlation: Correlation,
        entries: Vec<ActionAvailability>,
    ) {
        let Some(snapshot) = self.action_registry_snapshot.as_ref() else {
            return;
        };
        let generation = AvailabilityGeneration::new(correlation, entries);
        match snapshot.publish_availability(generation) {
            Ok(published) => self.action_registry_snapshot = Some(published),
            Err(error) => self.error_message = Some(error.to_string()),
        }
    }
}

fn authoritative_entries_match(
    snapshot: &crate::domain::action_registry::ActionRegistrySnapshot,
    entries: &[ActionAvailability],
) -> bool {
    let correlation = snapshot.availability_correlation();
    correlation.owner.as_str() == ACTION_AVAILABILITY_OWNER
        && correlation.semantic_key.family() == EffectFamily::Provider
        && correlation.semantic_key.subject() == ACTION_AVAILABILITY_SUBJECT
        && snapshot.availability_entries() == entries
}

fn availability_entries(
    state: &AppState,
    actions: &[crate::domain::action_registry::Action],
) -> Vec<ActionAvailability> {
    actions
        .iter()
        .map(|action| {
            let availability = unavailable_reason(state, action.id.as_str()).map_or(
                Availability::Available,
                |reason| Availability::Unavailable {
                    reason: reason.to_owned(),
                },
            );
            ActionAvailability::new(action.id.clone(), availability)
        })
        .collect()
}

fn unavailable_reason(state: &AppState, action: &str) -> Option<&'static str> {
    match action {
        "issues.list-send-agent" if state.issues_state.selected_issue_index().is_none() => {
            Some("No issue selected")
        }
        "prs.list-send-agent" if state.prs_state.selected_pr_index().is_none() => {
            Some("No pull request selected")
        }
        "prs.comment" => pr_comment_reason(state),
        "prs.reply" => pr_reply_reason(state),
        "prs.resolve" => pr_resolve_reason(state),
        "prs.edit" => Some(ReadOnlyHintKind::ReadOnlyNotEditable.reason()),
        "prs.list-browser" | "prs.open-browser" if !pr_target_present(state) => {
            Some(ReadOnlyHintKind::NoSelectionToOpen.reason())
        }
        "prs.open-merge" => pr_merge_reason(state),
        "issues.list-send-agent"
        | "issues.send-agent"
        | "prs.list-send-agent"
        | "prs.send-agent"
            if !agent_chooser_available(state) =>
        {
            Some(NO_AGENTS_AVAILABLE)
        }
        "issues.open-close" | "issues.detail-close" => issue_close_reason(state),
        "issues.open-delete" | "issues.detail-delete" if state.focused_issue_number().is_none() => {
            Some(ReadOnlyHintKind::NoIssueFocused.reason())
        }
        _ => None,
    }
}

fn pr_comment_reason(state: &AppState) -> Option<&'static str> {
    matches!(
        state.prs_state.detail_subfocus,
        PrDetailSubfocus::Review(_)
            | PrDetailSubfocus::ReviewThread(_)
            | PrDetailSubfocus::Check(_)
    )
    .then_some(ReadOnlyHintKind::ReadOnlyNoComment.reason())
}

fn pr_reply_reason(state: &AppState) -> Option<&'static str> {
    (!matches!(
        state.prs_state.detail_subfocus,
        PrDetailSubfocus::Comment(_) | PrDetailSubfocus::ReviewThread(_)
    ))
    .then_some(ReadOnlyHintKind::ReadOnlyReplyOnComment.reason())
}

fn pr_resolve_reason(state: &AppState) -> Option<&'static str> {
    (!matches!(
        state.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(_)
    ))
    .then_some(ReadOnlyHintKind::ReadOnlyResolveOnThread.reason())
}

fn pr_target_present(state: &AppState) -> bool {
    match state.prs_state.pr_focus {
        PrFocus::PrList => state.prs_state.selected_pr_index().is_some(),
        PrFocus::PrDetail | PrFocus::PrChanges => state.prs_state.pr_detail.is_some(),
        PrFocus::RepoList => false,
    }
}

fn pr_merge_reason(state: &AppState) -> Option<&'static str> {
    match state.prs_state.pr_detail.as_ref() {
        None => Some(ReadOnlyHintKind::NoPrToMerge.reason()),
        Some(detail) if detail.state != PrState::Open => {
            Some(ReadOnlyHintKind::PrNotMergeable.reason())
        }
        Some(_) => None,
    }
}

fn issue_close_reason(state: &AppState) -> Option<&'static str> {
    match state.focused_issue_state() {
        None => Some(ReadOnlyHintKind::NoIssueFocused.reason()),
        Some(IssueState::Closed) => Some(ReadOnlyHintKind::IssueAlreadyClosed.reason()),
        Some(IssueState::Open) => None,
    }
}

fn agent_chooser_available(state: &AppState) -> bool {
    let repo_id = state.selected_repository_id();
    !state.chooser_agents_for_repository(repo_id).is_empty()
        || state.is_transient_available_for_repo(repo_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Id;
    use crate::domain::effects::{EffectResponse, ProviderResponse};
    use crate::messages::{AppMessage, RepositoryAgentMessage};
    use crate::state::transition::TransitionExt;

    fn state_with_snapshot() -> AppState {
        let result = crate::persistence::keymap_edit::compose_published(
            &crate::persistence::settings_document::PublishedSettings::default(),
            "test",
        );
        let Ok(composed) = result else {
            panic!("test snapshot must compose: {result:?}");
        };
        AppState {
            action_registry_snapshot: Some(composed.snapshot().clone()),
            ..AppState::default()
        }
    }

    #[test]
    fn list_send_requires_a_selected_issue_or_pull_request() {
        let mut state = AppState::default();
        state.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::Issues);
        state.issues_state.issue_focus = crate::state::IssueFocus::IssueList;
        assert_eq!(
            unavailable_reason(&state, "issues.list-send-agent"),
            Some("No issue selected")
        );

        state.nav =
            crate::state::navigation::NavState::rooted(crate::state::ScreenId::PullRequests);
        state.prs_state.pr_focus = PrFocus::PrList;
        assert_eq!(
            unavailable_reason(&state, "prs.list-send-agent"),
            Some("No pull request selected")
        );
    }

    #[test]
    fn current_capability_reasons_match_the_existing_notice_authority() {
        let mut state = state_with_snapshot();
        state.prs_state.pr_focus = PrFocus::PrDetail;
        state.prs_state.pr_detail = None;
        let actions = state
            .action_registry_snapshot
            .as_ref()
            .map(|snapshot| snapshot.actions().to_vec())
            .unwrap_or_default();
        let entries = availability_entries(&state, &actions);
        let reason_for = |id: &str| {
            entries
                .iter()
                .find(|entry| entry.action().as_str() == id)
                .and_then(|entry| match entry.availability() {
                    Availability::Available => None,
                    Availability::Unavailable { reason } => Some(reason.as_str()),
                })
        };
        assert_eq!(
            reason_for("prs.open-merge"),
            Some(ReadOnlyHintKind::NoPrToMerge.reason())
        );
        assert_eq!(
            reason_for("prs.open-browser"),
            Some(ReadOnlyHintKind::NoSelectionToOpen.reason())
        );
        assert_eq!(reason_for("prs.send-agent"), Some(NO_AGENTS_AVAILABLE));
        assert_eq!(
            reason_for("issues.list-send-agent"),
            Some("No issue selected")
        );
        assert_eq!(
            reason_for("prs.list-send-agent"),
            Some("No pull request selected")
        );
    }

    /// Issue #633: the send-agent action must not be projected `Unavailable`
    /// merely because the async startup probe has not answered yet. The
    /// projection is what short-circuits the key before the reducer runs, so a
    /// pending verdict here is what makes `Shift+S` do nothing at all.
    #[test]
    fn send_agent_is_available_while_the_startup_probe_is_pending() {
        let mut state = state_with_snapshot();
        let type_id = crate::domain::shipped_agent_type(3);
        let definition = crate::domain::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id == type_id)
            .unwrap_or_else(|| panic!("shipped_agent_type(3) must have a shipped definition"));

        let mut repository = crate::domain::Repository::new(
            crate::domain::RepositoryId("repo-1".to_string()),
            type_id.clone(),
            crate::domain::TypedMap::new(),
            "Test Repo".to_string(),
            "repo-1".to_string(),
            std::path::PathBuf::from("/tmp/test"),
        );
        repository.github_repo = "owner/repo".to_string();
        state.repositories.push(repository);
        state.selected_repository_index = Some(0);

        let mut agent = crate::domain::Agent::new(
            crate::domain::AgentId("agent-1".to_string()),
            crate::domain::RepositoryId("repo-1".to_string()),
            type_id.clone(),
            crate::domain::TypedMap::new(),
            "My Agent".to_string(),
            std::path::PathBuf::from("/tmp/a1"),
        );
        agent.type_id = type_id;
        state.agents.push(agent);

        // The probe was dispatched but has not answered: this is the state the
        // app is in for the first seconds of every session on Windows.
        state.agent_type_availability = vec![
            crate::agent_status_view::AgentAvailabilityObservation::pending(
                &definition,
                true,
                1,
                crate::agent_candidate::CandidateResolution::NotFound(Vec::new()),
            ),
        ];
        assert!(
            state.available_agent_type_ids.is_empty(),
            "fixture precondition: the compatible list is still unpopulated"
        );

        let actions = state
            .action_registry_snapshot
            .as_ref()
            .map(|snapshot| snapshot.actions().to_vec())
            .unwrap_or_default();
        let entries = availability_entries(&state, &actions);
        let send_agent = entries
            .iter()
            .find(|entry| entry.action().as_str() == "issues.send-agent")
            .unwrap_or_else(|| panic!("issues.send-agent must be projected"));

        assert!(
            matches!(send_agent.availability(), Availability::Available),
            "a pending probe must not refuse Shift+S, got {:?}",
            send_agent.availability()
        );
    }

    #[test]
    fn stale_owner_generation_and_semantic_key_cannot_publish() {
        let initial = state_with_snapshot();
        let transition = initial.apply_message(AppMessage::RepositoryAgent(
            RepositoryAgentMessage::ProjectActionAvailability,
        ));
        let Ok(transition) = transition else {
            panic!("availability request must commit: {transition:?}");
        };
        let Some(issued) = transition.effects.first() else {
            panic!("availability request must stage one effect");
        };
        let Effect::Provider(ProviderEffect::ProjectActionAvailability { entries }) =
            &issued.effect
        else {
            panic!("availability must use the closed provider variant");
        };
        let baseline = transition.next_state.action_registry_snapshot.clone();
        let issued_correlation = issued.correlation.clone();
        let mut stale_values = Vec::new();
        let mut mismatch = issued_correlation.clone();
        mismatch.owner = Id::parse("core.other").unwrap_or_else(|error| panic!("owner: {error}"));
        stale_values.push(mismatch);
        let mut mismatch = issued_correlation.clone();
        mismatch.screen_generation = mismatch.screen_generation.saturating_add(1);
        stale_values.push(mismatch);
        let mut mismatch = issued_correlation.clone();
        mismatch.activation_generation = mismatch.activation_generation.saturating_add(1);
        stale_values.push(mismatch);
        let mut mismatch = issued_correlation.clone();
        mismatch.semantic_key = SemanticKey::new(EffectFamily::Provider, "other-availability");
        stale_values.push(mismatch);

        let mut state = transition.next_state;
        for stale in stale_values {
            let completion = crate::domain::effects::EffectCompletion {
                correlation: stale,
                result: Ok(EffectResponse::Provider(
                    ProviderResponse::ActionAvailability {
                        entries: entries.clone(),
                    },
                )),
            };
            state = state
                .apply_message(AppMessage::EffectCompletion(Box::new(completion)))
                .committed_pure();
            assert_eq!(state.action_registry_snapshot, baseline);
            assert_eq!(state.pending_effects.len(), 1);
        }
    }
}
