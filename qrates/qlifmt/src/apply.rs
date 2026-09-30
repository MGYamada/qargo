//! Per-file atomic replacement through held directories, without following links.

// The public failure contract carries a diagnostic and the completed update list.
#![allow(clippy::result_large_err)]

use std::path::Path;

use qlippy_engine::report::Diagnostic;
use qlippy_engine::snapshot::Files;

use super::{changed_files, validate_formatted};

#[derive(Debug)]
pub struct ApplyFailure {
    pub diagnostic: Diagnostic,
    pub updated_files: Vec<String>,
}

fn failure(diagnostic: Diagnostic, updated_files: &[String]) -> ApplyFailure {
    ApplyFailure {
        diagnostic,
        updated_files: updated_files.to_vec(),
    }
}

fn changed(label: &str) -> Diagnostic {
    Diagnostic::error(
        "source_changed",
        "qargo",
        format!("Source changed after capture: {label}"),
    )
}

fn io_error(label: &str, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error(
        "format_write",
        "qargo",
        format!("Cannot safely update source {label}: {error}"),
    )
}

/// Validate all candidates and all captured current bytes before the first write.
/// Each replacement is atomic; a later failure reports the already updated prefix.
pub fn apply_files(
    root: &Path,
    original: &Files,
    formatted: &Files,
) -> Result<Vec<String>, ApplyFailure> {
    validate_formatted(original, formatted).map_err(|diagnostic| failure(diagnostic, &[]))?;
    if original.is_empty() {
        return Ok(Vec::new());
    }
    apply(root, original, formatted)
}

#[cfg(unix)]
fn apply(root: &Path, original: &Files, formatted: &Files) -> Result<Vec<String>, ApplyFailure> {
    apply_with_hook(root, original, formatted, |_, _| {})
}

#[cfg(unix)]
fn apply_with_hook(
    root: &Path,
    original: &Files,
    formatted: &Files,
    mut before_replace: impl FnMut(&str, usize),
) -> Result<Vec<String>, ApplyFailure> {
    use std::fs::{self, File};
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

    use rustix::fs::{AtFlags, Mode, OFlags, fstat, open, openat, renameat, unlinkat};

    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let metadata =
        fs::symlink_metadata(root).map_err(|error| failure(io_error("root", error), &[]))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failure(
            io_error("root", "Source root must be an ordinary directory."),
            &[],
        ));
    }
    let root_file = File::from(
        open(root, directory_flags, Mode::empty())
            .map_err(|error| failure(io_error("root", error), &[]))?,
    );
    let root_identity = fstat(&root_file).map_err(|error| failure(io_error("root", error), &[]))?;

    fn parent(root: &File, label: &str, flags: OFlags) -> Result<(File, String), Diagnostic> {
        let mut directory = root.try_clone().map_err(|error| io_error(label, error))?;
        let mut components = label.split('/').peekable();
        while let Some(component) = components.next() {
            if components.peek().is_none() {
                return Ok((directory, component.to_owned()));
            }
            directory = File::from(
                openat(&directory, component, flags, Mode::empty())
                    .map_err(|error| io_error(label, error))?,
            );
        }
        Err(io_error(label, "Invalid empty source path."))
    }

    fn current(directory: &File, name: &str, label: &str) -> Result<Vec<u8>, Diagnostic> {
        let fd = openat(
            directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| io_error(label, error))?;
        let file = File::from(fd);
        if !file
            .metadata()
            .map_err(|error| io_error(label, error))?
            .is_file()
        {
            return Err(io_error(label, "Source must be an ordinary file."));
        }
        let mut bytes = Vec::new();
        file.take(qlippy_engine::snapshot::FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| io_error(label, error))?;
        Ok(bytes)
    }

    struct Staged {
        parent: File,
        temporary: Option<String>,
        name: String,
        label: String,
    }
    impl Drop for Staged {
        fn drop(&mut self) {
            if let Some(name) = &self.temporary {
                let _ = unlinkat(&self.parent, name, AtFlags::empty());
            }
        }
    }

    // All selected source paths are checked, including files with no changes.
    for (label, expected) in original {
        let (directory, name) =
            parent(&root_file, label, directory_flags).map_err(|error| failure(error, &[]))?;
        if current(&directory, &name, label).map_err(|error| failure(error, &[]))? != *expected {
            return Err(failure(changed(label), &[]));
        }
    }
    let mut staged = Vec::new();
    for label in changed_files(original, formatted) {
        let (directory, name) =
            parent(&root_file, &label, directory_flags).map_err(|error| failure(error, &[]))?;
        let input = File::from(
            openat(
                &directory,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| failure(io_error(&label, error), &[]))?,
        );
        let metadata = input
            .metadata()
            .map_err(|error| failure(io_error(&label, error), &[]))?;
        if !metadata.is_file() {
            return Err(failure(
                io_error(&label, "Source must be an ordinary file."),
                &[],
            ));
        }
        let permissions = metadata.permissions();
        let temporary = format!(
            ".qlifmt-{}-{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let fd = openat(
            &directory,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|error| failure(io_error(&label, error), &[]))?;
        let candidate = Staged {
            parent: directory,
            temporary: Some(temporary),
            name,
            label,
        };
        let mut output = File::from(fd);
        output
            .write_all(&formatted[&candidate.label])
            .and_then(|()| output.set_permissions(permissions))
            .and_then(|()| output.sync_all())
            .map_err(|error| failure(io_error(&candidate.label, error), &[]))?;
        staged.push(candidate);
    }
    let mut updated = Vec::new();
    for mut candidate in staged {
        before_replace(&candidate.label, updated.len());
        let new_root = File::from(
            open(root, directory_flags, Mode::empty())
                .map_err(|error| failure(io_error("root", error), &updated))?,
        );
        let identity =
            fstat(&new_root).map_err(|error| failure(io_error("root", error), &updated))?;
        if identity.st_dev != root_identity.st_dev || identity.st_ino != root_identity.st_ino {
            return Err(failure(changed("root"), &updated));
        }
        let (fresh_parent, _) = parent(&new_root, &candidate.label, directory_flags)
            .map_err(|error| failure(error, &updated))?;
        let old_parent = fstat(&candidate.parent)
            .map_err(|error| failure(io_error(&candidate.label, error), &updated))?;
        let new_parent = fstat(&fresh_parent)
            .map_err(|error| failure(io_error(&candidate.label, error), &updated))?;
        if old_parent.st_dev != new_parent.st_dev || old_parent.st_ino != new_parent.st_ino {
            return Err(failure(changed(&candidate.label), &updated));
        }
        if current(&candidate.parent, &candidate.name, &candidate.label)
            .map_err(|error| failure(error, &updated))?
            != original[&candidate.label]
        {
            return Err(failure(changed(&candidate.label), &updated));
        }
        renameat(
            &candidate.parent,
            candidate.temporary.as_deref().expect("staged file"),
            &candidate.parent,
            candidate.name.as_str(),
        )
        .map_err(|error| failure(io_error(&candidate.label, error), &updated))?;
        candidate.temporary = None;
        updated.push(candidate.label.clone());
    }
    Ok(updated)
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::format_files;
    use super::*;

    #[test]
    fn later_io_failure_reports_the_atomic_update_prefix_and_cleans_staging() {
        let directory = tempfile::tempdir().unwrap();
        let source = b"pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";
        let original = Files::from([
            ("a.qli".into(), source.to_vec()),
            ("b.qli".into(), source.to_vec()),
        ]);
        for (label, bytes) in &original {
            std::fs::write(directory.path().join(label), bytes).unwrap();
        }
        let formatted = format_files(&original).unwrap();
        let failed = apply_with_hook(directory.path(), &original, &formatted, |label, count| {
            if count == 1 {
                std::fs::remove_file(directory.path().join(label)).unwrap();
            }
        })
        .unwrap_err();
        assert_eq!(failed.updated_files, ["a.qli"]);
        assert_eq!(failed.diagnostic.id, "format_write");
        assert_eq!(
            std::fs::read(directory.path().join("a.qli")).unwrap(),
            formatted["a.qli"]
        );
        assert!(std::fs::read_dir(directory.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".qlifmt-")
        }));
    }
}

#[cfg(not(unix))]
fn apply(_root: &Path, _original: &Files, _formatted: &Files) -> Result<Vec<String>, ApplyFailure> {
    Err(failure(
        io_error(
            "root",
            "Atomic formatter updates are supported only on Linux and macOS.",
        ),
        &[],
    ))
}
