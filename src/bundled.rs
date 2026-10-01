//! Validate source-tool transports before updating sources or publishing docs.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::report::{Diagnostic, Envelope, Report};
use crate::snapshot::{self, FrozenSources};
use crate::tool_executable::SelectedExecutable;
use crate::tool_process::{ToolOutput, bounded_output};
use crate::tool_response::{
    DIAGNOSTIC_FIELDS, ENVELOPE_FIELDS, Response, Step, Tool, closed_object, location_valid,
    tool_matches, transport,
};
use crate::{qlidoc_engine, qlifmt_engine};

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

fn redact(envelope: &mut Envelope, paths: &[&Path]) {
    for diagnostic in &mut envelope.diagnostics {
        for path in paths {
            let prefix = path.to_string_lossy();
            diagnostic.message = diagnostic.message.replace(prefix.as_ref(), "<private>");
            if let Some(suggestion) = diagnostic.suggestion.as_mut() {
                *suggestion = suggestion
                    .replace(prefix.as_ref(), "<private>")
                    .into_boxed_str();
            }
        }
    }
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn decode<R: DeserializeOwned + Serialize>(
    bytes: &[u8],
    status: i32,
    sources: &FrozenSources,
    name: &str,
    command: &str,
    digest: &str,
    fields: &[&str],
) -> Result<Envelope, Diagnostic> {
    // The typed pass also rejects duplicate keys throughout the closed schema.
    let typed: Response<R> = serde_json::from_slice(bytes).map_err(|_| {
        transport("Source tool returned repeated fields, unsupported fields or invalid types.")
    })?;
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| transport("Source tool returned incomplete JSON."))?;
    let _typed_shape = serde_json::to_value(typed)
        .map_err(|_| transport("Invalid typed source-tool response."))?;
    if !closed_object(&value, ENVELOPE_FIELDS)
        || value["format"] != format!("{name}.result")
        || value["version"] != 1
        || value["command"] != command
    {
        return Err(transport("Source tool returned an incompatible envelope."));
    }
    let result = &value["result"];
    if !closed_object(result, fields)
        || result["source_count"].as_u64() != Some(sources.count() as u64)
        || result["source_id"] != sources.source_id
        || result["qleisli_check"] != qlippy_engine::source::syntax_step(sources.count())
    {
        return Err(transport(
            "Source tool did not bind its result to the captured inputs and syntax-only processing.",
        ));
    }
    let tool = &result["tool"];
    if !tool_matches(tool, name, digest) {
        return Err(transport(
            "Source tool identity or profile is incompatible.",
        ));
    }
    let diagnostics = value["diagnostics"]
        .as_array()
        .ok_or_else(|| transport("Invalid source-tool diagnostics."))?;
    for diagnostic in diagnostics {
        if !closed_object(diagnostic, DIAGNOSTIC_FIELDS)
            || diagnostic["severity"] != "error"
            || !["qargo", "compiler", "tool"]
                .contains(&diagnostic["category"].as_str().unwrap_or(""))
            || diagnostic["id"].as_str().is_none_or(str::is_empty)
            || !diagnostic["primary"].is_null()
                && !location_valid(&diagnostic["primary"], sources, false)
        {
            return Err(transport(
                "Source-tool diagnostics do not match the original source bytes.",
            ));
        }
    }
    if !(status == 0 && value["outcome"] == "ok" && diagnostics.is_empty()
        || status == 1 && value["outcome"] == "error" && !diagnostics.is_empty())
    {
        return Err(transport(
            "Source-tool exit code, outcome and diagnostics are inconsistent.",
        ));
    }
    let mut envelope: Envelope =
        serde_json::from_value(value).map_err(|_| transport("Invalid source-tool envelope."))?;
    redact(&mut envelope, &[sources.root()]);
    envelope.format = "qargo.result".into();
    Ok(envelope)
}

fn invoke(
    tool: &SelectedExecutable,
    sources: &FrozenSources,
    extra: &[String],
) -> Result<ToolOutput, Diagnostic> {
    let mut command = tool.command()?;
    command.arg(sources.root()).arg("--format=json").args(extra);
    let output = bounded_output(&mut command)?;
    tool.verify()?;
    Ok(output)
}

pub(crate) fn format(
    sources: &FrozenSources,
    original_root: &Path,
    tool: &SelectedExecutable,
    check: bool,
) -> Result<Report, Diagnostic> {
    // The child modifies a private copy; the parent alone updates user sources.
    let digest = tool.digest();
    let output = invoke(tool, sources, &[])?;
    let status = output.status;
    let mut envelope = decode::<FormatResult>(
        &output.stdout,
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
    let result = envelope.result.as_mut().expect("validated result");
    if result["check"] != false || result["diff"] != "" {
        return Err(transport(
            "The formatter did not follow the requested write mode.",
        ));
    }
    for field in ["changed_files", "updated_files"] {
        let paths = result[field].as_array().expect("typed array");
        let labels: Vec<_> = paths
            .iter()
            .map(|value| value.as_str().expect("typed string"))
            .collect();
        if labels.windows(2).any(|pair| pair[0] >= pair[1])
            || labels
                .iter()
                .any(|label| !sources.files.contains_key(*label))
        {
            return Err(transport(
                "Formatter returned invalid or repeated source paths.",
            ));
        }
    }
    result["check"] = json!(check);
    if status != 0 {
        // Child-side partial writes affected only the private copy.
        result["updated_files"] = json!([]);
        result["formatted_source_id"] = Value::Null;
        output.accept();
        return Ok(Report {
            envelope,
            exit_code: 1,
        });
    }
    let formatted = snapshot::collect_tree(sources.root(), None)?;
    qlifmt_engine::validate_formatted(&sources.files, &formatted)
        .map_err(|_| transport("Formatter output changed source tokens, comments, documentation attachment or file names."))?;
    let changed = qlifmt_engine::changed_files(&sources.files, &formatted);
    if result["formatted_source_id"] != snapshot::digest_files("qleisli.source.v1", &formatted)
        || result["changed_files"] != json!(changed)
        || result["updated_files"] != json!(changed)
    {
        return Err(transport(
            "Formatter output differs from its reported source identity or changes.",
        ));
    }
    output.accept();
    result["updated_files"] = json!([]);
    if check {
        result["diff"] = json!(qlifmt_engine::diff(&sources.files, &formatted));
        if !changed.is_empty() {
            envelope.outcome = "error".into();
            envelope.diagnostics.push(Diagnostic::error(
                "formatting_required",
                "qargo",
                "Source files require formatting.",
            ));
        }
    } else if snapshot::collect_tree(original_root, Some("qli"))? != sources.files {
        envelope.outcome = "error".into();
        envelope.diagnostics.push(Diagnostic::error(
            "input_changed",
            "qargo",
            "Source inputs changed before formatting could be applied.",
        ));
    } else {
        match qlifmt_engine::apply_files(original_root, &sources.files, &formatted) {
            Ok(updated) => result["updated_files"] = json!(updated),
            Err(failure) => {
                result["updated_files"] = json!(failure.updated_files);
                envelope.outcome = "error".into();
                envelope.diagnostics.push(failure.diagnostic);
            }
        }
    }
    let exit_code = u8::from(envelope.outcome == "error");
    Ok(Report {
        envelope,
        exit_code,
    })
}

pub(crate) fn document(
    sources: &FrozenSources,
    tool: &SelectedExecutable,
    output: &Path,
    include_private: bool,
) -> Result<Report, Diagnostic> {
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
    let digest = tool.digest();
    let output_capture = invoke(tool, sources, &args)?;
    let status = output_capture.status;
    let mut envelope = decode::<DocResult>(
        &output_capture.stdout,
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
        &[stage_parent.path(), stage.parent().expect("stage parent")],
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
    if status != 0 {
        result["artifact_path"] = Value::Null;
        output_capture.accept();
        return Ok(Report {
            envelope,
            exit_code: 1,
        });
    }
    let reported = result["artifact_path"]
        .as_str()
        .ok_or_else(|| transport("Document tool omitted its artifact path."))?;
    if std::fs::canonicalize(reported).ok() != std::fs::canonicalize(&stage).ok() || !stage.is_dir()
    {
        return Err(transport(
            "Document tool wrote to an unexpected artifact path.",
        ));
    }
    let expected = qlidoc_engine::render_files(&sources.files, include_private)
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
    // Bound reads by known renderer output sizes, rather than source-byte limits:
    // generated Markdown includes headers and can exceed its input size.
    if !crate::qargo::artifacts_match(&stage, &expected, &directories)
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
    output_capture.accept();
    // Publish from the captured, validated bytes, not another read of child files.
    result["artifact_path"] = json!(output.to_string_lossy());
    if let Err(diagnostic) = qlidoc_engine::publish(output, &expected) {
        envelope.outcome = "error".into();
        envelope.diagnostics.push(diagnostic);
    }
    let exit_code = u8::from(envelope.outcome == "error");
    Ok(Report {
        envelope,
        exit_code,
    })
}
