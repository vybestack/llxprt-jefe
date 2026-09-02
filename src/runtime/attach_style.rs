//! Pure color/style resolution for the attached-viewer terminal snapshot
//! (extracted to keep `attach.rs` under the source-file-size limit).
//!
//! These helpers are pure: they map alacritty ANSI color values through the
//! terminal's configured palette (with the standard xterm base/cube/grayscale
//! fallbacks) into the iocraft colors carried by `TerminalCellStyle`.

use alacritty_terminal::vte::ansi;

use super::session::TerminalCellStyle;

pub(super) fn rgb_to_iocraft(rgb: ansi::Rgb) -> iocraft::Color {
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

pub(super) fn fallback_ansi_color(index: u8) -> ansi::Rgb {
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

pub(super) fn resolve_color(
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

pub(super) fn dim_rgb(rgb: ansi::Rgb) -> ansi::Rgb {
    ansi::Rgb {
        r: rgb.r / 2,
        g: rgb.g / 2,
        b: rgb.b / 2,
    }
}

pub(super) fn base_terminal_style() -> TerminalCellStyle {
    // Blank/unwritten cells use `Color::Reset` so the host terminal's default
    // colors show through (issue #179). This keeps unwritten regions visually
    // consistent with written default-bg cells (which also resolve to Reset).
    TerminalCellStyle {
        fg: iocraft::Color::Reset,
        bg: iocraft::Color::Reset,
        bold: false,
        dim: false,
        underline: false,
    }
}
