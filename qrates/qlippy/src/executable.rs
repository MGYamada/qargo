//! Executable identities from held files and the operating system's running image.

use std::fs::{File, Metadata};
use std::io::Read;

use sha2::{Digest, Sha256};

use crate::support::report::Diagnostic;

pub fn identity_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("tool_identity", "tool", message)
}

/// Ignore access time, which hashing itself can change.
#[cfg(unix)]
pub fn same_file_state(left: &Metadata, right: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

/// Hash an already-open ordinary file, rejecting writes observed during the read.
pub fn digest_file(mut file: File) -> Result<String, Diagnostic> {
    let before = file
        .metadata()
        .map_err(|error| identity_error(error.to_string()))?;
    if !before.is_file() {
        return Err(identity_error(
            "Executable identity requires an ordinary file.",
        ));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| identity_error(error.to_string()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    #[cfg(unix)]
    if !same_file_state(
        &before,
        &file
            .metadata()
            .map_err(|error| identity_error(error.to_string()))?,
    ) {
        return Err(identity_error("Executable bytes changed during hashing."));
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

/// Identify the running executable object, never an unchecked current_exe pathname.
pub fn running_digest() -> Result<String, Diagnostic> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let image = File::open("/proc/self/exe").map_err(|error| {
            identity_error(format!("Cannot open running executable object: {error}"))
        })?;
        digest_file(image)
    }
    #[cfg(target_os = "macos")]
    {
        macos::running_digest()
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
    {
        Err(identity_error(
            "Running executable object identity is unavailable on this platform.",
        ))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::OsStr;
    use std::fs::OpenOptions;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::Path;

    use libproc::bsd_info::BSDInfo;
    use libproc::net_info::VInfoStat;
    use libproc::proc_pid::{PIDInfo, PidInfoFlavor, pidinfo};

    use super::{Diagnostic, digest_file, identity_error};

    // Darwin <sys/proc_info.h>, PROC_PIDREGIONPATHINFO (available before macOS 11).
    // All fields are plain integers/arrays; zero initialization by libproc is valid.
    #[repr(C)]
    struct RegionInfo {
        protection: u32,
        max_protection: u32,
        inheritance: u32,
        flags: u32,
        offset: u64,
        counters: [u32; 14],
        address: u64,
        size: u64,
    }

    #[repr(C)]
    struct RegionPathInfo {
        region: RegionInfo,
        stat: VInfoStat,
        vnode_type: i32,
        vnode_padding: i32,
        fsid: [i32; 2],
        path: [u8; 1024],
    }

    impl PIDInfo for RegionPathInfo {
        fn flavor() -> PidInfoFlavor {
            PidInfoFlavor::RegionPathInfo
        }
    }

    #[inline(never)]
    fn image_anchor() {}

    pub(super) fn running_digest() -> Result<String, Diagnostic> {
        let address = image_anchor as *const () as usize as u64;
        let info: RegionPathInfo =
            pidinfo(std::process::id() as i32, address).map_err(|error| {
                identity_error(format!("Cannot identify running executable vnode: {error}"))
            })?;
        if info.region.address > address
            || address - info.region.address >= info.region.size
            || info.region.protection & 4 == 0
            || info.stat.vst_ino == 0
        {
            return Err(identity_error(
                "Operating system did not identify the running executable region.",
            ));
        }
        let length = info
            .path
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| identity_error("Executable vnode path is not terminated."))?;
        let path = Path::new(OsStr::from_bytes(&info.path[..length]));
        let image = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|error| {
                identity_error(format!("Cannot open running executable vnode: {error}"))
            })?;
        let metadata = image
            .metadata()
            .map_err(|error| identity_error(error.to_string()))?;
        if metadata.dev() != u64::from(info.stat.vst_dev) || metadata.ino() != info.stat.vst_ino {
            return Err(identity_error(
                "Executable pathname no longer names the running image.",
            ));
        }
        let process: BSDInfo = pidinfo(std::process::id() as i32, 0).map_err(|error| {
            identity_error(format!("Cannot identify executable launch time: {error}"))
        })?;
        // Darwin permits opening a running image for writes. Conservatively reject
        // any vnode status change since process creation, including rename/chmod,
        // so post-launch in-place changes cannot be reported as the launched bytes.
        let changed = (metadata.ctime(), metadata.ctime_nsec() / 1000);
        let started = (
            process.pbi_start_tvsec as i64,
            process.pbi_start_tvusec as i64,
        );
        if process.pbi_start_tvsec == 0 || changed > started {
            return Err(identity_error(
                "Running executable vnode changed after process creation.",
            ));
        }
        digest_file(image)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn region_info_matches_the_darwin_64_bit_abi_and_current_image() {
            assert_eq!(std::mem::size_of::<RegionInfo>(), 96);
            assert_eq!(std::mem::size_of::<VInfoStat>(), 136);
            assert_eq!(std::mem::size_of::<RegionPathInfo>(), 1272);
            assert_eq!(
                running_digest().unwrap(),
                crate::support::snapshot::digest_path(&std::env::current_exe().unwrap()).unwrap()
            );
        }
    }
}
