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

fn initial_state_value(starting_resources: u32) -> DomainValue {
    object([
        ("bloodied", DomainValue::Bool(false)),
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
            "apply_shock" => resolve_apply_shock(request, state)?,
            "death_knell" => resolve_death_knell(request, state)?,
            "flashback" => resolve_flashback(request, state)?,
            "gain_dark_heart" => resolve_gain_dark_heart(request, state)?,
            "go_feral" => resolve_go_feral(request, state)?,
            "resolve_action" => resolve_action(definition, request, state, entropy)?,
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

    let definition_fields = value_object("definition", definition)?;
    let actions = object_map(definition_fields, "actions")?;
    let rating = object_u32(actions, action.symbol())?;
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
        let resistance_rating = object_u32(actions, resistance_action.symbol())?;
        if resistance_rating > u32::from(MAX_ACTION_RATING) {
            return Err(invalid(
                "definition.actions",
                "stored resistance action rating is outside the reviewed range",
            ));
        }
        let resistance_pool = resistance_rating
            .saturating_add(resistance_bonus)
            .clamp(1, MAX_ACTION_POOL);
        resistance_dice = roll_dice(entropy, resistance_pool)?;
        let (resistance_best, resistance_sixes) = effective_best(&resistance_dice, false);
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
    let types = BTreeMap::from([
        ("GearItem".to_owned(), gear_item_type),
        ("ResistanceChoice".to_owned(), resistance_type),
        ("TeamworkContribution".to_owned(), teamwork_type),
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
    ])
}

fn calling_type() -> TypeExpression {
    symbol(["forgekeeper", "warbringer", "whisperer", "wildcaller"])
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
