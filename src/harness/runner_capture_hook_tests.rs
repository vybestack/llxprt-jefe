use super::*;

#[derive(Default)]
struct RecordingHook {
    labels: Vec<String>,
}

impl CaptureHook for RecordingHook {
    fn before_capture(&mut self, label: &str) -> Result<(), String> {
        self.labels.push(label.to_string());
        Ok(())
    }
}

struct FailingHook {
    reason: String,
}

impl CaptureHook for FailingHook {
    fn before_capture(&mut self, _label: &str) -> Result<(), String> {
        Err(self.reason.clone())
    }
}

type SharedCounter = std::sync::Arc<std::sync::Mutex<usize>>;

struct OrderingHook {
    capture_counts_at_hook_call: Vec<usize>,
    driver_captures: SharedCounter,
}

impl CaptureHook for OrderingHook {
    fn before_capture(&mut self, _label: &str) -> Result<(), String> {
        let count = *self
            .driver_captures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.capture_counts_at_hook_call.push(count);
        Ok(())
    }
}

struct OrderingDriver {
    screens: VecDeque<ScreenCapture>,
    capture_counter: SharedCounter,
    capture_count: usize,
}

impl HarnessDriver for OrderingDriver {
    type Error = FakeError;

    fn send_line(&mut self, _line: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn send_type(&mut self, _text: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn send_key(&mut self, _key: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn send_keys(&mut self, _keys: &[String]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn capture_screen(&mut self) -> Result<ScreenCapture, Self::Error> {
        self.capture_count += 1;
        *self
            .capture_counter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = self.capture_count;
        Ok(self
            .screens
            .pop_front()
            .unwrap_or_else(|| ScreenCapture::new(1, 80, vec![String::new()])))
    }

    fn capture_screen_with_color(&mut self) -> Result<Vec<String>, Self::Error> {
        Ok(Vec::new())
    }

    fn capture_scrollback(&mut self, _lines: u32) -> Result<ScrollbackSample, Self::Error> {
        Ok(ScrollbackSample::new(0, Vec::new()))
    }

    fn pane_status(&mut self) -> Result<PaneStatus, Self::Error> {
        Ok(PaneStatus { dead: false })
    }

    fn history_size(&mut self) -> Result<u64, Self::Error> {
        Ok(0)
    }

    fn copy_mode(&mut self, _enabled: bool) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn default_run_uses_plain_text_policy_without_hook_side_effects() {
    let scenario = scenario(r#"[ { "capture": "plain" } ]"#);
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let mut driver = FakeDriver::default().with_screens(&["plain text screen"]);

    let summary = run_scenario(&scenario, &mut driver, Some(artifact_dir.path()))
        .value_or_panic("default run should pass");

    assert_eq!(summary.captures, vec!["plain".to_string()]);
    assert!(artifact_dir.path().join("plain.screen.txt").exists());
    assert!(!artifact_dir.path().join("plain.screen.ansi").exists());
    assert_eq!(driver.screen_capture_count, 1);
}

#[test]
fn recording_hook_receives_exact_capture_labels() {
    let scenario = scenario(
        r#"[
            { "capture": "first" },
            { "capture": "second-nested/deep" }
        ]"#,
    );
    let mut driver = FakeDriver::default().with_screens(&["screen one", "screen two"]);
    let mut hook = RecordingHook::default();

    let summary = run_scenario_with_hook(
        &scenario,
        &mut driver,
        None,
        &RunOptions::default(),
        &mut hook,
    )
    .value_or_panic("run should pass");

    assert_eq!(
        hook.labels,
        vec!["first".to_string(), "second-nested/deep".to_string()]
    );
    assert_eq!(summary.captures, hook.labels);
}

#[test]
fn hook_runs_before_driver_capture() {
    let shared_count: SharedCounter = std::sync::Arc::new(std::sync::Mutex::new(0));
    let artifact_dir = tempfile::tempdir().value_or_panic("tempdir");
    let mut hook = OrderingHook {
        capture_counts_at_hook_call: Vec::new(),
        driver_captures: shared_count.clone(),
    };
    let mut driver = OrderingDriver {
        screens: VecDeque::from([
            ScreenCapture::new(1, 80, vec!["a".to_string()]),
            ScreenCapture::new(1, 80, vec!["b".to_string()]),
        ]),
        capture_counter: shared_count,
        capture_count: 0,
    };

    let scenario = scenario(r#"[ { "capture": "a" }, { "capture": "b" } ]"#);
    run_scenario_with_hook(
        &scenario,
        &mut driver,
        Some(artifact_dir.path()),
        &RunOptions::default(),
        &mut hook,
    )
    .value_or_panic("run should pass");

    assert_eq!(hook.capture_counts_at_hook_call, vec![0, 1]);
}

#[test]
fn failing_hook_stops_before_screen_capture_and_surfaces_reason() {
    let scenario = scenario(r#"[ { "capture": "will-fail" } ]"#);
    let mut driver = FakeDriver::default().with_screens(&["should not be captured"]);

    let err = error_or_panic(
        run_scenario_with_hook(
            &scenario,
            &mut driver,
            None,
            &RunOptions::default(),
            &mut FailingHook {
                reason: "status suppression timed out".to_string(),
            },
        ),
        "hook failure should produce error",
    );

    let RunnerError::Driver(message) = err else {
        panic!("expected driver error");
    };
    assert!(message.contains("pre-capture hook failed"));
    assert!(message.contains("will-fail"));
    assert!(message.contains("status suppression timed out"));
    assert_eq!(driver.screen_capture_count, 0);
}

#[test]
fn no_hook_accepts_all_capture_labels() {
    let mut hook = NoHook;
    assert!(hook.before_capture("anything").is_ok());
    assert!(hook.before_capture("").is_ok());
}
