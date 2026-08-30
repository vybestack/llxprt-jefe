use super::provider_requests::ProviderConfirmationIdentity;
use super::types::{ConfirmFocus, IssueSelfAssignmentFollowUp};
use crate::domain::plugin::field::Field;
use crate::domain::{AgentId, AgentLaunchRequest, Id, RepositoryId, TypedMap, TypedValue};
use crate::github::SendPayload;
use crate::runtime::PreflightIssue;
use crate::workbench::descriptor::OverlayKind;

/// Exact host-confirmation request owned by one open screen instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationRequest {
    /// Delete one configured repository.
    DeleteRepository { id: RepositoryId },
    /// Delete one configured agent and optionally its working directory.
    DeleteAgent { id: AgentId, delete_work_dir: bool },
    /// Terminate one running agent.
    KillAgent { id: AgentId },
    /// Recover agents whose server connection was lost.
    ServerLostRecovery { agent_ids: Vec<AgentId> },
    /// Resolve one preflight issue before resuming launch.
    Preflight {
        agent_id: AgentId,
        signature: AgentLaunchRequest,
        issue: PreflightIssue,
        remaining_issues: Vec<PreflightIssue>,
        issue_self_assignment: Option<IssueSelfAssignmentFollowUp>,
    },
    /// Repair a dirty issue working copy before launch.
    IssueDirtyCopy {
        agent_id: AgentId,
        work_dir: std::path::PathBuf,
        signature: AgentLaunchRequest,
        payload: SendPayload,
    },
    /// Replace an issue working copy whose origin does not match.
    IssueOriginMismatch {
        agent_id: AgentId,
        work_dir: std::path::PathBuf,
        signature: AgentLaunchRequest,
        payload: SendPayload,
        actual: String,
        expected: String,
    },
}

/// Presentation state for one active declared host overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveOverlay {
    /// Keyboard-shortcut reference content.
    Help {
        /// First visible content row.
        viewport: usize,
    },
    /// Host text-query editor.
    Search {
        /// Current query text.
        query: String,
        /// Cursor byte offset in `query`.
        cursor: usize,
    },
    /// Host-owned confirmation request and presentation.
    GenericConfirmation {
        /// Exact operation awaiting the user's decision.
        request: Box<ConfirmationRequest>,
        /// Currently focused decision.
        focus: ConfirmFocus,
    },
    /// Provider-owned confirmation continuation and presentation.
    ProviderConfirmation {
        /// Currently focused decision.
        focus: ConfirmFocus,
        /// Complete host-authenticated provider token presented by this overlay.
        provider_confirmation: ProviderConfirmationIdentity,
        /// Ordered provider continuation fields available before the decision.
        continuation_fields: Vec<Id>,
        /// Exact typed values displayed for provider continuation fields.
        continuation_values: TypedMap,
        /// Provider continuation field currently accepting input, if any.
        focused_field: Option<Id>,
    },
}

impl ActiveOverlay {
    /// Closed declaration kind that authorized this live layer.
    #[must_use]
    pub const fn kind(&self) -> OverlayKind {
        match self {
            Self::Help { .. } => OverlayKind::Help,
            Self::Search { .. } => OverlayKind::Search,
            Self::GenericConfirmation { .. } | Self::ProviderConfirmation { .. } => {
                OverlayKind::Confirmation
            }
        }
    }
}

/// Overlay declarations and live presentation state owned by one screen instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenOverlayState {
    declared: Vec<OverlayKind>,
    active: Option<ActiveOverlay>,
}

fn initial_continuation_values(fields: &[Field]) -> TypedMap {
    fields
        .iter()
        .filter_map(|field| {
            field
                .default()
                .cloned()
                .map(|value| (field.id().clone(), value))
        })
        .collect()
}

impl ScreenOverlayState {
    /// Allocate empty live state from one validated descriptor declaration.
    #[must_use]
    pub fn new(declared: Vec<OverlayKind>) -> Self {
        Self {
            declared,
            active: None,
        }
    }

    /// Closed kinds this instance is permitted to open, in declaration order.
    #[must_use]
    pub fn declared(&self) -> &[OverlayKind] {
        &self.declared
    }

    /// Current live layer, if any.
    #[must_use]
    pub const fn active(&self) -> Option<&ActiveOverlay> {
        self.active.as_ref()
    }

    /// Open declared Help at its initial viewport.
    pub fn open_help(&mut self) -> bool {
        self.open(ActiveOverlay::Help { viewport: 0 })
    }

    /// Open declared Search with an empty query.
    pub fn open_search(&mut self) -> bool {
        self.open(ActiveOverlay::Search {
            query: String::new(),
            cursor: 0,
        })
    }

    /// Open a declared host Confirmation with the safe Cancel choice focused.
    pub fn open_generic_confirmation(&mut self, request: ConfirmationRequest) -> bool {
        self.open(ActiveOverlay::GenericConfirmation {
            request: Box::new(request),
            focus: ConfirmFocus::Cancel,
        })
    }

    /// The exact request presented by the active generic Confirmation overlay.
    #[must_use]
    pub fn generic_confirmation(&self) -> Option<&ConfirmationRequest> {
        match self.active.as_ref() {
            Some(ActiveOverlay::GenericConfirmation { request, .. }) => Some(request),
            _ => None,
        }
    }

    /// Open a declared provider Confirmation with one exact token and instance-owned draft.
    pub fn open_provider_confirmation(
        &mut self,
        provider_confirmation: ProviderConfirmationIdentity,
        fields: &[Field],
    ) -> bool {
        let values = initial_continuation_values(fields);
        self.open(ActiveOverlay::ProviderConfirmation {
            focus: ConfirmFocus::Cancel,
            provider_confirmation,
            continuation_fields: fields.iter().map(|field| field.id().clone()).collect(),
            continuation_values: values,
            focused_field: None,
        })
    }

    /// Complete provider token presented by the active Confirmation overlay.
    #[must_use]
    pub fn provider_confirmation(&self) -> Option<&ProviderConfirmationIdentity> {
        match self.active.as_ref() {
            Some(ActiveOverlay::ProviderConfirmation {
                provider_confirmation,
                ..
            }) => Some(provider_confirmation),
            _ => None,
        }
    }

    /// Provider-supplied id of the exact token presented by the active overlay.
    #[must_use]
    pub fn provider_confirmation_id(&self) -> Option<&Id> {
        self.provider_confirmation()
            .map(ProviderConfirmationIdentity::confirmation_id)
    }
    /// Current Confirmation decision focus, when Confirmation is active.
    #[must_use]
    pub const fn confirmation_focus(&self) -> Option<ConfirmFocus> {
        match self.active.as_ref() {
            Some(
                ActiveOverlay::GenericConfirmation { focus, .. }
                | ActiveOverlay::ProviderConfirmation { focus, .. },
            ) => Some(*focus),
            _ => None,
        }
    }

    /// Current provider continuation values, when provider Confirmation is active.
    #[must_use]
    pub fn confirmation_values(&self) -> Option<&TypedMap> {
        match self.active.as_ref() {
            Some(ActiveOverlay::ProviderConfirmation {
                continuation_values,
                ..
            }) => Some(continuation_values),
            _ => None,
        }
    }

    /// Provider continuation field currently accepting input.
    #[must_use]
    pub fn confirmation_focused_field(&self) -> Option<&Id> {
        match self.active.as_ref() {
            Some(ActiveOverlay::ProviderConfirmation { focused_field, .. }) => {
                focused_field.as_ref()
            }
            _ => None,
        }
    }

    /// Replace one declared provider continuation value on this instance.
    pub fn set_confirmation_value(&mut self, field: &Id, value: TypedValue) -> bool {
        let Some(ActiveOverlay::ProviderConfirmation {
            continuation_fields,
            continuation_values,
            ..
        }) = self.active.as_mut()
        else {
            return false;
        };
        if !continuation_fields.contains(field) {
            return false;
        }
        continuation_values.insert(field.clone(), value);
        true
    }

    /// Replace the working-directory choice on the active delete-agent request.
    pub fn set_delete_agent_work_dir(&mut self, value: bool) -> bool {
        let Some(ActiveOverlay::GenericConfirmation { request, .. }) = self.active.as_mut() else {
            return false;
        };
        let ConfirmationRequest::DeleteAgent {
            delete_work_dir, ..
        } = request.as_mut()
        else {
            return false;
        };
        *delete_work_dir = value;
        true
    }

    /// Move focus through continuation fields and the safe Cancel/Confirm decisions.
    pub fn cycle_confirmation_focus(&mut self) -> bool {
        if let Some(ActiveOverlay::GenericConfirmation { focus, .. }) = self.active.as_mut() {
            *focus = match *focus {
                ConfirmFocus::Cancel => ConfirmFocus::Confirm,
                ConfirmFocus::Confirm => ConfirmFocus::Cancel,
            };
            return true;
        }
        let Some(ActiveOverlay::ProviderConfirmation {
            focus,
            continuation_fields,
            focused_field,
            ..
        }) = self.active.as_mut()
        else {
            return false;
        };
        if let Some(current) = focused_field.as_ref() {
            let next = continuation_fields
                .iter()
                .position(|field| field == current)
                .and_then(|index| continuation_fields.get(index.saturating_add(1)))
                .cloned();
            if let Some(next) = next {
                *focused_field = Some(next);
            } else {
                *focused_field = None;
                *focus = ConfirmFocus::Confirm;
            }
            return true;
        }
        match *focus {
            ConfirmFocus::Cancel if continuation_fields.is_empty() => {
                *focus = ConfirmFocus::Confirm;
            }
            ConfirmFocus::Cancel => {
                *focused_field = continuation_fields.first().cloned();
            }
            ConfirmFocus::Confirm => {
                *focus = ConfirmFocus::Cancel;
            }
        }
        true
    }

    /// Current Help viewport, or zero when Help is not active.
    #[must_use]
    pub const fn help_viewport(&self) -> usize {
        match self.active.as_ref() {
            Some(ActiveOverlay::Help { viewport }) => *viewport,
            _ => 0,
        }
    }

    /// Replace the active Help viewport.
    pub fn set_help_viewport(&mut self, viewport: usize) -> bool {
        let Some(ActiveOverlay::Help { viewport: current }) = self.active.as_mut() else {
            return false;
        };
        *current = viewport;
        true
    }

    /// Search text the operator committed; kept for the active format and only an
    /// explicit clear removes it.
    #[must_use]
    pub fn search_query(&self) -> Option<&str> {
        match self.active.as_ref() {
            Some(ActiveOverlay::Search { query, .. }) => Some(query),
            _ => None,
        }
    }

    /// Append one character to active Search.
    pub fn push_search_char(&mut self, value: char) -> bool {
        let Some(ActiveOverlay::Search { query, cursor }) = self.active.as_mut() else {
            return false;
        };
        query.push(value);
        *cursor = query.len();
        true
    }

    /// Remove the last character from active Search.
    pub fn pop_search_char(&mut self) -> bool {
        let Some(ActiveOverlay::Search { query, cursor }) = self.active.as_mut() else {
            return false;
        };
        let changed = query.pop().is_some();
        *cursor = query.len();
        changed
    }

    /// Replace Search's instance-owned query and cursor.
    pub fn replace_search(&mut self, query: String, cursor: usize) -> bool {
        let Some(ActiveOverlay::Search {
            query: current,
            cursor: current_cursor,
        }) = self.active.as_mut()
        else {
            return false;
        };
        if cursor > query.len() || !query.is_char_boundary(cursor) {
            return false;
        }
        *current = query;
        *current_cursor = cursor;
        true
    }

    /// Close exactly the current live overlay.
    pub fn close(&mut self) -> bool {
        self.active.take().is_some()
    }

    fn open(&mut self, overlay: ActiveOverlay) -> bool {
        if self.active.is_some() || !self.declared.contains(&overlay.kind()) {
            return false;
        }
        self.active = Some(overlay);
        true
    }
}
