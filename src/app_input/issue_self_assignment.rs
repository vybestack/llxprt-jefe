//! Pure decision logic for the non-blocking issue self-assignment that runs
//! after a successful issue-driven send (issue #186).
//!
//! Extracted from `issues_send.rs` to keep that orchestration file under the
//! source-file length limit. Everything here is pure: it consumes the launch
//! outcome and the carried assignment intent and returns a deterministic
//! [`IssueAssignmentAction`]. The orchestration boundary (`issues_send.rs`)
//! applies the action's side effects (spawn the background assignment, surface
//! a warning, or do nothing). Keeping the decision pure lets the central
//! success/failure × resolved/unavailable gating matrix be unit-tested without
//! a runtime or network seam.

use jefe::domain::GitHubRepoRef;

/// Split a validated `owner/repo` shortform into its two components. Returns
/// `None` unless there are exactly two non-empty components, so a malformed
/// value (`owner/repo/extra`, `/repo`, `owner/`) cannot silently alter the
/// assignees REST endpoint path (issue #186).
pub(super) fn split_owner_repo(owner_repo: &str) -> Option<(String, String)> {
    let mut parts = owner_repo.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    // Reject a third segment so `owner/repo/extra` is not silently accepted.
    if parts.next().is_some() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// Parameters for the non-blocking self-assignment task. Captured by value so
/// the background closure is `'static`. `owner_repo` is the validated
/// `owner/repo` shortform carried through for the warning message.
#[derive(Debug)]
pub struct SelfAssignment {
    pub owner: String,
    pub repo: String,
    pub owner_repo: String,
    pub issue_number: u64,
}

impl SelfAssignment {
    /// Build from a validated tracker [`GitHubRepoRef`] and the issue number
    /// carried by the send payload (issue #266).
    ///
    /// The `tracker` is the effective issue/PR tracker target (upstream when
    /// `github_issue_pr_repo` is set, `github_repo` otherwise), NOT the clone
    /// identity. This decouples assignment from clone identity: a fork clones
    /// from one repo but assigns the issue on the upstream tracker. Returns
    /// `None` when the tracker is missing or malformed (no assignment
    /// attempted).
    pub(super) fn from_send_context(
        tracker: Option<&GitHubRepoRef>,
        issue_number: u64,
    ) -> Option<Self> {
        let reference = tracker?;
        Some(Self {
            owner: reference.owner().to_owned(),
            repo: reference.repo().to_owned(),
            owner_repo: reference.full(),
            issue_number,
        })
    }

    /// Project to the state-level follow-up carried through the preflight
    /// modal (issue #186). The modal lives in the `state` layer, which cannot
    /// depend on this binary-crate type.
    pub(super) fn to_state(&self) -> jefe::state::IssueSelfAssignmentFollowUp {
        jefe::state::IssueSelfAssignmentFollowUp::Resolved {
            owner_repo: self.owner_repo.clone(),
            issue_number: self.issue_number,
        }
    }

    /// Reconstruct from the state-level follow-up after a post-preflight
    /// launch. Returns `None` unless the carried variant is `Resolved` with a
    /// re-valid `owner/repo` split.
    pub(super) fn from_state(state: &jefe::state::IssueSelfAssignmentFollowUp) -> Option<Self> {
        let (owner_repo, issue_number) = match state {
            jefe::state::IssueSelfAssignmentFollowUp::Resolved {
                owner_repo,
                issue_number,
            } => (owner_repo.clone(), *issue_number),
            jefe::state::IssueSelfAssignmentFollowUp::Unavailable { .. } => return None,
        };
        let (owner, repo) = split_owner_repo(&owner_repo)?;
        Some(Self {
            owner,
            repo,
            owner_repo,
            issue_number,
        })
    }
}

/// The assignment intent carried through the issue-driven launch path: the
/// issue number (always known) plus the optional resolved identity. Bundling
/// them keeps the launch/preflight helpers under the argument-count limit
/// (issue #186).
pub struct IssueAssignment {
    pub issue_number: u64,
    pub assignment: Option<SelfAssignment>,
}

/// The reason used when an issue-driven launch has no valid repository
/// identity to self-assign against (issue #186). Kept as a constant so the
/// direct and post-preflight paths surface an identical warning.
pub(super) const NO_REPO_IDENTITY_REASON: &str = "No valid GitHub repo (owner/repo) configured for \
     this agent's repository; could not self-assign the issue";

impl IssueAssignment {
    /// Build the intent from the effective tracker [`GitHubRepoRef`] and the
    /// issue number (issue #266). When the tracker is missing/invalid,
    /// `assignment` is `None` so the launch path can surface a warning instead
    /// of silently skipping.
    pub(super) fn from_send_context(tracker: Option<&GitHubRepoRef>, issue_number: u64) -> Self {
        Self {
            issue_number,
            assignment: SelfAssignment::from_send_context(tracker, issue_number),
        }
    }

    /// Project the assignment intent to the state-level follow-up carried
    /// through the preflight modal. Distinguishes a resolved target from an
    /// unavailable one so the post-preflight path can still warn when the
    /// repository identity is missing (issue #186).
    pub(super) fn carried(&self) -> jefe::state::IssueSelfAssignmentFollowUp {
        use jefe::state::IssueSelfAssignmentFollowUp as FollowUp;
        match &self.assignment {
            Some(resolved) => resolved.to_state(),
            None => FollowUp::Unavailable {
                issue_number: self.issue_number,
                reason: NO_REPO_IDENTITY_REASON.to_string(),
            },
        }
    }
}

/// The deterministic follow-up action selected after an issue-driven launch
/// (issue #186). Pure: the orchestration paths compute this from the launch
/// outcome + carried intent, then the boundary applies the side effect. This
/// keeps the central gating decision unit-testable without a runtime/network
/// seam.
#[derive(Debug)]
pub(super) enum IssueAssignmentAction {
    /// Launch succeeded and a valid repository identity is available: start
    /// the non-blocking viewer self-assignment.
    Spawn(SelfAssignment),
    /// Launch succeeded but assignment cannot run (missing/invalid identity,
    /// or a carried `Resolved` shortform that failed revalidation): surface a
    /// non-blocking warning so the user sees the issue was not self-assigned.
    Warn {
        owner_repo: String,
        issue_number: u64,
        reason: String,
    },
    /// Launch failed, or this was not an issue-driven launch: do nothing. The
    /// launch failure is already the authoritative visible error.
    None,
}

/// Decide the direct-path follow-up after an issue-driven launch attempt
/// (issue #186). Pure so the success/failure × resolved/unavailable decision
/// matrix can be unit-tested.
pub(super) fn direct_assignment_action(
    launched: bool,
    assignment: IssueAssignment,
) -> IssueAssignmentAction {
    if !launched {
        return IssueAssignmentAction::None;
    }
    match assignment.assignment {
        Some(resolved) => IssueAssignmentAction::Spawn(resolved),
        None => IssueAssignmentAction::Warn {
            owner_repo: String::new(),
            issue_number: assignment.issue_number,
            reason: NO_REPO_IDENTITY_REASON.to_string(),
        },
    }
}

/// Decide the post-preflight follow-up from the carried modal state after a
/// resumed launch (issue #186). Pure so the launch-success × carried-variant
/// decision matrix can be unit-tested. `None` (non-issue launch) or a failed
/// resumed launch both yield `None`.
pub(super) fn post_preflight_assignment_action(
    launch_ok: bool,
    carried: Option<&jefe::state::IssueSelfAssignmentFollowUp>,
) -> IssueAssignmentAction {
    use jefe::state::IssueSelfAssignmentFollowUp as FollowUp;
    if !launch_ok {
        return IssueAssignmentAction::None;
    }
    let Some(carried) = carried else {
        return IssueAssignmentAction::None;
    };
    match carried {
        FollowUp::Resolved {
            owner_repo,
            issue_number,
        } => match SelfAssignment::from_state(carried) {
            Some(resolved) => IssueAssignmentAction::Spawn(resolved),
            // A carried Resolved that fails revalidation is defensive: surface
            // a warning rather than silently discarding it (issue #186).
            None => IssueAssignmentAction::Warn {
                owner_repo: owner_repo.clone(),
                issue_number: *issue_number,
                reason: "Invalid GitHub repository identity carried through preflight".to_string(),
            },
        },
        FollowUp::Unavailable {
            issue_number,
            reason,
        } => IssueAssignmentAction::Warn {
            owner_repo: String::new(),
            issue_number: *issue_number,
            reason: reason.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IssueAssignment, IssueAssignmentAction, direct_assignment_action,
        post_preflight_assignment_action, split_owner_repo,
    };
    use jefe::domain::GitHubRepoRef;
    use jefe::state::IssueSelfAssignmentFollowUp as FollowUp;

    #[test]
    fn split_owner_repo_valid() {
        assert_eq!(
            split_owner_repo("vybestack/llxprt-jefe"),
            Some(("vybestack".to_string(), "llxprt-jefe".to_string()))
        );
    }

    #[test]
    fn split_owner_repo_no_slash_is_none() {
        assert_eq!(split_owner_repo("just-a-slug"), None);
    }

    #[test]
    fn split_owner_repo_empty_component_is_none() {
        assert_eq!(split_owner_repo("/repo"), None);
        assert_eq!(split_owner_repo("owner/"), None);
    }

    #[test]
    fn split_owner_repo_dot_in_repo_name_keeps_two_components() {
        // A validated `owner/repo` may contain a dot (e.g. owner/repo.name);
        // the split must keep the dotted repo as a single component.
        assert_eq!(
            split_owner_repo("owner/repo.name"),
            Some(("owner".to_string(), "repo.name".to_string()))
        );
    }

    #[test]
    fn split_owner_repo_rejects_extra_components() {
        // A third segment (owner/repo/extra) must not silently resolve to the
        // first two — it could alter the REST endpoint path unexpectedly.
        assert_eq!(split_owner_repo("owner/repo/extra"), None);
    }

    #[test]
    fn split_owner_repo_trims_surrounding_whitespace() {
        // The source is the validated tracker shortform; surrounding
        // whitespace around the two components is trimmed.
        assert_eq!(
            split_owner_repo("owner / repo"),
            Some(("owner".to_string(), "repo".to_string()))
        );
    }

    // ── direct_assignment_action decision matrix (issue #186) ─────────────
    // Proves the central gating: a successful issue-driven launch starts the
    // self-assignment only with a resolved identity, warns when the identity
    // is unavailable, and does nothing when the launch failed.

    fn resolved_intent() -> IssueAssignment {
        let reference = GitHubRepoRef::parse("vybestack/llxprt-jefe")
            .unwrap_or_else(|e| panic!("valid owner/repo must parse: {e}"))
            .unwrap_or_else(|| panic!("valid owner/repo must yield Some"));
        IssueAssignment::from_send_context(Some(&reference), 186)
    }

    fn unavailable_intent() -> IssueAssignment {
        IssueAssignment::from_send_context(None, 186)
    }

    #[test]
    fn missing_tracker_produces_no_assignment() {
        assert!(
            IssueAssignment::from_send_context(None, 186)
                .assignment
                .is_none()
        );
    }

    #[test]
    fn direct_launch_success_with_resolved_identity_spawns_assignment() {
        let action = direct_assignment_action(true, resolved_intent());
        match action {
            IssueAssignmentAction::Spawn(assignment) => {
                assert_eq!(assignment.owner, "vybestack");
                assert_eq!(assignment.repo, "llxprt-jefe");
                assert_eq!(assignment.issue_number, 186);
            }
            other => panic!("resolved+success must Spawn, got {other:?}"),
        }
    }

    #[test]
    fn direct_launch_success_with_unavailable_identity_warns() {
        let action = direct_assignment_action(true, unavailable_intent());
        match action {
            IssueAssignmentAction::Warn {
                owner_repo,
                issue_number,
                reason,
            } => {
                assert!(owner_repo.is_empty());
                assert_eq!(issue_number, 186);
                assert!(
                    reason.contains("No valid GitHub repo"),
                    "warn reason must explain missing repo: {reason}"
                );
            }
            other => panic!("unavailable+success must Warn, got {other:?}"),
        }
    }

    #[test]
    fn direct_launch_failure_with_resolved_identity_is_none() {
        let action = direct_assignment_action(false, resolved_intent());
        assert!(
            matches!(action, IssueAssignmentAction::None),
            "a failed launch must not start or warn about assignment, got {action:?}"
        );
    }

    #[test]
    fn direct_launch_failure_with_unavailable_identity_is_none() {
        // The launch failure is authoritative; no assignment warning must
        // mask it (issue #186).
        let action = direct_assignment_action(false, unavailable_intent());
        assert!(
            matches!(action, IssueAssignmentAction::None),
            "a failed launch must not emit an assignment warning, got {action:?}"
        );
    }

    // ── post_preflight_assignment_action decision matrix (issue #186) ─────

    #[test]
    fn post_preflight_success_resolved_fires_assignment() {
        let carried = resolved_intent().carried();
        let action = post_preflight_assignment_action(true, Some(&carried));
        match action {
            IssueAssignmentAction::Spawn(assignment) => {
                assert_eq!(assignment.owner_repo, "vybestack/llxprt-jefe");
                assert_eq!(assignment.issue_number, 186);
            }
            other => panic!("resolved+post-preflight success must Spawn, got {other:?}"),
        }
    }

    #[test]
    fn post_preflight_success_unavailable_warns() {
        let carried = unavailable_intent().carried();
        let action = post_preflight_assignment_action(true, Some(&carried));
        match action {
            IssueAssignmentAction::Warn { issue_number, .. } => assert_eq!(issue_number, 186),
            other => panic!("unavailable+post-preflight success must Warn, got {other:?}"),
        }
    }

    #[test]
    fn post_preflight_failure_resolved_is_none() {
        let carried = resolved_intent().carried();
        let action = post_preflight_assignment_action(false, Some(&carried));
        assert!(
            matches!(action, IssueAssignmentAction::None),
            "failed resumed launch must not assign, got {action:?}"
        );
    }

    #[test]
    fn post_preflight_failure_unavailable_is_none() {
        let carried = unavailable_intent().carried();
        let action = post_preflight_assignment_action(false, Some(&carried));
        assert!(
            matches!(action, IssueAssignmentAction::None),
            "failed resumed launch must not warn about assignment, got {action:?}"
        );
    }

    #[test]
    fn post_preflight_non_issue_launch_is_none() {
        // A non-issue preflight launch carries no follow-up: no assignment.
        let action = post_preflight_assignment_action(true, None);
        assert!(
            matches!(action, IssueAssignmentAction::None),
            "non-issue launch must not assign, got {action:?}"
        );
    }

    #[test]
    fn post_preflight_malformed_resolved_warns_not_silent() {
        // A carried Resolved whose shortform fails revalidation must surface a
        // defensive warning rather than silently disappearing (issue #186).
        let malformed = FollowUp::Resolved {
            owner_repo: "not-a-valid-shortform".to_string(),
            issue_number: 186,
        };
        let action = post_preflight_assignment_action(true, Some(&malformed));
        match action {
            IssueAssignmentAction::Warn {
                owner_repo,
                issue_number,
                reason,
            } => {
                assert_eq!(owner_repo, "not-a-valid-shortform");
                assert_eq!(issue_number, 186);
                assert!(
                    reason.contains("Invalid GitHub repository identity"),
                    "malformed carried state must warn: {reason}"
                );
            }
            other => panic!("malformed carried state must Warn, got {other:?}"),
        }
    }
}
