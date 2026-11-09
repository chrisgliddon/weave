//! Canonical synthetic projects, one per shipped adapter.
//!
//! Each project under `examples/tabletop-adapters/<adapter>/project/` selects one exact adapter
//! coordinate, records a deterministic seed and scripted operation path, and checks in the
//! expected creation and resolution output. [`check_project`] compiles nothing at run time and
//! performs no network access: it creates the fixture, executes the scripted path through the
//! registry, replays every receipt from its recorded before-state, and compares the result with
//! `expected-output.json`.
//!
//! Regenerate every project deliberately with:
//!
//! ```bash
//! WEAVE_WRITE_COMPATIBILITY_PROJECTS=1 cargo test -p weave-tabletop --test compatibility
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use weave_domain::DomainValue;
use weave_tabletop::conformance::{
    CONFORMANCE_SEED, ConformanceAdapter, DungeonpunkConformance, FreehackConformance,
    OpenAdventureConformance, OpenTaleConformance, PlugAndPlayConformance, SparkConformance,
    WickedOnesConformance,
};
use weave_tabletop::{
    RESOLVER_FORMAT_VERSION, ResolutionRequest, ResolverRegistry, TabletopCapability,
    canonical_fingerprint, resolved_adapter, sha256_bytes, validate_adapter_manifest,
};

const PROJECT_FORMAT_VERSION: u32 = 1;
const PROJECT_SOURCE_CLASS: &str = "synthetic_original";
const PROJECT_LICENSE: &str = "MIT";
const PROJECT_PROVENANCE: &str = "Original synthetic project authored for Weave. Names, prose, settings, and scenario inputs are invented; no upstream rules text, artwork, layout, or asset is copied.";

const REGENERATE_ENV: &str = "WEAVE_WRITE_COMPATIBILITY_PROJECTS";

/// One canonical project selection and its source declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    project_format_version: u32,
    adapter_id: String,
    adapter_version: String,
    schema_version: u32,
    seed: u64,
    source: ProjectSource,
    manifest: ManifestSelection,
    scripted_path: Vec<ProjectOperation>,
}

/// Licence and provenance of the synthetic project material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSource {
    source_class: String,
    license: String,
    provenance: String,
}

/// Exact installed coordinate the project selects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestSelection {
    adapter_id: String,
    version: String,
    schema_version: u32,
    source: String,
}

/// One operation in the checked-in scripted path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectOperation {
    request_id: String,
    operation: String,
    capability: TabletopCapability,
    input: DomainValue,
}

/// Checked-in deterministic output for one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedOutput {
    project_format_version: u32,
    adapter_id: String,
    adapter_version: String,
    schema_version: u32,
    seed: u64,
    manifest_content_sha256: String,
    creation_preview_sha256: String,
    definition_sha256: String,
    initial_state_sha256: String,
    steps: Vec<ExpectedStep>,
    final_state_sha256: String,
    replay: String,
}

/// Expected output for one scripted operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedStep {
    request_id: String,
    operation: String,
    capability: TabletopCapability,
    request_sha256: String,
    before_state_sha256: String,
    receipt_sha256: String,
    after_state_sha256: String,
    revision: u64,
    entropy_consumed: u64,
    event_kinds: Vec<String>,
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn project_dir(relative: &str) -> PathBuf {
    repository_root().join(relative)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read `{}`: {error}", path.display()));
    weave_domain::parse_strict_json(&source)
        .unwrap_or_else(|error| panic!("could not parse `{}`: {error}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    let mut json = serde_json::to_string_pretty(value).expect("project data serializes as JSON");
    json.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("could not create `{}`: {error}", parent.display()));
    }
    fs::write(path, json)
        .unwrap_or_else(|error| panic!("could not write `{}`: {error}", path.display()));
}

fn build_project(
    manifest: &weave_tabletop::AdapterManifest,
    adapter: &dyn ConformanceAdapter,
) -> Project {
    assert_eq!(
        adapter.adapter_id(),
        manifest.id,
        "descriptor identity does not match its live manifest"
    );
    let creation = adapter
        .create(CONFORMANCE_SEED)
        .expect("the canonical creation fixture must succeed");
    let scripted_path = adapter
        .scenario(&creation)
        .expect("the canonical scripted path must build")
        .into_iter()
        .map(|operation| ProjectOperation {
            request_id: operation.request_id,
            operation: operation.operation,
            capability: operation.capability,
            input: operation.input,
        })
        .collect();

    Project {
        project_format_version: PROJECT_FORMAT_VERSION,
        adapter_id: manifest.id.clone(),
        adapter_version: manifest.version.clone(),
        schema_version: manifest.schema_version,
        seed: CONFORMANCE_SEED,
        source: ProjectSource {
            source_class: PROJECT_SOURCE_CLASS.to_owned(),
            license: PROJECT_LICENSE.to_owned(),
            provenance: PROJECT_PROVENANCE.to_owned(),
        },
        manifest: ManifestSelection {
            adapter_id: manifest.id.clone(),
            version: manifest.version.clone(),
            schema_version: manifest.schema_version,
            source: "live adapter manifest selected by exact coordinate".to_owned(),
        },
        scripted_path,
    }
}

fn run_project(
    adapter: &dyn ConformanceAdapter,
    manifest: &weave_tabletop::AdapterManifest,
    project: &Project,
) -> ExpectedOutput {
    assert_eq!(
        project.project_format_version, PROJECT_FORMAT_VERSION,
        "project format version is stale"
    );
    assert_eq!(
        project.adapter_id, manifest.id,
        "project selects adapter `{}` but the live manifest is `{}`",
        project.adapter_id, manifest.id
    );
    assert_eq!(
        project.adapter_version, manifest.version,
        "project selects version `{}` but the live manifest is `{}`",
        project.adapter_version, manifest.version
    );
    assert_eq!(
        project.schema_version, manifest.schema_version,
        "project selects schema version `{}` but the live manifest is `{}`",
        project.schema_version, manifest.schema_version
    );
    assert_eq!(
        project.manifest,
        ManifestSelection {
            adapter_id: manifest.id.clone(),
            version: manifest.version.clone(),
            schema_version: manifest.schema_version,
            source: project.manifest.source.clone(),
        },
        "project manifest selection does not match the live manifest"
    );
    assert_eq!(
        project.source.source_class, PROJECT_SOURCE_CLASS,
        "project source class must stay synthetic_original"
    );
    assert_eq!(
        project.source.license, PROJECT_LICENSE,
        "synthetic project material uses the repository MIT licence"
    );
    assert!(
        !project.scripted_path.is_empty(),
        "the canonical scripted path must contain at least one operation"
    );

    let resolved = resolved_adapter(manifest).expect("live manifest resolves");
    let creation = adapter
        .create(project.seed)
        .unwrap_or_else(|error| panic!("creation for seed {} failed: {error}", project.seed));
    let repeated = adapter
        .create(project.seed)
        .expect("repeat creation must succeed");
    assert_eq!(
        creation.canonical, repeated.canonical,
        "creation from seed {} is not reproducible",
        project.seed
    );
    assert_eq!(
        creation.adapter, resolved,
        "creation coordinate does not match the selected manifest"
    );

    let mut registry = ResolverRegistry::new();
    registry
        .register(manifest, adapter.resolver())
        .expect("the selected resolver registers for the selected manifest");

    let mut state = creation.initial_state.clone();
    let initial_state_sha256 =
        canonical_fingerprint(&state).expect("initial state serializes for fingerprint");
    let mut steps = Vec::with_capacity(project.scripted_path.len());

    for operation in &project.scripted_path {
        let declaration = manifest
            .operations
            .get(&operation.operation)
            .unwrap_or_else(|| {
                panic!(
                    "scripted operation `{}` is not declared by `{}`",
                    operation.operation, manifest.id
                )
            });
        assert_eq!(
            declaration.required_capability, operation.capability,
            "scripted operation `{}` names capability `{:?}` but the manifest requires `{:?}`",
            operation.operation, operation.capability, declaration.required_capability
        );

        let before_state_sha256 =
            canonical_fingerprint(&state).expect("before-state serializes for fingerprint");
        let request = ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: operation.request_id.clone(),
            adapter: creation.adapter.clone(),
            operation: operation.operation.clone(),
            capability: operation.capability,
            definition_sha256: creation.definition.definition_sha256.clone(),
            definition: creation.definition.definition.clone(),
            expected_state_sha256: before_state_sha256.clone(),
            input: operation.input.clone(),
        };
        let receipt = registry
            .execute(manifest, &request, &state)
            .unwrap_or_else(|error| {
                panic!(
                    "operation `{}` was rejected during the scripted path: {error}",
                    operation.operation
                )
            });
        registry
            .replay(manifest, &request, &state, &receipt)
            .unwrap_or_else(|error| {
                panic!(
                    "operation `{}` did not replay from its recorded before-state: {error}",
                    operation.operation
                )
            });
        assert_eq!(
            receipt.after_state.revision,
            state.revision + 1,
            "operation `{}` did not advance the revision exactly once",
            operation.operation
        );

        steps.push(ExpectedStep {
            request_id: operation.request_id.clone(),
            operation: operation.operation.clone(),
            capability: operation.capability,
            request_sha256: receipt.request_sha256.clone(),
            before_state_sha256,
            receipt_sha256: canonical_fingerprint(&receipt)
                .expect("receipt serializes for fingerprint"),
            after_state_sha256: receipt.after_state_sha256.clone(),
            revision: receipt.after_state.revision,
            entropy_consumed: receipt.entropy_consumed,
            event_kinds: receipt
                .events
                .iter()
                .map(|event| event.kind.clone())
                .collect(),
        });

        state = receipt.after_state;
    }

    ExpectedOutput {
        project_format_version: PROJECT_FORMAT_VERSION,
        adapter_id: manifest.id.clone(),
        adapter_version: manifest.version.clone(),
        schema_version: manifest.schema_version,
        seed: project.seed,
        manifest_content_sha256: resolved.content_sha256,
        creation_preview_sha256: sha256_bytes(creation.canonical.as_bytes()),
        definition_sha256: creation.definition.definition_sha256,
        initial_state_sha256,
        steps,
        final_state_sha256: canonical_fingerprint(&state)
            .expect("final state serializes for fingerprint"),
        replay: "exact".to_owned(),
    }
}

/// Run one canonical project, regenerating its checked-in data only when explicitly requested.
fn check_project(relative_dir: &str, adapter: &dyn ConformanceAdapter) {
    let directory = project_dir(relative_dir);
    let manifest = adapter.manifest();
    validate_adapter_manifest(&manifest).expect("the live manifest satisfies the adapter contract");
    let regenerate = std::env::var_os(REGENERATE_ENV).is_some();

    let project_path = directory.join("project.json");
    let expected_path = directory.join("expected-output.json");
    let project = if regenerate {
        let project = build_project(&manifest, adapter);
        write_json(&project_path, &project);
        project
    } else {
        read_json(&project_path)
    };

    let expected = run_project(adapter, &manifest, &project);
    if regenerate {
        write_json(&expected_path, &expected);
    } else {
        let checked_in: ExpectedOutput = read_json(&expected_path);
        assert_eq!(
            expected, checked_in,
            "expected output for `{}` is stale; regenerate with `{REGENERATE_ENV}=1 cargo test -p weave-tabletop --test compatibility`",
            project.adapter_id
        );
    }
}

#[test]
fn canonical_project_plug_and_play() {
    check_project(
        "examples/tabletop-adapters/plug-and-play/project",
        &PlugAndPlayConformance,
    );
}

#[test]
fn canonical_project_dungeonpunk() {
    check_project(
        "examples/tabletop-adapters/dungeonpunk/project",
        &DungeonpunkConformance,
    );
}

#[test]
fn canonical_project_freehack() {
    check_project(
        "examples/tabletop-adapters/freehack/project",
        &FreehackConformance,
    );
}

#[test]
fn canonical_project_wicked_ones() {
    check_project(
        "examples/tabletop-adapters/wicked-ones/project",
        &WickedOnesConformance,
    );
}

#[test]
fn canonical_project_spark() {
    check_project(
        "examples/tabletop-adapters/spark/project",
        &SparkConformance,
    );
}

#[test]
fn canonical_project_open_adventure() {
    check_project(
        "examples/tabletop-adapters/open-adventure/project",
        &OpenAdventureConformance,
    );
}

#[test]
fn canonical_project_open_tale() {
    check_project(
        "examples/tabletop-adapters/open-tale/project",
        &OpenTaleConformance,
    );
}
