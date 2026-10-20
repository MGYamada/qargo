//! Normal native checking over captured source and explicit edition configuration.

use std::path::Path;

use qleisli::frontend::ast::FnKind;
use qleisli::frontend::project::{ModuleOrigin, Project, SourcePolicy};
use qleisli::interchange::native::Kernel;
use serde_json::{Value, json};

use crate::support::report::{Diagnostic, Location};
use crate::support::snapshot::{
    self, Files, FrozenSources, InputDirectory, SourceStage, portable_relative,
};

/// One anchored source traversal plus the explicit nearest inherited manifest.
/// Neither this capture nor its hashes supplies semantic acceptance.
pub struct CapturedCheckInput {
    sources: FrozenSources,
    configuration: Files,
    directory: InputDirectory,
}

impl CapturedCheckInput {
    pub fn capture(root: &Path) -> Result<Self, Diagnostic> {
        // Select through the existing capability; pathname canonicalization
        // must not choose either the source subject or its inherited config.
        let directory = InputDirectory::open(root)?;
        let captured = directory.collect_source_context()?;
        let mut configuration = Files::new();
        let mut source_files = Files::new();
        for (name, bytes) in captured {
            if crate::support::manifest::is_manifest(Path::new(&name)) {
                configuration.insert(name, bytes);
            } else {
                source_files.insert(name, bytes);
            }
        }
        let sources = FrozenSources::from_files(source_files)?;
        if !configuration.contains_key("Qlirgo.toml") && !configuration.contains_key("Qargo.toml") {
            if let Some(pair) = directory.inherited_source_manifests()? {
                configuration.extend(pair);
            }
        }
        if sources.count() + configuration.len() > 4096
            || sources
                .files()
                .values()
                .chain(configuration.values())
                .map(Vec::len)
                .sum::<usize>()
                > snapshot::TOTAL_BYTES
        {
            return Err(Diagnostic::error(
                "input",
                "qlirgo",
                "Source and raw configuration exceed capture limits.",
            ));
        }
        let configuration = crate::support::manifest::normalize(&configuration)?;
        snapshot::validate_source_configuration(&sources, &configuration)?;
        directory.verify()?;
        Ok(Self {
            sources,
            configuration,
            directory,
        })
    }

    pub fn sources(&self) -> &FrozenSources {
        &self.sources
    }
    pub fn configuration(&self) -> &Files {
        &self.configuration
    }
    pub fn into_parts(self) -> (FrozenSources, Files, InputDirectory) {
        (self.sources, self.configuration, self.directory)
    }
}

pub struct CheckedSources {
    project: Option<Project>,
    source_count: usize,
    module_index: Value,
    configuration_id: String,
    native_summary: Option<Value>,
    _stage: Option<SourceStage>,
}

impl CheckedSources {
    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }
    pub fn source_count(&self) -> usize {
        self.source_count
    }
    pub fn qleisli_check(&self) -> Value {
        if self.project.is_some() {
            json!({"status":"passed", "reason":null})
        } else {
            json!({"status":"not_run", "reason":"no_sources"})
        }
    }
    pub fn module_index(&self) -> &Value {
        &self.module_index
    }
    pub fn configuration_id(&self) -> &str {
        &self.configuration_id
    }
    /// Actual successful native observations, not proof or binary attestation.
    pub fn native_summary(&self) -> Option<&Value> {
        self.native_summary.as_ref()
    }
}

pub fn configuration_id(configuration: &Files) -> String {
    snapshot::digest_files("qlirgo.source-configuration.v1", configuration)
}

/// Select one native kernel for a nonempty input; no selection or native call for empty inputs.
pub fn check(sources: &FrozenSources, configuration: &Files) -> Result<CheckedSources, Diagnostic> {
    check_selected(sources, configuration, None)
}

/// Use one caller-selected kernel for every concrete root and contract decision.
pub fn check_with_kernel(
    sources: &FrozenSources,
    configuration: &Files,
    kernel: &Kernel,
) -> Result<CheckedSources, Diagnostic> {
    check_selected(sources, configuration, Some(kernel))
}

fn check_selected(
    sources: &FrozenSources,
    configuration: &Files,
    selected: Option<&Kernel>,
) -> Result<CheckedSources, Diagnostic> {
    snapshot::validate_source_configuration(sources, configuration)?;
    let configuration_id = configuration_id(configuration);
    if sources.count() == 0 {
        if !configuration.is_empty() {
            let stage = sources.stage_for_beta(configuration)?;
            Project::load_with_policy(stage.root(), SourcePolicy::default())
                .map_err(|diagnostic| compiler_diagnostic(diagnostic, stage.root()))?;
        }
        return Ok(CheckedSources {
            project: None,
            source_count: 0,
            module_index: json!([]),
            configuration_id,
            native_summary: None,
            _stage: None,
        });
    }
    let stage = sources.stage_for_beta(configuration)?;
    // Capture one project and its edition before native selection.
    // Checking and metadata use these retained source and manifest bytes.
    let project = Project::load_with_policy(stage.root(), SourcePolicy::default())
        .map_err(|diagnostic| compiler_diagnostic(diagnostic, stage.root()))?;
    let owned;
    let kernel = if let Some(kernel) = selected {
        kernel
    } else {
        owned = Kernel::selected(project.edition()).map_err(|error| {
            Diagnostic::error(
                "project",
                "compiler",
                format!("Native checker selection failed: {error}"),
            )
        })?;
        &owned
    };
    let report = project
        .check_report_with_kernel(kernel)
        .map_err(|diagnostic| compiler_diagnostic(diagnostic, stage.root()))?;
    let mut modules = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for module in project
        .modules
        .values()
        .filter(|module| module.origin == ModuleOrigin::Local)
    {
        let path = portable_relative(module.path.strip_prefix(stage.root()).map_err(|_| {
            Diagnostic::error(
                "project",
                "compiler",
                "Compiler source escaped its snapshot.",
            )
        })?)?;
        if sources.files().get(&path).map(Vec::as_slice) != Some(module.source.as_bytes())
            || !seen.insert(path.clone())
        {
            return Err(Diagnostic::error(
                "snapshot_mismatch",
                "tool",
                "Checked module bytes differ from captured source bytes.",
            ));
        }
        let mut declarations: Vec<_> = module
            .ast
            .decls
            .iter()
            .filter(|decl| decl.public)
            .map(|decl| json!({"name":decl.name.text,"kind":kind_name(decl.kind)}))
            .collect();
        declarations.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
        if !declarations.is_empty() {
            modules.push(json!({"name":module.name,"path":path,"declarations":declarations}));
        }
    }
    if seen.len() != sources.count() {
        return Err(Diagnostic::error(
            "snapshot_mismatch",
            "tool",
            "Checked module inventory differs from the captured source inventory.",
        ));
    }
    let native = report.native_decisions();
    let paths: Result<Vec<_>, Diagnostic> = native
        .checker_paths()
        .map(|path| {
            path.to_str().map(str::to_owned).ok_or_else(|| {
                Diagnostic::error("tool_identity", "tool", "Native checker path is not UTF-8.")
            })
        })
        .collect();
    let native_summary = json!({"checker_identity":"unpinned","checker_paths":paths?,"count":native.decisions(),"requests_checked":native.requests_checked(),"complete":native.complete(),"entry_request_checked":report.entry_request_checked(),"concrete_ir_roots":{"local":report.concrete_local_roots(),"bundled":report.concrete_bundled_roots()},"source_preservation_verified":false});
    Ok(CheckedSources {
        project: Some(project),
        source_count: sources.count(),
        module_index: json!(modules),
        configuration_id,
        native_summary: Some(native_summary),
        _stage: Some(stage),
    })
}

fn kind_name(kind: FnKind) -> &'static str {
    match kind {
        FnKind::Static => "static",
        FnKind::Meaning => "meaning",
        FnKind::Classical => "classical",
        FnKind::Inferred => "inferred",
        FnKind::Isometry => "isometry",
        FnKind::Unitary => "unitary",
        FnKind::Observe => "observe",
    }
}

fn compiler_diagnostic(
    source: qleisli::frontend::diagnostic::Diagnostic,
    root: &Path,
) -> Diagnostic {
    let Some(stage) = root.to_str() else {
        return Diagnostic::error("input", "tool", "The source snapshot path is not UTF-8.");
    };
    let manifest_diagnostic = source.primary.as_ref().is_some_and(|location| {
        location
            .path
            .strip_prefix(root)
            .is_ok_and(crate::support::manifest::is_manifest)
    });
    let primary = source.primary.and_then(|location| {
        let path = if let Ok(relative) = location.path.strip_prefix(root) {
            if crate::support::manifest::is_manifest(relative) {
                return None;
            }
            portable_relative(relative).ok()?
        } else if let Ok(relative) = location.path.strip_prefix("<bundled>/std") {
            format!("std://{}", portable_relative(relative).ok()?)
        } else {
            return None;
        };
        Some(Box::new(Location {
            path,
            start: location.span.start,
            end: location.span.end,
            line: location.line,
            column: location.column,
        }))
    });
    Diagnostic {
        id: source.code.into(),
        category: "compiler".into(),
        severity: "error".into(),
        primary,
        // Some load diagnostics carry their filesystem path in prose. Remove
        // the private snapshot prefix without altering coordinates or categories.
        message: if manifest_diagnostic {
            redact_stage_path(&source.message, stage).replace("Qargo.toml", "Qlirgo.toml")
        } else {
            redact_stage_path(&source.message, stage)
        },
        suggestion: None,
    }
}

// Only redact a complete filesystem-path component. User-authored words which
// happen to contain the same bytes must retain their original diagnostic text.
fn redact_stage_path(message: &str, stage: &str) -> String {
    if stage.is_empty() {
        return message.to_owned();
    }
    let mut redacted = String::new();
    let mut copied = 0;
    for (start, _) in message.match_indices(stage) {
        let end = start + stage.len();
        let left = message[..start].chars().next_back();
        let right = message[end..].chars().next();
        let starts_path = left.is_none_or(|ch| {
            ch.is_whitespace() || matches!(ch, '\'' | '"' | '`' | '(' | '[' | '{' | '=' | ':')
        });
        let ends_root = right.is_none_or(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '/' | '\\' | '\'' | '"' | '`' | ')' | ']' | '}' | ':' | ',' | ';'
                )
        });
        if starts_path && ends_root {
            redacted.push_str(&message[copied..start]);
            redacted.push_str("<source>");
            copied = end;
        }
    }
    redacted.push_str(&message[copied..]);
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MANIFEST: &str =
        "schema-version=2\n[qrate]\nname=\"example\"\nversion=\"7.4.0\"\nedition=\"2026\"\n";
    const SOURCE: &str = "pub unitary fn identity(q:Q<Bit>)->Q<Bit>{q}";

    #[test]
    fn capture_and_stage_retain_actual_inherited_and_nested_configuration() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("src");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(home.path().join("Qlirgo.toml"), MANIFEST).unwrap();
        fs::write(root.join("main.qli"), SOURCE).unwrap();
        fs::write(root.join("nested/other.qli"), SOURCE).unwrap();
        let unsupported = MANIFEST.replace("2026", "2027");
        fs::write(root.join("nested/Qlirgo.toml"), &unsupported).unwrap();
        let input = CapturedCheckInput::capture(&root).unwrap();
        assert_eq!(input.sources().count(), 2);
        assert_eq!(input.configuration()["Qlirgo.toml"], MANIFEST.as_bytes());
        assert_eq!(
            input.configuration()["nested/Qlirgo.toml"],
            unsupported.as_bytes()
        );
        let source_id = input.sources().source_id().to_owned();
        let configuration_id = configuration_id(input.configuration());
        fs::write(root.join("nested/Qlirgo.toml"), MANIFEST).unwrap();
        let staged = input
            .sources()
            .stage_with_configuration(input.configuration())
            .unwrap();
        assert_eq!(
            fs::read(staged.root().join("nested/Qlirgo.toml")).unwrap(),
            unsupported.as_bytes()
        );
        assert_eq!(input.sources().source_id(), source_id);
        let current = CapturedCheckInput::capture(&root).unwrap();
        assert_eq!(current.sources().source_id(), source_id);
        assert_ne!(
            super::configuration_id(current.configuration()),
            configuration_id
        );
        // Loading the captured invalid manifest fails before kernel selection.
        let diagnostic = check(input.sources(), input.configuration()).err().unwrap();
        assert_eq!(diagnostic.category, "compiler");
        assert!(diagnostic.primary.is_none());
        assert!(diagnostic.message.contains("2027"));
        assert!(!diagnostic.message.contains(staged.root().to_str().unwrap()));
    }

    #[test]
    fn explicit_configuration_is_required_but_empty_sources_need_no_kernel() {
        let sources = FrozenSources::from_files(Files::from([(
            "main.qli".to_owned(),
            SOURCE.as_bytes().to_vec(),
        )]))
        .unwrap();
        assert!(check(&sources, &Files::new()).is_err());
        let empty = FrozenSources::from_files(Files::new()).unwrap();
        let absent = Kernel::new(
            "/nonexistent/qlirgo-no-native-checker",
            qleisli::edition::Edition::E2026,
        );
        let checked = check_with_kernel(&empty, &Files::new(), &absent).unwrap();
        assert_eq!(
            checked.qleisli_check(),
            json!({"status":"not_run","reason":"no_sources"})
        );
        assert_eq!(checked.source_count(), 0);
        assert!(checked.project().is_none());
        assert!(checked.native_summary().is_none());
        let nested_only = Files::from([(
            "nested/Qlirgo.toml".to_owned(),
            MANIFEST.as_bytes().to_vec(),
        )]);
        let diagnostic = check_with_kernel(&empty, &nested_only, &absent)
            .err()
            .unwrap();
        assert_eq!(diagnostic.id, "project");
        assert!(diagnostic.message.contains("root Qlirgo.toml"));
        let invalid = Files::from([(
            "Qlirgo.toml".to_owned(),
            MANIFEST.replace("2026", "2027").into_bytes(),
        )]);
        let diagnostic = check_with_kernel(&empty, &invalid, &absent).err().unwrap();
        assert_eq!(diagnostic.category, "compiler");
        assert!(diagnostic.primary.is_none());
    }

    #[test]
    fn nearest_invalid_manifest_is_not_replaced_by_a_valid_ancestor() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("inner/src");
        fs::create_dir_all(&root).unwrap();
        fs::write(home.path().join("Qlirgo.toml"), MANIFEST).unwrap();
        let nearest = MANIFEST.replace("2026", "2027");
        fs::write(home.path().join("inner/Qlirgo.toml"), &nearest).unwrap();
        fs::write(root.join("main.qli"), SOURCE).unwrap();
        let input = CapturedCheckInput::capture(&root).unwrap();
        assert_eq!(input.configuration()["Qlirgo.toml"], nearest.as_bytes());
        let diagnostic = check(input.sources(), input.configuration()).err().unwrap();
        assert_eq!(diagnostic.category, "compiler");
        assert!(diagnostic.message.contains("2027"));
        assert!(diagnostic.primary.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn context_capture_keeps_the_existing_root_symlink_refusal() {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("Qlirgo.toml"), MANIFEST).unwrap();
        fs::write(target.join("main.qli"), SOURCE).unwrap();
        let alias = home.path().join("alias");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert!(CapturedCheckInput::capture(&alias).is_err());
        assert!(CapturedCheckInput::capture(&target).is_ok());
    }

    #[test]
    fn a_manifest_directory_cannot_be_hidden_by_inherited_configuration() {
        for invalid in ["Qlirgo.toml", "nested/Qlirgo.toml"] {
            let home = tempfile::tempdir().unwrap();
            let root = home.path().join("src");
            fs::create_dir_all(root.join(invalid)).unwrap();
            fs::write(home.path().join("Qlirgo.toml"), MANIFEST).unwrap();
            fs::write(root.join("main.qli"), SOURCE).unwrap();
            let diagnostic = CapturedCheckInput::capture(&root).err().unwrap();
            assert_eq!(diagnostic.category, "compiler");
            assert!(diagnostic.message.contains("regular"));
        }
    }

    #[test]
    fn stage_redaction_preserves_user_substrings_and_relative_coordinates() {
        let stage = "/private/tmp/qlirgo-stage";
        let message = "read '/private/tmp/qlirgo-stage/main.qli', root=/private/tmp/qlirgo-stage; keep /private/tmp/qlirgo-stage-other and prefix/private/tmp/qlirgo-stage/main.qli";
        assert_eq!(
            redact_stage_path(message, stage),
            "read '<source>/main.qli', root=<source>; keep /private/tmp/qlirgo-stage-other and prefix/private/tmp/qlirgo-stage/main.qli"
        );
        assert_eq!(redact_stage_path("plain text", stage), "plain text");
    }
}
