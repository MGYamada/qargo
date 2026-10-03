//! Build artifacts can only be assembled from a bound checked qrate.

use super::error;
use super::manifest::CapturedQrate;
use super::subject::CheckedQrate;
use crate::PROFILE;
use crate::report::Diagnostic;
use crate::snapshot::Files;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn json_bytes(value: &Value) -> Result<Vec<u8>, Diagnostic> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| {
        error(
            "build_output",
            "qargo",
            format!("Cannot encode artifact: {e}"),
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn artifact_directories(qrate: &CapturedQrate, artifacts: &Files) -> BTreeSet<PathBuf> {
    let mut directories = BTreeSet::new();
    for path in artifacts.keys().map(Path::new).chain(
        [
            Path::new("snapshot").join(&qrate.manifest().source.root),
            Path::new("snapshot").join(&qrate.manifest().tests.root),
            Path::new("snapshot").join(&qrate.manifest().docs.root),
        ]
        .iter()
        .map(PathBuf::as_path),
    ) {
        let mut parent = if artifacts.contains_key(path.to_str().unwrap_or("")) {
            path.parent()
        } else {
            Some(path)
        };
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    directories
}

fn publication_error(error: crate::publication::PublicationError) -> Diagnostic {
    let id = match &error {
        crate::publication::PublicationError::Mismatch => "artifact_mismatch",
        crate::publication::PublicationError::UnsafePath(_) => "unsafe_output",
        crate::publication::PublicationError::Output(_) => "build_output",
    };
    Diagnostic::error(id, "qargo", error.to_string())
}

pub(super) fn publish(
    subject: &CheckedQrate<'_>,
    tool: &Value,
    verify_host: impl FnOnce() -> Result<(), Diagnostic>,
) -> Result<String, Diagnostic> {
    let qrate = subject.qrate();
    let checked = subject.checked();
    let record = json!({
        "format":"qargo.build-record", "version":1, "input_id":qrate.input_id(),
        "qrate":{"name":qrate.manifest().qrate.name,"version":qrate.manifest().qrate.version},
        "source_count":checked.source_count(), "qleisli_check":checked.qleisli_check(),
        "profile":PROFILE, "tool":tool,
        "steps":[{"name":"manifest","status":"passed","reason":null},
                 {"name":"snapshot","status":"passed","reason":null},
                 {"name":"qleisli","status":checked.qleisli_check()["status"],"reason":checked.qleisli_check()["reason"]},
                 {"name":"QLT","status":"not_run","reason":"backend_unavailable"},
                 {"name":"qlidoc","status":"not_run","reason":"not_requested"},
                 {"name":"build","status":"passed","reason":null}]
    });
    let mut artifacts: Files = qrate
        .files()
        .iter()
        .map(|(path, bytes)| (format!("snapshot/{path}"), bytes.clone()))
        .collect();
    artifacts.insert(
        "module-index.json".into(),
        json_bytes(checked.module_index())?,
    );
    artifacts.insert("build-record.json".into(), json_bytes(&record)?);
    let directories = artifact_directories(qrate, &artifacts);
    let hex = qrate
        .input_id()
        .strip_prefix("sha256:")
        .ok_or_else(|| error("build_output", "qargo", "Invalid input identity."))?;
    let tool_hex = tool["executable_sha256"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| error("tool_identity", "tool", "Invalid executable identity."))?;
    let relative = format!("target/qargo/{hex}/{tool_hex}");
    verify_host()?;
    crate::publication::publish_at(
        qrate.held_directory(),
        Path::new(&relative),
        &artifacts,
        &directories,
    )
    .map_err(publication_error)?;
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::super::manifest::capture;
    use super::*;
    use std::fs;

    #[test]
    fn replacing_the_captured_qrate_cannot_redirect_build_publication() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("qrate");
        let replacement = home.path().join("replacement");
        for directory in [&root, &replacement] {
            for name in ["src", "tests", "docs"] {
                fs::create_dir_all(directory.join(name)).unwrap();
            }
            fs::write(directory.join("Qargo.toml"), "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"0.1.0\"\nedition=\"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n").unwrap();
        }
        let captured = capture(Some(&root.join("Qargo.toml"))).unwrap();
        let checked = captured.check().unwrap();
        let tool = crate::report::tool_info("qargo", crate::VERSION).unwrap();
        let old = home.path().join("old");
        let failed = publish(&checked, &tool, || {
            fs::rename(&root, &old).unwrap();
            fs::rename(&replacement, &root).unwrap();
            Ok(())
        });
        assert_eq!(failed.unwrap_err().id, "build_output");
        assert!(!root.join("target").exists());
        assert!(!old.join("target").exists());
    }

    #[test]
    fn qrate_check_and_artifacts_use_captured_bytes_after_original_mutation() {
        let directory = tempfile::tempdir().unwrap();
        for root in ["src", "tests", "docs"] {
            fs::create_dir(directory.path().join(root)).unwrap();
        }
        let manifest = "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"0.1.0\"\nedition=\"2026\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n";
        fs::write(directory.path().join("Qargo.toml"), manifest).unwrap();
        let source = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
        fs::write(directory.path().join("src/module.qli"), source).unwrap();
        let frozen = capture(Some(&directory.path().join("Qargo.toml"))).unwrap();
        fs::write(directory.path().join("src/module.qli"), "invalid syntax").unwrap();
        fs::write(directory.path().join("Qargo.toml"), "invalid manifest").unwrap();
        let checked = frozen.check().unwrap();
        assert_eq!(checked.checked().qleisli_check()["status"], "passed");
        let tool = crate::report::tool_info("qargo", crate::VERSION).unwrap();
        let artifact = publish(&checked, &tool, || Ok(())).unwrap();
        assert_eq!(
            fs::read_to_string(
                directory
                    .path()
                    .join(&artifact)
                    .join("snapshot/src/module.qli")
            )
            .unwrap(),
            source
        );
        assert_eq!(
            fs::read_to_string(directory.path().join(&artifact).join("snapshot/Qargo.toml"))
                .unwrap(),
            manifest
        );
        // Rebuilding the host engine must preserve the earlier input/tool record.
        let mut rebuilt_tool = tool.clone();
        rebuilt_tool["executable_sha256"] = json!(format!("sha256:{}", "f".repeat(64)));
        let rebuilt_artifact = publish(&checked, &rebuilt_tool, || Ok(())).unwrap();
        assert_ne!(artifact, rebuilt_artifact);
        for (path, expected_tool) in [(&artifact, &tool), (&rebuilt_artifact, &rebuilt_tool)] {
            let record: Value = serde_json::from_slice(
                &fs::read(directory.path().join(path).join("build-record.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(record["input_id"], frozen.input_id());
            assert_eq!(&record["tool"], expected_tool);
        }
        assert_eq!(publish(&checked, &tool, || Ok(())).unwrap(), artifact);
    }
}
