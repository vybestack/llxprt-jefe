//! The top-level [`Scenario`] model.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::config::ScenarioConfig;
use super::error::ScenarioError;
use super::macro_def::MacroDef;
use super::step::Step;

/// A complete scenario: configuration, macro definitions, and the ordered
/// step list (which may include macro invocations prior to expansion).
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub config: ScenarioConfig,
    #[serde(default)]
    pub macros: BTreeMap<String, MacroDef>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

impl Scenario {
    /// Validate config bounds and macro definitions.
    ///
    /// Macro invocation resolution, arity checks, and cycle detection are
    /// performed by `expand_macros`.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P01
    /// @requirement REQ-TMUX-HARNESS-001
    pub fn validate(&self) -> Result<(), ScenarioError> {
        self.config.validate()?;
        self.validate_macros()
    }

    /// Validate macro definitions independent of invocation expansion.
    fn validate_macros(&self) -> Result<(), ScenarioError> {
        for (name, macro_def) in &self.macros {
            reject_duplicate_params(name, &macro_def.params)?;
        }
        Ok(())
    }
}

/// Reject duplicate parameter names because invocation args are keyed by name.
fn reject_duplicate_params(name: &str, params: &[String]) -> Result<(), ScenarioError> {
    let mut seen = BTreeSet::new();
    for param in params {
        if !seen.insert(param.as_str()) {
            return Err(ScenarioError::InvalidMacro {
                name: name.to_string(),
                reason: format!("duplicate parameter '{param}'"),
            });
        }
    }
    Ok(())
}
