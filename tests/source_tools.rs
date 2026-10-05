use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use qargo_tools::qargo;
use qargo_tools::report::Report;
use qargo_tools::snapshot::{FrozenSources, digest_files, digest_path};
use serde_json::{Value, json};

const SOURCE: &str = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";

fn qrate() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for directory in ["src", "tests", "docs"] {
        fs::create_dir(root.path().join(directory)).unwrap();
    }
    fs::write(root.path().join("Qargo.toml"), "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"0.1.8\"\nedition = \"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
    fs::write(root.path().join("src/module.qli"), SOURCE).unwrap();
    root
}

fn run(root: &Path, command: &str, extra: &[String]) -> Report {
    let tool = match command {
        "fmt" => format!("--qlifmt={}", env!("CARGO_BIN_EXE_qlifmt")),
        "doc" => format!("--qlidoc={}", env!("CARGO_BIN_EXE_qlidoc")),
        _ => String::new(),
    };
    let mut args = vec![
        OsString::from(command),
        OsString::from(format!(
            "--manifest-path={}",
            root.join("Qargo.toml").display()
        )),
    ];
    if !extra
        .iter()
        .any(|value| value.starts_with("--qlifmt=") || value.starts_with("--qlidoc="))
        && !tool.is_empty()
    {
        args.push(tool.into());
    }
    args.extend(extra.iter().map(OsString::from));
    qargo::run(&args)
}

fn result(report: &Report) -> &Value {
    report.envelope.result.as_ref().unwrap()
}

#[test]
fn formatting_check_and_apply_bind_original_inputs_and_preserve_checker_behavior() {
    let root = qrate();
    let before = run(root.path(), "check", &[]);
    let check = run(root.path(), "fmt", &["--check".into()]);
    assert_eq!(check.exit_code, 1, "{:?}", check.envelope);
    assert_eq!(check.envelope.diagnostics[0].id, "formatting_required");
    assert_eq!(
        fs::read_to_string(root.path().join("src/module.qli")).unwrap(),
        SOURCE
    );
    assert_eq!(result(&check)["input_id"], result(&before)["input_id"]);
    assert_eq!(result(&check)["updated_files"], json!([]));
    assert!(!result(&check)["diff"].as_str().unwrap().is_empty());
    let applied = run(root.path(), "fmt", &[]);
    assert_eq!(applied.exit_code, 0, "{:?}", applied.envelope);
    assert_eq!(result(&applied)["updated_files"], json!(["module.qli"]));
    assert_eq!(
        result(&applied)["qleisli_check"],
        json!({"status":"not_run","reason":"syntax_only"})
    );
    let after = run(root.path(), "check", &[]);
    assert_eq!(after.exit_code, 0);
    assert_eq!(result(&after)["qleisli_check"]["status"], "passed");
    assert_ne!(result(&after)["input_id"], result(&before)["input_id"]);
    assert_eq!(run(root.path(), "fmt", &["--check".into()]).exit_code, 0);
}

#[test]
fn syntax_errors_prevent_any_format_writes_or_doc_publication() {
    let root = qrate();
    fs::write(
        root.path().join("src/zbroken.qli"),
        "pub unitary fn broken(",
    )
    .unwrap();
    for command in ["fmt", "doc"] {
        let report = run(root.path(), command, &[]);
        assert_eq!(report.exit_code, 1, "{:?}", report.envelope);
        let diagnostic = &report.envelope.diagnostics[0];
        assert_eq!(diagnostic.category, "compiler");
        assert_eq!(diagnostic.primary.as_ref().unwrap().path, "src/zbroken.qli");
        assert_eq!(result(&report)["qleisli_check"]["status"], "not_run");
    }
    assert_eq!(
        fs::read_to_string(root.path().join("src/module.qli")).unwrap(),
        SOURCE
    );
    assert!(!root.path().join("target/qlidoc").exists());
}

#[test]
fn documentation_is_syntax_only_private_opt_in_and_conflicts_are_preserved() {
    let root = qrate();
    fs::write(root.path().join("src/module.qli"), "//! Module prose.\n/// Public prose.\npub unitary fn visible(q:Q<Bit>)->Q<Bit>{missing}\n/// Private prose.\nunitary fn hidden(q:Q<Bit>)->Q<Bit>{q}\n").unwrap();
    fs::write(root.path().join("docs/README.md"), "Handwritten guide").unwrap();
    assert_eq!(run(root.path(), "check", &[]).exit_code, 1);
    let report = run(root.path(), "doc", &[]);
    assert_eq!(report.exit_code, 0, "{:?}", report.envelope);
    let output = root
        .path()
        .join(result(&report)["artifact_path"].as_str().unwrap());
    let markdown = fs::read_to_string(output.join("modules/module.md")).unwrap();
    assert!(markdown.contains("visible") && markdown.contains("Public prose."));
    assert!(!markdown.contains("hidden") && !markdown.contains("Private prose."));
    assert_eq!(run(root.path(), "doc", &[]).envelope, report.envelope);
    let private = run(root.path(), "doc", &["--document-private-items".into()]);
    assert_eq!(private.exit_code, 0);
    let all = root
        .path()
        .join(result(&private)["artifact_path"].as_str().unwrap());
    assert_ne!(all, output);
    assert!(
        fs::read_to_string(all.join("modules/module.md"))
            .unwrap()
            .contains("hidden")
    );
    fs::write(output.join("index.md"), "Conflicting user content").unwrap();
    let conflict = run(root.path(), "doc", &[]);
    assert_eq!(conflict.exit_code, 1);
    assert_eq!(conflict.envelope.diagnostics[0].id, "artifact_mismatch");
    assert!(result(&conflict)["artifact_path"].is_null());
    assert_eq!(
        fs::read_to_string(output.join("index.md")).unwrap(),
        "Conflicting user content"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("docs/README.md")).unwrap(),
        "Handwritten guide"
    );
}

#[test]
fn failed_document_output_creation_has_no_artifact_path() {
    let root = qrate();
    // An ordinary file cannot serve as the publication parent.
    fs::write(root.path().join("target"), "Keep this file").unwrap();
    let failed = run(root.path(), "doc", &[]);
    assert_eq!(failed.exit_code, 1, "{:?}", failed.envelope);
    assert_eq!(failed.envelope.diagnostics[0].id, "doc_output");
    assert!(result(&failed)["artifact_path"].is_null());
    assert_eq!(
        fs::read_to_string(root.path().join("target")).unwrap(),
        "Keep this file"
    );
}

#[test]
fn source_tool_options_are_closed_and_command_specific() {
    for args in [
        vec!["check", "--check"],
        vec!["lint", "--qlifmt=x"],
        vec!["fmt", "--qlidoc=x"],
        vec!["doc", "--check"],
        vec!["fmt", "--document-private-items"],
        vec![
            "doc",
            "--document-private-items",
            "--document-private-items",
        ],
        vec!["fmt", "--check", "--check"],
        vec!["doc", "src"],
        vec!["fmt", "--qlifmt="],
        vec!["doc", "--qlidoc="],
    ] {
        let report = qargo::run(&args.iter().map(OsString::from).collect::<Vec<_>>());
        assert_eq!(report.exit_code, 2, "{args:?}: {:?}", report.envelope);
    }
    let root = qrate();
    for (command, flag) in [("fmt", "qlifmt"), ("doc", "qlidoc")] {
        let report = run(
            root.path(),
            command,
            &[format!("--{flag}=/missing/source-tool")],
        );
        assert_eq!(report.exit_code, 1);
        assert_eq!(report.envelope.diagnostics[0].id, "tool_missing");
    }
}

#[cfg(unix)]
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn script(root: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("selected-tool");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn response(tool: &Path, sources: &FrozenSources, command: &str) -> Value {
    let name = if command == "fmt" { "qlifmt" } else { "qlidoc" };
    let mut result = json!({"source_count":sources.count(), "source_id":sources.source_id(),
        "qleisli_check":{"status":"not_run","reason":"syntax_only"},
        "tool":{"name":name,"version":qargo_tools::VERSION,"executable_sha256":digest_path(tool).unwrap(),"qleisli_version":"0.2.1","profile":"finite-v0"}});
    if command == "fmt" {
        result["formatted_source_id"] = json!(sources.source_id());
        result["changed_files"] = json!([]);
        result["updated_files"] = json!([]);
        result["check"] = json!(false);
        result["diff"] = json!("");
    } else {
        result["document_private_items"] = json!(false);
        result["artifact_path"] = json!("/forged/output");
        result["files"] = json!([]);
    }
    json!({"format":format!("{name}.result"),"version":1,"command":command,"outcome":"ok","diagnostics":[],"result":result})
}

#[cfg(unix)]
#[test]
fn closed_child_responses_reject_versions_binding_fields_and_duplicate_keys() {
    let root = qrate();
    let sources = FrozenSources::capture(&root.path().join("src")).unwrap();
    let tools = tempfile::tempdir().unwrap();
    let response_path = tools.path().join("response.json");
    let tool = script(tools.path(), &format!("/bin/cat {}", quote(&response_path)));
    for command in ["fmt", "doc"] {
        let valid = response(&tool, &sources, command);
        let mut variants = Vec::new();
        for (pointer, value) in [
            ("/version", json!(2)),
            ("/result/source_id", json!("sha256:wrong")),
            ("/result/tool/version", json!("0.1.0")),
            ("/result/tool/executable_sha256", json!("sha256:wrong")),
            ("/result/qleisli_check/status", json!("passed")),
            ("/result/tool/profile", json!("unsupported")),
            ("/outcome", json!("error")),
        ] {
            let mut bad = valid.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            variants.push(serde_json::to_vec(&bad).unwrap());
        }
        let mut bad = valid.clone();
        bad["extra"] = json!(true);
        variants.push(serde_json::to_vec(&bad).unwrap());
        let mut bad = valid.clone();
        bad["result"]["extra"] = json!(true);
        variants.push(serde_json::to_vec(&bad).unwrap());
        let mut bad = valid.clone();
        bad["result"]["tool"]["extra"] = json!(true);
        variants.push(serde_json::to_vec(&bad).unwrap());
        variants.push(
            serde_json::to_string(&valid)
                .unwrap()
                .replace("\"version\":1", "\"version\":1,\"version\":1")
                .into_bytes(),
        );
        for bytes in variants {
            fs::write(&response_path, bytes).unwrap();
            let flag = if command == "fmt" { "qlifmt" } else { "qlidoc" };
            let report = run(
                root.path(),
                command,
                &[format!("--{flag}={}", tool.display())],
            );
            assert_eq!(report.exit_code, 1, "{:?}", report.envelope);
            assert_eq!(report.envelope.diagnostics[0].id, "invalid_tool_response");
        }
    }
    assert_eq!(
        fs::read_to_string(root.path().join("src/module.qli")).unwrap(),
        SOURCE
    );
}

#[cfg(unix)]
#[test]
fn formatter_cannot_change_tokens_or_add_files_to_the_user_root() {
    let root = qrate();
    let sources = FrozenSources::capture(&root.path().join("src")).unwrap();
    let tools = tempfile::tempdir().unwrap();
    let response_path = tools.path().join("response.json");
    for body in [
        "printf 'pub unitary fn changed(q:Q<Bit>)->Q<Bit>{q}' > \"$1/module.qli\"",
        "printf injected > \"$1/extra.txt\"",
    ] {
        let tool = script(
            tools.path(),
            &format!("{body}\n/bin/cat {}", quote(&response_path)),
        );
        fs::write(
            &response_path,
            serde_json::to_vec(&response(&tool, &sources, "fmt")).unwrap(),
        )
        .unwrap();
        let report = run(
            root.path(),
            "fmt",
            &[format!("--qlifmt={}", tool.display())],
        );
        assert_eq!(report.exit_code, 1);
        assert_eq!(report.envelope.diagnostics[0].id, "invalid_tool_response");
        assert_eq!(
            fs::read_to_string(root.path().join("src/module.qli")).unwrap(),
            SOURCE
        );
        assert!(!root.path().join("src/extra.txt").exists());
    }
}

#[cfg(unix)]
#[test]
fn concurrent_source_changes_are_preserved_instead_of_overwritten() {
    let root = qrate();
    let sources = FrozenSources::capture(&root.path().join("src")).unwrap();
    let formatted = qargo_tools::qlifmt_engine::format_files(sources.files()).unwrap();
    let tools = tempfile::tempdir().unwrap();
    let candidate = tools.path().join("candidate.qli");
    let response_path = tools.path().join("response.json");
    fs::write(&candidate, &formatted["module.qli"]).unwrap();
    let user_file = root.path().join("src/module.qli");
    let tool = script(
        tools.path(),
        &format!(
            "/bin/cp {} \"$1/module.qli\"\nprintf 'concurrent edit' > {}\n/bin/cat {}",
            quote(&candidate),
            quote(&user_file),
            quote(&response_path)
        ),
    );
    let mut valid = response(&tool, &sources, "fmt");
    valid["result"]["formatted_source_id"] = json!(digest_files("qleisli.source.v1", &formatted));
    valid["result"]["changed_files"] = json!(["module.qli"]);
    valid["result"]["updated_files"] = json!(["module.qli"]);
    fs::write(&response_path, serde_json::to_vec(&valid).unwrap()).unwrap();
    let report = run(
        root.path(),
        "fmt",
        &[format!("--qlifmt={}", tool.display())],
    );
    assert_eq!(report.exit_code, 1, "{:?}", report.envelope);
    assert_eq!(report.envelope.diagnostics[0].id, "input_changed");
    assert_eq!(result(&report)["updated_files"], json!([]));
    assert_eq!(fs::read_to_string(user_file).unwrap(), "concurrent edit");
}

#[cfg(unix)]
#[test]
fn document_success_without_the_requested_artifacts_is_rejected() {
    let root = qrate();
    let tools = tempfile::tempdir().unwrap();
    let response_path = tools.path().join("response.json");
    let tool = script(tools.path(), &format!("/bin/cat {}", quote(&response_path)));
    let sources = FrozenSources::capture(&root.path().join("src")).unwrap();
    fs::write(
        &response_path,
        serde_json::to_vec(&response(&tool, &sources, "doc")).unwrap(),
    )
    .unwrap();
    let report = run(
        root.path(),
        "doc",
        &[format!("--qlidoc={}", tool.display())],
    );
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.envelope.diagnostics[0].id, "invalid_tool_response");
    assert!(!root.path().join("target/qlidoc").exists());
}

#[cfg(unix)]
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn captured_source_root_and_ancestors_cannot_be_replaced_before_format_effects() {
    for replaced in [
        "standalone-root",
        "standalone-parent",
        "qrate",
        "source",
        "source-parent",
    ] {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("qrate");
        let fixture = qrate();
        copy_tree(fixture.path(), &root);
        fs::create_dir(root.join("inputs")).unwrap();
        fs::rename(root.join("src"), root.join("inputs/src")).unwrap();
        let manifest = fs::read_to_string(root.join("Qargo.toml"))
            .unwrap()
            .replace("root=\"src\"", "root=\"inputs/src\"");
        fs::write(root.join("Qargo.toml"), manifest).unwrap();
        let sources_path = root.join("inputs/src");
        let sources = FrozenSources::capture(&sources_path).unwrap();
        let formatted = qargo_tools::qlifmt_engine::format_files(sources.files()).unwrap();
        let replace = match replaced {
            "qrate" => root.clone(),
            "source-parent" | "standalone-parent" => root.join("inputs"),
            _ => sources_path.clone(),
        };
        let replacement = home.path().join("replacement");
        let old = home.path().join("old");
        copy_tree(&replace, &replacement);
        let tools = tempfile::tempdir().unwrap();
        let candidate = tools.path().join("candidate.qli");
        fs::write(&candidate, &formatted["module.qli"]).unwrap();
        let response_path = tools.path().join("response.json");
        let tool = script(
            tools.path(),
            &format!(
                "/bin/mv {} {} || exit 9\n/bin/mv {} {} || exit 9\n/bin/cp {} \"$1/module.qli\" || exit 9\n/bin/cat {}",
                quote(&replace),
                quote(&old),
                quote(&replacement),
                quote(&replace),
                quote(&candidate),
                quote(&response_path)
            ),
        );
        let mut report = response(&tool, &sources, "fmt");
        report["result"]["formatted_source_id"] =
            json!(digest_files("qleisli.source.v1", &formatted));
        report["result"]["changed_files"] = json!(["module.qli"]);
        report["result"]["updated_files"] = json!(["module.qli"]);
        fs::write(response_path, serde_json::to_vec(&report).unwrap()).unwrap();
        let failed = if replaced.starts_with("standalone") {
            qargo::run(&[
                "fmt".into(),
                sources_path.as_os_str().to_owned(),
                format!("--qlifmt={}", tool.display()).into(),
            ])
        } else {
            run(&root, "fmt", &[format!("--qlifmt={}", tool.display())])
        };
        assert_eq!(failed.exit_code, 1, "{replaced}: {:?}", failed.envelope);
        assert_eq!(
            failed.envelope.diagnostics[0].id, "input",
            "{replaced}: {:?}",
            failed.envelope
        );
        let suffix = sources_path.strip_prefix(&replace).unwrap();
        for source_root in [&sources_path, &old.join(suffix)] {
            assert_eq!(
                fs::read(source_root.join("module.qli")).unwrap(),
                SOURCE.as_bytes()
            );
            assert_eq!(fs::read_dir(source_root).unwrap().count(), 1);
        }
    }
}

#[cfg(unix)]
#[test]
fn replaced_qrate_cannot_receive_validated_document_artifacts() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("qrate");
    let fixture = qrate();
    copy_tree(fixture.path(), &root);
    let replacement = home.path().join("replacement");
    copy_tree(&root, &replacement);
    let old = home.path().join("old");
    let sources = FrozenSources::capture(&root.join("src")).unwrap();
    let rendered = qargo_tools::qlidoc_engine::render_files(sources.files(), false).unwrap();
    let tools = tempfile::tempdir().unwrap();
    let candidate = tools.path().join("candidate");
    qargo_tools::qlidoc_engine::publish(
        &fs::canonicalize(tools.path()).unwrap().join("candidate"),
        &rendered,
    )
    .unwrap();
    let response_path = tools.path().join("response.json");
    let tool = script(
        tools.path(),
        &format!(
            "/bin/mv {} {} || exit 9\n/bin/mv {} {} || exit 9\noutput=${{3#--output=}}\n/bin/cp -R {} \"$output\" || exit 9\n/usr/bin/sed \"s|__OUTPUT__|$output|g\" {}",
            quote(&root),
            quote(&old),
            quote(&replacement),
            quote(&root),
            quote(&candidate),
            quote(&response_path)
        ),
    );
    let mut report = response(&tool, &sources, "doc");
    report["result"]["artifact_path"] = json!("__OUTPUT__");
    report["result"]["files"] = json!(
        rendered
            .keys()
            .map(|path| json!({"path":path,"sha256":digest_path(&candidate.join(path)).unwrap()}))
            .collect::<Vec<_>>()
    );
    fs::write(response_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let failed = run(&root, "doc", &[format!("--qlidoc={}", tool.display())]);
    assert_eq!(failed.exit_code, 1, "{:?}", failed.envelope);
    assert_eq!(failed.envelope.diagnostics[0].id, "doc_output");
    assert!(result(&failed)["artifact_path"].is_null());
    assert!(!root.join("target").exists());
    assert!(!old.join("target").exists());
}
