//! Versioned diagnostic transport; human text never acts as acceptance evidence.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub id: String,
    pub category: String,
    pub severity: String,
    pub primary: Option<Box<Location>>,
    pub message: String,
    pub suggestion: Option<Box<str>>,
}

impl Diagnostic {
    pub fn error(
        id: impl Into<String>,
        category: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category: category.into(),
            severity: "error".into(),
            primary: None,
            message: message.into(),
            suggestion: None,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.id, self.message)
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub format: String,
    pub version: u32,
    pub command: String,
    pub outcome: String,
    pub diagnostics: Vec<Diagnostic>,
    pub result: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub envelope: Envelope,
    pub exit_code: u8,
}

impl Report {
    pub fn ok(format: &str, command: &str, result: Value) -> Self {
        Self {
            envelope: Envelope {
                format: format.into(),
                version: 1,
                command: command.into(),
                outcome: "ok".into(),
                diagnostics: Vec::new(),
                result: Some(result),
            },
            exit_code: 0,
        }
    }

    pub fn fail(format: &str, command: &str, diagnostic: Diagnostic, exit_code: u8) -> Self {
        Self {
            envelope: Envelope {
                format: format.into(),
                version: 1,
                command: command.into(),
                outcome: "error".into(),
                diagnostics: vec![diagnostic],
                result: None,
            },
            exit_code,
        }
    }
}

pub fn json_requested(args: &[OsString]) -> bool {
    args.iter().any(|arg| arg == "--format=json")
}

/// Read equality or space-separated path options, rejecting option-shaped values.
pub fn path_argument(
    option: &str,
    argument: &str,
    remaining: &mut std::slice::Iter<'_, OsString>,
) -> Result<PathBuf, String> {
    let value = if argument == option {
        remaining
            .next()
            .and_then(|value| value.to_str())
            .filter(|value| !value.starts_with('-'))
    } else {
        argument
            .strip_prefix(option)
            .and_then(|value| value.strip_prefix('='))
    };
    match value.filter(|value| !value.is_empty()) {
        Some(value) => Ok(PathBuf::from(value)),
        None => Err(format!(
            "{option} requires a nonempty UTF-8 path. Use {option}=PATH or {option} PATH."
        )),
    }
}

/// Bind metadata to the executable actually running, not to a path or timestamp.
pub fn tool_info(name: &str) -> Result<Value, Diagnostic> {
    Ok(json!({
        "name": name,
        "version": crate::VERSION,
        "executable_sha256": crate::executable::running_digest()?,
        "qleisli_version": crate::QLEISLI_VERSION,
        "profile": crate::PROFILE,
    }))
}

pub fn coordinates(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    let mut previous_cr = false;
    for ch in source.get(..offset).unwrap_or("").chars() {
        match ch {
            '\r' => {
                line += 1;
                column = 1;
            }
            '\n' => {
                if !previous_cr {
                    line += 1;
                }
                column = 1;
            }
            _ => column += 1,
        }
        previous_cr = ch == '\r';
    }
    (line, column)
}

pub fn emit(report: Report, json_mode: bool) -> ExitCode {
    let result = if json_mode {
        let mut output = io::stdout().lock();
        serde_json::to_writer(&mut output, &report.envelope)
            .map_err(io::Error::other)
            .and_then(|()| output.write_all(b"\n"))
    } else {
        emit_human(&report)
    };
    match result {
        Ok(()) => ExitCode::from(report.exit_code),
        Err(error) => {
            eprintln!("Could not write result: {error}");
            ExitCode::FAILURE
        }
    }
}

fn emit_human(report: &Report) -> io::Result<()> {
    let mut errors = io::stderr().lock();
    for diagnostic in &report.envelope.diagnostics {
        if let Some(location) = &diagnostic.primary {
            write!(
                errors,
                "{}:{}:{}: ",
                location.path, location.line, location.column
            )?;
        }
        writeln!(
            errors,
            "{}[{}]: {}",
            diagnostic.severity, diagnostic.id, diagnostic.message
        )?;
        if let Some(suggestion) = &diagnostic.suggestion {
            writeln!(errors, "  suggestion: {suggestion}")?;
        }
    }
    if let Some(result) = &report.envelope.result {
        let mut output = io::stdout().lock();
        if let Some(help) = result.get("help").and_then(Value::as_str) {
            writeln!(output, "{help}")?;
        } else if report.envelope.command == "version" {
            writeln!(
                output,
                "{} {} (Qleisli {}, {})",
                report.envelope.format.trim_end_matches(".result"),
                crate::VERSION,
                crate::QLEISLI_VERSION,
                crate::PROFILE
            )?;
        } else if report.envelope.command == "list-rules" {
            writeln!(
                output,
                "qlippy rule catalog {}",
                crate::rules::CATALOG_VERSION
            )?;
            for rule in crate::rules::RULES {
                let metadata = serde_json::to_value(rule).map_err(io::Error::other)?;
                writeln!(
                    output,
                    "{} [{}; {}]: {}",
                    rule.id,
                    metadata["group"].as_str().unwrap_or(""),
                    metadata["promotion"].as_str().unwrap_or(""),
                    rule.description
                )?;
            }
        } else {
            writeln!(
                output,
                "{} {}: {}",
                report.envelope.format.trim_end_matches(".result"),
                report.envelope.command,
                report.envelope.outcome
            )?;
            if let Some(count) = result.get("source_count").and_then(Value::as_u64) {
                writeln!(output, "  source_count: {count}")?;
            }
            if let Some(step) = result.get("qleisli_check") {
                if step.get("status").and_then(Value::as_str) == Some("not_run") {
                    let reason = step
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("unspecified");
                    writeln!(output, "  Qleisli check: not run ({reason})")?;
                } else if step.get("status").and_then(Value::as_str) == Some("passed") {
                    writeln!(output, "  Qleisli source/IR check: passed")?;
                }
            }
            if let Some(path) = result.get("artifact_path").and_then(Value::as_str) {
                writeln!(output, "  artifacts: {path}")?;
            }
            if let Some(diff) = result
                .get("diff")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                write!(output, "{diff}")?;
            }
            if let Some(updated) = result.get("updated_files").and_then(Value::as_array) {
                for path in updated.iter().filter_map(Value::as_str) {
                    writeln!(output, "  formatted: {path}")?;
                }
            }
        }
    }
    Ok(())
}
