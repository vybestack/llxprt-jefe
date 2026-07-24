//! Deterministic empty-base environment construction (issue #380).
//!
//! The runner never inherits ambient environment. It injects only scenario
//! env plus deterministic `HOME`, `PATH`, `TMPDIR`, `JEFE_CONFIG_DIR`,
//! `JEFE_STATE_DIR`, `JEFE_PLUGIN_DIR`, `LANG=C.UTF-8`, and
//! `TERM=xterm-256color`, all rooted in the workspace. Scenario entries may
//! override the deterministic values (e.g. to extend `PATH`), with
//! `${workspace}` interpolation applied per the closed grammar.

use std::collections::BTreeMap;

use super::contract::EnvVar;
use super::error::HarnessError;
use super::interp;

/// Deterministic name/value pairs rooted at `root` (the workspace path).
fn deterministic_pairs(root: &str) -> Vec<(String, String)> {
    vec![
        ("HOME".to_string(), format!("{root}/home")),
        ("PATH".to_string(), format!("{root}/bin")),
        ("TMPDIR".to_string(), format!("{root}/tmp")),
        ("JEFE_CONFIG_DIR".to_string(), format!("{root}/jefe-config")),
        ("JEFE_STATE_DIR".to_string(), format!("{root}/jefe-state")),
        (
            "JEFE_PLUGIN_DIR".to_string(),
            format!("{root}/jefe-plugins"),
        ),
        ("LANG".to_string(), "C.UTF-8".to_string()),
        ("TERM".to_string(), "xterm-256color".to_string()),
    ]
}

/// Build the closed environment for a launch: deterministic base, workspace
/// env, then launch env, later entries overriding earlier ones. Values are
/// interpolated against the workspace root.
///
/// # Errors
///
/// `HAR-E003` for interpolation violations.
pub fn build(
    workspace_root: &str,
    workspace_env: &[EnvVar],
    launch_env: &[EnvVar],
) -> Result<BTreeMap<String, String>, HarnessError> {
    let mut env: BTreeMap<String, String> =
        deterministic_pairs(workspace_root).into_iter().collect();
    for entry in workspace_env.iter().chain(launch_env) {
        let value = interp::apply(&format!("env.{}", entry.name), &entry.value, workspace_root)?;
        env.insert(entry.name.clone(), value);
    }
    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::super::contract::EnvVar;
    use super::build;

    fn var(name: &str, value: &str) -> EnvVar {
        EnvVar {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn deterministic_base_is_rooted_in_workspace() {
        let env = build("/ws", &[], &[]).unwrap_or_else(|err| panic!("should build: {err}"));
        assert_eq!(env.get("HOME").map(String::as_str), Some("/ws/home"));
        assert_eq!(env.get("PATH").map(String::as_str), Some("/ws/bin"));
        assert_eq!(env.get("TMPDIR").map(String::as_str), Some("/ws/tmp"));
        assert_eq!(
            env.get("JEFE_CONFIG_DIR").map(String::as_str),
            Some("/ws/jefe-config")
        );
        assert_eq!(
            env.get("JEFE_STATE_DIR").map(String::as_str),
            Some("/ws/jefe-state")
        );
        assert_eq!(
            env.get("JEFE_PLUGIN_DIR").map(String::as_str),
            Some("/ws/jefe-plugins")
        );
        assert_eq!(env.get("LANG").map(String::as_str), Some("C.UTF-8"));
        assert_eq!(env.get("TERM").map(String::as_str), Some("xterm-256color"));
        assert_eq!(env.len(), 8, "no ambient variable may leak in");
    }

    #[test]
    fn scenario_env_overrides_and_interpolates() {
        let env = build(
            "/ws",
            &[var("PATH", "${workspace}/bin:/ws-extra")],
            &[var("CUSTOM", "plain"), var("PATH", "${workspace}/override")],
        )
        .unwrap_or_else(|err| panic!("should build: {err}"));
        assert_eq!(env.get("PATH").map(String::as_str), Some("/ws/override"));
        assert_eq!(env.get("CUSTOM").map(String::as_str), Some("plain"));
    }

    #[test]
    fn second_workspace_reference_is_embedded_and_fails() {
        let err = build("/ws", &[var("BAD", "${workspace}/a:${workspace}/b")], &[])
            .err()
            .unwrap_or_else(|| panic!("must fail"));
        assert_eq!(err.code(), super::super::error::HarCode::E003);
    }

    #[test]
    fn invalid_interpolation_fails_build() {
        let err = build("/ws", &[var("BAD", "x${workspace}")], &[])
            .err()
            .unwrap_or_else(|| panic!("must fail"));
        assert_eq!(err.code(), super::super::error::HarCode::E003);
    }
}
