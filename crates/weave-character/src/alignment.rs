//! Declarative, explainable narrative alignment views for fictional character authoring.
//!
//! Alignment packs read existing canonical HEXACO evidence through explicit weighted axes. They
//! produce reviewable storytelling shorthand, never diagnostic evidence, canonical personality
//! mutations, or runtime authority.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    Provenance, ProvenanceTransformation, parse_strict_json, to_pretty_json, to_pretty_ron,
    validate_provenance,
};

use crate::model::{
    AlignmentPackRef, AlignmentPublicDecision, AlignmentView, ApprovedAlignmentValue,
    CharacterDiagnosticCode, CharacterExtension, CharacterProfile, ExtensionHeader,
    ExtensionWriteBack, Freshness, HexacoTrait, LockState, ReviewState, TraitMeasurement,
    ValueState, VersionedExtension,
};
use crate::synthesis::merge_provenance;
use crate::validation::{
    CharacterError, error, invalid_value, trait_path, trait_value, validate_profile,
};

/// Reserved profile extension carrying one reviewed public alignment view.
pub const ALIGNMENT_EXTENSION_NAMESPACE: &str = "org.weave.character.alignment";
/// Serialized alignment-pack contract version.
pub const ALIGNMENT_PACK_FORMAT_VERSION: u32 = 1;
/// Serialized alignment configuration contract version.
pub const ALIGNMENT_CONFIG_FORMAT_VERSION: u32 = 1;
/// Serialized alignment proposal contract version.
pub const ALIGNMENT_PROPOSAL_FORMAT_VERSION: u32 = 1;
/// Serialized alignment review contract version.
pub const ALIGNMENT_REVIEW_FORMAT_VERSION: u32 = 1;
/// Serialized independently reproducible alignment receipt version.
pub const ALIGNMENT_RECEIPT_FORMAT_VERSION: u32 = 1;

const SCORE_SCALE: i64 = 1_000_000;
const MAX_AXES: usize = 4_096;
const MAX_INPUTS: usize = 4_096;
const MAX_CALIBRATIONS: usize = 16_384;

const PACK_SCHEMA_ID: &str = "urn:weave:schema:character-alignment-pack:1";
const CONFIG_SCHEMA_ID: &str = "urn:weave:schema:character-alignment-config:1";
const PROPOSAL_SCHEMA_ID: &str = "urn:weave:schema:character-alignment-proposal:1";
const REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-alignment-review:1";
const RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-alignment-receipt:1";

/// One separately distributable, data-only narrative alignment system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentPack {
    pub pack_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub license: String,
    pub license_url: String,
    pub methodology: String,
    pub limitations: String,
    pub provider: AlignmentPackProvider,
    pub inputs: BTreeMap<String, AlignmentInputField>,
    pub axes: BTreeMap<String, AlignmentAxis>,
    pub calibrations: BTreeMap<String, AlignmentCalibrationFixture>,
    pub provenance: Provenance,
}

/// Exact provider identity for standalone and public domain-module packs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum AlignmentPackProvider {
    Standalone,
    DomainModule {
        module_id: String,
        module_version: String,
        pack_id: String,
        pack_version: String,
        content_sha256: String,
    },
}

/// One declared canonical input. V1 deliberately permits only HEXACO factors and facets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentInputField {
    pub id: String,
    pub trait_id: HexacoTrait,
    pub profile_path: String,
    pub label: String,
    pub description: String,
}

/// One signed weighted axis and its complete ordered label thresholds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentAxis {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Input weights in non-zero signed thousandths.
    pub inputs: BTreeMap<String, i16>,
    pub thresholds: Vec<AlignmentThreshold>,
    pub limitations: String,
}

/// Inclusive upper bound assigning one neutral narrative label to a signed score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentThreshold {
    pub id: String,
    pub label: String,
    pub upper_bound_micros: i32,
    pub description: String,
}

/// Synthetic calibration fixture proving exact fixed-point behavior for a complete pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentCalibrationFixture {
    pub id: String,
    pub description: String,
    pub inputs_micros: BTreeMap<String, u32>,
    pub expected: BTreeMap<String, AlignmentCalibrationExpected>,
}

/// Exact expected score and label for one calibration axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentCalibrationExpected {
    pub score_micros: i32,
    pub label_id: String,
}

#[derive(Serialize)]
struct AlignmentProviderContent<'a> {
    methodology: &'a str,
    limitations: &'a str,
    inputs: &'a BTreeMap<String, AlignmentInputField>,
    axes: &'a BTreeMap<String, AlignmentAxis>,
    calibrations: &'a BTreeMap<String, AlignmentCalibrationFixture>,
}

/// Project-specific selection and coverage policy, fingerprinted into every proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentConfig {
    pub config_format_version: u32,
    pub id: String,
    pub selected_axes: Vec<String>,
    pub minimum_coverage_micros: u32,
    pub override_locked_view: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_rationale: Option<String>,
}

/// Whether an axis has enough declared evidence to be proposed or must begin withheld.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentProposalState {
    Proposed,
    Withheld,
}

/// One ordered explanation input, including missing evidence rather than inventing a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentExplanationInput {
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

/// Complete fixed-point explanation trace for one proposed public value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentScoreTrace {
    pub axis_id: String,
    pub ordered_inputs: Vec<AlignmentExplanationInput>,
    pub total_weight_thousandths: u32,
    pub covered_weight_thousandths: u32,
    pub coverage_micros: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_sum: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_micros: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_id: Option<String>,
    pub seed_sha256: String,
    pub explanation: String,
}

/// One immutable, reviewable axis proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentProposedValue {
    pub id: String,
    pub state: AlignmentProposalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_label_id: Option<String>,
    pub trace: AlignmentScoreTrace,
}

/// Immutable proposal pinning profile, pack, configuration, seed, and explanation inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentProposal {
    pub proposal_format_version: u32,
    pub profile_id: String,
    pub profile_sha256: String,
    pub pack: AlignmentPackRef,
    pub config: AlignmentConfig,
    pub config_sha256: String,
    pub seed: u64,
    pub values: BTreeMap<String, AlignmentProposedValue>,
}

/// One complete review decision per selected axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentReview {
    pub review_format_version: u32,
    pub proposal_sha256: String,
    pub profile_sha256: String,
    pub pack_sha256: String,
    pub reviewer: String,
    pub rationale: String,
    pub decisions: BTreeMap<String, AlignmentReviewDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentReviewDecision {
    pub axis_id: String,
    pub action: AlignmentReviewAction,
}

/// Explicit editorial decision. Every non-accept path retains a rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "decision", deny_unknown_fields)]
pub enum AlignmentReviewAction {
    Accept {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    Reject {
        rationale: String,
    },
    Edit {
        label_id: String,
        rationale: String,
    },
    Withhold {
        rationale: String,
    },
    Override {
        label_id: String,
        rationale: String,
    },
}

/// Full proof allowing independent reproduction of proposal, review, and atomic output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentReceipt {
    pub receipt_format_version: u32,
    pub input_profile: CharacterProfile,
    pub pack: AlignmentPack,
    pub proposal: AlignmentProposal,
    pub review: AlignmentReview,
    pub proposal_sha256: String,
    pub review_sha256: String,
    pub applied_sha256: String,
    pub output_profile_sha256: String,
    pub output_profile: CharacterProfile,
}

#[derive(Serialize)]
struct AlignmentApplicationFingerprint<'a> {
    profile_sha256: &'a str,
    pack: &'a AlignmentPackRef,
    config_sha256: &'a str,
    proposal_sha256: &'a str,
    review_sha256: &'a str,
    values: &'a BTreeMap<String, ApprovedAlignmentValue>,
    input_paths: &'a [String],
}

#[derive(Serialize)]
struct AlignmentSeedFingerprint<'a> {
    pack: &'a AlignmentPackRef,
    config_sha256: &'a str,
    axis_id: &'a str,
    seed: u64,
    ordered_inputs: &'a [AlignmentExplanationInput],
}

macro_rules! impl_alignment_document {
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

impl_alignment_document!(AlignmentPack, validate_alignment_pack);
impl_alignment_document!(AlignmentConfig, validate_alignment_config);
impl_alignment_document!(AlignmentProposal, validate_proposal_structure);
impl_alignment_document!(AlignmentReview, validate_review_structure);
impl_alignment_document!(AlignmentReceipt, validate_alignment_receipt);

/// Generate the canonical alignment-pack JSON Schema.
pub fn alignment_pack_schema() -> Result<String, CharacterError> {
    alignment_schema::<AlignmentPack>(PACK_SCHEMA_ID, "Weave Character Alignment Pack v1")
}

/// Generate the canonical alignment configuration JSON Schema.
pub fn alignment_config_schema() -> Result<String, CharacterError> {
    alignment_schema::<AlignmentConfig>(
        CONFIG_SCHEMA_ID,
        "Weave Character Alignment Configuration v1",
    )
}

/// Generate the canonical alignment proposal JSON Schema.
pub fn alignment_proposal_schema() -> Result<String, CharacterError> {
    alignment_schema::<AlignmentProposal>(
        PROPOSAL_SCHEMA_ID,
        "Weave Character Alignment Proposal v1",
    )
}

/// Generate the canonical alignment review JSON Schema.
pub fn alignment_review_schema() -> Result<String, CharacterError> {
    alignment_schema::<AlignmentReview>(REVIEW_SCHEMA_ID, "Weave Character Alignment Review v1")
}

/// Generate the canonical independently reproducible receipt JSON Schema.
pub fn alignment_receipt_schema() -> Result<String, CharacterError> {
    alignment_schema::<AlignmentReceipt>(RECEIPT_SCHEMA_ID, "Weave Character Alignment Receipt v1")
}

/// Canonical content fingerprint for one independently validated alignment pack.
pub fn alignment_pack_fingerprint(pack: &AlignmentPack) -> Result<String, CharacterError> {
    validate_alignment_pack(pack)?;
    canonical_hash(pack)
}

/// Fingerprint the exact declarative content supplied by a public domain-module provider.
pub fn alignment_provider_content_fingerprint(
    pack: &AlignmentPack,
) -> Result<String, CharacterError> {
    canonical_hash(&AlignmentProviderContent {
        methodology: &pack.methodology,
        limitations: &pack.limitations,
        inputs: &pack.inputs,
        axes: &pack.axes,
        calibrations: &pack.calibrations,
    })
}

/// Canonical fingerprint for one independently validated alignment configuration.
pub fn alignment_config_fingerprint(config: &AlignmentConfig) -> Result<String, CharacterError> {
    validate_alignment_config(config)?;
    canonical_hash(config)
}

/// Canonical profile fingerprint pinned by alignment proposals and reviews.
pub fn alignment_profile_fingerprint(profile: &CharacterProfile) -> Result<String, CharacterError> {
    validate_profile(profile)?;
    canonical_hash(profile)
}

/// Canonical proposal fingerprint retained by the complete review.
pub fn alignment_proposal_fingerprint(
    proposal: &AlignmentProposal,
) -> Result<String, CharacterError> {
    validate_proposal_structure(proposal)?;
    canonical_hash(proposal)
}

/// Canonical review fingerprint retained by the public view and receipt.
pub fn alignment_review_fingerprint(review: &AlignmentReview) -> Result<String, CharacterError> {
    validate_review_structure(review)?;
    canonical_hash(review)
}

/// Validate one complete, data-only alignment pack and every calibration fixture.
pub fn validate_alignment_pack(pack: &AlignmentPack) -> Result<(), CharacterError> {
    if pack.pack_format_version != ALIGNMENT_PACK_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "pack_format_version",
            "unsupported alignment pack version",
        ));
    }
    validate_namespaced_id("id", &pack.id)?;
    validate_semver("version", &pack.version)?;
    validate_public_text("title", &pack.title, 1, 256)?;
    validate_license("license", &pack.license)?;
    validate_https("license_url", &pack.license_url)?;
    validate_public_text("methodology", &pack.methodology, 1, 8_192)?;
    validate_public_text("limitations", &pack.limitations, 1, 8_192)?;
    match &pack.provider {
        AlignmentPackProvider::Standalone => {}
        AlignmentPackProvider::DomainModule {
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
            "alignment pack provenance is malformed or incomplete",
        )
    })?;
    if pack.provenance.sources.is_empty() {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance.sources",
            "alignment pack requires at least one public or original source",
        ));
    }
    for claim in ["methodology", "inputs", "axes", "calibrations"] {
        if pack.provenance.claims.get(claim).is_none_or(Vec::is_empty) {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                format!("provenance.claims.{claim}"),
                "alignment pack methodology and calibration claims require explicit lineage",
            ));
        }
    }
    if pack.inputs.is_empty() || pack.inputs.len() > MAX_INPUTS {
        return Err(invalid_value(
            "inputs",
            "alignment pack requires a bounded non-empty input declaration",
        ));
    }
    let mut trait_ids = BTreeSet::new();
    for (id, input) in &pack.inputs {
        let path = format!("inputs.{id}");
        validate_local_id(&path, id)?;
        if input.id != *id || !trait_ids.insert(input.trait_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "alignment input identity and canonical trait must be unique",
            ));
        }
        let expected_path = trait_path(input.trait_id);
        if input.profile_path != expected_path {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.profile_path"),
                "alignment input path must exactly identify its declared canonical HEXACO trait",
            ));
        }
        validate_public_text(&format!("{path}.label"), &input.label, 1, 256)?;
        validate_public_text(&format!("{path}.description"), &input.description, 1, 2_048)?;
    }
    if pack.axes.is_empty() || pack.axes.len() > MAX_AXES {
        return Err(invalid_value(
            "axes",
            "alignment pack requires a bounded non-empty axis set",
        ));
    }
    for (id, axis) in &pack.axes {
        validate_axis(&format!("axes.{id}"), id, axis, &pack.inputs)?;
    }
    if pack.calibrations.is_empty() || pack.calibrations.len() > MAX_CALIBRATIONS {
        return Err(invalid_value(
            "calibrations",
            "alignment pack requires bounded synthetic calibration fixtures",
        ));
    }
    for (id, fixture) in &pack.calibrations {
        validate_calibration(pack, &format!("calibrations.{id}"), id, fixture)?;
    }
    if let AlignmentPackProvider::DomainModule { content_sha256, .. } = &pack.provider
        && content_sha256 != &alignment_provider_content_fingerprint(pack)?
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "provider.content_sha256",
            "domain-module alignment content fingerprint is stale",
        ));
    }
    Ok(())
}

fn validate_axis(
    path: &str,
    map_id: &str,
    axis: &AlignmentAxis,
    inputs: &BTreeMap<String, AlignmentInputField>,
) -> Result<(), CharacterError> {
    validate_local_id(path, map_id)?;
    if axis.id != map_id {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.id"),
            "alignment axis identifier must equal its containing map key",
        ));
    }
    validate_public_text(&format!("{path}.label"), &axis.label, 1, 256)?;
    validate_public_text(&format!("{path}.description"), &axis.description, 1, 2_048)?;
    validate_public_text(&format!("{path}.limitations"), &axis.limitations, 1, 2_048)?;
    if axis.inputs.is_empty() || axis.inputs.len() > MAX_INPUTS {
        return Err(invalid_value(
            format!("{path}.inputs"),
            "alignment axis requires a bounded non-empty input weight set",
        ));
    }
    for (input_id, weight) in &axis.inputs {
        validate_local_id(&format!("{path}.inputs"), input_id)?;
        if !inputs.contains_key(input_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.inputs.{input_id}"),
                "alignment axis references an undeclared input",
            ));
        }
        if !(-1_000..=1_000).contains(weight) || *weight == 0 {
            return Err(invalid_value(
                format!("{path}.inputs.{input_id}"),
                "alignment weights must be non-zero signed thousandths",
            ));
        }
    }
    if axis.thresholds.is_empty() || axis.thresholds.len() > 1_024 {
        return Err(invalid_value(
            format!("{path}.thresholds"),
            "alignment axis requires a bounded non-empty threshold set",
        ));
    }
    let mut prior = -1_000_001;
    let mut ids = BTreeSet::new();
    for (index, threshold) in axis.thresholds.iter().enumerate() {
        let threshold_path = format!("{path}.thresholds[{index}]");
        validate_local_id(&format!("{threshold_path}.id"), &threshold.id)?;
        if !ids.insert(threshold.id.as_str()) || threshold.upper_bound_micros <= prior {
            return Err(invalid_value(
                format!("{path}.thresholds"),
                "alignment thresholds must have unique ids and strictly increasing bounds",
            ));
        }
        if !(-1_000_000..=1_000_000).contains(&threshold.upper_bound_micros) {
            return Err(invalid_value(
                format!("{threshold_path}.upper_bound_micros"),
                "alignment threshold must be signed millionths",
            ));
        }
        validate_public_text(&format!("{threshold_path}.label"), &threshold.label, 1, 256)?;
        validate_public_text(
            &format!("{threshold_path}.description"),
            &threshold.description,
            1,
            2_048,
        )?;
        prior = threshold.upper_bound_micros;
    }
    if prior != 1_000_000 {
        return Err(invalid_value(
            format!("{path}.thresholds"),
            "the final alignment threshold must cover positive one million",
        ));
    }
    Ok(())
}

fn validate_calibration(
    pack: &AlignmentPack,
    path: &str,
    map_id: &str,
    fixture: &AlignmentCalibrationFixture,
) -> Result<(), CharacterError> {
    validate_local_id(path, map_id)?;
    if fixture.id != map_id {
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
    if fixture.inputs_micros.keys().ne(pack.inputs.keys()) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.inputs_micros"),
            "calibration inputs must exactly cover the pack input declaration",
        ));
    }
    for value in fixture.inputs_micros.values() {
        validate_micros(&format!("{path}.inputs_micros"), *value)?;
    }
    if fixture.expected.keys().ne(pack.axes.keys()) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.expected"),
            "calibration expectations must exactly cover every pack axis",
        ));
    }
    for (axis_id, expected) in &fixture.expected {
        validate_signed_micros(
            &format!("{path}.expected.{axis_id}.score_micros"),
            expected.score_micros,
        )?;
        validate_local_id(
            &format!("{path}.expected.{axis_id}.label_id"),
            &expected.label_id,
        )?;
        let axis = &pack.axes[axis_id];
        let (score, threshold) = evaluate_complete_axis(axis, &fixture.inputs_micros)?;
        if score != expected.score_micros || threshold.id != expected.label_id {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                format!("{path}.expected.{axis_id}"),
                "alignment calibration no longer matches the declared fixed-point rules",
            ));
        }
    }
    Ok(())
}

/// Validate one exact alignment selection and locked-view override policy.
pub fn validate_alignment_config(config: &AlignmentConfig) -> Result<(), CharacterError> {
    if config.config_format_version != ALIGNMENT_CONFIG_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "config_format_version",
            "unsupported alignment configuration version",
        ));
    }
    validate_namespaced_id("id", &config.id)?;
    if config.selected_axes.is_empty() || config.selected_axes.len() > MAX_AXES {
        return Err(invalid_value(
            "selected_axes",
            "alignment configuration requires a bounded non-empty axis selection",
        ));
    }
    validate_sorted_local_ids("selected_axes", &config.selected_axes)?;
    validate_micros("minimum_coverage_micros", config.minimum_coverage_micros)?;
    match (
        config.override_locked_view,
        config.override_rationale.as_deref(),
    ) {
        (true, Some(rationale)) => validate_public_text("override_rationale", rationale, 1, 2_048),
        (true, None) => Err(invalid_value(
            "override_rationale",
            "locked alignment override requires an explicit rationale",
        )),
        (false, None) => Ok(()),
        (false, Some(_)) => Err(invalid_value(
            "override_rationale",
            "override rationale is only valid when locked override is enabled",
        )),
    }
}

/// Produce byte-stable, explainable alignment proposals without mutating the character.
pub fn propose_alignment(
    profile: &CharacterProfile,
    pack: &AlignmentPack,
    config: &AlignmentConfig,
    seed: u64,
) -> Result<AlignmentProposal, CharacterError> {
    validate_profile(profile)?;
    validate_alignment_pack(pack)?;
    validate_alignment_config(config)?;
    validate_existing_alignment_lock(profile, config)?;
    for axis in &config.selected_axes {
        if !pack.axes.contains_key(axis) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("config.selected_axes.{axis}"),
                "alignment configuration selects an axis absent from the exact pack",
            ));
        }
    }

    let pack_ref = AlignmentPackRef {
        id: pack.id.clone(),
        version: pack.version.clone(),
        sha256: alignment_pack_fingerprint(pack)?,
    };
    let config_sha256 = alignment_config_fingerprint(config)?;
    let mut values = BTreeMap::new();
    for axis_id in &config.selected_axes {
        let axis = &pack.axes[axis_id];
        let mut ordered_inputs = Vec::with_capacity(axis.inputs.len());
        let total_weight = axis
            .inputs
            .values()
            .map(|weight| u32::from(weight.unsigned_abs()))
            .sum::<u32>();
        let mut covered_weight = 0_u32;
        let mut weighted_sum = 0_i64;
        for (input_id, weight) in &axis.inputs {
            let input = &pack.inputs[input_id];
            let measurement = trait_value(&profile.canon.personality, input.trait_id);
            let (profile_micros, centered_micros, contribution, lineage) = measurement
                .map(|attributed| {
                    let profile_micros = measurement_micros(attributed.value);
                    let centered = i32::try_from(i64::from(profile_micros) * 2 - SCORE_SCALE)
                        .expect("unit-interval centering fits signed millionths");
                    let contribution = i64::from(centered) * i64::from(*weight);
                    (
                        profile_micros,
                        centered,
                        contribution,
                        attributed.lineage.clone(),
                    )
                })
                .map_or(
                    (None, None, None, Vec::new()),
                    |(profile_micros, centered, contribution, lineage)| {
                        covered_weight += u32::from(weight.unsigned_abs());
                        weighted_sum += contribution;
                        (
                            Some(profile_micros),
                            Some(centered),
                            Some(contribution),
                            lineage,
                        )
                    },
                );
            ordered_inputs.push(AlignmentExplanationInput {
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
            .expect("weighted alignment score stays within signed millionths")
        });
        let threshold = score_micros.map(|score| threshold_for(axis, score));
        let seed_sha256 = canonical_hash(&AlignmentSeedFingerprint {
            pack: &pack_ref,
            config_sha256: &config_sha256,
            axis_id,
            seed,
            ordered_inputs: &ordered_inputs,
        })?;
        let state = if score_micros.is_some() && coverage_micros >= config.minimum_coverage_micros {
            AlignmentProposalState::Proposed
        } else {
            AlignmentProposalState::Withheld
        };
        let explanation = format!(
            "Axis `{axis_id}` used {covered_weight} of {total_weight} absolute weight thousandths ({coverage_micros} millionths coverage). Present inputs were centered around 0.5, multiplied by declared signed weights, summed with integer arithmetic, and divided by covered weight. Missing inputs contributed neither a value nor fabricated precision. The deterministic seed is retained only in the trace fingerprint; it does not change the score."
        );
        values.insert(
            axis_id.clone(),
            AlignmentProposedValue {
                id: axis_id.clone(),
                state,
                proposed_label_id: threshold.map(|threshold| threshold.id.clone()),
                trace: AlignmentScoreTrace {
                    axis_id: axis_id.clone(),
                    ordered_inputs,
                    total_weight_thousandths: total_weight,
                    covered_weight_thousandths: covered_weight,
                    coverage_micros,
                    weighted_sum: (covered_weight > 0).then_some(weighted_sum),
                    score_micros,
                    threshold_id: threshold.map(|threshold| threshold.id.clone()),
                    seed_sha256,
                    explanation,
                },
            },
        );
    }
    let proposal = AlignmentProposal {
        proposal_format_version: ALIGNMENT_PROPOSAL_FORMAT_VERSION,
        profile_id: profile.id.clone(),
        profile_sha256: alignment_profile_fingerprint(profile)?,
        pack: pack_ref,
        config: config.clone(),
        config_sha256,
        seed,
        values,
    };
    validate_proposal_structure(&proposal)?;
    Ok(proposal)
}

/// Reproduce a proposal from the exact current profile, pack, configuration, and seed.
pub fn validate_alignment_proposal(
    proposal: &AlignmentProposal,
    profile: &CharacterProfile,
    pack: &AlignmentPack,
) -> Result<(), CharacterError> {
    validate_proposal_structure(proposal)?;
    let expected = propose_alignment(profile, pack, &proposal.config, proposal.seed)?;
    if expected != *proposal {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal",
            "alignment proposal does not match its exact pinned inputs",
        ));
    }
    Ok(())
}

/// Validate a proposal against the currently selected project configuration and seed.
///
/// Receipt replay deliberately uses the immutable configuration and seed embedded in the
/// proposal. Editor/project stale checks use this stronger boundary so changing either selection
/// makes an unapplied proposal visibly stale before review or apply.
pub fn validate_alignment_proposal_selection(
    proposal: &AlignmentProposal,
    profile: &CharacterProfile,
    pack: &AlignmentPack,
    config: &AlignmentConfig,
    seed: u64,
) -> Result<(), CharacterError> {
    validate_alignment_proposal(proposal, profile, pack)?;
    validate_alignment_config(config)?;
    if proposal.config != *config || proposal.config_sha256 != alignment_config_fingerprint(config)?
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "config",
            "alignment proposal does not match the currently selected configuration",
        ));
    }
    if proposal.seed != seed {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "seed",
            "alignment proposal does not match the currently selected deterministic seed",
        ));
    }
    Ok(())
}

fn validate_proposal_structure(proposal: &AlignmentProposal) -> Result<(), CharacterError> {
    if proposal.proposal_format_version != ALIGNMENT_PROPOSAL_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "proposal_format_version",
            "unsupported alignment proposal version",
        ));
    }
    validate_namespaced_id("profile_id", &proposal.profile_id)?;
    validate_sha256("profile_sha256", &proposal.profile_sha256)?;
    validate_alignment_pack_ref("pack", &proposal.pack)?;
    validate_alignment_config(&proposal.config)?;
    validate_sha256("config_sha256", &proposal.config_sha256)?;
    if proposal.config_sha256 != alignment_config_fingerprint(&proposal.config)? {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "config_sha256",
            "alignment proposal configuration fingerprint is stale",
        ));
    }
    if proposal
        .values
        .keys()
        .ne(proposal.config.selected_axes.iter())
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "values",
            "alignment proposal values must exactly cover selected axes",
        ));
    }
    for (id, value) in &proposal.values {
        let path = format!("values.{id}");
        validate_local_id(&path, id)?;
        if value.id != *id || value.trace.axis_id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "alignment value and trace identity must equal the containing axis key",
            ));
        }
        if value.proposed_label_id != value.trace.threshold_id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.proposed_label_id"),
                "alignment proposed label must equal the score trace threshold",
            ));
        }
        if let Some(label) = &value.proposed_label_id {
            validate_local_id(&format!("{path}.proposed_label_id"), label)?;
        }
        validate_trace(&format!("{path}.trace"), &value.trace)?;
        let eligible = value.trace.score_micros.is_some()
            && value.trace.coverage_micros >= proposal.config.minimum_coverage_micros;
        if eligible != (value.state == AlignmentProposalState::Proposed) {
            return Err(invalid_value(
                format!("{path}.state"),
                "alignment proposal state must follow the pinned coverage policy",
            ));
        }
    }
    Ok(())
}

fn validate_trace(path: &str, trace: &AlignmentScoreTrace) -> Result<(), CharacterError> {
    validate_local_id(&format!("{path}.axis_id"), &trace.axis_id)?;
    if trace.total_weight_thousandths == 0
        || trace.covered_weight_thousandths > trace.total_weight_thousandths
    {
        return Err(invalid_value(
            format!("{path}.total_weight_thousandths"),
            "alignment trace contains invalid declared or covered weight",
        ));
    }
    validate_micros(&format!("{path}.coverage_micros"), trace.coverage_micros)?;
    if trace.score_micros.is_some() != trace.weighted_sum.is_some()
        || trace.score_micros.is_some() != trace.threshold_id.is_some()
        || trace.score_micros.is_some() != (trace.covered_weight_thousandths > 0)
    {
        return Err(invalid_value(
            format!("{path}.score_micros"),
            "alignment score, weighted sum, threshold, and evidence presence must agree",
        ));
    }
    if let Some(score) = trace.score_micros {
        validate_signed_micros(&format!("{path}.score_micros"), score)?;
    }
    if let Some(threshold) = &trace.threshold_id {
        validate_local_id(&format!("{path}.threshold_id"), threshold)?;
    }
    validate_sha256(&format!("{path}.seed_sha256"), &trace.seed_sha256)?;
    validate_public_text(&format!("{path}.explanation"), &trace.explanation, 1, 4_096)?;
    let mut prior = None;
    let mut declared_weight = 0_u32;
    let mut covered_weight = 0_u32;
    let mut weighted_sum = 0_i64;
    for (index, input) in trace.ordered_inputs.iter().enumerate() {
        let input_path = format!("{path}.ordered_inputs[{index}]");
        validate_local_id(&format!("{input_path}.input_id"), &input.input_id)?;
        if prior.is_some_and(|prior: &str| prior >= input.input_id.as_str()) {
            return Err(invalid_value(
                format!("{path}.ordered_inputs"),
                "alignment explanation inputs must be unique and sorted",
            ));
        }
        prior = Some(input.input_id.as_str());
        if input.profile_path != trait_path(input.trait_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{input_path}.profile_path"),
                "alignment explanation path must match its canonical HEXACO trait",
            ));
        }
        if !(-1_000..=1_000).contains(&input.weight_thousandths) || input.weight_thousandths == 0 {
            return Err(invalid_value(
                format!("{input_path}.weight_thousandths"),
                "alignment explanation weight must be non-zero signed thousandths",
            ));
        }
        declared_weight += u32::from(input.weight_thousandths.unsigned_abs());
        let present = input.profile_micros.is_some();
        if present != input.centered_micros.is_some()
            || present != input.weighted_contribution.is_some()
            || present == input.lineage.is_empty()
        {
            return Err(invalid_value(
                input_path,
                "alignment explanation must preserve missing evidence without partial values",
            ));
        }
        if let Some(profile_micros) = input.profile_micros {
            validate_micros(&format!("{input_path}.profile_micros"), profile_micros)?;
            let expected_centered =
                i32::try_from(i64::from(profile_micros) * 2 - SCORE_SCALE).unwrap();
            if input.centered_micros != Some(expected_centered)
                || input.weighted_contribution
                    != Some(i64::from(expected_centered) * i64::from(input.weight_thousandths))
            {
                return Err(invalid_value(
                    input_path,
                    "alignment explanation fixed-point contribution is inconsistent",
                ));
            }
            validate_sorted_provenance_ids(&format!("{input_path}.lineage"), &input.lineage)?;
            covered_weight += u32::from(input.weight_thousandths.unsigned_abs());
            weighted_sum += input.weighted_contribution.unwrap();
        }
    }
    if declared_weight != trace.total_weight_thousandths
        || covered_weight != trace.covered_weight_thousandths
        || trace.weighted_sum.is_some_and(|sum| sum != weighted_sum)
    {
        return Err(invalid_value(
            path,
            "alignment explanation totals do not match ordered inputs",
        ));
    }
    let expected_coverage = u32::try_from(rounded_div(
        i128::from(covered_weight) * i128::from(SCORE_SCALE),
        i128::from(declared_weight),
    ))
    .unwrap();
    if trace.coverage_micros != expected_coverage {
        return Err(invalid_value(
            format!("{path}.coverage_micros"),
            "alignment explanation coverage is inconsistent",
        ));
    }
    if covered_weight > 0 {
        let expected_score = i32::try_from(rounded_div(
            i128::from(weighted_sum),
            i128::from(covered_weight),
        ))
        .unwrap();
        if trace.score_micros != Some(expected_score) {
            return Err(invalid_value(
                format!("{path}.score_micros"),
                "alignment explanation score is inconsistent",
            ));
        }
    }
    Ok(())
}

/// Create a complete review whose labels are checked against the exact pinned pack.
pub fn create_alignment_review(
    proposal: &AlignmentProposal,
    pack: &AlignmentPack,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
    decisions: BTreeMap<String, AlignmentReviewDecision>,
) -> Result<AlignmentReview, CharacterError> {
    validate_proposal_structure(proposal)?;
    validate_alignment_pack(pack)?;
    let review = AlignmentReview {
        review_format_version: ALIGNMENT_REVIEW_FORMAT_VERSION,
        proposal_sha256: alignment_proposal_fingerprint(proposal)?,
        profile_sha256: proposal.profile_sha256.clone(),
        pack_sha256: proposal.pack.sha256.clone(),
        reviewer: reviewer.into(),
        rationale: rationale.into(),
        decisions,
    };
    validate_alignment_review(&review, proposal, pack)?;
    Ok(review)
}

/// Validate a complete review against the exact immutable proposal and label vocabulary.
pub fn validate_alignment_review(
    review: &AlignmentReview,
    proposal: &AlignmentProposal,
    pack: &AlignmentPack,
) -> Result<(), CharacterError> {
    validate_review_structure(review)?;
    validate_proposal_structure(proposal)?;
    validate_alignment_pack(pack)?;
    if proposal.pack
        != (AlignmentPackRef {
            id: pack.id.clone(),
            version: pack.version.clone(),
            sha256: alignment_pack_fingerprint(pack)?,
        })
        || review.proposal_sha256 != alignment_proposal_fingerprint(proposal)?
        || review.profile_sha256 != proposal.profile_sha256
        || review.pack_sha256 != proposal.pack.sha256
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "review",
            "alignment review does not fingerprint the exact proposal, profile, and pack",
        ));
    }
    if review.decisions.len() != proposal.values.len() {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "decisions",
            "alignment review must decide every selected axis exactly once",
        ));
    }
    for (axis_id, decision) in &review.decisions {
        let proposed = proposal.values.get(axis_id).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                format!("decisions.{axis_id}"),
                "alignment decision references an axis absent from the proposal",
            )
        })?;
        let axis = &pack.axes[axis_id];
        validate_review_action(
            &format!("decisions.{axis_id}.action"),
            &decision.action,
            proposed,
            axis,
        )?;
    }
    Ok(())
}

fn validate_review_structure(review: &AlignmentReview) -> Result<(), CharacterError> {
    if review.review_format_version != ALIGNMENT_REVIEW_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "review_format_version",
            "unsupported alignment review version",
        ));
    }
    validate_sha256("proposal_sha256", &review.proposal_sha256)?;
    validate_sha256("profile_sha256", &review.profile_sha256)?;
    validate_sha256("pack_sha256", &review.pack_sha256)?;
    validate_namespaced_id("reviewer", &review.reviewer)?;
    validate_public_text("rationale", &review.rationale, 1, 2_048)?;
    if review.decisions.len() > MAX_AXES {
        return Err(invalid_value(
            "decisions",
            "alignment review contains too many decisions",
        ));
    }
    for (id, decision) in &review.decisions {
        validate_local_id("decisions", id)?;
        if decision.axis_id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("decisions.{id}.axis_id"),
                "alignment decision axis id must equal its containing map key",
            ));
        }
        validate_review_action_text(&format!("decisions.{id}.action"), &decision.action)?;
    }
    Ok(())
}

fn validate_review_action(
    path: &str,
    action: &AlignmentReviewAction,
    proposed: &AlignmentProposedValue,
    axis: &AlignmentAxis,
) -> Result<(), CharacterError> {
    validate_review_action_text(path, action)?;
    match action {
        AlignmentReviewAction::Accept { .. }
            if proposed.state == AlignmentProposalState::Withheld =>
        {
            Err(error(
                CharacterDiagnosticCode::ForbiddenWriteBack,
                path,
                "a coverage-withheld alignment value requires an explicit override",
            ))
        }
        AlignmentReviewAction::Edit { label_id, .. } => {
            validate_axis_label(path, axis, label_id)?;
            if proposed.state == AlignmentProposalState::Withheld {
                return Err(error(
                    CharacterDiagnosticCode::ForbiddenWriteBack,
                    path,
                    "a coverage-withheld alignment value cannot be edited without override",
                ));
            }
            if proposed.proposed_label_id.as_deref() == Some(label_id) {
                return Err(invalid_value(
                    format!("{path}.label_id"),
                    "an edit must select a different declared label",
                ));
            }
            Ok(())
        }
        AlignmentReviewAction::Override { label_id, .. } => {
            validate_axis_label(path, axis, label_id)
        }
        AlignmentReviewAction::Accept { .. }
        | AlignmentReviewAction::Reject { .. }
        | AlignmentReviewAction::Withhold { .. } => Ok(()),
    }
}

fn validate_axis_label(
    path: &str,
    axis: &AlignmentAxis,
    label_id: &str,
) -> Result<(), CharacterError> {
    if axis.thresholds.iter().all(|label| label.id != label_id) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            format!("{path}.label_id"),
            "alignment decision selects a label absent from the exact pack axis",
        ));
    }
    Ok(())
}

fn validate_review_action_text(
    path: &str,
    action: &AlignmentReviewAction,
) -> Result<(), CharacterError> {
    match action {
        AlignmentReviewAction::Accept {
            rationale: Some(rationale),
        }
        | AlignmentReviewAction::Reject { rationale }
        | AlignmentReviewAction::Withhold { rationale } => {
            validate_public_text(&format!("{path}.rationale"), rationale, 1, 2_048)
        }
        AlignmentReviewAction::Accept { rationale: None } => Ok(()),
        AlignmentReviewAction::Edit {
            label_id,
            rationale,
        }
        | AlignmentReviewAction::Override {
            label_id,
            rationale,
        } => {
            validate_local_id(&format!("{path}.label_id"), label_id)?;
            validate_public_text(&format!("{path}.rationale"), rationale, 1, 2_048)
        }
    }
}

/// Reproduce and atomically apply a complete review, returning a self-validating receipt.
pub fn apply_reviewed_alignment(
    profile: &CharacterProfile,
    pack: &AlignmentPack,
    proposal: &AlignmentProposal,
    review: &AlignmentReview,
) -> Result<AlignmentReceipt, CharacterError> {
    validate_alignment_proposal(proposal, profile, pack)?;
    validate_alignment_review(review, proposal, pack)?;
    let output_profile = apply_alignment_profile(profile, pack, proposal, review)?;
    let CharacterExtension::AlignmentView(extension) = output_profile
        .extensions
        .get(ALIGNMENT_EXTENSION_NAMESPACE)
        .expect("alignment apply creates the reserved extension")
    else {
        unreachable!("reserved alignment namespace has the alignment extension kind")
    };
    let receipt = AlignmentReceipt {
        receipt_format_version: ALIGNMENT_RECEIPT_FORMAT_VERSION,
        input_profile: profile.clone(),
        pack: pack.clone(),
        proposal: proposal.clone(),
        review: review.clone(),
        proposal_sha256: alignment_proposal_fingerprint(proposal)?,
        review_sha256: alignment_review_fingerprint(review)?,
        applied_sha256: extension.value.applied_sha256.clone(),
        output_profile_sha256: alignment_profile_fingerprint(&output_profile)?,
        output_profile,
    };
    validate_alignment_receipt(&receipt)?;
    Ok(receipt)
}

/// Independently reproduce a serialized alignment receipt and every retained fingerprint.
pub fn validate_alignment_receipt(receipt: &AlignmentReceipt) -> Result<(), CharacterError> {
    if receipt.receipt_format_version != ALIGNMENT_RECEIPT_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "receipt_format_version",
            "unsupported alignment receipt version",
        ));
    }
    validate_profile(&receipt.input_profile)?;
    validate_alignment_pack(&receipt.pack)?;
    validate_alignment_proposal(&receipt.proposal, &receipt.input_profile, &receipt.pack)?;
    validate_alignment_review(&receipt.review, &receipt.proposal, &receipt.pack)?;
    for (path, actual, expected) in [
        (
            "proposal_sha256",
            receipt.proposal_sha256.as_str(),
            alignment_proposal_fingerprint(&receipt.proposal)?,
        ),
        (
            "review_sha256",
            receipt.review_sha256.as_str(),
            alignment_review_fingerprint(&receipt.review)?,
        ),
    ] {
        validate_sha256(path, actual)?;
        if actual != expected {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                path,
                "alignment receipt retains a stale fingerprint",
            ));
        }
    }
    let expected_output = apply_alignment_profile(
        &receipt.input_profile,
        &receipt.pack,
        &receipt.proposal,
        &receipt.review,
    )?;
    if receipt.output_profile != expected_output {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "output_profile",
            "alignment receipt output does not match independent reproduction",
        ));
    }
    let CharacterExtension::AlignmentView(extension) = expected_output
        .extensions
        .get(ALIGNMENT_EXTENSION_NAMESPACE)
        .expect("reproduced alignment output has its reserved extension")
    else {
        unreachable!("reserved alignment namespace has the alignment extension kind")
    };
    validate_sha256("applied_sha256", &receipt.applied_sha256)?;
    if receipt.applied_sha256 != extension.value.applied_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "applied_sha256",
            "alignment receipt application fingerprint is stale",
        ));
    }
    validate_sha256("output_profile_sha256", &receipt.output_profile_sha256)?;
    if receipt.output_profile_sha256 != alignment_profile_fingerprint(&expected_output)? {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "output_profile_sha256",
            "alignment receipt output fingerprint is stale",
        ));
    }
    Ok(())
}

fn apply_alignment_profile(
    profile: &CharacterProfile,
    pack: &AlignmentPack,
    proposal: &AlignmentProposal,
    review: &AlignmentReview,
) -> Result<CharacterProfile, CharacterError> {
    validate_alignment_proposal(proposal, profile, pack)?;
    validate_alignment_review(review, proposal, pack)?;
    validate_existing_alignment_lock(profile, &proposal.config)?;
    let proposal_sha256 = alignment_proposal_fingerprint(proposal)?;
    let review_sha256 = alignment_review_fingerprint(review)?;
    let mut values = BTreeMap::new();
    let mut input_paths = BTreeSet::new();
    let mut transformation_inputs = provenance_ids(&pack.provenance);
    for (axis_id, decision) in &review.decisions {
        let proposed = &proposal.values[axis_id];
        let Some((label_id, public_decision)) = decision_label(&decision.action, proposed) else {
            continue;
        };
        let axis = &pack.axes[axis_id];
        let threshold = axis
            .thresholds
            .iter()
            .find(|threshold| threshold.id == label_id)
            .expect("review validation guarantees a declared alignment label");
        let mut paths = proposed
            .trace
            .ordered_inputs
            .iter()
            .filter(|input| input.profile_micros.is_some())
            .map(|input| input.profile_path.clone())
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        for input in &proposed.trace.ordered_inputs {
            if input.profile_micros.is_some() {
                input_paths.insert(input.profile_path.clone());
                transformation_inputs.extend(input.lineage.iter().cloned());
            }
        }
        values.insert(
            axis_id.clone(),
            ApprovedAlignmentValue {
                id: axis_id.clone(),
                label_id: threshold.id.clone(),
                label: threshold.label.clone(),
                decision: public_decision,
                score_micros: proposed.trace.score_micros,
                coverage_micros: proposed.trace.coverage_micros,
                explanation: format!(
                    "{} {} This reviewed value is fictional narrative shorthand, not a diagnosis or authority over canon.",
                    axis.description, threshold.description
                ),
                input_paths: paths,
            },
        );
    }
    let input_paths = input_paths.into_iter().collect::<Vec<_>>();
    let applied_sha256 = canonical_hash(&AlignmentApplicationFingerprint {
        profile_sha256: &proposal.profile_sha256,
        pack: &proposal.pack,
        config_sha256: &proposal.config_sha256,
        proposal_sha256: &proposal_sha256,
        review_sha256: &review_sha256,
        values: &values,
        input_paths: &input_paths,
    })?;
    let transformation_id = format!("alignment_apply_{}", &applied_sha256[..16]);

    let mut output = profile.clone();
    output.provenance = merge_provenance(&output.provenance, &pack.provenance)?;
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
            "alignment application transformation identity already exists",
        ));
    }
    let transformation_inputs = transformation_inputs.into_iter().collect::<Vec<_>>();
    if transformation_inputs.is_empty() {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "alignment application requires explicit public lineage",
        ));
    }
    output
        .provenance
        .transformations
        .push(ProvenanceTransformation {
            id: transformation_id.clone(),
            inputs: transformation_inputs.clone(),
            description: "Applied a complete editorial review to a deterministic, non-diagnostic narrative alignment proposal without mutating canonical personality evidence or authored canon.".to_owned(),
        });
    output
        .provenance
        .transformations
        .sort_by(|left, right| left.id.cmp(&right.id));
    output
        .provenance
        .claims
        .entry(format!("extensions.{ALIGNMENT_EXTENSION_NAMESPACE}"))
        .or_default()
        .push(transformation_id.clone());
    sort_deduplicate_claims(&mut output.provenance);

    let state = if profile
        .extensions
        .contains_key(ALIGNMENT_EXTENSION_NAMESPACE)
    {
        ValueState::Overridden
    } else {
        ValueState::Reviewed
    };
    let mut lineage = transformation_inputs.into_iter().collect::<BTreeSet<_>>();
    lineage.insert(transformation_id);
    output.extensions.insert(
        ALIGNMENT_EXTENSION_NAMESPACE.to_owned(),
        CharacterExtension::AlignmentView(VersionedExtension {
            header: ExtensionHeader {
                namespace: ALIGNMENT_EXTENSION_NAMESPACE.to_owned(),
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
            value: AlignmentView {
                view_id: proposal.pack.id.clone(),
                pack: proposal.pack.clone(),
                values,
                input_paths,
                review_sha256,
                applied_sha256,
            },
        }),
    );
    validate_profile(&output)?;
    Ok(output)
}

fn decision_label(
    action: &AlignmentReviewAction,
    proposed: &AlignmentProposedValue,
) -> Option<(String, AlignmentPublicDecision)> {
    match action {
        AlignmentReviewAction::Accept { .. } => proposed
            .proposed_label_id
            .clone()
            .map(|label| (label, AlignmentPublicDecision::Reviewed)),
        AlignmentReviewAction::Edit { label_id, .. } => {
            Some((label_id.clone(), AlignmentPublicDecision::Edited))
        }
        AlignmentReviewAction::Override { label_id, .. } => {
            Some((label_id.clone(), AlignmentPublicDecision::Overridden))
        }
        AlignmentReviewAction::Reject { .. } | AlignmentReviewAction::Withhold { .. } => None,
    }
}

fn validate_existing_alignment_lock(
    profile: &CharacterProfile,
    config: &AlignmentConfig,
) -> Result<(), CharacterError> {
    let Some(existing) = profile.extensions.get(ALIGNMENT_EXTENSION_NAMESPACE) else {
        return Ok(());
    };
    let CharacterExtension::AlignmentView(existing) = existing else {
        return Err(error(
            CharacterDiagnosticCode::InvalidExtension,
            format!("extensions.{ALIGNMENT_EXTENSION_NAMESPACE}"),
            "reserved alignment namespace contains another extension kind",
        ));
    };
    if existing.header.lock == LockState::Locked && !config.override_locked_view {
        return Err(error(
            CharacterDiagnosticCode::LockedField,
            format!("extensions.{ALIGNMENT_EXTENSION_NAMESPACE}.header.lock"),
            "locked alignment view requires an explicit configured override",
        ));
    }
    Ok(())
}

fn evaluate_complete_axis<'a>(
    axis: &'a AlignmentAxis,
    inputs: &BTreeMap<String, u32>,
) -> Result<(i32, &'a AlignmentThreshold), CharacterError> {
    let total_weight = axis
        .inputs
        .values()
        .map(|weight| u32::from(weight.unsigned_abs()))
        .sum::<u32>();
    let mut weighted_sum = 0_i64;
    for (input_id, weight) in &axis.inputs {
        let value = inputs.get(input_id).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                format!("axis.{}.inputs.{input_id}", axis.id),
                "alignment calibration is missing a declared input",
            )
        })?;
        let centered = i64::from(*value) * 2 - SCORE_SCALE;
        weighted_sum += centered * i64::from(*weight);
    }
    let score = i32::try_from(rounded_div(
        i128::from(weighted_sum),
        i128::from(total_weight),
    ))
    .expect("complete alignment score stays within signed millionths");
    Ok((score, threshold_for(axis, score)))
}

fn threshold_for(axis: &AlignmentAxis, score: i32) -> &AlignmentThreshold {
    axis.thresholds
        .iter()
        .find(|threshold| score <= threshold.upper_bound_micros)
        .expect("validated final threshold covers every signed millionth score")
}

fn rounded_div(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    if numerator >= 0 {
        (numerator + denominator / 2) / denominator
    } else {
        -((-numerator + denominator / 2) / denominator)
    }
}

fn measurement_micros(value: TraitMeasurement) -> u32 {
    let normalized = match value {
        TraitMeasurement::Score { score } => score,
        TraitMeasurement::Band { band } => band.projection_anchor(),
    };
    (normalized * SCORE_SCALE as f64).round() as u32
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
    for values in provenance.claims.values_mut() {
        values.sort();
        values.dedup();
    }
}

fn validate_alignment_pack_ref(path: &str, value: &AlignmentPackRef) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

fn validate_sorted_local_ids(path: &str, values: &[String]) -> Result<(), CharacterError> {
    let mut prior = None;
    for value in values {
        validate_local_id(path, value)?;
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

fn validate_signed_micros(path: &str, value: i32) -> Result<(), CharacterError> {
    if !(-(SCORE_SCALE as i32)..=SCORE_SCALE as i32).contains(&value) {
        return Err(invalid_value(
            path,
            "fixed-point score must be signed millionths",
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

fn alignment_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, CharacterError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-alignment-version".to_owned(),
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
        "alignment",
        "could not encode the alignment document",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_domain::{ProvenanceKind, ProvenanceSource};

    const PROFILE: &str = include_str!(
        "../../../examples/domain-modules/weave-character/omitted-extensions.character.json"
    );

    fn pack() -> AlignmentPack {
        let inputs = BTreeMap::from([
            (
                "agreeableness".to_owned(),
                AlignmentInputField {
                    id: "agreeableness".to_owned(),
                    trait_id: HexacoTrait::Agreeableness,
                    profile_path: trait_path(HexacoTrait::Agreeableness),
                    label: "Agreeableness".to_owned(),
                    description: "Canonical agreeableness factor used only as reviewed fictional shorthand input.".to_owned(),
                },
            ),
            (
                "openness".to_owned(),
                AlignmentInputField {
                    id: "openness".to_owned(),
                    trait_id: HexacoTrait::Openness,
                    profile_path: trait_path(HexacoTrait::Openness),
                    label: "Openness".to_owned(),
                    description: "Canonical openness factor used only as reviewed fictional shorthand input.".to_owned(),
                },
            ),
        ]);
        let axis = AlignmentAxis {
            id: "horizon".to_owned(),
            label: "Horizon".to_owned(),
            description: "A neutral fictional axis describing how readily a character turns toward unfamiliar possibilities.".to_owned(),
            inputs: BTreeMap::from([
                ("agreeableness".to_owned(), 500),
                ("openness".to_owned(), 500),
            ]),
            thresholds: vec![
                AlignmentThreshold {
                    id: "rooted".to_owned(),
                    label: "Rooted".to_owned(),
                    upper_bound_micros: -250_000,
                    description: "Favors continuity in the current fictional situation.".to_owned(),
                },
                AlignmentThreshold {
                    id: "bridging".to_owned(),
                    label: "Bridging".to_owned(),
                    upper_bound_micros: 250_000,
                    description: "Moves between continuity and possibility in the current fictional situation.".to_owned(),
                },
                AlignmentThreshold {
                    id: "seeking".to_owned(),
                    label: "Seeking".to_owned(),
                    upper_bound_micros: 1_000_000,
                    description: "Turns toward unfamiliar possibilities in the current fictional situation.".to_owned(),
                },
            ],
            limitations: "This axis is original narrative shorthand, not a diagnosis, moral ranking, or prediction of behavior.".to_owned(),
        };
        AlignmentPack {
            pack_format_version: ALIGNMENT_PACK_FORMAT_VERSION,
            id: "org.weave.alignment.synthetic_compass".to_owned(),
            version: "1.0.0".to_owned(),
            title: "Synthetic Compass".to_owned(),
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            methodology: "Center declared canonical trait values around one half, apply signed integer weights, divide by present absolute weight, and select the first inclusive threshold containing the signed millionth score.".to_owned(),
            limitations: "The output is optional fictional storytelling shorthand. It is not clinical, psychometric, moral, causal, or authoritative over canon.".to_owned(),
            provider: AlignmentPackProvider::Standalone,
            inputs,
            axes: BTreeMap::from([("horizon".to_owned(), axis)]),
            calibrations: BTreeMap::from([
                (
                    "centered".to_owned(),
                    AlignmentCalibrationFixture {
                        id: "centered".to_owned(),
                        description: "Both inputs at the exact midpoint select the middle label.".to_owned(),
                        inputs_micros: BTreeMap::from([
                            ("agreeableness".to_owned(), 500_000),
                            ("openness".to_owned(), 500_000),
                        ]),
                        expected: BTreeMap::from([(
                            "horizon".to_owned(),
                            AlignmentCalibrationExpected {
                                score_micros: 0,
                                label_id: "bridging".to_owned(),
                            },
                        )]),
                    },
                ),
                (
                    "seeking".to_owned(),
                    AlignmentCalibrationFixture {
                        id: "seeking".to_owned(),
                        description: "High synthetic inputs select the upper declared label.".to_owned(),
                        inputs_micros: BTreeMap::from([
                            ("agreeableness".to_owned(), 700_000),
                            ("openness".to_owned(), 900_000),
                        ]),
                        expected: BTreeMap::from([(
                            "horizon".to_owned(),
                            AlignmentCalibrationExpected {
                                score_micros: 600_000,
                                label_id: "seeking".to_owned(),
                            },
                        )]),
                    },
                ),
            ]),
            provenance: Provenance {
                sources: vec![ProvenanceSource {
                    id: "weave_synthetic_compass".to_owned(),
                    kind: ProvenanceKind::Original,
                    url: "https://github.com/chrisgliddon/weave".to_owned(),
                    revision: "alignment-v1".to_owned(),
                    sha256: None,
                    license: "MIT".to_owned(),
                    license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE"
                        .to_owned(),
                    attribution: "Original synthetic alignment methodology, labels, explanations, and calibration fixtures.".to_owned(),
                    modified: false,
                }],
                transformations: Vec::new(),
                claims: ["axes", "calibrations", "inputs", "methodology"]
                    .map(|claim| {
                        (
                            claim.to_owned(),
                            vec!["weave_synthetic_compass".to_owned()],
                        )
                    })
                    .into(),
            },
        }
    }

    fn config() -> AlignmentConfig {
        AlignmentConfig {
            config_format_version: ALIGNMENT_CONFIG_FORMAT_VERSION,
            id: "org.weave.alignment.reference_config".to_owned(),
            selected_axes: vec!["horizon".to_owned()],
            minimum_coverage_micros: 1_000_000,
            override_locked_view: false,
            override_rationale: None,
        }
    }

    #[test]
    fn proposal_review_apply_and_receipt_are_exact_and_non_canonical() {
        let profile = CharacterProfile::from_json(PROFILE).unwrap();
        let pack = pack();
        let proposal = propose_alignment(&profile, &pack, &config(), 73).unwrap();
        assert_eq!(
            proposal.values["horizon"].state,
            AlignmentProposalState::Proposed
        );
        assert_eq!(
            proposal.values["horizon"].proposed_label_id.as_deref(),
            Some("seeking")
        );
        assert_eq!(proposal.values["horizon"].trace.score_micros, Some(460_000));
        assert_eq!(proposal.values["horizon"].trace.coverage_micros, 1_000_000);

        let review = create_alignment_review(
            &proposal,
            &pack,
            "org.weave.reviewer.test",
            "Accept this original fictional shorthand after inspecting every fixed-point input.",
            BTreeMap::from([(
                "horizon".to_owned(),
                AlignmentReviewDecision {
                    axis_id: "horizon".to_owned(),
                    action: AlignmentReviewAction::Accept { rationale: None },
                },
            )]),
        )
        .unwrap();
        let receipt = apply_reviewed_alignment(&profile, &pack, &proposal, &review).unwrap();
        assert_eq!(receipt.input_profile.canon, receipt.output_profile.canon);
        let CharacterExtension::AlignmentView(view) = receipt
            .output_profile
            .extensions
            .get(ALIGNMENT_EXTENSION_NAMESPACE)
            .unwrap()
        else {
            panic!("reviewed alignment view is absent")
        };
        assert_eq!(
            view.value.values["horizon"].decision,
            AlignmentPublicDecision::Reviewed
        );
        assert_eq!(
            receipt,
            AlignmentReceipt::from_json(&receipt.to_json().unwrap()).unwrap()
        );
        assert_eq!(
            receipt,
            AlignmentReceipt::from_ron(&receipt.to_ron().unwrap()).unwrap()
        );
        let mut canonical_tamper = receipt.clone();
        canonical_tamper
            .output_profile
            .canon
            .identity
            .display_name
            .value = "A canonical mutation not produced by alignment".to_owned();
        assert!(validate_alignment_receipt(&canonical_tamper).is_err());

        let changed_seed = propose_alignment(&profile, &pack, &config(), 74).unwrap();
        assert_ne!(proposal, changed_seed);
        assert_eq!(
            proposal.values["horizon"].trace.score_micros,
            changed_seed.values["horizon"].trace.score_micros
        );
    }

    #[test]
    fn incomplete_stale_invalid_label_and_provider_hash_inputs_fail_closed() {
        let profile = CharacterProfile::from_json(PROFILE).unwrap();
        let mut alignment_pack = pack();
        let proposal = propose_alignment(&profile, &alignment_pack, &config(), 91).unwrap();
        assert!(
            create_alignment_review(
                &proposal,
                &alignment_pack,
                "org.weave.reviewer.test",
                "A complete review must decide every selected axis.",
                BTreeMap::new(),
            )
            .is_err()
        );
        let invalid = BTreeMap::from([(
            "horizon".to_owned(),
            AlignmentReviewDecision {
                axis_id: "horizon".to_owned(),
                action: AlignmentReviewAction::Override {
                    label_id: "not_declared".to_owned(),
                    rationale:
                        "Attempt an invalid label to prove the exact pack vocabulary is enforced."
                            .to_owned(),
                },
            },
        )]);
        assert!(
            create_alignment_review(
                &proposal,
                &alignment_pack,
                "org.weave.reviewer.test",
                "Invalid labels must fail before any profile mutation.",
                invalid,
            )
            .is_err()
        );
        let mut changed_profile = profile.clone();
        changed_profile
            .canon
            .personality
            .openness
            .factor
            .as_mut()
            .unwrap()
            .value = TraitMeasurement::Score { score: 0.82 };
        assert!(validate_alignment_proposal(&proposal, &changed_profile, &alignment_pack).is_err());

        let content_sha256 = alignment_provider_content_fingerprint(&alignment_pack).unwrap();
        alignment_pack.provider = AlignmentPackProvider::DomainModule {
            module_id: "org.weave.alignment.synthetic_compass".to_owned(),
            module_version: "1.0.0".to_owned(),
            pack_id: "reference".to_owned(),
            pack_version: "1.0.0".to_owned(),
            content_sha256,
        };
        validate_alignment_pack(&alignment_pack).unwrap();
        alignment_pack.axes.get_mut("horizon").unwrap().description =
            "A changed provider rule requires a changed provider content hash.".to_owned();
        assert!(validate_alignment_pack(&alignment_pack).is_err());

        let mut unprovenanced = pack();
        unprovenanced.provenance.sources.clear();
        unprovenanced.provenance.claims.clear();
        assert!(validate_alignment_pack(&unprovenanced).is_err());
    }

    #[test]
    fn contributor_defined_standalone_axis_needs_no_core_registration() {
        let profile = CharacterProfile::from_json(PROFILE).unwrap();
        let mut contributor = pack();
        contributor.id = "org.example.alignment.lantern_compass".to_owned();
        contributor.title = "Lantern Compass".to_owned();
        let mut axis = contributor.axes.remove("horizon").unwrap();
        axis.id = "lantern_stance".to_owned();
        axis.label = "Lantern Stance".to_owned();
        contributor.axes.insert("lantern_stance".to_owned(), axis);
        for calibration in contributor.calibrations.values_mut() {
            let expected = calibration.expected.remove("horizon").unwrap();
            calibration
                .expected
                .insert("lantern_stance".to_owned(), expected);
        }

        validate_alignment_pack(&contributor).unwrap();
        let mut selection = config();
        selection.id = "org.example.alignment.lantern_selection".to_owned();
        selection.selected_axes = vec!["lantern_stance".to_owned()];
        let proposal = propose_alignment(&profile, &contributor, &selection, 73).unwrap();
        assert_eq!(
            proposal.values["lantern_stance"]
                .proposed_label_id
                .as_deref(),
            Some("seeking")
        );
        assert_eq!(proposal.pack.id, "org.example.alignment.lantern_compass");
    }

    #[test]
    fn missing_evidence_is_traced_and_withheld_without_imputation() {
        let mut profile = CharacterProfile::from_json(PROFILE).unwrap();
        profile.canon.personality.agreeableness.factor = None;
        crate::recompute_derived(&mut profile);
        let proposal = propose_alignment(&profile, &pack(), &config(), 73).unwrap();
        let value = &proposal.values["horizon"];
        assert_eq!(value.state, AlignmentProposalState::Withheld);
        assert_eq!(value.trace.coverage_micros, 500_000);
        let missing = value
            .trace
            .ordered_inputs
            .iter()
            .find(|input| input.input_id == "agreeableness")
            .unwrap();
        assert!(missing.profile_micros.is_none());
        assert!(missing.centered_micros.is_none());
        assert!(missing.weighted_contribution.is_none());
        assert!(missing.lineage.is_empty());
    }

    #[test]
    fn changed_project_configuration_or_seed_makes_a_proposal_stale() {
        let profile = CharacterProfile::from_json(PROFILE).unwrap();
        let pack = pack();
        let config = config();
        let proposal = propose_alignment(&profile, &pack, &config, 73).unwrap();
        validate_alignment_proposal_selection(&proposal, &profile, &pack, &config, 73).unwrap();

        let mut changed_config = config.clone();
        changed_config.minimum_coverage_micros = 500_000;
        assert!(
            validate_alignment_proposal_selection(&proposal, &profile, &pack, &changed_config, 73,)
                .is_err()
        );
        assert!(
            validate_alignment_proposal_selection(&proposal, &profile, &pack, &config, 74).is_err()
        );
    }
}
