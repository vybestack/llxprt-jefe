//! The top-level [`Scenario`] model.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use super::config::ScenarioConfig;
use super::error::ScenarioError;
use super::macro_def::MacroDef;
use super::step::Step;

/// A complete scenario: configuration, macro definitions, and the ordered
/// step list (which may include macro invocations prior to expansion).
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone)]
pub struct Scenario {
    pub config: ScenarioConfig,
    pub macros: BTreeMap<String, MacroDef>,
    pub steps: Vec<Step>,
}

impl Scenario {
    /// Validate config bounds, step arguments, and macro definitions.
    ///
    /// Macro invocation resolution, arity checks, and cycle detection are
    /// performed by `expand_macros`.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P01
    /// @requirement REQ-TMUX-HARNESS-001
    pub fn validate(&self) -> Result<(), ScenarioError> {
        self.config.validate()?;
        self.validate_steps()?;
        self.validate_macros()
    }

    /// Validate top-level concrete step arguments.
    fn validate_steps(&self) -> Result<(), ScenarioError> {
        for step in &self.steps {
            step.validate()?;
        }
        Ok(())
    }

    /// Validate macro definitions independent of invocation expansion.
    fn validate_macros(&self) -> Result<(), ScenarioError> {
        for (name, macro_def) in &self.macros {
            reject_duplicate_params(name, &macro_def.params)?;
            for step in &macro_def.steps {
                step.validate()?;
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for Scenario {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_map(ScenarioVisitor)
    }
}

struct ScenarioVisitor;

impl<'de> Visitor<'de> for ScenarioVisitor {
    type Value = Scenario;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a scenario object with config, macros, and steps")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut config: Option<ScenarioConfig> = None;
        let mut macros: Option<MacroMap> = None;
        let mut steps: Option<Vec<Step>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "config" => read_once(&mut map, &mut config, "config")?,
                "macros" => read_once(&mut map, &mut macros, "macros")?,
                "steps" => read_once(&mut map, &mut steps, "steps")?,
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                    return Err(de::Error::unknown_field(
                        &key,
                        &["config", "macros", "steps"],
                    ));
                }
            }
        }
        Ok(Scenario {
            config: config.ok_or_else(|| de::Error::missing_field("config"))?,
            macros: macros.unwrap_or_default().into_inner(),
            steps: steps.unwrap_or_default(),
        })
    }
}

fn read_once<'de, A, T>(map: &mut A, slot: &mut Option<T>, field: &str) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
    T: Deserialize<'de>,
{
    if slot.is_some() {
        return Err(de::Error::custom(format!(
            "duplicate field '{field}' in scenario"
        )));
    }
    *slot = Some(map.next_value()?);
    Ok(())
}

#[derive(Default)]
struct MacroMap(BTreeMap<String, MacroDef>);

impl MacroMap {
    fn into_inner(self) -> BTreeMap<String, MacroDef> {
        self.0
    }
}

impl<'de> Deserialize<'de> for MacroMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_map(MacroMapVisitor)
    }
}

struct MacroMapVisitor;

impl<'de> Visitor<'de> for MacroMapVisitor {
    type Value = MacroMap;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a map of macro names to macro definitions")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut macros = BTreeMap::new();
        while let Some(name) = map.next_key::<String>()? {
            if macros.contains_key(&name) {
                return Err(de::Error::custom(format!("duplicate macro name '{name}'")));
            }
            macros.insert(name, map.next_value()?);
        }
        Ok(MacroMap(macros))
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
