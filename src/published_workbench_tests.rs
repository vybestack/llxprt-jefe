//! Immutable resource-schema ownership in the published workbench (issue #705).

use std::collections::BTreeMap;

use crate::domain::{Id, TypedPortValue, TypedValue};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| unreachable!("valid fixture id: {error}"))
}

#[test]
fn the_published_workbench_owns_the_builtin_resource_schema_registry() {
    let workbench = crate::test_support::published_workbench();
    let value = TypedPortValue {
        type_id: id("github.issue"),
        schema_version: 1,
        semantic_key: "vybestack/llxprt-jefe#705".to_owned(),
        value: BTreeMap::from([(
            id("semantic-key"),
            TypedValue::String("vybestack/llxprt-jefe#705".to_owned()),
        )]),
    };

    assert_eq!(
        workbench
            .resource_schemas()
            .validate(&id("github.issues"), &value),
        Ok(())
    );
}
