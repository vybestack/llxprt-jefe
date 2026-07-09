//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1
//! @requirement REQ-TECH-001

#![allow(clippy::print_stderr)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::significant_drop_tightening)]

mod app_init;
mod app_input;
mod app_shell;
mod pty_encoding;

use std::sync::Arc;

use iocraft::prelude::*;
use tracing::error;

use jefe::layout::{compute_pty_layout, is_fullscreen_enabled};
use jefe::runtime::TmuxRuntimeManager;
use jefe::theme::FileThemeManager;

/// Shared application context passed to the root component.
struct AppContext {
    persistence: jefe::persistence::FilePersistenceManager,
    theme_manager: FileThemeManager,
    runtime: TmuxRuntimeManager,
    /// @plan PLAN-20260329-ISSUES-MODE.P09
    gh_client: jefe::github::GhClient,
}

#[allow(clippy::print_stdout)]
fn handle_cli_version_flag() -> bool {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--version" | "-V"), None) => {
            let version = jefe::VERSION;
            println!("jefe {version}");
            true
        }
        _ => false,
    }
}

fn main() {
    if handle_cli_version_flag() {
        return;
    }

    // Initialize structured logging (no-op if JEFE_LOG_FILE is unset).
    jefe::logging::init();
    tracing::info!(version = jefe::VERSION, "jefe starting");
    tracing::debug!(
        log_file = ?jefe::logging::log_file_path(),
        "logging initialized"
    );

    // Get terminal size and derive PTY viewport size from dashboard geometry.
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (pty_rows, pty_cols, _, _) = compute_pty_layout(cols, rows);

    // Initialize managers.
    let persistence = jefe::persistence::FilePersistenceManager::new();
    let mut theme_manager = FileThemeManager::new();
    // Load custom themes from the default config dir's themes/ directory
    // (overridden via JEFE_CONFIG_DIR / JEFE_SETTINGS_PATH).
    let themes_dir = jefe::persistence::default_themes_dir();
    theme_manager.load_from_dir(&themes_dir);
    let runtime = TmuxRuntimeManager::new(pty_rows, pty_cols);

    let context = Arc::new(std::sync::Mutex::new(AppContext {
        persistence,
        theme_manager,
        runtime,
        gh_client: jefe::github::GhClient::new(),
    }));

    smol::block_on(async {
        let mut app = element!(app_shell::App(context: Some(context)));

        if is_fullscreen_enabled() {
            if let Err(e) = app.fullscreen().await {
                error!(error = %e, "fullscreen mode failed");
            }
        } else if let Err(e) = app.render_loop().await {
            error!(error = %e, "render loop failed");
        }
    });
}
