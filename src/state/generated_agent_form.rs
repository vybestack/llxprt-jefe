//! Deterministic UI state for one definition-driven New Agent form.
//!
//! The state owns only typed draft values, declared support projections, and
//! focus. It performs no planning, persistence, preparation, or runtime work.

use crate::domain::agent_definition::{
    AgentDefinition, Availability, FieldKind, FieldValue, Operation, Support,
};

use super::generated_form::{
    FormFieldId, FormFieldValue, FormIntent, GeneratedFormBuildError, GeneratedFormDraft,
};
use super::util::{delete_char_at, delete_char_before, insert_char_at};

const OPERATIONS: [Operation; 4] = [
    Operation::Normal,
    Operation::Resume,
    Operation::FreshIssue,
    Operation::FreshPullRequest,
];
const TARGETS: [GeneratedTarget; 2] = [GeneratedTarget::Local, GeneratedTarget::Remote];

/// Target choice displayed by the generated form before launch planning exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedTarget {
    /// Local target support cell.
    Local,
    /// Remote target support cell.
    Remote,
}

/// One focus target in the generated New Agent form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedAgentFormFocus {
    /// An operation support row.
    Operation(Operation),
    /// A target support row.
    Target(GeneratedTarget),
    /// A generated typed field.
    Field(FormFieldId),
    /// Validate the current typed result for later planning.
    Create,
    /// Close the form without changing durable agent state.
    Back,
}

/// Typed intent accepted by the generated form reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedAgentFormIntent {
    /// Move to the next focus target, wrapping.
    Next,
    /// Move to the previous focus target, wrapping.
    Previous,
    /// Insert one character into the focused typed field.
    Insert(char),
    /// Delete the character before the focused cursor.
    Backspace,
    /// Delete the character at the focused cursor.
    Delete,
    /// Move the focused field cursor left.
    CursorLeft,
    /// Move the focused field cursor right.
    CursorRight,
    /// Move the focused field cursor to the start.
    CursorStart,
    /// Move the focused field cursor to the end.
    CursorEnd,
    /// Select, toggle, cycle, validate, or back according to focus.
    Activate,
}

/// Typed form result produced before launch planning is implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedAgentFormResult {
    /// Selected operation.
    pub operation: Operation,
    /// Selected target kind.
    pub target: GeneratedTarget,
    /// Visible, enabled typed values in declaration order.
    pub values: Vec<FormFieldValue>,
}

/// Definition-generated form state consumed by the thin renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedAgentForm {
    draft: GeneratedFormDraft,
    operation_support: Vec<(Operation, Support)>,
    target_support: Vec<(GeneratedTarget, Support)>,
    selected_operation: Operation,
    selected_target: GeneratedTarget,
    focus: GeneratedAgentFormFocus,
    validated_result: Option<GeneratedAgentFormResult>,
}

impl GeneratedAgentForm {
    /// Build a UI form from the selected definition and observed availability.
    pub fn from_definition(
        definition: &AgentDefinition,
        availability: &Availability,
    ) -> Result<Self, GeneratedFormBuildError> {
        let draft = GeneratedFormDraft::from_definition(definition, availability)?;
        let operation_support = OPERATIONS
            .into_iter()
            .map(|operation| {
                (
                    operation,
                    projected_operation_support(definition, availability, operation),
                )
            })
            .collect();
        let target_support = TARGETS
            .into_iter()
            .map(|target| (target, declared_target_support(definition, target)))
            .collect();
        Ok(Self {
            draft,
            operation_support,
            target_support,
            selected_operation: Operation::Resume,
            selected_target: GeneratedTarget::Local,
            focus: GeneratedAgentFormFocus::Operation(Operation::Resume),
            validated_result: None,
        })
    }

    /// Generated typed draft.
    #[must_use]
    pub const fn draft(&self) -> &GeneratedFormDraft {
        &self.draft
    }

    /// Current focus target.
    #[must_use]
    pub const fn focus(&self) -> &GeneratedAgentFormFocus {
        &self.focus
    }

    /// Currently selected operation.
    #[must_use]
    pub const fn selected_operation(&self) -> Operation {
        self.selected_operation
    }

    /// Currently selected target kind.
    #[must_use]
    pub const fn selected_target(&self) -> GeneratedTarget {
        self.selected_target
    }

    /// Projected support for one operation row.
    #[must_use]
    pub fn operation_support(&self, operation: Operation) -> &Support {
        self.operation_support
            .iter()
            .find(|(candidate, _)| *candidate == operation)
            .map_or(&Support::Supported, |(_, support)| support)
    }

    /// Declared support for one target row.
    #[must_use]
    pub fn target_support(&self, target: GeneratedTarget) -> &Support {
        self.target_support
            .iter()
            .find(|(candidate, _)| *candidate == target)
            .map_or(&Support::Supported, |(_, support)| support)
    }

    /// Whether the selected support cells and current typed values validate.
    #[must_use]
    pub fn create_enabled(&self) -> bool {
        !self
            .operation_support(self.selected_operation)
            .is_unsupported()
            && !self.target_support(self.selected_target).is_unsupported()
            && self.draft.validation_issues().is_empty()
    }

    /// Last validated typed result, if Create was activated while enabled.
    #[must_use]
    pub const fn validated_result(&self) -> Option<&GeneratedAgentFormResult> {
        self.validated_result.as_ref()
    }

    /// Take the validated typed result, clearing internal storage so the
    /// production submit path consumes it exactly once.
    #[must_use]
    pub fn take_validated_result(&mut self) -> Option<GeneratedAgentFormResult> {
        self.validated_result.take()
    }

    /// Whether Space should activate rather than insert into the focused field.
    #[must_use]
    pub fn focus_is_toggleable(&self) -> bool {
        let GeneratedAgentFormFocus::Field(id) = &self.focus else {
            return false;
        };
        self.draft.field(id).is_some_and(|field| {
            matches!(
                field.kind(),
                FieldKind::Boolean | FieldKind::OptionalBoolean | FieldKind::Enum
            )
        })
    }

    /// Apply one deterministic, I/O-free form intent.
    pub fn apply(&mut self, intent: GeneratedAgentFormIntent) {
        self.validated_result = None;
        match intent {
            GeneratedAgentFormIntent::Next => self.move_focus(true),
            GeneratedAgentFormIntent::Previous => self.move_focus(false),
            GeneratedAgentFormIntent::Insert(character) => self.insert(character),
            GeneratedAgentFormIntent::Backspace => self.delete_before(),
            GeneratedAgentFormIntent::Delete => self.delete_at(),
            GeneratedAgentFormIntent::CursorLeft => self.move_cursor(CursorMove::Left),
            GeneratedAgentFormIntent::CursorRight => self.move_cursor(CursorMove::Right),
            GeneratedAgentFormIntent::CursorStart => self.move_cursor(CursorMove::Start),
            GeneratedAgentFormIntent::CursorEnd => self.move_cursor(CursorMove::End),
            GeneratedAgentFormIntent::Activate => self.activate(),
        }
        self.normalize_focus();
    }

    fn focus_sequence(&self) -> Vec<GeneratedAgentFormFocus> {
        let mut sequence = Vec::with_capacity(OPERATIONS.len() + TARGETS.len() + 2);
        sequence.extend(
            OPERATIONS
                .into_iter()
                .map(GeneratedAgentFormFocus::Operation),
        );
        sequence.extend(TARGETS.into_iter().map(GeneratedAgentFormFocus::Target));
        sequence.extend(
            self.draft
                .visible_field_ids()
                .into_iter()
                .map(GeneratedAgentFormFocus::Field),
        );
        sequence.push(GeneratedAgentFormFocus::Create);
        sequence.push(GeneratedAgentFormFocus::Back);
        sequence
    }

    fn move_focus(&mut self, forward: bool) {
        let sequence = self.focus_sequence();
        let current = sequence
            .iter()
            .position(|candidate| candidate == &self.focus)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % sequence.len()
        } else if current == 0 {
            sequence.len() - 1
        } else {
            current - 1
        };
        if let Some(focus) = sequence.get(next).cloned() {
            self.set_focus(focus);
        }
    }

    fn set_focus(&mut self, focus: GeneratedAgentFormFocus) {
        if let GeneratedAgentFormFocus::Field(id) = &focus {
            self.reduce_draft(FormIntent::Focus(id.clone()));
        }
        self.focus = focus;
    }

    fn activate(&mut self) {
        match self.focus.clone() {
            GeneratedAgentFormFocus::Operation(operation) => {
                if !self.operation_support(operation).is_unsupported() {
                    self.selected_operation = operation;
                }
            }
            GeneratedAgentFormFocus::Target(target) => {
                if !self.target_support(target).is_unsupported() {
                    self.selected_target = target;
                }
            }
            GeneratedAgentFormFocus::Field(id) => self.activate_field(&id),
            GeneratedAgentFormFocus::Create if self.create_enabled() => {
                self.validated_result = Some(GeneratedAgentFormResult {
                    operation: self.selected_operation,
                    target: self.selected_target,
                    values: self.draft.active_values(),
                });
            }
            GeneratedAgentFormFocus::Create | GeneratedAgentFormFocus::Back => {}
        }
    }

    fn activate_field(&mut self, id: &FormFieldId) {
        let Some(field) = self.draft.field(id) else {
            return;
        };
        match field.kind() {
            FieldKind::Boolean | FieldKind::OptionalBoolean => {
                self.reduce_draft(FormIntent::Toggle { field: id.clone() });
            }
            FieldKind::Enum => {
                let choices = field.choices();
                let current = match field.value() {
                    FieldValue::String(value) => choices.iter().position(|choice| choice == value),
                    _ => None,
                };
                let next = current.map_or(0, |index| (index + 1) % choices.len());
                if let Some(value) = choices.get(next).cloned() {
                    self.reduce_draft(FormIntent::SetValue {
                        field: id.clone(),
                        value: FieldValue::String(value),
                    });
                }
            }
            FieldKind::String | FieldKind::Integer | FieldKind::Path | FieldKind::StringList => {}
        }
    }

    fn insert(&mut self, character: char) {
        let Some((id, value, cursor)) = self.focused_value() else {
            return;
        };
        let next = match value {
            FieldValue::String(mut text) => {
                insert_char_at(&mut text, cursor, character);
                FieldValue::String(text)
            }
            FieldValue::Path(mut text) => {
                insert_char_at(&mut text, cursor, character);
                FieldValue::Path(text)
            }
            FieldValue::Integer(value) if character.is_ascii_digit() || character == '-' => {
                let mut text = value.to_string();
                insert_char_at(&mut text, cursor, character);
                let Ok(parsed) = text.parse::<i64>() else {
                    return;
                };
                FieldValue::Integer(parsed)
            }
            FieldValue::StringList(mut values) => {
                insert_list_character(&mut values, character);
                FieldValue::StringList(values)
            }
            FieldValue::Boolean(_) | FieldValue::OptionalBoolean(_) | FieldValue::Integer(_) => {
                return;
            }
        };
        self.reduce_draft(FormIntent::SetValue {
            field: id,
            value: next,
        });
    }

    fn delete_before(&mut self) {
        let Some((id, value, cursor)) = self.focused_value() else {
            return;
        };
        let next = match value {
            FieldValue::String(mut text) => {
                delete_char_before(&mut text, cursor);
                FieldValue::String(text)
            }
            FieldValue::Path(mut text) => {
                delete_char_before(&mut text, cursor);
                FieldValue::Path(text)
            }
            FieldValue::Integer(value) => integer_after_delete(value, cursor, true),
            FieldValue::StringList(mut values) => {
                delete_list_character(&mut values);
                FieldValue::StringList(values)
            }
            FieldValue::Boolean(_) | FieldValue::OptionalBoolean(_) => return,
        };
        self.reduce_draft(FormIntent::SetValue {
            field: id,
            value: next,
        });
    }

    fn delete_at(&mut self) {
        let Some((id, value, cursor)) = self.focused_value() else {
            return;
        };
        let next = match value {
            FieldValue::String(mut text) => {
                delete_char_at(&mut text, cursor);
                FieldValue::String(text)
            }
            FieldValue::Path(mut text) => {
                delete_char_at(&mut text, cursor);
                FieldValue::Path(text)
            }
            FieldValue::Integer(value) => integer_after_delete(value, cursor, false),
            FieldValue::StringList(_) | FieldValue::Boolean(_) | FieldValue::OptionalBoolean(_) => {
                return;
            }
        };
        self.reduce_draft(FormIntent::SetValue {
            field: id,
            value: next,
        });
    }

    fn move_cursor(&mut self, movement: CursorMove) {
        let GeneratedAgentFormFocus::Field(id) = &self.focus else {
            return;
        };
        let intent = match movement {
            CursorMove::Left => FormIntent::MoveCursorLeft { field: id.clone() },
            CursorMove::Right => FormIntent::MoveCursorRight { field: id.clone() },
            CursorMove::Start => FormIntent::MoveCursorStart { field: id.clone() },
            CursorMove::End => FormIntent::MoveCursorEnd { field: id.clone() },
        };
        self.reduce_draft(intent);
    }

    fn focused_value(&self) -> Option<(FormFieldId, FieldValue, usize)> {
        let GeneratedAgentFormFocus::Field(id) = &self.focus else {
            return None;
        };
        self.draft
            .field(id)
            .map(|field| (id.clone(), field.value().clone(), field.cursor()))
    }

    fn reduce_draft(&mut self, intent: FormIntent) {
        if let Ok(next) = self.draft.clone().reduce(intent) {
            self.draft = next;
        }
    }

    fn normalize_focus(&mut self) {
        if self.focus_sequence().contains(&self.focus) {
            return;
        }
        self.focus = self.draft.focused().cloned().map_or(
            GeneratedAgentFormFocus::Create,
            GeneratedAgentFormFocus::Field,
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum CursorMove {
    Left,
    Right,
    Start,
    End,
}

fn projected_operation_support(
    definition: &AgentDefinition,
    availability: &Availability,
    operation: Operation,
) -> Support {
    let declared = &definition.operations.support_for(operation).supported;
    if declared.is_unsupported() {
        return declared.clone();
    }
    match availability {
        Availability::NotFound => Support::unsupported("no executable candidate resolved"),
        Availability::InstalledIncompatible { reason, .. } => Support::unsupported(reason),
        Availability::ProbeError { code, reason, .. } => {
            Support::unsupported(format!("{}: {reason}", code.as_str()))
        }
        Availability::InstalledCompatible { capabilities, .. } => {
            let required = operation_capability(definition, operation);
            if let Some(capability) = required
                && !capabilities.iter().any(|found| found == capability)
            {
                return Support::unsupported(format!(
                    "installed {} lacks required capability `{capability}`",
                    definition.display_name
                ));
            }
            Support::Supported
        }
    }
}

fn operation_capability(definition: &AgentDefinition, operation: Operation) -> Option<&str> {
    let capability = match operation {
        Operation::Resume => "resume",
        Operation::Normal | Operation::FreshIssue | Operation::FreshPullRequest => return None,
    };
    definition
        .probe
        .capabilities
        .as_ref()?
        .tokens
        .iter()
        .any(|token| token.id == capability)
        .then_some(capability)
}

fn declared_target_support(definition: &AgentDefinition, target: GeneratedTarget) -> Support {
    match target {
        GeneratedTarget::Local => definition.targets.local.supported.clone(),
        GeneratedTarget::Remote => definition.targets.remote.supported.clone(),
    }
}

fn insert_list_character(values: &mut Vec<String>, character: char) {
    if character == ',' {
        values.push(String::new());
        return;
    }
    if values.is_empty() {
        values.push(String::new());
    }
    if let Some(value) = values.last_mut() {
        value.push(character);
    }
}

fn delete_list_character(values: &mut Vec<String>) {
    let remove_item = values.last().is_some_and(String::is_empty);
    if remove_item {
        values.pop();
    } else if let Some(value) = values.last_mut() {
        value.pop();
    }
}

fn integer_after_delete(value: i64, cursor: usize, before: bool) -> FieldValue {
    let mut text = value.to_string();
    if before {
        delete_char_before(&mut text, cursor);
    } else {
        delete_char_at(&mut text, cursor);
    }
    FieldValue::Integer(text.parse::<i64>().unwrap_or(0))
}
