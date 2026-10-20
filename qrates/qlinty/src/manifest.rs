//! Transitional manifest names and byte-exact configuration selection.

use std::path::Path;

use crate::support::report::Diagnostic;
use crate::support::snapshot::{Files, portable_relative};

pub const CANONICAL: &str = "Qlirgo.toml";
pub const LEGACY: &str = "Qargo.toml";
pub const NAMES: [&str; 2] = [CANONICAL, LEGACY];

pub fn is_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| NAMES.contains(&name))
}

/// Select only after validating every present name; absence is not a parse fallback.
pub fn selected_name(files: &Files) -> Result<Option<&'static str>, Diagnostic> {
    if files
        .get(CANONICAL)
        .zip(files.get(LEGACY))
        .is_some_and(|(a, b)| a != b)
    {
        return Err(Diagnostic::error(
            "manifest_conflict",
            "qlirgo",
            "Qlirgo.toml and Qargo.toml must contain exactly identical bytes when both exist.",
        ));
    }
    for name in NAMES {
        if let Some(bytes) = files.get(name) {
            if bytes.len() > 65_536 || std::str::from_utf8(bytes).is_err() {
                return Err(Diagnostic::error(
                    "invalid_manifest",
                    "qlirgo",
                    format!("{name} must be UTF-8 and at most 65536 bytes."),
                ));
            }
        }
    }
    match (files.get(CANONICAL), files.get(LEGACY)) {
        (Some(_), _) => Ok(Some(CANONICAL)),
        (_, Some(_)) => Ok(Some(LEGACY)),
        _ => Ok(None),
    }
}

/// Normalize effective configuration only; original qrate inputs retain real labels.
pub fn normalize(files: &Files) -> Result<Files, Diagnostic> {
    let mut groups = std::collections::BTreeMap::<String, Files>::new();
    for (label, bytes) in files {
        let path = Path::new(label);
        if portable_relative(path)? != *label || !is_manifest(path) {
            return Err(Diagnostic::error(
                "input",
                "qlirgo",
                "Expected a captured manifest path.",
            ));
        }
        let parent = path.parent().unwrap_or(Path::new(""));
        groups
            .entry(parent.to_string_lossy().into_owned())
            .or_default()
            .insert(
                path.file_name().unwrap().to_str().unwrap().into(),
                bytes.clone(),
            );
    }
    let mut result = Files::new();
    for (parent, pair) in groups {
        if let Some(name) = selected_name(&pair)? {
            let label = if parent.is_empty() {
                CANONICAL.into()
            } else {
                format!("{parent}/{CANONICAL}")
            };
            result.insert(label, pair[name].clone());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_differing_pair_is_a_conflict_before_manifest_decoding() {
        let files = Files::from([
            (CANONICAL.into(), vec![255]),
            (LEGACY.into(), b"valid UTF-8".to_vec()),
        ]);
        assert_eq!(selected_name(&files).unwrap_err().id, "manifest_conflict");
        let same_invalid = Files::from([(CANONICAL.into(), vec![255]), (LEGACY.into(), vec![255])]);
        assert_eq!(
            selected_name(&same_invalid).unwrap_err().id,
            "invalid_manifest"
        );
    }
}
