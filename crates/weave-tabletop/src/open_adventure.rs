//! Open Adventure: Fantasy tabletop adapter.
//!
//! This is an independently authored implementation of the public `Community-Owned Content`
//! core rules for Open Adventure: Fantasy. It models authored character choices, deterministic
//! derived values, exploding action dice, Benefits and Drawbacks, and challenge completion against
//! environmental and NPC opposition. Fixtures use synthetic names and original prose. No upstream
//! rule text, setting prose, logo, artwork, or Masterwork Edition layout is redistributed.
//!
//! The manifest's primary provenance digest is the SHA-256 of
//! [`OPEN_ADVENTURE_SOURCE_RECORD`], an in-repository record of the canonical URL, implemented
//! edition, and licence. It is deliberately not the digest of a downloaded upstream file; this
//! adapter was specified from the published licence and content boundary and does not bundle an
//! upstream artifact.
//!
//! The publisher states that its Community-Owned Content is released under CC0 without asserting
//! a version. The repository's fail-closed licence gate admits only the SPDX allowlist token
//! `CC0-1.0` for verified public inputs, so the manifest records that token while the source
//! record and attribution state the unversioned `CC0` licence exactly as published.

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
    TabletopState, canonical_fingerprint, resolved_adapter, sha256_bytes,
    validate_adapter_manifest, validate_tabletop_state,
};

/// Stable adapter identity shared by source, editor, runtime, and host integrations.
pub const OPEN_ADVENTURE_ADAPTER_ID: &str = "org.weave.tabletop.open_adventure";
/// First independently modeled adapter release for Open Adventure: Fantasy.
pub const OPEN_ADVENTURE_ADAPTER_VERSION: &str = "1.0.0";
/// Versioned character creation request and preview format.
pub const OPEN_ADVENTURE_CREATION_FORMAT_VERSION: u32 = 1;
/// Canonical public project URL for the reviewed Community-Owned Content.
pub const OPEN_ADVENTURE_SOURCE_URL: &str = "https://openadventurerpg.com/";
/// Edition implemented by this adapter release.
pub const OPEN_ADVENTURE_EDITION: &str =
    "Open Adventure: Fantasy core rules (Community-Owned Content)";
/// Publisher's unversioned licence statement for the reviewed content.
pub const OPEN_ADVENTURE_LICENSE_STATEMENT: &str = "CC0";
/// Repository allowlist token required for a verified CC0 source.
pub const OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN: &str = "CC0-1.0";
/// SHA-256 of the official plain-text CC0 legal code retained with the fixture set.
pub const OPEN_ADVENTURE_CC0_LEGAL_CODE_SHA256: &str =
    "a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499";
/// Exact in-repository provenance record hashed by the manifest.
///
/// This string names the canonical project URL, the implemented edition, and the publisher's
/// licence statement. It is not a substitute for a downloaded upstream artifact.
pub const OPEN_ADVENTURE_SOURCE_RECORD: &str = "Open Adventure: Fantasy, Community-Owned Content core rules by Marc Tassin (Mechanical Muse); canonical project URL https://openadventurerpg.com/; edition implemented: Open Adventure: Fantasy core rules; licence: CC0; this record is the hashed artifact.";
/// SHA-256 of [`OPEN_ADVENTURE_SOURCE_RECORD`].
pub const OPEN_ADVENTURE_SOURCE_RECORD_SHA256: &str =
    "caebd872fa0340b543737f55bf21581316e4cfa74ebc1cbf153b997c3ca8f450";

const FAVORED_ACTION_BONUS: i32 = 2;
const MAX_ATTRIBUTE_VALUE: u32 = 5;
const MAX_SPECIALIZATION_BONUS: u32 = 3;
const MAX_MODUS_OPERANDI_SCORE: u32 = 5;
const MAX_TRAIT_MODIFIER: i32 = 10;
const MAX_SPECIALIZATIONS: usize = 9;
const MAX_DETAIL_ENTRIES: usize = 32;
const MAX_CHALLENGES: usize = 64;
const MAX_EXPLOSIONS_PER_DIE: usize = 32;
const MAX_DIE_TOTAL: u64 = 100;
const MAX_ROLL_TOTAL: u64 = 500;

/// One of the nine Body, Mind, and Spirit attributes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureAttribute {
    Agility,
    Empathy,
    Endurance,
    Focus,
    Insight,
    Might,
    Presence,
    Reasoning,
    Will,
}

impl OpenAdventureAttribute {
    /// Every attribute in canonical symbol order.
    pub const ALL: [Self; 9] = [
        Self::Agility,
        Self::Empathy,
        Self::Endurance,
        Self::Focus,
        Self::Insight,
        Self::Might,
        Self::Presence,
        Self::Reasoning,
        Self::Will,
    ];

    /// Stable lowercase symbol used by manifests and events.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agility => "agility",
            Self::Empathy => "empathy",
            Self::Endurance => "endurance",
            Self::Focus => "focus",
            Self::Insight => "insight",
            Self::Might => "might",
            Self::Presence => "presence",
            Self::Reasoning => "reasoning",
            Self::Will => "will",
        }
    }

    /// Body, Mind, or Spirit family for explanatory grouping.
    #[must_use]
    pub const fn group(self) -> OpenAdventureAttributeGroup {
        match self {
            Self::Agility | Self::Endurance | Self::Might => OpenAdventureAttributeGroup::Body,
            Self::Focus | Self::Insight | Self::Reasoning => OpenAdventureAttributeGroup::Mind,
            Self::Empathy | Self::Presence | Self::Will => OpenAdventureAttributeGroup::Spirit,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Agility => 0,
            Self::Empathy => 1,
            Self::Endurance => 2,
            Self::Focus => 3,
            Self::Insight => 4,
            Self::Might => 5,
            Self::Presence => 6,
            Self::Reasoning => 7,
            Self::Will => 8,
        }
    }
}

/// Explanatory attribute family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureAttributeGroup {
    Body,
    Mind,
    Spirit,
}

/// The nine authored attribute ratings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureAttributes {
    pub agility: u32,
    pub empathy: u32,
    pub endurance: u32,
    pub focus: u32,
    pub insight: u32,
    pub might: u32,
    pub presence: u32,
    pub reasoning: u32,
    pub will: u32,
}

/// A lineage, culture, or background selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureOrigin {
    pub id: String,
    pub label: String,
    pub note: String,
}

/// The character's Modus Operandi and its authored score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureModusOperandi {
    pub id: String,
    pub label: String,
    pub score: u32,
}

/// One specialization and the attribute it improves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureSpecialization {
    pub id: String,
    pub label: String,
    pub attribute: OpenAdventureAttribute,
    pub bonus: u32,
}

/// One named action that is favored for its attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureFavoredAction {
    pub id: String,
    pub label: String,
    pub attribute: OpenAdventureAttribute,
}

/// Equipment reference category. Full item rules stay outside this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureEquipmentKind {
    Armor,
    Tool,
    Weapon,
}

impl OpenAdventureEquipmentKind {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Armor => "armor",
            Self::Tool => "tool",
            Self::Weapon => "weapon",
        }
    }
}

/// One equipment reference carried by the character definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureEquipment {
    pub id: String,
    pub label: String,
    pub kind: OpenAdventureEquipmentKind,
}

/// Derived value changed by one Benefit or Drawback.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureModifierTarget {
    ActionScore,
    BodyPoints,
    Defense,
    MindPoints,
    Reaction,
    SpiritPoints,
}

impl OpenAdventureModifierTarget {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::ActionScore => "action_score",
            Self::BodyPoints => "body_points",
            Self::Defense => "defense",
            Self::MindPoints => "mind_points",
            Self::Reaction => "reaction",
            Self::SpiritPoints => "spirit_points",
        }
    }
}

/// One Benefit or Drawback with a signed modifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureTraitModifier {
    pub id: String,
    pub label: String,
    pub target: OpenAdventureModifierTarget,
    pub modifier: i32,
}

/// Complete authored Open Adventure character request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureCreationRequest {
    pub creation_format_version: u32,
    pub character_id: String,
    pub name: String,
    /// Deterministic resolver seed; creation itself consumes no entropy.
    pub seed: u64,
    pub lineage: OpenAdventureOrigin,
    pub culture: OpenAdventureOrigin,
    pub background: OpenAdventureOrigin,
    pub modus_operandi: OpenAdventureModusOperandi,
    pub attributes: OpenAdventureAttributes,
    pub specializations: Vec<OpenAdventureSpecialization>,
    pub favored_actions: Vec<OpenAdventureFavoredAction>,
    pub equipment: Vec<OpenAdventureEquipment>,
    pub benefits: Vec<OpenAdventureTraitModifier>,
    pub drawbacks: Vec<OpenAdventureTraitModifier>,
}

/// Action-dice progression for one final Action Score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureActionDice {
    pub count: u32,
    pub sides: u32,
}

/// One derived Action Score with every contributing input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureActionScore {
    pub attribute: OpenAdventureAttribute,
    pub base: i32,
    pub specialization: i32,
    pub modus_operandi: i32,
    pub benefits: i32,
    pub drawbacks: i32,
    pub total: i32,
    pub dice: OpenAdventureActionDice,
}

/// Deterministically derived defenses, tracks, Reaction, scores, and dice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureDerived {
    pub defense: i32,
    pub reaction: i32,
    pub body_points: u32,
    pub mind_points: u32,
    pub spirit_points: u32,
    pub action_scores: Vec<OpenAdventureActionScore>,
}

/// Explainable result of character creation before state export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureCreationPreview {
    pub creation_format_version: u32,
    pub request_sha256: String,
    pub adapter: ResolvedAdapter,
    pub definition: AdapterCharacterDefinition,
    pub initial_state: TabletopState,
    pub derived: OpenAdventureDerived,
    pub explanations: Vec<String>,
}

/// Whether a challenge is environmental or an NPC opposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureChallengeKind {
    Environmental,
    Npc,
}

impl OpenAdventureChallengeKind {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Environmental => "environmental",
            Self::Npc => "npc",
        }
    }
}

/// Character pool targeted by an NPC consequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventurePool {
    Body,
    Mind,
    Spirit,
}

impl OpenAdventurePool {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Mind => "mind",
            Self::Spirit => "spirit",
        }
    }

    const fn field_name(self) -> &'static str {
        match self {
            Self::Body => "body_points",
            Self::Mind => "mind_points",
            Self::Spirit => "spirit_points",
        }
    }
}

/// Author-configured event visibility for one resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureDisclosure {
    HostOnly,
    Public,
}

impl OpenAdventureDisclosure {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::HostOnly => "host_only",
            Self::Public => "public",
        }
    }
}

macro_rules! impl_open_adventure_io {
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

impl_open_adventure_io!(OpenAdventureCreationRequest);
impl_open_adventure_io!(OpenAdventureCreationPreview);

/// Canonical JSON Schema for Open Adventure creation requests.
pub fn open_adventure_creation_request_schema() -> Result<String, TabletopError> {
    open_adventure_schema::<OpenAdventureCreationRequest>(
        "urn:weave:schema:tabletop-open-adventure-creation-request:1",
        "Weave Open Adventure Creation Request v1",
        Some((
            "creation_format_version",
            OPEN_ADVENTURE_CREATION_FORMAT_VERSION,
        )),
    )
}

/// Canonical JSON Schema for explainable Open Adventure creation previews.
pub fn open_adventure_creation_preview_schema() -> Result<String, TabletopError> {
    open_adventure_schema::<OpenAdventureCreationPreview>(
        "urn:weave:schema:tabletop-open-adventure-creation-preview:1",
        "Weave Open Adventure Creation Preview v1",
        Some((
            "creation_format_version",
            OPEN_ADVENTURE_CREATION_FORMAT_VERSION,
        )),
    )
}

fn open_adventure_schema<T: JsonSchema>(
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

/// Deterministic action-dice progression for one final Action Score.
#[must_use]
pub fn open_adventure_action_dice(action_score: i32) -> OpenAdventureActionDice {
    let (count, sides) = match action_score {
        i32::MIN..=2 => (1, 6),
        3..=4 => (2, 6),
        5..=6 => (2, 8),
        7..=8 => (3, 8),
        9..=10 => (3, 10),
        11..=12 => (4, 10),
        13..=14 => (4, 12),
        _ => (5, 12),
    };
    OpenAdventureActionDice { count, sides }
}

/// Recompute every derived Open Adventure value from an immutable definition.
///
/// The authored definition is never modified. Every result names its contributing inputs through
/// [`OpenAdventureActionScore`] and the fixed derivation described in the module documentation.
pub fn open_adventure_derived(
    definition: &DomainValue,
) -> Result<OpenAdventureDerived, TabletopError> {
    Ok(parse_definition(definition)?.derived)
}

/// Validate and deterministically create one Open Adventure character.
pub fn create_open_adventure_character(
    request: &OpenAdventureCreationRequest,
) -> Result<OpenAdventureCreationPreview, TabletopError> {
    validate_creation_request(request)?;
    let manifest = open_adventure_manifest();
    validate_adapter_manifest(&manifest)?;
    let adapter = resolved_adapter(&manifest)?;
    let definition_value = creation_definition_value(request);
    let definition_sha256 = canonical_fingerprint(&definition_value)?;
    let derived = open_adventure_derived(&definition_value)?;
    let definition = AdapterCharacterDefinition {
        adapter: adapter.clone(),
        definition: definition_value,
        definition_sha256: definition_sha256.clone(),
        completed_creation_steps: creation_step_ids(),
    };
    let initial_state = TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter: adapter.clone(),
        owner_id: request.character_id.clone(),
        definition_sha256,
        revision: 0,
        entropy: EntropyState {
            algorithm: "sha256_counter_v1".to_owned(),
            seed: request.seed,
            cursor: 0,
        },
        value: initial_state_value(&derived),
    };
    validate_tabletop_state(&initial_state, &manifest)?;
    let preview = OpenAdventureCreationPreview {
        creation_format_version: OPEN_ADVENTURE_CREATION_FORMAT_VERSION,
        request_sha256: canonical_fingerprint(request)?,
        adapter,
        definition,
        initial_state,
        derived,
        explanations: vec![
            "Derived defense, Reaction, point tracks, Action Scores, and action dice are recomputed from authored choices without overwriting them."
                .to_owned(),
            "Creation consumes no entropy; the supplied seed initializes the deterministic stream used by challenge resolution."
                .to_owned(),
            "Open Adventure tabletop data remains adapter-owned and cannot write back into canonical Character evidence."
                .to_owned(),
        ],
    };
    validate_open_adventure_creation_preview(&preview)?;
    Ok(preview)
}

/// Revalidate a creation preview, its schema, and every recomputable value.
pub fn validate_open_adventure_creation_preview(
    preview: &OpenAdventureCreationPreview,
) -> Result<(), TabletopError> {
    if preview.creation_format_version != OPEN_ADVENTURE_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_sha256("request_sha256", &preview.request_sha256)?;
    let manifest = open_adventure_manifest();
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
    if preview.definition.completed_creation_steps != creation_step_ids() {
        return Err(invalid(
            "completed_creation_steps",
            "creation preview must complete every declared step exactly once",
        ));
    }
    let derived = open_adventure_derived(&preview.definition.definition)?;
    if derived != preview.derived {
        return Err(invalid(
            "derived",
            "derived values do not match the authored definition",
        ));
    }
    if preview.initial_state.definition_sha256 != preview.definition.definition_sha256 {
        return Err(TabletopError::ContentHashMismatch {
            path: "initial_state.definition_sha256",
        });
    }
    validate_tabletop_state(&preview.initial_state, &manifest)?;
    if preview.initial_state.revision != 0 || preview.initial_state.entropy.cursor != 0 {
        return Err(invalid(
            "initial_state",
            "creation must start at revision zero with an unconsumed entropy stream",
        ));
    }
    let state_fields = value_object("initial_state.value", &preview.initial_state.value)?;
    if object_u32(state_fields, "body_points")? != derived.body_points
        || object_u32(state_fields, "mind_points")? != derived.mind_points
        || object_u32(state_fields, "spirit_points")? != derived.spirit_points
    {
        return Err(invalid(
            "initial_state.value",
            "initial point tracks do not match the derived values",
        ));
    }
    if object_bool(state_fields, "defeated")? {
        return Err(invalid(
            "initial_state.value",
            "a newly created character cannot be defeated",
        ));
    }
    if !object_map(state_fields, "challenges")?.is_empty() {
        return Err(invalid(
            "initial_state.value",
            "a newly created character cannot start inside a challenge",
        ));
    }
    Ok(())
}

fn validate_creation_request(request: &OpenAdventureCreationRequest) -> Result<(), TabletopError> {
    if request.creation_format_version != OPEN_ADVENTURE_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_global_identifier("character_id", &request.character_id)?;
    validate_bounded_text("name", &request.name, 1, 160)?;
    validate_origin("lineage", &request.lineage)?;
    validate_origin("culture", &request.culture)?;
    validate_origin("background", &request.background)?;
    validate_local_identifier("modus_operandi.id", &request.modus_operandi.id)?;
    validate_bounded_text(
        "modus_operandi.label",
        &request.modus_operandi.label,
        1,
        160,
    )?;
    if !(1..=MAX_MODUS_OPERANDI_SCORE).contains(&request.modus_operandi.score) {
        return Err(invalid(
            "modus_operandi.score",
            "expected a score from 1 through 5",
        ));
    }
    for (path, value) in [
        ("attributes.agility", request.attributes.agility),
        ("attributes.empathy", request.attributes.empathy),
        ("attributes.endurance", request.attributes.endurance),
        ("attributes.focus", request.attributes.focus),
        ("attributes.insight", request.attributes.insight),
        ("attributes.might", request.attributes.might),
        ("attributes.presence", request.attributes.presence),
        ("attributes.reasoning", request.attributes.reasoning),
        ("attributes.will", request.attributes.will),
    ] {
        if !(1..=MAX_ATTRIBUTE_VALUE).contains(&value) {
            return Err(invalid(
                path,
                "expected an attribute rating from 1 through 5",
            ));
        }
    }
    if request.specializations.len() > MAX_SPECIALIZATIONS {
        return Err(invalid(
            "specializations",
            "at most one specialization per attribute is allowed",
        ));
    }
    let mut specialization_ids = BTreeSet::new();
    let mut specialized_attributes = BTreeSet::new();
    for (index, specialization) in request.specializations.iter().enumerate() {
        validate_local_identifier(format!("specializations[{index}].id"), &specialization.id)?;
        validate_bounded_text(
            format!("specializations[{index}].label"),
            &specialization.label,
            1,
            160,
        )?;
        if !(1..=MAX_SPECIALIZATION_BONUS).contains(&specialization.bonus) {
            return Err(invalid(
                format!("specializations[{index}].bonus"),
                "expected a bonus from 1 through 3",
            ));
        }
        if !specialization_ids.insert(&specialization.id) {
            return Err(invalid(
                "specializations",
                "duplicate specialization identity",
            ));
        }
        if !specialized_attributes.insert(specialization.attribute) {
            return Err(invalid(
                "specializations",
                "an attribute may have only one specialization",
            ));
        }
    }
    validate_detail_ids(
        "favored_actions",
        request.favored_actions.len(),
        request.favored_actions.iter().map(|item| item.id.as_str()),
    )?;
    for (index, action) in request.favored_actions.iter().enumerate() {
        validate_bounded_text(
            format!("favored_actions[{index}].label"),
            &action.label,
            1,
            160,
        )?;
    }
    validate_detail_ids(
        "equipment",
        request.equipment.len(),
        request.equipment.iter().map(|item| item.id.as_str()),
    )?;
    for (index, item) in request.equipment.iter().enumerate() {
        validate_bounded_text(format!("equipment[{index}].label"), &item.label, 1, 160)?;
    }
    validate_trait_modifiers("benefits", &request.benefits, true)?;
    validate_trait_modifiers("drawbacks", &request.drawbacks, false)?;
    let mut trait_ids = BTreeSet::new();
    for modifier in request.benefits.iter().chain(request.drawbacks.iter()) {
        if !trait_ids.insert(&modifier.id) {
            return Err(invalid(
                "benefits",
                "Benefit and Drawback identities must be globally unique",
            ));
        }
    }
    Ok(())
}

fn validate_origin(path: &str, origin: &OpenAdventureOrigin) -> Result<(), TabletopError> {
    validate_local_identifier(format!("{path}.id"), &origin.id)?;
    validate_bounded_text(format!("{path}.label"), &origin.label, 1, 160)?;
    validate_bounded_text(format!("{path}.note"), &origin.note, 1, 240)
}

fn validate_detail_ids<'a>(
    path: &str,
    length: usize,
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), TabletopError> {
    if length > MAX_DETAIL_ENTRIES {
        return Err(invalid(path, "too many detail entries"));
    }
    let mut seen = BTreeSet::new();
    for (index, id) in ids.into_iter().enumerate() {
        validate_local_identifier(format!("{path}[{index}].id"), id)?;
        if !seen.insert(id) {
            return Err(invalid(path, "duplicate detail identity"));
        }
    }
    Ok(())
}

fn validate_trait_modifiers(
    path: &str,
    modifiers: &[OpenAdventureTraitModifier],
    benefit: bool,
) -> Result<(), TabletopError> {
    if modifiers.len() > MAX_DETAIL_ENTRIES {
        return Err(invalid(path, "too many trait modifiers"));
    }
    let mut ids = BTreeSet::new();
    for (index, modifier) in modifiers.iter().enumerate() {
        validate_local_identifier(format!("{path}[{index}].id"), &modifier.id)?;
        validate_bounded_text(format!("{path}[{index}].label"), &modifier.label, 1, 160)?;
        let valid = if benefit {
            (1..=MAX_TRAIT_MODIFIER).contains(&modifier.modifier)
        } else {
            (-MAX_TRAIT_MODIFIER..=-1).contains(&modifier.modifier)
        };
        if !valid {
            return Err(invalid(
                format!("{path}[{index}].modifier"),
                if benefit {
                    "Benefits require a positive modifier from 1 through 10"
                } else {
                    "Drawbacks require a negative modifier from -10 through -1"
                },
            ));
        }
        if !ids.insert(&modifier.id) {
            return Err(invalid(path, "duplicate trait modifier identity"));
        }
    }
    Ok(())
}

fn creation_step_ids() -> Vec<String> {
    vec![
        "attributes".to_owned(),
        "details".to_owned(),
        "identity".to_owned(),
    ]
}

fn creation_definition_value(request: &OpenAdventureCreationRequest) -> DomainValue {
    object([
        ("attributes", attributes_value(&request.attributes)),
        ("background", origin_value(&request.background)),
        ("benefits", trait_modifier_map(&request.benefits)),
        ("culture", origin_value(&request.culture)),
        ("drawbacks", trait_modifier_map(&request.drawbacks)),
        ("equipment", equipment_map(&request.equipment)),
        (
            "favored_actions",
            favored_action_map(&request.favored_actions),
        ),
        ("lineage", origin_value(&request.lineage)),
        (
            "modus_operandi",
            object([
                ("id", DomainValue::String(request.modus_operandi.id.clone())),
                (
                    "label",
                    DomainValue::String(request.modus_operandi.label.clone()),
                ),
                (
                    "score",
                    DomainValue::Number(f64::from(request.modus_operandi.score)),
                ),
            ]),
        ),
        ("name", DomainValue::String(request.name.clone())),
        (
            "specializations",
            specialization_map(&request.specializations),
        ),
    ])
}

fn origin_value(origin: &OpenAdventureOrigin) -> DomainValue {
    object([
        ("id", DomainValue::String(origin.id.clone())),
        ("label", DomainValue::String(origin.label.clone())),
        ("note", DomainValue::String(origin.note.clone())),
    ])
}

fn attributes_value(attributes: &OpenAdventureAttributes) -> DomainValue {
    object([
        (
            "agility",
            DomainValue::Number(f64::from(attributes.agility)),
        ),
        (
            "empathy",
            DomainValue::Number(f64::from(attributes.empathy)),
        ),
        (
            "endurance",
            DomainValue::Number(f64::from(attributes.endurance)),
        ),
        ("focus", DomainValue::Number(f64::from(attributes.focus))),
        (
            "insight",
            DomainValue::Number(f64::from(attributes.insight)),
        ),
        ("might", DomainValue::Number(f64::from(attributes.might))),
        (
            "presence",
            DomainValue::Number(f64::from(attributes.presence)),
        ),
        (
            "reasoning",
            DomainValue::Number(f64::from(attributes.reasoning)),
        ),
        ("will", DomainValue::Number(f64::from(attributes.will))),
    ])
}

fn specialization_map(specializations: &[OpenAdventureSpecialization]) -> DomainValue {
    DomainValue::Object(
        specializations
            .iter()
            .map(|specialization| {
                (
                    specialization.id.clone(),
                    object([
                        (
                            "attribute",
                            DomainValue::Symbol(specialization.attribute.as_str().to_owned()),
                        ),
                        (
                            "bonus",
                            DomainValue::Number(f64::from(specialization.bonus)),
                        ),
                        ("id", DomainValue::String(specialization.id.clone())),
                        ("label", DomainValue::String(specialization.label.clone())),
                    ]),
                )
            })
            .collect(),
    )
}

fn favored_action_map(actions: &[OpenAdventureFavoredAction]) -> DomainValue {
    DomainValue::Object(
        actions
            .iter()
            .map(|action| {
                (
                    action.id.clone(),
                    object([
                        (
                            "attribute",
                            DomainValue::Symbol(action.attribute.as_str().to_owned()),
                        ),
                        ("id", DomainValue::String(action.id.clone())),
                        ("label", DomainValue::String(action.label.clone())),
                    ]),
                )
            })
            .collect(),
    )
}

fn equipment_map(equipment: &[OpenAdventureEquipment]) -> DomainValue {
    DomainValue::Object(
        equipment
            .iter()
            .map(|item| {
                (
                    item.id.clone(),
                    object([
                        ("id", DomainValue::String(item.id.clone())),
                        ("kind", DomainValue::Symbol(item.kind.as_str().to_owned())),
                        ("label", DomainValue::String(item.label.clone())),
                    ]),
                )
            })
            .collect(),
    )
}

fn trait_modifier_map(modifiers: &[OpenAdventureTraitModifier]) -> DomainValue {
    DomainValue::Object(
        modifiers
            .iter()
            .map(|modifier| {
                (
                    modifier.id.clone(),
                    object([
                        ("id", DomainValue::String(modifier.id.clone())),
                        ("label", DomainValue::String(modifier.label.clone())),
                        (
                            "modifier",
                            DomainValue::Number(f64::from(modifier.modifier)),
                        ),
                        (
                            "target",
                            DomainValue::Symbol(modifier.target.as_str().to_owned()),
                        ),
                    ]),
                )
            })
            .collect(),
    )
}

fn initial_state_value(derived: &OpenAdventureDerived) -> DomainValue {
    object([
        (
            "body_points",
            DomainValue::Number(f64::from(derived.body_points)),
        ),
        ("challenges", DomainValue::Object(BTreeMap::new())),
        ("defeated", DomainValue::Bool(false)),
        (
            "mind_points",
            DomainValue::Number(f64::from(derived.mind_points)),
        ),
        (
            "spirit_points",
            DomainValue::Number(f64::from(derived.spirit_points)),
        ),
    ])
}

struct ParsedDefinition {
    derived: OpenAdventureDerived,
    favored_actions: BTreeMap<String, OpenAdventureFavoredAction>,
    benefits: Vec<OpenAdventureTraitModifier>,
    drawbacks: Vec<OpenAdventureTraitModifier>,
}

fn parse_definition(definition: &DomainValue) -> Result<ParsedDefinition, TabletopError> {
    let fields = value_object("definition", definition)?;
    let attribute_fields = object_map(fields, "attributes")?;
    let mut attributes = [0_u32; 9];
    for attribute in OpenAdventureAttribute::ALL {
        let value = object_u32(attribute_fields, attribute.as_str())?;
        if !(1..=MAX_ATTRIBUTE_VALUE).contains(&value) {
            return Err(invalid(
                format!("definition.attributes.{}", attribute.as_str()),
                "attribute rating is outside 1..=5",
            ));
        }
        let slot = attributes
            .get_mut(attribute.index())
            .ok_or_else(|| invalid("definition.attributes", "attribute index is invalid"))?;
        *slot = value;
    }
    let modus_operandi_fields = value_object(
        "definition.modus_operandi",
        object_field(fields, "modus_operandi")?,
    )?;
    let modus_operandi = object_u32(modus_operandi_fields, "score")?;
    if !(1..=MAX_MODUS_OPERANDI_SCORE).contains(&modus_operandi) {
        return Err(invalid(
            "definition.modus_operandi.score",
            "Modus Operandi score is outside 1..=5",
        ));
    }
    let mut specialization_bonuses: BTreeMap<OpenAdventureAttribute, u32> = BTreeMap::new();
    for (id, value) in object_map(fields, "specializations")? {
        let specialization_fields = value_object("definition.specializations", value)?;
        let attribute = parse_attribute(object_symbol(specialization_fields, "attribute")?)?;
        let bonus = object_u32(specialization_fields, "bonus")?;
        if !(1..=MAX_SPECIALIZATION_BONUS).contains(&bonus) {
            return Err(invalid(
                "definition.specializations.bonus",
                "specialization bonus is outside 1..=3",
            ));
        }
        if object_string(specialization_fields, "id")? != id {
            return Err(invalid(
                "definition.specializations",
                "specialization map key does not match its stored identity",
            ));
        }
        let slot = specialization_bonuses.entry(attribute).or_insert(0);
        *slot = slot
            .checked_add(bonus)
            .ok_or_else(|| invalid("definition.specializations", "bonus total overflow"))?;
    }
    let benefits = parse_trait_modifiers(fields, "benefits", true)?;
    let drawbacks = parse_trait_modifiers(fields, "drawbacks", false)?;
    let favored_actions = parse_favored_actions(fields)?;

    let mut modifier_totals: BTreeMap<OpenAdventureModifierTarget, i32> = BTreeMap::new();
    for modifier in benefits.iter().chain(drawbacks.iter()) {
        let slot = modifier_totals.entry(modifier.target).or_insert(0);
        *slot = slot
            .checked_add(modifier.modifier)
            .ok_or_else(|| invalid("definition", "modifier total overflow"))?;
    }
    let total_for = |target: OpenAdventureModifierTarget| -> i32 {
        modifier_totals.get(&target).copied().unwrap_or(0)
    };
    let action_benefits = benefits
        .iter()
        .filter(|modifier| modifier.target == OpenAdventureModifierTarget::ActionScore)
        .map(|modifier| modifier.modifier)
        .sum::<i32>();
    let action_drawbacks = drawbacks
        .iter()
        .filter(|modifier| modifier.target == OpenAdventureModifierTarget::ActionScore)
        .map(|modifier| modifier.modifier)
        .sum::<i32>();

    let attribute_value = |attribute: OpenAdventureAttribute| -> Result<i32, TabletopError> {
        attributes
            .get(attribute.index())
            .copied()
            .ok_or_else(|| invalid("definition.attributes", "attribute index is invalid"))
            .and_then(|value| {
                i32::try_from(value)
                    .map_err(|_| invalid("definition.attributes", "attribute value overflow"))
            })
    };
    let agility = attribute_value(OpenAdventureAttribute::Agility)?;
    let endurance = attribute_value(OpenAdventureAttribute::Endurance)?;
    let focus = attribute_value(OpenAdventureAttribute::Focus)?;
    let insight = attribute_value(OpenAdventureAttribute::Insight)?;
    let might = attribute_value(OpenAdventureAttribute::Might)?;
    let presence = attribute_value(OpenAdventureAttribute::Presence)?;
    let reasoning = attribute_value(OpenAdventureAttribute::Reasoning)?;
    let will = attribute_value(OpenAdventureAttribute::Will)?;
    let modus_operandi = i32::try_from(modus_operandi)
        .map_err(|_| invalid("definition.modus_operandi", "score overflow"))?;

    let defense = checked_sum(&[10, agility, total_for(OpenAdventureModifierTarget::Defense)])?;
    let reaction = checked_sum(&[
        10,
        agility,
        insight,
        total_for(OpenAdventureModifierTarget::Reaction),
    ])?;
    let body_points = checked_points(&[
        5,
        might,
        endurance,
        total_for(OpenAdventureModifierTarget::BodyPoints),
    ])?;
    let mind_points = checked_points(&[
        5,
        reasoning,
        focus,
        total_for(OpenAdventureModifierTarget::MindPoints),
    ])?;
    let spirit_points = checked_points(&[
        5,
        will,
        presence,
        total_for(OpenAdventureModifierTarget::SpiritPoints),
    ])?;

    let mut action_scores = Vec::with_capacity(OpenAdventureAttribute::ALL.len());
    for attribute in OpenAdventureAttribute::ALL {
        let base = attribute_value(attribute)?;
        let specialization =
            i32::try_from(specialization_bonuses.get(&attribute).copied().unwrap_or(0))
                .map_err(|_| invalid("definition.specializations", "bonus overflow"))?;
        let total = checked_sum(&[
            base,
            specialization,
            modus_operandi,
            action_benefits,
            action_drawbacks,
        ])?;
        action_scores.push(OpenAdventureActionScore {
            attribute,
            base,
            specialization,
            modus_operandi,
            benefits: action_benefits,
            drawbacks: action_drawbacks,
            total,
            dice: open_adventure_action_dice(total),
        });
    }

    Ok(ParsedDefinition {
        derived: OpenAdventureDerived {
            defense,
            reaction,
            body_points,
            mind_points,
            spirit_points,
            action_scores,
        },
        favored_actions,
        benefits,
        drawbacks,
    })
}

fn parse_favored_actions(
    fields: &BTreeMap<String, DomainValue>,
) -> Result<BTreeMap<String, OpenAdventureFavoredAction>, TabletopError> {
    let mut actions = BTreeMap::new();
    for (id, value) in object_map(fields, "favored_actions")? {
        let action_fields = value_object("definition.favored_actions", value)?;
        let action = OpenAdventureFavoredAction {
            id: object_string(action_fields, "id")?.to_owned(),
            label: object_string(action_fields, "label")?.to_owned(),
            attribute: parse_attribute(object_symbol(action_fields, "attribute")?)?,
        };
        if &action.id != id {
            return Err(invalid(
                "definition.favored_actions",
                "favored action map key does not match its stored identity",
            ));
        }
        actions.insert(id.clone(), action);
    }
    Ok(actions)
}

fn parse_trait_modifiers(
    fields: &BTreeMap<String, DomainValue>,
    field_name: &str,
    benefit: bool,
) -> Result<Vec<OpenAdventureTraitModifier>, TabletopError> {
    let mut modifiers = Vec::new();
    for (id, value) in object_map(fields, field_name)? {
        let modifier_fields = value_object(format!("definition.{field_name}"), value)?;
        let modifier = OpenAdventureTraitModifier {
            id: object_string(modifier_fields, "id")?.to_owned(),
            label: object_string(modifier_fields, "label")?.to_owned(),
            target: parse_modifier_target(object_symbol(modifier_fields, "target")?)?,
            modifier: object_i32(modifier_fields, "modifier")?,
        };
        if &modifier.id != id {
            return Err(invalid(
                format!("definition.{field_name}"),
                "trait modifier map key does not match its stored identity",
            ));
        }
        let valid = if benefit {
            (1..=MAX_TRAIT_MODIFIER).contains(&modifier.modifier)
        } else {
            (-MAX_TRAIT_MODIFIER..=-1).contains(&modifier.modifier)
        };
        if !valid {
            return Err(invalid(
                format!("definition.{field_name}"),
                "trait modifier sign does not match its Benefit or Drawback category",
            ));
        }
        modifiers.push(modifier);
    }
    Ok(modifiers)
}

fn checked_sum(values: &[i32]) -> Result<i32, TabletopError> {
    values.iter().try_fold(0_i32, |total, value| {
        total
            .checked_add(*value)
            .ok_or_else(|| invalid("derived", "derived value overflow"))
    })
}

fn checked_points(values: &[i32]) -> Result<u32, TabletopError> {
    let total = checked_sum(values)?.max(1);
    u32::try_from(total).map_err(|_| invalid("derived", "point track must be a positive integer"))
}

/// Trusted resolver for the exact [`open_adventure_manifest`] coordinate.
#[derive(Debug, Default, Clone, Copy)]
pub struct OpenAdventureResolver;

impl TabletopResolver for OpenAdventureResolver {
    fn adapter_id(&self) -> &str {
        OPEN_ADVENTURE_ADAPTER_ID
    }

    fn adapter_version(&self) -> &str {
        OPEN_ADVENTURE_ADAPTER_VERSION
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
            "begin_challenge" => resolve_begin_challenge(request, state)?,
            "resolve_challenge_action" => {
                resolve_challenge_action(definition, request, state, entropy)?
            }
            _ => {
                return Err(invalid(
                    "operation",
                    "unsupported Open Adventure resolver operation",
                ));
            }
        };
        Ok(ResolverOutput {
            state: next_state,
            events,
        })
    }
}

fn resolve_begin_challenge(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let input = value_object("input", request)?;
    let challenge_id = object_string(input, "challenge_id")?.to_owned();
    let label = object_string(input, "label")?.to_owned();
    let kind = parse_challenge_kind(object_symbol(input, "kind")?)?;
    let disclosure = parse_disclosure(object_symbol(input, "disclosure")?)?;
    let difficulty_score = object_u32(input, "difficulty_score")?;
    let challenge_points = object_u32(input, "challenge_points")?;
    let effect = object_u32(input, "effect")?;
    validate_local_identifier("input.challenge_id", &challenge_id)?;
    validate_bounded_text("input.label", &label, 1, 160)?;
    if !(1..=100).contains(&difficulty_score) {
        return Err(invalid(
            "input.difficulty_score",
            "expected a Difficulty Score from 1 through 100",
        ));
    }
    if !(1..=1_000_000).contains(&challenge_points) {
        return Err(invalid(
            "input.challenge_points",
            "expected Challenge Points from 1 through 1000000",
        ));
    }
    if !(1..=1_000).contains(&effect) {
        return Err(invalid(
            "input.effect",
            "expected an Effect from 1 through 1000",
        ));
    }
    let npc_defense = optional_u32(input, "npc_defense")?;
    let npc_effect = optional_u32(input, "npc_effect")?;
    let npc_target = optional_symbol(input, "npc_target")?
        .map(parse_pool)
        .transpose()?;
    let (npc_defense, npc_effect, npc_target) = match kind {
        OpenAdventureChallengeKind::Environmental => {
            if npc_defense.is_some() || npc_effect.is_some() || npc_target.is_some() {
                return Err(invalid(
                    "input",
                    "environmental challenges cannot declare NPC opposition fields",
                ));
            }
            (0, 0, OpenAdventurePool::Body)
        }
        OpenAdventureChallengeKind::Npc => {
            let defense = npc_defense
                .ok_or_else(|| invalid("input.npc_defense", "NPC challenges require a defense"))?;
            let consequence = npc_effect.ok_or_else(|| {
                invalid(
                    "input.npc_effect",
                    "NPC challenges require a consequence effect",
                )
            })?;
            let target = npc_target.ok_or_else(|| {
                invalid("input.npc_target", "NPC challenges require a target pool")
            })?;
            if !(1..=1_000).contains(&defense) {
                return Err(invalid(
                    "input.npc_defense",
                    "expected an NPC defense from 1 through 1000",
                ));
            }
            if !(1..=1_000).contains(&consequence) {
                return Err(invalid(
                    "input.npc_effect",
                    "expected an NPC effect from 1 through 1000",
                ));
            }
            (defense, consequence, target)
        }
    };

    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    if object_bool(state_fields, "defeated")? {
        return Err(invalid(
            "state.defeated",
            "a defeated character cannot begin a challenge",
        ));
    }
    let challenges = object_map_mut(state_fields, "challenges")?;
    if challenges.contains_key(&challenge_id) {
        return Err(invalid(
            "input.challenge_id",
            "challenge identity already exists",
        ));
    }
    if challenges.len() >= MAX_CHALLENGES {
        return Err(invalid("state.challenges", "too many active challenges"));
    }
    challenges.insert(
        challenge_id.clone(),
        object([
            (
                "challenge_points",
                DomainValue::Number(f64::from(challenge_points)),
            ),
            ("completed", DomainValue::Bool(false)),
            (
                "difficulty_score",
                DomainValue::Number(f64::from(difficulty_score)),
            ),
            ("effect", DomainValue::Number(f64::from(effect))),
            ("kind", DomainValue::Symbol(kind.as_str().to_owned())),
            ("label", DomainValue::String(label.clone())),
            ("npc_defense", DomainValue::Number(f64::from(npc_defense))),
            ("npc_effect", DomainValue::Number(f64::from(npc_effect))),
            (
                "npc_target",
                DomainValue::Symbol(npc_target.as_str().to_owned()),
            ),
            (
                "remaining_challenge_points",
                DomainValue::Number(f64::from(challenge_points)),
            ),
        ]),
    );

    let mut public_fields = object_fields([
        ("challenge_id", DomainValue::String(challenge_id.clone())),
        (
            "challenge_points",
            DomainValue::Number(f64::from(challenge_points)),
        ),
        (
            "difficulty_score",
            DomainValue::Number(f64::from(difficulty_score)),
        ),
        ("effect", DomainValue::Number(f64::from(effect))),
        ("kind", DomainValue::Symbol(kind.as_str().to_owned())),
        ("label", DomainValue::String(label)),
        ("npc_defense", DomainValue::Number(f64::from(npc_defense))),
        ("npc_effect", DomainValue::Number(f64::from(npc_effect))),
        (
            "npc_target",
            DomainValue::Symbol(npc_target.as_str().to_owned()),
        ),
    ]);
    let mut events = Vec::new();
    if disclosure == OpenAdventureDisclosure::Public {
        events.push(public_event(
            "challenge_begun",
            DomainValue::Object(public_fields.clone()),
        ));
    }
    public_fields.insert(
        "disclosure".to_owned(),
        DomainValue::Symbol(disclosure.as_str().to_owned()),
    );
    events.push(authority_event(
        "challenge_begun_authority",
        DomainValue::Object(public_fields),
    ));
    Ok((next, events))
}

fn resolve_challenge_action(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let input = value_object("input", request)?;
    let challenge_id = object_string(input, "challenge_id")?.to_owned();
    let attribute = parse_attribute(object_symbol(input, "attribute")?)?;
    let disclosure = parse_disclosure(object_symbol(input, "disclosure")?)?;
    let situation_modifier = object_i32(input, "situation_modifier")?;
    let favored_action_id = optional_string(input, "favored_action_id")?.map(str::to_owned);
    validate_local_identifier("input.challenge_id", &challenge_id)?;
    if let Some(id) = &favored_action_id {
        validate_local_identifier("input.favored_action_id", id)?;
    }
    if !(-10..=10).contains(&situation_modifier) {
        return Err(invalid(
            "input.situation_modifier",
            "expected a situation modifier from -10 through 10",
        ));
    }

    let parsed = parse_definition(definition)?;
    let mut favored_bonus = 0;
    if let Some(id) = &favored_action_id {
        let favored = parsed.favored_actions.get(id).ok_or_else(|| {
            invalid(
                "input.favored_action_id",
                "favored action is not part of the definition",
            )
        })?;
        if favored.attribute != attribute {
            return Err(invalid(
                "input.favored_action_id",
                "favored action belongs to a different attribute",
            ));
        }
        favored_bonus = FAVORED_ACTION_BONUS;
    }
    let score = parsed
        .derived
        .action_scores
        .iter()
        .find(|score| score.attribute == attribute)
        .ok_or_else(|| invalid("input.attribute", "attribute score is unavailable"))?;
    let final_score = checked_sum(&[score.total, favored_bonus, situation_modifier])?;
    let dice = open_adventure_action_dice(final_score);

    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    if object_bool(state_fields, "defeated")? {
        return Err(invalid(
            "state.defeated",
            "a defeated character cannot resolve further actions",
        ));
    }
    let challenges = object_map_mut(state_fields, "challenges")?;
    let challenge = challenges.get(&challenge_id).cloned().ok_or_else(|| {
        invalid(
            "input.challenge_id",
            "challenge is not part of the current state",
        )
    })?;
    let challenge_fields = value_object("state.challenges", &challenge)?;
    let kind = parse_challenge_kind(object_symbol(challenge_fields, "kind")?)?;
    let label = object_string(challenge_fields, "label")?.to_owned();
    let completed = object_bool(challenge_fields, "completed")?;
    if completed {
        return Err(invalid(
            "input.challenge_id",
            "challenge is already complete",
        ));
    }
    let difficulty_score = object_u32(challenge_fields, "difficulty_score")?;
    let effect = object_u32(challenge_fields, "effect")?;
    let npc_defense = object_u32(challenge_fields, "npc_defense")?;
    let npc_effect = object_u32(challenge_fields, "npc_effect")?;
    let npc_target = parse_pool(object_symbol(challenge_fields, "npc_target")?)?;
    let remaining_before = object_u32(challenge_fields, "remaining_challenge_points")?;
    let target = match kind {
        OpenAdventureChallengeKind::Environmental => difficulty_score,
        OpenAdventureChallengeKind::Npc => {
            if npc_defense == 0 {
                return Err(invalid(
                    "state.challenges.npc_defense",
                    "NPC challenge defense must be positive",
                ));
            }
            npc_defense
        }
    };

    let cursor_before = entropy.state().cursor;
    let (rolled_dice, roll_total) = roll_action_dice(entropy, dice)?;
    let cursor_after = entropy.state().cursor;
    let success = roll_total >= u64::from(target);
    let (remaining_after, completed_now) = if success {
        let reduced = remaining_before.saturating_sub(effect);
        (reduced, reduced == 0)
    } else {
        (remaining_before, false)
    };

    let mut updated_challenge = challenge;
    let updated_fields = value_object_mut("state.challenges", &mut updated_challenge)?;
    set_u32(
        updated_fields,
        "remaining_challenge_points",
        remaining_after,
    );
    updated_fields.insert("completed".to_owned(), DomainValue::Bool(completed_now));
    challenges.insert(challenge_id.clone(), updated_challenge);

    let mut defeated_now = false;
    let mut pool_remaining = 0;
    if !success && kind == OpenAdventureChallengeKind::Npc {
        let pool_field = npc_target.field_name();
        let current = object_u32(state_fields, pool_field)?;
        let reduced = current.saturating_sub(npc_effect);
        set_u32(state_fields, pool_field, reduced);
        pool_remaining = reduced;
        if reduced == 0 {
            defeated_now = true;
            state_fields.insert("defeated".to_owned(), DomainValue::Bool(true));
        }
    }

    let modifiers = object([
        ("base", DomainValue::Number(f64::from(score.base))),
        ("benefits", DomainValue::Number(f64::from(score.benefits))),
        ("drawbacks", DomainValue::Number(f64::from(score.drawbacks))),
        ("favored", DomainValue::Number(f64::from(favored_bonus))),
        (
            "modus_operandi",
            DomainValue::Number(f64::from(score.modus_operandi)),
        ),
        (
            "situation",
            DomainValue::Number(f64::from(situation_modifier)),
        ),
        (
            "specialization",
            DomainValue::Number(f64::from(score.specialization)),
        ),
        ("total", DomainValue::Number(f64::from(final_score))),
    ]);
    let dice_value = DomainValue::List(
        rolled_dice
            .iter()
            .map(|die| {
                object([
                    (
                        "rolls",
                        DomainValue::List(
                            die.rolls
                                .iter()
                                .map(|roll| DomainValue::Number(f64::from(*roll)))
                                .collect(),
                        ),
                    ),
                    ("sides", DomainValue::Number(f64::from(die.sides))),
                    ("total", DomainValue::Number(f64::from(die.total))),
                ])
            })
            .collect(),
    );
    let outcome = if success { "success" } else { "failure" };
    let mut public_fields = object_fields([
        ("action_score", DomainValue::Number(f64::from(final_score))),
        (
            "attribute",
            DomainValue::Symbol(attribute.as_str().to_owned()),
        ),
        ("challenge_id", DomainValue::String(challenge_id.clone())),
        (
            "challenge_points_after",
            DomainValue::Number(f64::from(remaining_after)),
        ),
        (
            "challenge_points_before",
            DomainValue::Number(f64::from(remaining_before)),
        ),
        ("completed", DomainValue::Bool(completed_now)),
        ("difficulty", DomainValue::Number(f64::from(target))),
        ("dice", dice_value),
        ("effect", DomainValue::Number(f64::from(effect))),
        (
            "favored_action_id",
            DomainValue::String(favored_action_id.clone().unwrap_or_default()),
        ),
        ("modifiers", modifiers),
        ("outcome", DomainValue::Symbol(outcome.to_owned())),
        ("roll_total", DomainValue::Number(roll_total as f64)),
    ]);

    let mut events = Vec::new();
    if disclosure == OpenAdventureDisclosure::Public {
        events.push(public_event(
            "action_resolved",
            DomainValue::Object(public_fields.clone()),
        ));
    }
    let benefit_ids =
        action_modifier_ids(&parsed.benefits, OpenAdventureModifierTarget::ActionScore);
    let drawback_ids =
        action_modifier_ids(&parsed.drawbacks, OpenAdventureModifierTarget::ActionScore);
    public_fields.insert("benefit_ids".to_owned(), DomainValue::List(benefit_ids));
    public_fields.insert("drawback_ids".to_owned(), DomainValue::List(drawback_ids));
    public_fields.insert(
        "disclosure".to_owned(),
        DomainValue::Symbol(disclosure.as_str().to_owned()),
    );
    events.push(authority_event(
        "action_resolved_authority",
        DomainValue::Object(public_fields),
    ));

    if completed_now {
        let completion = object_fields([
            ("challenge_id", DomainValue::String(challenge_id.clone())),
            (
                "challenge_points_after",
                DomainValue::Number(f64::from(remaining_after)),
            ),
            ("label", DomainValue::String(label.clone())),
            ("outcome", DomainValue::Symbol(outcome.to_owned())),
        ]);
        if disclosure == OpenAdventureDisclosure::Public {
            events.push(public_event(
                "challenge_completed",
                DomainValue::Object(completion.clone()),
            ));
        }
        let mut authority_completion = completion;
        authority_completion.insert(
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.as_str().to_owned()),
        );
        events.push(authority_event(
            "challenge_completed_authority",
            DomainValue::Object(authority_completion),
        ));
    }

    if defeated_now {
        let defeat = object_fields([
            ("challenge_id", DomainValue::String(challenge_id)),
            ("outcome", DomainValue::Symbol(outcome.to_owned())),
            ("pool", DomainValue::Symbol(npc_target.as_str().to_owned())),
            (
                "remaining_points",
                DomainValue::Number(f64::from(pool_remaining)),
            ),
        ]);
        if disclosure == OpenAdventureDisclosure::Public {
            events.push(public_event(
                "character_defeated",
                DomainValue::Object(defeat.clone()),
            ));
        }
        let mut authority_defeat = defeat;
        authority_defeat.insert(
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.as_str().to_owned()),
        );
        events.push(authority_event(
            "character_defeated_authority",
            DomainValue::Object(authority_defeat),
        ));
    }

    events.push(authority_event(
        "entropy_trace",
        object([
            ("cursor_after", DomainValue::Number(cursor_after as f64)),
            ("cursor_before", DomainValue::Number(cursor_before as f64)),
            (
                "draws",
                DomainValue::Number(f64::from(
                    u32::try_from(cursor_after - cursor_before)
                        .map_err(|_| invalid("entropy", "entropy draw count is out of range"))?,
                )),
            ),
        ]),
    ));
    Ok((next, events))
}

fn action_modifier_ids(
    modifiers: &[OpenAdventureTraitModifier],
    target: OpenAdventureModifierTarget,
) -> Vec<DomainValue> {
    modifiers
        .iter()
        .filter(|modifier| modifier.target == target)
        .map(|modifier| DomainValue::String(modifier.id.clone()))
        .collect()
}

struct RolledDie {
    sides: u32,
    rolls: Vec<u32>,
    total: u32,
}

fn roll_action_dice(
    entropy: &mut EntropyStream,
    dice: OpenAdventureActionDice,
) -> Result<(Vec<RolledDie>, u64), TabletopError> {
    let mut rolled = Vec::with_capacity(usize::try_from(dice.count).unwrap_or_default());
    let mut roll_total = 0_u64;
    for _ in 0..dice.count {
        let mut rolls = Vec::new();
        let mut total = 0_u64;
        loop {
            let draw = entropy.draw_bounded(u64::from(dice.sides))?;
            let value =
                u32::try_from(draw + 1).map_err(|_| invalid("roll", "die result overflow"))?;
            let next_total = total
                .checked_add(u64::from(value))
                .ok_or_else(|| invalid("roll", "die total overflow"))?;
            if next_total > MAX_DIE_TOTAL {
                break;
            }
            rolls.push(value);
            total = next_total;
            if value < dice.sides || rolls.len() > MAX_EXPLOSIONS_PER_DIE {
                break;
            }
        }
        let total = u32::try_from(total).map_err(|_| invalid("roll", "die total overflow"))?;
        roll_total = roll_total
            .checked_add(u64::from(total))
            .ok_or_else(|| invalid("roll", "roll total overflow"))?;
        if roll_total > MAX_ROLL_TOTAL {
            return Err(invalid("roll", "roll total exceeds the reviewed bound"));
        }
        rolled.push(RolledDie {
            sides: dice.sides,
            rolls,
            total,
        });
    }
    Ok((rolled, roll_total))
}

fn parse_attribute(value: &str) -> Result<OpenAdventureAttribute, TabletopError> {
    match value {
        "agility" => Ok(OpenAdventureAttribute::Agility),
        "empathy" => Ok(OpenAdventureAttribute::Empathy),
        "endurance" => Ok(OpenAdventureAttribute::Endurance),
        "focus" => Ok(OpenAdventureAttribute::Focus),
        "insight" => Ok(OpenAdventureAttribute::Insight),
        "might" => Ok(OpenAdventureAttribute::Might),
        "presence" => Ok(OpenAdventureAttribute::Presence),
        "reasoning" => Ok(OpenAdventureAttribute::Reasoning),
        "will" => Ok(OpenAdventureAttribute::Will),
        _ => Err(invalid("attribute", "unknown Open Adventure attribute")),
    }
}

fn parse_modifier_target(value: &str) -> Result<OpenAdventureModifierTarget, TabletopError> {
    match value {
        "action_score" => Ok(OpenAdventureModifierTarget::ActionScore),
        "body_points" => Ok(OpenAdventureModifierTarget::BodyPoints),
        "defense" => Ok(OpenAdventureModifierTarget::Defense),
        "mind_points" => Ok(OpenAdventureModifierTarget::MindPoints),
        "reaction" => Ok(OpenAdventureModifierTarget::Reaction),
        "spirit_points" => Ok(OpenAdventureModifierTarget::SpiritPoints),
        _ => Err(invalid("target", "unknown Benefit or Drawback target")),
    }
}

fn parse_challenge_kind(value: &str) -> Result<OpenAdventureChallengeKind, TabletopError> {
    match value {
        "environmental" => Ok(OpenAdventureChallengeKind::Environmental),
        "npc" => Ok(OpenAdventureChallengeKind::Npc),
        _ => Err(invalid(
            "kind",
            "expected an environmental or NPC challenge",
        )),
    }
}

fn parse_pool(value: &str) -> Result<OpenAdventurePool, TabletopError> {
    match value {
        "body" => Ok(OpenAdventurePool::Body),
        "mind" => Ok(OpenAdventurePool::Mind),
        "spirit" => Ok(OpenAdventurePool::Spirit),
        _ => Err(invalid("pool", "expected body, mind, or spirit")),
    }
}

fn parse_disclosure(value: &str) -> Result<OpenAdventureDisclosure, TabletopError> {
    match value {
        "host_only" => Ok(OpenAdventureDisclosure::HostOnly),
        "public" => Ok(OpenAdventureDisclosure::Public),
        _ => Err(invalid("disclosure", "expected public or host_only")),
    }
}

/// Build the exact verified manifest shared by every Open Adventure surface.
#[must_use]
pub fn open_adventure_manifest() -> AdapterManifest {
    let types = BTreeMap::from([
        ("ActionModifiers".to_owned(), action_modifiers_type()),
        ("Attributes".to_owned(), attributes_type()),
        ("Background".to_owned(), origin_type()),
        ("Benefit".to_owned(), benefit_type()),
        ("Challenge".to_owned(), challenge_type()),
        ("Culture".to_owned(), origin_type()),
        ("Drawback".to_owned(), drawback_type()),
        ("DieResult".to_owned(), die_result_type()),
        ("Equipment".to_owned(), equipment_type()),
        ("FavoredAction".to_owned(), favored_action_type()),
        ("Lineage".to_owned(), origin_type()),
        ("ModusOperandi".to_owned(), modus_operandi_type()),
        ("Specialization".to_owned(), specialization_type()),
    ]);
    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: OPEN_ADVENTURE_ADAPTER_ID.to_owned(),
        version: OPEN_ADVENTURE_ADAPTER_VERSION.to_owned(),
        schema_version: 1,
        namespace: "open_adventure".to_owned(),
        title: "Open Adventure: Fantasy adapter".to_owned(),
        compatibility_label: "Compatible with Open Adventure: Fantasy Community-Owned Content"
            .to_owned(),
        summary: "Authored Open Adventure characters, derived defenses and point tracks, exploding action dice, Benefits and Drawbacks, and environmental or NPC challenge completion."
            .to_owned(),
        weave_version: ">=0.1.0, <0.2.0".to_owned(),
        character_contract_version: ">=1.0.0, <2.0.0".to_owned(),
        extension_surface: AdapterExtensionSurface::DeclarativeDataWithRegisteredResolver {
            resolver_contract_version: RESOLVER_CONTRACT_VERSION,
        },
        capabilities: vec![
            capability(TabletopCapability::CharacterCreation),
            capability(TabletopCapability::DerivedValues),
            capability(TabletopCapability::ChecksAndConflicts),
            capability(TabletopCapability::ResourcesAndConditions),
            capability(TabletopCapability::Encounters),
        ],
        types,
        definition_type: definition_type(),
        state_type: state_type(),
        creation_steps: creation_steps(),
        operations: operations(),
        event_types: event_types(),
        visibility: HostVisibilityPolicy::default(),
        migrations: Vec::new(),
        provenance: AdapterProvenance {
            source_class: AdapterSourceClass::VerifiedCc0,
            source_url: OPEN_ADVENTURE_SOURCE_URL.to_owned(),
            exact_artifact:
                "open-adventure-fantasy-community-owned-content-source-record".to_owned(),
            revision: OPEN_ADVENTURE_EDITION.to_owned(),
            retrieved_on: "2026-11-06".to_owned(),
            sha256: OPEN_ADVENTURE_SOURCE_RECORD_SHA256.to_owned(),
            license: OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN.to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                .to_owned(),
            covered_files_or_sections: vec![
                "Open Adventure: Fantasy Community-Owned Content core rules: character creation, nine attributes, derived values, Benefits, Drawbacks, action dice, and challenge resolution."
                    .to_owned(),
                "Official publisher project at https://openadventurerpg.com/ (Mechanical Muse), identified by canonical URL and reviewed edition only."
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
            attribution: "Open Adventure: Fantasy was created by Marc Tassin and is published by Mechanical Muse. The CC0 Community-Owned Content licence is recorded without a version because the publisher does not state one. Weave's adapter implementation, fixtures, schemas, and explanatory copy are independently authored."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "LICENSE-CC0-1.0.txt".to_owned(),
                sha256: OPEN_ADVENTURE_CC0_LEGAL_CODE_SHA256.to_owned(),
            },
            additional_artifacts: Vec::new(),
            notices: vec![
                "The provenance digest is the SHA-256 of the in-repository OPEN_ADVENTURE_SOURCE_RECORD string, not of a downloaded upstream file."
                    .to_owned(),
                "Masterwork Edition layout, third-party products, unmarked content, community additions, logos, artwork, and trade dress are excluded from the reviewed boundary."
                    .to_owned(),
            ],
            compatibility_statement: "Open Adventure is named only to describe rules compatibility. This project is independent and does not imply endorsement by Marc Tassin or Mechanical Muse."
                .to_owned(),
        },
    }
}

fn creation_steps() -> Vec<CreationStep> {
    vec![
        CreationStep {
            id: "attributes".to_owned(),
            title: "Attributes and specializations".to_owned(),
            description: "Assign the nine Body, Mind, and Spirit attributes and at most one specialization per attribute."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("attributes", "Attributes", named("Attributes"), true),
                creation_field(
                    "specializations",
                    "Specializations",
                    map(named("Specialization"), 0, MAX_SPECIALIZATIONS),
                    true,
                ),
            ],
        },
        CreationStep {
            id: "details".to_owned(),
            title: "Advantages and equipment".to_owned(),
            description: "Select favored actions, equipment references, Benefits, and Drawbacks."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field(
                    "benefits",
                    "Benefits",
                    map(named("Benefit"), 0, MAX_DETAIL_ENTRIES),
                    false,
                ),
                creation_field(
                    "drawbacks",
                    "Drawbacks",
                    map(named("Drawback"), 0, MAX_DETAIL_ENTRIES),
                    false,
                ),
                creation_field(
                    "equipment",
                    "Equipment references",
                    map(named("Equipment"), 0, MAX_DETAIL_ENTRIES),
                    false,
                ),
                creation_field(
                    "favored_actions",
                    "Favored actions",
                    map(named("FavoredAction"), 0, MAX_DETAIL_ENTRIES),
                    false,
                ),
            ],
        },
        CreationStep {
            id: "identity".to_owned(),
            title: "Identity and origins".to_owned(),
            description: "Name the character and select lineage, culture, background, and Modus Operandi."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("background", "Background", named("Background"), true),
                creation_field("culture", "Culture", named("Culture"), true),
                creation_field("lineage", "Lineage", named("Lineage"), true),
                creation_field(
                    "modus_operandi",
                    "Modus Operandi",
                    named("ModusOperandi"),
                    true,
                ),
                creation_field("name", "Name", string(1, 160), true),
            ],
        },
    ]
}

fn operations() -> BTreeMap<String, ResolverOperationDeclaration> {
    BTreeMap::from([
        (
            "begin_challenge".to_owned(),
            operation(
                "begin_challenge",
                "Begin a challenge",
                "Record an environmental or NPC challenge with an explicit Difficulty Score, Effect, and Challenge Points.",
                TabletopCapability::Encounters,
                object_type([
                    (
                        "challenge_id",
                        required(string(1, 128), "Stable challenge identity."),
                    ),
                    (
                        "challenge_points",
                        required(
                            integer(1.0, 1_000_000.0),
                            "Challenge Points required for completion.",
                        ),
                    ),
                    (
                        "difficulty_score",
                        required(integer(1.0, 100.0), "Environmental target number."),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Event visibility."),
                    ),
                    (
                        "effect",
                        required(
                            integer(1.0, 1_000.0),
                            "Challenge Point reduction on success.",
                        ),
                    ),
                    (
                        "kind",
                        required(challenge_kind_type(), "Environmental or NPC opposition."),
                    ),
                    ("label", required(string(1, 160), "Challenge label.")),
                    (
                        "npc_defense",
                        optional(integer(0.0, 1_000.0), "NPC target number."),
                    ),
                    (
                        "npc_effect",
                        optional(integer(0.0, 1_000.0), "NPC consequence on a failed action."),
                    ),
                    (
                        "npc_target",
                        optional(pool_type(), "Character pool harmed by an NPC."),
                    ),
                ]),
                ["challenge_begun", "challenge_begun_authority"],
                false,
            ),
        ),
        (
            "resolve_challenge_action".to_owned(),
            operation(
                "resolve_challenge_action",
                "Resolve a challenge action",
                "Roll exploding action dice, compare the result with the challenge target, apply Effect or an NPC consequence, and record completion.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    (
                        "attribute",
                        required(attribute_type(), "Attribute used by the action."),
                    ),
                    (
                        "challenge_id",
                        required(string(1, 128), "Active challenge identity."),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Event visibility."),
                    ),
                    (
                        "favored_action_id",
                        optional(string(1, 128), "Optional favored action identity."),
                    ),
                    (
                        "situation_modifier",
                        required(integer(-10.0, 10.0), "Authored situational adjustment."),
                    ),
                ]),
                [
                    "action_resolved",
                    "action_resolved_authority",
                    "challenge_completed",
                    "challenge_completed_authority",
                    "character_defeated",
                    "character_defeated_authority",
                    "entropy_trace",
                ],
                true,
            ),
        ),
    ])
}

fn event_types() -> BTreeMap<String, TypeExpression> {
    let action_resolved = action_resolved_type();
    BTreeMap::from([
        ("action_resolved".to_owned(), action_resolved.clone()),
        (
            "action_resolved_authority".to_owned(),
            extend_object_type(
                action_resolved,
                [
                    (
                        "benefit_ids",
                        required(
                            list(string(1, 128), 0, MAX_DETAIL_ENTRIES),
                            "Benefits that modified the action score.",
                        ),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Declared event visibility."),
                    ),
                    (
                        "drawback_ids",
                        required(
                            list(string(1, 128), 0, MAX_DETAIL_ENTRIES),
                            "Drawbacks that modified the action score.",
                        ),
                    ),
                ],
            ),
        ),
        ("challenge_begun".to_owned(), challenge_begun_type()),
        (
            "challenge_begun_authority".to_owned(),
            extend_object_type(
                challenge_begun_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        ("challenge_completed".to_owned(), challenge_completed_type()),
        (
            "challenge_completed_authority".to_owned(),
            extend_object_type(
                challenge_completed_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        ("character_defeated".to_owned(), character_defeated_type()),
        (
            "character_defeated_authority".to_owned(),
            extend_object_type(
                character_defeated_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        (
            "entropy_trace".to_owned(),
            object_type([
                (
                    "cursor_after",
                    required(
                        integer(0.0, 9_007_199_254_740_991.0),
                        "Entropy cursor after the roll.",
                    ),
                ),
                (
                    "cursor_before",
                    required(
                        integer(0.0, 9_007_199_254_740_991.0),
                        "Entropy cursor before the roll.",
                    ),
                ),
                (
                    "draws",
                    required(integer(0.0, 1_000.0), "Number of deterministic draws."),
                ),
            ]),
        ),
    ])
}

fn action_resolved_type() -> TypeExpression {
    object_type([
        (
            "action_score",
            required(integer(-1_000.0, 1_000.0), "Final Action Score."),
        ),
        (
            "attribute",
            required(attribute_type(), "Attribute used by the action."),
        ),
        (
            "challenge_id",
            required(string(1, 128), "Challenge identity."),
        ),
        (
            "challenge_points_after",
            required(
                integer(0.0, 1_000_000.0),
                "Remaining Challenge Points after the action.",
            ),
        ),
        (
            "challenge_points_before",
            required(
                integer(0.0, 1_000_000.0),
                "Remaining Challenge Points before the action.",
            ),
        ),
        (
            "completed",
            required(TypeExpression::Bool, "Whether the challenge completed."),
        ),
        (
            "difficulty",
            required(integer(1.0, 1_000.0), "Target number for this action."),
        ),
        (
            "dice",
            required(
                list(named("DieResult"), 1, 5),
                "Exploding action dice in roll order.",
            ),
        ),
        (
            "effect",
            required(
                integer(1.0, 1_000.0),
                "Challenge Point reduction on success.",
            ),
        ),
        (
            "favored_action_id",
            required(
                string(0, 128),
                "Favored action identity, or an empty string.",
            ),
        ),
        (
            "modifiers",
            required(named("ActionModifiers"), "Action Score inputs."),
        ),
        ("outcome", required(outcome_type(), "Success or failure.")),
        (
            "roll_total",
            required(integer(1.0, 500.0), "Sum of every exploding die."),
        ),
    ])
}

fn challenge_begun_type() -> TypeExpression {
    object_type([
        (
            "challenge_id",
            required(string(1, 128), "Challenge identity."),
        ),
        (
            "challenge_points",
            required(
                integer(1.0, 1_000_000.0),
                "Challenge Points required for completion.",
            ),
        ),
        (
            "difficulty_score",
            required(integer(1.0, 100.0), "Environmental target number."),
        ),
        (
            "effect",
            required(
                integer(1.0, 1_000.0),
                "Challenge Point reduction on success.",
            ),
        ),
        (
            "kind",
            required(challenge_kind_type(), "Environmental or NPC opposition."),
        ),
        ("label", required(string(1, 160), "Challenge label.")),
        (
            "npc_defense",
            required(integer(0.0, 1_000.0), "NPC target number or zero."),
        ),
        (
            "npc_effect",
            required(integer(0.0, 1_000.0), "NPC consequence or zero."),
        ),
        (
            "npc_target",
            required(pool_type(), "Character pool harmed by an NPC."),
        ),
    ])
}

fn challenge_completed_type() -> TypeExpression {
    object_type([
        (
            "challenge_id",
            required(string(1, 128), "Challenge identity."),
        ),
        (
            "challenge_points_after",
            required(integer(0.0, 1_000_000.0), "Remaining Challenge Points."),
        ),
        ("label", required(string(1, 160), "Challenge label.")),
        ("outcome", required(outcome_type(), "Completion outcome.")),
    ])
}

fn character_defeated_type() -> TypeExpression {
    object_type([
        (
            "challenge_id",
            required(string(1, 128), "Challenge identity."),
        ),
        ("outcome", required(outcome_type(), "Defeat outcome.")),
        (
            "pool",
            required(pool_type(), "Character pool reduced to zero."),
        ),
        (
            "remaining_points",
            required(integer(0.0, 1_000.0), "Remaining pool points."),
        ),
    ])
}

fn definition_type() -> TypeExpression {
    object_type([
        (
            "attributes",
            required(named("Attributes"), "Nine attributes."),
        ),
        (
            "background",
            required(named("Background"), "Background selection."),
        ),
        (
            "benefits",
            required(
                map(named("Benefit"), 0, MAX_DETAIL_ENTRIES),
                "Benefit modifiers.",
            ),
        ),
        ("culture", required(named("Culture"), "Culture selection.")),
        (
            "drawbacks",
            required(
                map(named("Drawback"), 0, MAX_DETAIL_ENTRIES),
                "Drawback modifiers.",
            ),
        ),
        (
            "equipment",
            required(
                map(named("Equipment"), 0, MAX_DETAIL_ENTRIES),
                "Equipment references.",
            ),
        ),
        (
            "favored_actions",
            required(
                map(named("FavoredAction"), 0, MAX_DETAIL_ENTRIES),
                "Favored actions.",
            ),
        ),
        ("lineage", required(named("Lineage"), "Lineage selection.")),
        (
            "modus_operandi",
            required(named("ModusOperandi"), "Modus Operandi and score."),
        ),
        ("name", required(string(1, 160), "Fictional display name.")),
        (
            "specializations",
            required(
                map(named("Specialization"), 0, MAX_SPECIALIZATIONS),
                "Attribute specializations.",
            ),
        ),
    ])
}

fn state_type() -> TypeExpression {
    object_type([
        (
            "body_points",
            required(integer(0.0, 1_000.0), "Current Body Points."),
        ),
        (
            "challenges",
            required(
                map(named("Challenge"), 0, MAX_CHALLENGES),
                "Active and completed challenges.",
            ),
        ),
        (
            "defeated",
            required(TypeExpression::Bool, "Whether the character is defeated."),
        ),
        (
            "mind_points",
            required(integer(0.0, 1_000.0), "Current Mind Points."),
        ),
        (
            "spirit_points",
            required(integer(0.0, 1_000.0), "Current Spirit Points."),
        ),
    ])
}

fn attributes_type() -> TypeExpression {
    object_type([
        ("agility", required(integer(1.0, 5.0), "Agility.")),
        ("empathy", required(integer(1.0, 5.0), "Empathy.")),
        ("endurance", required(integer(1.0, 5.0), "Endurance.")),
        ("focus", required(integer(1.0, 5.0), "Focus.")),
        ("insight", required(integer(1.0, 5.0), "Insight.")),
        ("might", required(integer(1.0, 5.0), "Might.")),
        ("presence", required(integer(1.0, 5.0), "Presence.")),
        ("reasoning", required(integer(1.0, 5.0), "Reasoning.")),
        ("will", required(integer(1.0, 5.0), "Will.")),
    ])
}

fn origin_type() -> TypeExpression {
    object_type([
        ("id", required(string(1, 128), "Stable origin identity.")),
        ("label", required(string(1, 160), "Origin label.")),
        ("note", required(string(1, 240), "Independent origin note.")),
    ])
}

fn modus_operandi_type() -> TypeExpression {
    object_type([
        (
            "id",
            required(string(1, 128), "Stable Modus Operandi identity."),
        ),
        ("label", required(string(1, 160), "Modus Operandi label.")),
        (
            "score",
            required(integer(1.0, 5.0), "Authored Modus Operandi score."),
        ),
    ])
}

fn specialization_type() -> TypeExpression {
    object_type([
        (
            "attribute",
            required(attribute_type(), "Improved attribute."),
        ),
        (
            "bonus",
            required(integer(1.0, 3.0), "Specialization bonus."),
        ),
        (
            "id",
            required(string(1, 128), "Stable specialization identity."),
        ),
        ("label", required(string(1, 160), "Specialization label.")),
    ])
}

fn favored_action_type() -> TypeExpression {
    object_type([
        (
            "attribute",
            required(attribute_type(), "Attribute used by the action."),
        ),
        (
            "id",
            required(string(1, 128), "Stable favored action identity."),
        ),
        ("label", required(string(1, 160), "Favored action label.")),
    ])
}

fn equipment_type() -> TypeExpression {
    object_type([
        ("id", required(string(1, 128), "Stable equipment identity.")),
        (
            "kind",
            required(equipment_kind_type(), "Equipment reference category."),
        ),
        ("label", required(string(1, 160), "Equipment label.")),
    ])
}

fn benefit_type() -> TypeExpression {
    trait_modifier_type(1.0, f64::from(MAX_TRAIT_MODIFIER))
}

fn drawback_type() -> TypeExpression {
    trait_modifier_type(-f64::from(MAX_TRAIT_MODIFIER), -1.0)
}

fn trait_modifier_type(minimum: f64, maximum: f64) -> TypeExpression {
    object_type([
        ("id", required(string(1, 128), "Stable trait identity.")),
        ("label", required(string(1, 160), "Trait label.")),
        (
            "modifier",
            required(integer(minimum, maximum), "Signed trait modifier."),
        ),
        (
            "target",
            required(
                modifier_target_type(),
                "Derived value changed by the trait.",
            ),
        ),
    ])
}

fn challenge_type() -> TypeExpression {
    object_type([
        (
            "challenge_points",
            required(
                integer(1.0, 1_000_000.0),
                "Challenge Points required for completion.",
            ),
        ),
        (
            "completed",
            required(TypeExpression::Bool, "Whether the challenge completed."),
        ),
        (
            "difficulty_score",
            required(integer(1.0, 100.0), "Environmental target number."),
        ),
        (
            "effect",
            required(
                integer(1.0, 1_000.0),
                "Challenge Point reduction on success.",
            ),
        ),
        (
            "kind",
            required(challenge_kind_type(), "Environmental or NPC opposition."),
        ),
        ("label", required(string(1, 160), "Challenge label.")),
        (
            "npc_defense",
            required(integer(0.0, 1_000.0), "NPC target number or zero."),
        ),
        (
            "npc_effect",
            required(integer(0.0, 1_000.0), "NPC consequence or zero."),
        ),
        (
            "npc_target",
            required(pool_type(), "Character pool harmed by an NPC."),
        ),
        (
            "remaining_challenge_points",
            required(integer(0.0, 1_000_000.0), "Remaining Challenge Points."),
        ),
    ])
}

fn action_modifiers_type() -> TypeExpression {
    object_type([
        ("base", required(integer(0.0, 5.0), "Attribute base value.")),
        (
            "benefits",
            required(integer(0.0, 320.0), "Benefit Action Score modifiers."),
        ),
        (
            "drawbacks",
            required(integer(-320.0, 0.0), "Drawback Action Score modifiers."),
        ),
        (
            "favored",
            required(integer(0.0, 2.0), "Favored action bonus."),
        ),
        (
            "modus_operandi",
            required(integer(0.0, 5.0), "Modus Operandi score."),
        ),
        (
            "situation",
            required(integer(-10.0, 10.0), "Authored situational modifier."),
        ),
        (
            "specialization",
            required(integer(0.0, 27.0), "Specialization bonus."),
        ),
        (
            "total",
            required(integer(-1_000.0, 1_000.0), "Final Action Score."),
        ),
    ])
}

fn die_result_type() -> TypeExpression {
    object_type([
        (
            "rolls",
            required(
                list(integer(1.0, 12.0), 1, MAX_EXPLOSIONS_PER_DIE + 1),
                "Every exploding die result in order.",
            ),
        ),
        ("sides", required(integer(4.0, 12.0), "Die size.")),
        (
            "total",
            required(integer(1.0, 100.0), "Sum of this die's exploding rolls."),
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

fn attribute_type() -> TypeExpression {
    symbol([
        "agility",
        "empathy",
        "endurance",
        "focus",
        "insight",
        "might",
        "presence",
        "reasoning",
        "will",
    ])
}

fn modifier_target_type() -> TypeExpression {
    symbol([
        "action_score",
        "body_points",
        "defense",
        "mind_points",
        "reaction",
        "spirit_points",
    ])
}

fn equipment_kind_type() -> TypeExpression {
    symbol(["armor", "tool", "weapon"])
}

fn challenge_kind_type() -> TypeExpression {
    symbol(["environmental", "npc"])
}

fn pool_type() -> TypeExpression {
    symbol(["body", "mind", "spirit"])
}

fn disclosure_type() -> TypeExpression {
    symbol(["host_only", "public"])
}

fn outcome_type() -> TypeExpression {
    symbol(["failure", "success"])
}

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(object_fields(fields))
}

fn object_fields<const N: usize>(
    fields: [(&str, DomainValue); N],
) -> BTreeMap<String, DomainValue> {
    fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
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

fn optional_symbol<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<Option<&'a str>, TabletopError> {
    match fields.get(field) {
        Some(DomainValue::Symbol(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(field, "expected a symbol")),
        None => Ok(None),
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

fn validate_local_identifier(path: impl Into<String>, value: &str) -> Result<(), TabletopError> {
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

/// SHA-256 of the in-repository source record, exposed for provenance verification.
#[must_use]
pub fn open_adventure_source_record_sha256() -> String {
    sha256_bytes(OPEN_ADVENTURE_SOURCE_RECORD.as_bytes())
}
