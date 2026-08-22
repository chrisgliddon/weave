//! Guided Character authoring built on the canonical template, overlay, and review contracts.
//!
//! This module does not introduce another character canon. A draft retains an optional immutable
//! [`CharacterTemplate`], one sparse [`CharacterOverlay`], reviewed enrichment receipts, and
//! review metadata. Every effective profile is reproduced through [`synthesize_character`].

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    Provenance, ProvenanceKind, parse_strict_json, to_pretty_json, to_pretty_ron,
    validate_provenance,
};

use crate::synthesis::{current_value_hash, merge_provenance, profile_field_paths};
use crate::validation::{
    ALL_HEXACO_TRAITS, error, operation_target, trait_path, trait_value, validate_local_id,
    validate_namespaced_id, validate_semver, validate_sha256, validate_text,
};
use crate::{
    ALIGNMENT_EXTENSION_NAMESPACE, AlignmentReceipt, AlignmentView, Attributed, BirthDate,
    CHARACTER_OVERLAY_FORMAT_VERSION, CharacterDerivedViews, CharacterDiagnostic,
    CharacterDiagnosticCode, CharacterExtension, CharacterIdentity, CharacterOperation,
    CharacterOperationAction, CharacterOverlay, CharacterProfile, CharacterTemplate,
    CharacterTemplateRef, Confidence, DATE_CONTEXT_EXTENSION_NAMESPACE, DateContext,
    DiagnosticSeverity, Freshness, HexacoTrait, IdentityPresentation, LockState, OceanView,
    PresentationReceipt, ReviewState, SynthesisOrigin, TemporalContextReceipt, TraitMeasurement,
    ValueState, synthesize_character, template_fingerprint, validate_alignment_receipt,
    validate_overlay, validate_presentation_receipt, validate_profile, validate_template,
    validate_temporal_context_receipt,
};

/// Current guided-authoring workspace format.
pub const CHARACTER_AUTHORING_WORKSPACE_FORMAT_VERSION: u32 = 1;
/// Current guided-authoring revision format.
pub const CHARACTER_AUTHORING_REVISION_FORMAT_VERSION: u32 = 1;
/// Current guided-authoring preview format.
pub const CHARACTER_AUTHORING_PREVIEW_FORMAT_VERSION: u32 = 1;
/// Current original behavior-questionnaire pack format.
pub const CHARACTER_QUESTIONNAIRE_PACK_FORMAT_VERSION: u32 = 1;
/// Current questionnaire answer-set format.
pub const CHARACTER_QUESTIONNAIRE_ANSWERS_FORMAT_VERSION: u32 = 1;
/// Current questionnaire proposal format.
pub const CHARACTER_QUESTIONNAIRE_PROPOSAL_FORMAT_VERSION: u32 = 1;
/// Current questionnaire review format.
pub const CHARACTER_QUESTIONNAIRE_REVIEW_FORMAT_VERSION: u32 = 1;
/// Current independently reproducible questionnaire receipt format.
pub const CHARACTER_QUESTIONNAIRE_RECEIPT_FORMAT_VERSION: u32 = 1;
/// Current final authoring-review format.
pub const CHARACTER_FINAL_REVIEW_FORMAT_VERSION: u32 = 1;

const WORKSPACE_SCHEMA_ID: &str = "urn:weave:schema:character-authoring-workspace:1";
const REVISION_SCHEMA_ID: &str = "urn:weave:schema:character-authoring-revision:1";
const PREVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-authoring-preview:1";
const QUESTIONNAIRE_PACK_SCHEMA_ID: &str = "urn:weave:schema:character-questionnaire-pack:1";
const QUESTIONNAIRE_ANSWERS_SCHEMA_ID: &str = "urn:weave:schema:character-questionnaire-answers:1";
const QUESTIONNAIRE_PROPOSAL_SCHEMA_ID: &str =
    "urn:weave:schema:character-questionnaire-proposal:1";
const QUESTIONNAIRE_REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-questionnaire-review:1";
const QUESTIONNAIRE_RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-questionnaire-receipt:1";
const FINAL_REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-final-review:1";

/// All 24 factor-specific HEXACO facets in stable contract order.
pub const ALL_HEXACO_FACETS: [HexacoTrait; 24] = [
    HexacoTrait::Sincerity,
    HexacoTrait::Fairness,
    HexacoTrait::GreedAvoidance,
    HexacoTrait::Modesty,
    HexacoTrait::Fearfulness,
    HexacoTrait::Anxiety,
    HexacoTrait::Dependence,
    HexacoTrait::Sentimentality,
    HexacoTrait::SocialSelfEsteem,
    HexacoTrait::SocialBoldness,
    HexacoTrait::Sociability,
    HexacoTrait::Liveliness,
    HexacoTrait::Forgivingness,
    HexacoTrait::Gentleness,
    HexacoTrait::Flexibility,
    HexacoTrait::Patience,
    HexacoTrait::Organization,
    HexacoTrait::Diligence,
    HexacoTrait::Perfectionism,
    HexacoTrait::Prudence,
    HexacoTrait::AestheticAppreciation,
    HexacoTrait::Inquisitiveness,
    HexacoTrait::Creativity,
    HexacoTrait::Unconventionality,
];

/// Portable collection of guided drafts. The workspace is project data, not an entitlement list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringWorkspace {
    pub workspace_format_version: u32,
    pub id: String,
    pub revision: u64,
    /// Drafts keyed by their exact stable character id.
    pub drafts: BTreeMap<String, CharacterAuthoringDraft>,
    pub provenance: Provenance,
}

/// One reopenable draft whose effective profile is always synthesized from retained source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringDraft {
    pub id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<CharacterTemplate>,
    pub overlay: CharacterOverlay,
    /// Immutable questionnaire applications retained in chronological order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questionnaire_receipts: Vec<CharacterQuestionnaireReceipt>,
    /// Reviewed presentation-catalog applications retained in chronological id order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presentation_receipts: Vec<PresentationReceipt>,
    /// Applied template releases retained so inherited field origins remain inspectable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_migrations: Vec<AppliedCharacterTemplateMigration>,
    /// Blocking projection/context work created by an accepted canonical revision.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved_invalidations: BTreeMap<String, CharacterAuthoringInvalidation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_review: Option<CharacterFinalReview>,
}

/// Stable list record shared by the text and editor workflows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringDraftSummary {
    pub id: String,
    pub revision: u64,
    pub display_name: String,
    pub template: Option<CharacterTemplateRef>,
    pub unresolved_invalidations: usize,
    pub final_reviewed: bool,
    pub effective_profile_sha256: String,
}

/// Authoring entry mode. All modes converge on the same typed overlay operations and validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterAuthoringInputMode {
    General,
    DirectFacets,
    ConcisePicker,
    Questionnaire,
    PresentationCatalog,
    ImportedProfile,
    ReviewedEnrichment,
    TemplateMigration,
}

/// One optimistic, previewable draft revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringRevision {
    pub revision_format_version: u32,
    pub id: String,
    pub draft_id: String,
    pub expected_draft_revision: u64,
    pub input_mode: CharacterAuthoringInputMode,
    pub rationale: String,
    /// Closed canonical actions; suggestions, extensions, and derived fields are rejected here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<CharacterAuthoringChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questionnaire_receipt: Option<Box<CharacterQuestionnaireReceipt>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_receipt: Option<Box<PresentationReceipt>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<Box<CharacterReviewedEnrichment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_migration: Option<Box<ReviewedCharacterTemplateMigration>>,
    pub provenance: Provenance,
}

/// Source representation of one guided editor edit before template-relative hashing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringChange {
    pub id: String,
    pub rationale: String,
    pub action: CharacterOperationAction,
}

/// Explicitly reviewed switch to another exact release of the same template identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewedCharacterTemplateMigration {
    pub prior_template_sha256: String,
    pub template: CharacterTemplate,
    pub reviewer: String,
    pub rationale: String,
}

/// One applied immutable template transition retained after its review artifact is consumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppliedCharacterTemplateMigration {
    pub prior_template: CharacterTemplateRef,
    pub template: CharacterTemplateRef,
    pub applied_draft_revision: u64,
    pub reviewer: String,
    pub rationale: String,
}

/// Existing enrichment pipelines may enter a draft only through a reproducible reviewed receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    tag = "kind",
    content = "receipt",
    deny_unknown_fields
)]
pub enum CharacterReviewedEnrichment {
    Alignment(Box<AlignmentReceipt>),
    DateContext(Box<TemporalContextReceipt>),
}

/// Complete dry-run inspection returned before a revision is persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringPreview {
    pub preview_format_version: u32,
    pub workspace_id: String,
    pub draft_id: String,
    pub input_draft_revision: u64,
    pub revision_id: String,
    pub base_fields: Vec<CharacterAuthoringFieldView>,
    pub changes: Vec<CharacterAuthoringFieldChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migration_effects: Vec<CharacterAuthoringFieldChange>,
    pub derived_ocean: DerivedOceanInspection,
    pub invalidations: Vec<CharacterAuthoringInvalidation>,
    pub resulting_profile_sha256: String,
    pub candidate_draft: CharacterAuthoringDraft,
}

/// Base/effective field pair with visible inheritance, override, lock, and protection state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringFieldView {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<SynthesisOrigin>,
    pub origin_kind: CharacterAuthoringFieldOriginKind,
    pub inherited: bool,
    pub overridden: bool,
    pub locked: bool,
    pub protected_or_pack_owned: bool,
}

/// Human-reviewable origin of one effective field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterAuthoringFieldOriginKind {
    Template,
    AuthoredOverride,
    AcceptedSuggestion,
    TemplateMigration,
}

/// One effective field difference in a revision or migration preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringFieldChange {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<serde_json::Value>,
    pub before_locked: bool,
    pub after_locked: bool,
}

/// Explicit derived styling metadata for editor and text renderers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedOceanInspection {
    pub authority: ValueState,
    pub visually_distinct_from_canon: bool,
    pub lossy: bool,
    pub before: OceanView,
    pub after: OceanView,
    pub recomputed_in_candidate: bool,
}

/// Downstream work exposed before a canonical change is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterAuthoringInvalidation {
    pub id: String,
    pub kind: CharacterAuthoringInvalidationKind,
    pub input_paths: Vec<String>,
    pub required_action: CharacterAuthoringRequiredAction,
    pub blocking_final_review: bool,
    pub resolved_in_candidate: bool,
    pub message: String,
}

/// Closed invalidation families.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CharacterAuthoringInvalidationKind {
    DerivedOcean,
    AlignmentReview,
    DateContextReview,
    FinalReview,
}

/// Action needed to clear an invalidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterAuthoringRequiredAction {
    Recompute,
    RefreshAndReview,
    RepeatFinalReview,
}

/// Original, data-only narrative behavior questionnaire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnairePack {
    pub pack_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub methodology: String,
    pub limitations: Vec<String>,
    /// Must be true: prompts are fictional authoring aids, not a psychometric instrument.
    pub narrative_authoring_only: bool,
    /// Must be true for a distributable pack.
    pub independently_authored_prompts: bool,
    pub license: String,
    pub license_url: String,
    pub items: BTreeMap<String, CharacterQuestionnaireItem>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub conflict_rules: BTreeMap<String, CharacterQuestionnaireConflictRule>,
    pub provenance: Provenance,
}

/// Exact questionnaire pack coordinate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnairePackRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

/// One original behavior prompt with signed fixed-point facet weights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireItem {
    pub id: String,
    pub prompt: String,
    /// Only the 24 facet identifiers are accepted. Values are signed millionths.
    pub facet_weights_micros: BTreeMap<HexacoTrait, i32>,
}

/// Declarative narrative-tension rule. It is not evidence or a diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireConflictRule {
    pub id: String,
    pub label: String,
    pub explanation: String,
    pub predicates: Vec<CharacterQuestionnaireConflictPredicate>,
    /// Facet suggestions omitted when the author rejects this conflict.
    pub rejected_traits: Vec<HexacoTrait>,
    pub rationale_required: bool,
}

/// One fixed-point predicate in a conflict rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireConflictPredicate {
    pub trait_id: HexacoTrait,
    pub comparison: CharacterQuestionnaireComparison,
    pub threshold_micros: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterQuestionnaireComparison {
    AtLeast,
    AtMost,
}

/// Exact responses to one pinned pack. Response values range from -2 through 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireAnswers {
    pub answers_format_version: u32,
    pub id: String,
    pub pack: CharacterQuestionnairePackRef,
    pub responses: BTreeMap<String, i8>,
}

/// Immutable, independently reproducible questionnaire scoring proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireProposal {
    pub proposal_format_version: u32,
    pub id: String,
    pub input_profile: CharacterProfile,
    pub input_profile_sha256: String,
    pub pack: CharacterQuestionnairePack,
    pub pack_ref: CharacterQuestionnairePackRef,
    pub answers: CharacterQuestionnaireAnswers,
    pub seed: u64,
    pub facets: BTreeMap<HexacoTrait, CharacterQuestionnaireFacetProposal>,
    pub conflicts: BTreeMap<String, CharacterQuestionnaireConflict>,
}

/// Proposed score, confidence, and complete answer-to-facet trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireFacetProposal {
    pub trait_id: HexacoTrait,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_micros: Option<u32>,
    pub confidence: Confidence,
    pub answered_items: usize,
    pub total_items: usize,
    pub trace: Vec<CharacterQuestionnaireContribution>,
}

/// One visible item contribution; omitted responses remain visible without a contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireContribution {
    pub item_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<i8>,
    pub weight_micros: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_contribution: Option<i64>,
}

/// Triggered narrative-tension warning requiring one explicit author decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireConflict {
    pub id: String,
    pub rule_id: String,
    pub label: String,
    pub explanation: String,
    pub input_paths: Vec<String>,
    pub rejected_traits: Vec<HexacoTrait>,
    pub rationale_required: bool,
}

/// Complete per-facet confidence review and per-conflict disposition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireReview {
    pub review_format_version: u32,
    pub proposal_sha256: String,
    pub reviewer: String,
    pub rationale: String,
    pub facet_decisions: BTreeMap<HexacoTrait, CharacterQuestionnaireFacetDecision>,
    pub conflict_decisions: BTreeMap<String, CharacterQuestionnaireConflictDecision>,
}

/// Explicit confidence-aware decision for every facet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
pub enum CharacterQuestionnaireFacetDecision {
    Accept {
        confidence: Confidence,
        lock: LockState,
    },
    Edit {
        value: TraitMeasurement,
        confidence: Confidence,
        lock: LockState,
        rationale: String,
    },
    Override {
        value: TraitMeasurement,
        confidence: Confidence,
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

/// Required disposition for one triggered conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
pub enum CharacterQuestionnaireConflictDecision {
    Accept {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    Reject {
        rationale: String,
    },
    DeliberateException {
        rationale: String,
    },
}

/// Full proof of questionnaire scoring, review, canonical operations, and output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterQuestionnaireReceipt {
    pub receipt_format_version: u32,
    pub id: String,
    pub proposal: CharacterQuestionnaireProposal,
    pub review: CharacterQuestionnaireReview,
    pub review_sha256: String,
    pub operations: Vec<CharacterOperation>,
    pub applied_traits: Vec<HexacoTrait>,
    pub rejected_traits: Vec<HexacoTrait>,
    pub output_profile: CharacterProfile,
    pub output_profile_sha256: String,
}

/// Accepted or needs-changes final authoring review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterFinalReviewDecision {
    Accepted,
    NeedsChanges,
}

/// Saved final review with the exact visible summary and unresolved diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterFinalReview {
    pub review_format_version: u32,
    pub draft_id: String,
    pub draft_revision: u64,
    pub profile_sha256: String,
    pub reviewer: String,
    pub rationale: String,
    pub decision: CharacterFinalReviewDecision,
    pub summary: CharacterFinalReviewSummary,
}

/// Complete human-review surface generated from one validated effective profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterFinalReviewSummary {
    pub profile_id: String,
    pub identity: CharacterIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_presentation: Option<IdentityPresentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<Attributed<BirthDate>>,
    pub canonical_traits: BTreeMap<HexacoTrait, Attributed<TraitMeasurement>>,
    pub derived_ocean: CharacterDerivedViews,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_alignment: Option<AlignmentView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_date_context: Option<DateContext>,
    pub inner_life: BTreeMap<String, crate::AuthoredNote>,
    pub voice: BTreeMap<String, crate::VoiceDirection>,
    pub provenance: Provenance,
    pub unresolved_diagnostics: Vec<CharacterDiagnostic>,
}

macro_rules! impl_authoring_document {
    ($type:ty, $validate:ident) => {
        impl $type {
            pub fn from_json(source: &str) -> Result<Self, crate::CharacterError> {
                let value = parse_strict_json(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn from_ron(source: &str) -> Result<Self, crate::CharacterError> {
                let value = ron::from_str(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn to_json(&self) -> Result<String, crate::CharacterError> {
                $validate(self)?;
                to_pretty_json(self).map_err(|_| encoding_error())
            }

            pub fn to_ron(&self) -> Result<String, crate::CharacterError> {
                $validate(self)?;
                to_pretty_ron(self).map_err(|_| encoding_error())
            }
        }
    };
}

impl_authoring_document!(CharacterAuthoringWorkspace, validate_authoring_workspace);
impl_authoring_document!(CharacterAuthoringRevision, validate_authoring_revision);
impl_authoring_document!(CharacterQuestionnairePack, validate_questionnaire_pack);
impl_authoring_document!(
    CharacterQuestionnaireAnswers,
    validate_questionnaire_answers
);
impl_authoring_document!(
    CharacterQuestionnaireProposal,
    validate_questionnaire_proposal
);
impl_authoring_document!(
    CharacterQuestionnaireReview,
    validate_questionnaire_review_structure
);
impl_authoring_document!(
    CharacterQuestionnaireReceipt,
    validate_questionnaire_receipt
);
impl_authoring_document!(CharacterFinalReview, validate_final_review_structure);

impl CharacterAuthoringPreview {
    pub fn to_json(&self) -> Result<String, crate::CharacterError> {
        to_pretty_json(self).map_err(|_| encoding_error())
    }

    pub fn to_ron(&self) -> Result<String, crate::CharacterError> {
        to_pretty_ron(self).map_err(|_| encoding_error())
    }
}

/// Generate the guided-authoring workspace JSON Schema.
pub fn character_authoring_workspace_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterAuthoringWorkspace>(
        WORKSPACE_SCHEMA_ID,
        "Weave Character Guided Authoring Workspace v1",
        Some((
            "workspace_format_version",
            CHARACTER_AUTHORING_WORKSPACE_FORMAT_VERSION,
        )),
    )
}

/// Generate the guided revision JSON Schema.
pub fn character_authoring_revision_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterAuthoringRevision>(
        REVISION_SCHEMA_ID,
        "Weave Character Guided Authoring Revision v1",
        Some((
            "revision_format_version",
            CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        )),
    )
}

/// Generate the pre-apply preview JSON Schema.
pub fn character_authoring_preview_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterAuthoringPreview>(
        PREVIEW_SCHEMA_ID,
        "Weave Character Guided Authoring Preview v1",
        Some((
            "preview_format_version",
            CHARACTER_AUTHORING_PREVIEW_FORMAT_VERSION,
        )),
    )
}

pub fn character_questionnaire_pack_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterQuestionnairePack>(
        QUESTIONNAIRE_PACK_SCHEMA_ID,
        "Weave Character Narrative Questionnaire Pack v1",
        Some((
            "pack_format_version",
            CHARACTER_QUESTIONNAIRE_PACK_FORMAT_VERSION,
        )),
    )
}

pub fn character_questionnaire_answers_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterQuestionnaireAnswers>(
        QUESTIONNAIRE_ANSWERS_SCHEMA_ID,
        "Weave Character Narrative Questionnaire Answers v1",
        Some((
            "answers_format_version",
            CHARACTER_QUESTIONNAIRE_ANSWERS_FORMAT_VERSION,
        )),
    )
}

pub fn character_questionnaire_proposal_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterQuestionnaireProposal>(
        QUESTIONNAIRE_PROPOSAL_SCHEMA_ID,
        "Weave Character Narrative Questionnaire Proposal v1",
        Some((
            "proposal_format_version",
            CHARACTER_QUESTIONNAIRE_PROPOSAL_FORMAT_VERSION,
        )),
    )
}

pub fn character_questionnaire_review_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterQuestionnaireReview>(
        QUESTIONNAIRE_REVIEW_SCHEMA_ID,
        "Weave Character Narrative Questionnaire Review v1",
        Some((
            "review_format_version",
            CHARACTER_QUESTIONNAIRE_REVIEW_FORMAT_VERSION,
        )),
    )
}

pub fn character_questionnaire_receipt_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterQuestionnaireReceipt>(
        QUESTIONNAIRE_RECEIPT_SCHEMA_ID,
        "Weave Character Narrative Questionnaire Receipt v1",
        Some((
            "receipt_format_version",
            CHARACTER_QUESTIONNAIRE_RECEIPT_FORMAT_VERSION,
        )),
    )
}

pub fn character_final_review_schema() -> Result<String, crate::CharacterError> {
    crate::schema::<CharacterFinalReview>(
        FINAL_REVIEW_SCHEMA_ID,
        "Weave Character Final Authoring Review v1",
        Some((
            "review_format_version",
            CHARACTER_FINAL_REVIEW_FORMAT_VERSION,
        )),
    )
}

/// Create an empty authoring workspace. Access and template availability remain ordinary project
/// data; the contract contains no account, commerce, tier, or service-entitlement field.
pub fn new_authoring_workspace(
    id: impl Into<String>,
    provenance: Provenance,
) -> Result<CharacterAuthoringWorkspace, crate::CharacterError> {
    let workspace = CharacterAuthoringWorkspace {
        workspace_format_version: CHARACTER_AUTHORING_WORKSPACE_FORMAT_VERSION,
        id: id.into(),
        revision: 0,
        drafts: BTreeMap::new(),
        provenance,
    };
    validate_authoring_workspace(&workspace)?;
    Ok(workspace)
}

/// Validate a complete reopenable workspace and independently synthesize every draft.
pub fn validate_authoring_workspace(
    workspace: &CharacterAuthoringWorkspace,
) -> Result<(), crate::CharacterError> {
    if workspace.workspace_format_version != CHARACTER_AUTHORING_WORKSPACE_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "workspace_format_version",
            "unsupported Character authoring workspace version",
        ));
    }
    validate_namespaced_id("id", &workspace.id)?;
    validate_provenance(&workspace.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "authoring workspace provenance is invalid",
        )
    })?;
    if workspace.drafts.len() > 65_536 {
        return Err(authoring_invalid(
            "drafts",
            "authoring workspace contains too many drafts",
        ));
    }
    for (id, draft) in &workspace.drafts {
        if draft.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("drafts.{id}.id"),
                "draft identifier must equal its containing map key",
            ));
        }
        validate_authoring_draft(draft)?;
    }
    Ok(())
}

/// Canonical SHA-256 identity of one validated workspace.
pub fn authoring_workspace_fingerprint(
    workspace: &CharacterAuthoringWorkspace,
) -> Result<String, crate::CharacterError> {
    validate_authoring_workspace(workspace)?;
    canonical_hash(workspace)
}

/// Add one source-authored template/overlay pair without changing the input workspace.
pub fn create_authoring_draft(
    workspace: &CharacterAuthoringWorkspace,
    template: Option<CharacterTemplate>,
    overlay: CharacterOverlay,
) -> Result<CharacterAuthoringWorkspace, crate::CharacterError> {
    validate_authoring_workspace(workspace)?;
    if let Some(template) = &template {
        validate_public_safe_template(template)?;
    }
    let draft = CharacterAuthoringDraft {
        id: overlay.character_id.clone(),
        revision: 0,
        template,
        overlay,
        questionnaire_receipts: Vec::new(),
        presentation_receipts: Vec::new(),
        template_migrations: Vec::new(),
        unresolved_invalidations: BTreeMap::new(),
        final_review: None,
    };
    validate_authoring_draft(&draft)?;
    if workspace.drafts.contains_key(&draft.id) {
        return Err(error(
            CharacterDiagnosticCode::ConflictingOverlay,
            "draft.id",
            "authoring workspace already contains this stable character id",
        ));
    }
    let mut result = workspace.clone();
    result.drafts.insert(draft.id.clone(), draft);
    result.revision = result
        .revision
        .checked_add(1)
        .ok_or_else(|| authoring_invalid("revision", "authoring workspace revision overflowed"))?;
    validate_authoring_workspace(&result)?;
    Ok(result)
}

/// Clone one draft while retaining the exact template and sparse overlay separately.
///
/// References that cannot safely follow the new character id make synthesis fail before the clone
/// enters the returned workspace.
pub fn clone_authoring_draft(
    workspace: &CharacterAuthoringWorkspace,
    source_id: &str,
    new_character_id: impl Into<String>,
    new_overlay_id: impl Into<String>,
) -> Result<CharacterAuthoringWorkspace, crate::CharacterError> {
    validate_authoring_workspace(workspace)?;
    let source = workspace.drafts.get(source_id).ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "source_id",
            "source authoring draft is absent",
        )
    })?;
    let new_character_id = new_character_id.into();
    if new_character_id == source_id || workspace.drafts.contains_key(&new_character_id) {
        return Err(error(
            CharacterDiagnosticCode::ConflictingOverlay,
            "new_character_id",
            "clone requires a distinct unused stable character id",
        ));
    }
    let mut overlay = source.overlay.clone();
    overlay.id = new_overlay_id.into();
    overlay.character_id.clone_from(&new_character_id);
    let draft = CharacterAuthoringDraft {
        id: new_character_id.clone(),
        revision: 0,
        template: source.template.clone(),
        overlay,
        questionnaire_receipts: Vec::new(),
        presentation_receipts: Vec::new(),
        template_migrations: Vec::new(),
        unresolved_invalidations: BTreeMap::new(),
        final_review: None,
    };
    validate_authoring_draft(&draft)?;
    let mut result = workspace.clone();
    result.drafts.insert(new_character_id, draft);
    result.revision = result
        .revision
        .checked_add(1)
        .ok_or_else(|| authoring_invalid("revision", "authoring workspace revision overflowed"))?;
    validate_authoring_workspace(&result)?;
    Ok(result)
}

/// List deterministic draft summaries.
pub fn list_authoring_drafts(
    workspace: &CharacterAuthoringWorkspace,
) -> Result<Vec<CharacterAuthoringDraftSummary>, crate::CharacterError> {
    validate_authoring_workspace(workspace)?;
    workspace
        .drafts
        .values()
        .map(|draft| {
            let result = synthesize_draft(draft)?;
            Ok(CharacterAuthoringDraftSummary {
                id: draft.id.clone(),
                revision: draft.revision,
                display_name: result
                    .effective_profile
                    .canon
                    .identity
                    .display_name
                    .value
                    .clone(),
                template: draft.overlay.template.clone(),
                unresolved_invalidations: draft.unresolved_invalidations.len(),
                final_reviewed: draft.final_review.as_ref().is_some_and(|review| {
                    review.decision == CharacterFinalReviewDecision::Accepted
                }),
                effective_profile_sha256: profile_fingerprint(&result.effective_profile)?,
            })
        })
        .collect()
}

/// Return one validated draft by stable id.
pub fn show_authoring_draft<'a>(
    workspace: &'a CharacterAuthoringWorkspace,
    id: &str,
) -> Result<&'a CharacterAuthoringDraft, crate::CharacterError> {
    validate_authoring_workspace(workspace)?;
    validate_namespaced_id("id", id)?;
    workspace.drafts.get(id).ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "id",
            "authoring draft is absent from the workspace",
        )
    })
}

/// Export one exact validated effective profile. Accepted final review is required by default.
pub fn export_authoring_profile(
    workspace: &CharacterAuthoringWorkspace,
    id: &str,
    require_final_review: bool,
) -> Result<CharacterProfile, crate::CharacterError> {
    let draft = show_authoring_draft(workspace, id)?;
    if require_final_review
        && draft
            .final_review
            .as_ref()
            .is_none_or(|review| review.decision != CharacterFinalReviewDecision::Accepted)
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidValue,
            "final_review",
            "an accepted final authoring review is required before export",
        ));
    }
    Ok(synthesize_draft(draft)?.effective_profile)
}

/// Validate the standalone shape and allowed mode of one revision.
pub fn validate_authoring_revision(
    revision: &CharacterAuthoringRevision,
) -> Result<(), crate::CharacterError> {
    if revision.revision_format_version != CHARACTER_AUTHORING_REVISION_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "revision_format_version",
            "unsupported Character authoring revision version",
        ));
    }
    validate_namespaced_id("id", &revision.id)?;
    validate_namespaced_id("draft_id", &revision.draft_id)?;
    validate_text("rationale", &revision.rationale, 1, 2_048)?;
    validate_provenance(&revision.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "authoring revision provenance is invalid",
        )
    })?;

    let special_count = usize::from(revision.questionnaire_receipt.is_some())
        + usize::from(revision.presentation_receipt.is_some())
        + usize::from(revision.enrichment.is_some())
        + usize::from(revision.template_migration.is_some());
    if special_count > 1 {
        return Err(authoring_invalid(
            "revision",
            "one revision cannot combine questionnaire, presentation, enrichment, and template migration",
        ));
    }
    match revision.input_mode {
        CharacterAuthoringInputMode::Questionnaire => {
            if revision.questionnaire_receipt.is_none() || !revision.changes.is_empty() {
                return Err(authoring_invalid(
                    "input_mode",
                    "questionnaire revision requires one receipt and no handwritten changes",
                ));
            }
        }
        CharacterAuthoringInputMode::PresentationCatalog => {
            if revision.presentation_receipt.is_none() || !revision.changes.is_empty() {
                return Err(authoring_invalid(
                    "input_mode",
                    "presentation-catalog revision requires one receipt and no handwritten changes",
                ));
            }
        }
        CharacterAuthoringInputMode::ReviewedEnrichment => {
            if revision.enrichment.is_none() || !revision.changes.is_empty() {
                return Err(authoring_invalid(
                    "input_mode",
                    "reviewed enrichment requires one receipt and no canonical changes",
                ));
            }
        }
        CharacterAuthoringInputMode::TemplateMigration => {
            if revision.template_migration.is_none() || !revision.changes.is_empty() {
                return Err(authoring_invalid(
                    "input_mode",
                    "template migration requires one reviewed migration and no field changes",
                ));
            }
        }
        _ => {
            if special_count != 0 || revision.changes.is_empty() {
                return Err(authoring_invalid(
                    "changes",
                    "ordinary authoring revision requires at least one canonical change",
                ));
            }
        }
    }

    let mut prior: Option<(String, &str)> = None;
    for (index, change) in revision.changes.iter().enumerate() {
        let path = format!("changes[{index}]");
        validate_local_id(&format!("{path}.id"), &change.id)?;
        validate_text(&format!("{path}.rationale"), &change.rationale, 1, 2_048)?;
        validate_authoring_action(&path, &change.action, revision.input_mode)?;
        let target = operation_target(&change.action);
        if prior.as_ref().is_some_and(|(prior_target, prior_id)| {
            (prior_target.as_str(), *prior_id) >= (target.as_str(), change.id.as_str())
        }) {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                "changes",
                "authoring changes must be unique and sorted by target path and change id",
            ));
        }
        prior = Some((target, &change.id));
    }

    if let Some(receipt) = &revision.questionnaire_receipt {
        validate_questionnaire_receipt(receipt)?;
    }
    if let Some(receipt) = &revision.presentation_receipt {
        validate_presentation_receipt(receipt)?;
    }
    if let Some(enrichment) = &revision.enrichment {
        validate_reviewed_enrichment(enrichment)?;
    }
    if let Some(migration) = &revision.template_migration {
        validate_sha256(
            "template_migration.prior_template_sha256",
            &migration.prior_template_sha256,
        )?;
        validate_public_safe_template(&migration.template)?;
        validate_text("template_migration.reviewer", &migration.reviewer, 1, 512)?;
        validate_text(
            "template_migration.rationale",
            &migration.rationale,
            1,
            2_048,
        )?;
    }
    Ok(())
}

fn validate_authoring_draft(draft: &CharacterAuthoringDraft) -> Result<(), crate::CharacterError> {
    validate_namespaced_id("draft.id", &draft.id)?;
    if draft.overlay.character_id != draft.id {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "draft.overlay.character_id",
            "overlay character id must equal its owning draft id",
        ));
    }
    if let Some(template) = &draft.template {
        validate_public_safe_template(template)?;
    }
    let result = synthesize_draft(draft)?;
    let mut prior_receipt_id: Option<&str> = None;
    for (index, receipt) in draft.questionnaire_receipts.iter().enumerate() {
        validate_questionnaire_receipt(receipt)?;
        if prior_receipt_id.is_some_and(|prior| prior >= receipt.id.as_str()) {
            return Err(authoring_invalid(
                format!("draft.questionnaire_receipts[{index}]"),
                "questionnaire receipt ids must be unique and sorted",
            ));
        }
        prior_receipt_id = Some(&receipt.id);
    }
    let mut prior_presentation_receipt_id: Option<&str> = None;
    for (index, receipt) in draft.presentation_receipts.iter().enumerate() {
        validate_presentation_receipt(receipt)?;
        if prior_presentation_receipt_id.is_some_and(|prior| prior >= receipt.id.as_str()) {
            return Err(authoring_invalid(
                format!("draft.presentation_receipts[{index}]"),
                "presentation receipt ids must be unique and sorted",
            ));
        }
        prior_presentation_receipt_id = Some(&receipt.id);
    }
    let mut prior_migration: Option<&AppliedCharacterTemplateMigration> = None;
    for (index, migration) in draft.template_migrations.iter().enumerate() {
        validate_template_ref(
            &format!("draft.template_migrations[{index}].prior_template"),
            &migration.prior_template,
        )?;
        validate_template_ref(
            &format!("draft.template_migrations[{index}].template"),
            &migration.template,
        )?;
        if migration.prior_template.id != migration.template.id
            || migration.prior_template.version == migration.template.version
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("draft.template_migrations[{index}]"),
                "template history requires distinct releases of one stable template id",
            ));
        }
        validate_text(
            &format!("draft.template_migrations[{index}].reviewer"),
            &migration.reviewer,
            1,
            512,
        )?;
        validate_text(
            &format!("draft.template_migrations[{index}].rationale"),
            &migration.rationale,
            1,
            2_048,
        )?;
        if migration.applied_draft_revision == 0
            || migration.applied_draft_revision > draft.revision
            || prior_migration.is_some_and(|prior| {
                prior.applied_draft_revision >= migration.applied_draft_revision
                    || prior.template != migration.prior_template
            })
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("draft.template_migrations[{index}].applied_draft_revision"),
                "template history must form one chronological release chain",
            ));
        }
        prior_migration = Some(migration);
    }
    if let Some(last) = draft.template_migrations.last()
        && draft.overlay.template.as_ref() != Some(&last.template)
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "draft.template_migrations",
            "template history does not end at the selected template release",
        ));
    }
    for (id, invalidation) in &draft.unresolved_invalidations {
        validate_local_id("draft.unresolved_invalidations.id", id)?;
        if invalidation.id != *id || invalidation.resolved_in_candidate {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("draft.unresolved_invalidations.{id}"),
                "stored invalidation identity or state is inconsistent",
            ));
        }
        validate_invalidation(invalidation)?;
    }
    if let Some(review) = &draft.final_review {
        validate_final_review_for_draft(review, draft, &result.effective_profile)?;
    }
    Ok(())
}

fn validate_template_ref(
    path: &str,
    template: &CharacterTemplateRef,
) -> Result<(), crate::CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &template.id)?;
    validate_semver(&format!("{path}.version"), &template.version)?;
    validate_sha256(&format!("{path}.sha256"), &template.sha256)
}

fn synthesize_draft(
    draft: &CharacterAuthoringDraft,
) -> Result<crate::CharacterSynthesisResult, crate::CharacterError> {
    synthesize_character(draft.template.as_ref(), &draft.overlay)
}

fn validate_public_safe_template(
    template: &CharacterTemplate,
) -> Result<(), crate::CharacterError> {
    validate_template(template)?;
    for source in &template.profile.provenance.sources {
        if !matches!(source.license.as_str(), "MIT" | "Apache-2.0" | "CC0-1.0") {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "template.profile.provenance.sources.license",
                "authoring templates require an explicitly allowed public-source license",
            ));
        }
        if source.kind == ProvenanceKind::Original && source.license != "MIT" {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "template.profile.provenance.sources.license",
                "original bundled authoring templates use the project MIT license",
            ));
        }
    }
    Ok(())
}

fn validate_authoring_action(
    path: &str,
    action: &CharacterOperationAction,
    mode: CharacterAuthoringInputMode,
) -> Result<(), crate::CharacterError> {
    if matches!(
        mode,
        CharacterAuthoringInputMode::DirectFacets | CharacterAuthoringInputMode::ConcisePicker
    ) && !matches!(
        action,
        CharacterOperationAction::SetHexacoTrait { trait_id, .. }
            if is_hexaco_facet(*trait_id)
    ) {
        return Err(error(
            CharacterDiagnosticCode::InvalidValue,
            format!("{path}.action"),
            "direct-facet and picker revisions may target only the 24 canonical facets",
        ));
    }
    match action {
        CharacterOperationAction::SetDisplayName { value } => {
            reject_placeholder(&format!("{path}.action.value"), &value.value)?;
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::SetAliases { value } => {
            for alias in &value.value {
                reject_placeholder(&format!("{path}.action.value"), alias)?;
            }
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::SetPronouns { value } => {
            for form in [
                &value.value.subject,
                &value.value.object,
                &value.value.possessive_determiner,
                &value.value.possessive_pronoun,
                &value.value.reflexive,
            ] {
                reject_placeholder(&format!("{path}.action.value"), form)?;
            }
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::UpsertIdentityContextNote { record } => {
            reject_placeholder(
                &format!("{path}.action.record.content"),
                &record.content.value,
            )?;
            validate_import_mode(path, record.content.state, mode)
        }
        CharacterOperationAction::UpsertAppearanceDescriptor { record } => {
            reject_placeholder(
                &format!("{path}.action.record.content"),
                &record.content.value,
            )?;
            validate_import_mode(path, record.content.state, mode)
        }
        CharacterOperationAction::SetPresentationPalette { value } => {
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::SetPresentationStyleTags { value } => {
            for tag in &value.value {
                reject_placeholder(&format!("{path}.action.value"), tag)?;
            }
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::UpsertPresentationAsset { value } => {
            reject_placeholder(&format!("{path}.action.value.path"), &value.value.path)?;
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::UpsertPresentationAssignment { value } => {
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::SetBirthDate { value } => {
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::SetHexacoTrait { trait_id, value } => {
            if matches!(
                mode,
                CharacterAuthoringInputMode::DirectFacets
                    | CharacterAuthoringInputMode::ConcisePicker
            ) && !is_hexaco_facet(*trait_id)
            {
                return Err(authoring_invalid(
                    format!("{path}.action.trait_id"),
                    "guided facet modes cannot set a factor directly",
                ));
            }
            validate_import_mode(path, value.state, mode)
        }
        CharacterOperationAction::UpsertInnerLife { record } => {
            reject_placeholder(
                &format!("{path}.action.record.content"),
                &record.content.value,
            )?;
            validate_import_mode(path, record.content.state, mode)
        }
        CharacterOperationAction::UpsertVoice { record } => {
            reject_placeholder(
                &format!("{path}.action.record.content"),
                &record.content.value,
            )?;
            validate_import_mode(path, record.content.state, mode)
        }
        CharacterOperationAction::ClearAliases
        | CharacterOperationAction::ClearPronouns
        | CharacterOperationAction::RemoveIdentityContextNote { .. }
        | CharacterOperationAction::RemoveAppearanceDescriptor { .. }
        | CharacterOperationAction::ClearPresentationPalette
        | CharacterOperationAction::ClearPresentationStyleTags
        | CharacterOperationAction::RemovePresentationAsset { .. }
        | CharacterOperationAction::RemovePresentationAssignment { .. }
        | CharacterOperationAction::ClearBirthDate
        | CharacterOperationAction::ClearHexacoTrait { .. }
        | CharacterOperationAction::RemoveInnerLife { .. }
        | CharacterOperationAction::RemoveVoice { .. } => {
            if mode == CharacterAuthoringInputMode::ImportedProfile {
                return Err(authoring_invalid(
                    format!("{path}.action"),
                    "imported profile revisions cannot encode absence as a destructive clear",
                ));
            }
            Ok(())
        }
        CharacterOperationAction::UpsertSuggestion { .. }
        | CharacterOperationAction::RemoveSuggestion { .. }
        | CharacterOperationAction::UpsertExtension { .. }
        | CharacterOperationAction::RemoveExtension { .. } => Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            format!("{path}.action"),
            "guided canonical edits cannot modify suggestion, derived, extension, or pack-owned data",
        )),
    }
}

fn validate_import_mode(
    path: &str,
    state: ValueState,
    mode: CharacterAuthoringInputMode,
) -> Result<(), crate::CharacterError> {
    if mode == CharacterAuthoringInputMode::ImportedProfile && state != ValueState::Imported {
        return Err(authoring_invalid(
            format!("{path}.action.value.state"),
            "imported profile fields must retain the imported authority state",
        ));
    }
    if mode != CharacterAuthoringInputMode::ImportedProfile && state == ValueState::Imported {
        return Err(authoring_invalid(
            format!("{path}.action.value.state"),
            "imported authority is accepted only through imported-profile mode",
        ));
    }
    Ok(())
}

fn reject_placeholder(path: &str, value: &str) -> Result<(), crate::CharacterError> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "unknown" | "<unknown>" | "tbd" | "<tbd>" | "???" | "unknown_placeholder"
    ) {
        return Err(authoring_invalid(
            path,
            "unknown placeholders must be represented by an omitted optional field",
        ));
    }
    Ok(())
}

fn is_hexaco_facet(trait_id: HexacoTrait) -> bool {
    ALL_HEXACO_FACETS.binary_search(&trait_id).is_ok()
}

fn validate_reviewed_enrichment(
    enrichment: &CharacterReviewedEnrichment,
) -> Result<(), crate::CharacterError> {
    match enrichment {
        CharacterReviewedEnrichment::Alignment(receipt) => validate_alignment_receipt(receipt),
        CharacterReviewedEnrichment::DateContext(receipt) => {
            validate_temporal_context_receipt(receipt)
        }
    }
}

fn validate_invalidation(
    invalidation: &CharacterAuthoringInvalidation,
) -> Result<(), crate::CharacterError> {
    validate_local_id("invalidation.id", &invalidation.id)?;
    validate_text("invalidation.message", &invalidation.message, 1, 1_024)?;
    if invalidation.input_paths.is_empty()
        || invalidation
            .input_paths
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(authoring_invalid(
            "invalidation.input_paths",
            "invalidation input paths must be non-empty, unique, and sorted",
        ));
    }
    Ok(())
}

fn authoring_invalid(path: impl Into<String>, message: &'static str) -> crate::CharacterError {
    error(CharacterDiagnosticCode::InvalidValue, path, message)
}

fn encoding_error() -> crate::CharacterError {
    error(
        CharacterDiagnosticCode::InvalidEncoding,
        "document",
        "guided Character document does not match the strict serialized contract",
    )
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, crate::CharacterError> {
    let bytes = to_pretty_json(value)
        .map_err(|_| encoding_error())?
        .into_bytes();
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn profile_fingerprint(profile: &CharacterProfile) -> Result<String, crate::CharacterError> {
    validate_profile(profile)?;
    canonical_hash(profile)
}

/// Validate one independently distributable original narrative-questionnaire pack.
pub fn validate_questionnaire_pack(
    pack: &CharacterQuestionnairePack,
) -> Result<(), crate::CharacterError> {
    if pack.pack_format_version != CHARACTER_QUESTIONNAIRE_PACK_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "pack_format_version",
            "unsupported Character questionnaire pack version",
        ));
    }
    validate_namespaced_id("id", &pack.id)?;
    validate_semver("version", &pack.version)?;
    validate_safe_authoring_text("title", &pack.title, 1, 256)?;
    validate_safe_authoring_text("methodology", &pack.methodology, 1, 8_192)?;
    if pack.limitations.is_empty() || pack.limitations.len() > 64 {
        return Err(authoring_invalid(
            "limitations",
            "questionnaire pack requires between 1 and 64 limitations",
        ));
    }
    let mut prior_limitation: Option<&str> = None;
    for (index, limitation) in pack.limitations.iter().enumerate() {
        validate_safe_authoring_text(&format!("limitations[{index}]"), limitation, 1, 2_048)?;
        if prior_limitation.is_some_and(|prior| prior >= limitation.as_str()) {
            return Err(authoring_invalid(
                "limitations",
                "questionnaire limitations must be unique and sorted",
            ));
        }
        prior_limitation = Some(limitation);
    }
    if !pack.narrative_authoring_only || !pack.independently_authored_prompts {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "pack",
            "questionnaire packs must declare original narrative-authoring-only prompts",
        ));
    }
    if !allowed_authoring_license(&pack.license) {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "license",
            "questionnaire pack license is outside the authoring allowlist",
        ));
    }
    validate_public_https("license_url", &pack.license_url)?;
    validate_provenance(&pack.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "questionnaire pack provenance is invalid",
        )
    })?;
    for source in &pack.provenance.sources {
        if !allowed_authoring_license(&source.license)
            || (source.kind == ProvenanceKind::Original && source.license != "MIT")
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance.sources.license",
                "questionnaire sources require MIT original material or an allowed public license",
            ));
        }
    }

    if pack.items.len() < ALL_HEXACO_FACETS.len() || pack.items.len() > 1_024 {
        return Err(authoring_invalid(
            "items",
            "questionnaire pack must cover all 24 facets with a bounded prompt set",
        ));
    }
    let mut coverage = BTreeSet::new();
    for (id, item) in &pack.items {
        validate_local_id("items.id", id)?;
        if item.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("items.{id}.id"),
                "questionnaire item id must equal its containing map key",
            ));
        }
        validate_safe_authoring_text(&format!("items.{id}.prompt"), &item.prompt, 1, 2_048)?;
        if item.facet_weights_micros.is_empty() || item.facet_weights_micros.len() > 24 {
            return Err(authoring_invalid(
                format!("items.{id}.facet_weights_micros"),
                "questionnaire item requires between 1 and 24 facet weights",
            ));
        }
        for (trait_id, weight) in &item.facet_weights_micros {
            if !is_hexaco_facet(*trait_id)
                || *weight == 0
                || !(-1_000_000..=1_000_000).contains(weight)
            {
                return Err(authoring_invalid(
                    format!("items.{id}.facet_weights_micros"),
                    "questionnaire weights must be non-zero signed millionths for canonical facets",
                ));
            }
            coverage.insert(*trait_id);
        }
    }
    if coverage != ALL_HEXACO_FACETS.into_iter().collect() {
        return Err(authoring_invalid(
            "items.facet_weights_micros",
            "questionnaire pack must provide traceable coverage for all 24 facets",
        ));
    }

    if pack.conflict_rules.len() > 1_024 {
        return Err(authoring_invalid(
            "conflict_rules",
            "questionnaire pack contains too many conflict rules",
        ));
    }
    for (id, rule) in &pack.conflict_rules {
        validate_local_id("conflict_rules.id", id)?;
        if rule.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("conflict_rules.{id}.id"),
                "conflict rule id must equal its containing map key",
            ));
        }
        validate_safe_authoring_text(&format!("conflict_rules.{id}.label"), &rule.label, 1, 256)?;
        validate_safe_authoring_text(
            &format!("conflict_rules.{id}.explanation"),
            &rule.explanation,
            1,
            2_048,
        )?;
        if rule.predicates.len() < 2 || rule.predicates.len() > 24 {
            return Err(authoring_invalid(
                format!("conflict_rules.{id}.predicates"),
                "cross-axis conflict rule requires between 2 and 24 predicates",
            ));
        }
        let mut predicate_traits = BTreeSet::new();
        for predicate in &rule.predicates {
            if !is_hexaco_facet(predicate.trait_id)
                || predicate.threshold_micros > 1_000_000
                || !predicate_traits.insert(predicate.trait_id)
            {
                return Err(authoring_invalid(
                    format!("conflict_rules.{id}.predicates"),
                    "conflict predicates require unique canonical facets and bounded thresholds",
                ));
            }
        }
        if rule.rejected_traits.is_empty()
            || rule
                .rejected_traits
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || rule
                .rejected_traits
                .iter()
                .any(|trait_id| !predicate_traits.contains(trait_id))
        {
            return Err(authoring_invalid(
                format!("conflict_rules.{id}.rejected_traits"),
                "rejected traits must be a non-empty sorted subset of conflict predicates",
            ));
        }
    }
    Ok(())
}

/// Canonical content fingerprint for one validated questionnaire pack.
pub fn questionnaire_pack_fingerprint(
    pack: &CharacterQuestionnairePack,
) -> Result<String, crate::CharacterError> {
    validate_questionnaire_pack(pack)?;
    canonical_hash(pack)
}

/// Validate a standalone answer set. Exact item references are checked when scoring against a pack.
pub fn validate_questionnaire_answers(
    answers: &CharacterQuestionnaireAnswers,
) -> Result<(), crate::CharacterError> {
    if answers.answers_format_version != CHARACTER_QUESTIONNAIRE_ANSWERS_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "answers_format_version",
            "unsupported Character questionnaire answers version",
        ));
    }
    validate_namespaced_id("id", &answers.id)?;
    validate_questionnaire_pack_ref("pack", &answers.pack)?;
    if answers.responses.len() > 1_024 {
        return Err(authoring_invalid(
            "responses",
            "questionnaire answer set contains too many responses",
        ));
    }
    for (id, response) in &answers.responses {
        validate_local_id("responses.id", id)?;
        if !(-2..=2).contains(response) {
            return Err(authoring_invalid(
                format!("responses.{id}"),
                "questionnaire responses must be integers from -2 through 2",
            ));
        }
    }
    Ok(())
}

/// Deterministically score original behavior prompts without changing the profile.
pub fn propose_character_questionnaire(
    profile: &CharacterProfile,
    pack: &CharacterQuestionnairePack,
    answers: &CharacterQuestionnaireAnswers,
    seed: u64,
) -> Result<CharacterQuestionnaireProposal, crate::CharacterError> {
    validate_profile(profile)?;
    validate_questionnaire_pack(pack)?;
    validate_questionnaire_answers(answers)?;
    let pack_ref = questionnaire_pack_ref(pack)?;
    if answers.pack != pack_ref {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "answers.pack",
            "questionnaire answers do not pin the exact selected pack",
        ));
    }
    if answers
        .responses
        .keys()
        .any(|id| !pack.items.contains_key(id))
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "answers.responses",
            "questionnaire answers contain an item absent from the selected pack",
        ));
    }
    let (facets, conflicts) = score_questionnaire_outputs(pack, answers)?;
    let proposal = CharacterQuestionnaireProposal {
        proposal_format_version: CHARACTER_QUESTIONNAIRE_PROPOSAL_FORMAT_VERSION,
        id: format!("{}.proposal", answers.id),
        input_profile: profile.clone(),
        input_profile_sha256: profile_fingerprint(profile)?,
        pack: pack.clone(),
        pack_ref,
        answers: answers.clone(),
        seed,
        facets,
        conflicts,
    };
    validate_questionnaire_proposal_structure(&proposal)?;
    Ok(proposal)
}

/// Validate and independently reproduce a serialized questionnaire proposal.
pub fn validate_questionnaire_proposal(
    proposal: &CharacterQuestionnaireProposal,
) -> Result<(), crate::CharacterError> {
    validate_questionnaire_proposal_structure(proposal)?;
    let expected = propose_character_questionnaire(
        &proposal.input_profile,
        &proposal.pack,
        &proposal.answers,
        proposal.seed,
    )?;
    if expected != *proposal {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal",
            "questionnaire proposal does not reproduce from its exact inputs",
        ));
    }
    Ok(())
}

/// Canonical proposal fingerprint used by reviews.
pub fn questionnaire_proposal_fingerprint(
    proposal: &CharacterQuestionnaireProposal,
) -> Result<String, crate::CharacterError> {
    validate_questionnaire_proposal(proposal)?;
    canonical_hash(proposal)
}

/// Create a complete per-facet and per-conflict review.
pub fn create_character_questionnaire_review(
    proposal: &CharacterQuestionnaireProposal,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
    facet_decisions: BTreeMap<HexacoTrait, CharacterQuestionnaireFacetDecision>,
    conflict_decisions: BTreeMap<String, CharacterQuestionnaireConflictDecision>,
) -> Result<CharacterQuestionnaireReview, crate::CharacterError> {
    validate_questionnaire_proposal(proposal)?;
    let review = CharacterQuestionnaireReview {
        review_format_version: CHARACTER_QUESTIONNAIRE_REVIEW_FORMAT_VERSION,
        proposal_sha256: questionnaire_proposal_fingerprint(proposal)?,
        reviewer: reviewer.into(),
        rationale: rationale.into(),
        facet_decisions,
        conflict_decisions,
    };
    validate_questionnaire_review_for_proposal(&review, proposal)?;
    Ok(review)
}

/// Apply one complete review through the same attributed-field and overlay validators used by
/// direct editing and concise picker authoring.
pub fn apply_character_questionnaire_review(
    proposal: &CharacterQuestionnaireProposal,
    review: &CharacterQuestionnaireReview,
) -> Result<CharacterQuestionnaireReceipt, crate::CharacterError> {
    validate_questionnaire_proposal(proposal)?;
    validate_questionnaire_review_for_proposal(review, proposal)?;
    let receipt = build_questionnaire_receipt(proposal, review)?;
    validate_questionnaire_receipt_structure(&receipt)?;
    Ok(receipt)
}

/// Independently reproduce a serialized questionnaire receipt.
pub fn validate_questionnaire_receipt(
    receipt: &CharacterQuestionnaireReceipt,
) -> Result<(), crate::CharacterError> {
    validate_questionnaire_receipt_structure(receipt)?;
    let expected = apply_character_questionnaire_review(&receipt.proposal, &receipt.review)?;
    if expected != *receipt {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "receipt",
            "questionnaire receipt does not reproduce from its exact proposal and review",
        ));
    }
    Ok(())
}

fn validate_questionnaire_proposal_structure(
    proposal: &CharacterQuestionnaireProposal,
) -> Result<(), crate::CharacterError> {
    if proposal.proposal_format_version != CHARACTER_QUESTIONNAIRE_PROPOSAL_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "proposal_format_version",
            "unsupported Character questionnaire proposal version",
        ));
    }
    validate_namespaced_id("id", &proposal.id)?;
    validate_profile(&proposal.input_profile)?;
    validate_sha256("input_profile_sha256", &proposal.input_profile_sha256)?;
    if profile_fingerprint(&proposal.input_profile)? != proposal.input_profile_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "input_profile_sha256",
            "questionnaire proposal profile fingerprint is stale",
        ));
    }
    validate_questionnaire_pack(&proposal.pack)?;
    validate_questionnaire_pack_ref("pack_ref", &proposal.pack_ref)?;
    if questionnaire_pack_ref(&proposal.pack)? != proposal.pack_ref {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "pack_ref",
            "questionnaire proposal pack fingerprint is stale",
        ));
    }
    validate_questionnaire_answers(&proposal.answers)?;
    if proposal.answers.pack != proposal.pack_ref {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "answers.pack",
            "questionnaire proposal answers select another pack",
        ));
    }
    let expected_traits: BTreeSet<_> = ALL_HEXACO_FACETS.into_iter().collect();
    if proposal.facets.keys().copied().collect::<BTreeSet<_>>() != expected_traits {
        return Err(authoring_invalid(
            "facets",
            "questionnaire proposal must expose all 24 facets",
        ));
    }
    for (trait_id, facet) in &proposal.facets {
        if facet.trait_id != *trait_id
            || facet.score_micros.is_some_and(|score| score > 1_000_000)
            || facet.answered_items > facet.total_items
            || facet.total_items == 0
        {
            return Err(authoring_invalid(
                "facets",
                "questionnaire facet proposal contains inconsistent score or coverage data",
            ));
        }
        if facet.trace.len() != facet.total_items {
            return Err(authoring_invalid(
                "facets.trace",
                "questionnaire trace must retain every contributing pack item",
            ));
        }
    }
    for (id, conflict) in &proposal.conflicts {
        validate_local_id("conflicts.id", id)?;
        if conflict.id != *id || conflict.rule_id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("conflicts.{id}"),
                "questionnaire conflict identity must match its triggering rule",
            ));
        }
    }
    Ok(())
}

fn validate_questionnaire_review_structure(
    review: &CharacterQuestionnaireReview,
) -> Result<(), crate::CharacterError> {
    if review.review_format_version != CHARACTER_QUESTIONNAIRE_REVIEW_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "review_format_version",
            "unsupported Character questionnaire review version",
        ));
    }
    validate_sha256("proposal_sha256", &review.proposal_sha256)?;
    validate_safe_authoring_text("reviewer", &review.reviewer, 1, 512)?;
    validate_safe_authoring_text("rationale", &review.rationale, 1, 2_048)?;
    if review.facet_decisions.len() > 24 || review.conflict_decisions.len() > 1_024 {
        return Err(authoring_invalid(
            "review",
            "questionnaire review contains too many decisions",
        ));
    }
    Ok(())
}

fn validate_questionnaire_review_for_proposal(
    review: &CharacterQuestionnaireReview,
    proposal: &CharacterQuestionnaireProposal,
) -> Result<(), crate::CharacterError> {
    validate_questionnaire_review_structure(review)?;
    if review.proposal_sha256 != questionnaire_proposal_fingerprint(proposal)? {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal_sha256",
            "questionnaire review does not fingerprint the exact proposal",
        ));
    }
    let expected_traits: BTreeSet<_> = ALL_HEXACO_FACETS.into_iter().collect();
    if review
        .facet_decisions
        .keys()
        .copied()
        .collect::<BTreeSet<_>>()
        != expected_traits
    {
        return Err(authoring_invalid(
            "facet_decisions",
            "questionnaire review requires exactly one decision for every facet",
        ));
    }
    if review.conflict_decisions.keys().collect::<BTreeSet<_>>()
        != proposal.conflicts.keys().collect::<BTreeSet<_>>()
    {
        return Err(authoring_invalid(
            "conflict_decisions",
            "questionnaire review requires exactly one decision for every conflict",
        ));
    }
    for (trait_id, decision) in &review.facet_decisions {
        let proposed = &proposal.facets[trait_id];
        match decision {
            CharacterQuestionnaireFacetDecision::Accept { confidence, .. } => {
                if proposed.score_micros.is_none()
                    || *confidence == Confidence::Unknown
                    || *confidence > proposed.confidence
                {
                    return Err(authoring_invalid(
                        "facet_decisions.accept",
                        "accepted facet requires a score and cannot increase computed confidence",
                    ));
                }
            }
            CharacterQuestionnaireFacetDecision::Edit {
                value,
                confidence,
                rationale,
                ..
            }
            | CharacterQuestionnaireFacetDecision::Override {
                value,
                confidence,
                rationale,
                ..
            } => {
                validate_trait_measurement("facet_decisions.value", *value)?;
                if *confidence == Confidence::Unknown {
                    return Err(authoring_invalid(
                        "facet_decisions.confidence",
                        "edited and overridden facets require explicit confidence",
                    ));
                }
                validate_safe_authoring_text("facet_decisions.rationale", rationale, 1, 2_048)?;
            }
            CharacterQuestionnaireFacetDecision::Reject { rationale }
            | CharacterQuestionnaireFacetDecision::Withhold { rationale } => {
                validate_safe_authoring_text("facet_decisions.rationale", rationale, 1, 2_048)?;
            }
        }
    }
    for (id, decision) in &review.conflict_decisions {
        let conflict = &proposal.conflicts[id];
        match decision {
            CharacterQuestionnaireConflictDecision::Accept { rationale } => {
                if conflict.rationale_required && rationale.is_none() {
                    return Err(authoring_invalid(
                        "conflict_decisions.rationale",
                        "this conflict policy requires an acceptance rationale",
                    ));
                }
                if let Some(rationale) = rationale {
                    validate_safe_authoring_text(
                        "conflict_decisions.rationale",
                        rationale,
                        1,
                        2_048,
                    )?;
                }
            }
            CharacterQuestionnaireConflictDecision::Reject { rationale }
            | CharacterQuestionnaireConflictDecision::DeliberateException { rationale } => {
                validate_safe_authoring_text("conflict_decisions.rationale", rationale, 1, 2_048)?;
            }
        }
    }
    Ok(())
}

fn validate_questionnaire_receipt_structure(
    receipt: &CharacterQuestionnaireReceipt,
) -> Result<(), crate::CharacterError> {
    if receipt.receipt_format_version != CHARACTER_QUESTIONNAIRE_RECEIPT_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "receipt_format_version",
            "unsupported Character questionnaire receipt version",
        ));
    }
    validate_namespaced_id("id", &receipt.id)?;
    validate_questionnaire_proposal(&receipt.proposal)?;
    validate_questionnaire_review_for_proposal(&receipt.review, &receipt.proposal)?;
    validate_sha256("review_sha256", &receipt.review_sha256)?;
    if canonical_hash(&receipt.review)? != receipt.review_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "review_sha256",
            "questionnaire receipt review fingerprint is stale",
        ));
    }
    if receipt
        .applied_traits
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || receipt
            .rejected_traits
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || receipt
            .applied_traits
            .iter()
            .chain(&receipt.rejected_traits)
            .any(|trait_id| !is_hexaco_facet(*trait_id))
    {
        return Err(authoring_invalid(
            "receipt.traits",
            "receipt trait lists must be unique, sorted canonical facets",
        ));
    }
    validate_profile(&receipt.output_profile)?;
    validate_sha256("output_profile_sha256", &receipt.output_profile_sha256)?;
    if profile_fingerprint(&receipt.output_profile)? != receipt.output_profile_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "output_profile_sha256",
            "questionnaire receipt output fingerprint is stale",
        ));
    }
    Ok(())
}

type QuestionnaireOutputs = (
    BTreeMap<HexacoTrait, CharacterQuestionnaireFacetProposal>,
    BTreeMap<String, CharacterQuestionnaireConflict>,
);

fn score_questionnaire_outputs(
    pack: &CharacterQuestionnairePack,
    answers: &CharacterQuestionnaireAnswers,
) -> Result<QuestionnaireOutputs, crate::CharacterError> {
    let mut facets = BTreeMap::new();
    for trait_id in ALL_HEXACO_FACETS {
        let mut trace = Vec::new();
        let mut numerator = 0_i128;
        let mut denominator = 0_i128;
        let mut answered_items = 0_usize;
        for (item_id, item) in &pack.items {
            let Some(weight) = item.facet_weights_micros.get(&trait_id) else {
                continue;
            };
            let response = answers.responses.get(item_id).copied();
            let weighted_contribution =
                response.map(|response| i64::from(response) * i64::from(*weight));
            if let Some(contribution) = weighted_contribution {
                numerator += i128::from(contribution);
                denominator += i128::from(weight.unsigned_abs()) * 2;
                answered_items += 1;
            }
            trace.push(CharacterQuestionnaireContribution {
                item_id: item_id.clone(),
                response,
                weight_micros: *weight,
                weighted_contribution,
            });
        }
        let total_items = trace.len();
        let score_micros = if denominator == 0 {
            None
        } else {
            let signed_micros = divide_round_nearest(numerator * 1_000_000, denominator);
            let centered = (signed_micros + 1_000_000) / 2;
            Some(u32::try_from(centered.clamp(0, 1_000_000)).map_err(|_| {
                authoring_invalid("facets.score_micros", "questionnaire score overflowed")
            })?)
        };
        let confidence = match (answered_items, total_items) {
            (0, _) => Confidence::Unknown,
            (answered, total) if answered < total => Confidence::Low,
            (_, 1) => Confidence::Moderate,
            _ => Confidence::High,
        };
        facets.insert(
            trait_id,
            CharacterQuestionnaireFacetProposal {
                trait_id,
                score_micros,
                confidence,
                answered_items,
                total_items,
                trace,
            },
        );
    }

    let mut conflicts = BTreeMap::new();
    for (id, rule) in &pack.conflict_rules {
        let triggered = rule.predicates.iter().all(|predicate| {
            facets
                .get(&predicate.trait_id)
                .and_then(|facet| facet.score_micros)
                .is_some_and(|score| match predicate.comparison {
                    CharacterQuestionnaireComparison::AtLeast => {
                        score >= predicate.threshold_micros
                    }
                    CharacterQuestionnaireComparison::AtMost => score <= predicate.threshold_micros,
                })
        });
        if triggered {
            let mut input_paths = rule
                .predicates
                .iter()
                .map(|predicate| trait_path(predicate.trait_id))
                .collect::<Vec<_>>();
            input_paths.sort();
            conflicts.insert(
                id.clone(),
                CharacterQuestionnaireConflict {
                    id: id.clone(),
                    rule_id: id.clone(),
                    label: rule.label.clone(),
                    explanation: rule.explanation.clone(),
                    input_paths,
                    rejected_traits: rule.rejected_traits.clone(),
                    rationale_required: rule.rationale_required,
                },
            );
        }
    }
    Ok((facets, conflicts))
}

fn build_questionnaire_receipt(
    proposal: &CharacterQuestionnaireProposal,
    review: &CharacterQuestionnaireReview,
) -> Result<CharacterQuestionnaireReceipt, crate::CharacterError> {
    let rejected_by_conflict = review
        .conflict_decisions
        .iter()
        .filter(|(_, decision)| {
            matches!(
                decision,
                CharacterQuestionnaireConflictDecision::Reject { .. }
            )
        })
        .flat_map(|(id, _)| proposal.conflicts[id].rejected_traits.iter().copied())
        .collect::<BTreeSet<_>>();
    let lineage = questionnaire_lineage(&proposal.pack);
    let mut operations = Vec::new();
    let mut applied_traits = Vec::new();
    let mut rejected_traits = rejected_by_conflict.clone();
    for trait_id in ALL_HEXACO_FACETS {
        let decision = &review.facet_decisions[&trait_id];
        if rejected_by_conflict.contains(&trait_id) {
            continue;
        }
        let attributed = match decision {
            CharacterQuestionnaireFacetDecision::Accept { confidence, lock } => {
                let score_micros = proposal.facets[&trait_id].score_micros.ok_or_else(|| {
                    authoring_invalid(
                        "facet_decisions.accept",
                        "cannot accept a facet without a computed score",
                    )
                })?;
                Some(Attributed {
                    value: TraitMeasurement::Score {
                        score: f64::from(score_micros) / 1_000_000.0,
                    },
                    state: ValueState::Reviewed,
                    confidence: *confidence,
                    review: ReviewState::Accepted,
                    lock: *lock,
                    freshness: Freshness::Current,
                    lineage: lineage.clone(),
                    rationale: Some(
                        "Accepted from a reviewed original narrative questionnaire.".to_owned(),
                    ),
                })
            }
            CharacterQuestionnaireFacetDecision::Edit {
                value,
                confidence,
                lock,
                rationale,
            } => Some(Attributed {
                value: *value,
                state: ValueState::Authored,
                confidence: *confidence,
                review: ReviewState::NotRequired,
                lock: *lock,
                freshness: Freshness::Current,
                lineage: lineage.clone(),
                rationale: Some(rationale.clone()),
            }),
            CharacterQuestionnaireFacetDecision::Override {
                value,
                confidence,
                lock,
                rationale,
            } => Some(Attributed {
                value: *value,
                state: ValueState::Overridden,
                confidence: *confidence,
                review: ReviewState::Accepted,
                lock: *lock,
                freshness: Freshness::Current,
                lineage: lineage.clone(),
                rationale: Some(rationale.clone()),
            }),
            CharacterQuestionnaireFacetDecision::Reject { .. }
            | CharacterQuestionnaireFacetDecision::Withhold { .. } => {
                rejected_traits.insert(trait_id);
                None
            }
        };
        let Some(value) = attributed else {
            continue;
        };
        let action = CharacterOperationAction::SetHexacoTrait { trait_id, value };
        operations.push(CharacterOperation {
            id: format!("questionnaire_{}", trait_slug(trait_id)),
            expected_prior_sha256: current_value_hash(&proposal.input_profile, &action)?,
            rationale: "Apply one explicitly reviewed questionnaire facet.".to_owned(),
            action,
        });
        applied_traits.push(trait_id);
    }
    operations.sort_by(|left, right| {
        operation_target(&left.action)
            .cmp(&operation_target(&right.action))
            .then_with(|| left.id.cmp(&right.id))
    });
    applied_traits.sort();
    let rejected_traits = rejected_traits.into_iter().collect::<Vec<_>>();

    let output_profile = if operations.is_empty() {
        proposal.input_profile.clone()
    } else {
        let template = CharacterTemplate {
            template_format_version: crate::CHARACTER_TEMPLATE_FORMAT_VERSION,
            id: "org.weave.character.template.questionnaire_input".to_owned(),
            version: "1.0.0".to_owned(),
            profile: proposal.input_profile.clone(),
        };
        let overlay = CharacterOverlay {
            overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
            id: format!("{}.apply", proposal.id),
            character_id: proposal.input_profile.id.clone(),
            template: Some(CharacterTemplateRef {
                id: template.id.clone(),
                version: template.version.clone(),
                sha256: template_fingerprint(&template)?,
            }),
            operations: operations.clone(),
            provenance: proposal.pack.provenance.clone(),
        };
        synthesize_character(Some(&template), &overlay)?.effective_profile
    };
    Ok(CharacterQuestionnaireReceipt {
        receipt_format_version: CHARACTER_QUESTIONNAIRE_RECEIPT_FORMAT_VERSION,
        id: format!("{}.receipt", proposal.id),
        proposal: proposal.clone(),
        review: review.clone(),
        review_sha256: canonical_hash(review)?,
        operations,
        applied_traits,
        rejected_traits,
        output_profile_sha256: profile_fingerprint(&output_profile)?,
        output_profile,
    })
}

fn questionnaire_pack_ref(
    pack: &CharacterQuestionnairePack,
) -> Result<CharacterQuestionnairePackRef, crate::CharacterError> {
    Ok(CharacterQuestionnairePackRef {
        id: pack.id.clone(),
        version: pack.version.clone(),
        sha256: questionnaire_pack_fingerprint(pack)?,
    })
}

fn validate_questionnaire_pack_ref(
    path: &str,
    pack: &CharacterQuestionnairePackRef,
) -> Result<(), crate::CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &pack.id)?;
    validate_semver(&format!("{path}.version"), &pack.version)?;
    validate_sha256(&format!("{path}.sha256"), &pack.sha256)
}

fn questionnaire_lineage(pack: &CharacterQuestionnairePack) -> Vec<String> {
    pack.provenance
        .sources
        .iter()
        .map(|source| source.id.clone())
        .collect()
}

fn validate_trait_measurement(
    path: &str,
    value: TraitMeasurement,
) -> Result<(), crate::CharacterError> {
    if let TraitMeasurement::Score { score } = value
        && (!score.is_finite() || !(0.0..=1.0).contains(&score))
    {
        return Err(authoring_invalid(
            path,
            "trait score must be finite and range from 0.0 through 1.0",
        ));
    }
    Ok(())
}

fn validate_safe_authoring_text(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), crate::CharacterError> {
    validate_text(path, value, minimum, maximum)?;
    let lower = value.to_ascii_lowercase();
    if lower.contains(concat!("github_", "pat_"))
        || lower.contains(concat!("gh", "p_"))
        || (lower.contains("-----begin") && lower.contains(concat!("private ", "key-----")))
        || lower.contains(concat!("authorization:", " bearer "))
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidValue,
            path,
            "authoring text resembles a secret or credential",
        ));
    }
    Ok(())
}

fn validate_public_https(path: &str, value: &str) -> Result<(), crate::CharacterError> {
    if !value.starts_with("https://")
        || value.contains('@')
        || value.contains(['\r', '\n'])
        || value.len() > 2_048
    {
        return Err(authoring_invalid(
            path,
            "expected a public HTTPS URL without credentials",
        ));
    }
    Ok(())
}

fn allowed_authoring_license(license: &str) -> bool {
    matches!(license, "MIT" | "Apache-2.0" | "CC0-1.0")
}

fn divide_round_nearest(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    if numerator >= 0 {
        (numerator + denominator / 2) / denominator
    } else {
        (numerator - denominator / 2) / denominator
    }
}

fn trait_slug(trait_id: HexacoTrait) -> String {
    trait_path(trait_id)
        .rsplit('.')
        .next()
        .unwrap_or("trait")
        .to_owned()
}

/// Inspect separately retained template values and effective sparse overrides.
pub fn inspect_authoring_fields(
    draft: &CharacterAuthoringDraft,
) -> Result<Vec<CharacterAuthoringFieldView>, crate::CharacterError> {
    validate_authoring_draft(draft)?;
    let result = synthesize_draft(draft)?;
    build_field_views(
        draft,
        draft.template.as_ref().map(|template| &template.profile),
        &result.effective_profile,
        &result.origins,
    )
}

/// Reproduce the current effective profile for an editor or text inspection.
pub fn effective_authoring_profile(
    draft: &CharacterAuthoringDraft,
) -> Result<CharacterProfile, crate::CharacterError> {
    validate_authoring_draft(draft)?;
    Ok(synthesize_draft(draft)?.effective_profile)
}

/// Dry-run one revision and return the exact candidate draft plus all changes and invalidations.
pub fn preview_authoring_revision(
    workspace: &CharacterAuthoringWorkspace,
    revision: &CharacterAuthoringRevision,
) -> Result<CharacterAuthoringPreview, crate::CharacterError> {
    validate_authoring_workspace(workspace)?;
    validate_authoring_revision(revision)?;
    let draft = workspace.drafts.get(&revision.draft_id).ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "draft_id",
            "authoring revision targets an absent draft",
        )
    })?;
    if draft.revision != revision.expected_draft_revision {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "expected_draft_revision",
            "authoring revision does not target the current draft revision",
        ));
    }
    let before_result = synthesize_draft(draft)?;
    let before_profile = &before_result.effective_profile;
    let before_template_profile = draft.template.as_ref().map(|template| &template.profile);
    let mut candidate = draft.clone();
    let mut expected_enrichment_output = None;

    if let Some(migration) = &revision.template_migration {
        apply_template_migration(&mut candidate, migration)?;
    } else if let Some(receipt) = &revision.questionnaire_receipt {
        if receipt.proposal.input_profile != *before_profile {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                "questionnaire_receipt.proposal.input_profile",
                "questionnaire receipt does not target the current effective profile",
            ));
        }
        candidate.overlay.provenance = merge_provenance(
            &candidate.overlay.provenance,
            &receipt.proposal.pack.provenance,
        )?;
        for operation in &receipt.operations {
            upsert_template_relative_operation(
                &mut candidate,
                operation.id.clone(),
                operation.rationale.clone(),
                operation.action.clone(),
            )?;
        }
        if candidate
            .questionnaire_receipts
            .iter()
            .any(|existing| existing.id == receipt.id)
        {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                "questionnaire_receipt.id",
                "questionnaire receipt was already recorded by this draft",
            ));
        }
        candidate.questionnaire_receipts.push((**receipt).clone());
        candidate
            .questionnaire_receipts
            .sort_by(|left, right| left.id.cmp(&right.id));
        expected_enrichment_output = Some(receipt.output_profile.clone());
    } else if let Some(receipt) = &revision.presentation_receipt {
        let input = receipt
            .proposal
            .input_collection
            .characters
            .get(&draft.id)
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    "presentation_receipt.proposal.input_collection",
                    "presentation receipt does not include this authoring draft",
                )
            })?;
        if input != before_profile {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                "presentation_receipt.proposal.input_collection",
                "presentation receipt does not target the current effective profile",
            ));
        }
        let output = receipt
            .output_collection
            .characters
            .get(&draft.id)
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    "presentation_receipt.output_collection",
                    "presentation receipt output omits this authoring draft",
                )
            })?;
        let operations = receipt.operations.get(&draft.id).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "presentation_receipt.operations",
                "presentation receipt contains no accepted operation for this authoring draft",
            )
        })?;
        candidate.overlay.provenance = merge_provenance(
            &candidate.overlay.provenance,
            &receipt.proposal.catalog.provenance,
        )?;
        for operation in operations {
            upsert_template_relative_operation(
                &mut candidate,
                operation.id.clone(),
                operation.rationale.clone(),
                operation.action.clone(),
            )?;
        }
        if candidate
            .presentation_receipts
            .iter()
            .any(|existing| existing.id == receipt.id)
        {
            return Err(error(
                CharacterDiagnosticCode::ConflictingOverlay,
                "presentation_receipt.id",
                "presentation receipt was already recorded by this draft",
            ));
        }
        candidate.presentation_receipts.push((**receipt).clone());
        candidate
            .presentation_receipts
            .sort_by(|left, right| left.id.cmp(&right.id));
        expected_enrichment_output = Some(output.clone());
    } else if let Some(enrichment) = &revision.enrichment {
        let (input, output, namespace, operation_id) = enrichment_profiles(enrichment)?;
        if input != before_profile {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                "enrichment.input_profile",
                "reviewed enrichment does not target the current effective profile",
            ));
        }
        validate_enrichment_delta(input, output, namespace)?;
        candidate.overlay.provenance =
            merge_provenance(&candidate.overlay.provenance, &output.provenance)?;
        let extension = output.extensions.get(namespace).cloned().ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "enrichment.output_profile.extensions",
                "reviewed enrichment output is missing its owned extension",
            )
        })?;
        upsert_template_relative_operation(
            &mut candidate,
            operation_id.to_owned(),
            "Adopt one independently reproduced reviewed enrichment receipt.".to_owned(),
            CharacterOperationAction::UpsertExtension {
                namespace: namespace.to_owned(),
                extension,
            },
        )?;
        candidate
            .unresolved_invalidations
            .remove(match enrichment.as_ref() {
                CharacterReviewedEnrichment::Alignment(_) => "alignment_review",
                CharacterReviewedEnrichment::DateContext(_) => "date_context_review",
            });
        expected_enrichment_output = Some(output.clone());
    } else {
        candidate.overlay.provenance =
            merge_provenance(&candidate.overlay.provenance, &revision.provenance)?;
        for change in &revision.changes {
            upsert_template_relative_operation(
                &mut candidate,
                change.id.clone(),
                change.rationale.clone(),
                change.action.clone(),
            )?;
        }
    }

    candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
        authoring_invalid("draft.revision", "authoring draft revision overflowed")
    })?;
    candidate.final_review = None;
    let after_result = synthesize_draft(&candidate)?;
    if let Some(expected) = expected_enrichment_output
        && after_result.effective_profile != expected
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "revision",
            "reviewed receipt does not project through the retained sparse authoring source",
        ));
    }
    let changes = profile_field_changes(before_profile, &after_result.effective_profile)?;
    if changes.is_empty()
        && revision.questionnaire_receipt.is_none()
        && revision.presentation_receipt.is_none()
        && revision.enrichment.is_none()
        && revision.template_migration.is_none()
    {
        return Err(authoring_invalid(
            "changes",
            "authoring revision must change at least one effective field",
        ));
    }
    let invalidations = derive_authoring_invalidations(
        draft,
        before_profile,
        &after_result.effective_profile,
        &changes,
    );
    for invalidation in &invalidations {
        if invalidation.blocking_final_review && !invalidation.resolved_in_candidate {
            candidate
                .unresolved_invalidations
                .insert(invalidation.id.clone(), invalidation.clone());
        }
    }
    validate_authoring_draft(&candidate)?;

    let migration_effects = if revision.template_migration.is_some() {
        match (
            before_template_profile,
            candidate
                .template
                .as_ref()
                .map(|template| &template.profile),
        ) {
            (Some(before), Some(after)) => profile_field_changes(before, after)?,
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };
    Ok(CharacterAuthoringPreview {
        preview_format_version: CHARACTER_AUTHORING_PREVIEW_FORMAT_VERSION,
        workspace_id: workspace.id.clone(),
        draft_id: draft.id.clone(),
        input_draft_revision: draft.revision,
        revision_id: revision.id.clone(),
        base_fields: build_field_views(
            &candidate,
            candidate
                .template
                .as_ref()
                .map(|template| &template.profile),
            &after_result.effective_profile,
            &after_result.origins,
        )?,
        changes,
        migration_effects,
        derived_ocean: DerivedOceanInspection {
            authority: ValueState::Derived,
            visually_distinct_from_canon: true,
            lossy: true,
            before: before_profile.derived.ocean.clone(),
            after: after_result.effective_profile.derived.ocean.clone(),
            recomputed_in_candidate: before_profile.derived.ocean
                != after_result.effective_profile.derived.ocean,
        },
        invalidations,
        resulting_profile_sha256: profile_fingerprint(&after_result.effective_profile)?,
        candidate_draft: candidate,
    })
}

/// Reproduce a preview and atomically replace one in-memory draft in a cloned workspace.
pub fn apply_authoring_revision(
    workspace: &CharacterAuthoringWorkspace,
    revision: &CharacterAuthoringRevision,
) -> Result<CharacterAuthoringWorkspace, crate::CharacterError> {
    let preview = preview_authoring_revision(workspace, revision)?;
    let mut result = workspace.clone();
    result
        .drafts
        .insert(preview.draft_id.clone(), preview.candidate_draft);
    result.revision = result
        .revision
        .checked_add(1)
        .ok_or_else(|| authoring_invalid("revision", "authoring workspace revision overflowed"))?;
    validate_authoring_workspace(&result)?;
    Ok(result)
}

/// Build the canonical authoring revision that records one reviewed questionnaire receipt.
pub fn questionnaire_authoring_revision(
    draft: &CharacterAuthoringDraft,
    id: impl Into<String>,
    rationale: impl Into<String>,
    receipt: CharacterQuestionnaireReceipt,
) -> Result<CharacterAuthoringRevision, crate::CharacterError> {
    validate_authoring_draft(draft)?;
    validate_questionnaire_receipt(&receipt)?;
    let revision = CharacterAuthoringRevision {
        revision_format_version: CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        id: id.into(),
        draft_id: draft.id.clone(),
        expected_draft_revision: draft.revision,
        input_mode: CharacterAuthoringInputMode::Questionnaire,
        rationale: rationale.into(),
        changes: Vec::new(),
        provenance: receipt.proposal.pack.provenance.clone(),
        questionnaire_receipt: Some(Box::new(receipt)),
        presentation_receipt: None,
        enrichment: None,
        template_migration: None,
    };
    validate_authoring_revision(&revision)?;
    Ok(revision)
}

/// Build the canonical authoring revision that adopts this draft's reviewed catalog operations.
pub fn presentation_authoring_revision(
    draft: &CharacterAuthoringDraft,
    id: impl Into<String>,
    rationale: impl Into<String>,
    receipt: PresentationReceipt,
) -> Result<CharacterAuthoringRevision, crate::CharacterError> {
    validate_authoring_draft(draft)?;
    validate_presentation_receipt(&receipt)?;
    if !receipt
        .proposal
        .input_collection
        .characters
        .contains_key(&draft.id)
        || !receipt.operations.contains_key(&draft.id)
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "presentation_receipt",
            "presentation receipt contains no accepted allocation for this draft",
        ));
    }
    let revision = CharacterAuthoringRevision {
        revision_format_version: CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        id: id.into(),
        draft_id: draft.id.clone(),
        expected_draft_revision: draft.revision,
        input_mode: CharacterAuthoringInputMode::PresentationCatalog,
        rationale: rationale.into(),
        changes: Vec::new(),
        questionnaire_receipt: None,
        presentation_receipt: Some(Box::new(receipt.clone())),
        enrichment: None,
        template_migration: None,
        provenance: receipt.proposal.catalog.provenance.clone(),
    };
    validate_authoring_revision(&revision)?;
    Ok(revision)
}

/// Build the canonical authoring revision that adopts one reviewed alignment or context receipt.
pub fn enrichment_authoring_revision(
    draft: &CharacterAuthoringDraft,
    id: impl Into<String>,
    rationale: impl Into<String>,
    enrichment: CharacterReviewedEnrichment,
) -> Result<CharacterAuthoringRevision, crate::CharacterError> {
    validate_authoring_draft(draft)?;
    validate_reviewed_enrichment(&enrichment)?;
    let (_, output, _, _) = enrichment_profiles(&enrichment)?;
    let provenance = output.provenance.clone();
    let revision = CharacterAuthoringRevision {
        revision_format_version: CHARACTER_AUTHORING_REVISION_FORMAT_VERSION,
        id: id.into(),
        draft_id: draft.id.clone(),
        expected_draft_revision: draft.revision,
        input_mode: CharacterAuthoringInputMode::ReviewedEnrichment,
        rationale: rationale.into(),
        changes: Vec::new(),
        questionnaire_receipt: None,
        presentation_receipt: None,
        enrichment: Some(Box::new(enrichment)),
        template_migration: None,
        provenance,
    };
    validate_authoring_revision(&revision)?;
    Ok(revision)
}

/// Save a final accepted or needs-changes review in a cloned workspace.
pub fn review_authoring_draft(
    workspace: &CharacterAuthoringWorkspace,
    id: &str,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
    decision: CharacterFinalReviewDecision,
) -> Result<CharacterAuthoringWorkspace, crate::CharacterError> {
    validate_authoring_workspace(workspace)?;
    let draft = workspace.drafts.get(id).ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "id",
            "authoring draft is absent from the workspace",
        )
    })?;
    let profile = synthesize_draft(draft)?.effective_profile;
    let review = CharacterFinalReview {
        review_format_version: CHARACTER_FINAL_REVIEW_FORMAT_VERSION,
        draft_id: draft.id.clone(),
        draft_revision: draft.revision,
        profile_sha256: profile_fingerprint(&profile)?,
        reviewer: reviewer.into(),
        rationale: rationale.into(),
        decision,
        summary: final_review_summary(draft, &profile)?,
    };
    validate_final_review_for_draft(&review, draft, &profile)?;
    let mut result = workspace.clone();
    result
        .drafts
        .get_mut(id)
        .expect("validated draft exists")
        .final_review = Some(review);
    result.revision = result
        .revision
        .checked_add(1)
        .ok_or_else(|| authoring_invalid("revision", "authoring workspace revision overflowed"))?;
    validate_authoring_workspace(&result)?;
    Ok(result)
}

fn apply_template_migration(
    draft: &mut CharacterAuthoringDraft,
    migration: &ReviewedCharacterTemplateMigration,
) -> Result<(), crate::CharacterError> {
    let current = draft.template.as_ref().ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "template_migration",
            "a blank draft has no template release to migrate",
        )
    })?;
    if template_fingerprint(current)? != migration.prior_template_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "template_migration.prior_template_sha256",
            "template migration does not fingerprint the current base",
        ));
    }
    if current.id != migration.template.id || current.version == migration.template.version {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "template_migration.template",
            "template migration requires a different release of the same stable template id",
        ));
    }
    let prior_template = draft.overlay.template.clone().ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "template_migration",
            "template-backed draft is missing its exact template coordinate",
        )
    })?;
    draft.template = Some(migration.template.clone());
    let template = draft.template.as_ref().expect("template just assigned");
    draft.overlay.template = Some(CharacterTemplateRef {
        id: template.id.clone(),
        version: template.version.clone(),
        sha256: template_fingerprint(template)?,
    });
    let selected_template = draft
        .overlay
        .template
        .clone()
        .expect("template coordinate just assigned");
    draft
        .template_migrations
        .push(AppliedCharacterTemplateMigration {
            prior_template,
            template: selected_template,
            applied_draft_revision: draft.revision.checked_add(1).ok_or_else(|| {
                authoring_invalid("draft.revision", "authoring draft revision overflowed")
            })?,
            reviewer: migration.reviewer.clone(),
            rationale: migration.rationale.clone(),
        });
    let actions = draft
        .overlay
        .operations
        .iter()
        .map(|operation| {
            (
                operation.id.clone(),
                operation.rationale.clone(),
                operation.action.clone(),
            )
        })
        .collect::<Vec<_>>();
    draft.overlay.operations.clear();
    for (id, rationale, action) in actions {
        upsert_template_relative_operation(draft, id, rationale, action)?;
    }
    Ok(())
}

fn upsert_template_relative_operation(
    draft: &mut CharacterAuthoringDraft,
    id: String,
    rationale: String,
    action: CharacterOperationAction,
) -> Result<(), crate::CharacterError> {
    let target = operation_target(&action);
    draft
        .overlay
        .operations
        .retain(|operation| operation_target(&operation.action) != target);
    let expected_prior_sha256 = match &draft.template {
        Some(template) => current_value_hash(&template.profile, &action)?,
        None => None,
    };
    draft.overlay.operations.push(CharacterOperation {
        id,
        expected_prior_sha256,
        rationale,
        action,
    });
    draft.overlay.operations.sort_by(|left, right| {
        operation_target(&left.action)
            .cmp(&operation_target(&right.action))
            .then_with(|| left.id.cmp(&right.id))
    });
    validate_overlay(&draft.overlay)
}

fn enrichment_profiles(
    enrichment: &CharacterReviewedEnrichment,
) -> Result<
    (
        &CharacterProfile,
        &CharacterProfile,
        &'static str,
        &'static str,
    ),
    crate::CharacterError,
> {
    validate_reviewed_enrichment(enrichment)?;
    Ok(match enrichment {
        CharacterReviewedEnrichment::Alignment(receipt) => (
            &receipt.input_profile,
            &receipt.output_profile,
            ALIGNMENT_EXTENSION_NAMESPACE,
            "adopt_reviewed_alignment",
        ),
        CharacterReviewedEnrichment::DateContext(receipt) => (
            &receipt.input_profile,
            &receipt.output_profile,
            DATE_CONTEXT_EXTENSION_NAMESPACE,
            "adopt_reviewed_date_context",
        ),
    })
}

fn validate_enrichment_delta(
    input: &CharacterProfile,
    output: &CharacterProfile,
    namespace: &str,
) -> Result<(), crate::CharacterError> {
    if input.id != output.id
        || input.canon != output.canon
        || input.derived != output.derived
        || input.suggestions != output.suggestions
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "enrichment.output_profile",
            "reviewed enrichment changed canonical, derived, suggestion, or identity data",
        ));
    }
    let mut expected_extensions = input.extensions.clone();
    let extension = output.extensions.get(namespace).cloned().ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            "enrichment.output_profile.extensions",
            "reviewed enrichment output is missing its owned namespace",
        )
    })?;
    expected_extensions.insert(namespace.to_owned(), extension);
    if expected_extensions != output.extensions {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "enrichment.output_profile.extensions",
            "reviewed enrichment changed a namespace outside its authority",
        ));
    }
    Ok(())
}

fn derive_authoring_invalidations(
    draft: &CharacterAuthoringDraft,
    before: &CharacterProfile,
    after: &CharacterProfile,
    changes: &[CharacterAuthoringFieldChange],
) -> Vec<CharacterAuthoringInvalidation> {
    let mut changed_paths = changes
        .iter()
        .map(|change| change.path.clone())
        .collect::<Vec<_>>();
    changed_paths.sort();
    let personality_paths = changed_paths
        .iter()
        .filter(|path| path.starts_with("canon.personality."))
        .cloned()
        .collect::<Vec<_>>();
    let birth_changed = changed_paths.iter().any(|path| path == "canon.birth_date");
    let mut invalidations = Vec::new();
    if !personality_paths.is_empty() {
        invalidations.push(CharacterAuthoringInvalidation {
            id: "derived_ocean".to_owned(),
            kind: CharacterAuthoringInvalidationKind::DerivedOcean,
            input_paths: personality_paths.clone(),
            required_action: CharacterAuthoringRequiredAction::Recompute,
            blocking_final_review: false,
            resolved_in_candidate: before.derived.ocean != after.derived.ocean,
            message:
                "The lossy OCEAN view is derived and is recomputed from accepted HEXACO canon."
                    .to_owned(),
        });
        if before
            .extensions
            .contains_key(ALIGNMENT_EXTENSION_NAMESPACE)
        {
            invalidations.push(CharacterAuthoringInvalidation {
                id: "alignment_review".to_owned(),
                kind: CharacterAuthoringInvalidationKind::AlignmentReview,
                input_paths: personality_paths.clone(),
                required_action: CharacterAuthoringRequiredAction::RefreshAndReview,
                blocking_final_review: true,
                resolved_in_candidate: false,
                message: "Approved alignment was derived from changed personality inputs and must be refreshed."
                    .to_owned(),
            });
        }
    }
    if (birth_changed || !personality_paths.is_empty())
        && before
            .extensions
            .contains_key(DATE_CONTEXT_EXTENSION_NAMESPACE)
    {
        let mut inputs = personality_paths;
        if birth_changed {
            inputs.push("canon.birth_date".to_owned());
            inputs.sort();
        }
        invalidations.push(CharacterAuthoringInvalidation {
            id: "date_context_review".to_owned(),
            kind: CharacterAuthoringInvalidationKind::DateContextReview,
            input_paths: inputs,
            required_action: CharacterAuthoringRequiredAction::RefreshAndReview,
            blocking_final_review: true,
            resolved_in_candidate: false,
            message:
                "Accepted historical-context candidates must be refreshed against changed inputs."
                    .to_owned(),
        });
    }
    if draft.final_review.is_some() && !changed_paths.is_empty() {
        invalidations.push(CharacterAuthoringInvalidation {
            id: "final_review".to_owned(),
            kind: CharacterAuthoringInvalidationKind::FinalReview,
            input_paths: changed_paths,
            required_action: CharacterAuthoringRequiredAction::RepeatFinalReview,
            blocking_final_review: true,
            resolved_in_candidate: true,
            message:
                "The prior final review is cleared; review the new effective profile before export."
                    .to_owned(),
        });
    }
    invalidations.sort_by(|left, right| left.id.cmp(&right.id));
    invalidations
}

fn build_field_views(
    draft: &CharacterAuthoringDraft,
    base: Option<&CharacterProfile>,
    effective: &CharacterProfile,
    origins: &BTreeMap<String, SynthesisOrigin>,
) -> Result<Vec<CharacterAuthoringFieldView>, crate::CharacterError> {
    let mut paths = profile_field_paths(effective)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if let Some(base) = base {
        paths.extend(profile_field_paths(base));
    }
    paths
        .into_iter()
        .map(|path| {
            let base_value = base
                .map(|profile| field_json_value(profile, &path))
                .transpose()?
                .flatten();
            let effective_value = field_json_value(effective, &path)?;
            let origin = origins.get(&path).cloned();
            let origin_kind = authoring_field_origin_kind(draft, origin.as_ref());
            let inherited = matches!(origin, Some(SynthesisOrigin::Template { .. }));
            let overridden = matches!(origin, Some(SynthesisOrigin::Overlay { .. }));
            let locked = effective_value.as_ref().is_some_and(json_value_locked);
            let editable_presentation_prefix = format!(
                "extensions.{}.value.",
                crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
            );
            let protected_or_pack_owned = (path.starts_with("extensions.")
                && !path.starts_with(&editable_presentation_prefix))
                || path.starts_with("suggestions.")
                || path.starts_with("derived.");
            Ok(CharacterAuthoringFieldView {
                path,
                base_value,
                effective_value,
                origin,
                origin_kind,
                inherited,
                overridden,
                locked,
                protected_or_pack_owned,
            })
        })
        .collect()
}

fn authoring_field_origin_kind(
    draft: &CharacterAuthoringDraft,
    origin: Option<&SynthesisOrigin>,
) -> CharacterAuthoringFieldOriginKind {
    match origin {
        Some(SynthesisOrigin::Template { .. }) if !draft.template_migrations.is_empty() => {
            CharacterAuthoringFieldOriginKind::TemplateMigration
        }
        Some(SynthesisOrigin::Template { .. }) => CharacterAuthoringFieldOriginKind::Template,
        Some(SynthesisOrigin::Overlay { operation_id, .. }) => draft
            .overlay
            .operations
            .iter()
            .find(|operation| operation.id == *operation_id)
            .map_or(
                CharacterAuthoringFieldOriginKind::AuthoredOverride,
                |operation| {
                    if accepted_suggestion_operation(draft, operation) {
                        CharacterAuthoringFieldOriginKind::AcceptedSuggestion
                    } else {
                        CharacterAuthoringFieldOriginKind::AuthoredOverride
                    }
                },
            ),
        None => CharacterAuthoringFieldOriginKind::AuthoredOverride,
    }
}

fn accepted_suggestion_operation(
    draft: &CharacterAuthoringDraft,
    operation: &CharacterOperation,
) -> bool {
    let reviewed_value = match &operation.action {
        CharacterOperationAction::SetHexacoTrait { value, .. } => {
            value.state == ValueState::Reviewed
        }
        CharacterOperationAction::UpsertPresentationAssignment { value } => {
            value.state == ValueState::Reviewed
        }
        _ => false,
    };
    reviewed_value
        && (draft
            .questionnaire_receipts
            .iter()
            .flat_map(|receipt| &receipt.operations)
            .any(|receipt_operation| receipt_operation == operation)
            || draft
                .presentation_receipts
                .iter()
                .filter_map(|receipt| receipt.operations.get(&draft.id))
                .flatten()
                .any(|receipt_operation| receipt_operation == operation))
}

fn profile_field_changes(
    before: &CharacterProfile,
    after: &CharacterProfile,
) -> Result<Vec<CharacterAuthoringFieldChange>, crate::CharacterError> {
    let mut paths = profile_field_paths(before)
        .into_iter()
        .collect::<BTreeSet<_>>();
    paths.extend(profile_field_paths(after));
    let mut changes = Vec::new();
    for path in paths {
        let before_value = field_json_value(before, &path)?;
        let after_value = field_json_value(after, &path)?;
        if before_value != after_value {
            changes.push(CharacterAuthoringFieldChange {
                path,
                before_locked: before_value.as_ref().is_some_and(json_value_locked),
                after_locked: after_value.as_ref().is_some_and(json_value_locked),
                before: before_value,
                after: after_value,
            });
        }
    }
    Ok(changes)
}

fn field_json_value(
    profile: &CharacterProfile,
    path: &str,
) -> Result<Option<serde_json::Value>, crate::CharacterError> {
    let mut value = serde_json::to_value(profile).map_err(|_| encoding_error())?;
    crate::sort_json_keys(&mut value);
    let presentation_prefix = format!(
        "extensions.{}.value.",
        crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE
    );
    let segments = if let Some(suffix) = path.strip_prefix(&presentation_prefix) {
        let mut segments = vec![
            "extensions",
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE,
            "record",
            "value",
        ];
        segments.extend(suffix.split('.'));
        segments
    } else if let Some(id) = path.strip_prefix("extensions.") {
        vec!["extensions", id]
    } else if let Some(id) = path.strip_prefix("suggestions.") {
        vec!["suggestions", id]
    } else if let Some(id) = path.strip_prefix("canon.inner_life.") {
        vec!["canon", "inner_life", id]
    } else if let Some(id) = path.strip_prefix("canon.voice.") {
        vec!["canon", "voice", id]
    } else {
        path.split('.').collect()
    };
    let mut current = &value;
    for segment in segments {
        let Some(next) = current.get(segment) else {
            return Ok(None);
        };
        current = next;
    }
    Ok(Some(current.clone()))
}

fn json_value_locked(value: &serde_json::Value) -> bool {
    value.get("lock").and_then(serde_json::Value::as_str) == Some("locked")
        || value
            .get("content")
            .and_then(|content| content.get("lock"))
            .and_then(serde_json::Value::as_str)
            == Some("locked")
        || value
            .get("record")
            .and_then(|record| record.get("header"))
            .and_then(|header| header.get("lock"))
            .and_then(serde_json::Value::as_str)
            == Some("locked")
}

fn final_review_summary(
    draft: &CharacterAuthoringDraft,
    profile: &CharacterProfile,
) -> Result<CharacterFinalReviewSummary, crate::CharacterError> {
    let canonical_traits = ALL_HEXACO_TRAITS
        .into_iter()
        .filter_map(|trait_id| {
            trait_value(&profile.canon.personality, trait_id)
                .cloned()
                .map(|value| (trait_id, value))
        })
        .collect();
    let approved_alignment = profile
        .extensions
        .get(ALIGNMENT_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::AlignmentView(record) => Some(record.value.clone()),
            _ => None,
        });
    let accepted_date_context = profile
        .extensions
        .get(DATE_CONTEXT_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::DateContext(record) => Some(record.value.clone()),
            _ => None,
        });
    let identity_presentation = profile
        .extensions
        .get(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::IdentityPresentation(record) => Some(record.value.clone()),
            _ => None,
        });
    let unresolved_diagnostics = draft
        .unresolved_invalidations
        .values()
        .map(|invalidation| CharacterDiagnostic {
            code: CharacterDiagnosticCode::StaleInput,
            severity: DiagnosticSeverity::Warning,
            path: format!("authoring.invalidations.{}", invalidation.id),
            message: invalidation.message.clone(),
        })
        .collect();
    Ok(CharacterFinalReviewSummary {
        profile_id: profile.id.clone(),
        identity: profile.canon.identity.clone(),
        identity_presentation,
        birth_date: profile.canon.birth_date.clone(),
        canonical_traits,
        derived_ocean: profile.derived.clone(),
        approved_alignment,
        accepted_date_context,
        inner_life: profile.canon.inner_life.clone(),
        voice: profile.canon.voice.clone(),
        provenance: profile.provenance.clone(),
        unresolved_diagnostics,
    })
}

fn validate_final_review_structure(
    review: &CharacterFinalReview,
) -> Result<(), crate::CharacterError> {
    if review.review_format_version != CHARACTER_FINAL_REVIEW_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "review_format_version",
            "unsupported final Character authoring review version",
        ));
    }
    validate_namespaced_id("draft_id", &review.draft_id)?;
    validate_sha256("profile_sha256", &review.profile_sha256)?;
    validate_safe_authoring_text("reviewer", &review.reviewer, 1, 512)?;
    validate_safe_authoring_text("rationale", &review.rationale, 1, 2_048)?;
    validate_namespaced_id("summary.profile_id", &review.summary.profile_id)?;
    if review.decision == CharacterFinalReviewDecision::Accepted
        && !review.summary.unresolved_diagnostics.is_empty()
    {
        return Err(authoring_invalid(
            "summary.unresolved_diagnostics",
            "accepted final review cannot retain unresolved diagnostics",
        ));
    }
    Ok(())
}

fn validate_final_review_for_draft(
    review: &CharacterFinalReview,
    draft: &CharacterAuthoringDraft,
    profile: &CharacterProfile,
) -> Result<(), crate::CharacterError> {
    validate_final_review_structure(review)?;
    if review.draft_id != draft.id
        || review.draft_revision != draft.revision
        || review.profile_sha256 != profile_fingerprint(profile)?
        || review.summary != final_review_summary(draft, profile)?
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "final_review",
            "final review does not match the exact current draft and effective profile",
        ));
    }
    if review.decision == CharacterFinalReviewDecision::Accepted
        && !draft.unresolved_invalidations.is_empty()
    {
        return Err(authoring_invalid(
            "final_review.decision",
            "accepted final review requires all blocking invalidations to be resolved",
        ));
    }
    Ok(())
}
