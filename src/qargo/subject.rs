//! Immutable subjects and their bound checking/reporting context.

use serde_json::{Value, json};

use crate::adapter::{self, CheckedSources};
use crate::report::{Diagnostic, Report};
use crate::snapshot::{FrozenSources, InputDirectory};

use super::manifest::{self, CapturedQrate};
use super::request::SourceSelection;

pub(super) enum CapturedSubject {
    Qrate(CapturedQrate),
    Standalone {
        sources: FrozenSources,
        original_root: InputDirectory,
    },
}

impl CapturedSubject {
    pub(super) fn capture(selection: &SourceSelection) -> Result<Self, Diagnostic> {
        match selection {
            SourceSelection::Qrate { manifest } => {
                Ok(Self::Qrate(manifest::capture(manifest.as_deref())?))
            }
            SourceSelection::Standalone(root) => {
                let original_root = InputDirectory::open(root)?;
                let sources = FrozenSources::from_files(original_root.collect(Some("qli"))?)?;
                Ok(Self::Standalone {
                    sources,
                    original_root,
                })
            }
        }
    }

    pub(super) fn sources(&self) -> &FrozenSources {
        match self {
            Self::Qrate(qrate) => qrate.sources(),
            Self::Standalone { sources, .. } => sources,
        }
    }

    pub(super) fn original_root(&self) -> &InputDirectory {
        match self {
            Self::Qrate(qrate) => qrate.held_source(),
            Self::Standalone { original_root, .. } => original_root,
        }
    }

    pub(super) fn bind_report(&self, mut report: Report, orchestrator: Value) -> Report {
        if let Self::Qrate(qrate) = self {
            report.envelope.diagnostics = report
                .envelope
                .diagnostics
                .into_iter()
                .map(|diagnostic| qrate.remap(diagnostic))
                .collect();
        }
        if let Some(result) = report.envelope.result.as_mut() {
            result["orchestrator"] = orchestrator;
            if let Self::Qrate(qrate) = self {
                result["input_id"] = json!(qrate.input_id());
            }
        }
        report.envelope.format = super::FORMAT.into();
        report
    }
}

/// Only ordinary checking of this qrate can construct this binding.
pub(super) struct CheckedQrate<'a> {
    qrate: &'a CapturedQrate,
    checked: CheckedSources,
}

impl CapturedQrate {
    pub(super) fn remap(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        if let Some(location) = diagnostic.primary.as_mut() {
            if self.sources().files().contains_key(&location.path) {
                location.path = format!("{}/{}", self.manifest().source.root, location.path);
            }
        }
        diagnostic
    }

    pub(super) fn check(&self) -> Result<CheckedQrate<'_>, Diagnostic> {
        Ok(CheckedQrate {
            qrate: self,
            checked: adapter::check(self.sources())?,
        })
    }
}

impl CheckedQrate<'_> {
    pub(super) fn qrate(&self) -> &CapturedQrate {
        self.qrate
    }

    pub(super) fn checked(&self) -> &CheckedSources {
        &self.checked
    }
}
