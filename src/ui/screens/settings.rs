//! The Settings screen (issue #387, CW-07).
//!
//! A thin renderer over `state::settings_view`. Every row, count, availability
//! marker and recovery choice this draws is decided by the projection, so what
//! the user sees and what the reducer will act on cannot disagree.

use iocraft::prelude::*;

use crate::messages::settings::RecoveryChoice;
use crate::persistence::diagnostic::Severity;
use crate::state::navigation_dirty::{DirtyState, GuardPhase, SaveIntent};
use crate::state::settings_types::DirtyChoiceCursor;
use crate::state::settings_view::{
    SettingsRow, SettingsRowKind, detail_rows, recovery_choices, section_rows,
};
use crate::state::{AppState, DraftStatus, SettingsDraft, SettingsFocus, SettingsState};
use crate::theme::{ResolvedColors, ThemeColors};

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
                last_error: props.state.as_ref().and_then(AppState::last_error_title),
                colors: colors.clone(),
                selection: props.state.as_ref().and_then(|s| s.selection),
            )
            #(dirty_guard_row(props.state.as_ref(), &settings, &rc))
            Box(flex_direction: FlexDirection::Row, flex_grow: 1.0_f32, background_color: rc.bg) {
                #(section_pane(&settings, &rc))
                #(detail_pane(&settings, &rc))
            }
            #(notice_row(&settings, &rc))
            KeybindBar(
                screen: props.state.as_ref().map_or(
                    crate::state::ScreenId::Settings,
                    crate::state::AppState::screen,
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
fn detail_pane(settings: &SettingsState, rc: &ResolvedColors) -> AnyElement<'static> {
    let focused = settings.focus == SettingsFocus::Detail;
    let title = settings.section.title().to_owned();
    let rows = detail_rows(settings);
    let selected_row = settings.selected_row;
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
            #(rows.into_iter().enumerate().map(|(index, row)| {
                let selected = focused && index == selected_row;
                element! {
                    Box(width: 100pct, background_color: if selected { rc.sel_bg } else { rc.bg }) {
                        Text(
                            content: render_row(&row, selected),
                            color: row_color(&row, selected, rc),
                        )
                    }
                }
            }))
        }
    }
    .into_any()
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
        | SettingsRowKind::Toggle { .. }
        | SettingsRowKind::Diagnostic { .. } => {
            format!("{marker}{}: {}", row.label, row.value)
        }
    }
}

fn row_color(row: &SettingsRow, selected: bool, rc: &ResolvedColors) -> Color {
    if selected {
        return rc.sel_fg;
    }
    match &row.kind {
        SettingsRowKind::Diagnostic {
            severity: Severity::Error,
            ..
        } => rc.error,
        // A row the user cannot act on reads as secondary, whether it is a
        // fact about the session or a theme that is not installed.
        SettingsRowKind::Theme {
            available: false, ..
        }
        | SettingsRowKind::Fact => rc.dim,
        SettingsRowKind::Theme { .. }
        | SettingsRowKind::Toggle { .. }
        | SettingsRowKind::Screen { .. }
        | SettingsRowKind::Diagnostic { .. } => rc.fg,
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
