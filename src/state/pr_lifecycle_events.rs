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
    /// Open the destructive confirm overlay for the focused pull request.
    OpenDeleteConfirm,
    /// Arm the delete overlay, or dispatch the delete once armed.
    DeleteConfirm,
    /// Close the delete overlay without deleting.
    DeleteCancel,
    /// The pull request's head branch was removed, and the pull request was
    /// closed first when `closed` is set.
    Deleted {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        branch: String,
        closed: bool,
    },
    /// The close or the branch removal failed.
    ///
    /// `closed` reports whether the close had already succeeded when the branch
    /// removal failed, so the screen never claims a pull request is open that
    /// GitHub has already closed.
    DeleteFailed {
        scope_repo_id: RepositoryId,
        pr_number: u64,
        mutation_id: u64,
        closed: bool,
        error: String,
    },
    /// Open the New PR composer.
    OpenNewForm,
    /// Close the New PR composer and discard the draft.
    NewFormCancel,
    /// Move to the next composer field.
    NewFormFocusNext,
    /// Move to the previous composer field.
    NewFormFocusPrevious,
    /// Move the focused branch selection towards the start of the list.
    NewFormBranchUp,
    /// Move the focused branch selection towards the end of the list.
    NewFormBranchDown,
    /// Type into the focused text field.
    NewFormChar(char),
    /// Break the body onto a new line.
    NewFormNewline,
    /// Delete the character before the cursor.
    NewFormBackspace,
    /// Delete the character at the cursor.
    NewFormDelete,
    /// Move the cursor one character towards the start.
    NewFormCursorLeft,
    /// Move the cursor one character towards the end.
    NewFormCursorRight,
    /// Move the cursor to the start of the field.
    NewFormCursorHome,
    /// Move the cursor to the end of the field.
    NewFormCursorEnd,
    /// Open the pull request the composer describes.
    NewFormSubmit,
    /// The repository's branches arrived for an open composer.
    BranchesLoaded {
        scope_repo_id: RepositoryId,
        request_id: u64,
        branches: Vec<String>,
        default_branch: Option<String>,
    },
    /// The repository's branches could not be listed.
    BranchesFailed {
        scope_repo_id: RepositoryId,
        request_id: u64,
        error: String,
    },
    /// A pull request was opened.
    Created {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        pr_number: u64,
    },
    /// A pull request could not be opened.
    CreateFailed {
        scope_repo_id: RepositoryId,
        mutation_id: u64,
        error: String,
    },
}

impl From<PrLifecycleEvent> for AppEvent {
    fn from(event: PrLifecycleEvent) -> Self {
        Self::PrLifecycle(Box::new(event))
    }
}
