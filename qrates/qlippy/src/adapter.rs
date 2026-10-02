//! Normal Qleisli checking over a private, immutable source snapshot.

use std::path::Path;

use qleisli::frontend::ast::FnKind;
use qleisli::frontend::compile::check_project_with_policy;
use qleisli::frontend::project::{ModuleOrigin, Project, SourcePolicy};
use serde_json::{Value, json};

use crate::report::{Diagnostic, Location};
use crate::snapshot::{FrozenSources, SourceStage, portable_relative};

pub struct CheckedSources {
    project: Option<Project>,
    source_count: usize,
    module_index: Value,
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
}

pub fn check(sources: &FrozenSources) -> Result<CheckedSources, Diagnostic> {
    if sources.count() == 0 {
        return Ok(CheckedSources {
            project: None,
            source_count: 0,
            module_index: json!([]),
            _stage: None,
        });
    }
    let stage = sources.stage()?;
    check_project_with_policy(stage.root(), SourcePolicy::default())
        .map_err(|diagnostic| compiler_diagnostic(diagnostic, stage.root()))?;
    let project = Project::load_with_policy(stage.root(), SourcePolicy::default())
        .map_err(|diagnostic| compiler_diagnostic(diagnostic, stage.root()))?;
    let mut modules = Vec::new();
    for module in project
        .modules
        .values()
        .filter(|module| module.origin == ModuleOrigin::Local)
    {
        let mut declarations: Vec<_> = module
            .ast
            .decls
            .iter()
            .filter(|decl| decl.public)
            .map(|decl| json!({"name":decl.name.text, "kind":kind_name(decl.kind)}))
            .collect();
        declarations.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
        if declarations.is_empty() {
            continue;
        }
        let path = portable_relative(module.path.strip_prefix(stage.root()).map_err(|_| {
            Diagnostic::error(
                "project",
                "compiler",
                "Compiler source escaped its snapshot.",
            )
        })?)?;
        modules.push(json!({"name":module.name,"path":path,"declarations":declarations}));
    }
    Ok(CheckedSources {
        project: Some(project),
        source_count: sources.count(),
        module_index: json!(modules),
        _stage: Some(stage),
    })
}

fn kind_name(kind: FnKind) -> &'static str {
    match kind {
        FnKind::Meaning => "meaning",
        FnKind::Basis => "basis",
        FnKind::Iso => "iso",
        FnKind::Unitary => "unitary",
        FnKind::Observe => "observe",
    }
}

fn compiler_diagnostic(
    source: qleisli::frontend::diagnostic::Diagnostic,
    root: &Path,
) -> Diagnostic {
    let primary = source.primary.and_then(|location| {
        let path = if let Ok(relative) = location.path.strip_prefix(root) {
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
        message: source
            .message
            .replace(&root.to_string_lossy().to_string(), "<source>"),
        suggestion: None,
    }
}
