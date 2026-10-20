//! One guarded boundary for selection, execution, and acceptance of auxiliaries.

use std::path::Path;

use serde_json::{Value, json};

use crate::report::{Diagnostic, Report, ToolIdentity};
use crate::snapshot::{Files, FrozenSources};

mod document;
mod executable;
mod format;
mod process;
mod response;
mod selection;

pub(crate) use document::prepare as prepare_document;
pub(crate) use format::prepare as prepare_format;

/// The callback must validate response identity, semantics, and all output bytes.
/// It returns an accepted report or effect plan while cleanup remains armed.
fn execute<T>(
    name: &str,
    explicit: Option<&Path>,
    sources: &FrozenSources,
    extra: &[String],
    orchestrator: &ToolIdentity,
    validate: impl FnOnce(&[u8], i32, &str, &Path) -> Result<T, Diagnostic>,
) -> Result<T, Diagnostic> {
    execute_with_configuration(name, explicit, sources, extra, orchestrator, None, validate)
}

fn execute_with_configuration<T>(
    name: &str,
    explicit: Option<&Path>,
    sources: &FrozenSources,
    extra: &[String],
    orchestrator: &ToolIdentity,
    configuration: Option<&Files>,
    validate: impl FnOnce(&[u8], i32, &str, &Path) -> Result<T, Diagnostic>,
) -> Result<T, Diagnostic> {
    let path = selection::find_tool(name, explicit)?;
    let tool = executable::SelectedExecutable::capture(&path)?;
    let stage = if let Some(configuration) = configuration {
        sources.stage_with_configuration(configuration)?
    } else {
        sources.stage()?
    };
    let mut command = tool.command()?;
    command.arg(stage.root()).arg("--format=json").args(extra);
    let output = process::bounded_output(&mut command)?;
    tool.verify()?;
    let accepted = validate(&output.stdout, output.status, tool.digest(), stage.root())?;
    tool.verify()?;
    orchestrator.verify()?;
    output.accept();
    Ok(accepted)
}

pub(crate) fn lint(
    sources: &FrozenSources,
    configuration: &Files,
    explicit: Option<&Path>,
    deny_warnings: bool,
    orchestrator: &ToolIdentity,
) -> Result<Report, Diagnostic> {
    // Configuration failures on empty inputs precede source checking. Reject
    // them here without asking a child for an impossible count-zero failed
    // checking result under the unchanged qlinty schema.
    if sources.count() == 0 {
        crate::adapter::check(sources, configuration)?;
    }
    let args = if deny_warnings {
        vec!["--deny-warnings".into()]
    } else {
        Vec::new()
    };
    execute_with_configuration(
        "qlinty",
        explicit,
        sources,
        &args,
        orchestrator,
        Some(configuration),
        |bytes, status, digest, _stage| {
            let mut envelope =
                response::lint_response(bytes, status, sources, digest, deny_warnings)?;
            if let Some(result) = envelope.result.as_mut() {
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
        },
    )
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn invalid_empty_configuration_fails_before_child_selection() {
        let sources = FrozenSources::from_files(Files::new()).unwrap();
        let configuration = Files::from([(
            "Qlirgo.toml".to_owned(),
            b"schema-version=2\n[qrate]\nedition='2027'\n".to_vec(),
        )]);
        let orchestrator = ToolIdentity::capture("qlirgo", crate::VERSION).unwrap();
        let diagnostic = lint(
            &sources,
            &configuration,
            Some(Path::new("/nonexistent/qlinty-not-selected")),
            false,
            &orchestrator,
        )
        .err()
        .unwrap();
        assert_eq!(diagnostic.category, "compiler");
        assert_eq!(diagnostic.id, "project");
        assert!(diagnostic.message.contains("2027"));
        assert!(diagnostic.primary.is_none());
    }
}
