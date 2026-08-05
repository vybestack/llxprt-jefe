//! Behavioral tests for issue #382 S5's generated typed form model.

use jefe::domain::agent_definition::{AgentDefinition, Field, FieldKind, FieldValue};
use jefe::state::generated_form::{
    FormEditError, FormFieldId, FormIntent, FormValidationProblem, GeneratedFormDraft,
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

#[test]
fn definition_generates_all_typed_fields_in_declaration_order() {
    let definition = comprehensive_definition();
    let draft = GeneratedFormDraft::from_definition(&definition);
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
    let draft = GeneratedFormDraft::from_definition(&definition);
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
    let draft = GeneratedFormDraft::from_definition(&definition);
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
fn reducer_edits_remaining_typed_kinds_and_reports_lower_bound() {
    let definition = comprehensive_definition();
    let draft = GeneratedFormDraft::from_definition(&definition);
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

/// A newly generated LLxprt form must default repository `yolo` to true so the
/// product-specific launch default is restored without touching other agents.
#[test]
fn llxprt_generated_form_defaults_yolo_to_true() {
    let definition = llxprt_shipped();
    let draft = GeneratedFormDraft::from_definition(&definition);
    let Ok(draft) = draft else {
        panic!("LLxprt definition should generate a form: {draft:?}");
    };
    let yolo = draft.field(&FormFieldId::repository("yolo"));
    let Some(yolo) = yolo else {
        panic!("LLxprt form must expose a repository yolo field");
    };
    assert_eq!(yolo.value(), &FieldValue::Boolean(true));
}

/// LLxprt must declare an agent-scope `continue` boolean without exposing the
/// prompt-valued interactive option as a Boolean form field.
#[test]
fn llxprt_generated_form_exposes_only_the_continue_boolean() {
    let definition = llxprt_shipped();
    assert!(
        definition
            .agent_fields
            .iter()
            .any(|field| field.id == "continue" && field.kind == FieldKind::Boolean),
        "LLxprt must declare an agent-scope continue boolean field"
    );
    let draft = GeneratedFormDraft::from_definition(&definition);
    let Ok(draft) = draft else {
        panic!("LLxprt definition should generate a form: {draft:?}");
    };
    let continue_field = draft.field(&FormFieldId::agent("continue"));
    let Some(continue_field) = continue_field else {
        panic!("LLxprt form must expose an agent continue field");
    };
    assert_eq!(continue_field.value(), &FieldValue::Boolean(true));
    // prompt_interactive is a declared agent field with default true.
    let prompt_interactive_field = draft.field(&FormFieldId::agent("prompt_interactive"));
    let Some(prompt_interactive_field) = prompt_interactive_field else {
        panic!("LLxprt form must expose a prompt_interactive field");
    };
    assert_eq!(prompt_interactive_field.value(), &FieldValue::Boolean(true));
}
