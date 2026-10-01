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
const SCANNED_ENTRY_COUNT: usize = 4096;
const CAPTURED_FILE_COUNT: usize = 4096;
const DIRECTORY_DEPTH: usize = 64;

fn input_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("input", "qargo", message)
}

/// Read an ordinary file through an opened parent; never follow its final component.
pub fn read_regular(path: &Path) -> Result<Vec<u8>, Diagnostic> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| input_error("Input must name an ordinary file."))?;
    InputDirectory::open(parent)?.read_regular(name)
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

/// Bound traversal by all scanned entries, using the opened root throughout capture.
pub fn collect_tree(root: &Path, extension: Option<&str>) -> Result<Files, Diagnostic> {
    InputDirectory::open(root)?.collect(extension)
}

#[cfg(unix)]
pub use anchored::InputDirectory;

#[cfg(not(unix))]
#[derive(Clone)]
pub struct InputDirectory;

#[cfg(not(unix))]
impl InputDirectory {
    pub fn open(_: &Path) -> Result<Self, Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }
    pub fn child(&self, _: &std::ffi::OsStr) -> Result<Self, Diagnostic> {
        Self::open(Path::new(""))
    }
    pub fn directory_identity_if_exists(
        &self,
        _: &std::ffi::OsStr,
    ) -> Result<Option<(u64, u64)>, Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }
    pub fn identity(&self) -> Result<(u64, u64), Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }
    pub fn read_regular(&self, _: &std::ffi::OsStr) -> Result<Vec<u8>, Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }
    pub fn collect(&self, _: Option<&str>) -> Result<Files, Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }
}

#[cfg(unix)]
mod anchored {
    use std::ffi::{OsStr, OsString};
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::Arc;

    use rustix::fd::OwnedFd;
    use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags, Stat};

    use super::{
        DIRECTORY_DEPTH, Diagnostic, FILE_BYTES, Files, SCANNED_ENTRY_COUNT, TOTAL_BYTES,
        input_error, portable_relative,
    };

    struct Directory {
        fd: OwnedFd,
        parent: Option<Arc<Directory>>,
        name: OsString,
    }

    /// An opened input directory and its retained ancestor chain.
    #[derive(Clone)]
    pub struct InputDirectory(Arc<Directory>);

    fn flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }

    fn io_error(error: impl std::fmt::Display) -> Diagnostic {
        input_error(format!("Cannot capture anchored input: {error}"))
    }

    fn same_object(a: &Stat, b: &Stat) -> bool {
        a.st_dev == b.st_dev
            && a.st_ino == b.st_ino
            && FileType::from_raw_mode(a.st_mode) == FileType::from_raw_mode(b.st_mode)
    }

    fn unchanged_file(a: &Stat, b: &Stat) -> bool {
        same_object(a, b)
            && a.st_size == b.st_size
            && a.st_mtime == b.st_mtime
            && a.st_mtime_nsec == b.st_mtime_nsec
            && a.st_ctime == b.st_ctime
            && a.st_ctime_nsec == b.st_ctime_nsec
    }

    impl InputDirectory {
        pub fn open(path: &Path) -> Result<Self, Diagnostic> {
            // Parent aliases such as /tmp are allowed only when selecting an external root.
            // Once the parent is open, neither root nor descendant operations resolve it again.
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir().map_err(io_error)?.join(path)
            };
            let absolute: std::path::PathBuf = absolute.components().collect();
            if absolute == Path::new("/") {
                let fd = fs::open("/", flags(), Mode::empty()).map_err(io_error)?;
                return Ok(Self(Arc::new(Directory {
                    fd,
                    parent: None,
                    name: OsString::new(),
                })));
            }
            let name = absolute
                .file_name()
                .ok_or_else(|| input_error("Input root must name a directory."))?;
            let parent = fs::open(
                absolute.parent().expect("absolute parent"),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(io_error)?;
            let parent = Self(Arc::new(Directory {
                fd: parent,
                parent: None,
                name: OsString::new(),
            }));
            parent.child_checked(name, &parent.stat(name)?)
        }

        fn stat(&self, name: &OsStr) -> Result<Stat, Diagnostic> {
            fs::statat(&self.0.fd, name, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)
        }

        fn child_checked(&self, name: &OsStr, expected: &Stat) -> Result<Self, Diagnostic> {
            if FileType::from_raw_mode(expected.st_mode) != FileType::Directory {
                return Err(input_error(
                    "Every input root and descendant directory must be a directory without symlinks.",
                ));
            }
            let fd = fs::openat(&self.0.fd, name, flags(), Mode::empty()).map_err(io_error)?;
            if !same_object(expected, &fs::fstat(&fd).map_err(io_error)?) {
                return Err(input_error(
                    "Input directory changed while it was being opened.",
                ));
            }
            let child = Self(Arc::new(Directory {
                fd,
                parent: Some(self.0.clone()),
                name: name.to_owned(),
            }));
            child.validate()?;
            Ok(child)
        }

        pub fn child(&self, name: &OsStr) -> Result<Self, Diagnostic> {
            if Path::new(name).components().count() != 1
                || portable_relative(Path::new(name))? != name.to_string_lossy()
            {
                return Err(input_error("Expected one portable directory component."));
            }
            self.validate()?;
            self.child_checked(name, &self.stat(name)?)
        }

        pub fn directory_identity_if_exists(
            &self,
            name: &OsStr,
        ) -> Result<Option<(u64, u64)>, Diagnostic> {
            self.validate()?;
            let stat = match fs::statat(&self.0.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(stat) => stat,
                Err(rustix::io::Errno::NOENT) => return Ok(None),
                Err(error) => return Err(io_error(error)),
            };
            if FileType::from_raw_mode(stat.st_mode) != FileType::Directory {
                return Ok(None);
            }
            Ok(Some(self.child_checked(name, &stat)?.identity()?))
        }

        #[allow(clippy::unnecessary_cast)] // Device identifiers differ in type across Unix targets.
        pub fn identity(&self) -> Result<(u64, u64), Diagnostic> {
            let stat = fs::fstat(&self.0.fd).map_err(io_error)?;
            Ok((stat.st_dev as u64, stat.st_ino as u64))
        }

        fn validate(&self) -> Result<(), Diagnostic> {
            if let Some(parent) = &self.0.parent {
                let parent = Self(parent.clone());
                parent.validate()?;
                if !same_object(
                    &parent.stat(&self.0.name)?,
                    &fs::fstat(&self.0.fd).map_err(io_error)?,
                ) {
                    return Err(input_error(
                        "Input directory was renamed or replaced during capture.",
                    ));
                }
            }
            Ok(())
        }

        fn read_checked(&self, name: &OsStr, expected: &Stat) -> Result<Vec<u8>, Diagnostic> {
            if FileType::from_raw_mode(expected.st_mode) != FileType::RegularFile {
                return Err(input_error(
                    "Input files must be regular files, never symlinks or special files.",
                ));
            }
            self.validate()?;
            let fd = fs::openat(
                &self.0.fd,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(io_error)?;
            let before = fs::fstat(&fd).map_err(io_error)?;
            if !unchanged_file(expected, &before) {
                return Err(input_error("Input file changed while it was being opened."));
            }
            let mut file = File::from(fd);
            let mut bytes = Vec::new();
            file.by_ref()
                .take(FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(io_error)?;
            if bytes.len() as u64 > FILE_BYTES {
                return Err(Diagnostic::error(
                    "limit",
                    "qargo",
                    "Input exceeds the 1 MiB file limit.",
                ));
            }
            if !unchanged_file(&before, &fs::fstat(&file).map_err(io_error)?)
                || !unchanged_file(&before, &self.stat(name)?)
            {
                return Err(input_error("Input file changed during capture."));
            }
            self.validate()?;
            Ok(bytes)
        }

        pub fn read_regular(&self, name: &OsStr) -> Result<Vec<u8>, Diagnostic> {
            portable_relative(Path::new(name))?;
            if Path::new(name).components().count() != 1 {
                return Err(input_error("Expected one portable file component."));
            }
            self.read_checked(name, &self.stat(name)?)
        }

        pub fn collect(&self, extension: Option<&str>) -> Result<Files, Diagnostic> {
            self.collect_with_hook(extension, &mut |_, _| {})
        }

        fn collect_with_hook(
            &self,
            extension: Option<&str>,
            hook: &mut impl FnMut(&Path, bool),
        ) -> Result<Files, Diagnostic> {
            let mut state = Capture {
                files: Files::new(),
                total: 0,
                visited: 0,
                extension,
            };
            self.walk(Path::new(""), 0, &mut state, hook)?;
            Ok(state.files)
        }

        fn walk(
            &self,
            relative: &Path,
            depth: usize,
            state: &mut Capture<'_>,
            hook: &mut impl FnMut(&Path, bool),
        ) -> Result<(), Diagnostic> {
            if depth > DIRECTORY_DEPTH {
                return Err(Diagnostic::error(
                    "limit",
                    "qargo",
                    "Input directory depth exceeds 64.",
                ));
            }
            hook(relative, true);
            self.validate()?;
            let mut entries = Dir::read_from(&self.0.fd).map_err(io_error)?;
            while let Some(entry) = entries.read() {
                let entry = entry.map_err(io_error)?;
                let name = OsStr::from_bytes(entry.file_name().to_bytes());
                if name == "." || name == ".." {
                    continue;
                }
                state.visited += 1;
                if state.visited > SCANNED_ENTRY_COUNT {
                    return Err(Diagnostic::error(
                        "limit",
                        "qargo",
                        "Input contains more than 4096 entries.",
                    ));
                }
                let path = relative.join(name);
                let label = portable_relative(&path)?;
                let expected = self.stat(name)?;
                if entry.ino() != expected.st_ino {
                    return Err(input_error("Input entry changed during enumeration."));
                }
                hook(&path, false);
                match FileType::from_raw_mode(expected.st_mode) {
                    FileType::Directory => {
                        self.child_checked(name, &expected)?
                            .walk(&path, depth + 1, state, hook)?
                    }
                    FileType::RegularFile => {
                        if state.extension.is_some_and(|suffix| {
                            path.extension().and_then(|part| part.to_str()) != Some(suffix)
                        }) {
                            continue;
                        }
                        let bytes = self.read_checked(name, &expected)?;
                        state.total += bytes.len();
                        if state.total > TOTAL_BYTES {
                            return Err(Diagnostic::error(
                                "limit",
                                "qargo",
                                "Input exceeds the 16 MiB total limit.",
                            ));
                        }
                        if state.files.insert(label, bytes).is_some() {
                            return Err(input_error("Repeated input entry during enumeration."));
                        }
                    }
                    FileType::Symlink => {
                        return Err(input_error(format!(
                            "Symlink input is not allowed: {label}"
                        )));
                    }
                    _ => {
                        return Err(input_error(format!(
                            "Special input file is not allowed: {label}"
                        )));
                    }
                }
            }
            self.validate()
        }
    }

    struct Capture<'a> {
        files: Files,
        total: usize,
        visited: usize,
        extension: Option<&'a str>,
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use std::os::unix::fs::symlink;

        #[test]
        fn root_replacement_after_open_cannot_redirect_capture() {
            let home = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            fs::create_dir(home.path().join("root")).unwrap();
            fs::write(outside.path().join("outside.qli"), "outside").unwrap();
            let root = InputDirectory::open(&home.path().join("root")).unwrap();
            fs::rename(home.path().join("root"), home.path().join("held")).unwrap();
            symlink(outside.path(), home.path().join("root")).unwrap();
            assert!(root.collect(None).is_err());
        }

        #[test]
        fn special_entries_are_rejected_even_when_their_extension_is_ignored() {
            let root = tempfile::tempdir().unwrap();
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(root.path().join("ignored.txt"))
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(
                InputDirectory::open(root.path())
                    .unwrap()
                    .collect(Some("qli"))
                    .is_err()
            );
        }

        #[test]
        fn nested_directory_and_file_swaps_are_rejected_at_each_boundary() {
            for (entry, opened, symlink_swap) in [
                ("nested", false, true),
                ("nested", true, true),
                ("nested/a.qli", false, true),
                ("nested/a.qli", false, false),
            ] {
                let home = tempfile::tempdir().unwrap();
                let outside = tempfile::tempdir().unwrap();
                fs::create_dir(home.path().join("nested")).unwrap();
                fs::write(home.path().join("nested/a.qli"), "inside").unwrap();
                fs::write(outside.path().join("a.qli"), "outside").unwrap();
                let root = InputDirectory::open(home.path()).unwrap();
                let mut swapped = false;
                let result = root.collect_with_hook(None, &mut |path, at_open| {
                    if !swapped && path == Path::new(entry) && at_open == opened {
                        let original = home.path().join(entry);
                        fs::rename(&original, home.path().join("old")).unwrap();
                        if symlink_swap {
                            symlink(
                                if entry == "nested" {
                                    outside.path().to_path_buf()
                                } else {
                                    outside.path().join("a.qli")
                                },
                                &original,
                            )
                            .unwrap();
                        } else {
                            fs::write(&original, "outside").unwrap();
                        }
                        swapped = true;
                    }
                });
                assert!(swapped);
                assert!(result.is_err(), "accepted a replacement at {entry}");
            }
        }
    }
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
        if files.len() > CAPTURED_FILE_COUNT {
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
