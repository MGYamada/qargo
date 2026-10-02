//! Execute typed requests through captured subjects and accepted effect plans.

use std::path::Path;

use serde_json::{Value, json};

use crate::report::{Diagnostic, Report, ToolIdentity, tool_info};
use crate::tools;

use super::manifest;
use super::request::{QrateOperation, Request};
use super::subject::CapturedSubject;
use super::{FORMAT, build, cli, error};

fn failed(command: &str, diagnostic: Diagnostic, result: Value) -> Report {
    let mut report = Report::fail(FORMAT, command, diagnostic, 1);
    report.envelope.result = Some(result);
    report
}

fn qrate_operation(
    operation: QrateOperation,
    manifest: Option<&Path>,
) -> Result<Report, Diagnostic> {
    let qrate = manifest::capture(manifest)?;
    let tool = tool_info("qargo")?;
    if let QrateOperation::Test = operation {
        return Ok(failed(
            "test",
            error(
                "backend_unavailable",
                "qargo",
                "QLT is not implemented; no substitute backend was run.",
            ),
            json!({
                "input_id":qrate.input_id(),"source_count":qrate.sources().count(),"tool":tool,
                "backend":{"name":"QLT","status":"unavailable","reason":"not_implemented"}
            }),
        ));
    }
    let subject = match qrate.check() {
        Ok(subject) => subject,
        Err(diagnostic) if diagnostic.category != "compiler" => return Err(diagnostic),
        Err(diagnostic) => {
            return Ok(failed(
                operation.name(),
                qrate.remap(diagnostic),
                json!({
                    "input_id":qrate.input_id(),"source_count":qrate.sources().count(),"source_id":qrate.sources().source_id(),
                    "qleisli_check":{"status":"failed","reason":"compiler_error"},"tool":tool
                }),
            ));
        }
    };
    let checked = subject.checked();
    let mut result = json!({
        "input_id":qrate.input_id(),"source_count":checked.source_count(),"source_id":qrate.sources().source_id(),
        "qleisli_check":checked.qleisli_check(),"tool":tool
    });
    if let QrateOperation::Build = operation {
        match build::publish(&subject, &tool) {
            Ok(path) => result["artifact_path"] = json!(path),
            Err(diagnostic) => {
                result["build"] = json!({"status":"failed","reason":diagnostic.id});
                return Ok(failed("build", diagnostic, result));
            }
        }
    }
    Ok(Report::ok(FORMAT, operation.name(), result))
}

pub(super) fn execute(request: &Request) -> Result<Report, Diagnostic> {
    match request {
        Request::Help => Ok(Report::ok(FORMAT, "help", json!({"help":cli::HELP}))),
        Request::Version => Ok(Report::ok(FORMAT, "version", tool_info("qargo")?)),
        Request::Qrate {
            operation,
            manifest,
        } => qrate_operation(*operation, manifest.as_deref()),
        Request::Lint {
            input,
            executable,
            deny_warnings,
        } => {
            let subject = CapturedSubject::capture(input)?;
            let orchestrator = ToolIdentity::capture("qargo")?;
            let report = tools::lint(
                subject.sources(),
                executable.as_deref(),
                *deny_warnings,
                &orchestrator,
            )?;
            Ok(subject.bind_report(report, orchestrator.to_value()))
        }
        Request::Format {
            input,
            executable,
            check,
        } => {
            let subject = CapturedSubject::capture(input)?;
            let original_root = subject.original_root()?;
            let orchestrator = ToolIdentity::capture("qargo")?;
            let plan = tools::prepare_format(
                subject.sources(),
                executable.as_deref(),
                *check,
                &orchestrator,
            )?;
            let report = plan.apply(&original_root)?;
            Ok(subject.bind_report(report, orchestrator.to_value()))
        }
        Request::Document {
            manifest,
            executable,
            include_private,
        } => {
            let qrate = manifest::capture(manifest.as_deref())?;
            let orchestrator = ToolIdentity::capture("qargo")?;
            let plan = tools::prepare_document(
                qrate.sources(),
                executable.as_deref(),
                *include_private,
                &orchestrator,
            )?;
            let relative = format!(
                "target/qlidoc/{}/{}/{}",
                qrate.input_id().trim_start_matches("sha256:"),
                plan.tool_digest().trim_start_matches("sha256:"),
                if *include_private { "all" } else { "public" },
            );
            let mut report = plan.publish(&qrate.directory().join(&relative));
            if let Some(result) = report.envelope.result.as_mut() {
                if result["artifact_path"].is_string() {
                    result["artifact_path"] = json!(relative);
                }
            }
            Ok(CapturedSubject::Qrate(qrate).bind_report(report, orchestrator.to_value()))
        }
    }
}
