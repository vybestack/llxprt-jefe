use super::*;

const OFFICIAL_WRAPPER_MARKER: &str = "LLXPRT_NATIVE_LAUNCHER owned by @vybestack/llxprt-code";
const OFFICIAL_BUN_REL: &str = "node_modules/@vybestack/llxprt-code/node_modules/bun/bin/bun.exe";
const OFFICIAL_ENTRY_REL: &str = "node_modules/@vybestack/llxprt-code/index.ts";

struct OfficialLlxprtFixture {
    work_dir: tempfile::TempDir,
    agent_executable: jefe::runtime::ResolvedAgentExecutable,
    record: PathBuf,
    prompt: String,
}

fn prepare_official_llxprt_fixture() -> OfficialLlxprtFixture {
    let work_dir =
        tempfile::tempdir().unwrap_or_else(|error| panic!("create official fixture dir: {error}"));
    let runtime_dir = work_dir.path().join("runtime");
    fs::create_dir_all(&runtime_dir).unwrap_or_else(|error| panic!("create runtime dir: {error}"));
    let wrapper = runtime_dir.join("llxprt.cmd");
    let body = format!("@echo off\r\nrem {OFFICIAL_WRAPPER_MARKER}\r\nexit /b 0\r\n");
    fs::write(&wrapper, body.as_bytes())
        .unwrap_or_else(|error| panic!("write official wrapper: {error}"));
    let bun = runtime_dir.join(OFFICIAL_BUN_REL);
    let entry = runtime_dir.join(OFFICIAL_ENTRY_REL);
    for path in [&bun, &entry] {
        fs::create_dir_all(path.parent().unwrap_or(runtime_dir.as_path()))
            .unwrap_or_else(|error| panic!("create official layout dir: {error}"));
        fs::copy(FIXTURE, path).unwrap_or_else(|error| panic!("copy fixture binary: {error}"));
    }
    let agent_executable = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![runtime_dir],
        Some(OsString::from(".CMD")),
    )
    .resolve("llxprt")
    .unwrap_or_else(|error| panic!("resolve official LLxprt layout: {error}"));
    let record = work_dir.path().join("official observation.json");
    let prompt = "x".repeat(8_092);
    OfficialLlxprtFixture {
        work_dir,
        agent_executable,
        record,
        prompt,
    }
}

#[test]
fn psmux_official_llxprt_launch_bypasses_cmd_and_delivers_full_prompt() {
    let Some((executable, version_text)) = qualified_psmux() else {
        return;
    };
    let mut namespace = namespace_or_panic(executable.clone(), "official-llxprt", &version_text);
    let fixture = prepare_official_llxprt_fixture();
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        executable,
        MultiplexerIsolation::Namespace(namespace.name.clone()),
    )
    .unwrap_or_else(|error| panic!("construct psmux plan: {error}"));
    let launch_args = vec![
        OsString::from("--record"),
        fixture.record.as_os_str().to_owned(),
        OsString::from("--profile-load"),
        OsString::from("profile"),
        OsString::from(fixture.prompt.clone()),
    ];
    let Some(script) = fixture.agent_executable.script_launch_plan() else {
        panic!("official LLxprt layout must resolve to its canonical Bun entrypoint");
    };
    let mut script_args = vec![script.entrypoint().as_os_str().to_owned()];
    script_args.extend(launch_args);
    let pane = plan
        .agent_pane_command_args_with_launcher(
            (
                script.runtime(),
                jefe::agent_candidate_path::AgentWrapperKind::Direct,
            ),
            &script_args,
            &[],
            Path::new(JEFE),
            fixture.work_dir.path(),
        )
        .unwrap_or_else(|error| panic!("build official pane command: {error}"));
    let mut command = vec![
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from("official-llxprt"),
        OsString::from("-c"),
        fixture.work_dir.path().as_os_str().to_owned(),
    ];
    command.extend(pane);
    namespace
        .run_os(&command)
        .unwrap_or_else(|error| panic!("launch official fixture through psmux: {error}"));
    let deadline = Instant::now() + POLL_TIMEOUT;
    while !fixture.record.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    assert_official_llxprt_observation(&fixture);
    if namespace
        .run(&["has-session", "-t", "official-llxprt"])
        .is_ok()
    {
        namespace
            .run(&["kill-session", "-t", "official-llxprt"])
            .unwrap_or_else(|error| panic!("clean up official session: {error}"));
    }
}

fn assert_official_llxprt_observation(fixture: &OfficialLlxprtFixture) {
    let bytes = fs::read(&fixture.record)
        .unwrap_or_else(|error| panic!("read official observation: {error}"));
    let observation: LaunchObservation = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("decode official observation: {error}"));
    assert_eq!(
        observation.args,
        ["--profile-load", "profile", &fixture.prompt]
    );
    assert!(observation.args.iter().all(|arg| {
        !Path::new(arg)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
    }));
}
