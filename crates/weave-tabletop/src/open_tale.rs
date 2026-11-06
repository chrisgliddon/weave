//! Optional tabletop adapter: Open Tale.
//!
//! This is an independent implementation of the public Open Tale rules reference published at
//! <https://open-tale.github.io/> under the Apache License 2.0. It is not a copy of upstream
//! text, artwork, fonts, or layout, and it is not endorsed by the Open Tale project.
//!
//! Provenance is recorded in [`OPEN_TALE_SOURCE_RECORD`]. The manifest pins the SHA-256 of that
//! record, not the digest of a downloaded file: the record states the canonical URL, the
//! implemented edition, and the licence. [`OPEN_TALE_LICENSE_TEXT`] retains the complete
//! Apache-2.0 text, and [`OPEN_TALE_LICENSE_TEXT_SHA256`] pins its exact bytes.
//!
//! Characters are authored from a fixed attribute budget, a bounded skill selection, and a
//! wealth budget. Checks use deterministic two-d6 resolution through [`EntropyStream`]. All
//! state mutations are typed portable values and every resolver operation is declared in
//! [`open_tale_manifest`].

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
    TabletopState, canonical_fingerprint, resolved_adapter, validate_adapter_manifest,
    validate_tabletop_state,
};

/// Stable adapter identity reserved for the Open Tale adapter.
pub const OPEN_TALE_ADAPTER_ID: &str = "org.weave.tabletop.open_tale";
/// First independently modelled adapter release for the public Open Tale rules reference.
pub const OPEN_TALE_ADAPTER_VERSION: &str = "1.0.0";
/// Versioned creation request and preview format.
pub const OPEN_TALE_CREATION_FORMAT_VERSION: u32 = 1;
/// Canonical public website for the reviewed Open Tale rules reference.
pub const OPEN_TALE_SOURCE_URL: &str = "https://open-tale.github.io/";
/// Public licence declared for the reviewed rules reference.
pub const OPEN_TALE_LICENSE: &str = "Apache-2.0";
/// Edition identifier implemented by this adapter release.
pub const OPEN_TALE_EDITION: &str = "public rules reference retrieved 2026-11-06";
/// Canonical Apache-2.0 licence URL.
pub const OPEN_TALE_LICENSE_URL: &str = "https://www.apache.org/licenses/LICENSE-2.0";

/// Exact public-source statement whose SHA-256 the manifest pins.
///
/// This record is authored for Weave. It is not a downloaded upstream artifact, and the digest
/// therefore covers this string rather than a claimed byte-for-byte copy of the website.
pub const OPEN_TALE_SOURCE_RECORD: &str = concat!(
    "Open Tale public rules reference\n",
    "source: https://open-tale.github.io/\n",
    "license: Apache-2.0\n",
    "edition: public rules reference retrieved 2026-11-06\n",
    "implementation: independent Weave adapter; no upstream text, artwork, fonts, or layout is redistributed\n",
);
/// SHA-256 of [`OPEN_TALE_SOURCE_RECORD`] as UTF-8 bytes.
pub const OPEN_TALE_SOURCE_SHA256: &str =
    "17a3f9dd52fc291ec10757fd8c72f9a8423e09f582ad87e24820377d15fb3270";

/// Starting number of points allocated across the nine attributes.
pub const OPEN_TALE_ATTRIBUTE_BUDGET: u32 = 18;
/// Maximum rating for one starting attribute.
pub const OPEN_TALE_ATTRIBUTE_MAX: u32 = 5;
/// Maximum number of selected skills during creation.
pub const OPEN_TALE_MAX_SKILLS: usize = 4;
/// Maximum total skill ranks during creation.
pub const OPEN_TALE_MAX_SKILL_RANKS: u32 = 6;
/// Largest starting wealth value accepted by creation.
pub const OPEN_TALE_MAX_STARTING_WEALTH: u32 = 20;
/// Largest number of starting items accepted by creation.
pub const OPEN_TALE_MAX_ITEMS: usize = 8;
/// Maximum number of heroic badges a character may hold.
pub const OPEN_TALE_MAX_HEROIC_BADGES: u32 = 3;
/// Maximum number of marks a character may carry.
pub const OPEN_TALE_MAX_MARKS: u32 = 3;
/// Experience required for each level above the first.
pub const OPEN_TALE_EXPERIENCE_PER_LEVEL: u32 = 10;
/// Highest level this adapter release models.
pub const OPEN_TALE_MAX_LEVEL: u32 = 20;
/// Largest experience total this adapter release accepts.
pub const OPEN_TALE_MAX_EXPERIENCE: u32 = 1_000_000;
/// Maximum number of challenges retained in state.
pub const OPEN_TALE_MAX_CHALLENGES: usize = 128;
/// Maximum number of combats retained in state.
pub const OPEN_TALE_MAX_COMBATS: usize = 64;

/// One of the nine Open Tale attributes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleAttribute {
    Dexterity,
    Intelligence,
    Magicy,
    Motion,
    Perception,
    Persuasion,
    Strength,
    Vitality,
    Volition,
}

/// Active skills add their rank to matching checks; passive skills add to Defense.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleSkillKind {
    Active,
    Passive,
}

/// Broad equipment category used only for bounded gear selection.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleItemKind {
    Armor,
    Gear,
    Tool,
    Weapon,
}

/// Deterministic dice-generation mode for one attribute check.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleRollMode {
    Advantage,
    Disadvantage,
    Normal,
}

/// Documented result band for one check.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleOutcomeBand {
    Critical,
    Failure,
    Partial,
    Success,
}

/// Lifecycle of one recorded challenge.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleChallengeStatus {
    Failed,
    Open,
    Succeeded,
}

/// Lifecycle of one recorded combat.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleCombatStatus {
    Concluded,
    Open,
}

/// Recovery procedure applied by one resolver operation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleRecoveryMode {
    Rest,
    Treat,
}

/// One of the five derived qualities.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OpenTaleQuality {
    Capacity,
    Defense,
    HitPoints,
    Resolve,
    Toughness,
}

/// The nine authored attribute ratings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleAttributes {
    pub dexterity: u32,
    pub intelligence: u32,
    pub magicy: u32,
    pub motion: u32,
    pub perception: u32,
    pub persuasion: u32,
    pub strength: u32,
    pub vitality: u32,
    pub volition: u32,
}

impl OpenTaleAttributes {
    /// Read one attribute by its closed enum value.
    #[must_use]
    pub const fn get(&self, attribute: OpenTaleAttribute) -> u32 {
        match attribute {
            OpenTaleAttribute::Dexterity => self.dexterity,
            OpenTaleAttribute::Intelligence => self.intelligence,
            OpenTaleAttribute::Magicy => self.magicy,
            OpenTaleAttribute::Motion => self.motion,
            OpenTaleAttribute::Perception => self.perception,
            OpenTaleAttribute::Persuasion => self.persuasion,
            OpenTaleAttribute::Strength => self.strength,
            OpenTaleAttribute::Vitality => self.vitality,
            OpenTaleAttribute::Volition => self.volition,
        }
    }

    /// Sum of all nine ratings.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.dexterity
            + self.intelligence
            + self.magicy
            + self.motion
            + self.perception
            + self.persuasion
            + self.strength
            + self.vitality
            + self.volition
    }

    /// Reject a rating outside the reviewed starting range.
    pub fn validate(&self) -> Result<(), TabletopError> {
        if self.total() > OPEN_TALE_ATTRIBUTE_BUDGET {
            return Err(invalid(
                "attributes",
                "attributes overspend the starting attribute budget",
            ));
        }
        for (name, value) in self.entries() {
            if value > OPEN_TALE_ATTRIBUTE_MAX {
                return Err(invalid(
                    format!("attributes.{name}"),
                    "attribute exceeds the starting maximum",
                ));
            }
        }
        Ok(())
    }

    fn entries(&self) -> [(&'static str, u32); 9] {
        [
            ("dexterity", self.dexterity),
            ("intelligence", self.intelligence),
            ("magicy", self.magicy),
            ("motion", self.motion),
            ("perception", self.perception),
            ("persuasion", self.persuasion),
            ("strength", self.strength),
            ("vitality", self.vitality),
            ("volition", self.volition),
        ]
    }
}

/// One entry in the fixed skill catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleSkillDefinition {
    pub attribute: OpenTaleAttribute,
    pub kind: OpenTaleSkillKind,
    pub label: String,
    pub rank: u32,
}

/// One entry in the fixed equipment catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleItemDefinition {
    pub capacity_cost: u32,
    pub damage_bonus: u32,
    pub defense_bonus: u32,
    pub kind: OpenTaleItemKind,
    pub label: String,
    pub wealth_cost: u32,
}

/// One addend in a declared quality formula.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleQualityTerm {
    pub attribute: OpenTaleAttribute,
    pub factor: u32,
}

/// Inspectable integer formula for one derived quality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleQualityFormula {
    pub base: u32,
    pub expression: String,
    pub includes_passive_skills: bool,
    pub quality: OpenTaleQuality,
    pub terms: Vec<OpenTaleQualityTerm>,
}

/// The five derived quality values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleQualities {
    pub capacity: u32,
    pub defense: u32,
    pub hit_points: u32,
    pub resolve: u32,
    pub toughness: u32,
}

/// One skill selected during creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleSkillSelection {
    pub id: String,
    pub rank: u32,
}

/// Complete, versioned input to deterministic Open Tale creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleCreationRequest {
    pub attributes: OpenTaleAttributes,
    pub character_id: String,
    pub creation_format_version: u32,
    pub experience: u32,
    pub heroic_badges: u32,
    pub items: Vec<String>,
    pub level: u32,
    pub name: String,
    pub seed: u64,
    pub skills: Vec<OpenTaleSkillSelection>,
    pub starting_wealth: u32,
}

/// Explainable result of creation before projection or play.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTaleCreationPreview {
    pub adapter: ResolvedAdapter,
    pub creation_format_version: u32,
    pub definition: AdapterCharacterDefinition,
    pub explanations: Vec<String>,
    pub initial_state: TabletopState,
    pub item_ids: Vec<String>,
    pub qualities: OpenTaleQualities,
    pub request_sha256: String,
    pub skill_ids: Vec<String>,
    pub wealth_remaining: u32,
    pub wealth_spent: u32,
}

/// Exact input to one deterministic attribute check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTaleRollInput {
    pub attribute_value: u32,
    pub difficulty: u32,
    pub distance: u32,
    pub heroic_badge: bool,
    pub mark_penalty: u32,
    pub mode: OpenTaleRollMode,
    pub modifier: i32,
    pub skill_bonus: u32,
}

/// Deterministic dice trace and documented band for one check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTaleRollResult {
    pub band: OpenTaleOutcomeBand,
    pub dice: Vec<u32>,
    pub dice_total: u32,
    pub heroic_badge_spent: bool,
    pub kept: Vec<u32>,
    pub total: i32,
}

macro_rules! impl_open_tale_io {
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

impl_open_tale_io!(OpenTaleCreationRequest);
impl_open_tale_io!(OpenTaleCreationPreview);

/// Canonical JSON Schema for Open Tale creation requests.
pub fn open_tale_creation_request_schema() -> Result<String, TabletopError> {
    open_tale_schema::<OpenTaleCreationRequest>(
        "urn:weave:schema:tabletop-open-tale-creation-request:1",
        "Weave Open Tale Creation Request v1",
    )
}

/// Canonical JSON Schema for explainable Open Tale creation previews.
pub fn open_tale_creation_preview_schema() -> Result<String, TabletopError> {
    open_tale_schema::<OpenTaleCreationPreview>(
        "urn:weave:schema:tabletop-open-tale-creation-preview:1",
        "Weave Open Tale Creation Preview v1",
    )
}

fn open_tale_schema<T: JsonSchema>(id: &str, title: &str) -> Result<String, TabletopError> {
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

/// The fixed skill catalog used by this adapter release.
#[must_use]
pub fn open_tale_skill_catalog() -> BTreeMap<String, OpenTaleSkillDefinition> {
    BTreeMap::from([
        (
            "athletics".to_owned(),
            skill(
                "Athletics",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Strength,
            ),
        ),
        (
            "channeling".to_owned(),
            skill(
                "Channeling",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Magicy,
            ),
        ),
        (
            "composure".to_owned(),
            skill(
                "Composure",
                OpenTaleSkillKind::Passive,
                OpenTaleAttribute::Volition,
            ),
        ),
        (
            "diplomacy".to_owned(),
            skill(
                "Diplomacy",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Persuasion,
            ),
        ),
        (
            "endurance".to_owned(),
            skill(
                "Endurance",
                OpenTaleSkillKind::Passive,
                OpenTaleAttribute::Vitality,
            ),
        ),
        (
            "grit".to_owned(),
            skill(
                "Grit",
                OpenTaleSkillKind::Passive,
                OpenTaleAttribute::Vitality,
            ),
        ),
        (
            "insight".to_owned(),
            skill(
                "Insight",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Perception,
            ),
        ),
        (
            "lore".to_owned(),
            skill(
                "Lore",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Intelligence,
            ),
        ),
        (
            "medicine".to_owned(),
            skill(
                "Medicine",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Intelligence,
            ),
        ),
        (
            "quick_reflexes".to_owned(),
            skill(
                "Quick Reflexes",
                OpenTaleSkillKind::Passive,
                OpenTaleAttribute::Motion,
            ),
        ),
        (
            "stealth".to_owned(),
            skill(
                "Stealth",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Motion,
            ),
        ),
        (
            "survival".to_owned(),
            skill(
                "Survival",
                OpenTaleSkillKind::Active,
                OpenTaleAttribute::Perception,
            ),
        ),
    ])
}

fn skill(
    label: &str,
    kind: OpenTaleSkillKind,
    attribute: OpenTaleAttribute,
) -> OpenTaleSkillDefinition {
    OpenTaleSkillDefinition {
        attribute,
        kind,
        label: label.to_owned(),
        rank: 0,
    }
}

/// The fixed equipment catalog used by this adapter release.
#[must_use]
pub fn open_tale_item_catalog() -> BTreeMap<String, OpenTaleItemDefinition> {
    BTreeMap::from([
        (
            "field_kit".to_owned(),
            item("Field Kit", OpenTaleItemKind::Tool, 2, 0, 1, 1),
        ),
        (
            "hunting_bow".to_owned(),
            item("Hunting Bow", OpenTaleItemKind::Weapon, 3, 2, 0, 1),
        ),
        (
            "lantern".to_owned(),
            item("Lantern", OpenTaleItemKind::Gear, 1, 0, 0, 1),
        ),
        (
            "quarter_staff".to_owned(),
            item("Quarter Staff", OpenTaleItemKind::Weapon, 2, 1, 1, 1),
        ),
        (
            "rope".to_owned(),
            item("Rope", OpenTaleItemKind::Tool, 1, 0, 0, 1),
        ),
        (
            "round_shield".to_owned(),
            item("Round Shield", OpenTaleItemKind::Armor, 3, 0, 2, 2),
        ),
        (
            "short_blade".to_owned(),
            item("Short Blade", OpenTaleItemKind::Weapon, 3, 2, 0, 1),
        ),
        (
            "travel_cloak".to_owned(),
            item("Travel Cloak", OpenTaleItemKind::Armor, 1, 0, 1, 1),
        ),
    ])
}

fn item(
    label: &str,
    kind: OpenTaleItemKind,
    wealth_cost: u32,
    damage_bonus: u32,
    defense_bonus: u32,
    capacity_cost: u32,
) -> OpenTaleItemDefinition {
    OpenTaleItemDefinition {
        capacity_cost,
        damage_bonus,
        defense_bonus,
        kind,
        label: label.to_owned(),
        wealth_cost,
    }
}

/// Every derived-quality formula, in stable order.
#[must_use]
pub fn open_tale_quality_formulas() -> Vec<OpenTaleQualityFormula> {
    vec![
        formula(
            OpenTaleQuality::HitPoints,
            "4 + vitality * 2",
            4,
            &[(OpenTaleAttribute::Vitality, 2)],
            false,
        ),
        formula(
            OpenTaleQuality::Capacity,
            "2 + strength * 2",
            2,
            &[(OpenTaleAttribute::Strength, 2)],
            false,
        ),
        formula(
            OpenTaleQuality::Defense,
            "3 + motion + passive skill ranks",
            3,
            &[(OpenTaleAttribute::Motion, 1)],
            true,
        ),
        formula(
            OpenTaleQuality::Toughness,
            "2 + vitality + strength",
            2,
            &[
                (OpenTaleAttribute::Vitality, 1),
                (OpenTaleAttribute::Strength, 1),
            ],
            false,
        ),
        formula(
            OpenTaleQuality::Resolve,
            "2 + volition * 2",
            2,
            &[(OpenTaleAttribute::Volition, 2)],
            false,
        ),
    ]
}

fn formula(
    quality: OpenTaleQuality,
    expression: &str,
    base: u32,
    terms: &[(OpenTaleAttribute, u32)],
    includes_passive_skills: bool,
) -> OpenTaleQualityFormula {
    OpenTaleQualityFormula {
        base,
        expression: expression.to_owned(),
        includes_passive_skills,
        quality,
        terms: terms
            .iter()
            .map(|(attribute, factor)| OpenTaleQualityTerm {
                attribute: *attribute,
                factor: *factor,
            })
            .collect(),
    }
}

/// Evaluate the declared integer formulas for one attribute set.
#[must_use]
pub fn open_tale_compute_qualities(
    attributes: &OpenTaleAttributes,
    passive_skill_ranks: u32,
) -> OpenTaleQualities {
    let formulas = open_tale_quality_formulas();
    let value = |quality: OpenTaleQuality| {
        let Some(formula) = formulas.iter().find(|formula| formula.quality == quality) else {
            return 0;
        };
        let mut total = formula.base;
        for term in &formula.terms {
            total += attributes.get(term.attribute) * term.factor;
        }
        if formula.includes_passive_skills {
            total += passive_skill_ranks;
        }
        total
    };
    OpenTaleQualities {
        capacity: value(OpenTaleQuality::Capacity),
        defense: value(OpenTaleQuality::Defense),
        hit_points: value(OpenTaleQuality::HitPoints),
        resolve: value(OpenTaleQuality::Resolve),
        toughness: value(OpenTaleQuality::Toughness),
    }
}

/// Map one total to the documented outcome bands.
///
/// Critical is five or more above difficulty, Success is at or above difficulty, Partial is
/// within two below difficulty, and anything lower is a Failure.
#[must_use]
pub fn open_tale_outcome_band(total: i32, difficulty: u32) -> OpenTaleOutcomeBand {
    let total = i64::from(total);
    let difficulty = i64::from(difficulty);
    if total >= difficulty + 5 {
        OpenTaleOutcomeBand::Critical
    } else if total >= difficulty {
        OpenTaleOutcomeBand::Success
    } else if total >= difficulty - 2 {
        OpenTaleOutcomeBand::Partial
    } else {
        OpenTaleOutcomeBand::Failure
    }
}

/// Resolve one deterministic attribute check.
///
/// Normal checks roll two d6. Advantage rolls three and keeps the highest two; Disadvantage
/// rolls three and keeps the lowest two. Spending a Heroic Badge adds one die, keeps the highest
/// two, and cancels Disadvantage. Each mark and each full ten units of distance subtract one from
/// the total.
pub fn open_tale_resolve_roll(
    input: &OpenTaleRollInput,
    entropy: &mut EntropyStream,
) -> Result<OpenTaleRollResult, TabletopError> {
    if !(1..=30).contains(&input.difficulty) {
        return Err(invalid("difficulty", "expected a difficulty within 1..30"));
    }
    if input.distance > 1_000 {
        return Err(invalid("distance", "distance exceeds 1000"));
    }
    if !(-5..=5).contains(&input.modifier) {
        return Err(invalid("modifier", "modifier must be within -5..5"));
    }
    if input.attribute_value > OPEN_TALE_ATTRIBUTE_MAX {
        return Err(invalid(
            "attribute_value",
            "attribute value exceeds the starting maximum",
        ));
    }
    if input.skill_bonus > OPEN_TALE_MAX_SKILL_RANKS {
        return Err(invalid(
            "skill_bonus",
            "skill bonus exceeds the reviewed range",
        ));
    }
    if input.mark_penalty > OPEN_TALE_MAX_MARKS {
        return Err(invalid(
            "mark_penalty",
            "mark penalty exceeds the mark track",
        ));
    }

    let mut count = 2usize;
    let mut keep_high = true;
    match input.mode {
        OpenTaleRollMode::Normal => {}
        OpenTaleRollMode::Advantage => count += 1,
        OpenTaleRollMode::Disadvantage => {
            if input.heroic_badge {
                // A Heroic Badge cancels Disadvantage.
            } else {
                count += 1;
                keep_high = false;
            }
        }
    }
    if input.heroic_badge {
        count += 1;
        keep_high = true;
    }

    let mut dice = Vec::with_capacity(count);
    for _ in 0..count {
        let die = u32::try_from(entropy.draw_bounded(6)?)
            .map_err(|_| invalid("entropy", "die result overflow"))?
            + 1;
        dice.push(die);
    }
    dice.sort_unstable();
    let (_, high) = dice.split_at(dice.len().saturating_sub(2));
    let kept = if keep_high {
        high.to_vec()
    } else {
        dice.iter().take(2).copied().collect::<Vec<_>>()
    };
    let dice_total = kept.iter().sum::<u32>();
    let distance_penalty = input.distance / 10;
    let total = i64::from(dice_total)
        + i64::from(input.attribute_value)
        + i64::from(input.skill_bonus)
        + i64::from(input.modifier)
        - i64::from(input.mark_penalty)
        - i64::from(distance_penalty);
    let total = i32::try_from(total).map_err(|_| invalid("roll", "total overflow"))?;
    Ok(OpenTaleRollResult {
        band: open_tale_outcome_band(total, input.difficulty),
        dice,
        dice_total,
        heroic_badge_spent: input.heroic_badge,
        kept,
        total,
    })
}

/// Validate and deterministically create one Open Tale character.
pub fn create_open_tale_character(
    request: &OpenTaleCreationRequest,
) -> Result<OpenTaleCreationPreview, TabletopError> {
    validate_open_tale_creation_request(request)?;
    let manifest = open_tale_manifest();
    validate_adapter_manifest(&manifest)?;
    let adapter = resolved_adapter(&manifest)?;

    let skills = selected_skills(request)?;
    let items = selected_items(request)?;
    let passive_rank_total = skills
        .values()
        .filter(|skill| skill.kind == OpenTaleSkillKind::Passive)
        .map(|skill| skill.rank)
        .sum::<u32>();
    let qualities = open_tale_compute_qualities(&request.attributes, passive_rank_total);
    let wealth_spent = items
        .values()
        .try_fold(0_u32, |total, item| total.checked_add(item.wealth_cost))
        .ok_or_else(|| invalid("items", "item cost overflow"))?;
    let wealth_remaining = request.starting_wealth.saturating_sub(wealth_spent);

    let definition_value = definition_value(request, &skills, &items, &qualities);
    let definition_sha256 = canonical_fingerprint(&definition_value)?;
    let definition = AdapterCharacterDefinition {
        adapter: adapter.clone(),
        definition: definition_value,
        definition_sha256: definition_sha256.clone(),
        completed_creation_steps: vec![
            "attributes".to_owned(),
            "identity".to_owned(),
            "skills_and_wealth".to_owned(),
        ],
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
        value: initial_state_value(request, &items, &qualities, wealth_remaining),
    };
    validate_tabletop_state(&initial_state, &manifest)?;

    let preview = OpenTaleCreationPreview {
        adapter,
        creation_format_version: OPEN_TALE_CREATION_FORMAT_VERSION,
        definition,
        explanations: vec![
            "Attributes allocate the fixed starting budget; derived qualities use the declared integer formulas."
                .to_owned(),
            "Creation is authored, not rolled, and the recorded seed starts the deterministic play stream."
                .to_owned(),
            "Tabletop data remains adapter-owned and cannot write back into canonical Character evidence."
                .to_owned(),
        ],
        initial_state,
        item_ids: items.keys().cloned().collect(),
        qualities,
        request_sha256: canonical_fingerprint(request)?,
        skill_ids: skills.keys().cloned().collect(),
        wealth_remaining,
        wealth_spent,
    };
    validate_open_tale_creation_preview(&preview)?;
    Ok(preview)
}

/// Revalidate a creation preview and every cross-artifact fingerprint.
pub fn validate_open_tale_creation_preview(
    preview: &OpenTaleCreationPreview,
) -> Result<(), TabletopError> {
    if preview.creation_format_version != OPEN_TALE_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_sha256("request_sha256", &preview.request_sha256)?;
    let manifest = open_tale_manifest();
    validate_adapter_manifest(&manifest)?;
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

    let fields = value_object("definition", &preview.definition.definition)?;
    let attributes = parse_attributes(object_field(fields, "attributes")?)?;
    let skills = parse_skills(object_map(fields, "skills")?)?;
    let passive_rank_total = skills
        .values()
        .filter(|skill| skill.kind == OpenTaleSkillKind::Passive)
        .map(|skill| skill.rank)
        .sum::<u32>();
    let expected = open_tale_compute_qualities(&attributes, passive_rank_total);
    if parse_qualities(object_field(fields, "qualities")?)? != expected
        || preview.qualities != expected
    {
        return Err(invalid(
            "qualities",
            "declared qualities do not match the inspectable formulas",
        ));
    }
    if object_string(fields, "character_id")? != preview.initial_state.owner_id {
        return Err(invalid(
            "initial_state.owner_id",
            "state owner must equal the definition character identity",
        ));
    }
    let state_fields = value_object("state", &preview.initial_state.value)?;
    if object_u32(state_fields, "health")? != expected.hit_points
        || object_u32(state_fields, "resolve")? != expected.resolve
        || object_u32(state_fields, "level")? != 1
        || object_u32(state_fields, "experience")? != 0
    {
        return Err(invalid(
            "initial_state",
            "initial resources do not match the created character",
        ));
    }
    Ok(())
}

fn validate_open_tale_creation_request(
    request: &OpenTaleCreationRequest,
) -> Result<(), TabletopError> {
    if request.creation_format_version != OPEN_TALE_CREATION_FORMAT_VERSION {
        return Err(TabletopError::UnsupportedVersion {
            path: "creation_format_version",
        });
    }
    validate_global_identifier("character_id", &request.character_id)?;
    validate_bounded_text("name", &request.name, 1, 160)?;
    request.attributes.validate()?;
    let total = request.attributes.total();
    if total != OPEN_TALE_ATTRIBUTE_BUDGET {
        return Err(invalid(
            "attributes",
            if total > OPEN_TALE_ATTRIBUTE_BUDGET {
                "attributes overspend the starting attribute budget"
            } else {
                "attributes must allocate the full starting budget"
            },
        ));
    }
    if request.level != 1 {
        return Err(invalid(
            "level",
            "creation requires the first level; use advancement after play",
        ));
    }
    if request.experience != 0 {
        return Err(invalid(
            "experience",
            "creation requires zero experience; use advancement after play",
        ));
    }
    if request.heroic_badges > OPEN_TALE_MAX_HEROIC_BADGES {
        return Err(invalid(
            "heroic_badges",
            "heroic badges exceed the reviewed maximum",
        ));
    }
    if request.starting_wealth > OPEN_TALE_MAX_STARTING_WEALTH {
        return Err(invalid(
            "starting_wealth",
            "starting wealth exceeds the reviewed maximum",
        ));
    }
    if request.skills.len() > OPEN_TALE_MAX_SKILLS {
        return Err(invalid(
            "skills",
            "too many skills selected for this creation",
        ));
    }
    let catalog = skill_catalog_by_id();
    let mut ids = BTreeSet::new();
    let mut ranks = 0_u32;
    for (index, selection) in request.skills.iter().enumerate() {
        let path = format!("skills[{index}]");
        validate_local_identifier(&format!("{path}.id"), &selection.id)?;
        if !catalog.contains_key(&selection.id) {
            return Err(invalid(
                format!("{path}.id"),
                "skill is not in the adapter catalog",
            ));
        }
        if !(1..=3).contains(&selection.rank) {
            return Err(invalid(
                format!("{path}.rank"),
                "skill rank must be within 1..3",
            ));
        }
        if !ids.insert(&selection.id) {
            return Err(invalid("skills", "duplicate skill selection"));
        }
        ranks = ranks
            .checked_add(selection.rank)
            .ok_or_else(|| invalid("skills", "skill rank overflow"))?;
    }
    if ranks > OPEN_TALE_MAX_SKILL_RANKS {
        return Err(invalid(
            "skills",
            "total skill ranks exceed the creation budget",
        ));
    }
    if request.items.len() > OPEN_TALE_MAX_ITEMS {
        return Err(invalid(
            "items",
            "too many items selected for this creation",
        ));
    }
    let items = item_catalog_by_id();
    let mut item_ids = BTreeSet::new();
    let mut spent = 0_u32;
    for (index, item_id) in request.items.iter().enumerate() {
        validate_local_identifier(&format!("items[{index}]"), item_id)?;
        let Some(item) = items.get(item_id) else {
            return Err(invalid(
                format!("items[{index}]"),
                "item is not in the adapter catalog",
            ));
        };
        if !item_ids.insert(item_id) {
            return Err(invalid("items", "duplicate item selection"));
        }
        spent = spent
            .checked_add(item.wealth_cost)
            .ok_or_else(|| invalid("items", "item cost overflow"))?;
    }
    if spent > request.starting_wealth {
        return Err(invalid(
            "items",
            "selected items exceed the starting wealth budget",
        ));
    }
    Ok(())
}

fn skill_catalog_by_id() -> BTreeMap<String, OpenTaleSkillDefinition> {
    open_tale_skill_catalog()
}

fn item_catalog_by_id() -> BTreeMap<String, OpenTaleItemDefinition> {
    open_tale_item_catalog()
}

fn selected_skills(
    request: &OpenTaleCreationRequest,
) -> Result<BTreeMap<String, OpenTaleSkillDefinition>, TabletopError> {
    let catalog = skill_catalog_by_id();
    let mut selected = BTreeMap::new();
    for selection in &request.skills {
        let Some(mut skill) = catalog.get(&selection.id).cloned() else {
            return Err(invalid("skills", "skill is not in the adapter catalog"));
        };
        skill.rank = selection.rank;
        selected.insert(selection.id.clone(), skill);
    }
    Ok(selected)
}

fn selected_items(
    request: &OpenTaleCreationRequest,
) -> Result<BTreeMap<String, OpenTaleItemDefinition>, TabletopError> {
    let catalog = item_catalog_by_id();
    let mut selected = BTreeMap::new();
    for id in &request.items {
        let Some(item) = catalog.get(id).cloned() else {
            return Err(invalid("items", "item is not in the adapter catalog"));
        };
        selected.insert(id.clone(), item);
    }
    Ok(selected)
}

fn definition_value(
    request: &OpenTaleCreationRequest,
    skills: &BTreeMap<String, OpenTaleSkillDefinition>,
    items: &BTreeMap<String, OpenTaleItemDefinition>,
    qualities: &OpenTaleQualities,
) -> DomainValue {
    object([
        ("attributes", attributes_value(&request.attributes)),
        (
            "character_id",
            DomainValue::String(request.character_id.clone()),
        ),
        (
            "items",
            DomainValue::Object(
                items
                    .iter()
                    .map(|(id, item)| (id.clone(), item_definition_value(item)))
                    .collect(),
            ),
        ),
        ("name", DomainValue::String(request.name.clone())),
        ("qualities", qualities_value(qualities)),
        (
            "skills",
            DomainValue::Object(
                skills
                    .iter()
                    .map(|(id, skill)| (id.clone(), skill_definition_value(skill)))
                    .collect(),
            ),
        ),
        (
            "starting_wealth",
            DomainValue::Number(f64::from(request.starting_wealth)),
        ),
    ])
}

fn initial_state_value(
    request: &OpenTaleCreationRequest,
    items: &BTreeMap<String, OpenTaleItemDefinition>,
    qualities: &OpenTaleQualities,
    wealth_remaining: u32,
) -> DomainValue {
    let inventory = items
        .keys()
        .map(|id| (id.clone(), inventory_state_value(1)))
        .collect();
    object([
        ("challenges", DomainValue::Object(BTreeMap::new())),
        ("combats", DomainValue::Object(BTreeMap::new())),
        ("experience", DomainValue::Number(0.0)),
        (
            "health",
            DomainValue::Number(f64::from(qualities.hit_points)),
        ),
        (
            "heroic_badges",
            DomainValue::Number(f64::from(request.heroic_badges)),
        ),
        ("inventory", DomainValue::Object(inventory)),
        ("level", DomainValue::Number(1.0)),
        ("marks", DomainValue::Number(0.0)),
        ("resolve", DomainValue::Number(f64::from(qualities.resolve))),
        ("wealth", DomainValue::Number(f64::from(wealth_remaining))),
    ])
}

fn attributes_value(attributes: &OpenTaleAttributes) -> DomainValue {
    object([
        (
            "dexterity",
            DomainValue::Number(f64::from(attributes.dexterity)),
        ),
        (
            "intelligence",
            DomainValue::Number(f64::from(attributes.intelligence)),
        ),
        ("magicy", DomainValue::Number(f64::from(attributes.magicy))),
        ("motion", DomainValue::Number(f64::from(attributes.motion))),
        (
            "perception",
            DomainValue::Number(f64::from(attributes.perception)),
        ),
        (
            "persuasion",
            DomainValue::Number(f64::from(attributes.persuasion)),
        ),
        (
            "strength",
            DomainValue::Number(f64::from(attributes.strength)),
        ),
        (
            "vitality",
            DomainValue::Number(f64::from(attributes.vitality)),
        ),
        (
            "volition",
            DomainValue::Number(f64::from(attributes.volition)),
        ),
    ])
}

fn qualities_value(qualities: &OpenTaleQualities) -> DomainValue {
    object([
        (
            "capacity",
            DomainValue::Number(f64::from(qualities.capacity)),
        ),
        ("defense", DomainValue::Number(f64::from(qualities.defense))),
        (
            "hit_points",
            DomainValue::Number(f64::from(qualities.hit_points)),
        ),
        ("resolve", DomainValue::Number(f64::from(qualities.resolve))),
        (
            "toughness",
            DomainValue::Number(f64::from(qualities.toughness)),
        ),
    ])
}

fn skill_definition_value(skill: &OpenTaleSkillDefinition) -> DomainValue {
    object([
        (
            "attribute",
            DomainValue::Symbol(attribute_symbol(skill.attribute).to_owned()),
        ),
        (
            "kind",
            DomainValue::Symbol(skill_kind_symbol(skill.kind).to_owned()),
        ),
        ("label", DomainValue::String(skill.label.clone())),
        ("rank", DomainValue::Number(f64::from(skill.rank))),
    ])
}

fn item_definition_value(item: &OpenTaleItemDefinition) -> DomainValue {
    object([
        (
            "capacity_cost",
            DomainValue::Number(f64::from(item.capacity_cost)),
        ),
        (
            "damage_bonus",
            DomainValue::Number(f64::from(item.damage_bonus)),
        ),
        (
            "defense_bonus",
            DomainValue::Number(f64::from(item.defense_bonus)),
        ),
        (
            "kind",
            DomainValue::Symbol(item_kind_symbol(item.kind).to_owned()),
        ),
        ("label", DomainValue::String(item.label.clone())),
        (
            "wealth_cost",
            DomainValue::Number(f64::from(item.wealth_cost)),
        ),
    ])
}

fn inventory_state_value(quantity: u32) -> DomainValue {
    object([("quantity", DomainValue::Number(f64::from(quantity)))])
}

/// Trusted resolver for the exact [`open_tale_manifest`] coordinate.
#[derive(Debug, Default, Clone, Copy)]
pub struct OpenTaleResolver;

impl TabletopResolver for OpenTaleResolver {
    fn adapter_id(&self) -> &str {
        OPEN_TALE_ADAPTER_ID
    }

    fn adapter_version(&self) -> &str {
        OPEN_TALE_ADAPTER_VERSION
    }

    fn resolve(
        &self,
        operation: &str,
        definition: &DomainValue,
        request: &DomainValue,
        state: &DomainValue,
        entropy: &mut EntropyStream,
    ) -> Result<ResolverOutput, TabletopError> {
        let (state, events) = match operation {
            "advance_character" => resolve_advance(definition, request, state)?,
            "apply_recovery" => resolve_recovery(definition, request, state)?,
            "inspect_character" => resolve_inspect(definition, state)?,
            "move_combat" => resolve_move_combat(definition, request, state)?,
            "purchase_item" => resolve_purchase(request, state)?,
            "resolve_challenge" => resolve_challenge(definition, request, state, entropy)?,
            "resolve_combat_turn" => resolve_combat_turn(definition, request, state, entropy)?,
            "start_combat" => resolve_start_combat(definition, request, state, entropy)?,
            _ => {
                return Err(invalid(
                    "operation",
                    "unsupported Open Tale resolver operation",
                ));
            }
        };
        Ok(ResolverOutput { state, events })
    }
}

fn resolve_challenge(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let challenge_id = object_string(fields, "challenge_id")?.to_owned();
    let label = object_string(fields, "label")?.to_owned();
    let attribute = parse_attribute(object_symbol(fields, "attribute")?)?;
    let difficulty = object_u32(fields, "difficulty")?;
    let distance = object_u32(fields, "distance")?;
    let mode = parse_roll_mode(object_symbol(fields, "mode")?)?;
    let heroic_badge = object_bool(fields, "heroic_badge")?;
    let modifier = object_i32(fields, "modifier")?;
    let skill_id = object_string(fields, "skill_id")?.to_owned();
    validate_local_identifier("input.challenge_id", &challenge_id)?;
    validate_bounded_text("input.label", &label, 1, 160)?;
    if !(1..=30).contains(&difficulty) {
        return Err(invalid(
            "input.difficulty",
            "expected a difficulty within 1..30",
        ));
    }
    if distance > 1_000 {
        return Err(invalid("input.distance", "distance exceeds 1000"));
    }
    if !(-5..=5).contains(&modifier) {
        return Err(invalid("input.modifier", "modifier must be within -5..5"));
    }

    let definition_fields = value_object("definition", definition)?;
    let attributes = parse_attributes(object_field(definition_fields, "attributes")?)?;
    let attribute_value = attributes.get(attribute);
    let skill_bonus = challenge_skill_bonus(definition_fields, &skill_id, attribute)?;

    let state_fields = value_object("state", state)?;
    let marks = object_u32(state_fields, "marks")?;
    let heroic_badges = object_u32(state_fields, "heroic_badges")?;
    if heroic_badge && heroic_badges == 0 {
        return Err(invalid(
            "input.heroic_badge",
            "no heroic badges remain to spend",
        ));
    }

    let roll = open_tale_resolve_roll(
        &OpenTaleRollInput {
            attribute_value,
            difficulty,
            distance,
            heroic_badge,
            mark_penalty: marks,
            mode,
            modifier,
            skill_bonus,
        },
        entropy,
    )?;
    let status = challenge_status(roll.band);
    let spent = u32::from(roll.heroic_badge_spent);
    let mut next_marks = marks;
    if roll.band == OpenTaleOutcomeBand::Failure && marks < OPEN_TALE_MAX_MARKS {
        next_marks += 1;
    }
    let existing_turns = existing_challenge_turns(state_fields, &challenge_id)?;
    let turns = existing_turns
        .checked_add(1)
        .ok_or_else(|| invalid("challenges", "challenge turn overflow"))?;

    let mut next = state.clone();
    {
        let fields = value_object_mut("state", &mut next)?;
        set_u32(fields, "marks", next_marks);
        set_u32(fields, "heroic_badges", heroic_badges.saturating_sub(spent));
        let challenges = object_map_mut(fields, "challenges")?;
        challenges.insert(
            challenge_id.clone(),
            object([
                (
                    "attribute",
                    DomainValue::Symbol(attribute_symbol(attribute).to_owned()),
                ),
                (
                    "band",
                    DomainValue::Symbol(outcome_band_symbol(roll.band).to_owned()),
                ),
                ("difficulty", DomainValue::Number(f64::from(difficulty))),
                ("distance", DomainValue::Number(f64::from(distance))),
                ("label", DomainValue::String(label.clone())),
                ("last_total", DomainValue::Number(f64::from(roll.total))),
                (
                    "status",
                    DomainValue::Symbol(challenge_status_symbol(status).to_owned()),
                ),
                ("turns", DomainValue::Number(f64::from(turns))),
            ]),
        );
    }

    let mut events = vec![
        public_event(
            "roll_recorded",
            roll_payload("character", &challenge_id, mode, &roll),
        ),
        public_event(
            "challenge_updated",
            object([
                (
                    "band",
                    DomainValue::Symbol(outcome_band_symbol(roll.band).to_owned()),
                ),
                ("challenge_id", DomainValue::String(challenge_id.clone())),
                ("label", DomainValue::String(label)),
                ("last_total", DomainValue::Number(f64::from(roll.total))),
                (
                    "status",
                    DomainValue::Symbol(challenge_status_symbol(status).to_owned()),
                ),
                ("turns", DomainValue::Number(f64::from(turns))),
            ]),
        ),
    ];
    if status != OpenTaleChallengeStatus::Open {
        events.push(public_event(
            "challenge_resolved",
            object([
                ("challenge_id", DomainValue::String(challenge_id.clone())),
                (
                    "status",
                    DomainValue::Symbol(challenge_status_symbol(status).to_owned()),
                ),
            ]),
        ));
    }
    if next_marks > marks {
        events.push(public_event(
            "mark_applied",
            object([
                ("marks_after", DomainValue::Number(f64::from(next_marks))),
                ("reason", DomainValue::String("failed_check".to_owned())),
            ]),
        ));
    }
    Ok((next, events))
}

fn challenge_skill_bonus(
    definition_fields: &BTreeMap<String, DomainValue>,
    skill_id: &str,
    attribute: OpenTaleAttribute,
) -> Result<u32, TabletopError> {
    if skill_id.is_empty() {
        return Ok(0);
    }
    validate_local_identifier("input.skill_id", skill_id)?;
    let skills = object_map(definition_fields, "skills")?;
    let Some(skill) = skills.get(skill_id) else {
        return Err(invalid(
            "input.skill_id",
            "skill is not present on the definition",
        ));
    };
    let fields = value_object("definition.skills", skill)?;
    let kind = parse_skill_kind(object_symbol(fields, "kind")?)?;
    if kind != OpenTaleSkillKind::Active {
        return Err(invalid(
            "input.skill_id",
            "only active skills add to attribute checks",
        ));
    }
    let skill_attribute = parse_attribute(object_symbol(fields, "attribute")?)?;
    if skill_attribute != attribute {
        return Err(invalid(
            "input.skill_id",
            "skill attribute does not match the check attribute",
        ));
    }
    object_u32(fields, "rank")
}

fn existing_challenge_turns(
    state_fields: &BTreeMap<String, DomainValue>,
    challenge_id: &str,
) -> Result<u32, TabletopError> {
    let challenges = object_map(state_fields, "challenges")?;
    match challenges.get(challenge_id) {
        Some(value) => {
            let fields = value_object("state.challenges", value)?;
            object_u32(fields, "turns")
        }
        None => Ok(0),
    }
}

fn challenge_status(band: OpenTaleOutcomeBand) -> OpenTaleChallengeStatus {
    match band {
        OpenTaleOutcomeBand::Critical | OpenTaleOutcomeBand::Success => {
            OpenTaleChallengeStatus::Succeeded
        }
        OpenTaleOutcomeBand::Partial => OpenTaleChallengeStatus::Open,
        OpenTaleOutcomeBand::Failure => OpenTaleChallengeStatus::Failed,
    }
}

fn resolve_start_combat(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let combat_id = object_string(fields, "combat_id")?.to_owned();
    let label = object_string(fields, "label")?.to_owned();
    let adversary_name = object_string(fields, "adversary_name")?.to_owned();
    let adversary_health = object_u32(fields, "adversary_health")?;
    let adversary_defense = object_u32(fields, "adversary_defense")?;
    let adversary_damage = object_u32(fields, "adversary_damage")?;
    let adversary_agility = object_u32(fields, "adversary_agility")?;
    let distance = object_u32(fields, "distance")?;
    validate_local_identifier("input.combat_id", &combat_id)?;
    validate_bounded_text("input.label", &label, 1, 160)?;
    validate_bounded_text("input.adversary_name", &adversary_name, 1, 160)?;
    if !(1..=999).contains(&adversary_health) {
        return Err(invalid("input.adversary_health", "expected 1..999"));
    }
    if !(1..=30).contains(&adversary_defense) {
        return Err(invalid("input.adversary_defense", "expected 1..30"));
    }
    if !(1..=20).contains(&adversary_damage) {
        return Err(invalid("input.adversary_damage", "expected 1..20"));
    }
    if adversary_agility > 10 {
        return Err(invalid("input.adversary_agility", "expected 0..10"));
    }
    if distance > 1_000 {
        return Err(invalid("input.distance", "distance exceeds 1000"));
    }
    let state_fields = value_object("state", state)?;
    if object_map(state_fields, "combats")?.contains_key(&combat_id) {
        return Err(invalid("input.combat_id", "combat identity already exists"));
    }
    let attributes = definition_attributes(definition)?;
    let character_initiative = roll_two_d6(entropy)?
        .checked_add(attributes.motion)
        .ok_or_else(|| invalid("initiative", "character initiative overflow"))?;
    let adversary_initiative = roll_two_d6(entropy)?
        .checked_add(adversary_agility)
        .ok_or_else(|| invalid("initiative", "adversary initiative overflow"))?;
    let character_first = character_initiative >= adversary_initiative;

    let combat = CombatData {
        adversary_agility,
        adversary_damage,
        adversary_defense,
        adversary_health,
        adversary_max_health: adversary_health,
        adversary_name: adversary_name.clone(),
        attacks: 0,
        character_first,
        character_initiative,
        distance,
        label: label.clone(),
        round: 0,
        status: OpenTaleCombatStatus::Open,
    };
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    object_map_mut(next_fields, "combats")?.insert(combat_id.clone(), combat.to_value());

    let events = vec![
        public_event(
            "combat_started",
            object([
                (
                    "adversary_health",
                    DomainValue::Number(f64::from(adversary_health)),
                ),
                ("adversary_name", DomainValue::String(adversary_name)),
                ("combat_id", DomainValue::String(combat_id.clone())),
                ("distance", DomainValue::Number(f64::from(distance))),
                ("label", DomainValue::String(label)),
            ]),
        ),
        public_event(
            "initiative_recorded",
            object([
                (
                    "adversary_initiative",
                    DomainValue::Number(f64::from(adversary_initiative)),
                ),
                ("character_first", DomainValue::Bool(character_first)),
                (
                    "character_initiative",
                    DomainValue::Number(f64::from(character_initiative)),
                ),
                ("combat_id", DomainValue::String(combat_id)),
            ]),
        ),
    ];
    Ok((next, events))
}

fn resolve_combat_turn(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
    entropy: &mut EntropyStream,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let combat_id = object_string(fields, "combat_id")?.to_owned();
    let mode = parse_roll_mode(object_symbol(fields, "mode")?)?;
    let heroic_badge = object_bool(fields, "heroic_badge")?;
    let weapon_id = object_string(fields, "weapon_id")?.to_owned();
    validate_local_identifier("input.combat_id", &combat_id)?;

    let definition_fields = value_object("definition", definition)?;
    let attributes = parse_attributes(object_field(definition_fields, "attributes")?)?;
    let qualities = parse_qualities(object_field(definition_fields, "qualities")?)?;
    let items = parse_items(object_map(definition_fields, "items")?)?;
    let character_defense = items
        .values()
        .try_fold(qualities.defense, |total, item| {
            total.checked_add(item.defense_bonus)
        })
        .ok_or_else(|| invalid("definition.items", "defense overflow"))?;
    let weapon_damage = if weapon_id.is_empty() {
        0
    } else {
        validate_local_identifier("input.weapon_id", &weapon_id)?;
        let Some(item) = items.get(&weapon_id) else {
            return Err(invalid(
                "input.weapon_id",
                "weapon is not present on the definition",
            ));
        };
        if item.kind != OpenTaleItemKind::Weapon {
            return Err(invalid("input.weapon_id", "selected item is not a weapon"));
        }
        item.damage_bonus
    };

    let state_fields = value_object("state", state)?;
    let health = object_u32(state_fields, "health")?;
    let marks = object_u32(state_fields, "marks")?;
    let heroic_badges = object_u32(state_fields, "heroic_badges")?;
    if health == 0 {
        return Err(invalid("state.health", "a defeated character cannot act"));
    }
    if heroic_badge && heroic_badges == 0 {
        return Err(invalid(
            "input.heroic_badge",
            "no heroic badges remain to spend",
        ));
    }
    let combat_value = object_map(state_fields, "combats")?
        .get(&combat_id)
        .ok_or_else(|| invalid("input.combat_id", "combat does not exist"))?;
    let mut combat = CombatData::from_value(combat_value)?;
    if combat.status != OpenTaleCombatStatus::Open {
        return Err(invalid("input.combat_id", "combat is already concluded"));
    }
    if combat.adversary_health == 0 {
        return Err(invalid("combat.adversary_health", "adversary is defeated"));
    }

    combat.attacks = combat
        .attacks
        .checked_add(1)
        .ok_or_else(|| invalid("combat.attacks", "attack count overflow"))?;
    combat.round = combat
        .round
        .checked_add(1)
        .ok_or_else(|| invalid("combat.round", "round overflow"))?;
    let mut next_health = health;
    let mut next_marks = marks;
    let mut heroic_spent = false;
    let mut events = Vec::new();
    let order = if combat.character_first {
        [CombatActor::Character, CombatActor::Adversary]
    } else {
        [CombatActor::Adversary, CombatActor::Character]
    };
    for actor in order {
        if next_health == 0 || combat.adversary_health == 0 {
            break;
        }
        match actor {
            CombatActor::Character => {
                let roll = open_tale_resolve_roll(
                    &OpenTaleRollInput {
                        attribute_value: attributes.dexterity,
                        difficulty: combat.adversary_defense,
                        distance: combat.distance,
                        heroic_badge,
                        mark_penalty: next_marks,
                        mode,
                        modifier: 0,
                        skill_bonus: 0,
                    },
                    entropy,
                )?;
                let damage = combat_damage(1 + weapon_damage, roll.band);
                if roll.band == OpenTaleOutcomeBand::Failure && next_marks < OPEN_TALE_MAX_MARKS {
                    next_marks += 1;
                }
                if damage > 0 {
                    combat.adversary_health = combat.adversary_health.saturating_sub(damage);
                }
                if heroic_badge {
                    heroic_spent = true;
                }
                events.push(public_event(
                    "roll_recorded",
                    roll_payload("character", &combat_id, mode, &roll),
                ));
                if damage > 0 {
                    events.push(public_event(
                        "damage_applied",
                        object([
                            ("amount", DomainValue::Number(f64::from(damage))),
                            ("combat_id", DomainValue::String(combat_id.clone())),
                            (
                                "health_after",
                                DomainValue::Number(f64::from(combat.adversary_health)),
                            ),
                            ("target", DomainValue::Symbol("adversary".to_owned())),
                        ]),
                    ));
                }
                if roll.band == OpenTaleOutcomeBand::Failure && next_marks > marks {
                    events.push(public_event(
                        "mark_applied",
                        object([
                            ("marks_after", DomainValue::Number(f64::from(next_marks))),
                            ("reason", DomainValue::String("failed_attack".to_owned())),
                        ]),
                    ));
                }
            }
            CombatActor::Adversary => {
                let roll = open_tale_resolve_roll(
                    &OpenTaleRollInput {
                        attribute_value: combat.adversary_agility,
                        difficulty: character_defense,
                        distance: combat.distance,
                        heroic_badge: false,
                        mark_penalty: 0,
                        mode: OpenTaleRollMode::Normal,
                        modifier: 0,
                        skill_bonus: 0,
                    },
                    entropy,
                )?;
                let raw = combat_damage(combat.adversary_damage, roll.band);
                let damage = raw.saturating_sub(qualities.toughness / 4);
                next_health = next_health.saturating_sub(damage);
                events.push(public_event(
                    "roll_recorded",
                    roll_payload("adversary", &combat_id, OpenTaleRollMode::Normal, &roll),
                ));
                if damage > 0 {
                    events.push(public_event(
                        "damage_applied",
                        object([
                            ("amount", DomainValue::Number(f64::from(damage))),
                            ("combat_id", DomainValue::String(combat_id.clone())),
                            ("health_after", DomainValue::Number(f64::from(next_health))),
                            ("target", DomainValue::Symbol("character".to_owned())),
                        ]),
                    ));
                }
            }
        }
    }

    let outcome = if next_health == 0 {
        Some("defeat")
    } else if combat.adversary_health == 0 {
        Some("victory")
    } else {
        None
    };
    if outcome.is_some() {
        combat.status = OpenTaleCombatStatus::Concluded;
    }
    events.push(public_event(
        "combat_updated",
        object([
            (
                "adversary_health",
                DomainValue::Number(f64::from(combat.adversary_health)),
            ),
            ("combat_id", DomainValue::String(combat_id.clone())),
            ("distance", DomainValue::Number(f64::from(combat.distance))),
            ("health", DomainValue::Number(f64::from(next_health))),
            ("round", DomainValue::Number(f64::from(combat.round))),
        ]),
    ));
    if let Some(outcome) = outcome {
        events.push(public_event(
            "combat_concluded",
            object([
                ("combat_id", DomainValue::String(combat_id.clone())),
                ("outcome", DomainValue::Symbol(outcome.to_owned())),
            ]),
        ));
    }

    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    set_u32(next_fields, "health", next_health);
    set_u32(next_fields, "marks", next_marks);
    if heroic_spent {
        set_u32(
            next_fields,
            "heroic_badges",
            heroic_badges.saturating_sub(1),
        );
    }
    object_map_mut(next_fields, "combats")?.insert(combat_id, combat.to_value());
    Ok((next, events))
}

fn resolve_move_combat(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let combat_id = object_string(fields, "combat_id")?.to_owned();
    let movement = object_u32(fields, "movement")?;
    validate_local_identifier("input.combat_id", &combat_id)?;
    let attributes = definition_attributes(definition)?;
    let maximum = 2_u32
        .checked_add(attributes.motion)
        .ok_or_else(|| invalid("movement", "movement allowance overflow"))?;
    if movement == 0 || movement > maximum {
        return Err(invalid(
            "input.movement",
            "movement must be within the character allowance",
        ));
    }
    let state_fields = value_object("state", state)?;
    let combat_value = object_map(state_fields, "combats")?
        .get(&combat_id)
        .ok_or_else(|| invalid("input.combat_id", "combat does not exist"))?;
    let mut combat = CombatData::from_value(combat_value)?;
    if combat.status != OpenTaleCombatStatus::Open {
        return Err(invalid("input.combat_id", "combat is already concluded"));
    }
    combat.distance = combat.distance.saturating_sub(movement);
    let distance_after = combat.distance;
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    object_map_mut(next_fields, "combats")?.insert(combat_id.clone(), combat.to_value());
    Ok((
        next,
        vec![public_event(
            "combat_moved",
            object([
                ("combat_id", DomainValue::String(combat_id)),
                (
                    "distance_after",
                    DomainValue::Number(f64::from(distance_after)),
                ),
                ("movement", DomainValue::Number(f64::from(movement))),
            ]),
        )],
    ))
}

fn resolve_recovery(
    definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let mode = parse_recovery_mode(object_symbol(fields, "mode")?)?;
    let qualities = definition_qualities(definition)?;
    let state_fields = value_object("state", state)?;
    let health = object_u32(state_fields, "health")?;
    let resolve = object_u32(state_fields, "resolve")?;
    let marks = object_u32(state_fields, "marks")?;
    let (health_after, resolve_after, marks_after) = match mode {
        OpenTaleRecoveryMode::Rest => (
            health.saturating_add(5).min(qualities.hit_points),
            resolve.saturating_add(3).min(qualities.resolve),
            marks.saturating_sub(1),
        ),
        OpenTaleRecoveryMode::Treat => (health, resolve, marks.saturating_sub(2)),
    };
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    set_u32(next_fields, "health", health_after);
    set_u32(next_fields, "resolve", resolve_after);
    set_u32(next_fields, "marks", marks_after);
    Ok((
        next,
        vec![public_event(
            "recovery_applied",
            object([
                ("health_after", DomainValue::Number(f64::from(health_after))),
                ("marks_after", DomainValue::Number(f64::from(marks_after))),
                (
                    "mode",
                    DomainValue::Symbol(recovery_mode_symbol(mode).to_owned()),
                ),
                (
                    "resolve_after",
                    DomainValue::Number(f64::from(resolve_after)),
                ),
            ]),
        )],
    ))
}

fn resolve_advance(
    _definition: &DomainValue,
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let experience_gain = object_u32(fields, "experience_gain")?;
    let milestone = object_string(fields, "milestone")?.to_owned();
    validate_bounded_text("input.milestone", &milestone, 1, 240)?;
    if !(1..=1_000).contains(&experience_gain) {
        return Err(invalid(
            "input.experience_gain",
            "advancement requires one to 1000 experience",
        ));
    }
    let state_fields = value_object("state", state)?;
    let experience_before = object_u32(state_fields, "experience")?;
    let level_before = object_u32(state_fields, "level")?;
    let experience_after = experience_before
        .checked_add(experience_gain)
        .ok_or_else(|| invalid("experience", "experience overflow"))?;
    if experience_after > OPEN_TALE_MAX_EXPERIENCE {
        return Err(invalid(
            "experience",
            "experience exceeds the reviewed maximum",
        ));
    }
    let level_after = level_for_experience(experience_after);
    if level_after > OPEN_TALE_MAX_LEVEL {
        return Err(invalid(
            "level",
            "advancement exceeds the maximum modelled level",
        ));
    }
    if level_before >= OPEN_TALE_MAX_LEVEL {
        return Err(invalid(
            "level",
            "character is already at the maximum level",
        ));
    }
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    set_u32(next_fields, "experience", experience_after);
    set_u32(next_fields, "level", level_after);
    Ok((
        next,
        vec![public_event(
            "advancement_applied",
            object([
                (
                    "experience_after",
                    DomainValue::Number(f64::from(experience_after)),
                ),
                (
                    "experience_before",
                    DomainValue::Number(f64::from(experience_before)),
                ),
                ("level_after", DomainValue::Number(f64::from(level_after))),
                ("level_before", DomainValue::Number(f64::from(level_before))),
                ("milestone", DomainValue::String(milestone)),
            ]),
        )],
    ))
}

fn level_for_experience(experience: u32) -> u32 {
    1 + experience / OPEN_TALE_EXPERIENCE_PER_LEVEL
}

fn resolve_purchase(
    request: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let fields = value_object("input", request)?;
    let item_id = object_string(fields, "item_id")?.to_owned();
    let quantity = object_u32(fields, "quantity")?;
    validate_local_identifier("input.item_id", &item_id)?;
    if !(1..=99).contains(&quantity) {
        return Err(invalid("input.quantity", "quantity must be within 1..99"));
    }
    let catalog = item_catalog_by_id();
    let Some(item) = catalog.get(&item_id) else {
        return Err(invalid(
            "input.item_id",
            "item is not in the adapter catalog",
        ));
    };
    let cost = item
        .wealth_cost
        .checked_mul(quantity)
        .ok_or_else(|| invalid("input.quantity", "purchase cost overflow"))?;
    let state_fields = value_object("state", state)?;
    let wealth = object_u32(state_fields, "wealth")?;
    if cost > wealth {
        return Err(invalid(
            "input.item_id",
            "purchase exceeds the available wealth",
        ));
    }
    let inventory = object_map(state_fields, "inventory")?;
    let current = match inventory.get(&item_id) {
        Some(value) => object_u32(value_object("state.inventory", value)?, "quantity")?,
        None => 0,
    };
    let quantity_after = current
        .checked_add(quantity)
        .ok_or_else(|| invalid("inventory", "inventory quantity overflow"))?;
    if quantity_after > 9_999 {
        return Err(invalid(
            "inventory",
            "inventory quantity exceeds the reviewed maximum",
        ));
    }
    let wealth_after = wealth - cost;
    let mut next = state.clone();
    let next_fields = value_object_mut("state", &mut next)?;
    set_u32(next_fields, "wealth", wealth_after);
    object_map_mut(next_fields, "inventory")?
        .insert(item_id.clone(), inventory_state_value(quantity_after));
    Ok((
        next,
        vec![
            public_event(
                "item_acquired",
                object([
                    ("cost", DomainValue::Number(f64::from(cost))),
                    ("item_id", DomainValue::String(item_id.clone())),
                    ("label", DomainValue::String(item.label.clone())),
                    (
                        "quantity_after",
                        DomainValue::Number(f64::from(quantity_after)),
                    ),
                ]),
            ),
            public_event(
                "wealth_spent",
                object([
                    ("cost", DomainValue::Number(f64::from(cost))),
                    ("item_id", DomainValue::String(item_id)),
                    ("wealth_after", DomainValue::Number(f64::from(wealth_after))),
                ]),
            ),
        ],
    ))
}

fn resolve_inspect(
    definition: &DomainValue,
    state: &DomainValue,
) -> Result<(DomainValue, Vec<ResolverEvent>), TabletopError> {
    let definition_fields = value_object("definition", definition)?;
    let attributes = parse_attributes(object_field(definition_fields, "attributes")?)?;
    let skills = parse_skills(object_map(definition_fields, "skills")?)?;
    let items = parse_items(object_map(definition_fields, "items")?)?;
    let passive_rank_total = skills
        .values()
        .filter(|skill| skill.kind == OpenTaleSkillKind::Passive)
        .map(|skill| skill.rank)
        .sum::<u32>();
    let qualities = open_tale_compute_qualities(&attributes, passive_rank_total);
    let state_fields = value_object("state", state)?;
    let wealth = object_u32(state_fields, "wealth")?;
    let level = object_u32(state_fields, "level")?;
    let experience = object_u32(state_fields, "experience")?;
    Ok((
        state.clone(),
        vec![public_event(
            "character_inspected",
            object([
                ("attributes", attributes_value(&attributes)),
                ("experience", DomainValue::Number(f64::from(experience))),
                (
                    "items",
                    string_list_value(&items.keys().cloned().collect::<Vec<_>>()),
                ),
                ("level", DomainValue::Number(f64::from(level))),
                ("qualities", qualities_value(&qualities)),
                (
                    "skills",
                    string_list_value(&skills.keys().cloned().collect::<Vec<_>>()),
                ),
                ("wealth", DomainValue::Number(f64::from(wealth))),
            ]),
        )],
    ))
}

fn roll_two_d6(entropy: &mut EntropyStream) -> Result<u32, TabletopError> {
    let first = u32::try_from(entropy.draw_bounded(6)?)
        .map_err(|_| invalid("entropy", "die result overflow"))?
        + 1;
    let second = u32::try_from(entropy.draw_bounded(6)?)
        .map_err(|_| invalid("entropy", "die result overflow"))?
        + 1;
    first
        .checked_add(second)
        .ok_or_else(|| invalid("initiative", "dice sum overflow"))
}

fn combat_damage(base: u32, band: OpenTaleOutcomeBand) -> u32 {
    match band {
        OpenTaleOutcomeBand::Critical => base.saturating_add(2),
        OpenTaleOutcomeBand::Success => base,
        OpenTaleOutcomeBand::Partial => base.div_ceil(2).max(1),
        OpenTaleOutcomeBand::Failure => 0,
    }
}

fn roll_payload(
    actor: &str,
    context_id: &str,
    mode: OpenTaleRollMode,
    roll: &OpenTaleRollResult,
) -> DomainValue {
    object([
        ("actor", DomainValue::String(actor.to_owned())),
        (
            "band",
            DomainValue::Symbol(outcome_band_symbol(roll.band).to_owned()),
        ),
        ("context_id", DomainValue::String(context_id.to_owned())),
        ("dice", number_list(&roll.dice)),
        (
            "dice_total",
            DomainValue::Number(f64::from(roll.dice_total)),
        ),
        (
            "heroic_badge_spent",
            DomainValue::Bool(roll.heroic_badge_spent),
        ),
        ("kept", number_list(&roll.kept)),
        (
            "mode",
            DomainValue::Symbol(roll_mode_symbol(mode).to_owned()),
        ),
        ("total", DomainValue::Number(f64::from(roll.total))),
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CombatActor {
    Adversary,
    Character,
}

#[derive(Debug, Clone)]
struct CombatData {
    adversary_agility: u32,
    adversary_damage: u32,
    adversary_defense: u32,
    adversary_health: u32,
    adversary_max_health: u32,
    adversary_name: String,
    attacks: u32,
    character_first: bool,
    character_initiative: u32,
    distance: u32,
    label: String,
    round: u32,
    status: OpenTaleCombatStatus,
}

impl CombatData {
    fn from_value(value: &DomainValue) -> Result<Self, TabletopError> {
        let fields = value_object("combat", value)?;
        Ok(Self {
            adversary_agility: object_u32(fields, "adversary_agility")?,
            adversary_damage: object_u32(fields, "adversary_damage")?,
            adversary_defense: object_u32(fields, "adversary_defense")?,
            adversary_health: object_u32(fields, "adversary_health")?,
            adversary_max_health: object_u32(fields, "adversary_max_health")?,
            adversary_name: object_string(fields, "adversary_name")?.to_owned(),
            attacks: object_u32(fields, "attacks")?,
            character_first: object_bool(fields, "character_first")?,
            character_initiative: object_u32(fields, "character_initiative")?,
            distance: object_u32(fields, "distance")?,
            label: object_string(fields, "label")?.to_owned(),
            round: object_u32(fields, "round")?,
            status: parse_combat_status(object_symbol(fields, "status")?)?,
        })
    }

    fn to_value(&self) -> DomainValue {
        object([
            (
                "adversary_agility",
                DomainValue::Number(f64::from(self.adversary_agility)),
            ),
            (
                "adversary_damage",
                DomainValue::Number(f64::from(self.adversary_damage)),
            ),
            (
                "adversary_defense",
                DomainValue::Number(f64::from(self.adversary_defense)),
            ),
            (
                "adversary_health",
                DomainValue::Number(f64::from(self.adversary_health)),
            ),
            (
                "adversary_max_health",
                DomainValue::Number(f64::from(self.adversary_max_health)),
            ),
            (
                "adversary_name",
                DomainValue::String(self.adversary_name.clone()),
            ),
            ("attacks", DomainValue::Number(f64::from(self.attacks))),
            ("character_first", DomainValue::Bool(self.character_first)),
            (
                "character_initiative",
                DomainValue::Number(f64::from(self.character_initiative)),
            ),
            ("distance", DomainValue::Number(f64::from(self.distance))),
            ("label", DomainValue::String(self.label.clone())),
            ("round", DomainValue::Number(f64::from(self.round))),
            (
                "status",
                DomainValue::Symbol(combat_status_symbol(self.status).to_owned()),
            ),
        ])
    }
}

fn definition_attributes(definition: &DomainValue) -> Result<OpenTaleAttributes, TabletopError> {
    let fields = value_object("definition", definition)?;
    parse_attributes(object_field(fields, "attributes")?)
}

fn definition_qualities(definition: &DomainValue) -> Result<OpenTaleQualities, TabletopError> {
    let fields = value_object("definition", definition)?;
    parse_qualities(object_field(fields, "qualities")?)
}

fn parse_attributes(value: &DomainValue) -> Result<OpenTaleAttributes, TabletopError> {
    let fields = value_object("attributes", value)?;
    Ok(OpenTaleAttributes {
        dexterity: object_u32(fields, "dexterity")?,
        intelligence: object_u32(fields, "intelligence")?,
        magicy: object_u32(fields, "magicy")?,
        motion: object_u32(fields, "motion")?,
        perception: object_u32(fields, "perception")?,
        persuasion: object_u32(fields, "persuasion")?,
        strength: object_u32(fields, "strength")?,
        vitality: object_u32(fields, "vitality")?,
        volition: object_u32(fields, "volition")?,
    })
}

fn parse_qualities(value: &DomainValue) -> Result<OpenTaleQualities, TabletopError> {
    let fields = value_object("qualities", value)?;
    Ok(OpenTaleQualities {
        capacity: object_u32(fields, "capacity")?,
        defense: object_u32(fields, "defense")?,
        hit_points: object_u32(fields, "hit_points")?,
        resolve: object_u32(fields, "resolve")?,
        toughness: object_u32(fields, "toughness")?,
    })
}

fn parse_skills(
    values: &BTreeMap<String, DomainValue>,
) -> Result<BTreeMap<String, OpenTaleSkillDefinition>, TabletopError> {
    values
        .iter()
        .map(|(id, value)| Ok((id.clone(), parse_skill_definition(value)?)))
        .collect()
}

fn parse_skill_definition(value: &DomainValue) -> Result<OpenTaleSkillDefinition, TabletopError> {
    let fields = value_object("skill", value)?;
    Ok(OpenTaleSkillDefinition {
        attribute: parse_attribute(object_symbol(fields, "attribute")?)?,
        kind: parse_skill_kind(object_symbol(fields, "kind")?)?,
        label: object_string(fields, "label")?.to_owned(),
        rank: object_u32(fields, "rank")?,
    })
}

fn parse_items(
    values: &BTreeMap<String, DomainValue>,
) -> Result<BTreeMap<String, OpenTaleItemDefinition>, TabletopError> {
    values
        .iter()
        .map(|(id, value)| Ok((id.clone(), parse_item_definition(value)?)))
        .collect()
}

fn parse_item_definition(value: &DomainValue) -> Result<OpenTaleItemDefinition, TabletopError> {
    let fields = value_object("item", value)?;
    Ok(OpenTaleItemDefinition {
        capacity_cost: object_u32(fields, "capacity_cost")?,
        damage_bonus: object_u32(fields, "damage_bonus")?,
        defense_bonus: object_u32(fields, "defense_bonus")?,
        kind: parse_item_kind(object_symbol(fields, "kind")?)?,
        label: object_string(fields, "label")?.to_owned(),
        wealth_cost: object_u32(fields, "wealth_cost")?,
    })
}

fn parse_attribute(value: &str) -> Result<OpenTaleAttribute, TabletopError> {
    match value {
        "dexterity" => Ok(OpenTaleAttribute::Dexterity),
        "intelligence" => Ok(OpenTaleAttribute::Intelligence),
        "magicy" => Ok(OpenTaleAttribute::Magicy),
        "motion" => Ok(OpenTaleAttribute::Motion),
        "perception" => Ok(OpenTaleAttribute::Perception),
        "persuasion" => Ok(OpenTaleAttribute::Persuasion),
        "strength" => Ok(OpenTaleAttribute::Strength),
        "vitality" => Ok(OpenTaleAttribute::Vitality),
        "volition" => Ok(OpenTaleAttribute::Volition),
        _ => Err(invalid("attribute", "unknown Open Tale attribute")),
    }
}

fn parse_skill_kind(value: &str) -> Result<OpenTaleSkillKind, TabletopError> {
    match value {
        "active" => Ok(OpenTaleSkillKind::Active),
        "passive" => Ok(OpenTaleSkillKind::Passive),
        _ => Err(invalid("skill.kind", "expected active or passive")),
    }
}

fn parse_item_kind(value: &str) -> Result<OpenTaleItemKind, TabletopError> {
    match value {
        "armor" => Ok(OpenTaleItemKind::Armor),
        "gear" => Ok(OpenTaleItemKind::Gear),
        "tool" => Ok(OpenTaleItemKind::Tool),
        "weapon" => Ok(OpenTaleItemKind::Weapon),
        _ => Err(invalid("item.kind", "unknown item category")),
    }
}

fn parse_roll_mode(value: &str) -> Result<OpenTaleRollMode, TabletopError> {
    match value {
        "advantage" => Ok(OpenTaleRollMode::Advantage),
        "disadvantage" => Ok(OpenTaleRollMode::Disadvantage),
        "normal" => Ok(OpenTaleRollMode::Normal),
        _ => Err(invalid(
            "mode",
            "expected normal, advantage, or disadvantage",
        )),
    }
}

fn parse_recovery_mode(value: &str) -> Result<OpenTaleRecoveryMode, TabletopError> {
    match value {
        "rest" => Ok(OpenTaleRecoveryMode::Rest),
        "treat" => Ok(OpenTaleRecoveryMode::Treat),
        _ => Err(invalid("mode", "expected rest or treat")),
    }
}

fn parse_combat_status(value: &str) -> Result<OpenTaleCombatStatus, TabletopError> {
    match value {
        "concluded" => Ok(OpenTaleCombatStatus::Concluded),
        "open" => Ok(OpenTaleCombatStatus::Open),
        _ => Err(invalid("combat.status", "expected open or concluded")),
    }
}

const fn attribute_symbol(attribute: OpenTaleAttribute) -> &'static str {
    match attribute {
        OpenTaleAttribute::Dexterity => "dexterity",
        OpenTaleAttribute::Intelligence => "intelligence",
        OpenTaleAttribute::Magicy => "magicy",
        OpenTaleAttribute::Motion => "motion",
        OpenTaleAttribute::Perception => "perception",
        OpenTaleAttribute::Persuasion => "persuasion",
        OpenTaleAttribute::Strength => "strength",
        OpenTaleAttribute::Vitality => "vitality",
        OpenTaleAttribute::Volition => "volition",
    }
}

const fn skill_kind_symbol(kind: OpenTaleSkillKind) -> &'static str {
    match kind {
        OpenTaleSkillKind::Active => "active",
        OpenTaleSkillKind::Passive => "passive",
    }
}

const fn item_kind_symbol(kind: OpenTaleItemKind) -> &'static str {
    match kind {
        OpenTaleItemKind::Armor => "armor",
        OpenTaleItemKind::Gear => "gear",
        OpenTaleItemKind::Tool => "tool",
        OpenTaleItemKind::Weapon => "weapon",
    }
}

const fn roll_mode_symbol(mode: OpenTaleRollMode) -> &'static str {
    match mode {
        OpenTaleRollMode::Advantage => "advantage",
        OpenTaleRollMode::Disadvantage => "disadvantage",
        OpenTaleRollMode::Normal => "normal",
    }
}

const fn outcome_band_symbol(band: OpenTaleOutcomeBand) -> &'static str {
    match band {
        OpenTaleOutcomeBand::Critical => "critical",
        OpenTaleOutcomeBand::Failure => "failure",
        OpenTaleOutcomeBand::Partial => "partial",
        OpenTaleOutcomeBand::Success => "success",
    }
}

const fn challenge_status_symbol(status: OpenTaleChallengeStatus) -> &'static str {
    match status {
        OpenTaleChallengeStatus::Failed => "failed",
        OpenTaleChallengeStatus::Open => "open",
        OpenTaleChallengeStatus::Succeeded => "succeeded",
    }
}

const fn combat_status_symbol(status: OpenTaleCombatStatus) -> &'static str {
    match status {
        OpenTaleCombatStatus::Concluded => "concluded",
        OpenTaleCombatStatus::Open => "open",
    }
}

const fn recovery_mode_symbol(mode: OpenTaleRecoveryMode) -> &'static str {
    match mode {
        OpenTaleRecoveryMode::Rest => "rest",
        OpenTaleRecoveryMode::Treat => "treat",
    }
}

/// Complete Open Tale adapter manifest.
#[must_use]
pub fn open_tale_manifest() -> AdapterManifest {
    let attributes_type = object_type([
        (
            "dexterity",
            required(integer(0.0, 5.0), "Dexterity rating."),
        ),
        (
            "intelligence",
            required(integer(0.0, 5.0), "Intelligence rating."),
        ),
        ("magicy", required(integer(0.0, 5.0), "Magicy rating.")),
        ("motion", required(integer(0.0, 5.0), "Motion rating.")),
        (
            "perception",
            required(integer(0.0, 5.0), "Perception rating."),
        ),
        (
            "persuasion",
            required(integer(0.0, 5.0), "Persuasion rating."),
        ),
        ("strength", required(integer(0.0, 5.0), "Strength rating.")),
        ("vitality", required(integer(0.0, 5.0), "Vitality rating.")),
        ("volition", required(integer(0.0, 5.0), "Volition rating.")),
    ]);
    let qualities_type = object_type([
        (
            "capacity",
            required(integer(0.0, 999.0), "Carrying capacity."),
        ),
        ("defense", required(integer(0.0, 999.0), "Defense quality.")),
        (
            "hit_points",
            required(integer(0.0, 999.0), "Maximum health."),
        ),
        ("resolve", required(integer(0.0, 999.0), "Maximum resolve.")),
        (
            "toughness",
            required(integer(0.0, 999.0), "Damage reduction."),
        ),
    ]);
    let skill_definition_type = object_type([
        (
            "attribute",
            required(attribute_type(), "Governing attribute."),
        ),
        (
            "kind",
            required(skill_kind_type(), "Active or passive skill."),
        ),
        ("label", required(string(1, 160), "Skill display label.")),
        ("rank", required(integer(0.0, 5.0), "Selected skill rank.")),
    ]);
    let item_definition_type = object_type([
        (
            "capacity_cost",
            required(integer(0.0, 20.0), "Capacity consumed."),
        ),
        (
            "damage_bonus",
            required(integer(0.0, 10.0), "Weapon damage bonus."),
        ),
        (
            "defense_bonus",
            required(integer(0.0, 10.0), "Armor defense bonus."),
        ),
        ("kind", required(item_kind_type(), "Equipment category.")),
        ("label", required(string(1, 160), "Item display label.")),
        ("wealth_cost", required(integer(0.0, 20.0), "Wealth cost.")),
    ]);
    let challenge_state_type = object_type([
        (
            "attribute",
            required(attribute_type(), "Attribute used by the check."),
        ),
        (
            "band",
            required(outcome_band_type(), "Last documented result band."),
        ),
        (
            "difficulty",
            required(integer(1.0, 30.0), "Challenge difficulty."),
        ),
        (
            "distance",
            required(integer(0.0, 1_000.0), "Challenge distance."),
        ),
        ("label", required(string(1, 160), "Challenge label.")),
        (
            "last_total",
            required(integer(-1_000.0, 1_000.0), "Last check total."),
        ),
        (
            "status",
            required(challenge_status_type(), "Challenge lifecycle."),
        ),
        (
            "turns",
            required(integer(1.0, 1_000.0), "Attempts recorded."),
        ),
    ]);
    let combat_state_type = object_type([
        (
            "adversary_agility",
            required(integer(0.0, 10.0), "Adversary agility rating."),
        ),
        (
            "adversary_damage",
            required(integer(1.0, 20.0), "Adversary damage rating."),
        ),
        (
            "adversary_defense",
            required(integer(1.0, 30.0), "Adversary defense value."),
        ),
        (
            "adversary_health",
            required(integer(0.0, 999.0), "Current adversary health."),
        ),
        (
            "adversary_max_health",
            required(integer(1.0, 999.0), "Starting adversary health."),
        ),
        (
            "adversary_name",
            required(string(1, 160), "Adversary display name."),
        ),
        (
            "attacks",
            required(integer(0.0, 1_000.0), "Exchanges taken."),
        ),
        (
            "character_first",
            required(TypeExpression::Bool, "Whether the character acts first."),
        ),
        (
            "character_initiative",
            required(integer(0.0, 40.0), "Character initiative."),
        ),
        (
            "distance",
            required(integer(0.0, 1_000.0), "Current distance."),
        ),
        ("label", required(string(1, 160), "Combat label.")),
        (
            "round",
            required(integer(0.0, 1_000.0), "Completed rounds."),
        ),
        (
            "status",
            required(combat_status_type(), "Combat lifecycle."),
        ),
    ]);
    let inventory_state_type = object_type([(
        "quantity",
        required(integer(1.0, 9_999.0), "Carried quantity."),
    )]);
    let types = BTreeMap::from([
        ("Attributes".to_owned(), attributes_type),
        ("ChallengeState".to_owned(), challenge_state_type),
        ("CombatState".to_owned(), combat_state_type),
        ("InventoryState".to_owned(), inventory_state_type),
        ("ItemDefinition".to_owned(), item_definition_type),
        ("Qualities".to_owned(), qualities_type),
        ("SkillDefinition".to_owned(), skill_definition_type),
    ]);
    let definition_type = object_type([
        (
            "attributes",
            required(named("Attributes"), "Authored attribute ratings."),
        ),
        (
            "character_id",
            required(string(3, 255), "Stable character identity."),
        ),
        (
            "items",
            required(map(named("ItemDefinition"), 0, 64), "Owned equipment."),
        ),
        ("name", required(string(1, 160), "Fictional display name.")),
        (
            "qualities",
            required(named("Qualities"), "Derived quality values."),
        ),
        (
            "skills",
            required(map(named("SkillDefinition"), 0, 16), "Selected skills."),
        ),
        (
            "starting_wealth",
            required(integer(0.0, 20.0), "Declared starting wealth."),
        ),
    ]);
    let state_type = object_type([
        (
            "challenges",
            required(map(named("ChallengeState"), 0, 128), "Recorded challenges."),
        ),
        (
            "combats",
            required(map(named("CombatState"), 0, 64), "Recorded combats."),
        ),
        (
            "experience",
            required(integer(0.0, 1_000_000.0), "Accumulated experience."),
        ),
        ("health", required(integer(0.0, 999.0), "Current health.")),
        (
            "heroic_badges",
            required(integer(0.0, 99.0), "Remaining heroic badges."),
        ),
        (
            "inventory",
            required(map(named("InventoryState"), 0, 64), "Purchased items."),
        ),
        ("level", required(integer(1.0, 20.0), "Current level.")),
        ("marks", required(integer(0.0, 3.0), "Carried marks.")),
        ("resolve", required(integer(0.0, 999.0), "Current resolve.")),
        (
            "wealth",
            required(integer(0.0, 1_000_000.0), "Remaining wealth."),
        ),
    ]);

    AdapterManifest {
        manifest_format_version: ADAPTER_MANIFEST_FORMAT_VERSION,
        id: OPEN_TALE_ADAPTER_ID.to_owned(),
        version: OPEN_TALE_ADAPTER_VERSION.to_owned(),
        schema_version: 1,
        namespace: "open_tale".to_owned(),
        title: "Open Tale adapter".to_owned(),
        compatibility_label: "Compatible with the Open Tale public rules reference".to_owned(),
        summary: "Point-budget characters, nine attributes, five derived qualities, active and passive skills, wealth, deterministic two-d6 checks, challenges, combat, recovery, and advancement."
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
            source_class: AdapterSourceClass::SeparatelyLicensedApache2,
            source_url: OPEN_TALE_SOURCE_URL.to_owned(),
            exact_artifact: "open-tale-public-rules-reference".to_owned(),
            revision: OPEN_TALE_EDITION.to_owned(),
            retrieved_on: "2026-11-06".to_owned(),
            sha256: OPEN_TALE_SOURCE_SHA256.to_owned(),
            license: OPEN_TALE_LICENSE.to_owned(),
            license_url: OPEN_TALE_LICENSE_URL.to_owned(),
            covered_files_or_sections: vec![
                "Public Open Tale rules reference: attributes, derived qualities, skills, wealth, checks, challenges, combat, recovery, and advancement."
                    .to_owned(),
                "Adapter behavior is independently specified from the public rules reference."
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
            attribution: "Open Tale is published at https://open-tale.github.io/ under the Apache License 2.0. This adapter is an independent implementation by the Weave project and is not endorsed by the Open Tale project. Weave integration code remains MIT; this adapter declares the upstream rules reference separately under Apache-2.0."
                .to_owned(),
            required_license_text: LicenseTextReference {
                artifact: "crates/weave-tabletop/src/open_tale.rs".to_owned(),
                sha256: OPEN_TALE_LICENSE_TEXT_SHA256.to_owned(),
            },
            additional_artifacts: Vec::new(),
            notices: vec![
                "The complete Apache-2.0 license text is retained in OPEN_TALE_LICENSE_TEXT in this module."
                    .to_owned(),
                "This adapter does not redistribute upstream text, artwork, fonts, or layout; only independently authored integration code, formulas, and fixtures are bundled."
                    .to_owned(),
                "No upstream NOTICE file is known for the public rules reference; this notice records the attribution and modification boundary instead."
                    .to_owned(),
            ],
            compatibility_statement: "Open Tale is named only to describe rules compatibility. This project is independent and does not imply endorsement, sponsorship, or affiliation."
                .to_owned(),
        },
    }
}

fn creation_steps() -> Vec<CreationStep> {
    vec![
        CreationStep {
            id: "attributes".to_owned(),
            title: "Attributes".to_owned(),
            description: "Allocate the fixed starting attribute budget across the nine attributes."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![creation_field(
                "attributes",
                "Attributes",
                named("Attributes"),
                true,
            )],
        },
        CreationStep {
            id: "identity".to_owned(),
            title: "Identity".to_owned(),
            description: "Name the character and declare the starting level, wealth, and heroic badges."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("character_id", "Character identity", string(3, 255), true),
                creation_field("name", "Name", string(1, 160), true),
                creation_field("level", "Level", integer(1.0, 1.0), true),
                creation_field(
                    "starting_wealth",
                    "Starting wealth",
                    integer(0.0, 20.0),
                    true,
                ),
                creation_field(
                    "heroic_badges",
                    "Heroic badges",
                    integer(0.0, 3.0),
                    false,
                ),
            ],
        },
        CreationStep {
            id: "skills_and_wealth".to_owned(),
            title: "Skills and wealth".to_owned(),
            description: "Select bounded active and passive skills and spend the starting wealth on catalog equipment."
                .to_owned(),
            required_capability: TabletopCapability::CharacterCreation,
            fields: vec![
                creation_field("skills", "Skills", list(string(1, 128), 0, 4), true),
                creation_field("items", "Items", list(string(1, 128), 0, 8), true),
            ],
        },
    ]
}

fn operations() -> BTreeMap<String, ResolverOperationDeclaration> {
    BTreeMap::from([
        (
            "advance_character".to_owned(),
            operation(
                "advance_character",
                "Advance a character",
                "Record a milestone and add experience when the character is below the maximum level.",
                TabletopCapability::Advancement,
                object_type([
                    (
                        "experience_gain",
                        required(integer(1.0, 1_000.0), "Experience awarded."),
                    ),
                    ("milestone", required(string(1, 240), "Recorded milestone.")),
                ]),
                ["advancement_applied"],
                false,
            ),
        ),
        (
            "apply_recovery".to_owned(),
            operation(
                "apply_recovery",
                "Apply recovery",
                "Restore health and resolve or remove marks through a rest or treatment.",
                TabletopCapability::ResourcesAndConditions,
                object_type([(
                    "mode",
                    required(recovery_mode_type(), "Recovery procedure."),
                )]),
                ["recovery_applied"],
                false,
            ),
        ),
        (
            "inspect_character".to_owned(),
            operation(
                "inspect_character",
                "Inspect the character",
                "Report attributes, qualities, skills, items, wealth, level, and experience without mutation.",
                TabletopCapability::DerivedValues,
                object_type([]),
                ["character_inspected"],
                false,
            ),
        ),
        (
            "move_combat".to_owned(),
            operation(
                "move_combat",
                "Move during combat",
                "Close or open distance by no more than the character's movement allowance.",
                TabletopCapability::Encounters,
                object_type([
                    ("combat_id", required(string(1, 128), "Combat identity.")),
                    ("movement", required(integer(1.0, 100.0), "Distance moved.")),
                ]),
                ["combat_moved"],
                false,
            ),
        ),
        (
            "purchase_item".to_owned(),
            operation(
                "purchase_item",
                "Purchase an item",
                "Spend wealth for catalog equipment and record the carried quantity.",
                TabletopCapability::EquipmentAndAbilities,
                object_type([
                    (
                        "item_id",
                        required(string(1, 128), "Catalog item identity."),
                    ),
                    (
                        "quantity",
                        required(integer(1.0, 99.0), "Purchased quantity."),
                    ),
                ]),
                ["item_acquired", "wealth_spent"],
                false,
            ),
        ),
        (
            "resolve_challenge".to_owned(),
            operation(
                "resolve_challenge",
                "Resolve a challenge",
                "Roll a deterministic attribute check against a difficulty, distance, and mode, then record the challenge state.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    ("attribute", required(attribute_type(), "Check attribute.")),
                    (
                        "challenge_id",
                        required(string(1, 128), "Challenge identity."),
                    ),
                    (
                        "difficulty",
                        required(integer(1.0, 30.0), "Challenge difficulty."),
                    ),
                    (
                        "distance",
                        required(integer(0.0, 1_000.0), "Challenge distance."),
                    ),
                    (
                        "heroic_badge",
                        required(TypeExpression::Bool, "Whether to spend a heroic badge."),
                    ),
                    ("label", required(string(1, 160), "Challenge label.")),
                    ("mode", required(roll_mode_type(), "Dice mode.")),
                    (
                        "modifier",
                        required(integer(-5.0, 5.0), "Situational modifier."),
                    ),
                    (
                        "skill_id",
                        required(string(0, 128), "Optional active skill."),
                    ),
                ]),
                [
                    "challenge_resolved",
                    "challenge_updated",
                    "mark_applied",
                    "roll_recorded",
                ],
                true,
            ),
        ),
        (
            "resolve_combat_turn".to_owned(),
            operation(
                "resolve_combat_turn",
                "Resolve a combat turn",
                "Resolve one ordered exchange, apply damage and marks, and conclude the combat when either side is defeated.",
                TabletopCapability::ChecksAndConflicts,
                object_type([
                    ("combat_id", required(string(1, 128), "Combat identity.")),
                    (
                        "heroic_badge",
                        required(TypeExpression::Bool, "Whether to spend a heroic badge."),
                    ),
                    ("mode", required(roll_mode_type(), "Character attack mode.")),
                    (
                        "weapon_id",
                        required(string(0, 128), "Optional owned weapon."),
                    ),
                ]),
                [
                    "combat_concluded",
                    "combat_updated",
                    "damage_applied",
                    "mark_applied",
                    "roll_recorded",
                ],
                true,
            ),
        ),
        (
            "start_combat".to_owned(),
            operation(
                "start_combat",
                "Start a combat",
                "Record an adversary and draw deterministic initiative for both sides.",
                TabletopCapability::Encounters,
                object_type([
                    (
                        "adversary_agility",
                        required(integer(0.0, 10.0), "Adversary agility."),
                    ),
                    (
                        "adversary_damage",
                        required(integer(1.0, 20.0), "Adversary damage."),
                    ),
                    (
                        "adversary_defense",
                        required(integer(1.0, 30.0), "Adversary defense."),
                    ),
                    (
                        "adversary_health",
                        required(integer(1.0, 999.0), "Adversary health."),
                    ),
                    (
                        "adversary_name",
                        required(string(1, 160), "Adversary name."),
                    ),
                    ("combat_id", required(string(1, 128), "Combat identity.")),
                    (
                        "distance",
                        required(integer(0.0, 1_000.0), "Starting distance."),
                    ),
                    ("label", required(string(1, 160), "Combat label.")),
                ]),
                ["combat_started", "initiative_recorded"],
                true,
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
                    "experience_after",
                    required(integer(0.0, 1_000_000.0), "Experience after."),
                ),
                (
                    "experience_before",
                    required(integer(0.0, 1_000_000.0), "Experience before."),
                ),
                ("level_after", required(integer(1.0, 20.0), "Level after.")),
                (
                    "level_before",
                    required(integer(1.0, 20.0), "Level before."),
                ),
                ("milestone", required(string(1, 240), "Recorded milestone.")),
            ]),
        ),
        (
            "challenge_resolved".to_owned(),
            object_type([
                (
                    "challenge_id",
                    required(string(1, 128), "Challenge identity."),
                ),
                ("status", required(challenge_status_type(), "Final status.")),
            ]),
        ),
        (
            "challenge_updated".to_owned(),
            object_type([
                ("band", required(outcome_band_type(), "Result band.")),
                (
                    "challenge_id",
                    required(string(1, 128), "Challenge identity."),
                ),
                ("label", required(string(1, 160), "Challenge label.")),
                (
                    "last_total",
                    required(integer(-1_000.0, 1_000.0), "Check total."),
                ),
                (
                    "status",
                    required(challenge_status_type(), "Challenge status."),
                ),
                ("turns", required(integer(1.0, 1_000.0), "Attempts.")),
            ]),
        ),
        (
            "character_inspected".to_owned(),
            object_type([
                (
                    "attributes",
                    required(named("Attributes"), "Attribute ratings."),
                ),
                (
                    "experience",
                    required(integer(0.0, 1_000_000.0), "Experience."),
                ),
                (
                    "items",
                    required(list(string(0, 128), 0, 64), "Owned item ids."),
                ),
                ("level", required(integer(1.0, 20.0), "Level.")),
                (
                    "qualities",
                    required(named("Qualities"), "Derived qualities."),
                ),
                (
                    "skills",
                    required(list(string(0, 128), 0, 16), "Selected skill ids."),
                ),
                (
                    "wealth",
                    required(integer(0.0, 1_000_000.0), "Remaining wealth."),
                ),
            ]),
        ),
        (
            "combat_concluded".to_owned(),
            object_type([
                ("combat_id", required(string(1, 128), "Combat identity.")),
                ("outcome", required(combat_outcome_type(), "Final outcome.")),
            ]),
        ),
        (
            "combat_moved".to_owned(),
            object_type([
                ("combat_id", required(string(1, 128), "Combat identity.")),
                (
                    "distance_after",
                    required(integer(0.0, 1_000.0), "Distance after."),
                ),
                ("movement", required(integer(1.0, 100.0), "Distance moved.")),
            ]),
        ),
        (
            "combat_started".to_owned(),
            object_type([
                (
                    "adversary_health",
                    required(integer(1.0, 999.0), "Adversary health."),
                ),
                (
                    "adversary_name",
                    required(string(1, 160), "Adversary name."),
                ),
                ("combat_id", required(string(1, 128), "Combat identity.")),
                (
                    "distance",
                    required(integer(0.0, 1_000.0), "Starting distance."),
                ),
                ("label", required(string(1, 160), "Combat label.")),
            ]),
        ),
        (
            "combat_updated".to_owned(),
            object_type([
                (
                    "adversary_health",
                    required(integer(0.0, 999.0), "Adversary health."),
                ),
                ("combat_id", required(string(1, 128), "Combat identity.")),
                (
                    "distance",
                    required(integer(0.0, 1_000.0), "Current distance."),
                ),
                ("health", required(integer(0.0, 999.0), "Character health.")),
                ("round", required(integer(1.0, 1_000.0), "Round number.")),
            ]),
        ),
        (
            "damage_applied".to_owned(),
            object_type([
                ("amount", required(integer(0.0, 999.0), "Damage dealt.")),
                ("combat_id", required(string(1, 128), "Combat identity.")),
                (
                    "health_after",
                    required(integer(0.0, 999.0), "Health after damage."),
                ),
                ("target", required(damage_target_type(), "Damage target.")),
            ]),
        ),
        (
            "initiative_recorded".to_owned(),
            object_type([
                (
                    "adversary_initiative",
                    required(integer(0.0, 40.0), "Adversary initiative."),
                ),
                (
                    "character_first",
                    required(TypeExpression::Bool, "Whether the character acts first."),
                ),
                (
                    "character_initiative",
                    required(integer(0.0, 40.0), "Character initiative."),
                ),
                ("combat_id", required(string(1, 128), "Combat identity.")),
            ]),
        ),
        (
            "item_acquired".to_owned(),
            object_type([
                ("cost", required(integer(0.0, 1_000_000.0), "Wealth cost.")),
                ("item_id", required(string(1, 128), "Item identity.")),
                ("label", required(string(1, 160), "Item label.")),
                (
                    "quantity_after",
                    required(integer(1.0, 9_999.0), "Carried quantity."),
                ),
            ]),
        ),
        (
            "mark_applied".to_owned(),
            object_type([
                ("marks_after", required(integer(1.0, 3.0), "Marks after.")),
                ("reason", required(string(1, 64), "Mark reason.")),
            ]),
        ),
        (
            "recovery_applied".to_owned(),
            object_type([
                (
                    "health_after",
                    required(integer(0.0, 999.0), "Health after."),
                ),
                ("marks_after", required(integer(0.0, 3.0), "Marks after.")),
                ("mode", required(recovery_mode_type(), "Recovery mode.")),
                (
                    "resolve_after",
                    required(integer(0.0, 999.0), "Resolve after."),
                ),
            ]),
        ),
        (
            "roll_recorded".to_owned(),
            object_type([
                ("actor", required(string(1, 32), "Acting side.")),
                ("band", required(outcome_band_type(), "Result band.")),
                (
                    "context_id",
                    required(string(0, 128), "Challenge or combat id."),
                ),
                (
                    "dice",
                    required(list(integer(1.0, 6.0), 2, 4), "Rolled dice."),
                ),
                (
                    "dice_total",
                    required(integer(2.0, 24.0), "Kept dice total."),
                ),
                (
                    "heroic_badge_spent",
                    required(TypeExpression::Bool, "Whether a badge was spent."),
                ),
                (
                    "kept",
                    required(list(integer(1.0, 6.0), 2, 2), "Kept dice."),
                ),
                ("mode", required(roll_mode_type(), "Dice mode.")),
                (
                    "total",
                    required(integer(-1_000.0, 1_000.0), "Final total."),
                ),
            ]),
        ),
        (
            "wealth_spent".to_owned(),
            object_type([
                ("cost", required(integer(0.0, 1_000_000.0), "Wealth cost.")),
                ("item_id", required(string(1, 128), "Item identity.")),
                (
                    "wealth_after",
                    required(integer(0.0, 1_000_000.0), "Remaining wealth."),
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
    FieldDeclaration {
        value_type,
        required: true,
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
    symbol([
        "dexterity",
        "intelligence",
        "magicy",
        "motion",
        "perception",
        "persuasion",
        "strength",
        "vitality",
        "volition",
    ])
}

fn skill_kind_type() -> TypeExpression {
    symbol(["active", "passive"])
}

fn item_kind_type() -> TypeExpression {
    symbol(["armor", "gear", "tool", "weapon"])
}

fn roll_mode_type() -> TypeExpression {
    symbol(["advantage", "disadvantage", "normal"])
}

fn outcome_band_type() -> TypeExpression {
    symbol(["critical", "failure", "partial", "success"])
}

fn challenge_status_type() -> TypeExpression {
    symbol(["failed", "open", "succeeded"])
}

fn combat_status_type() -> TypeExpression {
    symbol(["concluded", "open"])
}

fn recovery_mode_type() -> TypeExpression {
    symbol(["rest", "treat"])
}

fn damage_target_type() -> TypeExpression {
    symbol(["adversary", "character"])
}

fn combat_outcome_type() -> TypeExpression {
    symbol(["defeat", "victory"])
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

fn set_u32(fields: &mut BTreeMap<String, DomainValue>, field: &str, value: u32) {
    fields.insert(field.to_owned(), DomainValue::Number(f64::from(value)));
}

fn number_list(values: &[u32]) -> DomainValue {
    DomainValue::List(
        values
            .iter()
            .map(|value| DomainValue::Number(f64::from(*value)))
            .collect(),
    )
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

/// Complete Apache License 2.0 text retained verbatim for this adapter.
///
/// Apache-2.0 requires distributors to keep the licence text and notices. The text below is the
/// unmodified Apache License, Version 2.0. [`OPEN_TALE_LICENSE_TEXT_SHA256`] pins its exact bytes.
pub const OPEN_TALE_LICENSE_TEXT: &str = r#"                              Apache License
                        Version 2.0, January 2004
                     https://www.apache.org/licenses/LICENSE-2.0

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
"#;

/// SHA-256 of [`OPEN_TALE_LICENSE_TEXT`] as UTF-8 bytes.
pub const OPEN_TALE_LICENSE_TEXT_SHA256: &str =
    "8ada45cd9f843acf64e4722ae262c622a2b3b3007c7310ef36ac1061a30f6adb";
