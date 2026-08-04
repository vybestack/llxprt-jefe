//! Stable identity of one theme.
//!
//! A theme is named by its slug, which is what the settings document stores and
//! what the theme manager resolves. The slug is a value type rather than a bare
//! `String` so a caller cannot pass a screen identifier, a display name, or an
//! unvalidated fragment of user input where a theme is expected.

use std::fmt;

/// Longest accepted theme slug, in bytes.
///
/// Slugs are file-name shaped: custom themes are loaded from
/// `<config>/themes/<slug>.json`, so the bound keeps a settings document from
/// naming a theme no filesystem could hold.
pub const THEME_ID_BYTE_LIMIT: usize = 64;

/// Why a theme slug is not a valid identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeIdError {
    /// The slug is empty.
    Empty,
    /// The slug exceeds [`THEME_ID_BYTE_LIMIT`].
    TooLong,
    /// The slug contains a byte outside `a-z`, `0-9`, and `-`.
    InvalidByte,
    /// The slug starts or ends with `-`, or contains `--`.
    InvalidSeparator,
}

impl fmt::Display for ThemeIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self {
            Self::Empty => "theme slug is empty",
            Self::TooLong => "theme slug exceeds the byte limit",
            Self::InvalidByte => "theme slug may contain only 'a'-'z', '0'-'9', and '-'",
            Self::InvalidSeparator => "theme slug may not lead, trail, or repeat '-'",
        };
        formatter.write_str(detail)
    }
}

impl std::error::Error for ThemeIdError {}

/// A validated theme slug.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThemeId(String);

impl ThemeId {
    /// The slug of the default and fallback theme.
    pub const GREEN_SCREEN: &'static str = "green-screen";

    /// Parse a theme slug.
    ///
    /// # Errors
    ///
    /// Returns the specific [`ThemeIdError`] describing the violated rule.
    pub fn parse(value: &str) -> Result<Self, ThemeIdError> {
        if value.is_empty() {
            return Err(ThemeIdError::Empty);
        }
        if value.len() > THEME_ID_BYTE_LIMIT {
            return Err(ThemeIdError::TooLong);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(ThemeIdError::InvalidByte);
        }
        if value.starts_with('-') || value.ends_with('-') || value.contains("--") {
            return Err(ThemeIdError::InvalidSeparator);
        }
        Ok(Self(value.to_owned()))
    }

    /// The default and fallback theme.
    #[must_use]
    pub fn green_screen() -> Self {
        Self(Self::GREEN_SCREEN.to_owned())
    }

    /// Borrow the slug text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ThemeId {
    fn default() -> Self {
        Self::green_screen()
    }
}

impl fmt::Display for ThemeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{THEME_ID_BYTE_LIMIT, ThemeId, ThemeIdError};

    #[test]
    fn shipped_slugs_parse() {
        for slug in ["green-screen", "dracula", "atom-one-dark", "ansi"] {
            assert!(ThemeId::parse(slug).is_ok(), "{slug} must parse");
        }
    }

    #[test]
    fn the_default_is_green_screen() {
        assert_eq!(ThemeId::default().as_str(), "green-screen");
    }

    #[test]
    fn the_grammar_refuses_everything_it_does_not_accept() {
        assert_eq!(ThemeId::parse(""), Err(ThemeIdError::Empty));
        assert_eq!(
            ThemeId::parse(&"a".repeat(THEME_ID_BYTE_LIMIT + 1)),
            Err(ThemeIdError::TooLong)
        );
        assert_eq!(ThemeId::parse("Dracula"), Err(ThemeIdError::InvalidByte));
        assert_eq!(
            ThemeId::parse("green screen"),
            Err(ThemeIdError::InvalidByte)
        );
        assert_eq!(ThemeId::parse("../escape"), Err(ThemeIdError::InvalidByte));
        assert_eq!(ThemeId::parse("-lead"), Err(ThemeIdError::InvalidSeparator));
        assert_eq!(
            ThemeId::parse("trail-"),
            Err(ThemeIdError::InvalidSeparator)
        );
        assert_eq!(
            ThemeId::parse("double--dash"),
            Err(ThemeIdError::InvalidSeparator)
        );
    }

    #[test]
    fn the_slug_at_the_limit_is_accepted() {
        let slug = "a".repeat(THEME_ID_BYTE_LIMIT);
        assert!(ThemeId::parse(&slug).is_ok());
    }
}
