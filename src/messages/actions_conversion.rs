use crate::messages::{ActionsMessage, NavDir, ScrollDir};
use crate::state::AppEvent;

impl From<ActionsMessage> for AppEvent {
    fn from(message: ActionsMessage) -> Self {
        message.into_app_event()
    }
}

impl ActionsMessage {
    pub(super) fn from_app_event(event: AppEvent) -> Self {
        match event {
            AppEvent::EnterActionsMode => Self::EnterMode,
            AppEvent::ExitActionsMode => Self::ExitMode,
            AppEvent::RefocusActionsList => Self::RefocusList,
            AppEvent::ActionsReload => Self::Reload,
            AppEvent::ActionsNavigateUp => Self::Navigate(NavDir::Up),
            AppEvent::ActionsNavigateDown => Self::Navigate(NavDir::Down),
            AppEvent::ActionsNavigatePageUp(page) => Self::Navigate(NavDir::PageUp(page)),
            AppEvent::ActionsNavigatePageDown(page) => Self::Navigate(NavDir::PageDown(page)),
            AppEvent::ActionsNavigateHome => Self::Navigate(NavDir::Home),
            AppEvent::ActionsNavigateEnd => Self::Navigate(NavDir::End),
            AppEvent::ActionsEnter => Self::Enter,
            AppEvent::ActionsCycleFocus => Self::CycleFocus,
            AppEvent::ActionsCycleFocusReverse => Self::CycleFocusReverse,
            AppEvent::ActionsScrollDetailUp => Self::ScrollDetail(ScrollDir::Up),
            AppEvent::ActionsScrollDetailDown => Self::ScrollDetail(ScrollDir::Down),
            AppEvent::ActionsToggleJobExpand => Self::ToggleJobExpand,
            AppEvent::ActionsCollapseJob => Self::CollapseJob,
            AppEvent::ActionsNavigateJobUp => Self::NavigateJob(NavDir::Up),
            AppEvent::ActionsNavigateJobDown => Self::NavigateJob(NavDir::Down),
            AppEvent::ActionsRunsLoaded { .. } => Self::from_runs_loaded(event),
            AppEvent::ActionsRunsLoadFailed { .. } => Self::from_runs_failed(event),
            AppEvent::ActionsRunsPageLoaded { .. } => Self::from_runs_page_loaded(event),
            AppEvent::ActionsRunsPageLoadFailed { .. } => Self::from_runs_page_failed(event),
            AppEvent::ActionsDetailLoaded { .. } => Self::from_detail_loaded(event),
            AppEvent::ActionsDetailLoadFailed { .. } => Self::from_detail_failed(event),
            AppEvent::WorkflowsLoaded { .. } => Self::from_workflows_loaded(event),
            AppEvent::WorkflowsLoadFailed { .. } => Self::from_workflows_failed(event),
            AppEvent::ActionsOpenFilterControls => Self::OpenFilterControls,
            AppEvent::ActionsCloseFilterControls => Self::CloseFilterControls,
            AppEvent::ActionsApplyFilter => Self::ApplyFilter,
            AppEvent::ActionsClearFilter => Self::ClearFilter,
            AppEvent::ActionsClearDraftFilter => Self::ClearDraftFilter,
            AppEvent::ActionsFilterNavigateNext => Self::FilterNavigateNext,
            AppEvent::ActionsFilterNavigatePrev => Self::FilterNavigatePrev,
            AppEvent::ActionsCycleFilterStatus => Self::CycleFilterStatus,
            AppEvent::ActionsFocusSearchInput => Self::FocusSearchInput,
            AppEvent::ActionsBlurSearchInput => Self::BlurSearchInput,
            AppEvent::ActionsSetSearchQuery { query } => Self::SetSearchQuery { query },
            AppEvent::ActionsApplySearch => Self::ApplySearch,
            AppEvent::ActionsClearSearch => Self::ClearSearch,
            AppEvent::ActionsUpdateDraftFilter { field, value } => {
                Self::UpdateDraftFilter { field, value }
            }
            AppEvent::OpenWorkflowDispatch(workflow) => Self::OpenWorkflowDispatch(workflow),
            AppEvent::CloseWorkflowDispatch => Self::CloseWorkflowDispatch,
            AppEvent::WorkflowDispatchSubmitted { .. } => Self::from_dispatch_submitted(event),
            AppEvent::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            } => Self::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            },
            AppEvent::WorkflowDispatchFailed { .. } => Self::from_dispatch_failed(event),
            _ => unreachable!("unhandled event for ActionsMessage: {:?}", event),
        }
    }

    #[must_use]
    pub fn into_app_event(self) -> AppEvent {
        match self {
            Self::EnterMode => AppEvent::EnterActionsMode,
            Self::ExitMode => AppEvent::ExitActionsMode,
            Self::RefocusList => AppEvent::RefocusActionsList,
            Self::Reload => AppEvent::ActionsReload,
            Self::Navigate(dir) => Self::map_navigation(dir),
            Self::Enter => AppEvent::ActionsEnter,
            Self::CycleFocus => AppEvent::ActionsCycleFocus,
            Self::CycleFocusReverse => AppEvent::ActionsCycleFocusReverse,
            Self::ScrollDetail(dir) => Self::map_detail_scroll(dir),
            Self::ToggleJobExpand => AppEvent::ActionsToggleJobExpand,
            Self::CollapseJob => AppEvent::ActionsCollapseJob,
            Self::NavigateJob(dir) => match dir {
                NavDir::Up => AppEvent::ActionsNavigateJobUp,
                // Job navigation is vertical only; treat any non-Up direction
                // (Down, page, home/end, etc.) as Down so the conversion stays
                // total without duplicating the Up arm body.
                _ => AppEvent::ActionsNavigateJobDown,
            },
            Self::RunsLoaded { .. } => Self::into_runs_loaded(self),
            Self::RunsLoadFailed { .. } => Self::into_runs_failed(self),
            Self::RunsPageLoaded { .. } => Self::into_runs_page_loaded(self),
            Self::RunsPageLoadFailed { .. } => Self::into_runs_page_failed(self),
            Self::DetailLoaded { .. } => Self::into_detail_loaded(self),
            Self::DetailLoadFailed { .. } => Self::into_detail_failed(self),
            Self::WorkflowsLoaded { .. } => Self::into_workflows_loaded(self),
            Self::WorkflowsLoadFailed { .. } => Self::into_workflows_failed(self),
            Self::OpenFilterControls => AppEvent::ActionsOpenFilterControls,
            Self::CloseFilterControls => AppEvent::ActionsCloseFilterControls,
            Self::ApplyFilter => AppEvent::ActionsApplyFilter,
            Self::ClearFilter => AppEvent::ActionsClearFilter,
            Self::ClearDraftFilter => AppEvent::ActionsClearDraftFilter,
            Self::FilterNavigateNext => AppEvent::ActionsFilterNavigateNext,
            Self::FilterNavigatePrev => AppEvent::ActionsFilterNavigatePrev,
            Self::CycleFilterStatus => AppEvent::ActionsCycleFilterStatus,
            Self::FocusSearchInput => AppEvent::ActionsFocusSearchInput,
            Self::BlurSearchInput => AppEvent::ActionsBlurSearchInput,
            Self::SetSearchQuery { query } => AppEvent::ActionsSetSearchQuery { query },
            Self::ApplySearch => AppEvent::ActionsApplySearch,
            Self::ClearSearch => AppEvent::ActionsClearSearch,
            Self::UpdateDraftFilter { field, value } => {
                AppEvent::ActionsUpdateDraftFilter { field, value }
            }
            Self::OpenWorkflowDispatch(workflow) => AppEvent::OpenWorkflowDispatch(workflow),
            Self::CloseWorkflowDispatch => AppEvent::CloseWorkflowDispatch,
            Self::WorkflowDispatchSubmitted { .. } => Self::into_dispatch_submitted(self),
            Self::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            } => AppEvent::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            },
            Self::WorkflowDispatchFailed { .. } => Self::into_dispatch_failed(self),
        }
    }

    fn map_navigation(dir: NavDir) -> AppEvent {
        match dir {
            NavDir::Up => AppEvent::ActionsNavigateUp,
            NavDir::Down => AppEvent::ActionsNavigateDown,
            NavDir::PageUp(page) => AppEvent::ActionsNavigatePageUp(page),
            NavDir::PageDown(page) => AppEvent::ActionsNavigatePageDown(page),
            NavDir::Prev => {
                AppEvent::ActionsNavigatePageUp(crate::list_viewport::PageItemCount::new(1))
            }
            NavDir::Next => {
                AppEvent::ActionsNavigatePageDown(crate::list_viewport::PageItemCount::new(1))
            }
            NavDir::Home => AppEvent::ActionsNavigateHome,
            NavDir::End => AppEvent::ActionsNavigateEnd,
        }
    }

    fn map_detail_scroll(dir: ScrollDir) -> AppEvent {
        match dir {
            ScrollDir::Up | ScrollDir::PageUp => AppEvent::ActionsScrollDetailUp,
            ScrollDir::Down | ScrollDir::PageDown => AppEvent::ActionsScrollDetailDown,
        }
    }

    fn from_runs_loaded(event: AppEvent) -> Self {
        match event {
            AppEvent::ActionsRunsLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => Self::RunsLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            },
            _ => unreachable!(),
        }
    }

    fn from_runs_failed(event: AppEvent) -> Self {
        match event {
            AppEvent::ActionsRunsLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            } => Self::RunsLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn from_runs_page_loaded(event: AppEvent) -> Self {
        match event {
            AppEvent::ActionsRunsPageLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => Self::RunsPageLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            },
            _ => unreachable!(),
        }
    }

    fn from_runs_page_failed(event: AppEvent) -> Self {
        match event {
            AppEvent::ActionsRunsPageLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            } => Self::RunsPageLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn from_detail_loaded(event: AppEvent) -> Self {
        match event {
            AppEvent::ActionsDetailLoaded {
                scope_repo_id,
                run_id,
                request_id,
                detail,
            } => Self::DetailLoaded {
                scope_repo_id,
                run_id,
                request_id,
                detail,
            },
            _ => unreachable!(),
        }
    }

    fn from_detail_failed(event: AppEvent) -> Self {
        match event {
            AppEvent::ActionsDetailLoadFailed {
                scope_repo_id,
                run_id,
                request_id,
                error,
            } => Self::DetailLoadFailed {
                scope_repo_id,
                run_id,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn from_workflows_loaded(event: AppEvent) -> Self {
        match event {
            AppEvent::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            } => Self::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            },
            _ => unreachable!(),
        }
    }

    fn from_workflows_failed(event: AppEvent) -> Self {
        match event {
            AppEvent::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => Self::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn from_dispatch_submitted(event: AppEvent) -> Self {
        match event {
            AppEvent::WorkflowDispatchSubmitted {
                scope_repo_id,
                workflow_id,
                ref_name,
                inputs,
            } => Self::WorkflowDispatchSubmitted {
                scope_repo_id,
                workflow_id,
                ref_name,
                inputs,
            },
            _ => unreachable!(),
        }
    }

    fn from_dispatch_failed(event: AppEvent) -> Self {
        match event {
            AppEvent::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            } => Self::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn into_runs_loaded(self) -> AppEvent {
        match self {
            Self::RunsLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => AppEvent::ActionsRunsLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            },
            _ => unreachable!(),
        }
    }

    fn into_runs_failed(self) -> AppEvent {
        match self {
            Self::RunsLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            } => AppEvent::ActionsRunsLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn into_runs_page_loaded(self) -> AppEvent {
        match self {
            Self::RunsPageLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => AppEvent::ActionsRunsPageLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            },
            _ => unreachable!(),
        }
    }

    fn into_runs_page_failed(self) -> AppEvent {
        match self {
            Self::RunsPageLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            } => AppEvent::ActionsRunsPageLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn into_detail_loaded(self) -> AppEvent {
        match self {
            Self::DetailLoaded {
                scope_repo_id,
                run_id,
                request_id,
                detail,
            } => AppEvent::ActionsDetailLoaded {
                scope_repo_id,
                run_id,
                request_id,
                detail,
            },
            _ => unreachable!(),
        }
    }

    fn into_detail_failed(self) -> AppEvent {
        match self {
            Self::DetailLoadFailed {
                scope_repo_id,
                run_id,
                request_id,
                error,
            } => AppEvent::ActionsDetailLoadFailed {
                scope_repo_id,
                run_id,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn into_workflows_loaded(self) -> AppEvent {
        match self {
            Self::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            } => AppEvent::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            },
            _ => unreachable!(),
        }
    }

    fn into_workflows_failed(self) -> AppEvent {
        match self {
            Self::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => AppEvent::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }

    fn into_dispatch_submitted(self) -> AppEvent {
        match self {
            Self::WorkflowDispatchSubmitted {
                scope_repo_id,
                workflow_id,
                ref_name,
                inputs,
            } => AppEvent::WorkflowDispatchSubmitted {
                scope_repo_id,
                workflow_id,
                ref_name,
                inputs,
            },
            _ => unreachable!(),
        }
    }

    fn into_dispatch_failed(self) -> AppEvent {
        match self {
            Self::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            } => AppEvent::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            },
            _ => unreachable!(),
        }
    }
}
