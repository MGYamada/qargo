//! Immutable subjects and their bound checking/reporting context.

use serde_json::{Value, json};

use crate::adapter::{self, CheckedSources};
use crate::report::{Diagnostic, Report};
use crate::snapshot::{Files, FrozenSources, InputDirectory};

use super::environment::EffectiveStd;
use super::manifest::{self, CapturedQrate};
use super::request::SourceSelection;

pub(super) enum CapturedSubject {
    Qrate(Box<CapturedQrate>),
    Standalone {
        sources: FrozenSources,
        original_root: InputDirectory,
        configuration: Option<Files>,
    },
}

impl CapturedSubject {
    pub(super) fn capture(selection: &SourceSelection) -> Result<Self, Diagnostic> {
        match selection {
            SourceSelection::Qrate { manifest } => Ok(Self::Qrate(Box::new(manifest::capture(
                manifest.as_deref(),
            )?))),
            SourceSelection::Standalone(root) => {
                let original_root = InputDirectory::open(root)?;
                let sources = FrozenSources::from_files(original_root.collect(Some("qli"))?)?;
                Ok(Self::Standalone {
                    sources,
                    original_root,
                    configuration: None,
                })
            }
        }
    }

    pub(super) fn capture_for_lint(selection: &SourceSelection) -> Result<Self, Diagnostic> {
        match selection {
            SourceSelection::Qrate { manifest } => Ok(Self::Qrate(Box::new(manifest::capture(
                manifest.as_deref(),
            )?))),
            SourceSelection::Standalone(root) => {
                let (sources, configuration, original_root) =
                    adapter::CapturedCheckInput::capture(root)?.into_parts();
                Ok(Self::Standalone {
                    sources,
                    original_root,
                    configuration: Some(configuration),
                })
            }
        }
    }

    pub(super) fn lint_configuration(&self) -> Result<Files, Diagnostic> {
        match self {
            Self::Qrate(qrate) => Ok(qrate.source_configuration()),
            Self::Standalone {
                configuration: Some(configuration),
                ..
            } => Ok(configuration.clone()),
            Self::Standalone { .. } => Err(Diagnostic::error(
                "input",
                "qargo",
                "Lint source configuration was not captured.",
            )),
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
    effective_std: EffectiveStd,
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

    pub(super) fn check(
        &self,
        effective_std: &EffectiveStd,
        configuration: &Files,
    ) -> Result<CheckedQrate<'_>, Diagnostic> {
        if configuration != &self.source_configuration() {
            return Err(Diagnostic::error(
                "snapshot_mismatch",
                "tool",
                "Source configuration differs from the captured qrate inputs.",
            ));
        }
        Ok(CheckedQrate {
            qrate: self,
            checked: adapter::check(self.sources(), configuration)?,
            effective_std: effective_std.clone(),
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

    pub(super) fn effective_std(&self) -> &EffectiveStd {
        &self.effective_std
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_configuration_cannot_be_checked_under_the_qrate_input_id() {
        let root = tempfile::tempdir().unwrap();
        for name in ["src", "tests", "docs"] {
            std::fs::create_dir(root.path().join(name)).unwrap();
        }
        std::fs::write(root.path().join("Qargo.toml"), "schema-version=2\n[qrate]\nname=\"bound\"\nversion=\"2.1.0\"\nedition=\"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
        let qrate = manifest::capture(Some(&root.path().join("Qargo.toml"))).unwrap();
        let tool = crate::report::tool_info("qargo", crate::VERSION).unwrap();
        let environment = EffectiveStd::capture(&tool).unwrap();
        let mut configuration = qrate.source_configuration();
        configuration
            .get_mut("Qargo.toml")
            .unwrap()
            .extend_from_slice(b"# different captured bytes\n");
        let diagnostic = qrate.check(&environment, &configuration).err().unwrap();
        assert_eq!(diagnostic.id, "snapshot_mismatch");
        assert_eq!(qrate.sources().count(), 0);
    }
}
