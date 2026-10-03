//! Validate a formatter response completely before permitting source changes.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::qlifmt_engine;
use crate::report::{Diagnostic, Report, ToolIdentity};
use crate::snapshot::{self, Files, FrozenSources, InputDirectory};

use super::response::{Step, Tool, redact, syntax_response, transport};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FormatResult {
    source_count: usize,
    source_id: String,
    qleisli_check: Step,
    tool: Tool,
    formatted_source_id: Option<String>,
    changed_files: Vec<String>,
    updated_files: Vec<String>,
    check: bool,
    diff: String,
}

/// The candidate and its report are accepted together against these exact inputs.
pub(crate) struct FormatPlan<'a> {
    sources: &'a FrozenSources,
    candidate: Option<Files>,
    report: Report,
    check: bool,
}

pub(crate) fn prepare<'a>(
    sources: &'a FrozenSources,
    explicit: Option<&Path>,
    check: bool,
    orchestrator: &ToolIdentity,
) -> Result<FormatPlan<'a>, Diagnostic> {
    // The child always writes to its disposable working copy.
    super::execute(
        "qlifmt",
        explicit,
        sources,
        &[],
        orchestrator,
        |bytes, status, digest, stage| {
            let mut envelope = syntax_response::<FormatResult>(
                bytes,
                status,
                sources,
                "qlifmt",
                "fmt",
                digest,
                &[
                    "source_count",
                    "source_id",
                    "qleisli_check",
                    "tool",
                    "formatted_source_id",
                    "changed_files",
                    "updated_files",
                    "check",
                    "diff",
                ],
            )?;
            redact(&mut envelope, &[stage]);
            let result = envelope.result.as_mut().expect("validated result");
            if result["check"] != false || result["diff"] != "" {
                return Err(transport(
                    "The formatter did not follow the requested write mode.",
                ));
            }
            for field in ["changed_files", "updated_files"] {
                let labels: Vec<_> = result[field]
                    .as_array()
                    .expect("typed array")
                    .iter()
                    .map(|value| value.as_str().expect("typed string"))
                    .collect();
                if labels.windows(2).any(|pair| pair[0] >= pair[1])
                    || labels
                        .iter()
                        .any(|label| !sources.files().contains_key(*label))
                {
                    return Err(transport(
                        "Formatter returned invalid or repeated source paths.",
                    ));
                }
            }
            result["check"] = json!(check);
            let candidate = if status != 0 {
                // Partial child writes belong only to its disposable working copy.
                result["changed_files"] = json!([]);
                result["updated_files"] = json!([]);
                result["formatted_source_id"] = Value::Null;
                None
            } else {
                let formatted = snapshot::collect_tree(stage, None)?;
                qlifmt_engine::validate_formatted(sources.files(), &formatted)
                .map_err(|_| transport("Formatter output changed source tokens, comments, documentation attachment or file names."))?;
                let changed = qlifmt_engine::changed_files(sources.files(), &formatted);
                if result["formatted_source_id"]
                    != snapshot::digest_files("qleisli.source.v1", &formatted)
                    || result["changed_files"] != json!(changed)
                    || result["updated_files"] != json!(changed)
                {
                    return Err(transport(
                        "Formatter output differs from its reported source identity or changes.",
                    ));
                }
                result["updated_files"] = json!([]);
                Some(formatted)
            };
            Ok(FormatPlan {
                sources,
                candidate,
                check,
                report: Report {
                    envelope,
                    exit_code: status as u8,
                },
            })
        },
    )
}

impl FormatPlan<'_> {
    pub(crate) fn apply(mut self, original_root: &InputDirectory) -> Result<Report, Diagnostic> {
        let Some(formatted) = self.candidate else {
            return Ok(self.report);
        };
        let envelope = &mut self.report.envelope;
        let result = envelope.result.as_mut().expect("validated result");
        if self.check {
            result["diff"] = json!(qlifmt_engine::diff(self.sources.files(), &formatted));
            if !qlifmt_engine::changed_files(self.sources.files(), &formatted).is_empty() {
                envelope.outcome = "error".into();
                envelope.diagnostics.push(Diagnostic::error(
                    "formatting_required",
                    "qargo",
                    "Source files require formatting.",
                ));
            }
        } else if original_root.collect(Some("qli"))? != *self.sources.files() {
            envelope.outcome = "error".into();
            envelope.diagnostics.push(Diagnostic::error(
                "input_changed",
                "qargo",
                "Source inputs changed before formatting could be applied.",
            ));
        } else {
            match qlifmt_engine::apply_captured(original_root, self.sources.files(), &formatted) {
                Ok(updated) => result["updated_files"] = json!(updated),
                Err(failure) => {
                    result["updated_files"] = json!(failure.updated_files);
                    envelope.outcome = "error".into();
                    envelope.diagnostics.push(failure.diagnostic);
                }
            }
        }
        self.report.exit_code = u8::from(envelope.outcome == "error");
        Ok(self.report)
    }
}
