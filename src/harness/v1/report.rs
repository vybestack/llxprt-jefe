//! Deterministic schema-1 run report (issue #380).
//!
//! One report is emitted per run, always schema 1, fully redacted at this
//! serialization boundary before persistence. Frame and stream content are
//! bounded by the contract limits.

use serde::Serialize;

use super::capture::CaptureRecord;
use super::error::HarnessError;
use super::limits::MAX_FRAMES;
use super::redact::Redactor;

/// A captured terminal frame.
#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub cols: u16,
    pub rows: u16,
    pub lines: Vec<String>,
}

/// One executed step's outcome.
#[derive(Debug, Clone, Serialize)]
pub struct StepResult {
    pub index: usize,
    pub op: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Recorded invocations for one capture.
#[derive(Debug, Clone, Serialize)]
pub struct CaptureReport {
    pub name: String,
    pub invocations: Vec<CaptureRecord>,
}

/// How the app-under-test exited.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct AppExit {
    pub exit_code: Option<u32>,
}

/// The complete run report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: u32,
    pub scenario: String,
    pub status: String,
    pub workspace: String,
    pub steps: Vec<StepResult>,
    pub captures: Vec<CaptureReport>,
    pub frames: Vec<Frame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_exit: Option<AppExit>,
    pub redaction_count: u64,
}

impl Report {
    /// Start an empty passing report for `scenario` in `workspace`.
    #[must_use]
    pub fn new(scenario: &str, workspace: &str) -> Self {
        Self {
            schema: 1,
            scenario: scenario.to_string(),
            status: "passed".to_string(),
            workspace: workspace.to_string(),
            steps: Vec::new(),
            captures: Vec::new(),
            frames: Vec::new(),
            app_exit: None,
            redaction_count: 0,
        }
    }

    /// Record a frame, bounded by the contract frame limit.
    pub fn push_frame(&mut self, frame: Frame) {
        if self.frames.len() < MAX_FRAMES {
            self.frames.push(frame);
        }
    }

    /// Serialize with every field redacted at this boundary. Counts all
    /// replacements into `redaction_count`.
    ///
    /// # Errors
    ///
    /// `HAR-E005` on serialization failure.
    pub fn to_redacted_json(&self, redactor: &Redactor) -> Result<String, HarnessError> {
        let mut redacted = self.clone();
        let mut count = 0u64;
        let mut apply = |text: &mut String| {
            let (out, n) = redactor.redact(text);
            *text = out;
            count += n;
        };
        apply(&mut redacted.scenario);
        apply(&mut redacted.workspace);
        for step in &mut redacted.steps {
            if let Some(error) = &mut step.error {
                apply(error);
            }
        }
        for capture in &mut redacted.captures {
            for record in &mut capture.invocations {
                apply(&mut record.cwd);
                apply(&mut record.stdin);
                apply(&mut record.stdout);
                apply(&mut record.stderr);
                for (name, value) in &mut record.env {
                    apply(name);
                    apply(value);
                }
                for arg in &mut record.argv {
                    apply(arg);
                }
            }
        }
        for frame in &mut redacted.frames {
            for line in &mut frame.lines {
                apply(line);
            }
        }
        redacted.redaction_count = self.redaction_count + count;
        serde_json::to_string_pretty(&redacted)
            .map_err(|err| HarnessError::process(format!("encode report: {err}")))
    }
}

#[cfg(test)]
mod tests {
    use super::super::redact::Redactor;
    use super::{Frame, Report, StepResult};

    #[test]
    fn redacts_every_report_field_and_counts() {
        let mut report = Report::new("scn-hunter2", "/ws/hunter2");
        report.steps.push(StepResult {
            index: 0,
            op: "wait".to_string(),
            status: "failed".to_string(),
            error: Some("literal 'hunter2' not found".to_string()),
        });
        report.push_frame(Frame {
            cols: 10,
            rows: 1,
            lines: vec!["say hunter2 twice hunter2".to_string()],
        });
        let redactor = Redactor::new(&["hunter2".to_string()]);
        let json = report
            .to_redacted_json(&redactor)
            .unwrap_or_else(|err| panic!("should encode: {err}"));
        assert!(!json.contains("hunter2"), "secret leaked: {json}");
        assert!(json.contains("<redacted>"));
        assert!(json.contains("\"redaction_count\": 5"), "{json}");
        assert!(json.contains("\"schema\": 1"));
    }

    #[test]
    fn frame_limit_is_enforced() {
        let mut report = Report::new("s", "/w");
        for _ in 0..3000 {
            report.push_frame(Frame {
                cols: 1,
                rows: 1,
                lines: vec![String::new()],
            });
        }
        assert_eq!(report.frames.len(), super::super::limits::MAX_FRAMES);
    }
}
