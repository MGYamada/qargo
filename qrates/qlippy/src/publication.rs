//! Descriptor-anchored publication shared by build and documentation outputs.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::snapshot::{Files, portable_relative};

/// Publication failures retain the caller's diagnostic vocabulary.
#[derive(Debug)]
pub enum PublicationError {
    Output(String),
    UnsafePath(String),
    Mismatch,
}

impl fmt::Display for PublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Output(message) | Self::UnsafePath(message) => formatter.write_str(message),
            Self::Mismatch => {
                formatter.write_str("Existing artifacts are inconsistent; nothing was overwritten.")
            }
        }
    }
}

impl std::error::Error for PublicationError {}

fn output_error(message: impl Into<String>) -> PublicationError {
    PublicationError::Output(message.into())
}

fn artifact_label(path: &Path) -> Result<String, PublicationError> {
    portable_relative(path).map_err(|error| output_error(error.message))
}

/// Normalize an output path without traversing aliases or allowing parent components.
pub fn absolute_normalized(path: &Path) -> Result<PathBuf, PublicationError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| output_error(error.to_string()))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(output_error(
                    "Output paths cannot contain parent components.",
                ));
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    if normalized.file_name().is_none() {
        return Err(output_error("Output must name an artifact directory."));
    }
    Ok(normalized)
}

/// Publish a complete directory atomically, preserving explicitly declared empty directories.
pub fn publish(
    output: &Path,
    files: &Files,
    directories: &BTreeSet<PathBuf>,
) -> Result<(), PublicationError> {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        unix::publish(output, files, directories)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
    {
        let _ = (output, files, directories);
        Err(output_error(
            "Atomic directory publication without replacement is unavailable on this platform.",
        ))
    }
}

/// Compare complete artifact bytes and directory inventory using held directory descriptors.
pub fn artifacts_match(
    output: &Path,
    files: &Files,
    directories: &BTreeSet<PathBuf>,
) -> Result<bool, PublicationError> {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        unix::matches(output, files, directories)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
    {
        let _ = (output, files, directories);
        Err(output_error(
            "Descriptor-anchored artifact validation is unavailable on this platform.",
        ))
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
mod unix {
    use std::collections::BTreeSet;
    use std::ffi::{OsStr, OsString};
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Component, Path, PathBuf};

    use rustix::fd::{AsFd, OwnedFd};
    use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags, RenameFlags};

    use super::{PublicationError, absolute_normalized, artifact_label, output_error};
    use crate::snapshot::Files;

    fn directory_flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }

    fn directory_at(parent: &impl AsFd, name: &OsStr, create: bool) -> io::Result<OwnedFd> {
        match fs::openat(parent, name, directory_flags(), Mode::empty()) {
            Ok(directory) => Ok(directory),
            Err(rustix::io::Errno::NOENT) if create => {
                match fs::mkdirat(parent, name, Mode::from_raw_mode(0o755)) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                    Err(error) => return Err(error.into()),
                }
                fs::openat(parent, name, directory_flags(), Mode::empty()).map_err(Into::into)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn open_parent(path: &Path, create: bool) -> Result<OwnedFd, PublicationError> {
        let mut directory = fs::open("/", directory_flags(), Mode::empty())
            .map_err(|error| output_error(format!("Cannot open filesystem root: {error}")))?;
        for component in path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    directory = directory_at(&directory, name, create).map_err(|error| {
                        let message = format!("Cannot open output ancestor as a directory without symlinks: {error}");
                        if matches!(error.raw_os_error(), Some(code) if code == rustix::io::Errno::LOOP.raw_os_error() || code == rustix::io::Errno::NOTDIR.raw_os_error()) {
                            PublicationError::UnsafePath(message)
                        } else {
                            output_error(message)
                        }
                    })?;
                }
                _ => return Err(output_error("Output ancestors must have canonical paths.")),
            }
        }
        Ok(directory)
    }

    fn same_directory(left: &impl AsFd, right: &impl AsFd) -> io::Result<bool> {
        let left = fs::fstat(left)?;
        let right = fs::fstat(right)?;
        Ok(left.st_dev == right.st_dev && left.st_ino == right.st_ino)
    }

    fn still_named(parent: &impl AsFd, name: &OsStr, held: &impl AsFd) -> io::Result<bool> {
        let named = fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)?;
        let held = fs::fstat(held)?;
        Ok(named.st_dev == held.st_dev
            && named.st_ino == held.st_ino
            && FileType::from_raw_mode(named.st_mode) == FileType::from_raw_mode(held.st_mode))
    }

    fn validate_parent(parent: &OwnedFd, path: &Path) -> Result<(), PublicationError> {
        let current = open_parent(path, false)?;
        if !same_directory(parent, &current).map_err(|error| output_error(error.to_string()))? {
            return Err(output_error(
                "Artifact output ancestors changed during publication.",
            ));
        }
        Ok(())
    }

    fn expected_directories(
        files: &Files,
        extra: &BTreeSet<PathBuf>,
    ) -> Result<BTreeSet<PathBuf>, PublicationError> {
        let mut directories = BTreeSet::new();
        for label in files.keys() {
            if artifact_label(Path::new(label))? != *label {
                return Err(output_error(
                    "Artifact paths must be canonical relative paths.",
                ));
            }
            let mut parent = Path::new(label).parent();
            while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
                if files.contains_key(path.to_str().unwrap_or("")) {
                    return Err(output_error("Artifact file and directory paths conflict."));
                }
                directories.insert(path.to_path_buf());
                parent = path.parent();
            }
        }
        for path in extra {
            artifact_label(path)?;
            for parent in path.ancestors().filter(|path| !path.as_os_str().is_empty()) {
                if files.contains_key(parent.to_str().unwrap_or("")) {
                    return Err(output_error("Artifact file and directory paths conflict."));
                }
                directories.insert(parent.to_path_buf());
            }
        }
        Ok(directories)
    }

    fn artifacts_match(
        parent: &OwnedFd,
        name: &OsStr,
        files: &Files,
        directories: &BTreeSet<PathBuf>,
    ) -> Result<bool, PublicationError> {
        let root = match directory_at(parent, name, false) {
            Ok(root) => root,
            Err(error) if matches!(error.raw_os_error(), Some(code) if code == rustix::io::Errno::LOOP.raw_os_error() || code == rustix::io::Errno::NOTDIR.raw_os_error()) =>
            {
                return Ok(false);
            }
            Err(error) => {
                return Err(output_error(format!(
                    "Cannot open stored artifact: {error}"
                )));
            }
        };
        let traversal = fs::openat(&root, ".", directory_flags(), Mode::empty())
            .map_err(|error| output_error(error.to_string()))?;
        let mut pending = vec![(PathBuf::new(), traversal)];
        let mut seen_files = BTreeSet::new();
        let mut seen_directories = BTreeSet::new();
        while let Some((relative, directory)) = pending.pop() {
            let mut entries = Dir::read_from(&directory).map_err(|error| {
                output_error(format!("Cannot enumerate stored artifact: {error}"))
            })?;
            while let Some(entry) = entries.read() {
                let entry = entry.map_err(|error| output_error(error.to_string()))?;
                let name = entry.file_name();
                if name.to_bytes() == b"." || name.to_bytes() == b".." {
                    continue;
                }
                let path = relative.join(OsStr::from_bytes(name.to_bytes()));
                let stat =
                    fs::statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|error| {
                        output_error(format!("Cannot inspect stored artifact: {error}"))
                    })?;
                match FileType::from_raw_mode(stat.st_mode) {
                    FileType::Directory => {
                        if !directories.contains(&path) {
                            return Ok(false);
                        }
                        let child =
                            directory_at(&directory, OsStr::from_bytes(name.to_bytes()), false)
                                .map_err(|error| {
                                    output_error(format!(
                                        "Stored artifact directory changed: {error}"
                                    ))
                                })?;
                        if !still_named(&directory, OsStr::from_bytes(name.to_bytes()), &child)
                            .map_err(|error| output_error(error.to_string()))?
                        {
                            return Ok(false);
                        }
                        seen_directories.insert(path.clone());
                        pending.push((path, child));
                    }
                    FileType::RegularFile => {
                        let label = artifact_label(&path)?;
                        let Some(expected) = files.get(&label) else {
                            return Ok(false);
                        };
                        let fd = fs::openat(
                            &directory,
                            name,
                            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                            Mode::empty(),
                        )
                        .map_err(|error| {
                            output_error(format!("Cannot open stored artifact file: {error}"))
                        })?;
                        let file = File::from(fd);
                        if !still_named(&directory, OsStr::from_bytes(name.to_bytes()), &file)
                            .map_err(|error| output_error(error.to_string()))?
                        {
                            return Ok(false);
                        }
                        let metadata = file
                            .metadata()
                            .map_err(|error| output_error(error.to_string()))?;
                        if !metadata.is_file() || metadata.len() != expected.len() as u64 {
                            return Ok(false);
                        }
                        let mut bytes = Vec::new();
                        (&file)
                            .take(expected.len() as u64 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|error| {
                                output_error(format!("Cannot read stored artifact: {error}"))
                            })?;
                        if bytes != *expected
                            || !still_named(&directory, OsStr::from_bytes(name.to_bytes()), &file)
                                .map_err(|error| output_error(error.to_string()))?
                        {
                            return Ok(false);
                        }
                        seen_files.insert(label);
                    }
                    _ => return Ok(false),
                }
            }
        }
        Ok(seen_files.len() == files.len()
            && &seen_directories == directories
            && still_named(parent, name, &root).map_err(|error| output_error(error.to_string()))?)
    }

    /// A randomized directory whose creation, population and cleanup use held descriptors.
    struct Stage<'a> {
        parent: &'a OwnedFd,
        directory: OwnedFd,
        name: OsString,
        published: bool,
    }

    impl<'a> Stage<'a> {
        fn create(parent: &'a OwnedFd, parent_path: &Path) -> Result<Self, PublicationError> {
            // Builder supplies collision-resistant names; its pathname cleanup is disabled.
            // The callback uses only each generated basename and the already held parent.
            let temporary = tempfile::Builder::new()
                .prefix(".qargo-stage-")
                .rand_bytes(16)
                .disable_cleanup(true)
                .make_in(parent_path, |path| {
                    let name = path
                        .file_name()
                        .ok_or_else(|| io::Error::other("Invalid staging name."))?;
                    fs::mkdirat(parent, name, Mode::from_raw_mode(0o700))?;
                    fs::openat(parent, name, directory_flags(), Mode::empty()).map_err(Into::into)
                })
                .map_err(|error| {
                    output_error(format!("Cannot create artifact staging directory: {error}"))
                })?;
            let name = temporary
                .path()
                .file_name()
                .ok_or_else(|| output_error("Invalid staging directory name."))?
                .to_owned();
            let (directory, path) = temporary.into_parts();
            drop(path);
            Ok(Self {
                parent,
                directory,
                name,
                published: false,
            })
        }

        fn populate(
            &self,
            files: &Files,
            directories: &BTreeSet<PathBuf>,
        ) -> Result<(), PublicationError> {
            for path in directories {
                let mut parent = fs::openat(&self.directory, ".", directory_flags(), Mode::empty())
                    .map_err(|error| output_error(error.to_string()))?;
                for component in path.components() {
                    if let Component::Normal(name) = component {
                        parent = directory_at(&parent, name, true).map_err(|error| {
                            output_error(format!("Cannot create artifact directory: {error}"))
                        })?;
                    }
                }
            }
            for (label, bytes) in files {
                let path = Path::new(label);
                let mut parent = fs::openat(&self.directory, ".", directory_flags(), Mode::empty())
                    .map_err(|error| output_error(error.to_string()))?;
                if let Some(relative) = path.parent() {
                    for component in relative.components() {
                        if let Component::Normal(name) = component {
                            parent = directory_at(&parent, name, true).map_err(|error| {
                                output_error(format!(
                                    "Cannot create artifact module directory: {error}"
                                ))
                            })?;
                        }
                    }
                }
                let name = path
                    .file_name()
                    .ok_or_else(|| output_error("Invalid artifact file name."))?;
                let fd = fs::openat(
                    &parent,
                    name,
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o644),
                )
                .map_err(|error| output_error(format!("Cannot create artifact file: {error}")))?;
                File::from(fd).write_all(bytes).map_err(|error| {
                    output_error(format!("Cannot write artifact file: {error}"))
                })?;
            }
            Ok(())
        }

        fn still_named(&self) -> io::Result<bool> {
            still_named(self.parent, self.name.as_os_str(), &self.directory)
        }
    }

    fn cleanup(directory: &OwnedFd) -> io::Result<()> {
        let mut entries = Dir::read_from(directory)?;
        while let Some(entry) = entries.read() {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            let metadata = fs::statat(directory, name, AtFlags::SYMLINK_NOFOLLOW)?;
            if FileType::from_raw_mode(metadata.st_mode) == FileType::Directory {
                let child = directory_at(directory, OsStr::from_bytes(name.to_bytes()), false)?;
                cleanup(&child)?;
                fs::unlinkat(directory, name, AtFlags::REMOVEDIR)?;
            } else {
                fs::unlinkat(directory, name, AtFlags::empty())?;
            }
        }
        Ok(())
    }

    impl Drop for Stage<'_> {
        fn drop(&mut self) {
            if !self.published {
                let _ = cleanup(&self.directory);
                if self.still_named().unwrap_or(false) {
                    let _ = fs::unlinkat(self.parent, self.name.as_os_str(), AtFlags::REMOVEDIR);
                }
            }
        }
    }

    pub(super) fn publish(
        output: &Path,
        files: &Files,
        directories: &BTreeSet<PathBuf>,
    ) -> Result<(), PublicationError> {
        publish_with_hook(output, files, directories, || {})
    }

    pub(super) fn matches(
        output: &Path,
        files: &Files,
        directories: &BTreeSet<PathBuf>,
    ) -> Result<bool, PublicationError> {
        let directories = expected_directories(files, directories)?;
        let destination = absolute_normalized(output)?;
        let parent_path = destination
            .parent()
            .ok_or_else(|| output_error("Invalid output parent."))?;
        let name = destination
            .file_name()
            .ok_or_else(|| output_error("Invalid output name."))?;
        let parent = open_parent(parent_path, false)?;
        let matches = artifacts_match(&parent, name, files, &directories)?;
        validate_parent(&parent, parent_path)?;
        Ok(matches)
    }

    fn publish_with_hook(
        output: &Path,
        files: &Files,
        extra_directories: &BTreeSet<PathBuf>,
        before_install: impl FnOnce(),
    ) -> Result<(), PublicationError> {
        let directories = expected_directories(files, extra_directories)?;
        let destination = absolute_normalized(output)?;
        let parent_path = destination
            .parent()
            .ok_or_else(|| output_error("Invalid output parent."))?;
        let name = destination
            .file_name()
            .ok_or_else(|| output_error("Invalid output name."))?;
        let parent = open_parent(parent_path, true)?;
        match fs::statat(&parent, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => {
                return if artifacts_match(&parent, name, files, &directories)? {
                    validate_parent(&parent, parent_path)?;
                    Ok(())
                } else {
                    Err(PublicationError::Mismatch)
                };
            }
            Err(rustix::io::Errno::NOENT) => {}
            Err(error) => {
                return Err(output_error(format!(
                    "Cannot inspect artifact output: {error}"
                )));
            }
        }
        let mut stage = Stage::create(&parent, parent_path)?;
        stage.populate(files, &directories)?;
        before_install();
        let current_parent = open_parent(parent_path, false)?;
        if !same_directory(&parent, &current_parent)
            .map_err(|error| output_error(error.to_string()))?
            || !stage
                .still_named()
                .map_err(|error| output_error(error.to_string()))?
        {
            return Err(output_error(
                "Artifact output ancestors or staging directory changed during publication.",
            ));
        }
        match fs::renameat_with(
            &parent,
            stage.name.as_os_str(),
            &parent,
            name,
            RenameFlags::NOREPLACE,
        ) {
            Ok(()) => {
                stage.published = true;
                validate_parent(&parent, parent_path)?;
                Ok(())
            }
            Err(rustix::io::Errno::EXIST) => {
                if artifacts_match(&parent, name, files, &directories)? {
                    validate_parent(&parent, parent_path)?;
                    Ok(())
                } else {
                    Err(PublicationError::Mismatch)
                }
            }
            Err(error) => Err(output_error(format!(
                "Cannot atomically publish artifact: {error}"
            ))),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs as host_fs;
        use std::os::unix::fs::symlink;

        fn fixture() -> (tempfile::TempDir, PathBuf, Files) {
            let temp = tempfile::tempdir().unwrap();
            let root = host_fs::canonicalize(temp.path()).unwrap();
            let files = Files::from([
                ("index.md".into(), b"complete index".to_vec()),
                ("modules/a.md".into(), b"complete module".to_vec()),
            ]);
            (temp, root, files)
        }

        #[test]
        fn ancestor_swapped_for_symlink_cannot_redirect_staging_or_publication() {
            let (_temp, root, files) = fixture();
            let parent = root.join("parent");
            let moved = root.join("moved");
            let elsewhere = root.join("elsewhere");
            host_fs::create_dir(&parent).unwrap();
            host_fs::create_dir(&elsewhere).unwrap();
            let result = publish_with_hook(&parent.join("docs"), &files, &BTreeSet::new(), || {
                host_fs::rename(&parent, &moved).unwrap();
                symlink(&elsewhere, &parent).unwrap();
            });
            assert!(result.is_err());
            assert_eq!(host_fs::read_dir(&elsewhere).unwrap().count(), 0);
            assert_eq!(host_fs::read_dir(&moved).unwrap().count(), 0);
        }

        #[test]
        fn a_concurrent_final_directory_is_never_replaced() {
            let (_temp, root, files) = fixture();
            let destination = root.join("docs");
            let result = publish_with_hook(&destination, &files, &BTreeSet::new(), || {
                host_fs::create_dir(&destination).unwrap();
            });
            assert!(matches!(result.unwrap_err(), PublicationError::Mismatch));
            assert_eq!(host_fs::read_dir(&destination).unwrap().count(), 0);
            assert_eq!(host_fs::read_dir(&root).unwrap().count(), 1);
        }

        #[test]
        fn an_identical_concurrent_publication_is_reused() {
            let (_temp, root, files) = fixture();
            let destination = root.join("docs");
            publish_with_hook(&destination, &files, &BTreeSet::new(), || {
                publish(&destination, &files, &BTreeSet::new()).unwrap()
            })
            .unwrap();
            assert_eq!(
                host_fs::read(destination.join("index.md")).unwrap(),
                files["index.md"]
            );
            assert_eq!(host_fs::read_dir(&root).unwrap().count(), 1);
        }

        #[test]
        fn staging_creation_population_and_cleanup_survive_an_ancestor_swap() {
            let (_temp, root, files) = fixture();
            let parent_path = root.join("parent");
            let moved = root.join("moved");
            let elsewhere = root.join("elsewhere");
            host_fs::create_dir(&parent_path).unwrap();
            host_fs::create_dir(&elsewhere).unwrap();
            let parent = open_parent(&parent_path, false).unwrap();
            host_fs::rename(&parent_path, &moved).unwrap();
            symlink(&elsewhere, &parent_path).unwrap();
            let stage = Stage::create(&parent, &parent_path).unwrap();
            stage
                .populate(
                    &files,
                    &expected_directories(&files, &BTreeSet::new()).unwrap(),
                )
                .unwrap();
            assert_eq!(host_fs::read_dir(&elsewhere).unwrap().count(), 0);
            assert_eq!(host_fs::read_dir(&moved).unwrap().count(), 1);
            drop(stage);
            assert_eq!(host_fs::read_dir(&moved).unwrap().count(), 0);
        }

        #[test]
        fn build_layout_retains_empty_roots_and_exact_directory_inventory() {
            let (_temp, root, _) = fixture();
            let files = Files::from([
                ("snapshot/Qargo.toml".into(), b"captured manifest".to_vec()),
                ("build-record.json".into(), b"captured record".to_vec()),
                ("module-index.json".into(), b"[]".to_vec()),
            ]);
            let directories = BTreeSet::from([
                PathBuf::from("snapshot/src"),
                PathBuf::from("snapshot/tests"),
                PathBuf::from("snapshot/docs"),
            ]);
            let output = root.join("build");
            publish_with_hook(&output, &files, &directories, || {
                publish(&output, &files, &directories).unwrap();
            })
            .unwrap();
            assert!(matches(&output, &files, &directories).unwrap());
            for directory in &directories {
                assert!(output.join(directory).is_dir());
            }
            host_fs::remove_dir(output.join("snapshot/tests")).unwrap();
            assert!(!matches(&output, &files, &directories).unwrap());
            assert!(matches!(
                publish(&output, &files, &directories),
                Err(PublicationError::Mismatch)
            ));
            host_fs::create_dir(output.join("snapshot/tests")).unwrap();
            host_fs::create_dir(output.join("unexpected")).unwrap();
            assert!(!matches(&output, &files, &directories).unwrap());
        }

        #[test]
        fn stage_name_swapped_for_symlink_is_not_published_or_followed() {
            let (_temp, root, files) = fixture();
            let elsewhere = root.join("elsewhere");
            let moved = root.join("moved-stage");
            host_fs::create_dir(&elsewhere).unwrap();
            let output = root.join("output");
            let result = publish_with_hook(&output, &files, &BTreeSet::new(), || {
                let stage = host_fs::read_dir(&root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".qargo-stage-")
                    })
                    .unwrap();
                host_fs::rename(&stage, &moved).unwrap();
                symlink(&elsewhere, stage).unwrap();
            });
            assert!(result.is_err());
            assert!(!output.exists());
            assert_eq!(host_fs::read_dir(&elsewhere).unwrap().count(), 0);
            assert_eq!(host_fs::read_dir(&moved).unwrap().count(), 0);
        }
    }
}
