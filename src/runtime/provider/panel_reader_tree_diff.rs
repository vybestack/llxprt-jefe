fn read_tree_body(value: &BoundedJson) -> Result<TreeBody, ProviderError> {
    let path = "panel-snapshot.body";
    let members = closed_object(value, path, &TREE_BODY_KEYS)?;
    let schema_version = read_u64(members, path, "schema_version")?;
    validate_model_schema(schema_version, &format!("{path}.schema_version"))?;
    let node_values = array(
        require(members, path, "nodes")?,
        &format!("{path}.nodes"),
        TREE_NODE_LIMIT,
    )?;
    let mut nodes = Vec::with_capacity(node_values.len());
    for (index, node) in node_values.iter().enumerate() {
        nodes.push(read_tree_node(node, &format!("{path}.nodes[{index}]"))?);
    }
    validate_tree_nodes(&nodes, &format!("{path}.nodes"))?;
    let selected_id = read_optional_id(members, path, "selected_id")?;
    if let Some(selected) = selected_id.as_ref()
        && !nodes.iter().any(|node| &node.id == selected)
    {
        return Err(ProviderError::InvalidValue {
            path: format!("{path}.selected_id"),
            reason: "selected_id does not reference a tree node".to_owned(),
        });
    }
    Ok(TreeBody {
        schema_version,
        nodes,
        selected_id,
    })
}

fn read_tree_node(value: &BoundedJson, path: &str) -> Result<TreeNode, ProviderError> {
    let members = closed_object(value, path, &TREE_NODE_KEYS)?;
    Ok(TreeNode {
        id: read_id(members, path, "id")?,
        parent_id: read_optional_id(members, path, "parent_id")?,
        label: read_string(members, path, "label")?.to_owned(),
        semantic_key: read_id(members, path, "semantic_key")?,
        depth: read_u64(members, path, "depth")?,
        expandable: read_bool(members, path, "expandable")?,
        expanded: read_bool(members, path, "expanded")?,
    })
}

fn validate_tree_nodes(nodes: &[TreeNode], path: &str) -> Result<(), ProviderError> {
    reject_unique_ids(nodes.iter().map(|node| &node.id), path, "tree node id")?;
    collect_unique(
        nodes.iter().map(|node| &node.semantic_key),
        path,
        "semantic key",
    )?;

    let mut ancestors: Vec<&TreeNode> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let depth = usize::try_from(node.depth).map_err(|_| ProviderError::InvalidValue {
            path: format!("{path}[{index}].depth"),
            reason: "depth cannot be represented by the host".to_owned(),
        })?;
        if depth > ancestors.len() {
            return Err(ProviderError::InvalidValue {
                path: format!("{path}[{index}].depth"),
                reason: "depth skips a parent level".to_owned(),
            });
        }
        let expected_parent = depth
            .checked_sub(1)
            .and_then(|parent_depth| ancestors.get(parent_depth))
            .copied();
        match (node.parent_id.as_ref(), expected_parent) {
            (None, None) => {}
            (Some(parent), Some(expected)) if parent == &expected.id => {}
            _ => {
                return Err(ProviderError::InvalidValue {
                    path: format!("{path}[{index}].parent_id"),
                    reason: "parent_id must name the preceding node at depth - 1".to_owned(),
                });
            }
        }
        if expected_parent.is_some_and(|parent| !parent.expandable) {
            return Err(ProviderError::InvalidValue {
                path: format!("{path}[{index}].parent_id"),
                reason: "parent_id must name an expandable node".to_owned(),
            });
        }
        if node.expanded && !node.expandable {
            return Err(ProviderError::InvalidValue {
                path: format!("{path}[{index}].expanded"),
                reason: "a non-expandable node cannot be expanded".to_owned(),
            });
        }
        ancestors.truncate(depth);
        ancestors.push(node);
    }
    Ok(())
}

fn read_structured_diff_body(value: &BoundedJson) -> Result<StructuredDiffBody, ProviderError> {
    let path = "panel-snapshot.body";
    let members = closed_object(value, path, &STRUCTURED_DIFF_BODY_KEYS)?;
    let schema_version = read_u64(members, path, "schema_version")?;
    validate_model_schema(schema_version, &format!("{path}.schema_version"))?;
    let file_values = array(
        require(members, path, "files")?,
        &format!("{path}.files"),
        DIFF_FILE_LIMIT,
    )?;
    let mut files = Vec::with_capacity(file_values.len());
    for (index, file) in file_values.iter().enumerate() {
        files.push(read_structured_diff_file(
            file,
            &format!("{path}.files[{index}]"),
        )?);
    }
    reject_unique_ids(
        files.iter().map(|file| &file.id),
        &format!("{path}.files"),
        "structured-diff file id",
    )?;
    let selected_file_id = read_optional_id(members, path, "selected_file_id")?;
    if let Some(selected) = selected_file_id.as_ref()
        && !files.iter().any(|file| &file.id == selected)
    {
        return Err(ProviderError::InvalidValue {
            path: format!("{path}.selected_file_id"),
            reason: "selected_file_id does not reference a structured-diff file".to_owned(),
        });
    }
    Ok(StructuredDiffBody {
        schema_version,
        files,
        selected_file_id,
    })
}

fn read_structured_diff_file(
    value: &BoundedJson,
    path: &str,
) -> Result<StructuredDiffFile, ProviderError> {
    let members = closed_object(value, path, &STRUCTURED_DIFF_FILE_KEYS)?;
    let old_path = read_optional_string(members, path, "old_path")?;
    let new_path = read_optional_string(members, path, "new_path")?;
    let old_mode = read_optional_string(members, path, "old_mode")?;
    let new_mode = read_optional_string(members, path, "new_mode")?;
    validate_diff_file_sides(
        path,
        old_path.as_deref(),
        new_path.as_deref(),
        old_mode.as_deref(),
        new_mode.as_deref(),
    )?;
    let hunk_values = array(
        require(members, path, "hunks")?,
        &format!("{path}.hunks"),
        DIFF_HUNK_LIMIT,
    )?;
    let mut hunks = Vec::with_capacity(hunk_values.len());
    for (index, hunk) in hunk_values.iter().enumerate() {
        hunks.push(read_structured_diff_hunk(
            hunk,
            &format!("{path}.hunks[{index}]"),
        )?);
    }
    let binary = read_bool(members, path, "binary")?;
    if binary && !hunks.is_empty() {
        return Err(ProviderError::InvalidValue {
            path: format!("{path}.hunks"),
            reason: "a binary diff cannot carry text hunks".to_owned(),
        });
    }
    validate_hunk_order(&hunks, &format!("{path}.hunks"))?;
    Ok(StructuredDiffFile {
        id: read_id(members, path, "id")?,
        old_path,
        new_path,
        old_mode,
        new_mode,
        binary,
        hunks,
    })
}

fn validate_diff_file_sides(
    path: &str,
    old_path: Option<&str>,
    new_path: Option<&str>,
    old_mode: Option<&str>,
    new_mode: Option<&str>,
) -> Result<(), ProviderError> {
    if old_path.is_none() && new_path.is_none() {
        return Err(ProviderError::InvalidValue {
            path: path.to_owned(),
            reason: "a structured-diff file requires an old_path or new_path".to_owned(),
        });
    }
    for (name, value) in [
        ("old_path", old_path),
        ("new_path", new_path),
        ("old_mode", old_mode),
        ("new_mode", new_mode),
    ] {
        if value.is_some_and(str::is_empty) {
            return Err(ProviderError::InvalidValue {
                path: format!("{path}.{name}"),
                reason: format!("{name} must not be empty"),
            });
        }
    }
    for (name, mode, side_path) in [
        ("old_mode", old_mode, old_path),
        ("new_mode", new_mode, new_path),
    ] {
        if mode.is_some() && side_path.is_none() {
            return Err(ProviderError::InvalidValue {
                path: format!("{path}.{name}"),
                reason: format!("{name} requires its corresponding path"),
            });
        }
    }
    Ok(())
}

fn read_structured_diff_hunk(
    value: &BoundedJson,
    path: &str,
) -> Result<StructuredDiffHunk, ProviderError> {
    let members = closed_object(value, path, &STRUCTURED_DIFF_HUNK_KEYS)?;
    let old_start = read_u64(members, path, "old_start")?;
    let old_lines = read_u64(members, path, "old_lines")?;
    let new_start = read_u64(members, path, "new_start")?;
    let new_lines = read_u64(members, path, "new_lines")?;
    if (old_lines > 0 && old_start == 0) || (new_lines > 0 && new_start == 0) {
        return Err(ProviderError::InvalidValue {
            path: path.to_owned(),
            reason: "a nonempty hunk side must start at a positive line number".to_owned(),
        });
    }
    let line_values = array(
        require(members, path, "lines")?,
        &format!("{path}.lines"),
        DIFF_LINE_LIMIT,
    )?;
    if line_values.is_empty() {
        return Err(ProviderError::InvalidValue {
            path: format!("{path}.lines"),
            reason: "a hunk must carry at least one line".to_owned(),
        });
    }
    let mut lines = Vec::with_capacity(line_values.len());
    for (index, line) in line_values.iter().enumerate() {
        lines.push(read_structured_diff_line(
            line,
            &format!("{path}.lines[{index}]"),
        )?);
    }
    validate_diff_lines(&lines, old_start, old_lines, new_start, new_lines, path)?;
    Ok(StructuredDiffHunk {
        header: read_string(members, path, "header")?.to_owned(),
        old_start,
        old_lines,
        new_start,
        new_lines,
        lines,
    })
}

fn read_structured_diff_line(
    value: &BoundedJson,
    path: &str,
) -> Result<StructuredDiffLine, ProviderError> {
    let members = closed_object(value, path, &STRUCTURED_DIFF_LINE_KEYS)?;
    let content = read_string(members, path, "content")?.to_owned();
    if content.len() > DIFF_LINE_BYTE_LIMIT {
        return Err(ProviderError::InvalidValue {
            path: format!("{path}.content"),
            reason: format!(
                "line is {} bytes, over the {DIFF_LINE_BYTE_LIMIT} limit",
                content.len()
            ),
        });
    }
    Ok(StructuredDiffLine {
        origin: read_enum(members, path, "origin", DiffLineOrigin::from_wire)?,
        old_line: super::object_reader::read_optional_u64(members, path, "old_line")?,
        new_line: super::object_reader::read_optional_u64(members, path, "new_line")?,
        content,
        no_newline: read_bool(members, path, "no_newline")?,
    })
}

fn validate_diff_lines(
    lines: &[StructuredDiffLine],
    old_start: u64,
    old_lines: u64,
    new_start: u64,
    new_lines: u64,
    path: &str,
) -> Result<(), ProviderError> {
    let mut expected_old = old_start;
    let mut expected_new = new_start;
    let mut counted_old = 0_u64;
    let mut counted_new = 0_u64;
    for (index, line) in lines.iter().enumerate() {
        let (has_old, has_new) = match line.origin {
            DiffLineOrigin::Context => (true, true),
            DiffLineOrigin::Added => (false, true),
            DiffLineOrigin::Removed => (true, false),
        };
        if has_old {
            if line.old_line != Some(expected_old) || expected_old == 0 {
                return Err(invalid_diff_line_number(path, index, "old_line"));
            }
            expected_old = expected_old
                .checked_add(1)
                .ok_or_else(|| invalid_diff_line_number(path, index, "old_line"))?;
            counted_old += 1;
        } else if line.old_line.is_some() {
            return Err(invalid_diff_line_number(path, index, "old_line"));
        }
        if has_new {
            if line.new_line != Some(expected_new) || expected_new == 0 {
                return Err(invalid_diff_line_number(path, index, "new_line"));
            }
            expected_new = expected_new
                .checked_add(1)
                .ok_or_else(|| invalid_diff_line_number(path, index, "new_line"))?;
            counted_new += 1;
        } else if line.new_line.is_some() {
            return Err(invalid_diff_line_number(path, index, "new_line"));
        }
    }
    if counted_old != old_lines || counted_new != new_lines {
        return Err(ProviderError::InvalidValue {
            path: format!("{path}.lines"),
            reason: format!(
                "line origins count ({counted_old}, {counted_new}) but the hunk declares ({old_lines}, {new_lines})"
            ),
        });
    }
    Ok(())
}

fn invalid_diff_line_number(path: &str, index: usize, field: &str) -> ProviderError {
    ProviderError::InvalidValue {
        path: format!("{path}.lines[{index}].{field}"),
        reason: format!("{field} does not match the line origin and hunk sequence"),
    }
}

fn validate_hunk_order(hunks: &[StructuredDiffHunk], path: &str) -> Result<(), ProviderError> {
    for index in 1..hunks.len() {
        let previous = &hunks[index - 1];
        let current = &hunks[index];
        let old_end = previous
            .old_start
            .checked_add(previous.old_lines)
            .ok_or_else(|| ProviderError::InvalidValue {
                path: format!("{path}[{}].old_lines", index - 1),
                reason: "old hunk range overflows".to_owned(),
            })?;
        let new_end = previous
            .new_start
            .checked_add(previous.new_lines)
            .ok_or_else(|| ProviderError::InvalidValue {
                path: format!("{path}[{}].new_lines", index - 1),
                reason: "new hunk range overflows".to_owned(),
            })?;
        if current.old_start < old_end || current.new_start < new_end {
            return Err(ProviderError::InvalidValue {
                path: format!("{path}[{index}]"),
                reason: "hunks must be ordered and non-overlapping on both sides".to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_model_schema(schema_version: u64, path: &str) -> Result<(), ProviderError> {
    if schema_version == PANEL_MODEL_SCHEMA {
        Ok(())
    } else {
        Err(ProviderError::InvalidValue {
            path: path.to_owned(),
            reason: format!(
                "schema_version {schema_version} is not the supported version {PANEL_MODEL_SCHEMA}"
            ),
        })
    }
}
