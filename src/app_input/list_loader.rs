//! Shared list-loading lifecycle service for app-input orchestration.
//!
//! Screen dispatchers own backend-specific parameter collection, GitHub calls,
//! and result conversion. This service owns the common deterministic start
//! lifecycle: monotonic request-id allocation plus reload/page pending setup.

use jefe::domain::{ListRequestId, PageToken};
use jefe::state::pagination::{BeginOutcome, PaginatedList};

/// The list operation a screen dispatcher wants to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ListLoad {
    /// Replace the current list and expose visible loading state.
    Reload,
    /// Replace the current list without exposing visible loading state
    /// (background refresh; preserves selection/scroll, no spinner flash).
    SilentReload,
    /// Append the page identified by the continuation token.
    Page(PageToken),
}

/// Shared coordinator for list request allocation and pending-state setup.
pub(super) struct ListLoader;

impl ListLoader {
    /// Allocate a monotonic id and begin the requested operation.
    ///
    /// Returns `None` when request-id space is exhausted or a page cannot
    /// start because the list is busy, exhausted, or has a different token.
    pub(super) fn begin<T, I: Clone>(
        list: &mut PaginatedList<T, I>,
        identity: I,
        load: ListLoad,
    ) -> Option<ListRequestId> {
        let request_id = list.next_request_id().ok()?;
        let outcome = match load {
            ListLoad::Reload => list.begin_reload(identity, request_id),
            ListLoad::SilentReload => list.begin_silent_reload(identity, request_id),
            ListLoad::Page(token) => list.begin_page(token, request_id),
        };
        matches!(outcome, BeginOutcome::Started).then_some(request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{ListLoad, ListLoader};
    use jefe::domain::PageToken;
    use jefe::state::pagination::{PaginatedList, ReloadResult};

    #[test]
    fn reload_allocates_real_request_id_and_marks_pending() {
        let mut list: PaginatedList<u32, String> = PaginatedList::default();

        let request_id = ListLoader::begin(&mut list, "scope".to_string(), ListLoad::Reload);

        assert_eq!(request_id.map(jefe::domain::ListRequestId::get), Some(1));
        assert!(list.has_pending_request());
        assert!(list.is_loading());
    }

    #[test]
    fn page_start_reuses_established_identity_and_token() {
        let mut list: PaginatedList<u32, String> = PaginatedList::default();
        let reload = ListLoader::begin(&mut list, "scope".to_string(), ListLoad::Reload);
        let Some(reload_id) = reload else {
            panic!("reload must start in test setup");
        };
        list.accept_loaded(ReloadResult {
            identity: "scope".to_string(),
            request_id: reload_id,
            items: vec![1],
            next_page: PageToken::PageNumber(2),
        });

        let page_id = ListLoader::begin(
            &mut list,
            "ignored".to_string(),
            ListLoad::Page(PageToken::PageNumber(2)),
        );

        assert_eq!(page_id.map(jefe::domain::ListRequestId::get), Some(2));
        assert!(list.has_pending_request());
    }

    #[test]
    fn silent_reload_marks_pending_without_visible_loading() {
        let mut list: PaginatedList<u32, String> = PaginatedList::default();

        let request_id = ListLoader::begin(&mut list, "scope".to_string(), ListLoad::SilentReload);

        assert_eq!(request_id.map(jefe::domain::ListRequestId::get), Some(1));
        assert!(list.has_pending_request());
        // A silent reload must not surface the visible loading flag.
        assert!(!list.is_loading());
    }
}
