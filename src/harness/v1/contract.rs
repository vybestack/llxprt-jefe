//! Schema-1 scenario contract types (issue #380).
//!
//! These DTOs mirror the closed grammar exactly. Construction goes through
//! the strict parse layer; every enum is exhaustive so downstream `match`
//! sites stay compiler-checked. Mode values are constrained integers
//! (448/493 for dirs, 384/420/448/493 for files) validated during parsing.

/// A fully parsed and validated schema-1 scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioV1 {
    pub name: String,
    pub platform: Platform,
    pub terminal: Size,
    pub workspace: WorkspaceSpec,
    pub steps: Vec<Step>,
    pub secrets: Vec<String>,
}

/// Supported scenario platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Macos,
    Linux,
}

impl Platform {
    /// The platform the current build runs on, if supported.
    #[must_use]
    pub const fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else {
            None
        }
    }
}

/// Terminal dimensions: cols 1..=500, rows 1..=200.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

/// Workspace fixture set. The workspace root mode is fixed at 448 (0o700).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSpec {
    pub dirs: Vec<DirSpec>,
    pub files: Vec<FileSpec>,
    pub env: Vec<EnvVar>,
}

/// A directory fixture with mode 448 (0o700) or 493 (0o755).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirSpec {
    pub path: RelPath,
    pub mode: u32,
}

/// A file fixture with mode 384 (0o600), 420 (0o644), 448 (0o700), or
/// 493 (0o755).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpec {
    pub path: RelPath,
    pub content: FileContent,
    pub mode: u32,
}

/// File content, decoded at parse time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    Utf8(String),
    Base64(Vec<u8>),
}

impl FileContent {
    /// The raw bytes this content materializes to.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Utf8(text) => text.as_bytes(),
            Self::Base64(bytes) => bytes,
        }
    }
}

/// An environment variable declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// A validated workspace-relative path: UTF-8, 1..=4096 bytes, `/` separated,
/// no root/prefix, empty, `.`, `..`, NUL, or backslash component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelPath(pub String);

impl RelPath {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One scenario operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Write {
        file: FileSpec,
    },
    Mkdir {
        dir: DirSpec,
    },
    Remove {
        path: RelPath,
    },
    Capture {
        name: String,
        path: RelPath,
        behavior: CaptureBehavior,
    },
    Launch {
        argv: Vec<String>,
        env: Vec<EnvVar>,
        cwd: RelPath,
    },
    Key {
        key: String,
        modifiers: Vec<Modifier>,
    },
    Text {
        text: String,
    },
    Resize {
        size: Size,
    },
    Wait {
        source: WaitSource,
        literal: String,
        timeout_ms: u64,
    },
    AssertFrame {
        contains: Vec<String>,
        absent: Vec<String>,
    },
    AssertCapture {
        capture: CaptureExpectation,
    },
    AssertFile {
        file: FileExpectation,
    },
    Restart,
    Finish,
}

impl Step {
    /// Stable operation name used in reports and diagnostics.
    #[must_use]
    pub const fn op_name(&self) -> &'static str {
        match self {
            Self::Write { .. } => "write",
            Self::Mkdir { .. } => "mkdir",
            Self::Remove { .. } => "remove",
            Self::Capture { .. } => "capture",
            Self::Launch { .. } => "launch",
            Self::Key { .. } => "key",
            Self::Text { .. } => "text",
            Self::Resize { .. } => "resize",
            Self::Wait { .. } => "wait",
            Self::AssertFrame { .. } => "assert-frame",
            Self::AssertCapture { .. } => "assert-capture",
            Self::AssertFile { .. } => "assert-file",
            Self::Restart => "restart",
            Self::Finish => "finish",
        }
    }
}

/// Key modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Alt,
    Control,
    Shift,
}

/// Wait sources. In a real PTY the app's stdout and stderr share one stream;
/// both sources scan the merged PTY byte stream, while `frame` scans rendered
/// screen rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitSource {
    Frame,
    Stdout,
    Stderr,
}

/// Configured behavior for a capture shim executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureBehavior {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
    pub stdin_limit: u64,
    pub hang: bool,
    pub spawn_child_hang: bool,
}

/// Expected fields for one recorded capture invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureExpectation {
    pub name: String,
    pub invocation: u64,
    pub argv: Vec<String>,
    pub env: Vec<EnvVar>,
    pub cwd: String,
    pub stdin: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<u8>,
    pub signal: Option<i32>,
}

/// Expected state of a workspace file. `exists` defaults to `true`; `content`
/// may only be supplied when `exists` is `true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExpectation {
    pub path: RelPath,
    pub exists: bool,
    pub content: Option<FileContent>,
}
