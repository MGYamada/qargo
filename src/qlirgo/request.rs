//! Legal operations and input scopes, independent of argument spelling and I/O.

use std::path::PathBuf;

pub(super) enum SourceSelection {
    Qrate { manifest: Option<PathBuf> },
    Standalone(PathBuf),
}

#[derive(Clone, Copy)]
pub(super) enum QrateOperation {
    Check,
    Build,
    Test,
}

impl QrateOperation {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Build => "build",
            Self::Test => "test",
        }
    }
}

pub(super) enum Request {
    Help,
    Version,
    Qrate {
        operation: QrateOperation,
        manifest: Option<PathBuf>,
    },
    Lint {
        input: SourceSelection,
        executable: Option<PathBuf>,
        deny_warnings: bool,
    },
    Format {
        input: SourceSelection,
        executable: Option<PathBuf>,
        check: bool,
    },
    Document {
        manifest: Option<PathBuf>,
        executable: Option<PathBuf>,
        include_private: bool,
    },
}

impl Request {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Version => "version",
            Self::Qrate { operation, .. } => operation.name(),
            Self::Lint { .. } => "lint",
            Self::Format { .. } => "fmt",
            Self::Document { .. } => "doc",
        }
    }
}
