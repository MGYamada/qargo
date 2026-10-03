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
    qrate_operation_with_verifier(operation, manifest, ToolIdentity::verify)
}

fn qrate_operation_with_verifier(
    operation: QrateOperation,
    manifest: Option<&Path>,
    verify: impl FnOnce(&ToolIdentity) -> Result<(), Diagnostic>,
) -> Result<Report, Diagnostic> {
    let qrate = manifest::capture(manifest)?;
    let orchestrator = ToolIdentity::capture("qargo", crate::VERSION)?;
    let tool = orchestrator.to_value();
    if let QrateOperation::Test = operation {
        verify(&orchestrator)?;
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
            verify(&orchestrator)?;
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
        match build::publish(&subject, &tool, || verify(&orchestrator)) {
            Ok(path) => result["artifact_path"] = json!(path),
            Err(diagnostic) => {
                result["build"] = json!({"status":"failed","reason":diagnostic.id});
                return Ok(failed("build", diagnostic, result));
            }
        }
    } else {
        verify(&orchestrator)?;
    }
    Ok(Report::ok(FORMAT, operation.name(), result))
}

pub(super) fn execute(request: &Request) -> Result<Report, Diagnostic> {
    match request {
        Request::Help => Ok(Report::ok(FORMAT, "help", json!({"help":cli::HELP}))),
        Request::Version => Ok(Report::ok(
            FORMAT,
            "version",
            tool_info("qargo", crate::VERSION)?,
        )),
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
            let orchestrator = ToolIdentity::capture("qargo", crate::VERSION)?;
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
            let original_root = subject.original_root();
            let orchestrator = ToolIdentity::capture("qargo", crate::VERSION)?;
            let plan = tools::prepare_format(
                subject.sources(),
                executable.as_deref(),
                *check,
                &orchestrator,
            )?;
            let report = plan.apply(original_root)?;
            Ok(subject.bind_report(report, orchestrator.to_value()))
        }
        Request::Document {
            manifest,
            executable,
            include_private,
        } => {
            let qrate = manifest::capture(manifest.as_deref())?;
            let orchestrator = ToolIdentity::capture("qargo", crate::VERSION)?;
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
            let report = plan.publish(qrate.held_directory(), Path::new(&relative));
            Ok(CapturedSubject::Qrate(qrate).bind_report(report, orchestrator.to_value()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn check_and_build_revalidate_the_captured_host_before_acceptance() {
        for source in ["", "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}"] {
            for operation in [QrateOperation::Check, QrateOperation::Build] {
                let root = tempfile::tempdir().unwrap();
                for directory in ["src", "tests", "docs"] {
                    fs::create_dir(root.path().join(directory)).unwrap();
                }
                let manifest = root.path().join("Qargo.toml");
                fs::write(&manifest, "schema-version=2\n[qrate]\nname=\"host\"\nversion=\"0.1.0\"\nedition=\"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
                if !source.is_empty() {
                    fs::write(root.path().join("src/module.qli"), source).unwrap();
                }
                let mut rejected_identity = None;
                let rejected = qrate_operation_with_verifier(operation, Some(&manifest), |host| {
                    rejected_identity = Some(host.to_value());
                    assert!(!root.path().join("target").exists());
                    Err(Diagnostic::error(
                        "tool_identity",
                        "tool",
                        "Injected host identity rejection.",
                    ))
                });
                let diagnostic = match rejected {
                    Err(diagnostic) => diagnostic,
                    Ok(report) => {
                        assert_eq!(report.exit_code, 1);
                        assert!(
                            report.envelope.result.as_ref().unwrap()["artifact_path"].is_null()
                        );
                        report.envelope.diagnostics.into_iter().next().unwrap()
                    }
                };
                assert_eq!(diagnostic.id, "tool_identity");
                assert!(rejected_identity.is_some());
                assert!(!root.path().join("target").exists());

                let mut accepted_identity = None;
                let accepted = qrate_operation_with_verifier(operation, Some(&manifest), |host| {
                    host.verify()?;
                    accepted_identity = Some(host.to_value());
                    Ok(())
                })
                .unwrap();
                assert_eq!(accepted.exit_code, 0);
                let result = accepted.envelope.result.unwrap();
                let identity = accepted_identity.unwrap();
                assert_eq!(result["tool"], identity);
                if let QrateOperation::Build = operation {
                    let path = result["artifact_path"].as_str().unwrap();
                    assert!(
                        path.ends_with(
                            identity["executable_sha256"]
                                .as_str()
                                .unwrap()
                                .trim_start_matches("sha256:")
                        )
                    );
                    let record: Value = serde_json::from_slice(
                        &fs::read(root.path().join(path).join("build-record.json")).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(record["tool"], identity);
                }
            }
        }
    }
}
