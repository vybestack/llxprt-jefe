//! Bounded ANSI/OSC escape normalization (issue #382 CW-02 S3a/S3b).
//!
//! Code Puppy's help/version streams interleave terminal palette control
//! sequences (OSC palette queries, CSI SGR codes). The probe layer normalizes
//! the captured stream before identity/capability matching using a bounded
//! scanner rather than a new regex dependency.
//!
//! The scanner recognizes:
//! - CSI sequences: `ESC [` followed by parameter/intermediate bytes ending at
//!   a final byte in `0x40..=0x7e`.
//! - OSC sequences: `ESC ]` followed by bytes until `BEL` (`0x07`) or
//!   `ST` (`ESC \`).
//! - Two-byte escapes: `ESC` followed by any single byte (covers `ESC =`,
//!   `ESC >`, etc.).
//!
//! All other bytes, including valid UTF-8 continuation bytes, are preserved.

use serde::{Deserialize, Serialize};

/// Normalization mode for captured probe streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Normalize {
    /// No normalization; match the raw stream directly.
    #[default]
    None,
    /// Strip ANSI/OSC escape sequences before matching.
    StripAnsi,
}

/// Strip ANSI/OSC escape sequences from a byte stream, preserving all other
/// bytes including multi-byte UTF-8 continuation bytes.
///
/// Returns the normalized bytes. Invalid escape sequences (lone `ESC` at end
/// of stream, unterminated OSC) are handled defensively: a lone trailing
/// `ESC` is dropped and an unterminated OSC drops through to end of input.
#[must_use]
pub fn strip_ansi_escape(input: &[u8]) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(input.len());
    let mut i = 0usize;
    while i < input.len() {
        let byte = input[i];
        if byte != 0x1b {
            out.push(byte);
            i += 1;
            continue;
        }
        // ESC sequence
        if i + 1 >= input.len() {
            // Lone trailing ESC: drop it.
            break;
        }
        let next = input[i + 1];
        match next {
            b'[' => {
                // CSI: scan parameter/intermediate bytes until final byte.
                i += 2;
                while i < input.len() {
                    let c = input[i];
                    i += 1;
                    if (0x40..=0x7e).contains(&c) {
                        break;
                    }
                }
            }
            b']' => {
                // OSC: scan until BEL or ST (ESC \).
                i += 2;
                let mut terminated = false;
                while i < input.len() {
                    let c = input[i];
                    if c == 0x07 {
                        i += 1;
                        terminated = true;
                        break;
                    }
                    if c == 0x1b && i + 1 < input.len() && input[i + 1] == b'\\' {
                        i += 2;
                        terminated = true;
                        break;
                    }
                    i += 1;
                }
                if !terminated {
                    // Unterminated OSC: consumed to end of input.
                    break;
                }
            }
            _ => {
                // Two-byte escape (ESC + single byte).
                i += 2;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_csi_sgr() {
        let input = b"\x1b[1;31mhello\x1b[0m";
        assert_eq!(strip_ansi_escape(input), "hello");
    }

    #[test]
    fn strip_ansi_osc_palette_bel_terminated() {
        let input = b"\x1b]11;#000000\x070.0.634\n";
        assert_eq!(strip_ansi_escape(input), "0.0.634\n");
    }

    #[test]
    fn strip_ansi_osc_st_terminated() {
        let input = b"\x1b]11;#000000\x1b\\0.0.634\n";
        assert_eq!(strip_ansi_escape(input), "0.0.634\n");
    }

    #[test]
    fn strip_ansi_preserves_plain_text() {
        assert_eq!(strip_ansi_escape(b"plain text"), "plain text");
    }

    #[test]
    fn strip_ansi_preserves_utf8() {
        assert_eq!(strip_ansi_escape("héllo".as_bytes()), "héllo");
    }

    #[test]
    fn strip_ansi_two_byte_escape() {
        assert_eq!(strip_ansi_escape(b"\x1b=hi"), "hi");
    }

    #[test]
    fn strip_ansi_lone_trailing_esc_dropped() {
        assert_eq!(strip_ansi_escape(b"hi\x1b"), "hi");
    }

    #[test]
    fn strip_ansi_multiple_interleaved() {
        let input = b"\x1b]11;#000000\x07\x1b[32m\x1b]10;#ffffff\x07version\x1b[0m";
        assert_eq!(strip_ansi_escape(input), "version");
    }

    #[test]
    fn strip_ansi_unterminated_osc_consumes_rest() {
        let input = b"\x1b]11;#000000unterminated";
        assert_eq!(strip_ansi_escape(input), "");
    }

    #[test]
    fn strip_ansi_empty_input() {
        assert_eq!(strip_ansi_escape(b""), "");
    }

    #[test]
    fn strip_ansi_multiple_osc_then_version() {
        // Mimics code-puppy raw stream with palette sequences before version.
        let input = b"\x1b]11;#000000\x07\x1b]10;#ffffff\x07\x1b]12;#cccccc\x070.0.634\n";
        assert_eq!(strip_ansi_escape(input), "0.0.634\n");
    }
}
