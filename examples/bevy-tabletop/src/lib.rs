//! Bevy consumer for every checked-in tabletop adapter fixture.
//!
//! Two checked-in shapes exist:
//!
//! - Full fixture trees for Plug-And-Play, Dungeonpunk, Freehack, and Open Tale: manifest,
//!   creation request and preview, initial state, ordered playthrough requests and receipts, and
//!   final state.
//! - Canonical synthetic projects for Spark, Wicked Ones, and Open Adventure: a seed, a scripted
//!   operation path, and expected creation, request, receipt, state, and event data.
//!
//! For fixture trees the consumer reads the recorded requests and receipts. For canonical projects
//! it creates deterministically from the recorded seed through the public conformance adapter,
//! builds each request, and compares every generated hash and event kind with the checked-in
//! expected output. Both paths execute only through `weave-tabletop`'s trusted resolvers.
//!
//! Fixtures are synthetic and offline; no artwork, prose, or network access is involved.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use weave_domain::DomainValue;
use weave_tabletop::conformance::{
    ConformanceAdapter, OpenAdventureConformance, SparkConformance, WickedOnesConformance,
};
use weave_tabletop::{
    AdapterManifest, DungeonpunkCreationPreview, DungeonpunkCreationRequest, DungeonpunkResolver,
    EventVisibility, FreehackAuthorityReceipt, FreehackCreationPreview, FreehackCreationRequest,
    FreehackResolver, HostAudience, OpenTaleCreationPreview, OpenTaleCreationRequest,
    OpenTaleResolver, PlugAndPlayCreationPreview, PlugAndPlayCreationRequest, PlugAndPlayResolver,
    RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest, ResolverRegistry,
    TabletopCapability, TabletopState, canonical_fingerprint, create_dungeonpunk_character,
    create_freehack_character, create_open_tale_character, create_plug_and_play_character,
    dungeonpunk_manifest, freehack_manifest, open_tale_manifest, plug_and_play_manifest,
    project_freehack_public_state, project_receipt_for_audience, resolved_adapter, sha256_bytes,
    validate_adapter_manifest, validate_dungeonpunk_creation_preview,
    validate_freehack_authority_receipt, validate_freehack_creation_preview,
    validate_open_tale_creation_preview, validate_plug_and_play_creation_preview,
};

/// Every adapter with a checked-in fixture tree or canonical project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adapter {
    PlugAndPlay,
    Dungeonpunk,
    Freehack,
    OpenTale,
    Spark,
    WickedOnes,
    OpenAdventure,
}

impl Adapter {
    /// Every adapter with checked-in replay data.
    pub const ALL: [Self; 7] = [
        Self::PlugAndPlay,
        Self::Dungeonpunk,
        Self::Freehack,
        Self::OpenTale,
        Self::Spark,
        Self::WickedOnes,
        Self::OpenAdventure,
    ];

    /// Directory name under `examples/tabletop-adapters`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::PlugAndPlay => "plug-and-play",
            Self::Dungeonpunk => "dungeonpunk",
            Self::Freehack => "freehack",
            Self::OpenTale => "open-tale",
            Self::Spark => "spark",
            Self::WickedOnes => "wicked-ones",
            Self::OpenAdventure => "open-adventure",
        }
    }

    /// The live Rust manifest for this adapter.
    #[must_use]
    pub fn manifest(self) -> AdapterManifest {
        match self {
            Self::PlugAndPlay => plug_and_play_manifest(),
            Self::Dungeonpunk => dungeonpunk_manifest(),
            Self::Freehack => freehack_manifest(),
            Self::OpenTale => open_tale_manifest(),
            Self::Spark => SparkConformance.manifest(),
            Self::WickedOnes => WickedOnesConformance.manifest(),
            Self::OpenAdventure => OpenAdventureConformance.manifest(),
        }
    }

    const fn spec(self) -> Option<AdapterSpec> {
        match self {
            Self::PlugAndPlay => Some(AdapterSpec {
                manifest_file: "plug-and-play.tabletop-adapter.json",
                initial_state_file: "state.tabletop-state.json",
                final_state_file: "final-state.tabletop-state.json",
                receipt_suffix: ".tabletop-receipt.json",
                index_file: None,
            }),
            Self::Dungeonpunk => Some(AdapterSpec {
                manifest_file: "dungeonpunk.tabletop-adapter.json",
                initial_state_file: "state.tabletop-state.json",
                final_state_file: "final-state.tabletop-state.json",
                receipt_suffix: ".tabletop-receipt.json",
                index_file: None,
            }),
            Self::Freehack => Some(AdapterSpec {
                manifest_file: "freehack.tabletop-adapter.json",
                initial_state_file: "authority-state.tabletop-state.json",
                final_state_file: "final-authority-state.tabletop-state.json",
                receipt_suffix: ".authority-receipt.json",
                index_file: None,
            }),
            Self::OpenTale => Some(AdapterSpec {
                manifest_file: "open-tale.tabletop-adapter.json",
                initial_state_file: "initial-state.tabletop-state.json",
                final_state_file: "final-state.tabletop-state.json",
                receipt_suffix: ".tabletop-receipt.json",
                index_file: Some("playthrough/index.json"),
            }),
            Self::Spark | Self::WickedOnes | Self::OpenAdventure => None,
        }
    }
}

struct AdapterSpec {
    manifest_file: &'static str,
    initial_state_file: &'static str,
    final_state_file: &'static str,
    receipt_suffix: &'static str,
    index_file: Option<&'static str>,
}

const CREATION_FILE: &str = "creation.tabletop-creation.json";
const PREVIEW_FILE: &str = "preview.tabletop-creation-preview.json";
const PLAYTHROUGH_DIR: &str = "playthrough";
const PROJECT_DIR: &str = "project";
const PROJECT_FILE: &str = "project.json";
const EXPECTED_FILE: &str = "expected-output.json";

/// Where a run's checked-in expectations came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunSource {
    /// A complete creation, state, and playthrough fixture tree.
    FixtureTree,
    /// A canonical synthetic project with a seed, scripted path, and expected hashes.
    CanonicalProject,
}

impl RunSource {
    /// Short label used in reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::FixtureTree => "fixture tree",
            Self::CanonicalProject => "canonical project",
        }
    }
}

/// One replayed step with its request, receipt, and event sequence.
#[derive(Debug)]
pub struct ReplayedStep {
    pub id: String,
    pub operation: String,
    pub request: ResolutionRequest,
    pub receipt: ResolutionReceipt,
    /// Event kinds in receipt order.
    pub event_kinds: Vec<String>,
    /// Event kinds visible to a runtime audience.
    pub public_event_kinds: Vec<String>,
}

/// The complete replayed record for one adapter.
#[derive(Debug)]
pub struct AdapterRun {
    pub adapter: Adapter,
    pub source: RunSource,
    pub manifest: AdapterManifest,
    pub character_name: String,
    pub steps: Vec<ReplayedStep>,
    pub final_state: TabletopState,
    /// Fingerprint of the starting state, matching the PixiJS projection chain.
    pub initial_state_sha256: String,
    /// Fingerprint of the runtime-visible final state, matching the PixiJS projection.
    pub public_final_state_sha256: String,
    /// Flattened scalar leaves of the final adapter state, sorted by path.
    pub observable: Vec<String>,
}

impl AdapterRun {
    /// Total number of events across all receipts.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.steps.iter().map(|step| step.event_kinds.len()).sum()
    }

    /// Number of events a runtime audience may observe.
    #[must_use]
    pub fn public_event_count(&self) -> usize {
        self.steps
            .iter()
            .map(|step| step.public_event_kinds.len())
            .sum()
    }
}

#[derive(Debug, Deserialize)]
struct FixtureIndexStep {
    request_json: String,
    event_kinds: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CanonicalProject {
    project_format_version: u32,
    adapter_id: String,
    adapter_version: String,
    schema_version: u32,
    seed: u64,
    source: ProjectSource,
    manifest: ProjectManifestSelection,
    scripted_path: Vec<ProjectOperation>,
}

#[derive(Debug, Deserialize)]
struct ProjectSource {
    source_class: String,
    license: String,
    #[allow(dead_code)]
    provenance: String,
}

#[derive(Debug, Deserialize)]
struct ProjectManifestSelection {
    adapter_id: String,
    version: String,
    schema_version: u32,
    #[allow(dead_code)]
    source: String,
}

#[derive(Debug, Deserialize)]
struct ProjectOperation {
    request_id: String,
    operation: String,
    capability: TabletopCapability,
    input: DomainValue,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

struct CandidateStep {
    id: String,
    request: ResolutionRequest,
    receipt: ResolutionReceipt,
}

/// Replay every checked-in adapter from the `examples/tabletop-adapters` directory.
pub fn replay_all(root: &Path) -> Result<Vec<AdapterRun>, String> {
    Adapter::ALL
        .into_iter()
        .map(|adapter| replay_adapter(adapter, root))
        .collect()
}

/// Replay one adapter from its checked-in fixture tree or canonical project.
pub fn replay_adapter(adapter: Adapter, root: &Path) -> Result<AdapterRun, String> {
    match adapter.spec() {
        Some(spec) => replay_fixture_tree(adapter, &spec, root),
        None => replay_canonical_project(adapter, root),
    }
}

fn replay_fixture_tree(
    adapter: Adapter,
    spec: &AdapterSpec,
    root: &Path,
) -> Result<AdapterRun, String> {
    let directory = root.join(adapter.key());
    let manifest = adapter.manifest();
    validate_adapter_manifest(&manifest)
        .map_err(|error| format!("{}: live manifest is invalid: {error}", adapter.key()))?;

    let checked_manifest =
        AdapterManifest::from_json(&read_text(&directory.join(spec.manifest_file))?)
            .map_err(|error| format!("{}: checked manifest is invalid: {error}", adapter.key()))?;
    if checked_manifest != manifest {
        return Err(format!(
            "{}: checked manifest differs from the live Rust manifest",
            adapter.key()
        ));
    }

    let (character_name, preview_state) = load_creation(adapter, &directory)?;
    let initial_state =
        TabletopState::from_json(&read_text(&directory.join(spec.initial_state_file))?)
            .map_err(|error| format!("{}: initial state is invalid: {error}", adapter.key()))?;
    if initial_state != preview_state {
        return Err(format!(
            "{}: initial state does not match the creation preview",
            adapter.key()
        ));
    }
    let expected_final =
        TabletopState::from_json(&read_text(&directory.join(spec.final_state_file))?)
            .map_err(|error| format!("{}: final state is invalid: {error}", adapter.key()))?;
    let initial_state_sha256 = canonical_fingerprint(&initial_state).map_err(|error| {
        format!(
            "{}: initial state fingerprint failed: {error}",
            adapter.key()
        )
    })?;

    let mut registry = ResolverRegistry::new();
    register_fixture_resolver(adapter, &manifest, &mut registry)?;

    let candidates = load_steps(
        adapter,
        &directory.join(PLAYTHROUGH_DIR),
        spec.receipt_suffix,
    )?;
    let ordered = order_by_receipt_chain(&initial_state, candidates)?;

    let mut state = initial_state;
    let mut steps = Vec::with_capacity(ordered.len());
    for candidate in ordered {
        let expected_before = canonical_fingerprint(&state)
            .map_err(|error| format!("{}: state fingerprint failed: {error}", adapter.key()))?;
        if candidate.request.expected_state_sha256 != expected_before {
            return Err(format!(
                "{}: {} expects a different preceding state",
                adapter.key(),
                candidate.id
            ));
        }
        let generated = registry
            .execute(&manifest, &candidate.request, &state)
            .map_err(|error| {
                format!(
                    "{}: {} failed to execute: {error}",
                    adapter.key(),
                    candidate.id
                )
            })?;
        if generated != candidate.receipt {
            return Err(format!(
                "{}: {} replay does not reproduce the checked-in receipt",
                adapter.key(),
                candidate.id
            ));
        }
        registry
            .replay(&manifest, &candidate.request, &state, &candidate.receipt)
            .map_err(|error| {
                format!(
                    "{}: {} replay rejected: {error}",
                    adapter.key(),
                    candidate.id
                )
            })?;

        steps.push(step_record(
            candidate.id,
            candidate.request,
            candidate.receipt,
        ));
        state = steps
            .last()
            .map(|step| step.receipt.after_state.clone())
            .ok_or_else(|| format!("{}: missing replayed state", adapter.key()))?;
    }

    if state != expected_final {
        return Err(format!(
            "{}: final replayed state does not match the checked-in final state",
            adapter.key()
        ));
    }

    if let Some(index_file) = spec.index_file {
        verify_index(adapter, &directory.join(index_file), &steps)?;
    }

    let public_state = if adapter == Adapter::Freehack {
        Some(
            project_freehack_public_state(&state)
                .map_err(|error| format!("freehack: public state projection failed: {error}"))?,
        )
    } else {
        None
    };
    let public_final_state_sha256 = match &public_state {
        Some(public_state) => canonical_fingerprint(public_state)
            .map_err(|error| format!("freehack: public state fingerprint failed: {error}"))?,
        None => steps
            .last()
            .map(|step| step.receipt.after_state_sha256.clone())
            .ok_or_else(|| format!("{}: no replayed final state", adapter.key()))?,
    };
    let observable = match &public_state {
        Some(public_state) => {
            let json = serde_json::to_value(public_state)
                .map_err(|error| format!("freehack: public state serialization failed: {error}"))?;
            flatten_json(&json)
        }
        None => flatten_value(&state.value),
    };
    Ok(AdapterRun {
        adapter,
        source: RunSource::FixtureTree,
        manifest,
        character_name,
        steps,
        final_state: state,
        initial_state_sha256,
        public_final_state_sha256,
        observable,
    })
}

fn replay_canonical_project(adapter: Adapter, root: &Path) -> Result<AdapterRun, String> {
    let directory = root.join(adapter.key()).join(PROJECT_DIR);
    let project: CanonicalProject =
        serde_json::from_str(&read_text(&directory.join(PROJECT_FILE))?)
            .map_err(|error| format!("{}: project.json is invalid: {error}", adapter.key()))?;
    let expected: ExpectedOutput =
        serde_json::from_str(&read_text(&directory.join(EXPECTED_FILE))?).map_err(|error| {
            format!(
                "{}: expected-output.json is invalid: {error}",
                adapter.key()
            )
        })?;
    let conformance = conformance_adapter(adapter)
        .ok_or_else(|| format!("{}: no conformance adapter", adapter.key()))?;
    let manifest = conformance.manifest();
    validate_adapter_manifest(&manifest)
        .map_err(|error| format!("{}: live manifest is invalid: {error}", adapter.key()))?;

    verify_project_header(adapter, &project, &expected, &manifest)?;

    let creation = conformance
        .create(project.seed)
        .map_err(|error| format!("{}: canonical creation failed: {error}", adapter.key()))?;
    let resolved = resolved_adapter(&manifest)
        .map_err(|error| format!("{}: manifest does not resolve: {error}", adapter.key()))?;
    if resolved.content_sha256 != expected.manifest_content_sha256 {
        return Err(format!(
            "{}: manifest content hash differs from expected output",
            adapter.key()
        ));
    }
    if resolved != creation.adapter {
        return Err(format!(
            "{}: creation coordinate differs from the live manifest",
            adapter.key()
        ));
    }
    if sha256_bytes(creation.canonical.as_bytes()) != expected.creation_preview_sha256 {
        return Err(format!(
            "{}: creation preview hash differs from expected output",
            adapter.key()
        ));
    }
    if creation.definition.definition_sha256 != expected.definition_sha256 {
        return Err(format!(
            "{}: definition hash differs from expected output",
            adapter.key()
        ));
    }
    let mut initial_state_hash =
        canonical_fingerprint(&creation.initial_state).map_err(|error| {
            format!(
                "{}: initial state fingerprint failed: {error}",
                adapter.key()
            )
        })?;
    if initial_state_hash != expected.initial_state_sha256 {
        return Err(format!(
            "{}: initial state hash differs from expected output",
            adapter.key()
        ));
    }

    let generated_scenario = conformance
        .scenario(&creation)
        .map_err(|error| format!("{}: canonical scenario failed: {error}", adapter.key()))?;
    verify_scenario(adapter, &project.scripted_path, &generated_scenario)?;

    let mut registry = ResolverRegistry::new();
    registry
        .register(&manifest, conformance.resolver())
        .map_err(|error| format!("{}: resolver registration failed: {error}", adapter.key()))?;

    let mut state = creation.initial_state;
    let mut steps = Vec::with_capacity(project.scripted_path.len());
    for (index, operation) in project.scripted_path.iter().enumerate() {
        let expected_step = expected
            .steps
            .get(index)
            .ok_or_else(|| format!("{}: expected output is missing a step", adapter.key()))?;
        let request = ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: operation.request_id.clone(),
            adapter: creation.adapter.clone(),
            operation: operation.operation.clone(),
            capability: operation.capability,
            definition_sha256: creation.definition.definition_sha256.clone(),
            definition: creation.definition.definition.clone(),
            expected_state_sha256: initial_state_hash.clone(),
            input: operation.input.clone(),
        };
        let receipt = registry
            .execute(&manifest, &request, &state)
            .map_err(|error| {
                format!(
                    "{}: {} failed to execute: {error}",
                    adapter.key(),
                    operation.request_id
                )
            })?;
        registry
            .replay(&manifest, &request, &state, &receipt)
            .map_err(|error| {
                format!(
                    "{}: {} replay rejected: {error}",
                    adapter.key(),
                    operation.request_id
                )
            })?;
        verify_expected_step(adapter, expected_step, &request, &receipt)?;
        initial_state_hash = receipt.after_state_sha256.clone();
        state = receipt.after_state.clone();
        steps.push(step_record(operation.request_id.clone(), request, receipt));
    }

    let final_state_hash = canonical_fingerprint(&state)
        .map_err(|error| format!("{}: final state fingerprint failed: {error}", adapter.key()))?;
    if final_state_hash != expected.final_state_sha256 {
        return Err(format!(
            "{}: final state hash differs from expected output",
            adapter.key()
        ));
    }

    let character_name = definition_name(&creation.definition.definition)
        .unwrap_or_else(|| adapter.key().to_owned());
    let observable = flatten_value(&state.value);
    Ok(AdapterRun {
        adapter,
        source: RunSource::CanonicalProject,
        manifest,
        character_name,
        steps,
        final_state: state,
        initial_state_sha256: expected.initial_state_sha256,
        public_final_state_sha256: final_state_hash,
        observable,
    })
}

fn conformance_adapter(adapter: Adapter) -> Option<Box<dyn ConformanceAdapter>> {
    match adapter {
        Adapter::Spark => Some(Box::new(SparkConformance)),
        Adapter::WickedOnes => Some(Box::new(WickedOnesConformance)),
        Adapter::OpenAdventure => Some(Box::new(OpenAdventureConformance)),
        _ => None,
    }
}

fn verify_project_header(
    adapter: Adapter,
    project: &CanonicalProject,
    expected: &ExpectedOutput,
    manifest: &AdapterManifest,
) -> Result<(), String> {
    if project.project_format_version != 1 || expected.project_format_version != 1 {
        return Err(format!(
            "{}: unsupported canonical project format",
            adapter.key()
        ));
    }
    if project.adapter_id != manifest.id
        || project.adapter_version != manifest.version
        || project.schema_version != manifest.schema_version
    {
        return Err(format!(
            "{}: project selects a different adapter coordinate",
            adapter.key()
        ));
    }
    if project.manifest.adapter_id != manifest.id
        || project.manifest.version != manifest.version
        || project.manifest.schema_version != manifest.schema_version
    {
        return Err(format!(
            "{}: project manifest selection does not match the live manifest",
            adapter.key()
        ));
    }
    if project.source.source_class != "synthetic_original" || project.source.license != "MIT" {
        return Err(format!(
            "{}: project source is not the reviewed synthetic MIT source",
            adapter.key()
        ));
    }
    if project.seed != expected.seed
        || expected.adapter_id != manifest.id
        || expected.adapter_version != manifest.version
        || expected.schema_version != manifest.schema_version
        || expected.replay != "exact"
    {
        return Err(format!(
            "{}: expected output header is inconsistent",
            adapter.key()
        ));
    }
    if project.scripted_path.is_empty() || project.scripted_path.len() != expected.steps.len() {
        return Err(format!(
            "{}: scripted path and expected steps differ in length",
            adapter.key()
        ));
    }
    Ok(())
}

fn verify_scenario(
    adapter: Adapter,
    checked_in: &[ProjectOperation],
    generated: &[weave_tabletop::conformance::ConformanceOperation],
) -> Result<(), String> {
    if checked_in.len() != generated.len() {
        return Err(format!(
            "{}: checked scripted path has a different length than the live scenario",
            adapter.key()
        ));
    }
    for (checked, live) in checked_in.iter().zip(generated) {
        if checked.request_id != live.request_id
            || checked.operation != live.operation
            || checked.capability != live.capability
            || checked.input != live.input
        {
            return Err(format!(
                "{}: checked scripted path differs from the live scenario at {}",
                adapter.key(),
                checked.request_id
            ));
        }
    }
    Ok(())
}

fn verify_expected_step(
    adapter: Adapter,
    expected: &ExpectedStep,
    request: &ResolutionRequest,
    receipt: &ResolutionReceipt,
) -> Result<(), String> {
    let receipt_hash = canonical_fingerprint(receipt)
        .map_err(|error| format!("{}: receipt fingerprint failed: {error}", adapter.key()))?;
    let request_hash = canonical_fingerprint(request)
        .map_err(|error| format!("{}: request fingerprint failed: {error}", adapter.key()))?;
    let event_kinds = receipt
        .events
        .iter()
        .map(|event| event.kind.clone())
        .collect::<Vec<_>>();
    if expected.request_id != request.request_id
        || expected.operation != request.operation
        || expected.capability != request.capability
        || expected.request_sha256 != request_hash
        || expected.request_sha256 != receipt.request_sha256
        || expected.before_state_sha256 != receipt.before_state_sha256
        || expected.receipt_sha256 != receipt_hash
        || expected.after_state_sha256 != receipt.after_state_sha256
        || expected.revision != receipt.after_state.revision
        || expected.entropy_consumed != receipt.entropy_consumed
        || expected.event_kinds != event_kinds
    {
        return Err(format!(
            "{}: {} does not match the checked-in expected output",
            adapter.key(),
            expected.request_id
        ));
    }
    Ok(())
}

fn step_record(id: String, request: ResolutionRequest, receipt: ResolutionReceipt) -> ReplayedStep {
    let event_kinds = receipt
        .events
        .iter()
        .map(|event| event.kind.clone())
        .collect::<Vec<_>>();
    let public_event_kinds = receipt
        .events
        .iter()
        .filter(|event| event.visibility == EventVisibility::Public)
        .map(|event| event.kind.clone())
        .collect();
    ReplayedStep {
        operation: request.operation.clone(),
        id,
        request,
        receipt,
        event_kinds,
        public_event_kinds,
    }
}

fn definition_name(definition: &DomainValue) -> Option<String> {
    match definition {
        DomainValue::Object(fields) => fields.get("name").and_then(|value| match value {
            DomainValue::String(name) => Some(name.clone()),
            _ => None,
        }),
        _ => None,
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn register_fixture_resolver(
    adapter: Adapter,
    manifest: &AdapterManifest,
    registry: &mut ResolverRegistry,
) -> Result<(), String> {
    let result = match adapter {
        Adapter::PlugAndPlay => registry.register(manifest, PlugAndPlayResolver),
        Adapter::Dungeonpunk => registry.register(manifest, DungeonpunkResolver),
        Adapter::Freehack => registry.register(manifest, FreehackResolver),
        Adapter::OpenTale => registry.register(manifest, OpenTaleResolver),
        Adapter::Spark | Adapter::WickedOnes | Adapter::OpenAdventure => {
            return Err(format!(
                "{}: canonical projects register through their conformance adapter",
                adapter.key()
            ));
        }
    };
    result.map_err(|error| format!("{}: resolver registration failed: {error}", adapter.key()))
}

fn load_creation(adapter: Adapter, directory: &Path) -> Result<(String, TabletopState), String> {
    let creation_source = read_text(&directory.join(CREATION_FILE))?;
    let creation: serde_json::Value = serde_json::from_str(&creation_source)
        .map_err(|error| format!("{}: creation JSON is invalid: {error}", adapter.key()))?;
    let character_name = creation
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{}: creation request has no name", adapter.key()))?
        .to_owned();
    let preview_source = read_text(&directory.join(PREVIEW_FILE))?;

    let state = match adapter {
        Adapter::PlugAndPlay => {
            let request = PlugAndPlayCreationRequest::from_json(&creation_source)
                .map_err(|error| format!("plug-and-play: creation request: {error}"))?;
            let preview = PlugAndPlayCreationPreview::from_json(&preview_source)
                .map_err(|error| format!("plug-and-play: creation preview: {error}"))?;
            let generated = create_plug_and_play_character(&request)
                .map_err(|error| format!("plug-and-play: creation failed: {error}"))?;
            if generated != preview {
                return Err("plug-and-play: preview does not reproduce from creation".to_owned());
            }
            validate_plug_and_play_creation_preview(&preview)
                .map_err(|error| format!("plug-and-play: preview rejected: {error}"))?;
            preview.initial_state
        }
        Adapter::Dungeonpunk => {
            let request = DungeonpunkCreationRequest::from_json(&creation_source)
                .map_err(|error| format!("dungeonpunk: creation request: {error}"))?;
            let preview = DungeonpunkCreationPreview::from_json(&preview_source)
                .map_err(|error| format!("dungeonpunk: creation preview: {error}"))?;
            let generated = create_dungeonpunk_character(&request)
                .map_err(|error| format!("dungeonpunk: creation failed: {error}"))?;
            if generated != preview {
                return Err("dungeonpunk: preview does not reproduce from creation".to_owned());
            }
            validate_dungeonpunk_creation_preview(&preview)
                .map_err(|error| format!("dungeonpunk: preview rejected: {error}"))?;
            preview.initial_state
        }
        Adapter::Freehack => {
            let request = FreehackCreationRequest::from_json(&creation_source)
                .map_err(|error| format!("freehack: creation request: {error}"))?;
            let preview = FreehackCreationPreview::from_json(&preview_source)
                .map_err(|error| format!("freehack: creation preview: {error}"))?;
            let generated = create_freehack_character(&request)
                .map_err(|error| format!("freehack: creation failed: {error}"))?;
            if generated != preview {
                return Err("freehack: preview does not reproduce from creation".to_owned());
            }
            validate_freehack_creation_preview(&preview)
                .map_err(|error| format!("freehack: preview rejected: {error}"))?;
            preview.initial_state
        }
        Adapter::OpenTale => {
            let request = OpenTaleCreationRequest::from_json(&creation_source)
                .map_err(|error| format!("open-tale: creation request: {error}"))?;
            let preview = OpenTaleCreationPreview::from_json(&preview_source)
                .map_err(|error| format!("open-tale: creation preview: {error}"))?;
            let generated = create_open_tale_character(&request)
                .map_err(|error| format!("open-tale: creation failed: {error}"))?;
            if generated != preview {
                return Err("open-tale: preview does not reproduce from creation".to_owned());
            }
            validate_open_tale_creation_preview(&preview)
                .map_err(|error| format!("open-tale: preview rejected: {error}"))?;
            preview.initial_state
        }
        Adapter::Spark | Adapter::WickedOnes | Adapter::OpenAdventure => {
            return Err(format!(
                "{}: canonical projects create through their conformance adapter",
                adapter.key()
            ));
        }
    };

    Ok((character_name, state))
}

fn load_steps(
    adapter: Adapter,
    directory: &Path,
    receipt_suffix: &str,
) -> Result<Vec<CandidateStep>, String> {
    let mut request_paths = fs::read_dir(directory)
        .map_err(|error| format!("failed to read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| name.ends_with(".tabletop-request.json"))
        })
        .collect::<Vec<_>>();
    request_paths.sort();
    if request_paths.is_empty() {
        return Err(format!(
            "{}: no playthrough requests under {}",
            adapter.key(),
            directory.display()
        ));
    }

    request_paths
        .into_iter()
        .map(|request_path| {
            let file_name = request_path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .ok_or_else(|| "request file name is not UTF-8".to_owned())?;
            let id = file_name
                .strip_suffix(".tabletop-request.json")
                .ok_or_else(|| format!("unexpected request file name: {file_name}"))?
                .to_owned();
            let receipt_path = directory.join(format!("{id}{receipt_suffix}"));
            let request = ResolutionRequest::from_json(&read_text(&request_path)?)
                .map_err(|error| format!("{}: {id} request: {error}", adapter.key()))?;
            let receipt_source = read_text(&receipt_path)?;
            let receipt = match adapter {
                Adapter::Freehack => {
                    let authority = FreehackAuthorityReceipt::from_json(&receipt_source)
                        .map_err(|error| format!("freehack: {id} receipt: {error}"))?;
                    validate_freehack_authority_receipt(&authority)
                        .map_err(|error| format!("freehack: {id} receipt rejected: {error}"))?;
                    authority.receipt
                }
                _ => ResolutionReceipt::from_json(&receipt_source)
                    .map_err(|error| format!("{}: {id} receipt: {error}", adapter.key()))?,
            };
            Ok(CandidateStep {
                id,
                request,
                receipt,
            })
        })
        .collect()
}

fn order_by_receipt_chain(
    initial_state: &TabletopState,
    mut candidates: Vec<CandidateStep>,
) -> Result<Vec<CandidateStep>, String> {
    let mut remaining = candidates.len();
    let mut next_hash = canonical_fingerprint(initial_state)
        .map_err(|error| format!("initial state fingerprint failed: {error}"))?;
    let mut ordered = Vec::with_capacity(remaining);
    while remaining > 0 {
        let Some(index) = candidates
            .iter()
            .position(|candidate| candidate.request.expected_state_sha256 == next_hash)
        else {
            return Err(format!(
                "playthrough chain is broken after {} steps; {} steps remain",
                ordered.len(),
                remaining
            ));
        };
        let candidate = candidates.remove(index);
        next_hash = candidate.receipt.after_state_sha256.clone();
        ordered.push(candidate);
        remaining -= 1;
    }
    Ok(ordered)
}

fn verify_index(adapter: Adapter, index_path: &Path, steps: &[ReplayedStep]) -> Result<(), String> {
    let index_source = read_text(index_path)?;
    let index: Vec<FixtureIndexStep> = serde_json::from_str(&index_source)
        .map_err(|error| format!("{}: playthrough index is invalid: {error}", adapter.key()))?;
    if index.len() != steps.len() {
        return Err(format!(
            "{}: index lists {} steps but {} replayed",
            adapter.key(),
            index.len(),
            steps.len()
        ));
    }
    for (step, expected) in steps.iter().zip(&index) {
        let expected_id = Path::new(&expected.request_json)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(|name| name.strip_suffix(".tabletop-request.json"))
            .unwrap_or_default();
        if step.id != expected_id {
            return Err(format!(
                "{}: indexed step {expected_id} replayed as {}",
                adapter.key(),
                step.id
            ));
        }
        if step.event_kinds != expected.event_kinds {
            return Err(format!(
                "{}: {} event sequence differs from the index",
                adapter.key(),
                step.id
            ));
        }
    }
    Ok(())
}

fn flatten_value(value: &DomainValue) -> Vec<String> {
    let mut lines = Vec::new();
    flatten_into(value, "", &mut lines);
    lines
}

fn flatten_json(value: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();
    flatten_json_into(value, "", &mut lines);
    lines
}

fn flatten_json_into(value: &serde_json::Value, path: &str, lines: &mut Vec<String>) {
    match value {
        serde_json::Value::Null => lines.push(join_path(path, "null")),
        serde_json::Value::Bool(value) => lines.push(join_path(path, value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_u64() {
                lines.push(join_path(path, value));
            } else if let Some(value) = value.as_i64() {
                lines.push(join_path(path, value));
            } else if let Some(value) = value.as_f64() {
                lines.push(join_path(path, format_number(value)));
            }
        }
        serde_json::Value::String(value) => lines.push(join_path(path, value)),
        serde_json::Value::Array(values) => {
            if values.is_empty() {
                lines.push(join_path(path, "[]"));
            }
            for (index, value) in values.iter().enumerate() {
                flatten_json_into(value, &format!("{path}[{index}]"), lines);
            }
        }
        serde_json::Value::Object(fields) => {
            if fields.is_empty() {
                lines.push(join_path(path, "{}"));
            }
            for (key, value) in fields {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                flatten_json_into(value, &child, lines);
            }
        }
    }
}

fn flatten_into(value: &DomainValue, path: &str, lines: &mut Vec<String>) {
    match value {
        DomainValue::Null => lines.push(join_path(path, "null")),
        DomainValue::Bool(value) => lines.push(join_path(path, value)),
        DomainValue::Number(value) => lines.push(join_path(path, format_number(*value))),
        DomainValue::String(value) | DomainValue::Symbol(value) => {
            lines.push(join_path(path, value));
        }
        DomainValue::List(values) => {
            if values.is_empty() {
                lines.push(join_path(path, "[]"));
            }
            for (index, value) in values.iter().enumerate() {
                flatten_into(value, &format!("{path}[{index}]"), lines);
            }
        }
        DomainValue::Object(fields) => {
            if fields.is_empty() {
                lines.push(join_path(path, "{}"));
            }
            for (key, value) in fields {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                flatten_into(value, &child, lines);
            }
        }
    }
}

fn join_path<T: std::fmt::Display>(path: &str, value: T) -> String {
    if path.is_empty() {
        value.to_string()
    } else {
        format!("{path}={value}")
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.4}")
    }
}

/// The fixture directory used by the binaries and tests.
#[must_use]
pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tabletop-adapters")
}

#[derive(Debug, Serialize)]
struct PixijsProjection {
    projection_format_version: u32,
    generated_by: String,
    source: String,
    adapters: Vec<PixijsAdapter>,
}

#[derive(Debug, Serialize)]
struct PixijsAdapter {
    key: String,
    id: String,
    version: String,
    source: String,
    character_name: Option<String>,
    public_event_count: usize,
    initial_state_sha256: String,
    final_state_sha256: String,
    presentation_color: String,
    contract: PixijsContract,
    steps: Vec<PixijsStep>,
    observable: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PixijsContract {
    manifest_format_version: u32,
    operations: std::collections::BTreeMap<String, PixijsOperation>,
    event_types: Vec<String>,
    runtime_visibility: Vec<EventVisibility>,
}

#[derive(Debug, Serialize)]
struct PixijsOperation {
    required_capability: TabletopCapability,
    event_kinds: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PixijsStep {
    id: String,
    operation: String,
    capability: TabletopCapability,
    request_sha256: String,
    before_state_sha256: String,
    after_state_sha256: String,
    events: Vec<PixijsEvent>,
}

#[derive(Debug, Serialize)]
struct PixijsEvent {
    sequence: usize,
    kind: String,
    capability: TabletopCapability,
    payload: DomainValue,
    payload_sha256: String,
}

/// Deterministic presentation colour, matching the PixiJS consumer.
#[must_use]
pub fn presentation_color(id: &str) -> String {
    let mut hash = 2_166_136_261_u32;
    for byte in id.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("hsl({}, 52%, 58%)", hash % 360)
}

/// Build the public PixiJS projection from fully replayed and verified adapter data.
///
/// Public events are re-derived from each authority receipt with `project_receipt_for_audience`,
/// so a changed kind or payload cannot enter the browser bundle. Hidden event totals are omitted.
pub fn publish_pixijs_projection(root: &Path) -> Result<String, String> {
    let runs = replay_all(root)?;
    let adapters = runs
        .iter()
        .map(pixijs_adapter)
        .collect::<Result<Vec<_>, String>>()?;
    let projection = PixijsProjection {
        projection_format_version: 2,
        generated_by: "weave-example-bevy-tabletop publish-pixijs".to_owned(),
        source: "validated checked-in tabletop adapter fixture trees and canonical projects"
            .to_owned(),
        adapters,
    };
    let mut json = serde_json::to_string_pretty(&projection)
        .map_err(|error| format!("PixiJS projection serialization failed: {error}"))?;
    json.push('\n');
    Ok(json)
}

fn pixijs_adapter(run: &AdapterRun) -> Result<PixijsAdapter, String> {
    let mut public_event_count = 0;
    let mut steps = Vec::new();
    for step in &run.steps {
        let projected =
            project_receipt_for_audience(&step.receipt, &run.manifest, HostAudience::Runtime)
                .map_err(|error| {
                    format!(
                        "{}: {} runtime projection failed: {error}",
                        run.adapter.key(),
                        step.id
                    )
                })?;
        let events = projected
            .events
            .iter()
            .filter(|event| event.visibility == EventVisibility::Public)
            .enumerate()
            .map(|(sequence, event)| {
                let payload = event.payload.clone().ok_or_else(|| {
                    format!(
                        "{}: {} public event has no payload",
                        run.adapter.key(),
                        step.id
                    )
                })?;
                Ok(PixijsEvent {
                    sequence,
                    kind: event.kind.clone(),
                    capability: event.capability,
                    payload,
                    payload_sha256: event.payload_sha256.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        if events.is_empty() {
            continue;
        }
        public_event_count += events.len();
        steps.push(PixijsStep {
            id: step.id.clone(),
            operation: step.operation.clone(),
            capability: step.receipt.capability,
            request_sha256: step.receipt.request_sha256.clone(),
            before_state_sha256: step.receipt.before_state_sha256.clone(),
            after_state_sha256: step.receipt.after_state_sha256.clone(),
            events,
        });
    }

    let operations = run
        .manifest
        .operations
        .iter()
        .map(|(id, operation)| {
            (
                id.clone(),
                PixijsOperation {
                    required_capability: operation.required_capability,
                    event_kinds: operation.event_kinds.clone(),
                },
            )
        })
        .collect();
    let source = match run.source {
        RunSource::CanonicalProject => "canonical_project",
        RunSource::FixtureTree if run.adapter == Adapter::Freehack => "freehack_public",
        RunSource::FixtureTree => "fixture_tree",
    };
    Ok(PixijsAdapter {
        key: run.adapter.key().to_owned(),
        id: run.manifest.id.clone(),
        version: run.manifest.version.clone(),
        source: source.to_owned(),
        character_name: Some(run.character_name.clone()),
        public_event_count,
        initial_state_sha256: run.initial_state_sha256.clone(),
        final_state_sha256: run.public_final_state_sha256.clone(),
        presentation_color: presentation_color(&run.manifest.id),
        contract: PixijsContract {
            manifest_format_version: run.manifest.manifest_format_version,
            operations,
            event_types: run.manifest.event_types.keys().cloned().collect(),
            runtime_visibility: run.manifest.visibility.runtime.clone(),
        },
        steps,
        observable: if run.source == RunSource::CanonicalProject {
            Vec::new()
        } else {
            run.observable.clone()
        },
    })
}

/// A stable one-line summary used by both Bevy binaries and documentation.
#[must_use]
pub fn summary_line(run: &AdapterRun) -> String {
    format!(
        "{} {}: {} · {} · {} steps · {} authority events ({} public) · revision {} · entropy {} · final {}",
        run.manifest.id,
        run.manifest.version,
        run.character_name,
        run.source.label(),
        run.steps.len(),
        run.event_count(),
        run.public_event_count(),
        run.final_state.revision,
        run.final_state.entropy.cursor,
        run.public_final_state_sha256
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_tabletop::FreehackPublicState;

    fn runs() -> Vec<AdapterRun> {
        replay_all(&fixture_root()).expect("every checked-in adapter replays")
    }

    #[test]
    fn every_checked_in_adapter_replays_creation_playthrough_and_final_state() {
        let runs = runs();
        assert_eq!(runs.len(), Adapter::ALL.len());
        let expected_steps = [
            (Adapter::PlugAndPlay, 7),
            (Adapter::Dungeonpunk, 11),
            (Adapter::Freehack, 16),
            (Adapter::OpenTale, 11),
            (Adapter::Spark, 6),
            (Adapter::WickedOnes, 1),
            (Adapter::OpenAdventure, 2),
        ];
        for (run, (adapter, steps)) in runs.iter().zip(expected_steps) {
            assert_eq!(run.adapter, adapter);
            assert_eq!(run.steps.len(), steps, "{} step count", adapter.key());
            assert!(run.event_count() > 0, "{} events", adapter.key());
            assert!(
                run.public_event_count() > 0,
                "{} public events",
                adapter.key()
            );
            assert!(
                !run.observable.is_empty(),
                "{} observable state",
                adapter.key()
            );
        }
    }

    #[test]
    fn fixture_adapters_keep_their_recorded_event_counts() {
        let expected = [
            (Adapter::PlugAndPlay, 7, 15),
            (Adapter::Dungeonpunk, 20, 33),
            (Adapter::Freehack, 15, 33),
            (Adapter::OpenTale, 28, 28),
        ];
        for (adapter, public_events, all_events) in expected {
            let run = replay_adapter(adapter, &fixture_root()).expect("fixture replays");
            assert_eq!(run.public_event_count(), public_events, "{}", adapter.key());
            assert_eq!(run.event_count(), all_events, "{}", adapter.key());
        }
    }

    #[test]
    fn canonical_projects_for_the_remaining_adapters_replay_exact_hashes() {
        for (adapter, steps) in [
            (Adapter::Spark, 6),
            (Adapter::WickedOnes, 1),
            (Adapter::OpenAdventure, 2),
        ] {
            let run = replay_adapter(adapter, &fixture_root()).expect("canonical project replays");
            assert_eq!(run.source, RunSource::CanonicalProject);
            assert_eq!(run.steps.len(), steps, "{}", adapter.key());
        }
    }

    #[test]
    fn open_tale_replays_the_recorded_eleven_step_playthrough() {
        let run = replay_adapter(Adapter::OpenTale, &fixture_root()).expect("Open Tale replays");
        assert_eq!(run.steps.len(), 11);
        assert_eq!(
            run.steps.first().map(|step| step.id.as_str()),
            Some("01_cross_the_ford")
        );
        assert_eq!(
            run.steps.last().map(|step| step.id.as_str()),
            Some("11_inspect_character")
        );
        assert_eq!(run.public_event_count(), 28);
    }

    #[test]
    fn fixture_final_states_equal_the_checked_in_snapshots() {
        for run in runs()
            .into_iter()
            .filter(|run| run.source == RunSource::FixtureTree)
        {
            let spec = run.adapter.spec().expect("fixture adapters have a spec");
            let directory = fixture_root().join(run.adapter.key());
            let checked = TabletopState::from_json(
                &read_text(&directory.join(spec.final_state_file)).expect("final state reads"),
            )
            .expect("final state parses");
            assert_eq!(run.final_state, checked, "{}", run.adapter.key());
        }
    }

    #[test]
    fn public_final_state_hashes_match_the_pixijs_projection() {
        let expected = [
            (
                Adapter::PlugAndPlay,
                "962635b705d3b020874b76ccee019153ac6c1eb16f2e926a15088e59c2f669d5",
            ),
            (
                Adapter::Dungeonpunk,
                "d637bf445e993fb38263d3422a502bb736b802e20c9482eee1cfa25ebc50509c",
            ),
            (
                Adapter::Freehack,
                "14e3afe2446d887b20da3257a2f549b359ca549068b26bfa088d0cd413543754",
            ),
            (
                Adapter::OpenTale,
                "362f3423ac139cdf8cc45674e8c6aa1aa2396ebabe248018f6d056bee19c7fea",
            ),
            (
                Adapter::Spark,
                "43e0df3b9c9ae768b1cd48acb2c012e3a869305267a0d0cad95274f787af975f",
            ),
            (
                Adapter::WickedOnes,
                "0304f0f5b0d118a3f0624b9d97bfd1a07f4be6fa269b6b65cc9f0565e3d7ccd5",
            ),
            (
                Adapter::OpenAdventure,
                "0940e337105cc4a5098e51e24d6cfaff4a5203fe17275782c21ab4a1509c8be0",
            ),
        ];
        let runs = runs();
        for (run, (adapter, hash)) in runs.iter().zip(expected) {
            assert_eq!(run.adapter, adapter);
            assert_eq!(run.public_final_state_sha256, hash, "{}", adapter.key());
        }

        let freehack = &runs[2];
        let public_state = FreehackPublicState::from_json(
            &read_text(
                &fixture_root()
                    .join("freehack")
                    .join("public-state.freehack-public-state.json"),
            )
            .expect("public state reads"),
        )
        .expect("public state parses");
        assert_eq!(
            canonical_fingerprint(&public_state).expect("public state fingerprints"),
            freehack.public_final_state_sha256
        );
        for run in runs.iter().filter(|run| run.adapter != Adapter::Freehack) {
            assert_eq!(
                run.public_final_state_sha256,
                run.steps
                    .last()
                    .expect("a replayed step exists")
                    .receipt
                    .after_state_sha256
            );
        }
    }

    #[test]
    fn pixijs_projection_contains_only_public_data_and_exact_counts() {
        let json = publish_pixijs_projection(&fixture_root()).expect("projection publishes");
        let projection: serde_json::Value =
            serde_json::from_str(&json).expect("projection parses as JSON");
        assert_eq!(projection["projection_format_version"], 2);
        let adapters = projection["adapters"]
            .as_array()
            .expect("adapters is an array");
        let expected = [
            ("plug-and-play", 7, 7),
            ("dungeonpunk", 11, 20),
            ("freehack", 14, 15),
            ("open-tale", 11, 28),
            ("spark", 6, 8),
            ("wicked-ones", 1, 1),
            ("open-adventure", 2, 2),
        ];
        assert_eq!(adapters.len(), expected.len());
        for (adapter, (key, steps, public_events)) in adapters.iter().zip(expected) {
            assert_eq!(adapter["key"], key);
            assert!(adapter.get("event_count").is_none());
            assert_eq!(adapter["public_event_count"], public_events);
            let step_values = adapter["steps"].as_array().expect("steps is an array");
            assert_eq!(step_values.len(), steps);
            assert_eq!(
                step_values.first().expect("a first step")["before_state_sha256"],
                adapter["initial_state_sha256"]
            );
            for step in step_values {
                assert_eq!(step["capability"], step["events"][0]["capability"]);
                for (index, event) in step["events"]
                    .as_array()
                    .expect("events is an array")
                    .iter()
                    .enumerate()
                {
                    assert_eq!(event["sequence"], index);
                }
            }
            if key != "freehack" {
                assert_eq!(
                    step_values.last().expect("a last step")["after_state_sha256"],
                    adapter["final_state_sha256"]
                );
                for pair in step_values.windows(2) {
                    assert_eq!(
                        pair[0]["after_state_sha256"],
                        pair[1]["before_state_sha256"]
                    );
                }
            }
        }

        let freehack = &adapters[2];
        let observable = serde_json::to_string(&freehack["observable"]).expect("observable text");
        for marker in [
            "sealed_route",
            "host_only",
            "private",
            "authority",
            "entropy",
            "secret",
        ] {
            assert!(!observable.contains(marker), "Freehack leaks {marker}");
        }
    }

    #[test]
    fn observable_values_are_stable_and_match_known_final_fields() {
        let runs = runs();
        let plug_and_play = &runs[0];
        assert!(
            plug_and_play
                .observable
                .iter()
                .any(|line| line == "survivability_current=0")
        );
        assert!(
            plug_and_play
                .observable
                .iter()
                .any(|line| line == "last_operation=take_damage")
        );

        let dungeonpunk = &runs[1];
        assert!(
            dungeonpunk
                .observable
                .iter()
                .any(|line| line == "clocks.bridge_wakes.filled=4")
        );
        assert!(
            dungeonpunk
                .observable
                .iter()
                .any(|line| line == "advancement.learned_moves[0].id=bridgecraft")
        );

        let freehack = &runs[2];
        assert!(
            freehack
                .observable
                .iter()
                .any(|line| line == "memories[1].topic=the public tide-gate schedule")
        );
        for marker in [
            "sealed_route",
            "host_only",
            "private",
            "authority",
            "entropy",
        ] {
            assert!(
                !freehack.observable.iter().any(|line| line.contains(marker)),
                "Freehack authority data leaked into the public observable: {marker}"
            );
        }

        let open_tale = &runs[3];
        assert!(
            open_tale
                .observable
                .iter()
                .any(|line| line == "challenges.cross_the_ford.band=critical")
        );
        assert!(
            open_tale
                .observable
                .iter()
                .any(|line| line == "combats.reed_watch.status=concluded")
        );

        for run in &runs[4..] {
            assert_eq!(run.source, RunSource::CanonicalProject);
            assert!(
                !run.character_name.is_empty(),
                "{} exposes the created name",
                run.adapter.key()
            );
        }
    }
}
