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
    AdapterManifest, AdapterProvenance, AdapterSourceClass, AdapterSourcePolicy, ExcludedMaterial,
    PolicyFile, TabletopCapability, dungeonpunk_manifest, freehack_manifest,
    open_adventure_manifest, open_tale_manifest, plug_and_play_manifest, spark_manifest,
    wicked_ones_manifest,
};

const CATALOG_FORMAT_VERSION: u32 = 1;

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
            catalog_adapter(manifest, entry, &registered)
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

fn catalog_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/tabletop-adapters/catalog/catalog.json")
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
#[ignore = "rewrites examples/tabletop-adapters/catalog/catalog.json"]
fn regenerate_checked_in_catalog() {
    assert!(
        std::env::var_os("WEAVE_WRITE_ADAPTER_CATALOG").is_some(),
        "set WEAVE_WRITE_ADAPTER_CATALOG=1 to rewrite the checked-in adapter catalog"
    );

    let catalog = live_catalog();
    let mut json = serde_json::to_string_pretty(&catalog).expect("catalog serializes as JSON");
    json.push('\n');
    let path = catalog_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("could not create `{}`: {error}", parent.display()));
    }
    fs::write(&path, json)
        .unwrap_or_else(|error| panic!("could not write `{}`: {error}", path.display()));
    println!("wrote {}", path.display());
}
