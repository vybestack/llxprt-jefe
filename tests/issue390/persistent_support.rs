//! Shared helpers for the persistent provider integration tests
//! (issue #390 CW-10, Slice C2).
//!
//! Hosts the cross-platform `jefe-provider-fixture`-driven `Scene`, the
//! deterministic host-environment resolvers, the fast test bounds, and the
//! bounded poll/reap helpers shared by the `lifecycle` and `remediation`
//! modules. Mirrors the no-unwrap/no-expect test style of `tests/doctor`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use jefe::domain::{CanonicalSemver, Id, TypedMap};
use jefe::runtime::provider::environment::{HostEnv, ProviderEnvironment};
use jefe::runtime::provider::persistent::{
    PersistentCandidate, PersistentStartupResult, PersistentSupervisor, ReapedCandidate,
};
use jefe::runtime::provider::protocol::{Capability, ConfigurePayload, EnvName, RequestId};
use jefe::runtime::provider::supervisor::SupervisorBounds;

/// The cross-platform persistent-provider fixture binary under test.
pub const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-provider-fixture");

/// A distinctive secret canary used across the persistent redaction tests.
pub const SECRET: &str = "SUPER-secret-canary-390";

/// Bounds tuned for fast, deterministic tests while preserving the staged order.
pub fn fast_bounds() -> SupervisorBounds {
    SupervisorBounds {
        handshake: Duration::from_secs(3),
        invocation: Duration::from_secs(5),
        shutdown_ack: Duration::from_secs(1),
        stdin_close: Duration::from_secs(1),
        final_drain: Duration::from_secs(1),
    }
}

/// A deterministic host environment resolver (never touches the real process env).
pub struct EmptyEnv;

impl HostEnv for EmptyEnv {
    fn get(&self, _name: &str) -> Option<String> {
        None
    }
}

/// A deterministic host-environment resolver carrying fixed key/value pairs.
pub struct FixedEnv {
    vars: Vec<(String, String)>,
}

impl FixedEnv {
    /// Build a resolver from `(&str, &str)` pairs.
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            vars: pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }
}

impl HostEnv for FixedEnv {
    fn get(&self, name: &str) -> Option<String> {
        self.vars
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

/// A self-cleaning scene owning the temp home, the provider/record directories,
/// and the candidate factory.
pub struct Scene {
    /// Kept alive so the temp tree persists for the test's lifetime.
    pub home: tempfile::TempDir,
    pub provider_dir: PathBuf,
    pub record_dir: PathBuf,
}

impl Scene {
    /// Create a fresh scene with provider and record directories.
    pub fn new() -> Self {
        let home = tempfile::tempdir().unwrap_or_else(|error| panic!("home tempdir: {error:?}"));
        let provider_dir = home.path().join("bin");
        std::fs::create_dir_all(&provider_dir)
            .unwrap_or_else(|error| panic!("create provider dir: {error:?}"));
        let record_dir = home.path().join("records");
        std::fs::create_dir_all(&record_dir)
            .unwrap_or_else(|error| panic!("create record dir: {error:?}"));
        Self {
            home,
            provider_dir,
            record_dir,
        }
    }

    /// Build a persistent candidate with the given plugin id, fixture mode, and
    /// declared capabilities. The record directory is passed as argv[2] so the
    /// fixture records its plugin id and pid.
    pub fn candidate(
        &self,
        plugin_id: &str,
        mode: &str,
        declared: Vec<Capability>,
    ) -> PersistentCandidate {
        PersistentCandidate {
            plugin_id: Id::parse(plugin_id).unwrap_or_else(|err| panic!("plugin id: {err:?}")),
            plugin_version: CanonicalSemver::parse("1.0.0")
                .unwrap_or_else(|err| panic!("version: {err:?}")),
            binary: PathBuf::from(FIXTURE),
            arguments: vec![
                mode.to_owned(),
                self.record_dir.to_string_lossy().into_owned(),
            ],
            working_dir: self.provider_dir.clone(),
            environment: ProviderEnvironment {
                provider_dir: self.provider_dir.clone(),
                nonsecret: BTreeMap::new(),
                secret_env: BTreeMap::new(),
                configure_secret_sources: BTreeMap::new(),
            },
            home: self.home.path().join("home"),
            tmpdir: self.home.path().join("tmp"),
            locale: "C".to_owned(),
            host_api: "jefe/test".to_owned(),
            generation: 1,
            request_id: RequestId::parse("h-000001")
                .unwrap_or_else(|err| panic!("request id: {err:?}")),
            configure: ConfigurePayload {
                config_version: 1,
                config: TypedMap::new(),
                secrets: BTreeMap::new(),
                environment: BTreeMap::new(),
            },
            declared_capabilities: declared,
        }
    }

    /// Build a persistent candidate whose binary does not exist, so spawn fails.
    pub fn unspawnable_candidate(&self, plugin_id: &str) -> PersistentCandidate {
        let mut candidate =
            self.candidate(plugin_id, "persistent-ready", vec![Capability::Actions]);
        candidate.binary = PathBuf::from("/no/such/jefe-provider-binary-390");
        candidate
    }

    /// Build a persistent candidate that sources one Configure secret from the
    /// host variable `HOST_DEPLOY_KEY` under the owning binding `DEPLOY_KEY`.
    pub fn candidate_with_secret(
        &self,
        plugin_id: &str,
        mode: &str,
        declared: Vec<Capability>,
    ) -> PersistentCandidate {
        let mut candidate = self.candidate(plugin_id, mode, declared);
        candidate.environment.configure_secret_sources.insert(
            EnvName::parse("DEPLOY_KEY").unwrap_or_else(|e| panic!("env name: {e:?}")),
            EnvName::parse("HOST_DEPLOY_KEY").unwrap_or_else(|e| panic!("env name: {e:?}")),
        );
        candidate
    }
}

/// Read the deterministic startup-sequence file the fixtures wrote.
pub fn startup_sequence(scene: &Scene) -> Vec<String> {
    let text = std::fs::read_to_string(scene.record_dir.join("startup-sequence.txt"))
        .unwrap_or_else(|error| panic!("startup sequence: {error:?}"));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Assert every rollback entry was observed reaped.
pub fn assert_all_reaped(rollback: &[ReapedCandidate]) {
    for entry in rollback {
        assert!(
            entry.reaped,
            "rollback did not reap candidate {}",
            entry.plugin_id.as_str()
        );
    }
}

/// Unwrap a started result into its supervisor (panics on failure with context).
pub fn expect_supervisor(result: PersistentStartupResult) -> PersistentSupervisor {
    match result {
        PersistentStartupResult::Started { supervisor, .. } => supervisor,
        other @ PersistentStartupResult::Failed(_) => panic!("expected started, got {other:?}"),
    }
}

/// Read the pid the fixture recorded for one candidate.
pub fn candidate_pid(scene: &Scene, plugin_id: &str) -> u32 {
    let text = std::fs::read_to_string(scene.record_dir.join(format!("{plugin_id}.pid")))
        .unwrap_or_else(|err| panic!("pid file for {plugin_id}: {err:?}"));
    text.trim()
        .parse::<u32>()
        .unwrap_or_else(|err| panic!("pid parse for {plugin_id}: {err:?}"))
}

/// Poll `probe` until it returns `true` or `deadline` elapses. Keeps tests
/// deterministic without unbounded waits.
pub fn wait_until(deadline: Instant, mut probe: impl FnMut() -> bool) -> bool {
    loop {
        if probe() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// On Unix, return true when `pid` is no longer alive. `kill -0` exits zero
/// only while the process exists. Test-only; no production shell use.
#[cfg(unix)]
pub fn process_is_gone(pid: u32) -> bool {
    let status = std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    !matches!(status, Ok(s) if s.success())
}

#[cfg(windows)]
pub fn process_is_gone(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let output = std::process::Command::new("tasklist")
        .args(["/FI", filter.as_str(), "/FO", "CSV", "/NH"])
        .output()
        .unwrap_or_else(|err| panic!("query process {pid}: {err:?}"));
    assert!(
        output.status.success(),
        "tasklist failed while querying process {pid}: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pid_text = pid.to_string();
    let running = String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        line.split(',')
            .nth(1)
            .is_some_and(|field| field.trim_matches('"') == pid_text)
    });
    !running
}

/// How many of these tests may own live provider process trees at once.
///
/// Each test here spawns a real fixture process, and several deliberately hold
/// hanging descendants. Cargo runs the whole file in parallel, so without a
/// bound roughly twenty instrumented process trees exist simultaneously. Two at
/// a time bounds coverage resource use while preserving enough concurrency to
/// expose ordering bugs.
const MAX_CONCURRENT_PROCESS_TREES: usize = 2;

/// A permit to own live provider processes for the duration of one test.
///
/// Held for the whole test body, including the reap assertions, because the
/// processes are only certainly gone once those have run.
pub struct ProcessBudget {
    _permit: std::sync::MutexGuard<'static, ()>,
}

/// Wait for a slot before starting provider processes.
///
/// Poisoning is recovered rather than propagated: a panicking test leaves the
/// slot marked poisoned, and refusing every later test because an earlier one
/// failed would turn one failure into a whole-file failure.
#[must_use]
pub fn process_budget() -> ProcessBudget {
    use std::sync::{Mutex, OnceLock};

    static SLOTS: OnceLock<Vec<Mutex<()>>> = OnceLock::new();
    let slots = SLOTS.get_or_init(|| {
        (0..MAX_CONCURRENT_PROCESS_TREES)
            .map(|_| Mutex::new(()))
            .collect()
    });
    loop {
        for slot in slots {
            match slot.try_lock() {
                Ok(permit) => return ProcessBudget { _permit: permit },
                Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                    return ProcessBudget {
                        _permit: poisoned.into_inner(),
                    };
                }
                Err(std::sync::TryLockError::WouldBlock) => {}
            }
        }
        // Every slot is busy; yield rather than spin hot.
        std::thread::sleep(Duration::from_millis(25));
    }
}
