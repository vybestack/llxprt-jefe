//! Pure textual projection for the definition-generated New Agent form.

use crate::domain::agent_definition::{FieldValue, Operation, Support};
use crate::state::generated_agent_form::{
    GeneratedAgentForm, GeneratedAgentFormFocus, GeneratedTarget,
};
use crate::state::generated_form::GeneratedFormField;
use crate::ui::util::{text_with_caret, truncate_with_ellipsis};

const OPERATIONS: [Operation; 4] = [
    Operation::Normal,
    Operation::Resume,
    Operation::FreshIssue,
    Operation::FreshPullRequest,
];
const TARGETS: [GeneratedTarget; 2] = [GeneratedTarget::Local, GeneratedTarget::Remote];

#[must_use]
pub fn content_lines(form: &GeneratedAgentForm, max_width: usize) -> Vec<String> {
    let mut lines = vec![
        " New Agent".to_string(),
        form.draft().display_name().to_string(),
    ];
    lines.push(" Operations".to_string());
    for operation in OPERATIONS {
        lines.push(support_line(
            form.focus() == &GeneratedAgentFormFocus::Operation(operation),
            operation_label(operation),
            form.operation_support(operation),
        ));
    }
    lines.push(" Targets".to_string());
    for target in TARGETS {
        lines.push(support_line(
            form.focus() == &GeneratedAgentFormFocus::Target(target),
            target_label(target),
            form.target_support(target),
        ));
    }
    lines.push(" Fields".to_string());
    for field in form.draft().fields().iter().filter(|field| field.visible()) {
        lines.push(field_line(field, form.focus()));
    }
    lines.push(String::new());
    lines.push(action_line(
        form.focus(),
        GeneratedAgentFormFocus::Create,
        if form.create_enabled() {
            "[Create enabled]"
        } else {
            "[Create disabled]"
        },
    ));
    lines.push(action_line(
        form.focus(),
        GeneratedAgentFormFocus::Back,
        "[Back]",
    ));
    lines.push(" Tab/Down next  Shift+Tab/Up prev  Enter choose  Esc/q Back".to_string());
    lines
        .into_iter()
        .map(|line| truncate_with_ellipsis(&line, max_width))
        .collect()
}

fn support_line(focused: bool, label: &str, support: &Support) -> String {
    let marker = if focused { ">" } else { " " };
    match support {
        Support::Supported => format!("{marker} {label}: Supported"),
        Support::Unsupported { reason } => {
            format!("{marker} {label}: Unsupported: {reason}")
        }
    }
}

fn field_line(field: &GeneratedFormField, focus: &GeneratedAgentFormFocus) -> String {
    let focused = matches!(focus, GeneratedAgentFormFocus::Field(id) if id == field.id());
    let marker = if focused { ">" } else { " " };
    let value = field_value(field, focused);
    format!("{marker} {}: {value}", field.label())
}

fn field_value(field: &GeneratedFormField, focused: bool) -> String {
    match field.value() {
        FieldValue::Boolean(value) => format!("[{}]", if *value { "x" } else { " " }),
        FieldValue::OptionalBoolean(value) => match value {
            Some(value) => format!("[{}]", if *value { "x" } else { " " }),
            None => "[unset]".to_string(),
        },
        FieldValue::String(value) | FieldValue::Path(value) => {
            if focused {
                format!("[{}]", text_with_caret(value, field.cursor()))
            } else {
                format!("[{value}]")
            }
        }
        FieldValue::Integer(value) => format!("[{value}]"),
        FieldValue::StringList(values) => format!("[{}]", values.join(", ")),
    }
}

fn operation_label(operation: Operation) -> &'static str {
    match operation {
        Operation::Normal => "Normal",
        Operation::Resume => "Resume",
        Operation::FreshIssue => "Fresh Issue",
        Operation::FreshPullRequest => "Fresh PR",
    }
}

fn target_label(target: GeneratedTarget) -> &'static str {
    match target {
        GeneratedTarget::Local => "Local",
        GeneratedTarget::Remote => "Remote",
    }
}

fn action_line(
    focus: &GeneratedAgentFormFocus,
    target: GeneratedAgentFormFocus,
    label: &str,
) -> String {
    let marker = if focus == &target { ">" } else { " " };
    format!("{marker} {label}")
}
