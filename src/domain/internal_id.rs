/// Closed host-internal identifiers whose spellings are fixed at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalId {
    OverlayQuery,
    OverlayDecision,
    OverlayDeleteWorkDir,
    OverlaySubmit,
    ProviderRetry,
    ResourceSemanticKey,
    ResourceHeadSha,
    GitHubIssueResource,
    GitHubPullRequestResource,
    RepositoryItem,
    AgentItem,
    SessionItem,
    StatusBucketItem,
    WorkbenchCardItem,
}

impl InternalId {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::OverlayQuery => "query",
            Self::OverlayDecision => "decision",
            Self::OverlayDeleteWorkDir => "delete-work-dir",
            Self::OverlaySubmit => "submit",
            Self::ProviderRetry => "retry",
            Self::ResourceSemanticKey => "semantic-key",
            Self::ResourceHeadSha => "head-sha",
            Self::GitHubIssueResource => "github.issue",
            Self::GitHubPullRequestResource => "github.pull-request",
            Self::RepositoryItem => "host-repository",
            Self::AgentItem => "host-agent",
            Self::SessionItem => "host-session",
            Self::StatusBucketItem => "host-status-bucket",
            Self::WorkbenchCardItem => "host-workbench-card",
        }
    }
}

/// The exact closed internal id of a confirmation's decision row.
///
/// The bin crate (mouse routing) needs this without exposing `Id::internal`; it
/// is the only place a raw `"decision"` comparison used to live.
#[must_use]
pub fn overlay_decision_id() -> crate::domain::Id {
    crate::domain::Id::internal(InternalId::OverlayDecision)
}
