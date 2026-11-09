//! Drift test for the checked-in portable adapter catalog.
//!
//! [`live_catalog`] rebuilds the catalog from the adapter manifests and the reviewed source-policy
//! rows. [`checked_in_catalog_matches_live_manifests`] fails when
//! `examples/tabletop-adapters/catalog/catalog.json` no longer matches that live data. Regenerate
//! the file deliberately with the ignored [`regenerate_checked_in_catalog`] test.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use weave_tabletop::conformance::{ConformanceCheck, standard_source_policy, standard_suite};
use weave_tabletop::{
    AdapterManifest, AdapterMigrationDeclaration, AdapterProvenance, AdapterSourceClass,
    AdapterSourcePolicy, ExcludedMaterial, PolicyFile, TabletopCapability, dungeonpunk_manifest,
    freehack_manifest, open_adventure_manifest, open_tale_manifest, plug_and_play_manifest,
    resolved_adapter, sha256_bytes, spark_manifest, wicked_ones_manifest,
};

const CATALOG_FORMAT_VERSION: u32 = 1;
const COMPATIBILITY_MATRIX_FORMAT_VERSION: u32 = 1;

/// Every adapter manifest shipped in this build, in catalog order.
///
/// A contributor adding an adapter adds its manifest function here. The drift test also fails when
/// a source-policy row names an adapter that is absent from this list, so a new policy row cannot
/// be forgotten here.
fn live_manifests() -> Vec<AdapterManifest> {
    vec![
        plug_and_play_manifest(),
        dungeonpunk_manifest(),
        freehack_manifest(),
        wicked_ones_manifest(),
        spark_manifest(),
        open_adventure_manifest(),
        open_tale_manifest(),
    ]
}

/// Machine-readable index of the shipped adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    catalog_format_version: u32,
    adapters: Vec<CatalogAdapter>,
}

/// One shipped adapter and the exact reviewed data a consumer needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogAdapter {
    id: String,
    version: String,
    schema_version: u32,
    namespace: String,
    title: String,
    compatibility_label: String,
    summary: String,
    weave_version: String,
    character_contract_version: String,
    capabilities: Vec<CatalogCapability>,
    creation_steps: Vec<CatalogCreationStep>,
    operations: Vec<CatalogOperation>,
    source: CatalogSource,
    conformance: CatalogConformance,
    canonical_project: CatalogProject,
    consumers: CatalogConsumers,
    /// Exact content hash of the validated manifest; changes in types or visibility move it.
    content_sha256: String,
}

/// Runnable host consumers for one adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogConsumers {
    /// Exact command that replays this adapter in the Bevy consumer.
    bevy: String,
    /// Exact command that runs the PixiJS consumer suite, which covers every adapter.
    pixijs: String,
}

/// The checked-in canonical synthetic project and its single reproduction command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogProject {
    /// Repository-relative project directory.
    path: String,
    /// Exact command that compiles the harness, plays the scripted path, and replays every receipt.
    command: String,
    /// Repository-relative expected-output artifact.
    expected_output: String,
    /// SHA-256 of the exact checked-in expected-output bytes.
    expected_output_sha256: String,
    /// Deterministic creation seed recorded by the project.
    seed: u64,
}

/// One declared capability and its contract version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogCapability {
    id: String,
    version: u32,
}

/// One ordered creation step and its field identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogCreationStep {
    id: String,
    title: String,
    required_capability: String,
    fields: Vec<String>,
}

/// One declared resolver operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogOperation {
    id: String,
    title: String,
    required_capability: String,
    consumes_entropy: bool,
    event_kinds: Vec<String>,
}

/// Public-source boundary and licence record for one adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogSource {
    source_class: String,
    license: String,
    license_url: String,
    source_url: String,
    exact_artifact: String,
    revision: String,
    retrieved_on: String,
    sha256: String,
    attribution: String,
    covered_files_or_sections: Vec<String>,
    exclusions: Vec<String>,
    required_license_text: CatalogFile,
    additional_artifacts: Vec<CatalogArtifact>,
    notices: Vec<String>,
    compatibility_statement: String,
    policy: CatalogPolicy,
}

/// One reviewed artifact name paired with its exact digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogFile {
    artifact: String,
    sha256: String,
}

/// One companion source artifact in the reviewed boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogArtifact {
    source_url: String,
    exact_artifact: String,
    revision: String,
    retrieved_on: String,
    sha256: String,
    media_type: String,
    purpose: String,
    redistributed: bool,
}

/// The reviewed source-policy row that the conformance suite checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogPolicy {
    namespace: String,
    license: String,
    source_class: String,
    source_sha256: String,
    required_license_text: CatalogFile,
    required_notice_files: Vec<CatalogFile>,
    required_notices: Vec<String>,
    forbidden_files: Vec<String>,
}

/// Conformance-suite coverage for one adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogConformance {
    /// `standard_suite_registered` or `not_registered`.
    status: String,
    /// Check identifiers the standard suite applies, or an empty list when unregistered.
    checks: Vec<String>,
}

/// Build the catalog from the live manifests, source policy, and conformance registry.
fn live_catalog() -> Catalog {
    let policy = standard_source_policy();
    let registered = standard_suite()
        .adapter_ids()
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let manifests = live_manifests();
    let by_id = manifests
        .iter()
        .map(|manifest| (manifest.id.as_str(), manifest))
        .collect::<BTreeMap<_, _>>();

    for manifest in &manifests {
        assert!(
            policy.entry(&manifest.id).is_some(),
            "adapter `{}` has a manifest but no reviewed source-policy row",
            manifest.id
        );
    }
    for entry in policy.entries() {
        assert!(
            by_id.contains_key(entry.adapter_id),
            "source policy names adapter `{}` but no shipped manifest matches it",
            entry.adapter_id
        );
    }

    let adapters = manifests
        .iter()
        .map(|manifest| {
            let entry = policy
                .entry(&manifest.id)
                .expect("every shipped manifest has a checked policy row");
            catalog_adapter(manifest, entry, &registered, &canonical_project(manifest))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        adapters.len(),
        policy.entries().count(),
        "source policy and shipped manifests list different adapter counts"
    );

    Catalog {
        catalog_format_version: CATALOG_FORMAT_VERSION,
        adapters,
    }
}

fn catalog_adapter(
    manifest: &AdapterManifest,
    policy: &AdapterSourcePolicy,
    registered: &BTreeSet<String>,
    canonical_project: &CatalogProject,
) -> CatalogAdapter {
    let is_registered = registered.contains(&manifest.id);
    let checks = if is_registered {
        ConformanceCheck::ALL
            .iter()
            .map(|check| check.id().to_owned())
            .collect()
    } else {
        Vec::new()
    };

    CatalogAdapter {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        schema_version: manifest.schema_version,
        namespace: manifest.namespace.clone(),
        title: manifest.title.clone(),
        compatibility_label: manifest.compatibility_label.clone(),
        summary: manifest.summary.clone(),
        weave_version: manifest.weave_version.clone(),
        character_contract_version: manifest.character_contract_version.clone(),
        capabilities: manifest
            .capabilities
            .iter()
            .map(|declaration| CatalogCapability {
                id: capability_id(declaration.capability).to_owned(),
                version: declaration.version,
            })
            .collect(),
        creation_steps: manifest
            .creation_steps
            .iter()
            .map(|step| CatalogCreationStep {
                id: step.id.clone(),
                title: step.title.clone(),
                required_capability: capability_id(step.required_capability).to_owned(),
                fields: step.fields.iter().map(|field| field.id.clone()).collect(),
            })
            .collect(),
        operations: manifest
            .operations
            .values()
            .map(|operation| CatalogOperation {
                id: operation.id.clone(),
                title: operation.title.clone(),
                required_capability: capability_id(operation.required_capability).to_owned(),
                consumes_entropy: operation.consumes_entropy,
                event_kinds: operation.event_kinds.clone(),
            })
            .collect(),
        source: catalog_source(&manifest.provenance, policy),
        conformance: CatalogConformance {
            status: if is_registered {
                "standard_suite_registered".to_owned()
            } else {
                "not_registered".to_owned()
            },
            checks,
        },
        canonical_project: canonical_project.clone(),
        consumers: catalog_consumers(manifest),
        content_sha256: resolved_adapter(manifest)
            .expect("a shipped manifest resolves to its exact coordinate")
            .content_sha256,
    }
}

fn catalog_consumers(manifest: &AdapterManifest) -> CatalogConsumers {
    let key = manifest.namespace.replace('_', "-");
    CatalogConsumers {
        bevy: format!("cargo run -p weave-example-bevy-tabletop -- {key}"),
        pixijs: "cd examples/domain-module-pixijs && npm test".to_owned(),
    }
}

fn catalog_source(provenance: &AdapterProvenance, policy: &AdapterSourcePolicy) -> CatalogSource {
    CatalogSource {
        source_class: source_class_id(provenance.source_class).to_owned(),
        license: provenance.license.clone(),
        license_url: provenance.license_url.clone(),
        source_url: provenance.source_url.clone(),
        exact_artifact: provenance.exact_artifact.clone(),
        revision: provenance.revision.clone(),
        retrieved_on: provenance.retrieved_on.clone(),
        sha256: provenance.sha256.clone(),
        attribution: provenance.attribution.clone(),
        covered_files_or_sections: provenance.covered_files_or_sections.clone(),
        exclusions: provenance
            .exclusions
            .iter()
            .map(|material| excluded_material_id(*material).to_owned())
            .collect(),
        required_license_text: CatalogFile {
            artifact: provenance.required_license_text.artifact.clone(),
            sha256: provenance.required_license_text.sha256.clone(),
        },
        additional_artifacts: provenance
            .additional_artifacts
            .iter()
            .map(|artifact| CatalogArtifact {
                source_url: artifact.source_url.clone(),
                exact_artifact: artifact.exact_artifact.clone(),
                revision: artifact.revision.clone(),
                retrieved_on: artifact.retrieved_on.clone(),
                sha256: artifact.sha256.clone(),
                media_type: artifact.media_type.clone(),
                purpose: artifact.purpose.clone(),
                redistributed: artifact.redistributed,
            })
            .collect(),
        notices: provenance.notices.clone(),
        compatibility_statement: provenance.compatibility_statement.clone(),
        policy: CatalogPolicy {
            namespace: policy.namespace.to_owned(),
            license: policy.license.to_owned(),
            source_class: source_class_id(policy.source_class).to_owned(),
            source_sha256: policy.source_sha256.to_owned(),
            required_license_text: policy_file(policy.required_license_text),
            required_notice_files: policy
                .required_notice_files
                .iter()
                .copied()
                .map(policy_file)
                .collect(),
            required_notices: policy
                .required_notices
                .iter()
                .map(|notice| (*notice).to_owned())
                .collect(),
            forbidden_files: policy
                .forbidden_files
                .iter()
                .map(|file| (*file).to_owned())
                .collect(),
        },
    }
}

fn policy_file(file: PolicyFile) -> CatalogFile {
    CatalogFile {
        artifact: file.path.to_owned(),
        sha256: file.sha256.to_owned(),
    }
}

const fn capability_id(capability: TabletopCapability) -> &'static str {
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

const fn source_class_id(class: AdapterSourceClass) -> &'static str {
    match class {
        AdapterSourceClass::Original => "original",
        AdapterSourceClass::VerifiedCc0 => "verified_cc0",
        AdapterSourceClass::SeparatelyLicensedApache2 => "separately_licensed_apache2",
    }
}

const fn excluded_material_id(material: ExcludedMaterial) -> &'static str {
    match material {
        ExcludedMaterial::CommunitySupplements => "community_supplements",
        ExcludedMaterial::Logos => "logos",
        ExcludedMaterial::TradeDress => "trade_dress",
        ExcludedMaterial::Artwork => "artwork",
        ExcludedMaterial::Layout => "layout",
        ExcludedMaterial::UnverifiedAssets => "unverified_assets",
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalog_path() -> PathBuf {
    repository_root().join("examples/tabletop-adapters/catalog/catalog.json")
}

fn matrix_path() -> PathBuf {
    repository_root().join("examples/tabletop-adapters/catalog/compatibility-matrix.json")
}

/// Minimal project metadata needed to link a canonical project from the catalog.
#[derive(Debug, Deserialize)]
struct ProjectMetadata {
    project_format_version: u32,
    adapter_id: String,
    adapter_version: String,
    schema_version: u32,
    seed: u64,
}

fn project_directory(manifest: &AdapterManifest) -> String {
    format!(
        "examples/tabletop-adapters/{}/project",
        manifest.namespace.replace('_', "-")
    )
}

fn project_command(manifest: &AdapterManifest) -> String {
    format!(
        "cargo test -p weave-tabletop --test compatibility -- --exact canonical_project_{}",
        manifest.namespace
    )
}

fn canonical_project(manifest: &AdapterManifest) -> CatalogProject {
    let directory = project_directory(manifest);
    let project_path = repository_root().join(&directory).join("project.json");
    let source = fs::read_to_string(&project_path).unwrap_or_else(|error| {
        panic!(
            "adapter `{}` has no canonical project at `{}`: {error}",
            manifest.id,
            project_path.display()
        )
    });
    let metadata: ProjectMetadata = weave_domain::parse_strict_json(&source)
        .unwrap_or_else(|error| panic!("could not parse `{}`: {error}", project_path.display()));
    assert_eq!(
        metadata.project_format_version, 1,
        "canonical project for `{}` uses an unsupported format version",
        manifest.id
    );
    assert_eq!(
        metadata.adapter_id, manifest.id,
        "canonical project directory `{directory}` selects adapter `{}` but the manifest is `{}`",
        metadata.adapter_id, manifest.id
    );
    assert_eq!(
        metadata.adapter_version, manifest.version,
        "canonical project for `{}` selects version `{}` but the manifest is `{}`",
        manifest.id, metadata.adapter_version, manifest.version
    );
    assert_eq!(
        metadata.schema_version, manifest.schema_version,
        "canonical project for `{}` selects schema version `{}` but the manifest is `{}`",
        manifest.id, metadata.schema_version, manifest.schema_version
    );

    let expected_output = format!("{directory}/expected-output.json");
    let expected_path = repository_root().join(&expected_output);
    let expected_bytes = fs::read(&expected_path).unwrap_or_else(|error| {
        panic!(
            "canonical project for `{}` has no expected output at `{}`: {error}",
            manifest.id,
            expected_path.display()
        )
    });

    CatalogProject {
        path: directory,
        command: project_command(manifest),
        expected_output,
        expected_output_sha256: sha256_bytes(&expected_bytes),
        seed: metadata.seed,
    }
}

/// Machine-readable compatibility and migration matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatibilityMatrix {
    matrix_format_version: u32,
    conversion_policy: String,
    migration_policy: String,
    adapters: Vec<MatrixAdapter>,
    cross_system_pairs: Vec<CrossSystemPair>,
    migrations: Vec<MatrixMigration>,
}

/// One adapter row in the compatibility matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatrixAdapter {
    id: String,
    version: String,
    schema_version: u32,
    namespace: String,
    title: String,
    license: String,
    source_class: String,
    compatibility_label: String,
    weave_version: String,
    character_contract_version: String,
    capabilities: Vec<String>,
    creation_steps: Vec<String>,
    operations: Vec<String>,
    migrations: Vec<String>,
    source_sha256: String,
    required_license_text: CatalogFile,
    required_notices: Vec<String>,
    forbidden_files: Vec<String>,
    canonical_project: CatalogProject,
    /// Exact content hash of the validated manifest.
    content_sha256: String,
}

/// One directed cross-system pair and the data that does not transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CrossSystemPair {
    source: String,
    target: String,
    conversion_allowed: bool,
    requires_separately_approved_migration: bool,
    categories: Vec<NonConversionCategory>,
}

/// Explicit non-conversion statement for one data category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NonConversionCategory {
    category: String,
    transfers: bool,
    source_capabilities: Vec<String>,
    note: String,
}

/// One declared same-adapter migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatrixMigration {
    adapter_id: String,
    id: String,
    from_adapter_id: String,
    from_version: String,
    from_schema_version: u32,
    to_schema_version: u32,
    reviewed: bool,
    preserves_canonical_character: bool,
    description: String,
}

/// Data categories that never convert across adapters.
const NON_CONVERSION_CATEGORIES: &[(&str, &[TabletopCapability], &str)] = &[
    (
        "attributes",
        &[
            TabletopCapability::CharacterCreation,
            TabletopCapability::DerivedValues,
        ],
        "attributes, ratings, and derived values",
    ),
    (
        "items",
        &[TabletopCapability::EquipmentAndAbilities],
        "equipment, inventory, abilities, and moves",
    ),
    (
        "resources",
        &[TabletopCapability::ResourcesAndConditions],
        "resource tracks, harm, stress, and conditions",
    ),
    (
        "progression",
        &[TabletopCapability::Advancement],
        "experience and advancement",
    ),
    (
        "campaign_state",
        &[TabletopCapability::CampaignState],
        "campaign, faction, clock, scene, and world state",
    ),
];

const CONVERSION_POLICY: &str = "Cross-system conversion is not supported and is never implied by a shared field name, die shape, or system label. Moving data between adapters requires a separately approved migration; this build declares none. Without one, every adapter definition and state is archived unchanged under its own namespace.";
const MIGRATION_POLICY: &str = "Only a same-adapter release change may declare a migration. A migration must set reviewed and preserves_canonical_character to true, match the source adapter id, version requirement, and schema version, and pass a dry run against the target schema. A cross-system switch is never a migration.";

fn category_capabilities(
    manifest: &AdapterManifest,
    capabilities: &[TabletopCapability],
) -> Vec<String> {
    manifest
        .capabilities
        .iter()
        .filter(|declaration| capabilities.contains(&declaration.capability))
        .map(|declaration| capability_id(declaration.capability).to_owned())
        .collect()
}

fn non_conversion_note(
    source: &AdapterManifest,
    target: &AdapterManifest,
    surface: &str,
    capabilities: &[String],
) -> String {
    if capabilities.is_empty() {
        format!(
            "`{}` declares no {} capability, so there is nothing to transfer to `{}`.",
            source.id, surface, target.id
        )
    } else {
        format!(
            "`{}` owns {} inside the `{}` namespace through [{}]. None of it transfers to `{}`; the outgoing definition and state are archived unchanged, and `{}` starts from its own {}.",
            source.id,
            surface,
            source.namespace,
            capabilities.join(", "),
            target.id,
            target.id,
            surface
        )
    }
}

fn matrix_migration(adapter_id: &str, migration: &AdapterMigrationDeclaration) -> MatrixMigration {
    MatrixMigration {
        adapter_id: adapter_id.to_owned(),
        id: migration.id.clone(),
        from_adapter_id: migration.from_adapter_id.clone(),
        from_version: migration.from_version.clone(),
        from_schema_version: migration.from_schema_version,
        to_schema_version: migration.to_schema_version,
        reviewed: migration.reviewed,
        preserves_canonical_character: migration.preserves_canonical_character,
        description: migration.description.clone(),
    }
}

/// Build the matrix from the live manifests, policies, and canonical project files.
fn live_matrix() -> CompatibilityMatrix {
    let manifests = live_manifests();
    let policy = standard_source_policy();
    let manifest_ids = manifests
        .iter()
        .map(|manifest| manifest.id.as_str())
        .collect::<BTreeSet<_>>();
    for entry in policy.entries() {
        assert!(
            manifest_ids.contains(entry.adapter_id),
            "source policy names adapter `{}` but no shipped manifest matches it",
            entry.adapter_id
        );
    }

    let adapters = manifests
        .iter()
        .map(|manifest| {
            let entry = policy.entry(&manifest.id).unwrap_or_else(|| {
                panic!(
                    "adapter `{}` has a manifest but no reviewed source-policy row",
                    manifest.id
                )
            });
            assert_eq!(
                entry.license, manifest.provenance.license,
                "policy licence for `{}` disagrees with the manifest provenance",
                manifest.id
            );
            assert_eq!(
                entry.source_class, manifest.provenance.source_class,
                "policy source class for `{}` disagrees with the manifest provenance",
                manifest.id
            );
            assert_eq!(
                entry.source_sha256, manifest.provenance.sha256,
                "policy source digest for `{}` disagrees with the manifest provenance",
                manifest.id
            );

            MatrixAdapter {
                id: manifest.id.clone(),
                version: manifest.version.clone(),
                schema_version: manifest.schema_version,
                namespace: manifest.namespace.clone(),
                title: manifest.title.clone(),
                license: entry.license.to_owned(),
                source_class: source_class_id(entry.source_class).to_owned(),
                compatibility_label: manifest.compatibility_label.clone(),
                weave_version: manifest.weave_version.clone(),
                character_contract_version: manifest.character_contract_version.clone(),
                capabilities: manifest
                    .capabilities
                    .iter()
                    .map(|declaration| capability_id(declaration.capability).to_owned())
                    .collect(),
                creation_steps: manifest
                    .creation_steps
                    .iter()
                    .map(|step| step.id.clone())
                    .collect(),
                operations: manifest
                    .operations
                    .values()
                    .map(|operation| operation.id.clone())
                    .collect(),
                migrations: manifest
                    .migrations
                    .iter()
                    .map(|migration| migration.id.clone())
                    .collect(),
                source_sha256: entry.source_sha256.to_owned(),
                required_license_text: policy_file(entry.required_license_text),
                required_notices: entry
                    .required_notices
                    .iter()
                    .map(|notice| (*notice).to_owned())
                    .collect(),
                forbidden_files: entry
                    .forbidden_files
                    .iter()
                    .map(|file| (*file).to_owned())
                    .collect(),
                canonical_project: canonical_project(manifest),
                content_sha256: resolved_adapter(manifest)
                    .expect("a shipped manifest resolves to its exact coordinate")
                    .content_sha256,
            }
        })
        .collect::<Vec<_>>();

    let mut cross_system_pairs = Vec::new();
    for source in &manifests {
        for target in &manifests {
            if source.id == target.id {
                continue;
            }
            let categories = NON_CONVERSION_CATEGORIES
                .iter()
                .map(|(category, capabilities, surface)| {
                    let source_capabilities = category_capabilities(source, capabilities);
                    NonConversionCategory {
                        category: (*category).to_owned(),
                        transfers: false,
                        note: non_conversion_note(source, target, surface, &source_capabilities),
                        source_capabilities,
                    }
                })
                .collect();
            cross_system_pairs.push(CrossSystemPair {
                source: source.id.clone(),
                target: target.id.clone(),
                conversion_allowed: false,
                requires_separately_approved_migration: true,
                categories,
            });
        }
    }

    let migrations = manifests
        .iter()
        .flat_map(|manifest| {
            manifest
                .migrations
                .iter()
                .map(|migration| matrix_migration(&manifest.id, migration))
        })
        .collect::<Vec<_>>();

    CompatibilityMatrix {
        matrix_format_version: COMPATIBILITY_MATRIX_FORMAT_VERSION,
        conversion_policy: CONVERSION_POLICY.to_owned(),
        migration_policy: MIGRATION_POLICY.to_owned(),
        adapters,
        cross_system_pairs,
        migrations,
    }
}

fn read_checked_in_matrix() -> CompatibilityMatrix {
    let path = matrix_path();
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read `{}`: {error}", path.display()));
    weave_domain::parse_strict_json(&source)
        .unwrap_or_else(|error| panic!("could not parse `{}`: {error}", path.display()))
}

fn read_checked_in_catalog() -> Catalog {
    let path = catalog_path();
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read `{}`: {error}", path.display()));
    weave_domain::parse_strict_json(&source)
        .unwrap_or_else(|error| panic!("could not parse `{}`: {error}", path.display()))
}

#[test]
fn checked_in_catalog_matches_live_manifests() {
    let expected = live_catalog();
    let actual = read_checked_in_catalog();

    assert_eq!(
        expected.catalog_format_version, actual.catalog_format_version,
        "catalog format version is stale; regenerate the catalog"
    );
    let expected_ids = expected
        .adapters
        .iter()
        .map(|adapter| adapter.id.as_str())
        .collect::<Vec<_>>();
    let actual_ids = actual
        .adapters
        .iter()
        .map(|adapter| adapter.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        expected_ids, actual_ids,
        "catalog adapter set or order is stale; regenerate the catalog"
    );

    for (expected, actual) in expected.adapters.iter().zip(&actual.adapters) {
        assert_eq!(
            expected, actual,
            "catalog entry `{}` is stale; regenerate the catalog with `WEAVE_WRITE_ADAPTER_CATALOG=1 cargo test -p weave-tabletop --test catalog -- --ignored`",
            expected.id
        );
    }
}

#[test]
fn catalog_lists_every_shipped_adapter() {
    let catalog = live_catalog();

    assert_eq!(catalog.catalog_format_version, CATALOG_FORMAT_VERSION);
    assert_eq!(
        catalog
            .adapters
            .iter()
            .map(|adapter| adapter.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "org.weave.tabletop.plug_and_play",
            "org.weave.tabletop.dungeonpunk",
            "org.weave.tabletop.freehack",
            "org.weave.tabletop.wicked_ones",
            "org.weave.tabletop.spark",
            "org.weave.tabletop.open_adventure",
            "org.weave.tabletop.open_tale",
        ]
    );

    for adapter in &catalog.adapters {
        assert_eq!(
            adapter.conformance.status, "standard_suite_registered",
            "adapter `{}` is not registered in the standard conformance suite",
            adapter.id
        );
        assert_eq!(
            adapter.conformance.checks.len(),
            ConformanceCheck::ALL.len(),
            "adapter `{}` does not list every standard conformance check",
            adapter.id
        );
        assert!(!adapter.capabilities.is_empty());
        assert!(!adapter.creation_steps.is_empty());
        assert!(!adapter.operations.is_empty());
        assert_eq!(adapter.source.sha256, adapter.source.policy.source_sha256);
        assert_eq!(adapter.source.license, adapter.source.policy.license);
        assert_eq!(
            adapter.source.source_class,
            adapter.source.policy.source_class
        );
        assert_eq!(adapter.namespace, adapter.source.policy.namespace);
    }
}

#[test]
fn checked_in_compatibility_matrix_matches_live_manifests() {
    let expected = live_matrix();
    let actual = read_checked_in_matrix();

    assert_eq!(
        expected.matrix_format_version, actual.matrix_format_version,
        "compatibility matrix format version is stale; regenerate the matrix"
    );
    assert_eq!(
        expected, actual,
        "compatibility matrix is stale; regenerate it with `WEAVE_WRITE_ADAPTER_CATALOG=1 cargo test -p weave-tabletop --test catalog -- --ignored`"
    );
}

#[test]
fn compatibility_matrix_names_every_cross_system_non_conversion() {
    let matrix = live_matrix();
    let adapter_ids = matrix
        .adapters
        .iter()
        .map(|adapter| adapter.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        matrix.cross_system_pairs.len(),
        adapter_ids.len() * (adapter_ids.len() - 1),
        "every directed cross-system pair must have one row"
    );
    for pair in &matrix.cross_system_pairs {
        assert_ne!(pair.source, pair.target);
        assert!(!pair.conversion_allowed);
        assert!(pair.requires_separately_approved_migration);
        assert_eq!(
            pair.categories
                .iter()
                .map(|category| category.category.as_str())
                .collect::<Vec<_>>(),
            vec![
                "attributes",
                "items",
                "resources",
                "progression",
                "campaign_state"
            ],
            "pair `{}` -> `{}` must name every non-conversion category",
            pair.source,
            pair.target
        );
        for category in &pair.categories {
            assert!(
                !category.transfers,
                "pair `{}` -> `{}` must never transfer `{}`",
                pair.source, pair.target, category.category
            );
            assert!(
                !category.note.is_empty(),
                "pair `{}` -> `{}` must state what does not transfer for `{}`",
                pair.source,
                pair.target,
                category.category
            );
        }
    }
}

#[test]
fn upstream_snapshot_worked_example_matches_its_synthetic_source() {
    let directory =
        repository_root().join("examples/tabletop-adapters/catalog/upstream-snapshot-example");
    let source = fs::read(directory.join("source.txt"))
        .expect("the synthetic worked-example source is checked in");
    let record_source = fs::read_to_string(directory.join("record.json"))
        .expect("the synthetic worked-example record is checked in");
    let record: serde_json::Value = weave_domain::parse_strict_json(&record_source)
        .expect("the synthetic worked-example record is strict JSON");

    assert_eq!(record["snapshot_format_version"].as_u64(), Some(1));
    assert_eq!(
        record["adapter_id"].as_str(),
        Some("org.weave.tabletop.example_snapshot")
    );
    assert!(
        record["source_url"]
            .as_str()
            .is_some_and(|url| url.starts_with("https://example.org/")),
        "the worked example must name a reserved synthetic source"
    );
    assert_eq!(record["source_class"].as_str(), Some("verified_cc0"));
    assert_eq!(record["license"].as_str(), Some("CC0-1.0"));

    let artifacts = record["artifacts"]
        .as_array()
        .expect("the record lists its artifacts");
    let primary = artifacts
        .iter()
        .find(|artifact| artifact["exact_artifact"].as_str() == Some("synthetic-rules.txt"))
        .expect("the record pins the synthetic primary source");
    let digest = sha256_bytes(&source);
    assert_eq!(
        primary["sha256"].as_str(),
        Some(digest.as_str()),
        "the worked-example digest must match the exact checked-in source bytes"
    );
    assert_eq!(primary["redistributed"].as_bool(), Some(false));

    assert_eq!(
        record["review"]["checklist"]["digest_matches_exact_bytes"].as_bool(),
        Some(true)
    );
    assert_eq!(
        record["review"]["checklist"]["no_build_network_dependency"].as_bool(),
        Some(true)
    );
}

#[test]
#[ignore = "rewrites the checked-in adapter catalog and compatibility matrix"]
fn regenerate_checked_in_catalog() {
    assert!(
        std::env::var_os("WEAVE_WRITE_ADAPTER_CATALOG").is_some(),
        "set WEAVE_WRITE_ADAPTER_CATALOG=1 to rewrite the checked-in adapter catalog and matrix"
    );

    let catalog = live_catalog();
    let matrix = live_matrix();
    write_json(&catalog_path(), &catalog);
    write_json(&matrix_path(), &matrix);
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    let mut json = serde_json::to_string_pretty(value).expect("catalog data serializes as JSON");
    json.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("could not create `{}`: {error}", parent.display()));
    }
    fs::write(path, json)
        .unwrap_or_else(|error| panic!("could not write `{}`: {error}", path.display()));
    println!("wrote {}", path.display());
}
