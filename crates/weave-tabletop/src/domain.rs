use std::collections::BTreeMap;

use semver::Version;
use weave_domain::{
    CapabilityDeclaration, DOMAIN_CONTRACT_VERSION, DOMAIN_PACK_FORMAT_VERSION, DomainPack,
    DomainValue, ExportDeclaration, ExportSource, FieldDeclaration, ModuleAuthor, ModuleAuthoring,
    ModuleManifest, ModuleRequirement, Provenance, ProvenanceKind, ProvenanceSource,
    ReadOnlyPathDeclaration, TypeExpression, validate_manifest, validate_pack,
};

use crate::{
    AdapterManifest, AdapterSourceClass, ResolvedAdapter, TabletopCapability,
    TabletopCharacterProjection, TabletopError, TabletopState, resolved_adapter,
    validate_character_projection, validate_tabletop_state,
};

/// Project one adapter contract into an ordinary declarative domain module.
///
/// The generated module is adapter-id agnostic: its exact closed types and capability fields come
/// from the manifest, allowing existing parser/compiler/editor/runtime paths to type-check source
/// without any ruleset-specific branch.
pub fn tabletop_domain_manifest(
    adapter: &AdapterManifest,
) -> Result<ModuleManifest, TabletopError> {
    let provenance = domain_provenance(adapter, ["manifest"]);
    let manifest = ModuleManifest {
        contract_version: DOMAIN_CONTRACT_VERSION,
        pack_format_version: DOMAIN_PACK_FORMAT_VERSION,
        id: adapter.id.clone(),
        version: adapter.version.clone(),
        namespace: adapter.namespace.clone(),
        title: adapter.title.clone(),
        summary: adapter.summary.clone(),
        authors: vec![ModuleAuthor {
            name: "Declared adapter source steward".to_owned(),
            url: Some(adapter.provenance.source_url.clone()),
        }],
        license: adapter.provenance.license.clone(),
        license_url: adapter.provenance.license_url.clone(),
        weave_version: adapter.weave_version.clone(),
        capabilities: vec![
            CapabilityDeclaration {
                id: "data".to_owned(),
                version: 1,
            },
            CapabilityDeclaration {
                id: "editor_schema".to_owned(),
                version: 1,
            },
            CapabilityDeclaration {
                id: "operations".to_owned(),
                version: 1,
            },
            CapabilityDeclaration {
                id: "state".to_owned(),
                version: 1,
            },
        ],
        dependencies: Vec::new(),
        types: adapter.types.clone(),
        exports: BTreeMap::from([
            (
                "adapter".to_owned(),
                export(adapter_coordinate_type(), ExportSource::Pack, "Exact adapter coordinate."),
            ),
            (
                "capabilities".to_owned(),
                export(
                    capability_type(adapter),
                    ExportSource::Pack,
                    "Only capabilities explicitly declared by this adapter release.",
                ),
            ),
            (
                "definition".to_owned(),
                export(
                    adapter.definition_type.clone(),
                    ExportSource::Pack,
                    "Immutable adapter-owned character definition.",
                ),
            ),
            (
                "state".to_owned(),
                export(
                    adapter.state_type.clone(),
                    ExportSource::State,
                    "Versioned mutable adapter-owned state.",
                ),
            ),
        ]),
        authoring: ModuleAuthoring {
            entity_collections: Vec::new(),
            read_only_paths: ["adapter", "capabilities", "definition"]
                .into_iter()
                .map(|path| ReadOnlyPathDeclaration {
                    path: vec![path.to_owned()],
                    reason: "Generated from the exact reviewed adapter projection; revise it through the adapter authoring workflow.".to_owned(),
                })
                .collect(),
        },
        provenance,
    };
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|_| TabletopError::Artifact)?;
    validate_manifest(&manifest, &current).map_err(|_| TabletopError::Artifact)?;
    Ok(manifest)
}

/// Project one reviewed Character definition and mutable state into an ordinary domain pack.
pub fn tabletop_domain_pack(
    adapter: &AdapterManifest,
    projection: &TabletopCharacterProjection,
    state: &TabletopState,
    pack_id: &str,
    pack_version: &str,
    title: &str,
) -> Result<DomainPack, TabletopError> {
    validate_character_projection(projection, std::slice::from_ref(adapter))?;
    validate_tabletop_state(state, adapter)?;
    let active = projection
        .active
        .as_ref()
        .ok_or(TabletopError::AdapterNotInstalled)?;
    if active.adapter != state.adapter || active.definition_sha256 != state.definition_sha256 {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    let coordinate = resolved_adapter(adapter)?;
    let capabilities = DomainValue::Object(
        adapter
            .capabilities
            .iter()
            .map(|declaration| {
                (
                    capability_id(declaration.capability).to_owned(),
                    DomainValue::Bool(true),
                )
            })
            .collect(),
    );
    let provenance = domain_provenance(
        adapter,
        [
            "values.adapter",
            "values.capabilities",
            "values.definition",
            "values.state",
        ],
    );
    let pack = DomainPack {
        pack_format_version: DOMAIN_PACK_FORMAT_VERSION,
        id: pack_id.to_owned(),
        version: pack_version.to_owned(),
        title: title.to_owned(),
        module: ModuleRequirement {
            id: adapter.id.clone(),
            version: format!("={}", adapter.version),
        },
        dependencies: Vec::new(),
        values: BTreeMap::from([
            ("adapter".to_owned(), coordinate_value(&coordinate)),
            ("capabilities".to_owned(), capabilities),
            ("definition".to_owned(), active.definition.clone()),
            ("state".to_owned(), state.value.clone()),
        ]),
        provenance,
    };
    let manifest = tabletop_domain_manifest(adapter)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|_| TabletopError::Artifact)?;
    validate_pack(&pack, &manifest, &current).map_err(|_| TabletopError::Artifact)?;
    Ok(pack)
}

fn export(
    value_type: TypeExpression,
    source: ExportSource,
    description: &str,
) -> ExportDeclaration {
    ExportDeclaration {
        value_type,
        required: true,
        source,
        description: description.to_owned(),
    }
}

fn adapter_coordinate_type() -> TypeExpression {
    TypeExpression::Object {
        fields: BTreeMap::from([
            (
                "content_sha256".to_owned(),
                required_string(64, 64, "Exact validated manifest SHA-256."),
            ),
            (
                "id".to_owned(),
                required_string(1, 255, "Globally namespaced adapter id."),
            ),
            (
                "schema_version".to_owned(),
                FieldDeclaration {
                    value_type: TypeExpression::Number {
                        integer: true,
                        minimum: Some(1.0),
                        maximum: Some(u32::MAX as f64),
                    },
                    required: true,
                    description: "Mutable state schema version.".to_owned(),
                },
            ),
            (
                "version".to_owned(),
                required_string(1, 64, "Exact adapter semantic version."),
            ),
        ]),
    }
}

fn capability_type(adapter: &AdapterManifest) -> TypeExpression {
    TypeExpression::Object {
        fields: adapter
            .capabilities
            .iter()
            .map(|declaration| {
                (
                    capability_id(declaration.capability).to_owned(),
                    FieldDeclaration {
                        value_type: TypeExpression::Bool,
                        required: true,
                        description: "Exact declared adapter capability.".to_owned(),
                    },
                )
            })
            .collect(),
    }
}

fn coordinate_value(coordinate: &ResolvedAdapter) -> DomainValue {
    DomainValue::Object(BTreeMap::from([
        (
            "content_sha256".to_owned(),
            DomainValue::String(coordinate.content_sha256.clone()),
        ),
        ("id".to_owned(), DomainValue::String(coordinate.id.clone())),
        (
            "schema_version".to_owned(),
            DomainValue::Number(coordinate.schema_version as f64),
        ),
        (
            "version".to_owned(),
            DomainValue::String(coordinate.version.clone()),
        ),
    ]))
}

fn required_string(min_length: usize, max_length: usize, description: &str) -> FieldDeclaration {
    FieldDeclaration {
        value_type: TypeExpression::String {
            min_length,
            max_length,
        },
        required: true,
        description: description.to_owned(),
    }
}

fn domain_provenance<const N: usize>(adapter: &AdapterManifest, claims: [&str; N]) -> Provenance {
    let kind = match adapter.provenance.source_class {
        AdapterSourceClass::Original => ProvenanceKind::Original,
        AdapterSourceClass::VerifiedCc0 | AdapterSourceClass::SeparatelyLicensedApache2 => {
            ProvenanceKind::PublicSource
        }
    };
    Provenance {
        sources: vec![ProvenanceSource {
            id: "adapter_source".to_owned(),
            kind,
            url: adapter.provenance.source_url.clone(),
            revision: adapter.provenance.revision.clone(),
            sha256: Some(adapter.provenance.sha256.clone()),
            license: adapter.provenance.license.clone(),
            license_url: adapter.provenance.license_url.clone(),
            attribution: adapter.provenance.attribution.clone(),
            modified: false,
        }],
        transformations: Vec::new(),
        claims: claims
            .into_iter()
            .map(|claim| (claim.to_owned(), vec!["adapter_source".to_owned()]))
            .collect(),
    }
}

fn capability_id(capability: TabletopCapability) -> &'static str {
    match capability {
        TabletopCapability::CharacterCreation => "character_creation",
        TabletopCapability::DerivedValues => "derived_values",
        TabletopCapability::ChecksAndConflicts => "checks_and_conflicts",
        TabletopCapability::ResourcesAndConditions => "resources_and_conditions",
        TabletopCapability::EquipmentAndAbilities => "equipment_and_abilities",
        TabletopCapability::Advancement => "advancement",
        TabletopCapability::Encounters => "encounters",
        TabletopCapability::Clocks => "clocks",
        TabletopCapability::Scenes => "scenes",
        TabletopCapability::Factions => "factions",
        TabletopCapability::WorldState => "world_state",
        TabletopCapability::CampaignState => "campaign_state",
    }
}
