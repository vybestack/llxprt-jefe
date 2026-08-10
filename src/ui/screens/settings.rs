//! The Settings screen (issue #387, CW-07).
//!
//! A thin renderer over `state::settings_view`. Every row, count, availability
//! marker and recovery choice this draws is decided by the projection, so what
//! the user sees and what the reducer will act on cannot disagree.

use iocraft::prelude::*;

use crate::list_viewport::fit_text_to_width;
use crate::messages::settings::RecoveryChoice;
use crate::persistence::diagnostic::Severity;
use crate::state::agent_types_editor::AgentAvailability;
use crate::state::layout_editor::{NodeDialog, NodeDialogKind, SizeKind};
use crate::state::navigation_dirty::{DirtyState, GuardPhase, SaveIntent};
use crate::state::screens_editor::CompositionStatus;
use crate::state::screens_editor::preview_layout;
use crate::state::settings_types::DirtyChoiceCursor;
use crate::state::settings_view::{
    PluginConfigMigrationView, SettingsRow, SettingsRowKind, detail_window,
    plugin_config_migration_view, recovery_choices, section_rows,
};
use crate::state::{AppState, DraftStatus, SettingsDraft, SettingsFocus, SettingsState};
use crate::theme::{ResolvedColors, ThemeColors};
use crate::workbench::descriptor::{Axis, LayoutNode, ScreenDescriptor};
use crate::workbench::ids::PanelId;

use super::super::components::{KeybindBar, StatusBar};

/// Props for the Settings screen.
#[derive(Default, Props)]
pub struct SettingsScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
    /// Active theme name.
    pub theme_name: String,
}

/// The Settings screen: the section list beside one section's detail.
#[component]
pub fn SettingsScreen(props: &SettingsScreenProps) -> impl Into<AnyElement<'static>> {
    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));
    let settings = props
        .state
        .as_ref()
        .map(|state| state.settings_state.clone())
        .unwrap_or_default();

    let repo_count = props.state.as_ref().map_or(0, |s| s.repositories.len());
    let agent_count = props.state.as_ref().map_or(0, |s| s.agents.len());
    let running_count = props
        .state
        .as_ref()
        .map_or(0, |s| s.agents.iter().filter(|a| a.is_running()).count());

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            background_color: rc.bg,
        ) {
            StatusBar(
                repo_count: repo_count,
                running_count: running_count,
                agent_count: agent_count,
                theme_name: props.theme_name.clone(),
                version: crate::VERSION.to_owned(),
                warning_message: props.state.as_ref().and_then(|s| s.warning_message.clone()),
                error_count: props.state.as_ref().map_or(0, |s| s.errors_state.count()),
                colors: colors.clone(),
                selection: props.state.as_ref().and_then(|s| s.selection),
            )
            #(dirty_guard_row(props.state.as_ref(), &settings, &rc))
            Box(flex_direction: FlexDirection::Row, flex_grow: 1.0_f32, background_color: rc.bg) {
                #(section_pane(&settings, &rc))
                #(detail_pane(props.state.as_ref(), &settings, &rc))
                #(layout_pane(&settings, &rc))
            }
            #(notice_row(&settings, &rc))
            KeybindBar(
                screen: props.state.as_ref().map_or(
                    crate::state::ScreenId::Settings,
                    |state| state.compiled_screen().unwrap_or(crate::state::ScreenId::Settings),
                ),
                action_registry_snapshot: props
                    .state
                    .as_ref()
                    .and_then(|state| state.action_registry_snapshot.clone()),
                terminal_focused: false,
                actions_focus: None,
                colors: colors.clone(),
            )
        }
    }
}

/// The section list, with the diagnostics count its title carries.
fn section_pane(settings: &SettingsState, rc: &ResolvedColors) -> AnyElement<'static> {
    let focused = settings.focus == SettingsFocus::Sections;
    let rows = section_rows(settings);
    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 20u32,
            // The section list is the screen's only navigation; letting it
            // shrink wraps "Appearance" onto two lines at reduced geometry.
            flex_shrink: 0.0_f32,
            border_style: BorderStyle::Round,
            border_color: if focused { rc.border_focused } else { rc.border },
            background_color: rc.bg,
        ) {
            Text(content: "Settings", weight: Weight::Bold, color: rc.fg)
            #(rows.into_iter().map(|row| {
                let selected = focused && row.section == settings.section;
                let marker = if row.section == settings.section { ">>" } else { "  " };
                let label = match row.count {
                    Some(count) => format!("{marker}{} ({count})", row.title),
                    None => format!("{marker}{}", row.title),
                };
                element! {
                    Box(width: 100pct, background_color: if selected { rc.sel_bg } else { rc.bg }) {
                        Text(
                            content: label,
                            color: if selected { rc.sel_fg } else { rc.fg },
                        )
                    }
                }
            }))
        }
    }
    .into_any()
}

/// The focused section's rows, or the recovery it is waiting on.
fn detail_pane(
    state: Option<&AppState>,
    settings: &SettingsState,
    rc: &ResolvedColors,
) -> AnyElement<'static> {
    let focused = settings.focus == SettingsFocus::Detail;
    let title = settings.section.title().to_owned();
    let geometry = detail_geometry(state, settings);
    let window = detail_window(settings, geometry.rows);
    let width = geometry.columns;
    let selected_row = settings.selected_row;
    let above = window.above;
    let below = window.below;
    element! {
        Box(
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0_f32,
            border_style: BorderStyle::Round,
            border_color: if focused { rc.border_focused } else { rc.border },
            background_color: rc.bg,
        ) {
            Text(content: title, weight: Weight::Bold, color: rc.fg)
            #(recovery_row(settings, rc))
            #(reload_confirmation_row(settings, rc))
            #(plugin_config_editor_row(settings, rc))
            #(plugin_config_migration_row(settings, rc))
            #(overflow_row(above, "above", rc))
            #(window.rows.into_iter().map(|(index, row)| {
                let selected = focused && index == selected_row;
                element! {
                    Box(width: 100pct, background_color: if selected { rc.sel_bg } else { rc.bg }) {
                        Text(
                            // One row is one line. A row that wrapped would
                            // push the notice and the keybind bar off the
                            // bottom of a full list, taking with them the very
                            // reasons a long row was trying to explain.
                            content: fit_text_to_width(&render_row(&row, selected), width),
                            color: row_color(&row, selected, rc),
                        )
                    }
                }
            }))
            #(overflow_row(below, "below", rc))
        }
    }
    .into_any()
}

/// The rectangle the focused section's rows are drawn into.
struct DetailGeometry {
    /// How many rows the pane can draw.
    rows: usize,
    /// How many columns one row may occupy.
    columns: usize,
}

/// The focused section's drawable rectangle.
///
/// The geometry is the resolver's, so the window and the rectangle the renderer
/// was given cannot disagree. Two rows are reserved for the section title and
/// the overflow marker; the fallback matters only before the first layout is
/// resolved, and showing a few rows is better than showing none.
fn detail_geometry(state: Option<&AppState>, settings: &SettingsState) -> DetailGeometry {
    const FALLBACK_ROWS: usize = 16;
    const FALLBACK_COLUMNS: usize = 72;
    const RESERVED_ROWS: usize = 2;
    let panel = PanelId::from_static(crate::screen_layout::settings_section_panel(
        settings.section,
    ));
    let resolved = state
        .and_then(|state| state.resolved_layout.as_ref())
        .and_then(|layout| layout.panel(&panel));
    DetailGeometry {
        rows: resolved
            .map_or(FALLBACK_ROWS, |resolved| {
                usize::from(resolved.content.height)
            })
            .saturating_sub(RESERVED_ROWS)
            .max(1),
        columns: resolved
            .map_or(FALLBACK_COLUMNS, |resolved| {
                usize::from(resolved.content.width)
            })
            .max(1),
    }
}

/// The marker saying how many rows the window is hiding, when it hides any.
fn overflow_row(count: usize, side: &str, rc: &ResolvedColors) -> Option<AnyElement<'static>> {
    if count == 0 {
        return None;
    }
    Some(
        element! {
            Box(width: 100pct, background_color: rc.bg) {
                Text(content: format!("  … {count} more {side}"), color: rc.dim)
            }
        }
        .into_any(),
    )
}
fn plugin_config_editor_row(
    settings: &SettingsState,
    rc: &ResolvedColors,
) -> Option<AnyElement<'static>> {
    let editor = settings.plugin_config_editor.as_ref()?;
    let suffix = editor
        .error
        .map_or_else(String::new, |error| format!(" ! {error}"));
    Some(
        element! {
            Box(width: 100pct, background_color: rc.sel_bg) {
                Text(
                    content: format!(">>{}.{}: {}{suffix}", editor.plugin, editor.field, editor.text),
                    color: if editor.error.is_some() { rc.error } else { rc.sel_fg },
                )
            }
        }
        .into_any(),
    )
}

fn plugin_config_migration_row(
    settings: &SettingsState,
    rc: &ResolvedColors,
) -> Option<AnyElement<'static>> {
    let view = plugin_config_migration_view(settings)?;
    let (content, color) = match view {
        PluginConfigMigrationView::Running { owner } => {
            (format!("Migrating {owner} configuration..."), rc.bright)
        }
        PluginConfigMigrationView::Preview {
            owner,
            changes,
            notes,
        } => {
            let mut lines = vec![format!("Approve migration for {owner}")];
            lines.extend(changes);
            lines.extend(notes.into_iter().map(|note| format!("note: {note}")));
            lines.push("Enter Approve  Esc Cancel".to_owned());
            (lines.join("\n"), rc.bright)
        }
        PluginConfigMigrationView::Failed { owner, detail } => (
            format!("Migration failed for {owner}: {detail}\nEsc Dismiss"),
            rc.error,
        ),
    };
    Some(
        element! {
            Box(width: 100pct, border_style: BorderStyle::Round, border_color: color) {
                Text(content: content, color: color)
            }
        }
        .into_any(),
    )
}

/// The layout tree editor, while it is open.
///
/// Everything drawn here is decided by `state::layout_editor`; this only puts
/// the tree, the open dialog, and whatever was refused on the screen.
fn layout_pane(settings: &SettingsState, rc: &ResolvedColors) -> Option<AnyElement<'static>> {
    let editor = settings.layout_editor.as_ref()?;
    let screen = crate::workbench::screen_registry()
        .ok()
        .and_then(|registry| {
            registry
                .screens()
                .iter()
                .find(|screen| screen.id.as_str() == editor.screen_id.as_str())
        })?;
    let lines = layout_lines(&editor.tree, &editor.selected, &[], 0);
    let dialog = editor
        .dialog
        .as_ref()
        .map(|dialog| dialog_lines(dialog, &editor.addable_panels(screen)));
    let preview = preview_lines(screen, &editor.tree);
    Some(
        element! {
            Box(
                flex_direction: FlexDirection::Column,
                width: 34u32,
                flex_shrink: 0.0_f32,
                border_style: BorderStyle::Round,
                border_color: rc.border_focused,
                background_color: rc.bg,
            ) {
                Text(content: "Layout", weight: Weight::Bold, color: rc.fg)
                #(lines.into_iter().map(|(text, selected)| element! {
                    Box(width: 100pct, background_color: if selected { rc.sel_bg } else { rc.bg }) {
                        Text(content: text, color: if selected { rc.sel_fg } else { rc.fg })
                    }
                }))
                #(dialog.into_iter().flatten().map(|(text, error)| element! {
                    Box(width: 100pct, background_color: rc.bg) {
                        Text(content: text, color: if error { rc.error } else { rc.fg })
                    }
                }))
                #(preview.into_iter().map(|text| element! {
                    Box(width: 100pct, background_color: rc.bg) {
                        Text(content: text, color: rc.dim)
                    }
                }))
                Text(content: "q Back  Ctrl-Q quit", color: rc.dim)
            }
        }
        .into_any(),
    )
}

/// The geometry this tree resolves to, at normal and at reduced dimensions.
///
/// The resolver is the standard one, so what the preview shows and what a
/// restart would build cannot disagree. It resolves under a preview identity
/// and the result is drawn and discarded, so the session's own geometry is
/// untouched.
fn preview_lines(screen: &ScreenDescriptor, tree: &LayoutNode) -> Vec<String> {
    const NORMAL: (u16, u16) = (120, 36);
    const SMALL: (u16, u16) = (60, 18);
    [NORMAL, SMALL]
        .into_iter()
        .map(
            |(cols, rows)| match preview_layout(screen, tree, cols, rows) {
                Ok(resolved) => {
                    let visible = resolved.visible_panels().count();
                    let fitted = resolved.too_small.as_ref().map_or("", |_| " (too small)");
                    format!("preview {cols}x{rows}: {visible} panels{fitted}")
                }
                Err(_) => format!("preview {cols}x{rows}: does not resolve"),
            },
        )
        .collect()
}

/// One line per node, marked where the selection is.
fn layout_lines(
    node: &LayoutNode,
    selected: &[usize],
    path: &[usize],
    depth: usize,
) -> Vec<(String, bool)> {
    let here = path == selected;
    let indent = "  ".repeat(depth);
    let mut lines = match node {
        LayoutNode::Leaf { panel } => {
            vec![(format!("{indent}leaf: {}", panel.as_str()), here)]
        }
        LayoutNode::Split { axis, .. } => {
            let axis = match axis {
                Axis::Horizontal => "H",
                Axis::Vertical => "V",
            };
            vec![(format!("{indent}split {axis}"), here)]
        }
    };
    if let LayoutNode::Split { children, .. } = node {
        for (index, child) in children.iter().enumerate() {
            let mut child_path = path.to_vec();
            child_path.push(index);
            lines.extend(layout_lines(&child.node, selected, &child_path, depth + 1));
        }
    }
    lines
}

/// The open node dialog's fields, and whatever it refused.
fn dialog_lines(dialog: &NodeDialog, addable: &[PanelId]) -> Vec<(String, bool)> {
    let focus = |index: usize| if dialog.field == index { ">>" } else { "  " };
    let mut lines = Vec::new();
    if dialog.kind == NodeDialogKind::AddLeaf {
        // Which panel Enter places has to be visible, or the chooser is a
        // hidden setting the user changes by feel.
        lines.push((
            format!(
                "  panel: {}",
                addable
                    .get(dialog.panel_choice)
                    .map_or("(this screen places every panel it declares)", |panel| {
                        panel.as_str()
                    })
            ),
            addable.is_empty(),
        ));
    }
    lines.extend(vec![
        (
            format!(
                "{}size kind: {}",
                focus(0),
                match dialog.size_kind {
                    SizeKind::Fixed => "fixed",
                    SizeKind::Weight => "weight",
                }
            ),
            false,
        ),
        (format!("{}size: {}", focus(1), dialog.size), false),
        (format!("{}min: {}", focus(2), dialog.min), false),
        (format!("{}max: {}", focus(3), dialog.max), false),
        (
            format!("{}collapsible: {}", focus(4), dialog.collapsible),
            false,
        ),
        (
            format!("{}collapse order: {}", focus(5), dialog.collapse_priority),
            false,
        ),
    ]);
    if let Some(error) = dialog.error.as_ref() {
        lines.push((format!("! {error}"), true));
    }
    lines
}

/// One row's rendered text, including its selection marker.
fn render_row(row: &SettingsRow, selected: bool) -> String {
    let marker = if selected { ">>" } else { "  " };
    match &row.kind {
        SettingsRowKind::Theme { active, .. } | SettingsRowKind::Screen { active, .. } => {
            let active_marker = if *active { " *" } else { "  " };
            format!("{marker}{}{active_marker} {}", row.label, row.value)
        }
        SettingsRowKind::Fact
        | SettingsRowKind::PluginConfig { .. }
        | SettingsRowKind::Toggle { .. }
        | SettingsRowKind::Diagnostic { .. }
        | SettingsRowKind::AgentType { .. }
        | SettingsRowKind::ScreenMember { .. }
        | SettingsRowKind::KeyBinding { .. } => {
            format!("{marker}{}: {}", row.label, row.value)
        }
    }
}

fn row_color(row: &SettingsRow, selected: bool, rc: &ResolvedColors) -> Color {
    if selected {
        return rc.sel_fg;
    }
    match &row.kind {
        // A problem reads as a problem: an error diagnostic, and a screen whose
        // override the validator refuses.
        SettingsRowKind::Diagnostic {
            severity: Severity::Error,
            ..
        }
        | SettingsRowKind::ScreenMember {
            composition: CompositionStatus::Invalid { .. },
            ..
        } => rc.error,
        // A row the user cannot act on reads as secondary, whether it is a fact
        // about the session, a theme that is not installed, a control that
        // cannot be rebound, or an agent type this machine cannot run.
        SettingsRowKind::Theme {
            available: false, ..
        }
        | SettingsRowKind::Fact
        | SettingsRowKind::KeyBinding {
            protected: Some(_), ..
        } => rc.dim,
        SettingsRowKind::AgentType { availability, .. }
            if !matches!(availability, AgentAvailability::Compatible) =>
        {
            rc.dim
        }
        SettingsRowKind::Theme { .. }
        | SettingsRowKind::PluginConfig { .. }
        | SettingsRowKind::Toggle { .. }
        | SettingsRowKind::Screen { .. }
        | SettingsRowKind::Diagnostic { .. }
        | SettingsRowKind::AgentType { .. }
        | SettingsRowKind::ScreenMember { .. }
        | SettingsRowKind::KeyBinding { .. } => rc.fg,
    }
}

/// The recovery a conflict or a write failure offers.
fn recovery_row(settings: &SettingsState, rc: &ResolvedColors) -> Option<AnyElement<'static>> {
    let choices = recovery_choices(settings);
    if choices.is_empty() {
        return None;
    }
    let heading = match settings.draft.as_ref().map(SettingsDraft::status) {
        Some(DraftStatus::Conflict { .. }) => "External edit detected: disk and draft preserved",
        _ => "Save failed: the draft is intact",
    };
    let selected = settings.recovery_row;
    Some(
        element! {
            Box(flex_direction: FlexDirection::Column, background_color: rc.bg) {
                Text(content: heading, color: rc.error)
                Box(flex_direction: FlexDirection::Row, background_color: rc.bg) {
                    #(choices.into_iter().enumerate().map(|(index, choice)| {
                        let focused = index == selected;
                        element! {
                            Box(background_color: if focused { rc.sel_bg } else { rc.bg }) {
                                Text(
                                    content: format!(
                                        "{}{} ",
                                        if focused { ">>" } else { "  " },
                                        recovery_label(choice)
                                    ),
                                    color: if focused { rc.sel_fg } else { rc.fg },
                                )
                            }
                        }
                    }))
                }
            }
        }
        .into_any(),
    )
}

const fn recovery_label(choice: RecoveryChoice) -> &'static str {
    match choice {
        RecoveryChoice::Reload => "Reload",
        RecoveryChoice::Export => "Export",
        RecoveryChoice::Retry => "Retry",
        RecoveryChoice::Discard => "Discard",
    }
}

/// The question a reload asks before it can discard unsaved work.
fn reload_confirmation_row(
    settings: &SettingsState,
    rc: &ResolvedColors,
) -> Option<AnyElement<'static>> {
    if !settings.reload_confirm {
        return None;
    }
    Some(
        element! {
            Box(background_color: rc.bg) {
                Text(
                    content: "Reload from disk and lose unsaved changes? \
                              Enter discards them, Esc keeps them",
                    color: rc.error,
                )
            }
        }
        .into_any(),
    )
}

/// The host dirty guard, while it is holding a navigation back.
///
/// The guard is the navigation reducer's; this only draws the question it is
/// asking, and says why Save is unavailable when the owner declared it so.
fn dirty_guard_row(
    state: Option<&AppState>,
    settings: &SettingsState,
    rc: &ResolvedColors,
) -> Option<AnyElement<'static>> {
    let guard = state?.nav.guard()?;
    let unavailable: Option<&'static str> = match &state?.nav.current().dirty {
        DirtyState::Dirty {
            save: SaveIntent::Unavailable { reason },
            ..
        } => Some(reason),
        DirtyState::Dirty { .. } | DirtyState::Clean => None,
    };
    let detail = match guard.phase() {
        GuardPhase::Failed { detail } => detail.clone(),
        GuardPhase::SaveRequested { .. } | GuardPhase::Saving { .. } => "Saving…".to_owned(),
        GuardPhase::Choosing => {
            unavailable.map_or_else(|| "Save changes?".to_owned(), ToOwned::to_owned)
        }
    };
    let cursor = settings.dirty_choice;
    Some(
        element! {
            Box(flex_direction: FlexDirection::Column, background_color: rc.bg) {
                Text(content: detail, color: rc.error)
                Box(flex_direction: FlexDirection::Row, background_color: rc.bg) {
                    #(DirtyChoiceCursor::ALL.into_iter().map(|choice| {
                        let focused = choice == cursor;
                        let disabled = choice == DirtyChoiceCursor::Save && unavailable.is_some();
                        element! {
                            Box(background_color: if focused { rc.sel_bg } else { rc.bg }) {
                                Text(
                                    content: format!(
                                        "{}{} ",
                                        if focused { ">>" } else { "  " },
                                        choice.label()
                                    ),
                                    color: choice_color(focused, disabled, rc),
                                )
                            }
                        }
                    }))
                }
            }
        }
        .into_any(),
    )
}

const fn choice_color(focused: bool, disabled: bool, rc: &ResolvedColors) -> Color {
    if disabled {
        rc.dim
    } else if focused {
        rc.sel_fg
    } else {
        rc.fg
    }
}

/// The last completed action's redacted notice.
fn notice_row(settings: &SettingsState, rc: &ResolvedColors) -> Option<AnyElement<'static>> {
    let notice = settings.notice.clone()?;
    Some(
        element! {
            Box(height: 1u32, background_color: rc.bg) {
                Text(content: notice, color: rc.bright)
            }
        }
        .into_any(),
    )
}
