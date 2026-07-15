//! Normalized LLxprt npm package selector.
//!
//! This module provides [`LlxprtNpmPackageSelector`] — a domain newtype that
//! represents a *normalized, nonblank* npm package version selector such as
//! `0.9.0` or `0.10.0-nightly.260712.21cb698b6`.
//!
//! ## Invariants
//!
//! - Surrounding whitespace is trimmed; the inner selector content is
//!   preserved exactly (no semver validation, no normalization beyond trim).
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

/// A normalized, nonblank npm package version selector.
///
/// Wraps an inner `String` that is guaranteed non-empty after trimming.
/// `None` (represented as [`Option::None`] at the call site) means "direct
/// llxprt launch — no npm version pinning".
///
/// Construct via [`LlxprtNpmPackageSelector::normalize`] which trims
/// surrounding whitespace and returns `None` for blank input. This keeps the
/// normalization logic in one place so every form, persistence, and launch
/// path agrees.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LlxprtNpmPackageSelector {
    selector: String,
}

impl LlxprtNpmPackageSelector {
    /// Normalize a raw form/persisted value into an optional selector.
    ///
    /// Trims surrounding whitespace. Returns `None` for empty/whitespace-only
    /// input (direct llxprt launch). Nonblank values are preserved exactly
    /// after trimming — no semver validation is applied.
    #[must_use]
    pub fn normalize(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Self {
                selector: trimmed.to_owned(),
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
    #[must_use]
    pub fn package_spec(&self) -> String {
        format!("{LLXPRT_NPM_PACKAGE}@{}", self.selector)
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
        let malicious = "1.0.0; rm -rf /";
        let selector = LlxprtNpmPackageSelector::normalize(malicious);
        assert_eq!(
            selector.as_ref().map(|s| s.as_str().to_owned()),
            Some(malicious.to_owned())
        );
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
}
