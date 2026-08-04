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

    /// Merge events become their flat message variants.
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
            delete_or_composer => Self::from_pr_delete(delete_or_composer),
        }
    }

    /// Delete events become their flat message variants.
    fn from_pr_delete(event: PrLifecycleEvent) -> Self {
        match event {
            PrLifecycleEvent::OpenDeleteConfirm => Self::OpenDeleteConfirm,
            PrLifecycleEvent::DeleteConfirm => Self::DeleteConfirm,
            PrLifecycleEvent::DeleteCancel => Self::DeleteCancel,
            PrLifecycleEvent::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            } => Self::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            },
            PrLifecycleEvent::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            } => Self::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            },
            composer => Self::from_pr_composer(composer),
        }
    }

    /// New PR composer events (issue #183).
    fn from_pr_composer(event: PrLifecycleEvent) -> Self {
        match event {
            PrLifecycleEvent::OpenNewForm => Self::OpenNewForm,
            PrLifecycleEvent::NewFormCancel => Self::NewFormCancel,
            PrLifecycleEvent::NewFormFocusNext => Self::NewFormFocusNext,
            PrLifecycleEvent::NewFormFocusPrevious => Self::NewFormFocusPrevious,
            PrLifecycleEvent::NewFormBranchUp => Self::NewFormBranchUp,
            PrLifecycleEvent::NewFormBranchDown => Self::NewFormBranchDown,
            PrLifecycleEvent::NewFormChar(character) => Self::NewFormChar(character),
            PrLifecycleEvent::NewFormNewline => Self::NewFormNewline,
            PrLifecycleEvent::NewFormBackspace => Self::NewFormBackspace,
            PrLifecycleEvent::NewFormDelete => Self::NewFormDelete,
            PrLifecycleEvent::NewFormCursorLeft => Self::NewFormCursorLeft,
            PrLifecycleEvent::NewFormCursorRight => Self::NewFormCursorRight,
            PrLifecycleEvent::NewFormCursorHome => Self::NewFormCursorHome,
            PrLifecycleEvent::NewFormCursorEnd => Self::NewFormCursorEnd,
            PrLifecycleEvent::NewFormSubmit => Self::NewFormSubmit,
            PrLifecycleEvent::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            } => Self::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            },
            PrLifecycleEvent::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            } => Self::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            },
            PrLifecycleEvent::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            } => Self::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            },
            PrLifecycleEvent::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            } => Self::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            },
            // `from_pr_lifecycle` matches every merge and delete variant before
            // delegating here, so nothing else can arrive.
            other => unreachable!("{other:?} is not a New PR composer event"),
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
            // The merge chooser is a short fixed list with no paging, so any
            // other direction is a programming error. Naming it here keeps the
            // diagnostic accurate instead of surfacing it two hops later as a
            // composer message.
            Self::MergeNavigate(direction) => {
                unreachable!("the merge chooser does not navigate by {direction:?}")
            }
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
            delete_or_composer => delete_or_composer.into_pr_delete(),
        }
    }

    /// Delete messages become their lifecycle events.
    fn into_pr_delete(self) -> PrLifecycleEvent {
        match self {
            Self::OpenDeleteConfirm => PrLifecycleEvent::OpenDeleteConfirm,
            Self::DeleteConfirm => PrLifecycleEvent::DeleteConfirm,
            Self::DeleteCancel => PrLifecycleEvent::DeleteCancel,
            Self::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            } => PrLifecycleEvent::Deleted {
                scope_repo_id,
                pr_number,
                mutation_id,
                branch,
                closed,
            },
            Self::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            } => PrLifecycleEvent::DeleteFailed {
                scope_repo_id,
                pr_number,
                mutation_id,
                closed,
                error,
            },
            composer => composer.into_pr_composer(),
        }
    }

    /// New PR composer messages (issue #183).
    fn into_pr_composer(self) -> PrLifecycleEvent {
        match self {
            Self::OpenNewForm => PrLifecycleEvent::OpenNewForm,
            Self::NewFormCancel => PrLifecycleEvent::NewFormCancel,
            Self::NewFormFocusNext => PrLifecycleEvent::NewFormFocusNext,
            Self::NewFormFocusPrevious => PrLifecycleEvent::NewFormFocusPrevious,
            Self::NewFormBranchUp => PrLifecycleEvent::NewFormBranchUp,
            Self::NewFormBranchDown => PrLifecycleEvent::NewFormBranchDown,
            Self::NewFormChar(character) => PrLifecycleEvent::NewFormChar(character),
            Self::NewFormNewline => PrLifecycleEvent::NewFormNewline,
            Self::NewFormBackspace => PrLifecycleEvent::NewFormBackspace,
            Self::NewFormDelete => PrLifecycleEvent::NewFormDelete,
            Self::NewFormCursorLeft => PrLifecycleEvent::NewFormCursorLeft,
            Self::NewFormCursorRight => PrLifecycleEvent::NewFormCursorRight,
            Self::NewFormCursorHome => PrLifecycleEvent::NewFormCursorHome,
            Self::NewFormCursorEnd => PrLifecycleEvent::NewFormCursorEnd,
            Self::NewFormSubmit => PrLifecycleEvent::NewFormSubmit,
            Self::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            } => PrLifecycleEvent::BranchesLoaded {
                scope_repo_id,
                request_id,
                branches,
                default_branch,
            },
            Self::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            } => PrLifecycleEvent::BranchesFailed {
                scope_repo_id,
                request_id,
                error,
            },
            Self::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            } => PrLifecycleEvent::Created {
                scope_repo_id,
                mutation_id,
                pr_number,
            },
            Self::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            } => PrLifecycleEvent::CreateFailed {
                scope_repo_id,
                mutation_id,
                error,
            },
            other => unreachable!("{other:?} is not a PR lifecycle message"),
        }
    }
}
