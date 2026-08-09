//! Provider-action execution modal (issue #390 CW-10).

use iocraft::prelude::*;

use crate::state::ConfirmFocus;
use crate::state::provider_view::{
    FOCUS_MARKER, ProviderRowStatus, ProviderViewMode, ProviderViewProjection,
};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the provider-action execution modal.
#[derive(Props)]
pub struct ProviderModalProps {
    /// Pure provider execution projection.
    pub projection: ProviderViewProjection,
    /// Theme colors.
    pub colors: ThemeColors,
}

impl Default for ProviderModalProps {
    fn default() -> Self {
        Self {
            projection: ProviderViewProjection {
                mode: ProviderViewMode::Normal,
                rows: Vec::new(),
                has_active_request: false,
            },
            colors: ThemeColors::default(),
        }
    }
}

fn status_text(status: &ProviderRowStatus) -> String {
    match status {
        ProviderRowStatus::None => "Ready".to_owned(),
        ProviderRowStatus::Unavailable(reason)
        | ProviderRowStatus::Failed(reason)
        | ProviderRowStatus::GenerationUnavailable(reason)
        | ProviderRowStatus::Completed(reason) => reason.clone(),
        ProviderRowStatus::InProgress(summary) => summary.clone(),
        ProviderRowStatus::Cancelled => "Cancelled".to_owned(),
    }
}

fn mode_message(mode: &ProviderViewMode, has_active_request: bool) -> Option<String> {
    match mode {
        ProviderViewMode::Normal => None,
        ProviderViewMode::Focused => Some("Provider controls focused".to_owned()),
        ProviderViewMode::Unavailable { reason }
        | ProviderViewMode::Error { message: reason }
        | ProviderViewMode::Recovery { message: reason } => Some(reason.clone()),
        ProviderViewMode::Confirmation {
            confirm_focus,
            title,
            body,
            confirm_label,
            ..
        } => {
            let controls = match confirm_focus {
                ConfirmFocus::Cancel => format!("{FOCUS_MARKER}Cancel  {confirm_label}"),
                ConfirmFocus::Confirm => format!("Cancel  {FOCUS_MARKER}{confirm_label}"),
            };
            Some(format!("{title}\n{body}\n{controls}"))
        }
        ProviderViewMode::Small if has_active_request => {
            Some("Provider action running — press Esc to cancel".to_owned())
        }
        ProviderViewMode::Small => Some("Provider action".to_owned()),
    }
}

/// Build the exact operator-facing text rendered by the provider modal.
#[must_use]
pub fn provider_modal_lines(projection: &ProviderViewProjection) -> Vec<String> {
    let mut lines = vec!["Provider Action".to_owned()];
    if let Some(message) = mode_message(&projection.mode, projection.has_active_request) {
        lines.extend(message.lines().map(ToOwned::to_owned));
    }
    lines.extend(
        projection
            .rows
            .iter()
            .map(|row| format!("{}  {}", row.label, status_text(&row.status))),
    );
    match &projection.mode {
        ProviderViewMode::Confirmation { .. } => {
            lines.push("Tab Select   Enter Activate   Esc Cancel".to_owned());
        }
        ProviderViewMode::Unavailable { .. } => lines.push("Esc Close".to_owned()),
        _ if projection
            .rows
            .iter()
            .any(|row| matches!(row.status, ProviderRowStatus::Unavailable(_))) =>
        {
            lines.push("Esc Close".to_owned());
        }
        _ if projection.has_active_request => lines.push("Esc Cancel".to_owned()),
        _ => lines.push("Enter Retry   Esc Close".to_owned()),
    }
    lines
}

/// Compact modal for one provider action and its live request.
#[component]
pub fn ProviderModal(props: &ProviderModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let lines = provider_modal_lines(&props.projection);
    let height = u32::try_from(lines.len().saturating_add(2)).map_or(12, |value| value.max(6));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 64u32,
            height: height,
            border_style: BorderStyle::Round,
            border_color: rc.border_focused,
            background_color: rc.bg,
            padding: 1u32,
        ) {
            #(lines.into_iter().enumerate().map(|(index, line)| element! {
                Text(
                    content: line,
                    color: if index == 0 { rc.bright } else { rc.fg },
                    weight: if index == 0 { Weight::Bold } else { Weight::Normal },
                )
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::provider_view::{ProviderViewMode, ProviderViewRow};

    #[test]
    fn live_progress_includes_action_fraction_and_cancel() {
        let projection = ProviderViewProjection {
            mode: ProviderViewMode::Focused,
            rows: vec![ProviderViewRow {
                label: "Ship release".to_owned(),
                status: ProviderRowStatus::InProgress("Uploading release: 1 / 2".to_owned()),
                focused: true,
            }],
            has_active_request: true,
        };

        let lines = provider_modal_lines(&projection);
        assert!(lines.iter().any(|line| line == "Provider Action"));
        assert!(lines.iter().any(|line| line.contains("Ship release")));
        assert!(lines.iter().any(|line| line.contains("Uploading release")));
        assert!(lines.iter().any(|line| line.contains("1 / 2")));
        assert!(lines.iter().any(|line| line.contains("Cancel")));
    }

    #[test]
    fn terminal_notice_is_visible_with_retry_and_close() {
        let projection = ProviderViewProjection {
            mode: ProviderViewMode::Normal,
            rows: vec![ProviderViewRow {
                label: "Ship release".to_owned(),
                status: ProviderRowStatus::Completed("Release shipped".to_owned()),
                focused: false,
            }],
            has_active_request: false,
        };

        let lines = provider_modal_lines(&projection);
        assert!(lines.iter().any(|line| line.contains("Release shipped")));
        assert!(lines.iter().any(|line| line.contains("Retry")));
        assert!(lines.iter().any(|line| line.contains("Close")));
    }

    #[test]
    fn confirmation_marks_safe_focus_and_exposes_mode_specific_controls() {
        let projection = ProviderViewProjection {
            mode: ProviderViewMode::Confirmation {
                confirm_focus: ConfirmFocus::Cancel,
                title: "Confirm deployment?".to_owned(),
                body: "This action changes production.".to_owned(),
                confirm_label: "Deploy now".to_owned(),
                continuation_schema: Vec::new(),
            },
            rows: Vec::new(),
            has_active_request: false,
        };

        let lines = provider_modal_lines(&projection);
        assert!(lines.iter().any(|line| line == ">>Cancel  Deploy now"));
        assert!(lines.iter().any(|line| line.contains("Enter Activate")));
        assert!(!lines.iter().any(|line| line.contains("Enter Retry")));
    }

    #[test]
    fn small_unavailable_surface_never_advertises_running_cancel_or_retry() {
        let projection = ProviderViewProjection {
            mode: ProviderViewMode::Small,
            rows: vec![ProviderViewRow {
                label: "Ship release".to_owned(),
                status: ProviderRowStatus::Unavailable("provider stopped".to_owned()),
                focused: false,
            }],
            has_active_request: false,
        };

        let lines = provider_modal_lines(&projection);
        assert!(lines.iter().any(|line| line == "Provider action"));
        assert!(lines.iter().any(|line| line == "Esc Close"));
        assert!(!lines.iter().any(|line| line.contains("running")));
        assert!(!lines.iter().any(|line| line.contains("Retry")));
    }
}
