//! Target-owned remote candidate resolution and definition probe.
//!
//! Remote executable evidence is captured exclusively through the audited SSH
//! boundary. No local PATH or local executable fingerprint participates.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::agent_candidate::{PackageRunnerKind, VersionSelector};
use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::agent_candidate_path::AgentWrapperKind;
use crate::domain::RemoteRepositorySettings;
use crate::domain::agent_definition::probe::ProbeStream;
use crate::domain::agent_definition::{AgentDefinition, Availability, CandidateKind};
use crate::ssh::{SSH_OPERATION_TIMEOUT, SshMode, SshPlan};

use super::RuntimeError;
use super::agent_probe_parse::parse_identity;
use super::agent_remote_plan::posix_single_quote;

const RESOLVED_SENTINEL: &str = "JEFE_REMOTE_CANDIDATE_V1";
const MISSING_SENTINEL: &str = "JEFE_REMOTE_CANDIDATE_MISSING";

/// Remote executable, package prefix, and probe evidence captured on target.
pub struct RemoteProbeEvidence {
    pub executable: PathBuf,
    pub executable_fingerprint: CandidateFingerprint,
    pub executable_wrapper: AgentWrapperKind,
    pub argv_prefix: Vec<OsString>,
    pub availability: Availability,
}

#[derive(Clone)]
struct CandidateSpec {
    lookup: CandidateLookup,
    argv_prefix: Vec<OsString>,
}

#[derive(Clone)]
enum CandidateLookup {
    PathName(String),
    Repository(PathBuf),
}

struct ResolvedRemoteCandidate {
    spec: CandidateSpec,
    executable: PathBuf,
    fingerprint: CandidateFingerprint,
}

/// Resolve and probe the selected candidate on the remote target.
pub fn run_remote_agent_probe(
    definition: &AgentDefinition,
    selector: &VersionSelector,
    settings: &RemoteRepositorySettings,
    work_dir: &Path,
    generation: u64,
) -> Result<RemoteProbeEvidence, RuntimeError> {
    let specs = candidate_specs(definition, selector, work_dir)?;
    let mut selected = None;
    for spec in specs {
        if let Some(resolved) = resolve_candidate(settings, spec)? {
            selected = Some(resolved);
            break;
        }
    }
    let selected = selected.ok_or_else(|| {
        RuntimeError::SpawnFailed(
            "configured agent executable was not found on remote target".into(),
        )
    })?;
    let identity = run_probe_command(
        settings,
        &selected,
        &definition.probe.argv,
        definition.probe.stream,
    )
    .and_then(|bytes| {
        parse_identity(&bytes, &definition.probe)
            .map_err(|_| RuntimeError::SpawnFailed("remote identity probe was malformed".into()))
    })?;
    let availability = Availability::InstalledCompatible {
        identity,
        generation,
    };
    let recaptured = resolve_candidate(settings, selected.spec.clone())?.ok_or_else(|| {
        RuntimeError::SpawnFailed("remote executable disappeared after probe".into())
    })?;
    if recaptured.executable != selected.executable
        || recaptured.fingerprint != selected.fingerprint
    {
        return Err(RuntimeError::SpawnFailed(
            "AGT-E203: remote executable fingerprint changed during probe".into(),
        ));
    }
    Ok(RemoteProbeEvidence {
        executable: selected.executable,
        executable_fingerprint: selected.fingerprint,
        executable_wrapper: AgentWrapperKind::Direct,
        argv_prefix: selected.spec.argv_prefix,
        availability,
    })
}

fn candidate_specs(
    definition: &AgentDefinition,
    selector: &VersionSelector,
    work_dir: &Path,
) -> Result<Vec<CandidateSpec>, RuntimeError> {
    let mut specs = Vec::new();
    for candidate in &definition.candidates {
        let spec = match &candidate.kind {
            CandidateKind::PathName { name } if selector.is_direct() => Some(CandidateSpec {
                lookup: CandidateLookup::PathName(name.clone()),
                argv_prefix: Vec::new(),
            }),
            CandidateKind::RepositoryLlxprt if selector.is_direct() => Some(CandidateSpec {
                lookup: CandidateLookup::Repository(remote_repository_path(
                    work_dir,
                    &candidate.value,
                )),
                argv_prefix: Vec::new(),
            }),
            CandidateKind::NpmPackage { package, binary } if !selector.is_direct() => {
                let spec = selector
                    .package_spec(PackageRunnerKind::Npm, package)
                    .ok_or_else(|| {
                        RuntimeError::SpawnFailed("invalid npm package selector".into())
                    })?;
                Some(CandidateSpec {
                    lookup: CandidateLookup::PathName("npm".into()),
                    argv_prefix: vec![
                        "exec".into(),
                        "--yes".into(),
                        format!("--package={spec}").into(),
                        "--".into(),
                        binary.into(),
                    ],
                })
            }
            CandidateKind::UvxPackage { package, binary } if !selector.is_direct() => {
                let spec = selector
                    .package_spec(PackageRunnerKind::Uvx, package)
                    .ok_or_else(|| {
                        RuntimeError::SpawnFailed("invalid uvx package selector".into())
                    })?;
                Some(CandidateSpec {
                    lookup: CandidateLookup::PathName("uvx".into()),
                    argv_prefix: vec!["--from".into(), spec.into(), binary.into()],
                })
            }
            _ => None,
        };
        if let Some(spec) = spec {
            specs.push(spec);
        }
    }
    Ok(specs)
}

fn remote_repository_path(work_dir: &Path, candidate: &Path) -> PathBuf {
    let joined = format!(
        "{}/{}",
        work_dir.to_string_lossy().trim_end_matches('/'),
        candidate.to_string_lossy().trim_start_matches('/')
    );
    PathBuf::from(crate::domain::canonical_values::normalize_remote_path(
        &joined,
    ))
}

fn resolve_candidate(
    settings: &RemoteRepositorySettings,
    spec: CandidateSpec,
) -> Result<Option<ResolvedRemoteCandidate>, RuntimeError> {
    let lookup = match &spec.lookup {
        CandidateLookup::PathName(name) => format!(
            "p=$(command -v {} 2>/dev/null) || {{ printf '%s\\n' {}; exit 0; }}",
            quote(name)?,
            quote(MISSING_SENTINEL)?
        ),
        CandidateLookup::Repository(path) => format!(
            "p={}; [ -x \"$p\" ] || {{ printf '%s\\n' {}; exit 0; }}",
            quote_os(path)?,
            quote(MISSING_SENTINEL)?
        ),
    };
    let script = format!(
        "{lookup}; p=$(readlink -f \"$p\" 2>/dev/null || realpath \"$p\" 2>/dev/null) || exit 1; \
         meta=$(stat -Lc '%d\\t%i\\t%s\\t%Y' \"$p\" 2>/dev/null || stat -f '%d\\t%i\\t%z\\t%m' \"$p\" 2>/dev/null) || exit 1; \
         printf '%s\\n%s\\n%s\\n' {} \"$p\" \"$meta\"",
        quote(RESOLVED_SENTINEL)?
    );
    let output = execute(settings, &wrap_effective_user(settings, &script)?)?;
    parse_resolved_candidate(&output.stdout, spec)
}

fn parse_resolved_candidate(
    stdout: &[u8],
    spec: CandidateSpec,
) -> Result<Option<ResolvedRemoteCandidate>, RuntimeError> {
    let stdout = std::str::from_utf8(stdout)
        .map_err(|_| RuntimeError::SpawnFailed("remote candidate response is not UTF-8".into()))?;
    if stdout.trim() == MISSING_SENTINEL {
        return Ok(None);
    }
    let mut lines = stdout.lines();
    if lines.next() != Some(RESOLVED_SENTINEL) {
        return Err(RuntimeError::SpawnFailed(
            "remote candidate response has invalid framing".into(),
        ));
    }
    let path = lines.next().ok_or_else(|| {
        RuntimeError::SpawnFailed("remote candidate response omitted executable path".into())
    })?;
    let metadata = lines.next().ok_or_else(|| {
        RuntimeError::SpawnFailed("remote candidate response omitted metadata".into())
    })?;
    if lines.next().is_some() || !path.starts_with('/') {
        return Err(RuntimeError::SpawnFailed(
            "remote candidate response contains invalid executable path".into(),
        ));
    }
    let fields = metadata.split('\t').collect::<Vec<_>>();
    let [dev, ino, size, mtime] = fields.as_slice() else {
        return Err(RuntimeError::SpawnFailed(
            "remote candidate response contains invalid metadata".into(),
        ));
    };
    let parse_u64 = |value: &str| {
        value.parse::<u64>().map_err(|_| {
            RuntimeError::SpawnFailed("remote candidate metadata is not numeric".into())
        })
    };
    let mtime = mtime
        .parse::<i64>()
        .map_err(|_| RuntimeError::SpawnFailed("remote candidate mtime is not numeric".into()))?;
    let executable = PathBuf::from(path);
    let fingerprint = CandidateFingerprint::new(
        executable.clone(),
        Some(parse_u64(dev)?),
        Some(parse_u64(ino)?),
        parse_u64(size)?,
        mtime,
    );
    Ok(Some(ResolvedRemoteCandidate {
        spec,
        executable,
        fingerprint,
    }))
}

fn run_probe_command(
    settings: &RemoteRepositorySettings,
    candidate: &ResolvedRemoteCandidate,
    argv: &[String],
    stream: ProbeStream,
) -> Result<Vec<u8>, RuntimeError> {
    let mut elements = Vec::with_capacity(candidate.spec.argv_prefix.len() + argv.len() + 1);
    elements.push(quote_os(&candidate.executable)?);
    for argument in &candidate.spec.argv_prefix {
        elements.push(quote_os(Path::new(argument))?);
    }
    for argument in argv {
        elements.push(quote(argument)?);
    }
    let process = elements.join(" ");
    let selected = match stream {
        ProbeStream::Stdout => format!("({process}) 2>/dev/null"),
        ProbeStream::Stderr => format!("({process} 2>&1 1>/dev/null)"),
        ProbeStream::Combined => format!("({process}) 2>&1"),
    };
    let command = wrap_effective_user(settings, &selected)?;
    let output = execute(settings, &command)?;
    Ok(output.stdout)
}

fn execute(
    settings: &RemoteRepositorySettings,
    command: &str,
) -> Result<std::process::Output, RuntimeError> {
    let plan = SshPlan::new(settings, command, SshMode::NonInteractive)
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    let output = plan
        .execute(None, SSH_OPERATION_TIMEOUT, None)
        .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(RuntimeError::SpawnFailed(
            crate::ssh::classify_failure(
                output.status.code(),
                &String::from_utf8_lossy(&output.stderr),
            )
            .to_string(),
        ))
    }
}

fn wrap_effective_user(
    settings: &RemoteRepositorySettings,
    inner: &str,
) -> Result<String, RuntimeError> {
    let login = settings.login_user.trim();
    let effective = settings.run_as_user.trim();
    if effective.is_empty() || effective == login {
        return Ok(inner.to_owned());
    }
    Ok(format!(
        "sudo -n su - {} -c {}",
        quote(effective)?,
        quote(inner)?
    ))
}

fn quote(value: &str) -> Result<String, RuntimeError> {
    posix_single_quote(value).map_err(|error| RuntimeError::SpawnFailed(error.to_string()))
}

fn quote_os(path: &Path) -> Result<String, RuntimeError> {
    let value = path.to_str().ok_or_else(|| {
        RuntimeError::SpawnFailed("remote executable path contains non-UTF-8 bytes".into())
    })?;
    quote(value)
}
