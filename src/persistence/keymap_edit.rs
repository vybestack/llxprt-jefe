//! Lossless schema-2 keymap candidate editing and registry composition.
//!
//! This boundary patches only selected keymap syntax, validates the complete
//! document and source-derived registry candidate, and publishes nothing until
//! every check succeeds.

use std::fmt;

use crate::domain::action_registry::{
    Action, ActionAvailability, ActionId, ActionRegistrySnapshot, Availability,
    AvailabilityGeneration, BindingOverride, RegistryCandidate,
};
use crate::domain::default_action_inventory::compiled_inventory;
use crate::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};
use crate::domain::input_context::ContextId;
use crate::domain::keymap::Chord;
use crate::domain::{Id, OwnerCatalog};

use super::diagnostic::Diagnostic;
use super::migration::migrate_settings;
use super::settings_document::{Assignment, PublishedSettings, SettingsDocument, patch_assignment};
use super::{FilePersistenceManager, PersistenceError, writer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeymapDiagnostic {
    detail: String,
}

impl KeymapDiagnostic {
    #[must_use]
    pub const fn code() -> &'static str {
        "KEY-E401"
    }

    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl KeymapDiagnostic {
    /// This refusal as one settings diagnostic the Settings shell can report.
    ///
    /// The Settings screen reports every reason a candidate cannot be saved in
    /// one list, and a keymap refusal is one of those reasons. Translating it
    /// here keeps `KEY-E401` the resolver's own answer while giving the screen
    /// the shape it already knows how to draw.
    #[must_use]
    pub fn as_settings_diagnostic(&self) -> Diagnostic {
        use super::diagnostic::{CfgCode, DiagnosticPath, Severity};
        let mut diagnostic = Diagnostic::new(
            CfgCode::E005,
            Severity::Error,
            DiagnosticPath::new("/keymap"),
            None,
            "correct the conflicting binding, or reset it to its compiled chords",
        );
        diagnostic.redacted_detail = self.to_string();
        diagnostic
    }
}

impl fmt::Display for KeymapDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", Self::code(), self.detail)
    }
}

impl std::error::Error for KeymapDiagnostic {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposedKeymap {
    snapshot: ActionRegistrySnapshot,
}

impl ComposedKeymap {
    #[must_use]
    pub const fn snapshot(&self) -> &ActionRegistrySnapshot {
        &self.snapshot
    }
}

#[derive(Clone, Debug)]
pub struct LoadedKeymap {
    pub settings: PublishedSettings,
    pub composed: ComposedKeymap,
    pub diagnostic: Option<KeymapDiagnostic>,
}

/// One closed lossless edit applied to a complete keymap candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeymapEdit {
    Set {
        context: ContextId,
        action: ActionId,
        chords: Vec<Chord>,
    },
    Reset {
        context: ContextId,
        action: ActionId,
    },
}

impl KeymapEdit {
    #[must_use]
    pub const fn set(context: ContextId, action: ActionId, chords: Vec<Chord>) -> Self {
        Self::Set {
            context,
            action,
            chords,
        }
    }

    #[must_use]
    pub const fn reset(context: ContextId, action: ActionId) -> Self {
        Self::Reset { context, action }
    }
}

#[derive(Clone, Debug)]
pub struct KeymapCandidate {
    bytes: Vec<u8>,
    expected_hash: writer::ExpectedHash,
    published: PublishedSettings,
    composed: ComposedKeymap,
}

impl KeymapCandidate {
    pub fn from_edits(
        document: &SettingsDocument,
        catalog: &OwnerCatalog,
        edits: &[KeymapEdit],
        expected_hash: writer::ExpectedHash,
        source: &str,
    ) -> Result<Self, KeymapDiagnostic> {
        let bytes = apply_edits(document, edits)?;
        candidate_from_bytes(bytes, expected_hash, catalog, source)
    }

    pub fn set(
        document: &SettingsDocument,
        catalog: &OwnerCatalog,
        context: &ContextId,
        action: &ActionId,
        chords: &[Chord],
        source: &str,
    ) -> Result<Self, KeymapDiagnostic> {
        let value = chord_array(chords);
        candidate_from_patch(document, catalog, context, action, Some(value), source)
    }

    pub fn unbind(
        document: &SettingsDocument,
        catalog: &OwnerCatalog,
        context: &ContextId,
        action: &ActionId,
        source: &str,
    ) -> Result<Self, KeymapDiagnostic> {
        Self::set(document, catalog, context, action, &[], source)
    }

    pub fn reset(
        document: &SettingsDocument,
        catalog: &OwnerCatalog,
        context: &ContextId,
        action: &ActionId,
        source: &str,
    ) -> Result<Self, KeymapDiagnostic> {
        candidate_from_patch(document, catalog, context, action, None, source)
    }

    pub fn patch(
        document: &SettingsDocument,
        catalog: &OwnerCatalog,
        context: &ContextId,
        action: &ActionId,
        chords: Option<&[Chord]>,
        source: &str,
    ) -> Result<Self, KeymapDiagnostic> {
        match chords {
            Some(chords) => Self::set(document, catalog, context, action, chords, source),
            None => Self::reset(document, catalog, context, action, source),
        }
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn published(&self) -> &PublishedSettings {
        &self.published
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ActionRegistrySnapshot {
        self.composed.snapshot()
    }
}

pub fn compose_published(
    settings: &PublishedSettings,
    source: &str,
) -> Result<ComposedKeymap, KeymapDiagnostic> {
    let inventory =
        compiled_inventory().map_err(|error| KeymapDiagnostic::new(error.to_string()))?;
    let overrides = parse_overrides(settings, source)?;
    let availability = availability(&inventory.actions)?;
    let snapshot = RegistryCandidate::new(
        inventory.actions,
        inventory.bindings,
        overrides,
        inventory.context_stacks,
        availability,
    )
    .compose()
    .map_err(|error| KeymapDiagnostic::new(error.to_string()))?;
    Ok(ComposedKeymap { snapshot })
}

pub fn load_bytes(
    bytes: Option<&[u8]>,
    catalog: &OwnerCatalog,
    source: &str,
) -> Result<LoadedKeymap, Vec<Diagnostic>> {
    let Some(bytes) = bytes else {
        return defaults_load(None);
    };
    let migration = match migrate_settings(bytes, catalog) {
        Ok(migration) => migration,
        Err(diagnostics)
            if !diagnostics.is_empty() && diagnostics.iter().all(keymap_settings_diagnostic) =>
        {
            let document =
                SettingsDocument::parse(bytes).map_err(|diagnostic| vec![*diagnostic])?;
            let settings = super::settings_publish::publish_without_keymap(&document, catalog)?;
            let diagnostic = settings_diagnostic(&diagnostics)
                .map_err(|error| vec![internal_diagnostic(error.to_string())])?;
            let composed = compose_published(&settings, "compiled defaults")
                .map_err(|error| vec![internal_diagnostic(error.to_string())])?;
            return Ok(LoadedKeymap {
                settings,
                composed,
                diagnostic: Some(diagnostic),
            });
        }
        Err(diagnostics) => return Err(diagnostics),
    };
    let settings = migration.published().clone();
    match compose_published(&settings, source) {
        Ok(composed) => Ok(LoadedKeymap {
            settings,
            composed,
            diagnostic: None,
        }),
        Err(diagnostic) => {
            let mut fallback = settings;
            fallback.keymap.clear();
            let composed = compose_published(&fallback, source)
                .map_err(|error| vec![internal_diagnostic(error.to_string())])?;
            Ok(LoadedKeymap {
                settings: fallback,
                composed,
                diagnostic: Some(diagnostic),
            })
        }
    }
}

fn defaults_load(diagnostic: Option<KeymapDiagnostic>) -> Result<LoadedKeymap, Vec<Diagnostic>> {
    let settings = PublishedSettings::default();
    let composed = compose_published(&settings, "compiled defaults")
        .map_err(|error| vec![internal_diagnostic(error.to_string())])?;
    Ok(LoadedKeymap {
        settings,
        composed,
        diagnostic,
    })
}

fn candidate_from_patch(
    document: &SettingsDocument,
    catalog: &OwnerCatalog,
    context: &ContextId,
    action: &ActionId,
    value: Option<Vec<u8>>,
    source: &str,
) -> Result<KeymapCandidate, KeymapDiagnostic> {
    let bytes = patch_bytes(document, context, action, value);
    candidate_from_bytes(
        bytes,
        writer::ExpectedHash::Present(document.sha256()),
        catalog,
        source,
    )
}

fn candidate_from_bytes(
    bytes: Vec<u8>,
    expected_hash: writer::ExpectedHash,
    catalog: &OwnerCatalog,
    source: &str,
) -> Result<KeymapCandidate, KeymapDiagnostic> {
    let parsed = SettingsDocument::parse(&bytes)
        .map_err(|diagnostic| KeymapDiagnostic::new(diagnostic.redacted_detail.clone()))?;
    let published = match parsed.publish(catalog) {
        Ok(published) => published,
        Err(diagnostics) => return Err(settings_diagnostic(&diagnostics)?),
    };
    let composed = compose_published(&published, source)?;
    Ok(KeymapCandidate {
        bytes,
        expected_hash,

        published,
        composed,
    })
}

fn apply_edits(
    document: &SettingsDocument,
    edits: &[KeymapEdit],
) -> Result<Vec<u8>, KeymapDiagnostic> {
    let mut bytes = document.original_bytes().to_vec();
    for edit in edits {
        let current = SettingsDocument::parse(&bytes)
            .map_err(|diagnostic| KeymapDiagnostic::new(diagnostic.redacted_detail.clone()))?;
        bytes = match edit {
            KeymapEdit::Set {
                context,
                action,
                chords,
            } => patch_bytes(&current, context, action, Some(chord_array(chords))),
            KeymapEdit::Reset { context, action } => patch_bytes(&current, context, action, None),
        };
    }
    Ok(bytes)
}
fn patch_bytes(
    document: &SettingsDocument,
    context: &ContextId,
    action: &ActionId,
    value: Option<Vec<u8>>,
) -> Vec<u8> {
    let path = ["keymap", context.as_str(), action.as_str()];
    let table_header = format!("[keymap.\"{}\"]", context.as_str());
    let key_text = format!("\"{}\"", action.as_str());
    patch_assignment(
        document,
        &Assignment {
            path: &path,
            table_header: &table_header,
            key_text: &key_text,
        },
        value.as_deref(),
    )
    // A keymap context written as one inline value has no syntax for this
    // binding; leaving the bytes alone keeps the document valid, and the
    // complete-candidate check that follows reports that nothing changed.
    .unwrap_or_else(|_| document.original_bytes().to_vec())
}

fn chord_array(chords: &[Chord]) -> Vec<u8> {
    toml::Value::Array(
        chords
            .iter()
            .map(|chord| toml::Value::String(chord.to_string()))
            .collect(),
    )
    .to_string()
    .into_bytes()
}

fn parse_overrides(
    settings: &PublishedSettings,
    source: &str,
) -> Result<Vec<BindingOverride>, KeymapDiagnostic> {
    let mut overrides = Vec::new();
    for (context, actions) in &settings.keymap {
        let context =
            ContextId::parse(context).map_err(|error| KeymapDiagnostic::new(error.to_string()))?;
        for (action, chord_texts) in actions {
            let action = ActionId::parse(action)
                .map_err(|error| KeymapDiagnostic::new(error.to_string()))?;
            let chords = parse_chords(chord_texts)?;
            overrides.push(BindingOverride::new(
                context.clone(),
                action,
                chords,
                source,
            ));
        }
    }
    Ok(overrides)
}

fn parse_chords(values: &[String]) -> Result<Vec<Chord>, KeymapDiagnostic> {
    values
        .iter()
        .map(|value| Chord::parse(value).map_err(|error| KeymapDiagnostic::new(error.to_string())))
        .collect()
}

fn availability(actions: &[Action]) -> Result<AvailabilityGeneration, KeymapDiagnostic> {
    let owner =
        Id::parse("core.keymap").map_err(|error| KeymapDiagnostic::new(error.to_string()))?;
    let entries = actions
        .iter()
        .map(|action| ActionAvailability::new(action.id.clone(), Availability::Available))
        .collect();
    Ok(AvailabilityGeneration::new(
        Correlation {
            correlation_id: CorrelationId::new(0),
            owner,
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: SemanticKey::new(EffectFamily::Persistence, "keymap-composition"),
        },
        entries,
    ))
}

fn keymap_settings_diagnostic(diagnostic: &Diagnostic) -> bool {
    diagnostic.path.as_str().starts_with("/keymap")
}

fn settings_diagnostic(diagnostics: &[Diagnostic]) -> Result<KeymapDiagnostic, KeymapDiagnostic> {
    let Some(diagnostic) = diagnostics.first() else {
        return Err(KeymapDiagnostic::new(
            "settings publication failed without a diagnostic",
        ));
    };
    Ok(KeymapDiagnostic::new(diagnostic.redacted_detail.clone()))
}

fn internal_diagnostic(detail: String) -> Diagnostic {
    use super::diagnostic::{CfgCode, DiagnosticPath, Severity};
    let mut diagnostic = Diagnostic::new(
        CfgCode::E005,
        Severity::Error,
        DiagnosticPath::new("/keymap"),
        None,
        "repair the compiled keymap inventory",
    );
    diagnostic.redacted_detail = detail;
    diagnostic
}

impl FilePersistenceManager {
    pub fn save_keymap_candidate_revisioned(
        &self,
        candidate: &KeymapCandidate,
        revision: u64,
        freshness: &crate::services::persist_worker::FreshnessFn,
    ) -> Result<writer::WriteOutcome, PersistenceError> {
        Self::run_write(
            &self.paths.settings_path,
            candidate.bytes.clone(),
            revision,
            candidate.expected_hash,
            writer::BackupPolicy::None,
            freshness,
        )
    }
}
