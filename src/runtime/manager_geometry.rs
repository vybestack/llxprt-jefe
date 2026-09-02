//! First-frame geometry commitment for the production runtime manager.
//!
//! Production constructs a pending manager after the workbench commit. No
//! session may spawn or attach until the app shell supplies the first resolved
//! frame's nonzero PTY dimensions exactly once.

use std::path::PathBuf;

use super::{RuntimeError, TmuxRuntimeManager};

impl TmuxRuntimeManager {
    /// Create a production runtime that cannot start or attach sessions until
    /// the first committed frame supplies its PTY viewport.
    #[must_use]
    pub fn pending() -> Self {
        Self::build(0, 0, false, None)
    }

    /// Create a pending runtime with an explicit session-host root.
    #[must_use]
    pub fn pending_with_session_host_root(session_host_root: PathBuf) -> Self {
        Self::build(0, 0, false, Some(session_host_root))
    }

    /// Whether a committed frame has configured this runtime's PTY viewport.
    #[must_use]
    pub fn initial_geometry_configured(&self) -> bool {
        self.initial_geometry_configured
    }

    /// The frame whose PTY viewport the runtime last adopted.
    ///
    /// `zero` until the first committed frame configures the runtime; every
    /// applied `resize_to_frame` advances it (issue #706 CWR3-02/CWR3-04).
    #[must_use]
    pub fn frame_generation(&self) -> crate::workbench::LayoutGeneration {
        self.frame_generation
    }

    /// Commit the first frame's nonzero PTY viewport exactly once.
    ///
    /// The viewport carries the creating frame's generation so the create
    /// effect can be proven current, exactly like a resize (issue #706
    /// CWR3-02).
    pub fn configure_initial_geometry(
        &mut self,
        viewport: crate::workbench::RuntimeViewport,
    ) -> Result<(), RuntimeError> {
        if viewport.rows == 0 || viewport.cols == 0 {
            return Err(RuntimeError::InvalidInitialGeometry {
                rows: viewport.rows,
                cols: viewport.cols,
            });
        }
        if self.initial_geometry_configured {
            return Err(RuntimeError::InitialGeometryAlreadyConfigured);
        }
        self.rows = viewport.rows;
        self.cols = viewport.cols;
        self.frame_generation = viewport.generation;
        self.initial_geometry_configured = true;
        Ok(())
    }

    pub(super) fn ensure_initial_geometry(&self) -> Result<(), RuntimeError> {
        if self.initial_geometry_configured {
            Ok(())
        } else {
            Err(RuntimeError::InitialGeometryUnavailable)
        }
    }
}
