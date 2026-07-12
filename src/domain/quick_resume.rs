/// Typed checkbox value for Code Puppy autosave continuation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuickResume(pub bool);

impl QuickResume {
    #[must_use]
    pub fn enabled(self) -> bool {
        self.0
    }

    pub fn toggle(&mut self) {
        self.0 = !self.0;
    }
}
