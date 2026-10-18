//! Advisory linting of the same frozen sources accepted by the ordinary checker.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use qleisli::frontend::ast::{
    AxisSelection, BasisExpr, BasisExprKind, Block, Count, Decl, Expr, ExprKind, FnBody, Ident,
    NatKind, Natural, Pattern, PatternKind, Predicate, Requirement, Span, StaticOp, StaticOpKind,
    StaticParamKind, StmtKind, Type, TypeKind,
};
use qleisli::frontend::project::{ModuleOrigin, Project};
use serde_json::json;

use crate::adapter;
use crate::report::{Diagnostic, Location, Report, coordinates, tool_info};
use crate::rules::{self, DOUBLE_INVERSE, REDUNDANT_REPEAT_ONE, UNUSED_IMPORT};
use crate::snapshot::{Files, FrozenSources};

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
        return match tool_info("qlippy", super::VERSION) {
            Ok(tool) => Report::ok(FORMAT, "version", json!({ "tool": tool })),
            Err(diagnostic) => Report::fail(FORMAT, "version", diagnostic, 1),
        };
    }
    if options.list_rules {
        return match tool_info("qlippy", super::VERSION) {
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
    let input = match adapter::CapturedCheckInput::capture(&root) {
        Ok(input) => input,
        Err(diagnostic) => return Report::fail(FORMAT, "lint", diagnostic, 1),
    };
    analyze(
        input.sources(),
        input.configuration(),
        options.deny_warnings,
    )
}

fn analyze(sources: &FrozenSources, configuration: &Files, deny_warnings: bool) -> Report {
    let tool = match tool_info("qlippy", super::VERSION) {
        Ok(tool) => tool,
        Err(diagnostic) => return Report::fail(FORMAT, "lint", diagnostic, 1),
    };
    let checked = match adapter::check(sources, configuration) {
        Ok(checked) => checked,
        Err(diagnostic) if diagnostic.category != "compiler" || sources.count() == 0 => {
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
            match &parameter.kind {
                StaticParamKind::Natural | StaticParamKind::Basis => {}
                StaticParamKind::Operation {
                    basis,
                    codomain,
                    meaning,
                } => {
                    self.ty(basis);
                    if let Some(codomain) = codomain {
                        self.ty(codomain);
                    }
                    if let Some(meaning) = meaning {
                        self.name(meaning);
                    }
                }
            }
        }
        for requirement in &decl.requires {
            match requirement {
                Requirement::Access(constraint) => self.name(&constraint.name),
                Requirement::Predicate(predicate) => self.predicate(predicate),
            }
        }
        for parameter in &decl.params {
            self.pattern(&parameter.pattern);
            self.ty(&parameter.ty);
        }
        if let Some(domain) = &decl.meaning_domain {
            self.ty(domain);
        }
        self.ty(&decl.return_type);
        match &decl.body {
            FnBody::Natural(natural) => self.natural(natural),
            FnBody::Meaning { function, .. } | FnBody::MeaningReference { function } => {
                self.name(function);
            }
            FnBody::MeaningCompose { first, second } => {
                self.name(first);
                self.name(second);
            }
            FnBody::MeaningTensor { left, right } => {
                self.name(left);
                self.name(right);
            }
            FnBody::Basis(expression) => self.basis(expression),
            FnBody::Quantum(block) => self.block(block),
        }
    }

    fn ty(&mut self, ty: &Type) {
        match &ty.kind {
            TypeKind::Unit | TypeKind::Bit => {}
            TypeKind::Bits(size) => self.natural(size),
            TypeKind::Named(name) => self.name(name),
            TypeKind::Q(basis) => self.ty(basis),
            TypeKind::Tuple(items) => {
                for item in items {
                    self.ty(item);
                }
            }
        }
    }

    fn natural(&mut self, natural: &Natural) {
        match &natural.kind {
            NatKind::Number(_) => {}
            NatKind::Name(name) => {
                self.names.insert(name.clone());
            }
            NatKind::Call { callee, arguments } => {
                self.name(callee);
                for argument in arguments {
                    self.natural(argument);
                }
            }
            NatKind::Add(left, right) | NatKind::Sub(left, right) | NatKind::Mul(left, right) => {
                self.natural(left);
                self.natural(right);
            }
        }
    }

    fn predicate(&mut self, predicate: &Predicate) {
        self.natural(&predicate.left);
        self.natural(&predicate.right);
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
                StmtKind::StaticLet { name, value } => {
                    self.name(name);
                    self.natural(value);
                }
                StmtKind::Let { pattern, value } | StmtKind::MutableLet { pattern, value, .. } => {
                    self.pattern(pattern);
                    self.expr(value);
                }
                StmtKind::Assign { target, value } => {
                    self.name(target);
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
            StaticOpKind::Type(ty) => self.ty(ty),
            StaticOpKind::Natural(natural) => self.natural(natural),
            StaticOpKind::Specialize { name, arguments } => {
                self.name(name);
                for argument in arguments {
                    self.static_op(argument);
                }
            }
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
                // Preserve the literal-one rule; do not evaluate natural formulas.
                if matches!(count, Count::Natural(natural) if natural.kind == NatKind::Number(1)) {
                    self.findings.push(Finding {
                        id: REDUNDANT_REPEAT_ONE.id,
                        span: operation.span,
                    });
                }
                match count {
                    Count::Natural(natural) | Count::Power(natural) => self.natural(natural),
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
            ExprKind::UnsupportedClosure(closure) => {
                for parameter in &closure.parameters {
                    self.pattern(parameter);
                }
                self.block(&closure.body);
            }
            ExprKind::AccessCall {
                callee,
                static_args,
                args,
            } => {
                self.name(callee);
                for argument in static_args {
                    self.static_op(argument);
                }
                for argument in args {
                    self.expr(&argument.value);
                    if let Some(selection) = &argument.selection {
                        match selection {
                            AxisSelection::Index(index) => self.natural(index),
                            AxisSelection::Range { start, end } => {
                                self.natural(start);
                                self.natural(end);
                            }
                        }
                    }
                }
            }
            ExprKind::ApplyStatic { operation, input } | ExprKind::Adjoint { operation, input } => {
                self.static_op(operation);
                self.expr(input);
            }
            ExprKind::ApplyContract {
                implementation,
                specification,
                input,
            } => {
                self.name(implementation);
                self.name(specification);
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
            ExprKind::Controlled { operation, args } => {
                self.static_op(operation);
                for argument in args {
                    self.expr(argument);
                }
            }
            ExprKind::StaticIf {
                predicate,
                then_branch,
                else_branch,
            } => {
                self.predicate(predicate);
                self.block(then_branch);
                self.block(else_branch);
            }
            ExprKind::StaticFold {
                index,
                start,
                end,
                carry,
                initial,
                body,
                ..
            } => {
                self.name(index);
                self.natural(start);
                self.natural(end);
                self.pattern(carry);
                self.expr(initial);
                self.block(body);
            }
            ExprKind::Name(name) => self.name(name),
            ExprKind::Bit(_) | ExprKind::Unit => {}
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
        let source = "use std::gate::x; unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
        std::fs::write(root.path().join("module.qli"), source).unwrap();
        std::fs::write(
            root.path().join("Qargo.toml"),
            "schema-version=2\n[qrate]\nedition=\"2026\"\n",
        )
        .unwrap();
        let input = adapter::CapturedCheckInput::capture(root.path()).unwrap();
        let frozen = input.sources();
        std::fs::write(root.path().join("module.qli"), "invalid source").unwrap();
        let report = analyze(frozen, input.configuration(), false);
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
        let report = analyze(&source, &Files::new(), true);
        assert_eq!(report.exit_code, 0);
        let result = report.envelope.result.as_ref().unwrap();
        assert_eq!(result["source_count"], 0);
        assert_eq!(result["qleisli_check"]["status"], "not_run");
    }

    #[test]
    fn an_empty_input_with_invalid_explicit_configuration_is_not_a_source_check() {
        let source = FrozenSources::from_files(Files::new()).unwrap();
        let configuration = Files::from([(
            "Qargo.toml".to_owned(),
            b"schema-version=2\n[qrate]\nedition='2027'\n".to_vec(),
        )]);
        let report = analyze(&source, &configuration, false);
        assert_eq!(report.exit_code, 1);
        assert_eq!(report.envelope.version, 1);
        assert!(report.envelope.result.is_none());
        assert_eq!(report.envelope.diagnostics.len(), 1);
        assert_eq!(report.envelope.diagnostics[0].category, "compiler");
        assert!(report.envelope.diagnostics[0].primary.is_none());
        assert!(report.envelope.diagnostics[0].message.contains("2027"));
    }

    // These parser/visitor tests check lexical coverage, not Qleisli acceptance.
    #[test]
    fn current_declaration_forms_keep_possible_import_uses() {
        let source = r#"static fn size[const n: Nat]() -> Nat { dimension[n]() + offset - adjustment * factor }
classical fn label(q: Bit) -> Bit { flip(q) }
meaning reference_target: Bit = reference(reference_op);
meaning composition_target: Bit = compose(first, second);
meaning tensor_target: (Bit, Bit) = tensor(left, right);
meaning phase_target: Unit = phase_by(phase_body);
fn typed[const B: Basis, const U: Op<B -> Bits<width>, specification>, const n: Nat](q: Q<Bits<input_width>>) -> Q<Bits<output_width>> requires Applicable(U), Adjointable(U), Controllable(U), n >= threshold { q }
"#;
        let syntax = qleisli::frontend::parser::parse_module(source).unwrap();
        let mut visitor = Visitor::default();
        for decl in &syntax.decls {
            visitor.decl(decl);
        }
        for name in "n dimension offset adjustment factor flip reference_op first second left right phase_body B U width specification input_width output_width threshold".split_whitespace() {
            assert!(visitor.names.contains(name), "missing possible use: {name}");
        }
        assert!(!visitor.names.contains("unused"));
        assert!(visitor.findings.is_empty());
    }

    #[test]
    fn current_operation_forms_keep_names_and_complete_constructor_spans() {
        let source = r#"// λ
fn expressions(q: Q<Bit>) -> Q<Bit> {
    static let local = dimension[extent]();
    let a = invoke[type(Bits<arity>), specialized[extent], then_op(first_op, second_op), tensor_op(left_op, right_op), conjugate_op(before_op, after_op), checked_op(implementation, specification), 1 + increment](q);
    let b = invoke[adjoint(adjoint(operation))](adjoint(single_operation)(a));
    let c = power(repeatable, 1)(b);
    let d = power(repeatable, 2^exponent)(c);
    let e = controlled(controlled(operation))(control, d);
    let f = visit[selector](excl e[index], ctrl other[begin..end]);
    if static check_limit > ceiling {
        for static i in floor..upper carry value = f { yield value }
    } else {
        qfor static j in floor..upper carry other_value = fallback { yield other_value }
    }
}
"#;
        let syntax = qleisli::frontend::parser::parse_module(source).unwrap();
        let mut visitor = Visitor::default();
        visitor.decl(&syntax.decls[0]);
        for name in "local dimension extent invoke arity specialized first_op second_op left_op right_op before_op after_op implementation specification increment operation single_operation repeatable exponent control visit selector index other begin end check_limit ceiling i j floor upper value other_value fallback".split_whitespace() {
            assert!(visitor.names.contains(name), "missing possible use: {name}");
        }
        let findings: Vec<_> = visitor
            .findings
            .iter()
            .map(|finding| (finding.id, &source[finding.span.start..finding.span.end]))
            .collect();
        assert_eq!(
            findings,
            [
                (DOUBLE_INVERSE.id, "adjoint(adjoint(operation))"),
                (REDUNDANT_REPEAT_ONE.id, "power(repeatable, 1)"),
            ]
        );
        // A natural expression or exponent is not evaluated by the literal-one lint.
        let nonliteral =
            "fn f(q: Q<Bit>) -> Q<Bit> { let a = power(op, 1 + 0)(q); power(op, 2^0)(a) }";
        let syntax = qleisli::frontend::parser::parse_module(nonliteral).unwrap();
        let mut visitor = Visitor::default();
        visitor.decl(&syntax.decls[0]);
        assert!(visitor.findings.is_empty());
    }

    #[test]
    fn retained_diagnostic_syntax_is_traversed_without_becoming_accepted() {
        let source = r#"fn diagnostic() -> Unit {
    let mut shadow = first;
    shadow = replacement;
    let closure = |captured| { captured and external };
    ()
}
"#;
        let syntax = qleisli::frontend::parser::parse_module(source).unwrap();
        let mut visitor = Visitor::default();
        visitor.decl(&syntax.decls[0]);
        for name in "shadow first replacement closure captured external".split_whitespace() {
            assert!(visitor.names.contains(name), "missing lexical name: {name}");
        }
        assert!(visitor.findings.is_empty());
        // analyze still requires ordinary checker success before invoking this visitor.
    }
}
