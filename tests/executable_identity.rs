#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use qargo_tools::snapshot::{Files, digest_files, digest_path};
use serde_json::{Value, json};

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

fn empty_qrate(root: &Path) {
    for name in ["src", "tests", "docs"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    fs::write(root.join("Qargo.toml"), "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"0.1.0\"\nedition=\"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
}

#[test]
fn explicit_sibling_and_path_selection_share_the_same_captured_object_contract() {
    for selection in ["explicit", "sibling", "PATH"] {
        let root = tempfile::tempdir().unwrap();
        empty_qrate(root.path());
        let host = root.path().join("host");
        let tools = if selection == "sibling" {
            host.clone()
        } else {
            root.path().join("tools")
        };
        fs::create_dir(&host).unwrap();
        if tools != host {
            fs::create_dir(&tools).unwrap();
        }
        let qargo = host.join("qargo");
        fs::copy(env!("CARGO_BIN_EXE_qargo"), &qargo).unwrap();
        for (name, binary) in [
            ("qlippy", env!("CARGO_BIN_EXE_qlippy")),
            ("qlifmt", env!("CARGO_BIN_EXE_qlifmt")),
            ("qlidoc", env!("CARGO_BIN_EXE_qlidoc")),
        ] {
            fs::copy(binary, tools.join(name)).unwrap();
            let target = format!("{name}.binary");
            fs::rename(tools.join(name), tools.join(&target)).unwrap();
            symlink(target, tools.join(name)).unwrap();
        }
        for (operation, name) in [("lint", "qlippy"), ("fmt", "qlifmt"), ("doc", "qlidoc")] {
            let mut command = Command::new(&qargo);
            command
                .current_dir(root.path())
                .arg(operation)
                .arg("--format=json");
            if selection == "explicit" {
                command.arg(format!("--{name}={}", tools.join(name).display()));
            }
            if selection == "PATH" {
                command.env("PATH", &tools);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{selection}/{operation}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                report["result"]["tool"]["executable_sha256"],
                digest_path(&tools.join(name)).unwrap()
            );
            assert_eq!(
                report["result"]["orchestrator"]["executable_sha256"],
                digest_path(&qargo).unwrap()
            );
        }
    }
}

#[test]
fn replacing_the_host_path_never_reports_the_replacement_digest() {
    let root = tempfile::tempdir().unwrap();
    let sources = root.path().join("src");
    fs::create_dir(&sources).unwrap();
    let qargo = root.path().join("qargo");
    let original = root.path().join("running-qargo");
    fs::copy(env!("CARGO_BIN_EXE_qargo"), &qargo).unwrap();
    let expected = digest_path(&qargo).unwrap();
    let tool = root.path().join("tool");
    let response = root.path().join("response.json");
    fs::write(
        &tool,
        format!(
            "#!/bin/sh\n/bin/mv {} {}\n/bin/cp /usr/bin/true {}\n/bin/cat {}\n",
            quote(&qargo),
            quote(&original),
            quote(&qargo),
            quote(&response)
        ),
    )
    .unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
    let report = json!({"format":"qlippy.result","version":1,"command":"lint","outcome":"ok","diagnostics":[],"result":{
        "source_count":0,"source_id":digest_files("qleisli.source.v1", &Files::new()),
        "qleisli_check":{"status":"not_run","reason":"no_sources"},
        "tool":{"name":"qlippy","version":qargo_tools::VERSION,"executable_sha256":digest_path(&tool).unwrap(),"qleisli_version":"0.2.1","profile":"finite-v0"}}});
    fs::write(&response, serde_json::to_vec(&report).unwrap()).unwrap();
    let output = Command::new(&qargo)
        .arg("lint")
        .arg(&sources)
        .arg(format!("--qlippy={}", tool.display()))
        .arg("--format=json")
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    #[cfg(target_os = "linux")]
    {
        assert!(output.status.success());
        assert_eq!(
            result["result"]["orchestrator"]["executable_sha256"],
            expected
        );
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(result["diagnostics"][0]["id"], "tool_identity");
        assert!(result["result"].is_null());
    }
    assert_eq!(digest_path(&original).unwrap(), expected);
    assert_ne!(digest_path(&qargo).unwrap(), expected);
}

#[cfg(target_os = "macos")]
#[test]
fn a_rejected_host_identity_prevents_an_otherwise_valid_format_write() {
    let root = tempfile::tempdir().unwrap();
    let sources = root.path().join("src");
    fs::create_dir(&sources).unwrap();
    let original_bytes = b"pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
    fs::write(sources.join("module.qli"), original_bytes).unwrap();
    let inputs = Files::from([("module.qli".into(), original_bytes.to_vec())]);
    let formatted = qargo_tools::qlifmt_engine::format_files(&inputs).unwrap();
    assert_ne!(inputs, formatted);
    let candidate = root.path().join("formatted.qli");
    fs::write(&candidate, &formatted["module.qli"]).unwrap();

    let qargo = root.path().join("qargo");
    fs::copy(env!("CARGO_BIN_EXE_qargo"), &qargo).unwrap();
    let tool = root.path().join("formatter");
    let response = root.path().join("response.json");
    fs::write(&tool, format!(
        "#!/bin/sh\n/bin/cp {} \"$1/module.qli\"\n/bin/mv {} {}\n/bin/cp /usr/bin/true {}\n/bin/cat {}\n",
        quote(&candidate), quote(&qargo), quote(&root.path().join("running-qargo")),
        quote(&qargo), quote(&response),
    )).unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&response, serde_json::to_vec(&json!({
        "format":"qlifmt.result","version":1,"command":"fmt","outcome":"ok","diagnostics":[],
        "result":{
            "source_count":1,"source_id":digest_files("qleisli.source.v1", &inputs),
            "qleisli_check":{"status":"not_run","reason":"syntax_only"},
            "tool":{"name":"qlifmt","version":qargo_tools::VERSION,
                "executable_sha256":digest_path(&tool).unwrap(),"qleisli_version":"0.2.1","profile":"finite-v0"},
            "formatted_source_id":digest_files("qleisli.source.v1", &formatted),
            "changed_files":["module.qli"],"updated_files":["module.qli"],"check":false,"diff":""
        }
    })).unwrap()).unwrap();
    let output = Command::new(qargo)
        .arg("fmt")
        .arg(&sources)
        .arg(format!("--qlifmt={}", tool.display()))
        .arg("--format=json")
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["diagnostics"][0]["id"], "tool_identity");
    assert!(report["result"].is_null());
    assert_eq!(
        fs::read(sources.join("module.qli")).unwrap(),
        original_bytes
    );
}
