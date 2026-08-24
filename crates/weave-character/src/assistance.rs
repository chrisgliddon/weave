//! Provider-neutral, review-gated Character assistance with a deterministic offline path.
//!
//! The core never performs network I/O. Provider adapters receive only an explicitly previewed,
//! approved payload and return strict JSON which this module parses and validates. Accepted
//! candidates enter the non-canonical suggestion queue; original candidates, advisory reviews,
//! and author decisions remain immutable receipt inputs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    DomainValue, Provenance, ProvenanceTransformation, parse_strict_json, to_pretty_json,
    to_pretty_ron, validate_provenance,
};

use crate::model::{
    Attributed, CHARACTER_PROFILE_FORMAT_VERSION, CharacterDiagnosticCode, CharacterProfile,
    CharacterSuggestion, Confidence, Freshness, LockState, ReviewState, ValueState,
};
use crate::operations::{
    CharacterCollection, collection_fingerprint as corpus_collection_fingerprint,
    validate_character_collection as validate_corpus_collection,
};
use crate::synthesis::merge_provenance;
use crate::validation::{
    CharacterError, error, invalid_value, validate_local_id, validate_namespaced_id,
    validate_profile, validate_semver, validate_sha256, validate_text,
};

pub const ASSISTANCE_TEMPLATE_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_REQUEST_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_PREVIEW_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_APPROVAL_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_PROVIDER_RESPONSE_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_CANDIDATE_SET_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_ADVISORY_REVIEW_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_DECISION_REVIEW_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_RECEIPT_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_BATCH_REQUEST_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_BATCH_PREVIEW_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_JOB_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_BATCH_RECEIPT_FORMAT_VERSION: u32 = 1;
pub const ASSISTANCE_COMPARISON_FORMAT_VERSION: u32 = 1;

const TEMPLATE_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-template:1";
const REQUEST_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-request:1";
const PREVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-preview:1";
const APPROVAL_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-approval:1";
const PROVIDER_RESPONSE_SCHEMA_ID: &str =
    "urn:weave:schema:character-assistance-provider-response:1";
const CANDIDATE_SET_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-candidate-set:1";
const ADVISORY_REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-advisory-review:1";
const DECISION_REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-decision-review:1";
const RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-receipt:1";
const BATCH_REQUEST_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-batch-request:1";
const BATCH_PREVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-batch-preview:1";
const JOB_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-job:1";
const BATCH_RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-batch-receipt:1";
const COMPARISON_SCHEMA_ID: &str = "urn:weave:schema:character-assistance-comparison:1";

const OFFLINE_ADAPTER_ID: &str = "org.weave.character.assistance.offline";
const OFFLINE_ADAPTER_VERSION: &str = "1.0.0";
const OFFLINE_ENGINE_ID: &str = "glasswind_scaffold_v1";
const OFFLINE_VARIANTS: u16 = 4;
const MAX_FIELDS: usize = 64;
const MAX_CANDIDATES: usize = 4_096;
const MAX_BATCH_CHARACTERS: usize = 65_536;

/// Exact whole-value redaction marker used by every diagnostic boundary.
pub const REDACTED: &str = "[REDACTED]";

/// Non-serializable logging wrapper for a secret owned by an adapter or approved secret channel.
///
/// The wrapped value is deliberately inaccessible and both formatting traits redact it in full.
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = &self.0;
        formatter.write_str(REDACTED)
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = &self.0;
        formatter.write_str(REDACTED)
    }
}

/// Independently versioned request template shared by offline and adapter-backed generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceTemplate {
    pub template_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub compatible_profile_versions: Vec<u32>,
    pub offline_scaffold_version: u32,
    pub fields: BTreeMap<AssistanceFieldKind, AssistanceTemplateField>,
    pub safety: AssistanceSafetyContract,
    pub license: String,
    pub license_url: String,
    pub provenance: Provenance,
}

/// Exact template coordinate pinned by every request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceTemplateRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

/// One requested development surface. Each variant has its own typed candidate value.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceFieldKind {
    Biography,
    Motivation,
    Fear,
    GuardedTruth,
    Tension,
    NarrativeHook,
    PresentationCue,
    RoleIdea,
    RelationshipCue,
    ContextReaction,
    ExpressionExample,
}

impl AssistanceFieldKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Biography => "biography",
            Self::Motivation => "motivation",
            Self::Fear => "fear",
            Self::GuardedTruth => "guarded_truth",
            Self::Tension => "tension",
            Self::NarrativeHook => "narrative_hook",
            Self::PresentationCue => "presentation_cue",
            Self::RoleIdea => "role_idea",
            Self::RelationshipCue => "relationship_cue",
            Self::ContextReaction => "context_reaction",
            Self::ExpressionExample => "expression_example",
        }
    }
}

/// Closed profile input surfaces which may be disclosed by an explicit request.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceInputField {
    DisplayName,
    Aliases,
    Personality,
    InnerLife,
    Voice,
    BirthContext,
    Presentation,
    Projections,
    Relationships,
    DateContext,
    Expression,
}

impl AssistanceInputField {
    const ALL: [Self; 11] = [
        Self::DisplayName,
        Self::Aliases,
        Self::Personality,
        Self::InnerLife,
        Self::Voice,
        Self::BirthContext,
        Self::Presentation,
        Self::Projections,
        Self::Relationships,
        Self::DateContext,
        Self::Expression,
    ];

    #[must_use]
    pub const fn all() -> &'static [Self] {
        &Self::ALL
    }

    #[must_use]
    pub const fn profile_path(self) -> &'static str {
        match self {
            Self::DisplayName => "canon.identity.display_name",
            Self::Aliases => "canon.identity.aliases",
            Self::Personality => "canon.personality",
            Self::InnerLife => "canon.inner_life",
            Self::Voice => "canon.voice",
            Self::BirthContext => "canon.birth_date",
            Self::Presentation => "extensions.org.weave.character.identity_presentation",
            Self::Projections => "extensions.org.weave.character.role_projections",
            Self::Relationships => "extensions.org.weave.character.relationships",
            Self::DateContext => "extensions.org.weave.character.date_context",
            Self::Expression => "extensions.org.weave.character.expression",
        }
    }
}

/// Per-field prompt, disclosure minimum, bounds, and eventual suggestion target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceTemplateField {
    pub kind: AssistanceFieldKind,
    pub instructions: String,
    pub required_inputs: Vec<AssistanceInputField>,
    pub target_path: String,
    pub minimum_text_chars: usize,
    pub maximum_text_chars: usize,
    pub maximum_candidates: u16,
}

/// Fail-closed output rules. V1 templates cannot disable a mandatory check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceSafetyContract {
    pub reject_placeholders: bool,
    pub reject_credential_shapes: bool,
    pub reject_control_characters: bool,
    pub require_known_references: bool,
    pub maximum_total_response_chars: usize,
}

/// Exact provider adapter coordinate and declared execution capabilities.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceProviderRef {
    pub adapter_id: String,
    pub adapter_version: String,
    pub engine_id: String,
    pub mode: AssistanceProviderMode,
    pub supports_seed: bool,
    pub credential_required: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceProviderMode {
    Offline,
    External,
}

/// Serializable settings are deliberately credential-free.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceSettings {
    pub candidates_per_field: u16,
    pub language: String,
    pub creativity_micros: u32,
    pub maximum_output_chars: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_parameters: BTreeMap<String, DomainValue>,
}

/// Exact single-profile generation request. Credentials have no serialized field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceRequest {
    pub request_format_version: u32,
    pub id: String,
    pub profile_id: String,
    pub expected_profile_sha256: String,
    pub template: AssistanceTemplateRef,
    pub provider: AssistanceProviderRef,
    pub fields: Vec<AssistanceFieldKind>,
    pub included_inputs: Vec<AssistanceInputField>,
    pub settings: AssistanceSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub generation: u32,
    pub provenance: Provenance,
}

/// Exact authoring values visible before any provider adapter is invoked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceScopePreview {
    pub profile_id: String,
    pub input_profile_sha256: String,
    pub included_values: BTreeMap<AssistanceInputField, DomainValue>,
    pub known_character_ids: Vec<String>,
    pub serialized_character_count: usize,
}

/// Provider-neutral payload created from the visible scope preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceProviderPayload {
    pub payload_format_version: u32,
    pub request_id: String,
    pub profile_id: String,
    pub fields: Vec<AssistanceFieldKind>,
    pub instructions: BTreeMap<AssistanceFieldKind, String>,
    pub required_evidence: BTreeMap<AssistanceFieldKind, Vec<String>>,
    pub included_values: BTreeMap<AssistanceInputField, DomainValue>,
    pub known_character_ids: Vec<String>,
    pub settings: AssistanceSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub generation: u32,
    pub expected_response_schema: String,
}

/// Dry-run artifact. `provider_called` must remain false in every valid preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistancePreview {
    pub preview_format_version: u32,
    pub request: AssistanceRequest,
    pub request_sha256: String,
    pub template_sha256: String,
    pub scope: AssistanceScopePreview,
    pub payload: AssistanceProviderPayload,
    pub expected_output: AssistanceExpectedOutput,
    pub provider_called: bool,
    pub credential_value_stored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceExpectedOutput {
    pub response_format_version: u32,
    pub schema_id: String,
    pub fields: Vec<AssistanceFieldKind>,
    pub maximum_candidates_per_field: u16,
    pub maximum_output_chars: usize,
}

/// Explicit approval of one already-rendered scope. No raw credential can be represented here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceExecutionApproval {
    pub approval_format_version: u32,
    pub preview_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_batch_preview_sha256: Option<String>,
    pub allow_provider_call: bool,
    pub author: String,
    pub rationale: String,
}

/// Typed adapter response before Weave assigns stable ids, evidence hashes, and review metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceProviderResponse {
    pub response_format_version: u32,
    pub request_id: String,
    pub candidates: Vec<AssistanceProviderCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceProviderCandidate {
    pub character_id: String,
    pub field: AssistanceFieldKind,
    pub value: AssistanceCandidateValue,
    pub evidence_paths: Vec<String>,
}

/// Closed structured values returned for every supported development surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum AssistanceCandidateValue {
    Biography {
        summary: String,
        formative_thread: String,
    },
    Motivation {
        objective: String,
        reason: String,
    },
    Fear {
        concern: String,
        trigger: String,
    },
    GuardedTruth {
        truth: String,
        guarded_because: String,
    },
    Tension {
        premise: String,
        opposing_pressure: String,
    },
    NarrativeHook {
        hook: String,
        stakes: String,
    },
    PresentationCue {
        cue: String,
        rationale: String,
    },
    RoleIdea {
        label: String,
        rationale: String,
    },
    RelationshipCue {
        cue: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        other_character_id: Option<String>,
        rationale: String,
    },
    ContextReaction {
        context: String,
        reaction: String,
    },
    ExpressionExample {
        context: String,
        text: String,
    },
}

impl AssistanceCandidateValue {
    #[must_use]
    pub const fn field(&self) -> AssistanceFieldKind {
        match self {
            Self::Biography { .. } => AssistanceFieldKind::Biography,
            Self::Motivation { .. } => AssistanceFieldKind::Motivation,
            Self::Fear { .. } => AssistanceFieldKind::Fear,
            Self::GuardedTruth { .. } => AssistanceFieldKind::GuardedTruth,
            Self::Tension { .. } => AssistanceFieldKind::Tension,
            Self::NarrativeHook { .. } => AssistanceFieldKind::NarrativeHook,
            Self::PresentationCue { .. } => AssistanceFieldKind::PresentationCue,
            Self::RoleIdea { .. } => AssistanceFieldKind::RoleIdea,
            Self::RelationshipCue { .. } => AssistanceFieldKind::RelationshipCue,
            Self::ContextReaction { .. } => AssistanceFieldKind::ContextReaction,
            Self::ExpressionExample { .. } => AssistanceFieldKind::ExpressionExample,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceEvidence {
    pub input_path: String,
    pub input_sha256: String,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceCandidateValidation {
    pub total_text_chars: usize,
    pub checks: Vec<AssistanceValidationCheck>,
    pub safe: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceValidationCheck {
    Schema,
    Length,
    CredentialShape,
    Placeholder,
    ControlCharacters,
    Reference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceCandidate {
    pub id: String,
    pub character_id: String,
    pub field: AssistanceFieldKind,
    pub target_path: String,
    pub value: AssistanceCandidateValue,
    pub evidence: Vec<AssistanceEvidence>,
    pub validation: AssistanceCandidateValidation,
    pub provider_index: usize,
    pub generation: u32,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceGenerationOrigin {
    OfflineScaffold,
    ProviderAdapter,
}

/// Immutable normalized candidates plus the exact profile, template, scope, and response hashes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceCandidateSet {
    pub candidate_set_format_version: u32,
    pub id: String,
    pub input_profile: CharacterProfile,
    pub input_profile_sha256: String,
    pub template: AssistanceTemplate,
    pub template_sha256: String,
    pub preview: AssistancePreview,
    pub preview_sha256: String,
    pub approval: AssistanceExecutionApproval,
    pub approval_sha256: String,
    pub provider_response_sha256: String,
    pub origin: AssistanceGenerationOrigin,
    pub candidates: BTreeMap<String, AssistanceCandidate>,
}

/// Adapter credential state contains no value or identifying metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistanceCredentialStatus {
    NotRequired,
    Configured,
    Missing,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceProviderFailureCode {
    Unavailable,
    RateLimited,
    Timeout,
    Authentication,
    Budget,
    Internal,
}

/// Redaction-safe provider failure. Adapter messages and response bodies are never retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistanceProviderFailure {
    pub code: AssistanceProviderFailureCode,
    pub retryable: bool,
}

impl fmt::Display for AssistanceProviderFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "assistance provider failed with {:?}", self.code)
    }
}

impl std::error::Error for AssistanceProviderFailure {}

/// Provider-neutral boundary. Implementations own credential acquisition outside serializable data.
pub trait CharacterAssistanceProvider {
    fn descriptor(&self) -> AssistanceProviderRef;
    fn credential_status(&self) -> AssistanceCredentialStatus;
    fn generate(
        &mut self,
        payload: &AssistanceProviderPayload,
    ) -> Result<String, AssistanceProviderFailure>;
}

/// Redaction-safe execution failure separating validated contract errors from adapter failures.
#[derive(Debug)]
pub enum AssistanceError {
    Contract(CharacterError),
    Provider(AssistanceProviderFailure),
}

impl fmt::Display for AssistanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(error) => error.fmt(formatter),
            Self::Provider(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for AssistanceError {}

impl From<CharacterError> for AssistanceError {
    fn from(error: CharacterError) -> Self {
        Self::Contract(error)
    }
}

/// Built-in deterministic provider. It performs no I/O and requires no credential.
#[derive(Debug, Clone, Default)]
pub struct OfflineAssistanceProvider;

impl CharacterAssistanceProvider for OfflineAssistanceProvider {
    fn descriptor(&self) -> AssistanceProviderRef {
        offline_assistance_provider_ref()
    }

    fn credential_status(&self) -> AssistanceCredentialStatus {
        AssistanceCredentialStatus::NotRequired
    }

    fn generate(
        &mut self,
        payload: &AssistanceProviderPayload,
    ) -> Result<String, AssistanceProviderFailure> {
        validate_offline_provider_payload(payload).map_err(|_| AssistanceProviderFailure {
            code: AssistanceProviderFailureCode::Internal,
            retryable: false,
        })?;
        let response = offline_provider_response(payload);
        to_pretty_json(&response).map_err(|_| AssistanceProviderFailure {
            code: AssistanceProviderFailureCode::Internal,
            retryable: false,
        })
    }
}

#[must_use]
pub fn offline_assistance_provider_ref() -> AssistanceProviderRef {
    AssistanceProviderRef {
        adapter_id: OFFLINE_ADAPTER_ID.to_owned(),
        adapter_version: OFFLINE_ADAPTER_VERSION.to_owned(),
        engine_id: OFFLINE_ENGINE_ID.to_owned(),
        mode: AssistanceProviderMode::Offline,
        supports_seed: true,
        credential_required: false,
    }
}

/// One non-authoritative issue raised by an independent advisory pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceAdvisoryIssue {
    pub code: String,
    pub severity: AssistanceAdvisorySeverity,
    pub message: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceAdvisorySeverity {
    Note,
    Caution,
    Block,
}

/// Scores and optional edits remain advisory and never replace the original candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceAdvisoryAssessment {
    pub candidate_id: String,
    pub relevance_micros: u32,
    pub consistency_micros: u32,
    pub safety_micros: u32,
    pub issues: Vec<AssistanceAdvisoryIssue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_edit: Option<AssistanceCandidateValue>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceAdvisoryReview {
    pub advisory_format_version: u32,
    pub id: String,
    pub candidate_set_sha256: String,
    pub reviewer: String,
    pub method: String,
    pub advisory_only: bool,
    pub canonical_write_back: bool,
    pub assessments: BTreeMap<String, AssistanceAdvisoryAssessment>,
}

/// Complete author disposition for one immutable candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "decision", deny_unknown_fields)]
pub enum AssistanceDecision {
    Accept {
        rationale: String,
    },
    Reject {
        rationale: String,
    },
    Edit {
        value: AssistanceCandidateValue,
        rationale: String,
    },
    Defer {
        rationale: String,
    },
    Regenerate {
        rationale: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceDecisionReview {
    pub review_format_version: u32,
    pub id: String,
    pub candidate_set_sha256: String,
    pub input_profile_sha256: String,
    pub author: String,
    pub rationale: String,
    pub decisions: BTreeMap<String, AssistanceDecision>,
}

/// Reproducible single-profile application proof. Canon and extensions remain byte-identical.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceReceipt {
    pub receipt_format_version: u32,
    pub id: String,
    pub input_profile: CharacterProfile,
    pub input_profile_sha256: String,
    pub output_profile: CharacterProfile,
    pub output_profile_sha256: String,
    pub candidate_set: AssistanceCandidateSet,
    pub candidate_set_sha256: String,
    pub decision_review: AssistanceDecisionReview,
    pub decision_review_sha256: String,
    pub advisory_review_sha256s: Vec<String>,
    pub accepted_suggestion_ids: Vec<String>,
    pub rejected_candidate_ids: Vec<String>,
    pub deferred_candidate_ids: Vec<String>,
    pub regenerate_candidate_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regeneration_request: Option<AssistanceRequest>,
}

/// Explicit batch filter. Empty id and prefix lists mean every collection profile is eligible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceBatchFilter {
    pub character_ids: Vec<String>,
    pub id_prefixes: Vec<String>,
    pub fields_missing_suggestions: Vec<AssistanceFieldKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceBatchBudget {
    pub maximum_characters: usize,
    pub maximum_provider_calls: usize,
    pub maximum_candidates: usize,
    pub maximum_input_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceRetryPolicy {
    pub maximum_retries_per_character: u16,
    pub retryable_codes: Vec<AssistanceProviderFailureCode>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceCachePolicy {
    Disabled,
    JobLocal,
}

/// Exact batch request. Rate limiting is deterministic: each resume call is one scheduling window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceBatchRequest {
    pub batch_request_format_version: u32,
    pub id: String,
    pub collection_id: String,
    pub expected_collection_sha256: String,
    pub template: AssistanceTemplateRef,
    pub provider: AssistanceProviderRef,
    pub fields: Vec<AssistanceFieldKind>,
    pub included_inputs: Vec<AssistanceInputField>,
    pub settings: AssistanceSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub filter: AssistanceBatchFilter,
    pub budget: AssistanceBatchBudget,
    pub retry: AssistanceRetryPolicy,
    pub rate_limit_calls_per_resume: usize,
    pub cache_policy: AssistanceCachePolicy,
    pub provenance: Provenance,
}

/// One visible batch privacy preview containing every exact per-profile single preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceBatchPreview {
    pub batch_preview_format_version: u32,
    pub request: AssistanceBatchRequest,
    pub request_sha256: String,
    pub template_sha256: String,
    pub ordered_target_ids: Vec<String>,
    pub previews: BTreeMap<String, AssistancePreview>,
    pub total_input_chars: usize,
    pub provider_called: bool,
    pub credential_value_stored: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AssistanceJobState {
    Running,
    RateLimited,
    BudgetExhausted,
    Cancelled,
    ReadyForReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceJobFailure {
    pub character_id: String,
    pub code: AssistanceProviderFailureCode,
    pub attempts: u16,
    pub retry_exhausted: bool,
}

/// Serializable resumable batch state. Completed payloads are cached only when explicitly enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceJob {
    pub job_format_version: u32,
    pub id: String,
    pub input_collection: CharacterCollection,
    pub input_collection_sha256: String,
    pub template: AssistanceTemplate,
    pub batch_preview: AssistanceBatchPreview,
    pub batch_preview_sha256: String,
    pub approval: AssistanceExecutionApproval,
    pub approval_sha256: String,
    pub ordered_target_ids: Vec<String>,
    pub next_index: usize,
    pub completed_target_ids: Vec<String>,
    pub attempts: BTreeMap<String, u16>,
    pub provider_calls: usize,
    pub candidates_generated: usize,
    pub input_chars_consumed: usize,
    pub candidate_sets: BTreeMap<String, AssistanceCandidateSet>,
    pub cache: BTreeMap<String, AssistanceCandidateSet>,
    pub failures: BTreeMap<String, AssistanceJobFailure>,
    pub state: AssistanceJobState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellation_rationale: Option<String>,
}

/// Atomic collection receipt for a complete set of per-profile author decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceBatchReceipt {
    pub batch_receipt_format_version: u32,
    pub id: String,
    pub input_collection: CharacterCollection,
    pub input_collection_sha256: String,
    pub output_collection: CharacterCollection,
    pub output_collection_sha256: String,
    pub job_sha256: String,
    pub receipts: BTreeMap<String, AssistanceReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceComparisonEntry {
    pub provider: AssistanceProviderRef,
    pub candidate_id: String,
    pub field: AssistanceFieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance_micros: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consistency_micros: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_micros: Option<u32>,
    pub issues: Vec<AssistanceAdvisoryIssue>,
}

/// Provider comparison is sorted by coordinate and id, never by an automatic winner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssistanceComparisonReport {
    pub comparison_format_version: u32,
    pub id: String,
    pub input_profile_sha256: String,
    pub providers: Vec<AssistanceProviderRef>,
    pub entries: Vec<AssistanceComparisonEntry>,
    pub advisory_only: bool,
    pub canonical_write_back: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_candidate_id: Option<String>,
    pub order_basis: String,
}

macro_rules! impl_assistance_document {
    ($type:ty, $validate:ident) => {
        impl $type {
            pub fn from_json(source: &str) -> Result<Self, CharacterError> {
                let value = parse_strict_json(source).map_err(|_| assistance_encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn from_ron(source: &str) -> Result<Self, CharacterError> {
                let value = ron::from_str(source).map_err(|_| assistance_encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn to_json(&self) -> Result<String, CharacterError> {
                $validate(self)?;
                to_pretty_json(self).map_err(|_| assistance_encoding_error())
            }

            pub fn to_ron(&self) -> Result<String, CharacterError> {
                $validate(self)?;
                to_pretty_ron(self).map_err(|_| assistance_encoding_error())
            }
        }
    };
}

impl_assistance_document!(AssistanceTemplate, validate_assistance_template);
impl_assistance_document!(AssistanceRequest, validate_assistance_request_structure);
impl_assistance_document!(AssistancePreview, validate_assistance_preview);
impl_assistance_document!(
    AssistanceExecutionApproval,
    validate_assistance_execution_approval
);
impl_assistance_document!(
    AssistanceProviderResponse,
    validate_assistance_provider_response_structure
);
impl_assistance_document!(AssistanceCandidateSet, validate_assistance_candidate_set);
impl_assistance_document!(
    AssistanceAdvisoryReview,
    validate_assistance_advisory_review_structure
);
impl_assistance_document!(
    AssistanceDecisionReview,
    validate_assistance_decision_review_structure
);
impl_assistance_document!(AssistanceReceipt, validate_assistance_receipt);
impl_assistance_document!(
    AssistanceBatchRequest,
    validate_assistance_batch_request_structure
);
impl_assistance_document!(AssistanceBatchPreview, validate_assistance_batch_preview);
impl_assistance_document!(AssistanceJob, validate_assistance_job);
impl_assistance_document!(AssistanceBatchReceipt, validate_assistance_batch_receipt);
impl_assistance_document!(AssistanceComparisonReport, validate_assistance_comparison);

pub fn assistance_template_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceTemplate>(
        TEMPLATE_SCHEMA_ID,
        "Weave Character Assistance Template v1",
    )
}

pub fn assistance_request_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceRequest>(
        REQUEST_SCHEMA_ID,
        "Weave Character Assistance Request v1",
    )
}

pub fn assistance_preview_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistancePreview>(
        PREVIEW_SCHEMA_ID,
        "Weave Character Assistance Preview v1",
    )
}

pub fn assistance_approval_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceExecutionApproval>(
        APPROVAL_SCHEMA_ID,
        "Weave Character Assistance Execution Approval v1",
    )
}

pub fn assistance_provider_response_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceProviderResponse>(
        PROVIDER_RESPONSE_SCHEMA_ID,
        "Weave Character Assistance Provider Response v1",
    )
}

pub fn assistance_candidate_set_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceCandidateSet>(
        CANDIDATE_SET_SCHEMA_ID,
        "Weave Character Assistance Candidate Set v1",
    )
}

pub fn assistance_advisory_review_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceAdvisoryReview>(
        ADVISORY_REVIEW_SCHEMA_ID,
        "Weave Character Assistance Advisory Review v1",
    )
}

pub fn assistance_decision_review_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceDecisionReview>(
        DECISION_REVIEW_SCHEMA_ID,
        "Weave Character Assistance Decision Review v1",
    )
}

pub fn assistance_receipt_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceReceipt>(
        RECEIPT_SCHEMA_ID,
        "Weave Character Assistance Receipt v1",
    )
}

pub fn assistance_batch_request_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceBatchRequest>(
        BATCH_REQUEST_SCHEMA_ID,
        "Weave Character Assistance Batch Request v1",
    )
}

pub fn assistance_batch_preview_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceBatchPreview>(
        BATCH_PREVIEW_SCHEMA_ID,
        "Weave Character Assistance Batch Preview v1",
    )
}

pub fn assistance_job_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceJob>(JOB_SCHEMA_ID, "Weave Character Assistance Job v1")
}

pub fn assistance_batch_receipt_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceBatchReceipt>(
        BATCH_RECEIPT_SCHEMA_ID,
        "Weave Character Assistance Batch Receipt v1",
    )
}

pub fn assistance_comparison_schema() -> Result<String, CharacterError> {
    assistance_schema::<AssistanceComparisonReport>(
        COMPARISON_SCHEMA_ID,
        "Weave Character Assistance Comparison v1",
    )
}

pub fn assistance_template_fingerprint(
    template: &AssistanceTemplate,
) -> Result<String, CharacterError> {
    validate_assistance_template(template)?;
    assistance_hash(template)
}

pub fn assistance_request_fingerprint(
    request: &AssistanceRequest,
) -> Result<String, CharacterError> {
    validate_assistance_request_structure(request)?;
    assistance_hash(request)
}

pub fn assistance_preview_fingerprint(
    preview: &AssistancePreview,
) -> Result<String, CharacterError> {
    validate_assistance_preview(preview)?;
    assistance_hash(preview)
}

pub fn assistance_approval_fingerprint(
    approval: &AssistanceExecutionApproval,
) -> Result<String, CharacterError> {
    validate_assistance_execution_approval(approval)?;
    assistance_hash(approval)
}

pub fn assistance_candidate_set_fingerprint(
    candidates: &AssistanceCandidateSet,
) -> Result<String, CharacterError> {
    validate_assistance_candidate_set(candidates)?;
    assistance_hash(candidates)
}

pub fn assistance_advisory_review_fingerprint(
    review: &AssistanceAdvisoryReview,
) -> Result<String, CharacterError> {
    validate_assistance_advisory_review_structure(review)?;
    assistance_hash(review)
}

pub fn assistance_decision_review_fingerprint(
    review: &AssistanceDecisionReview,
) -> Result<String, CharacterError> {
    validate_assistance_decision_review_structure(review)?;
    assistance_hash(review)
}

pub fn assistance_job_fingerprint(job: &AssistanceJob) -> Result<String, CharacterError> {
    validate_assistance_job(job)?;
    assistance_hash(job)
}

pub fn assistance_profile_fingerprint(
    profile: &CharacterProfile,
) -> Result<String, CharacterError> {
    validate_profile(profile)?;
    assistance_hash(profile)
}

pub fn validate_assistance_template(template: &AssistanceTemplate) -> Result<(), CharacterError> {
    require_version(
        template.template_format_version,
        ASSISTANCE_TEMPLATE_FORMAT_VERSION,
        "template_format_version",
    )?;
    validate_namespaced_id("id", &template.id)?;
    validate_semver("version", &template.version)?;
    validate_public_text("title", &template.title, 1, 256, false)?;
    validate_public_text("description", &template.description, 1, 4_096, false)?;
    if template.compatible_profile_versions.is_empty()
        || template.compatible_profile_versions.len() > 64
        || !strictly_sorted_unique(&template.compatible_profile_versions)
        || !template
            .compatible_profile_versions
            .contains(&CHARACTER_PROFILE_FORMAT_VERSION)
    {
        return Err(invalid_value(
            "compatible_profile_versions",
            "template profile versions must be sorted, unique, bounded, and include v1",
        ));
    }
    if template.offline_scaffold_version != 1 {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "offline_scaffold_version",
            "unsupported deterministic offline scaffold version",
        ));
    }
    if template.fields.is_empty() || template.fields.len() > MAX_FIELDS {
        return Err(invalid_value(
            "fields",
            "assistance template requires a bounded non-empty field map",
        ));
    }
    for (kind, field) in &template.fields {
        let path = format!("fields.{}", kind.as_str());
        if *kind != field.kind {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "template field kind must equal its containing map key",
            ));
        }
        validate_public_text(
            &format!("{path}.instructions"),
            &field.instructions,
            1,
            4_096,
            false,
        )?;
        if field.required_inputs.is_empty()
            || field.required_inputs.len() > 32
            || !strictly_sorted_unique(&field.required_inputs)
        {
            return Err(invalid_value(
                format!("{path}.required_inputs"),
                "required input fields must be non-empty, sorted, unique, and bounded",
            ));
        }
        validate_profile_path(&format!("{path}.target_path"), &field.target_path)?;
        if field.minimum_text_chars == 0
            || field.minimum_text_chars > field.maximum_text_chars
            || field.maximum_text_chars > 16_384
            || field.maximum_candidates == 0
            || usize::from(field.maximum_candidates) > MAX_CANDIDATES
        {
            return Err(invalid_value(
                path,
                "template field length and candidate bounds are invalid",
            ));
        }
    }
    if !template.safety.reject_placeholders
        || !template.safety.reject_credential_shapes
        || !template.safety.reject_control_characters
        || !template.safety.require_known_references
        || !(1..=1_048_576).contains(&template.safety.maximum_total_response_chars)
    {
        return Err(invalid_value(
            "safety",
            "v1 templates must enable every mandatory safety check and bound total response text",
        ));
    }
    validate_public_text("license", &template.license, 1, 128, false)?;
    validate_https("license_url", &template.license_url)?;
    validate_provenance(&template.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "assistance template provenance is invalid",
        )
    })
}

pub fn validate_assistance_request_structure(
    request: &AssistanceRequest,
) -> Result<(), CharacterError> {
    require_version(
        request.request_format_version,
        ASSISTANCE_REQUEST_FORMAT_VERSION,
        "request_format_version",
    )?;
    validate_namespaced_id("id", &request.id)?;
    validate_namespaced_id("profile_id", &request.profile_id)?;
    validate_sha256("expected_profile_sha256", &request.expected_profile_sha256)?;
    validate_template_ref("template", &request.template)?;
    validate_provider_ref("provider", &request.provider)?;
    validate_requested_fields("fields", &request.fields)?;
    validate_included_inputs("included_inputs", &request.included_inputs)?;
    validate_assistance_settings(&request.settings)?;
    if request.seed.is_some() && !request.provider.supports_seed {
        return Err(invalid_value(
            "seed",
            "request cannot pin a seed for an adapter that does not support one",
        ));
    }
    if request.provider.mode == AssistanceProviderMode::Offline && request.seed.is_none() {
        return Err(invalid_value(
            "seed",
            "deterministic offline assistance requires an explicit seed",
        ));
    }
    if request.provider.mode == AssistanceProviderMode::Offline
        && request.settings.candidates_per_field > OFFLINE_VARIANTS
    {
        return Err(invalid_value(
            "settings.candidates_per_field",
            "the v1 offline scaffold supports at most four distinct candidates per field",
        ));
    }
    if request.generation > 65_535 {
        return Err(invalid_value(
            "generation",
            "assistance generation counter is outside the supported bound",
        ));
    }
    validate_provenance(&request.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "assistance request provenance is invalid",
        )
    })
}

pub fn validate_assistance_request(
    profile: &CharacterProfile,
    template: &AssistanceTemplate,
    request: &AssistanceRequest,
) -> Result<(), CharacterError> {
    validate_profile(profile)?;
    validate_assistance_template(template)?;
    validate_assistance_request_structure(request)?;
    if request.profile_id != profile.id
        || request.expected_profile_sha256 != assistance_profile_fingerprint(profile)?
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "expected_profile_sha256",
            "assistance request does not match the exact current profile",
        ));
    }
    let template_ref = assistance_template_ref(template)?;
    if request.template != template_ref {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "template",
            "assistance request does not pin the exact selected template",
        ));
    }
    for field in &request.fields {
        let specification = template.fields.get(field).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "fields",
                "request selects a field absent from the exact template",
            )
        })?;
        if usize::from(request.settings.candidates_per_field)
            > usize::from(specification.maximum_candidates)
            || specification
                .required_inputs
                .iter()
                .any(|input| !request.included_inputs.contains(input))
        {
            return Err(invalid_value(
                "fields",
                "request candidate count or disclosed inputs violate the selected template",
            ));
        }
    }
    Ok(())
}

pub fn validate_assistance_preview(preview: &AssistancePreview) -> Result<(), CharacterError> {
    require_version(
        preview.preview_format_version,
        ASSISTANCE_PREVIEW_FORMAT_VERSION,
        "preview_format_version",
    )?;
    validate_assistance_request_structure(&preview.request)?;
    validate_sha256("request_sha256", &preview.request_sha256)?;
    validate_sha256("template_sha256", &preview.template_sha256)?;
    if preview.request_sha256 != assistance_request_fingerprint(&preview.request)?
        || preview.template_sha256 != preview.request.template.sha256
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "preview",
            "assistance preview fingerprints do not match its request",
        ));
    }
    validate_scope_preview(&preview.scope, &preview.request)?;
    validate_provider_payload(&preview.payload, &preview.request, &preview.scope)?;
    if preview.expected_output.response_format_version
        != ASSISTANCE_PROVIDER_RESPONSE_FORMAT_VERSION
        || preview.expected_output.schema_id != PROVIDER_RESPONSE_SCHEMA_ID
        || preview.expected_output.fields != preview.request.fields
        || preview.expected_output.maximum_candidates_per_field
            != preview.request.settings.candidates_per_field
        || preview.expected_output.maximum_output_chars
            != preview.request.settings.maximum_output_chars
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "expected_output",
            "preview output contract does not match the exact request",
        ));
    }
    if preview.provider_called || preview.credential_value_stored {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "preview",
            "dry-run previews cannot call a provider or store credential values",
        ));
    }
    Ok(())
}

pub fn validate_assistance_execution_approval(
    approval: &AssistanceExecutionApproval,
) -> Result<(), CharacterError> {
    require_version(
        approval.approval_format_version,
        ASSISTANCE_APPROVAL_FORMAT_VERSION,
        "approval_format_version",
    )?;
    validate_sha256("preview_sha256", &approval.preview_sha256)?;
    if let Some(parent) = &approval.parent_batch_preview_sha256 {
        validate_sha256("parent_batch_preview_sha256", parent)?;
    }
    if !approval.allow_provider_call {
        return Err(invalid_value(
            "allow_provider_call",
            "execution approval must explicitly allow the provider operation",
        ));
    }
    validate_namespaced_id("author", &approval.author)?;
    validate_public_text("rationale", &approval.rationale, 1, 2_048, false)
}

pub fn validate_assistance_provider_response_structure(
    response: &AssistanceProviderResponse,
) -> Result<(), CharacterError> {
    require_version(
        response.response_format_version,
        ASSISTANCE_PROVIDER_RESPONSE_FORMAT_VERSION,
        "response_format_version",
    )?;
    validate_namespaced_id("request_id", &response.request_id)?;
    if response.candidates.is_empty() || response.candidates.len() > MAX_CANDIDATES {
        return Err(invalid_value(
            "candidates",
            "provider response requires a bounded non-empty candidate list",
        ));
    }
    let mut total_text_chars = 0usize;
    for (index, candidate) in response.candidates.iter().enumerate() {
        let path = format!("candidates[{index}]");
        validate_namespaced_id(&format!("{path}.character_id"), &candidate.character_id)?;
        if candidate.field != candidate.value.field() {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "provider candidate field does not match its typed value",
            ));
        }
        validate_sorted_paths(&format!("{path}.evidence_paths"), &candidate.evidence_paths)?;
        for text in candidate_texts(&candidate.value) {
            validate_public_text(&format!("{path}.value"), text, 1, 16_384, true)?;
        }
        if let AssistanceCandidateValue::RelationshipCue {
            other_character_id: Some(other_character_id),
            ..
        } = &candidate.value
        {
            validate_namespaced_id(
                &format!("{path}.value.other_character_id"),
                other_character_id,
            )?;
        }
        total_text_chars = total_text_chars
            .checked_add(candidate_text_chars(&candidate.value))
            .ok_or_else(|| invalid_value("candidates", "provider response text size overflowed"))?;
    }
    if total_text_chars > 1_048_576 {
        return Err(invalid_value(
            "candidates",
            "provider response candidate text exceeds the global assistance bound",
        ));
    }
    Ok(())
}

pub fn validate_assistance_candidate_set(
    set: &AssistanceCandidateSet,
) -> Result<(), CharacterError> {
    require_version(
        set.candidate_set_format_version,
        ASSISTANCE_CANDIDATE_SET_FORMAT_VERSION,
        "candidate_set_format_version",
    )?;
    validate_namespaced_id("id", &set.id)?;
    validate_profile(&set.input_profile)?;
    validate_sha256("input_profile_sha256", &set.input_profile_sha256)?;
    validate_assistance_template(&set.template)?;
    validate_sha256("template_sha256", &set.template_sha256)?;
    validate_assistance_preview(&set.preview)?;
    validate_sha256("preview_sha256", &set.preview_sha256)?;
    validate_assistance_execution_approval(&set.approval)?;
    validate_sha256("approval_sha256", &set.approval_sha256)?;
    validate_sha256("provider_response_sha256", &set.provider_response_sha256)?;
    let expected_preview = preview_assistance_profile(
        &set.input_profile,
        set.preview.scope.known_character_ids.clone(),
        &set.template,
        &set.preview.request,
    )?;
    if set.id != format!("{}.candidates", set.preview.request.id)
        || set.input_profile_sha256 != assistance_profile_fingerprint(&set.input_profile)?
        || set.template_sha256 != assistance_template_fingerprint(&set.template)?
        || set.preview != expected_preview
        || set.preview_sha256 != assistance_preview_fingerprint(&set.preview)?
        || set.approval_sha256 != assistance_approval_fingerprint(&set.approval)?
        || set.approval.preview_sha256 != set.preview_sha256
        || set.preview.request.profile_id != set.input_profile.id
        || set.preview.request.expected_profile_sha256 != set.input_profile_sha256
        || set.preview.request.template != assistance_template_ref(&set.template)?
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "candidate_set",
            "candidate set inputs and exact fingerprints disagree",
        ));
    }
    validate_assistance_request(&set.input_profile, &set.template, &set.preview.request)?;
    if !matches!(
        (set.origin, set.preview.request.provider.mode),
        (
            AssistanceGenerationOrigin::OfflineScaffold,
            AssistanceProviderMode::Offline
        ) | (
            AssistanceGenerationOrigin::ProviderAdapter,
            AssistanceProviderMode::External
        )
    ) {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "origin",
            "candidate origin must match the exact offline or adapter provider mode",
        ));
    }
    if set.candidates.is_empty() || set.candidates.len() > MAX_CANDIDATES {
        return Err(invalid_value(
            "candidates",
            "candidate set requires a bounded non-empty candidate map",
        ));
    }
    let mut field_indexes = BTreeMap::<AssistanceFieldKind, BTreeSet<usize>>::new();
    for (id, candidate) in &set.candidates {
        validate_assistance_candidate(&format!("candidates.{id}"), id, candidate, set)?;
        if !field_indexes
            .entry(candidate.field)
            .or_default()
            .insert(candidate.provider_index)
        {
            return Err(invalid_value(
                format!("candidates.{id}.provider_index"),
                "candidate provider indexes must be unique within each field",
            ));
        }
    }
    let field_counts = field_indexes
        .iter()
        .map(|(field, indexes)| (*field, indexes.len()))
        .collect::<BTreeMap<_, _>>();
    if set.preview.request.fields.iter().any(|field| {
        field_counts.get(field).copied().unwrap_or_default() == 0
            || field_counts.get(field).copied().unwrap_or_default()
                > usize::from(set.preview.request.settings.candidates_per_field)
    }) || field_counts
        .keys()
        .any(|field| !set.preview.request.fields.contains(field))
    {
        return Err(invalid_value(
            "candidates",
            "candidate counts do not match requested fields and limits",
        ));
    }
    for (field, indexes) in &field_indexes {
        if indexes.iter().copied().ne(0..indexes.len()) {
            return Err(invalid_value(
                format!("candidates.{}.provider_index", field.as_str()),
                "candidate provider indexes must form a contiguous zero-based sequence",
            ));
        }
    }
    Ok(())
}

fn validate_assistance_candidate(
    path: &str,
    map_id: &str,
    candidate: &AssistanceCandidate,
    set: &AssistanceCandidateSet,
) -> Result<(), CharacterError> {
    validate_local_id(&format!("{path}.id"), &candidate.id)?;
    let one_based_index = candidate
        .provider_index
        .checked_add(1)
        .ok_or_else(|| invalid_value(format!("{path}.provider_index"), "index overflowed"))?;
    let expected_id = format!(
        "{}_g{}_{}",
        candidate.field.as_str(),
        candidate.generation,
        one_based_index
    );
    if candidate.id != map_id
        || candidate.id != expected_id
        || candidate.character_id != set.input_profile.id
        || candidate.value.field() != candidate.field
        || !set.preview.request.fields.contains(&candidate.field)
        || candidate.generation != set.preview.request.generation
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "candidate identity, owner, field, or generation disagrees with its request",
        ));
    }
    let specification = set.template.fields.get(&candidate.field).ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "candidate field is absent from its exact template",
        )
    })?;
    if candidate.target_path != specification.target_path
        || candidate.provider_index
            >= usize::from(set.preview.request.settings.candidates_per_field)
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "candidate target or provider index disagrees with its template and request",
        ));
    }
    validate_candidate_value(
        &format!("{path}.value"),
        &candidate.value,
        specification,
        &set.preview.scope.known_character_ids,
    )?;
    if candidate.evidence.is_empty()
        || candidate.evidence.len() > set.preview.request.included_inputs.len()
        || candidate
            .evidence
            .windows(2)
            .any(|pair| pair[0].input_path >= pair[1].input_path)
    {
        return Err(invalid_value(
            format!("{path}.evidence"),
            "candidate evidence must be non-empty, sorted, unique, and bounded by disclosed inputs",
        ));
    }
    for evidence in &candidate.evidence {
        validate_sha256(
            &format!("{path}.evidence.input_sha256"),
            &evidence.input_sha256,
        )?;
        validate_public_text(
            &format!("{path}.evidence.explanation"),
            &evidence.explanation,
            1,
            512,
            false,
        )?;
        let input = AssistanceInputField::all()
            .iter()
            .find(|input| input.profile_path() == evidence.input_path)
            .copied()
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{path}.evidence.input_path"),
                    "candidate evidence path is outside the closed assistance input vocabulary",
                )
            })?;
        let value = set
            .preview
            .scope
            .included_values
            .get(&input)
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("{path}.evidence.input_path"),
                    "candidate evidence was not included in the approved scope",
                )
            })?;
        if evidence.input_sha256 != assistance_hash(value)? {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                format!("{path}.evidence.input_sha256"),
                "candidate evidence fingerprint does not match the approved input value",
            ));
        }
        let expected_explanation = format!(
            "The approved {} input was available to the selected request template.",
            evidence.input_path
        );
        if evidence.explanation != expected_explanation {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                format!("{path}.evidence.explanation"),
                "candidate evidence explanation does not reproduce from its approved input path",
            ));
        }
    }
    let expected_checks = all_validation_checks();
    if !candidate.validation.safe
        || candidate.validation.checks != expected_checks
        || candidate.validation.total_text_chars != candidate_text_chars(&candidate.value)
    {
        return Err(invalid_value(
            format!("{path}.validation"),
            "candidate validation receipt is incomplete or inconsistent",
        ));
    }
    Ok(())
}

pub fn validate_assistance_advisory_review_structure(
    review: &AssistanceAdvisoryReview,
) -> Result<(), CharacterError> {
    require_version(
        review.advisory_format_version,
        ASSISTANCE_ADVISORY_REVIEW_FORMAT_VERSION,
        "advisory_format_version",
    )?;
    validate_namespaced_id("id", &review.id)?;
    validate_sha256("candidate_set_sha256", &review.candidate_set_sha256)?;
    validate_namespaced_id("reviewer", &review.reviewer)?;
    validate_public_text("method", &review.method, 1, 512, false)?;
    if !review.advisory_only
        || review.canonical_write_back
        || review.assessments.is_empty()
        || review.assessments.len() > MAX_CANDIDATES
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "advisory",
            "advisory review must be non-empty, advisory-only, and have no canonical write-back",
        ));
    }
    let mut proposed_edit_text_chars = 0usize;
    for (id, assessment) in &review.assessments {
        validate_local_id("assessments.id", id)?;
        if assessment.candidate_id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("assessments.{id}.candidate_id"),
                "advisory assessment id must equal its containing map key",
            ));
        }
        for score in [
            assessment.relevance_micros,
            assessment.consistency_micros,
            assessment.safety_micros,
        ] {
            if score > 1_000_000 {
                return Err(invalid_value(
                    format!("assessments.{id}"),
                    "advisory scores must remain within unit millionths",
                ));
            }
        }
        if assessment.issues.len() > 256 {
            return Err(invalid_value(
                format!("assessments.{id}.issues"),
                "advisory issue list is too large",
            ));
        }
        for issue in &assessment.issues {
            validate_local_id("issue.code", &issue.code)?;
            validate_public_text("issue.message", &issue.message, 1, 1_024, false)?;
        }
        validate_public_text(
            &format!("assessments.{id}.explanation"),
            &assessment.explanation,
            1,
            2_048,
            false,
        )?;
        if let Some(edit) = &assessment.proposed_edit {
            for text in candidate_texts(edit) {
                validate_public_text(
                    &format!("assessments.{id}.proposed_edit"),
                    text,
                    1,
                    16_384,
                    true,
                )?;
            }
            if let AssistanceCandidateValue::RelationshipCue {
                other_character_id: Some(other_character_id),
                ..
            } = edit
            {
                validate_namespaced_id(
                    &format!("assessments.{id}.proposed_edit.other_character_id"),
                    other_character_id,
                )?;
            }
            proposed_edit_text_chars = proposed_edit_text_chars
                .checked_add(candidate_text_chars(edit))
                .ok_or_else(|| {
                    invalid_value("assessments", "advisory proposed edit size overflowed")
                })?;
        }
    }
    if proposed_edit_text_chars > 1_048_576 {
        return Err(invalid_value(
            "assessments",
            "advisory proposed edit text exceeds the global assistance bound",
        ));
    }
    Ok(())
}

pub fn validate_assistance_advisory_review(
    set: &AssistanceCandidateSet,
    review: &AssistanceAdvisoryReview,
) -> Result<(), CharacterError> {
    validate_assistance_candidate_set(set)?;
    validate_assistance_advisory_review_structure(review)?;
    if review.candidate_set_sha256 != assistance_candidate_set_fingerprint(set)?
        || review.assessments.len() != set.candidates.len()
        || review
            .assessments
            .keys()
            .any(|id| !set.candidates.contains_key(id))
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "advisory",
            "advisory review does not cover the exact immutable candidate set",
        ));
    }
    for (id, assessment) in &review.assessments {
        if let Some(edit) = &assessment.proposed_edit {
            let candidate = &set.candidates[id];
            if edit.field() != candidate.field {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("assessments.{id}.proposed_edit"),
                    "advisory edit must retain the candidate field kind",
                ));
            }
            validate_candidate_value(
                &format!("assessments.{id}.proposed_edit"),
                edit,
                &set.template.fields[&candidate.field],
                &set.preview.scope.known_character_ids,
            )?;
        }
    }
    Ok(())
}

pub fn validate_assistance_decision_review_structure(
    review: &AssistanceDecisionReview,
) -> Result<(), CharacterError> {
    require_version(
        review.review_format_version,
        ASSISTANCE_DECISION_REVIEW_FORMAT_VERSION,
        "review_format_version",
    )?;
    validate_namespaced_id("id", &review.id)?;
    validate_sha256("candidate_set_sha256", &review.candidate_set_sha256)?;
    validate_sha256("input_profile_sha256", &review.input_profile_sha256)?;
    validate_namespaced_id("author", &review.author)?;
    validate_public_text("rationale", &review.rationale, 1, 2_048, false)?;
    if review.decisions.is_empty() || review.decisions.len() > MAX_CANDIDATES {
        return Err(invalid_value(
            "decisions",
            "author review requires a bounded non-empty decision map",
        ));
    }
    let mut edited_text_chars = 0usize;
    for (id, decision) in &review.decisions {
        validate_local_id("decisions.id", id)?;
        match decision {
            AssistanceDecision::Accept { rationale }
            | AssistanceDecision::Reject { rationale }
            | AssistanceDecision::Defer { rationale }
            | AssistanceDecision::Regenerate { rationale } => {
                validate_public_text("decision.rationale", rationale, 1, 2_048, false)?;
            }
            AssistanceDecision::Edit { value, rationale } => {
                validate_public_text("decision.rationale", rationale, 1, 2_048, false)?;
                for text in candidate_texts(value) {
                    validate_public_text("decision.value", text, 1, 16_384, true)?;
                }
                if let AssistanceCandidateValue::RelationshipCue {
                    other_character_id: Some(other_character_id),
                    ..
                } = value
                {
                    validate_namespaced_id(
                        "decision.value.other_character_id",
                        other_character_id,
                    )?;
                }
                edited_text_chars = edited_text_chars
                    .checked_add(candidate_text_chars(value))
                    .ok_or_else(|| invalid_value("decisions", "edited text size overflowed"))?;
            }
        }
    }
    if edited_text_chars > 1_048_576 {
        return Err(invalid_value(
            "decisions",
            "edited candidate text exceeds the global assistance bound",
        ));
    }
    Ok(())
}

pub fn validate_assistance_decision_review(
    set: &AssistanceCandidateSet,
    review: &AssistanceDecisionReview,
) -> Result<(), CharacterError> {
    validate_assistance_candidate_set(set)?;
    validate_assistance_decision_review_structure(review)?;
    if review.candidate_set_sha256 != assistance_candidate_set_fingerprint(set)?
        || review.input_profile_sha256 != set.input_profile_sha256
        || review.decisions.len() != set.candidates.len()
        || review
            .decisions
            .keys()
            .any(|id| !set.candidates.contains_key(id))
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "decisions",
            "author decisions do not cover the exact immutable candidate set",
        ));
    }
    for (id, decision) in &review.decisions {
        if let AssistanceDecision::Edit { value, .. } = decision {
            let candidate = &set.candidates[id];
            if value.field() != candidate.field {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("decisions.{id}.value"),
                    "edited value must retain the candidate field kind",
                ));
            }
            let specification = &set.template.fields[&candidate.field];
            validate_candidate_value(
                &format!("decisions.{id}.value"),
                value,
                specification,
                &set.preview.scope.known_character_ids,
            )?;
        }
    }
    Ok(())
}

pub fn validate_assistance_receipt(receipt: &AssistanceReceipt) -> Result<(), CharacterError> {
    require_version(
        receipt.receipt_format_version,
        ASSISTANCE_RECEIPT_FORMAT_VERSION,
        "receipt_format_version",
    )?;
    validate_namespaced_id("id", &receipt.id)?;
    validate_profile(&receipt.input_profile)?;
    validate_profile(&receipt.output_profile)?;
    validate_sha256("input_profile_sha256", &receipt.input_profile_sha256)?;
    validate_sha256("output_profile_sha256", &receipt.output_profile_sha256)?;
    validate_assistance_candidate_set(&receipt.candidate_set)?;
    validate_sha256("candidate_set_sha256", &receipt.candidate_set_sha256)?;
    validate_assistance_decision_review(&receipt.candidate_set, &receipt.decision_review)?;
    validate_sha256("decision_review_sha256", &receipt.decision_review_sha256)?;
    validate_sorted_hashes("advisory_review_sha256s", &receipt.advisory_review_sha256s)?;
    validate_sorted_local_ids("accepted_suggestion_ids", &receipt.accepted_suggestion_ids)?;
    validate_sorted_local_ids("rejected_candidate_ids", &receipt.rejected_candidate_ids)?;
    validate_sorted_local_ids("deferred_candidate_ids", &receipt.deferred_candidate_ids)?;
    validate_sorted_local_ids(
        "regenerate_candidate_ids",
        &receipt.regenerate_candidate_ids,
    )?;
    if receipt.input_profile_sha256 != assistance_profile_fingerprint(&receipt.input_profile)?
        || receipt.output_profile_sha256 != assistance_profile_fingerprint(&receipt.output_profile)?
        || receipt.candidate_set_sha256
            != assistance_candidate_set_fingerprint(&receipt.candidate_set)?
        || receipt.decision_review_sha256
            != assistance_decision_review_fingerprint(&receipt.decision_review)?
        || receipt.input_profile != receipt.candidate_set.input_profile
        || receipt.input_profile_sha256 != receipt.candidate_set.input_profile_sha256
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "receipt",
            "assistance receipt fingerprints or immutable inputs disagree",
        ));
    }
    validate_assistance_profile_delta(&receipt.input_profile, &receipt.output_profile)?;
    let expected = build_assistance_receipt(
        &receipt.input_profile,
        &receipt.candidate_set,
        &receipt.decision_review,
        &receipt.advisory_review_sha256s,
    )?;
    if expected != *receipt {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "receipt",
            "assistance receipt does not reproduce from its immutable review inputs",
        ));
    }
    Ok(())
}

pub fn validate_assistance_batch_request_structure(
    request: &AssistanceBatchRequest,
) -> Result<(), CharacterError> {
    require_version(
        request.batch_request_format_version,
        ASSISTANCE_BATCH_REQUEST_FORMAT_VERSION,
        "batch_request_format_version",
    )?;
    validate_namespaced_id("id", &request.id)?;
    validate_namespaced_id("collection_id", &request.collection_id)?;
    validate_sha256(
        "expected_collection_sha256",
        &request.expected_collection_sha256,
    )?;
    validate_template_ref("template", &request.template)?;
    validate_provider_ref("provider", &request.provider)?;
    validate_requested_fields("fields", &request.fields)?;
    validate_included_inputs("included_inputs", &request.included_inputs)?;
    validate_assistance_settings(&request.settings)?;
    if request.seed.is_some() && !request.provider.supports_seed
        || request.provider.mode == AssistanceProviderMode::Offline && request.seed.is_none()
    {
        return Err(invalid_value(
            "seed",
            "batch seed is incompatible with the selected provider capabilities",
        ));
    }
    if request.provider.mode == AssistanceProviderMode::Offline
        && request.settings.candidates_per_field > OFFLINE_VARIANTS
    {
        return Err(invalid_value(
            "settings.candidates_per_field",
            "the v1 offline scaffold supports at most four distinct candidates per field",
        ));
    }
    validate_batch_filter(&request.filter)?;
    if request.budget.maximum_characters == 0
        || request.budget.maximum_characters > MAX_BATCH_CHARACTERS
        || request.budget.maximum_provider_calls == 0
        || request.budget.maximum_candidates == 0
        || request.budget.maximum_input_chars == 0
        || request.rate_limit_calls_per_resume == 0
        || request.rate_limit_calls_per_resume > request.budget.maximum_provider_calls
    {
        return Err(invalid_value(
            "budget",
            "batch budgets and deterministic rate limit must be positive and internally bounded",
        ));
    }
    if request.retry.maximum_retries_per_character > 32
        || !strictly_sorted_unique(&request.retry.retryable_codes)
    {
        return Err(invalid_value(
            "retry",
            "retry policy must be bounded with sorted unique failure codes",
        ));
    }
    validate_provenance(&request.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "assistance batch request provenance is invalid",
        )
    })
}

pub fn validate_assistance_batch_preview(
    preview: &AssistanceBatchPreview,
) -> Result<(), CharacterError> {
    require_version(
        preview.batch_preview_format_version,
        ASSISTANCE_BATCH_PREVIEW_FORMAT_VERSION,
        "batch_preview_format_version",
    )?;
    validate_assistance_batch_request_structure(&preview.request)?;
    validate_sha256("request_sha256", &preview.request_sha256)?;
    validate_sha256("template_sha256", &preview.template_sha256)?;
    validate_sorted_namespaced_ids("ordered_target_ids", &preview.ordered_target_ids)?;
    if preview.provider_called
        || preview.credential_value_stored
        || preview.request_sha256 != assistance_hash(&preview.request)?
        || preview.template_sha256 != preview.request.template.sha256
        || preview.previews.len() != preview.ordered_target_ids.len()
        || preview
            .previews
            .keys()
            .ne(preview.ordered_target_ids.iter())
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "batch_preview",
            "batch preview scope, ordering, or dry-run invariants disagree",
        ));
    }
    let mut total = 0usize;
    for (id, single) in &preview.previews {
        validate_assistance_preview(single)?;
        if single.scope.profile_id != *id
            || single.request.provider != preview.request.provider
            || single.request.fields != preview.request.fields
            || single.request.included_inputs != preview.request.included_inputs
            || single.request.settings != preview.request.settings
            || single.request.template != preview.request.template
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("previews.{id}"),
                "single preview does not derive from its batch request",
            ));
        }
        total = total
            .checked_add(single.scope.serialized_character_count)
            .ok_or_else(|| invalid_value("total_input_chars", "batch input size overflowed"))?;
    }
    if total != preview.total_input_chars {
        return Err(invalid_value(
            "total_input_chars",
            "batch input character count does not match its visible previews",
        ));
    }
    Ok(())
}

pub fn validate_assistance_job(job: &AssistanceJob) -> Result<(), CharacterError> {
    require_version(
        job.job_format_version,
        ASSISTANCE_JOB_FORMAT_VERSION,
        "job_format_version",
    )?;
    validate_namespaced_id("id", &job.id)?;
    validate_character_collection(&job.input_collection)?;
    validate_sha256("input_collection_sha256", &job.input_collection_sha256)?;
    validate_assistance_template(&job.template)?;
    validate_assistance_batch_preview(&job.batch_preview)?;
    validate_sha256("batch_preview_sha256", &job.batch_preview_sha256)?;
    validate_assistance_execution_approval(&job.approval)?;
    validate_sha256("approval_sha256", &job.approval_sha256)?;
    validate_sorted_namespaced_ids("ordered_target_ids", &job.ordered_target_ids)?;
    validate_sorted_namespaced_ids("completed_target_ids", &job.completed_target_ids)?;
    let expected_batch_preview = preview_assistance_batch(
        &job.input_collection,
        &job.template,
        &job.batch_preview.request,
    )?;
    if job.input_collection_sha256 != collection_fingerprint(&job.input_collection)?
        || job.input_collection_sha256 != job.batch_preview.request.expected_collection_sha256
        || job.template.id != job.batch_preview.request.template.id
        || assistance_template_ref(&job.template)? != job.batch_preview.request.template
        || job.batch_preview != expected_batch_preview
        || job.batch_preview_sha256 != assistance_hash(&job.batch_preview)?
        || job.approval_sha256 != assistance_approval_fingerprint(&job.approval)?
        || job.approval.preview_sha256 != job.batch_preview_sha256
        || job.approval.parent_batch_preview_sha256.is_some()
        || job.ordered_target_ids != job.batch_preview.ordered_target_ids
        || job.next_index > job.ordered_target_ids.len()
        || job.completed_target_ids != job.ordered_target_ids[..job.next_index]
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "job",
            "assistance job fingerprints, approval, or deterministic cursor disagree",
        ));
    }
    let request = &job.batch_preview.request;
    if job.provider_calls > request.budget.maximum_provider_calls
        || job.candidates_generated > request.budget.maximum_candidates
        || job.input_chars_consumed > request.budget.maximum_input_chars
        || job.candidate_sets.len() + job.failures.len() != job.next_index
    {
        return Err(invalid_value(
            "job",
            "assistance job counters exceed their pinned budgets or cursor",
        ));
    }
    let mut expected_provider_calls = 0usize;
    let mut expected_input_chars = 0usize;
    for (id, attempt) in &job.attempts {
        validate_namespaced_id("attempts.id", id)?;
        if !job.ordered_target_ids.contains(id)
            || usize::from(*attempt) > usize::from(request.retry.maximum_retries_per_character) + 1
        {
            return Err(invalid_value(
                "attempts",
                "job attempt record is outside its target list or retry policy",
            ));
        }
        expected_provider_calls = expected_provider_calls
            .checked_add(usize::from(*attempt))
            .ok_or_else(|| invalid_value("provider_calls", "attempt counter overflowed"))?;
        let input_chars = job.batch_preview.previews[id]
            .scope
            .serialized_character_count
            .checked_mul(usize::from(*attempt))
            .ok_or_else(|| invalid_value("input_chars_consumed", "input size overflowed"))?;
        expected_input_chars = expected_input_chars
            .checked_add(input_chars)
            .ok_or_else(|| invalid_value("input_chars_consumed", "input size overflowed"))?;
    }
    let expected_candidates = job.candidate_sets.values().try_fold(0usize, |total, set| {
        total
            .checked_add(set.candidates.len())
            .ok_or_else(|| invalid_value("candidates_generated", "counter overflowed"))
    })?;
    if job.provider_calls != expected_provider_calls
        || job.input_chars_consumed != expected_input_chars
        || job.candidates_generated != expected_candidates
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "job.counters",
            "assistance job counters do not reproduce from its attempts and candidate sets",
        ));
    }
    for (id, set) in &job.candidate_sets {
        validate_namespaced_id("candidate_sets.id", id)?;
        validate_assistance_candidate_set(set)?;
        validate_assistance_job_candidate_set(job, id, set, &format!("candidate_sets.{id}"))?;
    }
    match request.cache_policy {
        AssistanceCachePolicy::Disabled if !job.cache.is_empty() => {
            return Err(invalid_value(
                "cache",
                "disabled batch cache must remain empty",
            ));
        }
        _ => {}
    }
    for (key, set) in &job.cache {
        validate_sha256("cache.key", key)?;
        validate_assistance_candidate_set(set)?;
        let id = &set.input_profile.id;
        validate_assistance_job_candidate_set(job, id, set, &format!("cache.{key}"))?;
        if assistance_hash(&set.preview.payload)? != *key || job.candidate_sets.get(id) != Some(set)
        {
            return Err(error(
                CharacterDiagnosticCode::StaleInput,
                "cache",
                "job-local cache key or value does not match a completed approved candidate set",
            ));
        }
    }
    for (id, failure) in &job.failures {
        validate_namespaced_id("failures.id", id)?;
        if failure.character_id != *id
            || !job.completed_target_ids.contains(id)
            || failure.attempts == 0
            || job.attempts.get(id).copied() != Some(failure.attempts)
            || failure.retry_exhausted
                && (!request.retry.retryable_codes.contains(&failure.code)
                    || failure.attempts <= request.retry.maximum_retries_per_character)
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("failures.{id}"),
                "job failure does not match a completed target and attempt count",
            ));
        }
    }
    for id in &job.completed_target_ids {
        if job.candidate_sets.contains_key(id) == job.failures.contains_key(id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("completed_target_ids.{id}"),
                "every completed assistance target requires exactly one candidate set or safe failure",
            ));
        }
    }
    match job.state {
        AssistanceJobState::ReadyForReview if job.next_index == job.ordered_target_ids.len() => {}
        AssistanceJobState::Cancelled if job.cancellation_rationale.is_some() => {}
        AssistanceJobState::Running
        | AssistanceJobState::RateLimited
        | AssistanceJobState::BudgetExhausted
            if job.next_index < job.ordered_target_ids.len()
                && job.cancellation_rationale.is_none() => {}
        _ => {
            return Err(invalid_value(
                "state",
                "assistance job state does not match its cursor or cancellation record",
            ));
        }
    }
    if let Some(rationale) = &job.cancellation_rationale {
        validate_public_text("cancellation_rationale", rationale, 1, 2_048, false)?;
    }
    Ok(())
}

fn validate_assistance_job_candidate_set(
    job: &AssistanceJob,
    id: &str,
    set: &AssistanceCandidateSet,
    path: &str,
) -> Result<(), CharacterError> {
    let expected_preview = job.batch_preview.previews.get(id).ok_or_else(|| {
        error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "job candidate set references no approved batch preview",
        )
    })?;
    if set.input_profile.id != id
        || job.input_collection.characters.get(id) != Some(&set.input_profile)
        || !job
            .completed_target_ids
            .iter()
            .any(|completed_id| completed_id == id)
        || set.template != job.template
        || set.preview != *expected_preview
        || set.approval.parent_batch_preview_sha256.as_deref()
            != Some(job.batch_preview_sha256.as_str())
        || set.approval.author != job.approval.author
        || set.approval.rationale != job.approval.rationale
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            path,
            "job candidate set does not reproduce from its exact approved batch target",
        ));
    }
    Ok(())
}

pub fn validate_assistance_batch_receipt(
    receipt: &AssistanceBatchReceipt,
) -> Result<(), CharacterError> {
    require_version(
        receipt.batch_receipt_format_version,
        ASSISTANCE_BATCH_RECEIPT_FORMAT_VERSION,
        "batch_receipt_format_version",
    )?;
    validate_namespaced_id("id", &receipt.id)?;
    validate_character_collection(&receipt.input_collection)?;
    validate_character_collection(&receipt.output_collection)?;
    validate_sha256("input_collection_sha256", &receipt.input_collection_sha256)?;
    validate_sha256(
        "output_collection_sha256",
        &receipt.output_collection_sha256,
    )?;
    validate_sha256("job_sha256", &receipt.job_sha256)?;
    if receipt.input_collection_sha256 != collection_fingerprint(&receipt.input_collection)?
        || receipt.output_collection_sha256 != collection_fingerprint(&receipt.output_collection)?
        || receipt.receipts.is_empty()
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "batch_receipt",
            "batch receipt collection fingerprints or per-profile receipts are invalid",
        ));
    }
    let mut expected = receipt.input_collection.clone();
    for (id, single) in &receipt.receipts {
        validate_namespaced_id("receipts.id", id)?;
        validate_assistance_receipt(single)?;
        if single.input_profile.id != *id
            || expected.characters.get(id) != Some(&single.input_profile)
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("receipts.{id}"),
                "batch receipt profile does not match the exact input collection",
            ));
        }
        expected
            .characters
            .insert(id.clone(), single.output_profile.clone());
    }
    expected.revision = expected
        .revision
        .checked_add(u64::from(
            expected.characters != receipt.input_collection.characters,
        ))
        .ok_or_else(|| invalid_value("output_collection.revision", "revision overflowed"))?;
    if expected != receipt.output_collection {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "output_collection",
            "batch receipt output does not reproduce from its single-profile receipts",
        ));
    }
    Ok(())
}

pub fn validate_assistance_comparison(
    report: &AssistanceComparisonReport,
) -> Result<(), CharacterError> {
    require_version(
        report.comparison_format_version,
        ASSISTANCE_COMPARISON_FORMAT_VERSION,
        "comparison_format_version",
    )?;
    validate_namespaced_id("id", &report.id)?;
    validate_sha256("input_profile_sha256", &report.input_profile_sha256)?;
    if !(2..=64).contains(&report.providers.len())
        || !strictly_sorted_unique(&report.providers)
        || report.entries.is_empty()
        || report.entries.len() > MAX_CANDIDATES * 64
        || !report.advisory_only
        || report.canonical_write_back
        || report.selected_candidate_id.is_some()
        || report.order_basis != "provider_coordinate_then_candidate_id"
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "comparison",
            "provider comparison must be sorted, advisory-only, and select no candidate",
        ));
    }
    for provider in &report.providers {
        validate_provider_ref("providers", provider)?;
    }
    let mut prior = None;
    let mut providers_with_entries = BTreeSet::new();
    for entry in &report.entries {
        validate_provider_ref("entries.provider", &entry.provider)?;
        validate_local_id("entries.candidate_id", &entry.candidate_id)?;
        if !report.providers.contains(&entry.provider) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "entries.provider",
                "comparison entry references an undeclared provider",
            ));
        }
        providers_with_entries.insert(entry.provider.clone());
        let score_count = [
            entry.relevance_micros,
            entry.consistency_micros,
            entry.safety_micros,
        ]
        .into_iter()
        .flatten()
        .count();
        if !matches!(score_count, 0 | 3) || score_count == 0 && !entry.issues.is_empty() {
            return Err(invalid_value(
                "entries",
                "comparison entries require either a complete advisory assessment or none",
            ));
        }
        for score in [
            entry.relevance_micros,
            entry.consistency_micros,
            entry.safety_micros,
        ]
        .into_iter()
        .flatten()
        {
            if score > 1_000_000 {
                return Err(invalid_value(
                    "entries",
                    "comparison advisory scores must remain within unit millionths",
                ));
            }
        }
        if entry.issues.len() > 256 {
            return Err(invalid_value(
                "entries.issues",
                "comparison advisory issue list is too large",
            ));
        }
        for issue in &entry.issues {
            validate_local_id("entries.issues.code", &issue.code)?;
            validate_public_text("entries.issues.message", &issue.message, 1, 1_024, false)?;
        }
        let key = (&entry.provider, entry.candidate_id.as_str());
        if prior.is_some_and(|prior| prior >= key) {
            return Err(invalid_value(
                "entries",
                "comparison entries must follow provider-coordinate and candidate-id order",
            ));
        }
        prior = Some(key);
    }
    if !report
        .providers
        .iter()
        .all(|provider| providers_with_entries.contains(provider))
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "providers",
            "every compared provider requires at least one candidate entry",
        ));
    }
    Ok(())
}

/// Render the exact provider payload and expected response contract without invoking an adapter.
pub fn preview_assistance_request(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    request: &AssistanceRequest,
) -> Result<AssistancePreview, CharacterError> {
    validate_character_collection(collection)?;
    let profile = collection
        .characters
        .get(&request.profile_id)
        .ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "profile_id",
                "assistance request profile is absent from the collection",
            )
        })?;
    preview_assistance_profile(
        profile,
        collection.characters.keys().cloned().collect(),
        template,
        request,
    )
}

fn preview_assistance_profile(
    profile: &CharacterProfile,
    known_character_ids: Vec<String>,
    template: &AssistanceTemplate,
    request: &AssistanceRequest,
) -> Result<AssistancePreview, CharacterError> {
    validate_assistance_request(profile, template, request)?;
    if !strictly_sorted_unique(&known_character_ids) || !known_character_ids.contains(&profile.id) {
        return Err(invalid_value(
            "known_character_ids",
            "assistance scope character ids must be sorted, unique, and include the profile",
        ));
    }
    let included_values = request
        .included_inputs
        .iter()
        .copied()
        .map(|input| Ok((input, assistance_input_value(profile, input)?)))
        .collect::<Result<BTreeMap<_, _>, CharacterError>>()?;
    for (field, value) in &included_values {
        validate_disclosed_value(field.profile_path(), value)?;
    }
    let serialized_character_count = to_pretty_json(&included_values)
        .map_err(|_| assistance_encoding_error())?
        .chars()
        .count();
    let scope = AssistanceScopePreview {
        profile_id: profile.id.clone(),
        input_profile_sha256: request.expected_profile_sha256.clone(),
        included_values: included_values.clone(),
        known_character_ids: known_character_ids.clone(),
        serialized_character_count,
    };
    let instructions = request
        .fields
        .iter()
        .map(|field| (*field, template.fields[field].instructions.clone()))
        .collect();
    let required_evidence = request
        .fields
        .iter()
        .map(|field| {
            (
                *field,
                template.fields[field]
                    .required_inputs
                    .iter()
                    .map(|input| input.profile_path().to_owned())
                    .collect(),
            )
        })
        .collect();
    let payload = AssistanceProviderPayload {
        payload_format_version: 1,
        request_id: request.id.clone(),
        profile_id: profile.id.clone(),
        fields: request.fields.clone(),
        instructions,
        required_evidence,
        included_values,
        known_character_ids,
        settings: request.settings.clone(),
        seed: request.seed,
        generation: request.generation,
        expected_response_schema: PROVIDER_RESPONSE_SCHEMA_ID.to_owned(),
    };
    let preview = AssistancePreview {
        preview_format_version: ASSISTANCE_PREVIEW_FORMAT_VERSION,
        request: request.clone(),
        request_sha256: assistance_request_fingerprint(request)?,
        template_sha256: assistance_template_fingerprint(template)?,
        scope,
        payload,
        expected_output: AssistanceExpectedOutput {
            response_format_version: ASSISTANCE_PROVIDER_RESPONSE_FORMAT_VERSION,
            schema_id: PROVIDER_RESPONSE_SCHEMA_ID.to_owned(),
            fields: request.fields.clone(),
            maximum_candidates_per_field: request.settings.candidates_per_field,
            maximum_output_chars: request.settings.maximum_output_chars,
        },
        provider_called: false,
        credential_value_stored: false,
    };
    validate_assistance_preview(&preview)?;
    Ok(preview)
}

/// Record an explicit author decision to send the already-visible scope to its selected adapter.
pub fn approve_assistance_preview(
    preview: &AssistancePreview,
    author: impl Into<String>,
    rationale: impl Into<String>,
) -> Result<AssistanceExecutionApproval, CharacterError> {
    validate_assistance_preview(preview)?;
    let approval = AssistanceExecutionApproval {
        approval_format_version: ASSISTANCE_APPROVAL_FORMAT_VERSION,
        preview_sha256: assistance_preview_fingerprint(preview)?,
        parent_batch_preview_sha256: None,
        allow_provider_call: true,
        author: author.into(),
        rationale: rationale.into(),
    };
    validate_assistance_execution_approval(&approval)?;
    Ok(approval)
}

/// Execute one explicitly previewed and approved provider operation.
pub fn execute_assistance_request<P: CharacterAssistanceProvider>(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    preview: &AssistancePreview,
    approval: &AssistanceExecutionApproval,
    provider: &mut P,
) -> Result<AssistanceCandidateSet, AssistanceError> {
    validate_character_collection(collection)?;
    validate_assistance_template(template)?;
    validate_assistance_preview(preview)?;
    validate_assistance_execution_approval(approval)?;
    let expected_preview = preview_assistance_request(collection, template, &preview.request)?;
    if expected_preview != *preview
        || approval.preview_sha256 != assistance_preview_fingerprint(preview)?
        || approval.parent_batch_preview_sha256.is_some()
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "approval",
            "execution approval does not match the exact current visible scope preview",
        )
        .into());
    }
    execute_approved_preview(collection, template, preview, approval, provider)
}

fn execute_approved_preview<P: CharacterAssistanceProvider>(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    preview: &AssistancePreview,
    approval: &AssistanceExecutionApproval,
    provider: &mut P,
) -> Result<AssistanceCandidateSet, AssistanceError> {
    if provider.descriptor() != preview.request.provider {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "provider",
            "configured adapter does not match the exact provider coordinate in the request",
        )
        .into());
    }
    match (
        preview.request.provider.credential_required,
        provider.credential_status(),
    ) {
        (
            false,
            AssistanceCredentialStatus::NotRequired | AssistanceCredentialStatus::Configured,
        )
        | (true, AssistanceCredentialStatus::Configured) => {}
        _ => {
            return Err(AssistanceError::Provider(AssistanceProviderFailure {
                code: AssistanceProviderFailureCode::Authentication,
                retryable: false,
            }));
        }
    }
    let raw = provider
        .generate(&preview.payload)
        .map_err(AssistanceError::Provider)?;
    if raw.chars().count() > template.safety.maximum_total_response_chars
        || raw.chars().count() > preview.request.settings.maximum_output_chars
    {
        return Err(invalid_value(
            "provider_response",
            "provider response exceeds the exact approved output bounds",
        )
        .into());
    }
    let response: AssistanceProviderResponse =
        parse_strict_json(&raw).map_err(|_| assistance_encoding_error())?;
    normalize_provider_response(collection, template, preview, approval, &response)
        .map_err(AssistanceError::Contract)
}

fn normalize_provider_response(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    preview: &AssistancePreview,
    approval: &AssistanceExecutionApproval,
    response: &AssistanceProviderResponse,
) -> Result<AssistanceCandidateSet, CharacterError> {
    validate_assistance_provider_response_structure(response)?;
    if response.request_id != preview.request.id {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "provider_response.request_id",
            "provider response does not match the exact approved request",
        ));
    }
    let mut normalized = response
        .candidates
        .iter()
        .cloned()
        .map(|candidate| Ok((assistance_hash(&candidate.value)?, candidate)))
        .collect::<Result<Vec<_>, CharacterError>>()?;
    normalized.sort_by(|left, right| {
        (
            left.1.character_id.as_str(),
            left.1.field,
            left.0.as_str(),
            &left.1.evidence_paths,
        )
            .cmp(&(
                right.1.character_id.as_str(),
                right.1.field,
                right.0.as_str(),
                &right.1.evidence_paths,
            ))
    });
    let mut counts = BTreeMap::<AssistanceFieldKind, usize>::new();
    let mut seen_values = BTreeSet::new();
    let mut candidates = BTreeMap::new();
    let total_text = normalized
        .iter()
        .map(|(_, candidate)| candidate_text_chars(&candidate.value))
        .sum::<usize>();
    if total_text > template.safety.maximum_total_response_chars
        || total_text > preview.request.settings.maximum_output_chars
    {
        return Err(invalid_value(
            "provider_response.candidates",
            "provider candidate text exceeds the exact approved output bounds",
        ));
    }
    for (value_sha, candidate) in normalized {
        if candidate.character_id != preview.request.profile_id
            || !preview.request.fields.contains(&candidate.field)
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "provider_response.candidates",
                "provider returned a character or field outside the approved scope",
            ));
        }
        let specification = &template.fields[&candidate.field];
        validate_candidate_value(
            "provider_response.candidate.value",
            &candidate.value,
            specification,
            &preview.scope.known_character_ids,
        )?;
        if !candidate.evidence_paths.iter().all(|path| {
            specification
                .required_inputs
                .iter()
                .any(|input| input.profile_path() == path)
        }) || specification.required_inputs.iter().any(|input| {
            !candidate
                .evidence_paths
                .iter()
                .any(|path| path == input.profile_path())
        }) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "provider_response.candidate.evidence_paths",
                "provider evidence must cover exactly the field's required approved inputs",
            ));
        }
        if !seen_values.insert((candidate.field, value_sha)) {
            return Err(invalid_value(
                "provider_response.candidates",
                "provider returned a duplicate typed candidate",
            ));
        }
        let index = counts.entry(candidate.field).or_default();
        if *index >= usize::from(preview.request.settings.candidates_per_field) {
            return Err(invalid_value(
                "provider_response.candidates",
                "provider returned more candidates than the approved per-field limit",
            ));
        }
        let provider_index = *index;
        *index += 1;
        let id = format!(
            "{}_g{}_{}",
            candidate.field.as_str(),
            preview.request.generation,
            provider_index + 1
        );
        let evidence = candidate
            .evidence_paths
            .iter()
            .map(|path| {
                let input = AssistanceInputField::all()
                    .iter()
                    .find(|input| input.profile_path() == path)
                    .copied()
                    .ok_or_else(|| {
                        error(
                            CharacterDiagnosticCode::InvalidReference,
                            "provider_response.candidate.evidence_paths",
                            "provider evidence path is outside the closed assistance vocabulary",
                        )
                    })?;
                let value = preview.scope.included_values.get(&input).ok_or_else(|| {
                    error(
                        CharacterDiagnosticCode::InvalidReference,
                        "provider_response.candidate.evidence_paths",
                        "provider evidence path was absent from the approved scope",
                    )
                })?;
                Ok(AssistanceEvidence {
                    input_path: path.clone(),
                    input_sha256: assistance_hash(value)?,
                    explanation: format!(
                        "The approved {} input was available to the selected request template.",
                        input.profile_path()
                    ),
                })
            })
            .collect::<Result<Vec<_>, CharacterError>>()?;
        let total_text_chars = candidate_text_chars(&candidate.value);
        candidates.insert(
            id.clone(),
            AssistanceCandidate {
                id,
                character_id: candidate.character_id,
                field: candidate.field,
                target_path: specification.target_path.clone(),
                value: candidate.value,
                evidence,
                validation: AssistanceCandidateValidation {
                    total_text_chars,
                    checks: all_validation_checks(),
                    safe: true,
                },
                provider_index,
                generation: preview.request.generation,
            },
        );
    }
    if preview
        .request
        .fields
        .iter()
        .any(|field| counts.get(field).copied().unwrap_or_default() == 0)
    {
        return Err(invalid_value(
            "provider_response.candidates",
            "provider response omitted a requested field",
        ));
    }
    let profile = collection
        .characters
        .get(&preview.request.profile_id)
        .ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "profile_id",
                "approved assistance profile is absent from the current collection",
            )
        })?
        .clone();
    let set = AssistanceCandidateSet {
        candidate_set_format_version: ASSISTANCE_CANDIDATE_SET_FORMAT_VERSION,
        id: format!("{}.candidates", preview.request.id),
        input_profile: profile,
        input_profile_sha256: preview.request.expected_profile_sha256.clone(),
        template: template.clone(),
        template_sha256: assistance_template_fingerprint(template)?,
        preview: preview.clone(),
        preview_sha256: assistance_preview_fingerprint(preview)?,
        approval: approval.clone(),
        approval_sha256: assistance_approval_fingerprint(approval)?,
        provider_response_sha256: assistance_hash(response)?,
        origin: if preview.request.provider.mode == AssistanceProviderMode::Offline {
            AssistanceGenerationOrigin::OfflineScaffold
        } else {
            AssistanceGenerationOrigin::ProviderAdapter
        },
        candidates,
    };
    validate_assistance_candidate_set(&set)?;
    Ok(set)
}

fn offline_provider_response(payload: &AssistanceProviderPayload) -> AssistanceProviderResponse {
    let display_name = match payload
        .included_values
        .get(&AssistanceInputField::DisplayName)
    {
        Some(DomainValue::String(value)) => value.as_str(),
        _ => "The character",
    };
    let other_character_id = payload
        .known_character_ids
        .iter()
        .find(|id| *id != &payload.profile_id)
        .cloned();
    let seed = payload.seed.unwrap_or_default();
    let mut candidates = Vec::new();
    for field in &payload.fields {
        for index in 0..payload.settings.candidates_per_field {
            let variant =
                offline_variant(seed, payload.generation, &payload.profile_id, *field, index);
            candidates.push(AssistanceProviderCandidate {
                character_id: payload.profile_id.clone(),
                field: *field,
                value: offline_candidate_value(
                    display_name,
                    *field,
                    variant,
                    other_character_id.as_deref(),
                ),
                evidence_paths: payload
                    .required_evidence
                    .get(field)
                    .cloned()
                    .unwrap_or_default(),
            });
        }
    }
    AssistanceProviderResponse {
        response_format_version: ASSISTANCE_PROVIDER_RESPONSE_FORMAT_VERSION,
        request_id: payload.request_id.clone(),
        candidates,
    }
}

fn offline_variant(
    seed: u64,
    generation: u32,
    profile_id: &str,
    field: AssistanceFieldKind,
    index: u16,
) -> usize {
    let mut hasher = Sha256::new();
    hasher.update(seed.to_le_bytes());
    hasher.update(generation.to_le_bytes());
    hasher.update(profile_id.as_bytes());
    hasher.update(field.as_str().as_bytes());
    (usize::from(hasher.finalize()[0]) + usize::from(index)) % usize::from(OFFLINE_VARIANTS)
}

fn offline_candidate_value(
    name: &str,
    field: AssistanceFieldKind,
    variant: usize,
    other_character_id: Option<&str>,
) -> AssistanceCandidateValue {
    const FOCI: [&str; 4] = [
        "an unfinished promise",
        "a difficult question",
        "a fragile alliance",
        "an untested route",
    ];
    const PRESSURES: [&str; 4] = [
        "limited time",
        "conflicting loyalties",
        "uncertain evidence",
        "an unexpected witness",
    ];
    let focus = FOCI[variant];
    let pressure = PRESSURES[variant];
    match field {
        AssistanceFieldKind::Biography => AssistanceCandidateValue::Biography {
            summary: format!(
                "{name} has learned to approach {focus} through patient observation and deliberate choices."
            ),
            formative_thread: format!(
                "A prior encounter with {pressure} still shapes which questions feel worth pursuing."
            ),
        },
        AssistanceFieldKind::Motivation => AssistanceCandidateValue::Motivation {
            objective: format!("{name} wants to bring {focus} to a clear and useful conclusion."),
            reason: format!(
                "Doing so would turn experience with {pressure} into a choice that helps someone else."
            ),
        },
        AssistanceFieldKind::Fear => AssistanceCandidateValue::Fear {
            concern: format!(
                "{name} worries that acting too quickly could make {focus} harder to repair."
            ),
            trigger: format!("The concern sharpens when {pressure} prevents a careful check."),
        },
        AssistanceFieldKind::GuardedTruth => AssistanceCandidateValue::GuardedTruth {
            truth: format!(
                "{name} privately doubts whether the usual answer to {focus} is still adequate."
            ),
            guarded_because: format!(
                "Sharing the doubt before understanding {pressure} could close off needed cooperation."
            ),
        },
        AssistanceFieldKind::Tension => AssistanceCandidateValue::Tension {
            premise: format!("{name} values a measured response to {focus}."),
            opposing_pressure: format!(
                "The situation demands visible movement while {pressure} makes certainty impossible."
            ),
        },
        AssistanceFieldKind::NarrativeHook => AssistanceCandidateValue::NarrativeHook {
            hook: format!(
                "A new message asks {name} to revisit {focus} before anyone else sees the full pattern."
            ),
            stakes: format!(
                "Ignoring it leaves {pressure} to define the outcome; answering it risks a trusted arrangement."
            ),
        },
        AssistanceFieldKind::PresentationCue => AssistanceCandidateValue::PresentationCue {
            cue: format!(
                "When considering {focus}, show {name} pausing to align one small object before speaking."
            ),
            rationale: format!(
                "The repeatable action makes deliberation visible without treating {pressure} as personality evidence."
            ),
        },
        AssistanceFieldKind::RoleIdea => AssistanceCandidateValue::RoleIdea {
            label: [
                "Question Keeper",
                "Promise Mapper",
                "Boundary Listener",
                "Route Witness",
            ][variant]
                .to_owned(),
            rationale: format!(
                "This optional role frames how {name} might help a scene involving {focus}; it is not a personality fact."
            ),
        },
        AssistanceFieldKind::RelationshipCue => AssistanceCandidateValue::RelationshipCue {
            cue: format!(
                "Invite a conversation where {name} asks what the other person needs before addressing {focus}."
            ),
            other_character_id: other_character_id.map(str::to_owned),
            rationale: format!(
                "The cue creates room to explore trust under {pressure} without asserting a relationship change."
            ),
        },
        AssistanceFieldKind::ContextReaction => AssistanceCandidateValue::ContextReaction {
            context: format!("A familiar plan is interrupted by {pressure}."),
            reaction: format!(
                "{name} names what remains known, asks one clarifying question, and keeps {focus} open for review."
            ),
        },
        AssistanceFieldKind::ExpressionExample => AssistanceCandidateValue::ExpressionExample {
            context: format!("When the group reaches {focus} while facing {pressure}"),
            text: format!(
                "{name} says, “Let’s mark what we know, then choose the next question together.”"
            ),
        },
    }
}

fn assistance_input_value(
    profile: &CharacterProfile,
    field: AssistanceInputField,
) -> Result<DomainValue, CharacterError> {
    let value = match field {
        AssistanceInputField::DisplayName => {
            return Ok(DomainValue::String(
                profile.canon.identity.display_name.value.clone(),
            ));
        }
        AssistanceInputField::Aliases => serde_json::to_value(&profile.canon.identity.aliases),
        AssistanceInputField::Personality => serde_json::to_value(&profile.canon.personality),
        AssistanceInputField::InnerLife => serde_json::to_value(&profile.canon.inner_life),
        AssistanceInputField::Voice => serde_json::to_value(&profile.canon.voice),
        AssistanceInputField::BirthContext => serde_json::to_value(&profile.canon.birth_date),
        AssistanceInputField::Presentation => serde_json::to_value(
            profile
                .extensions
                .get("org.weave.character.identity_presentation"),
        ),
        AssistanceInputField::Projections => serde_json::to_value(
            profile
                .extensions
                .get("org.weave.character.role_projections"),
        ),
        AssistanceInputField::Relationships => {
            serde_json::to_value(profile.extensions.get("org.weave.character.relationships"))
        }
        AssistanceInputField::DateContext => {
            serde_json::to_value(profile.extensions.get("org.weave.character.date_context"))
        }
        AssistanceInputField::Expression => {
            serde_json::to_value(profile.extensions.get("org.weave.character.expression"))
        }
    }
    .map_err(|_| assistance_encoding_error())?;
    json_to_domain(value)
}

fn json_to_domain(value: serde_json::Value) -> Result<DomainValue, CharacterError> {
    match value {
        serde_json::Value::Null => Ok(DomainValue::Null),
        serde_json::Value::Bool(value) => Ok(DomainValue::Bool(value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(DomainValue::Number)
            .ok_or_else(assistance_encoding_error),
        serde_json::Value::String(value) => Ok(DomainValue::String(value)),
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(json_to_domain)
            .collect::<Result<Vec<_>, _>>()
            .map(DomainValue::List),
        serde_json::Value::Object(values) => values
            .into_iter()
            .map(|(key, value)| Ok((key, json_to_domain(value)?)))
            .collect::<Result<BTreeMap<_, _>, CharacterError>>()
            .map(DomainValue::Object),
    }
}

/// Produce a deterministic independent advisory pass without changing a candidate or profile.
pub fn review_assistance_candidates_offline(
    set: &AssistanceCandidateSet,
    reviewer: impl Into<String>,
) -> Result<AssistanceAdvisoryReview, CharacterError> {
    validate_assistance_candidate_set(set)?;
    let candidate_set_sha256 = assistance_candidate_set_fingerprint(set)?;
    let assessments = set
        .candidates
        .iter()
        .map(|(id, candidate)| {
            let digest = Sha256::digest(
                format!("{candidate_set_sha256}:{id}:offline_advisory_v1").as_bytes(),
            );
            let relevance_micros = 700_000 + u32::from(digest[0]) * 700;
            let consistency_micros = 720_000 + u32::from(digest[1]) * 650;
            let safety_micros = 900_000 + u32::from(digest[2]) * 300;
            let mut issues = Vec::new();
            if matches!(
                candidate.value,
                AssistanceCandidateValue::RelationshipCue {
                    other_character_id: None,
                    ..
                }
            ) {
                issues.push(AssistanceAdvisoryIssue {
                    code: "open_relationship_target".to_owned(),
                    severity: AssistanceAdvisorySeverity::Note,
                    message: "The relationship cue intentionally leaves its other character open for author selection."
                        .to_owned(),
                });
            }
            (
                id.clone(),
                AssistanceAdvisoryAssessment {
                    candidate_id: id.clone(),
                    relevance_micros: relevance_micros.min(1_000_000),
                    consistency_micros: consistency_micros.min(1_000_000),
                    safety_micros: safety_micros.min(1_000_000),
                    issues,
                    proposed_edit: None,
                    explanation: "Offline advisory scores are deterministic review aids derived from the immutable candidate id; they do not rank or select authoring content."
                        .to_owned(),
                },
            )
        })
        .collect();
    let review = AssistanceAdvisoryReview {
        advisory_format_version: ASSISTANCE_ADVISORY_REVIEW_FORMAT_VERSION,
        id: format!("{}.offline_advisory", set.id),
        candidate_set_sha256,
        reviewer: reviewer.into(),
        method: "org.weave.character.assistance.offline_advisory_v1".to_owned(),
        advisory_only: true,
        canonical_write_back: false,
        assessments,
    };
    validate_assistance_advisory_review(set, &review)?;
    Ok(review)
}

/// Build one complete author decision manifest without mutating candidates or the profile.
pub fn create_assistance_decision_review(
    set: &AssistanceCandidateSet,
    author: impl Into<String>,
    rationale: impl Into<String>,
    decisions: BTreeMap<String, AssistanceDecision>,
) -> Result<AssistanceDecisionReview, CharacterError> {
    validate_assistance_candidate_set(set)?;
    let review = AssistanceDecisionReview {
        review_format_version: ASSISTANCE_DECISION_REVIEW_FORMAT_VERSION,
        id: format!("{}.author_review", set.id),
        candidate_set_sha256: assistance_candidate_set_fingerprint(set)?,
        input_profile_sha256: set.input_profile_sha256.clone(),
        author: author.into(),
        rationale: rationale.into(),
        decisions,
    };
    validate_assistance_decision_review(set, &review)?;
    Ok(review)
}

/// Apply complete author decisions only to the non-canonical suggestion queue.
///
/// Reapplying the same review to its exact prior output is idempotent. Any other profile change
/// invalidates the candidate set before a field can be written.
pub fn apply_assistance_review(
    current_profile: &CharacterProfile,
    set: &AssistanceCandidateSet,
    review: &AssistanceDecisionReview,
    advisory_reviews: &[AssistanceAdvisoryReview],
) -> Result<AssistanceReceipt, CharacterError> {
    validate_profile(current_profile)?;
    validate_assistance_candidate_set(set)?;
    validate_assistance_decision_review(set, review)?;
    let mut advisory_hashes = Vec::with_capacity(advisory_reviews.len());
    for advisory in advisory_reviews {
        validate_assistance_advisory_review(set, advisory)?;
        advisory_hashes.push(assistance_advisory_review_fingerprint(advisory)?);
    }
    advisory_hashes.sort();
    advisory_hashes.dedup();
    let receipt = build_assistance_receipt(&set.input_profile, set, review, &advisory_hashes)?;
    let current_sha = assistance_profile_fingerprint(current_profile)?;
    if current_sha != receipt.input_profile_sha256 && current_sha != receipt.output_profile_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "current_profile",
            "assistance decisions are stale for the current profile",
        ));
    }
    Ok(receipt)
}

fn build_assistance_receipt(
    input_profile: &CharacterProfile,
    set: &AssistanceCandidateSet,
    review: &AssistanceDecisionReview,
    advisory_review_sha256s: &[String],
) -> Result<AssistanceReceipt, CharacterError> {
    validate_profile(input_profile)?;
    validate_assistance_candidate_set(set)?;
    validate_assistance_decision_review(set, review)?;
    validate_sorted_hashes("advisory_review_sha256s", advisory_review_sha256s)?;
    if *input_profile != set.input_profile {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "input_profile",
            "assistance application input differs from the candidate set profile",
        ));
    }
    let candidate_set_sha256 = assistance_candidate_set_fingerprint(set)?;
    let decision_review_sha256 = assistance_decision_review_fingerprint(review)?;
    let mut output = input_profile.clone();
    let mut accepted = Vec::<(&AssistanceCandidate, &AssistanceCandidateValue, &str, &str)>::new();
    let mut rejected_candidate_ids = Vec::new();
    let mut deferred_candidate_ids = Vec::new();
    let mut regenerate_candidate_ids = Vec::new();
    let mut regenerate_fields = BTreeSet::new();
    for (id, decision) in &review.decisions {
        let candidate = &set.candidates[id];
        match decision {
            AssistanceDecision::Accept { rationale } => {
                accepted.push((candidate, &candidate.value, "accepted", rationale));
            }
            AssistanceDecision::Edit { value, rationale } => {
                accepted.push((candidate, value, "edited", rationale));
            }
            AssistanceDecision::Reject { .. } => rejected_candidate_ids.push(id.clone()),
            AssistanceDecision::Defer { .. } => deferred_candidate_ids.push(id.clone()),
            AssistanceDecision::Regenerate { .. } => {
                regenerate_candidate_ids.push(id.clone());
                regenerate_fields.insert(candidate.field);
            }
        }
    }
    let mut accepted_suggestion_ids = Vec::new();
    if !accepted.is_empty() {
        output.provenance = merge_provenance(&output.provenance, &set.template.provenance)?;
        output.provenance = merge_provenance(&output.provenance, &set.preview.request.provenance)?;
        let transformation_id = format!(
            "assistance_review_{}",
            &assistance_hash(&(
                candidate_set_sha256.as_str(),
                decision_review_sha256.as_str(),
                advisory_review_sha256s,
            ))?[..20]
        );
        let inputs = provenance_ids(&output.provenance);
        if inputs.contains(&transformation_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance.transformations",
                "assistance review transformation identity already exists",
            ));
        }
        output
            .provenance
            .transformations
            .push(ProvenanceTransformation {
                id: transformation_id.clone(),
                inputs: inputs.iter().cloned().collect(),
                description: "Recorded explicit author decisions over immutable structured assistance candidates; accepted values entered only the non-canonical suggestion queue."
                    .to_owned(),
            });
        output
            .provenance
            .transformations
            .sort_by(|left, right| left.id.cmp(&right.id));
        for (candidate, value, decision, rationale) in accepted {
            let value_sha = assistance_hash(&(candidate.id.as_str(), value, decision))?;
            let suggestion_id = format!("assist_{}_{}", candidate.field.as_str(), &value_sha[..16]);
            let suggestion = CharacterSuggestion {
                id: suggestion_id.clone(),
                target_path: candidate.target_path.clone(),
                proposal: Attributed {
                    value: assistance_suggestion_value(
                        candidate,
                        value,
                        decision,
                        &candidate_set_sha256,
                        &decision_review_sha256,
                        &set.preview.request.provider,
                    ),
                    state: ValueState::Suggested,
                    confidence: Confidence::Moderate,
                    review: ReviewState::Pending,
                    lock: LockState::Unlocked,
                    freshness: Freshness::Current,
                    lineage: vec![transformation_id.clone()],
                    rationale: Some(rationale.to_owned()),
                },
            };
            if let Some(existing) = output.suggestions.get(&suggestion_id)
                && existing != &suggestion
            {
                return Err(error(
                    CharacterDiagnosticCode::ConflictingOverlay,
                    format!("suggestions.{suggestion_id}"),
                    "assistance suggestion identity conflicts with an existing value",
                ));
            }
            output.suggestions.insert(suggestion_id.clone(), suggestion);
            output
                .provenance
                .claims
                .entry(format!("suggestions.{suggestion_id}"))
                .or_default()
                .push(transformation_id.clone());
            accepted_suggestion_ids.push(suggestion_id);
        }
        sort_deduplicate_claims(&mut output.provenance);
    }
    accepted_suggestion_ids.sort();
    rejected_candidate_ids.sort();
    deferred_candidate_ids.sort();
    regenerate_candidate_ids.sort();
    validate_profile(&output)?;
    validate_assistance_profile_delta(input_profile, &output)?;
    let output_profile_sha256 = assistance_profile_fingerprint(&output)?;
    let regeneration_request = if regenerate_fields.is_empty() {
        None
    } else {
        let mut request = set.preview.request.clone();
        request.id = format!(
            "{}.generation{}",
            request.id,
            request.generation.saturating_add(1)
        );
        request.expected_profile_sha256 = output_profile_sha256.clone();
        request.fields = regenerate_fields.into_iter().collect();
        request.generation = request.generation.saturating_add(1);
        request.seed = request.seed.map(|seed| seed.wrapping_add(1));
        validate_assistance_request_structure(&request)?;
        Some(request)
    };
    Ok(AssistanceReceipt {
        receipt_format_version: ASSISTANCE_RECEIPT_FORMAT_VERSION,
        id: format!("{}.receipt", set.id),
        input_profile: input_profile.clone(),
        input_profile_sha256: set.input_profile_sha256.clone(),
        output_profile: output,
        output_profile_sha256,
        candidate_set: set.clone(),
        candidate_set_sha256,
        decision_review: review.clone(),
        decision_review_sha256,
        advisory_review_sha256s: advisory_review_sha256s.to_vec(),
        accepted_suggestion_ids,
        rejected_candidate_ids,
        deferred_candidate_ids,
        regenerate_candidate_ids,
        regeneration_request,
    })
}

fn assistance_suggestion_value(
    candidate: &AssistanceCandidate,
    value: &AssistanceCandidateValue,
    decision: &str,
    candidate_set_sha256: &str,
    review_sha256: &str,
    provider: &AssistanceProviderRef,
) -> DomainValue {
    DomainValue::Object(BTreeMap::from([
        (
            "candidate_id".to_owned(),
            DomainValue::String(candidate.id.clone()),
        ),
        (
            "candidate_set_sha256".to_owned(),
            DomainValue::String(candidate_set_sha256.to_owned()),
        ),
        (
            "decision".to_owned(),
            DomainValue::Symbol(decision.to_owned()),
        ),
        (
            "field".to_owned(),
            DomainValue::Symbol(candidate.field.as_str().to_owned()),
        ),
        (
            "provider_adapter".to_owned(),
            DomainValue::String(provider.adapter_id.clone()),
        ),
        (
            "provider_engine".to_owned(),
            DomainValue::String(provider.engine_id.clone()),
        ),
        (
            "review_sha256".to_owned(),
            DomainValue::String(review_sha256.to_owned()),
        ),
        ("value".to_owned(), candidate_value_domain(value)),
    ]))
}

/// Render every exact batch disclosure without invoking a provider.
pub fn preview_assistance_batch(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    request: &AssistanceBatchRequest,
) -> Result<AssistanceBatchPreview, CharacterError> {
    validate_character_collection(collection)?;
    validate_assistance_template(template)?;
    validate_assistance_batch_request_structure(request)?;
    if request.collection_id != collection.id
        || request.expected_collection_sha256 != collection_fingerprint(collection)?
        || request.template != assistance_template_ref(template)?
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "batch_request",
            "batch request does not match the exact collection or template",
        ));
    }
    validate_batch_template_compatibility(template, request)?;
    for id in &request.filter.character_ids {
        if !collection.characters.contains_key(id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "filter.character_ids",
                "batch filter references a character absent from the collection",
            ));
        }
    }
    let mut target_ids = collection
        .characters
        .iter()
        .filter(|(id, profile)| batch_filter_matches(id, profile, &request.filter))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    target_ids.truncate(request.budget.maximum_characters);
    if target_ids.is_empty() {
        return Err(invalid_value(
            "filter",
            "batch filter selected no character profiles",
        ));
    }
    let known_character_ids = collection.characters.keys().cloned().collect::<Vec<_>>();
    let mut previews = BTreeMap::new();
    let mut total_input_chars = 0usize;
    for id in &target_ids {
        let profile = &collection.characters[id];
        let profile_hash = assistance_profile_fingerprint(profile)?;
        let request_hash =
            assistance_hash(&(request.id.as_str(), id.as_str(), profile_hash.as_str()))?;
        let single = AssistanceRequest {
            request_format_version: ASSISTANCE_REQUEST_FORMAT_VERSION,
            id: format!("{}.target_{}", request.id, &request_hash[..16]),
            profile_id: id.clone(),
            expected_profile_sha256: profile_hash,
            template: request.template.clone(),
            provider: request.provider.clone(),
            fields: request.fields.clone(),
            included_inputs: request.included_inputs.clone(),
            settings: request.settings.clone(),
            seed: request.seed.map(|seed| {
                let digest = Sha256::digest(format!("{seed}:{id}").as_bytes());
                let mut bytes = [0_u8; 8];
                bytes.copy_from_slice(&digest[..8]);
                u64::from_le_bytes(bytes)
            }),
            generation: 0,
            provenance: request.provenance.clone(),
        };
        let preview =
            preview_assistance_profile(profile, known_character_ids.clone(), template, &single)?;
        total_input_chars = total_input_chars
            .checked_add(preview.scope.serialized_character_count)
            .ok_or_else(|| invalid_value("total_input_chars", "batch input size overflowed"))?;
        previews.insert(id.clone(), preview);
    }
    if total_input_chars > request.budget.maximum_input_chars {
        return Err(invalid_value(
            "budget.maximum_input_chars",
            "visible batch input exceeds its pinned disclosure budget",
        ));
    }
    let preview = AssistanceBatchPreview {
        batch_preview_format_version: ASSISTANCE_BATCH_PREVIEW_FORMAT_VERSION,
        request: request.clone(),
        request_sha256: assistance_hash(request)?,
        template_sha256: assistance_template_fingerprint(template)?,
        ordered_target_ids: target_ids,
        previews,
        total_input_chars,
        provider_called: false,
        credential_value_stored: false,
    };
    validate_assistance_batch_preview(&preview)?;
    Ok(preview)
}

pub fn approve_assistance_batch_preview(
    preview: &AssistanceBatchPreview,
    author: impl Into<String>,
    rationale: impl Into<String>,
) -> Result<AssistanceExecutionApproval, CharacterError> {
    validate_assistance_batch_preview(preview)?;
    let approval = AssistanceExecutionApproval {
        approval_format_version: ASSISTANCE_APPROVAL_FORMAT_VERSION,
        preview_sha256: assistance_hash(preview)?,
        parent_batch_preview_sha256: None,
        allow_provider_call: true,
        author: author.into(),
        rationale: rationale.into(),
    };
    validate_assistance_execution_approval(&approval)?;
    Ok(approval)
}

/// Start a serializable job only after its complete batch disclosure has been approved.
pub fn start_assistance_job(
    collection: &CharacterCollection,
    template: &AssistanceTemplate,
    preview: &AssistanceBatchPreview,
    approval: &AssistanceExecutionApproval,
) -> Result<AssistanceJob, CharacterError> {
    let expected = preview_assistance_batch(collection, template, &preview.request)?;
    validate_assistance_execution_approval(approval)?;
    let preview_sha256 = assistance_hash(preview)?;
    if expected != *preview
        || approval.preview_sha256 != preview_sha256
        || approval.parent_batch_preview_sha256.is_some()
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "approval",
            "batch approval does not match the exact current visible scope",
        ));
    }
    let job = AssistanceJob {
        job_format_version: ASSISTANCE_JOB_FORMAT_VERSION,
        id: format!("{}.job", preview.request.id),
        input_collection: collection.clone(),
        input_collection_sha256: preview.request.expected_collection_sha256.clone(),
        template: template.clone(),
        batch_preview: preview.clone(),
        batch_preview_sha256: preview_sha256,
        approval: approval.clone(),
        approval_sha256: assistance_approval_fingerprint(approval)?,
        ordered_target_ids: preview.ordered_target_ids.clone(),
        next_index: 0,
        completed_target_ids: Vec::new(),
        attempts: BTreeMap::new(),
        provider_calls: 0,
        candidates_generated: 0,
        input_chars_consumed: 0,
        candidate_sets: BTreeMap::new(),
        cache: BTreeMap::new(),
        failures: BTreeMap::new(),
        state: AssistanceJobState::Running,
        cancellation_rationale: None,
    };
    validate_assistance_job(&job)?;
    Ok(job)
}

/// Advance a bounded deterministic scheduling window. Provider failures retain only safe codes.
pub fn resume_assistance_job<P: CharacterAssistanceProvider>(
    collection: &CharacterCollection,
    job: &AssistanceJob,
    provider: &mut P,
    maximum_attempts_this_resume: usize,
) -> Result<AssistanceJob, AssistanceError> {
    validate_character_collection(collection)?;
    validate_assistance_job(job)?;
    if collection_fingerprint(collection)? != job.input_collection_sha256
        || *collection != job.input_collection
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "collection",
            "assistance job cannot resume against a changed collection",
        )
        .into());
    }
    if maximum_attempts_this_resume == 0 {
        return Err(invalid_value(
            "maximum_attempts_this_resume",
            "resume must permit at least one bounded attempt",
        )
        .into());
    }
    if matches!(
        job.state,
        AssistanceJobState::Cancelled | AssistanceJobState::ReadyForReview
    ) {
        return Ok(job.clone());
    }
    if provider.descriptor() != job.batch_preview.request.provider {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "provider",
            "resume adapter does not match the exact batch provider coordinate",
        )
        .into());
    }
    match (
        job.batch_preview.request.provider.credential_required,
        provider.credential_status(),
    ) {
        (
            false,
            AssistanceCredentialStatus::NotRequired | AssistanceCredentialStatus::Configured,
        )
        | (true, AssistanceCredentialStatus::Configured) => {}
        _ => {
            return Err(AssistanceError::Provider(AssistanceProviderFailure {
                code: AssistanceProviderFailureCode::Authentication,
                retryable: false,
            }));
        }
    }
    let mut next = job.clone();
    next.state = AssistanceJobState::Running;
    let request = next.batch_preview.request.clone();
    let rate_limit = request
        .rate_limit_calls_per_resume
        .min(maximum_attempts_this_resume);
    let mut calls_this_resume = 0usize;
    let mut steps = 0usize;
    while next.next_index < next.ordered_target_ids.len() && steps < maximum_attempts_this_resume {
        let id = next.ordered_target_ids[next.next_index].clone();
        let preview = next.batch_preview.previews[&id].clone();
        let payload_sha256 = assistance_hash(&preview.payload)?;
        if request.cache_policy == AssistanceCachePolicy::JobLocal
            && let Some(cached) = next.cache.get(&payload_sha256).cloned()
        {
            next.candidates_generated = next
                .candidates_generated
                .checked_add(cached.candidates.len())
                .ok_or_else(|| invalid_value("candidates_generated", "counter overflowed"))?;
            next.candidate_sets.insert(id.clone(), cached);
            complete_assistance_job_target(&mut next, &id)?;
            steps += 1;
            continue;
        }
        let prospective_candidates = preview.request.fields.len()
            * usize::from(preview.request.settings.candidates_per_field);
        if next.provider_calls >= request.budget.maximum_provider_calls
            || next
                .candidates_generated
                .saturating_add(prospective_candidates)
                > request.budget.maximum_candidates
            || next
                .input_chars_consumed
                .saturating_add(preview.scope.serialized_character_count)
                > request.budget.maximum_input_chars
        {
            next.state = AssistanceJobState::BudgetExhausted;
            break;
        }
        if calls_this_resume >= rate_limit {
            next.state = AssistanceJobState::RateLimited;
            break;
        }
        let child_approval = AssistanceExecutionApproval {
            approval_format_version: ASSISTANCE_APPROVAL_FORMAT_VERSION,
            preview_sha256: assistance_preview_fingerprint(&preview)?,
            parent_batch_preview_sha256: Some(next.batch_preview_sha256.clone()),
            allow_provider_call: true,
            author: next.approval.author.clone(),
            rationale: next.approval.rationale.clone(),
        };
        validate_assistance_execution_approval(&child_approval)?;
        next.provider_calls = next
            .provider_calls
            .checked_add(1)
            .ok_or_else(|| invalid_value("provider_calls", "counter overflowed"))?;
        calls_this_resume += 1;
        steps += 1;
        next.input_chars_consumed = next
            .input_chars_consumed
            .checked_add(preview.scope.serialized_character_count)
            .ok_or_else(|| invalid_value("input_chars_consumed", "counter overflowed"))?;
        let attempts = next.attempts.entry(id.clone()).or_default();
        *attempts = attempts
            .checked_add(1)
            .ok_or_else(|| invalid_value("attempts", "attempt counter overflowed"))?;
        match execute_approved_preview(
            collection,
            &next.template,
            &preview,
            &child_approval,
            provider,
        ) {
            Ok(set) => {
                next.candidates_generated = next
                    .candidates_generated
                    .checked_add(set.candidates.len())
                    .ok_or_else(|| invalid_value("candidates_generated", "counter overflowed"))?;
                if request.cache_policy == AssistanceCachePolicy::JobLocal {
                    next.cache.insert(payload_sha256, set.clone());
                }
                next.candidate_sets.insert(id.clone(), set);
                complete_assistance_job_target(&mut next, &id)?;
            }
            Err(AssistanceError::Provider(failure)) => {
                let configured_retry =
                    failure.retryable && request.retry.retryable_codes.contains(&failure.code);
                let retryable =
                    configured_retry && *attempts <= request.retry.maximum_retries_per_character;
                if !retryable {
                    next.failures.insert(
                        id.clone(),
                        AssistanceJobFailure {
                            character_id: id.clone(),
                            code: failure.code,
                            attempts: *attempts,
                            retry_exhausted: configured_retry,
                        },
                    );
                    complete_assistance_job_target(&mut next, &id)?;
                }
            }
            Err(error @ AssistanceError::Contract(_)) => return Err(error),
        }
    }
    if next.next_index == next.ordered_target_ids.len() {
        next.state = AssistanceJobState::ReadyForReview;
    } else if next.state == AssistanceJobState::Running && calls_this_resume >= rate_limit {
        next.state = AssistanceJobState::RateLimited;
    }
    validate_assistance_job(&next)?;
    Ok(next)
}

fn complete_assistance_job_target(job: &mut AssistanceJob, id: &str) -> Result<(), CharacterError> {
    if job
        .ordered_target_ids
        .get(job.next_index)
        .map(String::as_str)
        != Some(id)
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "job.next_index",
            "job target completion does not match the deterministic cursor",
        ));
    }
    job.next_index += 1;
    job.completed_target_ids = job.ordered_target_ids[..job.next_index].to_vec();
    Ok(())
}

pub fn cancel_assistance_job(
    job: &AssistanceJob,
    rationale: impl Into<String>,
) -> Result<AssistanceJob, CharacterError> {
    validate_assistance_job(job)?;
    if job.state == AssistanceJobState::ReadyForReview {
        return Err(invalid_value(
            "state",
            "a ready-for-review assistance job cannot be cancelled retroactively",
        ));
    }
    let mut cancelled = job.clone();
    cancelled.state = AssistanceJobState::Cancelled;
    cancelled.cancellation_rationale = Some(rationale.into());
    validate_assistance_job(&cancelled)?;
    Ok(cancelled)
}

/// Apply every successful batch candidate set atomically. Replaying against the exact prior output
/// returns the same receipt and never duplicates a suggestion.
pub fn apply_assistance_batch_reviews(
    current_collection: &CharacterCollection,
    job: &AssistanceJob,
    reviews: &BTreeMap<String, AssistanceDecisionReview>,
    advisory_reviews: &BTreeMap<String, Vec<AssistanceAdvisoryReview>>,
) -> Result<AssistanceBatchReceipt, CharacterError> {
    validate_character_collection(current_collection)?;
    validate_assistance_job(job)?;
    if job.state != AssistanceJobState::ReadyForReview {
        return Err(invalid_value(
            "job.state",
            "batch decisions require a ready-for-review assistance job",
        ));
    }
    if reviews.len() != job.candidate_sets.len()
        || reviews
            .keys()
            .any(|id| !job.candidate_sets.contains_key(id))
        || advisory_reviews
            .keys()
            .any(|id| !job.candidate_sets.contains_key(id))
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            "reviews",
            "batch author and advisory reviews must reference successful exact candidate sets",
        ));
    }
    let mut output = job.input_collection.clone();
    let mut receipts = BTreeMap::new();
    for id in &job.ordered_target_ids {
        let Some(set) = job.candidate_sets.get(id) else {
            continue;
        };
        let review = &reviews[id];
        let advisories = advisory_reviews.get(id).map(Vec::as_slice).unwrap_or(&[]);
        let receipt = apply_assistance_review(&set.input_profile, set, review, advisories)?;
        output
            .characters
            .insert(id.clone(), receipt.output_profile.clone());
        receipts.insert(id.clone(), receipt);
    }
    output.revision = output
        .revision
        .checked_add(u64::from(
            output.characters != job.input_collection.characters,
        ))
        .ok_or_else(|| invalid_value("output_collection.revision", "revision overflowed"))?;
    validate_character_collection(&output)?;
    let input_sha256 = collection_fingerprint(&job.input_collection)?;
    let output_sha256 = collection_fingerprint(&output)?;
    let current_sha256 = collection_fingerprint(current_collection)?;
    if current_sha256 != input_sha256 && current_sha256 != output_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "current_collection",
            "batch assistance decisions are stale for the current collection",
        ));
    }
    let receipt = AssistanceBatchReceipt {
        batch_receipt_format_version: ASSISTANCE_BATCH_RECEIPT_FORMAT_VERSION,
        id: format!("{}.receipt", job.id),
        input_collection: job.input_collection.clone(),
        input_collection_sha256: input_sha256,
        output_collection: output,
        output_collection_sha256: output_sha256,
        job_sha256: assistance_job_fingerprint(job)?,
        receipts,
    };
    validate_assistance_batch_receipt(&receipt)?;
    Ok(receipt)
}

/// Compare exact provider outputs in a stable coordinate/id order without choosing a winner.
pub fn compare_assistance_providers(
    sets: &[AssistanceCandidateSet],
    advisory_reviews: &[AssistanceAdvisoryReview],
) -> Result<AssistanceComparisonReport, CharacterError> {
    if sets.len() < 2 || sets.len() > 64 {
        return Err(invalid_value(
            "candidate_sets",
            "provider comparison requires two through 64 candidate sets",
        ));
    }
    for set in sets {
        validate_assistance_candidate_set(set)?;
    }
    let input_profile_sha256 = sets[0].input_profile_sha256.clone();
    if sets
        .iter()
        .any(|set| set.input_profile_sha256 != input_profile_sha256)
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "candidate_sets",
            "provider comparison requires one exact shared input profile",
        ));
    }
    let mut advisory_by_set = BTreeMap::new();
    for review in advisory_reviews {
        let set = sets
            .iter()
            .find(|set| {
                assistance_candidate_set_fingerprint(set).ok().as_deref()
                    == Some(review.candidate_set_sha256.as_str())
            })
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    "advisory_reviews",
                    "comparison advisory review references no supplied candidate set",
                )
            })?;
        validate_assistance_advisory_review(set, review)?;
        if advisory_by_set
            .insert(review.candidate_set_sha256.clone(), review)
            .is_some()
        {
            return Err(invalid_value(
                "advisory_reviews",
                "comparison accepts at most one advisory review per candidate set",
            ));
        }
    }
    let mut providers = sets
        .iter()
        .map(|set| set.preview.request.provider.clone())
        .collect::<Vec<_>>();
    providers.sort();
    providers.dedup();
    if providers.len() != sets.len() {
        return Err(invalid_value(
            "candidate_sets",
            "provider comparison requires one candidate set per distinct provider coordinate",
        ));
    }
    let mut entries = Vec::new();
    for set in sets {
        let set_sha = assistance_candidate_set_fingerprint(set)?;
        let advisory = advisory_by_set.get(&set_sha).copied();
        for (id, candidate) in &set.candidates {
            let assessment = advisory.and_then(|review| review.assessments.get(id));
            entries.push(AssistanceComparisonEntry {
                provider: set.preview.request.provider.clone(),
                candidate_id: id.clone(),
                field: candidate.field,
                relevance_micros: assessment.map(|value| value.relevance_micros),
                consistency_micros: assessment.map(|value| value.consistency_micros),
                safety_micros: assessment.map(|value| value.safety_micros),
                issues: assessment
                    .map(|value| value.issues.clone())
                    .unwrap_or_default(),
            });
        }
    }
    entries.sort_by(|left, right| {
        (&left.provider, left.candidate_id.as_str())
            .cmp(&(&right.provider, right.candidate_id.as_str()))
    });
    let report = AssistanceComparisonReport {
        comparison_format_version: ASSISTANCE_COMPARISON_FORMAT_VERSION,
        id: format!(
            "org.weave.character.assistance.comparison_{}",
            &assistance_hash(&(
                input_profile_sha256.as_str(),
                providers.as_slice(),
                entries.as_slice()
            ))?[..16]
        ),
        input_profile_sha256,
        providers,
        entries,
        advisory_only: true,
        canonical_write_back: false,
        selected_candidate_id: None,
        order_basis: "provider_coordinate_then_candidate_id".to_owned(),
    };
    validate_assistance_comparison(&report)?;
    Ok(report)
}

fn require_version(actual: u32, expected: u32, path: &str) -> Result<(), CharacterError> {
    if actual != expected {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            path,
            "unsupported Character assistance document version",
        ));
    }
    Ok(())
}

fn validate_public_text(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
    reject_placeholders: bool,
) -> Result<(), CharacterError> {
    validate_text(path, value, minimum, maximum)?;
    if value.chars().any(char::is_control) {
        return Err(invalid_value(
            path,
            "text contains a control character outside the assistance contract",
        ));
    }
    if contains_secret_shape(value) {
        return Err(invalid_value(
            path,
            "credential-shaped value was rejected as [REDACTED]",
        ));
    }
    if reject_placeholders && contains_placeholder(value) {
        return Err(invalid_value(
            path,
            "assistance candidate text contains an unresolved placeholder",
        ));
    }
    Ok(())
}

fn contains_secret_shape(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "authorization: bearer ",
        "api_key=",
        "api-key=",
        "apikey=",
        "client_secret=",
        "client-secret=",
        "database_url=",
        "password=",
        "private_key=",
        "access_token=",
        "refresh_token=",
        "github_pat_",
        "ghp_",
        "xoxb-",
        "akia",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || lower
            .split(|character: char| character.is_whitespace() || character == '"')
            .any(|token| token.starts_with("sk-") && token.len() >= 20)
}

fn contains_placeholder(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "todo" | "tbd" | "placeholder" | "unknown" | "n/a" | "fill me in"
    ) || lower.contains("lorem ipsum")
        || lower.contains("<placeholder>")
        || lower.contains("[insert ")
        || lower.contains("{{")
        || lower.contains("}}")
        || lower.contains(REDACTED.to_ascii_lowercase().as_str())
}

fn validate_https(path: &str, value: &str) -> Result<(), CharacterError> {
    validate_public_text(path, value, 8, 2_048, false)?;
    if !value.starts_with("https://") || value.contains('@') || value.contains(char::is_whitespace)
    {
        return Err(invalid_value(
            path,
            "expected a credential-free public HTTPS URL",
        ));
    }
    Ok(())
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn validate_profile_path(path: &str, value: &str) -> Result<(), CharacterError> {
    if value.is_empty()
        || value.len() > 512
        || value.split('.').any(|segment| {
            let mut characters = segment.chars();
            !characters
                .next()
                .is_some_and(|character| character.is_ascii_lowercase())
                || !characters.all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
                })
                || segment.ends_with('_')
                || segment.contains("__")
        })
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a dot-separated Character profile path",
        ));
    }
    Ok(())
}

fn validate_template_ref(
    path: &str,
    template: &AssistanceTemplateRef,
) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &template.id)?;
    validate_semver(&format!("{path}.version"), &template.version)?;
    validate_sha256(&format!("{path}.sha256"), &template.sha256)
}

fn validate_provider_ref(
    path: &str,
    provider: &AssistanceProviderRef,
) -> Result<(), CharacterError> {
    validate_namespaced_id(&format!("{path}.adapter_id"), &provider.adapter_id)?;
    validate_semver(
        &format!("{path}.adapter_version"),
        &provider.adapter_version,
    )?;
    validate_public_text(
        &format!("{path}.engine_id"),
        &provider.engine_id,
        1,
        256,
        false,
    )?;
    if !provider.engine_id.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
    }) {
        return Err(error(
            CharacterDiagnosticCode::InvalidIdentifier,
            format!("{path}.engine_id"),
            "provider engine coordinate contains unsupported characters",
        ));
    }
    if provider.mode == AssistanceProviderMode::Offline
        && (provider.credential_required || !provider.supports_seed)
    {
        return Err(invalid_value(
            path,
            "offline providers must be credential-free and support deterministic seeds",
        ));
    }
    Ok(())
}

fn validate_requested_fields(
    path: &str,
    fields: &[AssistanceFieldKind],
) -> Result<(), CharacterError> {
    if fields.is_empty() || fields.len() > MAX_FIELDS || !strictly_sorted_unique(fields) {
        return Err(invalid_value(
            path,
            "requested assistance fields must be non-empty, sorted, unique, and bounded",
        ));
    }
    Ok(())
}

fn validate_included_inputs(
    path: &str,
    inputs: &[AssistanceInputField],
) -> Result<(), CharacterError> {
    if inputs.is_empty()
        || inputs.len() > AssistanceInputField::all().len()
        || !strictly_sorted_unique(inputs)
    {
        return Err(invalid_value(
            path,
            "included assistance inputs must be non-empty, sorted, unique, and bounded",
        ));
    }
    Ok(())
}

fn validate_assistance_settings(settings: &AssistanceSettings) -> Result<(), CharacterError> {
    if settings.candidates_per_field == 0
        || usize::from(settings.candidates_per_field) > MAX_CANDIDATES
        || settings.creativity_micros > 1_000_000
        || !(1..=1_048_576).contains(&settings.maximum_output_chars)
        || settings.provider_parameters.len() > 128
    {
        return Err(invalid_value(
            "settings",
            "assistance settings exceed their deterministic contract bounds",
        ));
    }
    validate_public_text("settings.language", &settings.language, 2, 35, false)?;
    if !settings
        .language
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(invalid_value(
            "settings.language",
            "language must be an ASCII language tag",
        ));
    }
    for (key, value) in &settings.provider_parameters {
        validate_local_id("settings.provider_parameters.key", key)?;
        let lower = key.to_ascii_lowercase();
        if [
            "secret",
            "credential",
            "password",
            "api_key",
            "token",
            "authorization",
            "private_key",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        {
            return Err(error(
                CharacterDiagnosticCode::ForbiddenWriteBack,
                "settings.provider_parameters",
                "credential-bearing provider parameters cannot be serialized",
            ));
        }
        validate_disclosed_value("settings.provider_parameters", value)?;
    }
    Ok(())
}

fn assistance_template_ref(
    template: &AssistanceTemplate,
) -> Result<AssistanceTemplateRef, CharacterError> {
    Ok(AssistanceTemplateRef {
        id: template.id.clone(),
        version: template.version.clone(),
        sha256: assistance_template_fingerprint(template)?,
    })
}

fn validate_scope_preview(
    scope: &AssistanceScopePreview,
    request: &AssistanceRequest,
) -> Result<(), CharacterError> {
    validate_namespaced_id("scope.profile_id", &scope.profile_id)?;
    validate_sha256("scope.input_profile_sha256", &scope.input_profile_sha256)?;
    validate_sorted_namespaced_ids("scope.known_character_ids", &scope.known_character_ids)?;
    if scope.profile_id != request.profile_id
        || scope.input_profile_sha256 != request.expected_profile_sha256
        || !scope.known_character_ids.contains(&scope.profile_id)
        || !scope
            .included_values
            .keys()
            .copied()
            .eq(request.included_inputs.iter().copied())
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "scope",
            "assistance scope does not match the exact request inputs and profile",
        ));
    }
    for (field, value) in &scope.included_values {
        validate_disclosed_value(field.profile_path(), value)?;
    }
    let serialized_character_count = to_pretty_json(&scope.included_values)
        .map_err(|_| assistance_encoding_error())?
        .chars()
        .count();
    if scope.serialized_character_count != serialized_character_count {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "scope.serialized_character_count",
            "scope character count does not match the exact serialized disclosure",
        ));
    }
    Ok(())
}

fn validate_provider_payload(
    payload: &AssistanceProviderPayload,
    request: &AssistanceRequest,
    scope: &AssistanceScopePreview,
) -> Result<(), CharacterError> {
    require_version(payload.payload_format_version, 1, "payload_format_version")?;
    validate_namespaced_id("payload.request_id", &payload.request_id)?;
    validate_namespaced_id("payload.profile_id", &payload.profile_id)?;
    validate_requested_fields("payload.fields", &payload.fields)?;
    validate_assistance_settings(&payload.settings)?;
    if payload.request_id != request.id
        || payload.profile_id != request.profile_id
        || payload.fields != request.fields
        || payload.included_values != scope.included_values
        || payload.known_character_ids != scope.known_character_ids
        || payload.settings != request.settings
        || payload.seed != request.seed
        || payload.generation != request.generation
        || payload.expected_response_schema != PROVIDER_RESPONSE_SCHEMA_ID
        || !payload
            .instructions
            .keys()
            .copied()
            .eq(request.fields.iter().copied())
        || !payload
            .required_evidence
            .keys()
            .copied()
            .eq(request.fields.iter().copied())
    {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "payload",
            "provider payload does not reproduce from the exact visible request scope",
        ));
    }
    for (field, instructions) in &payload.instructions {
        validate_public_text(
            &format!("payload.instructions.{}", field.as_str()),
            instructions,
            1,
            4_096,
            false,
        )?;
    }
    for (field, paths) in &payload.required_evidence {
        validate_sorted_paths(
            &format!("payload.required_evidence.{}", field.as_str()),
            paths,
        )?;
        if paths.iter().any(|path| {
            !request
                .included_inputs
                .iter()
                .any(|input| input.profile_path() == path)
        }) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "payload.required_evidence",
                "provider evidence path was not included in the visible disclosure",
            ));
        }
    }
    for value in payload.included_values.values() {
        validate_disclosed_value("payload.included_values", value)?;
    }
    Ok(())
}

fn validate_offline_provider_payload(
    payload: &AssistanceProviderPayload,
) -> Result<(), CharacterError> {
    require_version(payload.payload_format_version, 1, "payload_format_version")?;
    validate_namespaced_id("payload.request_id", &payload.request_id)?;
    validate_namespaced_id("payload.profile_id", &payload.profile_id)?;
    validate_requested_fields("payload.fields", &payload.fields)?;
    validate_assistance_settings(&payload.settings)?;
    validate_sorted_namespaced_ids("payload.known_character_ids", &payload.known_character_ids)?;
    if payload.settings.candidates_per_field > OFFLINE_VARIANTS
        || payload.seed.is_none()
        || payload.generation > 65_535
        || payload.expected_response_schema != PROVIDER_RESPONSE_SCHEMA_ID
        || !payload.known_character_ids.contains(&payload.profile_id)
        || payload.included_values.is_empty()
        || payload.included_values.len() > AssistanceInputField::all().len()
        || !payload
            .instructions
            .keys()
            .copied()
            .eq(payload.fields.iter().copied())
        || !payload
            .required_evidence
            .keys()
            .copied()
            .eq(payload.fields.iter().copied())
    {
        return Err(invalid_value(
            "payload",
            "offline assistance payload is incomplete or outside deterministic bounds",
        ));
    }
    for (field, instructions) in &payload.instructions {
        validate_public_text(
            &format!("payload.instructions.{}", field.as_str()),
            instructions,
            1,
            4_096,
            false,
        )?;
    }
    for (field, paths) in &payload.required_evidence {
        validate_sorted_paths(
            &format!("payload.required_evidence.{}", field.as_str()),
            paths,
        )?;
        if paths.iter().any(|path| {
            !payload
                .included_values
                .keys()
                .any(|input| input.profile_path() == path)
        }) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                "payload.required_evidence",
                "offline provider evidence path is absent from the visible disclosure",
            ));
        }
    }
    for value in payload.included_values.values() {
        validate_disclosed_value("payload.included_values", value)?;
    }
    Ok(())
}

fn validate_candidate_value(
    path: &str,
    value: &AssistanceCandidateValue,
    specification: &AssistanceTemplateField,
    known_character_ids: &[String],
) -> Result<(), CharacterError> {
    if value.field() != specification.kind {
        return Err(error(
            CharacterDiagnosticCode::InvalidReference,
            path,
            "typed assistance candidate does not match its template field",
        ));
    }
    let total = candidate_text_chars(value);
    if !(specification.minimum_text_chars..=specification.maximum_text_chars).contains(&total) {
        return Err(invalid_value(
            path,
            "candidate text is outside the selected template field bounds",
        ));
    }
    for text in candidate_texts(value) {
        validate_public_text(path, text, 1, specification.maximum_text_chars, true)?;
    }
    if let AssistanceCandidateValue::RelationshipCue {
        other_character_id: Some(other_character_id),
        ..
    } = value
    {
        validate_namespaced_id(path, other_character_id)?;
        if !known_character_ids.contains(other_character_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "relationship cue references a character outside the approved scope vocabulary",
            ));
        }
    }
    Ok(())
}

fn candidate_texts(value: &AssistanceCandidateValue) -> Vec<&str> {
    match value {
        AssistanceCandidateValue::Biography {
            summary,
            formative_thread,
        } => vec![summary, formative_thread],
        AssistanceCandidateValue::Motivation { objective, reason } => vec![objective, reason],
        AssistanceCandidateValue::Fear { concern, trigger } => vec![concern, trigger],
        AssistanceCandidateValue::GuardedTruth {
            truth,
            guarded_because,
        } => vec![truth, guarded_because],
        AssistanceCandidateValue::Tension {
            premise,
            opposing_pressure,
        } => vec![premise, opposing_pressure],
        AssistanceCandidateValue::NarrativeHook { hook, stakes } => vec![hook, stakes],
        AssistanceCandidateValue::PresentationCue { cue, rationale }
        | AssistanceCandidateValue::RoleIdea {
            label: cue,
            rationale,
        }
        | AssistanceCandidateValue::RelationshipCue { cue, rationale, .. } => {
            vec![cue, rationale]
        }
        AssistanceCandidateValue::ContextReaction { context, reaction } => {
            vec![context, reaction]
        }
        AssistanceCandidateValue::ExpressionExample { context, text } => vec![context, text],
    }
}

fn candidate_text_chars(value: &AssistanceCandidateValue) -> usize {
    candidate_texts(value)
        .into_iter()
        .map(str::chars)
        .map(Iterator::count)
        .sum()
}

fn all_validation_checks() -> Vec<AssistanceValidationCheck> {
    vec![
        AssistanceValidationCheck::Schema,
        AssistanceValidationCheck::Length,
        AssistanceValidationCheck::CredentialShape,
        AssistanceValidationCheck::Placeholder,
        AssistanceValidationCheck::ControlCharacters,
        AssistanceValidationCheck::Reference,
    ]
}

fn validate_sorted_paths(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if values.is_empty() || !strictly_sorted_unique(values) {
        return Err(invalid_value(
            path,
            "assistance paths must be non-empty, sorted, and unique",
        ));
    }
    for value in values {
        if !AssistanceInputField::all()
            .iter()
            .any(|field| field.profile_path() == value)
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                path,
                "path is outside the closed assistance input vocabulary",
            ));
        }
    }
    Ok(())
}

fn validate_sorted_hashes(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if !strictly_sorted_unique(values) {
        return Err(invalid_value(
            path,
            "fingerprints must be sorted and unique",
        ));
    }
    for value in values {
        validate_sha256(path, value)?;
    }
    Ok(())
}

fn validate_sorted_local_ids(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if !strictly_sorted_unique(values) {
        return Err(invalid_value(path, "local ids must be sorted and unique"));
    }
    for value in values {
        validate_local_id(path, value)?;
    }
    Ok(())
}

fn validate_sorted_namespaced_ids(path: &str, values: &[String]) -> Result<(), CharacterError> {
    if !strictly_sorted_unique(values) {
        return Err(invalid_value(
            path,
            "namespaced ids must be sorted and unique",
        ));
    }
    for value in values {
        validate_namespaced_id(path, value)?;
    }
    Ok(())
}

fn validate_batch_filter(filter: &AssistanceBatchFilter) -> Result<(), CharacterError> {
    if filter.character_ids.len() > MAX_BATCH_CHARACTERS
        || filter.id_prefixes.len() > 1_024
        || filter.fields_missing_suggestions.len() > MAX_FIELDS
        || !strictly_sorted_unique(&filter.character_ids)
        || !strictly_sorted_unique(&filter.id_prefixes)
        || !strictly_sorted_unique(&filter.fields_missing_suggestions)
    {
        return Err(invalid_value(
            "filter",
            "batch filter coordinates must be sorted, unique, and bounded",
        ));
    }
    for id in &filter.character_ids {
        validate_namespaced_id("filter.character_ids", id)?;
    }
    for prefix in &filter.id_prefixes {
        validate_namespaced_id("filter.id_prefixes", prefix)?;
    }
    Ok(())
}

fn validate_batch_template_compatibility(
    template: &AssistanceTemplate,
    request: &AssistanceBatchRequest,
) -> Result<(), CharacterError> {
    for field in &request.fields {
        let specification = template.fields.get(field).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                "fields",
                "batch request selects a field absent from the exact template",
            )
        })?;
        if request.settings.candidates_per_field > specification.maximum_candidates
            || specification
                .required_inputs
                .iter()
                .any(|input| !request.included_inputs.contains(input))
        {
            return Err(invalid_value(
                "fields",
                "batch candidate count or disclosed inputs violate the selected template",
            ));
        }
    }
    Ok(())
}

fn batch_filter_matches(
    id: &str,
    profile: &CharacterProfile,
    filter: &AssistanceBatchFilter,
) -> bool {
    (filter.character_ids.is_empty()
        || filter.character_ids.iter().any(|candidate| candidate == id))
        && (filter.id_prefixes.is_empty()
            || filter
                .id_prefixes
                .iter()
                .any(|prefix| id.starts_with(prefix)))
        && (filter.fields_missing_suggestions.is_empty()
            || filter
                .fields_missing_suggestions
                .iter()
                .any(|field| !profile_has_assistance_field(profile, *field)))
}

fn profile_has_assistance_field(profile: &CharacterProfile, field: AssistanceFieldKind) -> bool {
    profile.suggestions.values().any(|suggestion| {
        let DomainValue::Object(value) = &suggestion.proposal.value else {
            return false;
        };
        value.get("field") == Some(&DomainValue::Symbol(field.as_str().to_owned()))
    })
}

fn candidate_value_domain(value: &AssistanceCandidateValue) -> DomainValue {
    let pairs = match value {
        AssistanceCandidateValue::Biography {
            summary,
            formative_thread,
        } => vec![("summary", summary), ("formative_thread", formative_thread)],
        AssistanceCandidateValue::Motivation { objective, reason } => {
            vec![("objective", objective), ("reason", reason)]
        }
        AssistanceCandidateValue::Fear { concern, trigger } => {
            vec![("concern", concern), ("trigger", trigger)]
        }
        AssistanceCandidateValue::GuardedTruth {
            truth,
            guarded_because,
        } => vec![("truth", truth), ("guarded_because", guarded_because)],
        AssistanceCandidateValue::Tension {
            premise,
            opposing_pressure,
        } => vec![
            ("premise", premise),
            ("opposing_pressure", opposing_pressure),
        ],
        AssistanceCandidateValue::NarrativeHook { hook, stakes } => {
            vec![("hook", hook), ("stakes", stakes)]
        }
        AssistanceCandidateValue::PresentationCue { cue, rationale } => {
            vec![("cue", cue), ("rationale", rationale)]
        }
        AssistanceCandidateValue::RoleIdea { label, rationale } => {
            vec![("label", label), ("rationale", rationale)]
        }
        AssistanceCandidateValue::RelationshipCue { cue, rationale, .. } => {
            vec![("cue", cue), ("rationale", rationale)]
        }
        AssistanceCandidateValue::ContextReaction { context, reaction } => {
            vec![("context", context), ("reaction", reaction)]
        }
        AssistanceCandidateValue::ExpressionExample { context, text } => {
            vec![("context", context), ("text", text)]
        }
    };
    let mut object = pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), DomainValue::String(value.clone())))
        .collect::<BTreeMap<_, _>>();
    if let AssistanceCandidateValue::RelationshipCue {
        other_character_id, ..
    } = value
    {
        object.insert(
            "other_character_id".to_owned(),
            other_character_id
                .as_ref()
                .map_or(DomainValue::Null, |id| DomainValue::String(id.clone())),
        );
    }
    DomainValue::Object(object)
}

fn validate_assistance_profile_delta(
    input: &CharacterProfile,
    output: &CharacterProfile,
) -> Result<(), CharacterError> {
    validate_profile(input)?;
    validate_profile(output)?;
    if input.profile_format_version != output.profile_format_version
        || input.id != output.id
        || input.canon != output.canon
        || input.extensions != output.extensions
        || input.derived != output.derived
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "output_profile",
            "assistance may write only non-canonical suggestions and their provenance",
        ));
    }
    if input
        .suggestions
        .iter()
        .any(|(id, suggestion)| output.suggestions.get(id) != Some(suggestion))
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "output_profile.suggestions",
            "assistance cannot replace or remove an existing suggestion",
        ));
    }
    for (id, suggestion) in output
        .suggestions
        .iter()
        .filter(|(id, _)| !input.suggestions.contains_key(*id))
    {
        if !id.starts_with("assist_")
            || suggestion.proposal.state != ValueState::Suggested
            || suggestion.proposal.review != ReviewState::Pending
        {
            return Err(error(
                CharacterDiagnosticCode::ForbiddenWriteBack,
                format!("output_profile.suggestions.{id}"),
                "new assistance values must remain pending non-canonical suggestions",
            ));
        }
    }
    if input.suggestions == output.suggestions && input.provenance != output.provenance {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "output_profile.provenance",
            "assistance provenance cannot change when no suggestion was added",
        ));
    }
    if input
        .provenance
        .sources
        .iter()
        .any(|source| !output.provenance.sources.contains(source))
        || input
            .provenance
            .transformations
            .iter()
            .any(|transformation| !output.provenance.transformations.contains(transformation))
        || input.provenance.claims.iter().any(|(path, ids)| {
            output
                .provenance
                .claims
                .get(path)
                .is_none_or(|output_ids| ids.iter().any(|id| !output_ids.contains(id)))
        })
    {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "output_profile.provenance",
            "assistance cannot remove or replace existing provenance",
        ));
    }
    Ok(())
}

fn provenance_ids(provenance: &Provenance) -> BTreeSet<String> {
    provenance
        .sources
        .iter()
        .map(|source| source.id.clone())
        .collect()
}

fn sort_deduplicate_claims(provenance: &mut Provenance) {
    for ids in provenance.claims.values_mut() {
        ids.sort();
        ids.dedup();
    }
}

fn validate_disclosed_value(path: &str, value: &DomainValue) -> Result<(), CharacterError> {
    let mut node_count = 0usize;
    validate_disclosed_value_at(path, value, 0, &mut node_count)
}

fn validate_disclosed_value_at(
    path: &str,
    value: &DomainValue,
    depth: usize,
    node_count: &mut usize,
) -> Result<(), CharacterError> {
    *node_count = node_count.saturating_add(1);
    if depth > 32 || *node_count > 262_144 {
        return Err(invalid_value(
            path,
            "disclosed assistance value exceeds depth or node bounds",
        ));
    }
    match value {
        DomainValue::Null | DomainValue::Bool(_) => Ok(()),
        DomainValue::Number(number) if number.is_finite() => Ok(()),
        DomainValue::Number(_) => Err(invalid_value(
            path,
            "disclosed assistance number must be finite",
        )),
        DomainValue::String(text) | DomainValue::Symbol(text) => {
            validate_public_text(path, text, 0, 65_536, false)
        }
        DomainValue::List(values) => {
            if values.len() > 65_536 {
                return Err(invalid_value(
                    path,
                    "disclosed assistance list exceeds its bound",
                ));
            }
            for value in values {
                validate_disclosed_value_at(path, value, depth + 1, node_count)?;
            }
            Ok(())
        }
        DomainValue::Object(values) => {
            if values.len() > 65_536 {
                return Err(invalid_value(
                    path,
                    "disclosed assistance object exceeds its bound",
                ));
            }
            for (key, value) in values {
                validate_public_text(path, key, 1, 512, false)?;
                validate_disclosed_value_at(path, value, depth + 1, node_count)?;
            }
            Ok(())
        }
    }
}

fn assistance_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, CharacterError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| assistance_encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-assistance-contract-version".to_owned(),
            serde_json::Value::from(1),
        );
    }
    crate::sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| assistance_encoding_error())
}

fn assistance_hash<T: Serialize + ?Sized>(value: &T) -> Result<String, CharacterError> {
    let bytes = serde_json::to_vec(value).map_err(|_| assistance_encoding_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn assistance_encoding_error() -> CharacterError {
    error(
        CharacterDiagnosticCode::InvalidEncoding,
        "document",
        "Character assistance document does not match the strict serialized contract",
    )
}

fn validate_character_collection(collection: &CharacterCollection) -> Result<(), CharacterError> {
    validate_corpus_collection(collection).map_err(|error_value| {
        let diagnostic = error_value.diagnostic();
        error(
            diagnostic.code,
            diagnostic.path.clone(),
            "Character collection violates its strict contract",
        )
    })
}

fn collection_fingerprint(collection: &CharacterCollection) -> Result<String, CharacterError> {
    corpus_collection_fingerprint(collection).map_err(|error_value| {
        let diagnostic = error_value.diagnostic();
        error(
            diagnostic.code,
            diagnostic.path.clone(),
            "Character collection fingerprinting failed",
        )
    })
}
