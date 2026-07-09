//! Viewer attachment and PTY management.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 07-14

use std::io::{Read, Write};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::{Dimensions, Indexed};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{RenderableCursor, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Processor, StdSyncHandler};
use portable_pty::{
    Child as PtyChild, CommandBuilder, MasterPty, PtyPair, PtySize, native_pty_system,
};
use tracing::{debug, warn};

use super::errors::RuntimeError;
use super::session::{TerminalCell, TerminalCellStyle, TerminalSnapshot};

/// Simple dimensions struct for terminal sizing.
struct TermDimensions {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermDimensions {
    fn columns(&self) -> usize {
        self.cols
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn total_lines(&self) -> usize {
        self.rows
    }
}

/// Runtime event listener for alacritty_terminal.
///
/// Handles OSC52 clipboard-store events so llxprt copy propagates to the host
/// clipboard when running inside jefe's embedded PTY.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeListener;

fn copy_to_system_clipboard(text: &str) {
    if text.is_empty() {
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let mut child = match Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                warn!(%error, "failed to spawn pbcopy for OSC52 clipboard store");
                return;
            }
        };

        if let Some(stdin) = child.stdin.as_mut()
            && let Err(error) = stdin.write_all(text.as_bytes())
        {
            warn!(%error, "failed to write clipboard payload to pbcopy");
        }

        if let Err(error) = child.wait() {
            warn!(%error, "failed waiting for pbcopy to complete");
        }
    }

    #[cfg(target_os = "linux")]
    {
        for (cmd, args) in [
            ("xclip", ["-selection", "clipboard"].as_slice()),
            ("xsel", ["--clipboard", "--input"].as_slice()),
        ] {
            let Ok(mut child) = Command::new(cmd)
                .args(args)
                .stdin(std::process::Stdio::piped())
                .spawn()
            else {
                continue;
            };

            if let Some(stdin) = child.stdin.as_mut()
                && stdin.write_all(text.as_bytes()).is_err()
            {
                continue;
            }

            if child.wait().is_ok_and(|status| status.success()) {
                return;
            }
        }

        warn!("failed to store OSC52 clipboard data: xclip/xsel unavailable or failed");
    }
}

impl EventListener for RuntimeListener {
    fn send_event(&self, event: TermEvent) {
        if let TermEvent::ClipboardStore(_, text) = event {
            debug!(len = text.len(), "received OSC52 ClipboardStore event");
            copy_to_system_clipboard(&text);
        }
    }
}

/// An attached viewer representing a PTY connected to a tmux session.
pub struct AttachedViewer {
    /// PTY master handle for resize.
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Write end for sending input.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Terminal state model.
    term: Arc<Mutex<Term<RuntimeListener>>>,
    /// Liveness flag.
    alive: Arc<AtomicBool>,
    /// Dirty flag set by the reader thread on every successful PTY read.
    ///
    /// The render loop polls this flag to decide whether to re-render. Using
    /// `Relaxed` ordering is safe because the flag is only a hint — the actual
    /// terminal state is protected by the `term` mutex.
    dirty: Arc<AtomicBool>,
    /// Child process handle for deterministic teardown.
    child: Arc<Mutex<Box<dyn PtyChild + Send + Sync>>>,
    /// Reader thread handle.
    _reader_thread: JoinHandle<()>,
}

fn rgb_to_iocraft(rgb: ansi::Rgb) -> iocraft::Color {
    iocraft::Color::Rgb {
        r: rgb.r,
        g: rgb.g,
        b: rgb.b,
    }
}

const ANSI_COLOR_CUBE_STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
const ANSI_BASE_COLORS: [ansi::Rgb; 16] = [
    ansi::Rgb { r: 0, g: 0, b: 0 },
    ansi::Rgb {
        r: 0xcd,
        g: 0,
        b: 0,
    },
    ansi::Rgb {
        r: 0,
        g: 0xcd,
        b: 0,
    },
    ansi::Rgb {
        r: 0xcd,
        g: 0xcd,
        b: 0,
    },
    ansi::Rgb {
        r: 0,
        g: 0,
        b: 0xee,
    },
    ansi::Rgb {
        r: 0xcd,
        g: 0,
        b: 0xcd,
    },
    ansi::Rgb {
        r: 0,
        g: 0xcd,
        b: 0xcd,
    },
    ansi::Rgb {
        r: 0xe5,
        g: 0xe5,
        b: 0xe5,
    },
    ansi::Rgb {
        r: 0x7f,
        g: 0x7f,
        b: 0x7f,
    },
    ansi::Rgb {
        r: 0xff,
        g: 0,
        b: 0,
    },
    ansi::Rgb {
        r: 0,
        g: 0xff,
        b: 0,
    },
    ansi::Rgb {
        r: 0xff,
        g: 0xff,
        b: 0,
    },
    ansi::Rgb {
        r: 0x5c,
        g: 0x5c,
        b: 0xff,
    },
    ansi::Rgb {
        r: 0xff,
        g: 0,
        b: 0xff,
    },
    ansi::Rgb {
        r: 0,
        g: 0xff,
        b: 0xff,
    },
    ansi::Rgb {
        r: 0xff,
        g: 0xff,
        b: 0xff,
    },
];

fn fallback_ansi_color(index: u8) -> ansi::Rgb {
    match index {
        0..=15 => ANSI_BASE_COLORS[usize::from(index)],
        n @ 16..=231 => ansi_color_cube(n),
        n @ 232..=255 => ansi_grayscale(n),
    }
}

fn ansi_color_cube(index: u8) -> ansi::Rgb {
    let idx = index - 16;
    let r = idx / 36;
    let g = (idx % 36) / 6;
    let b = idx % 6;
    ansi::Rgb {
        r: ANSI_COLOR_CUBE_STEPS[usize::from(r)],
        g: ANSI_COLOR_CUBE_STEPS[usize::from(g)],
        b: ANSI_COLOR_CUBE_STEPS[usize::from(b)],
    }
}

fn ansi_grayscale(index: u8) -> ansi::Rgb {
    let value = 8 + (index - 232) * 10;
    ansi::Rgb {
        r: value,
        g: value,
        b: value,
    }
}

fn resolve_named_color(
    named: ansi::NamedColor,
    term_colors: &alacritty_terminal::term::color::Colors,
) -> ansi::Rgb {
    term_colors[named].unwrap_or_else(|| match named {
        ansi::NamedColor::Black | ansi::NamedColor::Background => fallback_ansi_color(0),
        ansi::NamedColor::Red => fallback_ansi_color(1),
        ansi::NamedColor::Green => fallback_ansi_color(2),
        ansi::NamedColor::Yellow => fallback_ansi_color(3),
        ansi::NamedColor::Blue => fallback_ansi_color(4),
        ansi::NamedColor::Magenta => fallback_ansi_color(5),
        ansi::NamedColor::Cyan => fallback_ansi_color(6),
        ansi::NamedColor::White | ansi::NamedColor::Foreground | ansi::NamedColor::Cursor => {
            fallback_ansi_color(7)
        }
        ansi::NamedColor::BrightBlack
        | ansi::NamedColor::DimBlack
        | ansi::NamedColor::DimRed
        | ansi::NamedColor::DimGreen
        | ansi::NamedColor::DimYellow
        | ansi::NamedColor::DimBlue
        | ansi::NamedColor::DimMagenta
        | ansi::NamedColor::DimCyan
        | ansi::NamedColor::DimWhite
        | ansi::NamedColor::DimForeground => fallback_ansi_color(8),
        ansi::NamedColor::BrightRed => fallback_ansi_color(9),
        ansi::NamedColor::BrightGreen => fallback_ansi_color(10),
        ansi::NamedColor::BrightYellow => fallback_ansi_color(11),
        ansi::NamedColor::BrightBlue => fallback_ansi_color(12),
        ansi::NamedColor::BrightMagenta => fallback_ansi_color(13),
        ansi::NamedColor::BrightCyan => fallback_ansi_color(14),
        ansi::NamedColor::BrightWhite | ansi::NamedColor::BrightForeground => {
            fallback_ansi_color(15)
        }
    })
}

fn resolve_color(
    color: ansi::Color,
    term_colors: &alacritty_terminal::term::color::Colors,
) -> ansi::Rgb {
    match color {
        ansi::Color::Spec(rgb) => rgb,
        ansi::Color::Indexed(idx) => {
            term_colors[usize::from(idx)].unwrap_or_else(|| fallback_ansi_color(idx))
        }
        ansi::Color::Named(named) => resolve_named_color(named, term_colors),
    }
}

fn dim_rgb(rgb: ansi::Rgb) -> ansi::Rgb {
    ansi::Rgb {
        r: rgb.r / 2,
        g: rgb.g / 2,
        b: rgb.b / 2,
    }
}

fn base_terminal_style() -> TerminalCellStyle {
    TerminalCellStyle {
        fg: rgb_to_iocraft(fallback_ansi_color(7)),
        bg: rgb_to_iocraft(fallback_ansi_color(0)),
        bold: false,
        underline: false,
    }
}

fn snapshot_position(indexed: &Indexed<&Cell>, rows: usize, cols: usize) -> Option<(usize, usize)> {
    let line = indexed.point.line.0;
    if line < 0 {
        return None;
    }

    let row = usize::try_from(line).ok()?;
    let col = indexed.point.column.0;
    (row < rows && col < cols).then_some((row, col))
}

fn snapshot_cell_style(
    indexed: &Indexed<&Cell>,
    selection: Option<SelectionRange>,
    cursor: RenderableCursor,
    term_colors: &Colors,
) -> TerminalCellStyle {
    let mut fg = resolve_color(indexed.cell.fg, term_colors);
    let mut bg = resolve_color(indexed.cell.bg, term_colors);
    if indexed.cell.flags.intersects(Flags::DIM | Flags::DIM_BOLD) {
        fg = dim_rgb(fg);
    }
    if indexed.cell.flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if selection.is_some_and(|range| range.contains_cell(indexed, cursor.point, cursor.shape)) {
        fg = fallback_ansi_color(0);
        bg = fallback_ansi_color(7);
    }
    if cursor.shape != ansi::CursorShape::Hidden && indexed.point == cursor.point {
        std::mem::swap(&mut fg, &mut bg);
    }

    TerminalCellStyle {
        fg: rgb_to_iocraft(fg),
        bg: rgb_to_iocraft(bg),
        bold: indexed.cell.flags.intersects(Flags::BOLD | Flags::DIM_BOLD),
        underline: indexed.cell.flags.intersects(Flags::ALL_UNDERLINES),
    }
}

fn snapshot_cell(indexed: &Indexed<&Cell>, style: TerminalCellStyle) -> TerminalCell {
    let ch = if indexed.cell.flags.contains(Flags::HIDDEN) || indexed.cell.c == '\0' {
        ' '
    } else {
        indexed.cell.c
    };
    TerminalCell { ch, style }
}

fn snapshot_from_term(term: &Term<RuntimeListener>) -> TerminalSnapshot {
    let rows = term.screen_lines();
    let cols = term.columns();
    let mut snapshot = TerminalSnapshot::blank(rows, cols, base_terminal_style());
    let renderable = term.renderable_content();

    for indexed in renderable.display_iter {
        let Some((row, col)) = snapshot_position(&indexed, rows, cols) else {
            continue;
        };
        if indexed
            .cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }

        let style = snapshot_cell_style(
            &indexed,
            renderable.selection,
            renderable.cursor,
            renderable.colors,
        );
        snapshot.cells[row][col] = snapshot_cell(&indexed, style);
    }

    snapshot
}

fn attach_command(session_name: &str, ssh_command: Option<&str>) -> CommandBuilder {
    let mut cmd = if let Some(ssh_command) = ssh_command {
        let mut cmd = CommandBuilder::new("sh");
        cmd.arg("-lc");
        cmd.arg(ssh_command);
        cmd
    } else {
        // Reuse `commands::tmux_base_args()` which encodes exactly
        // `-f /dev/null -S <jefe-socket>`, reducing drift versus the rest of
        // jefe's tmux command construction. The remote (SSH) branch is
        // intentionally left without `-S` because remote tmux runs on the
        // remote host under its own (possibly shared) socket.
        let mut cmd = CommandBuilder::new("tmux");
        for arg in super::commands::tmux_base_args() {
            cmd.arg(arg);
        }
        cmd.arg("attach-session");
        cmd.arg("-t");
        cmd.arg(session_name);
        cmd
    };
    cmd.env("TERM", "xterm-256color");
    cmd
}

fn open_pty(rows: u16, cols: u16) -> Result<PtyPair, RuntimeError> {
    native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| RuntimeError::SpawnFailed(format!("openpty: {e}")))
}

impl AttachedViewer {
    /// Spawn a new attached viewer for a tmux session.
    ///
    /// @pseudocode component-002 lines 10-13
    pub fn spawn(session_name: &str, rows: u16, cols: u16) -> Result<Self, RuntimeError> {
        Self::spawn_command(session_name, rows, cols, None)
    }

    pub fn spawn_remote(
        session_name: &str,
        rows: u16,
        cols: u16,
        ssh_command: &str,
    ) -> Result<Self, RuntimeError> {
        Self::spawn_command(session_name, rows, cols, Some(ssh_command))
    }

    fn spawn_command(
        session_name: &str,
        rows: u16,
        cols: u16,
        ssh_command: Option<&str>,
    ) -> Result<Self, RuntimeError> {
        debug!(session_name = %session_name, rows, cols, remote = ssh_command.is_some(), "AttachedViewer::spawn start");

        let pty_pair = open_pty(rows, cols)?;
        let cmd = attach_command(session_name, ssh_command);

        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::SpawnFailed(format!("spawn tmux attach: {e}")))?;
        debug!(session_name = %session_name, "AttachedViewer::spawn tmux attach child spawned");
        let child = Arc::new(Mutex::new(child));

        let reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| RuntimeError::SpawnFailed(format!("clone reader: {e}")))?;

        let writer = pty_pair
            .master
            .take_writer()
            .map_err(|e| RuntimeError::SpawnFailed(format!("take writer: {e}")))?;

        let master = Arc::new(Mutex::new(pty_pair.master));
        let writer = Arc::new(Mutex::new(writer));

        // Create terminal model
        let config = TermConfig::default();
        let term_size = TermDimensions {
            cols: cols as usize,
            rows: rows as usize,
        };
        let term = Term::new(config, &term_size, RuntimeListener);
        let term = Arc::new(Mutex::new(term));

        let alive = Arc::new(AtomicBool::new(true));
        let dirty = Arc::new(AtomicBool::new(false));

        // Spawn reader thread
        let term_clone = Arc::clone(&term);
        let alive_clone = Arc::clone(&alive);
        let dirty_clone = Arc::clone(&dirty);
        let reader_thread = thread::spawn(move || {
            reader_loop(reader, term_clone, alive_clone, dirty_clone);
        });

        debug!(session_name = %session_name, "AttachedViewer::spawn ready");
        Ok(Self {
            master,
            writer,
            term,
            alive,
            dirty,
            child,
            _reader_thread: reader_thread,
        })
    }

    /// Check if the viewer is still alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Atomically read and clear the dirty flag.
    ///
    /// Returns `true` when new PTY data has arrived since the last call,
    /// `false` otherwise. The flag is cleared regardless of the return value.
    #[must_use]
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// Write input bytes to the PTY.
    ///
    /// @pseudocode component-002 lines 18-20
    pub fn write_input(&self, bytes: &[u8]) -> Result<(), RuntimeError> {
        if !self.is_alive() {
            return Err(RuntimeError::WriteFailed("viewer not alive".into()));
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| RuntimeError::WriteFailed("lock poisoned".into()))?;

        writer
            .write_all(bytes)
            .map_err(|e| RuntimeError::WriteFailed(e.to_string()))?;

        writer
            .flush()
            .map_err(|e| RuntimeError::WriteFailed(format!("flush: {e}")))?;
        drop(writer);

        Ok(())
    }

    /// Resize the terminal.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), RuntimeError> {
        let master = self
            .master
            .lock()
            .map_err(|_| RuntimeError::ResizeFailed("lock poisoned".into()))?;

        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::ResizeFailed(e.to_string()))?;

        drop(master);

        // Also update the terminal model
        if let Ok(mut term) = self.term.lock() {
            let new_size = TermDimensions {
                cols: cols as usize,
                rows: rows as usize,
            };
            term.resize(new_size);
        }

        Ok(())
    }

    /// Get a snapshot of the terminal state.
    pub fn snapshot(&self) -> Option<TerminalSnapshot> {
        let term = self.term.lock().ok()?;
        Some(snapshot_from_term(&term))
    }

    /// Whether the attached application has terminal mouse reporting enabled.
    pub fn mouse_reporting_active(&self) -> bool {
        let Ok(term) = self.term.lock() else {
            return false;
        };

        let mode = term.mode();
        mode.contains(TermMode::MOUSE_MODE)
            || mode.contains(TermMode::SGR_MOUSE)
            || mode.contains(TermMode::UTF8_MOUSE)
    }

    /// Whether the attached application has bracketed paste enabled.
    pub fn bracketed_paste_active(&self) -> bool {
        let Ok(term) = self.term.lock() else {
            return false;
        };

        term.mode().contains(TermMode::BRACKETED_PASTE)
    }
}

fn terminate_child_with_timeout(child: &mut dyn PtyChild, timeout: Duration) {
    match child.try_wait() {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(error) => {
            debug!(%error, "could not poll tmux child status before teardown");
        }
    }

    if let Err(error) = child.kill() {
        debug!(%error, "failed to signal tmux child during viewer teardown");
    }

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    warn!("timed out waiting for tmux child to exit during viewer teardown");
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                warn!(%error, "could not poll tmux child status during viewer teardown");
                break;
            }
        }
    }
}

impl Drop for AttachedViewer {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);

        let Ok(mut child) = self.child.lock() else {
            warn!("child lock poisoned during viewer teardown");
            return;
        };

        terminate_child_with_timeout(&mut **child, Duration::from_millis(300));
    }
}

/// Reader loop that feeds PTY output into the terminal model.
///
/// On every successful read, the `dirty` flag is set so the render loop knows
/// new terminal data is available and should trigger a re-render.
fn reader_loop(
    mut reader: Box<dyn Read + Send>,
    term: Arc<Mutex<Term<RuntimeListener>>>,
    alive: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
) {
    let mut buf = [0u8; 4096];
    let mut parser: Processor<StdSyncHandler> = Processor::new();

    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                // EOF - viewer died
                alive.store(false, Ordering::Relaxed);
                break;
            }
            Ok(n) => {
                process_pty_read(&buf[..n], &mut parser, &term, &dirty);
            }
            Err(_) => {
                // Reader error - mark viewer as dead
                alive.store(false, Ordering::Relaxed);
                break;
            }
        }
    }
}

/// Process a batch of bytes from a PTY read: advance the terminal parser and
/// mark the viewer dirty so the render loop knows new data arrived.
///
/// Extracted from `reader_loop` so the "data arrives → dirty is set" behavior
/// can be unit-tested without a live PTY.
fn process_pty_read(
    bytes: &[u8],
    parser: &mut Processor<StdSyncHandler>,
    term: &Mutex<Term<RuntimeListener>>,
    dirty: &AtomicBool,
) {
    if let Ok(mut term) = term.lock() {
        for byte in bytes {
            parser.advance(&mut *term, *byte);
        }
    }
    dirty.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal terminal model for testing `process_pty_read`.
    fn test_term() -> Arc<Mutex<Term<RuntimeListener>>> {
        let size = TermDimensions { cols: 80, rows: 24 };
        Arc::new(Mutex::new(Term::new(
            TermConfig::default(),
            &size,
            RuntimeListener,
        )))
    }

    /// Processing a batch of PTY bytes must set the dirty flag — this is the
    /// core wiring between the reader thread and the event-driven render loop.
    #[test]
    fn process_pty_read_marks_viewer_dirty() {
        let term = test_term();
        let dirty = Arc::new(AtomicBool::new(false));
        let mut parser: Processor<StdSyncHandler> = Processor::new();

        assert!(
            !dirty.load(Ordering::Relaxed),
            "dirty should be false before any data arrives"
        );

        process_pty_read(b"hello world", &mut parser, &term, &dirty);

        assert!(
            dirty.load(Ordering::Relaxed),
            "dirty must be set after PTY data arrives"
        );

        // take_dirty() pattern: swap clears and returns the previous value.
        assert!(
            dirty.swap(false, Ordering::Relaxed),
            "take_dirty must return true after data arrived"
        );
        assert!(
            !dirty.load(Ordering::Relaxed),
            "take_dirty must clear the flag"
        );

        // A second take_dirty() returns false (no new data since last clear).
        assert!(
            !dirty.swap(false, Ordering::Relaxed),
            "take_dirty must return false when no new data"
        );
    }

    /// Processing a PTY batch advances the terminal parser model (not just
    /// the dirty flag), proving the wiring feeds real bytes into the `Term`.
    #[test]
    fn process_pty_read_advances_terminal_model() {
        let term = test_term();
        let dirty = Arc::new(AtomicBool::new(false));
        let mut parser: Processor<StdSyncHandler> = Processor::new();

        // A blank terminal has no content in the first cell.
        {
            let Ok(guard) = term.lock() else {
                panic!("term lock should succeed");
            };
            let snapshot = snapshot_from_term(&guard);
            assert_eq!(
                snapshot.cells[0][0].ch, ' ',
                "terminal should be blank before processing"
            );
        }

        process_pty_read(b"X", &mut parser, &term, &dirty);

        let Ok(guard) = term.lock() else {
            panic!("term lock should succeed");
        };
        let snapshot = snapshot_from_term(&guard);
        assert!(
            snapshot
                .cells
                .iter()
                .any(|row| row.iter().any(|c| c.ch == 'X')),
            "terminal model should contain processed data after read"
        );
    }
}
