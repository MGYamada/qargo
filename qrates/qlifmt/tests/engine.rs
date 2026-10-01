use std::ffi::OsString;
use std::fs;
use std::process::Command;

use qargo_tools::qlifmt_engine::{
    apply_files, changed_files, diff, format_files, run, validate_formatted,
};
use qargo_tools::report::Envelope;
use qargo_tools::snapshot::Files;
use tempfile::TempDir;

const SOURCE: &str = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
const FORMATTED: &str = "pub unitary fn identity(q: Q<Bit>) -> Q<Bit> {\n    q\n}\n";

fn files(source: &str) -> Files {
    Files::from([("library.qli".into(), source.as_bytes().to_vec())])
}

fn root(sources: &[(&str, &str)]) -> TempDir {
    let root = tempfile::tempdir().unwrap();
    for (label, source) in sources {
        let path = root.path().join(label);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    root
}

#[test]
fn initial_style_is_four_spaces_and_idempotent() {
    let formatted = format_files(&files(SOURCE)).unwrap();
    assert_eq!(formatted["library.qli"], FORMATTED.as_bytes());
    assert_eq!(format_files(&formatted).unwrap(), formatted);
    assert_eq!(changed_files(&files(SOURCE), &formatted), ["library.qli"]);
    assert!(
        diff(&files(SOURCE), &formatted).starts_with("--- a/library.qli\n+++ b/library.qli\n@@")
    );
}

#[test]
fn check_preserves_input_while_write_updates_all_files() {
    let root = root(&[("library.qli", SOURCE), ("nested/other.qli", SOURCE)]);
    let report = run(&[root.path().as_os_str().to_owned(), "--check".into()]);
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.envelope.diagnostics[0].id, "formatting_required");
    let result = report.envelope.result.unwrap();
    assert_eq!(result["check"], true);
    assert_eq!(
        result["changed_files"],
        serde_json::json!(["library.qli", "nested/other.qli"])
    );
    assert_eq!(result["updated_files"], serde_json::json!([]));
    assert!(!result["diff"].as_str().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(root.path().join("library.qli")).unwrap(),
        SOURCE
    );
    let report = run(&[root.path().as_os_str().to_owned()]);
    assert_eq!(report.exit_code, 0, "{:?}", report.envelope);
    let result = report.envelope.result.unwrap();
    assert_eq!(result["diff"], "");
    assert_eq!(
        result["updated_files"],
        serde_json::json!(["library.qli", "nested/other.qli"])
    );
    assert_eq!(
        fs::read_to_string(root.path().join("nested/other.qli")).unwrap(),
        FORMATTED
    );
    assert_eq!(
        run(&[root.path().as_os_str().to_owned(), "--check".into()]).exit_code,
        0
    );
}

#[test]
fn single_file_input_leaves_sibling_sources_alone() {
    let root = root(&[("library.qli", SOURCE), ("other.qli", SOURCE)]);
    let report = run(&[root.path().join("library.qli").as_os_str().to_owned()]);
    assert_eq!(report.exit_code, 0, "{:?}", report.envelope);
    assert_eq!(
        fs::read_to_string(root.path().join("library.qli")).unwrap(),
        FORMATTED
    );
    assert_eq!(
        fs::read_to_string(root.path().join("other.qli")).unwrap(),
        SOURCE
    );
}

#[test]
fn parser_failure_prevents_every_write_and_retains_binding() {
    let root = root(&[
        ("a.qli", SOURCE),
        ("nested/b.qli", "// 雪\r\nunitary fn broken(\r\n"),
    ]);
    let report = run(&[root.path().as_os_str().to_owned()]);
    assert_eq!(report.exit_code, 1);
    let result = report.envelope.result.unwrap();
    assert_eq!(result["source_count"], 2);
    assert_eq!(result["formatted_source_id"], serde_json::Value::Null);
    assert_eq!(result["changed_files"], serde_json::json!([]));
    assert_eq!(result["updated_files"], serde_json::json!([]));
    assert_eq!(result["qleisli_check"]["status"], "not_run");
    assert_eq!(result["qleisli_check"]["reason"], "syntax_only");
    assert_eq!(report.envelope.diagnostics[0].category, "compiler");
    assert_eq!(
        report.envelope.diagnostics[0]
            .primary
            .as_ref()
            .unwrap()
            .path,
        "nested/b.qli"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("a.qli")).unwrap(),
        SOURCE
    );
}

#[test]
fn formatting_accepts_type_ownership_and_unresolved_name_errors() {
    for source in [
        "unitary fn bad(q:Q<Bit>)->CBit{q}",
        "unitary fn bad(q:Q<Bit>)->(Q<Bit>,Q<Bit>){(q,q)}",
        "use missing::module::function; unitary fn bad(q:Q<Bit>)->Q<Bit>{unknown(q)}",
        "unitary fn bad[static U:Op<Bit>](q:Q<Bit>)->Q<Bit>{U(q)}",
    ] {
        let formatted = format_files(&files(source)).unwrap();
        validate_formatted(&files(source), &formatted).unwrap();
        assert_eq!(format_files(&formatted).unwrap(), formatted);
    }
}

#[test]
fn ordinary_doc_nested_and_utf8_comments_retain_bytes_and_attachment() {
    let source = "//! Module 雪\n/** Outer λ */pub unitary fn identity(q:Q<Bit>)->Q<Bit>{/*! Inner */\n/* first\n  /* nested */ last */ let q=/* between */q; // trailing 雪\nq}//// ordinary\n/// Other\nunitary fn other(q:Q<Bit>)->Q<Bit>{q}";
    let formatted = format_files(&files(source)).unwrap();
    let text = std::str::from_utf8(&formatted["library.qli"]).unwrap();
    for comment in [
        "//! Module 雪",
        "/** Outer λ */",
        "/*! Inner */",
        "/* first\n  /* nested */ last */",
        "/* between */",
        "// trailing 雪",
        "//// ordinary",
        "/// Other",
    ] {
        assert!(text.contains(comment), "{text}");
    }
    validate_formatted(&files(source), &formatted).unwrap();
    assert_eq!(format_files(&formatted).unwrap(), formatted);
}

#[test]
fn crlf_and_comment_only_files_are_preserved() {
    let source = format!("// 雪\r\n{SOURCE}\r\n");
    let formatted = format_files(&files(&source)).unwrap();
    let text = std::str::from_utf8(&formatted["library.qli"]).unwrap();
    assert_eq!(
        text,
        format!("// 雪\r\n{}", FORMATTED.replace('\n', "\r\n"))
    );
    assert!(!text.replace("\r\n", "").contains('\n'));
    for source in ["", "   \n\t", "//! Empty module\n", "/* ordinary */"] {
        let formatted = format_files(&files(source)).unwrap();
        assert_eq!(format_files(&formatted).unwrap(), formatted);
    }
}

#[test]
fn syntax_forms_and_static_combinators_are_preserved() {
    let source = "use std::basis::xor2;\n\
        pub basis fn labels((a,b): (Bit,Bit), c:Bit)->(Bit,Bit){(not a and b xor c,1)}\n\
        pub meaning Perm:(Bit,Bit)=permutation_by(labels);\n\
        meaning Phase:Bit=phase_by(labels);\n\
        iso fn prepare(q:Unit)->Q<Bit>{let q=init0(); q}\n\
        observe fn observe_bit(q:Q<Bit>)->CBit{if true { measure_z(q) } else { false }}\n\
        unitary fn lift(q:Q<Bit>)->Q<Bit>{do b<-q; pure(not b)}\n\
        unitary fn operations[static U:Op<Bit,Phase>,static V:Op<Bit>](q:Q<Bit>)->Q<Bit> requires Apply(U),Adjoint(U),Controlled(V){\n\
            let q=adjoint(U,q); let q=repeat_static(2,provider,q);\n\
            let q=apply[conjugate_op(tensor_op(bind_op(provider,Phase),controlled_op(V)),then_op(repeat_op(2,U),inverse_op(V)))](q);\n\
            let q=apply_contract(provider,Phase,q);\n\
            with_computed(q,labels,Phase){|data,auxiliary| (data,auxiliary)}\n\
        }\n\
        unitary fn choose(q:Q<Bit>)->Q<Bit>{qif(q,q){0=>provider,1=>provider}}";
    let formatted = format_files(&files(source)).unwrap();
    validate_formatted(&files(source), &formatted).unwrap();
    assert_eq!(format_files(&formatted).unwrap(), formatted);
}

#[test]
fn width_target_wraps_long_signatures_and_expressions() {
    let parameters = (0..12)
        .map(|index| format!("parameter_{index}: Q<Bit>"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "unitary fn long_name({parameters})->Q<Bit>{{provider(parameter_0,parameter_1,parameter_2,parameter_3,parameter_4,parameter_5,parameter_6)}}"
    );
    let formatted = format_files(&files(&source)).unwrap();
    let text = std::str::from_utf8(&formatted["library.qli"]).unwrap();
    assert!(
        text.lines().all(|line| line.chars().count() <= 100),
        "{text}"
    );
    assert_eq!(format_files(&formatted).unwrap(), formatted);
    let source = format!("unitary fn {}(q:Q<Bit>)->Q<Bit>{{q}}", "x".repeat(150));
    assert!(format_files(&files(&source)).is_ok());
}

#[test]
fn output_validation_rejects_token_comment_and_file_set_changes() {
    let original = files("/// Identity\nunitary fn identity(q:Q<Bit>)->Q<Bit>{/* keep */q}");
    for invalid in [
        files("/// Identity\nunitary fn identity(q:Q<Bit>)->Q<Bit>{/* keep */unknown}"),
        files("/// Identity\nunitary fn identity(q:Q<Bit>)->Q<Bit>{/* removed */q}"),
        files("unitary fn identity(q:Q<Bit>)->Q<Bit>{q}"),
        Files::new(),
        Files::from([("../escape.qli".into(), SOURCE.as_bytes().to_vec())]),
    ] {
        assert!(validate_formatted(&original, &invalid).is_err());
    }
}

#[test]
fn input_change_preflight_prevents_all_updates() {
    let root = root(&[("a.qli", SOURCE), ("b.qli", SOURCE)]);
    let original = Files::from([
        ("a.qli".into(), SOURCE.as_bytes().to_vec()),
        ("b.qli".into(), SOURCE.as_bytes().to_vec()),
    ]);
    let formatted = format_files(&original).unwrap();
    fs::write(root.path().join("b.qli"), format!("{SOURCE}\n")).unwrap();
    let failed = apply_files(root.path(), &original, &formatted).unwrap_err();
    assert_eq!(failed.diagnostic.id, "source_changed");
    assert!(failed.updated_files.is_empty());
    assert_eq!(
        fs::read_to_string(root.path().join("a.qli")).unwrap(),
        SOURCE
    );
}

#[cfg(unix)]
#[test]
fn atomic_application_preserves_permissions_and_rejects_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = root(&[("library.qli", SOURCE)]);
    let path = root.path().join("library.qli");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let original = files(SOURCE);
    let formatted = format_files(&original).unwrap();
    assert_eq!(
        apply_files(root.path(), &original, &formatted).unwrap(),
        ["library.qli"]
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let target = root.path().join("target.qli");
    fs::rename(&path, &target).unwrap();
    symlink(&target, &path).unwrap();
    assert!(apply_files(root.path(), &formatted, &formatted).is_err());
    assert_eq!(fs::read(&target).unwrap(), formatted["library.qli"]);
    fs::create_dir(root.path().join("nested-real")).unwrap();
    fs::write(root.path().join("nested-real/a.qli"), SOURCE).unwrap();
    symlink(root.path().join("nested-real"), root.path().join("nested")).unwrap();
    let original = Files::from([("nested/a.qli".into(), SOURCE.as_bytes().to_vec())]);
    assert!(apply_files(root.path(), &original, &format_files(&original).unwrap()).is_err());
}

#[test]
fn diff_is_utf8_line_oriented_and_bounded() {
    let source = "// 雪\n".repeat(30_000);
    let original = files(&source);
    let formatted = files(&source.replace('雪', "雨"));
    let diff = diff(&original, &formatted);
    assert!(diff.len() <= 64 * 1024);
    assert!(diff.ends_with("... diff truncated ...\n"));
}

#[test]
fn empty_roots_and_json_metadata_do_not_claim_a_check() {
    let root = root(&[]);
    for args in [
        vec!["--help".into()],
        vec!["--version".into()],
        vec![root.path().as_os_str().to_owned()],
        vec![root.path().as_os_str().to_owned(), "--check".into()],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_qlifmt"))
            .arg("--format=json")
            .args(&args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout.iter().filter(|byte| **byte == b'\n').count(),
            1
        );
        let envelope: Envelope = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(envelope.format, "qlifmt.result");
        assert_eq!(envelope.version, 1);
        if envelope.command == "fmt" {
            let result = envelope.result.unwrap();
            assert_eq!(result["source_count"], 0);
            assert_eq!(
                result["qleisli_check"],
                serde_json::json!({"status":"not_run", "reason":"no_sources"})
            );
            assert_eq!(result["tool"]["version"], "0.1.4");
            assert_eq!(result["tool"]["qleisli_version"], "0.2.1");
        }
    }
}

#[test]
fn cli_rejects_repeated_unknown_and_conflicting_options() {
    for args in [
        vec![],
        vec![""],
        vec!["--check"],
        vec!["--help", "--version"],
        vec!["--version", "--check"],
        vec!["--help", "source"],
        vec!["source", "other"],
        vec!["source", "--check", "--check"],
        vec!["source", "--format=json", "--format=json"],
        vec!["source", "--unknown"],
        vec!["source", "--format=xml"],
    ] {
        let args: Vec<_> = args.into_iter().map(OsString::from).collect();
        let report = run(&args);
        assert_eq!(report.exit_code, 2, "{args:?}");
        assert_eq!(report.envelope.diagnostics[0].category, "usage");
        assert!(report.envelope.result.is_none());
    }
}
