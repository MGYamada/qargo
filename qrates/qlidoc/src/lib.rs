//! Syntax-only Qleisli documentation. Generated pages are not verification evidence.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use qleisli::frontend::ast::{Decl, FnBody};
use qleisli::frontend::documentation::DocComment;
use qleisli::frontend::lexer::{Token, TokenKind, lex};
use qleisli::frontend::parser::parse_documented_module;
use qlippy_engine::report::{Diagnostic, Report, path_argument, tool_info};
use qlippy_engine::snapshot::{Files, portable_relative};
use qlippy_engine::source::{capture_source, parse_diagnostic, syntax_step};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const FORMAT: &str = "qlidoc.result";
const HELP: &str = "qlidoc <file-or-source-root> [--output=PATH] [--document-private-items] [--format=json]\nqlidoc --help\nqlidoc --version\n--output also accepts --output PATH.\nGenerate syntax-only Markdown; documentation examples are never executed.";
const DISCLAIMER: &str = "Source documentation only; no type, ownership or contract verification is implied. Examples are not executed.\n\n";

fn output_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("doc_output", "tool", message)
}

fn usage(message: impl Into<String>) -> Report {
    Report::fail(FORMAT, "doc", usage_error(message), 2)
}

fn usage_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("usage", "usage", message)
}

struct Options {
    input: PathBuf,
    output: Option<PathBuf>,
    include_private: bool,
}

fn options(args: &[OsString]) -> Result<Options, Diagnostic> {
    let mut input = None;
    let mut output = None;
    let mut include_private = false;
    let mut json_mode = false;
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        if arg.to_str().is_none() {
            return Err(usage_error("Arguments must be UTF-8."));
        }
        if arg.is_empty() {
            return Err(usage_error("The input path must not be empty."));
        }
        if arg == "--format=json" {
            if json_mode {
                return Err(usage_error("--format=json may only be specified once."));
            }
            json_mode = true;
        } else if arg == "--document-private-items" {
            if include_private {
                return Err(usage_error(
                    "--document-private-items may only be specified once.",
                ));
            }
            include_private = true;
        } else if arg == "--output"
            || arg
                .to_str()
                .is_some_and(|value| value.starts_with("--output="))
        {
            if output.is_some() {
                return Err(usage_error("--output requires one nonempty path."));
            }
            output = Some(
                path_argument(
                    "--output",
                    arg.to_str().expect("UTF-8 argument"),
                    &mut remaining,
                )
                .map_err(usage_error)?,
            );
        } else if arg.to_str().is_some_and(|s| s.starts_with('-')) {
            return Err(usage_error("Unknown option; see qlidoc --help."));
        } else if input.is_some() {
            return Err(usage_error(
                "Specify exactly one source file or source root.",
            ));
        } else {
            input = Some(PathBuf::from(arg));
        }
    }
    Ok(Options {
        input: input.ok_or_else(|| usage_error("A source file or source root is required."))?,
        output,
        include_private,
    })
}

/// Execute the CLI against captured source bytes; no Qleisli checker or host backend runs.
pub fn run(args: &[OsString]) -> Report {
    let special: Vec<_> = args.iter().filter(|arg| *arg != "--format=json").collect();
    if special.len() == 1 && (special[0] == "--help" || special[0] == "--version") {
        if args.iter().filter(|arg| *arg == "--format=json").count() > 1 {
            return usage("--format=json may only be specified once.");
        }
        if special[0] == "--help" {
            return Report::ok(FORMAT, "help", json!({"help":HELP}));
        }
        return match tool_info("qlidoc") {
            Ok(tool) => Report::ok(FORMAT, "version", tool),
            Err(error) => Report::fail(FORMAT, "version", error, 1),
        };
    }
    let opts = match options(args) {
        Ok(opts) => opts,
        Err(error) => return Report::fail(FORMAT, "doc", error, 2),
    };
    let input = match capture_source(&opts.input) {
        Ok(input) => input,
        Err(error) => return Report::fail(FORMAT, "doc", error, 1),
    };
    let tool = match tool_info("qlidoc") {
        Ok(tool) => tool,
        Err(error) => return Report::fail(FORMAT, "doc", error, 1),
    };
    let mut result = json!({
        "source_count":input.sources.count(),
        "source_id":input.sources.source_id,
        "qleisli_check":syntax_step(input.sources.count()),
        "tool":tool,
        "document_private_items":opts.include_private,
        "artifact_path":null,
        "files":[],
    });
    let rendered = match render_files(&input.sources.files, opts.include_private) {
        Ok(files) => files,
        Err(error) => return bound_failure(error, result),
    };
    let output = opts.output.unwrap_or_else(|| {
        PathBuf::from("target")
            .join("qlidoc")
            .join(input.sources.source_id.trim_start_matches("sha256:"))
            .join(
                tool["executable_sha256"]
                    .as_str()
                    .unwrap_or("")
                    .trim_start_matches("sha256:"),
            )
            .join(if opts.include_private {
                "all"
            } else {
                "public"
            })
    });
    result["artifact_path"] = match display_path(&output) {
        Ok(label) => json!(label),
        Err(error) => return bound_failure(error, result),
    };
    result["files"] = json!(
        rendered
            .iter()
            .map(|(path, bytes)| {
                json!({"path":path,"sha256":format!("sha256:{:x}",Sha256::digest(bytes))})
            })
            .collect::<Vec<_>>()
    );
    if let Err(error) = reject_input_overlap(&opts.input, &output) {
        return bound_failure(error, result);
    }
    match publish(&output, &rendered) {
        Ok(()) => Report::ok(FORMAT, "doc", result),
        Err(error) => bound_failure(error, result),
    }
}

fn bound_failure(error: Diagnostic, result: Value) -> Report {
    let mut report = Report::fail(FORMAT, "doc", error, 1);
    report.envelope.result = Some(result);
    report
}

fn markdown_text(text: &str) -> String {
    let mut output = String::new();
    for ch in text.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '<' | '>' | '&' | '|' => {
                output.push_str(&format!("&#{};", ch as u32));
            }
            '\r' | '\n' => output.push(' '),
            _ => output.push(ch),
        }
    }
    output
}

fn link_path(path: &str) -> String {
    let mut output = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn render_comments(output: &mut String, comments: &[DocComment]) {
    for comment in comments {
        for line in comment.text.split('\n') {
            output.push_str(line.strip_prefix(' ').unwrap_or(line));
            output.push('\n');
        }
    }
    if !comments.is_empty() {
        output.push('\n');
    }
}

fn render_signature(output: &mut String, source: &str, decl: &Decl, tokens: &[Token]) {
    let end = match &decl.body {
        FnBody::Meaning { .. } => decl.span.end,
        FnBody::Quantum(block) => block.span.start,
        FnBody::Basis(_) => tokens
            .iter()
            .find(|token| {
                token.span.start >= decl.return_type.span.end
                    && token.span.start < decl.span.end
                    && token.kind == TokenKind::LBrace
            })
            .map_or(decl.return_type.span.end, |token| token.span.start),
    };
    let signature = source[decl.span.start..end]
        .trim_end()
        .replace("\r\n", "\n");
    let longest = signature
        .split(|ch| ch != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(3.max(longest + 1));
    output.push_str(&format!("{fence}qli\n{signature}\n{fence}\n\n"));
}

/// Render deterministic Markdown from source syntax, including an index for empty inputs.
pub fn render_files(files: &Files, include_private: bool) -> Result<Files, Diagnostic> {
    let mut pages = Files::new();
    let mut index = format!("# Qleisli documentation\n\n{DISCLAIMER}");
    if files.is_empty() {
        index.push_str("No source modules.\n");
    } else {
        index.push_str("## Modules\n\n");
    }
    for (label, bytes) in files {
        if portable_relative(Path::new(label))? != *label || !label.ends_with(".qli") {
            return Err(output_error(
                "Documentation inputs must have canonical relative .qli paths.",
            ));
        }
        let source = std::str::from_utf8(bytes).map_err(|_| {
            Diagnostic::error(
                "source_encoding",
                "compiler",
                "Qleisli source must be UTF-8.",
            )
        })?;
        let documented = parse_documented_module(source)
            .map_err(|error| parse_diagnostic(label, source, &error))?;
        let tokens = lex(source).map_err(|error| parse_diagnostic(label, source, &error.into()))?;
        let stem = label.strip_suffix(".qli").unwrap_or(label);
        let module = stem.replace('/', "::");
        let path = format!("modules/{stem}.md");
        index.push_str(&format!(
            "- [{}]({})\n",
            markdown_text(&module),
            link_path(&path)
        ));
        let mut page = format!("# Module {}\n\n{DISCLAIMER}", markdown_text(&module));
        render_comments(&mut page, &documented.module_docs);
        let mut visible = 0;
        for (decl, docs) in documented
            .syntax
            .decls
            .iter()
            .zip(&documented.declaration_docs)
        {
            if !decl.public && !include_private {
                continue;
            }
            visible += 1;
            page.push_str(&format!(
                "## {} ({})\n\n",
                markdown_text(&decl.name.text),
                if decl.public { "public" } else { "private" }
            ));
            render_signature(&mut page, source, decl, &tokens);
            render_comments(&mut page, docs);
        }
        if visible == 0 {
            page.push_str("No visible declarations.\n");
        }
        pages.insert(path, page.into_bytes());
    }
    pages.insert("index.md".into(), index.into_bytes());
    Ok(pages)
}

fn absolute_normalized(path: &Path) -> Result<PathBuf, Diagnostic> {
    qlippy_engine::publication::absolute_normalized(path)
        .map_err(|error| output_error(error.to_string()))
}

fn display_path(path: &Path) -> Result<String, Diagnostic> {
    let path = absolute_normalized(path)?;
    let cwd = std::env::current_dir()
        .map_err(|error| output_error(format!("Cannot locate working directory: {error}")))?;
    if let Ok(relative) = path.strip_prefix(&cwd) {
        if !relative.as_os_str().is_empty() {
            return portable_relative(relative);
        }
    }
    let label = path
        .to_str()
        .ok_or_else(|| output_error("Output path must be UTF-8."))?;
    if label.contains('\\') {
        return Err(output_error("Output paths cannot contain backslashes."));
    }
    Ok(label.to_owned())
}

/// Resolve existing aliases before comparing an output with selected input paths.
fn resolved_output(path: &Path) -> Result<PathBuf, Diagnostic> {
    let absolute = absolute_normalized(path)?;
    let mut existing = absolute.as_path();
    let mut tail = Vec::new();
    while !existing.exists() {
        if let Ok(metadata) = fs::symlink_metadata(existing) {
            if metadata.file_type().is_symlink() {
                return Err(output_error(
                    "Documentation output cannot traverse symlinks.",
                ));
            }
        }
        tail.push(
            existing
                .file_name()
                .ok_or_else(|| output_error("Invalid output path."))?,
        );
        existing = existing
            .parent()
            .ok_or_else(|| output_error("Invalid output path."))?;
    }
    let mut resolved = fs::canonicalize(existing)
        .map_err(|error| output_error(format!("Cannot resolve output ancestor: {error}")))?;
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

fn reject_input_overlap(input: &Path, output: &Path) -> Result<(), Diagnostic> {
    let selected = fs::canonicalize(input)
        .map_err(|error| output_error(format!("Cannot resolve selected source: {error}")))?;
    let output = resolved_output(output)?;
    let is_directory = fs::metadata(&selected)
        .map_err(|error| output_error(format!("Cannot inspect selected source: {error}")))?
        .is_dir();
    if selected.starts_with(&output) || (is_directory && output.starts_with(&selected)) {
        return Err(output_error(
            "Documentation output must not overlap the selected source input.",
        ));
    }
    Ok(())
}

/// Atomically publish a complete output directory, reusing only byte-identical artifacts.
pub fn publish(output: &Path, files: &Files) -> Result<(), Diagnostic> {
    qlippy_engine::publication::publish(output, files, &std::collections::BTreeSet::new()).map_err(
        |error| match error {
            qlippy_engine::publication::PublicationError::Mismatch => Diagnostic::error(
                "artifact_mismatch",
                "tool",
                "Existing documentation artifacts are inconsistent; nothing was overwritten.",
            ),
            error => output_error(error.to_string()),
        },
    )
}
