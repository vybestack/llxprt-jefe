//! Pure host projection for manifest-backed provider screens.

use crate::runtime::provider::protocol::{PanelBody, PanelSnapshot};
use crate::state::provider_panels::{PanelLifecycle, ProviderPanelState};
use crate::workbench::ScreenDescriptor;

/// Plain display data consumed by the iocraft provider-screen component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderScreenView {
    /// Descriptor-owned screen title.
    pub title: String,
    /// Ordered host-rendered lines for every descriptor panel.
    pub lines: Vec<String>,
}

/// Project one lowered screen without importing iocraft or performing I/O.
#[must_use]
pub fn project_provider_screen(
    descriptor: &ScreenDescriptor,
    screen_instance_id: u64,
    panels: &ProviderPanelState,
) -> ProviderScreenView {
    let mut lines = Vec::new();
    for panel in &descriptor.panels {
        lines.push(panel.id.as_str().to_owned());
        let Some(instance) = panels.panel_for_screen(screen_instance_id, &panel.id) else {
            lines.push("provider unavailable".to_owned());
            continue;
        };
        let lifecycle = panels.lifecycle(instance);
        let snapshot = panels.accepted_snapshot(instance);
        project_panel(
            snapshot,
            lifecycle,
            panels.accepted_model_is_stale(instance),
            panels
                .host_local(instance)
                .and_then(|local| local.selected_id.as_ref()),
            &mut lines,
        );
    }
    ProviderScreenView {
        title: descriptor.title.clone(),
        lines,
    }
}

fn project_panel(
    snapshot: Option<&PanelSnapshot>,
    lifecycle: Option<PanelLifecycle>,
    stale: bool,
    selected_id: Option<&crate::domain::Id>,
    lines: &mut Vec<String>,
) {
    let Some(snapshot) = snapshot else {
        lines.push(lifecycle_text(lifecycle).to_owned());
        return;
    };
    lines.push(snapshot.title.clone());
    if let Some(description) = &snapshot.description {
        lines.push(description.clone());
    }
    if snapshot.loading {
        lines.push("loading".to_owned());
    }
    if stale {
        lines.push("stale".to_owned());
    }
    project_body(&snapshot.body, selected_id, lines);
    project_affordances(&snapshot.action_affordances, lines);
}

fn project_affordances(
    affordances: &[crate::runtime::provider::protocol::Affordance],
    lines: &mut Vec<String>,
) {
    lines.extend(affordances.iter().map(|affordance| {
        if affordance.enabled {
            format!("[{}] {}", affordance.id, affordance.label)
        } else {
            format!(
                "[{}] {} (unavailable: {})",
                affordance.id,
                affordance.label,
                affordance
                    .unavailable_reason
                    .as_deref()
                    .unwrap_or("unavailable")
            )
        }
    }));
}

fn lifecycle_text(lifecycle: Option<PanelLifecycle>) -> &'static str {
    match lifecycle {
        Some(PanelLifecycle::Declared | PanelLifecycle::Activating) => "loading",
        Some(PanelLifecycle::Suspended) => "suspended",
        Some(PanelLifecycle::Failed) | None => "provider unavailable",
        Some(PanelLifecycle::Disposing | PanelLifecycle::Disposed) => "disposed",
        Some(PanelLifecycle::Active) => "empty",
    }
}

fn project_body(
    body: &PanelBody,
    selected_id: Option<&crate::domain::Id>,
    lines: &mut Vec<String>,
) {
    match body {
        PanelBody::List(body) => project_list(body, selected_id, lines),
        PanelBody::Detail(body) => {
            lines.push(body.document.clone());
            lines.extend(
                body.metadata
                    .iter()
                    .map(|row| format!("{}: {}", row.label, row.value)),
            );
            push_actions(&body.actions, lines);
        }
        PanelBody::Form(body) => {
            lines.extend(body.fields.iter().map(|field| {
                body.values.get(field.id()).map_or_else(
                    || field.label().to_owned(),
                    |value| format!("{}: {}", field.label(), display_value(value)),
                )
            }));
            lines.extend(
                body.field_errors
                    .iter()
                    .map(|error| format!("{}: {}", error.field_id, error.message)),
            );
            lines.push(format!("submit: {}", body.submit_action.as_str()));
        }
        PanelBody::Status(body) => lines.extend(
            body.rows
                .iter()
                .map(|row| format!("[{}] {}: {}", row.state.as_str(), row.label, row.value)),
        ),
        PanelBody::Progress(body) => lines.push(progress_line(body)),
        PanelBody::Empty(body) => lines.push(body.action.as_ref().map_or_else(
            || body.message.clone(),
            |action| format!("{} [{action}]", body.message),
        )),
        PanelBody::Error(body) => lines.push(body.retry_action.as_ref().map_or_else(
            || format!("{} {}", body.code, body.message),
            |action| format!("{} {} [Retry: {action}]", body.code, body.message),
        )),
    }
}

fn project_list(
    body: &crate::runtime::provider::protocol::ListBody,
    selected_id: Option<&crate::domain::Id>,
    lines: &mut Vec<String>,
) {
    let selected_id = selected_id.or(body.selected_id.as_ref());
    for item in &body.items {
        let selected = selected_id == Some(&item.id);
        lines.push(list_item_line(item, selected));
        if let Some(description) = &item.description {
            lines.push(format!("   {description}"));
        }
        if !item.actions.is_empty() {
            lines.push(format!("   actions: {}", join_ids(&item.actions)));
        }
    }
    if body.next_page_token.is_some() {
        lines.push("more results available".to_owned());
    }
}

fn list_item_line(item: &crate::runtime::provider::protocol::ListItem, selected: bool) -> String {
    let marker = if selected { ">> " } else { "   " };
    let status = item
        .status
        .as_deref()
        .map_or(String::new(), |value| format!(" [{value}]"));
    format!("{marker}{}{}", item.label, status)
}

fn push_actions(actions: &[crate::domain::Id], lines: &mut Vec<String>) {
    if !actions.is_empty() {
        lines.push(format!("actions: {}", join_ids(actions)));
    }
}

fn join_ids(ids: &[crate::domain::Id]) -> String {
    ids.iter()
        .map(crate::domain::Id::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_value(value: &crate::domain::TypedValue) -> String {
    use crate::domain::TypedValue;
    match value {
        TypedValue::String(value) => value.clone(),
        TypedValue::Bool(value) => value.to_string(),
        TypedValue::Integer(value) => value.to_string(),
        TypedValue::Decimal(value) => value.as_str().to_owned(),
        TypedValue::Datetime(value) => value.as_str().to_owned(),
        TypedValue::List(values) => values
            .iter()
            .map(display_value)
            .collect::<Vec<_>>()
            .join(", "),
        TypedValue::Map(_) => "<map>".to_owned(),
        TypedValue::SecretRef(reference) => format!("set ({})", reference.env.env()),
    }
}

fn progress_line(body: &crate::runtime::provider::protocol::ProgressBody) -> String {
    let progress = match (body.completed, body.total) {
        (Some(completed), Some(total)) => format!("{} {completed}/{total}", body.message),
        _ => body.message.clone(),
    };
    if body.cancellable {
        format!("{progress} [Cancel]")
    } else {
        progress
    }
}

#[cfg(test)]
#[path = "provider_panel_view_tests.rs"]
mod tests;
