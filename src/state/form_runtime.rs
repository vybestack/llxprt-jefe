//! Pure runtime-choice transitions shared by agent and repository forms.

use crate::domain::{AgentKind, SandboxEngine};

use super::{AgentFormFields, AgentFormFocus};

/// Resolve the effective agent-kind choices for the currently open agent form.
///
/// Delegates to [`super::form_projection::effective_agent_kinds`] — the single
/// shared pure projection consumed by form cycling, UI rendering, and
/// selection content.
pub(super) fn effective_agent_kinds(installed: &[AgentKind], is_remote: bool) -> Vec<AgentKind> {
    super::form_projection::effective_agent_kinds(installed, is_remote)
}

pub(super) fn cycle_agent_field(
    installed: &[AgentKind],
    fields: &mut AgentFormFields,
    focus: AgentFormFocus,
    c: char,
) {
    if c != ' ' && c != 'x' && c != 'X' {
        return;
    }

    match focus {
        AgentFormFocus::AgentKind => {
            if let Some(next) = next_installed_kind(installed, &fields.agent_kind) {
                next.label().clone_into(&mut fields.agent_kind);
            }
        }
        AgentFormFocus::CodePuppyYolo => fields.code_puppy_yolo = !fields.code_puppy_yolo,
        AgentFormFocus::CodePuppyQuickResume => {
            fields.code_puppy_quick_resume.toggle();
        }
        AgentFormFocus::PassContinue => fields.pass_continue = !fields.pass_continue,
        AgentFormFocus::Sandbox => fields.sandbox_enabled = !fields.sandbox_enabled,
        AgentFormFocus::SandboxEngine => {
            SandboxEngine::next_from_form_value(&fields.sandbox_engine)
                .label()
                .clone_into(&mut fields.sandbox_engine);
        }
        _ => {}
    }
}

/// Slugify a name: lowercase, spaces to dashes, keep only alphanumerics and
/// dashes. Shared by repository and agent work-directory derivation so the slug
/// rule has a single source of truth.
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

/// Derive a repository slug from its display name.
pub(super) fn repository_slug_from_name(name: &str) -> String {
    slugify(name)
}

/// Derive a local agent work directory from its name and repository base path.
///
/// Agent names map to one platform-native directory segment beneath `base_dir`.
/// Use the work-dir field for custom nested paths when needed.
pub(super) fn derive_local_work_dir_from_name(name: &str, base_dir: &std::path::Path) -> String {
    let slug = slugify(name);
    if slug.is_empty() {
        base_dir.to_string_lossy().into_owned()
    } else {
        base_dir.join(slug).to_string_lossy().into_owned()
    }
}

/// Derive a remote Unix agent work directory without applying local path rules.
pub(super) fn derive_remote_work_dir_from_name(name: &str, base_dir: &str) -> String {
    let slug = slugify(name);
    if slug.is_empty() {
        base_dir.to_owned()
    } else {
        format!("{}/{slug}", base_dir.trim_end_matches('/'))
    }
}

pub(super) fn next_installed_kind(installed: &[AgentKind], value: &str) -> Option<AgentKind> {
    let current = AgentKind::from_form_value(value).unwrap_or_default();
    installed
        .iter()
        .position(|kind| *kind == current)
        .map_or_else(
            || installed.first().copied(),
            |index| installed.get((index + 1) % installed.len()).copied(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_work_dir_trims_trailing_separators_before_joining_slug() {
        assert_eq!(
            derive_remote_work_dir_from_name("Branch 1", "~/remote///"),
            "~/remote/branch-1"
        );
    }
}
