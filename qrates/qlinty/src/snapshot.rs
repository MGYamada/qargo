//! Bounded immutable input capture with portable, byte-exact identities.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::support::report::Diagnostic;

pub type Files = BTreeMap<String, Vec<u8>>;
pub const FILE_BYTES: u64 = 1 << 20;
pub const TOTAL_BYTES: usize = 16 << 20;
const SCANNED_ENTRY_COUNT: usize = 4096;
const CAPTURED_FILE_COUNT: usize = 4096;
const DIRECTORY_DEPTH: usize = 64;

fn input_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("input", "qlirgo", message)
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
            "qlirgo",
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
    pub fn verify(&self) -> Result<(), Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }

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
    pub fn collect_source_context(&self) -> Result<Files, Diagnostic> {
        self.collect(None)
    }
    pub fn collect_qrate_source(&self) -> Result<Files, Diagnostic> {
        self.collect(None)
    }
    pub fn with_manifests<T>(
        &self,
        _: impl FnOnce(Files) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }
    pub fn read_manifests(&self) -> Result<Files, Diagnostic> {
        Err(input_error(
            "Descriptor-anchored input capture is unavailable on this platform.",
        ))
    }
    pub fn inherited_source_manifests(&self) -> Result<Option<Files>, Diagnostic> {
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
    use std::collections::BTreeMap;
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
        anchor: Option<std::path::PathBuf>,
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

    #[derive(Clone, Copy, PartialEq)]
    enum CapturePoint {
        DirectoryOpened,
        EntryInspected,
        DirectoryEnumerated,
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
                    anchor: None,
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
                anchor: Some(absolute.parent().expect("absolute parent").to_path_buf()),
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
                anchor: None,
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

        /// Revalidate the retained directory chain without acquiring a replacement subject.
        pub fn verify(&self) -> Result<(), Diagnostic> {
            self.validate()
        }

        /// Duplicate the captured directory capability for descriptor-relative effects.
        pub fn held_file(&self) -> Result<File, Diagnostic> {
            self.validate()?;
            self.0.fd.try_clone().map(File::from).map_err(io_error)
        }

        fn validate(&self) -> Result<(), Diagnostic> {
            if let Some(path) = &self.0.anchor {
                // External parent aliases are allowed, but must still name the held object.
                let current = fs::open(
                    path,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(io_error)?;
                if !same_object(
                    &fs::fstat(&current).map_err(io_error)?,
                    &fs::fstat(&self.0.fd).map_err(io_error)?,
                ) {
                    return Err(input_error(
                        "Input ancestor was renamed or replaced after capture.",
                    ));
                }
            }
            if let Some(parent) = &self.0.parent {
                let parent = Self(parent.clone());
                parent.validate()?;
                if !same_object(
                    &parent.stat(&self.0.name)?,
                    &fs::fstat(&self.0.fd).map_err(io_error)?,
                ) {
                    return Err(input_error(
                        "Input directory was renamed or replaced after capture.",
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
                    "qlirgo",
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

        fn manifest_entries(&self) -> Result<[Option<Stat>; 2], Diagnostic> {
            let mut entries = [None, None];
            for (index, name) in crate::support::manifest::NAMES.iter().enumerate() {
                entries[index] = match fs::statat(&self.0.fd, *name, AtFlags::SYMLINK_NOFOLLOW) {
                    Err(rustix::io::Errno::NOENT) => None,
                    Err(error) => return Err(io_error(error)),
                    Ok(stat) if FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile => {
                        Some(stat)
                    }
                    Ok(_) => {
                        return Err(Diagnostic::error(
                            "project",
                            "compiler",
                            format!("{name} must be a regular, non-symlink file."),
                        ));
                    }
                };
            }
            Ok(entries)
        }

        fn verify_manifest_entries(&self, expected: &[Option<Stat>; 2]) -> Result<(), Diagnostic> {
            let actual = self.manifest_entries()?;
            for (before, after) in expected.iter().zip(actual.iter()) {
                match (before, after) {
                    (None, None) => {}
                    (Some(before), Some(after)) if unchanged_file(before, after) => {}
                    _ => {
                        return Err(input_error(
                            "Source configuration ancestry changed during capture.",
                        ));
                    }
                }
            }
            Ok(())
        }

        fn read_manifest_entries(&self, expected: &[Option<Stat>; 2]) -> Result<Files, Diagnostic> {
            let mut files = Files::new();
            for (name, entry) in crate::support::manifest::NAMES.iter().zip(expected) {
                if let Some(stat) = entry {
                    files.insert((*name).into(), self.read_checked(OsStr::new(name), stat)?);
                }
            }
            crate::support::manifest::selected_name(&files)?;
            self.verify_manifest_entries(expected)?;
            self.validate()?;
            Ok(files)
        }

        /// Keep pair identities/absences bound until the entire managed capture completes.
        pub fn with_manifests<T>(
            &self,
            capture: impl FnOnce(Files) -> Result<T, Diagnostic>,
        ) -> Result<T, Diagnostic> {
            let expected = self.manifest_entries()?;
            let files = self.read_manifest_entries(&expected)?;
            let result = capture(files)?;
            self.verify_manifest_entries(&expected)?;
            self.validate()?;
            Ok(result)
        }

        /// Capture both names and their absence through one held directory.
        pub fn read_manifests(&self) -> Result<Files, Diagnostic> {
            self.validate()?;
            self.read_manifest_entries(&self.manifest_entries()?)
        }

        /// Capture the nearest pair from held descriptor ancestry, validating both names.
        pub fn inherited_source_manifests(&self) -> Result<Option<Files>, Diagnostic> {
            self.inherited_source_manifests_with_hook(&mut |_| {})
        }

        fn inherited_source_manifests_with_hook(
            &self,
            after_parent: &mut impl FnMut(usize),
        ) -> Result<Option<Files>, Diagnostic> {
            fn verify(
                root: &InputDirectory,
                ancestors: &[(InputDirectory, InputDirectory, [Option<Stat>; 2])],
            ) -> Result<(), Diagnostic> {
                root.validate()?;
                for (child, parent, expected) in ancestors {
                    let current =
                        fs::openat(&child.0.fd, "..", flags(), Mode::empty()).map_err(io_error)?;
                    if !same_object(
                        &fs::fstat(&current).map_err(io_error)?,
                        &fs::fstat(&parent.0.fd).map_err(io_error)?,
                    ) {
                        return Err(input_error(
                            "Inherited source configuration ancestry changed during capture.",
                        ));
                    }
                    parent.verify_manifest_entries(expected)?;
                }
                Ok(())
            }
            self.validate()?;
            let mut current = self.clone();
            let mut ancestors = Vec::new();
            loop {
                let fd =
                    fs::openat(&current.0.fd, "..", flags(), Mode::empty()).map_err(io_error)?;
                let before = fs::fstat(&fd).map_err(io_error)?;
                if same_object(&before, &fs::fstat(&current.0.fd).map_err(io_error)?) {
                    verify(self, &ancestors)?;
                    return Ok(None);
                }
                if ancestors.len() >= DIRECTORY_DEPTH {
                    return Err(input_error(
                        "Inherited source configuration exceeds the 64-directory capture limit.",
                    ));
                }
                let parent = Self(Arc::new(Directory {
                    fd,
                    parent: None,
                    name: OsString::new(),
                    anchor: None,
                }));
                after_parent(ancestors.len() + 1);
                let expected = parent.manifest_entries()?;
                let captured = parent.read_manifest_entries(&expected)?;
                ancestors.push((current, parent.clone(), expected));
                verify(self, &ancestors)?;
                if !captured.is_empty() {
                    return Ok(Some(captured));
                }
                current = parent;
            }
        }

        pub fn collect(&self, extension: Option<&str>) -> Result<Files, Diagnostic> {
            self.collect_with_hook(extension, &mut |_, _| {})
        }

        /// Capture source and edition manifests in one bounded anchored traversal.
        pub fn collect_source_context(&self) -> Result<Files, Diagnostic> {
            self.collect_filtered(None, true, true, &mut |_, _| {})
        }

        /// Preserve all qrate source-root files while validating manifest entry types.
        pub fn collect_qrate_source(&self) -> Result<Files, Diagnostic> {
            self.collect_filtered(None, false, true, &mut |_, _| {})
        }

        fn collect_with_hook(
            &self,
            extension: Option<&str>,
            hook: &mut impl FnMut(&Path, CapturePoint),
        ) -> Result<Files, Diagnostic> {
            self.collect_filtered(extension, false, false, hook)
        }

        fn collect_filtered(
            &self,
            extension: Option<&str>,
            source_context: bool,
            validate_manifests: bool,
            hook: &mut impl FnMut(&Path, CapturePoint),
        ) -> Result<Files, Diagnostic> {
            let mut state = Capture {
                files: Files::new(),
                total: 0,
                visited: 0,
                extension,
                source_context,
                validate_manifests,
            };
            self.walk(Path::new(""), 0, &mut state, hook)?;
            Ok(state.files)
        }

        fn inventory(
            &self,
            relative: &Path,
            remaining: usize,
            inspected: &mut impl FnMut(&Path),
        ) -> Result<BTreeMap<OsString, Stat>, Diagnostic> {
            let mut inventory = BTreeMap::new();
            let mut entries = Dir::read_from(&self.0.fd).map_err(io_error)?;
            while let Some(entry) = entries.read() {
                let entry = entry.map_err(io_error)?;
                let name = OsStr::from_bytes(entry.file_name().to_bytes());
                if name == "." || name == ".." {
                    continue;
                }
                if inventory.len() >= remaining {
                    return Err(Diagnostic::error(
                        "limit",
                        "qlirgo",
                        "Input contains more than 4096 entries.",
                    ));
                }
                let path = relative.join(name);
                portable_relative(&path)?;
                let expected = self.stat(name)?;
                if entry.ino() != expected.st_ino {
                    return Err(input_error("Input entry changed during enumeration."));
                }
                if inventory.insert(name.to_owned(), expected).is_some() {
                    return Err(input_error("Repeated input entry during enumeration."));
                }
                inspected(&path);
            }
            Ok(inventory)
        }

        fn walk(
            &self,
            relative: &Path,
            depth: usize,
            state: &mut Capture<'_>,
            hook: &mut impl FnMut(&Path, CapturePoint),
        ) -> Result<(), Diagnostic> {
            if depth > DIRECTORY_DEPTH {
                return Err(Diagnostic::error(
                    "limit",
                    "qlirgo",
                    "Input directory depth exceeds 64.",
                ));
            }
            hook(relative, CapturePoint::DirectoryOpened);
            self.validate()?;
            let before = fs::fstat(&self.0.fd).map_err(io_error)?;
            let remaining = SCANNED_ENTRY_COUNT - state.visited;
            let inventory = self.inventory(relative, remaining, &mut |path| {
                hook(path, CapturePoint::EntryInspected);
            })?;
            state.visited += inventory.len();
            hook(relative, CapturePoint::DirectoryEnumerated);
            for (name, expected) in &inventory {
                let path = relative.join(name);
                let label = portable_relative(&path)?;
                if state.validate_manifests
                    && crate::support::manifest::is_manifest(Path::new(name))
                    && FileType::from_raw_mode(expected.st_mode) != FileType::RegularFile
                {
                    return Err(Diagnostic::error(
                        "project",
                        "compiler",
                        format!("Qlirgo.toml must be a regular, non-symlink file: {label}"),
                    ));
                }
                match FileType::from_raw_mode(expected.st_mode) {
                    FileType::Directory => {
                        self.child_checked(name, expected)?
                            .walk(&path, depth + 1, state, hook)?
                    }
                    FileType::RegularFile => {
                        if (state.source_context
                            && path.extension().and_then(|part| part.to_str()) != Some("qli")
                            && !crate::support::manifest::is_manifest(&path))
                            || state.extension.is_some_and(|suffix| {
                                path.extension().and_then(|part| part.to_str()) != Some(suffix)
                            })
                        {
                            continue;
                        }
                        let bytes = self.read_checked(name, expected)?;
                        state.total += bytes.len();
                        if state.total > TOTAL_BYTES {
                            return Err(Diagnostic::error(
                                "limit",
                                "qlirgo",
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
            // Reopen enumeration through the same held descriptor. Verification
            // does not consume the logical entry budget a second time.
            let after = self.inventory(relative, remaining, &mut |_| {})?;
            if inventory.len() != after.len()
                || inventory.iter().any(|(name, expected)| {
                    !after
                        .get(name)
                        .is_some_and(|actual| unchanged_file(expected, actual))
                })
                || !unchanged_file(&before, &fs::fstat(&self.0.fd).map_err(io_error)?)
            {
                return Err(input_error(
                    "Input directory contents changed during capture.",
                ));
            }
            self.validate()
        }
    }

    struct Capture<'a> {
        files: Files,
        total: usize,
        visited: usize,
        extension: Option<&'a str>,
        source_context: bool,
        validate_manifests: bool,
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use std::os::unix::fs::symlink;

        #[test]
        fn inherited_manifest_uses_held_ancestry_despite_a_transient_parent_alias() {
            let home = tempfile::tempdir().unwrap();
            let first = home.path().join("first");
            let second = home.path().join("second");
            for path in [&first, &second] {
                fs::create_dir_all(path.join("src")).unwrap();
            }
            let expected = b"schema-version=2\n[qrate]\nedition='2026'\n# first\n";
            fs::write(first.join("Qlirgo.toml"), expected).unwrap();
            fs::write(
                second.join("Qlirgo.toml"),
                b"schema-version=2\n[qrate]\nedition='2027'\n# second\n",
            )
            .unwrap();
            let alias = home.path().join("alias");
            symlink(&first, &alias).unwrap();
            let held = InputDirectory::open(&alias.join("src")).unwrap();
            let actual = held
                .inherited_source_manifests_with_hook(&mut |index| {
                    assert_eq!(index, 1);
                    fs::remove_file(&alias).unwrap();
                    symlink(&second, &alias).unwrap();
                    assert_eq!(
                        fs::canonicalize(alias.join("src")).unwrap(),
                        fs::canonicalize(second.join("src")).unwrap()
                    );
                    fs::remove_file(&alias).unwrap();
                    symlink(&first, &alias).unwrap();
                })
                .unwrap()
                .unwrap();
            assert_eq!(actual["Qlirgo.toml"], expected);
            held.verify().unwrap();
        }

        #[test]
        fn a_new_nearer_manifest_invalidates_recorded_ancestor_absence() {
            let home = tempfile::tempdir().unwrap();
            let parent = home.path().join("outer");
            fs::create_dir_all(parent.join("src")).unwrap();
            fs::write(
                home.path().join("Qlirgo.toml"),
                b"schema-version=2\n[qrate]\nedition='2026'\n",
            )
            .unwrap();
            let held = InputDirectory::open(&parent.join("src")).unwrap();
            let diagnostic = held
                .inherited_source_manifests_with_hook(&mut |index| {
                    if index == 2 {
                        fs::write(
                            parent.join("Qlirgo.toml"),
                            b"schema-version=2\n[qrate]\nedition='2027'\n",
                        )
                        .unwrap();
                    }
                })
                .err()
                .unwrap();
            assert!(diagnostic.message.contains("ancestry changed"));
        }

        #[test]
        fn unrelated_ancestor_sibling_changes_do_not_change_configuration() {
            let home = tempfile::tempdir().unwrap();
            let parent = home.path().join("outer");
            fs::create_dir_all(parent.join("src")).unwrap();
            let expected = b"schema-version=2\n[qrate]\nedition='2026'\n";
            fs::write(home.path().join("Qlirgo.toml"), expected).unwrap();
            let held = InputDirectory::open(&parent.join("src")).unwrap();
            let actual = held
                .inherited_source_manifests_with_hook(&mut |index| {
                    if index == 2 {
                        fs::write(parent.join("unrelated.txt"), b"other input").unwrap();
                    }
                })
                .unwrap()
                .unwrap();
            assert_eq!(actual["Qlirgo.toml"], expected);
            assert_eq!(
                fs::read(parent.join("unrelated.txt")).unwrap(),
                b"other input"
            );
            held.verify().unwrap();
        }

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
                ("nested", CapturePoint::EntryInspected, true),
                ("nested", CapturePoint::DirectoryOpened, true),
                ("nested/a.qli", CapturePoint::EntryInspected, true),
                ("nested/a.qli", CapturePoint::EntryInspected, false),
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

        #[test]
        fn insertion_during_or_after_enumeration_is_rejected() {
            for point in [
                CapturePoint::EntryInspected,
                CapturePoint::DirectoryEnumerated,
            ] {
                for inserted in ["added.qli", "ignored.txt"] {
                    let home = tempfile::tempdir().unwrap();
                    fs::write(home.path().join("a.qli"), "inside").unwrap();
                    let root = InputDirectory::open(home.path()).unwrap();
                    let mut changed = false;
                    let result = root.collect_with_hook(Some("qli"), &mut |_, observed| {
                        if !changed && observed == point {
                            fs::write(home.path().join(inserted), "added").unwrap();
                            changed = true;
                        }
                    });
                    assert!(changed);
                    assert_eq!(result.unwrap_err().id, "input");
                }
            }
        }

        #[test]
        fn deletion_of_an_enumerated_but_unread_entry_is_rejected() {
            for removed in ["a.qli", "ignored.txt", "empty"] {
                let home = tempfile::tempdir().unwrap();
                fs::write(home.path().join("a.qli"), "inside").unwrap();
                fs::write(home.path().join("ignored.txt"), "ignored").unwrap();
                fs::create_dir(home.path().join("empty")).unwrap();
                let root = InputDirectory::open(home.path()).unwrap();
                let mut changed = false;
                let result = root.collect_with_hook(Some("qli"), &mut |path, point| {
                    if !changed
                        && path == Path::new("")
                        && point == CapturePoint::DirectoryEnumerated
                    {
                        if removed == "empty" {
                            fs::remove_dir(home.path().join(removed)).unwrap();
                        } else {
                            fs::remove_file(home.path().join(removed)).unwrap();
                        }
                        changed = true;
                    }
                });
                assert!(changed);
                assert_eq!(result.unwrap_err().id, "input");
            }
        }

        #[test]
        fn stable_inventory_verification_does_not_spend_the_entry_budget_twice() {
            let home = tempfile::tempdir().unwrap();
            for index in 0..SCANNED_ENTRY_COUNT {
                fs::write(home.path().join(format!("{index}.qli")), "inside").unwrap();
            }
            let root = InputDirectory::open(home.path()).unwrap();
            assert_eq!(
                root.collect(Some("qli")).unwrap().len(),
                SCANNED_ENTRY_COUNT
            );
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
        .prefix("qlirgo-snapshot-")
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

/// Captured source bytes and their identity, independent of any working copy.
pub struct FrozenSources {
    files: Files,
    source_id: String,
}

/// A disposable working copy. Mutating it cannot change its captured subject.
pub struct SourceStage {
    _directory: TempDir,
    canonical_root: PathBuf,
}

impl SourceStage {
    pub fn root(&self) -> &Path {
        &self.canonical_root
    }
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
                "qlirgo",
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
                    "qlirgo",
                    "Source exceeds the 1 MiB file limit.",
                ));
            }
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| input_error("Source byte accounting overflow."))?;
            if total > TOTAL_BYTES {
                return Err(Diagnostic::error(
                    "limit",
                    "qlirgo",
                    "Source exceeds the 16 MiB total limit.",
                ));
            }
        }
        let source_id = digest_files("qleisli.source.v1", &files);
        Ok(Self { files, source_id })
    }

    pub fn files(&self) -> &Files {
        &self.files
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Materialize a fresh copy for one checker or tool invocation.
    pub fn stage(&self) -> Result<SourceStage, Diagnostic> {
        self.stage_configuration(None)
    }

    /// Stage actual captured edition manifests, independently of .qli source_id.
    pub fn stage_with_configuration(
        &self,
        configuration: &Files,
    ) -> Result<SourceStage, Diagnostic> {
        validate_source_configuration(self, configuration)?;
        self.stage_configuration(Some(configuration))
    }

    /// The beta library still reads Qargo.toml; this derived view is never a raw input.
    pub fn stage_for_beta(&self, configuration: &Files) -> Result<SourceStage, Diagnostic> {
        validate_source_configuration(self, configuration)?;
        let legacy = configuration
            .iter()
            .map(|(label, bytes)| {
                let path = Path::new(label).with_file_name(crate::support::manifest::LEGACY);
                (path.to_str().unwrap().to_owned(), bytes.clone())
            })
            .collect();
        self.stage_configuration(Some(&legacy))
    }

    fn stage_configuration(
        &self,
        configuration: Option<&Files>,
    ) -> Result<SourceStage, Diagnostic> {
        let directory = materialize(&self.files)?;
        if let Some(configuration) = configuration {
            for (label, bytes) in configuration {
                let path = directory.path().join(label);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| input_error(error.to_string()))?;
                }
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|error| {
                        input_error(format!("Cannot stage source configuration: {error}"))
                    })?;
                file.write_all(bytes)
                    .map_err(|error| input_error(error.to_string()))?;
            }
        }
        let canonical_root = fs::canonicalize(directory.path())
            .map_err(|error| input_error(format!("Cannot resolve private snapshot: {error}")))?;
        Ok(SourceStage {
            _directory: directory,
            canonical_root,
        })
    }

    pub fn count(&self) -> usize {
        self.files.len()
    }
}

/// Existing capture ceilings also cover staged source/configuration together.
pub fn validate_source_configuration(
    sources: &FrozenSources,
    configuration: &Files,
) -> Result<(), Diagnostic> {
    if (sources.count() != 0 || !configuration.is_empty())
        && !configuration.contains_key("Qlirgo.toml")
    {
        return Err(Diagnostic::error(
            "project",
            "compiler",
            "Captured source configuration requires root Qlirgo.toml with an explicit edition; no edition is inferred.",
        ));
    }
    if sources.count() + configuration.len() > CAPTURED_FILE_COUNT {
        return Err(input_error(
            "Source and configuration exceed the 4096-file capture limit.",
        ));
    }
    let mut total: usize = sources.files().values().map(Vec::len).sum();
    for (label, bytes) in configuration {
        let path = Path::new(label);
        if portable_relative(path)? != *label
            || path.file_name().and_then(|name| name.to_str()) != Some("Qlirgo.toml")
        {
            return Err(input_error(
                "Source configuration must contain only canonical Qlirgo.toml paths.",
            ));
        }
        if bytes.len() > 65_536 {
            return Err(input_error(
                "Qlirgo.toml exceeds the 65536-byte manifest limit.",
            ));
        }
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| input_error("Source/configuration byte accounting overflow."))?;
        if total > TOTAL_BYTES {
            return Err(input_error(
                "Source and configuration exceed the 16 MiB capture limit.",
            ));
        }
        std::str::from_utf8(bytes).map_err(|_| input_error("Qlirgo.toml must be UTF-8."))?;
    }
    Ok(())
}
