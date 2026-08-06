use jefe::harness::v1::parse_scenario_v1;

const FAIL_CLOSED_SCENARIO: &[u8] =
    include_bytes!("../dev-docs/tmux-scenarios/issue687/socket-override-fail-closed.json");
const CONTINUITY_SCENARIO: &[u8] =
    include_bytes!("../dev-docs/tmux-scenarios/issue687/session-continuity.json");
const ISOLATION_SCENARIO: &[u8] =
    include_bytes!("../dev-docs/tmux-scenarios/issue687/config-isolation.json");

#[test]
fn socket_override_fail_closed_scenario_is_structurally_valid() {
    let scenario = parse_scenario_v1(FAIL_CLOSED_SCENARIO).unwrap_or_else(|error| {
        panic!("issue 687 fail-closed scenario must satisfy schema 1: {error}")
    });

    assert_eq!(scenario.name, "issue687-socket-override-fail-closed");
}

#[test]
fn same_config_session_continuity_scenario_is_structurally_valid() {
    let scenario = parse_scenario_v1(CONTINUITY_SCENARIO).unwrap_or_else(|error| {
        panic!("issue 687 continuity scenario must satisfy schema 1: {error}")
    });

    assert_eq!(scenario.name, "issue687-session-continuity");
}

#[test]
fn different_config_isolation_scenario_is_structurally_valid() {
    let scenario = parse_scenario_v1(ISOLATION_SCENARIO).unwrap_or_else(|error| {
        panic!("issue 687 isolation scenario must satisfy schema 1: {error}")
    });

    assert_eq!(scenario.name, "issue687-config-isolation");
}
