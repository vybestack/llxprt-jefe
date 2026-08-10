fn pending_revision(state: &AppState) -> u64 {
    state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::pending_revision)
        .unwrap_or_else(|| panic!("a scheduled save carries a revision"))
}

fn written(revision: u64, state: &AppState) -> SettingsSaveOutcome {
    let Some(candidate) = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::candidate_bytes)
    else {
        panic!("a saveable draft has a candidate");
    };
    SettingsSaveOutcome::Written {
        revision,
        hash: candidate.sha256(),
    }
}

fn complete(state: &mut AppState, outcome: SettingsSaveOutcome) {
    apply(state, SettingsMessage::SaveCompleted(Box::new(outcome)));
}

/// Answer the scheduled save as the writer would after a successful write.
fn complete_written(state: &mut AppState) {
    let revision = pending_revision(state);
    let outcome = written(revision, state);
    complete(state, outcome);
}

fn write_failure() -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E104,
        Severity::Error,
        DiagnosticPath::new("/tmp/jefe/settings.toml"),
        None,
        "preserve the draft and resolve the filesystem write failure",
    );
    "injected writer phase failure".clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}

#[test]
fn a_save_schedules_a_strictly_increasing_revision() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(&mut state, SettingsMessage::Save);
    let first = pending_revision(&state);
    complete_written(&mut state);

    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::OverrideAgentTheme(true)),
    );
    apply(&mut state, SettingsMessage::Save);
    let second = pending_revision(&state);

    assert!(second > first, "{second} must follow {first}");
}

#[test]
fn a_matching_completion_adopts_the_saved_bytes_as_the_new_base() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let revision = pending_revision(&state);
    let outcome = written(revision, &state);
    let SettingsSaveOutcome::Written { hash, .. } = outcome else {
        panic!("fixture outcome is a write");
    };

    complete(&mut state, SettingsSaveOutcome::Written { revision, hash });

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("a save keeps the draft");
    };
    assert_eq!(draft.status(), &DraftStatus::Clean);
    assert_eq!(draft.base_hash(), Some(hash));
    assert_eq!(draft.base_revision(), revision);
    assert_eq!(draft.edited_paths().count(), 0);
    assert_eq!(
        draft.published().appearance.theme.as_deref(),
        Some("dracula")
    );
}

#[test]
fn a_completion_for_a_superseded_revision_is_ignored() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let superseded_revision = pending_revision(&state);
    // The first attempt conflicts, the user retries, and only then does a late
    // answer for the first attempt arrive.
    complete(
        &mut state,
        SettingsSaveOutcome::Conflict {
            revision: superseded_revision,
            disk_hash: None,
        },
    );
    apply(&mut state, SettingsMessage::Save);
    let newest = pending_revision(&state);
    assert!(newest > superseded_revision);

    complete(
        &mut state,
        SettingsSaveOutcome::Written {
            revision: superseded_revision,
            hash: Sha256::digest(b"whatever was on disk then"),
        },
    );

    assert_eq!(
        draft_status(&state),
        DraftStatus::Saving { revision: newest },
        "the newest pending revision stands"
    );
    assert_eq!(pending_revision(&state), newest);
    assert!(
        state.settings_state.is_dirty(),
        "the superseded completion adopted nothing"
    );
}

// ── CW07-06: a hash conflict preserves disk and draft ────────────────────

#[test]
fn a_conflict_preserves_the_draft_and_offers_reload_export_and_retry() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let revision = pending_revision(&state);
    let disk_hash = Sha256::digest(b"someone else's settings");

    complete(
        &mut state,
        SettingsSaveOutcome::Conflict {
            revision,
            disk_hash: Some(disk_hash),
        },
    );

    assert_eq!(
        draft_status(&state),
        DraftStatus::Conflict {
            disk_hash: Some(disk_hash)
        }
    );
    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("a conflict keeps the draft");
    };
    assert_eq!(
        draft.published().appearance.theme.as_deref(),
        Some("dracula"),
        "the draft is preserved"
    );
    assert_eq!(
        settings_view::recovery_choices(&state.settings_state),
        vec![
            RecoveryChoice::Reload,
            RecoveryChoice::Export,
            RecoveryChoice::Retry
        ]
    );
}

#[test]
fn a_write_failure_offers_retry_export_and_discard() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let revision = pending_revision(&state);

    complete(
        &mut state,
        SettingsSaveOutcome::Failed {
            revision,
            diagnostic: Box::new(write_failure()),
        },
    );

    assert_eq!(
        draft_status(&state),
        DraftStatus::Failed {
            code: CfgCode::E104
        }
    );
    assert_eq!(
        settings_view::recovery_choices(&state.settings_state),
        vec![
            RecoveryChoice::Retry,
            RecoveryChoice::Export,
            RecoveryChoice::Discard
        ]
    );
}

#[test]
fn retrying_after_a_conflict_reschedules_the_same_draft() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    apply(&mut state, SettingsMessage::Save);
    let first = pending_revision(&state);
    complete(
        &mut state,
        SettingsSaveOutcome::Conflict {
            revision: first,
            disk_hash: None,
        },
    );

    apply(&mut state, SettingsMessage::Save);

    let retried = pending_revision(&state);
    assert!(retried > first);
    assert_eq!(
        draft_status(&state),
        DraftStatus::Saving { revision: retried }
    );
}

// ── CW07-07: reload rebuilds from the exact disk bytes ───────────────────

#[test]
fn a_dirty_reload_asks_before_it_discards_anything() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(&mut state, SettingsMessage::Reload);

    assert!(state.settings_state.reload_confirm);
    assert!(
        state.settings_state.is_dirty(),
        "asking discards nothing yet"
    );
}

#[test]
fn a_clean_reload_needs_no_confirmation() {
    let mut state = opened(Some(SCHEMA_2));

    apply(&mut state, SettingsMessage::Reload);

    assert!(!state.settings_state.reload_confirm);
}

#[test]
fn a_reload_rebuilds_the_draft_from_the_exact_current_bytes() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    let external =
        b"settings_schema = 2\n# somebody else was here\n[appearance]\ntheme = 'green-screen'\n";

    apply(
        &mut state,
        SettingsMessage::Reloaded(Box::new(source(Some(external)))),
    );

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("a reload binds a draft");
    };
    assert_eq!(draft.base().document().original_bytes(), external);
    assert_eq!(draft.base_hash(), Some(Sha256::digest(external)));
    assert!(!draft.is_dirty());
    assert!(!state.settings_state.reload_confirm);
}

// ── CW07-08: export leaves the draft exactly where it is ─────────────────

#[test]
fn an_export_result_changes_no_base_hash_or_dirty_status() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );
    let before_hash = state
        .settings_state
        .draft
        .as_ref()
        .and_then(super::SettingsDraft::base_hash);

    apply(
        &mut state,
        SettingsMessage::ExportCompleted(Box::new(Ok(PathBuf::from("/tmp/jefe/draft.toml")))),
    );

    let Some(draft) = state.settings_state.draft.as_ref() else {
        panic!("export keeps the draft");
    };
    assert_eq!(draft.base_hash(), before_hash);
    assert!(draft.is_dirty());
    assert!(
        state
            .settings_state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.contains("draft.toml"))
    );
}

#[test]
fn a_failed_export_retains_the_draft_and_reports_a_redacted_reason() {
    let mut state = opened(Some(SCHEMA_2));
    apply(
        &mut state,
        SettingsMessage::Edit(SettingsEdit::Theme(theme("dracula"))),
    );

    apply(
        &mut state,
        SettingsMessage::ExportCompleted(Box::new(Err(write_failure()))),
    );

    assert!(state.settings_state.is_dirty());
    assert!(
        state
            .settings_state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.starts_with("CFG-E104"))
    );
}
