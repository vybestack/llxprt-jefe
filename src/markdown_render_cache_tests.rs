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
    assert_eq!(ph_a, vec!["  (no description)"]);
    assert_eq!(ph_b, vec!["  (no body)"]);

    // Differing markdown (key includes body).
    let md_a = render_markdown_block("alpha", "  ", "(no body)");
    let md_b = render_markdown_block("beta", "  ", "(no body)");
    assert_ne!(md_a, md_b, "key includes body");
    assert!(md_a.iter().any(|l| l.contains("alpha")));
    assert!(md_b.iter().any(|l| l.contains("beta")));
}
