//! Identity and listing-order table for [`PackageCoordinate`]
//! (issue #389 CW-09, acceptance rows D3 and C1).

use super::*;

fn coordinate(id: &str, version: &str) -> PackageCoordinate {
    PackageCoordinate::parse(id, version)
        .unwrap_or_else(|error| panic!("{id}@{version} must parse: {error}"))
}

fn versions_in_listing_order(versions: &[&str]) -> Vec<String> {
    let mut coordinates: Vec<PackageCoordinate> = versions
        .iter()
        .map(|version| coordinate("vendor.pkg", version))
        .collect();
    coordinates.sort_by(PackageCoordinate::listing_cmp);
    coordinates
        .iter()
        .map(|entry| entry.version().as_str().to_owned())
        .collect()
}

#[test]
fn rejects_a_version_that_is_not_canonical_semver() {
    for version in [
        "1.0", "01.0.0", "1.0.0.0", "v1.0.0", "1.0.0 ", " 1.0.0", "1.0.0-", "1.0.0+", "1.0.0-01",
        "",
    ] {
        assert!(
            PackageCoordinate::parse("vendor.pkg", version).is_err(),
            "{version:?} is not canonical SemVer and must be rejected"
        );
    }
}

#[test]
fn rejects_an_invalid_plugin_id() {
    let error = PackageCoordinate::parse("core.dashboard", "1.0.0")
        .err()
        .unwrap_or_else(|| panic!("a reserved id must be rejected"));
    assert!(
        matches!(error, PackageCoordinateError::Id(_)),
        "the error must name the identifier: {error}"
    );
}

#[test]
fn build_metadata_only_variants_are_distinct_packages() {
    let a = coordinate("vendor.pkg", "1.0.0+a");
    let b = coordinate("vendor.pkg", "1.0.0+b");
    assert_ne!(a, b, "build metadata is part of exact package identity");
    assert_eq!(
        a.version().precedence_cmp(b.version()),
        std::cmp::Ordering::Equal,
        "build metadata must not affect SemVer precedence"
    );
}

#[test]
fn build_metadata_only_variants_order_by_exact_version_bytes() {
    assert_eq!(
        versions_in_listing_order(&["1.0.0+b", "1.0.0+a", "1.0.0"]),
        vec!["1.0.0", "1.0.0+a", "1.0.0+b"],
        "equal precedence falls back to ascending exact version bytes"
    );
}

#[test]
fn listing_order_is_semver_precedence_descending() {
    assert_eq!(
        versions_in_listing_order(&["0.9.0", "1.0.0", "1.0.0-rc.1", "1.0.1", "1.0.0-rc.2"]),
        vec!["1.0.1", "1.0.0", "1.0.0-rc.2", "1.0.0-rc.1", "0.9.0"],
        "a release outranks its prereleases and numeric cores compare numerically"
    );
}

#[test]
fn listing_order_compares_cores_numerically_not_lexically() {
    assert_eq!(
        versions_in_listing_order(&["2.0.0", "10.0.0", "9.0.0"]),
        vec!["10.0.0", "9.0.0", "2.0.0"],
        "10 outranks 9 numerically even though it sorts first lexically"
    );
}

#[test]
fn listing_order_sorts_by_identifier_before_version() {
    let mut coordinates = vec![
        coordinate("vendor.zeta", "9.0.0"),
        coordinate("vendor.alpha", "1.0.0"),
        coordinate("vendor.alpha", "2.0.0"),
    ];
    coordinates.sort_by(PackageCoordinate::listing_cmp);
    let rendered: Vec<String> = coordinates.iter().map(ToString::to_string).collect();
    assert_eq!(
        rendered,
        vec![
            "vendor.alpha@2.0.0",
            "vendor.alpha@1.0.0",
            "vendor.zeta@9.0.0"
        ]
    );
}

#[test]
fn listing_order_is_a_total_order_over_equal_coordinates() {
    let a = coordinate("vendor.pkg", "1.0.0");
    let b = coordinate("vendor.pkg", "1.0.0");
    assert_eq!(
        PackageCoordinate::listing_cmp(&a, &b),
        std::cmp::Ordering::Equal
    );
    assert_eq!(a, b, "identical coordinates are the same package identity");
}

#[test]
fn a_coordinate_renders_as_its_directory_identity() {
    let entry = coordinate("vendor.git-merger", "1.0.0-rc.1+build.5");
    assert_eq!(entry.to_string(), "vendor.git-merger@1.0.0-rc.1+build.5");
    assert_eq!(entry.id().as_str(), "vendor.git-merger");
    assert_eq!(entry.version().as_str(), "1.0.0-rc.1+build.5");
}
