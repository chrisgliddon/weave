//! Bridge to canonical compiler, runtime, module, and pattern data.

use std::collections::BTreeMap;

use weave_compiler::{CompileOptions, compile_with_modules};
use weave_core::ir::StoryIr;
use weave_core::{Diagnostic, Document, parse_document};
use weave_domain::{
    DomainCatalog, DomainValue, ExportSource, Provenance, ResolvedDomainModule, TypeExpression,
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
    /// SPDX expression covering the module-authored schema.
    pub license: String,
    /// Public location of the module license text.
    pub license_url: String,
    /// Module schema authorship and source lineage.
    pub manifest_provenance: Provenance,
    /// Selected pack value lineage, including public-source hashes and transformations.
    pub pack_provenance: Provenance,
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
                license: resolved.manifest.license.clone(),
                license_url: resolved.manifest.license_url.clone(),
                manifest_provenance: resolved.manifest.provenance.clone(),
                pack_provenance: resolved.pack.provenance.clone(),
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

    fn world_catalog() -> DomainCatalog {
        let manifest = weave_domain::ModuleManifest::from_json(include_str!(
            "../../examples/domain-modules/weave-world/module.weave-module.json"
        ))
        .expect("canonical world manifest");
        let packs = [
            include_str!("../../examples/domain-modules/weave-world/pack.weave-domain.json"),
            include_str!(
                "../../examples/domain-modules/weave-world/packs/british_columbia_temperate_forest.weave-domain.json"
            ),
            include_str!(
                "../../examples/domain-modules/weave-world/packs/hokkaido_japan.weave-domain.json"
            ),
            include_str!(
                "../../examples/domain-modules/weave-world/packs/maldives.weave-domain.json"
            ),
        ]
        .map(|source| {
            weave_domain::DomainPack::from_json(source).expect("canonical world corpus pack")
        });
        DomainCatalog::from_artifacts([manifest], packs).expect("world catalog")
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

    #[test]
    fn world_preset_values_and_provenance_survive_editor_round_trip() {
        let source =
            include_str!("../../examples/domain-modules/weave-world/reference-place.weave");
        let catalog = world_catalog();
        let mut session = DomainSession::with_domain_catalog(11, catalog.clone());
        session
            .compile_source(source, Some("reference-place.weave".to_owned()))
            .expect("editor selects world preset");

        let inspections = session.module_inspections();
        let world = &inspections[0];
        assert_eq!(world.alias, "world");
        assert_eq!(world.id, "org.weave.world");
        assert_eq!(world.pack_id, "aotearoa_new_zealand");
        assert_eq!(world.license, "MIT");
        assert!(
            world
                .pack_provenance
                .sources
                .iter()
                .any(|source| source.id == "niwa_temperature" && source.sha256.is_some())
        );
        assert!(
            world
                .pack_provenance
                .claims
                .contains_key("values.seed.climate")
        );
        let seed = world
            .exports
            .iter()
            .find(|export| export.name == "seed")
            .and_then(|export| export.value.as_ref())
            .expect("typed world seed is inspectable");
        let DomainValue::Object(seed) = seed else {
            panic!("world seed must remain an object");
        };
        assert_eq!(
            seed.get("primary_biome"),
            Some(&DomainValue::Symbol(
                "temperate_broadleaf_and_mixed_forest".to_owned()
            ))
        );

        let mut buffer = TextBuffer::with_domain_catalog(source, catalog);
        buffer.format().expect("world declaration formats");
        assert!(
            buffer
                .source()
                .contains("pack: \"aotearoa_new_zealand@=1.0.0\"")
        );
        let formatted = weave_compiler::compile_with_modules(
            buffer.source(),
            &CompileOptions {
                source_name: Some("reference-place.weave".to_owned()),
            },
            &world_catalog(),
        )
        .expect("formatted world source compiles");
        assert_eq!(
            session.last_valid_story().expect("world story").modules,
            formatted.story.modules
        );
    }

    #[test]
    fn editor_inspects_four_distinct_world_presets_with_exact_lineage() {
        let fixtures = [
            (
                include_str!(
                    "../../examples/domain-modules/weave-world/corpus/stories/british-columbia-temperate-forest.weave"
                ),
                "british_columbia_temperate_forest",
            ),
            (
                include_str!(
                    "../../examples/domain-modules/weave-world/corpus/stories/hokkaido-japan.weave"
                ),
                "hokkaido_japan",
            ),
            (
                include_str!(
                    "../../examples/domain-modules/weave-world/corpus/stories/maldives.weave"
                ),
                "maldives",
            ),
            (
                include_str!("../../examples/domain-modules/weave-world/reference-place.weave"),
                "aotearoa_new_zealand",
            ),
        ];
        let catalog = world_catalog();
        let mut inspected = Vec::new();
        for (source, expected_pack) in fixtures {
            let mut session = DomainSession::with_domain_catalog(13, catalog.clone());
            session
                .compile_source(source, Some(format!("{expected_pack}.weave")))
                .expect("editor compiles one corpus preset");
            let inspections = session.module_inspections();
            assert_eq!(inspections.len(), 1);
            let inspection = &inspections[0];
            assert_eq!(inspection.pack_id, expected_pack);
            assert!(
                inspection
                    .pack_provenance
                    .claims
                    .contains_key("values.seed.context")
            );
            assert!(
                inspection
                    .pack_provenance
                    .claims
                    .contains_key("values.seed.daylight")
            );
            assert!(
                inspection
                    .pack_provenance
                    .claims
                    .contains_key("values.seed.hazards")
            );
            if expected_pack == "maldives" {
                assert!(inspection.pack_provenance.sources.iter().any(|source| {
                    source.id == "nasa_power_climate"
                        && source.revision.contains("v2.9.7")
                        && source.sha256.is_some()
                }));
            }
            inspected.push(inspection.pack_id.clone());
        }
        assert_eq!(
            inspected,
            vec![
                "british_columbia_temperate_forest",
                "hokkaido_japan",
                "maldives",
                "aotearoa_new_zealand",
            ]
        );
    }
}
