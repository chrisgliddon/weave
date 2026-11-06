//! Registry-driven conformance suite for tabletop adapters.
//!
//! The suite validates every adapter against one public contract. Adapters register a manifest, a
//! resolver factory, and a deterministic creation fixture through [`ConformanceAdapter`]; the suite
//! itself contains no adapter-specific branches. Adding an adapter therefore means adding one
//! registration row in [`standard_suite`] and nothing else.
//!
//! Every failure names the adapter, the check, and the offending value. [`ConformanceReport`]
//! renders one summary line per adapter after the individual failures.

pub mod policy;
mod standard;

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use weave_domain::{DomainValue, validate_typed_value};

use crate::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition,
    AdapterManifest, CHARACTER_PROJECTION_FORMAT_VERSION, EntropyState, EntropyStream,
    EventVisibility, HostAudience, RESOLVER_FORMAT_VERSION, ResolutionReceipt, ResolutionRequest,
    ResolvedAdapter, ResolverRegistry, TabletopCapability, TabletopCharacterProjection,
    TabletopError, TabletopResolver, TabletopState, canonical_fingerprint, has_capability,
    project_receipt_for_audience, resolved_adapter, validate_adapter_manifest,
    validate_character_projection, validate_resolution_request, validate_tabletop_state,
};

pub use policy::{
    AdapterSourcePolicy, FORBIDDEN_REDISTRIBUTED_EXTENSIONS, LICENSE_ALLOWLIST, PolicyFile,
    SourcePolicy, standard_source_policy,
};
pub use standard::{
    DungeonpunkConformance, FreehackConformance, PlugAndPlayConformance, standard_suite,
};

/// Deterministic creation seed shared by every adapter fixture.
pub const CONFORMANCE_SEED: u64 = 20_261_031;

/// Operation id used to prove a resolver fails closed on undeclared input.
pub const UNDECLARED_OPERATION: &str = "conformance_undeclared_operation";

const CHECK_COUNT: usize = ConformanceCheck::ALL.len();

const ALL_CAPABILITIES: [TabletopCapability; 12] = [
    TabletopCapability::CharacterCreation,
    TabletopCapability::DerivedValues,
    TabletopCapability::ChecksAndConflicts,
    TabletopCapability::ResourcesAndConditions,
    TabletopCapability::EquipmentAndAbilities,
    TabletopCapability::Advancement,
    TabletopCapability::Encounters,
    TabletopCapability::Clocks,
    TabletopCapability::Scenes,
    TabletopCapability::Factions,
    TabletopCapability::WorldState,
    TabletopCapability::CampaignState,
];

/// One independently reportable conformance check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConformanceCheck {
    /// The manifest satisfies the complete adapter contract.
    ManifestValidation,
    /// The namespace is attributable to the identity and cannot accept foreign data.
    NamespaceIsolation,
    /// Creation from one seed is byte-for-byte reproducible.
    DeterministicCreation,
    /// Resolution of the same before-state is byte-for-byte reproducible.
    DeterministicResolution,
    /// Entropy replays from the same seed and survives save/restore.
    EntropyReplay,
    /// Mutable state round-trips through JSON and RON and resumes exactly.
    SaveRestore,
    /// Authoring projections reveal more than compact runtime projections.
    ProjectionBoundary,
    /// Structured events are contiguous, typed, hashed, and declared.
    EventShape,
    /// Undeclared capabilities and operations are rejected.
    UnknownCapability,
    /// Unsupported manifest, state, and resolver versions are rejected.
    UnsupportedVersion,
    /// Malformed state artifacts and coordinates are rejected.
    MalformedState,
    /// The exact primary source digest matches the reviewed policy.
    SourceHash,
    /// The declared licence and source class are on the public allowlist.
    LicensePolicy,
    /// Required licence text is present with the exact digest.
    LicenseText,
    /// Required notice text and notice files are present.
    LicenseNotice,
    /// No forbidden file is redistributed by the adapter.
    ForbiddenFile,
    /// No two adapters claim the same namespace or identity.
    NamespaceCollision,
    /// Serialization is stable and round-trips without drift.
    DeterministicOutput,
}

impl ConformanceCheck {
    /// Every check, in reporting order.
    pub const ALL: &'static [Self] = &[
        Self::ManifestValidation,
        Self::NamespaceIsolation,
        Self::DeterministicCreation,
        Self::DeterministicResolution,
        Self::EntropyReplay,
        Self::SaveRestore,
        Self::ProjectionBoundary,
        Self::EventShape,
        Self::UnknownCapability,
        Self::UnsupportedVersion,
        Self::MalformedState,
        Self::SourceHash,
        Self::LicensePolicy,
        Self::LicenseText,
        Self::LicenseNotice,
        Self::ForbiddenFile,
        Self::NamespaceCollision,
        Self::DeterministicOutput,
    ];

    /// Stable machine-readable identifier.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ManifestValidation => "manifest_validation",
            Self::NamespaceIsolation => "namespace_isolation",
            Self::DeterministicCreation => "deterministic_creation",
            Self::DeterministicResolution => "deterministic_resolution",
            Self::EntropyReplay => "entropy_replay",
            Self::SaveRestore => "save_restore",
            Self::ProjectionBoundary => "projection_boundary",
            Self::EventShape => "event_shape",
            Self::UnknownCapability => "unknown_capability",
            Self::UnsupportedVersion => "unsupported_version",
            Self::MalformedState => "malformed_state",
            Self::SourceHash => "source_hash",
            Self::LicensePolicy => "license_policy",
            Self::LicenseText => "license_text",
            Self::LicenseNotice => "license_notice",
            Self::ForbiddenFile => "forbidden_file",
            Self::NamespaceCollision => "namespace_collision",
            Self::DeterministicOutput => "deterministic_output",
        }
    }

    /// Short human-readable contract statement.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::ManifestValidation => "manifest satisfies the adapter contract",
            Self::NamespaceIsolation => "namespace is attributable and rejects foreign data",
            Self::DeterministicCreation => "creation from one seed is reproducible",
            Self::DeterministicResolution => "resolution of the same state is reproducible",
            Self::EntropyReplay => "entropy replays from the same seed",
            Self::SaveRestore => "state saves, restores, and resumes exactly",
            Self::ProjectionBoundary => "authoring projections exceed compact runtime output",
            Self::EventShape => "events are contiguous, declared, typed, and hashed",
            Self::UnknownCapability => "undeclared capabilities and operations fail closed",
            Self::UnsupportedVersion => "unsupported versions fail closed",
            Self::MalformedState => "malformed state fails closed",
            Self::SourceHash => "primary source digest matches the reviewed policy",
            Self::LicensePolicy => "licence and source class are allowlisted",
            Self::LicenseText => "required licence text is retained exactly",
            Self::LicenseNotice => "required notices and notice files are retained",
            Self::ForbiddenFile => "no forbidden material is redistributed",
            Self::NamespaceCollision => "namespace and identity are unique",
            Self::DeterministicOutput => "serialization is stable and round-trips",
        }
    }
}

/// One adapter-owned deterministic creation fixture produced from a seed.
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceCreation {
    /// Exact adapter coordinate embedded in the definition and state.
    pub adapter: ResolvedAdapter,
    /// Immutable adapter-owned character definition.
    pub definition: AdapterCharacterDefinition,
    /// Mutable state immediately after creation.
    pub initial_state: TabletopState,
    /// Canonical serialization of the complete creation preview.
    pub canonical: String,
}

/// One host-neutral resolution operation exercised by the suite.
#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceOperation {
    /// Local request identity.
    pub request_id: String,
    /// Declared operation identifier.
    pub operation: String,
    /// Capability the operation belongs to.
    pub capability: TabletopCapability,
    /// Typed request input.
    pub input: DomainValue,
}

/// Adapter-supplied conformance fixture.
///
/// Implementations are inert descriptors: they build the manifest, a trusted resolver, one
/// deterministic creation, and a short operation scenario. The suite owns all execution,
/// projection, replay, and policy checks.
pub trait ConformanceAdapter: Send + Sync {
    /// Stable registration identity; must equal the manifest identity.
    fn adapter_id(&self) -> &str;
    /// The exact manifest under test.
    fn manifest(&self) -> AdapterManifest;
    /// A fresh trusted resolver for this adapter release.
    fn resolver(&self) -> Box<dyn TabletopResolver>;
    /// Deterministically create one character from `seed`.
    fn create(&self, seed: u64) -> Result<ConformanceCreation, TabletopError>;
    /// Operations exercised by the suite, executed in order from the creation state.
    fn scenario(
        &self,
        creation: &ConformanceCreation,
    ) -> Result<Vec<ConformanceOperation>, TabletopError>;
}

/// One redaction-safe conformance failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceFailure {
    /// Registered adapter identity.
    pub adapter: String,
    /// Failed check.
    pub check: ConformanceCheck,
    /// Offending value or rejection reason.
    pub detail: String,
}

impl fmt::Display for ConformanceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "adapter `{}` failed `{}` ({}): {}",
            self.adapter,
            self.check.id(),
            self.check.description(),
            self.detail
        )
    }
}

/// Per-adapter conformance result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterReport {
    /// Registered adapter identity.
    pub adapter: String,
    /// Manifest version under test.
    pub version: String,
    /// Number of checks that ran.
    pub executed: usize,
    /// Failures, ordered by check.
    pub failures: Vec<ConformanceFailure>,
}

impl AdapterReport {
    /// True when every executed check passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// Number of checks that passed.
    #[must_use]
    pub fn passed_checks(&self) -> usize {
        self.executed.saturating_sub(self.failures.len())
    }

    /// Number of checks skipped because a prerequisite failed.
    #[must_use]
    pub fn skipped_checks(&self) -> usize {
        CHECK_COUNT.saturating_sub(self.executed)
    }

    /// One-line summary for this adapter.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.passed() {
            format!(
                "ok   {}@{}: {} of {} checks passed",
                self.adapter, self.version, self.executed, CHECK_COUNT
            )
        } else {
            let names = self
                .failures
                .iter()
                .map(|failure| failure.check.id())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "FAIL {}@{}: {} of {} checks passed; failed [{}]; {} skipped",
                self.adapter,
                self.version,
                self.passed_checks(),
                CHECK_COUNT,
                names,
                self.skipped_checks()
            )
        }
    }
}

/// Complete conformance run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceReport {
    /// Per-adapter results in registration order.
    pub adapters: Vec<AdapterReport>,
}

impl ConformanceReport {
    /// True when every adapter passed every executed check.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.adapters.iter().all(AdapterReport::passed)
    }

    /// Total failures across every adapter.
    #[must_use]
    pub fn failure_count(&self) -> usize {
        self.adapters
            .iter()
            .map(|report| report.failures.len())
            .sum()
    }

    /// Individual failure lines, one per failing check.
    #[must_use]
    pub fn detail_lines(&self) -> Vec<String> {
        self.adapters
            .iter()
            .flat_map(|report| report.failures.iter())
            .map(ToString::to_string)
            .collect()
    }

    /// One summary line per adapter, in registration order.
    #[must_use]
    pub fn summary_lines(&self) -> Vec<String> {
        self.adapters.iter().map(AdapterReport::summary).collect()
    }

    /// Render failures followed by exactly one line per adapter.
    #[must_use]
    pub fn render(&self) -> String {
        let mut lines = self.detail_lines();
        lines.extend(self.summary_lines());
        lines.join("\n")
    }
}

/// Registry-driven conformance suite.
pub struct ConformanceSuite {
    adapters: Vec<Box<dyn ConformanceAdapter>>,
    policy: SourcePolicy,
}

impl fmt::Debug for ConformanceSuite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConformanceSuite")
            .field("adapters", &self.adapters.len())
            .field("policy", &self.policy)
            .finish()
    }
}

impl ConformanceSuite {
    /// Build a suite from explicit registrations and a source policy.
    #[must_use]
    pub fn new(adapters: Vec<Box<dyn ConformanceAdapter>>, policy: SourcePolicy) -> Self {
        Self { adapters, policy }
    }

    /// Register one additional adapter.
    pub fn register(&mut self, adapter: Box<dyn ConformanceAdapter>) {
        self.adapters.push(adapter);
    }

    /// Source policy under test.
    #[must_use]
    pub fn policy(&self) -> &SourcePolicy {
        &self.policy
    }

    /// Registered adapter identities in registration order.
    #[must_use]
    pub fn adapter_ids(&self) -> Vec<&str> {
        self.adapters
            .iter()
            .map(|adapter| adapter.adapter_id())
            .collect()
    }

    /// Run every check against every registered adapter.
    #[must_use]
    pub fn run(&self) -> ConformanceReport {
        let manifests = self
            .adapters
            .iter()
            .map(|adapter| adapter.manifest())
            .collect::<Vec<_>>();
        let claims = NamespaceClaims::new(&self.adapters, &manifests);
        let adapters = self
            .adapters
            .iter()
            .zip(manifests)
            .map(|(adapter, manifest)| self.run_adapter(adapter.as_ref(), &manifest, &claims))
            .collect();
        ConformanceReport { adapters }
    }

    #[allow(clippy::redundant_closure_call)]
    fn run_adapter(
        &self,
        adapter: &dyn ConformanceAdapter,
        manifest: &AdapterManifest,
        claims: &NamespaceClaims,
    ) -> AdapterReport {
        let adapter_id = adapter.adapter_id().to_owned();
        let mut failures = Vec::new();
        let mut executed = 0usize;

        macro_rules! check {
            ($kind:ident, $body:block) => {{
                executed += 1;
                let outcome: Result<(), String> = (|| $body)();
                if let Err(detail) = outcome {
                    failures.push(ConformanceFailure {
                        adapter: adapter_id.clone(),
                        check: ConformanceCheck::$kind,
                        detail,
                    });
                }
            }};
        }

        check!(ManifestValidation, {
            if adapter_id != manifest.id {
                return Err(format!(
                    "registration identity `{adapter_id}` does not match manifest id `{}`",
                    manifest.id
                ));
            }
            validate_adapter_manifest(manifest).map_err(|error| format!("manifest: {error}"))
        });

        if !failures.is_empty() {
            return AdapterReport {
                adapter: adapter_id,
                version: manifest.version.clone(),
                executed,
                failures,
            };
        }

        check!(NamespaceIsolation, {
            namespace_isolation(manifest, self.policy.entry(&manifest.id))
        });

        check!(SourceHash, {
            let entry = self
                .policy
                .entry(&manifest.id)
                .ok_or_else(|| format!("no source policy entry for adapter `{}`", manifest.id))?;
            source_hash(manifest, entry)
        });

        check!(LicensePolicy, {
            let entry = self
                .policy
                .entry(&manifest.id)
                .ok_or_else(|| format!("no source policy entry for adapter `{}`", manifest.id))?;
            license_policy(manifest, entry)
        });

        check!(LicenseText, {
            let entry = self
                .policy
                .entry(&manifest.id)
                .ok_or_else(|| format!("no source policy entry for adapter `{}`", manifest.id))?;
            license_text(manifest, entry)
        });

        check!(LicenseNotice, {
            let entry = self
                .policy
                .entry(&manifest.id)
                .ok_or_else(|| format!("no source policy entry for adapter `{}`", manifest.id))?;
            license_notice(manifest, entry)
        });

        check!(ForbiddenFile, {
            let entry = self
                .policy
                .entry(&manifest.id)
                .ok_or_else(|| format!("no source policy entry for adapter `{}`", manifest.id))?;
            forbidden_file(manifest, entry)
        });

        check!(NamespaceCollision, {
            namespace_collision(manifest, claims)
        });

        let created = adapter.create(CONFORMANCE_SEED);

        check!(DeterministicCreation, {
            let first = created.as_ref().map_err(|error| {
                format!("creation from seed {CONFORMANCE_SEED} failed: {error}")
            })?;
            let second = adapter.create(CONFORMANCE_SEED).map_err(|error| {
                format!("repeat creation from seed {CONFORMANCE_SEED} failed: {error}")
            })?;
            let resolved = resolved_adapter(manifest).map_err(failure_detail)?;
            for (label, coordinate) in [
                ("definition", &first.definition.adapter),
                ("state", &first.initial_state.adapter),
            ] {
                if coordinate != &resolved {
                    return Err(format!(
                        "creation {label} coordinate `{}@{}` does not match manifest `{}@{}`",
                        coordinate.id, coordinate.version, resolved.id, resolved.version
                    ));
                }
            }
            if first.canonical != second.canonical {
                return Err(format!(
                    "creation canonical output differs between runs for seed {CONFORMANCE_SEED}: first=`{}` second=`{}`",
                    short_hash(&first.canonical),
                    short_hash(&second.canonical)
                ));
            }
            if first.definition.definition_sha256 != second.definition.definition_sha256 {
                return Err(format!(
                    "definition digest differs between runs: first=`{}` second=`{}`",
                    first.definition.definition_sha256, second.definition.definition_sha256
                ));
            }
            if first.definition != second.definition
                || first.initial_state != second.initial_state
                || first.adapter != second.adapter
            {
                return Err(format!(
                    "creation from seed {CONFORMANCE_SEED} produced a different definition, state, or coordinate between runs"
                ));
            }
            Ok(())
        });

        let Some(creation) = created.as_ref().ok() else {
            return AdapterReport {
                adapter: adapter_id,
                version: manifest.version.clone(),
                executed,
                failures,
            };
        };

        let scenario = adapter
            .scenario(creation)
            .map_err(|error| error.to_string());

        check!(DeterministicResolution, {
            let operations = scenario
                .as_ref()
                .map_err(|error| format!("adapter scenario could not be built: {error}"))?;
            let registry = registry_for(adapter, manifest)?;
            let first = execute_scenario(&registry, manifest, creation, operations)?;
            let second = execute_scenario(&registry, manifest, creation, operations)?;
            if first.len() != second.len() {
                return Err(format!(
                    "resolution receipt count differs between runs: first={} second={}",
                    first.len(),
                    second.len()
                ));
            }
            for (index, (left, right)) in first.iter().zip(&second).enumerate() {
                let left_digest = digest(left)?;
                let right_digest = digest(right)?;
                if left_digest != right_digest {
                    return Err(format!(
                        "resolution of operation `{}` is not reproducible: first=`{left_digest}` second=`{right_digest}`",
                        operations[index].operation
                    ));
                }
            }
            Ok(())
        });

        check!(EntropyReplay, {
            entropy_replay(creation, &scenario, manifest, adapter)
        });

        check!(SaveRestore, {
            let operations = scenario
                .as_ref()
                .map_err(|error| format!("adapter scenario could not be built: {error}"))?;
            save_restore(manifest, creation, operations, adapter)
        });

        check!(ProjectionBoundary, {
            let operations = scenario
                .as_ref()
                .map_err(|error| format!("adapter scenario could not be built: {error}"))?;
            let registry = registry_for(adapter, manifest)?;
            let receipts = execute_scenario(&registry, manifest, creation, operations)?;
            projection_boundary(manifest, &receipts)
        });

        check!(EventShape, {
            let operations = scenario
                .as_ref()
                .map_err(|error| format!("adapter scenario could not be built: {error}"))?;
            let registry = registry_for(adapter, manifest)?;
            let receipts = execute_scenario(&registry, manifest, creation, operations)?;
            event_shape(manifest, &receipts)
        });

        check!(UnknownCapability, {
            let operations = scenario
                .as_ref()
                .map_err(|error| format!("adapter scenario could not be built: {error}"))?;
            unknown_capability(manifest, creation, operations, adapter)
        });

        check!(UnsupportedVersion, {
            unsupported_version(manifest, creation)
        });

        check!(MalformedState, { malformed_state(manifest, creation) });

        check!(DeterministicOutput, {
            let operations = scenario
                .as_ref()
                .map_err(|error| format!("adapter scenario could not be built: {error}"))?;
            deterministic_output(manifest, creation, operations)
        });

        AdapterReport {
            adapter: adapter_id,
            version: manifest.version.clone(),
            executed,
            failures,
        }
    }
}

fn failure_detail(error: TabletopError) -> String {
    error.to_string()
}

fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    canonical_fingerprint(value).map_err(failure_detail)
}

fn short_hash(value: &str) -> String {
    let digest = crate::sha256_bytes(value.as_bytes());
    digest.get(..16).map_or_else(String::new, str::to_owned)
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn registry_for(
    adapter: &dyn ConformanceAdapter,
    manifest: &AdapterManifest,
) -> Result<ResolverRegistry, String> {
    let mut registry = ResolverRegistry::new();
    registry
        .register(manifest, adapter.resolver())
        .map_err(|error| format!("resolver registration failed: {error}"))?;
    Ok(registry)
}

fn execute_scenario(
    registry: &ResolverRegistry,
    manifest: &AdapterManifest,
    creation: &ConformanceCreation,
    operations: &[ConformanceOperation],
) -> Result<Vec<ResolutionReceipt>, String> {
    let mut state = creation.initial_state.clone();
    let mut receipts = Vec::with_capacity(operations.len());
    for operation in operations {
        let expected_state_sha256 = digest(&state)?;
        let request = ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: operation.request_id.clone(),
            adapter: creation.adapter.clone(),
            operation: operation.operation.clone(),
            capability: operation.capability,
            definition_sha256: creation.definition.definition_sha256.clone(),
            definition: creation.definition.definition.clone(),
            expected_state_sha256,
            input: operation.input.clone(),
        };
        let receipt = registry
            .execute(manifest, &request, &state)
            .map_err(|error| {
                format!("operation `{}` was rejected: {error}", operation.operation)
            })?;
        state = receipt.after_state.clone();
        receipts.push(receipt);
    }
    Ok(receipts)
}

fn namespace_isolation(
    manifest: &AdapterManifest,
    entry: Option<&AdapterSourcePolicy>,
) -> Result<(), String> {
    if !manifest
        .id
        .split('.')
        .any(|segment| segment == manifest.namespace)
    {
        return Err(format!(
            "namespace `{}` is not a segment of identity `{}`",
            manifest.namespace, manifest.id
        ));
    }
    if let Some(entry) = entry
        && entry.namespace != manifest.namespace
    {
        return Err(format!(
            "namespace `{}` does not match reviewed policy namespace `{}`",
            manifest.namespace, entry.namespace
        ));
    }

    let foreign_namespace = ResolvedAdapter {
        id: "org.weave.tabletop.foreign_fixture".to_owned(),
        version: "1.0.0".to_owned(),
        schema_version: 1,
        content_sha256: "0".repeat(64),
    };
    let sibling_identity = ResolvedAdapter {
        id: format!("{}.sibling_probe", manifest.namespace),
        version: "1.0.0".to_owned(),
        schema_version: 1,
        content_sha256: "0".repeat(64),
    };
    let installed = resolved_adapter(manifest).map_err(failure_detail)?;
    for foreign in [&foreign_namespace, &sibling_identity] {
        if foreign == &installed {
            continue;
        }
        reject_foreign_projection(manifest, foreign)?;
        reject_foreign_state(manifest, foreign)?;
    }
    Ok(())
}

fn reject_foreign_projection(
    manifest: &AdapterManifest,
    foreign: &ResolvedAdapter,
) -> Result<(), String> {
    let foreign_value = DomainValue::Object(BTreeMap::new());
    let foreign_definition = AdapterCharacterDefinition {
        definition_sha256: digest(&foreign_value)?,
        adapter: foreign.clone(),
        definition: foreign_value,
        completed_creation_steps: Vec::new(),
    };
    let projection = TabletopCharacterProjection {
        projection_format_version: CHARACTER_PROJECTION_FORMAT_VERSION,
        character_id: "org.weave.character.conformance_probe".to_owned(),
        canonical_profile_sha256: "0".repeat(64),
        canonical_character_write_back: false,
        active: Some(foreign_definition),
        inactive: BTreeMap::new(),
        suggestions: Vec::new(),
    };
    match validate_character_projection(&projection, std::slice::from_ref(manifest)) {
        Err(TabletopError::AdapterNotInstalled) => Ok(()),
        Err(other) => Err(format!(
            "foreign definition was rejected with `{other}` instead of AdapterNotInstalled"
        )),
        Ok(()) => Err(format!(
            "foreign definition for adapter `{}` was accepted by namespace `{}`",
            foreign.id, manifest.namespace
        )),
    }
}

fn reject_foreign_state(
    manifest: &AdapterManifest,
    foreign: &ResolvedAdapter,
) -> Result<(), String> {
    let state = TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter: foreign.clone(),
        owner_id: "org.weave.character.conformance_probe".to_owned(),
        definition_sha256: "0".repeat(64),
        revision: 0,
        entropy: EntropyState {
            algorithm: "sha256_counter_v1".to_owned(),
            seed: 0,
            cursor: 0,
        },
        value: DomainValue::Null,
    };
    match validate_tabletop_state(&state, manifest) {
        Err(TabletopError::ContentHashMismatch { path: "adapter" }) => Ok(()),
        Err(other) => Err(format!(
            "foreign state was rejected with `{other}` instead of an adapter mismatch"
        )),
        Ok(()) => Err(format!(
            "foreign state for adapter `{}` was accepted by namespace `{}`",
            foreign.id, manifest.namespace
        )),
    }
}

fn source_hash(manifest: &AdapterManifest, entry: &AdapterSourcePolicy) -> Result<(), String> {
    if entry.source_sha256.is_empty() {
        return Err(format!(
            "missing source hash in policy for adapter `{}`",
            manifest.id
        ));
    }
    if manifest.provenance.sha256.is_empty() {
        return Err(format!(
            "missing source hash in manifest for artifact `{}`",
            manifest.provenance.exact_artifact
        ));
    }
    if !is_lowercase_sha256(entry.source_sha256) {
        return Err(format!(
            "policy source hash `{}` is not a lowercase SHA-256 digest",
            entry.source_sha256
        ));
    }
    if entry.source_sha256 != manifest.provenance.sha256 {
        return Err(format!(
            "source hash mismatch for `{}`: policy=`{}` manifest=`{}`",
            manifest.provenance.exact_artifact, entry.source_sha256, manifest.provenance.sha256
        ));
    }
    Ok(())
}

fn license_policy(manifest: &AdapterManifest, entry: &AdapterSourcePolicy) -> Result<(), String> {
    let license = manifest.provenance.license.as_str();
    let source_class = manifest.provenance.source_class;
    if !LICENSE_ALLOWLIST
        .iter()
        .any(|(class, allowed)| *class == source_class && *allowed == license)
    {
        let allowed = LICENSE_ALLOWLIST
            .iter()
            .map(|(class, allowed)| format!("{class:?}={allowed}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "unknown licence `{license}` for source class `{source_class:?}`; allowed: {allowed}"
        ));
    }
    if entry.license != license {
        return Err(format!(
            "licence `{license}` does not match reviewed policy `{}`",
            entry.license
        ));
    }
    if entry.source_class != source_class {
        return Err(format!(
            "source class `{source_class:?}` does not match reviewed policy `{:?}`",
            entry.source_class
        ));
    }
    Ok(())
}

fn license_text(manifest: &AdapterManifest, entry: &AdapterSourcePolicy) -> Result<(), String> {
    let declared = &manifest.provenance.required_license_text;
    if declared.artifact != entry.required_license_text.path {
        return Err(format!(
            "missing required licence text `{}`; manifest declares `{}`",
            entry.required_license_text.path, declared.artifact
        ));
    }
    if declared.sha256 != entry.required_license_text.sha256 {
        return Err(format!(
            "licence text hash mismatch for `{}`: policy=`{}` manifest=`{}`",
            declared.artifact, entry.required_license_text.sha256, declared.sha256
        ));
    }
    Ok(())
}

fn license_notice(manifest: &AdapterManifest, entry: &AdapterSourcePolicy) -> Result<(), String> {
    for notice in entry.required_notices {
        if !manifest
            .provenance
            .notices
            .iter()
            .any(|declared| declared == notice)
        {
            return Err(format!(
                "missing required notice `{notice}`; manifest declares {} notice(s)",
                manifest.provenance.notices.len()
            ));
        }
    }
    for file in entry.required_notice_files {
        let Some(artifact) = manifest
            .provenance
            .additional_artifacts
            .iter()
            .find(|artifact| artifact.exact_artifact == file.path)
        else {
            return Err(format!(
                "missing required notice file `{}` in additional artifacts",
                file.path
            ));
        };
        if artifact.sha256 != file.sha256 {
            return Err(format!(
                "notice file hash mismatch for `{}`: policy=`{}` manifest=`{}`",
                file.path, file.sha256, artifact.sha256
            ));
        }
    }
    Ok(())
}

fn forbidden_file(manifest: &AdapterManifest, entry: &AdapterSourcePolicy) -> Result<(), String> {
    for artifact in manifest
        .provenance
        .additional_artifacts
        .iter()
        .filter(|artifact| artifact.redistributed)
    {
        if entry
            .forbidden_files
            .iter()
            .any(|forbidden| forbidden.eq_ignore_ascii_case(&artifact.exact_artifact))
        {
            return Err(format!(
                "forbidden file `{}` is marked as redistributed",
                artifact.exact_artifact
            ));
        }
        let lower = artifact.exact_artifact.to_ascii_lowercase();
        if let Some(extension) = FORBIDDEN_REDISTRIBUTED_EXTENSIONS
            .iter()
            .find(|extension| lower.ends_with(*extension))
        {
            return Err(format!(
                "redistributed file `{}` uses forbidden extension `{extension}`",
                artifact.exact_artifact
            ));
        }
    }
    Ok(())
}

struct NamespaceClaims {
    namespaces: BTreeMap<String, Vec<String>>,
    identities: BTreeMap<String, Vec<String>>,
}

impl NamespaceClaims {
    fn new(adapters: &[Box<dyn ConformanceAdapter>], manifests: &[AdapterManifest]) -> Self {
        let mut namespaces = BTreeMap::new();
        let mut identities = BTreeMap::new();
        for (adapter, manifest) in adapters.iter().zip(manifests) {
            namespaces
                .entry(manifest.namespace.clone())
                .or_insert_with(Vec::new)
                .push(adapter.adapter_id().to_owned());
            identities
                .entry(manifest.id.clone())
                .or_insert_with(Vec::new)
                .push(adapter.adapter_id().to_owned());
        }
        Self {
            namespaces,
            identities,
        }
    }
}

fn namespace_collision(manifest: &AdapterManifest, claims: &NamespaceClaims) -> Result<(), String> {
    if let Some(adapters) = claims.namespaces.get(&manifest.namespace)
        && adapters.len() > 1
    {
        return Err(format!(
            "namespace `{}` is claimed by adapters {}",
            manifest.namespace,
            adapters.join(", ")
        ));
    }
    if let Some(adapters) = claims.identities.get(&manifest.id)
        && adapters.len() > 1
    {
        return Err(format!(
            "identity `{}` is registered by adapters {}",
            manifest.id,
            adapters.join(", ")
        ));
    }
    Ok(())
}

fn entropy_replay(
    creation: &ConformanceCreation,
    scenario: &Result<Vec<ConformanceOperation>, String>,
    manifest: &AdapterManifest,
    adapter: &dyn ConformanceAdapter,
) -> Result<(), String> {
    let operations = scenario
        .as_ref()
        .map_err(|error| format!("adapter scenario could not be built: {error}"))?;
    let start = creation.initial_state.entropy.clone();
    if start.algorithm != "sha256_counter_v1" {
        return Err(format!(
            "unsupported entropy algorithm `{}` in created state",
            start.algorithm
        ));
    }

    let mut first = EntropyStream::from_state(start.clone()).map_err(failure_detail)?;
    let mut second = EntropyStream::from_state(start.clone()).map_err(failure_detail)?;
    for index in 0..8 {
        let left = first.draw_bounded(1_000).map_err(failure_detail)?;
        let right = second.draw_bounded(1_000).map_err(failure_detail)?;
        if left != right {
            return Err(format!(
                "entropy stream diverged at draw {index} for seed {}: first={left} second={right}",
                start.seed
            ));
        }
    }

    let mut saved_stream = EntropyStream::from_state(start.clone()).map_err(failure_detail)?;
    for _ in 0..3 {
        saved_stream.draw_bounded(1_000).map_err(failure_detail)?;
    }
    let saved = saved_stream.state();
    let expected = [
        saved_stream.draw_bounded(1_000).map_err(failure_detail)?,
        saved_stream.draw_bounded(1_000).map_err(failure_detail)?,
    ];
    let mut restored = EntropyStream::from_state(saved.clone()).map_err(failure_detail)?;
    let replayed = [
        restored.draw_bounded(1_000).map_err(failure_detail)?,
        restored.draw_bounded(1_000).map_err(failure_detail)?,
    ];
    if expected != replayed {
        return Err(format!(
            "saved entropy cursor {} did not resume exactly: expected={expected:?} replayed={replayed:?}",
            saved.cursor
        ));
    }

    let registry = registry_for(adapter, manifest)?;
    let receipts = execute_scenario(&registry, manifest, creation, operations)?;
    if !receipts.iter().any(|receipt| receipt.entropy_consumed > 0) {
        return Err(format!(
            "scenario operations {:?} consumed no entropy",
            operations
                .iter()
                .map(|operation| operation.operation.as_str())
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn save_restore(
    manifest: &AdapterManifest,
    creation: &ConformanceCreation,
    operations: &[ConformanceOperation],
    adapter: &dyn ConformanceAdapter,
) -> Result<(), String> {
    let state = &creation.initial_state;
    let json = state.to_json().map_err(failure_detail)?;
    let restored = TabletopState::from_json(&json)
        .map_err(|error| format!("saved state JSON could not be loaded: {error}"))?;
    if &restored != state {
        return Err("state changed after a JSON save/restore round trip".to_owned());
    }
    validate_tabletop_state(&restored, manifest)
        .map_err(|error| format!("restored state was rejected: {error}"))?;

    let ron = state.to_ron().map_err(failure_detail)?;
    let restored_ron = TabletopState::from_ron(&ron)
        .map_err(|error| format!("saved state RON could not be loaded: {error}"))?;
    if &restored_ron != state {
        return Err("state changed after a RON save/restore round trip".to_owned());
    }

    let registry = registry_for(adapter, manifest)?;
    let original = execute_scenario(&registry, manifest, creation, operations)?;
    let resumed = execute_scenario(
        &registry,
        manifest,
        &ConformanceCreation {
            initial_state: restored,
            ..creation.clone()
        },
        operations,
    )?;
    let original_digest = digest(&original)?;
    let resumed_digest = digest(&resumed)?;
    if original_digest != resumed_digest {
        return Err(format!(
            "restored state did not resume identically: original=`{original_digest}` restored=`{resumed_digest}`"
        ));
    }
    Ok(())
}

fn projection_boundary(
    manifest: &AdapterManifest,
    receipts: &[ResolutionReceipt],
) -> Result<(), String> {
    for receipt in receipts {
        let runtime = project_receipt_for_audience(receipt, manifest, HostAudience::Runtime)
            .map_err(|error| format!("runtime projection failed: {error}"))?;
        let authoring = project_receipt_for_audience(receipt, manifest, HostAudience::Authoring)
            .map_err(|error| format!("authoring projection failed: {error}"))?;
        let authority =
            project_receipt_for_audience(receipt, manifest, HostAudience::AuthorityHost)
                .map_err(|error| format!("authority-host projection failed: {error}"))?;

        if runtime.events.len() != receipt.events.len()
            || authoring.events.len() != receipt.events.len()
        {
            return Err(format!(
                "operation `{}` changed its event count during projection: source={} runtime={} authoring={}",
                receipt.operation,
                receipt.events.len(),
                runtime.events.len(),
                authoring.events.len()
            ));
        }

        for (index, event) in receipt.events.iter().enumerate() {
            let runtime_payload = runtime.events[index].payload.is_some();
            let authoring_payload = authoring.events[index].payload.is_some();
            let authority_payload = authority.events[index].payload.is_some();
            let expected_runtime = manifest.visibility.runtime.contains(&event.visibility);
            let expected_authoring = manifest.visibility.authoring.contains(&event.visibility);
            let expected_authority = manifest
                .visibility
                .authority_host
                .contains(&event.visibility);
            if runtime_payload != expected_runtime
                || authoring_payload != expected_authoring
                || authority_payload != expected_authority
            {
                return Err(format!(
                    "event `{}` visibility `{:?}` projected as runtime={runtime_payload} authoring={authoring_payload} authority={authority_payload}",
                    event.kind, event.visibility
                ));
            }
            for projection in [&runtime, &authoring, &authority] {
                let projected = &projection.events[index];
                if projected.payload_sha256 != event.payload_sha256 {
                    return Err(format!(
                        "event `{}` lost its payload fingerprint during projection: source=`{}` projected=`{}`",
                        event.kind, event.payload_sha256, projected.payload_sha256
                    ));
                }
            }
        }

        let runtime_payloads = runtime
            .events
            .iter()
            .filter(|event| event.payload.is_some())
            .count();
        let authoring_payloads = authoring
            .events
            .iter()
            .filter(|event| event.payload.is_some())
            .count();
        let authority_payloads = authority
            .events
            .iter()
            .filter(|event| event.payload.is_some())
            .count();
        if runtime_payloads > authoring_payloads || authoring_payloads > authority_payloads {
            return Err(format!(
                "projection payloads grew with audience: runtime={runtime_payloads} authoring={authoring_payloads} authority={authority_payloads}"
            ));
        }
        let has_authoring_only = receipt
            .events
            .iter()
            .any(|event| event.visibility == EventVisibility::Authoring);
        if has_authoring_only && runtime_payloads >= authoring_payloads {
            return Err(format!(
                "runtime projection is not more compact than authoring for operation `{}`: runtime={runtime_payloads} authoring={authoring_payloads}",
                receipt.operation
            ));
        }
    }
    Ok(())
}

fn event_shape(manifest: &AdapterManifest, receipts: &[ResolutionReceipt]) -> Result<(), String> {
    for receipt in receipts {
        let Some(operation) = manifest.operations.get(&receipt.operation) else {
            return Err(format!(
                "receipt names undeclared operation `{}`",
                receipt.operation
            ));
        };
        if receipt.events.is_empty() {
            return Err(format!(
                "operation `{}` produced no structured events",
                receipt.operation
            ));
        }
        for (index, event) in receipt.events.iter().enumerate() {
            if event.sequence != index as u64 {
                return Err(format!(
                    "operation `{}` event {index} has sequence {}",
                    receipt.operation, event.sequence
                ));
            }
            if !operation.event_kinds.contains(&event.kind) {
                return Err(format!(
                    "operation `{}` emitted undeclared event kind `{}`",
                    receipt.operation, event.kind
                ));
            }
            if event.capability != receipt.capability {
                return Err(format!(
                    "event `{}` carries capability `{:?}` instead of `{:?}`",
                    event.kind, event.capability, receipt.capability
                ));
            }
            if !manifest
                .visibility
                .authority_host
                .contains(&event.visibility)
            {
                return Err(format!(
                    "event `{}` uses unprojectable visibility `{:?}`",
                    event.kind, event.visibility
                ));
            }
            if !is_lowercase_sha256(&event.payload_sha256) {
                return Err(format!(
                    "event `{}` payload fingerprint `{}` is not a lowercase SHA-256 digest",
                    event.kind, event.payload_sha256
                ));
            }
            let Some(payload) = &event.payload else {
                return Err(format!(
                    "event `{}` has no payload before host projection",
                    event.kind
                ));
            };
            let payload_digest = digest(payload)?;
            if payload_digest != event.payload_sha256 {
                return Err(format!(
                    "event `{}` payload hash mismatch: declared=`{}` observed=`{payload_digest}`",
                    event.kind, event.payload_sha256
                ));
            }
            let Some(value_type) = manifest.event_types.get(&event.kind) else {
                return Err(format!(
                    "event `{}` has no declared payload type",
                    event.kind
                ));
            };
            validate_typed_value("events.payload", payload, value_type, &manifest.types)
                .map_err(|error| format!("event `{}` payload is invalid: {error}", event.kind))?;
        }
    }
    Ok(())
}

fn unknown_capability(
    manifest: &AdapterManifest,
    creation: &ConformanceCreation,
    operations: &[ConformanceOperation],
    adapter: &dyn ConformanceAdapter,
) -> Result<(), String> {
    let operation = operations
        .first()
        .ok_or_else(|| "adapter scenario declares no operation to probe".to_owned())?;
    let capability = ALL_CAPABILITIES
        .iter()
        .copied()
        .find(|capability| !has_capability(manifest, *capability))
        .or_else(|| {
            ALL_CAPABILITIES
                .iter()
                .copied()
                .find(|capability| *capability != operation.capability)
        })
        .ok_or_else(|| "manifest cannot express an undeclared capability".to_owned())?;

    let request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "conformance_unknown_capability".to_owned(),
        adapter: creation.adapter.clone(),
        operation: operation.operation.clone(),
        capability,
        definition_sha256: creation.definition.definition_sha256.clone(),
        definition: creation.definition.definition.clone(),
        expected_state_sha256: digest(&creation.initial_state)?,
        input: operation.input.clone(),
    };
    let registry = registry_for(adapter, manifest)?;
    match registry.execute(manifest, &request, &creation.initial_state) {
        Err(TabletopError::UndeclaredCapability { .. }) => {}
        Err(other) => {
            return Err(format!(
                "capability `{capability:?}` was rejected with `{other}` instead of UndeclaredCapability"
            ));
        }
        Ok(_) => {
            return Err(format!(
                "capability `{capability:?}` was accepted for operation `{}`",
                operation.operation
            ));
        }
    }

    let mut entropy = EntropyStream::from_state(creation.initial_state.entropy.clone())
        .map_err(failure_detail)?;
    let outcome = adapter.resolver().resolve(
        UNDECLARED_OPERATION,
        &creation.definition.definition,
        &operation.input,
        &creation.initial_state.value,
        &mut entropy,
    );
    match outcome {
        Err(_) => Ok(()),
        Ok(_) => Err(format!(
            "resolver accepted undeclared operation `{UNDECLARED_OPERATION}`; capability `{capability:?}` is not declared"
        )),
    }
}

fn unsupported_version(
    manifest: &AdapterManifest,
    creation: &ConformanceCreation,
) -> Result<(), String> {
    let mut future_manifest = manifest.clone();
    future_manifest.manifest_format_version = u32::MAX;
    match validate_adapter_manifest(&future_manifest) {
        Err(TabletopError::UnsupportedVersion {
            path: "manifest_format_version",
        }) => {}
        Err(other) => {
            return Err(format!(
                "manifest version {} was rejected with `{other}`",
                u32::MAX
            ));
        }
        Ok(()) => {
            return Err(format!(
                "manifest version {} was accepted instead of rejected (expected {ADAPTER_MANIFEST_FORMAT_VERSION})",
                u32::MAX
            ));
        }
    }

    let mut future_state = creation.initial_state.clone();
    future_state.state_format_version = u32::MAX;
    match validate_tabletop_state(&future_state, manifest) {
        Err(TabletopError::UnsupportedVersion {
            path: "state_format_version",
        }) => {}
        Err(other) => {
            return Err(format!(
                "state version {} was rejected with `{other}`",
                u32::MAX
            ));
        }
        Ok(()) => {
            return Err(format!(
                "state version {} was accepted instead of rejected",
                u32::MAX
            ));
        }
    }

    let mut future_state = creation.initial_state.clone();
    future_state.entropy.algorithm = "future_entropy_v9".to_owned();
    match validate_tabletop_state(&future_state, manifest) {
        Err(TabletopError::UnsupportedVersion {
            path: "entropy.algorithm",
        }) => {}
        Err(other) => {
            return Err(format!(
                "entropy algorithm `future_entropy_v9` was rejected with `{other}`"
            ));
        }
        Ok(()) => {
            return Err(
                "entropy algorithm `future_entropy_v9` was accepted instead of rejected".to_owned(),
            );
        }
    }

    let future_request = ResolutionRequest {
        resolver_format_version: u32::MAX,
        request_id: "conformance_future_request".to_owned(),
        adapter: creation.adapter.clone(),
        operation: "conformance_future_operation".to_owned(),
        capability: TabletopCapability::ChecksAndConflicts,
        definition_sha256: creation.definition.definition_sha256.clone(),
        definition: creation.definition.definition.clone(),
        expected_state_sha256: digest(&creation.initial_state)?,
        input: DomainValue::Null,
    };
    let outcome = validate_resolution_request(&future_request, &creation.initial_state, manifest);
    match outcome {
        Err(TabletopError::UnsupportedVersion {
            path: "resolver_format_version",
        }) => Ok(()),
        Err(other) => Err(format!(
            "resolver version {} was rejected with `{other}`",
            future_request.resolver_format_version
        )),
        Ok(()) => Err(format!(
            "resolver version {} was accepted instead of rejected",
            future_request.resolver_format_version
        )),
    }
}

fn malformed_state(
    manifest: &AdapterManifest,
    creation: &ConformanceCreation,
) -> Result<(), String> {
    if TabletopState::from_json("{\"state_format_version\": 1}").is_ok() {
        return Err("state JSON with missing fields was accepted".to_owned());
    }
    if TabletopState::from_json("{").is_ok() {
        return Err("unterminated state JSON was accepted".to_owned());
    }

    let mut forged_coordinate = creation.initial_state.clone();
    forged_coordinate.adapter.content_sha256 = "0".repeat(64);
    match validate_tabletop_state(&forged_coordinate, manifest) {
        Err(TabletopError::ContentHashMismatch { path: "adapter" }) => {}
        Err(other) => {
            return Err(format!(
                "forged adapter coordinate was rejected with `{other}`"
            ));
        }
        Ok(()) => {
            return Err(format!(
                "forged adapter digest `{}` was accepted",
                forged_coordinate.adapter.content_sha256
            ));
        }
    }

    let mut forged_value = creation.initial_state.clone();
    forged_value.value = DomainValue::Null;
    match validate_tabletop_state(&forged_value, manifest) {
        Err(TabletopError::SchemaMismatch { path: "value" }) => Ok(()),
        Err(other) => Err(format!("untyped state value was rejected with `{other}`")),
        Ok(()) => Err("untyped state value was accepted".to_owned()),
    }
}

fn deterministic_output(
    manifest: &AdapterManifest,
    creation: &ConformanceCreation,
    operations: &[ConformanceOperation],
) -> Result<(), String> {
    let first_json = manifest.to_json().map_err(failure_detail)?;
    let second_json = manifest.to_json().map_err(failure_detail)?;
    if first_json != second_json {
        return Err(format!(
            "manifest JSON changed between serializations: first=`{}` second=`{}`",
            short_hash(&first_json),
            short_hash(&second_json)
        ));
    }
    let reparsed = AdapterManifest::from_json(&first_json).map_err(failure_detail)?;
    let replayed = reparsed.to_json().map_err(failure_detail)?;
    if replayed != first_json {
        return Err(format!(
            "manifest JSON did not round-trip byte-for-byte: before=`{}` after=`{}`",
            short_hash(&first_json),
            short_hash(&replayed)
        ));
    }
    if AdapterManifest::from_json(&replayed).map_err(failure_detail)? != reparsed {
        return Err("manifest JSON parsed to a different value on the second parse".to_owned());
    }
    let ron = manifest.to_ron().map_err(failure_detail)?;
    if AdapterManifest::from_ron(&ron).map_err(failure_detail)? != *manifest {
        return Err("manifest RON did not round-trip to the same manifest".to_owned());
    }

    let first_state = creation.initial_state.to_json().map_err(failure_detail)?;
    let second_state = creation.initial_state.to_json().map_err(failure_detail)?;
    if first_state != second_state {
        return Err(format!(
            "state JSON changed between serializations: first=`{}` second=`{}`",
            short_hash(&first_state),
            short_hash(&second_state)
        ));
    }
    let reparsed_state = TabletopState::from_json(&first_state).map_err(failure_detail)?;
    if reparsed_state.to_json().map_err(failure_detail)? != first_state {
        return Err("state JSON did not round-trip byte-for-byte".to_owned());
    }

    if creation.canonical.is_empty() {
        return Err(format!(
            "creation canonical output is empty for operations {:?}",
            operations
                .iter()
                .map(|operation| operation.operation.as_str())
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}
