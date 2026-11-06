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
//!
//! Encounter documents are host-authored and validated structurally. The caller must supply the
//! SHA-256 of the canonical validated document, which is checked before state is written.
//! Character-kind participants derive defense, attack score, and starting point pools from the
//! immutable character definition and current character state; their document combat fields are
//! not trusted. Adversary statistics, challenge definitions, ranges, turn order, rewards, labels,
//! and narrative text remain host-supplied and are validated for structure and references, not
//! for narrative truth. The host also chooses actions and explicit host-decision endings.

use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, schema_for};
use semver::Version;
use serde::{Deserialize, Serialize};
use url::Url;
use weave_domain::{DomainValue, FieldDeclaration, TypeExpression, validate_typed_value};

use crate::{
    ADAPTER_MANIFEST_FORMAT_VERSION, ADAPTER_STATE_FORMAT_VERSION, AdapterCharacterDefinition,
    AdapterExtensionSurface, AdapterManifest, AdapterProvenance, AdapterSourceClass, CreationField,
    CreationFieldAuthority, CreationStep, EntropyState, EntropyStream, EventVisibility,
    ExcludedMaterial, HostVisibilityPolicy, LicenseTextReference, RESOLVER_CONTRACT_VERSION,
    RESOLVER_FORMAT_VERSION, ResolutionRequest, ResolvedAdapter, ResolverEvent,
    ResolverOperationDeclaration, ResolverOutput, ResolverRegistry, TabletopCapability,
    TabletopCapabilityDeclaration, TabletopError, TabletopResolver, TabletopState,
    canonical_fingerprint, resolved_adapter, sha256_bytes, validate_adapter_manifest,
    validate_tabletop_state,
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

/// Adapter namespace shared by every Open Adventure entity.
pub const OPEN_ADVENTURE_NAMESPACE: &str = "org.weave.tabletop.open_adventure";
/// Versioned encounter authoring document.
pub const OPEN_ADVENTURE_ENCOUNTER_FORMAT_VERSION: u32 = 1;
/// Versioned compact runtime encounter projection.
pub const OPEN_ADVENTURE_RUNTIME_ENCOUNTER_FORMAT_VERSION: u32 = 1;
/// Versioned optional-module descriptor and selection format.
pub const OPEN_ADVENTURE_MODULE_FORMAT_VERSION: u32 = 1;
/// Versioned golden-adventure fixture format.
pub const OPEN_ADVENTURE_GOLDEN_FORMAT_VERSION: u32 = 1;

const MAX_ENCOUNTERS: usize = 16;
const MAX_ENCOUNTER_PARTICIPANTS: usize = 64;
const MAX_ENCOUNTER_RANGES: usize = 16;
const MAX_ENCOUNTER_CHALLENGES: usize = 64;
const MAX_ENCOUNTER_ADVERSARIES: usize = 64;
const MAX_ENCOUNTER_REWARDS: usize = 32;

/// Whether an encounter participant is a player character or an adversary.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureParticipantKind {
    Adversary,
    Character,
}

/// Deterministic ending recorded when an encounter concludes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureEncounterEnding {
    Defeat,
    HostDecision,
    Success,
    Withdrawal,
}

impl OpenAdventureEncounterEnding {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Defeat => "defeat",
            Self::HostDecision => "host_decision",
            Self::Success => "success",
            Self::Withdrawal => "withdrawal",
        }
    }
}

/// Ordered range band used by encounter participants and spells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureRange {
    pub id: String,
    pub label: String,
    pub order: u32,
}

/// One wound that can be applied when a pool is exhausted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureWound {
    pub id: String,
    pub label: String,
    pub target: OpenAdventurePool,
    pub severity: u32,
    pub effect: String,
}

/// One spell available to an encounter participant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureSpell {
    pub id: String,
    pub label: String,
    pub cost: u32,
    pub target: OpenAdventurePool,
    pub effect: u32,
    pub range: String,
}

/// Encounter equipment with explicit combat numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureEncounterEquipment {
    pub id: String,
    pub label: String,
    pub kind: OpenAdventureEquipmentKind,
    pub attack_bonus: i32,
    pub armor: u32,
}

/// Reusable adversary stat block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureAdversary {
    pub id: String,
    pub label: String,
    pub defense: u32,
    pub armor: u32,
    pub body_points: u32,
    pub mind_points: u32,
    pub spirit_points: u32,
    pub attack_bonus: i32,
    pub effect: u32,
    pub effect_pool: OpenAdventurePool,
    pub spells: Vec<OpenAdventureSpell>,
}

/// One encounter participant with embedded spells, wounds, and equipment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureEncounterParticipant {
    pub id: String,
    pub label: String,
    pub kind: OpenAdventureParticipantKind,
    pub range: String,
    pub attribute: OpenAdventureAttribute,
    pub defense: u32,
    pub armor: u32,
    pub body_points: u32,
    pub mind_points: u32,
    pub spirit_points: u32,
    pub attack_bonus: i32,
    pub effect: u32,
    pub effect_pool: OpenAdventurePool,
    pub story_points: u32,
    pub wounds: Vec<OpenAdventureWound>,
    pub equipment: Vec<OpenAdventureEncounterEquipment>,
    pub spells: Vec<OpenAdventureSpell>,
    /// Empty for characters; must reference an adversary for adversary participants.
    pub adversary_id: String,
}

/// One encounter challenge with explicit participants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureChallengeEntity {
    pub id: String,
    pub label: String,
    pub kind: OpenAdventureChallengeKind,
    pub difficulty_score: u32,
    pub challenge_points: u32,
    pub effect: u32,
    pub participant_ids: Vec<String>,
}

/// One encounter reward expressed as Story Points and experience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureReward {
    pub id: String,
    pub label: String,
    pub story_points: u32,
    pub experience: u32,
}

/// Authored success, failure, and withdrawal text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureEncounterOutcomes {
    pub success: String,
    pub failure: String,
    pub withdrawal: String,
}

/// Complete namespaced encounter authoring document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureEncounterDocument {
    pub document_format_version: u32,
    pub namespace: String,
    pub id: String,
    pub label: String,
    pub goal: String,
    pub outcomes: OpenAdventureEncounterOutcomes,
    pub ranges: Vec<OpenAdventureRange>,
    pub adversaries: Vec<OpenAdventureAdversary>,
    pub participants: Vec<OpenAdventureEncounterParticipant>,
    pub challenges: Vec<OpenAdventureChallengeEntity>,
    pub turn_order: Vec<String>,
    pub rewards: Vec<OpenAdventureReward>,
}

/// Compact runtime range; labels stay in the authoring document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureRuntimeRange {
    pub id: String,
    pub order: u32,
}

/// Compact runtime participant retaining every resolution input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureRuntimeParticipant {
    pub id: String,
    pub kind: OpenAdventureParticipantKind,
    pub range: String,
    pub defense: u32,
    pub armor: u32,
    pub body_points: u32,
    pub mind_points: u32,
    pub spirit_points: u32,
    pub attack_bonus: i32,
    pub effect: u32,
    pub effect_pool: OpenAdventurePool,
    pub spell_ids: Vec<String>,
    pub wound_ids: Vec<String>,
    pub equipment_ids: Vec<String>,
    pub adversary_id: String,
}

/// Compact runtime challenge without prose labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureRuntimeChallenge {
    pub id: String,
    pub kind: OpenAdventureChallengeKind,
    pub difficulty_score: u32,
    pub challenge_points: u32,
    pub effect: u32,
    pub participant_ids: Vec<String>,
}

/// Compact runtime reward without prose labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureRuntimeReward {
    pub id: String,
    pub story_points: u32,
    pub experience: u32,
}

/// Compact runtime projection of an encounter authoring document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureRuntimeEncounter {
    pub runtime_format_version: u32,
    pub namespace: String,
    pub id: String,
    pub label: String,
    pub goal: String,
    pub outcomes: OpenAdventureEncounterOutcomes,
    pub ranges: Vec<OpenAdventureRuntimeRange>,
    pub participants: Vec<OpenAdventureRuntimeParticipant>,
    pub challenges: Vec<OpenAdventureRuntimeChallenge>,
    pub turn_order: Vec<String>,
    pub rewards: Vec<OpenAdventureRuntimeReward>,
}

/// Validate one complete encounter document and every reference it contains.
pub fn validate_open_adventure_encounter_document(
    document: &OpenAdventureEncounterDocument,
) -> Result<(), TabletopError> {
    if document.document_format_version != OPEN_ADVENTURE_ENCOUNTER_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "document_format_version",
        });
    }
    if document.namespace != OPEN_ADVENTURE_NAMESPACE {
        return Err(invalid(
            "namespace",
            "expected the Open Adventure adapter namespace",
        ));
    }
    validate_local_identifier("id", &document.id)?;
    validate_bounded_text("label", &document.label, 1, 160)?;
    validate_bounded_text("goal", &document.goal, 1, 512)?;
    validate_bounded_text("outcomes.success", &document.outcomes.success, 1, 512)?;
    validate_bounded_text("outcomes.failure", &document.outcomes.failure, 1, 512)?;
    validate_bounded_text("outcomes.withdrawal", &document.outcomes.withdrawal, 1, 512)?;
    if document.ranges.len() > MAX_ENCOUNTER_RANGES {
        return Err(invalid("ranges", "too many encounter ranges"));
    }
    let mut range_ids = BTreeSet::new();
    let mut previous_order = None;
    for (index, range) in document.ranges.iter().enumerate() {
        validate_local_identifier(format!("ranges[{index}].id"), &range.id)?;
        validate_bounded_text(format!("ranges[{index}].label"), &range.label, 1, 160)?;
        if previous_order.is_some_and(|previous| previous >= range.order) {
            return Err(invalid(
                "ranges",
                "expected unique ranges ordered by ascending order",
            ));
        }
        previous_order = Some(range.order);
        if !range_ids.insert(range.id.as_str()) {
            return Err(invalid("ranges", "duplicate range identity"));
        }
    }
    if document.adversaries.len() > MAX_ENCOUNTER_ADVERSARIES {
        return Err(invalid("adversaries", "too many encounter adversaries"));
    }
    let mut adversaries = BTreeMap::new();
    let mut previous = None;
    for (index, adversary) in document.adversaries.iter().enumerate() {
        validate_local_identifier(format!("adversaries[{index}].id"), &adversary.id)?;
        if previous.is_some_and(|previous| previous >= adversary.id.as_str()) {
            return Err(invalid(
                "adversaries",
                "expected unique adversaries ordered by id",
            ));
        }
        previous = Some(adversary.id.as_str());
        validate_bounded_text(
            format!("adversaries[{index}].label"),
            &adversary.label,
            1,
            160,
        )?;
        if !(1..=1_000).contains(&adversary.defense) {
            return Err(invalid(
                format!("adversaries[{index}].defense"),
                "expected a defense from 1 through 1000",
            ));
        }
        validate_encounter_stat_block(
            &format!("adversaries[{index}]"),
            adversary.armor,
            adversary.body_points,
            adversary.mind_points,
            adversary.spirit_points,
            adversary.attack_bonus,
            adversary.effect,
        )?;
        validate_encounter_spells(
            &format!("adversaries[{index}].spells"),
            &adversary.spells,
            &range_ids,
        )?;
        adversaries.insert(adversary.id.as_str(), adversary);
    }
    if document.participants.len() > MAX_ENCOUNTER_PARTICIPANTS {
        return Err(invalid("participants", "too many encounter participants"));
    }
    let mut participant_ids = BTreeSet::new();
    let mut previous = None;
    for (index, participant) in document.participants.iter().enumerate() {
        validate_local_identifier(format!("participants[{index}].id"), &participant.id)?;
        if previous.is_some_and(|previous| previous >= participant.id.as_str()) {
            return Err(invalid(
                "participants",
                "expected unique participants ordered by id",
            ));
        }
        previous = Some(participant.id.as_str());
        validate_bounded_text(
            format!("participants[{index}].label"),
            &participant.label,
            1,
            160,
        )?;
        if !range_ids.contains(participant.range.as_str()) {
            return Err(invalid(
                format!("participants[{index}].range"),
                "range reference does not exist",
            ));
        }
        if !(1..=1_000).contains(&participant.defense) {
            return Err(invalid(
                format!("participants[{index}].defense"),
                "expected a defense from 1 through 1000",
            ));
        }
        validate_encounter_stat_block(
            &format!("participants[{index}]"),
            participant.armor,
            participant.body_points,
            participant.mind_points,
            participant.spirit_points,
            participant.attack_bonus,
            participant.effect,
        )?;
        if participant.story_points > 1_000 {
            return Err(invalid(
                format!("participants[{index}].story_points"),
                "expected at most 1000 Story Points",
            ));
        }
        validate_encounter_wounds(
            &format!("participants[{index}].wounds"),
            &participant.wounds,
        )?;
        validate_encounter_equipment(
            &format!("participants[{index}].equipment"),
            &participant.equipment,
        )?;
        validate_encounter_spells(
            &format!("participants[{index}].spells"),
            &participant.spells,
            &range_ids,
        )?;
        match participant.kind {
            OpenAdventureParticipantKind::Character => {
                if !participant.adversary_id.is_empty() {
                    return Err(invalid(
                        format!("participants[{index}].adversary_id"),
                        "character participants cannot reference an adversary",
                    ));
                }
            }
            OpenAdventureParticipantKind::Adversary => {
                let Some(adversary) = adversaries.get(participant.adversary_id.as_str()) else {
                    return Err(invalid(
                        format!("participants[{index}].adversary_id"),
                        "adversary reference does not exist",
                    ));
                };
                if participant.defense != adversary.defense
                    || participant.armor != adversary.armor
                    || participant.body_points != adversary.body_points
                    || participant.mind_points != adversary.mind_points
                    || participant.spirit_points != adversary.spirit_points
                    || participant.attack_bonus != adversary.attack_bonus
                    || participant.effect != adversary.effect
                    || participant.effect_pool != adversary.effect_pool
                {
                    return Err(invalid(
                        format!("participants[{index}].adversary_id"),
                        "adversary reference has incompatible combat statistics",
                    ));
                }
            }
        }
        if !participant_ids.insert(participant.id.as_str()) {
            return Err(invalid("participants", "duplicate participant identity"));
        }
    }
    if document.challenges.len() > MAX_ENCOUNTER_CHALLENGES {
        return Err(invalid("challenges", "too many encounter challenges"));
    }
    let mut previous = None;
    for (index, challenge) in document.challenges.iter().enumerate() {
        validate_local_identifier(format!("challenges[{index}].id"), &challenge.id)?;
        if previous.is_some_and(|previous| previous >= challenge.id.as_str()) {
            return Err(invalid(
                "challenges",
                "expected unique challenges ordered by id",
            ));
        }
        previous = Some(challenge.id.as_str());
        validate_bounded_text(
            format!("challenges[{index}].label"),
            &challenge.label,
            1,
            160,
        )?;
        if !(1..=100).contains(&challenge.difficulty_score) {
            return Err(invalid(
                format!("challenges[{index}].difficulty_score"),
                "expected a Difficulty Score from 1 through 100",
            ));
        }
        if !(1..=1_000_000).contains(&challenge.challenge_points) {
            return Err(invalid(
                format!("challenges[{index}].challenge_points"),
                "expected Challenge Points from 1 through 1000000",
            ));
        }
        if !(1..=1_000).contains(&challenge.effect) {
            return Err(invalid(
                format!("challenges[{index}].effect"),
                "expected an Effect from 1 through 1000",
            ));
        }
        let mut previous_participant = None;
        for (position, participant_id) in challenge.participant_ids.iter().enumerate() {
            if previous_participant.is_some_and(|previous| previous >= participant_id.as_str()) {
                return Err(invalid(
                    format!("challenges[{index}].participant_ids"),
                    "expected unique participant references ordered by id",
                ));
            }
            previous_participant = Some(participant_id.as_str());
            if !participant_ids.contains(participant_id.as_str()) {
                return Err(invalid(
                    format!("challenges[{index}].participant_ids[{position}]"),
                    "participant reference does not exist",
                ));
            }
        }
    }
    if document.turn_order.len() != document.participants.len() {
        return Err(invalid(
            "turn_order",
            "turn order must name every participant exactly once",
        ));
    }
    let mut seen_turns = BTreeSet::new();
    for (index, participant_id) in document.turn_order.iter().enumerate() {
        if !participant_ids.contains(participant_id.as_str()) {
            return Err(invalid(
                format!("turn_order[{index}]"),
                "participant reference does not exist",
            ));
        }
        if !seen_turns.insert(participant_id.as_str()) {
            return Err(invalid("turn_order", "duplicate participant reference"));
        }
    }
    if document.rewards.len() > MAX_ENCOUNTER_REWARDS {
        return Err(invalid("rewards", "too many encounter rewards"));
    }
    let mut previous = None;
    for (index, reward) in document.rewards.iter().enumerate() {
        validate_local_identifier(format!("rewards[{index}].id"), &reward.id)?;
        if previous.is_some_and(|previous| previous >= reward.id.as_str()) {
            return Err(invalid("rewards", "expected unique rewards ordered by id"));
        }
        previous = Some(reward.id.as_str());
        validate_bounded_text(format!("rewards[{index}].label"), &reward.label, 1, 160)?;
        if reward.story_points > 1_000_000 || reward.experience > 1_000_000 {
            return Err(invalid(
                format!("rewards[{index}]"),
                "reward values exceed the reviewed bounds",
            ));
        }
    }
    Ok(())
}

fn validate_encounter_stat_block(
    path: &str,
    armor: u32,
    body_points: u32,
    mind_points: u32,
    spirit_points: u32,
    attack_bonus: i32,
    effect: u32,
) -> Result<(), TabletopError> {
    if armor > 1_000 {
        return Err(invalid(
            format!("{path}.armor"),
            "expected at most 1000 armor",
        ));
    }
    for (field, value) in [
        ("body_points", body_points),
        ("mind_points", mind_points),
        ("spirit_points", spirit_points),
    ] {
        if value > 1_000 {
            return Err(invalid(
                format!("{path}.{field}"),
                "expected at most 1000 points",
            ));
        }
    }
    if !(-100..=100).contains(&attack_bonus) {
        return Err(invalid(
            format!("{path}.attack_bonus"),
            "expected an attack bonus from -100 through 100",
        ));
    }
    if !(1..=1_000).contains(&effect) {
        return Err(invalid(
            format!("{path}.effect"),
            "expected an Effect from 1 through 1000",
        ));
    }
    Ok(())
}

fn validate_encounter_spells(
    path: &str,
    spells: &[OpenAdventureSpell],
    range_ids: &BTreeSet<&str>,
) -> Result<(), TabletopError> {
    if spells.len() > MAX_DETAIL_ENTRIES {
        return Err(invalid(path, "too many spells"));
    }
    let mut previous = None;
    for (index, spell) in spells.iter().enumerate() {
        validate_local_identifier(format!("{path}[{index}].id"), &spell.id)?;
        if previous.is_some_and(|previous| previous >= spell.id.as_str()) {
            return Err(invalid(path, "expected unique spells ordered by id"));
        }
        previous = Some(spell.id.as_str());
        validate_bounded_text(format!("{path}[{index}].label"), &spell.label, 1, 160)?;
        if !(1..=1_000).contains(&spell.cost) {
            return Err(invalid(
                format!("{path}[{index}].cost"),
                "expected a spell cost from 1 through 1000",
            ));
        }
        if !(1..=1_000).contains(&spell.effect) {
            return Err(invalid(
                format!("{path}[{index}].effect"),
                "expected a spell effect from 1 through 1000",
            ));
        }
        if !range_ids.contains(spell.range.as_str()) {
            return Err(invalid(
                format!("{path}[{index}].range"),
                "range reference does not exist",
            ));
        }
    }
    Ok(())
}

fn validate_encounter_wounds(
    path: &str,
    wounds: &[OpenAdventureWound],
) -> Result<(), TabletopError> {
    if wounds.len() > MAX_DETAIL_ENTRIES {
        return Err(invalid(path, "too many wounds"));
    }
    let mut previous = None;
    for (index, wound) in wounds.iter().enumerate() {
        validate_local_identifier(format!("{path}[{index}].id"), &wound.id)?;
        if previous.is_some_and(|previous| previous >= wound.id.as_str()) {
            return Err(invalid(path, "expected unique wounds ordered by id"));
        }
        previous = Some(wound.id.as_str());
        validate_bounded_text(format!("{path}[{index}].label"), &wound.label, 1, 160)?;
        if !(1..=10).contains(&wound.severity) {
            return Err(invalid(
                format!("{path}[{index}].severity"),
                "expected a wound severity from 1 through 10",
            ));
        }
        validate_bounded_text(format!("{path}[{index}].effect"), &wound.effect, 1, 240)?;
    }
    Ok(())
}

fn validate_encounter_equipment(
    path: &str,
    equipment: &[OpenAdventureEncounterEquipment],
) -> Result<(), TabletopError> {
    if equipment.len() > MAX_DETAIL_ENTRIES {
        return Err(invalid(path, "too many equipment entries"));
    }
    let mut previous = None;
    for (index, item) in equipment.iter().enumerate() {
        validate_local_identifier(format!("{path}[{index}].id"), &item.id)?;
        if previous.is_some_and(|previous| previous >= item.id.as_str()) {
            return Err(invalid(path, "expected unique equipment ordered by id"));
        }
        previous = Some(item.id.as_str());
        validate_bounded_text(format!("{path}[{index}].label"), &item.label, 1, 160)?;
        if !(-100..=100).contains(&item.attack_bonus) {
            return Err(invalid(
                format!("{path}[{index}].attack_bonus"),
                "expected an equipment bonus from -100 through 100",
            ));
        }
        if item.armor > 1_000 {
            return Err(invalid(
                format!("{path}[{index}].armor"),
                "expected at most 1000 armor",
            ));
        }
    }
    Ok(())
}

/// Project an authoring document to its compact runtime form.
pub fn project_open_adventure_runtime_encounter(
    document: &OpenAdventureEncounterDocument,
) -> Result<OpenAdventureRuntimeEncounter, TabletopError> {
    validate_open_adventure_encounter_document(document)?;
    Ok(OpenAdventureRuntimeEncounter {
        runtime_format_version: OPEN_ADVENTURE_RUNTIME_ENCOUNTER_FORMAT_VERSION,
        namespace: document.namespace.clone(),
        id: document.id.clone(),
        label: document.label.clone(),
        goal: document.goal.clone(),
        outcomes: document.outcomes.clone(),
        ranges: document
            .ranges
            .iter()
            .map(|range| OpenAdventureRuntimeRange {
                id: range.id.clone(),
                order: range.order,
            })
            .collect(),
        participants: document
            .participants
            .iter()
            .map(|participant| OpenAdventureRuntimeParticipant {
                id: participant.id.clone(),
                kind: participant.kind,
                range: participant.range.clone(),
                defense: participant.defense,
                armor: participant.armor,
                body_points: participant.body_points,
                mind_points: participant.mind_points,
                spirit_points: participant.spirit_points,
                attack_bonus: participant.attack_bonus,
                effect: participant.effect,
                effect_pool: participant.effect_pool,
                spell_ids: participant
                    .spells
                    .iter()
                    .map(|spell| spell.id.clone())
                    .collect(),
                wound_ids: participant
                    .wounds
                    .iter()
                    .map(|wound| wound.id.clone())
                    .collect(),
                equipment_ids: participant
                    .equipment
                    .iter()
                    .map(|item| item.id.clone())
                    .collect(),
                adversary_id: participant.adversary_id.clone(),
            })
            .collect(),
        challenges: document
            .challenges
            .iter()
            .map(|challenge| OpenAdventureRuntimeChallenge {
                id: challenge.id.clone(),
                kind: challenge.kind,
                difficulty_score: challenge.difficulty_score,
                challenge_points: challenge.challenge_points,
                effect: challenge.effect,
                participant_ids: challenge.participant_ids.clone(),
            })
            .collect(),
        turn_order: document.turn_order.clone(),
        rewards: document
            .rewards
            .iter()
            .map(|reward| OpenAdventureRuntimeReward {
                id: reward.id.clone(),
                story_points: reward.story_points,
                experience: reward.experience,
            })
            .collect(),
    })
}

/// Validate a standalone compact runtime encounter.
pub fn validate_open_adventure_runtime_encounter(
    encounter: &OpenAdventureRuntimeEncounter,
) -> Result<(), TabletopError> {
    if encounter.runtime_format_version != OPEN_ADVENTURE_RUNTIME_ENCOUNTER_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "runtime_format_version",
        });
    }
    if encounter.namespace != OPEN_ADVENTURE_NAMESPACE {
        return Err(invalid(
            "namespace",
            "expected the Open Adventure adapter namespace",
        ));
    }
    validate_local_identifier("id", &encounter.id)?;
    validate_bounded_text("label", &encounter.label, 1, 160)?;
    validate_bounded_text("goal", &encounter.goal, 1, 512)?;
    validate_bounded_text("outcomes.success", &encounter.outcomes.success, 1, 512)?;
    validate_bounded_text("outcomes.failure", &encounter.outcomes.failure, 1, 512)?;
    validate_bounded_text(
        "outcomes.withdrawal",
        &encounter.outcomes.withdrawal,
        1,
        512,
    )?;
    if encounter.ranges.len() > MAX_ENCOUNTER_RANGES {
        return Err(invalid("ranges", "too many encounter ranges"));
    }
    let mut range_ids = BTreeSet::new();
    let mut previous_order = None;
    for (index, range) in encounter.ranges.iter().enumerate() {
        validate_local_identifier(format!("ranges[{index}].id"), &range.id)?;
        if previous_order.is_some_and(|previous| previous >= range.order) {
            return Err(invalid(
                "ranges",
                "expected unique ranges ordered by ascending order",
            ));
        }
        previous_order = Some(range.order);
        if !range_ids.insert(range.id.as_str()) {
            return Err(invalid("ranges", "duplicate range identity"));
        }
    }
    if encounter.participants.len() > MAX_ENCOUNTER_PARTICIPANTS {
        return Err(invalid("participants", "too many encounter participants"));
    }
    let mut participant_ids = BTreeSet::new();
    let mut previous = None;
    for (index, participant) in encounter.participants.iter().enumerate() {
        validate_local_identifier(format!("participants[{index}].id"), &participant.id)?;
        if previous.is_some_and(|previous| previous >= participant.id.as_str()) {
            return Err(invalid(
                "participants",
                "expected unique participants ordered by id",
            ));
        }
        previous = Some(participant.id.as_str());
        if !range_ids.contains(participant.range.as_str()) {
            return Err(invalid(
                format!("participants[{index}].range"),
                "range reference does not exist",
            ));
        }
        validate_encounter_stat_block(
            &format!("participants[{index}]"),
            participant.armor,
            participant.body_points,
            participant.mind_points,
            participant.spirit_points,
            participant.attack_bonus,
            participant.effect,
        )?;
        if !(1..=1_000).contains(&participant.defense) {
            return Err(invalid(
                format!("participants[{index}].defense"),
                "expected a defense from 1 through 1000",
            ));
        }
        validate_identifier_list(
            &format!("participants[{index}].spell_ids"),
            &participant.spell_ids,
        )?;
        validate_identifier_list(
            &format!("participants[{index}].wound_ids"),
            &participant.wound_ids,
        )?;
        validate_identifier_list(
            &format!("participants[{index}].equipment_ids"),
            &participant.equipment_ids,
        )?;
        if participant.kind == OpenAdventureParticipantKind::Character
            && !participant.adversary_id.is_empty()
        {
            return Err(invalid(
                format!("participants[{index}].adversary_id"),
                "character participants cannot reference an adversary",
            ));
        }
        if !participant_ids.insert(participant.id.as_str()) {
            return Err(invalid("participants", "duplicate participant identity"));
        }
    }
    if encounter.challenges.len() > MAX_ENCOUNTER_CHALLENGES {
        return Err(invalid("challenges", "too many encounter challenges"));
    }
    let mut previous = None;
    for (index, challenge) in encounter.challenges.iter().enumerate() {
        validate_local_identifier(format!("challenges[{index}].id"), &challenge.id)?;
        if previous.is_some_and(|previous| previous >= challenge.id.as_str()) {
            return Err(invalid(
                "challenges",
                "expected unique challenges ordered by id",
            ));
        }
        previous = Some(challenge.id.as_str());
        if !(1..=100).contains(&challenge.difficulty_score)
            || !(1..=1_000_000).contains(&challenge.challenge_points)
            || !(1..=1_000).contains(&challenge.effect)
        {
            return Err(invalid(
                format!("challenges[{index}]"),
                "challenge values are outside the reviewed bounds",
            ));
        }
        let mut previous_participant = None;
        for (position, participant_id) in challenge.participant_ids.iter().enumerate() {
            if previous_participant.is_some_and(|previous| previous >= participant_id.as_str()) {
                return Err(invalid(
                    format!("challenges[{index}].participant_ids"),
                    "expected unique participant references ordered by id",
                ));
            }
            previous_participant = Some(participant_id.as_str());
            if !participant_ids.contains(participant_id.as_str()) {
                return Err(invalid(
                    format!("challenges[{index}].participant_ids[{position}]"),
                    "participant reference does not exist",
                ));
            }
        }
    }
    if encounter.turn_order.len() != encounter.participants.len() {
        return Err(invalid(
            "turn_order",
            "turn order must name every participant exactly once",
        ));
    }
    let mut seen_turns = BTreeSet::new();
    for (index, participant_id) in encounter.turn_order.iter().enumerate() {
        if !participant_ids.contains(participant_id.as_str()) {
            return Err(invalid(
                format!("turn_order[{index}]"),
                "participant reference does not exist",
            ));
        }
        if !seen_turns.insert(participant_id.as_str()) {
            return Err(invalid("turn_order", "duplicate participant reference"));
        }
    }
    if encounter.rewards.len() > MAX_ENCOUNTER_REWARDS {
        return Err(invalid("rewards", "too many encounter rewards"));
    }
    let mut previous = None;
    for (index, reward) in encounter.rewards.iter().enumerate() {
        validate_local_identifier(format!("rewards[{index}].id"), &reward.id)?;
        if previous.is_some_and(|previous| previous >= reward.id.as_str()) {
            return Err(invalid("rewards", "expected unique rewards ordered by id"));
        }
        previous = Some(reward.id.as_str());
        if reward.story_points > 1_000_000 || reward.experience > 1_000_000 {
            return Err(invalid(
                format!("rewards[{index}]"),
                "reward values exceed the reviewed bounds",
            ));
        }
    }
    Ok(())
}

fn validate_identifier_list(path: &str, values: &[String]) -> Result<(), TabletopError> {
    let mut previous = None;
    for (index, value) in values.iter().enumerate() {
        validate_local_identifier(format!("{path}[{index}]"), value)?;
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(invalid(path, "expected unique identities ordered by id"));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

/// Versioned optional-module selection artifact.
pub const OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION: u32 = 1;

/// Whether an optional module is verified official Community-Owned Content.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureModuleContentClass {
    Excluded,
    OfficialCommunityOwned,
}

/// Experience profile controlling which optional modules are offered.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenAdventureModuleExperience {
    Core,
    Extended,
}

/// One independently verified optional module descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureModuleDescriptor {
    pub module_format_version: u32,
    pub id: String,
    pub version: String,
    pub namespace: String,
    pub title: String,
    pub scope: String,
    pub experience: OpenAdventureModuleExperience,
    pub provides_capabilities: Vec<TabletopCapability>,
    pub replaces_capabilities: Vec<TabletopCapability>,
    pub requires_modules: Vec<String>,
    pub incompatible_modules: Vec<String>,
    pub source_url: String,
    pub license: String,
    pub content_class: OpenAdventureModuleContentClass,
    pub sha256: String,
}

/// Persisted activation set for optional modules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureModuleSelection {
    pub selection_format_version: u32,
    pub experience: OpenAdventureModuleExperience,
    pub enabled_modules: Vec<String>,
}

/// One actionable optional-module diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureModuleDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

/// One capability explicitly replaced by a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureCapabilityReplacement {
    pub capability: TabletopCapability,
    pub module_id: String,
}

/// Effective capability set after applying one module selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureEffectiveModules {
    pub selection_format_version: u32,
    pub experience: OpenAdventureModuleExperience,
    pub enabled_modules: Vec<String>,
    pub capabilities: Vec<TabletopCapability>,
    pub replaced_capabilities: Vec<OpenAdventureCapabilityReplacement>,
}

/// Exact bytes of one optional module's reviewed source record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureModuleArtifact {
    pub module_id: String,
    pub bytes: Vec<u8>,
}

/// One optional module explicitly reviewed and allowlisted by this adapter release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAdventureVerifiedModule {
    pub id: &'static str,
    pub version: &'static str,
    pub sha256: &'static str,
    pub provides_capabilities: &'static [TabletopCapability],
    pub replaces_capabilities: &'static [TabletopCapability],
}

/// Exact synthetic module artifact bytes used by the golden fixture and tests.
pub const OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES: &[u8] =
    b"open-adventure synthetic tide-clocks module source record\n";
/// SHA-256 of [`OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES`].
pub const OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256: &str =
    "9bf7b964fadf02b9091a0d4d6079abf3a67fcfbeefaca1dc7720a782f7dabe85";
/// Exact synthetic replacement-module artifact bytes.
pub const OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_BYTES: &[u8] =
    b"open-adventure synthetic tide-replacement module source record\n";
/// SHA-256 of [`OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_BYTES`].
pub const OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_SHA256: &str =
    "1b53de57a9cff0e8e1cc0a7e58e404e4f4bc10e2c5e42ac7e7552ccc151a3c75";

/// Checked-in allowlist of optional modules this release may activate.
///
/// A descriptor must match an entry exactly: identity, version, artifact digest, provided
/// capabilities, and explicit replacements. Descriptors outside this table never grant
/// capabilities, even when they self-assert official CC0 content.
pub const OPEN_ADVENTURE_VERIFIED_MODULES: &[OpenAdventureVerifiedModule] = &[
    OpenAdventureVerifiedModule {
        id: "org.weave.tabletop.open_adventure.tide_clocks",
        version: "1.0.0",
        sha256: OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256,
        provides_capabilities: &[TabletopCapability::Clocks],
        replaces_capabilities: &[],
    },
    OpenAdventureVerifiedModule {
        id: "org.weave.tabletop.open_adventure.tide_replacement",
        version: "1.0.0",
        sha256: OPEN_ADVENTURE_REPLACEMENT_ARTIFACT_SHA256,
        provides_capabilities: &[TabletopCapability::ChecksAndConflicts],
        replaces_capabilities: &[TabletopCapability::ChecksAndConflicts],
    },
];

/// Apply one persisted module selection to the core manifest.
///
/// Every module is versioned, namespaced, ordered, and provenance-checked. A module may add a
/// capability that the core does not declare, or replace a core capability only by naming it in
/// `replaces_capabilities`. Unknown, unverified, incompatible, or silently shadowing modules are
/// refused with actionable diagnostics. The descriptor digest must match the supplied artifact
/// bytes, and only descriptors matching [`OPEN_ADVENTURE_VERIFIED_MODULES`] may grant capabilities.
pub fn apply_open_adventure_modules(
    available: &[OpenAdventureModuleDescriptor],
    artifacts: &[OpenAdventureModuleArtifact],
    selection: &OpenAdventureModuleSelection,
    core_manifest: &AdapterManifest,
) -> Result<OpenAdventureEffectiveModules, Vec<OpenAdventureModuleDiagnostic>> {
    let mut diagnostics = Vec::new();
    if selection.selection_format_version != OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION {
        diagnostics.push(module_diagnostic(
            "OA200",
            "selection.selection_format_version",
            "unsupported module selection format version",
        ));
    }
    if !strictly_sorted_strings(&selection.enabled_modules) {
        diagnostics.push(module_diagnostic(
            "OA200",
            "selection.enabled_modules",
            "expected unique enabled module identities in sorted order",
        ));
    }
    let mut modules: BTreeMap<String, &OpenAdventureModuleDescriptor> = BTreeMap::new();
    for (index, module) in available.iter().enumerate() {
        match validate_module_descriptor(index, module) {
            Ok(()) => {
                if modules.insert(module.id.clone(), module).is_some() {
                    diagnostics.push(module_diagnostic(
                        "OA209",
                        format!("modules[{index}].id"),
                        format!("duplicate module identity `{}`", module.id),
                    ));
                }
            }
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    let mut enabled = BTreeSet::new();
    for (index, id) in selection.enabled_modules.iter().enumerate() {
        if !is_global_identifier(id) {
            diagnostics.push(module_diagnostic(
                "OA200",
                format!("selection.enabled_modules[{index}]"),
                "expected a namespaced lowercase module identity",
            ));
            continue;
        }
        if !modules.contains_key(id) {
            diagnostics.push(module_diagnostic(
                "OA202",
                format!("selection.enabled_modules[{index}]"),
                format!("enabled module `{id}` is not available"),
            ));
            continue;
        }
        if !enabled.insert(id.clone()) {
            diagnostics.push(module_diagnostic(
                "OA200",
                format!("selection.enabled_modules[{index}]"),
                "duplicate enabled module identity",
            ));
        }
    }
    for id in &enabled {
        let Some(module) = modules.get(id) else {
            continue;
        };
        if selection.experience == OpenAdventureModuleExperience::Core
            && module.experience != OpenAdventureModuleExperience::Core
        {
            diagnostics.push(module_diagnostic(
                "OA208",
                "selection.experience",
                format!("module `{id}` is not available in the Core experience"),
            ));
        }
        for (index, required) in module.requires_modules.iter().enumerate() {
            if !enabled.contains(required) {
                diagnostics.push(module_diagnostic(
                    "OA203",
                    format!("modules.{id}.requires_modules[{index}]"),
                    format!("module `{id}` requires `{required}`, which is not enabled"),
                ));
            }
        }
        for (index, incompatible) in module.incompatible_modules.iter().enumerate() {
            if enabled.contains(incompatible) {
                diagnostics.push(module_diagnostic(
                    "OA204",
                    format!("modules.{id}.incompatible_modules[{index}]"),
                    format!("modules `{id}` and `{incompatible}` are incompatible"),
                ));
            }
        }
    }
    let mut verified_modules = BTreeSet::new();
    for id in &enabled {
        let Some(module) = modules.get(id) else {
            continue;
        };
        let matching: Vec<&OpenAdventureModuleArtifact> = artifacts
            .iter()
            .filter(|artifact| &artifact.module_id == id)
            .collect();
        if matching.len() != 1 {
            diagnostics.push(module_diagnostic(
                "OA211",
                format!("artifacts.{id}"),
                format!("expected exactly one artifact for module `{id}`"),
            ));
        } else {
            let actual = sha256_bytes(&matching[0].bytes);
            if actual != module.sha256 {
                diagnostics.push(module_diagnostic(
                    "OA211",
                    format!("artifacts.{id}"),
                    format!(
                        "module artifact digest `{actual}` does not match descriptor `{}`",
                        module.sha256
                    ),
                ));
            }
        }
        let Some(verified) = OPEN_ADVENTURE_VERIFIED_MODULES
            .iter()
            .find(|entry| entry.id == id.as_str())
        else {
            diagnostics.push(module_diagnostic(
                "OA210",
                format!("modules.{id}"),
                format!("module `{id}` is not on the verified allowlist"),
            ));
            continue;
        };
        if verified.version != module.version.as_str()
            || verified.sha256 != module.sha256.as_str()
            || verified.provides_capabilities != module.provides_capabilities.as_slice()
            || verified.replaces_capabilities != module.replaces_capabilities.as_slice()
        {
            diagnostics.push(module_diagnostic(
                "OA210",
                format!("modules.{id}"),
                format!("module `{id}` does not match its verified allowlist entry"),
            ));
            continue;
        }
        verified_modules.insert(id.clone());
    }
    let core_capabilities: BTreeSet<TabletopCapability> = core_manifest
        .capabilities
        .iter()
        .filter(|declaration| declaration.version == 1)
        .map(|declaration| declaration.capability)
        .collect();
    let mut effective = core_capabilities.clone();
    let mut replaced: Vec<(TabletopCapability, String)> = Vec::new();
    let mut providers: BTreeMap<TabletopCapability, String> = BTreeMap::new();
    for id in &enabled {
        let Some(module) = modules.get(id) else {
            continue;
        };
        let verified = verified_modules.contains(id);
        for capability in &module.provides_capabilities {
            if core_capabilities.contains(capability)
                && !module.replaces_capabilities.contains(capability)
            {
                diagnostics.push(module_diagnostic(
                    "OA205",
                    format!("modules.{id}.provides_capabilities"),
                    format!(
                        "module `{id}` provides core capability `{capability:?}` without an explicit replacement"
                    ),
                ));
                continue;
            }
            if verified {
                if let Some(existing) = providers.insert(*capability, id.clone()) {
                    diagnostics.push(module_diagnostic(
                        "OA207",
                        format!("modules.{id}.provides_capabilities"),
                        format!(
                            "modules `{existing}` and `{id}` both provide capability `{capability:?}`"
                        ),
                    ));
                }
                effective.insert(*capability);
            }
        }
        if verified {
            for capability in &module.replaces_capabilities {
                if let Some(existing) = replaced
                    .iter()
                    .find(|(existing, _)| existing == capability)
                    .map(|(_, module_id)| module_id.clone())
                {
                    diagnostics.push(module_diagnostic(
                        "OA207",
                        format!("modules.{id}.replaces_capabilities"),
                        format!(
                            "modules `{existing}` and `{id}` both replace capability `{capability:?}`"
                        ),
                    ));
                } else {
                    replaced.push((*capability, id.clone()));
                }
            }
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let replaced_capabilities = replaced
        .into_iter()
        .map(
            |(capability, module_id)| OpenAdventureCapabilityReplacement {
                capability,
                module_id,
            },
        )
        .collect();
    Ok(OpenAdventureEffectiveModules {
        selection_format_version: OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION,
        experience: selection.experience,
        enabled_modules: selection.enabled_modules.clone(),
        capabilities: effective.into_iter().collect(),
        replaced_capabilities,
    })
}

/// List only verified modules appropriate to one experience profile.
#[must_use]
pub fn open_adventure_module_options(
    available: &[OpenAdventureModuleDescriptor],
    experience: OpenAdventureModuleExperience,
) -> Vec<OpenAdventureModuleDescriptor> {
    let mut options: Vec<OpenAdventureModuleDescriptor> = available
        .iter()
        .filter(|module| {
            module.content_class == OpenAdventureModuleContentClass::OfficialCommunityOwned
                && (experience == OpenAdventureModuleExperience::Extended
                    || module.experience == OpenAdventureModuleExperience::Core)
        })
        .cloned()
        .collect();
    options.sort_by(|left, right| left.id.cmp(&right.id));
    options
}

fn validate_module_descriptor(
    index: usize,
    module: &OpenAdventureModuleDescriptor,
) -> Result<(), OpenAdventureModuleDiagnostic> {
    let path = format!("modules[{index}]");
    if module.module_format_version != OPEN_ADVENTURE_MODULE_FORMAT_VERSION {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.module_format_version"),
            "unsupported module format version",
        ));
    }
    let prefix = format!("{OPEN_ADVENTURE_NAMESPACE}.");
    if !module.id.starts_with(&prefix) || !is_global_identifier(&module.id) {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.id"),
            "expected a namespaced module identity under the Open Adventure adapter",
        ));
    }
    if Version::parse(&module.version).is_err() {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.version"),
            "expected a semantic module version",
        ));
    }
    if module.namespace != OPEN_ADVENTURE_NAMESPACE {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.namespace"),
            "expected the Open Adventure adapter namespace",
        ));
    }
    if !is_bounded_text(&module.title, 1, 160) {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.title"),
            "module title is outside allowed bounds",
        ));
    }
    if !is_bounded_text(&module.scope, 1, 512) {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.scope"),
            "module scope is outside allowed bounds",
        ));
    }
    if module.content_class != OpenAdventureModuleContentClass::OfficialCommunityOwned {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.content_class"),
            "module is not verified official Community-Owned Content",
        ));
    }
    if module.license != OPEN_ADVENTURE_LICENSE_STATEMENT
        && module.license != OPEN_ADVENTURE_LICENSE_ALLOWLIST_TOKEN
    {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.license"),
            "module licence is outside the CC0 Community-Owned Content boundary",
        ));
    }
    if module.sha256.len() != 64
        || !module
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.sha256"),
            "expected a lowercase SHA-256 provenance digest",
        ));
    }
    let url = Url::parse(&module.source_url).map_err(|_| {
        module_diagnostic(
            "OA201",
            format!("{path}.source_url"),
            "expected a public HTTPS source URL on the allowlisted publisher origin",
        )
    })?;
    if url.scheme() != "https"
        || url.host_str() != Some("openadventurerpg.com")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.source_url"),
            "expected a public HTTPS source URL on the allowlisted publisher origin",
        ));
    }
    if !strictly_sorted_capabilities(&module.provides_capabilities) {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.provides_capabilities"),
            "expected unique capability declarations in sorted order",
        ));
    }
    if !strictly_sorted_capabilities(&module.replaces_capabilities) {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.replaces_capabilities"),
            "expected unique capability replacements in sorted order",
        ));
    }
    if !module
        .replaces_capabilities
        .iter()
        .all(|capability| module.provides_capabilities.contains(capability))
    {
        return Err(module_diagnostic(
            "OA206",
            format!("{path}.replaces_capabilities"),
            "every replacement must also be provided by the module",
        ));
    }
    if !strictly_sorted_strings(&module.requires_modules) {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.requires_modules"),
            "expected unique module requirements in sorted order",
        ));
    }
    for (position, required) in module.requires_modules.iter().enumerate() {
        if !is_global_identifier(required) {
            return Err(module_diagnostic(
                "OA201",
                format!("{path}.requires_modules[{position}]"),
                "expected a namespaced lowercase module identity",
            ));
        }
    }
    if !strictly_sorted_strings(&module.incompatible_modules) {
        return Err(module_diagnostic(
            "OA201",
            format!("{path}.incompatible_modules"),
            "expected unique module incompatibilities in sorted order",
        ));
    }
    for (position, incompatible) in module.incompatible_modules.iter().enumerate() {
        if !is_global_identifier(incompatible) {
            return Err(module_diagnostic(
                "OA201",
                format!("{path}.incompatible_modules[{position}]"),
                "expected a namespaced lowercase module identity",
            ));
        }
    }
    Ok(())
}

fn module_diagnostic(
    code: &str,
    path: impl Into<String>,
    message: impl Into<String>,
) -> OpenAdventureModuleDiagnostic {
    OpenAdventureModuleDiagnostic {
        code: code.to_owned(),
        path: path.into(),
        message: message.into(),
    }
}

fn is_bounded_text(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.chars().count())
}

fn is_global_identifier(value: &str) -> bool {
    value.len() <= 255
        && value.split('.').count() >= 3
        && value.split('.').all(valid_local_identifier)
}

fn strictly_sorted_capabilities(values: &[TabletopCapability]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn strictly_sorted_strings(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// One deterministic golden-adventure step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureGoldenStep {
    pub operation: String,
    pub capability: TabletopCapability,
    pub input: DomainValue,
}

/// One deterministic golden adventure over the synthetic Open Adventure fixtures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureGoldenAdventure {
    pub golden_format_version: u32,
    pub id: String,
    pub title: String,
    pub creation: OpenAdventureCreationRequest,
    pub document: OpenAdventureEncounterDocument,
    pub modules: Vec<OpenAdventureModuleDescriptor>,
    pub module_artifacts: Vec<OpenAdventureModuleArtifact>,
    pub module_selection: OpenAdventureModuleSelection,
    pub steps: Vec<OpenAdventureGoldenStep>,
    pub expected_ending: String,
    pub expected_event_kinds: Vec<String>,
}

/// Deterministic result of replaying one golden adventure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureGoldenReplay {
    pub state: TabletopState,
    pub event_kinds: Vec<String>,
    pub effective_modules: OpenAdventureEffectiveModules,
}

/// One optional module exposed by the adapter documentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureModuleDocumentation {
    pub id: String,
    pub title: String,
    pub scope: String,
    pub experience: OpenAdventureModuleExperience,
}

/// Documentation content expressed in the adapter so docs and editors can consume it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAdventureDocumentation {
    pub supported_scope: Vec<String>,
    pub optional_modules: Vec<OpenAdventureModuleDocumentation>,
    pub exclusions: Vec<String>,
    pub compatibility_statement: String,
    pub contributor_steps: Vec<String>,
    pub golden_adventure_ids: Vec<String>,
}

/// Build the contributor-facing documentation record.
#[must_use]
pub fn open_adventure_documentation(
    modules: &[OpenAdventureModuleDescriptor],
) -> OpenAdventureDocumentation {
    let manifest = open_adventure_manifest();
    let mut optional_modules: Vec<OpenAdventureModuleDocumentation> = modules
        .iter()
        .filter(|module| {
            module.content_class == OpenAdventureModuleContentClass::OfficialCommunityOwned
        })
        .map(|module| OpenAdventureModuleDocumentation {
            id: module.id.clone(),
            title: module.title.clone(),
            scope: module.scope.clone(),
            experience: module.experience,
        })
        .collect();
    optional_modules.sort_by(|left, right| left.id.cmp(&right.id));
    let mut exclusions: Vec<String> = manifest
        .provenance
        .exclusions
        .iter()
        .map(|exclusion| format!("Excluded material: {exclusion:?}"))
        .collect();
    exclusions.push("Excluded material: Masterwork Edition layout".to_owned());
    exclusions.push("Excluded material: third-party supplements".to_owned());
    exclusions.push("Excluded material: unmarked or community-added content".to_owned());
    OpenAdventureDocumentation {
        supported_scope: vec![
            "Character creation: identity, lineage, culture, background, Modus Operandi, nine attributes, specializations, equipment references, and favored actions."
                .to_owned(),
            "Derived defense, Reaction, Body, Mind, and Spirit Points, Action Scores, and action-dice progression."
                .to_owned(),
            "Exploding polyhedral action dice with Benefits and Drawbacks."
                .to_owned(),
            "Encounters with goals, outcomes, participants, ranges, turn order, challenges, wounds, magic, Story Points, rewards, and advancement."
                .to_owned(),
            "Optional verified CC0 Community-Owned Content modules with explicit add and replace declarations."
                .to_owned(),
        ],
        optional_modules,
        exclusions,
        compatibility_statement: manifest.provenance.compatibility_statement.clone(),
        contributor_steps: vec![
            "Create a module descriptor under the Open Adventure adapter namespace with a semantic version."
                .to_owned(),
            "Mark the module as official CC0 Community-Owned Content and record the public HTTPS source URL and a 64-hex SHA-256 digest of the reviewed source record."
                .to_owned(),
            "Declare added capabilities in `provides_capabilities` and any core replacement explicitly in `replaces_capabilities`."
                .to_owned(),
            "Declare required and incompatible module identities in sorted order."
                .to_owned(),
            "Apply the selection through `apply_open_adventure_modules` and add a golden adventure in this adapter; do not edit core resolver branches."
                .to_owned(),
        ],
        golden_adventure_ids: open_adventure_golden_adventures()
            .iter()
            .map(|adventure| adventure.id.clone())
            .collect(),
    }
}

/// Deterministically replay one golden adventure, reloading JSON and RON after every step.
pub fn replay_open_adventure_golden(
    adventure: &OpenAdventureGoldenAdventure,
    registry: &ResolverRegistry,
) -> Result<OpenAdventureGoldenReplay, TabletopError> {
    if adventure.golden_format_version != OPEN_ADVENTURE_GOLDEN_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "golden_format_version",
        });
    }
    let manifest = open_adventure_manifest();
    let effective_modules = apply_open_adventure_modules(
        &adventure.modules,
        &adventure.module_artifacts,
        &adventure.module_selection,
        &manifest,
    )
    .map_err(|_| {
        invalid(
            "golden.module_selection",
            "golden adventure module selection is invalid",
        )
    })?;
    let preview = create_open_adventure_character(&adventure.creation)?;
    let document_json = adventure.document.to_json()?;
    let document_sha256 = sha256_bytes(document_json.as_bytes());
    let start_input = object([
        ("disclosure", DomainValue::Symbol("public".to_owned())),
        ("document_json", DomainValue::String(document_json)),
        ("document_sha256", DomainValue::String(document_sha256)),
        (
            "encounter_id",
            DomainValue::String(adventure.document.id.clone()),
        ),
    ]);
    let mut state = preview.initial_state.clone();
    let mut event_kinds = Vec::new();
    let start_request = ResolutionRequest {
        resolver_format_version: RESOLVER_FORMAT_VERSION,
        request_id: "golden_start".to_owned(),
        adapter: preview.adapter.clone(),
        operation: "start_encounter".to_owned(),
        capability: TabletopCapability::Encounters,
        definition_sha256: preview.definition.definition_sha256.clone(),
        definition: preview.definition.definition.clone(),
        expected_state_sha256: canonical_fingerprint(&state)?,
        input: start_input,
    };
    let receipt = registry.execute(&manifest, &start_request, &state)?;
    event_kinds.extend(receipt.events.iter().map(|event| event.kind.clone()));
    state = reload_golden_state(&receipt.after_state)?;
    for (index, step) in adventure.steps.iter().enumerate() {
        let request = ResolutionRequest {
            resolver_format_version: RESOLVER_FORMAT_VERSION,
            request_id: format!("golden_step_{index}"),
            adapter: preview.adapter.clone(),
            operation: step.operation.clone(),
            capability: step.capability,
            definition_sha256: preview.definition.definition_sha256.clone(),
            definition: preview.definition.definition.clone(),
            expected_state_sha256: canonical_fingerprint(&state)?,
            input: step.input.clone(),
        };
        let receipt = registry.execute(&manifest, &request, &state)?;
        event_kinds.extend(receipt.events.iter().map(|event| event.kind.clone()));
        state = reload_golden_state(&receipt.after_state)?;
    }
    Ok(OpenAdventureGoldenReplay {
        state,
        event_kinds,
        effective_modules,
    })
}

fn reload_golden_state(state: &TabletopState) -> Result<TabletopState, TabletopError> {
    let json_state = TabletopState::from_json(&state.to_json()?)?;
    if &json_state != state {
        return Err(invalid(
            "golden.state",
            "JSON reload changed the deterministic state",
        ));
    }
    let ron_state = TabletopState::from_ron(&state.to_ron()?)?;
    if &ron_state != state {
        return Err(invalid(
            "golden.state",
            "RON reload changed the deterministic state",
        ));
    }
    Ok(json_state)
}

/// Synthetic golden adventures covering the supported Open Adventure scope.
#[must_use]
pub fn open_adventure_golden_adventures() -> Vec<OpenAdventureGoldenAdventure> {
    vec![OpenAdventureGoldenAdventure {
        golden_format_version: OPEN_ADVENTURE_GOLDEN_FORMAT_VERSION,
        id: "sable_quay_trial".to_owned(),
        title: "Sable Quay Trial".to_owned(),
        creation: golden_creation_request(),
        document: golden_encounter_document(),
        modules: vec![golden_module()],
        module_artifacts: vec![OpenAdventureModuleArtifact {
            module_id: format!("{OPEN_ADVENTURE_NAMESPACE}.tide_clocks"),
            bytes: OPEN_ADVENTURE_MODULE_ARTIFACT_BYTES.to_vec(),
        }],
        module_selection: OpenAdventureModuleSelection {
            selection_format_version: OPEN_ADVENTURE_MODULE_SELECTION_FORMAT_VERSION,
            experience: OpenAdventureModuleExperience::Extended,
            enabled_modules: vec![format!("{OPEN_ADVENTURE_NAMESPACE}.tide_clocks")],
        },
        steps: vec![
            golden_step(
                "resolve_encounter_action",
                TabletopCapability::Encounters,
                golden_action_input(
                    "sable",
                    "warden",
                    "participant",
                    "spell",
                    Some("clear_voice"),
                    None,
                ),
            ),
            golden_step(
                "resolve_encounter_action",
                TabletopCapability::Encounters,
                golden_action_input("warden", "sable", "participant", "attack", None, None),
            ),
            golden_step(
                "resolve_encounter_action",
                TabletopCapability::Encounters,
                golden_action_input(
                    "sable",
                    "social_terms",
                    "challenge",
                    "attack",
                    None,
                    Some("tide_staff"),
                ),
            ),
            golden_step(
                "resolve_encounter_action",
                TabletopCapability::Encounters,
                golden_action_input("warden", "sable", "participant", "attack", None, None),
            ),
            golden_step(
                "resolve_encounter_action",
                TabletopCapability::Encounters,
                golden_action_input(
                    "sable",
                    "explore_ledger",
                    "challenge",
                    "attack",
                    None,
                    Some("tide_staff"),
                ),
            ),
            golden_step(
                "heal_encounter_participant",
                TabletopCapability::ResourcesAndConditions,
                golden_heal_input("sable", "body", 3),
            ),
            golden_step(
                "advance_encounter_participant",
                TabletopCapability::Advancement,
                golden_advance_input("sable", "presence", 1),
            ),
        ],
        expected_ending: "success".to_owned(),
        expected_event_kinds: vec![
            "advancement_recorded".to_owned(),
            "advancement_recorded_authority".to_owned(),
            "encounter_action_resolved".to_owned(),
            "encounter_action_resolved_authority".to_owned(),
            "encounter_ended".to_owned(),
            "encounter_ended_authority".to_owned(),
            "encounter_reward_granted".to_owned(),
            "encounter_reward_granted_authority".to_owned(),
            "encounter_started".to_owned(),
            "encounter_started_authority".to_owned(),
            "entropy_trace".to_owned(),
            "participant_healed".to_owned(),
            "participant_healed_authority".to_owned(),
        ],
    }]
}

fn golden_origin(id: &str, label: &str, note: &str) -> OpenAdventureOrigin {
    OpenAdventureOrigin {
        id: id.to_owned(),
        label: label.to_owned(),
        note: note.to_owned(),
    }
}

fn golden_creation_request() -> OpenAdventureCreationRequest {
    OpenAdventureCreationRequest {
        creation_format_version: OPEN_ADVENTURE_CREATION_FORMAT_VERSION,
        character_id: "org.weave.character.sable_quill".to_owned(),
        name: "Sable Quill".to_owned(),
        seed: 41,
        lineage: golden_origin("duskborn", "Duskborn", "Raised among tide-marked archives."),
        culture: golden_origin(
            "reed_choir",
            "Reed Choir",
            "Trained to speak over moving water.",
        ),
        background: golden_origin(
            "ledger_runner",
            "Ledger Runner",
            "Carried sealed accounts between quays.",
        ),
        modus_operandi: OpenAdventureModusOperandi {
            id: "measured_risk".to_owned(),
            label: "Measured Risk".to_owned(),
            score: 3,
        },
        attributes: OpenAdventureAttributes {
            agility: 3,
            empathy: 3,
            endurance: 2,
            focus: 3,
            insight: 3,
            might: 2,
            presence: 4,
            reasoning: 3,
            will: 3,
        },
        specializations: vec![OpenAdventureSpecialization {
            id: "voice_craft".to_owned(),
            label: "Voice Craft".to_owned(),
            attribute: OpenAdventureAttribute::Presence,
            bonus: 2,
        }],
        favored_actions: vec![OpenAdventureFavoredAction {
            id: "call_the_tide".to_owned(),
            label: "Call the Tide".to_owned(),
            attribute: OpenAdventureAttribute::Presence,
        }],
        equipment: vec![OpenAdventureEquipment {
            id: "ledger_case".to_owned(),
            label: "Ledger Case".to_owned(),
            kind: OpenAdventureEquipmentKind::Tool,
        }],
        benefits: vec![OpenAdventureTraitModifier {
            id: "steady_voice".to_owned(),
            label: "Steady Voice".to_owned(),
            target: OpenAdventureModifierTarget::ActionScore,
            modifier: 2,
        }],
        drawbacks: vec![OpenAdventureTraitModifier {
            id: "salt_rust".to_owned(),
            label: "Salt Rust".to_owned(),
            target: OpenAdventureModifierTarget::Reaction,
            modifier: -1,
        }],
    }
}

fn golden_encounter_document() -> OpenAdventureEncounterDocument {
    OpenAdventureEncounterDocument {
        document_format_version: OPEN_ADVENTURE_ENCOUNTER_FORMAT_VERSION,
        namespace: OPEN_ADVENTURE_NAMESPACE.to_owned(),
        id: "sable_quay_trial".to_owned(),
        label: "Sable Quay Trial".to_owned(),
        goal: "Open the eastern quay and learn the warden's terms.".to_owned(),
        outcomes: OpenAdventureEncounterOutcomes {
            success: "The quay opens and the terms are recorded.".to_owned(),
            failure: "The quay stays closed and the terms remain sealed.".to_owned(),
            withdrawal: "The delegation leaves before the vote.".to_owned(),
        },
        ranges: vec![
            OpenAdventureRange {
                id: "near".to_owned(),
                label: "Near".to_owned(),
                order: 0,
            },
            OpenAdventureRange {
                id: "far".to_owned(),
                label: "Far".to_owned(),
                order: 1,
            },
        ],
        adversaries: vec![OpenAdventureAdversary {
            id: "quay_warden".to_owned(),
            label: "Quay Warden".to_owned(),
            defense: 10,
            armor: 1,
            body_points: 20,
            mind_points: 20,
            spirit_points: 20,
            attack_bonus: 10,
            effect: 2,
            effect_pool: OpenAdventurePool::Body,
            spells: vec![OpenAdventureSpell {
                id: "undertow".to_owned(),
                label: "Undertow".to_owned(),
                cost: 1,
                target: OpenAdventurePool::Spirit,
                effect: 2,
                range: "near".to_owned(),
            }],
        }],
        participants: vec![
            OpenAdventureEncounterParticipant {
                id: "sable".to_owned(),
                label: "Sable Quill".to_owned(),
                kind: OpenAdventureParticipantKind::Character,
                range: "near".to_owned(),
                attribute: OpenAdventureAttribute::Presence,
                defense: 12,
                armor: 1,
                body_points: 20,
                mind_points: 20,
                spirit_points: 20,
                attack_bonus: 6,
                effect: 2,
                effect_pool: OpenAdventurePool::Mind,
                story_points: 3,
                wounds: vec![OpenAdventureWound {
                    id: "rattled".to_owned(),
                    label: "Rattled".to_owned(),
                    target: OpenAdventurePool::Mind,
                    severity: 1,
                    effect: "Sable must steady their voice.".to_owned(),
                }],
                equipment: vec![
                    OpenAdventureEncounterEquipment {
                        id: "ledger_case".to_owned(),
                        label: "Ledger Case".to_owned(),
                        kind: OpenAdventureEquipmentKind::Tool,
                        attack_bonus: 1,
                        armor: 0,
                    },
                    OpenAdventureEncounterEquipment {
                        id: "tide_staff".to_owned(),
                        label: "Tide Staff".to_owned(),
                        kind: OpenAdventureEquipmentKind::Weapon,
                        attack_bonus: 1,
                        armor: 0,
                    },
                ],
                spells: vec![OpenAdventureSpell {
                    id: "clear_voice".to_owned(),
                    label: "Clear Voice".to_owned(),
                    cost: 1,
                    target: OpenAdventurePool::Spirit,
                    effect: 3,
                    range: "far".to_owned(),
                }],
                adversary_id: String::new(),
            },
            OpenAdventureEncounterParticipant {
                id: "warden".to_owned(),
                label: "Quay Warden".to_owned(),
                kind: OpenAdventureParticipantKind::Adversary,
                range: "far".to_owned(),
                attribute: OpenAdventureAttribute::Will,
                defense: 10,
                armor: 1,
                body_points: 20,
                mind_points: 20,
                spirit_points: 20,
                attack_bonus: 10,
                effect: 2,
                effect_pool: OpenAdventurePool::Body,
                story_points: 0,
                wounds: vec![OpenAdventureWound {
                    id: "battered".to_owned(),
                    label: "Battered".to_owned(),
                    target: OpenAdventurePool::Body,
                    severity: 2,
                    effect: "The warden loses their footing.".to_owned(),
                }],
                equipment: Vec::new(),
                spells: vec![OpenAdventureSpell {
                    id: "undertow".to_owned(),
                    label: "Undertow".to_owned(),
                    cost: 1,
                    target: OpenAdventurePool::Spirit,
                    effect: 2,
                    range: "near".to_owned(),
                }],
                adversary_id: "quay_warden".to_owned(),
            },
        ],
        challenges: vec![
            OpenAdventureChallengeEntity {
                id: "explore_ledger".to_owned(),
                label: "Trace the Ledger".to_owned(),
                kind: OpenAdventureChallengeKind::Environmental,
                difficulty_score: 1,
                challenge_points: 1,
                effect: 1,
                participant_ids: vec!["sable".to_owned()],
            },
            OpenAdventureChallengeEntity {
                id: "social_terms".to_owned(),
                label: "Negotiate Terms".to_owned(),
                kind: OpenAdventureChallengeKind::Environmental,
                difficulty_score: 1,
                challenge_points: 1,
                effect: 1,
                participant_ids: vec!["sable".to_owned()],
            },
        ],
        turn_order: vec!["sable".to_owned(), "warden".to_owned()],
        rewards: vec![OpenAdventureReward {
            id: "quay_favor".to_owned(),
            label: "Quay Favor".to_owned(),
            story_points: 2,
            experience: 4,
        }],
    }
}

fn golden_module() -> OpenAdventureModuleDescriptor {
    OpenAdventureModuleDescriptor {
        module_format_version: OPEN_ADVENTURE_MODULE_FORMAT_VERSION,
        id: format!("{OPEN_ADVENTURE_NAMESPACE}.tide_clocks"),
        version: "1.0.0".to_owned(),
        namespace: OPEN_ADVENTURE_NAMESPACE.to_owned(),
        title: "Tide Clocks".to_owned(),
        scope: "Original synthetic module adding progress clocks for harbor pacing.".to_owned(),
        experience: OpenAdventureModuleExperience::Extended,
        provides_capabilities: vec![TabletopCapability::Clocks],
        replaces_capabilities: Vec::new(),
        requires_modules: Vec::new(),
        incompatible_modules: Vec::new(),
        source_url: "https://openadventurerpg.com/".to_owned(),
        license: "CC0".to_owned(),
        content_class: OpenAdventureModuleContentClass::OfficialCommunityOwned,
        sha256: OPEN_ADVENTURE_MODULE_ARTIFACT_SHA256.to_owned(),
    }
}

fn golden_step(
    operation: &str,
    capability: TabletopCapability,
    input: DomainValue,
) -> OpenAdventureGoldenStep {
    OpenAdventureGoldenStep {
        operation: operation.to_owned(),
        capability,
        input,
    }
}

fn golden_action_input(
    actor_id: &str,
    target_id: &str,
    target_kind: &str,
    action: &str,
    spell_id: Option<&str>,
    equipment_id: Option<&str>,
) -> DomainValue {
    let mut fields = object_fields([
        ("action", DomainValue::Symbol(action.to_owned())),
        ("actor_id", DomainValue::String(actor_id.to_owned())),
        ("disclosure", DomainValue::Symbol("public".to_owned())),
        (
            "encounter_id",
            DomainValue::String("sable_quay_trial".to_owned()),
        ),
        ("situation_modifier", DomainValue::Number(0.0)),
        ("spend_story_point", DomainValue::Bool(false)),
        ("target_id", DomainValue::String(target_id.to_owned())),
        ("target_kind", DomainValue::Symbol(target_kind.to_owned())),
    ]);
    if let Some(id) = spell_id {
        fields.insert("spell_id".to_owned(), DomainValue::String(id.to_owned()));
    }
    if let Some(id) = equipment_id {
        fields.insert(
            "equipment_id".to_owned(),
            DomainValue::String(id.to_owned()),
        );
    }
    DomainValue::Object(fields)
}

fn golden_heal_input(participant_id: &str, pool: &str, amount: u32) -> DomainValue {
    object([
        ("amount", DomainValue::Number(f64::from(amount))),
        ("disclosure", DomainValue::Symbol("public".to_owned())),
        (
            "encounter_id",
            DomainValue::String("sable_quay_trial".to_owned()),
        ),
        (
            "participant_id",
            DomainValue::String(participant_id.to_owned()),
        ),
        ("pool", DomainValue::Symbol(pool.to_owned())),
    ])
}

fn golden_advance_input(participant_id: &str, attribute: &str, cost: u32) -> DomainValue {
    object([
        ("attribute", DomainValue::Symbol(attribute.to_owned())),
        ("disclosure", DomainValue::Symbol("public".to_owned())),
        (
            "encounter_id",
            DomainValue::String("sable_quay_trial".to_owned()),
        ),
        ("experience_cost", DomainValue::Number(f64::from(cost))),
        (
            "participant_id",
            DomainValue::String(participant_id.to_owned()),
        ),
    ])
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
impl_open_adventure_io!(OpenAdventureEncounterDocument);
impl_open_adventure_io!(OpenAdventureRuntimeEncounter);
impl_open_adventure_io!(OpenAdventureModuleDescriptor);
impl_open_adventure_io!(OpenAdventureModuleSelection);
impl_open_adventure_io!(OpenAdventureEffectiveModules);
impl_open_adventure_io!(OpenAdventureGoldenAdventure);
impl_open_adventure_io!(OpenAdventureGoldenReplay);
impl_open_adventure_io!(OpenAdventureDocumentation);

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
    if !object_map(state_fields, "encounters")?.is_empty() {
        return Err(invalid(
            "initial_state.value",
            "a newly created character cannot start inside an encounter",
        ));
    }
    let advancement = object_map(state_fields, "advancement")?;
    if object_u32(advancement, "experience")? != 0
        || object_u32(advancement, "spent")? != 0
        || object_u32(advancement, "rank")? != 0
        || object_u32(advancement, "story_points")? != 0
        || !object_map(advancement, "attribute_bonuses")?.is_empty()
    {
        return Err(invalid(
            "initial_state.value",
            "a newly created character cannot start with advancement",
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
        ("encounters", DomainValue::Object(BTreeMap::new())),
        (
            "advancement",
            object([
                ("attribute_bonuses", DomainValue::Object(BTreeMap::new())),
                ("experience", DomainValue::Number(0.0)),
                ("rank", DomainValue::Number(0.0)),
                ("spent", DomainValue::Number(0.0)),
                ("story_points", DomainValue::Number(0.0)),
            ]),
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
        if specialization_bonuses.contains_key(&attribute) {
            return Err(invalid(
                "definition.specializations",
                "an attribute may have only one specialization",
            ));
        }
        specialization_bonuses.insert(attribute, bonus);
        if specialization_bonuses.len() > MAX_SPECIALIZATIONS {
            return Err(invalid(
                "definition.specializations",
                "too many specializations",
            ));
        }
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
            "start_encounter" => resolve_start_encounter(definition, request, state)?,
            "resolve_encounter_action" => {
                resolve_encounter_action(definition, request, state, entropy)?
            }
            "end_encounter" => resolve_end_encounter(request, state)?,
            "heal_encounter_participant" => {
                resolve_heal_encounter_participant(definition, request, state)?
            }
            "advance_encounter_participant" => {
                resolve_advance_encounter_participant(request, state)?
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncounterAction {
    Attack,
    Spell,
}

impl EncounterAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Attack => "attack",
            Self::Spell => "spell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncounterTargetKind {
    Challenge,
    Participant,
}

impl EncounterTargetKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Challenge => "challenge",
            Self::Participant => "participant",
        }
    }
}

fn parse_encounter_action(value: &str) -> Result<EncounterAction, TabletopError> {
    match value {
        "attack" => Ok(EncounterAction::Attack),
        "spell" => Ok(EncounterAction::Spell),
        _ => Err(invalid("action", "expected attack or spell")),
    }
}

fn parse_target_kind(value: &str) -> Result<EncounterTargetKind, TabletopError> {
    match value {
        "challenge" => Ok(EncounterTargetKind::Challenge),
        "participant" => Ok(EncounterTargetKind::Participant),
        _ => Err(invalid("target_kind", "expected challenge or participant")),
    }
}

fn parse_encounter_ending(value: &str) -> Result<OpenAdventureEncounterEnding, TabletopError> {
    match value {
        "defeat" => Ok(OpenAdventureEncounterEnding::Defeat),
        "host_decision" => Ok(OpenAdventureEncounterEnding::HostDecision),
        "success" => Ok(OpenAdventureEncounterEnding::Success),
        "withdrawal" => Ok(OpenAdventureEncounterEnding::Withdrawal),
        _ => Err(invalid(
            "ending",
            "expected defeat, host_decision, success, or withdrawal",
        )),
    }
}

fn encounter_state_clone(
    state_fields: &BTreeMap<String, DomainValue>,
    encounter_id: &str,
) -> Result<BTreeMap<String, DomainValue>, TabletopError> {
    let encounters = object_map(state_fields, "encounters")?;
    let encounter = encounters
        .get(encounter_id)
        .ok_or_else(|| invalid("state.encounters", "encounter does not exist"))?;
    Ok(value_object("state.encounters", encounter)?.clone())
}

fn store_encounter_state(
    state_fields: &mut BTreeMap<String, DomainValue>,
    encounter_id: &str,
    encounter: BTreeMap<String, DomainValue>,
) -> Result<(), TabletopError> {
    let encounters = object_map_mut(state_fields, "encounters")?;
    encounters.insert(encounter_id.to_owned(), DomainValue::Object(encounter));
    Ok(())
}

fn encounter_document_from_state(
    encounter: &BTreeMap<String, DomainValue>,
) -> Result<OpenAdventureEncounterDocument, TabletopError> {
    let document_json = object_string(encounter, "document")?;
    let document = OpenAdventureEncounterDocument::from_json(document_json)?;
    validate_open_adventure_encounter_document(&document)?;
    Ok(document)
}

fn participant_state(
    encounter: &BTreeMap<String, DomainValue>,
    participant_id: &str,
) -> Result<BTreeMap<String, DomainValue>, TabletopError> {
    let participants = object_map(encounter, "participants")?;
    let participant = participants.get(participant_id).ok_or_else(|| {
        invalid(
            "state.encounters.participants",
            "participant does not exist",
        )
    })?;
    Ok(value_object("state.encounters.participants", participant)?.clone())
}

fn store_participant_state(
    encounter: &mut BTreeMap<String, DomainValue>,
    participant_id: &str,
    state: BTreeMap<String, DomainValue>,
) -> Result<(), TabletopError> {
    let participants = object_map_mut(encounter, "participants")?;
    participants.insert(participant_id.to_owned(), DomainValue::Object(state));
    Ok(())
}

fn challenge_state(
    encounter: &BTreeMap<String, DomainValue>,
    challenge_id: &str,
) -> Result<BTreeMap<String, DomainValue>, TabletopError> {
    let challenges = object_map(encounter, "challenges")?;
    let challenge = challenges
        .get(challenge_id)
        .ok_or_else(|| invalid("state.encounters.challenges", "challenge does not exist"))?;
    Ok(value_object("state.encounters.challenges", challenge)?.clone())
}

fn store_challenge_state(
    encounter: &mut BTreeMap<String, DomainValue>,
    challenge_id: &str,
    state: BTreeMap<String, DomainValue>,
) -> Result<(), TabletopError> {
    let challenges = object_map_mut(encounter, "challenges")?;
    challenges.insert(challenge_id.to_owned(), DomainValue::Object(state));
    Ok(())
}

fn new_encounter_state_value(
    document: &OpenAdventureEncounterDocument,
    canonical_document: String,
    character_body_points: u32,
    character_mind_points: u32,
    character_spirit_points: u32,
) -> Result<DomainValue, TabletopError> {
    let participants = document
        .participants
        .iter()
        .map(|participant| {
            let (body_points, mind_points, spirit_points) = match participant.kind {
                OpenAdventureParticipantKind::Character => (
                    character_body_points,
                    character_mind_points,
                    character_spirit_points,
                ),
                OpenAdventureParticipantKind::Adversary => (
                    participant.body_points,
                    participant.mind_points,
                    participant.spirit_points,
                ),
            };
            let exhausted_pool = if body_points == 0 {
                "body"
            } else if mind_points == 0 {
                "mind"
            } else if spirit_points == 0 {
                "spirit"
            } else {
                ""
            };
            let incapacitated = !exhausted_pool.is_empty();
            (
                participant.id.clone(),
                object([
                    ("body_points", DomainValue::Number(f64::from(body_points))),
                    ("defeated", DomainValue::Bool(incapacitated)),
                    (
                        "exhausted_pool",
                        DomainValue::String(exhausted_pool.to_owned()),
                    ),
                    ("incapacitated", DomainValue::Bool(incapacitated)),
                    ("mind_points", DomainValue::Number(f64::from(mind_points))),
                    (
                        "spirit_points",
                        DomainValue::Number(f64::from(spirit_points)),
                    ),
                    (
                        "story_points",
                        DomainValue::Number(f64::from(participant.story_points)),
                    ),
                    ("wounds", DomainValue::List(Vec::new())),
                ]),
            )
        })
        .collect();
    let challenges = document
        .challenges
        .iter()
        .map(|challenge| {
            (
                challenge.id.clone(),
                object([
                    ("completed", DomainValue::Bool(false)),
                    (
                        "remaining_challenge_points",
                        DomainValue::Number(f64::from(challenge.challenge_points)),
                    ),
                ]),
            )
        })
        .collect();
    Ok(object([
        ("challenges", DomainValue::Object(challenges)),
        ("document", DomainValue::String(canonical_document)),
        ("ended", DomainValue::Bool(false)),
        ("ending", DomainValue::String(String::new())),
        ("participants", DomainValue::Object(participants)),
        ("turn_index", DomainValue::Number(0.0)),
    ]))
}

fn advancement_bonus(
    state_fields: &BTreeMap<String, DomainValue>,
    attribute: &str,
) -> Result<i32, TabletopError> {
    let advancement = object_map(state_fields, "advancement")?;
    let bonuses = object_map(advancement, "attribute_bonuses")?;
    let value = optional_u32(bonuses, attribute)?.unwrap_or(0);
    i32::try_from(value)
        .map_err(|_| invalid("state.advancement.attribute_bonuses", "bonus overflow"))
}

fn derived_action_score(
    derived: &OpenAdventureDerived,
    attribute: OpenAdventureAttribute,
) -> Result<i32, TabletopError> {
    derived
        .action_scores
        .iter()
        .find(|score| score.attribute == attribute)
        .map(|score| score.total)
        .ok_or_else(|| invalid("definition", "derived action score is unavailable"))
}

fn grant_encounter_rewards(
    state_fields: &mut BTreeMap<String, DomainValue>,
    document: &OpenAdventureEncounterDocument,
) -> Result<(u32, u32), TabletopError> {
    let mut story_points = 0_u32;
    let mut experience = 0_u32;
    for reward in &document.rewards {
        story_points = story_points
            .checked_add(reward.story_points)
            .ok_or_else(|| invalid("rewards", "Story Point total overflow"))?;
        experience = experience
            .checked_add(reward.experience)
            .ok_or_else(|| invalid("rewards", "experience total overflow"))?;
    }
    if story_points == 0 && experience == 0 {
        return Ok((0, 0));
    }
    let advancement = object_map_mut(state_fields, "advancement")?;
    let current_story = object_u32(advancement, "story_points")?;
    set_u32(
        advancement,
        "story_points",
        current_story.saturating_add(story_points),
    );
    let current_experience = object_u32(advancement, "experience")?;
    set_u32(
        advancement,
        "experience",
        current_experience.saturating_add(experience),
    );
    Ok((story_points, experience))
}

fn encounter_ending_check(
    encounter: &BTreeMap<String, DomainValue>,
    document: &OpenAdventureEncounterDocument,
) -> Result<Option<OpenAdventureEncounterEnding>, TabletopError> {
    let mut characters = 0_u32;
    let mut characters_defeated = 0_u32;
    let mut adversaries = 0_u32;
    let mut adversaries_defeated = 0_u32;
    for participant in &document.participants {
        let state = participant_state(encounter, &participant.id)?;
        let defeated = object_bool(&state, "defeated")?;
        match participant.kind {
            OpenAdventureParticipantKind::Character => {
                characters += 1;
                if defeated {
                    characters_defeated += 1;
                }
            }
            OpenAdventureParticipantKind::Adversary => {
                adversaries += 1;
                if defeated {
                    adversaries_defeated += 1;
                }
            }
        }
    }
    if characters > 0 && characters_defeated == characters {
        return Ok(Some(OpenAdventureEncounterEnding::Defeat));
    }
    if adversaries > 0 && adversaries_defeated == adversaries {
        return Ok(Some(OpenAdventureEncounterEnding::Success));
    }
    let mut challenges = 0_u32;
    let mut challenges_completed = 0_u32;
    for challenge in &document.challenges {
        let state = challenge_state(encounter, &challenge.id)?;
        challenges += 1;
        if object_bool(&state, "completed")? {
            challenges_completed += 1;
        }
    }
    if challenges > 0 && challenges_completed == challenges {
        return Ok(Some(OpenAdventureEncounterEnding::Success));
    }
    Ok(None)
}

fn next_active_turn_index(
    encounter: &BTreeMap<String, DomainValue>,
    turn_order: &[String],
    start: usize,
) -> Result<usize, TabletopError> {
    if turn_order.is_empty() {
        return Err(invalid(
            "state.encounters.turn_order",
            "turn order is empty",
        ));
    }
    let mut index = start % turn_order.len();
    for _ in 0..=turn_order.len() {
        let state = participant_state(encounter, &turn_order[index])?;
        if !object_bool(&state, "incapacitated")? {
            return Ok(index);
        }
        index = (index + 1) % turn_order.len();
    }
    Err(invalid(
        "state.encounters.turn_order",
        "no active participant remains",
    ))
}

fn dice_payload(rolled: &[RolledDie]) -> DomainValue {
    DomainValue::List(
        rolled
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
    )
}

fn resolve_start_encounter(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let input = value_object("input", request)?;
    let encounter_id = object_string(input, "encounter_id")?.to_owned();
    let document_json = object_string(input, "document_json")?.to_owned();
    let document_sha256 = object_string(input, "document_sha256")?.to_owned();
    let disclosure = parse_disclosure(object_symbol(input, "disclosure")?)?;
    validate_local_identifier("input.encounter_id", &encounter_id)?;
    validate_sha256("input.document_sha256", &document_sha256)?;
    if document_json.chars().count() > 65_536 {
        return Err(invalid(
            "input.document_json",
            "encounter document exceeds 65536 characters",
        ));
    }
    let document = OpenAdventureEncounterDocument::from_json(&document_json)?;
    validate_open_adventure_encounter_document(&document)?;
    if document.id != encounter_id {
        return Err(invalid(
            "input.encounter_id",
            "encounter identity does not match the document",
        ));
    }
    let canonical_document = document.to_json()?;
    if sha256_bytes(canonical_document.as_bytes()) != document_sha256 {
        return Err(invalid(
            "input.document_sha256",
            "document digest does not match the canonical validated document",
        ));
    }
    let derived = open_adventure_derived(definition)?;
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    if object_bool(state_fields, "defeated")? {
        return Err(invalid(
            "state.defeated",
            "a defeated character cannot start an encounter",
        ));
    }
    let body_points = object_u32(state_fields, "body_points")?.min(derived.body_points);
    let mind_points = object_u32(state_fields, "mind_points")?.min(derived.mind_points);
    let spirit_points = object_u32(state_fields, "spirit_points")?.min(derived.spirit_points);
    let encounters = object_map_mut(state_fields, "encounters")?;
    if encounters.contains_key(&encounter_id) {
        return Err(invalid(
            "input.encounter_id",
            "encounter identity already exists",
        ));
    }
    if encounters.len() >= MAX_ENCOUNTERS {
        return Err(invalid("state.encounters", "too many active encounters"));
    }
    encounters.insert(
        encounter_id.clone(),
        new_encounter_state_value(
            &document,
            canonical_document,
            body_points,
            mind_points,
            spirit_points,
        )?,
    );
    let mut public_fields = object_fields([
        (
            "challenge_ids",
            DomainValue::List(
                document
                    .challenges
                    .iter()
                    .map(|challenge| DomainValue::String(challenge.id.clone()))
                    .collect(),
            ),
        ),
        ("encounter_id", DomainValue::String(encounter_id.clone())),
        ("goal", DomainValue::String(document.goal.clone())),
        ("label", DomainValue::String(document.label.clone())),
        (
            "participant_ids",
            DomainValue::List(
                document
                    .participants
                    .iter()
                    .map(|participant| DomainValue::String(participant.id.clone()))
                    .collect(),
            ),
        ),
        (
            "turn_order",
            DomainValue::List(
                document
                    .turn_order
                    .iter()
                    .map(|id| DomainValue::String(id.clone()))
                    .collect(),
            ),
        ),
    ]);
    let mut events = Vec::new();
    if disclosure == OpenAdventureDisclosure::Public {
        events.push(public_event(
            "encounter_started",
            DomainValue::Object(public_fields.clone()),
        ));
    }
    public_fields.insert(
        "disclosure".to_owned(),
        DomainValue::Symbol(disclosure.as_str().to_owned()),
    );
    events.push(authority_event(
        "encounter_started_authority",
        DomainValue::Object(public_fields),
    ));
    Ok((next, events))
}

fn resolve_encounter_action(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let derived = open_adventure_derived(definition)?;
    let input = value_object("input", request)?;
    let encounter_id = object_string(input, "encounter_id")?.to_owned();
    let actor_id = object_string(input, "actor_id")?.to_owned();
    let target_id = object_string(input, "target_id")?.to_owned();
    let action = parse_encounter_action(object_symbol(input, "action")?)?;
    let target_kind = parse_target_kind(object_symbol(input, "target_kind")?)?;
    let situation_modifier = object_i32(input, "situation_modifier")?;
    let spend_story_point = object_bool(input, "spend_story_point")?;
    let disclosure = parse_disclosure(object_symbol(input, "disclosure")?)?;
    let spell_id = optional_string(input, "spell_id")?.map(str::to_owned);
    let equipment_id = optional_string(input, "equipment_id")?.map(str::to_owned);
    validate_local_identifier("input.encounter_id", &encounter_id)?;
    validate_local_identifier("input.actor_id", &actor_id)?;
    validate_local_identifier("input.target_id", &target_id)?;
    if let Some(id) = &spell_id {
        validate_local_identifier("input.spell_id", id)?;
    }
    if let Some(id) = &equipment_id {
        validate_local_identifier("input.equipment_id", id)?;
    }
    if !(-10..=10).contains(&situation_modifier) {
        return Err(invalid(
            "input.situation_modifier",
            "expected a situation modifier from -10 through 10",
        ));
    }

    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let mut encounter = encounter_state_clone(state_fields, &encounter_id)?;
    let document = encounter_document_from_state(&encounter)?;
    if object_bool(&encounter, "ended")? {
        return Err(invalid(
            "state.encounters.ended",
            "encounter has already ended",
        ));
    }
    let current_index = next_active_turn_index(
        &encounter,
        &document.turn_order,
        usize::try_from(object_u32(&encounter, "turn_index")?)
            .map_err(|_| invalid("state.encounters.turn_index", "turn index overflow"))?,
    )?;
    let current_actor = &document.turn_order[current_index];
    if current_actor != &actor_id {
        return Err(invalid(
            "input.actor_id",
            "actor is not the current participant in turn order",
        ));
    }
    if target_kind == EncounterTargetKind::Participant && target_id == actor_id {
        return Err(invalid(
            "input.target_id",
            "an action cannot target its own actor",
        ));
    }
    let actor = document
        .participants
        .iter()
        .find(|participant| participant.id == actor_id)
        .ok_or_else(|| {
            invalid(
                "input.actor_id",
                "actor is not part of the encounter document",
            )
        })?;
    let actor_state = participant_state(&encounter, &actor_id)?;
    if object_bool(&actor_state, "incapacitated")? {
        return Err(invalid(
            "input.actor_id",
            "the current actor is incapacitated",
        ));
    }
    let mut story_points = object_u32(&actor_state, "story_points")?;
    let mut equipment_bonus = 0;
    if let Some(id) = &equipment_id {
        let item = actor
            .equipment
            .iter()
            .find(|item| &item.id == id)
            .ok_or_else(|| {
                invalid(
                    "input.equipment_id",
                    "equipment is not carried by the actor",
                )
            })?;
        if item.kind != OpenAdventureEquipmentKind::Weapon {
            return Err(invalid(
                "input.equipment_id",
                "only weapon equipment grants an attack bonus",
            ));
        }
        equipment_bonus = item.attack_bonus;
    }
    let mut story_point_spent = false;
    if spend_story_point {
        if story_points == 0 {
            return Err(invalid(
                "input.spend_story_point",
                "actor has no Story Points to spend",
            ));
        }
        story_points -= 1;
        story_point_spent = true;
    }
    let mut spell_cost = 0;
    let mut effect = actor.effect;
    let mut effect_pool = actor.effect_pool;
    match action {
        EncounterAction::Attack => {
            if spell_id.is_some() {
                return Err(invalid(
                    "input.spell_id",
                    "attack actions cannot declare a spell",
                ));
            }
        }
        EncounterAction::Spell => {
            let id = spell_id.as_ref().ok_or_else(|| {
                invalid("input.spell_id", "spell actions require a spell identity")
            })?;
            let spell = actor
                .spells
                .iter()
                .find(|spell| &spell.id == id)
                .ok_or_else(|| invalid("input.spell_id", "spell is not known to the actor"))?;
            if story_points < spell.cost {
                return Err(invalid(
                    "input.spell_id",
                    "actor lacks the Story Points required by the spell",
                ));
            }
            story_points -= spell.cost;
            spell_cost = spell.cost;
            effect = spell.effect;
            effect_pool = spell.target;
        }
    }
    let advancement = if actor.kind == OpenAdventureParticipantKind::Character {
        advancement_bonus(state_fields, actor.attribute.as_str())?
    } else {
        0
    };
    let actor_base = if actor.kind == OpenAdventureParticipantKind::Character {
        derived_action_score(&derived, actor.attribute)?
    } else {
        actor.attack_bonus
    };
    let action_score = checked_sum(&[
        actor_base,
        equipment_bonus,
        if story_point_spent { 2 } else { 0 },
        advancement,
        situation_modifier,
    ])?;
    if !(-1_000..=1_000).contains(&action_score) {
        return Err(invalid(
            "action_score",
            "final action score is outside the reviewed bounds",
        ));
    }
    let dice = open_adventure_action_dice(action_score);

    let mut challenge_effect = 0;
    let (target_number, target_document, target_state) = match target_kind {
        EncounterTargetKind::Participant => {
            let target = document
                .participants
                .iter()
                .find(|participant| participant.id == target_id)
                .ok_or_else(|| {
                    invalid(
                        "input.target_id",
                        "target is not part of the encounter document",
                    )
                })?;
            let target_state = participant_state(&encounter, &target_id)?;
            if object_bool(&target_state, "incapacitated")? {
                return Err(invalid(
                    "input.target_id",
                    "target participant is already incapacitated",
                ));
            }
            let target_defense = if target.kind == OpenAdventureParticipantKind::Character {
                let defense = u32::try_from(derived.defense).map_err(|_| {
                    invalid("definition.defense", "derived defense must be positive")
                })?;
                if !(1..=1_000).contains(&defense) {
                    return Err(invalid(
                        "definition.defense",
                        "derived defense is outside the reviewed bounds",
                    ));
                }
                defense
            } else {
                target.defense
            };
            (target_defense, Some(target), Some(target_state))
        }
        EncounterTargetKind::Challenge => {
            let challenge = document
                .challenges
                .iter()
                .find(|challenge| challenge.id == target_id)
                .ok_or_else(|| {
                    invalid(
                        "input.target_id",
                        "target is not part of the encounter document",
                    )
                })?;
            let challenge_state = challenge_state(&encounter, &target_id)?;
            if object_bool(&challenge_state, "completed")? {
                return Err(invalid(
                    "input.target_id",
                    "target challenge is already complete",
                ));
            }
            challenge_effect = challenge.effect;
            (challenge.difficulty_score, None, Some(challenge_state))
        }
    };

    let cursor_before = entropy.state().cursor;
    let (rolled_dice, roll_total) = roll_action_dice(entropy, dice)?;
    let cursor_after = entropy.state().cursor;
    let success = roll_total >= u64::from(target_number);
    let mut damage = 0_u32;
    let mut remaining_points = 0_u32;
    let mut challenge_points_after = 0_u32;
    let mut wound_id = String::new();
    let mut incapacitated = false;
    if success {
        match target_kind {
            EncounterTargetKind::Participant => {
                let target = target_document.ok_or_else(|| {
                    invalid("input.target_id", "target participant is unavailable")
                })?;
                let target_state = target_state.ok_or_else(|| {
                    invalid("input.target_id", "target participant state is unavailable")
                })?;
                damage = effect.saturating_sub(target.armor).max(1);
                let pool_field = effect_pool.field_name();
                let current = object_u32(&target_state, pool_field)?;
                let reduced = current.saturating_sub(damage);
                remaining_points = reduced;
                let mut updated_target = target_state;
                set_u32(&mut updated_target, pool_field, reduced);
                if reduced == 0 {
                    incapacitated = true;
                    updated_target.insert("incapacitated".to_owned(), DomainValue::Bool(true));
                    updated_target.insert("defeated".to_owned(), DomainValue::Bool(true));
                    updated_target.insert(
                        "exhausted_pool".to_owned(),
                        DomainValue::String(effect_pool.as_str().to_owned()),
                    );
                    let applied = object_list(&updated_target, "wounds")?;
                    if let Some(wound) = target.wounds.iter().find(|wound| {
                        wound.target == effect_pool
                            && !applied.iter().any(|value| {
                                matches!(value, DomainValue::String(id) if id.as_str() == wound.id.as_str())
                            })
                    }) {
                        wound_id = wound.id.clone();
                    }
                    if !wound_id.is_empty()
                        && let Some(DomainValue::List(wounds)) = updated_target.get_mut("wounds")
                    {
                        wounds.push(DomainValue::String(wound_id.clone()));
                    }
                }
                store_participant_state(&mut encounter, &target_id, updated_target)?;
            }
            EncounterTargetKind::Challenge => {
                let state_value = target_state.ok_or_else(|| {
                    invalid("input.target_id", "target challenge state is unavailable")
                })?;
                let current = object_u32(&state_value, "remaining_challenge_points")?;
                let reduced = current.saturating_sub(challenge_effect);
                challenge_points_after = reduced;
                let mut updated_challenge = state_value;
                set_u32(
                    &mut updated_challenge,
                    "remaining_challenge_points",
                    reduced,
                );
                updated_challenge.insert("completed".to_owned(), DomainValue::Bool(reduced == 0));
                store_challenge_state(&mut encounter, &target_id, updated_challenge)?;
            }
        }
    }
    let mut updated_actor = participant_state(&encounter, &actor_id)?;
    set_u32(&mut updated_actor, "story_points", story_points);
    store_participant_state(&mut encounter, &actor_id, updated_actor)?;

    let next_index = next_active_turn_index(
        &encounter,
        &document.turn_order,
        (current_index + 1) % document.turn_order.len(),
    )?;
    set_u32(
        &mut encounter,
        "turn_index",
        u32::try_from(next_index)
            .map_err(|_| invalid("state.encounters.turn_index", "turn index overflow"))?,
    );
    let next_index_number = u32::try_from(next_index)
        .map_err(|_| invalid("state.encounters.turn_index", "turn index overflow"))?;
    let ending = encounter_ending_check(&encounter, &document)?;
    if let Some(ending) = ending {
        encounter.insert("ended".to_owned(), DomainValue::Bool(true));
        encounter.insert(
            "ending".to_owned(),
            DomainValue::String(ending.as_str().to_owned()),
        );
    }
    let ended = ending.is_some();
    let ending_text = ending
        .map_or("", OpenAdventureEncounterEnding::as_str)
        .to_owned();
    store_encounter_state(state_fields, &encounter_id, encounter)?;
    let (story_points_awarded, experience_awarded) =
        if ended && ending == Some(OpenAdventureEncounterEnding::Success) {
            grant_encounter_rewards(state_fields, &document)?
        } else {
            (0, 0)
        };

    let outcome = if success { "success" } else { "failure" };
    let mut public_fields = object_fields([
        ("action", DomainValue::Symbol(action.as_str().to_owned())),
        ("action_score", DomainValue::Number(f64::from(action_score))),
        ("actor_id", DomainValue::String(actor_id.clone())),
        (
            "challenge_points_after",
            DomainValue::Number(f64::from(challenge_points_after)),
        ),
        ("damage", DomainValue::Number(f64::from(damage))),
        ("dice", dice_payload(&rolled_dice)),
        ("ended", DomainValue::Bool(ended)),
        ("ending", DomainValue::String(ending_text.clone())),
        ("outcome", DomainValue::Symbol(outcome.to_owned())),
        (
            "remaining_points",
            DomainValue::Number(f64::from(remaining_points)),
        ),
        ("roll_total", DomainValue::Number(roll_total as f64)),
        ("target_id", DomainValue::String(target_id.clone())),
        (
            "target_kind",
            DomainValue::Symbol(target_kind.as_str().to_owned()),
        ),
        (
            "target_number",
            DomainValue::Number(f64::from(target_number)),
        ),
        (
            "turn_index",
            DomainValue::Number(f64::from(next_index_number)),
        ),
    ]);
    let mut events = Vec::new();
    if disclosure == OpenAdventureDisclosure::Public {
        events.push(public_event(
            "encounter_action_resolved",
            DomainValue::Object(public_fields.clone()),
        ));
    }
    public_fields.insert(
        "advancement_bonus".to_owned(),
        DomainValue::Number(f64::from(advancement)),
    );
    public_fields.insert(
        "attack_bonus".to_owned(),
        DomainValue::Number(f64::from(actor_base)),
    );
    public_fields.insert(
        "disclosure".to_owned(),
        DomainValue::Symbol(disclosure.as_str().to_owned()),
    );
    public_fields.insert(
        "equipment_bonus".to_owned(),
        DomainValue::Number(f64::from(equipment_bonus)),
    );
    public_fields.insert(
        "equipment_id".to_owned(),
        DomainValue::String(equipment_id.clone().unwrap_or_default()),
    );
    public_fields.insert(
        "situation_modifier".to_owned(),
        DomainValue::Number(f64::from(situation_modifier)),
    );
    public_fields.insert(
        "spell_cost".to_owned(),
        DomainValue::Number(f64::from(spell_cost)),
    );
    public_fields.insert(
        "spell_id".to_owned(),
        DomainValue::String(spell_id.clone().unwrap_or_default()),
    );
    public_fields.insert(
        "story_point_spent".to_owned(),
        DomainValue::Bool(story_point_spent),
    );
    public_fields.insert(
        "story_points_after".to_owned(),
        DomainValue::Number(f64::from(story_points)),
    );
    events.push(authority_event(
        "encounter_action_resolved_authority",
        DomainValue::Object(public_fields),
    ));
    if incapacitated {
        let mut condition_fields = object_fields([
            ("actor_id", DomainValue::String(target_id.clone())),
            ("defeated", DomainValue::Bool(true)),
            ("incapacitated", DomainValue::Bool(true)),
            ("pool", DomainValue::Symbol(effect_pool.as_str().to_owned())),
            (
                "remaining_points",
                DomainValue::Number(f64::from(remaining_points)),
            ),
            ("wound_id", DomainValue::String(wound_id)),
        ]);
        if disclosure == OpenAdventureDisclosure::Public {
            events.push(public_event(
                "encounter_condition_changed",
                DomainValue::Object(condition_fields.clone()),
            ));
        }
        condition_fields.insert(
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.as_str().to_owned()),
        );
        events.push(authority_event(
            "encounter_condition_changed_authority",
            DomainValue::Object(condition_fields),
        ));
    }
    if ended {
        let mut ended_fields = object_fields([
            ("encounter_id", DomainValue::String(encounter_id.clone())),
            ("ending", DomainValue::Symbol(ending_text)),
            (
                "experience_awarded",
                DomainValue::Number(f64::from(experience_awarded)),
            ),
            (
                "story_points_awarded",
                DomainValue::Number(f64::from(story_points_awarded)),
            ),
        ]);
        if disclosure == OpenAdventureDisclosure::Public {
            events.push(public_event(
                "encounter_ended",
                DomainValue::Object(ended_fields.clone()),
            ));
        }
        ended_fields.insert(
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.as_str().to_owned()),
        );
        events.push(authority_event(
            "encounter_ended_authority",
            DomainValue::Object(ended_fields),
        ));
        if story_points_awarded > 0 || experience_awarded > 0 {
            let mut reward_fields = object_fields([
                ("encounter_id", DomainValue::String(encounter_id.clone())),
                (
                    "experience",
                    DomainValue::Number(f64::from(experience_awarded)),
                ),
                (
                    "reward_ids",
                    DomainValue::List(
                        document
                            .rewards
                            .iter()
                            .map(|reward| DomainValue::String(reward.id.clone()))
                            .collect(),
                    ),
                ),
                (
                    "story_points",
                    DomainValue::Number(f64::from(story_points_awarded)),
                ),
            ]);
            if disclosure == OpenAdventureDisclosure::Public {
                events.push(public_event(
                    "encounter_reward_granted",
                    DomainValue::Object(reward_fields.clone()),
                ));
            }
            reward_fields.insert(
                "disclosure".to_owned(),
                DomainValue::Symbol(disclosure.as_str().to_owned()),
            );
            events.push(authority_event(
                "encounter_reward_granted_authority",
                DomainValue::Object(reward_fields),
            ));
        }
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

fn resolve_end_encounter(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let input = value_object("input", request)?;
    let encounter_id = object_string(input, "encounter_id")?.to_owned();
    let ending = parse_encounter_ending(object_symbol(input, "ending")?)?;
    let disclosure = parse_disclosure(object_symbol(input, "disclosure")?)?;
    validate_local_identifier("input.encounter_id", &encounter_id)?;
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let mut encounter = encounter_state_clone(state_fields, &encounter_id)?;
    if object_bool(&encounter, "ended")? {
        return Err(invalid(
            "state.encounters.ended",
            "encounter has already ended",
        ));
    }
    let document = encounter_document_from_state(&encounter)?;
    if ending == OpenAdventureEncounterEnding::Success
        && encounter_ending_check(&encounter, &document)?
            != Some(OpenAdventureEncounterEnding::Success)
    {
        return Err(invalid(
            "input.ending",
            "claimed success does not satisfy an encounter success condition",
        ));
    }
    encounter.insert("ended".to_owned(), DomainValue::Bool(true));
    encounter.insert(
        "ending".to_owned(),
        DomainValue::String(ending.as_str().to_owned()),
    );
    store_encounter_state(state_fields, &encounter_id, encounter)?;
    let (story_points_awarded, experience_awarded) =
        if ending == OpenAdventureEncounterEnding::Success {
            grant_encounter_rewards(state_fields, &document)?
        } else {
            (0, 0)
        };
    let mut ended_fields = object_fields([
        ("encounter_id", DomainValue::String(encounter_id.clone())),
        ("ending", DomainValue::Symbol(ending.as_str().to_owned())),
        (
            "experience_awarded",
            DomainValue::Number(f64::from(experience_awarded)),
        ),
        (
            "story_points_awarded",
            DomainValue::Number(f64::from(story_points_awarded)),
        ),
    ]);
    let mut events = Vec::new();
    if disclosure == OpenAdventureDisclosure::Public {
        events.push(public_event(
            "encounter_ended",
            DomainValue::Object(ended_fields.clone()),
        ));
    }
    ended_fields.insert(
        "disclosure".to_owned(),
        DomainValue::Symbol(disclosure.as_str().to_owned()),
    );
    events.push(authority_event(
        "encounter_ended_authority",
        DomainValue::Object(ended_fields),
    ));
    if story_points_awarded > 0 || experience_awarded > 0 {
        let mut reward_fields = object_fields([
            ("encounter_id", DomainValue::String(encounter_id)),
            (
                "experience",
                DomainValue::Number(f64::from(experience_awarded)),
            ),
            (
                "reward_ids",
                DomainValue::List(
                    document
                        .rewards
                        .iter()
                        .map(|reward| DomainValue::String(reward.id.clone()))
                        .collect(),
                ),
            ),
            (
                "story_points",
                DomainValue::Number(f64::from(story_points_awarded)),
            ),
        ]);
        if disclosure == OpenAdventureDisclosure::Public {
            events.push(public_event(
                "encounter_reward_granted",
                DomainValue::Object(reward_fields.clone()),
            ));
        }
        reward_fields.insert(
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.as_str().to_owned()),
        );
        events.push(authority_event(
            "encounter_reward_granted_authority",
            DomainValue::Object(reward_fields),
        ));
    }
    Ok((next, events))
}

fn resolve_heal_encounter_participant(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let derived = open_adventure_derived(definition)?;
    let input = value_object("input", request)?;
    let encounter_id = object_string(input, "encounter_id")?.to_owned();
    let participant_id = object_string(input, "participant_id")?.to_owned();
    let pool = parse_pool(object_symbol(input, "pool")?)?;
    let amount = object_u32(input, "amount")?;
    let disclosure = parse_disclosure(object_symbol(input, "disclosure")?)?;
    validate_local_identifier("input.encounter_id", &encounter_id)?;
    validate_local_identifier("input.participant_id", &participant_id)?;
    if !(1..=1_000).contains(&amount) {
        return Err(invalid(
            "input.amount",
            "expected a healing amount from 1 through 1000",
        ));
    }
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let mut encounter = encounter_state_clone(state_fields, &encounter_id)?;
    let document = encounter_document_from_state(&encounter)?;
    let participant = document
        .participants
        .iter()
        .find(|participant| participant.id == participant_id)
        .ok_or_else(|| {
            invalid(
                "input.participant_id",
                "participant is not part of the encounter document",
            )
        })?;
    let mut participant_state = participant_state(&encounter, &participant_id)?;
    let pool_field = pool.field_name();
    let current = object_u32(&participant_state, pool_field)?;
    let initial = match participant.kind {
        OpenAdventureParticipantKind::Character => match pool {
            OpenAdventurePool::Body => derived.body_points,
            OpenAdventurePool::Mind => derived.mind_points,
            OpenAdventurePool::Spirit => derived.spirit_points,
        },
        OpenAdventureParticipantKind::Adversary => match pool {
            OpenAdventurePool::Body => participant.body_points,
            OpenAdventurePool::Mind => participant.mind_points,
            OpenAdventurePool::Spirit => participant.spirit_points,
        },
    };
    let healed = current.saturating_add(amount).min(initial);
    set_u32(&mut participant_state, pool_field, healed);
    let mut removed_wound = String::new();
    if let Some(DomainValue::List(wounds)) = participant_state.get_mut("wounds")
        && let Some(index) = wounds.iter().position(|value| {
            matches!(value, DomainValue::String(id) if participant
                .wounds
                .iter()
                .any(|wound| &wound.id == id && wound.target == pool))
        })
    {
        if let Some(DomainValue::String(id)) = wounds.get(index).cloned() {
            removed_wound = id;
        }
        wounds.remove(index);
    }
    let was_incapacitated = object_bool(&participant_state, "incapacitated")?;
    let exhausted_pool = object_string(&participant_state, "exhausted_pool")?.to_owned();
    let all_pools_positive = object_u32(&participant_state, "body_points")? > 0
        && object_u32(&participant_state, "mind_points")? > 0
        && object_u32(&participant_state, "spirit_points")? > 0;
    let recovered = was_incapacitated
        && healed > 0
        && (if exhausted_pool.is_empty() {
            all_pools_positive
        } else {
            exhausted_pool == pool.as_str()
        });
    if recovered {
        participant_state.insert("incapacitated".to_owned(), DomainValue::Bool(false));
        participant_state.insert("defeated".to_owned(), DomainValue::Bool(false));
        participant_state.insert(
            "exhausted_pool".to_owned(),
            DomainValue::String(String::new()),
        );
    }
    store_participant_state(&mut encounter, &participant_id, participant_state)?;
    store_encounter_state(state_fields, &encounter_id, encounter)?;

    let mut healed_fields = object_fields([
        ("amount", DomainValue::Number(f64::from(amount))),
        (
            "participant_id",
            DomainValue::String(participant_id.clone()),
        ),
        ("pool", DomainValue::Symbol(pool.as_str().to_owned())),
        ("remaining_points", DomainValue::Number(f64::from(healed))),
    ]);
    let mut events = Vec::new();
    if disclosure == OpenAdventureDisclosure::Public {
        events.push(public_event(
            "participant_healed",
            DomainValue::Object(healed_fields.clone()),
        ));
    }
    healed_fields.insert(
        "disclosure".to_owned(),
        DomainValue::Symbol(disclosure.as_str().to_owned()),
    );
    events.push(authority_event(
        "participant_healed_authority",
        DomainValue::Object(healed_fields),
    ));
    if recovered || !removed_wound.is_empty() {
        let mut condition_fields = object_fields([
            ("actor_id", DomainValue::String(participant_id)),
            ("defeated", DomainValue::Bool(false)),
            ("incapacitated", DomainValue::Bool(false)),
            ("pool", DomainValue::Symbol(pool.as_str().to_owned())),
            ("remaining_points", DomainValue::Number(f64::from(healed))),
            ("wound_id", DomainValue::String(removed_wound)),
        ]);
        if disclosure == OpenAdventureDisclosure::Public {
            events.push(public_event(
                "encounter_condition_changed",
                DomainValue::Object(condition_fields.clone()),
            ));
        }
        condition_fields.insert(
            "disclosure".to_owned(),
            DomainValue::Symbol(disclosure.as_str().to_owned()),
        );
        events.push(authority_event(
            "encounter_condition_changed_authority",
            DomainValue::Object(condition_fields),
        ));
    }
    Ok((next, events))
}

fn resolve_advance_encounter_participant(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let input = value_object("input", request)?;
    let encounter_id = object_string(input, "encounter_id")?.to_owned();
    let participant_id = object_string(input, "participant_id")?.to_owned();
    let attribute = parse_attribute(object_symbol(input, "attribute")?)?;
    let experience_cost = object_u32(input, "experience_cost")?;
    let disclosure = parse_disclosure(object_symbol(input, "disclosure")?)?;
    validate_local_identifier("input.encounter_id", &encounter_id)?;
    validate_local_identifier("input.participant_id", &participant_id)?;
    if !(1..=1_000).contains(&experience_cost) {
        return Err(invalid(
            "input.experience_cost",
            "expected an experience cost from 1 through 1000",
        ));
    }
    let mut next = state.clone();
    let state_fields = value_object_mut("state", &mut next)?;
    let encounter = encounter_state_clone(state_fields, &encounter_id)?;
    let document = encounter_document_from_state(&encounter)?;
    let participant = document
        .participants
        .iter()
        .find(|participant| participant.id == participant_id)
        .ok_or_else(|| {
            invalid(
                "input.participant_id",
                "participant is not part of the encounter document",
            )
        })?;
    if participant.kind != OpenAdventureParticipantKind::Character {
        return Err(invalid(
            "input.participant_id",
            "advancement applies only to character participants",
        ));
    }
    let advancement = object_map_mut(state_fields, "advancement")?;
    let experience = object_u32(advancement, "experience")?;
    if experience < experience_cost {
        return Err(invalid(
            "input.experience_cost",
            "insufficient experience for this advancement",
        ));
    }
    let spent = object_u32(advancement, "spent")?.saturating_add(experience_cost);
    let rank = object_u32(advancement, "rank")?.saturating_add(1);
    set_u32(advancement, "experience", experience - experience_cost);
    set_u32(advancement, "spent", spent);
    set_u32(advancement, "rank", rank);
    let bonuses = object_map_mut(advancement, "attribute_bonuses")?;
    let current = optional_u32(bonuses, attribute.as_str())?.unwrap_or(0);
    let updated = current
        .checked_add(1)
        .ok_or_else(|| invalid("state.advancement.attribute_bonuses", "bonus overflow"))?;
    bonuses.insert(
        attribute.as_str().to_owned(),
        DomainValue::Number(f64::from(updated)),
    );
    let remaining_experience = experience - experience_cost;
    let mut recorded_fields = object_fields([
        (
            "attribute",
            DomainValue::Symbol(attribute.as_str().to_owned()),
        ),
        (
            "experience_cost",
            DomainValue::Number(f64::from(experience_cost)),
        ),
        (
            "participant_id",
            DomainValue::String(participant_id.clone()),
        ),
        ("rank", DomainValue::Number(f64::from(rank))),
        (
            "remaining_experience",
            DomainValue::Number(f64::from(remaining_experience)),
        ),
    ]);
    let mut events = Vec::new();
    if disclosure == OpenAdventureDisclosure::Public {
        events.push(public_event(
            "advancement_recorded",
            DomainValue::Object(recorded_fields.clone()),
        ));
    }
    recorded_fields.insert(
        "disclosure".to_owned(),
        DomainValue::Symbol(disclosure.as_str().to_owned()),
    );
    events.push(authority_event(
        "advancement_recorded_authority",
        DomainValue::Object(recorded_fields),
    ));
    Ok((next, events))
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
        ("Advancement".to_owned(), advancement_type()),
        ("Attributes".to_owned(), attributes_type()),
        ("Background".to_owned(), origin_type()),
        ("Benefit".to_owned(), benefit_type()),
        ("Challenge".to_owned(), challenge_type()),
        ("Culture".to_owned(), origin_type()),
        ("Drawback".to_owned(), drawback_type()),
        ("DieResult".to_owned(), die_result_type()),
        (
            "EncounterChallengeState".to_owned(),
            encounter_challenge_state_type(),
        ),
        (
            "EncounterParticipantState".to_owned(),
            encounter_participant_state_type(),
        ),
        ("EncounterState".to_owned(), encounter_state_type()),
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
            capability(TabletopCapability::Advancement),
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
        (
            "start_encounter".to_owned(),
            operation(
                "start_encounter",
                "Start an encounter",
                "Validate a namespaced encounter document and record its initial participant, challenge, and turn state.",
                TabletopCapability::Encounters,
                object_type([
                    (
                        "disclosure",
                        required(disclosure_type(), "Event visibility."),
                    ),
                    (
                        "document_json",
                        required(string(1, 65_536), "Canonical encounter document JSON."),
                    ),
                    (
                        "document_sha256",
                        required(
                            string(64, 64),
                            "SHA-256 of the canonical validated document.",
                        ),
                    ),
                    (
                        "encounter_id",
                        required(string(1, 128), "Stable encounter identity."),
                    ),
                ]),
                ["encounter_started", "encounter_started_authority"],
                false,
            ),
        ),
        (
            "resolve_encounter_action".to_owned(),
            operation(
                "resolve_encounter_action",
                "Resolve an encounter action",
                "Roll exploding action dice for the current actor, apply damage or Challenge Point loss, advance turn order, and record any ending.",
                TabletopCapability::Encounters,
                object_type([
                    (
                        "action",
                        required(action_kind_type(), "Attack or spell action."),
                    ),
                    (
                        "actor_id",
                        required(string(1, 128), "Current actor identity."),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Event visibility."),
                    ),
                    (
                        "encounter_id",
                        required(string(1, 128), "Active encounter identity."),
                    ),
                    (
                        "equipment_id",
                        optional(string(1, 128), "Optional weapon identity."),
                    ),
                    (
                        "situation_modifier",
                        required(integer(-10.0, 10.0), "Authored situational adjustment."),
                    ),
                    (
                        "spell_id",
                        optional(string(1, 128), "Optional spell identity."),
                    ),
                    (
                        "spend_story_point",
                        required(TypeExpression::Bool, "Whether to spend one Story Point."),
                    ),
                    (
                        "target_id",
                        required(string(1, 128), "Participant or challenge target."),
                    ),
                    (
                        "target_kind",
                        required(target_kind_type(), "Target category."),
                    ),
                ]),
                [
                    "encounter_action_resolved",
                    "encounter_action_resolved_authority",
                    "encounter_condition_changed",
                    "encounter_condition_changed_authority",
                    "encounter_ended",
                    "encounter_ended_authority",
                    "encounter_reward_granted",
                    "encounter_reward_granted_authority",
                    "entropy_trace",
                ],
                true,
            ),
        ),
        (
            "end_encounter".to_owned(),
            operation(
                "end_encounter",
                "End an encounter",
                "Record an explicit success, withdrawal, defeat, or host decision and grant success rewards.",
                TabletopCapability::Encounters,
                object_type([
                    (
                        "disclosure",
                        required(disclosure_type(), "Event visibility."),
                    ),
                    (
                        "encounter_id",
                        required(string(1, 128), "Active encounter identity."),
                    ),
                    (
                        "ending",
                        required(encounter_ending_type(), "Recorded ending."),
                    ),
                ]),
                [
                    "encounter_ended",
                    "encounter_ended_authority",
                    "encounter_reward_granted",
                    "encounter_reward_granted_authority",
                ],
                false,
            ),
        ),
        (
            "heal_encounter_participant".to_owned(),
            operation(
                "heal_encounter_participant",
                "Heal an encounter participant",
                "Restore one Body, Mind, or Spirit pool, remove a matching wound, and clear incapacitation.",
                TabletopCapability::ResourcesAndConditions,
                object_type([
                    (
                        "amount",
                        required(integer(1.0, 1_000.0), "Points restored."),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Event visibility."),
                    ),
                    (
                        "encounter_id",
                        required(string(1, 128), "Encounter identity."),
                    ),
                    (
                        "participant_id",
                        required(string(1, 128), "Participant identity."),
                    ),
                    ("pool", required(pool_type(), "Pool restored.")),
                ]),
                [
                    "encounter_condition_changed",
                    "encounter_condition_changed_authority",
                    "participant_healed",
                    "participant_healed_authority",
                ],
                false,
            ),
        ),
        (
            "advance_encounter_participant".to_owned(),
            operation(
                "advance_encounter_participant",
                "Advance an encounter participant",
                "Spend experience to increase one attribute bonus and record the advancement.",
                TabletopCapability::Advancement,
                object_type([
                    (
                        "attribute",
                        required(attribute_type(), "Attribute advanced."),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Event visibility."),
                    ),
                    (
                        "encounter_id",
                        required(string(1, 128), "Encounter identity."),
                    ),
                    (
                        "experience_cost",
                        required(integer(1.0, 1_000.0), "Experience spent."),
                    ),
                    (
                        "participant_id",
                        required(string(1, 128), "Character participant identity."),
                    ),
                ]),
                ["advancement_recorded", "advancement_recorded_authority"],
                false,
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
        (
            "advancement_recorded".to_owned(),
            advancement_recorded_type(),
        ),
        (
            "advancement_recorded_authority".to_owned(),
            extend_object_type(
                advancement_recorded_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        (
            "encounter_action_resolved".to_owned(),
            encounter_action_resolved_type(),
        ),
        (
            "encounter_action_resolved_authority".to_owned(),
            extend_object_type(
                encounter_action_resolved_type(),
                [
                    (
                        "advancement_bonus",
                        required(integer(0.0, 100.0), "Applied advancement bonus."),
                    ),
                    (
                        "attack_bonus",
                        required(integer(-1_000.0, 1_000.0), "Actor action base."),
                    ),
                    (
                        "disclosure",
                        required(disclosure_type(), "Declared event visibility."),
                    ),
                    (
                        "equipment_bonus",
                        required(integer(-100.0, 100.0), "Weapon attack bonus."),
                    ),
                    (
                        "equipment_id",
                        required(string(0, 128), "Weapon identity or empty."),
                    ),
                    (
                        "situation_modifier",
                        required(integer(-10.0, 10.0), "Authored situational adjustment."),
                    ),
                    (
                        "spell_cost",
                        required(integer(0.0, 1_000.0), "Story Point spell cost."),
                    ),
                    (
                        "spell_id",
                        required(string(0, 128), "Spell identity or empty."),
                    ),
                    (
                        "story_point_spent",
                        required(TypeExpression::Bool, "Whether a Story Point was spent."),
                    ),
                    (
                        "story_points_after",
                        required(integer(0.0, 1_000.0), "Remaining Story Points."),
                    ),
                ],
            ),
        ),
        (
            "encounter_condition_changed".to_owned(),
            encounter_condition_changed_type(),
        ),
        (
            "encounter_condition_changed_authority".to_owned(),
            extend_object_type(
                encounter_condition_changed_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        ("encounter_ended".to_owned(), encounter_ended_type()),
        (
            "encounter_ended_authority".to_owned(),
            extend_object_type(
                encounter_ended_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        (
            "encounter_reward_granted".to_owned(),
            encounter_reward_granted_type(),
        ),
        (
            "encounter_reward_granted_authority".to_owned(),
            extend_object_type(
                encounter_reward_granted_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        ("encounter_started".to_owned(), encounter_started_type()),
        (
            "encounter_started_authority".to_owned(),
            extend_object_type(
                encounter_started_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
        ),
        ("participant_healed".to_owned(), participant_healed_type()),
        (
            "participant_healed_authority".to_owned(),
            extend_object_type(
                participant_healed_type(),
                [(
                    "disclosure",
                    required(disclosure_type(), "Declared event visibility."),
                )],
            ),
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

fn encounter_started_type() -> TypeExpression {
    object_type([
        (
            "challenge_ids",
            required(
                list(string(1, 128), 0, MAX_ENCOUNTER_CHALLENGES),
                "Challenge identities in document order.",
            ),
        ),
        (
            "encounter_id",
            required(string(1, 128), "Encounter identity."),
        ),
        ("goal", required(string(1, 512), "Encounter goal.")),
        ("label", required(string(1, 160), "Encounter label.")),
        (
            "participant_ids",
            required(
                list(string(1, 128), 0, MAX_ENCOUNTER_PARTICIPANTS),
                "Participant identities in document order.",
            ),
        ),
        (
            "turn_order",
            required(
                list(string(1, 128), 0, MAX_ENCOUNTER_PARTICIPANTS),
                "Turn order participant identities.",
            ),
        ),
    ])
}

fn encounter_action_resolved_type() -> TypeExpression {
    object_type([
        ("action", required(action_kind_type(), "Attack or spell.")),
        (
            "action_score",
            required(integer(-1_000.0, 1_000.0), "Final Action Score."),
        ),
        ("actor_id", required(string(1, 128), "Actor identity.")),
        (
            "challenge_points_after",
            required(integer(0.0, 1_000_000.0), "Challenge Points after."),
        ),
        ("damage", required(integer(0.0, 1_000.0), "Applied damage.")),
        (
            "dice",
            required(
                list(named("DieResult"), 1, 5),
                "Exploding action dice in roll order.",
            ),
        ),
        (
            "ended",
            required(TypeExpression::Bool, "Whether the encounter ended."),
        ),
        (
            "ending",
            required(string(0, 32), "Recorded ending or empty."),
        ),
        ("outcome", required(outcome_type(), "Success or failure.")),
        (
            "remaining_points",
            required(integer(0.0, 1_000.0), "Target pool after damage."),
        ),
        (
            "roll_total",
            required(integer(1.0, 500.0), "Sum of every exploding die."),
        ),
        ("target_id", required(string(1, 128), "Target identity.")),
        (
            "target_kind",
            required(target_kind_type(), "Target category."),
        ),
        (
            "target_number",
            required(integer(1.0, 1_000.0), "Target number."),
        ),
        (
            "turn_index",
            required(integer(0.0, 63.0), "Next turn order index."),
        ),
    ])
}

fn encounter_condition_changed_type() -> TypeExpression {
    object_type([
        ("actor_id", required(string(1, 128), "Condition target.")),
        ("defeated", required(TypeExpression::Bool, "Defeat flag.")),
        (
            "incapacitated",
            required(TypeExpression::Bool, "Incapacitation flag."),
        ),
        ("pool", required(pool_type(), "Affected pool.")),
        (
            "remaining_points",
            required(integer(0.0, 1_000.0), "Remaining pool points."),
        ),
        (
            "wound_id",
            required(string(0, 128), "Applied wound or empty."),
        ),
    ])
}

fn encounter_ended_type() -> TypeExpression {
    object_type([
        (
            "encounter_id",
            required(string(1, 128), "Encounter identity."),
        ),
        (
            "ending",
            required(encounter_ending_type(), "Recorded ending."),
        ),
        (
            "experience_awarded",
            required(integer(0.0, 1_000_000.0), "Experience awarded."),
        ),
        (
            "story_points_awarded",
            required(integer(0.0, 1_000_000.0), "Story Points awarded."),
        ),
    ])
}

fn encounter_reward_granted_type() -> TypeExpression {
    object_type([
        (
            "encounter_id",
            required(string(1, 128), "Encounter identity."),
        ),
        (
            "experience",
            required(integer(0.0, 1_000_000.0), "Experience granted."),
        ),
        (
            "reward_ids",
            required(
                list(string(1, 128), 0, MAX_ENCOUNTER_REWARDS),
                "Reward identities granted.",
            ),
        ),
        (
            "story_points",
            required(integer(0.0, 1_000_000.0), "Story Points granted."),
        ),
    ])
}

fn participant_healed_type() -> TypeExpression {
    object_type([
        (
            "amount",
            required(integer(1.0, 1_000.0), "Points restored."),
        ),
        (
            "participant_id",
            required(string(1, 128), "Participant identity."),
        ),
        ("pool", required(pool_type(), "Restored pool.")),
        (
            "remaining_points",
            required(integer(0.0, 1_000.0), "Pool after healing."),
        ),
    ])
}

fn advancement_recorded_type() -> TypeExpression {
    object_type([
        (
            "attribute",
            required(attribute_type(), "Advanced attribute."),
        ),
        (
            "experience_cost",
            required(integer(1.0, 1_000.0), "Experience spent."),
        ),
        (
            "participant_id",
            required(string(1, 128), "Participant identity."),
        ),
        ("rank", required(integer(0.0, 1_000.0), "Advancement rank.")),
        (
            "remaining_experience",
            required(integer(0.0, 1_000_000.0), "Unspent experience."),
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
            "advancement",
            required(named("Advancement"), "Persistent advancement state."),
        ),
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
            "encounters",
            required(
                map(named("EncounterState"), 0, MAX_ENCOUNTERS),
                "Active and completed encounters.",
            ),
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

fn advancement_type() -> TypeExpression {
    object_type([
        (
            "attribute_bonuses",
            required(
                map(integer(0.0, 100.0), 0, MAX_SPECIALIZATIONS),
                "Advancement bonuses by attribute.",
            ),
        ),
        (
            "experience",
            required(integer(0.0, 1_000_000.0), "Unspent experience."),
        ),
        ("rank", required(integer(0.0, 1_000.0), "Advancement rank.")),
        (
            "spent",
            required(integer(0.0, 1_000_000.0), "Spent experience."),
        ),
        (
            "story_points",
            required(integer(0.0, 1_000_000.0), "Persistent Story Points."),
        ),
    ])
}

fn encounter_participant_state_type() -> TypeExpression {
    object_type([
        (
            "body_points",
            required(integer(0.0, 1_000.0), "Current Body Points."),
        ),
        (
            "defeated",
            required(
                TypeExpression::Bool,
                "Whether this participant is defeated.",
            ),
        ),
        (
            "exhausted_pool",
            required(string(0, 16), "Exhausted pool name, or empty."),
        ),
        (
            "incapacitated",
            required(TypeExpression::Bool, "Whether this participant cannot act."),
        ),
        (
            "mind_points",
            required(integer(0.0, 1_000.0), "Current Mind Points."),
        ),
        (
            "spirit_points",
            required(integer(0.0, 1_000.0), "Current Spirit Points."),
        ),
        (
            "story_points",
            required(integer(0.0, 1_000.0), "Current Story Points."),
        ),
        (
            "wounds",
            required(
                list(string(1, 128), 0, MAX_DETAIL_ENTRIES),
                "Applied wound identities.",
            ),
        ),
    ])
}

fn encounter_challenge_state_type() -> TypeExpression {
    object_type([
        (
            "completed",
            required(TypeExpression::Bool, "Whether the challenge completed."),
        ),
        (
            "remaining_challenge_points",
            required(integer(0.0, 1_000_000.0), "Remaining Challenge Points."),
        ),
    ])
}

fn encounter_state_type() -> TypeExpression {
    object_type([
        (
            "challenges",
            required(
                map(
                    named("EncounterChallengeState"),
                    0,
                    MAX_ENCOUNTER_CHALLENGES,
                ),
                "Mutable encounter challenge state.",
            ),
        ),
        (
            "document",
            required(string(1, 65_536), "Canonical JSON authoring document."),
        ),
        (
            "ended",
            required(TypeExpression::Bool, "Whether the encounter ended."),
        ),
        (
            "ending",
            required(string(0, 32), "Recorded encounter ending."),
        ),
        (
            "participants",
            required(
                map(
                    named("EncounterParticipantState"),
                    0,
                    MAX_ENCOUNTER_PARTICIPANTS,
                ),
                "Mutable participant state.",
            ),
        ),
        (
            "turn_index",
            required(integer(0.0, 63.0), "Current turn order index."),
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

fn action_kind_type() -> TypeExpression {
    symbol(["attack", "spell"])
}

fn target_kind_type() -> TypeExpression {
    symbol(["challenge", "participant"])
}

fn encounter_ending_type() -> TypeExpression {
    symbol(["defeat", "host_decision", "success", "withdrawal"])
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

fn object_list<'a>(
    fields: &'a BTreeMap<String, DomainValue>,
    field: &str,
) -> Result<&'a Vec<DomainValue>, TabletopError> {
    match object_field(fields, field)? {
        DomainValue::List(values) => Ok(values),
        _ => Err(invalid(field, "expected a list")),
    }
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
