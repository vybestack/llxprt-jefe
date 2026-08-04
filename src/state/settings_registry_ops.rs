//! The registry editors' half of the Settings reducer (issue #388, CW-08).
//!
//! [`super::settings`] owns the draft: what it is bound to, when it may be
//! saved, and what a completion means. This owns what the three registry
//! editors ask of that draft — which agent type is offered, which screens
//! compose and in what order, which chords dispatch which actions, and the
//! layout tree an edit is built in.
//!
//! Every one of these turns a typed intent into sparse edits and hands them to
//! the draft. None of them validates: the agent registry, the descriptor/layout
//! validator, and the action/key resolver are asked whether the candidate they
//! compose is usable, and their answers are stored rather than second-guessed.

use crate::domain::Id;
use crate::domain::action_registry::{ActionId, PROTECTED_ACTION_REASON};
use crate::domain::input_context::ContextId;
use crate::domain::keymap::Chord;
use crate::messages::NavDir;
use crate::messages::settings::LayoutMessage;
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::keymap_edit::compose_published;
use crate::persistence::{SettingsCandidate, SettingsEdit, SyntaxPath};
use crate::workbench::descriptor::{LayoutNode, ScreenDescriptor};

use super::AppState;
use super::agent_types_editor::AgentIntent;
use super::keys_editor_project::{self, CaptureOutcome, KeyIntent, classify_capture};
use super::layout_editor::{LayoutEditorState, NodeDialog};
use super::screens_editor::{self, CompositionStatus, ScreenEditorRow, ScreenIntent};
use super::settings::{CAPTURE_PROMPT, step};
use super::settings_types::ChordCapture;
use super::settings_view::{self, SettingsActivation};

/// Which side of an anchor a reordered screen lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// In front of the anchor.
    Before,
    /// Behind the anchor.
    After,
}

impl AppState {
    /// Perform whatever one row asked for.
    pub(super) fn apply_settings_activation(&mut self, activation: SettingsActivation) -> bool {
        match activation {
            SettingsActivation::Edit(edit) => self.edit_settings(edit),
            SettingsActivation::Agent(intent) => self.draft_agent(intent),
            SettingsActivation::Screen(intent) => self.draft_screen(*intent),
            SettingsActivation::Key(intent) => self.draft_key(*intent),
            SettingsActivation::CaptureChord { context, action } => {
                self.begin_chord_capture(context, action)
            }
            SettingsActivation::OpenLayout { screen_id } => self.open_layout_editor(&screen_id),
        }
    }

    /// Draft one Agent Types editor intent.
    ///
    /// Enablement may be drafted for a type the probe could not find: what is
    /// installed is a fact about this machine now, and what the document offers
    /// is a decision that outlives it. The candidate the agent registry
    /// validates decides whether the whole document still stands.
    pub(super) fn draft_agent(&mut self, intent: AgentIntent) -> bool {
        let type_id = match &intent {
            AgentIntent::SetEnabled { type_id, .. } | AgentIntent::Reset { type_id } => type_id,
        };
        let Ok(agent) = Id::parse(type_id.as_str()) else {
            // A definition whose identity the configuration grammar cannot
            // spell has no syntax to write. Saying so is the only honest
            // answer: the type stays at its compiled default.
            self.settings_state.notice =
                Some(format!("{type_id} cannot be named in settings syntax"));
            return true;
        };
        let edit = match intent {
            AgentIntent::SetEnabled { enabled, .. } => {
                SettingsEdit::AgentEnabled { agent, enabled }
            }
            AgentIntent::Reset { .. } => SettingsEdit::Reset(SyntaxPath::AgentEnabled(agent)),
        };
        self.edit_settings(edit)
    }

    /// Draft one Screens/Layout editor intent.
    ///
    /// Membership and order are rewritten together from the projected rows, so
    /// "every enabled screen exactly once and no disabled screen" holds because
    /// of how the arrays are built rather than because something checks them
    /// afterwards.
    pub(super) fn draft_screen(&mut self, intent: ScreenIntent) -> bool {
        let Ok(registry) = crate::workbench::screen_registry() else {
            self.settings_state.notice = Some("the screen registry is unavailable".to_owned());
            return true;
        };
        let Some(published) = self
            .settings_state
            .draft
            .as_ref()
            .map(|draft| draft.published().clone())
        else {
            return false;
        };
        let rows = screens_editor::project_screens(registry, &published);
        match intent {
            ScreenIntent::SetEnabled { screen_id, enabled } => {
                self.draft_screen_membership(&rows, &screen_id, enabled)
            }
            ScreenIntent::MoveBefore { screen_id, anchor } => {
                self.draft_screen_order(&rows, &screen_id, &anchor, Placement::Before)
            }
            ScreenIntent::MoveAfter { screen_id, anchor } => {
                self.draft_screen_order(&rows, &screen_id, &anchor, Placement::After)
            }
            ScreenIntent::ReplaceLayout { screen_id, layout } => {
                self.edit_settings(SettingsEdit::ReplaceLayout {
                    screen: screen_id,
                    layout,
                })
            }
            ScreenIntent::ResetLayout { screen_id } => {
                self.edit_settings(SettingsEdit::Reset(SyntaxPath::LayoutOverride(screen_id)))
            }
        }
    }

    /// Rewrite membership and order with one screen's inclusion changed.
    fn draft_screen_membership(
        &mut self,
        rows: &[ScreenEditorRow],
        screen_id: &Id,
        enabled: bool,
    ) -> bool {
        let Some(row) = rows
            .iter()
            .find(|row| row.screen_id.as_str() == screen_id.as_str())
        else {
            self.settings_state.notice = Some(format!("{screen_id} is not a known screen"));
            return true;
        };
        if let Some(reason) = row.enablement_locked {
            self.settings_state.notice = Some(reason.to_owned());
            return true;
        }
        let mut changed = rows.to_vec();
        for row in &mut changed {
            if row.screen_id.as_str() == screen_id.as_str() {
                row.enabled = enabled;
            }
        }
        let membership = screens_editor::screen_membership(&changed);
        self.edit_settings_all(vec![
            SettingsEdit::EnabledScreens(membership.clone()),
            SettingsEdit::ScreenOrder(membership),
        ])
    }

    /// Rewrite the order with one screen moved beside another.
    fn draft_screen_order(
        &mut self,
        rows: &[ScreenEditorRow],
        screen_id: &Id,
        anchor: &Id,
        placement: Placement,
    ) -> bool {
        let mut order = screens_editor::screen_membership(rows);
        let Some(from) = order.iter().position(|id| id == screen_id) else {
            self.settings_state.notice = Some(format!("{screen_id} is not an enabled screen"));
            return true;
        };
        if screen_id == anchor {
            // A screen cannot move relative to itself, and pretending it did
            // would report unsaved work that changes nothing.
            return false;
        }
        let moved = order.remove(from);
        let Some(target) = order.iter().position(|id| id == anchor) else {
            self.settings_state.notice = Some(format!("{anchor} is not an enabled screen"));
            return true;
        };
        let insert_at = match placement {
            Placement::Before => target,
            Placement::After => target + 1,
        };
        order.insert(insert_at, moved);
        self.edit_settings(SettingsEdit::ScreenOrder(order))
    }

    /// Perform whatever the focused row answers to one question.
    pub(super) fn act_on_row<F>(&mut self, ask: F) -> bool
    where
        F: Fn(&settings_view::SettingsRow) -> Option<SettingsActivation>,
    {
        let rows = settings_view::detail_rows(&self.settings_state);
        let Some(activation) = rows.get(self.settings_state.selected_row).and_then(ask) else {
            return false;
        };
        self.apply_settings_activation(activation)
    }

    /// Move the focused screen one place earlier or later in the order.
    pub(super) fn reorder_row(&mut self, direction: NavDir) -> bool {
        let rows = settings_view::detail_rows(&self.settings_state);
        let index = self.settings_state.selected_row;
        let Some(screen_id) = rows
            .get(index)
            .and_then(settings_view::SettingsRow::reorderable_screen)
            .cloned()
        else {
            return false;
        };
        let anchor = match direction {
            NavDir::Up | NavDir::Prev | NavDir::Home | NavDir::PageUp(_) => index.checked_sub(1),
            NavDir::Down | NavDir::Next | NavDir::End | NavDir::PageDown(_) => index.checked_add(1),
        };
        let Some(anchor) = anchor
            .and_then(|anchor| rows.get(anchor))
            .and_then(settings_view::SettingsRow::reorderable_screen)
            .cloned()
        else {
            return false;
        };
        let moved_up = matches!(
            direction,
            NavDir::Up | NavDir::Prev | NavDir::Home | NavDir::PageUp(_)
        );
        let intent = if moved_up {
            ScreenIntent::MoveBefore { screen_id, anchor }
        } else {
            ScreenIntent::MoveAfter { screen_id, anchor }
        };
        // The row the user is looking at moves with the screen it names, or the
        // cursor would be left pointing at whatever took its place.
        let changed = self.draft_screen(intent);
        if changed {
            self.settings_state.selected_row = if moved_up { index - 1 } else { index + 1 };
        }
        changed
    }

    /// Withdraw a waiting capture.
    pub(super) fn cancel_chord_capture(&mut self) -> bool {
        if self.settings_state.capture.take().is_none() {
            return false;
        }
        self.settings_state.notice = Some("Capture cancelled".to_owned());
        true
    }

    /// Move, edit, or apply the open layout tree editor.
    pub(super) fn reduce_layout(&mut self, message: LayoutMessage) -> bool {
        let Some(screen) = self
            .settings_state
            .layout_editor
            .as_ref()
            .map(|editor| editor.screen_id.clone())
            .and_then(|id| Self::settings_screen(&id))
        else {
            return false;
        };
        match message {
            LayoutMessage::Apply => return self.apply_layout_editor(),
            LayoutMessage::Cancel => return self.close_layout_editor(),
            LayoutMessage::ResetOverride => {
                let Some(screen_id) = self
                    .settings_state
                    .layout_editor
                    .as_ref()
                    .map(|editor| editor.screen_id.clone())
                else {
                    return false;
                };
                self.settings_state.layout_editor = None;
                return self.draft_screen(ScreenIntent::ResetLayout { screen_id });
            }
            _ => {}
        }
        let Some(editor) = self.settings_state.layout_editor.as_mut() else {
            return false;
        };
        apply_layout_message(editor, &screen, message);
        // A structural refusal belongs on the screen's own notice row, not
        // inside the layout pane: the pane is a narrow column beside two
        // others, and a validator's reason wrapped across it is harder to read
        // than the tree it is about.
        self.settings_state.notice = self
            .settings_state
            .layout_editor
            .as_ref()
            .and_then(|editor| editor.notice.clone());
        true
    }

    /// Wait for exactly the next chord, to bind it to this action.
    fn begin_chord_capture(&mut self, context: ContextId, action: ActionId) -> bool {
        if let Some(reason) = self.protected_reason(&context, &action) {
            self.settings_state.notice = Some(reason);
            return true;
        }
        self.settings_state.capture = Some(ChordCapture { context, action });
        self.settings_state.notice = Some(CAPTURE_PROMPT.to_owned());
        true
    }

    /// Take, cancel, or refuse one chord offered to a waiting capture.
    pub(super) fn resolve_chord_capture(&mut self, chord: Chord) -> bool {
        let Some(capture) = self.settings_state.capture.take() else {
            return false;
        };
        match classify_capture(chord) {
            CaptureOutcome::Captured(chord) => {
                self.settings_state.notice = None;
                self.draft_key(KeyIntent::CaptureSingleChord {
                    context: capture.context,
                    action: capture.action,
                    chord,
                })
            }
            CaptureOutcome::Cancelled => {
                self.settings_state.notice = Some("Capture cancelled".to_owned());
                true
            }
            CaptureOutcome::Protected => {
                self.settings_state.notice = Some(PROTECTED_ACTION_REASON.to_owned());
                true
            }
        }
    }

    /// Open the layout tree editor on one screen's current layout.
    fn open_layout_editor(&mut self, screen_id: &Id) -> bool {
        let Some(screen) = Self::settings_screen(screen_id) else {
            self.settings_state.notice = Some(format!("{screen_id} is not a known screen"));
            return true;
        };
        let layout = self.drafted_layout(&screen).unwrap_or(screen.layout);
        self.settings_state.layout_editor =
            Some(LayoutEditorState::open(screen_id.clone(), layout));
        self.settings_state.notice = None;
        true
    }

    /// Apply the layout editor's tree, when the validator accepts it.
    fn apply_layout_editor(&mut self) -> bool {
        let Some(editor) = self.settings_state.layout_editor.clone() else {
            return false;
        };
        let Some(screen) = Self::settings_screen(&editor.screen_id) else {
            return false;
        };
        match editor.complete(&screen) {
            Ok(layout) => {
                self.settings_state.layout_editor = None;
                self.draft_screen(ScreenIntent::ReplaceLayout {
                    screen_id: editor.screen_id,
                    layout: Box::new(layout),
                })
            }
            Err(reason) => {
                if let Some(open) = self.settings_state.layout_editor.as_mut() {
                    open.notice = Some(reason);
                }
                true
            }
        }
    }

    /// Abandon the layout edit, leaving the draft exactly as it was.
    fn close_layout_editor(&mut self) -> bool {
        if self.settings_state.layout_editor.take().is_none() {
            return false;
        }
        true
    }

    /// The descriptor of one screen the registry knows.
    fn settings_screen(screen_id: &Id) -> Option<ScreenDescriptor> {
        crate::workbench::screen_registry()
            .ok()?
            .screens()
            .iter()
            .find(|screen| screen.id.as_str() == screen_id.as_str())
            .cloned()
    }

    /// The layout the candidate currently overrides this screen with, if any.
    fn drafted_layout(&self, screen: &ScreenDescriptor) -> Option<LayoutNode> {
        let published = self.settings_state.draft.as_ref()?.published();
        let id = Id::parse(screen.id.as_str()).ok()?;
        let values = published.workbench.layout_overrides.get(&id)?;
        super::screens_editor_layout::read(values, screen).ok()
    }

    /// Draft one Keys editor intent.
    ///
    /// A protected action is refused here with the registry's own reason rather
    /// than written and then refused by composition: the user asked to change a
    /// control that must keep working, and telling them why is more use than a
    /// candidate that will not save.
    ///
    /// Everything else is written and left to the action/key resolver, which
    /// owns chord grammar, conflicts, and every limit.
    pub(super) fn draft_key(&mut self, intent: KeyIntent) -> bool {
        let (context, action) = intent.binding();
        if let Some(reason) = self.protected_reason(context, action) {
            self.settings_state.notice = Some(reason);
            return true;
        }
        let (context, action) = (context.clone(), action.clone());
        let edit = match intent {
            KeyIntent::CaptureSingleChord { chord, .. } => SettingsEdit::Keymap {
                context,
                action,
                chords: vec![chord],
            },
            KeyIntent::SetChords { chords, .. } => SettingsEdit::Keymap {
                context,
                action,
                chords,
            },
            KeyIntent::Unbind { .. } => SettingsEdit::Keymap {
                context,
                action,
                chords: Vec::new(),
            },
            KeyIntent::Reset { .. } => SettingsEdit::Reset(SyntaxPath::Keymap { context, action }),
        };
        self.edit_settings(edit)
    }

    /// Why this binding is read-only, when the registry says it is.
    fn protected_reason(&self, context: &ContextId, action: &ActionId) -> Option<String> {
        let snapshot = self.action_registry_snapshot.as_ref()?;
        let published = self
            .settings_state
            .draft
            .as_ref()
            .map(|draft| draft.published().clone())
            .unwrap_or_default();
        keys_editor_project::project_keys(snapshot, &published)
            .into_iter()
            .find(|row| &row.context == context && &row.action == action)
            .and_then(|row| row.protected)
    }
}

/// Apply one movement or keystroke to the open layout editor.
///
/// Everything here changes the editor and nothing else. The tree reaches the
/// draft only through [`AppState::apply_layout_editor`], which is what keeps an
/// unfinished edit out of the document.
fn apply_layout_message(
    editor: &mut LayoutEditorState,
    screen: &ScreenDescriptor,
    message: LayoutMessage,
) {
    match message {
        LayoutMessage::SelectPrevious => editor.select_previous(),
        LayoutMessage::SelectNext => editor.select_next(),
        LayoutMessage::SelectParent => editor.select_parent(),
        LayoutMessage::SelectChild => editor.select_child(),
        LayoutMessage::BeginAdd => editor.dialog = Some(NodeDialog::adding()),
        LayoutMessage::BeginEdit => editor.dialog = editing_dialog(editor),
        LayoutMessage::ChoosePanel(direction) => choose_panel(editor, screen, direction),
        LayoutMessage::NextField => dialog_mut(editor, NodeDialog::next_field),
        LayoutMessage::TypeChar(character) => {
            if let Some(dialog) = editor.dialog.as_mut() {
                dialog.push(character);
            }
        }
        LayoutMessage::Backspace => dialog_mut(editor, NodeDialog::backspace),
        LayoutMessage::ToggleField => dialog_mut(editor, NodeDialog::toggle),
        LayoutMessage::ApplyDialog => editor.apply_dialog(screen),
        LayoutMessage::CancelDialog => editor.dialog = None,
        LayoutMessage::Split(axis) => editor.split_selected(axis),
        LayoutMessage::Remove => editor.remove_selected(screen),
        // Handled before the editor is borrowed, because each of these ends the
        // edit rather than changing it.
        LayoutMessage::Apply | LayoutMessage::Cancel | LayoutMessage::ResetOverride => {}
    }
}

fn dialog_mut<F: Fn(&mut NodeDialog)>(editor: &mut LayoutEditorState, apply: F) {
    if let Some(dialog) = editor.dialog.as_mut() {
        apply(dialog);
    }
}

/// The dialog editing whichever child is selected, when one is.
fn editing_dialog(editor: &LayoutEditorState) -> Option<NodeDialog> {
    let (index, parent) = editor.selected.split_last()?;
    let LayoutNode::Split { children, .. } = node_at(&editor.tree, parent)? else {
        return None;
    };
    children.get(*index).map(NodeDialog::editing)
}

/// The node `path` names, if the tree still has one there.
fn node_at<'tree>(tree: &'tree LayoutNode, path: &[usize]) -> Option<&'tree LayoutNode> {
    let Some((index, rest)) = path.split_first() else {
        return Some(tree);
    };
    let LayoutNode::Split { children, .. } = tree else {
        return None;
    };
    node_at(&children.get(*index)?.node, rest)
}

fn choose_panel(editor: &mut LayoutEditorState, screen: &ScreenDescriptor, direction: NavDir) {
    let count = editor.addable_panels(screen).len();
    let Some(dialog) = editor.dialog.as_mut() else {
        return;
    };
    dialog.panel_choice = step(dialog.panel_choice, count, direction);
}

/// Every reason a registry owner refuses this candidate.
///
/// The document publishing is not the whole of "this candidate is valid": the
/// registries composed from it have their own rules, and a candidate that
/// publishes but composes into no keymap or an unusable screen is one a save
/// would make the session unable to start from. Each owner is asked, and each
/// answers in its own words.
pub(super) fn registry_refusals(candidate: &SettingsCandidate) -> Vec<Diagnostic> {
    let mut refusals = Vec::new();
    if let Err(diagnostic) = compose_published(candidate.published(), "settings") {
        refusals.push(diagnostic.as_settings_diagnostic());
    }
    refusals.extend(screen_refusals(candidate));
    refusals.sort();
    refusals
}

/// Every screen whose candidate layout the descriptor validator refuses.
fn screen_refusals(candidate: &SettingsCandidate) -> Vec<Diagnostic> {
    let Ok(registry) = crate::workbench::screen_registry() else {
        return Vec::new();
    };
    screens_editor::project_screens(registry, candidate.published())
        .into_iter()
        .filter_map(|row| match row.composition {
            CompositionStatus::Valid => None,
            CompositionStatus::Invalid { code, reason } => {
                Some(layout_diagnostic(row.screen_id.as_str(), &code, &reason))
            }
        })
        .collect()
}

fn layout_diagnostic(screen: &str, code: &str, reason: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E005,
        Severity::Error,
        DiagnosticPath::new(format!("/workbench/layout_overrides/{screen}")),
        None,
        "correct the layout override, or reset it to the compiled layout",
    );
    diagnostic.redacted_detail = format!("{code}: {reason}");
    diagnostic
}
