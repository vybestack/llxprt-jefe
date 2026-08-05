//! Stable-code goldens for [`PluginCode`] (issue #389 CW-09, acceptance row D9).

use super::*;

#[test]
fn every_code_renders_its_exact_stable_text() {
    assert_eq!(PluginCode::Ambiguous.as_str(), "PLG-E501");
    assert_eq!(PluginCode::IndeterminateCommit.as_str(), "PLG-E503");
}

#[test]
fn display_matches_the_stable_text() {
    for code in PluginCode::ALL {
        assert_eq!(code.to_string(), code.as_str());
    }
}

#[test]
fn every_code_is_listed_exactly_once_and_is_unique() {
    let mut seen: Vec<&str> = PluginCode::ALL.iter().map(|code| code.as_str()).collect();
    let total = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), total, "codes must be unique");
}

#[test]
fn every_code_uses_the_plg_prefix_and_a_three_digit_number() {
    for code in PluginCode::ALL {
        let text = code.as_str();
        let digits = text
            .strip_prefix("PLG-E")
            .unwrap_or_else(|| panic!("{text} must use the PLG-E prefix"));
        assert_eq!(digits.len(), 3, "{text} must carry three digits");
        assert!(
            digits.bytes().all(|byte| byte.is_ascii_digit()),
            "{text} must carry three digits"
        );
    }
}

#[test]
fn every_code_carries_operator_facing_summary_text() {
    for code in PluginCode::ALL {
        assert!(
            !code.summary().is_empty(),
            "{code} must explain itself to an operator"
        );
    }
}
