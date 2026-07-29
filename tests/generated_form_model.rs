//! Behavioral tests for issue #382 S5's generated typed form model.

use jefe::domain::agent_definition::{
    AgentDefinition, Availability, Field, FieldKind, FieldValue, ProbeErrorCode,
};
use jefe::state::generated_form::{
    FormEditError, FormFieldDisabledReason, FormFieldId, FormIntent, FormValidationProblem,
    GeneratedFormDraft,
};

fn field(id: &str, kind: FieldKind) -> Field {
    Field {
        id: id.to_string(),
        kind,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: Vec::new(),
        visible_when: None,
        launch_signature: false,
    }
}

fn comprehensive_definition() -> AgentDefinition {
    let mut definition = AgentDefinition::shipped().remove(0);

    let mut gate = field("advanced", FieldKind::Boolean);
    gate.default = Some(FieldValue::Boolean(true));

    let mut optional = field("confirmation", FieldKind::OptionalBoolean);
    optional.default = Some(FieldValue::OptionalBoolean(None));

    let mut name = field("display_name", FieldKind::String);
    name.required = true;
    name.launch_signature = true;

    let mut retries = field("retries", FieldKind::Integer);
    retries.default = Some(FieldValue::Integer(2));
    retries.minimum = Some(1);
    retries.maximum = Some(3);

    let mut mode = field("mode", FieldKind::Enum);
    mode.default = Some(FieldValue::String("safe".to_string()));
    mode.choices = vec!["safe".to_string(), "fast".to_string()];

    let mut workspace = field("workspace", FieldKind::Path);
    workspace.default = Some(FieldValue::Path("/tmp/work".to_string()));

    let mut tags = field("tags", FieldKind::StringList);
    tags.default = Some(FieldValue::StringList(vec!["one".to_string()]));

    let mut notes = field("notes", FieldKind::String);
    notes.visible_when = Some("advanced".to_string());
    notes.default = Some(FieldValue::String("remember me".to_string()));
    notes.launch_signature = true;

    let mut detail = field("detail", FieldKind::String);
    detail.visible_when = Some("notes".to_string());
    detail.default = Some(FieldValue::String("nested value".to_string()));

    definition.repository_fields = vec![
        gate, optional, name, retries, mode, workspace, tags, detail, notes,
    ];
    definition
        .candidates
        .retain(|candidate| !candidate.kind.is_package_runner());
    definition.agent_fields = vec![field("agent_note", FieldKind::String)];
    definition.emitters.clear();
    definition
}

fn compatible() -> Availability {
    Availability::InstalledCompatible {
        identity: "fixture".to_string(),
        capabilities: vec![
            "profile".to_string(),
            "prompt-interactive".to_string(),
            "yolo".to_string(),
        ],
        generation: 7,
    }
}

#[test]
fn definition_generates_all_typed_fields_in_declaration_order() {
    let definition = comprehensive_definition();
    let draft = GeneratedFormDraft::from_definition(&definition, &compatible());
    let Ok(draft) = draft else {
        panic!("validated definition should generate a form: {draft:?}");
    };

    let ids: Vec<FormFieldId> = draft
        .fields()
        .iter()
        .map(|field| field.id().clone())
        .collect();
    assert_eq!(
        ids,
        vec![
            FormFieldId::repository("advanced"),
            FormFieldId::repository("confirmation"),
            FormFieldId::repository("display_name"),
            FormFieldId::repository("retries"),
            FormFieldId::repository("mode"),
            FormFieldId::repository("workspace"),
            FormFieldId::repository("tags"),
            FormFieldId::repository("detail"),
            FormFieldId::repository("notes"),
            FormFieldId::agent("agent_note"),
        ]
    );

    let retries = draft.field(&FormFieldId::repository("retries"));
    let Some(retries) = retries else {
        panic!("integer field should be generated");
    };
    assert_eq!(retries.value(), &FieldValue::Integer(2));
    assert_eq!(retries.minimum(), Some(1));
    assert_eq!(retries.maximum(), Some(3));

    let mode = draft.field(&FormFieldId::repository("mode"));
    let Some(mode) = mode else {
        panic!("enum field should be generated");
    };
    assert_eq!(mode.label(), "Mode");
    assert_eq!(mode.choices(), &["safe".to_string(), "fast".to_string()]);

    let display_name = draft.field(&FormFieldId::repository("display_name"));
    let Some(display_name) = display_name else {
        panic!("string field should be generated");
    };
    assert_eq!(display_name.label(), "Display Name");
    assert!(display_name.required());
    assert!(display_name.launch_signature());
}

fn reduce_or_panic(draft: GeneratedFormDraft, intent: FormIntent) -> GeneratedFormDraft {
    let reduced = draft.reduce(intent);
    let Ok(reduced) = reduced else {
        panic!("generated form transition should succeed: {reduced:?}");
    };
    reduced
}

fn assert_hidden_values_preserved_and_inactive(draft: &GeneratedFormDraft) {
    let notes = draft.field(&FormFieldId::repository("notes"));
    let Some(notes) = notes else {
        panic!("hidden field should remain in the draft");
    };
    assert!(!notes.visible());
    assert_eq!(notes.value(), &FieldValue::String("user value".to_string()));
    let detail = draft.field(&FormFieldId::repository("detail"));
    assert!(detail.is_some_and(|field| !field.visible()));
    assert!(
        !draft
            .active_values()
            .iter()
            .any(|value| value.id() == &FormFieldId::repository("notes"))
    );
    assert!(
        !draft
            .launch_signature_values()
            .iter()
            .any(|value| value.id() == &FormFieldId::repository("notes"))
    );
}

#[test]
fn reducer_preserves_hidden_values_and_projects_deterministic_focus_and_emission() {
    let definition = comprehensive_definition();
    let draft = GeneratedFormDraft::from_definition(&definition, &compatible());
    let Ok(draft) = draft else {
        panic!("validated definition should generate a form: {draft:?}");
    };

    let draft = reduce_or_panic(
        draft,
        FormIntent::SetValue {
            field: FormFieldId::repository("display_name"),
            value: FieldValue::String("typed name".to_string()),
        },
    );
    let draft = reduce_or_panic(draft, FormIntent::Focus(FormFieldId::repository("notes")));
    let draft = reduce_or_panic(
        draft,
        FormIntent::SetValue {
            field: FormFieldId::repository("notes"),
            value: FieldValue::String("user value".to_string()),
        },
    );
    let draft = reduce_or_panic(
        draft,
        FormIntent::Toggle {
            field: FormFieldId::repository("advanced"),
        },
    );

    assert_hidden_values_preserved_and_inactive(&draft);

    let visible_ids = draft.visible_field_ids();
    assert_eq!(draft.focused(), visible_ids.first());
    let draft = reduce_or_panic(draft, FormIntent::FocusNext);
    assert_eq!(draft.focused(), visible_ids.get(1));

    let draft = reduce_or_panic(
        draft,
        FormIntent::Toggle {
            field: FormFieldId::repository("advanced"),
        },
    );
    let notes = draft.field(&FormFieldId::repository("notes"));
    let Some(notes) = notes else {
        panic!("reshown field should remain in the draft");
    };
    assert!(notes.visible());
    assert_eq!(notes.value(), &FieldValue::String("user value".to_string()));
}

#[test]
fn typed_validation_covers_required_bounds_choices_kind_and_unknown_ids() {
    let definition = comprehensive_definition();
    let draft = GeneratedFormDraft::from_definition(&definition, &compatible());
    let Ok(draft) = draft else {
        panic!("validated definition should generate a form: {draft:?}");
    };

    assert!(draft.validation_issues().iter().any(|issue| {
        issue.field() == &FormFieldId::repository("display_name")
            && matches!(issue.problem(), FormValidationProblem::Required)
    }));

    let wrong_kind = draft.clone().reduce(FormIntent::SetValue {
        field: FormFieldId::repository("retries"),
        value: FieldValue::String("2".to_string()),
    });
    assert!(matches!(
        wrong_kind,
        Err(FormEditError::KindMismatch { .. })
    ));

    let unknown = draft.clone().reduce(FormIntent::SetValue {
        field: FormFieldId::repository("missing"),
        value: FieldValue::String("not accepted".to_string()),
    });
    assert!(matches!(unknown, Err(FormEditError::UnknownField { .. })));

    let draft = draft.reduce(FormIntent::SetValue {
        field: FormFieldId::repository("display_name"),
        value: FieldValue::String("valid".to_string()),
    });
    let Ok(draft) = draft else {
        panic!("required string edit should succeed: {draft:?}");
    };
    let draft = draft.reduce(FormIntent::SetValue {
        field: FormFieldId::repository("retries"),
        value: FieldValue::Integer(9),
    });
    let Ok(draft) = draft else {
        panic!("out-of-range draft should remain editable: {draft:?}");
    };
    let draft = draft.reduce(FormIntent::SetValue {
        field: FormFieldId::repository("mode"),
        value: FieldValue::String("unknown".to_string()),
    });
    let Ok(draft) = draft else {
        panic!("invalid enum draft should remain editable: {draft:?}");
    };

    let issues = draft.validation_issues();
    assert!(issues.iter().any(|issue| matches!(
        issue.problem(),
        FormValidationProblem::AboveMaximum {
            maximum: 3,
            actual: 9
        }
    )));
    assert!(issues.iter().any(|issue| matches!(
        issue.problem(),
        FormValidationProblem::InvalidChoice { value } if value == "unknown"
    )));
}

#[test]
fn unavailable_capability_fields_remain_visible_with_typed_disabled_reasons() {
    let definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.codex");
    let Some(definition) = definition else {
        panic!("shipped definition should exist");
    };
    let unavailable = Availability::ProbeError {
        code: ProbeErrorCode::Agte202,
        reason: "invalid probe stream".to_string(),
        generation: 11,
    };
    let draft = GeneratedFormDraft::from_definition(&definition, &unavailable);
    let Ok(draft) = draft else {
        panic!("unavailable definition should still generate a form: {draft:?}");
    };

    let model_id = FormFieldId::repository("model");
    let model = draft.field(&model_id);
    let Some(model) = model else {
        panic!("capability-backed field should remain present");
    };
    assert!(model.visible());
    assert!(matches!(
        model.disabled_reason(),
        Some(FormFieldDisabledReason::ProbeError {
            capability,
            code: ProbeErrorCode::Agte202,
            reason,
        }) if capability == "model" && reason == "invalid probe stream"
    ));
    assert!(draft.visible_field_ids().contains(&model_id));

    let edit = draft.reduce(FormIntent::SetValue {
        field: model_id,
        value: FieldValue::String("new-model".to_string()),
    });
    assert!(matches!(edit, Err(FormEditError::DisabledField { .. })));
}

#[test]
fn reducer_edits_remaining_typed_kinds_and_reports_lower_bound() {
    let definition = comprehensive_definition();
    let draft = GeneratedFormDraft::from_definition(&definition, &compatible());
    let Ok(draft) = draft else {
        panic!("validated definition should generate a form: {draft:?}");
    };
    let draft = reduce_or_panic(
        draft,
        FormIntent::Toggle {
            field: FormFieldId::repository("confirmation"),
        },
    );
    assert_eq!(
        draft
            .field(&FormFieldId::repository("confirmation"))
            .map(jefe::state::generated_form::GeneratedFormField::value),
        Some(&FieldValue::OptionalBoolean(Some(true)))
    );
    let draft = reduce_or_panic(
        draft,
        FormIntent::SetValue {
            field: FormFieldId::repository("workspace"),
            value: FieldValue::Path("/srv/repo".to_string()),
        },
    );
    let draft = reduce_or_panic(
        draft,
        FormIntent::SetValue {
            field: FormFieldId::repository("tags"),
            value: FieldValue::StringList(vec!["red".to_string(), "green".to_string()]),
        },
    );
    let draft = reduce_or_panic(
        draft,
        FormIntent::SetValue {
            field: FormFieldId::repository("retries"),
            value: FieldValue::Integer(0),
        },
    );
    assert!(draft.validation_issues().iter().any(|issue| matches!(
        issue.problem(),
        FormValidationProblem::BelowMinimum {
            minimum: 1,
            actual: 0
        }
    )));
    assert_eq!(
        draft
            .field(&FormFieldId::repository("tags"))
            .map(jefe::state::generated_form::GeneratedFormField::cursor),
        Some(2)
    );
}

// ---------------------------------------------------------------------------
// Issue #519: LLxprt default YOLO and independent Continue field
// ---------------------------------------------------------------------------

fn llxprt_shipped() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.llxprt")
        .unwrap_or_else(|| panic!("LLxprt definition must be shipped"))
}

fn llxprt_compatible() -> Availability {
    Availability::InstalledCompatible {
        identity: "0.10.0".to_string(),
        capabilities: vec![
            "prompt-interactive".to_string(),
            "profile".to_string(),
            "yolo".to_string(),
            "continue".to_string(),
        ],
        generation: 1,
    }
}

/// A newly generated LLxprt form must default repository `yolo` to true so the
/// product-specific launch default is restored without touching other agents.
#[test]
fn llxprt_generated_form_defaults_yolo_to_true() {
    let definition = llxprt_shipped();
    let draft = GeneratedFormDraft::from_definition(&definition, &llxprt_compatible());
    let Ok(draft) = draft else {
        panic!("LLxprt definition should generate a form: {draft:?}");
    };
    let yolo = draft.field(&FormFieldId::repository("yolo"));
    let Some(yolo) = yolo else {
        panic!("LLxprt form must expose a repository yolo field");
    };
    assert_eq!(yolo.value(), &FieldValue::Boolean(true));
}

/// LLxprt must declare an independent agent-scope `continue` boolean field
/// distinct from `prompt_interactive`.
#[test]
fn llxprt_generated_form_exposes_independent_continue_field() {
    let definition = llxprt_shipped();
    assert!(
        definition
            .agent_fields
            .iter()
            .any(|field| field.id == "continue" && field.kind == FieldKind::Boolean),
        "LLxprt must declare an agent-scope continue boolean field"
    );
    let draft = GeneratedFormDraft::from_definition(&definition, &llxprt_compatible());
    let Ok(draft) = draft else {
        panic!("LLxprt definition should generate a form: {draft:?}");
    };
    let continue_field = draft.field(&FormFieldId::agent("continue"));
    let Some(continue_field) = continue_field else {
        panic!("LLxprt form must expose an agent continue field");
    };
    assert_eq!(continue_field.value(), &FieldValue::Boolean(false));
    // prompt_interactive remains independent and present.
    assert!(
        draft
            .field(&FormFieldId::agent("prompt_interactive"))
            .is_some()
    );
}
