//! Bridge to canonical compiler, runtime, module, and pattern data.

use std::collections::BTreeMap;

use weave_compiler::{CompileOptions, compile_with_modules};
use weave_core::ir::StoryIr;
use weave_core::{Diagnostic, Document, parse_document};
use weave_domain::{
    DomainCatalog, DomainValue, ExportSource, ResolvedDomainModule, TypeExpression,
};
use weave_patterns::{
    PatternDefinition, elder_futhark_definition, i_ching_definition, tarot_definition,
};
use weave_runtime::Story;

/// Failure while preparing editor-owned canonical domain state.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// Current source does not compile.
    #[error("source has {count} compiler diagnostic(s)")]
    Compile {
        /// Number of diagnostics retained for inline display.
        count: usize,
    },
    /// Compiled data could not initialize a preview runtime.
    #[error("runtime initialization failed: {0}")]
    Runtime(#[from] weave_runtime::RuntimeError),
}

/// Editor-facing schema and selected value for one module export.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleExportInspection {
    /// Source-visible export name.
    pub name: String,
    /// Closed portable value type.
    pub value_type: TypeExpression,
    /// Runtime ownership boundary.
    pub source: ExportSource,
    /// Author-facing purpose from the manifest.
    pub description: String,
    /// Selected initial value, absent only for an optional export omitted by the pack.
    pub value: Option<DomainValue>,
}

/// Editor-facing view of one exact source activation.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleInspection {
    /// Story-local source alias.
    pub alias: String,
    /// Stable module identity.
    pub id: String,
    /// Exact module release.
    pub version: String,
    /// Exact selected pack identity.
    pub pack_id: String,
    /// Exact selected pack release.
    pub pack_version: String,
    /// Sorted schema and value records.
    pub exports: Vec<ModuleExportInspection>,
}

/// Canonical source, compiled IR, diagnostics, and preview runtime for one editor session.
#[derive(Debug)]
pub struct DomainSession {
    source: String,
    source_name: Option<String>,
    seed: u64,
    diagnostics: Vec<Diagnostic>,
    document: Option<Document>,
    last_valid_story: Option<StoryIr>,
    domain_catalog: DomainCatalog,
    active_modules: BTreeMap<String, ResolvedDomainModule>,
    runtime: Option<Story>,
}

impl DomainSession {
    /// Create an empty session with deterministic preview entropy.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            source: String::new(),
            source_name: None,
            seed,
            diagnostics: Vec::new(),
            document: None,
            last_valid_story: None,
            domain_catalog: DomainCatalog::new(),
            active_modules: BTreeMap::new(),
            runtime: None,
        }
    }

    /// Create a session with an explicit set of available domain artifacts.
    #[must_use]
    pub fn with_domain_catalog(seed: u64, domain_catalog: DomainCatalog) -> Self {
        Self {
            domain_catalog,
            ..Self::new(seed)
        }
    }

    /// Replace the explicit artifact catalog used by subsequent compiles.
    pub fn set_domain_catalog(&mut self, domain_catalog: DomainCatalog) {
        self.domain_catalog = domain_catalog;
    }

    /// Compile source through `weave-compiler` and initialize `weave-runtime` on success.
    ///
    /// A failed compile replaces diagnostics but deliberately preserves the last valid IR and
    /// preview runtime.
    pub fn compile_source(
        &mut self,
        source: impl Into<String>,
        source_name: Option<String>,
    ) -> Result<(), DomainError> {
        self.source = source.into();
        self.source_name = source_name;
        let parsed = parse_document(&self.source);
        let compiled = match compile_with_modules(
            &self.source,
            &CompileOptions {
                source_name: self.source_name.clone(),
            },
            &self.domain_catalog,
        ) {
            Ok(compiled) => compiled,
            Err(error) => {
                self.diagnostics = error.diagnostics;
                self.document = parsed.ok();
                return Err(DomainError::Compile {
                    count: self.diagnostics.len(),
                });
            }
        };
        let runtime = Story::with_seed(compiled.story.clone(), self.seed)?;
        self.diagnostics = compiled.diagnostics;
        self.document = parsed.ok();
        self.active_modules = compiled.domain_modules;
        self.last_valid_story = Some(compiled.story);
        self.runtime = Some(runtime);
        Ok(())
    }

    /// Current source, including invalid in-progress edits.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Compiler diagnostics for the current source.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Current syntax tree when the in-progress source is parseable.
    #[must_use]
    pub const fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// Most recent valid compiled story.
    #[must_use]
    pub const fn last_valid_story(&self) -> Option<&StoryIr> {
        self.last_valid_story.as_ref()
    }

    /// Exact validated module artifacts selected by the last successful compile.
    #[must_use]
    pub const fn active_modules(&self) -> &BTreeMap<String, ResolvedDomainModule> {
        &self.active_modules
    }

    /// Schema-and-value records suitable for an editor module inspector.
    #[must_use]
    pub fn module_inspections(&self) -> Vec<ModuleInspection> {
        self.active_modules
            .iter()
            .map(|(alias, resolved)| ModuleInspection {
                alias: alias.clone(),
                id: resolved.manifest.id.clone(),
                version: resolved.manifest.version.clone(),
                pack_id: resolved.pack.id.clone(),
                pack_version: resolved.pack.version.clone(),
                exports: resolved
                    .manifest
                    .exports
                    .iter()
                    .map(|(name, declaration)| ModuleExportInspection {
                        name: name.clone(),
                        value_type: declaration.value_type.clone(),
                        source: declaration.source,
                        description: declaration.description.clone(),
                        value: resolved.pack.values.get(name).cloned(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Preview runtime created from the most recent valid source.
    #[must_use]
    pub const fn runtime(&self) -> Option<&Story> {
        self.runtime.as_ref()
    }

    /// Mutable preview runtime for play controls.
    #[must_use]
    pub const fn runtime_mut(&mut self) -> Option<&mut Story> {
        self.runtime.as_mut()
    }
}

/// Built-in definitions exposed to the editor without duplicating pattern records.
#[must_use]
pub fn builtin_pattern_definitions() -> Vec<PatternDefinition> {
    vec![
        tarot_definition(),
        i_ching_definition(),
        elder_futhark_definition(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_editor::TextBuffer;

    const VALID_SOURCE: &str = "=== start ===\nHello.\n-> END\n";

    fn tracer_catalog() -> DomainCatalog {
        let manifest = weave_domain::ModuleManifest::from_json(include_str!(
            "../../examples/domain-modules/contract/module.weave-module.json"
        ))
        .expect("canonical manifest");
        let pack = weave_domain::DomainPack::from_json(include_str!(
            "../../examples/domain-modules/contract/pack.weave-domain.json"
        ))
        .expect("canonical pack");
        DomainCatalog::from_artifacts([manifest], [pack]).expect("catalog")
    }

    #[test]
    fn compiler_runtime_and_pattern_models_are_shared_directly() {
        let mut session = DomainSession::new(17);
        session
            .compile_source(VALID_SOURCE, Some("story.weave".to_owned()))
            .expect("valid source initializes a preview");
        assert!(session.last_valid_story().is_some());
        assert!(session.runtime().is_some());
        assert!(session.diagnostics().is_empty());
        assert_eq!(builtin_pattern_definitions().len(), 3);
    }

    #[test]
    fn invalid_edits_keep_the_last_valid_preview() {
        let mut session = DomainSession::new(5);
        session
            .compile_source(VALID_SOURCE, None)
            .expect("valid source compiles");
        let valid_entry = session
            .last_valid_story()
            .expect("valid story")
            .entry
            .clone();
        let error = session
            .compile_source("=== broken", None)
            .expect_err("invalid source fails");
        assert!(matches!(error, DomainError::Compile { .. }));
        assert!(!session.diagnostics().is_empty());
        assert_eq!(
            session
                .last_valid_story()
                .expect("last valid remains")
                .entry,
            valid_entry
        );
        assert!(session.runtime().is_some());
    }

    #[test]
    fn editor_and_text_buffer_share_module_schema_values_and_compiler_semantics() {
        let source = include_str!("../../examples/domain-modules/contract/tracer.weave");
        let catalog = tracer_catalog();
        let mut session = DomainSession::with_domain_catalog(7, catalog.clone());
        session
            .compile_source(source, Some("tracer.weave".to_owned()))
            .expect("editor compiles tracer");
        let inspections = session.module_inspections();
        let inspection = &inspections[0];
        assert_eq!(inspection.alias, "constellation");
        assert_eq!(inspection.id, "org.weave.synthetic_constellation");
        let phase = inspection
            .exports
            .iter()
            .find(|export| export.name == "phase")
            .expect("phase schema is offered");
        assert_eq!(
            phase.value,
            Some(DomainValue::Symbol("twilight".to_owned()))
        );

        let mut buffer = TextBuffer::with_domain_catalog(source, catalog);
        assert!(buffer.diagnostics().is_empty());
        buffer.format().expect("module source formats");
        assert!(buffer.source().contains("module constellation"));
        assert!(buffer.diagnostics().is_empty());
        assert_eq!(
            session.last_valid_story().expect("compiled story").modules,
            weave_compiler::compile_with_modules(
                buffer.source(),
                &CompileOptions {
                    source_name: Some("tracer.weave".to_owned()),
                },
                &tracer_catalog(),
            )
            .expect("formatted text compiles")
            .story
            .modules
        );
    }
}
