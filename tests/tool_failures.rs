#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use qargo_tools::qargo;
use qargo_tools::snapshot::{FrozenSources, digest_path};
use serde_json::{Value, json};

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

fn fixture(command: &str, dynamic_paths: bool) -> (tempfile::TempDir, Value) {
    let root = tempfile::tempdir().unwrap();
    let sources = root.path().join("sources");
    fs::create_dir(&sources).unwrap();
    fs::write(
        sources.join("module.qli"),
        "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    let frozen = FrozenSources::capture(&sources).unwrap();
    let output = root.path().join("response.json");
    let script = root.path().join("tool");
    let body = if dynamic_paths {
        let substitutions = if command == "doc" {
            "s|__SOURCE__|$1|g;s|__OUTPUT__|${3#--output=}|g"
        } else {
            "s|__SOURCE__|$1|g"
        };
        format!("/usr/bin/sed \"{substitutions}\" {}", quote(&output))
    } else {
        format!("/bin/cat {}", quote(&output))
    };
    fs::write(&script, format!("#!/bin/sh\n{body}\nexit 1\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let name = if command == "fmt" { "qlifmt" } else { "qlidoc" };
    let mut result = json!({"source_count":1,"source_id":frozen.source_id(),
        "qleisli_check":{"status":"not_run","reason":"syntax_only"},
        "tool":{"name":name,"version":qargo_tools::VERSION,"executable_sha256":digest_path(&script).unwrap(),"qleisli_version":"0.2.1","profile":"finite-v0"}});
    if command == "fmt" {
        result["formatted_source_id"] = json!("unverified child identity");
        result["changed_files"] = json!([]);
        result["updated_files"] = json!([]);
        result["check"] = json!(false);
        result["diff"] = json!("");
    } else {
        result["document_private_items"] = json!(false);
        result["artifact_path"] = Value::Null;
        result["files"] = json!([]);
    }
    let report = json!({"format":format!("{name}.result"),"version":1,"command":command,"outcome":"error",
        "diagnostics":[{"id":"tool_failure","category":"tool","severity":"error","primary":null,
            "message":"Cannot process __SOURCE__ __OUTPUT__","suggestion":"Inspect __SOURCE__ __OUTPUT__"}],"result":result});
    (root, report)
}

fn run(root: &Path, command: &str, report: &Value) -> qargo_tools::report::Report {
    fs::write(
        root.join("response.json"),
        serde_json::to_vec(report).unwrap(),
    )
    .unwrap();
    let name = if command == "fmt" { "qlifmt" } else { "qlidoc" };
    let mut args = vec![
        OsString::from(command),
        OsString::from(format!("--{name}={}", root.join("tool").display())),
    ];
    if command == "fmt" {
        args.push(root.join("sources").into_os_string());
    } else {
        for directory in ["tests", "docs"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(root.join("Qargo.toml"), "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"0.1.8\"\nedition = \"2026\"\n[source]\nroot=\"sources\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
        args.push(format!("--manifest-path={}", root.join("Qargo.toml").display()).into());
    }
    qargo::run(&args)
}

#[test]
fn failed_document_transport_still_rejects_unsafe_paths_and_invalid_digests() {
    for (path, digest) in [
        (
            "/private/qargo-doc/secret",
            format!("sha256:{}", "0".repeat(64)),
        ),
        ("../outside.md", format!("sha256:{}", "0".repeat(64))),
        ("index.md", "wrong".into()),
    ] {
        let (root, mut report) = fixture("doc", false);
        report["result"]["files"] = json!([{"path":path,"sha256":digest}]);
        let result = run(root.path(), "doc", &report);
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.envelope.diagnostics[0].id, "invalid_tool_response");
        assert!(!root.path().join("target").exists());
    }
}

#[test]
fn failed_children_cannot_publish_unverified_format_or_document_claims() {
    for command in ["fmt", "doc"] {
        let (root, mut report) = fixture(command, false);
        let before = fs::read(root.path().join("sources/module.qli")).unwrap();
        if command == "fmt" {
            report["result"]["changed_files"] = json!(["module.qli"]);
            report["result"]["updated_files"] = json!(["module.qli"]);
        } else {
            report["result"]["artifact_path"] = json!("/unverified/output");
            report["result"]["files"] = json!([
                {"path":"index.md", "sha256":format!("sha256:{}", "0".repeat(64))}
            ]);
        }
        let accepted = run(root.path(), command, &report);
        assert_eq!(accepted.exit_code, 1, "{:?}", accepted.envelope);
        assert_eq!(accepted.envelope.diagnostics[0].id, "tool_failure");
        let result = accepted.envelope.result.unwrap();
        assert_eq!(result["source_id"], report["result"]["source_id"]);
        assert_eq!(result["tool"], report["result"]["tool"]);
        if command == "fmt" {
            assert_eq!(result["changed_files"], json!([]));
            assert_eq!(result["updated_files"], json!([]));
            assert!(result["formatted_source_id"].is_null());
        } else {
            assert_eq!(result["files"], json!([]));
            assert!(result["artifact_path"].is_null());
        }
        assert_eq!(
            fs::read(root.path().join("sources/module.qli")).unwrap(),
            before
        );
        assert!(!root.path().join("target").exists());
    }
}

#[test]
fn child_failures_redact_private_paths_in_messages_and_suggestions() {
    for command in ["fmt", "doc"] {
        let (root, report) = fixture(command, true);
        let result = run(root.path(), command, &report);
        assert_eq!(result.exit_code, 1, "{:?}", result.envelope);
        let diagnostic = &result.envelope.diagnostics[0];
        assert_eq!(diagnostic.id, "tool_failure");
        for text in [
            diagnostic.message.as_str(),
            diagnostic.suggestion.as_deref().unwrap(),
        ] {
            assert!(text.contains("<private>"));
            assert!(!text.contains("qargo-snapshot-") && !text.contains("qargo-doc-"));
        }
        if command == "fmt" {
            assert!(result.envelope.result.as_ref().unwrap()["formatted_source_id"].is_null());
        }
    }
}

struct DetachedDescendant {
    pid_path: std::path::PathBuf,
}

impl DetachedDescendant {
    fn is_dead(&self) -> bool {
        let pid = fs::read_to_string(&self.pid_path).unwrap();
        let output = std::process::Command::new("ps")
            .args(["-p", pid.trim(), "-o", "stat="])
            .output()
            .unwrap();
        assert!(
            output.stderr.is_empty(),
            "Cannot inspect descendant: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let status = String::from_utf8(output.stdout).unwrap();
        status.trim().is_empty() || status.trim().starts_with('Z')
    }
}

impl Drop for DetachedDescendant {
    fn drop(&mut self) {
        if self.pid_path.exists() && !self.is_dead() {
            let pid = fs::read_to_string(&self.pid_path).unwrap();
            let _ = std::process::Command::new("kill")
                .args(["-KILL", pid.trim()])
                .status();
        }
    }
}

fn descendant_fixture(command: &str, rejection: &str) -> (tempfile::TempDir, DetachedDescendant) {
    let root = tempfile::tempdir().unwrap();
    for directory in ["sources", "tests", "docs"] {
        fs::create_dir(root.path().join(directory)).unwrap();
    }
    fs::write(
        root.path().join("sources/module.qli"),
        "pub unitary fn f(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    fs::write(root.path().join("Qargo.toml"), "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"0.1.8\"\nedition=\"2026\"\n[source]\nroot=\"sources\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
    let pid_path = root.path().join("descendant.pid");
    let script = root.path().join("tool");
    let response = root.path().join("response.json");
    let mutate = if rejection == "executable" {
        "printf '#changed\\n' >> \"$0\"\n"
    } else {
        ""
    };
    fs::write(&script, format!("#!/bin/sh\nsleep 60 >/dev/null 2>&1 &\nprintf '%s' \"$!\" > {}\n{mutate}/bin/cat {}\nexit 1\n", quote(&pid_path), quote(&response))).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let frozen = FrozenSources::capture(&root.path().join("sources")).unwrap();
    let name = match command {
        "lint" => "qlippy",
        "fmt" => "qlifmt",
        _ => "qlidoc",
    };
    let mut result = json!({"source_count":1,"source_id":frozen.source_id(),
        "qleisli_check":if command == "lint" { json!({"status":"failed","reason":"compiler_error"}) } else { json!({"status":"not_run","reason":"syntax_only"}) },
        "tool":{"name":name,"version":qargo_tools::VERSION,"executable_sha256":digest_path(&script).unwrap(),"qleisli_version":"0.2.1","profile":"finite-v0"}});
    if command == "fmt" {
        result["formatted_source_id"] = Value::Null;
        result["changed_files"] = json!([]);
        result["updated_files"] = json!([]);
        result["check"] = json!(false);
        result["diff"] = json!("");
    } else if command == "doc" {
        result["document_private_items"] = json!(false);
        result["artifact_path"] = Value::Null;
        result["files"] = json!([]);
    }
    let mut report = json!({"format":format!("{name}.result"),"version":1,"command":command,"outcome":"error",
        "diagnostics":[{"id":"type_mismatch","category":"compiler","severity":"error","primary":null,"message":"rejected","suggestion":null}],"result":result});
    match rejection {
        "schema" => report["unexpected"] = json!(true),
        "identity" => report["result"]["tool"]["version"] = json!("0.0.0"),
        "source" => report["result"]["source_id"] = json!("wrong source"),
        "location" => {
            report["diagnostics"][0]["primary"] =
                json!({"path":"module.qli","start":0,"end":1,"line":2,"column":1})
        }
        "result" if command == "fmt" => report["result"]["check"] = json!(true),
        "result" if command == "doc" => report["result"]["document_private_items"] = json!(true),
        "result" => report["result"]["qleisli_check"]["reason"] = Value::Null,
        _ => {}
    }
    let bytes = if rejection == "json" {
        b"{invalid".to_vec()
    } else {
        serde_json::to_vec(&report).unwrap()
    };
    fs::write(response, bytes).unwrap();
    (root, DetachedDescendant { pid_path })
}

fn run_descendant_tool(root: &Path, command: &str) -> qargo_tools::report::Report {
    let name = match command {
        "lint" => "qlippy",
        "fmt" => "qlifmt",
        _ => "qlidoc",
    };
    qargo::run(&[
        command.into(),
        format!("--manifest-path={}", root.join("Qargo.toml").display()).into(),
        format!("--{name}={}", root.join("tool").display()).into(),
    ])
}

#[test]
fn transport_rejection_terminates_descendants_after_output_capture() {
    use std::time::{Duration, Instant};
    for command in ["lint", "fmt", "doc"] {
        for rejection in [
            "json",
            "schema",
            "identity",
            "source",
            "location",
            "result",
            "executable",
        ] {
            let (root, descendant) = descendant_fixture(command, rejection);
            let report = run_descendant_tool(root.path(), command);
            assert_eq!(
                report.exit_code, 1,
                "{command}: {rejection}: {:?}",
                report.envelope
            );
            assert_eq!(
                report.envelope.diagnostics[0].id, "invalid_tool_response",
                "{command}: {rejection}: {:?}",
                report.envelope
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            while !descendant.is_dead() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                descendant.is_dead(),
                "{command}: {rejection}: descendant survived transport rejection"
            );
        }
    }
}

struct UnrelatedProcess(std::process::Child);

impl Drop for UnrelatedProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn accepted_tool_failure_disarms_descendant_cleanup() {
    let mut unrelated = UnrelatedProcess(
        std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap(),
    );
    for command in ["lint", "fmt", "doc"] {
        let (root, descendant) = descendant_fixture(command, "accepted");
        let report = run_descendant_tool(root.path(), command);
        assert_eq!(report.exit_code, 1, "{:?}", report.envelope);
        assert_eq!(report.envelope.diagnostics[0].id, "type_mismatch");
        assert!(
            !descendant.is_dead(),
            "Accepted transport was cleaned up prematurely: {command}"
        );
        assert!(unrelated.0.try_wait().unwrap().is_none());
    }
}
