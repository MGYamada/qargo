//! Discover, validate, and freeze a local qrate's declared inputs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::report::Diagnostic;
use crate::snapshot::{self, Files, FrozenSources, InputDirectory};

const MANIFEST_SCHEMA_VERSION: u32 = 2;
const QLEISLI_EDITION: &str = "2026";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    #[serde(rename = "schema-version")]
    schema_version: u32,
    pub(super) qrate: Qrate,
    pub(super) source: Root,
    pub(super) tests: Root,
    pub(super) docs: Root,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Qrate {
    pub(super) name: String,
    pub(super) version: String,
    edition: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Root {
    pub(super) root: String,
}

pub(super) struct CapturedQrate {
    directory: PathBuf,
    manifest: Manifest,
    files: Files,
    input_id: String,
    sources: FrozenSources,
}

impl CapturedQrate {
    pub(super) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(super) fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub(super) fn files(&self) -> &Files {
        &self.files
    }

    pub(super) fn input_id(&self) -> &str {
        &self.input_id
    }

    pub(super) fn sources(&self) -> &FrozenSources {
        &self.sources
    }
}

fn discover(explicit: Option<&Path>) -> Result<PathBuf, Diagnostic> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| {
        Diagnostic::error(
            "manifest_discovery",
            "qargo",
            format!("Cannot read current directory: {e}"),
        )
    })?;
    for directory in cwd.ancestors() {
        let path = directory.join("Qargo.toml");
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(Diagnostic::error(
                    "manifest_discovery",
                    "qargo",
                    format!("Cannot inspect Qargo.toml: {e}"),
                ));
            }
        }
    }
    Err(Diagnostic::error(
        "manifest_missing",
        "qargo",
        "No Qargo.toml found in the current or ancestor directories.",
    ))
}

fn root_path(value: &str) -> Result<PathBuf, Diagnostic> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || path
            .components()
            .next()
            .is_some_and(|part| part.as_os_str() == "target")
    {
        return Err(Diagnostic::error(
            "invalid_manifest",
            "qargo",
            format!("Invalid qrate-relative root: {value}"),
        ));
    }
    Ok(path.to_path_buf())
}

fn validate_manifest(
    manifest: &Manifest,
    directory: &InputDirectory,
) -> Result<Vec<InputDirectory>, Diagnostic> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        let mut diagnostic = Diagnostic::error(
            "unsupported_manifest_version",
            "qargo",
            "Only manifest schema-version 2 is supported.",
        );
        diagnostic.suggestion = Some(
            "Migrate Qargo.toml to schema-version = 2 and add edition = \"2026\" under [qrate]."
                .into(),
        );
        return Err(diagnostic);
    }
    let Some(edition) = &manifest.qrate.edition else {
        let mut diagnostic = Diagnostic::error(
            "invalid_manifest",
            "qargo",
            "[qrate].edition is required and must be an explicit string; there is no default edition.",
        );
        diagnostic.suggestion = Some("Add edition = \"2026\" under [qrate].".into());
        return Err(diagnostic);
    };
    if edition != QLEISLI_EDITION {
        let mut diagnostic = Diagnostic::error(
            "unsupported_edition",
            "qargo",
            format!("Unsupported Qleisli edition {edition:?}; only \"2026\" is supported."),
        );
        diagnostic.suggestion = Some("Set edition = \"2026\" under [qrate].".into());
        return Err(diagnostic);
    }
    let name = &manifest.qrate.name;
    if !name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(Diagnostic::error(
            "invalid_manifest",
            "qargo",
            "Qrate names must begin with an ASCII letter and contain only ASCII letters, digits, hyphens and underscores.",
        ));
    }
    let version: Vec<_> = manifest.qrate.version.split('.').collect();
    if version.len() != 3
        || version.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|c| c.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        return Err(Diagnostic::error(
            "invalid_manifest",
            "qargo",
            "Qrate version must be canonical decimal MAJOR.MINOR.PATCH.",
        ));
    }
    let roots = [
        &manifest.source.root,
        &manifest.tests.root,
        &manifest.docs.root,
    ]
    .into_iter()
    .map(|root| root_path(root))
    .collect::<Result<Vec<_>, _>>()?;
    let reserved_target = directory.directory_identity_if_exists(std::ffi::OsStr::new("target"))?;
    let mut resolved_roots = Vec::new();
    let mut held_roots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        if roots
            .iter()
            .enumerate()
            .any(|(other_index, other)| index != other_index && root.starts_with(other))
        {
            return Err(Diagnostic::error(
                "invalid_manifest",
                "qargo",
                "Source, test and documentation roots must not overlap.",
            ));
        }
        let mut held = directory.clone();
        let mut identities = Vec::new();
        for component in root.components() {
            held = held.child(component.as_os_str()).map_err(|error| {
                Diagnostic::error(
                    "invalid_manifest",
                    "qargo",
                    format!(
                        "Cannot open declared root {root:?} without symlinks: {}",
                        error.message
                    ),
                )
            })?;
            identities.push(held.identity()?);
        }
        resolved_roots.push(identities);
        held_roots.push(held);
    }
    // Compare filesystem objects after rejecting symlinks, not normalized spellings.
    for (index, identities) in resolved_roots.iter().enumerate() {
        if reserved_target
            .as_ref()
            .is_some_and(|target| identities.contains(target))
        {
            return Err(Diagnostic::error(
                "invalid_manifest",
                "qargo",
                "Declared roots must not include the reserved target directory or any filesystem alias of it.",
            ));
        }
        let root = identities.last().expect("nonempty declared root");
        if resolved_roots
            .iter()
            .enumerate()
            .any(|(other_index, other)| index != other_index && other.contains(root))
        {
            return Err(Diagnostic::error(
                "invalid_manifest",
                "qargo",
                "Source, test and documentation roots must not overlap.",
            ));
        }
    }
    Ok(held_roots)
}

pub(super) fn capture(explicit: Option<&Path>) -> Result<CapturedQrate, Diagnostic> {
    capture_with_hook(explicit, |_| {})
}

fn capture_with_hook(
    explicit: Option<&Path>,
    after_validation: impl FnOnce(&Path),
) -> Result<CapturedQrate, Diagnostic> {
    let path = discover(explicit)?;
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = fs::canonicalize(directory).map_err(|error| {
        Diagnostic::error(
            "invalid_manifest",
            "qargo",
            format!("Cannot resolve qrate directory: {error}"),
        )
    })?;
    let held_directory = InputDirectory::open(&directory)?;
    let raw = held_directory.read_regular(path.file_name().ok_or_else(|| {
        Diagnostic::error(
            "invalid_manifest",
            "qargo",
            "Manifest must name an ordinary file.",
        )
    })?)?;
    let text = std::str::from_utf8(&raw)
        .map_err(|_| Diagnostic::error("invalid_manifest", "qargo", "Qargo.toml must be UTF-8."))?;
    let parsed: toml::Value = toml::from_str(text).map_err(|e| {
        Diagnostic::error(
            "invalid_manifest",
            "qargo",
            format!("Invalid Qargo.toml: {e}"),
        )
    })?;
    if parsed.get("dependencies").is_some() || parsed.get("dev-dependencies").is_some() {
        let mut diagnostic = Diagnostic::error(
            "invalid_manifest",
            "qargo",
            "Qargo manifest schema 2 does not support dependency tables.",
        );
        diagnostic.suggestion = Some(
            "Remove [dependencies] and [dev-dependencies] from Qargo.toml. Use local Qleisli source modules; keep Rust engine dependencies in developer Cargo.toml outside the qrate. Qrate dependency resolution is deferred.".into(),
        );
        return Err(diagnostic);
    }
    // Decode the original text to preserve TOML field types; Value converts datetimes to strings.
    let manifest: Manifest = toml::from_str(text).map_err(|e| {
        Diagnostic::error(
            "invalid_manifest",
            "qargo",
            format!("Invalid Qargo.toml: {e}"),
        )
    })?;
    let held_roots = validate_manifest(&manifest, &held_directory)?;
    after_validation(&directory);
    let mut files = BTreeMap::from([("Qargo.toml".into(), raw)]);
    for (root, held) in [
        &manifest.source.root,
        &manifest.tests.root,
        &manifest.docs.root,
    ]
    .into_iter()
    .zip(&held_roots)
    {
        for (path, bytes) in held.collect(None)? {
            files.insert(format!("{root}/{path}"), bytes);
        }
    }
    if files.len() > 4096 || files.values().map(Vec::len).sum::<usize>() > 16 * 1024 * 1024 {
        return Err(Diagnostic::error(
            "input_budget",
            "qargo",
            "Qrate exceeds the 4096-file or 16 MiB input budget.",
        ));
    }
    let prefix = format!("{}/", manifest.source.root);
    let sources = files
        .iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix(&prefix)
                .filter(|path| Path::new(path).extension().is_some_and(|ext| ext == "qli"))
                .map(|path| (path.to_string(), bytes.clone()))
        })
        .collect();
    let sources = FrozenSources::from_files(sources)?;
    let input_id = snapshot::digest_files("qargo.qrate.v1", &files);
    Ok(CapturedQrate {
        directory,
        manifest,
        files,
        input_id,
        sources,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn validated_root_and_its_ancestor_cannot_be_redirected_before_capture() {
        for replaced in ["inputs/src", "inputs", "tests", "docs"] {
            let home = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            fs::create_dir_all(home.path().join("inputs/src")).unwrap();
            fs::create_dir(home.path().join("tests")).unwrap();
            fs::create_dir(home.path().join("docs")).unwrap();
            fs::write(home.path().join("inputs/src/inside.qli"), "inside").unwrap();
            fs::create_dir(outside.path().join("src")).unwrap();
            fs::write(outside.path().join("outside.qli"), "outside").unwrap();
            fs::write(outside.path().join("src/outside.qli"), "outside").unwrap();
            fs::write(home.path().join("Qargo.toml"), "schema-version = 2\n[qrate]\nname = \"race\"\nversion = \"0.1.0\"\nedition = \"2026\"\n[source]\nroot = \"inputs/src\"\n[tests]\nroot = \"tests\"\n[docs]\nroot = \"docs\"\n").unwrap();
            let result = capture_with_hook(Some(&home.path().join("Qargo.toml")), |directory| {
                fs::rename(directory.join(replaced), directory.join("old")).unwrap();
                symlink(outside.path(), directory.join(replaced)).unwrap();
            });
            assert!(
                result.is_err(),
                "captured a replaced declared root: {replaced}"
            );
        }
    }
}
