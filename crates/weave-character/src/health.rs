//! Read-only Character corpus health, drift, safety, coverage, and portability reporting.
//!
//! The health contract is intentionally manifest-driven. Source documents remain separate files,
//! so an audit can diagnose malformed input without embedding author text or credentials in its
//! report. Every emitted explanation and remediation is fixed, redaction-safe text.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    DomainPack, Provenance, parse_strict_json, to_pretty_json, to_pretty_ron, validate_provenance,
};

use crate::validation::{
    ALL_HEXACO_TRAITS, trait_path, trait_value, validate_local_id, validate_namespaced_id,
    validate_relative_path, validate_text,
};
use crate::*;

/// Current health-project manifest format.
pub const CHARACTER_HEALTH_MANIFEST_FORMAT_VERSION: u32 = 1;
/// Current stable health-report format.
pub const CHARACTER_HEALTH_REPORT_FORMAT_VERSION: u32 = 1;
/// Current health policy format.
pub const CHARACTER_HEALTH_POLICY_FORMAT_VERSION: u32 = 1;
/// Current reviewable suppression format.
pub const CHARACTER_HEALTH_SUPPRESSION_FORMAT_VERSION: u32 = 1;
/// Maximum source references in one bounded health project.
pub const CHARACTER_HEALTH_MAX_DOCUMENTS: usize = 4_096;
/// Maximum UTF-8 bytes in one loaded health source.
pub const CHARACTER_HEALTH_MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

const MANIFEST_SCHEMA_ID: &str = "urn:weave:schema:character-health-manifest:1";
const REPORT_SCHEMA_ID: &str = "urn:weave:schema:character-health-report:1";

/// One audit manifest. Referenced source files are loaded separately and never copied into reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthManifest {
    pub manifest_format_version: u32,
    pub id: String,
    /// Canonically sorted source documents. Exactly one collection document is primary.
    pub documents: Vec<CharacterHealthDocumentRef>,
    pub policy: CharacterHealthPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressions: Vec<CharacterHealthSuppression>,
    pub provenance: Provenance,
}

/// Safe, project-relative source reference used by an audit manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthDocumentRef {
    pub id: String,
    pub kind: CharacterHealthDocumentKind,
    pub format: CharacterHealthDocumentFormat,
    pub path: String,
    /// Stable semantic identity used to pair equivalent RON and JSON documents.
    pub logical_id: String,
    /// Exactly one Character collection source is the audit's canonical corpus.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary: bool,
    /// Required for runtime pack documents and optional for character-owned artifacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
}

const fn is_false(value: &bool) -> bool {
    !*value
}

/// Source artifact kinds understood by the extensible v1 health router.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CharacterHealthDocumentKind {
    CharacterCollection,
    CharacterProfile,
    CharacterTemplate,
    CharacterOverlay,
    CharacterSynthesis,
    TemporalContextPack,
    TemporalContextReceipt,
    AlignmentPack,
    AlignmentReceipt,
    PresentationCatalog,
    PresentationReceipt,
    RelationshipKindPack,
    RelationshipPolicy,
    ExpressionPack,
    ExpressionRevision,
    ProjectionPack,
    ProjectionProposal,
    ProjectionReview,
    ProjectionReceipt,
    AssistanceTemplate,
    AssistanceCandidateSet,
    AssistanceAdvisoryReview,
    AssistanceDecisionReview,
    AssistanceReceipt,
    AssistanceJob,
    AssistanceBatchReceipt,
    RuntimeDomainPack,
}

/// Supported source encodings. CSV is deliberately an export diagnostic, not canonical input.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CharacterHealthDocumentFormat {
    Json,
    Ron,
}

/// Explicit project policy. Distribution constraints are absent unless a project opts into them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthPolicy {
    pub policy_format_version: u32,
    /// CI is successful when absent; otherwise active diagnostics at or above this threshold fail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_threshold: Option<CharacterHealthSeverity>,
    pub require_portable_pairs: bool,
    pub csv_loss_diagnostics: bool,
    /// Optional project-required extensions. The base audit imposes none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_extension_namespaces: Vec<String>,
    /// Optional, explicitly documented quotas. Empty means distributions are descriptive only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub distribution_constraints: Vec<CharacterHealthDistributionConstraint>,
}

impl Default for CharacterHealthPolicy {
    fn default() -> Self {
        Self {
            policy_format_version: CHARACTER_HEALTH_POLICY_FORMAT_VERSION,
            failure_threshold: Some(CharacterHealthSeverity::Error),
            require_portable_pairs: true,
            csv_loss_diagnostics: false,
            required_extension_namespaces: Vec::new(),
            distribution_constraints: Vec::new(),
        }
    }
}

/// One explicit distribution rule. It is never inferred from corpus composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthDistributionConstraint {
    pub id: String,
    pub metric: CharacterHealthDistributionMetric,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<u64>,
    pub rationale: String,
    pub policy_url: String,
}

/// Descriptive metrics that a project may explicitly constrain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "metric", deny_unknown_fields)]
pub enum CharacterHealthDistributionMetric {
    CharacterCount,
    TraitBand {
        trait_id: HexacoTrait,
        band: TraitBand,
    },
    RelationshipKind {
        kind_id: String,
    },
    RoleTaxonomy {
        taxonomy: String,
    },
    ExpressionCategory {
        category: String,
    },
}

/// Versioned, attributable, reviewable suppression. Matching is exact except for path prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthSuppression {
    pub suppression_format_version: u32,
    pub id: String,
    pub code: CharacterHealthDiagnosticCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
    pub path_prefix: String,
    pub reviewed_by: String,
    pub rationale: String,
    pub review_revision: u64,
}

/// In-memory loaded project. Raw source is input-only and has no serialization implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterHealthProject {
    pub manifest: CharacterHealthManifest,
    pub documents: BTreeMap<String, String>,
}

impl CharacterHealthProject {
    /// Construct an exact project only when every manifest reference has one loaded source.
    pub fn new(
        manifest: CharacterHealthManifest,
        documents: BTreeMap<String, String>,
    ) -> Result<Self, CharacterHealthError> {
        validate_character_health_manifest(&manifest)?;
        let expected = manifest
            .documents
            .iter()
            .map(|document| document.id.as_str())
            .collect::<BTreeSet<_>>();
        let actual = documents
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if expected != actual {
            return Err(health_error(
                "documents",
                "loaded health sources must exactly match manifest document identifiers",
            ));
        }
        if documents
            .values()
            .any(|source| source.len() > CHARACTER_HEALTH_MAX_DOCUMENT_BYTES)
        {
            return Err(health_error(
                "documents",
                "a health source exceeds the documented byte limit",
            ));
        }
        Ok(Self {
            manifest,
            documents,
        })
    }
}

/// Stable health diagnostic severity ordered from advisory note through blocking error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CharacterHealthSeverity {
    Note,
    Warning,
    Error,
}

/// Stable, versioned diagnostic vocabulary for the aggregate Character audit.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum CharacterHealthDiagnosticCode {
    #[serde(rename = "H100")]
    InvalidDocument,
    #[serde(rename = "H101")]
    MissingRequiredField,
    #[serde(rename = "H102")]
    UnsupportedVersion,
    #[serde(rename = "H103")]
    DuplicateIdentifier,
    #[serde(rename = "H104")]
    IncompleteFacetConfidence,
    #[serde(rename = "H105")]
    InconsistentDerivedView,
    #[serde(rename = "H106")]
    StaleFingerprint,
    #[serde(rename = "H107")]
    UnresolvedReference,
    #[serde(rename = "H108")]
    InvalidTemplateOverlay,
    #[serde(rename = "H109")]
    ProtectedFieldAuthority,
    #[serde(rename = "H110")]
    SensitiveValue,
    #[serde(rename = "H200")]
    BrokenRelationshipInverse,
    #[serde(rename = "H201")]
    RelationshipSymmetry,
    #[serde(rename = "H202")]
    PedigreeConflict,
    #[serde(rename = "H203")]
    InvalidRelationshipDate,
    #[serde(rename = "H204")]
    DuplicateRelationship,
    #[serde(rename = "H205")]
    StaleAffinityEvidence,
    #[serde(rename = "H206")]
    AgeSafeguard,
    #[serde(rename = "H207")]
    KinshipSafeguard,
    #[serde(rename = "H208")]
    PartnershipSafeguard,
    #[serde(rename = "H209")]
    ConsentSafeguard,
    #[serde(rename = "H300")]
    MissingExpressionLink,
    #[serde(rename = "H301")]
    PlaceholderViolation,
    #[serde(rename = "H302")]
    ExpressionText,
    #[serde(rename = "H303")]
    ExpressionConstraintConflict,
    #[serde(rename = "H304")]
    UnsafePersonalization,
    #[serde(rename = "H305")]
    UnreviewedExpressionSuggestion,
    #[serde(rename = "H306")]
    DuplicateExpressionVariant,
    #[serde(rename = "H307")]
    NearDuplicateExpressionVariant,
    #[serde(rename = "H400")]
    MissingPackProvenance,
    #[serde(rename = "H401")]
    UnsupportedProjectionVersion,
    #[serde(rename = "H402")]
    UnexplainedProjectionScore,
    #[serde(rename = "H403")]
    UnreviewedProjection,
    #[serde(rename = "H404")]
    StaleProjection,
    #[serde(rename = "H410")]
    UnreviewedAssistanceCandidate,
    #[serde(rename = "H411")]
    StaleAssistanceCandidate,
    #[serde(rename = "H412")]
    ProviderMetadataPolicy,
    #[serde(rename = "H413")]
    MissingAcceptedSource,
    #[serde(rename = "H500")]
    RonJsonMismatch,
    #[serde(rename = "H501")]
    NonCanonicalOrdering,
    #[serde(rename = "H502")]
    MissingRuntimeReference,
    #[serde(rename = "H503")]
    RuntimeAuthorityBoundary,
    #[serde(rename = "H504")]
    CsvLoss,
    #[serde(rename = "H600")]
    CoveragePolicy,
    #[serde(rename = "H601")]
    DistributionPolicy,
    #[serde(rename = "H700")]
    InvalidSuppression,
    #[serde(rename = "H701")]
    UnusedSuppression,
}

impl CharacterHealthDiagnosticCode {
    /// Public stable string used by text reports, editor links, and CI filters.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidDocument => "H100",
            Self::MissingRequiredField => "H101",
            Self::UnsupportedVersion => "H102",
            Self::DuplicateIdentifier => "H103",
            Self::IncompleteFacetConfidence => "H104",
            Self::InconsistentDerivedView => "H105",
            Self::StaleFingerprint => "H106",
            Self::UnresolvedReference => "H107",
            Self::InvalidTemplateOverlay => "H108",
            Self::ProtectedFieldAuthority => "H109",
            Self::SensitiveValue => "H110",
            Self::BrokenRelationshipInverse => "H200",
            Self::RelationshipSymmetry => "H201",
            Self::PedigreeConflict => "H202",
            Self::InvalidRelationshipDate => "H203",
            Self::DuplicateRelationship => "H204",
            Self::StaleAffinityEvidence => "H205",
            Self::AgeSafeguard => "H206",
            Self::KinshipSafeguard => "H207",
            Self::PartnershipSafeguard => "H208",
            Self::ConsentSafeguard => "H209",
            Self::MissingExpressionLink => "H300",
            Self::PlaceholderViolation => "H301",
            Self::ExpressionText => "H302",
            Self::ExpressionConstraintConflict => "H303",
            Self::UnsafePersonalization => "H304",
            Self::UnreviewedExpressionSuggestion => "H305",
            Self::DuplicateExpressionVariant => "H306",
            Self::NearDuplicateExpressionVariant => "H307",
            Self::MissingPackProvenance => "H400",
            Self::UnsupportedProjectionVersion => "H401",
            Self::UnexplainedProjectionScore => "H402",
            Self::UnreviewedProjection => "H403",
            Self::StaleProjection => "H404",
            Self::UnreviewedAssistanceCandidate => "H410",
            Self::StaleAssistanceCandidate => "H411",
            Self::ProviderMetadataPolicy => "H412",
            Self::MissingAcceptedSource => "H413",
            Self::RonJsonMismatch => "H500",
            Self::NonCanonicalOrdering => "H501",
            Self::MissingRuntimeReference => "H502",
            Self::RuntimeAuthorityBoundary => "H503",
            Self::CsvLoss => "H504",
            Self::CoveragePolicy => "H600",
            Self::DistributionPolicy => "H601",
            Self::InvalidSuppression => "H700",
            Self::UnusedSuppression => "H701",
        }
    }
}

/// Collection, character, or standalone artifact scope for one diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "scope", deny_unknown_fields)]
pub enum CharacterHealthDiagnosticScope {
    Collection,
    Character {
        character_id: String,
    },
    Document {
        document_id: String,
    },
    CharacterDocument {
        document_id: String,
        character_id: String,
    },
}

impl CharacterHealthDiagnosticScope {
    fn document_id(&self) -> Option<&str> {
        match self {
            Self::Document { document_id } | Self::CharacterDocument { document_id, .. } => {
                Some(document_id)
            }
            _ => None,
        }
    }

    fn character_id(&self) -> Option<&str> {
        match self {
            Self::Character { character_id } | Self::CharacterDocument { character_id, .. } => {
                Some(character_id)
            }
            _ => None,
        }
    }
}

/// Optional file and line coordinate. Values are locations only and never rejected source text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthSourceLocation {
    pub document_id: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// One deterministic, redaction-safe, actionable diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthDiagnostic {
    pub id: String,
    pub code: CharacterHealthDiagnosticCode,
    pub severity: CharacterHealthSeverity,
    pub scope: CharacterHealthDiagnosticScope,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CharacterHealthSourceLocation>,
    pub explanation: String,
    pub remediation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppressed_by: Option<String>,
}

/// Aggregate counts after suppressions are applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthSummary {
    pub documents_total: u64,
    pub documents_valid: u64,
    pub characters_total: u64,
    pub diagnostics_total: u64,
    pub active_diagnostics: u64,
    pub suppressed_diagnostics: u64,
    pub active_by_severity: BTreeMap<CharacterHealthSeverity, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_threshold: Option<CharacterHealthSeverity>,
    pub ci_exit_code: u8,
}

/// Per-character completeness and review counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthCharacterCoverage {
    pub factors_present: u32,
    pub factors_total: u32,
    pub facets_present: u32,
    pub facets_total: u32,
    pub known_confidence_values: u32,
    pub measured_values: u32,
    pub current_values: u32,
    pub stale_values: u32,
    pub relationship_edges: u32,
    pub expression_records: u32,
    pub role_projections: u32,
    pub pending_suggestions: u32,
}

/// Corpus-wide coverage, retaining exact per-character denominators.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthCoverage {
    pub characters: BTreeMap<String, CharacterHealthCharacterCoverage>,
    pub factor_values_present: u64,
    pub factor_values_total: u64,
    pub facet_values_present: u64,
    pub facet_values_total: u64,
    pub known_confidence_values: u64,
    pub measured_values: u64,
}

/// Descriptive distributions only. They have no normative target absent an explicit constraint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthDistributions {
    pub trait_bands: BTreeMap<HexacoTrait, BTreeMap<TraitBand, u64>>,
    pub relationship_kinds: BTreeMap<String, u64>,
    pub role_taxonomies: BTreeMap<String, u64>,
    pub expression_categories: BTreeMap<String, u64>,
    pub constraints_configured: u64,
}

/// Review trace for one configured suppression, including whether it matched anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthSuppressionResult {
    pub suppression_id: String,
    pub reviewed_by: String,
    pub rationale: String,
    pub review_revision: u64,
    pub matched_diagnostic_ids: Vec<String>,
}

/// Complete stable JSON/RON health report. It contains hashes and locations, never source payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterHealthReport {
    pub report_format_version: u32,
    pub id: String,
    pub manifest_id: String,
    pub input_sha256: String,
    pub summary: CharacterHealthSummary,
    pub coverage: CharacterHealthCoverage,
    pub distributions: CharacterHealthDistributions,
    pub diagnostics: Vec<CharacterHealthDiagnostic>,
    pub suppressions: Vec<CharacterHealthSuppressionResult>,
    pub read_only: bool,
    pub source_payloads_retained: bool,
}

/// Filter for CI, editor, or CLI presentation. Filtering never alters the underlying report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterHealthFilter {
    pub character_ids: BTreeSet<String>,
    pub codes: BTreeSet<CharacterHealthDiagnosticCode>,
    pub minimum_severity: Option<CharacterHealthSeverity>,
    pub include_suppressed: bool,
}

/// Manifest or project construction failure. Source document failures become report diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterHealthError {
    path: String,
    message: &'static str,
}

impl CharacterHealthError {
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for CharacterHealthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Character health manifest at `{}`: {}",
            self.path, self.message
        )
    }
}

impl std::error::Error for CharacterHealthError {}

impl CharacterHealthManifest {
    pub fn from_json(source: &str) -> Result<Self, CharacterHealthError> {
        let value = parse_strict_json(source).map_err(|_| {
            health_error(
                "document",
                "health manifest does not match the strict JSON contract",
            )
        })?;
        validate_character_health_manifest(&value)?;
        Ok(value)
    }

    pub fn from_ron(source: &str) -> Result<Self, CharacterHealthError> {
        let value = ron::from_str(source).map_err(|_| {
            health_error(
                "document",
                "health manifest does not match the strict RON contract",
            )
        })?;
        validate_character_health_manifest(&value)?;
        Ok(value)
    }

    pub fn to_json(&self) -> Result<String, CharacterHealthError> {
        validate_character_health_manifest(self)?;
        to_pretty_json(self).map_err(|_| health_encoding_error())
    }

    pub fn to_ron(&self) -> Result<String, CharacterHealthError> {
        validate_character_health_manifest(self)?;
        to_pretty_ron(self).map_err(|_| health_encoding_error())
    }
}

impl CharacterHealthReport {
    pub fn from_json(source: &str) -> Result<Self, CharacterHealthError> {
        let value = parse_strict_json(source).map_err(|_| health_encoding_error())?;
        validate_character_health_report(&value)?;
        Ok(value)
    }

    pub fn from_ron(source: &str) -> Result<Self, CharacterHealthError> {
        let value = ron::from_str(source).map_err(|_| health_encoding_error())?;
        validate_character_health_report(&value)?;
        Ok(value)
    }

    pub fn to_json(&self) -> Result<String, CharacterHealthError> {
        validate_character_health_report(self)?;
        to_pretty_json(self).map_err(|_| health_encoding_error())
    }

    pub fn to_ron(&self) -> Result<String, CharacterHealthError> {
        validate_character_health_report(self)?;
        to_pretty_ron(self).map_err(|_| health_encoding_error())
    }
}

/// Canonical health-manifest JSON Schema.
pub fn character_health_manifest_schema() -> Result<String, CharacterHealthError> {
    health_schema::<CharacterHealthManifest>(
        MANIFEST_SCHEMA_ID,
        "Weave Character Health Manifest v1",
        "manifest_format_version",
        CHARACTER_HEALTH_MANIFEST_FORMAT_VERSION,
    )
}

/// Canonical stable health-report JSON Schema.
pub fn character_health_report_schema() -> Result<String, CharacterHealthError> {
    health_schema::<CharacterHealthReport>(
        REPORT_SCHEMA_ID,
        "Weave Character Health Report v1",
        "report_format_version",
        CHARACTER_HEALTH_REPORT_FORMAT_VERSION,
    )
}

fn health_schema<T: JsonSchema>(
    id: &str,
    title: &str,
    version_property: &str,
    version: u32,
) -> Result<String, CharacterHealthError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| health_encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-health-contract-version".to_owned(),
            serde_json::Value::from(1),
        );
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut(version_property))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert("const".to_owned(), serde_json::Value::from(version));
        }
    }
    sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| health_encoding_error())
}

fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => values.iter_mut().for_each(sort_json_keys),
        serde_json::Value::Object(values) => {
            values.values_mut().for_each(sort_json_keys);
            values.sort_keys();
        }
        _ => {}
    }
}

fn health_error(path: impl Into<String>, message: &'static str) -> CharacterHealthError {
    CharacterHealthError {
        path: path.into(),
        message,
    }
}

fn health_encoding_error() -> CharacterHealthError {
    health_error(
        "document",
        "health document does not match the strict serialized contract",
    )
}

/// Validate a manifest without opening any referenced file.
pub fn validate_character_health_manifest(
    manifest: &CharacterHealthManifest,
) -> Result<(), CharacterHealthError> {
    if manifest.manifest_format_version != CHARACTER_HEALTH_MANIFEST_FORMAT_VERSION {
        return Err(health_error(
            "manifest_format_version",
            "unsupported Character health manifest version",
        ));
    }
    health_namespaced("id", &manifest.id)?;
    if manifest.documents.is_empty() || manifest.documents.len() > CHARACTER_HEALTH_MAX_DOCUMENTS {
        return Err(health_error(
            "documents",
            "health manifest requires a bounded non-empty document list",
        ));
    }
    let mut prior = None::<&str>;
    let mut coordinates = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut primary_collections = 0usize;
    for (index, document) in manifest.documents.iter().enumerate() {
        let path = format!("documents[{index}]");
        health_namespaced(&format!("{path}.id"), &document.id)?;
        health_namespaced(&format!("{path}.logical_id"), &document.logical_id)?;
        health_relative_path(&format!("{path}.path"), &document.path)?;
        if prior.is_some_and(|prior| prior >= document.id.as_str()) {
            return Err(health_error(
                "documents",
                "health document identifiers must be unique and canonically sorted",
            ));
        }
        prior = Some(&document.id);
        if !paths.insert(document.path.as_str()) {
            return Err(health_error(
                "documents",
                "health source paths must be unique",
            ));
        }
        if !coordinates.insert((document.logical_id.as_str(), document.kind, document.format)) {
            return Err(health_error(
                "documents",
                "a health semantic coordinate may have only one source per encoding",
            ));
        }
        if document.primary {
            if document.kind != CharacterHealthDocumentKind::CharacterCollection {
                return Err(health_error(
                    format!("{path}.primary"),
                    "only a Character collection document may be primary",
                ));
            }
            primary_collections += 1;
        }
        if let Some(character_id) = &document.character_id {
            health_namespaced(&format!("{path}.character_id"), character_id)?;
        }
        if document.kind == CharacterHealthDocumentKind::RuntimeDomainPack
            && document.character_id.is_none()
        {
            return Err(health_error(
                format!("{path}.character_id"),
                "runtime Character packs require an owning character identifier",
            ));
        }
    }
    if primary_collections != 1 {
        return Err(health_error(
            "documents",
            "health manifest requires exactly one primary Character collection",
        ));
    }
    validate_health_policy(&manifest.policy)?;
    let mut prior_suppression = None::<&str>;
    for (index, suppression) in manifest.suppressions.iter().enumerate() {
        let path = format!("suppressions[{index}]");
        if suppression.suppression_format_version != CHARACTER_HEALTH_SUPPRESSION_FORMAT_VERSION {
            return Err(health_error(
                format!("{path}.suppression_format_version"),
                "unsupported Character health suppression version",
            ));
        }
        health_local(&format!("{path}.id"), &suppression.id)?;
        if prior_suppression.is_some_and(|prior| prior >= suppression.id.as_str()) {
            return Err(health_error(
                "suppressions",
                "health suppressions must be unique and canonically sorted",
            ));
        }
        prior_suppression = Some(&suppression.id);
        if let Some(document_id) = &suppression.document_id {
            health_namespaced(&format!("{path}.document_id"), document_id)?;
            if !manifest
                .documents
                .iter()
                .any(|document| document.id == *document_id)
            {
                return Err(health_error(
                    format!("{path}.document_id"),
                    "health suppression references an unavailable document",
                ));
            }
        }
        if let Some(character_id) = &suppression.character_id {
            health_namespaced(&format!("{path}.character_id"), character_id)?;
        }
        health_text(
            &format!("{path}.path_prefix"),
            &suppression.path_prefix,
            1,
            2_048,
        )?;
        health_text(
            &format!("{path}.reviewed_by"),
            &suppression.reviewed_by,
            1,
            256,
        )?;
        health_text(
            &format!("{path}.rationale"),
            &suppression.rationale,
            1,
            2_048,
        )?;
        if suppression.review_revision == 0 {
            return Err(health_error(
                format!("{path}.review_revision"),
                "health suppression review revision must be positive",
            ));
        }
    }
    validate_provenance(&manifest.provenance).map_err(|_| {
        health_error(
            "provenance",
            "health manifest provenance is invalid or not publicly attributable",
        )
    })
}

fn validate_health_policy(policy: &CharacterHealthPolicy) -> Result<(), CharacterHealthError> {
    if policy.policy_format_version != CHARACTER_HEALTH_POLICY_FORMAT_VERSION {
        return Err(health_error(
            "policy.policy_format_version",
            "unsupported Character health policy version",
        ));
    }
    let mut prior_namespace = None::<&str>;
    for namespace in &policy.required_extension_namespaces {
        health_namespaced("policy.required_extension_namespaces", namespace)?;
        if prior_namespace.is_some_and(|prior| prior >= namespace.as_str()) {
            return Err(health_error(
                "policy.required_extension_namespaces",
                "required extension namespaces must be unique and sorted",
            ));
        }
        prior_namespace = Some(namespace);
    }
    let mut prior_constraint = None::<&str>;
    for (index, constraint) in policy.distribution_constraints.iter().enumerate() {
        let path = format!("policy.distribution_constraints[{index}]");
        health_local(&format!("{path}.id"), &constraint.id)?;
        if prior_constraint.is_some_and(|prior| prior >= constraint.id.as_str()) {
            return Err(health_error(
                "policy.distribution_constraints",
                "distribution constraints must be unique and sorted",
            ));
        }
        prior_constraint = Some(&constraint.id);
        if constraint.minimum.is_none() && constraint.maximum.is_none() {
            return Err(health_error(
                path,
                "distribution constraint requires a minimum, maximum, or both",
            ));
        }
        if matches!((constraint.minimum, constraint.maximum), (Some(min), Some(max)) if min > max) {
            return Err(health_error(
                path,
                "distribution constraint minimum must not exceed its maximum",
            ));
        }
        health_text(
            &format!("{path}.rationale"),
            &constraint.rationale,
            1,
            2_048,
        )?;
        health_text(
            &format!("{path}.policy_url"),
            &constraint.policy_url,
            8,
            2_048,
        )?;
        if !matches!(
            constraint.policy_url.strip_prefix("https://"),
            Some(rest) if !rest.is_empty()
        ) {
            return Err(health_error(
                format!("{path}.policy_url"),
                "distribution policy documentation must use an absolute HTTPS URL",
            ));
        }
        match &constraint.metric {
            CharacterHealthDistributionMetric::RelationshipKind { kind_id } => {
                health_namespaced(&format!("{path}.metric.kind_id"), kind_id)?;
            }
            CharacterHealthDistributionMetric::RoleTaxonomy { taxonomy } => {
                health_namespaced(&format!("{path}.metric.taxonomy"), taxonomy)?;
            }
            CharacterHealthDistributionMetric::ExpressionCategory { category } => {
                health_namespaced(&format!("{path}.metric.category"), category)?;
            }
            CharacterHealthDistributionMetric::CharacterCount
            | CharacterHealthDistributionMetric::TraitBand { .. } => {}
        }
    }
    Ok(())
}

/// Independently validate a report's ordering, hashes, summary, and redaction-safe boundary.
pub fn validate_character_health_report(
    report: &CharacterHealthReport,
) -> Result<(), CharacterHealthError> {
    if report.report_format_version != CHARACTER_HEALTH_REPORT_FORMAT_VERSION {
        return Err(health_error(
            "report_format_version",
            "unsupported Character health report version",
        ));
    }
    health_namespaced("id", &report.id)?;
    health_namespaced("manifest_id", &report.manifest_id)?;
    validate_health_sha256("input_sha256", &report.input_sha256)?;
    if !report.read_only || report.source_payloads_retained {
        return Err(health_error(
            "read_only",
            "health reports must remain read-only and retain no source payloads",
        ));
    }
    let mut prior = None::<(
        &CharacterHealthDiagnosticScope,
        &str,
        CharacterHealthDiagnosticCode,
        &str,
    )>;
    let mut ids = BTreeSet::new();
    for diagnostic in &report.diagnostics {
        health_local("diagnostics.id", &diagnostic.id)?;
        if !ids.insert(diagnostic.id.as_str()) {
            return Err(health_error(
                "diagnostics",
                "health diagnostic identifiers must be unique",
            ));
        }
        health_text("diagnostics.path", &diagnostic.path, 1, 2_048)?;
        health_text("diagnostics.explanation", &diagnostic.explanation, 1, 2_048)?;
        health_text("diagnostics.remediation", &diagnostic.remediation, 1, 2_048)?;
        if contains_sensitive_value(&diagnostic.explanation)
            || contains_sensitive_value(&diagnostic.remediation)
        {
            return Err(health_error(
                "diagnostics",
                "health diagnostic text must not contain credential-shaped values",
            ));
        }
        if let Some(source) = &diagnostic.source {
            health_namespaced("diagnostics.source.document_id", &source.document_id)?;
            health_relative_path("diagnostics.source.file", &source.file)?;
            if source.line == Some(0) || source.column == Some(0) {
                return Err(health_error(
                    "diagnostics.source",
                    "health source line and column coordinates are one-based",
                ));
            }
        }
        if let Some(suppression) = &diagnostic.suppressed_by {
            health_local("diagnostics.suppressed_by", suppression)?;
        }
        let key = (
            &diagnostic.scope,
            diagnostic.path.as_str(),
            diagnostic.code,
            diagnostic.id.as_str(),
        );
        if prior.is_some_and(|prior| prior >= key) {
            return Err(health_error(
                "diagnostics",
                "health diagnostics must be unique and canonically sorted",
            ));
        }
        prior = Some(key);
    }
    let expected_summary = summarize_health(
        report.summary.documents_total,
        report.summary.documents_valid,
        report.summary.characters_total,
        &report.diagnostics,
        report.summary.failure_threshold,
    );
    if report.summary != expected_summary {
        return Err(health_error(
            "summary",
            "health summary does not match its diagnostics and CI policy",
        ));
    }
    let mut prior_suppression = None::<&str>;
    for suppression in &report.suppressions {
        health_local("suppressions.suppression_id", &suppression.suppression_id)?;
        if prior_suppression.is_some_and(|prior| prior >= suppression.suppression_id.as_str()) {
            return Err(health_error(
                "suppressions",
                "health suppression results must be unique and sorted",
            ));
        }
        prior_suppression = Some(&suppression.suppression_id);
        health_text("suppressions.reviewed_by", &suppression.reviewed_by, 1, 256)?;
        health_text("suppressions.rationale", &suppression.rationale, 1, 2_048)?;
        if suppression.review_revision == 0
            || !strictly_sorted_strings(&suppression.matched_diagnostic_ids)
        {
            return Err(health_error(
                "suppressions",
                "health suppression review trace is invalid or unsorted",
            ));
        }
        if suppression
            .matched_diagnostic_ids
            .iter()
            .any(|id| !ids.contains(id.as_str()))
        {
            return Err(health_error(
                "suppressions.matched_diagnostic_ids",
                "health suppression result references an unavailable diagnostic",
            ));
        }
    }
    Ok(())
}

fn health_namespaced(path: &str, value: &str) -> Result<(), CharacterHealthError> {
    validate_namespaced_id(path, value)
        .map_err(|_| health_error(path, "health value requires a valid namespaced identifier"))
}

fn health_local(path: &str, value: &str) -> Result<(), CharacterHealthError> {
    validate_local_id(path, value)
        .map_err(|_| health_error(path, "health value requires a valid local identifier"))
}

fn health_relative_path(path: &str, value: &str) -> Result<(), CharacterHealthError> {
    validate_relative_path(path, value)
        .map_err(|_| health_error(path, "health source requires a safe project-relative path"))
}

fn health_text(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterHealthError> {
    validate_text(path, value, minimum, maximum)
        .map_err(|_| health_error(path, "health text is outside its documented safe limits"))
}

fn validate_health_sha256(path: &str, value: &str) -> Result<(), CharacterHealthError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(health_error(
            path,
            "health fingerprint must be lowercase SHA-256 hexadecimal",
        ));
    }
    Ok(())
}

fn strictly_sorted_strings(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

#[derive(Debug, Clone)]
struct TypedHealthDocument<T> {
    document_id: String,
    value: T,
    valid: bool,
}

#[derive(Debug, Clone)]
struct ParsedHealthDocument {
    semantic_sha256: Option<String>,
    valid: bool,
}

#[derive(Default)]
struct ParsedHealthProject {
    records: BTreeMap<String, ParsedHealthDocument>,
    collections: Vec<TypedHealthDocument<CharacterCollection>>,
    profiles: Vec<TypedHealthDocument<CharacterProfile>>,
    templates: Vec<TypedHealthDocument<CharacterTemplate>>,
    overlays: Vec<TypedHealthDocument<CharacterOverlay>>,
    syntheses: Vec<TypedHealthDocument<CharacterSynthesisResult>>,
    relationship_packs: Vec<TypedHealthDocument<RelationshipKindPack>>,
    relationship_policies: Vec<TypedHealthDocument<RelationshipGraphPolicy>>,
    expression_packs: Vec<TypedHealthDocument<ExpressionPack>>,
    projection_packs: Vec<TypedHealthDocument<ProjectionPack>>,
    projection_proposals: Vec<TypedHealthDocument<ProjectionProposal>>,
    projection_reviews: Vec<TypedHealthDocument<ProjectionReview>>,
    projection_receipts: Vec<TypedHealthDocument<ProjectionReceipt>>,
    assistance_candidate_sets: Vec<TypedHealthDocument<AssistanceCandidateSet>>,
    assistance_decision_reviews: Vec<TypedHealthDocument<AssistanceDecisionReview>>,
    assistance_receipts: Vec<TypedHealthDocument<AssistanceReceipt>>,
    assistance_jobs: Vec<TypedHealthDocument<AssistanceJob>>,
    assistance_batch_receipts: Vec<TypedHealthDocument<AssistanceBatchReceipt>>,
    runtime_packs: Vec<TypedHealthDocument<DomainPack>>,
}

impl ParsedHealthProject {
    fn primary_collection<'a>(
        &'a self,
        manifest: &CharacterHealthManifest,
    ) -> Option<&'a TypedHealthDocument<CharacterCollection>> {
        let primary_id = manifest
            .documents
            .iter()
            .find(|document| document.primary)
            .map(|document| document.id.as_str())?;
        self.collections
            .iter()
            .find(|document| document.document_id == primary_id)
    }
}

/// Run the complete audit without changing the manifest, loaded sources, or any Character value.
pub fn audit_character_health(
    project: &CharacterHealthProject,
) -> Result<CharacterHealthReport, CharacterHealthError> {
    validate_character_health_manifest(&project.manifest)?;
    let expected = project
        .manifest
        .documents
        .iter()
        .map(|document| document.id.as_str())
        .collect::<BTreeSet<_>>();
    let actual = project
        .documents
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(health_error(
            "documents",
            "loaded health sources must exactly match manifest document identifiers",
        ));
    }
    let input_sha256 = health_project_fingerprint(project)?;
    let mut diagnostics = Vec::new();
    let parsed = parse_health_documents(project, &mut diagnostics);

    audit_portability(project, &parsed, &mut diagnostics);
    let mut coverage = CharacterHealthCoverage::default();
    let mut distributions = CharacterHealthDistributions {
        constraints_configured: project.manifest.policy.distribution_constraints.len() as u64,
        ..CharacterHealthDistributions::default()
    };
    let characters_total = if let Some(primary) = parsed.primary_collection(&project.manifest) {
        audit_primary_collection(
            project,
            primary,
            &parsed,
            &mut coverage,
            &mut distributions,
            &mut diagnostics,
        );
        primary.value.characters.len() as u64
    } else {
        0
    };
    audit_template_overlay_lineage(project, &parsed, &mut diagnostics);
    audit_projection_artifacts(project, &parsed, &mut diagnostics);
    audit_assistance_artifacts(project, &parsed, &mut diagnostics);
    audit_runtime_packs(project, &parsed, &mut diagnostics);
    audit_distribution_constraints(
        &project.manifest.policy,
        &distributions,
        characters_total,
        &mut diagnostics,
    );

    assign_diagnostic_ids(&mut diagnostics)?;
    let suppressions = apply_health_suppressions(
        &project.manifest.suppressions,
        &mut diagnostics,
        &project.manifest,
    )?;
    assign_diagnostic_ids(&mut diagnostics)?;
    sort_health_diagnostics(&mut diagnostics);
    let documents_valid = parsed
        .records
        .values()
        .filter(|record| record.valid)
        .count() as u64;
    let summary = summarize_health(
        project.manifest.documents.len() as u64,
        documents_valid,
        characters_total,
        &diagnostics,
        project.manifest.policy.failure_threshold,
    );
    let report = CharacterHealthReport {
        report_format_version: CHARACTER_HEALTH_REPORT_FORMAT_VERSION,
        id: format!("org.weave.character.health.report_{}", &input_sha256[..16]),
        manifest_id: project.manifest.id.clone(),
        input_sha256,
        summary,
        coverage,
        distributions,
        diagnostics,
        suppressions,
        read_only: true,
        source_payloads_retained: false,
    };
    validate_character_health_report(&report)?;
    Ok(report)
}

fn health_project_fingerprint(
    project: &CharacterHealthProject,
) -> Result<String, CharacterHealthError> {
    let manifest = project.manifest.to_json()?;
    let mut hasher = Sha256::new();
    fingerprint_part(&mut hasher, manifest.as_bytes());
    for document in &project.manifest.documents {
        fingerprint_part(&mut hasher, document.id.as_bytes());
        fingerprint_part(&mut hasher, document.path.as_bytes());
        let source = project.documents.get(&document.id).ok_or_else(|| {
            health_error(
                "documents",
                "health manifest source was not loaded before fingerprinting",
            )
        })?;
        fingerprint_part(&mut hasher, source.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn fingerprint_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn parse_health_documents(
    project: &CharacterHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) -> ParsedHealthProject {
    let mut parsed = ParsedHealthProject::default();
    for reference in &project.manifest.documents {
        let source = &project.documents[&reference.id];
        detect_missing_required_fields(reference, source, diagnostics);
        if contains_sensitive_value(source) {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::SensitiveValue,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "document",
                Some(source_location(reference)),
                "source contains a credential-shaped value that cannot enter a portable Character audit",
                "Replace the complete sensitive value with [REDACTED] and load credentials only through an approved secret channel.",
            );
        }
        macro_rules! character_document {
            ($type:ty, $validator:path, $field:ident) => {{
                match parse_health_source::<$type>(source, reference.format) {
                    Ok(value) => {
                        let result = $validator(&value);
                        let valid = result.is_ok();
                        if let Err(failure) = result {
                            push_character_failure(diagnostics, reference, failure.diagnostic());
                        }
                        record_parsed_document(
                            &mut parsed,
                            reference,
                            source,
                            &value,
                            valid,
                            diagnostics,
                        );
                        parsed.$field.push(TypedHealthDocument {
                            document_id: reference.id.clone(),
                            value,
                            valid,
                        });
                    }
                    Err(()) => record_parse_failure(&mut parsed, reference, diagnostics),
                }
            }};
        }
        match reference.kind {
            CharacterHealthDocumentKind::CharacterCollection => {
                match parse_health_source::<CharacterCollection>(source, reference.format) {
                    Ok(value) => {
                        let result = validate_character_collection(&value);
                        let valid = result.is_ok();
                        if let Err(failure) = result {
                            push_character_corpus_failure(
                                diagnostics,
                                reference,
                                failure.diagnostic(),
                            );
                        }
                        record_parsed_document(
                            &mut parsed,
                            reference,
                            source,
                            &value,
                            valid,
                            diagnostics,
                        );
                        parsed.collections.push(TypedHealthDocument {
                            document_id: reference.id.clone(),
                            value,
                            valid,
                        });
                    }
                    Err(()) => record_parse_failure(&mut parsed, reference, diagnostics),
                }
            }
            CharacterHealthDocumentKind::CharacterProfile => {
                character_document!(CharacterProfile, validate_profile, profiles);
            }
            CharacterHealthDocumentKind::CharacterTemplate => {
                character_document!(CharacterTemplate, validate_template, templates);
            }
            CharacterHealthDocumentKind::CharacterOverlay => {
                character_document!(CharacterOverlay, validate_overlay, overlays);
            }
            CharacterHealthDocumentKind::CharacterSynthesis => {
                character_document!(
                    CharacterSynthesisResult,
                    validate_synthesis_result,
                    syntheses
                );
            }
            CharacterHealthDocumentKind::TemporalContextPack => {
                parse_character_only::<TemporalContextPack>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_temporal_context_pack,
                );
            }
            CharacterHealthDocumentKind::TemporalContextReceipt => {
                parse_character_only::<TemporalContextReceipt>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_temporal_context_receipt,
                );
            }
            CharacterHealthDocumentKind::AlignmentPack => {
                parse_character_only::<AlignmentPack>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_alignment_pack,
                );
            }
            CharacterHealthDocumentKind::AlignmentReceipt => {
                parse_character_only::<AlignmentReceipt>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_alignment_receipt,
                );
            }
            CharacterHealthDocumentKind::PresentationCatalog => {
                parse_character_only::<PresentationCatalog>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_presentation_catalog,
                );
            }
            CharacterHealthDocumentKind::PresentationReceipt => {
                parse_character_only::<PresentationReceipt>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_presentation_receipt,
                );
            }
            CharacterHealthDocumentKind::RelationshipKindPack => {
                match parse_health_source::<RelationshipKindPack>(source, reference.format) {
                    Ok(value) => {
                        let result = validate_relationship_kind_pack(&value);
                        let valid = result.is_ok();
                        if let Err(failure) = result {
                            push_relationship_failure(diagnostics, reference, failure.diagnostic());
                        }
                        record_parsed_document(
                            &mut parsed,
                            reference,
                            source,
                            &value,
                            valid,
                            diagnostics,
                        );
                        parsed.relationship_packs.push(TypedHealthDocument {
                            document_id: reference.id.clone(),
                            value,
                            valid,
                        });
                    }
                    Err(()) => record_parse_failure(&mut parsed, reference, diagnostics),
                }
            }
            CharacterHealthDocumentKind::RelationshipPolicy => {
                match parse_health_source::<RelationshipGraphPolicy>(source, reference.format) {
                    Ok(value) => {
                        let result = validate_relationship_graph_policy(&value);
                        let valid = result.is_ok();
                        if let Err(failure) = result {
                            push_relationship_failure(diagnostics, reference, failure.diagnostic());
                        }
                        record_parsed_document(
                            &mut parsed,
                            reference,
                            source,
                            &value,
                            valid,
                            diagnostics,
                        );
                        parsed.relationship_policies.push(TypedHealthDocument {
                            document_id: reference.id.clone(),
                            value,
                            valid,
                        });
                    }
                    Err(()) => record_parse_failure(&mut parsed, reference, diagnostics),
                }
            }
            CharacterHealthDocumentKind::ExpressionPack => {
                match parse_health_source::<ExpressionPack>(source, reference.format) {
                    Ok(value) => {
                        let result = validate_expression_pack(&value);
                        let valid = result.is_ok();
                        if let Err(failure) = result {
                            push_expression_failure(diagnostics, reference, failure.diagnostic());
                        }
                        record_parsed_document(
                            &mut parsed,
                            reference,
                            source,
                            &value,
                            valid,
                            diagnostics,
                        );
                        parsed.expression_packs.push(TypedHealthDocument {
                            document_id: reference.id.clone(),
                            value,
                            valid,
                        });
                    }
                    Err(()) => record_parse_failure(&mut parsed, reference, diagnostics),
                }
            }
            CharacterHealthDocumentKind::ExpressionRevision => {
                parse_expression_only::<ExpressionRevision>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_expression_revision_structure,
                );
            }
            CharacterHealthDocumentKind::ProjectionPack => {
                character_document!(ProjectionPack, validate_projection_pack, projection_packs);
            }
            CharacterHealthDocumentKind::ProjectionProposal => {
                character_document!(
                    ProjectionProposal,
                    validate_projection_proposal,
                    projection_proposals
                );
            }
            CharacterHealthDocumentKind::ProjectionReview => {
                character_document!(
                    ProjectionReview,
                    validate_projection_review_structure,
                    projection_reviews
                );
            }
            CharacterHealthDocumentKind::ProjectionReceipt => {
                character_document!(
                    ProjectionReceipt,
                    validate_projection_receipt,
                    projection_receipts
                );
            }
            CharacterHealthDocumentKind::AssistanceTemplate => {
                parse_character_only::<AssistanceTemplate>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_assistance_template,
                );
            }
            CharacterHealthDocumentKind::AssistanceCandidateSet => {
                character_document!(
                    AssistanceCandidateSet,
                    validate_assistance_candidate_set,
                    assistance_candidate_sets
                );
            }
            CharacterHealthDocumentKind::AssistanceAdvisoryReview => {
                parse_character_only::<AssistanceAdvisoryReview>(
                    &mut parsed,
                    reference,
                    source,
                    diagnostics,
                    validate_assistance_advisory_review_structure,
                );
            }
            CharacterHealthDocumentKind::AssistanceDecisionReview => {
                character_document!(
                    AssistanceDecisionReview,
                    validate_assistance_decision_review_structure,
                    assistance_decision_reviews
                );
            }
            CharacterHealthDocumentKind::AssistanceReceipt => {
                character_document!(
                    AssistanceReceipt,
                    validate_assistance_receipt,
                    assistance_receipts
                );
            }
            CharacterHealthDocumentKind::AssistanceJob => {
                character_document!(AssistanceJob, validate_assistance_job, assistance_jobs);
            }
            CharacterHealthDocumentKind::AssistanceBatchReceipt => {
                character_document!(
                    AssistanceBatchReceipt,
                    validate_assistance_batch_receipt,
                    assistance_batch_receipts
                );
            }
            CharacterHealthDocumentKind::RuntimeDomainPack => {
                match parse_health_source::<DomainPack>(source, reference.format) {
                    Ok(value) => {
                        let valid = validate_runtime_pack_shape(&value).is_ok();
                        if !valid {
                            push_health_diagnostic(
                                diagnostics,
                                CharacterHealthDiagnosticCode::InvalidDocument,
                                CharacterHealthSeverity::Error,
                                document_scope(reference),
                                "runtime_pack",
                                Some(source_location(reference)),
                                "runtime Character pack does not match the portable domain-pack boundary",
                                "Regenerate the runtime pack from a validated Character profile and module contract.",
                            );
                        }
                        record_parsed_document(
                            &mut parsed,
                            reference,
                            source,
                            &value,
                            valid,
                            diagnostics,
                        );
                        parsed.runtime_packs.push(TypedHealthDocument {
                            document_id: reference.id.clone(),
                            value,
                            valid,
                        });
                    }
                    Err(()) => record_parse_failure(&mut parsed, reference, diagnostics),
                }
            }
        }
    }
    parsed
}

fn parse_character_only<T: DeserializeOwned + Serialize>(
    parsed: &mut ParsedHealthProject,
    reference: &CharacterHealthDocumentRef,
    source: &str,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    validate: impl FnOnce(&T) -> Result<(), CharacterError>,
) {
    match parse_health_source::<T>(source, reference.format) {
        Ok(value) => {
            let result = validate(&value);
            let valid = result.is_ok();
            if let Err(failure) = result {
                push_character_failure(diagnostics, reference, failure.diagnostic());
            }
            record_parsed_document(parsed, reference, source, &value, valid, diagnostics);
        }
        Err(()) => record_parse_failure(parsed, reference, diagnostics),
    }
}

fn parse_expression_only<T: DeserializeOwned + Serialize>(
    parsed: &mut ParsedHealthProject,
    reference: &CharacterHealthDocumentRef,
    source: &str,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    validate: impl FnOnce(&T) -> Result<(), ExpressionError>,
) {
    match parse_health_source::<T>(source, reference.format) {
        Ok(value) => {
            let result = validate(&value);
            let valid = result.is_ok();
            if let Err(failure) = result {
                push_expression_failure(diagnostics, reference, failure.diagnostic());
            }
            record_parsed_document(parsed, reference, source, &value, valid, diagnostics);
        }
        Err(()) => record_parse_failure(parsed, reference, diagnostics),
    }
}

fn parse_health_source<T: DeserializeOwned>(
    source: &str,
    format: CharacterHealthDocumentFormat,
) -> Result<T, ()> {
    match format {
        CharacterHealthDocumentFormat::Json => parse_strict_json(source).map_err(|_| ()),
        CharacterHealthDocumentFormat::Ron => ron::from_str(source).map_err(|_| ()),
    }
}

fn record_parsed_document<T: Serialize>(
    parsed: &mut ParsedHealthProject,
    reference: &CharacterHealthDocumentRef,
    source: &str,
    value: &T,
    valid: bool,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let semantic = to_pretty_json(value).ok();
    let semantic_sha256 = semantic.as_deref().map(health_sha256);
    if valid {
        let canonical = match reference.format {
            CharacterHealthDocumentFormat::Json => semantic,
            CharacterHealthDocumentFormat::Ron => to_pretty_ron(value).ok(),
        };
        if canonical.as_deref() != Some(source) {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::NonCanonicalOrdering,
                CharacterHealthSeverity::Warning,
                document_scope(reference),
                "document",
                Some(source_location(reference)),
                "valid source is not in canonical deterministic field and collection order",
                "Normalize the source through the matching Weave Character serializer and review the byte-level diff.",
            );
        }
    }
    parsed.records.insert(
        reference.id.clone(),
        ParsedHealthDocument {
            semantic_sha256,
            valid,
        },
    );
}

fn record_parse_failure(
    parsed: &mut ParsedHealthProject,
    reference: &CharacterHealthDocumentRef,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    parsed.records.insert(
        reference.id.clone(),
        ParsedHealthDocument {
            semantic_sha256: None,
            valid: false,
        },
    );
    push_health_diagnostic(
        diagnostics,
        CharacterHealthDiagnosticCode::InvalidDocument,
        CharacterHealthSeverity::Error,
        document_scope(reference),
        "document",
        Some(source_location(reference)),
        "source cannot be decoded as its declared strict Character document kind and encoding",
        "Restore every required field, remove unknown or duplicate fields, and validate the declared JSON or RON document.",
    );
}

fn validate_runtime_pack_shape(pack: &DomainPack) -> Result<(), ()> {
    if pack.pack_format_version != 1
        || pack.module.id != CHARACTER_MODULE_ID
        || pack.values.keys().map(String::as_str).ne(["profile"])
        || validate_provenance(&pack.provenance).is_err()
    {
        Err(())
    } else {
        Ok(())
    }
}

fn health_sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn detect_missing_required_fields(
    reference: &CharacterHealthDocumentRef,
    source: &str,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    if reference.format != CharacterHealthDocumentFormat::Json {
        return;
    }
    let Ok(value) = parse_strict_json::<serde_json::Value>(source) else {
        return;
    };
    let required: &[(&str, &str)] = match reference.kind {
        CharacterHealthDocumentKind::CharacterCollection => &[
            ("collection_format_version", "/collection_format_version"),
            ("id", "/id"),
            ("revision", "/revision"),
            ("characters", "/characters"),
        ],
        CharacterHealthDocumentKind::CharacterProfile => &[
            ("profile_format_version", "/profile_format_version"),
            ("id", "/id"),
            ("canon", "/canon"),
            ("derived", "/derived"),
            ("provenance", "/provenance"),
        ],
        _ => &[],
    };
    for (field, pointer) in required {
        if value.pointer(pointer).is_none() {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::MissingRequiredField,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                *field,
                Some(source_location(reference)),
                "source omits a required field from its declared Character document contract",
                "Add the required typed field using the current schema, then rerun strict validation.",
            );
        }
    }
    if reference.kind == CharacterHealthDocumentKind::CharacterCollection {
        let Some(characters) = value
            .get("characters")
            .and_then(serde_json::Value::as_object)
        else {
            return;
        };
        for (character_id, profile) in characters {
            for (field, pointer) in [
                ("profile_format_version", "/profile_format_version"),
                ("id", "/id"),
                ("canon", "/canon"),
                ("derived", "/derived"),
                ("provenance", "/provenance"),
            ] {
                if profile.pointer(pointer).is_none() {
                    push_health_diagnostic(
                        diagnostics,
                        CharacterHealthDiagnosticCode::MissingRequiredField,
                        CharacterHealthSeverity::Error,
                        CharacterHealthDiagnosticScope::CharacterDocument {
                            document_id: reference.id.clone(),
                            character_id: character_id.clone(),
                        },
                        format!("characters.{character_id}.{field}"),
                        Some(source_location(reference)),
                        "collection profile omits a required field from the current Character contract",
                        "Add the required typed profile field using the current schema, then rerun the audit.",
                    );
                }
            }
            for (field, pointer) in [
                ("identity", "/canon/identity"),
                ("personality", "/canon/personality"),
            ] {
                if profile.pointer(pointer).is_none() {
                    push_health_diagnostic(
                        diagnostics,
                        CharacterHealthDiagnosticCode::MissingRequiredField,
                        CharacterHealthSeverity::Error,
                        CharacterHealthDiagnosticScope::CharacterDocument {
                            document_id: reference.id.clone(),
                            character_id: character_id.clone(),
                        },
                        format!("characters.{character_id}.canon.{field}"),
                        Some(source_location(reference)),
                        "collection profile omits required canonical Character data",
                        "Restore the required canonical record from reviewed source data without fabricating missing values.",
                    );
                }
            }
        }
    }
}

fn contains_sensitive_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let assignment = [
        "api_key=",
        "api_key:",
        "\"api_key\":",
        "api-key:",
        "apikey=",
        "apikey:",
        "\"apikey\":",
        "access_token=",
        "access_token:",
        "\"access_token\":",
        "client_secret=",
        "client_secret:",
        "\"client_secret\":",
        "github_token=",
        "github_token:",
        "\"github_token\":",
        "database_url=",
        "database_url:",
        "\"database_url\":",
        "password=",
        "password:",
        "\"password\":",
        "authorization: bearer ",
    ]
    .into_iter()
    .any(|marker| {
        lower.match_indices(marker).any(|(index, _)| {
            let remainder = lower[index + marker.len()..].trim_start();
            let remainder = remainder
                .strip_prefix('"')
                .or_else(|| remainder.strip_prefix('\''))
                .unwrap_or(remainder);
            !remainder.starts_with("[redacted]")
        })
    });
    assignment
        || value.contains("-----BEGIN PRIVATE KEY-----")
        || value.contains("-----BEGIN RSA PRIVATE KEY-----")
        || has_token_prefix(value, "github_pat_", 20)
        || has_token_prefix(value, "ghp_", 20)
        || has_token_prefix(value, "gho_", 20)
        || has_token_prefix(value, "sk-", 20)
}

fn has_token_prefix(value: &str, prefix: &str, minimum_suffix: usize) -> bool {
    value.match_indices(prefix).any(|(index, _)| {
        value[index + prefix.len()..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .count()
            >= minimum_suffix
    })
}

fn document_scope(reference: &CharacterHealthDocumentRef) -> CharacterHealthDiagnosticScope {
    reference.character_id.as_ref().map_or_else(
        || CharacterHealthDiagnosticScope::Document {
            document_id: reference.id.clone(),
        },
        |character_id| CharacterHealthDiagnosticScope::CharacterDocument {
            document_id: reference.id.clone(),
            character_id: character_id.clone(),
        },
    )
}

fn source_location(reference: &CharacterHealthDocumentRef) -> CharacterHealthSourceLocation {
    CharacterHealthSourceLocation {
        document_id: reference.id.clone(),
        file: reference.path.clone(),
        line: None,
        column: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_health_diagnostic(
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    code: CharacterHealthDiagnosticCode,
    severity: CharacterHealthSeverity,
    scope: CharacterHealthDiagnosticScope,
    path: impl Into<String>,
    source: Option<CharacterHealthSourceLocation>,
    explanation: impl Into<String>,
    remediation: impl Into<String>,
) {
    diagnostics.push(CharacterHealthDiagnostic {
        id: String::new(),
        code,
        severity,
        scope,
        path: path.into(),
        source,
        explanation: explanation.into(),
        remediation: remediation.into(),
        suppressed_by: None,
    });
}

fn push_character_failure(
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    reference: &CharacterHealthDocumentRef,
    diagnostic: &CharacterDiagnostic,
) {
    let (code, remediation) = map_character_code(diagnostic.code, reference.kind);
    push_health_diagnostic(
        diagnostics,
        code,
        CharacterHealthSeverity::Error,
        document_scope(reference),
        diagnostic.path.clone(),
        Some(source_location(reference)),
        diagnostic.message.clone(),
        remediation,
    );
}

fn push_character_corpus_failure(
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    reference: &CharacterHealthDocumentRef,
    diagnostic: &CharacterDiagnostic,
) {
    let (code, remediation) = map_character_code(diagnostic.code, reference.kind);
    push_health_diagnostic(
        diagnostics,
        code,
        CharacterHealthSeverity::Error,
        document_scope(reference),
        diagnostic.path.clone(),
        Some(source_location(reference)),
        diagnostic.message.clone(),
        remediation,
    );
}

fn map_character_code(
    code: CharacterDiagnosticCode,
    kind: CharacterHealthDocumentKind,
) -> (CharacterHealthDiagnosticCode, &'static str) {
    match code {
        CharacterDiagnosticCode::UnsupportedVersion
            if matches!(
                kind,
                CharacterHealthDocumentKind::ProjectionPack
                    | CharacterHealthDocumentKind::ProjectionProposal
                    | CharacterHealthDocumentKind::ProjectionReview
                    | CharacterHealthDocumentKind::ProjectionReceipt
            ) =>
        {
            (
                CharacterHealthDiagnosticCode::UnsupportedProjectionVersion,
                "Migrate the projection artifact through an explicitly supported versioned path before reuse.",
            )
        }
        CharacterDiagnosticCode::UnsupportedVersion => (
            CharacterHealthDiagnosticCode::UnsupportedVersion,
            "Migrate the document through an explicitly supported versioned path before reuse.",
        ),
        CharacterDiagnosticCode::StaleInput => (
            CharacterHealthDiagnosticCode::StaleFingerprint,
            "Recompute the artifact from its exact current inputs and repeat review where required.",
        ),
        CharacterDiagnosticCode::InvalidReference => (
            CharacterHealthDiagnosticCode::UnresolvedReference,
            "Restore the exact referenced artifact or revise the reference through a reviewed operation.",
        ),
        CharacterDiagnosticCode::ForbiddenWriteBack | CharacterDiagnosticCode::LockedField => (
            CharacterHealthDiagnosticCode::ProtectedFieldAuthority,
            "Restore protected data and use an explicit authorized, reviewed override if policy permits it.",
        ),
        CharacterDiagnosticCode::ConflictingOverlay
            if kind == CharacterHealthDocumentKind::CharacterOverlay =>
        {
            (
                CharacterHealthDiagnosticCode::InvalidTemplateOverlay,
                "Normalize overlay operations and resolve duplicate or conflicting target ownership.",
            )
        }
        _ => (
            CharacterHealthDiagnosticCode::InvalidDocument,
            "Correct the typed field through the matching Character schema and validator.",
        ),
    }
}

fn push_relationship_failure(
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    reference: &CharacterHealthDocumentRef,
    diagnostic: &RelationshipDiagnostic,
) {
    let (code, remediation) = map_relationship_code(diagnostic.code);
    push_health_diagnostic(
        diagnostics,
        code,
        health_severity(diagnostic.severity),
        document_scope(reference),
        diagnostic.path.clone(),
        Some(source_location(reference)),
        diagnostic.message.clone(),
        remediation,
    );
}

fn map_relationship_code(
    code: RelationshipDiagnosticCode,
) -> (CharacterHealthDiagnosticCode, &'static str) {
    match code {
        RelationshipDiagnosticCode::BrokenInverse => (
            CharacterHealthDiagnosticCode::BrokenRelationshipInverse,
            "Add or repair the exact inverse or symmetric edge through a reviewed graph revision.",
        ),
        RelationshipDiagnosticCode::ContradictoryPedigree => (
            CharacterHealthDiagnosticCode::PedigreeConflict,
            "Review the pedigree assertions and remove the contradictory edge through an explicit revision.",
        ),
        RelationshipDiagnosticCode::InvalidDateRange => (
            CharacterHealthDiagnosticCode::InvalidRelationshipDate,
            "Correct the proleptic-Gregorian bounds without inventing unknown date components.",
        ),
        RelationshipDiagnosticCode::DuplicateEdge => (
            CharacterHealthDiagnosticCode::DuplicateRelationship,
            "Retain one authoritative edge and remove the duplicate through a reviewed graph revision.",
        ),
        RelationshipDiagnosticCode::StaleEvidence => (
            CharacterHealthDiagnosticCode::StaleAffinityEvidence,
            "Recompute advisory affinity evidence from the current pinned inputs before review.",
        ),
        RelationshipDiagnosticCode::AgeSafeguard => (
            CharacterHealthDiagnosticCode::AgeSafeguard,
            "Review the configured age safeguard and either revise the edge or record a justified permitted exception.",
        ),
        RelationshipDiagnosticCode::KinshipSafeguard => (
            CharacterHealthDiagnosticCode::KinshipSafeguard,
            "Review the configured kinship safeguard and either revise the edge or record a justified permitted exception.",
        ),
        RelationshipDiagnosticCode::PartnershipSafeguard => (
            CharacterHealthDiagnosticCode::PartnershipSafeguard,
            "Review the configured partnership limit and resolve the conflicting active edges explicitly.",
        ),
        RelationshipDiagnosticCode::ConsentSafeguard => (
            CharacterHealthDiagnosticCode::ConsentSafeguard,
            "Record explicit reviewed consent or remove the relationship assertion; absence is never consent.",
        ),
        RelationshipDiagnosticCode::MissingCharacter
        | RelationshipDiagnosticCode::InvalidKind
        | RelationshipDiagnosticCode::InvalidMetadata => (
            CharacterHealthDiagnosticCode::UnresolvedReference,
            "Restore the missing roster, kind-pack, or metadata reference and rerun graph validation.",
        ),
        RelationshipDiagnosticCode::LockedEdge => (
            CharacterHealthDiagnosticCode::ProtectedFieldAuthority,
            "Preserve the locked edge unless an explicit authorized graph revision overrides it.",
        ),
        RelationshipDiagnosticCode::ForbiddenSelfEdge => (
            CharacterHealthDiagnosticCode::InvalidDocument,
            "Remove the forbidden self-edge or select a kind that explicitly permits it.",
        ),
    }
}

fn push_expression_failure(
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    reference: &CharacterHealthDocumentRef,
    diagnostic: &ExpressionDiagnostic,
) {
    let (code, remediation) = map_expression_code(diagnostic.code);
    let mut source = source_location(reference);
    if let Some(expression_source) = &diagnostic.source {
        source.line = Some(expression_source.line);
        source.column = Some(expression_source.column);
    }
    push_health_diagnostic(
        diagnostics,
        code,
        match diagnostic.severity {
            ExpressionDiagnosticSeverity::Warning => CharacterHealthSeverity::Warning,
            ExpressionDiagnosticSeverity::Error => CharacterHealthSeverity::Error,
        },
        document_scope(reference),
        diagnostic.path.clone(),
        Some(source),
        diagnostic.message.clone(),
        remediation,
    );
}

fn map_expression_code(
    code: ExpressionDiagnosticCode,
) -> (CharacterHealthDiagnosticCode, &'static str) {
    match code {
        ExpressionDiagnosticCode::InvalidLink | ExpressionDiagnosticCode::StalePack => (
            CharacterHealthDiagnosticCode::MissingExpressionLink,
            "Restore the exact expression record or pack coordinate, then repeat assignment review.",
        ),
        ExpressionDiagnosticCode::InvalidPlaceholder
        | ExpressionDiagnosticCode::UnavailablePlaceholder
        | ExpressionDiagnosticCode::UnresolvedToken => (
            CharacterHealthDiagnosticCode::PlaceholderViolation,
            "Declare only approved placeholders and ensure every runtime value is available before selection.",
        ),
        ExpressionDiagnosticCode::EmptyText | ExpressionDiagnosticCode::TextLimit => (
            CharacterHealthDiagnosticCode::ExpressionText,
            "Revise expression text within the documented non-empty length limits.",
        ),
        ExpressionDiagnosticCode::ConflictingConstraint => (
            CharacterHealthDiagnosticCode::ExpressionConstraintConflict,
            "Resolve overlapping expression constraints through an explicit reviewed revision.",
        ),
        ExpressionDiagnosticCode::UnsafePersonalization => (
            CharacterHealthDiagnosticCode::UnsafePersonalization,
            "Remove the restricted personalization field and use only the declared safe runtime context.",
        ),
        ExpressionDiagnosticCode::UnreviewedSuggestion => (
            CharacterHealthDiagnosticCode::UnreviewedExpressionSuggestion,
            "Accept, edit, or reject the suggestion explicitly before runtime use.",
        ),
        ExpressionDiagnosticCode::DuplicateVariant => (
            CharacterHealthDiagnosticCode::DuplicateExpressionVariant,
            "Retain one reviewed variant or rewrite it so each alternative is meaningfully distinct.",
        ),
        ExpressionDiagnosticCode::NearDuplicateVariant => (
            CharacterHealthDiagnosticCode::NearDuplicateExpressionVariant,
            "Review similar variants and consolidate or differentiate them intentionally.",
        ),
        ExpressionDiagnosticCode::InvalidStructure => (
            CharacterHealthDiagnosticCode::InvalidDocument,
            "Correct the expression artifact through its current strict schema.",
        ),
    }
}

const fn health_severity(value: DiagnosticSeverity) -> CharacterHealthSeverity {
    match value {
        DiagnosticSeverity::Warning => CharacterHealthSeverity::Warning,
        DiagnosticSeverity::Error => CharacterHealthSeverity::Error,
    }
}

fn assign_diagnostic_ids(
    diagnostics: &mut [CharacterHealthDiagnostic],
) -> Result<(), CharacterHealthError> {
    for diagnostic in diagnostics {
        let scope = to_pretty_json(&diagnostic.scope).map_err(|_| health_encoding_error())?;
        let mut hasher = Sha256::new();
        fingerprint_part(&mut hasher, diagnostic.code.as_str().as_bytes());
        fingerprint_part(&mut hasher, scope.as_bytes());
        fingerprint_part(&mut hasher, diagnostic.path.as_bytes());
        fingerprint_part(&mut hasher, diagnostic.explanation.as_bytes());
        let digest = format!("{:x}", hasher.finalize());
        diagnostic.id = format!("health_{}", &digest[..16]);
    }
    Ok(())
}

fn sort_health_diagnostics(diagnostics: &mut Vec<CharacterHealthDiagnostic>) {
    diagnostics.sort_by(|left, right| {
        (&left.scope, &left.path, left.code, &left.id).cmp(&(
            &right.scope,
            &right.path,
            right.code,
            &right.id,
        ))
    });
    diagnostics.dedup_by(|left, right| left.id == right.id);
}

fn apply_health_suppressions(
    suppressions: &[CharacterHealthSuppression],
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    manifest: &CharacterHealthManifest,
) -> Result<Vec<CharacterHealthSuppressionResult>, CharacterHealthError> {
    let mut results = Vec::new();
    for suppression in suppressions {
        if suppression.code == CharacterHealthDiagnosticCode::SensitiveValue {
            let scope = match (&suppression.document_id, &suppression.character_id) {
                (Some(document_id), Some(character_id)) => {
                    CharacterHealthDiagnosticScope::CharacterDocument {
                        document_id: document_id.clone(),
                        character_id: character_id.clone(),
                    }
                }
                (Some(document_id), None) => CharacterHealthDiagnosticScope::Document {
                    document_id: document_id.clone(),
                },
                (None, Some(character_id)) => CharacterHealthDiagnosticScope::Character {
                    character_id: character_id.clone(),
                },
                (None, None) => CharacterHealthDiagnosticScope::Collection,
            };
            let source = suppression.document_id.as_ref().and_then(|document_id| {
                manifest
                    .documents
                    .iter()
                    .find(|document| document.id == *document_id)
                    .map(source_location)
            });
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::InvalidSuppression,
                CharacterHealthSeverity::Error,
                scope,
                format!("suppressions.{}", suppression.id),
                source,
                "credential-shaped source values cannot be hidden by a health suppression",
                "Remove the suppression and replace the complete sensitive value with [REDACTED].",
            );
            results.push(CharacterHealthSuppressionResult {
                suppression_id: suppression.id.clone(),
                reviewed_by: suppression.reviewed_by.clone(),
                rationale: suppression.rationale.clone(),
                review_revision: suppression.review_revision,
                matched_diagnostic_ids: Vec::new(),
            });
            continue;
        }
        let mut matched = Vec::new();
        for diagnostic in diagnostics.iter_mut() {
            if diagnostic.code == suppression.code
                && suppression
                    .document_id
                    .as_deref()
                    .is_none_or(|id| diagnostic.scope.document_id() == Some(id))
                && suppression
                    .character_id
                    .as_deref()
                    .is_none_or(|id| diagnostic.scope.character_id() == Some(id))
                && diagnostic.path.starts_with(&suppression.path_prefix)
                && diagnostic.suppressed_by.is_none()
            {
                diagnostic.suppressed_by = Some(suppression.id.clone());
                matched.push(diagnostic.id.clone());
            }
        }
        matched.sort();
        matched.dedup();
        if matched.is_empty() {
            let scope = suppression.document_id.as_ref().map_or(
                CharacterHealthDiagnosticScope::Collection,
                |document_id| CharacterHealthDiagnosticScope::Document {
                    document_id: document_id.clone(),
                },
            );
            let source = suppression.document_id.as_ref().and_then(|document_id| {
                manifest
                    .documents
                    .iter()
                    .find(|document| document.id == *document_id)
                    .map(source_location)
            });
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::UnusedSuppression,
                CharacterHealthSeverity::Note,
                scope,
                format!("suppressions.{}", suppression.id),
                source,
                "reviewed suppression does not match any current diagnostic",
                "Remove the obsolete suppression or update its reviewed scope for an existing diagnostic.",
            );
        }
        results.push(CharacterHealthSuppressionResult {
            suppression_id: suppression.id.clone(),
            reviewed_by: suppression.reviewed_by.clone(),
            rationale: suppression.rationale.clone(),
            review_revision: suppression.review_revision,
            matched_diagnostic_ids: matched,
        });
    }
    Ok(results)
}

fn summarize_health(
    documents_total: u64,
    documents_valid: u64,
    characters_total: u64,
    diagnostics: &[CharacterHealthDiagnostic],
    failure_threshold: Option<CharacterHealthSeverity>,
) -> CharacterHealthSummary {
    let mut active_by_severity = BTreeMap::new();
    let mut active = 0u64;
    let mut suppressed = 0u64;
    for diagnostic in diagnostics {
        if diagnostic.suppressed_by.is_some() {
            suppressed += 1;
        } else {
            active += 1;
            *active_by_severity.entry(diagnostic.severity).or_insert(0) += 1;
        }
    }
    let ci_exit_code = failure_threshold.map_or(0, |threshold| {
        u8::from(diagnostics.iter().any(|diagnostic| {
            diagnostic.suppressed_by.is_none() && diagnostic.severity >= threshold
        })) * 2
    });
    CharacterHealthSummary {
        documents_total,
        documents_valid,
        characters_total,
        diagnostics_total: diagnostics.len() as u64,
        active_diagnostics: active,
        suppressed_diagnostics: suppressed,
        active_by_severity,
        failure_threshold,
        ci_exit_code,
    }
}

fn audit_primary_collection(
    project: &CharacterHealthProject,
    primary: &TypedHealthDocument<CharacterCollection>,
    parsed: &ParsedHealthProject,
    coverage: &mut CharacterHealthCoverage,
    distributions: &mut CharacterHealthDistributions,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let Some(reference) = health_reference(&project.manifest, &primary.document_id) else {
        return;
    };
    let expression_packs = parsed
        .expression_packs
        .iter()
        .filter(|document| document.valid)
        .map(|document| document.value.clone())
        .collect::<Vec<_>>();
    let projection_refs = parsed
        .projection_packs
        .iter()
        .filter(|document| document.valid)
        .filter_map(|document| {
            projection_pack_fingerprint(&document.value)
                .ok()
                .map(|sha256| ProjectionPackRef {
                    id: document.value.id.clone(),
                    version: document.value.version.clone(),
                    sha256,
                })
        })
        .collect::<BTreeSet<_>>();
    for (map_id, profile) in &primary.value.characters {
        let scope = CharacterHealthDiagnosticScope::CharacterDocument {
            document_id: primary.document_id.clone(),
            character_id: map_id.clone(),
        };
        if profile.id != *map_id {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::DuplicateIdentifier,
                CharacterHealthSeverity::Error,
                scope.clone(),
                format!("characters.{map_id}.id"),
                Some(source_location(reference)),
                "profile identifier does not match its unique collection key",
                "Rename the profile through the reference-safe collection operation so its key and stable id match.",
            );
        }
        if let Err(failure) = validate_profile(profile) {
            let (code, remediation) = map_character_code(failure.diagnostic().code, reference.kind);
            push_health_diagnostic(
                diagnostics,
                code,
                CharacterHealthSeverity::Error,
                scope.clone(),
                format!("characters.{map_id}.{}", failure.diagnostic().path),
                Some(source_location(reference)),
                failure.diagnostic().message.clone(),
                remediation,
            );
        }
        if profile.derived.ocean != derive_ocean(&profile.canon.personality) {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::InconsistentDerivedView,
                CharacterHealthSeverity::Error,
                scope.clone(),
                format!("characters.{map_id}.derived.ocean"),
                Some(source_location(reference)),
                "derived OCEAN values do not reproduce from the canonical HEXACO evidence",
                "Recompute derived views from canon; never edit a derived field directly.",
            );
        }
        for namespace in &project.manifest.policy.required_extension_namespaces {
            if !profile.extensions.contains_key(namespace) {
                push_health_diagnostic(
                    diagnostics,
                    CharacterHealthDiagnosticCode::CoveragePolicy,
                    CharacterHealthSeverity::Warning,
                    scope.clone(),
                    format!("characters.{map_id}.extensions.{namespace}"),
                    Some(source_location(reference)),
                    "profile is missing an extension explicitly required by project health policy",
                    "Author or import the required extension through its reviewed workflow, or revise the documented project policy.",
                );
            }
        }
        let character_coverage = inspect_profile_health(
            map_id,
            profile,
            scope.clone(),
            reference,
            &projection_refs,
            distributions,
            diagnostics,
        );
        coverage.factor_values_present += u64::from(character_coverage.factors_present);
        coverage.factor_values_total += u64::from(character_coverage.factors_total);
        coverage.facet_values_present += u64::from(character_coverage.facets_present);
        coverage.facet_values_total += u64::from(character_coverage.facets_total);
        coverage.known_confidence_values += u64::from(character_coverage.known_confidence_values);
        coverage.measured_values += u64::from(character_coverage.measured_values);
        coverage
            .characters
            .insert(map_id.clone(), character_coverage);

        if validate_profile(profile).is_ok()
            && let Ok(report) = lint_expression(profile, &expression_packs)
        {
            for diagnostic in &report.diagnostics {
                push_expression_character_diagnostic(diagnostics, reference, map_id, diagnostic);
            }
        }
        if project.manifest.policy.csv_loss_diagnostics && profile_has_csv_loss(profile) {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::CsvLoss,
                CharacterHealthSeverity::Note,
                scope,
                format!("characters.{map_id}"),
                Some(source_location(reference)),
                "optional CSV projection would omit typed or provenance-bearing Character data",
                "Use canonical JSON or RON for round trips and treat CSV only as a review export.",
            );
        }
    }
    audit_relationship_graphs(project, primary, parsed, diagnostics);
    audit_duplicate_standalone_profile_ids(project, parsed, diagnostics);
}

#[allow(clippy::too_many_arguments)]
fn inspect_profile_health(
    character_id: &str,
    profile: &CharacterProfile,
    scope: CharacterHealthDiagnosticScope,
    reference: &CharacterHealthDocumentRef,
    projection_refs: &BTreeSet<ProjectionPackRef>,
    distributions: &mut CharacterHealthDistributions,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) -> CharacterHealthCharacterCoverage {
    let mut result = CharacterHealthCharacterCoverage {
        factors_total: 6,
        facets_total: 24,
        pending_suggestions: profile.suggestions.len() as u32,
        ..CharacterHealthCharacterCoverage::default()
    };
    for trait_id in ALL_HEXACO_TRAITS {
        let is_factor = is_factor_trait(trait_id);
        let path = format!("characters.{character_id}.{}", trait_path(trait_id));
        let Some(value) = trait_value(&profile.canon.personality, trait_id) else {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::IncompleteFacetConfidence,
                CharacterHealthSeverity::Note,
                scope.clone(),
                path,
                Some(source_location(reference)),
                if is_factor {
                    "canonical HEXACO factor is absent and remains explicitly unmeasured"
                } else {
                    "canonical HEXACO facet is absent and remains explicitly unmeasured"
                },
                "Add evidence only when it is available; otherwise retain the absence and review coverage intentionally.",
            );
            continue;
        };
        result.measured_values += 1;
        if is_factor {
            result.factors_present += 1;
        } else {
            result.facets_present += 1;
        }
        if value.confidence != Confidence::Unknown {
            result.known_confidence_values += 1;
        } else {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::IncompleteFacetConfidence,
                CharacterHealthSeverity::Warning,
                scope.clone(),
                format!("{path}.confidence"),
                Some(source_location(reference)),
                "measured HEXACO value has explicitly unknown confidence",
                "Review the supporting evidence and set only the confidence that the representation supports.",
            );
        }
        if value.freshness == Freshness::Current {
            result.current_values += 1;
        } else {
            result.stale_values += 1;
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::StaleFingerprint,
                CharacterHealthSeverity::Warning,
                scope.clone(),
                format!("{path}.freshness"),
                Some(source_location(reference)),
                "canonical measurement is explicitly marked stale against its lineage",
                "Review the upstream evidence and revise or reaffirm the canonical measurement through an authorized operation.",
            );
        }
        *distributions
            .trait_bands
            .entry(trait_id)
            .or_default()
            .entry(measurement_band(value.value))
            .or_insert(0) += 1;
    }
    audit_attributed_freshness(
        character_id,
        "canon.identity.display_name",
        &profile.canon.identity.display_name,
        &scope,
        reference,
        diagnostics,
    );
    if let Some(aliases) = &profile.canon.identity.aliases {
        audit_attributed_freshness(
            character_id,
            "canon.identity.aliases",
            aliases,
            &scope,
            reference,
            diagnostics,
        );
    }
    if let Some(birth_date) = &profile.canon.birth_date {
        audit_attributed_freshness(
            character_id,
            "canon.birth_date",
            birth_date,
            &scope,
            reference,
            diagnostics,
        );
    }
    for (namespace, extension) in &profile.extensions {
        let header = extension_header(extension);
        if header.freshness == Freshness::Stale {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::StaleFingerprint,
                CharacterHealthSeverity::Warning,
                scope.clone(),
                format!("characters.{character_id}.extensions.{namespace}.header.freshness"),
                Some(source_location(reference)),
                "accepted extension is explicitly stale against its pinned source lineage",
                "Recompute or review the extension using its exact current packs and source inputs.",
            );
        }
        if header.review == ReviewState::Pending {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::UnreviewedProjection,
                CharacterHealthSeverity::Warning,
                scope.clone(),
                format!("characters.{character_id}.extensions.{namespace}.header.review"),
                Some(source_location(reference)),
                "extension remains pending review and is not accepted runtime authority",
                "Accept, edit, reject, or remove the extension through its complete review workflow.",
            );
        }
    }
    if let Some(CharacterExtension::Relationships(record)) =
        profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE)
    {
        result.relationship_edges = record.value.edges.len() as u32;
        for edge in record.value.edges.values() {
            *distributions
                .relationship_kinds
                .entry(edge.kind.clone())
                .or_insert(0) += 1;
        }
    }
    if let Some(CharacterExtension::Expression(record)) =
        profile.extensions.get(EXPRESSION_EXTENSION_NAMESPACE)
    {
        result.expression_records = expression_record_count(&record.value);
        for value in record.value.lexicon.values() {
            *distributions
                .expression_categories
                .entry(value.category.clone())
                .or_insert(0) += 1;
        }
        for value in record.value.preferences.values() {
            *distributions
                .expression_categories
                .entry(value.category.clone())
                .or_insert(0) += 1;
        }
    }
    if let Some(CharacterExtension::RoleProjections(record)) =
        profile.extensions.get(ROLE_PROJECTION_EXTENSION_NAMESPACE)
    {
        result.role_projections = record.value.roles.len() as u32;
        for (role_id, role) in &record.value.roles {
            *distributions
                .role_taxonomies
                .entry(role.taxonomy.clone())
                .or_insert(0) += 1;
            if role.decision != ProjectionPublicDecision::Authored
                && role.decision != ProjectionPublicDecision::Overridden
                && role
                    .pack
                    .as_ref()
                    .is_none_or(|pack| !projection_refs.contains(pack))
            {
                push_health_diagnostic(
                    diagnostics,
                    CharacterHealthDiagnosticCode::MissingPackProvenance,
                    CharacterHealthSeverity::Error,
                    scope.clone(),
                    format!(
                        "characters.{character_id}.extensions.{ROLE_PROJECTION_EXTENSION_NAMESPACE}.value.roles.{role_id}.pack"
                    ),
                    Some(source_location(reference)),
                    "accepted role projection no longer resolves to its exact immutable source pack",
                    "Restore the exact pack coordinate or re-review the projection against an explicitly selected current pack.",
                );
            }
            if role.score_micros.is_some()
                && (role.input_paths.is_empty() || role.explanation.trim().is_empty())
            {
                push_health_diagnostic(
                    diagnostics,
                    CharacterHealthDiagnosticCode::UnexplainedProjectionScore,
                    CharacterHealthSeverity::Error,
                    scope.clone(),
                    format!(
                        "characters.{character_id}.extensions.{ROLE_PROJECTION_EXTENSION_NAMESPACE}.value.roles.{role_id}"
                    ),
                    Some(source_location(reference)),
                    "scored role projection lacks complete input paths or an inspectable explanation",
                    "Recompute the projection with retained evidence traces before editorial review.",
                );
            }
        }
    }
    result
}

fn is_factor_trait(value: HexacoTrait) -> bool {
    matches!(
        value,
        HexacoTrait::HonestyHumility
            | HexacoTrait::Emotionality
            | HexacoTrait::Extraversion
            | HexacoTrait::Agreeableness
            | HexacoTrait::Conscientiousness
            | HexacoTrait::Openness
    )
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

fn audit_attributed_freshness<T>(
    character_id: &str,
    path: &str,
    value: &Attributed<T>,
    scope: &CharacterHealthDiagnosticScope,
    reference: &CharacterHealthDocumentRef,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    if value.freshness == Freshness::Stale {
        push_health_diagnostic(
            diagnostics,
            CharacterHealthDiagnosticCode::StaleFingerprint,
            CharacterHealthSeverity::Warning,
            scope.clone(),
            format!("characters.{character_id}.{path}.freshness"),
            Some(source_location(reference)),
            "canonical attributed value is explicitly marked stale against its lineage",
            "Review its upstream source and revise or reaffirm the value through an authorized operation.",
        );
    }
}

fn extension_header(extension: &CharacterExtension) -> &ExtensionHeader {
    match extension {
        CharacterExtension::IdentityPresentation(record) => &record.header,
        CharacterExtension::Expression(record) => &record.header,
        CharacterExtension::BehavioralSignatures(record) => &record.header,
        CharacterExtension::RoleProjections(record) => &record.header,
        CharacterExtension::Relationships(record) => &record.header,
        CharacterExtension::AlignmentView(record) => &record.header,
        CharacterExtension::DateContext(record) => &record.header,
        CharacterExtension::Tabletop(record) => &record.header,
        CharacterExtension::Opaque(record) => &record.header,
    }
}

fn expression_record_count(value: &ExpressionData) -> u32 {
    (value.lexicon.len()
        + value.preferences.len()
        + value.vocabulary_pools.len()
        + value.voice_constraints.len()
        + value.template_assignments.len()) as u32
}

fn profile_has_csv_loss(profile: &CharacterProfile) -> bool {
    profile.canon.identity.aliases.is_some()
        || profile.canon.birth_date.is_some()
        || !profile.canon.inner_life.is_empty()
        || !profile.canon.voice.is_empty()
        || !profile.extensions.is_empty()
        || !profile.suggestions.is_empty()
        || !profile.provenance.transformations.is_empty()
}

fn push_expression_character_diagnostic(
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
    reference: &CharacterHealthDocumentRef,
    character_id: &str,
    diagnostic: &ExpressionDiagnostic,
) {
    let (code, remediation) = map_expression_code(diagnostic.code);
    let mut source = source_location(reference);
    if let Some(expression_source) = &diagnostic.source {
        source.line = Some(expression_source.line);
        source.column = Some(expression_source.column);
    }
    push_health_diagnostic(
        diagnostics,
        code,
        match diagnostic.severity {
            ExpressionDiagnosticSeverity::Warning => CharacterHealthSeverity::Warning,
            ExpressionDiagnosticSeverity::Error => CharacterHealthSeverity::Error,
        },
        CharacterHealthDiagnosticScope::CharacterDocument {
            document_id: reference.id.clone(),
            character_id: character_id.to_owned(),
        },
        format!("characters.{character_id}.{}", diagnostic.path),
        Some(source),
        diagnostic.message.clone(),
        remediation,
    );
}

fn audit_duplicate_standalone_profile_ids(
    project: &CharacterHealthProject,
    parsed: &ParsedHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let mut owners = BTreeMap::<&str, (&str, &str)>::new();
    for document in &parsed.profiles {
        let Some(reference) = health_reference(&project.manifest, &document.document_id) else {
            continue;
        };
        match owners.entry(&document.value.id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((&reference.logical_id, &reference.id));
            }
            std::collections::btree_map::Entry::Occupied(entry)
                if entry.get().0 != reference.logical_id =>
            {
                push_health_diagnostic(
                    diagnostics,
                    CharacterHealthDiagnosticCode::DuplicateIdentifier,
                    CharacterHealthSeverity::Error,
                    document_scope(reference),
                    "id",
                    Some(source_location(reference)),
                    "different logical profile documents claim the same stable character identifier",
                    "Assign unique stable identifiers or declare the documents as one RON/JSON semantic pair.",
                );
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }
}

fn audit_relationship_graphs(
    project: &CharacterHealthProject,
    primary: &TypedHealthDocument<CharacterCollection>,
    parsed: &ParsedHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let Some(reference) = health_reference(&project.manifest, &primary.document_id) else {
        return;
    };
    let available_packs = parsed
        .relationship_packs
        .iter()
        .filter(|document| document.valid)
        .filter_map(|document| {
            relationship_kind_pack_ref(&document.value)
                .ok()
                .map(|pack_ref| (pack_ref, &document.value))
        })
        .collect::<BTreeMap<_, _>>();
    for (character_id, profile) in &primary.value.characters {
        let Some(CharacterExtension::Relationships(record)) =
            profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE)
        else {
            continue;
        };
        if record.value.edges.is_empty() {
            continue;
        }
        let scope = CharacterHealthDiagnosticScope::CharacterDocument {
            document_id: primary.document_id.clone(),
            character_id: character_id.clone(),
        };
        match &record.value.kind_pack {
            Some(pack_ref) if available_packs.contains_key(pack_ref) => {}
            _ => push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::MissingPackProvenance,
                CharacterHealthSeverity::Error,
                scope,
                format!(
                    "characters.{character_id}.extensions.{RELATIONSHIP_EXTENSION_NAMESPACE}.value.kind_pack"
                ),
                Some(source_location(reference)),
                "relationship graph no longer resolves to its exact immutable kind pack",
                "Restore the exact kind pack or migrate the graph through a reviewed revision.",
            ),
        }
    }
    let Some(policy) = parsed
        .relationship_policies
        .iter()
        .find(|document| document.valid)
        .map(|document| &document.value)
    else {
        return;
    };
    for (pack_ref, pack) in available_packs {
        let mut scoped_collection = primary.value.clone();
        for profile in scoped_collection.characters.values_mut() {
            let keep = matches!(
                profile.extensions.get(RELATIONSHIP_EXTENSION_NAMESPACE),
                Some(CharacterExtension::Relationships(record))
                    if record.value.kind_pack.as_ref() == Some(&pack_ref)
            );
            if !keep {
                profile.extensions.remove(RELATIONSHIP_EXTENSION_NAMESPACE);
            }
        }
        if validate_character_collection(&scoped_collection).is_err() {
            continue;
        }
        let Ok(values) = relationship_graph_diagnostics(
            &scoped_collection,
            pack,
            policy.reference_date,
            &policy.safeguards,
        ) else {
            continue;
        };
        for diagnostic in values {
            let character_id = relationship_character_from_path(&diagnostic.path);
            let scope = character_id.as_ref().map_or_else(
                || document_scope(reference),
                |character_id| CharacterHealthDiagnosticScope::CharacterDocument {
                    document_id: primary.document_id.clone(),
                    character_id: character_id.clone(),
                },
            );
            let (mut code, remediation) = map_relationship_code(diagnostic.code);
            if diagnostic.code == RelationshipDiagnosticCode::BrokenInverse
                && diagnostic.message.contains("symmetric")
            {
                code = CharacterHealthDiagnosticCode::RelationshipSymmetry;
            }
            push_health_diagnostic(
                diagnostics,
                code,
                health_severity(diagnostic.severity),
                scope,
                diagnostic.path,
                Some(source_location(reference)),
                diagnostic.message,
                remediation,
            );
        }
    }
}

fn relationship_character_from_path(path: &str) -> Option<String> {
    let suffix = path.strip_prefix("characters.")?;
    suffix.split('.').next().map(ToOwned::to_owned)
}

fn health_reference<'a>(
    manifest: &'a CharacterHealthManifest,
    document_id: &str,
) -> Option<&'a CharacterHealthDocumentRef> {
    manifest
        .documents
        .iter()
        .find(|reference| reference.id == document_id)
}

fn audit_portability(
    project: &CharacterHealthProject,
    parsed: &ParsedHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let mut groups = BTreeMap::<
        (CharacterHealthDocumentKind, &str),
        BTreeMap<CharacterHealthDocumentFormat, &CharacterHealthDocumentRef>,
    >::new();
    for reference in &project.manifest.documents {
        groups
            .entry((reference.kind, &reference.logical_id))
            .or_default()
            .insert(reference.format, reference);
    }
    for ((kind, logical_id), formats) in groups {
        let json = formats.get(&CharacterHealthDocumentFormat::Json);
        let ron = formats.get(&CharacterHealthDocumentFormat::Ron);
        if project.manifest.policy.require_portable_pairs
            && portable_pair_kind(kind)
            && (json.is_none() || ron.is_none())
        {
            let reference = json.or(ron).copied();
            let (scope, source) = reference.map_or(
                (CharacterHealthDiagnosticScope::Collection, None),
                |reference| (document_scope(reference), Some(source_location(reference))),
            );
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::RonJsonMismatch,
                CharacterHealthSeverity::Warning,
                scope,
                format!("portable.{logical_id}"),
                source,
                "portable Character artifact is missing its equivalent canonical RON or JSON representation",
                "Generate both encodings from the same validated typed value and retain their shared logical identifier.",
            );
            continue;
        }
        let (Some(json), Some(ron)) = (json, ron) else {
            continue;
        };
        let json_hash = parsed
            .records
            .get(&json.id)
            .and_then(|record| record.semantic_sha256.as_deref());
        let ron_hash = parsed
            .records
            .get(&ron.id)
            .and_then(|record| record.semantic_sha256.as_deref());
        if json_hash.is_some() && ron_hash.is_some() && json_hash != ron_hash {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::RonJsonMismatch,
                CharacterHealthSeverity::Error,
                document_scope(json),
                format!("portable.{logical_id}"),
                Some(source_location(json)),
                "canonical RON and JSON documents decode to different typed Character semantics",
                "Choose the reviewed canonical value, regenerate both encodings from it, and review the semantic diff.",
            );
        }
    }
}

const fn portable_pair_kind(kind: CharacterHealthDocumentKind) -> bool {
    matches!(
        kind,
        CharacterHealthDocumentKind::CharacterCollection
            | CharacterHealthDocumentKind::CharacterProfile
            | CharacterHealthDocumentKind::CharacterTemplate
            | CharacterHealthDocumentKind::CharacterOverlay
            | CharacterHealthDocumentKind::CharacterSynthesis
            | CharacterHealthDocumentKind::RuntimeDomainPack
    )
}

fn audit_template_overlay_lineage(
    project: &CharacterHealthProject,
    parsed: &ParsedHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let mut templates = BTreeMap::<(String, String), (&CharacterTemplate, String)>::new();
    for document in parsed.templates.iter().filter(|document| document.valid) {
        if let Ok(fingerprint) = template_fingerprint(&document.value) {
            templates.insert(
                (document.value.id.clone(), document.value.version.clone()),
                (&document.value, fingerprint),
            );
        }
    }
    for document in &parsed.overlays {
        let Some(reference) = health_reference(&project.manifest, &document.document_id) else {
            continue;
        };
        let Some(selected) = &document.value.template else {
            continue;
        };
        let Some((template, fingerprint)) =
            templates.get(&(selected.id.clone(), selected.version.clone()))
        else {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::UnresolvedReference,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "template",
                Some(source_location(reference)),
                "overlay references a template coordinate unavailable to the health project",
                "Restore the exact immutable template document or migrate the overlay explicitly.",
            );
            continue;
        };
        if fingerprint != &selected.sha256 {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::StaleFingerprint,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "template.sha256",
                Some(source_location(reference)),
                "overlay template fingerprint no longer matches the available immutable template",
                "Review the template change and regenerate or migrate the overlay against the selected version.",
            );
            continue;
        }
        if document.valid && synthesize_character(Some(template), &document.value).is_err() {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::InvalidTemplateOverlay,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "operations",
                Some(source_location(reference)),
                "overlay cannot be reproduced safely over its exact immutable template",
                "Resolve locked, stale, conflicting, or unauthorized operations before applying the overlay.",
            );
        }
    }
}

fn audit_projection_artifacts(
    project: &CharacterHealthProject,
    parsed: &ParsedHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let primary = parsed.primary_collection(&project.manifest);
    let primary_collection = primary.map(|document| &document.value);
    let mut reviewed = BTreeSet::new();
    for review in parsed
        .projection_reviews
        .iter()
        .filter(|document| document.valid)
    {
        reviewed.insert(review.value.proposal_sha256.clone());
    }
    for receipt in parsed
        .projection_receipts
        .iter()
        .filter(|document| document.valid)
    {
        reviewed.insert(receipt.value.proposal_sha256.clone());
    }
    for proposal in &parsed.projection_proposals {
        let Some(reference) = health_reference(&project.manifest, &proposal.document_id) else {
            continue;
        };
        let fingerprint = projection_proposal_fingerprint(&proposal.value).ok();
        if fingerprint
            .as_ref()
            .is_some_and(|fingerprint| !reviewed.contains(fingerprint))
        {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::UnreviewedProjection,
                CharacterHealthSeverity::Warning,
                document_scope(reference),
                "review",
                Some(source_location(reference)),
                "projection proposal has no complete review or reproducible receipt in the health project",
                "Accept, edit, override, reject, or withhold every proposal slot through a complete review.",
            );
        }
        if let Some(current) = primary_collection
            && proposal.value.input_collection != *current
        {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::StaleProjection,
                CharacterHealthSeverity::Warning,
                document_scope(reference),
                "input_collection",
                Some(source_location(reference)),
                "projection proposal was computed from a different Character collection revision",
                "Regenerate the projection from the current collection before making editorial decisions.",
            );
        }
    }
    let proposal_hashes = parsed
        .projection_proposals
        .iter()
        .filter_map(|proposal| projection_proposal_fingerprint(&proposal.value).ok())
        .chain(
            parsed
                .projection_receipts
                .iter()
                .map(|receipt| receipt.value.proposal_sha256.clone()),
        )
        .collect::<BTreeSet<_>>();
    for review in &parsed.projection_reviews {
        if !proposal_hashes.contains(&review.value.proposal_sha256) {
            let Some(reference) = health_reference(&project.manifest, &review.document_id) else {
                continue;
            };
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::UnresolvedReference,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "proposal_sha256",
                Some(source_location(reference)),
                "projection review references a proposal unavailable to the health project",
                "Restore the exact immutable proposal or discard the orphaned review.",
            );
        }
    }
}

fn audit_assistance_artifacts(
    project: &CharacterHealthProject,
    parsed: &ParsedHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let primary = parsed.primary_collection(&project.manifest);
    let current_profiles = primary
        .map(|document| &document.value.characters)
        .cloned()
        .unwrap_or_default();
    let mut reviewed_sets = BTreeSet::new();
    for review in parsed
        .assistance_decision_reviews
        .iter()
        .filter(|document| document.valid)
    {
        reviewed_sets.insert(review.value.candidate_set_sha256.clone());
    }
    let mut accepted_sources = BTreeMap::<String, BTreeSet<String>>::new();
    for receipt in parsed
        .assistance_receipts
        .iter()
        .filter(|document| document.valid)
    {
        reviewed_sets.insert(receipt.value.candidate_set_sha256.clone());
        accepted_sources
            .entry(receipt.value.output_profile.id.clone())
            .or_default()
            .extend(receipt.value.accepted_suggestion_ids.iter().cloned());
    }
    for receipt in parsed
        .assistance_batch_receipts
        .iter()
        .filter(|document| document.valid)
    {
        for (character_id, child) in &receipt.value.receipts {
            reviewed_sets.insert(child.candidate_set_sha256.clone());
            accepted_sources
                .entry(character_id.clone())
                .or_default()
                .extend(child.accepted_suggestion_ids.iter().cloned());
        }
    }
    for set in &parsed.assistance_candidate_sets {
        let Some(reference) = health_reference(&project.manifest, &set.document_id) else {
            continue;
        };
        let fingerprint = assistance_candidate_set_fingerprint(&set.value).ok();
        if fingerprint
            .as_ref()
            .is_some_and(|fingerprint| !reviewed_sets.contains(fingerprint))
        {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::UnreviewedAssistanceCandidate,
                CharacterHealthSeverity::Warning,
                CharacterHealthDiagnosticScope::CharacterDocument {
                    document_id: reference.id.clone(),
                    character_id: set.value.input_profile.id.clone(),
                },
                "decisions",
                Some(source_location(reference)),
                "assistance candidate set has no complete author decision review in the health project",
                "Accept, edit, reject, defer, or regenerate every immutable candidate explicitly.",
            );
        }
        if current_profiles
            .get(&set.value.input_profile.id)
            .is_some_and(|profile| profile != &set.value.input_profile)
        {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::StaleAssistanceCandidate,
                CharacterHealthSeverity::Warning,
                CharacterHealthDiagnosticScope::CharacterDocument {
                    document_id: reference.id.clone(),
                    character_id: set.value.input_profile.id.clone(),
                },
                "input_profile_sha256",
                Some(source_location(reference)),
                "assistance candidates were generated from a different current profile value",
                "Regenerate candidates from a newly approved disclosure preview before review.",
            );
        }
        let provider = &set.value.preview.request.provider;
        if (provider.mode == AssistanceProviderMode::Offline && provider.credential_required)
            || (provider.mode == AssistanceProviderMode::External && provider.adapter_id.is_empty())
        {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::ProviderMetadataPolicy,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "provider",
                Some(source_location(reference)),
                "assistance provider metadata violates the credential-free provider-neutral policy",
                "Use a pinned adapter coordinate and keep credential values exclusively in the host secret channel.",
            );
        }
    }
    for job in &parsed.assistance_jobs {
        let Some(reference) = health_reference(&project.manifest, &job.document_id) else {
            continue;
        };
        if primary.is_some_and(|primary| primary.value != job.value.input_collection) {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::StaleAssistanceCandidate,
                CharacterHealthSeverity::Warning,
                document_scope(reference),
                "input_collection_sha256",
                Some(source_location(reference)),
                "resumable assistance job is pinned to a different collection revision",
                "Start a new batch preview and approval from the current collection rather than resuming stale work.",
            );
        }
    }
    let Some(primary) = primary else {
        return;
    };
    let primary_reference = health_reference(&project.manifest, &primary.document_id);
    for (character_id, profile) in &primary.value.characters {
        for suggestion_id in profile
            .suggestions
            .keys()
            .filter(|id| id.starts_with("assist_"))
        {
            if !accepted_sources
                .get(character_id)
                .is_some_and(|sources| sources.contains(suggestion_id))
            {
                push_health_diagnostic(
                    diagnostics,
                    CharacterHealthDiagnosticCode::MissingAcceptedSource,
                    CharacterHealthSeverity::Error,
                    CharacterHealthDiagnosticScope::CharacterDocument {
                        document_id: primary.document_id.clone(),
                        character_id: character_id.clone(),
                    },
                    format!("characters.{character_id}.suggestions.{suggestion_id}"),
                    primary_reference.map(source_location),
                    "accepted assistance suggestion no longer has its reproducible receipt and candidate source",
                    "Restore the exact receipt or remove and regenerate the suggestion through a complete author review.",
                );
            }
        }
    }
}

fn audit_runtime_packs(
    project: &CharacterHealthProject,
    parsed: &ParsedHealthProject,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    let Some(primary) = parsed.primary_collection(&project.manifest) else {
        return;
    };
    let mut represented = BTreeSet::new();
    for document in &parsed.runtime_packs {
        let Some(reference) = health_reference(&project.manifest, &document.document_id) else {
            continue;
        };
        let Some(character_id) = &reference.character_id else {
            continue;
        };
        represented.insert(character_id.clone());
        let Some(profile) = primary.value.characters.get(character_id) else {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::MissingRuntimeReference,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "character_id",
                Some(source_location(reference)),
                "runtime pack owner is absent from the canonical Character collection",
                "Restore the canonical profile or remove the orphaned runtime pack reference.",
            );
            continue;
        };
        if document.value.values.get("profile") != Some(&character_profile_domain_value(profile)) {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::RuntimeAuthorityBoundary,
                CharacterHealthSeverity::Error,
                document_scope(reference),
                "values.profile",
                Some(source_location(reference)),
                "runtime pack does not equal the deterministic authoring-to-runtime Character projection",
                "Regenerate the runtime pack from the current validated profile; do not copy suggestions or authoring-only fields into runtime authority.",
            );
        }
    }
    if !parsed.runtime_packs.is_empty() {
        let primary_reference = health_reference(&project.manifest, &primary.document_id);
        for character_id in primary.value.characters.keys() {
            if !represented.contains(character_id) {
                push_health_diagnostic(
                    diagnostics,
                    CharacterHealthDiagnosticCode::MissingRuntimeReference,
                    CharacterHealthSeverity::Warning,
                    CharacterHealthDiagnosticScope::CharacterDocument {
                        document_id: primary.document_id.clone(),
                        character_id: character_id.clone(),
                    },
                    format!("characters.{character_id}.runtime_pack"),
                    primary_reference.map(source_location),
                    "canonical profile has no required portable runtime pack in this audited output set",
                    "Generate and include the immutable Character domain pack for this profile.",
                );
            }
        }
    }
}

fn audit_distribution_constraints(
    policy: &CharacterHealthPolicy,
    distributions: &CharacterHealthDistributions,
    character_count: u64,
    diagnostics: &mut Vec<CharacterHealthDiagnostic>,
) {
    for constraint in &policy.distribution_constraints {
        let value = match &constraint.metric {
            CharacterHealthDistributionMetric::CharacterCount => character_count,
            CharacterHealthDistributionMetric::TraitBand { trait_id, band } => distributions
                .trait_bands
                .get(trait_id)
                .and_then(|values| values.get(band))
                .copied()
                .unwrap_or(0),
            CharacterHealthDistributionMetric::RelationshipKind { kind_id } => distributions
                .relationship_kinds
                .get(kind_id)
                .copied()
                .unwrap_or(0),
            CharacterHealthDistributionMetric::RoleTaxonomy { taxonomy } => distributions
                .role_taxonomies
                .get(taxonomy)
                .copied()
                .unwrap_or(0),
            CharacterHealthDistributionMetric::ExpressionCategory { category } => distributions
                .expression_categories
                .get(category)
                .copied()
                .unwrap_or(0),
        };
        if constraint.minimum.is_some_and(|minimum| value < minimum)
            || constraint.maximum.is_some_and(|maximum| value > maximum)
        {
            push_health_diagnostic(
                diagnostics,
                CharacterHealthDiagnosticCode::DistributionPolicy,
                CharacterHealthSeverity::Warning,
                CharacterHealthDiagnosticScope::Collection,
                format!("policy.distribution_constraints.{}", constraint.id),
                None,
                "descriptive corpus distribution is outside an explicitly configured documented project constraint",
                "Review the project-specific policy and corpus; the base Character contract imposes no demographic, personality, role, relationship, or expression quota.",
            );
        }
    }
}

/// Return a presentation-only filtered clone while preserving the immutable source fingerprint.
pub fn filter_character_health_report(
    report: &CharacterHealthReport,
    filter: &CharacterHealthFilter,
) -> Result<CharacterHealthReport, CharacterHealthError> {
    validate_character_health_report(report)?;
    let mut filtered = report.clone();
    filtered.diagnostics.retain(|diagnostic| {
        (filter.character_ids.is_empty()
            || diagnostic
                .scope
                .character_id()
                .is_some_and(|id| filter.character_ids.contains(id)))
            && (filter.codes.is_empty() || filter.codes.contains(&diagnostic.code))
            && filter
                .minimum_severity
                .is_none_or(|minimum| diagnostic.severity >= minimum)
            && (filter.include_suppressed || diagnostic.suppressed_by.is_none())
    });
    let retained_ids = filtered
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.id.as_str())
        .collect::<BTreeSet<_>>();
    for suppression in &mut filtered.suppressions {
        suppression
            .matched_diagnostic_ids
            .retain(|id| retained_ids.contains(id.as_str()));
    }
    if !filter.character_ids.is_empty() {
        filtered
            .coverage
            .characters
            .retain(|character_id, _| filter.character_ids.contains(character_id));
    }
    filtered.summary = summarize_health(
        report.summary.documents_total,
        report.summary.documents_valid,
        if filter.character_ids.is_empty() {
            report.summary.characters_total
        } else {
            filtered.coverage.characters.len() as u64
        },
        &filtered.diagnostics,
        report.summary.failure_threshold,
    );
    validate_character_health_report(&filtered)?;
    Ok(filtered)
}

/// Render every JSON report field as deterministic, human-readable text without source payloads.
pub fn render_character_health_text(
    report: &CharacterHealthReport,
) -> Result<String, CharacterHealthError> {
    validate_character_health_report(report)?;
    let mut output = String::new();
    use fmt::Write as _;
    writeln!(output, "Character health report").map_err(|_| health_encoding_error())?;
    writeln!(
        output,
        "report_format_version: {}",
        report.report_format_version
    )
    .map_err(|_| health_encoding_error())?;
    writeln!(output, "id: {}", report.id).map_err(|_| health_encoding_error())?;
    writeln!(output, "manifest_id: {}", report.manifest_id).map_err(|_| health_encoding_error())?;
    writeln!(output, "input_sha256: {}", report.input_sha256)
        .map_err(|_| health_encoding_error())?;
    writeln!(output, "read_only: {}", report.read_only).map_err(|_| health_encoding_error())?;
    writeln!(
        output,
        "source_payloads_retained: {}",
        report.source_payloads_retained
    )
    .map_err(|_| health_encoding_error())?;
    writeln!(
        output,
        "summary: documents_valid={} documents_total={} characters_total={} diagnostics_total={} active_diagnostics={} suppressed_diagnostics={} ci_exit_code={}",
        report.summary.documents_valid,
        report.summary.documents_total,
        report.summary.characters_total,
        report.summary.diagnostics_total,
        report.summary.active_diagnostics,
        report.summary.suppressed_diagnostics,
        report.summary.ci_exit_code,
    )
    .map_err(|_| health_encoding_error())?;
    for severity in [
        CharacterHealthSeverity::Note,
        CharacterHealthSeverity::Warning,
        CharacterHealthSeverity::Error,
    ] {
        writeln!(
            output,
            "summary active_{}={}",
            health_severity_name(severity),
            report
                .summary
                .active_by_severity
                .get(&severity)
                .copied()
                .unwrap_or_default(),
        )
        .map_err(|_| health_encoding_error())?;
    }
    writeln!(
        output,
        "summary failure_threshold={}",
        report
            .summary
            .failure_threshold
            .map_or("none", health_severity_name),
    )
    .map_err(|_| health_encoding_error())?;
    writeln!(
        output,
        "coverage: factor_values_present={} factor_values_total={} facet_values_present={} facet_values_total={} known_confidence_values={} measured_values={}",
        report.coverage.factor_values_present,
        report.coverage.factor_values_total,
        report.coverage.facet_values_present,
        report.coverage.facet_values_total,
        report.coverage.known_confidence_values,
        report.coverage.measured_values,
    )
    .map_err(|_| health_encoding_error())?;
    for (trait_id, bands) in &report.distributions.trait_bands {
        for (band, count) in bands {
            writeln!(
                output,
                "distribution trait {} {}={count}",
                render_serialized_name(trait_id)?,
                render_serialized_name(band)?,
            )
            .map_err(|_| health_encoding_error())?;
        }
    }
    for (kind, count) in &report.distributions.relationship_kinds {
        writeln!(output, "distribution relationship_kind {kind}={count}")
            .map_err(|_| health_encoding_error())?;
    }
    for (taxonomy, count) in &report.distributions.role_taxonomies {
        writeln!(output, "distribution role_taxonomy {taxonomy}={count}")
            .map_err(|_| health_encoding_error())?;
    }
    for (category, count) in &report.distributions.expression_categories {
        writeln!(
            output,
            "distribution expression_category {category}={count}"
        )
        .map_err(|_| health_encoding_error())?;
    }
    for (character_id, coverage) in &report.coverage.characters {
        writeln!(
            output,
            "character {character_id}: factors={}/{} facets={}/{} confidence={}/{} current={} stale={} relationships={} expressions={} roles={} suggestions={}",
            coverage.factors_present,
            coverage.factors_total,
            coverage.facets_present,
            coverage.facets_total,
            coverage.known_confidence_values,
            coverage.measured_values,
            coverage.current_values,
            coverage.stale_values,
            coverage.relationship_edges,
            coverage.expression_records,
            coverage.role_projections,
            coverage.pending_suggestions,
        )
        .map_err(|_| health_encoding_error())?;
    }
    writeln!(
        output,
        "distributions: trait_bands={} relationship_kinds={} role_taxonomies={} expression_categories={} constraints_configured={}",
        report.distributions.trait_bands.len(),
        report.distributions.relationship_kinds.len(),
        report.distributions.role_taxonomies.len(),
        report.distributions.expression_categories.len(),
        report.distributions.constraints_configured,
    )
    .map_err(|_| health_encoding_error())?;
    for diagnostic in &report.diagnostics {
        let status = diagnostic
            .suppressed_by
            .as_ref()
            .map_or("active", |_| "suppressed");
        writeln!(
            output,
            "[{status}] {} {} {} {}",
            diagnostic.code.as_str(),
            health_severity_name(diagnostic.severity),
            render_health_scope(&diagnostic.scope),
            diagnostic.path,
        )
        .map_err(|_| health_encoding_error())?;
        writeln!(output, "  id: {}", diagnostic.id).map_err(|_| health_encoding_error())?;
        if let Some(source) = &diagnostic.source {
            write!(output, "  source: {} ({})", source.file, source.document_id)
                .map_err(|_| health_encoding_error())?;
            if let Some(line) = source.line {
                write!(output, ":{line}").map_err(|_| health_encoding_error())?;
                if let Some(column) = source.column {
                    write!(output, ":{column}").map_err(|_| health_encoding_error())?;
                }
            }
            writeln!(output).map_err(|_| health_encoding_error())?;
        }
        writeln!(output, "  explanation: {}", diagnostic.explanation)
            .map_err(|_| health_encoding_error())?;
        writeln!(output, "  remediation: {}", diagnostic.remediation)
            .map_err(|_| health_encoding_error())?;
        if let Some(suppression) = &diagnostic.suppressed_by {
            writeln!(output, "  suppression: {suppression}")
                .map_err(|_| health_encoding_error())?;
        }
    }
    for suppression in &report.suppressions {
        writeln!(
            output,
            "suppression {} review_revision={} reviewer={} matches={}",
            suppression.suppression_id,
            suppression.review_revision,
            suppression.reviewed_by,
            suppression.matched_diagnostic_ids.join(","),
        )
        .map_err(|_| health_encoding_error())?;
        writeln!(output, "  rationale: {}", suppression.rationale)
            .map_err(|_| health_encoding_error())?;
    }
    Ok(output)
}

fn render_serialized_name<T: Serialize>(value: &T) -> Result<String, CharacterHealthError> {
    serde_json::to_string(value)
        .map(|value| value.trim_matches('"').to_owned())
        .map_err(|_| health_encoding_error())
}

/// Resolve a public `H###` string for CLI and editor filters.
#[must_use]
pub fn character_health_code_from_str(value: &str) -> Option<CharacterHealthDiagnosticCode> {
    use CharacterHealthDiagnosticCode as Code;
    Some(match value {
        "H100" => Code::InvalidDocument,
        "H101" => Code::MissingRequiredField,
        "H102" => Code::UnsupportedVersion,
        "H103" => Code::DuplicateIdentifier,
        "H104" => Code::IncompleteFacetConfidence,
        "H105" => Code::InconsistentDerivedView,
        "H106" => Code::StaleFingerprint,
        "H107" => Code::UnresolvedReference,
        "H108" => Code::InvalidTemplateOverlay,
        "H109" => Code::ProtectedFieldAuthority,
        "H110" => Code::SensitiveValue,
        "H200" => Code::BrokenRelationshipInverse,
        "H201" => Code::RelationshipSymmetry,
        "H202" => Code::PedigreeConflict,
        "H203" => Code::InvalidRelationshipDate,
        "H204" => Code::DuplicateRelationship,
        "H205" => Code::StaleAffinityEvidence,
        "H206" => Code::AgeSafeguard,
        "H207" => Code::KinshipSafeguard,
        "H208" => Code::PartnershipSafeguard,
        "H209" => Code::ConsentSafeguard,
        "H300" => Code::MissingExpressionLink,
        "H301" => Code::PlaceholderViolation,
        "H302" => Code::ExpressionText,
        "H303" => Code::ExpressionConstraintConflict,
        "H304" => Code::UnsafePersonalization,
        "H305" => Code::UnreviewedExpressionSuggestion,
        "H306" => Code::DuplicateExpressionVariant,
        "H307" => Code::NearDuplicateExpressionVariant,
        "H400" => Code::MissingPackProvenance,
        "H401" => Code::UnsupportedProjectionVersion,
        "H402" => Code::UnexplainedProjectionScore,
        "H403" => Code::UnreviewedProjection,
        "H404" => Code::StaleProjection,
        "H410" => Code::UnreviewedAssistanceCandidate,
        "H411" => Code::StaleAssistanceCandidate,
        "H412" => Code::ProviderMetadataPolicy,
        "H413" => Code::MissingAcceptedSource,
        "H500" => Code::RonJsonMismatch,
        "H501" => Code::NonCanonicalOrdering,
        "H502" => Code::MissingRuntimeReference,
        "H503" => Code::RuntimeAuthorityBoundary,
        "H504" => Code::CsvLoss,
        "H600" => Code::CoveragePolicy,
        "H601" => Code::DistributionPolicy,
        "H700" => Code::InvalidSuppression,
        "H701" => Code::UnusedSuppression,
        _ => return None,
    })
}

const fn health_severity_name(value: CharacterHealthSeverity) -> &'static str {
    match value {
        CharacterHealthSeverity::Note => "note",
        CharacterHealthSeverity::Warning => "warning",
        CharacterHealthSeverity::Error => "error",
    }
}

fn render_health_scope(scope: &CharacterHealthDiagnosticScope) -> String {
    match scope {
        CharacterHealthDiagnosticScope::Collection => "collection".to_owned(),
        CharacterHealthDiagnosticScope::Character { character_id } => {
            format!("character:{character_id}")
        }
        CharacterHealthDiagnosticScope::Document { document_id } => {
            format!("document:{document_id}")
        }
        CharacterHealthDiagnosticScope::CharacterDocument {
            document_id,
            character_id,
        } => format!("document:{document_id}/character:{character_id}"),
    }
}
