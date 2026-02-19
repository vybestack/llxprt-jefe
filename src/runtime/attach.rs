//! Viewer attachment and PTY management.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 07-14

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Processor, StdSyncHandler};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

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

/// Null event listener for alacritty_terminal.
#[derive(Clone, Copy, Debug)]
pub struct NullListener;

impl EventListener for NullListener {
    fn send_event(&self, _event: alacritty_terminal::event::Event) {}
}

/// An attached viewer representing a PTY connected to a tmux session.
pub struct AttachedViewer {
    /// PTY master handle for resize.
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Write end for sending input.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Terminal state model.
    term: Arc<Mutex<Term<NullListener>>>,
    /// Liveness flag.
    alive: Arc<AtomicBool>,
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

fn fallback_ansi_color(index: u8) -> ansi::Rgb {
    match index {
        0 => ansi::Rgb { r: 0, g: 0, b: 0 },
        1 => ansi::Rgb {
            r: 0xcd,
            g: 0x00,
            b: 0x00,
        },
        2 => ansi::Rgb {
            r: 0x00,
            g: 0xcd,
            b: 0x00,
        },
        3 => ansi::Rgb {
            r: 0xcd,
            g: 0xcd,
            b: 0x00,
        },
        4 => ansi::Rgb {
            r: 0x00,
            g: 0x00,
            b: 0xee,
        },
        5 => ansi::Rgb {
            r: 0xcd,
            g: 0x00,
            b: 0xcd,
        },
        6 => ansi::Rgb {
            r: 0x00,
            g: 0xcd,
            b: 0xcd,
        },
        7 => ansi::Rgb {
            r: 0xe5,
            g: 0xe5,
            b: 0xe5,
        },
        8 => ansi::Rgb {
            r: 0x7f,
            g: 0x7f,
            b: 0x7f,
        },
        9 => ansi::Rgb {
            r: 0xff,
            g: 0x00,
            b: 0x00,
        },
        10 => ansi::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0x00,
        },
        11 => ansi::Rgb {
            r: 0xff,
            g: 0xff,
            b: 0x00,
        },
        12 => ansi::Rgb {
            r: 0x5c,
            g: 0x5c,
            b: 0xff,
        },
        13 => ansi::Rgb {
            r: 0xff,
            g: 0x00,
            b: 0xff,
        },
        14 => ansi::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0xff,
        },
        15 => ansi::Rgb {
            r: 0xff,
            g: 0xff,
            b: 0xff,
        },
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
) -> ansi::Rgb {
    term_colors[named].unwrap_or_else(|| match named {
        ansi::NamedColor::Black => fallback_ansi_color(0),
        ansi::NamedColor::Red => fallback_ansi_color(1),
        ansi::NamedColor::Green => fallback_ansi_color(2),
        ansi::NamedColor::Yellow => fallback_ansi_color(3),
        ansi::NamedColor::Blue => fallback_ansi_color(4),
        ansi::NamedColor::Magenta => fallback_ansi_color(5),
        ansi::NamedColor::Cyan => fallback_ansi_color(6),
        ansi::NamedColor::White => fallback_ansi_color(7),
        ansi::NamedColor::BrightBlack => fallback_ansi_color(8),
        ansi::NamedColor::BrightRed => fallback_ansi_color(9),
        ansi::NamedColor::BrightGreen => fallback_ansi_color(10),
        ansi::NamedColor::BrightYellow => fallback_ansi_color(11),
        ansi::NamedColor::BrightBlue => fallback_ansi_color(12),
        ansi::NamedColor::BrightMagenta => fallback_ansi_color(13),
        ansi::NamedColor::BrightCyan => fallback_ansi_color(14),
        ansi::NamedColor::BrightWhite => fallback_ansi_color(15),
        ansi::NamedColor::Foreground => fallback_ansi_color(7),
        ansi::NamedColor::Background => fallback_ansi_color(0),
        ansi::NamedColor::Cursor => fallback_ansi_color(7),
        ansi::NamedColor::DimBlack => fallback_ansi_color(8),
        ansi::NamedColor::DimRed => fallback_ansi_color(8),
        ansi::NamedColor::DimGreen => fallback_ansi_color(8),
        ansi::NamedColor::DimYellow => fallback_ansi_color(8),
        ansi::NamedColor::DimBlue => fallback_ansi_color(8),
        ansi::NamedColor::DimMagenta => fallback_ansi_color(8),
        ansi::NamedColor::DimCyan => fallback_ansi_color(8),
        ansi::NamedColor::DimWhite => fallback_ansi_color(8),
        ansi::NamedColor::BrightForeground => fallback_ansi_color(15),
        ansi::NamedColor::DimForeground => fallback_ansi_color(8),
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

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn snapshot_from_term(term: &Term<NullListener>) -> TerminalSnapshot {
    let rows = term.screen_lines();
    let cols = term.columns();

    let base_style = TerminalCellStyle {
        fg: rgb_to_iocraft(fallback_ansi_color(7)),
        bg: rgb_to_iocraft(fallback_ansi_color(0)),
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

        let mut fg = resolve_color(indexed.cell.fg, term_colors);
        let mut bg = resolve_color(indexed.cell.bg, term_colors);
        let bold = indexed.cell.flags.contains(Flags::BOLD)
            || indexed.cell.flags.contains(Flags::DIM_BOLD);
        let underline = indexed.cell.flags.intersects(Flags::ALL_UNDERLINES);

        if indexed.cell.flags.contains(Flags::DIM) || indexed.cell.flags.contains(Flags::DIM_BOLD) {
            fg = fallback_ansi_color(8);
        }

        if indexed.cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }

        let in_selection = selection
            .map(|range| range.contains_cell(&indexed, cursor.point, cursor.shape))
            .unwrap_or(false);
        if in_selection {
            fg = fallback_ansi_color(0);
            bg = fallback_ansi_color(7);
        }

        let is_cursor_cell =
            cursor.shape != ansi::CursorShape::Hidden && indexed.point == cursor.point;
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

impl AttachedViewer {
    /// Spawn a new attached viewer for a tmux session.
    ///
    /// @pseudocode component-002 lines 10-13
    pub fn spawn(session_name: &str, rows: u16, cols: u16) -> Result<Self, RuntimeError> {
        let pty_system = native_pty_system();

        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::SpawnFailed(format!("openpty: {e}")))?;

        let mut cmd = CommandBuilder::new("tmux");
        cmd.arg("attach-session");
        cmd.arg("-t");
        cmd.arg(session_name);
        cmd.env("TERM", "xterm-256color");

        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::SpawnFailed(format!("spawn tmux attach: {e}")))?;

        // We don't need to keep the child handle for kill - tmux session manages that
        drop(child);

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
        let term = Term::new(config, &term_size, NullListener);
        let term = Arc::new(Mutex::new(term));

        let alive = Arc::new(AtomicBool::new(true));

        // Spawn reader thread
        let term_clone = Arc::clone(&term);
        let alive_clone = Arc::clone(&alive);
        let reader_thread = thread::spawn(move || {
            reader_loop(reader, term_clone, alive_clone);
        });

        Ok(Self {
            master,
            writer,
            term,
            alive,
            _reader_thread: reader_thread,
        })
    }

    /// Check if the viewer is still alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Write input bytes to the PTY.
    ///
    /// @pseudocode component-002 lines 18-20
    #[allow(clippy::significant_drop_tightening)]
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

        Ok(())
    }

    /// Resize the terminal.
    #[allow(clippy::significant_drop_tightening)]
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
    #[allow(clippy::significant_drop_tightening)]
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

    /// Mark the viewer as dead.
    pub fn mark_dead(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

/// Reader loop that feeds PTY output into the terminal model.
fn reader_loop(
    mut reader: Box<dyn Read + Send>,
    term: Arc<Mutex<Term<NullListener>>>,
    alive: Arc<AtomicBool>,
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
                if let Ok(mut term) = term.lock() {
                    for byte in &buf[..n] {
                        parser.advance(&mut *term, *byte);
                    }
                }
            }
            Err(_) => {
                // Reader error - mark viewer as dead
                alive.store(false, Ordering::Relaxed);
                break;
            }
        }
    }
}
