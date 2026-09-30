use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use qargo_tools::qargo;
use qargo_tools::report::Report;
use qargo_tools::snapshot::FrozenSources;
use serde_json::{Value, json};

const MANIFEST: &str = "schema-version = 2\n[qrate]\nname = \"example\"\nversion = \"0.1.0\"\nedition = \"2026\"\n[source]\nroot = \"src\"\n[tests]\nroot = \"tests\"\n[docs]\nroot = \"docs\"\n";

fn qrate() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for root in ["src", "tests", "docs"] {
        fs::create_dir(directory.path().join(root)).unwrap();
    }
    fs::write(directory.path().join("Qargo.toml"), MANIFEST).unwrap();
    directory
}

fn run(directory: &Path, command: &str, other: &[&str]) -> Report {
    let mut args = vec![
        OsString::from(command),
        OsString::from(format!(
            "--manifest-path={}",
            directory.join("Qargo.toml").display()
        )),
    ];
    args.extend(other.iter().map(OsString::from));
    qargo::run(&args)
}

fn result(report: &Report) -> &Value {
    report.envelope.result.as_ref().unwrap()
}

#[test]
fn empty_qrate_check_and_build_report_no_compiler_result() {
    let qrate = qrate();
    let check = run(qrate.path(), "check", &[]);
    assert_eq!(check.exit_code, 0, "{:?}", check.envelope);
    assert_eq!(result(&check)["source_count"], 0);
    assert_eq!(
        result(&check)["qleisli_check"],
        json!({"status":"not_run","reason":"no_sources"})
    );
    let build = run(qrate.path(), "build", &[]);
    assert_eq!(build.exit_code, 0, "{:?}", build.envelope);
    let path = qrate
        .path()
        .join(result(&build)["artifact_path"].as_str().unwrap());
    for root in ["src", "tests", "docs"] {
        assert!(path.join("snapshot").join(root).is_dir());
    }
    assert_eq!(
        fs::read(path.join("snapshot/Qargo.toml")).unwrap(),
        MANIFEST.as_bytes()
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(path.join("module-index.json")).unwrap())
            .unwrap(),
        json!([])
    );
    let record: Value =
        serde_json::from_slice(&fs::read(path.join("build-record.json")).unwrap()).unwrap();
    assert_eq!(record["qleisli_check"]["status"], "not_run");
    assert_eq!(record["tool"]["name"], "qargo");
    for backend in ["QLT", "qlidoc"] {
        let step = record["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|step| step["name"] == backend)
            .unwrap();
        assert_eq!(step["status"], "not_run");
        assert_eq!(
            step["reason"],
            if backend == "QLT" {
                "backend_unavailable"
            } else {
                "not_requested"
            }
        );
    }
    assert_eq!(run(qrate.path(), "build", &[]).exit_code, 0);
}

#[test]
fn input_identity_survives_relocation_and_tracks_every_declared_input() {
    let first = qrate();
    let second = qrate();
    fs::write(first.path().join("docs/guide.md"), "one").unwrap();
    fs::write(second.path().join("docs/guide.md"), "one").unwrap();
    let before = run(first.path(), "check", &[]);
    let moved = run(second.path(), "check", &[]);
    assert_eq!(result(&before)["input_id"], result(&moved)["input_id"]);
    fs::write(first.path().join("docs/guide.md"), "two").unwrap();
    let changed = run(first.path(), "check", &[]);
    assert_ne!(result(&before)["input_id"], result(&changed)["input_id"]);
    assert_eq!(result(&before)["source_id"], result(&changed)["source_id"]);
}

#[test]
fn empty_qrates_keep_qlt_unavailable_and_generate_documentation() {
    let qrate = qrate();
    let report = run(qrate.path(), "test", &[]);
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.envelope.diagnostics[0].id, "backend_unavailable");
    assert_eq!(result(&report)["backend"]["status"], "unavailable");
    let tool = format!("--qlidoc={}", env!("CARGO_BIN_EXE_qlidoc"));
    let report = run(qrate.path(), "doc", &[&tool]);
    assert_eq!(report.exit_code, 0, "{:?}", report.envelope);
    assert_eq!(
        result(&report)["qleisli_check"],
        json!({"status":"not_run","reason":"no_sources"})
    );
    let path = qrate
        .path()
        .join(result(&report)["artifact_path"].as_str().unwrap());
    assert!(path.join("index.md").is_file());
}

#[test]
fn manifest_roots_require_strings_even_when_date_named_directories_exist() {
    for literal in [
        "2026-09-30",
        "2026-09-30T12:34:56",
        "2026-09-30T12:34:56Z",
        "12:34:56",
    ] {
        for root in ["src", "tests", "docs"] {
            let qrate = qrate();
            fs::create_dir(qrate.path().join(literal)).unwrap();
            let field = format!("root = \"{root}\"");
            let quoted = MANIFEST.replace(&field, &format!("root = \"{literal}\""));
            fs::write(qrate.path().join("Qargo.toml"), quoted).unwrap();
            let accepted = run(qrate.path(), "check", &[]);
            assert_eq!(
                accepted.exit_code, 0,
                "{root}: {literal}: {:?}",
                accepted.envelope
            );
            assert_eq!(result(&accepted)["qleisli_check"]["reason"], "no_sources");

            let unquoted = MANIFEST.replace(&field, &format!("root = {literal}"));
            fs::write(qrate.path().join("Qargo.toml"), unquoted).unwrap();
            for command in ["check", "build"] {
                let rejected = run(qrate.path(), command, &[]);
                assert_eq!(
                    rejected.exit_code, 1,
                    "{command}: {root}: {literal}: {:?}",
                    rejected.envelope
                );
                assert_eq!(rejected.envelope.diagnostics[0].id, "invalid_manifest");
                assert_eq!(rejected.envelope.diagnostics[0].category, "qargo");
                assert!(rejected.envelope.result.is_none());
            }
            assert!(!qrate.path().join("target").exists());
        }
    }
}

#[test]
fn manifest_validation_is_closed_and_rejects_bad_roots_names_versions() {
    for manifest in [
        MANIFEST.replace("schema-version = 2", "schema-version = 3"),
        format!("unexpected = true\n{MANIFEST}"),
        MANIFEST.replace("name = \"example\"", "name = \"9example\""),
        MANIFEST.replace("version = \"0.1.0\"", "version = \"00.1.0\""),
        MANIFEST.replace("root = \"src\"", "root = \"../src\""),
        MANIFEST.replace("root = \"src\"", "root = \"./src\""),
        MANIFEST.replace("root = \"src\"", "root = \"target\""),
        MANIFEST.replace("root = \"tests\"", "root = \"src\""),
        MANIFEST.replace("root = \"tests\"", "root = \"missing\""),
    ] {
        let qrate = qrate();
        fs::write(qrate.path().join("Qargo.toml"), manifest).unwrap();
        assert_eq!(run(qrate.path(), "check", &[]).exit_code, 1);
    }
}

#[test]
fn manifest_edition_is_required_and_validated_before_every_qrate_operation() {
    let field = "edition = \"2026\"\n";
    let mut invalid = vec![(MANIFEST.replace(field, ""), "invalid_manifest")];
    for literal in ["2026", "2026.0", "true", "2026-09-30", "[\"2026\"]"] {
        invalid.push((
            MANIFEST.replace(field, &format!("edition = {literal}\n")),
            "invalid_manifest",
        ));
    }
    for edition in ["", "2024", "2025", "2027", "02026", "2026 "] {
        invalid.push((
            MANIFEST.replace(field, &format!("edition = \"{edition}\"\n")),
            "unsupported_edition",
        ));
    }
    invalid.push((
        MANIFEST.replace(field, "edition = \"2026\"\nedition = \"2026\"\n"),
        "invalid_manifest",
    ));
    invalid.push((
        MANIFEST
            .replace(field, "")
            .replace("[source]", "[source]\nedition = \"2026\""),
        "invalid_manifest",
    ));
    for (manifest, expected_id) in invalid {
        let qrate = qrate();
        fs::write(qrate.path().join("Qargo.toml"), &manifest).unwrap();
        for command in ["check", "build", "lint", "fmt", "doc", "test"] {
            let rejected = run(qrate.path(), command, &[]);
            assert_eq!(rejected.exit_code, 1, "{command}: {manifest}");
            assert_eq!(rejected.envelope.version, 1);
            assert_eq!(rejected.envelope.diagnostics[0].id, expected_id);
            assert_eq!(rejected.envelope.diagnostics[0].category, "qargo");
            assert!(rejected.envelope.result.is_none());
        }
        assert!(!qrate.path().join("target").exists());
    }
}

#[test]
fn legacy_manifest_schema_is_rejected_with_an_explicit_edition_migration() {
    let qrate = qrate();
    let legacy = MANIFEST
        .replace("schema-version = 2", "schema-version = 1")
        .replace("edition = \"2026\"\n", "");
    fs::write(qrate.path().join("Qargo.toml"), legacy).unwrap();
    for command in ["check", "build", "lint", "fmt", "doc", "test"] {
        let report = run(qrate.path(), command, &[]);
        assert_eq!(report.exit_code, 1);
        let diagnostic = &report.envelope.diagnostics[0];
        assert_eq!(diagnostic.id, "unsupported_manifest_version");
        let suggestion = diagnostic.suggestion.as_ref().unwrap();
        assert!(suggestion.contains("schema-version = 2"));
        assert!(suggestion.contains("edition = \"2026\""));
        assert!(report.envelope.result.is_none());
    }
}

#[test]
fn artifacts_are_never_silently_overwritten() {
    let qrate = qrate();
    let first = run(qrate.path(), "build", &[]);
    let path = qrate
        .path()
        .join(result(&first)["artifact_path"].as_str().unwrap());
    fs::write(path.join("module-index.json"), "user content").unwrap();
    let next = run(qrate.path(), "build", &[]);
    assert_eq!(next.exit_code, 1);
    assert_eq!(next.envelope.diagnostics[0].id, "artifact_mismatch");
    assert_eq!(result(&next)["input_id"], result(&first)["input_id"]);
    assert_eq!(result(&next)["qleisli_check"]["status"], "not_run");
    assert_eq!(result(&next)["build"]["status"], "failed");
    assert_eq!(
        fs::read_to_string(path.join("module-index.json")).unwrap(),
        "user content"
    );
}

#[test]
fn upward_discovery_stops_at_a_malformed_manifest() {
    let qrate = qrate();
    let nested = qrate.path().join("nested/deep");
    fs::create_dir_all(&nested).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_qargo"))
        .current_dir(&nested)
        .args(["check", "--format=json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    fs::write(qrate.path().join("nested/Qargo.toml"), "invalid").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_qargo"))
        .current_dir(&nested)
        .args(["check", "--format=json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["diagnostics"][0]["id"], "invalid_manifest");
}

#[test]
fn compiler_errors_remain_compiler_errors_with_qrate_relative_locations() {
    let qrate = qrate();
    fs::write(
        qrate.path().join("src/module.qli"),
        "unitary fn broken(q:Q<Bit>)->Q<Bit>{missing}",
    )
    .unwrap();
    let report = run(qrate.path(), "check", &[]);
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.envelope.diagnostics[0].category, "compiler");
    let location = report.envelope.diagnostics[0].primary.as_ref().unwrap();
    assert_eq!(location.path, "src/module.qli");
    assert_eq!(result(&report)["qleisli_check"]["status"], "failed");
}

#[test]
fn lint_preserves_normal_compiler_failures_and_remaps_default_qrate_locations() {
    let qrate = qrate();
    let tool = format!("--qlippy={}", env!("CARGO_BIN_EXE_qlippy"));
    for source in [
        "unitary fn broken(q:Q<Bit>)->Q<Bit>{missing}",
        "invalid syntax!",
    ] {
        fs::write(qrate.path().join("src/module.qli"), source).unwrap();
        let report = run(qrate.path(), "lint", &[&tool]);
        assert_eq!(report.exit_code, 1, "{:?}", report.envelope);
        assert_eq!(report.envelope.diagnostics[0].category, "compiler");
        if let Some(location) = &report.envelope.diagnostics[0].primary {
            assert_eq!(location.path, "src/module.qli");
        }
        assert_eq!(result(&report)["qleisli_check"]["status"], "failed");
        assert_eq!(
            result(&report)["input_id"],
            result(&run(qrate.path(), "check", &[]))["input_id"]
        );
    }
}

#[test]
fn full_input_budget_builds_are_reusable_and_aggregate_overflow_is_rejected() {
    let qrate = qrate();
    let full = vec![b'x'; 1024 * 1024];
    for index in 0..15 {
        fs::write(qrate.path().join(format!("docs/{index}.txt")), &full).unwrap();
    }
    fs::write(
        qrate.path().join("tests/last.txt"),
        &full[..full.len() - MANIFEST.len()],
    )
    .unwrap();
    let first = run(qrate.path(), "build", &[]);
    assert_eq!(first.exit_code, 0, "{:?}", first.envelope);
    assert_eq!(run(qrate.path(), "build", &[]).exit_code, 0);
    fs::write(
        qrate.path().join("tests/last.txt"),
        &full[..full.len() - MANIFEST.len() + 1],
    )
    .unwrap();
    let overflow = run(qrate.path(), "check", &[]);
    assert_eq!(overflow.exit_code, 1);
    assert_eq!(overflow.envelope.diagnostics[0].id, "input_budget");
}

#[test]
fn standalone_lint_needs_no_qrate_and_preserves_warnings() {
    let source = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("module.qli"),
        "// 雪\r\nuse std::quantum::x;\r\nunitary fn identity(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    let tool = format!("--qlippy={}", env!("CARGO_BIN_EXE_qlippy"));
    let args = [
        OsString::from("lint"),
        source.path().as_os_str().to_os_string(),
        OsString::from(&tool),
    ];
    let report = qargo::run(&args);
    assert_eq!(report.exit_code, 0, "{:?}", report.envelope);
    assert_eq!(report.envelope.diagnostics[0].id, "unused_import");
    assert_eq!(
        report.envelope.diagnostics[0]
            .primary
            .as_ref()
            .unwrap()
            .path,
        "module.qli"
    );
    let mut args = args.to_vec();
    args.push(OsString::from("--deny-warnings"));
    let denied = qargo::run(&args);
    assert_eq!(denied.exit_code, 1);
    assert_eq!(denied.envelope.diagnostics[0].severity, "warning");
    assert!(denied.envelope.result.is_some());
}

#[test]
fn cli_misuse_is_exit_two_and_json_is_one_object() {
    for args in [
        vec!["check", "--qlippy=somewhere"],
        vec!["check", "--deny-warnings"],
        vec!["lint", "--qlippy="],
        vec!["--version", "check"],
        vec!["check", "--format=json", "--format=json"],
        vec!["build", "unexpected"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_qargo"))
            .args(args)
            .arg("--format=json")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stderr.is_empty());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["format"], "qargo.result");
        assert_eq!(response["version"], 1);
    }
    let output = Command::new(env!("CARGO_BIN_EXE_qargo"))
        .args(["--help", "--format=json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        response["result"]["help"]
            .as_str()
            .unwrap()
            .contains("qargo lint")
    );
}

#[cfg(unix)]
fn script(directory: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join("fake-qlippy");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn quoted(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

#[cfg(unix)]
#[test]
fn no_qargo_command_invokes_cargo() {
    let qrate = qrate();
    let trap = tempfile::tempdir().unwrap();
    let marker = trap.path().join("cargo-started");
    let fake = script(
        trap.path(),
        &format!("printf called > {}\nexit 98", quoted(&marker)),
    );
    fs::rename(fake, trap.path().join("cargo")).unwrap();
    for command in ["check", "build", "test", "doc", "lint"] {
        let mut process = Command::new(env!("CARGO_BIN_EXE_qargo"));
        process
            .current_dir(qrate.path())
            .env("PATH", trap.path())
            .args([command, "--format=json"]);
        if command == "lint" {
            process.arg(format!("--qlippy={}", env!("CARGO_BIN_EXE_qlippy")));
        }
        let output = process.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(if command == "test" { 1 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn symlink_inputs_and_outputs_are_rejected() {
    use std::os::unix::fs::symlink;
    let qrate = qrate();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("file"), "outside").unwrap();
    symlink(outside.path().join("file"), qrate.path().join("src/link")).unwrap();
    assert_eq!(run(qrate.path(), "check", &[]).exit_code, 1);
    fs::remove_file(qrate.path().join("src/link")).unwrap();
    symlink(outside.path(), qrate.path().join("target")).unwrap();
    let build = run(qrate.path(), "build", &[]);
    assert_eq!(build.exit_code, 1);
    assert_eq!(build.envelope.diagnostics[0].id, "unsafe_output");
    assert_eq!(
        fs::read_to_string(outside.path().join("file")).unwrap(),
        "outside"
    );
}

#[cfg(unix)]
#[test]
fn selected_tools_do_not_fall_back_and_bad_transport_is_rejected() {
    let source = tempfile::tempdir().unwrap();
    let tool = tempfile::tempdir().unwrap();
    for body in ["printf invalid", "printf error >&2\nexit 1", "exit 7"] {
        let path = script(tool.path(), body);
        let report = qargo::run(&[
            OsString::from("lint"),
            source.path().as_os_str().to_os_string(),
            OsString::from(format!("--qlippy={}", path.display())),
        ]);
        assert_eq!(report.exit_code, 1);
        assert_eq!(report.envelope.diagnostics[0].category, "tool");
    }
    let report = qargo::run(&[
        OsString::from("lint"),
        source.path().as_os_str().to_os_string(),
        OsString::from("--qlippy=/this/path/does/not/exist"),
    ]);
    assert_eq!(report.exit_code, 1);
    assert_eq!(report.envelope.diagnostics[0].id, "tool_missing");
}

#[cfg(unix)]
#[test]
fn closed_child_schema_identity_and_coordinates_are_checked() {
    let source = tempfile::tempdir().unwrap();
    let tool = tempfile::tempdir().unwrap();
    fs::write(
        source.path().join("module.qli"),
        "unitary fn identity(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    let frozen = FrozenSources::capture(source.path()).unwrap();
    let response_path = tool.path().join("response.json");
    let tool_path = script(tool.path(), &format!("/bin/cat {}", quoted(&response_path)));
    let digest = qargo_tools::snapshot::digest_path(&tool_path).unwrap();
    let valid = json!({
        "format":"qlippy.result","version":1,"command":"lint","outcome":"ok","diagnostics":[],
        "result":{"source_count":1,"source_id":frozen.source_id,"qleisli_check":{"status":"passed","reason":null},
          "tool":{"name":"qlippy","version":qargo_tools::VERSION,"executable_sha256":digest,"qleisli_version":"0.2.1","profile":"finite-v0"}}
    });
    let args = [
        OsString::from("lint"),
        source.path().as_os_str().to_os_string(),
        OsString::from(format!("--qlippy={}", tool_path.display())),
    ];
    fs::write(&response_path, serde_json::to_vec(&valid).unwrap()).unwrap();
    assert_eq!(qargo::run(&args).exit_code, 0);
    let mut variants = Vec::new();
    let mut bad = valid.clone();
    bad["extra"] = json!(true);
    variants.push(bad);
    let mut bad = valid.clone();
    bad["version"] = json!(2);
    variants.push(bad);
    let mut bad = valid.clone();
    bad["result"]["source_id"] = json!("sha256:wrong");
    variants.push(bad);
    let mut bad = valid.clone();
    bad["result"]["tool"]["profile"] = json!("unknown");
    variants.push(bad);
    let mut bad = valid.clone();
    bad["result"]["tool"]["executable_sha256"] = json!("sha256:wrong");
    variants.push(bad);
    for location in [
        json!({"path":"../module.qli","start":0,"end":1,"line":1,"column":1}),
        json!({"path":"module.qli","start":0,"end":999,"line":1,"column":1}),
        json!({"path":"module.qli","start":0,"end":1,"line":2,"column":1}),
    ] {
        let mut bad = valid.clone();
        bad["diagnostics"] = json!([{"id":"unused_import","category":"lint","severity":"warning","primary":location,"message":"advice","suggestion":null}]);
        variants.push(bad);
    }
    for response in variants {
        fs::write(&response_path, serde_json::to_vec(&response).unwrap()).unwrap();
        let report = qargo::run(&args);
        assert_eq!(report.exit_code, 1);
        assert_eq!(report.envelope.diagnostics[0].id, "invalid_tool_response");
    }
}
