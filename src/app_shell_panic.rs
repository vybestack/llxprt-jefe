//! Executor-owned delivery of process-global panic reports.

use crate::app_input::AppStateHandle;

pub fn drain_into_errors(app_state: &mut AppStateHandle) -> bool {
    let reports = crate::panic_capture::drain_panic_reports();
    if reports.is_empty() {
        return false;
    }
    let mut state = app_state.write();
    for report in reports {
        jefe::state::transition::commit_pure_site(
            &mut state,
            jefe::messages::AppMessage::Errors(report.into_errors_message()),
        );
    }
    drop(state);
    true
}
