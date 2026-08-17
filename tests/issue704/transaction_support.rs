//! Shared scene, staging, and process helpers for the issue #704 S2
//! required-provider transaction tests.
//!
//! The transaction tests drive the real `jefe-provider-fixture` through the
//! full `PublishedWorkbench` → `run_provider_transaction` path. Everything
//! here stages physical packages under a temp directory, copies the fixture
//! binary (with the platform's executable name and permissions) plus its
//! test-only control sidecar, and observes recorded process evidence — so the
//! tests themselves stay focused on the transaction contract.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jefe::published_workbench::PublishedWorkbench;
use jefe::runtime::provider::environment::HostEnv;
use jefe::runtime::provider::persistent::ReapedCandidate;
use jefe::runtime::provider::supervisor::SupervisorBounds;
use jefe::startup_transaction::{
    ProviderTransactionFailure, ProviderTransactionResult, run_provider_transaction,
};

use super::support::{
    PackageSpec, build, config_root, host_binaries, plugins_root, provider_exe_name,
    publish_settings, resolve_paths, scan_roots, stage,
};

/// The cross-platform provider fixture binary, baked at compile time.
pub const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-provider-fixture");

/// Bounds tuned for fast, deterministic tests while preserving the staged
/// shutdown order.
pub fn fast_bounds() -> SupervisorBounds {
    SupervisorBounds {
        handshake: Duration::from_secs(3),
        invocation: Duration::from_secs(5),
        shutdown_ack: Duration::from_secs(1),
        stdin_close: Duration::from_secs(1),
        final_drain: Duration::from_secs(1),
    }
}

/// A host environment that resolves nothing (no secrets to resolve).
pub struct EmptyEnv;

impl HostEnv for EmptyEnv {
    fn get(&self, _name: &str) -> Option<String> {
        None
    }
}

/// Settings text enabling a set of owners.
pub fn settings_for(ids: &[&str]) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("settings_schema = 2\n");
    for id in ids {
        s.push('\n');
        writeln!(s, "[plugins.\"{id}\"]").unwrap_or_else(|e| panic!("write error: {e}"));
        writeln!(s, "enabled = true").unwrap_or_else(|e| panic!("write error: {e}"));
    }
    s
}

// ---------------------------------------------------------------------------
// Process budget — limits concurrent process trees in the test binary.
// ---------------------------------------------------------------------------

const MAX_CONCURRENT: usize = 2;

pub struct ProcessBudget {
    _permit: std::sync::MutexGuard<'static, ()>,
}

#[must_use]
pub fn process_budget() -> ProcessBudget {
    use std::sync::{Mutex, OnceLock};
    static SLOTS: OnceLock<Vec<Mutex<()>>> = OnceLock::new();
    let slots = SLOTS.get_or_init(|| (0..MAX_CONCURRENT).map(|_| Mutex::new(())).collect());
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
        std::thread::sleep(Duration::from_millis(25));
    }
}

// ---------------------------------------------------------------------------
// Cross-platform staging: fixture executable copy + control sidecar.
// ---------------------------------------------------------------------------

/// Copy the fixture executable into a staged package's `bin` directory under
/// the platform's executable name, mark it executable where that is a
/// distinct permission, and write the `<executable>.control` sidecar the
/// fixture reads when the composition spawns it with no argv. `entries` are
/// `key=value` control lines (`mode`, `record_dir`, `spawn_marker`).
pub fn stage_fixture_executable(bin_dir: &Path, entries: &[(&str, String)]) {
    use std::fmt::Write as _;
    fs::create_dir_all(bin_dir).unwrap_or_else(|e| panic!("bin dir: {e:?}"));
    let exe_name = provider_exe_name();
    let exe_path = bin_dir.join(exe_name);
    fs::copy(FIXTURE, &exe_path).unwrap_or_else(|e| panic!("stage fixture copy: {e:?}"));
    set_permissions_mode(&exe_path, 0o755);
    let mut control = String::new();
    for (key, value) in entries {
        writeln!(control, "{key}={value}").unwrap_or_else(|e| panic!("control line: {e}"));
    }
    fs::write(bin_dir.join(format!("{exe_name}.control")), control)
        .unwrap_or_else(|e| panic!("control sidecar: {e:?}"));
}

/// Stage a file that satisfies every metadata/executability preparation
/// check but cannot be loaded by the OS, so the genuine spawn itself fails.
///
/// On Unix the file is a script whose interpreter path does not exist: exec
/// fails with ENOENT, which — unlike the ENOEXEC of garbage bytes — has no
/// execvp/shell fallback that could accidentally run it. On Windows the file
/// is not a valid PE image, which CreateProcess rejects outright.
pub fn stage_invalid_executable(bin_dir: &Path) {
    fs::create_dir_all(bin_dir).unwrap_or_else(|e| panic!("bin dir: {e:?}"));
    let exe_path = bin_dir.join(provider_exe_name());
    #[cfg(unix)]
    fs::write(&exe_path, b"#!/nonexistent/jefe/unloadable-interpreter\n")
        .unwrap_or_else(|e| panic!("stage invalid executable: {e:?}"));
    #[cfg(windows)]
    fs::write(
        &exe_path,
        b"this is not a valid portable executable image\n",
    )
    .unwrap_or_else(|e| panic!("stage invalid executable: {e:?}"));
    set_permissions_mode(&exe_path, 0o755);
}

/// Set the exact permission mode where the platform has a distinct
/// executable bit; a no-op elsewhere.
pub fn set_permissions_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(mode);
        fs::set_permissions(path, perms)
            .unwrap_or_else(|e| panic!("chmod {}: {e}", path.display()));
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
}

/// On Unix, return true when `pid` is no longer alive (`kill -0` fails). On
/// Windows, return true when `tasklist` no longer lists `pid`. Mirrors the
/// cross-platform helper in `tests/issue390/persistent_support.rs`.
pub fn process_is_gone(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let status = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        !matches!(status.map(|s| s.success()), Ok(true))
    }
    #[cfg(windows)]
    {
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
}

/// Read the pid the fixture recorded for one candidate.
pub fn read_pid(record_dir: &Path, plugin_id: &str) -> u32 {
    let text = fs::read_to_string(record_dir.join(format!("{plugin_id}.pid")))
        .unwrap_or_else(|e| panic!("pid file for {plugin_id}: {e:?}"));
    text.trim()
        .parse::<u32>()
        .unwrap_or_else(|e| panic!("pid parse for {plugin_id}: {e:?}"))
}

/// Read the deterministic startup-sequence file the fixtures wrote.
pub fn startup_sequence(record_dir: &Path) -> Vec<String> {
    let text = fs::read_to_string(record_dir.join("startup-sequence.txt"))
        .unwrap_or_else(|e| panic!("startup sequence: {e:?}"));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Assert no fixture ever recorded a spawn: neither the shared
/// startup-sequence file nor any per-candidate pid file exists.
pub fn assert_nothing_spawned(scene: &Scene) {
    assert!(
        !scene.record_dir.join("startup-sequence.txt").exists(),
        "no provider may spawn before every required candidate is prepared"
    );
    for entry in fs::read_dir(&scene.record_dir).unwrap_or_else(|e| panic!("records: {e:?}")) {
        let entry = entry.unwrap_or_else(|e| panic!("records entry: {e:?}"));
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.ends_with(".pid"),
            "no pid file may exist before every required candidate is prepared: {name}"
        );
    }
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

/// A self-cleaning scene owning the temp config, record, and containment dirs.
pub struct Scene {
    _temp: tempfile::TempDir,
    pub config: PathBuf,
    pub record_dir: PathBuf,
    pub containment_base: PathBuf,
}

impl Scene {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
        let config = config_root(temp.path());
        fs::create_dir_all(&config).unwrap_or_else(|e| panic!("config dir: {e:?}"));
        let record_dir = temp.path().join("records");
        fs::create_dir_all(&record_dir).unwrap_or_else(|e| panic!("records: {e:?}"));
        let containment_base = temp.path().join("containment");
        Self {
            _temp: temp,
            config,
            record_dir,
            containment_base,
        }
    }

    pub fn plugins_root(&self) -> PathBuf {
        plugins_root(&self.config)
    }

    /// Stage a required persistent provider (actions declared) as a fixture
    /// executable copy whose control sidecar selects the given fixture mode
    /// and the scene's record directory.
    pub fn stage_required(&self, id: &'static str, fixture_mode: &str) -> PathBuf {
        let spec = PackageSpec::persistent_actions(id);
        let dir = stage(&self.plugins_root(), &spec, &host_binaries());
        stage_fixture_executable(
            &dir.join("bin"),
            &[
                ("mode", fixture_mode.to_owned()),
                ("record_dir", self.record_dir_text()),
            ],
        );
        dir
    }

    /// Stage a one-shot provider whose staged fixture touches a marker the
    /// instant it starts (fail-if-spawned trap).
    pub fn stage_one_shot(&self, id: &'static str) -> PathBuf {
        let spec = PackageSpec::one_shot(id);
        let dir = stage(&self.plugins_root(), &spec, &host_binaries());
        stage_fixture_executable(&dir.join("bin"), &self.trap_control(id));
        dir
    }

    /// Stage a declaration-empty persistent provider with the same
    /// fail-if-spawned trap.
    pub fn stage_declaration_empty(&self, id: &'static str) -> PathBuf {
        let spec = PackageSpec::declaration_empty(id);
        let dir = stage(&self.plugins_root(), &spec, &host_binaries());
        stage_fixture_executable(&dir.join("bin"), &self.trap_control(id));
        dir
    }

    /// Stage a required persistent provider whose manifest declares a
    /// secret-reference config field defaulting to `secret_ref`; the binary
    /// itself is a healthy fixture. An empty host environment cannot resolve
    /// the reference, which is exactly the environment-resolution defect.
    pub fn stage_secret_ref_required(&self, id: &'static str, secret_ref: &'static str) -> PathBuf {
        let spec = PackageSpec::persistent_actions_secret(id, secret_ref);
        let dir = stage(&self.plugins_root(), &spec, &host_binaries());
        stage_fixture_executable(
            &dir.join("bin"),
            &[
                ("mode", "persistent-ready".to_owned()),
                ("record_dir", self.record_dir_text()),
            ],
        );
        dir
    }

    /// Control entries for a provider that must never spawn: if the host
    /// erroneously starts it, the fixture records `{id}.spawned` immediately
    /// and would complete the full handshake, making the regression visible.
    fn trap_control(&self, id: &'static str) -> Vec<(&'static str, String)> {
        vec![
            ("mode", "persistent-ready".to_owned()),
            ("record_dir", self.record_dir_text()),
            ("spawn_marker", {
                let marker = self.record_dir.join(format!("{id}.spawned"));
                marker.to_string_lossy().into_owned()
            }),
        ]
    }

    /// Stage a required persistent provider whose binary does not exist.
    /// The manifest points at the provider path but no file is created there.
    pub fn stage_missing_binary(&self, id: &'static str) -> PathBuf {
        let spec = PackageSpec::persistent_actions(id);
        stage(&self.plugins_root(), &spec, &host_binaries())
    }

    /// Stage a required persistent provider whose selected binary path is a
    /// directory, so the path exists but is not a regular file.
    pub fn stage_directory_binary(&self, id: &'static str) -> PathBuf {
        let dir = self.stage_missing_binary(id);
        let bin_dir = dir.join("bin");
        fs::create_dir_all(bin_dir.join(provider_exe_name()))
            .unwrap_or_else(|e| panic!("directory binary: {e:?}"));
        dir
    }

    /// Stage a required persistent provider whose binary exists and is a
    /// regular file but carries no executable permission bit. Unix only:
    /// Windows has no distinct executable permission to withhold.
    #[cfg(unix)]
    pub fn stage_non_executable_binary(&self, id: &'static str) -> PathBuf {
        let dir = self.stage_missing_binary(id);
        let bin_dir = dir.join("bin");
        stage_fixture_executable(&bin_dir, &[]);
        set_permissions_mode(&bin_dir.join(provider_exe_name()), 0o644);
        dir
    }

    /// Stage a required persistent provider whose binary passes every
    /// metadata/executability preparation check but cannot be loaded by the
    /// OS, so it fails at spawn — the genuine post-preflight spawn defect.
    pub fn stage_unloadable_binary(&self, id: &'static str) -> PathBuf {
        let dir = self.stage_missing_binary(id);
        stage_invalid_executable(&dir.join("bin"));
        dir
    }

    /// Scan the plugins root, publish settings for the given owners, and build
    /// the workbench candidate.
    pub fn build_workbench(&self, ids: &[&str]) -> PublishedWorkbench {
        let inventory = scan_roots(&[self.plugins_root()]);
        let settings = publish_settings(&inventory, &settings_for(ids));
        let paths = resolve_paths(&self.config);
        build(&paths, &inventory, &settings, &self.containment_base)
            .unwrap_or_else(|e| panic!("workbench must build: {e}"))
    }

    /// Run the provider transaction with fast bounds and an empty host env.
    pub fn run_transaction(
        &self,
        workbench: &PublishedWorkbench,
    ) -> Result<ProviderTransactionResult, ProviderTransactionFailure> {
        let _ = self;
        run_provider_transaction(workbench, &fast_bounds(), &EmptyEnv)
    }

    /// The scene's record directory as control-file text.
    fn record_dir_text(&self) -> String {
        self.record_dir.to_string_lossy().into_owned()
    }
}
