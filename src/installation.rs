//! Publish complete build directories without replacing a concurrent result.

use std::io;
use std::path::Path;

pub(crate) fn rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            source,
            rustix::fs::CWD,
            destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(Into::into)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
    {
        let _ = (source, destination);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Atomic directory publication without replacement is unavailable on this platform.",
        ))
    }
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn concurrent_empty_directory_is_never_replaced() {
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("stage");
        let destination = root.path().join("published");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("record.json"), "complete").unwrap();
        fs::create_dir(&destination).unwrap();
        assert_eq!(
            rename_noreplace(&staged, &destination).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        assert_eq!(
            fs::read_to_string(staged.join("record.json")).unwrap(),
            "complete"
        );
    }

    #[test]
    fn new_destination_receives_the_complete_directory() {
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("stage");
        let destination = root.path().join("published");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("record.json"), "complete").unwrap();
        rename_noreplace(&staged, &destination).unwrap();
        assert!(!staged.exists());
        assert_eq!(
            fs::read_to_string(destination.join("record.json")).unwrap(),
            "complete"
        );
    }
}
