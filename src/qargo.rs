//! Local qrate orchestration. Rust development tools are not qrate backends.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::PROFILE;
use crate::adapter;
use crate::report::{Diagnostic, Envelope, Report};
use crate::snapshot::{self, Files, FrozenSources};
use crate::tool_process::bounded_output;
use crate::tool_response::{lint_response, transport};

mod cli;
mod manifest;

use cli::{HELP, Options, parse};
use manifest::{CapturedQrate, capture};

const FORMAT: &str = "qargo.result";

fn error(id: &str, category: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(id, category, message)
}

fn remap_qrate(mut diagnostic: Diagnostic, qrate: &CapturedQrate) -> Diagnostic {
    if let Some(location) = diagnostic.primary.as_mut() {
        if qrate.sources.files.contains_key(&location.path) {
            location.path = format!("{}/{}", qrate.manifest.source.root, location.path);
        }
    }
    diagnostic
}

fn failed(command: &str, diagnostic: Diagnostic, result: Option<Value>) -> Report {
    Report {
        envelope: Envelope {
            format: FORMAT.into(),
            version: 1,
            command: command.into(),
            outcome: "error".into(),
            diagnostics: vec![diagnostic],
            result,
        },
        exit_code: 1,
    }
}

fn json_bytes(value: &Value) -> Result<Vec<u8>, Diagnostic> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| {
        error(
            "build_output",
            "qargo",
            format!("Cannot encode artifact: {e}"),
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn artifact_directories(qrate: &CapturedQrate, artifacts: &Files) -> BTreeSet<PathBuf> {
    let mut directories = BTreeSet::new();
    for path in artifacts.keys().map(Path::new).chain(
        [
            Path::new("snapshot").join(&qrate.manifest.source.root),
            Path::new("snapshot").join(&qrate.manifest.tests.root),
            Path::new("snapshot").join(&qrate.manifest.docs.root),
        ]
        .iter()
        .map(PathBuf::as_path),
    ) {
        let mut parent = if artifacts.contains_key(path.to_str().unwrap_or("")) {
            path.parent()
        } else {
            Some(path)
        };
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    directories
}

fn publication_error(error: crate::publication::PublicationError) -> Diagnostic {
    let id = match &error {
        crate::publication::PublicationError::Mismatch => "artifact_mismatch",
        crate::publication::PublicationError::UnsafePath(_) => "unsafe_output",
        crate::publication::PublicationError::Output(_) => "build_output",
    };
    Diagnostic::error(id, "qargo", error.to_string())
}

pub(crate) fn artifacts_match(
    base: &Path,
    artifacts: &Files,
    directories: &BTreeSet<PathBuf>,
) -> Result<bool, Diagnostic> {
    crate::publication::artifacts_match(base, artifacts, directories).map_err(publication_error)
}

fn build_artifacts(
    qrate: &CapturedQrate,
    checked: &adapter::CheckedSources,
    tool: &Value,
) -> Result<String, Diagnostic> {
    let record = json!({
        "format":"qargo.build-record", "version":1, "input_id":qrate.input_id,
        "qrate":{"name":qrate.manifest.qrate.name,"version":qrate.manifest.qrate.version},
        "source_count":checked.source_count, "qleisli_check":checked.qleisli_check,
        "profile":PROFILE, "tool":tool,
        "steps":[{"name":"manifest","status":"passed","reason":null},
                 {"name":"snapshot","status":"passed","reason":null},
                 {"name":"qleisli","status":checked.qleisli_check["status"],"reason":checked.qleisli_check["reason"]},
                 {"name":"QLT","status":"not_run","reason":"backend_unavailable"},
                 {"name":"qlidoc","status":"not_run","reason":"not_requested"},
                 {"name":"build","status":"passed","reason":null}]
    });
    let mut artifacts: Files = qrate
        .files
        .iter()
        .map(|(path, bytes)| (format!("snapshot/{path}"), bytes.clone()))
        .collect();
    artifacts.insert(
        "module-index.json".into(),
        json_bytes(&checked.module_index)?,
    );
    artifacts.insert("build-record.json".into(), json_bytes(&record)?);
    let directories = artifact_directories(qrate, &artifacts);
    let target = qrate.directory.join("target");
    let output = target.join("qargo");
    let hex = qrate
        .input_id
        .strip_prefix("sha256:")
        .ok_or_else(|| error("build_output", "qargo", "Invalid input identity."))?;
    let tool_hex = tool["executable_sha256"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| error("tool_identity", "tool", "Invalid executable identity."))?;
    let base = output.join(hex);
    let destination = base.join(tool_hex);
    let relative = format!("target/qargo/{hex}/{tool_hex}");
    crate::publication::publish(&destination, &artifacts, &directories)
        .map_err(publication_error)?;
    Ok(relative)
}

fn executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        if !metadata.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

fn find_tool(tool_name: &str, explicit: Option<&Path>) -> Result<PathBuf, Diagnostic> {
    if let Some(path) = explicit {
        if executable(path) {
            return fs::canonicalize(path).map_err(|e| {
                error(
                    "tool_missing",
                    "tool",
                    format!("Cannot resolve selected {tool_name}: {e}"),
                )
            });
        }
        return Err(error(
            "tool_missing",
            "tool",
            format!("The explicit {tool_name} path is not an executable file."),
        ));
    }
    let name = if cfg!(windows) {
        format!("{tool_name}.exe")
    } else {
        tool_name.to_owned()
    };
    if let Ok(current) = std::env::current_exe() {
        if let Some(directory) = current.parent() {
            let sibling = directory.join(&name);
            if executable(&sibling) {
                return Ok(sibling);
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(&name);
            if executable(&candidate) {
                return fs::canonicalize(&candidate).map_err(|e| {
                    error(
                        "tool_missing",
                        "tool",
                        format!("Cannot resolve {tool_name} on PATH: {e}"),
                    )
                });
            }
        }
    }
    Err(error(
        "tool_missing",
        "tool",
        format!(
            "{tool_name} was not found beside qargo or on PATH. Build the Rust engine during development or supply --{tool_name}=PATH."
        ),
    ))
}

fn lint(options: &Options) -> Result<Report, Diagnostic> {
    let qrate = if options.source.is_none() {
        Some(capture(options.manifest.as_deref())?)
    } else {
        None
    };
    let standalone;
    let sources = if let Some(source) = &options.source {
        standalone = FrozenSources::capture(source)?;
        &standalone
    } else {
        &qrate
            .as_ref()
            .expect("qrate captured for default lint source")
            .sources
    };
    let tool_path = find_tool("qlippy", options.qlippy.as_deref())?;
    let digest = snapshot::digest_path(&tool_path)?;
    let mut command = Command::new(&tool_path);
    command.arg(sources.root()).arg("--format=json");
    if options.deny_warnings {
        command.arg("--deny-warnings");
    }
    let (stdout, status) = bounded_output(&mut command)?;
    let mut envelope = lint_response(&stdout, status, sources, &digest, options.deny_warnings)?;
    if snapshot::digest_path(&tool_path)? != digest {
        return Err(transport(
            "The selected qlippy executable changed during execution.",
        ));
    }
    if options.source.is_none() {
        if let Some(qrate) = &qrate {
            envelope.diagnostics = envelope
                .diagnostics
                .into_iter()
                .map(|diagnostic| remap_qrate(diagnostic, qrate))
                .collect();
        }
    }
    envelope.format = FORMAT.into();
    if let Some(result) = envelope.result.as_mut() {
        result["orchestrator"] = crate::report::tool_info("qargo")?;
        if let Some(qrate) = &qrate {
            result["input_id"] = Value::String(qrate.input_id.clone());
        }
        let compiler_failed = envelope
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.category == "compiler");
        let (lint_status, lint_reason) = if compiler_failed {
            ("not_run", json!("compiler_error"))
        } else if sources.count() == 0 {
            ("not_run", json!("no_sources"))
        } else if status != 0 {
            ("failed", json!("denied_warnings"))
        } else {
            ("passed", Value::Null)
        };
        result["steps"] = json!([
            {"name":"snapshot","status":"passed","reason":null},
            {"name":"qleisli","status":result["qleisli_check"]["status"],"reason":result["qleisli_check"]["reason"]},
            {"name":"lint","status":lint_status,"reason":lint_reason}
        ]);
    }
    Ok(Report {
        envelope,
        exit_code: status as u8,
    })
}

fn source_tool_report(
    mut report: Report,
    qrate: Option<&CapturedQrate>,
    remap: bool,
) -> Result<Report, Diagnostic> {
    if remap {
        if let Some(qrate) = qrate {
            report.envelope.diagnostics = report
                .envelope
                .diagnostics
                .into_iter()
                .map(|diagnostic| remap_qrate(diagnostic, qrate))
                .collect();
        }
    }
    if let Some(result) = report.envelope.result.as_mut() {
        result["orchestrator"] = crate::report::tool_info("qargo")?;
        if let Some(qrate) = qrate {
            result["input_id"] = json!(qrate.input_id);
        }
    }
    Ok(report)
}

fn format_sources(options: &Options) -> Result<Report, Diagnostic> {
    let qrate = if options.source.is_none() {
        Some(capture(options.manifest.as_deref())?)
    } else {
        None
    };
    let standalone;
    let (sources, original_root) = if let Some(root) = &options.source {
        standalone = FrozenSources::capture(root)?;
        let root = fs::canonicalize(root)
            .map_err(|error| error.to_string())
            .map_err(|message| error("input", "qargo", message))?;
        (&standalone, root)
    } else {
        let qrate = qrate.as_ref().expect("default source qrate");
        (
            &qrate.sources,
            qrate.directory.join(&qrate.manifest.source.root),
        )
    };
    let tool = find_tool("qlifmt", options.qlifmt.as_deref())?;
    let digest = snapshot::digest_path(&tool)?;
    let report = crate::bundled::format(sources, &original_root, &tool, &digest, options.check)?;
    source_tool_report(report, qrate.as_ref(), options.source.is_none())
}

fn document_qrate(options: &Options) -> Result<Report, Diagnostic> {
    let qrate = capture(options.manifest.as_deref())?;
    let tool = find_tool("qlidoc", options.qlidoc.as_deref())?;
    let digest = snapshot::digest_path(&tool)?;
    let relative = format!(
        "target/qlidoc/{}/{}/{}",
        qrate.input_id.trim_start_matches("sha256:"),
        digest.trim_start_matches("sha256:"),
        if options.document_private_items {
            "all"
        } else {
            "public"
        }
    );
    let mut report = crate::bundled::document(
        &qrate.sources,
        &tool,
        &digest,
        &qrate.directory.join(&relative),
        options.document_private_items,
    )?;
    if let Some(result) = report.envelope.result.as_mut() {
        if result["artifact_path"].is_string() {
            result["artifact_path"] = json!(relative);
        }
    }
    source_tool_report(report, Some(&qrate), true)
}

fn execute(options: &Options) -> Result<Report, Diagnostic> {
    match options.command.as_str() {
        "help" => return Ok(Report::ok(FORMAT, "help", json!({"help":HELP}))),
        "version" => {
            return Ok(Report::ok(
                FORMAT,
                "version",
                crate::report::tool_info("qargo")?,
            ));
        }
        "lint" => return lint(options),
        "fmt" => return format_sources(options),
        "doc" => return document_qrate(options),
        _ => {}
    }
    let qrate = capture(options.manifest.as_deref())?;
    let tool = crate::report::tool_info("qargo")?;
    if options.command == "test" {
        let backend = "QLT";
        return Ok(failed(
            &options.command,
            error(
                "backend_unavailable",
                "qargo",
                format!("{backend} is not implemented; no substitute backend was run."),
            ),
            Some(json!({
                "input_id":qrate.input_id,"source_count":qrate.sources.count(),"tool":tool,
                "backend":{"name":backend,"status":"unavailable","reason":"not_implemented"}
            })),
        ));
    }
    let checked = match adapter::check(&qrate.sources) {
        Ok(checked) => checked,
        Err(diagnostic) => {
            return Ok(failed(
                &options.command,
                remap_qrate(diagnostic, &qrate),
                Some(json!({
                    "input_id":qrate.input_id,"source_count":qrate.sources.count(),"source_id":qrate.sources.source_id,
                    "qleisli_check":{"status":"failed","reason":"compiler_error"},"tool":tool
                })),
            ));
        }
    };
    let mut result = json!({"input_id":qrate.input_id,"source_count":checked.source_count,"source_id":qrate.sources.source_id,
        "qleisli_check":checked.qleisli_check,"tool":tool});
    if options.command == "build" {
        match build_artifacts(&qrate, &checked, &tool) {
            Ok(path) => result["artifact_path"] = Value::String(path),
            Err(diagnostic) => {
                result["build"] = json!({"status":"failed","reason":diagnostic.id});
                return Ok(failed("build", diagnostic, Some(result)));
            }
        }
    }
    Ok(Report::ok(FORMAT, &options.command, result))
}

/// Run a qargo command using arguments without the executable name.
pub fn run(args: &[OsString]) -> Report {
    let options = match parse(args) {
        Ok(options) => options,
        Err(diagnostic) => return Report::fail(FORMAT, "usage", diagnostic, 2),
    };
    execute(&options)
        .unwrap_or_else(|diagnostic| Report::fail(FORMAT, &options.command, diagnostic, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qrate_check_and_artifacts_use_captured_bytes_after_original_mutation() {
        let directory = tempfile::tempdir().unwrap();
        for root in ["src", "tests", "docs"] {
            fs::create_dir(directory.path().join(root)).unwrap();
        }
        let manifest = "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"0.1.0\"\nedition=\"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n";
        fs::write(directory.path().join("Qargo.toml"), manifest).unwrap();
        let source = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
        fs::write(directory.path().join("src/module.qli"), source).unwrap();
        let frozen = capture(Some(&directory.path().join("Qargo.toml"))).unwrap();
        fs::write(directory.path().join("src/module.qli"), "invalid syntax").unwrap();
        fs::write(directory.path().join("Qargo.toml"), "invalid manifest").unwrap();
        let checked = adapter::check(&frozen.sources).unwrap();
        assert_eq!(checked.qleisli_check["status"], "passed");
        let tool = crate::report::tool_info("qargo").unwrap();
        let artifact = build_artifacts(&frozen, &checked, &tool).unwrap();
        assert_eq!(
            fs::read_to_string(
                directory
                    .path()
                    .join(&artifact)
                    .join("snapshot/src/module.qli")
            )
            .unwrap(),
            source
        );
        assert_eq!(
            fs::read_to_string(directory.path().join(&artifact).join("snapshot/Qargo.toml"))
                .unwrap(),
            manifest
        );
        // Rebuilding the host engine must preserve the earlier input/tool record.
        let mut rebuilt_tool = tool.clone();
        rebuilt_tool["executable_sha256"] = json!(format!("sha256:{}", "f".repeat(64)));
        let rebuilt_artifact = build_artifacts(&frozen, &checked, &rebuilt_tool).unwrap();
        assert_ne!(artifact, rebuilt_artifact);
        for (path, expected_tool) in [(&artifact, &tool), (&rebuilt_artifact, &rebuilt_tool)] {
            let record: Value = serde_json::from_slice(
                &fs::read(directory.path().join(path).join("build-record.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(record["input_id"], frozen.input_id);
            assert_eq!(&record["tool"], expected_tool);
        }
        assert_eq!(build_artifacts(&frozen, &checked, &tool).unwrap(), artifact);
    }
}
