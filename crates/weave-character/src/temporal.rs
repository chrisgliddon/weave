//! Deterministic, review-gated temporal context for fictional character authoring.
//!
//! Temporal records keep dated facts separate from interpretive authoring cues. Matching a date
//! can make a cue discoverable, but it never turns a historical, calendrical, environmental, or
//! celestial fact into personality evidence and never writes directly into character canon.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    DomainValue, Provenance, ProvenanceTransformation, parse_strict_json, to_pretty_json,
    to_pretty_ron, validate_provenance,
};

use crate::model::{
    AcceptedDateContextCue, Attributed, BirthDate, Calendar, CharacterDiagnosticCode,
    CharacterExtension, CharacterProfile, CharacterSuggestion, Confidence, DateContext,
    DateContextCueKind, DateContextDecision, DateContextPackRef, DateContextSensitivity,
    DateContextUncertainty, ExtensionHeader, ExtensionWriteBack, Freshness, HexacoTrait, LockState,
    ReviewState, TraitMeasurement, ValueState, VersionedExtension,
};
use crate::synthesis::merge_provenance;
use crate::validation::{CharacterError, error, invalid_value, trait_value, validate_profile};

/// Serialized temporal-context pack contract version.
pub const TEMPORAL_CONTEXT_PACK_FORMAT_VERSION: u32 = 1;
/// Serialized temporal ranking configuration contract version.
pub const TEMPORAL_CONTEXT_CONFIG_FORMAT_VERSION: u32 = 1;
/// Serialized temporal proposal contract version.
pub const TEMPORAL_CONTEXT_PROPOSAL_FORMAT_VERSION: u32 = 1;
/// Serialized temporal review contract version.
pub const TEMPORAL_CONTEXT_REVIEW_FORMAT_VERSION: u32 = 1;
/// Serialized independently reproducible receipt contract version.
pub const TEMPORAL_CONTEXT_RECEIPT_FORMAT_VERSION: u32 = 1;

/// Reserved profile extension carrying reviewed temporal cues.
pub const DATE_CONTEXT_EXTENSION_NAMESPACE: &str = "org.weave.character.date_context";

const SCORE_SCALE: i64 = 1_000_000;
const MAX_TEXT: usize = 65_536;
const MAX_RECORDS: usize = 65_536;
const MAX_DOMAIN_DEPTH: usize = 32;
const MAX_DOMAIN_NODES: usize = 262_144;
const TEMPORAL_SUGGESTION_RATIONALE: &str = "Reviewed temporal context remains a pending fictional authoring suggestion until a separate canonical authoring operation accepts it.";

const PACK_SCHEMA_ID: &str = "urn:weave:schema:character-temporal-pack:1";
const CONFIG_SCHEMA_ID: &str = "urn:weave:schema:character-temporal-config:1";
const PROPOSAL_SCHEMA_ID: &str = "urn:weave:schema:character-temporal-proposal:1";
const REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-temporal-review:1";
const RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-temporal-receipt:1";

/// Offline, versioned temporal context records with complete public provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextPack {
    pub pack_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub license: String,
    pub license_url: String,
    pub provider: TemporalContextProvider,
    pub records: BTreeMap<String, TemporalContextRecord>,
    pub provenance: Provenance,
}

/// Exact offline provider identity; a domain module can supply World-compatible projections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum TemporalContextProvider {
    Standalone,
    DomainModule {
        module_id: String,
        module_version: String,
        pack_id: String,
        pack_version: String,
        content_sha256: String,
    },
}

/// One dated fact plus separately authored fictional cues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextRecord {
    pub id: String,
    pub kind: TemporalRecordKind,
    pub evidence_class: TemporalEvidenceClass,
    pub extent: TemporalExtent,
    pub place_scope: TemporalPlaceScope,
    pub time_zone: TemporalTimeZone,
    pub reference_period: TemporalReferencePeriod,
    /// Structured fact payload. Ranking never reads this field.
    pub fact: DomainValue,
    pub tags: Vec<String>,
    /// Original fictional authoring cues keyed by stable local identifier.
    pub cues: BTreeMap<String, TemporalAuthoringCue>,
    pub source_ids: Vec<String>,
    pub uncertainty: TemporalUncertainty,
    pub sensitivity: TemporalSensitivity,
}

/// Closed record taxonomy used for filters and coverage reporting.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TemporalRecordKind {
    CalendricalFact,
    SeasonalFact,
    EnvironmentalFact,
    CelestialFact,
    Commemoration,
    HistoricalEvent,
}

/// Whether the fact payload is measured, calendrical, or explicitly interpretive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporalEvidenceClass {
    MeasuredFact,
    CalendricalFact,
    InterpretiveContext,
}

/// Proleptic-Gregorian date with no time-of-day conversion.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct TemporalDate {
    pub calendar: Calendar,
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// Temporal coverage of a record. Month/day records are explicitly recurring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "precision", deny_unknown_fields)]
pub enum TemporalExtent {
    Year {
        year: i32,
    },
    MonthDay {
        month: u8,
        day: u8,
    },
    Date {
        date: TemporalDate,
    },
    YearRange {
        start_year: i32,
        end_year: i32,
    },
    DateRange {
        start: TemporalDate,
        end: TemporalDate,
    },
}

/// Geographic applicability. Matching is exact in v1; hierarchy can be supplied as multiple IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "scope", deny_unknown_fields)]
pub enum TemporalPlaceScope {
    Global,
    Place { id: String, kind: String },
}

/// Time-zone metadata retained for interpretation; date-only matching never shifts a date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum TemporalTimeZone {
    Utc,
    Iana { id: String },
    Unknown { reason: String },
}

/// Exact period and resolution against which the record was authored or measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalReferencePeriod {
    pub label: String,
    pub start: TemporalDate,
    pub end: TemporalDate,
    pub resolution: TemporalResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporalResolution {
    Day,
    Year,
    RecurringDay,
    Period,
}

/// One original fictional prompt and its declared, non-causal ranking vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalAuthoringCue {
    pub id: String,
    pub kind: DateContextCueKind,
    pub content: String,
    /// Signed weights in inclusive thousandths (-1000..=1000), never inferred from the fact.
    pub trait_vector: BTreeMap<HexacoTrait, i16>,
    pub tags: Vec<String>,
    pub limitations: String,
    /// Lineage for the fictional prompt and declared vector, separate from the dated fact.
    pub source_ids: Vec<String>,
}

/// Bounded fact uncertainty with a mandatory public explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalUncertainty {
    pub level: DateContextUncertainty,
    pub reason: String,
}

/// Review sensitivity attached independently from uncertainty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalSensitivity {
    pub level: DateContextSensitivity,
    pub topics: Vec<String>,
    pub requires_explicit_review: bool,
    pub note: String,
}

#[derive(Serialize)]
struct TemporalProviderRecordFingerprint<'a> {
    id: &'a str,
    kind: TemporalRecordKind,
    evidence_class: TemporalEvidenceClass,
    extent: &'a TemporalExtent,
    place_scope: &'a TemporalPlaceScope,
    time_zone: &'a TemporalTimeZone,
    reference_period: &'a TemporalReferencePeriod,
    fact: &'a DomainValue,
    tags: &'a [String],
    source_ids: &'a [String],
    uncertainty: &'a TemporalUncertainty,
    sensitivity: &'a TemporalSensitivity,
}

/// Deterministic ranking and policy configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextConfig {
    pub config_format_version: u32,
    pub id: String,
    /// Inclusive relevance threshold in millionths.
    pub minimum_relevance_micros: u32,
    /// Inclusive trait-vector coverage threshold in millionths.
    pub minimum_trait_coverage_micros: u32,
    pub maximum_candidates: u32,
    pub allowed_record_kinds: Vec<TemporalRecordKind>,
    pub allowed_cue_kinds: Vec<DateContextCueKind>,
    pub allowed_sensitivities: Vec<DateContextSensitivity>,
    pub required_tags: Vec<String>,
    /// Exact place IDs considered applicable, sorted and pinned by the proposal.
    pub place_scope_ids: Vec<String>,
    pub time_zone: TemporalTimeZone,
    pub allow_global_without_place: bool,
    pub auto_approve: TemporalAutoApprovePolicy,
    pub override_locked_context: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_rationale: Option<String>,
}

/// Explicit, narrow auto-review policy. Only low-sensitivity cues can qualify.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "policy", deny_unknown_fields)]
pub enum TemporalAutoApprovePolicy {
    Disabled,
    LowSensitivity {
        policy_id: String,
        minimum_relevance_micros: u32,
        cue_kinds: Vec<DateContextCueKind>,
        rationale: String,
    },
}

/// Exact temporal precision used to match a candidate.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TemporalMatchKind {
    ExactDate,
    RecurringMonthDay,
    SameYear,
    ContainingPeriod,
}

/// Why one cue was selected, skipped, or downgraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporalCoverageDisposition {
    Selected,
    Skipped,
    Downgraded,
}

/// Stable coverage report entry for every cue considered from every selected pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalCoverageEntry {
    pub candidate_id: String,
    pub pack_id: String,
    pub record_id: String,
    pub cue_id: String,
    pub disposition: TemporalCoverageDisposition,
    pub reason: String,
}

/// One fixed-point trait contribution in an explainable score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalRankingContribution {
    pub trait_id: HexacoTrait,
    pub profile_micros: u32,
    pub cue_weight_thousandths: i16,
    pub compatibility_micros: u32,
    pub weighted_contribution_micros: i64,
}

/// Complete score trace; no fact payload contributes to these numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalRankingTrace {
    pub match_kind: TemporalMatchKind,
    pub temporal_factor_micros: u32,
    pub place_factor_micros: u32,
    pub time_zone_factor_micros: u32,
    pub uncertainty_factor_micros: u32,
    pub trait_coverage_micros: u32,
    pub cue_compatibility_micros: u32,
    pub contributions: Vec<TemporalRankingContribution>,
    pub explanation: String,
}

/// One ranked fictional cue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextCandidate {
    pub id: String,
    pub pack: DateContextPackRef,
    pub record_id: String,
    pub cue_id: String,
    pub kind: DateContextCueKind,
    pub content: String,
    pub relevance_micros: u32,
    pub uncertainty: DateContextUncertainty,
    pub sensitivity: DateContextSensitivity,
    pub requires_explicit_review: bool,
    pub fact_source_ids: Vec<String>,
    pub cue_source_ids: Vec<String>,
    /// Sorted union retained for downstream lineage and compatibility.
    pub source_ids: Vec<String>,
    pub trace: TemporalRankingTrace,
    pub tie_break_sha256: String,
    pub auto_approve_eligible: bool,
}

/// Immutable proposal pinning every ranking input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextProposal {
    pub proposal_format_version: u32,
    pub profile_id: String,
    pub profile_sha256: String,
    pub birth_date: BirthDate,
    pub packs: Vec<DateContextPackRef>,
    pub config: TemporalContextConfig,
    pub config_sha256: String,
    pub seed: u64,
    pub candidates: Vec<TemporalContextCandidate>,
    pub coverage: Vec<TemporalCoverageEntry>,
}

/// One complete review decision per proposed candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextReview {
    pub review_format_version: u32,
    pub proposal_sha256: String,
    pub profile_sha256: String,
    pub reviewer: String,
    pub rationale: String,
    pub decisions: BTreeMap<String, TemporalReviewDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalReviewDecision {
    pub candidate_id: String,
    pub action: TemporalReviewAction,
}

/// Explicit editor decision. Edit and override retain both the changed content and rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "decision", deny_unknown_fields)]
pub enum TemporalReviewAction {
    Accept {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    Reject {
        rationale: String,
    },
    Edit {
        content: String,
        rationale: String,
    },
    Withhold {
        rationale: String,
    },
    Override {
        content: String,
        rationale: String,
    },
    AutoApprove {
        policy_id: String,
    },
}

/// Full proof allowing a consumer to independently reproduce proposal, review, and atomic output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextReceipt {
    pub receipt_format_version: u32,
    pub input_profile: CharacterProfile,
    pub packs: Vec<TemporalContextPack>,
    pub proposal: TemporalContextProposal,
    pub review: TemporalContextReview,
    pub proposal_sha256: String,
    pub review_sha256: String,
    pub output_profile_sha256: String,
    pub output_profile: CharacterProfile,
}

macro_rules! impl_temporal_document {
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

impl_temporal_document!(TemporalContextPack, validate_temporal_context_pack);
impl_temporal_document!(TemporalContextConfig, validate_temporal_context_config);
impl_temporal_document!(TemporalContextProposal, validate_proposal_structure);
impl_temporal_document!(TemporalContextReview, validate_review_structure);
impl_temporal_document!(TemporalContextReceipt, validate_temporal_context_receipt);

/// Generate the canonical temporal pack JSON Schema.
pub fn temporal_context_pack_schema() -> Result<String, CharacterError> {
    temporal_schema::<TemporalContextPack>(PACK_SCHEMA_ID, "Weave Character Temporal Pack v1")
}

/// Generate the canonical temporal ranking configuration JSON Schema.
pub fn temporal_context_config_schema() -> Result<String, CharacterError> {
    temporal_schema::<TemporalContextConfig>(
        CONFIG_SCHEMA_ID,
        "Weave Character Temporal Configuration v1",
    )
}

/// Generate the canonical temporal proposal JSON Schema.
pub fn temporal_context_proposal_schema() -> Result<String, CharacterError> {
    temporal_schema::<TemporalContextProposal>(
        PROPOSAL_SCHEMA_ID,
        "Weave Character Temporal Proposal v1",
    )
}

/// Generate the canonical temporal review JSON Schema.
pub fn temporal_context_review_schema() -> Result<String, CharacterError> {
    temporal_schema::<TemporalContextReview>(REVIEW_SCHEMA_ID, "Weave Character Temporal Review v1")
}

/// Generate the canonical independently reproducible receipt JSON Schema.
pub fn temporal_context_receipt_schema() -> Result<String, CharacterError> {
    temporal_schema::<TemporalContextReceipt>(
        RECEIPT_SCHEMA_ID,
        "Weave Character Temporal Receipt v1",
    )
}

/// Canonical content fingerprint for one independently validated context pack.
pub fn temporal_pack_fingerprint(pack: &TemporalContextPack) -> Result<String, CharacterError> {
    validate_temporal_context_pack(pack)?;
    canonical_hash(pack)
}

/// Fingerprint the exact fact projection supplied by a domain-module provider.
///
/// Fictional cues and their declared vectors are deliberately excluded so their lineage remains
/// independent from the measured, calendrical, or interpretive records supplied by the provider.
pub fn temporal_provider_content_fingerprint(
    records: &BTreeMap<String, TemporalContextRecord>,
) -> Result<String, CharacterError> {
    let projection = records
        .iter()
        .map(|(id, record)| {
            (
                id.as_str(),
                TemporalProviderRecordFingerprint {
                    id: &record.id,
                    kind: record.kind,
                    evidence_class: record.evidence_class,
                    extent: &record.extent,
                    place_scope: &record.place_scope,
                    time_zone: &record.time_zone,
                    reference_period: &record.reference_period,
                    fact: &record.fact,
                    tags: &record.tags,
                    source_ids: &record.source_ids,
                    uncertainty: &record.uncertainty,
                    sensitivity: &record.sensitivity,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    canonical_hash(&projection)
}

/// Canonical content fingerprint for one independently validated ranking configuration.
pub fn temporal_config_fingerprint(
    config: &TemporalContextConfig,
) -> Result<String, CharacterError> {
    validate_temporal_context_config(config)?;
    canonical_hash(config)
}

/// Canonical profile fingerprint pinned by proposals and reviews.
pub fn temporal_profile_fingerprint(profile: &CharacterProfile) -> Result<String, CharacterError> {
    validate_profile(profile)?;
    canonical_hash(profile)
}

/// Canonical proposal fingerprint used by a complete review.
pub fn temporal_proposal_fingerprint(
    proposal: &TemporalContextProposal,
) -> Result<String, CharacterError> {
    validate_proposal_structure(proposal)?;
    canonical_hash(proposal)
}

/// Canonical review fingerprint retained by accepted cues and receipts.
pub fn temporal_review_fingerprint(
    review: &TemporalContextReview,
) -> Result<String, CharacterError> {
    validate_review_structure(review)?;
    canonical_hash(review)
}

/// Validate one complete offline temporal pack.
pub fn validate_temporal_context_pack(pack: &TemporalContextPack) -> Result<(), CharacterError> {
    if pack.pack_format_version != TEMPORAL_CONTEXT_PACK_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "pack_format_version",
            "unsupported temporal context pack version",
        ));
    }
    validate_namespaced_id("id", &pack.id)?;
    validate_semver("version", &pack.version)?;
    validate_public_text("title", &pack.title, 1, 256)?;
    validate_license("license", &pack.license)?;
    validate_https("license_url", &pack.license_url)?;
    match &pack.provider {
        TemporalContextProvider::Standalone => {}
        TemporalContextProvider::DomainModule {
            module_id,
            module_version,
            pack_id,
            pack_version,
            content_sha256,
        } => {
            validate_namespaced_id("provider.module_id", module_id)?;
            validate_semver("provider.module_version", module_version)?;
            validate_local_id("provider.pack_id", pack_id)?;
            validate_semver("provider.pack_version", pack_version)?;
            validate_sha256("provider.content_sha256", content_sha256)?;
        }
    }
    validate_provenance(&pack.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "temporal context provenance is malformed or incomplete",
        )
    })?;
    let lineage = provenance_ids(&pack.provenance);
    if pack.records.is_empty() || pack.records.len() > MAX_RECORDS {
        return Err(invalid_value(
            "records",
            "temporal pack requires a bounded non-empty record set",
        ));
    }
    for (id, record) in &pack.records {
        let path = format!("records.{id}");
        validate_local_id(&path, id)?;
        if record.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "temporal record id must equal its containing map key",
            ));
        }
        validate_temporal_record(&path, record, &lineage)?;
    }
    if let TemporalContextProvider::DomainModule { content_sha256, .. } = &pack.provider
        && content_sha256 != &temporal_provider_content_fingerprint(&pack.records)?
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "provider.content_sha256",
            "domain-module temporal projection content fingerprint is stale",
        ));
    }
    Ok(())
}

fn validate_temporal_record(
    path: &str,
    record: &TemporalContextRecord,
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    validate_extent(&format!("{path}.extent"), &record.extent)?;
    match &record.place_scope {
        TemporalPlaceScope::Global => {}
        TemporalPlaceScope::Place { id, kind } => {
            validate_namespaced_id(&format!("{path}.place_scope.id"), id)?;
            validate_namespaced_id(&format!("{path}.place_scope.kind"), kind)?;
        }
    }
    validate_time_zone(&format!("{path}.time_zone"), &record.time_zone)?;
    validate_reference_period(
        &format!("{path}.reference_period"),
        &record.reference_period,
    )?;
    validate_domain_value(&format!("{path}.fact"), &record.fact)?;
    validate_sorted_tags(&format!("{path}.tags"), &record.tags)?;
    if record.cues.is_empty() || record.cues.len() > 4_096 {
        return Err(invalid_value(
            format!("{path}.cues"),
            "temporal record requires a bounded non-empty cue set",
        ));
    }
    for (id, cue) in &record.cues {
        let cue_path = format!("{path}.cues.{id}");
        validate_local_id(&cue_path, id)?;
        if cue.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{cue_path}.id"),
                "temporal cue id must equal its containing map key",
            ));
        }
        validate_public_text(&format!("{cue_path}.content"), &cue.content, 1, 2_048)?;
        validate_public_text(
            &format!("{cue_path}.limitations"),
            &cue.limitations,
            1,
            2_048,
        )?;
        validate_sorted_tags(&format!("{cue_path}.tags"), &cue.tags)?;
        validate_lineage(&format!("{cue_path}.source_ids"), &cue.source_ids, lineage)?;
        if cue.trait_vector.is_empty() || cue.trait_vector.len() > 30 {
            return Err(invalid_value(
                format!("{cue_path}.trait_vector"),
                "cue requires one through thirty declared trait weights",
            ));
        }
        if cue
            .trait_vector
            .values()
            .any(|weight| !(-1_000..=1_000).contains(weight) || *weight == 0)
        {
            return Err(invalid_value(
                format!("{cue_path}.trait_vector"),
                "trait weights must be non-zero signed thousandths",
            ));
        }
    }
    validate_lineage(&format!("{path}.source_ids"), &record.source_ids, lineage)?;
    validate_public_text(
        &format!("{path}.uncertainty.reason"),
        &record.uncertainty.reason,
        1,
        2_048,
    )?;
    validate_sorted_tags(
        &format!("{path}.sensitivity.topics"),
        &record.sensitivity.topics,
    )?;
    validate_public_text(
        &format!("{path}.sensitivity.note"),
        &record.sensitivity.note,
        1,
        2_048,
    )?;
    if record.sensitivity.level != DateContextSensitivity::Low
        && !record.sensitivity.requires_explicit_review
    {
        return Err(invalid_value(
            format!("{path}.sensitivity.requires_explicit_review"),
            "moderate and high sensitivity records require explicit review",
        ));
    }
    match (record.kind, record.evidence_class) {
        (TemporalRecordKind::CalendricalFact, TemporalEvidenceClass::CalendricalFact)
        | (
            TemporalRecordKind::SeasonalFact
            | TemporalRecordKind::EnvironmentalFact
            | TemporalRecordKind::CelestialFact,
            TemporalEvidenceClass::MeasuredFact | TemporalEvidenceClass::CalendricalFact,
        )
        | (
            TemporalRecordKind::Commemoration | TemporalRecordKind::HistoricalEvent,
            TemporalEvidenceClass::MeasuredFact
            | TemporalEvidenceClass::CalendricalFact
            | TemporalEvidenceClass::InterpretiveContext,
        ) => Ok(()),
        _ => Err(invalid_value(
            format!("{path}.evidence_class"),
            "record kind and evidence class are incompatible",
        )),
    }
}

/// Validate one deterministic ranking and review policy configuration.
pub fn validate_temporal_context_config(
    config: &TemporalContextConfig,
) -> Result<(), CharacterError> {
    if config.config_format_version != TEMPORAL_CONTEXT_CONFIG_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "config_format_version",
            "unsupported temporal ranking configuration version",
        ));
    }
    validate_namespaced_id("id", &config.id)?;
    validate_micros("minimum_relevance_micros", config.minimum_relevance_micros)?;
    validate_micros(
        "minimum_trait_coverage_micros",
        config.minimum_trait_coverage_micros,
    )?;
    if config.maximum_candidates == 0 || config.maximum_candidates > 16_384 {
        return Err(invalid_value(
            "maximum_candidates",
            "maximum candidates must be from one through 16384",
        ));
    }
    validate_sorted_enum("allowed_record_kinds", &config.allowed_record_kinds)?;
    validate_sorted_enum("allowed_cue_kinds", &config.allowed_cue_kinds)?;
    validate_sorted_enum("allowed_sensitivities", &config.allowed_sensitivities)?;
    if config.allowed_record_kinds.is_empty()
        || config.allowed_cue_kinds.is_empty()
        || config.allowed_sensitivities.is_empty()
    {
        return Err(invalid_value(
            "allowed_record_kinds",
            "ranking filters must each retain at least one allowed value",
        ));
    }
    validate_sorted_tags("required_tags", &config.required_tags)?;
    validate_sorted_namespaced_ids("place_scope_ids", &config.place_scope_ids)?;
    validate_time_zone("time_zone", &config.time_zone)?;
    match &config.auto_approve {
        TemporalAutoApprovePolicy::Disabled => {}
        TemporalAutoApprovePolicy::LowSensitivity {
            policy_id,
            minimum_relevance_micros,
            cue_kinds,
            rationale,
        } => {
            validate_namespaced_id("auto_approve.policy_id", policy_id)?;
            validate_micros(
                "auto_approve.minimum_relevance_micros",
                *minimum_relevance_micros,
            )?;
            validate_sorted_enum("auto_approve.cue_kinds", cue_kinds)?;
            if cue_kinds.is_empty() {
                return Err(invalid_value(
                    "auto_approve.cue_kinds",
                    "auto-approval requires at least one cue kind",
                ));
            }
            validate_public_text("auto_approve.rationale", rationale, 1, 2_048)?;
        }
    }
    match (
        config.override_locked_context,
        config.override_rationale.as_deref(),
    ) {
        (true, Some(rationale)) => validate_public_text("override_rationale", rationale, 1, 2_048),
        (true, None) => Err(invalid_value(
            "override_rationale",
            "locked context override requires an explicit rationale",
        )),
        (false, None) => Ok(()),
        (false, Some(_)) => Err(invalid_value(
            "override_rationale",
            "override rationale is only valid when locked override is enabled",
        )),
    }
}

/// Rank fictional authoring cues from exact offline inputs without mutating the profile.
pub fn propose_temporal_context(
    profile: &CharacterProfile,
    packs: &[TemporalContextPack],
    config: &TemporalContextConfig,
    seed: u64,
) -> Result<TemporalContextProposal, CharacterError> {
    validate_profile(profile)?;
    validate_temporal_context_config(config)?;
    if packs.is_empty() || packs.len() > 4_096 {
        return Err(invalid_value(
            "packs",
            "temporal proposal requires a bounded non-empty pack set",
        ));
    }
    let birth_date = profile
        .canon
        .birth_date
        .as_ref()
        .map(|value| value.value.clone())
        .ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "canon.birth_date",
                "temporal context requires an explicit birth date",
            )
        })?;
    validate_birth_date("canon.birth_date.value", &birth_date)?;
    validate_existing_context_lock(profile, config)?;

    let mut resolved = packs
        .iter()
        .map(|pack| {
            validate_temporal_context_pack(pack)?;
            let reference = DateContextPackRef {
                id: pack.id.clone(),
                version: pack.version.clone(),
                sha256: temporal_pack_fingerprint(pack)?,
            };
            Ok((reference, pack))
        })
        .collect::<Result<Vec<_>, CharacterError>>()?;
    resolved.sort_by(|(left, _), (right, _)| left.cmp(right));
    if resolved.windows(2).any(|pair| pair[0].0.id == pair[1].0.id) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "packs",
            "a temporal proposal cannot select one pack identity more than once",
        ));
    }
    let _merged_provenance = resolved
        .iter()
        .try_fold(profile.provenance.clone(), |merged, (_, pack)| {
            merge_provenance(&merged, &pack.provenance)
        })?;

    let mut record_owners = BTreeMap::new();
    for (reference, pack) in &resolved {
        for id in pack.records.keys() {
            if record_owners.insert(id, &reference.id).is_some() {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    "packs.records",
                    "record identifiers must be globally unique across selected packs",
                ));
            }
        }
    }

    let mut candidates = Vec::new();
    let mut coverage = Vec::new();
    for (pack_ref, pack) in &resolved {
        for (record_id, record) in &pack.records {
            for (cue_id, cue) in &record.cues {
                let candidate_id = candidate_id(&pack_ref.id, record_id, cue_id);
                let base_coverage = |disposition, reason: &str| TemporalCoverageEntry {
                    candidate_id: candidate_id.clone(),
                    pack_id: pack_ref.id.clone(),
                    record_id: record_id.clone(),
                    cue_id: cue_id.clone(),
                    disposition,
                    reason: reason.to_owned(),
                };

                if !config.allowed_record_kinds.contains(&record.kind) {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Skipped,
                        "record kind is excluded by the pinned configuration",
                    ));
                    continue;
                }
                if !config.allowed_cue_kinds.contains(&cue.kind) {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Skipped,
                        "cue kind is excluded by the pinned configuration",
                    ));
                    continue;
                }
                if !config
                    .allowed_sensitivities
                    .contains(&record.sensitivity.level)
                {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Skipped,
                        "sensitivity is excluded by the pinned configuration",
                    ));
                    continue;
                }
                if !required_tags_match(&config.required_tags, &record.tags, &cue.tags) {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Skipped,
                        "required tags are absent",
                    ));
                    continue;
                }
                let Some(match_kind) = match_birth_date(&birth_date, &record.extent) else {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Skipped,
                        "record temporal precision does not match the known birth-date precision",
                    ));
                    continue;
                };
                let Some(place_factor_micros) = place_factor(record, config) else {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Skipped,
                        "record place is outside the pinned place scope",
                    ));
                    continue;
                };

                let (trait_coverage_micros, cue_compatibility_micros, contributions) =
                    score_trait_vector(profile, &cue.trait_vector);
                let temporal_factor_micros = temporal_factor(match_kind);
                let time_zone_factor_micros =
                    time_zone_factor(&record.time_zone, &config.time_zone);
                let uncertainty_factor_micros = uncertainty_factor(record.uncertainty.level);
                let relevance_micros = multiply_factors(&[
                    cue_compatibility_micros,
                    temporal_factor_micros,
                    place_factor_micros,
                    time_zone_factor_micros,
                    uncertainty_factor_micros,
                ]);

                if trait_coverage_micros < config.minimum_trait_coverage_micros {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Downgraded,
                        "declared cue vector has insufficient authored personality coverage",
                    ));
                    continue;
                }
                if relevance_micros < config.minimum_relevance_micros {
                    coverage.push(base_coverage(
                        TemporalCoverageDisposition::Downgraded,
                        "fixed-point relevance is below the pinned threshold",
                    ));
                    continue;
                }

                let tie_break_sha256 = sha256(
                    format!(
                        "{seed}\0{}\0{record_id}\0{cue_id}\0{}",
                        pack_ref.sha256, config.id
                    )
                    .as_bytes(),
                );
                let auto_approve_eligible = auto_approve_eligible(
                    config,
                    cue.kind,
                    record.sensitivity.level,
                    record.sensitivity.requires_explicit_review,
                    relevance_micros,
                );
                let source_ids = record
                    .source_ids
                    .iter()
                    .chain(&cue.source_ids)
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                candidates.push(TemporalContextCandidate {
                    id: candidate_id.clone(),
                    pack: pack_ref.clone(),
                    record_id: record_id.clone(),
                    cue_id: cue_id.clone(),
                    kind: cue.kind,
                    content: cue.content.clone(),
                    relevance_micros,
                    uncertainty: record.uncertainty.level,
                    sensitivity: record.sensitivity.level,
                    requires_explicit_review: record.sensitivity.requires_explicit_review,
                    fact_source_ids: record.source_ids.clone(),
                    cue_source_ids: cue.source_ids.clone(),
                    source_ids,
                    trace: TemporalRankingTrace {
                        match_kind,
                        temporal_factor_micros,
                        place_factor_micros,
                        time_zone_factor_micros,
                        uncertainty_factor_micros,
                        trait_coverage_micros,
                        cue_compatibility_micros,
                        contributions,
                        explanation: "Relevance uses only the cue's declared HEXACO vector plus temporal, place, time-zone, and uncertainty factors; the fact payload is never scored.".to_owned(),
                    },
                    tie_break_sha256,
                    auto_approve_eligible,
                });
                coverage.push(base_coverage(
                    TemporalCoverageDisposition::Selected,
                    "candidate met every pinned filter and fixed-point threshold",
                ));
            }
        }
    }

    candidates.sort_by(candidate_order);
    if candidates.len() > config.maximum_candidates as usize {
        let omitted = candidates
            .drain(config.maximum_candidates as usize..)
            .map(|candidate| candidate.id)
            .collect::<BTreeSet<_>>();
        for entry in &mut coverage {
            if omitted.contains(&entry.candidate_id) {
                entry.disposition = TemporalCoverageDisposition::Skipped;
                entry.reason = "candidate fell beyond the pinned result limit".to_owned();
            }
        }
    }
    coverage.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));

    let proposal = TemporalContextProposal {
        proposal_format_version: TEMPORAL_CONTEXT_PROPOSAL_FORMAT_VERSION,
        profile_id: profile.id.clone(),
        profile_sha256: temporal_profile_fingerprint(profile)?,
        birth_date,
        packs: resolved
            .into_iter()
            .map(|(reference, _)| reference)
            .collect(),
        config: config.clone(),
        config_sha256: temporal_config_fingerprint(config)?,
        seed,
        candidates,
        coverage,
    };
    validate_proposal_structure(&proposal)?;
    Ok(proposal)
}

/// Reproduce a proposal from the exact current profile and selected packs.
pub fn validate_temporal_context_proposal(
    proposal: &TemporalContextProposal,
    profile: &CharacterProfile,
    packs: &[TemporalContextPack],
) -> Result<(), CharacterError> {
    validate_proposal_structure(proposal)?;
    let expected = propose_temporal_context(profile, packs, &proposal.config, proposal.seed)?;
    if expected != *proposal {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal",
            "temporal proposal does not match its exact pinned inputs",
        ));
    }
    Ok(())
}

fn validate_proposal_structure(proposal: &TemporalContextProposal) -> Result<(), CharacterError> {
    if proposal.proposal_format_version != TEMPORAL_CONTEXT_PROPOSAL_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "proposal_format_version",
            "unsupported temporal context proposal version",
        ));
    }
    validate_namespaced_id("profile_id", &proposal.profile_id)?;
    validate_sha256("profile_sha256", &proposal.profile_sha256)?;
    validate_birth_date("birth_date", &proposal.birth_date)?;
    if proposal.packs.is_empty() || proposal.packs.len() > 4_096 {
        return Err(invalid_value(
            "packs",
            "proposal requires a bounded non-empty pack coordinate set",
        ));
    }
    let mut prior_pack: Option<&DateContextPackRef> = None;
    for pack in &proposal.packs {
        validate_pack_ref("packs", pack)?;
        if prior_pack.is_some_and(|prior| prior >= pack) {
            return Err(invalid_value(
                "packs",
                "proposal pack coordinates must be unique and sorted",
            ));
        }
        prior_pack = Some(pack);
    }
    validate_temporal_context_config(&proposal.config)?;
    validate_sha256("config_sha256", &proposal.config_sha256)?;
    if temporal_config_fingerprint(&proposal.config)? != proposal.config_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "config_sha256",
            "proposal configuration fingerprint is stale",
        ));
    }
    if proposal.candidates.len() > proposal.config.maximum_candidates as usize {
        return Err(invalid_value(
            "candidates",
            "proposal exceeds its pinned candidate limit",
        ));
    }
    let pack_refs = proposal.packs.iter().collect::<BTreeSet<_>>();
    let mut candidate_ids = BTreeSet::new();
    let mut prior_candidate: Option<&TemporalContextCandidate> = None;
    for (index, candidate) in proposal.candidates.iter().enumerate() {
        let path = format!("candidates[{index}]");
        validate_candidate(&path, candidate, &pack_refs)?;
        if !candidate_ids.insert(candidate.id.as_str()) {
            return Err(invalid_value(
                "candidates",
                "candidate identifiers must be unique",
            ));
        }
        if prior_candidate.is_some_and(|prior| candidate_order(prior, candidate).is_gt()) {
            return Err(invalid_value(
                "candidates",
                "candidates must use deterministic relevance and tie-break order",
            ));
        }
        prior_candidate = Some(candidate);
    }
    let mut prior_coverage = None;
    let mut coverage_ids = BTreeSet::new();
    for (index, entry) in proposal.coverage.iter().enumerate() {
        let path = format!("coverage[{index}]");
        validate_candidate_id(&format!("{path}.candidate_id"), &entry.candidate_id)?;
        validate_namespaced_id(&format!("{path}.pack_id"), &entry.pack_id)?;
        validate_local_id(&format!("{path}.record_id"), &entry.record_id)?;
        validate_local_id(&format!("{path}.cue_id"), &entry.cue_id)?;
        validate_public_text(&format!("{path}.reason"), &entry.reason, 1, 512)?;
        if prior_coverage.is_some_and(|prior: &str| prior >= entry.candidate_id.as_str())
            || !coverage_ids.insert(entry.candidate_id.as_str())
        {
            return Err(invalid_value(
                "coverage",
                "coverage entries must be unique and sorted by candidate id",
            ));
        }
        prior_coverage = Some(entry.candidate_id.as_str());
        let selected = entry.disposition == TemporalCoverageDisposition::Selected;
        if selected != candidate_ids.contains(entry.candidate_id.as_str()) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.disposition"),
                "selected coverage and ranked candidates must agree",
            ));
        }
    }
    if candidate_ids
        .iter()
        .any(|candidate| !coverage_ids.contains(candidate))
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "coverage",
            "every ranked candidate requires one coverage entry",
        ));
    }
    Ok(())
}

fn validate_candidate(
    path: &str,
    candidate: &TemporalContextCandidate,
    packs: &BTreeSet<&DateContextPackRef>,
) -> Result<(), CharacterError> {
    validate_candidate_id(&format!("{path}.id"), &candidate.id)?;
    validate_pack_ref(&format!("{path}.pack"), &candidate.pack)?;
    if !packs.contains(&candidate.pack) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.pack"),
            "candidate references a pack absent from the proposal",
        ));
    }
    validate_local_id(&format!("{path}.record_id"), &candidate.record_id)?;
    validate_local_id(&format!("{path}.cue_id"), &candidate.cue_id)?;
    validate_public_text(&format!("{path}.content"), &candidate.content, 1, 2_048)?;
    validate_micros(
        &format!("{path}.relevance_micros"),
        candidate.relevance_micros,
    )?;
    validate_sorted_provenance_ids(
        &format!("{path}.fact_source_ids"),
        &candidate.fact_source_ids,
    )?;
    validate_sorted_provenance_ids(&format!("{path}.cue_source_ids"), &candidate.cue_source_ids)?;
    validate_sorted_provenance_ids(&format!("{path}.source_ids"), &candidate.source_ids)?;
    let union = candidate
        .fact_source_ids
        .iter()
        .chain(&candidate.cue_source_ids)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if candidate.source_ids != union {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            format!("{path}.source_ids"),
            "candidate lineage must be the sorted union of fact and cue sources",
        ));
    }
    validate_sha256(
        &format!("{path}.tie_break_sha256"),
        &candidate.tie_break_sha256,
    )?;
    for (name, value) in [
        (
            "temporal_factor_micros",
            candidate.trace.temporal_factor_micros,
        ),
        ("place_factor_micros", candidate.trace.place_factor_micros),
        (
            "time_zone_factor_micros",
            candidate.trace.time_zone_factor_micros,
        ),
        (
            "uncertainty_factor_micros",
            candidate.trace.uncertainty_factor_micros,
        ),
        (
            "trait_coverage_micros",
            candidate.trace.trait_coverage_micros,
        ),
        (
            "cue_compatibility_micros",
            candidate.trace.cue_compatibility_micros,
        ),
    ] {
        validate_micros(&format!("{path}.trace.{name}"), value)?;
    }
    validate_public_text(
        &format!("{path}.trace.explanation"),
        &candidate.trace.explanation,
        1,
        2_048,
    )?;
    let mut prior_trait = None;
    for contribution in &candidate.trace.contributions {
        if prior_trait.is_some_and(|prior| prior >= contribution.trait_id) {
            return Err(invalid_value(
                format!("{path}.trace.contributions"),
                "ranking contributions must be unique and sorted by trait",
            ));
        }
        prior_trait = Some(contribution.trait_id);
        validate_micros(
            &format!("{path}.trace.contributions.profile_micros"),
            contribution.profile_micros,
        )?;
        validate_micros(
            &format!("{path}.trace.contributions.compatibility_micros"),
            contribution.compatibility_micros,
        )?;
        if !(-1_000..=1_000).contains(&contribution.cue_weight_thousandths)
            || contribution.cue_weight_thousandths == 0
        {
            return Err(invalid_value(
                format!("{path}.trace.contributions.cue_weight_thousandths"),
                "ranking contribution contains an invalid cue weight",
            ));
        }
    }
    Ok(())
}

/// Build a complete review, filling only explicitly eligible low-sensitivity auto decisions.
pub fn create_temporal_context_review(
    proposal: &TemporalContextProposal,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
    mut decisions: BTreeMap<String, TemporalReviewDecision>,
) -> Result<TemporalContextReview, CharacterError> {
    validate_proposal_structure(proposal)?;
    for candidate in &proposal.candidates {
        if decisions.contains_key(&candidate.id) {
            continue;
        }
        if candidate.auto_approve_eligible {
            let TemporalAutoApprovePolicy::LowSensitivity { policy_id, .. } =
                &proposal.config.auto_approve
            else {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    "config.auto_approve",
                    "candidate auto-review eligibility lacks an active policy",
                ));
            };
            decisions.insert(
                candidate.id.clone(),
                TemporalReviewDecision {
                    candidate_id: candidate.id.clone(),
                    action: TemporalReviewAction::AutoApprove {
                        policy_id: policy_id.clone(),
                    },
                },
            );
        }
    }
    let review = TemporalContextReview {
        review_format_version: TEMPORAL_CONTEXT_REVIEW_FORMAT_VERSION,
        proposal_sha256: temporal_proposal_fingerprint(proposal)?,
        profile_sha256: proposal.profile_sha256.clone(),
        reviewer: reviewer.into(),
        rationale: rationale.into(),
        decisions,
    };
    validate_temporal_context_review(&review, proposal)?;
    Ok(review)
}

/// Validate a complete review against the exact immutable proposal.
pub fn validate_temporal_context_review(
    review: &TemporalContextReview,
    proposal: &TemporalContextProposal,
) -> Result<(), CharacterError> {
    validate_review_structure(review)?;
    validate_proposal_structure(proposal)?;
    if review.proposal_sha256 != temporal_proposal_fingerprint(proposal)?
        || review.profile_sha256 != proposal.profile_sha256
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "review",
            "temporal review does not fingerprint the exact proposal and profile",
        ));
    }
    if review.decisions.len() != proposal.candidates.len() {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "decisions",
            "temporal review must decide every proposed candidate exactly once",
        ));
    }
    let candidates = proposal
        .candidates
        .iter()
        .map(|candidate| (candidate.id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    for (id, decision) in &review.decisions {
        let candidate = candidates.get(id.as_str()).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                format!("decisions.{id}"),
                "review decision references a candidate absent from the proposal",
            )
        })?;
        if decision.candidate_id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("decisions.{id}.candidate_id"),
                "decision candidate id must equal its containing map key",
            ));
        }
        validate_review_action(
            &format!("decisions.{id}.action"),
            &decision.action,
            candidate,
            &proposal.config,
        )?;
    }
    Ok(())
}

fn validate_review_structure(review: &TemporalContextReview) -> Result<(), CharacterError> {
    if review.review_format_version != TEMPORAL_CONTEXT_REVIEW_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "review_format_version",
            "unsupported temporal context review version",
        ));
    }
    validate_sha256("proposal_sha256", &review.proposal_sha256)?;
    validate_sha256("profile_sha256", &review.profile_sha256)?;
    validate_namespaced_id("reviewer", &review.reviewer)?;
    validate_public_text("rationale", &review.rationale, 1, 2_048)?;
    if review.decisions.len() > 16_384 {
        return Err(invalid_value(
            "decisions",
            "temporal review contains too many decisions",
        ));
    }
    for (id, decision) in &review.decisions {
        validate_local_id("decisions", id)?;
        if decision.candidate_id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("decisions.{id}.candidate_id"),
                "decision candidate id must equal its containing map key",
            ));
        }
        validate_review_action_text(&format!("decisions.{id}.action"), &decision.action)?;
    }
    Ok(())
}

fn validate_review_action(
    path: &str,
    action: &TemporalReviewAction,
    candidate: &TemporalContextCandidate,
    config: &TemporalContextConfig,
) -> Result<(), CharacterError> {
    validate_review_action_text(path, action)?;
    match action {
        TemporalReviewAction::Accept { rationale } => {
            if (candidate.requires_explicit_review
                || candidate.sensitivity != DateContextSensitivity::Low)
                && rationale.is_none()
            {
                return Err(invalid_value(
                    format!("{path}.rationale"),
                    "sensitive acceptance requires an explicit rationale",
                ));
            }
        }
        TemporalReviewAction::AutoApprove { policy_id } => {
            let TemporalAutoApprovePolicy::LowSensitivity {
                policy_id: expected,
                ..
            } = &config.auto_approve
            else {
                return Err(error(
                    CharacterDiagnosticCode::ForbiddenWriteBack,
                    path,
                    "auto-approval is disabled by the pinned configuration",
                ));
            };
            if policy_id != expected || !candidate.auto_approve_eligible {
                return Err(error(
                    CharacterDiagnosticCode::ForbiddenWriteBack,
                    path,
                    "candidate does not satisfy the exact low-sensitivity auto-review policy",
                ));
            }
        }
        TemporalReviewAction::Reject { .. }
        | TemporalReviewAction::Edit { .. }
        | TemporalReviewAction::Withhold { .. }
        | TemporalReviewAction::Override { .. } => {}
    }
    Ok(())
}

fn validate_review_action_text(
    path: &str,
    action: &TemporalReviewAction,
) -> Result<(), CharacterError> {
    match action {
        TemporalReviewAction::Accept {
            rationale: Some(rationale),
        }
        | TemporalReviewAction::Reject { rationale }
        | TemporalReviewAction::Withhold { rationale } => {
            validate_public_text(&format!("{path}.rationale"), rationale, 1, 2_048)
        }
        TemporalReviewAction::Accept { rationale: None } => Ok(()),
        TemporalReviewAction::Edit { content, rationale }
        | TemporalReviewAction::Override { content, rationale } => {
            validate_public_text(&format!("{path}.content"), content, 1, 2_048)?;
            validate_public_text(&format!("{path}.rationale"), rationale, 1, 2_048)
        }
        TemporalReviewAction::AutoApprove { policy_id } => {
            validate_namespaced_id(&format!("{path}.policy_id"), policy_id)
        }
    }
}

/// Reproduce and atomically apply reviewed cues, returning a self-validating receipt.
pub fn apply_reviewed_temporal_context(
    profile: &CharacterProfile,
    packs: &[TemporalContextPack],
    proposal: &TemporalContextProposal,
    review: &TemporalContextReview,
) -> Result<TemporalContextReceipt, CharacterError> {
    validate_temporal_context_proposal(proposal, profile, packs)?;
    validate_temporal_context_review(review, proposal)?;
    let output_profile = apply_temporal_profile(profile, packs, proposal, review)?;
    let receipt = TemporalContextReceipt {
        receipt_format_version: TEMPORAL_CONTEXT_RECEIPT_FORMAT_VERSION,
        input_profile: profile.clone(),
        packs: canonical_pack_order(packs)?,
        proposal: proposal.clone(),
        review: review.clone(),
        proposal_sha256: temporal_proposal_fingerprint(proposal)?,
        review_sha256: temporal_review_fingerprint(review)?,
        output_profile_sha256: temporal_profile_fingerprint(&output_profile)?,
        output_profile,
    };
    validate_temporal_context_receipt(&receipt)?;
    Ok(receipt)
}

/// Independently reproduce a serialized receipt and every fingerprint it retains.
pub fn validate_temporal_context_receipt(
    receipt: &TemporalContextReceipt,
) -> Result<(), CharacterError> {
    if receipt.receipt_format_version != TEMPORAL_CONTEXT_RECEIPT_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "receipt_format_version",
            "unsupported temporal context receipt version",
        ));
    }
    validate_profile(&receipt.input_profile)?;
    let canonical_packs = canonical_pack_order(&receipt.packs)?;
    if canonical_packs != receipt.packs {
        return Err(invalid_value(
            "packs",
            "receipt packs must use canonical identity and version order",
        ));
    }
    validate_temporal_context_proposal(&receipt.proposal, &receipt.input_profile, &receipt.packs)?;
    validate_temporal_context_review(&receipt.review, &receipt.proposal)?;
    for (path, actual, expected) in [
        (
            "proposal_sha256",
            receipt.proposal_sha256.as_str(),
            temporal_proposal_fingerprint(&receipt.proposal)?,
        ),
        (
            "review_sha256",
            receipt.review_sha256.as_str(),
            temporal_review_fingerprint(&receipt.review)?,
        ),
    ] {
        validate_sha256(path, actual)?;
        if actual != expected {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                path,
                "temporal receipt retains a stale fingerprint",
            ));
        }
    }
    let expected_output = apply_temporal_profile(
        &receipt.input_profile,
        &receipt.packs,
        &receipt.proposal,
        &receipt.review,
    )?;
    if receipt.output_profile != expected_output {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "output_profile",
            "temporal receipt output does not match independent reproduction",
        ));
    }
    validate_sha256("output_profile_sha256", &receipt.output_profile_sha256)?;
    if receipt.output_profile_sha256 != temporal_profile_fingerprint(&expected_output)? {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "output_profile_sha256",
            "temporal receipt output fingerprint is stale",
        ));
    }
    Ok(())
}

fn apply_temporal_profile(
    profile: &CharacterProfile,
    packs: &[TemporalContextPack],
    proposal: &TemporalContextProposal,
    review: &TemporalContextReview,
) -> Result<CharacterProfile, CharacterError> {
    validate_temporal_context_proposal(proposal, profile, packs)?;
    validate_temporal_context_review(review, proposal)?;
    validate_existing_context_lock(profile, &proposal.config)?;
    let review_sha256 = temporal_review_fingerprint(review)?;

    let accepted = proposal
        .candidates
        .iter()
        .filter_map(|candidate| {
            let decision = &review.decisions[&candidate.id].action;
            decision_content(decision, candidate).map(|(content, kind)| (candidate, content, kind))
        })
        .collect::<Vec<_>>();
    if accepted.is_empty() {
        return Err(invalid_value(
            "review.decisions",
            "applying temporal context requires at least one accepted cue",
        ));
    }

    let mut output = profile.clone();
    remove_managed_temporal_suggestions(&mut output)?;
    for pack in packs {
        output.provenance = merge_provenance(&output.provenance, &pack.provenance)?;
    }
    let transformation_id = format!("temporal_review_{}", &review_sha256[..16]);
    let inputs = accepted
        .iter()
        .flat_map(|(candidate, _, _)| candidate.source_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "review.decisions",
            "accepted temporal cues require public lineage",
        ));
    }
    if output
        .provenance
        .sources
        .iter()
        .any(|source| source.id == transformation_id)
        || output
            .provenance
            .transformations
            .iter()
            .any(|transformation| transformation.id == transformation_id)
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance.transformations",
            "temporal review transformation identity already exists",
        ));
    }
    output
        .provenance
        .transformations
        .push(ProvenanceTransformation {
            id: transformation_id.clone(),
            inputs,
            description: "Recorded explicit editorial decisions over non-causal fictional temporal authoring cues; no dated fact was treated as personality evidence or written into canon.".to_owned(),
        });
    output
        .provenance
        .transformations
        .sort_by(|left, right| left.id.cmp(&right.id));
    output
        .provenance
        .claims
        .entry(format!("extensions.{DATE_CONTEXT_EXTENSION_NAMESPACE}"))
        .or_default()
        .push(transformation_id.clone());
    sort_deduplicate_claims(&mut output.provenance);

    let mut accepted_cues = BTreeMap::new();
    let mut accepted_record_ids = BTreeSet::new();
    let mut extension_lineage = BTreeSet::from([transformation_id.clone()]);
    for (candidate, content, decision_kind) in accepted {
        accepted_record_ids.insert(candidate.record_id.clone());
        extension_lineage.extend(candidate.source_ids.iter().cloned());
        accepted_cues.insert(
            candidate.id.clone(),
            AcceptedDateContextCue {
                id: candidate.id.clone(),
                record_id: candidate.record_id.clone(),
                pack: candidate.pack.clone(),
                kind: candidate.kind,
                content: content.clone(),
                relevance: f64::from(candidate.relevance_micros) / SCORE_SCALE as f64,
                uncertainty: candidate.uncertainty,
                sensitivity: candidate.sensitivity,
                decision: decision_kind,
                fact_source_ids: candidate.fact_source_ids.clone(),
                cue_source_ids: candidate.cue_source_ids.clone(),
                source_ids: candidate.source_ids.clone(),
                review_sha256: review_sha256.clone(),
            },
        );

        let suggestion_id = temporal_suggestion_id(&candidate.id)?;
        if output.suggestions.contains_key(&suggestion_id) {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                format!("suggestions.{suggestion_id}"),
                "temporal application would replace an existing suggestion",
            ));
        }
        let target_root = if candidate.kind == DateContextCueKind::Voice {
            "canon.voice"
        } else {
            "canon.inner_life"
        };
        let suggestion_lineage = candidate
            .source_ids
            .iter()
            .cloned()
            .chain([transformation_id.clone()])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        output.suggestions.insert(
            suggestion_id.clone(),
            CharacterSuggestion {
                id: suggestion_id.clone(),
                target_path: format!("{target_root}.{}", candidate.id),
                proposal: Attributed {
                    value: DomainValue::String(content),
                    state: ValueState::Suggested,
                    confidence: confidence_from_uncertainty(candidate.uncertainty),
                    review: ReviewState::Pending,
                    lock: LockState::Unlocked,
                    freshness: Freshness::Current,
                    lineage: suggestion_lineage,
                    rationale: Some(TEMPORAL_SUGGESTION_RATIONALE.to_owned()),
                },
            },
        );
        output
            .provenance
            .claims
            .entry(format!("suggestions.{suggestion_id}"))
            .or_default()
            .push(transformation_id.clone());
    }
    sort_deduplicate_claims(&mut output.provenance);

    let primary = proposal.packs.first().ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "proposal.packs",
            "temporal proposal has no primary pack",
        )
    })?;
    let state = if profile
        .extensions
        .contains_key(DATE_CONTEXT_EXTENSION_NAMESPACE)
    {
        ValueState::Overridden
    } else {
        ValueState::Reviewed
    };
    output.extensions.insert(
        DATE_CONTEXT_EXTENSION_NAMESPACE.to_owned(),
        CharacterExtension::DateContext(VersionedExtension {
            header: ExtensionHeader {
                namespace: DATE_CONTEXT_EXTENSION_NAMESPACE.to_owned(),
                extension_version: 1,
                authority: review.reviewer.clone(),
                rationale: review.rationale.clone(),
                state,
                review: ReviewState::Accepted,
                lock: LockState::Unlocked,
                freshness: Freshness::Current,
                lineage: extension_lineage.into_iter().collect(),
                canonical_personality_write_back: ExtensionWriteBack::Forbidden,
            },
            value: DateContext {
                context_pack: primary.id.clone(),
                context_version: primary.version.clone(),
                context_hash: primary.sha256.clone(),
                additional_context_packs: proposal.packs[1..].to_vec(),
                accepted_record_ids: accepted_record_ids.into_iter().collect(),
                accepted_cues,
            },
        }),
    );
    validate_profile(&output)?;
    Ok(output)
}

fn remove_managed_temporal_suggestions(
    profile: &mut CharacterProfile,
) -> Result<(), CharacterError> {
    let Some(CharacterExtension::DateContext(extension)) =
        profile.extensions.get(DATE_CONTEXT_EXTENSION_NAMESPACE)
    else {
        return Ok(());
    };
    let expected = extension
        .value
        .accepted_cues
        .values()
        .map(|cue| {
            let suggestion_id = temporal_suggestion_id(&cue.id)?;
            let target_root = if cue.kind == DateContextCueKind::Voice {
                "canon.voice"
            } else {
                "canon.inner_life"
            };
            let transformation_id = format!("temporal_review_{}", &cue.review_sha256[..16]);
            let lineage = cue
                .source_ids
                .iter()
                .cloned()
                .chain([transformation_id])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            Ok((
                suggestion_id.clone(),
                CharacterSuggestion {
                    id: suggestion_id,
                    target_path: format!("{target_root}.{}", cue.id),
                    proposal: Attributed {
                        value: DomainValue::String(cue.content.clone()),
                        state: ValueState::Suggested,
                        confidence: confidence_from_uncertainty(cue.uncertainty),
                        review: ReviewState::Pending,
                        lock: LockState::Unlocked,
                        freshness: Freshness::Current,
                        lineage,
                        rationale: Some(TEMPORAL_SUGGESTION_RATIONALE.to_owned()),
                    },
                },
            ))
        })
        .collect::<Result<Vec<_>, CharacterError>>()?;
    for (id, expected_suggestion) in expected {
        if let Some(existing) = profile.suggestions.get(&id)
            && existing != &expected_suggestion
        {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                format!("suggestions.{id}"),
                "an existing temporal suggestion was independently changed",
            ));
        }
        profile.suggestions.remove(&id);
        profile
            .provenance
            .claims
            .remove(&format!("suggestions.{id}"));
    }
    Ok(())
}

fn temporal_suggestion_id(candidate_id: &str) -> Result<String, CharacterError> {
    validate_candidate_id("candidate_id", candidate_id)?;
    Ok(format!(
        "date_context_{}",
        candidate_id
            .strip_prefix("cue_")
            .expect("validated temporal candidate ids have a cue_ prefix")
    ))
}

fn decision_content(
    action: &TemporalReviewAction,
    candidate: &TemporalContextCandidate,
) -> Option<(String, DateContextDecision)> {
    match action {
        TemporalReviewAction::Accept { .. } => {
            Some((candidate.content.clone(), DateContextDecision::Accepted))
        }
        TemporalReviewAction::Edit { content, .. } => {
            Some((content.clone(), DateContextDecision::Edited))
        }
        TemporalReviewAction::Override { content, .. } => {
            Some((content.clone(), DateContextDecision::Overridden))
        }
        TemporalReviewAction::AutoApprove { .. } => {
            Some((candidate.content.clone(), DateContextDecision::AutoApproved))
        }
        TemporalReviewAction::Reject { .. } | TemporalReviewAction::Withhold { .. } => None,
    }
}

fn validate_existing_context_lock(
    profile: &CharacterProfile,
    config: &TemporalContextConfig,
) -> Result<(), CharacterError> {
    let Some(existing) = profile.extensions.get(DATE_CONTEXT_EXTENSION_NAMESPACE) else {
        return Ok(());
    };
    let CharacterExtension::DateContext(existing) = existing else {
        return Err(error(
            CharacterDiagnosticCode::InvalidExtension,
            format!("extensions.{DATE_CONTEXT_EXTENSION_NAMESPACE}"),
            "reserved date-context namespace contains another extension kind",
        ));
    };
    if existing.header.lock == LockState::Locked && !config.override_locked_context {
        return Err(error(
            CharacterDiagnosticCode::LockedField,
            format!("extensions.{DATE_CONTEXT_EXTENSION_NAMESPACE}.header.lock"),
            "locked date context requires an explicit configured override and rationale",
        ));
    }
    Ok(())
}

fn canonical_pack_order(
    packs: &[TemporalContextPack],
) -> Result<Vec<TemporalContextPack>, CharacterError> {
    if packs.is_empty() || packs.len() > 4_096 {
        return Err(invalid_value(
            "packs",
            "expected a bounded non-empty temporal pack set",
        ));
    }
    let mut values = packs.to_vec();
    for pack in &values {
        validate_temporal_context_pack(pack)?;
    }
    values.sort_by(|left, right| {
        (&left.id, &left.version)
            .cmp(&(&right.id, &right.version))
            .then_with(|| left.title.cmp(&right.title))
    });
    if values.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "packs",
            "one temporal pack identity cannot appear more than once",
        ));
    }
    Ok(values)
}

fn sort_deduplicate_claims(provenance: &mut Provenance) {
    for references in provenance.claims.values_mut() {
        references.sort();
        references.dedup();
    }
}

fn confidence_from_uncertainty(value: DateContextUncertainty) -> Confidence {
    match value {
        DateContextUncertainty::Exact => Confidence::High,
        DateContextUncertainty::Bounded => Confidence::Moderate,
        DateContextUncertainty::Disputed => Confidence::Low,
    }
}

fn candidate_order(
    left: &TemporalContextCandidate,
    right: &TemporalContextCandidate,
) -> std::cmp::Ordering {
    right
        .relevance_micros
        .cmp(&left.relevance_micros)
        .then_with(|| left.tie_break_sha256.cmp(&right.tie_break_sha256))
        .then_with(|| left.id.cmp(&right.id))
}

fn candidate_id(pack_id: &str, record_id: &str, cue_id: &str) -> String {
    let digest = sha256(format!("{pack_id}\0{record_id}\0{cue_id}").as_bytes());
    format!("cue_{}", &digest[..24])
}

fn required_tags_match(required: &[String], record: &[String], cue: &[String]) -> bool {
    required.iter().all(|required| {
        record.binary_search(required).is_ok() || cue.binary_search(required).is_ok()
    })
}

fn match_birth_date(birth: &BirthDate, extent: &TemporalExtent) -> Option<TemporalMatchKind> {
    match (birth, extent) {
        (BirthDate::Year { year, .. }, TemporalExtent::Year { year: record }) if year == record => {
            Some(TemporalMatchKind::SameYear)
        }
        (BirthDate::Year { year, .. }, TemporalExtent::Date { date }) if year == &date.year => {
            Some(TemporalMatchKind::SameYear)
        }
        (
            BirthDate::Year { year, .. },
            TemporalExtent::YearRange {
                start_year,
                end_year,
            },
        ) if (start_year..=end_year).contains(&year) => Some(TemporalMatchKind::ContainingPeriod),
        (BirthDate::Year { year, .. }, TemporalExtent::DateRange { start, end })
            if (start.year..=end.year).contains(year) =>
        {
            Some(TemporalMatchKind::ContainingPeriod)
        }
        (
            BirthDate::MonthDay { month, day, .. },
            TemporalExtent::MonthDay {
                month: record_month,
                day: record_day,
            },
        ) if month == record_month && day == record_day => {
            Some(TemporalMatchKind::RecurringMonthDay)
        }
        (
            BirthDate::Full {
                year, month, day, ..
            },
            TemporalExtent::Date { date },
        ) if year == &date.year && month == &date.month && day == &date.day => {
            Some(TemporalMatchKind::ExactDate)
        }
        (
            BirthDate::Full { month, day, .. },
            TemporalExtent::MonthDay {
                month: record_month,
                day: record_day,
            },
        ) if month == record_month && day == record_day => {
            Some(TemporalMatchKind::RecurringMonthDay)
        }
        (BirthDate::Full { year, .. }, TemporalExtent::Year { year: record }) if year == record => {
            Some(TemporalMatchKind::SameYear)
        }
        (
            BirthDate::Full { year, .. },
            TemporalExtent::YearRange {
                start_year,
                end_year,
            },
        ) if (start_year..=end_year).contains(&year) => Some(TemporalMatchKind::ContainingPeriod),
        (
            BirthDate::Full {
                year, month, day, ..
            },
            TemporalExtent::DateRange { start, end },
        ) => {
            let birth = TemporalDate {
                calendar: Calendar::ProlepticGregorian,
                year: *year,
                month: *month,
                day: *day,
            };
            (start <= &birth && &birth <= end).then_some(TemporalMatchKind::ContainingPeriod)
        }
        _ => None,
    }
}

const fn temporal_factor(value: TemporalMatchKind) -> u32 {
    match value {
        TemporalMatchKind::ExactDate => 1_000_000,
        TemporalMatchKind::RecurringMonthDay => 925_000,
        TemporalMatchKind::SameYear => 825_000,
        TemporalMatchKind::ContainingPeriod => 725_000,
    }
}

const fn uncertainty_factor(value: DateContextUncertainty) -> u32 {
    match value {
        DateContextUncertainty::Exact => 1_000_000,
        DateContextUncertainty::Bounded => 750_000,
        DateContextUncertainty::Disputed => 500_000,
    }
}

fn place_factor(record: &TemporalContextRecord, config: &TemporalContextConfig) -> Option<u32> {
    match &record.place_scope {
        TemporalPlaceScope::Global if config.allow_global_without_place => Some(950_000),
        TemporalPlaceScope::Global if !config.place_scope_ids.is_empty() => Some(900_000),
        TemporalPlaceScope::Global => None,
        TemporalPlaceScope::Place { id, .. }
            if config.place_scope_ids.binary_search(id).is_ok() =>
        {
            Some(1_000_000)
        }
        TemporalPlaceScope::Place { .. } => None,
    }
}

fn time_zone_factor(record: &TemporalTimeZone, configured: &TemporalTimeZone) -> u32 {
    if record == configured || matches!(record, TemporalTimeZone::Utc) {
        1_000_000
    } else if matches!(record, TemporalTimeZone::Unknown { .. })
        || matches!(configured, TemporalTimeZone::Unknown { .. })
    {
        750_000
    } else {
        // A mismatch reduces contextual relevance but never shifts a date-only value.
        850_000
    }
}

fn auto_approve_eligible(
    config: &TemporalContextConfig,
    kind: DateContextCueKind,
    sensitivity: DateContextSensitivity,
    requires_explicit_review: bool,
    relevance_micros: u32,
) -> bool {
    let TemporalAutoApprovePolicy::LowSensitivity {
        minimum_relevance_micros,
        cue_kinds,
        ..
    } = &config.auto_approve
    else {
        return false;
    };
    sensitivity == DateContextSensitivity::Low
        && !requires_explicit_review
        && relevance_micros >= *minimum_relevance_micros
        && cue_kinds.binary_search(&kind).is_ok()
}

fn score_trait_vector(
    profile: &CharacterProfile,
    vector: &BTreeMap<HexacoTrait, i16>,
) -> (u32, u32, Vec<TemporalRankingContribution>) {
    let total_weight = vector
        .values()
        .map(|weight| i64::from(weight.unsigned_abs()))
        .sum::<i64>();
    let mut covered_weight = 0_i64;
    let mut weighted_compatibility = 0_i128;
    let mut contributions = Vec::new();
    for (trait_id, weight) in vector {
        let Some(value) = trait_value(&profile.canon.personality, *trait_id) else {
            continue;
        };
        let profile_micros = measurement_micros(value.value);
        let compatibility_micros = if *weight > 0 {
            profile_micros
        } else {
            SCORE_SCALE as u32 - profile_micros
        };
        let magnitude = i64::from(weight.unsigned_abs());
        covered_weight += magnitude;
        weighted_compatibility += i128::from(compatibility_micros) * i128::from(magnitude);
        contributions.push(TemporalRankingContribution {
            trait_id: *trait_id,
            profile_micros,
            cue_weight_thousandths: *weight,
            compatibility_micros,
            weighted_contribution_micros: i64::from(compatibility_micros) * magnitude / 1_000,
        });
    }
    let coverage = if total_weight == 0 {
        0
    } else {
        u32::try_from((covered_weight * SCORE_SCALE + total_weight / 2) / total_weight).unwrap_or(0)
    };
    let compatibility = if covered_weight == 0 {
        500_000
    } else {
        u32::try_from(
            (weighted_compatibility + i128::from(covered_weight / 2)) / i128::from(covered_weight),
        )
        .unwrap_or(500_000)
    };
    (coverage, compatibility, contributions)
}

fn measurement_micros(value: TraitMeasurement) -> u32 {
    let score = match value {
        TraitMeasurement::Score { score } => score,
        TraitMeasurement::Band { band } => band.projection_anchor(),
    };
    (score * SCORE_SCALE as f64).round() as u32
}

fn multiply_factors(values: &[u32]) -> u32 {
    values.iter().fold(SCORE_SCALE as u32, |result, factor| {
        let product = u64::from(result) * u64::from(*factor);
        u32::try_from((product + SCORE_SCALE as u64 / 2) / SCORE_SCALE as u64)
            .unwrap_or(SCORE_SCALE as u32)
    })
}

fn validate_extent(path: &str, extent: &TemporalExtent) -> Result<(), CharacterError> {
    match extent {
        TemporalExtent::Year { year } => validate_year(path, *year),
        TemporalExtent::MonthDay { month, day } => validate_month_day(path, None, *month, *day),
        TemporalExtent::Date { date } => validate_temporal_date(path, date),
        TemporalExtent::YearRange {
            start_year,
            end_year,
        } => {
            validate_year(path, *start_year)?;
            validate_year(path, *end_year)?;
            if start_year > end_year {
                return Err(invalid_value(path, "year range must be ordered"));
            }
            Ok(())
        }
        TemporalExtent::DateRange { start, end } => {
            validate_temporal_date(&format!("{path}.start"), start)?;
            validate_temporal_date(&format!("{path}.end"), end)?;
            if start > end {
                return Err(invalid_value(path, "date range must be ordered"));
            }
            Ok(())
        }
    }
}

fn validate_reference_period(
    path: &str,
    period: &TemporalReferencePeriod,
) -> Result<(), CharacterError> {
    validate_public_text(&format!("{path}.label"), &period.label, 1, 256)?;
    validate_temporal_date(&format!("{path}.start"), &period.start)?;
    validate_temporal_date(&format!("{path}.end"), &period.end)?;
    if period.start > period.end {
        return Err(invalid_value(path, "reference period must be ordered"));
    }
    Ok(())
}

fn validate_birth_date(path: &str, value: &BirthDate) -> Result<(), CharacterError> {
    match value {
        BirthDate::Year { calendar, year } => {
            validate_calendar(path, *calendar)?;
            validate_year(path, *year)
        }
        BirthDate::MonthDay {
            calendar,
            month,
            day,
        } => {
            validate_calendar(path, *calendar)?;
            validate_month_day(path, None, *month, *day)
        }
        BirthDate::Full {
            calendar,
            year,
            month,
            day,
        } => {
            validate_calendar(path, *calendar)?;
            validate_year(path, *year)?;
            validate_month_day(path, Some(*year), *month, *day)
        }
    }
}

fn validate_temporal_date(path: &str, value: &TemporalDate) -> Result<(), CharacterError> {
    validate_calendar(path, value.calendar)?;
    validate_year(path, value.year)?;
    validate_month_day(path, Some(value.year), value.month, value.day)
}

fn validate_calendar(path: &str, value: Calendar) -> Result<(), CharacterError> {
    if value != Calendar::ProlepticGregorian {
        return Err(invalid_value(
            format!("{path}.calendar"),
            "temporal context v1 uses the proleptic Gregorian calendar",
        ));
    }
    Ok(())
}

fn validate_year(path: &str, year: i32) -> Result<(), CharacterError> {
    if !(1..=9_999).contains(&year) {
        return Err(invalid_value(
            format!("{path}.year"),
            "year must be from 1 through 9999",
        ));
    }
    Ok(())
}

fn validate_month_day(
    path: &str,
    year: Option<i32>,
    month: u8,
    day: u8,
) -> Result<(), CharacterError> {
    let leap = year.is_none_or(is_leap_year);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if days == 0 || day == 0 || day > days {
        return Err(invalid_value(path, "date contains an invalid month or day"));
    }
    Ok(())
}

const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn validate_time_zone(path: &str, value: &TemporalTimeZone) -> Result<(), CharacterError> {
    match value {
        TemporalTimeZone::Utc => Ok(()),
        TemporalTimeZone::Iana { id } => {
            if id.len() > 128
                || !id.contains('/')
                || id.starts_with('/')
                || id.ends_with('/')
                || id.split('/').any(|segment| {
                    segment.is_empty()
                        || !segment.chars().all(|character| {
                            character.is_ascii_alphanumeric()
                                || matches!(character, '_' | '-' | '+')
                        })
                })
            {
                return Err(invalid_value(
                    format!("{path}.id"),
                    "expected a bounded IANA time-zone identifier",
                ));
            }
            Ok(())
        }
        TemporalTimeZone::Unknown { reason } => {
            validate_public_text(&format!("{path}.reason"), reason, 1, 512)
        }
    }
}

fn validate_pack_ref(path: &str, value: &DateContextPackRef) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

fn validate_lineage(
    path: &str,
    values: &[String],
    lineage: &BTreeSet<&str>,
) -> Result<(), CharacterError> {
    if values.is_empty() || values.len() > 4_096 {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            path,
            "value requires at least one bounded lineage reference",
        ));
    }
    let mut prior = None;
    for value in values {
        if prior.is_some_and(|prior: &str| prior >= value.as_str())
            || !lineage.contains(value.as_str())
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                path,
                "lineage references must be declared, unique, and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn provenance_ids(provenance: &Provenance) -> BTreeSet<&str> {
    provenance
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .chain(
            provenance
                .transformations
                .iter()
                .map(|transformation| transformation.id.as_str()),
        )
        .collect()
}

fn validate_domain_value(path: &str, value: &DomainValue) -> Result<(), CharacterError> {
    fn walk(value: &DomainValue, depth: usize, nodes: &mut usize) -> bool {
        *nodes += 1;
        if depth > MAX_DOMAIN_DEPTH || *nodes > MAX_DOMAIN_NODES {
            return false;
        }
        match value {
            DomainValue::Null | DomainValue::Bool(_) => true,
            DomainValue::Number(number) => number.is_finite(),
            DomainValue::String(value) | DomainValue::Symbol(value) => {
                value.len() <= MAX_TEXT && !contains_secret_shape(value)
            }
            DomainValue::List(values) => {
                values.len() <= MAX_RECORDS
                    && values.iter().all(|value| walk(value, depth + 1, nodes))
            }
            DomainValue::Object(values) => {
                values.len() <= MAX_RECORDS
                    && values
                        .iter()
                        .all(|(key, value)| valid_local_id(key) && walk(value, depth + 1, nodes))
            }
        }
    }
    let mut nodes = 0;
    if !walk(value, 0, &mut nodes) {
        return Err(invalid_value(
            path,
            "temporal fact exceeds portable public value bounds",
        ));
    }
    Ok(())
}

fn validate_sorted_enum<T: Ord>(path: &str, values: &[T]) -> Result<(), CharacterError> {
    if values.len() > 4_096 || values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid_value(
            path,
            "values must be bounded, unique, and sorted",
        ));
    }
    Ok(())
}

fn validate_sorted_tags(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if values.len() > 4_096 {
        return Err(invalid_value(path, "too many tag values"));
    }
    let mut prior = None;
    for value in values {
        validate_local_id(path, value)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_value(path, "tags must be unique and sorted"));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_sorted_namespaced_ids(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if values.len() > 4_096 {
        return Err(invalid_value(path, "too many identifier values"));
    }
    let mut prior = None;
    for value in values {
        validate_namespaced_id(path, value)?;
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_value(path, "identifiers must be unique and sorted"));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_sorted_provenance_ids(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if values.is_empty() || values.len() > 4_096 {
        return Err(invalid_value(
            path,
            "expected a bounded non-empty provenance identifier set",
        ));
    }
    let mut prior = None;
    for value in values {
        if value.len() > 256 || value.split('.').any(|segment| !valid_local_id(segment)) {
            return Err(error(
                CharacterDiagnosticCode::InvalidIdentifier,
                path,
                "expected a stable provenance identifier",
            ));
        }
        if prior.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid_value(
                path,
                "provenance identifiers must be unique and sorted",
            ));
        }
        prior = Some(value.as_str());
    }
    Ok(())
}

fn validate_namespaced_id(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.len() > 256
        || value.split('.').count() < 2
        || value.split('.').any(|part| !valid_local_id(part))
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a dot-separated lowercase stable identifier",
        ));
    }
    Ok(())
}

fn validate_local_id(path: &str, value: &str) -> Result<(), CharacterError> {
    if !valid_local_id(value) {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a lowercase stable identifier",
        ));
    }
    Ok(())
}

fn validate_candidate_id(path: &str, value: &str) -> Result<(), CharacterError> {
    let digest = value.strip_prefix("cue_").unwrap_or_default();
    if digest.len() != 24
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a stable temporal candidate identifier",
        ));
    }
    Ok(())
}

fn valid_local_id(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && value.len() <= 128
        && !value.ends_with('_')
        && !value.contains("__")
}

fn validate_semver(path: &str, value: &str) -> Result<(), CharacterError> {
    Version::parse(value)
        .map(|_| ())
        .map_err(|_| invalid_value(path, "expected a semantic version"))
}

fn validate_sha256(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid_value(path, "expected a lowercase SHA-256"));
    }
    Ok(())
}

fn validate_micros(path: &str, value: u32) -> Result<(), CharacterError> {
    if value > SCORE_SCALE as u32 {
        return Err(invalid_value(
            path,
            "fixed-point value must be from zero through one million",
        ));
    }
    Ok(())
}

fn validate_license(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric()
                || matches!(character, '-' | '.' | '+' | '(' | ')' | ' '))
        })
    {
        return Err(invalid_value(
            path,
            "expected a bounded SPDX license expression",
        ));
    }
    Ok(())
}

fn validate_https(path: &str, value: &str) -> Result<(), CharacterError> {
    if !value.starts_with("https://")
        || value.len() > 2_048
        || value.chars().any(char::is_whitespace)
        || value.contains('@')
        || contains_secret_shape(value)
    {
        return Err(invalid_value(
            path,
            "expected a public credential-free HTTPS URL",
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
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length)
        || value.contains('\0')
        || contains_secret_shape(value)
    {
        return Err(invalid_value(
            path,
            "public text length or contents are invalid",
        ));
    }
    Ok(())
}

fn contains_secret_shape(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    [
        "-----begin private key-----",
        "authorization: bearer ",
        "github_pat_",
        "ghp_",
        "xoxb-",
        "postgres://",
        "postgresql://",
        "mysql://",
        "mongodb+srv://",
        "aws_secret_access_key",
        "client_secret=",
        "api_key=",
        "apikey=",
        "password=",
    ]
    .iter()
    .any(|marker| lowercase.contains(marker))
}

fn temporal_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, CharacterError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-temporal-version".to_owned(),
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
    Ok(sha256(bytes.as_bytes()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn encoding_error() -> CharacterError {
    error(
        CharacterDiagnosticCode::InvalidEncoding,
        "temporal_context",
        "could not encode the temporal context document",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_domain::{ProvenanceKind, ProvenanceSource};

    const PROFILE: &str =
        include_str!("../../../examples/domain-modules/weave-character/profile.character.json");

    fn pack() -> TemporalContextPack {
        TemporalContextPack {
            pack_format_version: TEMPORAL_CONTEXT_PACK_FORMAT_VERSION,
            id: "org.weave.context.synthetic_calendar".to_owned(),
            version: "1.0.0".to_owned(),
            title: "Synthetic calendar cues".to_owned(),
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            provider: TemporalContextProvider::Standalone,
            records: BTreeMap::from([(
                "recurring_third_month_day".to_owned(),
                TemporalContextRecord {
                    id: "recurring_third_month_day".to_owned(),
                    kind: TemporalRecordKind::CalendricalFact,
                    evidence_class: TemporalEvidenceClass::CalendricalFact,
                    extent: TemporalExtent::MonthDay { month: 3, day: 14 },
                    place_scope: TemporalPlaceScope::Global,
                    time_zone: TemporalTimeZone::Utc,
                    reference_period: TemporalReferencePeriod {
                        label: "Synthetic recurring calendar reference".to_owned(),
                        start: TemporalDate {
                            calendar: Calendar::ProlepticGregorian,
                            year: 1,
                            month: 1,
                            day: 1,
                        },
                        end: TemporalDate {
                            calendar: Calendar::ProlepticGregorian,
                            year: 9_999,
                            month: 12,
                            day: 31,
                        },
                        resolution: TemporalResolution::RecurringDay,
                    },
                    fact: DomainValue::Object(BTreeMap::from([(
                        "calendar_label".to_owned(),
                        DomainValue::String("third month, fourteenth day".to_owned()),
                    )])),
                    tags: vec!["calendar".to_owned()],
                    cues: BTreeMap::from([(
                        "patient_observation".to_owned(),
                        TemporalAuthoringCue {
                            id: "patient_observation".to_owned(),
                            kind: DateContextCueKind::Memory,
                            content: "Imagine a recurring observance built around patient measurement and shared wonder.".to_owned(),
                            trait_vector: BTreeMap::from([
                                (HexacoTrait::Openness, 700),
                                (HexacoTrait::Patience, 300),
                            ]),
                            tags: vec!["observance".to_owned()],
                            limitations: "This original fictional cue is not caused by the date and does not characterize any real person.".to_owned(),
                            source_ids: vec!["temporal_original".to_owned()],
                        },
                    )]),
                    source_ids: vec!["temporal_original".to_owned()],
                    uncertainty: TemporalUncertainty {
                        level: DateContextUncertainty::Exact,
                        reason: "The recurring month and day are explicit synthetic fixture data."
                            .to_owned(),
                    },
                    sensitivity: TemporalSensitivity {
                        level: DateContextSensitivity::Low,
                        topics: Vec::new(),
                        requires_explicit_review: false,
                        note: "Neutral fictional authoring prompt.".to_owned(),
                    },
                },
            )]),
            provenance: Provenance {
                sources: vec![ProvenanceSource {
                    id: "temporal_original".to_owned(),
                    kind: ProvenanceKind::Original,
                    url: "https://github.com/chrisgliddon/weave".to_owned(),
                    revision: "temporal-context-v1".to_owned(),
                    sha256: None,
                    license: "MIT".to_owned(),
                    license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE"
                        .to_owned(),
                    attribution: "Original synthetic Weave temporal-context fixture.".to_owned(),
                    modified: false,
                }],
                transformations: Vec::new(),
                claims: BTreeMap::from([(
                    "records.recurring_third_month_day".to_owned(),
                    vec!["temporal_original".to_owned()],
                )]),
            },
        }
    }

    fn config() -> TemporalContextConfig {
        TemporalContextConfig {
            config_format_version: TEMPORAL_CONTEXT_CONFIG_FORMAT_VERSION,
            id: "org.weave.context.default_ranking".to_owned(),
            minimum_relevance_micros: 0,
            minimum_trait_coverage_micros: 1,
            maximum_candidates: 8,
            allowed_record_kinds: vec![TemporalRecordKind::CalendricalFact],
            allowed_cue_kinds: vec![DateContextCueKind::Memory],
            allowed_sensitivities: vec![DateContextSensitivity::Low],
            required_tags: Vec::new(),
            place_scope_ids: Vec::new(),
            time_zone: TemporalTimeZone::Utc,
            allow_global_without_place: true,
            auto_approve: TemporalAutoApprovePolicy::Disabled,
            override_locked_context: false,
            override_rationale: None,
        }
    }

    #[test]
    fn proposal_review_apply_and_receipt_round_trip_deterministically() {
        let profile = CharacterProfile::from_json(PROFILE).unwrap();
        let pack = pack();
        let config = config();
        let first =
            propose_temporal_context(&profile, std::slice::from_ref(&pack), &config, 41).unwrap();
        let second =
            propose_temporal_context(&profile, std::slice::from_ref(&pack), &config, 41).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.candidates.len(), 1);
        assert_eq!(
            first.candidates[0].trace.match_kind,
            TemporalMatchKind::RecurringMonthDay
        );

        let id = first.candidates[0].id.clone();
        let review = create_temporal_context_review(
            &first,
            "org.weave.reviewer.test",
            "Accept one fictional cue after inspecting its non-causal explanation.",
            BTreeMap::from([(
                id.clone(),
                TemporalReviewDecision {
                    candidate_id: id,
                    action: TemporalReviewAction::Accept { rationale: None },
                },
            )]),
        )
        .unwrap();
        let receipt =
            apply_reviewed_temporal_context(&profile, std::slice::from_ref(&pack), &first, &review)
                .unwrap();
        assert_eq!(
            receipt,
            TemporalContextReceipt::from_json(&receipt.to_json().unwrap()).unwrap()
        );
        assert_eq!(
            receipt,
            TemporalContextReceipt::from_ron(&receipt.to_ron().unwrap()).unwrap()
        );
        assert!(
            receipt
                .output_profile
                .extensions
                .contains_key(DATE_CONTEXT_EXTENSION_NAMESPACE)
        );
        assert_eq!(
            receipt.output_profile.suggestions.len(),
            profile.suggestions.len() + 1
        );
        let mut inconsistent = receipt.output_profile.clone();
        let CharacterExtension::DateContext(context) = inconsistent
            .extensions
            .get_mut(DATE_CONTEXT_EXTENSION_NAMESPACE)
            .unwrap()
        else {
            panic!("reviewed temporal output must retain typed date context");
        };
        context
            .value
            .accepted_record_ids
            .push("zz_orphan_record".to_owned());
        assert!(validate_profile(&inconsistent).is_err());

        let refreshed = propose_temporal_context(
            &receipt.output_profile,
            std::slice::from_ref(&pack),
            &config,
            41,
        )
        .unwrap();
        let refreshed_id = refreshed.candidates[0].id.clone();
        let refreshed_review = create_temporal_context_review(
            &refreshed,
            "org.weave.reviewer.test",
            "Refresh one reviewed fictional cue against the new exact profile fingerprint.",
            BTreeMap::from([(
                refreshed_id.clone(),
                TemporalReviewDecision {
                    candidate_id: refreshed_id,
                    action: TemporalReviewAction::Accept { rationale: None },
                },
            )]),
        )
        .unwrap();
        let refreshed_receipt = apply_reviewed_temporal_context(
            &receipt.output_profile,
            std::slice::from_ref(&pack),
            &refreshed,
            &refreshed_review,
        )
        .unwrap();
        assert_eq!(
            refreshed_receipt.output_profile.suggestions.len(),
            receipt.output_profile.suggestions.len()
        );

        let mut independently_changed = receipt.output_profile.clone();
        let managed = independently_changed
            .suggestions
            .values_mut()
            .find(|suggestion| suggestion.id.starts_with("date_context_"))
            .unwrap();
        managed.proposal.value = DomainValue::String(
            "An independently changed suggestion must never be overwritten.".to_owned(),
        );
        let changed_proposal = propose_temporal_context(
            &independently_changed,
            std::slice::from_ref(&pack),
            &config,
            41,
        )
        .unwrap();
        let changed_id = changed_proposal.candidates[0].id.clone();
        let changed_review = create_temporal_context_review(
            &changed_proposal,
            "org.weave.reviewer.test",
            "Attempt a refresh without replacing independent suggestion edits.",
            BTreeMap::from([(
                changed_id.clone(),
                TemporalReviewDecision {
                    candidate_id: changed_id,
                    action: TemporalReviewAction::Accept { rationale: None },
                },
            )]),
        )
        .unwrap();
        assert!(
            apply_reviewed_temporal_context(
                &independently_changed,
                &[pack],
                &changed_proposal,
                &changed_review,
            )
            .is_err()
        );
    }

    #[test]
    fn month_day_precision_never_matches_one_time_events() {
        let birth = BirthDate::MonthDay {
            calendar: Calendar::ProlepticGregorian,
            month: 7,
            day: 20,
        };
        let event = TemporalExtent::Date {
            date: TemporalDate {
                calendar: Calendar::ProlepticGregorian,
                year: 1969,
                month: 7,
                day: 20,
            },
        };
        assert_eq!(match_birth_date(&birth, &event), None);
        assert_eq!(
            match_birth_date(&birth, &TemporalExtent::MonthDay { month: 7, day: 20 }),
            Some(TemporalMatchKind::RecurringMonthDay)
        );
    }

    #[test]
    fn year_and_full_date_precision_match_only_facts_they_actually_know() {
        let event = TemporalExtent::Date {
            date: TemporalDate {
                calendar: Calendar::ProlepticGregorian,
                year: 1969,
                month: 7,
                day: 20,
            },
        };
        assert_eq!(
            match_birth_date(
                &BirthDate::Year {
                    calendar: Calendar::ProlepticGregorian,
                    year: 1969,
                },
                &event,
            ),
            Some(TemporalMatchKind::SameYear)
        );
        assert_eq!(
            match_birth_date(
                &BirthDate::Full {
                    calendar: Calendar::ProlepticGregorian,
                    year: 1969,
                    month: 7,
                    day: 20,
                },
                &event,
            ),
            Some(TemporalMatchKind::ExactDate)
        );
        assert_eq!(
            match_birth_date(
                &BirthDate::Full {
                    calendar: Calendar::ProlepticGregorian,
                    year: 1969,
                    month: 7,
                    day: 21,
                },
                &event,
            ),
            None
        );
    }

    #[test]
    fn incomplete_stale_and_secret_shaped_inputs_fail_closed() {
        let profile = CharacterProfile::from_json(PROFILE).unwrap();
        let context_pack = pack();
        let proposal =
            propose_temporal_context(&profile, std::slice::from_ref(&context_pack), &config(), 8)
                .unwrap();
        assert!(
            create_temporal_context_review(
                &proposal,
                "org.weave.reviewer.test",
                "Review every proposed cue.",
                BTreeMap::new(),
            )
            .is_err()
        );

        let mut stale = proposal.clone();
        stale.profile_sha256 = "0".repeat(64);
        assert!(validate_temporal_context_proposal(&stale, &profile, &[context_pack]).is_err());

        let mut unsafe_pack = pack();
        unsafe_pack.title = "Authorization: Bearer sensitive-value".to_owned();
        let error = validate_temporal_context_pack(&unsafe_pack).unwrap_err();
        assert!(!error.to_string().contains("sensitive-value"));
    }

    #[test]
    fn domain_provider_hash_pins_fact_projection_without_absorbing_fictional_cues() {
        let mut projected = pack();
        let content_sha256 = temporal_provider_content_fingerprint(&projected.records).unwrap();
        projected.provider = TemporalContextProvider::DomainModule {
            module_id: "org.weave.world".to_owned(),
            module_version: "1.0.0".to_owned(),
            pack_id: "synthetic_projection".to_owned(),
            pack_version: "1.0.0".to_owned(),
            content_sha256: content_sha256.clone(),
        };
        validate_temporal_context_pack(&projected).unwrap();

        projected
            .records
            .values_mut()
            .next()
            .unwrap()
            .cues
            .values_mut()
            .next()
            .unwrap()
            .content = "A separately authored fictional cue can change independently.".to_owned();
        assert_eq!(
            temporal_provider_content_fingerprint(&projected.records).unwrap(),
            content_sha256
        );
        validate_temporal_context_pack(&projected).unwrap();

        projected.records.values_mut().next().unwrap().fact =
            DomainValue::String("A changed provider fact requires a new content hash.".to_owned());
        assert!(validate_temporal_context_pack(&projected).is_err());
    }

    #[test]
    fn leap_day_validation_uses_proleptic_gregorian_rules() {
        assert!(
            validate_temporal_date(
                "date",
                &TemporalDate {
                    calendar: Calendar::ProlepticGregorian,
                    year: 2000,
                    month: 2,
                    day: 29,
                }
            )
            .is_ok()
        );
        assert!(
            validate_temporal_date(
                "date",
                &TemporalDate {
                    calendar: Calendar::ProlepticGregorian,
                    year: 1900,
                    month: 2,
                    day: 29,
                }
            )
            .is_err()
        );
    }
}
