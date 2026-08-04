//! Sandbox engine + platform capability types (extracted from `mod.rs`).
//!
//! Keeps the sandbox-related enums, capability detection, and serde default
//! helpers together so the parent module stays under the source-file-size
//! limit. All items are re-exported via `pub use sandbox::*;` in `mod.rs`.

use serde::{Deserialize, Serialize};

use super::agent_definition::AgentDefinition;
use super::{Id, TypedMap, TypedValue};

/// Default sandbox resource flags passed to llxprt via SANDBOX_FLAGS.
///
/// Memory is expressed in MiB to avoid unitless podman/crun interpretation issues.
pub const DEFAULT_SANDBOX_FLAGS: &str = "--cpus=2 --memory=12288m --pids-limit=256";

/// Project the fixed LLxprt sandbox form controls into definition-owned values.
///
/// Disabled forms remove dependent values so ordinary option/environment
/// emitters remain silent. Enabled forms reject unknown engines before either
/// preview or persistence can observe an invalid enum value.
pub fn apply_sandbox_form_values(
    values: &mut TypedMap,
    definition: &AgentDefinition,
    enabled: bool,
    engine: &str,
    flags: &str,
) -> Option<()> {
    let engine = if enabled {
        let engine = SandboxEngine::from_form_value(engine)?;
        Some(TypedValue::String(engine.as_llxprt_arg().to_owned()))
    } else {
        None
    };
    set_declared_value(
        values,
        definition,
        "sandbox_enabled",
        Some(TypedValue::Bool(enabled)),
    )?;
    set_declared_value(values, definition, "sandbox_engine", engine)?;
    let flags = enabled
        .then(|| flags.trim())
        .filter(|flags| !flags.is_empty())
        .map(|flags| TypedValue::String(flags.to_owned()));
    set_declared_value(values, definition, "sandbox_flags", flags)
}

fn set_declared_value(
    values: &mut TypedMap,
    definition: &AgentDefinition,
    field_id: &str,
    value: Option<TypedValue>,
) -> Option<()> {
    let declared = definition
        .repository_fields
        .iter()
        .chain(definition.agent_fields.iter())
        .any(|field| field.id == field_id);
    if !declared {
        return Some(());
    }
    let key = Id::parse(&field_id.replace('_', "-")).ok()?;
    if let Some(value) = value {
        values.insert(key, value);
    } else {
        values.remove(&key);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent_definition::shipped::shipped_definitions;

    #[test]
    fn invalid_enabled_engine_does_not_mutate_values() {
        let definition = shipped_definitions()
            .into_iter()
            .find(|definition| definition.id.as_str() == "core.llxprt")
            .unwrap_or_else(|| panic!("shipped LLxprt definition must exist"));
        let mut values = TypedMap::new();

        assert!(
            apply_sandbox_form_values(
                &mut values,
                &definition,
                true,
                "unknown",
                DEFAULT_SANDBOX_FLAGS,
            )
            .is_none()
        );
        assert!(values.is_empty());
    }
}
/// Sandbox engine to use when launching llxprt sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxEngine {
    #[default]
    Podman,
    Docker,
    #[serde(alias = "sandbox-exec")]
    Seatbelt,
}

/// All known engine variants in canonical order.
const ALL_ENGINES: [SandboxEngine; 3] = [
    SandboxEngine::Podman,
    SandboxEngine::Docker,
    SandboxEngine::Seatbelt,
];

/// Linux-supported engine variants in canonical order.
const LINUX_ENGINES: [SandboxEngine; 2] = [SandboxEngine::Podman, SandboxEngine::Docker];

impl SandboxEngine {
    /// Convert to llxprt CLI `--sandbox-engine` argument.
    #[must_use]
    pub const fn as_llxprt_arg(self) -> &'static str {
        match self {
            Self::Podman => "podman",
            Self::Docker => "docker",
            Self::Seatbelt => "sandbox-exec",
        }
    }

    /// Parse from user-facing form value.
    #[must_use]
    pub fn from_form_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "podman" => Some(Self::Podman),
            "docker" => Some(Self::Docker),
            "seatbelt" | "sandbox-exec" => Some(Self::Seatbelt),
            _ => None,
        }
    }

    /// User-facing display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Podman => "Podman",
            Self::Docker => "Docker",
            Self::Seatbelt => "Seatbelt",
        }
    }

    /// Cycle to the next *supported* engine for form UX.
    #[must_use]
    pub fn next(self) -> Self {
        self.next_for_capabilities(&PlatformCapabilities::current())
    }

    #[must_use]
    pub(super) fn next_for_capabilities(self, caps: &PlatformCapabilities) -> Self {
        let supported = caps.supported_engines();
        if supported.is_empty() {
            return self;
        }

        let current_pos = supported.iter().position(|e| *e == self);
        match current_pos {
            Some(pos) => supported[(pos + 1) % supported.len()],
            // Current engine not in supported list — reset to first supported.
            None => supported[0],
        }
    }

    /// Parse a form value and advance to the next supported engine.
    #[must_use]
    pub fn next_from_form_value(value: &str) -> Self {
        Self::from_form_value(value).map_or_else(Self::default, Self::next)
    }
}

/// Runtime platform capabilities — resolves which sandbox engines and features
/// are available on the current OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub os: &'static str,
}

impl PlatformCapabilities {
    /// Detect capabilities for the running platform.
    #[must_use]
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS,
        }
    }

    /// Build capabilities for a specific OS (for testing).
    #[must_use]
    pub fn for_os(os: &'static str) -> Self {
        Self { os }
    }

    /// Engines supported on this platform in display/cycle order.
    #[must_use]
    pub fn supported_engines(&self) -> &'static [SandboxEngine] {
        match self.os {
            "macos" => &ALL_ENGINES,
            "linux" => &LINUX_ENGINES,
            _ => &[],
        }
    }

    /// Whether a specific engine is supported on this platform.
    #[must_use]
    pub fn is_engine_supported(&self, engine: SandboxEngine) -> bool {
        match self.os {
            "macos" => true,
            "linux" => !matches!(engine, SandboxEngine::Seatbelt),
            _ => false,
        }
    }

    /// If `engine` is unsupported, return the first supported fallback.
    ///
    /// Returns `None` when this platform supports no sandbox engines.
    #[must_use]
    pub fn normalize_engine(&self, engine: SandboxEngine) -> Option<SandboxEngine> {
        if self.is_engine_supported(engine) {
            return Some(engine);
        }

        self.supported_engines().first().copied()
    }

    /// Short human-readable platform description for diagnostics.
    #[must_use]
    pub fn platform_label(&self) -> &'static str {
        match self.os {
            "macos" => "macOS",
            "linux" => "Linux",
            "windows" => "Windows",
            _ => "Unknown",
        }
    }
}
