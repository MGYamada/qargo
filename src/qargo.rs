//! Local qrate orchestration. Rust development tools are not qrate backends.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::adapter;
use crate::report::{Diagnostic, Envelope, Report, path_argument};
use crate::snapshot::{self, Files, FrozenSources};
use crate::{PROFILE, QLEISLI_VERSION, VERSION};

const FORMAT: &str = "qargo.result";
const HELP: &str = "qargo check|build|test [--manifest-path=PATH] [--format=json]\nqargo lint [source-root] [--manifest-path=PATH] [--qlippy=PATH] [--deny-warnings] [--format=json]\nqargo fmt [source-root] [--manifest-path=PATH] [--qlifmt=PATH] [--check] [--format=json]\nqargo doc [--manifest-path=PATH] [--qlidoc=PATH] [--document-private-items] [--format=json]\nqargo --help|--version [--format=json]\nPath options also accept --option PATH.\nUse qlippy --list-rules to inspect advisory rule policy.\nQargo manages Qleisli qrates. Cargo builds and installs Rust tools outside Qargo commands.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(rename = "schema-version")]
    schema_version: u32,
    qrate: Qrate,
    source: Root,
    tests: Root,
    docs: Root,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Qrate {
    name: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Root {
    root: String,
}

struct CapturedQrate {
    directory: PathBuf,
    manifest: Manifest,
    files: Files,
    input_id: String,
    sources: FrozenSources,
}

#[derive(Default)]
struct Options {
    command: String,
    manifest: Option<PathBuf>,
    source: Option<PathBuf>,
    qlippy: Option<PathBuf>,
    qlifmt: Option<PathBuf>,
    qlidoc: Option<PathBuf>,
    deny_warnings: bool,
    check: bool,
    document_private_items: bool,
}

// Typed decoding rejects repeated fields as well as invalid field types, while
// the raw-object checks below also require explicit nullable fields.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChildEnvelope {
    format: String,
    version: u32,
    command: String,
    outcome: String,
    diagnostics: Vec<Diagnostic>,
    result: Option<ChildResult>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChildResult {
    source_count: usize,
    source_id: String,
    qleisli_check: ChildStep,
    tool: ChildTool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChildStep {
    status: String,
    reason: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChildTool {
    name: String,
    version: String,
    executable_sha256: String,
    qleisli_version: String,
    profile: String,
}

fn error(id: &str, category: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(id, category, message)
}

fn usage(message: impl Into<String>) -> Diagnostic {
    error("invalid_arguments", "usage", message)
}

fn parse(args: &[OsString]) -> Result<Options, Diagnostic> {
    let mut options = Options::default();
    let mut format_seen = false;
    let mut remaining = args.iter();
    while let Some(arg) = remaining.next() {
        let arg = arg
            .to_str()
            .ok_or_else(|| usage("Arguments must be UTF-8."))?;
        if arg == "--format=json" {
            if format_seen {
                return Err(usage("--format=json may only be supplied once."));
            }
            format_seen = true;
        } else if arg == "--help" || arg == "--version" {
            if !options.command.is_empty() {
                return Err(usage(
                    "--help and --version must be used without a command.",
                ));
            }
            options.command = arg[2..].into();
        } else if arg == "--manifest-path" || arg.starts_with("--manifest-path=") {
            if options.manifest.is_some() {
                return Err(usage("--manifest-path requires one nonempty path."));
            }
            options.manifest =
                Some(path_argument("--manifest-path", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--qlippy" || arg.starts_with("--qlippy=") {
            if options.qlippy.is_some() {
                return Err(usage("--qlippy requires one nonempty path."));
            }
            options.qlippy = Some(path_argument("--qlippy", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--qlifmt" || arg.starts_with("--qlifmt=") {
            if options.qlifmt.is_some() {
                return Err(usage("--qlifmt requires one nonempty path."));
            }
            options.qlifmt = Some(path_argument("--qlifmt", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--qlidoc" || arg.starts_with("--qlidoc=") {
            if options.qlidoc.is_some() {
                return Err(usage("--qlidoc requires one nonempty path."));
            }
            options.qlidoc = Some(path_argument("--qlidoc", arg, &mut remaining).map_err(usage)?);
        } else if arg == "--check" {
            if options.check {
                return Err(usage("--check may only be supplied once."));
            }
            options.check = true;
        } else if arg == "--document-private-items" {
            if options.document_private_items {
                return Err(usage("--document-private-items may only be supplied once."));
            }
            options.document_private_items = true;
        } else if arg == "--deny-warnings" {
            if options.deny_warnings {
                return Err(usage("--deny-warnings may only be supplied once."));
            }
            options.deny_warnings = true;
        } else if arg.starts_with('-') {
            return Err(usage(format!("Unknown option: {arg}")));
        } else if options.command.is_empty() {
            if !["check", "build", "lint", "fmt", "test", "doc"].contains(&arg) {
                let mut diagnostic = usage(format!("Unknown command: {arg}"));
                diagnostic.suggestion = Some(if ["add", "remove", "update", "install", "publish"].contains(&arg) {
                    "Qargo has no qrate registry or dependency management yet. Use a local Qargo.toml with check, build, lint, fmt, doc, or test. To install the Rust tools, use Cargo outside Qargo operations."
                } else {
                    "Use qargo --help to see supported commands."
                }.into());
                return Err(diagnostic);
            }
            options.command = arg.into();
        } else if ["lint", "fmt"].contains(&options.command.as_str())
            && options.source.is_none()
            && !arg.is_empty()
        {
            options.source = Some(PathBuf::from(arg));
        } else {
            return Err(usage(format!("Unexpected argument: {arg}")));
        }
    }
    if options.command.is_empty() {
        return Err(usage("A command is required. Use qargo --help."));
    }
    if options.command != "lint" && (options.qlippy.is_some() || options.deny_warnings) {
        return Err(usage("--qlippy and --deny-warnings apply only to lint."));
    }
    if options.command != "fmt" && (options.qlifmt.is_some() || options.check) {
        return Err(usage("--qlifmt and --check apply only to fmt."));
    }
    if options.command != "doc" && (options.qlidoc.is_some() || options.document_private_items) {
        return Err(usage(
            "--qlidoc and --document-private-items apply only to doc.",
        ));
    }
    if ["help", "version"].contains(&options.command.as_str()) && options.manifest.is_some() {
        return Err(usage("--manifest-path requires a qrate command."));
    }
    Ok(options)
}

fn discover(explicit: Option<&Path>) -> Result<PathBuf, Diagnostic> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| {
        error(
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
                return Err(error(
                    "manifest_discovery",
                    "qargo",
                    format!("Cannot inspect Qargo.toml: {e}"),
                ));
            }
        }
    }
    Err(error(
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
        return Err(error(
            "invalid_manifest",
            "qargo",
            format!("Invalid qrate-relative root: {value}"),
        ));
    }
    Ok(path.to_path_buf())
}

fn validate_manifest(manifest: &Manifest, directory: &Path) -> Result<(), Diagnostic> {
    if manifest.schema_version != 1 {
        return Err(error(
            "unsupported_manifest_version",
            "qargo",
            "Only manifest schema-version 1 is supported.",
        ));
    }
    let name = &manifest.qrate.name;
    if !name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(error(
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
        return Err(error(
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
    for (index, root) in roots.iter().enumerate() {
        if roots
            .iter()
            .enumerate()
            .any(|(other_index, other)| index != other_index && root.starts_with(other))
        {
            return Err(error(
                "invalid_manifest",
                "qargo",
                "Source, test and documentation roots must not overlap.",
            ));
        }
        let mut path = directory.to_path_buf();
        for component in root.components() {
            path.push(component.as_os_str());
            let metadata = fs::symlink_metadata(&path).map_err(|e| {
                error(
                    "invalid_manifest",
                    "qargo",
                    format!("Cannot inspect declared root {root:?}: {e}"),
                )
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(error(
                    "invalid_manifest",
                    "qargo",
                    "Every declared root and its ancestors must be existing directories without symlinks.",
                ));
            }
        }
    }
    Ok(())
}

fn capture(explicit: Option<&Path>) -> Result<CapturedQrate, Diagnostic> {
    let path = discover(explicit)?;
    let raw = snapshot::read_regular(&path)?;
    let text = std::str::from_utf8(&raw)
        .map_err(|_| error("invalid_manifest", "qargo", "Qargo.toml must be UTF-8."))?;
    let parsed: toml::Value = toml::from_str(text).map_err(|e| {
        error(
            "invalid_manifest",
            "qargo",
            format!("Invalid Qargo.toml: {e}"),
        )
    })?;
    if parsed.get("dependencies").is_some() || parsed.get("dev-dependencies").is_some() {
        let mut diagnostic = error(
            "invalid_manifest",
            "qargo",
            "Qargo manifest schema 1 does not support dependency tables.",
        );
        diagnostic.suggestion = Some(
            "Remove [dependencies] and [dev-dependencies] from Qargo.toml. Use local Qleisli source modules; keep Rust engine dependencies in developer Cargo.toml outside the qrate. Qrate dependency resolution is deferred.".into(),
        );
        return Err(diagnostic);
    }
    // Decode the original text to preserve TOML field types; Value converts datetimes to strings.
    let manifest: Manifest = toml::from_str(text).map_err(|e| {
        error(
            "invalid_manifest",
            "qargo",
            format!("Invalid Qargo.toml: {e}"),
        )
    })?;
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = fs::canonicalize(directory).map_err(|e| {
        error(
            "invalid_manifest",
            "qargo",
            format!("Cannot resolve qrate directory: {e}"),
        )
    })?;
    validate_manifest(&manifest, &directory)?;
    let mut files = BTreeMap::from([("Qargo.toml".into(), raw)]);
    for root in [
        &manifest.source.root,
        &manifest.tests.root,
        &manifest.docs.root,
    ] {
        for (path, bytes) in snapshot::collect_tree(&directory.join(root), None)? {
            files.insert(format!("{root}/{path}"), bytes);
        }
    }
    if files.len() > 4096 || files.values().map(Vec::len).sum::<usize>() > 16 * 1024 * 1024 {
        return Err(error(
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

fn remap_qrate(mut diagnostic: Diagnostic, qrate: &CapturedQrate) -> Diagnostic {
    if let Some(location) = diagnostic.primary.as_mut() {
        if qrate.sources.files.contains_key(&location.path) {
            location.path = format!("{}/{}", qrate.manifest.source.root, location.path);
        }
    }
    diagnostic
}

fn failed(command: &str, diagnostic: Diagnostic, result: Option<Value>) -> Report {
    Report {
        envelope: Envelope {
            format: FORMAT.into(),
            version: 1,
            command: command.into(),
            outcome: "error".into(),
            diagnostics: vec![diagnostic],
            result,
        },
        exit_code: 1,
    }
}

fn safe_output_directory(path: &Path) -> Result<(), Diagnostic> {
    if let Err(e) = fs::create_dir(path) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error(
                "build_output",
                "qargo",
                format!("Cannot create build directory: {e}"),
            ));
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| {
        error(
            "build_output",
            "qargo",
            format!("Cannot inspect build directory: {e}"),
        )
    })?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(error(
            "unsafe_output",
            "qargo",
            "Build output paths must be directories without symlinks.",
        ))
    }
}

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
            Path::new("snapshot").join(&qrate.manifest.source.root),
            Path::new("snapshot").join(&qrate.manifest.tests.root),
            Path::new("snapshot").join(&qrate.manifest.docs.root),
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

pub(crate) fn artifacts_match(
    base: &Path,
    artifacts: &Files,
    directories: &BTreeSet<PathBuf>,
) -> Result<bool, Diagnostic> {
    let metadata = fs::symlink_metadata(base).map_err(|e| {
        error(
            "build_output",
            "qargo",
            format!("Cannot inspect stored artifacts: {e}"),
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    let mut pending = vec![PathBuf::new()];
    let mut seen_files = BTreeSet::new();
    let mut seen_directories = BTreeSet::new();
    while let Some(relative) = pending.pop() {
        let entries = fs::read_dir(base.join(&relative)).map_err(|e| {
            error(
                "build_output",
                "qargo",
                format!("Cannot read stored artifacts: {e}"),
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                error(
                    "build_output",
                    "qargo",
                    format!("Cannot read stored artifact entry: {e}"),
                )
            })?;
            let path = relative.join(entry.file_name());
            let metadata = fs::symlink_metadata(entry.path()).map_err(|e| {
                error(
                    "build_output",
                    "qargo",
                    format!("Cannot inspect stored artifact entry: {e}"),
                )
            })?;
            if metadata.file_type().is_symlink() {
                return Ok(false);
            }
            if metadata.is_dir() {
                if !directories.contains(&path) {
                    return Ok(false);
                }
                seen_directories.insert(path.clone());
                pending.push(path);
            } else if metadata.is_file() {
                let Some(key) = path.to_str().map(|path| path.replace('\\', "/")) else {
                    return Ok(false);
                };
                let Some(expected) = artifacts.get(&key) else {
                    return Ok(false);
                };
                if metadata.len() != expected.len() as u64 {
                    return Ok(false);
                }
                let file = fs::File::open(entry.path()).map_err(|e| {
                    error(
                        "build_output",
                        "qargo",
                        format!("Cannot read stored artifact: {e}"),
                    )
                })?;
                let mut bytes = Vec::with_capacity(expected.len());
                file.take(expected.len() as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| {
                        error(
                            "build_output",
                            "qargo",
                            format!("Cannot read stored artifact: {e}"),
                        )
                    })?;
                if &bytes != expected {
                    return Ok(false);
                }
                seen_files.insert(key);
            } else {
                return Ok(false);
            }
        }
    }
    Ok(seen_files.len() == artifacts.len() && &seen_directories == directories)
}

fn build_artifacts(
    qrate: &CapturedQrate,
    checked: &adapter::CheckedSources,
    tool: &Value,
) -> Result<String, Diagnostic> {
    let record = json!({
        "format":"qargo.build-record", "version":1, "input_id":qrate.input_id,
        "qrate":{"name":qrate.manifest.qrate.name,"version":qrate.manifest.qrate.version},
        "source_count":checked.source_count, "qleisli_check":checked.qleisli_check,
        "profile":PROFILE, "tool":tool,
        "steps":[{"name":"manifest","status":"passed","reason":null},
                 {"name":"snapshot","status":"passed","reason":null},
                 {"name":"qleisli","status":checked.qleisli_check["status"],"reason":checked.qleisli_check["reason"]},
                 {"name":"QLT","status":"not_run","reason":"backend_unavailable"},
                 {"name":"qlidoc","status":"not_run","reason":"not_requested"},
                 {"name":"build","status":"passed","reason":null}]
    });
    let mut artifacts: Files = qrate
        .files
        .iter()
        .map(|(path, bytes)| (format!("snapshot/{path}"), bytes.clone()))
        .collect();
    artifacts.insert(
        "module-index.json".into(),
        json_bytes(&checked.module_index)?,
    );
    artifacts.insert("build-record.json".into(), json_bytes(&record)?);
    let directories = artifact_directories(qrate, &artifacts);
    let target = qrate.directory.join("target");
    safe_output_directory(&target)?;
    let output = target.join("qargo");
    safe_output_directory(&output)?;
    let hex = qrate
        .input_id
        .strip_prefix("sha256:")
        .ok_or_else(|| error("build_output", "qargo", "Invalid input identity."))?;
    let tool_hex = tool["executable_sha256"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| error("tool_identity", "tool", "Invalid executable identity."))?;
    let base = output.join(hex);
    safe_output_directory(&base)?;
    let destination = base.join(tool_hex);
    let relative = format!("target/qargo/{hex}/{tool_hex}");
    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || !artifacts_match(&destination, &artifacts, &directories)?
            {
                return Err(error(
                    "artifact_mismatch",
                    "qargo",
                    "Existing build artifacts are inconsistent with the captured inputs and record; nothing was overwritten.",
                ));
            }
            return Ok(relative);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(error(
                "build_output",
                "qargo",
                format!("Cannot inspect artifact directory: {e}"),
            ));
        }
    }
    let staging = tempfile::Builder::new()
        .prefix(".qargo-stage-")
        .tempdir_in(&base)
        .map_err(|e| {
            error(
                "build_output",
                "qargo",
                format!("Cannot create staging directory: {e}"),
            )
        })?;
    for directory in &directories {
        fs::create_dir_all(staging.path().join(directory)).map_err(|e| {
            error(
                "build_output",
                "qargo",
                format!("Cannot create snapshot root: {e}"),
            )
        })?;
    }
    for (path, bytes) in &artifacts {
        let output = staging.path().join(path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                error(
                    "build_output",
                    "qargo",
                    format!("Cannot create artifact parent: {e}"),
                )
            })?;
        }
        fs::write(output, bytes).map_err(|e| {
            error(
                "build_output",
                "qargo",
                format!("Cannot write artifact: {e}"),
            )
        })?;
    }
    match crate::installation::rename_noreplace(staging.path(), &destination) {
        Ok(()) => Ok(relative),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another builder won publication. Reuse only an identical complete result.
            if artifacts_match(&destination, &artifacts, &directories)? {
                Ok(relative)
            } else {
                Err(error(
                    "artifact_mismatch",
                    "qargo",
                    "A conflicting artifact directory appeared during build; nothing was overwritten.",
                ))
            }
        }
        Err(e) => Err(error(
            "build_output",
            "qargo",
            format!("Cannot atomically install artifacts: {e}"),
        )),
    }
}

fn executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        if !metadata.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

fn find_tool(tool_name: &str, explicit: Option<&Path>) -> Result<PathBuf, Diagnostic> {
    if let Some(path) = explicit {
        if executable(path) {
            return fs::canonicalize(path).map_err(|e| {
                error(
                    "tool_missing",
                    "tool",
                    format!("Cannot resolve selected {tool_name}: {e}"),
                )
            });
        }
        return Err(error(
            "tool_missing",
            "tool",
            format!("The explicit {tool_name} path is not an executable file."),
        ));
    }
    let name = if cfg!(windows) {
        format!("{tool_name}.exe")
    } else {
        tool_name.to_owned()
    };
    if let Ok(current) = std::env::current_exe() {
        if let Some(directory) = current.parent() {
            let sibling = directory.join(&name);
            if executable(&sibling) {
                return Ok(sibling);
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(&name);
            if executable(&candidate) {
                return fs::canonicalize(&candidate).map_err(|e| {
                    error(
                        "tool_missing",
                        "tool",
                        format!("Cannot resolve {tool_name} on PATH: {e}"),
                    )
                });
            }
        }
    }
    Err(error(
        "tool_missing",
        "tool",
        format!(
            "{tool_name} was not found beside qargo or on PATH. Build the Rust engine during development or supply --{tool_name}=PATH."
        ),
    ))
}

pub(crate) fn closed_object(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field))
    })
}

fn step_valid(step: &Value, source_count: usize) -> bool {
    closed_object(step, &["status", "reason"])
        && if source_count == 0 {
            step["status"] == "not_run" && step["reason"] == "no_sources"
        } else {
            (step["status"] == "passed" && step["reason"].is_null())
                || step["status"] == "failed"
                    && (step["reason"].is_null() || step["reason"].is_string())
        }
}

pub(crate) fn transport(message: impl Into<String>) -> Diagnostic {
    error("invalid_tool_response", "tool", message)
}

fn bundled_sources() -> Option<&'static Files> {
    static BUNDLED: std::sync::OnceLock<Option<Files>> = std::sync::OnceLock::new();
    BUNDLED
        .get_or_init(|| {
            let empty = tempfile::tempdir().ok()?;
            let project = qleisli::frontend::project::Project::load_with_policy(
                empty.path(),
                qleisli::frontend::project::SourcePolicy::default(),
            )
            .ok()?;
            project
                .modules
                .values()
                .filter(|module| module.origin == qleisli::frontend::project::ModuleOrigin::Bundled)
                .map(|module| {
                    let relative = module.path.strip_prefix("<bundled>/std").ok()?.to_str()?;
                    Some((
                        format!("std://{}", relative.replace('\\', "/")),
                        module.source.as_bytes().to_vec(),
                    ))
                })
                .collect::<Option<Files>>()
        })
        .as_ref()
}

pub(crate) fn location_valid(location: &Value, sources: &FrozenSources, compiler: bool) -> bool {
    if location.is_null() {
        return compiler;
    }
    if !closed_object(location, &["path", "start", "end", "line", "column"]) {
        return false;
    }
    let Some(path) = location["path"].as_str() else {
        return false;
    };
    let Some(start) = location["start"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    let Some(end) = location["end"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    let Some(line) = location["line"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    let Some(column) = location["column"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    if start > end || line == 0 || column == 0 {
        return false;
    }
    let bytes = if compiler && path.starts_with("std://") {
        bundled_sources().and_then(|sources| sources.get(path))
    } else {
        sources.files.get(path)
    };
    let Some(bytes) = bytes else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    if end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return false;
    }
    let (expected_line, expected_column) = crate::report::coordinates(text, start);
    line == expected_line && column == expected_column
}

fn child_response(
    bytes: &[u8],
    status: i32,
    sources: &FrozenSources,
    digest: &str,
    deny: bool,
) -> Result<Envelope, Diagnostic> {
    let _: ChildEnvelope = serde_json::from_slice(bytes).map_err(|_| {
        transport("qlippy returned repeated fields, unsupported fields or invalid field types.")
    })?;
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| transport("qlippy did not emit a complete JSON response."))?;
    if !closed_object(
        &value,
        &[
            "format",
            "version",
            "command",
            "outcome",
            "diagnostics",
            "result",
        ],
    ) || value["format"] != "qlippy.result"
        || value["version"] != 1
        || value["command"] != "lint"
        || !["ok", "error"].contains(&value["outcome"].as_str().unwrap_or(""))
    {
        return Err(transport(
            "qlippy returned an unsupported or malformed envelope.",
        ));
    }
    let Some(diagnostics) = value["diagnostics"].as_array() else {
        return Err(transport("qlippy diagnostics must be an array."));
    };
    for diagnostic in diagnostics {
        if !closed_object(
            diagnostic,
            &[
                "id",
                "category",
                "severity",
                "primary",
                "message",
                "suggestion",
            ],
        ) || !diagnostic["message"].is_string()
            || !(diagnostic["suggestion"].is_null() || diagnostic["suggestion"].is_string())
        {
            return Err(transport("qlippy returned a malformed diagnostic."));
        }
        let category = diagnostic["category"].as_str().unwrap_or("");
        let id = diagnostic["id"].as_str().unwrap_or("");
        let severity = diagnostic["severity"].as_str().unwrap_or("");
        if category == "lint" {
            if crate::rules::find(id).is_none() || severity != "warning" {
                return Err(transport(
                    "qlippy returned an unknown lint rule or severity.",
                ));
            }
        } else if category != "compiler"
            || ![
                "project",
                "parse",
                "unknown_name",
                "recursive_call",
                "type_mismatch",
                "arity",
                "ownership",
                "effect",
                "invalid_entry",
                "unsupported",
                "limit",
                "invalid_ir",
                "capability",
                "contract",
            ]
            .contains(&id)
            || severity != "error"
        {
            return Err(transport(
                "qlippy returned an unexpected diagnostic category.",
            ));
        }
        if !location_valid(&diagnostic["primary"], sources, category == "compiler") {
            return Err(transport(
                "qlippy diagnostic locations do not match the captured source bytes.",
            ));
        }
    }
    let result = &value["result"];
    if result.is_null() {
        return Err(transport(
            "qlippy omitted the captured source and tool identity binding.",
        ));
    }
    if !closed_object(
        result,
        &["source_count", "source_id", "qleisli_check", "tool"],
    ) || result["source_count"].as_u64() != Some(sources.count() as u64)
        || result["source_id"] != sources.source_id
        || !step_valid(&result["qleisli_check"], sources.count())
    {
        return Err(transport(
            "qlippy results are not bound to the captured sources.",
        ));
    }
    let tool = &result["tool"];
    if !closed_object(
        tool,
        &[
            "name",
            "version",
            "executable_sha256",
            "qleisli_version",
            "profile",
        ],
    ) || tool["name"] != "qlippy"
        || tool["version"] != VERSION
        || tool["qleisli_version"] != QLEISLI_VERSION
        || tool["profile"] != PROFILE
        || tool["executable_sha256"] != digest
    {
        return Err(transport(
            "qlippy tool identity or profile is incompatible.",
        ));
    }
    let errors = diagnostics
        .iter()
        .any(|diagnostic| diagnostic["severity"] == "error");
    let warnings = diagnostics
        .iter()
        .any(|diagnostic| diagnostic["severity"] == "warning");
    let outcome = value["outcome"].as_str().unwrap_or("");
    if (outcome == "ok"
        && (status != 0
            || errors
            || deny && warnings
            || result["qleisli_check"]["status"] == "failed"))
        || (outcome == "error" && (status != 1 || !(errors || deny && warnings)))
        || (errors && result["qleisli_check"]["status"] != "failed")
        || (result["qleisli_check"]["status"] == "failed" && (!errors || warnings))
    {
        return Err(transport(
            "qlippy exit status, outcome and diagnostics are inconsistent.",
        ));
    }
    serde_json::from_value(value)
        .map_err(|_| transport("qlippy returned malformed typed diagnostics."))
}

pub(crate) fn bounded_output(command: &mut Command) -> Result<(Vec<u8>, i32), Diagnostic> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = command.spawn().map_err(|e| {
        error(
            "tool_execution",
            "tool",
            format!("Cannot execute selected tool: {e}"),
        )
    })?;
    let stdout = child.stdout.take().expect("piped child stdout");
    let stderr = child.stderr.take().expect("piped child stderr");
    let (sender, receiver) = std::sync::mpsc::channel();
    for (is_stdout, stream, limit) in [
        (
            true,
            Box::new(stdout) as Box<dyn Read + Send>,
            4 * 1024 * 1024,
        ),
        (false, Box::new(stderr) as Box<dyn Read + Send>, 64 * 1024),
    ] {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stream
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
                .map_err(|e| format!("Cannot read selected tool output: {e}"));
            let _ = sender.send((is_stdout, limit as usize, result));
        });
    }
    drop(sender);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut stdout = Vec::new();
    for _ in 0..2 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let received = receiver.recv_timeout(remaining);
        let result = match received {
            Ok((is_stdout, limit, Ok(bytes))) if bytes.len() <= limit => {
                if !is_stdout && !bytes.is_empty() {
                    Err(transport(
                        "The selected tool wrote unexpected stderr output.",
                    ))
                } else {
                    if is_stdout {
                        stdout = bytes;
                    }
                    Ok(())
                }
            }
            Ok((_, _, Ok(_))) => Err(transport(
                "The selected tool output exceeds the transport size budget.",
            )),
            Ok((_, _, Err(message))) => Err(transport(message)),
            Err(_) => Err(error(
                "tool_execution",
                "tool",
                "The selected tool exceeded the 30-second execution limit.",
            )),
        };
        if let Err(diagnostic) = result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(diagnostic);
        }
    }
    // A tool can close its output streams and remain alive; polling retains the deadline.
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error(
                    "tool_execution",
                    "tool",
                    "The selected tool exceeded the 30-second execution limit.",
                ));
            }
            Err(e) => {
                return Err(error(
                    "tool_execution",
                    "tool",
                    format!("Cannot wait for selected tool: {e}"),
                ));
            }
        }
    };
    let code = status.code().ok_or_else(|| {
        error(
            "tool_execution",
            "tool",
            "The selected tool was terminated without an exit code.",
        )
    })?;
    Ok((stdout, code))
}

fn lint(options: &Options) -> Result<Report, Diagnostic> {
    let qrate = if options.source.is_none() || options.manifest.is_some() {
        Some(capture(options.manifest.as_deref())?)
    } else {
        None
    };
    let standalone;
    let sources = if let Some(source) = &options.source {
        standalone = FrozenSources::capture(source)?;
        &standalone
    } else {
        &qrate
            .as_ref()
            .expect("qrate captured for default lint source")
            .sources
    };
    let tool_path = find_tool("qlippy", options.qlippy.as_deref())?;
    let digest = snapshot::digest_path(&tool_path)?;
    let mut command = Command::new(&tool_path);
    command.arg(sources.root()).arg("--format=json");
    if options.deny_warnings {
        command.arg("--deny-warnings");
    }
    let (stdout, status) = bounded_output(&mut command)?;
    let mut envelope = child_response(&stdout, status, sources, &digest, options.deny_warnings)?;
    if snapshot::digest_path(&tool_path)? != digest {
        return Err(transport(
            "The selected qlippy executable changed during execution.",
        ));
    }
    if options.source.is_none() {
        if let Some(qrate) = &qrate {
            envelope.diagnostics = envelope
                .diagnostics
                .into_iter()
                .map(|diagnostic| remap_qrate(diagnostic, qrate))
                .collect();
        }
    }
    envelope.format = FORMAT.into();
    if let Some(result) = envelope.result.as_mut() {
        result["orchestrator"] = crate::report::tool_info("qargo")?;
        if let Some(qrate) = &qrate {
            result["input_id"] = Value::String(qrate.input_id.clone());
        }
        let compiler_failed = envelope
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.category == "compiler");
        let (lint_status, lint_reason) = if compiler_failed {
            ("not_run", json!("compiler_error"))
        } else if sources.count() == 0 {
            ("not_run", json!("no_sources"))
        } else if status != 0 {
            ("failed", json!("denied_warnings"))
        } else {
            ("passed", Value::Null)
        };
        result["steps"] = json!([
            {"name":"snapshot","status":"passed","reason":null},
            {"name":"qleisli","status":result["qleisli_check"]["status"],"reason":result["qleisli_check"]["reason"]},
            {"name":"lint","status":lint_status,"reason":lint_reason}
        ]);
    }
    Ok(Report {
        envelope,
        exit_code: status as u8,
    })
}

fn source_tool_report(
    mut report: Report,
    qrate: Option<&CapturedQrate>,
    remap: bool,
) -> Result<Report, Diagnostic> {
    if remap {
        if let Some(qrate) = qrate {
            report.envelope.diagnostics = report
                .envelope
                .diagnostics
                .into_iter()
                .map(|diagnostic| remap_qrate(diagnostic, qrate))
                .collect();
        }
    }
    if let Some(result) = report.envelope.result.as_mut() {
        result["orchestrator"] = crate::report::tool_info("qargo")?;
        if let Some(qrate) = qrate {
            result["input_id"] = json!(qrate.input_id);
        }
    }
    Ok(report)
}

fn format_sources(options: &Options) -> Result<Report, Diagnostic> {
    let qrate = if options.source.is_none() || options.manifest.is_some() {
        Some(capture(options.manifest.as_deref())?)
    } else {
        None
    };
    let standalone;
    let (sources, original_root) = if let Some(root) = &options.source {
        standalone = FrozenSources::capture(root)?;
        let root = fs::canonicalize(root)
            .map_err(|error| error.to_string())
            .map_err(|message| error("input", "qargo", message))?;
        (&standalone, root)
    } else {
        let qrate = qrate.as_ref().expect("default source qrate");
        (
            &qrate.sources,
            qrate.directory.join(&qrate.manifest.source.root),
        )
    };
    let tool = find_tool("qlifmt", options.qlifmt.as_deref())?;
    let digest = snapshot::digest_path(&tool)?;
    let report = crate::bundled::format(sources, &original_root, &tool, &digest, options.check)?;
    source_tool_report(report, qrate.as_ref(), options.source.is_none())
}

fn document_qrate(options: &Options) -> Result<Report, Diagnostic> {
    let qrate = capture(options.manifest.as_deref())?;
    let tool = find_tool("qlidoc", options.qlidoc.as_deref())?;
    let digest = snapshot::digest_path(&tool)?;
    let relative = format!(
        "target/qlidoc/{}/{}/{}",
        qrate.input_id.trim_start_matches("sha256:"),
        digest.trim_start_matches("sha256:"),
        if options.document_private_items {
            "all"
        } else {
            "public"
        }
    );
    let mut report = crate::bundled::document(
        &qrate.sources,
        &tool,
        &digest,
        &qrate.directory.join(&relative),
        options.document_private_items,
    )?;
    if let Some(result) = report.envelope.result.as_mut() {
        if result["artifact_path"].is_string() {
            result["artifact_path"] = json!(relative);
        }
    }
    source_tool_report(report, Some(&qrate), true)
}

fn execute(options: &Options) -> Result<Report, Diagnostic> {
    match options.command.as_str() {
        "help" => return Ok(Report::ok(FORMAT, "help", json!({"help":HELP}))),
        "version" => {
            return Ok(Report::ok(
                FORMAT,
                "version",
                crate::report::tool_info("qargo")?,
            ));
        }
        "lint" => return lint(options),
        "fmt" => return format_sources(options),
        "doc" => return document_qrate(options),
        _ => {}
    }
    let qrate = capture(options.manifest.as_deref())?;
    let tool = crate::report::tool_info("qargo")?;
    if options.command == "test" {
        let backend = "QLT";
        return Ok(failed(
            &options.command,
            error(
                "backend_unavailable",
                "qargo",
                format!("{backend} is not implemented; no substitute backend was run."),
            ),
            Some(json!({
                "input_id":qrate.input_id,"source_count":qrate.sources.count(),"tool":tool,
                "backend":{"name":backend,"status":"unavailable","reason":"not_implemented"}
            })),
        ));
    }
    let checked = match adapter::check(&qrate.sources) {
        Ok(checked) => checked,
        Err(diagnostic) => {
            return Ok(failed(
                &options.command,
                remap_qrate(diagnostic, &qrate),
                Some(json!({
                    "input_id":qrate.input_id,"source_count":qrate.sources.count(),"source_id":qrate.sources.source_id,
                    "qleisli_check":{"status":"failed","reason":"compiler_error"},"tool":tool
                })),
            ));
        }
    };
    let mut result = json!({"input_id":qrate.input_id,"source_count":checked.source_count,"source_id":qrate.sources.source_id,
        "qleisli_check":checked.qleisli_check,"tool":tool});
    if options.command == "build" {
        match build_artifacts(&qrate, &checked, &tool) {
            Ok(path) => result["artifact_path"] = Value::String(path),
            Err(diagnostic) => {
                result["build"] = json!({"status":"failed","reason":diagnostic.id});
                return Ok(failed("build", diagnostic, Some(result)));
            }
        }
    }
    Ok(Report::ok(FORMAT, &options.command, result))
}

/// Run a qargo command using arguments without the executable name.
pub fn run(args: &[OsString]) -> Report {
    let options = match parse(args) {
        Ok(options) => options,
        Err(diagnostic) => return Report::fail(FORMAT, "usage", diagnostic, 2),
    };
    execute(&options)
        .unwrap_or_else(|diagnostic| Report::fail(FORMAT, &options.command, diagnostic, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_compiler_steps_cannot_carry_lint_warnings() {
        let sources = FrozenSources::from_files(BTreeMap::from([(
            "module.qli".into(),
            b"unitary fn identity(q:Q<Bit>)->Q<Bit>{q}".to_vec(),
        )]))
        .unwrap();
        let warning = json!({"id":"unused_import","category":"lint","severity":"warning", "primary":{"path":"module.qli","start":0,"end":1,"line":1,"column":1},"message":"advice","suggestion":null});
        let compiler = json!({"id":"type_mismatch","category":"compiler","severity":"error","primary":null,"message":"rejected","suggestion":null});
        let mut response = json!({"format":"qlippy.result","version":1,"command":"lint","outcome":"error","diagnostics":[warning],
            "result":{"source_count":1,"source_id":sources.source_id,"qleisli_check":{"status":"failed","reason":"compiler_error"},
            "tool":{"name":"qlippy","version":VERSION,"executable_sha256":"digest","qleisli_version":QLEISLI_VERSION,"profile":PROFILE}}});
        assert!(
            child_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_err()
        );
        response["diagnostics"] = json!([compiler.clone(), warning]);
        assert!(
            child_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_err()
        );
        response["diagnostics"] = json!([compiler]);
        assert!(
            child_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_ok()
        );
        let repeated = serde_json::to_string(&response)
            .unwrap()
            .replace("\"version\":1", "\"version\":1,\"version\":1");
        assert!(child_response(repeated.as_bytes(), 1, &sources, "digest", true).is_err());
        response["result"] = Value::Null;
        assert!(
            child_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_err()
        );
    }

    #[test]
    fn bundled_locations_are_bound_to_actual_linked_source_bytes() {
        let sources = FrozenSources::from_files(Files::new()).unwrap();
        let valid = json!({"path":"std://arithmetic.qli","start":0,"end":1,"line":1,"column":1});
        assert!(location_valid(&valid, &sources, true));
        let mut invalid = valid.clone();
        invalid["end"] = json!(usize::MAX);
        assert!(!location_valid(&invalid, &sources, true));
        let mut invalid = valid.clone();
        invalid["line"] = json!(2);
        assert!(!location_valid(&invalid, &sources, true));
        let mut invalid = valid;
        invalid["path"] = json!("std://missing.qli");
        assert!(!location_valid(&invalid, &sources, true));
    }

    #[test]
    fn qrate_check_and_artifacts_use_captured_bytes_after_original_mutation() {
        let directory = tempfile::tempdir().unwrap();
        for root in ["src", "tests", "docs"] {
            fs::create_dir(directory.path().join(root)).unwrap();
        }
        let manifest = "schema-version=1\n[qrate]\nname=\"example\"\nversion=\"0.1.0\"\n[source]\nroot=\"src\"\n[tests]\nroot=\"tests\"\n[docs]\nroot=\"docs\"\n";
        fs::write(directory.path().join("Qargo.toml"), manifest).unwrap();
        let source = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
        fs::write(directory.path().join("src/module.qli"), source).unwrap();
        let frozen = capture(Some(&directory.path().join("Qargo.toml"))).unwrap();
        fs::write(directory.path().join("src/module.qli"), "invalid syntax").unwrap();
        fs::write(directory.path().join("Qargo.toml"), "invalid manifest").unwrap();
        let checked = adapter::check(&frozen.sources).unwrap();
        assert_eq!(checked.qleisli_check["status"], "passed");
        let tool = crate::report::tool_info("qargo").unwrap();
        let artifact = build_artifacts(&frozen, &checked, &tool).unwrap();
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
        let rebuilt_artifact = build_artifacts(&frozen, &checked, &rebuilt_tool).unwrap();
        assert_ne!(artifact, rebuilt_artifact);
        for (path, expected_tool) in [(&artifact, &tool), (&rebuilt_artifact, &rebuilt_tool)] {
            let record: Value = serde_json::from_slice(
                &fs::read(directory.path().join(path).join("build-record.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(record["input_id"], frozen.input_id);
            assert_eq!(&record["tool"], expected_tool);
        }
        assert_eq!(build_artifacts(&frozen, &checked, &tool).unwrap(), artifact);
    }
}
