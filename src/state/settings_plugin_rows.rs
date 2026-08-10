fn append_plugin_config_rows(
    state: &SettingsState,
    published: &PublishedSettings,
    rows: &mut Vec<SettingsRow>,
) {
    for (owner, plugin) in &published.plugins {
        if plugin.enabled != Some(true) {
            continue;
        }
        let Some(versions) = state.installed_plugin_configs.get(owner) else {
            continue;
        };
        let selected = plugin.version.as_ref().map_or_else(
            || versions.first(),
            |version| {
                versions
                    .iter()
                    .find(|candidate| candidate.version == *version)
            },
        );
        let Some(selected) = selected else {
            continue;
        };
        for row in
            super::plugin_config_view::project_plugin_config(&selected.schema, &plugin.values)
        {
            if !row.visible {
                continue;
            }
            let editor_value = plugin_control_editor_value(&row.control);
            let mut display_value = plugin_control_text(&row.control);
            if let Some(error) = row.error {
                display_value.push_str(" — error: ");
                display_value.push_str(plugin_config_error_text(error.reason));
            }
            append_plugin_config_metadata(&mut display_value, &row);
            let (boolean, choices) = match &row.control {
                super::plugin_config_view::PluginConfigControl::Boolean { value } => {
                    (Some(*value), Vec::new())
                }
                super::plugin_config_view::PluginConfigControl::Enum { choices, .. } => {
                    (None, choices.clone())
                }
                _ => (None, Vec::new()),
            };
            rows.push(SettingsRow {
                label: format!("{} / {}", owner.as_str(), row.label),
                value: display_value,
                kind: SettingsRowKind::PluginConfig {
                    plugin: owner.clone(),
                    field: row.field_id,
                    kind: row.kind,
                    value: editor_value,
                    boolean,
                    choices,
                },
            });
        }
    }
}

fn append_plugin_config_metadata(
    display: &mut String,
    row: &super::plugin_config_view::PluginConfigRow,
) {
    if let Some(description) = row.description.as_ref() {
        display.push_str(" — ");
        display.push_str(description);
    }
    if row.required {
        display.push_str("; required");
    }
    append_metadata_value(display, "default", row.default.as_deref());
    append_metadata_value(display, "min", row.min.as_deref());
    append_metadata_value(display, "max", row.max.as_deref());
    if !row.choices.is_empty() {
        display.push_str("; choices ");
        display.push_str(&row.choices.join("|"));
    }
    if row.unique {
        display.push_str("; unique");
    }
    display.push_str("; restart ");
    display.push_str(match row.restart {
        crate::domain::plugin::RestartScope::None => "none",
        crate::domain::plugin::RestartScope::Provider => "provider",
        crate::domain::plugin::RestartScope::Host => "host",
    });
}

fn append_metadata_value(display: &mut String, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        display.push_str("; ");
        display.push_str(label);
        display.push(' ');
        display.push_str(value);
    }
}
fn plugin_control_editor_value(control: &super::plugin_config_view::PluginConfigControl) -> String {
    use super::plugin_config_view::PluginConfigControl;
    match control {
        PluginConfigControl::Boolean { value } => value.to_string(),
        PluginConfigControl::Scalar { value } => value.clone(),
        PluginConfigControl::Enum { selected, .. } => selected.clone().unwrap_or_default(),
        PluginConfigControl::SecretReference { env, .. } => env.clone().unwrap_or_default(),
        PluginConfigControl::Hidden => String::new(),
    }
}

fn plugin_control_text(control: &super::plugin_config_view::PluginConfigControl) -> String {
    use super::plugin_config_view::PluginConfigControl;
    match control {
        PluginConfigControl::Boolean { value } => if *value { "[x]" } else { "[ ]" }.to_owned(),
        PluginConfigControl::Scalar { value } => {
            if value.is_empty() {
                "<unset>".to_owned()
            } else {
                value.clone()
            }
        }
        PluginConfigControl::Enum { selected, .. } => {
            selected.clone().unwrap_or_else(|| "<unset>".to_owned())
        }
        PluginConfigControl::SecretReference { env, set } => env.as_ref().map_or_else(
            || "unset".to_owned(),
            |env| format!("{} ({})", env, if *set { "set" } else { "unset" }),
        ),
        PluginConfigControl::Hidden => "hidden".to_owned(),
    }
}

fn plugin_config_activation(
    plugin: &Id,
    field: &Id,
    kind: FieldKind,
    value: &str,
    boolean: Option<bool>,
    choices: &[String],
) -> Option<SettingsActivation> {
    let edit = match kind {
        FieldKind::Boolean => Some(PluginConfigEditValue::Boolean(!boolean.unwrap_or(false))),
        FieldKind::Enum => {
            let next = choices
                .iter()
                .position(|choice| choice == value)
                .map_or(0, |index| (index + 1) % choices.len().max(1));
            choices
                .get(next)
                .cloned()
                .map(PluginConfigEditValue::String)
        }
        FieldKind::String
        | FieldKind::Integer
        | FieldKind::FiniteNumber
        | FieldKind::Path
        | FieldKind::StringList
        | FieldKind::SecretReference => {
            return Some(SettingsActivation::OpenPluginConfig {
                plugin: plugin.clone(),
                field: field.clone(),
                kind,
                value: value.to_owned(),
            });
        }
    };
    edit.map(|value| {
        SettingsActivation::Edit(SettingsEdit::PluginConfig {
            plugin: plugin.clone(),
            field: field.clone(),
            value,
        })
    })
}

fn plugin_config_error_text(
    reason: crate::domain::plugin_config::ConfigValueErrorKind,
) -> &'static str {
    use crate::domain::plugin_config::ConfigValueErrorKind;
    match reason {
        ConfigValueErrorKind::Required => "required",
        ConfigValueErrorKind::Type => "wrong type",
        ConfigValueErrorKind::BelowMinimum => "below minimum",
        ConfigValueErrorKind::AboveMaximum => "above maximum",
        ConfigValueErrorKind::Choice => "not an allowed choice",
        ConfigValueErrorKind::Duplicate => "duplicate list value",
        ConfigValueErrorKind::Unknown => "unknown field",
    }
}

#[cfg(test)]
#[test]
fn literal_unset_strings_are_preserved_when_the_editor_opens() {
    let plugin = Id::parse("vendor.config")
        .unwrap_or_else(|error| panic!("plugin id fixture: {error}"));
    let field = Id::parse("value").unwrap_or_else(|error| panic!("field id fixture: {error}"));

    for literal in ["unset", "<unset>"] {
        let activation = plugin_config_activation(
            &plugin,
            &field,
            FieldKind::String,
            literal,
            None,
            &[],
        );
        assert!(matches!(
            activation,
            Some(SettingsActivation::OpenPluginConfig { value, .. }) if value == literal
        ));
    }
}
