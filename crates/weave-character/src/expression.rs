//! Reusable, provider-free character expression and deterministic dialogue templates.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    Provenance, ProvenanceSource, ProvenanceTransformation, parse_strict_json, to_pretty_json,
    to_pretty_ron, validate_provenance,
};

use crate::model::{
    BehavioralSignature, BehavioralSignatures, CharacterExtension, CharacterProfile,
    ExpressionApplicability, ExpressionConstraintEffect, ExpressionContextPredicate,
    ExpressionData, ExpressionMedium, ExpressionPackRef, ExpressionRecordOrigin,
    ExpressionTemplateAssignment, ExpressionTermKind, ExpressionVocabularyPool,
    ExpressionVoiceConstraint, ExtensionHeader, ExtensionWriteBack, Freshness, LockState,
    NormalizedExpressionTerm, NormalizedPreference, PreferencePolarity, ReviewState, TraitBand,
    TraitMeasurement, ValueState, VersionedExtension, expression_format_version,
};
use crate::validate_profile;
use crate::validation::{
    trait_value, validate_local_id, validate_namespaced_id, validate_semver, validate_sha256,
    validate_text,
};

pub const EXPRESSION_PACK_FORMAT_VERSION: u32 = 1;
pub const EXPRESSION_REVISION_FORMAT_VERSION: u32 = 1;
pub const EXPRESSION_ASSIGNMENT_REQUEST_FORMAT_VERSION: u32 = 1;
pub const EXPRESSION_ASSIGNMENT_RECEIPT_FORMAT_VERSION: u32 = 1;
pub const EXPRESSION_RESOLUTION_REQUEST_FORMAT_VERSION: u32 = 1;
pub const EXPRESSION_RESOLUTION_FORMAT_VERSION: u32 = 1;
pub const EXPRESSION_COVERAGE_FORMAT_VERSION: u32 = 1;
pub const EXPRESSION_LINT_FORMAT_VERSION: u32 = 1;

/// Stable namespace for the normalized expression extension.
pub const EXPRESSION_EXTENSION_NAMESPACE: &str = "org.weave.character.expression";
/// Stable namespace for linked behavioral signatures.
pub const BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE: &str =
    "org.weave.character.behavioral_signatures";

/// Immutable, independently distributable expression catalog and dialogue-template pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPack {
    pub pack_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub independently_authored: bool,
    pub license: String,
    pub license_url: String,
    pub eligibility: ExpressionPackEligibility,
    pub entries: BTreeMap<String, ExpressionPackEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vocabulary_pools: BTreeMap<String, ExpressionPackVocabularyPool>,
    pub templates: BTreeMap<String, ExpressionDialogueTemplate>,
    pub coverage: ExpressionPackCoverageRequirements,
    pub provenance: Provenance,
}

/// Closed eligibility gates evaluated without a service or hidden project state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPackEligibility {
    pub compatible_profile_versions: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_extension_namespaces: Vec<String>,
}

/// Minimum review coverage declared by a pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPackCoverageRequirements {
    pub minimum_terms: u32,
    pub minimum_preferences: u32,
    pub minimum_behavioral_signatures: u32,
    pub minimum_voice_constraints: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_scenario_ids: Vec<String>,
}

/// One assignable public-pack record with explicit limitations and source lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPackEntry {
    pub id: String,
    pub label: String,
    pub description: String,
    pub value: ExpressionPackValue,
    pub strength: f64,
    pub applicability: ExpressionApplicability,
    pub source_ids: Vec<String>,
    pub limitations: Vec<String>,
}

/// Values that an expression pack may assign to a character after explicit review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum ExpressionPackValue {
    Term {
        category: String,
        term_kind: ExpressionTermKind,
        surface: String,
    },
    Preference {
        category: String,
        target: String,
        polarity: PreferencePolarity,
    },
    BehavioralSignature {
        category: String,
        cue: String,
    },
    VoiceConstraint {
        category: String,
        medium: ExpressionMedium,
        effect: ExpressionConstraintEffect,
        target: String,
        instruction: String,
    },
}

/// Reusable pack pool whose entries must all be lexicon terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPackVocabularyPool {
    pub id: String,
    pub category: String,
    pub entry_ids: Vec<String>,
    pub applicability: ExpressionApplicability,
    pub source_ids: Vec<String>,
}

/// One reusable dialogue template with a stable fallback and closed placeholder allowlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionDialogueTemplate {
    pub id: String,
    pub scenario_id: String,
    pub speaker_requirement: ExpressionSpeakerRequirement,
    pub placeholders: BTreeMap<String, ExpressionPlaceholderDeclaration>,
    pub variants: BTreeMap<String, ExpressionDialogueVariant>,
    pub fallback_variant_id: String,
    pub source_ids: Vec<String>,
    pub limitations: Vec<String>,
}

/// Which profile may speak a template after it is assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum ExpressionSpeakerRequirement {
    AssignedCharacter,
    ExactCharacter { character_id: String },
}

/// One declared placeholder key and its closed runtime value source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPlaceholderDeclaration {
    pub id: String,
    pub value: ExpressionPlaceholderValue,
    pub required: bool,
}

/// Runtime substitution can only read these public, typed values; no arbitrary variables exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum ExpressionPlaceholderValue {
    SpeakerDisplayName,
    SpeakerPronounSubject,
    ListenerDisplayName,
    RelationshipKind,
    DateLabel,
    WorldPlaceName,
    LexiconTerm { term_id: String },
}

/// One authored or reviewed template variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionDialogueVariant {
    pub id: String,
    pub content: String,
    pub priority: i32,
    pub authority: ExpressionTextAuthority,
    pub review: ReviewState,
    pub applicability: ExpressionApplicability,
    pub source: ExpressionSourceLocation,
    pub source_ids: Vec<String>,
}

/// Runtime precedence: authored and accepted text always outrank suggestions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionTextAuthority {
    Suggested,
    Reviewed,
    Authored,
}

impl ExpressionTextAuthority {
    const fn precedence(self) -> u8 {
        match self {
            Self::Suggested => 0,
            Self::Reviewed => 1,
            Self::Authored => 2,
        }
    }
}

/// Stable public source coordinate used by placeholder diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionSourceLocation {
    pub source_id: String,
    pub line: u32,
    pub column: u32,
}

/// Fingerprinted direct add/edit/remove operation over one character expression layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionRevision {
    pub revision_format_version: u32,
    pub id: String,
    pub character_id: String,
    pub expected_profile_sha256: String,
    pub mutations: Vec<ExpressionMutation>,
    pub rationale: String,
    pub provenance: Provenance,
}

/// Atomic expression mutation. One revision cannot target the same record twice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
pub enum ExpressionMutation {
    UpsertTerm {
        value: NormalizedExpressionTerm,
    },
    UpsertPreference {
        value: NormalizedPreference,
    },
    UpsertVocabularyPool {
        value: ExpressionVocabularyPool,
    },
    UpsertBehavioralSignature {
        value: BehavioralSignature,
    },
    UpsertVoiceConstraint {
        value: ExpressionVoiceConstraint,
    },
    UpsertTemplateAssignment {
        value: ExpressionTemplateAssignment,
    },
    Remove {
        kind: ExpressionRecordKind,
        id: String,
    },
}

/// Stable record families exposed by list, show, revision, and coverage operations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionRecordKind {
    Term,
    Preference,
    VocabularyPool,
    BehavioralSignature,
    VoiceConstraint,
    TemplateAssignment,
}

/// Explicit, exact public-pack assignment request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionAssignmentRequest {
    pub request_format_version: u32,
    pub id: String,
    pub character_id: String,
    pub expected_profile_sha256: String,
    pub pack: ExpressionPackRef,
    pub entry_ids: Vec<String>,
    pub vocabulary_pool_ids: Vec<String>,
    pub template_ids: Vec<String>,
    pub reviewer: String,
    pub rationale: String,
    pub seed: u64,
    pub provenance: Provenance,
}

/// Reproducible proof of one atomic public-pack assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionAssignmentReceipt {
    pub receipt_format_version: u32,
    pub id: String,
    pub input_profile: CharacterProfile,
    pub input_sha256: String,
    pub pack: ExpressionPack,
    pub pack_ref: ExpressionPackRef,
    pub request: ExpressionAssignmentRequest,
    pub request_sha256: String,
    pub output_profile: CharacterProfile,
    pub output_sha256: String,
}

/// Deterministic dialogue resolution request with only typed, public runtime context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionResolutionRequest {
    pub request_format_version: u32,
    pub id: String,
    pub assignment_id: String,
    pub scenario_id: String,
    pub speaker_character_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listener: Option<ExpressionRuntimeParticipant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationship_kind_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub date_context_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_place_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub world_context_tags: Vec<String>,
    pub seed: u64,
}

/// Explicit listener data; no open variable or metadata map is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionRuntimeParticipant {
    pub character_id: String,
    pub display_name: String,
}

/// One ranked variant trace retained in deterministic runtime output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionVariantTrace {
    pub variant_id: String,
    pub authority: ExpressionTextAuthority,
    pub priority: i32,
    pub applicable: bool,
    pub placeholders_available: bool,
    pub seeded_sha256: String,
    pub reason: String,
}

/// Byte-stable, fully inspectable offline dialogue resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionResolution {
    pub resolution_format_version: u32,
    pub id: String,
    pub profile_sha256: String,
    pub pack: ExpressionPackRef,
    pub request: ExpressionResolutionRequest,
    pub request_sha256: String,
    pub template_id: String,
    pub selected_variant_id: String,
    pub fallback_used: bool,
    pub rendered_text: String,
    pub substitutions: BTreeMap<String, String>,
    pub trace: Vec<ExpressionVariantTrace>,
}

/// Deterministic list filter shared by CLI and editor tooling.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionFilter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<ExpressionRecordKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub origins: Vec<ExpressionRecordOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_id: Option<String>,
}

/// One exact record returned by list/show without flattening typed payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    tag = "kind",
    content = "value",
    deny_unknown_fields
)]
pub enum ExpressionRecord {
    Term(NormalizedExpressionTerm),
    Preference(NormalizedPreference),
    VocabularyPool(ExpressionVocabularyPool),
    BehavioralSignature(BehavioralSignature),
    VoiceConstraint(ExpressionVoiceConstraint),
    TemplateAssignment(ExpressionTemplateAssignment),
}

/// Redaction-safe expression validation and lint vocabulary.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum ExpressionDiagnosticCode {
    #[serde(rename = "X100")]
    EmptyText,
    #[serde(rename = "X101")]
    TextLimit,
    #[serde(rename = "X102")]
    UnresolvedToken,
    #[serde(rename = "X103")]
    ConflictingConstraint,
    #[serde(rename = "X104")]
    UnsafePersonalization,
    #[serde(rename = "X105")]
    UnreviewedSuggestion,
    #[serde(rename = "X106")]
    InvalidLink,
    #[serde(rename = "X107")]
    DuplicateVariant,
    #[serde(rename = "X108")]
    NearDuplicateVariant,
    #[serde(rename = "X109")]
    InvalidPlaceholder,
    #[serde(rename = "X110")]
    UnavailablePlaceholder,
    #[serde(rename = "X111")]
    StalePack,
    #[serde(rename = "X112")]
    InvalidStructure,
}

/// Whether a lint blocks validation or remains review guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionDiagnosticSeverity {
    Warning,
    Error,
}

/// One fixed-message diagnostic. Author text is never echoed into errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionDiagnostic {
    pub code: ExpressionDiagnosticCode,
    pub severity: ExpressionDiagnosticSeverity,
    pub path: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ExpressionSourceLocation>,
}

/// Complete lint output; the linter never rewrites author content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionLintReport {
    pub lint_format_version: u32,
    pub profile_sha256: String,
    pub pack_sha256: Vec<String>,
    pub diagnostics: Vec<ExpressionDiagnostic>,
}

/// Scenario and record coverage for one exact profile/pack set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionCoverageReport {
    pub coverage_format_version: u32,
    pub profile_sha256: String,
    pub record_counts: BTreeMap<ExpressionRecordKind, u32>,
    pub category_counts: BTreeMap<String, u32>,
    pub scenario_coverage: BTreeMap<String, ExpressionScenarioCoverage>,
    pub diagnostics: Vec<ExpressionDiagnostic>,
}

/// Coverage for one required or assigned dialogue scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionScenarioCoverage {
    pub scenario_id: String,
    pub assigned_template_ids: Vec<String>,
    pub has_fallback: bool,
    pub reviewed_variant_count: u32,
}

/// Redaction-safe expression workflow failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionError {
    diagnostic: ExpressionDiagnostic,
}

impl ExpressionError {
    #[must_use]
    pub const fn diagnostic(&self) -> &ExpressionDiagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for ExpressionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at `{}`: {}",
            expression_diagnostic_code(self.diagnostic.code),
            self.diagnostic.path,
            self.diagnostic.message
        )
    }
}

impl std::error::Error for ExpressionError {}

macro_rules! impl_expression_document {
    ($type:ty, $validate:ident) => {
        impl $type {
            pub fn from_json(source: &str) -> Result<Self, ExpressionError> {
                let value = parse_strict_json(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn from_ron(source: &str) -> Result<Self, ExpressionError> {
                let value = ron::from_str(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn to_json(&self) -> Result<String, ExpressionError> {
                $validate(self)?;
                to_pretty_json(self).map_err(|_| encoding_error())
            }

            pub fn to_ron(&self) -> Result<String, ExpressionError> {
                $validate(self)?;
                to_pretty_ron(self).map_err(|_| encoding_error())
            }
        }
    };
}

impl_expression_document!(ExpressionPack, validate_expression_pack);
impl_expression_document!(ExpressionRevision, validate_expression_revision_structure);
impl_expression_document!(
    ExpressionAssignmentRequest,
    validate_expression_assignment_request_structure
);
impl_expression_document!(
    ExpressionAssignmentReceipt,
    validate_expression_assignment_receipt
);
impl_expression_document!(
    ExpressionResolutionRequest,
    validate_expression_resolution_request
);
impl_expression_document!(ExpressionResolution, validate_expression_resolution);
impl_expression_document!(ExpressionLintReport, validate_expression_lint_report);
impl_expression_document!(
    ExpressionCoverageReport,
    validate_expression_coverage_report
);

/// Generate the canonical public expression-pack schema.
pub fn expression_pack_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionPack>(
        "urn:weave:schema:character-expression-pack:1",
        "Weave Character Expression Pack v1",
    )
}

/// Generate the canonical direct expression-revision schema.
pub fn expression_revision_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionRevision>(
        "urn:weave:schema:character-expression-revision:1",
        "Weave Character Expression Revision v1",
    )
}

/// Generate the canonical public-pack assignment request schema.
pub fn expression_assignment_request_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionAssignmentRequest>(
        "urn:weave:schema:character-expression-assignment-request:1",
        "Weave Character Expression Assignment Request v1",
    )
}

/// Generate the canonical public-pack assignment receipt schema.
pub fn expression_assignment_receipt_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionAssignmentReceipt>(
        "urn:weave:schema:character-expression-assignment-receipt:1",
        "Weave Character Expression Assignment Receipt v1",
    )
}

/// Generate the canonical deterministic dialogue request schema.
pub fn expression_resolution_request_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionResolutionRequest>(
        "urn:weave:schema:character-expression-resolution-request:1",
        "Weave Character Expression Resolution Request v1",
    )
}

/// Generate the canonical deterministic dialogue output schema.
pub fn expression_resolution_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionResolution>(
        "urn:weave:schema:character-expression-resolution:1",
        "Weave Character Expression Resolution v1",
    )
}

/// Generate the canonical expression-lint report schema.
pub fn expression_lint_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionLintReport>(
        "urn:weave:schema:character-expression-lint:1",
        "Weave Character Expression Lint Report v1",
    )
}

/// Generate the canonical expression-coverage report schema.
pub fn expression_coverage_schema() -> Result<String, ExpressionError> {
    expression_schema::<ExpressionCoverageReport>(
        "urn:weave:schema:character-expression-coverage:1",
        "Weave Character Expression Coverage Report v1",
    )
}

fn expression_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, ExpressionError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-expression-contract-version".to_owned(),
            serde_json::Value::from(1),
        );
    }
    sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| encoding_error())
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json_keys(value);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                sort_json_keys(value);
            }
            values.sort_keys();
        }
        _ => {}
    }
}

/// Canonical byte-stable public expression-pack fingerprint.
pub fn expression_pack_fingerprint(pack: &ExpressionPack) -> Result<String, ExpressionError> {
    canonical_hash(pack)
}

/// Exact immutable coordinate for a validated expression pack.
pub fn expression_pack_ref(pack: &ExpressionPack) -> Result<ExpressionPackRef, ExpressionError> {
    validate_expression_pack(pack)?;
    Ok(ExpressionPackRef {
        id: pack.id.clone(),
        version: pack.version.clone(),
        sha256: expression_pack_fingerprint(pack)?,
    })
}

/// Canonical fingerprint for one exact Character profile.
pub fn expression_profile_fingerprint(
    profile: &CharacterProfile,
) -> Result<String, ExpressionError> {
    canonical_hash(profile)
}

/// Validate a complete public expression pack and every template token.
pub fn validate_expression_pack(pack: &ExpressionPack) -> Result<(), ExpressionError> {
    if pack.pack_format_version != EXPRESSION_PACK_FORMAT_VERSION {
        return Err(invalid_structure(
            "pack_format_version",
            "unsupported expression pack version",
        ));
    }
    namespaced("id", &pack.id)?;
    semver("version", &pack.version)?;
    text_value("title", &pack.title, 1, 256)?;
    text_value("description", &pack.description, 1, 2_048)?;
    if !pack.independently_authored {
        return Err(invalid_structure(
            "independently_authored",
            "public expression packs must affirm independent authorship",
        ));
    }
    text_value("license", &pack.license, 1, 128)?;
    text_value("license_url", &pack.license_url, 8, 2_048)?;
    validate_provenance(&pack.provenance).map_err(|_| {
        invalid_structure(
            "provenance",
            "expression pack provenance is invalid or incomplete",
        )
    })?;
    validate_pack_eligibility(&pack.eligibility)?;
    validate_pack_coverage_requirements(&pack.coverage)?;
    let provenance_ids = provenance_ids(&pack.provenance);
    if pack.entries.is_empty() || pack.entries.len() > 16_384 {
        return Err(invalid_structure(
            "entries",
            "expression pack requires a bounded non-empty entry catalog",
        ));
    }
    for (id, entry) in &pack.entries {
        let path = format!("entries.{id}");
        local(&path, id)?;
        if entry.id != *id {
            return Err(invalid_link(
                format!("{path}.id"),
                "expression entry id must equal its containing map key",
            ));
        }
        validate_expression_pack_entry(&path, entry, &provenance_ids)?;
    }
    for (id, pool) in &pack.vocabulary_pools {
        let path = format!("vocabulary_pools.{id}");
        local(&path, id)?;
        if pool.id != *id {
            return Err(invalid_link(
                format!("{path}.id"),
                "expression pool id must equal its containing map key",
            ));
        }
        namespaced(&format!("{path}.category"), &pool.category)?;
        validate_applicability(&format!("{path}.applicability"), &pool.applicability)?;
        validate_source_ids(
            &format!("{path}.source_ids"),
            &pool.source_ids,
            &provenance_ids,
        )?;
        validate_sorted_local_ids(&format!("{path}.entry_ids"), &pool.entry_ids, false)?;
        for entry_id in &pool.entry_ids {
            if !matches!(
                pack.entries.get(entry_id).map(|entry| &entry.value),
                Some(ExpressionPackValue::Term { .. })
            ) {
                return Err(invalid_link(
                    format!("{path}.entry_ids"),
                    "expression pool references a missing or non-term entry",
                ));
            }
        }
    }
    if pack.templates.is_empty() || pack.templates.len() > 4_096 {
        return Err(invalid_structure(
            "templates",
            "expression pack requires a bounded non-empty template catalog",
        ));
    }
    for (id, template) in &pack.templates {
        let path = format!("templates.{id}");
        local(&path, id)?;
        if template.id != *id {
            return Err(invalid_link(
                format!("{path}.id"),
                "dialogue template id must equal its containing map key",
            ));
        }
        validate_dialogue_template(&path, template, &provenance_ids)?;
    }
    Ok(())
}

fn validate_pack_eligibility(value: &ExpressionPackEligibility) -> Result<(), ExpressionError> {
    if value.compatible_profile_versions.is_empty()
        || value.compatible_profile_versions.len() > 32
        || !strictly_sorted(&value.compatible_profile_versions)
        || value.compatible_profile_versions.contains(&0)
    {
        return Err(invalid_structure(
            "eligibility.compatible_profile_versions",
            "compatible profile versions must be unique and sorted",
        ));
    }
    validate_sorted_namespaced_ids(
        "eligibility.required_extension_namespaces",
        &value.required_extension_namespaces,
        true,
    )
}

fn validate_pack_coverage_requirements(
    value: &ExpressionPackCoverageRequirements,
) -> Result<(), ExpressionError> {
    if [
        value.minimum_terms,
        value.minimum_preferences,
        value.minimum_behavioral_signatures,
        value.minimum_voice_constraints,
    ]
    .into_iter()
    .any(|count| count > 65_536)
    {
        return Err(invalid_structure(
            "coverage",
            "expression coverage minima are out of range",
        ));
    }
    validate_sorted_namespaced_ids(
        "coverage.required_categories",
        &value.required_categories,
        true,
    )?;
    validate_sorted_local_ids(
        "coverage.required_scenario_ids",
        &value.required_scenario_ids,
        true,
    )
}

fn validate_expression_pack_entry(
    path: &str,
    entry: &ExpressionPackEntry,
    provenance_ids: &BTreeSet<&str>,
) -> Result<(), ExpressionError> {
    text_value(&format!("{path}.label"), &entry.label, 1, 256)?;
    text_value(&format!("{path}.description"), &entry.description, 1, 2_048)?;
    unit_interval(&format!("{path}.strength"), entry.strength)?;
    validate_applicability(&format!("{path}.applicability"), &entry.applicability)?;
    validate_source_ids(
        &format!("{path}.source_ids"),
        &entry.source_ids,
        provenance_ids,
    )?;
    validate_sorted_text(
        &format!("{path}.limitations"),
        &entry.limitations,
        1,
        2_048,
        false,
    )?;
    match &entry.value {
        ExpressionPackValue::Term {
            category,
            term_kind,
            surface,
        } => {
            namespaced(&format!("{path}.value.category"), category)?;
            text_value(&format!("{path}.value.surface"), surface, 1, 512)?;
            let normalized = normalize_expression_text(surface);
            if normalized.is_empty()
                || (*term_kind == ExpressionTermKind::Term && normalized.contains(' '))
            {
                return Err(invalid_structure(
                    format!("{path}.value.surface"),
                    "expression term kind does not match its normalized text",
                ));
            }
        }
        ExpressionPackValue::Preference {
            category, target, ..
        } => {
            namespaced(&format!("{path}.value.category"), category)?;
            normalized_text(&format!("{path}.value.target"), target)?;
        }
        ExpressionPackValue::BehavioralSignature { category, cue } => {
            namespaced(&format!("{path}.value.category"), category)?;
            text_value(&format!("{path}.value.cue"), cue, 1, 2_048)?;
        }
        ExpressionPackValue::VoiceConstraint {
            category,
            target,
            instruction,
            ..
        } => {
            namespaced(&format!("{path}.value.category"), category)?;
            normalized_text(&format!("{path}.value.target"), target)?;
            text_value(&format!("{path}.value.instruction"), instruction, 1, 2_048)?;
        }
    }
    Ok(())
}

fn validate_dialogue_template(
    path: &str,
    template: &ExpressionDialogueTemplate,
    provenance_ids: &BTreeSet<&str>,
) -> Result<(), ExpressionError> {
    local(&format!("{path}.scenario_id"), &template.scenario_id)?;
    if let ExpressionSpeakerRequirement::ExactCharacter { character_id } =
        &template.speaker_requirement
    {
        namespaced(
            &format!("{path}.speaker_requirement.character_id"),
            character_id,
        )?;
    }
    if template.placeholders.len() > 128 {
        return Err(invalid_structure(
            format!("{path}.placeholders"),
            "dialogue template declares too many placeholders",
        ));
    }
    for (id, declaration) in &template.placeholders {
        let placeholder_path = format!("{path}.placeholders.{id}");
        local(&placeholder_path, id)?;
        if declaration.id != *id {
            return Err(invalid_link(
                format!("{placeholder_path}.id"),
                "placeholder id must equal its containing map key",
            ));
        }
        if restricted_placeholder_name(id) {
            let source = template.variants.values().find_map(|variant| {
                parse_placeholder_tokens(
                    &format!("{path}.variants.{}.content", variant.id),
                    &variant.content,
                    &variant.source,
                )
                .ok()?
                .into_iter()
                .find(|token| token.id == *id)
                .map(|token| token.source)
            });
            return Err(expression_error(
                ExpressionDiagnosticCode::UnsafePersonalization,
                ExpressionDiagnosticSeverity::Error,
                placeholder_path,
                "placeholder name is reserved because it could imply sensitive runtime data",
                source,
            ));
        }
        if let ExpressionPlaceholderValue::LexiconTerm { term_id } = &declaration.value {
            local(&format!("{placeholder_path}.value.term_id"), term_id)?;
        }
    }
    if template.variants.is_empty() || template.variants.len() > 4_096 {
        return Err(invalid_structure(
            format!("{path}.variants"),
            "dialogue template requires bounded variants",
        ));
    }
    local(
        &format!("{path}.fallback_variant_id"),
        &template.fallback_variant_id,
    )?;
    let fallback = template
        .variants
        .get(&template.fallback_variant_id)
        .ok_or_else(|| {
            invalid_link(
                format!("{path}.fallback_variant_id"),
                "dialogue fallback variant is absent",
            )
        })?;
    if !fallback.applicability.scenario_ids.is_empty()
        || !fallback.applicability.predicates.is_empty()
        || fallback.authority == ExpressionTextAuthority::Suggested
        || fallback.review == ReviewState::Pending
    {
        return Err(invalid_structure(
            format!("{path}.fallback_variant_id"),
            "dialogue fallback must be unconditional authored or reviewed text",
        ));
    }
    validate_source_ids(
        &format!("{path}.source_ids"),
        &template.source_ids,
        provenance_ids,
    )?;
    validate_sorted_text(
        &format!("{path}.limitations"),
        &template.limitations,
        1,
        2_048,
        false,
    )?;
    for (id, variant) in &template.variants {
        let variant_path = format!("{path}.variants.{id}");
        local(&variant_path, id)?;
        if variant.id != *id {
            return Err(invalid_link(
                format!("{variant_path}.id"),
                "dialogue variant id must equal its containing map key",
            ));
        }
        validate_dialogue_variant(
            &variant_path,
            variant,
            &template.placeholders,
            provenance_ids,
        )?;
    }
    let fallback_tokens = parse_placeholder_tokens(
        &format!("{path}.variants.{}.content", template.fallback_variant_id),
        &fallback.content,
        &fallback.source,
    )?;
    for token in fallback_tokens {
        let declaration = &template.placeholders[&token.id];
        if !matches!(
            declaration.value,
            ExpressionPlaceholderValue::SpeakerDisplayName
        ) {
            return Err(expression_error(
                ExpressionDiagnosticCode::UnavailablePlaceholder,
                ExpressionDiagnosticSeverity::Error,
                format!("{path}.variants.{}.content", template.fallback_variant_id),
                "dialogue fallback may only use the always-available speaker display name",
                Some(token.source),
            ));
        }
    }
    Ok(())
}

fn validate_dialogue_variant(
    path: &str,
    variant: &ExpressionDialogueVariant,
    placeholders: &BTreeMap<String, ExpressionPlaceholderDeclaration>,
    provenance_ids: &BTreeSet<&str>,
) -> Result<(), ExpressionError> {
    text_value(&format!("{path}.content"), &variant.content, 1, 8_192)?;
    if !matches!(
        (variant.authority, variant.review),
        (
            ExpressionTextAuthority::Suggested,
            ReviewState::Pending | ReviewState::Rejected
        ) | (ExpressionTextAuthority::Reviewed, ReviewState::Accepted)
            | (
                ExpressionTextAuthority::Authored,
                ReviewState::NotRequired | ReviewState::Accepted
            )
    ) {
        return Err(invalid_structure(
            format!("{path}.review"),
            "dialogue variant authority and review state are inconsistent",
        ));
    }
    validate_applicability(&format!("{path}.applicability"), &variant.applicability)?;
    validate_source_location(&format!("{path}.source"), &variant.source)?;
    validate_source_ids(
        &format!("{path}.source_ids"),
        &variant.source_ids,
        provenance_ids,
    )?;
    let tokens = parse_placeholder_tokens(
        &format!("{path}.content"),
        &variant.content,
        &variant.source,
    )?;
    for token in tokens {
        if !placeholders.contains_key(&token.id) {
            return Err(expression_error(
                ExpressionDiagnosticCode::UnresolvedToken,
                ExpressionDiagnosticSeverity::Error,
                format!("{path}.content"),
                "dialogue text contains a token outside its declared placeholder allowlist",
                Some(token.source),
            ));
        }
    }
    Ok(())
}

fn validate_source_location(
    path: &str,
    value: &ExpressionSourceLocation,
) -> Result<(), ExpressionError> {
    text_value(&format!("{path}.source_id"), &value.source_id, 1, 1_024)?;
    if value.line == 0 || value.column == 0 {
        return Err(invalid_structure(
            path,
            "source locations use one-based line and column coordinates",
        ));
    }
    Ok(())
}

fn validate_applicability(
    path: &str,
    value: &ExpressionApplicability,
) -> Result<(), ExpressionError> {
    validate_sorted_local_ids(&format!("{path}.scenario_ids"), &value.scenario_ids, true)?;
    if value.predicates.len() > 128 {
        return Err(invalid_structure(
            format!("{path}.predicates"),
            "expression applicability contains too many predicates",
        ));
    }
    let mut canonical = Vec::with_capacity(value.predicates.len());
    for (index, predicate) in value.predicates.iter().enumerate() {
        let predicate_path = format!("{path}.predicates[{index}]");
        match predicate {
            ExpressionContextPredicate::PersonalityBand { bands, .. } => {
                if bands.is_empty() || bands.len() > 5 || has_duplicate_trait_bands(bands) {
                    return Err(invalid_structure(
                        format!("{predicate_path}.bands"),
                        "personality predicate bands must be unique and non-empty",
                    ));
                }
            }
            ExpressionContextPredicate::Relationship {
                relationship_kind_id,
                other_character_id,
            } => {
                namespaced(
                    &format!("{predicate_path}.relationship_kind_id"),
                    relationship_kind_id,
                )?;
                if let Some(character_id) = other_character_id {
                    namespaced(
                        &format!("{predicate_path}.other_character_id"),
                        character_id,
                    )?;
                }
            }
            ExpressionContextPredicate::DateContext { cue_id } => {
                local(&format!("{predicate_path}.cue_id"), cue_id)?;
            }
            ExpressionContextPredicate::WorldContext { tag } => {
                local(&format!("{predicate_path}.tag"), tag)?;
            }
        }
        canonical.push(canonical_json(predicate)?);
    }
    if !strictly_sorted(&canonical) {
        return Err(invalid_structure(
            format!("{path}.predicates"),
            "expression predicates must be unique and canonically sorted",
        ));
    }
    Ok(())
}

fn has_duplicate_trait_bands(values: &[TraitBand]) -> bool {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .any(|value| !seen.insert(trait_band_rank(*value)))
}

const fn trait_band_rank(value: TraitBand) -> u8 {
    match value {
        TraitBand::VeryLow => 0,
        TraitBand::Low => 1,
        TraitBand::Middle => 2,
        TraitBand::High => 3,
        TraitBand::VeryHigh => 4,
    }
}

/// Validate the normalized expression payload embedded in one Character profile.
pub(crate) fn validate_expression_data(
    namespace: &str,
    profile_id: &str,
    value: &ExpressionData,
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    let root = format!("extensions.{namespace}.value");
    if value.expression_format_version != expression_format_version() {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::UnsupportedVersion,
            format!("{root}.expression_format_version"),
            "unsupported expression value version",
        ));
    }
    if value.character_id != profile_id {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            format!("{root}.character_id"),
            "expression owner must equal the containing profile id",
        ));
    }
    if value.lexicon.len() > 65_536
        || value.preferences.len() > 65_536
        || value.vocabulary_pools.len() > 16_384
        || value.voice_constraints.len() > 16_384
        || value.template_assignments.len() > 4_096
    {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            root,
            "expression record collection exceeds its bounded limit",
        ));
    }
    for (id, term) in &value.lexicon {
        let path = format!("{root}.lexicon.{id}");
        validate_profile_term(&path, profile_id, id, term, lineage)?;
    }
    for (id, preference) in &value.preferences {
        let path = format!("{root}.preferences.{id}");
        validate_profile_preference(&path, profile_id, id, preference, lineage)?;
    }
    for (id, pool) in &value.vocabulary_pools {
        let path = format!("{root}.vocabulary_pools.{id}");
        validate_profile_pool(&path, profile_id, id, pool, &value.lexicon, lineage)?;
    }
    for (id, constraint) in &value.voice_constraints {
        let path = format!("{root}.voice_constraints.{id}");
        validate_profile_constraint(&path, profile_id, id, constraint, lineage)?;
    }
    for (id, assignment) in &value.template_assignments {
        let path = format!("{root}.template_assignments.{id}");
        validate_profile_assignment(path.as_str(), profile_id, id, assignment, lineage)?;
    }
    validate_character_refs(
        &format!("{root}.behavioral_signature_refs"),
        &value.behavioral_signature_refs,
    )?;
    let mut prior_pack = None::<(&str, &str, &str)>;
    for (index, pack) in value.source_pack_refs.iter().enumerate() {
        let path = format!("{root}.source_pack_refs[{index}]");
        validate_expression_pack_ref_contract(&path, pack)?;
        let key = (
            pack.id.as_str(),
            pack.version.as_str(),
            pack.sha256.as_str(),
        );
        if prior_pack.is_some_and(|prior| prior >= key) {
            return Err(crate::validation::error(
                crate::CharacterDiagnosticCode::InvalidValue,
                format!("{root}.source_pack_refs"),
                "expression pack coordinates must be unique and sorted",
            ));
        }
        prior_pack = Some(key);
    }
    for (id, assignment) in &value.template_assignments {
        if value
            .source_pack_refs
            .binary_search(&assignment.pack)
            .is_err()
        {
            return Err(crate::validation::error(
                crate::CharacterDiagnosticCode::InvalidReference,
                format!("{root}.template_assignments.{id}.pack"),
                "template assignment pack is absent from retained expression pack coordinates",
            ));
        }
    }
    Ok(())
}

/// Validate behavioral signatures embedded beside the expression extension.
pub(crate) fn validate_behavioral_signature_data(
    namespace: &str,
    profile_id: &str,
    value: &BehavioralSignatures,
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    let root = format!("extensions.{namespace}.value");
    if value.character_id != profile_id {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            format!("{root}.character_id"),
            "behavioral-signature owner must equal the containing profile id",
        ));
    }
    if value.signatures.len() > 16_384 {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            format!("{root}.signatures"),
            "behavioral-signature collection exceeds its bounded limit",
        ));
    }
    for (id, signature) in &value.signatures {
        let path = format!("{root}.signatures.{id}");
        contract_local(&path, id)?;
        if signature.id != *id || signature.character_id != profile_id {
            return Err(crate::validation::error(
                crate::CharacterDiagnosticCode::InvalidReference,
                path,
                "behavioral signature id and character link must match their container",
            ));
        }
        contract_namespaced(
            &format!("{root}.signatures.{id}.category"),
            &signature.category,
        )?;
        contract_text(
            &format!("{root}.signatures.{id}.cue"),
            &signature.cue,
            1,
            2_048,
        )?;
        contract_unit_interval(
            &format!("{root}.signatures.{id}.strength"),
            signature.strength,
        )?;
        contract_applicability(
            &format!("{root}.signatures.{id}.applicability"),
            &signature.applicability,
        )?;
        contract_origin_review(
            &format!("{root}.signatures.{id}"),
            signature.origin,
            signature.review,
            signature.rationale.as_deref(),
        )?;
        contract_source_ids(
            &format!("{root}.signatures.{id}.source_ids"),
            &signature.source_ids,
            lineage,
        )?;
    }
    Ok(())
}

/// Validate cross-extension behavioral-signature and assignment links.
pub(crate) fn validate_expression_profile_links(
    profile: &CharacterProfile,
) -> Result<(), crate::CharacterError> {
    let expression = profile
        .extensions
        .get(EXPRESSION_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::Expression(record) => Some(&record.value),
            _ => None,
        });
    let signatures = profile
        .extensions
        .get(BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::BehavioralSignatures(record) => Some(&record.value),
            _ => None,
        });
    let Some(expression) = expression else {
        return Ok(());
    };
    let available = signatures
        .into_iter()
        .flat_map(|value| value.signatures.keys())
        .map(|id| behavioral_signature_ref(&profile.id, id))
        .collect::<BTreeSet<_>>();
    for (index, reference) in expression.behavioral_signature_refs.iter().enumerate() {
        if !available.contains(reference) {
            return Err(crate::validation::error(
                crate::CharacterDiagnosticCode::InvalidReference,
                format!(
                    "extensions.{EXPRESSION_EXTENSION_NAMESPACE}.value.behavioral_signature_refs[{index}]"
                ),
                "expression references an unavailable behavioral signature",
            ));
        }
    }
    Ok(())
}

fn validate_profile_term(
    path: &str,
    profile_id: &str,
    map_id: &str,
    term: &NormalizedExpressionTerm,
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    contract_local(path, map_id)?;
    if term.id != map_id || term.character_id != profile_id {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            path,
            "expression term id and character link must match their container",
        ));
    }
    contract_namespaced(&format!("{path}.category"), &term.category)?;
    contract_text(&format!("{path}.surface"), &term.surface, 1, 512)?;
    contract_normalized(&format!("{path}.normalized"), &term.normalized)?;
    if term.normalized != normalize_expression_text(&term.surface)
        || (term.kind == ExpressionTermKind::Term && term.normalized.contains(' '))
    {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            format!("{path}.normalized"),
            "expression term does not match deterministic normalization",
        ));
    }
    contract_unit_interval(&format!("{path}.strength"), term.strength)?;
    contract_applicability(&format!("{path}.applicability"), &term.applicability)?;
    contract_origin_review(path, term.origin, term.review, term.rationale.as_deref())?;
    contract_source_ids(&format!("{path}.source_ids"), &term.source_ids, lineage)
}

fn validate_profile_preference(
    path: &str,
    profile_id: &str,
    map_id: &str,
    preference: &NormalizedPreference,
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    contract_local(path, map_id)?;
    if preference.id != map_id || preference.character_id != profile_id {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            path,
            "expression preference id and character link must match their container",
        ));
    }
    contract_namespaced(&format!("{path}.category"), &preference.category)?;
    contract_normalized(&format!("{path}.target"), &preference.target)?;
    contract_unit_interval(&format!("{path}.strength"), preference.strength)?;
    contract_applicability(&format!("{path}.applicability"), &preference.applicability)?;
    contract_origin_review(
        path,
        preference.origin,
        preference.review,
        preference.rationale.as_deref(),
    )?;
    contract_source_ids(
        &format!("{path}.source_ids"),
        &preference.source_ids,
        lineage,
    )
}

fn validate_profile_pool(
    path: &str,
    profile_id: &str,
    map_id: &str,
    pool: &ExpressionVocabularyPool,
    lexicon: &BTreeMap<String, NormalizedExpressionTerm>,
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    contract_local(path, map_id)?;
    if pool.id != map_id || pool.character_id != profile_id {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            path,
            "vocabulary pool id and character link must match their container",
        ));
    }
    contract_namespaced(&format!("{path}.category"), &pool.category)?;
    contract_sorted_local_ids(&format!("{path}.term_ids"), &pool.term_ids, false)?;
    if pool.term_ids.iter().any(|id| !lexicon.contains_key(id)) {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            format!("{path}.term_ids"),
            "vocabulary pool references an unavailable expression term",
        ));
    }
    contract_applicability(&format!("{path}.applicability"), &pool.applicability)?;
    contract_origin_review(path, pool.origin, pool.review, pool.rationale.as_deref())?;
    contract_source_ids(&format!("{path}.source_ids"), &pool.source_ids, lineage)
}

fn validate_profile_constraint(
    path: &str,
    profile_id: &str,
    map_id: &str,
    constraint: &ExpressionVoiceConstraint,
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    contract_local(path, map_id)?;
    if constraint.id != map_id || constraint.character_id != profile_id {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            path,
            "voice constraint id and character link must match their container",
        ));
    }
    contract_namespaced(&format!("{path}.category"), &constraint.category)?;
    contract_normalized(&format!("{path}.target"), &constraint.target)?;
    contract_text(
        &format!("{path}.instruction"),
        &constraint.instruction,
        1,
        2_048,
    )?;
    contract_unit_interval(&format!("{path}.strength"), constraint.strength)?;
    contract_applicability(&format!("{path}.applicability"), &constraint.applicability)?;
    contract_origin_review(
        path,
        constraint.origin,
        constraint.review,
        constraint.rationale.as_deref(),
    )?;
    contract_source_ids(
        &format!("{path}.source_ids"),
        &constraint.source_ids,
        lineage,
    )
}

fn validate_profile_assignment(
    path: &str,
    profile_id: &str,
    map_id: &str,
    assignment: &ExpressionTemplateAssignment,
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    contract_local(path, map_id)?;
    if assignment.id != map_id || assignment.character_id != profile_id {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            path,
            "template assignment id and character link must match their container",
        ));
    }
    contract_local(&format!("{path}.scenario_id"), &assignment.scenario_id)?;
    validate_expression_pack_ref_contract(&format!("{path}.pack"), &assignment.pack)?;
    contract_local(&format!("{path}.template_id"), &assignment.template_id)?;
    if !matches!(
        (assignment.state, assignment.review),
        (
            ValueState::Authored | ValueState::Reviewed | ValueState::Overridden,
            ReviewState::Accepted | ReviewState::NotRequired
        )
    ) {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            path,
            "template assignment must be authored or accepted reviewed data",
        ));
    }
    contract_source_ids(
        &format!("{path}.source_ids"),
        &assignment.source_ids,
        lineage,
    )?;
    contract_text(
        &format!("{path}.rationale"),
        &assignment.rationale,
        1,
        2_048,
    )
}

fn contract_origin_review(
    path: &str,
    origin: ExpressionRecordOrigin,
    review: ReviewState,
    rationale: Option<&str>,
) -> Result<(), crate::CharacterError> {
    let valid = matches!(
        (origin, review),
        (
            ExpressionRecordOrigin::Authored,
            ReviewState::NotRequired | ReviewState::Accepted
        ) | (
            ExpressionRecordOrigin::Imported | ExpressionRecordOrigin::PackAssigned,
            ReviewState::Accepted
        ) | (
            ExpressionRecordOrigin::Suggested,
            ReviewState::Pending | ReviewState::Rejected
        ) | (
            ExpressionRecordOrigin::ReviewedSuggestion,
            ReviewState::Accepted
        )
    );
    if !valid || (origin != ExpressionRecordOrigin::Authored && rationale.is_none()) {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            path,
            "expression origin, review, and rationale are inconsistent",
        ));
    }
    if let Some(rationale) = rationale {
        contract_text(&format!("{path}.rationale"), rationale, 1, 2_048)?;
    }
    Ok(())
}

fn contract_source_ids(
    path: &str,
    values: &[String],
    lineage: &BTreeSet<&str>,
) -> Result<(), crate::CharacterError> {
    if values.is_empty() || values.len() > 4_096 || !strictly_sorted(values) {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidLineage,
            path,
            "expression source ids must be bounded, unique, and sorted",
        ));
    }
    if values.iter().any(|value| !lineage.contains(value.as_str())) {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidLineage,
            path,
            "expression source id is absent from profile provenance",
        ));
    }
    Ok(())
}

fn contract_applicability(
    path: &str,
    value: &ExpressionApplicability,
) -> Result<(), crate::CharacterError> {
    validate_applicability(path, value).map_err(|_| {
        crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            path,
            "expression applicability is invalid or not canonically ordered",
        )
    })
}

fn contract_unit_interval(path: &str, value: f64) -> Result<(), crate::CharacterError> {
    unit_interval(path, value).map_err(|_| {
        crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            path,
            "expression strength must be exact bounded millionths",
        )
    })
}

fn contract_normalized(path: &str, value: &str) -> Result<(), crate::CharacterError> {
    normalized_text(path, value).map_err(|_| {
        crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            path,
            "expression text must be normalized lowercase text",
        )
    })
}

fn contract_local(path: &str, value: &str) -> Result<(), crate::CharacterError> {
    validate_local_id(path, value)
}

fn contract_namespaced(path: &str, value: &str) -> Result<(), crate::CharacterError> {
    validate_namespaced_id(path, value)
}

fn contract_text(
    path: &str,
    value: &str,
    min: usize,
    max: usize,
) -> Result<(), crate::CharacterError> {
    validate_text(path, value, min, max)
}

fn contract_sorted_local_ids(
    path: &str,
    values: &[String],
    allow_empty: bool,
) -> Result<(), crate::CharacterError> {
    validate_sorted_local_ids(path, values, allow_empty).map_err(|_| {
        crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidValue,
            path,
            "expression identifiers must be unique and sorted",
        )
    })
}

fn validate_expression_pack_ref_contract(
    path: &str,
    value: &ExpressionPackRef,
) -> Result<(), crate::CharacterError> {
    validate_namespaced_id(&format!("{path}.id"), &value.id)?;
    validate_semver(&format!("{path}.version"), &value.version)?;
    validate_sha256(&format!("{path}.sha256"), &value.sha256)
}

fn validate_character_refs(path: &str, values: &[String]) -> Result<(), crate::CharacterError> {
    if values.len() > 16_384 || !strictly_sorted(values) {
        return Err(crate::validation::error(
            crate::CharacterDiagnosticCode::InvalidReference,
            path,
            "behavioral-signature references must be unique and sorted",
        ));
    }
    for value in values {
        validate_namespaced_id(path, value)?;
    }
    Ok(())
}

fn behavioral_signature_ref(character_id: &str, signature_id: &str) -> String {
    format!("{character_id}.signature.{signature_id}")
}

#[derive(Debug)]
struct PlaceholderToken {
    id: String,
    source: ExpressionSourceLocation,
}

/// Deterministic comparison normalizer. Authored surface text remains separately retained.
#[must_use]
pub fn normalize_expression_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn parse_placeholder_tokens(
    path: &str,
    content: &str,
    source: &ExpressionSourceLocation,
) -> Result<Vec<PlaceholderToken>, ExpressionError> {
    let mut tokens = Vec::new();
    let mut cursor = 0usize;
    while cursor < content.len() {
        let Some(relative_start) = content[cursor..].find("{{") else {
            if let Some(relative_close) = content[cursor..].find("}}") {
                let offset = cursor + relative_close;
                return Err(expression_error(
                    ExpressionDiagnosticCode::InvalidPlaceholder,
                    ExpressionDiagnosticSeverity::Error,
                    path,
                    "dialogue text contains a malformed placeholder delimiter",
                    Some(offset_source_location(content, source, offset)),
                ));
            }
            break;
        };
        let start = cursor + relative_start;
        if content[cursor..start].contains("}}") {
            return Err(expression_error(
                ExpressionDiagnosticCode::InvalidPlaceholder,
                ExpressionDiagnosticSeverity::Error,
                path,
                "dialogue text contains a malformed placeholder delimiter",
                Some(offset_source_location(content, source, start)),
            ));
        }
        let token_start = start + 2;
        let Some(relative_end) = content[token_start..].find("}}") else {
            return Err(expression_error(
                ExpressionDiagnosticCode::InvalidPlaceholder,
                ExpressionDiagnosticSeverity::Error,
                path,
                "dialogue text contains an unterminated placeholder",
                Some(offset_source_location(content, source, start)),
            ));
        };
        let end = token_start + relative_end;
        let raw = &content[token_start..end];
        if raw.contains("{{") || raw.trim() != raw || local(path, raw).is_err() {
            return Err(expression_error(
                ExpressionDiagnosticCode::InvalidPlaceholder,
                ExpressionDiagnosticSeverity::Error,
                path,
                "dialogue text contains a malformed placeholder identifier",
                Some(offset_source_location(content, source, start)),
            ));
        }
        tokens.push(PlaceholderToken {
            id: raw.to_owned(),
            source: offset_source_location(content, source, start),
        });
        cursor = end + 2;
    }
    Ok(tokens)
}

fn offset_source_location(
    content: &str,
    base: &ExpressionSourceLocation,
    byte_offset: usize,
) -> ExpressionSourceLocation {
    let prefix = &content[..byte_offset.min(content.len())];
    let added_lines = prefix.chars().filter(|value| *value == '\n').count() as u32;
    let column = prefix
        .rsplit_once('\n')
        .map_or(base.column + prefix.chars().count() as u32, |(_, tail)| {
            1 + tail.chars().count() as u32
        });
    ExpressionSourceLocation {
        source_id: base.source_id.clone(),
        line: base.line.saturating_add(added_lines),
        column,
    }
}

fn restricted_placeholder_name(value: &str) -> bool {
    const RESTRICTED: [&str; 10] = [
        "authorization",
        "cookie",
        "credential",
        "database_url",
        "password",
        "private_key",
        "secret",
        "session",
        "token",
        "webhook",
    ];
    RESTRICTED
        .into_iter()
        .any(|restricted| value == restricted || value.starts_with(&format!("{restricted}_")))
}

fn provenance_ids(value: &Provenance) -> BTreeSet<&str> {
    value
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .chain(
            value
                .transformations
                .iter()
                .map(|transformation| transformation.id.as_str()),
        )
        .collect()
}

fn validate_source_ids(
    path: &str,
    values: &[String],
    available: &BTreeSet<&str>,
) -> Result<(), ExpressionError> {
    if values.is_empty() || values.len() > 4_096 || !strictly_sorted(values) {
        return Err(invalid_structure(
            path,
            "expression source ids must be bounded, unique, and sorted",
        ));
    }
    if values
        .iter()
        .any(|value| !available.contains(value.as_str()))
    {
        return Err(invalid_link(
            path,
            "expression source id is absent from retained provenance",
        ));
    }
    Ok(())
}

fn validate_sorted_local_ids(
    path: &str,
    values: &[String],
    allow_empty: bool,
) -> Result<(), ExpressionError> {
    if (!allow_empty && values.is_empty()) || values.len() > 65_536 || !strictly_sorted(values) {
        return Err(invalid_structure(
            path,
            "expression identifiers must be bounded, unique, and sorted",
        ));
    }
    for value in values {
        local(path, value)?;
    }
    Ok(())
}

fn validate_sorted_namespaced_ids(
    path: &str,
    values: &[String],
    allow_empty: bool,
) -> Result<(), ExpressionError> {
    if (!allow_empty && values.is_empty()) || values.len() > 65_536 || !strictly_sorted(values) {
        return Err(invalid_structure(
            path,
            "expression namespaced identifiers must be bounded, unique, and sorted",
        ));
    }
    for value in values {
        namespaced(path, value)?;
    }
    Ok(())
}

fn validate_sorted_text(
    path: &str,
    values: &[String],
    min: usize,
    max: usize,
    allow_empty: bool,
) -> Result<(), ExpressionError> {
    if (!allow_empty && values.is_empty()) || values.len() > 65_536 || !strictly_sorted(values) {
        return Err(invalid_structure(
            path,
            "expression text list must be bounded, unique, and sorted",
        ));
    }
    for value in values {
        text_value(path, value, min, max)?;
    }
    Ok(())
}

fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|window| window[0] < window[1])
}

fn unit_interval(path: &str, value: f64) -> Result<(), ExpressionError> {
    let micros = value * 1_000_000.0;
    if !value.is_finite()
        || !(0.0..=1.0).contains(&value)
        || (micros - micros.round()).abs() > 0.000_001
    {
        return Err(invalid_structure(
            path,
            "expression strength must retain exact bounded millionths",
        ));
    }
    Ok(())
}

fn normalized_text(path: &str, value: &str) -> Result<(), ExpressionError> {
    text_value(path, value, 1, 2_048)?;
    if value != normalize_expression_text(value) {
        return Err(invalid_structure(
            path,
            "expression comparison text is not deterministically normalized",
        ));
    }
    Ok(())
}

fn namespaced(path: &str, value: &str) -> Result<(), ExpressionError> {
    validate_namespaced_id(path, value)
        .map_err(|_| invalid_structure(path, "expected a stable namespaced identifier"))
}

fn local(path: &str, value: &str) -> Result<(), ExpressionError> {
    validate_local_id(path, value)
        .map_err(|_| invalid_structure(path, "expected a stable local identifier"))
}

fn semver(path: &str, value: &str) -> Result<(), ExpressionError> {
    validate_semver(path, value)
        .map_err(|_| invalid_structure(path, "expected a stable semantic version"))
}

fn sha256(path: &str, value: &str) -> Result<(), ExpressionError> {
    validate_sha256(path, value)
        .map_err(|_| invalid_structure(path, "expected a lowercase SHA-256 fingerprint"))
}

fn text_value(path: &str, value: &str, min: usize, max: usize) -> Result<(), ExpressionError> {
    validate_text(path, value, min, max).map_err(|_| {
        expression_error(
            if value.is_empty() {
                ExpressionDiagnosticCode::EmptyText
            } else {
                ExpressionDiagnosticCode::TextLimit
            },
            ExpressionDiagnosticSeverity::Error,
            path,
            if value.is_empty() {
                "expression text must not be empty"
            } else {
                "expression text is outside its documented limit"
            },
            None,
        )
    })
}

fn canonical_json(value: &impl Serialize) -> Result<String, ExpressionError> {
    serde_json::to_string(value).map_err(|_| encoding_error())
}

fn canonical_hash(value: &impl Serialize) -> Result<String, ExpressionError> {
    let bytes = serde_json::to_vec(value).map_err(|_| encoding_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn invalid_structure(path: impl Into<String>, message: &'static str) -> ExpressionError {
    expression_error(
        ExpressionDiagnosticCode::InvalidStructure,
        ExpressionDiagnosticSeverity::Error,
        path,
        message,
        None,
    )
}

fn invalid_link(path: impl Into<String>, message: &'static str) -> ExpressionError {
    expression_error(
        ExpressionDiagnosticCode::InvalidLink,
        ExpressionDiagnosticSeverity::Error,
        path,
        message,
        None,
    )
}

fn stale_pack(path: impl Into<String>, message: &'static str) -> ExpressionError {
    expression_error(
        ExpressionDiagnosticCode::StalePack,
        ExpressionDiagnosticSeverity::Error,
        path,
        message,
        None,
    )
}

fn expression_error(
    code: ExpressionDiagnosticCode,
    severity: ExpressionDiagnosticSeverity,
    path: impl Into<String>,
    message: impl Into<String>,
    source: Option<ExpressionSourceLocation>,
) -> ExpressionError {
    ExpressionError {
        diagnostic: ExpressionDiagnostic {
            code,
            severity,
            path: path.into(),
            message: message.into(),
            source,
        },
    }
}

fn encoding_error() -> ExpressionError {
    invalid_structure(
        "document",
        "expression document does not match the strict serialized contract",
    )
}

fn expression_diagnostic_code(code: ExpressionDiagnosticCode) -> &'static str {
    match code {
        ExpressionDiagnosticCode::EmptyText => "X100",
        ExpressionDiagnosticCode::TextLimit => "X101",
        ExpressionDiagnosticCode::UnresolvedToken => "X102",
        ExpressionDiagnosticCode::ConflictingConstraint => "X103",
        ExpressionDiagnosticCode::UnsafePersonalization => "X104",
        ExpressionDiagnosticCode::UnreviewedSuggestion => "X105",
        ExpressionDiagnosticCode::InvalidLink => "X106",
        ExpressionDiagnosticCode::DuplicateVariant => "X107",
        ExpressionDiagnosticCode::NearDuplicateVariant => "X108",
        ExpressionDiagnosticCode::InvalidPlaceholder => "X109",
        ExpressionDiagnosticCode::UnavailablePlaceholder => "X110",
        ExpressionDiagnosticCode::StalePack => "X111",
        ExpressionDiagnosticCode::InvalidStructure => "X112",
    }
}

/// Normalize an explicit revision without mutating the source value or any profile.
pub fn normalize_expression_revision(
    revision: &ExpressionRevision,
) -> Result<ExpressionRevision, ExpressionError> {
    if revision.revision_format_version != EXPRESSION_REVISION_FORMAT_VERSION {
        return Err(invalid_structure(
            "revision_format_version",
            "unsupported expression revision version",
        ));
    }
    let mut normalized = revision.clone();
    for mutation in &mut normalized.mutations {
        match mutation {
            ExpressionMutation::UpsertTerm { value } => {
                value.surface = collapse_whitespace(&value.surface);
                value.normalized = normalize_expression_text(&value.surface);
                normalize_applicability(&mut value.applicability)?;
                normalize_strings(&mut value.source_ids);
                normalize_optional_text(&mut value.rationale);
            }
            ExpressionMutation::UpsertPreference { value } => {
                value.target = normalize_expression_text(&value.target);
                normalize_applicability(&mut value.applicability)?;
                normalize_strings(&mut value.source_ids);
                normalize_optional_text(&mut value.rationale);
            }
            ExpressionMutation::UpsertVocabularyPool { value } => {
                normalize_strings(&mut value.term_ids);
                normalize_applicability(&mut value.applicability)?;
                normalize_strings(&mut value.source_ids);
                normalize_optional_text(&mut value.rationale);
            }
            ExpressionMutation::UpsertBehavioralSignature { value } => {
                value.cue = collapse_whitespace(&value.cue);
                normalize_applicability(&mut value.applicability)?;
                normalize_strings(&mut value.source_ids);
                normalize_optional_text(&mut value.rationale);
            }
            ExpressionMutation::UpsertVoiceConstraint { value } => {
                value.target = normalize_expression_text(&value.target);
                value.instruction = collapse_whitespace(&value.instruction);
                normalize_applicability(&mut value.applicability)?;
                normalize_strings(&mut value.source_ids);
                normalize_optional_text(&mut value.rationale);
            }
            ExpressionMutation::UpsertTemplateAssignment { value } => {
                normalize_strings(&mut value.source_ids);
                value.rationale = collapse_whitespace(&value.rationale);
            }
            ExpressionMutation::Remove { .. } => {}
        }
    }
    normalized
        .mutations
        .sort_by(|left, right| mutation_key(left).cmp(&mutation_key(right)));
    normalized.rationale = collapse_whitespace(&normalized.rationale);
    validate_expression_revision_structure(&normalized)?;
    Ok(normalized)
}

fn normalize_applicability(value: &mut ExpressionApplicability) -> Result<(), ExpressionError> {
    normalize_strings(&mut value.scenario_ids);
    let mut keyed = value
        .predicates
        .drain(..)
        .map(|predicate| Ok((canonical_json(&predicate)?, predicate)))
        .collect::<Result<Vec<_>, ExpressionError>>()?;
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    keyed.dedup_by(|left, right| left.0 == right.0);
    value.predicates = keyed.into_iter().map(|(_, predicate)| predicate).collect();
    Ok(())
}

fn normalize_strings(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn normalize_optional_text(value: &mut Option<String>) {
    if let Some(text) = value {
        *text = collapse_whitespace(text);
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn mutation_key(value: &ExpressionMutation) -> (ExpressionRecordKind, &str) {
    match value {
        ExpressionMutation::UpsertTerm { value } => (ExpressionRecordKind::Term, &value.id),
        ExpressionMutation::UpsertPreference { value } => {
            (ExpressionRecordKind::Preference, &value.id)
        }
        ExpressionMutation::UpsertVocabularyPool { value } => {
            (ExpressionRecordKind::VocabularyPool, &value.id)
        }
        ExpressionMutation::UpsertBehavioralSignature { value } => {
            (ExpressionRecordKind::BehavioralSignature, &value.id)
        }
        ExpressionMutation::UpsertVoiceConstraint { value } => {
            (ExpressionRecordKind::VoiceConstraint, &value.id)
        }
        ExpressionMutation::UpsertTemplateAssignment { value } => {
            (ExpressionRecordKind::TemplateAssignment, &value.id)
        }
        ExpressionMutation::Remove { kind, id } => (*kind, id),
    }
}

/// Validate normalized revision structure independently of ambient profile state.
pub fn validate_expression_revision_structure(
    revision: &ExpressionRevision,
) -> Result<(), ExpressionError> {
    if revision.revision_format_version != EXPRESSION_REVISION_FORMAT_VERSION {
        return Err(invalid_structure(
            "revision_format_version",
            "unsupported expression revision version",
        ));
    }
    namespaced("id", &revision.id)?;
    namespaced("character_id", &revision.character_id)?;
    sha256("expected_profile_sha256", &revision.expected_profile_sha256)?;
    text_value("rationale", &revision.rationale, 1, 2_048)?;
    validate_provenance(&revision.provenance).map_err(|_| {
        invalid_structure(
            "provenance",
            "expression revision provenance is invalid or incomplete",
        )
    })?;
    if revision.mutations.is_empty() || revision.mutations.len() > 16_384 {
        return Err(invalid_structure(
            "mutations",
            "expression revision requires bounded mutations",
        ));
    }
    let lineage = provenance_ids(&revision.provenance);
    let mut prior = None::<(ExpressionRecordKind, &str)>;
    for (index, mutation) in revision.mutations.iter().enumerate() {
        let key = mutation_key(mutation);
        if prior.is_some_and(|prior| prior >= key) {
            return Err(invalid_structure(
                "mutations",
                "expression mutations must target unique records in canonical order",
            ));
        }
        prior = Some(key);
        let path = format!("mutations[{index}]");
        match mutation {
            ExpressionMutation::UpsertTerm { value } => {
                map_contract(validate_profile_term(
                    &format!("{path}.value"),
                    &revision.character_id,
                    &value.id,
                    value,
                    &lineage,
                ))?;
            }
            ExpressionMutation::UpsertPreference { value } => {
                map_contract(validate_profile_preference(
                    &format!("{path}.value"),
                    &revision.character_id,
                    &value.id,
                    value,
                    &lineage,
                ))?;
            }
            ExpressionMutation::UpsertVocabularyPool { value } => {
                local(&format!("{path}.value.id"), &value.id)?;
                if value.character_id != revision.character_id {
                    return Err(invalid_link(
                        format!("{path}.value.character_id"),
                        "expression mutation character link is inconsistent",
                    ));
                }
                namespaced(&format!("{path}.value.category"), &value.category)?;
                validate_sorted_local_ids(
                    &format!("{path}.value.term_ids"),
                    &value.term_ids,
                    false,
                )?;
                validate_applicability(
                    &format!("{path}.value.applicability"),
                    &value.applicability,
                )?;
                validate_origin_review_expression(
                    &path,
                    value.origin,
                    value.review,
                    value.rationale.as_deref(),
                )?;
                validate_source_ids(
                    &format!("{path}.value.source_ids"),
                    &value.source_ids,
                    &lineage,
                )?;
            }
            ExpressionMutation::UpsertBehavioralSignature { value } => {
                let signatures = BehavioralSignatures {
                    character_id: revision.character_id.clone(),
                    signatures: BTreeMap::from([(value.id.clone(), value.clone())]),
                };
                map_contract(validate_behavioral_signature_data(
                    BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE,
                    &revision.character_id,
                    &signatures,
                    &lineage,
                ))?;
            }
            ExpressionMutation::UpsertVoiceConstraint { value } => {
                map_contract(validate_profile_constraint(
                    &format!("{path}.value"),
                    &revision.character_id,
                    &value.id,
                    value,
                    &lineage,
                ))?;
            }
            ExpressionMutation::UpsertTemplateAssignment { value } => {
                map_contract(validate_profile_assignment(
                    &format!("{path}.value"),
                    &revision.character_id,
                    &value.id,
                    value,
                    &lineage,
                ))?;
            }
            ExpressionMutation::Remove { id, .. } => {
                local(&format!("{path}.id"), id)?;
            }
        }
    }
    Ok(())
}

fn validate_origin_review_expression(
    path: &str,
    origin: ExpressionRecordOrigin,
    review: ReviewState,
    rationale: Option<&str>,
) -> Result<(), ExpressionError> {
    let valid = matches!(
        (origin, review),
        (
            ExpressionRecordOrigin::Authored,
            ReviewState::NotRequired | ReviewState::Accepted
        ) | (
            ExpressionRecordOrigin::Imported | ExpressionRecordOrigin::PackAssigned,
            ReviewState::Accepted
        ) | (
            ExpressionRecordOrigin::Suggested,
            ReviewState::Pending | ReviewState::Rejected
        ) | (
            ExpressionRecordOrigin::ReviewedSuggestion,
            ReviewState::Accepted
        )
    );
    if !valid || (origin != ExpressionRecordOrigin::Authored && rationale.is_none()) {
        return Err(invalid_structure(
            path,
            "expression origin, review, and rationale are inconsistent",
        ));
    }
    if let Some(rationale) = rationale {
        text_value(&format!("{path}.rationale"), rationale, 1, 2_048)?;
    }
    Ok(())
}

fn map_contract(result: Result<(), crate::CharacterError>) -> Result<(), ExpressionError> {
    result.map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "expression record violates the Character profile contract",
        )
    })
}

/// Validate a direct revision against one exact profile.
pub fn validate_expression_revision(
    profile: &CharacterProfile,
    revision: &ExpressionRevision,
) -> Result<(), ExpressionError> {
    validate_profile(profile).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "input Character profile is invalid",
        )
    })?;
    validate_expression_revision_structure(revision)?;
    if &normalize_expression_revision(revision)? != revision {
        return Err(invalid_structure(
            "revision",
            "expression revision is not in normalized canonical form",
        ));
    }
    if profile.id != revision.character_id {
        return Err(invalid_link(
            "character_id",
            "expression revision targets another Character profile",
        ));
    }
    if expression_profile_fingerprint(profile)? != revision.expected_profile_sha256 {
        return Err(stale_pack(
            "expected_profile_sha256",
            "Character profile changed after expression revision authoring",
        ));
    }
    Ok(())
}

/// Apply one normalized direct revision atomically by returning a validated clone.
pub fn apply_expression_revision(
    profile: &CharacterProfile,
    revision: &ExpressionRevision,
) -> Result<CharacterProfile, ExpressionError> {
    validate_expression_revision(profile, revision)?;
    let mut output = profile.clone();
    let revision_sha = canonical_hash(revision)?;
    let transformation_id = format!("expression_revision_{}", &revision_sha[..16]);
    output.provenance = merge_expression_provenance(
        &profile.provenance,
        &revision.provenance,
        &transformation_id,
        "Applied one fingerprinted authored expression revision without changing canonical personality evidence.",
    )?;
    let header_lineage = expression_header_lineage(
        existing_expression_header(&output),
        &revision.provenance,
        &transformation_id,
    );
    ensure_expression_extensions(&mut output, &revision.rationale, &header_lineage)?;
    for mutation in &revision.mutations {
        apply_expression_mutation(&mut output, mutation)?;
    }
    refresh_behavioral_signature_refs(&mut output)?;
    validate_profile(&output).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "expression revision produced an invalid Character profile",
        )
    })?;
    Ok(output)
}

fn existing_expression_header(profile: &CharacterProfile) -> Option<&ExtensionHeader> {
    profile
        .extensions
        .get(EXPRESSION_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::Expression(record) => Some(&record.header),
            _ => None,
        })
}

fn expression_header_lineage(
    existing: Option<&ExtensionHeader>,
    provenance: &Provenance,
    transformation_id: &str,
) -> Vec<String> {
    let mut values = existing
        .map(|header| header.lineage.clone())
        .unwrap_or_default();
    values.extend(provenance.sources.iter().map(|source| source.id.clone()));
    values.extend(
        provenance
            .transformations
            .iter()
            .map(|transformation| transformation.id.clone()),
    );
    values.push(transformation_id.to_owned());
    normalize_strings(&mut values);
    values
}

fn ensure_expression_extensions(
    profile: &mut CharacterProfile,
    rationale: &str,
    lineage: &[String],
) -> Result<(), ExpressionError> {
    match profile.extensions.get_mut(EXPRESSION_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::Expression(record)) => {
            update_expression_header(&mut record.header, rationale, lineage);
        }
        Some(_) => {
            return Err(invalid_link(
                "extensions.org.weave.character.expression",
                "expression namespace contains another extension kind",
            ));
        }
        None => {
            profile.extensions.insert(
                EXPRESSION_EXTENSION_NAMESPACE.to_owned(),
                CharacterExtension::Expression(VersionedExtension {
                    header: new_expression_header(
                        EXPRESSION_EXTENSION_NAMESPACE,
                        rationale,
                        lineage,
                    ),
                    value: ExpressionData {
                        expression_format_version: expression_format_version(),
                        character_id: profile.id.clone(),
                        lexicon: BTreeMap::new(),
                        preferences: BTreeMap::new(),
                        vocabulary_pools: BTreeMap::new(),
                        voice_constraints: BTreeMap::new(),
                        template_assignments: BTreeMap::new(),
                        behavioral_signature_refs: Vec::new(),
                        source_pack_refs: Vec::new(),
                    },
                }),
            );
        }
    }
    match profile
        .extensions
        .get_mut(BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::BehavioralSignatures(record)) => {
            update_expression_header(&mut record.header, rationale, lineage);
        }
        Some(_) => {
            return Err(invalid_link(
                "extensions.org.weave.character.behavioral_signatures",
                "behavioral-signatures namespace contains another extension kind",
            ));
        }
        None => {
            profile.extensions.insert(
                BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE.to_owned(),
                CharacterExtension::BehavioralSignatures(VersionedExtension {
                    header: new_expression_header(
                        BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE,
                        rationale,
                        lineage,
                    ),
                    value: BehavioralSignatures {
                        character_id: profile.id.clone(),
                        signatures: BTreeMap::new(),
                    },
                }),
            );
        }
    }
    Ok(())
}

fn new_expression_header(namespace: &str, rationale: &str, lineage: &[String]) -> ExtensionHeader {
    ExtensionHeader {
        namespace: namespace.to_owned(),
        extension_version: 1,
        authority: "org.weave.character.expression.workflow".to_owned(),
        rationale: rationale.to_owned(),
        state: ValueState::Reviewed,
        review: ReviewState::Accepted,
        lock: LockState::Unlocked,
        freshness: Freshness::Current,
        lineage: lineage.to_vec(),
        canonical_personality_write_back: ExtensionWriteBack::Forbidden,
    }
}

fn update_expression_header(header: &mut ExtensionHeader, rationale: &str, lineage: &[String]) {
    header.authority = "org.weave.character.expression.workflow".to_owned();
    header.rationale = rationale.to_owned();
    header.state = ValueState::Reviewed;
    header.review = ReviewState::Accepted;
    header.freshness = Freshness::Current;
    header.lineage = lineage.to_vec();
}

fn apply_expression_mutation(
    profile: &mut CharacterProfile,
    mutation: &ExpressionMutation,
) -> Result<(), ExpressionError> {
    match mutation {
        ExpressionMutation::UpsertBehavioralSignature { value } => {
            let signatures = behavioral_signatures_mut(profile)?;
            signatures
                .signatures
                .insert(value.id.clone(), value.clone());
        }
        ExpressionMutation::Remove {
            kind: ExpressionRecordKind::BehavioralSignature,
            id,
        } => {
            behavioral_signatures_mut(profile)?.signatures.remove(id);
        }
        ExpressionMutation::UpsertTerm { value } => {
            expression_data_mut(profile)?
                .lexicon
                .insert(value.id.clone(), value.clone());
        }
        ExpressionMutation::UpsertPreference { value } => {
            expression_data_mut(profile)?
                .preferences
                .insert(value.id.clone(), value.clone());
        }
        ExpressionMutation::UpsertVocabularyPool { value } => {
            expression_data_mut(profile)?
                .vocabulary_pools
                .insert(value.id.clone(), value.clone());
        }
        ExpressionMutation::UpsertVoiceConstraint { value } => {
            expression_data_mut(profile)?
                .voice_constraints
                .insert(value.id.clone(), value.clone());
        }
        ExpressionMutation::UpsertTemplateAssignment { value } => {
            let expression = expression_data_mut(profile)?;
            expression
                .template_assignments
                .insert(value.id.clone(), value.clone());
            if expression
                .source_pack_refs
                .binary_search(&value.pack)
                .is_err()
            {
                expression.source_pack_refs.push(value.pack.clone());
                expression.source_pack_refs.sort();
                expression.source_pack_refs.dedup();
            }
        }
        ExpressionMutation::Remove { kind, id } => {
            let expression = expression_data_mut(profile)?;
            match kind {
                ExpressionRecordKind::Term => {
                    expression.lexicon.remove(id);
                }
                ExpressionRecordKind::Preference => {
                    expression.preferences.remove(id);
                }
                ExpressionRecordKind::VocabularyPool => {
                    expression.vocabulary_pools.remove(id);
                }
                ExpressionRecordKind::VoiceConstraint => {
                    expression.voice_constraints.remove(id);
                }
                ExpressionRecordKind::TemplateAssignment => {
                    expression.template_assignments.remove(id);
                }
                ExpressionRecordKind::BehavioralSignature => unreachable!("handled above"),
            }
        }
    }
    Ok(())
}

fn expression_data_mut(
    profile: &mut CharacterProfile,
) -> Result<&mut ExpressionData, ExpressionError> {
    match profile.extensions.get_mut(EXPRESSION_EXTENSION_NAMESPACE) {
        Some(CharacterExtension::Expression(record)) => Ok(&mut record.value),
        _ => Err(invalid_link(
            "extensions.org.weave.character.expression",
            "expression extension is unavailable",
        )),
    }
}

fn behavioral_signatures_mut(
    profile: &mut CharacterProfile,
) -> Result<&mut BehavioralSignatures, ExpressionError> {
    match profile
        .extensions
        .get_mut(BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::BehavioralSignatures(record)) => Ok(&mut record.value),
        _ => Err(invalid_link(
            "extensions.org.weave.character.behavioral_signatures",
            "behavioral-signatures extension is unavailable",
        )),
    }
}

fn refresh_behavioral_signature_refs(
    profile: &mut CharacterProfile,
) -> Result<(), ExpressionError> {
    let refs = match profile
        .extensions
        .get(BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::BehavioralSignatures(record)) => record
            .value
            .signatures
            .keys()
            .map(|id| behavioral_signature_ref(&profile.id, id))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    expression_data_mut(profile)?.behavioral_signature_refs = refs;
    Ok(())
}

fn merge_expression_provenance(
    current: &Provenance,
    addition: &Provenance,
    transformation_id: &str,
    description: &str,
) -> Result<Provenance, ExpressionError> {
    let mut sources = current.sources.clone();
    merge_by_id(
        &mut sources,
        &addition.sources,
        |value: &ProvenanceSource| value.id.as_str(),
        "provenance.sources",
    )?;
    let mut transformations = current.transformations.clone();
    merge_by_id(
        &mut transformations,
        &addition.transformations,
        |value: &ProvenanceTransformation| value.id.as_str(),
        "provenance.transformations",
    )?;
    if transformations
        .iter()
        .any(|value| value.id == transformation_id)
    {
        return Err(invalid_link(
            "provenance.transformations",
            "expression transformation id collides with retained provenance",
        ));
    }
    let mut inputs = addition
        .sources
        .iter()
        .map(|source| source.id.clone())
        .chain(
            addition
                .transformations
                .iter()
                .map(|transformation| transformation.id.clone()),
        )
        .collect::<Vec<_>>();
    normalize_strings(&mut inputs);
    transformations.push(ProvenanceTransformation {
        id: transformation_id.to_owned(),
        inputs,
        description: description.to_owned(),
    });
    transformations.sort_by(|left, right| left.id.cmp(&right.id));
    let mut claims = current.claims.clone();
    for (path, ids) in &addition.claims {
        claims.entry(path.clone()).or_default().extend(ids.clone());
    }
    claims
        .entry("extensions.expression".to_owned())
        .or_default()
        .push(transformation_id.to_owned());
    for ids in claims.values_mut() {
        normalize_strings(ids);
    }
    let value = Provenance {
        sources,
        transformations,
        claims,
    };
    validate_provenance(&value).map_err(|_| {
        invalid_structure(
            "provenance",
            "merged expression provenance is invalid or incomplete",
        )
    })?;
    Ok(value)
}

fn merge_by_id<T: Clone + PartialEq>(
    target: &mut Vec<T>,
    incoming: &[T],
    id: impl Fn(&T) -> &str,
    path: &str,
) -> Result<(), ExpressionError> {
    for value in incoming {
        if let Some(existing) = target.iter().find(|existing| id(existing) == id(value)) {
            if existing != value {
                return Err(invalid_link(
                    path,
                    "expression provenance id has conflicting definitions",
                ));
            }
        } else {
            target.push(value.clone());
        }
    }
    target.sort_by(|left, right| id(left).cmp(id(right)));
    Ok(())
}

/// Validate an assignment request independently of the selected profile and pack.
pub fn validate_expression_assignment_request_structure(
    request: &ExpressionAssignmentRequest,
) -> Result<(), ExpressionError> {
    if request.request_format_version != EXPRESSION_ASSIGNMENT_REQUEST_FORMAT_VERSION {
        return Err(invalid_structure(
            "request_format_version",
            "unsupported expression assignment request version",
        ));
    }
    namespaced("id", &request.id)?;
    namespaced("character_id", &request.character_id)?;
    sha256("expected_profile_sha256", &request.expected_profile_sha256)?;
    validate_expression_pack_ref_value("pack", &request.pack)?;
    validate_sorted_local_ids("entry_ids", &request.entry_ids, true)?;
    validate_sorted_local_ids("vocabulary_pool_ids", &request.vocabulary_pool_ids, true)?;
    validate_sorted_local_ids("template_ids", &request.template_ids, true)?;
    if request.entry_ids.is_empty()
        && request.vocabulary_pool_ids.is_empty()
        && request.template_ids.is_empty()
    {
        return Err(invalid_structure(
            "request",
            "expression assignment must select at least one pack record",
        ));
    }
    namespaced("reviewer", &request.reviewer)?;
    text_value("rationale", &request.rationale, 1, 2_048)?;
    validate_provenance(&request.provenance).map_err(|_| {
        invalid_structure(
            "provenance",
            "expression assignment provenance is invalid or incomplete",
        )
    })
}

fn validate_expression_pack_ref_value(
    path: &str,
    value: &ExpressionPackRef,
) -> Result<(), ExpressionError> {
    namespaced(&format!("{path}.id"), &value.id)?;
    semver(&format!("{path}.version"), &value.version)?;
    sha256(&format!("{path}.sha256"), &value.sha256)
}

/// Assign exact eligible public-pack records and return a reproducible atomic receipt.
pub fn assign_expression_pack(
    profile: &CharacterProfile,
    pack: &ExpressionPack,
    request: &ExpressionAssignmentRequest,
) -> Result<ExpressionAssignmentReceipt, ExpressionError> {
    validate_profile(profile).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "input Character profile is invalid",
        )
    })?;
    validate_expression_pack(pack)?;
    validate_expression_assignment_request_structure(request)?;
    let pack_ref = expression_pack_ref(pack)?;
    if request.pack != pack_ref {
        return Err(stale_pack(
            "pack",
            "expression assignment does not pin the exact supplied pack",
        ));
    }
    let input_sha256 = expression_profile_fingerprint(profile)?;
    if request.expected_profile_sha256 != input_sha256 {
        return Err(stale_pack(
            "expected_profile_sha256",
            "Character profile changed after expression assignment authoring",
        ));
    }
    if profile.id != request.character_id {
        return Err(invalid_link(
            "character_id",
            "expression assignment targets another Character profile",
        ));
    }
    validate_pack_eligible_for_profile(profile, pack)?;
    for id in &request.entry_ids {
        if !pack.entries.contains_key(id) {
            return Err(invalid_link(
                "entry_ids",
                "expression assignment references an unavailable pack entry",
            ));
        }
    }
    for id in &request.vocabulary_pool_ids {
        let Some(pool) = pack.vocabulary_pools.get(id) else {
            return Err(invalid_link(
                "vocabulary_pool_ids",
                "expression assignment references an unavailable vocabulary pool",
            ));
        };
        if pool
            .entry_ids
            .iter()
            .any(|entry_id| request.entry_ids.binary_search(entry_id).is_err())
        {
            return Err(invalid_link(
                "vocabulary_pool_ids",
                "assigned vocabulary pool requires all referenced term entries",
            ));
        }
    }
    for id in &request.template_ids {
        if !pack.templates.contains_key(id) {
            return Err(invalid_link(
                "template_ids",
                "expression assignment references an unavailable dialogue template",
            ));
        }
    }
    let provenance = combine_expression_provenance(&pack.provenance, &request.provenance)?;
    let mut mutations = Vec::new();
    for id in &request.entry_ids {
        mutations.push(pack_entry_mutation(
            &profile.id,
            &pack.entries[id],
            &request.rationale,
        ));
    }
    for id in &request.vocabulary_pool_ids {
        let pool = &pack.vocabulary_pools[id];
        mutations.push(ExpressionMutation::UpsertVocabularyPool {
            value: ExpressionVocabularyPool {
                id: pool.id.clone(),
                character_id: profile.id.clone(),
                category: pool.category.clone(),
                term_ids: pool.entry_ids.clone(),
                applicability: pool.applicability.clone(),
                origin: ExpressionRecordOrigin::PackAssigned,
                review: ReviewState::Accepted,
                source_ids: pool.source_ids.clone(),
                rationale: Some(request.rationale.clone()),
            },
        });
    }
    for id in &request.template_ids {
        let template = &pack.templates[id];
        mutations.push(ExpressionMutation::UpsertTemplateAssignment {
            value: ExpressionTemplateAssignment {
                id: template.id.clone(),
                character_id: profile.id.clone(),
                scenario_id: template.scenario_id.clone(),
                pack: pack_ref.clone(),
                template_id: template.id.clone(),
                state: ValueState::Reviewed,
                review: ReviewState::Accepted,
                lock: LockState::Unlocked,
                source_ids: template.source_ids.clone(),
                rationale: request.rationale.clone(),
            },
        });
    }
    let revision = normalize_expression_revision(&ExpressionRevision {
        revision_format_version: EXPRESSION_REVISION_FORMAT_VERSION,
        id: format!("{}.revision", request.id),
        character_id: profile.id.clone(),
        expected_profile_sha256: input_sha256.clone(),
        mutations,
        rationale: request.rationale.clone(),
        provenance,
    })?;
    let mut output_profile = apply_expression_revision(profile, &revision)?;
    let expression = expression_data_mut(&mut output_profile)?;
    if expression
        .source_pack_refs
        .binary_search(&pack_ref)
        .is_err()
    {
        expression.source_pack_refs.push(pack_ref.clone());
        expression.source_pack_refs.sort();
        expression.source_pack_refs.dedup();
    }
    validate_profile(&output_profile).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "expression assignment produced an invalid Character profile",
        )
    })?;
    let request_sha256 = canonical_hash(request)?;
    let output_sha256 = expression_profile_fingerprint(&output_profile)?;
    let receipt = ExpressionAssignmentReceipt {
        receipt_format_version: EXPRESSION_ASSIGNMENT_RECEIPT_FORMAT_VERSION,
        id: format!("{}.receipt", request.id),
        input_profile: profile.clone(),
        input_sha256,
        pack: pack.clone(),
        pack_ref,
        request: request.clone(),
        request_sha256,
        output_profile,
        output_sha256,
    };
    validate_expression_assignment_receipt_structure(&receipt)?;
    Ok(receipt)
}

fn validate_pack_eligible_for_profile(
    profile: &CharacterProfile,
    pack: &ExpressionPack,
) -> Result<(), ExpressionError> {
    if pack
        .eligibility
        .compatible_profile_versions
        .binary_search(&profile.profile_format_version)
        .is_err()
    {
        return Err(invalid_link(
            "eligibility.compatible_profile_versions",
            "expression pack is incompatible with this Character profile version",
        ));
    }
    if pack
        .eligibility
        .required_extension_namespaces
        .iter()
        .any(|namespace| !profile.extensions.contains_key(namespace))
    {
        return Err(invalid_link(
            "eligibility.required_extension_namespaces",
            "Character profile does not satisfy expression pack eligibility",
        ));
    }
    Ok(())
}

fn pack_entry_mutation(
    character_id: &str,
    entry: &ExpressionPackEntry,
    rationale: &str,
) -> ExpressionMutation {
    match &entry.value {
        ExpressionPackValue::Term {
            category,
            term_kind,
            surface,
        } => ExpressionMutation::UpsertTerm {
            value: NormalizedExpressionTerm {
                id: entry.id.clone(),
                character_id: character_id.to_owned(),
                category: category.clone(),
                kind: *term_kind,
                surface: collapse_whitespace(surface),
                normalized: normalize_expression_text(surface),
                strength: entry.strength,
                applicability: entry.applicability.clone(),
                origin: ExpressionRecordOrigin::PackAssigned,
                review: ReviewState::Accepted,
                source_ids: entry.source_ids.clone(),
                rationale: Some(rationale.to_owned()),
            },
        },
        ExpressionPackValue::Preference {
            category,
            target,
            polarity,
        } => ExpressionMutation::UpsertPreference {
            value: NormalizedPreference {
                id: entry.id.clone(),
                character_id: character_id.to_owned(),
                category: category.clone(),
                target: normalize_expression_text(target),
                polarity: *polarity,
                strength: entry.strength,
                applicability: entry.applicability.clone(),
                origin: ExpressionRecordOrigin::PackAssigned,
                review: ReviewState::Accepted,
                source_ids: entry.source_ids.clone(),
                rationale: Some(rationale.to_owned()),
            },
        },
        ExpressionPackValue::BehavioralSignature { category, cue } => {
            ExpressionMutation::UpsertBehavioralSignature {
                value: BehavioralSignature {
                    id: entry.id.clone(),
                    character_id: character_id.to_owned(),
                    category: category.clone(),
                    cue: collapse_whitespace(cue),
                    strength: entry.strength,
                    applicability: entry.applicability.clone(),
                    origin: ExpressionRecordOrigin::PackAssigned,
                    review: ReviewState::Accepted,
                    source_ids: entry.source_ids.clone(),
                    rationale: Some(rationale.to_owned()),
                },
            }
        }
        ExpressionPackValue::VoiceConstraint {
            category,
            medium,
            effect,
            target,
            instruction,
        } => ExpressionMutation::UpsertVoiceConstraint {
            value: ExpressionVoiceConstraint {
                id: entry.id.clone(),
                character_id: character_id.to_owned(),
                category: category.clone(),
                medium: *medium,
                effect: *effect,
                target: normalize_expression_text(target),
                instruction: collapse_whitespace(instruction),
                strength: entry.strength,
                applicability: entry.applicability.clone(),
                origin: ExpressionRecordOrigin::PackAssigned,
                review: ReviewState::Accepted,
                source_ids: entry.source_ids.clone(),
                rationale: Some(rationale.to_owned()),
            },
        },
    }
}

fn combine_expression_provenance(
    pack: &Provenance,
    request: &Provenance,
) -> Result<Provenance, ExpressionError> {
    let mut sources = pack.sources.clone();
    merge_by_id(
        &mut sources,
        &request.sources,
        |value: &ProvenanceSource| value.id.as_str(),
        "provenance.sources",
    )?;
    let mut transformations = pack.transformations.clone();
    merge_by_id(
        &mut transformations,
        &request.transformations,
        |value: &ProvenanceTransformation| value.id.as_str(),
        "provenance.transformations",
    )?;
    let mut claims = pack.claims.clone();
    for (path, ids) in &request.claims {
        claims.entry(path.clone()).or_default().extend(ids.clone());
    }
    for ids in claims.values_mut() {
        normalize_strings(ids);
    }
    let value = Provenance {
        sources,
        transformations,
        claims,
    };
    validate_provenance(&value).map_err(|_| {
        invalid_structure(
            "provenance",
            "combined expression assignment provenance is invalid",
        )
    })?;
    Ok(value)
}

fn validate_expression_assignment_receipt_structure(
    receipt: &ExpressionAssignmentReceipt,
) -> Result<(), ExpressionError> {
    if receipt.receipt_format_version != EXPRESSION_ASSIGNMENT_RECEIPT_FORMAT_VERSION {
        return Err(invalid_structure(
            "receipt_format_version",
            "unsupported expression assignment receipt version",
        ));
    }
    namespaced("id", &receipt.id)?;
    sha256("input_sha256", &receipt.input_sha256)?;
    sha256("request_sha256", &receipt.request_sha256)?;
    sha256("output_sha256", &receipt.output_sha256)?;
    validate_expression_pack(&receipt.pack)?;
    validate_expression_assignment_request_structure(&receipt.request)?;
    validate_profile(&receipt.input_profile).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "expression receipt input profile is invalid",
        )
    })?;
    validate_profile(&receipt.output_profile).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "expression receipt output profile is invalid",
        )
    })?;
    if receipt.pack_ref != expression_pack_ref(&receipt.pack)?
        || receipt.request.pack != receipt.pack_ref
        || receipt.input_sha256 != expression_profile_fingerprint(&receipt.input_profile)?
        || receipt.request.expected_profile_sha256 != receipt.input_sha256
        || receipt.request_sha256 != canonical_hash(&receipt.request)?
        || receipt.output_sha256 != expression_profile_fingerprint(&receipt.output_profile)?
    {
        return Err(stale_pack(
            "receipt",
            "expression assignment receipt fingerprints are inconsistent",
        ));
    }
    Ok(())
}

/// Validate an assignment receipt by independently replaying its exact inputs.
pub fn validate_expression_assignment_receipt(
    receipt: &ExpressionAssignmentReceipt,
) -> Result<(), ExpressionError> {
    validate_expression_assignment_receipt_structure(receipt)?;
    let reproduced =
        assign_expression_pack(&receipt.input_profile, &receipt.pack, &receipt.request)?;
    if reproduced != *receipt {
        return Err(stale_pack(
            "receipt",
            "expression assignment receipt does not reproduce from embedded inputs",
        ));
    }
    Ok(())
}

/// List exact normalized expression records using one deterministic filter contract.
pub fn list_expression_records(
    profile: &CharacterProfile,
    filter: &ExpressionFilter,
) -> Result<Vec<ExpressionRecord>, ExpressionError> {
    validate_profile(profile).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "input Character profile is invalid",
        )
    })?;
    validate_expression_filter(filter)?;
    let mut records = Vec::new();
    if let Some(expression) = expression_data(profile) {
        records.extend(
            expression
                .lexicon
                .values()
                .filter(|value| {
                    filter_matches(
                        filter,
                        ExpressionRecordKind::Term,
                        Some(&value.category),
                        value.origin,
                        &value.applicability,
                    )
                })
                .cloned()
                .map(ExpressionRecord::Term),
        );
        records.extend(
            expression
                .preferences
                .values()
                .filter(|value| {
                    filter_matches(
                        filter,
                        ExpressionRecordKind::Preference,
                        Some(&value.category),
                        value.origin,
                        &value.applicability,
                    )
                })
                .cloned()
                .map(ExpressionRecord::Preference),
        );
        records.extend(
            expression
                .vocabulary_pools
                .values()
                .filter(|value| {
                    filter_matches(
                        filter,
                        ExpressionRecordKind::VocabularyPool,
                        Some(&value.category),
                        value.origin,
                        &value.applicability,
                    )
                })
                .cloned()
                .map(ExpressionRecord::VocabularyPool),
        );
        records.extend(
            expression
                .voice_constraints
                .values()
                .filter(|value| {
                    filter_matches(
                        filter,
                        ExpressionRecordKind::VoiceConstraint,
                        Some(&value.category),
                        value.origin,
                        &value.applicability,
                    )
                })
                .cloned()
                .map(ExpressionRecord::VoiceConstraint),
        );
        records.extend(
            expression
                .template_assignments
                .values()
                .filter(|_| {
                    filter.kinds.is_empty()
                        || filter
                            .kinds
                            .binary_search(&ExpressionRecordKind::TemplateAssignment)
                            .is_ok()
                })
                .filter(|_| {
                    filter.categories.is_empty()
                        || filter
                            .categories
                            .iter()
                            .any(|category| category == "org.weave.expression.dialogue")
                })
                .filter(|_| {
                    filter.origins.is_empty()
                        || filter
                            .origins
                            .binary_search(&ExpressionRecordOrigin::PackAssigned)
                            .is_ok()
                })
                .filter(|value| {
                    filter
                        .scenario_id
                        .as_ref()
                        .is_none_or(|scenario| scenario == &value.scenario_id)
                })
                .cloned()
                .map(ExpressionRecord::TemplateAssignment),
        );
    }
    if let Some(signatures) = behavioral_signatures(profile) {
        records.extend(
            signatures
                .signatures
                .values()
                .filter(|value| {
                    filter_matches(
                        filter,
                        ExpressionRecordKind::BehavioralSignature,
                        Some(&value.category),
                        value.origin,
                        &value.applicability,
                    )
                })
                .cloned()
                .map(ExpressionRecord::BehavioralSignature),
        );
    }
    records.sort_by(|left, right| expression_record_key(left).cmp(&expression_record_key(right)));
    Ok(records)
}

/// Show one exact expression record.
pub fn show_expression_record(
    profile: &CharacterProfile,
    kind: ExpressionRecordKind,
    id: &str,
) -> Result<Option<ExpressionRecord>, ExpressionError> {
    local("id", id)?;
    Ok(
        list_expression_records(profile, &ExpressionFilter::default())?
            .into_iter()
            .find(|record| expression_record_key(record) == (kind, id)),
    )
}

fn validate_expression_filter(filter: &ExpressionFilter) -> Result<(), ExpressionError> {
    if !strictly_sorted(&filter.kinds)
        || !strictly_sorted(&filter.categories)
        || !strictly_sorted(&filter.origins)
    {
        return Err(invalid_structure(
            "filter",
            "expression filters must be unique and sorted",
        ));
    }
    for category in &filter.categories {
        namespaced("filter.categories", category)?;
    }
    if let Some(scenario) = &filter.scenario_id {
        local("filter.scenario_id", scenario)?;
    }
    Ok(())
}

fn filter_matches(
    filter: &ExpressionFilter,
    kind: ExpressionRecordKind,
    category: Option<&String>,
    origin: ExpressionRecordOrigin,
    applicability: &ExpressionApplicability,
) -> bool {
    (filter.kinds.is_empty() || filter.kinds.binary_search(&kind).is_ok())
        && (filter.categories.is_empty()
            || category.is_some_and(|category| filter.categories.binary_search(category).is_ok()))
        && (filter.origins.is_empty() || filter.origins.binary_search(&origin).is_ok())
        && filter.scenario_id.as_ref().is_none_or(|scenario| {
            applicability.scenario_ids.is_empty()
                || applicability.scenario_ids.binary_search(scenario).is_ok()
        })
}

fn expression_record_key(value: &ExpressionRecord) -> (ExpressionRecordKind, &str) {
    match value {
        ExpressionRecord::Term(value) => (ExpressionRecordKind::Term, &value.id),
        ExpressionRecord::Preference(value) => (ExpressionRecordKind::Preference, &value.id),
        ExpressionRecord::VocabularyPool(value) => {
            (ExpressionRecordKind::VocabularyPool, &value.id)
        }
        ExpressionRecord::BehavioralSignature(value) => {
            (ExpressionRecordKind::BehavioralSignature, &value.id)
        }
        ExpressionRecord::VoiceConstraint(value) => {
            (ExpressionRecordKind::VoiceConstraint, &value.id)
        }
        ExpressionRecord::TemplateAssignment(value) => {
            (ExpressionRecordKind::TemplateAssignment, &value.id)
        }
    }
}

fn expression_data(profile: &CharacterProfile) -> Option<&ExpressionData> {
    profile
        .extensions
        .get(EXPRESSION_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::Expression(record) => Some(&record.value),
            _ => None,
        })
}

fn behavioral_signatures(profile: &CharacterProfile) -> Option<&BehavioralSignatures> {
    profile
        .extensions
        .get(BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE)
        .and_then(|extension| match extension {
            CharacterExtension::BehavioralSignatures(record) => Some(&record.value),
            _ => None,
        })
}

/// Return all structural, editorial, placeholder, link, and duplication diagnostics.
///
/// This function never changes or normalizes author content.
pub fn lint_expression(
    profile: &CharacterProfile,
    packs: &[ExpressionPack],
) -> Result<ExpressionLintReport, ExpressionError> {
    let profile_sha256 = expression_profile_fingerprint(profile)?;
    let mut diagnostics = Vec::new();
    if let Err(failure) = validate_profile(profile) {
        diagnostics.push(ExpressionDiagnostic {
            code: ExpressionDiagnosticCode::InvalidStructure,
            severity: ExpressionDiagnosticSeverity::Error,
            path: failure.diagnostic().path.clone(),
            message: "Character profile violates the expression-aware profile contract".to_owned(),
            source: None,
        });
    }
    let mut pack_sha256 = Vec::new();
    let mut available_packs = BTreeMap::new();
    for (index, pack) in packs.iter().enumerate() {
        let fingerprint = expression_pack_fingerprint(pack)?;
        pack_sha256.push(fingerprint.clone());
        if let Err(failure) = validate_expression_pack(pack) {
            let mut diagnostic = failure.diagnostic().clone();
            diagnostic.path = format!("packs[{index}].{}", diagnostic.path);
            diagnostics.push(diagnostic);
        }
        let pack_ref = ExpressionPackRef {
            id: pack.id.clone(),
            version: pack.version.clone(),
            sha256: fingerprint,
        };
        available_packs.insert(pack_ref, pack);
        lint_pack_templates(index, pack, &mut diagnostics);
    }
    pack_sha256.sort();
    pack_sha256.dedup();
    if let Some(expression) = expression_data(profile) {
        lint_expression_records(expression, &available_packs, &mut diagnostics);
    }
    if let Some(signatures) = behavioral_signatures(profile) {
        for (id, signature) in &signatures.signatures {
            let path = format!(
                "extensions.{BEHAVIORAL_SIGNATURES_EXTENSION_NAMESPACE}.value.signatures.{id}"
            );
            lint_text(
                &format!("{path}.cue"),
                &signature.cue,
                1,
                2_048,
                &mut diagnostics,
            );
            lint_unreviewed(&path, signature.origin, signature.review, &mut diagnostics);
        }
    }
    sort_diagnostics(&mut diagnostics);
    let report = ExpressionLintReport {
        lint_format_version: EXPRESSION_LINT_FORMAT_VERSION,
        profile_sha256,
        pack_sha256,
        diagnostics,
    };
    validate_expression_lint_report(&report)?;
    Ok(report)
}

fn lint_expression_records(
    expression: &ExpressionData,
    available_packs: &BTreeMap<ExpressionPackRef, &ExpressionPack>,
    diagnostics: &mut Vec<ExpressionDiagnostic>,
) {
    let root = format!("extensions.{EXPRESSION_EXTENSION_NAMESPACE}.value");
    for (id, term) in &expression.lexicon {
        let path = format!("{root}.lexicon.{id}");
        lint_text(
            &format!("{path}.surface"),
            &term.surface,
            1,
            512,
            diagnostics,
        );
        lint_unreviewed(&path, term.origin, term.review, diagnostics);
    }
    for (id, preference) in &expression.preferences {
        let path = format!("{root}.preferences.{id}");
        lint_text(
            &format!("{path}.target"),
            &preference.target,
            1,
            2_048,
            diagnostics,
        );
        lint_unreviewed(&path, preference.origin, preference.review, diagnostics);
    }
    let constraints = expression.voice_constraints.iter().collect::<Vec<_>>();
    for (id, constraint) in &constraints {
        let path = format!("{root}.voice_constraints.{id}");
        lint_text(
            &format!("{path}.instruction"),
            &constraint.instruction,
            1,
            2_048,
            diagnostics,
        );
        lint_unreviewed(&path, constraint.origin, constraint.review, diagnostics);
    }
    for left_index in 0..constraints.len() {
        for right_index in left_index + 1..constraints.len() {
            let (left_id, left) = constraints[left_index];
            let (_, right) = constraints[right_index];
            if left.target == right.target
                && media_overlap(left.medium, right.medium)
                && left.effect != right.effect
                && applicability_overlap(&left.applicability, &right.applicability)
            {
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::ConflictingConstraint,
                    severity: ExpressionDiagnosticSeverity::Error,
                    path: format!("{root}.voice_constraints.{left_id}"),
                    message: "overlapping voice constraints prefer and avoid the same target"
                        .to_owned(),
                    source: None,
                });
            }
        }
    }
    for (id, pool) in &expression.vocabulary_pools {
        if pool
            .term_ids
            .iter()
            .any(|term_id| !expression.lexicon.contains_key(term_id))
        {
            diagnostics.push(ExpressionDiagnostic {
                code: ExpressionDiagnosticCode::InvalidLink,
                severity: ExpressionDiagnosticSeverity::Error,
                path: format!("{root}.vocabulary_pools.{id}.term_ids"),
                message: "vocabulary pool references an unavailable normalized term".to_owned(),
                source: None,
            });
        }
    }
    for (id, assignment) in &expression.template_assignments {
        let path = format!("{root}.template_assignments.{id}");
        match available_packs.get(&assignment.pack) {
            Some(pack) if pack.templates.contains_key(&assignment.template_id) => {}
            Some(_) => diagnostics.push(ExpressionDiagnostic {
                code: ExpressionDiagnosticCode::InvalidLink,
                severity: ExpressionDiagnosticSeverity::Error,
                path: format!("{path}.template_id"),
                message: "template assignment references an unavailable template".to_owned(),
                source: None,
            }),
            None => diagnostics.push(ExpressionDiagnostic {
                code: ExpressionDiagnosticCode::StalePack,
                severity: ExpressionDiagnosticSeverity::Error,
                path: format!("{path}.pack"),
                message: "template assignment pack is missing or has a different fingerprint"
                    .to_owned(),
                source: None,
            }),
        }
    }
}

fn lint_pack_templates(
    pack_index: usize,
    pack: &ExpressionPack,
    diagnostics: &mut Vec<ExpressionDiagnostic>,
) {
    for (template_id, template) in &pack.templates {
        let base = format!("packs[{pack_index}].templates.{template_id}");
        for (placeholder_id, declaration) in &template.placeholders {
            if restricted_placeholder_name(placeholder_id) {
                let source = template.variants.values().find_map(|variant| {
                    parse_placeholder_tokens(
                        &format!("{base}.variants.{}.content", variant.id),
                        &variant.content,
                        &variant.source,
                    )
                    .ok()?
                    .into_iter()
                    .find(|token| token.id == *placeholder_id)
                    .map(|token| token.source)
                });
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::UnsafePersonalization,
                    severity: ExpressionDiagnosticSeverity::Error,
                    path: format!("{base}.placeholders.{placeholder_id}"),
                    message:
                        "placeholder name is reserved because it could imply sensitive runtime data"
                            .to_owned(),
                    source,
                });
            }
            if declaration.id != *placeholder_id {
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::InvalidLink,
                    severity: ExpressionDiagnosticSeverity::Error,
                    path: format!("{base}.placeholders.{placeholder_id}.id"),
                    message: "placeholder id does not match its containing key".to_owned(),
                    source: None,
                });
            }
        }
        let variants = template.variants.iter().collect::<Vec<_>>();
        for (variant_id, variant) in &variants {
            let path = format!("{base}.variants.{variant_id}");
            lint_text(
                &format!("{path}.content"),
                &variant.content,
                1,
                8_192,
                diagnostics,
            );
            if variant.authority == ExpressionTextAuthority::Suggested
                && variant.review != ReviewState::Rejected
            {
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::UnreviewedSuggestion,
                    severity: ExpressionDiagnosticSeverity::Warning,
                    path: format!("{path}.review"),
                    message: "unreviewed dialogue suggestion is excluded from runtime selection"
                        .to_owned(),
                    source: Some(variant.source.clone()),
                });
            }
            match parse_placeholder_tokens(
                &format!("{path}.content"),
                &variant.content,
                &variant.source,
            ) {
                Ok(tokens) => {
                    for token in tokens {
                        if !template.placeholders.contains_key(&token.id) {
                            diagnostics.push(ExpressionDiagnostic {
                                code: ExpressionDiagnosticCode::UnresolvedToken,
                                severity: ExpressionDiagnosticSeverity::Error,
                                path: format!("{path}.content"),
                                message: "dialogue token is outside its placeholder allowlist"
                                    .to_owned(),
                                source: Some(token.source),
                            });
                        }
                    }
                }
                Err(failure) => diagnostics.push(failure.diagnostic().clone()),
            }
        }
        for left_index in 0..variants.len() {
            for right_index in left_index + 1..variants.len() {
                let (left_id, left) = variants[left_index];
                let (_, right) = variants[right_index];
                let left_normalized = normalize_for_similarity(&left.content);
                let right_normalized = normalize_for_similarity(&right.content);
                if left_normalized == right_normalized {
                    diagnostics.push(ExpressionDiagnostic {
                        code: ExpressionDiagnosticCode::DuplicateVariant,
                        severity: ExpressionDiagnosticSeverity::Error,
                        path: format!("{base}.variants.{left_id}.content"),
                        message: "dialogue template contains duplicate normalized variants"
                            .to_owned(),
                        source: Some(left.source.clone()),
                    });
                } else if similarity_micros(&left_normalized, &right_normalized) >= 800_000 {
                    diagnostics.push(ExpressionDiagnostic {
                        code: ExpressionDiagnosticCode::NearDuplicateVariant,
                        severity: ExpressionDiagnosticSeverity::Warning,
                        path: format!("{base}.variants.{left_id}.content"),
                        message: "dialogue template contains near-duplicate variants".to_owned(),
                        source: Some(left.source.clone()),
                    });
                }
            }
        }
    }
}

fn lint_unreviewed(
    path: &str,
    origin: ExpressionRecordOrigin,
    review: ReviewState,
    diagnostics: &mut Vec<ExpressionDiagnostic>,
) {
    if origin == ExpressionRecordOrigin::Suggested && review != ReviewState::Rejected {
        diagnostics.push(ExpressionDiagnostic {
            code: ExpressionDiagnosticCode::UnreviewedSuggestion,
            severity: ExpressionDiagnosticSeverity::Warning,
            path: format!("{path}.review"),
            message: "unreviewed expression suggestion is excluded from runtime selection"
                .to_owned(),
            source: None,
        });
    }
}

fn lint_text(
    path: &str,
    value: &str,
    min: usize,
    max: usize,
    diagnostics: &mut Vec<ExpressionDiagnostic>,
) {
    let length = value.chars().count();
    if length < min {
        diagnostics.push(ExpressionDiagnostic {
            code: ExpressionDiagnosticCode::EmptyText,
            severity: ExpressionDiagnosticSeverity::Error,
            path: path.to_owned(),
            message: "expression text must not be empty".to_owned(),
            source: None,
        });
    } else if length > max {
        diagnostics.push(ExpressionDiagnostic {
            code: ExpressionDiagnosticCode::TextLimit,
            severity: ExpressionDiagnosticSeverity::Error,
            path: path.to_owned(),
            message: "expression text is outside its documented limit".to_owned(),
            source: None,
        });
    }
}

fn media_overlap(left: ExpressionMedium, right: ExpressionMedium) -> bool {
    left == right || left == ExpressionMedium::Both || right == ExpressionMedium::Both
}

fn applicability_overlap(left: &ExpressionApplicability, right: &ExpressionApplicability) -> bool {
    left == right
        || (left.scenario_ids.is_empty() && left.predicates.is_empty())
        || (right.scenario_ids.is_empty() && right.predicates.is_empty())
}

fn normalize_for_similarity(value: &str) -> BTreeSet<String> {
    normalize_expression_text(value)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn similarity_micros(left: &BTreeSet<String>, right: &BTreeSet<String>) -> u32 {
    let union = left.union(right).count();
    if union == 0 {
        return 0;
    }
    let intersection = left.intersection(right).count();
    ((intersection as u64 * 1_000_000) / union as u64) as u32
}

fn sort_diagnostics(values: &mut Vec<ExpressionDiagnostic>) {
    values.sort_by(|left, right| {
        (&left.path, left.code, &left.message).cmp(&(&right.path, right.code, &right.message))
    });
    values.dedup();
}

fn validate_expression_lint_report(report: &ExpressionLintReport) -> Result<(), ExpressionError> {
    if report.lint_format_version != EXPRESSION_LINT_FORMAT_VERSION {
        return Err(invalid_structure(
            "lint_format_version",
            "unsupported expression lint report version",
        ));
    }
    sha256("profile_sha256", &report.profile_sha256)?;
    if !strictly_sorted(&report.pack_sha256) && report.pack_sha256.len() > 1 {
        return Err(invalid_structure(
            "pack_sha256",
            "expression lint pack fingerprints must be unique and sorted",
        ));
    }
    for fingerprint in &report.pack_sha256 {
        sha256("pack_sha256", fingerprint)?;
    }
    validate_expression_diagnostics(&report.diagnostics)
}

fn validate_expression_diagnostics(values: &[ExpressionDiagnostic]) -> Result<(), ExpressionError> {
    let mut prior = None::<(&str, ExpressionDiagnosticCode, &str)>;
    for diagnostic in values {
        text_value("diagnostics.path", &diagnostic.path, 1, 2_048)?;
        text_value("diagnostics.message", &diagnostic.message, 1, 2_048)?;
        if let Some(source) = &diagnostic.source {
            validate_source_location("diagnostics.source", source)?;
        }
        let key = (
            diagnostic.path.as_str(),
            diagnostic.code,
            diagnostic.message.as_str(),
        );
        if prior.is_some_and(|prior| prior >= key) {
            return Err(invalid_structure(
                "diagnostics",
                "expression diagnostics must be unique and canonically sorted",
            ));
        }
        prior = Some(key);
    }
    Ok(())
}

/// Validate expression data and exact packs, failing on the first blocking lint.
pub fn validate_expression_profile(
    profile: &CharacterProfile,
    packs: &[ExpressionPack],
) -> Result<(), ExpressionError> {
    let report = lint_expression(profile, packs)?;
    report
        .diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.severity == ExpressionDiagnosticSeverity::Error)
        .map_or(Ok(()), |diagnostic| Err(ExpressionError { diagnostic }))
}

/// Inspect complete record, category, and scenario coverage without changing profile state.
pub fn inspect_expression_coverage(
    profile: &CharacterProfile,
    packs: &[ExpressionPack],
) -> Result<ExpressionCoverageReport, ExpressionError> {
    let lint = lint_expression(profile, packs)?;
    let records = list_expression_records(profile, &ExpressionFilter::default())?;
    let mut record_counts = BTreeMap::new();
    let mut category_counts = BTreeMap::new();
    for record in &records {
        *record_counts
            .entry(expression_record_key(record).0)
            .or_insert(0) += 1;
        if let Some(category) = expression_record_category(record) {
            *category_counts.entry(category.to_owned()).or_insert(0) += 1;
        }
    }
    for kind in [
        ExpressionRecordKind::Term,
        ExpressionRecordKind::Preference,
        ExpressionRecordKind::VocabularyPool,
        ExpressionRecordKind::BehavioralSignature,
        ExpressionRecordKind::VoiceConstraint,
        ExpressionRecordKind::TemplateAssignment,
    ] {
        record_counts.entry(kind).or_insert(0);
    }
    let mut scenarios = BTreeSet::new();
    for pack in packs {
        scenarios.extend(pack.coverage.required_scenario_ids.iter().cloned());
    }
    if let Some(expression) = expression_data(profile) {
        scenarios.extend(
            expression
                .template_assignments
                .values()
                .map(|assignment| assignment.scenario_id.clone()),
        );
    }
    let packs_by_ref = packs
        .iter()
        .filter_map(|pack| expression_pack_ref(pack).ok().map(|value| (value, pack)))
        .collect::<BTreeMap<_, _>>();
    let mut scenario_coverage = BTreeMap::new();
    for scenario_id in scenarios {
        let assignments = expression_data(profile)
            .into_iter()
            .flat_map(|expression| expression.template_assignments.values())
            .filter(|assignment| assignment.scenario_id == scenario_id)
            .collect::<Vec<_>>();
        let mut assigned_template_ids = assignments
            .iter()
            .map(|assignment| assignment.template_id.clone())
            .collect::<Vec<_>>();
        normalize_strings(&mut assigned_template_ids);
        let templates = assignments
            .iter()
            .filter_map(|assignment| {
                packs_by_ref
                    .get(&assignment.pack)
                    .and_then(|pack| pack.templates.get(&assignment.template_id))
            })
            .collect::<Vec<_>>();
        let has_fallback = templates.iter().any(|template| {
            template
                .variants
                .contains_key(&template.fallback_variant_id)
        });
        let reviewed_variant_count = templates
            .iter()
            .flat_map(|template| template.variants.values())
            .filter(|variant| {
                variant.authority != ExpressionTextAuthority::Suggested
                    && matches!(
                        variant.review,
                        ReviewState::Accepted | ReviewState::NotRequired
                    )
            })
            .count() as u32;
        scenario_coverage.insert(
            scenario_id.clone(),
            ExpressionScenarioCoverage {
                scenario_id,
                assigned_template_ids,
                has_fallback,
                reviewed_variant_count,
            },
        );
    }
    let mut diagnostics = lint.diagnostics;
    for (pack_index, pack) in packs.iter().enumerate() {
        for (kind, minimum, field) in [
            (
                ExpressionRecordKind::Term,
                pack.coverage.minimum_terms,
                "minimum_terms",
            ),
            (
                ExpressionRecordKind::Preference,
                pack.coverage.minimum_preferences,
                "minimum_preferences",
            ),
            (
                ExpressionRecordKind::BehavioralSignature,
                pack.coverage.minimum_behavioral_signatures,
                "minimum_behavioral_signatures",
            ),
            (
                ExpressionRecordKind::VoiceConstraint,
                pack.coverage.minimum_voice_constraints,
                "minimum_voice_constraints",
            ),
        ] {
            if record_counts.get(&kind).copied().unwrap_or_default() < minimum {
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::InvalidLink,
                    severity: ExpressionDiagnosticSeverity::Error,
                    path: format!("packs[{pack_index}].coverage.{field}"),
                    message: "Character expression data does not meet the pack coverage minimum"
                        .to_owned(),
                    source: None,
                });
            }
        }
        for category in &pack.coverage.required_categories {
            if category_counts.get(category).copied().unwrap_or_default() == 0 {
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::InvalidLink,
                    severity: ExpressionDiagnosticSeverity::Error,
                    path: format!("packs[{pack_index}].coverage.required_categories.{category}"),
                    message: "Character expression data omits a required pack category".to_owned(),
                    source: None,
                });
            }
        }
        for scenario_id in &pack.coverage.required_scenario_ids {
            let coverage = scenario_coverage.get(scenario_id);
            if coverage.is_none_or(|coverage| coverage.assigned_template_ids.is_empty()) {
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::InvalidLink,
                    severity: ExpressionDiagnosticSeverity::Error,
                    path: format!(
                        "packs[{pack_index}].coverage.required_scenario_ids.{scenario_id}"
                    ),
                    message: "Character expression data omits a required dialogue scenario"
                        .to_owned(),
                    source: None,
                });
            } else if coverage.is_some_and(|coverage| !coverage.has_fallback) {
                diagnostics.push(ExpressionDiagnostic {
                    code: ExpressionDiagnosticCode::InvalidStructure,
                    severity: ExpressionDiagnosticSeverity::Error,
                    path: format!("scenario_coverage.{scenario_id}.has_fallback"),
                    message: "Assigned dialogue scenario has no stable fallback".to_owned(),
                    source: None,
                });
            }
        }
    }
    sort_diagnostics(&mut diagnostics);
    let report = ExpressionCoverageReport {
        coverage_format_version: EXPRESSION_COVERAGE_FORMAT_VERSION,
        profile_sha256: lint.profile_sha256,
        record_counts,
        category_counts,
        scenario_coverage,
        diagnostics,
    };
    validate_expression_coverage_report(&report)?;
    Ok(report)
}

fn expression_record_category(value: &ExpressionRecord) -> Option<&str> {
    match value {
        ExpressionRecord::Term(value) => Some(&value.category),
        ExpressionRecord::Preference(value) => Some(&value.category),
        ExpressionRecord::VocabularyPool(value) => Some(&value.category),
        ExpressionRecord::BehavioralSignature(value) => Some(&value.category),
        ExpressionRecord::VoiceConstraint(value) => Some(&value.category),
        ExpressionRecord::TemplateAssignment(_) => Some("org.weave.expression.dialogue"),
    }
}

fn validate_expression_coverage_report(
    report: &ExpressionCoverageReport,
) -> Result<(), ExpressionError> {
    if report.coverage_format_version != EXPRESSION_COVERAGE_FORMAT_VERSION {
        return Err(invalid_structure(
            "coverage_format_version",
            "unsupported expression coverage report version",
        ));
    }
    sha256("profile_sha256", &report.profile_sha256)?;
    for category in report.category_counts.keys() {
        namespaced("category_counts", category)?;
    }
    for (scenario_id, coverage) in &report.scenario_coverage {
        local("scenario_coverage", scenario_id)?;
        if coverage.scenario_id != *scenario_id {
            return Err(invalid_link(
                "scenario_coverage.scenario_id",
                "scenario coverage id must equal its containing map key",
            ));
        }
        validate_sorted_local_ids(
            "scenario_coverage.assigned_template_ids",
            &coverage.assigned_template_ids,
            true,
        )?;
    }
    validate_expression_diagnostics(&report.diagnostics)
}

/// Validate one deterministic dialogue request independently of profile and pack state.
pub fn validate_expression_resolution_request(
    request: &ExpressionResolutionRequest,
) -> Result<(), ExpressionError> {
    if request.request_format_version != EXPRESSION_RESOLUTION_REQUEST_FORMAT_VERSION {
        return Err(invalid_structure(
            "request_format_version",
            "unsupported expression resolution request version",
        ));
    }
    namespaced("id", &request.id)?;
    local("assignment_id", &request.assignment_id)?;
    local("scenario_id", &request.scenario_id)?;
    namespaced("speaker_character_id", &request.speaker_character_id)?;
    if let Some(listener) = &request.listener {
        namespaced("listener.character_id", &listener.character_id)?;
        text_value("listener.display_name", &listener.display_name, 1, 256)?;
        if listener.character_id == request.speaker_character_id {
            return Err(invalid_link(
                "listener.character_id",
                "dialogue listener must differ from the speaker",
            ));
        }
    }
    validate_sorted_namespaced_ids(
        "relationship_kind_ids",
        &request.relationship_kind_ids,
        true,
    )?;
    validate_sorted_local_ids("date_context_ids", &request.date_context_ids, true)?;
    validate_sorted_local_ids("world_context_tags", &request.world_context_tags, true)?;
    if let Some(value) = &request.date_label {
        text_value("date_label", value, 1, 256)?;
    }
    if let Some(value) = &request.world_place_name {
        text_value("world_place_name", value, 1, 256)?;
    }
    Ok(())
}

/// Resolve one assigned dialogue template entirely offline and byte-stably.
pub fn resolve_expression_dialogue(
    profile: &CharacterProfile,
    pack: &ExpressionPack,
    request: &ExpressionResolutionRequest,
) -> Result<ExpressionResolution, ExpressionError> {
    validate_profile(profile).map_err(|failure| {
        invalid_structure(
            failure.diagnostic().path.clone(),
            "input Character profile is invalid",
        )
    })?;
    validate_expression_pack(pack)?;
    validate_expression_resolution_request(request)?;
    if profile.id != request.speaker_character_id {
        return Err(invalid_link(
            "speaker_character_id",
            "dialogue request speaker differs from the supplied Character profile",
        ));
    }
    let expression = expression_data(profile).ok_or_else(|| {
        invalid_link(
            "extensions.org.weave.character.expression",
            "Character profile has no expression extension",
        )
    })?;
    let assignment = expression
        .template_assignments
        .get(&request.assignment_id)
        .ok_or_else(|| {
            invalid_link(
                "assignment_id",
                "dialogue request references an unavailable template assignment",
            )
        })?;
    let pack_ref = expression_pack_ref(pack)?;
    if assignment.pack != pack_ref {
        return Err(stale_pack(
            "assignment.pack",
            "dialogue assignment does not pin the supplied expression pack",
        ));
    }
    let template = pack.templates.get(&assignment.template_id).ok_or_else(|| {
        invalid_link(
            "assignment.template_id",
            "dialogue assignment references an unavailable template",
        )
    })?;
    if assignment.scenario_id != request.scenario_id || template.scenario_id != request.scenario_id
    {
        return Err(invalid_link(
            "scenario_id",
            "dialogue request, assignment, and template scenarios differ",
        ));
    }
    match &template.speaker_requirement {
        ExpressionSpeakerRequirement::AssignedCharacter => {}
        ExpressionSpeakerRequirement::ExactCharacter { character_id }
            if character_id == &profile.id => {}
        ExpressionSpeakerRequirement::ExactCharacter { .. } => {
            return Err(invalid_link(
                "speaker_requirement",
                "Character profile does not satisfy the exact template speaker requirement",
            ));
        }
    }
    let mut trace = Vec::with_capacity(template.variants.len());
    let mut eligible = Vec::<(&ExpressionDialogueVariant, String)>::new();
    for variant in template.variants.values() {
        let applicable = applicability_matches(profile, request, &variant.applicability);
        let placeholders_available =
            variant_placeholders(profile, request, expression, template, variant)?.is_some();
        let reviewed = variant.authority != ExpressionTextAuthority::Suggested
            && matches!(
                variant.review,
                ReviewState::Accepted | ReviewState::NotRequired
            );
        let seeded_sha256 = seeded_variant_hash(request, template, variant)?;
        let reason = if !reviewed {
            "unreviewed suggestion excluded"
        } else if !applicable {
            "context predicates did not match"
        } else if !placeholders_available {
            "declared placeholder value unavailable"
        } else if variant.id == template.fallback_variant_id {
            "stable fallback retained"
        } else {
            "eligible authored or reviewed variant"
        };
        trace.push(ExpressionVariantTrace {
            variant_id: variant.id.clone(),
            authority: variant.authority,
            priority: variant.priority,
            applicable,
            placeholders_available,
            seeded_sha256: seeded_sha256.clone(),
            reason: reason.to_owned(),
        });
        if reviewed
            && applicable
            && placeholders_available
            && variant.id != template.fallback_variant_id
        {
            eligible.push((variant, seeded_sha256));
        }
    }
    eligible.sort_by(|(left, left_seed), (right, right_seed)| {
        right
            .authority
            .precedence()
            .cmp(&left.authority.precedence())
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left_seed.cmp(right_seed))
            .then_with(|| left.id.cmp(&right.id))
    });
    let (selected, fallback_used) = if let Some((variant, _)) = eligible.first() {
        (*variant, false)
    } else {
        (&template.variants[&template.fallback_variant_id], true)
    };
    let substitutions = variant_placeholders(profile, request, expression, template, selected)?
        .ok_or_else(|| {
            expression_error(
                ExpressionDiagnosticCode::UnavailablePlaceholder,
                ExpressionDiagnosticSeverity::Error,
                format!("templates.{}.variants.{}.content", template.id, selected.id),
                "selected dialogue variant requires unavailable typed runtime context",
                Some(selected.source.clone()),
            )
        })?;
    let rendered_text = render_variant(selected, &substitutions)?;
    trace.sort_by(|left, right| left.variant_id.cmp(&right.variant_id));
    let request_sha256 = canonical_hash(request)?;
    let resolution = ExpressionResolution {
        resolution_format_version: EXPRESSION_RESOLUTION_FORMAT_VERSION,
        id: format!("{}.resolution", request.id),
        profile_sha256: expression_profile_fingerprint(profile)?,
        pack: pack_ref,
        request: request.clone(),
        request_sha256,
        template_id: template.id.clone(),
        selected_variant_id: selected.id.clone(),
        fallback_used,
        rendered_text,
        substitutions,
        trace,
    };
    validate_expression_resolution(&resolution)?;
    Ok(resolution)
}

fn applicability_matches(
    profile: &CharacterProfile,
    request: &ExpressionResolutionRequest,
    applicability: &ExpressionApplicability,
) -> bool {
    if !applicability.scenario_ids.is_empty()
        && applicability
            .scenario_ids
            .binary_search(&request.scenario_id)
            .is_err()
    {
        return false;
    }
    applicability
        .predicates
        .iter()
        .all(|predicate| match predicate {
            ExpressionContextPredicate::PersonalityBand { trait_id, bands } => {
                trait_value(&profile.canon.personality, *trait_id)
                    .map(|value| measurement_band(value.value))
                    .is_some_and(|band| bands.contains(&band))
            }
            ExpressionContextPredicate::Relationship {
                relationship_kind_id,
                other_character_id,
            } => {
                request
                    .relationship_kind_ids
                    .binary_search(relationship_kind_id)
                    .is_ok()
                    && other_character_id.as_ref().is_none_or(|expected| {
                        request
                            .listener
                            .as_ref()
                            .is_some_and(|listener| &listener.character_id == expected)
                    })
            }
            ExpressionContextPredicate::DateContext { cue_id } => {
                request.date_context_ids.binary_search(cue_id).is_ok()
            }
            ExpressionContextPredicate::WorldContext { tag } => {
                request.world_context_tags.binary_search(tag).is_ok()
            }
        })
}

fn measurement_band(value: TraitMeasurement) -> TraitBand {
    match value {
        TraitMeasurement::Band { band } => band,
        TraitMeasurement::Score { score } if score < 0.2 => TraitBand::VeryLow,
        TraitMeasurement::Score { score } if score < 0.4 => TraitBand::Low,
        TraitMeasurement::Score { score } if score < 0.6 => TraitBand::Middle,
        TraitMeasurement::Score { score } if score < 0.8 => TraitBand::High,
        TraitMeasurement::Score { .. } => TraitBand::VeryHigh,
    }
}

fn variant_placeholders(
    profile: &CharacterProfile,
    request: &ExpressionResolutionRequest,
    expression: &ExpressionData,
    template: &ExpressionDialogueTemplate,
    variant: &ExpressionDialogueVariant,
) -> Result<Option<BTreeMap<String, String>>, ExpressionError> {
    let tokens = parse_placeholder_tokens(
        &format!("templates.{}.variants.{}.content", template.id, variant.id),
        &variant.content,
        &variant.source,
    )?;
    let mut required_ids = tokens
        .iter()
        .map(|token| token.id.clone())
        .collect::<BTreeSet<_>>();
    required_ids.extend(
        template
            .placeholders
            .values()
            .filter(|declaration| declaration.required)
            .map(|declaration| declaration.id.clone()),
    );
    let mut substitutions = BTreeMap::new();
    for id in required_ids {
        let Some(declaration) = template.placeholders.get(&id) else {
            return Err(expression_error(
                ExpressionDiagnosticCode::UnresolvedToken,
                ExpressionDiagnosticSeverity::Error,
                format!("templates.{}.variants.{}.content", template.id, variant.id),
                "dialogue token is outside its declared placeholder allowlist",
                tokens
                    .iter()
                    .find(|token| token.id == id)
                    .map(|token| token.source.clone()),
            ));
        };
        let Some(value) = placeholder_value(profile, request, expression, &declaration.value)
        else {
            return Ok(None);
        };
        substitutions.insert(id, value);
    }
    Ok(Some(substitutions))
}

fn placeholder_value(
    profile: &CharacterProfile,
    request: &ExpressionResolutionRequest,
    expression: &ExpressionData,
    value: &ExpressionPlaceholderValue,
) -> Option<String> {
    match value {
        ExpressionPlaceholderValue::SpeakerDisplayName => {
            Some(profile.canon.identity.display_name.value.clone())
        }
        ExpressionPlaceholderValue::SpeakerPronounSubject => {
            profile
                .extensions
                .values()
                .find_map(|extension| match extension {
                    CharacterExtension::IdentityPresentation(record) => record
                        .value
                        .pronouns
                        .as_ref()
                        .map(|pronouns| pronouns.value.subject.clone()),
                    _ => None,
                })
        }
        ExpressionPlaceholderValue::ListenerDisplayName => request
            .listener
            .as_ref()
            .map(|listener| listener.display_name.clone()),
        ExpressionPlaceholderValue::RelationshipKind => {
            request.relationship_kind_ids.first().cloned()
        }
        ExpressionPlaceholderValue::DateLabel => request.date_label.clone(),
        ExpressionPlaceholderValue::WorldPlaceName => request.world_place_name.clone(),
        ExpressionPlaceholderValue::LexiconTerm { term_id } => expression
            .lexicon
            .get(term_id)
            .filter(|term| {
                term.origin != ExpressionRecordOrigin::Suggested
                    && matches!(
                        term.review,
                        ReviewState::Accepted | ReviewState::NotRequired
                    )
                    && applicability_matches(profile, request, &term.applicability)
            })
            .map(|term| term.surface.clone()),
    }
}

fn seeded_variant_hash(
    request: &ExpressionResolutionRequest,
    template: &ExpressionDialogueTemplate,
    variant: &ExpressionDialogueVariant,
) -> Result<String, ExpressionError> {
    canonical_hash(&(
        request.seed,
        &request.scenario_id,
        &request.speaker_character_id,
        request
            .listener
            .as_ref()
            .map(|listener| &listener.character_id),
        &request.relationship_kind_ids,
        &request.date_context_ids,
        &request.world_context_tags,
        &template.id,
        &variant.id,
    ))
}

fn render_variant(
    variant: &ExpressionDialogueVariant,
    substitutions: &BTreeMap<String, String>,
) -> Result<String, ExpressionError> {
    let tokens = parse_placeholder_tokens(
        "selected_variant.content",
        &variant.content,
        &variant.source,
    )?;
    let mut output = String::with_capacity(variant.content.len());
    let mut cursor = 0usize;
    for token in tokens {
        let marker = format!("{{{{{}}}}}", token.id);
        let Some(relative) = variant.content[cursor..].find(&marker) else {
            return Err(invalid_structure(
                "selected_variant.content",
                "validated placeholder token could not be replayed",
            ));
        };
        let start = cursor + relative;
        output.push_str(&variant.content[cursor..start]);
        let value = substitutions.get(&token.id).ok_or_else(|| {
            expression_error(
                ExpressionDiagnosticCode::UnavailablePlaceholder,
                ExpressionDiagnosticSeverity::Error,
                "selected_variant.content",
                "selected dialogue placeholder has no typed runtime value",
                Some(token.source),
            )
        })?;
        output.push_str(value);
        cursor = start + marker.len();
    }
    output.push_str(&variant.content[cursor..]);
    Ok(output)
}

fn validate_expression_resolution(value: &ExpressionResolution) -> Result<(), ExpressionError> {
    if value.resolution_format_version != EXPRESSION_RESOLUTION_FORMAT_VERSION {
        return Err(invalid_structure(
            "resolution_format_version",
            "unsupported expression resolution version",
        ));
    }
    namespaced("id", &value.id)?;
    sha256("profile_sha256", &value.profile_sha256)?;
    validate_expression_pack_ref_value("pack", &value.pack)?;
    validate_expression_resolution_request(&value.request)?;
    sha256("request_sha256", &value.request_sha256)?;
    if value.request_sha256 != canonical_hash(&value.request)? {
        return Err(stale_pack(
            "request_sha256",
            "expression resolution request fingerprint is inconsistent",
        ));
    }
    local("template_id", &value.template_id)?;
    local("selected_variant_id", &value.selected_variant_id)?;
    text_value("rendered_text", &value.rendered_text, 1, 8_192)?;
    for (id, substitution) in &value.substitutions {
        local("substitutions", id)?;
        text_value("substitutions.value", substitution, 1, 2_048)?;
    }
    let mut prior = None::<&str>;
    for trace in &value.trace {
        local("trace.variant_id", &trace.variant_id)?;
        sha256("trace.seeded_sha256", &trace.seeded_sha256)?;
        text_value("trace.reason", &trace.reason, 1, 512)?;
        if prior.is_some_and(|prior| prior >= trace.variant_id.as_str()) {
            return Err(invalid_structure(
                "trace",
                "expression variant trace must be unique and sorted",
            ));
        }
        prior = Some(&trace.variant_id);
    }
    if !value
        .trace
        .iter()
        .any(|trace| trace.variant_id == value.selected_variant_id)
    {
        return Err(invalid_link(
            "selected_variant_id",
            "selected dialogue variant is absent from the resolution trace",
        ));
    }
    Ok(())
}
