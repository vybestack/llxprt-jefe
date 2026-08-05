//! Behavioral contracts for the Windows Job Object containment boundary
//! (issue #467 Slice 3).
//!
//! These tests prove the narrow `job_object` module owns safe `win32job` calls,
//! produces typed failures, configures `KILL_ON_JOB_CLOSE`, and that releasing a
//! kill-on-close Job handle terminates a contained descendant within a bounded
//! timeout. The native containment proof deliberately contains a *spawned
//! child* (never the test runner), so dropping the guard cannot kill the test
//! process. The host-owns-handle contract used by `run_launch_plan` relies on
//! the same kernel mechanism: process exit auto-closes owned handles.

#![cfg(windows)]

use std::os::windows::io::AsRawHandle;
use std::process::Command;
use std::time::{Duration, Instant};

use super::job_object::{JobContainment, JobObjectError};

/// A long-lived Windows child used as the contained descendant in the native
/// proof. `ping -n 60` sleeps for roughly a minute, far longer than the bounded
/// observation window, so it never exits on its own during the test.
fn spawn_long_lived_child() -> std::process::Child {
    Command::new("cmd.exe")
        .args(["/C", "ping -n 60 127.0.0.1 > nul"])
        .spawn()
        .unwrap_or_else(|error| panic!("spawn containment child: {error}"))
}

/// Poll `child.try_wait()` until it exits or `bound` elapses.
fn wait_for_exit(child: &mut std::process::Child, bound: Duration) -> bool {
    let deadline = Instant::now() + bound;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    child.try_wait().ok().flatten().is_some()
}

#[test]
fn enabling_containment_for_current_process_yields_kill_on_job_close_guard() {
    // Production path: the host assigns itself before spawning a worker so the
    // worker tree inherits containment. The returned guard owns the kill-on-close
    // Job handle.
    //
    // CAUTION: this test calls the real production entrypoint, which assigns the
    // current process (the test runner) to a kill-on-close Job. We must NOT let
    // the guard drop inside the test, or closing the handle would terminate the
    // test runner mid-suite. `mem::forget` leaks the handle for the remainder of
    // the process so the kernel only closes it (and reaps the empty tree) when
    // the test runner exits normally at end of process.
    let containment = JobContainment::enable_for_current_process()
        .unwrap_or_else(|error| panic!("enable containment: {error}"));

    let kill_on_close_active = containment.is_kill_on_job_close_active();
    std::mem::forget(containment);

    assert!(
        kill_on_close_active,
        "containment guard must report KILL_ON_JOB_CLOSE as active"
    );
}

#[test]
fn native_kill_on_job_close_terminates_a_contained_descendant_within_bound() {
    // Spawn the descendant first; only the spawned child is contained, never the
    // test runner. This isolates ownership so the assertion below cannot kill the
    // test process.
    let mut child = spawn_long_lived_child();
    // SAFETY (handle ownership): the child owns this handle; we only lend the
    // raw value to assign_process for the duration of the syscall. The Child
    // guard retains ownership and closes the handle when it drops.
    let raw_handle = child.as_raw_handle() as isize;

    let containment = JobContainment::contain_handle(raw_handle)
        .unwrap_or_else(|error| panic!("contain child: {error}"));

    assert!(
        containment.is_kill_on_job_close_active(),
        "containment guard must report KILL_ON_JOB_CLOSE as active for contained child"
    );
    assert!(
        child.try_wait().ok().flatten().is_none(),
        "child must be alive before the job handle is released"
    );

    // Releasing the guard closes the Job handle. With KILL_ON_JOB_CLOSE set the
    // kernel terminates every process still assigned to the Job.
    drop(containment);

    let exited_within_bound = wait_for_exit(&mut child, Duration::from_secs(5));
    assert!(
        exited_within_bound,
        "contained descendant must exit within the bounded timeout once the kill-on-close Job handle is released"
    );

    // Issue #664: the reap must stop at the Job's members. This process created
    // and owned the Job but never joined it, so releasing the handle terminated
    // the child and nothing else. Reaching this line at all is the observation,
    // and spawning again proves the owner is still a working process rather
    // than one the kernel has begun to tear down.
    let mut unrelated = spawn_long_lived_child();
    let owner_survived = unrelated.try_wait().ok().flatten().is_none();
    let _ = unrelated.kill();
    let _ = unrelated.wait();
    assert!(
        owner_survived,
        "releasing a kill-on-close Job handle must terminate only the processes assigned \
         to that Job. An owner that is also a member would be killed by its own guard, \
         which is the silent whole-tree death reported in issue #664."
    );
}

#[test]
fn job_object_error_variants_are_typed_and_name_the_failing_operation() {
    // The error type must name the failing operation (create/query/configure/
    // assign) so diagnostics stay actionable without leaking raw HANDLE values.
    let cases: [(JobObjectError, &str); 4] = [
        (
            JobObjectError::Create(std::io::Error::from_raw_os_error(5)),
            "create",
        ),
        (
            JobObjectError::Query(std::io::Error::from_raw_os_error(5)),
            "query",
        ),
        (
            JobObjectError::Configure(std::io::Error::from_raw_os_error(5)),
            "configure",
        ),
        (
            JobObjectError::Assign(std::io::Error::from_raw_os_error(5)),
            "assign",
        ),
    ];
    for (error, expected_token) in cases {
        let message = error.to_string();
        assert!(
            message.contains(expected_token),
            "JobObjectError message must name the operation '{expected_token}': got {message}"
        );
        assert!(
            !message.contains("0x"),
            "JobObjectError message must not leak raw handle values: got {message}"
        );
    }
}
