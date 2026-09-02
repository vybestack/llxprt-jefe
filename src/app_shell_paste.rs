//! Paste dispatch for the root application shell, split from `app_shell.rs`.
//!
//! A paste is routed by the active input mode: straight to the attached PTY,
//! into a form field, or into the issues inline/search editors.

use tracing::warn;

use crate::app_input::{durable_save_request, schedule_durable_save};
use crate::app_shell::{CtxArc, HookState};
use crate::pty_encoding::PasteEnterSuppression;
use jefe::input::{InputMode, input_mode_for_state};
use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, AppState};

pub fn handle_paste(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let input_mode = {
        let state = app_state.read();
        input_mode_for_state(&state)
    };

    match input_mode {
        InputMode::TerminalCapture => paste_to_terminal(ctx, suppress_next_enter, pasted_text),
        InputMode::Form | InputMode::Search => {
            paste_to_form(ctx, app_state, suppress_next_enter, pasted_text);
        }
        InputMode::IssuesInline => {
            paste_to_issues_inline(ctx, app_state, suppress_next_enter, pasted_text);
        }
        InputMode::IssuesSearch => {
            paste_to_issues_search(app_state, suppress_next_enter, pasted_text);
        }
        _ => {
            suppress_next_enter.set(PasteEnterSuppression::new());
        }
    }
}

fn paste_to_terminal(
    ctx: Option<&CtxArc>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return;
    };

    let bytes = if ctx_guard.runtime.bracketed_paste_active() {
        let mut payload = Vec::with_capacity(pasted_text.len() + 12);
        payload.extend_from_slice(b"\x1b[200~");
        payload.extend_from_slice(pasted_text.as_bytes());
        payload.extend_from_slice(b"\x1b[201~");
        payload
    } else {
        pasted_text.into_bytes()
    };

    if let Err(e) = ctx_guard.runtime.write_input(&bytes) {
        warn!(error = %e, "runtime.write_input failed for paste");
    }
    suppress_next_enter.set(PasteEnterSuppression::new());
}

fn paste_to_form(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let mut state = app_state.write();
    for ch in pasted_text.chars().filter(|ch| *ch != '\r' && *ch != '\n') {
        jefe::state::transition::commit_pure_site(&mut state, (AppEvent::FormChar(ch)).into());
    }
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(&ctx.cloned(), persisted);
    suppress_next_enter.set(PasteEnterSuppression::new());
}

fn paste_to_issues_inline(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let mut state = app_state.write();
    for ch in pasted_text.chars().filter(|ch| *ch != '\r') {
        if ch == '\n' {
            jefe::state::transition::commit_pure_site(&mut state, (AppEvent::InlineNewline).into());
        } else {
            jefe::state::transition::commit_pure_site(
                &mut state,
                (AppEvent::InlineChar(ch)).into(),
            );
        }
    }
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(&ctx.cloned(), persisted);
    suppress_next_enter.set(PasteEnterSuppression::new());
}

fn paste_to_issues_search(
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let mut state = app_state.write();
    let filtered: String = pasted_text
        .chars()
        .filter(|ch| *ch != '\r' && *ch != '\n')
        .collect();
    if !filtered.is_empty() {
        let mut query = state.issues_state.search_query.clone();
        query.push_str(&filtered);
        jefe::state::transition::commit_pure_site(
            &mut state,
            (AppEvent::SetSearchQuery { query }).into(),
        );
    }
    drop(state);
    suppress_next_enter.set(PasteEnterSuppression::new());
}
