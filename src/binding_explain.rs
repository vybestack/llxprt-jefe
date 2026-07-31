//! Provider-free, read-only explanation of one composed key binding.

use std::path::Path;

use crate::domain::action_registry::{ActionRegistrySnapshot, Binding, Provenance, Resolution};
use crate::domain::input_context::{ContextId, ContextStack};
use crate::domain::keymap::Chord;
use crate::persistence::keymap_edit::load_bytes;
use crate::recovery::RecoveryOutput;

/// Read current settings and explain one binding without initializing providers or runtime.
#[must_use]
pub fn run(
    chord_text: &str,
    context_text: Option<&str>,
    config_dir: Option<&Path>,
) -> RecoveryOutput {
    let chord = match Chord::parse(chord_text) {
        Ok(chord) => chord,
        Err(error) => return invalid(format!("KEY-E401: {error}")),
    };
    let context = match context_text {
        Some(context) => ContextId::parse(context),
        None => ContextId::parse("global"),
    };
    let context = match context {
        Ok(context) => context,
        Err(error) => return invalid(format!("KEY-E401: {error}")),
    };
    let paths = match crate::persistence::paths::resolve(config_dir) {
        Ok(paths) => paths,
        Err(error) => return invalid(error.diagnostic.redacted_detail.clone()),
    };
    let bytes = match read_optional(&paths.settings.path) {
        Ok(bytes) => bytes,
        Err(error) => return invalid(error),
    };
    let catalog = match crate::config_owners::builtin_owner_catalog() {
        Ok(catalog) => catalog,
        Err(error) => return invalid(format!("configuration owner catalog: {error}")),
    };
    let source = paths.settings.path.to_string_lossy();
    let loaded = match load_bytes(bytes.as_deref(), &catalog, &source) {
        Ok(loaded) => loaded,
        Err(diagnostics) => return invalid(settings_error(&diagnostics)),
    };
    let stderr = loaded
        .diagnostic
        .as_ref()
        .map_or_else(String::new, ToString::to_string);
    explain_snapshot(&chord, &context, loaded.composed.snapshot(), stderr)
}

fn explain_snapshot(
    chord: &Chord,
    context: &ContextId,
    snapshot: &ActionRegistrySnapshot,
    stderr: String,
) -> RecoveryOutput {
    let Some(stack) = snapshot.context_stack(context) else {
        return invalid(format!("KEY-E401: unknown context {context:?}"));
    };
    let searched = stack.iter().map(ContextId::as_str).collect::<Vec<_>>();
    let resolution = snapshot.resolve(chord, stack);
    let winner = winner_binding(&resolution, &searched, snapshot.effective_bindings());
    let stdout = render(chord, &searched, &resolution, winner, snapshot);
    let exit_code = if matches!(resolution, Resolution::Unbound | Resolution::ForwardToPty) {
        2
    } else {
        0
    };
    RecoveryOutput {
        stdout,
        stderr,
        exit_code,
    }
}

fn winner_binding<'a>(
    resolution: &Resolution,
    searched: &[&str],
    bindings: &'a [Binding],
) -> Option<&'a Binding> {
    let action = match resolution {
        Resolution::Dispatch { action, .. } | Resolution::Unavailable { action, .. } => action,
        Resolution::ForwardToPty | Resolution::Unbound => return None,
    };
    searched.iter().find_map(|context| {
        bindings
            .iter()
            .find(|binding| binding.context.as_str() == *context && binding.action == *action)
    })
}

fn render(
    chord: &Chord,
    searched: &[&str],
    resolution: &Resolution,
    winner: Option<&Binding>,
    snapshot: &ActionRegistrySnapshot,
) -> String {
    let (resolution_name, action, handler, availability, reason) = resolution_fields(resolution);
    let winner_name = action.map_or("none", |action| action);
    let context = winner.map_or("none", |binding| binding.context.as_str());
    let provenance = winner.map_or_else(|| "none".to_owned(), provenance_text);
    let shadows = shadow_text(chord, searched, winner, snapshot);
    format!(
        "normalized chord: {chord}\nsearched contexts: {}\nresolution: {resolution_name}\nwinner: {winner_name}\nhandler: {handler}\ncontext: {context}\navailability: {availability}\nreason: {reason}\nshadows: {shadows}\nprovenance: {provenance}",
        searched.join(" -> ")
    )
}

fn resolution_fields(
    resolution: &Resolution,
) -> (&'static str, Option<&str>, String, &'static str, &str) {
    match resolution {
        Resolution::Dispatch { action, handler } => (
            "dispatch",
            Some(action.as_str()),
            format!("{handler:?}"),
            "available",
            "none",
        ),
        Resolution::Unavailable { action, reason } => (
            "unavailable",
            Some(action.as_str()),
            "none".to_owned(),
            "unavailable",
            reason,
        ),
        Resolution::ForwardToPty => (
            "forward-to-pty",
            None,
            "none".to_owned(),
            "n/a",
            "terminal capture",
        ),
        Resolution::Unbound => (
            "unbound",
            None,
            "none".to_owned(),
            "n/a",
            "no matching binding",
        ),
    }
}

fn provenance_text(binding: &Binding) -> String {
    match &binding.provenance {
        Provenance::Compiled => "compiled".to_owned(),
        Provenance::Settings { source } => format!("settings:{source}"),
    }
}

fn shadow_text(
    chord: &Chord,
    searched: &[&str],
    winner: Option<&Binding>,
    snapshot: &ActionRegistrySnapshot,
) -> String {
    let winner_context = winner.map(|binding| binding.context.as_str());
    let mut after_winner = winner_context.is_none();
    let mut shadows = Vec::new();
    for context in searched {
        if Some(*context) == winner_context {
            after_winner = true;
            continue;
        }
        if after_winner
            && let Some(binding) = resolved_binding_for_context(chord, context, snapshot)
        {
            shadows.push(binding);
        }
    }
    if shadows.is_empty() {
        "none".to_owned()
    } else {
        shadows
            .iter()
            .map(|binding| format!("{}:{}", binding.context, binding.action.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn resolved_binding_for_context<'a>(
    chord: &Chord,
    context: &str,
    snapshot: &'a ActionRegistrySnapshot,
) -> Option<&'a Binding> {
    let stack = ContextStack::from_ordered([context], context == "terminal").ok()?;
    let action = match snapshot.resolve(chord, &stack) {
        Resolution::Dispatch { action, .. } | Resolution::Unavailable { action, .. } => action,
        Resolution::ForwardToPty | Resolution::Unbound => return None,
    };
    snapshot
        .effective_bindings()
        .iter()
        .find(|binding| binding.context.as_str() == context && binding.action == action)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("read settings: {error}")),
    }
}

fn settings_error(diagnostics: &[crate::persistence::diagnostic::Diagnostic]) -> String {
    diagnostics.first().map_or_else(
        || "invalid settings document".to_owned(),
        |diagnostic| {
            format!(
                "{}: {}",
                diagnostic.code.as_str(),
                diagnostic.redacted_detail
            )
        },
    )
}

fn invalid(stderr: String) -> RecoveryOutput {
    RecoveryOutput {
        stdout: String::new(),
        stderr,
        exit_code: 2,
    }
}
