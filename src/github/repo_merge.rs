use crate::domain::MergeMethod;

/// Parse repository merge-method settings returned by `gh api --jq`.
/// Malformed output degrades to an empty set, which callers treat as unknown.
pub(super) fn parse_repo_merge_methods(jq_output: &str) -> Vec<MergeMethod> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(jq_output.trim()) else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    for (field, method) in [
        ("allow_merge_commit", MergeMethod::Merge),
        ("allow_squash_merge", MergeMethod::Squash),
        ("allow_rebase_merge", MergeMethod::Rebase),
    ] {
        if value
            .get(field)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            methods.push(method);
        }
    }
    methods
}
