use std::ffi::OsString;
use std::fs;
use std::process::Command;

use qargo_tools::qlippy;
use qargo_tools::report::{Diagnostic, Envelope, Report};
use tempfile::TempDir;

const APPLY: &str = "unitary fn apply[static U:Op<Bit>](q:Q<Bit>)->Q<Bit> requires Apply(U){U(q)}";

fn root(files: &[(&str, &str)]) -> TempDir {
    let root = tempfile::tempdir().unwrap();
    for (path, source) in files {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    root
}

fn lint(files: &[(&str, &str)], deny: bool) -> Report {
    let root = root(files);
    let mut args = vec![root.path().as_os_str().to_owned()];
    if deny {
        args.push(OsString::from("--deny-warnings"));
    }
    qlippy::run(&args)
}

fn warnings(report: &Report) -> &[Diagnostic] {
    assert_eq!(report.exit_code, 0, "{:?}", report.envelope);
    assert_eq!(report.envelope.outcome, "ok");
    for diagnostic in &report.envelope.diagnostics {
        assert_eq!(diagnostic.category, "lint");
        assert_eq!(diagnostic.severity, "warning");
        assert!(diagnostic.suggestion.is_some());
        let policy = qargo_tools::rules::find(&diagnostic.id).expect("catalogued lint rule");
        assert_eq!(diagnostic.severity, policy.default_severity);
        assert_eq!(policy.promotion, qargo_tools::rules::Promotion::Advisory);
    }
    &report.envelope.diagnostics
}

#[test]
fn rule_catalog_is_machine_readable_and_independent_of_checking() {
    let root = root(&[("invalid.qli", "this is not Qleisli")]);
    let output = Command::new(env!("CARGO_BIN_EXE_qlippy"))
        .current_dir(root.path())
        .args(["--list-rules", "--format=json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    let envelope: Envelope = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope.format, "qlippy.result");
    assert_eq!(envelope.version, 1);
    assert_eq!(envelope.command, "list-rules");
    let result = envelope.result.unwrap();
    assert_eq!(result["catalog_version"], 1);
    assert_eq!(result["tool"]["version"], "0.1.5");
    assert!(result.get("qleisli_check").is_none());
    assert!(result.get("verified").is_none());
    let groups: Vec<_> = result["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|group| group["id"].as_str().unwrap())
        .collect();
    assert_eq!(groups, ["idiom", "complexity", "resource"]);
    let rules = result["rules"].as_array().unwrap();
    let ids: Vec<_> = rules
        .iter()
        .map(|rule| rule["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["double_inverse", "redundant_repeat_one", "unused_import"]
    );
    for rule in rules {
        assert!(groups.contains(&rule["group"].as_str().unwrap()));
        assert_eq!(rule["promotion"], "advisory");
        assert_eq!(rule["default_severity"], "warning");
        assert!(!rule["rationale"].as_str().unwrap().is_empty());
    }
    for args in [
        vec!["--list-rules", "--list-rules"],
        vec!["--list-rules", "--help"],
        vec!["--version", "--list-rules"],
        vec!["--list-rules", "source"],
        vec!["--list-rules", "--deny-warnings"],
        vec!["--list-rules", "--format=json", "--format=json"],
    ] {
        let args: Vec<_> = args.into_iter().map(OsString::from).collect();
        assert_eq!(qlippy::run(&args).exit_code, 2);
    }
}

#[test]
fn unused_imports_are_advisory_and_used_calls_are_retained() {
    let source =
        "use std::quantum::h; use std::quantum::x; unitary fn flip(q:Q<Bit>)->Q<Bit>{x(q)}";
    let report = lint(&[("library.qli", source)], false);
    let diagnostics = warnings(&report);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id, "unused_import");
    let primary = diagnostics[0].primary.as_ref().unwrap();
    assert_eq!(primary.path, "library.qli");
    assert_eq!(&source[primary.start..primary.end], "use std::quantum::h;");
    let result = report.envelope.result.as_ref().unwrap();
    assert_eq!(result["qleisli_check"]["status"], "passed");
    assert_eq!(result["source_count"], 1);
    assert_eq!(result["tool"]["qleisli_version"], "0.2.1");
    assert_eq!(result["tool"]["profile"], "finite-v0");
}

#[test]
fn one_repetition_points_to_complete_runtime_and_static_constructors() {
    let source = format!(
        "use std::quantum::x; {APPLY}
         unitary fn flip(q:Q<Bit>)->Q<Bit>{{x(q)}}
         unitary fn once(q:Q<Bit>)->Q<Bit>{{repeat_static(1,flip,q)}}
         unitary fn once_op(q:Q<Bit>)->Q<Bit>{{apply[repeat_op(1,flip)](q)}}"
    );
    let report = lint(&[("library.qli", &source)], false);
    let diagnostics = warnings(&report);
    assert_eq!(diagnostics.len(), 2);
    let snippets: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.id, "redundant_repeat_one");
            let primary = diagnostic.primary.as_ref().unwrap();
            &source[primary.start..primary.end]
        })
        .collect();
    assert_eq!(snippets, ["repeat_static(1,flip,q)", "repeat_op(1,flip)"]);
}

#[test]
fn inverse_lint_covers_the_complete_outer_constructor() {
    let source = format!(
        "{APPLY} unitary fn identity(q:Q<Bit>)->Q<Bit>{{q}}
         unitary fn twice(q:Q<Bit>)->Q<Bit>{{apply[inverse_op(inverse_op(identity))](q)}}"
    );
    let report = lint(&[("library.qli", &source)], false);
    let diagnostics = warnings(&report);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id, "double_inverse");
    let primary = diagnostics[0].primary.as_ref().unwrap();
    assert_eq!(
        &source[primary.start..primary.end],
        "inverse_op(inverse_op(identity))"
    );
}

#[test]
fn nested_static_combinators_are_traversed_for_all_rules() {
    let source = format!(
        "{APPLY} unitary fn identity(q:Q<Bit>)->Q<Bit>{{q}}
         unitary fn nested(q:Q<Bit>)->Q<Bit>{{
             apply[conjugate_op(identity,then_op(repeat_op(1,identity),inverse_op(inverse_op(identity))))](q)
         }}"
    );
    let report = lint(&[("library.qli", &source)], false);
    let diagnostics = warnings(&report);
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].id, "redundant_repeat_one");
    assert_eq!(diagnostics[1].id, "double_inverse");
}

#[test]
fn zero_many_and_single_inverse_are_not_redundant() {
    let source = format!(
        "{APPLY} unitary fn identity(q:Q<Bit>)->Q<Bit>{{q}}
         unitary fn zero(q:Q<Bit>)->Q<Bit>{{repeat_static(0,identity,q)}}
         unitary fn many(q:Q<Bit>)->Q<Bit>{{repeat_static(2,identity,q)}}
         unitary fn zero_op(q:Q<Bit>)->Q<Bit>{{apply[repeat_op(0,identity)](q)}}
         unitary fn many_op(q:Q<Bit>)->Q<Bit>{{apply[repeat_op(2,identity)](q)}}
         unitary fn inverse(q:Q<Bit>)->Q<Bit>{{apply[inverse_op(identity)](q)}}"
    );
    assert!(warnings(&lint(&[("library.qli", &source)], false)).is_empty());
}

#[test]
fn meaning_static_arguments_and_annotations_keep_imports() {
    let client = "use provider::flip; use provider::Flip; use provider::implementation;
        meaning LocalFlip:Bit=permutation_by(flip);
        unitary fn apply[static U:Op<Bit,Flip>](q:Q<Bit>)->Q<Bit> requires Apply(U){U(q)}
        unitary fn client(q:Q<Bit>)->Q<Bit>{apply[bind_op(implementation,Flip)](q)}";
    let provider = "use std::quantum::x;
        pub basis fn flip(b:Bit)->Bit{not b}
        pub meaning Flip:Bit=permutation_by(flip);
        pub unitary fn implementation(q:Q<Bit>)->Q<Bit>{x(q)}";
    let report = lint(&[("client.qli", client), ("provider.qli", provider)], false);
    assert!(warnings(&report).is_empty());
    assert_eq!(report.envelope.result.as_ref().unwrap()["source_count"], 2);
}

#[test]
fn imported_meaning_used_only_in_a_static_annotation_is_retained() {
    let client = "use meanings::Flip;
        unitary fn apply[static U:Op<Bit,Flip>](q:Q<Bit>)->Q<Bit> requires Apply(U){U(q)}";
    let meanings = "basis fn flip(b:Bit)->Bit{not b}
        pub meaning Flip:Bit=permutation_by(flip);";
    assert!(
        warnings(&lint(
            &[("client.qli", client), ("meanings.qli", meanings)],
            false
        ))
        .is_empty()
    );
}

#[test]
fn imported_operations_used_only_as_static_arguments_are_retained() {
    let client = format!(
        "use provider::identity; {APPLY}
         unitary fn client(q:Q<Bit>)->Q<Bit>{{apply[identity](q)}}"
    );
    let provider = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
    assert!(
        warnings(&lint(
            &[("client.qli", &client), ("provider.qli", provider)],
            false
        ))
        .is_empty()
    );
}

#[test]
fn shadowed_import_names_are_conservatively_retained() {
    for source in [
        "use std::quantum::x; unitary fn identity(x:Q<Bit>)->Q<Bit>{x}",
        "use std::quantum::x; unitary fn identity(q:Q<Bit>)->Q<Bit>{let x=q;x}",
        "use std::quantum::x; unitary fn identity[static x:Op<Bit>](q:Q<Bit>)->Q<Bit> requires Apply(x){q}",
    ] {
        assert!(warnings(&lint(&[("library.qli", source)], false)).is_empty());
    }
}

#[test]
fn utf8_crlf_and_multiple_modules_keep_original_byte_spans() {
    let first = "// 雪\r\nuse std::quantum::h;\r\nunitary fn identity(q:Q<Bit>)->Q<Bit>{q}\r\n";
    let second = "// λ\r\nunitary fn identity(q:Q<Bit>)->Q<Bit>{ repeat_static(1,identity,q) }";
    // The repeat function must be independent to avoid an ordinary recursive-call error.
    let second = second.replace("repeat_static(1,identity,q)", "repeat_static(1,provider,q)")
        + "\r\nunitary fn provider(q:Q<Bit>)->Q<Bit>{q}";
    let report = lint(&[("a.qli", first), ("nested/b.qli", &second)], false);
    let diagnostics = warnings(&report);
    assert_eq!(diagnostics.len(), 2);
    let unused = diagnostics[0].primary.as_ref().unwrap();
    assert_eq!((unused.line, unused.column), (2, 1));
    assert_eq!(unused.start, first.find("use").unwrap());
    let repeat = diagnostics[1].primary.as_ref().unwrap();
    assert_eq!(repeat.path, "nested/b.qli");
    assert_eq!(repeat.line, 2);
    let before = &second[second.find("unitary").unwrap()..repeat.start];
    assert_eq!(repeat.column, before.chars().count() + 1);
    assert_eq!(
        &second[repeat.start..repeat.end],
        "repeat_static(1,provider,q)"
    );
}

#[test]
fn compiler_failures_prevent_all_advisory_lints() {
    let cases = [
        ("type_mismatch", "unitary fn bad(q:Q<Bit>)->CBit{q}"),
        (
            "ownership",
            "unitary fn bad(q:Q<Bit>)->(Q<Bit>,Q<Bit>){(q,q)}",
        ),
        (
            "effect",
            "use std::observe::measure_z; unitary fn bad(q:Q<Bit>)->CBit{measure_z(q)}",
        ),
        (
            "contract",
            "basis fn flip(b:Bit)->Bit{not b} meaning Flip:Bit=permutation_by(flip);
             unitary fn wrong(q:Q<Bit>)->Q<Bit>{q}
             unitary fn apply[static U:Op<Bit,Flip>](q:Q<Bit>)->Q<Bit> requires Apply(U){U(q)}
             unitary fn bad(q:Q<Bit>)->Q<Bit>{apply[bind_op(wrong,Flip)](q)}",
        ),
        (
            "capability",
            "unitary fn bad[static U:Op<Bit>](q:Q<Bit>)->Q<Bit>{U(q)}",
        ),
    ];
    for (id, source) in cases {
        let source = format!("use std::quantum::h; {source}");
        let report = lint(&[("library.qli", &source)], false);
        assert_eq!(report.exit_code, 1, "{id}: {:?}", report.envelope);
        let result = report.envelope.result.as_ref().unwrap();
        assert_eq!(result["qleisli_check"]["status"], "failed");
        assert_eq!(result["source_count"], 1);
        assert!(result["source_id"].as_str().unwrap().starts_with("sha256:"));
        assert_eq!(result["tool"]["qleisli_version"], "0.2.1");
        assert_eq!(report.envelope.diagnostics.len(), 1);
        let diagnostic = &report.envelope.diagnostics[0];
        assert_eq!(diagnostic.category, "compiler", "{id}: {diagnostic:?}");
        assert_eq!(diagnostic.id, id, "{diagnostic:?}");
        assert_eq!(diagnostic.severity, "error");
        assert_eq!(diagnostic.primary.as_ref().unwrap().path, "library.qli");
    }
}

#[test]
fn denied_warnings_retain_the_actual_analysis_result() {
    let source = "use std::quantum::h; unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
    let report = lint(&[("library.qli", source)], true);
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.envelope.outcome, "error");
    assert!(report.envelope.result.is_some());
    assert_eq!(report.envelope.diagnostics[0].severity, "warning");
    assert_eq!(report.envelope.diagnostics[0].id, "unused_import");
}

#[test]
fn invalid_cli_combinations_are_usage_failures() {
    for args in [
        vec![],
        vec!["--version", "--help"],
        vec!["--version", "--deny-warnings"],
        vec!["--version", "source"],
        vec!["--help", "source"],
        vec!["source", "other"],
        vec!["--format=json", "--format=json", "source"],
        vec!["source", "--deny-warnings", "--deny-warnings"],
        vec!["source", "--unknown"],
        vec!["source", "--format=xml"],
        vec![""],
    ] {
        let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
        let report = qlippy::run(&args);
        assert_eq!(report.exit_code, 2, "{args:?}: {:?}", report.envelope);
        assert_eq!(report.envelope.diagnostics[0].category, "usage");
        assert!(report.envelope.result.is_none());
    }
}

#[test]
fn binary_json_version_help_and_empty_roots_have_one_envelope() {
    let root = root(&[]);
    for args in [
        vec![OsString::from("--version")],
        vec![OsString::from("--help")],
        vec![root.path().as_os_str().to_owned()],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_qlippy"))
            .arg("--format=json")
            .args(&args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout.last(), Some(&b'\n'));
        assert_eq!(
            output.stdout.iter().filter(|byte| **byte == b'\n').count(),
            1
        );
        let envelope: Envelope = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(envelope.format, "qlippy.result");
        assert_eq!(envelope.version, 1);
        assert_eq!(envelope.outcome, "ok");
        if envelope.command == "lint" {
            let result = envelope.result.unwrap();
            assert_eq!(result["source_count"], 0);
            assert_eq!(result["qleisli_check"]["status"], "not_run");
            assert_eq!(result["qleisli_check"]["reason"], "no_sources");
        }
    }
}

#[test]
fn binary_json_failures_use_consistent_exit_codes_and_results() {
    let root = root(&[(
        "library.qli",
        "use std::quantum::h; unitary fn identity(q:Q<Bit>)->Q<Bit>{q}",
    )]);
    for (options, expected_exit, result_present, severity) in [
        (vec!["--deny-warnings"], 1, true, "warning"),
        (vec!["--unknown"], 2, false, "error"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_qlippy"))
            .arg(root.path())
            .args(options)
            .arg("--format=json")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected_exit));
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout.iter().filter(|byte| **byte == b'\n').count(),
            1
        );
        let envelope: Envelope = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(envelope.outcome, "error");
        assert_eq!(envelope.result.is_some(), result_present);
        assert_eq!(envelope.diagnostics[0].severity, severity);
    }
}

#[test]
fn source_ids_are_portable_and_content_sensitive() {
    let source = "unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
    let first = lint(&[("library.qli", source)], false);
    let second = lint(&[("library.qli", source)], false);
    let changed = lint(&[("library.qli", &format!("{source}\n"))], false);
    assert_eq!(first.exit_code, 0);
    assert_eq!(second.exit_code, 0);
    assert_eq!(changed.exit_code, 0);
    let source_id = |report: &Report| report.envelope.result.as_ref().unwrap()["source_id"].clone();
    assert_eq!(source_id(&first), source_id(&second));
    assert_ne!(source_id(&first), source_id(&changed));
}
