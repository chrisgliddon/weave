//! Independently authored adapter for The Spark Roleplaying Game core mechanics.
//!
//! The adapter implements identity and concept, three Beliefs, relationships, Body/Heart/Mind/
//! Spark step-die attributes, Broad/Common/Deep Talents, declarations, supporters, Influence,
//! Harm by attribute, competitive conflicts, escalation, victory costs, post-roll boosts, scene
//! closing with Belief review, and recovery. Fixtures are synthetic and authored for this
//! repository; no upstream rule text, setting prose, logo, artwork, font, or sheet layout is
//! redistributed.
//!
//! ## Public-source record
//!
//! [`SPARK_SOURCE_RECORD`] is the checked-in text record that states the canonical URL, creator,
//! publisher, edition, and CC0-1.0 licence. [`SPARK_SOURCE_RECORD_SHA256`] is the SHA-256 of that
//! exact UTF-8 text, and the manifest provenance uses that digest. The digest does not claim to
//! hash an upstream download: no upstream file bytes were acquired for this module. Only core
//! mechanics are modelled; example settings, logos, artwork, fonts, and presentation are
//! excluded.

use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, FieldDeclaration, TypeExpression, validate_typed_value};

use crate::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition,
    AdapterExtensionSurface, AdapterManifest, AdapterProvenance, AdapterSourceClass, CreationField,
    CreationFieldAuthority, CreationStep, EntropyState, EntropyStream, EventVisibility,
    ExcludedMaterial, HostVisibilityPolicy, LicenseTextReference, RESOLVER_CONTRACT_VERSION,
    ResolvedAdapter, ResolverEvent, ResolverOperationDeclaration, ResolverOutput,
    TabletopCapability, TabletopCapabilityDeclaration, TabletopError, TabletopResolver,
    TabletopState, canonical_fingerprint, resolved_adapter, validate_tabletop_state,
};

/// Stable adapter identity shared by source, editor, runtime, and host integrations.
pub const SPARK_ADAPTER_ID: &str = "org.weave.tabletop.spark";
/// First independently modelled adapter release for the Spark core rules.
pub const SPARK_ADAPTER_VERSION: &str = "1.0.0";
/// Versioned character creation request and preview format.
pub const SPARK_CREATION_FORMAT_VERSION: u32 = 1;
/// Canonical public page for The Spark Roleplaying Game.
pub const SPARK_SOURCE_URL: &str = "https://www.genesisoflegend.com/the-spark-rpg/";
/// Checked-in text record whose SHA-256 is recorded by the manifest.
pub const SPARK_SOURCE_RECORD: &str = concat!(
    "Spark Roleplaying Game source record\n",
    "Canonical URL: https://www.genesisoflegend.com/the-spark-rpg/\n",
    "Creator: Jason Pitre\n",
    "Publisher: Genesis of Legend Publishing\n",
    "Edition: Spark Roleplaying Game core rules\n",
    "Licence: CC0-1.0\n",
    "Covered scope: core mechanics only\n",
    "Excluded scope: example settings, logos, artwork, fonts, and presentation\n",
    "Digest note: this record is the checked-in source text; no upstream file bytes were downloaded.\n",
);
/// SHA-256 of [`SPARK_SOURCE_RECORD`] as exact UTF-8 bytes.
pub const SPARK_SOURCE_RECORD_SHA256: &str =
    "3eacf5d0eb87873af6e5974bcc0d863c641c35ecbe1fef2d810315000fa3ec46";
/// SHA-256 of the official plain-text CC0 1.0 legal code.
pub const SPARK_CC0_1_0_LEGAL_CODE_SHA256: &str =
    "a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499";

/// Adapter manifest artifact consumed by the shared tabletop registry.
pub type SparkManifest = AdapterManifest;
/// Adapter-owned mutable state artifact for one Spark character.
pub type SparkState = TabletopState;
/// One structured event returned by the trusted Spark resolver.
pub type SparkEvent = ResolverEvent;
/// Redaction-safe failure type shared with the tabletop contract.
pub type SparkError = TabletopError;

const MAX_INFLUENCE: u32 = 99;
const MAX_HARM: u32 = 4;
const MAX_ESCALATION: u32 = 3;
const MAX_BOOST: u32 = 16;
const MAX_COST_AMOUNT: u32 = 16;
const MAX_TOTAL: u32 = 128;
const MAX_BELIEFS: usize = 3;
const MAX_RELATIONSHIPS: usize = 64;
const MAX_TALENTS: usize = 64;
const MAX_DECLARATIONS: usize = 16;
const MAX_SUPPORTERS: usize = 8;
const MAX_CONFLICTS: usize = 32;
const MAX_SCENES: usize = 32;

/// One of the four Spark attributes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkAttribute {
    Body,
    Heart,
    Mind,
    Spark,
}

impl SparkAttribute {
    /// Every attribute in canonical order.
    pub const ALL: [Self; 4] = [Self::Body, Self::Heart, Self::Mind, Self::Spark];

    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Heart => "heart",
            Self::Mind => "mind",
            Self::Spark => "spark",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "body" => Some(Self::Body),
            "heart" => Some(Self::Heart),
            "mind" => Some(Self::Mind),
            "spark" => Some(Self::Spark),
            _ => None,
        }
    }
}

/// Step die used by one attribute.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkStep {
    D4,
    D6,
    D8,
    D10,
    D12,
}

impl SparkStep {
    /// Zero-based step rank, where d4 is zero and d12 is four.
    #[must_use]
    pub const fn rank(self) -> u32 {
        match self {
            Self::D4 => 0,
            Self::D6 => 1,
            Self::D8 => 2,
            Self::D10 => 3,
            Self::D12 => 4,
        }
    }

    /// Step at a rank, clamped to the d4..d12 range.
    #[must_use]
    pub const fn from_rank(rank: u32) -> Self {
        match rank {
            0 => Self::D4,
            1 => Self::D6,
            2 => Self::D8,
            3 => Self::D10,
            _ => Self::D12,
        }
    }

    /// Number of faces on the die.
    #[must_use]
    pub const fn sides(self) -> u64 {
        match self {
            Self::D4 => 4,
            Self::D6 => 6,
            Self::D8 => 8,
            Self::D10 => 10,
            Self::D12 => 12,
        }
    }

    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::D4 => "d4",
            Self::D6 => "d6",
            Self::D8 => "d8",
            Self::D10 => "d10",
            Self::D12 => "d12",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "d4" => Some(Self::D4),
            "d6" => Some(Self::D6),
            "d8" => Some(Self::D8),
            "d10" => Some(Self::D10),
            "d12" => Some(Self::D12),
            _ => None,
        }
    }
}

/// Tier of one Talent. The tier supplies the deterministic flat bonus.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkTalentTier {
    Broad,
    Common,
    Deep,
}

impl SparkTalentTier {
    /// Flat bonus granted by the tier: Broad 1, Common 2, Deep 3.
    #[must_use]
    pub const fn bonus(self) -> u32 {
        match self {
            Self::Broad => 1,
            Self::Common => 2,
            Self::Deep => 3,
        }
    }

    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Broad => "broad",
            Self::Common => "common",
            Self::Deep => "deep",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "broad" => Some(Self::Broad),
            "common" => Some(Self::Common),
            "deep" => Some(Self::Deep),
            _ => None,
        }
    }
}

/// Review state of one Belief.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkBeliefStatus {
    Confirmed,
    Held,
    Refuted,
}

impl SparkBeliefStatus {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Held => "held",
            Self::Refuted => "refuted",
        }
    }
}

/// Lifecycle of one conflict.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkConflictStatus {
    Open,
    Resolved,
    Rolled,
}

impl SparkConflictStatus {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Rolled => "rolled",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "resolved" => Some(Self::Resolved),
            "rolled" => Some(Self::Rolled),
            _ => None,
        }
    }
}

/// Kind of explicit post-roll boost.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkBoostKind {
    Harm,
    Influence,
}

impl SparkBoostKind {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Harm => "harm",
            Self::Influence => "influence",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "harm" => Some(Self::Harm),
            "influence" => Some(Self::Influence),
            _ => None,
        }
    }
}

/// How a closing scene changes Influence.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkInfluenceMode {
    Award,
    Refresh,
}

impl SparkInfluenceMode {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Award => "award",
            Self::Refresh => "refresh",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "award" => Some(Self::Award),
            "refresh" => Some(Self::Refresh),
            _ => None,
        }
    }
}

/// Verdict recorded by one closing-scene Belief review.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkBeliefVerdict {
    Confirmed,
    Refuted,
}

impl SparkBeliefVerdict {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Refuted => "refuted",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "confirmed" => Some(Self::Confirmed),
            "refuted" => Some(Self::Refuted),
            _ => None,
        }
    }
}

/// Kind of cost charged to the winning declaration.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkCostKind {
    Harm,
    Influence,
}

impl SparkCostKind {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Harm => "harm",
            Self::Influence => "influence",
        }
    }

    fn from_symbol(value: &str) -> Option<Self> {
        match value {
            "harm" => Some(Self::Harm),
            "influence" => Some(Self::Influence),
            _ => None,
        }
    }
}

/// One authored Belief.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkBeliefSeed {
    pub id: String,
    pub statement: String,
}

/// One authored relationship. Relationships can support declarations during play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkRelationshipSeed {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// One authored starting attribute step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkAttributeSeed {
    pub attribute: SparkAttribute,
    pub step: SparkStep,
}

/// One authored tiered Talent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkTalentSeed {
    pub id: String,
    pub name: String,
    pub attribute: SparkAttribute,
    pub tier: SparkTalentTier,
}

/// Complete, versioned input to deterministic Spark character creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkCreationRequest {
    pub creation_format_version: u32,
    pub character_id: String,
    pub name: String,
    pub concept: String,
    pub beliefs: Vec<SparkBeliefSeed>,
    pub relationships: Vec<SparkRelationshipSeed>,
    pub attributes: Vec<SparkAttributeSeed>,
    pub talents: Vec<SparkTalentSeed>,
    pub starting_influence: u32,
}

/// Explainable result of creation before projection/state export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkCreationPreview {
    pub creation_format_version: u32,
    pub request_sha256: String,
    pub adapter: ResolvedAdapter,
    pub definition: AdapterCharacterDefinition,
    pub initial_state: TabletopState,
    pub explanations: Vec<String>,
}

macro_rules! impl_spark_io {
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

impl_spark_io!(SparkCreationRequest);
impl_spark_io!(SparkCreationPreview);

/// Canonical JSON Schema for Spark creation requests.
pub fn spark_creation_request_schema() -> Result<String, TabletopError> {
    spark_schema::<SparkCreationRequest>(
        "urn:weave:schema:tabletop-spark-creation-request:1",
        "Weave Spark Creation Request v1",
        Some(("creation_format_version", SPARK_CREATION_FORMAT_VERSION)),
    )
}

/// Canonical JSON Schema for explainable Spark creation previews.
pub fn spark_creation_preview_schema() -> Result<String, TabletopError> {
    spark_schema::<SparkCreationPreview>(
        "urn:weave:schema:tabletop-spark-creation-preview:1",
        "Weave Spark Creation Preview v1",
        Some(("creation_format_version", SPARK_CREATION_FORMAT_VERSION)),
    )
}

fn spark_schema<T: JsonSchema>(
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

/// Validate and deterministically create one Spark character.
pub fn create_spark_character(
    request: &SparkCreationRequest,
) -> Result<SparkCreationPreview, TabletopError> {
    validate_creation_request(request)?;
    let manifest = spark_manifest();
    crate::validate_adapter_manifest(&manifest)?;
    let adapter = resolved_adapter(&manifest)?;
    let definition_value = creation_definition_value(request);
    let definition_sha256 = canonical_fingerprint(&definition_value)?;
    let definition = AdapterCharacterDefinition {
        adapter: adapter.clone(),
        definition: definition_value,
        definition_sha256: definition_sha256.clone(),
        completed_creation_steps: vec![
            "attributes".to_owned(),
            "beliefs".to_owned(),
            "identity".to_owned(),
            "relationships".to_owned(),
            "talents".to_owned(),
        ],
    };
    let initial_state = TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter,
        owner_id: request.character_id.clone(),
        definition_sha256,
        revision: 0,
        entropy: EntropyState {
            algorithm: "sha256_counter_v1".to_owned(),
            seed: 0,
            cursor: 0,
        },
        value: initial_state_value(request),
    };
    validate_tabletop_state(&initial_state, &manifest)?;
    let preview = SparkCreationPreview {
        creation_format_version: SPARK_CREATION_FORMAT_VERSION,
        request_sha256: canonical_fingerprint(request)?,
        adapter: resolved_adapter(&manifest)?,
        definition,
        initial_state,
        explanations: vec![
            "Attributes are authored step dice; Harm reduces the effective step during play."
                .to_owned(),
            "Every Belief, relationship, and Talent is an explicit typed authoring value."
                .to_owned(),
            "Creation consumes no entropy; conflict rolls begin at the saved stream cursor."
                .to_owned(),
            "Tabletop data remains adapter-owned and cannot write back into canonical Character evidence."
                .to_owned(),
        ],
    };
    validate_spark_creation_preview(&preview)?;
    Ok(preview)
}

/// Revalidate a creation preview and all of its cross-artifact fingerprints.
pub fn validate_spark_creation_preview(
    preview: &SparkCreationPreview,
) -> Result<(), TabletopError> {
    if preview.creation_format_version != SPARK_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_sha256("request_sha256", &preview.request_sha256)?;
    let manifest = spark_manifest();
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
    let definition_fields = value_object("definition", &preview.definition.definition)?;
    let character_id = object_string(definition_fields, "character_id")?;
    validate_global_identifier("definition.character_id", character_id)?;
    if preview.initial_state.owner_id != character_id {
        return Err(invalid(
            "initial_state.owner_id",
            "owner must equal the definition character_id",
        ));
    }
    let definition_beliefs = object_map(definition_fields, "beliefs")?;
    let definition_attributes = object_map(definition_fields, "attributes")?;
    if definition_beliefs.len() != MAX_BELIEFS || definition_attributes.len() != 4 {
        return Err(invalid(
            "definition",
            "expected exactly three Beliefs and four attributes",
        ));
    }
    if SparkAttribute::ALL
        .iter()
        .any(|attribute| !definition_attributes.contains_key(attribute.symbol()))
    {
        return Err(invalid(
            "definition.attributes",
            "every Spark attribute must be present",
        ));
    }
    let state_fields = value_object("initial_state.value", &preview.initial_state.value)?;
    let state_beliefs = object_map(state_fields, "beliefs")?;
    let definition_ids = definition_beliefs.keys().collect::<BTreeSet<_>>();
    let state_ids = state_beliefs.keys().collect::<BTreeSet<_>>();
    if definition_ids != state_ids {
        return Err(invalid(
            "initial_state.value.beliefs",
            "state Belief identities must match the definition",
        ));
    }
    let declared_steps = manifest
        .creation_steps
        .iter()
        .map(|step| step.id.as_str())
        .collect::<BTreeSet<_>>();
    if !strictly_ordered(&preview.definition.completed_creation_steps)
        || preview
            .definition
            .completed_creation_steps
            .iter()
            .any(|step| !declared_steps.contains(step.as_str()))
    {
        return Err(invalid(
            "definition.completed_creation_steps",
            "unknown, duplicate, or unordered creation step",
        ));
    }
    Ok(())
}

fn validate_creation_request(request: &SparkCreationRequest) -> Result<(), TabletopError> {
    if request.creation_format_version != SPARK_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_global_identifier("character_id", &request.character_id)?;
    validate_bounded_text("name", &request.name, 1, 160)?;
    validate_bounded_text("concept", &request.concept, 1, 512)?;
    if request.starting_influence > MAX_INFLUENCE {
        return Err(invalid(
            "starting_influence",
            "starting Influence exceeds the reviewed domain",
        ));
    }
    if request.beliefs.len() != MAX_BELIEFS {
        return Err(invalid("beliefs", "expected exactly three Beliefs"));
    }
    let mut ids = BTreeSet::new();
    for (index, belief) in request.beliefs.iter().enumerate() {
        validate_local_identifier(&format!("beliefs[{index}].id"), &belief.id)?;
        validate_bounded_text(
            format!("beliefs[{index}].statement"),
            &belief.statement,
            1,
            512,
        )?;
        if !ids.insert(&belief.id) {
            return Err(invalid("beliefs", "duplicate Belief identity"));
        }
    }
    if request.relationships.len() > MAX_RELATIONSHIPS {
        return Err(invalid(
            "relationships",
            "relationship collection exceeds 64 entries",
        ));
    }
    ids.clear();
    for (index, relationship) in request.relationships.iter().enumerate() {
        validate_local_identifier(&format!("relationships[{index}].id"), &relationship.id)?;
        validate_bounded_text(
            format!("relationships[{index}].name"),
            &relationship.name,
            1,
            160,
        )?;
        validate_bounded_text(
            format!("relationships[{index}].description"),
            &relationship.description,
            0,
            512,
        )?;
        if !ids.insert(&relationship.id) {
            return Err(invalid("relationships", "duplicate relationship identity"));
        }
    }
    if request.attributes.len() != 4 {
        return Err(invalid(
            "attributes",
            "expected exactly four attribute selections",
        ));
    }
    let mut selected_attributes = BTreeSet::new();
    for (index, attribute) in request.attributes.iter().enumerate() {
        if !selected_attributes.insert(attribute.attribute) {
            return Err(invalid(
                format!("attributes[{index}].attribute"),
                "duplicate attribute selection",
            ));
        }
    }
    if selected_attributes.len() != SparkAttribute::ALL.len() {
        return Err(invalid(
            "attributes",
            "every Spark attribute must be selected exactly once",
        ));
    }
    if request.talents.len() > MAX_TALENTS {
        return Err(invalid("talents", "talent collection exceeds 64 entries"));
    }
    ids.clear();
    for (index, talent) in request.talents.iter().enumerate() {
        validate_local_identifier(&format!("talents[{index}].id"), &talent.id)?;
        validate_bounded_text(format!("talents[{index}].name"), &talent.name, 1, 160)?;
        if !ids.insert(&talent.id) {
            return Err(invalid("talents", "duplicate Talent identity"));
        }
    }
    Ok(())
}

fn creation_definition_value(request: &SparkCreationRequest) -> DomainValue {
    let attributes = request
        .attributes
        .iter()
        .map(|seed| {
            (
                seed.attribute.symbol().to_owned(),
                DomainValue::Symbol(seed.step.symbol().to_owned()),
            )
        })
        .collect();
    let beliefs = request
        .beliefs
        .iter()
        .map(|belief| {
            (
                belief.id.clone(),
                object([("statement", DomainValue::String(belief.statement.clone()))]),
            )
        })
        .collect();
    let relationships = request
        .relationships
        .iter()
        .map(|relationship| {
            (
                relationship.id.clone(),
                object([
                    (
                        "description",
                        DomainValue::String(relationship.description.clone()),
                    ),
                    ("name", DomainValue::String(relationship.name.clone())),
                ]),
            )
        })
        .collect();
    let talents = request
        .talents
        .iter()
        .map(|talent| {
            (
                talent.id.clone(),
                object([
                    (
                        "attribute",
                        DomainValue::Symbol(talent.attribute.symbol().to_owned()),
                    ),
                    ("name", DomainValue::String(talent.name.clone())),
                    ("tier", DomainValue::Symbol(talent.tier.symbol().to_owned())),
                ]),
            )
        })
        .collect();
    object([
        ("attributes", DomainValue::Object(attributes)),
        ("beliefs", DomainValue::Object(beliefs)),
        (
            "character_id",
            DomainValue::String(request.character_id.clone()),
        ),
        ("concept", DomainValue::String(request.concept.clone())),
        ("name", DomainValue::String(request.name.clone())),
        ("relationships", DomainValue::Object(relationships)),
        ("talents", DomainValue::Object(talents)),
    ])
}

fn initial_state_value(request: &SparkCreationRequest) -> DomainValue {
    let attributes = SparkAttribute::ALL
        .iter()
        .map(|attribute| {
            (
                attribute.symbol().to_owned(),
                object([("harm", DomainValue::Number(0.0))]),
            )
        })
        .collect();
    let beliefs = request
        .beliefs
        .iter()
        .map(|belief| {
            (
                belief.id.clone(),
                object([
                    (
                        "rationale",
                        DomainValue::String(
                            "Belief recorded during character creation.".to_owned(),
                        ),
                    ),
                    (
                        "status",
                        DomainValue::Symbol(SparkBeliefStatus::Held.symbol().to_owned()),
                    ),
                ]),
            )
        })
        .collect();
    object([
        ("attributes", DomainValue::Object(attributes)),
        ("belief_replacements", DomainValue::Object(BTreeMap::new())),
        ("beliefs", DomainValue::Object(beliefs)),
        ("conflicts", DomainValue::Object(BTreeMap::new())),
        ("growth", DomainValue::Number(0.0)),
        (
            "influence",
            DomainValue::Number(f64::from(request.starting_influence)),
        ),
        ("scenes", DomainValue::Object(BTreeMap::new())),
    ])
}

/// Build the exact verified manifest shared by every Spark surface.
#[must_use]
pub fn spark_manifest() -> AdapterManifest {
    let attribute_state_type = object_type([(
        "harm",
        required(
            integer(0.0, f64::from(MAX_HARM)),
            "Harm steps currently reducing the attribute.",
        ),
    )]);
    let belief_definition_type = object_type([(
        "statement",
        required(string(1, 512), "Authored Belief statement."),
    )]);
    let belief_review_type = object_type([
        (
            "belief_id",
            required(string(1, 128), "Reviewed Belief identity."),
        ),
        (
            "rationale",
            required(string(1, 512), "Reviewer rationale retained verbatim."),
        ),
        (
            "verdict",
            required(belief_verdict_type(), "Confirmed or refuted."),
        ),
    ]);
    let belief_state_type = object_type([
        (
            "rationale",
            required(string(0, 512), "Latest review rationale."),
        ),
        (
            "status",
            required(belief_status_type(), "Current Belief review state."),
        ),
    ]);
    let recovery_type = object_type([
        (
            "amount",
            required(
                integer(0.0, f64::from(MAX_HARM)),
                "Harm steps removed by recovery.",
            ),
        ),
        (
            "attribute",
            required(attribute_type(), "Attribute receiving recovery."),
        ),
    ]);
    let declaration_state_type = object_type([
        (
            "actor_id",
            required(string(1, 128), "Stable participating actor."),
        ),
        (
            "attribute",
            required(attribute_type(), "Attribute whose step die is rolled."),
        ),
        (
            "description",
            required(string(1, 512), "Declared fictional intent."),
        ),
        (
            "harm_boost",
            required(
                integer(0.0, f64::from(MAX_HARM)),
                "Post-roll Harm boost applied to this declaration.",
            ),
        ),
        (
            "harm_boost_attribute",
            required(
                string(0, 16),
                "Attribute that received the Harm boost, or empty.",
            ),
        ),
        (
            "influence_boost",
            required(
                integer(0.0, f64::from(MAX_BOOST)),
                "Post-roll Influence boost applied to this declaration.",
            ),
        ),
        (
            "roll",
            required(integer(0.0, 12.0), "Die result before bonuses."),
        ),
        (
            "supporter_bonus",
            required(
                integer(0.0, f64::from(MAX_SUPPORTERS as u32)),
                "Flat bonus from supporters.",
            ),
        ),
        (
            "supporters",
            required(
                list(string(1, 128), 0, MAX_SUPPORTERS),
                "Supporting relationship identities.",
            ),
        ),
        (
            "talent_bonus",
            required(
                integer(0.0, 3.0),
                "Flat bonus from the selected Talent tier.",
            ),
        ),
        (
            "talent_id",
            required(string(0, 128), "Selected Talent identity, or empty."),
        ),
        (
            "total",
            required(
                integer(0.0, f64::from(MAX_TOTAL)),
                "Roll plus every applied bonus.",
            ),
        ),
    ]);
    let conflict_state_type = object_type([
        (
            "declaration_order",
            required(
                list(string(1, 128), 0, MAX_DECLARATIONS),
                "Deterministic declaration order.",
            ),
        ),
        (
            "declarations",
            required(
                map(named("DeclarationState"), 0, MAX_DECLARATIONS),
                "Declarations competing in this conflict.",
            ),
        ),
        (
            "escalation",
            required(
                integer(0.0, f64::from(MAX_ESCALATION)),
                "Current escalation level.",
            ),
        ),
        (
            "max_escalation",
            required(
                integer(0.0, f64::from(MAX_ESCALATION)),
                "Maximum escalation level for this conflict.",
            ),
        ),
        (
            "stakes",
            required(string(1, 512), "Authored conflict stakes."),
        ),
        (
            "status",
            required(conflict_status_type(), "Conflict lifecycle state."),
        ),
        (
            "tie_broken",
            required(
                TypeExpression::Bool,
                "Whether a maximum-escalation tie break selected the winner.",
            ),
        ),
        (
            "victory_cost_amount",
            required(
                integer(0.0, f64::from(MAX_COST_AMOUNT)),
                "Declared cost paid by the winner.",
            ),
        ),
        (
            "victory_cost_attribute",
            required(
                string(0, 16),
                "Attribute used by a Harm victory cost, or empty.",
            ),
        ),
        (
            "victory_cost_kind",
            required(cost_kind_type(), "Influence or Harm victory cost."),
        ),
        (
            "winner_id",
            required(string(0, 128), "Winning declaration identity, or empty."),
        ),
    ]);
    let relationship_definition_type = object_type([
        (
            "description",
            required(string(0, 512), "Relationship detail."),
        ),
        (
            "name",
            required(string(1, 160), "Relationship display name."),
        ),
    ]);
    let scene_state_type = object_type([
        (
            "belief_reviews",
            required(
                list(named("BeliefReviewState"), 0, MAX_BELIEFS),
                "Belief reviews recorded when the scene closed.",
            ),
        ),
        (
            "closed",
            required(TypeExpression::Bool, "Scene closure flag."),
        ),
        ("id", required(string(1, 128), "Scene identity.")),
        (
            "influence_rationale",
            required(string(1, 512), "Explicit Influence review rationale."),
        ),
        ("label", required(string(1, 160), "Scene display label.")),
        (
            "recovery",
            required(
                list(named("RecoveryState"), 0, 4),
                "Permitted Harm removed by recovery.",
            ),
        ),
    ]);
    let talent_definition_type = object_type([
        (
            "attribute",
            required(attribute_type(), "Attribute the Talent supports."),
        ),
        ("name", required(string(1, 160), "Talent display name.")),
        (
            "tier",
            required(talent_tier_type(), "Broad, Common, or Deep."),
        ),
    ]);
    let belief_replacement_type = object_type([
        (
            "belief_id",
            required(string(1, 128), "Replaced Belief identity."),
        ),
        (
            "confirmed_by",
            required(
                list(string(1, 128), 1, 16),
                "Participants confirming the replacement.",
            ),
        ),
        (
            "growth_awarded",
            required(TypeExpression::Bool, "Whether Growth was granted."),
        ),
        (
            "inspiration",
            required(string(1, 512), "Inspiration used for the replacement."),
        ),
        (
            "new_statement",
            required(string(1, 512), "Replacement Belief statement."),
        ),
        (
            "old_statement",
            required(string(1, 512), "Prior Belief statement."),
        ),
        (
            "rationale",
            required(string(1, 512), "Participant rationale."),
        ),
        (
            "replacement_id",
            required(string(1, 128), "Stable replacement identity."),
        ),
        (
            "sequence",
            required(integer(1.0, 64.0), "Deterministic replacement order."),
        ),
    ]);
    let types = BTreeMap::from([
        ("AttributeState".to_owned(), attribute_state_type),
        ("BeliefDefinition".to_owned(), belief_definition_type),
        ("BeliefReplacementState".to_owned(), belief_replacement_type),
        ("BeliefReviewState".to_owned(), belief_review_type),
        ("BeliefState".to_owned(), belief_state_type),
        ("ConflictState".to_owned(), conflict_state_type),
        ("DeclarationState".to_owned(), declaration_state_type),
        ("RecoveryState".to_owned(), recovery_type),
        (
            "RelationshipDefinition".to_owned(),
            relationship_definition_type,
        ),
        ("SceneState".to_owned(), scene_state_type),
        ("TalentDefinition".to_owned(), talent_definition_type),
    ]);
    let definition_type = object_type([
        (
            "attributes",
            required(
                map(step_type(), 4, 4),
                "Authored Body, Heart, Mind, and Spark step dice.",
            ),
        ),
        (
            "beliefs",
            required(
                map(named("BeliefDefinition"), MAX_BELIEFS, MAX_BELIEFS),
                "Exactly three authored Beliefs.",
            ),
        ),
        (
            "character_id",
            required(string(3, 255), "Canonical character reference."),
        ),
        ("concept", required(string(1, 512), "Character concept.")),
        ("name", required(string(1, 160), "Fictional display name.")),
        (
            "relationships",
            required(
                map(named("RelationshipDefinition"), 0, MAX_RELATIONSHIPS),
                "Authored relationships available as supporters.",
            ),
        ),
        (
            "talents",
            required(
                map(named("TalentDefinition"), 0, MAX_TALENTS),
                "Authored Broad, Common, and Deep Talents.",
            ),
        ),
    ]);
    let state_type = object_type([
        (
            "attributes",
            required(map(named("AttributeState"), 4, 4), "Per-attribute Harm."),
        ),
        (
            "belief_replacements",
            required(
                map(named("BeliefReplacementState"), 0, 64),
                "Retained Belief replacement lineage.",
            ),
        ),
        (
            "beliefs",
            required(
                map(named("BeliefState"), MAX_BELIEFS, MAX_BELIEFS),
                "Per-Belief review state.",
            ),
        ),
        (
            "conflicts",
            required(
                map(named("ConflictState"), 0, MAX_CONFLICTS),
                "Saved conflicts.",
            ),
        ),
        (
            "growth",
            required(
                integer(0.0, 99.0),
                "Growth earned from confirmed replacements.",
            ),
        ),
        (
            "influence",
            required(integer(0.0, f64::from(MAX_INFLUENCE)), "Current Influence."),
        ),
        (
            "scenes",
            required(
                map(named("SceneState"), 0, MAX_SCENES),
                "Closed-scene review records.",
            ),
        ),
    ]);

    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: SPARK_ADAPTER_ID.to_owned(),
        version: SPARK_ADAPTER_VERSION.to_owned(),
        schema_version: 1,
        namespace: "spark".to_owned(),
        title: "Spark adapter".to_owned(),
        compatibility_label: "Compatible with The Spark Roleplaying Game core rules".to_owned(),
        summary: "Independently authored Spark core mechanics: Beliefs, relationships, step-die attributes, tiered Talents, declarations, supporters, Influence, Harm, competitive conflicts, escalation, victory costs, and scene review."
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
            capability(TabletopCapability::Advancement),
            capability(TabletopCapability::Scenes),
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
            source_url: SPARK_SOURCE_URL.to_owned(),
            exact_artifact: "spark-source-record.txt".to_owned(),
            revision: "Spark Roleplaying Game core rules source record".to_owned(),
            retrieved_on: "2026-11-06".to_owned(),
            sha256: SPARK_SOURCE_RECORD_SHA256.to_owned(),
            license: "CC0-1.0".to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                .to_owned(),
            covered_files_or_sections: vec![
                "Checked-in source record in spark.rs: canonical URL, creator, publisher, edition, and CC0-1.0 licence."
                    .to_owned(),
                "Spark core mechanics: identity, concept, Beliefs, relationships, step-die attributes, tiered Talents, declarations, supporters, Influence, Harm, conflicts, escalation, victory costs, scene review, and recovery."
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
            attribution: "The Spark Roleplaying Game was created by Jason Pitre and published by Genesis of Legend Publishing; its game text is released under CC0-1.0. Weave's adapter, fixtures, schemas, and explanatory copy are independently authored."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "LICENSE-CC0-1.0.txt".to_owned(),
                sha256: SPARK_CC0_1_0_LEGAL_CODE_SHA256.to_owned(),
            },
            additional_artifacts: Vec::new(),
            notices: vec![
                "Only independently authored core-mechanics behavior and the in-module source record are bundled; no downloaded upstream file is redistributed."
                    .to_owned(),
                "Spark example settings, logos, artwork, fonts, and presentation layout are excluded."
                    .to_owned(),
                "The manifest digest is the SHA-256 of SPARK_SOURCE_RECORD, not of an upstream download."
                    .to_owned(),
            ],
            compatibility_statement: "The Spark Roleplaying Game is named only to describe rules compatibility. This project is independent and does not imply endorsement by Jason Pitre or Genesis of Legend Publishing."
                .to_owned(),
        },
    }
}

fn creation_steps() -> Vec<CreationStep> {
    vec![
        CreationStep {
            id: "identity".to_owned(),
            title: "Identity and concept".to_owned(),
            description: "Name the character and state the concept that guides play.".to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("name", "Name", string(1, 160), true),
                creation_field("concept", "Concept", string(1, 512), true),
            ],
        },
        CreationStep {
            id: "beliefs".to_owned(),
            title: "Beliefs".to_owned(),
            description: "Authored Belief statements that later scenes confirm or refute."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![creation_field(
                "beliefs",
                "Beliefs",
                list(string(1, 512), MAX_BELIEFS, MAX_BELIEFS),
                true,
            )],
        },
        CreationStep {
            id: "relationships".to_owned(),
            title: "Relationships".to_owned(),
            description: "Authored relationships that can support declarations during play."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![creation_field(
                "relationships",
                "Relationships",
                list(string(1, 160), 0, MAX_RELATIONSHIPS),
                false,
            )],
        },
        CreationStep {
            id: "attributes".to_owned(),
            title: "Attributes".to_owned(),
            description: "Select a step die for each of Body, Heart, Mind, and Spark.".to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![creation_field(
                "attributes",
                "Attributes",
                map(step_type(), 4, 4),
                true,
            )],
        },
        CreationStep {
            id: "talents".to_owned(),
            title: "Talents".to_owned(),
            description: "Select Broad, Common, and Deep Talents and the attribute each supports."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![creation_field(
                "talents",
                "Talents",
                list(string(1, 160), 0, MAX_TALENTS),
                false,
            )],
        },
    ]
}

fn operations() -> BTreeMap<String, ResolverOperationDeclaration> {
    BTreeMap::from([
        (
            "add_supporter".to_owned(),
            operation(
                "add_supporter",
                "Add a supporter",
                "Attach one authored relationship to an open declaration before the conflict roll.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "conflict_id",
                        required(string(1, 128), "Open conflict identity."),
                    ),
                    (
                        "declaration_id",
                        required(string(1, 128), "Open declaration identity."),
                    ),
                    (
                        "relationship_id",
                        required(string(1, 128), "Relationship supplying support."),
                    ),
                ]),
                ["supporter_added"],
                false,
            ),
        ),
        (
            "boost_roll".to_owned(),
            operation(
                "boost_roll",
                "Boost a rolled declaration",
                "Spend Influence or take Harm after the roll to add an explicit flat bonus.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "amount",
                        required(integer(1.0, f64::from(MAX_BOOST)), "Boost magnitude."),
                    ),
                    (
                        "attribute",
                        required(
                            string(0, 16),
                            "Attribute receiving Harm; required for Harm boosts.",
                        ),
                    ),
                    (
                        "conflict_id",
                        required(string(1, 128), "Rolled conflict identity."),
                    ),
                    (
                        "declaration_id",
                        required(string(1, 128), "Declaration to boost."),
                    ),
                    (
                        "kind",
                        required(boost_kind_type(), "Influence or Harm boost."),
                    ),
                    (
                        "rationale",
                        required(string(1, 512), "Explicit boost rationale."),
                    ),
                ]),
                ["boost_applied", "harm_changed", "influence_changed"],
                false,
            ),
        ),
        (
            "close_scene".to_owned(),
            operation(
                "close_scene",
                "Close a scene",
                "Record confirmed or refuted Beliefs, review Influence, and remove permitted Harm with retained rationales.",
                TabletopCapability::Scenes,
                object_type([
                    (
                        "belief_reviews",
                        required(
                            list(named("BeliefReviewState"), 0, MAX_BELIEFS),
                            "Belief reviews.",
                        ),
                    ),
                    (
                        "influence_amount",
                        required(
                            integer(0.0, f64::from(MAX_INFLUENCE)),
                            "Awarded or refreshed Influence.",
                        ),
                    ),
                    (
                        "influence_mode",
                        required(
                            influence_mode_type(),
                            "Award adds; Refresh raises to the reviewed amount.",
                        ),
                    ),
                    (
                        "influence_rationale",
                        required(string(1, 512), "Explicit Influence rationale."),
                    ),
                    ("label", required(string(1, 160), "Scene label.")),
                    (
                        "recovery",
                        required(
                            list(named("RecoveryState"), 0, 4),
                            "Permitted Harm removal.",
                        ),
                    ),
                    ("scene_id", required(string(1, 128), "Scene identity.")),
                ]),
                [
                    "belief_reviewed",
                    "harm_changed",
                    "influence_changed",
                    "scene_closed",
                ],
                false,
            ),
        ),
        (
            "declare_action".to_owned(),
            operation(
                "declare_action",
                "Declare an action",
                "Record one actor's competing intent, attribute step die, and optional Talent before rolling.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "actor_id",
                        required(string(1, 128), "Stable participating actor."),
                    ),
                    (
                        "attribute",
                        required(attribute_type(), "Attribute to roll."),
                    ),
                    (
                        "conflict_id",
                        required(string(1, 128), "Open conflict identity."),
                    ),
                    (
                        "declaration_id",
                        required(string(1, 128), "Stable declaration identity."),
                    ),
                    (
                        "description",
                        required(string(1, 512), "Declared fictional intent."),
                    ),
                    (
                        "talent_id",
                        optional(
                            string(1, 128),
                            "Optional Talent whose tier supplies a flat bonus.",
                        ),
                    ),
                ]),
                ["declaration_registered"],
                false,
            ),
        ),
        (
            "open_conflict".to_owned(),
            operation(
                "open_conflict",
                "Open a competitive conflict",
                "Declare the stakes, escalation ceiling, and the winner's explicit victory cost.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "conflict_id",
                        required(string(1, 128), "Stable conflict identity."),
                    ),
                    (
                        "max_escalation",
                        required(
                            integer(0.0, f64::from(MAX_ESCALATION)),
                            "Maximum escalation level.",
                        ),
                    ),
                    ("stakes", required(string(1, 512), "Conflict stakes.")),
                    (
                        "victory_cost_amount",
                        required(
                            integer(0.0, f64::from(MAX_COST_AMOUNT)),
                            "Cost paid by the winning character.",
                        ),
                    ),
                    (
                        "victory_cost_attribute",
                        required(string(0, 16), "Attribute for a Harm cost, or empty."),
                    ),
                    (
                        "victory_cost_kind",
                        required(cost_kind_type(), "Influence or Harm cost."),
                    ),
                ]),
                ["conflict_opened"],
                false,
            ),
        ),
        (
            "replace_belief".to_owned(),
            operation(
                "replace_belief",
                "Replace a Belief through Inspiration",
                "Record a confirmed scene Belief replacement, retain both statements, and grant Growth only after a refuted Belief.",
                TabletopCapability::Advancement,
                object_type([
                    ("belief_id", required(string(1, 128), "Belief to replace.")),
                    (
                        "confirmed_by",
                        required(
                            list(string(1, 128), 1, 16),
                            "Participants confirming the replacement.",
                        ),
                    ),
                    (
                        "inspiration",
                        required(string(1, 512), "Inspiration used for the change."),
                    ),
                    (
                        "new_statement",
                        required(string(1, 512), "Replacement Belief statement."),
                    ),
                    (
                        "rationale",
                        required(string(1, 512), "Participant rationale."),
                    ),
                    (
                        "replacement_id",
                        required(string(1, 128), "Stable replacement identity."),
                    ),
                ]),
                ["belief_replaced", "growth_changed"],
                false,
            ),
        ),
        (
            "resolve_conflict".to_owned(),
            operation(
                "resolve_conflict",
                "Resolve a rolled conflict",
                "Select the highest total, escalate ties deterministically, then charge the winner's cost.",
                TabletopCapability::ChecksAndConflicts,
                object_type([(
                    "conflict_id",
                    required(string(1, 128), "Rolled conflict identity."),
                )]),
                [
                    "conflict_escalated",
                    "conflict_resolved",
                    "harm_changed",
                    "influence_changed",
                    "victory_cost_paid",
                ],
                false,
            ),
        ),
        (
            "roll_conflict".to_owned(),
            operation(
                "roll_conflict",
                "Roll a conflict",
                "Draw one deterministic step die per declaration and add Talent, supporter, and boost bonuses.",
                TabletopCapability::ChecksAndConflicts,
                object_type([(
                    "conflict_id",
                    required(string(1, 128), "Open conflict identity."),
                )]),
                ["entropy_trace", "roll_recorded"],
                true,
            ),
        ),
    ])
}

fn event_types() -> BTreeMap<String, TypeExpression> {
    BTreeMap::from([
        (
            "belief_replaced".to_owned(),
            object_type([
                (
                    "belief_id",
                    required(string(1, 128), "Replaced Belief identity."),
                ),
                (
                    "canonical_write_back",
                    required(
                        TypeExpression::Bool,
                        "Always false: only adapter state changes.",
                    ),
                ),
                (
                    "confirmed_by",
                    required(
                        list(string(1, 128), 1, 16),
                        "Participants confirming the replacement.",
                    ),
                ),
                (
                    "growth_awarded",
                    required(TypeExpression::Bool, "Whether Growth was granted."),
                ),
                ("inspiration", required(string(1, 512), "Inspiration used.")),
                (
                    "new_statement",
                    required(string(1, 512), "Replacement statement."),
                ),
                (
                    "old_statement",
                    required(string(1, 512), "Prior statement."),
                ),
                (
                    "rationale",
                    required(string(1, 512), "Participant rationale."),
                ),
                (
                    "replacement_id",
                    required(string(1, 128), "Stable replacement identity."),
                ),
            ]),
        ),
        (
            "belief_reviewed".to_owned(),
            object_type([
                (
                    "belief_id",
                    required(string(1, 128), "Reviewed Belief identity."),
                ),
                ("rationale", required(string(1, 512), "Review rationale.")),
                (
                    "status",
                    required(belief_verdict_type(), "Confirmed or refuted."),
                ),
            ]),
        ),
        (
            "boost_applied".to_owned(),
            object_type([
                (
                    "amount",
                    required(
                        integer(1.0, f64::from(MAX_BOOST)),
                        "Applied boost magnitude.",
                    ),
                ),
                (
                    "conflict_id",
                    required(string(1, 128), "Conflict identity."),
                ),
                (
                    "declaration_id",
                    required(string(1, 128), "Boosted declaration identity."),
                ),
                (
                    "kind",
                    required(boost_kind_type(), "Influence or Harm boost."),
                ),
                (
                    "total_after",
                    required(
                        integer(0.0, f64::from(MAX_TOTAL)),
                        "Declaration total after the boost.",
                    ),
                ),
            ]),
        ),
        (
            "conflict_escalated".to_owned(),
            object_type([
                (
                    "conflict_id",
                    required(string(1, 128), "Escalated conflict identity."),
                ),
                (
                    "escalation",
                    required(
                        integer(1.0, f64::from(MAX_ESCALATION)),
                        "New escalation level.",
                    ),
                ),
                (
                    "tied_declarations",
                    required(
                        list(string(1, 128), 1, MAX_DECLARATIONS),
                        "Declarations that tied at the previous level.",
                    ),
                ),
            ]),
        ),
        (
            "conflict_opened".to_owned(),
            object_type([
                (
                    "conflict_id",
                    required(string(1, 128), "Conflict identity."),
                ),
                (
                    "max_escalation",
                    required(
                        integer(0.0, f64::from(MAX_ESCALATION)),
                        "Maximum escalation level.",
                    ),
                ),
                ("stakes", required(string(1, 512), "Conflict stakes.")),
                (
                    "victory_cost_amount",
                    required(
                        integer(0.0, f64::from(MAX_COST_AMOUNT)),
                        "Declared victory cost.",
                    ),
                ),
                (
                    "victory_cost_attribute",
                    required(string(0, 16), "Harm attribute or empty."),
                ),
                (
                    "victory_cost_kind",
                    required(cost_kind_type(), "Influence or Harm cost."),
                ),
            ]),
        ),
        (
            "conflict_resolved".to_owned(),
            object_type([
                (
                    "conflict_id",
                    required(string(1, 128), "Conflict identity."),
                ),
                (
                    "escalation",
                    required(
                        integer(0.0, f64::from(MAX_ESCALATION)),
                        "Final escalation level.",
                    ),
                ),
                (
                    "tie_broken",
                    required(
                        TypeExpression::Bool,
                        "Whether a tie break selected the winner.",
                    ),
                ),
                (
                    "total",
                    required(integer(0.0, f64::from(MAX_TOTAL)), "Winning total."),
                ),
                (
                    "winner_actor_id",
                    required(string(1, 128), "Winning actor identity."),
                ),
                (
                    "winner_declaration_id",
                    required(string(1, 128), "Winning declaration identity."),
                ),
            ]),
        ),
        (
            "declaration_registered".to_owned(),
            object_type([
                ("actor_id", required(string(1, 128), "Declaring actor.")),
                (
                    "attribute",
                    required(attribute_type(), "Attribute to roll."),
                ),
                (
                    "conflict_id",
                    required(string(1, 128), "Conflict identity."),
                ),
                (
                    "declaration_id",
                    required(string(1, 128), "Declaration identity."),
                ),
                ("description", required(string(1, 512), "Declared intent.")),
                (
                    "talent_bonus",
                    required(integer(0.0, 3.0), "Talent tier bonus, or zero."),
                ),
                (
                    "talent_id",
                    required(string(0, 128), "Selected Talent, or empty."),
                ),
            ]),
        ),
        (
            "entropy_trace".to_owned(),
            object_type([
                (
                    "cursor_after",
                    required(
                        integer(1.0, 9_007_199_254_740_991.0),
                        "Entropy cursor after the rolls.",
                    ),
                ),
                (
                    "cursor_before",
                    required(
                        integer(0.0, 9_007_199_254_740_990.0),
                        "Entropy cursor before the rolls.",
                    ),
                ),
                (
                    "draws",
                    required(
                        integer(1.0, f64::from(MAX_DECLARATIONS as u32)),
                        "Declarations rolled.",
                    ),
                ),
            ]),
        ),
        (
            "growth_changed".to_owned(),
            object_type([
                (
                    "after",
                    required(integer(0.0, 99.0), "Growth after the change."),
                ),
                (
                    "before",
                    required(integer(0.0, 99.0), "Growth before the change."),
                ),
                (
                    "delta",
                    required(integer(-99.0, 99.0), "Signed Growth change."),
                ),
                (
                    "reason",
                    required(string(1, 512), "Explicit change rationale."),
                ),
            ]),
        ),
        (
            "harm_changed".to_owned(),
            object_type([
                (
                    "after",
                    required(integer(0.0, f64::from(MAX_HARM)), "Harm after the change."),
                ),
                (
                    "attribute",
                    required(attribute_type(), "Affected attribute."),
                ),
                (
                    "before",
                    required(integer(0.0, f64::from(MAX_HARM)), "Harm before the change."),
                ),
                (
                    "delta",
                    required(
                        integer(-f64::from(MAX_HARM), f64::from(MAX_HARM)),
                        "Signed Harm change.",
                    ),
                ),
                (
                    "reason",
                    required(string(1, 512), "Explicit change rationale."),
                ),
            ]),
        ),
        (
            "influence_changed".to_owned(),
            object_type([
                (
                    "after",
                    required(
                        integer(0.0, f64::from(MAX_INFLUENCE)),
                        "Influence after the change.",
                    ),
                ),
                (
                    "before",
                    required(
                        integer(0.0, f64::from(MAX_INFLUENCE)),
                        "Influence before the change.",
                    ),
                ),
                (
                    "delta",
                    required(
                        integer(-f64::from(MAX_INFLUENCE), f64::from(MAX_INFLUENCE)),
                        "Signed Influence change.",
                    ),
                ),
                (
                    "reason",
                    required(string(1, 512), "Explicit change rationale."),
                ),
            ]),
        ),
        (
            "roll_recorded".to_owned(),
            object_type([
                ("actor_id", required(string(1, 128), "Declaring actor.")),
                ("attribute", required(attribute_type(), "Attribute rolled.")),
                (
                    "declaration_id",
                    required(string(1, 128), "Declaration identity."),
                ),
                (
                    "die",
                    required(step_type(), "Effective step die after Harm."),
                ),
                (
                    "harm_boost",
                    required(integer(0.0, f64::from(MAX_HARM)), "Applied Harm boost."),
                ),
                (
                    "influence_boost",
                    required(
                        integer(0.0, f64::from(MAX_BOOST)),
                        "Applied Influence boost.",
                    ),
                ),
                ("roll", required(integer(1.0, 12.0), "Die result.")),
                (
                    "supporter_bonus",
                    required(
                        integer(0.0, f64::from(MAX_SUPPORTERS as u32)),
                        "Supporter bonus.",
                    ),
                ),
                (
                    "talent_bonus",
                    required(integer(0.0, 3.0), "Talent tier bonus."),
                ),
                (
                    "total",
                    required(
                        integer(0.0, f64::from(MAX_TOTAL)),
                        "Total after every bonus.",
                    ),
                ),
            ]),
        ),
        (
            "scene_closed".to_owned(),
            object_type([
                (
                    "belief_review_count",
                    required(
                        integer(0.0, f64::from(MAX_BELIEFS as u32)),
                        "Number of Belief reviews recorded.",
                    ),
                ),
                (
                    "influence_after",
                    required(
                        integer(0.0, f64::from(MAX_INFLUENCE)),
                        "Influence after review.",
                    ),
                ),
                (
                    "influence_before",
                    required(
                        integer(0.0, f64::from(MAX_INFLUENCE)),
                        "Influence before review.",
                    ),
                ),
                ("label", required(string(1, 160), "Scene label.")),
                (
                    "recovered_harm_total",
                    required(integer(0.0, f64::from(MAX_HARM * 4)), "Total Harm removed."),
                ),
                ("scene_id", required(string(1, 128), "Scene identity.")),
            ]),
        ),
        (
            "supporter_added".to_owned(),
            object_type([
                (
                    "actor_id",
                    required(string(1, 128), "Supported declaration's actor."),
                ),
                (
                    "conflict_id",
                    required(string(1, 128), "Conflict identity."),
                ),
                (
                    "declaration_id",
                    required(string(1, 128), "Declaration identity."),
                ),
                (
                    "relationship_id",
                    required(string(1, 128), "Supporting relationship."),
                ),
                (
                    "supporter_bonus",
                    required(
                        integer(1.0, f64::from(MAX_SUPPORTERS as u32)),
                        "Flat supporter bonus after addition.",
                    ),
                ),
                (
                    "supporters",
                    required(
                        list(string(1, 128), 1, MAX_SUPPORTERS),
                        "Ordered supporting relationships.",
                    ),
                ),
            ]),
        ),
        (
            "victory_cost_paid".to_owned(),
            object_type([
                (
                    "amount",
                    required(
                        integer(0.0, f64::from(MAX_COST_AMOUNT)),
                        "Amount actually charged to this state.",
                    ),
                ),
                (
                    "attribute",
                    required(string(0, 16), "Harm attribute or empty."),
                ),
                (
                    "charged",
                    required(
                        TypeExpression::Bool,
                        "Whether the winning character paid in this state.",
                    ),
                ),
                (
                    "kind",
                    required(cost_kind_type(), "Influence or Harm cost."),
                ),
                (
                    "payer_actor_id",
                    required(string(1, 128), "Winning actor identity."),
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

fn attribute_type() -> TypeExpression {
    symbol(["body", "heart", "mind", "spark"])
}

fn step_type() -> TypeExpression {
    symbol(["d10", "d12", "d4", "d6", "d8"])
}

fn talent_tier_type() -> TypeExpression {
    symbol(["broad", "common", "deep"])
}

fn belief_status_type() -> TypeExpression {
    symbol(["confirmed", "held", "refuted"])
}

fn conflict_status_type() -> TypeExpression {
    symbol(["open", "resolved", "rolled"])
}

fn boost_kind_type() -> TypeExpression {
    symbol(["harm", "influence"])
}

fn influence_mode_type() -> TypeExpression {
    symbol(["award", "refresh"])
}

fn belief_verdict_type() -> TypeExpression {
    symbol(["confirmed", "refuted"])
}

fn cost_kind_type() -> TypeExpression {
    symbol(["harm", "influence"])
}

/// Trusted resolver for the exact [`spark_manifest`] coordinate.
#[derive(Debug, Default, Clone, Copy)]
pub struct SparkResolver;

impl TabletopResolver for SparkResolver {
    fn adapter_id(&self) -> &str {
        SPARK_ADAPTER_ID
    }

    fn adapter_version(&self) -> &str {
        SPARK_ADAPTER_VERSION
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
            "add_supporter" => resolve_add_supporter(definition, request, state)?,
            "boost_roll" => resolve_boost_roll(definition, request, state)?,
            "close_scene" => resolve_close_scene(definition, request, state)?,
            "declare_action" => resolve_declare_action(definition, request, state)?,
            "open_conflict" => resolve_open_conflict(request, state)?,
            "replace_belief" => resolve_replace_belief(definition, request, state)?,
            "resolve_conflict" => resolve_resolve_conflict(definition, request, state)?,
            "roll_conflict" => resolve_roll_conflict(definition, request, state, entropy)?,
            _ => {
                return Err(invalid("operation", "unsupported Spark resolver operation"));
            }
        };
        Ok(ResolverOutput {
            state: next_state,
            events,
        })
    }
}

fn resolve_open_conflict(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let conflict_id = object_string(fields, "conflict_id")?.to_owned();
    let stakes = object_string(fields, "stakes")?.to_owned();
    let max_escalation = object_u32(fields, "max_escalation")?;
    let victory_cost_amount = object_u32(fields, "victory_cost_amount")?;
    let victory_cost_kind = parse_cost_kind(object_symbol(fields, "victory_cost_kind")?)?;
    let victory_cost_attribute = object_string(fields, "victory_cost_attribute")?.to_owned();
    validate_local_identifier("input.conflict_id", &conflict_id)?;
    validate_bounded_text("input.stakes", &stakes, 1, 512)?;
    if max_escalation > MAX_ESCALATION {
        return Err(invalid(
            "input.max_escalation",
            "escalation level exceeds the reviewed domain",
        ));
    }
    if victory_cost_amount > MAX_COST_AMOUNT {
        return Err(invalid(
            "input.victory_cost_amount",
            "victory cost exceeds the reviewed domain",
        ));
    }
    if victory_cost_kind == SparkCostKind::Harm || !victory_cost_attribute.is_empty() {
        parse_attribute(&victory_cost_attribute)?;
    }
    let existing = object_map(value_object("state", state)?, "conflicts")?;
    if existing.contains_key(&conflict_id) {
        return Err(invalid(
            "input.conflict_id",
            "conflict identity already exists",
        ));
    }
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let conflicts = object_map_mut(state_fields, "conflicts")?;
    conflicts.insert(
        conflict_id.clone(),
        object([
            (
                "declaration_order",
                DomainValue::List(Vec::<DomainValue>::new()),
            ),
            ("declarations", DomainValue::Object(BTreeMap::new())),
            ("escalation", DomainValue::Number(0.0)),
            (
                "max_escalation",
                DomainValue::Number(f64::from(max_escalation)),
            ),
            ("stakes", DomainValue::String(stakes.clone())),
            (
                "status",
                DomainValue::Symbol(SparkConflictStatus::Open.symbol().to_owned()),
            ),
            ("tie_broken", DomainValue::Bool(false)),
            (
                "victory_cost_amount",
                DomainValue::Number(f64::from(victory_cost_amount)),
            ),
            (
                "victory_cost_attribute",
                DomainValue::String(victory_cost_attribute.clone()),
            ),
            (
                "victory_cost_kind",
                DomainValue::Symbol(victory_cost_kind.symbol().to_owned()),
            ),
            ("winner_id", DomainValue::String(String::new())),
        ]),
    );
    Ok((
        next,
        vec![public_event(
            "conflict_opened",
            object([
                ("conflict_id", DomainValue::String(conflict_id)),
                (
                    "max_escalation",
                    DomainValue::Number(f64::from(max_escalation)),
                ),
                ("stakes", DomainValue::String(stakes)),
                (
                    "victory_cost_amount",
                    DomainValue::Number(f64::from(victory_cost_amount)),
                ),
                (
                    "victory_cost_attribute",
                    DomainValue::String(victory_cost_attribute),
                ),
                (
                    "victory_cost_kind",
                    DomainValue::Symbol(victory_cost_kind.symbol().to_owned()),
                ),
            ]),
        )],
    ))
}

fn resolve_declare_action(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let conflict_id = object_string(fields, "conflict_id")?.to_owned();
    let declaration_id = object_string(fields, "declaration_id")?.to_owned();
    let actor_id = object_string(fields, "actor_id")?.to_owned();
    let attribute = parse_attribute(object_symbol(fields, "attribute")?)?;
    let description = object_string(fields, "description")?.to_owned();
    validate_local_identifier("input.conflict_id", &conflict_id)?;
    validate_local_identifier("input.declaration_id", &declaration_id)?;
    validate_bounded_text("input.actor_id", &actor_id, 1, 128)?;
    validate_bounded_text("input.description", &description, 1, 512)?;
    let (talent_id, talent_bonus) = match optional_string(fields, "talent_id")? {
        Some(value) => {
            validate_local_identifier("input.talent_id", value)?;
            let (id, bonus) = talent_bonus_for(definition, value, attribute)?;
            (id, bonus)
        }
        None => (String::new(), 0),
    };
    let conflict = conflict_ref(state, &conflict_id)?;
    if parse_conflict_status(object_symbol(conflict, "status")?)? != SparkConflictStatus::Open {
        return Err(invalid(
            "state.conflicts.status",
            "declarations require an open conflict",
        ));
    }
    let declarations = object_map(conflict, "declarations")?;
    if declarations.len() >= MAX_DECLARATIONS {
        return Err(invalid(
            "state.conflicts.declarations",
            "conflict already contains the maximum declarations",
        ));
    }
    if declarations.contains_key(&declaration_id) {
        return Err(invalid(
            "input.declaration_id",
            "declaration identity already exists",
        ));
    }
    if declarations.values().any(|declaration| {
        value_object("declaration", declaration)
            .and_then(|value| object_string(value, "actor_id"))
            .is_ok_and(|value| value == actor_id)
    }) {
        return Err(invalid(
            "input.actor_id",
            "actor already declared in this conflict",
        ));
    }
    let mut next = state.clone();
    let conflict = conflict_mut(&mut next, &conflict_id)?;
    let declarations = object_map_mut(conflict, "declarations")?;
    declarations.insert(
        declaration_id.clone(),
        object([
            ("actor_id", DomainValue::String(actor_id.clone())),
            (
                "attribute",
                DomainValue::Symbol(attribute.symbol().to_owned()),
            ),
            ("description", DomainValue::String(description.clone())),
            ("harm_boost", DomainValue::Number(0.0)),
            ("harm_boost_attribute", DomainValue::String(String::new())),
            ("influence_boost", DomainValue::Number(0.0)),
            ("roll", DomainValue::Number(0.0)),
            ("supporter_bonus", DomainValue::Number(0.0)),
            ("supporters", DomainValue::List(Vec::<DomainValue>::new())),
            ("talent_bonus", DomainValue::Number(f64::from(talent_bonus))),
            ("talent_id", DomainValue::String(talent_id.clone())),
            ("total", DomainValue::Number(0.0)),
        ]),
    );
    let order = object_field_mut(conflict, "declaration_order")?;
    let DomainValue::List(order) = order else {
        return Err(invalid(
            "state.conflicts.declaration_order",
            "expected a list",
        ));
    };
    order.push(DomainValue::String(declaration_id.clone()));
    Ok((
        next,
        vec![public_event(
            "declaration_registered",
            object([
                ("actor_id", DomainValue::String(actor_id)),
                (
                    "attribute",
                    DomainValue::Symbol(attribute.symbol().to_owned()),
                ),
                ("conflict_id", DomainValue::String(conflict_id)),
                ("declaration_id", DomainValue::String(declaration_id)),
                ("description", DomainValue::String(description)),
                ("talent_bonus", DomainValue::Number(f64::from(talent_bonus))),
                ("talent_id", DomainValue::String(talent_id)),
            ]),
        )],
    ))
}

fn resolve_add_supporter(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let conflict_id = object_string(fields, "conflict_id")?.to_owned();
    let declaration_id = object_string(fields, "declaration_id")?.to_owned();
    let relationship_id = object_string(fields, "relationship_id")?.to_owned();
    validate_local_identifier("input.conflict_id", &conflict_id)?;
    validate_local_identifier("input.declaration_id", &declaration_id)?;
    validate_local_identifier("input.relationship_id", &relationship_id)?;
    let definition_fields = value_object("definition", definition)?;
    let character_id = object_string(definition_fields, "character_id")?;
    let relationships = object_map(definition_fields, "relationships")?;
    if !relationships.contains_key(&relationship_id) {
        return Err(invalid(
            "input.relationship_id",
            "relationship is absent from the definition",
        ));
    }
    let conflict = conflict_ref(state, &conflict_id)?;
    if parse_conflict_status(object_symbol(conflict, "status")?)? != SparkConflictStatus::Open {
        return Err(invalid(
            "state.conflicts.status",
            "support requires an open conflict",
        ));
    }
    let declarations = object_map(conflict, "declarations")?;
    let declaration = value_object(
        "state.conflicts.declarations",
        object_field(declarations, &declaration_id)?,
    )?;
    let actor_id = object_string(declaration, "actor_id")?;
    if actor_id != character_id {
        return Err(invalid(
            "input.declaration_id",
            "only the owning character's declarations may receive supporters",
        ));
    }
    let supporters = object_list(declaration, "supporters")?;
    if supporters.len() >= MAX_SUPPORTERS {
        return Err(invalid(
            "state.conflicts.declarations.supporters",
            "declaration already has the maximum supporters",
        ));
    }
    if supporters
        .iter()
        .any(|value| matches!(value, DomainValue::String(existing) if existing == &relationship_id))
    {
        return Err(invalid(
            "input.relationship_id",
            "relationship already supports this declaration",
        ));
    }
    let mut next = state.clone();
    let conflict = conflict_mut(&mut next, &conflict_id)?;
    let declarations = object_map_mut(conflict, "declarations")?;
    let declaration_value = declarations
        .get_mut(&declaration_id)
        .ok_or_else(|| invalid("input.declaration_id", "declaration does not exist"))?;
    let declaration = value_object_mut("state.conflicts.declarations", declaration_value)?;
    let supporter_bonus = {
        let supporters = object_list_mut(declaration, "supporters")?;
        supporters.push(DomainValue::String(relationship_id.clone()));
        u32::try_from(supporters.len())
            .map_err(|_| invalid("supporters", "supporter count overflow"))?
    };
    set_u32(declaration, "supporter_bonus", supporter_bonus);
    let supporters = object_list(declaration, "supporters")?.clone();
    Ok((
        next,
        vec![public_event(
            "supporter_added",
            object([
                ("actor_id", DomainValue::String(actor_id.to_owned())),
                ("conflict_id", DomainValue::String(conflict_id)),
                ("declaration_id", DomainValue::String(declaration_id)),
                ("relationship_id", DomainValue::String(relationship_id)),
                (
                    "supporter_bonus",
                    DomainValue::Number(f64::from(supporter_bonus)),
                ),
                ("supporters", DomainValue::List(supporters)),
            ]),
        )],
    ))
}

fn resolve_roll_conflict(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let conflict_id = object_string(fields, "conflict_id")?.to_owned();
    validate_local_identifier("input.conflict_id", &conflict_id)?;
    let conflict = conflict_ref(state, &conflict_id)?;
    if parse_conflict_status(object_symbol(conflict, "status")?)? != SparkConflictStatus::Open {
        return Err(invalid(
            "state.conflicts.status",
            "roll requires an open conflict",
        ));
    }
    let order = string_values(
        "state.conflicts.declaration_order",
        object_list(conflict, "declaration_order")?,
    )?;
    if order.len() < 2 || order.len() > MAX_DECLARATIONS {
        return Err(invalid(
            "state.conflicts.declaration_order",
            "competitive rolls require two to sixteen declarations",
        ));
    }
    let declarations = object_map(conflict, "declarations")?;
    if declarations.len() != order.len() {
        return Err(invalid(
            "state.conflicts.declarations",
            "declaration order does not cover every declaration",
        ));
    }
    let cursor_before = entropy.state().cursor;
    let mut rolls = Vec::with_capacity(order.len());
    let state_fields = value_object("state", state)?;
    for declaration_id in &order {
        let declaration = value_object(
            "state.conflicts.declarations",
            object_field(declarations, declaration_id)?,
        )?;
        let actor_id = object_string(declaration, "actor_id")?.to_owned();
        let attribute = parse_attribute(object_symbol(declaration, "attribute")?)?;
        let step = effective_step(definition, state_fields, attribute)?;
        let roll = u32::try_from(entropy.draw_bounded(step.sides())? + 1)
            .map_err(|_| invalid("roll", "die result overflow"))?;
        let talent_bonus = object_u32(declaration, "talent_bonus")?;
        let supporter_bonus = object_u32(declaration, "supporter_bonus")?;
        let influence_boost = object_u32(declaration, "influence_boost")?;
        let harm_boost = object_u32(declaration, "harm_boost")?;
        let total = roll
            .checked_add(talent_bonus)
            .and_then(|value| value.checked_add(supporter_bonus))
            .and_then(|value| value.checked_add(influence_boost))
            .and_then(|value| value.checked_add(harm_boost))
            .ok_or_else(|| invalid("roll", "declaration total overflow"))?;
        if total > MAX_TOTAL {
            return Err(invalid(
                "roll",
                "declaration total exceeds the reviewed domain",
            ));
        }
        rolls.push((
            declaration_id.clone(),
            actor_id,
            attribute,
            step,
            roll,
            talent_bonus,
            supporter_bonus,
            influence_boost,
            harm_boost,
            total,
        ));
    }
    let cursor_after = entropy.state().cursor;
    let draws = cursor_after.saturating_sub(cursor_before);
    let mut next = state.clone();
    let conflict = conflict_mut(&mut next, &conflict_id)?;
    conflict.insert(
        "status".to_owned(),
        DomainValue::Symbol(SparkConflictStatus::Rolled.symbol().to_owned()),
    );
    let declarations = object_map_mut(conflict, "declarations")?;
    let mut events = Vec::with_capacity(rolls.len() + 1);
    for (
        declaration_id,
        actor_id,
        attribute,
        step,
        roll,
        talent_bonus,
        supporter_bonus,
        influence_boost,
        harm_boost,
        total,
    ) in &rolls
    {
        let declaration = declarations
            .get_mut(declaration_id)
            .ok_or_else(|| invalid("input.conflict_id", "declaration disappeared"))?;
        let declaration = value_object_mut("state.conflicts.declarations", declaration)?;
        set_u32(declaration, "roll", *roll);
        set_u32(declaration, "total", *total);
        events.push(public_event(
            "roll_recorded",
            object([
                ("actor_id", DomainValue::String(actor_id.clone())),
                (
                    "attribute",
                    DomainValue::Symbol(attribute.symbol().to_owned()),
                ),
                (
                    "declaration_id",
                    DomainValue::String(declaration_id.clone()),
                ),
                ("die", DomainValue::Symbol(step.symbol().to_owned())),
                ("harm_boost", DomainValue::Number(f64::from(*harm_boost))),
                (
                    "influence_boost",
                    DomainValue::Number(f64::from(*influence_boost)),
                ),
                ("roll", DomainValue::Number(f64::from(*roll))),
                (
                    "supporter_bonus",
                    DomainValue::Number(f64::from(*supporter_bonus)),
                ),
                (
                    "talent_bonus",
                    DomainValue::Number(f64::from(*talent_bonus)),
                ),
                ("total", DomainValue::Number(f64::from(*total))),
            ]),
        ));
    }
    events.push(authority_event(
        "entropy_trace",
        object([
            ("cursor_after", DomainValue::Number(cursor_after as f64)),
            ("cursor_before", DomainValue::Number(cursor_before as f64)),
            ("draws", DomainValue::Number(draws as f64)),
        ]),
    ));
    Ok((next, events))
}

fn resolve_boost_roll(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let conflict_id = object_string(fields, "conflict_id")?.to_owned();
    let declaration_id = object_string(fields, "declaration_id")?.to_owned();
    let kind = parse_boost_kind(object_symbol(fields, "kind")?)?;
    let amount = object_u32(fields, "amount")?;
    let rationale = object_string(fields, "rationale")?.to_owned();
    validate_local_identifier("input.conflict_id", &conflict_id)?;
    validate_local_identifier("input.declaration_id", &declaration_id)?;
    validate_bounded_text("input.rationale", &rationale, 1, 512)?;
    if amount == 0 || amount > MAX_BOOST {
        return Err(invalid(
            "input.amount",
            "boost amount is outside the reviewed domain",
        ));
    }
    let definition_fields = value_object("definition", definition)?;
    let character_id = object_string(definition_fields, "character_id")?.to_owned();
    let conflict = conflict_ref(state, &conflict_id)?;
    if parse_conflict_status(object_symbol(conflict, "status")?)? != SparkConflictStatus::Rolled {
        return Err(invalid(
            "state.conflicts.status",
            "boosts require a rolled conflict",
        ));
    }
    let declarations = object_map(conflict, "declarations")?;
    let declaration = value_object(
        "state.conflicts.declarations",
        object_field(declarations, &declaration_id)?,
    )?;
    let actor_id = object_string(declaration, "actor_id")?.to_owned();
    if actor_id != character_id {
        return Err(invalid(
            "input.declaration_id",
            "only the owning character may spend Influence or take Harm",
        ));
    }
    let attribute = parse_attribute(object_symbol(declaration, "attribute")?)?;
    let mut next = state.clone();
    let (new_total, influence_event, harm_event) = match kind {
        SparkBoostKind::Influence => {
            let state_fields = value_object("state", state)?;
            let before = object_u32(state_fields, "influence")?;
            if before < amount {
                return Err(invalid(
                    "state.influence",
                    "insufficient Influence for the requested boost",
                ));
            }
            let conflict = conflict_mut(&mut next, &conflict_id)?;
            let declarations = object_map_mut(conflict, "declarations")?;
            let declaration = declarations
                .get_mut(&declaration_id)
                .ok_or_else(|| invalid("input.declaration_id", "declaration does not exist"))?;
            let declaration = value_object_mut("state.conflicts.declarations", declaration)?;
            let existing = object_u32(declaration, "influence_boost")?;
            let updated = existing
                .checked_add(amount)
                .ok_or_else(|| invalid("input.amount", "Influence boost overflow"))?;
            if updated > MAX_BOOST {
                return Err(invalid(
                    "input.amount",
                    "Influence boost exceeds the reviewed domain",
                ));
            }
            set_u32(declaration, "influence_boost", updated);
            let total = object_u32(declaration, "total")?;
            let total = total
                .checked_add(amount)
                .ok_or_else(|| invalid("input.amount", "declaration total overflow"))?;
            if total > MAX_TOTAL {
                return Err(invalid(
                    "input.amount",
                    "declaration total exceeds the reviewed domain",
                ));
            }
            set_u32(declaration, "total", total);
            let state_fields = value_object_mut("state", &mut next)?;
            set_u32(state_fields, "influence", before - amount);
            (
                total,
                Some(public_event(
                    "influence_changed",
                    object([
                        ("after", DomainValue::Number(f64::from(before - amount))),
                        ("before", DomainValue::Number(f64::from(before))),
                        ("delta", DomainValue::Number(-f64::from(amount))),
                        (
                            "reason",
                            DomainValue::String(format!("Explicit post-roll boost: {rationale}")),
                        ),
                    ]),
                )),
                None,
            )
        }
        SparkBoostKind::Harm => {
            let boost_attribute = parse_attribute(object_string(fields, "attribute")?)?;
            if boost_attribute != attribute {
                return Err(invalid(
                    "input.attribute",
                    "Harm boost must use the declaration's attribute",
                ));
            }
            let state_fields = value_object("state", state)?;
            let attributes = object_map(state_fields, "attributes")?;
            let attribute_state = value_object(
                "state.attributes",
                object_field(attributes, attribute.symbol())?,
            )?;
            let before = object_u32(attribute_state, "harm")?;
            let after = before
                .checked_add(amount)
                .ok_or_else(|| invalid("input.amount", "Harm overflow"))?;
            if after > MAX_HARM {
                return Err(invalid(
                    "input.amount",
                    "Harm boost exceeds the reviewed Harm track",
                ));
            }
            let conflict = conflict_mut(&mut next, &conflict_id)?;
            let declarations = object_map_mut(conflict, "declarations")?;
            let declaration = declarations
                .get_mut(&declaration_id)
                .ok_or_else(|| invalid("input.declaration_id", "declaration does not exist"))?;
            let declaration = value_object_mut("state.conflicts.declarations", declaration)?;
            let existing = object_u32(declaration, "harm_boost")?;
            let updated = existing
                .checked_add(amount)
                .ok_or_else(|| invalid("input.amount", "Harm boost overflow"))?;
            if updated > MAX_HARM {
                return Err(invalid(
                    "input.amount",
                    "Harm boost exceeds the reviewed domain",
                ));
            }
            set_u32(declaration, "harm_boost", updated);
            declaration.insert(
                "harm_boost_attribute".to_owned(),
                DomainValue::String(attribute.symbol().to_owned()),
            );
            let total = object_u32(declaration, "total")?;
            let total = total
                .checked_add(amount)
                .ok_or_else(|| invalid("input.amount", "declaration total overflow"))?;
            if total > MAX_TOTAL {
                return Err(invalid(
                    "input.amount",
                    "declaration total exceeds the reviewed domain",
                ));
            }
            set_u32(declaration, "total", total);
            let state_fields = value_object_mut("state", &mut next)?;
            let attributes = object_map_mut(state_fields, "attributes")?;
            let attribute_state = object_map_mut(attributes, attribute.symbol())?;
            set_u32(attribute_state, "harm", after);
            (
                total,
                None,
                Some(public_event(
                    "harm_changed",
                    object([
                        ("after", DomainValue::Number(f64::from(after))),
                        (
                            "attribute",
                            DomainValue::Symbol(attribute.symbol().to_owned()),
                        ),
                        ("before", DomainValue::Number(f64::from(before))),
                        ("delta", DomainValue::Number(f64::from(amount))),
                        (
                            "reason",
                            DomainValue::String(format!(
                                "Explicit post-roll Harm boost: {rationale}"
                            )),
                        ),
                    ]),
                )),
            )
        }
    };
    let mut events = vec![public_event(
        "boost_applied",
        object([
            ("amount", DomainValue::Number(f64::from(amount))),
            ("conflict_id", DomainValue::String(conflict_id)),
            ("declaration_id", DomainValue::String(declaration_id)),
            ("kind", DomainValue::Symbol(kind.symbol().to_owned())),
            ("total_after", DomainValue::Number(f64::from(new_total))),
        ]),
    )];
    if let Some(event) = influence_event {
        events.push(event);
    }
    if let Some(event) = harm_event {
        events.push(event);
    }
    Ok((next, events))
}

fn resolve_resolve_conflict(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let conflict_id = object_string(fields, "conflict_id")?.to_owned();
    validate_local_identifier("input.conflict_id", &conflict_id)?;
    let conflict = conflict_ref(state, &conflict_id)?;
    if parse_conflict_status(object_symbol(conflict, "status")?)? != SparkConflictStatus::Rolled {
        return Err(invalid(
            "state.conflicts.status",
            "resolution requires a rolled conflict",
        ));
    }
    let order = string_values(
        "state.conflicts.declaration_order",
        object_list(conflict, "declaration_order")?,
    )?;
    let declarations = object_map(conflict, "declarations")?;
    let escalation = object_u32(conflict, "escalation")?;
    let max_escalation = object_u32(conflict, "max_escalation")?;
    let mut highest = 0_u32;
    for declaration_id in &order {
        let declaration = value_object(
            "state.conflicts.declarations",
            object_field(declarations, declaration_id)?,
        )?;
        highest = highest.max(object_u32(declaration, "total")?);
    }
    let tied = order
        .iter()
        .filter(|declaration_id| {
            declarations
                .get(declaration_id.as_str())
                .and_then(|value| value_object("declaration", value).ok())
                .and_then(|value| object_u32(value, "total").ok())
                .is_some_and(|total| total == highest)
        })
        .cloned()
        .collect::<Vec<_>>();
    if tied.is_empty() {
        return Err(invalid(
            "state.conflicts.declarations",
            "no declaration carries the highest total",
        ));
    }
    if tied.len() > 1 && escalation < max_escalation {
        let next_escalation = escalation + 1;
        let definition_fields = value_object("definition", definition)?;
        let character_id = object_string(definition_fields, "character_id")?.to_owned();
        let mut next = state.clone();
        let mut events = Vec::new();
        let mut harmed = Vec::new();
        for declaration_id in &tied {
            let declaration = value_object(
                "state.conflicts.declarations",
                object_field(declarations, declaration_id)?,
            )?;
            let actor_id = object_string(declaration, "actor_id")?.to_owned();
            if actor_id != character_id {
                continue;
            }
            let attribute = parse_attribute(object_symbol(declaration, "attribute")?)?;
            let state_fields = value_object("state", state)?;
            let attributes = object_map(state_fields, "attributes")?;
            let attribute_state = value_object(
                "state.attributes",
                object_field(attributes, attribute.symbol())?,
            )?;
            let before = object_u32(attribute_state, "harm")?;
            if before >= MAX_HARM {
                continue;
            }
            let after = before + 1;
            let state_fields = value_object_mut("state", &mut next)?;
            let attributes = object_map_mut(state_fields, "attributes")?;
            let attribute_state = object_map_mut(attributes, attribute.symbol())?;
            set_u32(attribute_state, "harm", after);
            harmed.push((attribute, before, after));
        }
        for (attribute, before, after) in harmed {
            events.push(public_event(
                "harm_changed",
                object([
                    ("after", DomainValue::Number(f64::from(after))),
                    (
                        "attribute",
                        DomainValue::Symbol(attribute.symbol().to_owned()),
                    ),
                    ("before", DomainValue::Number(f64::from(before))),
                    ("delta", DomainValue::Number(1.0)),
                    (
                        "reason",
                        DomainValue::String(
                            "Conflict escalation applied one Harm step.".to_owned(),
                        ),
                    ),
                ]),
            ));
        }
        let conflict = conflict_mut(&mut next, &conflict_id)?;
        conflict.insert(
            "escalation".to_owned(),
            DomainValue::Number(f64::from(next_escalation)),
        );
        conflict.insert(
            "status".to_owned(),
            DomainValue::Symbol(SparkConflictStatus::Open.symbol().to_owned()),
        );
        let declarations = object_map_mut(conflict, "declarations")?;
        for declaration_id in &order {
            let declaration = declarations
                .get_mut(declaration_id)
                .ok_or_else(|| invalid("input.conflict_id", "declaration disappeared"))?;
            let declaration = value_object_mut("state.conflicts.declarations", declaration)?;
            set_u32(declaration, "roll", 0);
            set_u32(declaration, "total", 0);
        }
        events.push(public_event(
            "conflict_escalated",
            object([
                ("conflict_id", DomainValue::String(conflict_id)),
                (
                    "escalation",
                    DomainValue::Number(f64::from(next_escalation)),
                ),
                ("tied_declarations", string_list_value(&tied)),
            ]),
        ));
        return Ok((next, events));
    }
    let (winner_id, tie_broken) = if tied.len() > 1 {
        (
            tied.first()
                .cloned()
                .ok_or_else(|| invalid("input.conflict_id", "tie has no declarations"))?,
            true,
        )
    } else {
        (
            tied.first()
                .cloned()
                .ok_or_else(|| invalid("input.conflict_id", "conflict has no winner"))?,
            false,
        )
    };
    let winner = value_object(
        "state.conflicts.declarations",
        object_field(declarations, &winner_id)?,
    )?;
    let winner_actor_id = object_string(winner, "actor_id")?.to_owned();
    let winner_total = object_u32(winner, "total")?;
    let definition_fields = value_object("definition", definition)?;
    let character_id = object_string(definition_fields, "character_id")?.to_owned();
    let cost_kind = parse_cost_kind(object_symbol(conflict, "victory_cost_kind")?)?;
    let cost_amount = object_u32(conflict, "victory_cost_amount")?;
    let cost_attribute = object_string(conflict, "victory_cost_attribute")?.to_owned();
    let payer_is_owner = winner_actor_id == character_id;
    let mut next = state.clone();
    let conflict = conflict_mut(&mut next, &conflict_id)?;
    conflict.insert(
        "status".to_owned(),
        DomainValue::Symbol(SparkConflictStatus::Resolved.symbol().to_owned()),
    );
    conflict.insert(
        "winner_id".to_owned(),
        DomainValue::String(winner_id.clone()),
    );
    conflict.insert("tie_broken".to_owned(), DomainValue::Bool(tie_broken));
    let mut events = vec![public_event(
        "conflict_resolved",
        object([
            ("conflict_id", DomainValue::String(conflict_id.clone())),
            ("escalation", DomainValue::Number(f64::from(escalation))),
            ("tie_broken", DomainValue::Bool(tie_broken)),
            ("total", DomainValue::Number(f64::from(winner_total))),
            (
                "winner_actor_id",
                DomainValue::String(winner_actor_id.clone()),
            ),
            (
                "winner_declaration_id",
                DomainValue::String(winner_id.clone()),
            ),
        ]),
    )];
    let mut paid_amount = 0_u32;
    let mut cost_change = None;
    if payer_is_owner {
        match cost_kind {
            SparkCostKind::Influence => {
                let state_fields = value_object("state", state)?;
                let before = object_u32(state_fields, "influence")?;
                let paid = cost_amount.min(before);
                let after = before - paid;
                paid_amount = paid;
                let state_fields = value_object_mut("state", &mut next)?;
                set_u32(state_fields, "influence", after);
                if paid > 0 {
                    cost_change = Some(public_event(
                        "influence_changed",
                        object([
                            ("after", DomainValue::Number(f64::from(after))),
                            ("before", DomainValue::Number(f64::from(before))),
                            ("delta", DomainValue::Number(-f64::from(paid))),
                            (
                                "reason",
                                DomainValue::String(
                                    "Winning declaration paid the conflict's Influence cost."
                                        .to_owned(),
                                ),
                            ),
                        ]),
                    ));
                }
            }
            SparkCostKind::Harm => {
                let attribute = parse_attribute(&cost_attribute)?;
                let state_fields = value_object("state", state)?;
                let attributes = object_map(state_fields, "attributes")?;
                let attribute_state = value_object(
                    "state.attributes",
                    object_field(attributes, attribute.symbol())?,
                )?;
                let before = object_u32(attribute_state, "harm")?;
                let after = before.saturating_add(cost_amount).min(MAX_HARM);
                paid_amount = after - before;
                let state_fields = value_object_mut("state", &mut next)?;
                let attributes = object_map_mut(state_fields, "attributes")?;
                let attribute_state = object_map_mut(attributes, attribute.symbol())?;
                set_u32(attribute_state, "harm", after);
                if paid_amount > 0 {
                    cost_change = Some(public_event(
                        "harm_changed",
                        object([
                            ("after", DomainValue::Number(f64::from(after))),
                            (
                                "attribute",
                                DomainValue::Symbol(attribute.symbol().to_owned()),
                            ),
                            ("before", DomainValue::Number(f64::from(before))),
                            ("delta", DomainValue::Number(f64::from(paid_amount))),
                            (
                                "reason",
                                DomainValue::String(
                                    "Winning declaration paid the conflict's Harm cost.".to_owned(),
                                ),
                            ),
                        ]),
                    ));
                }
            }
        }
    }
    events.push(public_event(
        "victory_cost_paid",
        object([
            ("amount", DomainValue::Number(f64::from(paid_amount))),
            ("attribute", DomainValue::String(cost_attribute)),
            ("charged", DomainValue::Bool(paid_amount > 0)),
            ("kind", DomainValue::Symbol(cost_kind.symbol().to_owned())),
            ("payer_actor_id", DomainValue::String(winner_actor_id)),
        ]),
    ));
    if let Some(event) = cost_change {
        events.push(event);
    }
    Ok((next, events))
}

fn resolve_close_scene(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let scene_id = object_string(fields, "scene_id")?.to_owned();
    let label = object_string(fields, "label")?.to_owned();
    let influence_mode = parse_influence_mode(object_symbol(fields, "influence_mode")?)?;
    let influence_amount = object_u32(fields, "influence_amount")?;
    let influence_rationale = object_string(fields, "influence_rationale")?.to_owned();
    validate_local_identifier("input.scene_id", &scene_id)?;
    validate_bounded_text("input.label", &label, 1, 160)?;
    validate_bounded_text("input.influence_rationale", &influence_rationale, 1, 512)?;
    if influence_amount > MAX_INFLUENCE {
        return Err(invalid(
            "input.influence_amount",
            "Influence amount exceeds the reviewed domain",
        ));
    }
    let definition_fields = value_object("definition", definition)?;
    let definition_beliefs = object_map(definition_fields, "beliefs")?;
    let state_fields = value_object("state", state)?;
    let scenes = object_map(state_fields, "scenes")?;
    if scenes.contains_key(&scene_id) {
        return Err(invalid("input.scene_id", "scene identity already exists"));
    }
    let reviews = object_list(fields, "belief_reviews")?;
    if reviews.len() > MAX_BELIEFS {
        return Err(invalid(
            "input.belief_reviews",
            "closing a scene reviews at most three Beliefs",
        ));
    }
    let mut reviewed_ids = BTreeSet::new();
    let mut parsed_reviews = Vec::with_capacity(reviews.len());
    for (index, review) in reviews.iter().enumerate() {
        let review = value_object(format!("input.belief_reviews[{index}]"), review)?;
        let belief_id = object_string(review, "belief_id")?.to_owned();
        let rationale = object_string(review, "rationale")?.to_owned();
        let verdict = parse_belief_verdict(object_symbol(review, "verdict")?)?;
        validate_local_identifier(
            &format!("input.belief_reviews[{index}].belief_id"),
            &belief_id,
        )?;
        validate_bounded_text(
            format!("input.belief_reviews[{index}].rationale"),
            &rationale,
            1,
            512,
        )?;
        if !definition_beliefs.contains_key(&belief_id) {
            return Err(invalid(
                format!("input.belief_reviews[{index}].belief_id"),
                "Belief is absent from the definition",
            ));
        }
        if !reviewed_ids.insert(belief_id.clone()) {
            return Err(invalid("input.belief_reviews", "duplicate Belief review"));
        }
        parsed_reviews.push((belief_id, rationale, verdict));
    }
    let recovery = object_list(fields, "recovery")?;
    let mut recovered_attributes = BTreeSet::new();
    let mut parsed_recovery = Vec::with_capacity(recovery.len());
    for (index, entry) in recovery.iter().enumerate() {
        let entry = value_object(format!("input.recovery[{index}]"), entry)?;
        let attribute = parse_attribute(object_symbol(entry, "attribute")?)?;
        let amount = object_u32(entry, "amount")?;
        if amount > MAX_HARM {
            return Err(invalid(
                format!("input.recovery[{index}].amount"),
                "recovery exceeds the reviewed Harm track",
            ));
        }
        if !recovered_attributes.insert(attribute) {
            return Err(invalid("input.recovery", "duplicate recovery attribute"));
        }
        let attributes = object_map(state_fields, "attributes")?;
        let attribute_state = value_object(
            "state.attributes",
            object_field(attributes, attribute.symbol())?,
        )?;
        let before = object_u32(attribute_state, "harm")?;
        if amount > before {
            return Err(invalid(
                format!("input.recovery[{index}].amount"),
                "recovery exceeds recorded Harm",
            ));
        }
        parsed_recovery.push((attribute, amount, before));
    }
    let influence_before = object_u32(state_fields, "influence")?;
    let influence_after = match influence_mode {
        SparkInfluenceMode::Award => influence_before
            .saturating_add(influence_amount)
            .min(MAX_INFLUENCE),
        SparkInfluenceMode::Refresh => influence_before.max(influence_amount),
    };
    let mut next = state.clone();
    let mut events = Vec::with_capacity(parsed_reviews.len() + parsed_recovery.len() + 2);
    for (belief_id, rationale, verdict) in &parsed_reviews {
        let status = match verdict {
            SparkBeliefVerdict::Confirmed => SparkBeliefStatus::Confirmed,
            SparkBeliefVerdict::Refuted => SparkBeliefStatus::Refuted,
        };
        let state_fields = value_object_mut("state", &mut next)?;
        let beliefs = object_map_mut(state_fields, "beliefs")?;
        let belief = beliefs
            .get_mut(belief_id)
            .ok_or_else(|| invalid("input.belief_reviews", "Belief state disappeared"))?;
        let belief = value_object_mut("state.beliefs", belief)?;
        belief.insert(
            "status".to_owned(),
            DomainValue::Symbol(status.symbol().to_owned()),
        );
        belief.insert(
            "rationale".to_owned(),
            DomainValue::String(rationale.clone()),
        );
        events.push(public_event(
            "belief_reviewed",
            object([
                ("belief_id", DomainValue::String(belief_id.clone())),
                ("rationale", DomainValue::String(rationale.clone())),
                ("status", DomainValue::Symbol(verdict.symbol().to_owned())),
            ]),
        ));
    }
    let mut recovered_total = 0_u32;
    for (attribute, amount, before) in &parsed_recovery {
        if *amount == 0 {
            continue;
        }
        let after = *before - *amount;
        recovered_total += *amount;
        let state_fields = value_object_mut("state", &mut next)?;
        let attributes = object_map_mut(state_fields, "attributes")?;
        let attribute_state = object_map_mut(attributes, attribute.symbol())?;
        set_u32(attribute_state, "harm", after);
        events.push(public_event(
            "harm_changed",
            object([
                ("after", DomainValue::Number(f64::from(after))),
                (
                    "attribute",
                    DomainValue::Symbol(attribute.symbol().to_owned()),
                ),
                ("before", DomainValue::Number(f64::from(*before))),
                ("delta", DomainValue::Number(-f64::from(*amount))),
                (
                    "reason",
                    DomainValue::String(
                        "Closing-scene recovery removed permitted Harm.".to_owned(),
                    ),
                ),
            ]),
        ));
    }
    if influence_after != influence_before {
        let state_fields = value_object_mut("state", &mut next)?;
        set_u32(state_fields, "influence", influence_after);
        events.push(public_event(
            "influence_changed",
            object([
                ("after", DomainValue::Number(f64::from(influence_after))),
                ("before", DomainValue::Number(f64::from(influence_before))),
                (
                    "delta",
                    DomainValue::Number((influence_after as i64 - influence_before as i64) as f64),
                ),
                ("reason", DomainValue::String(influence_rationale.clone())),
            ]),
        ));
    }
    let belief_reviews = parsed_reviews
        .iter()
        .map(|(belief_id, rationale, verdict)| {
            object([
                ("belief_id", DomainValue::String(belief_id.clone())),
                ("rationale", DomainValue::String(rationale.clone())),
                ("verdict", DomainValue::Symbol(verdict.symbol().to_owned())),
            ])
        })
        .collect::<Vec<_>>();
    let recovery_values = parsed_recovery
        .iter()
        .map(|(attribute, amount, _)| {
            object([
                ("amount", DomainValue::Number(f64::from(*amount))),
                (
                    "attribute",
                    DomainValue::Symbol(attribute.symbol().to_owned()),
                ),
            ])
        })
        .collect::<Vec<_>>();
    let state_fields = value_object_mut("state", &mut next)?;
    let scenes = object_map_mut(state_fields, "scenes")?;
    scenes.insert(
        scene_id.clone(),
        object([
            ("belief_reviews", DomainValue::List(belief_reviews)),
            ("closed", DomainValue::Bool(true)),
            ("id", DomainValue::String(scene_id.clone())),
            (
                "influence_rationale",
                DomainValue::String(influence_rationale),
            ),
            ("label", DomainValue::String(label.clone())),
            ("recovery", DomainValue::List(recovery_values)),
        ]),
    );
    events.push(public_event(
        "scene_closed",
        object([
            (
                "belief_review_count",
                DomainValue::Number(parsed_reviews.len() as f64),
            ),
            (
                "influence_after",
                DomainValue::Number(f64::from(influence_after)),
            ),
            (
                "influence_before",
                DomainValue::Number(f64::from(influence_before)),
            ),
            ("label", DomainValue::String(label)),
            (
                "recovered_harm_total",
                DomainValue::Number(f64::from(recovered_total)),
            ),
            ("scene_id", DomainValue::String(scene_id)),
        ]),
    ));
    Ok((next, events))
}

fn talent_bonus_for(
    definition: &DomainValue,
    talent_id: &str,
    attribute: SparkAttribute,
) -> Result<(String, u32), TabletopError> {
    let fields = value_object("definition", definition)?;
    let talents = object_map(fields, "talents")?;
    let talent = value_object(
        "definition.talents",
        talents
            .get(talent_id)
            .ok_or_else(|| invalid("input.talent_id", "Talent is absent from the definition"))?,
    )?;
    let talent_attribute = parse_attribute(object_symbol(talent, "attribute")?)?;
    if talent_attribute != attribute {
        return Err(invalid(
            "input.talent_id",
            "Talent does not support the declared attribute",
        ));
    }
    let tier = parse_tier(object_symbol(talent, "tier")?)?;
    Ok((talent_id.to_owned(), tier.bonus()))
}

fn effective_step(
    definition: &DomainValue,
    state_fields: &BTreeMap<String, DomainValue>,
    attribute: SparkAttribute,
) -> Result<SparkStep, TabletopError> {
    let definition_fields = value_object("definition", definition)?;
    let attributes = object_map(definition_fields, "attributes")?;
    let step = parse_step(object_symbol(attributes, attribute.symbol())?)?;
    let attribute_states = object_map(state_fields, "attributes")?;
    let attribute_state = value_object(
        "state.attributes",
        object_field(attribute_states, attribute.symbol())?,
    )?;
    let harm = object_u32(attribute_state, "harm")?;
    Ok(SparkStep::from_rank(step.rank().saturating_sub(harm)))
}

fn conflict_ref<'a>(
    state: &'a DomainValue,
    conflict_id: &str,
) -> Result<&'a BTreeMap<String, DomainValue>, TabletopError> {
    let fields = value_object("state", state)?;
    let conflicts = object_map(fields, "conflicts")?;
    value_object("state.conflicts", object_field(conflicts, conflict_id)?)
}

fn conflict_mut<'a>(
    state: &'a mut DomainValue,
    conflict_id: &str,
) -> Result<&'a mut BTreeMap<String, DomainValue>, TabletopError> {
    let fields = value_object_mut("state", state)?;
    let conflicts = object_map_mut(fields, "conflicts")?;
    let value = conflicts
        .get_mut(conflict_id)
        .ok_or_else(|| invalid("input.conflict_id", "conflict does not exist"))?;
    value_object_mut("state.conflicts", value)
}

fn parse_attribute(value: &str) -> Result<SparkAttribute, TabletopError> {
    SparkAttribute::from_symbol(value)
        .ok_or_else(|| invalid("attribute", "unknown Spark attribute"))
}

fn parse_step(value: &str) -> Result<SparkStep, TabletopError> {
    SparkStep::from_symbol(value).ok_or_else(|| invalid("step", "unknown step die"))
}

fn parse_tier(value: &str) -> Result<SparkTalentTier, TabletopError> {
    SparkTalentTier::from_symbol(value).ok_or_else(|| invalid("tier", "unknown Talent tier"))
}

fn parse_conflict_status(value: &str) -> Result<SparkConflictStatus, TabletopError> {
    SparkConflictStatus::from_symbol(value)
        .ok_or_else(|| invalid("conflict_status", "unknown conflict status"))
}

fn parse_boost_kind(value: &str) -> Result<SparkBoostKind, TabletopError> {
    SparkBoostKind::from_symbol(value).ok_or_else(|| invalid("boost_kind", "unknown boost kind"))
}

fn parse_influence_mode(value: &str) -> Result<SparkInfluenceMode, TabletopError> {
    SparkInfluenceMode::from_symbol(value)
        .ok_or_else(|| invalid("influence_mode", "unknown Influence review mode"))
}

fn parse_belief_verdict(value: &str) -> Result<SparkBeliefVerdict, TabletopError> {
    SparkBeliefVerdict::from_symbol(value)
        .ok_or_else(|| invalid("belief_verdict", "unknown Belief verdict"))
}

fn parse_cost_kind(value: &str) -> Result<SparkCostKind, TabletopError> {
    SparkCostKind::from_symbol(value)
        .ok_or_else(|| invalid("victory_cost_kind", "unknown victory cost kind"))
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

fn object_field_mut<'a>(
    fields: &'a mut BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a mut DomainValue, TabletopError> {
    fields
        .get_mut(field)
        .ok_or_else(|| invalid(field, "required field is missing"))
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

fn object_bool(fields: &BTreeMap<String, DomainValue>, field: &str) -> Result<bool, TabletopError> {
    match object_field(fields, field)? {
        DomainValue::Bool(value) => Ok(*value),
        _ => Err(invalid(field, "expected a boolean")),
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

fn object_list_mut<'a>(
    fields: &'a mut BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a mut Vec<DomainValue>, TabletopError> {
    match fields.get_mut(field) {
        Some(DomainValue::List(values)) => Ok(values),
        Some(_) => Err(invalid(field, "expected a list")),
        None => Err(invalid(field, "required field is missing")),
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

fn strictly_ordered<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn invalid(path: impl Into<String>, reason: &'static str) -> TabletopError {
    TabletopError::InvalidField {
        path: path.into(),
        reason,
    }
}

// ---------------------------------------------------------------------------
// Setting model: typed, namespaced, portable without a bundled setting.
// ---------------------------------------------------------------------------

/// Versioned portable Spark setting artifact.
pub const SPARK_SETTING_FORMAT_VERSION: u32 = 1;
/// Adapter namespace recorded by every setting entity.
pub const SPARK_SETTING_NAMESPACE: &str = "spark";
const MAX_SETTING_ENTITIES: usize = 64;
const MAX_SETTING_BELIEFS: usize = 3;
const MAX_SETTING_TAGS: usize = 64;
const MAX_GM_ATTRIBUTE_VALUE: u32 = 99;

/// Participant role in a collaborative setting workshop.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkParticipantRole {
    Owner,
    Gm,
    Player,
}

impl SparkParticipantRole {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Gm => "gm",
            Self::Player => "player",
        }
    }
}

/// One named setting contributor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSettingParticipant {
    pub id: String,
    pub display_name: String,
    pub role: SparkParticipantRole,
}

/// One authored setting Belief.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSettingBelief {
    pub id: String,
    pub namespace: String,
    pub owner_id: String,
    pub statement: String,
}

/// One authored GM-facing setting attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkGmAttribute {
    pub id: String,
    pub namespace: String,
    pub owner_id: String,
    pub name: String,
    pub value: u32,
    pub description: String,
}

/// One setting Faction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkFaction {
    pub id: String,
    pub namespace: String,
    pub owner_id: String,
    pub name: String,
    pub description: String,
}

/// One authored Tie between two Factions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkTie {
    pub id: String,
    pub namespace: String,
    pub owner_id: String,
    pub from_faction: String,
    pub to_faction: String,
    pub description: String,
}

/// One setting Face.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkFace {
    pub id: String,
    pub namespace: String,
    pub owner_id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub faction_id: Option<String>,
}

/// One setting Agenda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkAgenda {
    pub id: String,
    pub namespace: String,
    pub owner_id: String,
    pub name: String,
    pub description: String,
    pub faction_id: Option<String>,
}

/// Optional target kind for a stable Weave World reference.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SparkWorldTargetKind {
    Character,
    Faction,
    Location,
    Other,
}

impl SparkWorldTargetKind {
    /// Portable lower-case symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Faction => "faction",
            Self::Location => "location",
            Self::Other => "other",
        }
    }
}

/// Optional stable reference into a Weave World module.
///
/// The reference is inert data. Loading the setting never requires Weave World, and the adapter
/// never reads or mutates World data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkWorldReference {
    pub id: String,
    pub namespace: String,
    pub owner_id: String,
    pub target_kind: SparkWorldTargetKind,
    pub target_id: String,
    pub description: String,
}

/// Complete portable Spark setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSetting {
    pub format_version: u32,
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub inspirations: BTreeSet<String>,
    pub genres: BTreeSet<String>,
    pub facts: BTreeSet<String>,
    pub setting_beliefs: BTreeMap<String, SparkSettingBelief>,
    pub gm_attributes: BTreeMap<String, SparkGmAttribute>,
    pub factions: BTreeMap<String, SparkFaction>,
    pub ties: BTreeMap<String, SparkTie>,
    pub faces: BTreeMap<String, SparkFace>,
    pub agendas: BTreeMap<String, SparkAgenda>,
    pub world_references: BTreeMap<String, SparkWorldReference>,
}

impl_spark_io!(SparkSetting);

/// Canonical JSON Schema for portable Spark settings.
pub fn spark_setting_schema() -> Result<String, TabletopError> {
    spark_schema::<SparkSetting>(
        "urn:weave:schema:tabletop-spark-setting:1",
        "Weave Spark Setting v1",
        Some(("format_version", SPARK_SETTING_FORMAT_VERSION)),
    )
}

impl SparkSetting {
    /// Validate every setting identity, owner, reference, and deterministic map key.
    pub fn validate(&self) -> Result<(), TabletopError> {
        validate_spark_setting(self)
    }

    /// Portable values exposed to story expressions.
    ///
    /// The projection contains only authored setting facts. It makes no personality, alignment,
    /// or historical claim about any character.
    #[must_use]
    pub fn story_values(&self) -> BTreeMap<String, DomainValue> {
        spark_setting_story_values(self)
    }
}

/// Validate one portable Spark setting.
pub fn validate_spark_setting(setting: &SparkSetting) -> Result<(), TabletopError> {
    if setting.format_version != SPARK_SETTING_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "format_version",
        });
    }
    validate_global_identifier("id", &setting.id)?;
    if setting.namespace != SPARK_SETTING_NAMESPACE {
        return Err(invalid(
            "namespace",
            "setting namespace must equal the adapter namespace",
        ));
    }
    validate_bounded_text("title", &setting.title, 1, 160)?;
    validate_setting_tags("inspirations", &setting.inspirations)?;
    validate_setting_tags("genres", &setting.genres)?;
    validate_setting_tags("facts", &setting.facts)?;
    if setting.setting_beliefs.len() != MAX_SETTING_BELIEFS {
        return Err(invalid(
            "setting_beliefs",
            "expected exactly three setting Beliefs",
        ));
    }
    let mut entity_count = 0usize;
    for (key, belief) in &setting.setting_beliefs {
        validate_setting_entity_identity(
            "setting_beliefs",
            key,
            &belief.id,
            &belief.namespace,
            &belief.owner_id,
            setting.namespace.as_str(),
        )?;
        validate_bounded_text(
            format!("setting_beliefs.{key}.statement"),
            &belief.statement,
            1,
            512,
        )?;
        entity_count += 1;
    }
    for (key, attribute) in &setting.gm_attributes {
        validate_setting_entity_identity(
            "gm_attributes",
            key,
            &attribute.id,
            &attribute.namespace,
            &attribute.owner_id,
            setting.namespace.as_str(),
        )?;
        validate_bounded_text(format!("gm_attributes.{key}.name"), &attribute.name, 1, 160)?;
        validate_bounded_text(
            format!("gm_attributes.{key}.description"),
            &attribute.description,
            0,
            512,
        )?;
        if attribute.value > MAX_GM_ATTRIBUTE_VALUE {
            return Err(invalid(
                format!("gm_attributes.{key}.value"),
                "GM attribute exceeds the reviewed domain",
            ));
        }
        entity_count += 1;
    }
    for (key, faction) in &setting.factions {
        validate_setting_entity_identity(
            "factions",
            key,
            &faction.id,
            &faction.namespace,
            &faction.owner_id,
            setting.namespace.as_str(),
        )?;
        validate_bounded_text(format!("factions.{key}.name"), &faction.name, 1, 160)?;
        validate_bounded_text(
            format!("factions.{key}.description"),
            &faction.description,
            0,
            512,
        )?;
        entity_count += 1;
    }
    for (key, tie) in &setting.ties {
        validate_setting_entity_identity(
            "ties",
            key,
            &tie.id,
            &tie.namespace,
            &tie.owner_id,
            setting.namespace.as_str(),
        )?;
        require_setting_faction(
            "ties",
            key,
            "from_faction",
            &tie.from_faction,
            &setting.factions,
        )?;
        require_setting_faction(
            "ties",
            key,
            "to_faction",
            &tie.to_faction,
            &setting.factions,
        )?;
        if tie.from_faction == tie.to_faction {
            return Err(invalid(
                format!("ties.{key}"),
                "a Tie must connect two distinct Factions",
            ));
        }
        validate_bounded_text(format!("ties.{key}.description"), &tie.description, 0, 512)?;
        entity_count += 1;
    }
    for (key, face) in &setting.faces {
        validate_setting_entity_identity(
            "faces",
            key,
            &face.id,
            &face.namespace,
            &face.owner_id,
            setting.namespace.as_str(),
        )?;
        validate_bounded_text(format!("faces.{key}.name"), &face.name, 1, 160)?;
        validate_bounded_text(format!("faces.{key}.role"), &face.role, 1, 160)?;
        validate_bounded_text(
            format!("faces.{key}.description"),
            &face.description,
            0,
            512,
        )?;
        if let Some(faction_id) = &face.faction_id {
            require_setting_faction("faces", key, "faction_id", faction_id, &setting.factions)?;
        }
        entity_count += 1;
    }
    for (key, agenda) in &setting.agendas {
        validate_setting_entity_identity(
            "agendas",
            key,
            &agenda.id,
            &agenda.namespace,
            &agenda.owner_id,
            setting.namespace.as_str(),
        )?;
        validate_bounded_text(format!("agendas.{key}.name"), &agenda.name, 1, 160)?;
        validate_bounded_text(
            format!("agendas.{key}.description"),
            &agenda.description,
            0,
            512,
        )?;
        if let Some(faction_id) = &agenda.faction_id {
            require_setting_faction("agendas", key, "faction_id", faction_id, &setting.factions)?;
        }
        entity_count += 1;
    }
    for (key, reference) in &setting.world_references {
        validate_setting_entity_identity(
            "world_references",
            key,
            &reference.id,
            &reference.namespace,
            &reference.owner_id,
            setting.namespace.as_str(),
        )?;
        validate_global_identifier(
            &format!("world_references.{key}.target_id"),
            &reference.target_id,
        )?;
        validate_bounded_text(
            format!("world_references.{key}.description"),
            &reference.description,
            0,
            512,
        )?;
        entity_count += 1;
    }
    if entity_count > MAX_SETTING_ENTITIES {
        return Err(invalid(
            "setting",
            "setting entity count exceeds the reviewed domain",
        ));
    }
    Ok(())
}

fn validate_setting_tags(path: &str, values: &BTreeSet<String>) -> Result<(), TabletopError> {
    if values.len() > MAX_SETTING_TAGS {
        return Err(invalid(path, "setting tag collection exceeds 64 entries"));
    }
    for value in values {
        validate_bounded_text(path, value, 1, 512)?;
    }
    Ok(())
}

fn validate_setting_entity_identity(
    path: &str,
    key: &str,
    id: &str,
    namespace: &str,
    owner_id: &str,
    setting_namespace: &str,
) -> Result<(), TabletopError> {
    if key != id {
        return Err(invalid(
            path,
            "deterministic map key must equal the stable entity id",
        ));
    }
    validate_local_identifier(&format!("{path}.{key}.id"), id)?;
    if namespace != setting_namespace || namespace != SPARK_SETTING_NAMESPACE {
        return Err(invalid(
            format!("{path}.{key}.namespace"),
            "entity namespace must equal the adapter namespace",
        ));
    }
    validate_global_identifier(&format!("{path}.{key}.owner_id"), owner_id)?;
    Ok(())
}

fn require_setting_faction(
    path: &str,
    key: &str,
    field: &str,
    faction_id: &str,
    factions: &BTreeMap<String, SparkFaction>,
) -> Result<(), TabletopError> {
    validate_local_identifier(&format!("{path}.{key}.{field}"), faction_id)?;
    if !factions.contains_key(faction_id) {
        return Err(invalid(
            format!("{path}.{key}.{field}"),
            "reference does not name a declared Faction",
        ));
    }
    Ok(())
}

/// Build the portable value projection read by story expressions.
#[must_use]
pub fn spark_setting_story_values(setting: &SparkSetting) -> BTreeMap<String, DomainValue> {
    let setting_beliefs = setting
        .setting_beliefs
        .iter()
        .map(|(id, belief)| {
            (
                id.clone(),
                object([
                    ("namespace", DomainValue::String(belief.namespace.clone())),
                    ("owner_id", DomainValue::String(belief.owner_id.clone())),
                    ("statement", DomainValue::String(belief.statement.clone())),
                ]),
            )
        })
        .collect();
    let gm_attributes = setting
        .gm_attributes
        .iter()
        .map(|(id, attribute)| {
            (
                id.clone(),
                object([
                    (
                        "description",
                        DomainValue::String(attribute.description.clone()),
                    ),
                    ("name", DomainValue::String(attribute.name.clone())),
                    ("owner_id", DomainValue::String(attribute.owner_id.clone())),
                    ("value", DomainValue::Number(f64::from(attribute.value))),
                ]),
            )
        })
        .collect();
    let factions = setting
        .factions
        .iter()
        .map(|(id, faction)| {
            (
                id.clone(),
                object([
                    (
                        "description",
                        DomainValue::String(faction.description.clone()),
                    ),
                    ("name", DomainValue::String(faction.name.clone())),
                    ("owner_id", DomainValue::String(faction.owner_id.clone())),
                ]),
            )
        })
        .collect();
    let ties = setting
        .ties
        .iter()
        .map(|(id, tie)| {
            (
                id.clone(),
                object([
                    ("description", DomainValue::String(tie.description.clone())),
                    (
                        "from_faction",
                        DomainValue::String(tie.from_faction.clone()),
                    ),
                    ("owner_id", DomainValue::String(tie.owner_id.clone())),
                    ("to_faction", DomainValue::String(tie.to_faction.clone())),
                ]),
            )
        })
        .collect();
    let faces = setting
        .faces
        .iter()
        .map(|(id, face)| {
            (
                id.clone(),
                object([
                    ("description", DomainValue::String(face.description.clone())),
                    (
                        "faction_id",
                        DomainValue::String(face.faction_id.clone().unwrap_or_default()),
                    ),
                    ("name", DomainValue::String(face.name.clone())),
                    ("owner_id", DomainValue::String(face.owner_id.clone())),
                    ("role", DomainValue::String(face.role.clone())),
                ]),
            )
        })
        .collect();
    let agendas = setting
        .agendas
        .iter()
        .map(|(id, agenda)| {
            (
                id.clone(),
                object([
                    (
                        "description",
                        DomainValue::String(agenda.description.clone()),
                    ),
                    (
                        "faction_id",
                        DomainValue::String(agenda.faction_id.clone().unwrap_or_default()),
                    ),
                    ("name", DomainValue::String(agenda.name.clone())),
                    ("owner_id", DomainValue::String(agenda.owner_id.clone())),
                ]),
            )
        })
        .collect();
    let world_references = setting
        .world_references
        .iter()
        .map(|(id, reference)| {
            (
                id.clone(),
                object([
                    (
                        "description",
                        DomainValue::String(reference.description.clone()),
                    ),
                    ("owner_id", DomainValue::String(reference.owner_id.clone())),
                    (
                        "target_id",
                        DomainValue::String(reference.target_id.clone()),
                    ),
                    (
                        "target_kind",
                        DomainValue::Symbol(reference.target_kind.symbol().to_owned()),
                    ),
                ]),
            )
        })
        .collect();
    BTreeMap::from([
        (
            "facts".to_owned(),
            string_list_value(&setting.facts.iter().cloned().collect::<Vec<_>>()),
        ),
        ("factions".to_owned(), DomainValue::Object(factions)),
        ("faces".to_owned(), DomainValue::Object(faces)),
        ("agendas".to_owned(), DomainValue::Object(agendas)),
        (
            "genres".to_owned(),
            string_list_value(&setting.genres.iter().cloned().collect::<Vec<_>>()),
        ),
        (
            "gm_attributes".to_owned(),
            DomainValue::Object(gm_attributes),
        ),
        (
            "inspirations".to_owned(),
            string_list_value(&setting.inspirations.iter().cloned().collect::<Vec<_>>()),
        ),
        (
            "namespace".to_owned(),
            DomainValue::String(setting.namespace.clone()),
        ),
        (
            "setting_beliefs".to_owned(),
            DomainValue::Object(setting_beliefs),
        ),
        (
            "setting_id".to_owned(),
            DomainValue::String(setting.id.clone()),
        ),
        ("ties".to_owned(), DomainValue::Object(ties)),
        (
            "title".to_owned(),
            DomainValue::String(setting.title.clone()),
        ),
        (
            "world_references".to_owned(),
            DomainValue::Object(world_references),
        ),
    ])
}

// ---------------------------------------------------------------------------
// Collaborative setting creation and scene framing.
// ---------------------------------------------------------------------------

/// Boundary notes for the Weave-authored collaboration layer.
pub const SPARK_COLLABORATION_NOTES: &[&str] = &[
    "The CC0-1.0 Spark core procedures are implemented independently in this adapter.",
    "Collaborative setting creation, scene framing, belief review, and Evolution are Weave-authored workflow features, not Spark core rules.",
    "Weave World references are optional stable identifiers; no World module is required and no World data is mutated.",
    "Published settings, characters, themes, logos, artwork, and fonts are not reproduced.",
];

/// Versioned collaborative setting-creation format.
pub const SPARK_SETTING_CREATION_FORMAT_VERSION: u32 = 1;
/// Versioned scene framing format.
pub const SPARK_SCENE_FRAME_FORMAT_VERSION: u32 = 1;
const MAX_SETTING_CONTRIBUTIONS: usize = 256;
const MAX_SETTING_PARTICIPANTS: usize = 64;

/// Typed value proposed by one setting contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum SparkSettingContributionValue {
    Text {
        text: String,
    },
    Belief {
        statement: String,
    },
    GmAttribute {
        name: String,
        value: u32,
        description: String,
    },
    Faction {
        name: String,
        description: String,
    },
    Tie {
        from_faction: String,
        to_faction: String,
        description: String,
    },
    Face {
        name: String,
        role: String,
        description: String,
        faction_id: Option<String>,
    },
    Agenda {
        name: String,
        description: String,
        faction_id: Option<String>,
    },
    WorldReference {
        target_kind: SparkWorldTargetKind,
        target_id: String,
        description: String,
    },
}

/// One ordered, owned, consented setting contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSettingContribution {
    pub contribution_id: String,
    pub participant_id: String,
    pub path: String,
    pub revision: u32,
    pub consent: bool,
    pub value: SparkSettingContributionValue,
}

/// Complete collaborative setting-creation input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSettingCreation {
    pub format_version: u32,
    pub setting_id: String,
    pub namespace: String,
    pub participants: BTreeMap<String, SparkSettingParticipant>,
    pub contributions: Vec<SparkSettingContribution>,
}

/// One retained contribution revision for a field or entity path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSettingRevision {
    pub contribution_id: String,
    pub participant_id: String,
    pub revision: u32,
    pub consent: bool,
    pub applied: bool,
    pub value: SparkSettingContributionValue,
}

/// One predictable next responsibility for the workshop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkResponsibility {
    pub path: String,
    pub participant_id: String,
}

/// Deterministic result of merging setting contributions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSettingCreationResult {
    pub format_version: u32,
    pub setting_id: String,
    pub completed: bool,
    pub setting: Option<SparkSetting>,
    pub history: BTreeMap<String, Vec<SparkSettingRevision>>,
    pub missing_paths: Vec<String>,
    pub next_responsibilities: Vec<SparkResponsibility>,
    pub canonical_write_back: bool,
}

impl_spark_io!(SparkSettingCreation);
impl_spark_io!(SparkSettingCreationResult);

/// Validate a collaborative setting-creation request.
pub fn validate_spark_setting_creation(
    creation: &SparkSettingCreation,
) -> Result<(), TabletopError> {
    if creation.format_version != SPARK_SETTING_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "format_version",
        });
    }
    validate_global_identifier("setting_id", &creation.setting_id)?;
    if creation.namespace != SPARK_SETTING_NAMESPACE {
        return Err(invalid(
            "namespace",
            "setting namespace must equal the adapter namespace",
        ));
    }
    if creation.participants.is_empty() || creation.participants.len() > MAX_SETTING_PARTICIPANTS {
        return Err(invalid(
            "participants",
            "expected one to 64 setting participants",
        ));
    }
    for (key, participant) in &creation.participants {
        if key != &participant.id {
            return Err(invalid(
                "participants",
                "participant map key must equal the participant id",
            ));
        }
        validate_global_identifier(&format!("participants.{key}.id"), &participant.id)?;
        validate_bounded_text(
            format!("participants.{key}.display_name"),
            &participant.display_name,
            1,
            160,
        )?;
    }
    if creation.contributions.len() > MAX_SETTING_CONTRIBUTIONS {
        return Err(invalid(
            "contributions",
            "contribution collection exceeds 256 entries",
        ));
    }
    let mut ids = BTreeSet::new();
    for (index, contribution) in creation.contributions.iter().enumerate() {
        validate_local_identifier(
            &format!("contributions[{index}].contribution_id"),
            &contribution.contribution_id,
        )?;
        if !ids.insert(&contribution.contribution_id) {
            return Err(invalid("contributions", "duplicate contribution identity"));
        }
        if !creation
            .participants
            .contains_key(&contribution.participant_id)
        {
            return Err(invalid(
                format!("contributions[{index}].participant_id"),
                "contribution owner is not a declared participant",
            ));
        }
        if contribution.revision == 0 {
            return Err(invalid(
                format!("contributions[{index}].revision"),
                "contribution revision must be positive",
            ));
        }
        validate_contribution_path(
            &format!("contributions[{index}].path"),
            &contribution.path,
            &contribution.value,
        )?;
    }
    Ok(())
}

/// Merge contributions deterministically and report completion and next responsibilities.
pub fn resolve_spark_setting_creation(
    creation: &SparkSettingCreation,
) -> Result<SparkSettingCreationResult, TabletopError> {
    validate_spark_setting_creation(creation)?;
    let mut ordered = creation.contributions.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        (
            left.path.as_str(),
            left.revision,
            left.participant_id.as_str(),
            left.contribution_id.as_str(),
        )
            .cmp(&(
                right.path.as_str(),
                right.revision,
                right.participant_id.as_str(),
                right.contribution_id.as_str(),
            ))
    });
    let mut winners: BTreeMap<String, &SparkSettingContribution> = BTreeMap::new();
    for contribution in &ordered {
        if contribution.consent {
            winners.insert(contribution.path.clone(), contribution);
        }
    }
    let mut history: BTreeMap<String, Vec<SparkSettingRevision>> = BTreeMap::new();
    for contribution in &ordered {
        let applied = winners
            .get(&contribution.path)
            .is_some_and(|winner| winner.contribution_id == contribution.contribution_id);
        history
            .entry(contribution.path.clone())
            .or_default()
            .push(SparkSettingRevision {
                contribution_id: contribution.contribution_id.clone(),
                participant_id: contribution.participant_id.clone(),
                revision: contribution.revision,
                consent: contribution.consent,
                applied,
                value: contribution.value.clone(),
            });
    }

    let mut title = None;
    let mut inspirations = BTreeSet::new();
    let mut genres = BTreeSet::new();
    let mut facts = BTreeSet::new();
    let mut setting_beliefs = BTreeMap::new();
    let mut gm_attributes = BTreeMap::new();
    let mut factions = BTreeMap::new();
    let mut ties = BTreeMap::new();
    let mut faces = BTreeMap::new();
    let mut agendas = BTreeMap::new();
    let mut world_references = BTreeMap::new();
    for (path, contribution) in &winners {
        let owner_id = contribution.participant_id.clone();
        let namespace = creation.namespace.clone();
        match (&contribution.value, contribution_path_suffix(path)) {
            (SparkSettingContributionValue::Text { text }, None) if path == "title" => {
                title = Some(text.clone());
            }
            (SparkSettingContributionValue::Text { text }, Some(_))
                if path.starts_with("inspiration.") =>
            {
                inspirations.insert(text.clone());
            }
            (SparkSettingContributionValue::Text { text }, Some(_))
                if path.starts_with("genre.") =>
            {
                genres.insert(text.clone());
            }
            (SparkSettingContributionValue::Text { text }, Some(_))
                if path.starts_with("fact.") =>
            {
                facts.insert(text.clone());
            }
            (SparkSettingContributionValue::Belief { statement }, Some(id))
                if path.starts_with("belief.") =>
            {
                setting_beliefs.insert(
                    id.to_owned(),
                    SparkSettingBelief {
                        id: id.to_owned(),
                        namespace: namespace.clone(),
                        owner_id,
                        statement: statement.clone(),
                    },
                );
            }
            (
                SparkSettingContributionValue::GmAttribute {
                    name,
                    value,
                    description,
                },
                Some(id),
            ) if path.starts_with("gm_attribute.") => {
                gm_attributes.insert(
                    id.to_owned(),
                    SparkGmAttribute {
                        id: id.to_owned(),
                        namespace: namespace.clone(),
                        owner_id,
                        name: name.clone(),
                        value: *value,
                        description: description.clone(),
                    },
                );
            }
            (SparkSettingContributionValue::Faction { name, description }, Some(id))
                if path.starts_with("faction.") =>
            {
                factions.insert(
                    id.to_owned(),
                    SparkFaction {
                        id: id.to_owned(),
                        namespace: namespace.clone(),
                        owner_id,
                        name: name.clone(),
                        description: description.clone(),
                    },
                );
            }
            (
                SparkSettingContributionValue::Tie {
                    from_faction,
                    to_faction,
                    description,
                },
                Some(id),
            ) if path.starts_with("tie.") => {
                ties.insert(
                    id.to_owned(),
                    SparkTie {
                        id: id.to_owned(),
                        namespace: namespace.clone(),
                        owner_id,
                        from_faction: from_faction.clone(),
                        to_faction: to_faction.clone(),
                        description: description.clone(),
                    },
                );
            }
            (
                SparkSettingContributionValue::Face {
                    name,
                    role,
                    description,
                    faction_id,
                },
                Some(id),
            ) if path.starts_with("face.") => {
                faces.insert(
                    id.to_owned(),
                    SparkFace {
                        id: id.to_owned(),
                        namespace: namespace.clone(),
                        owner_id,
                        name: name.clone(),
                        role: role.clone(),
                        description: description.clone(),
                        faction_id: faction_id.clone(),
                    },
                );
            }
            (
                SparkSettingContributionValue::Agenda {
                    name,
                    description,
                    faction_id,
                },
                Some(id),
            ) if path.starts_with("agenda.") => {
                agendas.insert(
                    id.to_owned(),
                    SparkAgenda {
                        id: id.to_owned(),
                        namespace: namespace.clone(),
                        owner_id,
                        name: name.clone(),
                        description: description.clone(),
                        faction_id: faction_id.clone(),
                    },
                );
            }
            (
                SparkSettingContributionValue::WorldReference {
                    target_kind,
                    target_id,
                    description,
                },
                Some(id),
            ) if path.starts_with("world_reference.") => {
                world_references.insert(
                    id.to_owned(),
                    SparkWorldReference {
                        id: id.to_owned(),
                        namespace: namespace.clone(),
                        owner_id,
                        target_kind: *target_kind,
                        target_id: target_id.clone(),
                        description: description.clone(),
                    },
                );
            }
            _ => {
                return Err(invalid(
                    format!("contributions.{path}"),
                    "contribution value does not match its path",
                ));
            }
        }
    }

    let mut missing_paths = Vec::new();
    if title.is_none() {
        missing_paths.push("title".to_owned());
    }
    if setting_beliefs.len() < MAX_SETTING_BELIEFS {
        missing_paths.push("belief".to_owned());
    }
    let mut next_responsibilities = Vec::new();
    let participant_ids = creation.participants.keys().cloned().collect::<Vec<_>>();
    for (index, path) in missing_paths.iter().enumerate() {
        if let Some(participant_id) = participant_ids.get(index % participant_ids.len()) {
            next_responsibilities.push(SparkResponsibility {
                path: path.clone(),
                participant_id: participant_id.clone(),
            });
        }
    }

    let setting = if missing_paths.is_empty() {
        let setting = SparkSetting {
            format_version: SPARK_SETTING_FORMAT_VERSION,
            id: creation.setting_id.clone(),
            namespace: creation.namespace.clone(),
            title: title.ok_or_else(|| invalid("title", "completed setting requires a title"))?,
            inspirations,
            genres,
            facts,
            setting_beliefs,
            gm_attributes,
            factions,
            ties,
            faces,
            agendas,
            world_references,
        };
        validate_spark_setting(&setting)?;
        Some(setting)
    } else {
        None
    };
    Ok(SparkSettingCreationResult {
        format_version: SPARK_SETTING_CREATION_FORMAT_VERSION,
        setting_id: creation.setting_id.clone(),
        completed: setting.is_some(),
        setting,
        history,
        missing_paths,
        next_responsibilities,
        canonical_write_back: false,
    })
}

fn validate_contribution_path(
    path: &str,
    value_path: &str,
    value: &SparkSettingContributionValue,
) -> Result<(), TabletopError> {
    validate_bounded_text(path, value_path, 1, 160)?;
    let (prefix, suffix) = match value_path.split_once('.') {
        Some((prefix, suffix)) => (prefix, Some(suffix)),
        None => (value_path, None),
    };
    let expected = match prefix {
        "title" if suffix.is_none() => matches!(value, SparkSettingContributionValue::Text { .. }),
        "inspiration" | "genre" | "fact" if suffix.is_some() => {
            matches!(value, SparkSettingContributionValue::Text { .. })
        }
        "belief" if suffix.is_some() => {
            matches!(value, SparkSettingContributionValue::Belief { .. })
        }
        "gm_attribute" if suffix.is_some() => {
            matches!(value, SparkSettingContributionValue::GmAttribute { .. })
        }
        "faction" if suffix.is_some() => {
            matches!(value, SparkSettingContributionValue::Faction { .. })
        }
        "tie" if suffix.is_some() => matches!(value, SparkSettingContributionValue::Tie { .. }),
        "face" if suffix.is_some() => matches!(value, SparkSettingContributionValue::Face { .. }),
        "agenda" if suffix.is_some() => {
            matches!(value, SparkSettingContributionValue::Agenda { .. })
        }
        "world_reference" if suffix.is_some() => {
            matches!(value, SparkSettingContributionValue::WorldReference { .. })
        }
        _ => {
            return Err(invalid(path, "unknown setting contribution path"));
        }
    };
    if !expected {
        return Err(invalid(
            path,
            "contribution value kind does not match its path",
        ));
    }
    if let Some(suffix) = suffix {
        validate_local_identifier(&format!("{path}.suffix"), suffix)?;
    }
    validate_contribution_value(path, value)
}

fn validate_contribution_value(
    path: &str,
    value: &SparkSettingContributionValue,
) -> Result<(), TabletopError> {
    match value {
        SparkSettingContributionValue::Text { text } => {
            validate_bounded_text(format!("{path}.text"), text, 1, 512)
        }
        SparkSettingContributionValue::Belief { statement } => {
            validate_bounded_text(format!("{path}.statement"), statement, 1, 512)
        }
        SparkSettingContributionValue::GmAttribute {
            name,
            value,
            description,
        } => {
            validate_bounded_text(format!("{path}.name"), name, 1, 160)?;
            if *value > MAX_GM_ATTRIBUTE_VALUE {
                return Err(invalid(
                    format!("{path}.value"),
                    "GM attribute exceeds the reviewed domain",
                ));
            }
            validate_bounded_text(format!("{path}.description"), description, 0, 512)
        }
        SparkSettingContributionValue::Faction { name, description } => {
            validate_bounded_text(format!("{path}.name"), name, 1, 160)?;
            validate_bounded_text(format!("{path}.description"), description, 0, 512)
        }
        SparkSettingContributionValue::Tie {
            from_faction,
            to_faction,
            description,
        } => {
            validate_local_identifier(&format!("{path}.from_faction"), from_faction)?;
            validate_local_identifier(&format!("{path}.to_faction"), to_faction)?;
            validate_bounded_text(format!("{path}.description"), description, 0, 512)
        }
        SparkSettingContributionValue::Face {
            name,
            role,
            description,
            faction_id,
        } => {
            validate_bounded_text(format!("{path}.name"), name, 1, 160)?;
            validate_bounded_text(format!("{path}.role"), role, 1, 160)?;
            validate_bounded_text(format!("{path}.description"), description, 0, 512)?;
            if let Some(faction_id) = faction_id {
                validate_local_identifier(&format!("{path}.faction_id"), faction_id)?;
            }
            Ok(())
        }
        SparkSettingContributionValue::Agenda {
            name,
            description,
            faction_id,
        } => {
            validate_bounded_text(format!("{path}.name"), name, 1, 160)?;
            validate_bounded_text(format!("{path}.description"), description, 0, 512)?;
            if let Some(faction_id) = faction_id {
                validate_local_identifier(&format!("{path}.faction_id"), faction_id)?;
            }
            Ok(())
        }
        SparkSettingContributionValue::WorldReference {
            target_id,
            description,
            ..
        } => {
            validate_global_identifier(&format!("{path}.target_id"), target_id)?;
            validate_bounded_text(format!("{path}.description"), description, 0, 512)
        }
    }
}

fn contribution_path_suffix(path: &str) -> Option<&str> {
    path.split_once('.').map(|(_, suffix)| suffix)
}

/// Explicit scene-framing input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSceneFrameRequest {
    pub format_version: u32,
    pub scene_id: String,
    pub platform: String,
    pub tilt: String,
    pub question: String,
    pub end_condition: String,
    pub contributors: BTreeMap<String, String>,
}

/// Validated scene frame ready to feed the existing conflict loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkSceneFrame {
    pub format_version: u32,
    pub scene_id: String,
    pub platform: String,
    pub tilt: String,
    pub question: String,
    pub end_condition: String,
    pub contributors: BTreeMap<String, String>,
    pub next_actor_id: String,
}

impl_spark_io!(SparkSceneFrameRequest);
impl_spark_io!(SparkSceneFrame);

impl SparkSceneFrame {
    /// Build the exact `open_conflict` input for the existing Spark resolver.
    #[must_use]
    pub fn open_conflict_input(
        &self,
        conflict_id: &str,
        max_escalation: u32,
        victory_cost_kind: SparkCostKind,
        victory_cost_amount: u32,
        victory_cost_attribute: &str,
    ) -> DomainValue {
        object([
            ("conflict_id", DomainValue::String(conflict_id.to_owned())),
            (
                "max_escalation",
                DomainValue::Number(f64::from(max_escalation)),
            ),
            ("stakes", DomainValue::String(self.question.clone())),
            (
                "victory_cost_amount",
                DomainValue::Number(f64::from(victory_cost_amount)),
            ),
            (
                "victory_cost_attribute",
                DomainValue::String(victory_cost_attribute.to_owned()),
            ),
            (
                "victory_cost_kind",
                DomainValue::Symbol(victory_cost_kind.symbol().to_owned()),
            ),
        ])
    }
}

/// Validate and frame one scene with explicit contributor ownership.
pub fn frame_spark_scene(
    request: &SparkSceneFrameRequest,
    participants: &BTreeMap<String, SparkSettingParticipant>,
) -> Result<SparkSceneFrame, TabletopError> {
    if request.format_version != SPARK_SCENE_FRAME_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "format_version",
        });
    }
    validate_local_identifier("scene_id", &request.scene_id)?;
    validate_bounded_text("platform", &request.platform, 1, 240)?;
    validate_bounded_text("tilt", &request.tilt, 1, 240)?;
    validate_bounded_text("question", &request.question, 1, 512)?;
    validate_bounded_text("end_condition", &request.end_condition, 1, 512)?;
    if request.contributors.len() > 16 {
        return Err(invalid(
            "contributors",
            "scene frame exceeds the reviewed contributor domain",
        ));
    }
    for field in ["platform", "tilt", "question", "end_condition"] {
        let Some(contributor) = request.contributors.get(field) else {
            return Err(invalid(
                format!("contributors.{field}"),
                "framed scene requires an explicit contributor",
            ));
        };
        if !participants.contains_key(contributor) {
            return Err(invalid(
                format!("contributors.{field}"),
                "scene contributor is not a declared participant",
            ));
        }
    }
    let question_contributor = request
        .contributors
        .get("question")
        .cloned()
        .ok_or_else(|| invalid("contributors.question", "question contributor is missing"))?;
    let next_actor_id = participants
        .keys()
        .find(|participant| participant.as_str() != question_contributor)
        .cloned()
        .unwrap_or(question_contributor);
    Ok(SparkSceneFrame {
        format_version: SPARK_SCENE_FRAME_FORMAT_VERSION,
        scene_id: request.scene_id.clone(),
        platform: request.platform.clone(),
        tilt: request.tilt.clone(),
        question: request.question.clone(),
        end_condition: request.end_condition.clone(),
        contributors: request.contributors.clone(),
        next_actor_id,
    })
}

// ---------------------------------------------------------------------------
// Belief replacement, Growth, and setting Evolution.
// ---------------------------------------------------------------------------

/// One retained Belief replacement in sequence order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkBeliefLineage {
    pub replacement_id: String,
    pub belief_id: String,
    pub old_statement: String,
    pub new_statement: String,
    pub inspiration: String,
    pub confirmed_by: Vec<String>,
    pub growth_awarded: bool,
    pub rationale: String,
    pub sequence: u32,
}

impl_spark_io!(SparkBeliefLineage);

fn resolve_replace_belief(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let belief_id = object_string(fields, "belief_id")?.to_owned();
    let replacement_id = object_string(fields, "replacement_id")?.to_owned();
    let new_statement = object_string(fields, "new_statement")?.to_owned();
    let inspiration = object_string(fields, "inspiration")?.to_owned();
    let rationale = object_string(fields, "rationale")?.to_owned();
    validate_local_identifier("input.belief_id", &belief_id)?;
    validate_local_identifier("input.replacement_id", &replacement_id)?;
    validate_bounded_text("input.new_statement", &new_statement, 1, 512)?;
    validate_bounded_text("input.inspiration", &inspiration, 1, 512)?;
    validate_bounded_text("input.rationale", &rationale, 1, 512)?;
    let confirmed_by = string_values("input.confirmed_by", object_list(fields, "confirmed_by")?)?;
    if confirmed_by.is_empty() || confirmed_by.len() > 16 {
        return Err(invalid(
            "input.confirmed_by",
            "expected one to 16 confirming participants",
        ));
    }
    let mut confirmed_by = confirmed_by;
    confirmed_by.sort();
    confirmed_by.dedup();
    if confirmed_by.len() != object_list(fields, "confirmed_by")?.len() {
        return Err(invalid(
            "input.confirmed_by",
            "duplicate confirming participant",
        ));
    }
    for participant in &confirmed_by {
        validate_global_identifier("input.confirmed_by", participant)?;
    }
    let definition_fields = value_object("definition", definition)?;
    let definition_beliefs = object_map(definition_fields, "beliefs")?;
    let definition_belief = value_object(
        "definition.beliefs",
        object_field(definition_beliefs, &belief_id)?,
    )?;
    let definition_statement = object_string(definition_belief, "statement")?.to_owned();
    let state_fields = value_object("state", state)?;
    let beliefs = object_map(state_fields, "beliefs")?;
    let belief_state = value_object("state.beliefs", object_field(beliefs, &belief_id)?)?;
    let status = match object_symbol(belief_state, "status")? {
        "confirmed" => SparkBeliefStatus::Confirmed,
        "held" => SparkBeliefStatus::Held,
        "refuted" => SparkBeliefStatus::Refuted,
        _ => {
            return Err(invalid("state.beliefs.status", "unknown Belief status"));
        }
    };
    let replacements = object_map(state_fields, "belief_replacements")?;
    if replacements.contains_key(&replacement_id) {
        return Err(invalid(
            "input.replacement_id",
            "replacement identity already exists",
        ));
    }
    let mut latest_sequence = 0_u32;
    let mut old_statement = definition_statement;
    for value in replacements.values() {
        let record = value_object("state.belief_replacements", value)?;
        if object_string(record, "belief_id")? == belief_id {
            let sequence = object_u32(record, "sequence")?;
            if sequence > latest_sequence {
                latest_sequence = sequence;
                old_statement = object_string(record, "new_statement")?.to_owned();
            }
        }
    }
    let sequence = latest_sequence
        .checked_add(1)
        .ok_or_else(|| invalid("input.replacement_id", "replacement sequence overflow"))?;
    if sequence > 64 {
        return Err(invalid(
            "state.belief_replacements",
            "replacement lineage exceeds the reviewed domain",
        ));
    }
    let growth_awarded = status == SparkBeliefStatus::Refuted;
    let growth_before = object_u32(state_fields, "growth")?;
    let growth_after = if growth_awarded {
        growth_before
            .checked_add(1)
            .ok_or_else(|| invalid("state.growth", "Growth overflow"))?
    } else {
        growth_before
    };
    if growth_after > 99 {
        return Err(invalid(
            "state.growth",
            "Growth exceeds the reviewed domain",
        ));
    }
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let replacements = object_map_mut(state_fields, "belief_replacements")?;
    replacements.insert(
        replacement_id.clone(),
        object([
            ("belief_id", DomainValue::String(belief_id.clone())),
            ("confirmed_by", string_list_value(&confirmed_by)),
            ("growth_awarded", DomainValue::Bool(growth_awarded)),
            ("inspiration", DomainValue::String(inspiration.clone())),
            ("new_statement", DomainValue::String(new_statement.clone())),
            ("old_statement", DomainValue::String(old_statement.clone())),
            ("rationale", DomainValue::String(rationale.clone())),
            (
                "replacement_id",
                DomainValue::String(replacement_id.clone()),
            ),
            ("sequence", DomainValue::Number(f64::from(sequence))),
        ]),
    );
    let beliefs = object_map_mut(state_fields, "beliefs")?;
    let belief_state = beliefs
        .get_mut(&belief_id)
        .ok_or_else(|| invalid("state.beliefs", "Belief state disappeared"))?;
    let belief_state = value_object_mut("state.beliefs", belief_state)?;
    belief_state.insert(
        "status".to_owned(),
        DomainValue::Symbol(SparkBeliefStatus::Held.symbol().to_owned()),
    );
    belief_state.insert(
        "rationale".to_owned(),
        DomainValue::String(rationale.clone()),
    );
    if growth_awarded {
        set_u32(state_fields, "growth", growth_after);
    }
    let mut events = vec![public_event(
        "belief_replaced",
        object([
            ("belief_id", DomainValue::String(belief_id)),
            ("canonical_write_back", DomainValue::Bool(false)),
            ("confirmed_by", string_list_value(&confirmed_by)),
            ("growth_awarded", DomainValue::Bool(growth_awarded)),
            ("inspiration", DomainValue::String(inspiration)),
            ("new_statement", DomainValue::String(new_statement)),
            ("old_statement", DomainValue::String(old_statement)),
            ("rationale", DomainValue::String(rationale)),
            ("replacement_id", DomainValue::String(replacement_id)),
        ]),
    )];
    if growth_awarded {
        events.push(public_event(
            "growth_changed",
            object([
                ("after", DomainValue::Number(f64::from(growth_after))),
                ("before", DomainValue::Number(f64::from(growth_before))),
                ("delta", DomainValue::Number(1.0)),
                (
                    "reason",
                    DomainValue::String(
                        "A confirmed Belief replacement followed a refuted Belief.".to_owned(),
                    ),
                ),
            ]),
        ));
    }
    Ok((next, events))
}

/// Read the complete retained replacement lineage for one Belief.
pub fn spark_belief_lineage(
    definition: &DomainValue,
    state: &DomainValue,
    belief_id: &str,
) -> Result<Vec<SparkBeliefLineage>, TabletopError> {
    validate_local_identifier("belief_id", belief_id)?;
    let definition_fields = value_object("definition", definition)?;
    let definition_beliefs = object_map(definition_fields, "beliefs")?;
    if !definition_beliefs.contains_key(belief_id) {
        return Err(invalid("belief_id", "Belief is absent from the definition"));
    }
    let state_fields = value_object("state", state)?;
    let replacements = object_map(state_fields, "belief_replacements")?;
    let mut lineage = Vec::new();
    for value in replacements.values() {
        let record = value_object("state.belief_replacements", value)?;
        if object_string(record, "belief_id")? != belief_id {
            continue;
        }
        lineage.push(SparkBeliefLineage {
            replacement_id: object_string(record, "replacement_id")?.to_owned(),
            belief_id: belief_id.to_owned(),
            old_statement: object_string(record, "old_statement")?.to_owned(),
            new_statement: object_string(record, "new_statement")?.to_owned(),
            inspiration: object_string(record, "inspiration")?.to_owned(),
            confirmed_by: string_values(
                "state.belief_replacements.confirmed_by",
                object_list(record, "confirmed_by")?,
            )?,
            growth_awarded: object_bool(record, "growth_awarded")?,
            rationale: object_string(record, "rationale")?.to_owned(),
            sequence: object_u32(record, "sequence")?,
        });
    }
    lineage.sort_by_key(|entry| entry.sequence);
    Ok(lineage)
}

/// Read the effective Belief statement after every retained replacement.
pub fn spark_effective_belief_statement(
    definition: &DomainValue,
    state: &DomainValue,
    belief_id: &str,
) -> Result<String, TabletopError> {
    let definition_fields = value_object("definition", definition)?;
    let definition_beliefs = object_map(definition_fields, "beliefs")?;
    let belief = value_object(
        "definition.beliefs",
        object_field(definition_beliefs, belief_id)?,
    )?;
    let mut statement = object_string(belief, "statement")?.to_owned();
    for entry in spark_belief_lineage(definition, state, belief_id)? {
        statement = entry.new_statement;
    }
    Ok(statement)
}

/// One condition that must hold before setting Evolution may apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum SparkEvolutionCondition {
    BeliefRefuted { belief_id: String },
    GrowthAtLeast { amount: u32 },
    FactionExists { faction_id: String },
    SceneClosed { scene_id: String },
}

impl SparkEvolutionCondition {
    /// Stable condition identifier used in diagnostics and review records.
    #[must_use]
    pub fn id(&self) -> String {
        match self {
            Self::BeliefRefuted { belief_id } => format!("belief_refuted.{belief_id}"),
            Self::GrowthAtLeast { amount } => format!("growth_at_least.{amount}"),
            Self::FactionExists { faction_id } => format!("faction_exists.{faction_id}"),
            Self::SceneClosed { scene_id } => format!("scene_closed.{scene_id}"),
        }
    }

    fn satisfied(&self, context: &SparkEvolutionContext) -> bool {
        match self {
            Self::BeliefRefuted { belief_id } => context.refuted_beliefs.contains(belief_id),
            Self::GrowthAtLeast { amount } => context.growth >= *amount,
            Self::FactionExists { faction_id } => context.faction_ids.contains(faction_id),
            Self::SceneClosed { scene_id } => context.closed_scenes.contains(scene_id),
        }
    }
}

/// Facts available to setting Evolution conditions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkEvolutionContext {
    pub refuted_beliefs: BTreeSet<String>,
    pub growth: u32,
    pub faction_ids: BTreeSet<String>,
    pub closed_scenes: BTreeSet<String>,
}

/// One reviewable change proposed by setting Evolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum SparkEvolutionChange {
    SetSettingBelief {
        belief_id: String,
        statement: String,
    },
    SetGmAttribute {
        attribute_id: String,
        value: u32,
    },
    SetFactionName {
        faction_id: String,
        name: String,
    },
    SetFactionDescription {
        faction_id: String,
        description: String,
    },
}

impl SparkEvolutionChange {
    /// Stable path used in the review record.
    #[must_use]
    pub fn path(&self) -> String {
        match self {
            Self::SetSettingBelief { belief_id, .. } => {
                format!("setting_beliefs.{belief_id}")
            }
            Self::SetGmAttribute { attribute_id, .. } => {
                format!("gm_attributes.{attribute_id}")
            }
            Self::SetFactionName { faction_id, .. } => {
                format!("factions.{faction_id}.name")
            }
            Self::SetFactionDescription { faction_id, .. } => {
                format!("factions.{faction_id}.description")
            }
        }
    }
}

/// Explicit, reviewable setting Evolution request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkEvolutionRequest {
    pub format_version: u32,
    pub evolution_id: String,
    pub setting_id: String,
    pub conditions: Vec<SparkEvolutionCondition>,
    pub changes: Vec<SparkEvolutionChange>,
    pub rationale: String,
    pub accepted: bool,
    pub accepted_by: Vec<String>,
}

/// One before/after change retained in the Evolution review record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkEvolutionChangeRecord {
    pub path: String,
    pub before: String,
    pub after: String,
    pub rationale: String,
}

/// Reviewable Evolution outcome. It never mutates the supplied setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparkEvolutionResult {
    pub format_version: u32,
    pub evolution_id: String,
    pub applied: bool,
    pub failed_conditions: Vec<String>,
    pub accepted: bool,
    pub canonical_write_back: bool,
    pub setting: Option<SparkSetting>,
    pub changes: Vec<SparkEvolutionChangeRecord>,
}

impl_spark_io!(SparkEvolutionRequest);
impl_spark_io!(SparkEvolutionResult);

/// Versioned setting Evolution format.
pub const SPARK_EVOLUTION_FORMAT_VERSION: u32 = 1;

/// Apply explicit Evolution changes when every condition holds and the proposal is accepted.
///
/// The supplied setting is never mutated. The result carries a complete replacement setting and
/// one review record per change; `canonical_write_back` is always false.
pub fn evolve_spark_setting(
    setting: &SparkSetting,
    request: &SparkEvolutionRequest,
    context: &SparkEvolutionContext,
) -> Result<SparkEvolutionResult, TabletopError> {
    if request.format_version != SPARK_EVOLUTION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "format_version",
        });
    }
    validate_local_identifier("evolution_id", &request.evolution_id)?;
    validate_global_identifier("setting_id", &request.setting_id)?;
    if request.setting_id != setting.id {
        return Err(invalid(
            "setting_id",
            "Evolution request does not match the supplied setting",
        ));
    }
    validate_bounded_text("rationale", &request.rationale, 1, 512)?;
    if request.conditions.len() > 64 || request.changes.len() > 64 {
        return Err(invalid(
            "evolution",
            "Evolution request exceeds the reviewed domain",
        ));
    }
    if request.accepted {
        if request.accepted_by.is_empty() || request.accepted_by.len() > 16 {
            return Err(invalid(
                "accepted_by",
                "accepted Evolution requires one to 16 participants",
            ));
        }
        let mut accepted_by = request.accepted_by.clone();
        accepted_by.sort();
        accepted_by.dedup();
        if accepted_by.len() != request.accepted_by.len() {
            return Err(invalid("accepted_by", "duplicate accepting participant"));
        }
        for participant in &accepted_by {
            validate_global_identifier("accepted_by", participant)?;
        }
    }
    let mut failed_conditions = Vec::new();
    for condition in &request.conditions {
        if !condition.satisfied(context) {
            failed_conditions.push(condition.id());
        }
    }
    if failed_conditions.len() > 1 {
        failed_conditions.sort();
    }
    if !request.accepted || !failed_conditions.is_empty() {
        return Ok(SparkEvolutionResult {
            format_version: SPARK_EVOLUTION_FORMAT_VERSION,
            evolution_id: request.evolution_id.clone(),
            applied: false,
            failed_conditions,
            accepted: request.accepted,
            canonical_write_back: false,
            setting: None,
            changes: Vec::new(),
        });
    }
    let mut evolved = setting.clone();
    let mut records = Vec::with_capacity(request.changes.len());
    for change in &request.changes {
        let record = match change {
            SparkEvolutionChange::SetSettingBelief {
                belief_id,
                statement,
            } => {
                validate_local_identifier("changes.belief_id", belief_id)?;
                validate_bounded_text("changes.statement", statement, 1, 512)?;
                let Some(belief) = evolved.setting_beliefs.get_mut(belief_id) else {
                    return Err(invalid(
                        "changes.belief_id",
                        "setting Belief does not exist",
                    ));
                };
                let before = belief.statement.clone();
                belief.statement = statement.clone();
                SparkEvolutionChangeRecord {
                    path: change.path(),
                    before,
                    after: statement.clone(),
                    rationale: request.rationale.clone(),
                }
            }
            SparkEvolutionChange::SetGmAttribute {
                attribute_id,
                value,
            } => {
                validate_local_identifier("changes.attribute_id", attribute_id)?;
                if *value > MAX_GM_ATTRIBUTE_VALUE {
                    return Err(invalid(
                        "changes.value",
                        "GM attribute exceeds the reviewed domain",
                    ));
                }
                let Some(attribute) = evolved.gm_attributes.get_mut(attribute_id) else {
                    return Err(invalid(
                        "changes.attribute_id",
                        "GM attribute does not exist",
                    ));
                };
                let before = attribute.value.to_string();
                attribute.value = *value;
                SparkEvolutionChangeRecord {
                    path: change.path(),
                    before,
                    after: value.to_string(),
                    rationale: request.rationale.clone(),
                }
            }
            SparkEvolutionChange::SetFactionName { faction_id, name } => {
                validate_local_identifier("changes.faction_id", faction_id)?;
                validate_bounded_text("changes.name", name, 1, 160)?;
                let Some(faction) = evolved.factions.get_mut(faction_id) else {
                    return Err(invalid("changes.faction_id", "Faction does not exist"));
                };
                let before = faction.name.clone();
                faction.name = name.clone();
                SparkEvolutionChangeRecord {
                    path: change.path(),
                    before,
                    after: name.clone(),
                    rationale: request.rationale.clone(),
                }
            }
            SparkEvolutionChange::SetFactionDescription {
                faction_id,
                description,
            } => {
                validate_local_identifier("changes.faction_id", faction_id)?;
                validate_bounded_text("changes.description", description, 0, 512)?;
                let Some(faction) = evolved.factions.get_mut(faction_id) else {
                    return Err(invalid("changes.faction_id", "Faction does not exist"));
                };
                let before = faction.description.clone();
                faction.description = description.clone();
                SparkEvolutionChangeRecord {
                    path: change.path(),
                    before,
                    after: description.clone(),
                    rationale: request.rationale.clone(),
                }
            }
        };
        records.push(record);
    }
    validate_spark_setting(&evolved)?;
    Ok(SparkEvolutionResult {
        format_version: SPARK_EVOLUTION_FORMAT_VERSION,
        evolution_id: request.evolution_id.clone(),
        applied: true,
        failed_conditions: Vec::new(),
        accepted: true,
        canonical_write_back: false,
        setting: Some(evolved),
        changes: records,
    })
}
