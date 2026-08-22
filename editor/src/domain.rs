//! Bridge to canonical compiler, runtime, module, and pattern data.

use std::collections::{BTreeMap, BTreeSet};

use weave_compiler::{CompileOptions, compile_with_modules};
use weave_core::ast::{Expr, Item, Literal, ModuleEntry, Span, Spanned, UnaryOperator};
use weave_core::ir::StoryIr;
use weave_core::{Diagnostic, Document, parse_document};
use weave_domain::{
    DomainCatalog, DomainValue, ExportSource, Provenance, ReadOnlyPathDeclaration,
    ResolvedDomainModule, TypeExpression,
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
    /// Portable World composition metadata is invalid.
    #[error(transparent)]
    WorldComposition(#[from] weave_world::WorldCompositionError),
    /// A structured editor mutation could not be represented safely.
    #[error("domain edit rejected: {reason}")]
    Edit {
        /// Redaction-safe reason.
        reason: &'static str,
    },
}

/// Editor-visible ownership of one effective domain value path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainValueOrigin {
    /// Unmodified value inherited directly from immutable pack material.
    Inherited,
    /// Deterministically transformed value supplied by the selected pack.
    Generated,
    /// Fictional value explicitly written in `.weave` source.
    Authored,
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
    /// Effective nested paths mapped to inherited, generated, or authored ownership.
    pub origins: BTreeMap<String, DomainValueOrigin>,
}

/// One stable entity in a manifest-declared editor hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityInspection {
    /// Stable identity used by hierarchy and relationship references.
    pub id: String,
    /// Human-editable label; renaming this never changes [`Self::id`].
    pub label: String,
    /// Deterministic sibling order.
    pub order: i64,
    /// Parent stable id, absent for roots.
    pub parent_id: Option<String>,
    /// Symmetric related-entity ids.
    pub related_ids: Vec<String>,
    /// Explicit inheritance-source fields; `None` means the pack/reference fallback.
    pub inheritance: BTreeMap<String, Option<String>>,
    /// Complete effective entity value.
    pub value: DomainValue,
    /// Nested value ownership keyed by path relative to the collection export.
    pub origins: BTreeMap<String, DomainValueOrigin>,
}

/// One generic hierarchy declared by a module manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityCollectionInspection {
    /// Export that owns the stable entity map.
    pub export: String,
    /// Root ids in deterministic order.
    pub roots: Vec<String>,
    /// All entities keyed by stable id.
    pub entities: BTreeMap<String, EntityInspection>,
}

/// Author-supplied fields used when creating one stable entity through the generic editor surface.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityDraft {
    /// Stable lowercase identifier.
    pub id: String,
    /// Initial human-editable label.
    pub label: String,
    /// Initial deterministic sibling order.
    pub order: i64,
    /// Optional parent stable id.
    pub parent_id: Option<String>,
    /// Additional direct fields required by the module schema, such as a place kind.
    pub fields: BTreeMap<String, DomainValue>,
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
    /// Generic place/entity hierarchies declared by the selected manifest.
    pub entity_collections: Vec<EntityCollectionInspection>,
    /// Typed paths that the module exposes for inspection but protects from source writeback.
    pub read_only_paths: Vec<ReadOnlyPathDeclaration>,
    /// Resolved World environment/climate inheritance when this is the Weave World module.
    pub resolved_world: Option<weave_world::ResolvedWorld>,
    /// Selected composition layers and differences when this pack was produced by a World plan.
    pub world_composition: Option<weave_world::WorldCompositionReceipt>,
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
    world_compositions: BTreeMap<(String, String), weave_world::WorldCompositionReceipt>,
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
            world_compositions: BTreeMap::new(),
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

    /// Register one validated World composition receipt for editor inspection.
    pub fn register_world_composition(
        &mut self,
        receipt: weave_world::WorldCompositionReceipt,
    ) -> Result<(), DomainError> {
        receipt.to_json()?;
        self.world_compositions.insert(
            (receipt.output.id.clone(), receipt.output.version.clone()),
            receipt,
        );
        Ok(())
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
                        value: resolved.effective_values.get(name).cloned(),
                        origins: resolved
                            .effective_values
                            .get(name)
                            .map(|value| export_origins(resolved, name, value))
                            .unwrap_or_default(),
                    })
                    .collect(),
                entity_collections: entity_collection_inspections(resolved),
                read_only_paths: resolved.manifest.authoring.read_only_paths.clone(),
                resolved_world: weave_world::resolve_world(resolved).ok(),
                world_composition: self
                    .world_compositions
                    .get(&(resolved.pack.id.clone(), resolved.pack.version.clone()))
                    .cloned(),
            })
            .collect()
    }

    /// Set or replace one compile-time constant module value in canonical source.
    pub fn set_module_override(
        &mut self,
        alias: &str,
        path: &[&str],
        value: DomainValue,
    ) -> Result<(), DomainError> {
        if path.is_empty() {
            return Err(DomainError::Edit {
                reason: "override path must not be empty",
            });
        }
        self.edit_overrides(
            alias,
            [(
                path.iter().map(|segment| (*segment).to_owned()).collect(),
                Some(value),
            )],
        )
    }

    /// Copy one effective pack-supplied leaf into source as an explicit authored override.
    pub fn promote_module_value(&mut self, alias: &str, path: &[&str]) -> Result<(), DomainError> {
        if path.is_empty() {
            return Err(DomainError::Edit {
                reason: "promoted value path must not be empty",
            });
        }
        let owned_path = path
            .iter()
            .map(|segment| (*segment).to_owned())
            .collect::<Vec<_>>();
        let resolved = self.active_modules.get(alias).ok_or(DomainError::Edit {
            reason: "module alias is not active",
        })?;
        if classify_origin(resolved, &owned_path) == DomainValueOrigin::Authored {
            return Err(DomainError::Edit {
                reason: "value is already authored",
            });
        }
        let value = domain_value_at(&resolved.effective_values, &owned_path)
            .cloned()
            .ok_or(DomainError::Edit {
                reason: "effective module value is unavailable",
            })?;
        if matches!(value, DomainValue::Object(_)) {
            return Err(DomainError::Edit {
                reason: "promote one value leaf rather than an object",
            });
        }
        self.set_module_override(alias, path, value)
    }

    /// Remove one source-authored replacement so the selected pack or inheritance chain wins.
    pub fn reset_module_override(&mut self, alias: &str, path: &[&str]) -> Result<(), DomainError> {
        if path.is_empty() {
            return Err(DomainError::Edit {
                reason: "override path must not be empty",
            });
        }
        self.edit_overrides(
            alias,
            [(
                path.iter().map(|segment| (*segment).to_owned()).collect(),
                None,
            )],
        )
    }

    /// Create a stable entity from a manifest-declared collection without changing other ids.
    pub fn create_entity(
        &mut self,
        alias: &str,
        export: &str,
        draft: EntityDraft,
    ) -> Result<(), DomainError> {
        let collection = self.collection_declaration(alias, export)?;
        if !valid_editor_identifier(&draft.id) {
            return Err(DomainError::Edit {
                reason: "entity id must be a stable lowercase identifier",
            });
        }
        if self.entity(alias, export, &draft.id).is_some() {
            return Err(DomainError::Edit {
                reason: "entity id already exists",
            });
        }
        if let Some(parent) = draft.parent_id.as_deref()
            && self.entity(alias, export, parent).is_none()
        {
            return Err(DomainError::Edit {
                reason: "entity parent does not exist",
            });
        }
        let reserved = [
            collection.id_field.as_str(),
            collection.label_field.as_str(),
            collection.order_field.as_str(),
            collection.parent_field.as_str(),
            collection.relation_field.as_str(),
        ]
        .into_iter()
        .chain(collection.inheritance_fields.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
        if draft
            .fields
            .keys()
            .any(|field| reserved.contains(field.as_str()))
        {
            return Err(DomainError::Edit {
                reason: "extra entity fields cannot replace structural authoring fields",
            });
        }
        let parent = draft.parent_id.unwrap_or_default();
        let mut edits = vec![
            (
                vec![export.to_owned(), draft.id.clone(), collection.id_field],
                Some(DomainValue::String(draft.id.clone())),
            ),
            (
                vec![export.to_owned(), draft.id.clone(), collection.label_field],
                Some(DomainValue::String(draft.label)),
            ),
            (
                vec![export.to_owned(), draft.id.clone(), collection.order_field],
                Some(DomainValue::Number(draft.order as f64)),
            ),
            (
                vec![export.to_owned(), draft.id.clone(), collection.parent_field],
                Some(DomainValue::String(parent.clone())),
            ),
            (
                vec![
                    export.to_owned(),
                    draft.id.clone(),
                    collection.relation_field,
                ],
                Some(DomainValue::List(Vec::new())),
            ),
        ];
        edits.extend(collection.inheritance_fields.into_iter().map(|field| {
            (
                vec![export.to_owned(), draft.id.clone(), field],
                Some(DomainValue::String(parent.clone())),
            )
        }));
        edits.extend(draft.fields.into_iter().map(|(field, value)| {
            (
                vec![export.to_owned(), draft.id.clone(), field],
                Some(value),
            )
        }));
        self.edit_overrides(alias, edits)
    }

    /// Rename an entity label while preserving its stable id and every reference to it.
    pub fn rename_entity(
        &mut self,
        alias: &str,
        export: &str,
        id: &str,
        label: String,
    ) -> Result<(), DomainError> {
        let collection = self.collection_declaration(alias, export)?;
        self.require_entity(alias, export, id)?;
        self.edit_overrides(
            alias,
            [(
                vec![export.to_owned(), id.to_owned(), collection.label_field],
                Some(DomainValue::String(label)),
            )],
        )
    }

    /// Replace sibling order values without changing stable ids or relationships.
    pub fn reorder_entities(
        &mut self,
        alias: &str,
        export: &str,
        ordered_ids: &[String],
    ) -> Result<(), DomainError> {
        let collection = self.collection_declaration(alias, export)?;
        let Some(inspection) = self.collection_inspection(alias, export) else {
            return Err(DomainError::Edit {
                reason: "entity collection is unavailable",
            });
        };
        let requested = ordered_ids.iter().collect::<BTreeSet<_>>();
        let existing = inspection.entities.keys().collect::<BTreeSet<_>>();
        if requested.len() != ordered_ids.len() || requested != existing {
            return Err(DomainError::Edit {
                reason: "reorder must contain every stable entity id exactly once",
            });
        }
        let edits = ordered_ids.iter().enumerate().map(|(order, id)| {
            (
                vec![
                    export.to_owned(),
                    id.clone(),
                    collection.order_field.clone(),
                ],
                Some(DomainValue::Number(order as f64)),
            )
        });
        self.edit_overrides(alias, edits)
    }

    /// Move one entity under another root/entity, rejecting unknown ids and cycles atomically.
    pub fn nest_entity(
        &mut self,
        alias: &str,
        export: &str,
        id: &str,
        parent_id: Option<&str>,
    ) -> Result<(), DomainError> {
        let collection = self.collection_declaration(alias, export)?;
        self.require_entity(alias, export, id)?;
        if let Some(parent) = parent_id {
            self.require_entity(alias, export, parent)?;
        }
        self.edit_overrides(
            alias,
            [(
                vec![export.to_owned(), id.to_owned(), collection.parent_field],
                Some(DomainValue::String(
                    parent_id.unwrap_or_default().to_owned(),
                )),
            )],
        )
    }

    /// Add or remove a symmetric stable relationship in one atomic source edit.
    pub fn relate_entities(
        &mut self,
        alias: &str,
        export: &str,
        left: &str,
        right: &str,
        related: bool,
    ) -> Result<(), DomainError> {
        if left == right {
            return Err(DomainError::Edit {
                reason: "an entity cannot relate to itself",
            });
        }
        let collection = self.collection_declaration(alias, export)?;
        let left_entity = self.require_entity(alias, export, left)?;
        let right_entity = self.require_entity(alias, export, right)?;
        let mut left_relations = left_entity.related_ids;
        let mut right_relations = right_entity.related_ids;
        update_relation(&mut left_relations, right, related);
        update_relation(&mut right_relations, left, related);
        self.edit_overrides(
            alias,
            [
                (
                    vec![
                        export.to_owned(),
                        left.to_owned(),
                        collection.relation_field.clone(),
                    ],
                    Some(string_list_value(left_relations)),
                ),
                (
                    vec![
                        export.to_owned(),
                        right.to_owned(),
                        collection.relation_field,
                    ],
                    Some(string_list_value(right_relations)),
                ),
            ],
        )
    }

    /// Select a pack fallback or another stable entity as one explicit inheritance source.
    pub fn set_entity_inheritance(
        &mut self,
        alias: &str,
        export: &str,
        id: &str,
        field: &str,
        source_id: Option<&str>,
    ) -> Result<(), DomainError> {
        let collection = self.collection_declaration(alias, export)?;
        self.require_entity(alias, export, id)?;
        if collection
            .inheritance_fields
            .binary_search_by(|candidate| candidate.as_str().cmp(field))
            .is_err()
        {
            return Err(DomainError::Edit {
                reason: "field is not a declared inheritance source",
            });
        }
        if let Some(source) = source_id {
            self.require_entity(alias, export, source)?;
        }
        self.edit_overrides(
            alias,
            [(
                vec![export.to_owned(), id.to_owned(), field.to_owned()],
                Some(DomainValue::String(
                    source_id.unwrap_or_default().to_owned(),
                )),
            )],
        )
    }

    fn collection_declaration(
        &self,
        alias: &str,
        export: &str,
    ) -> Result<weave_domain::EntityCollectionDeclaration, DomainError> {
        self.active_modules
            .get(alias)
            .and_then(|module| {
                module
                    .manifest
                    .authoring
                    .entity_collections
                    .iter()
                    .find(|collection| collection.export == export)
            })
            .cloned()
            .ok_or(DomainError::Edit {
                reason: "module entity collection is unavailable",
            })
    }

    fn collection_inspection(
        &self,
        alias: &str,
        export: &str,
    ) -> Option<EntityCollectionInspection> {
        self.module_inspections()
            .into_iter()
            .find(|module| module.alias == alias)?
            .entity_collections
            .into_iter()
            .find(|collection| collection.export == export)
    }

    fn entity(&self, alias: &str, export: &str, id: &str) -> Option<EntityInspection> {
        self.collection_inspection(alias, export)?
            .entities
            .remove(id)
    }

    fn require_entity(
        &self,
        alias: &str,
        export: &str,
        id: &str,
    ) -> Result<EntityInspection, DomainError> {
        self.entity(alias, export, id).ok_or(DomainError::Edit {
            reason: "entity stable id does not exist",
        })
    }

    fn edit_overrides(
        &mut self,
        alias: &str,
        edits: impl IntoIterator<Item = (Vec<String>, Option<DomainValue>)>,
    ) -> Result<(), DomainError> {
        let mut document = self.document.clone().ok_or(DomainError::Edit {
            reason: "current source is not parseable",
        })?;
        let module = document
            .items
            .iter_mut()
            .find_map(|item| match &mut item.node {
                Item::Module(module) if module.node.alias == alias => Some(&mut module.node),
                _ => None,
            })
            .ok_or(DomainError::Edit {
                reason: "module alias is not active in source",
            })?;
        let mut pending = BTreeMap::new();
        for (path, value) in edits {
            if pending.insert(path, value).is_some() {
                return Err(DomainError::Edit {
                    reason: "one editor action repeats an override path",
                });
            }
        }
        let mut rewritten = Vec::with_capacity(module.entries.len() + pending.len());
        for mut entry in std::mem::take(&mut module.entries) {
            let ModuleEntry::Override { path, value } = &mut entry.node else {
                rewritten.push(entry);
                continue;
            };
            let Some(replacement) = pending.remove(path) else {
                rewritten.push(entry);
                continue;
            };
            if let Some(replacement) = replacement {
                *value = domain_value_expression(replacement)?;
                rewritten.push(entry);
            }
        }
        for (path, replacement) in pending {
            if let Some(replacement) = replacement {
                rewritten.push(Spanned::new(
                    ModuleEntry::Override {
                        path,
                        value: domain_value_expression(replacement)?,
                    },
                    Span::default(),
                ));
            }
        }
        module.entries = rewritten;
        let source = weave_fmt::format_document(&document);
        let compiled = compile_with_modules(
            &source,
            &CompileOptions {
                source_name: self.source_name.clone(),
            },
            &self.domain_catalog,
        )
        .map_err(|error| DomainError::Compile {
            count: error.diagnostics.len(),
        })?;
        let runtime = Story::with_seed(compiled.story.clone(), self.seed)?;
        self.source = source;
        self.diagnostics = compiled.diagnostics;
        self.document = Some(document);
        self.active_modules = compiled.domain_modules;
        self.last_valid_story = Some(compiled.story);
        self.runtime = Some(runtime);
        Ok(())
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

fn export_origins(
    resolved: &ResolvedDomainModule,
    export: &str,
    value: &DomainValue,
) -> BTreeMap<String, DomainValueOrigin> {
    let mut origins = BTreeMap::new();
    let mut path = vec![export.to_owned()];
    collect_value_origins(resolved, value, &mut path, &mut origins);
    origins
}

fn collect_value_origins(
    resolved: &ResolvedDomainModule,
    value: &DomainValue,
    path: &mut Vec<String>,
    origins: &mut BTreeMap<String, DomainValueOrigin>,
) {
    origins.insert(path.join("."), classify_origin(resolved, path));
    if let DomainValue::Object(fields) = value {
        for (name, value) in fields {
            path.push(name.clone());
            collect_value_origins(resolved, value, path, origins);
            path.pop();
        }
    }
}

fn classify_origin(resolved: &ResolvedDomainModule, path: &[String]) -> DomainValueOrigin {
    if resolved
        .authored_overrides
        .iter()
        .any(|authored| path.starts_with(&authored.path) || authored.path.starts_with(path))
        && domain_value_at(&resolved.pack.values, path).is_none()
    {
        return DomainValueOrigin::Authored;
    }
    if resolved
        .authored_overrides
        .iter()
        .any(|authored| path.starts_with(&authored.path))
    {
        return DomainValueOrigin::Authored;
    }
    let transformations = resolved
        .pack
        .provenance
        .transformations
        .iter()
        .map(|transformation| transformation.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut claim_path = path.to_vec();
    while !claim_path.is_empty() {
        let claim = format!("values.{}", claim_path.join("."));
        if let Some(references) = resolved.pack.provenance.claims.get(&claim) {
            return if references
                .iter()
                .any(|reference| transformations.contains(reference.as_str()))
            {
                DomainValueOrigin::Generated
            } else {
                DomainValueOrigin::Inherited
            };
        }
        claim_path.pop();
    }
    DomainValueOrigin::Inherited
}

fn domain_value_at<'a>(
    values: &'a BTreeMap<String, DomainValue>,
    path: &[String],
) -> Option<&'a DomainValue> {
    let (export, fields) = path.split_first()?;
    let mut value = values.get(export)?;
    for field in fields {
        let DomainValue::Object(object) = value else {
            return None;
        };
        value = object.get(field)?;
    }
    Some(value)
}

fn entity_collection_inspections(
    resolved: &ResolvedDomainModule,
) -> Vec<EntityCollectionInspection> {
    resolved
        .manifest
        .authoring
        .entity_collections
        .iter()
        .filter_map(|collection| {
            let DomainValue::Object(values) = resolved.effective_values.get(&collection.export)?
            else {
                return None;
            };
            let origins = export_origins(
                resolved,
                &collection.export,
                resolved.effective_values.get(&collection.export)?,
            );
            let mut entities = BTreeMap::new();
            for (id, value) in values {
                let DomainValue::Object(fields) = value else {
                    continue;
                };
                let Some(DomainValue::String(label)) = fields.get(&collection.label_field) else {
                    continue;
                };
                let Some(DomainValue::Number(order)) = fields.get(&collection.order_field) else {
                    continue;
                };
                let Some(DomainValue::String(parent)) = fields.get(&collection.parent_field) else {
                    continue;
                };
                let Some(DomainValue::List(related)) = fields.get(&collection.relation_field)
                else {
                    continue;
                };
                let related_ids = related
                    .iter()
                    .filter_map(|value| match value {
                        DomainValue::String(value) => Some(value.clone()),
                        _ => None,
                    })
                    .collect();
                let inheritance = collection
                    .inheritance_fields
                    .iter()
                    .filter_map(|field| match fields.get(field) {
                        Some(DomainValue::String(value)) => {
                            Some((field.clone(), (!value.is_empty()).then(|| value.clone())))
                        }
                        _ => None,
                    })
                    .collect();
                let prefix = format!("{}.{}", collection.export, id);
                let entity_origins = origins
                    .iter()
                    .filter(|(path, _)| *path == &prefix || path.starts_with(&format!("{prefix}.")))
                    .map(|(path, origin)| (path.clone(), *origin))
                    .collect();
                entities.insert(
                    id.clone(),
                    EntityInspection {
                        id: id.clone(),
                        label: label.clone(),
                        order: *order as i64,
                        parent_id: (!parent.is_empty()).then(|| parent.clone()),
                        related_ids,
                        inheritance,
                        value: value.clone(),
                        origins: entity_origins,
                    },
                );
            }
            let mut roots = entities
                .values()
                .filter(|entity| entity.parent_id.is_none())
                .map(|entity| (entity.order, entity.id.clone()))
                .collect::<Vec<_>>();
            roots.sort();
            Some(EntityCollectionInspection {
                export: collection.export.clone(),
                roots: roots.into_iter().map(|(_, id)| id).collect(),
                entities,
            })
        })
        .collect()
}

fn domain_value_expression(value: DomainValue) -> Result<Spanned<Expr>, DomainError> {
    let expression = match value {
        DomainValue::Null => Expr::Literal(Literal::Null),
        DomainValue::Bool(value) => Expr::Literal(Literal::Bool(value)),
        DomainValue::Number(value) if value.is_finite() && value >= 0.0 => {
            Expr::Literal(Literal::Number(value))
        }
        DomainValue::Number(value) if value.is_finite() => Expr::Unary {
            operator: UnaryOperator::Negate,
            operand: Box::new(Spanned::new(
                Expr::Literal(Literal::Number(-value)),
                Span::default(),
            )),
        },
        DomainValue::Number(_) => {
            return Err(DomainError::Edit {
                reason: "domain numbers must be finite",
            });
        }
        DomainValue::String(value) => Expr::Literal(Literal::String(value)),
        DomainValue::Symbol(value) => Expr::Literal(Literal::Symbol(value)),
        DomainValue::List(values) => Expr::List(
            values
                .into_iter()
                .map(domain_value_expression)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        DomainValue::Object(_) => {
            return Err(DomainError::Edit {
                reason: "editor overrides set object leaves rather than whole objects",
            });
        }
    };
    Ok(Spanned::new(expression, Span::default()))
}

fn valid_editor_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
        && !value.contains("__")
}

fn update_relation(relations: &mut Vec<String>, id: &str, related: bool) {
    relations.retain(|relation| relation != id);
    if related {
        relations.push(id.to_owned());
        relations.sort();
    }
}

fn string_list_value(values: Vec<String>) -> DomainValue {
    DomainValue::List(values.into_iter().map(DomainValue::String).collect())
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
                "../../examples/domain-modules/weave-world/packs/glasswind_composed.weave-domain.json"
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

    fn character_catalog() -> DomainCatalog {
        let manifest = weave_domain::ModuleManifest::from_json(include_str!(
            "../../examples/domain-modules/weave-character/module.weave-module.json"
        ))
        .expect("canonical Character manifest");
        let pack = weave_domain::DomainPack::from_json(include_str!(
            "../../examples/domain-modules/weave-character/ari_vale.weave-domain.json"
        ))
        .expect("canonical Character pack");
        DomainCatalog::from_artifacts([manifest], [pack]).expect("Character catalog")
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
                .contains("pack: \"aotearoa_new_zealand@=1.1.0\"")
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
    fn character_profile_is_typed_editable_and_derived_writeback_safe() {
        let source = include_str!("../../examples/domain-modules/weave-character/ari-vale.weave");
        let catalog = character_catalog();
        let mut session = DomainSession::with_domain_catalog(31, catalog.clone());
        session
            .compile_source(source, Some("ari-vale.weave".to_owned()))
            .expect("editor compiles Character source");

        let inspection = &session.module_inspections()[0];
        assert_eq!(inspection.alias, "character");
        assert_eq!(inspection.id, "org.weave.character");
        assert_eq!(inspection.pack_id, "ari_vale");
        assert_eq!(inspection.read_only_paths.len(), 6);
        assert!(inspection.read_only_paths.iter().any(|declaration| {
            declaration.path == ["profile", "date_context"].map(str::to_owned)
                && declaration.reason.contains("non-causal read-only context")
        }));
        assert!(inspection.read_only_paths.iter().any(|declaration| {
            declaration.path == ["profile", "ocean"].map(str::to_owned)
                && declaration.reason.contains("lossy derived")
        }));
        let profile = inspection
            .exports
            .iter()
            .find(|export| export.name == "profile")
            .expect("Character profile schema and value are inspectable");
        assert_eq!(
            profile.origins.get("profile.identity.display_name.value"),
            Some(&DomainValueOrigin::Authored)
        );
        assert_eq!(
            profile.origins.get("profile.ocean.openness.score"),
            Some(&DomainValueOrigin::Generated)
        );
        assert_eq!(
            domain_value_at(
                &session.active_modules()["character"].effective_values,
                &[
                    "profile",
                    "hexaco",
                    "openness",
                    "creativity",
                    "projection_score",
                ]
                .map(str::to_owned),
            ),
            Some(&DomainValue::Number(0.86))
        );

        session
            .set_module_override(
                "character",
                &["profile", "identity", "display_name", "value"],
                DomainValue::String("Ari Vale of the Lantern Road".to_owned()),
            )
            .expect("editor revises the presentation-facing name");
        assert!(session.source().contains("Ari Vale of the Lantern Road"));
        assert_eq!(
            domain_value_at(
                &session.active_modules()["character"].effective_values,
                &["profile", "identity", "id"].map(str::to_owned),
            ),
            Some(&DomainValue::String(
                "org.weave.character.ari_vale".to_owned()
            ))
        );

        let before_writeback = session.source().to_owned();
        assert!(
            session
                .set_module_override(
                    "character",
                    &["profile", "ocean", "openness", "score"],
                    DomainValue::Number(0.1),
                )
                .is_err()
        );
        assert_eq!(session.source(), before_writeback);
        assert!(session.diagnostics().is_empty());

        let mut buffer = TextBuffer::with_domain_catalog(source, catalog);
        buffer.format().expect("Character source formats");
        assert!(buffer.diagnostics().is_empty());
        let formatted = weave_compiler::compile_with_modules(
            buffer.source(),
            &CompileOptions {
                source_name: Some("ari-vale.weave".to_owned()),
            },
            &character_catalog(),
        )
        .expect("formatted Character source compiles");
        assert_eq!(
            formatted.story.modules,
            weave_compiler::compile_with_modules(
                source,
                &CompileOptions::default(),
                &character_catalog()
            )
            .expect("original Character source compiles")
            .story
            .modules
        );
    }

    #[test]
    fn authored_world_hierarchy_edits_are_stable_atomic_and_lineage_aware() {
        let source =
            include_str!("../../examples/domain-modules/weave-world/authored-setting.weave");
        let mut session = DomainSession::with_domain_catalog(23, world_catalog());
        session
            .compile_source(source, Some("authored-setting.weave".to_owned()))
            .expect("authored world compiles");

        let world = &session.module_inspections()[0];
        let seed = world
            .exports
            .iter()
            .find(|export| export.name == "seed")
            .expect("seed inspection");
        assert_eq!(
            seed.origins.get("seed.climate"),
            Some(&DomainValueOrigin::Generated)
        );
        let places = &world.entity_collections[0];
        assert_eq!(places.roots, ["glasswind_reach"]);
        assert_eq!(
            places.entities["emberwake_harbor"].parent_id.as_deref(),
            Some("glasswind_reach")
        );
        assert_eq!(
            places.entities["lantern_road"]
                .inheritance
                .get("environment_source"),
            Some(&Some("glasswind_reach".to_owned()))
        );
        assert_eq!(
            places.entities["emberwake_harbor"]
                .origins
                .get("places.emberwake_harbor.name"),
            Some(&DomainValueOrigin::Authored)
        );
        let resolved = world.resolved_world.as_ref().expect("resolved World view");
        assert!(matches!(
            resolved.places["glasswind_reach"].environment.origins["primary_biome"],
            weave_world::WorldValueOrigin::Generated { .. }
        ));
        assert!(matches!(
            resolved.places["lantern_road"].environment.origins["primary_biome"],
            weave_world::WorldValueOrigin::Inherited { .. }
        ));
        assert!(matches!(
            resolved.places["emberwake_harbor"].environment.origins["coastal"],
            weave_world::WorldValueOrigin::Authored { .. }
        ));

        session
            .rename_entity(
                "world",
                "places",
                "emberwake_harbor",
                "Cinderwake Harbor".to_owned(),
            )
            .expect("rename label");
        assert!(session.source().contains("\"Cinderwake Harbor\""));
        assert!(session.source().contains("\"emberwake_harbor\""));

        session
            .create_entity(
                "world",
                "places",
                EntityDraft {
                    id: "fogbound_step".to_owned(),
                    label: "Fogbound Step".to_owned(),
                    order: 2,
                    parent_id: Some("glasswind_reach".to_owned()),
                    fields: BTreeMap::from([(
                        "kind".to_owned(),
                        DomainValue::Symbol("site".to_owned()),
                    )]),
                },
            )
            .expect("create stable place");
        session
            .nest_entity("world", "places", "fogbound_step", Some("emberwake_harbor"))
            .expect("nest stable place");
        session
            .relate_entities("world", "places", "fogbound_step", "lantern_road", true)
            .expect("relate stable places");
        session
            .reorder_entities(
                "world",
                "places",
                &[
                    "glasswind_reach".to_owned(),
                    "emberwake_harbor".to_owned(),
                    "lantern_road".to_owned(),
                    "saltglass_point".to_owned(),
                    "fogbound_step".to_owned(),
                ],
            )
            .expect("reorder without changing ids");

        session
            .reset_module_override(
                "world",
                &[
                    "places",
                    "emberwake_harbor",
                    "environment_override",
                    "coastal",
                ],
            )
            .expect("reset local override to inheritance");
        assert!(
            !session
                .source()
                .contains("override places.emberwake_harbor.environment_override.coastal")
        );

        let before_cycle = session.source().to_owned();
        assert!(
            session
                .nest_entity(
                    "world",
                    "places",
                    "glasswind_reach",
                    Some("saltglass_point"),
                )
                .is_err()
        );
        assert_eq!(session.source(), before_cycle);

        let inspection = &session.module_inspections()[0].entity_collections[0];
        assert_eq!(
            inspection.entities["fogbound_step"].parent_id.as_deref(),
            Some("emberwake_harbor")
        );
        assert_eq!(
            inspection.entities["fogbound_step"].related_ids,
            ["lantern_road"]
        );
        assert!(
            inspection.entities["lantern_road"]
                .related_ids
                .contains(&"fogbound_step".to_owned())
        );
        assert_eq!(
            session.last_valid_story().expect("edited story").modules["world"].value(&[
                "rules",
                "booleans",
                "beacons_answer_storms"
            ]),
            Some(&weave_core::ir::DomainValueIr::Bool(true))
        );
    }

    #[test]
    fn composed_world_preview_exposes_layers_and_promotes_generated_values() {
        let source =
            include_str!("../../examples/domain-modules/weave-world/composed-setting.weave");
        let receipt = weave_world::WorldCompositionReceipt::from_json(include_str!(
            "../../examples/domain-modules/weave-world/composition.receipt.json"
        ))
        .expect("checked composition receipt");
        let mut session = DomainSession::with_domain_catalog(29, world_catalog());
        session
            .register_world_composition(receipt)
            .expect("register composition receipt");
        session
            .compile_source(source, Some("composed-setting.weave".to_owned()))
            .expect("composed world compiles");

        let inspection = &session.module_inspections()[0];
        let composition = inspection
            .world_composition
            .as_ref()
            .expect("composition is inspectable");
        assert_eq!(
            composition
                .layers
                .iter()
                .map(|layer| layer.id.as_str())
                .collect::<Vec<_>>(),
            ["broad_reference", "regional_climate", "ecosystem_surface"]
        );
        assert!(composition.differences.iter().any(|difference| {
            difference.path == "seed.primary_biome"
                && difference.incoming_layer_id == "ecosystem_surface"
                && difference.resolution == weave_world::WorldLayerResolution::Replaced
        }));
        let seed = inspection
            .exports
            .iter()
            .find(|export| export.name == "seed")
            .expect("seed inspection");
        assert_eq!(
            seed.origins.get("seed.primary_biome"),
            Some(&DomainValueOrigin::Generated)
        );
        let resolved = inspection.resolved_world.as_ref().expect("resolved World");
        assert_eq!(
            resolved.places["glasswind_reach"].environment.values["primary_biome"],
            DomainValue::Symbol("temperate_conifer_forest".to_owned())
        );
        assert_eq!(
            resolved.places["glasswind_reach"].climate.values["band"],
            DomainValue::Symbol("humid_continental".to_owned())
        );

        session
            .promote_module_value("world", &["seed", "primary_biome"])
            .expect("promote generated biome");
        assert!(
            session
                .source()
                .contains("override seed.primary_biome: temperate_conifer_forest")
        );
        let promoted = &session.module_inspections()[0];
        let promoted_seed = promoted
            .exports
            .iter()
            .find(|export| export.name == "seed")
            .expect("promoted seed inspection");
        assert_eq!(
            promoted_seed.origins.get("seed.primary_biome"),
            Some(&DomainValueOrigin::Authored)
        );
        assert!(promoted.world_composition.is_some());

        let before_rejected_promotion = session.source().to_owned();
        assert!(
            session
                .promote_module_value("world", &["seed", "climate"])
                .is_err()
        );
        assert_eq!(session.source(), before_rejected_promotion);
        session
            .reset_module_override("world", &["seed", "primary_biome"])
            .expect("reset promotion");
        let reset = &session.module_inspections()[0];
        let reset_seed = reset
            .exports
            .iter()
            .find(|export| export.name == "seed")
            .expect("reset seed inspection");
        assert_eq!(
            reset_seed.origins.get("seed.primary_biome"),
            Some(&DomainValueOrigin::Generated)
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
