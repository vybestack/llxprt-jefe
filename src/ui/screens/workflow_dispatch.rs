use iocraft::prelude::*;

use crate::state::{
    AppState, ModalState, WorkflowDispatchFormCursor, WorkflowDispatchFormFields,
    WorkflowDispatchFormFocus,
};
use crate::theme::ResolvedColors;

#[derive(Default, Props)]
pub struct WorkflowDispatchFormProps {
    pub state: Option<AppState>,
    pub colors: Option<crate::theme::ThemeColors>,
}

fn render_text_with_caret(value: &str, cursor: usize) -> String {
    let mut s = String::new();
    let chars: Vec<char> = value.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i == cursor {
            s.push('█');
        }
        s.push(c);
    }
    if cursor >= chars.len() {
        s.push('█');
    }
    s
}

#[component]
pub fn WorkflowDispatchForm(props: &WorkflowDispatchFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    let (workflow_name, fields, focus, cursor) = props.state.as_ref().map_or_else(
        || {
            (
                String::new(),
                WorkflowDispatchFormFields::default(),
                WorkflowDispatchFormFocus::default(),
                WorkflowDispatchFormCursor::default(),
            )
        },
        |state| match &state.modal {
            ModalState::WorkflowDispatch {
                workflow,
                fields,
                focus,
                cursor,
            } => (
                workflow.name.clone(),
                fields.clone(),
                *focus,
                cursor.clone(),
            ),
            _ => (
                String::new(),
                WorkflowDispatchFormFields::default(),
                WorkflowDispatchFormFocus::default(),
                WorkflowDispatchFormCursor::default(),
            ),
        },
    );

    let labels = ["Target Ref", "Inputs (k=v,...)"];
    let values = [&fields.ref_name, &fields.inputs];
    let focuses = [
        WorkflowDispatchFormFocus::RefName,
        WorkflowDispatchFormFocus::Inputs,
    ];
    let cursors = [cursor.ref_name, cursor.inputs];

    let field_elements: Vec<AnyElement<'static>> = labels
        .iter()
        .zip(values.iter())
        .zip(focuses.iter())
        .zip(cursors.iter())
        .map(|(((label, value), field_focus), field_cursor)| {
            let is_focused = focus == *field_focus;
            let rendered_value = if is_focused {
                render_text_with_caret(value, *field_cursor)
            } else {
                (*value).to_owned()
            };
            let display = format!("  {label:<18} [{rendered_value}]");
            let color = if is_focused { rc.bright } else { rc.fg };
            element! {
                Box(height: 1u32) {
                    Text(content: display, color: color)
                }
            }
            .into_any()
        })
        .collect();

    let submit_focused = focus == WorkflowDispatchFormFocus::Submit;
    let cancel_focused = focus == WorkflowDispatchFormFocus::Cancel;

    element! {
        Box(
            width: 60u32,
            height: 10u32,
            border_style: BorderStyle::Single,
            border_color: rc.border_focused,
            flex_direction: FlexDirection::Column,
            padding_left: 1u32,
            padding_right: 1u32,
            background_color: rc.bg,
        ) {
            // Title
            Box(height: 1u32, margin_bottom: 1u32) {
                Text(
                    content: format!("Trigger Workflow: {}", workflow_name),
                    color: rc.bright,
                    weight: Weight::Bold,
                )
            }

            // Input Fields
            Box(flex_direction: FlexDirection::Column, flex_grow: 1.0_f32) {
                #(field_elements)
            }

            // Buttons
            Box(
                height: 1u32,
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::End,
                margin_top: 1u32,
            ) {
                Box(margin_right: 2u32) {
                    Text(
                        content: "[Submit]",
                        color: if submit_focused { rc.bright } else { rc.fg },
                        weight: if submit_focused { Weight::Bold } else { Weight::Normal },
                    )
                }
                Box() {
                    Text(
                        content: "[Cancel]",
                        color: if cancel_focused { rc.bright } else { rc.fg },
                        weight: if cancel_focused { Weight::Bold } else { Weight::Normal },
                    )
                }
            }
        }
    }
}
