//! Validate closed child-tool responses against frozen sources and tool identities.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::report::{Diagnostic, Envelope};
use crate::snapshot::{Files, FrozenSources};
use crate::{PROFILE, QLEISLI_VERSION, VERSION};

pub(crate) const ENVELOPE_FIELDS: &[&str] = &[
    "format",
    "version",
    "command",
    "outcome",
    "diagnostics",
    "result",
];
pub(crate) const DIAGNOSTIC_FIELDS: &[&str] = &[
    "id",
    "category",
    "severity",
    "primary",
    "message",
    "suggestion",
];

// Typed decoding rejects repeated fields and invalid types. Raw-object checks
// also require explicit nullable fields, which serde otherwise permits missing.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Response<R> {
    format: String,
    version: u32,
    command: String,
    outcome: String,
    diagnostics: Vec<Diagnostic>,
    result: R,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Step {
    status: String,
    reason: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tool {
    name: String,
    version: String,
    executable_sha256: String,
    qleisli_version: String,
    profile: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LintResult {
    source_count: usize,
    source_id: String,
    qleisli_check: Step,
    tool: Tool,
}

pub(crate) fn closed_object(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field))
    })
}

pub(crate) fn tool_matches(tool: &Value, name: &str, digest: &str) -> bool {
    closed_object(
        tool,
        &[
            "name",
            "version",
            "executable_sha256",
            "qleisli_version",
            "profile",
        ],
    ) && tool["name"] == name
        && tool["version"] == VERSION
        && tool["qleisli_version"] == QLEISLI_VERSION
        && tool["profile"] == PROFILE
        && tool["executable_sha256"] == digest
}

fn step_valid(step: &Value, source_count: usize) -> bool {
    closed_object(step, &["status", "reason"])
        && if source_count == 0 {
            step["status"] == "not_run" && step["reason"] == "no_sources"
        } else {
            (step["status"] == "passed" && step["reason"].is_null())
                || (step["status"] == "failed" && step["reason"] == "compiler_error")
        }
}

pub(crate) fn transport(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("invalid_tool_response", "tool", message)
}

fn bundled_sources() -> Option<&'static Files> {
    static BUNDLED: std::sync::OnceLock<Option<Files>> = std::sync::OnceLock::new();
    BUNDLED
        .get_or_init(|| {
            let empty = tempfile::tempdir().ok()?;
            let project = qleisli::frontend::project::Project::load_with_policy(
                empty.path(),
                qleisli::frontend::project::SourcePolicy::default(),
            )
            .ok()?;
            project
                .modules
                .values()
                .filter(|module| module.origin == qleisli::frontend::project::ModuleOrigin::Bundled)
                .map(|module| {
                    let relative = module.path.strip_prefix("<bundled>/std").ok()?.to_str()?;
                    Some((
                        format!("std://{}", relative.replace('\\', "/")),
                        module.source.as_bytes().to_vec(),
                    ))
                })
                .collect::<Option<Files>>()
        })
        .as_ref()
}

pub(crate) fn location_valid(location: &Value, sources: &FrozenSources, compiler: bool) -> bool {
    if location.is_null() {
        return compiler;
    }
    if !closed_object(location, &["path", "start", "end", "line", "column"]) {
        return false;
    }
    let Some(path) = location["path"].as_str() else {
        return false;
    };
    let Some(start) = location["start"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    let Some(end) = location["end"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    let Some(line) = location["line"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    let Some(column) = location["column"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
    else {
        return false;
    };
    if start > end || line == 0 || column == 0 {
        return false;
    }
    let bytes = if compiler && path.starts_with("std://") {
        bundled_sources().and_then(|sources| sources.get(path))
    } else {
        sources.files.get(path)
    };
    let Some(bytes) = bytes else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    if end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return false;
    }
    let (expected_line, expected_column) = crate::report::coordinates(text, start);
    line == expected_line && column == expected_column
}

pub(crate) fn lint_response(
    bytes: &[u8],
    status: i32,
    sources: &FrozenSources,
    digest: &str,
    deny: bool,
) -> Result<Envelope, Diagnostic> {
    let _: Response<Option<LintResult>> = serde_json::from_slice(bytes).map_err(|_| {
        transport("qlippy returned repeated fields, unsupported fields or invalid field types.")
    })?;
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| transport("qlippy did not emit a complete JSON response."))?;
    if !closed_object(&value, ENVELOPE_FIELDS)
        || value["format"] != "qlippy.result"
        || value["version"] != 1
        || value["command"] != "lint"
        || !["ok", "error"].contains(&value["outcome"].as_str().unwrap_or(""))
    {
        return Err(transport(
            "qlippy returned an unsupported or malformed envelope.",
        ));
    }
    let Some(diagnostics) = value["diagnostics"].as_array() else {
        return Err(transport("qlippy diagnostics must be an array."));
    };
    for diagnostic in diagnostics {
        if !closed_object(diagnostic, DIAGNOSTIC_FIELDS)
            || !diagnostic["message"].is_string()
            || !(diagnostic["suggestion"].is_null() || diagnostic["suggestion"].is_string())
        {
            return Err(transport("qlippy returned a malformed diagnostic."));
        }
        let category = diagnostic["category"].as_str().unwrap_or("");
        let id = diagnostic["id"].as_str().unwrap_or("");
        let severity = diagnostic["severity"].as_str().unwrap_or("");
        if category == "lint" {
            if crate::rules::find(id).is_none() || severity != "warning" {
                return Err(transport(
                    "qlippy returned an unknown lint rule or severity.",
                ));
            }
        } else if category != "compiler"
            || ![
                "project",
                "parse",
                "unknown_name",
                "recursive_call",
                "type_mismatch",
                "arity",
                "ownership",
                "effect",
                "invalid_entry",
                "unsupported",
                "limit",
                "invalid_ir",
                "capability",
                "contract",
            ]
            .contains(&id)
            || severity != "error"
        {
            return Err(transport(
                "qlippy returned an unexpected diagnostic category.",
            ));
        }
        if !location_valid(&diagnostic["primary"], sources, category == "compiler") {
            return Err(transport(
                "qlippy diagnostic locations do not match the captured source bytes.",
            ));
        }
    }
    let result = &value["result"];
    if result.is_null() {
        return Err(transport(
            "qlippy omitted the captured source and tool identity binding.",
        ));
    }
    if !closed_object(
        result,
        &["source_count", "source_id", "qleisli_check", "tool"],
    ) || result["source_count"].as_u64() != Some(sources.count() as u64)
        || result["source_id"] != sources.source_id
        || !step_valid(&result["qleisli_check"], sources.count())
    {
        return Err(transport(
            "qlippy results are not bound to the captured sources.",
        ));
    }
    let tool = &result["tool"];
    if !tool_matches(tool, "qlippy", digest) {
        return Err(transport(
            "qlippy tool identity or profile is incompatible.",
        ));
    }
    let errors = diagnostics
        .iter()
        .any(|diagnostic| diagnostic["severity"] == "error");
    let warnings = diagnostics
        .iter()
        .any(|diagnostic| diagnostic["severity"] == "warning");
    let outcome = value["outcome"].as_str().unwrap_or("");
    if (outcome == "ok"
        && (status != 0
            || errors
            || deny && warnings
            || result["qleisli_check"]["status"] == "failed"))
        || (outcome == "error" && (status != 1 || !(errors || deny && warnings)))
        || (errors && result["qleisli_check"]["status"] != "failed")
        || (result["qleisli_check"]["status"] == "failed" && (!errors || warnings))
    {
        return Err(transport(
            "qlippy exit status, outcome and diagnostics are inconsistent.",
        ));
    }
    serde_json::from_value(value)
        .map_err(|_| transport("qlippy returned malformed typed diagnostics."))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    #[test]
    fn failed_compiler_steps_cannot_carry_lint_warnings() {
        let sources = FrozenSources::from_files(BTreeMap::from([(
            "module.qli".into(),
            b"unitary fn identity(q:Q<Bit>)->Q<Bit>{q}".to_vec(),
        )]))
        .unwrap();
        let warning = json!({"id":"unused_import","category":"lint","severity":"warning", "primary":{"path":"module.qli","start":0,"end":1,"line":1,"column":1},"message":"advice","suggestion":null});
        let compiler = json!({"id":"type_mismatch","category":"compiler","severity":"error","primary":null,"message":"rejected","suggestion":null});
        let mut response = json!({"format":"qlippy.result","version":1,"command":"lint","outcome":"error","diagnostics":[warning],
            "result":{"source_count":1,"source_id":sources.source_id,"qleisli_check":{"status":"failed","reason":"compiler_error"},
            "tool":{"name":"qlippy","version":VERSION,"executable_sha256":"digest","qleisli_version":QLEISLI_VERSION,"profile":PROFILE}}});
        assert!(
            lint_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_err()
        );
        response["diagnostics"] = json!([compiler.clone(), warning]);
        assert!(
            lint_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_err()
        );
        response["diagnostics"] = json!([compiler]);
        assert!(
            lint_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_ok()
        );
        for reason in [Value::Null, json!("other"), json!("no_sources"), json!(42)] {
            response["result"]["qleisli_check"]["reason"] = reason;
            assert!(
                lint_response(
                    &serde_json::to_vec(&response).unwrap(),
                    1,
                    &sources,
                    "digest",
                    true
                )
                .is_err()
            );
        }
        response["result"]["qleisli_check"]["reason"] = json!("compiler_error");
        let repeated = serde_json::to_string(&response)
            .unwrap()
            .replace("\"version\":1", "\"version\":1,\"version\":1");
        assert!(lint_response(repeated.as_bytes(), 1, &sources, "digest", true).is_err());
        response["result"] = Value::Null;
        assert!(
            lint_response(
                &serde_json::to_vec(&response).unwrap(),
                1,
                &sources,
                "digest",
                true
            )
            .is_err()
        );
    }

    #[test]
    fn bundled_locations_are_bound_to_actual_linked_source_bytes() {
        let sources = FrozenSources::from_files(Files::new()).unwrap();
        let valid = json!({"path":"std://arithmetic.qli","start":0,"end":1,"line":1,"column":1});
        assert!(location_valid(&valid, &sources, true));
        let mut invalid = valid.clone();
        invalid["end"] = json!(usize::MAX);
        assert!(!location_valid(&invalid, &sources, true));
        let mut invalid = valid.clone();
        invalid["line"] = json!(2);
        assert!(!location_valid(&invalid, &sources, true));
        let mut invalid = valid;
        invalid["path"] = json!("std://missing.qli");
        assert!(!location_valid(&invalid, &sources, true));
    }
}
