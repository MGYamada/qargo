//! Documentation is parsed syntax, never compiler or mathematical evidence.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use qargo_tools::qlidoc_engine::{publish, render_files, run};
use qargo_tools::snapshot::{Files, digest_path};
use serde_json::Value;

fn sources(label: &str, source: &str) -> Files {
    Files::from([(label.to_owned(), source.as_bytes().to_vec())])
}

fn page(files: &Files, path: &str) -> String {
    String::from_utf8(files[path].clone()).unwrap()
}

fn canonical_temp() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(temp.path()).unwrap();
    (temp, path)
}

fn command_args(input: &Path, output: &Path) -> Vec<OsString> {
    vec![
        input.as_os_str().to_owned(),
        format!("--output={}", output.display()).into(),
    ]
}

#[test]
fn public_default_and_private_option_preserve_all_module_pages() {
    let mut files = sources(
        "main.qli",
        "//! Visible module.\n/// Public notes.\npub unitary fn shown(q:Q<Bit>)->Q<Bit>{q}\n/// Secret notes.\nunitary fn hidden(q:Q<Bit>)->Q<Bit>{q}",
    );
    files.insert(
        "nested/private.qli".into(),
        b"unitary fn internal(q:Q<Bit>)->Q<Bit>{q}".to_vec(),
    );
    let public = render_files(&files, false).unwrap();
    let main = page(&public, "modules/main.md");
    assert!(main.contains("Visible module.") && main.contains("Public notes."));
    assert!(main.contains("shown (public)") && !main.contains("hidden"));
    assert!(!main.contains("Secret notes."));
    assert!(page(&public, "index.md").contains("nested::private"));
    assert!(page(&public, "modules/nested/private.md").contains("No visible declarations."));
    let all = render_files(&files, true).unwrap();
    assert!(page(&all, "modules/main.md").contains("hidden (private)"));
    assert!(page(&all, "modules/main.md").contains("Secret notes."));
    assert!(page(&all, "modules/nested/private.md").contains("internal (private)"));
}

#[test]
fn signatures_retain_static_parameters_constraints_and_meaning_bindings() {
    let input = sources(
        "api.qli",
        "pub meaning M:Bit=permutation_by(flip);\npub unitary fn apply[static U:Op<Bit,M>](q:Q<Bit>)->Q<Bit> requires Apply(U), Adjoint(U), Controlled(U) {U(q)}\npub basis fn flip(b:Bit)->Bit{not b}",
    );
    let output = render_files(&input, false).unwrap();
    let text = page(&output, "modules/api.md");
    assert!(text.contains("pub meaning M:Bit=permutation_by(flip);"));
    assert!(text.contains("[static U:Op<Bit,M>]"));
    assert!(text.contains("requires Apply(U), Adjoint(U), Controlled(U)"));
    assert!(!text.contains("{U(q)}"));
    assert!(text.contains("pub basis fn flip(b:Bit)->Bit\n```"));
}

#[test]
fn every_declaration_kind_and_signature_comment_is_documented() {
    let input = sources(
        "kinds.qli",
        "pub meaning P:Bit=phase_by(phase);\npub basis fn phase(b:Bit)->Bit /* { comment } */ {b}\npub iso fn isometry(q:Q<Bit>)->Q<Bit>{q}\npub unitary fn unit(q:Q<Bit>)->Q<Bit>{q}\npub observe fn measurement(q:Q<Bit>)->CBit{true}",
    );
    let output = render_files(&input, false).unwrap();
    let text = page(&output, "modules/kinds.md");
    for name in ["P", "phase", "isometry", "unit", "measurement"] {
        assert!(text.contains(&format!("## {name} (public)")));
    }
    assert!(text.contains("pub basis fn phase(b:Bit)->Bit /* { comment } */\n```"));
    assert!(!text.contains("{true}"));
}

#[test]
fn inner_outer_utf8_crlf_and_markdown_fences_survive_rendering() {
    let input = sources(
        "雪 [module].qli",
        "//! 雪 module.\r\n/** Outer documentation.\r\n```qli\r\nexample\r\n```\r\n*/\r\npub unitary fn example(/* ```` signature */ q:Q<Bit>)->Q<Bit>{\r\n/*! Inner documentation. */\r\nq\r\n}",
    );
    let output = render_files(&input, false).unwrap();
    let text = page(&output, "modules/雪 [module].md");
    assert!(text.contains("雪 module."));
    assert!(text.contains("Outer documentation."));
    assert!(text.contains("Inner documentation."));
    assert!(text.contains("`````qli\npub unitary fn example(/* ```` signature */"));
    assert!(text.contains("```qli\nexample\n```"));
    assert!(!text.contains('\r'));
    let index = page(&output, "index.md");
    assert!(index.contains("%E9%9B%AA%20%5Bmodule%5D.md"));
    assert!(index.contains("&#91;module&#93;"));
}

#[test]
fn syntax_only_generation_accepts_type_and_name_errors_without_executing_examples() {
    let input = sources(
        "bad_types.qli",
        "/// ```qli\n/// this is not executable source\n/// ```\npub unitary fn invalid()->Q<Bit>{missing()}",
    );
    let output = render_files(&input, false).unwrap();
    let text = page(&output, "modules/bad_types.md");
    assert!(text.contains("invalid (public)"));
    assert!(text.contains("this is not executable source"));
    assert!(text.contains("no type, ownership or contract verification"));
}

#[test]
fn empty_sources_generate_a_deterministic_index() {
    let output = render_files(&Files::new(), false).unwrap();
    assert_eq!(output.len(), 1);
    assert!(page(&output, "index.md").contains("No source modules."));
    assert_eq!(output, render_files(&Files::new(), true).unwrap());
}

#[test]
fn parse_failures_preserve_original_utf8_coordinates_and_do_not_publish() {
    let (_temp, root) = canonical_temp();
    fs::write(root.join("invalid.qli"), "// 雪\r\ninvalid").unwrap();
    let output = root.join("docs");
    let report = run(&command_args(&root.join("invalid.qli"), &output));
    assert_eq!(report.exit_code, 1);
    let result = report.envelope.result.unwrap();
    assert_eq!(result["source_count"], 1);
    assert_eq!(result["qleisli_check"]["reason"], "syntax_only");
    assert!(result["artifact_path"].is_null());
    assert_eq!(result["files"], serde_json::json!([]));
    assert_eq!(result["tool"]["version"], "0.1.1");
    let location = report.envelope.diagnostics[0].primary.as_ref().unwrap();
    assert_eq!(location.path, "invalid.qli");
    assert_eq!((location.line, location.column), (2, 1));
    assert_eq!(location.start, "// 雪\r\n".len());
    assert!(!output.exists());
}

#[test]
fn relocation_and_repeated_publication_are_deterministic() {
    let (_first, first) = canonical_temp();
    let (_second, second) = canonical_temp();
    let source = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
    for root in [&first, &second] {
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/module.qli"), source).unwrap();
    }
    let one = run(&command_args(&first.join("src"), &first.join("docs")));
    let two = run(&command_args(&second.join("src"), &second.join("docs")));
    let again = run(&command_args(&first.join("src"), &first.join("docs")));
    assert_eq!((one.exit_code, two.exit_code, again.exit_code), (0, 0, 0));
    let one = one.envelope.result.unwrap();
    let two = two.envelope.result.unwrap();
    assert_eq!(one["source_id"], two["source_id"]);
    assert_eq!(one["files"], two["files"]);
    assert_eq!(
        fs::read(first.join("docs/index.md")).unwrap(),
        fs::read(second.join("docs/index.md")).unwrap()
    );
    for file in one["files"].as_array().unwrap() {
        assert_eq!(
            file["sha256"].as_str().unwrap(),
            digest_path(&first.join("docs").join(file["path"].as_str().unwrap())).unwrap()
        );
    }
}

#[test]
fn conflicting_and_extra_artifacts_are_never_overwritten() {
    let (_temp, root) = canonical_temp();
    let generated = render_files(
        &sources("module.qli", "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}"),
        false,
    )
    .unwrap();
    let output = root.join("docs");
    publish(&output, &generated).unwrap();
    fs::write(output.join("index.md"), "unrelated content").unwrap();
    assert_eq!(
        publish(&output, &generated).unwrap_err().id,
        "artifact_mismatch"
    );
    assert_eq!(
        fs::read_to_string(output.join("index.md")).unwrap(),
        "unrelated content"
    );
    fs::write(output.join("index.md"), &generated["index.md"]).unwrap();
    fs::create_dir(output.join("extra")).unwrap();
    assert_eq!(
        publish(&output, &generated).unwrap_err().id,
        "artifact_mismatch"
    );
    assert!(output.join("extra").exists());
}

#[test]
fn output_failure_retains_input_binding_and_intended_artifact_inventory() {
    let (_temp, root) = canonical_temp();
    let input = root.join("module.qli");
    fs::write(&input, "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}").unwrap();
    let output = root.join("docs");
    fs::write(&output, "unrelated").unwrap();
    let report = run(&command_args(&input, &output));
    assert_eq!(report.exit_code, 1);
    let result = report.envelope.result.unwrap();
    assert_eq!(result["source_count"], 1);
    assert_eq!(result["artifact_path"], output.to_str().unwrap());
    assert_eq!(result["files"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read_to_string(output).unwrap(), "unrelated");
}

#[test]
fn source_overlap_is_rejected_but_a_file_can_write_a_sibling_directory() {
    let (_temp, root) = canonical_temp();
    let input = root.join("src");
    fs::create_dir(&input).unwrap();
    fs::write(
        input.join("module.qli"),
        "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    let child = input.join("docs");
    assert_eq!(run(&command_args(&input, &child)).exit_code, 1);
    assert!(!child.exists());
    assert_eq!(run(&command_args(&input, &root)).exit_code, 1);
    assert_eq!(
        run(&command_args(&input.join("module.qli"), &child)).exit_code,
        0
    );
}

#[test]
fn artifact_path_traversal_and_file_directory_collisions_are_rejected() {
    let (_temp, root) = canonical_temp();
    let output = root.join("docs");
    assert!(
        publish(
            &output,
            &Files::from([("../escape".into(), b"bad".to_vec())])
        )
        .is_err()
    );
    assert!(
        publish(
            &output,
            &Files::from([("dir".into(), vec![]), ("dir/file".into(), vec![])])
        )
        .is_err()
    );
    assert!(!output.exists());
    assert!(!root.join("escape").exists());
}

#[test]
fn source_names_that_produce_conflicting_output_paths_fail_before_publication() {
    let (_temp, root) = canonical_temp();
    let input = Files::from([
        (
            "a.qli".into(),
            b"pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}".to_vec(),
        ),
        (
            "a.md/b.qli".into(),
            b"pub unitary fn g(q:Q<Bit>)->Q<Bit>{q}".to_vec(),
        ),
    ]);
    let output = root.join("docs");
    let rendered = render_files(&input, false).unwrap();
    assert!(publish(&output, &rendered).is_err());
    assert!(!output.exists());
}

#[test]
fn concurrent_identical_publications_reuse_one_complete_output() {
    let (_temp, root) = canonical_temp();
    let output = root.join("docs");
    let rendered = render_files(
        &sources("module.qli", "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}"),
        false,
    )
    .unwrap();
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    publish(&output, &rendered)
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
    });
    assert_eq!(
        fs::read(output.join("index.md")).unwrap(),
        rendered["index.md"]
    );
    assert_eq!(
        fs::read(output.join("modules/module.md")).unwrap(),
        rendered["modules/module.md"]
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn special_output_files_are_rejected_without_changing_them() {
    let (_temp, root) = canonical_temp();
    let rendered = render_files(&Files::new(), false).unwrap();
    let output = root.join("docs");
    assert!(
        Command::new("mkfifo")
            .arg(&output)
            .status()
            .unwrap()
            .success()
    );
    assert!(publish(&output, &rendered).is_err());
    assert!(!fs::symlink_metadata(&output).unwrap().is_dir());
}

#[cfg(unix)]
#[test]
fn symlink_inputs_outputs_ancestors_and_entries_are_rejected() {
    use std::os::unix::fs::symlink;
    let (_temp, root) = canonical_temp();
    let input = root.join("src");
    fs::create_dir(&input).unwrap();
    fs::write(
        input.join("module.qli"),
        "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    symlink(input.join("module.qli"), root.join("alias.qli")).unwrap();
    assert_eq!(
        run(&command_args(&root.join("alias.qli"), &root.join("docs"))).exit_code,
        1
    );
    let generated = render_files(
        &sources("module.qli", "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}"),
        false,
    )
    .unwrap();
    fs::create_dir(root.join("elsewhere")).unwrap();
    symlink(root.join("elsewhere"), root.join("alias")).unwrap();
    assert!(publish(&root.join("alias/docs"), &generated).is_err());
    assert!(publish(&root.join("alias"), &generated).is_err());
    assert!(
        fs::read_dir(root.join("elsewhere"))
            .unwrap()
            .next()
            .is_none()
    );
    let output = root.join("docs");
    publish(&output, &generated).unwrap();
    symlink(input.join("module.qli"), output.join("extra.md")).unwrap();
    assert!(publish(&output, &generated).is_err());
}

#[test]
fn cli_is_one_json_envelope_with_portable_relative_paths_and_usage_exit_codes() {
    let (_temp, root) = canonical_temp();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("src/module.qli"),
        "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_qlidoc"))
        .current_dir(&root)
        .args(["src", "--output=docs", "--format=json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["format"], "qlidoc.result");
    assert_eq!(envelope["version"], 1);
    assert_eq!(envelope["command"], "doc");
    assert_eq!(envelope["result"]["artifact_path"], "docs");
    assert_eq!(envelope["result"].as_object().unwrap().len(), 7);
    let version = Command::new(env!("CARGO_BIN_EXE_qlidoc"))
        .args(["--version", "--format=json"])
        .output()
        .unwrap();
    assert!(version.status.success());
    let version: Value = serde_json::from_slice(&version.stdout).unwrap();
    assert_eq!(version["result"]["version"], "0.1.1");
    assert_eq!(version["result"]["qleisli_version"], "0.2.1");
    for args in [
        vec!["--format=json"],
        vec!["", "--format=json"],
        vec!["src", "--output=", "--format=json"],
        vec![
            "src",
            "--document-private-items",
            "--document-private-items",
            "--format=json",
        ],
        vec!["--help", "src", "--format=json"],
        vec!["src", "--format=json", "--format=json"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_qlidoc"))
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["diagnostics"][0]["category"],
            "usage"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let output = Command::new(env!("CARGO_BIN_EXE_qlidoc"))
            .arg(OsString::from_vec(vec![0xff]))
            .arg("--format=json")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["diagnostics"][0]["category"],
            "usage"
        );
    }
}

#[test]
fn cli_empty_root_and_default_identity_output_are_supported() {
    let (_temp, root) = canonical_temp();
    fs::create_dir(root.join("src")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_qlidoc"))
        .current_dir(&root)
        .args(["src", "--format=json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = &envelope["result"];
    assert_eq!(result["qleisli_check"]["reason"], "no_sources");
    assert_eq!(result["source_count"], 0);
    assert_eq!(result["files"].as_array().unwrap().len(), 1);
    let expected = format!(
        "target/qlidoc/{}/{}/public",
        result["source_id"]
            .as_str()
            .unwrap()
            .trim_start_matches("sha256:"),
        result["tool"]["executable_sha256"]
            .as_str()
            .unwrap()
            .trim_start_matches("sha256:")
    );
    assert_eq!(result["artifact_path"], expected);
    assert!(root.join(expected).join("index.md").exists());
}
