//! Syntax-only tools share bounded capture and original-source coordinates.

use std::fs;
use std::path::Path;

use qleisli::frontend::parser::ParseError;
use serde_json::{Value, json};

use crate::support::report::{Diagnostic, Location, coordinates};
use crate::support::snapshot::{Files, FrozenSources, InputDirectory, portable_relative};

pub struct SourceInput {
    pub sources: FrozenSources,
    pub original_root: InputDirectory,
    pub directory_input: bool,
}

/// Capture a selected .qli file or source directory without resolving file links.
pub fn capture_source(path: &Path) -> Result<SourceInput, Diagnostic> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        Diagnostic::error(
            "input",
            "qargo",
            format!("Cannot inspect source input: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(Diagnostic::error(
            "input",
            "qargo",
            "Source inputs cannot be symlinks.",
        ));
    }
    if metadata.is_dir() {
        let original_root = InputDirectory::open(path)?;
        let sources = FrozenSources::from_files(original_root.collect(Some("qli"))?)?;
        return Ok(SourceInput {
            sources,
            original_root,
            directory_input: true,
        });
    }
    if !metadata.is_file() || path.extension().and_then(|value| value.to_str()) != Some("qli") {
        return Err(Diagnostic::error(
            "input",
            "qargo",
            "Expected a .qli file or a source directory.",
        ));
    }
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| {
        Diagnostic::error(
            "input",
            "qargo",
            format!("Cannot resolve source parent: {error}"),
        )
    })?;
    let original_root = InputDirectory::open(&parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| Diagnostic::error("input", "qargo", "Source file has no name."))?;
    let label = portable_relative(Path::new(name))?;
    let mut files = Files::new();
    files.insert(label, original_root.read_regular(name)?);
    Ok(SourceInput {
        sources: FrozenSources::from_files(files)?,
        original_root,
        directory_input: false,
    })
}

pub fn parse_diagnostic(label: &str, source: &str, error: &ParseError) -> Diagnostic {
    let (line, column) = coordinates(source, error.span.start);
    Diagnostic {
        id: "parse".into(),
        category: "compiler".into(),
        severity: "error".into(),
        primary: Some(Box::new(Location {
            path: label.into(),
            start: error.span.start,
            end: error.span.end,
            line,
            column,
        })),
        message: error.message.clone(),
        suggestion: None,
    }
}

/// Syntax processing does not run the source/IR checker or establish evidence.
pub fn syntax_step(count: usize) -> Value {
    json!({"status":"not_run", "reason":if count == 0 { "no_sources" } else { "syntax_only" }})
}
