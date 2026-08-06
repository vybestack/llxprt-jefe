//! What a multiplexer binary says it is.
//!
//! Parsing `-V` output is a self-contained concern: it is pure, it is the only
//! part of multiplexer handling that never touches the filesystem or a child
//! process, and it is exercised by far more cases than the surrounding plan
//! resolution. It lives apart from `multiplexer.rs` so that the code deciding
//! *how to run* a multiplexer is not interleaved with the code deciding *what
//! the multiplexer is*.

use super::multiplexer::{LocalPlatform, MultiplexerError, ProbeObservation};
use std::path::{Path, PathBuf};
use std::process::Output;

const MINIMUM_PSMUX_VERSION: MultiplexerVersion = MultiplexerVersion::new(3, 3, 7);
const WINDOWS_INSTALL_GUIDANCE: &str =
    "install psmux 3.3.7 or newer with `winget upgrade marlocarlo.psmux`, then restart Jefe";
const UNIX_INSTALL_GUIDANCE: &str =
    "install upstream tmux with your operating system package manager";

/// Parsed tmux-compatible semantic version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MultiplexerVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl MultiplexerVersion {
    /// Construct a parsed version.
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse output such as `tmux 3.3.6`.
    pub fn parse(output: &str) -> Result<Self, MultiplexerError> {
        let token = output
            .split_whitespace()
            .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            .ok_or_else(|| MultiplexerError::MalformedVersion {
                path: None,
                output: output.to_owned(),
            })?;
        let mut components = token.split('.');
        let major_raw = components.next().ok_or_else(|| malformed_version(output))?;
        let major = parse_strict_version_part(major_raw, output)?;
        let minor_raw = components.next();
        let patch_raw = components.next();
        // After consuming up to three components, no trailing component may remain.
        if components.next().is_some() {
            return Err(malformed_version(output));
        }
        // The major component is always strict. Only the final present component
        // may carry a single alphabetic release letter (e.g. Homebrew `tmux 3.7b`).
        let (minor, patch) = match (minor_raw, patch_raw) {
            (Some(minor_raw), None) => {
                let minor = parse_final_version_part(minor_raw, output)?;
                (minor, 0)
            }
            (Some(minor_raw), Some(patch_raw)) => {
                let minor = parse_strict_version_part(minor_raw, output)?;
                let patch = parse_final_version_part(patch_raw, output)?;
                (minor, patch)
            }
            (None, _) => return Err(malformed_version(output)),
        };
        Ok(Self::new(major, minor, patch))
    }
}

impl std::fmt::Display for MultiplexerVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Identity of the multiplexer binary: the version it reports, plus the commit
/// it was built from when it reports one.
///
/// `psmux -V` prints two lines — the tmux version it emulates, then its own
/// version and build commit:
///
/// ```text
/// tmux 3.3.7
/// psmux 3.3.7 (cb098c0 2026-08-03)
/// ```
///
/// Builds routinely share a version while differing in commit, so the version
/// alone cannot serve as the identity of anything that must notice the binary
/// changing underneath it (issue #547 V8-V10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiplexerIdentity {
    version: MultiplexerVersion,
    commit: Option<String>,
}

impl MultiplexerIdentity {
    /// Parse the full stdout of a `-V` invocation.
    ///
    /// The version is required; the commit is optional, because upstream tmux
    /// does not report one.
    pub fn parse(output: &str) -> Result<Self, MultiplexerError> {
        Ok(Self {
            version: MultiplexerVersion::parse(output)?,
            commit: parse_build_commit(output),
        })
    }

    /// The reported version.
    #[must_use]
    pub const fn version(&self) -> MultiplexerVersion {
        self.version
    }

    /// The build commit, when the binary reports one.
    #[must_use]
    pub fn commit(&self) -> Option<&str> {
        self.commit.as_deref()
    }
}

impl std::fmt::Display for MultiplexerIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.commit {
            Some(commit) => write!(formatter, "{} ({commit})", self.version),
            None => write!(formatter, "{}", self.version),
        }
    }
}

pub(super) fn output_observation(
    platform: LocalPlatform,
    path: &Path,
    output: Output,
) -> ProbeObservation {
    ProbeObservation::Output {
        platform,
        path: path.to_path_buf(),
        status_success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

pub(super) fn classify_output(
    platform: LocalPlatform,
    path: PathBuf,
    status_success: bool,
    stdout: String,
    stderr: String,
) -> Result<MultiplexerIdentity, MultiplexerError> {
    if !status_success {
        return Err(MultiplexerError::LaunchFailed {
            path,
            reason: stderr,
            guidance: guidance(platform),
        });
    }
    let identity = MultiplexerIdentity::parse(&stdout).map_err(|error| match error {
        MultiplexerError::MalformedVersion { output, .. } => MultiplexerError::MalformedVersion {
            path: Some(path.clone()),
            output,
        },
        other => other,
    })?;
    if platform == LocalPlatform::Windows && identity.version() < MINIMUM_PSMUX_VERSION {
        return Err(MultiplexerError::UnsupportedVersion {
            path,
            detected: identity.version(),
            minimum: MINIMUM_PSMUX_VERSION,
            guidance: WINDOWS_INSTALL_GUIDANCE,
        });
    }
    Ok(identity)
}

pub(super) const fn guidance(platform: LocalPlatform) -> &'static str {
    match platform {
        LocalPlatform::Unix => UNIX_INSTALL_GUIDANCE,
        LocalPlatform::Windows => WINDOWS_INSTALL_GUIDANCE,
    }
}

/// Extract the build commit from a `psmux <version> (<commit> <date>)` line.
///
/// A token that is not a plausible abbreviated hash is treated as absent.
/// Accepting one would be worse than reporting none: a value that varies per
/// launch would key a namespace that can never be found again, which is the
/// exact failure this issue exists to remove.
fn parse_build_commit(output: &str) -> Option<String> {
    let line = output
        .lines()
        .find(|line| line.trim_start().starts_with("psmux"))?;
    let open = line.find('(')?;
    let close = line[open..].find(')')? + open;
    let token = line[open + 1..close].split_whitespace().next()?;
    is_commit_hash(token).then(|| token.to_owned())
}

/// Whether a token looks like an abbreviated git hash.
fn is_commit_hash(token: &str) -> bool {
    (7..=40).contains(&token.len()) && token.chars().all(|character| character.is_ascii_hexdigit())
}

fn parse_strict_version_part(part: &str, source: &str) -> Result<u32, MultiplexerError> {
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed_version(source));
    }
    part.parse::<u32>().map_err(|_| malformed_version(source))
}

/// Parse the final present version component, permitting an optional single
/// trailing *lowercase* ASCII release letter (e.g. Homebrew `tmux 3.7b`).
///
/// Uppercase is rejected on purpose: upstream tmux has only ever shipped
/// lowercase release letters, so `3.7B` is far more likely to be a mangled
/// version string than a real release, and accepting it would let a
/// misidentified binary pass as a known one.
///
/// The letter carries no semantic weight beyond release identification; it is
/// discarded so that `3.7b` resolves to `3.7.0` and `3.3.6a` to `3.3.6`.
fn parse_final_version_part(part: &str, source: &str) -> Result<u32, MultiplexerError> {
    let digits_end = part
        .bytes()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(part.len());
    let (digits, suffix) = part.split_at(digits_end);
    let valid_suffix = suffix.is_empty()
        || (suffix.len() == 1
            && suffix
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase()));
    if digits.is_empty() || !valid_suffix {
        return Err(malformed_version(source));
    }
    digits.parse::<u32>().map_err(|_| malformed_version(source))
}

fn malformed_version(source: &str) -> MultiplexerError {
    MultiplexerError::MalformedVersion {
        path: None,
        output: source.to_owned(),
    }
}
