use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    Provenance, parse_strict_json, to_pretty_json, to_pretty_ron, validate_provenance,
};

use crate::validation::{ALL_HEXACO_TRAITS, trait_value};
use crate::{
    CHARACTER_OVERLAY_FORMAT_VERSION, CHARACTER_TEMPLATE_FORMAT_VERSION, CharacterDiagnostic,
    CharacterDiagnosticCode, CharacterExtension, CharacterOperation, CharacterOverlay,
    CharacterProfile, CharacterTemplate, CharacterTemplateRef, DiagnosticSeverity, Freshness,
    LockState, ReviewState, ValueState, recompute_derived, synthesize_character,
    template_fingerprint, validate_overlay, validate_profile,
};

/// Current serialized Character collection version.
pub const CHARACTER_COLLECTION_FORMAT_VERSION: u32 = 1;
/// Current serialized collection-operation request version.
pub const CHARACTER_OPERATION_REQUEST_FORMAT_VERSION: u32 = 1;
/// Current serialized reviewable proposal version.
pub const CHARACTER_PROPOSAL_FORMAT_VERSION: u32 = 1;
/// Current serialized proposal-review version.
pub const CHARACTER_REVIEW_FORMAT_VERSION: u32 = 1;
/// Current serialized resumable progress version.
pub const CHARACTER_PROGRESS_FORMAT_VERSION: u32 = 1;

const COLLECTION_SCHEMA_ID: &str = "urn:weave:schema:character-collection:1";
const REQUEST_SCHEMA_ID: &str = "urn:weave:schema:character-operation-request:1";
const PROPOSAL_SCHEMA_ID: &str = "urn:weave:schema:character-proposal:1";
const REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-review:1";
const PROGRESS_SCHEMA_ID: &str = "urn:weave:schema:character-progress:1";

/// A deterministic, versioned collection of independently valid profiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterCollection {
    pub collection_format_version: u32,
    pub id: String,
    /// Monotonic corpus revision incremented by every non-empty atomic application.
    pub revision: u64,
    /// Profiles keyed by the exact stable profile id.
    pub characters: BTreeMap<String, CharacterProfile>,
}

/// One deterministic operation request shared by CLI, editor, and source tooling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterOperationRequest {
    pub request_format_version: u32,
    pub id: String,
    /// Exact canonical input collection hash.
    pub expected_input_sha256: String,
    /// Explicit deterministic seed retained even for operations that use no entropy.
    pub seed: u64,
    pub scope: CharacterScope,
    pub action: CharacterCorpusAction,
    pub provenance: Provenance,
}

/// A single-character, explicit set, or deterministic filtered collection scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum CharacterScope {
    All,
    Characters {
        ids: Vec<String>,
    },
    Filter {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id_prefix: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extension_namespace: Option<String>,
    },
}

/// Closed collection-operation vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
pub enum CharacterCorpusAction {
    Create {
        profile: Box<CharacterProfile>,
    },
    Clone {
        source_id: String,
        new_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        rationale: String,
    },
    Revise {
        character_id: String,
        operations: Vec<CharacterOperation>,
    },
    Rename {
        character_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_display_name: Option<String>,
        override_locked: bool,
        rationale: String,
    },
    Import {
        profiles: Vec<CharacterProfile>,
        force: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    Recompute,
}

/// Complete deterministic dry-run result retained for review and atomic application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterProposal {
    pub proposal_format_version: u32,
    pub id: String,
    pub collection_id: String,
    pub input_sha256: String,
    pub request_sha256: String,
    pub output_sha256: String,
    pub seed: u64,
    /// Canonically sorted units used by resumable proposal generation.
    pub selected_ids: Vec<String>,
    /// Canonically sorted human-reviewable changes without duplicated profile payloads.
    pub changes: Vec<CharacterChange>,
    /// Complete candidate result. Applying it still independently recomputes the request.
    pub resulting_collection: CharacterCollection,
    /// Original request needed for independent reproduction.
    pub request: CharacterOperationRequest,
}

/// One fingerprinted profile or reference change in a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterChange {
    pub id: String,
    pub kind: CharacterChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_sha256: Option<String>,
    /// Sorted owner/path records for stable references changed atomically with this profile.
    pub affected_references: Vec<CharacterReferenceChange>,
}

/// Review-facing category for one collection change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterChangeKind {
    Created,
    Cloned,
    Revised,
    Renamed,
    Imported,
    Recomputed,
    ReferenceUpdated,
}

/// One stable relationship reference affected by an identifier migration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterReferenceChange {
    pub owner_character_id: String,
    pub path: String,
}

/// Whole-proposal review. Partial application is deliberately unsupported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterProposalReview {
    pub review_format_version: u32,
    pub proposal_sha256: String,
    pub input_sha256: String,
    pub decision: CharacterReviewDecision,
    pub reviewer: String,
    pub rationale: String,
}

/// Explicit proposal disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterReviewDecision {
    Accepted,
    Rejected,
}

/// Payload-free resumable proposal-generation cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterJobProgress {
    pub progress_format_version: u32,
    pub collection_id: String,
    pub input_sha256: String,
    pub request_sha256: String,
    pub total: usize,
    pub next_index: usize,
    /// Exact sorted prefix already processed; contains no character values or prose.
    pub completed_target_ids: Vec<String>,
    pub state: CharacterJobState,
}

/// Resumable job state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CharacterJobState {
    Running,
    ReadyForReview,
}

/// One resumable step; proposal appears only after all target ids are accounted for.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterJobStep {
    pub progress: CharacterJobProgress,
    pub proposal: Option<CharacterProposal>,
}

/// Redaction-safe collection operation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterCorpusError {
    diagnostic: CharacterDiagnostic,
}

impl CharacterCorpusError {
    #[must_use]
    pub const fn diagnostic(&self) -> &CharacterDiagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for CharacterCorpusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Character corpus {:?} at `{}`: {}",
            self.diagnostic.code, self.diagnostic.path, self.diagnostic.message
        )
    }
}

impl std::error::Error for CharacterCorpusError {}

/// Validate a complete collection and every contained profile.
pub fn validate_character_collection(
    collection: &CharacterCollection,
) -> Result<(), CharacterCorpusError> {
    validate_collection_structure(collection, false)
}

/// Validate a request independently of any collection state.
pub fn validate_character_operation_request(
    request: &CharacterOperationRequest,
) -> Result<(), CharacterCorpusError> {
    if request.request_format_version != CHARACTER_OPERATION_REQUEST_FORMAT_VERSION {
        return Err(corpus_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "request_format_version",
            "unsupported Character operation request version",
        ));
    }
    validate_namespaced_id("id", &request.id)?;
    validate_sha256("expected_input_sha256", &request.expected_input_sha256)?;
    validate_scope(&request.scope)?;
    validate_provenance(&request.provenance).map_err(|_| {
        corpus_error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "operation request provenance is invalid",
        )
    })?;
    match &request.action {
        CharacterCorpusAction::Create { profile } => {
            validate_profile(profile).map_err(from_profile)
        }
        CharacterCorpusAction::Clone {
            source_id,
            new_id,
            display_name,
            rationale,
        } => {
            validate_namespaced_id("action.source_id", source_id)?;
            validate_namespaced_id("action.new_id", new_id)?;
            if source_id == new_id {
                return Err(invalid_value(
                    "action.new_id",
                    "clone requires a distinct stable identifier",
                ));
            }
            if let Some(display_name) = display_name {
                validate_text("action.display_name", display_name, 1, 256)?;
            }
            validate_text("action.rationale", rationale, 1, 2_048)
        }
        CharacterCorpusAction::Revise {
            character_id,
            operations,
        } => {
            validate_namespaced_id("action.character_id", character_id)?;
            let overlay = CharacterOverlay {
                overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
                id: request.id.clone(),
                character_id: character_id.clone(),
                template: None,
                operations: operations.clone(),
                provenance: request.provenance.clone(),
            };
            validate_overlay(&overlay).map_err(from_profile)
        }
        CharacterCorpusAction::Rename {
            character_id,
            new_id,
            new_display_name,
            rationale,
            ..
        } => {
            validate_namespaced_id("action.character_id", character_id)?;
            if let Some(new_id) = new_id {
                validate_namespaced_id("action.new_id", new_id)?;
            }
            if let Some(display_name) = new_display_name {
                validate_text("action.new_display_name", display_name, 1, 256)?;
            }
            if new_id.is_none() && new_display_name.is_none() {
                return Err(invalid_value(
                    "action",
                    "rename requires a new stable id, display name, or both",
                ));
            }
            validate_text("action.rationale", rationale, 1, 2_048)
        }
        CharacterCorpusAction::Import {
            profiles,
            force,
            rationale,
        } => {
            if profiles.is_empty() || profiles.len() > 65_536 {
                return Err(invalid_value(
                    "action.profiles",
                    "import requires between 1 and 65536 profiles",
                ));
            }
            let mut previous = None;
            for (index, profile) in profiles.iter().enumerate() {
                validate_profile(profile).map_err(from_profile)?;
                if previous.is_some_and(|prior: &str| prior >= profile.id.as_str()) {
                    return Err(invalid_value(
                        "action.profiles",
                        "import profiles must be unique and sorted by stable id",
                    ));
                }
                previous = Some(&profile.id);
                validate_namespaced_id(&format!("action.profiles[{index}].id"), &profile.id)?;
            }
            if *force {
                validate_text(
                    "action.rationale",
                    rationale.as_deref().unwrap_or_default(),
                    1,
                    2_048,
                )?;
            } else if rationale.is_some() {
                return Err(invalid_value(
                    "action.rationale",
                    "non-forced import must omit override rationale",
                ));
            }
            Ok(())
        }
        CharacterCorpusAction::Recompute => Ok(()),
    }
}

/// Deterministically list summaries within one scope.
pub fn list_characters(
    collection: &CharacterCollection,
    scope: &CharacterScope,
) -> Result<Vec<CharacterSummary>, CharacterCorpusError> {
    validate_character_collection(collection)?;
    validate_scope(scope)?;
    selected_existing_ids(collection, scope)?
        .into_iter()
        .map(|id| {
            let profile = &collection.characters[&id];
            Ok(CharacterSummary {
                id,
                display_name: profile.canon.identity.display_name.value.clone(),
                profile_sha256: profile_fingerprint(profile)?,
                extension_namespaces: profile.extensions.keys().cloned().collect(),
            })
        })
        .collect()
}

/// Stable read-only list record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterSummary {
    pub id: String,
    pub display_name: String,
    pub profile_sha256: String,
    pub extension_namespaces: Vec<String>,
}

/// Show one exact profile by stable id.
pub fn show_character<'a>(
    collection: &'a CharacterCollection,
    id: &str,
) -> Result<&'a CharacterProfile, CharacterCorpusError> {
    validate_character_collection(collection)?;
    validate_namespaced_id("id", id)?;
    collection.characters.get(id).ok_or_else(|| {
        corpus_error(
            CharacterDiagnosticCode::InvalidReference,
            "id",
            "character id is absent from the collection",
        )
    })
}

/// Produce a deterministic dry-run proposal without mutating the input collection.
pub fn propose_character_operation(
    collection: &CharacterCollection,
    request: &CharacterOperationRequest,
) -> Result<CharacterProposal, CharacterCorpusError> {
    let allow_stale_derived = matches!(&request.action, CharacterCorpusAction::Recompute);
    validate_collection_structure(collection, allow_stale_derived)?;
    validate_character_operation_request(request)?;
    let input_sha256 = collection_fingerprint(collection)?;
    if request.expected_input_sha256 != input_sha256 {
        return Err(stale(
            "expected_input_sha256",
            "operation request does not fingerprint the current collection",
        ));
    }
    let request_sha256 = request_fingerprint(request)?;
    let selected_ids = operation_targets(collection, request)?;
    let mut result = collection.clone();
    let mut references = BTreeMap::<String, Vec<CharacterReferenceChange>>::new();
    let mut rename_pair = None;
    apply_action(&mut result, request, &mut references, &mut rename_pair)?;
    validate_character_collection(&result)?;
    let changed = result.characters != collection.characters;
    result.revision = collection.revision + u64::from(changed);
    let output_sha256 = collection_fingerprint(&result)?;
    let changes = build_changes(
        collection,
        &result,
        request,
        &request_sha256,
        rename_pair.as_ref(),
        &references,
    )?;
    Ok(CharacterProposal {
        proposal_format_version: CHARACTER_PROPOSAL_FORMAT_VERSION,
        id: format!("{}.proposal", request.id),
        collection_id: collection.id.clone(),
        input_sha256,
        request_sha256,
        output_sha256,
        seed: request.seed,
        selected_ids,
        changes,
        resulting_collection: result,
        request: request.clone(),
    })
}

/// Advance a payload-free deterministic cursor and emit the proposal only when complete.
pub fn resume_character_operation(
    collection: &CharacterCollection,
    request: &CharacterOperationRequest,
    progress: Option<&CharacterJobProgress>,
    max_items: usize,
) -> Result<CharacterJobStep, CharacterCorpusError> {
    if max_items == 0 {
        return Err(invalid_value(
            "max_items",
            "resume step must process at least one item",
        ));
    }
    let allow_stale_derived = matches!(&request.action, CharacterCorpusAction::Recompute);
    validate_collection_structure(collection, allow_stale_derived)?;
    validate_character_operation_request(request)?;
    let input_sha256 = collection_fingerprint(collection)?;
    if request.expected_input_sha256 != input_sha256 {
        return Err(stale(
            "expected_input_sha256",
            "operation request does not fingerprint the current collection",
        ));
    }
    let request_sha256 = request_fingerprint(request)?;
    let targets = operation_targets(collection, request)?;
    let mut next = match progress {
        Some(progress) => {
            validate_character_job_progress(progress)?;
            if progress.collection_id != collection.id
                || progress.input_sha256 != input_sha256
                || progress.request_sha256 != request_sha256
                || progress.total != targets.len()
                || progress.completed_target_ids
                    != targets[..progress.next_index.min(targets.len())]
            {
                return Err(stale(
                    "progress",
                    "progress does not match the exact collection, request, or target ordering",
                ));
            }
            progress.next_index
        }
        None => 0,
    };
    next = next.saturating_add(max_items).min(targets.len());
    let ready = next == targets.len();
    let progress = CharacterJobProgress {
        progress_format_version: CHARACTER_PROGRESS_FORMAT_VERSION,
        collection_id: collection.id.clone(),
        input_sha256,
        request_sha256,
        total: targets.len(),
        next_index: next,
        completed_target_ids: targets[..next].to_vec(),
        state: if ready {
            CharacterJobState::ReadyForReview
        } else {
            CharacterJobState::Running
        },
    };
    let proposal = ready
        .then(|| propose_character_operation(collection, request))
        .transpose()?;
    Ok(CharacterJobStep { progress, proposal })
}

/// Create a byte-stable whole-proposal review.
pub fn review_character_proposal(
    proposal: &CharacterProposal,
    decision: CharacterReviewDecision,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
) -> Result<CharacterProposalReview, CharacterCorpusError> {
    validate_character_proposal(proposal)?;
    let review = CharacterProposalReview {
        review_format_version: CHARACTER_REVIEW_FORMAT_VERSION,
        proposal_sha256: proposal_fingerprint(proposal)?,
        input_sha256: proposal.input_sha256.clone(),
        decision,
        reviewer: reviewer.into(),
        rationale: rationale.into(),
    };
    validate_character_proposal_review(&review)?;
    Ok(review)
}

/// Independently reproduce and atomically return an accepted proposal result.
pub fn apply_reviewed_character_proposal(
    collection: &CharacterCollection,
    proposal: &CharacterProposal,
    review: &CharacterProposalReview,
) -> Result<CharacterCollection, CharacterCorpusError> {
    validate_character_proposal(proposal)?;
    validate_character_proposal_review(review)?;
    if review.decision != CharacterReviewDecision::Accepted {
        return Err(corpus_error(
            CharacterDiagnosticCode::ConflictingOverlay,
            "review.decision",
            "only an accepted whole-proposal review can be applied",
        ));
    }
    if review.proposal_sha256 != proposal_fingerprint(proposal)?
        || review.input_sha256 != proposal.input_sha256
    {
        return Err(stale(
            "review",
            "review fingerprints do not match the exact proposal and input",
        ));
    }
    if collection_fingerprint(collection)? != proposal.input_sha256 {
        return Err(stale(
            "collection",
            "collection changed after the proposal was produced",
        ));
    }
    let reproduced = propose_character_operation(collection, &proposal.request)?;
    if reproduced != *proposal {
        return Err(stale(
            "proposal",
            "proposal does not reproduce from its pinned request and collection",
        ));
    }
    validate_character_collection(&proposal.resulting_collection)?;
    Ok(proposal.resulting_collection.clone())
}

/// Validate a proposal independently of its original input collection.
pub fn validate_character_proposal(
    proposal: &CharacterProposal,
) -> Result<(), CharacterCorpusError> {
    if proposal.proposal_format_version != CHARACTER_PROPOSAL_FORMAT_VERSION {
        return Err(corpus_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "proposal_format_version",
            "unsupported Character proposal version",
        ));
    }
    validate_namespaced_id("id", &proposal.id)?;
    validate_namespaced_id("collection_id", &proposal.collection_id)?;
    for (path, value) in [
        ("input_sha256", &proposal.input_sha256),
        ("request_sha256", &proposal.request_sha256),
        ("output_sha256", &proposal.output_sha256),
    ] {
        validate_sha256(path, value)?;
    }
    validate_character_operation_request(&proposal.request)?;
    if proposal.request_sha256 != request_fingerprint(&proposal.request)? {
        return Err(stale(
            "request_sha256",
            "proposal request fingerprint does not match its embedded request",
        ));
    }
    if proposal.input_sha256 != proposal.request.expected_input_sha256
        || proposal.seed != proposal.request.seed
        || proposal.collection_id != proposal.resulting_collection.id
    {
        return Err(stale(
            "proposal",
            "proposal coordinates do not match its request or result",
        ));
    }
    validate_sorted_ids("selected_ids", &proposal.selected_ids)?;
    validate_changes(&proposal.changes)?;
    validate_character_collection(&proposal.resulting_collection)?;
    if proposal.output_sha256 != collection_fingerprint(&proposal.resulting_collection)? {
        return Err(stale(
            "output_sha256",
            "proposal output fingerprint does not match its candidate collection",
        ));
    }
    Ok(())
}

/// Validate a whole-proposal review independently.
pub fn validate_character_proposal_review(
    review: &CharacterProposalReview,
) -> Result<(), CharacterCorpusError> {
    if review.review_format_version != CHARACTER_REVIEW_FORMAT_VERSION {
        return Err(corpus_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "review_format_version",
            "unsupported Character review version",
        ));
    }
    validate_sha256("proposal_sha256", &review.proposal_sha256)?;
    validate_sha256("input_sha256", &review.input_sha256)?;
    validate_namespaced_id("reviewer", &review.reviewer)?;
    validate_text("rationale", &review.rationale, 1, 2_048)
}

/// Validate a payload-free resumable progress record.
pub fn validate_character_job_progress(
    progress: &CharacterJobProgress,
) -> Result<(), CharacterCorpusError> {
    if progress.progress_format_version != CHARACTER_PROGRESS_FORMAT_VERSION {
        return Err(corpus_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "progress_format_version",
            "unsupported Character progress version",
        ));
    }
    validate_namespaced_id("collection_id", &progress.collection_id)?;
    validate_sha256("input_sha256", &progress.input_sha256)?;
    validate_sha256("request_sha256", &progress.request_sha256)?;
    if progress.next_index > progress.total
        || progress.completed_target_ids.len() != progress.next_index
    {
        return Err(invalid_value(
            "progress",
            "progress cursor and completed-id prefix are inconsistent",
        ));
    }
    validate_sorted_ids("completed_target_ids", &progress.completed_target_ids)?;
    match progress.state {
        CharacterJobState::Running if progress.next_index < progress.total => Ok(()),
        CharacterJobState::ReadyForReview if progress.next_index == progress.total => Ok(()),
        _ => Err(invalid_value(
            "state",
            "progress state does not match the cursor",
        )),
    }
}

/// Canonical collection fingerprint.
pub fn collection_fingerprint(
    collection: &CharacterCollection,
) -> Result<String, CharacterCorpusError> {
    canonical_hash(collection)
}

/// Canonical request fingerprint.
pub fn request_fingerprint(
    request: &CharacterOperationRequest,
) -> Result<String, CharacterCorpusError> {
    canonical_hash(request)
}

/// Canonical proposal fingerprint used by reviews.
pub fn proposal_fingerprint(proposal: &CharacterProposal) -> Result<String, CharacterCorpusError> {
    canonical_hash(proposal)
}

macro_rules! impl_document {
    ($type:ty, $validate:ident) => {
        impl $type {
            pub fn from_json(source: &str) -> Result<Self, CharacterCorpusError> {
                let value = parse_strict_json(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn from_ron(source: &str) -> Result<Self, CharacterCorpusError> {
                let value = ron::from_str(source).map_err(|_| encoding_error())?;
                $validate(&value)?;
                Ok(value)
            }

            pub fn to_json(&self) -> Result<String, CharacterCorpusError> {
                $validate(self)?;
                to_pretty_json(self).map_err(|_| encoding_error())
            }

            pub fn to_ron(&self) -> Result<String, CharacterCorpusError> {
                $validate(self)?;
                to_pretty_ron(self).map_err(|_| encoding_error())
            }
        }
    };
}

impl_document!(CharacterCollection, validate_character_collection);
impl_document!(
    CharacterOperationRequest,
    validate_character_operation_request
);
impl_document!(CharacterProposal, validate_character_proposal);
impl_document!(CharacterProposalReview, validate_character_proposal_review);
impl_document!(CharacterJobProgress, validate_character_job_progress);

pub fn character_collection_schema() -> Result<String, CharacterCorpusError> {
    schema::<CharacterCollection>(COLLECTION_SCHEMA_ID, "Weave Character Collection v1")
}

pub fn character_operation_request_schema() -> Result<String, CharacterCorpusError> {
    schema::<CharacterOperationRequest>(REQUEST_SCHEMA_ID, "Weave Character Operation Request v1")
}

pub fn character_proposal_schema() -> Result<String, CharacterCorpusError> {
    schema::<CharacterProposal>(PROPOSAL_SCHEMA_ID, "Weave Character Proposal v1")
}

pub fn character_review_schema() -> Result<String, CharacterCorpusError> {
    schema::<CharacterProposalReview>(REVIEW_SCHEMA_ID, "Weave Character Review v1")
}

pub fn character_progress_schema() -> Result<String, CharacterCorpusError> {
    schema::<CharacterJobProgress>(PROGRESS_SCHEMA_ID, "Weave Character Progress v1")
}

fn schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, CharacterCorpusError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-operation-version".to_owned(),
            serde_json::Value::from(1),
        );
    }
    sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| encoding_error())
}

fn validate_collection_structure(
    collection: &CharacterCollection,
    allow_stale_derived: bool,
) -> Result<(), CharacterCorpusError> {
    if collection.collection_format_version != CHARACTER_COLLECTION_FORMAT_VERSION {
        return Err(corpus_error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "collection_format_version",
            "unsupported Character collection version",
        ));
    }
    validate_namespaced_id("id", &collection.id)?;
    if collection.characters.len() > 65_536 {
        return Err(invalid_value(
            "characters",
            "collection contains too many profiles",
        ));
    }
    for (id, profile) in &collection.characters {
        if id != &profile.id {
            return Err(corpus_error(
                CharacterDiagnosticCode::InvalidReference,
                format!("characters.{id}.id"),
                "profile id must equal its collection map key",
            ));
        }
        match validate_profile(profile) {
            Ok(()) => {}
            Err(error)
                if allow_stale_derived
                    && error.diagnostic().code == CharacterDiagnosticCode::ForbiddenWriteBack
                    && error.diagnostic().path == "derived.ocean" =>
            {
                let mut repaired = profile.clone();
                recompute_derived(&mut repaired);
                validate_profile(&repaired).map_err(from_profile)?;
            }
            Err(error) => return Err(from_profile(error)),
        }
    }
    Ok(())
}

fn validate_scope(scope: &CharacterScope) -> Result<(), CharacterCorpusError> {
    match scope {
        CharacterScope::All => Ok(()),
        CharacterScope::Characters { ids } => {
            if ids.is_empty() {
                return Err(invalid_value(
                    "scope.ids",
                    "explicit scope must not be empty",
                ));
            }
            validate_sorted_ids("scope.ids", ids)
        }
        CharacterScope::Filter {
            id_prefix,
            extension_namespace,
        } => {
            if id_prefix.is_none() && extension_namespace.is_none() {
                return Err(invalid_value(
                    "scope",
                    "filtered scope requires at least one filter",
                ));
            }
            if let Some(prefix) = id_prefix {
                validate_text("scope.id_prefix", prefix, 1, 256)?;
                if !prefix.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '_'
                        || character == '.'
                }) {
                    return Err(invalid_value(
                        "scope.id_prefix",
                        "id prefix contains unsupported characters",
                    ));
                }
            }
            if let Some(namespace) = extension_namespace {
                validate_namespaced_id("scope.extension_namespace", namespace)?;
            }
            Ok(())
        }
    }
}

fn selected_existing_ids(
    collection: &CharacterCollection,
    scope: &CharacterScope,
) -> Result<Vec<String>, CharacterCorpusError> {
    let ids = match scope {
        CharacterScope::All => collection.characters.keys().cloned().collect(),
        CharacterScope::Characters { ids } => {
            for id in ids {
                if !collection.characters.contains_key(id) {
                    return Err(corpus_error(
                        CharacterDiagnosticCode::InvalidReference,
                        "scope.ids",
                        "explicit scope references an unknown character",
                    ));
                }
            }
            ids.clone()
        }
        CharacterScope::Filter {
            id_prefix,
            extension_namespace,
        } => collection
            .characters
            .iter()
            .filter_map(|(id, profile)| {
                let id_matches = id_prefix
                    .as_ref()
                    .is_none_or(|prefix| id.starts_with(prefix));
                let extension_matches = extension_namespace
                    .as_ref()
                    .is_none_or(|namespace| profile.extensions.contains_key(namespace));
                (id_matches && extension_matches).then(|| id.clone())
            })
            .collect(),
    };
    Ok(ids)
}

fn operation_targets(
    collection: &CharacterCollection,
    request: &CharacterOperationRequest,
) -> Result<Vec<String>, CharacterCorpusError> {
    let mut targets = match &request.action {
        CharacterCorpusAction::Create { profile } => vec![profile.id.clone()],
        CharacterCorpusAction::Clone { new_id, .. } => vec![new_id.clone()],
        CharacterCorpusAction::Revise { character_id, .. } => vec![character_id.clone()],
        CharacterCorpusAction::Rename { character_id, .. } => {
            let mut targets = vec![character_id.clone()];
            for (owner_id, profile) in &collection.characters {
                if relationship_references(profile, character_id) {
                    targets.push(owner_id.clone());
                }
            }
            targets
        }
        CharacterCorpusAction::Import { profiles, .. } => {
            profiles.iter().map(|profile| profile.id.clone()).collect()
        }
        CharacterCorpusAction::Recompute => selected_existing_ids(collection, &request.scope)?,
    };
    targets.sort();
    targets.dedup();
    Ok(targets)
}

fn apply_action(
    result: &mut CharacterCollection,
    request: &CharacterOperationRequest,
    references: &mut BTreeMap<String, Vec<CharacterReferenceChange>>,
    rename_pair: &mut Option<(String, String)>,
) -> Result<(), CharacterCorpusError> {
    match &request.action {
        CharacterCorpusAction::Create { profile } => {
            if result.characters.contains_key(&profile.id) {
                return Err(conflict(
                    "action.profile.id",
                    "create cannot replace an existing character",
                ));
            }
            result
                .characters
                .insert(profile.id.clone(), profile.as_ref().clone());
        }
        CharacterCorpusAction::Clone {
            source_id,
            new_id,
            display_name,
            rationale,
        } => {
            if result.characters.contains_key(new_id) {
                return Err(conflict("action.new_id", "clone target id already exists"));
            }
            let mut clone = result.characters.get(source_id).cloned().ok_or_else(|| {
                corpus_error(
                    CharacterDiagnosticCode::InvalidReference,
                    "action.source_id",
                    "clone source is absent from the collection",
                )
            })?;
            clone.id.clone_from(new_id);
            if let Some(display_name) = display_name {
                clone
                    .canon
                    .identity
                    .display_name
                    .value
                    .clone_from(display_name);
                clone.canon.identity.display_name.state = ValueState::Overridden;
                clone.canon.identity.display_name.review = ReviewState::Accepted;
                clone.canon.identity.display_name.rationale = Some(rationale.clone());
            }
            rewrite_owned_relationship_source(&mut clone, source_id, new_id, rationale, true)?;
            rewrite_owned_expression_links(&mut clone, source_id, new_id, rationale, true)?;
            recompute_derived(&mut clone);
            validate_profile(&clone).map_err(from_profile)?;
            result.characters.insert(new_id.clone(), clone);
        }
        CharacterCorpusAction::Revise {
            character_id,
            operations,
        } => {
            let profile = result
                .characters
                .get(character_id)
                .cloned()
                .ok_or_else(|| {
                    corpus_error(
                        CharacterDiagnosticCode::InvalidReference,
                        "action.character_id",
                        "revision target is absent from the collection",
                    )
                })?;
            let template = CharacterTemplate {
                template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
                id: format!("{character_id}.operation_template"),
                version: "1.0.0".to_owned(),
                profile,
            };
            let overlay = CharacterOverlay {
                overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
                id: request.id.clone(),
                character_id: character_id.clone(),
                template: Some(CharacterTemplateRef {
                    id: template.id.clone(),
                    version: template.version.clone(),
                    sha256: template_fingerprint(&template).map_err(from_profile)?,
                }),
                operations: operations.clone(),
                provenance: request.provenance.clone(),
            };
            let synthesis =
                synthesize_character(Some(&template), &overlay).map_err(from_profile)?;
            result
                .characters
                .insert(character_id.clone(), synthesis.effective_profile);
        }
        CharacterCorpusAction::Rename {
            character_id,
            new_id,
            new_display_name,
            override_locked,
            rationale,
        } => {
            let final_id = new_id.as_deref().unwrap_or(character_id);
            if final_id != character_id && result.characters.contains_key(final_id) {
                return Err(conflict("action.new_id", "rename target id already exists"));
            }
            let mut profile = result.characters.remove(character_id).ok_or_else(|| {
                corpus_error(
                    CharacterDiagnosticCode::InvalidReference,
                    "action.character_id",
                    "rename target is absent from the collection",
                )
            })?;
            if let Some(display_name) = new_display_name {
                if profile.canon.identity.display_name.lock == LockState::Locked && !override_locked
                {
                    return Err(locked(
                        "action.new_display_name",
                        "display name is locked and requires an explicit reviewed override",
                    ));
                }
                profile
                    .canon
                    .identity
                    .display_name
                    .value
                    .clone_from(display_name);
                profile.canon.identity.display_name.state = ValueState::Overridden;
                profile.canon.identity.display_name.review = ReviewState::Accepted;
                profile.canon.identity.display_name.rationale = Some(rationale.clone());
                profile.canon.identity.display_name.freshness = Freshness::Current;
            }
            if final_id != character_id {
                profile.id = final_id.to_owned();
                let own_reference_changes = rewrite_owned_relationship_source(
                    &mut profile,
                    character_id,
                    final_id,
                    rationale,
                    *override_locked,
                )?;
                let mut own_reference_changes = own_reference_changes;
                own_reference_changes.extend(rewrite_owned_expression_links(
                    &mut profile,
                    character_id,
                    final_id,
                    rationale,
                    *override_locked,
                )?);
                own_reference_changes.sort();
                if !own_reference_changes.is_empty() {
                    references.insert(final_id.to_owned(), own_reference_changes);
                }
                for (owner_id, owner) in &mut result.characters {
                    let mut changes = rewrite_relationship_targets(
                        owner,
                        character_id,
                        final_id,
                        rationale,
                        *override_locked,
                    )?;
                    changes.extend(rewrite_expression_context_targets(
                        owner,
                        character_id,
                        final_id,
                        rationale,
                        *override_locked,
                    )?);
                    changes.sort();
                    if !changes.is_empty() {
                        references.insert(owner_id.clone(), changes);
                    }
                }
                *rename_pair = Some((character_id.clone(), final_id.to_owned()));
            }
            recompute_derived(&mut profile);
            validate_profile(&profile).map_err(from_profile)?;
            result.characters.insert(final_id.to_owned(), profile);
        }
        CharacterCorpusAction::Import {
            profiles,
            force,
            rationale,
        } => {
            for profile in profiles {
                match result.characters.get(&profile.id) {
                    Some(existing) if existing == profile => {}
                    Some(existing) => {
                        if !force {
                            return Err(conflict(
                                "action.profiles",
                                "import conflicts with an existing profile",
                            ));
                        }
                        if profile_has_locks(existing)
                            && rationale.as_deref().is_none_or(str::is_empty)
                        {
                            return Err(locked(
                                "action.profiles",
                                "forced replacement of locked data requires a rationale",
                            ));
                        }
                        result
                            .characters
                            .insert(profile.id.clone(), profile.clone());
                    }
                    None => {
                        result
                            .characters
                            .insert(profile.id.clone(), profile.clone());
                    }
                }
            }
        }
        CharacterCorpusAction::Recompute => {
            for id in selected_existing_ids(result, &request.scope)? {
                let profile = result
                    .characters
                    .get_mut(&id)
                    .expect("selected ids come from the collection");
                recompute_derived(profile);
            }
        }
    }
    Ok(())
}

fn rewrite_owned_relationship_source(
    profile: &mut CharacterProfile,
    old_id: &str,
    new_id: &str,
    rationale: &str,
    override_locked: bool,
) -> Result<Vec<CharacterReferenceChange>, CharacterCorpusError> {
    let mut changes = Vec::new();
    for (namespace, extension) in &mut profile.extensions {
        let CharacterExtension::Relationships(record) = extension else {
            continue;
        };
        let affected = record
            .value
            .edges
            .iter()
            .filter(|(_, edge)| edge.source_character_id == old_id)
            .map(|(edge_id, _)| edge_id.clone())
            .collect::<Vec<_>>();
        if affected.is_empty() {
            continue;
        }
        require_extension_override(
            &mut record.header,
            rationale,
            override_locked,
            "relationships.source_character_id",
        )?;
        for edge_id in affected {
            record
                .value
                .edges
                .get_mut(&edge_id)
                .expect("affected edge remains present")
                .source_character_id = new_id.to_owned();
            changes.push(CharacterReferenceChange {
                owner_character_id: profile.id.clone(),
                path: format!("extensions.{namespace}.value.edges.{edge_id}.source_character_id"),
            });
        }
    }
    changes.sort();
    Ok(changes)
}

fn rewrite_relationship_targets(
    profile: &mut CharacterProfile,
    old_id: &str,
    new_id: &str,
    rationale: &str,
    override_locked: bool,
) -> Result<Vec<CharacterReferenceChange>, CharacterCorpusError> {
    let mut changes = Vec::new();
    for (namespace, extension) in &mut profile.extensions {
        let CharacterExtension::Relationships(record) = extension else {
            continue;
        };
        let affected = record
            .value
            .edges
            .iter()
            .filter(|(_, edge)| edge.target_character_id == old_id)
            .map(|(edge_id, _)| edge_id.clone())
            .collect::<Vec<_>>();
        if affected.is_empty() {
            continue;
        }
        require_extension_override(
            &mut record.header,
            rationale,
            override_locked,
            "relationships.target_character_id",
        )?;
        for edge_id in affected {
            record
                .value
                .edges
                .get_mut(&edge_id)
                .expect("affected edge remains present")
                .target_character_id = new_id.to_owned();
            changes.push(CharacterReferenceChange {
                owner_character_id: profile.id.clone(),
                path: format!("extensions.{namespace}.value.edges.{edge_id}.target_character_id"),
            });
        }
    }
    changes.sort();
    Ok(changes)
}

fn rewrite_owned_expression_links(
    profile: &mut CharacterProfile,
    old_id: &str,
    new_id: &str,
    rationale: &str,
    override_locked: bool,
) -> Result<Vec<CharacterReferenceChange>, CharacterCorpusError> {
    let mut changes = Vec::new();
    for (namespace, extension) in &mut profile.extensions {
        match extension {
            CharacterExtension::Expression(record) => {
                let has_owner_links = record.value.character_id == old_id
                    || record
                        .value
                        .lexicon
                        .values()
                        .any(|value| value.character_id == old_id)
                    || record
                        .value
                        .preferences
                        .values()
                        .any(|value| value.character_id == old_id)
                    || record
                        .value
                        .vocabulary_pools
                        .values()
                        .any(|value| value.character_id == old_id)
                    || record
                        .value
                        .voice_constraints
                        .values()
                        .any(|value| value.character_id == old_id)
                    || record
                        .value
                        .template_assignments
                        .values()
                        .any(|value| value.character_id == old_id)
                    || record
                        .value
                        .behavioral_signature_refs
                        .iter()
                        .any(|value| value.starts_with(&format!("{old_id}.signature.")));
                if !has_owner_links {
                    continue;
                }
                require_extension_override(
                    &mut record.header,
                    rationale,
                    override_locked,
                    "expression.character_id",
                )?;
                if record.value.character_id == old_id {
                    record.value.character_id = new_id.to_owned();
                    changes.push(CharacterReferenceChange {
                        owner_character_id: profile.id.clone(),
                        path: format!("extensions.{namespace}.value.character_id"),
                    });
                }
                for (id, value) in &mut record.value.lexicon {
                    rewrite_expression_record_owner(
                        &mut value.character_id,
                        old_id,
                        new_id,
                        &mut changes,
                        &profile.id,
                        format!("extensions.{namespace}.value.lexicon.{id}.character_id"),
                    );
                }
                for (id, value) in &mut record.value.preferences {
                    rewrite_expression_record_owner(
                        &mut value.character_id,
                        old_id,
                        new_id,
                        &mut changes,
                        &profile.id,
                        format!("extensions.{namespace}.value.preferences.{id}.character_id"),
                    );
                }
                for (id, value) in &mut record.value.vocabulary_pools {
                    rewrite_expression_record_owner(
                        &mut value.character_id,
                        old_id,
                        new_id,
                        &mut changes,
                        &profile.id,
                        format!("extensions.{namespace}.value.vocabulary_pools.{id}.character_id"),
                    );
                }
                for (id, value) in &mut record.value.voice_constraints {
                    rewrite_expression_record_owner(
                        &mut value.character_id,
                        old_id,
                        new_id,
                        &mut changes,
                        &profile.id,
                        format!("extensions.{namespace}.value.voice_constraints.{id}.character_id"),
                    );
                }
                for (id, value) in &mut record.value.template_assignments {
                    rewrite_expression_record_owner(
                        &mut value.character_id,
                        old_id,
                        new_id,
                        &mut changes,
                        &profile.id,
                        format!(
                            "extensions.{namespace}.value.template_assignments.{id}.character_id"
                        ),
                    );
                }
                let prefix = format!("{old_id}.signature.");
                for (index, value) in record
                    .value
                    .behavioral_signature_refs
                    .iter_mut()
                    .enumerate()
                {
                    if let Some(suffix) = value.strip_prefix(&prefix) {
                        *value = format!("{new_id}.signature.{suffix}");
                        changes.push(CharacterReferenceChange {
                            owner_character_id: profile.id.clone(),
                            path: format!(
                                "extensions.{namespace}.value.behavioral_signature_refs[{index}]"
                            ),
                        });
                    }
                }
            }
            CharacterExtension::BehavioralSignatures(record) => {
                let has_owner_links = record.value.character_id == old_id
                    || record
                        .value
                        .signatures
                        .values()
                        .any(|value| value.character_id == old_id);
                if !has_owner_links {
                    continue;
                }
                require_extension_override(
                    &mut record.header,
                    rationale,
                    override_locked,
                    "behavioral_signatures.character_id",
                )?;
                if record.value.character_id == old_id {
                    record.value.character_id = new_id.to_owned();
                    changes.push(CharacterReferenceChange {
                        owner_character_id: profile.id.clone(),
                        path: format!("extensions.{namespace}.value.character_id"),
                    });
                }
                for (id, value) in &mut record.value.signatures {
                    rewrite_expression_record_owner(
                        &mut value.character_id,
                        old_id,
                        new_id,
                        &mut changes,
                        &profile.id,
                        format!("extensions.{namespace}.value.signatures.{id}.character_id"),
                    );
                }
            }
            CharacterExtension::RoleProjections(record) => {
                let has_owner_links = record.value.character_id == old_id
                    || record
                        .value
                        .roles
                        .values()
                        .any(|value| value.character_id == old_id);
                if !has_owner_links {
                    continue;
                }
                require_extension_override(
                    &mut record.header,
                    rationale,
                    override_locked,
                    "role_projections.character_id",
                )?;
                if record.value.character_id == old_id {
                    record.value.character_id = new_id.to_owned();
                    changes.push(CharacterReferenceChange {
                        owner_character_id: profile.id.clone(),
                        path: format!("extensions.{namespace}.value.character_id"),
                    });
                }
                for (id, value) in &mut record.value.roles {
                    rewrite_expression_record_owner(
                        &mut value.character_id,
                        old_id,
                        new_id,
                        &mut changes,
                        &profile.id,
                        format!("extensions.{namespace}.value.roles.{id}.character_id"),
                    );
                }
            }
            _ => {}
        }
    }
    changes.sort();
    changes.dedup();
    Ok(changes)
}

fn rewrite_expression_record_owner(
    value: &mut String,
    old_id: &str,
    new_id: &str,
    changes: &mut Vec<CharacterReferenceChange>,
    owner_id: &str,
    path: String,
) {
    if value == old_id {
        *value = new_id.to_owned();
        changes.push(CharacterReferenceChange {
            owner_character_id: owner_id.to_owned(),
            path,
        });
    }
}

fn rewrite_expression_context_targets(
    profile: &mut CharacterProfile,
    old_id: &str,
    new_id: &str,
    rationale: &str,
    override_locked: bool,
) -> Result<Vec<CharacterReferenceChange>, CharacterCorpusError> {
    let mut changes = Vec::new();
    for (namespace, extension) in &mut profile.extensions {
        let mut changed_paths = Vec::new();
        match extension {
            CharacterExtension::Expression(record) => {
                for (id, term) in &mut record.value.lexicon {
                    rewrite_applicability_targets(
                        &mut term.applicability,
                        old_id,
                        new_id,
                        format!("extensions.{namespace}.value.lexicon.{id}.applicability"),
                        &mut changed_paths,
                    );
                }
                for (id, preference) in &mut record.value.preferences {
                    rewrite_applicability_targets(
                        &mut preference.applicability,
                        old_id,
                        new_id,
                        format!("extensions.{namespace}.value.preferences.{id}.applicability"),
                        &mut changed_paths,
                    );
                }
                for (id, pool) in &mut record.value.vocabulary_pools {
                    rewrite_applicability_targets(
                        &mut pool.applicability,
                        old_id,
                        new_id,
                        format!("extensions.{namespace}.value.vocabulary_pools.{id}.applicability"),
                        &mut changed_paths,
                    );
                }
                for (id, constraint) in &mut record.value.voice_constraints {
                    rewrite_applicability_targets(
                        &mut constraint.applicability,
                        old_id,
                        new_id,
                        format!(
                            "extensions.{namespace}.value.voice_constraints.{id}.applicability"
                        ),
                        &mut changed_paths,
                    );
                }
                if !changed_paths.is_empty() {
                    require_extension_override(
                        &mut record.header,
                        rationale,
                        override_locked,
                        "expression.applicability",
                    )?;
                }
            }
            CharacterExtension::BehavioralSignatures(record) => {
                for (id, signature) in &mut record.value.signatures {
                    rewrite_applicability_targets(
                        &mut signature.applicability,
                        old_id,
                        new_id,
                        format!("extensions.{namespace}.value.signatures.{id}.applicability"),
                        &mut changed_paths,
                    );
                }
                if !changed_paths.is_empty() {
                    require_extension_override(
                        &mut record.header,
                        rationale,
                        override_locked,
                        "behavioral_signatures.applicability",
                    )?;
                }
            }
            _ => {}
        }
        changes.extend(
            changed_paths
                .into_iter()
                .map(|path| CharacterReferenceChange {
                    owner_character_id: profile.id.clone(),
                    path,
                }),
        );
    }
    changes.sort();
    changes.dedup();
    Ok(changes)
}

fn rewrite_applicability_targets(
    applicability: &mut crate::ExpressionApplicability,
    old_id: &str,
    new_id: &str,
    path: String,
    changes: &mut Vec<String>,
) {
    for (index, predicate) in applicability.predicates.iter_mut().enumerate() {
        let crate::ExpressionContextPredicate::Relationship {
            other_character_id: Some(character_id),
            ..
        } = predicate
        else {
            continue;
        };
        if character_id == old_id {
            *character_id = new_id.to_owned();
            changes.push(format!("{path}.predicates[{index}].other_character_id"));
        }
    }
}

fn require_extension_override(
    header: &mut crate::ExtensionHeader,
    rationale: &str,
    override_locked: bool,
    path: &str,
) -> Result<(), CharacterCorpusError> {
    if header.lock == LockState::Locked && !override_locked {
        return Err(locked(
            path,
            "extension reference is locked and requires an explicit reviewed override",
        ));
    }
    header.state = ValueState::Overridden;
    header.review = ReviewState::Accepted;
    header.rationale = rationale.to_owned();
    header.freshness = Freshness::Current;
    Ok(())
}

fn relationship_references(profile: &CharacterProfile, id: &str) -> bool {
    profile
        .extensions
        .values()
        .any(|extension| match extension {
            CharacterExtension::Relationships(record) => record
                .value
                .edges
                .values()
                .any(|edge| edge.source_character_id == id || edge.target_character_id == id),
            CharacterExtension::Expression(record) => record
                .value
                .lexicon
                .values()
                .map(|value| &value.applicability)
                .chain(
                    record
                        .value
                        .preferences
                        .values()
                        .map(|value| &value.applicability),
                )
                .chain(
                    record
                        .value
                        .vocabulary_pools
                        .values()
                        .map(|value| &value.applicability),
                )
                .chain(
                    record
                        .value
                        .voice_constraints
                        .values()
                        .map(|value| &value.applicability),
                )
                .any(|applicability| applicability_references(applicability, id)),
            CharacterExtension::BehavioralSignatures(record) => record
                .value
                .signatures
                .values()
                .any(|value| applicability_references(&value.applicability, id)),
            _ => false,
        })
}

fn applicability_references(value: &crate::ExpressionApplicability, id: &str) -> bool {
    value.predicates.iter().any(|predicate| {
        matches!(
            predicate,
            crate::ExpressionContextPredicate::Relationship {
                other_character_id: Some(character_id),
                ..
            } if character_id == id
        )
    })
}

fn profile_has_locks(profile: &CharacterProfile) -> bool {
    let identity_locked = profile.canon.identity.display_name.lock == LockState::Locked
        || profile
            .canon
            .identity
            .aliases
            .as_ref()
            .is_some_and(|value| value.lock == LockState::Locked)
        || profile
            .canon
            .birth_date
            .as_ref()
            .is_some_and(|value| value.lock == LockState::Locked);
    identity_locked
        || profile.extensions.values().any(|extension| {
            extension_header(extension).lock == LockState::Locked
                || matches!(
                    extension,
                    CharacterExtension::RoleProjections(record)
                        if record
                            .value
                            .roles
                            .values()
                            .any(|value| value.lock == LockState::Locked)
                )
        })
        || ALL_HEXACO_TRAITS.iter().any(|trait_id| {
            trait_value(&profile.canon.personality, *trait_id)
                .is_some_and(|value| value.lock == LockState::Locked)
        })
}

fn extension_header(extension: &CharacterExtension) -> &crate::ExtensionHeader {
    match extension {
        CharacterExtension::IdentityPresentation(record) => &record.header,
        CharacterExtension::Expression(record) => &record.header,
        CharacterExtension::BehavioralSignatures(record) => &record.header,
        CharacterExtension::RoleProjections(record) => &record.header,
        CharacterExtension::Relationships(record) => &record.header,
        CharacterExtension::AlignmentView(record) => &record.header,
        CharacterExtension::DateContext(record) => &record.header,
        CharacterExtension::Tabletop(record) | CharacterExtension::Opaque(record) => &record.header,
    }
}

fn build_changes(
    input: &CharacterCollection,
    output: &CharacterCollection,
    request: &CharacterOperationRequest,
    request_sha256: &str,
    rename_pair: Option<&(String, String)>,
    references: &BTreeMap<String, Vec<CharacterReferenceChange>>,
) -> Result<Vec<CharacterChange>, CharacterCorpusError> {
    let mut pairs = Vec::<(Option<String>, Option<String>)>::new();
    let mut skipped = BTreeSet::new();
    if let Some((before, after)) = rename_pair {
        pairs.push((Some(before.clone()), Some(after.clone())));
        skipped.insert(before.clone());
        skipped.insert(after.clone());
    }
    let ids = input
        .characters
        .keys()
        .chain(output.characters.keys())
        .filter(|id| !skipped.contains(*id))
        .cloned()
        .collect::<BTreeSet<_>>();
    pairs.extend(ids.into_iter().map(|id| {
        let before_id = input.characters.contains_key(&id).then(|| id.clone());
        let after_id = output.characters.contains_key(&id).then_some(id);
        (before_id, after_id)
    }));
    let mut changes = Vec::new();
    for (before_id, after_id) in pairs {
        let before = before_id.as_ref().and_then(|id| input.characters.get(id));
        let after = after_id.as_ref().and_then(|id| output.characters.get(id));
        if before == after {
            continue;
        }
        if before.is_none() && after.is_none() {
            continue;
        }
        let affected_references = after_id
            .as_ref()
            .and_then(|id| references.get(id))
            .cloned()
            .unwrap_or_default();
        let kind = change_kind(
            request,
            before_id.as_deref(),
            after_id.as_deref(),
            !affected_references.is_empty(),
        );
        let before_sha256 = before.map(profile_fingerprint).transpose()?;
        let after_sha256 = after.map(profile_fingerprint).transpose()?;
        let material = format!(
            "{request_sha256}\0{}\0{}\0{}",
            before_id.as_deref().unwrap_or(""),
            after_id.as_deref().unwrap_or(""),
            changes.len()
        );
        let digest = format!("{:x}", Sha256::digest(material.as_bytes()));
        changes.push(CharacterChange {
            id: format!("change_{}", &digest[..24]),
            kind,
            before_id,
            after_id,
            before_sha256,
            after_sha256,
            affected_references,
        });
    }
    changes.sort_by(|left, right| {
        left.after_id
            .as_deref()
            .or(left.before_id.as_deref())
            .cmp(&right.after_id.as_deref().or(right.before_id.as_deref()))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(changes)
}

fn change_kind(
    request: &CharacterOperationRequest,
    before_id: Option<&str>,
    after_id: Option<&str>,
    reference_updated: bool,
) -> CharacterChangeKind {
    if before_id != after_id {
        return CharacterChangeKind::Renamed;
    }
    if reference_updated {
        return CharacterChangeKind::ReferenceUpdated;
    }
    match &request.action {
        CharacterCorpusAction::Create { .. } => CharacterChangeKind::Created,
        CharacterCorpusAction::Clone { .. } => CharacterChangeKind::Cloned,
        CharacterCorpusAction::Revise { .. } => CharacterChangeKind::Revised,
        CharacterCorpusAction::Rename { .. } => CharacterChangeKind::Renamed,
        CharacterCorpusAction::Import { .. } => CharacterChangeKind::Imported,
        CharacterCorpusAction::Recompute => CharacterChangeKind::Recomputed,
    }
}

fn validate_changes(changes: &[CharacterChange]) -> Result<(), CharacterCorpusError> {
    let mut ids = BTreeSet::new();
    let mut previous_key: Option<String> = None;
    for (index, change) in changes.iter().enumerate() {
        validate_local_id(&format!("changes[{index}].id"), &change.id)?;
        if !ids.insert(change.id.as_str()) {
            return Err(invalid_value("changes", "change ids must be unique"));
        }
        if change.before_id.is_none() && change.after_id.is_none() {
            return Err(invalid_value(
                format!("changes[{index}]"),
                "change requires a before or after character id",
            ));
        }
        for (path, id) in [
            ("before_id", change.before_id.as_deref()),
            ("after_id", change.after_id.as_deref()),
        ] {
            if let Some(id) = id {
                validate_namespaced_id(&format!("changes[{index}].{path}"), id)?;
            }
        }
        for (path, hash) in [
            ("before_sha256", change.before_sha256.as_deref()),
            ("after_sha256", change.after_sha256.as_deref()),
        ] {
            if let Some(hash) = hash {
                validate_sha256(&format!("changes[{index}].{path}"), hash)?;
            }
        }
        if change.before_id.is_some() != change.before_sha256.is_some()
            || change.after_id.is_some() != change.after_sha256.is_some()
        {
            return Err(invalid_value(
                format!("changes[{index}]"),
                "each before or after id requires its corresponding profile fingerprint",
            ));
        }
        let key = change
            .after_id
            .as_deref()
            .or(change.before_id.as_deref())
            .expect("validated above")
            .to_owned();
        if previous_key
            .as_ref()
            .is_some_and(|previous| previous > &key)
        {
            return Err(invalid_value(
                "changes",
                "changes must be sorted by effective character id",
            ));
        }
        previous_key = Some(key);
        let mut previous_reference = None;
        for reference in &change.affected_references {
            validate_namespaced_id(
                "changes.affected_references.owner_character_id",
                &reference.owner_character_id,
            )?;
            validate_text("changes.affected_references.path", &reference.path, 1, 512)?;
            if previous_reference.is_some_and(|prior: &CharacterReferenceChange| prior >= reference)
            {
                return Err(invalid_value(
                    "changes.affected_references",
                    "affected references must be unique and sorted",
                ));
            }
            previous_reference = Some(reference);
        }
    }
    Ok(())
}

fn profile_fingerprint(profile: &CharacterProfile) -> Result<String, CharacterCorpusError> {
    canonical_hash(profile)
}

fn canonical_hash(value: &impl Serialize) -> Result<String, CharacterCorpusError> {
    let bytes = serde_json::to_vec(value).map_err(|_| encoding_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_sorted_ids(path: &str, ids: &[String]) -> Result<(), CharacterCorpusError> {
    let mut previous = None;
    for id in ids {
        validate_namespaced_id(path, id)?;
        if previous.is_some_and(|prior: &str| prior >= id.as_str()) {
            return Err(invalid_value(path, "ids must be unique and sorted"));
        }
        previous = Some(id);
    }
    Ok(())
}

fn validate_namespaced_id(path: &str, id: &str) -> Result<(), CharacterCorpusError> {
    let segments = id.split('.').collect::<Vec<_>>();
    if segments.len() < 2 || segments.iter().any(|segment| !valid_local_id(segment)) {
        return Err(corpus_error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a dot-separated lowercase stable identifier",
        ));
    }
    Ok(())
}

fn validate_local_id(path: &str, id: &str) -> Result<(), CharacterCorpusError> {
    if valid_local_id(id) {
        Ok(())
    } else {
        Err(corpus_error(
            CharacterDiagnosticCode::InvalidIdentifier,
            path,
            "expected a lowercase stable local identifier",
        ))
    }
}

fn valid_local_id(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
        && !value.contains("__")
}

fn validate_sha256(path: &str, value: &str) -> Result<(), CharacterCorpusError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(corpus_error(
            CharacterDiagnosticCode::InvalidLineage,
            path,
            "expected a lowercase SHA-256 fingerprint",
        ))
    }
}

fn validate_text(
    path: impl Into<String>,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), CharacterCorpusError> {
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length)
        || value.chars().any(char::is_control)
        || resembles_secret(value)
    {
        return Err(invalid_value(
            path,
            "text is outside bounds or resembles a credential",
        ));
    }
    Ok(())
}

fn resembles_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "-----begin private key-----",
        "authorization: bearer ",
        "api_key=",
        "apikey=",
        "client_secret=",
        "github_pat_",
        "ghp_",
        "sk-",
        "xoxb-",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
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

fn from_profile(error: crate::CharacterError) -> CharacterCorpusError {
    CharacterCorpusError {
        diagnostic: error.diagnostic().clone(),
    }
}

fn corpus_error(
    code: CharacterDiagnosticCode,
    path: impl Into<String>,
    message: impl Into<String>,
) -> CharacterCorpusError {
    CharacterCorpusError {
        diagnostic: CharacterDiagnostic {
            code,
            severity: DiagnosticSeverity::Error,
            path: path.into(),
            message: message.into(),
        },
    }
}

fn invalid_value(path: impl Into<String>, message: impl Into<String>) -> CharacterCorpusError {
    corpus_error(CharacterDiagnosticCode::InvalidValue, path, message)
}

fn conflict(path: impl Into<String>, message: impl Into<String>) -> CharacterCorpusError {
    corpus_error(CharacterDiagnosticCode::ConflictingOverlay, path, message)
}

fn locked(path: impl Into<String>, message: impl Into<String>) -> CharacterCorpusError {
    corpus_error(CharacterDiagnosticCode::LockedField, path, message)
}

fn stale(path: impl Into<String>, message: impl Into<String>) -> CharacterCorpusError {
    corpus_error(CharacterDiagnosticCode::StaleInput, path, message)
}

fn encoding_error() -> CharacterCorpusError {
    corpus_error(
        CharacterDiagnosticCode::InvalidEncoding,
        "document",
        "Character corpus document does not match the strict serialized contract",
    )
}
