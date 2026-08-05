//! Behavioral contracts for reclaiming stranded conformance namespaces
//! (issue #613).
//!
//! The decisions here are deliberately conservative: a namespace is only ever
//! reclaimed on positive evidence that the jefe which created it is gone and
//! that a server it recorded is still running. Anything ambiguous is left
//! alone, because the cost of a wrong reclaim is killing a live multiplexer.

use std::path::Path;

use super::multiplexer_conformance_sweep::{
    ConformanceLeftover, LeftoverVerdict, classify_leftover, conformance_owner_pid,
    discover_leftovers,
};
use super::process::ProcessLiveness;

/// Write one registry entry as psmux lays them out.
fn registry_entry(directory: &Path, name: &str, contents: &str) {
    std::fs::write(directory.join(name), contents)
        .unwrap_or_else(|error| panic!("registry fixture must be writable: {error}"));
}

/// A registry holding one stranded conformance namespace plus distractions.
fn registry_fixture(directory: &Path) {
    registry_entry(
        directory,
        "jefe-conformance-14296-0__jefe-conformance.pid",
        "3428:134303562883797173",
    );
    registry_entry(
        directory,
        "jefe-conformance-14296-0____warm__.pid",
        "19672:134303562891035964",
    );
    registry_entry(
        directory,
        "jefe-conformance-14296-0__jefe-conformance.key",
        "0123456789abcdef",
    );
    registry_entry(
        directory,
        "jefe-conformance-14296-0__jefe-conformance.port",
        "51423",
    );
    registry_entry(
        directory,
        "jefe-conformance-14296-0__jefe-conformance.sid",
        "17004",
    );
    // A namespace whose server never finished starting: no identity to act on.
    registry_entry(
        directory,
        "jefe-conformance-17004-0____warm__.spawnlock",
        "9516",
    );
    // A real jefe namespace. Reclaiming one of these would kill live agents.
    registry_entry(
        directory,
        "jefe-76134a0ba22f56e9__work.pid",
        "1234:134303562883797173",
    );
    // Neither of these can be acted on: no owner, and no server identity.
    registry_entry(directory, "jefe-conformance-nobody-0__session.pid", "1:2");
    registry_entry(
        directory,
        "jefe-conformance-14297-1__jefe-conformance.pid",
        "not a process",
    );
}

#[test]
fn an_owner_pid_is_read_only_from_a_well_formed_conformance_namespace() {
    assert_eq!(
        conformance_owner_pid("jefe-conformance-14296-0"),
        Some(14296)
    );
    assert_eq!(
        conformance_owner_pid("jefe-conformance-1-4294967295"),
        Some(1)
    );
    for foreign in [
        "jefe-76134a0ba22f56e9",
        "jefe-conformance",
        "jefe-conformance-14296",
        "jefe-conformance-nobody-0",
        "jefe-conformance--0",
        "jefe-conformance-0-0",
        "jefe-conformance-14296-0-1",
        "jefe-conformance-14296-x",
        "not-jefe-conformance-14296-0",
    ] {
        assert_eq!(
            conformance_owner_pid(foreign),
            None,
            "{foreign} must not be read as a conformance namespace"
        );
    }
}

#[test]
fn only_a_dead_owner_with_a_running_server_is_reclaimed() {
    assert_eq!(
        classify_leftover(ProcessLiveness::Dead, &[ProcessLiveness::Alive]),
        LeftoverVerdict::Reclaim
    );
    assert_eq!(
        classify_leftover(
            ProcessLiveness::Dead,
            &[ProcessLiveness::Dead, ProcessLiveness::Alive]
        ),
        LeftoverVerdict::Reclaim
    );

    for retained in [
        (ProcessLiveness::Dead, &[ProcessLiveness::Dead][..]),
        (ProcessLiveness::Dead, &[ProcessLiveness::ReusedPid][..]),
        (
            ProcessLiveness::Dead,
            &[ProcessLiveness::MalformedIdentity][..],
        ),
        (ProcessLiveness::Dead, &[][..]),
        (ProcessLiveness::Alive, &[ProcessLiveness::Alive][..]),
        (ProcessLiveness::Inaccessible, &[ProcessLiveness::Alive][..]),
        (ProcessLiveness::ProbeFailure, &[ProcessLiveness::Alive][..]),
        (ProcessLiveness::ReusedPid, &[ProcessLiveness::Alive][..]),
    ] {
        let (owner, servers) = retained;
        assert_eq!(
            classify_leftover(owner, servers),
            LeftoverVerdict::Retain,
            "owner {owner:?} with servers {servers:?} must be retained"
        );
    }
}

#[test]
fn discovery_reports_one_leftover_per_conformance_namespace_with_every_server_it_recorded() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory must be available: {error}"));
    registry_fixture(directory.path());

    let leftovers = discover_leftovers(directory.path());

    let [leftover] = leftovers.as_slice() else {
        let names: Vec<&str> = leftovers
            .iter()
            .map(ConformanceLeftover::namespace)
            .collect();
        panic!("exactly one namespace is reclaimable, found {names:?}");
    };
    assert_eq!(leftover.namespace(), "jefe-conformance-14296-0");
    assert_eq!(leftover.owner_pid(), 14296);
    assert_eq!(
        leftover
            .servers()
            .iter()
            .map(|server| (server.pid(), server.started_at()))
            .collect::<Vec<_>>(),
        vec![
            (3428, Some(134_303_562_883_797_173)),
            (19672, Some(134_303_562_891_035_964)),
        ],
        "every server the namespace recorded must be considered"
    );
}

#[test]
fn discovery_of_an_absent_registry_yields_nothing() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory must be available: {error}"));

    assert!(discover_leftovers(&directory.path().join("absent")).is_empty());
    assert!(discover_leftovers(directory.path()).is_empty());
}

/// Whether this environment promises a usable psmux.
///
/// CI sets `JEFE_REQUIRE_PSMUX` on the native Windows job. Where it is set, a
/// missing binary is a failure rather than a reason to skip -- a test that
/// quietly does nothing is how a broken runner survives a green build.
#[cfg(windows)]
fn psmux_is_required() -> bool {
    std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|value| value == "1")
}

/// A PID that stands in for the jefe that owned a namespace and is now gone.
///
/// A freshly reaped PID would be more lifelike, but Windows hands PIDs out
/// again quickly and a busy test suite spawns enough short-lived processes to
/// resurrect the "departed" owner mid-test. This value is far outside the range
/// the kernel allocates and is not a multiple of four, which every Windows
/// process ID is, so it reads as departed every time and the sweep sees exactly
/// the evidence a crashed jefe leaves behind.
#[cfg(windows)]
const DEPARTED_OWNER_PID: u32 = 4_294_967_293;

/// A namespace the test brought up, ended when the test lets go of it.
///
/// This test is about a namespace nobody was left to clean up, so it must not
/// become another way to strand one: a panic between bringing the session up
/// and the last assertion would otherwise leave the server running.
#[cfg(windows)]
struct TestNamespace {
    plan: super::MultiplexerPlan,
}

#[cfg(windows)]
impl TestNamespace {
    /// Bring up the scratch session in `namespace`.
    fn start(plan: &super::MultiplexerPlan, namespace: &str) -> Self {
        let plan = plan
            .with_isolation(super::MultiplexerIsolation::Namespace(namespace.to_owned()))
            .unwrap_or_else(|error| panic!("{namespace} must be addressable: {error}"));
        start_session(&plan);
        Self { plan }
    }

    /// The plan addressing this namespace.
    fn plan(&self) -> &super::MultiplexerPlan {
        &self.plan
    }
}

#[cfg(windows)]
impl Drop for TestNamespace {
    fn drop(&mut self) {
        let _ = super::multiplexer_conformance_io::execute_probe(
            &self.plan,
            &["kill-server".to_owned()],
        );
    }
}

/// Bring up the scratch session inside `plan`'s namespace.
#[cfg(windows)]
fn start_session(plan: &super::MultiplexerPlan) {
    let started = super::multiplexer_conformance_io::execute_probe(
        plan,
        &[
            "new-session".to_owned(),
            "-d".to_owned(),
            "-s".to_owned(),
            super::multiplexer_conformance_io::SCRATCH_SESSION.to_owned(),
        ],
    );
    assert_eq!(
        started.exit_code,
        Some(0),
        "the namespace must have come up: {}",
        started.stderr.trim()
    );
}

/// Whether `plan`'s namespace still serves the scratch session.
#[cfg(windows)]
fn session_is_serving(plan: &super::MultiplexerPlan) -> bool {
    super::multiplexer_conformance_io::execute_probe(
        plan,
        &[
            "has-session".to_owned(),
            "-t".to_owned(),
            super::multiplexer_conformance_io::SCRATCH_SESSION.to_owned(),
        ],
    )
    .exit_code
        == Some(0)
}

/// Whether `plan`'s namespace stops serving within a bounded wait.
///
/// `kill-server` returns as soon as the server accepts it, and a loaded machine
/// can take a moment to finish tearing the server down. Waiting keeps the test
/// about whether the sweep reclaimed the namespace rather than about how busy
/// the host was; a namespace the sweep ignored stays up for the whole window.
#[cfg(windows)]
fn session_stops_serving(plan: &super::MultiplexerPlan) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        if !session_is_serving(plan) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

#[cfg(windows)]
#[test]
fn startup_reclaims_a_namespace_whose_jefe_is_gone_and_spares_one_still_in_use() {
    use super::multiplexer_conformance_io::{ScratchNamespace, qualify_multiplexer_for_startup};

    if !psmux_is_required() {
        return;
    }
    let plan = match super::MultiplexerPlan::current_for_test() {
        Ok(plan) => plan,
        Err(error) => panic!("JEFE_REQUIRE_PSMUX is set but no multiplexer resolved: {error}"),
    };

    // A namespace an earlier jefe left behind: its owner is a PID that is gone.
    let stranded_namespace = format!("jefe-conformance-{DEPARTED_OWNER_PID}-0");
    let stranded = TestNamespace::start(&plan, &stranded_namespace);

    // A namespace a running jefe owns. Killing this is the failure the sweep
    // must never commit, so it is held live across the whole startup.
    let Some(live) = ScratchNamespace::reserve(&plan) else {
        panic!("the resolved plan must yield a scratch namespace");
    };
    start_session(live.plan());

    let _qualification = qualify_multiplexer_for_startup(&plan);

    let reclaimed = session_stops_serving(stranded.plan());
    let spared = session_is_serving(live.plan());
    assert!(
        reclaimed,
        "startup must reclaim {stranded_namespace}, whose jefe is gone"
    );
    assert!(
        spared,
        "startup must leave a namespace owned by a running jefe alone"
    );
}
