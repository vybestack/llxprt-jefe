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
/// snapshot, whether the target repository is remote, and whether `npm` is
/// available on the local PATH.
///
/// For remote repositories **both** supported kinds are always offered,
/// regardless of the local installed snapshot or npm availability. This is
/// intentional and correct: the local PATH cannot determine what is
/// installed on a remote host, so restricting choices to locally-installed
/// runtimes would prevent the user from selecting a perfectly valid remote
/// runtime.
///
/// The **target remote availability probe** (`remote_probe`) is the guard:
/// it runs a side-effect-free `ssh -T` check for the exact binary on the
/// remote host immediately before any side effect or launch. An
/// unavailable remote runtime is caught there, not at form-choice time.
/// No local startup cache of remote availability is built.
///
/// For local repositories only the locally installed kinds are offered so
/// the user cannot select a runtime that cannot launch — **except** that
/// LLxprt is also offered when `npm` is available, because a versioned
/// LLxprt launch routes through `npm exec` and never requires a directly
/// installed `llxprt` binary. This ensures repository and agent forms can
/// select versioned LLxprt even when only npm is present.
///
/// This is the single shared pure projection consumed by the form-state
/// cycling logic, the UI form components, and the selection-content
/// projections. All three must agree on the effective choice set.
#[must_use]
pub fn effective_agent_kinds_with_npm(
    installed: &[AgentKind],
    is_remote: bool,
    npm_available: bool,
) -> Vec<AgentKind> {
    if is_remote {
        // Both kinds are offered for remote repos — local PATH cannot
        // determine remote installation. The remote availability probe
        // (remote_probe) guards before side effects/launch, not the form.
        ALL_AGENT_KINDS.to_vec()
    } else {
        let mut kinds: Vec<AgentKind> = installed.to_vec();
        // Offer LLxprt when npm is available even if not directly installed,
        // so a versioned launch (npm exec) can be selected.
        if npm_available && !kinds.contains(&AgentKind::Llxprt) {
            kinds.insert(0, AgentKind::Llxprt);
        }
        // Deduplicate while preserving canonical order.
        let mut seen = Vec::new();
        kinds.retain(|kind| {
            if seen.contains(kind) {
                false
            } else {
                seen.push(*kind);
                true
            }
        });
        // Sort to canonical order (Llxprt before CodePuppy).
        kinds.sort_by_key(|kind| match kind {
            AgentKind::Llxprt => 0,
            AgentKind::CodePuppy => 1,
        });
        kinds
    }
}

/// Resolve the effective agent-kind choices for a form given the installed
/// snapshot and whether the target repository is remote.
///
/// Delegates to [`effective_agent_kinds_with_npm`] with `npm_available =
/// false`, preserving the original contract for callers that do not yet
/// pass npm availability. New callers should use
/// [`effective_agent_kinds_with_npm`] directly.
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
    effective_agent_kinds_with_npm(installed, is_remote, false)
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
        | F::LlxprtVersion
        | F::Mode
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
        assert_eq!(next_visible_focus(F::Profile, vis), F::LlxprtVersion);
        assert_eq!(next_visible_focus(F::LlxprtVersion, vis), F::AgentKind);
        assert_eq!(next_visible_focus(F::AgentKind, vis), F::Mode);
        assert_eq!(prev_visible_focus(F::Mode, vis), F::AgentKind);
        assert_eq!(prev_visible_focus(F::AgentKind, vis), F::LlxprtVersion);
    }

    #[test]
    fn code_puppy_next_focus_skips_llxprt_version() {
        let vis = agent_form_visibility(AgentKind::CodePuppy);
        // For CodePuppy, Profile AND LlxprtVersion are both hidden, so
        // WorkDir → AgentKind and AgentKind ← WorkDir.
        assert_eq!(next_visible_focus(F::WorkDir, vis), F::AgentKind);
        assert_eq!(prev_visible_focus(F::AgentKind, vis), F::WorkDir);
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
    }

    #[test]
    fn local_restricts_to_installed_kinds() {
        let installed = vec![AgentKind::Llxprt];
        let kinds = effective_agent_kinds(&installed, false);
        assert_eq!(kinds, vec![AgentKind::Llxprt]);
    }

    // ── Issue #269: npm-available LLxprt in local runtime choices ──────────
    //
    // When npm is present on the local PATH but llxprt is NOT directly
    // installed, a local repository must still offer LLxprt as a runtime
    // choice so the user can select a versioned LLxprt launch. The versioned
    // launch routes through `npm exec` and never requires a directly
    // installed llxprt binary.

    #[test]
    fn local_includes_llxprt_when_npm_available_without_direct_llxprt() {
        // Only code-puppy is directly installed, but npm is available.
        // LLxprt must still be offered so a versioned launch can be selected.
        let installed = vec![AgentKind::CodePuppy];
        let kinds = effective_agent_kinds_with_npm(&installed, false, true);
        assert!(
            kinds.contains(&AgentKind::Llxprt),
            "LLxprt must be offered when npm is available even without direct llxprt: {kinds:?}"
        );
        assert!(
            kinds.contains(&AgentKind::CodePuppy),
            "directly installed kinds must still be offered: {kinds:?}"
        );
    }

    #[test]
    fn local_excludes_llxprt_when_npm_absent_and_llxprt_not_installed() {
        // Neither llxprt nor npm available — LLxprt must NOT be offered.
        let installed = vec![AgentKind::CodePuppy];
        let kinds = effective_agent_kinds_with_npm(&installed, false, false);
        assert!(
            !kinds.contains(&AgentKind::Llxprt),
            "LLxprt must not be offered when neither llxprt nor npm is available: {kinds:?}"
        );
    }

    #[test]
    fn local_llxprt_directly_installed_takes_precedence_over_npm_check() {
        // When llxprt is directly installed, it must be offered regardless of
        // npm availability (blank version preserves direct launch).
        let installed = vec![AgentKind::Llxprt];
        let kinds_no_npm = effective_agent_kinds_with_npm(&installed, false, false);
        let kinds_npm = effective_agent_kinds_with_npm(&installed, false, true);
        assert_eq!(kinds_no_npm, vec![AgentKind::Llxprt]);
        assert_eq!(kinds_npm, vec![AgentKind::Llxprt]);
    }

    #[test]
    fn remote_always_offers_both_regardless_of_npm() {
        // Remote repos always offer both kinds; npm presence is irrelevant
        // for the form choice (the remote probe checks npm at launch time).
        let installed = vec![];
        let kinds = effective_agent_kinds_with_npm(&installed, true, false);
        assert_eq!(kinds, vec![AgentKind::Llxprt, AgentKind::CodePuppy]);
    }
}
