//! PR lifecycle-mutation conversion sub-handlers for `PullRequestsMessage`
//! (issue #183). Extracted from `prs_conversion.rs`, which is at the
//! source-file-size limit.
//!
//! `AppEvent` carries the whole family wrapped in
//! [`AppEvent::PrLifecycle`]; `PullRequestsMessage` keeps one flat variant per
//! action, so this module is where the two shapes meet. It is also the tail of
//! the PR converter chain: anything that is neither a thread nor a
//! property-editor message lands here.

use crate::state::{AppEvent, PrLifecycleEvent};

use super::{NavDir, PullRequestsMessage};

impl PullRequestsMessage {
    /// Lifecycle-mutation variants, plus the thread/property tail routes.
    pub(super) fn from_app_event_lifecycle(event: AppEvent) -> Self {
        if let Some(message) = Self::from_app_event_thread(&event) {
            return message;
        }
        match event {
            AppEvent::PrLifecycle(lifecycle) => Self::from_pr_lifecycle(*lifecycle),
            property if Self::is_pr_property_app_event(&property) => {
                Self::from_app_event_property(property)
            }
            _ => unreachable!("non-PR AppEvent routed to PR converter"),
        }
    }

    /// One lifecycle event becomes its flat message variant.
    fn from_pr_lifecycle(event: PrLifecycleEvent) -> Self {
        match event {
            PrLifecycleEvent::OpenMergeChooser => Self::OpenMergeChooser,
            PrLifecycleEvent::MergeNavigateUp => Self::MergeNavigate(NavDir::Up),
            PrLifecycleEvent::MergeNavigateDown => Self::MergeNavigate(NavDir::Down),
            PrLifecycleEvent::MergeConfirm => Self::MergeConfirm,
            PrLifecycleEvent::MergeCancel => Self::MergeCancel,
            PrLifecycleEvent::Merged {
                scope_repo_id,
                pr_number,
                method,
            } => Self::Merged {
                scope_repo_id,
                pr_number,
                method,
            },
            PrLifecycleEvent::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => Self::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            },
            PrLifecycleEvent::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            } => Self::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            },
            PrLifecycleEvent::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            } => Self::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            },
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
        AppEvent::from(self.into_pr_lifecycle())
    }

    /// One flat message variant becomes its lifecycle event.
    fn into_pr_lifecycle(self) -> PrLifecycleEvent {
        match self {
            Self::OpenMergeChooser => PrLifecycleEvent::OpenMergeChooser,
            Self::MergeNavigate(NavDir::Up | NavDir::Prev) => PrLifecycleEvent::MergeNavigateUp,
            Self::MergeNavigate(NavDir::Down | NavDir::Next) => PrLifecycleEvent::MergeNavigateDown,
            Self::MergeConfirm => PrLifecycleEvent::MergeConfirm,
            Self::MergeCancel => PrLifecycleEvent::MergeCancel,
            Self::Merged {
                scope_repo_id,
                pr_number,
                method,
            } => PrLifecycleEvent::Merged {
                scope_repo_id,
                pr_number,
                method,
            },
            Self::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            } => PrLifecycleEvent::MergeFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                error,
            },
            Self::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            } => PrLifecycleEvent::MergeMethodsLoaded {
                scope_repo_id,
                pr_number,
                allowed_methods,
            },
            Self::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            } => PrLifecycleEvent::MergeMethodsLoadFailed {
                scope_repo_id,
                pr_number,
                error,
            },
            _ => unreachable!("unrouted PullRequestsMessage variant reached lifecycle converter"),
        }
    }
}
