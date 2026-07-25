//! Normalized LLxprt npm package selector.
//!
//! This module provides [`LlxprtNpmPackageSelector`] — a domain newtype that
//! represents a *normalized, nonblank* npm package version selector such as
//! `0.9.0` or `0.10.0-nightly.260712.21cb698b6`.
//!
//! ## Invariants
//!
//! - All whitespace (surrounding AND embedded) is stripped; the resulting
//!   selector contains no whitespace characters of any kind. This prevents
//!   pasted version strings with embedded newlines/control characters from
//!   producing unresolvable npm specs (issue #403).
//! - Blank/null/missing values normalize to `None` (direct llxprt launch).
//! - The npm package name is centralized in [`LLXPRT_NPM_PACKAGE`].
//!
//! ## Serialization
//!
//! The selector serializes as a plain JSON string (or `null`/absent for
//! `None`). Legacy state files that lack the field or have a blank value
//! deserialize as `None`, preserving the existing direct-llxprt behavior.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The canonical npm package name for LLxprt Code.
///
/// Centralized so every launch path (local and remote) uses exactly the same
/// package name. The full npm spec is `@vybestack/llxprt-code@VERSION`.
pub const LLXPRT_NPM_PACKAGE: &str = "@vybestack/llxprt-code";

/// The PyPI package name for Code Puppy, used in uvx `--from` specs.
///
/// Centralized so every launch path (local and remote) uses exactly the same
/// package name.
pub const CODE_PUPPY_PACKAGE: &str = "code-puppy";

/// User-facing sentinel meaning "latest stable release" (#337).
///
/// For LLxprt this maps to the npm dist-tag `latest`; for Code Puppy it
/// produces a bare uvx `--from code-puppy` spec, letting uv resolve the
/// newest PyPI release.
pub const LATEST: &str = "latest";

/// User-facing sentinel meaning "latest nightly build" (#337).
///
/// For LLxprt this maps to the npm dist-tag `nightly`; for Code Puppy it
/// also produces a bare uvx `--from code-puppy` spec, since PyPI does not
/// publish a separate nightly channel for `code-puppy`.
pub const LATEST_NIGHTLY: &str = "latest nightly";

/// The npm dist-tag that resolves to the latest nightly build of
/// `@vybestack/llxprt-code`.
const NPM_NIGHTLY_DIST_TAG: &str = "nightly";

/// Return `true` when `value` (after trimming) matches the `latest` sentinel
/// case-insensitively but is **not** `latest nightly`.
///
/// This is the stable-release sentinel: it means "whatever the latest
/// published release is".
#[must_use]
pub fn is_latest_sentinel(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case(LATEST)
}

/// Return `true` when `value` (after trimming) matches the `latest nightly`
/// sentinel case-insensitively.
#[must_use]
pub fn is_latest_nightly_sentinel(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case(LATEST_NIGHTLY)
}

/// Return `true` when `value` matches either the `latest` or `latest nightly`
/// sentinel (case-insensitive after trim).
#[must_use]
pub fn is_version_sentinel(value: &str) -> bool {
    is_latest_sentinel(value) || is_latest_nightly_sentinel(value)
}

/// A normalized, nonblank npm package version selector.
///
/// Wraps an inner `String` that is guaranteed non-empty and contains no
/// whitespace characters. `None` (represented as [`Option::None`] at the call
/// site) means "direct llxprt launch — no npm version pinning".
///
/// Construct via [`LlxprtNpmPackageSelector::normalize`] which strips all
/// whitespace (leading, trailing, and embedded) and returns `None` for
/// empty/whitespace-only input. This keeps the normalization logic in one
/// place so every form, persistence, and launch path agrees.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LlxprtNpmPackageSelector {
    selector: String,
}

impl LlxprtNpmPackageSelector {
    /// Normalize a raw form/persisted value into an optional selector.
    ///
    /// Strips **all** whitespace (leading, trailing, and embedded), including
    /// newlines, tabs, carriage returns, and Unicode/zero-width whitespace.
    /// Returns `None` for empty/whitespace-only input (direct llxprt launch).
    /// Non-whitespace content is preserved exactly — no semver validation is
    /// applied.
    ///
    /// This prevents a pasted version string containing an embedded newline
    /// (issue #403) from producing an unresolvable npm spec: the resulting
    /// selector is guaranteed to contain no whitespace characters.
    #[must_use]
    pub fn normalize(raw: &str) -> Option<Self> {
        let sanitized = strip_internal_whitespace(raw);
        if sanitized.is_empty() {
            None
        } else {
            Some(Self {
                selector: sanitized,
            })
        }
    }

    /// The normalized selector string (e.g. `0.10.0-nightly.260712.21cb698b6`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.selector
    }

    /// The full npm package spec: `@vybestack/llxprt-code@VERSION`.
    ///
    /// Used by the launch path to build `npm exec --yes --package=SPEC`.
    ///
    /// The `latest nightly` sentinel maps to the npm dist-tag `nightly`
    /// (#337): the user types "latest nightly" but npm's dist-tag is
    /// `nightly`. The `latest` sentinel normalizes to the lowercase npm
    /// dist-tag `latest` since npm dist-tags are case-sensitive — a
    /// user-entered `LATEST` would otherwise produce
    /// `@vybestack/llxprt-code@LATEST`, which npm cannot resolve.
    /// Explicit version strings pass through verbatim.
    ///
    /// Sentinel detection uses the selector with whitespace re-inserted as
    /// spaces so `LATEST NIGHTLY` (stored as `LATESTNIGHTLY` after
    /// whitespace stripping) is still recognized. Explicit versions are
    /// emitted verbatim (already whitespace-free).
    #[must_use]
    pub fn package_spec(&self) -> String {
        let sentinel_form = sentinel_with_spaces(&self.selector);
        let effective = if is_latest_nightly_sentinel(&sentinel_form) {
            NPM_NIGHTLY_DIST_TAG.to_owned()
        } else if is_latest_sentinel(&sentinel_form) {
            LATEST.to_owned()
        } else {
            self.selector.clone()
        };
        format!("{LLXPRT_NPM_PACKAGE}@{effective}")
    }
}

/// Determine whether an LLxprt agent launch should use npm or the direct
/// binary.
///
/// A nonblank [`LlxprtNpmPackageSelector`] means the launch must go through
/// `npm exec --yes --package=@vybestack/llxprt-code@VERSION -- llxprt ARGS`.
/// `None` means the existing direct/resolved llxprt binary path is used.
///
/// Code Puppy always uses the direct binary — a dormant selector stored on
/// the agent (from a prior LLxprt configuration) is ignored but retained so
/// switching back to LLxprt restores it.
#[must_use]
pub fn llxprt_launch_source(
    kind: crate::domain::AgentKind,
    version: Option<&LlxprtNpmPackageSelector>,
) -> LaunchSource {
    match kind {
        crate::domain::AgentKind::Llxprt => match version {
            Some(selector) => LaunchSource::NpmBacked(selector.clone()),
            None => LaunchSource::Direct,
        },
        crate::domain::AgentKind::CodePuppy => LaunchSource::Direct,
    }
}

/// The uvx `--from` spec for a Code Puppy version string (#337).
///
/// Returns `None` for blank input (meaning a direct `code-puppy` binary
/// launch, no uvx wrapper). For the `latest` or `latest nightly` sentinels,
/// returns the bare package name `code-puppy` — uvx resolves the newest PyPI
/// release. For an explicit version string, returns
/// `code-puppy==VERSION`.
///
/// Whitespace (leading, trailing, and embedded) is stripped so a pasted
/// version with an embedded newline does not produce a broken uvx spec
/// (issue #403).
///
/// PyPI does not publish a separate nightly channel for `code-puppy`, so
/// `latest nightly` resolves to the same bare package as `latest`.
///
/// This centralizes the spec so the launch planner, capability probe, and
/// package probe all agree.
#[must_use]
pub fn code_puppy_uvx_from_spec(version: &str) -> Option<String> {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_version_sentinel(trimmed) {
        return Some(CODE_PUPPY_PACKAGE.to_owned());
    }
    let sanitized = strip_internal_whitespace(trimmed);
    if sanitized.is_empty() {
        return None;
    }
    if is_version_sentinel(&sanitized) {
        return Some(CODE_PUPPY_PACKAGE.to_owned());
    }
    Some(format!("{CODE_PUPPY_PACKAGE}=={sanitized}"))
}

/// Whether a Code Puppy version string requires the uvx wrapper (#337).
///
/// Returns `true` for any nonblank version (including sentinels). Blank
/// versions — after stripping all whitespace and zero-width characters —
/// launch the direct `code-puppy` binary without uvx.
#[must_use]
pub fn code_puppy_requires_uvx(version: &str) -> bool {
    !strip_internal_whitespace(version).is_empty()
}

/// Strip all embedded whitespace (newlines, tabs, control chars) and
/// zero-width characters from a version string after trimming surrounding
/// whitespace.
///
/// This is the shared sanitization used by both the LLxprt npm selector and
/// the Code Puppy uvx spec path so a pasted version containing embedded
/// newlines cannot produce an unresolvable package spec (issue #403).
/// Sentinel detection (`latest`, `latest nightly`) is performed by callers
/// BEFORE stripping, so sentinels are recognized from their original
/// whitespace-containing form.
fn strip_internal_whitespace(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && !is_version_invisible(*c))
        .collect()
}

/// Whether a character is invisible/zero-width and should be stripped from a
/// version selector.
///
/// Covers zero-width space (U+200B), zero-width non-joiner (U+200C),
/// zero-width joiner (U+200D), byte-order mark / zero-width no-break space
/// (U+FEFF), and soft hyphen (U+00AD). These can be introduced by clipboard
/// paste and are invisible in the single-line form field (issue #403).
fn is_version_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{00AD}'
    )
}

/// Reconstruct the sentinel form (with a space) from a whitespace-stripped
/// selector so sentinel detection works on stored values.
///
/// `latestnightly` → `latest nightly`, `LATESTNIGHTLY` → `LATEST NIGHTLY`.
/// Non-sentinel values pass through unchanged.
fn sentinel_with_spaces(selector: &str) -> String {
    let lower = selector.to_ascii_lowercase();
    if lower == "latestnightly" {
        // Split at the char boundary between "latest" (6 chars) and "nightly".
        let chars: Vec<char> = selector.chars().collect();
        let (head, tail) = chars.split_at(6);
        format!(
            "{} {}",
            head.iter().collect::<String>(),
            tail.iter().collect::<String>()
        )
    } else {
        selector.to_owned()
    }
}

/// Typed launch-source decision for an agent session.
///
/// Distinguishes a direct-binary launch (Code Puppy or unversioned LLxprt)
/// from an npm-backed LLxprt launch (versioned selector).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchSource {
    /// Launch the resolved/direct binary (`llxprt` or `code-puppy`).
    Direct,
    /// Launch via `npm exec --yes --package=@vybestack/llxprt-code@VERSION --
    /// llxprt ARGS`.
    NpmBacked(LlxprtNpmPackageSelector),
}

impl LaunchSource {
    /// Whether this launch source requires npm on the target.
    #[must_use]
    pub const fn requires_npm(&self) -> bool {
        matches!(self, Self::NpmBacked(_))
    }

    /// The npm package selector, if this is an npm-backed launch.
    #[must_use]
    pub fn selector(&self) -> Option<&LlxprtNpmPackageSelector> {
        match self {
            Self::NpmBacked(selector) => Some(selector),
            Self::Direct => None,
        }
    }
}

impl Serialize for LlxprtNpmPackageSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.selector.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LlxprtNpmPackageSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::normalize(&raw).ok_or_else(|| {
            serde::de::Error::custom("blank llxprt_version should be null, not empty string")
        })
    }
}

/// Custom deserializer for `Option<LlxprtNpmPackageSelector>` that treats
/// null, missing, and blank-string values as `None`.
///
/// This is the compatibility layer: legacy state files that lack the field
/// (serde `default`) or store an empty/whitespace string deserialize as
/// `None`, preserving direct-llxprt behavior. Nonblank values are normalized
/// (trimmed) and round-trip exactly.
pub fn deserialize_optional_selector<'de, D>(
    deserializer: D,
) -> Result<Option<LlxprtNpmPackageSelector>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;
    Ok(raw.and_then(|s| LlxprtNpmPackageSelector::normalize(&s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AgentKind;

    #[test]
    fn normalize_trims_surrounding_whitespace() {
        let selector = LlxprtNpmPackageSelector::normalize("  0.9.0  ");
        assert_eq!(
            selector.as_ref().map(|s| s.as_str().to_owned()),
            Some("0.9.0".to_owned())
        );
        normalize_returns_none_for_blank();
        normalize_preserves_nightly_selector_exactly();
        normalize_preserves_metacharacters_as_data();
        package_spec_is_centralized_name_at_version();
    }

    fn normalize_returns_none_for_blank() {
        assert!(LlxprtNpmPackageSelector::normalize("").is_none());
        assert!(LlxprtNpmPackageSelector::normalize("   ").is_none());
        assert!(LlxprtNpmPackageSelector::normalize("\t\n").is_none());
    }

    fn normalize_preserves_nightly_selector_exactly() {
        let nightly = "0.10.0-nightly.260712.21cb698b6";
        let selector = LlxprtNpmPackageSelector::normalize(nightly);
        assert_eq!(
            selector.as_ref().map(|s| s.as_str().to_owned()),
            Some(nightly.to_owned())
        );
    }

    fn normalize_preserves_metacharacters_as_data() {
        // Shell metacharacters must be preserved as data, not interpreted.
        // The launch path shell-escapes them, but the selector stores them.
        // Note: whitespace is stripped (see normalize_strips_internal_whitespace),
        // so the space in the original input is removed.
        let malicious = "1.0.0;rm-rf/";
        let selector = LlxprtNpmPackageSelector::normalize("1.0.0; rm -rf /");
        assert_eq!(
            selector.as_ref().map(|s| s.as_str().to_owned()),
            Some(malicious.to_owned())
        );
    }

    #[test]
    fn normalize_strips_internal_whitespace() {
        // Embedded newlines, tabs, carriage returns, and spaces must be
        // stripped — an npm version selector is a single whitespace-free
        // token. This is the root cause of issue #403 Bug 2: a pasted
        // version with an embedded newline produced an unresolvable npm spec.
        assert_eq!(
            LlxprtNpmPackageSelector::normalize(
                "0.9.0
0"
            )
            .as_ref()
            .map(|s| s.as_str().to_owned()),
            Some("0.9.00".to_owned())
        );
        assert_eq!(
            LlxprtNpmPackageSelector::normalize(
                "0.9.0
1"
            )
            .as_ref()
            .map(|s| s.as_str().to_owned()),
            Some("0.9.01".to_owned())
        );
        assert_eq!(
            LlxprtNpmPackageSelector::normalize("0	9	0")
                .as_ref()
                .map(|s| s.as_str().to_owned()),
            Some("090".to_owned())
        );
        assert_eq!(
            LlxprtNpmPackageSelector::normalize("0 9 0")
                .as_ref()
                .map(|s| s.as_str().to_owned()),
            Some("090".to_owned())
        );
    }

    #[test]
    fn normalize_strips_unicode_and_control_whitespace() {
        // Zero-width and Unicode whitespace must also be stripped.
        assert_eq!(
            LlxprtNpmPackageSelector::normalize("0.9.0\u{200B}")
                .as_ref()
                .map(|s| s.as_str().to_owned()),
            Some("0.9.0".to_owned())
        );
        assert_eq!(
            LlxprtNpmPackageSelector::normalize("0.9.0\u{00A0}1")
                .as_ref()
                .map(|s| s.as_str().to_owned()),
            Some("0.9.01".to_owned())
        );
    }

    #[test]
    fn normalize_returns_none_when_only_whitespace_remains() {
        // Input that is entirely whitespace (internal or surrounding) yields None.
        assert!(
            LlxprtNpmPackageSelector::normalize(
                " 
	 "
            )
            .is_none()
        );
        assert!(LlxprtNpmPackageSelector::normalize("\u{200B}").is_none());
    }

    #[test]
    fn code_puppy_helpers_strip_internal_whitespace() {
        // The Code Puppy uvx spec path must apply the same whitespace
        // sanitization so a pasted version with embedded whitespace does not
        // produce a broken uvx spec.
        assert_eq!(
            code_puppy_uvx_from_spec(
                "0.0.361
0"
            ),
            Some("code-puppy==0.0.3610".to_owned())
        );
        assert_eq!(
            code_puppy_uvx_from_spec("0	0	361"),
            Some("code-puppy==00361".to_owned())
        );
        assert!(code_puppy_requires_uvx(
            "0.0.361
0"
        ));
    }

    fn selector(value: &str) -> LlxprtNpmPackageSelector {
        LlxprtNpmPackageSelector::normalize(value)
            .unwrap_or_else(|| panic!("selector fixture must be nonblank"))
    }

    fn package_spec_is_centralized_name_at_version() {
        assert_eq!(
            selector("0.9.0").package_spec(),
            "@vybestack/llxprt-code@0.9.0"
        );
    }

    fn launch_source_direct_for_unversioned_llxprt() {
        let source = llxprt_launch_source(AgentKind::Llxprt, None);
        assert_eq!(source, LaunchSource::Direct);
        assert!(!source.requires_npm());
        launch_source_npm_backed_for_versioned_llxprt();
        launch_source_ignores_dormant_selector_for_code_puppy();
    }

    fn launch_source_npm_backed_for_versioned_llxprt() {
        let selector = selector("0.9.0");
        let source = llxprt_launch_source(AgentKind::Llxprt, Some(&selector));
        assert!(source.requires_npm());
        assert_eq!(source.selector(), Some(&selector));
    }

    fn launch_source_ignores_dormant_selector_for_code_puppy() {
        let selector = selector("0.9.0");
        let source = llxprt_launch_source(AgentKind::CodePuppy, Some(&selector));
        assert_eq!(source, LaunchSource::Direct);
        assert!(!source.requires_npm());
    }

    #[test]

    fn serde_round_trips_nonblank_selector() {
        launch_source_direct_for_unversioned_llxprt();
        let selector = selector("0.10.0-nightly.260712.21cb698b6");
        let json = serde_json::to_string(&selector)
            .unwrap_or_else(|error| panic!("serialize selector: {error}"));
        assert_eq!(json, "\"0.10.0-nightly.260712.21cb698b6\"");
        let deserialized: LlxprtNpmPackageSelector = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("deserialize selector: {error}"));
        assert_eq!(deserialized, selector);
        optional_deserialize_null_as_none();
        optional_serialize_none_as_null();
    }

    fn optional_deserialize_null_as_none() {
        let json = "null";
        let result: Option<LlxprtNpmPackageSelector> = serde_json::from_str(json).unwrap_or(None);
        assert!(result.is_none());
        optional_deserialize_blank_as_none_via_custom();
        optional_deserialize_nonblank_as_normalized();
    }

    fn optional_deserialize_blank_as_none_via_custom() {
        let mut de = serde_json::Deserializer::from_str("\"   \"");
        assert!(deserialize_optional_selector(&mut de).is_ok_and(|value| value.is_none()));
    }

    fn optional_deserialize_nonblank_as_normalized() {
        let mut de = serde_json::Deserializer::from_str("\"  0.9.0  \"");
        let Ok(Some(selector)) = deserialize_optional_selector(&mut de) else {
            panic!("expected normalized optional selector");
        };
        assert_eq!(selector.as_str(), "0.9.0");
    }

    fn optional_serialize_none_as_null() {
        let value: Option<LlxprtNpmPackageSelector> = None;
        let json = serde_json::to_string(&value)
            .unwrap_or_else(|error| panic!("serialize empty selector: {error}"));
        assert_eq!(json, "null");
        optional_serialize_some_as_string();
    }

    fn optional_serialize_some_as_string() {
        let value = Some(selector("0.9.0"));
        let json = serde_json::to_string(&value)
            .unwrap_or_else(|error| panic!("serialize selector: {error}"));
        assert_eq!(json, "\"0.9.0\"");
    }

    #[test]
    fn latest_sentinel_is_recognized_case_insensitively() {
        assert!(is_latest_sentinel("latest"));
        assert!(is_latest_sentinel("Latest"));
        assert!(is_latest_sentinel("LATEST"));
        assert!(is_latest_sentinel("  latest  "));
        // Not the nightly variant
        assert!(!is_latest_sentinel("latest nightly"));
        // Explicit versions are not sentinels
        assert!(!is_latest_sentinel("0.9.0"));
    }

    #[test]
    fn latest_nightly_sentinel_is_recognized_case_insensitively() {
        assert!(is_latest_nightly_sentinel("latest nightly"));
        assert!(is_latest_nightly_sentinel("Latest Nightly"));
        assert!(is_latest_nightly_sentinel("LATEST NIGHTLY"));
        assert!(is_latest_nightly_sentinel("  latest nightly  "));
        // Plain latest is not nightly
        assert!(!is_latest_nightly_sentinel("latest"));
        // Explicit nightly version is not the sentinel
        assert!(!is_latest_nightly_sentinel(
            "0.10.0-nightly.260712.21cb698b6"
        ));
    }

    #[test]
    fn version_sentinel_predicate_covers_both() {
        assert!(is_version_sentinel("latest"));
        assert!(is_version_sentinel("latest nightly"));
        assert!(is_version_sentinel("Latest"));
        assert!(!is_version_sentinel("0.9.0"));
        assert!(!is_version_sentinel(""));
    }

    #[test]
    fn npm_package_spec_maps_latest_sentinel_to_latest_dist_tag() {
        let spec = selector("latest").package_spec();
        assert_eq!(spec, "@vybestack/llxprt-code@latest");
    }

    #[test]
    fn npm_package_spec_maps_latest_nightly_sentinel_to_nightly_dist_tag() {
        // The user types "latest nightly" but npm's dist-tag is "nightly"
        let spec = selector("latest nightly").package_spec();
        assert_eq!(spec, "@vybestack/llxprt-code@nightly");
    }

    #[test]
    fn npm_package_spec_preserves_explicit_nightly_version() {
        let version = "0.10.0-nightly.260712.21cb698b6";
        let spec = selector(version).package_spec();
        assert_eq!(spec, format!("@vybestack/llxprt-code@{version}"));
    }

    #[test]
    fn npm_package_spec_normalizes_latest_sentinel_case() {
        // npm dist-tags are case-sensitive; uppercase sentinels must normalize
        // to lowercase so npm can resolve them.
        assert_eq!(
            selector("LATEST").package_spec(),
            "@vybestack/llxprt-code@latest"
        );
        assert_eq!(
            selector("Latest").package_spec(),
            "@vybestack/llxprt-code@latest"
        );
        assert_eq!(
            selector("LATEST NIGHTLY").package_spec(),
            "@vybestack/llxprt-code@nightly"
        );
        assert_eq!(
            selector("Latest Nightly").package_spec(),
            "@vybestack/llxprt-code@nightly"
        );
    }

    #[test]
    fn code_puppy_uvx_spec_latest_is_bare_package() {
        // "latest" → bare "code-puppy" (uv resolves newest PyPI release)
        assert_eq!(
            code_puppy_uvx_from_spec("latest"),
            Some("code-puppy".to_owned())
        );
        assert_eq!(
            code_puppy_uvx_from_spec("Latest"),
            Some("code-puppy".to_owned())
        );
        assert_eq!(
            code_puppy_uvx_from_spec("  latest  "),
            Some("code-puppy".to_owned())
        );
    }

    #[test]
    fn code_puppy_uvx_spec_latest_nightly_is_bare_package() {
        // PyPI has no nightly channel for code-puppy, so both sentinels map
        // to the bare package name
        assert_eq!(
            code_puppy_uvx_from_spec("latest nightly"),
            Some("code-puppy".to_owned())
        );
        assert_eq!(
            code_puppy_uvx_from_spec("Latest Nightly"),
            Some("code-puppy".to_owned())
        );
    }

    #[test]
    fn code_puppy_uvx_spec_explicit_version_is_pinned() {
        assert_eq!(
            code_puppy_uvx_from_spec("0.0.361"),
            Some("code-puppy==0.0.361".to_owned())
        );
        assert_eq!(
            code_puppy_uvx_from_spec("  0.0.361  "),
            Some("code-puppy==0.0.361".to_owned())
        );
    }

    #[test]
    fn code_puppy_uvx_spec_blank_is_none() {
        assert!(code_puppy_uvx_from_spec("").is_none());
        assert!(code_puppy_uvx_from_spec("   ").is_none());
    }

    #[test]
    fn code_puppy_requires_uvx_for_nonblank_versions() {
        assert!(code_puppy_requires_uvx("latest"));
        assert!(code_puppy_requires_uvx("latest nightly"));
        assert!(code_puppy_requires_uvx("0.0.361"));
        assert!(!code_puppy_requires_uvx(""));
        assert!(!code_puppy_requires_uvx("  "));
    }
}
