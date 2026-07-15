//! Pure projection of agent form field visibility per agent kind.
//!
//! This module is iocraft-free and side-effect-free: it turns an
//! [`AgentKind`] into a boolean mask of which LLxprt-only form controls
//! should be visible. The UI components and the selection-content
//! projection both consume this so rendering and focus navigation agree.
//!
//! Code Puppy does not support `--profile-load`, `--sandbox`, `--continue`,
//! `LLXPRT_DEBUG`, or the sandbox engine/flags — so those controls are hidden
//! when the active agent kind is [`AgentKind::CodePuppy`].

use crate::domain::AgentKind;

/// All agent kinds supported by Jefe, in canonical order.
const ALL_AGENT_KINDS: [AgentKind; 2] = [AgentKind::Llxprt, AgentKind::CodePuppy];

/// Resolve the effective agent-kind choices for a form given the installed
/// snapshot and whether the target repository is remote.
///
/// For remote repositories **both** supported kinds are always offered,
/// regardless of the local installed snapshot. This is intentional and
/// correct: the local PATH cannot determine what is installed on a remote
/// host, so restricting choices to locally-installed runtimes would
/// prevent the user from selecting a perfectly valid remote runtime.
///
/// The **target remote availability probe** (`remote_probe`) is the guard:
/// it runs a side-effect-free `ssh -T` check for the exact binary on the
/// remote host immediately before any side effect or launch. An
/// unavailable remote runtime is caught there, not at form-choice time.
/// No local startup cache of remote availability is built.
///
/// For local repositories only the locally installed kinds are offered so
/// the user cannot select a runtime that cannot launch.
///
/// This is the single shared pure projection consumed by the form-state
/// cycling logic, the UI form components, and the selection-content
/// projections. All three must agree on the effective choice set.
#[must_use]
pub fn effective_agent_kinds(installed: &[AgentKind], is_remote: bool) -> Vec<AgentKind> {
    if is_remote {
        // Both kinds are offered for remote repos — local PATH cannot
        // determine remote installation. The remote availability probe
        // (remote_probe) guards before side effects/launch, not the form.
        ALL_AGENT_KINDS.to_vec()
    } else {
        installed.to_vec()
    }
}

/// Format the effective agent kinds as a space-separated label list for form
/// hints (e.g. `"LLxprt / code_puppy"`).
#[must_use]
pub fn effective_kinds_hint(kinds: &[AgentKind]) -> String {
    let labels: Vec<&str> = kinds.iter().map(|k| k.label()).collect();
    if labels.is_empty() {
        String::from("no installed agents")
    } else {
        format!("space cycles: {}", labels.join(" / "))
    }
}

/// Per-field visibility mask derived from the active agent kind.
///
/// All fields default to `true` (visible). Code Puppy hides the LLxprt-only
/// controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentFormFieldVisibility {
    #[default]
    Llxprt,
    CodePuppy,
}

impl AgentFormFieldVisibility {
    #[must_use]
    pub const fn shows_llxprt_fields(self) -> bool {
        matches!(self, Self::Llxprt)
    }
}

/// Compute the field-visibility mask for the given agent kind.
///
/// # Examples
///
/// ```
/// # use jefe::domain::AgentKind;
/// use jefe::state::agent_form_visibility;
/// let llxprt = agent_form_visibility(AgentKind::Llxprt);
/// assert!(llxprt.shows_llxprt_fields());
/// let puppy = agent_form_visibility(AgentKind::CodePuppy);
/// assert!(!puppy.shows_llxprt_fields());
/// ```
#[must_use]
pub fn agent_form_visibility(kind: AgentKind) -> AgentFormFieldVisibility {
    match kind {
        AgentKind::Llxprt => AgentFormFieldVisibility::Llxprt,
        AgentKind::CodePuppy => AgentFormFieldVisibility::CodePuppy,
    }
}

/// Resolve the effective [`AgentKind`] from a form value string, falling
/// back to [`AgentKind::default`] (Llxprt) when the value does not parse.
#[must_use]
pub fn kind_from_form_value(value: &str) -> AgentKind {
    AgentKind::from_form_value(value).unwrap_or_default()
}

/// Whether a specific agent form focus variant is visible under the given
/// visibility mask.
///
/// Always-visible fields (Shortcut, Name, Description, WorkDir, AgentKind)
/// return `true` regardless of the mask.
#[must_use]
pub fn is_field_visible(
    focus: crate::state::AgentFormFocus,
    visibility: AgentFormFieldVisibility,
) -> bool {
    use crate::state::AgentFormFocus as F;
    match focus {
        F::Profile
        | F::Mode
        | F::LlxprtVersion
        | F::LlxprtDebug
        | F::PassContinue
        | F::Sandbox
        | F::SandboxEngine
        | F::SandboxFlags => visibility.shows_llxprt_fields(),
        F::CodePuppyModel | F::CodePuppyYolo | F::CodePuppyQuickResume => {
            matches!(visibility, AgentFormFieldVisibility::CodePuppy)
        }
        F::Shortcut | F::Name | F::Description | F::WorkDir | F::AgentKind => true,
    }
}

/// Advance focus to the next visible field, skipping hidden ones.
///
/// Wraps around. If all fields are hidden (degenerate), returns the original
/// focus to avoid an infinite loop.
#[must_use]
pub fn next_visible_focus(
    focus: crate::state::AgentFormFocus,
    visibility: AgentFormFieldVisibility,
) -> crate::state::AgentFormFocus {
    let start = focus;
    let mut current = focus.next();
    while current != start {
        if is_field_visible(current, visibility) {
            return current;
        }
        current = current.next();
    }
    // Every field is hidden except possibly `start` — keep the cursor put.
    start
}

/// Advance focus to the previous visible field, skipping hidden ones.
///
/// Wraps around. If all fields are hidden (degenerate), returns the original
/// focus to avoid an infinite loop.
#[must_use]
pub fn prev_visible_focus(
    focus: crate::state::AgentFormFocus,
    visibility: AgentFormFieldVisibility,
) -> crate::state::AgentFormFocus {
    let start = focus;
    let mut current = focus.prev();
    while current != start {
        if is_field_visible(current, visibility) {
            return current;
        }
        current = current.prev();
    }
    start
}

/// Whether a repository form field is visible for the selected default runtime.
#[must_use]
pub fn is_repository_field_visible(
    focus: crate::state::RepositoryFormFocus,
    kind: AgentKind,
) -> bool {
    use crate::state::RepositoryFormFocus as F;
    match focus {
        F::DefaultCodePuppyModel | F::DefaultCodePuppyYolo => kind == AgentKind::CodePuppy,
        F::DefaultLlxprtMode | F::DefaultLlxprtVersion => kind == AgentKind::Llxprt,
        _ => true,
    }
}

/// Advance to the next visible repository field.
#[must_use]
pub fn next_visible_repository_focus(
    focus: crate::state::RepositoryFormFocus,
    kind: AgentKind,
) -> crate::state::RepositoryFormFocus {
    let start = focus;
    let mut current = focus.next();
    while current != start {
        if is_repository_field_visible(current, kind) {
            return current;
        }
        current = current.next();
    }
    start
}

/// Advance to the previous visible repository field.
#[must_use]
pub fn prev_visible_repository_focus(
    focus: crate::state::RepositoryFormFocus,
    kind: AgentKind,
) -> crate::state::RepositoryFormFocus {
    let start = focus;
    let mut current = focus.prev();
    while current != start {
        if is_repository_field_visible(current, kind) {
            return current;
        }
        current = current.prev();
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentFormFocus as F;

    #[test]
    fn llxprt_shows_all_fields() {
        let vis = agent_form_visibility(AgentKind::Llxprt);
        assert!(is_field_visible(F::Profile, vis));
        assert!(is_field_visible(F::Mode, vis));
        assert!(is_field_visible(F::Sandbox, vis));
        assert!(is_field_visible(F::SandboxFlags, vis));
        assert!(!is_field_visible(F::CodePuppyModel, vis));
    }

    #[test]
    fn code_puppy_hides_llxprt_only_fields() {
        let vis = agent_form_visibility(AgentKind::CodePuppy);
        assert!(!is_field_visible(F::Profile, vis));
        assert!(!is_field_visible(F::Mode, vis));
        assert!(!is_field_visible(F::LlxprtDebug, vis));
        assert!(!is_field_visible(F::PassContinue, vis));
        assert!(!is_field_visible(F::Sandbox, vis));
        assert!(!is_field_visible(F::SandboxEngine, vis));
        assert!(!is_field_visible(F::SandboxFlags, vis));
        assert!(is_field_visible(F::CodePuppyModel, vis));
    }

    #[test]
    fn code_puppy_keeps_common_fields() {
        let vis = agent_form_visibility(AgentKind::CodePuppy);
        assert!(is_field_visible(F::Shortcut, vis));
        assert!(is_field_visible(F::Name, vis));
        assert!(is_field_visible(F::Description, vis));
        assert!(is_field_visible(F::WorkDir, vis));
        assert!(is_field_visible(F::AgentKind, vis));
    }

    #[test]
    fn code_puppy_next_focus_skips_hidden_fields() {
        let vis = agent_form_visibility(AgentKind::CodePuppy);
        // Runtime selection precedes Code Puppy-specific controls visually.
        let next = next_visible_focus(F::Profile, vis);
        assert_eq!(next, F::AgentKind);
        assert_eq!(next_visible_focus(F::AgentKind, vis), F::CodePuppyModel);
    }

    #[test]
    fn code_puppy_resume_focus_is_between_yolo_and_mode() {
        let vis = agent_form_visibility(AgentKind::CodePuppy);
        assert_eq!(
            next_visible_focus(F::CodePuppyYolo, vis),
            F::CodePuppyQuickResume
        );
        assert_eq!(prev_visible_focus(F::Mode, vis), F::CodePuppyQuickResume);
    }

    #[test]
    fn llxprt_next_focus_uses_normal_order() {
        let vis = agent_form_visibility(AgentKind::Llxprt);
        assert_eq!(next_visible_focus(F::Profile, vis), F::AgentKind);
        assert_eq!(next_visible_focus(F::AgentKind, vis), F::Mode);
        assert_eq!(prev_visible_focus(F::Mode, vis), F::AgentKind);
    }

    #[test]
    fn kind_from_form_value_parses_variants() {
        assert_eq!(kind_from_form_value("code_puppy"), AgentKind::CodePuppy);
        assert_eq!(kind_from_form_value("LLxprt"), AgentKind::Llxprt);
        assert_eq!(kind_from_form_value("garbage"), AgentKind::Llxprt);
    }

    // ── Remote kinds: both offered regardless of local install ─────────
    //
    // Remote repositories intentionally offer both supported kinds because
    // the local PATH cannot determine what is installed on the remote host.
    // The target remote availability probe (remote_probe) guards before
    // side effects/launch — no local startup cache of remote availability
    // is built.

    #[test]
    fn remote_offers_both_kinds_even_when_locally_uninstalled() {
        // Only LLxprt is locally installed, but a remote repo offers both.
        let installed = vec![AgentKind::Llxprt];
        let kinds = effective_agent_kinds(&installed, true);
        assert_eq!(kinds, vec![AgentKind::Llxprt, AgentKind::CodePuppy]);
    }

    #[test]
    fn remote_offers_both_kinds_even_when_nothing_installed() {
        // Even with zero local installs, remote offers both kinds.
        let kinds = effective_agent_kinds(&[], true);
        assert_eq!(kinds, vec![AgentKind::Llxprt, AgentKind::CodePuppy]);
        repository_llxprt_focus_order_is_forward_reverse_and_wrapped();
    }

    fn new_agent_copies_repository_version_and_appends_at_end() {
        use crate::domain::{LlxprtNpmPackageSelector, Repository, RepositoryId};
        use crate::state::{AgentFormFocus, AppEvent, AppState, ModalState};

        let mut repository = Repository::new(
            RepositoryId("repo-version".to_owned()),
            "repo".to_owned(),
            "repo".to_owned(),
            "/tmp/repo".into(),
        );
        repository.default_llxprt_version = LlxprtNpmPackageSelector::normalize("0.9.0");
        let mut state = AppState {
            repositories: vec![repository],
            ..AppState::default()
        };
        state = state.apply(AppEvent::OpenNewAgent(RepositoryId(
            "repo-version".to_owned(),
        )));
        let ModalState::NewAgent { focus, .. } = &mut state.modal else {
            panic!("expected new-agent modal");
        };
        *focus = AgentFormFocus::LlxprtVersion;
        state = state.apply(AppEvent::FormChar('-'));
        state = state.apply(AppEvent::FormChar('x'));
        let ModalState::NewAgent { fields, cursor, .. } = &state.modal else {
            panic!("expected new-agent modal");
        };
        assert_eq!(fields.llxprt_version, "0.9.0-x");
        assert_eq!(cursor.llxprt_version, 7);
        assert_eq!(
            state.repositories[0]
                .default_llxprt_version
                .as_ref()
                .map(LlxprtNpmPackageSelector::as_str),
            Some("0.9.0")
        );
    }

    fn repository_llxprt_focus_order_is_forward_reverse_and_wrapped() {
        use crate::state::RepositoryFormFocus as R;

        new_agent_copies_repository_version_and_appends_at_end();

        assert_eq!(
            next_visible_repository_focus(R::DefaultProfile, AgentKind::Llxprt),
            R::DefaultAgentKind
        );
        assert_eq!(
            next_visible_repository_focus(R::DefaultAgentKind, AgentKind::Llxprt),
            R::DefaultLlxprtMode
        );
        assert_eq!(
            next_visible_repository_focus(R::DefaultLlxprtMode, AgentKind::Llxprt),
            R::DefaultLlxprtVersion
        );
        assert_eq!(
            next_visible_repository_focus(R::DefaultLlxprtVersion, AgentKind::Llxprt),
            R::TransientAgentDir
        );
        assert_eq!(
            prev_visible_repository_focus(R::GitHubRepo, AgentKind::Llxprt),
            R::TransientMaxConcurrent
        );
        assert_eq!(
            prev_visible_repository_focus(R::DefaultLlxprtVersion, AgentKind::Llxprt),
            R::DefaultLlxprtMode
        );
        assert_eq!(
            prev_visible_repository_focus(R::DefaultLlxprtMode, AgentKind::Llxprt),
            R::DefaultAgentKind
        );
        assert_eq!(
            next_visible_repository_focus(R::SetupEnvDefault, AgentKind::Llxprt),
            R::Name
        );
        assert_eq!(
            prev_visible_repository_focus(R::Name, AgentKind::Llxprt),
            R::SetupEnvDefault
        );
        repository_code_puppy_focus_order_skips_version_in_both_directions();
    }

    fn repository_code_puppy_focus_order_skips_version_in_both_directions() {
        use crate::state::RepositoryFormFocus as R;

        assert_eq!(
            next_visible_repository_focus(R::DefaultProfile, AgentKind::CodePuppy),
            R::DefaultCodePuppyModel
        );
        assert_eq!(
            next_visible_repository_focus(R::DefaultCodePuppyModel, AgentKind::CodePuppy),
            R::DefaultAgentKind
        );
        assert_eq!(
            next_visible_repository_focus(R::DefaultAgentKind, AgentKind::CodePuppy),
            R::DefaultCodePuppyYolo
        );
        assert_eq!(
            next_visible_repository_focus(R::DefaultCodePuppyYolo, AgentKind::CodePuppy),
            R::TransientAgentDir
        );
        assert_eq!(
            prev_visible_repository_focus(R::GitHubRepo, AgentKind::CodePuppy),
            R::TransientMaxConcurrent
        );
        assert_eq!(
            prev_visible_repository_focus(R::DefaultAgentKind, AgentKind::CodePuppy),
            R::DefaultCodePuppyModel
        );
    }
    #[test]
    fn local_restricts_to_installed_kinds() {
        let installed = vec![AgentKind::Llxprt];
        let kinds = effective_agent_kinds(&installed, false);
        assert_eq!(kinds, vec![AgentKind::Llxprt]);
    }
}
