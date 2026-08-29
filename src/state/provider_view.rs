//! Pure, iocraft-free provider view projection (issue #390 CW-10, Slice B).
//!
//! This module is the sole authority for what the thin provider UI renders. It
//! is a pure function of the handle-free request state, the shared action
//! availability reason, the existing [`ConfirmFocus`] convention, and the
//! viewport dimensions — no iocraft component, color, or side effect lives
//! here. The renderer reads the projected [`ProviderViewProjection`] and draws
//! it; it never interprets reducer state directly.
//!
//! The projection distinguishes seven visual modes ([`ProviderViewMode`]):
//! [`Normal`], [`Focused`], [`Unavailable`], [`Error`], [`Confirmation`],
//! [`Recovery`], and [`Small`]. The unavailable reason is byte-identical to
//! the action-registry availability so an operator reading the panel and a
//! refused keybind say the same thing.
//!
//! [`Normal`]: ProviderViewMode::Normal
//! [`Focused`]: ProviderViewMode::Focused
//! [`Unavailable`]: ProviderViewMode::Unavailable
//! [`Error`]: ProviderViewMode::Error
//! [`Confirmation`]: ProviderViewMode::Confirmation
//! [`Recovery`]: ProviderViewMode::Recovery
//! [`Small`]: ProviderViewMode::Small

use crate::domain::action_registry::Availability;
use crate::domain::plugin::field::Field;
use crate::runtime::provider::protocol::{Id, TypedMap};
use crate::state::ConfirmFocus;
use crate::state::provider_requests::{
    ActiveRequest, PendingConfirmationView, ProviderRequestState,
};

/// Below this terminal-row count the provider surface renders in [`Small`]
/// mode so a tiny viewport stays usable.
pub const SMALL_VIEWPORT_ROW_THRESHOLD: usize = 12;

/// The focused prefix marker for an active provider request row.
pub const FOCUS_MARKER: &str = ">>";

/// One projected row in the provider view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderViewRow {
    /// The action or request label.
    pub label: String,
    /// The row's status summary.
    pub status: ProviderRowStatus,
    /// Whether this row has keyboard focus.
    pub focused: bool,
}

/// The status text a row carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRowStatus {
    /// No special status.
    None,
    /// The action is unavailable with this shared reason.
    Unavailable(String),
    /// The request is in progress with a progress summary.
    InProgress(String),
    /// The request completed with this operator-facing result summary.
    Completed(String),
    /// The request failed.
    Failed(String),
    /// The request was cancelled.
    Cancelled,
    /// The generation is unavailable.
    GenerationUnavailable(String),
}

/// The seven distinct visual modes (CW10-13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderViewMode {
    /// Idle, actions available, normal viewport.
    Normal,
    /// The provider surface has keyboard focus.
    Focused,
    /// The provider/action is unavailable; the reason is byte-identical to the
    /// action-registry availability.
    Unavailable {
        /// The shared availability reason.
        reason: String,
    },
    /// A provider terminal error is visible.
    Error {
        /// The error message.
        message: String,
    },
    /// A confirmation modal is open (DIRTY/CONFIRMATION).
    Confirmation {
        /// The confirm-focus convention (defaults to [`ConfirmFocus::Cancel`]).
        confirm_focus: ConfirmFocus,
        /// Modal title, byte-identical to the provider declaration.
        title: String,
        /// Modal body, byte-identical to the provider declaration.
        body: String,
        /// Confirm-button label, byte-identical to the provider declaration.
        confirm_label: String,
        /// Exact declared continuation field schema.
        continuation_schema: Vec<Field>,
        /// Exact typed values displayed by the owning screen instance.
        continuation_values: TypedMap,
        /// Provider field currently focused by the owning screen instance.
        focused_field: Option<Id>,
    },
    /// A runtime failure (crash/EOF/protocol/timeout) recovery state.
    Recovery {
        /// The recovery diagnostic.
        message: String,
    },
    /// The viewport is too small for the full surface.
    Small,
}

/// The pure provider view projection consumed by the thin renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderViewProjection {
    /// The dominant visual mode.
    pub mode: ProviderViewMode,
    /// The rows to render.
    pub rows: Vec<ProviderViewRow>,
    /// Whether any request is currently active.
    pub has_active_request: bool,
}

/// Input to the pure projection.
#[derive(Debug, Clone)]
pub struct ProviderViewInput<'a> {
    /// The handle-free request state.
    pub requests: &'a ProviderRequestState,
    /// Exact screen identity that owns the projected surface.
    pub context_screen: &'a str,
    /// Exact screen-instance identity that owns the projected surface.
    pub context_instance: &'a str,
    /// The shared action availability, when known.
    pub availability: Option<&'a Availability>,
    /// Whether the provider surface has keyboard focus.
    pub focused: bool,
    /// The confirm-focus override when a confirmation modal is open; defaults
    /// to [`ConfirmFocus::Cancel`] (the safe choice) when `None`.
    pub confirm: Option<ConfirmFocus>,
    /// Terminal viewport rows (for SMALL detection).
    pub viewport_rows: usize,
    /// The focused row index within the provider surface, if any.
    pub focused_index: Option<usize>,
    /// The action label to show, when projecting a single action surface.
    pub action_label: Option<&'a str>,
}

impl<'a> ProviderViewInput<'a> {
    /// Construct an input for a normal (non-modal) provider surface.
    #[must_use]
    pub fn normal(
        requests: &'a ProviderRequestState,
        context_screen: &'a str,
        context_instance: &'a str,
        viewport_rows: usize,
    ) -> Self {
        Self {
            requests,
            context_screen,
            context_instance,
            availability: None,
            focused: false,
            confirm: None,
            viewport_rows,
            focused_index: None,
            action_label: None,
        }
    }
}

/// Project the provider view from pure inputs (CW10-13).
///
/// This is the sole authority for what the thin renderer draws. The mode
/// precedence is: [`Small`] > [`Recovery`] > [`Confirmation`] > [`Error`] >
/// [`Unavailable`] > [`Focused`] > [`Normal`].
///
/// [`Small`]: ProviderViewMode::Small
/// [`Recovery`]: ProviderViewMode::Recovery
/// [`Confirmation`]: ProviderViewMode::Confirmation
/// [`Error`]: ProviderViewMode::Error
/// [`Unavailable`]: ProviderViewMode::Unavailable
/// [`Focused`]: ProviderViewMode::Focused
/// [`Normal`]: ProviderViewMode::Normal
#[must_use]
pub fn project_provider_view(input: &ProviderViewInput<'_>) -> ProviderViewProjection {
    project_provider_view_with_confirmation(
        input,
        input
            .requests
            .first_pending_confirmation_for(input.context_screen, input.context_instance),
    )
}

/// Project the provider view using one exact pending confirmation selected by the host.
#[must_use]
pub(crate) fn project_provider_view_with_confirmation(
    input: &ProviderViewInput<'_>,
    confirmation: Option<PendingConfirmationView<'_>>,
) -> ProviderViewProjection {
    let rows = project_rows(input);
    let has_active_request = input
        .requests
        .requests()
        .iter()
        .any(|request| request_belongs_to_input(request, input) && !request.is_terminal());

    let mode = if input.viewport_rows < SMALL_VIEWPORT_ROW_THRESHOLD {
        ProviderViewMode::Small
    } else if let Some(reason) = dominant_recovery(input) {
        ProviderViewMode::Recovery { message: reason }
    } else if let Some(pending) = confirmation {
        // The confirmation modal content is read directly from the exact
        // pending token the reducer registered — title/body/confirm label/
        // schema are byte-identical to the provider declaration, and the focus
        // defaults to Cancel (the safe choice) unless overridden.
        ProviderViewMode::Confirmation {
            confirm_focus: input.confirm.unwrap_or(ConfirmFocus::Cancel),
            title: pending.title().to_owned(),
            body: pending.body().to_owned(),
            confirm_label: pending.confirm_label().to_owned(),
            continuation_schema: pending.continuation_schema().to_owned(),
            continuation_values: TypedMap::new(),
            focused_field: None,
        }
    } else if let Some(message) = dominant_error(input) {
        ProviderViewMode::Error { message }
    } else if let Some(Availability::Unavailable { reason }) = input.availability {
        ProviderViewMode::Unavailable {
            reason: reason.clone(),
        }
    } else if input.focused {
        ProviderViewMode::Focused
    } else {
        ProviderViewMode::Normal
    };

    ProviderViewProjection {
        mode,
        rows,
        has_active_request,
    }
}

/// Build the row list from active requests and the action label.
fn request_belongs_to_input(request: &ActiveRequest, input: &ProviderViewInput<'_>) -> bool {
    request.context_screen().as_str() == input.context_screen
        && request.context_instance().as_str() == input.context_instance
}

fn project_rows(input: &ProviderViewInput<'_>) -> Vec<ProviderViewRow> {
    let mut rows = Vec::new();

    if let Some(label) = input.action_label {
        let availability_status = match input.availability {
            Some(Availability::Unavailable { reason }) => {
                ProviderRowStatus::Unavailable(reason.clone())
            }
            _ => ProviderRowStatus::None,
        };
        rows.push(ProviderViewRow {
            label: label.to_owned(),
            status: availability_status,
            focused: input.focused && input.focused_index.is_none(),
        });
    }

    for (index, request) in input
        .requests
        .requests()
        .iter()
        .filter(|request| request_belongs_to_input(request, input))
        .enumerate()
    {
        let focused = input.focused_index.is_some_and(|idx| idx == index);
        let label = request_label(request, input.action_label);
        let status = row_status(request);
        rows.push(ProviderViewRow {
            label,
            status,
            focused,
        });
    }

    rows
}

/// Derive a human-readable label for an active request.
fn request_label(request: &ActiveRequest, action_label: Option<&str>) -> String {
    let base = action_label.unwrap_or("provider action");
    format!("{base} (gen {})", request.key().generation)
}

/// Derive the row status from a request's lifecycle state.
fn row_status(request: &ActiveRequest) -> ProviderRowStatus {
    if let Some(reason) = request.unavailable_reason() {
        return ProviderRowStatus::GenerationUnavailable(reason.label().to_owned());
    }
    if let Some(message) = request.failed_message() {
        return ProviderRowStatus::Failed(message.to_owned());
    }
    if request.is_cancelled() {
        return ProviderRowStatus::Cancelled;
    }
    if let Some(outcome) = request.completed_outcome() {
        return ProviderRowStatus::Completed(outcome_summary(outcome));
    }
    if let Some(progress) = request.latest_progress() {
        let summary = match (progress.completed, progress.total) {
            (Some(completed), Some(total)) => {
                format!("{}: {} / {}", progress.message, completed, total)
            }
            _ => progress.message.clone(),
        };
        return ProviderRowStatus::InProgress(summary);
    }
    if request.is_progressing() {
        return ProviderRowStatus::InProgress("in progress".to_owned());
    }
    ProviderRowStatus::None
}

fn outcome_summary(outcome: &crate::runtime::provider::protocol::Outcome) -> String {
    use crate::runtime::provider::protocol::Outcome;

    match outcome {
        Outcome::Navigate { route_id, .. } => format!("Navigate to {route_id}"),
        Outcome::Refresh { .. } => "Refresh requested".to_owned(),
        Outcome::Notice { message, .. } => message.clone(),
        Outcome::RequestHostConfirmation { .. } => "Confirmation required".to_owned(),
    }
}

/// Extract the dominant recovery message (crash/EOF/protocol/timeout).
fn dominant_recovery(input: &ProviderViewInput<'_>) -> Option<String> {
    input
        .requests
        .requests()
        .iter()
        .rev()
        .filter(|request| request_belongs_to_input(request, input))
        .find_map(|request| {
            request
                .unavailable_reason()
                .map(|reason| reason.label().to_owned())
        })
}

/// Extract the dominant error message (terminal provider error).
fn dominant_error(input: &ProviderViewInput<'_>) -> Option<String> {
    input
        .requests
        .requests()
        .iter()
        .rev()
        .filter(|request| request_belongs_to_input(request, input))
        .find_map(|request| request.failed_message().map(ToString::to_string))
}

/// Project a typed provider notice into the shared non-error status line.
#[must_use]
pub fn provider_notice_line(notice: &crate::domain::effects::ProviderNotice) -> String {
    match notice.severity {
        crate::domain::effects::ProviderNoticeSeverity::Info => notice.message.clone(),
        crate::domain::effects::ProviderNoticeSeverity::Warning => {
            format!("Warning: {}", notice.message)
        }
    }
}

#[cfg(test)]
#[path = "provider_view_tests.rs"]
mod tests;
