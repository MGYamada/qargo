//! Resolve exactly one selected auxiliary, without execution fallback.

use crate::report::Diagnostic;
use std::fs;
use std::path::{Path, PathBuf};

fn error(id: &str, category: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(id, category, message)
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

pub(super) fn find_tool(tool_name: &str, explicit: Option<&Path>) -> Result<PathBuf, Diagnostic> {
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
                return fs::canonicalize(&sibling).map_err(|failure| {
                    error(
                        "tool_missing",
                        "tool",
                        format!("Cannot resolve sibling {tool_name}: {failure}"),
                    )
                });
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
