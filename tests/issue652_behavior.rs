use jefe::harness::v1::parse_scenario_v1;

const SANDBOX_SAVE_SCENARIO: &[u8] =
    include_bytes!("../dev-docs/tmux-scenarios/issue652/llxprt-sandbox-save.json");

#[test]
fn llxprt_sandbox_save_scenario_is_structurally_valid() {
    let scenario = parse_scenario_v1(SANDBOX_SAVE_SCENARIO).unwrap_or_else(|error| {
        panic!("issue 652 sandbox-save scenario must satisfy the schema-1 grammar: {error}")
    });

    assert_eq!(scenario.name, "llxprt-sandbox-save");
}
