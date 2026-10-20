use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const MANIFEST: &str = "schema-version=2\n[qrate]\nname=\"migration\"\nversion=\"0.2.0\"\nedition=\"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n";
const NAMES: [&str; 2] = ["Qlirgo.toml", "Qargo.toml"];

fn project(names: &[&str], text: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for name in ["src", "tests", "docs"] {
        fs::create_dir(root.path().join(name)).unwrap();
    }
    for name in names {
        fs::write(root.path().join(name), text).unwrap();
    }
    root
}
fn report(output: Output, expected: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    serde_json::from_slice(&output.stdout).unwrap()
}
fn managed(root: &Path, command: &str, extra: &[&str], expected: i32) -> Value {
    report(
        Command::new(env!("CARGO_BIN_EXE_qlirgo"))
            .current_dir(root)
            .arg(command)
            .args(extra)
            .arg("--format=json")
            .output()
            .unwrap(),
        expected,
    )
}
fn standalone(root: &Path, expected: i32) -> Value {
    report(
        Command::new(env!("CARGO_BIN_EXE_qlinty"))
            .arg(root)
            .arg("--format=json")
            .output()
            .unwrap(),
        expected,
    )
}

#[test]
fn both_names_work_in_every_managed_command_without_rewriting_user_files() {
    for names in [&NAMES[..1], &NAMES[1..], &NAMES[..]] {
        let root = project(names, MANIFEST);
        for command in ["check", "build", "lint", "fmt", "doc", "test"] {
            let value = managed(root.path(), command, &[], i32::from(command == "test"));
            assert_eq!(value["format"], "qlirgo.result");
            for name in NAMES {
                assert_eq!(root.path().join(name).exists(), names.contains(&name));
            }
            for name in names {
                assert_eq!(
                    fs::read(root.path().join(name)).unwrap(),
                    MANIFEST.as_bytes()
                );
            }
        }
    }
}

#[test]
fn effective_configuration_normalizes_names_but_input_snapshots_do_not() {
    let mut configurations = Vec::new();
    let mut inputs = Vec::new();
    for names in [&NAMES[..1], &NAMES[1..], &NAMES[..]] {
        let root = project(names, MANIFEST);
        let value = managed(root.path(), "build", &[], 0);
        configurations.push(value["result"]["source_configuration_id"].clone());
        inputs.push(value["result"]["input_id"].clone());
        let output = root
            .path()
            .join(value["result"]["artifact_path"].as_str().unwrap());
        assert!(
            value["result"]["artifact_path"]
                .as_str()
                .unwrap()
                .starts_with("target/qlirgo/")
        );
        for name in NAMES {
            assert_eq!(
                output.join("snapshot").join(name).exists(),
                names.contains(&name)
            );
        }
        assert_eq!(
            standalone(&root.path().join("src"), 0)["format"],
            "qlinty.result"
        );
    }
    assert!(configurations.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(inputs[0] != inputs[1] && inputs[0] != inputs[2] && inputs[1] != inputs[2]);
}

#[test]
fn byte_differences_reject_both_explicit_paths_and_never_publish() {
    for changed in [
        MANIFEST.replace('\n', "\r\n"),
        format!("{MANIFEST}# only a comment differs\n"),
    ] {
        let root = project(&NAMES, MANIFEST);
        fs::write(root.path().join(NAMES[1]), changed).unwrap();
        for command in ["check", "build", "lint", "fmt", "doc", "test"] {
            for selected in [None, Some(NAMES[0]), Some(NAMES[1])] {
                let extra = selected
                    .map(|name| vec!["--manifest-path", name])
                    .unwrap_or_default();
                let value = managed(root.path(), command, &extra, 1);
                assert_eq!(value["diagnostics"][0]["id"], "manifest_conflict");
                assert!(!root.path().join("target").exists());
            }
        }
        assert_eq!(
            standalone(&root.path().join("src"), 1)["diagnostics"][0]["id"],
            "manifest_conflict"
        );
    }
}

#[test]
fn explicit_missing_and_custom_paths_keep_their_original_contract() {
    let root = project(&NAMES[1..], MANIFEST);
    managed(root.path(), "check", &["--manifest-path", NAMES[0]], 1);
    fs::write(root.path().join("custom with spaces.toml"), MANIFEST).unwrap();
    let value = managed(
        root.path(),
        "build",
        &["--manifest-path", "custom with spaces.toml"],
        0,
    );
    let snapshot = root
        .path()
        .join(value["result"]["artifact_path"].as_str().unwrap())
        .join("snapshot");
    assert!(snapshot.join("custom with spaces.toml").is_file());
    assert!(!snapshot.join("Qlirgo.toml").exists());
}

#[test]
fn a_nearer_invalid_manifest_never_falls_back_to_a_valid_ancestor() {
    let root = project(&NAMES, MANIFEST);
    for name in NAMES {
        fs::write(
            root.path().join("src").join(name),
            "schema-version=2\n[qrate]\n",
        )
        .unwrap();
        managed(&root.path().join("src"), "check", &[], 1);
        standalone(&root.path().join("src"), 1);
        fs::remove_file(root.path().join("src").join(name)).unwrap();
    }
}

#[test]
fn missing_or_invalid_editions_are_errors_for_every_manifest_spelling() {
    for names in [&NAMES[..1], &NAMES[1..], &NAMES[..]] {
        for invalid in [
            MANIFEST.replace("edition=\"2026\"\n", ""),
            MANIFEST.replace("2026", "2027"),
        ] {
            let root = project(names, &invalid);
            managed(root.path(), "check", &[], 1);
            standalone(&root.path().join("src"), 1);
            assert!(!root.path().join("target").exists());
        }
    }
}

#[test]
fn new_only_manifests_reach_the_real_beta_checker_for_nonempty_sources() {
    let root = project(&NAMES[..1], MANIFEST);
    fs::write(
        root.path().join("src/main.qli"),
        "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}",
    )
    .unwrap();
    for command in ["check", "build", "lint"] {
        assert_eq!(
            managed(root.path(), command, &[], 0)["result"]["qleisli_check"]["status"],
            "passed"
        );
    }
    assert_eq!(
        standalone(&root.path().join("src"), 0)["result"]["qleisli_check"]["status"],
        "passed"
    );
    assert!(!root.path().join("Qargo.toml").exists());
}

#[test]
fn nested_manifest_pairs_are_checked_before_any_tool_execution() {
    let root = project(&NAMES[..1], MANIFEST);
    fs::create_dir(root.path().join("src/nested")).unwrap();
    for name in NAMES {
        fs::write(root.path().join("src/nested").join(name), MANIFEST).unwrap();
    }
    standalone(&root.path().join("src"), 0);
    fs::write(
        root.path().join("src/nested/Qargo.toml"),
        format!("{MANIFEST}# mismatch"),
    )
    .unwrap();
    managed(root.path(), "lint", &["--qlinty", "/nonexistent-child"], 1);
    let value = standalone(&root.path().join("src"), 1);
    assert_eq!(value["diagnostics"][0]["id"], "manifest_conflict");
    assert!(!root.path().join("target").exists());
}

#[cfg(unix)]
#[test]
fn invalid_new_name_entries_do_not_disappear_behind_legacy_files() {
    use std::os::unix::fs::symlink;
    for kind in ["directory", "symlink", "invalid_utf8"] {
        let root = project(&NAMES[1..], MANIFEST);
        let new = root.path().join(NAMES[0]);
        match kind {
            "directory" => fs::create_dir(&new).unwrap(),
            "symlink" => symlink(root.path().join(NAMES[1]), &new).unwrap(),
            _ => fs::write(&new, [255]).unwrap(),
        }
        managed(root.path(), "check", &[], 1);
        standalone(&root.path().join("src"), 1);
    }
}

#[test]
fn old_linter_option_is_not_an_alias() {
    let root = project(&NAMES[..1], MANIFEST);
    managed(root.path(), "lint", &["--qlippy", "/nonexistent-child"], 2);
}

#[cfg(unix)]
#[test]
fn old_linter_json_is_rejected_without_running_a_fallback() {
    use std::os::unix::fs::PermissionsExt;
    let root = project(&NAMES[..1], MANIFEST);
    let child = root.path().join("old-response");
    fs::write(&child, "#!/bin/sh\nprintf '%s\\n' '{\"format\":\"qlippy.result\",\"version\":1,\"command\":\"lint\",\"outcome\":\"ok\",\"diagnostics\":[],\"result\":null}'\n").unwrap();
    fs::set_permissions(&child, fs::Permissions::from_mode(0o755)).unwrap();
    let value = managed(
        root.path(),
        "lint",
        &["--qlinty", child.to_str().unwrap()],
        1,
    );
    assert_eq!(value["diagnostics"][0]["id"], "invalid_tool_response");
}
