//! Explainable, pack-driven categorical views and role suggestions.
//!
//! Projection packs are immutable data. They rank declared candidates with fixed-point arithmetic,
//! expose every present and missing input, and allocate collection-wide capacity deterministically.
//! Results remain review-gated and can only update the role-projection extension.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    Provenance, ProvenanceTransformation, parse_strict_json, to_pretty_json, to_pretty_ron,
    validate_provenance,
};

use crate::model::{
    CHARACTER_PROFILE_FORMAT_VERSION, CharacterDiagnosticCode, CharacterExtension,
    CharacterProfile, ExtensionHeader, ExtensionWriteBack, Freshness, HexacoTrait, LockState,
    ProjectionKind, ProjectionPackRef, ProjectionPublicDecision, ReviewState, RoleProjection,
    RoleProjections, TraitMeasurement, ValueState, VersionedExtension,
    role_projection_format_version,
};
use crate::operations::{
    CharacterCollection, collection_fingerprint as corpus_collection_fingerprint,
    validate_character_collection as validate_corpus_collection,
};
use crate::synthesis::merge_provenance;
use crate::validation::{
    CharacterError, error, invalid_value, trait_path, trait_value, validate_local_id,
    validate_namespaced_id, validate_profile, validate_semver, validate_sha256, validate_text,
};

pub const PROJECTION_PACK_FORMAT_VERSION: u32 = 1;
pub const PROJECTION_CONFIG_FORMAT_VERSION: u32 = 1;
pub const PROJECTION_PROPOSAL_FORMAT_VERSION: u32 = 1;
pub const PROJECTION_REVIEW_FORMAT_VERSION: u32 = 1;
pub const PROJECTION_RECEIPT_FORMAT_VERSION: u32 = 1;
pub const PROJECTION_LOCK_REVISION_FORMAT_VERSION: u32 = 1;

pub const ROLE_PROJECTION_EXTENSION_NAMESPACE: &str = "org.weave.character.role_projections";

const SCORE_SCALE: i64 = 1_000_000;
const MAX_INPUTS: usize = 4_096;
const MAX_TAXONOMIES: usize = 4_096;
const MAX_ENTRIES: usize = 16_384;
const MAX_CHARACTERS: usize = 65_536;

const PACK_SCHEMA_ID: &str = "urn:weave:schema:character-projection-pack:1";
const CONFIG_SCHEMA_ID: &str = "urn:weave:schema:character-projection-config:1";
const PROPOSAL_SCHEMA_ID: &str = "urn:weave:schema:character-projection-proposal:1";
const REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-projection-review:1";
const RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-projection-receipt:1";
const LOCK_SCHEMA_ID: &str = "urn:weave:schema:character-projection-lock-revision:1";

/// One independently distributable, data-only projection system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionPack {
    pub pack_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub compatible_profile_versions: Vec<u32>,
    pub independently_authored: bool,
    pub license: String,
    pub license_url: String,
    pub methodology: String,
    pub limitations: String,
    pub inputs: BTreeMap<String, ProjectionInputField>,
    pub taxonomies: BTreeMap<String, ProjectionTaxonomy>,
    pub calibrations: BTreeMap<String, ProjectionCalibrationFixture>,
    pub provenance: Provenance,
}

/// One declared canonical input. V1 deliberately permits only HEXACO factors and facets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionInputField {
    pub id: String,
    pub trait_id: HexacoTrait,
    pub profile_path: String,
    pub label: String,
    pub description: String,
}

/// One categorical display or suggestion taxonomy with a single output slot per character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionTaxonomy {
    pub id: String,
    pub output_id: String,
    pub kind: ProjectionKind,
    pub label: String,
    pub description: String,
    pub lossy: bool,
    pub independent_evidence: bool,
    pub requires_review: bool,
    pub minimum_coverage_micros: u32,
    pub limitations: String,
    pub entries: BTreeMap<String, ProjectionEntry>,
}

/// One rankable label with signed fixed-point evidence and explicit eligibility/capacity rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionEntry {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Input weights in non-zero signed thousandths.
    pub evidence: BTreeMap<String, i16>,
    /// Inclusive minimum signed score required for eligibility.
    pub minimum_score_micros: i32,
    /// Exact ids eligible for this entry. Eligibility is the union with id prefixes.
    pub eligible_character_ids: Vec<String>,
    pub eligible_id_prefixes: Vec<String>,
    pub excluded_character_ids: Vec<String>,
    /// Default collection-wide capacity. A project may replace it explicitly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_capacity: Option<u32>,
}

/// Synthetic calibration proving the exact fixed-point ranking for a complete input vector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCalibrationFixture {
    pub id: String,
    pub description: String,
    pub inputs_micros: BTreeMap<String, u32>,
    /// Complete ranked entries for every taxonomy, highest score first.
    pub expected_rankings: BTreeMap<String, Vec<ProjectionCalibrationExpected>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCalibrationExpected {
    pub entry_id: String,
    pub score_micros: i32,
    pub qualifies: bool,
}

/// Project-specific selection, eligible pool, capacity, reservation, and rebalance policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionConfig {
    pub config_format_version: u32,
    pub id: String,
    pub selected_taxonomies: Vec<String>,
    pub eligible_character_ids: Vec<String>,
    pub minimum_coverage_micros: u32,
    pub mode: ProjectionAssignmentMode,
    /// Taxonomy id, then entry id, to an explicit positive capacity.
    pub capacity_overrides: BTreeMap<String, BTreeMap<String, u32>>,
    pub reservations: Vec<ProjectionReservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionAssignmentMode {
    FillMissing,
    Rebalance,
}

/// One exact reserved assignment, applied before ordinary ranked capacity allocation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReservation {
    pub character_id: String,
    pub taxonomy_id: String,
    pub entry_id: String,
}

/// One ordered input contribution. Missing inputs are retained rather than imputed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionEvidenceTrace {
    pub input_id: String,
    pub profile_path: String,
    pub trait_id: HexacoTrait,
    pub weight_thousandths: i16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_micros: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centered_micros: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_contribution: Option<i64>,
    pub lineage: Vec<String>,
}

/// Complete evidence, threshold, eligibility, capacity, and seed trace for one entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCandidateTrace {
    pub entry_id: String,
    pub ordered_evidence: Vec<ProjectionEvidenceTrace>,
    pub total_weight_thousandths: u32,
    pub covered_weight_thousandths: u32,
    pub coverage_micros: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_sum: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_micros: Option<i32>,
    pub threshold_micros: i32,
    pub explicitly_eligible: bool,
    pub explicitly_excluded: bool,
    pub qualified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
    pub used_before: u32,
    pub capacity_available: bool,
    pub seeded_sha256: String,
    pub selected: bool,
    pub explanation: String,
}

/// Whether one character/taxonomy value was preserved, proposed, reserved, or unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionAssignmentDisposition {
    Retained,
    Reserved,
    Proposed,
    Unavailable,
}

/// One immutable character/taxonomy allocation proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionProposedAssignment {
    pub character_id: String,
    pub taxonomy_id: String,
    pub output_id: String,
    pub disposition: ProjectionAssignmentDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior: Option<RoleProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_entry_id: Option<String>,
    pub rationale: String,
    pub candidates: Vec<ProjectionCandidateTrace>,
}

/// Exact counts and unavailable slots for a proposal or reviewed output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionDistribution {
    pub counts: BTreeMap<String, BTreeMap<String, u32>>,
    pub retained: BTreeMap<String, Vec<String>>,
    pub reserved: BTreeMap<String, Vec<String>>,
    pub unavailable: BTreeMap<String, Vec<String>>,
}

/// Immutable proposal embedding every input required for independent replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionProposal {
    pub proposal_format_version: u32,
    pub id: String,
    pub input_collection: CharacterCollection,
    pub input_sha256: String,
    pub pack: ProjectionPack,
    pub pack_ref: ProjectionPackRef,
    pub config: ProjectionConfig,
    pub config_sha256: String,
    pub seed: u64,
    /// Character id, then taxonomy id, both canonically sorted by their maps.
    pub assignments: BTreeMap<String, BTreeMap<String, ProjectionProposedAssignment>>,
    /// Exact decision slots required from a complete review.
    pub review_manifest: BTreeMap<String, Vec<String>>,
    pub distribution: ProjectionDistribution,
}

/// Complete decision map for every proposed or reserved character/taxonomy pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReview {
    pub review_format_version: u32,
    pub proposal_sha256: String,
    pub reviewer: String,
    pub rationale: String,
    pub decisions: BTreeMap<String, BTreeMap<String, ProjectionReviewDecision>>,
}

/// Explicit editorial disposition of one projected value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "decision", deny_unknown_fields)]
pub enum ProjectionReviewDecision {
    Accept {
        lock: LockState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    Edit {
        entry_id: String,
        lock: LockState,
        rationale: String,
    },
    Override {
        entry_id: String,
        label: String,
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

/// Full proof of proposal replay, editorial review, and atomic collection output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReceipt {
    pub receipt_format_version: u32,
    pub id: String,
    pub proposal: ProjectionProposal,
    pub review: ProjectionReview,
    pub proposal_sha256: String,
    pub review_sha256: String,
    pub output_collection: CharacterCollection,
    pub output_sha256: String,
    pub distribution: ProjectionDistribution,
}

/// Portable deterministic lock/unlock request for accepted projection values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionLockRevision {
    pub revision_format_version: u32,
    pub id: String,
    pub expected_input_sha256: String,
    pub targets: Vec<ProjectionLockTarget>,
    pub lock: LockState,
    pub rationale: String,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionLockTarget {
    pub character_id: String,
    pub projection_id: String,
}

#[derive(Serialize)]
struct ProjectionSeedFingerprint<'a> {
    pack: &'a ProjectionPackRef,
    config_sha256: &'a str,
    character_id: &'a str,
    taxonomy_id: &'a str,
    entry_id: &'a str,
    seed: u64,
    ordered_evidence: &'a [ProjectionEvidenceTrace],
}

struct ProjectionEvaluationContext<'a> {
    profile: &'a CharacterProfile,
    pack: &'a ProjectionPack,
    pack_ref: &'a ProjectionPackRef,
    config_sha256: &'a str,
    config: &'a ProjectionConfig,
    seed: u64,
}

struct ReviewedProjectionContext<'a> {
    proposal: &'a ProjectionProposal,
    review: &'a ProjectionReview,
    proposal_sha256: &'a str,
    review_sha256: &'a str,
}

macro_rules! impl_projection_document {
    ($type:ty, $validate:ident) => {
        impl $type {
            pub fn from_json(source: &str) -> Result<Self, CharacterError> {
                let value = parse_strict_json(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn from_ron(source: &str) -> Result<Self, CharacterError> {
                let value = ron::from_str(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn to_json(&self) -> Result<String, CharacterError> {
                $validate(self)?;
                to_pretty_json(self).map_err(|_| encoding_error())
            }

            pub fn to_ron(&self) -> Result<String, CharacterError> {
                $validate(self)?;
                to_pretty_ron(self).map_err(|_| encoding_error())
            }
        }
    };
}

impl_projection_document!(ProjectionPack, validate_projection_pack);
impl_projection_document!(ProjectionConfig, validate_projection_config_structure);
impl_projection_document!(ProjectionProposal, validate_projection_proposal);
impl_projection_document!(ProjectionReview, validate_projection_review_structure);
impl_projection_document!(ProjectionReceipt, validate_projection_receipt);
impl_projection_document!(
    ProjectionLockRevision,
    validate_projection_lock_revision_structure
);

pub fn projection_pack_schema() -> Result<String, CharacterError> {
    projection_schema::<ProjectionPack>(PACK_SCHEMA_ID, "Weave Character Projection Pack v1")
}

pub fn projection_config_schema() -> Result<String, CharacterError> {
    projection_schema::<ProjectionConfig>(
        CONFIG_SCHEMA_ID,
        "Weave Character Projection Configuration v1",
    )
}

pub fn projection_proposal_schema() -> Result<String, CharacterError> {
    projection_schema::<ProjectionProposal>(
        PROPOSAL_SCHEMA_ID,
        "Weave Character Projection Proposal v1",
    )
}

pub fn projection_review_schema() -> Result<String, CharacterError> {
    projection_schema::<ProjectionReview>(REVIEW_SCHEMA_ID, "Weave Character Projection Review v1")
}

pub fn projection_receipt_schema() -> Result<String, CharacterError> {
    projection_schema::<ProjectionReceipt>(
        RECEIPT_SCHEMA_ID,
        "Weave Character Projection Receipt v1",
    )
}

pub fn projection_lock_revision_schema() -> Result<String, CharacterError> {
    projection_schema::<ProjectionLockRevision>(
        LOCK_SCHEMA_ID,
        "Weave Character Projection Lock Revision v1",
    )
}

pub fn projection_pack_fingerprint(pack: &ProjectionPack) -> Result<String, CharacterError> {
    validate_projection_pack(pack)?;
    canonical_hash(pack)
}

pub fn projection_config_fingerprint(config: &ProjectionConfig) -> Result<String, CharacterError> {
    validate_projection_config_structure(config)?;
    canonical_hash(config)
}

pub fn projection_proposal_fingerprint(
    proposal: &ProjectionProposal,
) -> Result<String, CharacterError> {
    validate_projection_proposal(proposal)?;
    canonical_hash(proposal)
}

pub fn projection_review_fingerprint(review: &ProjectionReview) -> Result<String, CharacterError> {
    validate_projection_review_structure(review)?;
    canonical_hash(review)
}

pub fn validate_projection_pack(pack: &ProjectionPack) -> Result<(), CharacterError> {
    if pack.pack_format_version != PROJECTION_PACK_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "pack_format_version",
            "unsupported projection pack version",
        ));
    }
    validate_namespaced_id("id", &pack.id)?;
    validate_semver("version", &pack.version)?;
    validate_public_text("title", &pack.title, 1, 256)?;
    validate_public_text("description", &pack.description, 1, 2_048)?;
    if pack.compatible_profile_versions.is_empty()
        || !strictly_sorted_unique(&pack.compatible_profile_versions)
        || !pack
            .compatible_profile_versions
            .contains(&CHARACTER_PROFILE_FORMAT_VERSION)
    {
        return Err(invalid_value(
            "compatible_profile_versions",
            "projection pack compatibility versions must be sorted, unique, and include the current profile version",
        ));
    }
    if !pack.independently_authored {
        return Err(invalid_value(
            "independently_authored",
            "projection pack must declare independently authored content",
        ));
    }
    validate_public_text("license", &pack.license, 1, 128)?;
    validate_https("license_url", &pack.license_url)?;
    validate_public_text("methodology", &pack.methodology, 1, 8_192)?;
    validate_public_text("limitations", &pack.limitations, 1, 8_192)?;
    validate_provenance(&pack.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "projection pack provenance is malformed or incomplete",
        )
    })?;
    if pack.provenance.sources.is_empty() {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance.sources",
            "projection pack requires explicit public provenance",
        ));
    }
    if pack.inputs.is_empty() || pack.inputs.len() > MAX_INPUTS {
        return Err(invalid_value(
            "inputs",
            "projection pack input count is invalid",
        ));
    }
    for (id, input) in &pack.inputs {
        let path = format!("inputs.{id}");
        validate_local_id(&path, id)?;
        if input.id != *id || input.profile_path != trait_path(input.trait_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "projection input identity or canonical path is inconsistent",
            ));
        }
        validate_public_text(&format!("inputs.{id}.label"), &input.label, 1, 256)?;
        validate_public_text(
            &format!("inputs.{id}.description"),
            &input.description,
            1,
            2_048,
        )?;
    }
    if pack.taxonomies.is_empty() || pack.taxonomies.len() > MAX_TAXONOMIES {
        return Err(invalid_value(
            "taxonomies",
            "projection pack taxonomy count is invalid",
        ));
    }
    let mut output_ids = BTreeSet::new();
    let mut total_entries = 0_usize;
    for (taxonomy_id, taxonomy) in &pack.taxonomies {
        validate_taxonomy(pack, taxonomy_id, taxonomy)?;
        if !output_ids.insert(taxonomy.output_id.as_str()) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("taxonomies.{taxonomy_id}.output_id"),
                "projection taxonomy output identifiers must be unique within a pack",
            ));
        }
        total_entries += taxonomy.entries.len();
    }
    if total_entries > MAX_ENTRIES {
        return Err(invalid_value(
            "taxonomies.entries",
            "projection pack contains too many entries",
        ));
    }
    if pack.calibrations.is_empty() || pack.calibrations.len() > MAX_ENTRIES {
        return Err(invalid_value(
            "calibrations",
            "projection pack calibration count is invalid",
        ));
    }
    for (id, fixture) in &pack.calibrations {
        validate_calibration(pack, id, fixture)?;
    }
    Ok(())
}

fn validate_taxonomy(
    pack: &ProjectionPack,
    taxonomy_id: &str,
    taxonomy: &ProjectionTaxonomy,
) -> Result<(), CharacterError> {
    let path = format!("taxonomies.{taxonomy_id}");
    validate_namespaced_id(&path, taxonomy_id)?;
    if taxonomy.id != taxonomy_id {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.id"),
            "taxonomy identifier must equal its containing map key",
        ));
    }
    validate_local_id(&format!("{path}.output_id"), &taxonomy.output_id)?;
    validate_public_text(&format!("{path}.label"), &taxonomy.label, 1, 256)?;
    validate_public_text(
        &format!("{path}.description"),
        &taxonomy.description,
        1,
        2_048,
    )?;
    validate_public_text(
        &format!("{path}.limitations"),
        &taxonomy.limitations,
        1,
        4_096,
    )?;
    validate_micros(
        &format!("{path}.minimum_coverage_micros"),
        taxonomy.minimum_coverage_micros,
    )?;
    if taxonomy.independent_evidence || !taxonomy.requires_review {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            path,
            "projection taxonomies must remain review-gated and cannot claim independent evidence",
        ));
    }
    if matches!(taxonomy.kind, ProjectionKind::CategoricalPersonality) != taxonomy.lossy {
        return Err(invalid_value(
            format!("{path}.lossy"),
            "only categorical personality views must be labeled lossy",
        ));
    }
    if taxonomy.entries.is_empty() || taxonomy.entries.len() > MAX_ENTRIES {
        return Err(invalid_value(
            format!("{path}.entries"),
            "projection taxonomy entry count is invalid",
        ));
    }
    for (entry_id, entry) in &taxonomy.entries {
        let entry_path = format!("{path}.entries.{entry_id}");
        validate_local_id(&entry_path, entry_id)?;
        if entry.id != *entry_id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{entry_path}.id"),
                "projection entry identifier must equal its containing map key",
            ));
        }
        validate_public_text(&format!("{entry_path}.label"), &entry.label, 1, 256)?;
        validate_public_text(
            &format!("{entry_path}.description"),
            &entry.description,
            1,
            2_048,
        )?;
        validate_signed_micros(
            &format!("{entry_path}.minimum_score_micros"),
            entry.minimum_score_micros,
        )?;
        if entry.evidence.is_empty() || entry.evidence.len() > MAX_INPUTS {
            return Err(invalid_value(
                format!("{entry_path}.evidence"),
                "projection entry evidence count is invalid",
            ));
        }
        for (input_id, weight) in &entry.evidence {
            if !pack.inputs.contains_key(input_id) {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{entry_path}.evidence.{input_id}"),
                    "projection entry references an undeclared input",
                ));
            }
            if *weight == 0 {
                return Err(invalid_value(
                    format!("{entry_path}.evidence.{input_id}"),
                    "projection evidence weights must be non-zero",
                ));
            }
        }
        validate_sorted_namespaced_ids(
            &format!("{entry_path}.eligible_character_ids"),
            &entry.eligible_character_ids,
        )?;
        validate_sorted_namespaced_ids(
            &format!("{entry_path}.excluded_character_ids"),
            &entry.excluded_character_ids,
        )?;
        validate_sorted_prefixes(
            &format!("{entry_path}.eligible_id_prefixes"),
            &entry.eligible_id_prefixes,
        )?;
        if entry.eligible_character_ids.is_empty() && entry.eligible_id_prefixes.is_empty() {
            return Err(invalid_value(
                format!("{entry_path}.eligible_character_ids"),
                "projection entry must declare an explicit eligible id or prefix",
            ));
        }
        if entry.default_capacity == Some(0) {
            return Err(invalid_value(
                format!("{entry_path}.default_capacity"),
                "projection capacity must be positive when present",
            ));
        }
    }
    Ok(())
}

fn validate_calibration(
    pack: &ProjectionPack,
    id: &str,
    fixture: &ProjectionCalibrationFixture,
) -> Result<(), CharacterError> {
    let path = format!("calibrations.{id}");
    validate_local_id(&path, id)?;
    if fixture.id != id {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.id"),
            "calibration identifier must equal its containing map key",
        ));
    }
    validate_public_text(
        &format!("{path}.description"),
        &fixture.description,
        1,
        2_048,
    )?;
    if fixture.inputs_micros.len() != pack.inputs.len()
        || fixture.inputs_micros.keys().ne(pack.inputs.keys())
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.inputs_micros"),
            "calibration must provide every declared projection input exactly once",
        ));
    }
    for (input_id, value) in &fixture.inputs_micros {
        validate_micros(&format!("{path}.inputs_micros.{input_id}"), *value)?;
    }
    if fixture.expected_rankings.keys().ne(pack.taxonomies.keys()) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.expected_rankings"),
            "calibration must cover every declared taxonomy exactly once",
        ));
    }
    for (taxonomy_id, expected) in &fixture.expected_rankings {
        let taxonomy = &pack.taxonomies[taxonomy_id];
        let actual = calibration_ranking(taxonomy, &fixture.inputs_micros)?;
        if expected != &actual {
            return Err(error(
                CharacterDiagnosticCode::InvalidValue,
                format!("{path}.expected_rankings.{taxonomy_id}"),
                "projection calibration does not match the declared fixed-point method",
            ));
        }
    }
    Ok(())
}

fn calibration_ranking(
    taxonomy: &ProjectionTaxonomy,
    inputs: &BTreeMap<String, u32>,
) -> Result<Vec<ProjectionCalibrationExpected>, CharacterError> {
    let mut values = taxonomy
        .entries
        .values()
        .map(|entry| {
            let score = evaluate_complete_entry(entry, inputs)?;
            Ok(ProjectionCalibrationExpected {
                entry_id: entry.id.clone(),
                score_micros: score,
                qualifies: score >= entry.minimum_score_micros,
            })
        })
        .collect::<Result<Vec<_>, CharacterError>>()?;
    values.sort_by(|left, right| {
        right
            .score_micros
            .cmp(&left.score_micros)
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    Ok(values)
}

/// Compute exact calibration rankings while authoring a pack.
///
/// This helper performs no mutation and deliberately accepts a pack whose calibration map is not
/// populated yet. Full pack validation still requires checked expected rankings.
pub fn projection_calibration_rankings(
    pack: &ProjectionPack,
    inputs_micros: &BTreeMap<String, u32>,
) -> Result<BTreeMap<String, Vec<ProjectionCalibrationExpected>>, CharacterError> {
    if inputs_micros.len() != pack.inputs.len() || inputs_micros.keys().ne(pack.inputs.keys()) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "inputs_micros",
            "projection calibration input vector must cover every declared input exactly",
        ));
    }
    for (input_id, value) in inputs_micros {
        validate_micros(&format!("inputs_micros.{input_id}"), *value)?;
    }
    pack.taxonomies
        .iter()
        .map(|(taxonomy_id, taxonomy)| {
            Ok((
                taxonomy_id.clone(),
                calibration_ranking(taxonomy, inputs_micros)?,
            ))
        })
        .collect()
}

pub fn validate_projection_config(
    config: &ProjectionConfig,
    pack: &ProjectionPack,
    collection: &CharacterCollection,
) -> Result<(), CharacterError> {
    validate_projection_config_structure(config)?;
    validate_projection_pack(pack)?;
    validate_character_collection(collection)?;
    for taxonomy_id in &config.selected_taxonomies {
        if !pack.taxonomies.contains_key(taxonomy_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "selected_taxonomies",
                "projection configuration selects a taxonomy absent from the exact pack",
            ));
        }
    }
    for character_id in &config.eligible_character_ids {
        if !collection.characters.contains_key(character_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "eligible_character_ids",
                "projection configuration references a character absent from the collection",
            ));
        }
    }
    for (taxonomy_id, entries) in &config.capacity_overrides {
        let Some(taxonomy) = pack.taxonomies.get(taxonomy_id) else {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "capacity_overrides",
                "projection capacity override references an unknown taxonomy",
            ));
        };
        for entry_id in entries.keys() {
            if !taxonomy.entries.contains_key(entry_id) {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    "capacity_overrides",
                    "projection capacity override references an unknown entry",
                ));
            }
        }
    }
    for reservation in &config.reservations {
        if !config
            .eligible_character_ids
            .contains(&reservation.character_id)
            || !config
                .selected_taxonomies
                .contains(&reservation.taxonomy_id)
            || !pack.taxonomies[&reservation.taxonomy_id]
                .entries
                .contains_key(&reservation.entry_id)
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "reservations",
                "projection reservation is outside the selected eligible pool",
            ));
        }
    }
    Ok(())
}

pub fn validate_projection_config_structure(
    config: &ProjectionConfig,
) -> Result<(), CharacterError> {
    if config.config_format_version != PROJECTION_CONFIG_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "config_format_version",
            "unsupported projection configuration version",
        ));
    }
    validate_namespaced_id("id", &config.id)?;
    if config.selected_taxonomies.is_empty() || !strictly_sorted_unique(&config.selected_taxonomies)
    {
        return Err(invalid_value(
            "selected_taxonomies",
            "selected projection taxonomies must be non-empty, sorted, and unique",
        ));
    }
    for id in &config.selected_taxonomies {
        validate_namespaced_id("selected_taxonomies", id)?;
    }
    if config.eligible_character_ids.is_empty()
        || config.eligible_character_ids.len() > MAX_CHARACTERS
        || !strictly_sorted_unique(&config.eligible_character_ids)
    {
        return Err(invalid_value(
            "eligible_character_ids",
            "eligible character ids must be non-empty, sorted, unique, and bounded",
        ));
    }
    for id in &config.eligible_character_ids {
        validate_namespaced_id("eligible_character_ids", id)?;
    }
    validate_micros("minimum_coverage_micros", config.minimum_coverage_micros)?;
    for (taxonomy_id, entries) in &config.capacity_overrides {
        validate_namespaced_id("capacity_overrides.taxonomy_id", taxonomy_id)?;
        if entries.is_empty() {
            return Err(invalid_value(
                "capacity_overrides",
                "capacity override maps cannot be empty",
            ));
        }
        for (entry_id, capacity) in entries {
            validate_local_id("capacity_overrides.entry_id", entry_id)?;
            if *capacity == 0 {
                return Err(invalid_value(
                    "capacity_overrides.capacity",
                    "projection capacity overrides must be positive",
                ));
            }
        }
    }
    if !strictly_sorted_unique(&config.reservations) {
        return Err(invalid_value(
            "reservations",
            "projection reservations must be sorted and unique",
        ));
    }
    let mut slots = BTreeSet::new();
    for reservation in &config.reservations {
        validate_namespaced_id("reservations.character_id", &reservation.character_id)?;
        validate_namespaced_id("reservations.taxonomy_id", &reservation.taxonomy_id)?;
        validate_local_id("reservations.entry_id", &reservation.entry_id)?;
        if !slots.insert((&reservation.character_id, &reservation.taxonomy_id)) {
            return Err(invalid_value(
                "reservations",
                "one character/taxonomy slot cannot have multiple reservations",
            ));
        }
    }
    Ok(())
}

/// Produce an immutable, capacity-aware proposal for an exact collection, pack, config, and seed.
pub fn propose_projections(
    collection: &CharacterCollection,
    pack: &ProjectionPack,
    config: &ProjectionConfig,
    seed: u64,
) -> Result<ProjectionProposal, CharacterError> {
    build_projection_proposal(collection, pack, config, seed)
}

fn build_projection_proposal(
    collection: &CharacterCollection,
    pack: &ProjectionPack,
    config: &ProjectionConfig,
    seed: u64,
) -> Result<ProjectionProposal, CharacterError> {
    validate_projection_config(config, pack, collection)?;
    let input_sha256 = collection_fingerprint(collection)?;
    let pack_ref = ProjectionPackRef {
        id: pack.id.clone(),
        version: pack.version.clone(),
        sha256: projection_pack_fingerprint(pack)?,
    };
    let config_sha256 = projection_config_fingerprint(config)?;
    let mut assignments = BTreeMap::<String, BTreeMap<String, ProjectionProposedAssignment>>::new();
    let mut distribution = ProjectionDistribution::default();
    let mut usage = BTreeMap::<(String, String), u32>::new();
    let mut open_slots = BTreeSet::<(String, String)>::new();

    for character_id in &config.eligible_character_ids {
        let profile = &collection.characters[character_id];
        let mut character_assignments = BTreeMap::new();
        for taxonomy_id in &config.selected_taxonomies {
            let taxonomy = &pack.taxonomies[taxonomy_id];
            let prior = existing_projection(profile, taxonomy_id).cloned();
            let retain = prior.as_ref().is_some_and(|value| {
                matches!(config.mode, ProjectionAssignmentMode::FillMissing)
                    || value.lock == LockState::Locked
                    || matches!(
                        value.decision,
                        ProjectionPublicDecision::Authored | ProjectionPublicDecision::Overridden
                    )
                    || role_extension_locked(profile)
            });
            if retain {
                let value = prior.as_ref().expect("retained slot has a prior value");
                if value.pack.as_ref() == Some(&pack_ref) {
                    *usage
                        .entry((taxonomy_id.clone(), value.entry_id.clone()))
                        .or_default() += 1;
                    distribution
                        .counts
                        .entry(taxonomy_id.clone())
                        .or_default()
                        .entry(value.entry_id.clone())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                distribution
                    .retained
                    .entry(character_id.clone())
                    .or_default()
                    .push(taxonomy_id.clone());
                character_assignments.insert(
                    taxonomy_id.clone(),
                    ProjectionProposedAssignment {
                        character_id: character_id.clone(),
                        taxonomy_id: taxonomy_id.clone(),
                        output_id: taxonomy.output_id.clone(),
                        disposition: ProjectionAssignmentDisposition::Retained,
                        prior,
                        proposed_entry_id: None,
                        rationale: "Preserved an authored, overridden, locked, or fill-missing value before allocation.".to_owned(),
                        candidates: Vec::new(),
                    },
                );
            } else {
                open_slots.insert((character_id.clone(), taxonomy_id.clone()));
                let evaluation = ProjectionEvaluationContext {
                    profile,
                    pack,
                    pack_ref: &pack_ref,
                    config_sha256: &config_sha256,
                    config,
                    seed,
                };
                let candidates = taxonomy
                    .entries
                    .values()
                    .map(|entry| evaluate_candidate(&evaluation, taxonomy, entry))
                    .collect::<Result<Vec<_>, CharacterError>>()?;
                character_assignments.insert(
                    taxonomy_id.clone(),
                    ProjectionProposedAssignment {
                        character_id: character_id.clone(),
                        taxonomy_id: taxonomy_id.clone(),
                        output_id: taxonomy.output_id.clone(),
                        disposition: ProjectionAssignmentDisposition::Unavailable,
                        prior,
                        proposed_entry_id: None,
                        rationale: "No capacity-qualified projection entry has been allocated yet."
                            .to_owned(),
                        candidates,
                    },
                );
            }
        }
        assignments.insert(character_id.clone(), character_assignments);
    }

    for reservation in &config.reservations {
        let slot = (
            reservation.character_id.clone(),
            reservation.taxonomy_id.clone(),
        );
        if !open_slots.contains(&slot) {
            continue;
        }
        let assignment = assignments
            .get_mut(&reservation.character_id)
            .and_then(|values| values.get_mut(&reservation.taxonomy_id))
            .expect("validated reservation belongs to a selected slot");
        let candidate = assignment
            .candidates
            .iter()
            .find(|candidate| candidate.entry_id == reservation.entry_id)
            .expect("validated reservation references one declared entry");
        if !candidate.qualified {
            return Err(error(
                CharacterDiagnosticCode::InvalidValue,
                "reservations",
                "reserved projection entry does not satisfy evidence and eligibility requirements",
            ));
        }
        let capacity = effective_capacity(
            pack,
            config,
            &reservation.taxonomy_id,
            &reservation.entry_id,
        );
        let used = usage
            .get(&(
                reservation.taxonomy_id.clone(),
                reservation.entry_id.clone(),
            ))
            .copied()
            .unwrap_or(0);
        if capacity.is_some_and(|limit| used >= limit) {
            return Err(error(
                CharacterDiagnosticCode::InvalidValue,
                "reservations",
                "reserved projection entry exceeds its effective capacity",
            ));
        }
        select_assignment(
            assignment,
            &reservation.entry_id,
            ProjectionAssignmentDisposition::Reserved,
            used,
            capacity,
        );
        *usage
            .entry((
                reservation.taxonomy_id.clone(),
                reservation.entry_id.clone(),
            ))
            .or_default() += 1;
        distribution
            .reserved
            .entry(reservation.character_id.clone())
            .or_default()
            .push(reservation.taxonomy_id.clone());
        open_slots.remove(&slot);
    }

    let mut candidates = Vec::<(i32, String, String, String, String)>::new();
    for (character_id, taxonomy_id) in &open_slots {
        for candidate in &assignments[character_id][taxonomy_id].candidates {
            if candidate.qualified {
                candidates.push((
                    candidate
                        .score_micros
                        .expect("qualified candidate has a score"),
                    candidate.seeded_sha256.clone(),
                    character_id.clone(),
                    taxonomy_id.clone(),
                    candidate.entry_id.clone(),
                ));
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(&right.3))
            .then_with(|| left.4.cmp(&right.4))
    });
    for (_, _, character_id, taxonomy_id, entry_id) in candidates {
        let slot = (character_id.clone(), taxonomy_id.clone());
        if !open_slots.contains(&slot) {
            continue;
        }
        let capacity = effective_capacity(pack, config, &taxonomy_id, &entry_id);
        let used = usage
            .get(&(taxonomy_id.clone(), entry_id.clone()))
            .copied()
            .unwrap_or(0);
        let assignment = assignments
            .get_mut(&character_id)
            .and_then(|values| values.get_mut(&taxonomy_id))
            .expect("open slot belongs to a validated assignment");
        if let Some(trace) = assignment
            .candidates
            .iter_mut()
            .find(|candidate| candidate.entry_id == entry_id)
        {
            trace.used_before = used;
            trace.capacity_available = capacity.is_none_or(|limit| used < limit);
        }
        if capacity.is_some_and(|limit| used >= limit) {
            continue;
        }
        select_assignment(
            assignment,
            &entry_id,
            ProjectionAssignmentDisposition::Proposed,
            used,
            capacity,
        );
        *usage
            .entry((taxonomy_id.clone(), entry_id.clone()))
            .or_default() += 1;
        open_slots.remove(&slot);
    }

    let mut review_manifest = BTreeMap::<String, Vec<String>>::new();
    for (character_id, taxonomy_id) in open_slots {
        let assignment = assignments
            .get_mut(&character_id)
            .and_then(|values| values.get_mut(&taxonomy_id))
            .expect("open slot belongs to a validated assignment");
        assignment.rationale =
            "No eligible entry met coverage, threshold, reservation, and remaining capacity rules."
                .to_owned();
        distribution
            .unavailable
            .entry(character_id)
            .or_default()
            .push(taxonomy_id);
    }
    for (character_id, values) in &assignments {
        for (taxonomy_id, assignment) in values {
            if matches!(
                assignment.disposition,
                ProjectionAssignmentDisposition::Proposed
                    | ProjectionAssignmentDisposition::Reserved
            ) {
                review_manifest
                    .entry(character_id.clone())
                    .or_default()
                    .push(taxonomy_id.clone());
                let entry_id = assignment
                    .proposed_entry_id
                    .as_ref()
                    .expect("reviewable assignment has an entry");
                *distribution
                    .counts
                    .entry(taxonomy_id.clone())
                    .or_default()
                    .entry(entry_id.clone())
                    .or_default() += 1;
            }
        }
    }
    let proposal = ProjectionProposal {
        proposal_format_version: PROJECTION_PROPOSAL_FORMAT_VERSION,
        id: format!("{}.proposal", config.id),
        input_collection: collection.clone(),
        input_sha256,
        pack: pack.clone(),
        pack_ref,
        config: config.clone(),
        config_sha256,
        seed,
        assignments,
        review_manifest,
        distribution,
    };
    validate_projection_proposal_structure(&proposal)?;
    Ok(proposal)
}

fn evaluate_candidate(
    context: &ProjectionEvaluationContext<'_>,
    taxonomy: &ProjectionTaxonomy,
    entry: &ProjectionEntry,
) -> Result<ProjectionCandidateTrace, CharacterError> {
    let total_weight = entry
        .evidence
        .values()
        .map(|weight| u32::from(weight.unsigned_abs()))
        .sum::<u32>();
    let mut covered_weight = 0_u32;
    let mut weighted_sum = 0_i64;
    let mut ordered_evidence = Vec::with_capacity(entry.evidence.len());
    for (input_id, weight) in &entry.evidence {
        let input = &context.pack.inputs[input_id];
        let measurement = trait_value(&context.profile.canon.personality, input.trait_id);
        let (profile_micros, centered_micros, contribution, lineage) = measurement
            .map(|attributed| {
                let value = measurement_micros(attributed.value);
                let centered = i32::try_from(i64::from(value) * 2 - SCORE_SCALE)
                    .expect("unit interval centering fits signed millionths");
                let contribution = i64::from(centered) * i64::from(*weight);
                (value, centered, contribution, attributed.lineage.clone())
            })
            .map_or((None, None, None, Vec::new()), |value| {
                covered_weight += u32::from(weight.unsigned_abs());
                weighted_sum += value.2;
                (Some(value.0), Some(value.1), Some(value.2), value.3)
            });
        ordered_evidence.push(ProjectionEvidenceTrace {
            input_id: input_id.clone(),
            profile_path: input.profile_path.clone(),
            trait_id: input.trait_id,
            weight_thousandths: *weight,
            profile_micros,
            centered_micros,
            weighted_contribution: contribution,
            lineage,
        });
    }
    let coverage_micros = u32::try_from(rounded_div(
        i128::from(covered_weight) * i128::from(SCORE_SCALE),
        i128::from(total_weight),
    ))
    .expect("coverage ratio stays within unsigned millionths");
    let score_micros = (covered_weight > 0).then(|| {
        i32::try_from(rounded_div(
            i128::from(weighted_sum),
            i128::from(covered_weight),
        ))
        .expect("weighted projection score stays within signed millionths")
    });
    let explicitly_excluded = entry.excluded_character_ids.contains(&context.profile.id);
    let explicitly_eligible = entry.eligible_character_ids.contains(&context.profile.id)
        || entry.eligible_id_prefixes.iter().any(|prefix| {
            context.profile.id == *prefix || context.profile.id.starts_with(&format!("{prefix}."))
        });
    let required_coverage = context
        .config
        .minimum_coverage_micros
        .max(taxonomy.minimum_coverage_micros);
    let qualified = explicitly_eligible
        && !explicitly_excluded
        && coverage_micros >= required_coverage
        && score_micros.is_some_and(|score| score >= entry.minimum_score_micros);
    let seeded_sha256 = canonical_hash(&ProjectionSeedFingerprint {
        pack: context.pack_ref,
        config_sha256: context.config_sha256,
        character_id: &context.profile.id,
        taxonomy_id: &taxonomy.id,
        entry_id: &entry.id,
        seed: context.seed,
        ordered_evidence: &ordered_evidence,
    })?;
    let capacity = effective_capacity(context.pack, context.config, &taxonomy.id, &entry.id);
    Ok(ProjectionCandidateTrace {
        entry_id: entry.id.clone(),
        ordered_evidence,
        total_weight_thousandths: total_weight,
        covered_weight_thousandths: covered_weight,
        coverage_micros,
        weighted_sum: (covered_weight > 0).then_some(weighted_sum),
        score_micros,
        threshold_micros: entry.minimum_score_micros,
        explicitly_eligible,
        explicitly_excluded,
        qualified,
        capacity,
        used_before: 0,
        capacity_available: capacity.is_none_or(|limit| limit > 0),
        seeded_sha256,
        selected: false,
        explanation: format!(
            "Candidate `{}` used {} of {} absolute weight thousandths ({} millionths coverage). Present canonical inputs were centered around 0.5, multiplied by declared signed weights, and divided by covered weight with integer arithmetic. Missing inputs contributed neither a value nor fabricated precision.",
            entry.id, covered_weight, total_weight, coverage_micros
        ),
    })
}

fn select_assignment(
    assignment: &mut ProjectionProposedAssignment,
    entry_id: &str,
    disposition: ProjectionAssignmentDisposition,
    used_before: u32,
    capacity: Option<u32>,
) {
    assignment.disposition = disposition;
    assignment.proposed_entry_id = Some(entry_id.to_owned());
    assignment.rationale = match disposition {
        ProjectionAssignmentDisposition::Reserved => {
            "Selected the explicit eligible reservation before ordinary capacity allocation."
                .to_owned()
        }
        ProjectionAssignmentDisposition::Proposed => "Selected the highest-scoring eligible entry; a pinned seed hash and stable identifiers break ties deterministically.".to_owned(),
        _ => unreachable!("selection only produces reserved or proposed assignments"),
    };
    for candidate in &mut assignment.candidates {
        if candidate.entry_id == entry_id {
            candidate.selected = true;
            candidate.used_before = used_before;
            candidate.capacity = capacity;
            candidate.capacity_available = capacity.is_none_or(|limit| used_before < limit);
        }
    }
}

fn effective_capacity(
    pack: &ProjectionPack,
    config: &ProjectionConfig,
    taxonomy_id: &str,
    entry_id: &str,
) -> Option<u32> {
    config
        .capacity_overrides
        .get(taxonomy_id)
        .and_then(|entries| entries.get(entry_id))
        .copied()
        .or(pack.taxonomies[taxonomy_id].entries[entry_id].default_capacity)
}

fn existing_projection<'a>(
    profile: &'a CharacterProfile,
    taxonomy_id: &str,
) -> Option<&'a RoleProjection> {
    let CharacterExtension::RoleProjections(record) = profile
        .extensions
        .get(ROLE_PROJECTION_EXTENSION_NAMESPACE)?
    else {
        return None;
    };
    record
        .value
        .roles
        .values()
        .find(|value| value.taxonomy == taxonomy_id)
}

fn role_extension_locked(profile: &CharacterProfile) -> bool {
    matches!(
        profile.extensions.get(ROLE_PROJECTION_EXTENSION_NAMESPACE),
        Some(CharacterExtension::RoleProjections(record)) if record.header.lock == LockState::Locked
    )
}

pub fn validate_projection_proposal(proposal: &ProjectionProposal) -> Result<(), CharacterError> {
    validate_projection_proposal_structure(proposal)?;
    let expected = build_projection_proposal(
        &proposal.input_collection,
        &proposal.pack,
        &proposal.config,
        proposal.seed,
    )?;
    if &expected != proposal {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal",
            "projection proposal does not reproduce from its pinned inputs",
        ));
    }
    Ok(())
}

fn validate_projection_proposal_structure(
    proposal: &ProjectionProposal,
) -> Result<(), CharacterError> {
    if proposal.proposal_format_version != PROJECTION_PROPOSAL_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "proposal_format_version",
            "unsupported projection proposal version",
        ));
    }
    validate_namespaced_id("id", &proposal.id)?;
    validate_character_collection(&proposal.input_collection)?;
    validate_sha256("input_sha256", &proposal.input_sha256)?;
    validate_projection_pack(&proposal.pack)?;
    validate_projection_pack_ref("pack_ref", &proposal.pack_ref)?;
    validate_projection_config(&proposal.config, &proposal.pack, &proposal.input_collection)?;
    validate_sha256("config_sha256", &proposal.config_sha256)?;
    if proposal
        .assignments
        .keys()
        .ne(proposal.config.eligible_character_ids.iter())
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "assignments",
            "projection assignments must cover the eligible pool exactly",
        ));
    }
    for (character_id, values) in &proposal.assignments {
        if values.keys().ne(proposal.config.selected_taxonomies.iter()) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("assignments.{character_id}"),
                "projection assignments must cover selected taxonomies exactly",
            ));
        }
        for (taxonomy_id, assignment) in values {
            if assignment.character_id != *character_id
                || assignment.taxonomy_id != *taxonomy_id
                || assignment.output_id != proposal.pack.taxonomies[taxonomy_id].output_id
            {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("assignments.{character_id}.{taxonomy_id}"),
                    "projection assignment identity is inconsistent",
                ));
            }
            validate_public_text(
                &format!("assignments.{character_id}.{taxonomy_id}.rationale"),
                &assignment.rationale,
                1,
                2_048,
            )?;
            for candidate in &assignment.candidates {
                validate_candidate_trace(candidate)?;
            }
        }
    }
    for (character_id, taxonomies) in &proposal.review_manifest {
        validate_namespaced_id("review_manifest.character_id", character_id)?;
        if taxonomies.is_empty() || !strictly_sorted_unique(taxonomies) {
            return Err(invalid_value(
                "review_manifest",
                "projection review manifest entries must be non-empty, sorted, and unique",
            ));
        }
    }
    Ok(())
}

fn validate_candidate_trace(candidate: &ProjectionCandidateTrace) -> Result<(), CharacterError> {
    validate_local_id("candidate.entry_id", &candidate.entry_id)?;
    validate_micros("candidate.coverage_micros", candidate.coverage_micros)?;
    validate_signed_micros("candidate.threshold_micros", candidate.threshold_micros)?;
    if let Some(score) = candidate.score_micros {
        validate_signed_micros("candidate.score_micros", score)?;
    }
    if candidate.capacity == Some(0) {
        return Err(invalid_value(
            "candidate.capacity",
            "projection candidate capacity must be positive when present",
        ));
    }
    validate_sha256("candidate.seeded_sha256", &candidate.seeded_sha256)?;
    validate_public_text("candidate.explanation", &candidate.explanation, 1, 4_096)
}

/// Create a complete review after an author has inspected the immutable proposal traces.
pub fn create_projection_review(
    proposal: &ProjectionProposal,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
    decisions: BTreeMap<String, BTreeMap<String, ProjectionReviewDecision>>,
) -> Result<ProjectionReview, CharacterError> {
    validate_projection_proposal(proposal)?;
    let review = ProjectionReview {
        review_format_version: PROJECTION_REVIEW_FORMAT_VERSION,
        proposal_sha256: projection_proposal_fingerprint(proposal)?,
        reviewer: reviewer.into(),
        rationale: rationale.into(),
        decisions,
    };
    validate_projection_review(&review, proposal)?;
    Ok(review)
}

pub fn validate_projection_review(
    review: &ProjectionReview,
    proposal: &ProjectionProposal,
) -> Result<(), CharacterError> {
    validate_projection_review_structure(review)?;
    if review.proposal_sha256 != projection_proposal_fingerprint(proposal)? {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal_sha256",
            "projection review does not target the exact proposal",
        ));
    }
    if review.decisions.len() != proposal.review_manifest.len() {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "decisions",
            "projection review must cover every manifest character exactly once",
        ));
    }
    for (character_id, taxonomies) in &proposal.review_manifest {
        let Some(decisions) = review.decisions.get(character_id) else {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "decisions",
                "projection review is missing a manifest character",
            ));
        };
        if decisions.keys().ne(taxonomies.iter()) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("decisions.{character_id}"),
                "projection review must cover every manifest taxonomy exactly once",
            ));
        }
        for (taxonomy_id, decision) in decisions {
            let assignment = &proposal.assignments[character_id][taxonomy_id];
            validate_review_decision(decision, assignment)?;
        }
    }
    Ok(())
}

pub fn validate_projection_review_structure(
    review: &ProjectionReview,
) -> Result<(), CharacterError> {
    if review.review_format_version != PROJECTION_REVIEW_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "review_format_version",
            "unsupported projection review version",
        ));
    }
    validate_sha256("proposal_sha256", &review.proposal_sha256)?;
    validate_public_text("reviewer", &review.reviewer, 1, 512)?;
    validate_public_text("rationale", &review.rationale, 1, 2_048)?;
    for (character_id, decisions) in &review.decisions {
        validate_namespaced_id("decisions.character_id", character_id)?;
        if decisions.is_empty() {
            return Err(invalid_value(
                format!("decisions.{character_id}"),
                "projection decision maps cannot be empty",
            ));
        }
        for (taxonomy_id, decision) in decisions {
            validate_namespaced_id("decisions.taxonomy_id", taxonomy_id)?;
            match decision {
                ProjectionReviewDecision::Accept { rationale, .. } => {
                    if let Some(rationale) = rationale {
                        validate_public_text("decisions.accept.rationale", rationale, 1, 2_048)?;
                    }
                }
                ProjectionReviewDecision::Edit {
                    entry_id,
                    rationale,
                    ..
                } => {
                    validate_local_id("decisions.edit.entry_id", entry_id)?;
                    validate_public_text("decisions.edit.rationale", rationale, 1, 2_048)?;
                }
                ProjectionReviewDecision::Override {
                    entry_id,
                    label,
                    rationale,
                    ..
                } => {
                    validate_local_id("decisions.override.entry_id", entry_id)?;
                    validate_public_text("decisions.override.label", label, 1, 256)?;
                    validate_public_text("decisions.override.rationale", rationale, 1, 2_048)?;
                }
                ProjectionReviewDecision::Reject { rationale }
                | ProjectionReviewDecision::Withhold { rationale } => {
                    validate_public_text("decisions.rationale", rationale, 1, 2_048)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_review_decision(
    decision: &ProjectionReviewDecision,
    assignment: &ProjectionProposedAssignment,
) -> Result<(), CharacterError> {
    match decision {
        ProjectionReviewDecision::Accept { .. }
        | ProjectionReviewDecision::Reject { .. }
        | ProjectionReviewDecision::Withhold { .. } => Ok(()),
        ProjectionReviewDecision::Edit { entry_id, .. } => {
            if !assignment
                .candidates
                .iter()
                .any(|candidate| candidate.entry_id == *entry_id && candidate.qualified)
            {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    "decisions.edit.entry_id",
                    "edited projection must select an eligible qualified pack entry",
                ));
            }
            Ok(())
        }
        ProjectionReviewDecision::Override { .. } => Ok(()),
    }
}

/// Apply a complete review atomically and return an independently reproducible receipt.
pub fn apply_projection_review(
    current_collection: &CharacterCollection,
    proposal: &ProjectionProposal,
    review: &ProjectionReview,
) -> Result<ProjectionReceipt, CharacterError> {
    build_projection_receipt(current_collection, proposal, review)
}

fn build_projection_receipt(
    current_collection: &CharacterCollection,
    proposal: &ProjectionProposal,
    review: &ProjectionReview,
) -> Result<ProjectionReceipt, CharacterError> {
    validate_character_collection(current_collection)?;
    validate_projection_proposal(proposal)?;
    validate_projection_review(review, proposal)?;
    if collection_fingerprint(current_collection)? != proposal.input_sha256
        || *current_collection != proposal.input_collection
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "current_collection",
            "projection proposal does not target the current collection",
        ));
    }
    let proposal_sha256 = projection_proposal_fingerprint(proposal)?;
    let review_sha256 = projection_review_fingerprint(review)?;
    let reviewed_context = ReviewedProjectionContext {
        proposal,
        review,
        proposal_sha256: &proposal_sha256,
        review_sha256: &review_sha256,
    };
    let mut output = current_collection.clone();
    for character_id in &proposal.config.eligible_character_ids {
        let before = &current_collection.characters[character_id];
        let mut after = before.clone();
        let mut roles = existing_role_data(before);
        let mut changed = false;
        if let Some(decisions) = review.decisions.get(character_id) {
            for (taxonomy_id, decision) in decisions {
                let assignment = &proposal.assignments[character_id][taxonomy_id];
                let existing_key = roles
                    .roles
                    .iter()
                    .find_map(|(id, value)| (value.taxonomy == *taxonomy_id).then(|| id.clone()));
                match decision {
                    ProjectionReviewDecision::Reject { .. }
                    | ProjectionReviewDecision::Withhold { .. } => {
                        if let Some(existing_key) = existing_key {
                            roles.roles.remove(&existing_key);
                            changed = true;
                        }
                    }
                    _ => {
                        let value = reviewed_projection_value(
                            &reviewed_context,
                            character_id,
                            taxonomy_id,
                            assignment,
                            decision,
                        )?;
                        if let Some(existing_key) = existing_key
                            && existing_key != value.id
                        {
                            roles.roles.remove(&existing_key);
                        }
                        changed |= roles.roles.get(&value.id) != Some(&value);
                        roles.roles.insert(value.id.clone(), value);
                    }
                }
            }
        }
        if !changed {
            continue;
        }
        roles.character_id = character_id.clone();
        roles.source_pack_refs = roles
            .roles
            .values()
            .filter_map(|value| value.pack.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        after.provenance = merge_provenance(&after.provenance, &proposal.pack.provenance)?;
        let transformation_id =
            projection_transformation_id(character_id, &proposal_sha256, &review_sha256, &roles)?;
        let mut lineage = provenance_ids(&proposal.pack.provenance);
        lineage.extend(profile_projection_input_lineage(
            before,
            proposal,
            character_id,
        ));
        if lineage.is_empty() {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance",
                "projection application requires explicit public lineage",
            ));
        }
        after
            .provenance
            .transformations
            .push(ProvenanceTransformation {
                id: transformation_id.clone(),
                inputs: lineage.iter().cloned().collect(),
                description: "Applied a complete editorial review to deterministic categorical and role projections without mutating character canon or another extension family."
                    .to_owned(),
            });
        after
            .provenance
            .transformations
            .sort_by(|left, right| left.id.cmp(&right.id));
        after
            .provenance
            .claims
            .entry(format!("extensions.{ROLE_PROJECTION_EXTENSION_NAMESPACE}"))
            .or_default()
            .push(transformation_id.clone());
        sort_deduplicate_claims(&mut after.provenance);
        lineage.insert(transformation_id);
        if roles.roles.is_empty() {
            after.extensions.remove(ROLE_PROJECTION_EXTENSION_NAMESPACE);
        } else {
            let state = if roles
                .roles
                .values()
                .any(|value| matches!(value.decision, ProjectionPublicDecision::Overridden))
            {
                ValueState::Overridden
            } else {
                ValueState::Reviewed
            };
            after.extensions.insert(
                ROLE_PROJECTION_EXTENSION_NAMESPACE.to_owned(),
                CharacterExtension::RoleProjections(VersionedExtension {
                    header: ExtensionHeader {
                        namespace: ROLE_PROJECTION_EXTENSION_NAMESPACE.to_owned(),
                        extension_version: 1,
                        authority: review.reviewer.clone(),
                        rationale: review.rationale.clone(),
                        state,
                        review: ReviewState::Accepted,
                        lock: LockState::Unlocked,
                        freshness: Freshness::Current,
                        lineage: lineage.into_iter().collect(),
                        canonical_personality_write_back: ExtensionWriteBack::Forbidden,
                    },
                    value: roles,
                }),
            );
        }
        assert_projection_only(before, &after)?;
        validate_profile(&after)?;
        output.characters.insert(character_id.clone(), after);
    }
    if output.characters != current_collection.characters {
        output.revision = output.revision.checked_add(1).ok_or_else(|| {
            invalid_value(
                "output_collection.revision",
                "collection revision overflowed",
            )
        })?;
    }
    validate_character_collection(&output)?;
    validate_reviewed_capacities(current_collection, &output, proposal)?;
    let distribution = collection_projection_distribution(&output, &proposal.pack_ref);
    let output_sha256 = collection_fingerprint(&output)?;
    let receipt = ProjectionReceipt {
        receipt_format_version: PROJECTION_RECEIPT_FORMAT_VERSION,
        id: format!("{}.receipt", proposal.config.id),
        proposal: proposal.clone(),
        review: review.clone(),
        proposal_sha256,
        review_sha256,
        output_collection: output,
        output_sha256,
        distribution,
    };
    validate_projection_receipt_structure(&receipt)?;
    Ok(receipt)
}

fn reviewed_projection_value(
    context: &ReviewedProjectionContext<'_>,
    character_id: &str,
    taxonomy_id: &str,
    assignment: &ProjectionProposedAssignment,
    decision: &ProjectionReviewDecision,
) -> Result<RoleProjection, CharacterError> {
    let taxonomy = &context.proposal.pack.taxonomies[taxonomy_id];
    let (
        entry_id,
        label,
        public_decision,
        score,
        coverage,
        explanation,
        input_paths,
        lock,
        rationale,
    ) = match decision {
        ProjectionReviewDecision::Accept { lock, rationale } => {
            let entry_id = assignment
                .proposed_entry_id
                .as_ref()
                .expect("reviewable assignment has a proposed entry");
            let entry = &taxonomy.entries[entry_id];
            let trace = candidate_trace(assignment, entry_id);
            (
                entry_id.clone(),
                entry.label.clone(),
                if matches!(taxonomy.kind, ProjectionKind::CategoricalPersonality) {
                    ProjectionPublicDecision::Derived
                } else {
                    ProjectionPublicDecision::Reviewed
                },
                trace.score_micros,
                trace.coverage_micros,
                format!("{} {}", taxonomy.description, entry.description),
                candidate_input_paths(trace),
                *lock,
                rationale
                    .clone()
                    .unwrap_or_else(|| context.review.rationale.clone()),
            )
        }
        ProjectionReviewDecision::Edit {
            entry_id,
            lock,
            rationale,
        } => {
            let entry = &taxonomy.entries[entry_id];
            let trace = candidate_trace(assignment, entry_id);
            (
                entry_id.clone(),
                entry.label.clone(),
                if matches!(taxonomy.kind, ProjectionKind::CategoricalPersonality) {
                    ProjectionPublicDecision::Derived
                } else {
                    ProjectionPublicDecision::Edited
                },
                trace.score_micros,
                trace.coverage_micros,
                format!("{} {}", taxonomy.description, entry.description),
                candidate_input_paths(trace),
                *lock,
                rationale.clone(),
            )
        }
        ProjectionReviewDecision::Override {
            entry_id,
            label,
            lock,
            rationale,
        } => (
            entry_id.clone(),
            label.clone(),
            ProjectionPublicDecision::Overridden,
            None,
            0,
            "Editorial override retained without claiming a pack score or independent evidence."
                .to_owned(),
            Vec::new(),
            *lock,
            rationale.clone(),
        ),
        ProjectionReviewDecision::Reject { .. } | ProjectionReviewDecision::Withhold { .. } => {
            return Err(error(
                CharacterDiagnosticCode::InvalidValue,
                "decision",
                "non-publishing decisions do not produce a projection value",
            ));
        }
    };
    Ok(RoleProjection {
        id: taxonomy.output_id.clone(),
        character_id: character_id.to_owned(),
        kind: taxonomy.kind,
        taxonomy: taxonomy_id.to_owned(),
        entry_id,
        label,
        lossy: taxonomy.lossy,
        independent_evidence: false,
        decision: public_decision,
        score_micros: score,
        coverage_micros: coverage,
        explanation,
        rationale,
        input_paths,
        pack: Some(context.proposal.pack_ref.clone()),
        proposal_sha256: Some(context.proposal_sha256.to_owned()),
        review_sha256: Some(context.review_sha256.to_owned()),
        lock,
    })
}

fn candidate_trace<'a>(
    assignment: &'a ProjectionProposedAssignment,
    entry_id: &str,
) -> &'a ProjectionCandidateTrace {
    assignment
        .candidates
        .iter()
        .find(|candidate| candidate.entry_id == entry_id)
        .expect("review validation guarantees an eligible candidate")
}

fn candidate_input_paths(trace: &ProjectionCandidateTrace) -> Vec<String> {
    let mut paths = trace
        .ordered_evidence
        .iter()
        .filter(|input| input.profile_micros.is_some())
        .map(|input| input.profile_path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn existing_role_data(profile: &CharacterProfile) -> RoleProjections {
    match profile.extensions.get(ROLE_PROJECTION_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::RoleProjections(record)) => record.value.clone(),
        _ => RoleProjections {
            projection_format_version: role_projection_format_version(),
            character_id: profile.id.clone(),
            roles: BTreeMap::new(),
            source_pack_refs: Vec::new(),
        },
    }
}

fn projection_transformation_id(
    character_id: &str,
    proposal_sha256: &str,
    review_sha256: &str,
    roles: &RoleProjections,
) -> Result<String, CharacterError> {
    #[derive(Serialize)]
    struct Fingerprint<'a> {
        character_id: &'a str,
        proposal_sha256: &'a str,
        review_sha256: &'a str,
        roles: &'a RoleProjections,
    }
    let hash = canonical_hash(&Fingerprint {
        character_id,
        proposal_sha256,
        review_sha256,
        roles,
    })?;
    Ok(format!("projection_apply_{}", &hash[..16]))
}

fn profile_projection_input_lineage(
    profile: &CharacterProfile,
    proposal: &ProjectionProposal,
    character_id: &str,
) -> BTreeSet<String> {
    proposal.assignments[character_id]
        .values()
        .flat_map(|assignment| &assignment.candidates)
        .flat_map(|candidate| &candidate.ordered_evidence)
        .filter(|evidence| evidence.profile_micros.is_some())
        .flat_map(|evidence| evidence.lineage.iter().cloned())
        .filter(|id| {
            profile
                .provenance
                .sources
                .iter()
                .any(|source| &source.id == id)
                || profile
                    .provenance
                    .transformations
                    .iter()
                    .any(|transformation| &transformation.id == id)
        })
        .collect()
}

fn assert_projection_only(
    before: &CharacterProfile,
    after: &CharacterProfile,
) -> Result<(), CharacterError> {
    if before.id != after.id
        || before.profile_format_version != after.profile_format_version
        || before.canon != after.canon
        || before.derived != after.derived
        || before.suggestions != after.suggestions
        || before.extensions.iter().any(|(namespace, value)| {
            namespace != ROLE_PROJECTION_EXTENSION_NAMESPACE
                && after.extensions.get(namespace) != Some(value)
        })
        || after.extensions.iter().any(|(namespace, value)| {
            namespace != ROLE_PROJECTION_EXTENSION_NAMESPACE
                && before.extensions.get(namespace) != Some(value)
        })
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "output_collection",
            "projection review attempted to mutate canon or another extension family",
        ));
    }
    Ok(())
}

fn validate_reviewed_capacities(
    input: &CharacterCollection,
    output: &CharacterCollection,
    proposal: &ProjectionProposal,
) -> Result<(), CharacterError> {
    let before = projection_counts(input, &proposal.pack_ref);
    let after = projection_counts(output, &proposal.pack_ref);
    for taxonomy_id in &proposal.config.selected_taxonomies {
        for entry_id in proposal.pack.taxonomies[taxonomy_id].entries.keys() {
            let capacity =
                effective_capacity(&proposal.pack, &proposal.config, taxonomy_id, entry_id);
            if let Some(capacity) = capacity {
                let before_count = before
                    .get(taxonomy_id)
                    .and_then(|entries| entries.get(entry_id))
                    .copied()
                    .unwrap_or(0);
                let after_count = after
                    .get(taxonomy_id)
                    .and_then(|entries| entries.get(entry_id))
                    .copied()
                    .unwrap_or(0);
                if after_count > capacity.max(before_count) {
                    return Err(error(
                        CharacterDiagnosticCode::InvalidValue,
                        "review.decisions",
                        "reviewed projection edits exceed effective capacity",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn projection_counts(
    collection: &CharacterCollection,
    pack_ref: &ProjectionPackRef,
) -> BTreeMap<String, BTreeMap<String, u32>> {
    let mut counts = BTreeMap::<String, BTreeMap<String, u32>>::new();
    for profile in collection.characters.values() {
        let Some(CharacterExtension::RoleProjections(record)) =
            profile.extensions.get(ROLE_PROJECTION_EXTENSION_NAMESPACE)
        else {
            continue;
        };
        for value in record
            .value
            .roles
            .values()
            .filter(|value| value.pack.as_ref() == Some(pack_ref))
        {
            *counts
                .entry(value.taxonomy.clone())
                .or_default()
                .entry(value.entry_id.clone())
                .or_default() += 1;
        }
    }
    counts
}

fn collection_projection_distribution(
    collection: &CharacterCollection,
    pack_ref: &ProjectionPackRef,
) -> ProjectionDistribution {
    ProjectionDistribution {
        counts: projection_counts(collection, pack_ref),
        ..ProjectionDistribution::default()
    }
}

pub fn validate_projection_receipt(receipt: &ProjectionReceipt) -> Result<(), CharacterError> {
    validate_projection_receipt_structure(receipt)?;
    let expected = build_projection_receipt(
        &receipt.proposal.input_collection,
        &receipt.proposal,
        &receipt.review,
    )?;
    if &expected != receipt {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "receipt",
            "projection receipt does not reproduce from its embedded inputs",
        ));
    }
    Ok(())
}

fn validate_projection_receipt_structure(
    receipt: &ProjectionReceipt,
) -> Result<(), CharacterError> {
    if receipt.receipt_format_version != PROJECTION_RECEIPT_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "receipt_format_version",
            "unsupported projection receipt version",
        ));
    }
    validate_namespaced_id("id", &receipt.id)?;
    validate_projection_proposal_structure(&receipt.proposal)?;
    validate_projection_review_structure(&receipt.review)?;
    validate_sha256("proposal_sha256", &receipt.proposal_sha256)?;
    validate_sha256("review_sha256", &receipt.review_sha256)?;
    validate_character_collection(&receipt.output_collection)?;
    validate_sha256("output_sha256", &receipt.output_sha256)
}

/// Apply a lock/unlock revision atomically without changing any projection content.
pub fn apply_projection_lock_revision(
    collection: &CharacterCollection,
    revision: &ProjectionLockRevision,
) -> Result<CharacterCollection, CharacterError> {
    validate_character_collection(collection)?;
    validate_projection_lock_revision(revision, collection)?;
    let mut output = collection.clone();
    for target in &revision.targets {
        let profile = output
            .characters
            .get_mut(&target.character_id)
            .expect("validated projection lock target has a character");
        let CharacterExtension::RoleProjections(record) = profile
            .extensions
            .get_mut(ROLE_PROJECTION_EXTENSION_NAMESPACE)
            .expect("validated projection lock target has an extension")
        else {
            unreachable!("reserved role namespace is typed")
        };
        record
            .value
            .roles
            .get_mut(&target.projection_id)
            .expect("validated projection lock target has a value")
            .lock = revision.lock;
    }
    if output != *collection {
        output.revision = output.revision.checked_add(1).ok_or_else(|| {
            invalid_value(
                "output_collection.revision",
                "collection revision overflowed",
            )
        })?;
    }
    validate_character_collection(&output)?;
    Ok(output)
}

pub fn validate_projection_lock_revision(
    revision: &ProjectionLockRevision,
    collection: &CharacterCollection,
) -> Result<(), CharacterError> {
    validate_projection_lock_revision_structure(revision)?;
    if collection_fingerprint(collection)? != revision.expected_input_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "expected_input_sha256",
            "projection lock revision targets a stale collection",
        ));
    }
    for target in &revision.targets {
        let Some(profile) = collection.characters.get(&target.character_id) else {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "targets.character_id",
                "projection lock target references an unknown character",
            ));
        };
        let Some(CharacterExtension::RoleProjections(record)) =
            profile.extensions.get(ROLE_PROJECTION_EXTENSION_NAMESPACE)
        else {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "targets.projection_id",
                "projection lock target references a missing role projection",
            ));
        };
        if !record.value.roles.contains_key(&target.projection_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "targets.projection_id",
                "projection lock target references a missing role projection",
            ));
        }
    }
    Ok(())
}

pub fn validate_projection_lock_revision_structure(
    revision: &ProjectionLockRevision,
) -> Result<(), CharacterError> {
    if revision.revision_format_version != PROJECTION_LOCK_REVISION_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "revision_format_version",
            "unsupported projection lock revision version",
        ));
    }
    validate_namespaced_id("id", &revision.id)?;
    validate_sha256("expected_input_sha256", &revision.expected_input_sha256)?;
    if revision.targets.is_empty() || !strictly_sorted_unique(&revision.targets) {
        return Err(invalid_value(
            "targets",
            "projection lock targets must be non-empty, sorted, and unique",
        ));
    }
    for target in &revision.targets {
        validate_namespaced_id("targets.character_id", &target.character_id)?;
        validate_local_id("targets.projection_id", &target.projection_id)?;
    }
    validate_public_text("rationale", &revision.rationale, 1, 2_048)?;
    validate_provenance(&revision.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "projection lock revision provenance is malformed or incomplete",
        )
    })
}

/// Validate the role-projection extension independently of a particular installed pack.
pub(crate) fn validate_role_projections_data(
    namespace: &str,
    profile_id: &str,
    value: &RoleProjections,
) -> Result<(), CharacterError> {
    let root = format!("extensions.{namespace}.value");
    if value.projection_format_version != role_projection_format_version() {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            format!("{root}.projection_format_version"),
            "unsupported role-projection value version",
        ));
    }
    if value.character_id != profile_id {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{root}.character_id"),
            "role-projection owner must equal the containing profile id",
        ));
    }
    if value.roles.len() > MAX_ENTRIES {
        return Err(invalid_value(
            format!("{root}.roles"),
            "role-projection value contains too many entries",
        ));
    }
    let mut taxonomies = BTreeSet::new();
    for (id, role) in &value.roles {
        let path = format!("{root}.roles.{id}");
        validate_local_id(&path, id)?;
        if role.id != *id || role.character_id != profile_id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "role-projection identity or owner is inconsistent",
            ));
        }
        if !taxonomies.insert(role.taxonomy.as_str()) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{root}.roles"),
                "a profile can retain only one projection per taxonomy",
            ));
        }
        validate_namespaced_id(&format!("{path}.taxonomy"), &role.taxonomy)?;
        validate_local_id(&format!("{path}.entry_id"), &role.entry_id)?;
        validate_public_text(&format!("{path}.label"), &role.label, 1, 256)?;
        if role.independent_evidence
            || matches!(role.kind, ProjectionKind::CategoricalPersonality) != role.lossy
        {
            return Err(error(
                CharacterDiagnosticCode::ForbiddenWriteBack,
                path,
                "role projections cannot claim independent evidence and categorical views must remain lossy",
            ));
        }
        if let Some(score) = role.score_micros {
            validate_signed_micros(&format!("{path}.score_micros"), score)?;
        }
        validate_micros(&format!("{path}.coverage_micros"), role.coverage_micros)?;
        validate_public_text(&format!("{path}.explanation"), &role.explanation, 1, 4_096)?;
        validate_public_text(&format!("{path}.rationale"), &role.rationale, 1, 2_048)?;
        validate_sorted_paths(&format!("{path}.input_paths"), &role.input_paths)?;
        if let Some(pack) = &role.pack {
            validate_projection_pack_ref(&format!("{path}.pack"), pack)?;
        }
        if let Some(value) = &role.proposal_sha256 {
            validate_sha256(&format!("{path}.proposal_sha256"), value)?;
        }
        if let Some(value) = &role.review_sha256 {
            validate_sha256(&format!("{path}.review_sha256"), value)?;
        }
        let authored = matches!(role.decision, ProjectionPublicDecision::Authored);
        if authored != role.pack.is_none()
            || authored != role.proposal_sha256.is_none()
            || authored != role.review_sha256.is_none()
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "authored projections omit pack lineage while reviewed projections require it",
            ));
        }
        if matches!(role.decision, ProjectionPublicDecision::Derived)
            && !matches!(role.kind, ProjectionKind::CategoricalPersonality)
        {
            return Err(invalid_value(
                format!("{path}.decision"),
                "only categorical personality views use the derived public decision",
            ));
        }
    }
    if !strictly_sorted_unique(&value.source_pack_refs) {
        return Err(invalid_value(
            format!("{root}.source_pack_refs"),
            "projection pack references must be sorted and unique",
        ));
    }
    for (index, pack_ref) in value.source_pack_refs.iter().enumerate() {
        validate_projection_pack_ref(&format!("{root}.source_pack_refs[{index}]"), pack_ref)?;
    }
    for role in value.roles.values() {
        if role
            .pack
            .as_ref()
            .is_some_and(|pack| !value.source_pack_refs.contains(pack))
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{root}.source_pack_refs"),
                "role projection references an undeclared exact pack",
            ));
        }
    }
    Ok(())
}

fn validate_projection_pack_ref(
    path: &str,
    value: &ProjectionPackRef,
) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

fn validate_character_collection(value: &CharacterCollection) -> Result<(), CharacterError> {
    validate_corpus_collection(value).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidValue,
            "collection",
            "character collection is invalid for projection processing",
        )
    })
}

fn collection_fingerprint(value: &CharacterCollection) -> Result<String, CharacterError> {
    corpus_collection_fingerprint(value).map_err(|_| encoding_error())
}

fn evaluate_complete_entry(
    entry: &ProjectionEntry,
    inputs: &BTreeMap<String, u32>,
) -> Result<i32, CharacterError> {
    let total_weight = entry
        .evidence
        .values()
        .map(|weight| u32::from(weight.unsigned_abs()))
        .sum::<u32>();
    let mut weighted_sum = 0_i64;
    for (input_id, weight) in &entry.evidence {
        let value = inputs.get(input_id).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "calibration.inputs_micros",
                "projection calibration is missing a declared input",
            )
        })?;
        let centered = i64::from(*value) * 2 - SCORE_SCALE;
        weighted_sum += centered * i64::from(*weight);
    }
    Ok(i32::try_from(rounded_div(
        i128::from(weighted_sum),
        i128::from(total_weight),
    ))
    .expect("complete projection score stays within signed millionths"))
}

fn measurement_micros(value: TraitMeasurement) -> u32 {
    let normalized = match value {
        TraitMeasurement::Score { score } => score,
        TraitMeasurement::Band { band } => band.projection_anchor(),
    };
    (normalized * SCORE_SCALE as f64).round() as u32
}

fn rounded_div(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    if numerator >= 0 {
        (numerator + denominator / 2) / denominator
    } else {
        -((-numerator + denominator / 2) / denominator)
    }
}

fn validate_micros(path: &str, value: u32) -> Result<(), CharacterError> {
    if value > SCORE_SCALE as u32 {
        return Err(invalid_value(
            path,
            "expected unsigned millionths from zero through one million",
        ));
    }
    Ok(())
}

fn validate_signed_micros(path: &str, value: i32) -> Result<(), CharacterError> {
    if !(-(SCORE_SCALE as i32)..=SCORE_SCALE as i32).contains(&value) {
        return Err(invalid_value(
            path,
            "expected signed millionths from negative through positive one million",
        ));
    }
    Ok(())
}

fn validate_public_text(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterError> {
    validate_text(path, value, minimum, maximum)?;
    if contains_secret_shape(value) {
        return Err(error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "text resembles a credential and was rejected without disclosure",
        ));
    }
    Ok(())
}

fn contains_secret_shape(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "authorization: bearer ",
        "api_key=",
        "api-key=",
        "password=",
        "private_key",
        "database_url=",
        "github_pat_",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || lower.contains("ghp_")
        || lower.contains("akia")
        || lower.contains("begin rsa private key")
}

fn validate_https(path: &str, value: &str) -> Result<(), CharacterError> {
    validate_text(path, value, 8, 2_048)?;
    if !value.starts_with("https://") || value.contains('@') || value.contains(char::is_whitespace)
    {
        return Err(invalid_value(
            path,
            "expected a credential-free public HTTPS URL",
        ));
    }
    Ok(())
}

fn validate_sorted_namespaced_ids(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if !strictly_sorted_unique(values) {
        return Err(invalid_value(path, "identifiers must be sorted and unique"));
    }
    for value in values {
        validate_namespaced_id(path, value)?;
    }
    Ok(())
}

fn validate_sorted_prefixes(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if !strictly_sorted_unique(values) {
        return Err(invalid_value(
            path,
            "identifier prefixes must be sorted and unique",
        ));
    }
    for value in values {
        validate_namespaced_id(path, value)?;
    }
    Ok(())
}

fn validate_sorted_paths(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if !strictly_sorted_unique(values) {
        return Err(invalid_value(path, "input paths must be sorted and unique"));
    }
    for value in values {
        if !value.starts_with("canon.personality.")
            || value.split('.').any(|segment| segment.is_empty())
        {
            return Err(error(
                CharacterDiagnosticCode::ForbiddenWriteBack,
                path,
                "projection input paths must remain canonical HEXACO paths",
            ));
        }
    }
    Ok(())
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn provenance_ids(provenance: &Provenance) -> BTreeSet<String> {
    provenance
        .sources
        .iter()
        .map(|source| source.id.clone())
        .chain(
            provenance
                .transformations
                .iter()
                .map(|transformation| transformation.id.clone()),
        )
        .collect()
}

fn sort_deduplicate_claims(provenance: &mut Provenance) {
    for references in provenance.claims.values_mut() {
        references.sort();
        references.dedup();
    }
}

fn projection_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, CharacterError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-projection-version".to_owned(),
            serde_json::Value::from(1),
        );
    }
    sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| encoding_error())
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for value in map.values_mut() {
                sort_json_keys(value);
            }
            let old = std::mem::take(map);
            let mut entries = old.into_iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            map.extend(entries);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        _ => {}
    }
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, CharacterError> {
    let bytes = to_pretty_json(value).map_err(|_| encoding_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes.as_bytes())))
}

fn encoding_error() -> CharacterError {
    error(
        CharacterDiagnosticCode::InvalidEncoding,
        "projection",
        "could not encode the projection document",
    )
}
