//! Whitespace layout over compiler tokens; comments remain byte-exact pieces.

use std::path::Path;

use qleisli::frontend::documentation::{DocComment, DocumentedModule};
use qleisli::frontend::lexer::{TokenKind, lex};
use qleisli::frontend::parser::parse_documented_module;
use qlippy_engine::report::Diagnostic;
use qlippy_engine::snapshot::{FILE_BYTES, Files, TOTAL_BYTES, portable_relative};
use qlippy_engine::source::parse_diagnostic;

use super::{error, source_text};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Token(TokenKind, String),
    Comment(String),
}

fn pieces(source: &str) -> Result<Vec<Piece>, Diagnostic> {
    let tokens = lex(source).map_err(|failure| error(failure.to_string()))?;
    let mut result = Vec::new();
    let mut cursor = 0;
    for token in tokens {
        let gap = &source[cursor..token.span.start];
        let mut position = 0;
        while position < gap.len() {
            if gap[position..].starts_with("//") {
                let end = gap[position..]
                    .find('\n')
                    .map_or(gap.len(), |offset| position + offset);
                let end = if gap[..end].ends_with('\r') {
                    end - 1
                } else {
                    end
                };
                result.push(Piece::Comment(gap[position..end].into()));
                position = end;
            } else if gap[position..].starts_with("/*") {
                let start = position;
                position += 2;
                let mut depth = 1;
                while depth > 0 {
                    if gap[position..].starts_with("/*") {
                        depth += 1;
                        position += 2;
                    } else if gap[position..].starts_with("*/") {
                        depth -= 1;
                        position += 2;
                    } else {
                        position += gap[position..]
                            .chars()
                            .next()
                            .expect("parsed comment")
                            .len_utf8();
                    }
                }
                result.push(Piece::Comment(gap[start..position].into()));
            } else {
                position += gap[position..]
                    .chars()
                    .next()
                    .expect("gap character")
                    .len_utf8();
            }
        }
        if token.kind != TokenKind::Eof {
            result.push(Piece::Token(
                token.kind,
                source[token.span.start..token.span.end].into(),
            ));
        }
        cursor = token.span.end;
    }
    Ok(result)
}

struct Writer<'a> {
    output: String,
    newline: &'a str,
    column: usize,
    line_start: bool,
}

impl<'a> Writer<'a> {
    fn new(newline: &'a str) -> Self {
        Self {
            output: String::new(),
            newline,
            column: 0,
            line_start: true,
        }
    }

    fn newline(&mut self) {
        if !self.line_start {
            self.output.push_str(self.newline);
            self.column = 0;
            self.line_start = true;
        }
    }

    fn write(&mut self, text: &str, indent: usize, space: bool) {
        if self.line_start {
            self.output.push_str(&"    ".repeat(indent));
            self.column = indent * 4;
            self.line_start = false;
        } else if space {
            self.output.push(' ');
            self.column += 1;
        }
        self.output.push_str(text);
        if let Some((_, last)) = text.rsplit_once('\n') {
            self.column = last.chars().count();
            self.line_start = last.is_empty();
        } else {
            self.column += text.chars().count();
        }
    }
}

fn space_before(previous: Option<&TokenKind>, current: &TokenKind) -> bool {
    use TokenKind::*;
    if matches!(
        current,
        RParen | RBracket | RAngle | Comma | Colon | DoubleColon | Semicolon | LAngle | LBracket
    ) {
        return false;
    }
    let Some(previous) = previous else {
        return false;
    };
    if matches!(current, LParen) {
        return matches!(previous, If | Qif);
    }
    !matches!(previous, LParen | LBracket | LAngle | DoubleColon)
}

fn format_source(source: &str) -> Result<Vec<u8>, Diagnostic> {
    let newline = if source
        .find('\n')
        .is_some_and(|index| index > 0 && source.as_bytes()[index - 1] == b'\r')
    {
        "\r\n"
    } else {
        "\n"
    };
    let mut writer = Writer::new(newline);
    let mut blocks = 0usize;
    let mut delimiters = 0usize;
    let mut previous = None;
    let source_pieces = pieces(source)?;
    // Include adjacent punctuation in the width of each breakable token group.
    let mut widths = vec![0usize; source_pieces.len()];
    for index in (0..source_pieces.len()).rev() {
        if let Piece::Token(kind, spelling) = &source_pieces[index] {
            widths[index] = spelling.chars().count();
            if let Some(Piece::Token(next, _)) = source_pieces.get(index + 1) {
                if !space_before(Some(kind), next)
                    && !matches!(
                        kind,
                        TokenKind::LBrace | TokenKind::RBrace | TokenKind::Semicolon
                    )
                    && *next != TokenKind::RBrace
                {
                    widths[index] += widths[index + 1];
                }
            }
        }
    }
    for (index, piece) in source_pieces.into_iter().enumerate() {
        match piece {
            Piece::Comment(comment) => {
                writer.newline();
                writer.write(&comment, blocks, false);
                writer.newline();
            }
            Piece::Token(kind, spelling) => {
                use TokenKind::*;
                if kind == RBrace {
                    blocks = blocks.saturating_sub(1);
                    writer.newline();
                } else if previous == Some(RBrace)
                    && !matches!(kind, Else | Comma | Semicolon | RParen | RBracket | RAngle)
                {
                    writer.newline();
                }
                if matches!(kind, RParen | RBracket | RAngle) {
                    delimiters = delimiters.saturating_sub(1);
                }
                let space = space_before(previous.as_ref(), &kind);
                let breakable = space || matches!(previous, Some(LParen | LBracket));
                let wrap = !writer.line_start
                    && breakable
                    && writer.column + usize::from(space) + widths[index] > 100;
                if wrap {
                    writer.newline();
                }
                let indent = blocks
                    + usize::from(wrap || writer.line_start && delimiters > 0 && kind != RBrace);
                writer.write(&spelling, indent, space);
                if matches!(kind, LParen | LBracket | LAngle) {
                    delimiters += 1;
                }
                if kind == LBrace {
                    blocks += 1;
                    writer.newline();
                } else if kind == Semicolon {
                    writer.newline();
                }
                previous = Some(kind);
            }
        }
    }
    writer.newline();
    Ok(writer.output.into_bytes())
}

fn validate_file_set(files: &Files) -> Result<(), Diagnostic> {
    let mut total = 0usize;
    if files.len() > 4096 {
        return Err(error("Too many source files."));
    }
    for (label, bytes) in files {
        if !label.ends_with(".qli") || portable_relative(Path::new(label))? != *label {
            return Err(error("Formatter input contains a noncanonical .qli path."));
        }
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| error("Source byte accounting overflow."))?;
        if bytes.len() as u64 > FILE_BYTES || total > TOTAL_BYTES {
            return Err(error("Formatter input exceeds source byte limits."));
        }
    }
    Ok(())
}

/// Parse every input and produce candidates before permitting any source write.
pub fn format_files(original: &Files) -> Result<Files, Diagnostic> {
    validate_file_set(original)?;
    let mut formatted = Files::new();
    for (label, bytes) in original {
        let source = source_text(label, bytes)?;
        parse_documented_module(source)
            .map_err(|failure| parse_diagnostic(label, source, &failure))?;
        formatted.insert(label.clone(), format_source(source)?);
    }
    validate_formatted(original, &formatted)?;
    Ok(formatted)
}

fn docs(comments: &[DocComment]) -> Vec<(qleisli::frontend::documentation::DocStyle, &str)> {
    comments
        .iter()
        .map(|comment| (comment.style, comment.text.as_str()))
        .collect()
}

fn same_documentation(original: &DocumentedModule, formatted: &DocumentedModule) -> bool {
    docs(&original.module_docs) == docs(&formatted.module_docs)
        && original.declaration_docs.len() == formatted.declaration_docs.len()
        && original.import_docs.len() == formatted.import_docs.len()
        && original
            .declaration_docs
            .iter()
            .zip(&formatted.declaration_docs)
            .all(|(a, b)| docs(a) == docs(b))
        && original
            .import_docs
            .iter()
            .zip(&formatted.import_docs)
            .all(|(a, b)| docs(a) == docs(b))
}

/// Reject filename, syntax, spelling, comment or documentation attachment changes.
pub fn validate_formatted(original: &Files, formatted: &Files) -> Result<(), Diagnostic> {
    validate_file_set(original)?;
    validate_file_set(formatted)?;
    if !original.keys().eq(formatted.keys()) {
        return Err(error("Formatter output changed the source file set."));
    }
    for (label, bytes) in original {
        let before = source_text(label, bytes)?;
        let after = source_text(label, &formatted[label])?;
        let original_ast = parse_documented_module(before)
            .map_err(|failure| parse_diagnostic(label, before, &failure))?;
        let formatted_ast = parse_documented_module(after)
            .map_err(|_| error(format!("Formatter output does not parse: {label}")))?;
        if pieces(before)? != pieces(after)? || !same_documentation(&original_ast, &formatted_ast) {
            return Err(error(format!(
                "Formatter output changed tokens, comments or documentation attachment: {label}"
            )));
        }
    }
    Ok(())
}

pub fn changed_files(original: &Files, formatted: &Files) -> Vec<String> {
    original
        .iter()
        .filter(|(label, bytes)| formatted.get(*label) != Some(*bytes))
        .map(|(label, _)| label.clone())
        .collect()
}

/// Linear-time line diff bounded to 64 KiB; common prefix/suffix are omitted.
pub fn diff(original: &Files, formatted: &Files) -> String {
    const LIMIT: usize = 64 * 1024;
    const END: &str = "... diff truncated ...\n";
    let mut output = String::new();
    for label in changed_files(original, formatted) {
        let before = String::from_utf8_lossy(&original[&label]);
        let after = formatted
            .get(&label)
            .map_or_else(|| "".into(), |bytes| String::from_utf8_lossy(bytes));
        let before: Vec<_> = before.lines().collect();
        let after: Vec<_> = after.lines().collect();
        let prefix = before
            .iter()
            .zip(&after)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = before[prefix..]
            .iter()
            .rev()
            .zip(after[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let header = format!(
            "--- a/{label}\n+++ b/{label}\n@@ -{},{} +{},{} @@\n",
            prefix + 1,
            before.len() - prefix - suffix,
            prefix + 1,
            after.len() - prefix - suffix
        );
        if output.len() + header.len() + END.len() > LIMIT {
            output.push_str(END);
            return output;
        }
        output.push_str(&header);
        for (marker, lines) in [
            ('-', &before[prefix..before.len() - suffix]),
            ('+', &after[prefix..after.len() - suffix]),
        ] {
            for line in lines {
                if output.len() + line.len() + 2 + END.len() > LIMIT {
                    output.push_str(END);
                    return output;
                }
                output.push(marker);
                output.push_str(line);
                output.push('\n');
            }
        }
        if original[&label].ends_with(b"\n")
            != formatted
                .get(&label)
                .is_some_and(|bytes| bytes.ends_with(b"\n"))
        {
            let annotation = "\\ No newline at end of original file\n";
            if output.len() + annotation.len() + END.len() > LIMIT {
                output.push_str(END);
                return output;
            }
            output.push_str(annotation);
        }
    }
    output
}
