//! Descriptor-anchored publication: ancestor changes cannot redirect filesystem operations.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use rustix::fd::{AsFd, OwnedFd};
use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags, RenameFlags};

use super::{Diagnostic, Files, absolute_normalized, mismatch, output_error, portable_relative};

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

fn open_parent(path: &Path, create: bool) -> Result<OwnedFd, Diagnostic> {
    let mut directory = fs::open("/", directory_flags(), Mode::empty())
        .map_err(|error| output_error(format!("Cannot open filesystem root: {error}")))?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = directory_at(&directory, name, create).map_err(|error| {
                    output_error(format!(
                        "Output ancestors must be directories without symlinks: {error}"
                    ))
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

fn expected_directories(files: &Files) -> Result<BTreeSet<PathBuf>, Diagnostic> {
    let mut directories = BTreeSet::new();
    for label in files.keys() {
        if portable_relative(Path::new(label))? != *label {
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
    Ok(directories)
}

fn artifacts_match(
    parent: &OwnedFd,
    name: &OsStr,
    files: &Files,
    directories: &BTreeSet<PathBuf>,
) -> Result<bool, Diagnostic> {
    let root = match directory_at(parent, name, false) {
        Ok(root) => root,
        Err(error) if matches!(error.raw_os_error(), Some(code) if code == rustix::io::Errno::LOOP.raw_os_error() || code == rustix::io::Errno::NOTDIR.raw_os_error()) =>
        {
            return Ok(false);
        }
        Err(error) => {
            return Err(output_error(format!(
                "Cannot open stored documentation: {error}"
            )));
        }
    };
    let mut pending = vec![(PathBuf::new(), root)];
    let mut seen_files = BTreeSet::new();
    let mut seen_directories = BTreeSet::new();
    while let Some((relative, directory)) = pending.pop() {
        let mut entries = Dir::read_from(&directory).map_err(|error| {
            output_error(format!("Cannot enumerate stored documentation: {error}"))
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
                    output_error(format!("Cannot inspect stored documentation: {error}"))
                })?;
            match FileType::from_raw_mode(stat.st_mode) {
                FileType::Directory => {
                    if !directories.contains(&path) {
                        return Ok(false);
                    }
                    let child = directory_at(&directory, OsStr::from_bytes(name.to_bytes()), false)
                        .map_err(|error| {
                            output_error(format!("Stored documentation directory changed: {error}"))
                        })?;
                    seen_directories.insert(path.clone());
                    pending.push((path, child));
                }
                FileType::RegularFile => {
                    let label = portable_relative(&path)?;
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
                        output_error(format!("Cannot open stored documentation file: {error}"))
                    })?;
                    let file = File::from(fd);
                    let metadata = file
                        .metadata()
                        .map_err(|error| output_error(error.to_string()))?;
                    if !metadata.is_file() || metadata.len() != expected.len() as u64 {
                        return Ok(false);
                    }
                    let mut bytes = Vec::new();
                    file.take(expected.len() as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|error| {
                            output_error(format!("Cannot read stored documentation: {error}"))
                        })?;
                    if bytes != *expected {
                        return Ok(false);
                    }
                    seen_files.insert(label);
                }
                _ => return Ok(false),
            }
        }
    }
    Ok(seen_files.len() == files.len() && &seen_directories == directories)
}

/// A randomized directory whose creation, population and cleanup use held descriptors.
struct Stage<'a> {
    parent: &'a OwnedFd,
    directory: OwnedFd,
    name: OsString,
    published: bool,
}

impl<'a> Stage<'a> {
    fn create(parent: &'a OwnedFd, parent_path: &Path) -> Result<Self, Diagnostic> {
        // Builder supplies collision-resistant names; its pathname cleanup is disabled.
        // The callback uses only each generated basename and the already held parent.
        let temporary = tempfile::Builder::new()
            .prefix(".qlidoc-stage-")
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
                output_error(format!(
                    "Cannot create documentation staging directory: {error}"
                ))
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

    fn populate(&self, files: &Files) -> Result<(), Diagnostic> {
        for (label, bytes) in files {
            let path = Path::new(label);
            let mut parent = fs::openat(&self.directory, ".", directory_flags(), Mode::empty())
                .map_err(|error| output_error(error.to_string()))?;
            if let Some(relative) = path.parent() {
                for component in relative.components() {
                    if let Component::Normal(name) = component {
                        parent = directory_at(&parent, name, true).map_err(|error| {
                            output_error(format!(
                                "Cannot create documentation module directory: {error}"
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
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o644),
            )
            .map_err(|error| output_error(format!("Cannot create documentation file: {error}")))?;
            File::from(fd).write_all(bytes).map_err(|error| {
                output_error(format!("Cannot write documentation file: {error}"))
            })?;
        }
        Ok(())
    }

    fn still_named(&self) -> io::Result<bool> {
        let named = fs::statat(
            self.parent,
            self.name.as_os_str(),
            AtFlags::SYMLINK_NOFOLLOW,
        )?;
        let held = fs::fstat(&self.directory)?;
        Ok(
            FileType::from_raw_mode(named.st_mode) == FileType::Directory
                && named.st_dev == held.st_dev
                && named.st_ino == held.st_ino,
        )
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

pub(super) fn publish(output: &Path, files: &Files) -> Result<(), Diagnostic> {
    publish_with_hook(output, files, || {})
}

fn publish_with_hook(
    output: &Path,
    files: &Files,
    before_install: impl FnOnce(),
) -> Result<(), Diagnostic> {
    let directories = expected_directories(files)?;
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
                Ok(())
            } else {
                Err(mismatch())
            };
        }
        Err(rustix::io::Errno::NOENT) => {}
        Err(error) => {
            return Err(output_error(format!(
                "Cannot inspect documentation output: {error}"
            )));
        }
    }
    let mut stage = Stage::create(&parent, parent_path)?;
    stage.populate(files)?;
    before_install();
    let current_parent = open_parent(parent_path, false)?;
    if !same_directory(&parent, &current_parent).map_err(|error| output_error(error.to_string()))?
        || !stage
            .still_named()
            .map_err(|error| output_error(error.to_string()))?
    {
        return Err(output_error(
            "Documentation output ancestors or staging directory changed during publication.",
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
            Ok(())
        }
        Err(rustix::io::Errno::EXIST) => {
            if artifacts_match(&parent, name, files, &directories)? {
                Ok(())
            } else {
                Err(mismatch())
            }
        }
        Err(error) => Err(output_error(format!(
            "Cannot atomically publish documentation: {error}"
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
        let result = publish_with_hook(&parent.join("docs"), &files, || {
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
        let result = publish_with_hook(&destination, &files, || {
            host_fs::create_dir(&destination).unwrap();
        });
        assert_eq!(result.unwrap_err().id, "artifact_mismatch");
        assert_eq!(host_fs::read_dir(&destination).unwrap().count(), 0);
        assert_eq!(host_fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn an_identical_concurrent_publication_is_reused() {
        let (_temp, root, files) = fixture();
        let destination = root.join("docs");
        publish_with_hook(&destination, &files, || {
            publish(&destination, &files).unwrap()
        })
        .unwrap();
        assert_eq!(
            host_fs::read(destination.join("index.md")).unwrap(),
            files["index.md"]
        );
        assert_eq!(host_fs::read_dir(&root).unwrap().count(), 1);
    }
}
