/// Typed checkbox value for Code Puppy autosave continuation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuickResume(bool);

impl From<bool> for QuickResume {
    fn from(enabled: bool) -> Self {
        Self(enabled)
    }
}

impl QuickResume {
    #[must_use]
    pub fn enabled(self) -> bool {
        self.0
    }

    pub fn toggle(&mut self) {
        self.0 = !self.0;
    }
}
