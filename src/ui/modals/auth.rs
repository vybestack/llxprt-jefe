//! In-app device-code auth remediation modal (issue #244).
//!
//! Follows the pure-views pattern: `auth_dialog_view` is an iocraft-free,
//! side-effect-free projection that turns an `AuthDialogState` into a fixed
//! list of display lines. The `AuthModal` iocraft component only renders that
//! projection.
//!
//! @plan PLAN-20260712-AUTH-DIALOG.P04

use iocraft::prelude::*;

use crate::selection::{SelectablePane, TextSelection};
use crate::state::{AuthDialogPhase, AuthDialogState};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// The fixed title of the auth dialog.
pub const AUTH_MODAL_TITLE: &str = "Authenticate with GitHub";

/// A single display line in the auth dialog projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthDialogLine {
    pub text: String,
}

/// The pure projection of the auth dialog: a fixed list of display lines
/// derived solely from the dialog state. No iocraft types, no `Color`, no
/// runtime — trivially unit-testable.
#[must_use]
pub fn auth_dialog_view(state: &AuthDialogState) -> Vec<AuthDialogLine> {
    match &state.phase {
        AuthDialogPhase::Idle | AuthDialogPhase::AwaitingCode => awaiting_code_lines(),
        AuthDialogPhase::Confirming { code, url } => confirming_lines(code, url),
        AuthDialogPhase::Failed { error, can_retry } => failed_lines(error, *can_retry),
        AuthDialogPhase::Cancelled => vec![AuthDialogLine {
            text: String::from("Cancelled."),
        }],
    }
}

fn line(text: impl Into<String>) -> AuthDialogLine {
    AuthDialogLine { text: text.into() }
}

fn awaiting_code_lines() -> Vec<AuthDialogLine> {
    vec![
        line(AUTH_MODAL_TITLE),
        line(String::new()),
        line("Starting the GitHub device-code flow..."),
        line("A one-time code will appear here shortly."),
        line("Press Esc to cancel."),
    ]
}

fn confirming_lines(code: &str, url: &str) -> Vec<AuthDialogLine> {
    vec![
        line(AUTH_MODAL_TITLE),
        line(String::new()),
        line(format!("One-time code: {code}")),
        line(format!("Open this URL: {url}")),
        line("Enter the code in your browser to authorize Jefe."),
        line("Scopes requested: repo, read:org, gist"),
        line("Waiting for authorization... Press Esc to cancel."),
    ]
}

fn failed_lines(error: &str, can_retry: bool) -> Vec<AuthDialogLine> {
    let retry_hint = if can_retry {
        "Press r or Enter to retry. Press Esc to cancel."
    } else {
        "Press Esc to cancel."
    };
    vec![
        line(AUTH_MODAL_TITLE),
        line(String::new()),
        line(format!("Authentication failed: {error}")),
        line(retry_hint),
    ]
}

/// Props for the auth modal.
#[derive(Default, Props)]
pub struct AuthModalProps {
    pub state: AuthDialogState,
    pub colors: ThemeColors,
    pub selection: Option<TextSelection>,
}

/// Auth remediation modal — render-only.
#[component]
pub fn AuthModal(props: &AuthModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let sel = SelectionColors::from_resolved(&rc);
    let pane = SelectablePane::ConfirmModal;
    let selection = props.selection;

    let view = auth_dialog_view(&props.state);
    let lines: Vec<AnyElement<'static>> = view
        .iter()
        .enumerate()
        .map(|(idx, line)| selectable_line(&line.text, idx, selection, pane, rc.fg, sel))
        .collect();

    // Padding (2) plus the projected line count, clamped to a sane height.
    // `u32::try_from` avoids the truncation cast lint; the line count is tiny
    // so the fallback is never hit in practice.
    let line_count = u32::try_from(lines.len().max(1)).unwrap_or(50);
    let height = line_count + 2;

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
            #(lines)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AuthDialogPhase;

    fn phase(p: AuthDialogPhase) -> AuthDialogState {
        AuthDialogState { phase: p }
    }

    #[test]
    fn awaiting_code_view_has_title_and_cancel_hint() {
        let view = auth_dialog_view(&phase(AuthDialogPhase::AwaitingCode));
        let texts: Vec<&str> = view.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts.first(), Some(&AUTH_MODAL_TITLE));
        assert!(
            texts.iter().any(|t| t.contains("Esc")),
            "must tell the user how to cancel: {texts:?}"
        );
    }

    #[test]
    fn confirming_view_shows_code_and_url_and_scopes() {
        let view = auth_dialog_view(&phase(AuthDialogPhase::Confirming {
            code: "7701-C5F6".to_string(),
            url: "https://github.com/login/device".to_string(),
        }));
        let joined = view
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("7701-C5F6"), "must show the code");
        assert!(
            joined.contains("https://github.com/login/device"),
            "must show the URL"
        );
        assert!(
            joined.contains("repo, read:org, gist"),
            "must disclose the requested scopes for informed consent"
        );
    }

    #[test]
    fn failed_view_shows_error_and_retry_hint() {
        let view = auth_dialog_view(&phase(AuthDialogPhase::Failed {
            error: "the device code expired".to_string(),
            can_retry: true,
        }));
        let joined = view
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("the device code expired"));
        assert!(joined.contains("retry"), "must offer retry when can_retry");
    }

    #[test]
    fn failed_view_no_retry_omits_retry_hint() {
        let view = auth_dialog_view(&phase(AuthDialogPhase::Failed {
            error: "denied".to_string(),
            can_retry: false,
        }));
        let joined = view
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!joined.contains("retry"));
        assert!(joined.contains("Esc"));
    }

    #[test]
    fn idle_view_matches_awaiting() {
        // Idle is the pre-open default; it renders the same waiting screen.
        let idle = auth_dialog_view(&phase(AuthDialogPhase::Idle));
        let awaiting = auth_dialog_view(&phase(AuthDialogPhase::AwaitingCode));
        assert_eq!(idle, awaiting);
    }
}
