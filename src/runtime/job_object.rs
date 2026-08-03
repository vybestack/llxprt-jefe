//! Windows Job Object containment boundary (issue #467 Slice 3).
//!
//! Native Windows psmux panes run a staged host copy of the Jefe image as the
//! long-lived launcher. The host creates and owns a kill-on-close Job Object
//! before spawning the agent worker and holds the handle for the worker's whole
//! lifetime, so a host crash/exit closes the handle and the kernel terminates
//! the owned descendant tree. The dashboard never owns this handle, so a
//! dashboard quit/crash leaves the host and worker alive (AC3); only host death
//! reaps the tree (AC6).
//!
//! This module is the narrow boundary that owns every `win32job` call so the
//! rest of the runtime depends only on the typed `JobContainment` /
//! `JobObjectError` contract. The crate forbids `unsafe` in Jefe source; all
//! Win32 interaction stays inside the safe `win32job` wrapper.
//!
//! Containment is the *mechanism*, not the ownership model. It answers "what
//! happens to the worker when the host dies", and issue #542 supplied the
//! missing half — what makes the host die when *its* owner does. See
//! `dev-docs/standards/windows-session-ownership.md`; this module is the
//! anchor holder's release valve and its behaviour is unchanged by #542.

#![cfg(windows)]

use std::io;

use win32job::Job;

/// Typed failure for Job Object create/query/configure/assign operations.
///
/// Each variant names the failing operation so callers can surface an actionable
/// diagnostic without leaking raw Win32 HANDLE values.
#[derive(Debug)]
pub enum JobObjectError {
    /// `CreateJobObjectW` failed.
    Create(io::Error),
    /// `QueryInformationJobObject` failed.
    Query(io::Error),
    /// `SetInformationJobObject` (kill-on-close configuration) failed.
    Configure(io::Error),
    /// `AssignProcessToJobObject` failed.
    Assign(io::Error),
}

impl From<win32job::JobError> for JobObjectError {
    fn from(error: win32job::JobError) -> Self {
        match error {
            win32job::JobError::CreateFailed(inner) => Self::Create(inner),
            win32job::JobError::GetInfoFailed(inner) => Self::Query(inner),
            win32job::JobError::SetInfoFailed(inner) => Self::Configure(inner),
            win32job::JobError::AssignFailed(inner) => Self::Assign(inner),
            // `JobError` is non_exhaustive; future variants are still job-setup
            // failures and surface as a configuration error so callers always see
            // a typed, actionable diagnostic.
            other => Self::Configure(io::Error::other(other.to_string())),
        }
    }
}

impl std::fmt::Display for JobObjectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create(_) => formatter.write_str("failed to create windows job object"),
            Self::Query(_) => formatter.write_str("failed to query windows job object limits"),
            Self::Configure(_) => {
                formatter.write_str("failed to configure windows job object kill-on-close")
            }
            Self::Assign(_) => {
                formatter.write_str("failed to assign process to windows job object")
            }
        }
    }
}

impl std::error::Error for JobObjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Create(inner)
            | Self::Query(inner)
            | Self::Configure(inner)
            | Self::Assign(inner) => Some(inner),
        }
    }
}

/// Owns a kill-on-close Windows Job Object handle for the lifetime of a host
/// process tree.
///
/// Dropping the guard closes the underlying Job handle. When
/// `KILL_ON_JOB_CLOSE` is configured, the kernel terminates every process still
/// assigned to the Job at that point, which is the containment guarantee relied
/// on by the private pane-host entrypoint.
pub struct JobContainment {
    // Held solely for its Drop side-effect: dropping the Job closes the kernel
    // handle and, with KILL_ON_JOB_CLOSE set, terminates the contained tree.
    #[allow(dead_code)]
    job: Job,
    // Written on construction; read by `is_kill_on_job_close_active`, which is a
    // public contract asserted by the test suite (the lib path trusts the flag
    // unconditionally once construction succeeds).
    kill_on_job_close_active: bool,
}

impl JobContainment {
    /// Create and configure a kill-on-close Job, then assign the current
    /// process so spawned descendants inherit containment.
    ///
    /// This is the production entrypoint used by the private pane host: the
    /// returned guard must outlive every spawned worker so the host process
    /// owns the Job handle for the full worker lifetime.
    pub fn enable_for_current_process() -> Result<Self, JobObjectError> {
        let job = Self::create_configured()?;
        job.assign_current_process().map_err(JobObjectError::from)?;
        Ok(Self {
            kill_on_job_close_active: true,
            job,
        })
    }

    /// Create and configure a kill-on-close Job, then assign the process behind
    /// the supplied raw handle.
    ///
    /// Test-only helper used to contain a spawned child without ever assigning
    /// the test runner, so dropping the guard cannot kill the test process. The
    /// caller retains ownership of the supplied handle.
    #[cfg(test)]
    pub(super) fn contain_handle(raw_handle: isize) -> Result<Self, JobObjectError> {
        let job = Self::create_configured()?;
        job.assign_process(raw_handle)
            .map_err(JobObjectError::from)?;
        Ok(Self {
            kill_on_job_close_active: true,
            job,
        })
    }

    fn create_configured() -> Result<Job, JobObjectError> {
        let job = Job::create().map_err(JobObjectError::from)?;
        let mut info = job
            .query_extended_limit_info()
            .map_err(JobObjectError::from)?;
        info.limit_kill_on_job_close();
        job.set_extended_limit_info(&info)
            .map_err(JobObjectError::from)?;
        Ok(job)
    }

    /// Reports whether `KILL_ON_JOB_CLOSE` is active on the owned Job.
    #[must_use]
    #[allow(dead_code)]
    pub fn is_kill_on_job_close_active(&self) -> bool {
        self.kill_on_job_close_active
    }
}
