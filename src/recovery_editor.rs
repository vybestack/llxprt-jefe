//! Configured-editor argv parsing and process execution for recovery.
//!
//! This boundary never invokes a shell or performs expansion, substitution,
//! globbing, or redirection.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

const EDITOR_VARIABLES: [&str; 3] = ["JEFE_EDITOR", "VISUAL", "EDITOR"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EditorError {
    Missing,
    NonUnicode(&'static str),
    Invalid(&'static str),
    Spawn(String),
    Failed(Option<i32>),
}

impl std::fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => formatter.write_str("no configured editor was found"),
            Self::NonUnicode(variable) => write!(formatter, "{variable} is not valid Unicode"),
            Self::Invalid(variable) => write!(formatter, "{variable} has invalid argv syntax"),
            Self::Spawn(error) => write!(formatter, "cannot start configured editor: {error}"),
            Self::Failed(Some(code)) => {
                write!(formatter, "configured editor exited with code {code}")
            }
            Self::Failed(None) => {
                formatter.write_str("configured editor terminated without an exit code")
            }
        }
    }
}

pub(super) fn execute(settings_path: &Path) -> Result<(), EditorError> {
    let (variable, command_line) = configured_editor()?;
    let argv = parse_argv(&command_line).map_err(|()| EditorError::Invalid(variable))?;
    let Some(program) = argv.first() else {
        return Err(EditorError::Invalid(variable));
    };
    let status = Command::new(program)
        .args(&argv[1..])
        .arg(settings_path)
        .status()
        .map_err(|error| EditorError::Spawn(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(EditorError::Failed(status.code()))
    }
}

fn configured_editor() -> Result<(&'static str, String), EditorError> {
    for variable in EDITOR_VARIABLES {
        let Some(value) = std::env::var_os(variable) else {
            continue;
        };
        return os_string(variable, value);
    }
    Err(EditorError::Missing)
}

fn os_string(
    variable: &'static str,
    value: OsString,
) -> Result<(&'static str, String), EditorError> {
    let value = value
        .into_string()
        .map_err(|_| EditorError::NonUnicode(variable))?;
    if value.trim().is_empty() {
        return Err(EditorError::Invalid(variable));
    }
    Ok((variable, value))
}

#[cfg(not(windows))]
fn parse_argv(command_line: &str) -> Result<Vec<String>, ()> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    for character in command_line.chars() {
        if escaped {
            word.push(character);
            escaped = false;
            started = true;
            continue;
        }
        match (quote, character) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('"') | None, '\\') => escaped = true,
            (Some(_), _) => word.push(character),
            (None, '\'' | '"') => {
                quote = Some(character);
                started = true;
            }
            (None, value) if value.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, _) => {
                word.push(character);
                started = true;
            }
        }
    }
    if escaped || quote.is_some() {
        return Err(());
    }
    if started {
        words.push(word);
    }
    (!words.is_empty()).then_some(words).ok_or(())
}

#[cfg(windows)]
fn parse_argv(command_line: &str) -> Result<Vec<String>, ()> {
    let mut words = Vec::new();
    let chars = command_line.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        while chars.get(index).is_some_and(|value| value.is_whitespace()) {
            index += 1;
        }
        if index == chars.len() {
            break;
        }
        let (word, next) = parse_windows_word(&chars, index);
        words.push(word);
        index = next;
    }
    (!words.is_empty()).then_some(words).ok_or(())
}

#[cfg(windows)]
fn parse_windows_word(chars: &[char], mut index: usize) -> (String, usize) {
    let mut word = String::new();
    let mut quoted = false;
    while index < chars.len() {
        if !quoted && chars[index].is_whitespace() {
            break;
        }
        let mut backslashes = 0usize;
        while chars.get(index) == Some(&'\\') {
            backslashes += 1;
            index += 1;
        }
        if chars.get(index) == Some(&'"') {
            push_backslashes(&mut word, backslashes / 2);
            if backslashes.is_multiple_of(2) {
                quoted = !quoted;
            } else {
                word.push('"');
            }
            index += 1;
        } else {
            push_backslashes(&mut word, backslashes);
            if let Some(character) = chars.get(index) {
                word.push(*character);
                index += 1;
            }
        }
    }
    (word, index)
}

#[cfg(windows)]
fn push_backslashes(word: &mut String, count: usize) {
    for _ in 0..count {
        word.push('\\');
    }
}

#[cfg(test)]
mod tests {
    use super::parse_argv;

    #[cfg(not(windows))]
    #[test]
    fn unix_parser_preserves_quoted_arguments_without_shell_expansion() {
        let result = parse_argv("/bin/editor --wait 'two words' \"$HOME/*.toml\"");
        assert_eq!(
            result,
            Ok(vec![
                "/bin/editor".to_owned(),
                "--wait".to_owned(),
                "two words".to_owned(),
                "$HOME/*.toml".to_owned(),
            ])
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_parser_rejects_unterminated_quote_or_escape() {
        assert!(parse_argv("editor 'unfinished").is_err());
        assert!(parse_argv("editor trailing\\").is_err());
    }

    #[test]
    fn parser_rejects_empty_command_line() {
        assert!(parse_argv("  \t ").is_err());
    }
}
