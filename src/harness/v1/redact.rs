//! Secret redaction (issue #380, CW00-09).
//!
//! Every nonempty declared secret byte sequence is replaced with
//! `<redacted>` in every observation — frame cells, streams, env, paths in
//! errors, the report, and stderr — before persistence. The redactor is
//! applied at the serialization boundary so no emitter can bypass it, and it
//! counts replacements into `Report.redaction_count`.

/// Replacement token for redacted secret occurrences.
pub const REDACTED: &str = "<redacted>";

/// A compiled redactor over the scenario's declared secrets.
#[derive(Debug, Clone)]
pub struct Redactor {
    /// Longest-first so an overlapping longer secret wins deterministically.
    secrets: Vec<String>,
}

impl Redactor {
    /// Compile a redactor. Empty secrets were rejected at parse time; they
    /// are skipped defensively here so redaction can never loop.
    #[must_use]
    pub fn new(secrets: &[String]) -> Self {
        let mut secrets: Vec<String> = secrets
            .iter()
            .filter(|secret| !secret.is_empty())
            .cloned()
            .collect();
        secrets.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        Self { secrets }
    }

    /// Redact all secret occurrences in `text`, returning the redacted text
    /// and the number of replacements.
    #[must_use]
    pub fn redact(&self, text: &str) -> (String, u64) {
        let mut out = text.to_string();
        let mut count = 0u64;
        for secret in &self.secrets {
            let occurrences = out.matches(secret.as_str()).count() as u64;
            if occurrences > 0 {
                out = out.replace(secret.as_str(), REDACTED);
                count += occurrences;
            }
        }
        (out, count)
    }

    /// Redact raw bytes: valid UTF-8 is redacted textually; invalid UTF-8 is
    /// lossily converted first so a secret can never hide behind invalid
    /// bytes.
    #[must_use]
    pub fn redact_bytes(&self, bytes: &[u8]) -> (String, u64) {
        self.redact(&String::from_utf8_lossy(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTED, Redactor};

    #[test]
    fn replaces_every_occurrence_and_counts() {
        let redactor = Redactor::new(&["hunter2".to_string()]);
        let (out, count) = redactor.redact("a hunter2 b hunter2 c");
        assert_eq!(out, format!("a {REDACTED} b {REDACTED} c"));
        assert_eq!(count, 2);
    }

    #[test]
    fn longer_secret_wins_over_contained_shorter_one() {
        let redactor = Redactor::new(&["token".to_string(), "token-extended".to_string()]);
        let (out, count) = redactor.redact("token-extended and token");
        assert_eq!(out, format!("{REDACTED} and {REDACTED}"));
        assert_eq!(count, 2);
    }

    #[test]
    fn multiple_distinct_secrets_all_redact() {
        let redactor = Redactor::new(&["alpha".to_string(), "beta".to_string()]);
        let (out, count) = redactor.redact("alpha beta alpha");
        assert_eq!(out, format!("{REDACTED} {REDACTED} {REDACTED}"));
        assert_eq!(count, 3);
    }

    #[test]
    fn no_secrets_is_identity() {
        let redactor = Redactor::new(&[]);
        let (out, count) = redactor.redact("nothing to hide");
        assert_eq!(out, "nothing to hide");
        assert_eq!(count, 0);
    }

    #[test]
    fn bytes_with_invalid_utf8_still_redact() {
        let redactor = Redactor::new(&["secret".to_string()]);
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend_from_slice(b"secret");
        let (out, count) = redactor.redact_bytes(&bytes);
        assert!(out.contains(REDACTED), "{out}");
        assert_eq!(count, 1);
    }
}
