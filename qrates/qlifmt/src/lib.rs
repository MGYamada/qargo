//! Syntax-only Qleisli formatting with byte-exact token and comment checks.

mod apply;
mod formatter;

use std::ffi::OsString;
use std::path::PathBuf;

use qlippy_engine::report::{Diagnostic, Report, tool_info};
use qlippy_engine::snapshot::{collect_tree, digest_files};
use qlippy_engine::source::{capture_source, syntax_step};
use serde_json::json;

pub use apply::{ApplyFailure, apply_files};
pub use formatter::{changed_files, diff, format_files, validate_formatted};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const FORMAT: &str = "qlifmt.result";
const HELP: &str = "qlifmt <file-or-source-root> [--check] [--format=json]\nqlifmt --version [--format=json]\nqlifmt --help [--format=json]\n\nFormat Qleisli syntax with four spaces and a target line width of 100.\nPreserve syntax tokens and comments; no type or quantum verification is implied.\n--check prints a diff without changing source and exits 1 if changes are needed.";

struct Options {
    root: Option<PathBuf>,
    check: bool,
    help: bool,
    version: bool,
}

fn usage(message: &str) -> Report {
    Report::fail(
        FORMAT,
        "fmt",
        Diagnostic::error("invalid_arguments", "usage", message),
        2,
    )
}

fn options(args: &[OsString]) -> Result<Options, Box<Report>> {
    let mut options = Options {
        root: None,
        check: false,
        help: false,
        version: false,
    };
    let mut json_seen = false;
    for arg in args {
        if arg.to_str().is_none() {
            return Err(Box::new(usage("Arguments must be UTF-8.")));
        }
        match arg.to_str() {
            Some("--format=json") if !json_seen => json_seen = true,
            Some("--check") if !options.check => options.check = true,
            Some("--help") if !options.help => options.help = true,
            Some("--version") if !options.version => options.version = true,
            _ if arg.to_string_lossy().starts_with('-') => {
                return Err(Box::new(usage("Unknown, repeated, or malformed option.")));
            }
            _ if arg.is_empty() => {
                return Err(Box::new(usage("The input path must not be empty.")));
            }
            _ if options.root.is_some() => {
                return Err(Box::new(usage("Only one input path may be supplied.")));
            }
            _ => options.root = Some(PathBuf::from(arg)),
        }
    }
    if options.help || options.version {
        if options.help && options.version || options.root.is_some() || options.check {
            return Err(Box::new(usage(
                "Help and version cannot be combined with formatting arguments.",
            )));
        }
    } else if options.root.is_none() {
        return Err(Box::new(usage(
            "An input path is required. Use --help for usage.",
        )));
    }
    Ok(options)
}

/// Run the CLI against one immutable capture, retaining its binding on failure.
pub fn run(args: &[OsString]) -> Report {
    let options = match options(args) {
        Ok(options) => options,
        Err(report) => return *report,
    };
    if options.help {
        return Report::ok(FORMAT, "help", json!({"help": HELP}));
    }
    let tool = match tool_info("qlifmt") {
        Ok(tool) => tool,
        Err(error) => return Report::fail(FORMAT, "fmt", error, 1),
    };
    if options.version {
        return Report::ok(FORMAT, "version", json!({"tool": tool}));
    }
    let Some(root) = options.root else {
        return usage("An input path is required.");
    };
    let input = match capture_source(&root) {
        Ok(input) => input,
        Err(error) => return Report::fail(FORMAT, "fmt", error, 1),
    };
    let directory_input = root.is_dir();
    let sources = &input.sources;
    let result = json!({
        "source_count": sources.count(),
        "source_id": sources.source_id,
        "qleisli_check": syntax_step(sources.count()),
        "tool": tool,
        "formatted_source_id": null,
        "changed_files": [],
        "updated_files": [],
        "check": options.check,
        "diff": "",
    });
    let mut report = Report::ok(FORMAT, "fmt", result);
    let formatted = match format_files(&sources.files) {
        Ok(files) => files,
        Err(error) => return bound_failure(report, error),
    };
    let changed = changed_files(&sources.files, &formatted);
    let result = report.envelope.result.as_mut().expect("bound result");
    result["formatted_source_id"] = json!(digest_files("qleisli.source.v1", &formatted));
    result["changed_files"] = json!(changed);
    if options.check && !changed.is_empty() {
        result["diff"] = json!(diff(&sources.files, &formatted));
        return bound_failure(
            report,
            Diagnostic::error(
                "formatting_required",
                "qargo",
                "Source formatting is required; rerun without --check to apply changes.",
            ),
        );
    }
    if !options.check {
        if directory_input {
            match collect_tree(&root, Some("qli")) {
                Ok(current) if current == sources.files => {}
                Ok(_) => {
                    return bound_failure(
                        report,
                        Diagnostic::error(
                            "source_changed",
                            "qargo",
                            "Source file set or bytes changed after capture.",
                        ),
                    );
                }
                Err(error) => return bound_failure(report, error),
            }
        }
        match apply_files(&input.original_root, &sources.files, &formatted) {
            Ok(updated) => result["updated_files"] = json!(updated),
            Err(failure) => {
                result["updated_files"] = json!(failure.updated_files);
                return bound_failure(report, failure.diagnostic);
            }
        }
    }
    report
}

fn bound_failure(mut report: Report, diagnostic: Diagnostic) -> Report {
    report.envelope.outcome = "error".into();
    report.envelope.diagnostics.push(diagnostic);
    report.exit_code = 1;
    report
}

fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("format_validation", "qargo", message)
}

fn source_text<'a>(label: &str, bytes: &'a [u8]) -> Result<&'a str, Diagnostic> {
    std::str::from_utf8(bytes).map_err(|_| error(format!("Source is not UTF-8: {label}")))
}
