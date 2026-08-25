//! Verified CC0 Dungeonpunk adapter, creation workflow, and deterministic resolver.
//!
//! The implementation models the public core rules as original typed Rust procedures. The
//! official eight-page Google Doc exports are pinned by revision and exact hashes but their prose,
//! artwork, and layout are not redistributed in adapter fixtures or runtime data.

use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, FieldDeclaration, TypeExpression};

use crate::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition,
    AdapterExtensionSurface, AdapterManifest, AdapterProvenance, AdapterSourceArtifact,
    AdapterSourceClass, CC0_1_0_LEGAL_CODE_SHA256, CreationField, CreationFieldAuthority,
    CreationStep, EntropyState, EntropyStream, EventVisibility, ExcludedMaterial,
    HostVisibilityPolicy, LicenseTextReference, RESOLVER_CONTRACT_VERSION, ResolvedAdapter,
    ResolverEvent, ResolverOperationDeclaration, ResolverOutput, TabletopCapability,
    TabletopCapabilityDeclaration, TabletopError, TabletopResolver, TabletopState,
    canonical_fingerprint, resolved_adapter, validate_tabletop_state,
};

/// Stable adapter identity used by source, editor, runtime, and host integrations.
pub const DUNGEONPUNK_ADAPTER_ID: &str = "org.weave.tabletop.dungeonpunk";
/// First independently modeled Dungeonpunk adapter release.
pub const DUNGEONPUNK_ADAPTER_VERSION: &str = "1.0.0";
/// Versioned creation request and preview format.
pub const DUNGEONPUNK_CREATION_FORMAT_VERSION: u32 = 1;
/// Official public release page that links the reviewed source document.
pub const DUNGEONPUNK_RELEASE_URL: &str = "https://acegiak.itch.io/dungeonpunk";
/// Official commentable core document linked by the release page.
pub const DUNGEONPUNK_SOURCE_URL: &str = "https://docs.google.com/document/d/1hzbh6dLCUvYBhDJZzk4qlPOZLPlYZmSxZigGrw29EZE/edit?usp=sharing";
/// Exact public Google Doc revision reviewed for this adapter.
pub const DUNGEONPUNK_SOURCE_REVISION: &str = "google-doc-revision-15499";
/// SHA-256 of the stable PDF export acquired from revision 15499 on 2026-08-25.
pub const DUNGEONPUNK_PDF_SHA256: &str =
    "34baec19dc4c16db6505b791f4bae2a5e482f9b59b95cfe9236496f6daaab209";
/// SHA-256 of the UTF-8 text export acquired from the same revision.
pub const DUNGEONPUNK_TEXT_SHA256: &str =
    "85832950a335d4f8a4217e69fcefb88fc88a6a125cd663ea905d32e20bdebe2f";

/// The six ordinary character attributes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DungeonpunkAttribute {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Charisma,
    Wisdom,
}

impl DungeonpunkAttribute {
    const ALL: [Self; 6] = [
        Self::Strength,
        Self::Dexterity,
        Self::Constitution,
        Self::Intelligence,
        Self::Charisma,
        Self::Wisdom,
    ];

    const fn symbol(self) -> &'static str {
        match self {
            Self::Strength => "strength",
            Self::Dexterity => "dexterity",
            Self::Constitution => "constitution",
            Self::Intelligence => "intelligence",
            Self::Charisma => "charisma",
            Self::Wisdom => "wisdom",
        }
    }

    fn from_symbol(value: &str) -> Result<Self, TabletopError> {
        match value {
            "strength" => Ok(Self::Strength),
            "dexterity" => Ok(Self::Dexterity),
            "constitution" => Ok(Self::Constitution),
            "intelligence" => Ok(Self::Intelligence),
            "charisma" => Ok(Self::Charisma),
            "wisdom" => Ok(Self::Wisdom),
            _ => Err(invalid("attribute", "unknown Dungeonpunk attribute")),
        }
    }
}

/// Six authored or advanced attribute ratings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DungeonpunkAttributes {
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub intelligence: i32,
    pub charisma: i32,
    pub wisdom: i32,
}

impl DungeonpunkAttributes {
    fn value(&self, attribute: DungeonpunkAttribute) -> i32 {
        match attribute {
            DungeonpunkAttribute::Strength => self.strength,
            DungeonpunkAttribute::Dexterity => self.dexterity,
            DungeonpunkAttribute::Constitution => self.constitution,
            DungeonpunkAttribute::Intelligence => self.intelligence,
            DungeonpunkAttribute::Charisma => self.charisma,
            DungeonpunkAttribute::Wisdom => self.wisdom,
        }
    }

    fn sum(&self) -> i32 {
        DungeonpunkAttribute::ALL
            .into_iter()
            .map(|attribute| self.value(attribute))
            .sum()
    }
}

/// Broad grouping used by the official core move catalog.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DungeonpunkArchetype {
    Peasant,
    Fighter,
    Rogue,
    Wizard,
    Custom,
}

impl DungeonpunkArchetype {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Peasant => "peasant",
            Self::Fighter => "fighter",
            Self::Rogue => "rogue",
            Self::Wizard => "wizard",
            Self::Custom => "custom",
        }
    }
}

/// Whether a special move is rolled, passive, or limited by a play-session trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DungeonpunkMoveKind {
    Rolled,
    Passive,
    Session,
}

impl DungeonpunkMoveKind {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Rolled => "rolled",
            Self::Passive => "passive",
            Self::Session => "session",
        }
    }
}

/// Creation-time choice between one exact core move and an independently authored move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "source", deny_unknown_fields)]
pub enum DungeonpunkMoveChoice {
    Core {
        id: String,
    },
    Authored {
        id: String,
        label: String,
        kind: DungeonpunkMoveKind,
        attribute: Option<DungeonpunkAttribute>,
        trigger: String,
        success: String,
        twist: String,
        failure: String,
    },
}

/// Expanded, portable special-move definition shown by generic editor and runtime hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DungeonpunkSpecialMove {
    pub id: String,
    pub label: String,
    pub source: String,
    pub archetype: DungeonpunkArchetype,
    pub kind: DungeonpunkMoveKind,
    pub attribute: Option<DungeonpunkAttribute>,
    pub trigger: String,
    pub success: String,
    pub twist: String,
    pub failure: String,
}

/// Portable carried-item category used for load, uses, armor, and Brace behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DungeonpunkGearKind {
    Weapon,
    Armor,
    Shield,
    Ammunition,
    Book,
    Pack,
    Supplies,
    Tool,
    Instrument,
    Rations,
    Personal,
}

impl DungeonpunkGearKind {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Armor => "armor",
            Self::Shield => "shield",
            Self::Ammunition => "ammunition",
            Self::Book => "book",
            Self::Pack => "pack",
            Self::Supplies => "supplies",
            Self::Tool => "tool",
            Self::Instrument => "instrument",
            Self::Rations => "rations",
            Self::Personal => "personal",
        }
    }
}

/// One explicit piece of starting gear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DungeonpunkGear {
    pub id: String,
    pub label: String,
    pub kind: DungeonpunkGearKind,
    /// Whole Weight before any equipped-item special move is applied.
    pub weight: u8,
    pub uses: Option<u8>,
    pub equipped: bool,
}

/// Boundary at which a clock emits its trigger event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DungeonpunkClockTrigger {
    Empty,
    Full,
}

impl DungeonpunkClockTrigger {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Full => "full",
        }
    }
}

/// Initial campaign clock stored beside character state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DungeonpunkClock {
    pub id: String,
    pub label: String,
    pub segments: u8,
    pub filled: u8,
    pub trigger: DungeonpunkClockTrigger,
}

/// Closed threat categories with their own structured GM-move vocabularies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DungeonpunkThreatKind {
    CursedPlace,
    Overlord,
    Swarm,
    Affliction,
    Terror,
    Foe,
}

impl DungeonpunkThreatKind {
    const fn symbol(self) -> &'static str {
        match self {
            Self::CursedPlace => "cursed_place",
            Self::Overlord => "overlord",
            Self::Swarm => "swarm",
            Self::Affliction => "affliction",
            Self::Terror => "terror",
            Self::Foe => "foe",
        }
    }
}

/// One campaign-facing threat initialized with the portable play state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DungeonpunkThreat {
    pub id: String,
    pub label: String,
    pub kind: DungeonpunkThreatKind,
    pub hit_points: u16,
    pub armor: u8,
    pub clock_id: Option<String>,
}

/// Complete, versioned input to deterministic Dungeonpunk character creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DungeonpunkCreationRequest {
    pub creation_format_version: u32,
    pub character_id: String,
    pub name: String,
    pub seed: u64,
    pub attributes: DungeonpunkAttributes,
    pub special_moves: Vec<DungeonpunkMoveChoice>,
    pub gear: Vec<DungeonpunkGear>,
    pub bonds: Vec<String>,
    pub clocks: Vec<DungeonpunkClock>,
    pub threats: Vec<DungeonpunkThreat>,
}

/// Explainable result of creation before projection and mutable-state export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DungeonpunkCreationPreview {
    pub creation_format_version: u32,
    pub request_sha256: String,
    pub adapter: ResolvedAdapter,
    pub hp_rolls: Vec<u8>,
    pub attributes: DungeonpunkAttributes,
    pub fate: i32,
    pub none: i32,
    pub max_hp: i32,
    pub special_moves: Vec<DungeonpunkSpecialMove>,
    /// Two units represent one Weight, preserving exact half-weight gear effects.
    pub gear_weight_half_units: u16,
    pub starting_total_half_units: u16,
    pub starting_encumbered: bool,
    pub definition: AdapterCharacterDefinition,
    pub initial_state: TabletopState,
    pub explanations: Vec<String>,
}

macro_rules! impl_creation_io {
    ($type:ty) => {
        impl $type {
            /// Parse strict JSON and reject duplicate keys.
            pub fn from_json(source: &str) -> Result<Self, TabletopError> {
                weave_domain::parse_strict_json(source).map_err(|_| TabletopError::Artifact)
            }

            /// Parse one strict RON artifact.
            pub fn from_ron(source: &str) -> Result<Self, TabletopError> {
                ron::from_str(source).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize canonical, pretty JSON.
            pub fn to_json(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_json(self).map_err(|_| TabletopError::Artifact)
            }

            /// Serialize canonical, pretty RON.
            pub fn to_ron(&self) -> Result<String, TabletopError> {
                weave_domain::to_pretty_ron(self).map_err(|_| TabletopError::Artifact)
            }
        }
    };
}

impl_creation_io!(DungeonpunkCreationRequest);
impl_creation_io!(DungeonpunkCreationPreview);

/// Canonical JSON Schema for Dungeonpunk creation requests.
pub fn dungeonpunk_creation_request_schema() -> Result<String, TabletopError> {
    creation_schema::<DungeonpunkCreationRequest>(
        "urn:weave:schema:tabletop-dungeonpunk-creation-request:1",
        "Weave Dungeonpunk Creation Request v1",
    )
}

/// Canonical JSON Schema for explainable Dungeonpunk creation previews.
pub fn dungeonpunk_creation_preview_schema() -> Result<String, TabletopError> {
    creation_schema::<DungeonpunkCreationPreview>(
        "urn:weave:schema:tabletop-dungeonpunk-creation-preview:1",
        "Weave Dungeonpunk Creation Preview v1",
    )
}

fn creation_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, TabletopError> {
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
        if let Some(property) = root
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut("creation_format_version"))
            .and_then(serde_json::Value::as_object_mut)
        {
            property.insert(
                "const".to_owned(),
                serde_json::Value::from(DUNGEONPUNK_CREATION_FORMAT_VERSION),
            );
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

#[derive(Debug, Clone, Copy)]
struct CoreMove {
    id: &'static str,
    label: &'static str,
    archetype: DungeonpunkArchetype,
    kind: DungeonpunkMoveKind,
    attribute: Option<DungeonpunkAttribute>,
    trigger: &'static str,
    success: &'static str,
    twist: &'static str,
    failure: &'static str,
}

/// Independently phrased metadata for every special move in the reviewed core document.
const CORE_MOVES: &[CoreMove] = &[
    core_passive(
        "stone_soup",
        "Stone Soup",
        DungeonpunkArchetype::Peasant,
        "Prepare a shared meal from available supplies so participants can recover.",
    ),
    core_passive(
        "hale_and_hearty",
        "Hale and Hearty",
        DungeonpunkArchetype::Peasant,
        "Increase the character's maximum endurance with one additional d6.",
    ),
    core_passive(
        "companion",
        "Companion",
        DungeonpunkArchetype::Peasant,
        "Define a companion with one practical specialty that can act or contribute a die.",
    ),
    core_session(
        "stalwart_ally",
        "Stalwart Ally",
        DungeonpunkArchetype::Peasant,
        "Once per session, earn XP by accepting risk or disadvantage to aid an ally.",
    ),
    core_rolled(
        "warriors_strike",
        "Warrior's Strike",
        DungeonpunkArchetype::Fighter,
        DungeonpunkAttribute::Strength,
        "Strike a foe with intent to harm.",
        "Deal harm, with an option to trade incoming harm for an extra damage die.",
        "Deal harm and expose yourself or an ally to danger.",
        "The attack fails and invites a GM response.",
    ),
    core_rolled(
        "ranged_attack",
        "Ranged Attack",
        DungeonpunkArchetype::Fighter,
        DungeonpunkAttribute::Dexterity,
        "Attack a distant target with a missile weapon.",
        "Deal the weapon's harm.",
        "Deal harm while spending ammunition, moving into danger, or reducing damage.",
        "The shot fails and invites a GM response.",
    ),
    core_rolled(
        "assess",
        "Assess",
        DungeonpunkArchetype::Fighter,
        DungeonpunkAttribute::Wisdom,
        "Read a person or unfolding situation.",
        "Ask two actionable questions from the move's inquiry list.",
        "Ask one actionable question.",
        "Gain no reliable answer and invite a GM response.",
    ),
    core_passive(
        "martial_training",
        "Martial Training",
        DungeonpunkArchetype::Fighter,
        "Raise the damage tier of one chosen weapon category.",
    ),
    core_rolled(
        "field_medic",
        "Field Medic",
        DungeonpunkArchetype::Fighter,
        DungeonpunkAttribute::Wisdom,
        "Spend time and medical supplies treating wounds.",
        "Restore one d6 HP while retaining supplies.",
        "Restore one d6 HP and consume the supplies.",
        "Treatment does not stabilize the situation and invites a GM response.",
    ),
    core_rolled(
        "heavy_words",
        "Heavy Words",
        DungeonpunkArchetype::Fighter,
        DungeonpunkAttribute::Charisma,
        "State a demand backed by a credible threat.",
        "The subject works to satisfy the demand.",
        "Compliance lasts only while the pressure remains immediate.",
        "The threat fails and invites a GM response.",
    ),
    core_rolled(
        "brute_force",
        "Brute Force",
        DungeonpunkArchetype::Fighter,
        DungeonpunkAttribute::Strength,
        "Break through an inanimate obstacle by force.",
        "Avoid two collateral costs involving noise, delay, or damage.",
        "Avoid one collateral cost.",
        "The attempt fails with a consequential cost.",
    ),
    core_passive(
        "loaded_for_bear",
        "Loaded for Bear",
        DungeonpunkArchetype::Fighter,
        "Equipped weapons and armor contribute half their ordinary carried Weight.",
    ),
    core_session(
        "valour",
        "Valour",
        DungeonpunkArchetype::Fighter,
        "Once per session, earn XP after defeating a worthy opponent.",
    ),
    core_rolled(
        "careful",
        "Careful",
        DungeonpunkArchetype::Rogue,
        DungeonpunkAttribute::Dexterity,
        "Attack a target that has not detected you.",
        "Deal harm without revealing your position.",
        "Deal harm but accept exposure, danger, or reduced damage.",
        "The ambush fails and invites a GM response.",
    ),
    core_rolled(
        "investigation",
        "Investigation",
        DungeonpunkArchetype::Rogue,
        DungeonpunkAttribute::Intelligence,
        "Closely examine a place or situation.",
        "Ask a question and receive a useful contextual answer.",
        "Ask a question that can be answered only yes, no, or unknown.",
        "The search yields trouble rather than clarity.",
    ),
    core_passive(
        "poisoner",
        "Poisoner",
        DungeonpunkArchetype::Rogue,
        "Spend prepared poison after dealing weapon damage to add harm or impose a condition.",
    ),
    core_rolled(
        "ghost",
        "Ghost",
        DungeonpunkArchetype::Rogue,
        DungeonpunkAttribute::Dexterity,
        "Hide from ordinary scrutiny.",
        "Move while remaining unseen.",
        "Remain unseen only while stationary.",
        "Your position becomes vulnerable to a GM response.",
    ),
    core_rolled(
        "infiltrator",
        "Infiltrator",
        DungeonpunkArchetype::Rogue,
        DungeonpunkAttribute::Dexterity,
        "Traverse a dangerous or hard-to-reach route.",
        "Reach the intended destination cleanly.",
        "Reach it while accepting injury, attention, or a blocked return route.",
        "Fail to reach it and invite a GM response.",
    ),
    core_rolled(
        "trap_expert",
        "Trap Expert",
        DungeonpunkArchetype::Rogue,
        DungeonpunkAttribute::Dexterity,
        "Build or dismantle a trap under risk.",
        "Preserve two benefits involving safety, reuse, reversibility, or materials.",
        "Preserve one such benefit.",
        "The trap work causes a consequential problem.",
    ),
    core_rolled(
        "antidote",
        "Antidote",
        DungeonpunkArchetype::Rogue,
        DungeonpunkAttribute::Intelligence,
        "Treat poison, sickness, or disease with medical supplies.",
        "Remove the malady while retaining supplies.",
        "Remove it and consume the supplies.",
        "The treatment fails and invites a GM response.",
    ),
    core_rolled(
        "honeyed_words",
        "Honeyed Words",
        DungeonpunkArchetype::Rogue,
        DungeonpunkAttribute::Charisma,
        "Persuade someone by appealing to what they want to hear.",
        "They act or offer a meaningful lesser version.",
        "They require a concession before acting.",
        "The appeal fails and invites a GM response.",
    ),
    core_session(
        "shiny",
        "Shiny",
        DungeonpunkArchetype::Rogue,
        "Once per session, earn XP after claiming valuable treasure.",
    ),
    core_rolled(
        "fury_of_the_elements",
        "Fury of the Elements",
        DungeonpunkArchetype::Wizard,
        DungeonpunkAttribute::Wisdom,
        "Shape an element or energy into an attack at a declared damage tier.",
        "Inflict the declared elemental harm.",
        "Inflict it while the same danger reaches you or allies.",
        "The power fails and invites a GM response.",
    ),
    core_rolled(
        "arcane_insight",
        "Arcane Insight",
        DungeonpunkArchetype::Wizard,
        DungeonpunkAttribute::Intelligence,
        "Open your mind while seeking a particular answer.",
        "Discover something useful.",
        "Discover something troubling, unclear, or dangerous.",
        "The inquiry fails and invites a GM response.",
    ),
    core_rolled(
        "magical_literacy",
        "Magical Literacy",
        DungeonpunkArchetype::Wizard,
        DungeonpunkAttribute::Intelligence,
        "Invoke magic recorded in an arcane text.",
        "Bring the recorded magic into effect.",
        "Bring it into effect while erasing the source text.",
        "The reading fails and invites a GM response.",
    ),
    core_passive(
        "bound_element",
        "Bound Element",
        DungeonpunkArchetype::Wizard,
        "Choose one element that can be summoned in a small sustained quantity while concentrating.",
    ),
    core_passive(
        "beast_form",
        "Beast Form",
        DungeonpunkArchetype::Wizard,
        "Transform into a deeply studied animal and use its natural features as inventory.",
    ),
    core_rolled(
        "healing_touch",
        "Healing Touch",
        DungeonpunkArchetype::Wizard,
        DungeonpunkAttribute::Wisdom,
        "Use direct magic to mend a creature.",
        "Restore one d6 HP or resolve another injury.",
        "Move the same harm or injury onto yourself.",
        "The magic fails and invites a GM response.",
    ),
    core_rolled(
        "grand_words",
        "Grand Words",
        DungeonpunkArchetype::Wizard,
        DungeonpunkAttribute::Charisma,
        "Appeal to reason, morality, or duty and name the desired action.",
        "The listener believes the appeal and follows through.",
        "They comply temporarily from uncomfortable pressure.",
        "The appeal fails and invites a GM response.",
    ),
    core_passive(
        "strange_tongue",
        "Strange Tongue",
        DungeonpunkArchetype::Wizard,
        "Gain fluent use of one additional language.",
    ),
    core_session(
        "curiosity",
        "Curiosity",
        DungeonpunkArchetype::Wizard,
        "Once per session, earn XP after uncovering a new world secret.",
    ),
];

const fn core_passive(
    id: &'static str,
    label: &'static str,
    archetype: DungeonpunkArchetype,
    trigger: &'static str,
) -> CoreMove {
    CoreMove {
        id,
        label,
        archetype,
        kind: DungeonpunkMoveKind::Passive,
        attribute: None,
        trigger,
        success: "",
        twist: "",
        failure: "",
    }
}

const fn core_session(
    id: &'static str,
    label: &'static str,
    archetype: DungeonpunkArchetype,
    trigger: &'static str,
) -> CoreMove {
    CoreMove {
        id,
        label,
        archetype,
        kind: DungeonpunkMoveKind::Session,
        attribute: None,
        trigger,
        success: "",
        twist: "",
        failure: "",
    }
}

#[allow(clippy::too_many_arguments)]
const fn core_rolled(
    id: &'static str,
    label: &'static str,
    archetype: DungeonpunkArchetype,
    attribute: DungeonpunkAttribute,
    trigger: &'static str,
    success: &'static str,
    twist: &'static str,
    failure: &'static str,
) -> CoreMove {
    CoreMove {
        id,
        label,
        archetype,
        kind: DungeonpunkMoveKind::Rolled,
        attribute: Some(attribute),
        trigger,
        success,
        twist,
        failure,
    }
}

/// Return the complete reviewed core move catalog as independently phrased portable metadata.
#[must_use]
pub fn dungeonpunk_core_moves() -> Vec<DungeonpunkSpecialMove> {
    CORE_MOVES.iter().copied().map(expand_core_move).collect()
}

fn expand_core_move(entry: CoreMove) -> DungeonpunkSpecialMove {
    DungeonpunkSpecialMove {
        id: entry.id.to_owned(),
        label: entry.label.to_owned(),
        source: "core".to_owned(),
        archetype: entry.archetype,
        kind: entry.kind,
        attribute: entry.attribute,
        trigger: entry.trigger.to_owned(),
        success: entry.success.to_owned(),
        twist: entry.twist.to_owned(),
        failure: entry.failure.to_owned(),
    }
}

/// Validate and deterministically create a complete playable character and campaign state.
pub fn create_dungeonpunk_character(
    request: &DungeonpunkCreationRequest,
) -> Result<DungeonpunkCreationPreview, TabletopError> {
    validate_creation_request(request)?;
    let manifest = dungeonpunk_manifest();
    crate::validate_adapter_manifest(&manifest)?;
    let adapter = resolved_adapter(&manifest)?;
    let mut entropy = EntropyStream::from_state(EntropyState {
        algorithm: "sha256_counter_v1".to_owned(),
        seed: request.seed,
        cursor: 0,
    })?;
    let special_moves = request
        .special_moves
        .iter()
        .map(expand_move_choice)
        .collect::<Result<Vec<_>, _>>()?;
    let hp_dice = 2 + usize::from(
        special_moves
            .iter()
            .any(|special_move| special_move.id == "hale_and_hearty"),
    );
    let hp_rolls = (0..hp_dice)
        .map(|_| roll_d6(&mut entropy, &mut Vec::new()))
        .collect::<Result<Vec<_>, _>>()?;
    let max_hp = hp_rolls.iter().map(|value| i32::from(*value)).sum::<i32>()
        + request.attributes.constitution;
    let loaded_for_bear = special_moves
        .iter()
        .any(|special_move| special_move.id == "loaded_for_bear");
    let gear_weight_half_units = request.gear.iter().try_fold(0_u16, |total, item| {
        let ordinary = u16::from(item.weight) * 2;
        let effective = if loaded_for_bear
            && item.equipped
            && matches!(
                item.kind,
                DungeonpunkGearKind::Weapon
                    | DungeonpunkGearKind::Armor
                    | DungeonpunkGearKind::Shield
            ) {
            u16::from(item.weight)
        } else {
            ordinary
        };
        total
            .checked_add(effective)
            .ok_or_else(|| invalid("gear", "gear load overflowed"))
    })?;
    let starting_total_half_units = gear_weight_half_units;
    let starting_encumbered = starting_total_half_units > 24;

    let definition_value = definition_value(
        request,
        &hp_rolls,
        max_hp,
        &special_moves,
        gear_weight_half_units,
        loaded_for_bear,
    );
    weave_domain::validate_typed_value(
        "definition",
        &definition_value,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    let definition_sha256 = canonical_fingerprint(&definition_value)?;
    let definition = AdapterCharacterDefinition {
        adapter: adapter.clone(),
        definition: definition_value,
        definition_sha256: definition_sha256.clone(),
        completed_creation_steps: vec![
            "attributes".to_owned(),
            "campaign".to_owned(),
            "gear_and_bonds".to_owned(),
            "identity".to_owned(),
            "moves".to_owned(),
        ],
    };
    let initial_state = TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter: adapter.clone(),
        owner_id: request.character_id.clone(),
        definition_sha256,
        revision: 0,
        entropy: entropy.state(),
        value: initial_state_value(
            request,
            max_hp,
            gear_weight_half_units,
            starting_total_half_units,
            starting_encumbered,
        ),
    };
    validate_tabletop_state(&initial_state, &manifest)?;

    Ok(DungeonpunkCreationPreview {
        creation_format_version: DUNGEONPUNK_CREATION_FORMAT_VERSION,
        request_sha256: canonical_fingerprint(request)?,
        adapter,
        hp_rolls,
        attributes: request.attributes.clone(),
        fate: 1,
        none: 0,
        max_hp,
        special_moves,
        gear_weight_half_units,
        starting_total_half_units,
        starting_encumbered,
        definition,
        initial_state,
        explanations: vec![
            "Starting HP is two deterministic d6 results plus Constitution and any selected endurance-move bonus; play continues from the retained entropy cursor."
                .to_owned(),
            "Load uses half-Weight units so equipped-item reductions and Stress Weight remain exact without floating-point rounding."
                .to_owned(),
            "Core and authored special moves remain declarative data; only the trusted registered resolver can change mutable state."
                .to_owned(),
        ],
    })
}

/// Validate an exported creation preview and its redundant editor-facing derivations.
pub fn validate_dungeonpunk_creation_preview(
    preview: &DungeonpunkCreationPreview,
) -> Result<(), TabletopError> {
    if preview.creation_format_version != DUNGEONPUNK_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_sha256("request_sha256", &preview.request_sha256)?;
    let manifest = dungeonpunk_manifest();
    let adapter = resolved_adapter(&manifest)?;
    if preview.adapter != adapter || preview.definition.adapter != adapter {
        return Err(TabletopError::ContentHashMismatch { path: "adapter" });
    }
    if preview.definition.definition_sha256
        != canonical_fingerprint(&preview.definition.definition)?
        || preview.initial_state.definition_sha256 != preview.definition.definition_sha256
    {
        return Err(TabletopError::ContentHashMismatch {
            path: "definition_sha256",
        });
    }
    if preview.definition.completed_creation_steps
        != [
            "attributes",
            "campaign",
            "gear_and_bonds",
            "identity",
            "moves",
        ]
    {
        return Err(invalid(
            "completed_creation_steps",
            "creation preview is incomplete",
        ));
    }
    let expects_bonus_hp = preview
        .special_moves
        .iter()
        .any(|special_move| special_move.id == "hale_and_hearty");
    if preview.hp_rolls.len() != 2 + usize::from(expects_bonus_hp)
        || preview
            .hp_rolls
            .iter()
            .any(|value| !(1..=6).contains(value))
        || preview.max_hp
            != preview
                .hp_rolls
                .iter()
                .map(|value| i32::from(*value))
                .sum::<i32>()
                + preview.attributes.constitution
        || preview.fate != 1
        || preview.none != 0
        || preview.special_moves.len() != 3
        || preview.starting_total_half_units != preview.gear_weight_half_units
        || preview.starting_encumbered != (preview.starting_total_half_units > 24)
    {
        return Err(invalid(
            "creation_preview",
            "redundant creation values disagree",
        ));
    }
    weave_domain::validate_typed_value(
        "definition",
        &preview.definition.definition,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    validate_tabletop_state(&preview.initial_state, &manifest)?;
    for explanation in &preview.explanations {
        validate_bounded_text("explanations", explanation, 1, 2_048)?;
    }
    Ok(())
}

fn validate_creation_request(request: &DungeonpunkCreationRequest) -> Result<(), TabletopError> {
    if request.creation_format_version != DUNGEONPUNK_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_global_identifier("character_id", &request.character_id)?;
    validate_bounded_text("name", &request.name, 1, 160)?;
    for attribute in DungeonpunkAttribute::ALL {
        if !(0..=3).contains(&request.attributes.value(attribute)) {
            return Err(invalid(
                format!("attributes.{}", attribute.symbol()),
                "starting attributes must range from zero through three",
            ));
        }
    }
    if request.attributes.sum() != 5 {
        return Err(invalid(
            "attributes",
            "exactly five starting points must be allocated",
        ));
    }
    if request.special_moves.len() != 3 {
        return Err(invalid(
            "special_moves",
            "exactly three starting special moves are required",
        ));
    }
    let mut move_ids = BTreeSet::new();
    for special_move in &request.special_moves {
        let expanded = expand_move_choice(special_move)?;
        if !move_ids.insert(expanded.id.clone()) {
            return Err(invalid(
                "special_moves",
                "special move identifiers must be unique",
            ));
        }
    }
    validate_gear(&request.gear)?;
    if request.bonds.len() != 2 {
        return Err(invalid("bonds", "exactly two starting bonds are required"));
    }
    let mut bonds = BTreeSet::new();
    for bond in &request.bonds {
        validate_bounded_text("bonds", bond, 1, 240)?;
        if !bonds.insert(bond) {
            return Err(invalid("bonds", "starting bonds must be distinct"));
        }
    }
    validate_campaign(&request.clocks, &request.threats)
}

fn expand_move_choice(
    choice: &DungeonpunkMoveChoice,
) -> Result<DungeonpunkSpecialMove, TabletopError> {
    match choice {
        DungeonpunkMoveChoice::Core { id } => {
            validate_local_identifier("special_moves.id", id)?;
            CORE_MOVES
                .iter()
                .find(|entry| entry.id == id)
                .copied()
                .map(expand_core_move)
                .ok_or_else(|| invalid("special_moves.id", "unknown core special move"))
        }
        DungeonpunkMoveChoice::Authored {
            id,
            label,
            kind,
            attribute,
            trigger,
            success,
            twist,
            failure,
        } => {
            validate_local_identifier("special_moves.id", id)?;
            if CORE_MOVES.iter().any(|entry| entry.id == id) {
                return Err(invalid(
                    "special_moves.id",
                    "authored move cannot shadow a core move",
                ));
            }
            validate_bounded_text("special_moves.label", label, 1, 160)?;
            validate_bounded_text("special_moves.trigger", trigger, 1, 512)?;
            for (path, value) in [
                ("special_moves.success", success),
                ("special_moves.twist", twist),
                ("special_moves.failure", failure),
            ] {
                validate_bounded_text(path, value, 0, 512)?;
            }
            match kind {
                DungeonpunkMoveKind::Rolled
                    if attribute.is_none()
                        || success.is_empty()
                        || twist.is_empty()
                        || failure.is_empty() =>
                {
                    return Err(invalid(
                        "special_moves",
                        "rolled authored moves require a stat and all three outcome bands",
                    ));
                }
                DungeonpunkMoveKind::Passive | DungeonpunkMoveKind::Session
                    if attribute.is_some()
                        || !success.is_empty()
                        || !twist.is_empty()
                        || !failure.is_empty() =>
                {
                    return Err(invalid(
                        "special_moves",
                        "non-rolled authored moves cannot declare roll outcomes",
                    ));
                }
                _ => {}
            }
            Ok(DungeonpunkSpecialMove {
                id: id.clone(),
                label: label.clone(),
                source: "authored".to_owned(),
                archetype: DungeonpunkArchetype::Custom,
                kind: *kind,
                attribute: *attribute,
                trigger: trigger.clone(),
                success: success.clone(),
                twist: twist.clone(),
                failure: failure.clone(),
            })
        }
    }
}

fn validate_gear(gear: &[DungeonpunkGear]) -> Result<(), TabletopError> {
    if gear.len() > 64 {
        return Err(invalid(
            "gear",
            "no more than 64 carried items are supported",
        ));
    }
    let mut previous: Option<&str> = None;
    for item in gear {
        validate_local_identifier("gear.id", &item.id)?;
        if previous.is_some_and(|value| value >= item.id.as_str()) {
            return Err(invalid(
                "gear.id",
                "gear must be uniquely ordered by identifier",
            ));
        }
        previous = Some(&item.id);
        validate_bounded_text("gear.label", &item.label, 1, 160)?;
        if !matches!(item.weight, 0 | 1 | 2 | 4) {
            return Err(invalid(
                "gear.weight",
                "gear Weight must be zero, one, two, or four",
            ));
        }
        if item.kind == DungeonpunkGearKind::Personal && item.weight != 0 {
            return Err(invalid(
                "gear.weight",
                "ordinary clothes, aids, and personal essentials have zero Weight",
            ));
        }
        if item.uses.is_some_and(|uses| uses > 3) {
            return Err(invalid(
                "gear.uses",
                "starting use tracks cannot exceed three",
            ));
        }
    }
    Ok(())
}

fn validate_campaign(
    clocks: &[DungeonpunkClock],
    threats: &[DungeonpunkThreat],
) -> Result<(), TabletopError> {
    if clocks.len() > 128 || threats.len() > 128 {
        return Err(invalid("campaign", "campaign setup is too large"));
    }
    let mut clock_ids = BTreeSet::new();
    let mut previous_clock: Option<&str> = None;
    for clock in clocks {
        validate_local_identifier("clocks.id", &clock.id)?;
        if previous_clock.is_some_and(|value| value >= clock.id.as_str()) {
            return Err(invalid("clocks.id", "clocks must be uniquely ordered"));
        }
        previous_clock = Some(&clock.id);
        clock_ids.insert(clock.id.as_str());
        validate_bounded_text("clocks.label", &clock.label, 1, 160)?;
        if !(1..=24).contains(&clock.segments) || clock.filled > clock.segments {
            return Err(invalid("clocks", "clock segments or fill are out of range"));
        }
    }
    let mut previous_threat: Option<&str> = None;
    for threat in threats {
        validate_local_identifier("threats.id", &threat.id)?;
        if previous_threat.is_some_and(|value| value >= threat.id.as_str()) {
            return Err(invalid("threats.id", "threats must be uniquely ordered"));
        }
        previous_threat = Some(&threat.id);
        validate_bounded_text("threats.label", &threat.label, 1, 160)?;
        if threat.hit_points > 512 || threat.armor > 32 {
            return Err(invalid("threats", "threat HP or armor is out of range"));
        }
        if threat
            .clock_id
            .as_deref()
            .is_some_and(|clock_id| !clock_ids.contains(clock_id))
        {
            return Err(invalid(
                "threats.clock_id",
                "threat references an unknown clock",
            ));
        }
    }
    Ok(())
}

fn definition_value(
    request: &DungeonpunkCreationRequest,
    hp_rolls: &[u8],
    max_hp: i32,
    moves: &[DungeonpunkSpecialMove],
    gear_weight_half_units: u16,
    loaded_for_bear: bool,
) -> DomainValue {
    object([
        ("attributes", attributes_value(&request.attributes)),
        (
            "bonds",
            DomainValue::List(
                request
                    .bonds
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        (
            "constants",
            object([
                ("fate", DomainValue::Number(1.0)),
                ("none", DomainValue::Number(0.0)),
            ]),
        ),
        ("encumbrance_limit_half_units", DomainValue::Number(24.0)),
        (
            "gear",
            gear_definition_value(&request.gear, loaded_for_bear),
        ),
        (
            "gear_weight_half_units",
            DomainValue::Number(f64::from(gear_weight_half_units)),
        ),
        (
            "hp_rolls",
            DomainValue::List(
                hp_rolls
                    .iter()
                    .map(|value| DomainValue::Number(f64::from(*value)))
                    .collect(),
            ),
        ),
        ("loaded_for_bear", DomainValue::Bool(loaded_for_bear)),
        ("max_hp", DomainValue::Number(f64::from(max_hp))),
        ("name", DomainValue::String(request.name.clone())),
        (
            "special_moves",
            DomainValue::List(moves.iter().map(special_move_value).collect()),
        ),
    ])
}

fn initial_state_value(
    request: &DungeonpunkCreationRequest,
    max_hp: i32,
    gear_weight_half_units: u16,
    total_half_units: u16,
    encumbered: bool,
) -> DomainValue {
    object([
        (
            "advancement",
            object([
                ("learned_moves", DomainValue::List(Vec::new())),
                (
                    "stat_increases",
                    attributes_value(&DungeonpunkAttributes {
                        strength: 0,
                        dexterity: 0,
                        constitution: 0,
                        intelligence: 0,
                        charisma: 0,
                        wisdom: 0,
                    }),
                ),
            ]),
        ),
        (
            "bonds",
            DomainValue::List(
                request
                    .bonds
                    .iter()
                    .cloned()
                    .map(DomainValue::String)
                    .collect(),
            ),
        ),
        ("clocks", clock_state_value(&request.clocks)),
        ("dead", DomainValue::Bool(false)),
        ("death_check_pending", DomainValue::Bool(false)),
        ("encumbered", DomainValue::Bool(encumbered)),
        (
            "gear",
            DomainValue::Object(
                request
                    .gear
                    .iter()
                    .map(|item| {
                        let mut fields = BTreeMap::from([
                            ("damage_marks".to_owned(), DomainValue::Number(0.0)),
                            ("destroyed".to_owned(), DomainValue::Bool(false)),
                        ]);
                        if let Some(uses) = item.uses {
                            fields.insert("uses".to_owned(), DomainValue::Number(f64::from(uses)));
                        }
                        (item.id.clone(), DomainValue::Object(fields))
                    })
                    .collect(),
            ),
        ),
        (
            "gear_weight_half_units",
            DomainValue::Number(f64::from(gear_weight_half_units)),
        ),
        ("hp_current", DomainValue::Number(f64::from(max_hp))),
        ("hp_max", DomainValue::Number(f64::from(max_hp))),
        ("impairments", DomainValue::List(Vec::new())),
        ("last_operation", DomainValue::String(String::new())),
        ("stress", DomainValue::Number(0.0)),
        ("survival_offer_pending", DomainValue::Bool(false)),
        ("threats", threat_state_value(&request.threats)),
        (
            "total_load_half_units",
            DomainValue::Number(f64::from(total_half_units)),
        ),
        ("unconscious", DomainValue::Bool(false)),
        ("xp", DomainValue::Number(0.0)),
    ])
}

fn attributes_value(attributes: &DungeonpunkAttributes) -> DomainValue {
    object([
        (
            "charisma",
            DomainValue::Number(f64::from(attributes.charisma)),
        ),
        (
            "constitution",
            DomainValue::Number(f64::from(attributes.constitution)),
        ),
        (
            "dexterity",
            DomainValue::Number(f64::from(attributes.dexterity)),
        ),
        (
            "intelligence",
            DomainValue::Number(f64::from(attributes.intelligence)),
        ),
        (
            "strength",
            DomainValue::Number(f64::from(attributes.strength)),
        ),
        ("wisdom", DomainValue::Number(f64::from(attributes.wisdom))),
    ])
}

fn special_move_value(special_move: &DungeonpunkSpecialMove) -> DomainValue {
    let mut fields = BTreeMap::from([
        (
            "archetype".to_owned(),
            DomainValue::Symbol(special_move.archetype.symbol().to_owned()),
        ),
        (
            "failure".to_owned(),
            DomainValue::String(special_move.failure.clone()),
        ),
        (
            "id".to_owned(),
            DomainValue::String(special_move.id.clone()),
        ),
        (
            "kind".to_owned(),
            DomainValue::Symbol(special_move.kind.symbol().to_owned()),
        ),
        (
            "label".to_owned(),
            DomainValue::String(special_move.label.clone()),
        ),
        (
            "source".to_owned(),
            DomainValue::Symbol(special_move.source.clone()),
        ),
        (
            "success".to_owned(),
            DomainValue::String(special_move.success.clone()),
        ),
        (
            "trigger".to_owned(),
            DomainValue::String(special_move.trigger.clone()),
        ),
        (
            "twist".to_owned(),
            DomainValue::String(special_move.twist.clone()),
        ),
    ]);
    if let Some(attribute) = special_move.attribute {
        fields.insert(
            "attribute".to_owned(),
            DomainValue::Symbol(attribute.symbol().to_owned()),
        );
    }
    DomainValue::Object(fields)
}

fn gear_definition_value(gear: &[DungeonpunkGear], loaded_for_bear: bool) -> DomainValue {
    DomainValue::Object(
        gear.iter()
            .map(|item| {
                let ordinary = u16::from(item.weight) * 2;
                let effective = if loaded_for_bear
                    && item.equipped
                    && matches!(
                        item.kind,
                        DungeonpunkGearKind::Weapon
                            | DungeonpunkGearKind::Armor
                            | DungeonpunkGearKind::Shield
                    ) {
                    u16::from(item.weight)
                } else {
                    ordinary
                };
                let mut fields = BTreeMap::from([
                    (
                        "effective_weight_half_units".to_owned(),
                        DomainValue::Number(f64::from(effective)),
                    ),
                    ("equipped".to_owned(), DomainValue::Bool(item.equipped)),
                    (
                        "kind".to_owned(),
                        DomainValue::Symbol(item.kind.symbol().to_owned()),
                    ),
                    ("label".to_owned(), DomainValue::String(item.label.clone())),
                    (
                        "weight".to_owned(),
                        DomainValue::Number(f64::from(item.weight)),
                    ),
                ]);
                if let Some(uses) = item.uses {
                    fields.insert(
                        "starting_uses".to_owned(),
                        DomainValue::Number(f64::from(uses)),
                    );
                }
                (item.id.clone(), DomainValue::Object(fields))
            })
            .collect(),
    )
}

fn clock_state_value(clocks: &[DungeonpunkClock]) -> DomainValue {
    DomainValue::Object(
        clocks
            .iter()
            .map(|clock| {
                (
                    clock.id.clone(),
                    object([
                        ("filled", DomainValue::Number(f64::from(clock.filled))),
                        ("label", DomainValue::String(clock.label.clone())),
                        ("segments", DomainValue::Number(f64::from(clock.segments))),
                        (
                            "trigger",
                            DomainValue::Symbol(clock.trigger.symbol().to_owned()),
                        ),
                    ]),
                )
            })
            .collect(),
    )
}

fn threat_state_value(threats: &[DungeonpunkThreat]) -> DomainValue {
    DomainValue::Object(
        threats
            .iter()
            .map(|threat| {
                let mut fields = BTreeMap::from([
                    (
                        "armor".to_owned(),
                        DomainValue::Number(f64::from(threat.armor)),
                    ),
                    (
                        "defeated".to_owned(),
                        DomainValue::Bool(threat.hit_points == 0),
                    ),
                    (
                        "hp_current".to_owned(),
                        DomainValue::Number(f64::from(threat.hit_points)),
                    ),
                    (
                        "hp_max".to_owned(),
                        DomainValue::Number(f64::from(threat.hit_points)),
                    ),
                    (
                        "kind".to_owned(),
                        DomainValue::Symbol(threat.kind.symbol().to_owned()),
                    ),
                    (
                        "label".to_owned(),
                        DomainValue::String(threat.label.clone()),
                    ),
                ]);
                if let Some(clock_id) = &threat.clock_id {
                    fields.insert("clock_id".to_owned(), DomainValue::String(clock_id.clone()));
                }
                (threat.id.clone(), DomainValue::Object(fields))
            })
            .collect(),
    )
}

/// Trusted resolver compiled into Weave for the exact Dungeonpunk adapter release.
#[derive(Debug, Clone, Copy, Default)]
pub struct DungeonpunkResolver;

impl TabletopResolver for DungeonpunkResolver {
    fn adapter_id(&self) -> &str {
        DUNGEONPUNK_ADAPTER_ID
    }

    fn adapter_version(&self) -> &str {
        DUNGEONPUNK_ADAPTER_VERSION
    }

    fn resolve(
        &self,
        operation: &str,
        definition: &DomainValue,
        request: &DomainValue,
        state: &DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        let definition = value_object("definition", definition)?;
        let request = value_object("request", request)?;
        let mut state = value_object("state", state)?.clone();
        let dead = object_bool("state", &state, "dead")?;
        let campaign_operation = matches!(
            operation,
            "gm_move" | "harm_threat" | "harm_threat_minor" | "tick_clock"
        );
        if dead && !campaign_operation {
            return Err(invalid(
                "state.dead",
                "a dead character cannot resolve this operation",
            ));
        }
        if object_bool("state", &state, "death_check_pending")?
            && operation != "death_check"
            && !campaign_operation
        {
            return Err(invalid(
                "state.death_check_pending",
                "resolve the pending FATE death check before this operation",
            ));
        }

        let cursor_before = entropy.state().cursor;
        let mut draws = Vec::new();
        let mut events = match operation {
            "damage" => resolve_damage(definition, request, &mut state, entropy, &mut draws, None)?,
            "death_check" => resolve_death_check(&mut state, entropy, &mut draws)?,
            "gm_move" => resolve_gm_move(request, &state)?,
            "grow" => resolve_growth(definition, request, &mut state)?,
            "harm_threat" => resolve_harm_threat(request, &mut state, entropy, &mut draws, None)?,
            "harm_threat_minor" => {
                resolve_harm_threat(request, &mut state, entropy, &mut draws, Some(1))?
            }
            "impairment" => resolve_impairment(request, &mut state)?,
            "minor_damage" => resolve_damage(
                definition,
                request,
                &mut state,
                entropy,
                &mut draws,
                Some(1),
            )?,
            "rest" => resolve_rest(request, &mut state)?,
            "struggle" => resolve_struggle(definition, request, &mut state, entropy, &mut draws)?,
            "survival_decision" => resolve_survival_decision(request, &mut state)?,
            "tick_clock" => resolve_tick_clock(request, &mut state)?,
            _ => {
                return Err(invalid(
                    "operation",
                    "operation is not implemented by this resolver",
                ));
            }
        };
        state.insert(
            "last_operation".to_owned(),
            DomainValue::String(operation.to_owned()),
        );
        if entropy.state().cursor != cursor_before {
            events.push(entropy_trace_event(
                operation,
                cursor_before,
                entropy.state().cursor,
                draws,
            ));
        }
        Ok(ResolverOutput {
            state: DomainValue::Object(state),
            events,
        })
    }
}

fn resolve_struggle(
    definition: &BTreeMap<String, DomainValue>,
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    if object_bool("state", state, "unconscious")?
        || object_bool("state", state, "survival_offer_pending")?
    {
        return Err(invalid(
            "state",
            "an unconscious character or unresolved survival offer cannot Struggle",
        ));
    }
    let attribute =
        DungeonpunkAttribute::from_symbol(object_symbol("request", request, "attribute")?)?;
    let edge = object_symbol("request", request, "edge")?;
    let push = object_bool("request", request, "push")?;
    let helper_id = optional_string(request, "helper_id")?;
    let helper_encumbered = optional_bool(request, "helper_encumbered")?;
    if helper_id.is_none() != helper_encumbered.is_none() {
        return Err(invalid(
            "request.helper_id",
            "helper identity and helper encumbrance must be supplied together",
        ));
    }
    if helper_encumbered == Some(true) {
        return Err(invalid(
            "request.helper_encumbered",
            "an encumbered ally cannot help",
        ));
    }
    if push && object_bool("state", state, "encumbered")? {
        return Err(invalid(
            "request.push",
            "an encumbered character cannot push",
        ));
    }
    if push {
        let stress = object_integer("state", state, "stress")?;
        if stress > 510 {
            return Err(invalid("state.stress", "Stress limit would be exceeded"));
        }
        set_integer(state, "stress", stress + 2);
        recompute_load(definition, state)?;
    }

    let base = effective_attribute(definition, state, attribute)?;
    let edge_adjustment = match edge {
        "advantage" => 1,
        "disadvantage" => -1,
        "neutral" => 0,
        _ => return Err(invalid("request.edge", "unknown situational edge")),
    };
    let impairment_penalty = i32::from(state_has_impairment(state, attribute)?);
    let encumbrance_penalty = i32::from(object_bool("state", state, "encumbered")?);
    let help_bonus = i32::from(helper_id.is_some());
    let push_bonus = i32::from(push);
    let dice_pool =
        base + edge_adjustment + help_bonus + push_bonus - impairment_penalty - encumbrance_penalty;
    if !(-16..=32).contains(&dice_pool) {
        return Err(invalid("dice_pool", "effective dice pool is out of range"));
    }
    let (dice, selected, selection) = roll_pool(dice_pool, entropy, draws)?;
    let outcome = outcome_band(selected);
    let failure_xp_gained = outcome == "failure";
    if failure_xp_gained {
        let xp = object_integer("state", state, "xp")?;
        if xp >= 1_000_000 {
            return Err(invalid("state.xp", "XP limit would be exceeded"));
        }
        set_integer(state, "xp", xp + 1);
    }

    let (choices, cost_required, partial, failed) = match outcome {
        "success" => (
            vec!["full_success_with_cost", "partial_success_without_cost"],
            false,
            false,
            false,
        ),
        "twist" => (
            vec!["failure_without_extra_cost", "partial_success_with_cost"],
            false,
            true,
            false,
        ),
        _ => (vec!["failure_with_cost"], true, false, true),
    };
    let mut events = vec![
        public_event(
            "roll_resolved",
            object([
                (
                    "attribute",
                    DomainValue::Symbol(attribute.symbol().to_owned()),
                ),
                ("dice", integer_list(dice.iter().copied())),
                ("dice_pool", DomainValue::Number(f64::from(dice_pool))),
                ("edge", DomainValue::Symbol(edge.to_owned())),
                ("failure_xp_gained", DomainValue::Bool(failure_xp_gained)),
                ("helped", DomainValue::Bool(helper_id.is_some())),
                ("outcome", DomainValue::Symbol(outcome.to_owned())),
                ("push", DomainValue::Bool(push)),
                ("selected", DomainValue::Number(f64::from(selected))),
                ("selection", DomainValue::Symbol(selection.to_owned())),
            ]),
        ),
        public_event(
            "struggle_resolved",
            object([
                ("choices", symbol_list(choices)),
                ("cost_required", DomainValue::Bool(cost_required)),
                ("failure", DomainValue::Bool(failed)),
                ("outcome", DomainValue::Symbol(outcome.to_owned())),
                ("partial", DomainValue::Bool(partial)),
            ]),
        ),
    ];
    if let Some(helper_id) = helper_id {
        events.push(public_event(
            "help_applied",
            object([
                ("helper_id", DomainValue::String(helper_id.to_owned())),
                ("stress_delta", DomainValue::Number(1.0)),
            ]),
        ));
    }
    if failure_xp_gained {
        events.push(gm_move_prompt_event("failed_roll", None, None));
    }
    if push || failure_xp_gained {
        events.push(resource_changed_event(state)?);
    }
    Ok(events)
}

fn resolve_damage(
    definition: &BTreeMap<String, DomainValue>,
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
    fixed_damage: Option<i32>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    if object_bool("state", state, "survival_offer_pending")?
        || object_bool("state", state, "death_check_pending")?
    {
        return Err(invalid(
            "state",
            "resolve the pending death outcome before applying more character damage",
        ));
    }
    let (tier, dice, raw_damage) = match fixed_damage {
        Some(amount) => ("minor", Vec::new(), amount),
        None => {
            let tier = object_symbol("request", request, "tier")?;
            let count = damage_dice(tier)?;
            let dice = (0..count)
                .map(|_| roll_d6(entropy, draws))
                .collect::<Result<Vec<_>, _>>()?;
            let amount = dice.iter().map(|value| i32::from(*value)).sum();
            (tier, dice, amount)
        }
    };
    let hp_before = object_integer("state", state, "hp_current")?;
    let brace_item_id = optional_string(request, "brace_item_id")?;
    let mut prevented = 0;
    let mut applied = raw_damage;
    let mut equipment_event = None;
    if let Some(item_id) = brace_item_id {
        validate_local_identifier("request.brace_item_id", item_id)?;
        let gear_definition = object_map("definition.gear", definition, "gear")?;
        let item_definition = gear_definition
            .get(item_id)
            .ok_or_else(|| invalid("request.brace_item_id", "brace item is not carried"))?;
        let item_definition = value_object("definition.gear item", item_definition)?;
        let kind = object_symbol("definition.gear item", item_definition, "kind")?;
        if !matches!(kind, "armor" | "shield")
            || !object_bool("definition.gear item", item_definition, "equipped")?
        {
            return Err(invalid(
                "request.brace_item_id",
                "Brace requires equipped armor or a shield",
            ));
        }
        let weight = object_integer("definition.gear item", item_definition, "weight")?;
        let gear_state = object_map_mut("state.gear", state, "gear")?;
        let item_state = gear_state
            .get_mut(item_id)
            .ok_or_else(|| invalid("state.gear", "brace item state is absent"))?;
        let item_state = value_object_mut("state.gear item", item_state)?;
        if object_bool("state.gear item", item_state, "destroyed")? {
            return Err(invalid(
                "request.brace_item_id",
                "destroyed equipment cannot Brace",
            ));
        }
        let damage_marks = object_integer("state.gear item", item_state, "damage_marks")? + 1;
        let destroyed = damage_marks > weight;
        set_integer(item_state, "damage_marks", damage_marks);
        item_state.insert("destroyed".to_owned(), DomainValue::Bool(destroyed));
        prevented = raw_damage;
        applied = 0;
        equipment_event = Some(public_event(
            "equipment_changed",
            object([
                ("damage_marks", DomainValue::Number(f64::from(damage_marks))),
                ("destroyed", DomainValue::Bool(destroyed)),
                ("item_id", DomainValue::String(item_id.to_owned())),
            ]),
        ));
        recompute_load(definition, state)?;
    }

    let hp_after = hp_before.saturating_sub(applied).max(-128);
    set_integer(state, "hp_current", hp_after);
    let mut status = "active";
    let mut collapse_event = None;
    if hp_after == 0 {
        state.insert("unconscious".to_owned(), DomainValue::Bool(true));
        status = "unconscious";
        collapse_event = Some(death_resolved_event(status, None, false));
    } else if hp_after < 0 {
        state.insert("death_check_pending".to_owned(), DomainValue::Bool(true));
        state.insert("unconscious".to_owned(), DomainValue::Bool(false));
        status = "death_check_pending";
    }
    let mut events = vec![public_event(
        "damage_applied",
        object([
            ("applied", DomainValue::Number(f64::from(applied))),
            ("dice", integer_list(dice.iter().copied())),
            ("hp_after", DomainValue::Number(f64::from(hp_after))),
            ("hp_before", DomainValue::Number(f64::from(hp_before))),
            ("prevented", DomainValue::Number(f64::from(prevented))),
            ("raw_damage", DomainValue::Number(f64::from(raw_damage))),
            ("status", DomainValue::Symbol(status.to_owned())),
            ("tier", DomainValue::Symbol(tier.to_owned())),
        ]),
    )];
    if let Some(event) = equipment_event {
        events.push(event);
    }
    if let Some(event) = collapse_event {
        events.push(event);
    }
    events.push(resource_changed_event(state)?);
    Ok(events)
}

fn resolve_death_check(
    state: &mut BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    if !object_bool("state", state, "death_check_pending")? {
        return Err(invalid(
            "state.death_check_pending",
            "no FATE death check is pending",
        ));
    }
    state.insert("death_check_pending".to_owned(), DomainValue::Bool(false));
    let fate_die = roll_d6(entropy, draws)?;
    let status = match outcome_band(fate_die) {
        "success" => {
            state.insert("unconscious".to_owned(), DomainValue::Bool(true));
            "unconscious"
        }
        "twist" => {
            state.insert("survival_offer_pending".to_owned(), DomainValue::Bool(true));
            state.insert("unconscious".to_owned(), DomainValue::Bool(false));
            "survival_offer"
        }
        _ => {
            state.insert("dead".to_owned(), DomainValue::Bool(true));
            state.insert("unconscious".to_owned(), DomainValue::Bool(false));
            "dead"
        }
    };
    Ok(vec![
        death_resolved_event(status, Some(fate_die), status == "survival_offer"),
        resource_changed_event(state)?,
    ])
}

fn resolve_survival_decision(
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    if !object_bool("state", state, "survival_offer_pending")? {
        return Err(invalid(
            "state.survival_offer_pending",
            "no survival offer is pending",
        ));
    }
    let accept = object_bool("request", request, "accept")?;
    state.insert(
        "survival_offer_pending".to_owned(),
        DomainValue::Bool(false),
    );
    let status = if accept {
        state.insert("unconscious".to_owned(), DomainValue::Bool(true));
        "survived_at_cost"
    } else {
        state.insert("dead".to_owned(), DomainValue::Bool(true));
        state.insert("unconscious".to_owned(), DomainValue::Bool(false));
        "dead"
    };
    Ok(vec![
        death_resolved_event(status, None, accept),
        resource_changed_event(state)?,
    ])
}

fn resolve_rest(
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    if object_bool("state", state, "survival_offer_pending")?
        || object_bool("state", state, "death_check_pending")?
    {
        return Err(invalid(
            "state",
            "a pending death outcome must be resolved before rest",
        ));
    }
    let hours = object_integer("request", request, "hours")?;
    if !(2..=336).contains(&hours) {
        return Err(invalid(
            "request.hours",
            "rest must last from two to 336 hours",
        ));
    }
    let recovery = hours / 2;
    let hp_before = object_integer("state", state, "hp_current")?;
    let hp_max = object_integer("state", state, "hp_max")?;
    let stress_before = object_integer("state", state, "stress")?;
    let hp_after = (hp_before.max(0) + recovery).min(hp_max);
    let stress_after = (stress_before - recovery).max(0);
    set_integer(state, "hp_current", hp_after);
    set_integer(state, "stress", stress_after);
    if hp_after > 0 {
        state.insert("unconscious".to_owned(), DomainValue::Bool(false));
    }
    let definition_stub = BTreeMap::new();
    recompute_load_with_fixed_gear(&definition_stub, state)?;
    Ok(vec![
        public_event(
            "rest_resolved",
            object([
                ("hours", DomainValue::Number(f64::from(hours))),
                ("hp_after", DomainValue::Number(f64::from(hp_after))),
                ("hp_before", DomainValue::Number(f64::from(hp_before))),
                ("stress_after", DomainValue::Number(f64::from(stress_after))),
                (
                    "stress_before",
                    DomainValue::Number(f64::from(stress_before)),
                ),
            ]),
        ),
        resource_changed_event(state)?,
    ])
}

fn resolve_impairment(
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    let attribute =
        DungeonpunkAttribute::from_symbol(object_symbol("request", request, "attribute")?)?;
    let impaired = object_bool("request", request, "impaired")?;
    let list = object_list_mut("state", state, "impairments")?;
    let mut values = list
        .iter()
        .map(|value| match value {
            DomainValue::Symbol(value) => Ok(value.clone()),
            _ => Err(invalid("state.impairments", "impairment list is invalid")),
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if impaired {
        values.insert(attribute.symbol().to_owned());
    } else {
        values.remove(attribute.symbol());
    }
    *list = values.into_iter().map(DomainValue::Symbol).collect();
    Ok(vec![public_event(
        "impairment_changed",
        object([
            (
                "attribute",
                DomainValue::Symbol(attribute.symbol().to_owned()),
            ),
            ("impaired", DomainValue::Bool(impaired)),
        ]),
    )])
}

fn resolve_growth(
    definition: &BTreeMap<String, DomainValue>,
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    let kind = object_symbol("request", request, "kind")?;
    let xp = object_integer("state", state, "xp")?;
    let mut event_fields =
        BTreeMap::from([("kind".to_owned(), DomainValue::Symbol(kind.to_owned()))]);
    let cost = match kind {
        "add_move" => {
            let move_id = optional_string(request, "move_id")?
                .ok_or_else(|| invalid("request.move_id", "adding a move requires its id"))?;
            let move_label = optional_string(request, "move_label")?
                .ok_or_else(|| invalid("request.move_label", "adding a move requires its label"))?;
            validate_local_identifier("request.move_id", move_id)?;
            validate_bounded_text("request.move_label", move_label, 1, 160)?;
            if definition_has_move(definition, move_id)? || state_has_learned_move(state, move_id)?
            {
                return Err(invalid("request.move_id", "special move is already known"));
            }
            let current_moves = object_list("definition", definition, "special_moves")?.len()
                + learned_moves(state)?.len();
            let cost = i32::try_from(current_moves)
                .map_err(|_| invalid("state.advancement", "move count is out of range"))?;
            if xp < cost {
                return Err(invalid("state.xp", "not enough XP for this special move"));
            }
            learned_moves_mut(state)?.push(object([
                ("id", DomainValue::String(move_id.to_owned())),
                ("label", DomainValue::String(move_label.to_owned())),
            ]));
            event_fields.insert(
                "move_id".to_owned(),
                DomainValue::String(move_id.to_owned()),
            );
            cost
        }
        "increase_stat" => {
            let attribute = DungeonpunkAttribute::from_symbol(
                optional_symbol(request, "attribute")?.ok_or_else(|| {
                    invalid("request.attribute", "stat growth requires an attribute")
                })?,
            )?;
            let base_sum = DungeonpunkAttribute::ALL
                .into_iter()
                .map(|attribute| definition_attribute(definition, attribute))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .sum::<i32>();
            let increases = advancement_increases(state)?;
            let increase_sum = DungeonpunkAttribute::ALL
                .into_iter()
                .map(|attribute| {
                    object_integer("state.stat_increases", increases, attribute.symbol())
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .sum::<i32>();
            let cost = 2 * (base_sum + increase_sum);
            if xp < cost {
                return Err(invalid("state.xp", "not enough XP for this stat increase"));
            }
            let increases = advancement_increases_mut(state)?;
            let current = object_integer("state.stat_increases", increases, attribute.symbol())?;
            if current >= 12 {
                return Err(invalid(
                    "state.stat_increases",
                    "stat increase limit reached",
                ));
            }
            set_integer(increases, attribute.symbol(), current + 1);
            event_fields.insert(
                "attribute".to_owned(),
                DomainValue::Symbol(attribute.symbol().to_owned()),
            );
            event_fields.insert(
                "new_rating".to_owned(),
                DomainValue::Number(f64::from(
                    definition_attribute(definition, attribute)? + current + 1,
                )),
            );
            cost
        }
        _ => return Err(invalid("request.kind", "unknown growth option")),
    };
    set_integer(state, "xp", xp - cost);
    event_fields.insert("cost".to_owned(), DomainValue::Number(f64::from(cost)));
    Ok(vec![
        public_event("advancement_applied", DomainValue::Object(event_fields)),
        resource_changed_event(state)?,
    ])
}

fn resolve_tick_clock(
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    let clock_id = object_string("request", request, "clock_id")?;
    validate_local_identifier("request.clock_id", clock_id)?;
    let delta = object_integer("request", request, "delta")?;
    let clocks = object_map_mut("state", state, "clocks")?;
    let clock = clocks
        .get_mut(clock_id)
        .ok_or_else(|| invalid("request.clock_id", "unknown clock"))?;
    let clock = value_object_mut("state.clock", clock)?;
    let before = object_integer("state.clock", clock, "filled")?;
    let segments = object_integer("state.clock", clock, "segments")?;
    let after = (before + delta).clamp(0, segments);
    set_integer(clock, "filled", after);
    let trigger = object_symbol("state.clock", clock, "trigger")?;
    let triggered = after != before
        && ((trigger == "empty" && after == 0) || (trigger == "full" && after == segments));
    let mut events = vec![public_event(
        "clock_changed",
        object([
            ("after", DomainValue::Number(f64::from(after))),
            ("before", DomainValue::Number(f64::from(before))),
            ("clock_id", DomainValue::String(clock_id.to_owned())),
            ("delta", DomainValue::Number(f64::from(delta))),
            ("segments", DomainValue::Number(f64::from(segments))),
        ]),
    )];
    if triggered {
        events.push(public_event(
            "clock_triggered",
            object([
                ("boundary", DomainValue::Symbol(trigger.to_owned())),
                ("clock_id", DomainValue::String(clock_id.to_owned())),
            ]),
        ));
    }
    Ok(events)
}

fn resolve_harm_threat(
    request: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
    fixed_damage: Option<i32>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    let threat_id = object_string("request", request, "threat_id")?;
    validate_local_identifier("request.threat_id", threat_id)?;
    let (tier, dice, raw_damage) = match fixed_damage {
        Some(amount) => ("minor", Vec::new(), amount),
        None => {
            let tier = object_symbol("request", request, "tier")?;
            let dice = (0..damage_dice(tier)?)
                .map(|_| roll_d6(entropy, draws))
                .collect::<Result<Vec<_>, _>>()?;
            let amount = dice.iter().map(|value| i32::from(*value)).sum();
            (tier, dice, amount)
        }
    };
    let threats = object_map_mut("state", state, "threats")?;
    let threat = threats
        .get_mut(threat_id)
        .ok_or_else(|| invalid("request.threat_id", "unknown threat"))?;
    let threat = value_object_mut("state.threat", threat)?;
    if object_bool("state.threat", threat, "defeated")? {
        return Err(invalid("state.threat", "threat is already defeated"));
    }
    let hp_before = object_integer("state.threat", threat, "hp_current")?;
    let armor = object_integer("state.threat", threat, "armor")?;
    let applied = (raw_damage - armor).max(0);
    let hp_after = (hp_before - applied).max(0);
    let defeated = hp_after == 0;
    set_integer(threat, "hp_current", hp_after);
    threat.insert("defeated".to_owned(), DomainValue::Bool(defeated));
    Ok(vec![public_event(
        "threat_changed",
        object([
            ("applied", DomainValue::Number(f64::from(applied))),
            ("armor", DomainValue::Number(f64::from(armor))),
            ("defeated", DomainValue::Bool(defeated)),
            ("dice", integer_list(dice.iter().copied())),
            ("hp_after", DomainValue::Number(f64::from(hp_after))),
            ("hp_before", DomainValue::Number(f64::from(hp_before))),
            ("raw_damage", DomainValue::Number(f64::from(raw_damage))),
            ("threat_id", DomainValue::String(threat_id.to_owned())),
            ("tier", DomainValue::Symbol(tier.to_owned())),
        ]),
    )])
}

fn resolve_gm_move(
    request: &BTreeMap<String, DomainValue>,
    state: &BTreeMap<String, DomainValue>,
) -> Result<Vec<ResolverEvent>, TabletopError> {
    let category = object_symbol("request", request, "category")?;
    let selected_move = object_symbol("request", request, "move")?;
    let threat_id = optional_string(request, "threat_id")?;
    if category == "threat_specific" {
        let threat_id = threat_id
            .ok_or_else(|| invalid("request.threat_id", "threat move requires a threat"))?;
        let threats = object_map("state", state, "threats")?;
        let threat = threats
            .get(threat_id)
            .ok_or_else(|| invalid("request.threat_id", "unknown threat"))?;
        let threat = value_object("state.threat", threat)?;
        let kind = object_symbol("state.threat", threat, "kind")?;
        if !threat_moves(kind).contains(&selected_move) {
            return Err(invalid(
                "request.move",
                "move does not belong to this threat kind",
            ));
        }
    } else {
        if threat_id.is_some() {
            return Err(invalid(
                "request.threat_id",
                "general GM moves cannot carry a threat id",
            ));
        }
        if !general_moves(category).contains(&selected_move) {
            return Err(invalid(
                "request.move",
                "move does not belong to this GM category",
            ));
        }
    }
    let mut fields = BTreeMap::from([
        (
            "category".to_owned(),
            DomainValue::Symbol(category.to_owned()),
        ),
        (
            "move".to_owned(),
            DomainValue::Symbol(selected_move.to_owned()),
        ),
    ]);
    if let Some(threat_id) = threat_id {
        fields.insert(
            "threat_id".to_owned(),
            DomainValue::String(threat_id.to_owned()),
        );
    }
    Ok(vec![public_event(
        "gm_move_selected",
        DomainValue::Object(fields),
    )])
}

fn damage_dice(tier: &str) -> Result<usize, TabletopError> {
    match tier {
        "standard" => Ok(1),
        "serious" => Ok(2),
        "deadly" => Ok(4),
        "catastrophic" => Ok(8),
        _ => Err(invalid("request.tier", "unknown rolled damage tier")),
    }
}

fn roll_pool(
    dice_pool: i32,
    entropy: &mut EntropyStream,
    draws: &mut Vec<u64>,
) -> Result<(Vec<u8>, u8, &'static str), TabletopError> {
    let count = if dice_pool > 0 {
        usize::try_from(dice_pool)
            .map_err(|_| invalid("dice_pool", "positive dice pool is out of range"))?
    } else {
        2
    };
    let dice = (0..count)
        .map(|_| roll_d6(entropy, draws))
        .collect::<Result<Vec<_>, _>>()?;
    let selected = if dice_pool > 0 {
        *dice
            .iter()
            .max()
            .ok_or_else(|| invalid("dice_pool", "positive pool produced no dice"))?
    } else {
        *dice
            .iter()
            .min()
            .ok_or_else(|| invalid("dice_pool", "fallback pool produced no dice"))?
    };
    Ok((
        dice,
        selected,
        if dice_pool > 0 { "highest" } else { "lowest" },
    ))
}

fn roll_d6(entropy: &mut EntropyStream, draws: &mut Vec<u64>) -> Result<u8, TabletopError> {
    let result = entropy.draw_bounded(6)? + 1;
    draws.push(result);
    u8::try_from(result).map_err(|_| invalid("entropy", "d6 result is out of range"))
}

const fn outcome_band(die: u8) -> &'static str {
    match die {
        6 => "success",
        4 | 5 => "twist",
        _ => "failure",
    }
}

fn effective_attribute(
    definition: &BTreeMap<String, DomainValue>,
    state: &BTreeMap<String, DomainValue>,
    attribute: DungeonpunkAttribute,
) -> Result<i32, TabletopError> {
    Ok(definition_attribute(definition, attribute)?
        + object_integer(
            "state.stat_increases",
            advancement_increases(state)?,
            attribute.symbol(),
        )?)
}

fn definition_attribute(
    definition: &BTreeMap<String, DomainValue>,
    attribute: DungeonpunkAttribute,
) -> Result<i32, TabletopError> {
    let attributes = object_map("definition", definition, "attributes")?;
    object_integer("definition.attributes", attributes, attribute.symbol())
}

fn state_has_impairment(
    state: &BTreeMap<String, DomainValue>,
    attribute: DungeonpunkAttribute,
) -> Result<bool, TabletopError> {
    Ok(object_list("state", state, "impairments")?
        .iter()
        .any(|value| matches!(value, DomainValue::Symbol(symbol) if symbol == attribute.symbol())))
}

fn recompute_load(
    definition: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<(), TabletopError> {
    let gear_definition = object_map("definition", definition, "gear")?;
    let gear_state = object_map("state", state, "gear")?;
    let mut gear_half_units = 0_i32;
    for (item_id, item_definition) in gear_definition {
        let item_state = gear_state
            .get(item_id)
            .ok_or_else(|| invalid("state.gear", "gear state is incomplete"))?;
        let item_state = value_object("state.gear item", item_state)?;
        if !object_bool("state.gear item", item_state, "destroyed")? {
            gear_half_units += object_integer(
                "definition.gear item",
                value_object("definition.gear item", item_definition)?,
                "effective_weight_half_units",
            )?;
        }
    }
    set_integer(state, "gear_weight_half_units", gear_half_units);
    recompute_load_with_fixed_gear(definition, state)
}

fn recompute_load_with_fixed_gear(
    _definition: &BTreeMap<String, DomainValue>,
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<(), TabletopError> {
    let gear = object_integer("state", state, "gear_weight_half_units")?;
    let stress = object_integer("state", state, "stress")?;
    let total = gear
        .checked_add(stress.saturating_mul(2))
        .ok_or_else(|| invalid("state.total_load_half_units", "load overflowed"))?;
    set_integer(state, "total_load_half_units", total);
    state.insert("encumbered".to_owned(), DomainValue::Bool(total > 24));
    Ok(())
}

fn advancement_increases(
    state: &BTreeMap<String, DomainValue>,
) -> Result<&BTreeMap<String, DomainValue>, TabletopError> {
    let advancement = object_map("state", state, "advancement")?;
    object_map("state.advancement", advancement, "stat_increases")
}

fn advancement_increases_mut(
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<&mut BTreeMap<String, DomainValue>, TabletopError> {
    let advancement = object_map_mut("state", state, "advancement")?;
    object_map_mut("state.advancement", advancement, "stat_increases")
}

fn learned_moves(
    state: &BTreeMap<String, DomainValue>,
) -> Result<&Vec<DomainValue>, TabletopError> {
    let advancement = object_map("state", state, "advancement")?;
    object_list("state.advancement", advancement, "learned_moves")
}

fn learned_moves_mut(
    state: &mut BTreeMap<String, DomainValue>,
) -> Result<&mut Vec<DomainValue>, TabletopError> {
    let advancement = object_map_mut("state", state, "advancement")?;
    object_list_mut("state.advancement", advancement, "learned_moves")
}

fn definition_has_move(
    definition: &BTreeMap<String, DomainValue>,
    move_id: &str,
) -> Result<bool, TabletopError> {
    object_list("definition", definition, "special_moves")?
        .iter()
        .map(|value| {
            Ok(object_string(
                "definition.special_move",
                value_object("definition.special_move", value)?,
                "id",
            )? == move_id)
        })
        .collect::<Result<Vec<_>, TabletopError>>()
        .map(|matches| matches.into_iter().any(|value| value))
}

fn state_has_learned_move(
    state: &BTreeMap<String, DomainValue>,
    move_id: &str,
) -> Result<bool, TabletopError> {
    learned_moves(state)?
        .iter()
        .map(|value| {
            Ok(object_string(
                "state.learned_move",
                value_object("state.learned_move", value)?,
                "id",
            )? == move_id)
        })
        .collect::<Result<Vec<_>, TabletopError>>()
        .map(|matches| matches.into_iter().any(|value| value))
}

fn general_moves(category: &str) -> &'static [&'static str] {
    match category {
        "escalate" => &["drain_resource", "introduce_problem", "worsen_situation"],
        "injure" => &["damage_armor", "hurt_ally", "impair_stat", "kill"],
        "rob" => &["take_or_damage_asset"],
        "twist_outcome" => &["apparent_reward", "partial_success", "success_at_cost"],
        "warn" => &["show_distant_trouble", "start_or_tick_clock"],
        _ => &[],
    }
}

fn threat_moves(kind: &str) -> &'static [&'static str] {
    match kind {
        "affliction" => &["complicate_simple_action", "false_perception", "spread"],
        "cursed_place" => &["disgorge", "leave_mark", "lure", "rearrange"],
        "foe" => &["follow_fiction"],
        "overlord" => &[
            "claim_ground",
            "forceful_attack",
            "negotiate",
            "seize_leverage",
        ],
        "swarm" => &["grow", "overwhelm", "raise_champion"],
        "terror" => &["inflict_horror", "sudden_appearance"],
        _ => &[],
    }
}

fn gm_move_prompt_event(
    reason: &str,
    threat_id: Option<&str>,
    threat_kind: Option<&str>,
) -> ResolverEvent {
    let mut fields = BTreeMap::from([
        (
            "categories".to_owned(),
            symbol_list(["escalate", "injure", "rob", "twist_outcome", "warn"]),
        ),
        ("reason".to_owned(), DomainValue::Symbol(reason.to_owned())),
    ]);
    if let Some(threat_id) = threat_id {
        fields.insert(
            "threat_id".to_owned(),
            DomainValue::String(threat_id.to_owned()),
        );
    }
    if let Some(threat_kind) = threat_kind {
        fields.insert(
            "threat_kind".to_owned(),
            DomainValue::Symbol(threat_kind.to_owned()),
        );
    }
    public_event("gm_move_prompt", DomainValue::Object(fields))
}

fn death_resolved_event(status: &str, fate_die: Option<u8>, cost_required: bool) -> ResolverEvent {
    let mut fields = BTreeMap::from([
        ("cost_required".to_owned(), DomainValue::Bool(cost_required)),
        ("status".to_owned(), DomainValue::Symbol(status.to_owned())),
    ]);
    if let Some(fate_die) = fate_die {
        fields.insert(
            "fate_die".to_owned(),
            DomainValue::Number(f64::from(fate_die)),
        );
    }
    public_event("death_resolved", DomainValue::Object(fields))
}

fn resource_changed_event(
    state: &BTreeMap<String, DomainValue>,
) -> Result<ResolverEvent, TabletopError> {
    Ok(ResolverEvent {
        kind: "resource_changed".to_owned(),
        visibility: EventVisibility::Authoring,
        payload: object([
            (
                "dead",
                DomainValue::Bool(object_bool("state", state, "dead")?),
            ),
            (
                "death_check_pending",
                DomainValue::Bool(object_bool("state", state, "death_check_pending")?),
            ),
            (
                "encumbered",
                DomainValue::Bool(object_bool("state", state, "encumbered")?),
            ),
            (
                "hp_current",
                DomainValue::Number(f64::from(object_integer("state", state, "hp_current")?)),
            ),
            (
                "stress",
                DomainValue::Number(f64::from(object_integer("state", state, "stress")?)),
            ),
            (
                "total_load_half_units",
                DomainValue::Number(f64::from(object_integer(
                    "state",
                    state,
                    "total_load_half_units",
                )?)),
            ),
            (
                "unconscious",
                DomainValue::Bool(object_bool("state", state, "unconscious")?),
            ),
            (
                "xp",
                DomainValue::Number(f64::from(object_integer("state", state, "xp")?)),
            ),
        ]),
    })
}

fn public_event(kind: &str, payload: DomainValue) -> ResolverEvent {
    ResolverEvent {
        kind: kind.to_owned(),
        visibility: EventVisibility::Public,
        payload,
    }
}

fn entropy_trace_event(
    operation: &str,
    cursor_before: u64,
    cursor_after: u64,
    draws: Vec<u64>,
) -> ResolverEvent {
    ResolverEvent {
        kind: "entropy_trace".to_owned(),
        visibility: EventVisibility::HostOnly,
        payload: object([
            ("cursor_after", DomainValue::Number(cursor_after as f64)),
            ("cursor_before", DomainValue::Number(cursor_before as f64)),
            ("draws", integer_list(draws)),
            ("operation", DomainValue::String(operation.to_owned())),
        ]),
    }
}

/// Build the exact verified manifest shared by every Dungeonpunk surface.
#[must_use]
pub fn dungeonpunk_manifest() -> AdapterManifest {
    let attributes_type = object_type([
        ("charisma", required(integer(0.0, 15.0), "Charisma rating.")),
        (
            "constitution",
            required(integer(0.0, 15.0), "Constitution rating."),
        ),
        (
            "dexterity",
            required(integer(0.0, 15.0), "Dexterity rating."),
        ),
        (
            "intelligence",
            required(integer(0.0, 15.0), "Intelligence rating."),
        ),
        ("strength", required(integer(0.0, 15.0), "Strength rating.")),
        ("wisdom", required(integer(0.0, 15.0), "Wisdom rating.")),
    ]);
    let special_move_type = object_type([
        (
            "archetype",
            required(archetype_symbol(), "Core grouping or custom origin."),
        ),
        (
            "attribute",
            optional(attribute_symbol(), "Attribute rolled by this move."),
        ),
        (
            "failure",
            required(string(0, 512), "Independent failure-band summary."),
        ),
        ("id", required(string(1, 128), "Stable special-move id.")),
        (
            "kind",
            required(
                symbol(["passive", "rolled", "session"]),
                "Move interaction kind.",
            ),
        ),
        ("label", required(string(1, 160), "Move display label.")),
        (
            "source",
            required(symbol(["authored", "core"]), "Move metadata origin."),
        ),
        (
            "success",
            required(string(0, 512), "Independent success-band summary."),
        ),
        (
            "trigger",
            required(
                string(1, 512),
                "Independent move trigger or passive effect.",
            ),
        ),
        (
            "twist",
            required(string(0, 512), "Independent twist-band summary."),
        ),
    ]);
    let gear_definition_type = object_type([
        (
            "effective_weight_half_units",
            required(integer(0.0, 8.0), "Load after equipped-item effects."),
        ),
        (
            "equipped",
            required(TypeExpression::Bool, "Whether the item is worn or wielded."),
        ),
        (
            "kind",
            required(gear_kind_symbol(), "Portable item category."),
        ),
        ("label", required(string(1, 160), "Item display label.")),
        (
            "starting_uses",
            optional(integer(0.0, 3.0), "Initial bounded use track."),
        ),
        ("weight", required(integer(0.0, 4.0), "Whole base Weight.")),
    ]);
    let gear_state_type = object_type([
        (
            "damage_marks",
            required(integer(0.0, 255.0), "Brace damage marks."),
        ),
        (
            "destroyed",
            required(
                TypeExpression::Bool,
                "Whether damage marks exceeded Weight.",
            ),
        ),
        (
            "uses",
            optional(integer(0.0, 3.0), "Remaining consumable uses."),
        ),
    ]);
    let learned_move_type = object_type([
        ("id", required(string(1, 128), "Learned move identifier.")),
        ("label", required(string(1, 160), "Learned move label.")),
    ]);
    let advancement_type = object_type([
        (
            "learned_moves",
            required(
                list(named("LearnedMove"), 0, 128),
                "Moves gained after creation.",
            ),
        ),
        (
            "stat_increases",
            required(named("Attributes"), "Post-creation stat increases."),
        ),
    ]);
    let clock_state_type = object_type([
        ("filled", required(integer(0.0, 24.0), "Filled segments.")),
        ("label", required(string(1, 160), "Clock display label.")),
        (
            "segments",
            required(integer(1.0, 24.0), "Total clock segments."),
        ),
        (
            "trigger",
            required(
                symbol(["empty", "full"]),
                "Boundary that triggers an event.",
            ),
        ),
    ]);
    let threat_state_type = object_type([
        ("armor", required(integer(0.0, 32.0), "Damage reduction.")),
        (
            "clock_id",
            optional(string(1, 128), "Related campaign clock."),
        ),
        (
            "defeated",
            required(TypeExpression::Bool, "Whether HP reached zero."),
        ),
        (
            "hp_current",
            required(integer(0.0, 512.0), "Current threat HP."),
        ),
        (
            "hp_max",
            required(integer(0.0, 512.0), "Maximum threat HP."),
        ),
        (
            "kind",
            required(threat_kind_symbol(), "Threat move category."),
        ),
        ("label", required(string(1, 160), "Threat display label.")),
    ]);
    let types = BTreeMap::from([
        ("Advancement".to_owned(), advancement_type),
        ("Attributes".to_owned(), attributes_type),
        ("ClockState".to_owned(), clock_state_type),
        ("GearDefinition".to_owned(), gear_definition_type),
        ("GearState".to_owned(), gear_state_type),
        ("LearnedMove".to_owned(), learned_move_type),
        ("SpecialMove".to_owned(), special_move_type),
        ("ThreatState".to_owned(), threat_state_type),
    ]);

    let definition_type = object_type([
        (
            "attributes",
            required(named("Attributes"), "Five allocated starting points."),
        ),
        (
            "bonds",
            required(list(string(1, 240), 2, 2), "Exactly two starting bonds."),
        ),
        (
            "constants",
            required(
                object_type([
                    ("fate", required(integer(1.0, 1.0), "Constant FATE rating.")),
                    ("none", required(integer(0.0, 0.0), "Constant NONE rating.")),
                ]),
                "Fixed roll constants.",
            ),
        ),
        (
            "encumbrance_limit_half_units",
            required(integer(24.0, 24.0), "Twelve Weight in exact half units."),
        ),
        (
            "gear",
            required(
                map(named("GearDefinition"), 0, 64),
                "Immutable starting gear definitions.",
            ),
        ),
        (
            "gear_weight_half_units",
            required(integer(0.0, 512.0), "Derived starting gear load."),
        ),
        (
            "hp_rolls",
            required(
                list(integer(1.0, 6.0), 2, 3),
                "Two base HP dice and an optional endurance bonus die.",
            ),
        ),
        (
            "loaded_for_bear",
            required(TypeExpression::Bool, "Equipped gear half-Weight effect."),
        ),
        (
            "max_hp",
            required(integer(2.0, 32.0), "Derived maximum HP."),
        ),
        ("name", required(string(1, 160), "Fictional display name.")),
        (
            "special_moves",
            required(
                list(named("SpecialMove"), 3, 3),
                "Three selected or independently authored moves.",
            ),
        ),
    ]);
    let state_type = object_type([
        (
            "advancement",
            required(named("Advancement"), "Learned moves and stat increases."),
        ),
        (
            "bonds",
            required(
                list(string(1, 240), 2, 32),
                "Portable character bonds, initialized with exactly two entries.",
            ),
        ),
        (
            "clocks",
            required(map(named("ClockState"), 0, 128), "Campaign clocks."),
        ),
        ("dead", required(TypeExpression::Bool, "Final death state.")),
        (
            "death_check_pending",
            required(
                TypeExpression::Bool,
                "Whether below-zero HP requires an explicit FATE roll.",
            ),
        ),
        (
            "encumbered",
            required(
                TypeExpression::Bool,
                "Whether total load exceeds twelve Weight.",
            ),
        ),
        (
            "gear",
            required(map(named("GearState"), 0, 64), "Mutable item state."),
        ),
        (
            "gear_weight_half_units",
            required(integer(0.0, 512.0), "Current carried gear load."),
        ),
        (
            "hp_current",
            required(
                integer(-128.0, 1_024.0),
                "Current HP, including below zero.",
            ),
        ),
        (
            "hp_max",
            required(integer(2.0, 1_024.0), "Current maximum HP."),
        ),
        (
            "impairments",
            required(
                list(attribute_symbol(), 0, 6),
                "Impaired stats with a one-die penalty.",
            ),
        ),
        (
            "last_operation",
            required(string(0, 128), "Most recently resolved operation."),
        ),
        ("stress", required(integer(0.0, 512.0), "Current Stress.")),
        (
            "survival_offer_pending",
            required(
                TypeExpression::Bool,
                "Twist-band death offer awaiting a decision.",
            ),
        ),
        (
            "threats",
            required(map(named("ThreatState"), 0, 128), "Campaign threats."),
        ),
        (
            "total_load_half_units",
            required(integer(0.0, 2_048.0), "Gear plus Stress load."),
        ),
        (
            "unconscious",
            required(
                TypeExpression::Bool,
                "Whether the character is unconscious.",
            ),
        ),
        ("xp", required(integer(0.0, 1_000_000.0), "Unspent XP.")),
    ]);

    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: DUNGEONPUNK_ADAPTER_ID.to_owned(),
        version: DUNGEONPUNK_ADAPTER_VERSION.to_owned(),
        schema_version: 1,
        namespace: "dungeonpunk".to_owned(),
        title: "Dungeonpunk compatible adapter".to_owned(),
        compatibility_label:
            "Compatible with the CC0 Dungeonpunk core rules; not endorsed by its creators"
                .to_owned(),
        summary: "An independently implemented d6-pool adapter for creation, Struggle outcomes, Stress and HP, gear load and Brace, impairment, growth, clocks, threats, and structured GM moves."
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
            capability(TabletopCapability::EquipmentAndAbilities),
            capability(TabletopCapability::Advancement),
            capability(TabletopCapability::Encounters),
            capability(TabletopCapability::Clocks),
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
            source_url: DUNGEONPUNK_SOURCE_URL.to_owned(),
            exact_artifact: "dungeonpunk-google-doc.pdf".to_owned(),
            revision: DUNGEONPUNK_SOURCE_REVISION.to_owned(),
            retrieved_on: "2026-08-25".to_owned(),
            sha256: DUNGEONPUNK_PDF_SHA256.to_owned(),
            license: "CC0-1.0".to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                .to_owned(),
            covered_files_or_sections: vec![
                "six attributes, fixed FATE and NONE ratings, creation points, HP, Stress, XP, gear, and bonds"
                    .to_owned(),
                "positive and non-positive d6 pools, outcome bands, help, push, advantage, disadvantage, and Struggle choices"
                    .to_owned(),
                "encumbrance, rest, damage tiers, impairment, collapse, death, Brace, and growth"
                    .to_owned(),
                "core archetype move inventory, campaign clocks, threats, foes, armor, and GM move categories"
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
            attribution: "Compatibility implementation for Dungeonpunk by Ash McAllan, with the official core document released under CC0-1.0. Adapter code, fixture prose, scenarios, and UI summaries are independently authored for Weave."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "LICENSE-CC0-1.0.txt".to_owned(),
                sha256: CC0_1_0_LEGAL_CODE_SHA256.to_owned(),
            },
            additional_artifacts: vec![AdapterSourceArtifact {
                source_url: DUNGEONPUNK_SOURCE_URL.to_owned(),
                exact_artifact: "dungeonpunk-google-doc.txt".to_owned(),
                revision: DUNGEONPUNK_SOURCE_REVISION.to_owned(),
                retrieved_on: "2026-08-25".to_owned(),
                sha256: DUNGEONPUNK_TEXT_SHA256.to_owned(),
                media_type: "text/plain".to_owned(),
                purpose: "Pin the stable textual content independently from the Google PDF renderer and confirm the reviewed revision's semantic rules surface."
                    .to_owned(),
                redistributed: false,
            }],
            notices: vec![
                format!("The official release page is {DUNGEONPUNK_RELEASE_URL}."),
                "The official PDF and text exports are audit inputs and are not redistributed with Weave."
                    .to_owned(),
                "The community wiki and every third-party supplement remain outside this adapter's reviewed source boundary."
                    .to_owned(),
            ],
            compatibility_statement: "This adapter is an independent compatibility implementation for the CC0 Dungeonpunk core rules. It does not bundle the official document, branding, artwork, layout, community wiki, or supplements and is not endorsed by the original creators."
                .to_owned(),
        },
    }
}

fn creation_steps() -> Vec<CreationStep> {
    vec![
        CreationStep {
            id: "identity".to_owned(),
            title: "Character identity".to_owned(),
            description: "Enter an original fictional name without modifying canonical Character data."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![creation_field("name", "Name", string(1, 160), true)],
        },
        CreationStep {
            id: "attributes".to_owned(),
            title: "Allocate attributes".to_owned(),
            description: "Allocate exactly five points across six stats, with each starting value from zero through three; preview deterministic HP before accepting."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("attributes", "Attributes", named("Attributes"), true),
                creation_field("seed", "Creation seed", string(1, 20), true),
            ],
        },
        CreationStep {
            id: "moves".to_owned(),
            title: "Choose special moves".to_owned(),
            description: "Select core archetype entries or supply independently authored declarative moves; exactly three are required and no package code is loaded."
                .to_owned(),
            required_capability: TabletopCapability::EquipmentAndAbilities,
            fields: vec![creation_field(
                "special_moves",
                "Special moves",
                list(named("SpecialMove"), 3, 3),
                true,
            )],
        },
        CreationStep {
            id: "gear_and_bonds".to_owned(),
            title: "Gear, load, and bonds".to_owned(),
            description: "Record carried gear with explicit Weight and uses, inspect exact derived load, and add exactly two bonds."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field(
                    "gear",
                    "Starting gear",
                    map(named("GearDefinition"), 0, 64),
                    true,
                ),
                creation_field("bonds", "Bonds", list(string(1, 240), 2, 2), true),
            ],
        },
        CreationStep {
            id: "campaign".to_owned(),
            title: "Campaign clocks and threats".to_owned(),
            description: "Optionally initialize bounded clocks and typed threats as portable state rather than editor-only metadata."
                .to_owned(),
            required_capability: TabletopCapability::CampaignState,
            fields: vec![
                creation_field(
                    "clocks",
                    "Clocks",
                    map(named("ClockState"), 0, 128),
                    true,
                ),
                creation_field(
                    "threats",
                    "Threats",
                    map(named("ThreatState"), 0, 128),
                    true,
                ),
            ],
        },
    ]
}

fn operations() -> BTreeMap<String, ResolverOperationDeclaration> {
    BTreeMap::from([
        (
            "damage".to_owned(),
            operation(
                "damage",
                "Roll and apply damage",
                "Roll a standard-or-greater damage tier, optionally Brace with equipped armor, then record collapse or a pending FATE death check.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    (
                        "brace_item_id",
                        optional(string(1, 128), "Equipped armor or shield used to Brace."),
                    ),
                    (
                        "tier",
                        required(rolled_damage_tier_symbol(), "Rolled damage tier."),
                    ),
                ]),
                [
                    "damage_applied",
                    "death_resolved",
                    "entropy_trace",
                    "equipment_changed",
                    "resource_changed",
                ],
                true,
            ),
        ),
        (
            "death_check".to_owned(),
            operation(
                "death_check",
                "Resolve a FATE death check",
                "When HP is below zero, roll FATE and resolve unconsciousness, a survival-at-cost offer, or death.",
                TabletopCapability::ResourcesAndConditions,
                object_type([]),
                ["death_resolved", "entropy_trace", "resource_changed"],
                true,
            ),
        ),
        (
            "gm_move".to_owned(),
            operation(
                "gm_move",
                "Select a structured GM move",
                "Validate a general or threat-specific GM move and emit it as portable structured data.",
                TabletopCapability::CampaignState,
                object_type([
                    (
                        "category",
                        required(gm_category_symbol(), "GM move category."),
                    ),
                    ("move", required(gm_move_symbol(), "Closed GM move id.")),
                    (
                        "threat_id",
                        optional(string(1, 128), "Required only for threat-specific moves."),
                    ),
                ]),
                ["gm_move_selected"],
                false,
            ),
        ),
        (
            "grow".to_owned(),
            operation(
                "grow",
                "Spend XP on growth",
                "Spend the current move count to learn a move or twice the current stat sum to increase one stat.",
                TabletopCapability::Advancement,
                object_type([
                    (
                        "attribute",
                        optional(attribute_symbol(), "Stat selected for growth."),
                    ),
                    (
                        "kind",
                        required(symbol(["add_move", "increase_stat"]), "Growth option."),
                    ),
                    ("move_id", optional(string(1, 128), "New special-move id.")),
                    (
                        "move_label",
                        optional(string(1, 160), "New special-move label."),
                    ),
                ]),
                ["advancement_applied", "resource_changed"],
                false,
            ),
        ),
        (
            "harm_threat".to_owned(),
            operation(
                "harm_threat",
                "Roll harm against a threat",
                "Roll a standard-or-greater damage tier, subtract threat armor, and update its HP and defeated state.",
                TabletopCapability::Encounters,
                object_type([
                    ("threat_id", required(string(1, 128), "Threat id.")),
                    (
                        "tier",
                        required(rolled_damage_tier_symbol(), "Rolled damage tier."),
                    ),
                ]),
                ["entropy_trace", "threat_changed"],
                true,
            ),
        ),
        (
            "harm_threat_minor".to_owned(),
            operation(
                "harm_threat_minor",
                "Apply minor harm to a threat",
                "Apply one point before armor without consuming entropy.",
                TabletopCapability::Encounters,
                object_type([("threat_id", required(string(1, 128), "Threat id."))]),
                ["threat_changed"],
                false,
            ),
        ),
        (
            "impairment".to_owned(),
            operation(
                "impairment",
                "Set or resolve an impairment",
                "Add or remove the one-die penalty attached to a specific stat.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    (
                        "attribute",
                        required(attribute_symbol(), "Affected attribute."),
                    ),
                    (
                        "impaired",
                        required(TypeExpression::Bool, "Whether the impairment is active."),
                    ),
                ]),
                ["impairment_changed"],
                false,
            ),
        ),
        (
            "minor_damage".to_owned(),
            operation(
                "minor_damage",
                "Apply minor damage",
                "Apply one point, optionally Brace, then record collapse or a pending FATE death check without consuming entropy.",
                TabletopCapability::ResourcesAndConditions,
                object_type([(
                    "brace_item_id",
                    optional(string(1, 128), "Equipped armor or shield used to Brace."),
                )]),
                [
                    "damage_applied",
                    "death_resolved",
                    "equipment_changed",
                    "resource_changed",
                ],
                false,
            ),
        ),
        (
            "rest".to_owned(),
            operation(
                "rest",
                "Rest and recover",
                "Recover one Stress and one HP per complete two-hour interval and recompute carried load.",
                TabletopCapability::ResourcesAndConditions,
                object_type([("hours", required(integer(2.0, 336.0), "Rest duration."))]),
                ["resource_changed", "rest_resolved"],
                false,
            ),
        ),
        (
            "struggle".to_owned(),
            operation(
                "struggle",
                "Resolve Struggle",
                "Build a positive or fallback dice pool from stat, edge, help, push, impairment, and encumbrance; select highest or lowest and emit structured choices.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    ("attribute", required(attribute_symbol(), "Rolled stat.")),
                    (
                        "edge",
                        required(edge_symbol(), "Fictional advantage state."),
                    ),
                    (
                        "helper_encumbered",
                        optional(
                            TypeExpression::Bool,
                            "Whether the named helper is encumbered.",
                        ),
                    ),
                    (
                        "helper_id",
                        optional(
                            string(1, 128),
                            "Ally contributing one die and gaining one Stress.",
                        ),
                    ),
                    (
                        "push",
                        required(TypeExpression::Bool, "Gain two Stress for one die."),
                    ),
                ]),
                [
                    "entropy_trace",
                    "gm_move_prompt",
                    "help_applied",
                    "resource_changed",
                    "roll_resolved",
                    "struggle_resolved",
                ],
                true,
            ),
        ),
        (
            "survival_decision".to_owned(),
            operation(
                "survival_decision",
                "Resolve a survival offer",
                "Accept survival at a required cost or reject the pending offer and die.",
                TabletopCapability::ResourcesAndConditions,
                object_type([(
                    "accept",
                    required(
                        TypeExpression::Bool,
                        "Whether the offered survival is accepted.",
                    ),
                )]),
                ["death_resolved", "resource_changed"],
                false,
            ),
        ),
        (
            "tick_clock".to_owned(),
            operation(
                "tick_clock",
                "Tick a campaign clock",
                "Move a clock up or down within its bounds and emit a separate event when its configured boundary is reached.",
                TabletopCapability::Clocks,
                object_type([
                    ("clock_id", required(string(1, 128), "Clock id.")),
                    (
                        "delta",
                        required(integer(-24.0, 24.0), "Signed segment change."),
                    ),
                ]),
                ["clock_changed", "clock_triggered"],
                false,
            ),
        ),
    ])
}

fn event_types() -> BTreeMap<String, TypeExpression> {
    BTreeMap::from([
        (
            "advancement_applied".to_owned(),
            object_type([
                (
                    "attribute",
                    optional(attribute_symbol(), "Increased attribute."),
                ),
                ("cost", required(integer(0.0, 1_000_000.0), "XP spent.")),
                (
                    "kind",
                    required(symbol(["add_move", "increase_stat"]), "Growth option."),
                ),
                (
                    "move_id",
                    optional(string(1, 128), "Learned move identifier."),
                ),
                (
                    "new_rating",
                    optional(integer(0.0, 15.0), "Effective stat after growth."),
                ),
            ]),
        ),
        (
            "clock_changed".to_owned(),
            object_type([
                (
                    "after",
                    required(integer(0.0, 24.0), "Filled segments after."),
                ),
                (
                    "before",
                    required(integer(0.0, 24.0), "Filled segments before."),
                ),
                ("clock_id", required(string(1, 128), "Clock id.")),
                ("delta", required(integer(-24.0, 24.0), "Requested change.")),
                ("segments", required(integer(1.0, 24.0), "Clock size.")),
            ]),
        ),
        (
            "clock_triggered".to_owned(),
            object_type([
                (
                    "boundary",
                    required(symbol(["empty", "full"]), "Reached boundary."),
                ),
                ("clock_id", required(string(1, 128), "Clock id.")),
            ]),
        ),
        (
            "damage_applied".to_owned(),
            object_type([
                ("applied", required(integer(0.0, 128.0), "HP removed.")),
                (
                    "dice",
                    required(list(integer(1.0, 6.0), 0, 8), "Rolled damage dice."),
                ),
                (
                    "hp_after",
                    required(integer(-128.0, 1_024.0), "HP after damage."),
                ),
                (
                    "hp_before",
                    required(integer(-128.0, 1_024.0), "HP before damage."),
                ),
                ("prevented", required(integer(0.0, 128.0), "Damage Braced.")),
                (
                    "raw_damage",
                    required(integer(1.0, 128.0), "Rolled or fixed harm."),
                ),
                (
                    "status",
                    required(damage_status_symbol(), "Resulting life state."),
                ),
                ("tier", required(damage_tier_symbol(), "Damage tier.")),
            ]),
        ),
        (
            "death_resolved".to_owned(),
            object_type([
                (
                    "cost_required",
                    required(TypeExpression::Bool, "Whether survival requires a cost."),
                ),
                ("fate_die", optional(integer(1.0, 6.0), "FATE die result.")),
                (
                    "status",
                    required(
                        symbol(["dead", "survival_offer", "survived_at_cost", "unconscious"]),
                        "Death-move result.",
                    ),
                ),
            ]),
        ),
        (
            "entropy_trace".to_owned(),
            object_type([
                (
                    "cursor_after",
                    required(integer(0.0, u32::MAX as f64), "Entropy cursor after."),
                ),
                (
                    "cursor_before",
                    required(integer(0.0, u32::MAX as f64), "Entropy cursor before."),
                ),
                (
                    "draws",
                    required(list(integer(1.0, 6.0), 1, 64), "Selected d6 results."),
                ),
                ("operation", required(string(1, 128), "Resolver operation.")),
            ]),
        ),
        (
            "equipment_changed".to_owned(),
            object_type([
                (
                    "damage_marks",
                    required(integer(0.0, 255.0), "Current damage marks."),
                ),
                (
                    "destroyed",
                    required(TypeExpression::Bool, "Whether marks exceeded Weight."),
                ),
                ("item_id", required(string(1, 128), "Affected item.")),
            ]),
        ),
        (
            "gm_move_prompt".to_owned(),
            object_type([
                (
                    "categories",
                    required(
                        list(general_gm_category_symbol(), 5, 5),
                        "General GM categories.",
                    ),
                ),
                (
                    "reason",
                    required(symbol(["failed_roll", "needs_direction"]), "Prompt reason."),
                ),
                ("threat_id", optional(string(1, 128), "Relevant threat id.")),
                (
                    "threat_kind",
                    optional(threat_kind_symbol(), "Relevant threat category."),
                ),
            ]),
        ),
        (
            "gm_move_selected".to_owned(),
            object_type([
                (
                    "category",
                    required(gm_category_symbol(), "GM move category."),
                ),
                (
                    "move",
                    required(gm_move_symbol(), "Selected structured move."),
                ),
                (
                    "threat_id",
                    optional(string(1, 128), "Relevant threat for a specific move."),
                ),
            ]),
        ),
        (
            "help_applied".to_owned(),
            object_type([
                ("helper_id", required(string(1, 128), "Helping ally.")),
                (
                    "stress_delta",
                    required(integer(1.0, 1.0), "Stress the helper must gain."),
                ),
            ]),
        ),
        (
            "impairment_changed".to_owned(),
            object_type([
                ("attribute", required(attribute_symbol(), "Affected stat.")),
                (
                    "impaired",
                    required(TypeExpression::Bool, "Whether impairment is active."),
                ),
            ]),
        ),
        (
            "resource_changed".to_owned(),
            object_type([
                ("dead", required(TypeExpression::Bool, "Final death state.")),
                (
                    "death_check_pending",
                    required(TypeExpression::Bool, "Whether a FATE roll is required."),
                ),
                (
                    "encumbered",
                    required(TypeExpression::Bool, "Current load penalty."),
                ),
                (
                    "hp_current",
                    required(integer(-128.0, 1_024.0), "Current HP."),
                ),
                ("stress", required(integer(0.0, 512.0), "Current Stress.")),
                (
                    "total_load_half_units",
                    required(integer(0.0, 2_048.0), "Current exact load."),
                ),
                (
                    "unconscious",
                    required(TypeExpression::Bool, "Current collapse state."),
                ),
                ("xp", required(integer(0.0, 1_000_000.0), "Current XP.")),
            ]),
        ),
        (
            "rest_resolved".to_owned(),
            object_type([
                ("hours", required(integer(2.0, 336.0), "Rest duration.")),
                ("hp_after", required(integer(0.0, 1_024.0), "Recovered HP.")),
                (
                    "hp_before",
                    required(integer(-128.0, 1_024.0), "HP before rest."),
                ),
                (
                    "stress_after",
                    required(integer(0.0, 512.0), "Stress after rest."),
                ),
                (
                    "stress_before",
                    required(integer(0.0, 512.0), "Stress before rest."),
                ),
            ]),
        ),
        (
            "roll_resolved".to_owned(),
            object_type([
                ("attribute", required(attribute_symbol(), "Rolled stat.")),
                (
                    "dice",
                    required(list(integer(1.0, 6.0), 1, 32), "Ordered d6 results."),
                ),
                (
                    "dice_pool",
                    required(integer(-16.0, 32.0), "Effective pool before fallback."),
                ),
                ("edge", required(edge_symbol(), "Fictional edge.")),
                (
                    "failure_xp_gained",
                    required(TypeExpression::Bool, "Failure XP marker."),
                ),
                (
                    "helped",
                    required(TypeExpression::Bool, "Help contribution marker."),
                ),
                ("outcome", required(outcome_symbol(), "Outcome band.")),
                (
                    "push",
                    required(TypeExpression::Bool, "Push contribution marker."),
                ),
                ("selected", required(integer(1.0, 6.0), "Selected die.")),
                (
                    "selection",
                    required(symbol(["highest", "lowest"]), "Pool selection direction."),
                ),
            ]),
        ),
        (
            "struggle_resolved".to_owned(),
            object_type([
                (
                    "choices",
                    required(
                        list(struggle_choice_symbol(), 1, 2),
                        "Structured outcome choices.",
                    ),
                ),
                (
                    "cost_required",
                    required(TypeExpression::Bool, "Whether a cost is mandatory."),
                ),
                ("failure", required(TypeExpression::Bool, "Failure marker.")),
                (
                    "outcome",
                    required(outcome_symbol(), "Rolled outcome band."),
                ),
                (
                    "partial",
                    required(TypeExpression::Bool, "Partial-result marker."),
                ),
            ]),
        ),
        (
            "threat_changed".to_owned(),
            object_type([
                (
                    "applied",
                    required(integer(0.0, 128.0), "Damage after armor."),
                ),
                ("armor", required(integer(0.0, 32.0), "Threat armor.")),
                (
                    "defeated",
                    required(TypeExpression::Bool, "Defeated marker."),
                ),
                (
                    "dice",
                    required(list(integer(1.0, 6.0), 0, 8), "Rolled damage dice."),
                ),
                (
                    "hp_after",
                    required(integer(0.0, 512.0), "Threat HP after."),
                ),
                (
                    "hp_before",
                    required(integer(0.0, 512.0), "Threat HP before."),
                ),
                ("raw_damage", required(integer(1.0, 128.0), "Raw harm.")),
                ("threat_id", required(string(1, 128), "Threat id.")),
                ("tier", required(damage_tier_symbol(), "Damage tier.")),
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

#[allow(clippy::too_many_arguments)]
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

fn attribute_symbol() -> TypeExpression {
    symbol([
        "charisma",
        "constitution",
        "dexterity",
        "intelligence",
        "strength",
        "wisdom",
    ])
}

fn archetype_symbol() -> TypeExpression {
    symbol(["custom", "fighter", "peasant", "rogue", "wizard"])
}

fn gear_kind_symbol() -> TypeExpression {
    symbol([
        "ammunition",
        "armor",
        "book",
        "instrument",
        "pack",
        "personal",
        "rations",
        "shield",
        "supplies",
        "tool",
        "weapon",
    ])
}

fn threat_kind_symbol() -> TypeExpression {
    symbol([
        "affliction",
        "cursed_place",
        "foe",
        "overlord",
        "swarm",
        "terror",
    ])
}

fn edge_symbol() -> TypeExpression {
    symbol(["advantage", "disadvantage", "neutral"])
}

fn outcome_symbol() -> TypeExpression {
    symbol(["failure", "success", "twist"])
}

fn rolled_damage_tier_symbol() -> TypeExpression {
    symbol(["catastrophic", "deadly", "serious", "standard"])
}

fn damage_tier_symbol() -> TypeExpression {
    symbol(["catastrophic", "deadly", "minor", "serious", "standard"])
}

fn damage_status_symbol() -> TypeExpression {
    symbol([
        "active",
        "dead",
        "death_check_pending",
        "survival_offer",
        "unconscious",
    ])
}

fn struggle_choice_symbol() -> TypeExpression {
    symbol([
        "failure_with_cost",
        "failure_without_extra_cost",
        "full_success_with_cost",
        "partial_success_with_cost",
        "partial_success_without_cost",
    ])
}

fn general_gm_category_symbol() -> TypeExpression {
    symbol(["escalate", "injure", "rob", "twist_outcome", "warn"])
}

fn gm_category_symbol() -> TypeExpression {
    symbol([
        "escalate",
        "injure",
        "rob",
        "threat_specific",
        "twist_outcome",
        "warn",
    ])
}

fn gm_move_symbol() -> TypeExpression {
    symbol([
        "apparent_reward",
        "claim_ground",
        "complicate_simple_action",
        "damage_armor",
        "disgorge",
        "drain_resource",
        "false_perception",
        "follow_fiction",
        "forceful_attack",
        "grow",
        "hurt_ally",
        "impair_stat",
        "inflict_horror",
        "introduce_problem",
        "kill",
        "leave_mark",
        "lure",
        "negotiate",
        "overwhelm",
        "partial_success",
        "raise_champion",
        "rearrange",
        "seize_leverage",
        "show_distant_trouble",
        "spread",
        "start_or_tick_clock",
        "success_at_cost",
        "sudden_appearance",
        "take_or_damage_asset",
        "worsen_situation",
    ])
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

fn object<const N: usize>(fields: [(&str, DomainValue); N]) -> DomainValue {
    DomainValue::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn integer_list<T>(values: impl IntoIterator<Item = T>) -> DomainValue
where
    T: Into<u64>,
{
    DomainValue::List(
        values
            .into_iter()
            .map(|value| DomainValue::Number(value.into() as f64))
            .collect(),
    )
}

fn symbol_list<'a>(values: impl IntoIterator<Item = &'a str>) -> DomainValue {
    DomainValue::List(
        values
            .into_iter()
            .map(|value| DomainValue::Symbol(value.to_owned()))
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
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a DomainValue, TabletopError> {
    fields
        .get(field)
        .ok_or_else(|| invalid(format!("{path}.{field}"), "required field is missing"))
}

fn object_integer(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<i32, TabletopError> {
    let value = object_field(path, fields, field)?;
    let DomainValue::Number(number) = value else {
        return Err(invalid(format!("{path}.{field}"), "expected an integer"));
    };
    if !number.is_finite()
        || number.fract() != 0.0
        || *number < f64::from(i32::MIN)
        || *number > f64::from(i32::MAX)
    {
        return Err(invalid(
            format!("{path}.{field}"),
            "expected a finite 32-bit integer",
        ));
    }
    Ok(*number as i32)
}

fn set_integer(fields: &mut BTreeMap<String, DomainValue>, field: &str, value: i32) {
    fields.insert(field.to_owned(), DomainValue::Number(f64::from(value)));
}

fn object_bool(
    path: &str,
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<bool, TabletopError> {
    match object_field(path, fields, field)? {
        DomainValue::Bool(value) => Ok(*value),
        _ => Err(invalid(format!("{path}.{field}"), "expected a boolean")),
    }
}

fn optional_bool(
    fields: &BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<Option<bool>, TabletopError> {
    match fields.get(field) {
        Some(DomainValue::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(invalid(field, "expected a boolean")),
        None => Ok(None),
    }
}

fn object_string<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, TabletopError> {
    match object_field(path, fields, field)? {
        DomainValue::String(value) => Ok(value),
        _ => Err(invalid(format!("{path}.{field}"), "expected a string")),
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
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a str, TabletopError> {
    match object_field(path, fields, field)? {
        DomainValue::Symbol(value) => Ok(value),
        _ => Err(invalid(format!("{path}.{field}"), "expected a symbol")),
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

fn object_list<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a Vec<DomainValue>, TabletopError> {
    match object_field(path, fields, field)? {
        DomainValue::List(values) => Ok(values),
        _ => Err(invalid(format!("{path}.{field}"), "expected a list")),
    }
}

fn object_list_mut<'a>(
    path: &str,
    fields: &'a mut BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a mut Vec<DomainValue>, TabletopError> {
    match fields.get_mut(field) {
        Some(DomainValue::List(values)) => Ok(values),
        Some(_) => Err(invalid(format!("{path}.{field}"), "expected a list")),
        None => Err(invalid(
            format!("{path}.{field}"),
            "required field is missing",
        )),
    }
}

fn object_map<'a>(
    path: &str,
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a BTreeMap<String, DomainValue>, TabletopError> {
    value_object(
        format!("{path}.{field}"),
        object_field(path, fields, field)?,
    )
}

fn object_map_mut<'a>(
    path: &str,
    fields: &'a mut BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a mut BTreeMap<String, DomainValue>, TabletopError> {
    match fields.get_mut(field) {
        Some(DomainValue::Object(values)) => Ok(values),
        Some(_) => Err(invalid(format!("{path}.{field}"), "expected an object")),
        None => Err(invalid(
            format!("{path}.{field}"),
            "required field is missing",
        )),
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
