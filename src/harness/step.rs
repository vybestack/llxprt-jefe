//! Step primitives for the harness scenario language.
//!
//! Each step is a single JSON object with a discriminator key naming the
//! primitive (e.g. `{"wait": 100}`), deserialized into the fully typed
//! [`Step`] enum. No `serde_json::Value` escapes into the domain model.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P01
//! @requirement REQ-TMUX-HARNESS-001

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use super::error::ScenarioError;

/// A single concrete step primitive.
///
/// One variant per scenario primitive plus a `Macro` invocation variant that
/// macro expansion replaces with concrete steps.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P01
/// @requirement REQ-TMUX-HARNESS-001
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Pause for `milliseconds` before the next step.
    Wait { milliseconds: u64 },
    /// Type a full line of text (implies a trailing Enter in later phases).
    Line { text: String },
    /// Type literal text without pressing Enter.
    Type { text: String },
    /// Send a single named key (e.g. `"Enter"`, `"Escape"`).
    Key { key: String },
    /// Send a sequence of named keys.
    Keys { keys: Vec<String> },
    /// Block until `pattern` appears on screen.
    WaitFor { pattern: String },
    /// Block until `pattern` no longer appears on screen.
    WaitForNot { pattern: String },
    /// Assert that `pattern` is currently on screen.
    Expect { pattern: String },
    /// Assert that a full-width line ends with `pattern` at the right edge.
    ExpectRightEdge { pattern: String },
    /// Assert that `pattern` appears exactly `count` times.
    ExpectCount { pattern: String, count: u32 },
    /// Capture the current screen under `name`.
    Capture { name: String },
    /// Sample the scrollback history under `name`.
    HistorySample { name: String },
    /// Assert that history changed since the prior sample `name`.
    ExpectHistoryDelta { name: String },
    /// Enter (`true`) or exit (`false`) tmux copy mode.
    CopyMode { enabled: bool },
    /// Block until the target process exits, up to `timeout_ms`.
    WaitForExit { timeout_ms: u64 },
    /// Invoke macro `name` with `args`; expanded away before execution.
    Macro {
        name: String,
        args: BTreeMap<String, String>,
    },
}

impl Step {
    /// Validate semantic step arguments after deserialization.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P01
    /// @requirement REQ-TMUX-HARNESS-001
    pub fn validate(&self) -> Result<(), ScenarioError> {
        match self {
            Self::Key { key } => reject_empty("key", key),
            Self::Keys { keys } => validate_keys(keys),
            Self::WaitFor { pattern } => reject_empty("waitFor", pattern),
            Self::WaitForNot { pattern } => reject_empty("waitForNot", pattern),
            Self::Expect { pattern }
            | Self::ExpectRightEdge { pattern }
            | Self::ExpectCount { pattern, .. } => reject_empty("expect", pattern),
            Self::Capture { name }
            | Self::HistorySample { name }
            | Self::ExpectHistoryDelta { name } => reject_empty("capture name", name),
            Self::Macro { name, args } => validate_macro_invocation(name, args),
            Self::Wait { .. }
            | Self::Line { .. }
            | Self::Type { .. }
            | Self::CopyMode { .. }
            | Self::WaitForExit { .. } => Ok(()),
        }
    }
}

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_map(StepVisitor)
    }
}

/// serde visitor that maps a step object directly to typed fields.
struct StepVisitor;

impl<'de> Visitor<'de> for StepVisitor {
    type Value = Step;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a step object with a single kind key")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut parts = StepParts::default();
        while let Some(key) = map.next_key::<String>()? {
            parts.reject_duplicate::<A::Error>(&key)?;
            read_step_field(&mut map, &mut parts, key)?;
        }
        parts.finish().map_err(de::Error::custom)
    }
}

#[derive(Default)]
struct StepParts {
    seen: BTreeSet<String>,
    core: Option<StepCore>,
    count: Option<u32>,
    args: Option<BTreeMap<String, String>>,
}

enum StepCore {
    Wait(u64),
    Line(String),
    Type(String),
    Key(String),
    Keys(Vec<String>),
    WaitFor(String),
    WaitForNot(String),
    Expect(String),
    ExpectRightEdge(String),
    ExpectCount(String),
    Capture(String),
    HistorySample(String),
    ExpectHistoryDelta(String),
    CopyMode(bool),
    WaitForExit(u64),
    Macro(String),
}

impl StepParts {
    fn reject_duplicate<E>(&mut self, key: &str) -> Result<(), E>
    where
        E: de::Error,
    {
        if !self.seen.insert(key.to_string()) {
            return Err(E::custom(format!(
                "invalid step: duplicate field '{key}' in step"
            )));
        }
        Ok(())
    }

    fn set_core(&mut self, key: &str, core: StepCore) -> Result<(), ScenarioError> {
        if self.core.is_some() {
            return Err(ScenarioError::InvalidStep {
                reason: format!("step has multiple kind keys including '{key}'"),
            });
        }
        self.core = Some(core);
        Ok(())
    }

    fn finish(self) -> Result<Step, ScenarioError> {
        let Self {
            seen: _,
            core,
            count,
            args,
        } = self;
        match core.ok_or_else(missing_kind)? {
            StepCore::Wait(milliseconds) => {
                no_aux(count, args.as_ref()).map(|()| Step::Wait { milliseconds })
            }
            StepCore::Line(text) => no_aux(count, args.as_ref()).map(|()| Step::Line { text }),
            StepCore::Type(text) => no_aux(count, args.as_ref()).map(|()| Step::Type { text }),
            StepCore::Key(key) => no_aux(count, args.as_ref()).map(|()| Step::Key { key }),
            StepCore::Keys(keys) => no_aux(count, args.as_ref()).map(|()| Step::Keys { keys }),
            StepCore::WaitFor(pattern) => {
                no_aux(count, args.as_ref()).map(|()| Step::WaitFor { pattern })
            }
            StepCore::WaitForNot(pattern) => {
                no_aux(count, args.as_ref()).map(|()| Step::WaitForNot { pattern })
            }
            StepCore::Expect(pattern) => {
                no_aux(count, args.as_ref()).map(|()| Step::Expect { pattern })
            }
            StepCore::ExpectRightEdge(pattern) => {
                no_aux(count, args.as_ref()).map(|()| Step::ExpectRightEdge { pattern })
            }
            StepCore::ExpectCount(pattern) => finish_expect_count(pattern, count, args),
            StepCore::Capture(name) => {
                no_aux(count, args.as_ref()).map(|()| Step::Capture { name })
            }
            StepCore::HistorySample(name) => {
                no_aux(count, args.as_ref()).map(|()| Step::HistorySample { name })
            }
            StepCore::ExpectHistoryDelta(name) => {
                no_aux(count, args.as_ref()).map(|()| Step::ExpectHistoryDelta { name })
            }
            StepCore::CopyMode(enabled) => {
                no_aux(count, args.as_ref()).map(|()| Step::CopyMode { enabled })
            }
            StepCore::WaitForExit(timeout_ms) => {
                no_aux(count, args.as_ref()).map(|()| Step::WaitForExit { timeout_ms })
            }
            StepCore::Macro(name) => finish_macro(name, args, count),
        }
    }
}

fn read_step_field<'de, A>(map: &mut A, parts: &mut StepParts, key: String) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match key.as_str() {
        "wait" => set_core(map, parts, &key, StepCore::Wait)?,
        "line" => set_core(map, parts, &key, StepCore::Line)?,
        "type" => set_core(map, parts, &key, StepCore::Type)?,
        "key" => set_core(map, parts, &key, StepCore::Key)?,
        "keys" => set_core(map, parts, &key, StepCore::Keys)?,
        "waitFor" => set_core(map, parts, &key, StepCore::WaitFor)?,
        "waitForNot" => set_core(map, parts, &key, StepCore::WaitForNot)?,
        "expect" => set_core(map, parts, &key, StepCore::Expect)?,
        "expectRightEdge" => set_core(map, parts, &key, StepCore::ExpectRightEdge)?,
        "expectCount" => set_core(map, parts, &key, StepCore::ExpectCount)?,
        "capture" => set_core(map, parts, &key, StepCore::Capture)?,
        "historySample" => set_core(map, parts, &key, StepCore::HistorySample)?,
        "expectHistoryDelta" => set_core(map, parts, &key, StepCore::ExpectHistoryDelta)?,
        "copyMode" => set_core(map, parts, &key, StepCore::CopyMode)?,
        "waitForExit" => set_core(map, parts, &key, StepCore::WaitForExit)?,
        "macro" => set_core(map, parts, &key, StepCore::Macro)?,
        "count" => parts.count = Some(map.next_value()?),
        "args" => parts.args = Some(map.next_value::<MacroArgs>()?.into_inner()),
        other => {
            return Err(de::Error::custom(ScenarioError::UnknownStepKind {
                kind: other.to_string(),
            }));
        }
    }
    Ok(())
}

fn set_core<'de, A, T, F>(
    map: &mut A,
    parts: &mut StepParts,
    key: &str,
    make_core: F,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
    T: Deserialize<'de>,
    F: FnOnce(T) -> StepCore,
{
    let value = map.next_value::<T>()?;
    parts
        .set_core(key, make_core(value))
        .map_err(de::Error::custom)
}

fn finish_expect_count(
    pattern: String,
    count: Option<u32>,
    args: Option<BTreeMap<String, String>>,
) -> Result<Step, ScenarioError> {
    if args.is_some() {
        return Err(unexpected_aux("args"));
    }
    let count = count.ok_or_else(|| missing_field("count"))?;
    Ok(Step::ExpectCount { pattern, count })
}

fn finish_macro(
    name: String,
    args: Option<BTreeMap<String, String>>,
    count: Option<u32>,
) -> Result<Step, ScenarioError> {
    if count.is_some() {
        return Err(unexpected_aux("count"));
    }
    let args = args.ok_or_else(|| missing_field("args"))?;
    Ok(Step::Macro { name, args })
}

#[derive(Debug)]
struct MacroArgs(BTreeMap<String, String>);

impl MacroArgs {
    fn into_inner(self) -> BTreeMap<String, String> {
        self.0
    }
}

impl<'de> Deserialize<'de> for MacroArgs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_map(MacroArgsVisitor)
    }
}

struct MacroArgsVisitor;

impl<'de> Visitor<'de> for MacroArgsVisitor {
    type Value = MacroArgs;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a macro args object with scalar argument values")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut args = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            if args.contains_key(&key) {
                return Err(de::Error::custom(format!(
                    "invalid step: duplicate macro argument '{key}'"
                )));
            }
            args.insert(key, map.next_value::<ScalarArg>()?.into_inner());
        }
        Ok(MacroArgs(args))
    }
}

struct ScalarArg(String);

impl ScalarArg {
    fn into_inner(self) -> String {
        self.0
    }
}

impl<'de> Deserialize<'de> for ScalarArg {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_any(ScalarArgVisitor)
    }
}

struct ScalarArgVisitor;

impl Visitor<'_> for ScalarArgVisitor {
    type Value = ScalarArg;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a string, number, or boolean macro argument")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(ScalarArg(value.to_string()))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(ScalarArg(value.to_string()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(ScalarArg(value.to_string()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        let text = if value.is_finite() && value.fract() == 0.0 {
            format!("{value:.1}")
        } else {
            value.to_string()
        };
        Ok(ScalarArg(text))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(ScalarArg(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(ScalarArg(value))
    }
}

fn reject_empty(field: &str, value: &str) -> Result<(), ScenarioError> {
    if value.is_empty() {
        return Err(ScenarioError::InvalidStep {
            reason: format!("{field} must not be empty"),
        });
    }
    Ok(())
}

fn validate_keys(keys: &[String]) -> Result<(), ScenarioError> {
    if keys.is_empty() {
        return Err(ScenarioError::InvalidStep {
            reason: "keys must contain at least one key".to_string(),
        });
    }
    for key in keys {
        reject_empty("keys item", key)?;
    }
    Ok(())
}

fn no_aux(
    count: Option<u32>,
    args: Option<&BTreeMap<String, String>>,
) -> Result<(), ScenarioError> {
    if count.is_some() {
        return Err(unexpected_aux("count"));
    }
    if args.is_some() {
        return Err(unexpected_aux("args"));
    }
    Ok(())
}

fn unexpected_aux(field: &str) -> ScenarioError {
    ScenarioError::InvalidStep {
        reason: format!("unexpected key '{field}' in step"),
    }
}

fn validate_macro_invocation(
    name: &str,
    args: &BTreeMap<String, String>,
) -> Result<(), ScenarioError> {
    reject_empty("macro name", name)?;
    for key in args.keys() {
        reject_empty("macro argument name", key)?;
    }
    Ok(())
}

fn missing_kind() -> ScenarioError {
    ScenarioError::InvalidStep {
        reason: "step object is missing a step kind".to_string(),
    }
}

/// Build a `MissingField` error for a step field.
fn missing_field(field: &str) -> ScenarioError {
    ScenarioError::MissingField {
        field: field.to_string(),
        context: "step".to_string(),
    }
}
