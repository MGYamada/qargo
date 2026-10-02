//! Advisory linting of the same frozen sources accepted by the ordinary checker.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use qleisli::frontend::ast::{
    BasisExpr, BasisExprKind, Block, Decl, Expr, ExprKind, FnBody, Ident, Pattern, PatternKind,
    Span, StaticOp, StaticOpKind, StmtKind,
};
use qleisli::frontend::project::{ModuleOrigin, Project};
use serde_json::json;

use crate::adapter;
use crate::report::{Diagnostic, Location, Report, coordinates, tool_info};
use crate::rules::{self, DOUBLE_INVERSE, REDUNDANT_REPEAT_ONE, UNUSED_IMPORT};
use crate::snapshot::FrozenSources;

const FORMAT: &str = "qlippy.result";
const HELP: &str = "qlippy <source-root> [--format=json] [--deny-warnings]\nqlippy --list-rules [--format=json]\nqlippy --version [--format=json]\nqlippy --help [--format=json]\n\nCheck frozen Qleisli sources before emitting advisory lint diagnostics.\nNo source rewriting or mathematical evidence is produced.";

struct Options {
    source_root: Option<PathBuf>,
    deny_warnings: bool,
    help: bool,
    version: bool,
    list_rules: bool,
}

fn usage(message: &str) -> Report {
    Report::fail(
        FORMAT,
        "lint",
        Diagnostic::error("invalid_arguments", "usage", message),
        2,
    )
}

fn options(args: &[OsString]) -> Result<Options, Box<Report>> {
    let mut options = Options {
        source_root: None,
        deny_warnings: false,
        help: false,
        version: false,
        list_rules: false,
    };
    let mut format_seen = false;
    for arg in args {
        match arg.to_str() {
            Some("--format=json") if !format_seen => format_seen = true,
            Some("--deny-warnings") if !options.deny_warnings => options.deny_warnings = true,
            Some("--help") if !options.help => options.help = true,
            Some("--version") if !options.version => options.version = true,
            Some("--list-rules") if !options.list_rules => options.list_rules = true,
            _ if arg.to_string_lossy().starts_with('-') => {
                return Err(Box::new(usage("Unknown, repeated, or malformed option.")));
            }
            _ if arg.is_empty() => {
                return Err(Box::new(usage("The source root must not be empty.")));
            }
            _ if options.source_root.is_some() => {
                return Err(Box::new(usage("Only one source root may be supplied.")));
            }
            _ => options.source_root = Some(PathBuf::from(arg)),
        }
    }
    let special_count = [options.help, options.version, options.list_rules]
        .into_iter()
        .filter(|value| *value)
        .count();
    if special_count > 0 {
        if special_count > 1 || options.source_root.is_some() || options.deny_warnings {
            return Err(Box::new(usage(
                "Help, version, and rule listing must be used separately, without lint arguments.",
            )));
        }
    } else if options.source_root.is_none() {
        return Err(Box::new(usage(
            "A source root is required. Use --help for usage.",
        )));
    }
    Ok(options)
}

/// Handle CLI arguments without process-global state or a host-language runner.
pub fn run(args: &[OsString]) -> Report {
    let options = match options(args) {
        Ok(options) => options,
        Err(report) => return *report,
    };
    if options.help {
        return Report::ok(FORMAT, "help", json!({ "help": HELP }));
    }
    if options.version {
        return match tool_info("qlippy") {
            Ok(tool) => Report::ok(FORMAT, "version", json!({ "tool": tool })),
            Err(diagnostic) => Report::fail(FORMAT, "version", diagnostic, 1),
        };
    }
    if options.list_rules {
        return match tool_info("qlippy") {
            Ok(tool) => {
                let mut catalog = rules::catalog();
                catalog["tool"] = tool;
                Report::ok(FORMAT, "list-rules", catalog)
            }
            Err(diagnostic) => Report::fail(FORMAT, "list-rules", diagnostic, 1),
        };
    }
    let Some(root) = options.source_root else {
        return usage("A source root is required.");
    };
    let sources = match FrozenSources::capture(&root) {
        Ok(sources) => sources,
        Err(diagnostic) => return Report::fail(FORMAT, "lint", diagnostic, 1),
    };
    analyze(&sources, options.deny_warnings)
}

fn analyze(sources: &FrozenSources, deny_warnings: bool) -> Report {
    let tool = match tool_info("qlippy") {
        Ok(tool) => tool,
        Err(diagnostic) => return Report::fail(FORMAT, "lint", diagnostic, 1),
    };
    let checked = match adapter::check(sources) {
        Ok(checked) => checked,
        Err(diagnostic) if diagnostic.category != "compiler" => {
            return Report::fail(FORMAT, "lint", diagnostic, 1);
        }
        Err(diagnostic) => {
            let mut report = Report::fail(FORMAT, "lint", diagnostic, 1);
            report.envelope.result = Some(json!({
                "source_count": sources.count(),
                "source_id": sources.source_id(),
                "qleisli_check": {"status":"failed", "reason":"compiler_error"},
                "tool": tool,
            }));
            return report;
        }
    };
    let diagnostics = match checked.project() {
        Some(project) => match lint_project(project, sources) {
            Ok(diagnostics) => diagnostics,
            Err(diagnostic) => return Report::fail(FORMAT, "lint", diagnostic, 1),
        },
        None => Vec::new(),
    };
    let denied = deny_warnings && !diagnostics.is_empty();
    let mut report = Report::ok(
        FORMAT,
        "lint",
        json!({
            "source_count": checked.source_count(),
            "source_id": sources.source_id(),
            "qleisli_check": checked.qleisli_check(),
            "tool": tool,
        }),
    );
    report.envelope.diagnostics = diagnostics;
    if denied {
        report.envelope.outcome = "error".to_owned();
        report.exit_code = 1;
    }
    report
}

fn lint_project(project: &Project, sources: &FrozenSources) -> Result<Vec<Diagnostic>, Diagnostic> {
    let mut diagnostics = Vec::new();
    for module in project.modules.values() {
        if module.origin != ModuleOrigin::Local {
            continue;
        }
        let path = module
            .path
            .strip_prefix(&project.root)
            .ok()
            .and_then(|path| path.to_str())
            .map(|path| path.replace('\\', "/"))
            .ok_or_else(|| {
                Diagnostic::error("snapshot_mismatch", "tool", "Invalid checked source path.")
            })?;
        if sources.files().get(&path).map(Vec::as_slice) != Some(module.source.as_bytes()) {
            return Err(Diagnostic::error(
                "snapshot_mismatch",
                "tool",
                "Checked source bytes do not match the captured source snapshot.",
            ));
        }
        let mut visitor = Visitor::default();
        for decl in &module.ast.decls {
            visitor.decl(decl);
        }
        for import in &module.ast.uses {
            let Some(name) = import.path.last() else {
                continue;
            };
            if !visitor.names.contains(&name.text) {
                diagnostics.push(warning(
                    UNUSED_IMPORT.id,
                    &path,
                    &module.source,
                    import.span,
                    format!("Import `{}` has no possible use in this module.", name.text),
                    "Remove the unused import.",
                ));
            }
        }
        for finding in visitor.findings {
            let (message, suggestion) = match finding.id {
                "redundant_repeat_one" => (
                    "Repeating an operation once is redundant.",
                    "Use the operation directly instead of the one-repetition constructor.",
                ),
                "double_inverse" => (
                    "Taking the inverse twice is redundant.",
                    "Use the original operation instead of the two inverse constructors.",
                ),
                _ => unreachable!("the visitor produces only known lint rules"),
            };
            diagnostics.push(warning(
                finding.id,
                &path,
                &module.source,
                finding.span,
                message.to_owned(),
                suggestion,
            ));
        }
    }
    diagnostics.sort_by(|a, b| {
        a.primary
            .as_ref()
            .map(|location| (location.path.as_str(), location.start, location.end))
            .cmp(
                &b.primary
                    .as_ref()
                    .map(|location| (location.path.as_str(), location.start, location.end)),
            )
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(diagnostics)
}

fn warning(
    id: &str,
    path: &str,
    source: &str,
    span: Span,
    message: String,
    suggestion: &str,
) -> Diagnostic {
    let (line, column) = coordinates(source, span.start);
    Diagnostic {
        id: id.to_owned(),
        category: "lint".to_owned(),
        severity: rules::find(id)
            .expect("registered lint rule")
            .default_severity
            .to_owned(),
        primary: Some(Box::new(Location {
            path: path.to_owned(),
            start: span.start,
            end: span.end,
            line,
            column,
        })),
        message,
        suggestion: Some(suggestion.into()),
    }
}

struct Finding {
    id: &'static str,
    span: Span,
}

#[derive(Default)]
struct Visitor {
    names: BTreeSet<String>,
    findings: Vec<Finding>,
}

impl Visitor {
    fn name(&mut self, name: &Ident) {
        self.names.insert(name.text.clone());
    }

    fn decl(&mut self, decl: &Decl) {
        // Binder names deliberately count as possible use under shadowing.
        for parameter in &decl.static_params {
            self.name(&parameter.name);
            if let Some(meaning) = &parameter.meaning {
                self.name(meaning);
            }
        }
        for requirement in &decl.requires {
            self.name(&requirement.name);
        }
        for parameter in &decl.params {
            self.pattern(&parameter.pattern);
        }
        match &decl.body {
            FnBody::Meaning { function, .. } => self.name(function),
            FnBody::Basis(expression) => self.basis(expression),
            FnBody::Quantum(block) => self.block(block),
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match &pattern.kind {
            PatternKind::Name(name) => self.name(name),
            PatternKind::Wildcard => {}
            PatternKind::Tuple(items) => {
                for item in items {
                    self.pattern(item);
                }
            }
        }
    }

    fn block(&mut self, block: &Block) {
        for statement in &block.statements {
            match &statement.kind {
                StmtKind::Let { pattern, value } => {
                    self.pattern(pattern);
                    self.expr(value);
                }
                StmtKind::Expr(expression) => self.expr(expression),
            }
        }
        self.expr(&block.result);
    }

    fn basis(&mut self, expression: &BasisExpr) {
        match &expression.kind {
            BasisExprKind::Name(name) => self.name(name),
            BasisExprKind::Bit(_) | BasisExprKind::Unit => {}
            BasisExprKind::Tuple(items) => {
                for item in items {
                    self.basis(item);
                }
            }
            BasisExprKind::Call { callee, args } => {
                self.name(callee);
                for arg in args {
                    self.basis(arg);
                }
            }
            BasisExprKind::Not(inner) => self.basis(inner),
            BasisExprKind::Xor(a, b) | BasisExprKind::And(a, b) => {
                self.basis(a);
                self.basis(b);
            }
        }
    }

    fn static_op(&mut self, operation: &StaticOp) {
        match &operation.kind {
            StaticOpKind::Name(name) => self.name(name),
            StaticOpKind::Bind {
                implementation,
                meaning,
            } => {
                self.name(implementation);
                self.name(meaning);
            }
            StaticOpKind::Inverse(inner) => {
                if matches!(inner.kind, StaticOpKind::Inverse(_)) {
                    self.findings.push(Finding {
                        id: DOUBLE_INVERSE.id,
                        span: operation.span,
                    });
                }
                self.static_op(inner);
            }
            StaticOpKind::Controlled(inner) => self.static_op(inner),
            StaticOpKind::Repeat(count, inner) => {
                if *count == 1 {
                    self.findings.push(Finding {
                        id: REDUNDANT_REPEAT_ONE.id,
                        span: operation.span,
                    });
                }
                self.static_op(inner);
            }
            StaticOpKind::Then(a, b)
            | StaticOpKind::Tensor(a, b)
            | StaticOpKind::Conjugate(a, b) => {
                self.static_op(a);
                self.static_op(b);
            }
        }
    }

    fn expr(&mut self, expression: &Expr) {
        match &expression.kind {
            ExprKind::ApplyContract {
                implementation,
                specification,
                input,
            } => {
                self.name(implementation);
                self.name(specification);
                self.expr(input);
            }
            ExprKind::Adjoint { function, input } => {
                self.name(function);
                self.expr(input);
            }
            ExprKind::RepeatStatic {
                count,
                function,
                input,
            } => {
                if *count == 1 {
                    self.findings.push(Finding {
                        id: REDUNDANT_REPEAT_ONE.id,
                        span: expression.span,
                    });
                }
                self.name(function);
                self.expr(input);
            }
            ExprKind::QuantumIf {
                control,
                target,
                zero,
                one,
            } => {
                self.expr(control);
                self.expr(target);
                self.name(zero);
                self.name(one);
            }
            ExprKind::Name(name) => self.name(name),
            ExprKind::CBit(_) | ExprKind::Unit => {}
            ExprKind::Tuple(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            ExprKind::Not(inner) => self.expr(inner),
            ExprKind::And(a, b) | ExprKind::Xor(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Call {
                callee,
                static_args,
                args,
            } => {
                self.name(callee);
                for arg in static_args {
                    self.static_op(arg);
                }
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(condition);
                self.block(then_branch);
                self.block(else_branch);
            }
            ExprKind::CoherentLift {
                binder,
                input,
                basis,
            } => {
                self.pattern(binder);
                self.expr(input);
                self.basis(basis);
            }
            ExprKind::WithComputed {
                source,
                function,
                binder,
                body,
            } => {
                self.expr(source);
                self.name(function);
                self.name(binder);
                self.block(body);
            }
            ExprKind::CertifiedComputed {
                source,
                function,
                logical,
                data_binder,
                ancilla_binder,
                body,
            } => {
                self.expr(source);
                self.name(function);
                self.name(logical);
                self.name(data_binder);
                self.name(ancilla_binder);
                self.block(body);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::Files;

    #[test]
    fn frozen_sources_remain_bound_after_the_original_changes() {
        let root = tempfile::tempdir().unwrap();
        let source = "use std::quantum::x; unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
        std::fs::write(root.path().join("module.qli"), source).unwrap();
        let frozen = FrozenSources::capture(root.path()).unwrap();
        std::fs::write(root.path().join("module.qli"), "invalid source").unwrap();
        let report = analyze(&frozen, false);
        assert_eq!(report.exit_code, 0);
        assert_eq!(report.envelope.diagnostics[0].id, "unused_import");
        assert_eq!(
            report.envelope.result.as_ref().unwrap()["source_id"],
            frozen.source_id()
        );
    }

    #[test]
    fn scalar_coordinates_count_crlf_once() {
        let source = "// 雪\r\n// é\r\n α constructor";
        assert_eq!(
            coordinates(source, source.find("constructor").unwrap()),
            (3, 4)
        );
    }

    #[test]
    fn a_frozen_empty_input_skips_the_compiler() {
        let source = FrozenSources::from_files(Files::new()).unwrap();
        let report = analyze(&source, true);
        assert_eq!(report.exit_code, 0);
        let result = report.envelope.result.as_ref().unwrap();
        assert_eq!(result["source_count"], 0);
        assert_eq!(result["qleisli_check"]["status"], "not_run");
    }
}
