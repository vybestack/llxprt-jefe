//! Pull-request lifecycle-mutation events (issue #183).
//!
//! Merging, closing, deleting, and creating a pull request are one family of
//! mutations on the same entity, so they travel as one wrapped sub-enum on
//! [`AppEvent::PrLifecycle`] rather than as loose top-level variants. This
//! follows the shape `AppEvent` already uses for
//! [`AppEvent::Observation`](super::AppEvent::Observation) and
//! [`AppEvent::Keys`](super::AppEvent::Keys).
//!
//! Every variant is a pure reducer input: the boundary layers build one of
//! these and the reducer applies it. Nothing here performs I/O.

use crate::domain::{MergeMethod, RepositoryId};

use super::AppEvent;

/// One pull-request lifecycle-mutation event.
#[derive(Debug, Clone)]
pub enum PrLifecycleEvent {
    /// Open the merge-method chooser overlay (issue #92).
    OpenMergeChooser,
    /// Move the merge-method selection towards the start of the method list.
    MergeNavigateUp,
    /// Move the merge-method selection towards the end of the method list.
    MergeNavigateDown,
    /// Arm the merge chooser, or dispatch the merge once armed.
    MergeConfirm,
    /// Close the merge chooser without merging.
    MergeCancel,
    /// A merge completed successfully.
    Merged {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        method: MergeMethod,
    },
    /// A merge failed; the pending mutation is cleared and the error surfaced.
    MergeFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        error: String,
    },
    /// The repository's allowed merge methods resolved.
    MergeMethodsLoaded {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        allowed_methods: Vec<MergeMethod>,
    },
    /// The repository's allowed merge methods could not be resolved.
    MergeMethodsLoadFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        error: String,
    },
}

impl From<PrLifecycleEvent> for AppEvent {
    fn from(event: PrLifecycleEvent) -> Self {
        Self::PrLifecycle(Box::new(event))
    }
}
