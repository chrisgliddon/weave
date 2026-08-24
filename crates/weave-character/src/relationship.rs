//! Deterministic, provenance-aware Character relationship graphs.
//!
//! Relationship kinds are immutable data packs. Graph proposals use only explicitly selected
//! roster members and evidence rules, retain every contribution, and remain advisory until a
//! complete author review is replayed. Computed affinity is always labeled as a subjective
//! authoring aid and cannot write back into personality, identity, alignment, date context, or
//! ruleset-owned state.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    Provenance, ProvenanceKind, ProvenanceTransformation, parse_strict_json, to_pretty_json,
    to_pretty_ron, validate_provenance,
};

use crate::synthesis::merge_provenance;
use crate::validation::{
    trait_path, trait_value, validate_local_id, validate_namespaced_id, validate_semver,
    validate_sha256, validate_text,
};
use crate::{
    BirthDate, CharacterCollection, CharacterExtension, CharacterProfile, Confidence,
    DiagnosticSeverity, ExtensionHeader, ExtensionWriteBack, Freshness, HexacoTrait, LockState,
    PreferencePolarity, RELATIONSHIP_EXTENSION_NAMESPACE, RelationshipConsent,
    RelationshipConsentState, RelationshipDate, RelationshipEdge, RelationshipEdgeOrigin,
    RelationshipEdges, RelationshipEvidenceContribution, RelationshipEvidenceKind,
    RelationshipKindPackRef, RelationshipNote, RelationshipSafeguardException,
    RelationshipValidityPeriod, ReviewState, TraitMeasurement, ValueState, VersionedExtension,
    collection_fingerprint, validate_character_collection,
};

/// Current immutable relationship-kind pack format.
pub const RELATIONSHIP_KIND_PACK_FORMAT_VERSION: u32 = 1;
/// Current proposal-configuration format.
pub const RELATIONSHIP_CONFIG_FORMAT_VERSION: u32 = 1;
/// Current deterministic proposal format.
pub const RELATIONSHIP_PROPOSAL_FORMAT_VERSION: u32 = 1;
/// Current editorial review format.
pub const RELATIONSHIP_REVIEW_FORMAT_VERSION: u32 = 1;
/// Current independently reproducible receipt format.
pub const RELATIONSHIP_RECEIPT_FORMAT_VERSION: u32 = 1;
/// Current direct graph-revision format.
pub const RELATIONSHIP_REVISION_FORMAT_VERSION: u32 = 1;
/// Current reconciliation report format.
pub const RELATIONSHIP_RECONCILIATION_FORMAT_VERSION: u32 = 1;
/// Current graph-inspection policy format.
pub const RELATIONSHIP_POLICY_FORMAT_VERSION: u32 = 1;

const PACK_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-kind-pack:1";
const CONFIG_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-config:1";
const PROPOSAL_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-proposal:1";
const REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-review:1";
const RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-receipt:1";
const REVISION_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-revision:1";
const RECONCILIATION_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-reconciliation:1";
const POLICY_SCHEMA_ID: &str = "urn:weave:schema:character-relationship-policy:1";

const MICROS: i64 = 1_000_000;

/// Immutable, independently distributable vocabulary and semantics for relationship edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipKindPack {
    pub pack_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub independently_authored: bool,
    pub license: String,
    pub license_url: String,
    pub kinds: BTreeMap<String, RelationshipKindDefinition>,
    pub provenance: Provenance,
}

/// One relationship kind and the structural metadata it requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipKindDefinition {
    pub id: String,
    pub label: String,
    pub description: String,
    pub family: RelationshipKindFamily,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinship_semantics: Option<RelationshipKinshipSemantics>,
    pub directionality: RelationshipDirectionality,
    pub allows_self: bool,
    pub allows_multiple_concurrent: bool,
    pub required_metadata: Vec<RelationshipMetadataRequirement>,
    pub limitations: Vec<String>,
}

/// Open-pack-compatible baseline families used by safeguards and presentation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKindFamily {
    Kinship,
    Friendship,
    Mentorship,
    Rivalry,
    Partnership,
    Affinity,
    Narrative,
    Custom,
}

/// Explicit pedigree meaning used for cycle and contradiction checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKinshipSemantics {
    ParentOf,
    ChildOf,
    SiblingOf,
    Other,
}

/// Direction semantics declared by a kind pack rather than inferred from labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "mode", deny_unknown_fields)]
pub enum RelationshipDirectionality {
    Directed,
    Symmetric,
    InversePaired { inverse_kind_id: String },
}

/// Metadata a kind requires on every applied edge.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipMetadataRequirement {
    Confidence,
    Validity,
    Notes,
    Consent,
    Evidence,
}

/// Exact, replayable configuration for deterministic relationship proposals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipProposalConfig {
    pub config_format_version: u32,
    pub id: String,
    pub expected_input_sha256: String,
    pub kind_pack: RelationshipKindPackRef,
    pub reference_date: RelationshipDate,
    pub seed: u64,
    /// Exact sorted roster. No ambient character discovery is permitted.
    pub roster: Vec<String>,
    /// Proposal targets are processed in declared order and must have unique ids.
    pub targets: Vec<RelationshipProposalTarget>,
    /// Evidence rules are processed in declared order and must have unique ids.
    pub evidence_rules: Vec<RelationshipEvidenceRule>,
    /// Explicit pair-specific consent assertions; absence never means consent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consent_records: Vec<RelationshipConsentRecord>,
    pub safeguards: RelationshipSafeguards,
    pub provenance: Provenance,
}

/// One relationship kind and advisory layer requested from the scorer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipProposalTarget {
    pub id: String,
    pub kind_id: String,
    pub origin: RelationshipEdgeOrigin,
    pub minimum_score_micros: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_candidates: Option<u32>,
    pub confidence: Confidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validity: Option<RelationshipValidityPeriod>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub notes: BTreeMap<String, RelationshipNote>,
    pub rationale: String,
}

/// One configurable, inspectable contribution to the advisory affinity score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum RelationshipEvidenceRule {
    TraitSimilarity {
        id: String,
        trait_id: HexacoTrait,
        weight_micros: i32,
    },
    PreferenceOverlap {
        id: String,
        category: String,
        weight_micros: i32,
    },
    SharedContext {
        id: String,
        context_ref: String,
        weight_micros: i32,
    },
    ExistingCanon {
        id: String,
        relationship_kind_id: String,
        weight_micros: i32,
    },
}

impl RelationshipEvidenceRule {
    fn id(&self) -> &str {
        match self {
            Self::TraitSimilarity { id, .. }
            | Self::PreferenceOverlap { id, .. }
            | Self::SharedContext { id, .. }
            | Self::ExistingCanon { id, .. } => id,
        }
    }

    const fn kind(&self) -> RelationshipEvidenceKind {
        match self {
            Self::TraitSimilarity { .. } => RelationshipEvidenceKind::TraitSimilarity,
            Self::PreferenceOverlap { .. } => RelationshipEvidenceKind::PreferenceOverlap,
            Self::SharedContext { .. } => RelationshipEvidenceKind::SharedContext,
            Self::ExistingCanon { .. } => RelationshipEvidenceKind::ExistingCanon,
        }
    }

    const fn weight_micros(&self) -> i32 {
        match self {
            Self::TraitSimilarity { weight_micros, .. }
            | Self::PreferenceOverlap { weight_micros, .. }
            | Self::SharedContext { weight_micros, .. }
            | Self::ExistingCanon { weight_micros, .. } => *weight_micros,
        }
    }
}

/// Exact author-reviewed consent record for one pair and kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipConsentRecord {
    pub id: String,
    pub source_character_id: String,
    pub target_character_id: String,
    pub kind_id: String,
    pub consent: RelationshipConsent,
}

/// Project-defined age, kinship, partnership, and consent policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipSafeguards {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_partnership_age_years: Option<u16>,
    pub forbid_close_kin_partnership: bool,
    pub require_affirmed_partnership_consent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_concurrent_partnerships: Option<u16>,
    pub allow_reviewed_exceptions: bool,
}

/// Exact project policy used by list, inspect, validation, reconciliation, and CSV views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipGraphPolicy {
    pub policy_format_version: u32,
    pub reference_date: RelationshipDate,
    pub safeguards: RelationshipSafeguards,
}

/// Complete immutable dry-run manifest with all replay inputs and candidate traces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipProposal {
    pub proposal_format_version: u32,
    pub id: String,
    pub input_collection: CharacterCollection,
    pub input_sha256: String,
    pub kind_pack: RelationshipKindPack,
    pub kind_pack_ref: RelationshipKindPackRef,
    pub config: RelationshipProposalConfig,
    pub config_sha256: String,
    pub roster_sha256: String,
    pub profile_sha256: BTreeMap<String, String>,
    /// Candidate map is byte-stably keyed by deterministic candidate id.
    pub candidates: BTreeMap<String, RelationshipCandidate>,
    pub distribution: RelationshipProposalDistribution,
}

/// One pair/kind candidate. The original edges and evidence never change during review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipCandidate {
    pub id: String,
    pub target_id: String,
    pub source_character_id: String,
    pub target_character_id: String,
    pub kind_id: String,
    pub origin: RelationshipEdgeOrigin,
    pub score_micros: u32,
    pub disposition: RelationshipCandidateDisposition,
    pub rank: u32,
    pub seeded_sha256: String,
    pub evidence: Vec<RelationshipEvidenceContribution>,
    pub edges: Vec<RelationshipEdge>,
    pub safeguards: Vec<RelationshipDiagnostic>,
    pub limitations: Vec<String>,
    pub rationale: String,
}

/// Whether a candidate crossed the threshold, capacity, and safety gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipCandidateDisposition {
    Proposed,
    BelowThreshold,
    CapacityWithheld,
    SafeguardBlocked,
}

/// Deterministic candidate counts grouped by target and disposition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipProposalDistribution {
    pub by_target: BTreeMap<String, BTreeMap<String, u32>>,
}

/// Complete author decision map. Every proposal candidate requires one decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipReview {
    pub review_format_version: u32,
    pub proposal_sha256: String,
    pub reviewer: String,
    pub rationale: String,
    pub decisions: BTreeMap<String, RelationshipReviewDecision>,
}

/// Explicit editorial disposition for one immutable relationship candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "decision", deny_unknown_fields)]
pub enum RelationshipReviewDecision {
    Accept {
        lock: LockState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    Edit {
        edges: Vec<RelationshipEdge>,
        lock: LockState,
        rationale: String,
    },
    Override {
        edges: Vec<RelationshipEdge>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        replacements: Vec<RelationshipEdgeRemoval>,
        lock: LockState,
        rationale: String,
    },
    Exception {
        edges: Vec<RelationshipEdge>,
        exception_codes: Vec<RelationshipDiagnosticCode>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        replacements: Vec<RelationshipEdgeRemoval>,
        lock: LockState,
        rationale: String,
    },
    Reject {
        rationale: String,
    },
    Withhold {
        rationale: String,
    },
}

/// Atomic application proof retaining original candidates and all review decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipReceipt {
    pub receipt_format_version: u32,
    pub id: String,
    pub proposal: RelationshipProposal,
    pub review: RelationshipReview,
    pub proposal_sha256: String,
    pub review_sha256: String,
    pub applied_edges: BTreeMap<String, Vec<RelationshipAppliedEdge>>,
    pub output_collection: CharacterCollection,
    pub output_sha256: String,
}

/// One edge written by an accepted review decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipAppliedEdge {
    pub owner_character_id: String,
    pub edge: RelationshipEdge,
}

/// Direct authored/imported graph revision used by source, editor, and CLI surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipGraphRevision {
    pub revision_format_version: u32,
    pub id: String,
    pub expected_input_sha256: String,
    pub kind_pack: RelationshipKindPackRef,
    pub reference_date: RelationshipDate,
    pub safeguards: RelationshipSafeguards,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additions: Vec<RelationshipEdge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removals: Vec<RelationshipEdgeRemoval>,
    pub rationale: String,
    pub provenance: Provenance,
}

/// Fingerprinted edge removal; locked edges require a deliberate reviewed override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipEdgeRemoval {
    pub owner_character_id: String,
    pub edge_id: String,
    pub expected_edge_sha256: String,
    pub override_locked: bool,
}

/// Read-only graph filter shared by CLI and editor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_character_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_character_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub origins: Vec<RelationshipEdgeOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_on: Option<RelationshipDate>,
}

/// Stable list/inspect record including owning profile and kind semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipEdgeView {
    pub owner_character_id: String,
    pub edge: RelationshipEdge,
    pub family: RelationshipKindFamily,
    pub directionality: RelationshipDirectionality,
}

/// Redaction-safe project diagnostic codes for relationship reconciliation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum RelationshipDiagnosticCode {
    #[serde(rename = "R100")]
    MissingCharacter,
    #[serde(rename = "R101")]
    ForbiddenSelfEdge,
    #[serde(rename = "R102")]
    BrokenInverse,
    #[serde(rename = "R103")]
    InvalidDateRange,
    #[serde(rename = "R104")]
    ContradictoryPedigree,
    #[serde(rename = "R105")]
    StaleEvidence,
    #[serde(rename = "R106")]
    DuplicateEdge,
    #[serde(rename = "R107")]
    AgeSafeguard,
    #[serde(rename = "R108")]
    KinshipSafeguard,
    #[serde(rename = "R109")]
    PartnershipSafeguard,
    #[serde(rename = "R110")]
    ConsentSafeguard,
    #[serde(rename = "R111")]
    InvalidKind,
    #[serde(rename = "R112")]
    InvalidMetadata,
    #[serde(rename = "R113")]
    LockedEdge,
}

/// One static, value-redacting relationship diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipDiagnostic {
    pub code: RelationshipDiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub path: String,
    pub message: String,
}

/// Deterministic read-only graph audit and repair suggestions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipReconciliationReport {
    pub report_format_version: u32,
    pub id: String,
    pub input_sha256: String,
    pub kind_pack: RelationshipKindPackRef,
    pub reference_date: RelationshipDate,
    pub edges: Vec<RelationshipEdgeView>,
    pub diagnostics: Vec<RelationshipDiagnostic>,
    pub repairs: Vec<RelationshipRepairSuggestion>,
}

/// Advisory repair. Reconciliation never mutates the collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipRepairSuggestion {
    pub id: String,
    pub action: RelationshipRepairAction,
    pub rationale: String,
}

/// Closed repair vocabulary; applying any repair requires a separate revision or review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
pub enum RelationshipRepairAction {
    AddInverse {
        owner_character_id: String,
        edge: Box<RelationshipEdge>,
    },
    RemoveDuplicate {
        owner_character_id: String,
        edge_id: String,
    },
    ReviewSafeguard {
        diagnostic_code: RelationshipDiagnosticCode,
    },
}

/// Error wrapper whose display never contains rejected source values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipError {
    diagnostic: RelationshipDiagnostic,
}

impl RelationshipError {
    #[must_use]
    pub const fn diagnostic(&self) -> &RelationshipDiagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for RelationshipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Character relationship {} at `{}`: {}",
            relationship_diagnostic_code(self.diagnostic.code),
            self.diagnostic.path,
            self.diagnostic.message
        )
    }
}

impl std::error::Error for RelationshipError {}

macro_rules! impl_relationship_document {
    ($type:ty, $validate:expr) => {
        impl $type {
            pub fn from_json(source: &str) -> Result<Self, RelationshipError> {
                let value = parse_strict_json(source).map_err(|_| encoding_error())?;
                ($validate)(&value)?;
                Ok(value)
            }

            pub fn from_ron(source: &str) -> Result<Self, RelationshipError> {
                let value = ron::from_str(source).map_err(|_| encoding_error())?;
                ($validate)(&value)?;
                Ok(value)
            }

            pub fn to_json(&self) -> Result<String, RelationshipError> {
                ($validate)(self)?;
                to_pretty_json(self).map_err(|_| encoding_error())
            }

            pub fn to_ron(&self) -> Result<String, RelationshipError> {
                ($validate)(self)?;
                to_pretty_ron(self).map_err(|_| encoding_error())
            }
        }
    };
}

impl_relationship_document!(RelationshipKindPack, validate_relationship_kind_pack);
impl_relationship_document!(
    RelationshipProposalConfig,
    validate_relationship_proposal_config_structure
);
impl_relationship_document!(RelationshipProposal, validate_relationship_proposal);
impl_relationship_document!(RelationshipReview, validate_relationship_review_structure);
impl_relationship_document!(RelationshipReceipt, validate_relationship_receipt);
impl_relationship_document!(
    RelationshipGraphRevision,
    validate_relationship_graph_revision_structure
);
impl_relationship_document!(
    RelationshipReconciliationReport,
    validate_relationship_reconciliation_report
);
impl_relationship_document!(RelationshipGraphPolicy, validate_relationship_graph_policy);

/// Generate the canonical relationship-kind pack JSON Schema.
pub fn relationship_kind_pack_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipKindPack>(
        PACK_SCHEMA_ID,
        "Weave Character Relationship Kind Pack v1",
    )
}

/// Generate the canonical relationship proposal-config JSON Schema.
pub fn relationship_config_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipProposalConfig>(
        CONFIG_SCHEMA_ID,
        "Weave Character Relationship Proposal Config v1",
    )
}

/// Generate the canonical relationship proposal JSON Schema.
pub fn relationship_proposal_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipProposal>(
        PROPOSAL_SCHEMA_ID,
        "Weave Character Relationship Proposal v1",
    )
}

/// Generate the canonical relationship review JSON Schema.
pub fn relationship_review_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipReview>(
        REVIEW_SCHEMA_ID,
        "Weave Character Relationship Review v1",
    )
}

/// Generate the canonical relationship receipt JSON Schema.
pub fn relationship_receipt_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipReceipt>(
        RECEIPT_SCHEMA_ID,
        "Weave Character Relationship Receipt v1",
    )
}

/// Generate the canonical direct relationship revision JSON Schema.
pub fn relationship_revision_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipGraphRevision>(
        REVISION_SCHEMA_ID,
        "Weave Character Relationship Graph Revision v1",
    )
}

/// Generate the canonical relationship reconciliation-report JSON Schema.
pub fn relationship_reconciliation_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipReconciliationReport>(
        RECONCILIATION_SCHEMA_ID,
        "Weave Character Relationship Reconciliation Report v1",
    )
}

/// Generate the canonical relationship graph-policy JSON Schema.
pub fn relationship_policy_schema() -> Result<String, RelationshipError> {
    relationship_schema::<RelationshipGraphPolicy>(
        POLICY_SCHEMA_ID,
        "Weave Character Relationship Graph Policy v1",
    )
}

fn relationship_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, RelationshipError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-relationship-version".to_owned(),
            serde_json::Value::from(1),
        );
    }
    crate::sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| encoding_error())
}

/// Canonical fingerprint of an independently validated relationship-kind pack.
pub fn relationship_kind_pack_fingerprint(
    pack: &RelationshipKindPack,
) -> Result<String, RelationshipError> {
    validate_relationship_kind_pack(pack)?;
    canonical_hash(pack)
}

/// Exact immutable relationship-kind pack coordinate.
pub fn relationship_kind_pack_ref(
    pack: &RelationshipKindPack,
) -> Result<RelationshipKindPackRef, RelationshipError> {
    Ok(RelationshipKindPackRef {
        id: pack.id.clone(),
        version: pack.version.clone(),
        sha256: relationship_kind_pack_fingerprint(pack)?,
    })
}

/// Validate an immutable kind pack, including reciprocal inverse declarations.
pub fn validate_relationship_kind_pack(
    pack: &RelationshipKindPack,
) -> Result<(), RelationshipError> {
    if pack.pack_format_version != RELATIONSHIP_KIND_PACK_FORMAT_VERSION {
        return Err(relationship_error(
            RelationshipDiagnosticCode::InvalidKind,
            "pack_format_version",
            "unsupported relationship-kind pack version",
        ));
    }
    namespaced("id", &pack.id)?;
    semver("version", &pack.version)?;
    text_value("title", &pack.title, 1, 256)?;
    text_value("description", &pack.description, 1, 2_048)?;
    if !pack.independently_authored {
        return Err(invalid_metadata(
            "independently_authored",
            "relationship-kind packs must attest independent authorship",
        ));
    }
    validate_allowed_license("license", &pack.license)?;
    text_value("license_url", &pack.license_url, 8, 2_048)?;
    validate_provenance(&pack.provenance).map_err(|_| {
        invalid_metadata("provenance", "relationship-kind pack provenance is invalid")
    })?;
    for source in &pack.provenance.sources {
        validate_allowed_license("provenance.sources.license", &source.license)?;
        if source.kind == ProvenanceKind::Original && source.license != "MIT" {
            return Err(invalid_metadata(
                "provenance.sources.license",
                "original bundled relationship material must use the project MIT license",
            ));
        }
    }
    if pack.kinds.is_empty() || pack.kinds.len() > 4_096 {
        return Err(invalid_metadata(
            "kinds",
            "relationship-kind pack requires between 1 and 4096 kinds",
        ));
    }
    for (id, definition) in &pack.kinds {
        let path = format!("kinds.{id}");
        namespaced(&path, id)?;
        if definition.id != *id {
            return Err(invalid_kind(
                format!("{path}.id"),
                "relationship kind id must equal its containing map key",
            ));
        }
        text_value(&format!("{path}.label"), &definition.label, 1, 256)?;
        text_value(
            &format!("{path}.description"),
            &definition.description,
            1,
            2_048,
        )?;
        if (definition.family == RelationshipKindFamily::Kinship)
            != definition.kinship_semantics.is_some()
        {
            return Err(invalid_kind(
                format!("{path}.kinship_semantics"),
                "kinship semantics are required only for kinship-family kinds",
            ));
        }
        validate_unique_sorted(
            &format!("{path}.required_metadata"),
            &definition.required_metadata,
        )?;
        validate_sorted_texts(&format!("{path}.limitations"), &definition.limitations)?;
        if let RelationshipDirectionality::InversePaired { inverse_kind_id } =
            &definition.directionality
        {
            namespaced(
                &format!("{path}.directionality.inverse_kind_id"),
                inverse_kind_id,
            )?;
            if inverse_kind_id == id {
                return Err(invalid_kind(
                    format!("{path}.directionality.inverse_kind_id"),
                    "inverse-paired kind must name a distinct reciprocal kind",
                ));
            }
        }
    }
    for (id, definition) in &pack.kinds {
        if let RelationshipDirectionality::InversePaired { inverse_kind_id } =
            &definition.directionality
        {
            let Some(inverse) = pack.kinds.get(inverse_kind_id) else {
                return Err(invalid_kind(
                    format!("kinds.{id}.directionality.inverse_kind_id"),
                    "inverse-paired kind references an absent kind",
                ));
            };
            if !matches!(
                &inverse.directionality,
                RelationshipDirectionality::InversePaired { inverse_kind_id: reciprocal }
                    if reciprocal == id
            ) {
                return Err(invalid_kind(
                    format!("kinds.{id}.directionality.inverse_kind_id"),
                    "inverse-paired declarations must be reciprocal",
                ));
            }
        }
    }
    Ok(())
}

/// Validate config structure independently of the collection it fingerprints.
pub fn validate_relationship_proposal_config_structure(
    config: &RelationshipProposalConfig,
) -> Result<(), RelationshipError> {
    if config.config_format_version != RELATIONSHIP_CONFIG_FORMAT_VERSION {
        return Err(invalid_metadata(
            "config_format_version",
            "unsupported relationship proposal-config version",
        ));
    }
    namespaced("id", &config.id)?;
    sha256("expected_input_sha256", &config.expected_input_sha256)?;
    validate_pack_ref("kind_pack", &config.kind_pack)?;
    validate_date("reference_date", config.reference_date)?;
    validate_sorted_namespaced("roster", &config.roster, 2, 65_536)?;
    if config.targets.is_empty() || config.targets.len() > 256 {
        return Err(invalid_metadata(
            "targets",
            "relationship proposals require between 1 and 256 targets",
        ));
    }
    let mut target_ids = BTreeSet::new();
    for (index, target) in config.targets.iter().enumerate() {
        let path = format!("targets[{index}]");
        local(&format!("{path}.id"), &target.id)?;
        if !target_ids.insert(target.id.as_str()) {
            return Err(invalid_metadata(
                format!("{path}.id"),
                "proposal target ids must be unique",
            ));
        }
        namespaced(&format!("{path}.kind_id"), &target.kind_id)?;
        if !matches!(
            target.origin,
            RelationshipEdgeOrigin::ComputedAffinity | RelationshipEdgeOrigin::SuggestedNarrative
        ) {
            return Err(invalid_metadata(
                format!("{path}.origin"),
                "proposal targets may create only computed affinity or narrative suggestions",
            ));
        }
        if target.minimum_score_micros > MICROS as u32 || target.maximum_candidates == Some(0) {
            return Err(invalid_metadata(
                path,
                "proposal target threshold or capacity is outside the supported range",
            ));
        }
        if let Some(validity) = &target.validity {
            validate_validity(&format!("{path}.validity"), validity)?;
        }
        for (id, note) in &target.notes {
            let note_path = format!("{path}.notes.{id}");
            local(&note_path, id)?;
            if note.id != *id {
                return Err(invalid_metadata(
                    format!("{note_path}.id"),
                    "relationship note id must equal its containing map key",
                ));
            }
            text_value(&format!("{note_path}.content"), &note.content, 1, 4_096)?;
            validate_sorted_local_or_namespaced(&format!("{note_path}.lineage"), &note.lineage)?;
        }
        text_value(&format!("{path}.rationale"), &target.rationale, 1, 2_048)?;
    }
    if config.evidence_rules.is_empty() || config.evidence_rules.len() > 256 {
        return Err(invalid_metadata(
            "evidence_rules",
            "relationship proposals require between 1 and 256 ordered evidence rules",
        ));
    }
    let mut rule_ids = BTreeSet::new();
    for (index, rule) in config.evidence_rules.iter().enumerate() {
        let path = format!("evidence_rules[{index}]");
        local(&format!("{path}.id"), rule.id())?;
        if !rule_ids.insert(rule.id()) {
            return Err(invalid_metadata(
                format!("{path}.id"),
                "evidence rule ids must be unique while declared order is retained",
            ));
        }
        if rule.weight_micros() == 0 || rule.weight_micros().unsigned_abs() > MICROS as u32 {
            return Err(invalid_metadata(
                format!("{path}.weight_micros"),
                "evidence rule weight must be non-zero and within one million micros",
            ));
        }
        match rule {
            RelationshipEvidenceRule::PreferenceOverlap { category, .. } => {
                namespaced(&format!("{path}.category"), category)?;
            }
            RelationshipEvidenceRule::SharedContext { context_ref, .. } => {
                text_value(&format!("{path}.context_ref"), context_ref, 1, 512)?;
            }
            RelationshipEvidenceRule::ExistingCanon {
                relationship_kind_id,
                ..
            } => namespaced(
                &format!("{path}.relationship_kind_id"),
                relationship_kind_id,
            )?,
            RelationshipEvidenceRule::TraitSimilarity { .. } => {}
        }
    }
    let mut consent_ids = BTreeSet::new();
    for (index, record) in config.consent_records.iter().enumerate() {
        let path = format!("consent_records[{index}]");
        local(&format!("{path}.id"), &record.id)?;
        if !consent_ids.insert(record.id.as_str()) {
            return Err(invalid_metadata(
                format!("{path}.id"),
                "consent record ids must be unique",
            ));
        }
        namespaced(
            &format!("{path}.source_character_id"),
            &record.source_character_id,
        )?;
        namespaced(
            &format!("{path}.target_character_id"),
            &record.target_character_id,
        )?;
        namespaced(&format!("{path}.kind_id"), &record.kind_id)?;
        validate_consent(&format!("{path}.consent"), &record.consent)?;
    }
    validate_safeguards("safeguards", &config.safeguards)?;
    validate_provenance(&config.provenance).map_err(|_| {
        invalid_metadata(
            "provenance",
            "relationship proposal-config provenance is invalid",
        )
    })
}

fn validate_pack_ref(
    path: &str,
    reference: &RelationshipKindPackRef,
) -> Result<(), RelationshipError> {
    namespaced(&format!("{path}.id"), &reference.id)?;
    semver(&format!("{path}.version"), &reference.version)?;
    sha256(&format!("{path}.sha256"), &reference.sha256)
}

fn validate_safeguards(
    path: &str,
    safeguards: &RelationshipSafeguards,
) -> Result<(), RelationshipError> {
    if safeguards
        .minimum_partnership_age_years
        .is_some_and(|age| age > 150)
        || safeguards
            .maximum_concurrent_partnerships
            .is_some_and(|maximum| maximum == 0 || maximum > 1_024)
    {
        return Err(invalid_metadata(
            path,
            "relationship safeguard limits are outside the supported range",
        ));
    }
    Ok(())
}

/// Validate one standalone project graph policy.
pub fn validate_relationship_graph_policy(
    policy: &RelationshipGraphPolicy,
) -> Result<(), RelationshipError> {
    if policy.policy_format_version != RELATIONSHIP_POLICY_FORMAT_VERSION {
        return Err(invalid_metadata(
            "policy_format_version",
            "unsupported relationship graph-policy version",
        ));
    }
    validate_date("reference_date", policy.reference_date)?;
    validate_safeguards("safeguards", &policy.safeguards)
}

fn validate_allowed_license(path: &str, value: &str) -> Result<(), RelationshipError> {
    if !matches!(value, "MIT" | "Apache-2.0" | "CC0-1.0") {
        return Err(invalid_metadata(
            path,
            "relationship packs require an allowed public-source license",
        ));
    }
    Ok(())
}

/// Return every redaction-safe structural and project-safeguard diagnostic for a graph.
pub fn relationship_graph_diagnostics(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    reference_date: RelationshipDate,
    safeguards: &RelationshipSafeguards,
) -> Result<Vec<RelationshipDiagnostic>, RelationshipError> {
    validate_relationship_kind_pack(pack)?;
    validate_date("reference_date", reference_date)?;
    validate_safeguards("safeguards", safeguards)?;
    validate_character_collection(collection).map_err(|failure| {
        relationship_error(
            RelationshipDiagnosticCode::InvalidMetadata,
            failure.diagnostic().path.clone(),
            "Character collection is invalid before relationship graph validation",
        )
    })?;
    let expected_pack = relationship_kind_pack_ref(pack)?;
    let mut diagnostics = Vec::new();
    let mut edges = Vec::<(&str, &RelationshipEdge, &RelationshipKindDefinition)>::new();

    for (owner_id, profile) in &collection.characters {
        let Some(record) = relationship_record(profile) else {
            continue;
        };
        if record.value.graph_format_version != 1 {
            diagnostics.push(diagnostic(
                RelationshipDiagnosticCode::InvalidMetadata,
                format!("characters.{owner_id}.relationships.graph_format_version"),
                "unsupported relationship graph value version",
            ));
        }
        if !record.value.edges.is_empty() && record.value.kind_pack.as_ref() != Some(&expected_pack)
        {
            diagnostics.push(diagnostic(
                RelationshipDiagnosticCode::StaleEvidence,
                format!("characters.{owner_id}.relationships.kind_pack"),
                "relationship graph kind-pack coordinate is stale",
            ));
        }
        for (edge_id, edge) in &record.value.edges {
            let path = format!("characters.{owner_id}.relationships.edges.{edge_id}");
            if let Err(failure) = validate_edge_shape(&path, edge_id, owner_id, edge, pack, false) {
                diagnostics.push(failure.diagnostic);
                continue;
            }
            let definition = &pack.kinds[&edge.kind];
            if edge.freshness == Freshness::Stale {
                diagnostics.push(diagnostic(
                    RelationshipDiagnosticCode::StaleEvidence,
                    format!("{path}.freshness"),
                    "relationship edge is explicitly marked stale and requires review",
                ));
            }
            if !collection
                .characters
                .contains_key(&edge.target_character_id)
            {
                diagnostics.push(diagnostic(
                    RelationshipDiagnosticCode::MissingCharacter,
                    format!("{path}.target_character_id"),
                    "relationship target is absent from the pinned collection roster",
                ));
            }
            if edge.source_character_id == edge.target_character_id && !definition.allows_self {
                diagnostics.push(diagnostic(
                    RelationshipDiagnosticCode::ForbiddenSelfEdge,
                    format!("{path}.target_character_id"),
                    "relationship kind forbids self-edges",
                ));
            }
            edges.push((owner_id, edge, definition));
        }
    }

    validate_inverse_edges(collection, &edges, &mut diagnostics);
    validate_duplicate_edges(&edges, &mut diagnostics);
    validate_pedigree(&edges, &mut diagnostics);
    validate_partnership_safeguards(
        collection,
        &edges,
        reference_date,
        safeguards,
        &mut diagnostics,
    );
    diagnostics.sort_by(|left, right| {
        (&left.path, left.code, &left.message).cmp(&(&right.path, right.code, &right.message))
    });
    diagnostics.dedup();
    Ok(diagnostics)
}

/// Fail closed when a graph has any blocking diagnostic.
pub fn validate_relationship_graph(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    reference_date: RelationshipDate,
    safeguards: &RelationshipSafeguards,
) -> Result<(), RelationshipError> {
    let diagnostics = relationship_graph_diagnostics(collection, pack, reference_date, safeguards)?;
    diagnostics
        .into_iter()
        .find(|value| value.severity == DiagnosticSeverity::Error)
        .map_or(Ok(()), |diagnostic| Err(RelationshipError { diagnostic }))
}

fn relationship_record(
    profile: &CharacterProfile,
) -> Option<&VersionedExtension<RelationshipEdges>> {
    match profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::Relationships(record)) => Some(record),
        _ => None,
    }
}

fn relationship_record_mut(
    profile: &mut CharacterProfile,
) -> Option<&mut VersionedExtension<RelationshipEdges>> {
    match profile.extensions.get_mut(RELATIONSHIP_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::Relationships(record)) => Some(record),
        _ => None,
    }
}

fn validate_edge_shape(
    path: &str,
    map_id: &str,
    owner_id: &str,
    edge: &RelationshipEdge,
    pack: &RelationshipKindPack,
    proposal_edge: bool,
) -> Result<(), RelationshipError> {
    local(path, map_id)?;
    if edge.id != map_id || edge.source_character_id != owner_id {
        return Err(invalid_metadata(
            path,
            "relationship edge id and source must match its map key and owning profile",
        ));
    }
    namespaced(
        &format!("{path}.source_character_id"),
        &edge.source_character_id,
    )?;
    namespaced(
        &format!("{path}.target_character_id"),
        &edge.target_character_id,
    )?;
    namespaced(&format!("{path}.kind"), &edge.kind)?;
    let Some(definition) = pack.kinds.get(&edge.kind) else {
        return Err(invalid_kind(
            format!("{path}.kind"),
            "relationship edge kind is absent from the selected immutable pack",
        ));
    };
    if let Some(validity) = &edge.validity {
        validate_validity(&format!("{path}.validity"), validity)?;
    }
    if let Some(inverse_edge_id) = &edge.inverse_edge_id {
        local(&format!("{path}.inverse_edge_id"), inverse_edge_id)?;
    }
    match definition.directionality {
        RelationshipDirectionality::InversePaired { .. } if edge.inverse_edge_id.is_none() => {
            return Err(relationship_error(
                RelationshipDiagnosticCode::BrokenInverse,
                format!("{path}.inverse_edge_id"),
                "inverse-paired relationship edge requires an explicit reciprocal edge id",
            ));
        }
        RelationshipDirectionality::Directed | RelationshipDirectionality::Symmetric
            if edge.inverse_edge_id.is_some() =>
        {
            return Err(relationship_error(
                RelationshipDiagnosticCode::BrokenInverse,
                format!("{path}.inverse_edge_id"),
                "non-inverse relationship edge must omit inverse edge metadata",
            ));
        }
        _ => {}
    }
    if matches!(
        definition.directionality,
        RelationshipDirectionality::Symmetric
    ) && edge.source_character_id > edge.target_character_id
    {
        return Err(invalid_metadata(
            format!("{path}.source_character_id"),
            "symmetric relationships use the lexicographically ordered pair once",
        ));
    }
    let mut note_ids = BTreeSet::new();
    for (id, note) in &edge.notes {
        let note_path = format!("{path}.notes.{id}");
        local(&note_path, id)?;
        if note.id != *id || !note_ids.insert(id.as_str()) {
            return Err(invalid_metadata(
                format!("{note_path}.id"),
                "relationship note id must equal its unique containing map key",
            ));
        }
        text_value(&format!("{note_path}.content"), &note.content, 1, 4_096)?;
        validate_sorted_local_or_namespaced(&format!("{note_path}.lineage"), &note.lineage)?;
    }
    if let Some(consent) = &edge.consent {
        validate_consent(&format!("{path}.consent"), consent)?;
    }
    for (id, exception) in &edge.safeguard_exceptions {
        let exception_path = format!("{path}.safeguard_exceptions.{id}");
        local(&exception_path, id)?;
        if exception.id != *id || !is_safeguard_code(&exception.code) {
            return Err(invalid_metadata(
                exception_path,
                "safeguard exception id or public R### code is invalid",
            ));
        }
        namespaced(
            &format!("{path}.safeguard_exceptions.{id}.reviewer"),
            &exception.reviewer,
        )?;
        text_value(
            &format!("{path}.safeguard_exceptions.{id}.rationale"),
            &exception.rationale,
            1,
            2_048,
        )?;
        validate_sorted_local_or_namespaced(
            &format!("{path}.safeguard_exceptions.{id}.lineage"),
            &exception.lineage,
        )?;
    }
    if edge
        .affinity_score_micros
        .is_some_and(|score| score > MICROS as u32)
    {
        return Err(invalid_metadata(
            format!("{path}.affinity_score_micros"),
            "relationship advisory score must be between zero and one million micros",
        ));
    }
    let mut evidence_ids = BTreeSet::new();
    for (index, evidence) in edge.evidence.iter().enumerate() {
        let evidence_path = format!("{path}.evidence[{index}]");
        local(&format!("{evidence_path}.id"), &evidence.id)?;
        if !evidence_ids.insert(evidence.id.as_str()) {
            return Err(invalid_metadata(
                format!("{evidence_path}.id"),
                "relationship evidence ids must be unique while declared order is retained",
            ));
        }
        if evidence.contribution_micros.unsigned_abs() > MICROS as u32 {
            return Err(invalid_metadata(
                format!("{evidence_path}.contribution_micros"),
                "relationship evidence contribution exceeds one million micros",
            ));
        }
        validate_sorted_texts(
            &format!("{evidence_path}.input_paths"),
            &evidence.input_paths,
        )?;
        sha256(
            &format!("{evidence_path}.input_sha256"),
            &evidence.input_sha256,
        )?;
        text_value(
            &format!("{evidence_path}.explanation"),
            &evidence.explanation,
            1,
            2_048,
        )?;
    }
    validate_sorted_local_or_namespaced(&format!("{path}.lineage"), &edge.lineage)?;
    if let Some(rationale) = &edge.rationale {
        text_value(&format!("{path}.rationale"), rationale, 1, 2_048)?;
    }
    match edge.origin {
        RelationshipEdgeOrigin::Authored => {}
        RelationshipEdgeOrigin::Imported if edge.rationale.is_none() => {
            return Err(invalid_metadata(
                format!("{path}.rationale"),
                "imported relationships require an explicit rationale",
            ));
        }
        RelationshipEdgeOrigin::ComputedAffinity => {
            if edge.affinity_score_micros.is_none() || edge.evidence.is_empty() {
                return Err(invalid_metadata(
                    path,
                    "computed affinity requires a bounded score and inspectable evidence",
                ));
            }
        }
        RelationshipEdgeOrigin::SuggestedNarrative | RelationshipEdgeOrigin::ReviewedSuggestion
            if edge.rationale.is_none() =>
        {
            return Err(invalid_metadata(
                format!("{path}.rationale"),
                "narrative relationship suggestions require an explicit rationale",
            ));
        }
        _ => {}
    }
    if proposal_edge {
        if edge.review != ReviewState::Pending || edge.lock != LockState::Unlocked {
            return Err(invalid_metadata(
                path,
                "proposal edges must remain pending and unlocked",
            ));
        }
    } else {
        match edge.origin {
            RelationshipEdgeOrigin::ComputedAffinity
            | RelationshipEdgeOrigin::ReviewedSuggestion
                if edge.review != ReviewState::Accepted =>
            {
                return Err(invalid_metadata(
                    format!("{path}.review"),
                    "applied computed or reviewed-suggestion edges require accepted review",
                ));
            }
            RelationshipEdgeOrigin::SuggestedNarrative if edge.review != ReviewState::Pending => {
                return Err(invalid_metadata(
                    format!("{path}.review"),
                    "unaccepted narrative suggestions remain pending",
                ));
            }
            _ => {}
        }
    }
    for requirement in &definition.required_metadata {
        let present = match requirement {
            RelationshipMetadataRequirement::Confidence => edge.confidence != Confidence::Unknown,
            RelationshipMetadataRequirement::Validity => edge.validity.is_some(),
            RelationshipMetadataRequirement::Notes => !edge.notes.is_empty(),
            RelationshipMetadataRequirement::Consent => edge.consent.is_some(),
            RelationshipMetadataRequirement::Evidence => !edge.evidence.is_empty(),
        };
        if !present {
            return Err(invalid_metadata(
                path,
                "relationship edge omits metadata required by its selected kind",
            ));
        }
    }
    Ok(())
}

fn validate_inverse_edges(
    collection: &CharacterCollection,
    edges: &[(&str, &RelationshipEdge, &RelationshipKindDefinition)],
    diagnostics: &mut Vec<RelationshipDiagnostic>,
) {
    for (owner_id, edge, definition) in edges {
        let RelationshipDirectionality::InversePaired { inverse_kind_id } =
            &definition.directionality
        else {
            continue;
        };
        let inverse = collection
            .characters
            .get(&edge.target_character_id)
            .and_then(relationship_record)
            .and_then(|record| {
                edge.inverse_edge_id
                    .as_ref()
                    .and_then(|id| record.value.edges.get(id))
            });
        let valid = inverse.is_some_and(|inverse| {
            inverse.source_character_id == edge.target_character_id
                && inverse.target_character_id == edge.source_character_id
                && inverse.kind == *inverse_kind_id
                && inverse.inverse_edge_id.as_deref() == Some(edge.id.as_str())
        });
        if !valid {
            diagnostics.push(diagnostic(
                RelationshipDiagnosticCode::BrokenInverse,
                format!(
                    "characters.{owner_id}.relationships.edges.{}.inverse_edge_id",
                    edge.id
                ),
                "inverse-paired relationship does not resolve to an exact reciprocal edge",
            ));
        }
    }
}

fn validate_duplicate_edges(
    edges: &[(&str, &RelationshipEdge, &RelationshipKindDefinition)],
    diagnostics: &mut Vec<RelationshipDiagnostic>,
) {
    let mut grouped =
        BTreeMap::<(String, String, String), Vec<(&str, &RelationshipEdge, bool)>>::new();
    for (owner_id, edge, definition) in edges {
        let (source, target) = if matches!(
            definition.directionality,
            RelationshipDirectionality::Symmetric
        ) {
            let mut pair = [
                edge.source_character_id.clone(),
                edge.target_character_id.clone(),
            ];
            pair.sort();
            (pair[0].clone(), pair[1].clone())
        } else {
            (
                edge.source_character_id.clone(),
                edge.target_character_id.clone(),
            )
        };
        grouped
            .entry((source, target, edge.kind.clone()))
            .or_default()
            .push((owner_id, edge, definition.allows_multiple_concurrent));
    }
    for values in grouped.values() {
        if values.len() < 2 {
            continue;
        }
        let exact_duplicate = values.iter().enumerate().any(|(index, (_, edge, _))| {
            values[index + 1..]
                .iter()
                .any(|(_, other, _)| edge.validity == other.validity)
        });
        if !values[0].2 || exact_duplicate {
            for (owner_id, edge, _) in values {
                diagnostics.push(diagnostic(
                    RelationshipDiagnosticCode::DuplicateEdge,
                    format!("characters.{owner_id}.relationships.edges.{}", edge.id),
                    "relationship graph contains a duplicate or forbidden concurrent edge",
                ));
            }
        }
    }
}

fn validate_pedigree(
    edges: &[(&str, &RelationshipEdge, &RelationshipKindDefinition)],
    diagnostics: &mut Vec<RelationshipDiagnostic>,
) {
    let mut parents = BTreeMap::<String, BTreeSet<String>>::new();
    let mut edge_paths = BTreeMap::<(String, String), String>::new();
    for (owner_id, edge, definition) in edges {
        let Some(semantics) = definition.kinship_semantics else {
            continue;
        };
        let relation = match semantics {
            RelationshipKinshipSemantics::ParentOf => Some((
                edge.source_character_id.clone(),
                edge.target_character_id.clone(),
            )),
            RelationshipKinshipSemantics::ChildOf => Some((
                edge.target_character_id.clone(),
                edge.source_character_id.clone(),
            )),
            RelationshipKinshipSemantics::SiblingOf | RelationshipKinshipSemantics::Other => None,
        };
        if let Some((parent, child)) = relation {
            parents
                .entry(parent.clone())
                .or_default()
                .insert(child.clone());
            edge_paths.insert(
                (parent, child),
                format!("characters.{owner_id}.relationships.edges.{}", edge.id),
            );
        }
    }
    for ((parent, child), path) in &edge_paths {
        if edge_paths.contains_key(&(child.clone(), parent.clone())) {
            diagnostics.push(diagnostic(
                RelationshipDiagnosticCode::ContradictoryPedigree,
                path.clone(),
                "pedigree cannot declare each character as the other's parent",
            ));
        }
    }
    for node in parents.keys() {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        if pedigree_cycle(node, &parents, &mut visiting, &mut visited) {
            diagnostics.push(diagnostic(
                RelationshipDiagnosticCode::ContradictoryPedigree,
                "relationships.pedigree",
                "parent relationships form a contradictory pedigree cycle",
            ));
            break;
        }
    }
}

fn pedigree_cycle(
    node: &str,
    parents: &BTreeMap<String, BTreeSet<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node.to_owned()) {
        return true;
    }
    if parents.get(node).is_some_and(|children| {
        children
            .iter()
            .any(|child| pedigree_cycle(child, parents, visiting, visited))
    }) {
        return true;
    }
    visiting.remove(node);
    visited.insert(node.to_owned());
    false
}

fn validate_partnership_safeguards(
    collection: &CharacterCollection,
    edges: &[(&str, &RelationshipEdge, &RelationshipKindDefinition)],
    reference_date: RelationshipDate,
    safeguards: &RelationshipSafeguards,
    diagnostics: &mut Vec<RelationshipDiagnostic>,
) {
    let close_kin = edges
        .iter()
        .filter(|(_, _, definition)| definition.family == RelationshipKindFamily::Kinship)
        .map(|(_, edge, _)| unordered_pair(&edge.source_character_id, &edge.target_character_id))
        .collect::<BTreeSet<_>>();
    let mut active_partnership_counts = BTreeMap::<String, u32>::new();
    let mut partnership_count_exceptions = BTreeSet::<String>::new();
    for (owner_id, edge, definition) in edges {
        if definition.family != RelationshipKindFamily::Partnership
            || !edge_is_active(edge, reference_date)
        {
            continue;
        }
        let path = format!("characters.{owner_id}.relationships.edges.{}", edge.id);
        *active_partnership_counts
            .entry(edge.source_character_id.clone())
            .or_default() += 1;
        *active_partnership_counts
            .entry(edge.target_character_id.clone())
            .or_default() += 1;
        if edge_has_exception(edge, RelationshipDiagnosticCode::PartnershipSafeguard) {
            partnership_count_exceptions.insert(edge.source_character_id.clone());
            partnership_count_exceptions.insert(edge.target_character_id.clone());
        }
        if let Some(minimum) = safeguards.minimum_partnership_age_years {
            for character_id in [&edge.source_character_id, &edge.target_character_id] {
                let age = collection
                    .characters
                    .get(character_id)
                    .and_then(|profile| minimum_known_age(profile, reference_date));
                if age.is_none_or(|age| age < i32::from(minimum))
                    && !edge_has_exception(edge, RelationshipDiagnosticCode::AgeSafeguard)
                {
                    diagnostics.push(diagnostic(
                        RelationshipDiagnosticCode::AgeSafeguard,
                        format!("{path}.validity"),
                        "partnership age safeguard lacks sufficient eligible date evidence",
                    ));
                    break;
                }
            }
        }
        if safeguards.forbid_close_kin_partnership
            && close_kin.contains(&unordered_pair(
                &edge.source_character_id,
                &edge.target_character_id,
            ))
            && !edge_has_exception(edge, RelationshipDiagnosticCode::KinshipSafeguard)
        {
            diagnostics.push(diagnostic(
                RelationshipDiagnosticCode::KinshipSafeguard,
                path.clone(),
                "project policy forbids partnership between directly related characters",
            ));
        }
        if safeguards.require_affirmed_partnership_consent
            && edge.consent.as_ref().map(|value| value.state)
                != Some(RelationshipConsentState::Affirmed)
            && !edge_has_exception(edge, RelationshipDiagnosticCode::ConsentSafeguard)
        {
            diagnostics.push(diagnostic(
                RelationshipDiagnosticCode::ConsentSafeguard,
                format!("{path}.consent"),
                "project policy requires explicit affirmed consent for partnership",
            ));
        }
    }
    if let Some(maximum) = safeguards.maximum_concurrent_partnerships {
        for (character_id, count) in active_partnership_counts {
            if count > u32::from(maximum) && !partnership_count_exceptions.contains(&character_id) {
                diagnostics.push(diagnostic(
                    RelationshipDiagnosticCode::PartnershipSafeguard,
                    format!("characters.{character_id}.relationships"),
                    "active partnership count exceeds the project-defined maximum",
                ));
            }
        }
    }
}

fn edge_has_exception(edge: &RelationshipEdge, code: RelationshipDiagnosticCode) -> bool {
    let code = relationship_diagnostic_code(code);
    edge.safeguard_exceptions
        .values()
        .any(|exception| exception.code == code)
}

fn is_safeguard_code(value: &str) -> bool {
    matches!(value, "R107" | "R108" | "R109" | "R110")
}

fn minimum_known_age(profile: &CharacterProfile, reference_date: RelationshipDate) -> Option<i32> {
    let birth = profile.canon.birth_date.as_ref()?;
    match birth.value {
        BirthDate::Year { year, .. } => Some(reference_date.year - year - 1),
        BirthDate::MonthDay { .. } => None,
        BirthDate::Full {
            year, month, day, ..
        } => {
            let birthday_passed = (reference_date.month, reference_date.day) >= (month, day);
            Some(reference_date.year - year - i32::from(!birthday_passed))
        }
    }
}

fn edge_is_active(edge: &RelationshipEdge, date: RelationshipDate) -> bool {
    edge.validity.as_ref().is_none_or(|period| {
        period.start.is_none_or(|start| start <= date) && period.end.is_none_or(|end| date <= end)
    })
}

fn unordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
}

fn validate_date(path: &str, date: RelationshipDate) -> Result<(), RelationshipError> {
    if !(-999_999..=999_999).contains(&date.year)
        || !(1..=12).contains(&date.month)
        || date.day == 0
        || date.day > days_in_month(date.year, date.month)
    {
        return Err(relationship_error(
            RelationshipDiagnosticCode::InvalidDateRange,
            path,
            "relationship date is not a valid proleptic-Gregorian date",
        ));
    }
    Ok(())
}

fn validate_validity(
    path: &str,
    validity: &RelationshipValidityPeriod,
) -> Result<(), RelationshipError> {
    if validity.start.is_none() && validity.end.is_none() {
        return Err(relationship_error(
            RelationshipDiagnosticCode::InvalidDateRange,
            path,
            "relationship validity must include at least one date bound",
        ));
    }
    if let Some(start) = validity.start {
        validate_date(&format!("{path}.start"), start)?;
    }
    if let Some(end) = validity.end {
        validate_date(&format!("{path}.end"), end)?;
    }
    if matches!((validity.start, validity.end), (Some(start), Some(end)) if start > end) {
        return Err(relationship_error(
            RelationshipDiagnosticCode::InvalidDateRange,
            path,
            "relationship validity start must not follow its end",
        ));
    }
    Ok(())
}

const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: i32) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

fn validate_consent(path: &str, consent: &RelationshipConsent) -> Result<(), RelationshipError> {
    match consent.state {
        RelationshipConsentState::Affirmed => {
            let Some(reviewer) = &consent.reviewed_by else {
                return Err(invalid_metadata(
                    format!("{path}.reviewed_by"),
                    "affirmed consent requires an explicit reviewer identity",
                ));
            };
            namespaced(&format!("{path}.reviewed_by"), reviewer)?;
            text_value(
                &format!("{path}.rationale"),
                consent.rationale.as_deref().unwrap_or_default(),
                1,
                2_048,
            )?;
        }
        RelationshipConsentState::NotApplicable
        | RelationshipConsentState::Unknown
        | RelationshipConsentState::Withheld => {
            if let Some(reviewer) = &consent.reviewed_by {
                namespaced(&format!("{path}.reviewed_by"), reviewer)?;
            }
            if let Some(rationale) = &consent.rationale {
                text_value(&format!("{path}.rationale"), rationale, 1, 2_048)?;
            }
        }
    }
    validate_sorted_local_or_namespaced(&format!("{path}.lineage"), &consent.lineage)
}

/// Build a deterministic relationship proposal without mutating the input collection.
pub fn propose_relationships(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    config: &RelationshipProposalConfig,
) -> Result<RelationshipProposal, RelationshipError> {
    validate_relationship_kind_pack(pack)?;
    validate_relationship_proposal_config(collection, pack, config)?;
    validate_relationship_graph(collection, pack, config.reference_date, &config.safeguards)?;
    build_relationship_proposal(collection, pack, config)
}

/// Validate config coordinates against the exact collection and immutable pack.
pub fn validate_relationship_proposal_config(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    config: &RelationshipProposalConfig,
) -> Result<(), RelationshipError> {
    validate_relationship_proposal_config_structure(config)?;
    validate_relationship_kind_pack(pack)?;
    validate_character_collection(collection).map_err(|failure| {
        relationship_error(
            RelationshipDiagnosticCode::InvalidMetadata,
            failure.diagnostic().path.clone(),
            "Character collection is invalid for relationship proposal generation",
        )
    })?;
    if config.expected_input_sha256 != collection_hash(collection)? {
        return Err(stale(
            "expected_input_sha256",
            "relationship config does not fingerprint the current collection",
        ));
    }
    if config.kind_pack != relationship_kind_pack_ref(pack)? {
        return Err(stale(
            "kind_pack",
            "relationship config does not pin the exact selected kind pack",
        ));
    }
    for id in &config.roster {
        if !collection.characters.contains_key(id) {
            return Err(relationship_error(
                RelationshipDiagnosticCode::MissingCharacter,
                "roster",
                "relationship roster references a character absent from the collection",
            ));
        }
    }
    let target_kinds = config
        .targets
        .iter()
        .map(|target| target.kind_id.as_str())
        .collect::<BTreeSet<_>>();
    for (index, target) in config.targets.iter().enumerate() {
        let Some(definition) = pack.kinds.get(&target.kind_id) else {
            return Err(invalid_kind(
                format!("targets[{index}].kind_id"),
                "proposal target kind is absent from the selected immutable pack",
            ));
        };
        if let RelationshipDirectionality::InversePaired { inverse_kind_id } =
            &definition.directionality
            && target_kinds.contains(inverse_kind_id.as_str())
        {
            return Err(invalid_kind(
                format!("targets[{index}].kind_id"),
                "proposal targets must select only one side of an inverse-paired kind",
            ));
        }
    }
    for (index, record) in config.consent_records.iter().enumerate() {
        if config
            .roster
            .binary_search(&record.source_character_id)
            .is_err()
            || config
                .roster
                .binary_search(&record.target_character_id)
                .is_err()
            || !pack.kinds.contains_key(&record.kind_id)
        {
            return Err(relationship_error(
                RelationshipDiagnosticCode::MissingCharacter,
                format!("consent_records[{index}]"),
                "consent record references a character or kind outside the pinned proposal scope",
            ));
        }
    }
    Ok(())
}

fn build_relationship_proposal(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    config: &RelationshipProposalConfig,
) -> Result<RelationshipProposal, RelationshipError> {
    let input_sha256 = collection_hash(collection)?;
    let kind_pack_ref = relationship_kind_pack_ref(pack)?;
    let config_sha256 = canonical_hash(config)?;
    let roster_sha256 = canonical_hash(&config.roster)?;
    let profile_sha256 = config
        .roster
        .iter()
        .map(|id| canonical_hash(&collection.characters[id]).map(|hash| (id.clone(), hash)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let lineage = proposal_lineage(&config.provenance);
    let mut candidates = BTreeMap::new();

    for target in &config.targets {
        let definition = &pack.kinds[&target.kind_id];
        for left_index in 0..config.roster.len() {
            for right_index in (left_index + 1)..config.roster.len() {
                let left = &config.roster[left_index];
                let right = &config.roster[right_index];
                let pairs = match definition.directionality {
                    RelationshipDirectionality::Directed => {
                        vec![
                            (left.as_str(), right.as_str()),
                            (right.as_str(), left.as_str()),
                        ]
                    }
                    RelationshipDirectionality::Symmetric
                    | RelationshipDirectionality::InversePaired { .. } => {
                        vec![(left.as_str(), right.as_str())]
                    }
                };
                for (source_id, target_id) in pairs {
                    let evidence = score_pair(collection, source_id, target_id, config)?;
                    let score_micros = normalize_score(&evidence, &config.evidence_rules);
                    let seeded_sha256 = canonical_hash(&(
                        config.seed,
                        config.id.as_str(),
                        target.id.as_str(),
                        source_id,
                        target_id,
                        score_micros,
                    ))?;
                    let candidate_id = format!("candidate_{}", &seeded_sha256[..24]);
                    let consent = consent_for_pair(config, source_id, target_id, &target.kind_id);
                    let edges = proposed_edges(
                        &candidate_id,
                        source_id,
                        target_id,
                        target,
                        definition,
                        &evidence,
                        score_micros,
                        consent,
                        &lineage,
                        pack,
                    )?;
                    let safeguards =
                        candidate_diagnostics(collection, pack, config, &edges, &candidate_id)?;
                    let disposition = if safeguards
                        .iter()
                        .any(|value| value.severity == DiagnosticSeverity::Error)
                    {
                        RelationshipCandidateDisposition::SafeguardBlocked
                    } else if score_micros < target.minimum_score_micros {
                        RelationshipCandidateDisposition::BelowThreshold
                    } else {
                        RelationshipCandidateDisposition::Proposed
                    };
                    let candidate = RelationshipCandidate {
                        id: candidate_id.clone(),
                        target_id: target.id.clone(),
                        source_character_id: source_id.to_owned(),
                        target_character_id: target_id.to_owned(),
                        kind_id: target.kind_id.clone(),
                        origin: target.origin,
                        score_micros,
                        disposition,
                        rank: 0,
                        seeded_sha256,
                        evidence,
                        edges,
                        safeguards,
                        limitations: definition.limitations.clone(),
                        rationale: target.rationale.clone(),
                    };
                    if candidates.insert(candidate_id, candidate).is_some() {
                        return Err(invalid_metadata(
                            "candidates",
                            "deterministic relationship candidate identity collision",
                        ));
                    }
                }
            }
        }
    }
    rank_and_limit_candidates(&mut candidates, config);
    let distribution = proposal_distribution(&candidates);
    Ok(RelationshipProposal {
        proposal_format_version: RELATIONSHIP_PROPOSAL_FORMAT_VERSION,
        id: derived_document_id(&config.id, "proposal"),
        input_collection: collection.clone(),
        input_sha256,
        kind_pack: pack.clone(),
        kind_pack_ref,
        config: config.clone(),
        config_sha256,
        roster_sha256,
        profile_sha256,
        candidates,
        distribution,
    })
}

fn score_pair(
    collection: &CharacterCollection,
    source_id: &str,
    target_id: &str,
    config: &RelationshipProposalConfig,
) -> Result<Vec<RelationshipEvidenceContribution>, RelationshipError> {
    let source = &collection.characters[source_id];
    let target = &collection.characters[target_id];
    config
        .evidence_rules
        .iter()
        .map(|rule| score_rule(source, target, rule))
        .collect()
}

fn score_rule(
    source: &CharacterProfile,
    target: &CharacterProfile,
    rule: &RelationshipEvidenceRule,
) -> Result<RelationshipEvidenceContribution, RelationshipError> {
    let (available, signal_micros, input_paths, input_value, explanation): (
        bool,
        i64,
        Vec<String>,
        String,
        &'static str,
    ) = match rule {
        RelationshipEvidenceRule::TraitSimilarity { trait_id, .. } => {
            let left = trait_value(&source.canon.personality, *trait_id);
            let right = trait_value(&target.canon.personality, *trait_id);
            let values = left.zip(right).map(|(left, right)| {
                let left = measurement_score(left.value);
                let right = measurement_score(right.value);
                (left, right)
            });
            let signal = values
                .map(|(left, right)| ((1.0 - (left - right).abs()) * MICROS as f64).round() as i64);
            let path = trait_path(*trait_id);
            (
                signal.is_some(),
                signal.unwrap_or(0),
                vec![
                    format!("characters.{}.{}", source.id, path),
                    format!("characters.{}.{}", target.id, path),
                ],
                canonical_hash(&values)?,
                "Similarity of two explicitly approved canonical trait measurements; advisory, not interpersonal truth.",
            )
        }
        RelationshipEvidenceRule::PreferenceOverlap { category, .. } => {
            let left = preference_keys(source, category);
            let right = preference_keys(target, category);
            let union = left.union(&right).count();
            let intersection = left.intersection(&right).count();
            let available = !left.is_empty() && !right.is_empty();
            let signal = if available {
                (intersection as i64 * MICROS) / union.max(1) as i64
            } else {
                0
            };
            (
                available,
                signal,
                vec![
                    format!("characters.{}.extensions.expression.preferences", source.id),
                    format!("characters.{}.extensions.expression.preferences", target.id),
                ],
                canonical_hash(&(left, right))?,
                "Exact overlap of reviewed normalized preference records in the selected category.",
            )
        }
        RelationshipEvidenceRule::SharedContext { context_ref, .. } => {
            let left = context_refs(source);
            let right = context_refs(target);
            let available = left.contains(context_ref) && right.contains(context_ref);
            (
                available,
                if available { MICROS } else { 0 },
                vec![
                    format!("characters.{}.extensions.context_refs", source.id),
                    format!("characters.{}.extensions.context_refs", target.id),
                ],
                canonical_hash(&(context_ref, left, right))?,
                "Exact shared reference explicitly retained by both character profiles.",
            )
        }
        RelationshipEvidenceRule::ExistingCanon {
            relationship_kind_id,
            ..
        } => {
            let ids = existing_canon_edge_ids(source, target, relationship_kind_id);
            let available = !ids.is_empty();
            (
                available,
                if available { MICROS } else { 0 },
                vec![format!(
                    "characters.{}.extensions.relationships.edges",
                    source.id
                )],
                canonical_hash(&ids)?,
                "Presence of an explicitly authored, imported, or reviewed relationship of the selected kind.",
            )
        }
    };
    let contribution = (signal_micros * i64::from(rule.weight_micros())) / MICROS;
    let mut input_paths = input_paths;
    input_paths.sort();
    input_paths.dedup();
    Ok(RelationshipEvidenceContribution {
        id: rule.id().to_owned(),
        kind: rule.kind(),
        contribution_micros: contribution.clamp(-MICROS, MICROS) as i32,
        available,
        input_paths,
        input_sha256: input_value,
        explanation: explanation.to_owned(),
    })
}

const fn measurement_score(value: TraitMeasurement) -> f64 {
    match value {
        TraitMeasurement::Score { score } => score,
        TraitMeasurement::Band { band } => band.projection_anchor(),
    }
}

fn preference_keys(profile: &CharacterProfile, category: &str) -> BTreeSet<(String, String)> {
    profile
        .extensions
        .values()
        .find_map(|extension| match extension {
            CharacterExtension::Expression(record) => Some(&record.value.preferences),
            _ => None,
        })
        .into_iter()
        .flatten()
        .filter(|(_, value)| value.category == category)
        .map(|(_, value)| {
            (
                value.target.clone(),
                match value.polarity {
                    PreferencePolarity::Prefer => "prefer",
                    PreferencePolarity::Avoid => "avoid",
                }
                .to_owned(),
            )
        })
        .collect()
}

fn context_refs(profile: &CharacterProfile) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for extension in profile.extensions.values() {
        match extension {
            CharacterExtension::IdentityPresentation(record) => {
                result.extend(record.value.identity_refs.iter().cloned());
                result.extend(record.value.presentation_refs.iter().cloned());
            }
            CharacterExtension::DateContext(record) => {
                result.extend(record.value.accepted_record_ids.iter().cloned());
            }
            _ => {}
        }
    }
    result
}

fn existing_canon_edge_ids(
    source: &CharacterProfile,
    target: &CharacterProfile,
    kind_id: &str,
) -> Vec<String> {
    [source, target]
        .into_iter()
        .filter_map(relationship_record)
        .flat_map(|record| record.value.edges.values())
        .filter(|edge| {
            edge.kind == kind_id
                && unordered_pair(&edge.source_character_id, &edge.target_character_id)
                    == unordered_pair(&source.id, &target.id)
                && matches!(
                    edge.origin,
                    RelationshipEdgeOrigin::Authored
                        | RelationshipEdgeOrigin::Imported
                        | RelationshipEdgeOrigin::ReviewedSuggestion
                )
                && edge.review != ReviewState::Rejected
        })
        .map(|edge| edge.id.clone())
        .collect()
}

fn normalize_score(
    evidence: &[RelationshipEvidenceContribution],
    rules: &[RelationshipEvidenceRule],
) -> u32 {
    let numerator = evidence
        .iter()
        .map(|value| i64::from(value.contribution_micros))
        .sum::<i64>();
    let denominator = rules
        .iter()
        .map(|rule| i64::from(rule.weight_micros().unsigned_abs()))
        .sum::<i64>()
        .max(1);
    ((numerator * MICROS) / denominator).clamp(0, MICROS) as u32
}

fn consent_for_pair(
    config: &RelationshipProposalConfig,
    source_id: &str,
    target_id: &str,
    kind_id: &str,
) -> Option<RelationshipConsent> {
    config
        .consent_records
        .iter()
        .find(|record| {
            record.kind_id == kind_id
                && unordered_pair(&record.source_character_id, &record.target_character_id)
                    == unordered_pair(source_id, target_id)
        })
        .map(|record| record.consent.clone())
}

#[allow(clippy::too_many_arguments)]
fn proposed_edges(
    candidate_id: &str,
    source_id: &str,
    target_id: &str,
    target: &RelationshipProposalTarget,
    definition: &RelationshipKindDefinition,
    evidence: &[RelationshipEvidenceContribution],
    score_micros: u32,
    consent: Option<RelationshipConsent>,
    lineage: &[String],
    pack: &RelationshipKindPack,
) -> Result<Vec<RelationshipEdge>, RelationshipError> {
    let edge_seed = canonical_hash(&(candidate_id, source_id, target_id, &target.kind_id))?;
    let forward_id = format!("edge_{}", &edge_seed[..24]);
    let inverse_id = matches!(
        definition.directionality,
        RelationshipDirectionality::InversePaired { .. }
    )
    .then(|| format!("edge_{}", &edge_seed[24..48]));
    let forward = RelationshipEdge {
        id: forward_id.clone(),
        source_character_id: source_id.to_owned(),
        target_character_id: target_id.to_owned(),
        kind: target.kind_id.clone(),
        confidence: target.confidence,
        origin: target.origin,
        review: ReviewState::Pending,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        validity: target.validity.clone(),
        inverse_edge_id: inverse_id.clone(),
        notes: target.notes.clone(),
        consent: consent.clone(),
        safeguard_exceptions: BTreeMap::new(),
        affinity_score_micros: Some(score_micros),
        evidence: evidence.to_vec(),
        lineage: lineage.to_vec(),
        rationale: Some(target.rationale.clone()),
    };
    let mut result = vec![forward];
    if let RelationshipDirectionality::InversePaired { inverse_kind_id } =
        &definition.directionality
    {
        result.push(RelationshipEdge {
            id: inverse_id.expect("inverse id exists for inverse-paired kind"),
            source_character_id: target_id.to_owned(),
            target_character_id: source_id.to_owned(),
            kind: inverse_kind_id.clone(),
            confidence: target.confidence,
            origin: target.origin,
            review: ReviewState::Pending,
            lock: LockState::Unlocked,
            freshness: Freshness::Current,
            validity: target.validity.clone(),
            inverse_edge_id: Some(forward_id),
            notes: target.notes.clone(),
            consent,
            safeguard_exceptions: BTreeMap::new(),
            affinity_score_micros: Some(score_micros),
            evidence: evidence.to_vec(),
            lineage: lineage.to_vec(),
            rationale: Some(target.rationale.clone()),
        });
    }
    for edge in &result {
        validate_edge_shape(
            "candidate.edges",
            &edge.id,
            &edge.source_character_id,
            edge,
            pack,
            true,
        )?;
    }
    Ok(result)
}

fn candidate_diagnostics(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    config: &RelationshipProposalConfig,
    candidate_edges: &[RelationshipEdge],
    candidate_id: &str,
) -> Result<Vec<RelationshipDiagnostic>, RelationshipError> {
    let mut hypothetical = collection.clone();
    let mut accepted = candidate_edges.to_vec();
    for edge in &mut accepted {
        edge.review = ReviewState::Accepted;
        if edge.origin == RelationshipEdgeOrigin::SuggestedNarrative {
            edge.origin = RelationshipEdgeOrigin::ReviewedSuggestion;
        }
    }
    insert_relationship_edges(
        &mut hypothetical,
        &relationship_kind_pack_ref(pack)?,
        &config.provenance,
        &accepted,
        &format!("Candidate {candidate_id} safeguard audit."),
    )?;
    relationship_graph_diagnostics(
        &hypothetical,
        pack,
        config.reference_date,
        &config.safeguards,
    )
}

fn insert_relationship_edges(
    collection: &mut CharacterCollection,
    pack_ref: &RelationshipKindPackRef,
    provenance: &Provenance,
    edges: &[RelationshipEdge],
    rationale: &str,
) -> Result<(), RelationshipError> {
    let lineage = proposal_lineage(provenance);
    for edge in edges {
        let profile = collection
            .characters
            .get_mut(&edge.source_character_id)
            .ok_or_else(|| {
                relationship_error(
                    RelationshipDiagnosticCode::MissingCharacter,
                    "edges.source_character_id",
                    "relationship edge source is absent from the collection",
                )
            })?;
        profile.provenance = merge_provenance(&profile.provenance, provenance).map_err(|_| {
            invalid_metadata(
                "provenance",
                "relationship provenance conflicts with existing profile lineage",
            )
        })?;
        if relationship_record(profile).is_none() {
            profile.extensions.insert(
                RELATIONSHIP_EXTENSION_NAMESPACE.to_owned(),
                CharacterExtension::Relationships(VersionedExtension {
                    header: ExtensionHeader {
                        namespace: RELATIONSHIP_EXTENSION_NAMESPACE.to_owned(),
                        extension_version: 1,
                        authority: "reviewed relationship graph operation".to_owned(),
                        rationale: rationale.to_owned(),
                        state: ValueState::Reviewed,
                        review: ReviewState::Accepted,
                        lock: LockState::Unlocked,
                        freshness: Freshness::Current,
                        lineage: lineage.clone(),
                        canonical_personality_write_back: ExtensionWriteBack::Forbidden,
                    },
                    value: RelationshipEdges {
                        graph_format_version: 1,
                        kind_pack: Some(pack_ref.clone()),
                        edges: BTreeMap::new(),
                    },
                }),
            );
        }
        let record = relationship_record_mut(profile).expect("relationship record inserted");
        match &record.value.kind_pack {
            Some(existing) if existing != pack_ref => {
                return Err(stale(
                    "relationships.kind_pack",
                    "relationship graph already pins a different kind pack",
                ));
            }
            None => record.value.kind_pack = Some(pack_ref.clone()),
            Some(_) => {}
        }
        for id in &lineage {
            if record.header.lineage.binary_search(id).is_err() {
                record.header.lineage.push(id.clone());
            }
        }
        record.header.lineage.sort();
        record.header.lineage.dedup();
        if record.value.edges.contains_key(&edge.id) {
            return Err(relationship_error(
                RelationshipDiagnosticCode::DuplicateEdge,
                "relationships.edges",
                "relationship operation cannot silently replace an existing edge",
            ));
        }
        let mut inserted = edge.clone();
        inserted.lineage.extend(lineage.iter().cloned());
        inserted.lineage.sort();
        inserted.lineage.dedup();
        record.value.edges.insert(inserted.id.clone(), inserted);
    }
    Ok(())
}

fn proposal_lineage(provenance: &Provenance) -> Vec<String> {
    let mut lineage = provenance
        .sources
        .iter()
        .map(|source| source.id.clone())
        .chain(
            provenance
                .transformations
                .iter()
                .map(|transformation| transformation.id.clone()),
        )
        .collect::<Vec<_>>();
    lineage.sort();
    lineage.dedup();
    lineage
}

fn rank_and_limit_candidates(
    candidates: &mut BTreeMap<String, RelationshipCandidate>,
    config: &RelationshipProposalConfig,
) {
    for target in &config.targets {
        let mut ids = candidates
            .iter()
            .filter(|(_, candidate)| candidate.target_id == target.id)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        ids.sort_by(|left, right| {
            let left = &candidates[left];
            let right = &candidates[right];
            right
                .score_micros
                .cmp(&left.score_micros)
                .then_with(|| left.seeded_sha256.cmp(&right.seeded_sha256))
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut selected = 0_u32;
        for (index, id) in ids.iter().enumerate() {
            let candidate = candidates.get_mut(id).expect("candidate id from map");
            candidate.rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
            if candidate.disposition == RelationshipCandidateDisposition::Proposed {
                selected = selected.saturating_add(1);
                if target
                    .maximum_candidates
                    .is_some_and(|maximum| selected > maximum)
                {
                    candidate.disposition = RelationshipCandidateDisposition::CapacityWithheld;
                }
            }
        }
    }
}

fn proposal_distribution(
    candidates: &BTreeMap<String, RelationshipCandidate>,
) -> RelationshipProposalDistribution {
    let mut distribution = RelationshipProposalDistribution::default();
    for candidate in candidates.values() {
        *distribution
            .by_target
            .entry(candidate.target_id.clone())
            .or_default()
            .entry(candidate_disposition(candidate.disposition).to_owned())
            .or_default() += 1;
    }
    distribution
}

const fn candidate_disposition(value: RelationshipCandidateDisposition) -> &'static str {
    match value {
        RelationshipCandidateDisposition::Proposed => "proposed",
        RelationshipCandidateDisposition::BelowThreshold => "below_threshold",
        RelationshipCandidateDisposition::CapacityWithheld => "capacity_withheld",
        RelationshipCandidateDisposition::SafeguardBlocked => "safeguard_blocked",
    }
}

/// Canonical proposal fingerprint used by complete editorial reviews.
pub fn relationship_proposal_fingerprint(
    proposal: &RelationshipProposal,
) -> Result<String, RelationshipError> {
    canonical_hash(proposal)
}

/// Canonical relationship review fingerprint retained by applied edges and receipts.
pub fn relationship_review_fingerprint(
    review: &RelationshipReview,
) -> Result<String, RelationshipError> {
    canonical_hash(review)
}

/// Validate a proposal and independently reproduce every candidate and trace.
pub fn validate_relationship_proposal(
    proposal: &RelationshipProposal,
) -> Result<(), RelationshipError> {
    if proposal.proposal_format_version != RELATIONSHIP_PROPOSAL_FORMAT_VERSION {
        return Err(invalid_metadata(
            "proposal_format_version",
            "unsupported relationship proposal version",
        ));
    }
    namespaced("id", &proposal.id)?;
    sha256("input_sha256", &proposal.input_sha256)?;
    sha256("config_sha256", &proposal.config_sha256)?;
    sha256("roster_sha256", &proposal.roster_sha256)?;
    validate_relationship_kind_pack(&proposal.kind_pack)?;
    validate_relationship_proposal_config(
        &proposal.input_collection,
        &proposal.kind_pack,
        &proposal.config,
    )?;
    if proposal.input_sha256 != collection_hash(&proposal.input_collection)?
        || proposal.kind_pack_ref != relationship_kind_pack_ref(&proposal.kind_pack)?
        || proposal.config.kind_pack != proposal.kind_pack_ref
        || proposal.config_sha256 != canonical_hash(&proposal.config)?
        || proposal.roster_sha256 != canonical_hash(&proposal.config.roster)?
    {
        return Err(stale(
            "proposal",
            "relationship proposal coordinates do not match its embedded replay inputs",
        ));
    }
    let expected_profiles = proposal
        .config
        .roster
        .iter()
        .map(|id| {
            canonical_hash(&proposal.input_collection.characters[id]).map(|hash| (id.clone(), hash))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    if proposal.profile_sha256 != expected_profiles {
        return Err(stale(
            "profile_sha256",
            "relationship proposal profile fingerprints are stale",
        ));
    }
    for (id, candidate) in &proposal.candidates {
        let path = format!("candidates.{id}");
        local(&path, id)?;
        if candidate.id != *id
            || !proposal.config.targets.iter().any(|target| {
                target.id == candidate.target_id && target.kind_id == candidate.kind_id
            })
            || !proposal
                .config
                .roster
                .contains(&candidate.source_character_id)
            || !proposal
                .config
                .roster
                .contains(&candidate.target_character_id)
            || candidate.source_character_id == candidate.target_character_id
            || candidate.score_micros > MICROS as u32
            || candidate.rank == 0
        {
            return Err(invalid_metadata(
                path,
                "relationship candidate coordinates are invalid",
            ));
        }
        sha256(&format!("{path}.seeded_sha256"), &candidate.seeded_sha256)?;
        text_value(&format!("{path}.rationale"), &candidate.rationale, 1, 2_048)?;
        validate_sorted_texts(&format!("{path}.limitations"), &candidate.limitations)?;
        for edge in &candidate.edges {
            validate_edge_shape(
                &format!("{path}.edges"),
                &edge.id,
                &edge.source_character_id,
                edge,
                &proposal.kind_pack,
                true,
            )?;
            if edge.evidence != candidate.evidence
                || edge.affinity_score_micros != Some(candidate.score_micros)
            {
                return Err(stale(
                    format!("{path}.edges"),
                    "candidate edge does not retain the exact score and evidence trace",
                ));
            }
        }
        validate_relationship_diagnostics(&format!("{path}.safeguards"), &candidate.safeguards)?;
    }
    if proposal.distribution != proposal_distribution(&proposal.candidates) {
        return Err(stale(
            "distribution",
            "relationship proposal distribution does not match its candidates",
        ));
    }
    let reproduced = build_relationship_proposal(
        &proposal.input_collection,
        &proposal.kind_pack,
        &proposal.config,
    )?;
    if reproduced != *proposal {
        return Err(stale(
            "proposal",
            "relationship proposal does not reproduce from its pinned inputs",
        ));
    }
    Ok(())
}

/// Create a complete, byte-stable review from an explicit candidate-decision map.
pub fn create_relationship_review(
    proposal: &RelationshipProposal,
    decisions: BTreeMap<String, RelationshipReviewDecision>,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
) -> Result<RelationshipReview, RelationshipError> {
    validate_relationship_proposal(proposal)?;
    let review = RelationshipReview {
        review_format_version: RELATIONSHIP_REVIEW_FORMAT_VERSION,
        proposal_sha256: relationship_proposal_fingerprint(proposal)?,
        reviewer: reviewer.into(),
        rationale: rationale.into(),
        decisions,
    };
    validate_relationship_review(proposal, &review)?;
    Ok(review)
}

/// Validate review shape without trusting an external candidate document.
pub fn validate_relationship_review_structure(
    review: &RelationshipReview,
) -> Result<(), RelationshipError> {
    if review.review_format_version != RELATIONSHIP_REVIEW_FORMAT_VERSION {
        return Err(invalid_metadata(
            "review_format_version",
            "unsupported relationship review version",
        ));
    }
    sha256("proposal_sha256", &review.proposal_sha256)?;
    namespaced("reviewer", &review.reviewer)?;
    text_value("rationale", &review.rationale, 1, 2_048)?;
    for (id, decision) in &review.decisions {
        let path = format!("decisions.{id}");
        local(&path, id)?;
        validate_review_decision_structure(&path, decision)?;
    }
    Ok(())
}

/// Validate a complete review against its exact immutable proposal.
pub fn validate_relationship_review(
    proposal: &RelationshipProposal,
    review: &RelationshipReview,
) -> Result<(), RelationshipError> {
    validate_relationship_review_structure(review)?;
    if review.proposal_sha256 != relationship_proposal_fingerprint(proposal)? {
        return Err(stale(
            "proposal_sha256",
            "relationship review does not fingerprint the exact proposal",
        ));
    }
    if review.decisions.len() != proposal.candidates.len()
        || review.decisions.keys().ne(proposal.candidates.keys())
    {
        return Err(invalid_metadata(
            "decisions",
            "relationship review requires one decision for every exact candidate id",
        ));
    }
    for (id, decision) in &review.decisions {
        let candidate = &proposal.candidates[id];
        validate_review_decision_for_candidate(proposal, candidate, decision)?;
    }
    Ok(())
}

fn validate_review_decision_structure(
    path: &str,
    decision: &RelationshipReviewDecision,
) -> Result<(), RelationshipError> {
    match decision {
        RelationshipReviewDecision::Accept { rationale, .. } => {
            if let Some(rationale) = rationale {
                text_value(&format!("{path}.rationale"), rationale, 1, 2_048)?;
            }
        }
        RelationshipReviewDecision::Edit {
            edges, rationale, ..
        }
        | RelationshipReviewDecision::Override {
            edges, rationale, ..
        }
        | RelationshipReviewDecision::Exception {
            edges, rationale, ..
        } => {
            if edges.is_empty() || edges.len() > 2 {
                return Err(invalid_metadata(
                    format!("{path}.edges"),
                    "review edit, override, or exception requires one or two complete edges",
                ));
            }
            text_value(&format!("{path}.rationale"), rationale, 1, 2_048)?;
        }
        RelationshipReviewDecision::Reject { rationale }
        | RelationshipReviewDecision::Withhold { rationale } => {
            text_value(&format!("{path}.rationale"), rationale, 1, 2_048)?;
        }
    }
    match decision {
        RelationshipReviewDecision::Override { replacements, .. }
        | RelationshipReviewDecision::Exception { replacements, .. } => {
            validate_replacements(&format!("{path}.replacements"), replacements)?;
        }
        _ => {}
    }
    if let RelationshipReviewDecision::Exception {
        exception_codes, ..
    } = decision
        && (exception_codes.is_empty()
            || exception_codes.windows(2).any(|pair| pair[0] >= pair[1])
            || exception_codes.iter().any(|code| {
                !matches!(
                    code,
                    RelationshipDiagnosticCode::AgeSafeguard
                        | RelationshipDiagnosticCode::KinshipSafeguard
                        | RelationshipDiagnosticCode::PartnershipSafeguard
                        | RelationshipDiagnosticCode::ConsentSafeguard
                )
            }))
    {
        return Err(invalid_metadata(
            format!("{path}.exception_codes"),
            "relationship exceptions require unique sorted project-safeguard codes",
        ));
    }
    Ok(())
}

fn validate_review_decision_for_candidate(
    proposal: &RelationshipProposal,
    candidate: &RelationshipCandidate,
    decision: &RelationshipReviewDecision,
) -> Result<(), RelationshipError> {
    match decision {
        RelationshipReviewDecision::Accept { .. } | RelationshipReviewDecision::Edit { .. }
            if candidate.disposition != RelationshipCandidateDisposition::Proposed =>
        {
            return Err(invalid_metadata(
                format!("decisions.{}", candidate.id),
                "only an eligible proposed relationship may be accepted or edited",
            ));
        }
        RelationshipReviewDecision::Exception {
            exception_codes, ..
        } => {
            if !proposal.config.safeguards.allow_reviewed_exceptions {
                return Err(invalid_metadata(
                    format!("decisions.{}", candidate.id),
                    "project policy disables relationship safeguard exceptions",
                ));
            }
            let required = candidate
                .safeguards
                .iter()
                .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
                .map(|diagnostic| diagnostic.code)
                .collect::<BTreeSet<_>>();
            let declared = exception_codes.iter().copied().collect::<BTreeSet<_>>();
            if !required.is_subset(&declared) {
                return Err(invalid_metadata(
                    format!("decisions.{}.exception_codes", candidate.id),
                    "exception decision must name every blocking candidate safeguard",
                ));
            }
        }
        _ => {}
    }
    let edges = match decision {
        RelationshipReviewDecision::Edit { edges, .. }
        | RelationshipReviewDecision::Override { edges, .. }
        | RelationshipReviewDecision::Exception { edges, .. } => Some(edges),
        _ => None,
    };
    if let Some(edges) = edges {
        for edge in edges {
            validate_edge_shape(
                &format!("decisions.{}.edges", candidate.id),
                &edge.id,
                &edge.source_character_id,
                edge,
                &proposal.kind_pack,
                true,
            )?;
            if !proposal.config.roster.contains(&edge.source_character_id)
                || !proposal.config.roster.contains(&edge.target_character_id)
            {
                return Err(relationship_error(
                    RelationshipDiagnosticCode::MissingCharacter,
                    format!("decisions.{}.edges", candidate.id),
                    "reviewed relationship edge is outside the pinned roster",
                ));
            }
        }
    }
    if let RelationshipReviewDecision::Edit { edges, .. } = decision {
        let topology = |edges: &[RelationshipEdge]| {
            edges
                .iter()
                .map(|edge| {
                    (
                        edge.id.clone(),
                        edge.source_character_id.clone(),
                        edge.target_character_id.clone(),
                        edge.kind.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        if topology(edges) != topology(&candidate.edges) {
            return Err(invalid_metadata(
                format!("decisions.{}.edges", candidate.id),
                "an edit may change metadata but must retain candidate edge identities and topology",
            ));
        }
    }
    Ok(())
}

/// Independently replay a complete review and atomically return its receipt and output graph.
pub fn apply_reviewed_relationships(
    collection: &CharacterCollection,
    proposal: &RelationshipProposal,
    review: &RelationshipReview,
) -> Result<RelationshipReceipt, RelationshipError> {
    validate_relationship_proposal(proposal)?;
    validate_relationship_review(proposal, review)?;
    let receipt = apply_relationship_review_internal(collection, proposal, review)?;
    validate_relationship_receipt_structure(&receipt)?;
    Ok(receipt)
}

fn apply_relationship_review_internal(
    collection: &CharacterCollection,
    proposal: &RelationshipProposal,
    review: &RelationshipReview,
) -> Result<RelationshipReceipt, RelationshipError> {
    if collection_hash(collection)? != proposal.input_sha256 {
        return Err(stale(
            "collection",
            "Character collection changed after the relationship proposal was produced",
        ));
    }
    let reproduced =
        build_relationship_proposal(collection, &proposal.kind_pack, &proposal.config)?;
    if reproduced != *proposal {
        return Err(stale(
            "proposal",
            "relationship proposal no longer reproduces from the current collection",
        ));
    }
    let proposal_sha256 = relationship_proposal_fingerprint(proposal)?;
    let review_sha256 = relationship_review_fingerprint(review)?;
    let (application_provenance, application_id) = relationship_application_provenance(
        &proposal.config.provenance,
        &proposal_sha256,
        &review_sha256,
    )?;
    let mut output = collection.clone();
    let mut applied_edges = BTreeMap::<String, Vec<RelationshipAppliedEdge>>::new();

    for (candidate_id, decision) in &review.decisions {
        let candidate = &proposal.candidates[candidate_id];
        let (source_edges, replacements, lock, rationale, origin, exception_codes) = match decision
        {
            RelationshipReviewDecision::Accept { lock, rationale } => (
                Some(candidate.edges.as_slice()),
                &[][..],
                *lock,
                rationale.as_deref().unwrap_or(&candidate.rationale),
                candidate.origin,
                &[][..],
            ),
            RelationshipReviewDecision::Edit {
                edges,
                lock,
                rationale,
            } => (
                Some(edges.as_slice()),
                &[][..],
                *lock,
                rationale.as_str(),
                candidate.origin,
                &[][..],
            ),
            RelationshipReviewDecision::Override {
                edges,
                replacements,
                lock,
                rationale,
            } => (
                Some(edges.as_slice()),
                replacements.as_slice(),
                *lock,
                rationale.as_str(),
                RelationshipEdgeOrigin::Authored,
                &[][..],
            ),
            RelationshipReviewDecision::Exception {
                edges,
                exception_codes,
                replacements,
                lock,
                rationale,
            } => (
                Some(edges.as_slice()),
                replacements.as_slice(),
                *lock,
                rationale.as_str(),
                RelationshipEdgeOrigin::Authored,
                exception_codes.as_slice(),
            ),
            RelationshipReviewDecision::Reject { .. }
            | RelationshipReviewDecision::Withhold { .. } => {
                applied_edges.insert(candidate_id.clone(), Vec::new());
                continue;
            }
        };
        remove_relationship_edges(&mut output, replacements)?;
        let normalized = normalize_reviewed_edges(
            source_edges.expect("applied decision has edge source"),
            origin,
            lock,
            rationale,
            &review.reviewer,
            exception_codes,
            &application_id,
        );
        insert_relationship_edges(
            &mut output,
            &proposal.kind_pack_ref,
            &application_provenance,
            &normalized,
            rationale,
        )?;
        applied_edges.insert(
            candidate_id.clone(),
            normalized
                .into_iter()
                .map(|edge| RelationshipAppliedEdge {
                    owner_character_id: edge.source_character_id.clone(),
                    edge,
                })
                .collect(),
        );
    }

    if output.characters != collection.characters {
        output.revision = collection.revision.saturating_add(1);
    }
    validate_relationship_graph(
        &output,
        &proposal.kind_pack,
        proposal.config.reference_date,
        &proposal.config.safeguards,
    )?;
    validate_character_collection(&output).map_err(|failure| {
        relationship_error(
            RelationshipDiagnosticCode::InvalidMetadata,
            failure.diagnostic().path.clone(),
            "reviewed relationship application produced an invalid Character collection",
        )
    })?;
    let output_sha256 = collection_hash(&output)?;
    Ok(RelationshipReceipt {
        receipt_format_version: RELATIONSHIP_RECEIPT_FORMAT_VERSION,
        id: derived_document_id(&proposal.id, "receipt"),
        proposal: proposal.clone(),
        review: review.clone(),
        proposal_sha256,
        review_sha256,
        applied_edges,
        output_collection: output,
        output_sha256,
    })
}

fn normalize_reviewed_edges(
    edges: &[RelationshipEdge],
    origin: RelationshipEdgeOrigin,
    lock: LockState,
    rationale: &str,
    reviewer: &str,
    exception_codes: &[RelationshipDiagnosticCode],
    application_id: &str,
) -> Vec<RelationshipEdge> {
    edges
        .iter()
        .cloned()
        .map(|mut edge| {
            edge.origin = match origin {
                RelationshipEdgeOrigin::SuggestedNarrative => {
                    RelationshipEdgeOrigin::ReviewedSuggestion
                }
                value => value,
            };
            edge.review = ReviewState::Accepted;
            edge.lock = lock;
            edge.freshness = Freshness::Current;
            edge.rationale = Some(rationale.to_owned());
            if edge
                .lineage
                .binary_search_by(|value| value.as_str().cmp(application_id))
                .is_err()
            {
                edge.lineage.push(application_id.to_owned());
                edge.lineage.sort();
                edge.lineage.dedup();
            }
            for code in exception_codes {
                let public_code = relationship_diagnostic_code(*code);
                let id = format!("exception_{}", public_code.to_ascii_lowercase());
                edge.safeguard_exceptions.insert(
                    id.clone(),
                    RelationshipSafeguardException {
                        id,
                        code: public_code.to_owned(),
                        reviewer: reviewer.to_owned(),
                        rationale: rationale.to_owned(),
                        lineage: vec![application_id.to_owned()],
                    },
                );
            }
            edge
        })
        .collect()
}

fn relationship_application_provenance(
    base: &Provenance,
    proposal_sha256: &str,
    review_sha256: &str,
) -> Result<(Provenance, String), RelationshipError> {
    let mut provenance = base.clone();
    let application_hash = canonical_hash(&(proposal_sha256, review_sha256))?;
    let id = format!("relationship_review_{}", &application_hash[..16]);
    let inputs = proposal_lineage(base);
    provenance.transformations.push(ProvenanceTransformation {
        id: id.clone(),
        inputs,
        description: "Applied one complete author-reviewed relationship proposal without changing canonical personality or other protected domains.".to_owned(),
    });
    provenance
        .transformations
        .sort_by(|left, right| left.id.cmp(&right.id));
    provenance
        .claims
        .entry("extensions.relationships".to_owned())
        .or_default()
        .push(id.clone());
    if let Some(claims) = provenance.claims.get_mut("extensions.relationships") {
        claims.sort();
        claims.dedup();
    }
    validate_provenance(&provenance).map_err(|_| {
        invalid_metadata(
            "provenance",
            "relationship application provenance is invalid",
        )
    })?;
    Ok((provenance, id))
}

fn remove_relationship_edges(
    collection: &mut CharacterCollection,
    removals: &[RelationshipEdgeRemoval],
) -> Result<(), RelationshipError> {
    for removal in removals {
        let profile = collection
            .characters
            .get_mut(&removal.owner_character_id)
            .ok_or_else(|| {
                relationship_error(
                    RelationshipDiagnosticCode::MissingCharacter,
                    "replacements.owner_character_id",
                    "relationship replacement owner is absent from the collection",
                )
            })?;
        let record = relationship_record_mut(profile).ok_or_else(|| {
            relationship_error(
                RelationshipDiagnosticCode::MissingCharacter,
                "replacements.edge_id",
                "relationship replacement edge is absent",
            )
        })?;
        let edge = record.value.edges.get(&removal.edge_id).ok_or_else(|| {
            relationship_error(
                RelationshipDiagnosticCode::MissingCharacter,
                "replacements.edge_id",
                "relationship replacement edge is absent",
            )
        })?;
        if canonical_hash(edge)? != removal.expected_edge_sha256 {
            return Err(stale(
                "replacements.expected_edge_sha256",
                "relationship replacement edge fingerprint is stale",
            ));
        }
        if edge.lock == LockState::Locked && !removal.override_locked {
            return Err(relationship_error(
                RelationshipDiagnosticCode::LockedEdge,
                "replacements.override_locked",
                "locked relationship edge requires an explicit reviewed override",
            ));
        }
        record.value.edges.remove(&removal.edge_id);
    }
    Ok(())
}

fn validate_replacements(
    path: &str,
    replacements: &[RelationshipEdgeRemoval],
) -> Result<(), RelationshipError> {
    let mut prior = None::<(&str, &str)>;
    for (index, replacement) in replacements.iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        namespaced(
            &format!("{item_path}.owner_character_id"),
            &replacement.owner_character_id,
        )?;
        local(&format!("{item_path}.edge_id"), &replacement.edge_id)?;
        sha256(
            &format!("{item_path}.expected_edge_sha256"),
            &replacement.expected_edge_sha256,
        )?;
        let key = (
            replacement.owner_character_id.as_str(),
            replacement.edge_id.as_str(),
        );
        if prior.is_some_and(|prior| prior >= key) {
            return Err(invalid_metadata(
                path,
                "relationship replacements must be unique and sorted by owner then edge id",
            ));
        }
        prior = Some(key);
    }
    Ok(())
}

fn validate_relationship_receipt_structure(
    receipt: &RelationshipReceipt,
) -> Result<(), RelationshipError> {
    if receipt.receipt_format_version != RELATIONSHIP_RECEIPT_FORMAT_VERSION {
        return Err(invalid_metadata(
            "receipt_format_version",
            "unsupported relationship receipt version",
        ));
    }
    namespaced("id", &receipt.id)?;
    sha256("proposal_sha256", &receipt.proposal_sha256)?;
    sha256("review_sha256", &receipt.review_sha256)?;
    sha256("output_sha256", &receipt.output_sha256)?;
    if receipt.proposal_sha256 != relationship_proposal_fingerprint(&receipt.proposal)?
        || receipt.review_sha256 != relationship_review_fingerprint(&receipt.review)?
        || receipt.output_sha256 != collection_hash(&receipt.output_collection)?
        || receipt
            .applied_edges
            .keys()
            .ne(receipt.proposal.candidates.keys())
    {
        return Err(stale(
            "receipt",
            "relationship receipt fingerprints or applied-candidate map are inconsistent",
        ));
    }
    validate_relationship_review(&receipt.proposal, &receipt.review)?;
    Ok(())
}

/// Validate a receipt by independently replaying its proposal and review.
pub fn validate_relationship_receipt(
    receipt: &RelationshipReceipt,
) -> Result<(), RelationshipError> {
    validate_relationship_receipt_structure(receipt)?;
    let reproduced = apply_relationship_review_internal(
        &receipt.proposal.input_collection,
        &receipt.proposal,
        &receipt.review,
    )?;
    if reproduced != *receipt {
        return Err(stale(
            "receipt",
            "relationship receipt does not reproduce from its embedded proposal and review",
        ));
    }
    Ok(())
}

/// Apply an explicit authored/imported graph revision atomically.
pub fn apply_relationship_graph_revision(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    revision: &RelationshipGraphRevision,
) -> Result<CharacterCollection, RelationshipError> {
    validate_relationship_kind_pack(pack)?;
    validate_relationship_graph_revision(collection, pack, revision)?;
    let mut output = collection.clone();
    remove_relationship_edges(&mut output, &revision.removals)?;
    insert_relationship_edges(
        &mut output,
        &revision.kind_pack,
        &revision.provenance,
        &revision.additions,
        &revision.rationale,
    )?;
    if output.characters != collection.characters {
        output.revision = collection.revision.saturating_add(1);
    }
    validate_relationship_graph(&output, pack, revision.reference_date, &revision.safeguards)?;
    Ok(output)
}

/// Validate direct revision structure without trusting ambient collection state.
pub fn validate_relationship_graph_revision_structure(
    revision: &RelationshipGraphRevision,
) -> Result<(), RelationshipError> {
    if revision.revision_format_version != RELATIONSHIP_REVISION_FORMAT_VERSION {
        return Err(invalid_metadata(
            "revision_format_version",
            "unsupported relationship graph-revision version",
        ));
    }
    namespaced("id", &revision.id)?;
    sha256("expected_input_sha256", &revision.expected_input_sha256)?;
    validate_pack_ref("kind_pack", &revision.kind_pack)?;
    validate_date("reference_date", revision.reference_date)?;
    validate_safeguards("safeguards", &revision.safeguards)?;
    if revision.additions.is_empty() && revision.removals.is_empty() {
        return Err(invalid_metadata(
            "revision",
            "relationship graph revision requires at least one addition or removal",
        ));
    }
    if revision.additions.len() > 65_536 || revision.removals.len() > 65_536 {
        return Err(invalid_metadata(
            "revision",
            "relationship graph revision exceeds the supported operation count",
        ));
    }
    validate_replacements("removals", &revision.removals)?;
    text_value("rationale", &revision.rationale, 1, 2_048)?;
    validate_provenance(&revision.provenance).map_err(|_| {
        invalid_metadata(
            "provenance",
            "relationship graph-revision provenance is invalid",
        )
    })?;
    if proposal_lineage(&revision.provenance).is_empty() {
        return Err(invalid_metadata(
            "provenance",
            "relationship graph revision requires explicit source or transformation lineage",
        ));
    }
    Ok(())
}

/// Validate a direct revision against its exact input collection and pack.
pub fn validate_relationship_graph_revision(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    revision: &RelationshipGraphRevision,
) -> Result<(), RelationshipError> {
    validate_relationship_graph_revision_structure(revision)?;
    if revision.expected_input_sha256 != collection_hash(collection)? {
        return Err(stale(
            "expected_input_sha256",
            "relationship graph revision does not fingerprint the current collection",
        ));
    }
    if revision.kind_pack != relationship_kind_pack_ref(pack)? {
        return Err(stale(
            "kind_pack",
            "relationship graph revision does not pin the exact selected kind pack",
        ));
    }
    let mut ids = BTreeSet::new();
    for edge in &revision.additions {
        if !matches!(
            edge.origin,
            RelationshipEdgeOrigin::Authored | RelationshipEdgeOrigin::Imported
        ) || !matches!(
            edge.review,
            ReviewState::NotRequired | ReviewState::Accepted
        ) {
            return Err(invalid_metadata(
                "additions",
                "direct graph revisions may add only authored or explicitly reviewed imported edges",
            ));
        }
        let key = (edge.source_character_id.as_str(), edge.id.as_str());
        if !ids.insert(key) {
            return Err(invalid_metadata(
                "additions",
                "relationship graph-revision additions must have unique owner/id coordinates",
            ));
        }
        validate_edge_shape(
            "additions",
            &edge.id,
            &edge.source_character_id,
            edge,
            pack,
            false,
        )?;
        if !collection
            .characters
            .contains_key(&edge.source_character_id)
            || !collection
                .characters
                .contains_key(&edge.target_character_id)
        {
            return Err(relationship_error(
                RelationshipDiagnosticCode::MissingCharacter,
                "additions",
                "relationship graph-revision edge references an absent character",
            ));
        }
    }
    Ok(())
}

/// List graph edges with stable filtering and kind semantics.
pub fn list_relationships(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    reference_date: RelationshipDate,
    safeguards: &RelationshipSafeguards,
    filter: &RelationshipFilter,
) -> Result<Vec<RelationshipEdgeView>, RelationshipError> {
    validate_relationship_filter(filter)?;
    validate_relationship_graph(collection, pack, reference_date, safeguards)?;
    let mut result = Vec::new();
    for (owner_id, profile) in &collection.characters {
        let Some(record) = relationship_record(profile) else {
            continue;
        };
        for edge in record.value.edges.values() {
            if filter
                .source_character_id
                .as_ref()
                .is_some_and(|id| id != &edge.source_character_id)
                || filter
                    .target_character_id
                    .as_ref()
                    .is_some_and(|id| id != &edge.target_character_id)
                || (!filter.kind_ids.is_empty()
                    && filter.kind_ids.binary_search(&edge.kind).is_err())
                || (!filter.origins.is_empty() && !filter.origins.contains(&edge.origin))
                || filter
                    .active_on
                    .is_some_and(|date| !edge_is_active(edge, date))
            {
                continue;
            }
            let definition = &pack.kinds[&edge.kind];
            result.push(RelationshipEdgeView {
                owner_character_id: owner_id.clone(),
                edge: edge.clone(),
                family: definition.family,
                directionality: definition.directionality.clone(),
            });
        }
    }
    result.sort_by(|left, right| {
        (
            &left.edge.source_character_id,
            &left.edge.target_character_id,
            &left.edge.kind,
            &left.edge.id,
        )
            .cmp(&(
                &right.edge.source_character_id,
                &right.edge.target_character_id,
                &right.edge.kind,
                &right.edge.id,
            ))
    });
    Ok(result)
}

/// Validate read-only relationship filters.
pub fn validate_relationship_filter(filter: &RelationshipFilter) -> Result<(), RelationshipError> {
    if let Some(id) = &filter.source_character_id {
        namespaced("filter.source_character_id", id)?;
    }
    if let Some(id) = &filter.target_character_id {
        namespaced("filter.target_character_id", id)?;
    }
    validate_sorted_namespaced("filter.kind_ids", &filter.kind_ids, 0, 4_096)?;
    validate_unique_sorted("filter.origins", &filter.origins)?;
    if let Some(date) = filter.active_on {
        validate_date("filter.active_on", date)?;
    }
    Ok(())
}

/// Inspect and reconcile without mutating; every repair remains advisory.
pub fn reconcile_relationship_graph(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    reference_date: RelationshipDate,
    safeguards: &RelationshipSafeguards,
) -> Result<RelationshipReconciliationReport, RelationshipError> {
    validate_relationship_kind_pack(pack)?;
    let diagnostics = relationship_graph_diagnostics(collection, pack, reference_date, safeguards)?;
    let edges = list_relationships_unchecked(collection, pack);
    let mut repairs = inverse_repairs(collection, pack);
    repairs.extend(duplicate_repairs(&edges));
    for diagnostic in &diagnostics {
        if !matches!(
            diagnostic.code,
            RelationshipDiagnosticCode::BrokenInverse | RelationshipDiagnosticCode::DuplicateEdge
        ) {
            let hash = canonical_hash(&(diagnostic.code, diagnostic.path.as_str()))?;
            repairs.push(RelationshipRepairSuggestion {
                id: format!("repair_{}", &hash[..20]),
                action: RelationshipRepairAction::ReviewSafeguard {
                    diagnostic_code: diagnostic.code,
                },
                rationale: "Review the redaction-safe diagnostic and author a separate explicit revision or exception if appropriate.".to_owned(),
            });
        }
    }
    repairs.sort_by(|left, right| left.id.cmp(&right.id));
    repairs.dedup_by(|left, right| left.id == right.id);
    let report = RelationshipReconciliationReport {
        report_format_version: RELATIONSHIP_RECONCILIATION_FORMAT_VERSION,
        id: derived_document_id(&collection.id, "relationship_reconciliation"),
        input_sha256: collection_hash(collection)?,
        kind_pack: relationship_kind_pack_ref(pack)?,
        reference_date,
        edges,
        diagnostics,
        repairs,
    };
    validate_relationship_reconciliation_report(&report)?;
    Ok(report)
}

fn list_relationships_unchecked(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
) -> Vec<RelationshipEdgeView> {
    let mut result = collection
        .characters
        .iter()
        .flat_map(|(owner_id, profile)| {
            relationship_record(profile)
                .into_iter()
                .flat_map(move |record| {
                    record.value.edges.values().filter_map(move |edge| {
                        pack.kinds
                            .get(&edge.kind)
                            .map(|definition| RelationshipEdgeView {
                                owner_character_id: owner_id.clone(),
                                edge: edge.clone(),
                                family: definition.family,
                                directionality: definition.directionality.clone(),
                            })
                    })
                })
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        (&left.owner_character_id, &left.edge.id).cmp(&(&right.owner_character_id, &right.edge.id))
    });
    result
}

fn inverse_repairs(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
) -> Vec<RelationshipRepairSuggestion> {
    let mut result = Vec::new();
    for (owner_id, profile) in &collection.characters {
        let Some(record) = relationship_record(profile) else {
            continue;
        };
        for edge in record.value.edges.values() {
            let Some(definition) = pack.kinds.get(&edge.kind) else {
                continue;
            };
            let RelationshipDirectionality::InversePaired { inverse_kind_id } =
                &definition.directionality
            else {
                continue;
            };
            let Some(inverse_id) = &edge.inverse_edge_id else {
                continue;
            };
            let exists = collection
                .characters
                .get(&edge.target_character_id)
                .and_then(relationship_record)
                .is_some_and(|record| record.value.edges.contains_key(inverse_id));
            if exists {
                continue;
            }
            let mut inverse = edge.clone();
            inverse.id.clone_from(inverse_id);
            inverse
                .source_character_id
                .clone_from(&edge.target_character_id);
            inverse
                .target_character_id
                .clone_from(&edge.source_character_id);
            inverse.kind.clone_from(inverse_kind_id);
            inverse.inverse_edge_id = Some(edge.id.clone());
            let repair_key = format!("inverse\0{owner_id}\0{}", edge.id);
            let digest = format!("{:x}", Sha256::digest(repair_key.as_bytes()));
            result.push(RelationshipRepairSuggestion {
                id: format!("repair_inverse_{}", &digest[..20]),
                action: RelationshipRepairAction::AddInverse {
                    owner_character_id: inverse.source_character_id.clone(),
                    edge: Box::new(inverse),
                },
                rationale: "Author the missing explicit reciprocal edge declared by the immutable kind pack.".to_owned(),
            });
        }
    }
    result
}

fn duplicate_repairs(edges: &[RelationshipEdgeView]) -> Vec<RelationshipRepairSuggestion> {
    let mut grouped = BTreeMap::<(String, String, String), Vec<&RelationshipEdgeView>>::new();
    for view in edges {
        grouped
            .entry((
                view.edge.source_character_id.clone(),
                view.edge.target_character_id.clone(),
                view.edge.kind.clone(),
            ))
            .or_default()
            .push(view);
    }
    let mut result = Vec::new();
    for values in grouped.into_values().filter(|values| values.len() > 1) {
        for view in values.into_iter().skip(1) {
            let repair_key = format!("duplicate\0{}\0{}", view.owner_character_id, view.edge.id);
            let digest = format!("{:x}", Sha256::digest(repair_key.as_bytes()));
            result.push(RelationshipRepairSuggestion {
                id: format!("repair_duplicate_{}", &digest[..20]),
                action: RelationshipRepairAction::RemoveDuplicate {
                    owner_character_id: view.owner_character_id.clone(),
                    edge_id: view.edge.id.clone(),
                },
                rationale:
                    "Review and remove the later duplicate through a fingerprinted graph revision."
                        .to_owned(),
            });
        }
    }
    result
}

/// Export a review-only, row-oriented CSV view of relationship edges.
///
/// CSV is deliberately not accepted by any relationship mutation API. It is an inspection
/// surface only; JSON or RON review documents remain the authority for changes.
pub fn relationship_edge_review_csv(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    reference_date: RelationshipDate,
    safeguards: &RelationshipSafeguards,
    filter: &RelationshipFilter,
) -> Result<String, RelationshipError> {
    let edges = list_relationships(collection, pack, reference_date, safeguards, filter)?;
    let mut output = String::new();
    output.push_str("review_only,owner_character_id,edge_id,source_character_id,target_character_id,kind,family,directionality,origin,review,lock,freshness,confidence,active_on,valid_from,valid_to,affinity_score_micros,consent_state,evidence_count,rationale\n");
    for view in edges {
        let active_on = edge_is_active(&view.edge, reference_date).to_string();
        let valid_from = view
            .edge
            .validity
            .as_ref()
            .and_then(|period| period.start)
            .map_or_else(String::new, relationship_date_label);
        let valid_to = view
            .edge
            .validity
            .as_ref()
            .and_then(|period| period.end)
            .map_or_else(String::new, relationship_date_label);
        let row = vec![
            "true".to_owned(),
            view.owner_character_id,
            view.edge.id,
            view.edge.source_character_id,
            view.edge.target_character_id,
            view.edge.kind,
            relationship_family_label(view.family).to_owned(),
            relationship_directionality_label(&view.directionality),
            relationship_origin_label(view.edge.origin).to_owned(),
            review_state_label(view.edge.review).to_owned(),
            lock_state_label(view.edge.lock).to_owned(),
            freshness_label(view.edge.freshness).to_owned(),
            confidence_label(view.edge.confidence).to_owned(),
            active_on,
            valid_from,
            valid_to,
            view.edge
                .affinity_score_micros
                .map_or_else(String::new, |value| value.to_string()),
            view.edge
                .consent
                .as_ref()
                .map_or("", |consent| consent_state_label(consent.state))
                .to_owned(),
            view.edge.evidence.len().to_string(),
            view.edge.rationale.unwrap_or_default(),
        ];
        output.push_str(&csv_row(&row));
    }
    Ok(output)
}

/// Export a review-only dense roster matrix.
///
/// Each non-empty cell contains stable `kind:edge_id` tokens and, when present, the advisory
/// score in millionths. Symmetric edges are mirrored for readability even though canon stores
/// the ordered pair only once.
pub fn relationship_dense_matrix_review_csv(
    collection: &CharacterCollection,
    pack: &RelationshipKindPack,
    reference_date: RelationshipDate,
    safeguards: &RelationshipSafeguards,
    filter: &RelationshipFilter,
) -> Result<String, RelationshipError> {
    let edges = list_relationships(collection, pack, reference_date, safeguards, filter)?;
    let roster = collection.characters.keys().cloned().collect::<Vec<_>>();
    let mut cells = BTreeMap::<(String, String), Vec<String>>::new();
    for view in edges {
        let token = view.edge.affinity_score_micros.map_or_else(
            || format!("{}:{}", view.edge.kind, view.edge.id),
            |score| format!("{}:{}@{score}", view.edge.kind, view.edge.id),
        );
        cells
            .entry((
                view.edge.source_character_id.clone(),
                view.edge.target_character_id.clone(),
            ))
            .or_default()
            .push(token.clone());
        if matches!(view.directionality, RelationshipDirectionality::Symmetric) {
            cells
                .entry((view.edge.target_character_id, view.edge.source_character_id))
                .or_default()
                .push(token);
        }
    }
    for values in cells.values_mut() {
        values.sort();
        values.dedup();
    }

    let mut output = String::new();
    let mut header = Vec::with_capacity(roster.len() + 2);
    header.push("review_only".to_owned());
    header.push("character_id".to_owned());
    header.extend(roster.iter().cloned());
    output.push_str(&csv_row(&header));
    for source in &roster {
        let mut row = Vec::with_capacity(roster.len() + 2);
        row.push("true".to_owned());
        row.push(source.clone());
        for target in &roster {
            row.push(
                cells
                    .get(&(source.clone(), target.clone()))
                    .map(|values| values.join(";"))
                    .unwrap_or_default(),
            );
        }
        output.push_str(&csv_row(&row));
    }
    Ok(output)
}

fn relationship_family_label(value: RelationshipKindFamily) -> &'static str {
    match value {
        RelationshipKindFamily::Kinship => "kinship",
        RelationshipKindFamily::Friendship => "friendship",
        RelationshipKindFamily::Mentorship => "mentorship",
        RelationshipKindFamily::Rivalry => "rivalry",
        RelationshipKindFamily::Partnership => "partnership",
        RelationshipKindFamily::Affinity => "affinity",
        RelationshipKindFamily::Narrative => "narrative",
        RelationshipKindFamily::Custom => "custom",
    }
}

fn relationship_directionality_label(value: &RelationshipDirectionality) -> String {
    match value {
        RelationshipDirectionality::Directed => "directed".to_owned(),
        RelationshipDirectionality::Symmetric => "symmetric".to_owned(),
        RelationshipDirectionality::InversePaired { inverse_kind_id } => {
            format!("inverse_paired:{inverse_kind_id}")
        }
    }
}

fn relationship_origin_label(value: RelationshipEdgeOrigin) -> &'static str {
    match value {
        RelationshipEdgeOrigin::Authored => "authored",
        RelationshipEdgeOrigin::Imported => "imported",
        RelationshipEdgeOrigin::ComputedAffinity => "computed_affinity",
        RelationshipEdgeOrigin::SuggestedNarrative => "suggested_narrative",
        RelationshipEdgeOrigin::ReviewedSuggestion => "reviewed_suggestion",
    }
}

fn review_state_label(value: ReviewState) -> &'static str {
    match value {
        ReviewState::NotRequired => "not_required",
        ReviewState::Pending => "pending",
        ReviewState::Accepted => "accepted",
        ReviewState::Rejected => "rejected",
    }
}

fn lock_state_label(value: LockState) -> &'static str {
    match value {
        LockState::Unlocked => "unlocked",
        LockState::Locked => "locked",
    }
}

fn freshness_label(value: Freshness) -> &'static str {
    match value {
        Freshness::Current => "current",
        Freshness::Stale => "stale",
    }
}

fn confidence_label(value: Confidence) -> &'static str {
    match value {
        Confidence::Unknown => "unknown",
        Confidence::Low => "low",
        Confidence::Moderate => "moderate",
        Confidence::High => "high",
    }
}

fn consent_state_label(value: RelationshipConsentState) -> &'static str {
    match value {
        RelationshipConsentState::Affirmed => "affirmed",
        RelationshipConsentState::NotApplicable => "not_applicable",
        RelationshipConsentState::Unknown => "unknown",
        RelationshipConsentState::Withheld => "withheld",
    }
}

fn relationship_date_label(value: RelationshipDate) -> String {
    format!("{:+07}-{:02}-{:02}", value.year, value.month, value.day)
}

fn csv_row(values: &[String]) -> String {
    let mut output = values
        .iter()
        .map(|value| csv_cell(value))
        .collect::<Vec<_>>()
        .join(",");
    output.push('\n');
    output
}

fn csv_cell(value: &str) -> String {
    // Prevent a spreadsheet from interpreting author-controlled text as a formula.
    let safe = if value.starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if safe.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

fn validate_relationship_reconciliation_report(
    report: &RelationshipReconciliationReport,
) -> Result<(), RelationshipError> {
    if report.report_format_version != RELATIONSHIP_RECONCILIATION_FORMAT_VERSION {
        return Err(invalid_metadata(
            "report_format_version",
            "unsupported relationship reconciliation-report version",
        ));
    }
    namespaced("id", &report.id)?;
    sha256("input_sha256", &report.input_sha256)?;
    validate_pack_ref("kind_pack", &report.kind_pack)?;
    validate_date("reference_date", report.reference_date)?;
    if report.edges.len() > 65_536
        || report.diagnostics.len() > 65_536
        || report.repairs.len() > 65_536
    {
        return Err(invalid_metadata(
            "report",
            "relationship reconciliation report exceeds supported bounds",
        ));
    }
    let mut prior_edge = None::<(&str, &str)>;
    for (index, view) in report.edges.iter().enumerate() {
        let path = format!("edges[{index}]");
        namespaced(
            &format!("{path}.owner_character_id"),
            &view.owner_character_id,
        )?;
        validate_report_edge(
            &format!("{path}.edge"),
            &view.owner_character_id,
            &view.edge,
        )?;
        match view.directionality {
            RelationshipDirectionality::InversePaired { .. }
                if view.edge.inverse_edge_id.is_none() =>
            {
                return Err(invalid_metadata(
                    format!("{path}.directionality"),
                    "inverse-paired report edge requires reciprocal edge metadata",
                ));
            }
            RelationshipDirectionality::Directed | RelationshipDirectionality::Symmetric
                if view.edge.inverse_edge_id.is_some() =>
            {
                return Err(invalid_metadata(
                    format!("{path}.directionality"),
                    "non-inverse report edge must omit reciprocal edge metadata",
                ));
            }
            _ => {}
        }
        let key = (view.owner_character_id.as_str(), view.edge.id.as_str());
        if prior_edge.is_some_and(|prior| prior >= key) {
            return Err(invalid_metadata(
                "edges",
                "reconciliation edges must be unique and sorted by owner then edge id",
            ));
        }
        prior_edge = Some(key);
    }
    validate_relationship_diagnostics("diagnostics", &report.diagnostics)?;
    let mut prior_repair = None;
    for (index, repair) in report.repairs.iter().enumerate() {
        let path = format!("repairs[{index}]");
        local(&format!("{path}.id"), &repair.id)?;
        if prior_repair.is_some_and(|prior: &str| prior >= repair.id.as_str()) {
            return Err(invalid_metadata(
                "repairs",
                "relationship repair suggestions must have unique sorted ids",
            ));
        }
        prior_repair = Some(repair.id.as_str());
        text_value(&format!("{path}.rationale"), &repair.rationale, 1, 2_048)?;
        match &repair.action {
            RelationshipRepairAction::AddInverse {
                owner_character_id,
                edge,
            } => {
                namespaced(
                    &format!("{path}.action.owner_character_id"),
                    owner_character_id,
                )?;
                validate_report_edge(&format!("{path}.action.edge"), owner_character_id, edge)?;
                if edge.inverse_edge_id.is_none() {
                    return Err(invalid_metadata(
                        format!("{path}.action.edge.inverse_edge_id"),
                        "inverse repair requires reciprocal edge metadata",
                    ));
                }
            }
            RelationshipRepairAction::RemoveDuplicate {
                owner_character_id,
                edge_id,
            } => {
                namespaced(
                    &format!("{path}.action.owner_character_id"),
                    owner_character_id,
                )?;
                local(&format!("{path}.action.edge_id"), edge_id)?;
            }
            RelationshipRepairAction::ReviewSafeguard { .. } => {}
        }
    }
    Ok(())
}

fn validate_report_edge(
    path: &str,
    owner_character_id: &str,
    edge: &RelationshipEdge,
) -> Result<(), RelationshipError> {
    local(&format!("{path}.id"), &edge.id)?;
    namespaced(
        &format!("{path}.source_character_id"),
        &edge.source_character_id,
    )?;
    if edge.source_character_id != owner_character_id {
        return Err(invalid_metadata(
            format!("{path}.source_character_id"),
            "relationship edge source must equal its owning character",
        ));
    }
    namespaced(
        &format!("{path}.target_character_id"),
        &edge.target_character_id,
    )?;
    namespaced(&format!("{path}.kind"), &edge.kind)?;
    if let Some(validity) = &edge.validity {
        validate_validity(&format!("{path}.validity"), validity)?;
    }
    if let Some(inverse_edge_id) = &edge.inverse_edge_id {
        local(&format!("{path}.inverse_edge_id"), inverse_edge_id)?;
    }
    for (id, note) in &edge.notes {
        let note_path = format!("{path}.notes.{id}");
        local(&note_path, id)?;
        if note.id != *id {
            return Err(invalid_metadata(
                format!("{note_path}.id"),
                "relationship note id must equal its containing map key",
            ));
        }
        text_value(&format!("{note_path}.content"), &note.content, 1, 4_096)?;
        validate_sorted_local_or_namespaced(&format!("{note_path}.lineage"), &note.lineage)?;
    }
    if let Some(consent) = &edge.consent {
        validate_consent(&format!("{path}.consent"), consent)?;
    }
    for (id, exception) in &edge.safeguard_exceptions {
        let exception_path = format!("{path}.safeguard_exceptions.{id}");
        local(&exception_path, id)?;
        if exception.id != *id || !is_safeguard_code(&exception.code) {
            return Err(invalid_metadata(
                exception_path,
                "relationship safeguard exception is invalid",
            ));
        }
        namespaced(
            &format!("{path}.safeguard_exceptions.{id}.reviewer"),
            &exception.reviewer,
        )?;
        text_value(
            &format!("{path}.safeguard_exceptions.{id}.rationale"),
            &exception.rationale,
            1,
            2_048,
        )?;
        validate_sorted_local_or_namespaced(
            &format!("{path}.safeguard_exceptions.{id}.lineage"),
            &exception.lineage,
        )?;
    }
    if edge
        .affinity_score_micros
        .is_some_and(|score| score > MICROS as u32)
    {
        return Err(invalid_metadata(
            format!("{path}.affinity_score_micros"),
            "relationship advisory score must be bounded millionths",
        ));
    }
    let mut evidence_ids = BTreeSet::new();
    for (index, evidence) in edge.evidence.iter().enumerate() {
        let evidence_path = format!("{path}.evidence[{index}]");
        local(&format!("{evidence_path}.id"), &evidence.id)?;
        if !evidence_ids.insert(evidence.id.as_str())
            || evidence.contribution_micros.unsigned_abs() > MICROS as u32
        {
            return Err(invalid_metadata(
                evidence_path,
                "relationship evidence id or contribution is invalid",
            ));
        }
        validate_sorted_texts(
            &format!("{path}.evidence[{index}].input_paths"),
            &evidence.input_paths,
        )?;
        sha256(
            &format!("{path}.evidence[{index}].input_sha256"),
            &evidence.input_sha256,
        )?;
        text_value(
            &format!("{path}.evidence[{index}].explanation"),
            &evidence.explanation,
            1,
            2_048,
        )?;
    }
    validate_sorted_local_or_namespaced(&format!("{path}.lineage"), &edge.lineage)?;
    if let Some(rationale) = &edge.rationale {
        text_value(&format!("{path}.rationale"), rationale, 1, 2_048)?;
    }
    Ok(())
}

fn validate_relationship_diagnostics(
    path: &str,
    diagnostics: &[RelationshipDiagnostic],
) -> Result<(), RelationshipError> {
    if diagnostics.len() > 65_536 {
        return Err(invalid_metadata(path, "too many relationship diagnostics"));
    }
    let mut prior = None::<(&str, RelationshipDiagnosticCode, u8, &str)>;
    for (index, value) in diagnostics.iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        text_value(&format!("{item_path}.path"), &value.path, 1, 1_024)?;
        text_value(&format!("{item_path}.message"), &value.message, 1, 2_048)?;
        let severity = match value.severity {
            DiagnosticSeverity::Error => 0,
            DiagnosticSeverity::Warning => 1,
        };
        let key = (
            value.path.as_str(),
            value.code,
            severity,
            value.message.as_str(),
        );
        if prior.is_some_and(|prior| prior >= key) {
            return Err(invalid_metadata(
                path,
                "relationship diagnostics must be unique and deterministically sorted",
            ));
        }
        prior = Some(key);
    }
    Ok(())
}

fn validate_sorted_namespaced(
    path: &str,
    values: &[String],
    minimum: usize,
    maximum: usize,
) -> Result<(), RelationshipError> {
    if !(minimum..=maximum).contains(&values.len()) {
        return Err(invalid_metadata(
            path,
            "namespaced identifier collection is outside supported bounds",
        ));
    }
    let mut prior = None;
    for value in values {
        namespaced(path, value)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_metadata(
                path,
                "namespaced identifiers must be unique and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_sorted_local_or_namespaced(
    path: &str,
    values: &[String],
) -> Result<(), RelationshipError> {
    if values.len() > 4_096 {
        return Err(invalid_metadata(path, "too many lineage identifiers"));
    }
    let mut prior = None;
    for value in values {
        if validate_local_id(path, value).is_err() && validate_namespaced_id(path, value).is_err() {
            return Err(invalid_metadata(
                path,
                "lineage requires lowercase local or namespaced stable identifiers",
            ));
        }
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_metadata(
                path,
                "lineage identifiers must be unique and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_sorted_texts(path: &str, values: &[String]) -> Result<(), RelationshipError> {
    if values.len() > 4_096 {
        return Err(invalid_metadata(path, "too many ordered text values"));
    }
    let mut prior = None;
    for value in values {
        text_value(path, value, 1, 2_048)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_metadata(
                path,
                "text values must be unique and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_unique_sorted<T: Ord>(path: &str, values: &[T]) -> Result<(), RelationshipError> {
    if values.len() > 4_096 || values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid_metadata(
            path,
            "values must be unique, sorted, and within supported bounds",
        ));
    }
    Ok(())
}

fn collection_hash(collection: &CharacterCollection) -> Result<String, RelationshipError> {
    collection_fingerprint(collection).map_err(|failure| {
        relationship_error(
            RelationshipDiagnosticCode::InvalidMetadata,
            failure.diagnostic().path.clone(),
            "Character collection fingerprint could not be encoded",
        )
    })
}

fn canonical_hash<T: Serialize + ?Sized>(value: &T) -> Result<String, RelationshipError> {
    let bytes = serde_json::to_vec(value).map_err(|_| encoding_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn derived_document_id(base: &str, suffix: &str) -> String {
    let candidate = format!("{base}.{suffix}");
    if candidate.len() <= 256 {
        return candidate;
    }
    let digest = format!("{:x}", Sha256::digest(candidate.as_bytes()));
    format!("org.weave.relationship.{suffix}_{}", &digest[..32])
}

fn namespaced(path: &str, value: &str) -> Result<(), RelationshipError> {
    validate_namespaced_id(path, value).map_err(|failure| map_character_error(failure, path))
}

fn local(path: &str, value: &str) -> Result<(), RelationshipError> {
    validate_local_id(path, value).map_err(|failure| map_character_error(failure, path))
}

fn semver(path: &str, value: &str) -> Result<(), RelationshipError> {
    validate_semver(path, value).map_err(|failure| map_character_error(failure, path))
}

fn sha256(path: &str, value: &str) -> Result<(), RelationshipError> {
    validate_sha256(path, value).map_err(|failure| map_character_error(failure, path))
}

fn text_value(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), RelationshipError> {
    validate_text(path, value, minimum, maximum)
        .map_err(|failure| map_character_error(failure, path))
}

fn map_character_error(failure: crate::CharacterError, fallback_path: &str) -> RelationshipError {
    let diagnostic = failure.diagnostic();
    RelationshipError {
        diagnostic: RelationshipDiagnostic {
            code: RelationshipDiagnosticCode::InvalidMetadata,
            severity: diagnostic.severity,
            path: if diagnostic.path.is_empty() {
                fallback_path.to_owned()
            } else {
                diagnostic.path.clone()
            },
            message: diagnostic.message.clone(),
        },
    }
}

fn invalid_kind(path: impl Into<String>, message: &'static str) -> RelationshipError {
    relationship_error(RelationshipDiagnosticCode::InvalidKind, path, message)
}

fn invalid_metadata(path: impl Into<String>, message: &'static str) -> RelationshipError {
    relationship_error(RelationshipDiagnosticCode::InvalidMetadata, path, message)
}

fn stale(path: impl Into<String>, message: &'static str) -> RelationshipError {
    relationship_error(RelationshipDiagnosticCode::StaleEvidence, path, message)
}

fn encoding_error() -> RelationshipError {
    invalid_metadata(
        "document",
        "relationship document does not match the strict serialized contract",
    )
}

fn relationship_error(
    code: RelationshipDiagnosticCode,
    path: impl Into<String>,
    message: &'static str,
) -> RelationshipError {
    RelationshipError {
        diagnostic: diagnostic(code, path, message),
    }
}

fn diagnostic(
    code: RelationshipDiagnosticCode,
    path: impl Into<String>,
    message: &'static str,
) -> RelationshipDiagnostic {
    RelationshipDiagnostic {
        code,
        severity: DiagnosticSeverity::Error,
        path: path.into(),
        message: message.to_owned(),
    }
}

/// Stable public text for a relationship diagnostic code.
#[must_use]
pub const fn relationship_diagnostic_code(code: RelationshipDiagnosticCode) -> &'static str {
    match code {
        RelationshipDiagnosticCode::MissingCharacter => "R100",
        RelationshipDiagnosticCode::ForbiddenSelfEdge => "R101",
        RelationshipDiagnosticCode::BrokenInverse => "R102",
        RelationshipDiagnosticCode::InvalidDateRange => "R103",
        RelationshipDiagnosticCode::ContradictoryPedigree => "R104",
        RelationshipDiagnosticCode::StaleEvidence => "R105",
        RelationshipDiagnosticCode::DuplicateEdge => "R106",
        RelationshipDiagnosticCode::AgeSafeguard => "R107",
        RelationshipDiagnosticCode::KinshipSafeguard => "R108",
        RelationshipDiagnosticCode::PartnershipSafeguard => "R109",
        RelationshipDiagnosticCode::ConsentSafeguard => "R110",
        RelationshipDiagnosticCode::InvalidKind => "R111",
        RelationshipDiagnosticCode::InvalidMetadata => "R112",
        RelationshipDiagnosticCode::LockedEdge => "R113",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Calendar, CharacterCollection, RelationshipEdgeOrigin};
    use weave_domain::{ProvenanceSource, ProvenanceTransformation};

    const PROFILE: &str = include_str!(
        "../../../examples/domain-modules/weave-character/omitted-extensions.character.json"
    );
    const ARI: &str = "org.weave.character.ari";
    const SABLE: &str = "org.weave.character.sable";
    const TAVI: &str = "org.weave.character.tavi";

    fn provenance(id: &str, claim: &str) -> Provenance {
        Provenance {
            sources: vec![ProvenanceSource {
                id: id.to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave".to_owned(),
                revision: "relationship-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
                attribution: "Original synthetic relationship vocabulary and fixtures.".to_owned(),
                modified: false,
            }],
            transformations: Vec::new(),
            claims: BTreeMap::from([(claim.to_owned(), vec![id.to_owned()])]),
        }
    }

    fn definition(
        id: &str,
        family: RelationshipKindFamily,
        kinship_semantics: Option<RelationshipKinshipSemantics>,
        directionality: RelationshipDirectionality,
    ) -> RelationshipKindDefinition {
        RelationshipKindDefinition {
            id: id.to_owned(),
            label: id.rsplit('.').next().unwrap_or(id).replace('_', " "),
            description: "Original synthetic relationship kind for deterministic tests.".to_owned(),
            family,
            kinship_semantics,
            directionality,
            allows_self: false,
            allows_multiple_concurrent: false,
            required_metadata: Vec::new(),
            limitations: vec![
                "A relationship assertion is authored context, not objective interpersonal truth."
                    .to_owned(),
            ],
        }
    }

    fn pack() -> RelationshipKindPack {
        let parent = "org.weave.relationship.parent_of";
        let child = "org.weave.relationship.child_of";
        RelationshipKindPack {
            pack_format_version: RELATIONSHIP_KIND_PACK_FORMAT_VERSION,
            id: "org.weave.relationship.synthetic".to_owned(),
            version: "1.0.0".to_owned(),
            title: "Synthetic Relationship Kinds".to_owned(),
            description: "Original deterministic relationship semantics for public tests."
                .to_owned(),
            independently_authored: true,
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            kinds: BTreeMap::from([
                (
                    child.to_owned(),
                    definition(
                        child,
                        RelationshipKindFamily::Kinship,
                        Some(RelationshipKinshipSemantics::ChildOf),
                        RelationshipDirectionality::InversePaired {
                            inverse_kind_id: parent.to_owned(),
                        },
                    ),
                ),
                (
                    "org.weave.relationship.friend".to_owned(),
                    definition(
                        "org.weave.relationship.friend",
                        RelationshipKindFamily::Friendship,
                        None,
                        RelationshipDirectionality::Symmetric,
                    ),
                ),
                (
                    "org.weave.relationship.mentor".to_owned(),
                    definition(
                        "org.weave.relationship.mentor",
                        RelationshipKindFamily::Mentorship,
                        None,
                        RelationshipDirectionality::Directed,
                    ),
                ),
                (
                    parent.to_owned(),
                    definition(
                        parent,
                        RelationshipKindFamily::Kinship,
                        Some(RelationshipKinshipSemantics::ParentOf),
                        RelationshipDirectionality::InversePaired {
                            inverse_kind_id: child.to_owned(),
                        },
                    ),
                ),
                (
                    "org.weave.relationship.partner".to_owned(),
                    definition(
                        "org.weave.relationship.partner",
                        RelationshipKindFamily::Partnership,
                        None,
                        RelationshipDirectionality::Symmetric,
                    ),
                ),
                (
                    "org.weave.relationship.rival".to_owned(),
                    definition(
                        "org.weave.relationship.rival",
                        RelationshipKindFamily::Rivalry,
                        None,
                        RelationshipDirectionality::Directed,
                    ),
                ),
                (
                    "org.weave.relationship.shared_affinity".to_owned(),
                    RelationshipKindDefinition {
                        required_metadata: vec![RelationshipMetadataRequirement::Evidence],
                        ..definition(
                            "org.weave.relationship.shared_affinity",
                            RelationshipKindFamily::Affinity,
                            None,
                            RelationshipDirectionality::Symmetric,
                        )
                    },
                ),
                (
                    "org.weave.relationship.sibling".to_owned(),
                    definition(
                        "org.weave.relationship.sibling",
                        RelationshipKindFamily::Kinship,
                        Some(RelationshipKinshipSemantics::SiblingOf),
                        RelationshipDirectionality::Symmetric,
                    ),
                ),
            ]),
            provenance: provenance("weave_relationship_pack", "kinds"),
        }
    }

    fn profile(id: &str) -> CharacterProfile {
        let mut profile = CharacterProfile::from_json(PROFILE).unwrap();
        profile.id = id.to_owned();
        profile.extensions.remove(RELATIONSHIP_EXTENSION_NAMESPACE);
        profile
    }

    fn collection() -> CharacterCollection {
        let collection = CharacterCollection {
            collection_format_version: crate::CHARACTER_COLLECTION_FORMAT_VERSION,
            id: "org.weave.character.synthetic_roster".to_owned(),
            revision: 0,
            characters: [ARI, SABLE, TAVI]
                .map(|id| (id.to_owned(), profile(id)))
                .into(),
        };
        validate_character_collection(&collection).unwrap();
        collection
    }

    fn safeguards() -> RelationshipSafeguards {
        RelationshipSafeguards {
            minimum_partnership_age_years: None,
            forbid_close_kin_partnership: false,
            require_affirmed_partnership_consent: false,
            maximum_concurrent_partnerships: None,
            allow_reviewed_exceptions: false,
        }
    }

    fn edge(id: &str, source: &str, target: &str, kind: &str) -> RelationshipEdge {
        RelationshipEdge {
            id: id.to_owned(),
            source_character_id: source.to_owned(),
            target_character_id: target.to_owned(),
            kind: kind.to_owned(),
            confidence: Confidence::High,
            origin: RelationshipEdgeOrigin::Authored,
            review: ReviewState::NotRequired,
            lock: LockState::Unlocked,
            freshness: Freshness::Current,
            validity: None,
            inverse_edge_id: None,
            notes: BTreeMap::new(),
            consent: None,
            safeguard_exceptions: BTreeMap::new(),
            affinity_score_micros: None,
            evidence: Vec::new(),
            lineage: vec!["weave_relationship_revision".to_owned()],
            rationale: None,
        }
    }

    fn revision(
        collection: &CharacterCollection,
        pack: &RelationshipKindPack,
    ) -> RelationshipGraphRevision {
        let mut parent = edge(
            "parent_ari_sable",
            ARI,
            SABLE,
            "org.weave.relationship.parent_of",
        );
        parent.inverse_edge_id = Some("child_sable_ari".to_owned());
        let mut child = edge(
            "child_sable_ari",
            SABLE,
            ARI,
            "org.weave.relationship.child_of",
        );
        child.inverse_edge_id = Some("parent_ari_sable".to_owned());
        RelationshipGraphRevision {
            revision_format_version: RELATIONSHIP_REVISION_FORMAT_VERSION,
            id: "org.weave.relationship.synthetic_revision".to_owned(),
            expected_input_sha256: collection_hash(collection).unwrap(),
            kind_pack: relationship_kind_pack_ref(pack).unwrap(),
            reference_date: RelationshipDate {
                year: 2035,
                month: 6,
                day: 15,
            },
            safeguards: safeguards(),
            additions: vec![
                edge(
                    "friend_ari_tavi",
                    ARI,
                    TAVI,
                    "org.weave.relationship.friend",
                ),
                edge(
                    "mentor_ari_tavi",
                    ARI,
                    TAVI,
                    "org.weave.relationship.mentor",
                ),
                parent,
                child,
            ],
            removals: Vec::new(),
            rationale: "Author a synthetic directed, symmetric, and inverse-paired graph."
                .to_owned(),
            provenance: provenance("weave_relationship_revision", "relationships"),
        }
    }

    fn config(
        collection: &CharacterCollection,
        pack: &RelationshipKindPack,
        kind_id: &str,
        allow_exceptions: bool,
        require_consent: bool,
    ) -> RelationshipProposalConfig {
        RelationshipProposalConfig {
            config_format_version: RELATIONSHIP_CONFIG_FORMAT_VERSION,
            id: "org.weave.relationship.synthetic_config".to_owned(),
            expected_input_sha256: collection_hash(collection).unwrap(),
            kind_pack: relationship_kind_pack_ref(pack).unwrap(),
            reference_date: RelationshipDate {
                year: 2035,
                month: 6,
                day: 15,
            },
            seed: 73,
            roster: vec![ARI.to_owned(), SABLE.to_owned(), TAVI.to_owned()],
            targets: vec![RelationshipProposalTarget {
                id: "affinity_target".to_owned(),
                kind_id: kind_id.to_owned(),
                origin: RelationshipEdgeOrigin::ComputedAffinity,
                minimum_score_micros: 1,
                maximum_candidates: None,
                confidence: Confidence::Moderate,
                validity: Some(RelationshipValidityPeriod {
                    start: Some(RelationshipDate {
                        year: 2035,
                        month: 1,
                        day: 1,
                    }),
                    end: None,
                }),
                notes: BTreeMap::new(),
                rationale: "Offer an inspectable fictional relationship authoring prompt."
                    .to_owned(),
            }],
            evidence_rules: vec![RelationshipEvidenceRule::TraitSimilarity {
                id: "openness_similarity".to_owned(),
                trait_id: HexacoTrait::Openness,
                weight_micros: 1_000_000,
            }],
            consent_records: Vec::new(),
            safeguards: RelationshipSafeguards {
                require_affirmed_partnership_consent: require_consent,
                allow_reviewed_exceptions: allow_exceptions,
                ..safeguards()
            },
            provenance: provenance("weave_relationship_config", "configuration"),
        }
    }

    fn attach_edges(
        collection: &mut CharacterCollection,
        pack: &RelationshipKindPack,
        edges: Vec<RelationshipEdge>,
    ) {
        let reference = relationship_kind_pack_ref(pack).unwrap();
        for edge in edges {
            let profile = collection
                .characters
                .get_mut(&edge.source_character_id)
                .unwrap();
            let lineage = profile
                .provenance
                .sources
                .first()
                .map(|source| vec![source.id.clone()])
                .unwrap_or_default();
            let extension = profile
                .extensions
                .entry(RELATIONSHIP_EXTENSION_NAMESPACE.to_owned())
                .or_insert_with(|| {
                    CharacterExtension::Relationships(VersionedExtension {
                        header: ExtensionHeader {
                            namespace: RELATIONSHIP_EXTENSION_NAMESPACE.to_owned(),
                            extension_version: 1,
                            authority: "synthetic authored relationship graph".to_owned(),
                            rationale: "Exercise deterministic graph diagnostics.".to_owned(),
                            state: ValueState::Authored,
                            review: ReviewState::NotRequired,
                            lock: LockState::Unlocked,
                            freshness: Freshness::Current,
                            lineage,
                            canonical_personality_write_back: ExtensionWriteBack::Forbidden,
                        },
                        value: RelationshipEdges {
                            graph_format_version: 1,
                            kind_pack: Some(reference.clone()),
                            edges: BTreeMap::new(),
                        },
                    })
                });
            let CharacterExtension::Relationships(record) = extension else {
                unreachable!()
            };
            record.value.edges.insert(edge.id.clone(), edge);
        }
    }

    #[test]
    fn kind_pack_round_trips_and_rejects_broken_inverse_declarations() {
        let pack = pack();
        validate_relationship_kind_pack(&pack).unwrap();
        assert_eq!(
            RelationshipKindPack::from_json(&pack.to_json().unwrap()).unwrap(),
            pack
        );
        assert_eq!(
            RelationshipKindPack::from_ron(&pack.to_ron().unwrap()).unwrap(),
            pack
        );
        assert_eq!(relationship_kind_pack_fingerprint(&pack).unwrap().len(), 64);

        let mut broken = pack;
        broken
            .kinds
            .get_mut("org.weave.relationship.child_of")
            .unwrap()
            .directionality = RelationshipDirectionality::Directed;
        assert_eq!(
            validate_relationship_kind_pack(&broken)
                .unwrap_err()
                .diagnostic()
                .code,
            RelationshipDiagnosticCode::InvalidKind
        );
    }

    #[test]
    fn direct_revision_lists_directed_symmetric_and_inverse_paired_edges() {
        let input = collection();
        let pack = pack();
        let revision = revision(&input, &pack);
        let output = apply_relationship_graph_revision(&input, &pack, &revision).unwrap();
        assert_eq!(output.revision, 1);
        validate_relationship_graph(
            &output,
            &pack,
            revision.reference_date,
            &revision.safeguards,
        )
        .unwrap();

        let edges = list_relationships(
            &output,
            &pack,
            revision.reference_date,
            &revision.safeguards,
            &RelationshipFilter::default(),
        )
        .unwrap();
        assert_eq!(edges.len(), 4);
        assert_eq!(
            edges
                .iter()
                .filter(|view| matches!(view.directionality, RelationshipDirectionality::Symmetric))
                .count(),
            1
        );
        assert_eq!(
            CharacterCollection::from_json(&output.to_json().unwrap()).unwrap(),
            output
        );
        assert_eq!(
            CharacterCollection::from_ron(&output.to_ron().unwrap()).unwrap(),
            output
        );

        let filtered = list_relationships(
            &output,
            &pack,
            revision.reference_date,
            &revision.safeguards,
            &RelationshipFilter {
                kind_ids: vec!["org.weave.relationship.mentor".to_owned()],
                ..RelationshipFilter::default()
            },
        )
        .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].edge.id, "mentor_ari_tavi");
    }

    #[test]
    fn affinity_proposal_review_apply_and_exports_are_replayable() {
        let input = collection();
        let pack = pack();
        let config = config(
            &input,
            &pack,
            "org.weave.relationship.shared_affinity",
            false,
            false,
        );
        let proposal = propose_relationships(&input, &pack, &config).unwrap();
        assert_eq!(
            proposal,
            propose_relationships(&input, &pack, &config).unwrap()
        );
        assert_eq!(proposal.candidates.len(), 3);
        assert!(proposal.candidates.values().all(|candidate| {
            candidate.disposition == RelationshipCandidateDisposition::Proposed
                && candidate.score_micros == 1_000_000
                && candidate.evidence.len() == 1
        }));
        let json = proposal.to_json().unwrap();
        let ron = proposal.to_ron().unwrap();
        assert_eq!(RelationshipProposal::from_json(&json).unwrap(), proposal);
        assert_eq!(RelationshipProposal::from_ron(&ron).unwrap(), proposal);

        let decisions = proposal
            .candidates
            .keys()
            .enumerate()
            .map(|(index, id)| {
                let decision = match index {
                    0 => RelationshipReviewDecision::Accept {
                        lock: LockState::Locked,
                        rationale: Some("Keep this reviewed affinity prompt.".to_owned()),
                    },
                    1 => RelationshipReviewDecision::Reject {
                        rationale: "The prompt does not serve this draft.".to_owned(),
                    },
                    _ => RelationshipReviewDecision::Withhold {
                        rationale: "Hold this prompt for a later author pass.".to_owned(),
                    },
                };
                (id.clone(), decision)
            })
            .collect();
        let review = create_relationship_review(
            &proposal,
            decisions,
            "org.weave.reviewer.synthetic",
            "Review every deterministic affinity candidate.",
        )
        .unwrap();
        let receipt = apply_reviewed_relationships(&input, &proposal, &review).unwrap();
        validate_relationship_receipt(&receipt).unwrap();
        assert_eq!(
            RelationshipReceipt::from_json(&receipt.to_json().unwrap()).unwrap(),
            receipt
        );
        assert_eq!(
            RelationshipReceipt::from_ron(&receipt.to_ron().unwrap()).unwrap(),
            receipt
        );

        let accepted = list_relationships(
            &receipt.output_collection,
            &pack,
            config.reference_date,
            &config.safeguards,
            &RelationshipFilter::default(),
        )
        .unwrap();
        assert_eq!(accepted.len(), 1);
        assert_eq!(
            accepted[0].edge.origin,
            RelationshipEdgeOrigin::ComputedAffinity
        );
        assert_eq!(accepted[0].edge.review, ReviewState::Accepted);
        assert_eq!(accepted[0].edge.lock, LockState::Locked);

        let edge_csv = relationship_edge_review_csv(
            &receipt.output_collection,
            &pack,
            config.reference_date,
            &config.safeguards,
            &RelationshipFilter::default(),
        )
        .unwrap();
        let matrix_csv = relationship_dense_matrix_review_csv(
            &receipt.output_collection,
            &pack,
            config.reference_date,
            &config.safeguards,
            &RelationshipFilter::default(),
        )
        .unwrap();
        assert!(edge_csv.starts_with("review_only,"));
        assert!(edge_csv.contains(&accepted[0].edge.id));
        assert!(matrix_csv.starts_with("review_only,character_id,"));
        assert!(matrix_csv.contains(&accepted[0].edge.id));

        let mut changed = input.clone();
        changed.revision += 1;
        let before = changed.clone();
        let error = apply_reviewed_relationships(&changed, &proposal, &review).unwrap_err();
        assert_eq!(
            error.diagnostic().code,
            RelationshipDiagnosticCode::StaleEvidence
        );
        assert_eq!(changed, before);
    }

    #[test]
    fn author_exception_is_explicit_and_satisfies_consent_policy() {
        let input = collection();
        let pack = pack();
        let config = config(&input, &pack, "org.weave.relationship.partner", true, true);
        let proposal = propose_relationships(&input, &pack, &config).unwrap();
        assert!(proposal.candidates.values().all(|candidate| {
            candidate.disposition == RelationshipCandidateDisposition::SafeguardBlocked
                && candidate
                    .safeguards
                    .iter()
                    .any(|value| value.code == RelationshipDiagnosticCode::ConsentSafeguard)
        }));
        let selected = proposal.candidates.keys().next().unwrap().clone();
        let decisions = proposal
            .candidates
            .iter()
            .map(|(id, candidate)| {
                let decision = if id == &selected {
                    RelationshipReviewDecision::Exception {
                        edges: candidate.edges.clone(),
                        exception_codes: vec![RelationshipDiagnosticCode::ConsentSafeguard],
                        replacements: Vec::new(),
                        lock: LockState::Locked,
                        rationale: "The author explicitly records a fictional policy exception."
                            .to_owned(),
                    }
                } else {
                    RelationshipReviewDecision::Withhold {
                        rationale: "No exception is authorized for this candidate.".to_owned(),
                    }
                };
                (id.clone(), decision)
            })
            .collect();
        let review = create_relationship_review(
            &proposal,
            decisions,
            "org.weave.reviewer.synthetic",
            "Complete an explicit exception review.",
        )
        .unwrap();
        let receipt = apply_reviewed_relationships(&input, &proposal, &review).unwrap();
        let edges = list_relationships(
            &receipt.output_collection,
            &pack,
            config.reference_date,
            &config.safeguards,
            &RelationshipFilter::default(),
        )
        .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge.origin, RelationshipEdgeOrigin::Authored);
        assert_eq!(edges[0].edge.safeguard_exceptions.len(), 1);
        assert_eq!(
            edges[0]
                .edge
                .safeguard_exceptions
                .values()
                .next()
                .unwrap()
                .code,
            "R110"
        );
    }

    #[test]
    fn diagnostics_cover_conflicts_safety_staleness_and_reconciliation() {
        let mut collection = collection();
        let pack = pack();
        let mut self_edge = edge("self_friend", ARI, ARI, "org.weave.relationship.friend");
        self_edge.lineage.clear();
        let mut duplicate_one = edge(
            "friend_ari_sable_one",
            ARI,
            SABLE,
            "org.weave.relationship.friend",
        );
        duplicate_one.lineage.clear();
        let mut duplicate_two = duplicate_one.clone();
        duplicate_two.id = "friend_ari_sable_two".to_owned();
        let mut missing = edge(
            "rival_missing",
            ARI,
            "org.weave.character.missing",
            "org.weave.relationship.rival",
        );
        missing.lineage.clear();
        let mut stale_edge = edge("rival_ari_tavi", ARI, TAVI, "org.weave.relationship.rival");
        stale_edge.lineage.clear();
        stale_edge.freshness = Freshness::Stale;
        let mut broken_parent = edge(
            "parent_ari_tavi",
            ARI,
            TAVI,
            "org.weave.relationship.parent_of",
        );
        broken_parent.lineage.clear();
        broken_parent.inverse_edge_id = Some("child_tavi_ari_missing".to_owned());
        let mut sibling = edge(
            "sibling_ari_sable",
            ARI,
            SABLE,
            "org.weave.relationship.sibling",
        );
        sibling.lineage.clear();
        let mut partner_one = edge(
            "partner_ari_sable",
            ARI,
            SABLE,
            "org.weave.relationship.partner",
        );
        partner_one.lineage.clear();
        let mut partner_two = edge(
            "partner_ari_tavi",
            ARI,
            TAVI,
            "org.weave.relationship.partner",
        );
        partner_two.lineage.clear();

        let mut parent_ab = edge(
            "parent_cycle_ari_sable",
            ARI,
            SABLE,
            "org.weave.relationship.parent_of",
        );
        parent_ab.lineage.clear();
        parent_ab.inverse_edge_id = Some("child_cycle_sable_ari".to_owned());
        let mut child_ba = edge(
            "child_cycle_sable_ari",
            SABLE,
            ARI,
            "org.weave.relationship.child_of",
        );
        child_ba.lineage.clear();
        child_ba.inverse_edge_id = Some("parent_cycle_ari_sable".to_owned());
        let mut parent_ba = edge(
            "parent_cycle_sable_ari",
            SABLE,
            ARI,
            "org.weave.relationship.parent_of",
        );
        parent_ba.lineage.clear();
        parent_ba.inverse_edge_id = Some("child_cycle_ari_sable".to_owned());
        let mut child_ab = edge(
            "child_cycle_ari_sable",
            ARI,
            SABLE,
            "org.weave.relationship.child_of",
        );
        child_ab.lineage.clear();
        child_ab.inverse_edge_id = Some("parent_cycle_sable_ari".to_owned());

        attach_edges(
            &mut collection,
            &pack,
            vec![
                self_edge,
                duplicate_one,
                duplicate_two,
                missing,
                stale_edge,
                broken_parent,
                sibling,
                partner_one,
                partner_two,
                parent_ab,
                child_ba,
                parent_ba,
                child_ab,
            ],
        );
        collection
            .characters
            .get_mut(SABLE)
            .unwrap()
            .canon
            .birth_date = Some(crate::Attributed {
            value: BirthDate::Full {
                calendar: Calendar::ProlepticGregorian,
                year: 2028,
                month: 1,
                day: 1,
            },
            state: ValueState::Authored,
            confidence: Confidence::High,
            review: ReviewState::NotRequired,
            lock: LockState::Unlocked,
            freshness: Freshness::Current,
            lineage: collection.characters[SABLE]
                .provenance
                .sources
                .iter()
                .map(|source| source.id.clone())
                .collect(),
            rationale: None,
        });
        let safeguards = RelationshipSafeguards {
            minimum_partnership_age_years: Some(18),
            forbid_close_kin_partnership: true,
            require_affirmed_partnership_consent: true,
            maximum_concurrent_partnerships: Some(1),
            allow_reviewed_exceptions: true,
        };
        let date = RelationshipDate {
            year: 2035,
            month: 1,
            day: 1,
        };
        let diagnostics =
            relationship_graph_diagnostics(&collection, &pack, date, &safeguards).unwrap();
        let codes = diagnostics
            .iter()
            .map(|value| value.code)
            .collect::<BTreeSet<_>>();
        for expected in [
            RelationshipDiagnosticCode::MissingCharacter,
            RelationshipDiagnosticCode::ForbiddenSelfEdge,
            RelationshipDiagnosticCode::BrokenInverse,
            RelationshipDiagnosticCode::ContradictoryPedigree,
            RelationshipDiagnosticCode::StaleEvidence,
            RelationshipDiagnosticCode::DuplicateEdge,
            RelationshipDiagnosticCode::AgeSafeguard,
            RelationshipDiagnosticCode::KinshipSafeguard,
            RelationshipDiagnosticCode::PartnershipSafeguard,
            RelationshipDiagnosticCode::ConsentSafeguard,
        ] {
            assert!(
                codes.contains(&expected),
                "missing {}",
                relationship_diagnostic_code(expected)
            );
        }
        assert!(validate_relationship_graph(&collection, &pack, date, &safeguards).is_err());
        let report = reconcile_relationship_graph(&collection, &pack, date, &safeguards).unwrap();
        assert!(!report.repairs.is_empty());
        assert_eq!(
            RelationshipReconciliationReport::from_json(&report.to_json().unwrap()).unwrap(),
            report
        );
    }

    #[test]
    fn csv_formula_cells_are_neutralized_as_review_only_text() {
        assert_eq!(csv_cell("=1+1"), "'=1+1");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn relationship_application_provenance_is_sorted_and_reproducible() {
        let base = provenance("weave_relationship_config", "configuration");
        let (left, left_id) =
            relationship_application_provenance(&base, &"a".repeat(64), &"b".repeat(64)).unwrap();
        let (right, right_id) =
            relationship_application_provenance(&base, &"a".repeat(64), &"b".repeat(64)).unwrap();
        assert_eq!(left, right);
        assert_eq!(left_id, right_id);
        assert!(
            left.transformations
                .iter()
                .any(|transformation: &ProvenanceTransformation| transformation.id == left_id)
        );
    }

    #[test]
    fn derived_document_ids_remain_valid_for_maximum_length_inputs() {
        let base = format!("org.{}.{}", "a".repeat(128), "b".repeat(123));
        assert_eq!(base.len(), 256);
        namespaced("base", &base).unwrap();
        for suffix in ["proposal", "receipt", "relationship_reconciliation"] {
            let left = derived_document_id(&base, suffix);
            let right = derived_document_id(&base, suffix);
            assert_eq!(left, right);
            namespaced("derived", &left).unwrap();
        }
    }
}
