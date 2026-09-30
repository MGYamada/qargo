use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const MANIFEST: &str = "schema-version = 1\n[qrate]\nname = \"sample\"\nversion = \"0.1.0\"\n[source]\nroot = \"src\"\n[tests]\nroot = \"tests\"\n[docs]\nroot = \"docs\"\n";

fn qrate() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for directory in ["src", "tests", "docs"] {
        fs::create_dir(root.path().join(directory)).unwrap();
    }
    fs::write(root.path().join("Qargo.toml"), MANIFEST).unwrap();
    root
}

fn json(output: &Output) -> Value {
    assert!(
        output.stderr.is_empty(),
        "Unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"));
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn qargo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qargo"))
        .args(args)
        .arg("--format=json")
        .output()
        .unwrap()
}

#[test]
fn external_lint_cli_preserves_warnings_denial_and_source_locations() {
    let root = tempfile::tempdir().unwrap();
    let source = "// 日本語 🦀\r\nuse std::quantum::h;\r\nuse std::quantum::x;\r\nunitary fn once(q:Q<Bit>)->Q<Bit>{repeat_static(1,h,q)}\r\n";
    fs::write(root.path().join("library.qli"), source).unwrap();
    let path = root.path().to_str().unwrap();
    let tool = format!("--qlippy={}", env!("CARGO_BIN_EXE_qlippy"));
    for denied in [false, true] {
        let mut args = vec!["lint", path, &tool];
        if denied {
            args.push("--deny-warnings");
        }
        let output = qargo(&args);
        assert_eq!(output.status.code(), Some(if denied { 1 } else { 0 }));
        let report = json(&output);
        assert_eq!(report["format"], "qargo.result");
        assert_eq!(report["outcome"], if denied { "error" } else { "ok" });
        assert_eq!(report["result"]["source_count"], 1);
        assert_eq!(report["result"]["qleisli_check"]["status"], "passed");
        assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
        assert_eq!(report["diagnostics"][0]["id"], "unused_import");
        assert_eq!(report["diagnostics"][0]["primary"]["path"], "library.qli");
        assert_eq!(report["diagnostics"][0]["primary"]["line"], 3);
        let start = report["diagnostics"][1]["primary"]["start"]
            .as_u64()
            .unwrap() as usize;
        let end = report["diagnostics"][1]["primary"]["end"].as_u64().unwrap() as usize;
        assert_eq!(&source[start..end], "repeat_static(1,h,q)");
        assert!(report["diagnostics"][1]["suggestion"].is_string());
    }
    assert_eq!(
        fs::read_to_string(root.path().join("library.qli")).unwrap(),
        source
    );
}

#[test]
fn actual_compiler_rejection_survives_the_child_transport() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("library.qli"),
        "unitary fn bad(q:Q<Bit>)->Q<Bit>{let moved=q; q}",
    )
    .unwrap();
    let output = qargo(&[
        "lint",
        root.path().to_str().unwrap(),
        &format!("--qlippy={}", env!("CARGO_BIN_EXE_qlippy")),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output);
    assert_eq!(report["diagnostics"][0]["category"], "compiler");
    assert_eq!(report["diagnostics"][0]["id"], "ownership");
    assert_eq!(report["diagnostics"][0]["primary"]["path"], "library.qli");
    assert_eq!(report["result"]["qleisli_check"]["status"], "failed");
    assert_eq!(report["result"]["steps"][2]["status"], "not_run");
    assert_eq!(report["result"]["steps"][2]["reason"], "compiler_error");
    assert_eq!(report["result"]["tool"]["name"], "qlippy");
    assert_eq!(report["result"]["orchestrator"]["name"], "qargo");
}

#[test]
fn empty_qrate_builds_a_reusable_package_snapshot() {
    let root = qrate();
    let manifest = format!(
        "--manifest-path={}",
        root.path().join("Qargo.toml").display()
    );
    let first = qargo(&["build", &manifest]);
    assert_eq!(first.status.code(), Some(0));
    let report = json(&first);
    assert_eq!(report["result"]["source_count"], 0);
    assert_eq!(report["result"]["qleisli_check"]["status"], "not_run");
    let directory = root
        .path()
        .join(report["result"]["artifact_path"].as_str().unwrap());
    for part in ["src", "tests", "docs"] {
        assert!(
            directory.join("snapshot").join(part).is_dir(),
            "Missing empty declared root {part}"
        );
    }
    assert_eq!(
        fs::read_to_string(directory.join("snapshot/Qargo.toml")).unwrap(),
        MANIFEST
    );
    let second = qargo(&["build", &manifest]);
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(first.stdout, second.stdout);
    let snapshot_manifest = format!(
        "--manifest-path={}",
        directory.join("snapshot/Qargo.toml").display()
    );
    let reused = json(&qargo(&["check", &snapshot_manifest]));
    assert_eq!(reused["result"]["input_id"], report["result"]["input_id"]);
}

#[test]
fn qlippy_qrate_checks_and_lints_its_trivial_language_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("qrates/qlippy");
    let sources = qargo_tools::snapshot::collect_tree(&root.join("src"), Some("qli")).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources["smoke.qli"],
        include_bytes!("../qrates/qlippy/src/smoke.qli")
    );
    assert!(
        qargo_tools::snapshot::collect_tree(&root.join("tests"), Some("qlt"))
            .unwrap()
            .is_empty()
    );
    let manifest = format!("--manifest-path={}", root.join("Qargo.toml").display());
    let output = qargo(&["check", &manifest]);
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output);
    assert_eq!(report["result"]["source_count"], 1);
    assert_eq!(report["result"]["qleisli_check"]["status"], "passed");
    assert!(report["result"].get("verified").is_none());
    let linted = qargo(&["lint", &manifest, "--deny-warnings"]);
    assert_eq!(linted.status.code(), Some(0));
    let linted = json(&linted);
    assert_eq!(linted["result"]["source_count"], 1);
    assert_eq!(linted["result"]["qleisli_check"]["status"], "passed");
    assert_eq!(linted["result"]["input_id"], report["result"]["input_id"]);
    assert_eq!(linted["result"]["source_id"], report["result"]["source_id"]);
    assert!(linted["diagnostics"].as_array().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn every_qargo_operation_leaves_a_cargo_path_trap_untouched() {
    use std::os::unix::fs::PermissionsExt;
    let root = qrate();
    let bin = tempfile::tempdir().unwrap();
    let trap = bin.path().join("cargo");
    let marker = bin.path().join("cargo-was-started");
    fs::write(
        &trap,
        "#!/bin/sh\nprintf started > \"$QARGO_CARGO_TRAP\"\nexit 99\n",
    )
    .unwrap();
    fs::set_permissions(&trap, fs::Permissions::from_mode(0o755)).unwrap();
    let manifest = format!(
        "--manifest-path={}",
        root.path().join("Qargo.toml").display()
    );
    for command in ["check", "build", "lint", "test", "doc"] {
        let output = Command::new(env!("CARGO_BIN_EXE_qargo"))
            .args([command, &manifest, "--format=json"])
            .env("PATH", bin.path())
            .env("CARGO", &trap)
            .env("QARGO_CARGO_TRAP", &marker)
            .output()
            .unwrap();
        let report = json(&output);
        if command == "test" || command == "doc" {
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(report["diagnostics"][0]["id"], "backend_unavailable");
        } else {
            assert_eq!(output.status.code(), Some(0), "{command}: {report}");
            assert_eq!(report["result"]["source_count"], 0);
        }
        assert!(!marker.exists(), "{command} invoked development Cargo");
    }
}

#[test]
fn qlippy_rust_and_language_sources_belong_to_the_qrate_identity_and_snapshot() {
    use qargo_tools::snapshot::{Files, collect_tree, materialize};
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("qrates/qlippy");
    let mut files = Files::new();
    files.insert(
        "Qargo.toml".into(),
        fs::read(original.join("Qargo.toml")).unwrap(),
    );
    for root in ["src", "tests", "docs"] {
        for (path, bytes) in collect_tree(&original.join(root), None).unwrap() {
            files.insert(format!("{root}/{path}"), bytes);
        }
    }
    for rust_file in [
        "src/lib.rs",
        "src/adapter.rs",
        "src/qlippy.rs",
        "src/report.rs",
        "src/snapshot.rs",
        "src/bin/qlippy.rs",
        "tests/engine.rs",
        "tests/snapshot.rs",
    ] {
        assert!(
            files.contains_key(rust_file),
            "qlippy Rust source missing from qrate: {rust_file}"
        );
    }
    let root = materialize(&files).unwrap();
    let manifest = format!(
        "--manifest-path={}",
        root.path().join("Qargo.toml").display()
    );
    let built = qargo(&["build", &manifest]);
    assert_eq!(built.status.code(), Some(0));
    let report = json(&built);
    assert_eq!(report["result"]["source_count"], 1);
    assert_eq!(report["result"]["qleisli_check"]["status"], "passed");
    let artifact = root
        .path()
        .join(report["result"]["artifact_path"].as_str().unwrap());
    let index: Value =
        serde_json::from_slice(&fs::read(artifact.join("module-index.json")).unwrap()).unwrap();
    assert_eq!(
        index,
        serde_json::json!([{"name":"smoke", "path":"smoke.qli", "declarations":[{"name":"identity", "kind":"unitary"}]}])
    );
    let snapshot = artifact.join("snapshot");
    assert_eq!(collect_tree(&snapshot, None).unwrap(), files);
    let engine = root.path().join("src/qlippy.rs");
    let mut changed = files["src/qlippy.rs"].clone();
    changed.extend_from_slice(b"\n// Changed qrate Rust source.\n");
    fs::write(&engine, changed).unwrap();
    let checked = qargo(&["check", &manifest]);
    assert_eq!(checked.status.code(), Some(0));
    let checked = json(&checked);
    assert_ne!(checked["result"]["input_id"], report["result"]["input_id"]);
    assert_eq!(
        checked["result"]["source_id"],
        report["result"]["source_id"]
    );
    assert_eq!(
        fs::read(snapshot.join("src/qlippy.rs")).unwrap(),
        files["src/qlippy.rs"]
    );
    let mut changed = files["src/smoke.qli"].clone();
    changed.extend_from_slice(b"\n// Changed Qleisli source.\n");
    fs::write(root.path().join("src/smoke.qli"), changed).unwrap();
    let source_changed = qargo(&["check", &manifest]);
    assert_eq!(source_changed.status.code(), Some(0));
    let source_changed = json(&source_changed);
    assert_ne!(
        source_changed["result"]["input_id"],
        checked["result"]["input_id"]
    );
    assert_ne!(
        source_changed["result"]["source_id"],
        checked["result"]["source_id"]
    );
    assert_eq!(
        fs::read(snapshot.join("src/smoke.qli")).unwrap(),
        files["src/smoke.qli"]
    );
}
