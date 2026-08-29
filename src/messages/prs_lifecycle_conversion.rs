//! PR lifecycle-mutation conversion sub-handlers for `PullRequestsMessage`
//! (issue #183). Extracted from `prs_conversion.rs`, which is at the
//! source-file-size limit.
//!
//! `AppEvent` carries the whole family wrapped in
//! [`AppEvent::PrLifecycle`]; `PullRequestsMessage` keeps one flat variant per
//! action, so this module is where the two shapes meet. It is also the tail of
//! the PR converter chain: the thread and property routes are claimed first,
//! and any event the PR domain does not claim is returned to the caller via
//! [`ControlFlow::Continue`] instead of panicking.

use std::ops::ControlFlow;

use crate::domain::ErrorSource;
use crate::state::{AppEvent, PrLifecycleEvent};

use super::PullRequestsMessage;
use super::prs::MergeNavDirection;

impl PullRequestsMessage {
    /// Lifecycle-mutation variants, plus the thread/property tail routes.
    ///
    /// Returns [`ControlFlow::Continue`] with the event when it belongs to no
    /// PR converter layer, so the dispatcher can hand it to another domain.
    pub(super) fn from_app_event_lifecycle(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        if let Some(message) = Self::from_app_event_thread(&event) {
            return ControlFlow::Break(message);
        }
        match event {
            AppEvent::PrLifecycle(lifecycle) => match Self::from_pr_lifecycle(*lifecycle) {
                ControlFlow::Break(message) => ControlFlow::Break(message),
                ControlFlow::Continue(residual) => {
                    ControlFlow::Continue(AppEvent::PrLifecycle(Box::new(residual)))
                }
            },
            property if Self::is_pr_property_app_event(&property) => {
                ControlFlow::Break(Self::from_app_event_property(property))
            }
            other => ControlFlow::Continue(other),
        }
    }

    /// Merge events become their flat message variants.
    fn from_pr_lifecycle(event: PrLifecycleEvent) -> ControlFlow<Self, PrLifecycleEvent> {
        match event {
            PrLifecycleEvent::OpenMergeChooser => ControlFlow::Break(Self::OpenMergeChooser),
            PrLifecycleEvent::MergeNavigateUp => {
                ControlFlow::Break(Self::MergeNavigate(MergeNavDirection::Up))
            }
            PrLifecycleEvent::MergeNavigateDown => {
                ControlFlow::Break(Self::MergeNavigate(MergeNavDirection::Down))
            }
            PrLifecycleEvent::MergeConfirm => ControlFlow::Break(Self::MergeConfirm),
            PrLifecycleEvent::MergeCancel => ControlFlow::Break(Self::MergeCancel),
            PrLifecycleEvent::Merged {
                scope_repo_id,
                pr_number,
                method,
            } => ControlFlow::Break(Self::Merged {
                scope_repo_id,
                pr_number,
                method,
            }),
            PrLifecycleEvent::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => ControlFlow::Break(Self::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            }),
            PrLifecycleEvent::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            } => ControlFlow::Break(Self::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            }),
            PrLifecycleEvent::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            } => ControlFlow::Break(Self::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            }),
            delete_or_composer => Self::from_pr_delete(delete_or_composer),
        }
    }

    /// Delete events become their flat message variants.
    fn from_pr_delete(event: PrLifecycleEvent) -> ControlFlow<Self, PrLifecycleEvent> {
        match event {
            PrLifecycleEvent::OpenDeleteConfirm => ControlFlow::Break(Self::OpenDeleteConfirm),
            PrLifecycleEvent::DeleteConfirm => ControlFlow::Break(Self::DeleteConfirm),
            PrLifecycleEvent::DeleteCancel => ControlFlow::Break(Self::DeleteCancel),
            PrLifecycleEvent::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            } => ControlFlow::Break(Self::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            }),
            PrLifecycleEvent::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            } => ControlFlow::Break(Self::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            }),
            composer => Self::from_pr_composer(composer),
        }
    }

    /// New PR composer events (issue #183).
    ///
    /// Terminal of the `PrLifecycleEvent` chain: composer variants are claimed
    /// here and any residual returns to the dispatcher, which reports it as a
    /// captured converter-drift error instead of panicking.
    fn from_pr_composer(event: PrLifecycleEvent) -> ControlFlow<Self, PrLifecycleEvent> {
        match event {
            PrLifecycleEvent::OpenNewForm => ControlFlow::Break(Self::OpenNewForm),
            PrLifecycleEvent::NewFormCancel => ControlFlow::Break(Self::NewFormCancel),
            PrLifecycleEvent::NewFormFocusNext => ControlFlow::Break(Self::NewFormFocusNext),
            PrLifecycleEvent::NewFormFocusPrevious => {
                ControlFlow::Break(Self::NewFormFocusPrevious)
            }
            PrLifecycleEvent::NewFormBranchUp => ControlFlow::Break(Self::NewFormBranchUp),
            PrLifecycleEvent::NewFormBranchDown => ControlFlow::Break(Self::NewFormBranchDown),
            PrLifecycleEvent::NewFormChar(character) => {
                ControlFlow::Break(Self::NewFormChar(character))
            }
            PrLifecycleEvent::NewFormNewline => ControlFlow::Break(Self::NewFormNewline),
            PrLifecycleEvent::NewFormBackspace => ControlFlow::Break(Self::NewFormBackspace),
            PrLifecycleEvent::NewFormDelete => ControlFlow::Break(Self::NewFormDelete),
            PrLifecycleEvent::NewFormCursorLeft => ControlFlow::Break(Self::NewFormCursorLeft),
            PrLifecycleEvent::NewFormCursorRight => ControlFlow::Break(Self::NewFormCursorRight),
            PrLifecycleEvent::NewFormCursorHome => ControlFlow::Break(Self::NewFormCursorHome),
            PrLifecycleEvent::NewFormCursorEnd => ControlFlow::Break(Self::NewFormCursorEnd),
            PrLifecycleEvent::NewFormSubmit => ControlFlow::Break(Self::NewFormSubmit),
            PrLifecycleEvent::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            } => ControlFlow::Break(Self::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            }),
            PrLifecycleEvent::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            } => ControlFlow::Break(Self::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            }),
            PrLifecycleEvent::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            } => ControlFlow::Break(Self::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            }),
            PrLifecycleEvent::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            } => ControlFlow::Break(Self::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            }),
            other => ControlFlow::Continue(other),
        }
    }

    /// Lifecycle-mutation variants, plus the thread/property tail routes.
    pub(super) fn into_app_event_lifecycle(self) -> AppEvent {
        if let Some(event) = self.thread_to_app_event() {
            return event;
        }
        if Self::is_pr_property_message(&self) {
            return self.into_app_event_property();
        }
        self.into_pr_lifecycle()
    }

    /// One flat message variant becomes its lifecycle event.
    fn into_pr_lifecycle(self) -> AppEvent {
        match self {
            Self::OpenMergeChooser => AppEvent::from(PrLifecycleEvent::OpenMergeChooser),
            Self::MergeNavigate(MergeNavDirection::Up) => {
                AppEvent::from(PrLifecycleEvent::MergeNavigateUp)
            }
            Self::MergeNavigate(MergeNavDirection::Down) => {
                AppEvent::from(PrLifecycleEvent::MergeNavigateDown)
            }
            Self::MergeConfirm => AppEvent::from(PrLifecycleEvent::MergeConfirm),
            Self::MergeCancel => AppEvent::from(PrLifecycleEvent::MergeCancel),
            Self::Merged {
                scope_repo_id,
                pr_number,
                method,
            } => AppEvent::from(PrLifecycleEvent::Merged {
                scope_repo_id,
                pr_number,
                method,
            }),
            Self::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => AppEvent::from(PrLifecycleEvent::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            }),
            Self::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            } => AppEvent::from(PrLifecycleEvent::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            }),
            Self::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            } => AppEvent::from(PrLifecycleEvent::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            }),
            delete_or_composer => delete_or_composer.into_pr_delete(),
        }
    }

    /// Delete messages become their lifecycle events.
    fn into_pr_delete(self) -> AppEvent {
        match self {
            Self::OpenDeleteConfirm => AppEvent::from(PrLifecycleEvent::OpenDeleteConfirm),
            Self::DeleteConfirm => AppEvent::from(PrLifecycleEvent::DeleteConfirm),
            Self::DeleteCancel => AppEvent::from(PrLifecycleEvent::DeleteCancel),
            Self::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            } => AppEvent::from(PrLifecycleEvent::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            }),
            Self::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            } => AppEvent::from(PrLifecycleEvent::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            }),
            composer => composer.into_pr_composer(),
        }
    }

    /// New PR composer messages (issue #183).
    ///
    /// Terminal of the `PullRequestsMessage` lifecycle chain: composer
    /// messages are claimed here and any residual is reported as a captured
    /// converter-drift error instead of panicking.
    fn into_pr_composer(self) -> AppEvent {
        match self {
            Self::OpenNewForm => AppEvent::from(PrLifecycleEvent::OpenNewForm),
            Self::NewFormCancel => AppEvent::from(PrLifecycleEvent::NewFormCancel),
            Self::NewFormFocusNext => AppEvent::from(PrLifecycleEvent::NewFormFocusNext),
            Self::NewFormFocusPrevious => AppEvent::from(PrLifecycleEvent::NewFormFocusPrevious),
            Self::NewFormBranchUp => AppEvent::from(PrLifecycleEvent::NewFormBranchUp),
            Self::NewFormBranchDown => AppEvent::from(PrLifecycleEvent::NewFormBranchDown),
            Self::NewFormChar(character) => {
                AppEvent::from(PrLifecycleEvent::NewFormChar(character))
            }
            Self::NewFormNewline => AppEvent::from(PrLifecycleEvent::NewFormNewline),
            Self::NewFormBackspace => AppEvent::from(PrLifecycleEvent::NewFormBackspace),
            Self::NewFormDelete => AppEvent::from(PrLifecycleEvent::NewFormDelete),
            Self::NewFormCursorLeft => AppEvent::from(PrLifecycleEvent::NewFormCursorLeft),
            Self::NewFormCursorRight => AppEvent::from(PrLifecycleEvent::NewFormCursorRight),
            Self::NewFormCursorHome => AppEvent::from(PrLifecycleEvent::NewFormCursorHome),
            Self::NewFormCursorEnd => AppEvent::from(PrLifecycleEvent::NewFormCursorEnd),
            Self::NewFormSubmit => AppEvent::from(PrLifecycleEvent::NewFormSubmit),
            other => other.into_pr_composer_result(),
        }
    }

    fn into_pr_composer_result(self) -> AppEvent {
        match self {
            Self::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            } => AppEvent::from(PrLifecycleEvent::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            }),
            Self::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            } => AppEvent::from(PrLifecycleEvent::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            }),
            Self::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            } => AppEvent::from(PrLifecycleEvent::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            }),
            Self::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            } => AppEvent::from(PrLifecycleEvent::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            }),
            other => AppEvent::CaptureSilentError(
                "Unconvertible PR lifecycle message".to_owned(),
                format!("{other:?} matched no PR lifecycle converter"),
                ErrorSource::Panic,
                unix_timestamp(),
            ),
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
    use super::*;

    #[test]
    fn merge_navigate_admits_only_closed_up_down() {
        assert!(matches!(
            AppEvent::from(PullRequestsMessage::MergeNavigate(MergeNavDirection::Up)),
            AppEvent::PrLifecycle(event)
                if matches!(*event, PrLifecycleEvent::MergeNavigateUp)
        ));
        assert!(matches!(
            AppEvent::from(PullRequestsMessage::MergeNavigate(MergeNavDirection::Down)),
            AppEvent::PrLifecycle(event)
                if matches!(*event, PrLifecycleEvent::MergeNavigateDown)
        ));
        assert!(matches!(
            PullRequestsMessage::from_app_event_lifecycle(AppEvent::PrLifecycle(Box::new(
                PrLifecycleEvent::MergeNavigateUp
            ))),
            ControlFlow::Break(PullRequestsMessage::MergeNavigate(MergeNavDirection::Up))
        ));
        assert!(matches!(
            PullRequestsMessage::from_app_event_lifecycle(AppEvent::PrLifecycle(Box::new(
                PrLifecycleEvent::MergeNavigateDown
            ))),
            ControlFlow::Break(PullRequestsMessage::MergeNavigate(MergeNavDirection::Down))
        ));
    }

    #[test]
    fn lifecycle_residual_events_continue_instead_of_panicking() {
        assert!(matches!(
            PullRequestsMessage::from_app_event_lifecycle(AppEvent::Quit),
            ControlFlow::Continue(AppEvent::Quit)
        ));
    }

    #[test]
    fn composer_events_round_trip_through_message_variants() {
        let event = AppEvent::PrLifecycle(Box::new(PrLifecycleEvent::NewFormChar('x')));
        let ControlFlow::Break(message) = PullRequestsMessage::from_app_event_lifecycle(event)
        else {
            panic!("composer event should be claimed by the PR lifecycle chain");
        };
        assert!(matches!(message, PullRequestsMessage::NewFormChar('x')));
        assert!(matches!(
            AppEvent::from(message),
            AppEvent::PrLifecycle(event)
                if matches!(*event, PrLifecycleEvent::NewFormChar('x'))
        ));
    }
}
