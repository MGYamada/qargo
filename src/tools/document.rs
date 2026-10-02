//! Accept a complete documentation candidate before publishing its captured bytes.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::qlidoc_engine;
use crate::report::{Diagnostic, Report, ToolIdentity};
use crate::snapshot::{self, Files, FrozenSources};

use super::response::{Step, Tool, redact, syntax_response, transport, valid_digest};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DocResult {
    source_count: usize,
    source_id: String,
    qleisli_check: Step,
    tool: Tool,
    document_private_items: bool,
    artifact_path: Option<String>,
    files: Vec<OutputFile>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OutputFile {
    path: String,
    sha256: String,
}

pub(crate) struct DocumentPlan {
    report: Report,
    files: Option<Files>,
    tool_digest: String,
}

pub(crate) fn prepare(
    sources: &FrozenSources,
    explicit: Option<&Path>,
    include_private: bool,
    orchestrator: &ToolIdentity,
) -> Result<DocumentPlan, Diagnostic> {
    let stage_parent = tempfile::Builder::new()
        .prefix("qargo-doc-")
        .tempdir()
        .map_err(|error| {
            Diagnostic::error(
                "doc_output",
                "qargo",
                format!("Cannot stage documentation: {error}"),
            )
        })?;
    let stage = std::fs::canonicalize(stage_parent.path())
        .map_err(|error| {
            Diagnostic::error(
                "doc_output",
                "qargo",
                format!("Cannot resolve document stage: {error}"),
            )
        })?
        .join("document");
    let mut args = vec![format!("--output={}", stage.display())];
    if include_private {
        args.push("--document-private-items".into());
    }
    super::execute(
        "qlidoc",
        explicit,
        sources,
        &args,
        orchestrator,
        |bytes, status, digest, source_stage| {
            let mut envelope = syntax_response::<DocResult>(
                bytes,
                status,
                sources,
                "qlidoc",
                "doc",
                digest,
                &[
                    "source_count",
                    "source_id",
                    "qleisli_check",
                    "tool",
                    "document_private_items",
                    "artifact_path",
                    "files",
                ],
            )?;
            redact(
                &mut envelope,
                &[
                    source_stage,
                    stage_parent.path(),
                    stage.parent().expect("stage parent"),
                ],
            );
            let result = envelope.result.as_mut().expect("validated result");
            if result["document_private_items"] != include_private {
                return Err(transport(
                    "The document tool used different visibility options.",
                ));
            }
            let inventory = result["files"].as_array().expect("typed file inventory");
            let mut previous: Option<&str> = None;
            for file in inventory {
                let label = file["path"].as_str().expect("typed path");
                if snapshot::portable_relative(Path::new(label))
                    .ok()
                    .as_deref()
                    != Some(label)
                    || previous.is_some_and(|previous| previous >= label)
                    || !valid_digest(file["sha256"].as_str().expect("typed digest"))
                {
                    return Err(transport(
                        "Document tool returned unsafe paths, repeated files or invalid digests.",
                    ));
                }
                previous = Some(label);
            }
            let files = if status != 0 {
                result["artifact_path"] = Value::Null;
                None
            } else {
                let reported = result["artifact_path"]
                    .as_str()
                    .ok_or_else(|| transport("Document tool omitted its artifact path."))?;
                if std::fs::canonicalize(reported).ok() != std::fs::canonicalize(&stage).ok()
                    || !stage.is_dir()
                {
                    return Err(transport(
                        "Document tool wrote to an unexpected artifact path.",
                    ));
                }
                let expected = qlidoc_engine::render_files(sources.files(), include_private)
                    .map_err(|_| transport("Document tool reported success for invalid syntax."))?;
                let directories: BTreeSet<_> = expected
                    .keys()
                    .flat_map(|label| {
                        Path::new(label)
                            .ancestors()
                            .skip(1)
                            .filter(|parent| !parent.as_os_str().is_empty())
                            .map(Path::to_path_buf)
                    })
                    .collect();
                // Generated Markdown is bounded by known renderer output sizes.
                if !crate::publication::artifacts_match(&stage, &expected, &directories)
                    .map_err(|_| transport("Cannot validate generated document artifacts."))?
                {
                    return Err(transport(
                        "Document artifacts differ from the captured source documentation.",
                    ));
                }
                let manifest: Vec<_> = expected.iter().map(|(path, bytes)| {
                json!({"path":path, "sha256":format!("sha256:{:x}", Sha256::digest(bytes))})
            }).collect();
                if result["files"] != json!(manifest) {
                    return Err(transport(
                        "Document tool returned incorrect artifact identities.",
                    ));
                }
                Some(expected)
            };
            Ok(DocumentPlan {
                report: Report {
                    envelope,
                    exit_code: status as u8,
                },
                files,
                tool_digest: digest.into(),
            })
        },
    )
}

impl DocumentPlan {
    pub(crate) fn tool_digest(&self) -> &str {
        &self.tool_digest
    }

    pub(crate) fn publish(mut self, output: &Path) -> Report {
        if let Some(files) = self.files {
            let envelope = &mut self.report.envelope;
            envelope.result.as_mut().expect("validated result")["artifact_path"] =
                json!(output.to_string_lossy());
            if let Err(diagnostic) = qlidoc_engine::publish(output, &files) {
                envelope.outcome = "error".into();
                envelope.diagnostics.push(diagnostic);
            }
            self.report.exit_code = u8::from(envelope.outcome == "error");
        }
        self.report
    }
}
