//! Qrate operation boundary: parse a legal request, execute it, and report.

use std::ffi::OsString;

use crate::report::{Diagnostic, Report};

mod build;
mod cli;
mod environment;
mod manifest;
mod operations;
mod request;
mod subject;

const FORMAT: &str = "qlirgo.result";

fn error(id: &str, category: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(id, category, message)
}

/// Run a qlirgo command using arguments without the executable name.
pub fn run(args: &[OsString]) -> Report {
    let mut report = match cli::parse(args) {
        Ok(request) => operations::execute(&request)
            .unwrap_or_else(|diagnostic| Report::fail(FORMAT, request.name(), diagnostic, 1)),
        Err(diagnostic) => Report::fail(FORMAT, "usage", diagnostic, 2),
    };
    // Only qlirgo's envelope evolves; shared/standalone child schemas stay at 1.
    report.envelope.version = 2;
    report
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    #[test]
    fn qlirgo_only_finalizes_version_two_on_success_and_early_error() {
        for args in [
            vec![OsString::from("--help")],
            vec![OsString::from("--bad-option")],
        ] {
            let report = run(&args);
            assert_eq!(report.envelope.format, FORMAT);
            assert_eq!(report.envelope.version, 2);
        }
        assert_eq!(
            Report::ok("qlinty.result", "help", serde_json::json!({}))
                .envelope
                .version,
            1
        );
    }
}
