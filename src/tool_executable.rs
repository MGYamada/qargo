//! Capture selected executables once so installation-path swaps cannot redirect launch.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::executable::{digest_file, identity_error};
use crate::report::Diagnostic;

const EXECUTABLE_BYTES: u64 = 256 << 20;

pub(crate) struct SelectedExecutable {
    digest: String,
    path: PathBuf,
    #[cfg(unix)]
    file: File,
    #[cfg(target_os = "macos")]
    stage: MacStage,
    #[cfg(target_os = "macos")]
    file_state: std::fs::Metadata,
    #[cfg(target_os = "macos")]
    directory_state: std::fs::Metadata,
}

impl SelectedExecutable {
    pub(crate) fn capture(path: &Path) -> Result<Self, Diagnostic> {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
        {
            use std::fs::OpenOptions;
            use std::io::{Read, Write};
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

            use crate::executable::same_file_state;

            let mut source = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(path)
                .map_err(|error| {
                    identity_error(format!("Cannot open selected executable: {error}"))
                })?;
            let before = source
                .metadata()
                .map_err(|error| identity_error(error.to_string()))?;
            if !before.is_file() || before.permissions().mode() & 0o111 == 0 {
                return Err(identity_error(
                    "Selected executable must be an executable ordinary file.",
                ));
            }

            #[cfg(any(target_os = "linux", target_os = "android"))]
            let mut file = {
                use rustix::fs::{MemfdFlags, memfd_create};
                let flags = MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING;
                let fd = match memfd_create("qargo-tool", flags | MemfdFlags::EXEC) {
                    Ok(fd) => fd,
                    Err(rustix::io::Errno::INVAL) => memfd_create("qargo-tool", flags)
                        .map_err(|error| identity_error(error.to_string()))?,
                    Err(error) => {
                        return Err(identity_error(format!(
                            "Cannot create executable snapshot: {error}"
                        )));
                    }
                };
                File::from(fd)
            };
            #[cfg(target_os = "macos")]
            let (stage, mut file) = {
                let stage = tempfile::Builder::new()
                    .prefix("qargo-tool-")
                    .tempdir()
                    .map_err(|error| identity_error(error.to_string()))?;
                let file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .read(true)
                    .mode(0o600)
                    .open(stage.path().join("tool"))
                    .map_err(|error| identity_error(error.to_string()))?;
                (stage, file)
            };
            let count = std::io::copy(
                &mut Read::by_ref(&mut source).take(EXECUTABLE_BYTES + 1),
                &mut file,
            )
            .map_err(|error| {
                identity_error(format!("Cannot capture selected executable: {error}"))
            })?;
            if count > EXECUTABLE_BYTES {
                return Err(identity_error(
                    "Selected executable exceeds the 256 MiB capture limit.",
                ));
            }
            if !same_file_state(
                &before,
                &source
                    .metadata()
                    .map_err(|error| identity_error(error.to_string()))?,
            ) || count != before.len()
            {
                return Err(identity_error(
                    "Selected executable changed during capture.",
                ));
            }
            file.flush()
                .map_err(|error| identity_error(error.to_string()))?;
            file.set_permissions(std::fs::Permissions::from_mode(0o500))
                .map_err(|error| identity_error(error.to_string()))?;

            #[cfg(any(target_os = "linux", target_os = "android"))]
            {
                use rustix::fs::{SealFlags, fcntl_add_seals};
                use std::io::{Seek, SeekFrom};
                use std::os::fd::AsRawFd;
                fcntl_add_seals(
                    &file,
                    SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
                )
                .map_err(|error| {
                    identity_error(format!("Cannot seal selected executable: {error}"))
                })?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|error| identity_error(error.to_string()))?;
                let digest = digest_file(
                    file.try_clone()
                        .map_err(|error| identity_error(error.to_string()))?,
                )?;
                // The descriptor belongs to the live parent. Shebang interpreters can reopen
                // it after the child's CLOEXEC descriptors close, without leaking an FD.
                let path = PathBuf::from(format!(
                    "/proc/{}/fd/{}",
                    std::process::id(),
                    file.as_raw_fd()
                ));
                Ok(Self { digest, path, file })
            }
            #[cfg(target_os = "macos")]
            {
                file.sync_all()
                    .map_err(|error| identity_error(error.to_string()))?;
                drop(file);
                let stage_path = std::fs::canonicalize(stage.path())
                    .map_err(|error| identity_error(error.to_string()))?;
                let path = stage_path.join("tool");
                let file = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&path)
                    .map_err(|error| identity_error(error.to_string()))?;
                let digest = digest_file(
                    file.try_clone()
                        .map_err(|error| identity_error(error.to_string()))?,
                )?;
                let directory = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
                    .open(&stage_path)
                    .map_err(|error| identity_error(error.to_string()))?;
                // The selected installation path is never reopened for launch. Only Qargo
                // owns this private stage; no child receives its path in arguments or env.
                let stage = MacStage {
                    _root: stage,
                    directory,
                };
                stage
                    .directory
                    .set_permissions(std::fs::Permissions::from_mode(0o500))
                    .map_err(|error| identity_error(error.to_string()))?;
                let file_state = file
                    .metadata()
                    .map_err(|error| identity_error(error.to_string()))?;
                let directory_state = stage
                    .directory
                    .metadata()
                    .map_err(|error| identity_error(error.to_string()))?;
                Ok(Self {
                    digest,
                    path,
                    file,
                    stage,
                    file_state,
                    directory_state,
                })
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
        {
            let _ = path;
            Err(identity_error(
                "Object-bound executable launch is unavailable on this platform.",
            ))
        }
    }

    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }

    pub(crate) fn command(&self) -> Result<Command, Diagnostic> {
        self.verify()?;
        Ok(Command::new(&self.path))
    }

    pub(crate) fn verify(&self) -> Result<(), Diagnostic> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            use rustix::fs::{SealFlags, fcntl_get_seals};
            let seals =
                fcntl_get_seals(&self.file).map_err(|error| identity_error(error.to_string()))?;
            if !seals
                .contains(SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL)
            {
                return Err(identity_error(
                    "Selected executable snapshot is not sealed.",
                ));
            }
        }
        #[cfg(target_os = "macos")]
        {
            use crate::executable::same_file_state;
            for (held, path, expected) in [
                (&self.file, self.path.as_path(), &self.file_state),
                (
                    &self.stage.directory,
                    self.path.parent().expect("private parent"),
                    &self.directory_state,
                ),
            ] {
                let current = held
                    .metadata()
                    .map_err(|error| identity_error(error.to_string()))?;
                let named = std::fs::symlink_metadata(path)
                    .map_err(|error| identity_error(error.to_string()))?;
                if !same_file_state(expected, &current) || !same_file_state(expected, &named) {
                    return Err(identity_error(
                        "Private executable snapshot changed during execution.",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
struct MacStage {
    _root: tempfile::TempDir,
    directory: File,
}

#[cfg(target_os = "macos")]
impl Drop for MacStage {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        // Restore write permission before TempDir cleanup, including capture errors.
        let _ = self
            .directory
            .set_permissions(std::fs::Permissions::from_mode(0o700));
    }
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "android", target_os = "macos")
))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn selected_path_aba_cannot_change_the_captured_executable() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("tool");
        fs::write(&original, "#!/bin/sh\nprintf 'A'\n").unwrap();
        fs::set_permissions(&original, fs::Permissions::from_mode(0o700)).unwrap();
        let expected = crate::snapshot::digest_path(&original).unwrap();
        let tool = SelectedExecutable::capture(&original).unwrap();
        assert_eq!(tool.digest(), expected);
        let mut command = tool.command().unwrap();
        // Deterministic process-creation boundary: A was hashed, B occupies its
        // installation path at spawn, and A is restored before validation.
        fs::rename(&original, root.path().join("A")).unwrap();
        fs::write(&original, "#!/bin/sh\nprintf 'B'\n").unwrap();
        fs::set_permissions(&original, fs::Permissions::from_mode(0o700)).unwrap();
        let output = crate::tool_process::bounded_output(&mut command).unwrap();
        fs::remove_file(&original).unwrap();
        fs::rename(root.path().join("A"), &original).unwrap();
        tool.verify().unwrap();
        assert_eq!(output.stdout, b"A");
        assert_eq!(crate::snapshot::digest_path(&original).unwrap(), expected);
        output.accept();
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn anonymous_snapshot_cannot_be_modified_or_truncated() {
        use std::io::Write;
        let tool = SelectedExecutable::capture(Path::new("/bin/sh")).unwrap();
        let mut file = tool.file.try_clone().unwrap();
        assert!(file.write_all(b"modified").is_err());
        assert!(file.set_len(0).is_err());
        tool.verify().unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn private_stage_aba_is_rejected_and_cleanup_restores_directory_access() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("tool");
        fs::write(&original, "#!/bin/sh\nprintf 'A'\n").unwrap();
        fs::set_permissions(&original, fs::Permissions::from_mode(0o700)).unwrap();
        let tool = SelectedExecutable::capture(&original).unwrap();
        let stage = tool.path.parent().unwrap().to_path_buf();
        let mut command = tool.command().unwrap();
        // Deliberate same-user tampering of Qargo's private state is detected.
        tool.stage
            .directory
            .set_permissions(fs::Permissions::from_mode(0o700))
            .unwrap();
        fs::rename(&tool.path, stage.join("A")).unwrap();
        fs::write(&tool.path, "#!/bin/sh\nprintf 'B'\n").unwrap();
        fs::set_permissions(&tool.path, fs::Permissions::from_mode(0o500)).unwrap();
        let output = crate::tool_process::bounded_output(&mut command).unwrap();
        fs::remove_file(&tool.path).unwrap();
        fs::rename(stage.join("A"), &tool.path).unwrap();
        tool.stage
            .directory
            .set_permissions(fs::Permissions::from_mode(0o500))
            .unwrap();
        assert!(tool.verify().is_err());
        assert_eq!(output.stdout, b"B");
        drop(output);
        drop(tool);
        assert!(!stage.exists());
    }
}
