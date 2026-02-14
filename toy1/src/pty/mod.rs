//! PTY session management for embedded terminal views.
//!
//! Each agent gets a real PTY running a shell. The `PtyManager` spawns
//! and owns all sessions; UI code reads screen content via `screen_lines()`
//! and writes input via `write_input()`.

use alacritty_terminal::event::{Event as AlacrittyEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config as AlacrittyConfig, Term, TermMode};
use alacritty_terminal::vte::ansi;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Copy)]
struct TermDimensions {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone, Copy)]
struct NullListener;

impl EventListener for NullListener {
    fn send_event(&self, _event: AlacrittyEvent) {}
}

/// Default terminal colors used when a cell references logical colors
/// like `Foreground`/`Background` and the terminal has not overridden them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalColorDefaults {
    /// Default foreground color.
    pub fg: ansi::Rgb,
    /// Default background color.
    pub bg: ansi::Rgb,
    /// Bright/default emphasis color.
    pub bright: ansi::Rgb,
    /// Dim/default muted color.
    pub dim: ansi::Rgb,
    /// Selection foreground color.
    pub selection_fg: ansi::Rgb,
    /// Selection background color.
    pub selection_bg: ansi::Rgb,
}

impl TerminalColorDefaults {
    /// Green-screen defaults matching llxprt's Green Screen theme.
    pub const GREEN_SCREEN: Self = Self {
        fg: ansi::Rgb {
            r: 0x6a,
            g: 0x99,
            b: 0x55,
        },
        bg: ansi::Rgb { r: 0, g: 0, b: 0 },
        bright: ansi::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0x00,
        },
        dim: ansi::Rgb {
            r: 0x4a,
            g: 0x70,
            b: 0x35,
        },
        selection_fg: ansi::Rgb { r: 0, g: 0, b: 0 },
        selection_bg: ansi::Rgb {
            r: 0x6a,
            g: 0x99,
            b: 0x55,
        },
    };
}

/// Style information for one terminal cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalCellStyle {
    /// Foreground color.
    pub fg: iocraft::Color,
    /// Background color.
    pub bg: iocraft::Color,
    /// Bold weight.
    pub bold: bool,
    /// Underline decoration.
    pub underline: bool,
}

/// One renderable terminal cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalCell {
    /// Display character.
    pub ch: char,
    /// Cell style.
    pub style: TerminalCellStyle,
}

/// Full terminal viewport snapshot for one PTY session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSnapshot {
    /// Number of visible rows.
    pub rows: usize,
    /// Number of visible columns.
    pub cols: usize,
    /// Cells in row-major order (`cells[row][col]`).
    pub cells: Vec<Vec<TerminalCell>>,
}

impl TerminalSnapshot {
    fn blank(rows: usize, cols: usize, style: TerminalCellStyle) -> Self {
        let cell = TerminalCell { ch: ' ', style };
        let cells = vec![vec![cell; cols]; rows];
        Self { rows, cols, cells }
    }

    fn from_message(message: &str, style: TerminalCellStyle) -> Self {
        let text = if message.is_empty() { "(empty)" } else { message };
        let cols = text.chars().count().max(1);
        let mut snapshot = Self::blank(1, cols, style);
        for (i, ch) in text.chars().enumerate() {
            snapshot.cells[0][i].ch = ch;
        }
        snapshot
    }
}

/// One PTY session: a shell process with an Alacritty terminal model.
struct PtySession {
    /// PTY master handle, used for kernel-side resize.
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Write end — send keystrokes here.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Child-process killer handle.
    killer: Arc<Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>>,
    /// Alacritty terminal model.
    term: Arc<Mutex<Term<NullListener>>>,
    /// Whether the child process/reader loop appears alive.
    alive: Arc<AtomicBool>,
    /// Reader thread handle (kept alive so the thread isn't detached and lost).
    _reader: thread::JoinHandle<()>,
}

/// Manages multiple PTY sessions, one per agent.
/// Sessions that failed to spawn are stored as `None`.
pub struct PtyManager {
    sessions: Vec<Mutex<Option<PtySession>>>,
    /// Error messages for sessions that failed to start.
    errors: Vec<Mutex<Option<String>>>,
    /// Original working directory for each slot (used for relaunch).
    work_dirs: Vec<String>,
    /// Last requested PTY rows.
    rows: Arc<Mutex<u16>>,
    /// Last requested PTY columns.
    cols: Arc<Mutex<u16>>,
    /// Default colors used when a PTY cell references logical palette values.
    color_defaults: Arc<Mutex<TerminalColorDefaults>>,
}

/// Attempt to spawn a single shell PTY in the given working directory.
fn try_spawn_session(
    dir: &str,
    rows: u16,
    cols: u16,
) -> Result<PtySession, String> {
    let pty_system = native_pty_system();

    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pair = pty_system
        .openpty(size)
        .map_err(|e| format!("openpty: {e}"))?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let mut cmd = CommandBuilder::new(&shell);

    let dir_path = Path::new(dir);
    if dir_path.is_dir() {
        cmd.cwd(dir_path);
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn: {e}"))?;
    let killer: Box<dyn portable_pty::ChildKiller + Send + Sync> = child.clone_killer();
    drop(pair.slave);

    let writer: Box<dyn Write + Send> = pair
        .master
        .take_writer()
        .map_err(|e| format!("writer: {e}"))?;
    let mut reader: Box<dyn Read + Send> = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("reader: {e}"))?;
    let master: Box<dyn MasterPty + Send> = pair.master;

    let safe_rows = rows.max(2);
    let safe_cols = cols.max(2);

    let dims = TermDimensions {
        cols: usize::from(safe_cols),
        rows: usize::from(safe_rows),
    };
    let term = Arc::new(Mutex::new(Term::new(
        AlacrittyConfig::default(),
        &dims,
        NullListener,
    )));
    let parser: Arc<Mutex<ansi::Processor>> = Arc::new(Mutex::new(ansi::Processor::new()));

    let term_clone = Arc::clone(&term);
    let parser_clone = Arc::clone(&parser);
    let alive = Arc::new(AtomicBool::new(true));
    let alive_clone = Arc::clone(&alive);

    let reader_handle = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let (Ok(mut t), Ok(mut p)) = (term_clone.lock(), parser_clone.lock()) {
                        p.advance(&mut *t, &buf[..n]);
                    }
                }
            }
        }
        alive_clone.store(false, Ordering::Relaxed);
    });

    Ok(PtySession {
        master: Arc::new(Mutex::new(master)),
        writer: Arc::new(Mutex::new(writer)),
        killer: Arc::new(Mutex::new(killer)),
        term,
        alive,
        _reader: reader_handle,
    })
}

impl PtyManager {
    /// Spawn shell sessions, one per working directory.
    ///
    /// Individual failures are captured — the app still starts even if
    /// every PTY fails. Failed sessions show their error in the terminal pane.
    pub fn spawn(work_dirs: &[&str], rows: u16, cols: u16) -> Self {
        let mut sessions = Vec::with_capacity(work_dirs.len());
        let mut errors = Vec::with_capacity(work_dirs.len());

        for dir in work_dirs {
            match try_spawn_session(dir, rows, cols) {
                Ok(session) => {
                    sessions.push(Mutex::new(Some(session)));
                    errors.push(Mutex::new(None));
                }
                Err(e) => {
                    sessions.push(Mutex::new(None));
                    errors.push(Mutex::new(Some(format!("PTY failed for {dir}: {e}"))));
                }
            }
        }

        Self {
            sessions,
            errors,
            work_dirs: work_dirs.iter().map(|s| (*s).to_owned()).collect(),
            rows: Arc::new(Mutex::new(rows.max(2))),
            cols: Arc::new(Mutex::new(cols.max(2))),
            color_defaults: Arc::new(Mutex::new(TerminalColorDefaults::GREEN_SCREEN)),
        }
    }

    /// Number of session slots (includes failed ones).
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Update fallback/default terminal colors for new render snapshots.
    pub fn set_color_defaults(&self, defaults: TerminalColorDefaults) {
        if let Ok(mut guard) = self.color_defaults.lock() {
            *guard = defaults;
        }
    }

    /// Get the current fallback/default terminal colors.
    pub fn color_defaults(&self) -> TerminalColorDefaults {
        self.color_defaults
            .lock()
            .map_or(TerminalColorDefaults::GREEN_SCREEN, |guard| *guard)
    }

    /// Get a styled viewport snapshot for a session.
    pub fn terminal_snapshot(&self, idx: usize) -> TerminalSnapshot {
        let defaults = self.color_defaults();
        let base_style = TerminalCellStyle {
            fg: rgb_to_iocraft(defaults.fg),
            bg: rgb_to_iocraft(defaults.bg),
            bold: false,
            underline: false,
        };

        if let Some(err_slot) = self.errors.get(idx) {
            if let Ok(err_guard) = err_slot.lock() {
                if let Some(err) = err_guard.as_ref() {
                    return TerminalSnapshot::from_message(err, base_style);
                }
            }
        }

        let Some(slot) = self.sessions.get(idx) else {
            return TerminalSnapshot::from_message("(no session)", base_style);
        };

        let Ok(guard) = slot.lock() else {
            return TerminalSnapshot::from_message("(session lock error)", base_style);
        };

        let Some(session) = guard.as_ref() else {
            return TerminalSnapshot::from_message("(no session)", base_style);
        };

        let Ok(term) = session.term.lock() else {
            return TerminalSnapshot::from_message("(lock error)", base_style);
        };

        snapshot_from_term(&term, defaults)
    }

    /// Get plain-text viewport lines for a session.
    ///
    /// This is now derived from `terminal_snapshot` so style and text extraction
    /// stay consistent.
    pub fn screen_lines(&self, idx: usize) -> Vec<String> {
        let snapshot = self.terminal_snapshot(idx);
        snapshot
            .cells
            .into_iter()
            .map(|row| row.into_iter().map(|c| c.ch).collect())
            .collect()
    }

    /// Write raw bytes to a session's PTY (input forwarding).
    pub fn write_input(&self, idx: usize, data: &[u8]) {
        if let Some(slot) = self.sessions.get(idx) {
            if let Ok(guard) = slot.lock() {
                if let Some(session) = guard.as_ref() {
                    if let Ok(mut w) = session.writer.lock() {
                        let _ = w.write_all(data);
                        let _ = w.flush();
                    }
                }
            }
        }
    }

    /// Resize a specific PTY session.
    pub fn resize(&self, idx: usize, rows: u16, cols: u16) {
        let safe_rows = rows.max(2);
        let safe_cols = cols.max(2);

        if let Ok(mut r) = self.rows.lock() {
            *r = safe_rows;
        }
        if let Ok(mut c) = self.cols.lock() {
            *c = safe_cols;
        }

        if let Some(slot) = self.sessions.get(idx) {
            if let Ok(guard) = slot.lock() {
                if let Some(session) = guard.as_ref() {
                    if let Ok(master) = session.master.lock() {
                        let _ = master.resize(PtySize {
                            rows: safe_rows,
                            cols: safe_cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }

                    if let Ok(mut term) = session.term.lock() {
                        let dims = TermDimensions {
                            cols: usize::from(safe_cols),
                            rows: usize::from(safe_rows),
                        };
                        term.resize(dims);
                    }
                }
            }
        }
    }


    /// Whether the child app has enabled terminal mouse reporting for this session.
    pub fn mouse_reporting_active(&self, idx: usize) -> bool {
        let Some(slot) = self.sessions.get(idx) else {
            return false;
        };

        let Ok(guard) = slot.lock() else {
            return false;
        };

        let Some(session) = guard.as_ref() else {
            return false;
        };

        let Ok(term) = session.term.lock() else {
            return false;
        };

        let mode = term.mode();
        mode.contains(TermMode::MOUSE_MODE)
            || mode.contains(TermMode::SGR_MOUSE)
            || mode.contains(TermMode::UTF8_MOUSE)
    }

    /// Resize all sessions to the same dimensions.
    pub fn resize_all(&self, rows: u16, cols: u16) {
        for idx in 0..self.sessions.len() {
            self.resize(idx, rows, cols);
        }
    }

    /// Returns whether the PTY session appears alive.
    pub fn is_alive(&self, idx: usize) -> bool {
        self.sessions
            .get(idx)
            .and_then(|slot| slot.lock().ok())
            .and_then(|guard| guard.as_ref().map(|session| session.alive.load(Ordering::Relaxed)))
            .unwrap_or(false)
    }

    /// Best-effort kill/close for a PTY session.
    pub fn kill_session(&self, idx: usize) {
        if let Some(slot) = self.sessions.get(idx) {
            if let Ok(guard) = slot.lock() {
                if let Some(session) = guard.as_ref() {
                    if let Ok(mut killer) = session.killer.lock() {
                        let _ = killer.kill();
                    } else if let Ok(mut writer) = session.writer.lock() {
                        let _ = writer.write_all(&[0x03]);
                        let _ = writer.flush();
                    }
                    session.alive.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    /// Relaunch a PTY slot from its original working directory.
    pub fn relaunch_session(&self, idx: usize) -> Result<(), String> {
        let dir = self
            .work_dirs
            .get(idx)
            .cloned()
            .ok_or_else(|| format!("invalid PTY index: {idx}"))?;

        let rows = self.rows.lock().map_or(24, |r| *r).max(2);
        let cols = self.cols.lock().map_or(80, |c| *c).max(2);

        let session = try_spawn_session(&dir, rows, cols)?;

        let Some(slot) = self.sessions.get(idx) else {
            return Err(format!("invalid PTY slot: {idx}"));
        };
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(session);
        }

        if let Some(err_slot) = self.errors.get(idx) {
            if let Ok(mut err_guard) = err_slot.lock() {
                *err_guard = None;
            }
        }

        Ok(())
    }
}

fn rgb_to_iocraft(rgb: ansi::Rgb) -> iocraft::Color {
    iocraft::Color::Rgb {
        r: rgb.r,
        g: rgb.g,
        b: rgb.b,
    }
}

fn themed_ansi_color(index: u8, defaults: TerminalColorDefaults) -> ansi::Rgb {
    match index {
        0 => defaults.bg,
        1 => defaults.fg,
        2 => defaults.fg,
        3 => defaults.fg,
        4 => defaults.fg,
        5 => defaults.fg,
        6 => defaults.fg,
        7 => defaults.fg,
        8 => defaults.dim,
        9 => defaults.fg,
        10 => defaults.bright,
        11 => defaults.fg,
        12 => defaults.fg,
        13 => defaults.fg,
        14 => defaults.fg,
        15 => defaults.fg,
        n @ 16..=231 => {
            let idx = n - 16;
            let r = idx / 36;
            let g = (idx % 36) / 6;
            let b = idx % 6;
            const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            ansi::Rgb {
                r: STEPS[usize::from(r)],
                g: STEPS[usize::from(g)],
                b: STEPS[usize::from(b)],
            }
        }
        n @ 232..=255 => {
            let v = 8 + (n - 232) * 10;
            ansi::Rgb { r: v, g: v, b: v }
        }
    }
}

fn resolve_named_color(
    named: ansi::NamedColor,
    term_colors: &alacritty_terminal::term::color::Colors,
    defaults: TerminalColorDefaults,
) -> ansi::Rgb {
    term_colors[named].unwrap_or_else(|| match named {
        ansi::NamedColor::Black => themed_ansi_color(0, defaults),
        ansi::NamedColor::Red => themed_ansi_color(1, defaults),
        ansi::NamedColor::Green => themed_ansi_color(2, defaults),
        ansi::NamedColor::Yellow => themed_ansi_color(3, defaults),
        ansi::NamedColor::Blue => themed_ansi_color(4, defaults),
        ansi::NamedColor::Magenta => themed_ansi_color(5, defaults),
        ansi::NamedColor::Cyan => themed_ansi_color(6, defaults),
        ansi::NamedColor::White => themed_ansi_color(7, defaults),
        ansi::NamedColor::BrightBlack => themed_ansi_color(8, defaults),
        ansi::NamedColor::BrightRed => themed_ansi_color(9, defaults),
        ansi::NamedColor::BrightGreen => themed_ansi_color(10, defaults),
        ansi::NamedColor::BrightYellow => themed_ansi_color(11, defaults),
        ansi::NamedColor::BrightBlue => themed_ansi_color(12, defaults),
        ansi::NamedColor::BrightMagenta => themed_ansi_color(13, defaults),
        ansi::NamedColor::BrightCyan => themed_ansi_color(14, defaults),
        ansi::NamedColor::BrightWhite => themed_ansi_color(15, defaults),
        ansi::NamedColor::Foreground => defaults.fg,
        ansi::NamedColor::Background => defaults.bg,
        ansi::NamedColor::Cursor => defaults.fg,
        ansi::NamedColor::DimBlack => defaults.dim,
        ansi::NamedColor::DimRed => defaults.dim,
        ansi::NamedColor::DimGreen => defaults.dim,
        ansi::NamedColor::DimYellow => defaults.dim,
        ansi::NamedColor::DimBlue => defaults.dim,
        ansi::NamedColor::DimMagenta => defaults.dim,
        ansi::NamedColor::DimCyan => defaults.dim,
        ansi::NamedColor::DimWhite => defaults.dim,
        ansi::NamedColor::BrightForeground => defaults.bright,
        ansi::NamedColor::DimForeground => defaults.dim,
    })
}

fn resolve_color(
    color: ansi::Color,
    term_colors: &alacritty_terminal::term::color::Colors,
    defaults: TerminalColorDefaults,
) -> ansi::Rgb {
    match color {
        ansi::Color::Spec(rgb) => rgb,
        ansi::Color::Indexed(idx) => term_colors[usize::from(idx)]
            .unwrap_or_else(|| themed_ansi_color(idx, defaults)),
        ansi::Color::Named(named) => resolve_named_color(named, term_colors, defaults),
    }
}

fn snapshot_from_term(term: &Term<NullListener>, defaults: TerminalColorDefaults) -> TerminalSnapshot {
    let rows = term.screen_lines();
    let cols = term.columns();

    let base_style = TerminalCellStyle {
        fg: rgb_to_iocraft(defaults.fg),
        bg: rgb_to_iocraft(defaults.bg),
        bold: false,
        underline: false,
    };
    let mut snapshot = TerminalSnapshot::blank(rows, cols, base_style);

    let renderable = term.renderable_content();
    let selection = renderable.selection;
    let cursor = renderable.cursor;
    let term_colors = renderable.colors;

    for indexed in renderable.display_iter {
        let line_i32 = indexed.point.line.0;
        if line_i32 < 0 {
            continue;
        }

        let Ok(row) = usize::try_from(line_i32) else {
            continue;
        };
        if row >= rows {
            continue;
        }

        let col = indexed.point.column.0;
        if col >= cols {
            continue;
        }

        if indexed
            .cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }

        let mut fg = resolve_color(indexed.cell.fg, term_colors, defaults);
        let mut bg = resolve_color(indexed.cell.bg, term_colors, defaults);
        let bold = indexed.cell.flags.contains(Flags::BOLD)
            || indexed.cell.flags.contains(Flags::DIM_BOLD);
        let underline = indexed.cell.flags.intersects(Flags::ALL_UNDERLINES);

        if indexed.cell.flags.contains(Flags::DIM) || indexed.cell.flags.contains(Flags::DIM_BOLD) {
            fg = defaults.dim;
        }

        if indexed.cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }

        let in_selection = selection
            .map(|range| range.contains_cell(&indexed, cursor.point, cursor.shape))
            .unwrap_or(false);
        if in_selection {
            fg = defaults.selection_fg;
            bg = defaults.selection_bg;
        }

        let is_cursor_cell = cursor.shape != ansi::CursorShape::Hidden && indexed.point == cursor.point;
        if is_cursor_cell {
            std::mem::swap(&mut fg, &mut bg);
        }

        let ch = if indexed.cell.flags.contains(Flags::HIDDEN) {
            ' '
        } else {
            let c = indexed.cell.c;
            if c == '\0' { ' ' } else { c }
        };

        snapshot.cells[row][col] = TerminalCell {
            ch,
            style: TerminalCellStyle {
                fg: rgb_to_iocraft(fg),
                bg: rgb_to_iocraft(bg),
                bold,
                underline,
            },
        };
    }

    snapshot
}


/// Convert an iocraft `KeyEvent` into bytes suitable for writing to a PTY.
///
/// Returns `None` for keys we don't know how to encode.
pub fn key_event_to_bytes(key: &iocraft::KeyEvent) -> Option<Vec<u8>> {
    use iocraft::{KeyCode, KeyModifiers};

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let mut out = match key.code {
        KeyCode::Char(c) if ctrl => {
            // Ctrl+letter → ASCII control character (a=1, b=2, ..., z=26)
            let byte = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
            vec![byte]
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        _ => return None,
    };

    // Alt-modified keys are commonly encoded as ESC prefix.
    if alt {
        let mut prefixed = Vec::with_capacity(out.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend_from_slice(&out);
        out = prefixed;
    }

    Some(out)
}

/// Convert a fullscreen mouse event into xterm SGR mouse reporting bytes.
///
/// Generated sequence: ESC [ < Cb ; Cx ; Cy (M|m)
/// where Cx/Cy are 1-based positions.
pub fn mouse_event_to_bytes(event: &iocraft::FullscreenMouseEvent) -> Option<Vec<u8>> {
    use iocraft::MouseEventKind;

    let (cb, release) = match event.kind {
        // Forward only left-button selection gestures to avoid noisy middle/right
        // button sequences from some terminals/mice (e.g. wheel-click artifacts)
        // interfering with llxprt's selection state machine.
        MouseEventKind::Down(button) => {
            let code = match button {
                crossterm::event::MouseButton::Left => 0,
                _ => return None,
            };
            (code, false)
        }
        MouseEventKind::Up(button) => {
            // LLxprt's in-app parser treats release as button-specific and expects
            // left-release as ESC [ < 0 ; x ; y m.
            let code = match button {
                crossterm::event::MouseButton::Left => 0,
                _ => return None,
            };
            (code, true)
        },
        MouseEventKind::Drag(button) => {
            let base = match button {
                crossterm::event::MouseButton::Left => 0,
                _ => return None,
            };
            // Xterm SGR drag uses bit 5 (32) plus base button code.
            (base + 32, false)
        }
        // Do not synthesize pure mouse-move events. In llxprt, selection updates are
        // driven by drag events while a button is pressed; forwarding passive move
        // events can inject noisy cursor movement into the child UI.
        MouseEventKind::Moved => return None,
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
    };

    let mut cb_with_mods = cb;
    if event.modifiers.contains(iocraft::KeyModifiers::SHIFT) {
        cb_with_mods += 4;
    }
    if event.modifiers.contains(iocraft::KeyModifiers::ALT) {
        cb_with_mods += 8;
    }
    if event.modifiers.contains(iocraft::KeyModifiers::CONTROL) {
        cb_with_mods += 16;
    }

    let cx = event.column.saturating_add(1);
    let cy = event.row.saturating_add(1);
    let suffix = if release { 'm' } else { 'M' };
    let seq = format!("\x1b[<{};{};{}{}", cb_with_mods, cx, cy, suffix);
    Some(seq.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::{KeyCode, KeyEventKind, KeyModifiers};

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> iocraft::KeyEvent {
        let mut ke = iocraft::KeyEvent::new(KeyEventKind::Press, code);
        ke.modifiers = modifiers;
        ke
    }

    #[test]
    fn test_char_to_bytes() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(bytes, Some(vec![b'a']));
    }

    #[test]
    fn test_ctrl_c_to_bytes() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(bytes, Some(vec![3])); // ETX
    }

    #[test]
    fn test_enter_to_bytes() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(bytes, Some(vec![b'\r']));
    }

    #[test]
    fn test_arrow_up_to_bytes() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(bytes, Some(b"\x1b[A".to_vec()));
    }

    #[test]
    fn test_backspace_to_bytes() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(bytes, Some(vec![0x7f]));
    }

    #[test]
    fn test_esc_to_bytes() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(bytes, Some(vec![0x1b]));
    }

    #[test]
    fn test_unicode_char() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::Char('é'), KeyModifiers::NONE));
        assert_eq!(bytes, Some("é".as_bytes().to_vec()));
    }

    #[test]
    fn test_unknown_key() {
        let bytes = key_event_to_bytes(&make_key(KeyCode::F(1), KeyModifiers::NONE));
        assert_eq!(bytes, None);
    }
}
