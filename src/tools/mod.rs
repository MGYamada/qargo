//! One guarded boundary for selection, execution, and acceptance of auxiliaries.

use std::path::Path;

use serde_json::{Value, json};

use crate::report::{Diagnostic, Report, ToolIdentity};
use crate::snapshot::FrozenSources;

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
    let path = selection::find_tool(name, explicit)?;
    let tool = executable::SelectedExecutable::capture(&path)?;
    let stage = sources.stage()?;
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
    explicit: Option<&Path>,
    deny_warnings: bool,
    orchestrator: &ToolIdentity,
) -> Result<Report, Diagnostic> {
    let args = if deny_warnings {
        vec!["--deny-warnings".into()]
    } else {
        Vec::new()
    };
    execute(
        "qlippy",
        explicit,
        sources,
        &args,
        orchestrator,
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
