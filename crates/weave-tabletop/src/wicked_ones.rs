//! Verified CC0 Wicked Ones adapter, declarative creation, and deterministic play.
//!
//! The adapter is independently specified as typed Rust behavior. It pins the canonical public
//! release URL, the v1.4 edition, and the CC0-1.0 licence, but it does not redistribute the
//! upstream rulebook, artwork, layout, or branding.
//!
//! [`SOURCE_RECORD`] is the checked-in text artifact hashed by the manifest's public-source
//! record. It is authored here, not downloaded; the recorded digest is the SHA-256 of these exact
//! bytes. War for the Overworld and Undead Awakening content remains owned by its author and is
//! denied by this adapter. No such material appears in this module or its fixtures.

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
pub const WICKED_ONES_ADAPTER_ID: &str = "org.weave.tabletop.wicked_ones";
/// First independently modeled adapter release for Wicked Ones v1.4.
pub const WICKED_ONES_ADAPTER_VERSION: &str = "1.0.0";
/// Versioned character creation request and preview format.
pub const WICKED_ONES_CREATION_FORMAT_VERSION: u32 = 1;
/// Canonical public release page for the reviewed edition.
pub const WICKED_ONES_SOURCE_URL: &str =
    "https://www.kickstarter.com/projects/banditcamp/wo-ua-hardcover";
/// Edition named by the public-source record.
pub const WICKED_ONES_SOURCE_EDITION: &str = "v1.4";
/// SPDX identifier of the reviewed core release.
pub const WICKED_ONES_LICENSE: &str = "CC0-1.0";
/// SHA-256 of the official plain-text CC0 1.0 legal code.
pub const WICKED_ONES_CC0_LEGAL_CODE_SHA256: &str =
    "a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499";

/// Checked-in text record of the public source. The manifest hashes these exact bytes.
pub const SOURCE_RECORD: &str = concat!(
    "Wicked Ones adapter source record.\n",
    "Source URL: https://www.kickstarter.com/projects/banditcamp/wo-ua-hardcover\n",
    "Edition: v1.4\n",
    "License: CC0-1.0\n",
    "Scope: Wicked Ones core game text; War for the Overworld and Undead Awakening content excluded.\n",
    "This record is the checked-in text artifact hashed by the manifest. It is not a downloaded upstream file.\n",
);

/// SHA-256 of [`SOURCE_RECORD`], recorded verbatim in the manifest provenance.
pub const WICKED_ONES_SOURCE_RECORD_SHA256: &str =
    "c3d2ca581eacb12afca452d75eae83e925d5be46662f08a04f383584fde276b0";

/// Exact notices shared by the manifest and its conformance source-policy row.
pub const WICKED_ONES_NOTICES: &[&str] = &[
    "War for the Overworld and Undead Awakening content remains owned by its author and is denied by this adapter; no such material appears in this module or its fixtures.",
    "The primary digest covers the checked-in SOURCE_RECORD.txt text record, not a downloaded rulebook. The record states the canonical URL, CC0-1.0 licence, and v1.4 edition.",
];

const MAX_ACTION_RATING: u8 = 3;
const STARTING_ACTION_POINTS: u8 = 6;
const MAX_ACTION_POOL: u32 = 8;
const MAX_TEAMWORK_DICE: u32 = 4;
const MAX_STRESS: u32 = 9;
const MAX_SHOCK: u32 = 4;
const MAX_DARK_HEARTS: u32 = 6;
const MAX_STARTING_RESOURCES: u32 = 999;
const MAX_GEAR_ITEMS: usize = 8;

/// The closed Calling catalog used by this adapter release.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WickedOnesCalling {
    Forgekeeper,
    Warbringer,
    Whisperer,
    Wildcaller,
}

impl WickedOnesCalling {
    /// Every Calling in stable declaration order.
    pub const ALL: [Self; 4] = [
        Self::Forgekeeper,
        Self::Warbringer,
        Self::Whisperer,
        Self::Wildcaller,
    ];

    const fn symbol(self) -> &'static str {
        match self {
            Self::Forgekeeper => "forgekeeper",
            Self::Warbringer => "warbringer",
            Self::Whisperer => "whisperer",
            Self::Wildcaller => "wildcaller",
        }
    }

    fn from_symbol(value: &str) -> Result<Self, TabletopError> {
        match value {
            "forgekeeper" => Ok(Self::Forgekeeper),
            "warbringer" => Ok(Self::Warbringer),
            "whisperer" => Ok(Self::Whisperer),
            "wildcaller" => Ok(Self::Wildcaller),
            _ => Err(invalid("calling", "unknown Wicked Ones Calling")),
        }
    }
}

/// One action category. Ratings are grouped by these three headings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WickedOnesActionGroup {
    Brains,
    Muscles,
    Guts,
}

/// The closed action vocabulary of this adapter release.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WickedOnesAction {
    Analyze,
    Invent,
    Perceive,
    Brawl,
    Demolish,
    Sneak,
    Command,
    Persuade,
    Endure,
}

impl WickedOnesAction {
    /// Every action in stable symbol order.
    pub const ALL: [Self; 9] = [
        Self::Analyze,
        Self::Invent,
        Self::Perceive,
        Self::Brawl,
        Self::Demolish,
        Self::Sneak,
        Self::Command,
        Self::Persuade,
        Self::Endure,
    ];

    const fn symbol(self) -> &'static str {
        match self {
            Self::Analyze => "analyze",
            Self::Invent => "invent",
            Self::Perceive => "perceive",
            Self::Brawl => "brawl",
            Self::Demolish => "demolish",
            Self::Sneak => "sneak",
            Self::Command => "command",
            Self::Persuade => "persuade",
            Self::Endure => "endure",
        }
    }

    /// The Brains, Muscles, or Guts group that owns this action.
    pub const fn group(self) -> WickedOnesActionGroup {
        match self {
            Self::Analyze | Self::Invent | Self::Perceive => WickedOnesActionGroup::Brains,
            Self::Brawl | Self::Demolish | Self::Sneak => WickedOnesActionGroup::Muscles,
            Self::Command | Self::Persuade | Self::Endure => WickedOnesActionGroup::Guts,
        }
    }

    fn from_symbol(value: &str) -> Result<Self, TabletopError> {
        match value {
            "analyze" => Ok(Self::Analyze),
            "invent" => Ok(Self::Invent),
            "perceive" => Ok(Self::Perceive),
            "brawl" => Ok(Self::Brawl),
            "demolish" => Ok(Self::Demolish),
            "sneak" => Ok(Self::Sneak),
            "command" => Ok(Self::Command),
            "persuade" => Ok(Self::Persuade),
            "endure" => Ok(Self::Endure),
            _ => Err(invalid("action", "unknown Wicked Ones action")),
        }
    }
}

/// Authored starting ratings for every action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesActionRatings {
    pub analyze: u8,
    pub invent: u8,
    pub perceive: u8,
    pub brawl: u8,
    pub demolish: u8,
    pub sneak: u8,
    pub command: u8,
    pub persuade: u8,
    pub endure: u8,
}

impl WickedOnesActionRatings {
    /// Read one rating by action.
    #[must_use]
    pub const fn value(&self, action: WickedOnesAction) -> u8 {
        match action {
            WickedOnesAction::Analyze => self.analyze,
            WickedOnesAction::Invent => self.invent,
            WickedOnesAction::Perceive => self.perceive,
            WickedOnesAction::Brawl => self.brawl,
            WickedOnesAction::Demolish => self.demolish,
            WickedOnesAction::Sneak => self.sneak,
            WickedOnesAction::Command => self.command,
            WickedOnesAction::Persuade => self.persuade,
            WickedOnesAction::Endure => self.endure,
        }
    }

    /// Sum every rating.
    #[must_use]
    pub fn total(&self) -> u8 {
        Self::all()
            .into_iter()
            .map(|action| self.value(action))
            .sum()
    }

    fn all() -> [WickedOnesAction; 9] {
        WickedOnesAction::ALL
    }
}

/// One gear line chosen during creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesGearItem {
    pub id: String,
    pub label: String,
    pub quantity: u8,
}

/// The character's vile friend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesVileFriend {
    pub name: String,
    pub detail: String,
}

/// Complete, versioned input to deterministic Wicked Ones character creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesCreationRequest {
    pub creation_format_version: u32,
    pub character_id: String,
    pub name: String,
    pub race: String,
    pub look: String,
    pub calling: WickedOnesCalling,
    pub actions: WickedOnesActionRatings,
    pub dark_impulse: String,
    pub revelry: String,
    pub gear: Vec<WickedOnesGearItem>,
    pub abilities: Vec<String>,
    pub vile_friend: WickedOnesVileFriend,
    pub starting_resources: u32,
    pub seed: u64,
}

/// Derived totals used to show ratings grouped by Brains, Muscles, and Guts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesActionGroupTotals {
    pub brains: u8,
    pub muscles: u8,
    pub guts: u8,
}

/// Explainable result of creation before projection and mutable-state export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesCreationPreview {
    pub creation_format_version: u32,
    /// SHA-256 of the creation request that produced this preview.
    ///
    /// The preview does not retain the originating request, so
    /// [`validate_wicked_ones_creation_preview`] can only check the digest format and binds the
    /// preview to the definition hash and typed definition instead. Callers that still hold the
    /// request should use [`validate_wicked_ones_creation_preview_with_request`], which recomputes
    /// this digest exactly.
    pub request_sha256: String,
    pub adapter: ResolvedAdapter,
    pub calling: WickedOnesCalling,
    pub action_group_totals: WickedOnesActionGroupTotals,
    pub ability_labels: Vec<String>,
    pub definition: AdapterCharacterDefinition,
    pub initial_state: TabletopState,
    pub explanations: Vec<String>,
}

/// Stable entity identity namespace used by every campaign-owned identifier.
pub const WICKED_ONES_NAMESPACE: &str = "wicked_ones";
/// Campaign cycle phases in their canonical symbol order.
pub const WICKED_ONES_PHASES: [&str; 8] = [
    "advancement",
    "blowback",
    "calamity",
    "complete",
    "downtime",
    "dungeon_events",
    "raiding",
    "recovery",
];

/// A dungeon room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesRoom {
    pub id: String,
    pub label: String,
    pub purpose: String,
}

impl WickedOnesRoom {
    fn to_value(&self) -> DomainValue {
        object([
            ("id", DomainValue::String(self.id.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("purpose", DomainValue::String(self.purpose.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            purpose: read_string(fields, &format!("{path}.purpose"), "purpose")?.to_owned(),
        })
    }
}

/// A dungeon structure placed in a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesStructure {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub room_id: String,
}

impl WickedOnesStructure {
    fn to_value(&self) -> DomainValue {
        object([
            ("id", DomainValue::String(self.id.clone())),
            ("kind", DomainValue::Symbol(self.kind.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("room_id", DomainValue::String(self.room_id.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            kind: read_symbol(fields, &format!("{path}.kind"), "kind")?.to_owned(),
            room_id: read_string(fields, &format!("{path}.room_id"), "room_id")?.to_owned(),
        })
    }
}

/// A creature resident in the dungeon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesCreature {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub room_id: String,
    pub pack_id: String,
}

impl WickedOnesCreature {
    fn to_value(&self) -> DomainValue {
        object([
            ("id", DomainValue::String(self.id.clone())),
            ("kind", DomainValue::Symbol(self.kind.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("pack_id", DomainValue::String(self.pack_id.clone())),
            ("room_id", DomainValue::String(self.room_id.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            kind: read_symbol(fields, &format!("{path}.kind"), "kind")?.to_owned(),
            room_id: read_string(fields, &format!("{path}.room_id"), "room_id")?.to_owned(),
            pack_id: read_string(fields, &format!("{path}.pack_id"), "pack_id")?.to_owned(),
        })
    }
}

/// A dungeon lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesLock {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub room_id: String,
}

impl WickedOnesLock {
    fn to_value(&self) -> DomainValue {
        object([
            ("id", DomainValue::String(self.id.clone())),
            ("kind", DomainValue::Symbol(self.kind.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("room_id", DomainValue::String(self.room_id.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            kind: read_symbol(fields, &format!("{path}.kind"), "kind")?.to_owned(),
            room_id: read_string(fields, &format!("{path}.room_id"), "room_id")?.to_owned(),
        })
    }
}

/// A dungeon trap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesTrap {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub room_id: String,
    pub armed: bool,
}

impl WickedOnesTrap {
    fn to_value(&self) -> DomainValue {
        object([
            ("armed", DomainValue::Bool(self.armed)),
            ("id", DomainValue::String(self.id.clone())),
            ("kind", DomainValue::Symbol(self.kind.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("room_id", DomainValue::String(self.room_id.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            kind: read_symbol(fields, &format!("{path}.kind"), "kind")?.to_owned(),
            room_id: read_string(fields, &format!("{path}.room_id"), "room_id")?.to_owned(),
            armed: read_bool(fields, &format!("{path}.armed"), "armed")?,
        })
    }
}

/// A dungeon trick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesTrick {
    pub id: String,
    pub label: String,
    pub room_id: String,
    pub effect: String,
}

impl WickedOnesTrick {
    fn to_value(&self) -> DomainValue {
        object([
            ("effect", DomainValue::String(self.effect.clone())),
            ("id", DomainValue::String(self.id.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("room_id", DomainValue::String(self.room_id.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            room_id: read_string(fields, &format!("{path}.room_id"), "room_id")?.to_owned(),
            effect: read_string(fields, &format!("{path}.effect"), "effect")?.to_owned(),
        })
    }
}

/// A dungeon imp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesImp {
    pub id: String,
    pub name: String,
    pub duty_id: String,
}

impl WickedOnesImp {
    fn to_value(&self) -> DomainValue {
        object([
            ("duty_id", DomainValue::String(self.duty_id.clone())),
            ("id", DomainValue::String(self.id.clone())),
            ("name", DomainValue::String(self.name.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            name: read_string(fields, &format!("{path}.name"), "name")?.to_owned(),
            duty_id: read_string(fields, &format!("{path}.duty_id"), "duty_id")?.to_owned(),
        })
    }
}

/// A minion pack with morale and count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesMinionPack {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub morale: u8,
    pub count: u8,
    pub room_id: String,
}

impl WickedOnesMinionPack {
    fn to_value(&self) -> DomainValue {
        object([
            ("count", DomainValue::Number(f64::from(self.count))),
            ("id", DomainValue::String(self.id.clone())),
            ("kind", DomainValue::Symbol(self.kind.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("morale", DomainValue::Number(f64::from(self.morale))),
            ("room_id", DomainValue::String(self.room_id.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            kind: read_symbol(fields, &format!("{path}.kind"), "kind")?.to_owned(),
            morale: read_u8(fields, &format!("{path}.morale"), "morale")?,
            count: read_u8(fields, &format!("{path}.count"), "count")?,
            room_id: read_string(fields, &format!("{path}.room_id"), "room_id")?.to_owned(),
        })
    }
}

/// Complete dungeon identity, layout, and holdings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesDungeon {
    pub id: String,
    pub world_ref: String,
    pub atmosphere: String,
    pub entrance: String,
    pub hoard: String,
    pub tier: u8,
    pub gold: u32,
    pub supply: u32,
    pub rooms: Vec<WickedOnesRoom>,
    pub structures: Vec<WickedOnesStructure>,
    pub creatures: Vec<WickedOnesCreature>,
    pub locks: Vec<WickedOnesLock>,
    pub traps: Vec<WickedOnesTrap>,
    pub tricks: Vec<WickedOnesTrick>,
    pub imps: Vec<WickedOnesImp>,
    pub minion_packs: Vec<WickedOnesMinionPack>,
}

impl WickedOnesDungeon {
    fn to_value(&self) -> DomainValue {
        object([
            ("atmosphere", DomainValue::String(self.atmosphere.clone())),
            (
                "creatures",
                domain_list(&self.creatures, WickedOnesCreature::to_value),
            ),
            ("entrance", DomainValue::String(self.entrance.clone())),
            ("gold", DomainValue::Number(f64::from(self.gold))),
            ("hoard", DomainValue::String(self.hoard.clone())),
            ("id", DomainValue::String(self.id.clone())),
            ("imps", domain_list(&self.imps, WickedOnesImp::to_value)),
            ("locks", domain_list(&self.locks, WickedOnesLock::to_value)),
            (
                "minion_packs",
                domain_list(&self.minion_packs, WickedOnesMinionPack::to_value),
            ),
            ("rooms", domain_list(&self.rooms, WickedOnesRoom::to_value)),
            (
                "structures",
                domain_list(&self.structures, WickedOnesStructure::to_value),
            ),
            ("supply", DomainValue::Number(f64::from(self.supply))),
            ("tier", DomainValue::Number(f64::from(self.tier))),
            ("traps", domain_list(&self.traps, WickedOnesTrap::to_value)),
            (
                "tricks",
                domain_list(&self.tricks, WickedOnesTrick::to_value),
            ),
            ("world_ref", DomainValue::String(self.world_ref.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            world_ref: read_string(fields, &format!("{path}.world_ref"), "world_ref")?.to_owned(),
            atmosphere: read_string(fields, &format!("{path}.atmosphere"), "atmosphere")?
                .to_owned(),
            entrance: read_string(fields, &format!("{path}.entrance"), "entrance")?.to_owned(),
            hoard: read_string(fields, &format!("{path}.hoard"), "hoard")?.to_owned(),
            tier: read_u8(fields, &format!("{path}.tier"), "tier")?,
            gold: read_u32(fields, &format!("{path}.gold"), "gold")?,
            supply: read_u32(fields, &format!("{path}.supply"), "supply")?,
            rooms: read_list(fields, &format!("{path}.rooms"), "rooms")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesRoom::from_value(&format!("{path}.rooms[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            structures: read_list(fields, &format!("{path}.structures"), "structures")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesStructure::from_value(&format!("{path}.structures[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            creatures: read_list(fields, &format!("{path}.creatures"), "creatures")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesCreature::from_value(&format!("{path}.creatures[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            locks: read_list(fields, &format!("{path}.locks"), "locks")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesLock::from_value(&format!("{path}.locks[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            traps: read_list(fields, &format!("{path}.traps"), "traps")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesTrap::from_value(&format!("{path}.traps[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            tricks: read_list(fields, &format!("{path}.tricks"), "tricks")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesTrick::from_value(&format!("{path}.tricks[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            imps: read_list(fields, &format!("{path}.imps"), "imps")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesImp::from_value(&format!("{path}.imps[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            minion_packs: read_list(fields, &format!("{path}.minion_packs"), "minion_packs")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesMinionPack::from_value(
                        &format!("{path}.minion_packs[{index}]"),
                        value,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

/// A faction clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesFactionClock {
    pub id: String,
    pub label: String,
    pub segments: u8,
    pub filled: u8,
}

impl WickedOnesFactionClock {
    fn to_value(&self) -> DomainValue {
        object([
            ("filled", DomainValue::Number(f64::from(self.filled))),
            ("id", DomainValue::String(self.id.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("segments", DomainValue::Number(f64::from(self.segments))),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            segments: read_u8(fields, &format!("{path}.segments"), "segments")?,
            filled: read_u8(fields, &format!("{path}.filled"), "filled")?,
        })
    }
}

/// A faction tracking clocks and tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesFaction {
    pub id: String,
    pub name: String,
    pub tier: u8,
    pub world_ref: String,
    pub clocks: Vec<WickedOnesFactionClock>,
}

impl WickedOnesFaction {
    fn to_value(&self) -> DomainValue {
        object([
            (
                "clocks",
                domain_list(&self.clocks, WickedOnesFactionClock::to_value),
            ),
            ("id", DomainValue::String(self.id.clone())),
            ("name", DomainValue::String(self.name.clone())),
            ("tier", DomainValue::Number(f64::from(self.tier))),
            ("world_ref", DomainValue::String(self.world_ref.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            name: read_string(fields, &format!("{path}.name"), "name")?.to_owned(),
            tier: read_u8(fields, &format!("{path}.tier"), "tier")?,
            world_ref: read_string(fields, &format!("{path}.world_ref"), "world_ref")?.to_owned(),
            clocks: read_list(fields, &format!("{path}.clocks"), "clocks")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesFactionClock::from_value(&format!("{path}.clocks[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

/// The master plan clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesMasterPlan {
    pub title: String,
    pub segments: u8,
    pub filled: u8,
}

impl WickedOnesMasterPlan {
    fn to_value(&self) -> DomainValue {
        object([
            ("filled", DomainValue::Number(f64::from(self.filled))),
            ("segments", DomainValue::Number(f64::from(self.segments))),
            ("title", DomainValue::String(self.title.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            title: read_string(fields, &format!("{path}.title"), "title")?.to_owned(),
            segments: read_u8(fields, &format!("{path}.segments"), "segments")?,
            filled: read_u8(fields, &format!("{path}.filled"), "filled")?,
        })
    }
}

/// A dungeon duty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesDuty {
    pub id: String,
    pub label: String,
    pub assignee_id: String,
}

impl WickedOnesDuty {
    fn to_value(&self) -> DomainValue {
        object([
            ("assignee_id", DomainValue::String(self.assignee_id.clone())),
            ("id", DomainValue::String(self.id.clone())),
            ("label", DomainValue::String(self.label.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            assignee_id: read_string(fields, &format!("{path}.assignee_id"), "assignee_id")?
                .to_owned(),
        })
    }
}

/// A dungeon project clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesProject {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub segments: u8,
    pub filled: u8,
    pub room_id: String,
}

impl WickedOnesProject {
    fn to_value(&self) -> DomainValue {
        object([
            ("filled", DomainValue::Number(f64::from(self.filled))),
            ("id", DomainValue::String(self.id.clone())),
            ("kind", DomainValue::Symbol(self.kind.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("room_id", DomainValue::String(self.room_id.clone())),
            ("segments", DomainValue::Number(f64::from(self.segments))),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            kind: read_symbol(fields, &format!("{path}.kind"), "kind")?.to_owned(),
            segments: read_u8(fields, &format!("{path}.segments"), "segments")?,
            filled: read_u8(fields, &format!("{path}.filled"), "filled")?,
            room_id: read_string(fields, &format!("{path}.room_id"), "room_id")?.to_owned(),
        })
    }
}

/// A raid target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesTarget {
    pub id: String,
    pub label: String,
    pub faction_id: String,
    pub tier: u8,
    pub description: String,
}

impl WickedOnesTarget {
    fn to_value(&self) -> DomainValue {
        object([
            ("description", DomainValue::String(self.description.clone())),
            ("faction_id", DomainValue::String(self.faction_id.clone())),
            ("id", DomainValue::String(self.id.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("tier", DomainValue::Number(f64::from(self.tier))),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            faction_id: read_string(fields, &format!("{path}.faction_id"), "faction_id")?
                .to_owned(),
            tier: read_u8(fields, &format!("{path}.tier"), "tier")?,
            description: read_string(fields, &format!("{path}.description"), "description")?
                .to_owned(),
        })
    }
}

/// A planned or resolved raid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesRaid {
    pub id: String,
    pub label: String,
    pub plan: String,
    pub goal: String,
    pub target_id: String,
    pub state: String,
    pub loot: u32,
    pub blowback: u8,
    pub position: String,
    pub effect: String,
    pub scene: String,
    pub advantages: Vec<String>,
    pub disadvantages: Vec<String>,
}

impl WickedOnesRaid {
    fn to_value(&self) -> DomainValue {
        object([
            ("advantages", string_list_value(&self.advantages)),
            ("blowback", DomainValue::Number(f64::from(self.blowback))),
            ("disadvantages", string_list_value(&self.disadvantages)),
            ("effect", DomainValue::Symbol(self.effect.clone())),
            ("goal", DomainValue::String(self.goal.clone())),
            ("id", DomainValue::String(self.id.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("loot", DomainValue::Number(f64::from(self.loot))),
            ("plan", DomainValue::String(self.plan.clone())),
            ("position", DomainValue::Symbol(self.position.clone())),
            ("scene", DomainValue::String(self.scene.clone())),
            ("state", DomainValue::Symbol(self.state.clone())),
            ("target_id", DomainValue::String(self.target_id.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            plan: read_string(fields, &format!("{path}.plan"), "plan")?.to_owned(),
            goal: read_string(fields, &format!("{path}.goal"), "goal")?.to_owned(),
            target_id: read_string(fields, &format!("{path}.target_id"), "target_id")?.to_owned(),
            state: read_symbol(fields, &format!("{path}.state"), "state")?.to_owned(),
            loot: read_u32(fields, &format!("{path}.loot"), "loot")?,
            blowback: read_u8(fields, &format!("{path}.blowback"), "blowback")?,
            position: read_symbol(fields, &format!("{path}.position"), "position")?.to_owned(),
            effect: read_symbol(fields, &format!("{path}.effect"), "effect")?.to_owned(),
            scene: read_string(fields, &format!("{path}.scene"), "scene")?.to_owned(),
            advantages: read_strings(fields, &format!("{path}.advantages"), "advantages")?,
            disadvantages: read_strings(fields, &format!("{path}.disadvantages"), "disadvantages")?,
        })
    }
}

/// A purchased or available dungeon upgrade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesUpgrade {
    pub id: String,
    pub label: String,
    pub tier: u8,
    pub purchased: bool,
}

impl WickedOnesUpgrade {
    fn to_value(&self) -> DomainValue {
        object([
            ("id", DomainValue::String(self.id.clone())),
            ("label", DomainValue::String(self.label.clone())),
            ("purchased", DomainValue::Bool(self.purchased)),
            ("tier", DomainValue::Number(f64::from(self.tier))),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            id: read_string(fields, &format!("{path}.id"), "id")?.to_owned(),
            label: read_string(fields, &format!("{path}.label"), "label")?.to_owned(),
            tier: read_u8(fields, &format!("{path}.tier"), "tier")?,
            purchased: read_bool(fields, &format!("{path}.purchased"), "purchased")?,
        })
    }
}

/// One recorded action advancement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesActionAdvancement {
    pub action: String,
    pub from: u8,
    pub to: u8,
}

impl WickedOnesActionAdvancement {
    fn to_value(&self) -> DomainValue {
        object([
            ("action", DomainValue::Symbol(self.action.clone())),
            ("from", DomainValue::Number(f64::from(self.from))),
            ("to", DomainValue::Number(f64::from(self.to))),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            action: read_symbol(fields, &format!("{path}.action"), "action")?.to_owned(),
            from: read_u8(fields, &format!("{path}.from"), "from")?,
            to: read_u8(fields, &format!("{path}.to"), "to")?,
        })
    }
}

/// Advancement points, action increases, and gained abilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesAdvancement {
    pub xp: u32,
    pub wicked: u32,
    pub reckless: u32,
    pub actions: Vec<WickedOnesActionAdvancement>,
    pub abilities: Vec<String>,
}

impl WickedOnesAdvancement {
    fn to_value(&self) -> DomainValue {
        object([
            ("abilities", string_list_value(&self.abilities)),
            (
                "actions",
                domain_list(&self.actions, WickedOnesActionAdvancement::to_value),
            ),
            ("reckless", DomainValue::Number(f64::from(self.reckless))),
            ("wicked", DomainValue::Number(f64::from(self.wicked))),
            ("xp", DomainValue::Number(f64::from(self.xp))),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            xp: read_u32(fields, &format!("{path}.xp"), "xp")?,
            wicked: read_u32(fields, &format!("{path}.wicked"), "wicked")?,
            reckless: read_u32(fields, &format!("{path}.reckless"), "reckless")?,
            actions: read_list(fields, &format!("{path}.actions"), "actions")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesActionAdvancement::from_value(
                        &format!("{path}.actions[{index}]"),
                        value,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            abilities: read_strings(fields, &format!("{path}.abilities"), "abilities")?,
        })
    }
}

/// Explicit campaign cycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesCycle {
    pub phase: String,
    pub paused: bool,
    pub history: Vec<String>,
    pub recovery_used: u32,
    pub downtime_actions: u32,
    pub lurking: Vec<String>,
    pub revelry: String,
    pub calamity: String,
    pub raid_reviewed: bool,
    pub last_raid_id: String,
}

impl WickedOnesCycle {
    fn to_value(&self) -> DomainValue {
        object([
            ("calamity", DomainValue::String(self.calamity.clone())),
            (
                "downtime_actions",
                DomainValue::Number(f64::from(self.downtime_actions)),
            ),
            ("history", string_list_value(&self.history)),
            (
                "last_raid_id",
                DomainValue::String(self.last_raid_id.clone()),
            ),
            ("lurking", string_list_value(&self.lurking)),
            ("paused", DomainValue::Bool(self.paused)),
            ("phase", DomainValue::Symbol(self.phase.clone())),
            ("raid_reviewed", DomainValue::Bool(self.raid_reviewed)),
            (
                "recovery_used",
                DomainValue::Number(f64::from(self.recovery_used)),
            ),
            ("revelry", DomainValue::String(self.revelry.clone())),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            phase: read_symbol(fields, &format!("{path}.phase"), "phase")?.to_owned(),
            paused: read_bool(fields, &format!("{path}.paused"), "paused")?,
            history: read_strings(fields, &format!("{path}.history"), "history")?,
            recovery_used: read_u32(fields, &format!("{path}.recovery_used"), "recovery_used")?,
            downtime_actions: read_u32(
                fields,
                &format!("{path}.downtime_actions"),
                "downtime_actions",
            )?,
            lurking: read_strings(fields, &format!("{path}.lurking"), "lurking")?,
            revelry: read_string(fields, &format!("{path}.revelry"), "revelry")?.to_owned(),
            calamity: read_string(fields, &format!("{path}.calamity"), "calamity")?.to_owned(),
            raid_reviewed: read_bool(fields, &format!("{path}.raid_reviewed"), "raid_reviewed")?,
            last_raid_id: read_string(fields, &format!("{path}.last_raid_id"), "last_raid_id")?
                .to_owned(),
        })
    }
}

/// Complete dungeon, faction, raid, and campaign state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesCampaign {
    pub namespace: String,
    pub dungeon: WickedOnesDungeon,
    pub factions: Vec<WickedOnesFaction>,
    pub master_plan: WickedOnesMasterPlan,
    pub duties: Vec<WickedOnesDuty>,
    pub projects: Vec<WickedOnesProject>,
    pub targets: Vec<WickedOnesTarget>,
    pub raids: Vec<WickedOnesRaid>,
    pub upgrades: Vec<WickedOnesUpgrade>,
    pub advancement: WickedOnesAdvancement,
    pub cycle: WickedOnesCycle,
    pub loot: u32,
}

impl WickedOnesCampaign {
    fn to_value(&self) -> DomainValue {
        object([
            ("advancement", self.advancement.to_value()),
            ("cycle", self.cycle.to_value()),
            (
                "duties",
                domain_list(&self.duties, WickedOnesDuty::to_value),
            ),
            ("dungeon", self.dungeon.to_value()),
            (
                "factions",
                domain_list(&self.factions, WickedOnesFaction::to_value),
            ),
            ("loot", DomainValue::Number(f64::from(self.loot))),
            ("master_plan", self.master_plan.to_value()),
            ("namespace", DomainValue::String(self.namespace.clone())),
            (
                "projects",
                domain_list(&self.projects, WickedOnesProject::to_value),
            ),
            ("raids", domain_list(&self.raids, WickedOnesRaid::to_value)),
            (
                "targets",
                domain_list(&self.targets, WickedOnesTarget::to_value),
            ),
            (
                "upgrades",
                domain_list(&self.upgrades, WickedOnesUpgrade::to_value),
            ),
        ])
    }

    fn from_value(path: &str, value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object(path, value)?;
        Ok(Self {
            namespace: read_string(fields, &format!("{path}.namespace"), "namespace")?.to_owned(),
            dungeon: WickedOnesDungeon::from_value(
                &format!("{path}.dungeon"),
                object_field(fields, "dungeon")?,
            )?,
            factions: read_list(fields, &format!("{path}.factions"), "factions")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesFaction::from_value(&format!("{path}.factions[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            master_plan: WickedOnesMasterPlan::from_value(
                &format!("{path}.master_plan"),
                object_field(fields, "master_plan")?,
            )?,
            duties: read_list(fields, &format!("{path}.duties"), "duties")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesDuty::from_value(&format!("{path}.duties[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            projects: read_list(fields, &format!("{path}.projects"), "projects")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesProject::from_value(&format!("{path}.projects[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            targets: read_list(fields, &format!("{path}.targets"), "targets")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesTarget::from_value(&format!("{path}.targets[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            raids: read_list(fields, &format!("{path}.raids"), "raids")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesRaid::from_value(&format!("{path}.raids[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            upgrades: read_list(fields, &format!("{path}.upgrades"), "upgrades")?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    WickedOnesUpgrade::from_value(&format!("{path}.upgrades[{index}]"), value)
                })
                .collect::<Result<Vec<_>, _>>()?,
            advancement: WickedOnesAdvancement::from_value(
                &format!("{path}.advancement"),
                object_field(fields, "advancement")?,
            )?,
            cycle: WickedOnesCycle::from_value(
                &format!("{path}.cycle"),
                object_field(fields, "cycle")?,
            )?,
            loot: read_u32(fields, &format!("{path}.loot"), "loot")?,
        })
    }

    /// Project this campaign to its portable typed value.
    #[must_use]
    pub fn to_domain_value(&self) -> DomainValue {
        self.to_value()
    }

    /// Parse a campaign from its portable typed value.
    pub fn from_domain_value(value: &DomainValue) -> Result<Self, TabletopError> {
        Self::from_value("campaign", value)
    }

    /// Sort every entity collection by stable identifier.
    pub fn normalize(&mut self) {
        self.dungeon
            .rooms
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.dungeon
            .structures
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.dungeon
            .creatures
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.dungeon
            .locks
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.dungeon
            .traps
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.dungeon
            .tricks
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.dungeon
            .imps
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.dungeon
            .minion_packs
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.factions.sort_by(|left, right| left.id.cmp(&right.id));
        for faction in &mut self.factions {
            faction.clocks.sort_by(|left, right| left.id.cmp(&right.id));
        }
        self.duties.sort_by(|left, right| left.id.cmp(&right.id));
        self.projects.sort_by(|left, right| left.id.cmp(&right.id));
        self.targets.sort_by(|left, right| left.id.cmp(&right.id));
        self.raids.sort_by(|left, right| left.id.cmp(&right.id));
        self.upgrades.sort_by(|left, right| left.id.cmp(&right.id));
        self.advancement
            .actions
            .sort_by(|left, right| left.action.cmp(&right.action));
    }

    /// Validate identifiers, bounds, and every cross-entity reference.
    pub fn validate(&self) -> Result<(), TabletopError> {
        if self.namespace != WICKED_ONES_NAMESPACE {
            return Err(invalid(
                "campaign.namespace",
                "unexpected adapter namespace",
            ));
        }
        self.dungeon.validate()?;
        for faction in &self.factions {
            faction.validate()?;
        }
        self.master_plan.validate()?;
        for duty in &self.duties {
            duty.validate()?;
        }
        for project in &self.projects {
            project.validate()?;
        }
        for target in &self.targets {
            target.validate()?;
        }
        for raid in &self.raids {
            raid.validate()?;
        }
        for upgrade in &self.upgrades {
            upgrade.validate()?;
        }
        self.advancement.validate()?;
        self.cycle.validate()?;
        if self.loot > 999_999 {
            return Err(invalid(
                "campaign.loot",
                "loot exceeds the reviewed maximum",
            ));
        }

        require_sorted_ids(
            "campaign.factions",
            &self
                .factions
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        require_sorted_ids(
            "campaign.duties",
            &self
                .duties
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        require_sorted_ids(
            "campaign.projects",
            &self
                .projects
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        require_sorted_ids(
            "campaign.targets",
            &self
                .targets
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        require_sorted_ids(
            "campaign.raids",
            &self
                .raids
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        require_sorted_ids(
            "campaign.upgrades",
            &self
                .upgrades
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;

        let rooms: BTreeSet<&str> = self
            .dungeon
            .rooms
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        let factions: BTreeSet<&str> = self.factions.iter().map(|item| item.id.as_str()).collect();
        let targets: BTreeSet<&str> = self.targets.iter().map(|item| item.id.as_str()).collect();
        let duties: BTreeSet<&str> = self.duties.iter().map(|item| item.id.as_str()).collect();
        let imps: BTreeSet<&str> = self
            .dungeon
            .imps
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        let packs: BTreeSet<&str> = self
            .dungeon
            .minion_packs
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        let raids: BTreeSet<&str> = self.raids.iter().map(|item| item.id.as_str()).collect();

        for (index, structure) in self.dungeon.structures.iter().enumerate() {
            validate_reference(
                &format!("campaign.dungeon.structures[{index}].room_id"),
                &structure.room_id,
                &rooms,
                true,
            )?;
        }
        for (index, creature) in self.dungeon.creatures.iter().enumerate() {
            validate_reference(
                &format!("campaign.dungeon.creatures[{index}].room_id"),
                &creature.room_id,
                &rooms,
                true,
            )?;
            validate_reference(
                &format!("campaign.dungeon.creatures[{index}].pack_id"),
                &creature.pack_id,
                &packs,
                true,
            )?;
        }
        for (index, lock) in self.dungeon.locks.iter().enumerate() {
            validate_reference(
                &format!("campaign.dungeon.locks[{index}].room_id"),
                &lock.room_id,
                &rooms,
                true,
            )?;
        }
        for (index, trap) in self.dungeon.traps.iter().enumerate() {
            validate_reference(
                &format!("campaign.dungeon.traps[{index}].room_id"),
                &trap.room_id,
                &rooms,
                true,
            )?;
        }
        for (index, trick) in self.dungeon.tricks.iter().enumerate() {
            validate_reference(
                &format!("campaign.dungeon.tricks[{index}].room_id"),
                &trick.room_id,
                &rooms,
                true,
            )?;
        }
        for (index, imp) in self.dungeon.imps.iter().enumerate() {
            validate_reference(
                &format!("campaign.dungeon.imps[{index}].duty_id"),
                &imp.duty_id,
                &duties,
                false,
            )?;
        }
        for (index, pack) in self.dungeon.minion_packs.iter().enumerate() {
            validate_reference(
                &format!("campaign.dungeon.minion_packs[{index}].room_id"),
                &pack.room_id,
                &rooms,
                true,
            )?;
        }
        for (index, duty) in self.duties.iter().enumerate() {
            if !duty.assignee_id.is_empty()
                && !imps.contains(duty.assignee_id.as_str())
                && !packs.contains(duty.assignee_id.as_str())
            {
                return Err(invalid(
                    format!("campaign.duties[{index}].assignee_id"),
                    "reference does not resolve to an imp or minion pack",
                ));
            }
        }
        for (index, project) in self.projects.iter().enumerate() {
            validate_reference(
                &format!("campaign.projects[{index}].room_id"),
                &project.room_id,
                &rooms,
                true,
            )?;
        }
        for (index, target) in self.targets.iter().enumerate() {
            validate_reference(
                &format!("campaign.targets[{index}].faction_id"),
                &target.faction_id,
                &factions,
                false,
            )?;
        }
        for (index, raid) in self.raids.iter().enumerate() {
            validate_reference(
                &format!("campaign.raids[{index}].target_id"),
                &raid.target_id,
                &targets,
                true,
            )?;
        }
        if !self.cycle.last_raid_id.is_empty() && !raids.contains(self.cycle.last_raid_id.as_str())
        {
            return Err(invalid(
                "campaign.cycle.last_raid_id",
                "reference does not resolve to a raid",
            ));
        }
        Ok(())
    }
}

impl WickedOnesDungeon {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_local_identifier("campaign.dungeon.id", &self.id)?;
        if !self.world_ref.is_empty() {
            validate_global_identifier("campaign.dungeon.world_ref", &self.world_ref)?;
        }
        validate_bounded_text("campaign.dungeon.atmosphere", &self.atmosphere, 1, 240)?;
        validate_bounded_text("campaign.dungeon.entrance", &self.entrance, 1, 240)?;
        validate_bounded_text("campaign.dungeon.hoard", &self.hoard, 1, 240)?;
        if !(1..=4).contains(&self.tier) {
            return Err(invalid(
                "campaign.dungeon.tier",
                "dungeon tier must be within 1..4",
            ));
        }
        if self.gold > 999_999 || self.supply > 999_999 {
            return Err(invalid(
                "campaign.dungeon.gold",
                "dungeon holdings exceed the reviewed maximum",
            ));
        }
        for (index, room) in self.rooms.iter().enumerate() {
            validate_local_identifier(format!("campaign.dungeon.rooms[{index}].id"), &room.id)?;
            validate_bounded_text(
                format!("campaign.dungeon.rooms[{index}].label"),
                &room.label,
                1,
                160,
            )?;
            validate_bounded_text(
                format!("campaign.dungeon.rooms[{index}].purpose"),
                &room.purpose,
                0,
                512,
            )?;
        }
        require_sorted_ids(
            "campaign.dungeon.rooms",
            &self
                .rooms
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        for (index, structure) in self.structures.iter().enumerate() {
            validate_local_identifier(
                format!("campaign.dungeon.structures[{index}].id"),
                &structure.id,
            )?;
            validate_bounded_text(
                format!("campaign.dungeon.structures[{index}].label"),
                &structure.label,
                1,
                160,
            )?;
            validate_symbol(
                format!("campaign.dungeon.structures[{index}].kind"),
                &structure.kind,
                &["barracks", "shrine", "trapworks", "vault", "workshop"],
            )?;
        }
        require_sorted_ids(
            "campaign.dungeon.structures",
            &self
                .structures
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        for (index, creature) in self.creatures.iter().enumerate() {
            validate_local_identifier(
                format!("campaign.dungeon.creatures[{index}].id"),
                &creature.id,
            )?;
            validate_bounded_text(
                format!("campaign.dungeon.creatures[{index}].label"),
                &creature.label,
                1,
                160,
            )?;
            validate_symbol(
                format!("campaign.dungeon.creatures[{index}].kind"),
                &creature.kind,
                &["beast", "construct", "swarm", "vermin"],
            )?;
        }
        require_sorted_ids(
            "campaign.dungeon.creatures",
            &self
                .creatures
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        for (index, lock) in self.locks.iter().enumerate() {
            validate_local_identifier(format!("campaign.dungeon.locks[{index}].id"), &lock.id)?;
            validate_bounded_text(
                format!("campaign.dungeon.locks[{index}].label"),
                &lock.label,
                1,
                160,
            )?;
            validate_symbol(
                format!("campaign.dungeon.locks[{index}].kind"),
                &lock.kind,
                &["lockbox", "seal", "ward"],
            )?;
        }
        require_sorted_ids(
            "campaign.dungeon.locks",
            &self
                .locks
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        for (index, trap) in self.traps.iter().enumerate() {
            validate_local_identifier(format!("campaign.dungeon.traps[{index}].id"), &trap.id)?;
            validate_bounded_text(
                format!("campaign.dungeon.traps[{index}].label"),
                &trap.label,
                1,
                160,
            )?;
            validate_symbol(
                format!("campaign.dungeon.traps[{index}].kind"),
                &trap.kind,
                &["collapse", "flame", "snare"],
            )?;
        }
        require_sorted_ids(
            "campaign.dungeon.traps",
            &self
                .traps
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        for (index, trick) in self.tricks.iter().enumerate() {
            validate_local_identifier(format!("campaign.dungeon.tricks[{index}].id"), &trick.id)?;
            validate_bounded_text(
                format!("campaign.dungeon.tricks[{index}].label"),
                &trick.label,
                1,
                160,
            )?;
            validate_bounded_text(
                format!("campaign.dungeon.tricks[{index}].effect"),
                &trick.effect,
                0,
                512,
            )?;
        }
        require_sorted_ids(
            "campaign.dungeon.tricks",
            &self
                .tricks
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        for (index, imp) in self.imps.iter().enumerate() {
            validate_local_identifier(format!("campaign.dungeon.imps[{index}].id"), &imp.id)?;
            validate_bounded_text(
                format!("campaign.dungeon.imps[{index}].name"),
                &imp.name,
                1,
                160,
            )?;
        }
        require_sorted_ids(
            "campaign.dungeon.imps",
            &self
                .imps
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        for (index, pack) in self.minion_packs.iter().enumerate() {
            validate_local_identifier(
                format!("campaign.dungeon.minion_packs[{index}].id"),
                &pack.id,
            )?;
            validate_bounded_text(
                format!("campaign.dungeon.minion_packs[{index}].label"),
                &pack.label,
                1,
                160,
            )?;
            validate_symbol(
                format!("campaign.dungeon.minion_packs[{index}].kind"),
                &pack.kind,
                &["brutes", "diggers", "flyers", "skulkers"],
            )?;
            if pack.morale > 3 {
                return Err(invalid(
                    format!("campaign.dungeon.minion_packs[{index}].morale"),
                    "morale must be within 0..3",
                ));
            }
        }
        require_sorted_ids(
            "campaign.dungeon.minion_packs",
            &self
                .minion_packs
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )?;
        Ok(())
    }
}

impl WickedOnesFaction {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_local_identifier("campaign.factions.id", &self.id)?;
        validate_bounded_text("campaign.factions.name", &self.name, 1, 160)?;
        if !(1..=4).contains(&self.tier) {
            return Err(invalid(
                "campaign.factions.tier",
                "faction tier must be within 1..4",
            ));
        }
        if !self.world_ref.is_empty() {
            validate_global_identifier("campaign.factions.world_ref", &self.world_ref)?;
        }
        for (index, clock) in self.clocks.iter().enumerate() {
            clock.validate(&format!("campaign.factions.clocks[{index}]"))?;
        }
        require_sorted_ids(
            "campaign.factions.clocks",
            &self
                .clocks
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        )
    }
}

impl WickedOnesFactionClock {
    fn validate(&self, path: &str) -> Result<(), TabletopError> {
        validate_local_identifier(format!("{path}.id"), &self.id)?;
        validate_bounded_text(format!("{path}.label"), &self.label, 1, 160)?;
        if !(1..=8).contains(&self.segments) || self.filled > self.segments {
            return Err(invalid(
                format!("{path}.filled"),
                "clock fill must be within its segment count",
            ));
        }
        Ok(())
    }
}

impl WickedOnesMasterPlan {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_bounded_text("campaign.master_plan.title", &self.title, 1, 160)?;
        if !(1..=8).contains(&self.segments) || self.filled > self.segments {
            return Err(invalid(
                "campaign.master_plan.filled",
                "master plan fill must be within its segment count",
            ));
        }
        Ok(())
    }
}

impl WickedOnesDuty {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_local_identifier("campaign.duties.id", &self.id)?;
        validate_bounded_text("campaign.duties.label", &self.label, 1, 160)?;
        Ok(())
    }
}

impl WickedOnesProject {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_local_identifier("campaign.projects.id", &self.id)?;
        validate_bounded_text("campaign.projects.label", &self.label, 1, 160)?;
        validate_symbol(
            "campaign.projects.kind",
            &self.kind,
            &["defense", "raid", "ritual", "upgrade"],
        )?;
        if !(1..=8).contains(&self.segments) || self.filled > self.segments {
            return Err(invalid(
                "campaign.projects.filled",
                "project fill must be within its segment count",
            ));
        }
        Ok(())
    }
}

impl WickedOnesTarget {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_local_identifier("campaign.targets.id", &self.id)?;
        validate_bounded_text("campaign.targets.label", &self.label, 1, 160)?;
        if !(1..=4).contains(&self.tier) {
            return Err(invalid(
                "campaign.targets.tier",
                "target tier must be within 1..4",
            ));
        }
        validate_bounded_text("campaign.targets.description", &self.description, 0, 512)?;
        Ok(())
    }
}

impl WickedOnesRaid {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_local_identifier("campaign.raids.id", &self.id)?;
        validate_bounded_text("campaign.raids.label", &self.label, 1, 160)?;
        validate_bounded_text("campaign.raids.plan", &self.plan, 0, 512)?;
        validate_bounded_text("campaign.raids.goal", &self.goal, 0, 512)?;
        validate_symbol(
            "campaign.raids.state",
            &self.state,
            &["engaged", "planned", "resolved"],
        )?;
        validate_symbol(
            "campaign.raids.position",
            &self.position,
            &["controlled", "desperate", "risky"],
        )?;
        validate_symbol(
            "campaign.raids.effect",
            &self.effect,
            &["great", "limited", "standard"],
        )?;
        if self.blowback > 3 {
            return Err(invalid(
                "campaign.raids.blowback",
                "blowback severity must be within 0..3",
            ));
        }
        if self.loot > 999_999 {
            return Err(invalid(
                "campaign.raids.loot",
                "raid loot exceeds the reviewed maximum",
            ));
        }
        if self.advantages.len() > 8 || self.disadvantages.len() > 8 {
            return Err(invalid(
                "campaign.raids.advantages",
                "raid modifiers are bounded to eight entries",
            ));
        }
        for (index, advantage) in self.advantages.iter().enumerate() {
            validate_bounded_text(
                format!("campaign.raids.advantages[{index}]"),
                advantage,
                1,
                160,
            )?;
        }
        for (index, disadvantage) in self.disadvantages.iter().enumerate() {
            validate_bounded_text(
                format!("campaign.raids.disadvantages[{index}]"),
                disadvantage,
                1,
                160,
            )?;
        }
        Ok(())
    }
}

impl WickedOnesUpgrade {
    fn validate(&self) -> Result<(), TabletopError> {
        validate_local_identifier("campaign.upgrades.id", &self.id)?;
        validate_bounded_text("campaign.upgrades.label", &self.label, 1, 160)?;
        if !(1..=4).contains(&self.tier) {
            return Err(invalid(
                "campaign.upgrades.tier",
                "upgrade tier must be within 1..4",
            ));
        }
        Ok(())
    }
}

impl WickedOnesAdvancement {
    fn validate(&self) -> Result<(), TabletopError> {
        if self.xp > 999_999 || self.wicked > 999 || self.reckless > 999 {
            return Err(invalid(
                "campaign.advancement",
                "advancement points exceed the reviewed maximum",
            ));
        }
        let mut actions = BTreeSet::new();
        for (index, advancement) in self.actions.iter().enumerate() {
            validate_action_symbol(
                format!("campaign.advancement.actions[{index}].action"),
                &advancement.action,
            )?;
            if advancement.from > MAX_ACTION_RATING
                || advancement.to > MAX_ACTION_RATING
                || advancement.to <= advancement.from
            {
                return Err(invalid(
                    format!("campaign.advancement.actions[{index}].to"),
                    "action advancement must increase a rating within the reviewed maximum",
                ));
            }
            if !actions.insert(advancement.action.as_str()) {
                return Err(invalid(
                    "campaign.advancement.actions",
                    "duplicate action advancement",
                ));
            }
        }
        let mut abilities = BTreeSet::new();
        for (index, ability) in self.abilities.iter().enumerate() {
            validate_local_identifier(format!("campaign.advancement.abilities[{index}]"), ability)?;
            if !abilities.insert(ability.as_str()) {
                return Err(invalid(
                    "campaign.advancement.abilities",
                    "duplicate advanced ability",
                ));
            }
        }
        Ok(())
    }
}

impl WickedOnesCycle {
    fn validate(&self) -> Result<(), TabletopError> {
        if !WICKED_ONES_PHASES.contains(&self.phase.as_str()) {
            return Err(invalid("campaign.cycle.phase", "unknown campaign phase"));
        }
        if self.history.len() > 128 || self.lurking.len() > 64 {
            return Err(invalid(
                "campaign.cycle.history",
                "campaign cycle log exceeds its reviewed bound",
            ));
        }
        for (index, entry) in self.history.iter().enumerate() {
            validate_bounded_text(format!("campaign.cycle.history[{index}]"), entry, 0, 512)?;
        }
        for (index, entry) in self.lurking.iter().enumerate() {
            validate_bounded_text(format!("campaign.cycle.lurking[{index}]"), entry, 0, 512)?;
        }
        validate_bounded_text("campaign.cycle.revelry", &self.revelry, 0, 512)?;
        validate_bounded_text("campaign.cycle.calamity", &self.calamity, 0, 512)?;
        Ok(())
    }
}

fn domain_list<T>(items: &[T], convert: fn(&T) -> DomainValue) -> DomainValue {
    DomainValue::List(items.iter().map(convert).collect())
}

fn string_list_value(values: &[String]) -> DomainValue {
    DomainValue::List(
        values
            .iter()
            .map(|value| DomainValue::String(value.clone()))
            .collect(),
    )
}

fn read_string<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<&'a str, TabletopError> {
    match fields.get(key) {
        Some(DomainValue::String(value)) => Ok(value),
        Some(_) => Err(invalid(path, "expected a string")),
        None => Err(invalid(path, "required field is missing")),
    }
}

fn read_symbol<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<&'a str, TabletopError> {
    match fields.get(key) {
        Some(DomainValue::Symbol(value)) => Ok(value),
        Some(_) => Err(invalid(path, "expected a symbol")),
        None => Err(invalid(path, "required field is missing")),
    }
}

fn read_bool(
    fields: &BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<bool, TabletopError> {
    match fields.get(key) {
        Some(DomainValue::Bool(value)) => Ok(*value),
        Some(_) => Err(invalid(path, "expected a boolean")),
        None => Err(invalid(path, "required field is missing")),
    }
}

fn read_u32(
    fields: &BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<u32, TabletopError> {
    match fields.get(key) {
        Some(DomainValue::Number(number))
            if number.is_finite()
                && number.fract() == 0.0
                && *number >= 0.0
                && *number <= f64::from(u32::MAX) =>
        {
            Ok(*number as u32)
        }
        Some(DomainValue::Number(_)) => {
            Err(invalid(path, "expected a non-negative 32-bit integer"))
        }
        Some(_) => Err(invalid(path, "expected an unsigned integer")),
        None => Err(invalid(path, "required field is missing")),
    }
}

fn read_u8(
    fields: &BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<u8, TabletopError> {
    u8::try_from(read_u32(fields, path, key)?)
        .map_err(|_| invalid(path, "expected an 8-bit unsigned integer"))
}

fn read_list<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<&'a Vec<DomainValue>, TabletopError> {
    match fields.get(key) {
        Some(DomainValue::List(values)) => Ok(values),
        Some(_) => Err(invalid(path, "expected a list")),
        None => Err(invalid(path, "required field is missing")),
    }
}

fn read_strings(
    fields: &BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<Vec<String>, TabletopError> {
    string_values(path, read_list(fields, path, key)?)
}

fn require_sorted_ids(path: &str, ids: &[&str]) -> Result<(), TabletopError> {
    if ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid(path, "entity identities must be unique and sorted"));
    }
    Ok(())
}

fn validate_reference(
    path: &str,
    value: &str,
    known: &BTreeSet<&str>,
    required: bool,
) -> Result<(), TabletopError> {
    if value.is_empty() {
        return if required {
            Err(invalid(path, "reference is required"))
        } else {
            Ok(())
        };
    }
    if !known.contains(value) {
        return Err(invalid(path, "reference does not resolve"));
    }
    Ok(())
}

fn validate_symbol(
    path: impl Into<String>,
    value: &str,
    allowed: &[&str],
) -> Result<(), TabletopError> {
    if !allowed.contains(&value) {
        return Err(invalid(path, "unknown closed-vocabulary value"));
    }
    Ok(())
}

fn validate_action_symbol(path: impl Into<String>, value: &str) -> Result<(), TabletopError> {
    WickedOnesAction::from_symbol(value)
        .map(|_| ())
        .map_err(|_| invalid(path, "unknown Wicked Ones action"))
}

/// Versioned golden-cycle artifact.
pub const WICKED_ONES_GOLDEN_FORMAT_VERSION: u32 = 1;
/// Exact public-source prefix accepted by the adapter allowlist.
pub const WICKED_ONES_ALLOWED_SOURCE_PREFIX: &str =
    "https://www.kickstarter.com/projects/banditcamp/";
/// Upstream properties this adapter must never represent or redistribute.
pub const WICKED_ONES_DENIED_PROPERTIES: [&str; 2] = ["War for the Overworld", "Undead Awakening"];

/// Text-equivalent editor projection of one campaign area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesEditorView {
    pub id: String,
    pub title: String,
    pub lines: Vec<String>,
}

/// One operation in a deterministic golden cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesGoldenStep {
    pub operation: String,
    pub capability: TabletopCapability,
    pub input: DomainValue,
}

/// Deterministic synthetic campaign cycle, portable as RON or JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WickedOnesGoldenCycle {
    pub format_version: u32,
    pub campaign: WickedOnesCampaign,
    pub steps: Vec<WickedOnesGoldenStep>,
}

/// Documentation of covered scope, exclusions, and compatibility naming.
pub const WICKED_ONES_DOCUMENTATION: &str = concat!(
    "Wicked Ones adapter scope. The adapter models character creation and the action loop, ",
    "dungeon, faction, raid, and campaign-cycle entities, and text-equivalent editor views. ",
    "It ships with synthetic fixtures only. It redistributes no upstream prose, sheet layout, ",
    "artwork, logo, or trade dress. The primary source record names the canonical public URL, ",
    "the v1.4 edition, and CC0-1.0. War for the Overworld and Undead Awakening content remains ",
    "owned by its author and is excluded from every fixture, reference, and document. ",
    "Compatibility naming is factual only and never implies affiliation, sponsorship, or ",
    "endorsement. The source allowlist accepts only the canonical Kickstarter project prefix. ",
    "Editor views are text-equivalent projections; host-specific UI wiring is outside this module. ",
    "The golden cycle covers dungeon creation, downtime, a project, calamity, a raid, loot, ",
    "blowback, recovery, a faction-clock change, and advancement, and is portable as RON or JSON."
);

/// True when a source URL is inside the reviewed official allowlist.
#[must_use]
pub fn wicked_ones_source_url_allowed(url: &str) -> bool {
    url.starts_with(WICKED_ONES_ALLOWED_SOURCE_PREFIX)
}

/// True when an artifact name is denied by the upstream exclusion boundary.
#[must_use]
pub fn wicked_ones_forbidden_artifact(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    WICKED_ONES_DENIED_PROPERTIES
        .iter()
        .any(|property| lower.contains(&property.to_ascii_lowercase()))
        || [
            ".pdf", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".indd", ".ttf", ".otf",
            ".woff", ".woff2",
        ]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

/// Text-equivalent editor views for dungeon, clocks, factions, projects, minions, raids,
/// recovery, and advancement. No upstream sheet layout or art is reproduced.
#[must_use]
pub fn wicked_ones_editor_views(campaign: &WickedOnesCampaign) -> Vec<WickedOnesEditorView> {
    let mut dungeon = vec![
        format!(
            "Dungeon {}: tier {}, atmosphere {}, entrance {}",
            campaign.dungeon.id,
            campaign.dungeon.tier,
            campaign.dungeon.atmosphere,
            campaign.dungeon.entrance
        ),
        format!(
            "Hoard: {}; gold {}; supply {}",
            campaign.dungeon.hoard, campaign.dungeon.gold, campaign.dungeon.supply
        ),
    ];
    for room in &campaign.dungeon.rooms {
        dungeon.push(format!(
            "Room {}: {} ({})",
            room.id, room.label, room.purpose
        ));
    }
    for structure in &campaign.dungeon.structures {
        dungeon.push(format!(
            "Structure {}: {} [{}] in {}",
            structure.id, structure.label, structure.kind, structure.room_id
        ));
    }
    for lock in &campaign.dungeon.locks {
        dungeon.push(format!(
            "Lock {}: {} [{}] in {}",
            lock.id, lock.label, lock.kind, lock.room_id
        ));
    }
    for trap in &campaign.dungeon.traps {
        dungeon.push(format!(
            "Trap {}: {} [{}] in {} ({})",
            trap.id,
            trap.label,
            trap.kind,
            trap.room_id,
            if trap.armed { "armed" } else { "unarmed" }
        ));
    }
    for trick in &campaign.dungeon.tricks {
        dungeon.push(format!(
            "Trick {}: {} in {} ({})",
            trick.id, trick.label, trick.room_id, trick.effect
        ));
    }
    for creature in &campaign.dungeon.creatures {
        dungeon.push(format!(
            "Creature {}: {} [{}] in {} with {}",
            creature.id, creature.label, creature.kind, creature.room_id, creature.pack_id
        ));
    }

    let mut clocks = vec![format!(
        "Master plan {}: {}/{}",
        campaign.master_plan.title, campaign.master_plan.filled, campaign.master_plan.segments
    )];
    for faction in &campaign.factions {
        for clock in &faction.clocks {
            clocks.push(format!(
                "Faction clock {}: {} {}/{}",
                clock.id, clock.label, clock.filled, clock.segments
            ));
        }
    }
    for project in &campaign.projects {
        clocks.push(format!(
            "Project clock {}: {} {}/{}",
            project.id, project.label, project.filled, project.segments
        ));
    }

    let mut factions = Vec::new();
    for faction in &campaign.factions {
        factions.push(format!(
            "Faction {}: {} tier {} clocks {}",
            faction.id,
            faction.name,
            faction.tier,
            faction.clocks.len()
        ));
    }

    let mut projects = Vec::new();
    for project in &campaign.projects {
        projects.push(format!(
            "Project {}: {} [{}] {}/{} in {}",
            project.id,
            project.label,
            project.kind,
            project.filled,
            project.segments,
            project.room_id
        ));
    }

    let mut minions = Vec::new();
    for imp in &campaign.dungeon.imps {
        minions.push(format!("Imp {}: {} duty {}", imp.id, imp.name, imp.duty_id));
    }
    for pack in &campaign.dungeon.minion_packs {
        minions.push(format!(
            "Pack {}: {} [{}] morale {} count {} in {}",
            pack.id, pack.label, pack.kind, pack.morale, pack.count, pack.room_id
        ));
    }
    for duty in &campaign.duties {
        minions.push(format!(
            "Duty {}: {} assigned to {}",
            duty.id, duty.label, duty.assignee_id
        ));
    }

    let mut raids = Vec::new();
    for target in &campaign.targets {
        raids.push(format!(
            "Target {}: {} tier {} faction {}",
            target.id, target.label, target.tier, target.faction_id
        ));
    }
    for raid in &campaign.raids {
        raids.push(format!(
            "Raid {}: {} [{}] plan {} goal {} target {} position {} effect {}",
            raid.id,
            raid.label,
            raid.state,
            raid.plan,
            raid.goal,
            raid.target_id,
            raid.position,
            raid.effect
        ));
    }
    for upgrade in &campaign.upgrades {
        raids.push(format!(
            "Upgrade {}: {} tier {} purchased {}",
            upgrade.id, upgrade.label, upgrade.tier, upgrade.purchased
        ));
    }

    let recovery = vec![
        format!(
            "Cycle phase {} paused {} recovery uses {} downtime actions {}",
            campaign.cycle.phase,
            campaign.cycle.paused,
            campaign.cycle.recovery_used,
            campaign.cycle.downtime_actions
        ),
        format!(
            "Revelry {}; calamity {}; last raid {}",
            campaign.cycle.revelry, campaign.cycle.calamity, campaign.cycle.last_raid_id
        ),
        format!("Lurking: {}", campaign.cycle.lurking.join("; ")),
    ];

    let mut advancement = vec![format!(
        "Advancement xp {} wicked {} reckless {}",
        campaign.advancement.xp, campaign.advancement.wicked, campaign.advancement.reckless
    )];
    for action in &campaign.advancement.actions {
        advancement.push(format!(
            "Action advancement {}: {} to {}",
            action.action, action.from, action.to
        ));
    }
    for ability in &campaign.advancement.abilities {
        advancement.push(format!("Ability {ability}"));
    }

    vec![
        WickedOnesEditorView {
            id: "dungeon_layout".to_owned(),
            title: "Dungeon layout".to_owned(),
            lines: dungeon,
        },
        WickedOnesEditorView {
            id: "clocks".to_owned(),
            title: "Clocks".to_owned(),
            lines: clocks,
        },
        WickedOnesEditorView {
            id: "factions".to_owned(),
            title: "Factions".to_owned(),
            lines: factions,
        },
        WickedOnesEditorView {
            id: "projects".to_owned(),
            title: "Projects".to_owned(),
            lines: projects,
        },
        WickedOnesEditorView {
            id: "minions".to_owned(),
            title: "Minions".to_owned(),
            lines: minions,
        },
        WickedOnesEditorView {
            id: "raid_planning".to_owned(),
            title: "Raid planning".to_owned(),
            lines: raids,
        },
        WickedOnesEditorView {
            id: "recovery".to_owned(),
            title: "Recovery".to_owned(),
            lines: recovery,
        },
        WickedOnesEditorView {
            id: "advancement".to_owned(),
            title: "Advancement".to_owned(),
            lines: advancement,
        },
    ]
}

/// Synthetic campaign fixture used by the golden cycle and editor views.
#[must_use]
pub fn wicked_ones_golden_campaign() -> WickedOnesCampaign {
    let mut campaign = WickedOnesCampaign {
        namespace: WICKED_ONES_NAMESPACE.to_owned(),
        dungeon: WickedOnesDungeon {
            id: "ember_vault".to_owned(),
            world_ref: String::new(),
            atmosphere: "Sulphur, soot, and cold ash".to_owned(),
            entrance: "A dry well behind a tannery".to_owned(),
            hoard: "Ledgers and chipped coins".to_owned(),
            tier: 2,
            gold: 12,
            supply: 8,
            rooms: vec![
                WickedOnesRoom {
                    id: "audience_hall".to_owned(),
                    label: "Banner Hall".to_owned(),
                    purpose: "Receives visitors and boasts".to_owned(),
                },
                WickedOnesRoom {
                    id: "cistern".to_owned(),
                    label: "Cistern".to_owned(),
                    purpose: "Holds water and hidden routes".to_owned(),
                },
                WickedOnesRoom {
                    id: "forge_pit".to_owned(),
                    label: "Forge Pit".to_owned(),
                    purpose: "Shapes metal and tools".to_owned(),
                },
                WickedOnesRoom {
                    id: "vault_door".to_owned(),
                    label: "Vault Door".to_owned(),
                    purpose: "Guards the hoard".to_owned(),
                },
            ],
            structures: vec![
                WickedOnesStructure {
                    id: "banner_hall".to_owned(),
                    label: "Banner Hall".to_owned(),
                    kind: "barracks".to_owned(),
                    room_id: "audience_hall".to_owned(),
                },
                WickedOnesStructure {
                    id: "deep_vault".to_owned(),
                    label: "Deep Vault".to_owned(),
                    kind: "vault".to_owned(),
                    room_id: "vault_door".to_owned(),
                },
                WickedOnesStructure {
                    id: "smoke_forge".to_owned(),
                    label: "Smoke Forge".to_owned(),
                    kind: "workshop".to_owned(),
                    room_id: "forge_pit".to_owned(),
                },
                WickedOnesStructure {
                    id: "snare_gallery".to_owned(),
                    label: "Snare Gallery".to_owned(),
                    kind: "trapworks".to_owned(),
                    room_id: "cistern".to_owned(),
                },
            ],
            creatures: vec![
                WickedOnesCreature {
                    id: "cinder_hounds".to_owned(),
                    label: "Cinder Hounds".to_owned(),
                    kind: "beast".to_owned(),
                    room_id: "cistern".to_owned(),
                    pack_id: "cinder_pack".to_owned(),
                },
                WickedOnesCreature {
                    id: "gearwrights".to_owned(),
                    label: "Gearwrights".to_owned(),
                    kind: "construct".to_owned(),
                    room_id: "forge_pit".to_owned(),
                    pack_id: "gear_pack".to_owned(),
                },
            ],
            locks: vec![WickedOnesLock {
                id: "ledger_lock".to_owned(),
                label: "Ledger Lock".to_owned(),
                kind: "lockbox".to_owned(),
                room_id: "vault_door".to_owned(),
            }],
            traps: vec![WickedOnesTrap {
                id: "ash_snare".to_owned(),
                label: "Ash Snare".to_owned(),
                kind: "snare".to_owned(),
                room_id: "cistern".to_owned(),
                armed: true,
            }],
            tricks: vec![WickedOnesTrick {
                id: "false_ledger".to_owned(),
                label: "False Ledger".to_owned(),
                room_id: "audience_hall".to_owned(),
                effect: "Misleads a visitor about the hoard".to_owned(),
            }],
            imps: vec![WickedOnesImp {
                id: "pip".to_owned(),
                name: "Pip".to_owned(),
                duty_id: "watch_duty".to_owned(),
            }],
            minion_packs: vec![
                WickedOnesMinionPack {
                    id: "cinder_pack".to_owned(),
                    label: "Cinder Pack".to_owned(),
                    kind: "brutes".to_owned(),
                    morale: 2,
                    count: 6,
                    room_id: "cistern".to_owned(),
                },
                WickedOnesMinionPack {
                    id: "gear_pack".to_owned(),
                    label: "Gear Pack".to_owned(),
                    kind: "diggers".to_owned(),
                    morale: 3,
                    count: 4,
                    room_id: "forge_pit".to_owned(),
                },
            ],
        },
        factions: vec![
            WickedOnesFaction {
                id: "harbor_wardens".to_owned(),
                name: "Harbor Wardens".to_owned(),
                tier: 2,
                world_ref: String::new(),
                clocks: vec![WickedOnesFactionClock {
                    id: "patrols".to_owned(),
                    label: "Patrols".to_owned(),
                    segments: 4,
                    filled: 1,
                }],
            },
            WickedOnesFaction {
                id: "salt_guild".to_owned(),
                name: "Salt Guild".to_owned(),
                tier: 3,
                world_ref: String::new(),
                clocks: vec![WickedOnesFactionClock {
                    id: "ledger_audit".to_owned(),
                    label: "Ledger Audit".to_owned(),
                    segments: 6,
                    filled: 2,
                }],
            },
        ],
        master_plan: WickedOnesMasterPlan {
            title: "Open the drowned gate".to_owned(),
            segments: 6,
            filled: 2,
        },
        duties: vec![
            WickedOnesDuty {
                id: "forge_duty".to_owned(),
                label: "Mind the forge".to_owned(),
                assignee_id: "gear_pack".to_owned(),
            },
            WickedOnesDuty {
                id: "watch_duty".to_owned(),
                label: "Watch the cistern".to_owned(),
                assignee_id: "pip".to_owned(),
            },
        ],
        projects: vec![
            WickedOnesProject {
                id: "banner_works".to_owned(),
                label: "Banner Works".to_owned(),
                kind: "upgrade".to_owned(),
                segments: 3,
                filled: 0,
                room_id: "forge_pit".to_owned(),
            },
            WickedOnesProject {
                id: "gate_ritual".to_owned(),
                label: "Gate Ritual".to_owned(),
                kind: "ritual".to_owned(),
                segments: 4,
                filled: 1,
                room_id: "cistern".to_owned(),
            },
        ],
        targets: vec![
            WickedOnesTarget {
                id: "salt_warehouse".to_owned(),
                label: "Salt Warehouse".to_owned(),
                faction_id: "salt_guild".to_owned(),
                tier: 3,
                description: "A guarded store of salt and ledgers".to_owned(),
            },
            WickedOnesTarget {
                id: "warden_post".to_owned(),
                label: "Warden Post".to_owned(),
                faction_id: "harbor_wardens".to_owned(),
                tier: 2,
                description: "A small watch post above the harbor".to_owned(),
            },
        ],
        raids: Vec::new(),
        upgrades: vec![WickedOnesUpgrade {
            id: "reinforced_gate".to_owned(),
            label: "Reinforced Gate".to_owned(),
            tier: 2,
            purchased: false,
        }],
        advancement: WickedOnesAdvancement {
            xp: 0,
            wicked: 0,
            reckless: 0,
            actions: Vec::new(),
            abilities: Vec::new(),
        },
        cycle: WickedOnesCycle {
            phase: "recovery".to_owned(),
            paused: false,
            history: vec!["Campaign initialized".to_owned()],
            recovery_used: 0,
            downtime_actions: 0,
            lurking: Vec::new(),
            revelry: String::new(),
            calamity: String::new(),
            raid_reviewed: false,
            last_raid_id: String::new(),
        },
        loot: 0,
    };
    campaign.normalize();
    campaign
}

/// Deterministic golden cycle covering dungeon creation through advancement.
#[must_use]
pub fn wicked_ones_golden_cycle() -> WickedOnesGoldenCycle {
    let campaign = wicked_ones_golden_campaign();
    let campaign_input = object([("campaign", campaign.to_value())]);
    let steps = vec![
        golden_step(
            "set_campaign",
            TabletopCapability::CampaignState,
            campaign_input,
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                ("next_phase", DomainValue::Symbol("downtime".to_owned())),
                (
                    "note",
                    DomainValue::String("The vault settles into downtime".to_owned()),
                ),
            ]),
        ),
        golden_step(
            "perform_downtime_action",
            TabletopCapability::CampaignState,
            object([
                ("kind", DomainValue::Symbol("project".to_owned())),
                (
                    "note",
                    DomainValue::String("Carve the gate sigil".to_owned()),
                ),
                ("project_id", DomainValue::String("gate_ritual".to_owned())),
            ]),
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                (
                    "next_phase",
                    DomainValue::Symbol("dungeon_events".to_owned()),
                ),
                ("note", DomainValue::String("The dungeon stirs".to_owned())),
            ]),
        ),
        golden_step(
            "resolve_dungeon_event",
            TabletopCapability::CampaignState,
            object([
                (
                    "event",
                    DomainValue::String("A cistern pipe bursts".to_owned()),
                ),
                (
                    "note",
                    DomainValue::String("The lower rooms flood".to_owned()),
                ),
            ]),
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                ("next_phase", DomainValue::Symbol("calamity".to_owned())),
                (
                    "note",
                    DomainValue::String("Consequences arrive".to_owned()),
                ),
            ]),
        ),
        golden_step(
            "resolve_calamity",
            TabletopCapability::CampaignState,
            object([
                (
                    "note",
                    DomainValue::String("A patrol finds the well".to_owned()),
                ),
                ("severity", DomainValue::Number(1.0)),
            ]),
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                ("next_phase", DomainValue::Symbol("raiding".to_owned())),
                ("note", DomainValue::String("The raid begins".to_owned())),
            ]),
        ),
        golden_step(
            "plan_raid",
            TabletopCapability::CampaignState,
            object([
                (
                    "advantages",
                    DomainValue::List(vec![DomainValue::String(
                        "The salt ledgers are poorly watched".to_owned(),
                    )]),
                ),
                (
                    "disadvantages",
                    DomainValue::List(vec![DomainValue::String(
                        "The warehouse has armed patrols".to_owned(),
                    )]),
                ),
                (
                    "goal",
                    DomainValue::String("Take the salt ledgers".to_owned()),
                ),
                ("label", DomainValue::String("Salt Run".to_owned())),
                (
                    "plan",
                    DomainValue::String("Enter through the drain and leave by the roof".to_owned()),
                ),
                ("raid_id", DomainValue::String("salt_run".to_owned())),
                (
                    "target_id",
                    DomainValue::String("salt_warehouse".to_owned()),
                ),
            ]),
        ),
        golden_step(
            "engage_raid",
            TabletopCapability::CampaignState,
            object([("raid_id", DomainValue::String("salt_run".to_owned()))]),
        ),
        golden_step(
            "resolve_action",
            TabletopCapability::ChecksAndConflicts,
            object([
                ("action", DomainValue::Symbol("sneak".to_owned())),
                ("approach", DomainValue::Symbol("guile".to_owned())),
                ("bonus", DomainValue::Number(0.0)),
                ("effect", DomainValue::Symbol("standard".to_owned())),
                ("going_hard", DomainValue::Bool(false)),
                (
                    "intention",
                    DomainValue::String("Slip past the warehouse patrol".to_owned()),
                ),
                ("position", DomainValue::Symbol("desperate".to_owned())),
                ("teamwork", DomainValue::List(Vec::new())),
            ]),
        ),
        golden_step(
            "resolve_raid_outcome",
            TabletopCapability::CampaignState,
            object([
                (
                    "note",
                    DomainValue::String("The escape costs more than planned".to_owned()),
                ),
                ("outcome", DomainValue::Symbol("mixed".to_owned())),
                ("raid_id", DomainValue::String("salt_run".to_owned())),
            ]),
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                ("next_phase", DomainValue::Symbol("blowback".to_owned())),
                (
                    "note",
                    DomainValue::String("Blowback reaches the dungeon".to_owned()),
                ),
            ]),
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                ("next_phase", DomainValue::Symbol("recovery".to_owned())),
                (
                    "note",
                    DomainValue::String("The dungeon recovers".to_owned()),
                ),
            ]),
        ),
        golden_step(
            "perform_recovery",
            TabletopCapability::CampaignState,
            object([
                ("amount", DomainValue::Number(1.0)),
                ("note", DomainValue::String("Rest in the vault".to_owned())),
            ]),
        ),
        golden_step(
            "adjust_faction_clock",
            TabletopCapability::Factions,
            object([
                ("clock_id", DomainValue::String("ledger_audit".to_owned())),
                ("delta", DomainValue::Number(1.0)),
                ("faction_id", DomainValue::String("salt_guild".to_owned())),
                (
                    "note",
                    DomainValue::String("The guild audits its ledgers".to_owned()),
                ),
            ]),
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                ("next_phase", DomainValue::Symbol("advancement".to_owned())),
                ("note", DomainValue::String("The crew advances".to_owned())),
            ]),
        ),
        golden_step(
            "spend_reckless_advancement",
            TabletopCapability::Advancement,
            object([
                ("action", DomainValue::Symbol("sneak".to_owned())),
                (
                    "note",
                    DomainValue::String("The raid taught a harder lesson".to_owned()),
                ),
                ("to_rating", DomainValue::Number(3.0)),
            ]),
        ),
        golden_step(
            "advance_campaign_phase",
            TabletopCapability::CampaignState,
            object([
                ("next_phase", DomainValue::Symbol("complete".to_owned())),
                ("note", DomainValue::String("The cycle closes".to_owned())),
            ]),
        ),
    ];
    WickedOnesGoldenCycle {
        format_version: WICKED_ONES_GOLDEN_FORMAT_VERSION,
        campaign,
        steps,
    }
}

fn golden_step(
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> WickedOnesGoldenStep {
    WickedOnesGoldenStep {
        operation: operation.to_owned(),
        capability,
        input,
    }
}

macro_rules! impl_wicked_ones_io {
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

impl_wicked_ones_io!(WickedOnesCreationRequest);
impl_wicked_ones_io!(WickedOnesCreationPreview);
impl_wicked_ones_io!(WickedOnesGoldenCycle);

/// Canonical JSON Schema for Wicked Ones creation requests.
pub fn wicked_ones_creation_request_schema() -> Result<String, TabletopError> {
    wicked_ones_schema::<WickedOnesCreationRequest>(
        "urn:weave:schema:tabletop-wicked-ones-creation-request:1",
        "Weave Wicked Ones Creation Request v1",
    )
}

/// Canonical JSON Schema for Wicked Ones creation previews.
pub fn wicked_ones_creation_preview_schema() -> Result<String, TabletopError> {
    wicked_ones_schema::<WickedOnesCreationPreview>(
        "urn:weave:schema:tabletop-wicked-ones-creation-preview:1",
        "Weave Wicked Ones Creation Preview v1",
    )
}

fn wicked_ones_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, TabletopError> {
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
                serde_json::Value::from(WICKED_ONES_CREATION_FORMAT_VERSION),
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

const ABILITY_CATALOG: [(&str, WickedOnesCalling, &str); 8] = [
    (
        "clockwork_hands",
        WickedOnesCalling::Forgekeeper,
        "Clockwork Hands",
    ),
    (
        "steady_ledger",
        WickedOnesCalling::Forgekeeper,
        "Steady Ledger",
    ),
    ("battle_roar", WickedOnesCalling::Warbringer, "Battle Roar"),
    (
        "relentless_charge",
        WickedOnesCalling::Warbringer,
        "Relentless Charge",
    ),
    ("silent_step", WickedOnesCalling::Whisperer, "Silent Step"),
    (
        "silver_tongue",
        WickedOnesCalling::Whisperer,
        "Silver Tongue",
    ),
    ("beast_bond", WickedOnesCalling::Wildcaller, "Beast Bond"),
    (
        "rooted_stance",
        WickedOnesCalling::Wildcaller,
        "Rooted Stance",
    ),
];

fn calling_ability_label(calling: WickedOnesCalling, id: &str) -> Option<&'static str> {
    ABILITY_CATALOG
        .iter()
        .find(|(ability_id, ability_calling, _)| *ability_id == id && *ability_calling == calling)
        .map(|(_, _, label)| *label)
}

/// Validate and deterministically create one complete Wicked Ones character.
pub fn create_wicked_ones_character(
    request: &WickedOnesCreationRequest,
) -> Result<WickedOnesCreationPreview, TabletopError> {
    validate_creation_request(request)?;
    let manifest = wicked_ones_manifest();
    crate::validate_adapter_manifest(&manifest)?;
    let adapter = resolved_adapter(&manifest)?;
    let entropy = EntropyState {
        algorithm: "sha256_counter_v1".to_owned(),
        seed: request.seed,
        cursor: 0,
    };
    let action_group_totals = group_totals(&request.actions);
    let ability_labels = request
        .abilities
        .iter()
        .map(|id| {
            calling_ability_label(request.calling, id)
                .map(str::to_owned)
                .ok_or_else(|| {
                    invalid(
                        "abilities",
                        "selected ability is not offered by the chosen Calling",
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let definition_value = definition_value(request);
    validate_typed_value(
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
        completed_creation_steps: completed_creation_steps(),
    };
    let initial_state = TabletopState {
        state_format_version: ADAPTER_STATE_FORMAT_VERSION,
        adapter: adapter.clone(),
        owner_id: request.character_id.clone(),
        definition_sha256,
        revision: 0,
        entropy,
        value: initial_state_value(request.starting_resources),
    };
    validate_tabletop_state(&initial_state, &manifest)?;

    Ok(WickedOnesCreationPreview {
        creation_format_version: WICKED_ONES_CREATION_FORMAT_VERSION,
        request_sha256: canonical_fingerprint(request)?,
        adapter,
        calling: request.calling,
        action_group_totals,
        ability_labels,
        definition,
        initial_state,
        explanations: vec![
            "Action ratings are authored directly and grouped by Brains, Muscles, and Guts."
                .to_owned(),
            "Every roll consumes the retained deterministic entropy stream; play resumes at its saved cursor."
                .to_owned(),
            "Tabletop data remains adapter-owned and cannot write back into canonical Character evidence."
                .to_owned(),
        ],
    })
}

/// Revalidate a creation preview and its cross-artifact fingerprints.
pub fn validate_wicked_ones_creation_preview(
    preview: &WickedOnesCreationPreview,
) -> Result<(), TabletopError> {
    if preview.creation_format_version != WICKED_ONES_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_sha256("request_sha256", &preview.request_sha256)?;
    // The preview does not store the originating request, so this digest is format-checked only.
    // The definition hash below and the typed-definition revalidation bind the preview content.
    let manifest = wicked_ones_manifest();
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
    if preview.definition.completed_creation_steps != completed_creation_steps() {
        return Err(invalid(
            "completed_creation_steps",
            "creation preview is incomplete",
        ));
    }
    validate_typed_value(
        "definition",
        &preview.definition.definition,
        &manifest.definition_type,
        &manifest.types,
    )
    .map_err(|_| TabletopError::SchemaMismatch { path: "definition" })?;
    validate_tabletop_state(&preview.initial_state, &manifest)?;

    let definition_fields = value_object("definition", &preview.definition.definition)?;
    let calling = WickedOnesCalling::from_symbol(object_symbol(definition_fields, "calling")?)?;
    if calling != preview.calling {
        return Err(invalid(
            "calling",
            "preview Calling disagrees with the definition",
        ));
    }
    let actions = object_map(definition_fields, "actions")?;
    let ratings = ratings_from_map(actions)?;
    for action in WickedOnesAction::ALL {
        if ratings.value(action) > MAX_ACTION_RATING {
            return Err(invalid(
                format!("definition.actions.{}", action.symbol()),
                "stored action rating exceeds the reviewed maximum",
            ));
        }
    }
    if ratings.total() != STARTING_ACTION_POINTS {
        return Err(invalid(
            "definition.actions",
            "action ratings must allocate exactly six points",
        ));
    }
    let totals = group_totals(&ratings);
    if totals != preview.action_group_totals {
        return Err(invalid(
            "action_group_totals",
            "preview group totals disagree with the definition",
        ));
    }
    if u16::from(totals.brains) + u16::from(totals.muscles) + u16::from(totals.guts)
        != u16::from(STARTING_ACTION_POINTS)
    {
        return Err(invalid(
            "action_group_totals",
            "group totals do not sum to the starting action points",
        ));
    }
    let ability_ids = string_values(
        "definition.abilities",
        object_list(definition_fields, "abilities")?,
    )?;
    let expected_labels = ability_ids
        .iter()
        .map(|id| {
            calling_ability_label(calling, id)
                .map(str::to_owned)
                .ok_or_else(|| invalid("abilities", "ability is not offered by the Calling"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if expected_labels != preview.ability_labels {
        return Err(invalid(
            "ability_labels",
            "preview ability labels disagree with the definition",
        ));
    }
    let resources = object_u32(
        value_object("state", &preview.initial_state.value)?,
        "resources",
    )?;
    if resources != object_u32(definition_fields, "starting_resources")? {
        return Err(invalid(
            "resources",
            "initial resources disagree with the definition",
        ));
    }
    Ok(())
}

/// Revalidate a preview against the exact creation request that produced it.
///
/// This is the strong form: it recomputes `request_sha256` from the request and then applies the
/// complete preview validation. Use it whenever the caller retained the request.
pub fn validate_wicked_ones_creation_preview_with_request(
    preview: &WickedOnesCreationPreview,
    request: &WickedOnesCreationRequest,
) -> Result<(), TabletopError> {
    validate_wicked_ones_creation_preview(preview)?;
    if preview.request_sha256 != canonical_fingerprint(request)? {
        return Err(TabletopError::ContentHashMismatch {
            path: "request_sha256",
        });
    }
    Ok(())
}

fn completed_creation_steps() -> Vec<String> {
    vec![
        "abilities_and_friend".to_owned(),
        "actions".to_owned(),
        "core_identity".to_owned(),
        "gear_and_resources".to_owned(),
    ]
}

fn validate_creation_request(request: &WickedOnesCreationRequest) -> Result<(), TabletopError> {
    if request.creation_format_version != WICKED_ONES_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_global_identifier("character_id", &request.character_id)?;
    validate_bounded_text("name", &request.name, 1, 160)?;
    validate_bounded_text("race", &request.race, 1, 80)?;
    validate_bounded_text("look", &request.look, 1, 240)?;
    validate_bounded_text("dark_impulse", &request.dark_impulse, 1, 160)?;
    validate_bounded_text("revelry", &request.revelry, 1, 160)?;
    validate_bounded_text("vile_friend.name", &request.vile_friend.name, 1, 160)?;
    validate_bounded_text("vile_friend.detail", &request.vile_friend.detail, 0, 512)?;
    for action in WickedOnesAction::ALL {
        let rating = request.actions.value(action);
        if rating > MAX_ACTION_RATING {
            return Err(invalid(
                format!("actions.{}", action.symbol()),
                "starting action ratings cannot exceed three",
            ));
        }
    }
    if request.actions.total() != STARTING_ACTION_POINTS {
        return Err(invalid(
            "actions",
            "starting action ratings must allocate exactly six points",
        ));
    }
    if request.abilities.len() != 2 {
        return Err(invalid(
            "abilities",
            "exactly two starting abilities are required",
        ));
    }
    let mut ability_ids = BTreeSet::new();
    for id in &request.abilities {
        validate_local_identifier("abilities", id)?;
        if !ability_ids.insert(id.as_str()) {
            return Err(invalid("abilities", "duplicate ability identity"));
        }
        if calling_ability_label(request.calling, id).is_none() {
            return Err(invalid(
                "abilities",
                "selected ability is not offered by the chosen Calling",
            ));
        }
    }
    if request.gear.is_empty() || request.gear.len() > MAX_GEAR_ITEMS {
        return Err(invalid("gear", "expected one to eight gear items"));
    }
    let mut gear_ids = BTreeSet::new();
    for item in &request.gear {
        validate_local_identifier("gear.id", &item.id)?;
        validate_bounded_text("gear.label", &item.label, 1, 160)?;
        if item.quantity == 0 || item.quantity > 99 {
            return Err(invalid("gear.quantity", "quantity must be within 1..99"));
        }
        if !gear_ids.insert(item.id.as_str()) {
            return Err(invalid("gear", "duplicate gear identity"));
        }
    }
    if request.starting_resources > MAX_STARTING_RESOURCES {
        return Err(invalid(
            "starting_resources",
            "resources must be within 0..999",
        ));
    }
    Ok(())
}

fn ratings_from_map(
    actions: &BTreeMap<String, DomainValue>,
) -> Result<WickedOnesActionRatings, TabletopError> {
    Ok(WickedOnesActionRatings {
        analyze: object_u32(actions, "analyze")?
            .try_into()
            .map_err(|_| invalid("definition.actions.analyze", "rating is out of range"))?,
        invent: object_u32(actions, "invent")?
            .try_into()
            .map_err(|_| invalid("definition.actions.invent", "rating is out of range"))?,
        perceive: object_u32(actions, "perceive")?
            .try_into()
            .map_err(|_| invalid("definition.actions.perceive", "rating is out of range"))?,
        brawl: object_u32(actions, "brawl")?
            .try_into()
            .map_err(|_| invalid("definition.actions.brawl", "rating is out of range"))?,
        demolish: object_u32(actions, "demolish")?
            .try_into()
            .map_err(|_| invalid("definition.actions.demolish", "rating is out of range"))?,
        sneak: object_u32(actions, "sneak")?
            .try_into()
            .map_err(|_| invalid("definition.actions.sneak", "rating is out of range"))?,
        command: object_u32(actions, "command")?
            .try_into()
            .map_err(|_| invalid("definition.actions.command", "rating is out of range"))?,
        persuade: object_u32(actions, "persuade")?
            .try_into()
            .map_err(|_| invalid("definition.actions.persuade", "rating is out of range"))?,
        endure: object_u32(actions, "endure")?
            .try_into()
            .map_err(|_| invalid("definition.actions.endure", "rating is out of range"))?,
    })
}

fn group_totals(ratings: &WickedOnesActionRatings) -> WickedOnesActionGroupTotals {
    let mut brains = 0_u8;
    let mut muscles = 0_u8;
    let mut guts = 0_u8;
    for action in WickedOnesAction::ALL {
        match action.group() {
            WickedOnesActionGroup::Brains => brains = brains.saturating_add(ratings.value(action)),
            WickedOnesActionGroup::Muscles => {
                muscles = muscles.saturating_add(ratings.value(action));
            }
            WickedOnesActionGroup::Guts => guts = guts.saturating_add(ratings.value(action)),
        }
    }
    WickedOnesActionGroupTotals {
        brains,
        muscles,
        guts,
    }
}

fn definition_value(request: &WickedOnesCreationRequest) -> DomainValue {
    let actions = WickedOnesAction::ALL
        .into_iter()
        .map(|action| {
            (
                action.symbol().to_owned(),
                DomainValue::Number(f64::from(request.actions.value(action))),
            )
        })
        .collect();
    let abilities = request
        .abilities
        .iter()
        .map(|id| DomainValue::String(id.clone()))
        .collect();
    let gear = request
        .gear
        .iter()
        .map(|item| {
            object([
                ("id", DomainValue::String(item.id.clone())),
                ("label", DomainValue::String(item.label.clone())),
                ("quantity", DomainValue::Number(f64::from(item.quantity))),
            ])
        })
        .collect();
    object([
        ("abilities", DomainValue::List(abilities)),
        ("actions", DomainValue::Object(actions)),
        (
            "calling",
            DomainValue::Symbol(request.calling.symbol().to_owned()),
        ),
        (
            "dark_impulse",
            DomainValue::String(request.dark_impulse.clone()),
        ),
        ("gear", DomainValue::List(gear)),
        ("look", DomainValue::String(request.look.clone())),
        ("name", DomainValue::String(request.name.clone())),
        ("race", DomainValue::String(request.race.clone())),
        ("revelry", DomainValue::String(request.revelry.clone())),
        (
            "starting_resources",
            DomainValue::Number(f64::from(request.starting_resources)),
        ),
        (
            "vile_friend",
            object([
                (
                    "detail",
                    DomainValue::String(request.vile_friend.detail.clone()),
                ),
                (
                    "name",
                    DomainValue::String(request.vile_friend.name.clone()),
                ),
            ]),
        ),
    ])
}

/// Valid empty campaign scaffold present immediately after character creation.
#[must_use]
pub fn wicked_ones_default_campaign() -> WickedOnesCampaign {
    WickedOnesCampaign {
        namespace: WICKED_ONES_NAMESPACE.to_owned(),
        dungeon: WickedOnesDungeon {
            id: "unclaimed_vault".to_owned(),
            world_ref: String::new(),
            atmosphere: "Cold stone and old smoke".to_owned(),
            entrance: "A sealed cistern mouth".to_owned(),
            hoard: "An empty vault".to_owned(),
            tier: 1,
            gold: 0,
            supply: 0,
            rooms: Vec::new(),
            structures: Vec::new(),
            creatures: Vec::new(),
            locks: Vec::new(),
            traps: Vec::new(),
            tricks: Vec::new(),
            imps: Vec::new(),
            minion_packs: Vec::new(),
        },
        factions: Vec::new(),
        master_plan: WickedOnesMasterPlan {
            title: "The first plan".to_owned(),
            segments: 4,
            filled: 0,
        },
        duties: Vec::new(),
        projects: Vec::new(),
        targets: Vec::new(),
        raids: Vec::new(),
        upgrades: Vec::new(),
        advancement: WickedOnesAdvancement {
            xp: 0,
            wicked: 0,
            reckless: 0,
            actions: Vec::new(),
            abilities: Vec::new(),
        },
        cycle: WickedOnesCycle {
            phase: "recovery".to_owned(),
            paused: false,
            history: vec!["Campaign initialized".to_owned()],
            recovery_used: 0,
            downtime_actions: 0,
            lurking: Vec::new(),
            revelry: String::new(),
            calamity: String::new(),
            raid_reviewed: false,
            last_raid_id: String::new(),
        },
        loot: 0,
    }
}

fn initial_state_value(starting_resources: u32) -> DomainValue {
    object([
        ("bloodied", DomainValue::Bool(false)),
        ("campaign", wicked_ones_default_campaign().to_value()),
        ("condition", DomainValue::Symbol("steady".to_owned())),
        ("dark_hearts", DomainValue::Number(0.0)),
        ("dead", DomainValue::Bool(false)),
        ("death_knell", DomainValue::String(String::new())),
        ("feral", DomainValue::Bool(false)),
        ("flashbacks", DomainValue::Number(0.0)),
        ("last_outcome", DomainValue::Symbol("none".to_owned())),
        (
            "resources",
            DomainValue::Number(f64::from(starting_resources)),
        ),
        ("shock", DomainValue::Number(0.0)),
        ("stress", DomainValue::Number(0.0)),
    ])
}

/// Trusted resolver for the exact [`wicked_ones_manifest`] coordinate.
#[derive(Debug, Default, Clone, Copy)]
pub struct WickedOnesResolver;

impl TabletopResolver for WickedOnesResolver {
    fn adapter_id(&self) -> &str {
        WICKED_ONES_ADAPTER_ID
    }

    fn adapter_version(&self) -> &str {
        WICKED_ONES_ADAPTER_VERSION
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
            "advance_campaign_phase" => resolve_advance_campaign_phase(request, state)?,
            "advance_master_plan" => resolve_advance_master_plan(request, state)?,
            "adjust_faction_clock" => resolve_adjust_faction_clock(request, state)?,
            "apply_shock" => resolve_apply_shock(request, state)?,
            "death_knell" => resolve_death_knell(request, state)?,
            "engage_raid" => resolve_engage_raid(request, state)?,
            "flashback" => resolve_flashback(request, state)?,
            "gain_dark_heart" => resolve_gain_dark_heart(request, state)?,
            "go_feral" => resolve_go_feral(request, state)?,
            "pause_campaign" => resolve_pause_campaign(request, state)?,
            "perform_downtime_action" => resolve_perform_downtime_action(request, state)?,
            "perform_recovery" => resolve_perform_recovery(request, state)?,
            "plan_raid" => resolve_plan_raid(request, state)?,
            "purchase_upgrade" => resolve_purchase_upgrade(request, state)?,
            "resolve_action" => resolve_action(definition, request, state, entropy)?,
            "resolve_calamity" => resolve_calamity(request, state)?,
            "resolve_dungeon_event" => resolve_dungeon_event(request, state)?,
            "resolve_raid_outcome" => resolve_raid_outcome(request, state)?,
            "resume_campaign" => resolve_resume_campaign(request, state)?,
            "set_campaign" => resolve_set_campaign(request, state)?,
            "spend_reckless_advancement" => {
                resolve_spend_reckless_advancement(definition, request, state)?
            }
            "spend_wicked_advancement" => resolve_spend_wicked_advancement(request, state)?,
            _ => {
                return Err(invalid(
                    "operation",
                    "unsupported Wicked Ones resolver operation",
                ));
            }
        };
        Ok(ResolverOutput {
            state: next_state,
            events,
        })
    }
}

fn require_alive(state: &BTreeMap<String, DomainValue>) -> Result<(), TabletopError> {
    if object_bool(state, "dead")? {
        return Err(invalid("state.dead", "the character is dead"));
    }
    Ok(())
}

fn load_campaign(state: &DomainValue) -> Result<WickedOnesCampaign, TabletopError> {
    let fields = value_object("state", state)?;
    WickedOnesCampaign::from_value("state.campaign", object_field(fields, "campaign")?)
}

fn store_campaign(
    state: &mut DomainValue,
    campaign: &WickedOnesCampaign,
) -> Result<(), TabletopError> {
    let fields = value_object_mut("state", state)?;
    fields.insert("campaign".to_owned(), campaign.to_value());
    Ok(())
}

fn require_phase(campaign: &WickedOnesCampaign, expected: &str) -> Result<(), TabletopError> {
    if campaign.cycle.phase != expected {
        return Err(invalid(
            format!("campaign.cycle.phase.{}", campaign.cycle.phase),
            "operation is not allowed in the current phase",
        ));
    }
    Ok(())
}

fn require_unpaused(campaign: &WickedOnesCampaign) -> Result<(), TabletopError> {
    if campaign.cycle.paused {
        return Err(invalid(
            "campaign.cycle.paused",
            "campaign is paused; resume before advancing",
        ));
    }
    Ok(())
}

fn append_history(campaign: &mut WickedOnesCampaign, entry: String) {
    if campaign.cycle.history.len() >= 128 {
        campaign.cycle.history.remove(0);
    }
    campaign.cycle.history.push(entry);
}

fn allowed_phase_transition(current: &str, next: &str) -> bool {
    matches!(
        (current, next),
        ("recovery", "downtime")
            | ("recovery", "advancement")
            | ("downtime", "dungeon_events")
            | ("dungeon_events", "calamity")
            | ("calamity", "raiding")
            | ("raiding", "blowback")
            | ("blowback", "recovery")
            | ("blowback", "advancement")
            | ("advancement", "complete")
    )
}

fn effective_action_rating(
    definition: &DomainValue,
    campaign: &WickedOnesCampaign,
    action: &str,
) -> Result<u32, TabletopError> {
    let actions = object_map(value_object("definition", definition)?, "actions")?;
    let base = object_u32(actions, action)?;
    let advanced = campaign
        .advancement
        .actions
        .iter()
        .find(|advancement| advancement.action == action)
        .map_or(0, |advancement| u32::from(advancement.to));
    Ok(base.max(advanced))
}

fn resolve_set_campaign(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let mut campaign =
        WickedOnesCampaign::from_value("input.campaign", object_field(fields, "campaign")?)?;
    campaign.normalize();
    campaign.validate()?;
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        (
            "dungeon_id",
            DomainValue::String(campaign.dungeon.id.clone()),
        ),
        (
            "faction_count",
            DomainValue::Number(campaign.factions.len() as f64),
        ),
        ("phase", DomainValue::Symbol(campaign.cycle.phase.clone())),
        (
            "raid_count",
            DomainValue::Number(campaign.raids.len() as f64),
        ),
        (
            "room_count",
            DomainValue::Number(campaign.dungeon.rooms.len() as f64),
        ),
    ]);
    Ok((next, vec![public_event("campaign_initialized", payload)]))
}

fn resolve_advance_campaign_phase(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let next_phase = object_symbol(fields, "next_phase")?.to_owned();
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_unpaused(&campaign)?;
    let current = campaign.cycle.phase.clone();
    if !allowed_phase_transition(&current, &next_phase) {
        return Err(invalid(
            format!("campaign.cycle.phase.{current}"),
            "next phase is not reachable from the current phase",
        ));
    }
    campaign.cycle.phase = next_phase.clone();
    append_history(&mut campaign, format!("{current}->{next_phase}: {note}"));
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("from", DomainValue::Symbol(current)),
        ("note", DomainValue::String(note)),
        ("paused", DomainValue::Bool(campaign.cycle.paused)),
        ("to", DomainValue::Symbol(next_phase)),
    ]);
    Ok((next, vec![public_event("campaign_phase_changed", payload)]))
}

fn resolve_pause_campaign(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    if campaign.cycle.paused {
        return Err(invalid(
            "campaign.cycle.paused",
            "campaign is already paused",
        ));
    }
    campaign.cycle.paused = true;
    append_history(&mut campaign, format!("paused: {note}"));
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("note", DomainValue::String(note)),
        ("phase", DomainValue::Symbol(campaign.cycle.phase.clone())),
    ]);
    Ok((next, vec![public_event("campaign_paused", payload)]))
}

fn resolve_resume_campaign(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    if !campaign.cycle.paused {
        return Err(invalid("campaign.cycle.paused", "campaign is not paused"));
    }
    campaign.cycle.paused = false;
    append_history(&mut campaign, format!("resumed: {note}"));
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("note", DomainValue::String(note)),
        ("phase", DomainValue::Symbol(campaign.cycle.phase.clone())),
    ]);
    Ok((next, vec![public_event("campaign_resumed", payload)]))
}

fn resolve_perform_recovery(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let amount = object_u32(fields, "amount")?;
    if amount == 0 || amount > 3 {
        return Err(invalid(
            "input.amount",
            "recovery amount must be within 1..3",
        ));
    }
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "recovery")?;
    require_unpaused(&campaign)?;
    let state_fields = state_fields(state)?;
    let stress_before = object_u32(state_fields, "stress")?;
    let stress_after = stress_before.saturating_sub(amount).min(MAX_STRESS);
    campaign.cycle.recovery_used = campaign.cycle.recovery_used.saturating_add(1);
    append_history(&mut campaign, format!("recovery {amount}: {note}"));
    let mut next = state.clone();
    set_u32(
        value_object_mut("state", &mut next)?,
        "stress",
        stress_after,
    );
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("amount", DomainValue::Number(f64::from(amount))),
        ("note", DomainValue::String(note)),
        ("stress_after", DomainValue::Number(f64::from(stress_after))),
        (
            "stress_before",
            DomainValue::Number(f64::from(stress_before)),
        ),
    ]);
    Ok((next, vec![public_event("recovery_performed", payload)]))
}

fn resolve_perform_downtime_action(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let kind = object_symbol(fields, "kind")?.to_owned();
    let project_id = object_string(fields, "project_id")?.to_owned();
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "downtime")?;
    require_unpaused(&campaign)?;
    let state_fields = state_fields(state)?;
    let mut stress_after = object_u32(state_fields, "stress")?;
    let mut filled = 0_u8;
    let mut segments = 0_u8;
    match kind.as_str() {
        "project" => {
            validate_local_identifier("input.project_id", &project_id)?;
            let project = campaign
                .projects
                .iter_mut()
                .find(|project| project.id == project_id)
                .ok_or_else(|| invalid("input.project_id", "project does not exist"))?;
            project.filled = project.filled.saturating_add(1).min(project.segments);
            filled = project.filled;
            segments = project.segments;
        }
        "recovery" => {
            stress_after = stress_after.saturating_sub(1).min(MAX_STRESS);
        }
        "lurk" => {
            if campaign.cycle.lurking.len() >= 64 {
                campaign.cycle.lurking.remove(0);
            }
            campaign.cycle.lurking.push(note.clone());
        }
        "revel" => {
            campaign.cycle.revelry = note.clone();
        }
        _ => {
            return Err(invalid("input.kind", "unknown downtime action kind"));
        }
    }
    campaign.cycle.downtime_actions = campaign.cycle.downtime_actions.saturating_add(1);
    append_history(
        &mut campaign,
        format!("downtime {kind} {project_id}: {note}"),
    );
    let mut next = state.clone();
    set_u32(
        value_object_mut("state", &mut next)?,
        "stress",
        stress_after,
    );
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        (
            "downtime_actions",
            DomainValue::Number(f64::from(campaign.cycle.downtime_actions)),
        ),
        ("filled", DomainValue::Number(f64::from(filled))),
        ("kind", DomainValue::Symbol(kind)),
        ("note", DomainValue::String(note)),
        ("project_id", DomainValue::String(project_id)),
        ("segments", DomainValue::Number(f64::from(segments))),
        ("stress_after", DomainValue::Number(f64::from(stress_after))),
    ]);
    Ok((
        next,
        vec![public_event("downtime_action_resolved", payload)],
    ))
}

fn resolve_dungeon_event(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let event = object_string(fields, "event")?.to_owned();
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.event", &event, 1, 512)?;
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "dungeon_events")?;
    require_unpaused(&campaign)?;
    append_history(&mut campaign, format!("dungeon event {event}: {note}"));
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("event", DomainValue::String(event)),
        (
            "history_len",
            DomainValue::Number(campaign.cycle.history.len() as f64),
        ),
        ("note", DomainValue::String(note)),
    ]);
    Ok((next, vec![public_event("dungeon_event_resolved", payload)]))
}

fn resolve_calamity(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let severity = object_u32(fields, "severity")?;
    if severity == 0 || severity > 3 {
        return Err(invalid(
            "input.severity",
            "calamity severity must be within 1..3",
        ));
    }
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "calamity")?;
    require_unpaused(&campaign)?;
    campaign.cycle.calamity = note.clone();
    campaign.dungeon.supply = campaign.dungeon.supply.saturating_sub(severity);
    append_history(&mut campaign, format!("calamity {severity}: {note}"));
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("note", DomainValue::String(note)),
        ("severity", DomainValue::Number(f64::from(severity))),
        (
            "supply_after",
            DomainValue::Number(f64::from(campaign.dungeon.supply)),
        ),
    ]);
    Ok((next, vec![public_event("calamity_resolved", payload)]))
}

fn resolve_plan_raid(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let raid_id = object_string(fields, "raid_id")?.to_owned();
    let label = object_string(fields, "label")?.to_owned();
    let plan = object_string(fields, "plan")?.to_owned();
    let goal = object_string(fields, "goal")?.to_owned();
    let target_id = object_string(fields, "target_id")?.to_owned();
    let advantages = string_values("input.advantages", object_list(fields, "advantages")?)?;
    let disadvantages =
        string_values("input.disadvantages", object_list(fields, "disadvantages")?)?;
    validate_local_identifier("input.raid_id", &raid_id)?;
    validate_bounded_text("input.label", &label, 1, 160)?;
    if plan.is_empty() {
        return Err(invalid("input.plan", "a raid requires a declared plan"));
    }
    if goal.is_empty() {
        return Err(invalid("input.goal", "a raid requires a declared goal"));
    }
    validate_bounded_text("input.plan", &plan, 1, 512)?;
    validate_bounded_text("input.goal", &goal, 1, 512)?;
    for (index, advantage) in advantages.iter().enumerate() {
        validate_bounded_text(format!("input.advantages[{index}]"), advantage, 1, 160)?;
    }
    for (index, disadvantage) in disadvantages.iter().enumerate() {
        validate_bounded_text(
            format!("input.disadvantages[{index}]"),
            disadvantage,
            1,
            160,
        )?;
    }
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "raiding")?;
    require_unpaused(&campaign)?;
    if !campaign.targets.iter().any(|target| target.id == target_id) {
        return Err(invalid("input.target_id", "raid target does not exist"));
    }
    let advantage_count = advantages.len();
    let disadvantage_count = disadvantages.len();
    if let Some(raid) = campaign.raids.iter_mut().find(|raid| raid.id == raid_id) {
        if raid.state != "planned" {
            return Err(invalid(
                "input.raid_id",
                "raid is already engaged or resolved",
            ));
        }
        raid.label = label;
        raid.plan = plan.clone();
        raid.goal = goal.clone();
        raid.target_id = target_id.clone();
        raid.advantages = advantages;
        raid.disadvantages = disadvantages;
    } else {
        campaign.raids.push(WickedOnesRaid {
            id: raid_id.clone(),
            label,
            plan: plan.clone(),
            goal: goal.clone(),
            target_id: target_id.clone(),
            state: "planned".to_owned(),
            loot: 0,
            blowback: 0,
            position: "controlled".to_owned(),
            effect: "standard".to_owned(),
            scene: String::new(),
            advantages,
            disadvantages,
        });
    }
    campaign.normalize();
    campaign.cycle.raid_reviewed = true;
    campaign.cycle.last_raid_id = raid_id.clone();
    append_history(
        &mut campaign,
        format!("raid planned {raid_id}: {plan} / {goal}"),
    );
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        (
            "advantages_count",
            DomainValue::Number(advantage_count as f64),
        ),
        (
            "disadvantages_count",
            DomainValue::Number(disadvantage_count as f64),
        ),
        ("goal", DomainValue::String(goal)),
        ("plan", DomainValue::String(plan)),
        ("raid_id", DomainValue::String(raid_id)),
        ("target_id", DomainValue::String(target_id)),
    ]);
    Ok((next, vec![public_event("raid_planned", payload)]))
}

fn resolve_purchase_upgrade(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let upgrade_id = object_string(fields, "upgrade_id")?.to_owned();
    let note = object_string(fields, "note")?.to_owned();
    validate_local_identifier("input.upgrade_id", &upgrade_id)?;
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "downtime")?;
    require_unpaused(&campaign)?;
    let upgrade = campaign
        .upgrades
        .iter_mut()
        .find(|upgrade| upgrade.id == upgrade_id)
        .ok_or_else(|| invalid("input.upgrade_id", "upgrade does not exist"))?;
    if upgrade.purchased {
        return Err(invalid("input.upgrade_id", "upgrade is already purchased"));
    }
    let cost = u32::from(upgrade.tier).saturating_mul(4);
    if campaign.dungeon.gold < cost {
        return Err(invalid(
            "campaign.dungeon.gold",
            "not enough gold for this upgrade",
        ));
    }
    campaign.dungeon.gold -= cost;
    upgrade.purchased = true;
    append_history(&mut campaign, format!("upgrade {upgrade_id}: {note}"));
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("cost", DomainValue::Number(f64::from(cost))),
        (
            "gold_after",
            DomainValue::Number(f64::from(campaign.dungeon.gold)),
        ),
        ("note", DomainValue::String(note)),
        ("upgrade_id", DomainValue::String(upgrade_id)),
    ]);
    Ok((next, vec![public_event("upgrade_purchased", payload)]))
}

fn resolve_engage_raid(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let raid_id = object_string(fields, "raid_id")?.to_owned();
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "raiding")?;
    require_unpaused(&campaign)?;
    let raid_index = campaign
        .raids
        .iter()
        .position(|raid| raid.id == raid_id)
        .ok_or_else(|| invalid("input.raid_id", "raid does not exist"))?;
    let raid = &campaign.raids[raid_index];
    if raid.state != "planned" {
        return Err(invalid(
            "input.raid_id",
            "raid must be planned before engagement",
        ));
    }
    if raid.plan.is_empty() {
        return Err(invalid("input.raid_id", "a raid requires a declared plan"));
    }
    if raid.goal.is_empty() {
        return Err(invalid("input.raid_id", "a raid requires a declared goal"));
    }
    let target = campaign
        .targets
        .iter()
        .find(|target| target.id == raid.target_id)
        .ok_or_else(|| invalid("campaign.raids.target_id", "raid target does not exist"))?;
    let faction_tier = if target.faction_id.is_empty() {
        target.tier
    } else {
        campaign
            .factions
            .iter()
            .find(|faction| faction.id == target.faction_id)
            .map_or(target.tier, |faction| faction.tier)
    };
    let position = if faction_tier > campaign.dungeon.tier {
        "desperate"
    } else if faction_tier == campaign.dungeon.tier {
        "risky"
    } else {
        "controlled"
    };
    let effect = if raid.advantages.len() > raid.disadvantages.len() {
        "great"
    } else if raid.advantages.len() == raid.disadvantages.len() {
        "standard"
    } else {
        "limited"
    };
    let bonus = i64::try_from(raid.advantages.len())
        .unwrap_or(0)
        .saturating_sub(i64::try_from(raid.disadvantages.len()).unwrap_or(0))
        .clamp(0, 3);
    let scene = format!(
        "Raid {} on {}: {}. Goal: {}.",
        raid.label, target.label, raid.plan, raid.goal
    );
    let advantages = raid.advantages.clone();
    let disadvantages = raid.disadvantages.clone();
    let target_id = raid.target_id.clone();
    let raid = &mut campaign.raids[raid_index];
    raid.state = "engaged".to_owned();
    raid.position = position.to_owned();
    raid.effect = effect.to_owned();
    raid.scene = scene.clone();
    append_history(
        &mut campaign,
        format!("raid engaged {raid_id}: {position}/{effect}"),
    );
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("advantages", string_list_value(&advantages)),
        ("bonus", DomainValue::Number(bonus as f64)),
        ("disadvantages", string_list_value(&disadvantages)),
        (
            "dungeon_tier",
            DomainValue::Number(f64::from(campaign.dungeon.tier)),
        ),
        ("effect", DomainValue::Symbol(effect.to_owned())),
        ("faction_tier", DomainValue::Number(f64::from(faction_tier))),
        ("position", DomainValue::Symbol(position.to_owned())),
        ("raid_id", DomainValue::String(raid_id)),
        ("scene", DomainValue::String(scene)),
        ("target_id", DomainValue::String(target_id)),
    ]);
    Ok((next, vec![public_event("raid_engagement_ready", payload)]))
}

fn resolve_raid_outcome(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let raid_id = object_string(fields, "raid_id")?.to_owned();
    let outcome = object_symbol(fields, "outcome")?;
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    if !["critical", "failure", "mixed", "success"].contains(&outcome) {
        return Err(invalid("input.outcome", "unknown outcome band"));
    }
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "raiding")?;
    require_unpaused(&campaign)?;
    let raid_index = campaign
        .raids
        .iter()
        .position(|raid| raid.id == raid_id)
        .ok_or_else(|| invalid("input.raid_id", "raid does not exist"))?;
    if campaign.raids[raid_index].state != "engaged" {
        return Err(invalid(
            "input.raid_id",
            "raid must be engaged before its outcome resolves",
        ));
    }
    let target_tier = campaign
        .targets
        .iter()
        .find(|target| target.id == campaign.raids[raid_index].target_id)
        .map_or(1, |target| target.tier);
    let dungeon_tier = u32::from(campaign.dungeon.tier);
    let target_tier = u32::from(target_tier);
    let (loot, blowback, xp, wicked, reckless) = match outcome {
        "critical" => (dungeon_tier + target_tier + 2, 0_u8, 3, 1, 0),
        "success" => (dungeon_tier + target_tier, 0, 2, 1, 0),
        "mixed" => (target_tier.max(1), 1, 1, 0, 1),
        _ => (0, 2, 0, 0, 0),
    };
    campaign.loot = campaign.loot.saturating_add(loot).min(999_999);
    campaign.dungeon.gold = campaign.dungeon.gold.saturating_add(loot).min(999_999);
    campaign.advancement.xp = campaign.advancement.xp.saturating_add(xp).min(999_999);
    campaign.advancement.wicked = campaign.advancement.wicked.saturating_add(wicked).min(999);
    campaign.advancement.reckless = campaign
        .advancement
        .reckless
        .saturating_add(reckless)
        .min(999);
    let raid_label = campaign.raids[raid_index].label.clone();
    let raid = &mut campaign.raids[raid_index];
    raid.state = "resolved".to_owned();
    raid.loot = loot;
    raid.blowback = blowback;
    append_history(
        &mut campaign,
        format!("raid resolved {raid_id}: {outcome} loot {loot} blowback {blowback}"),
    );
    let (blowback_position, blowback_effect) = match blowback {
        0 => ("controlled", "great"),
        1 => ("risky", "standard"),
        _ => ("desperate", "limited"),
    };
    let blowback_scene = format!("Blowback from {raid_label}: {note}");
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let loot_payload = object([
        (
            "gold_after",
            DomainValue::Number(f64::from(campaign.dungeon.gold)),
        ),
        ("loot", DomainValue::Number(f64::from(loot))),
        ("loot_total", DomainValue::Number(f64::from(campaign.loot))),
        ("outcome", DomainValue::Symbol(outcome.to_owned())),
        ("raid_id", DomainValue::String(raid_id.clone())),
        (
            "reckless",
            DomainValue::Number(f64::from(campaign.advancement.reckless)),
        ),
        (
            "wicked",
            DomainValue::Number(f64::from(campaign.advancement.wicked)),
        ),
        (
            "xp",
            DomainValue::Number(f64::from(campaign.advancement.xp)),
        ),
    ]);
    let blowback_payload = object([
        ("effect", DomainValue::Symbol(blowback_effect.to_owned())),
        ("note", DomainValue::String(note)),
        (
            "position",
            DomainValue::Symbol(blowback_position.to_owned()),
        ),
        ("raid_id", DomainValue::String(raid_id)),
        ("scene", DomainValue::String(blowback_scene)),
        ("severity", DomainValue::Number(f64::from(blowback))),
    ]);
    Ok((
        next,
        vec![
            public_event("raid_loot_resolved", loot_payload),
            public_event("raid_blowback", blowback_payload),
        ],
    ))
}

fn resolve_adjust_faction_clock(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let faction_id = object_string(fields, "faction_id")?.to_owned();
    let clock_id = object_string(fields, "clock_id")?.to_owned();
    let delta = read_i32(fields, "input.delta", "delta")?;
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    if delta == 0 {
        return Err(invalid("input.delta", "clock change must be nonzero"));
    }
    let mut campaign = load_campaign(state)?;
    require_unpaused(&campaign)?;
    let faction = campaign
        .factions
        .iter_mut()
        .find(|faction| faction.id == faction_id)
        .ok_or_else(|| invalid("input.faction_id", "faction does not exist"))?;
    let clock = faction
        .clocks
        .iter_mut()
        .find(|clock| clock.id == clock_id)
        .ok_or_else(|| invalid("input.clock_id", "faction clock does not exist"))?;
    let from = clock.filled;
    let target = i32::from(from).saturating_add(delta);
    let to = u8::try_from(target.clamp(0, i32::from(clock.segments)))
        .map_err(|_| invalid("input.delta", "clock fill is out of range"))?;
    if to == from {
        return Err(invalid("input.delta", "clock is already at its bound"));
    }
    let segments = clock.segments;
    clock.filled = to;
    append_history(
        &mut campaign,
        format!("faction clock {faction_id}/{clock_id} {from}->{to}: {note}"),
    );
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("clock_id", DomainValue::String(clock_id)),
        ("faction_id", DomainValue::String(faction_id)),
        ("from", DomainValue::Number(f64::from(from))),
        ("note", DomainValue::String(note)),
        ("segments", DomainValue::Number(f64::from(segments))),
        ("to", DomainValue::Number(f64::from(to))),
    ]);
    Ok((next, vec![public_event("faction_clock_changed", payload)]))
}

fn resolve_advance_master_plan(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let delta = object_u32(fields, "delta")?;
    if delta == 0 || delta > 8 {
        return Err(invalid(
            "input.delta",
            "master plan advance must be within 1..8",
        ));
    }
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_unpaused(&campaign)?;
    let from = campaign.master_plan.filled;
    let segments = campaign.master_plan.segments;
    let to = from.saturating_add(delta as u8).min(segments);
    if to == from {
        return Err(invalid(
            "campaign.master_plan.filled",
            "master plan is already complete",
        ));
    }
    campaign.master_plan.filled = to;
    append_history(&mut campaign, format!("master plan {from}->{to}: {note}"));
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("from", DomainValue::Number(f64::from(from))),
        ("note", DomainValue::String(note)),
        ("segments", DomainValue::Number(f64::from(segments))),
        ("to", DomainValue::Number(f64::from(to))),
    ]);
    Ok((next, vec![public_event("master_plan_advanced", payload)]))
}

fn resolve_spend_wicked_advancement(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let ability = object_string(fields, "ability")?.to_owned();
    let note = object_string(fields, "note")?.to_owned();
    validate_local_identifier("input.ability", &ability)?;
    validate_bounded_text("input.note", &note, 1, 512)?;
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "advancement")?;
    require_unpaused(&campaign)?;
    if campaign.advancement.wicked == 0 {
        return Err(invalid(
            "campaign.advancement.wicked",
            "no Wicked advancement point is available",
        ));
    }
    if campaign.advancement.abilities.contains(&ability) {
        return Err(invalid("input.ability", "ability was already gained"));
    }
    campaign.advancement.wicked -= 1;
    campaign.advancement.abilities.push(ability.clone());
    campaign.advancement.abilities.sort();
    append_history(
        &mut campaign,
        format!("wicked advancement {ability}: {note}"),
    );
    let mut next = state.clone();
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("ability", DomainValue::String(ability)),
        ("note", DomainValue::String(note)),
        (
            "wicked_after",
            DomainValue::Number(f64::from(campaign.advancement.wicked)),
        ),
    ]);
    Ok((
        next,
        vec![public_event("wicked_advancement_spent", payload)],
    ))
}

fn resolve_spend_reckless_advancement(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let action = object_symbol(fields, "action")?.to_owned();
    let to_rating = object_u32(fields, "to_rating")?;
    let note = object_string(fields, "note")?.to_owned();
    validate_bounded_text("input.note", &note, 1, 512)?;
    if to_rating == 0 || to_rating > u32::from(MAX_ACTION_RATING) {
        return Err(invalid("input.to_rating", "new rating must be within 1..3"));
    }
    let mut campaign = load_campaign(state)?;
    require_phase(&campaign, "advancement")?;
    require_unpaused(&campaign)?;
    if campaign.advancement.reckless == 0 {
        return Err(invalid(
            "campaign.advancement.reckless",
            "no reckless advancement point is available",
        ));
    }
    let from = effective_action_rating(definition, &campaign, &action)?;
    if to_rating <= from {
        return Err(invalid(
            "input.to_rating",
            "new rating must exceed the current effective rating",
        ));
    }
    if let Some(advancement) = campaign
        .advancement
        .actions
        .iter_mut()
        .find(|advancement| advancement.action == action)
    {
        advancement.from =
            u8::try_from(from).map_err(|_| invalid("input.to_rating", "rating is out of range"))?;
        advancement.to = u8::try_from(to_rating)
            .map_err(|_| invalid("input.to_rating", "rating is out of range"))?;
    } else {
        campaign
            .advancement
            .actions
            .push(WickedOnesActionAdvancement {
                action: action.clone(),
                from: u8::try_from(from)
                    .map_err(|_| invalid("input.to_rating", "rating is out of range"))?,
                to: u8::try_from(to_rating)
                    .map_err(|_| invalid("input.to_rating", "rating is out of range"))?,
            });
    }
    campaign.advancement.reckless -= 1;
    campaign.normalize();
    append_history(
        &mut campaign,
        format!("reckless advancement {action} {from}->{to_rating}: {note}"),
    );
    let state_fields = state_fields(state)?;
    let stress_before = object_u32(state_fields, "stress")?;
    let stress_after = stress_before.saturating_add(1).min(MAX_STRESS);
    let mut next = state.clone();
    set_u32(
        value_object_mut("state", &mut next)?,
        "stress",
        stress_after,
    );
    store_campaign(&mut next, &campaign)?;
    let payload = object([
        ("action", DomainValue::Symbol(action)),
        ("from", DomainValue::Number(f64::from(from))),
        ("note", DomainValue::String(note)),
        (
            "reckless_after",
            DomainValue::Number(f64::from(campaign.advancement.reckless)),
        ),
        ("stress_after", DomainValue::Number(f64::from(stress_after))),
        ("to", DomainValue::Number(f64::from(to_rating))),
    ]);
    Ok((
        next,
        vec![public_event("reckless_advancement_spent", payload)],
    ))
}

fn read_i32(
    fields: &BTreeMap<String, DomainValue>,
    path: &str,
    key: &str,
) -> Result<i32, TabletopError> {
    match fields.get(key) {
        Some(DomainValue::Number(number))
            if number.is_finite()
                && number.fract() == 0.0
                && *number >= f64::from(i32::MIN)
                && *number <= f64::from(i32::MAX) =>
        {
            Ok(*number as i32)
        }
        Some(DomainValue::Number(_)) => Err(invalid(path, "expected a finite 32-bit integer")),
        Some(_) => Err(invalid(path, "expected an integer")),
        None => Err(invalid(path, "required field is missing")),
    }
}

fn resolve_action(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    require_alive(state_fields(state)?)?;
    let intention = object_string(fields, "intention")?.to_owned();
    validate_bounded_text("input.intention", &intention, 1, 512)?;
    let approach = object_symbol(fields, "approach")?;
    let approach_label = match approach {
        "force" | "guile" | "insight" => approach,
        _ => return Err(invalid("input.approach", "unknown approach")),
    };
    let action = WickedOnesAction::from_symbol(object_symbol(fields, "action")?)?;
    let position = object_symbol(fields, "position")?;
    let position = match position {
        "controlled" | "risky" | "desperate" => position,
        _ => return Err(invalid("input.position", "unknown position")),
    };
    let effect = object_symbol(fields, "effect")?;
    let effect = match effect {
        "limited" | "standard" | "great" => effect,
        _ => return Err(invalid("input.effect", "unknown effect")),
    };
    let bonus = object_u32(fields, "bonus")?;
    if bonus > 3 {
        return Err(invalid("input.bonus", "declared bonus cannot exceed three"));
    }
    let teamwork = teamwork_dice(fields)?;
    let going_hard = object_bool(fields, "going_hard")?;
    let resistance = resistance_choice(fields)?;

    let campaign = load_campaign(state)?;
    let rating = effective_action_rating(definition, &campaign, action.symbol())?;
    if rating > u32::from(MAX_ACTION_RATING) {
        return Err(invalid(
            "definition.actions",
            "stored action rating is outside the reviewed range",
        ));
    }
    let declared_pool = rating + bonus + teamwork + u32::from(going_hard);
    let pool = declared_pool.min(MAX_ACTION_POOL);
    let dice_count = if pool == 0 { 2 } else { pool };
    let dice = roll_dice(entropy, dice_count)?;
    let (best, sixes) = effective_best(&dice, pool == 0);
    let outcome = outcome_band(best, sixes);

    let mut stress_before = object_u32(state_fields(state)?, "stress")?;
    if stress_before > MAX_STRESS {
        return Err(invalid("state.stress", "stored Stress is outside range"));
    }
    let mut dark_hearts = object_u32(state_fields(state)?, "dark_hearts")?;
    if dark_hearts > MAX_DARK_HEARTS {
        return Err(invalid(
            "state.dark_hearts",
            "stored dark hearts are outside range",
        ));
    }

    let mut severity = consequence_severity(outcome, position, effect).to_owned();
    let kind = consequence_kind(approach_label);
    let mut stress_cost = action_stress(outcome, going_hard);
    let mut resistance_outcome = "none";
    let mut resistance_dice = Vec::new();
    let resisted = resistance.is_some() && severity != "none";
    if let Some((resistance_action, resistance_bonus)) = resistance
        && severity != "none"
    {
        let resistance_rating =
            effective_action_rating(definition, &campaign, resistance_action.symbol())?;
        if resistance_rating > u32::from(MAX_ACTION_RATING) {
            return Err(invalid(
                "definition.actions",
                "stored resistance action rating is outside the reviewed range",
            ));
        }
        let resistance_declared_pool = resistance_rating.saturating_add(resistance_bonus);
        let resistance_pool = resistance_declared_pool.min(MAX_ACTION_POOL);
        let resistance_dice_count = if resistance_pool == 0 {
            2
        } else {
            resistance_pool
        };
        resistance_dice = roll_dice(entropy, resistance_dice_count)?;
        let (resistance_best, resistance_sixes) =
            effective_best(&resistance_dice, resistance_pool == 0);
        resistance_outcome = outcome_band(resistance_best, resistance_sixes);
        match resistance_outcome {
            "critical" | "success" => {
                severity = "none".to_owned();
                stress_cost = stress_cost.saturating_add(1);
            }
            "mixed" => {
                severity = reduce_severity(&severity).to_owned();
                stress_cost = stress_cost.saturating_add(2);
            }
            _ => {
                stress_cost = stress_cost.saturating_add(3);
            }
        }
    }
    if outcome == "critical" {
        dark_hearts = (dark_hearts + 1).min(MAX_DARK_HEARTS);
    }
    let stress_after = stress_before.saturating_add(stress_cost).min(MAX_STRESS);
    stress_before = stress_before.min(MAX_STRESS);

    let condition = object_symbol(state_fields(state)?, "condition")?.to_owned();
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    set_u32(next_fields, "stress", stress_after);
    set_u32(next_fields, "dark_hearts", dark_hearts);
    next_fields.insert(
        "last_outcome".to_owned(),
        DomainValue::Symbol(outcome.to_owned()),
    );

    let consequence_label = if severity == "none" {
        "No lasting consequence".to_owned()
    } else {
        format!("A {severity} {kind} follows from the action")
    };
    let payload = object([
        ("action", DomainValue::Symbol(action.symbol().to_owned())),
        ("approach", DomainValue::Symbol(approach_label.to_owned())),
        ("condition", DomainValue::Symbol(condition)),
        ("consequence_kind", DomainValue::Symbol(kind.to_owned())),
        ("consequence_label", DomainValue::String(consequence_label)),
        ("consequence_severity", DomainValue::Symbol(severity)),
        ("dark_hearts", DomainValue::Number(f64::from(dark_hearts))),
        (
            "dice",
            DomainValue::List(
                dice.iter()
                    .map(|value| DomainValue::Number(f64::from(*value)))
                    .collect(),
            ),
        ),
        ("effect", DomainValue::Symbol(effect.to_owned())),
        ("going_hard", DomainValue::Bool(going_hard)),
        ("intention", DomainValue::String(intention)),
        ("outcome", DomainValue::Symbol(outcome.to_owned())),
        ("pool", DomainValue::Number(f64::from(dice_count))),
        ("position", DomainValue::Symbol(position.to_owned())),
        ("resisted", DomainValue::Bool(resisted)),
        (
            "resistance_dice",
            DomainValue::List(
                resistance_dice
                    .iter()
                    .map(|value| DomainValue::Number(f64::from(*value)))
                    .collect(),
            ),
        ),
        (
            "resistance_outcome",
            DomainValue::Symbol(resistance_outcome.to_owned()),
        ),
        ("stress_after", DomainValue::Number(f64::from(stress_after))),
        (
            "stress_before",
            DomainValue::Number(f64::from(stress_before)),
        ),
        ("stress_cost", DomainValue::Number(f64::from(stress_cost))),
    ]);
    Ok((next, vec![public_event("action_resolved", payload)]))
}

fn teamwork_dice(fields: &BTreeMap<String, DomainValue>) -> Result<u32, TabletopError> {
    let contributions = object_list(fields, "teamwork")?;
    if contributions.len() > 4 {
        return Err(invalid("input.teamwork", "at most four helpers may assist"));
    }
    let mut helpers = BTreeSet::new();
    let mut total = 0_u32;
    for (index, contribution) in contributions.iter().enumerate() {
        let contribution_fields = value_object(format!("input.teamwork[{index}]"), contribution)?;
        let helper_id = object_string(contribution_fields, "helper_id")?;
        validate_local_identifier(format!("input.teamwork[{index}].helper_id"), helper_id)?;
        if !helpers.insert(helper_id.to_owned()) {
            return Err(invalid("input.teamwork", "one helper cannot assist twice"));
        }
        let dice = object_u32(contribution_fields, "dice")?;
        if dice > 2 {
            return Err(invalid(
                "input.teamwork.dice",
                "one helper cannot contribute more than two dice",
            ));
        }
        total = total.saturating_add(dice);
    }
    if total > MAX_TEAMWORK_DICE {
        return Err(invalid(
            "input.teamwork",
            "teamwork cannot contribute more than four dice",
        ));
    }
    Ok(total)
}

fn resistance_choice(
    fields: &BTreeMap<String, DomainValue>,
) -> Result<Option<(WickedOnesAction, u32)>, TabletopError> {
    match fields.get("resistance") {
        None | Some(DomainValue::Null) => Ok(None),
        Some(value) => {
            let resistance_fields = value_object("input.resistance", value)?;
            let action =
                WickedOnesAction::from_symbol(object_symbol(resistance_fields, "action")?)?;
            let bonus = object_u32(resistance_fields, "bonus")?;
            if bonus > 3 {
                return Err(invalid(
                    "input.resistance.bonus",
                    "resistance bonus cannot exceed three",
                ));
            }
            Ok(Some((action, bonus)))
        }
    }
}

fn roll_dice(entropy: &mut EntropyStream, count: u32) -> Result<Vec<u32>, TabletopError> {
    (0..count)
        .map(|_| {
            u32::try_from(entropy.draw_bounded(6)? + 1)
                .map_err(|_| invalid("roll", "die result overflow"))
        })
        .collect()
}

fn effective_best(dice: &[u32], take_lowest: bool) -> (u32, usize) {
    let sixes = dice.iter().filter(|value| **value == 6).count();
    let best = if take_lowest {
        dice.iter().copied().min().unwrap_or(1)
    } else {
        dice.iter().copied().max().unwrap_or(1)
    };
    (best, sixes)
}

fn outcome_band(best: u32, sixes: usize) -> &'static str {
    if sixes >= 2 {
        "critical"
    } else if best == 6 {
        "success"
    } else if best >= 4 {
        "mixed"
    } else {
        "failure"
    }
}

fn consequence_severity(outcome: &str, position: &str, effect: &str) -> &'static str {
    let base = match position {
        "controlled" => "minor",
        "risky" => "moderate",
        _ => "severe",
    };
    let adjusted = match effect {
        "limited" => reduce_severity(base),
        "great" => increase_severity(base),
        _ => base,
    };
    match outcome {
        "success" | "critical" => "none",
        "mixed" => reduce_severity(adjusted),
        _ => adjusted,
    }
}

fn reduce_severity(severity: &str) -> &'static str {
    match severity {
        "severe" => "moderate",
        "moderate" => "minor",
        _ => "none",
    }
}

fn increase_severity(severity: &str) -> &'static str {
    match severity {
        "minor" => "moderate",
        "moderate" | "severe" => "severe",
        _ => "none",
    }
}

fn consequence_kind(approach: &str) -> &'static str {
    match approach {
        "force" => "harm",
        "guile" => "complication",
        _ => "lost_opportunity",
    }
}

fn action_stress(outcome: &str, going_hard: bool) -> u32 {
    let base = match outcome {
        "mixed" => 1,
        "failure" => 2,
        _ => 0,
    };
    base + u32::from(going_hard)
}

fn resolve_flashback(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let summary = object_string(fields, "summary")?.to_owned();
    validate_bounded_text("input.summary", &summary, 1, 512)?;
    let stress_cost = object_u32(fields, "stress_cost")?;
    if stress_cost == 0 || stress_cost > 4 {
        return Err(invalid(
            "input.stress_cost",
            "flashback Stress cost must be within 1..4",
        ));
    }
    let state_fields = state_fields(state)?;
    require_alive(state_fields)?;
    let stress_before = object_u32(state_fields, "stress")?;
    let stress_after = stress_before.saturating_add(stress_cost).min(MAX_STRESS);
    let flashbacks = object_u32(state_fields, "flashbacks")?
        .saturating_add(1)
        .min(99);
    let condition = object_symbol(state_fields, "condition")?.to_owned();
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    set_u32(next_fields, "stress", stress_after);
    set_u32(next_fields, "flashbacks", flashbacks);
    let payload = object([
        ("condition", DomainValue::Symbol(condition)),
        ("flashbacks", DomainValue::Number(f64::from(flashbacks))),
        ("stress_after", DomainValue::Number(f64::from(stress_after))),
        (
            "stress_before",
            DomainValue::Number(f64::from(stress_before)),
        ),
        ("summary", DomainValue::String(summary)),
    ]);
    Ok((next, vec![public_event("flashback_recorded", payload)]))
}

fn resolve_gain_dark_heart(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let reason = object_string(fields, "reason")?.to_owned();
    validate_bounded_text("input.reason", &reason, 1, 512)?;
    let state_fields = state_fields(state)?;
    require_alive(state_fields)?;
    let dark_hearts = object_u32(state_fields, "dark_hearts")?;
    if dark_hearts >= MAX_DARK_HEARTS {
        return Err(invalid(
            "state.dark_hearts",
            "the dark heart track is already full",
        ));
    }
    let after = dark_hearts + 1;
    let condition = object_symbol(state_fields, "condition")?.to_owned();
    let mut next = state.clone();
    set_u32(value_object_mut("state", &mut next)?, "dark_hearts", after);
    let payload = object([
        ("condition", DomainValue::Symbol(condition)),
        ("dark_hearts", DomainValue::Number(f64::from(after))),
        ("reason", DomainValue::String(reason)),
    ]);
    Ok((next, vec![public_event("dark_heart_gained", payload)]))
}

fn resolve_apply_shock(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let amount = object_u32(fields, "amount")?;
    if amount == 0 || amount > MAX_SHOCK {
        return Err(invalid("input.amount", "shock amount must be within 1..4"));
    }
    let source = object_string(fields, "source")?.to_owned();
    validate_bounded_text("input.source", &source, 1, 512)?;
    let state_fields = state_fields(state)?;
    require_alive(state_fields)?;
    let shock_before = object_u32(state_fields, "shock")?;
    if shock_before >= MAX_SHOCK {
        return Err(invalid("state.shock", "the shock track is already full"));
    }
    let shock_after = shock_before.saturating_add(amount).min(MAX_SHOCK);
    let condition_before = object_symbol(state_fields, "condition")?.to_owned();
    let bloodied_before = object_bool(state_fields, "bloodied")?;
    let bloodied = bloodied_before || shock_after == MAX_SHOCK;
    let condition_after = if bloodied && condition_before == "steady" {
        "bloodied"
    } else {
        condition_before.as_str()
    };
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    set_u32(next_fields, "shock", shock_after);
    next_fields.insert("bloodied".to_owned(), DomainValue::Bool(bloodied));
    next_fields.insert(
        "condition".to_owned(),
        DomainValue::Symbol(condition_after.to_owned()),
    );
    let mut events = Vec::new();
    if condition_before != condition_after {
        events.push(public_event(
            "condition_changed",
            object([
                (
                    "cause",
                    DomainValue::String("Shock filled the track".to_owned()),
                ),
                ("from", DomainValue::Symbol(condition_before.clone())),
                ("to", DomainValue::Symbol(condition_after.to_owned())),
            ]),
        ));
    }
    events.push(public_event(
        "shock_applied",
        object([
            ("amount", DomainValue::Number(f64::from(amount))),
            ("bloodied", DomainValue::Bool(bloodied)),
            ("condition", DomainValue::Symbol(condition_after.to_owned())),
            ("shock_after", DomainValue::Number(f64::from(shock_after))),
            ("shock_before", DomainValue::Number(f64::from(shock_before))),
            ("source", DomainValue::String(source)),
        ]),
    ));
    Ok((next, events))
}

fn resolve_go_feral(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let trigger = object_string(fields, "trigger")?.to_owned();
    validate_bounded_text("input.trigger", &trigger, 1, 512)?;
    let state_fields = state_fields(state)?;
    require_alive(state_fields)?;
    if !object_bool(state_fields, "bloodied")? {
        return Err(invalid(
            "state.bloodied",
            "a character must be bloodied before going feral",
        ));
    }
    if object_bool(state_fields, "feral")? {
        return Err(invalid("state.feral", "the character is already feral"));
    }
    let condition_before = object_symbol(state_fields, "condition")?.to_owned();
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    next_fields.insert("feral".to_owned(), DomainValue::Bool(true));
    next_fields.insert(
        "condition".to_owned(),
        DomainValue::Symbol("feral".to_owned()),
    );
    let payload = object([
        ("cause", DomainValue::String(trigger)),
        ("from", DomainValue::Symbol(condition_before)),
        ("to", DomainValue::Symbol("feral".to_owned())),
    ]);
    Ok((next, vec![public_event("condition_changed", payload)]))
}

fn resolve_death_knell(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let knell = object_string(fields, "knell")?.to_owned();
    validate_bounded_text("input.knell", &knell, 1, 512)?;
    let state_fields = state_fields(state)?;
    require_alive(state_fields)?;
    if !object_bool(state_fields, "feral")? {
        return Err(invalid(
            "state.feral",
            "a death knell requires a feral character",
        ));
    }
    let condition_before = object_symbol(state_fields, "condition")?.to_owned();
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    next_fields.insert("dead".to_owned(), DomainValue::Bool(true));
    next_fields.insert(
        "condition".to_owned(),
        DomainValue::Symbol("dead".to_owned()),
    );
    next_fields.insert("death_knell".to_owned(), DomainValue::String(knell.clone()));
    let events = vec![
        public_event(
            "condition_changed",
            object([
                (
                    "cause",
                    DomainValue::String("A final death knell was recorded".to_owned()),
                ),
                ("from", DomainValue::Symbol(condition_before)),
                ("to", DomainValue::Symbol("dead".to_owned())),
            ]),
        ),
        public_event(
            "death_knell_recorded",
            object([
                ("condition", DomainValue::Symbol("dead".to_owned())),
                ("knell", DomainValue::String(knell)),
            ]),
        ),
    ];
    Ok((next, events))
}

fn state_fields(state: &DomainValue) -> Result<&BTreeMap<String, DomainValue>, TabletopError> {
    value_object("state", state)
}

/// Complete, portable Wicked Ones adapter manifest.
#[must_use]
pub fn wicked_ones_manifest() -> AdapterManifest {
    let gear_item_type = object_type([
        ("id", required(string(1, 128), "Stable gear identity.")),
        ("label", required(string(1, 160), "Gear display label.")),
        (
            "quantity",
            required(integer(1.0, 99.0), "Carried quantity."),
        ),
    ]);
    let vile_friend_type = object_type([
        (
            "detail",
            required(string(0, 512), "Vile friend description."),
        ),
        ("name", required(string(1, 160), "Vile friend name.")),
    ]);
    let teamwork_type = object_type([
        (
            "dice",
            required(integer(0.0, 2.0), "Dice contributed by this helper."),
        ),
        (
            "helper_id",
            required(string(1, 128), "Stable helper identity."),
        ),
    ]);
    let resistance_type = object_type([
        (
            "action",
            required(action_type(), "Action used to resist the consequence."),
        ),
        (
            "bonus",
            required(integer(0.0, 3.0), "Additional resistance dice."),
        ),
    ]);
    let room_type = object_type([
        ("id", required(string(1, 128), "Stable room identity.")),
        ("label", required(string(1, 160), "Room display label.")),
        ("purpose", required(string(0, 512), "Room purpose.")),
    ]);
    let structure_type = object_type([
        ("id", required(string(1, 128), "Stable structure identity.")),
        (
            "kind",
            required(
                symbol(["barracks", "shrine", "trapworks", "vault", "workshop"]),
                "Structure category.",
            ),
        ),
        (
            "label",
            required(string(1, 160), "Structure display label."),
        ),
        (
            "room_id",
            required(string(0, 128), "Owning room reference."),
        ),
    ]);
    let creature_type = object_type([
        ("id", required(string(1, 128), "Stable creature identity.")),
        (
            "kind",
            required(
                symbol(["beast", "construct", "swarm", "vermin"]),
                "Creature category.",
            ),
        ),
        ("label", required(string(1, 160), "Creature display label.")),
        (
            "pack_id",
            required(string(0, 128), "Owning pack reference."),
        ),
        (
            "room_id",
            required(string(0, 128), "Owning room reference."),
        ),
    ]);
    let lock_type = object_type([
        ("id", required(string(1, 128), "Stable lock identity.")),
        (
            "kind",
            required(symbol(["lockbox", "seal", "ward"]), "Lock category."),
        ),
        ("label", required(string(1, 160), "Lock display label.")),
        (
            "room_id",
            required(string(0, 128), "Owning room reference."),
        ),
    ]);
    let trap_type = object_type([
        (
            "armed",
            required(TypeExpression::Bool, "Whether the trap is armed."),
        ),
        ("id", required(string(1, 128), "Stable trap identity.")),
        (
            "kind",
            required(symbol(["collapse", "flame", "snare"]), "Trap category."),
        ),
        ("label", required(string(1, 160), "Trap display label.")),
        (
            "room_id",
            required(string(0, 128), "Owning room reference."),
        ),
    ]);
    let trick_type = object_type([
        ("effect", required(string(0, 512), "Trick effect.")),
        ("id", required(string(1, 128), "Stable trick identity.")),
        ("label", required(string(1, 160), "Trick display label.")),
        (
            "room_id",
            required(string(0, 128), "Owning room reference."),
        ),
    ]);
    let imp_type = object_type([
        (
            "duty_id",
            required(string(0, 128), "Assigned duty reference."),
        ),
        ("id", required(string(1, 128), "Stable imp identity.")),
        ("name", required(string(1, 160), "Imp name.")),
    ]);
    let minion_pack_type = object_type([
        ("count", required(integer(0.0, 255.0), "Minion count.")),
        ("id", required(string(1, 128), "Stable pack identity.")),
        (
            "kind",
            required(
                symbol(["brutes", "diggers", "flyers", "skulkers"]),
                "Minion category.",
            ),
        ),
        ("label", required(string(1, 160), "Pack display label.")),
        ("morale", required(integer(0.0, 3.0), "Pack morale.")),
        (
            "room_id",
            required(string(0, 128), "Owning room reference."),
        ),
    ]);
    let dungeon_type = object_type([
        (
            "atmosphere",
            required(string(1, 240), "Dungeon atmosphere."),
        ),
        (
            "creatures",
            required(list(named("Creature"), 0, 128), "Dungeon creatures."),
        ),
        ("entrance", required(string(1, 240), "Dungeon entrance.")),
        ("gold", required(integer(0.0, 999_999.0), "Held gold.")),
        ("hoard", required(string(1, 240), "Hoard description.")),
        ("id", required(string(1, 128), "Stable dungeon identity.")),
        ("imps", required(list(named("Imp"), 0, 64), "Dungeon imps.")),
        (
            "locks",
            required(list(named("Lock"), 0, 64), "Dungeon locks."),
        ),
        (
            "minion_packs",
            required(list(named("MinionPack"), 0, 64), "Minion packs."),
        ),
        (
            "rooms",
            required(list(named("Room"), 0, 128), "Dungeon rooms."),
        ),
        (
            "structures",
            required(list(named("Structure"), 0, 128), "Dungeon structures."),
        ),
        ("supply", required(integer(0.0, 999_999.0), "Held supply.")),
        ("tier", required(integer(1.0, 4.0), "Dungeon tier.")),
        (
            "traps",
            required(list(named("Trap"), 0, 64), "Dungeon traps."),
        ),
        (
            "tricks",
            required(list(named("Trick"), 0, 64), "Dungeon tricks."),
        ),
        (
            "world_ref",
            required(string(0, 255), "Optional Weave World reference."),
        ),
    ]);
    let faction_clock_type = object_type([
        ("filled", required(integer(0.0, 8.0), "Filled segments.")),
        ("id", required(string(1, 128), "Stable clock identity.")),
        ("label", required(string(1, 160), "Clock display label.")),
        ("segments", required(integer(1.0, 8.0), "Total segments.")),
    ]);
    let faction_type = object_type([
        (
            "clocks",
            required(list(named("FactionClock"), 0, 16), "Faction clocks."),
        ),
        ("id", required(string(1, 128), "Stable faction identity.")),
        ("name", required(string(1, 160), "Faction name.")),
        ("tier", required(integer(1.0, 4.0), "Faction tier.")),
        (
            "world_ref",
            required(string(0, 255), "Optional Weave World reference."),
        ),
    ]);
    let master_plan_type = object_type([
        ("filled", required(integer(0.0, 8.0), "Filled segments.")),
        ("segments", required(integer(1.0, 8.0), "Total segments.")),
        ("title", required(string(1, 160), "Master plan title.")),
    ]);
    let duty_type = object_type([
        (
            "assignee_id",
            required(string(0, 128), "Assigned imp or pack."),
        ),
        ("id", required(string(1, 128), "Stable duty identity.")),
        ("label", required(string(1, 160), "Duty display label.")),
    ]);
    let project_type = object_type([
        ("filled", required(integer(0.0, 8.0), "Filled segments.")),
        ("id", required(string(1, 128), "Stable project identity.")),
        (
            "kind",
            required(
                symbol(["defense", "raid", "ritual", "upgrade"]),
                "Project category.",
            ),
        ),
        ("label", required(string(1, 160), "Project display label.")),
        (
            "room_id",
            required(string(0, 128), "Owning room reference."),
        ),
        ("segments", required(integer(1.0, 8.0), "Total segments.")),
    ]);
    let target_type = object_type([
        (
            "description",
            required(string(0, 512), "Target description."),
        ),
        (
            "faction_id",
            required(string(0, 128), "Owning faction reference."),
        ),
        ("id", required(string(1, 128), "Stable target identity.")),
        ("label", required(string(1, 160), "Target display label.")),
        ("tier", required(integer(1.0, 4.0), "Target tier.")),
    ]);
    let raid_type = object_type([
        (
            "advantages",
            required(list(string(1, 160), 0, 8), "Declared advantages."),
        ),
        (
            "blowback",
            required(integer(0.0, 3.0), "Blowback severity."),
        ),
        (
            "disadvantages",
            required(list(string(1, 160), 0, 8), "Declared disadvantages."),
        ),
        ("effect", required(effect_type(), "Engagement effect.")),
        ("goal", required(string(0, 512), "Declared goal.")),
        ("id", required(string(1, 128), "Stable raid identity.")),
        ("label", required(string(1, 160), "Raid display label.")),
        ("loot", required(integer(0.0, 999_999.0), "Raid loot.")),
        ("plan", required(string(0, 512), "Declared plan.")),
        (
            "position",
            required(position_type(), "Engagement position."),
        ),
        (
            "scene",
            required(string(0, 512), "Structured scene summary."),
        ),
        (
            "state",
            required(
                symbol(["engaged", "planned", "resolved"]),
                "Raid lifecycle state.",
            ),
        ),
        ("target_id", required(string(1, 128), "Target reference.")),
    ]);
    let upgrade_type = object_type([
        ("id", required(string(1, 128), "Stable upgrade identity.")),
        ("label", required(string(1, 160), "Upgrade display label.")),
        (
            "purchased",
            required(TypeExpression::Bool, "Purchase flag."),
        ),
        ("tier", required(integer(1.0, 4.0), "Upgrade tier.")),
    ]);
    let action_advancement_type = object_type([
        ("action", required(action_type(), "Advanced action.")),
        ("from", required(integer(0.0, 3.0), "Prior rating.")),
        ("to", required(integer(0.0, 3.0), "New rating.")),
    ]);
    let advancement_type = object_type([
        (
            "abilities",
            required(list(string(1, 128), 0, 16), "Gained abilities."),
        ),
        (
            "actions",
            required(
                list(named("ActionAdvancement"), 0, 16),
                "Recorded action advancements.",
            ),
        ),
        (
            "reckless",
            required(integer(0.0, 999.0), "Reckless points."),
        ),
        ("wicked", required(integer(0.0, 999.0), "Wicked points.")),
        (
            "xp",
            required(integer(0.0, 999_999.0), "Experience points."),
        ),
    ]);
    let cycle_type = object_type([
        ("calamity", required(string(0, 512), "Latest calamity.")),
        (
            "downtime_actions",
            required(integer(0.0, 999.0), "Downtime actions taken."),
        ),
        (
            "history",
            required(list(string(0, 512), 0, 128), "Cycle history."),
        ),
        (
            "last_raid_id",
            required(string(0, 128), "Most recent raid reference."),
        ),
        (
            "lurking",
            required(list(string(0, 512), 0, 64), "Lurking notes."),
        ),
        ("paused", required(TypeExpression::Bool, "Pause flag.")),
        (
            "phase",
            required(symbol(WICKED_ONES_PHASES), "Current phase."),
        ),
        (
            "raid_reviewed",
            required(TypeExpression::Bool, "Raid plan review flag."),
        ),
        (
            "recovery_used",
            required(integer(0.0, 999.0), "Recovery uses."),
        ),
        ("revelry", required(string(0, 512), "Latest revelry.")),
    ]);
    let campaign_type = object_type([
        (
            "advancement",
            required(named("AdvancementState"), "Advancement state."),
        ),
        ("cycle", required(named("CycleState"), "Cycle state.")),
        (
            "duties",
            required(list(named("Duty"), 0, 64), "Dungeon duties."),
        ),
        ("dungeon", required(named("DungeonState"), "Dungeon state.")),
        (
            "factions",
            required(list(named("FactionState"), 0, 64), "Factions."),
        ),
        ("loot", required(integer(0.0, 999_999.0), "Total loot.")),
        ("master_plan", required(named("MasterPlan"), "Master plan.")),
        ("namespace", required(string(1, 32), "Adapter namespace.")),
        (
            "projects",
            required(list(named("Project"), 0, 64), "Projects."),
        ),
        ("raids", required(list(named("RaidState"), 0, 64), "Raids.")),
        (
            "targets",
            required(list(named("Target"), 0, 64), "Raid targets."),
        ),
        (
            "upgrades",
            required(list(named("Upgrade"), 0, 64), "Upgrades."),
        ),
    ]);
    let types = BTreeMap::from([
        ("ActionAdvancement".to_owned(), action_advancement_type),
        ("AdvancementState".to_owned(), advancement_type),
        ("CampaignState".to_owned(), campaign_type),
        ("Creature".to_owned(), creature_type),
        ("CycleState".to_owned(), cycle_type),
        ("DungeonState".to_owned(), dungeon_type),
        ("Duty".to_owned(), duty_type),
        ("FactionClock".to_owned(), faction_clock_type),
        ("FactionState".to_owned(), faction_type),
        ("GearItem".to_owned(), gear_item_type),
        ("Imp".to_owned(), imp_type),
        ("Lock".to_owned(), lock_type),
        ("MasterPlan".to_owned(), master_plan_type),
        ("MinionPack".to_owned(), minion_pack_type),
        ("Project".to_owned(), project_type),
        ("RaidState".to_owned(), raid_type),
        ("ResistanceChoice".to_owned(), resistance_type),
        ("Room".to_owned(), room_type),
        ("Structure".to_owned(), structure_type),
        ("Target".to_owned(), target_type),
        ("TeamworkContribution".to_owned(), teamwork_type),
        ("Trap".to_owned(), trap_type),
        ("Trick".to_owned(), trick_type),
        ("Upgrade".to_owned(), upgrade_type),
        ("VileFriend".to_owned(), vile_friend_type),
    ]);

    let definition_type = object_type([
        (
            "abilities",
            required(
                list(string(1, 128), 2, 2),
                "Exactly two abilities offered by the Calling.",
            ),
        ),
        (
            "actions",
            required(
                map(integer(0.0, 3.0), 9, 9),
                "Ratings for all nine actions.",
            ),
        ),
        ("calling", required(calling_type(), "Selected Calling.")),
        (
            "dark_impulse",
            required(string(1, 160), "Authored dark impulse."),
        ),
        (
            "gear",
            required(list(named("GearItem"), 1, 8), "Starting gear lines."),
        ),
        ("look", required(string(1, 240), "Authored look.")),
        ("name", required(string(1, 160), "Fictional name.")),
        ("race", required(string(1, 80), "Authored race.")),
        ("revelry", required(string(1, 160), "Authored revelry.")),
        (
            "starting_resources",
            required(integer(0.0, 999.0), "Starting resource amount."),
        ),
        (
            "vile_friend",
            required(named("VileFriend"), "The character's vile friend."),
        ),
    ]);
    let state_type = object_type([
        (
            "bloodied",
            required(TypeExpression::Bool, "Whether Shock is full."),
        ),
        (
            "campaign",
            required(
                named("CampaignState"),
                "Durable dungeon, faction, raid, and campaign state.",
            ),
        ),
        (
            "condition",
            required(condition_type(), "Current condition."),
        ),
        (
            "dark_hearts",
            required(integer(0.0, 6.0), "Dark heart count."),
        ),
        (
            "dead",
            required(
                TypeExpression::Bool,
                "Whether the death knell was recorded.",
            ),
        ),
        (
            "death_knell",
            required(string(0, 512), "Final death knell or an empty value."),
        ),
        (
            "feral",
            required(TypeExpression::Bool, "Whether the character went feral."),
        ),
        (
            "flashbacks",
            required(integer(0.0, 99.0), "Recorded flashbacks."),
        ),
        (
            "last_outcome",
            required(outcome_type_with_none(), "Most recent action outcome."),
        ),
        (
            "resources",
            required(integer(0.0, 999.0), "Current resources."),
        ),
        ("shock", required(integer(0.0, 4.0), "Current Shock.")),
        ("stress", required(integer(0.0, 9.0), "Current Stress.")),
    ]);

    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: WICKED_ONES_ADAPTER_ID.to_owned(),
        version: WICKED_ONES_ADAPTER_VERSION.to_owned(),
        schema_version: 1,
        namespace: "wicked_ones".to_owned(),
        title: "Wicked Ones v1.4 adapter".to_owned(),
        compatibility_label: "Compatible with Wicked Ones v1.4".to_owned(),
        summary: "CC0 Wicked Ones characters with Callings, grouped action ratings, dark impulses, Stress, dark hearts, action rolls, resistance, teamwork, flashbacks, and explicit condition transitions."
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
            capability(TabletopCapability::EquipmentAndAbilities),
            capability(TabletopCapability::Advancement),
            capability(TabletopCapability::Clocks),
            capability(TabletopCapability::Factions),
            capability(TabletopCapability::WorldState),
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
            source_url: WICKED_ONES_SOURCE_URL.to_owned(),
            exact_artifact: "SOURCE_RECORD.txt".to_owned(),
            revision: WICKED_ONES_SOURCE_EDITION.to_owned(),
            retrieved_on: "2026-11-06".to_owned(),
            sha256: WICKED_ONES_SOURCE_RECORD_SHA256.to_owned(),
            license: WICKED_ONES_LICENSE.to_owned(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt"
                .to_owned(),
            covered_files_or_sections: vec![
                "Wicked Ones v1.4 core game text released under CC0-1.0.".to_owned(),
                "Character creation, Callings, action ratings grouped by Brains, Muscles, and Guts, dark impulse, revelry, gear, abilities, vile friend, and resources."
                    .to_owned(),
                "Action rolls with intention, approach, position and effect, teamwork, going hard, resistance, and outcomes."
                    .to_owned(),
                "Stress, dark hearts, flashbacks, shock, bloodied, feral, death, and the final death knell."
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
            attribution: "Wicked Ones was created by Ben Nielson and published by Bandit Camp; the core game was released under CC0-1.0 in January 2024. Weave's adapter implementation, fixtures, schemas, and explanatory copy are independently authored."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "LICENSE-CC0-1.0.txt".to_owned(),
                sha256: WICKED_ONES_CC0_LEGAL_CODE_SHA256.to_owned(),
            },
            additional_artifacts: Vec::new(),
            notices: WICKED_ONES_NOTICES
                .iter()
                .map(|notice| (*notice).to_owned())
                .collect(),
            compatibility_statement: "Wicked Ones is named only to describe rules compatibility. This project is independent and does not imply endorsement by the original creator or publisher."
                .to_owned(),
        },
    }
}

fn creation_steps() -> Vec<CreationStep> {
    vec![
        CreationStep {
            id: "core_identity".to_owned(),
            title: "Identity and Calling".to_owned(),
            description: "Name the character, author race and look, and choose a Calling."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("name", "Name", string(1, 160), true),
                creation_field("race", "Race", string(1, 80), true),
                creation_field("look", "Look", string(1, 240), true),
                creation_field("calling", "Calling", calling_type(), true),
            ],
        },
        CreationStep {
            id: "actions".to_owned(),
            title: "Actions, impulse, and revelry".to_owned(),
            description: "Allocate six action points across Brains, Muscles, and Guts, then record the dark impulse and revelry."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field(
                    "actions",
                    "Action ratings",
                    map(integer(0.0, 3.0), 9, 9),
                    true,
                ),
                creation_field(
                    "dark_impulse",
                    "Dark impulse",
                    string(1, 160),
                    true,
                ),
                creation_field("revelry", "Revelry", string(1, 160), true),
            ],
        },
        CreationStep {
            id: "abilities_and_friend".to_owned(),
            title: "Abilities and vile friend".to_owned(),
            description: "Choose exactly two abilities offered by the Calling and describe the vile friend."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field(
                    "abilities",
                    "Abilities",
                    list(string(1, 128), 2, 2),
                    true,
                ),
                creation_field(
                    "vile_friend",
                    "Vile friend",
                    named("VileFriend"),
                    true,
                ),
            ],
        },
        CreationStep {
            id: "gear_and_resources".to_owned(),
            title: "Gear and resources".to_owned(),
            description: "Record starting gear and the initial resource amount."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field(
                    "gear",
                    "Gear",
                    list(named("GearItem"), 1, 8),
                    true,
                ),
                creation_field(
                    "resources",
                    "Resources",
                    integer(0.0, 999.0),
                    true,
                ),
            ],
        },
    ]
}

fn operations() -> BTreeMap<String, ResolverOperationDeclaration> {
    BTreeMap::from([
        (
            "apply_shock".to_owned(),
            operation(
                "apply_shock",
                "Apply Shock",
                "Add Shock up to the tracked maximum and mark the character bloodied when the track fills.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    ("amount", required(integer(1.0, 4.0), "Shock to add.")),
                    (
                        "source",
                        required(string(1, 512), "Fictional source of the Shock."),
                    ),
                ]),
                ["condition_changed", "shock_applied"],
                false,
            ),
        ),
        (
            "death_knell".to_owned(),
            operation(
                "death_knell",
                "Record the final death knell",
                "Mark a feral character dead and store the final death knell.",
                TabletopCapability::ResourcesAndConditions,
                object_type([("knell", required(string(1, 512), "Final death knell."))]),
                ["condition_changed", "death_knell_recorded"],
                false,
            ),
        ),
        (
            "flashback".to_owned(),
            operation(
                "flashback",
                "Declare a flashback",
                "Spend Stress to establish retroactive preparation and count the flashback.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    (
                        "stress_cost",
                        required(integer(1.0, 4.0), "Stress paid for the flashback."),
                    ),
                    (
                        "summary",
                        required(string(1, 512), "Retroactive preparation summary."),
                    ),
                ]),
                ["flashback_recorded"],
                false,
            ),
        ),
        (
            "gain_dark_heart".to_owned(),
            operation(
                "gain_dark_heart",
                "Gain a dark heart",
                "Advance the dark heart track by one up to its declared maximum.",
                TabletopCapability::ResourcesAndConditions,
                object_type([(
                    "reason",
                    required(string(1, 512), "Why the dark heart was gained."),
                )]),
                ["dark_heart_gained"],
                false,
            ),
        ),
        (
            "go_feral".to_owned(),
            operation(
                "go_feral",
                "Go feral",
                "Move a bloodied character into the feral condition.",
                TabletopCapability::ResourcesAndConditions,
                object_type([(
                    "trigger",
                    required(string(1, 512), "Fictional trigger for going feral."),
                )]),
                ["condition_changed"],
                false,
            ),
        ),
        (
            "resolve_action".to_owned(),
            operation(
                "resolve_action",
                "Resolve an action roll",
                "Validate intention, approach, action, position, and effect; add declared and teamwork dice; draw deterministic d6 results; and apply outcome bands, consequences, resistance, and Stress.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    ("action", required(action_type(), "Action rolled.")),
                    ("approach", required(approach_type(), "Approach used.")),
                    ("bonus", required(integer(0.0, 3.0), "Declared bonus dice.")),
                    ("effect", required(effect_type(), "Declared effect.")),
                    (
                        "going_hard",
                        required(TypeExpression::Bool, "Add one die and one Stress."),
                    ),
                    ("intention", required(string(1, 512), "Declared intention.")),
                    ("position", required(position_type(), "Declared position.")),
                    (
                        "resistance",
                        optional(
                            named("ResistanceChoice"),
                            "Optional resistance roll against a consequence.",
                        ),
                    ),
                    (
                        "teamwork",
                        required(
                            list(named("TeamworkContribution"), 0, 4),
                            "Declared teamwork dice.",
                        ),
                    ),
                ]),
                ["action_resolved", "condition_changed"],
                true,
            ),
        ),
        (
            "set_campaign".to_owned(),
            operation(
                "set_campaign",
                "Replace campaign state",
                "Validate, normalize, and store dungeon, faction, raid, project, and advancement entities.",
                TabletopCapability::CampaignState,
                object_type([(
                    "campaign",
                    required(named("CampaignState"), "Complete campaign state."),
                )]),
                ["campaign_initialized"],
                false,
            ),
        ),
        (
            "advance_campaign_phase".to_owned(),
            operation(
                "advance_campaign_phase",
                "Advance the campaign phase",
                "Move to the next allowed cycle phase and record the transition.",
                TabletopCapability::CampaignState,
                object_type([
                    (
                        "next_phase",
                        required(symbol(WICKED_ONES_PHASES), "Requested next phase."),
                    ),
                    ("note", required(string(1, 512), "Transition note.")),
                ]),
                ["campaign_phase_changed"],
                false,
            ),
        ),
        (
            "pause_campaign".to_owned(),
            operation(
                "pause_campaign",
                "Pause the campaign",
                "Hold the cycle at its current phase boundary until an explicit resume.",
                TabletopCapability::CampaignState,
                object_type([("note", required(string(1, 512), "Pause note."))]),
                ["campaign_paused"],
                false,
            ),
        ),
        (
            "resume_campaign".to_owned(),
            operation(
                "resume_campaign",
                "Resume the campaign",
                "Clear the pause flag at the current phase boundary.",
                TabletopCapability::CampaignState,
                object_type([("note", required(string(1, 512), "Resume note."))]),
                ["campaign_resumed"],
                false,
            ),
        ),
        (
            "perform_recovery".to_owned(),
            operation(
                "perform_recovery",
                "Recover during the recovery phase",
                "Reduce Stress by a declared amount and record the recovery use.",
                TabletopCapability::CampaignState,
                object_type([
                    ("amount", required(integer(1.0, 3.0), "Stress recovered.")),
                    ("note", required(string(1, 512), "Recovery note.")),
                ]),
                ["recovery_performed"],
                false,
            ),
        ),
        (
            "perform_downtime_action".to_owned(),
            operation(
                "perform_downtime_action",
                "Perform a downtime action",
                "Advance a project, lurk, revel, or recover while the cycle is in downtime.",
                TabletopCapability::CampaignState,
                object_type([
                    (
                        "kind",
                        required(
                            symbol(["lurk", "project", "recovery", "revel"]),
                            "Downtime action kind.",
                        ),
                    ),
                    ("note", required(string(1, 512), "Downtime action note.")),
                    (
                        "project_id",
                        required(string(0, 128), "Project reference or an empty value."),
                    ),
                ]),
                ["downtime_action_resolved"],
                false,
            ),
        ),
        (
            "resolve_dungeon_event".to_owned(),
            operation(
                "resolve_dungeon_event",
                "Resolve a dungeon event",
                "Record one explicit dungeon event while the cycle is in dungeon events.",
                TabletopCapability::CampaignState,
                object_type([
                    ("event", required(string(1, 512), "Dungeon event.")),
                    ("note", required(string(1, 512), "Resolution note.")),
                ]),
                ["dungeon_event_resolved"],
                false,
            ),
        ),
        (
            "resolve_calamity".to_owned(),
            operation(
                "resolve_calamity",
                "Resolve a calamity",
                "Record the calamity and reduce Supply by its severity.",
                TabletopCapability::CampaignState,
                object_type([
                    ("note", required(string(1, 512), "Calamity note.")),
                    (
                        "severity",
                        required(integer(1.0, 3.0), "Calamity severity."),
                    ),
                ]),
                ["calamity_resolved"],
                false,
            ),
        ),
        (
            "plan_raid".to_owned(),
            operation(
                "plan_raid",
                "Plan and review a raid",
                "Declare the plan, goal, target, and raid modifiers during the raiding phase.",
                TabletopCapability::CampaignState,
                object_type([
                    (
                        "advantages",
                        required(list(string(1, 160), 0, 8), "Declared advantages."),
                    ),
                    (
                        "disadvantages",
                        required(list(string(1, 160), 0, 8), "Declared disadvantages."),
                    ),
                    ("goal", required(string(0, 512), "Declared goal.")),
                    ("label", required(string(1, 160), "Raid display label.")),
                    ("plan", required(string(0, 512), "Declared plan.")),
                    ("raid_id", required(string(1, 128), "Stable raid identity.")),
                    ("target_id", required(string(1, 128), "Target reference.")),
                ]),
                ["raid_planned"],
                false,
            ),
        ),
        (
            "purchase_upgrade".to_owned(),
            operation(
                "purchase_upgrade",
                "Purchase a dungeon upgrade",
                "Spend gold during downtime to mark one declared upgrade as purchased.",
                TabletopCapability::EquipmentAndAbilities,
                object_type([
                    ("note", required(string(1, 512), "Purchase note.")),
                    ("upgrade_id", required(string(1, 128), "Upgrade identity.")),
                ]),
                ["upgrade_purchased"],
                false,
            ),
        ),
        (
            "engage_raid".to_owned(),
            operation(
                "engage_raid",
                "Engage a planned raid",
                "Derive the starting position, effect, and scene data for the existing action loop.",
                TabletopCapability::CampaignState,
                object_type([(
                    "raid_id",
                    required(string(1, 128), "Planned raid identity."),
                )]),
                ["raid_engagement_ready"],
                false,
            ),
        ),
        (
            "resolve_raid_outcome".to_owned(),
            operation(
                "resolve_raid_outcome",
                "Resolve a raid outcome",
                "Apply loot, blowback severity, and advancement points from the declared outcome.",
                TabletopCapability::CampaignState,
                object_type([
                    ("note", required(string(1, 512), "Outcome note.")),
                    ("outcome", required(outcome_type(), "Action-loop outcome.")),
                    (
                        "raid_id",
                        required(string(1, 128), "Engaged raid identity."),
                    ),
                ]),
                ["raid_blowback", "raid_loot_resolved"],
                false,
            ),
        ),
        (
            "adjust_faction_clock".to_owned(),
            operation(
                "adjust_faction_clock",
                "Adjust a faction clock",
                "Apply one explicit, reviewable faction-clock change within its bounds.",
                TabletopCapability::Factions,
                object_type([
                    (
                        "clock_id",
                        required(string(1, 128), "Faction clock identity."),
                    ),
                    (
                        "delta",
                        required(integer(-8.0, 8.0), "Signed segment change."),
                    ),
                    ("faction_id", required(string(1, 128), "Faction identity.")),
                    ("note", required(string(1, 512), "Change note.")),
                ]),
                ["faction_clock_changed"],
                false,
            ),
        ),
        (
            "advance_master_plan".to_owned(),
            operation(
                "advance_master_plan",
                "Advance the master plan",
                "Apply one explicit, reviewable master-plan clock change within its bounds.",
                TabletopCapability::Factions,
                object_type([
                    ("delta", required(integer(1.0, 8.0), "Segments to advance.")),
                    ("note", required(string(1, 512), "Change note.")),
                ]),
                ["master_plan_advanced"],
                false,
            ),
        ),
        (
            "spend_wicked_advancement".to_owned(),
            operation(
                "spend_wicked_advancement",
                "Spend a Wicked advancement",
                "Trade one Wicked point for a new named ability during the advancement phase.",
                TabletopCapability::Advancement,
                object_type([
                    ("ability", required(string(1, 128), "New ability identity.")),
                    ("note", required(string(1, 512), "Advancement note.")),
                ]),
                ["wicked_advancement_spent"],
                false,
            ),
        ),
        (
            "spend_reckless_advancement".to_owned(),
            operation(
                "spend_reckless_advancement",
                "Spend a reckless advancement",
                "Trade one reckless point for an action-rating increase and one Stress.",
                TabletopCapability::Advancement,
                object_type([
                    ("action", required(action_type(), "Action to advance.")),
                    ("note", required(string(1, 512), "Advancement note.")),
                    (
                        "to_rating",
                        required(integer(1.0, 3.0), "New action rating."),
                    ),
                ]),
                ["reckless_advancement_spent"],
                false,
            ),
        ),
    ])
}

fn event_types() -> BTreeMap<String, TypeExpression> {
    BTreeMap::from([
        (
            "action_resolved".to_owned(),
            object_type([
                ("action", required(action_type(), "Action rolled.")),
                ("approach", required(approach_type(), "Approach used.")),
                (
                    "condition",
                    required(condition_type(), "Condition after the roll."),
                ),
                (
                    "consequence_kind",
                    required(consequence_kind_type(), "Consequence category."),
                ),
                (
                    "consequence_label",
                    required(string(0, 512), "Independent consequence summary."),
                ),
                (
                    "consequence_severity",
                    required(consequence_severity_type(), "Final consequence severity."),
                ),
                (
                    "dark_hearts",
                    required(integer(0.0, 6.0), "Dark hearts after the roll."),
                ),
                (
                    "dice",
                    required(list(integer(1.0, 6.0), 1, 8), "Action dice results."),
                ),
                ("effect", required(effect_type(), "Declared effect.")),
                (
                    "going_hard",
                    required(TypeExpression::Bool, "Whether the roll went hard."),
                ),
                ("intention", required(string(1, 512), "Declared intention.")),
                ("outcome", required(outcome_type(), "Outcome band.")),
                (
                    "pool",
                    required(integer(1.0, 8.0), "Dice rolled after the minimum rule."),
                ),
                ("position", required(position_type(), "Declared position.")),
                (
                    "resisted",
                    required(TypeExpression::Bool, "Whether resistance was rolled."),
                ),
                (
                    "resistance_dice",
                    required(list(integer(1.0, 6.0), 0, 8), "Resistance dice results."),
                ),
                (
                    "resistance_outcome",
                    required(outcome_type_with_none(), "Resistance outcome or none."),
                ),
                (
                    "stress_after",
                    required(integer(0.0, 9.0), "Stress after the roll."),
                ),
                (
                    "stress_before",
                    required(integer(0.0, 9.0), "Stress before the roll."),
                ),
                (
                    "stress_cost",
                    required(integer(0.0, 9.0), "Stress paid by the roll."),
                ),
            ]),
        ),
        (
            "condition_changed".to_owned(),
            object_type([
                ("cause", required(string(1, 512), "Recorded cause.")),
                ("from", required(condition_type(), "Previous condition.")),
                ("to", required(condition_type(), "New condition.")),
            ]),
        ),
        (
            "dark_heart_gained".to_owned(),
            object_type([
                (
                    "condition",
                    required(condition_type(), "Current condition."),
                ),
                (
                    "dark_hearts",
                    required(integer(0.0, 6.0), "Dark hearts after the change."),
                ),
                ("reason", required(string(1, 512), "Recorded reason.")),
            ]),
        ),
        (
            "death_knell_recorded".to_owned(),
            object_type([
                ("condition", required(condition_type(), "Final condition.")),
                ("knell", required(string(1, 512), "Final death knell.")),
            ]),
        ),
        (
            "flashback_recorded".to_owned(),
            object_type([
                (
                    "condition",
                    required(condition_type(), "Current condition."),
                ),
                (
                    "flashbacks",
                    required(integer(0.0, 99.0), "Flashbacks after the change."),
                ),
                (
                    "stress_after",
                    required(integer(0.0, 9.0), "Stress after the cost."),
                ),
                (
                    "stress_before",
                    required(integer(0.0, 9.0), "Stress before the cost."),
                ),
                ("summary", required(string(1, 512), "Recorded summary.")),
            ]),
        ),
        (
            "shock_applied".to_owned(),
            object_type([
                ("amount", required(integer(1.0, 4.0), "Shock added.")),
                (
                    "bloodied",
                    required(TypeExpression::Bool, "Whether Shock is full."),
                ),
                (
                    "condition",
                    required(condition_type(), "Current condition."),
                ),
                (
                    "shock_after",
                    required(integer(0.0, 4.0), "Shock after the change."),
                ),
                (
                    "shock_before",
                    required(integer(0.0, 4.0), "Shock before the change."),
                ),
                ("source", required(string(1, 512), "Recorded source.")),
            ]),
        ),
        (
            "campaign_initialized".to_owned(),
            object_type([
                ("dungeon_id", required(string(1, 128), "Dungeon identity.")),
                (
                    "faction_count",
                    required(integer(0.0, 64.0), "Faction count."),
                ),
                ("phase", required(phase_type(), "Current phase.")),
                ("raid_count", required(integer(0.0, 64.0), "Raid count.")),
                ("room_count", required(integer(0.0, 128.0), "Room count.")),
            ]),
        ),
        (
            "campaign_paused".to_owned(),
            object_type([
                ("note", required(string(1, 512), "Pause note.")),
                ("phase", required(phase_type(), "Paused phase.")),
            ]),
        ),
        (
            "campaign_phase_changed".to_owned(),
            object_type([
                ("from", required(phase_type(), "Previous phase.")),
                ("note", required(string(1, 512), "Transition note.")),
                ("paused", required(TypeExpression::Bool, "Pause flag.")),
                ("to", required(phase_type(), "Requested phase.")),
            ]),
        ),
        (
            "campaign_resumed".to_owned(),
            object_type([
                ("note", required(string(1, 512), "Resume note.")),
                ("phase", required(phase_type(), "Resumed phase.")),
            ]),
        ),
        (
            "calamity_resolved".to_owned(),
            object_type([
                ("note", required(string(1, 512), "Calamity note.")),
                (
                    "severity",
                    required(integer(1.0, 3.0), "Calamity severity."),
                ),
                (
                    "supply_after",
                    required(integer(0.0, 999_999.0), "Supply after the calamity."),
                ),
            ]),
        ),
        (
            "downtime_action_resolved".to_owned(),
            object_type([
                (
                    "downtime_actions",
                    required(integer(0.0, 999.0), "Downtime actions taken."),
                ),
                ("filled", required(integer(0.0, 8.0), "Project progress.")),
                (
                    "kind",
                    required(
                        symbol(["lurk", "project", "recovery", "revel"]),
                        "Downtime action kind.",
                    ),
                ),
                ("note", required(string(1, 512), "Downtime action note.")),
                (
                    "project_id",
                    required(string(0, 128), "Project reference or empty."),
                ),
                ("segments", required(integer(0.0, 8.0), "Project segments.")),
                (
                    "stress_after",
                    required(integer(0.0, 9.0), "Stress after recovering."),
                ),
            ]),
        ),
        (
            "dungeon_event_resolved".to_owned(),
            object_type([
                ("event", required(string(1, 512), "Dungeon event.")),
                (
                    "history_len",
                    required(integer(0.0, 128.0), "Cycle history length."),
                ),
                ("note", required(string(1, 512), "Resolution note.")),
            ]),
        ),
        (
            "faction_clock_changed".to_owned(),
            object_type([
                (
                    "clock_id",
                    required(string(1, 128), "Faction clock identity."),
                ),
                ("faction_id", required(string(1, 128), "Faction identity.")),
                ("from", required(integer(0.0, 8.0), "Previous fill.")),
                ("note", required(string(1, 512), "Change note.")),
                ("segments", required(integer(1.0, 8.0), "Total segments.")),
                ("to", required(integer(0.0, 8.0), "New fill.")),
            ]),
        ),
        (
            "master_plan_advanced".to_owned(),
            object_type([
                ("from", required(integer(0.0, 8.0), "Previous fill.")),
                ("note", required(string(1, 512), "Change note.")),
                ("segments", required(integer(1.0, 8.0), "Total segments.")),
                ("to", required(integer(0.0, 8.0), "New fill.")),
            ]),
        ),
        (
            "raid_blowback".to_owned(),
            object_type([
                ("effect", required(effect_type(), "Blowback effect.")),
                ("note", required(string(1, 512), "Blowback note.")),
                ("position", required(position_type(), "Blowback position.")),
                ("raid_id", required(string(1, 128), "Raid identity.")),
                ("scene", required(string(0, 512), "Blowback scene summary.")),
                (
                    "severity",
                    required(integer(0.0, 3.0), "Blowback severity."),
                ),
            ]),
        ),
        (
            "raid_engagement_ready".to_owned(),
            object_type([
                (
                    "advantages",
                    required(list(string(1, 160), 0, 8), "Declared advantages."),
                ),
                (
                    "bonus",
                    required(integer(0.0, 3.0), "Suggested bonus dice."),
                ),
                (
                    "disadvantages",
                    required(list(string(1, 160), 0, 8), "Declared disadvantages."),
                ),
                ("dungeon_tier", required(integer(1.0, 4.0), "Dungeon tier.")),
                ("effect", required(effect_type(), "Starting effect.")),
                (
                    "faction_tier",
                    required(integer(1.0, 4.0), "Opposing faction tier."),
                ),
                ("position", required(position_type(), "Starting position.")),
                ("raid_id", required(string(1, 128), "Raid identity.")),
                ("scene", required(string(0, 512), "Scene summary.")),
                ("target_id", required(string(1, 128), "Target identity.")),
            ]),
        ),
        (
            "raid_loot_resolved".to_owned(),
            object_type([
                (
                    "gold_after",
                    required(integer(0.0, 999_999.0), "Dungeon gold after the raid."),
                ),
                ("loot", required(integer(0.0, 999_999.0), "Loot gained.")),
                (
                    "loot_total",
                    required(integer(0.0, 999_999.0), "Campaign loot total."),
                ),
                ("outcome", required(outcome_type(), "Action-loop outcome.")),
                ("raid_id", required(string(1, 128), "Raid identity.")),
                (
                    "reckless",
                    required(integer(0.0, 999.0), "Reckless points."),
                ),
                ("wicked", required(integer(0.0, 999.0), "Wicked points.")),
                (
                    "xp",
                    required(integer(0.0, 999_999.0), "Experience points."),
                ),
            ]),
        ),
        (
            "raid_planned".to_owned(),
            object_type([
                (
                    "advantages_count",
                    required(integer(0.0, 8.0), "Advantage count."),
                ),
                (
                    "disadvantages_count",
                    required(integer(0.0, 8.0), "Disadvantage count."),
                ),
                ("goal", required(string(0, 512), "Declared goal.")),
                ("plan", required(string(0, 512), "Declared plan.")),
                ("raid_id", required(string(1, 128), "Raid identity.")),
                ("target_id", required(string(1, 128), "Target identity.")),
            ]),
        ),
        (
            "reckless_advancement_spent".to_owned(),
            object_type([
                ("action", required(action_type(), "Advanced action.")),
                ("from", required(integer(0.0, 3.0), "Prior rating.")),
                ("note", required(string(1, 512), "Advancement note.")),
                (
                    "reckless_after",
                    required(integer(0.0, 999.0), "Reckless points after spending."),
                ),
                (
                    "stress_after",
                    required(integer(0.0, 9.0), "Stress after the reckless cost."),
                ),
                ("to", required(integer(0.0, 3.0), "New rating.")),
            ]),
        ),
        (
            "recovery_performed".to_owned(),
            object_type([
                ("amount", required(integer(1.0, 3.0), "Stress recovered.")),
                ("note", required(string(1, 512), "Recovery note.")),
                (
                    "stress_after",
                    required(integer(0.0, 9.0), "Stress after recovery."),
                ),
                (
                    "stress_before",
                    required(integer(0.0, 9.0), "Stress before recovery."),
                ),
            ]),
        ),
        (
            "upgrade_purchased".to_owned(),
            object_type([
                ("cost", required(integer(0.0, 999_999.0), "Gold paid.")),
                (
                    "gold_after",
                    required(integer(0.0, 999_999.0), "Gold after the purchase."),
                ),
                ("note", required(string(1, 512), "Purchase note.")),
                ("upgrade_id", required(string(1, 128), "Upgrade identity.")),
            ]),
        ),
        (
            "wicked_advancement_spent".to_owned(),
            object_type([
                ("ability", required(string(1, 128), "Gained ability.")),
                ("note", required(string(1, 512), "Advancement note.")),
                (
                    "wicked_after",
                    required(integer(0.0, 999.0), "Wicked points after spending."),
                ),
            ]),
        ),
    ])
}

fn calling_type() -> TypeExpression {
    symbol(["forgekeeper", "warbringer", "whisperer", "wildcaller"])
}

fn phase_type() -> TypeExpression {
    symbol(WICKED_ONES_PHASES)
}

fn action_type() -> TypeExpression {
    symbol([
        "analyze", "brawl", "command", "demolish", "endure", "invent", "perceive", "persuade",
        "sneak",
    ])
}

fn approach_type() -> TypeExpression {
    symbol(["force", "guile", "insight"])
}

fn position_type() -> TypeExpression {
    symbol(["controlled", "desperate", "risky"])
}

fn effect_type() -> TypeExpression {
    symbol(["great", "limited", "standard"])
}

fn condition_type() -> TypeExpression {
    symbol(["bloodied", "dead", "feral", "steady"])
}

fn outcome_type() -> TypeExpression {
    symbol(["critical", "failure", "mixed", "success"])
}

fn outcome_type_with_none() -> TypeExpression {
    symbol(["critical", "failure", "mixed", "none", "success"])
}

fn consequence_kind_type() -> TypeExpression {
    symbol(["complication", "harm", "lost_opportunity"])
}

fn consequence_severity_type() -> TypeExpression {
    symbol(["minor", "moderate", "none", "severe"])
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

fn public_event(kind: &str, payload: DomainValue) -> ResolverEvent {
    ResolverEvent {
        kind: kind.to_owned(),
        visibility: EventVisibility::Public,
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
