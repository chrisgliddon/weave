use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, TypeExpression};

use crate::TabletopError;

/// Current tabletop adapter manifest format.
pub const ADAPTER_MANIFEST_FORMAT_VERSION: u32 = 1;
/// Current selection artifact format.
pub const ADAPTER_SELECTION_FORMAT_VERSION: u32 = 1;
/// Current character projection format.
pub const CHARACTER_PROJECTION_FORMAT_VERSION: u32 = 1;
/// Current mutable state format.
pub const ADAPTER_STATE_FORMAT_VERSION: u32 = 1;
/// Current resolver request and receipt format.
pub const RESOLVER_FORMAT_VERSION: u32 = 1;
/// Current host resolver trait contract.
pub const RESOLVER_CONTRACT_VERSION: u32 = 1;
/// Current explicit switch request and audit format.
pub const ADAPTER_SWITCH_FORMAT_VERSION: u32 = 1;

/// Closed capability vocabulary shared by syntax, editor, resolver, and host integrations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TabletopCapability {
    CharacterCreation,
    DerivedValues,
    ChecksAndConflicts,
    ResourcesAndConditions,
    EquipmentAndAbilities,
    Advancement,
    Encounters,
    Clocks,
    Scenes,
    Factions,
    WorldState,
    CampaignState,
}

/// One supported capability contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabletopCapabilityDeclaration {
    pub capability: TabletopCapability,
    pub version: u32,
}

/// Deliberately narrow extension surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum AdapterExtensionSurface {
    /// Packs are inert data. A host may explicitly compile and register trusted resolver code.
    DeclarativeDataWithRegisteredResolver { resolver_contract_version: u32 },
}

/// Authority of one creation field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreationFieldAuthority {
    AdapterOwned,
    CanonicalSuggestion,
}

/// One field in a generic editor creation panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreationField {
    pub id: String,
    pub label: String,
    pub description: String,
    pub value_type: TypeExpression,
    pub required: bool,
    pub authority: CreationFieldAuthority,
}

/// One ordered generic character-creation panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreationStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub required_capability: TabletopCapability,
    pub fields: Vec<CreationField>,
}

/// One resolver operation declared without host-specific types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolverOperationDeclaration {
    pub id: String,
    pub title: String,
    pub description: String,
    pub required_capability: TabletopCapability,
    pub request_type: TypeExpression,
    pub event_kinds: Vec<String>,
    pub consumes_entropy: bool,
}

/// Visibility assigned to one structured event payload.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EventVisibility {
    Public,
    Authoring,
    HostOnly,
}

/// Consumer class used when projecting event payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAudience {
    Runtime,
    Authoring,
    AuthorityHost,
}

/// Explicit visibility matrix. Validators require the conservative v1 matrix exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostVisibilityPolicy {
    pub runtime: Vec<EventVisibility>,
    pub authoring: Vec<EventVisibility>,
    pub authority_host: Vec<EventVisibility>,
}

impl Default for HostVisibilityPolicy {
    fn default() -> Self {
        Self {
            runtime: vec![EventVisibility::Public],
            authoring: vec![EventVisibility::Public, EventVisibility::Authoring],
            authority_host: vec![
                EventVisibility::Public,
                EventVisibility::Authoring,
                EventVisibility::HostOnly,
            ],
        }
    }
}

/// A reviewed migration advertised by an adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterMigrationDeclaration {
    pub id: String,
    pub from_adapter_id: String,
    pub from_version: String,
    pub from_schema_version: u32,
    pub to_schema_version: u32,
    pub reviewed: bool,
    pub preserves_canonical_character: bool,
    pub description: String,
}

/// Why a provenance source is eligible for distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSourceClass {
    Original,
    VerifiedCc0,
    SeparatelyLicensedApache2,
}

/// Material categories that must remain outside every adapter bundle unless separately reviewed.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExcludedMaterial {
    CommunitySupplements,
    Logos,
    TradeDress,
    Artwork,
    Layout,
    UnverifiedAssets,
}

/// Exact redistributed license text or notice file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LicenseTextReference {
    pub artifact: String,
    pub sha256: String,
}

/// One additional official artifact covered by the adapter's public-source boundary.
///
/// The primary artifact remains in [`AdapterProvenance`] for format-v1 compatibility. This
/// record pins companion rules, sheets, notices, or other reviewed inputs independently so a
/// changed download cannot silently enter an adapter release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterSourceArtifact {
    pub source_url: String,
    pub exact_artifact: String,
    pub revision: String,
    pub retrieved_on: String,
    pub sha256: String,
    pub media_type: String,
    pub purpose: String,
    pub redistributed: bool,
}

/// Conservative provenance record for an adapter's complete public source boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterProvenance {
    pub source_class: AdapterSourceClass,
    pub source_url: String,
    pub exact_artifact: String,
    pub revision: String,
    pub retrieved_on: String,
    pub sha256: String,
    pub license: String,
    pub license_url: String,
    pub covered_files_or_sections: Vec<String>,
    pub exclusions: Vec<ExcludedMaterial>,
    pub attribution: String,
    pub required_license_text: LicenseTextReference,
    /// Additional exact source artifacts, ordered by `exact_artifact`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_artifacts: Vec<AdapterSourceArtifact>,
    pub notices: Vec<String>,
    pub compatibility_statement: String,
}

/// Complete portable adapter contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
    pub manifest_format_version: u32,
    pub id: String,
    pub version: String,
    pub schema_version: u32,
    pub namespace: String,
    pub title: String,
    pub compatibility_label: String,
    pub summary: String,
    pub weave_version: String,
    pub character_contract_version: String,
    pub extension_surface: AdapterExtensionSurface,
    pub capabilities: Vec<TabletopCapabilityDeclaration>,
    pub types: BTreeMap<String, TypeExpression>,
    pub definition_type: TypeExpression,
    pub state_type: TypeExpression,
    pub creation_steps: Vec<CreationStep>,
    pub operations: BTreeMap<String, ResolverOperationDeclaration>,
    pub event_types: BTreeMap<String, TypeExpression>,
    pub visibility: HostVisibilityPolicy,
    pub migrations: Vec<AdapterMigrationDeclaration>,
    pub provenance: AdapterProvenance,
}

/// Exact installed adapter coordinate embedded in compiled data.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedAdapter {
    pub id: String,
    pub version: String,
    pub schema_version: u32,
    pub content_sha256: String,
}

/// Project-level adapter installation and primary selection.
///
/// `primary` is a vector only so malformed/conflicting input can be represented and diagnosed;
/// validation permits zero or one entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterSelection {
    pub selection_format_version: u32,
    pub installed: Vec<ResolvedAdapter>,
    pub primary: Vec<ResolvedAdapter>,
}

/// Review state for a suggestion that targets canonical Character data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionDecision {
    Proposed,
    Accepted,
    Rejected,
    Overridden,
    Withheld,
}

/// Explainable cross-domain suggestion. It is lineage only and never writes the target itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanonicalSuggestion {
    pub id: String,
    pub target_path: Vec<String>,
    pub proposed_value: DomainValue,
    pub explanation: String,
    pub source_paths: Vec<String>,
    pub decision: SuggestionDecision,
    pub rationale: Option<String>,
}

/// Active immutable adapter-owned character definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterCharacterDefinition {
    pub adapter: ResolvedAdapter,
    pub definition: DomainValue,
    pub definition_sha256: String,
    pub completed_creation_steps: Vec<String>,
}

/// Inactive adapter data retained without conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArchivedAdapterProjection {
    pub adapter: ResolvedAdapter,
    pub definition: DomainValue,
    pub definition_sha256: String,
    /// Exact creation-step completion retained with the immutable definition.
    #[serde(default)]
    pub completed_creation_steps: Vec<String>,
    pub reason: String,
}

/// System-neutral Character reference plus isolated adapter projections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabletopCharacterProjection {
    pub projection_format_version: u32,
    pub character_id: String,
    pub canonical_profile_sha256: String,
    pub canonical_character_write_back: bool,
    pub active: Option<AdapterCharacterDefinition>,
    pub inactive: BTreeMap<String, ArchivedAdapterProjection>,
    pub suggestions: Vec<CanonicalSuggestion>,
}

/// Serializable deterministic entropy cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntropyState {
    pub algorithm: String,
    pub seed: u64,
    pub cursor: u64,
}

/// Adapter-owned mutable state kept outside the immutable definition and Character canon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabletopState {
    pub state_format_version: u32,
    pub adapter: ResolvedAdapter,
    pub owner_id: String,
    pub definition_sha256: String,
    pub revision: u64,
    pub entropy: EntropyState,
    pub value: DomainValue,
}

/// One host-neutral request to a registered resolver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolutionRequest {
    pub resolver_format_version: u32,
    pub request_id: String,
    pub adapter: ResolvedAdapter,
    pub operation: String,
    pub capability: TabletopCapability,
    pub definition_sha256: String,
    pub definition: DomainValue,
    pub expected_state_sha256: String,
    pub input: DomainValue,
}

/// Unnumbered event returned by trusted resolver code.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolverEvent {
    pub kind: String,
    pub visibility: EventVisibility,
    pub payload: DomainValue,
}

/// Resolver-owned output before the contract layer validates and fingerprints it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolverOutput {
    pub state: DomainValue,
    pub events: Vec<ResolverEvent>,
}

/// Structured event envelope. Unauthorized projections omit payload but retain its fingerprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabletopEvent {
    pub sequence: u64,
    pub kind: String,
    pub capability: TabletopCapability,
    pub visibility: EventVisibility,
    pub payload_sha256: String,
    pub payload: Option<DomainValue>,
}

/// Atomic result of one state transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolutionReceipt {
    pub resolver_format_version: u32,
    pub adapter: ResolvedAdapter,
    pub operation: String,
    pub capability: TabletopCapability,
    pub request_sha256: String,
    pub before_state_sha256: String,
    pub before_revision: u64,
    pub entropy_before: EntropyState,
    pub after_state_sha256: String,
    pub entropy_consumed: u64,
    pub after_state: TabletopState,
    pub events: Vec<TabletopEvent>,
}

/// Who or what initiated an explicit switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SwitchInitiator {
    /// The character author made the decision directly.
    Author,
    /// The authoring or editing host performed a reviewed action.
    Editor,
    /// A runtime host applied an explicit, pre-approved action.
    Host,
    /// An import or restore workflow applied the change.
    Import,
}

/// Disposition of the outgoing active projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SwitchOutgoingPolicy {
    /// Preserve the outgoing definition under its exact adapter namespace.
    Archive,
    /// Remove the outgoing definition after an explicit decision.
    Discard,
}

/// Kinds of author decisions a switch can require before it applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SwitchDecisionKind {
    DiscardCurrentProjection,
    AcceptCapabilityLoss,
    AcceptNonTransferableFields,
    ResolveIncompatibleResources,
    ResolvePendingConflicts,
    ProvideTargetProjection,
    ApplyReviewedMigration,
}

/// One explicit decision that must be acknowledged before applying a switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwitchDecision {
    pub id: String,
    pub kind: SwitchDecisionKind,
    pub required: bool,
    pub description: String,
}

/// Severity of one non-mutating switch diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SwitchDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

/// One redaction-safe switch diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwitchDiagnostic {
    /// Stable contract code, for example `TT101`.
    pub code: String,
    pub severity: SwitchDiagnosticSeverity,
    pub message: String,
}

/// One archived or restored projection cited by a preview or audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwitchArchiveReport {
    pub adapter: ResolvedAdapter,
    pub namespace: String,
    pub definition_sha256: String,
}

/// Non-mutating switch analysis. Applying a switch is a later explicit reviewed operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterSwitchPreview {
    pub canonical_profile_sha256: String,
    pub from: Option<ResolvedAdapter>,
    pub to: Option<ResolvedAdapter>,
    pub archive_current: bool,
    pub restore_existing_archive: bool,
    pub automatic_conversion: bool,
    pub reviewed_migration_id: Option<String>,
    pub warnings: Vec<String>,
    /// True when the target coordinate was resolved from an installed manifest.
    pub target_installed: bool,
    /// Capabilities declared only by the target adapter.
    pub capabilities_gained: Vec<TabletopCapability>,
    /// Capabilities declared by the active adapter but absent from the target.
    pub capabilities_lost: Vec<TabletopCapability>,
    /// Definition paths that cannot transfer across systems.
    pub non_transferable_fields: Vec<String>,
    /// Active mutable resource paths that cannot transfer across systems.
    pub incompatible_active_resources: Vec<String>,
    /// Conditions the author must resolve or acknowledge before applying.
    pub pending_conflicts: Vec<String>,
    /// Outgoing projection that will be preserved in the archive.
    pub archive: Option<SwitchArchiveReport>,
    /// Exact archived projection that will be restored.
    pub restore: Option<SwitchArchiveReport>,
    /// Inactive namespaces explicitly discarded by the request.
    pub discard_inactive: Vec<String>,
    /// Inactive namespaces preserved unchanged by the request.
    pub retained_inactive: Vec<String>,
    /// Fingerprint of the supplied outgoing mutable state, never its contents.
    pub active_state_sha256: Option<String>,
    /// True when the host must archive the outgoing mutable state.
    pub state_archive_required: bool,
    /// Every author decision required before the switch can apply.
    pub required_decisions: Vec<SwitchDecision>,
    /// Dry-run and incompatibility diagnostics, including migration checks.
    pub diagnostics: Vec<SwitchDiagnostic>,
}

/// One explicit, serializable switch request. Accepted decisions must match a preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterSwitchRequest {
    pub switch_format_version: u32,
    /// Target release, or `None` to deactivate tabletop rules.
    pub target: Option<ResolvedAdapter>,
    pub initiator: SwitchInitiator,
    /// Stable namespaced actor reference, never a credential.
    pub initiator_id: String,
    pub outgoing_policy: SwitchOutgoingPolicy,
    /// Inactive namespaces to discard explicitly. Sorted and unique.
    pub discard_inactive: Vec<String>,
    /// Accepted decision ids from the matching preview. Sorted and unique.
    pub accepted_decisions: Vec<String>,
    /// Independently created or migrated target projection. Never converted automatically.
    pub target_definition: Option<AdapterCharacterDefinition>,
}

/// Immutable audit record for one applied switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterSwitchAudit {
    pub switch_format_version: u32,
    pub character_id: String,
    pub canonical_profile_sha256: String,
    pub canonical_character_write_back: bool,
    pub initiator: SwitchInitiator,
    pub initiator_id: String,
    pub from: Option<ResolvedAdapter>,
    pub to: Option<ResolvedAdapter>,
    pub accepted_decisions: Vec<String>,
    pub diagnostics: Vec<SwitchDiagnostic>,
    pub archived_projection: Option<SwitchArchiveReport>,
    pub restored_projection: Option<SwitchArchiveReport>,
    pub discarded_current: bool,
    pub discarded_inactive: Vec<String>,
    pub retained_inactive: Vec<String>,
    pub preserved_state_sha256: Option<String>,
    pub discarded_state_sha256: Option<String>,
    pub migration_id: Option<String>,
    /// Definition fingerprint replaced by a reviewed same-adapter migration.
    pub migrated_from_definition_sha256: Option<String>,
    /// Fingerprint of the committed projection.
    pub result_projection_sha256: String,
}

macro_rules! impl_switch_io {
    ($type:ty) => {
        impl $type {
            /// Parse strict JSON and reject duplicate object keys.
            pub fn from_json(source: &str) -> Result<Self, TabletopError> {
                weave_domain::parse_strict_json(source).map_err(|_| TabletopError::Artifact)
            }

            /// Parse one RON artifact.
            pub fn from_ron(source: &str) -> Result<Self, TabletopError> {
                ron::from_str(source).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize stable human-readable JSON with a trailing newline.
            pub fn to_json(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_json(self).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize stable human-readable RON with a trailing newline.
            pub fn to_ron(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_ron(self).map_err(|_| TabletopError::Artifact)
            }
        }
    };
}

impl_switch_io!(AdapterSwitchRequest);
impl_switch_io!(AdapterSwitchAudit);

impl TabletopCharacterProjection {
    /// The only definition that may affect expressions, events, or panels.
    #[must_use]
    pub fn active_definition(&self) -> Option<&AdapterCharacterDefinition> {
        self.active.as_ref()
    }

    /// Resolve one adapter namespace for evaluation. Inactive archives never match.
    #[must_use]
    pub fn projection_for_namespace(&self, namespace: &str) -> Option<&AdapterCharacterDefinition> {
        self.active
            .as_ref()
            .filter(|active| active.adapter.namespace_key() == namespace)
    }

    /// Exact inactive archive for one installed release, if retained.
    #[must_use]
    pub fn archived_projection(
        &self,
        adapter: &ResolvedAdapter,
    ) -> Option<&ArchivedAdapterProjection> {
        self.inactive
            .get(&adapter.namespace_key())
            .filter(|archived| &archived.adapter == adapter)
    }
}
