use std::ops::ControlFlow;

use crate::domain::ErrorSource;
use crate::messages::{ActionsMessage, NavDir, ScrollDir};
use crate::state::AppEvent;

impl From<ActionsMessage> for AppEvent {
    fn from(message: ActionsMessage) -> Self {
        message.into_app_event()
    }
}

impl ActionsMessage {
    /// Convert an actions-domain [`AppEvent`] into the typed message.
    ///
    /// Each layer claims its own variants and hands the residual to the next
    /// layer; an event no actions layer claims returns
    /// [`ControlFlow::Continue`] so the dispatcher can route it elsewhere.
    pub(super) fn try_from_app_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::EnterActionsMode => ControlFlow::Break(Self::EnterMode),
            AppEvent::ExitActionsMode => ControlFlow::Break(Self::ExitMode),
            AppEvent::RefocusActionsList => ControlFlow::Break(Self::RefocusList),
            AppEvent::ActionsReload => ControlFlow::Break(Self::Reload),
            AppEvent::ActionsNavigateUp => ControlFlow::Break(Self::Navigate(NavDir::Up)),
            AppEvent::ActionsNavigateDown => ControlFlow::Break(Self::Navigate(NavDir::Down)),
            AppEvent::ActionsNavigatePageUp(page) => {
                ControlFlow::Break(Self::Navigate(NavDir::PageUp(page)))
            }
            AppEvent::ActionsNavigatePageDown(page) => {
                ControlFlow::Break(Self::Navigate(NavDir::PageDown(page)))
            }
            AppEvent::ActionsNavigateHome => ControlFlow::Break(Self::Navigate(NavDir::Home)),
            AppEvent::ActionsNavigateEnd => ControlFlow::Break(Self::Navigate(NavDir::End)),
            AppEvent::ActionsEnter => ControlFlow::Break(Self::Enter),
            AppEvent::ActionsCycleFocus => ControlFlow::Break(Self::CycleFocus),
            AppEvent::ActionsCycleFocusReverse => ControlFlow::Break(Self::CycleFocusReverse),
            AppEvent::ActionsScrollDetailUp => {
                ControlFlow::Break(Self::ScrollDetail(ScrollDir::Up))
            }
            AppEvent::ActionsScrollDetailDown => {
                ControlFlow::Break(Self::ScrollDetail(ScrollDir::Down))
            }
            AppEvent::ActionsExpandJob => ControlFlow::Break(Self::ExpandJob),
            AppEvent::ActionsCollapseJob => ControlFlow::Break(Self::CollapseJob),
            AppEvent::ActionsDetailEscape => ControlFlow::Break(Self::DetailEscape),
            AppEvent::ActionsNavigateJobUp => ControlFlow::Break(Self::NavigateJob(NavDir::Up)),
            AppEvent::ActionsNavigateJobDown => ControlFlow::Break(Self::NavigateJob(NavDir::Down)),
            AppEvent::ActionsOpenFilterControls => ControlFlow::Break(Self::OpenFilterControls),
            AppEvent::ActionsCloseFilterControls => ControlFlow::Break(Self::CloseFilterControls),
            AppEvent::ActionsApplyFilter => ControlFlow::Break(Self::ApplyFilter),
            AppEvent::ActionsClearFilter => ControlFlow::Break(Self::ClearFilter),
            AppEvent::ActionsClearDraftFilter => ControlFlow::Break(Self::ClearDraftFilter),
            AppEvent::ActionsFilterNavigateNext => ControlFlow::Break(Self::FilterNavigateNext),
            AppEvent::ActionsFilterNavigatePrev => ControlFlow::Break(Self::FilterNavigatePrev),
            AppEvent::ActionsCycleFilterStatus => ControlFlow::Break(Self::CycleFilterStatus),
            AppEvent::CycleActionsSortByNext => ControlFlow::Break(Self::CycleActionsSortByNext),
            AppEvent::CycleActionsSortByPrev => ControlFlow::Break(Self::CycleActionsSortByPrev),
            AppEvent::ToggleActionsSortOrder => ControlFlow::Break(Self::ToggleActionsSortOrder),
            AppEvent::ActionsFocusSearchInput => ControlFlow::Break(Self::FocusSearchInput),
            AppEvent::ActionsBlurSearchInput => ControlFlow::Break(Self::BlurSearchInput),
            AppEvent::ActionsSetSearchQuery { query } => {
                ControlFlow::Break(Self::SetSearchQuery { query })
            }
            AppEvent::ActionsApplySearch => ControlFlow::Break(Self::ApplySearch),
            AppEvent::ActionsClearSearch => ControlFlow::Break(Self::ClearSearch),
            AppEvent::ActionsUpdateDraftFilter { field, value } => {
                ControlFlow::Break(Self::UpdateDraftFilter { field, value })
            }
            AppEvent::OpenWorkflowDispatch(workflow) => {
                ControlFlow::Break(Self::OpenWorkflowDispatch(workflow))
            }
            AppEvent::CloseWorkflowDispatch => ControlFlow::Break(Self::CloseWorkflowDispatch),
            other => Self::from_app_event_runs(other),
        }
    }

    /// Actions run-list payload events, destructured at the caller.
    fn from_app_event_runs(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::ActionsRunsLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => ControlFlow::Break(Self::RunsLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            }),
            AppEvent::ActionsRunsLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            } => ControlFlow::Break(Self::RunsLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            }),
            AppEvent::ActionsRunsPageLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            } => ControlFlow::Break(Self::RunsPageLoaded {
                scope_repo_id,
                filter,
                page,
                request_id,
                runs,
                has_more,
            }),
            AppEvent::ActionsRunsPageLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            } => ControlFlow::Break(Self::RunsPageLoadFailed {
                scope_repo_id,
                filter,
                page,
                request_id,
                error,
            }),
            other => Self::from_app_event_detail(other),
        }
    }

    /// Actions detail payload events, destructured at the caller.
    fn from_app_event_detail(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::ActionsSetDetailGeometry {
                viewport_rows,
                content_width,
            } => ControlFlow::Break(Self::SetDetailGeometry {
                viewport_rows,
                content_width,
            }),
            AppEvent::ActionsBeginDetailReload {
                scope_repo_id,
                run_id,
                request_id,
            } => ControlFlow::Break(Self::BeginDetailReload {
                scope_repo_id,
                run_id,
                request_id,
            }),
            AppEvent::ActionsDetailLoaded {
                scope_repo_id,
                run_id,
                request_id,
                detail,
            } => ControlFlow::Break(Self::DetailLoaded {
                scope_repo_id,
                run_id,
                request_id,
                detail,
            }),
            AppEvent::ActionsDetailLoadFailed {
                scope_repo_id,
                run_id,
                request_id,
                error,
            } => ControlFlow::Break(Self::DetailLoadFailed {
                scope_repo_id,
                run_id,
                request_id,
                error,
            }),
            other => Self::from_app_event_workflows(other),
        }
    }

    /// Workflow and dispatch payload events, destructured at the caller.
    fn from_app_event_workflows(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::EnterActionsModeWithPrFilter {
                pr_number,
                head_sha,
            } => ControlFlow::Break(Self::EnterModeWithPrFilter {
                pr_number,
                head_sha,
            }),
            AppEvent::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            } => ControlFlow::Break(Self::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            }),
            AppEvent::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => ControlFlow::Break(Self::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            }),
            AppEvent::WorkflowDispatchSubmitted {
                scope_repo_id,
                workflow_id,
                ref_name,
                inputs,
            } => ControlFlow::Break(Self::WorkflowDispatchSubmitted {
                scope_repo_id,
                workflow_id,
                ref_name,
                inputs,
            }),
            AppEvent::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            } => ControlFlow::Break(Self::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            }),
            AppEvent::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            } => ControlFlow::Break(Self::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            }),
            other => ControlFlow::Continue(other),
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
            Self::ExpandJob => AppEvent::ActionsExpandJob,
            Self::CollapseJob => AppEvent::ActionsCollapseJob,
            Self::DetailEscape => AppEvent::ActionsDetailEscape,
            Self::NavigateJob(dir) => match dir {
                NavDir::Up => AppEvent::ActionsNavigateJobUp,
                // Job navigation is vertical only; treat any non-Up direction
                // (Down, page, home/end, etc.) as Down so the conversion stays
                // total without duplicating the Up arm body.
                _ => AppEvent::ActionsNavigateJobDown,
            },
            Self::OpenFilterControls => AppEvent::ActionsOpenFilterControls,
            Self::CloseFilterControls => AppEvent::ActionsCloseFilterControls,
            Self::ApplyFilter => AppEvent::ActionsApplyFilter,
            Self::ClearFilter => AppEvent::ActionsClearFilter,
            Self::ClearDraftFilter => AppEvent::ActionsClearDraftFilter,
            Self::FilterNavigateNext => AppEvent::ActionsFilterNavigateNext,
            Self::FilterNavigatePrev => AppEvent::ActionsFilterNavigatePrev,
            Self::CycleFilterStatus => AppEvent::ActionsCycleFilterStatus,
            Self::CycleActionsSortByNext => AppEvent::CycleActionsSortByNext,
            Self::CycleActionsSortByPrev => AppEvent::CycleActionsSortByPrev,
            Self::ToggleActionsSortOrder => AppEvent::ToggleActionsSortOrder,
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
            other => other.into_app_event_runs(),
        }
    }

    /// Actions run-list payload messages, destructured at the caller.
    fn into_app_event_runs(self) -> AppEvent {
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
            other => other.into_app_event_detail(),
        }
    }

    /// Actions detail payload messages, destructured at the caller.
    fn into_app_event_detail(self) -> AppEvent {
        match self {
            Self::SetDetailGeometry {
                viewport_rows,
                content_width,
            } => AppEvent::ActionsSetDetailGeometry {
                viewport_rows,
                content_width,
            },
            Self::BeginDetailReload {
                scope_repo_id,
                run_id,
                request_id,
            } => AppEvent::ActionsBeginDetailReload {
                scope_repo_id,
                run_id,
                request_id,
            },
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
            other => other.into_app_event_workflows(),
        }
    }

    /// Workflow and dispatch payload messages, destructured at the caller.
    ///
    /// Terminal of the `ActionsMessage` converter chain: a residual means a
    /// new variant was added without a converter arm, which is reported as a
    /// captured converter-drift error instead of panicking.
    fn into_app_event_workflows(self) -> AppEvent {
        match self {
            Self::EnterModeWithPrFilter {
                pr_number,
                head_sha,
            } => AppEvent::EnterActionsModeWithPrFilter {
                pr_number,
                head_sha,
            },
            Self::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            } => AppEvent::WorkflowsLoaded {
                scope_repo_id,
                request_id,
                workflows,
            },
            Self::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            } => AppEvent::WorkflowsLoadFailed {
                scope_repo_id,
                request_id,
                error,
            },
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
            Self::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            } => AppEvent::WorkflowDispatchSuccess {
                scope_repo_id,
                request_id,
            },
            Self::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            } => AppEvent::WorkflowDispatchFailed {
                scope_repo_id,
                request_id,
                error,
            },
            // Terminal of the `ActionsMessage` chain. The layers above claim
            // every other variant; a residual here means a new variant was
            // added without a converter arm, which is reported as a captured
            // converter-drift error instead of panicking.
            other => AppEvent::CaptureSilentError(
                "Unconvertible actions message".to_owned(),
                format!("{other:?} matched no actions converter"),
                ErrorSource::Panic,
                unix_timestamp(),
            ),
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
}

/// Unix epoch seconds used to stamp a captured converter-drift error, matching
/// the panic-capture timestamp convention.
fn unix_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |_| "0".to_owned(),
            |duration| duration.as_secs().to_string(),
        )
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;

    use super::*;
    use crate::domain::RepositoryId;

    #[test]
    fn payload_events_break_with_destructured_messages() {
        let event = AppEvent::EnterActionsModeWithPrFilter {
            pr_number: 7,
            head_sha: "abc123".to_owned(),
        };
        assert!(matches!(
            ActionsMessage::try_from_app_event(event),
            ControlFlow::Break(ActionsMessage::EnterModeWithPrFilter { pr_number: 7, .. })
        ));
        assert!(matches!(
            ActionsMessage::try_from_app_event(AppEvent::WorkflowsLoadFailed {
                scope_repo_id: RepositoryId("root".to_owned()),
                request_id: 1,
                error: "boom".to_owned(),
            }),
            ControlFlow::Break(ActionsMessage::WorkflowsLoadFailed { .. })
        ));
    }

    #[test]
    fn non_actions_events_continue_to_next_domain() {
        assert!(matches!(
            ActionsMessage::try_from_app_event(AppEvent::Quit),
            ControlFlow::Continue(AppEvent::Quit)
        ));
    }

    #[test]
    fn payload_messages_round_trip() {
        let message = ActionsMessage::RunsLoaded {
            scope_repo_id: RepositoryId("root".to_owned()),
            filter: Box::new(crate::domain::ActionsFilter::default()),
            page: 0,
            request_id: 3,
            runs: Vec::new(),
            has_more: false,
        };
        assert!(matches!(
            AppEvent::from(message),
            AppEvent::ActionsRunsLoaded { request_id: 3, .. }
        ));
        let message = ActionsMessage::EnterModeWithPrFilter {
            pr_number: 9,
            head_sha: "def456".to_owned(),
        };
        assert!(matches!(
            AppEvent::from(message),
            AppEvent::EnterActionsModeWithPrFilter { pr_number: 9, .. }
        ));
    }
}
