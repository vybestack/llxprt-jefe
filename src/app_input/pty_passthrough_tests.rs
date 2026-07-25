//! Ctrl-C passthrough when the AppContext mutex is contended (issue #333).

use std::sync::{Arc, Mutex, PoisonError};

use crate::AppContext;
use jefe::domain::AgentId;
use jefe::github::GhClient;
use jefe::persistence::FilePersistenceManager;
use jefe::runtime::TmuxRuntimeManager;
use jefe::services::capture_worker::CaptureHandle;
use jefe::services::persist_worker::PersistHandle;
use jefe::theme::FileThemeManager;

use super::pty_passthrough::{
    AttachedTerminalProbe, CtxArc, attached_terminal_probe_from_lock,
    ctrl_c_passthrough_may_forward, probe_attached_terminal,
};

fn minimal_test_ctx() -> CtxArc {
    Arc::new(Mutex::new(AppContext {
        persistence: FilePersistenceManager::default(),
        theme_manager: FileThemeManager::default(),
        runtime: TmuxRuntimeManager::new(24, 80),
        gh_client: GhClient::new(),
        gh_deliveries: super::GhDeliveryHandle::default(),
        persist_handle: PersistHandle::new(Arc::new(|_| Ok(()))),
        capture_handle: CaptureHandle::new(),
    }))
}

#[test]
fn attached_terminal_probe_from_lock_classifies_tri_state() {
    assert_eq!(
        attached_terminal_probe_from_lock(true, true),
        AttachedTerminalProbe::Attached
    );
    assert_eq!(
        attached_terminal_probe_from_lock(true, false),
        AttachedTerminalProbe::Absent
    );
    assert_eq!(
        attached_terminal_probe_from_lock(false, false),
        AttachedTerminalProbe::Busy
    );
    assert_eq!(
        attached_terminal_probe_from_lock(false, true),
        AttachedTerminalProbe::Busy
    );
}

#[test]
fn ctrl_c_passthrough_forwards_on_attached_or_busy_only() {
    assert!(ctrl_c_passthrough_may_forward(
        AttachedTerminalProbe::Attached
    ));
    assert!(ctrl_c_passthrough_may_forward(AttachedTerminalProbe::Busy));
    assert!(!ctrl_c_passthrough_may_forward(
        AttachedTerminalProbe::Absent
    ));
}

#[test]
fn probe_attached_terminal_reports_busy_when_ctx_mutex_held() {
    let ctx = minimal_test_ctx();
    let _guard = ctx.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        probe_attached_terminal(Some(&ctx)),
        AttachedTerminalProbe::Busy
    );
}

#[test]
fn probe_attached_terminal_reports_absent_when_unlocked_without_agent() {
    let ctx = minimal_test_ctx();
    assert_eq!(
        probe_attached_terminal(Some(&ctx)),
        AttachedTerminalProbe::Absent
    );
}

#[test]
fn probe_attached_terminal_reports_attached_when_unlocked_with_agent() {
    // The production probe only inspects `attached_agent()` after a successful
    // try_lock — no real AttachedViewer is required for the Attached path.
    let ctx = minimal_test_ctx();
    {
        let mut guard = ctx.lock().unwrap_or_else(PoisonError::into_inner);
        guard
            .runtime
            .set_attached_agent_id_for_test(Some(AgentId("agent-1".to_owned())));
    }
    assert_eq!(
        probe_attached_terminal(Some(&ctx)),
        AttachedTerminalProbe::Attached
    );
}

#[test]
fn ctrl_c_passthrough_gate_passes_under_mutex_contention() {
    let ctx = minimal_test_ctx();
    let _guard = ctx.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        ctrl_c_passthrough_may_forward(probe_attached_terminal(Some(&ctx))),
        "Ctrl-C must not be dropped when try_lock fails (issue #333)"
    );
}
