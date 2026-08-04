//! Lossless schema-2 settings candidate editing for the Settings shell.
//!
//! This boundary owns no parser and no writer of its own. It patches exactly
//! the syntax the user changed, re-validates the complete document, and
//! publishes nothing until every check succeeds — the same contract
//! [`super::keymap_edit`] applies to keymap syntax, over the same lossless
//! document and the same atomic writer.
//!
//! The editable set is closed. A settings shell that could name any path would
//! have to answer "what type does this path hold" at runtime; naming a leaf
//! instead makes an ill-typed edit unrepresentable, so the only type errors a
//! candidate can carry are the ones a person hand-wrote into the file.

use std::path::{Component, Path, PathBuf};

use crate::domain::action_registry::ActionId;
use crate::domain::input_context::ContextId;
use crate::domain::keymap::Chord;
use crate::domain::sha256::Sha256;
use crate::domain::{Id, OwnerCatalog, ThemeId};
use crate::workbench::descriptor::LayoutNode;

use super::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use super::migration::{SettingsMigration, format_migrated_settings};
use super::settings_document::{
    Assignment, PublishedSettings, SettingsDocument, patch_assignment, remove_table_block,
};
use super::writer::{self, AtomicWrite, DraftBytes, ExpectedHash};
use super::{FilePersistenceManager, PersistencePaths};

/// Maximum distinct syntax paths one draft may hold edits for.
///
/// The editable set is a closed enum well inside this bound, so the limit is
/// proven structurally rather than checked at runtime; it is stated here
/// because the settings contract documents it.
pub const EDITED_PATH_LIMIT: usize = 256;

/// The document an absent settings file is bound to.
///
/// A missing settings file is normal, so opening Settings on a fresh
/// installation binds to the same empty schema-2 document startup validates.
/// Its first Save creates the file.
pub const EMPTY_SCHEMA_2: &[u8] = b"settings_schema = 2
";

/// Bind a draft's base to loaded bytes, or report what blocks it.
///
/// This is the only place the Settings shell turns bytes into a base, so a
/// document the shell cannot edit is reported as sorted diagnostics rather than
/// as a half-built draft. A schema-1 document loads as its in-memory schema-2
/// view; nothing is rewritten by reading.
///
/// # Errors
///
/// Returns the sorted diagnostics that make the document uneditable.
pub fn load_settings_base(
    bytes: Option<&[u8]>,
    catalog: &OwnerCatalog,
) -> Result<SettingsMigration, Vec<Diagnostic>> {
    super::migration::migrate_settings(bytes.unwrap_or(EMPTY_SCHEMA_2), catalog).map_err(sorted)
}

/// One host-owned settings leaf the Settings shell may edit.
///
/// Diagnostics owns no leaf: it reports what the document says and never
/// changes it.
///
/// The registry-editor leaves (issue #388) carry the identity they name rather
/// than being separate variants per owner: an agent, a screen, or an
/// action/context pair is decided at runtime by what the registries hold, and
/// the identity types have already proved their own grammar, so an ill-formed
/// path stays unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxPath {
    /// `appearance.theme` — the active theme slug.
    Theme,
    /// `appearance.override_agent_theme` — apply the Jefe theme to agent output.
    OverrideAgentTheme,
    /// `workbench.initial_screen` — the screen a session opens on.
    InitialScreen,
    /// `workbench.enabled_screens` — the screens composition includes.
    EnabledScreens,
    /// `workbench.screen_order` — the order enabled screens are presented in.
    ScreenOrder,
    /// `agents.<id>.enabled` — whether one agent type is offered.
    AgentEnabled(Id),
    /// `workbench.layout_overrides.<id>` — one screen's whole layout tree.
    LayoutOverride(Id),
    /// `keymap.<context>.<action>` — one action's whole chord list.
    Keymap {
        /// The input context the binding applies in.
        context: ContextId,
        /// The action the binding dispatches.
        action: ActionId,
    },
}

impl SyntaxPath {
    /// Every editable leaf that names no runtime identity, in section then
    /// declaration order.
    ///
    /// The registry-editor leaves are deliberately absent: they are one leaf
    /// per known agent, screen, or action/context pair, so there is no finite
    /// list of them independent of the registries.
    pub const HOST_LEAVES: [Self; 5] = [
        Self::Theme,
        Self::OverrideAgentTheme,
        Self::InitialScreen,
        Self::EnabledScreens,
        Self::ScreenOrder,
    ];

    /// The decoded path of this leaf in the settings document.
    #[must_use]
    pub fn segments(&self) -> Vec<&str> {
        match self {
            Self::Theme => vec!["appearance", "theme"],
            Self::OverrideAgentTheme => vec!["appearance", "override_agent_theme"],
            Self::InitialScreen => vec!["workbench", "initial_screen"],
            Self::EnabledScreens => vec!["workbench", "enabled_screens"],
            Self::ScreenOrder => vec!["workbench", "screen_order"],
            Self::AgentEnabled(agent) => vec!["agents", agent.as_str(), "enabled"],
            Self::LayoutOverride(screen) => {
                vec!["workbench", "layout_overrides", screen.as_str()]
            }
            Self::Keymap { context, action } => {
                vec!["keymap", context.as_str(), action.as_str()]
            }
        }
    }

    /// The canonical diagnostic path of this leaf.
    #[must_use]
    pub fn diagnostic_path(&self) -> String {
        let mut path = String::new();
        for segment in self.segments() {
            path.push('/');
            path.push_str(segment);
        }
        path
    }

    /// Whether a saved change to this leaf only takes effect after a restart.
    ///
    /// The theme and the agent-theme override are cosmetic and apply to the
    /// running process. Everything else is read once while the session builds a
    /// registry — the first screen instance, the composed screen registry, the
    /// agent type registry, the action registry — so changing it cannot move a
    /// session that has already started.
    #[must_use]
    pub const fn structural(&self) -> bool {
        match self {
            Self::Theme | Self::OverrideAgentTheme => false,
            Self::InitialScreen
            | Self::EnabledScreens
            | Self::ScreenOrder
            | Self::AgentEnabled(_)
            | Self::LayoutOverride(_)
            | Self::Keymap { .. } => true,
        }
    }

    /// Whether this leaf holds a whole subtree rather than one scalar value.
    ///
    /// A subtree can legitimately be written either as one inline value or as
    /// its own `[table]` block, so replacing it has to consider both spellings;
    /// a scalar leaf only ever has the one.
    const fn is_subtree(&self) -> bool {
        matches!(self, Self::LayoutOverride(_))
    }

    /// The exact header written when this leaf's table is absent.
    fn table_header(&self) -> String {
        match self {
            Self::Theme | Self::OverrideAgentTheme => "[appearance]".to_owned(),
            Self::InitialScreen | Self::EnabledScreens | Self::ScreenOrder => {
                "[workbench]".to_owned()
            }
            Self::AgentEnabled(agent) => format!("[agents.{}]", quoted_key(agent.as_str())),
            Self::LayoutOverride(_) => "[workbench.layout_overrides]".to_owned(),
            Self::Keymap { context, .. } => {
                format!("[keymap.{}]", quoted_key(context.as_str()))
            }
        }
    }

    /// The exact key text written when this leaf's assignment is absent.
    fn key_text(&self) -> String {
        match self {
            Self::Theme => "theme".to_owned(),
            Self::OverrideAgentTheme => "override_agent_theme".to_owned(),
            Self::InitialScreen => "initial_screen".to_owned(),
            Self::EnabledScreens => "enabled_screens".to_owned(),
            Self::ScreenOrder => "screen_order".to_owned(),
            Self::AgentEnabled(_) => "enabled".to_owned(),
            Self::LayoutOverride(screen) => quoted_key(screen.as_str()),
            Self::Keymap { action, .. } => quoted_key(action.as_str()),
        }
    }
}

/// One dotted-path component written as a quoted TOML key.
///
/// Every identity this writes contains a `.`, which is a path separator in bare
/// key syntax, so quoting is what keeps `core.llxprt` one owner rather than an
/// owner named `llxprt` inside a table named `core`.
fn quoted_key(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

/// One closed lossless edit applied to a complete settings candidate.
///
/// Each variant carries the value type its leaf holds, so a candidate cannot be
/// asked to write a string where the schema declares a boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEdit {
    /// Select the active theme.
    Theme(ThemeId),
    /// Apply the Jefe theme to embedded agent terminal output, or stop doing so.
    OverrideAgentTheme(bool),
    /// Select the screen a session opens on.
    InitialScreen(Id),
    /// Replace the whole set of screens composition includes.
    EnabledScreens(Vec<Id>),
    /// Replace the whole order enabled screens are presented in.
    ScreenOrder(Vec<Id>),
    /// Offer one agent type, or stop offering it.
    AgentEnabled {
        /// The agent type this writes.
        agent: Id,
        /// Whether the type is offered.
        enabled: bool,
    },
    /// Replace one screen's whole layout tree.
    ///
    /// The tree is boxed because a layout is far larger than every other edit,
    /// and an edit travels by value through the draft and the message bus.
    ReplaceLayout {
        /// The screen whose layout this overrides.
        screen: Id,
        /// The complete tree to write.
        layout: Box<LayoutNode>,
    },
    /// Replace one action's whole chord list; an empty list unbinds it.
    Keymap {
        /// The input context the binding applies in.
        context: ContextId,
        /// The action the binding dispatches.
        action: ActionId,
        /// The canonical chords, in order.
        chords: Vec<Chord>,
    },
    /// Remove the source assignment so the compiled default is inherited.
    Reset(SyntaxPath),
}

impl SettingsEdit {
    /// The leaf this edit writes.
    #[must_use]
    pub fn path(&self) -> SyntaxPath {
        match self {
            Self::Theme(_) => SyntaxPath::Theme,
            Self::OverrideAgentTheme(_) => SyntaxPath::OverrideAgentTheme,
            Self::InitialScreen(_) => SyntaxPath::InitialScreen,
            Self::EnabledScreens(_) => SyntaxPath::EnabledScreens,
            Self::ScreenOrder(_) => SyntaxPath::ScreenOrder,
            Self::AgentEnabled { agent, .. } => SyntaxPath::AgentEnabled(agent.clone()),
            Self::ReplaceLayout { screen, .. } => SyntaxPath::LayoutOverride(screen.clone()),
            Self::Keymap {
                context, action, ..
            } => SyntaxPath::Keymap {
                context: context.clone(),
                action: action.clone(),
            },
            Self::Reset(path) => path.clone(),
        }
    }

    /// The exact TOML value text to write, or `None` to remove the assignment.
    fn rendered(&self) -> Option<Vec<u8>> {
        match self {
            Self::Theme(theme) => Some(toml_string(theme.as_str())),
            Self::OverrideAgentTheme(flag) => Some(flag.to_string().into_bytes()),
            Self::InitialScreen(screen) => Some(toml_string(screen.as_str())),
            Self::EnabledScreens(screens) | Self::ScreenOrder(screens) => {
                Some(toml_string_array(screens.iter().map(Id::as_str)))
            }
            Self::AgentEnabled { enabled, .. } => Some(enabled.to_string().into_bytes()),
            Self::ReplaceLayout { layout, .. } => Some(super::settings_layout::render(layout)),
            Self::Keymap { chords, .. } => Some(toml_string_array(
                chords.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )),
            Self::Reset(_) => None,
        }
    }
}

fn toml_string(value: &str) -> Vec<u8> {
    toml::Value::String(value.to_owned())
        .to_string()
        .into_bytes()
}

fn toml_string_array<I>(values: I) -> Vec<u8>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    toml::Value::Array(
        values
            .into_iter()
            .map(|value| toml::Value::String(value.as_ref().to_owned()))
            .collect(),
    )
    .to_string()
    .into_bytes()
}

/// A complete, validated settings document candidate.
#[derive(Debug, Clone)]
pub struct SettingsCandidate {
    bytes: Vec<u8>,
    hash: Sha256,
    expected: ExpectedHash,
    published: PublishedSettings,
    structural: bool,
}

impl SettingsCandidate {
    /// Build one complete candidate from a loaded document and typed edits.
    ///
    /// A schema-1 base is first rendered as the explicit schema-2 document the
    /// migration view describes, so saving a schema-1 file is the one moment it
    /// becomes schema 2 and dormant syntax survives the transition.
    ///
    /// # Errors
    ///
    /// Returns the sorted diagnostics that block the complete candidate,
    /// including type and ownership errors a person hand-wrote into the file.
    pub fn from_edits(
        migration: &SettingsMigration,
        catalog: &OwnerCatalog,
        edits: &[SettingsEdit],
        expected: ExpectedHash,
    ) -> Result<Self, Vec<Diagnostic>> {
        let mut bytes = if migration.was_migrated() {
            format_migrated_settings(migration, catalog)?
        } else {
            migration.document().original_bytes().to_vec()
        };
        for edit in edits {
            bytes = patch(&bytes, &edit.path(), edit.rendered().as_deref())?;
        }
        let parsed = SettingsDocument::parse(&bytes).map_err(|diagnostic| vec![*diagnostic])?;
        let published = parsed.publish(catalog).map_err(sorted)?;
        Ok(Self {
            hash: parsed.sha256(),
            bytes,
            expected,
            published,
            structural: edits.iter().any(|edit| edit.path().structural()),
        })
    }

    /// Borrow the exact bytes a save would make authoritative.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The digest of this candidate's own bytes.
    #[must_use]
    pub const fn sha256(&self) -> Sha256 {
        self.hash
    }

    /// Borrow the typed settings this candidate publishes.
    #[must_use]
    pub const fn published(&self) -> &PublishedSettings {
        &self.published
    }

    /// Whether any edit in this candidate only takes effect after a restart.
    #[must_use]
    pub const fn structural(&self) -> bool {
        self.structural
    }
}

/// Write one leaf into `bytes`, or remove it, preserving every other byte.
///
/// A subtree leaf is written in two steps: whatever block spells it today is
/// removed first, and the replacement is then inserted like any other
/// assignment. Doing it in one step would have to reconcile two spellings of
/// the same tree at once, and getting that wrong writes a document with the
/// same key defined twice.
fn patch(
    bytes: &[u8],
    path: &SyntaxPath,
    value: Option<&[u8]>,
) -> Result<Vec<u8>, Vec<Diagnostic>> {
    let mut bytes = bytes.to_vec();
    if path.is_subtree() {
        let document = SettingsDocument::parse(&bytes).map_err(|diagnostic| vec![*diagnostic])?;
        bytes = remove_table_block(&document, &path.segments());
    }
    let document = SettingsDocument::parse(&bytes).map_err(|diagnostic| vec![*diagnostic])?;
    let segments = path.segments();
    let table_header = path.table_header();
    let key_text = path.key_text();
    patch_assignment(
        &document,
        &Assignment {
            path: &segments,
            table_header: &table_header,
            key_text: &key_text,
        },
        value,
    )
    .map_err(|_| vec![inline_ancestor_diagnostic(path)])
}

fn sorted(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort();
    diagnostics
}

/// The refusal for a leaf whose owning table is written as one inline value.
fn inline_ancestor_diagnostic(path: &SyntaxPath) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E006,
        Severity::Error,
        DiagnosticPath::new(path.diagnostic_path()),
        None,
        "rewrite the owning table as a [table] header, or edit this value in the file",
    );
    "the owning table is written as an inline table, which has no syntax for this leaf"
        .clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}

/// What a durable settings save did.
///
/// The four outcomes are the four the Settings shell must offer different
/// recoveries for, which is why a conflict is not folded into a failure: a
/// conflict means the file is fine and newer, and a failure means the write did
/// not happen at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSaveOutcome {
    /// The candidate is now authoritative at this revision.
    Written {
        /// The revision that was made authoritative.
        revision: u64,
        /// The digest of the bytes now on disk.
        hash: Sha256,
    },
    /// A newer revision was scheduled before this one reached replacement.
    Superseded {
        /// The revision that was abandoned.
        revision: u64,
    },
    /// The target changed since the draft was bound to it; nothing was written.
    Conflict {
        /// The revision that was attempted.
        revision: u64,
        /// The digest of the bytes now on disk, when they could be read.
        disk_hash: Option<Sha256>,
    },
    /// The durable write failed; the target is unchanged and the draft intact.
    Failed {
        /// The revision that was attempted.
        revision: u64,
        /// The typed reason, already redacted.
        diagnostic: Box<Diagnostic>,
    },
}

impl SettingsSaveOutcome {
    /// The revision this outcome answers for.
    ///
    /// Every outcome names one, including the two that never reached
    /// replacement: without it the shell could not tell a conflict answering
    /// the save the user is waiting on from one answering a save they have
    /// already replaced.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        match self {
            Self::Written { revision, .. }
            | Self::Superseded { revision }
            | Self::Conflict { revision, .. }
            | Self::Failed { revision, .. } => *revision,
        }
    }
}

impl FilePersistenceManager {
    /// Make one validated settings candidate authoritative.
    ///
    /// The freshness callback is consulted immediately before replacement, so a
    /// revision superseded while the temporary file was being written is
    /// abandoned rather than making stale bytes authoritative.
    pub fn save_settings_candidate_revisioned(
        &self,
        candidate: &SettingsCandidate,
        revision: u64,
        freshness: &crate::services::persist_worker::FreshnessFn,
    ) -> SettingsSaveOutcome {
        let target = self.paths_ref().settings_path.clone();
        let outcome = writer::write(
            AtomicWrite {
                target: target.clone(),
                draft: DraftBytes::new(candidate.bytes.clone()),
                expected: candidate.expected,
                revision,
                backup: writer::BackupPolicy::None,
            },
            freshness,
        );
        match outcome {
            Ok(writer::WriteOutcome::Authoritative { revision, hash }) => {
                SettingsSaveOutcome::Written { revision, hash }
            }
            Ok(writer::WriteOutcome::Stale { revision }) => {
                SettingsSaveOutcome::Superseded { revision }
            }
            Err(error) if error.diagnostic().code == CfgCode::E007 => {
                SettingsSaveOutcome::Conflict {
                    revision,
                    disk_hash: read_digest(&target),
                }
            }
            Err(error) => SettingsSaveOutcome::Failed {
                revision,
                diagnostic: Box::new(error.diagnostic().clone()),
            },
        }
    }

    /// The directory settings exports are contained in.
    #[must_use]
    pub fn export_directory(&self) -> PathBuf {
        export_directory(self.paths_ref())
    }
}

fn export_directory(paths: &PersistencePaths) -> PathBuf {
    paths
        .settings_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn read_digest(path: &Path) -> Option<Sha256> {
    std::fs::read(path).ok().map(|bytes| Sha256::digest(&bytes))
}

/// A relative export target contained under the configuration directory.
///
/// Export is the escape hatch a user reaches for when the file on disk is not
/// theirs any more, so it must not be able to reach outside the directory that
/// escape hatch belongs to. Containment is a property of the value, checked
/// once at parse time, rather than a check every caller has to remember.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPath(PathBuf);

impl ExportPath {
    /// Parse a relative, contained export path.
    ///
    /// # Errors
    ///
    /// Returns `CFG-E101`-shaped path diagnostics for an empty, absolute, or
    /// escaping path, and `CFG-E008` when the path exceeds the encoded limit.
    pub fn parse(text: &str) -> Result<Self, Box<Diagnostic>> {
        if text.is_empty() {
            return Err(path_diagnostic("export path is empty"));
        }
        if text.len() > super::diagnostic::PATH_LIMIT {
            return Err(limit_diagnostic("export path exceeds the encoded limit"));
        }
        let candidate = Path::new(text);
        let mut normal = PathBuf::new();
        for component in candidate.components() {
            match component {
                Component::Normal(segment) => normal.push(segment),
                Component::CurDir | Component::ParentDir => {
                    return Err(path_diagnostic(
                        "export path must not contain '.' or '..' segments",
                    ));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(path_diagnostic("export path must be relative"));
                }
            }
        }
        if normal.as_os_str().is_empty() {
            return Err(path_diagnostic("export path names no file"));
        }
        Ok(Self(normal))
    }

    /// Borrow the normalized relative path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Write a redacted canonical representation of one draft to a contained path.
///
/// The export never becomes authoritative and never touches the settings
/// target, so it changes no base, hash, or dirty status. It refuses to replace
/// an existing file: the whole point of exporting during a conflict is to not
/// lose bytes, and silently overwriting a previous export would do exactly that.
///
/// # Errors
///
/// Returns the typed diagnostic describing why the export did not happen. The
/// draft is unaffected in every failure case.
pub fn export_candidate(
    candidate: &SettingsCandidate,
    directory: &Path,
    relative: &ExportPath,
    catalog: &OwnerCatalog,
) -> Result<PathBuf, Box<Diagnostic>> {
    let target = directory.join(relative.as_path());
    let document =
        SettingsDocument::parse(&candidate.bytes).map_err(|diagnostic| Box::new(*diagnostic))?;
    let canonical = document
        .format_owned(catalog)
        .map_err(|diagnostics| Box::new(first_diagnostic(diagnostics)))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| write_diagnostic(&target, &error))?;
    }
    let mut file = create_user_only(&target).map_err(|error| write_diagnostic(&target, &error))?;
    write_and_sync(&mut file, &canonical).map_err(|error| {
        drop(file);
        let _ = std::fs::remove_file(&target);
        write_diagnostic(&target, &error)
    })?;
    Ok(target)
}

fn write_and_sync(file: &mut std::fs::File, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(unix)]
fn create_user_only(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// Mirrors the `unix` arm so the caller stays platform-agnostic.
///
/// These platforms have no mode bits to set at creation, so the export inherits
/// whatever the containing directory grants. `create_new` still guarantees the
/// two properties the export depends on: the file is this process's own, and no
/// existing file is replaced.
#[cfg(not(unix))]
fn create_user_only(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

fn first_diagnostic(diagnostics: Vec<Diagnostic>) -> Diagnostic {
    sorted(diagnostics).into_iter().next().unwrap_or_else(|| {
        let mut diagnostic = Diagnostic::new(
            CfgCode::E103,
            Severity::Error,
            DiagnosticPath::root(),
            None,
            "correct the selected settings document",
        );
        "settings validation failed without a diagnostic"
            .clone_into(&mut diagnostic.redacted_detail);
        diagnostic
    })
}

fn path_diagnostic(detail: &str) -> Box<Diagnostic> {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E103,
        Severity::Error,
        DiagnosticPath::new("/export"),
        None,
        "choose a relative path inside the configuration directory",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    Box::new(diagnostic)
}

fn limit_diagnostic(detail: &str) -> Box<Diagnostic> {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E008,
        Severity::Error,
        DiagnosticPath::new("/export"),
        None,
        "reduce the value to the documented inclusive limit",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    Box::new(diagnostic)
}

fn write_diagnostic(path: &Path, error: &std::io::Error) -> Box<Diagnostic> {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E104,
        Severity::Error,
        DiagnosticPath::new(path.to_string_lossy()),
        None,
        "retain the draft and choose a writable export path",
    );
    diagnostic.redacted_detail = error.to_string();
    Box::new(diagnostic)
}
