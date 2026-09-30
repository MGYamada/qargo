//! Bounded immutable input capture with portable, byte-exact identities.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::report::Diagnostic;

pub type Files = BTreeMap<String, Vec<u8>>;
pub const FILE_BYTES: u64 = 1 << 20;
pub const TOTAL_BYTES: usize = 16 << 20;
const FILE_COUNT: usize = 4096;
const DIRECTORY_DEPTH: usize = 64;

fn input_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("input", "qargo", message)
}

/// Open only an ordinary file. O_NOFOLLOW protects the final component on Unix.
pub fn read_regular(path: &Path) -> Result<Vec<u8>, Diagnostic> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| input_error(format!("Cannot inspect input file: {error}")))?;
    if !metadata.file_type().is_file() {
        return Err(input_error(
            "Input files must be regular files, never symlinks or special files.",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|error| input_error(format!("Cannot open input file: {error}")))?;
    if !file
        .metadata()
        .map_err(|error| input_error(error.to_string()))?
        .is_file()
    {
        return Err(input_error("Input changed to a nonregular file."));
    }
    let mut bytes = Vec::new();
    file.take(FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| input_error(format!("Cannot read input file: {error}")))?;
    if bytes.len() as u64 > FILE_BYTES {
        return Err(Diagnostic::error(
            "limit",
            "qargo",
            "Input exceeds the 1 MiB file limit.",
        ));
    }
    Ok(bytes)
}

pub fn portable_relative(path: &Path) -> Result<String, Diagnostic> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| input_error("Input path is not UTF-8."))?;
                if part.contains('\\') || part.contains('\0') {
                    return Err(input_error(
                        "Input paths cannot contain backslashes or NUL.",
                    ));
                }
                parts.push(part);
            }
            _ => {
                return Err(input_error(
                    "Expected a nonempty relative input path without parent components.",
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(input_error("Expected a nonempty relative input path."));
    }
    let label = parts.join("/");
    if label.len() > 4096 {
        return Err(Diagnostic::error(
            "limit",
            "qargo",
            "Input path exceeds 4096 UTF-8 bytes.",
        ));
    }
    Ok(label)
}

pub fn collect_tree(root: &Path, extension: Option<&str>) -> Result<Files, Diagnostic> {
    // Parent aliases such as /tmp are permitted; the selected root itself cannot be a symlink.
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| input_error(format!("Cannot inspect input root: {error}")))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(input_error(
            "Input root must be a directory and cannot be a symlink.",
        ));
    }
    let root = fs::canonicalize(root)
        .map_err(|error| input_error(format!("Cannot resolve input root: {error}")))?;
    let mut pending = vec![(root.clone(), 0usize)];
    let mut files = Files::new();
    let mut total = 0usize;
    let mut visited = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        if depth > DIRECTORY_DEPTH {
            return Err(Diagnostic::error(
                "limit",
                "qargo",
                "Input directory depth exceeds 64.",
            ));
        }
        let metadata =
            fs::symlink_metadata(&directory).map_err(|error| input_error(error.to_string()))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(input_error("Input directory changed or is a symlink."));
        }
        let entries = fs::read_dir(&directory)
            .map_err(|error| input_error(format!("Cannot enumerate input directory: {error}")))?;
        for entry in entries {
            let entry = entry.map_err(|error| input_error(error.to_string()))?;
            visited += 1;
            if visited > FILE_COUNT {
                return Err(Diagnostic::error(
                    "limit",
                    "qargo",
                    "Input contains more than 4096 entries.",
                ));
            }
            let path = entry.path();
            let label = portable_relative(
                path.strip_prefix(&root)
                    .map_err(|_| input_error("Input escaped its root."))?,
            )?;
            let kind = entry
                .file_type()
                .map_err(|error| input_error(error.to_string()))?;
            if kind.is_symlink() {
                return Err(input_error(format!(
                    "Symlink input is not allowed: {label}"
                )));
            }
            if kind.is_dir() {
                pending.push((path, depth + 1));
            } else if kind.is_file() {
                if extension.is_some_and(|suffix| {
                    path.extension().and_then(|part| part.to_str()) != Some(suffix)
                }) {
                    continue;
                }
                let bytes = read_regular(&path)?;
                total = total
                    .checked_add(bytes.len())
                    .ok_or_else(|| input_error("Input byte accounting overflow."))?;
                if total > TOTAL_BYTES {
                    return Err(Diagnostic::error(
                        "limit",
                        "qargo",
                        "Input exceeds the 16 MiB total limit.",
                    ));
                }
                files.insert(label, bytes);
            } else {
                return Err(input_error(format!(
                    "Special input file is not allowed: {label}"
                )));
            }
        }
    }
    Ok(files)
}

pub fn digest_files(domain: &str, files: &Files) -> String {
    let mut digest = Sha256::new();
    digest.update((domain.len() as u64).to_be_bytes());
    digest.update(domain.as_bytes());
    digest.update((files.len() as u64).to_be_bytes());
    for (path, bytes) in files {
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    format!("sha256:{:x}", digest.finalize())
}

pub fn digest_path(path: &Path) -> Result<String, Diagnostic> {
    let mut file = File::open(path).map_err(|error| {
        Diagnostic::error(
            "tool_identity",
            "tool",
            format!("Cannot read executable: {error}"),
        )
    })?;
    if !file
        .metadata()
        .map_err(|error| input_error(error.to_string()))?
        .is_file()
    {
        return Err(Diagnostic::error(
            "tool_identity",
            "tool",
            "Executable is not a regular file.",
        ));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| Diagnostic::error("tool_identity", "tool", error.to_string()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

pub fn materialize(files: &Files) -> Result<TempDir, Diagnostic> {
    let directory = tempfile::Builder::new()
        .prefix("qargo-snapshot-")
        .tempdir()
        .map_err(|error| input_error(format!("Cannot create private snapshot: {error}")))?;
    for (label, bytes) in files {
        if portable_relative(Path::new(label))? != *label {
            return Err(input_error("Snapshot input path is not canonical."));
        }
        let path = directory.path().join(label);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| input_error(error.to_string()))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| input_error(format!("Cannot materialize snapshot file: {error}")))?;
        file.write_all(bytes)
            .map_err(|error| input_error(error.to_string()))?;
    }
    Ok(directory)
}

pub struct FrozenSources {
    pub files: Files,
    pub source_id: String,
    _directory: TempDir,
    canonical_root: PathBuf,
}

impl FrozenSources {
    pub fn capture(root: &Path) -> Result<Self, Diagnostic> {
        Self::from_files(collect_tree(root, Some("qli"))?)
    }

    pub fn from_files(files: Files) -> Result<Self, Diagnostic> {
        let mut total = 0usize;
        if files.len() > FILE_COUNT {
            return Err(Diagnostic::error(
                "limit",
                "qargo",
                "Too many source files.",
            ));
        }
        for (label, bytes) in &files {
            if !label.ends_with(".qli") || portable_relative(Path::new(label))? != *label {
                return Err(input_error(
                    "Source snapshot contains an invalid .qli path.",
                ));
            }
            if bytes.len() as u64 > FILE_BYTES {
                return Err(Diagnostic::error(
                    "limit",
                    "qargo",
                    "Source exceeds the 1 MiB file limit.",
                ));
            }
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| input_error("Source byte accounting overflow."))?;
            if total > TOTAL_BYTES {
                return Err(Diagnostic::error(
                    "limit",
                    "qargo",
                    "Source exceeds the 16 MiB total limit.",
                ));
            }
        }
        let source_id = digest_files("qleisli.source.v1", &files);
        let directory = materialize(&files)?;
        let canonical_root = fs::canonicalize(directory.path())
            .map_err(|error| input_error(format!("Cannot resolve private snapshot: {error}")))?;
        Ok(Self {
            files,
            source_id,
            _directory: directory,
            canonical_root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.canonical_root
    }
    pub fn count(&self) -> usize {
        self.files.len()
    }
}
