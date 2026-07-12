//! Cache-correctness tests for the `render_markdown_block` memo layer
//! (issue #155 perf). Split out of `markdown_render_tests.rs` to keep each
//! test module under the source-file length policy.

use super::*;

/// The memo cache on `render_markdown_block` must key on the FULL
/// (markdown, prefix, placeholder) triple: identical args return identical
/// output, and differing any ONE arg produces the correct differing output.
/// This test FAILS if someone keys only on markdown.
#[test]
fn render_markdown_block_memoized_is_idempotent() {
    let md = "## Heading\n\nbody text";
    // Idempotency.
    let a = render_markdown_block(md, "  ", "(no body)");
    let b = render_markdown_block(md, "  ", "(no body)");
    assert_eq!(a, b, "identical args must return identical output");
    assert!(
        Arc::ptr_eq(&a, &b),
        "identical args must return the memoized allocation"
    );

    // Differing prefix (key includes prefix).
    let pfx_l = render_markdown_block(md, "L>", "(no body)");
    let pfx_r = render_markdown_block(md, "R>", "(no body)");
    assert_ne!(pfx_l, pfx_r, "key includes prefix");
    assert!(pfx_l.iter().any(|l| l.starts_with("L>")));
    assert!(pfx_r.iter().any(|l| l.starts_with("R>")));

    // Differing placeholder (key includes placeholder).
    let ph_a = render_markdown_block("", "  ", "(no description)");
    let ph_b = render_markdown_block("", "  ", "(no body)");
    assert_ne!(ph_a, ph_b, "key includes placeholder");
    assert_eq!(ph_a.as_ref(), &["  (no description)".to_string()]);
    assert_eq!(ph_b.as_ref(), &["  (no body)".to_string()]);

    // Differing markdown (key includes body).
    let md_a = render_markdown_block("alpha", "  ", "(no body)");
    let md_b = render_markdown_block("beta", "  ", "(no body)");
    assert_ne!(md_a, md_b, "key includes body");
    assert!(md_a.iter().any(|l| l.contains("alpha")));
    assert!(md_b.iter().any(|l| l.contains("beta")));
}

/// The restructured cache keys on markdown alone with a small value-vec of
/// `(prefix, placeholder)` variants per body. This test exercises the
/// value-vec path explicitly: the SAME markdown body rendered first with one
/// prefix then another must return correctly-prefixed results from the
/// same map entry (the second call populates a second variant, and a repeat
/// of the first prefix must still hit its own variant correctly).
#[test]
fn render_markdown_block_same_body_different_prefixes_via_value_vec() {
    let md = "## Shared Heading

body line";

    // Warm the cache entry with prefix "AA".
    let aa_first = render_markdown_block(md, "AA", "(no body)");
    assert!(
        aa_first.iter().any(|l| l.starts_with("AA")),
        "first call with AA prefix must produce AA-prefixed lines: {aa_first:?}"
    );

    // Same body, different prefix — exercises the value-vec (second variant).
    let bb = render_markdown_block(md, "BB", "(no body)");
    assert!(
        bb.iter().any(|l| l.starts_with("BB")),
        "second prefix BB must produce BB-prefixed lines: {bb:?}"
    );
    assert_ne!(aa_first, bb, "different prefixes must differ");

    // Repeat the FIRST prefix — must still return its own correct variant,
    // not accidentally collide with BB (proves the value-vec lookup matches
    // the exact (prefix, placeholder) pair).
    let aa_again = render_markdown_block(md, "AA", "(no body)");
    assert_eq!(
        aa_first, aa_again,
        "repeating the first prefix must return the same result"
    );
    assert!(
        aa_again.iter().all(|l| !l.starts_with("BB")),
        "AA result must not contain BB-prefixed lines: {aa_again:?}"
    );
}

/// Symmetric variant-vec coverage for the OTHER axis: same body and prefix
/// but different placeholders must be distinct variants. Placeholders only
/// surface for empty bodies, so an empty body exercises the axis end-to-end
/// (warm one placeholder, read the other, re-read the first).
#[test]
fn render_markdown_block_same_body_different_placeholders_via_value_vec() {
    let first = render_markdown_block("", "  ", "(no description)");
    assert_eq!(first.as_ref(), &["  (no description)".to_string()]);

    let second = render_markdown_block("", "  ", "(no body)");
    assert_eq!(second.as_ref(), &["  (no body)".to_string()]);

    let first_again = render_markdown_block("", "  ", "(no description)");
    assert_eq!(
        first, first_again,
        "repeating the first placeholder returns its own variant"
    );
}
