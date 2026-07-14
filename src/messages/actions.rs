use crate::domain::{ActionsFilter, RepositoryId, Workflow, WorkflowRun, WorkflowRunDetail};
use crate::messages::{NavDir, ScrollDir};
use crate::state::ActionsFilterField;

/// Actions mode messages.
#[derive(Debug, Clone)]
pub enum ActionsMessage {
    EnterMode,
    EnterModeWithPrFilter {
        pr_number: u64,
        head_sha: String,
    },
    ExitMode,
    RefocusList,
    Reload,
    Navigate(NavDir),
    Enter,
    CycleFocus,
    CycleFocusReverse,
    /// Synchronize the renderer's exact wrapped detail geometry.
    SetDetailGeometry {
        viewport_rows: usize,
        content_width: usize,
    },
    ScrollDetail(ScrollDir),
    ExpandJob,
    CollapseJob,
    DetailEscape,
    NavigateJob(crate::messages::NavDir),

    /// Begin a correlated run-detail reload and clear stale inspection state.
    BeginDetailReload {
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
    },
    RunsLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<ActionsFilter>,
        page: u32,
        request_id: u64,
        runs: Vec<WorkflowRun>,
        has_more: bool,
    },
    RunsLoadFailed {
        scope_repo_id: RepositoryId,
        filter: Box<ActionsFilter>,
        page: u32,
        request_id: u64,
        error: String,
    },
    /// Page append result (load-more). Items are appended, not replaced.
    RunsPageLoaded {
        scope_repo_id: RepositoryId,
        filter: Box<ActionsFilter>,
        page: u32,
        request_id: u64,
        runs: Vec<WorkflowRun>,
        has_more: bool,
    },
    /// Page append failure — clears the pending page so load-more can retry.
    RunsPageLoadFailed {
        scope_repo_id: RepositoryId,
        filter: Box<ActionsFilter>,
        page: u32,
        request_id: u64,
        error: String,
    },
    DetailLoaded {
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
        detail: Box<WorkflowRunDetail>,
    },
    DetailLoadFailed {
        scope_repo_id: RepositoryId,
        run_id: u64,
        request_id: u64,
        error: String,
    },
    WorkflowsLoaded {
        scope_repo_id: RepositoryId,
        request_id: u64,
        workflows: Vec<Workflow>,
    },
    WorkflowsLoadFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },

    OpenFilterControls,
    CloseFilterControls,
    ApplyFilter,
    ClearFilter,
    ClearDraftFilter,
    FilterNavigateNext,
    FilterNavigatePrev,
    CycleFilterStatus,
    FocusSearchInput,
    BlurSearchInput,
    SetSearchQuery {
        query: String,
    },
    ApplySearch,
    ClearSearch,
    UpdateDraftFilter {
        field: ActionsFilterField,
        value: String,
    },

    OpenWorkflowDispatch(Workflow),
    CloseWorkflowDispatch,
    WorkflowDispatchSubmitted {
        scope_repo_id: RepositoryId,
        workflow_id: String,
        ref_name: String,
        inputs: Vec<(String, String)>,
    },
    WorkflowDispatchSuccess {
        scope_repo_id: RepositoryId,
        request_id: u64,
    },
    WorkflowDispatchFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },
}

impl ActionsMessage {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EnterMode => "EnterActionsMode",
            Self::EnterModeWithPrFilter { .. } => "EnterActionsModeWithPrFilter",
            Self::ExitMode => "ExitActionsMode",
            Self::RefocusList => "RefocusActionsList",
            Self::Reload => "ActionsReload",
            Self::Navigate(_) => "ActionsNavigate",
            Self::Enter => "ActionsListEnter",
            Self::CycleFocus => "ActionsCycleFocus",
            Self::CycleFocusReverse => "ActionsCycleFocusReverse",
            Self::SetDetailGeometry { .. } => "ActionsSetDetailGeometry",
            Self::ScrollDetail(_) => "ActionsScrollDetail",
            Self::ExpandJob => "ActionsExpandJob",
            Self::CollapseJob => "ActionsCollapseJob",
            Self::DetailEscape => "ActionsDetailEscape",
            Self::NavigateJob(_) => "ActionsNavigateJob",
            Self::BeginDetailReload { .. } => "ActionsBeginDetailReload",
            Self::RunsLoaded { .. } => "ActionsRunsLoaded",
            Self::RunsLoadFailed { .. } => "ActionsRunsLoadFailed",
            Self::RunsPageLoaded { .. } => "ActionsRunsPageLoaded",
            Self::RunsPageLoadFailed { .. } => "ActionsRunsPageLoadFailed",
            Self::DetailLoaded { .. } => "ActionsDetailLoaded",
            Self::DetailLoadFailed { .. } => "ActionsDetailLoadFailed",
            Self::WorkflowsLoaded { .. } => "WorkflowsLoaded",
            Self::WorkflowsLoadFailed { .. } => "WorkflowsLoadFailed",
            Self::OpenFilterControls => "ActionsOpenFilterControls",
            Self::CloseFilterControls => "ActionsCloseFilterControls",
            Self::ApplyFilter => "ActionsApplyFilter",
            Self::ClearFilter => "ActionsClearFilter",
            Self::ClearDraftFilter => "ActionsClearDraftFilter",
            Self::FilterNavigateNext => "ActionsFilterNavigateNext",
            Self::FilterNavigatePrev => "ActionsFilterNavigatePrev",
            Self::CycleFilterStatus => "ActionsCycleFilterStatus",
            Self::FocusSearchInput => "ActionsFocusSearchInput",
            Self::BlurSearchInput => "ActionsBlurSearchInput",
            Self::SetSearchQuery { .. } => "ActionsSetSearchQuery",
            Self::ApplySearch => "ActionsApplySearch",
            Self::ClearSearch => "ActionsClearSearch",
            Self::UpdateDraftFilter { .. } => "ActionsUpdateDraftFilter",
            Self::OpenWorkflowDispatch(_) => "OpenWorkflowDispatch",
            Self::CloseWorkflowDispatch => "CloseWorkflowDispatch",
            Self::WorkflowDispatchSubmitted { .. } => "WorkflowDispatchSubmitted",
            Self::WorkflowDispatchSuccess { .. } => "WorkflowDispatchSuccess",
            Self::WorkflowDispatchFailed { .. } => "WorkflowDispatchFailed",
        }
    }
}
