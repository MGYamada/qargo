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
    let mut result = json!({"source_count":1,"source_id":frozen.source_id,
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
        fs::write(root.join("Qargo.toml"), "schema-version=1\n[qrate]\nname=\"example\"\nversion=\"0.1.1\"\n[source]\nroot=\"sources\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
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
