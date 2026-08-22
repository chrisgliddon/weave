//! Deterministic, reviewable allocation of non-canonical Character presentation data.
//!
//! Catalog eligibility is deliberately limited to explicitly declared stable character ids and
//! id prefixes. The allocator never reads personality, birth data, alignment, ruleset state, or
//! any protected identity characteristic. Proposals are immutable dry-runs; only a complete
//! editorial review can produce an atomic collection receipt.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use weave_domain::{
    Provenance, ProvenanceKind, parse_strict_json, to_pretty_json, to_pretty_ron,
    validate_provenance,
};

use crate::synthesis::{current_value_hash, merge_provenance};
use crate::validation::{
    error, validate_local_id, validate_namespaced_id, validate_presentation_catalog_ref,
    validate_presentation_catalog_value, validate_relative_path, validate_semver, validate_sha256,
    validate_text,
};
use crate::{
    Attributed, CHARACTER_COLLECTION_FORMAT_VERSION, CHARACTER_OVERLAY_FORMAT_VERSION,
    CHARACTER_TEMPLATE_FORMAT_VERSION, CharacterCollection, CharacterDiagnosticCode,
    CharacterExtension, CharacterOperation, CharacterOperationAction, CharacterOverlay,
    CharacterProfile, CharacterTemplate, CharacterTemplateRef, Confidence, Freshness, LockState,
    PresentationCatalogAssignment, PresentationCatalogRef, PresentationCatalogValue, ReviewState,
    ValueState, collection_fingerprint, synthesize_character, template_fingerprint,
    validate_character_collection,
};

/// Current immutable presentation-catalog format.
pub const PRESENTATION_CATALOG_FORMAT_VERSION: u32 = 1;
/// Current allocation-request format.
pub const PRESENTATION_ALLOCATION_REQUEST_FORMAT_VERSION: u32 = 1;
/// Current deterministic proposal format.
pub const PRESENTATION_PROPOSAL_FORMAT_VERSION: u32 = 1;
/// Current editorial-review format.
pub const PRESENTATION_REVIEW_FORMAT_VERSION: u32 = 1;
/// Current independently reproducible receipt format.
pub const PRESENTATION_RECEIPT_FORMAT_VERSION: u32 = 1;
/// Current lock-revision format.
pub const PRESENTATION_LOCK_REVISION_FORMAT_VERSION: u32 = 1;

const CATALOG_SCHEMA_ID: &str = "urn:weave:schema:character-presentation-catalog:1";
const REQUEST_SCHEMA_ID: &str = "urn:weave:schema:character-presentation-request:1";
const PROPOSAL_SCHEMA_ID: &str = "urn:weave:schema:character-presentation-proposal:1";
const REVIEW_SCHEMA_ID: &str = "urn:weave:schema:character-presentation-review:1";
const RECEIPT_SCHEMA_ID: &str = "urn:weave:schema:character-presentation-receipt:1";
const LOCK_SCHEMA_ID: &str = "urn:weave:schema:character-presentation-lock-revision:1";

/// Immutable, data-only catalog of explicitly eligible presentation values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationCatalog {
    pub catalog_format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    /// Must be true for a distributable catalog.
    pub independently_authored: bool,
    pub license: String,
    pub license_url: String,
    pub slots: BTreeMap<String, PresentationCatalogSlot>,
    pub entries: BTreeMap<String, PresentationCatalogEntry>,
    pub provenance: Provenance,
}

/// One independently allocatable presentation slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationCatalogSlot {
    pub id: String,
    pub label: String,
    pub description: String,
    pub value_kind: PresentationCatalogValueKind,
}

/// Closed portable value kinds used to reject incompatible catalog entries and overrides.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PresentationCatalogValueKind {
    Appearance,
    PaletteColor,
    StyleTag,
    Asset,
}

/// One presentation value together with transparent eligibility and capacity constraints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationCatalogEntry {
    pub id: String,
    pub slot_id: String,
    pub label: String,
    pub value: PresentationCatalogValue,
    /// Exact ids eligible for this entry. Eligibility is the union with `eligible_id_prefixes`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eligible_character_ids: Vec<String>,
    /// Stable id prefixes eligible for this entry. No profile field is inspected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eligible_id_prefixes: Vec<String>,
    /// Explicit exclusions take precedence over either eligible list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_character_ids: Vec<String>,
    /// Maximum proposed allocations, after retained assignments consume capacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
}

/// Exact dry-run inputs. Every host supplies the same finite asset inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationAllocationRequest {
    pub request_format_version: u32,
    pub id: String,
    pub expected_input_sha256: String,
    pub catalog: PresentationCatalogRef,
    pub seed: u64,
    pub character_ids: Vec<String>,
    pub slot_ids: Vec<String>,
    pub mode: PresentationAllocationMode,
    /// Sorted project-relative paths known to exist at the authoring boundary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_asset_paths: Vec<String>,
}

/// Whether existing unlocked reviewed allocations are retained or reconsidered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PresentationAllocationMode {
    FillMissing,
    Rebalance,
}

/// Immutable proposal embedding every input required for independent replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationProposal {
    pub proposal_format_version: u32,
    pub id: String,
    pub input_collection: CharacterCollection,
    pub input_sha256: String,
    pub catalog: PresentationCatalog,
    pub catalog_ref: PresentationCatalogRef,
    pub request: PresentationAllocationRequest,
    pub request_sha256: String,
    /// Character id, then slot id, both canonically sorted by their maps.
    pub allocations: BTreeMap<String, BTreeMap<String, PresentationProposedAllocation>>,
    pub distribution: PresentationDistribution,
}

/// One retained, proposed, or unavailable character/slot allocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationProposedAllocation {
    pub character_id: String,
    pub slot_id: String,
    pub disposition: PresentationAllocationDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior: Option<Attributed<PresentationCatalogAssignment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_entry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_value: Option<PresentationCatalogValue>,
    pub rationale: String,
    pub trace: Vec<PresentationCandidateTrace>,
}

/// Proposal state. Only `proposed` allocations require a review decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PresentationAllocationDisposition {
    Retained,
    Proposed,
    Unavailable,
}

/// Transparent eligibility, balance, capacity, and seed trace for one candidate entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationCandidateTrace {
    pub entry_id: String,
    pub explicitly_eligible: bool,
    pub explicitly_excluded: bool,
    pub used_before: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
    pub capacity_available: bool,
    pub seeded_sha256: String,
    pub selected: bool,
}

/// Exact counts grouped by slot and entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationDistribution {
    pub counts: BTreeMap<String, BTreeMap<String, u32>>,
    pub unavailable: BTreeMap<String, Vec<String>>,
}

/// Complete decision map for every proposed character/slot pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationReview {
    pub review_format_version: u32,
    pub proposal_sha256: String,
    pub reviewer: String,
    pub rationale: String,
    pub decisions: BTreeMap<String, BTreeMap<String, PresentationReviewDecision>>,
}

/// Explicit editorial disposition of one proposed value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "decision", deny_unknown_fields)]
pub enum PresentationReviewDecision {
    Accept {
        lock: LockState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    /// Select another eligible entry from the same immutable catalog.
    Edit {
        entry_id: String,
        lock: LockState,
        rationale: String,
    },
    /// Author a presentation-only value outside the catalog allocation.
    Override {
        value: PresentationCatalogValue,
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

/// Full proof of proposal replay, review, sparse operations, and atomic collection output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationReceipt {
    pub receipt_format_version: u32,
    pub id: String,
    pub proposal: PresentationProposal,
    pub review: PresentationReview,
    pub proposal_sha256: String,
    pub review_sha256: String,
    pub operations: BTreeMap<String, Vec<CharacterOperation>>,
    pub output_collection: CharacterCollection,
    pub output_sha256: String,
    pub distribution: PresentationDistribution,
}

/// Portable deterministic lock/unlock request for existing presentation assignments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationLockRevision {
    pub revision_format_version: u32,
    pub id: String,
    pub expected_input_sha256: String,
    pub targets: Vec<PresentationLockTarget>,
    pub lock: LockState,
    pub rationale: String,
    pub provenance: Provenance,
}

/// One exact assignment coordinate in a lock revision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationLockTarget {
    pub character_id: String,
    pub slot_id: String,
}

macro_rules! impl_presentation_document {
    ($type:ty, $validate:expr) => {
        impl $type {
            pub fn from_json(source: &str) -> Result<Self, crate::CharacterError> {
                let value = parse_strict_json(source).map_err(|_| encoding_error())?;
                ($validate)(&value)?;
                Ok(value)
            }

            pub fn from_ron(source: &str) -> Result<Self, crate::CharacterError> {
                let value = ron::from_str(source).map_err(|_| encoding_error())?;
                ($validate)(&value)?;
                Ok(value)
            }

            pub fn to_json(&self) -> Result<String, crate::CharacterError> {
                ($validate)(self)?;
                to_pretty_json(self).map_err(|_| encoding_error())
            }

            pub fn to_ron(&self) -> Result<String, crate::CharacterError> {
                ($validate)(self)?;
                to_pretty_ron(self).map_err(|_| encoding_error())
            }
        }
    };
}

impl_presentation_document!(PresentationCatalog, validate_presentation_catalog);
impl_presentation_document!(
    PresentationAllocationRequest,
    validate_presentation_allocation_request
);
impl_presentation_document!(PresentationProposal, validate_presentation_proposal);
impl_presentation_document!(PresentationReview, validate_presentation_review_structure);
impl_presentation_document!(PresentationReceipt, validate_presentation_receipt);
impl_presentation_document!(
    PresentationLockRevision,
    validate_presentation_lock_revision
);

/// Generate the canonical presentation-catalog JSON Schema.
pub fn presentation_catalog_schema() -> Result<String, crate::CharacterError> {
    presentation_schema::<PresentationCatalog>(
        CATALOG_SCHEMA_ID,
        "Weave Character Presentation Catalog v1",
    )
}

/// Generate the canonical presentation allocation-request JSON Schema.
pub fn presentation_allocation_request_schema() -> Result<String, crate::CharacterError> {
    presentation_schema::<PresentationAllocationRequest>(
        REQUEST_SCHEMA_ID,
        "Weave Character Presentation Allocation Request v1",
    )
}

/// Generate the canonical presentation proposal JSON Schema.
pub fn presentation_proposal_schema() -> Result<String, crate::CharacterError> {
    presentation_schema::<PresentationProposal>(
        PROPOSAL_SCHEMA_ID,
        "Weave Character Presentation Proposal v1",
    )
}

/// Generate the canonical presentation review JSON Schema.
pub fn presentation_review_schema() -> Result<String, crate::CharacterError> {
    presentation_schema::<PresentationReview>(
        REVIEW_SCHEMA_ID,
        "Weave Character Presentation Review v1",
    )
}

/// Generate the canonical presentation receipt JSON Schema.
pub fn presentation_receipt_schema() -> Result<String, crate::CharacterError> {
    presentation_schema::<PresentationReceipt>(
        RECEIPT_SCHEMA_ID,
        "Weave Character Presentation Receipt v1",
    )
}

/// Generate the canonical presentation lock-revision JSON Schema.
pub fn presentation_lock_revision_schema() -> Result<String, crate::CharacterError> {
    presentation_schema::<PresentationLockRevision>(
        LOCK_SCHEMA_ID,
        "Weave Character Presentation Lock Revision v1",
    )
}

fn presentation_schema<T: JsonSchema>(
    id: &str,
    title: &str,
) -> Result<String, crate::CharacterError> {
    let generated = schemars::schema_for!(T);
    let mut value = serde_json::to_value(generated).map_err(|_| encoding_error())?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-character-presentation-version".to_owned(),
            serde_json::Value::from(1),
        );
    }
    crate::sort_json_keys(&mut value);
    to_pretty_json(&value).map_err(|_| encoding_error())
}

/// Canonical SHA-256 of one independently validated immutable catalog.
pub fn presentation_catalog_fingerprint(
    catalog: &PresentationCatalog,
) -> Result<String, crate::CharacterError> {
    validate_presentation_catalog(catalog)?;
    canonical_hash(catalog)
}

/// Return the exact immutable catalog coordinate.
pub fn presentation_catalog_ref(
    catalog: &PresentationCatalog,
) -> Result<PresentationCatalogRef, crate::CharacterError> {
    Ok(PresentationCatalogRef {
        id: catalog.id.clone(),
        version: catalog.version.clone(),
        sha256: presentation_catalog_fingerprint(catalog)?,
    })
}

/// Validate one public, data-only presentation catalog.
pub fn validate_presentation_catalog(
    catalog: &PresentationCatalog,
) -> Result<(), crate::CharacterError> {
    if catalog.catalog_format_version != PRESENTATION_CATALOG_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "catalog_format_version",
            "unsupported presentation catalog version",
        ));
    }
    validate_namespaced_id("id", &catalog.id)?;
    validate_semver("version", &catalog.version)?;
    validate_text("title", &catalog.title, 1, 256)?;
    validate_text("description", &catalog.description, 1, 2_048)?;
    if !catalog.independently_authored {
        return Err(invalid(
            "independently_authored",
            "presentation catalogs must attest independent authorship",
        ));
    }
    if !matches!(catalog.license.as_str(), "MIT" | "Apache-2.0" | "CC0-1.0") {
        return Err(error(
            CharacterDiagnosticCode::InvalidLineage,
            "license",
            "presentation catalogs require an allowed public-source license",
        ));
    }
    validate_text("license_url", &catalog.license_url, 8, 2_048)?;
    validate_provenance(&catalog.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "presentation catalog provenance is invalid",
        )
    })?;
    for source in &catalog.provenance.sources {
        if !matches!(source.license.as_str(), "MIT" | "Apache-2.0" | "CC0-1.0") {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance.sources.license",
                "presentation catalog sources require an allowed public-source license",
            ));
        }
        if source.kind == ProvenanceKind::Original && source.license != "MIT" {
            return Err(error(
                CharacterDiagnosticCode::InvalidLineage,
                "provenance.sources.license",
                "original project catalog material uses the project MIT license",
            ));
        }
    }
    if catalog.slots.is_empty() || catalog.slots.len() > 256 {
        return Err(invalid("slots", "a catalog requires one through 256 slots"));
    }
    for (id, slot) in &catalog.slots {
        validate_local_id(&format!("slots.{id}"), id)?;
        if slot.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("slots.{id}.id"),
                "slot id must equal its containing map key",
            ));
        }
        validate_text(&format!("slots.{id}.label"), &slot.label, 1, 256)?;
        validate_text(
            &format!("slots.{id}.description"),
            &slot.description,
            1,
            1_024,
        )?;
    }
    if catalog.entries.is_empty() || catalog.entries.len() > 16_384 {
        return Err(invalid(
            "entries",
            "a catalog requires one through 16384 entries",
        ));
    }
    let mut slot_counts = BTreeMap::<String, usize>::new();
    for (id, entry) in &catalog.entries {
        let path = format!("entries.{id}");
        validate_local_id(&path, id)?;
        if entry.id != *id {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.id"),
                "entry id must equal its containing map key",
            ));
        }
        validate_local_id(&format!("{path}.slot_id"), &entry.slot_id)?;
        let slot = catalog.slots.get(&entry.slot_id).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                format!("{path}.slot_id"),
                "entry references an absent catalog slot",
            )
        })?;
        validate_text(&format!("{path}.label"), &entry.label, 1, 256)?;
        validate_presentation_catalog_value(&format!("{path}.value"), &entry.value)?;
        if value_kind(&entry.value) != slot.value_kind {
            return Err(invalid(
                format!("{path}.value"),
                "entry value kind does not match its catalog slot",
            ));
        }
        validate_sorted_namespaced_ids(
            &format!("{path}.eligible_character_ids"),
            &entry.eligible_character_ids,
        )?;
        validate_sorted_prefixes(
            &format!("{path}.eligible_id_prefixes"),
            &entry.eligible_id_prefixes,
        )?;
        validate_sorted_namespaced_ids(
            &format!("{path}.excluded_character_ids"),
            &entry.excluded_character_ids,
        )?;
        if entry.eligible_character_ids.is_empty() && entry.eligible_id_prefixes.is_empty() {
            return Err(invalid(
                format!("{path}.eligible_character_ids"),
                "each entry requires explicit id or id-prefix eligibility",
            ));
        }
        if entry.capacity == Some(0) {
            return Err(invalid(
                format!("{path}.capacity"),
                "entry capacity must be positive when present",
            ));
        }
        *slot_counts.entry(entry.slot_id.clone()).or_default() += 1;
    }
    for id in catalog.slots.keys() {
        if !slot_counts.contains_key(id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("slots.{id}"),
                "each catalog slot requires at least one entry",
            ));
        }
    }
    Ok(())
}

/// Validate one standalone allocation request.
pub fn validate_presentation_allocation_request(
    request: &PresentationAllocationRequest,
) -> Result<(), crate::CharacterError> {
    if request.request_format_version != PRESENTATION_ALLOCATION_REQUEST_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "request_format_version",
            "unsupported presentation allocation request version",
        ));
    }
    validate_namespaced_id("id", &request.id)?;
    validate_sha256("expected_input_sha256", &request.expected_input_sha256)?;
    validate_presentation_catalog_ref("catalog", &request.catalog)?;
    if request.character_ids.is_empty() {
        return Err(invalid(
            "character_ids",
            "presentation request requires at least one character",
        ));
    }
    validate_sorted_namespaced_ids("character_ids", &request.character_ids)?;
    if request.slot_ids.is_empty() {
        return Err(invalid(
            "slot_ids",
            "presentation request requires at least one slot",
        ));
    }
    validate_sorted_local_ids("slot_ids", &request.slot_ids)?;
    validate_sorted_relative_paths("available_asset_paths", &request.available_asset_paths)
}

/// Produce a deterministic dry-run without changing the input collection.
pub fn propose_presentation_allocations(
    collection: &CharacterCollection,
    catalog: &PresentationCatalog,
    request: &PresentationAllocationRequest,
) -> Result<PresentationProposal, crate::CharacterError> {
    validate_collection(collection)?;
    validate_presentation_catalog(catalog)?;
    validate_presentation_allocation_request(request)?;
    let input_sha256 = collection_hash(collection)?;
    if request.expected_input_sha256 != input_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "expected_input_sha256",
            "presentation request does not fingerprint the current collection",
        ));
    }
    let catalog_ref = presentation_catalog_ref(catalog)?;
    if request.catalog != catalog_ref {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "catalog",
            "presentation request does not select this exact immutable catalog",
        ));
    }
    for character_id in &request.character_ids {
        if !collection.characters.contains_key(character_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("character_ids.{character_id}"),
                "presentation request references an absent character",
            ));
        }
    }
    for slot_id in &request.slot_ids {
        if !catalog.slots.contains_key(slot_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("slot_ids.{slot_id}"),
                "presentation request references an absent catalog slot",
            ));
        }
    }
    validate_requested_assets(catalog, request)?;

    let request_sha256 = canonical_hash(request)?;
    let mut usage = retained_usage(collection, catalog, request);
    let mut allocations =
        BTreeMap::<String, BTreeMap<String, PresentationProposedAllocation>>::new();
    let mut targets = Vec::<(String, String, String)>::new();

    for character_id in &request.character_ids {
        let profile = &collection.characters[character_id];
        for slot_id in &request.slot_ids {
            let prior = existing_assignment(profile, slot_id).cloned();
            if prior
                .as_ref()
                .is_some_and(|value| retain_existing(value, request.mode))
            {
                allocations.entry(character_id.clone()).or_default().insert(
                    slot_id.clone(),
                    PresentationProposedAllocation {
                        character_id: character_id.clone(),
                        slot_id: slot_id.clone(),
                        disposition: PresentationAllocationDisposition::Retained,
                        prior,
                        proposed_entry_id: None,
                        proposed_value: None,
                        rationale: "Retained an existing assignment according to allocation mode, authority, or lock.".to_owned(),
                        trace: Vec::new(),
                    },
                );
                continue;
            }
            targets.push((
                allocation_seed(request.seed, character_id, slot_id, "target"),
                character_id.clone(),
                slot_id.clone(),
            ));
        }
    }
    targets.sort();

    for (_, character_id, slot_id) in targets {
        let profile = &collection.characters[&character_id];
        let prior = existing_assignment(profile, &slot_id).cloned();
        let entries = catalog
            .entries
            .values()
            .filter(|entry| entry.slot_id == slot_id)
            .collect::<Vec<_>>();
        let mut trace = entries
            .iter()
            .map(|entry| {
                let explicitly_excluded = entry
                    .excluded_character_ids
                    .binary_search(&character_id)
                    .is_ok();
                let explicitly_eligible = entry_eligible(entry, &character_id);
                let used_before = usage
                    .get(&slot_id)
                    .and_then(|counts| counts.get(&entry.id))
                    .copied()
                    .unwrap_or(0);
                let capacity_available = entry.capacity.is_none_or(|limit| used_before < limit);
                PresentationCandidateTrace {
                    entry_id: entry.id.clone(),
                    explicitly_eligible,
                    explicitly_excluded,
                    used_before,
                    capacity: entry.capacity,
                    capacity_available,
                    seeded_sha256: allocation_seed(
                        request.seed,
                        &character_id,
                        &slot_id,
                        &entry.id,
                    ),
                    selected: false,
                }
            })
            .collect::<Vec<_>>();
        let selected = trace
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.explicitly_eligible
                    && !candidate.explicitly_excluded
                    && candidate.capacity_available
            })
            .min_by(|(_, left), (_, right)| {
                left.used_before
                    .cmp(&right.used_before)
                    .then_with(|| left.seeded_sha256.cmp(&right.seeded_sha256))
                    .then_with(|| left.entry_id.cmp(&right.entry_id))
            })
            .map(|(index, _)| index);
        let allocation = if let Some(index) = selected {
            trace[index].selected = true;
            let entry_id = trace[index].entry_id.clone();
            let entry = &catalog.entries[&entry_id];
            *usage
                .entry(slot_id.clone())
                .or_default()
                .entry(entry_id.clone())
                .or_default() += 1;
            PresentationProposedAllocation {
                character_id: character_id.clone(),
                slot_id: slot_id.clone(),
                disposition: PresentationAllocationDisposition::Proposed,
                prior,
                proposed_entry_id: Some(entry_id),
                proposed_value: Some(entry.value.clone()),
                rationale: "Selected the least-used eligible entry; seed hash and entry id break ties deterministically.".to_owned(),
                trace,
            }
        } else {
            PresentationProposedAllocation {
                character_id: character_id.clone(),
                slot_id: slot_id.clone(),
                disposition: PresentationAllocationDisposition::Unavailable,
                prior,
                proposed_entry_id: None,
                proposed_value: None,
                rationale: "No explicitly eligible entry had remaining proposal capacity."
                    .to_owned(),
                trace,
            }
        };
        allocations
            .entry(character_id)
            .or_default()
            .insert(slot_id, allocation);
    }

    let distribution = proposal_distribution(&allocations);
    let proposal = PresentationProposal {
        proposal_format_version: PRESENTATION_PROPOSAL_FORMAT_VERSION,
        id: format!("{}.proposal", request.id),
        input_collection: collection.clone(),
        input_sha256,
        catalog: catalog.clone(),
        catalog_ref,
        request: request.clone(),
        request_sha256,
        allocations,
        distribution,
    };
    validate_presentation_proposal_structure(&proposal)?;
    Ok(proposal)
}

/// Validate and independently replay one serialized proposal.
pub fn validate_presentation_proposal(
    proposal: &PresentationProposal,
) -> Result<(), crate::CharacterError> {
    validate_presentation_proposal_structure(proposal)?;
    let expected = propose_presentation_allocations(
        &proposal.input_collection,
        &proposal.catalog,
        &proposal.request,
    )?;
    if expected != *proposal {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal",
            "presentation proposal does not match its immutable inputs",
        ));
    }
    Ok(())
}

fn validate_presentation_proposal_structure(
    proposal: &PresentationProposal,
) -> Result<(), crate::CharacterError> {
    if proposal.proposal_format_version != PRESENTATION_PROPOSAL_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "proposal_format_version",
            "unsupported presentation proposal version",
        ));
    }
    validate_namespaced_id("id", &proposal.id)?;
    validate_collection(&proposal.input_collection)?;
    validate_sha256("input_sha256", &proposal.input_sha256)?;
    validate_presentation_catalog(&proposal.catalog)?;
    validate_presentation_catalog_ref("catalog_ref", &proposal.catalog_ref)?;
    validate_presentation_allocation_request(&proposal.request)?;
    validate_sha256("request_sha256", &proposal.request_sha256)?;
    if proposal.allocations.len() != proposal.request.character_ids.len() {
        return Err(invalid(
            "allocations",
            "proposal must contain every selected character",
        ));
    }
    for character_id in &proposal.request.character_ids {
        let slots = proposal.allocations.get(character_id).ok_or_else(|| {
            error(
                CharacterDiagnosticCode::InvalidReference,
                format!("allocations.{character_id}"),
                "proposal is missing one selected character",
            )
        })?;
        if slots.len() != proposal.request.slot_ids.len() {
            return Err(invalid(
                format!("allocations.{character_id}"),
                "proposal must contain every selected slot",
            ));
        }
        for slot_id in &proposal.request.slot_ids {
            let allocation = slots.get(slot_id).ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("allocations.{character_id}.{slot_id}"),
                    "proposal is missing one selected slot",
                )
            })?;
            if allocation.character_id != *character_id || allocation.slot_id != *slot_id {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("allocations.{character_id}.{slot_id}"),
                    "allocation identity does not match its map coordinates",
                ));
            }
            validate_text(
                &format!("allocations.{character_id}.{slot_id}.rationale"),
                &allocation.rationale,
                1,
                2_048,
            )?;
            match allocation.disposition {
                PresentationAllocationDisposition::Proposed => {
                    let entry_id = allocation.proposed_entry_id.as_deref().ok_or_else(|| {
                        invalid(
                            format!("allocations.{character_id}.{slot_id}.proposed_entry_id"),
                            "proposed allocation requires one entry",
                        )
                    })?;
                    validate_local_id("proposed_entry_id", entry_id)?;
                    let value = allocation.proposed_value.as_ref().ok_or_else(|| {
                        invalid(
                            format!("allocations.{character_id}.{slot_id}.proposed_value"),
                            "proposed allocation requires one value",
                        )
                    })?;
                    validate_presentation_catalog_value("proposed_value", value)?;
                    if allocation.trace.iter().filter(|item| item.selected).count() != 1 {
                        return Err(invalid(
                            format!("allocations.{character_id}.{slot_id}.trace"),
                            "proposed allocation trace requires exactly one selection",
                        ));
                    }
                }
                PresentationAllocationDisposition::Retained
                | PresentationAllocationDisposition::Unavailable => {
                    if allocation.proposed_entry_id.is_some()
                        || allocation.proposed_value.is_some()
                        || allocation.trace.iter().any(|item| item.selected)
                    {
                        return Err(invalid(
                            format!("allocations.{character_id}.{slot_id}"),
                            "non-proposed allocation cannot retain a proposed selection",
                        ));
                    }
                    if allocation.disposition == PresentationAllocationDisposition::Retained
                        && allocation.prior.is_none()
                    {
                        return Err(invalid(
                            format!("allocations.{character_id}.{slot_id}.prior"),
                            "retained allocation requires a prior assignment",
                        ));
                    }
                }
            }
            for trace in &allocation.trace {
                validate_local_id("trace.entry_id", &trace.entry_id)?;
                validate_sha256("trace.seeded_sha256", &trace.seeded_sha256)?;
            }
        }
    }
    Ok(())
}

/// Build and validate a complete review for a proposal.
pub fn review_presentation_proposal(
    proposal: &PresentationProposal,
    reviewer: impl Into<String>,
    rationale: impl Into<String>,
    decisions: BTreeMap<String, BTreeMap<String, PresentationReviewDecision>>,
) -> Result<PresentationReview, crate::CharacterError> {
    validate_presentation_proposal(proposal)?;
    let review = PresentationReview {
        review_format_version: PRESENTATION_REVIEW_FORMAT_VERSION,
        proposal_sha256: canonical_hash(proposal)?,
        reviewer: reviewer.into(),
        rationale: rationale.into(),
        decisions,
    };
    validate_presentation_review(proposal, &review)?;
    Ok(review)
}

/// Validate a complete review against the exact immutable proposal.
pub fn validate_presentation_review(
    proposal: &PresentationProposal,
    review: &PresentationReview,
) -> Result<(), crate::CharacterError> {
    validate_presentation_proposal(proposal)?;
    validate_presentation_review_structure(review)?;
    if review.proposal_sha256 != canonical_hash(proposal)? {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "proposal_sha256",
            "presentation review does not fingerprint this proposal",
        ));
    }
    let proposed =
        proposal
            .allocations
            .iter()
            .flat_map(|(character_id, slots)| {
                slots.iter().filter_map(move |(slot_id, allocation)| {
                    (allocation.disposition == PresentationAllocationDisposition::Proposed)
                        .then_some((character_id.as_str(), slot_id.as_str(), allocation))
                })
            })
            .collect::<Vec<_>>();
    let decision_count = review.decisions.values().map(BTreeMap::len).sum::<usize>();
    if decision_count != proposed.len() {
        return Err(invalid(
            "decisions",
            "review requires exactly one decision for every proposed allocation",
        ));
    }
    let mut accepted_usage = retained_usage(
        &proposal.input_collection,
        &proposal.catalog,
        &proposal.request,
    );
    for (character_id, slot_id, allocation) in proposed {
        let decision = review
            .decisions
            .get(character_id)
            .and_then(|slots| slots.get(slot_id))
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("decisions.{character_id}.{slot_id}"),
                    "review is missing one proposed allocation decision",
                )
            })?;
        let selected_entry = match decision {
            PresentationReviewDecision::Accept { rationale, .. } => {
                if let Some(rationale) = rationale {
                    validate_text("decisions.accept.rationale", rationale, 1, 2_048)?;
                }
                allocation.proposed_entry_id.as_deref()
            }
            PresentationReviewDecision::Edit {
                entry_id,
                rationale,
                ..
            } => {
                validate_local_id("decisions.edit.entry_id", entry_id)?;
                validate_text("decisions.edit.rationale", rationale, 1, 2_048)?;
                let entry = proposal.catalog.entries.get(entry_id).ok_or_else(|| {
                    error(
                        CharacterDiagnosticCode::InvalidReference,
                        "decisions.edit.entry_id",
                        "edited allocation references an absent catalog entry",
                    )
                })?;
                if entry.slot_id != slot_id || !entry_eligible(entry, character_id) {
                    return Err(error(
                        CharacterDiagnosticCode::ForbiddenWriteBack,
                        "decisions.edit.entry_id",
                        "edited allocation must use an explicitly eligible entry in the same slot",
                    ));
                }
                Some(entry_id.as_str())
            }
            PresentationReviewDecision::Override {
                value, rationale, ..
            } => {
                validate_text("decisions.override.rationale", rationale, 1, 2_048)?;
                validate_presentation_catalog_value("decisions.override.value", value)?;
                if value_kind(value) != proposal.catalog.slots[slot_id].value_kind {
                    return Err(invalid(
                        "decisions.override.value",
                        "override value kind must match the selected presentation slot",
                    ));
                }
                if let PresentationCatalogValue::Asset { asset } = value
                    && proposal
                        .request
                        .available_asset_paths
                        .binary_search(&asset.path)
                        .is_err()
                {
                    return Err(error(
                        CharacterDiagnosticCode::InvalidReference,
                        "decisions.override.value.asset.path",
                        "presentation override asset is absent from the declared project inventory",
                    ));
                }
                None
            }
            PresentationReviewDecision::Reject { rationale }
            | PresentationReviewDecision::Withhold { rationale } => {
                validate_text("decisions.rationale", rationale, 1, 2_048)?;
                None
            }
        };
        if let Some(entry_id) = selected_entry {
            let entry = &proposal.catalog.entries[entry_id];
            let count = accepted_usage
                .entry(slot_id.to_owned())
                .or_default()
                .entry(entry_id.to_owned())
                .or_default();
            if entry.capacity.is_some_and(|capacity| *count >= capacity) {
                return Err(error(
                    CharacterDiagnosticCode::ConflictingOverlay,
                    format!("decisions.{character_id}.{slot_id}"),
                    "reviewed allocations exceed the catalog entry capacity",
                ));
            }
            *count += 1;
        }
    }
    for (character_id, slots) in &review.decisions {
        if !proposal.allocations.contains_key(character_id) {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("decisions.{character_id}"),
                "review targets an unselected character",
            ));
        }
        for slot_id in slots.keys() {
            if proposal.allocations[character_id]
                .get(slot_id)
                .is_none_or(|allocation| {
                    allocation.disposition != PresentationAllocationDisposition::Proposed
                })
            {
                return Err(error(
                    CharacterDiagnosticCode::InvalidReference,
                    format!("decisions.{character_id}.{slot_id}"),
                    "review targets an allocation that was not proposed",
                ));
            }
        }
    }
    Ok(())
}

/// Validate the standalone review shape. Proposal-dependent constraints are checked at apply.
pub fn validate_presentation_review_structure(
    review: &PresentationReview,
) -> Result<(), crate::CharacterError> {
    if review.review_format_version != PRESENTATION_REVIEW_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "review_format_version",
            "unsupported presentation review version",
        ));
    }
    validate_sha256("proposal_sha256", &review.proposal_sha256)?;
    validate_text("reviewer", &review.reviewer, 1, 512)?;
    validate_text("rationale", &review.rationale, 1, 2_048)?;
    for (character_id, slots) in &review.decisions {
        validate_namespaced_id("decisions.character_id", character_id)?;
        if slots.is_empty() {
            return Err(invalid(
                format!("decisions.{character_id}"),
                "decision maps cannot be empty",
            ));
        }
        for slot_id in slots.keys() {
            validate_local_id("decisions.slot_id", slot_id)?;
        }
        for decision in slots.values() {
            match decision {
                PresentationReviewDecision::Accept { rationale, .. } => {
                    if let Some(rationale) = rationale {
                        validate_text("decisions.accept.rationale", rationale, 1, 2_048)?;
                    }
                }
                PresentationReviewDecision::Edit {
                    entry_id,
                    rationale,
                    ..
                } => {
                    validate_local_id("decisions.edit.entry_id", entry_id)?;
                    validate_text("decisions.edit.rationale", rationale, 1, 2_048)?;
                }
                PresentationReviewDecision::Override {
                    value, rationale, ..
                } => {
                    validate_presentation_catalog_value("decisions.override.value", value)?;
                    validate_text("decisions.override.rationale", rationale, 1, 2_048)?;
                }
                PresentationReviewDecision::Reject { rationale }
                | PresentationReviewDecision::Withhold { rationale } => {
                    validate_text("decisions.rationale", rationale, 1, 2_048)?;
                }
            }
        }
    }
    Ok(())
}

/// Apply a complete review atomically and return an independently reproducible receipt.
pub fn apply_presentation_review(
    current_collection: &CharacterCollection,
    proposal: &PresentationProposal,
    review: &PresentationReview,
) -> Result<PresentationReceipt, crate::CharacterError> {
    build_presentation_receipt(current_collection, proposal, review)
}

fn build_presentation_receipt(
    current_collection: &CharacterCollection,
    proposal: &PresentationProposal,
    review: &PresentationReview,
) -> Result<PresentationReceipt, crate::CharacterError> {
    validate_collection(current_collection)?;
    validate_presentation_proposal(proposal)?;
    validate_presentation_review(proposal, review)?;
    let current_sha256 = collection_hash(current_collection)?;
    if current_sha256 != proposal.input_sha256 || *current_collection != proposal.input_collection {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "current_collection",
            "presentation proposal does not target the current collection",
        ));
    }
    let proposal_sha256 = canonical_hash(proposal)?;
    let review_sha256 = canonical_hash(review)?;
    let lineage = presentation_lineage(&proposal.catalog.provenance);
    let mut output = current_collection.clone();
    let mut operations = BTreeMap::<String, Vec<CharacterOperation>>::new();

    for (character_id, slot_decisions) in &review.decisions {
        let profile = &current_collection.characters[character_id];
        let mut character_operations = Vec::new();
        for (slot_id, decision) in slot_decisions {
            let allocation = &proposal.allocations[character_id][slot_id];
            let (entry_id, value, state, lock, rationale) = match decision {
                PresentationReviewDecision::Accept { lock, rationale } => (
                    allocation
                        .proposed_entry_id
                        .clone()
                        .expect("validated proposal has an entry"),
                    allocation
                        .proposed_value
                        .clone()
                        .expect("validated proposal has a value"),
                    ValueState::Reviewed,
                    *lock,
                    rationale
                        .clone()
                        .unwrap_or_else(|| review.rationale.clone()),
                ),
                PresentationReviewDecision::Edit {
                    entry_id,
                    lock,
                    rationale,
                } => (
                    entry_id.clone(),
                    proposal.catalog.entries[entry_id].value.clone(),
                    ValueState::Reviewed,
                    *lock,
                    rationale.clone(),
                ),
                PresentationReviewDecision::Override {
                    value,
                    lock,
                    rationale,
                } => (
                    "author_override".to_owned(),
                    value.clone(),
                    if allocation.prior.is_some() {
                        ValueState::Overridden
                    } else {
                        ValueState::Authored
                    },
                    *lock,
                    rationale.clone(),
                ),
                PresentationReviewDecision::Reject { .. }
                | PresentationReviewDecision::Withhold { .. } => continue,
            };
            let action = CharacterOperationAction::UpsertPresentationAssignment {
                value: Attributed {
                    value: PresentationCatalogAssignment {
                        slot_id: slot_id.clone(),
                        catalog: proposal.catalog_ref.clone(),
                        entry_id,
                        value,
                        proposal_sha256: proposal_sha256.clone(),
                        review_sha256: review_sha256.clone(),
                    },
                    state,
                    confidence: Confidence::High,
                    review: ReviewState::Accepted,
                    lock,
                    freshness: Freshness::Current,
                    lineage: lineage.clone(),
                    rationale: Some(rationale.clone()),
                },
            };
            character_operations.push(CharacterOperation {
                id: format!("presentation_{slot_id}"),
                expected_prior_sha256: current_value_hash(profile, &action)?,
                rationale,
                action,
            });
        }
        character_operations.sort_by(|left, right| left.id.cmp(&right.id));
        if character_operations.is_empty() {
            continue;
        }
        let candidate = apply_profile_operations(
            profile,
            &character_operations,
            &proposal.catalog.provenance,
            &proposal.request.id,
        )?;
        assert_presentation_only(profile, &candidate)?;
        output.characters.insert(character_id.clone(), candidate);
        operations.insert(character_id.clone(), character_operations);
    }
    if output.characters != current_collection.characters {
        output.revision = output.revision.checked_add(1).ok_or_else(|| {
            invalid(
                "output_collection.revision",
                "collection revision overflowed",
            )
        })?;
    }
    validate_collection(&output)?;
    let output_sha256 = collection_hash(&output)?;
    let receipt = PresentationReceipt {
        receipt_format_version: PRESENTATION_RECEIPT_FORMAT_VERSION,
        id: format!("{}.receipt", proposal.request.id),
        proposal: proposal.clone(),
        review: review.clone(),
        proposal_sha256,
        review_sha256,
        operations,
        distribution: collection_distribution(&output, &proposal.catalog_ref),
        output_collection: output,
        output_sha256,
    };
    validate_presentation_receipt_structure(&receipt)?;
    Ok(receipt)
}

/// Independently replay one receipt and compare every byte-bearing field.
pub fn validate_presentation_receipt(
    receipt: &PresentationReceipt,
) -> Result<(), crate::CharacterError> {
    validate_presentation_receipt_structure(receipt)?;
    let expected = build_presentation_receipt(
        &receipt.proposal.input_collection,
        &receipt.proposal,
        &receipt.review,
    )?;
    if expected != *receipt {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "receipt",
            "presentation receipt does not match its proposal, review, and atomic output",
        ));
    }
    Ok(())
}

fn validate_presentation_receipt_structure(
    receipt: &PresentationReceipt,
) -> Result<(), crate::CharacterError> {
    if receipt.receipt_format_version != PRESENTATION_RECEIPT_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "receipt_format_version",
            "unsupported presentation receipt version",
        ));
    }
    validate_namespaced_id("id", &receipt.id)?;
    validate_sha256("proposal_sha256", &receipt.proposal_sha256)?;
    validate_sha256("review_sha256", &receipt.review_sha256)?;
    validate_sha256("output_sha256", &receipt.output_sha256)?;
    validate_collection(&receipt.output_collection)
}

/// Apply an exact lock or unlock revision without changing assignment value or provenance.
pub fn apply_presentation_lock_revision(
    collection: &CharacterCollection,
    revision: &PresentationLockRevision,
) -> Result<CharacterCollection, crate::CharacterError> {
    validate_collection(collection)?;
    validate_presentation_lock_revision(revision)?;
    if collection_hash(collection)? != revision.expected_input_sha256 {
        return Err(error(
            CharacterDiagnosticCode::StaleInput,
            "expected_input_sha256",
            "presentation lock revision does not target the current collection",
        ));
    }
    let mut output = collection.clone();
    for target in &revision.targets {
        let profile = output
            .characters
            .get_mut(&target.character_id)
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    "targets.character_id",
                    "presentation lock target character is absent",
                )
            })?;
        let assignment = identity_presentation_mut(profile)
            .and_then(|presentation| presentation.catalog_assignments.get_mut(&target.slot_id))
            .ok_or_else(|| {
                error(
                    CharacterDiagnosticCode::InvalidReference,
                    "targets.slot_id",
                    "presentation lock target assignment is absent",
                )
            })?;
        assignment.lock = revision.lock;
    }
    if output != *collection {
        output.revision = output.revision.checked_add(1).ok_or_else(|| {
            invalid(
                "output_collection.revision",
                "collection revision overflowed",
            )
        })?;
    }
    validate_collection(&output)?;
    Ok(output)
}

/// Validate one standalone lock/unlock revision.
pub fn validate_presentation_lock_revision(
    revision: &PresentationLockRevision,
) -> Result<(), crate::CharacterError> {
    if revision.revision_format_version != PRESENTATION_LOCK_REVISION_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "revision_format_version",
            "unsupported presentation lock revision version",
        ));
    }
    validate_namespaced_id("id", &revision.id)?;
    validate_sha256("expected_input_sha256", &revision.expected_input_sha256)?;
    validate_text("rationale", &revision.rationale, 1, 2_048)?;
    validate_provenance(&revision.provenance).map_err(|_| {
        error(
            CharacterDiagnosticCode::InvalidLineage,
            "provenance",
            "presentation lock revision provenance is invalid",
        )
    })?;
    if revision.targets.is_empty() || revision.targets.len() > 65_536 {
        return Err(invalid(
            "targets",
            "lock revision requires one through 65536 targets",
        ));
    }
    let mut previous = None;
    for target in &revision.targets {
        validate_namespaced_id("targets.character_id", &target.character_id)?;
        validate_local_id("targets.slot_id", &target.slot_id)?;
        if previous.is_some_and(|prior: &PresentationLockTarget| prior >= target) {
            return Err(invalid("targets", "lock targets must be unique and sorted"));
        }
        previous = Some(target);
    }
    Ok(())
}

fn apply_profile_operations(
    profile: &CharacterProfile,
    operations: &[CharacterOperation],
    provenance: &Provenance,
    overlay_id: &str,
) -> Result<CharacterProfile, crate::CharacterError> {
    let template = CharacterTemplate {
        template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
        id: format!("{}.presentation_base", profile.id),
        version: "1.0.0".to_owned(),
        profile: profile.clone(),
    };
    let overlay = CharacterOverlay {
        overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
        id: overlay_id.to_owned(),
        character_id: profile.id.clone(),
        template: Some(CharacterTemplateRef {
            id: template.id.clone(),
            version: template.version.clone(),
            sha256: template_fingerprint(&template)?,
        }),
        operations: operations.to_vec(),
        provenance: merge_provenance(&profile.provenance, provenance)?,
    };
    Ok(synthesize_character(Some(&template), &overlay)?.effective_profile)
}

fn assert_presentation_only(
    input: &CharacterProfile,
    output: &CharacterProfile,
) -> Result<(), crate::CharacterError> {
    if input.id != output.id
        || input.canon != output.canon
        || input.derived != output.derived
        || input.suggestions != output.suggestions
    {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "output_profile",
            "presentation allocation changed protected Character data",
        ));
    }
    let mut expected_extensions = input.extensions.clone();
    if let Some(extension) = output
        .extensions
        .get(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
    {
        expected_extensions.insert(
            crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE.to_owned(),
            extension.clone(),
        );
    }
    if expected_extensions != output.extensions {
        return Err(error(
            CharacterDiagnosticCode::ForbiddenWriteBack,
            "output_profile.extensions",
            "presentation allocation changed an extension outside its authority",
        ));
    }
    Ok(())
}

fn retain_existing(
    assignment: &Attributed<PresentationCatalogAssignment>,
    mode: PresentationAllocationMode,
) -> bool {
    mode == PresentationAllocationMode::FillMissing
        || assignment.lock == LockState::Locked
        || matches!(
            assignment.state,
            ValueState::Authored | ValueState::Overridden
        )
}

fn retained_usage(
    collection: &CharacterCollection,
    catalog: &PresentationCatalog,
    request: &PresentationAllocationRequest,
) -> BTreeMap<String, BTreeMap<String, u32>> {
    let selected_characters = request.character_ids.iter().collect::<BTreeSet<_>>();
    let selected_slots = request.slot_ids.iter().collect::<BTreeSet<_>>();
    let mut usage = BTreeMap::<String, BTreeMap<String, u32>>::new();
    for (character_id, profile) in &collection.characters {
        let Some(presentation) = identity_presentation(profile) else {
            continue;
        };
        for (slot_id, assignment) in &presentation.catalog_assignments {
            if !selected_slots.contains(slot_id)
                || (selected_characters.contains(character_id)
                    && !retain_existing(assignment, request.mode))
                || assignment.value.catalog != request.catalog
                || !catalog.entries.contains_key(&assignment.value.entry_id)
            {
                continue;
            }
            *usage
                .entry(slot_id.clone())
                .or_default()
                .entry(assignment.value.entry_id.clone())
                .or_default() += 1;
        }
    }
    usage
}

fn entry_eligible(entry: &PresentationCatalogEntry, character_id: &str) -> bool {
    if entry
        .excluded_character_ids
        .binary_search_by(|candidate| candidate.as_str().cmp(character_id))
        .is_ok()
    {
        return false;
    }
    entry
        .eligible_character_ids
        .binary_search_by(|candidate| candidate.as_str().cmp(character_id))
        .is_ok()
        || entry.eligible_id_prefixes.iter().any(|prefix| {
            character_id == prefix
                || character_id
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        })
}

fn value_kind(value: &PresentationCatalogValue) -> PresentationCatalogValueKind {
    match value {
        PresentationCatalogValue::Appearance { .. } => PresentationCatalogValueKind::Appearance,
        PresentationCatalogValue::PaletteColor { .. } => PresentationCatalogValueKind::PaletteColor,
        PresentationCatalogValue::StyleTag { .. } => PresentationCatalogValueKind::StyleTag,
        PresentationCatalogValue::Asset { .. } => PresentationCatalogValueKind::Asset,
    }
}

fn validate_requested_assets(
    catalog: &PresentationCatalog,
    request: &PresentationAllocationRequest,
) -> Result<(), crate::CharacterError> {
    let available = request
        .available_asset_paths
        .iter()
        .collect::<BTreeSet<_>>();
    let selected_slots = request.slot_ids.iter().collect::<BTreeSet<_>>();
    for entry in catalog.entries.values() {
        if !selected_slots.contains(&entry.slot_id) {
            continue;
        }
        if let PresentationCatalogValue::Asset { asset } = &entry.value
            && !available.contains(&asset.path)
        {
            return Err(error(
                CharacterDiagnosticCode::InvalidReference,
                format!("available_asset_paths.{}", asset.id),
                "selected catalog references an asset absent from the declared project inventory",
            ));
        }
    }
    Ok(())
}

fn proposal_distribution(
    allocations: &BTreeMap<String, BTreeMap<String, PresentationProposedAllocation>>,
) -> PresentationDistribution {
    let mut result = PresentationDistribution::default();
    for (character_id, slots) in allocations {
        for (slot_id, allocation) in slots {
            let entry_id = match allocation.disposition {
                PresentationAllocationDisposition::Retained => allocation
                    .prior
                    .as_ref()
                    .map(|prior| prior.value.entry_id.as_str()),
                PresentationAllocationDisposition::Proposed => {
                    allocation.proposed_entry_id.as_deref()
                }
                PresentationAllocationDisposition::Unavailable => None,
            };
            if let Some(entry_id) = entry_id {
                *result
                    .counts
                    .entry(slot_id.clone())
                    .or_default()
                    .entry(entry_id.to_owned())
                    .or_default() += 1;
            } else if allocation.disposition == PresentationAllocationDisposition::Unavailable {
                result
                    .unavailable
                    .entry(slot_id.clone())
                    .or_default()
                    .push(character_id.clone());
            }
        }
    }
    result
}

fn collection_distribution(
    collection: &CharacterCollection,
    catalog: &PresentationCatalogRef,
) -> PresentationDistribution {
    let mut result = PresentationDistribution::default();
    for profile in collection.characters.values() {
        let Some(presentation) = identity_presentation(profile) else {
            continue;
        };
        for (slot_id, assignment) in &presentation.catalog_assignments {
            if assignment.value.catalog == *catalog {
                *result
                    .counts
                    .entry(slot_id.clone())
                    .or_default()
                    .entry(assignment.value.entry_id.clone())
                    .or_default() += 1;
            }
        }
    }
    result
}

fn existing_assignment<'a>(
    profile: &'a CharacterProfile,
    slot_id: &str,
) -> Option<&'a Attributed<PresentationCatalogAssignment>> {
    identity_presentation(profile)?
        .catalog_assignments
        .get(slot_id)
}

fn identity_presentation(profile: &CharacterProfile) -> Option<&crate::IdentityPresentation> {
    match profile
        .extensions
        .get(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::IdentityPresentation(record)) => Some(&record.value),
        _ => None,
    }
}

fn identity_presentation_mut(
    profile: &mut CharacterProfile,
) -> Option<&mut crate::IdentityPresentation> {
    match profile
        .extensions
        .get_mut(crate::IDENTITY_PRESENTATION_EXTENSION_NAMESPACE)
    {
        Some(CharacterExtension::IdentityPresentation(record)) => Some(&mut record.value),
        _ => None,
    }
}

fn presentation_lineage(provenance: &Provenance) -> Vec<String> {
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
    lineage
}

fn allocation_seed(seed: u64, character_id: &str, slot_id: &str, entry_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(seed.to_be_bytes());
    digest.update([0]);
    digest.update(character_id.as_bytes());
    digest.update([0]);
    digest.update(slot_id.as_bytes());
    digest.update([0]);
    digest.update(entry_id.as_bytes());
    format!("{:x}", digest.finalize())
}

fn validate_sorted_namespaced_ids(
    path: &str,
    values: &[String],
) -> Result<(), crate::CharacterError> {
    validate_sorted(values, path, |value| validate_namespaced_id(path, value))
}

fn validate_sorted_local_ids(path: &str, values: &[String]) -> Result<(), crate::CharacterError> {
    validate_sorted(values, path, |value| validate_local_id(path, value))
}

fn validate_sorted_relative_paths(
    path: &str,
    values: &[String],
) -> Result<(), crate::CharacterError> {
    validate_sorted(values, path, |value| validate_relative_path(path, value))
}

fn validate_sorted_prefixes(path: &str, values: &[String]) -> Result<(), crate::CharacterError> {
    validate_sorted(values, path, |value| validate_namespaced_id(path, value))
}

fn validate_sorted(
    values: &[String],
    path: &str,
    mut validate: impl FnMut(&str) -> Result<(), crate::CharacterError>,
) -> Result<(), crate::CharacterError> {
    if values.len() > 65_536 {
        return Err(invalid(path, "too many sorted values"));
    }
    let mut previous = None;
    for value in values {
        validate(value)?;
        if previous.is_some_and(|prior: &str| prior >= value.as_str()) {
            return Err(invalid(path, "values must be unique and sorted"));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

fn validate_collection(collection: &CharacterCollection) -> Result<(), crate::CharacterError> {
    if collection.collection_format_version != CHARACTER_COLLECTION_FORMAT_VERSION {
        return Err(error(
            CharacterDiagnosticCode::UnsupportedVersion,
            "collection_format_version",
            "unsupported Character collection version",
        ));
    }
    validate_character_collection(collection).map_err(|failure| {
        error(
            failure.diagnostic().code,
            failure.diagnostic().path.clone(),
            "Character collection is invalid for presentation allocation",
        )
    })
}

fn collection_hash(collection: &CharacterCollection) -> Result<String, crate::CharacterError> {
    collection_fingerprint(collection).map_err(|_| encoding_error())
}

fn canonical_hash(value: &impl Serialize) -> Result<String, crate::CharacterError> {
    let bytes = serde_json::to_vec(value).map_err(|_| encoding_error())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn invalid(path: impl Into<String>, message: &'static str) -> crate::CharacterError {
    error(CharacterDiagnosticCode::InvalidValue, path, message)
}

fn encoding_error() -> crate::CharacterError {
    error(
        CharacterDiagnosticCode::InvalidEncoding,
        "document",
        "presentation document does not match the strict serialized contract",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use weave_domain::ProvenanceSource;

    const COLLECTION: &str = include_str!(
        "../../../examples/domain-modules/weave-character/presentation/input.character-collection.json"
    );

    fn catalog_provenance() -> Provenance {
        Provenance {
            sources: vec![ProvenanceSource {
                id: "presentation_catalog_original".to_owned(),
                kind: ProvenanceKind::Original,
                url: "https://github.com/chrisgliddon/weave".to_owned(),
                revision: "presentation-catalog-v1".to_owned(),
                sha256: None,
                license: "MIT".to_owned(),
                license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
                attribution: "Original synthetic presentation catalog test data.".to_owned(),
                modified: false,
            }],
            transformations: Vec::new(),
            claims: BTreeMap::from([(
                "entries".to_owned(),
                vec!["presentation_catalog_original".to_owned()],
            )]),
        }
    }

    fn catalog() -> PresentationCatalog {
        PresentationCatalog {
            catalog_format_version: PRESENTATION_CATALOG_FORMAT_VERSION,
            id: "org.weave.character.presentation.glasswind".to_owned(),
            version: "1.0.0".to_owned(),
            title: "Glasswind presentation tokens".to_owned(),
            description: "Synthetic presentation-only tokens for deterministic allocation tests."
                .to_owned(),
            independently_authored: true,
            license: "MIT".to_owned(),
            license_url: "https://github.com/chrisgliddon/weave/blob/main/LICENSE".to_owned(),
            slots: BTreeMap::from([(
                "visual_tone".to_owned(),
                PresentationCatalogSlot {
                    id: "visual_tone".to_owned(),
                    label: "Visual tone".to_owned(),
                    description: "One synthetic presentation tag.".to_owned(),
                    value_kind: PresentationCatalogValueKind::StyleTag,
                },
            )]),
            entries: BTreeMap::from([
                (
                    "ember_glow".to_owned(),
                    PresentationCatalogEntry {
                        id: "ember_glow".to_owned(),
                        slot_id: "visual_tone".to_owned(),
                        label: "Ember glow".to_owned(),
                        value: PresentationCatalogValue::StyleTag {
                            tag: "ember_glow".to_owned(),
                        },
                        eligible_character_ids: Vec::new(),
                        eligible_id_prefixes: vec!["org.weave.character".to_owned()],
                        excluded_character_ids: Vec::new(),
                        capacity: Some(1),
                    },
                ),
                (
                    "river_blue".to_owned(),
                    PresentationCatalogEntry {
                        id: "river_blue".to_owned(),
                        slot_id: "visual_tone".to_owned(),
                        label: "River blue".to_owned(),
                        value: PresentationCatalogValue::StyleTag {
                            tag: "river_blue".to_owned(),
                        },
                        eligible_character_ids: Vec::new(),
                        eligible_id_prefixes: vec!["org.weave.character".to_owned()],
                        excluded_character_ids: Vec::new(),
                        capacity: Some(1),
                    },
                ),
            ]),
            provenance: catalog_provenance(),
        }
    }

    fn request(
        collection: &CharacterCollection,
        catalog: &PresentationCatalog,
        mode: PresentationAllocationMode,
    ) -> PresentationAllocationRequest {
        PresentationAllocationRequest {
            request_format_version: PRESENTATION_ALLOCATION_REQUEST_FORMAT_VERSION,
            id: "org.weave.character.presentation.allocate".to_owned(),
            expected_input_sha256: collection_hash(collection).unwrap(),
            catalog: presentation_catalog_ref(catalog).unwrap(),
            seed: 20_260_822,
            character_ids: collection.characters.keys().cloned().collect(),
            slot_ids: vec!["visual_tone".to_owned()],
            mode,
            available_asset_paths: Vec::new(),
        }
    }

    fn accept_all(proposal: &PresentationProposal, first_lock: LockState) -> PresentationReview {
        let first_id = proposal.request.character_ids.first().unwrap();
        let decisions = proposal
            .allocations
            .iter()
            .map(|(character_id, slots)| {
                let decisions = slots
                    .iter()
                    .filter(|(_, allocation)| {
                        allocation.disposition == PresentationAllocationDisposition::Proposed
                    })
                    .map(|(slot_id, _)| {
                        (
                            slot_id.clone(),
                            PresentationReviewDecision::Accept {
                                lock: if character_id == first_id {
                                    first_lock
                                } else {
                                    LockState::Unlocked
                                },
                                rationale: None,
                            },
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                (character_id.clone(), decisions)
            })
            .filter(|(_, decisions)| !decisions.is_empty())
            .collect();
        review_presentation_proposal(
            proposal,
            "org.weave.reviewer.presentation",
            "Accept the balanced synthetic presentation allocation.",
            decisions,
        )
        .unwrap()
    }

    #[test]
    fn allocation_is_balanced_replayable_and_presentation_only() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let catalog = catalog();
        let request = request(
            &collection,
            &catalog,
            PresentationAllocationMode::FillMissing,
        );
        let proposal = propose_presentation_allocations(&collection, &catalog, &request).unwrap();
        assert_eq!(proposal.distribution.counts["visual_tone"].len(), 2);
        assert!(
            proposal.distribution.counts["visual_tone"]
                .values()
                .all(|count| *count == 1)
        );
        assert_eq!(
            proposal,
            PresentationProposal::from_json(&proposal.to_json().unwrap()).unwrap()
        );
        assert_eq!(
            proposal,
            PresentationProposal::from_ron(&proposal.to_ron().unwrap()).unwrap()
        );

        let review = accept_all(&proposal, LockState::Locked);
        let receipt = apply_presentation_review(&collection, &proposal, &review).unwrap();
        assert_eq!(receipt.output_collection.revision, collection.revision + 1);
        assert_eq!(receipt.operations.len(), 2);
        for (id, before) in &collection.characters {
            let after = &receipt.output_collection.characters[id];
            assert_eq!(before.canon, after.canon);
            assert_eq!(before.derived, after.derived);
            assert_eq!(before.suggestions, after.suggestions);
            assert!(existing_assignment(after, "visual_tone").is_some());
            let weave_domain::DomainValue::Object(projected) =
                crate::character_profile_domain_value(after)
            else {
                panic!("Character domain projection must be an object");
            };
            let weave_domain::DomainValue::Object(presentation) = &projected["presentation"] else {
                panic!("presentation projection must be an object");
            };
            let weave_domain::DomainValue::Object(assignments) =
                &presentation["catalog_assignments"]
            else {
                panic!("catalog assignments projection must be an object");
            };
            assert!(assignments.contains_key("visual_tone"));
        }
        assert_eq!(
            receipt,
            PresentationReceipt::from_json(&receipt.to_json().unwrap()).unwrap()
        );
        assert_eq!(
            receipt,
            PresentationReceipt::from_ron(&receipt.to_ron().unwrap()).unwrap()
        );
    }

    #[test]
    fn rebalance_retains_locks_and_explicit_override_is_deterministic() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let catalog = catalog();
        let proposal = propose_presentation_allocations(
            &collection,
            &catalog,
            &request(
                &collection,
                &catalog,
                PresentationAllocationMode::FillMissing,
            ),
        )
        .unwrap();
        let receipt = apply_presentation_review(
            &collection,
            &proposal,
            &accept_all(&proposal, LockState::Locked),
        )
        .unwrap();
        let allocated = receipt.output_collection;
        let rebalance = propose_presentation_allocations(
            &allocated,
            &catalog,
            &request(&allocated, &catalog, PresentationAllocationMode::Rebalance),
        )
        .unwrap();
        let locked_id = rebalance.request.character_ids.first().unwrap();
        assert_eq!(
            rebalance.allocations[locked_id]["visual_tone"].disposition,
            PresentationAllocationDisposition::Retained
        );
        let unlocked_id = rebalance.request.character_ids.last().unwrap();
        assert_eq!(
            rebalance.allocations[unlocked_id]["visual_tone"].disposition,
            PresentationAllocationDisposition::Proposed
        );
        let decisions = BTreeMap::from([(
            unlocked_id.clone(),
            BTreeMap::from([(
                "visual_tone".to_owned(),
                PresentationReviewDecision::Override {
                    value: PresentationCatalogValue::StyleTag {
                        tag: "moonlit_ink".to_owned(),
                    },
                    lock: LockState::Locked,
                    rationale: "Use one explicit authored presentation override.".to_owned(),
                },
            )]),
        )]);
        let review = review_presentation_proposal(
            &rebalance,
            "org.weave.reviewer.presentation",
            "Retain one lock and review one explicit override.",
            decisions,
        )
        .unwrap();
        let overridden = apply_presentation_review(&allocated, &rebalance, &review).unwrap();
        let assignment = existing_assignment(
            &overridden.output_collection.characters[unlocked_id],
            "visual_tone",
        )
        .unwrap();
        assert_eq!(assignment.state, ValueState::Overridden);
        assert_eq!(assignment.value.entry_id, "author_override");
        assert_eq!(assignment.lock, LockState::Locked);
    }

    #[test]
    fn asset_inventory_and_ineligible_edits_fail_before_application() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let mut asset_catalog = catalog();
        asset_catalog.slots.insert(
            "avatar".to_owned(),
            PresentationCatalogSlot {
                id: "avatar".to_owned(),
                label: "Avatar".to_owned(),
                description: "One portable avatar reference.".to_owned(),
                value_kind: PresentationCatalogValueKind::Asset,
            },
        );
        asset_catalog.entries.insert(
            "lumen_avatar".to_owned(),
            PresentationCatalogEntry {
                id: "lumen_avatar".to_owned(),
                slot_id: "avatar".to_owned(),
                label: "Lumen avatar".to_owned(),
                value: PresentationCatalogValue::Asset {
                    asset: crate::PresentationAssetReference {
                        id: "lumen_avatar".to_owned(),
                        kind: crate::PresentationAssetKind::Avatar,
                        path: "assets/characters/lumen-avatar.svg".to_owned(),
                        media_type: "image/svg+xml".to_owned(),
                        sha256: None,
                        alt_text: "Synthetic geometric avatar.".to_owned(),
                    },
                },
                eligible_character_ids: collection.characters.keys().cloned().collect(),
                eligible_id_prefixes: Vec::new(),
                excluded_character_ids: Vec::new(),
                capacity: None,
            },
        );
        let mut request = request(
            &collection,
            &asset_catalog,
            PresentationAllocationMode::FillMissing,
        );
        request.slot_ids = vec!["avatar".to_owned()];
        assert!(propose_presentation_allocations(&collection, &asset_catalog, &request).is_err());
        request.available_asset_paths = vec!["assets/characters/lumen-avatar.svg".to_owned()];
        let proposal =
            propose_presentation_allocations(&collection, &asset_catalog, &request).unwrap();
        let character_id = proposal.request.character_ids.first().unwrap();
        let decisions = BTreeMap::from([(
            character_id.clone(),
            BTreeMap::from([(
                "avatar".to_owned(),
                PresentationReviewDecision::Edit {
                    entry_id: "ember_glow".to_owned(),
                    lock: LockState::Unlocked,
                    rationale: "Attempt an incompatible slot edit.".to_owned(),
                },
            )]),
        )]);
        assert!(
            review_presentation_proposal(
                &proposal,
                "org.weave.reviewer.presentation",
                "Exercise validation before application.",
                decisions,
            )
            .is_err()
        );
    }

    #[test]
    fn reviewed_catalog_receipt_round_trips_through_sparse_authoring_source() {
        let collection = CharacterCollection::from_json(COLLECTION).unwrap();
        let catalog = catalog();
        let proposal = propose_presentation_allocations(
            &collection,
            &catalog,
            &request(
                &collection,
                &catalog,
                PresentationAllocationMode::FillMissing,
            ),
        )
        .unwrap();
        let receipt = apply_presentation_review(
            &collection,
            &proposal,
            &accept_all(&proposal, LockState::Unlocked),
        )
        .unwrap();
        let character_id = proposal.request.character_ids.first().unwrap();
        let profile = collection.characters[character_id].clone();
        let template = CharacterTemplate {
            template_format_version: CHARACTER_TEMPLATE_FORMAT_VERSION,
            id: "org.weave.character.template.presentation_test".to_owned(),
            version: "1.0.0".to_owned(),
            profile: profile.clone(),
        };
        let overlay = CharacterOverlay {
            overlay_format_version: CHARACTER_OVERLAY_FORMAT_VERSION,
            id: "org.weave.character.overlay.presentation_test".to_owned(),
            character_id: character_id.clone(),
            template: Some(CharacterTemplateRef {
                id: template.id.clone(),
                version: template.version.clone(),
                sha256: template_fingerprint(&template).unwrap(),
            }),
            operations: Vec::new(),
            provenance: profile.provenance.clone(),
        };
        let workspace = crate::new_authoring_workspace(
            "org.weave.character.authoring.presentation_test",
            profile.provenance.clone(),
        )
        .and_then(|workspace| crate::create_authoring_draft(&workspace, Some(template), overlay))
        .unwrap();
        let draft = &workspace.drafts[character_id];
        let revision = crate::presentation_authoring_revision(
            draft,
            "org.weave.character.revision.presentation_test",
            "Adopt one reviewed deterministic catalog allocation.",
            receipt.clone(),
        )
        .unwrap();
        let preview = crate::preview_authoring_revision(&workspace, &revision).unwrap();
        assert_eq!(preview.candidate_draft.presentation_receipts, vec![receipt]);
        let assignment = preview
            .base_fields
            .iter()
            .find(|field| field.path.ends_with("catalog_assignments.visual_tone"))
            .unwrap();
        assert_eq!(
            assignment.origin_kind,
            crate::CharacterAuthoringFieldOriginKind::AcceptedSuggestion
        );
        assert!(assignment.overridden);
        let expected = crate::effective_authoring_profile(&preview.candidate_draft).unwrap();
        let applied = crate::apply_authoring_revision(&workspace, &revision).unwrap();
        assert_eq!(
            crate::export_authoring_profile(&applied, character_id, false).unwrap(),
            expected
        );
    }
}
