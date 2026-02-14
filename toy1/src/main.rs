//! Jefe TUI - Repository and AI agent orchestrator.
//!
//! A terminal user interface for managing llxprt-code AI agent instances
//! across multiple repositories. This is a toy prototype with mock data.

mod app;
mod data;
mod events;
mod presenter;
mod pty;
mod theme;
mod ui;

use app::{AppState, ModalState, Screen};
use data::mock::generate_mock_data;
use data::models::AgentStatus;
use events::AppEvent;
use iocraft::prelude::*;
use pty::{PtyManager, TerminalColorDefaults};
use theme::ThemeManager;
use ui::modals::confirm::ConfirmModal;
use ui::modals::help::HelpModal;
use ui::screens::agent_detail::AgentDetail;
use ui::screens::dashboard::Dashboard;
use ui::screens::new_agent::NewAgentForm;
use ui::screens::new_repository::NewRepositoryForm;
use ui::screens::split::SplitScreen;

use std::sync::Arc;

const JEFE_MOUSE_DEBUG_ENV: &str = "JEFE_MOUSE_DEBUG";

/// Props for the root app component.
#[derive(Default, Props)]
struct AppProps {
    /// Shared PTY manager (passed from main, Arc-wrapped).
    pub pty_manager: Option<Arc<PtyManager>>,
}

const LEFT_COL_WIDTH: u16 = 22;
const RIGHT_COL_WIDTH: u16 = 36;
const OUTER_BARS_HEIGHT: u16 = 2; // status + keybind
const TERMINAL_WIDGET_CHROME_ROWS: u16 = 3; // top/bottom border + header row
const TERMINAL_WIDGET_CHROME_COLS: u16 = 2; // left/right border

fn is_fullscreen_enabled() -> bool {
    // Fullscreen by default so the app runs in alternate screen mode,
    // captures mouse events, and avoids host-terminal scrollback artifacts.
    // Set JEFE_WINDOWED=1 only for local debugging of non-fullscreen mode.
    std::env::var("JEFE_WINDOWED").ok().as_deref() != Some("1")
}

fn effective_render_rows(term_rows: u16) -> u16 {
    if is_fullscreen_enabled() {
        term_rows
    } else {
        // Leave extra slack in non-fullscreen mode to avoid host-terminal scroll artifacts.
        term_rows.saturating_sub(2).max(1)
    }
}

fn effective_render_cols(term_cols: u16) -> u16 {
    if is_fullscreen_enabled() {
        term_cols
    } else {
        // Keep one extra column free to avoid right-edge auto-wrap.
        term_cols.saturating_sub(2).max(1)
    }
}

fn compute_layout(term_cols: u16, term_rows: u16) -> (u16, u16, u16, u16) {
    let render_rows = effective_render_rows(term_rows);
    let render_cols = effective_render_cols(term_cols);

    // Content area excludes top status bar + bottom keybind bar.
    let content_rows = render_rows.saturating_sub(OUTER_BARS_HEIGHT);
    let middle_cols = render_cols.saturating_sub(LEFT_COL_WIDTH + RIGHT_COL_WIDTH);

    // In dashboard middle column: agent list top 25%, terminal bottom 75%.
    let agent_rows = content_rows.saturating_mul(25).saturating_div(100);
    let terminal_slot_rows = content_rows.saturating_sub(agent_rows);

    // Terminal widget has border + header chrome.
    let pty_rows = terminal_slot_rows
        .saturating_sub(TERMINAL_WIDGET_CHROME_ROWS)
        .max(2);
    let pty_cols = middle_cols
        .saturating_sub(TERMINAL_WIDGET_CHROME_COLS)
        .max(2);

    // Terminal pane begins after left column and after status+agent list+top border+header.
    let pane_col0 = LEFT_COL_WIDTH.saturating_add(1); // inside left border
    let pane_row0 = 1u16
        .saturating_add(agent_rows)
        .saturating_add(2); // status bar + terminal top border + header row

    (pty_rows, pty_cols, pane_col0, pane_row0)
}

/// Root application component that manages state and renders the active screen.
fn mouse_debug_enabled() -> bool {
    matches!(
        std::env::var(JEFE_MOUSE_DEBUG_ENV).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}


fn fmt_mouse_kind(kind: iocraft::MouseEventKind) -> &'static str {
    use iocraft::MouseEventKind;
    match kind {
        MouseEventKind::Down(_) => "Down",
        MouseEventKind::Up(_) => "Up",
        MouseEventKind::Drag(_) => "Drag",
        MouseEventKind::Moved => "Moved",
        MouseEventKind::ScrollUp => "ScrollUp",
        MouseEventKind::ScrollDown => "ScrollDown",
        MouseEventKind::ScrollLeft => "ScrollLeft",
        MouseEventKind::ScrollRight => "ScrollRight",
    }
}

fn parse_hex_rgb(hex: &str) -> Option<alacritty_terminal::vte::ansi::Rgb> {
    let value = hex.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;

    Some(alacritty_terminal::vte::ansi::Rgb { r, g, b })
}

fn to_rgb(hex: &str, fallback_r: u8, fallback_g: u8, fallback_b: u8) -> alacritty_terminal::vte::ansi::Rgb {
    parse_hex_rgb(hex).unwrap_or(alacritty_terminal::vte::ansi::Rgb {
        r: fallback_r,
        g: fallback_g,
        b: fallback_b,
    })
}

#[component]
fn App(mut hooks: Hooks, props: &AppProps) -> impl Into<AnyElement<'static>> {
    let mut should_quit = hooks.use_state(|| false);
    let mut app_state = hooks.use_state(|| AppState::new(generate_mock_data()));
    let mut theme_mgr = hooks.use_state(ThemeManager::new);
    // Counter bumped to force re-render when PTY output arrives.
    let mut render_tick = hooks.use_state(|| 0u64);
    // Some terminals may only emit release events; track if we've seen
    // press/repeat so we can avoid duplicate handling where possible.
    let mut saw_non_release_key = hooks.use_state(|| false);

    // Mouse coordinates from crossterm/iocraft are 1-based screen positions.
    // We normalize to 0-based before mapping into pane-local coordinates.

    let mouse_debug = mouse_debug_enabled();
    let key_debug = true;

    let pty_mgr = props.pty_manager.clone();

    // Poll for PTY output updates (~30fps).
    hooks.use_future(async move {
        loop {
            smol::Timer::after(std::time::Duration::from_millis(33)).await;
            let tick = render_tick.get();
            render_tick.set(tick.wrapping_add(1));
        }
    });

    hooks.use_terminal_events({
        let pty_mgr_for_events = props.pty_manager.clone();
        move |event| match event {
            TerminalEvent::Resize(cols, rows) => {
                if let Some(ref mgr) = pty_mgr_for_events {
                    let (pty_rows, pty_cols, _, _) = compute_layout(cols, rows);
                    mgr.resize_all(pty_rows, pty_cols);
                }
            }
            TerminalEvent::FullscreenMouse(mouse_event) => {
                // When terminal is focused: pass ALL mouse events through to the PTY,
                // mapped into terminal-pane local coordinates and clamped to bounds.
                let term_focused = app_state.read().terminal_focused;

                if mouse_debug {
                    eprintln!(
                        "[mouse-raw] focused={} kind={} raw=({}, {}) mods={:?}",
                        term_focused,
                        fmt_mouse_kind(mouse_event.kind),
                        mouse_event.column,
                        mouse_event.row,
                        mouse_event.modifiers,
                    );
                }

                if term_focused {
                    if let Some(ref mgr) = pty_mgr_for_events {
                        let idx = app_state.read().global_agent_index();

                        let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
                        let (pty_rows, pty_cols, pane_col0, pane_row0) = compute_layout(cols, rows);

                        let row_end = pane_row0.saturating_add(pty_rows.saturating_sub(1));
                        let col_end = pane_col0.saturating_add(pty_cols.saturating_sub(1));

                        // iocraft fullscreen mouse coordinates are already 0-based relative
                        // to the rendered root component, so use them directly.
                        let screen_row0 = mouse_event.row;
                        let screen_col0 = mouse_event.column;

                        let in_terminal_bounds = screen_col0 >= pane_col0
                            && screen_col0 <= col_end
                            && screen_row0 >= pane_row0
                            && screen_row0 <= row_end;

                        if !in_terminal_bounds {
                            if mouse_debug {
                                eprintln!(
                                    "[mouse] focused={} idx={} kind={} raw=({}, {}) pane_col0={} pane_row0={} pane_end=({}, {}) in_bounds=false",
                                    term_focused,
                                    idx,
                                    fmt_mouse_kind(mouse_event.kind),
                                    mouse_event.column,
                                    mouse_event.row,
                                    pane_col0,
                                    pane_row0,
                                    col_end,
                                    row_end,
                                );
                            }
                            return;
                        }

                        let local_row = screen_row0.saturating_sub(pane_row0);
                        let local_col = screen_col0.saturating_sub(pane_col0);

                        // Only forward mouse bytes when the child app has explicitly enabled
                        // terminal mouse reporting. Otherwise keep host-terminal selection behavior.
                        let app_mouse_mode = mgr.mouse_reporting_active(idx);

                        if mouse_debug {
                            eprintln!(
                                "[mouse] focused={} idx={} kind={} raw=({}, {}) raw0=({}, {}) pane_col0={} pane_row0={} pane_end=({}, {}) local=({}, {}) app_mouse_mode={} in_bounds=true",
                                term_focused,
                                idx,
                                fmt_mouse_kind(mouse_event.kind),
                                mouse_event.column,
                                mouse_event.row,
                                screen_col0,
                                screen_row0,
                                pane_col0,
                                pane_row0,
                                col_end,
                                row_end,
                                local_col,
                                local_row,
                                app_mouse_mode,
                            );
                        }

                        if app_mouse_mode {
                            let mut local_event =
                                iocraft::FullscreenMouseEvent::new(mouse_event.kind, local_col, local_row);
                            local_event.modifiers = mouse_event.modifiers;

                            if let Some(bytes) = pty::mouse_event_to_bytes(&local_event) {
                                if mouse_debug {
                                    eprintln!(
                                        "[mouse->pty] bytes={} {:?}",
                                        bytes.len(),
                                        String::from_utf8_lossy(&bytes)
                                    );
                                }
                                mgr.write_input(idx, &bytes);
                            }
                        }
                    }
                }
            }
            TerminalEvent::Key(key_event) => {
                let is_searching = app_state.read().is_searching;
                let term_focused = app_state.read().terminal_focused;
                let in_input_screen = false;

                if key_event.kind != KeyEventKind::Release {
                    saw_non_release_key.set(true);
                } else if saw_non_release_key.get() {
                    // If press/repeat events are available, ignore release duplicates.
                    if key_debug {
                        eprintln!("[key] ignore-release code={:?}", key_event.code);
                    }
                    return;
                }

                if key_debug {
                    eprintln!(
                        "[key-raw] kind={:?} code={:?} mods={:?} term_focused={} is_searching={} screen={:?}",
                        key_event.kind,
                        key_event.code,
                        key_event.modifiers,
                        term_focused,
                        is_searching,
                        app_state.read().screen,
                    );
                }

                // Toggle terminal focus uses F12 only.
                let is_toggle_terminal_focus = key_event.code == KeyCode::F(12);
                if is_toggle_terminal_focus {
                    let mut state = app_state.write();
                    state.handle_event(AppEvent::ToggleTerminalFocus);
                    if key_debug {
                        eprintln!("[key] handled=ToggleTerminalFocus");
                    }
                    return;
                }

                // When terminal is focused, forward everything to the PTY.
                // (F12 above is the only escape hatch.)
                if term_focused {
                    if let Some(ref mgr) = pty_mgr_for_events {
                        let idx = app_state.read().global_agent_index();
                        if let Some(bytes) = pty::key_event_to_bytes(&key_event) {
                            if key_debug {
                                eprintln!("[key] forwarded-to-pty bytes={} code={:?}", bytes.len(), key_event.code);
                            }
                            mgr.write_input(idx, &bytes);
                        } else if key_debug {
                            eprintln!("[key] drop-in-terminal (no-bytes) code={:?}", key_event.code);
                        }
                    }
                    return;
                }

                // Normal Jefe keybindings (press/repeat).
                let screen_now = app_state.read().screen;
                let app_event = match key_event.code {
                    KeyCode::Char('q') if !is_searching => Some(AppEvent::Quit),
                    KeyCode::Char('Q') if !is_searching => Some(AppEvent::Quit),
                    KeyCode::Char('n') if !is_searching => Some(AppEvent::NewAgent),
                    KeyCode::Char('N') if !is_searching => Some(AppEvent::NewRepository),
                    KeyCode::Char('d') | KeyCode::Char('D')
                        if !is_searching && !in_input_screen && screen_now != Screen::Split =>
                    {
                        Some(AppEvent::DeleteAgent)
                    }
                    KeyCode::Char('/') => Some(AppEvent::OpenSearch),
                    KeyCode::Char('?') if !is_searching => Some(AppEvent::OpenHelp),
                    KeyCode::Char('r') | KeyCode::Char('R') if !is_searching && !in_input_screen => {
                        Some(AppEvent::FocusRepository)
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') if !is_searching && !in_input_screen => {
                        Some(AppEvent::FocusAgentList)
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') if !is_searching && !in_input_screen => {
                        Some(AppEvent::FocusTerminal)
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') if !is_searching && !in_input_screen => {
                        Some(AppEvent::ToggleSplitMode)
                    }
                    KeyCode::Char('k') | KeyCode::Char('K')
                        if !is_searching && !in_input_screen && screen_now != Screen::Split =>
                    {
                        Some(AppEvent::KillAgent)
                    }
                    KeyCode::Char('l') | KeyCode::Char('L')
                        if !is_searching && !in_input_screen && screen_now != Screen::Split =>
                    {
                        Some(AppEvent::RelaunchAgent)
                    }
                    KeyCode::Char('m') | KeyCode::Char('M') if !is_searching && !in_input_screen => {
                        Some(AppEvent::ReturnToMainFocused)
                    }
                    KeyCode::Up => Some(AppEvent::NavigateUp),
                    KeyCode::Down => Some(AppEvent::NavigateDown),
                    KeyCode::Left => Some(AppEvent::NavigateLeft),
                    KeyCode::Right => Some(AppEvent::NavigateRight),
                    KeyCode::Enter => Some(AppEvent::Select),
                    KeyCode::Esc => Some(AppEvent::Back),
                    KeyCode::Char(c) if is_searching => Some(AppEvent::Char(c)),
                    _ => None,
                };

                if let Some(evt) = app_event {
                    if key_debug {
                        eprintln!("[key] mapped-event={:?}", evt);
                    }

                    if evt == AppEvent::Quit {
                        should_quit.set(true);
                    } else if evt == AppEvent::OpenSearch {
                        let mut state = app_state.write();
                        state.handle_event(evt);
                    } else {
                        let mut state = app_state.write();
                        match evt {
                            AppEvent::KillAgent => {
                                let idx = state.global_agent_index();
                                if let Some(ref mgr) = pty_mgr_for_events {
                                    mgr.kill_session(idx);
                                }
                                state.handle_event(evt);
                                state.terminal_focused = false;
                            }
                            AppEvent::RelaunchAgent => {
                                let should_relaunch = state
                                    .current_agent()
                                    .is_some_and(|a| a.status == AgentStatus::Dead);
                                if should_relaunch {
                                    let idx = state.global_agent_index();
                                    if let Some(ref mgr) = pty_mgr_for_events {
                                        let _ = mgr.relaunch_session(idx);
                                    }
                                }
                                state.handle_event(evt);
                            }
                            _ => state.handle_event(evt),
                        }
                    }
                } else if key_debug {
                    eprintln!("[key] unmapped code={:?}", key_event.code);
                }
            }
            _ => {}
        }
    });

    // Handle quit via system exit.
    if should_quit.get() {
        hooks.use_context_mut::<SystemContext>().exit();
        let rc = crate::theme::ResolvedColors::from_theme(None);
        let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
        let render_rows = effective_render_rows(term_rows);
        let render_cols = effective_render_cols(term_cols);
        return element! {
            Box(
                background_color: rc.bg,
                width: u32::from(render_cols),
                height: u32::from(render_rows),
            )
        };
    }

    // Keep app model status in sync with PTY liveness.
    // IMPORTANT: only call app_state.write() when something actually changed,
    // because write() marks state dirty and triggers a re-render.  Calling it
    // unconditionally creates an infinite render loop that starves the event
    // stream — which is why keys stopped working.
    if let Some(ref mgr) = pty_mgr {
        let mut need_update = Vec::new();
        {
            let state_ro = app_state.read();
            let mut global_idx = 0usize;
            for (ri, repo) in state_ro.repositories.iter().enumerate() {
                for (ai, agent) in repo.agents.iter().enumerate() {
                    if agent.status != AgentStatus::Dead && !mgr.is_alive(global_idx) {
                        need_update.push((ri, ai));
                    }
                    global_idx = global_idx.saturating_add(1);
                }
            }
        }
        if !need_update.is_empty() {
            let mut state_mut = app_state.write();
            for (ri, ai) in need_update {
                if let Some(repo) = state_mut.repositories.get_mut(ri) {
                    if let Some(agent) = repo.agents.get_mut(ai) {
                        agent.status = AgentStatus::Dead;
                    }
                }
            }
        }
    }

    // Read state for rendering.
    let state_ref = app_state.read();
    let theme_ref = theme_mgr.read();

    let current_screen = state_ref.screen;
    let show_help = state_ref.modal == ModalState::Help;
    let show_confirm = matches!(state_ref.modal, ModalState::ConfirmKill(_));
    let confirm_msg = if let ModalState::ConfirmKill(idx) = &state_ref.modal {
        state_ref
            .current_repo()
            .and_then(|p| p.agents.get(*idx))
            .map_or("Kill agent?".to_owned(), |a| {
                format!("Kill agent for {} {}?", a.display_id, a.purpose)
            })
    } else {
        String::new()
    };

    let snapshot = state_ref.clone();
    let theme_name = theme_ref.active().name.clone();
    let colors = theme_ref.colors().clone();

    if let Some(ref mgr) = pty_mgr {
        let defaults = TerminalColorDefaults {
            fg: to_rgb(colors.foreground.as_str(), 0x6a, 0x99, 0x55),
            bg: to_rgb(colors.background.as_str(), 0, 0, 0),
            bright: to_rgb(colors.bright_foreground.as_str(), 0x00, 0xff, 0x00),
            dim: to_rgb(colors.dim_foreground.as_str(), 0x4a, 0x70, 0x35),
            selection_fg: to_rgb(colors.selection_fg.as_str(), 0, 0, 0),
            selection_bg: to_rgb(colors.selection_bg.as_str(), 0x6a, 0x99, 0x55),
        };
        mgr.set_color_defaults(defaults);
    }

    // Read PTY terminal content for the active agent.
    let (terminal_lines, terminal_snapshot) = if let Some(ref mgr) = pty_mgr {
        let idx = snapshot.global_agent_index();
        let styled = mgr.terminal_snapshot(idx);
        let lines = styled
            .cells
            .iter()
            .map(|row| row.iter().map(|cell| cell.ch).collect())
            .collect();
        (lines, Some(styled))
    } else {
        (vec!["(no PTY manager)".to_owned()], None)
    };

    drop(state_ref);
    drop(theme_ref);
    // Consume the tick so clippy doesn't complain about unused state.
    let _ = render_tick.get();

    let screen_el: AnyElement<'static> = match current_screen {
        Screen::Dashboard | Screen::CommandPalette | Screen::Terminal => element! {
            Dashboard(
                state: snapshot.clone(),
                colors: colors.clone(),
                theme_name: theme_name.clone(),
                terminal_lines: terminal_lines,
                terminal_snapshot: terminal_snapshot,
            )
        }
        .into(),
        Screen::AgentDetail => element! {
            AgentDetail(
                state: snapshot.clone(),
                colors: colors.clone(),
                theme_name: theme_name.clone(),
            )
        }
        .into(),
        Screen::NewAgent => element! {
            NewAgentForm(
                state: snapshot.clone(),
                colors: colors.clone(),
            )
        }
        .into(),
        Screen::NewRepository => element! {
            NewRepositoryForm(
                state: snapshot.clone(),
                colors: colors.clone(),
            )
        }
        .into(),
        Screen::Split => element! {
            SplitScreen(
                state: snapshot.clone(),
                colors: colors.clone(),
                theme_name: theme_name.clone(),
            )
        }
        .into(),
    };

    let modal_els: Vec<AnyElement<'static>> = if show_help {
        vec![element!(HelpModal(visible: true, colors: colors.clone())).into()]
    } else if show_confirm {
        vec![element!(ConfirmModal(
            visible: true,
            title: "Kill Agent".to_owned(),
            message: confirm_msg.clone(),
            colors: colors.clone(),
        ))
        .into()]
    } else {
        vec![]
    };

    let root_rc = crate::theme::ResolvedColors::from_theme(Some(&colors));
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let render_rows = effective_render_rows(term_rows);
    let render_cols = effective_render_cols(term_cols);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: root_rc.bg,
            width: u32::from(render_cols),
            height: u32::from(render_rows),
        ) {
            #(vec![screen_el])
            #(modal_els)
        }
    }
}

fn main() {
    // Compute PTY dimensions from terminal size.
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (pty_rows, pty_cols, _, _) = compute_layout(term_cols, term_rows);

    // Build working dirs for each agent in mock data.
    let seed_state = AppState::new(generate_mock_data());
    let total_agents = seed_state.agent_count();

    let exe_dir = std::env::current_dir().unwrap_or_default();
    let work_dirs: Vec<String> = (1..=total_agents)
        .map(|i| {
            let dir = exe_dir.join(format!("tmp/working-{i}"));
            // Ensure dir exists.
            let _ = std::fs::create_dir_all(&dir);
            dir.to_string_lossy().to_string()
        })
        .collect();
    let work_dir_refs: Vec<&str> = work_dirs.iter().map(String::as_str).collect();

    let pty_mgr = Arc::new(PtyManager::spawn(&work_dir_refs, pty_rows, pty_cols));

    smol::block_on(async {
        let mut app = element!(App(
            pty_manager: Some(Arc::clone(&pty_mgr)),
        ));
        if is_fullscreen_enabled() {
            if let Err(e) = app.fullscreen().await {
                eprintln!("Error: {e}");
            }
        } else if let Err(e) = app.render_loop().await {
            eprintln!("Error: {e}");
        }
    });
}
