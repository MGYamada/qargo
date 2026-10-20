//! Actual linked standard-qrate identity; metadata never supplies acceptance.

use std::collections::BTreeMap;

use qleisli::frontend::project::bundled_std;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::report::Diagnostic;
use crate::snapshot::digest_files;

const STD_DOMAIN: &str = "qargo.bundled-std.v1";

/// Privately captured facts from the linked library's existing manifest gate.
#[derive(Clone)]
pub(super) struct EffectiveStd {
    value: Value,
}

impl EffectiveStd {
    pub(super) fn capture(tool: &Value) -> Result<Self, Diagnostic> {
        let bundle = bundled_std()
            .map_err(|failure| Diagnostic::error("std_identity", "tool", failure.message))?;
        if tool["qleisli_version"].as_str() != Some(bundle.product_version()) {
            return Err(Diagnostic::error(
                "tool_identity",
                "tool",
                format!(
                    "Declared linked Qleisli version {:?} differs from actual {:?}.",
                    tool["qleisli_version"].as_str(),
                    bundle.product_version()
                ),
            ));
        }
        let mut modules = BTreeMap::new();
        for (name, source) in bundle.modules() {
            if modules
                .insert(name.to_owned(), source.as_bytes().to_vec())
                .is_some()
            {
                return Err(Diagnostic::error(
                    "std_identity",
                    "tool",
                    "The linked standard qrate has duplicate canonical module names.",
                ));
            }
        }
        let mut constituents = BTreeMap::from([(
            "manifest:std".to_owned(),
            bundle.manifest().as_bytes().to_vec(),
        )]);
        for (name, bytes) in &modules {
            constituents.insert(format!("module:{name}"), bytes.clone());
        }
        let module_inventory: Vec<_> = modules
            .iter()
            .map(|(name, bytes)| json!({"name":name,"sha256":digest(bytes)}))
            .collect();
        Ok(Self {
            value: json!({
                "qleisli_version":bundle.product_version(),
                "std":{
                    "name":bundle.name(),"version":bundle.version(),"edition":bundle.edition(),
                    "manifest_sha256":digest(bundle.manifest().as_bytes()),
                    "modules":module_inventory,
                    "content_id":digest_files(STD_DOMAIN, &constituents)
                }
            }),
        })
    }

    pub(super) fn value(&self) -> &Value {
        &self.value
    }

    pub(super) fn require_tool(&self, tool: &Value) -> Result<(), Diagnostic> {
        if tool["qleisli_version"] != self.value["qleisli_version"] {
            return Err(Diagnostic::error(
                "tool_identity",
                "tool",
                "The build tool declaration differs from the captured linked Qleisli version.",
            ));
        }
        Ok(())
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_std_binds_actual_complete_bytes_and_rejects_stale_declarations() {
        let bundle = bundled_std().unwrap();
        let tool = json!({"qleisli_version":bundle.product_version()});
        let effective = EffectiveStd::capture(&tool).unwrap();
        assert_eq!(
            effective.value()["qleisli_version"],
            bundle.product_version()
        );
        assert_eq!(effective.value()["std"]["version"], bundle.version());
        let mut constituents = BTreeMap::from([(
            "manifest:std".to_owned(),
            bundle.manifest().as_bytes().to_vec(),
        )]);
        let mut names = Vec::new();
        for (name, text) in bundle.modules() {
            constituents.insert(format!("module:{name}"), text.as_bytes().to_vec());
            names.push(name);
        }
        names.sort_unstable();
        let actual: Vec<_> = effective.value()["std"]["modules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect();
        assert_eq!(actual, names);
        let expected = digest_files(STD_DOMAIN, &constituents);
        assert_eq!(effective.value()["std"]["content_id"], expected);
        let mut changed = constituents.clone();
        changed.get_mut("manifest:std").unwrap().push(b' ');
        assert_ne!(digest_files(STD_DOMAIN, &changed), expected);
        let module = format!("module:{}", names[0]);
        let mut changed = constituents.clone();
        changed.get_mut(&module).unwrap().push(b' ');
        assert_ne!(digest_files(STD_DOMAIN, &changed), expected);
        let mut changed = constituents.clone();
        changed.remove(&module);
        assert_ne!(digest_files(STD_DOMAIN, &changed), expected);
        let mut changed = constituents;
        changed.insert("module:std::unregistered".to_owned(), b"".to_vec());
        assert_ne!(digest_files(STD_DOMAIN, &changed), expected);
        let stale = json!({"qleisli_version":"0.2.1"});
        assert_eq!(
            EffectiveStd::capture(&stale).err().unwrap().id,
            "tool_identity"
        );
    }
}
