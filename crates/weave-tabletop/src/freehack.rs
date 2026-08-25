//! Verified CC0 Freehack adapter, configurable creation, and host-authoritative play.
//!
//! The adapter is independently specified as typed Rust behavior. It pins the official 2.1
//! release and source revision but does not redistribute the upstream PDF, icon, stylesheet, or
//! page composition. Public projections are deliberately separate from authority receipts so a
//! hidden check cannot leak through event envelopes, entropy state, section submissions, or
//! secret tracks.

use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, FieldDeclaration, TypeExpression, validate_typed_value};

use crate::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition,
    AdapterExtensionSurface, AdapterManifest, AdapterProvenance, AdapterSourceArtifact,
    AdapterSourceClass, CreationField, CreationFieldAuthority, CreationStep, EntropyState,
    EntropyStream, EventVisibility, ExcludedMaterial, HostVisibilityPolicy, LicenseTextReference,
    RESOLVER_CONTRACT_VERSION, ResolutionReceipt, ResolvedAdapter, ResolverEvent,
    ResolverOperationDeclaration, ResolverOutput, TabletopCapability,
    TabletopCapabilityDeclaration, TabletopError, TabletopEvent, TabletopResolver, TabletopState,
    canonical_fingerprint, resolved_adapter, validate_resolution_receipt, validate_tabletop_state,
};

/// Stable adapter identity shared by source, editor, runtime, and host integrations.
pub const FREEHACK_ADAPTER_ID: &str = "org.weave.tabletop.freehack";
/// First independently modeled adapter release for Freehack 2.1.
pub const FREEHACK_ADAPTER_VERSION: &str = "1.0.0";
/// Versioned campaign/character creation request and preview format.
pub const FREEHACK_CREATION_FORMAT_VERSION: u32 = 1;
/// Versioned exact probability request and preview format.
pub const FREEHACK_PROBABILITY_FORMAT_VERSION: u32 = 1;
/// Versioned authority/public receipt projection format.
pub const FREEHACK_PROJECTION_FORMAT_VERSION: u32 = 1;
/// Campaign-schema format understood by this adapter release.
pub const FREEHACK_CAMPAIGN_SCHEMA_VERSION: u32 = 1;
/// Official public release page for the reviewed 2.1 PDF.
pub const FREEHACK_RELEASE_URL: &str = "https://amini-allight.itch.io/freehack";
/// Official public source repository.
pub const FREEHACK_SOURCE_URL: &str = "https://gitlab.com/amini-allight/freehack";
/// Exact GitLab revision reviewed for this adapter.
pub const FREEHACK_SOURCE_REVISION: &str = "c98ac40f6b0bee2504c5c436dea2841a5d511ad9";
/// SHA-256 of the official `freehack.pdf` 2.1 release download.
pub const FREEHACK_PDF_SHA256: &str =
    "ca5c0420ec05884c1b0182f2a5857a4991e23573e10b9e28ecb009157e8ad393";
/// SHA-256 of `src/freehack.md` at [`FREEHACK_SOURCE_REVISION`].
pub const FREEHACK_MARKDOWN_SHA256: &str =
    "cfb7910f9dfe73335bb575df107712735a7c806a4a392599958f8f4298be2c64";
/// SHA-256 of `src/freehack.yml` at [`FREEHACK_SOURCE_REVISION`].
pub const FREEHACK_METADATA_SHA256: &str =
    "a4b2d8bbe45b4d2f96e8ac161999d35fec5f27d4908f004bbfed871e1f1655f0";
/// SHA-256 of `tools/roll.py` at [`FREEHACK_SOURCE_REVISION`].
pub const FREEHACK_ROLL_TOOL_SHA256: &str =
    "838ee10829d00fce8a66416f85401de48fc3927fa9a0b3536137e6007b9aed19";
/// SHA-256 of the official plain-text CC0 1.0 legal code retained with fixtures.
pub const FREEHACK_CC0_1_0_LEGAL_CODE_SHA256: &str =
    "a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499";

const MAX_MODIFIER_TOTAL: u32 = 1_000_000;
const MAX_CAMPAIGN_ENTRIES: usize = 256;
const MAX_SECTION_PARTICIPANTS: usize = 64;

/// Optional rarity label used only when a campaign configures archetype offers.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FreehackRarity {
    Common,
    Uncommon,
    Rare,
}

/// Campaign-selected archetype offering procedure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackArchetypeProcedure {
    Disabled,
    OpenChoice,
    RarityOffer,
}

/// One setting-authored archetype option; the adapter assigns it no genre semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackArchetypeOption {
    pub id: String,
    pub label: String,
    pub description: String,
    pub rarity: FreehackRarity,
}

/// How a campaign obtains one numeric character modifier during creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum FreehackModifierGeneration {
    Authored,
    Fixed { value: i32 },
    RandomInclusive,
}

/// One arbitrary, campaign-named numeric modifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackModifierSpec {
    pub id: String,
    pub label: String,
    pub description: String,
    pub minimum: i32,
    pub maximum: i32,
    pub generation: FreehackModifierGeneration,
    pub player_visible: bool,
}

/// One campaign-authored feature available during creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackFeatureOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// One campaign-authored inventory option with an optional point cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackInventoryOption {
    pub id: String,
    pub label: String,
    pub description: String,
    pub cost: Option<u32>,
}

/// Which check outcome advances a generic track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackTrackOutcome {
    Failure,
    Success,
}

/// One initial generic interval/target/consequence track configured by a campaign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackTrackTemplate {
    pub id: String,
    pub label: String,
    pub secret: bool,
    pub interval: String,
    pub target_count: u32,
    pub advances_on: FreehackTrackOutcome,
    pub consequence: String,
}

/// Complete setting-neutral creation configuration interpreted without adapter-code changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackCampaignSchema {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub archetype_procedure: FreehackArchetypeProcedure,
    pub archetypes: Vec<FreehackArchetypeOption>,
    pub modifiers: Vec<FreehackModifierSpec>,
    pub feature_count: u16,
    pub feature_options: Vec<FreehackFeatureOption>,
    pub allow_authored_features: bool,
    pub inventory_budget: Option<u32>,
    pub inventory_options: Vec<FreehackInventoryOption>,
    pub allow_authored_inventory: bool,
    pub initial_tracks: Vec<FreehackTrackTemplate>,
}

/// One selected catalog feature or a campaign-permitted authored feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum FreehackFeatureSelection {
    Catalog {
        id: String,
    },
    Authored {
        id: String,
        label: String,
        description: String,
    },
}

/// One selected catalog item or a campaign-permitted authored item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum FreehackInventorySelection {
    Catalog {
        id: String,
        quantity: u16,
    },
    Authored {
        id: String,
        label: String,
        quantity: u16,
        cost_per_item: u32,
    },
}

/// Visibility of a memory, check, track event, or section detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackDisclosure {
    HostOnly,
    Public,
}

/// One explicitly authored memory present before play begins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackMemorySeed {
    pub id: String,
    pub topic: String,
    pub has_experience: bool,
    pub rationale: String,
    pub disclosure: FreehackDisclosure,
}

/// Complete, versioned input to deterministic Freehack character creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackCreationRequest {
    pub creation_format_version: u32,
    pub character_id: String,
    pub name: String,
    pub seed: u64,
    pub campaign: FreehackCampaignSchema,
    pub selected_archetype: Option<String>,
    pub authored_modifiers: BTreeMap<String, i32>,
    pub features: Vec<FreehackFeatureSelection>,
    pub inventory: Vec<FreehackInventorySelection>,
    pub memories: Vec<FreehackMemorySeed>,
}

/// Trace for one campaign-configured generated modifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackGeneratedModifier {
    pub id: String,
    pub value: i32,
    pub source: String,
    pub entropy_start: u64,
    pub entropy_end: u64,
}

/// Explainable result of creation before projection/state export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackCreationPreview {
    pub creation_format_version: u32,
    pub request_sha256: String,
    pub adapter: ResolvedAdapter,
    pub campaign: FreehackCampaignSchema,
    pub campaign_sha256: String,
    pub offered_archetypes: Vec<String>,
    pub selected_archetype: Option<String>,
    pub modifiers: BTreeMap<String, i32>,
    pub generated_modifiers: Vec<FreehackGeneratedModifier>,
    pub feature_ids: Vec<String>,
    pub inventory_ids: Vec<String>,
    pub inventory_spent: u32,
    pub definition: AdapterCharacterDefinition,
    pub initial_state: TabletopState,
    pub explanations: Vec<String>,
}

/// Source category for a modifier contributing to one check side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackContributionSource {
    Character,
    Equipment,
    Situation,
    Other,
}

/// One non-negative contribution assigned to support or opposition by the authority host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackContribution {
    pub id: String,
    pub label: String,
    pub value: u32,
    pub source: FreehackContributionSource,
}

/// Exact input to a non-mutating probability preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackProbabilityRequest {
    pub probability_format_version: u32,
    pub support: Vec<FreehackContribution>,
    pub opposition: Vec<FreehackContribution>,
}

/// Rational probability weights plus a deterministic, floored basis-point presentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackProbabilityPreview {
    pub probability_format_version: u32,
    pub support_total: u32,
    pub opposition_total: u32,
    pub favorable_weight: u64,
    pub unfavorable_weight: u64,
    pub total_weight: u64,
    pub success_basis_points_floor: u32,
}

macro_rules! impl_freehack_io {
    ($type:ty) => {
        impl $type {
            /// Parse strict JSON and reject duplicate object keys.
            pub fn from_json(source: &str) -> Result<Self, TabletopError> {
                weave_domain::parse_strict_json(source).map_err(|_| TabletopError::Artifact)
            }

            /// Parse one RON artifact with unknown-field rejection.
            pub fn from_ron(source: &str) -> Result<Self, TabletopError> {
                ron::from_str(source).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize stable, pretty JSON with a trailing newline.
            pub fn to_json(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_json(self).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize stable, pretty RON with a trailing newline.
            pub fn to_ron(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_ron(self).map_err(|_| TabletopError::Artifact)
            }
        }
    };
}

impl_freehack_io!(FreehackCreationRequest);
impl_freehack_io!(FreehackCreationPreview);
impl_freehack_io!(FreehackProbabilityRequest);
impl_freehack_io!(FreehackProbabilityPreview);

/// Canonical JSON Schema for configurable Freehack creation requests.
pub fn freehack_creation_request_schema() -> Result<String, TabletopError> {
    freehack_schema::<FreehackCreationRequest>(
        "urn:weave:schema:tabletop-freehack-creation-request:1",
        "Weave Freehack Creation Request v1",
        Some(("creation_format_version", FREEHACK_CREATION_FORMAT_VERSION)),
    )
}

/// Canonical JSON Schema for explainable Freehack creation previews.
pub fn freehack_creation_preview_schema() -> Result<String, TabletopError> {
    freehack_schema::<FreehackCreationPreview>(
        "urn:weave:schema:tabletop-freehack-creation-preview:1",
        "Weave Freehack Creation Preview v1",
        Some(("creation_format_version", FREEHACK_CREATION_FORMAT_VERSION)),
    )
}

/// Canonical JSON Schema for exact Freehack probability requests.
pub fn freehack_probability_request_schema() -> Result<String, TabletopError> {
    freehack_schema::<FreehackProbabilityRequest>(
        "urn:weave:schema:tabletop-freehack-probability-request:1",
        "Weave Freehack Probability Request v1",
        Some((
            "probability_format_version",
            FREEHACK_PROBABILITY_FORMAT_VERSION,
        )),
    )
}

/// Canonical JSON Schema for exact Freehack probability previews.
pub fn freehack_probability_preview_schema() -> Result<String, TabletopError> {
    freehack_schema::<FreehackProbabilityPreview>(
        "urn:weave:schema:tabletop-freehack-probability-preview:1",
        "Weave Freehack Probability Preview v1",
        Some((
            "probability_format_version",
            FREEHACK_PROBABILITY_FORMAT_VERSION,
        )),
    )
}

fn freehack_schema<T: JsonSchema>(
    id: &str,
    title: &str,
    version: Option<(&str, u32)>,
) -> Result<String, TabletopError> {
    let schema = schema_for!(T);
    let mut value = serde_json::to_value(schema).map_err(|_| TabletopError::Artifact)?;
    if let Some(root) = value.as_object_mut() {
        root.insert("$id".to_owned(), serde_json::Value::String(id.to_owned()));
        root.insert(
            "title".to_owned(),
            serde_json::Value::String(title.to_owned()),
        );
        root.insert(
            "x-weave-tabletop-contract-version".to_owned(),
            serde_json::Value::from(1),
        );
        if let Some((property_name, version)) = version
            && let Some(property) = root
                .get_mut("properties")
                .and_then(serde_json::Value::as_object_mut)
                .and_then(|properties| properties.get_mut(property_name))
                .and_then(serde_json::Value::as_object_mut)
        {
            property.insert("const".to_owned(), serde_json::Value::from(version));
        }
    }
    sort_json_keys(&mut value);
    weave_domain::to_pretty_json(&value).map_err(|_| TabletopError::Artifact)
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

/// Compute the exact support-versus-opposition probability without floating-point arithmetic.
pub fn freehack_probability_preview(
    request: &FreehackProbabilityRequest,
) -> Result<FreehackProbabilityPreview, TabletopError> {
    if request.probability_format_version != FREEHACK_PROBABILITY_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "probability_format_version",
        });
    }
    let support_total = contribution_total("support", &request.support)?;
    let opposition_total = contribution_total("opposition", &request.opposition)?;
    probability_from_totals(support_total, opposition_total)
}

/// Validate a standalone probability preview against the exact integer formula.
pub fn validate_freehack_probability_preview(
    preview: &FreehackProbabilityPreview,
) -> Result<(), TabletopError> {
    if preview.probability_format_version != FREEHACK_PROBABILITY_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "probability_format_version",
        });
    }
    let expected = probability_from_totals(preview.support_total, preview.opposition_total)?;
    if &expected != preview {
        return Err(invalid(
            "probability_preview",
            "probability weights do not match the declared totals",
        ));
    }
    Ok(())
}

fn contribution_total(
    path: &str,
    contributions: &[FreehackContribution],
) -> Result<u32, TabletopError> {
    if contributions.is_empty() || contributions.len() > MAX_CAMPAIGN_ENTRIES {
        return Err(invalid(path, "expected one to 256 modifier contributions"));
    }
    let mut ids = BTreeSet::new();
    let mut total = 0_u32;
    for (index, contribution) in contributions.iter().enumerate() {
        validate_local_identifier(&format!("{path}[{index}].id"), &contribution.id)?;
        validate_bounded_text(
            format!("{path}[{index}].label"),
            &contribution.label,
            1,
            160,
        )?;
        if contribution.value == 0 || contribution.value > MAX_MODIFIER_TOTAL {
            return Err(invalid(
                format!("{path}[{index}].value"),
                "expected a positive modifier no greater than 1000000",
            ));
        }
        if !ids.insert(&contribution.id) {
            return Err(invalid(path, "duplicate contribution identity"));
        }
        total = total
            .checked_add(contribution.value)
            .ok_or_else(|| invalid(path, "modifier sum overflow"))?;
        if total > MAX_MODIFIER_TOTAL {
            return Err(invalid(
                path,
                "modifier total exceeds the reviewed exact-arithmetic domain",
            ));
        }
    }
    Ok(total)
}

fn probability_from_totals(
    support_total: u32,
    opposition_total: u32,
) -> Result<FreehackProbabilityPreview, TabletopError> {
    if support_total == 0 || opposition_total == 0 {
        return Err(invalid(
            "probability",
            "uncertain checks require positive support and opposition",
        ));
    }
    if support_total > MAX_MODIFIER_TOTAL || opposition_total > MAX_MODIFIER_TOTAL {
        return Err(invalid(
            "probability",
            "modifier total exceeds the reviewed exact-arithmetic domain",
        ));
    }
    let favorable_weight = u64::from(support_total)
        .checked_mul(u64::from(support_total))
        .ok_or_else(|| invalid("probability.support", "support square overflow"))?;
    let unfavorable_weight = u64::from(opposition_total)
        .checked_mul(u64::from(opposition_total))
        .ok_or_else(|| invalid("probability.opposition", "opposition square overflow"))?;
    let total_weight = favorable_weight
        .checked_add(unfavorable_weight)
        .ok_or_else(|| invalid("probability", "probability denominator overflow"))?;
    let scaled = favorable_weight
        .checked_mul(10_000)
        .ok_or_else(|| invalid("probability", "probability presentation overflow"))?;
    let success_basis_points_floor = u32::try_from(scaled / total_weight)
        .map_err(|_| invalid("probability", "probability presentation is out of range"))?;
    Ok(FreehackProbabilityPreview {
        probability_format_version: FREEHACK_PROBABILITY_FORMAT_VERSION,
        support_total,
        opposition_total,
        favorable_weight,
        unfavorable_weight,
        total_weight,
        success_basis_points_floor,
    })
}

/// Validate and deterministically create one campaign-configured Freehack character.
pub fn create_freehack_character(
    request: &FreehackCreationRequest,
) -> Result<FreehackCreationPreview, TabletopError> {
    validate_creation_request(request)?;
    let manifest = freehack_manifest();
    crate::validate_adapter_manifest(&manifest)?;
    let adapter = resolved_adapter(&manifest)?;
    let campaign_sha256 = canonical_fingerprint(&request.campaign)?;
    let mut entropy = EntropyStream::from_state(EntropyState {
        algorithm: "sha256_counter_v1".to_owned(),
        seed: request.seed,
        cursor: 0,
    })?;

    let offered_archetypes = archetype_offer(&request.campaign, &mut entropy)?;
    validate_archetype_selection(request, &offered_archetypes)?;

    let mut modifiers = BTreeMap::new();
    let mut generated_modifiers = Vec::new();
    for spec in &request.campaign.modifiers {
        let entropy_start = entropy.state().cursor;
        let (value, source) = match spec.generation {
            FreehackModifierGeneration::Authored => (
                *request.authored_modifiers.get(&spec.id).ok_or_else(|| {
                    invalid(
                        format!("authored_modifiers.{}", spec.id),
                        "required campaign modifier is missing",
                    )
                })?,
                "authored",
            ),
            FreehackModifierGeneration::Fixed { value } => (value, "fixed"),
            FreehackModifierGeneration::RandomInclusive => {
                let minimum = i64::from(spec.minimum);
                let span = i64::from(spec.maximum)
                    .checked_sub(minimum)
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(|| invalid("campaign.modifiers", "modifier range overflow"))?;
                let span = u64::try_from(span)
                    .map_err(|_| invalid("campaign.modifiers", "invalid modifier range"))?;
                let draw = entropy.draw_bounded(span)?;
                let value = minimum
                    .checked_add(i64::try_from(draw).map_err(|_| {
                        invalid("campaign.modifiers", "generated modifier overflow")
                    })?)
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(|| invalid("campaign.modifiers", "generated modifier overflow"))?;
                (value, "random_inclusive")
            }
        };
        if value < spec.minimum || value > spec.maximum {
            return Err(invalid(
                format!("modifiers.{}", spec.id),
                "modifier is outside its campaign-declared range",
            ));
        }
        modifiers.insert(spec.id.clone(), value);
        generated_modifiers.push(FreehackGeneratedModifier {
            id: spec.id.clone(),
            value,
            source: source.to_owned(),
            entropy_start,
            entropy_end: entropy.state().cursor,
        });
    }

    let (feature_ids, feature_values) = selected_features(request)?;
    let (inventory_ids, inventory_values, inventory_spent) = selected_inventory(request)?;
    let definition_value = creation_definition_value(
        request,
        &campaign_sha256,
        &modifiers,
        feature_values,
        inventory_values,
    );
    let definition_sha256 = canonical_fingerprint(&definition_value)?;
    let definition = AdapterCharacterDefinition {
        adapter: adapter.clone(),
        definition: definition_value,
        definition_sha256: definition_sha256.clone(),
        completed_creation_steps: vec![
            "campaign".to_owned(),
            "features_and_inventory".to_owned(),
            "identity_and_modifiers".to_owned(),
        ],
    };
    let initial_state = TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter: adapter.clone(),
        owner_id: request.character_id.clone(),
        definition_sha256,
        revision: 0,
        entropy: entropy.state(),
        value: initial_state_value(request),
    };
    validate_tabletop_state(&initial_state, &manifest)?;

    let explanations = vec![
        "The campaign schema, not adapter code, defines every character-facing numeric field."
            .to_owned(),
        format!(
            "Creation consumed {} deterministic entropy draw(s); play resumes at that cursor.",
            initial_state.entropy.cursor
        ),
        "Tabletop data remains adapter-owned and cannot write back into canonical Character evidence."
            .to_owned(),
    ];
    let preview = FreehackCreationPreview {
        creation_format_version: FREEHACK_CREATION_FORMAT_VERSION,
        request_sha256: canonical_fingerprint(request)?,
        adapter,
        campaign: request.campaign.clone(),
        campaign_sha256,
        offered_archetypes,
        selected_archetype: request.selected_archetype.clone(),
        modifiers,
        generated_modifiers,
        feature_ids,
        inventory_ids,
        inventory_spent,
        definition,
        initial_state,
        explanations,
    };
    validate_freehack_creation_preview(&preview)?;
    Ok(preview)
}

/// Revalidate a creation preview and all of its cross-artifact fingerprints.
pub fn validate_freehack_creation_preview(
    preview: &FreehackCreationPreview,
) -> Result<(), TabletopError> {
    if preview.creation_format_version != FREEHACK_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_campaign_schema(&preview.campaign)?;
    if canonical_fingerprint(&preview.campaign)? != preview.campaign_sha256 {
        return Err(TabletopError::ContentHashMismatch {
            path: "campaign_sha256",
        });
    }
    validate_sha256("request_sha256", &preview.request_sha256)?;
    let manifest = freehack_manifest();
    let adapter = resolved_adapter(&manifest)?;
    if preview.adapter != adapter || preview.definition.adapter != adapter {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    if preview.definition.definition_sha256
        != canonical_fingerprint(&preview.definition.definition)?
    {
        return Err(TabletopError::ContentHashMismatch {
            path: "definition_sha256",
        });
    }
    validate_typed_value(
        "definition",
        &preview.definition.definition,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    if preview.initial_state.definition_sha256 != preview.definition.definition_sha256 {
        return Err(TabletopError::ContentHashMismatch {
            path: "initial_state.definition_sha256",
        });
    }
    validate_tabletop_state(&preview.initial_state, &manifest)?;
    if preview.inventory_spent > preview.campaign.inventory_budget.unwrap_or(u32::MAX) {
        return Err(invalid(
            "inventory_spent",
            "inventory exceeds the configured budget",
        ));
    }
    if preview.modifiers.len() != preview.campaign.modifiers.len()
        || preview.generated_modifiers.len() != preview.campaign.modifiers.len()
    {
        return Err(invalid(
            "modifiers",
            "creation preview does not cover every campaign modifier exactly",
        ));
    }
    Ok(())
}

fn validate_creation_request(request: &FreehackCreationRequest) -> Result<(), TabletopError> {
    if request.creation_format_version != FREEHACK_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_global_identifier("character_id", &request.character_id)?;
    validate_bounded_text("name", &request.name, 1, 160)?;
    validate_campaign_schema(&request.campaign)?;
    if request.authored_modifiers.len() > request.campaign.modifiers.len() {
        return Err(invalid(
            "authored_modifiers",
            "contains undeclared modifier values",
        ));
    }
    let specs = request
        .campaign
        .modifiers
        .iter()
        .map(|spec| (spec.id.as_str(), spec))
        .collect::<BTreeMap<_, _>>();
    for (id, value) in &request.authored_modifiers {
        let Some(spec) = specs.get(id.as_str()) else {
            return Err(invalid(
                format!("authored_modifiers.{id}"),
                "modifier is not declared by the campaign",
            ));
        };
        if !matches!(spec.generation, FreehackModifierGeneration::Authored) {
            return Err(invalid(
                format!("authored_modifiers.{id}"),
                "only authored modifiers accept request values",
            ));
        }
        if *value < spec.minimum || *value > spec.maximum {
            return Err(invalid(
                format!("authored_modifiers.{id}"),
                "modifier is outside its campaign-declared range",
            ));
        }
    }
    let authored_required = request
        .campaign
        .modifiers
        .iter()
        .filter(|spec| matches!(spec.generation, FreehackModifierGeneration::Authored))
        .count();
    if request.authored_modifiers.len() != authored_required {
        return Err(invalid(
            "authored_modifiers",
            "every authored campaign modifier must be supplied exactly once",
        ));
    }
    validate_memory_seeds(&request.memories)
}

fn validate_campaign_schema(campaign: &FreehackCampaignSchema) -> Result<(), TabletopError> {
    if campaign.schema_version != FREEHACK_CAMPAIGN_SCHEMA_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "campaign.schema_version",
        });
    }
    validate_global_identifier("campaign.id", &campaign.id)?;
    validate_bounded_text("campaign.title", &campaign.title, 1, 160)?;
    for (path, len) in [
        ("campaign.archetypes", campaign.archetypes.len()),
        ("campaign.modifiers", campaign.modifiers.len()),
        ("campaign.feature_options", campaign.feature_options.len()),
        (
            "campaign.inventory_options",
            campaign.inventory_options.len(),
        ),
        ("campaign.initial_tracks", campaign.initial_tracks.len()),
    ] {
        if len > MAX_CAMPAIGN_ENTRIES {
            return Err(invalid(path, "campaign collection exceeds 256 entries"));
        }
    }
    match campaign.archetype_procedure {
        FreehackArchetypeProcedure::Disabled if !campaign.archetypes.is_empty() => {
            return Err(invalid(
                "campaign.archetypes",
                "disabled archetype creation requires an empty catalog",
            ));
        }
        FreehackArchetypeProcedure::Disabled => {}
        _ if campaign.archetypes.is_empty() => {
            return Err(invalid(
                "campaign.archetypes",
                "configured archetype creation requires options",
            ));
        }
        _ => {}
    }
    let mut ids = BTreeSet::new();
    for (index, option) in campaign.archetypes.iter().enumerate() {
        validate_local_identifier(&format!("campaign.archetypes[{index}].id"), &option.id)?;
        validate_bounded_text(
            format!("campaign.archetypes[{index}].label"),
            &option.label,
            1,
            160,
        )?;
        validate_bounded_text(
            format!("campaign.archetypes[{index}].description"),
            &option.description,
            1,
            512,
        )?;
        if !ids.insert(&option.id) {
            return Err(invalid(
                "campaign.archetypes",
                "duplicate archetype identity",
            ));
        }
    }
    ids.clear();
    for (index, spec) in campaign.modifiers.iter().enumerate() {
        validate_local_identifier(&format!("campaign.modifiers[{index}].id"), &spec.id)?;
        validate_bounded_text(
            format!("campaign.modifiers[{index}].label"),
            &spec.label,
            1,
            160,
        )?;
        validate_bounded_text(
            format!("campaign.modifiers[{index}].description"),
            &spec.description,
            1,
            512,
        )?;
        if spec.minimum < -1_000_000 || spec.maximum > 1_000_000 || spec.minimum > spec.maximum {
            return Err(invalid(
                format!("campaign.modifiers[{index}]"),
                "modifier range must be ordered within -1000000..1000000",
            ));
        }
        if let FreehackModifierGeneration::Fixed { value } = spec.generation
            && (value < spec.minimum || value > spec.maximum)
        {
            return Err(invalid(
                format!("campaign.modifiers[{index}].generation"),
                "fixed value is outside the declared range",
            ));
        }
        if !ids.insert(&spec.id) {
            return Err(invalid("campaign.modifiers", "duplicate modifier identity"));
        }
    }
    if usize::from(campaign.feature_count) > MAX_CAMPAIGN_ENTRIES {
        return Err(invalid(
            "campaign.feature_count",
            "feature count exceeds 256",
        ));
    }
    ids.clear();
    for (index, option) in campaign.feature_options.iter().enumerate() {
        validate_local_identifier(&format!("campaign.feature_options[{index}].id"), &option.id)?;
        validate_bounded_text(
            format!("campaign.feature_options[{index}].label"),
            &option.label,
            1,
            160,
        )?;
        validate_bounded_text(
            format!("campaign.feature_options[{index}].description"),
            &option.description,
            1,
            512,
        )?;
        if !ids.insert(&option.id) {
            return Err(invalid(
                "campaign.feature_options",
                "duplicate feature identity",
            ));
        }
    }
    if !campaign.allow_authored_features
        && usize::from(campaign.feature_count) > campaign.feature_options.len()
    {
        return Err(invalid(
            "campaign.feature_count",
            "feature count exceeds the fixed catalog",
        ));
    }
    ids.clear();
    for (index, option) in campaign.inventory_options.iter().enumerate() {
        validate_local_identifier(
            &format!("campaign.inventory_options[{index}].id"),
            &option.id,
        )?;
        validate_bounded_text(
            format!("campaign.inventory_options[{index}].label"),
            &option.label,
            1,
            160,
        )?;
        validate_bounded_text(
            format!("campaign.inventory_options[{index}].description"),
            &option.description,
            1,
            512,
        )?;
        if !ids.insert(&option.id) {
            return Err(invalid(
                "campaign.inventory_options",
                "duplicate inventory identity",
            ));
        }
    }
    ids.clear();
    for (index, track) in campaign.initial_tracks.iter().enumerate() {
        validate_track_template(&format!("campaign.initial_tracks[{index}]"), track)?;
        if !ids.insert(&track.id) {
            return Err(invalid(
                "campaign.initial_tracks",
                "duplicate track identity",
            ));
        }
    }
    Ok(())
}

fn validate_memory_seeds(memories: &[FreehackMemorySeed]) -> Result<(), TabletopError> {
    if memories.len() > MAX_CAMPAIGN_ENTRIES {
        return Err(invalid("memories", "too many initial memories"));
    }
    let mut ids = BTreeSet::new();
    for (index, memory) in memories.iter().enumerate() {
        validate_local_identifier(&format!("memories[{index}].id"), &memory.id)?;
        validate_bounded_text(format!("memories[{index}].topic"), &memory.topic, 1, 240)?;
        validate_bounded_text(
            format!("memories[{index}].rationale"),
            &memory.rationale,
            1,
            512,
        )?;
        if !ids.insert(&memory.id) {
            return Err(invalid("memories", "duplicate memory identity"));
        }
    }
    Ok(())
}

fn validate_track_template(path: &str, track: &FreehackTrackTemplate) -> Result<(), TabletopError> {
    validate_local_identifier(&format!("{path}.id"), &track.id)?;
    validate_bounded_text(format!("{path}.label"), &track.label, 1, 160)?;
    validate_bounded_text(format!("{path}.interval"), &track.interval, 1, 240)?;
    validate_bounded_text(format!("{path}.consequence"), &track.consequence, 1, 512)?;
    if track.target_count == 0 || track.target_count > 1_000_000 {
        return Err(invalid(
            format!("{path}.target_count"),
            "target count must be within 1..1000000",
        ));
    }
    Ok(())
}

fn archetype_offer(
    campaign: &FreehackCampaignSchema,
    entropy: &mut EntropyStream,
) -> Result<Vec<String>, TabletopError> {
    match campaign.archetype_procedure {
        FreehackArchetypeProcedure::Disabled => Ok(Vec::new()),
        FreehackArchetypeProcedure::OpenChoice => Ok(campaign
            .archetypes
            .iter()
            .map(|item| item.id.clone())
            .collect()),
        FreehackArchetypeProcedure::RarityOffer => {
            let common = campaign
                .archetypes
                .iter()
                .filter(|item| item.rarity == FreehackRarity::Common)
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            let uncommon = campaign
                .archetypes
                .iter()
                .filter(|item| item.rarity == FreehackRarity::Uncommon)
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            let rare = campaign
                .archetypes
                .iter()
                .filter(|item| item.rarity == FreehackRarity::Rare)
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            let uncommon_count = uncommon.len().div_ceil(3).max(2).min(uncommon.len());
            let rare_count = rare.len().div_ceil(10).max(1).min(rare.len());
            let mut offered = common;
            offered.extend(sample_without_replacement(
                uncommon,
                uncommon_count,
                entropy,
            )?);
            offered.extend(sample_without_replacement(rare, rare_count, entropy)?);
            Ok(offered)
        }
    }
}

fn sample_without_replacement(
    mut values: Vec<String>,
    count: usize,
    entropy: &mut EntropyStream,
) -> Result<Vec<String>, TabletopError> {
    for index in 0..count {
        let remaining = values.len() - index;
        let offset = usize::try_from(entropy.draw_bounded(remaining as u64)?)
            .map_err(|_| invalid("campaign.archetypes", "sample index overflow"))?;
        values.swap(index, index + offset);
    }
    values.truncate(count);
    values.sort();
    Ok(values)
}

fn validate_archetype_selection(
    request: &FreehackCreationRequest,
    offered: &[String],
) -> Result<(), TabletopError> {
    match request.campaign.archetype_procedure {
        FreehackArchetypeProcedure::Disabled if request.selected_archetype.is_some() => Err(
            invalid("selected_archetype", "campaign disables archetypes"),
        ),
        FreehackArchetypeProcedure::Disabled => Ok(()),
        _ => {
            let selected = request
                .selected_archetype
                .as_ref()
                .ok_or_else(|| invalid("selected_archetype", "campaign requires an archetype"))?;
            if !offered.contains(selected) {
                return Err(invalid(
                    "selected_archetype",
                    "archetype was not offered by this deterministic creation pass",
                ));
            }
            Ok(())
        }
    }
}

fn selected_features(
    request: &FreehackCreationRequest,
) -> Result<(Vec<String>, BTreeMap<String, DomainValue>), TabletopError> {
    if request.features.len() != usize::from(request.campaign.feature_count) {
        return Err(invalid(
            "features",
            "selection count must equal the campaign feature count",
        ));
    }
    let catalog = request
        .campaign
        .feature_options
        .iter()
        .map(|option| (option.id.as_str(), option))
        .collect::<BTreeMap<_, _>>();
    let mut ids = Vec::with_capacity(request.features.len());
    let mut values = BTreeMap::new();
    for (index, selection) in request.features.iter().enumerate() {
        let (id, label, description, source) = match selection {
            FreehackFeatureSelection::Catalog { id } => {
                let option = catalog.get(id.as_str()).ok_or_else(|| {
                    invalid(
                        format!("features[{index}].id"),
                        "feature is not in the campaign catalog",
                    )
                })?;
                (
                    id.clone(),
                    option.label.clone(),
                    option.description.clone(),
                    "catalog",
                )
            }
            FreehackFeatureSelection::Authored {
                id,
                label,
                description,
            } => {
                if !request.campaign.allow_authored_features {
                    return Err(invalid(
                        format!("features[{index}]"),
                        "campaign does not permit authored features",
                    ));
                }
                validate_local_identifier(&format!("features[{index}].id"), id)?;
                validate_bounded_text(format!("features[{index}].label"), label, 1, 160)?;
                validate_bounded_text(
                    format!("features[{index}].description"),
                    description,
                    1,
                    512,
                )?;
                (id.clone(), label.clone(), description.clone(), "authored")
            }
        };
        if values.contains_key(&id) {
            return Err(invalid("features", "duplicate feature identity"));
        }
        ids.push(id.clone());
        values.insert(
            id,
            object([
                ("description", DomainValue::String(description)),
                ("label", DomainValue::String(label)),
                ("source", DomainValue::Symbol(source.to_owned())),
            ]),
        );
    }
    ids.sort();
    Ok((ids, values))
}

type SelectedInventory = (Vec<String>, BTreeMap<String, DomainValue>, u32);

fn selected_inventory(
    request: &FreehackCreationRequest,
) -> Result<SelectedInventory, TabletopError> {
    if request.inventory.len() > MAX_CAMPAIGN_ENTRIES {
        return Err(invalid("inventory", "too many inventory selections"));
    }
    let catalog = request
        .campaign
        .inventory_options
        .iter()
        .map(|option| (option.id.as_str(), option))
        .collect::<BTreeMap<_, _>>();
    let mut ids = Vec::with_capacity(request.inventory.len());
    let mut values = BTreeMap::new();
    let mut spent = 0_u32;
    for (index, selection) in request.inventory.iter().enumerate() {
        let (id, label, quantity, cost_per_item, source) = match selection {
            FreehackInventorySelection::Catalog { id, quantity } => {
                let option = catalog.get(id.as_str()).ok_or_else(|| {
                    invalid(
                        format!("inventory[{index}].id"),
                        "item is not in the campaign catalog",
                    )
                })?;
                (
                    id.clone(),
                    option.label.clone(),
                    *quantity,
                    option.cost.unwrap_or(0),
                    "catalog",
                )
            }
            FreehackInventorySelection::Authored {
                id,
                label,
                quantity,
                cost_per_item,
            } => {
                if !request.campaign.allow_authored_inventory {
                    return Err(invalid(
                        format!("inventory[{index}]"),
                        "campaign does not permit authored inventory",
                    ));
                }
                validate_local_identifier(&format!("inventory[{index}].id"), id)?;
                validate_bounded_text(format!("inventory[{index}].label"), label, 1, 160)?;
                (
                    id.clone(),
                    label.clone(),
                    *quantity,
                    *cost_per_item,
                    "authored",
                )
            }
        };
        if quantity == 0 || quantity > 1_000 {
            return Err(invalid(
                format!("inventory[{index}].quantity"),
                "quantity must be within 1..1000",
            ));
        }
        if values.contains_key(&id) {
            return Err(invalid("inventory", "duplicate inventory identity"));
        }
        let line_cost = cost_per_item
            .checked_mul(u32::from(quantity))
            .ok_or_else(|| invalid("inventory", "inventory cost overflow"))?;
        spent = spent
            .checked_add(line_cost)
            .ok_or_else(|| invalid("inventory", "inventory budget overflow"))?;
        ids.push(id.clone());
        values.insert(
            id,
            object([
                (
                    "cost_per_item",
                    DomainValue::Number(f64::from(cost_per_item)),
                ),
                ("label", DomainValue::String(label)),
                ("quantity", DomainValue::Number(f64::from(quantity))),
                ("source", DomainValue::Symbol(source.to_owned())),
            ]),
        );
    }
    if spent > request.campaign.inventory_budget.unwrap_or(u32::MAX) {
        return Err(invalid(
            "inventory",
            "inventory cost exceeds the campaign budget",
        ));
    }
    ids.sort();
    Ok((ids, values, spent))
}

fn creation_definition_value(
    request: &FreehackCreationRequest,
    campaign_sha256: &str,
    modifiers: &BTreeMap<String, i32>,
    features: BTreeMap<String, DomainValue>,
    inventory: BTreeMap<String, DomainValue>,
) -> DomainValue {
    let modifier_specs = request
        .campaign
        .modifiers
        .iter()
        .map(|spec| (spec.id.as_str(), spec))
        .collect::<BTreeMap<_, _>>();
    let modifier_values = modifiers
        .iter()
        .map(|(id, value)| {
            let spec = modifier_specs[id.as_str()];
            (
                id.clone(),
                object([
                    ("label", DomainValue::String(spec.label.clone())),
                    ("player_visible", DomainValue::Bool(spec.player_visible)),
                    ("value", DomainValue::Number(f64::from(*value))),
                ]),
            )
        })
        .collect();
    object([
        (
            "archetype_id",
            request.selected_archetype.as_ref().map_or_else(
                || DomainValue::String(String::new()),
                |value| DomainValue::String(value.clone()),
            ),
        ),
        (
            "campaign_id",
            DomainValue::String(request.campaign.id.clone()),
        ),
        (
            "campaign_schema_sha256",
            DomainValue::String(campaign_sha256.to_owned()),
        ),
        ("features", DomainValue::Object(features)),
        ("inventory", DomainValue::Object(inventory)),
        ("modifiers", DomainValue::Object(modifier_values)),
        ("name", DomainValue::String(request.name.clone())),
    ])
}

fn initial_state_value(request: &FreehackCreationRequest) -> DomainValue {
    let tracks = request
        .campaign
        .initial_tracks
        .iter()
        .map(|track| (track.id.clone(), track_state_value(track)))
        .collect();
    let memories = request
        .memories
        .iter()
        .map(|memory| {
            (
                memory.id.clone(),
                object([
                    ("connected", DomainValue::Bool(false)),
                    (
                        "disclosure",
                        DomainValue::Symbol(disclosure_symbol(memory.disclosure).to_owned()),
                    ),
                    ("draw", DomainValue::Number(0.0)),
                    ("effective_obscurity", DomainValue::Number(0.0)),
                    ("has_experience", DomainValue::Bool(memory.has_experience)),
                    ("obscurity", DomainValue::Number(0.0)),
                    ("rationale", DomainValue::String(memory.rationale.clone())),
                    ("topic", DomainValue::String(memory.topic.clone())),
                ]),
            )
        })
        .collect();
    object([
        ("memories", DomainValue::Object(memories)),
        ("sections", DomainValue::Object(BTreeMap::new())),
        ("tracks", DomainValue::Object(tracks)),
    ])
}

fn track_state_value(track: &FreehackTrackTemplate) -> DomainValue {
    object([
        (
            "advances_on",
            DomainValue::Symbol(track_outcome_symbol(track.advances_on).to_owned()),
        ),
        ("completed", DomainValue::Bool(false)),
        (
            "consequence",
            DomainValue::String(track.consequence.clone()),
        ),
        ("interval", DomainValue::String(track.interval.clone())),
        ("label", DomainValue::String(track.label.clone())),
        ("progress", DomainValue::Number(0.0)),
        ("secret", DomainValue::Bool(track.secret)),
        (
            "target_count",
            DomainValue::Number(f64::from(track.target_count)),
        ),
    ])
}

const fn disclosure_symbol(disclosure: FreehackDisclosure) -> &'static str {
    match disclosure {
        FreehackDisclosure::HostOnly => "host_only",
        FreehackDisclosure::Public => "public",
    }
}

const fn track_outcome_symbol(outcome: FreehackTrackOutcome) -> &'static str {
    match outcome {
        FreehackTrackOutcome::Failure => "failure",
        FreehackTrackOutcome::Success => "success",
    }
}

/// Trusted resolver for the exact [`freehack_manifest`] coordinate.
#[derive(Debug, Default, Clone, Copy)]
pub struct FreehackResolver;

impl TabletopResolver for FreehackResolver {
    fn adapter_id(&self) -> &str {
        FREEHACK_ADAPTER_ID
    }

    fn adapter_version(&self) -> &str {
        FREEHACK_ADAPTER_VERSION
    }

    fn resolve(
        &self,
        operation: &str,
        definition: &DomainValue,
        request: &DomainValue,
        state: &DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        let (next_state, events) = match operation {
            "advance_track" => resolve_advance_track(request, state)?,
            "cancel_submission" => resolve_cancel_submission(request, state)?,
            "create_track" => resolve_create_track(request, state)?,
            "open_section" => resolve_open_section(request, state)?,
            "preview_probability" => resolve_probability_event(definition, request, state)?,
            "recall_memory" => resolve_memory(request, state, entropy)?,
            "resolve_check" => resolve_check(definition, request, state, entropy)?,
            "resolve_section" => resolve_section(request, state)?,
            "submit_action" => resolve_submit_action(request, state)?,
            "timeout_section" => resolve_timeout_section(request, state)?,
            _ => {
                return Err(invalid(
                    "operation",
                    "unsupported Freehack resolver operation",
                ));
            }
        };
        Ok(ResolverOutput {
            state: next_state,
            events,
        })
    }
}

#[derive(Debug)]
struct CheckConfiguration {
    label: String,
    support: Vec<FreehackContribution>,
    opposition: Vec<FreehackContribution>,
    disclosure: FreehackDisclosure,
    reveal_support: bool,
    reveal_opposition: bool,
    reveal_probability: bool,
}

#[derive(Debug, Clone, Copy)]
struct CheckRoll {
    draw_index: u64,
    signed_result: i64,
    magnitude: i32,
    success: bool,
}

fn resolve_probability_event(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let configuration = check_configuration(definition, request)?;
    let probability = freehack_probability_preview(&FreehackProbabilityRequest {
        probability_format_version: FREEHACK_PROBABILITY_FORMAT_VERSION,
        support: configuration.support.clone(),
        opposition: configuration.opposition.clone(),
    })?;
    let authority_payload = authority_probability_payload(&configuration, &probability);
    let mut events = vec![authority_event(
        "probability_previewed_authority",
        authority_payload,
    )];
    if configuration.disclosure == FreehackDisclosure::Public {
        events.insert(
            0,
            public_event(
                "probability_previewed",
                public_probability_payload(&configuration, &probability),
            ),
        );
    }
    Ok((state.clone(), events))
}

fn resolve_check(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let configuration = check_configuration(definition, request)?;
    let probability = freehack_probability_preview(&FreehackProbabilityRequest {
        probability_format_version: FREEHACK_PROBABILITY_FORMAT_VERSION,
        support: configuration.support.clone(),
        opposition: configuration.opposition.clone(),
    })?;
    let cursor_before = entropy.state().cursor;
    let roll = draw_check(entropy, &probability)?;
    let cursor_after = entropy.state().cursor;

    let mut authority_fields = authority_probability_fields(&configuration, &probability);
    authority_fields.insert(
        "draw_index".to_owned(),
        DomainValue::Number(roll.draw_index as f64),
    );
    authority_fields.insert(
        "magnitude".to_owned(),
        DomainValue::Number(f64::from(roll.magnitude)),
    );
    authority_fields.insert(
        "outcome".to_owned(),
        DomainValue::Symbol(if roll.success { "success" } else { "failure" }.to_owned()),
    );
    authority_fields.insert(
        "signed_result".to_owned(),
        DomainValue::Number(roll.signed_result as f64),
    );

    let mut events = Vec::new();
    if configuration.disclosure == FreehackDisclosure::Public {
        let mut fields = public_probability_fields(&configuration, &probability);
        fields.insert(
            "magnitude".to_owned(),
            DomainValue::Number(f64::from(roll.magnitude)),
        );
        fields.insert(
            "outcome".to_owned(),
            DomainValue::Symbol(if roll.success { "success" } else { "failure" }.to_owned()),
        );
        events.push(public_event("check_resolved", DomainValue::Object(fields)));
    }
    events.push(authority_event(
        "check_resolved_authority",
        DomainValue::Object(authority_fields),
    ));
    events.push(authority_event(
        "entropy_trace",
        object([
            ("cursor_after", DomainValue::Number(cursor_after as f64)),
            ("cursor_before", DomainValue::Number(cursor_before as f64)),
            ("draw_index", DomainValue::Number(roll.draw_index as f64)),
        ]),
    ));
    Ok((state.clone(), events))
}

fn check_configuration(
    definition: &DomainValue,
    request: &DomainValue,
) -> Result<CheckConfiguration, TabletopError> {
    let fields = value_object("input", request)?;
    let label = object_string(fields, "label")?.to_owned();
    validate_bounded_text("input.label", &label, 1, 240)?;
    let support =
        parse_contributions("input.support", object_list(fields, "support")?, definition)?;
    let opposition = parse_contributions(
        "input.opposition",
        object_list(fields, "opposition")?,
        definition,
    )?;
    let support_ids = support
        .iter()
        .map(|contribution| contribution.id.as_str())
        .collect::<BTreeSet<_>>();
    if opposition
        .iter()
        .any(|contribution| support_ids.contains(contribution.id.as_str()))
    {
        return Err(invalid(
            "input",
            "one contribution cannot support and oppose the same check",
        ));
    }
    let disclosure = match object_symbol(fields, "disclosure")? {
        "host_only" => FreehackDisclosure::HostOnly,
        "public" => FreehackDisclosure::Public,
        _ => {
            return Err(invalid("input.disclosure", "expected public or host_only"));
        }
    };
    let reveal_support = object_bool(fields, "reveal_support")?;
    let reveal_opposition = object_bool(fields, "reveal_opposition")?;
    let reveal_probability = object_bool(fields, "reveal_probability")?;
    if disclosure == FreehackDisclosure::HostOnly
        && (reveal_support || reveal_opposition || reveal_probability)
    {
        return Err(invalid(
            "input.disclosure",
            "host-only checks cannot request public numeric fields",
        ));
    }
    if !reveal_opposition && reveal_probability {
        return Err(invalid(
            "input.reveal_probability",
            "probability would disclose a hidden opposition total",
        ));
    }
    Ok(CheckConfiguration {
        label,
        support,
        opposition,
        disclosure,
        reveal_support,
        reveal_opposition,
        reveal_probability,
    })
}

fn parse_contributions(
    path: &str,
    values: &[DomainValue],
    definition: &DomainValue,
) -> Result<Vec<FreehackContribution>, TabletopError> {
    if values.is_empty() || values.len() > MAX_CAMPAIGN_ENTRIES {
        return Err(invalid(path, "expected one to 256 modifier contributions"));
    }
    let definition_fields = value_object("definition", definition)?;
    let character_modifiers = object_map(definition_fields, "modifiers")?;
    let mut contributions = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let fields = value_object(format!("{path}[{index}]"), value)?;
        let id = object_string(fields, "id")?.to_owned();
        let label = object_string(fields, "label")?.to_owned();
        let value = object_u32(fields, "value")?;
        let source = match object_symbol(fields, "source")? {
            "character" => FreehackContributionSource::Character,
            "equipment" => FreehackContributionSource::Equipment,
            "other" => FreehackContributionSource::Other,
            "situation" => FreehackContributionSource::Situation,
            _ => {
                return Err(invalid(
                    format!("{path}[{index}].source"),
                    "unknown contribution source",
                ));
            }
        };
        let contribution = FreehackContribution {
            id: id.clone(),
            label,
            value,
            source,
        };
        if source == FreehackContributionSource::Character {
            let stored = character_modifiers.get(&id).ok_or_else(|| {
                invalid(
                    format!("{path}[{index}].id"),
                    "character contribution is absent from the definition",
                )
            })?;
            let stored_fields = value_object("definition.modifiers", stored)?;
            let stored_value = object_i32(stored_fields, "value")?;
            if stored_value.unsigned_abs() != value {
                return Err(invalid(
                    format!("{path}[{index}].value"),
                    "character contribution must equal the stored modifier magnitude",
                ));
            }
        }
        contributions.push(contribution);
    }
    contribution_total(path, &contributions)?;
    Ok(contributions)
}

fn draw_check(
    entropy: &mut EntropyStream,
    probability: &FreehackProbabilityPreview,
) -> Result<CheckRoll, TabletopError> {
    let draw_index = entropy.draw_bounded(probability.total_weight)?;
    if draw_index < probability.unfavorable_weight {
        let magnitude_from_zero = probability.unfavorable_weight - draw_index;
        let signed_result = -i64::try_from(magnitude_from_zero)
            .map_err(|_| invalid("roll", "negative result overflow"))?;
        let opposition = u64::from(probability.opposition_total);
        let magnitude = magnitude_from_zero
            .checked_add(opposition - 1)
            .map(|value| value / opposition)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| invalid("roll", "negative magnitude overflow"))?;
        Ok(CheckRoll {
            draw_index,
            signed_result,
            magnitude: -magnitude,
            success: false,
        })
    } else {
        let positive_result = draw_index - probability.unfavorable_weight + 1;
        let support = u64::from(probability.support_total);
        let magnitude = positive_result
            .checked_add(support - 1)
            .map(|value| value / support)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| invalid("roll", "positive magnitude overflow"))?;
        Ok(CheckRoll {
            draw_index,
            signed_result: i64::try_from(positive_result)
                .map_err(|_| invalid("roll", "positive result overflow"))?,
            magnitude,
            success: true,
        })
    }
}

fn authority_probability_payload(
    configuration: &CheckConfiguration,
    probability: &FreehackProbabilityPreview,
) -> DomainValue {
    DomainValue::Object(authority_probability_fields(configuration, probability))
}

fn authority_probability_fields(
    configuration: &CheckConfiguration,
    probability: &FreehackProbabilityPreview,
) -> BTreeMap<String, DomainValue> {
    BTreeMap::from([
        (
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure_symbol(configuration.disclosure).to_owned()),
        ),
        (
            "favorable_weight".to_owned(),
            DomainValue::Number(probability.favorable_weight as f64),
        ),
        (
            "label".to_owned(),
            DomainValue::String(configuration.label.clone()),
        ),
        (
            "opposition".to_owned(),
            contributions_value(&configuration.opposition),
        ),
        (
            "opposition_total".to_owned(),
            DomainValue::Number(f64::from(probability.opposition_total)),
        ),
        (
            "success_basis_points_floor".to_owned(),
            DomainValue::Number(f64::from(probability.success_basis_points_floor)),
        ),
        (
            "support".to_owned(),
            contributions_value(&configuration.support),
        ),
        (
            "support_total".to_owned(),
            DomainValue::Number(f64::from(probability.support_total)),
        ),
        (
            "total_weight".to_owned(),
            DomainValue::Number(probability.total_weight as f64),
        ),
        (
            "unfavorable_weight".to_owned(),
            DomainValue::Number(probability.unfavorable_weight as f64),
        ),
    ])
}

fn public_probability_payload(
    configuration: &CheckConfiguration,
    probability: &FreehackProbabilityPreview,
) -> DomainValue {
    DomainValue::Object(public_probability_fields(configuration, probability))
}

fn public_probability_fields(
    configuration: &CheckConfiguration,
    probability: &FreehackProbabilityPreview,
) -> BTreeMap<String, DomainValue> {
    let mut fields = BTreeMap::from([(
        "label".to_owned(),
        DomainValue::String(configuration.label.clone()),
    )]);
    if configuration.reveal_support {
        fields.insert(
            "support_total".to_owned(),
            DomainValue::Number(f64::from(probability.support_total)),
        );
    }
    if configuration.reveal_opposition {
        fields.insert(
            "opposition_total".to_owned(),
            DomainValue::Number(f64::from(probability.opposition_total)),
        );
    }
    if configuration.reveal_probability {
        fields.insert(
            "success_basis_points_floor".to_owned(),
            DomainValue::Number(f64::from(probability.success_basis_points_floor)),
        );
    }
    fields
}

fn contributions_value(contributions: &[FreehackContribution]) -> DomainValue {
    DomainValue::List(
        contributions
            .iter()
            .map(|contribution| {
                object([
                    ("id", DomainValue::String(contribution.id.clone())),
                    ("label", DomainValue::String(contribution.label.clone())),
                    (
                        "source",
                        DomainValue::Symbol(
                            match contribution.source {
                                FreehackContributionSource::Character => "character",
                                FreehackContributionSource::Equipment => "equipment",
                                FreehackContributionSource::Other => "other",
                                FreehackContributionSource::Situation => "situation",
                            }
                            .to_owned(),
                        ),
                    ),
                    ("value", DomainValue::Number(f64::from(contribution.value))),
                ])
            })
            .collect(),
    )
}

fn resolve_memory(
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let id = object_string(fields, "id")?.to_owned();
    let topic = object_string(fields, "topic")?.to_owned();
    let obscurity = object_u32(fields, "obscurity")?;
    let connected = object_bool(fields, "connected")?;
    let disclosure = parse_disclosure(object_symbol(fields, "disclosure")?)?;
    validate_local_identifier("input.id", &id)?;
    validate_bounded_text("input.topic", &topic, 1, 240)?;
    if !(1..=9).contains(&obscurity) {
        return Err(invalid("input.obscurity", "obscurity must be within 1..9"));
    }
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let memories = object_map_mut(state_fields, "memories")?;
    if memories.contains_key(&id) {
        return Err(invalid("input.id", "memory identity already exists"));
    }
    let effective_obscurity = if connected { obscurity / 2 } else { obscurity };
    let cursor_before = entropy.state().cursor;
    let draw = u32::try_from(entropy.draw_bounded(10)? + 1)
        .map_err(|_| invalid("memory.draw", "memory draw overflow"))?;
    let cursor_after = entropy.state().cursor;
    let has_experience = draw < effective_obscurity;
    let rationale = if has_experience {
        "The deterministic memory check placed this topic in the character's past experience."
    } else {
        "The deterministic memory check did not establish prior experience with this topic."
    };
    let memory = object([
        ("connected", DomainValue::Bool(connected)),
        (
            "disclosure",
            DomainValue::Symbol(disclosure_symbol(disclosure).to_owned()),
        ),
        ("draw", DomainValue::Number(f64::from(draw))),
        (
            "effective_obscurity",
            DomainValue::Number(f64::from(effective_obscurity)),
        ),
        ("has_experience", DomainValue::Bool(has_experience)),
        ("obscurity", DomainValue::Number(f64::from(obscurity))),
        ("rationale", DomainValue::String(rationale.to_owned())),
        ("topic", DomainValue::String(topic.clone())),
    ]);
    memories.insert(id.clone(), memory);
    let authority_payload = object([
        ("connected", DomainValue::Bool(connected)),
        ("draw", DomainValue::Number(f64::from(draw))),
        (
            "effective_obscurity",
            DomainValue::Number(f64::from(effective_obscurity)),
        ),
        ("has_experience", DomainValue::Bool(has_experience)),
        ("id", DomainValue::String(id.clone())),
        ("obscurity", DomainValue::Number(f64::from(obscurity))),
        ("topic", DomainValue::String(topic.clone())),
    ]);
    let mut events = Vec::new();
    if disclosure == FreehackDisclosure::Public {
        events.push(public_event(
            "memory_resolved",
            object([
                ("has_experience", DomainValue::Bool(has_experience)),
                ("id", DomainValue::String(id)),
                ("topic", DomainValue::String(topic)),
            ]),
        ));
    }
    events.push(authority_event(
        "memory_resolved_authority",
        authority_payload,
    ));
    events.push(authority_event(
        "entropy_trace",
        object([
            ("cursor_after", DomainValue::Number(cursor_after as f64)),
            ("cursor_before", DomainValue::Number(cursor_before as f64)),
            (
                "draw_index",
                DomainValue::Number(f64::from(draw.saturating_sub(1))),
            ),
        ]),
    ));
    Ok((next, events))
}

fn resolve_create_track(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let template = FreehackTrackTemplate {
        id: object_string(fields, "id")?.to_owned(),
        label: object_string(fields, "label")?.to_owned(),
        secret: object_bool(fields, "secret")?,
        interval: object_string(fields, "interval")?.to_owned(),
        target_count: object_u32(fields, "target_count")?,
        advances_on: parse_track_outcome(object_symbol(fields, "advances_on")?)?,
        consequence: object_string(fields, "consequence")?.to_owned(),
    };
    validate_track_template("input", &template)?;
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let tracks = object_map_mut(state_fields, "tracks")?;
    if tracks.contains_key(&template.id) {
        return Err(invalid("input.id", "track identity already exists"));
    }
    tracks.insert(template.id.clone(), track_state_value(&template));
    let public_payload = object([
        (
            "advances_on",
            DomainValue::Symbol(track_outcome_symbol(template.advances_on).to_owned()),
        ),
        ("id", DomainValue::String(template.id.clone())),
        ("interval", DomainValue::String(template.interval.clone())),
        ("label", DomainValue::String(template.label.clone())),
        (
            "target_count",
            DomainValue::Number(f64::from(template.target_count)),
        ),
    ]);
    let authority_payload = object([
        (
            "advances_on",
            DomainValue::Symbol(track_outcome_symbol(template.advances_on).to_owned()),
        ),
        ("consequence", DomainValue::String(template.consequence)),
        ("id", DomainValue::String(template.id)),
        ("interval", DomainValue::String(template.interval)),
        ("label", DomainValue::String(template.label)),
        ("secret", DomainValue::Bool(template.secret)),
        (
            "target_count",
            DomainValue::Number(f64::from(template.target_count)),
        ),
    ]);
    let mut events = Vec::new();
    if !template.secret {
        events.push(public_event("track_created", public_payload));
    }
    events.push(authority_event(
        "track_created_authority",
        authority_payload,
    ));
    Ok((next, events))
}

fn resolve_advance_track(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let track_id = object_string(fields, "track_id")?.to_owned();
    let outcome = parse_track_outcome(object_symbol(fields, "outcome")?)?;
    validate_local_identifier("input.track_id", &track_id)?;
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let tracks = object_map_mut(state_fields, "tracks")?;
    let track = tracks
        .get_mut(&track_id)
        .ok_or_else(|| invalid("input.track_id", "track does not exist"))?;
    let track_fields = value_object_mut("state.tracks", track)?;
    if object_bool(track_fields, "completed")? {
        return Err(invalid("input.track_id", "track is already complete"));
    }
    let advances_on = parse_track_outcome(object_symbol(track_fields, "advances_on")?)?;
    let before = object_u32(track_fields, "progress")?;
    let target_count = object_u32(track_fields, "target_count")?;
    let after = if outcome == advances_on {
        before.saturating_add(1).min(target_count)
    } else {
        before
    };
    let completed = after == target_count;
    set_u32(track_fields, "progress", after);
    track_fields.insert("completed".to_owned(), DomainValue::Bool(completed));
    let secret = object_bool(track_fields, "secret")?;
    let label = object_string(track_fields, "label")?.to_owned();
    let consequence = object_string(track_fields, "consequence")?.to_owned();
    let public_payload = object([
        ("completed", DomainValue::Bool(completed)),
        ("id", DomainValue::String(track_id.clone())),
        ("label", DomainValue::String(label.clone())),
        ("progress_after", DomainValue::Number(f64::from(after))),
        ("progress_before", DomainValue::Number(f64::from(before))),
        ("target_count", DomainValue::Number(f64::from(target_count))),
    ]);
    let authority_payload = object([
        ("completed", DomainValue::Bool(completed)),
        ("consequence", DomainValue::String(consequence.clone())),
        ("id", DomainValue::String(track_id.clone())),
        ("label", DomainValue::String(label)),
        (
            "outcome",
            DomainValue::Symbol(track_outcome_symbol(outcome).to_owned()),
        ),
        ("progress_after", DomainValue::Number(f64::from(after))),
        ("progress_before", DomainValue::Number(f64::from(before))),
        ("secret", DomainValue::Bool(secret)),
        ("target_count", DomainValue::Number(f64::from(target_count))),
    ]);
    let mut events = Vec::new();
    if !secret {
        events.push(public_event("track_advanced", public_payload));
        if completed {
            events.push(public_event(
                "track_completed",
                object([
                    ("consequence", DomainValue::String(consequence.clone())),
                    ("id", DomainValue::String(track_id.clone())),
                ]),
            ));
        }
    }
    events.push(authority_event(
        "track_advanced_authority",
        authority_payload,
    ));
    if completed {
        events.push(authority_event(
            "track_completed_authority",
            object([
                ("consequence", DomainValue::String(consequence)),
                ("id", DomainValue::String(track_id)),
                ("secret", DomainValue::Bool(secret)),
            ]),
        ));
    }
    Ok((next, events))
}

fn parse_disclosure(value: &str) -> Result<FreehackDisclosure, TabletopError> {
    match value {
        "host_only" => Ok(FreehackDisclosure::HostOnly),
        "public" => Ok(FreehackDisclosure::Public),
        _ => Err(invalid("disclosure", "expected host_only or public")),
    }
}

fn parse_track_outcome(value: &str) -> Result<FreehackTrackOutcome, TabletopError> {
    match value {
        "failure" => Ok(FreehackTrackOutcome::Failure),
        "success" => Ok(FreehackTrackOutcome::Success),
        _ => Err(invalid("outcome", "expected failure or success")),
    }
}

/// Whether a section gathers simultaneous actions against a fixed turn duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackSectionTiming {
    Timed,
    Untimed,
}

/// Whether movement declarations refer to an authority-maintained map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackSectionMapping {
    Mapped,
    Unmapped,
}

/// Scale at which participants direct their characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackSectionAbstraction {
    Automatic,
    Objective,
    Strategic,
}

/// Serializable lifecycle for one section turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FreehackSectionStatus {
    Open,
    Resolved,
    TimedOut,
}

fn resolve_open_section(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let id = object_string(fields, "id")?.to_owned();
    let label = object_string(fields, "label")?.to_owned();
    let timing = parse_section_timing(object_symbol(fields, "timing")?)?;
    let mapping = parse_section_mapping(object_symbol(fields, "mapping")?)?;
    let abstraction = parse_section_abstraction(object_symbol(fields, "abstraction")?)?;
    let reveal_after_resolution = object_bool(fields, "reveal_after_resolution")?;
    let turn_ticks = optional_u32(fields, "turn_ticks")?;
    validate_local_identifier("input.id", &id)?;
    validate_bounded_text("input.label", &label, 1, 160)?;
    match (timing, turn_ticks) {
        (FreehackSectionTiming::Timed, Some(value)) if (1..=1_000_000).contains(&value) => {}
        (FreehackSectionTiming::Timed, _) => {
            return Err(invalid(
                "input.turn_ticks",
                "timed sections require a duration within 1..1000000",
            ));
        }
        (FreehackSectionTiming::Untimed, None) => {}
        (FreehackSectionTiming::Untimed, Some(_)) => {
            return Err(invalid(
                "input.turn_ticks",
                "untimed sections cannot declare a turn duration",
            ));
        }
    }
    let participants = string_values("input.participants", object_list(fields, "participants")?)?;
    if participants.is_empty() || participants.len() > MAX_SECTION_PARTICIPANTS {
        return Err(invalid(
            "input.participants",
            "expected one to 64 section participants",
        ));
    }
    let mut unique = BTreeSet::new();
    for participant in &participants {
        validate_local_identifier("input.participants", participant)?;
        if !unique.insert(participant) {
            return Err(invalid(
                "input.participants",
                "duplicate section participant",
            ));
        }
    }
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let sections = object_map_mut(state_fields, "sections")?;
    if sections.contains_key(&id) {
        return Err(invalid("input.id", "section identity already exists"));
    }
    let participant_value = string_list_value(&participants);
    sections.insert(
        id.clone(),
        object([
            (
                "abstraction",
                DomainValue::Symbol(section_abstraction_symbol(abstraction).to_owned()),
            ),
            ("elapsed_ticks", DomainValue::Number(0.0)),
            ("label", DomainValue::String(label.clone())),
            (
                "mapping",
                DomainValue::Symbol(section_mapping_symbol(mapping).to_owned()),
            ),
            ("outcomes", DomainValue::Object(BTreeMap::new())),
            ("participants", participant_value.clone()),
            ("public_summary", DomainValue::String(String::new())),
            (
                "resolved_order",
                DomainValue::List(Vec::<DomainValue>::new()),
            ),
            (
                "reveal_after_resolution",
                DomainValue::Bool(reveal_after_resolution),
            ),
            (
                "status",
                DomainValue::Symbol(section_status_symbol(FreehackSectionStatus::Open).to_owned()),
            ),
            ("submissions", DomainValue::Object(BTreeMap::new())),
            (
                "timing",
                DomainValue::Symbol(section_timing_symbol(timing).to_owned()),
            ),
            (
                "turn_ticks",
                DomainValue::Number(f64::from(turn_ticks.unwrap_or(0))),
            ),
        ]),
    );
    let payload = object([
        (
            "abstraction",
            DomainValue::Symbol(section_abstraction_symbol(abstraction).to_owned()),
        ),
        ("id", DomainValue::String(id)),
        ("label", DomainValue::String(label)),
        (
            "mapping",
            DomainValue::Symbol(section_mapping_symbol(mapping).to_owned()),
        ),
        ("participants", participant_value),
        (
            "timing",
            DomainValue::Symbol(section_timing_symbol(timing).to_owned()),
        ),
        (
            "turn_ticks",
            DomainValue::Number(f64::from(turn_ticks.unwrap_or(0))),
        ),
    ]);
    Ok((next, vec![public_event("section_opened", payload)]))
}

fn resolve_submit_action(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let section_id = object_string(fields, "section_id")?.to_owned();
    let participant_id = object_string(fields, "participant_id")?.to_owned();
    let submission_id = object_string(fields, "submission_id")?.to_owned();
    let action = object_string(fields, "action")?.to_owned();
    let map_intent = optional_string(fields, "map_intent")?
        .unwrap_or_default()
        .to_owned();
    validate_local_identifier("input.section_id", &section_id)?;
    validate_local_identifier("input.participant_id", &participant_id)?;
    validate_local_identifier("input.submission_id", &submission_id)?;
    validate_bounded_text("input.action", &action, 1, 2_048)?;
    validate_bounded_text("input.map_intent", &map_intent, 0, 512)?;

    let mut next = state.clone();
    let section = section_mut(&mut next, &section_id)?;
    require_open_section(section)?;
    let participants = string_values(
        "state.sections.participants",
        object_list(section, "participants")?,
    )?;
    if !participants.contains(&participant_id) {
        return Err(invalid(
            "input.participant_id",
            "participant is not part of this section",
        ));
    }
    let mapping = parse_section_mapping(object_symbol(section, "mapping")?)?;
    if mapping == FreehackSectionMapping::Mapped && map_intent.is_empty() {
        return Err(invalid(
            "input.map_intent",
            "mapped sections require an explicit movement/position intent",
        ));
    }
    let submissions = object_map_mut(section, "submissions")?;
    if submissions.contains_key(&participant_id) {
        return Err(invalid(
            "input.participant_id",
            "participant already submitted for this section",
        ));
    }
    if submissions.values().any(|submission| {
        value_object("submission", submission)
            .and_then(|fields| object_string(fields, "submission_id"))
            .is_ok_and(|value| value == submission_id)
    }) {
        return Err(invalid(
            "input.submission_id",
            "submission identity already exists",
        ));
    }
    submissions.insert(
        participant_id.clone(),
        object([
            ("action", DomainValue::String(action.clone())),
            ("map_intent", DomainValue::String(map_intent.clone())),
            ("submission_id", DomainValue::String(submission_id.clone())),
        ]),
    );
    let submitted_count = submissions.len() as u32;
    let required_count = participants.len() as u32;
    let public_payload = object([
        (
            "participant_id",
            DomainValue::String(participant_id.clone()),
        ),
        (
            "required_count",
            DomainValue::Number(f64::from(required_count)),
        ),
        ("section_id", DomainValue::String(section_id.clone())),
        (
            "submitted_count",
            DomainValue::Number(f64::from(submitted_count)),
        ),
    ]);
    let authority_payload = object([
        ("action", DomainValue::String(action)),
        ("map_intent", DomainValue::String(map_intent)),
        ("participant_id", DomainValue::String(participant_id)),
        ("section_id", DomainValue::String(section_id)),
        ("submission_id", DomainValue::String(submission_id)),
    ]);
    Ok((
        next,
        vec![
            public_event("submission_recorded", public_payload),
            authority_event("submission_recorded_authority", authority_payload),
        ],
    ))
}

fn resolve_cancel_submission(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let section_id = object_string(fields, "section_id")?.to_owned();
    let participant_id = object_string(fields, "participant_id")?.to_owned();
    validate_local_identifier("input.section_id", &section_id)?;
    validate_local_identifier("input.participant_id", &participant_id)?;
    let mut next = state.clone();
    let section = section_mut(&mut next, &section_id)?;
    require_open_section(section)?;
    let submissions = object_map_mut(section, "submissions")?;
    let removed = submissions.remove(&participant_id).ok_or_else(|| {
        invalid(
            "input.participant_id",
            "participant has no submission to cancel",
        )
    })?;
    let submitted_count = submissions.len() as u32;
    let authority_fields = value_object("submission", &removed)?;
    let authority_payload = object([
        (
            "action",
            DomainValue::String(object_string(authority_fields, "action")?.to_owned()),
        ),
        (
            "map_intent",
            DomainValue::String(object_string(authority_fields, "map_intent")?.to_owned()),
        ),
        (
            "participant_id",
            DomainValue::String(participant_id.clone()),
        ),
        ("section_id", DomainValue::String(section_id.clone())),
        (
            "submission_id",
            DomainValue::String(object_string(authority_fields, "submission_id")?.to_owned()),
        ),
    ]);
    Ok((
        next,
        vec![
            public_event(
                "submission_cancelled",
                object([
                    ("participant_id", DomainValue::String(participant_id)),
                    ("section_id", DomainValue::String(section_id)),
                    (
                        "submitted_count",
                        DomainValue::Number(f64::from(submitted_count)),
                    ),
                ]),
            ),
            authority_event("submission_cancelled_authority", authority_payload),
        ],
    ))
}

fn resolve_section(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let section_id = object_string(fields, "section_id")?.to_owned();
    let public_summary = object_string(fields, "public_summary")?.to_owned();
    validate_local_identifier("input.section_id", &section_id)?;
    validate_bounded_text("input.public_summary", &public_summary, 1, 1_024)?;
    let outcome_values = object_list(fields, "outcomes")?;
    let mut supplied_outcomes = BTreeMap::new();
    for (index, outcome) in outcome_values.iter().enumerate() {
        let fields = value_object(format!("input.outcomes[{index}]"), outcome)?;
        let submission_id = object_string(fields, "submission_id")?.to_owned();
        let summary = object_string(fields, "summary")?.to_owned();
        validate_local_identifier(
            &format!("input.outcomes[{index}].submission_id"),
            &submission_id,
        )?;
        validate_bounded_text(
            format!("input.outcomes[{index}].summary"),
            &summary,
            1,
            1_024,
        )?;
        if supplied_outcomes.insert(submission_id, summary).is_some() {
            return Err(invalid("input.outcomes", "duplicate submission outcome"));
        }
    }

    let mut next = state.clone();
    let section = section_mut(&mut next, &section_id)?;
    require_open_section(section)?;
    let timing = parse_section_timing(object_symbol(section, "timing")?)?;
    let participants = string_values(
        "state.sections.participants",
        object_list(section, "participants")?,
    )?;
    let submissions = object_map(section, "submissions")?.clone();
    if submissions.is_empty() {
        return Err(invalid(
            "input.section_id",
            "cannot resolve a section without submissions",
        ));
    }
    if timing == FreehackSectionTiming::Timed && submissions.len() != participants.len() {
        return Err(invalid(
            "input.section_id",
            "timed section requires every participant submission or an explicit timeout",
        ));
    }
    if supplied_outcomes.len() != submissions.len() {
        return Err(invalid(
            "input.outcomes",
            "one outcome is required for every submitted action",
        ));
    }
    let mut resolved_order = Vec::new();
    let mut authority_rows = Vec::new();
    let mut revealed_rows = Vec::new();
    let reveal_after_resolution = object_bool(section, "reveal_after_resolution")?;
    let mut stored_outcomes = BTreeMap::new();
    for participant in &participants {
        let Some(submission) = submissions.get(participant) else {
            continue;
        };
        let submission_fields = value_object("state.sections.submissions", submission)?;
        let submission_id = object_string(submission_fields, "submission_id")?.to_owned();
        let summary = supplied_outcomes.remove(&submission_id).ok_or_else(|| {
            invalid(
                "input.outcomes",
                "outcome does not match a submitted action",
            )
        })?;
        let action = object_string(submission_fields, "action")?.to_owned();
        let map_intent = object_string(submission_fields, "map_intent")?.to_owned();
        resolved_order.push(participant.clone());
        stored_outcomes.insert(
            submission_id.clone(),
            object([("summary", DomainValue::String(summary.clone()))]),
        );
        let row = object([
            ("action", DomainValue::String(action)),
            ("map_intent", DomainValue::String(map_intent)),
            ("participant_id", DomainValue::String(participant.clone())),
            ("submission_id", DomainValue::String(submission_id)),
            ("summary", DomainValue::String(summary)),
        ]);
        authority_rows.push(row.clone());
        if reveal_after_resolution {
            revealed_rows.push(row);
        }
    }
    if !supplied_outcomes.is_empty() {
        return Err(invalid(
            "input.outcomes",
            "outcome references an unknown submission",
        ));
    }
    section.insert("outcomes".to_owned(), DomainValue::Object(stored_outcomes));
    section.insert(
        "public_summary".to_owned(),
        DomainValue::String(public_summary.clone()),
    );
    section.insert(
        "resolved_order".to_owned(),
        string_list_value(&resolved_order),
    );
    section.insert(
        "status".to_owned(),
        DomainValue::Symbol(section_status_symbol(FreehackSectionStatus::Resolved).to_owned()),
    );
    let mut public_fields = BTreeMap::from([
        (
            "public_summary".to_owned(),
            DomainValue::String(public_summary.clone()),
        ),
        (
            "resolved_order".to_owned(),
            string_list_value(&resolved_order),
        ),
        (
            "section_id".to_owned(),
            DomainValue::String(section_id.clone()),
        ),
    ]);
    if reveal_after_resolution {
        public_fields.insert(
            "revealed_submissions".to_owned(),
            DomainValue::List(revealed_rows),
        );
    }
    let authority_payload = object([
        ("ordered_submissions", DomainValue::List(authority_rows)),
        ("public_summary", DomainValue::String(public_summary)),
        ("section_id", DomainValue::String(section_id)),
    ]);
    Ok((
        next,
        vec![
            public_event("section_resolved", DomainValue::Object(public_fields)),
            authority_event("section_resolved_authority", authority_payload),
        ],
    ))
}

fn resolve_timeout_section(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let section_id = object_string(fields, "section_id")?.to_owned();
    let elapsed_ticks = object_u32(fields, "elapsed_ticks")?;
    validate_local_identifier("input.section_id", &section_id)?;
    let mut next = state.clone();
    let section = section_mut(&mut next, &section_id)?;
    require_open_section(section)?;
    if parse_section_timing(object_symbol(section, "timing")?)? != FreehackSectionTiming::Timed {
        return Err(invalid(
            "input.section_id",
            "only timed sections can time out",
        ));
    }
    let turn_ticks = object_u32(section, "turn_ticks")?;
    if elapsed_ticks < turn_ticks || elapsed_ticks > 1_000_000 {
        return Err(invalid(
            "input.elapsed_ticks",
            "elapsed ticks must reach the configured duration",
        ));
    }
    let participants = string_values(
        "state.sections.participants",
        object_list(section, "participants")?,
    )?;
    let submissions = object_map(section, "submissions")?.clone();
    let submitted = participants
        .iter()
        .filter(|participant| submissions.contains_key(*participant))
        .cloned()
        .collect::<Vec<_>>();
    let missing = participants
        .iter()
        .filter(|participant| !submissions.contains_key(*participant))
        .cloned()
        .collect::<Vec<_>>();
    let authority_rows = submitted
        .iter()
        .map(|participant| {
            let fields = value_object("state.sections.submissions", &submissions[participant])?;
            Ok(object([
                (
                    "action",
                    DomainValue::String(object_string(fields, "action")?.to_owned()),
                ),
                (
                    "map_intent",
                    DomainValue::String(object_string(fields, "map_intent")?.to_owned()),
                ),
                ("participant_id", DomainValue::String(participant.clone())),
                ("section_id", DomainValue::String(section_id.clone())),
                (
                    "submission_id",
                    DomainValue::String(object_string(fields, "submission_id")?.to_owned()),
                ),
            ]))
        })
        .collect::<Result<Vec<_>, TabletopError>>()?;
    section.insert(
        "elapsed_ticks".to_owned(),
        DomainValue::Number(f64::from(elapsed_ticks)),
    );
    section.insert("resolved_order".to_owned(), string_list_value(&submitted));
    section.insert(
        "status".to_owned(),
        DomainValue::Symbol(section_status_symbol(FreehackSectionStatus::TimedOut).to_owned()),
    );
    Ok((
        next,
        vec![
            public_event(
                "section_timed_out",
                object([
                    (
                        "elapsed_ticks",
                        DomainValue::Number(f64::from(elapsed_ticks)),
                    ),
                    ("missing_participants", string_list_value(&missing)),
                    ("section_id", DomainValue::String(section_id.clone())),
                    ("submitted_participants", string_list_value(&submitted)),
                ]),
            ),
            authority_event(
                "section_timed_out_authority",
                object([
                    (
                        "elapsed_ticks",
                        DomainValue::Number(f64::from(elapsed_ticks)),
                    ),
                    ("missing_participants", string_list_value(&missing)),
                    ("section_id", DomainValue::String(section_id)),
                    ("submissions", DomainValue::List(authority_rows)),
                ]),
            ),
        ],
    ))
}

fn section_mut<'a>(
    state: &'a mut DomainValue,
    section_id: &str,
) -> Result<&'a mut BTreeMap<String, DomainValue>, TabletopError> {
    let state_fields = value_object_mut("state", state)?;
    let sections = object_map_mut(state_fields, "sections")?;
    let section = sections
        .get_mut(section_id)
        .ok_or_else(|| invalid("input.section_id", "section does not exist"))?;
    value_object_mut("state.sections", section)
}

fn require_open_section(section: &BTreeMap<String, DomainValue>) -> Result<(), TabletopError> {
    if object_symbol(section, "status")? != "open" {
        return Err(invalid(
            "input.section_id",
            "section no longer accepts mutations",
        ));
    }
    Ok(())
}

fn parse_section_timing(value: &str) -> Result<FreehackSectionTiming, TabletopError> {
    match value {
        "timed" => Ok(FreehackSectionTiming::Timed),
        "untimed" => Ok(FreehackSectionTiming::Untimed),
        _ => Err(invalid("timing", "expected timed or untimed")),
    }
}

fn parse_section_mapping(value: &str) -> Result<FreehackSectionMapping, TabletopError> {
    match value {
        "mapped" => Ok(FreehackSectionMapping::Mapped),
        "unmapped" => Ok(FreehackSectionMapping::Unmapped),
        _ => Err(invalid("mapping", "expected mapped or unmapped")),
    }
}

fn parse_section_abstraction(value: &str) -> Result<FreehackSectionAbstraction, TabletopError> {
    match value {
        "automatic" => Ok(FreehackSectionAbstraction::Automatic),
        "objective" => Ok(FreehackSectionAbstraction::Objective),
        "strategic" => Ok(FreehackSectionAbstraction::Strategic),
        _ => Err(invalid(
            "abstraction",
            "expected automatic, objective, or strategic",
        )),
    }
}

fn parse_section_status(value: &str) -> Result<FreehackSectionStatus, TabletopError> {
    match value {
        "open" => Ok(FreehackSectionStatus::Open),
        "resolved" => Ok(FreehackSectionStatus::Resolved),
        "timed_out" => Ok(FreehackSectionStatus::TimedOut),
        _ => Err(invalid("status", "unknown section status")),
    }
}

const fn section_timing_symbol(value: FreehackSectionTiming) -> &'static str {
    match value {
        FreehackSectionTiming::Timed => "timed",
        FreehackSectionTiming::Untimed => "untimed",
    }
}

const fn section_mapping_symbol(value: FreehackSectionMapping) -> &'static str {
    match value {
        FreehackSectionMapping::Mapped => "mapped",
        FreehackSectionMapping::Unmapped => "unmapped",
    }
}

const fn section_abstraction_symbol(value: FreehackSectionAbstraction) -> &'static str {
    match value {
        FreehackSectionAbstraction::Automatic => "automatic",
        FreehackSectionAbstraction::Objective => "objective",
        FreehackSectionAbstraction::Strategic => "strategic",
    }
}

const fn section_status_symbol(value: FreehackSectionStatus) -> &'static str {
    match value {
        FreehackSectionStatus::Open => "open",
        FreehackSectionStatus::Resolved => "resolved",
        FreehackSectionStatus::TimedOut => "timed_out",
    }
}

/// One non-secret generic track exposed to player/runtime consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackPublicTrack {
    pub id: String,
    pub label: String,
    pub interval: String,
    pub target_count: u32,
    pub progress: u32,
    pub advances_on: FreehackTrackOutcome,
    pub consequence: String,
    pub completed: bool,
}

/// One non-secret memory exposed without its authority-only draw or obscurity values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackPublicMemory {
    pub id: String,
    pub topic: String,
    pub has_experience: bool,
    pub rationale: String,
}

/// One section action deliberately revealed only after a resolved turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackRevealedSubmission {
    pub participant_id: String,
    pub submission_id: String,
    pub action: String,
    pub map_intent: String,
    pub summary: String,
}

/// Leak-free player/runtime view of one section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackPublicSection {
    pub id: String,
    pub label: String,
    pub timing: FreehackSectionTiming,
    pub mapping: FreehackSectionMapping,
    pub abstraction: FreehackSectionAbstraction,
    pub status: FreehackSectionStatus,
    pub turn_ticks: Option<u32>,
    pub elapsed_ticks: u32,
    pub participants: Vec<String>,
    pub submitted_participants: Vec<String>,
    pub resolved_order: Vec<String>,
    pub public_summary: Option<String>,
    pub revealed_submissions: Vec<FreehackRevealedSubmission>,
}

/// Public state intentionally omits entropy and all host-only state records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackPublicState {
    pub projection_format_version: u32,
    pub adapter: ResolvedAdapter,
    pub owner_id: String,
    pub tracks: Vec<FreehackPublicTrack>,
    pub sections: Vec<FreehackPublicSection>,
    pub memories: Vec<FreehackPublicMemory>,
}

/// Public event with no placeholder for undisclosed host events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackPublicEvent {
    pub sequence: u64,
    pub kind: String,
    pub capability: TabletopCapability,
    pub payload: DomainValue,
}

/// Player/runtime receipt containing only facts explicitly marked public.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackPublicReceipt {
    pub projection_format_version: u32,
    pub adapter: ResolvedAdapter,
    pub operation: String,
    pub capability: TabletopCapability,
    pub public_state_sha256: String,
    pub public_state: FreehackPublicState,
    pub events: Vec<FreehackPublicEvent>,
}

/// Authority artifact retaining the validated generic receipt and every private payload.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreehackAuthorityReceipt {
    pub projection_format_version: u32,
    pub receipt: ResolutionReceipt,
}

impl_freehack_io!(FreehackPublicState);
impl_freehack_io!(FreehackPublicReceipt);
impl_freehack_io!(FreehackAuthorityReceipt);

/// Canonical JSON Schema for leak-free public Freehack state.
pub fn freehack_public_state_schema() -> Result<String, TabletopError> {
    freehack_schema::<FreehackPublicState>(
        "urn:weave:schema:tabletop-freehack-public-state:1",
        "Weave Freehack Public State v1",
        Some((
            "projection_format_version",
            FREEHACK_PROJECTION_FORMAT_VERSION,
        )),
    )
}

/// Canonical JSON Schema for leak-free public Freehack receipts.
pub fn freehack_public_receipt_schema() -> Result<String, TabletopError> {
    freehack_schema::<FreehackPublicReceipt>(
        "urn:weave:schema:tabletop-freehack-public-receipt:1",
        "Weave Freehack Public Receipt v1",
        Some((
            "projection_format_version",
            FREEHACK_PROJECTION_FORMAT_VERSION,
        )),
    )
}

/// Canonical JSON Schema for authority-only Freehack receipts.
pub fn freehack_authority_receipt_schema() -> Result<String, TabletopError> {
    freehack_schema::<FreehackAuthorityReceipt>(
        "urn:weave:schema:tabletop-freehack-authority-receipt:1",
        "Weave Freehack Authority Receipt v1",
        Some((
            "projection_format_version",
            FREEHACK_PROJECTION_FORMAT_VERSION,
        )),
    )
}

/// Wrap one complete receipt for authority-only persistence or transport.
pub fn project_freehack_authority_receipt(
    receipt: &ResolutionReceipt,
) -> Result<FreehackAuthorityReceipt, TabletopError> {
    let manifest = freehack_manifest();
    validate_resolution_receipt(receipt, &manifest)?;
    if receipt.events.iter().any(|event| event.payload.is_none()) {
        return Err(invalid(
            "receipt.events",
            "authority projection requires complete event payloads",
        ));
    }
    Ok(FreehackAuthorityReceipt {
        projection_format_version: FREEHACK_PROJECTION_FORMAT_VERSION,
        receipt: receipt.clone(),
    })
}

/// Validate an authority wrapper and require every private payload to remain present.
pub fn validate_freehack_authority_receipt(
    authority: &FreehackAuthorityReceipt,
) -> Result<(), TabletopError> {
    if authority.projection_format_version != FREEHACK_PROJECTION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "projection_format_version",
        });
    }
    let expected = project_freehack_authority_receipt(&authority.receipt)?;
    if &expected != authority {
        return Err(invalid(
            "authority_receipt",
            "authority wrapper is not canonical",
        ));
    }
    Ok(())
}

/// Produce a player/runtime receipt, or `None` when the operation has no public observable event.
///
/// Host-only event envelopes are removed rather than retained with missing payloads. The public
/// artifact also omits request/state hashes derived from private input and all entropy fields.
pub fn project_freehack_public_receipt(
    receipt: &ResolutionReceipt,
) -> Result<Option<FreehackPublicReceipt>, TabletopError> {
    let manifest = freehack_manifest();
    validate_resolution_receipt(receipt, &manifest)?;
    let events = receipt
        .events
        .iter()
        .filter(|event| event.visibility == EventVisibility::Public)
        .enumerate()
        .map(|(sequence, event)| public_event_projection(sequence as u64, event))
        .collect::<Result<Vec<_>, _>>()?;
    if events.is_empty() {
        return Ok(None);
    }
    let public_state = project_freehack_public_state(&receipt.after_state)?;
    let public_state_sha256 = canonical_fingerprint(&public_state)?;
    let projected = FreehackPublicReceipt {
        projection_format_version: FREEHACK_PROJECTION_FORMAT_VERSION,
        adapter: receipt.adapter.clone(),
        operation: receipt.operation.clone(),
        capability: receipt.capability,
        public_state_sha256,
        public_state,
        events,
    };
    validate_freehack_public_receipt(&projected)?;
    Ok(Some(projected))
}

fn public_event_projection(
    sequence: u64,
    event: &TabletopEvent,
) -> Result<FreehackPublicEvent, TabletopError> {
    let payload = event.payload.clone().ok_or_else(|| {
        invalid(
            "receipt.events.payload",
            "public event payload is unexpectedly absent",
        )
    })?;
    Ok(FreehackPublicEvent {
        sequence,
        kind: event.kind.clone(),
        capability: event.capability,
        payload,
    })
}

/// Strip one validated authority state to the explicit player/runtime state contract.
pub fn project_freehack_public_state(
    state: &TabletopState,
) -> Result<FreehackPublicState, TabletopError> {
    let manifest = freehack_manifest();
    validate_tabletop_state(state, &manifest)?;
    let fields = value_object("state.value", &state.value)?;
    let tracks = object_map(fields, "tracks")?
        .iter()
        .filter_map(|(id, value)| match public_track(id, value) {
            Ok(Some(track)) => Some(Ok(track)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sections = object_map(fields, "sections")?
        .iter()
        .map(|(id, value)| public_section(id, value))
        .collect::<Result<Vec<_>, _>>()?;
    let memories = object_map(fields, "memories")?
        .iter()
        .filter_map(|(id, value)| match public_memory(id, value) {
            Ok(Some(memory)) => Some(Ok(memory)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FreehackPublicState {
        projection_format_version: FREEHACK_PROJECTION_FORMAT_VERSION,
        adapter: state.adapter.clone(),
        owner_id: state.owner_id.clone(),
        tracks,
        sections,
        memories,
    })
}

/// Validate one standalone public state without requiring access to authority state.
pub fn validate_freehack_public_state(state: &FreehackPublicState) -> Result<(), TabletopError> {
    if state.projection_format_version != FREEHACK_PROJECTION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "projection_format_version",
        });
    }
    if state.adapter != resolved_adapter(&freehack_manifest())? {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    validate_global_identifier("owner_id", &state.owner_id)?;
    let mut ids = BTreeSet::new();
    for track in &state.tracks {
        validate_track_template(
            "public_state.tracks",
            &FreehackTrackTemplate {
                id: track.id.clone(),
                label: track.label.clone(),
                secret: false,
                interval: track.interval.clone(),
                target_count: track.target_count,
                advances_on: track.advances_on,
                consequence: track.consequence.clone(),
            },
        )?;
        if track.progress > track.target_count
            || track.completed != (track.progress == track.target_count)
        {
            return Err(invalid(
                "public_state.tracks",
                "track progress/completion is inconsistent",
            ));
        }
        if !ids.insert(&track.id) {
            return Err(invalid("public_state.tracks", "duplicate track identity"));
        }
    }
    ids.clear();
    for section in &state.sections {
        validate_local_identifier("public_state.sections.id", &section.id)?;
        validate_bounded_text("public_state.sections.label", &section.label, 1, 160)?;
        if section.participants.is_empty() || section.participants.len() > MAX_SECTION_PARTICIPANTS
        {
            return Err(invalid(
                "public_state.sections.participants",
                "expected one to 64 participants",
            ));
        }
        let participant_set = section.participants.iter().collect::<BTreeSet<_>>();
        if participant_set.len() != section.participants.len()
            || section
                .submitted_participants
                .iter()
                .any(|participant| !participant_set.contains(participant))
            || section
                .resolved_order
                .iter()
                .any(|participant| !participant_set.contains(participant))
        {
            return Err(invalid(
                "public_state.sections",
                "participant sets are inconsistent",
            ));
        }
        match (section.timing, section.turn_ticks) {
            (FreehackSectionTiming::Timed, Some(value)) if (1..=1_000_000).contains(&value) => {}
            (FreehackSectionTiming::Untimed, None) => {}
            _ => {
                return Err(invalid(
                    "public_state.sections.turn_ticks",
                    "timing and duration disagree",
                ));
            }
        }
        if section.status == FreehackSectionStatus::Open
            && (!section.resolved_order.is_empty()
                || section.public_summary.is_some()
                || !section.revealed_submissions.is_empty())
        {
            return Err(invalid(
                "public_state.sections",
                "open section exposes resolved data",
            ));
        }
        if !ids.insert(&section.id) {
            return Err(invalid(
                "public_state.sections",
                "duplicate section identity",
            ));
        }
    }
    ids.clear();
    for memory in &state.memories {
        validate_local_identifier("public_state.memories.id", &memory.id)?;
        validate_bounded_text("public_state.memories.topic", &memory.topic, 1, 240)?;
        validate_bounded_text("public_state.memories.rationale", &memory.rationale, 1, 512)?;
        if !ids.insert(&memory.id) {
            return Err(invalid(
                "public_state.memories",
                "duplicate memory identity",
            ));
        }
    }
    Ok(())
}

/// Validate a standalone public receipt and every manifest-declared public payload.
pub fn validate_freehack_public_receipt(
    receipt: &FreehackPublicReceipt,
) -> Result<(), TabletopError> {
    if receipt.projection_format_version != FREEHACK_PROJECTION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "projection_format_version",
        });
    }
    let manifest = freehack_manifest();
    if receipt.adapter != resolved_adapter(&manifest)?
        || receipt.public_state.adapter != receipt.adapter
    {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    validate_freehack_public_state(&receipt.public_state)?;
    if receipt.public_state_sha256 != canonical_fingerprint(&receipt.public_state)? {
        return Err(TabletopError::ContentHashMismatch {
            path: "public_state_sha256",
        });
    }
    let operation = manifest
        .operations
        .get(&receipt.operation)
        .ok_or_else(|| invalid("operation", "operation is not declared"))?;
    if operation.required_capability != receipt.capability || receipt.events.is_empty() {
        return Err(invalid("events", "public event capability is inconsistent"));
    }
    for (index, event) in receipt.events.iter().enumerate() {
        if event.sequence != index as u64
            || event.capability != receipt.capability
            || !operation.event_kinds.contains(&event.kind)
            || event.kind.ends_with("_authority")
            || event.kind == "entropy_trace"
        {
            return Err(invalid(
                "events",
                "event is not part of the public operation contract",
            ));
        }
        let value_type = manifest
            .event_types
            .get(&event.kind)
            .ok_or_else(|| invalid("events.kind", "event type is not declared"))?;
        validate_typed_value(
            "events.payload",
            &event.payload,
            value_type,
            &manifest.types,
        )
        .map_err(|_| TabletopError::SchemaMismatch {
            path: "events.payload",
        })?;
    }
    Ok(())
}

fn public_track(
    id: &str,
    value: &DomainValue,
) -> Result<Option<FreehackPublicTrack>, TabletopError> {
    let fields = value_object("state.tracks", value)?;
    if object_bool(fields, "secret")? {
        return Ok(None);
    }
    Ok(Some(FreehackPublicTrack {
        id: id.to_owned(),
        label: object_string(fields, "label")?.to_owned(),
        interval: object_string(fields, "interval")?.to_owned(),
        target_count: object_u32(fields, "target_count")?,
        progress: object_u32(fields, "progress")?,
        advances_on: parse_track_outcome(object_symbol(fields, "advances_on")?)?,
        consequence: object_string(fields, "consequence")?.to_owned(),
        completed: object_bool(fields, "completed")?,
    }))
}

fn public_memory(
    id: &str,
    value: &DomainValue,
) -> Result<Option<FreehackPublicMemory>, TabletopError> {
    let fields = value_object("state.memories", value)?;
    if parse_disclosure(object_symbol(fields, "disclosure")?)? == FreehackDisclosure::HostOnly {
        return Ok(None);
    }
    Ok(Some(FreehackPublicMemory {
        id: id.to_owned(),
        topic: object_string(fields, "topic")?.to_owned(),
        has_experience: object_bool(fields, "has_experience")?,
        rationale: object_string(fields, "rationale")?.to_owned(),
    }))
}

fn public_section(id: &str, value: &DomainValue) -> Result<FreehackPublicSection, TabletopError> {
    let fields = value_object("state.sections", value)?;
    let timing = parse_section_timing(object_symbol(fields, "timing")?)?;
    let status = parse_section_status(object_symbol(fields, "status")?)?;
    let participants = string_values(
        "state.sections.participants",
        object_list(fields, "participants")?,
    )?;
    let submissions = object_map(fields, "submissions")?;
    let submitted_participants = participants
        .iter()
        .filter(|participant| submissions.contains_key(*participant))
        .cloned()
        .collect::<Vec<_>>();
    let resolved_order = string_values(
        "state.sections.resolved_order",
        object_list(fields, "resolved_order")?,
    )?;
    let summary = object_string(fields, "public_summary")?;
    let public_summary = (!summary.is_empty()).then(|| summary.to_owned());
    let reveal = object_bool(fields, "reveal_after_resolution")?
        && status == FreehackSectionStatus::Resolved;
    let outcomes = object_map(fields, "outcomes")?;
    let mut revealed_submissions = Vec::new();
    if reveal {
        for participant in &resolved_order {
            let submission = submissions.get(participant).ok_or_else(|| {
                invalid(
                    "state.sections.resolved_order",
                    "resolved participant lacks a submission",
                )
            })?;
            let submission_fields = value_object("state.sections.submissions", submission)?;
            let submission_id = object_string(submission_fields, "submission_id")?.to_owned();
            let outcome = outcomes.get(&submission_id).ok_or_else(|| {
                invalid(
                    "state.sections.outcomes",
                    "resolved submission lacks an outcome",
                )
            })?;
            let outcome_fields = value_object("state.sections.outcomes", outcome)?;
            revealed_submissions.push(FreehackRevealedSubmission {
                participant_id: participant.clone(),
                submission_id,
                action: object_string(submission_fields, "action")?.to_owned(),
                map_intent: object_string(submission_fields, "map_intent")?.to_owned(),
                summary: object_string(outcome_fields, "summary")?.to_owned(),
            });
        }
    }
    Ok(FreehackPublicSection {
        id: id.to_owned(),
        label: object_string(fields, "label")?.to_owned(),
        timing,
        mapping: parse_section_mapping(object_symbol(fields, "mapping")?)?,
        abstraction: parse_section_abstraction(object_symbol(fields, "abstraction")?)?,
        status,
        turn_ticks: (timing == FreehackSectionTiming::Timed)
            .then(|| object_u32(fields, "turn_ticks"))
            .transpose()?,
        elapsed_ticks: object_u32(fields, "elapsed_ticks")?,
        participants,
        submitted_participants,
        resolved_order,
        public_summary,
        revealed_submissions,
    })
}

/// Build the exact verified manifest shared by every Freehack surface.
#[must_use]
pub fn freehack_manifest() -> AdapterManifest {
    let contribution_type = object_type([
        (
            "id",
            required(string(1, 128), "Stable contribution identity."),
        ),
        (
            "label",
            required(string(1, 160), "Contribution display label."),
        ),
        (
            "source",
            required(
                symbol(["character", "equipment", "other", "situation"]),
                "Origin category used for integrity checks.",
            ),
        ),
        (
            "value",
            required(
                integer(1.0, f64::from(MAX_MODIFIER_TOTAL)),
                "Positive contribution magnitude.",
            ),
        ),
    ]);
    let modifier_definition_type = object_type([
        ("label", required(string(1, 160), "Campaign display label.")),
        (
            "player_visible",
            required(
                TypeExpression::Bool,
                "Whether this modifier is player-visible.",
            ),
        ),
        (
            "value",
            required(integer(-1_000_000.0, 1_000_000.0), "Stored numeric value."),
        ),
    ]);
    let feature_definition_type = object_type([
        (
            "description",
            required(
                string(1, 512),
                "Campaign or author-supplied feature detail.",
            ),
        ),
        ("label", required(string(1, 160), "Feature display label.")),
        (
            "source",
            required(symbol(["authored", "catalog"]), "Feature origin."),
        ),
    ]);
    let inventory_definition_type = object_type([
        (
            "cost_per_item",
            required(integer(0.0, f64::from(u32::MAX)), "Configured point cost."),
        ),
        ("label", required(string(1, 160), "Item display label.")),
        (
            "quantity",
            required(integer(1.0, 1_000.0), "Selected item quantity."),
        ),
        (
            "source",
            required(symbol(["authored", "catalog"]), "Inventory origin."),
        ),
    ]);
    let memory_state_type = object_type([
        (
            "connected",
            required(
                TypeExpression::Bool,
                "Whether the topic had a prior connection.",
            ),
        ),
        (
            "disclosure",
            required(disclosure_type(), "Player/runtime disclosure boundary."),
        ),
        (
            "draw",
            required(integer(0.0, 10.0), "Authority-only d10 result."),
        ),
        (
            "effective_obscurity",
            required(integer(0.0, 9.0), "Connection-adjusted comparison value."),
        ),
        (
            "has_experience",
            required(
                TypeExpression::Bool,
                "Whether prior experience was established.",
            ),
        ),
        (
            "obscurity",
            required(integer(0.0, 9.0), "Configured topic obscurity."),
        ),
        (
            "rationale",
            required(string(1, 512), "Independent result rationale."),
        ),
        ("topic", required(string(1, 240), "Memory topic.")),
    ]);
    let track_state_type = object_type([
        (
            "advances_on",
            required(track_outcome_type(), "Outcome that increments the track."),
        ),
        (
            "completed",
            required(TypeExpression::Bool, "Whether progress reached the target."),
        ),
        (
            "consequence",
            required(string(1, 512), "Consequence applied at completion."),
        ),
        (
            "interval",
            required(string(1, 240), "Time or event trigger."),
        ),
        ("label", required(string(1, 160), "Track display label.")),
        (
            "progress",
            required(integer(0.0, 1_000_000.0), "Current qualifying outcomes."),
        ),
        (
            "secret",
            required(TypeExpression::Bool, "Whether the track is authority-only."),
        ),
        (
            "target_count",
            required(integer(1.0, 1_000_000.0), "Qualifying outcomes required."),
        ),
    ]);
    let submission_state_type = object_type([
        (
            "action",
            required(string(1, 2_048), "Submitted fictional action."),
        ),
        (
            "map_intent",
            required(string(0, 512), "Mapped position or movement declaration."),
        ),
        (
            "submission_id",
            required(string(1, 128), "Stable submission identity."),
        ),
    ]);
    let outcome_state_type = object_type([(
        "summary",
        required(string(1, 1_024), "Authority-authored outcome summary."),
    )]);
    let section_state_type = object_type([
        (
            "abstraction",
            required(section_abstraction_type(), "Configured control scale."),
        ),
        (
            "elapsed_ticks",
            required(integer(0.0, 1_000_000.0), "Elapsed host timing units."),
        ),
        ("label", required(string(1, 160), "Section display label.")),
        (
            "mapping",
            required(section_mapping_type(), "Position/movement mode."),
        ),
        (
            "outcomes",
            required(
                map(named("OutcomeState"), 0, 64),
                "Resolved submission outcomes.",
            ),
        ),
        (
            "participants",
            required(list(string(1, 128), 1, 64), "Stable participant order."),
        ),
        (
            "public_summary",
            required(string(0, 1_024), "Summary revealed after resolution."),
        ),
        (
            "resolved_order",
            required(
                list(string(1, 128), 0, 64),
                "Deterministic participant order.",
            ),
        ),
        (
            "reveal_after_resolution",
            required(
                TypeExpression::Bool,
                "Whether completed submissions may enter the public state.",
            ),
        ),
        (
            "status",
            required(section_status_type(), "Section lifecycle state."),
        ),
        (
            "submissions",
            required(
                map(named("SubmissionState"), 0, 64),
                "Authority-held simultaneous submissions.",
            ),
        ),
        (
            "timing",
            required(section_timing_type(), "Timed or free-action mode."),
        ),
        (
            "turn_ticks",
            required(
                integer(0.0, 1_000_000.0),
                "Zero for untimed; positive for timed.",
            ),
        ),
    ]);
    let submission_outcome_type = object_type([
        ("action", required(string(1, 2_048), "Submitted action.")),
        (
            "map_intent",
            required(string(0, 512), "Mapped position or movement declaration."),
        ),
        (
            "participant_id",
            required(string(1, 128), "Stable participant identity."),
        ),
        (
            "submission_id",
            required(string(1, 128), "Stable submission identity."),
        ),
        (
            "summary",
            required(string(1, 1_024), "Resolved outcome summary."),
        ),
    ]);
    let types = BTreeMap::from([
        ("Contribution".to_owned(), contribution_type),
        ("FeatureDefinition".to_owned(), feature_definition_type),
        ("InventoryDefinition".to_owned(), inventory_definition_type),
        ("MemoryState".to_owned(), memory_state_type),
        ("ModifierDefinition".to_owned(), modifier_definition_type),
        ("OutcomeState".to_owned(), outcome_state_type),
        ("SectionState".to_owned(), section_state_type),
        ("SubmissionOutcome".to_owned(), submission_outcome_type),
        ("SubmissionState".to_owned(), submission_state_type),
        ("TrackState".to_owned(), track_state_type),
    ]);

    let definition_type = object_type([
        (
            "archetype_id",
            required(
                string(0, 128),
                "Selected campaign archetype or an empty value.",
            ),
        ),
        (
            "campaign_id",
            required(string(3, 255), "Stable campaign-schema identity."),
        ),
        (
            "campaign_schema_sha256",
            required(string(64, 64), "Exact campaign-schema fingerprint."),
        ),
        (
            "features",
            required(
                map(named("FeatureDefinition"), 0, 256),
                "Selected generic features.",
            ),
        ),
        (
            "inventory",
            required(
                map(named("InventoryDefinition"), 0, 256),
                "Selected generic inventory.",
            ),
        ),
        (
            "modifiers",
            required(
                map(named("ModifierDefinition"), 0, 256),
                "Campaign-named numeric modifiers.",
            ),
        ),
        ("name", required(string(1, 160), "Fictional display name.")),
    ]);
    let state_type = object_type([
        (
            "memories",
            required(
                map(named("MemoryState"), 0, 256),
                "Established memory records.",
            ),
        ),
        (
            "sections",
            required(
                map(named("SectionState"), 0, 256),
                "Saved simultaneous sections.",
            ),
        ),
        (
            "tracks",
            required(map(named("TrackState"), 0, 256), "Generic progress tracks."),
        ),
    ]);

    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: FREEHACK_ADAPTER_ID.to_owned(),
        version: FREEHACK_ADAPTER_VERSION.to_owned(),
        schema_version: 1,
        namespace: "freehack".to_owned(),
        title: "Freehack 2.1 adapter".to_owned(),
        compatibility_label: "Compatible with Freehack 2.1".to_owned(),
        summary: "Setting-neutral campaign-defined characters, exact host resolution, generic tracks, memory, and simultaneous sections with explicit authority/public boundaries."
            .to_owned(),
        weave_version: ">=0.1.0, <0.2.0".to_owned(),
        character_contract_version: ">=1.0.0, <2.0.0".to_owned(),
        extension_surface: AdapterExtensionSurface::DeclarativeDataWithRegisteredResolver {
            resolver_contract_version: RESOLVER_CONTRACT_VERSION,
        },
        capabilities: vec![
            capability(TabletopCapability::CharacterCreation),
            capability(TabletopCapability::ChecksAndConflicts),
            capability(TabletopCapability::ResourcesAndConditions),
            capability(TabletopCapability::Scenes),
            capability(TabletopCapability::CampaignState),
        ],
        types,
        definition_type,
        state_type,
        creation_steps: creation_steps(),
        operations: operations(),
        event_types: event_types(),
        visibility: HostVisibilityPolicy::default(),
        migrations: Vec::new(),
        provenance: AdapterProvenance {
            source_class: AdapterSourceClass::VerifiedCc0,
            source_url: FREEHACK_RELEASE_URL.to_owned(),
            exact_artifact: "freehack.pdf".to_owned(),
            revision: "2.1 (released 2026-07-13)".to_owned(),
            retrieved_on: "2026-08-25".to_owned(),
            sha256: FREEHACK_PDF_SHA256.to_owned(),
            license: "CC0-1.0".to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                .to_owned(),
            covered_files_or_sections: vec![
                "Freehack 2.1 core mechanics: modifiers, rolls, tracks, sections, creation, and memory."
                    .to_owned(),
                "Exact integer behavior cross-checked against the official roll tool."
                    .to_owned(),
            ],
            exclusions: vec![
                ExcludedMaterial::CommunitySupplements,
                ExcludedMaterial::Logos,
                ExcludedMaterial::TradeDress,
                ExcludedMaterial::Artwork,
                ExcludedMaterial::Layout,
                ExcludedMaterial::UnverifiedAssets,
            ],
            attribution: "Freehack was created by Amini Allight and released under CC0-1.0; Weave's adapter implementation, fixtures, schemas, and explanatory copy are independently authored."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "LICENSE-CC0-1.0.txt".to_owned(),
                sha256: FREEHACK_CC0_1_0_LEGAL_CODE_SHA256.to_owned(),
            },
            additional_artifacts: vec![
                AdapterSourceArtifact {
                    source_url: format!(
                        "{FREEHACK_SOURCE_URL}/-/raw/{FREEHACK_SOURCE_REVISION}/src/freehack.md"
                    ),
                    exact_artifact: "src/freehack.md".to_owned(),
                    revision: FREEHACK_SOURCE_REVISION.to_owned(),
                    retrieved_on: "2026-08-25".to_owned(),
                    sha256: FREEHACK_MARKDOWN_SHA256.to_owned(),
                    media_type: "text/markdown".to_owned(),
                    purpose: "Exact source text reviewed for the core behavior boundary."
                        .to_owned(),
                    redistributed: false,
                },
                AdapterSourceArtifact {
                    source_url: format!(
                        "{FREEHACK_SOURCE_URL}/-/raw/{FREEHACK_SOURCE_REVISION}/src/freehack.yml"
                    ),
                    exact_artifact: "src/freehack.yml".to_owned(),
                    revision: FREEHACK_SOURCE_REVISION.to_owned(),
                    retrieved_on: "2026-08-25".to_owned(),
                    sha256: FREEHACK_METADATA_SHA256.to_owned(),
                    media_type: "application/yaml".to_owned(),
                    purpose: "Publication metadata reviewed with the source revision.".to_owned(),
                    redistributed: false,
                },
                AdapterSourceArtifact {
                    source_url: format!(
                        "{FREEHACK_SOURCE_URL}/-/raw/{FREEHACK_SOURCE_REVISION}/tools/roll.py"
                    ),
                    exact_artifact: "tools/roll.py".to_owned(),
                    revision: FREEHACK_SOURCE_REVISION.to_owned(),
                    retrieved_on: "2026-08-25".to_owned(),
                    sha256: FREEHACK_ROLL_TOOL_SHA256.to_owned(),
                    media_type: "text/x-python".to_owned(),
                    purpose: "Official executable reference used to cross-check result bands."
                        .to_owned(),
                    redistributed: false,
                },
            ],
            notices: vec![
                "Only hashes and independently authored behavior are bundled; the upstream PDF, icon, stylesheet, and page layout are excluded."
                    .to_owned(),
                "Freehack public payloads must be produced by the dedicated projection API; generic receipt projection retains host-event envelopes and is not a player transport."
                    .to_owned(),
            ],
            compatibility_statement: "Freehack is named only to describe rules compatibility. This project is independent and does not imply endorsement by the original creator."
                .to_owned(),
        },
    }
}

fn creation_steps() -> Vec<CreationStep> {
    vec![
        CreationStep {
            id: "campaign".to_owned(),
            title: "Campaign procedure".to_owned(),
            description: "Choose the versioned setting-neutral schema that defines available character fields and budgets."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![creation_field(
                "campaign_schema",
                "Campaign schema",
                string(1, 16_384),
                true,
            )],
        },
        CreationStep {
            id: "identity_and_modifiers".to_owned(),
            title: "Identity and modifiers".to_owned(),
            description: "Name the character, accept an offered archetype when configured, and assign campaign-declared numeric values."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("name", "Name", string(1, 160), true),
                creation_field("archetype", "Archetype", string(0, 128), false),
                creation_field(
                    "modifiers",
                    "Numeric modifiers",
                    map(integer(-1_000_000.0, 1_000_000.0), 0, 256),
                    true,
                ),
            ],
        },
        CreationStep {
            id: "features_and_inventory".to_owned(),
            title: "Features, inventory, and memories".to_owned(),
            description: "Select or author permitted details while preserving configured feature counts and item budgets."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("features", "Features", list(string(1, 512), 0, 256), true),
                creation_field("inventory", "Inventory", list(string(1, 512), 0, 256), true),
                creation_field("memories", "Memories", list(string(1, 512), 0, 256), false),
            ],
        },
    ]
}

fn check_request_type() -> TypeExpression {
    object_type([
        (
            "disclosure",
            required(disclosure_type(), "Whether the roll itself is public."),
        ),
        (
            "label",
            required(string(1, 240), "Authority-authored check label."),
        ),
        (
            "opposition",
            required(list(named("Contribution"), 1, 256), "Opposing modifiers."),
        ),
        (
            "reveal_opposition",
            required(
                TypeExpression::Bool,
                "Expose the opposition total publicly.",
            ),
        ),
        (
            "reveal_probability",
            required(
                TypeExpression::Bool,
                "Expose the exact probability preview publicly.",
            ),
        ),
        (
            "reveal_support",
            required(TypeExpression::Bool, "Expose the support total publicly."),
        ),
        (
            "support",
            required(list(named("Contribution"), 1, 256), "Supporting modifiers."),
        ),
    ])
}

fn operations() -> BTreeMap<String, ResolverOperationDeclaration> {
    BTreeMap::from([
        (
            "advance_track".to_owned(),
            operation(
                "advance_track",
                "Advance a generic track",
                "Apply one explicit success/failure result to a track and emit its consequence at the configured target.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    (
                        "outcome",
                        required(track_outcome_type(), "Observed check outcome."),
                    ),
                    ("track_id", required(string(1, 128), "Track identity.")),
                ]),
                [
                    "track_advanced",
                    "track_advanced_authority",
                    "track_completed",
                    "track_completed_authority",
                ],
                false,
            ),
        ),
        (
            "cancel_submission".to_owned(),
            operation(
                "cancel_submission",
                "Cancel a pending action",
                "Remove one participant submission while its section remains open.",
                TabletopCapability::Scenes,
                object_type([
                    (
                        "participant_id",
                        required(string(1, 128), "Participant whose action is withdrawn."),
                    ),
                    (
                        "section_id",
                        required(string(1, 128), "Open section identity."),
                    ),
                ]),
                ["submission_cancelled", "submission_cancelled_authority"],
                false,
            ),
        ),
        (
            "create_track".to_owned(),
            operation(
                "create_track",
                "Create a generic track",
                "Create an interval/target/consequence track without assuming a setting subsystem.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    (
                        "advances_on",
                        required(track_outcome_type(), "Outcome that increments progress."),
                    ),
                    (
                        "consequence",
                        required(string(1, 512), "Completion consequence."),
                    ),
                    ("id", required(string(1, 128), "Stable track identity.")),
                    (
                        "interval",
                        required(string(1, 240), "Time or event trigger."),
                    ),
                    ("label", required(string(1, 160), "Track display label.")),
                    (
                        "secret",
                        required(TypeExpression::Bool, "Authority-only track flag."),
                    ),
                    (
                        "target_count",
                        required(integer(1.0, 1_000_000.0), "Qualifying outcomes required."),
                    ),
                ]),
                ["track_created", "track_created_authority"],
                false,
            ),
        ),
        (
            "open_section".to_owned(),
            operation(
                "open_section",
                "Open a section turn",
                "Declare timing, mapping, abstraction, participants, and an optional public reveal policy.",
                TabletopCapability::Scenes,
                object_type([
                    (
                        "abstraction",
                        required(section_abstraction_type(), "Character-control scale."),
                    ),
                    ("id", required(string(1, 128), "Stable section identity.")),
                    ("label", required(string(1, 160), "Section display label.")),
                    (
                        "mapping",
                        required(section_mapping_type(), "Map participation mode."),
                    ),
                    (
                        "participants",
                        required(list(string(1, 128), 1, 64), "Stable participant order."),
                    ),
                    (
                        "reveal_after_resolution",
                        required(
                            TypeExpression::Bool,
                            "Reveal actions only after resolution.",
                        ),
                    ),
                    (
                        "timing",
                        required(section_timing_type(), "Turn timing mode."),
                    ),
                    (
                        "turn_ticks",
                        optional(
                            integer(1.0, 1_000_000.0),
                            "Required only for timed sections.",
                        ),
                    ),
                ]),
                ["section_opened"],
                false,
            ),
        ),
        (
            "preview_probability".to_owned(),
            operation(
                "preview_probability",
                "Preview exact success probability",
                "Return rational support/opposition weights and a deterministic basis-point display without consuming entropy.",
                TabletopCapability::ChecksAndConflicts,
                check_request_type(),
                ["probability_previewed", "probability_previewed_authority"],
                false,
            ),
        ),
        (
            "recall_memory".to_owned(),
            operation(
                "recall_memory",
                "Resolve prior experience",
                "Test a bounded topic obscurity, applying the declared connection adjustment before one deterministic draw.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    (
                        "connected",
                        required(TypeExpression::Bool, "Prior topic connection."),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Memory visibility."),
                    ),
                    ("id", required(string(1, 128), "Stable memory identity.")),
                    ("obscurity", required(integer(1.0, 9.0), "Topic obscurity.")),
                    ("topic", required(string(1, 240), "Memory topic.")),
                ]),
                [
                    "entropy_trace",
                    "memory_resolved",
                    "memory_resolved_authority",
                ],
                true,
            ),
        ),
        (
            "resolve_check".to_owned(),
            operation(
                "resolve_check",
                "Resolve a host-authoritative check",
                "Draw uniformly over the nonzero support/opposition weight domain and calculate an exact signed magnitude.",
                TabletopCapability::ChecksAndConflicts,
                check_request_type(),
                [
                    "check_resolved",
                    "check_resolved_authority",
                    "entropy_trace",
                ],
                true,
            ),
        ),
        (
            "resolve_section".to_owned(),
            operation(
                "resolve_section",
                "Resolve simultaneous submissions",
                "Require the applicable submission set, order it by declared participants, and retain authority outcomes atomically.",
                TabletopCapability::Scenes,
                object_type([
                    (
                        "outcomes",
                        required(
                            list(
                                object_type([
                                    (
                                        "submission_id",
                                        required(string(1, 128), "Submitted action identity."),
                                    ),
                                    (
                                        "summary",
                                        required(string(1, 1_024), "Authority outcome summary."),
                                    ),
                                ]),
                                1,
                                64,
                            ),
                            "One outcome per submitted action.",
                        ),
                    ),
                    (
                        "public_summary",
                        required(string(1, 1_024), "Public section-level result."),
                    ),
                    (
                        "section_id",
                        required(string(1, 128), "Open section identity."),
                    ),
                ]),
                ["section_resolved", "section_resolved_authority"],
                false,
            ),
        ),
        (
            "submit_action".to_owned(),
            operation(
                "submit_action",
                "Submit one simultaneous action",
                "Store one participant action privately while publishing only receipt progress until resolution.",
                TabletopCapability::Scenes,
                object_type([
                    ("action", required(string(1, 2_048), "Fictional action.")),
                    (
                        "map_intent",
                        optional(string(1, 512), "Mapped position or movement intent."),
                    ),
                    (
                        "participant_id",
                        required(string(1, 128), "Submitting participant identity."),
                    ),
                    (
                        "section_id",
                        required(string(1, 128), "Open section identity."),
                    ),
                    (
                        "submission_id",
                        required(string(1, 128), "Stable submission identity."),
                    ),
                ]),
                ["submission_recorded", "submission_recorded_authority"],
                false,
            ),
        ),
        (
            "timeout_section".to_owned(),
            operation(
                "timeout_section",
                "Time out a section turn",
                "Close a timed section after its duration and deterministically identify submitted and missing participants.",
                TabletopCapability::Scenes,
                object_type([
                    (
                        "elapsed_ticks",
                        required(integer(1.0, 1_000_000.0), "Authority timing units elapsed."),
                    ),
                    (
                        "section_id",
                        required(string(1, 128), "Open timed section identity."),
                    ),
                ]),
                ["section_timed_out", "section_timed_out_authority"],
                false,
            ),
        ),
    ])
}

fn event_types() -> BTreeMap<String, TypeExpression> {
    let public_probability = || {
        object_type([
            ("label", required(string(1, 240), "Public check label.")),
            (
                "opposition_total",
                optional(
                    integer(1.0, f64::from(MAX_MODIFIER_TOTAL)),
                    "Revealed opposition.",
                ),
            ),
            (
                "success_basis_points_floor",
                optional(integer(0.0, 10_000.0), "Floored success basis points."),
            ),
            (
                "support_total",
                optional(
                    integer(1.0, f64::from(MAX_MODIFIER_TOTAL)),
                    "Revealed support.",
                ),
            ),
        ])
    };
    let authority_probability = || {
        object_type([
            (
                "disclosure",
                required(disclosure_type(), "Declared result visibility."),
            ),
            (
                "favorable_weight",
                required(integer(1.0, 1_000_000_000_000.0), "Positive result count."),
            ),
            ("label", required(string(1, 240), "Authority check label.")),
            (
                "opposition",
                required(list(named("Contribution"), 1, 256), "Opposition detail."),
            ),
            (
                "opposition_total",
                required(
                    integer(1.0, f64::from(MAX_MODIFIER_TOTAL)),
                    "Opposition sum.",
                ),
            ),
            (
                "success_basis_points_floor",
                required(integer(0.0, 10_000.0), "Floored success basis points."),
            ),
            (
                "support",
                required(list(named("Contribution"), 1, 256), "Support detail."),
            ),
            (
                "support_total",
                required(integer(1.0, f64::from(MAX_MODIFIER_TOTAL)), "Support sum."),
            ),
            (
                "total_weight",
                required(integer(2.0, 2_000_000_000_000.0), "Nonzero result count."),
            ),
            (
                "unfavorable_weight",
                required(integer(1.0, 1_000_000_000_000.0), "Negative result count."),
            ),
        ])
    };
    let track_public = || {
        object_type([
            (
                "completed",
                required(TypeExpression::Bool, "Completion flag."),
            ),
            ("id", required(string(1, 128), "Track identity.")),
            ("label", required(string(1, 160), "Track label.")),
            (
                "progress_after",
                required(integer(0.0, 1_000_000.0), "Progress after the event."),
            ),
            (
                "progress_before",
                required(integer(0.0, 1_000_000.0), "Progress before the event."),
            ),
            (
                "target_count",
                required(integer(1.0, 1_000_000.0), "Completion target."),
            ),
        ])
    };
    let submission_authority = || {
        object_type([
            (
                "action",
                required(string(1, 2_048), "Private submitted action."),
            ),
            (
                "map_intent",
                required(string(0, 512), "Private mapped intent."),
            ),
            (
                "participant_id",
                required(string(1, 128), "Participant identity."),
            ),
            ("section_id", required(string(1, 128), "Section identity.")),
            (
                "submission_id",
                required(string(1, 128), "Submission identity."),
            ),
        ])
    };
    BTreeMap::from([
        (
            "check_resolved".to_owned(),
            object_type([
                ("label", required(string(1, 240), "Public check label.")),
                (
                    "magnitude",
                    required(integer(-1_000_000.0, 1_000_000.0), "Signed margin."),
                ),
                (
                    "opposition_total",
                    optional(
                        integer(1.0, f64::from(MAX_MODIFIER_TOTAL)),
                        "Revealed opposition.",
                    ),
                ),
                (
                    "outcome",
                    required(track_outcome_type(), "Success or failure."),
                ),
                (
                    "success_basis_points_floor",
                    optional(integer(0.0, 10_000.0), "Floored success basis points."),
                ),
                (
                    "support_total",
                    optional(
                        integer(1.0, f64::from(MAX_MODIFIER_TOTAL)),
                        "Revealed support.",
                    ),
                ),
            ]),
        ),
        (
            "check_resolved_authority".to_owned(),
            extend_object_type(
                authority_probability(),
                [
                    (
                        "draw_index",
                        required(
                            integer(0.0, 1_999_999_999_999.0),
                            "Uniform result-domain index.",
                        ),
                    ),
                    (
                        "magnitude",
                        required(integer(-1_000_000.0, 1_000_000.0), "Signed margin."),
                    ),
                    (
                        "outcome",
                        required(track_outcome_type(), "Success or failure."),
                    ),
                    (
                        "signed_result",
                        required(
                            integer(-1_000_000_000_000.0, 1_000_000_000_000.0),
                            "Raw nonzero result.",
                        ),
                    ),
                ],
            ),
        ),
        (
            "entropy_trace".to_owned(),
            object_type([
                (
                    "cursor_after",
                    required(
                        integer(1.0, 9_007_199_254_740_991.0),
                        "Entropy cursor after one draw.",
                    ),
                ),
                (
                    "cursor_before",
                    required(
                        integer(0.0, 9_007_199_254_740_990.0),
                        "Entropy cursor before one draw.",
                    ),
                ),
                (
                    "draw_index",
                    required(integer(0.0, 1_999_999_999_999.0), "Bounded draw index."),
                ),
            ]),
        ),
        (
            "memory_resolved".to_owned(),
            object_type([
                (
                    "has_experience",
                    required(TypeExpression::Bool, "Public memory result."),
                ),
                ("id", required(string(1, 128), "Memory identity.")),
                ("topic", required(string(1, 240), "Memory topic.")),
            ]),
        ),
        (
            "memory_resolved_authority".to_owned(),
            object_type([
                (
                    "connected",
                    required(TypeExpression::Bool, "Connection adjustment flag."),
                ),
                (
                    "draw",
                    required(integer(1.0, 10.0), "Authority d10 result."),
                ),
                (
                    "effective_obscurity",
                    required(integer(0.0, 9.0), "Adjusted comparison value."),
                ),
                (
                    "has_experience",
                    required(TypeExpression::Bool, "Memory result."),
                ),
                ("id", required(string(1, 128), "Memory identity.")),
                (
                    "obscurity",
                    required(integer(1.0, 9.0), "Original obscurity."),
                ),
                ("topic", required(string(1, 240), "Memory topic.")),
            ]),
        ),
        ("probability_previewed".to_owned(), public_probability()),
        (
            "probability_previewed_authority".to_owned(),
            authority_probability(),
        ),
        (
            "section_opened".to_owned(),
            object_type([
                (
                    "abstraction",
                    required(section_abstraction_type(), "Control scale."),
                ),
                ("id", required(string(1, 128), "Section identity.")),
                ("label", required(string(1, 160), "Section label.")),
                ("mapping", required(section_mapping_type(), "Map mode.")),
                (
                    "participants",
                    required(list(string(1, 128), 1, 64), "Participant order."),
                ),
                ("timing", required(section_timing_type(), "Timing mode.")),
                (
                    "turn_ticks",
                    required(integer(0.0, 1_000_000.0), "Zero or timed duration."),
                ),
            ]),
        ),
        (
            "section_resolved".to_owned(),
            object_type([
                (
                    "public_summary",
                    required(string(1, 1_024), "Public result summary."),
                ),
                (
                    "resolved_order",
                    required(
                        list(string(1, 128), 1, 64),
                        "Deterministic participant order.",
                    ),
                ),
                (
                    "revealed_submissions",
                    optional(
                        list(named("SubmissionOutcome"), 1, 64),
                        "Actions explicitly revealed after resolution.",
                    ),
                ),
                ("section_id", required(string(1, 128), "Section identity.")),
            ]),
        ),
        (
            "section_resolved_authority".to_owned(),
            object_type([
                (
                    "ordered_submissions",
                    required(
                        list(named("SubmissionOutcome"), 1, 64),
                        "Private actions and outcomes in resolution order.",
                    ),
                ),
                (
                    "public_summary",
                    required(string(1, 1_024), "Public result summary."),
                ),
                ("section_id", required(string(1, 128), "Section identity.")),
            ]),
        ),
        (
            "section_timed_out".to_owned(),
            object_type([
                (
                    "elapsed_ticks",
                    required(integer(1.0, 1_000_000.0), "Elapsed timing units."),
                ),
                (
                    "missing_participants",
                    required(
                        list(string(1, 128), 0, 64),
                        "Participants without submissions.",
                    ),
                ),
                ("section_id", required(string(1, 128), "Section identity.")),
                (
                    "submitted_participants",
                    required(
                        list(string(1, 128), 0, 64),
                        "Participants with submissions.",
                    ),
                ),
            ]),
        ),
        (
            "section_timed_out_authority".to_owned(),
            object_type([
                (
                    "elapsed_ticks",
                    required(integer(1.0, 1_000_000.0), "Elapsed timing units."),
                ),
                (
                    "missing_participants",
                    required(
                        list(string(1, 128), 0, 64),
                        "Participants without submissions.",
                    ),
                ),
                ("section_id", required(string(1, 128), "Section identity.")),
                (
                    "submissions",
                    required(
                        list(submission_authority(), 0, 64),
                        "Private submissions received before timeout.",
                    ),
                ),
            ]),
        ),
        (
            "submission_cancelled".to_owned(),
            object_type([
                (
                    "participant_id",
                    required(string(1, 128), "Participant identity."),
                ),
                ("section_id", required(string(1, 128), "Section identity.")),
                (
                    "submitted_count",
                    required(integer(0.0, 64.0), "Remaining submission count."),
                ),
            ]),
        ),
        (
            "submission_cancelled_authority".to_owned(),
            submission_authority(),
        ),
        (
            "submission_recorded".to_owned(),
            object_type([
                (
                    "participant_id",
                    required(string(1, 128), "Participant identity."),
                ),
                (
                    "required_count",
                    required(integer(1.0, 64.0), "Participant count."),
                ),
                ("section_id", required(string(1, 128), "Section identity.")),
                (
                    "submitted_count",
                    required(integer(1.0, 64.0), "Current submission count."),
                ),
            ]),
        ),
        (
            "submission_recorded_authority".to_owned(),
            submission_authority(),
        ),
        ("track_advanced".to_owned(), track_public()),
        (
            "track_advanced_authority".to_owned(),
            object_type([
                (
                    "completed",
                    required(TypeExpression::Bool, "Completion flag."),
                ),
                (
                    "consequence",
                    required(string(1, 512), "Completion consequence."),
                ),
                ("id", required(string(1, 128), "Track identity.")),
                ("label", required(string(1, 160), "Track label.")),
                (
                    "outcome",
                    required(track_outcome_type(), "Applied outcome."),
                ),
                (
                    "progress_after",
                    required(integer(0.0, 1_000_000.0), "Progress after."),
                ),
                (
                    "progress_before",
                    required(integer(0.0, 1_000_000.0), "Progress before."),
                ),
                (
                    "secret",
                    required(TypeExpression::Bool, "Authority-only flag."),
                ),
                (
                    "target_count",
                    required(integer(1.0, 1_000_000.0), "Completion target."),
                ),
            ]),
        ),
        (
            "track_completed".to_owned(),
            object_type([
                (
                    "consequence",
                    required(string(1, 512), "Public completion consequence."),
                ),
                ("id", required(string(1, 128), "Track identity.")),
            ]),
        ),
        (
            "track_completed_authority".to_owned(),
            object_type([
                (
                    "consequence",
                    required(string(1, 512), "Completion consequence."),
                ),
                ("id", required(string(1, 128), "Track identity.")),
                (
                    "secret",
                    required(TypeExpression::Bool, "Authority-only flag."),
                ),
            ]),
        ),
        (
            "track_created".to_owned(),
            object_type([
                (
                    "advances_on",
                    required(track_outcome_type(), "Outcome that increments progress."),
                ),
                ("id", required(string(1, 128), "Track identity.")),
                (
                    "interval",
                    required(string(1, 240), "Time or event trigger."),
                ),
                ("label", required(string(1, 160), "Track label.")),
                (
                    "target_count",
                    required(integer(1.0, 1_000_000.0), "Completion target."),
                ),
            ]),
        ),
        (
            "track_created_authority".to_owned(),
            object_type([
                (
                    "advances_on",
                    required(track_outcome_type(), "Outcome that increments progress."),
                ),
                (
                    "consequence",
                    required(string(1, 512), "Completion consequence."),
                ),
                ("id", required(string(1, 128), "Track identity.")),
                (
                    "interval",
                    required(string(1, 240), "Time or event trigger."),
                ),
                ("label", required(string(1, 160), "Track label.")),
                (
                    "secret",
                    required(TypeExpression::Bool, "Authority-only flag."),
                ),
                (
                    "target_count",
                    required(integer(1.0, 1_000_000.0), "Completion target."),
                ),
            ]),
        ),
    ])
}

fn capability(capability: TabletopCapability) -> TabletopCapabilityDeclaration {
    TabletopCapabilityDeclaration {
        capability,
        version: 1,
    }
}

fn operation<const N: usize>(
    id: &str,
    title: &str,
    description: &str,
    required_capability: TabletopCapability,
    request_type: TypeExpression,
    event_kinds: [&str; N],
    consumes_entropy: bool,
) -> ResolverOperationDeclaration {
    ResolverOperationDeclaration {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        required_capability,
        request_type,
        event_kinds: event_kinds.into_iter().map(str::to_owned).collect(),
        consumes_entropy,
    }
}

fn creation_field(
    id: &str,
    label: &str,
    value_type: TypeExpression,
    required: bool,
) -> CreationField {
    CreationField {
        id: id.to_owned(),
        label: label.to_owned(),
        description: format!("Explicit adapter-owned {label} input."),
        value_type,
        required,
        authority: CreationFieldAuthority::AdapterOwned,
    }
}

fn required(value_type: TypeExpression, description: &str) -> FieldDeclaration {
    field(value_type, true, description)
}

fn optional(value_type: TypeExpression, description: &str) -> FieldDeclaration {
    field(value_type, false, description)
}

fn field(value_type: TypeExpression, required: bool, description: &str) -> FieldDeclaration {
    FieldDeclaration {
        value_type,
        required,
        description: description.to_owned(),
    }
}

fn integer(minimum: f64, maximum: f64) -> TypeExpression {
    TypeExpression::Number {
        integer: true,
        minimum: Some(minimum),
        maximum: Some(maximum),
    }
}

fn string(min_length: usize, max_length: usize) -> TypeExpression {
    TypeExpression::String {
        min_length,
        max_length,
    }
}

fn symbol<const N: usize>(values: [&str; N]) -> TypeExpression {
    TypeExpression::Symbol {
        values: values.into_iter().map(str::to_owned).collect(),
    }
}

fn list(items: TypeExpression, min_items: usize, max_items: usize) -> TypeExpression {
    TypeExpression::List {
        items: Box::new(items),
        min_items,
        max_items,
    }
}

fn map(values: TypeExpression, min_entries: usize, max_entries: usize) -> TypeExpression {
    TypeExpression::Map {
        values: Box::new(values),
        min_entries,
        max_entries,
    }
}

fn named(name: &str) -> TypeExpression {
    TypeExpression::Named {
        name: name.to_owned(),
    }
}

fn object_type<const N: usize>(fields: [(&str, FieldDeclaration); N]) -> TypeExpression {
    TypeExpression::Object {
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    }
}

fn extend_object_type<const N: usize>(
    value_type: TypeExpression,
    additional: [(&str, FieldDeclaration); N],
) -> TypeExpression {
    match value_type {
        TypeExpression::Object { mut fields } => {
            fields.extend(
                additional
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value)),
            );
            TypeExpression::Object { fields }
        }
        _ => value_type,
    }
}

fn disclosure_type() -> TypeExpression {
    symbol(["host_only", "public"])
}

fn track_outcome_type() -> TypeExpression {
    symbol(["failure", "success"])
}

fn section_timing_type() -> TypeExpression {
    symbol(["timed", "untimed"])
}

fn section_mapping_type() -> TypeExpression {
    symbol(["mapped", "unmapped"])
}

fn section_abstraction_type() -> TypeExpression {
    symbol(["automatic", "objective", "strategic"])
}

fn section_status_type() -> TypeExpression {
    symbol(["open", "resolved", "timed_out"])
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn value_object(
    path: impl Into<String>,
    value: &DomainValue,
) -> Result<&BTreeMap<String, DomainValue>, TabletopError> {
    match value {
        DomainValue::Object(fields) => Ok(fields),
        _ => Err(invalid(path, "expected an object")),
    }
}

fn value_object_mut(
    path: impl Into<String>,
    value: &mut DomainValue,
) -> Result<&mut BTreeMap<String, DomainValue>, TabletopError> {
    match value {
        DomainValue::Object(fields) => Ok(fields),
        _ => Err(invalid(path, "expected an object")),
    }
}

fn object_field<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a DomainValue, TabletopError> {
    fields
        .get(field)
        .ok_or_else(|| invalid(field, "required field is missing"))
}

fn object_bool(fields: &BTreeMap<String, DomainValue>, field: &str) -> Result<bool, TabletopError> {
    match object_field(fields, field)? {
        DomainValue::Bool(value) => Ok(*value),
        _ => Err(invalid(field, "expected a boolean")),
    }
}

fn object_string<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, TabletopError> {
    match object_field(fields, field)? {
        DomainValue::String(value) => Ok(value),
        _ => Err(invalid(field, "expected a string")),
    }
}

fn optional_string<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<Option<&'a str>, TabletopError> {
    match fields.get(field) {
        Some(DomainValue::String(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(field, "expected a string")),
        None => Ok(None),
    }
}

fn object_symbol<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, TabletopError> {
    match object_field(fields, field)? {
        DomainValue::Symbol(value) => Ok(value),
        _ => Err(invalid(field, "expected a symbol")),
    }
}

fn object_list<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a Vec<DomainValue>, TabletopError> {
    match object_field(fields, field)? {
        DomainValue::List(values) => Ok(values),
        _ => Err(invalid(field, "expected a list")),
    }
}

fn object_map<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a BTreeMap<String, DomainValue>, TabletopError> {
    value_object(field, object_field(fields, field)?)
}

fn object_map_mut<'a>(
    fields: &'a mut BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a mut BTreeMap<String, DomainValue>, TabletopError> {
    match fields.get_mut(field) {
        Some(DomainValue::Object(values)) => Ok(values),
        Some(_) => Err(invalid(field, "expected an object")),
        None => Err(invalid(field, "required field is missing")),
    }
}

fn object_u32(fields: &BTreeMap<String, DomainValue>, field: &str) -> Result<u32, TabletopError> {
    let DomainValue::Number(number) = object_field(fields, field)? else {
        return Err(invalid(field, "expected an unsigned integer"));
    };
    if !number.is_finite()
        || number.fract() != 0.0
        || *number < 0.0
        || *number > f64::from(u32::MAX)
    {
        return Err(invalid(field, "expected a finite 32-bit unsigned integer"));
    }
    Ok(*number as u32)
}

fn object_i32(fields: &BTreeMap<String, DomainValue>, field: &str) -> Result<i32, TabletopError> {
    let DomainValue::Number(number) = object_field(fields, field)? else {
        return Err(invalid(field, "expected an integer"));
    };
    if !number.is_finite()
        || number.fract() != 0.0
        || *number < f64::from(i32::MIN)
        || *number > f64::from(i32::MAX)
    {
        return Err(invalid(field, "expected a finite 32-bit integer"));
    }
    Ok(*number as i32)
}

fn optional_u32(
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<Option<u32>, TabletopError> {
    match fields.get(field) {
        None => Ok(None),
        Some(DomainValue::Number(number))
            if number.is_finite()
                && number.fract() == 0.0
                && *number >= 0.0
                && *number <= f64::from(u32::MAX) =>
        {
            Ok(Some(*number as u32))
        }
        Some(_) => Err(invalid(field, "expected a finite 32-bit unsigned integer")),
    }
}

fn set_u32(fields: &mut BTreeMap<String, DomainValue>, field: &str, value: u32) {
    fields.insert(field.to_owned(), DomainValue::Number(f64::from(value)));
}

fn string_values(path: &str, values: &[DomainValue]) -> Result<Vec<String>, TabletopError> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| match value {
            DomainValue::String(value) => Ok(value.clone()),
            _ => Err(invalid(format!("{path}[{index}]"), "expected a string")),
        })
        .collect()
}

fn string_list_value(values: &[String]) -> DomainValue {
    DomainValue::List(
        values
            .iter()
            .map(|value| DomainValue::String(value.clone()))
            .collect(),
    )
}

fn public_event(kind: &str, payload: DomainValue) -> ResolverEvent {
    ResolverEvent {
        kind: kind.to_owned(),
        visibility: EventVisibility::Public,
        payload,
    }
}

fn authority_event(kind: &str, payload: DomainValue) -> ResolverEvent {
    ResolverEvent {
        kind: kind.to_owned(),
        visibility: EventVisibility::HostOnly,
        payload,
    }
}

fn validate_bounded_text(
    path: impl Into<String>,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), TabletopError> {
    if !(minimum..=maximum).contains(&value.chars().count()) {
        return Err(invalid(path, "text length is outside allowed bounds"));
    }
    Ok(())
}

fn validate_sha256(path: &str, value: &str) -> Result<(), TabletopError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(path, "expected a lowercase SHA-256 fingerprint"));
    }
    Ok(())
}

fn validate_global_identifier(path: &str, value: &str) -> Result<(), TabletopError> {
    if value.len() > 255
        || value.split('.').count() < 3
        || value
            .split('.')
            .any(|segment| !valid_local_identifier(segment))
    {
        return Err(invalid(path, "expected a namespaced lowercase identifier"));
    }
    Ok(())
}

fn validate_local_identifier(path: &str, value: &str) -> Result<(), TabletopError> {
    if !valid_local_identifier(value) {
        return Err(invalid(path, "expected a lowercase identifier"));
    }
    Ok(())
}

fn valid_local_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && value.len() <= 128
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.ends_with('_')
        && !value.contains("__")
}

fn invalid(path: impl Into<String>, reason: &'static str) -> TabletopError {
    TabletopError::InvalidField {
        path: path.into(),
        reason,
    }
}
