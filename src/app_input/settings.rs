//! The Settings shell's I/O boundary (issue #387, CW-07).
//!
//! Everything that decides *what* the draft becomes lives in
//! `jefe::state::settings`. This module does only the three things a pure
//! reducer cannot: read the settings bytes, write them, and make the theme
//! manager show what the state says it should be showing.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use jefe::domain::ThemeId;
use jefe::messages::AppMessage;
use jefe::messages::NavDir;
use jefe::messages::settings::{
    LayoutMessage, RecoveryChoice, SettingsEnvironment, SettingsMessage, SettingsSection,
    SettingsSource, ThemeChoice,
};
use jefe::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use jefe::persistence::settings_edit::{ExportPath, export_candidate};
use jefe::persistence::writer::{ExpectedHash, Freshness};
use jefe::persistence::{PersistenceManager, SettingsCandidate, SettingsSaveOutcome};
use jefe::state::navigation_dirty::DirtyChoice;
use jefe::state::{DraftStatus, SettingsDraft, settings_view};
use jefe::theme::ThemeManager;
use jefe::workbench::descriptor::Axis;

use super::{AppStateHandle, SharedContext};

/// One Settings intent the key layer resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// Open the screen on a freshly read document.
    Open,
    /// Open the screen on a freshly read document, showing the Keys section.
    OpenKeys,
    /// Leave the screen, raising the host dirty guard when work is unsaved.
    Back,
    /// Move the selection up.
    Up,
    /// Move the selection down.
    Down,
    /// Move focus from the section list to the detail pane, or back.
    CyclePane,
    /// Move focus the other way.
    CyclePaneReverse,
    /// Apply the focused row, or take the offered recovery.
    Activate,
    /// Step the selection backwards.
    SelectPrevious,
    /// Step the selection forwards.
    SelectNext,
    /// Make the draft authoritative.
    Save,
    /// Make the draft authoritative and leave the screen.
    SaveAndExit,
    /// Return the focused row's leaf to its compiled default.
    Reset,
    /// Flip whatever the focused row holds, when it holds something that flips.
    Toggle,
    /// Bind nothing to the focused action.
    Unbind,
    /// Wait for one more chord to add to the focused binding.
    AddChord,
    /// Move the focused screen one place earlier.
    MoveUp,
    /// Move the focused screen one place later.
    MoveDown,
}

/// Apply one resolved Settings intent.
pub fn apply(action: SettingsAction, app_state: &mut AppStateHandle, ctx: &SharedContext) {
    match action {
        SettingsAction::Open => dispatch_open(app_state, ctx, None),
        SettingsAction::OpenKeys => {
            dispatch_open(app_state, ctx, Some(SettingsSection::Keys));
        }
        SettingsAction::Back => back(app_state),
        SettingsAction::Save => save(app_state, ctx, false),
        SettingsAction::SaveAndExit => save(app_state, ctx, true),
        SettingsAction::Up => dispatch(app_state, SettingsMessage::Navigate(NavDir::Up)),
        SettingsAction::Down => dispatch(app_state, SettingsMessage::Navigate(NavDir::Down)),
        SettingsAction::CyclePane => dispatch(app_state, SettingsMessage::CycleFocus),
        SettingsAction::CyclePaneReverse => {
            dispatch(app_state, SettingsMessage::CycleFocusReverse);
        }
        SettingsAction::Activate => activate(app_state, ctx),
        SettingsAction::SelectPrevious => select(app_state, NavDir::Prev),
        SettingsAction::SelectNext => select(app_state, NavDir::Next),
        SettingsAction::Reset => reset(app_state),
        SettingsAction::Toggle => dispatch(app_state, SettingsMessage::ToggleRow),
        SettingsAction::Unbind => dispatch(app_state, SettingsMessage::UnbindRow),
        SettingsAction::AddChord => dispatch(app_state, SettingsMessage::AddChord),
        SettingsAction::MoveUp => {
            dispatch(app_state, SettingsMessage::ReorderRow(NavDir::Up));
        }
        SettingsAction::MoveDown => {
            dispatch(app_state, SettingsMessage::ReorderRow(NavDir::Down));
        }
    }
    reconcile_theme(app_state, ctx);
}

/// Answer the host dirty guard while it is holding a navigation back.
///
/// The guard traps the keyboard: while it is up, only its own choices and the
/// protected exit get through, so a stray keystroke cannot act on the screen
/// the user is being asked about leaving.
#[must_use]
pub fn handle_dirty_guard_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    if app_state.read().nav.guard().is_none() {
        return false;
    }
    if key_event.kind == KeyEventKind::Release {
        // Terminals that report releases would otherwise answer the guard
        // twice for one physical keypress.
        return true;
    }
    if key_event.modifiers == KeyModifiers::CONTROL
        && matches!(key_event.code, KeyCode::Char('q' | 'Q'))
    {
        // Ctrl-Q is the protected exit and is never aliased to Back.
        return false;
    }
    match key_event.code {
        KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
            dispatch(app_state, SettingsMessage::NavigateDirty(NavDir::Next));
        }
        KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
            dispatch(app_state, SettingsMessage::NavigateDirty(NavDir::Prev));
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let choice = app_state.read().settings_state.dirty_choice.choice();
            resolve_dirty(choice, app_state, ctx);
        }
        KeyCode::Esc => resolve_dirty(DirtyChoice::Cancel, app_state, ctx),
        _ => {}
    }
    true
}

/// Give one key press to a waiting chord capture.
///
/// A capture takes exactly the next chord, so while one is waiting the keyboard
/// belongs to it — otherwise the key the user is trying to bind would also do
/// whatever it is already bound to.
#[must_use]
pub fn handle_capture_key(app_state: &mut AppStateHandle, key_event: &KeyEvent) -> bool {
    if app_state.read().settings_state.capture.is_none() {
        return false;
    }
    if key_event.kind == KeyEventKind::Release {
        // A release is the same physical press reported twice; capturing it
        // would bind the chord and then immediately answer for it again.
        return true;
    }
    // A press this grammar has no chord for is not a chord the registry could
    // dispatch either — a bare modifier is the ordinary case — so there is
    // nothing to bind, and the capture goes on waiting for a key that names one.
    if let Ok(chord) = jefe::input::canonical_chord(key_event) {
        dispatch(app_state, SettingsMessage::CapturedChord(chord));
    }
    true
}

/// Route one key press to the open layout tree editor.
///
/// The editor owns the keyboard while it is open, so a keystroke meant for a
/// node dialog cannot also move the row list behind it.
#[must_use]
pub fn handle_layout_key(app_state: &mut AppStateHandle, key_event: &KeyEvent) -> bool {
    let in_dialog = {
        let state = app_state.read();
        match state.settings_state.layout_editor.as_ref() {
            None => return false,
            Some(editor) => editor.dialog.is_some(),
        }
    };
    if key_event.kind == KeyEventKind::Release {
        return true;
    }
    if key_event.modifiers == KeyModifiers::CONTROL
        && matches!(key_event.code, KeyCode::Char('q' | 'Q'))
    {
        // Ctrl-Q is the protected exit and is never captured by a sub-editor.
        return false;
    }
    let message = if in_dialog {
        dialog_message(key_event)
    } else {
        tree_message(key_event)
    };
    if let Some(message) = message {
        dispatch(app_state, SettingsMessage::Layout(message));
    }
    true
}

/// What one key press means while the node dialog is open.
fn dialog_message(key_event: &KeyEvent) -> Option<LayoutMessage> {
    match key_event.code {
        KeyCode::Esc => Some(LayoutMessage::CancelDialog),
        KeyCode::Enter => Some(LayoutMessage::ApplyDialog),
        KeyCode::Tab => Some(LayoutMessage::NextField),
        KeyCode::Backspace => Some(LayoutMessage::Backspace),
        KeyCode::Up => Some(LayoutMessage::ChoosePanel(NavDir::Prev)),
        KeyCode::Down => Some(LayoutMessage::ChoosePanel(NavDir::Next)),
        KeyCode::Char(' ') => Some(LayoutMessage::ToggleField),
        KeyCode::Char(character) => Some(LayoutMessage::TypeChar(character)),
        _ => None,
    }
}

/// What one key press means while the tree itself has focus.
fn tree_message(key_event: &KeyEvent) -> Option<LayoutMessage> {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(LayoutMessage::Cancel),
        KeyCode::Enter => Some(LayoutMessage::Apply),
        KeyCode::Up | KeyCode::Char('k') => Some(LayoutMessage::SelectPrevious),
        KeyCode::Down | KeyCode::Char('j') => Some(LayoutMessage::SelectNext),
        KeyCode::Left | KeyCode::Char('h') => Some(LayoutMessage::SelectParent),
        KeyCode::Right | KeyCode::Char('l') => Some(LayoutMessage::SelectChild),
        KeyCode::Char('a') => Some(LayoutMessage::BeginAdd),
        KeyCode::Char('e') => Some(LayoutMessage::BeginEdit),
        KeyCode::Char('x') => Some(LayoutMessage::Remove),
        KeyCode::Char('r') => Some(LayoutMessage::ResetOverride),
        KeyCode::Char('H') => Some(LayoutMessage::Split(Axis::Horizontal)),
        KeyCode::Char('V') => Some(LayoutMessage::Split(Axis::Vertical)),
        _ => None,
    }
}

fn resolve_dirty(choice: DirtyChoice, app_state: &mut AppStateHandle, ctx: &SharedContext) {
    dispatch(app_state, SettingsMessage::ResolveDirty(choice));
    write_pending(app_state, ctx);
    reconcile_theme(app_state, ctx);
}

fn dispatch_open(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    section: Option<SettingsSection>,
) {
    match read_source(ctx) {
        Ok(source) => {
            dispatch(app_state, SettingsMessage::Open(Box::new(source)));
            // A key that names one section — "edit my keys" — lands on it
            // rather than on the screen's front page.
            if let Some(section) = section {
                dispatch(app_state, SettingsMessage::SelectSection(section));
            }
        }
        // A key that appears to do nothing is the worst answer available, so
        // the screen opens on the reason it could not bind a draft.
        Err(detail) => dispatch(
            app_state,
            SettingsMessage::OpenFailed(Box::new(settings_failure(&detail))),
        ),
    }
}

/// Leave the screen, or withdraw a reload that is waiting to be confirmed.
fn back(app_state: &mut AppStateHandle) {
    let message = if awaiting_reload_confirmation(app_state) {
        SettingsMessage::ReloadCancelled
    } else {
        SettingsMessage::Back
    };
    dispatch(app_state, message);
}

/// Apply the focused row, confirm a reload, or take the offered recovery.
fn activate(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    if awaiting_reload_confirmation(app_state) {
        reload(app_state, ctx);
        return;
    }
    match offered_recovery(app_state) {
        Some(RecoveryChoice::Reload) => request_reload(app_state, ctx),
        Some(RecoveryChoice::Export) => export(app_state, ctx),
        Some(RecoveryChoice::Retry) => save(app_state, ctx, false),
        Some(RecoveryChoice::Discard) => dispatch(app_state, SettingsMessage::Discard),
        None => dispatch(app_state, SettingsMessage::Activate),
    }
}

/// Left and Right step the recovery choices when one is offered, and otherwise
/// move the same selection the vertical keys do.
fn select(app_state: &mut AppStateHandle, direction: NavDir) {
    let message = if offered_recovery(app_state).is_some() {
        SettingsMessage::NavigateRecovery(direction)
    } else {
        SettingsMessage::Navigate(direction)
    };
    dispatch(app_state, message);
}

fn reset(app_state: &mut AppStateHandle) {
    dispatch(app_state, SettingsMessage::ResetRow);
}

/// The recovery choice the screen is currently offering, if any.
fn offered_recovery(app_state: &AppStateHandle) -> Option<RecoveryChoice> {
    let state = app_state.read();
    settings_view::recovery_choices(&state.settings_state)
        .get(state.settings_state.recovery_row)
        .copied()
}

fn awaiting_reload_confirmation(app_state: &AppStateHandle) -> bool {
    app_state.read().settings_state.reload_confirm
}

/// Read the exact current bytes and everything else a draft binds to.
///
/// # Errors
///
/// Returns a redacted reason when the settings target cannot be read. An absent
/// file is not one of those reasons: it is a normal base whose first save
/// creates the file.
fn read_source(ctx: &SharedContext) -> Result<SettingsSource, String> {
    let context = ctx
        .as_ref()
        .ok_or_else(|| "the settings context is unavailable".to_owned())?;
    let context = context
        .lock()
        .map_err(|_| "the settings context lock failed".to_owned())?;
    let path = context.persistence.settings_path();
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let themes = context
        .theme_manager
        .themes_with_names()
        .into_iter()
        .filter_map(|(slug, name)| match ThemeId::parse(&slug) {
            Ok(id) => Some(ThemeChoice { id, name }),
            Err(error) => {
                // A theme that cannot be named cannot be selected or reverted
                // to, so it is not offered; saying which one keeps a misspelt
                // custom theme diagnosable.
                tracing::warn!(%error, %slug, "settings: theme slug is not a valid identity");
                None
            }
        })
        .collect();
    Ok(SettingsSource {
        bytes,
        revision: context.settings_revision,
        active_theme: context.theme_manager.active_theme_id(),
        themes,
        environment: SettingsEnvironment {
            settings_path: path,
            state_path: context.persistence.paths_ref().state_path.clone(),
            platform: platform_name(),
            isolated: context.config_isolated,
        },
    })
}

const fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "unsupported"
    }
}

/// Schedule a save in the reducer, then perform the one it scheduled.
fn save(app_state: &mut AppStateHandle, ctx: &SharedContext, exit_after: bool) {
    dispatch(
        app_state,
        if exit_after {
            SettingsMessage::SaveAndExit
        } else {
            SettingsMessage::Save
        },
    );
    write_pending(app_state, ctx);
}

/// The revision and candidate the reducer scheduled, if it scheduled one.
fn pending_save(app_state: &AppStateHandle) -> Option<(u64, SettingsCandidate)> {
    let state = app_state.read();
    let scheduled = state.settings_state.draft.as_ref().and_then(|draft| {
        let DraftStatus::Saving { revision } = draft.status() else {
            return None;
        };
        draft
            .candidate_bytes()
            .map(|candidate| (*revision, candidate))
    });
    drop(state);
    scheduled
}

/// Perform the scheduled write and report what it did.
fn write_pending(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some((revision, candidate)) = pending_save(app_state) else {
        return;
    };
    let Some(context) = ctx.as_ref().filter(|_| true) else {
        report_save_unavailable(app_state, revision, "the settings context is unavailable");
        return;
    };
    let Ok(mut guard) = context.lock() else {
        report_save_unavailable(app_state, revision, "the settings context lock failed");
        return;
    };
    // The write happens here, on the input path, so no newer revision can be
    // scheduled between the temporary file and the replacement. Supersession is
    // the writer's answer for a concurrent scheduler, which this is not.
    let freshness = |_revision: u64| Freshness::Current;
    let outcome = guard
        .persistence
        .save_settings_candidate_revisioned(&candidate, revision, &freshness);
    if let SettingsSaveOutcome::Written { hash, .. } = &outcome {
        guard.settings_revision = revision;
        guard.settings_expected_hash = ExpectedHash::Present(*hash);
        guard.published_settings = candidate.published().clone();
    }
    drop(guard);
    dispatch(app_state, SettingsMessage::SaveCompleted(Box::new(outcome)));
}

/// Report a save that never reached the writer.
///
/// A scheduled save that no completion ever answers leaves the draft stuck in
/// `Saving` with every edit and retry disabled, so a boundary that cannot write
/// has to say so rather than quietly returning.
fn report_save_unavailable(app_state: &mut AppStateHandle, revision: u64, detail: &str) {
    dispatch(
        app_state,
        SettingsMessage::SaveCompleted(Box::new(SettingsSaveOutcome::Failed {
            revision,
            diagnostic: Box::new(settings_failure(detail)),
        })),
    );
}

/// Ask for a reload, and perform it once nothing needs confirming.
fn request_reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    dispatch(app_state, SettingsMessage::Reload);
    if !awaiting_reload_confirmation(app_state) {
        reload(app_state, ctx);
    }
}

/// Re-read the settings target and rebind the draft to those exact bytes.
fn reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    match read_source(ctx) {
        Ok(source) => dispatch(app_state, SettingsMessage::Reloaded(Box::new(source))),
        Err(detail) => dispatch(
            app_state,
            SettingsMessage::OpenFailed(Box::new(settings_failure(&detail))),
        ),
    }
}

/// Write the draft to a contained path beside the settings document.
///
/// The name is derived from the draft's own digest, so exporting the same draft
/// twice names the same file — which the writer then refuses to replace, rather
/// than quietly overwriting a rescue copy.
fn export(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(candidate) = app_state
        .read()
        .settings_state
        .draft
        .as_ref()
        .and_then(SettingsDraft::candidate_bytes)
    else {
        return;
    };
    let Some(directory) = ctx
        .as_ref()
        .and_then(|context| context.lock().ok())
        .map(|context| context.persistence.export_directory())
    else {
        report_export_unavailable(app_state, "the settings context is unavailable");
        return;
    };
    let Ok(catalog) = jefe::config_owners::builtin_owner_catalog() else {
        report_export_unavailable(app_state, "the compiled owner catalog is unavailable");
        return;
    };
    let Ok(relative) = ExportPath::parse(&format!("settings-draft-{}.toml", candidate.sha256()))
    else {
        report_export_unavailable(app_state, "the export target could not be named");
        return;
    };
    let result = export_candidate(&candidate, &directory, &relative, &catalog)
        .map_err(|diagnostic| *diagnostic);
    dispatch(
        app_state,
        SettingsMessage::ExportCompleted(Box::new(result)),
    );
}

/// Report an export that never reached the filesystem.
///
/// A recovery choice that appears to do nothing is worse than one that says why
/// it could not, because the user is already looking at a conflict.
fn report_export_unavailable(app_state: &mut AppStateHandle, detail: &str) {
    dispatch(
        app_state,
        SettingsMessage::ExportCompleted(Box::new(Err(settings_failure(detail)))),
    );
}

/// A redacted `CFG-E104` naming the settings surface rather than a path.
fn settings_failure(detail: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E104,
        Severity::Error,
        DiagnosticPath::new("/settings"),
        None,
        "preserve the draft and retry once the session is healthy",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}

/// Make the theme manager show what the state says it should be showing.
///
/// This is the whole of applying, adopting, and reverting a preview: the state
/// names one theme, and the manager wears it.
fn reconcile_theme(app_state: &AppStateHandle, ctx: &SharedContext) {
    let desired = {
        let state = app_state.read();
        state.settings_state.desired_theme().cloned()
    };
    let Some(desired) = desired else {
        return;
    };
    if let Some(context) = ctx
        && let Ok(mut context) = context.lock()
        && let Err(error) = context.theme_manager.select(&desired)
    {
        tracing::warn!(%error, "settings: could not show the selected theme");
    }
}

fn dispatch(app_state: &mut AppStateHandle, message: SettingsMessage) {
    let mut state = app_state.write();
    jefe::state::transition::commit_pure_site(&mut state, AppMessage::Settings(Box::new(message)));
}
