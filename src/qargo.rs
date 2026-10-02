//! Qrate operation boundary: parse a legal request, execute it, and report.

use std::ffi::OsString;

use crate::report::{Diagnostic, Report};

mod build;
mod cli;
mod manifest;
mod operations;
mod request;
mod subject;

const FORMAT: &str = "qargo.result";

fn error(id: &str, category: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(id, category, message)
}

/// Run a qargo command using arguments without the executable name.
pub fn run(args: &[OsString]) -> Report {
    let request = match cli::parse(args) {
        Ok(request) => request,
        Err(diagnostic) => return Report::fail(FORMAT, "usage", diagnostic, 2),
    };
    operations::execute(&request)
        .unwrap_or_else(|diagnostic| Report::fail(FORMAT, request.name(), diagnostic, 1))
}
