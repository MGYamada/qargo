use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use qargo_tools::snapshot::{Files, collect_tree, digest_files, digest_path, materialize};
use serde_json::{Value, json};

const TOOLS: [(&str, &str); 4] = [
    ("qargo", env!("CARGO_BIN_EXE_qargo")),
    ("qlippy", env!("CARGO_BIN_EXE_qlippy")),
    ("qlifmt", env!("CARGO_BIN_EXE_qlifmt")),
    ("qlidoc", env!("CARGO_BIN_EXE_qlidoc")),
];
const QRATES: [&str; 3] = ["qlippy", "qlifmt", "qlidoc"];

fn response(output: &Output) -> Value {
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    assert_eq!(
        output.stdout.iter().filter(|&&byte| byte == b'\n').count(),
        1
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run(binary: &str, args: &[&str]) -> (Output, Value) {
    let output = Command::new(binary)
        .args(args)
        .arg("--format=json")
        .output()
        .unwrap();
    let report = response(&output);
    (output, report)
}

fn qrate_files(name: &str) -> Files {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("qrates")
        .join(name);
    let mut files = Files::new();
    files.insert(
        "Qargo.toml".into(),
        fs::read(root.join("Qargo.toml")).unwrap(),
    );
    for category in ["src", "tests", "docs"] {
        for (path, bytes) in collect_tree(&root.join(category), None).unwrap() {
            files.insert(format!("{category}/{path}"), bytes);
        }
    }
    files
}

#[test]
fn all_four_executables_bind_version_to_the_actual_engine() {
    for (name, binary) in TOOLS {
        let (output, report) = run(binary, &["--version"]);
        assert!(output.status.success(), "{name}: {report}");
        assert_eq!(report["format"], format!("{name}.result"));
        assert_eq!(report["version"], 1);
        assert_eq!(report["command"], "version");
        assert_eq!(report["outcome"], "ok");
        assert_eq!(report["diagnostics"], json!([]));
        let tool = report["result"].get("tool").unwrap_or(&report["result"]);
        assert_eq!(tool["name"], name);
        assert_eq!(tool["version"], "0.1.1");
        assert_eq!(tool["qleisli_version"], "0.2.1");
        assert_eq!(tool["profile"], "finite-v0");
        assert_eq!(
            tool["executable_sha256"],
            digest_path(Path::new(binary)).unwrap()
        );
    }
}

#[test]
fn standard_qrates_capture_all_declared_sources_and_keep_developer_config_outside() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in QRATES {
        let files = qrate_files(name);
        let manifest: toml::Value =
            toml::from_str(std::str::from_utf8(&files["Qargo.toml"]).unwrap()).unwrap();
        assert_eq!(manifest["schema-version"].as_integer(), Some(1));
        assert_eq!(manifest["qrate"]["name"].as_str(), Some(name));
        assert_eq!(manifest["qrate"]["version"].as_str(), Some("0.1.1"));
        assert!(files.contains_key("src/lib.rs"), "{name}");
        assert!(files.contains_key(&format!("src/bin/{name}.rs")), "{name}");
        assert!(
            files
                .keys()
                .any(|path| path.starts_with("tests/") && path.ends_with(".rs"))
        );
        assert!(files.contains_key("src/smoke.qli"), "{name}");
        assert!(!files.keys().any(|path| path.ends_with(".qlt")));
        assert!(
            !repository
                .join("qrates")
                .join(name)
                .join("Cargo.toml")
                .exists()
        );
        assert!(
            repository
                .join("rust")
                .join(name)
                .join("Cargo.toml")
                .is_file()
        );

        let sources: Files = files
            .iter()
            .filter_map(|(path, bytes)| {
                path.strip_prefix("src/")
                    .filter(|path| path.ends_with(".qli"))
                    .map(|path| (path.into(), bytes.clone()))
            })
            .collect();
        let root = materialize(&files).unwrap();
        let manifest = format!(
            "--manifest-path={}",
            root.path().join("Qargo.toml").display()
        );
        let (output, report) = run(env!("CARGO_BIN_EXE_qargo"), &["build", &manifest]);
        assert!(output.status.success(), "{name}: {report}");
        assert_eq!(
            report["result"]["input_id"],
            digest_files("qargo.qrate.v1", &files)
        );
        assert_eq!(
            report["result"]["source_id"],
            digest_files("qleisli.source.v1", &sources)
        );
        assert_eq!(report["result"]["source_count"], sources.len());
        assert_eq!(report["result"]["qleisli_check"]["status"], "passed");
        let artifact = root
            .path()
            .join(report["result"]["artifact_path"].as_str().unwrap());
        assert_eq!(
            collect_tree(&artifact.join("snapshot"), None).unwrap(),
            files
        );
        let index: Value =
            serde_json::from_slice(&fs::read(artifact.join("module-index.json")).unwrap()).unwrap();
        assert_eq!(
            index,
            json!([{"name":"smoke","path":"smoke.qli","declarations":[{"name":"identity","kind":"unitary"}]}])
        );

        let changed_file = files
            .keys()
            .find(|path| path.starts_with("src/") && path.ends_with(".rs"))
            .unwrap();
        let mut changed = files[changed_file].clone();
        changed.extend_from_slice(b"\n// Qrate identity includes Rust engine inputs.\n");
        fs::write(root.path().join(changed_file), changed).unwrap();
        let (output, updated) = run(env!("CARGO_BIN_EXE_qargo"), &["check", &manifest]);
        assert!(output.status.success(), "{name}: {updated}");
        assert_ne!(updated["result"]["input_id"], report["result"]["input_id"]);
        assert_eq!(
            updated["result"]["source_id"],
            report["result"]["source_id"]
        );
        assert_eq!(
            collect_tree(&artifact.join("snapshot"), None).unwrap(),
            files
        );
    }
}

#[cfg(unix)]
#[test]
fn bundled_operations_never_start_cargo_or_rustdoc() {
    use std::os::unix::fs::PermissionsExt;

    let traps = tempfile::tempdir().unwrap();
    let cargo_marker = traps.path().join("cargo-started");
    let rustdoc_marker = traps.path().join("rustdoc-started");
    for (name, variable) in [
        ("cargo", "QARGO_CARGO_TRAP"),
        ("rustdoc", "QARGO_RUSTDOC_TRAP"),
    ] {
        let path = traps.path().join(name);
        fs::write(
            &path,
            format!("#!/bin/sh\nprintf started > \"${variable}\"\nexit 99\n"),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let invoke = |binary: &str, args: &[&str]| {
        let output = Command::new(binary)
            .args(args)
            .arg("--format=json")
            .env("PATH", traps.path())
            .env("CARGO", traps.path().join("cargo"))
            .env("RUSTDOC", traps.path().join("rustdoc"))
            .env("QARGO_CARGO_TRAP", &cargo_marker)
            .env("QARGO_RUSTDOC_TRAP", &rustdoc_marker)
            .output()
            .unwrap();
        let report = response(&output);
        assert!(!cargo_marker.exists(), "{binary} {args:?} invoked Cargo");
        assert!(
            !rustdoc_marker.exists(),
            "{binary} {args:?} invoked Rustdoc"
        );
        (output, report)
    };

    for name in QRATES {
        let root = materialize(&qrate_files(name)).unwrap();
        let manifest = format!(
            "--manifest-path={}",
            root.path().join("Qargo.toml").display()
        );
        for command in ["check", "build", "lint", "fmt", "doc", "test"] {
            let (output, report) = invoke(env!("CARGO_BIN_EXE_qargo"), &[command, &manifest]);
            if command == "test" {
                assert_eq!(output.status.code(), Some(1));
                assert_eq!(report["diagnostics"][0]["id"], "backend_unavailable");
            } else {
                assert!(output.status.success(), "{name} {command}: {report}");
                assert_eq!(report["result"]["source_count"], 1);
            }
        }
        let source = root.path().join("src");
        let source = source.to_str().unwrap();
        let output_dir = tempfile::tempdir().unwrap();
        let destination = format!(
            "--output={}",
            fs::canonicalize(output_dir.path())
                .unwrap()
                .join("docs")
                .display()
        );
        for (binary, args) in [
            (env!("CARGO_BIN_EXE_qlippy"), vec![source]),
            (env!("CARGO_BIN_EXE_qlifmt"), vec![source, "--check"]),
            (env!("CARGO_BIN_EXE_qlidoc"), vec![source, &destination]),
        ] {
            let (output, report) = invoke(binary, &args);
            assert!(output.status.success(), "{name}: {report}");
        }
    }
}
