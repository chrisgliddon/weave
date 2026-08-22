use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use weave_domain::{DomainValue, Provenance};

/// Current serialized Character Profile contract version.
pub const CHARACTER_PROFILE_FORMAT_VERSION: u32 = 1;

/// Current immutable character-template contract version.
pub const CHARACTER_TEMPLATE_FORMAT_VERSION: u32 = 1;

/// Current sparse character-overlay contract version.
pub const CHARACTER_OVERLAY_FORMAT_VERSION: u32 = 1;

/// Current deterministic synthesis-result contract version.
pub const CHARACTER_SYNTHESIS_FORMAT_VERSION: u32 = 1;

/// Portable Character Profile with canon, projections, suggestions, and derived views separated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterProfile {
    /// Serialized profile contract version.
    pub profile_format_version: u32,
    /// Globally stable, namespaced character identifier.
    pub id: String,
    /// Normative authored or imported character data.
    pub canon: CharacterCanon,
    /// Optional, versioned records that cannot write back into canonical personality evidence.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, CharacterExtension>,
    /// Reviewable proposals kept outside canon until an explicit operation accepts them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub suggestions: BTreeMap<String, CharacterSuggestion>,
    /// Reproducible compatibility projections computed from canon.
    pub derived: CharacterDerivedViews,
    /// Machine-readable source and transformation lineage.
    pub provenance: Provenance,
}

/// Normative character data. Optional projections and suggestions live outside this record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterCanon {
    /// Stable identity and human-facing names.
    pub identity: CharacterIdentity,
    /// Explicitly precise date-only birth information, when authored or imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<Attributed<BirthDate>>,
    /// Canonical HEXACO evidence. Missing traits remain absent and are never imputed.
    pub personality: HexacoProfile,
    /// Authored inner-life records keyed by stable local identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inner_life: BTreeMap<String, AuthoredNote>,
    /// Authored voice guidance keyed by stable local identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub voice: BTreeMap<String, VoiceDirection>,
}

/// Stable identity distinct from optional visual presentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterIdentity {
    /// Primary human-readable name.
    pub display_name: Attributed<String>,
    /// Optional ordered aliases; an omitted field means no assertion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Attributed<Vec<String>>>,
}

/// A value together with authority, review, locking, freshness, confidence, and lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Attributed<T> {
    /// Typed value.
    pub value: T,
    /// How this value entered the record.
    pub state: ValueState,
    /// Confidence in the representation, not a claim of objective truth.
    pub confidence: Confidence,
    /// Editorial review state.
    pub review: ReviewState,
    /// Whether ordinary synthesis operations may replace the field.
    pub lock: LockState,
    /// Whether an upstream input changed after this value was produced.
    pub freshness: Freshness,
    /// Sorted source or transformation identifiers from profile provenance.
    pub lineage: Vec<String>,
    /// Required explanation for derived, suggested, reviewed, or overridden values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Distinguishable value states with deterministic precedence.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ValueState {
    /// Unaccepted proposal; lowest precedence and never canonical personality evidence.
    Suggested,
    /// Mechanical projection; never independent evidence and never writable to canon.
    Derived,
    /// Value acquired from a declared external source.
    Imported,
    /// Proposal or import explicitly accepted by a reviewer.
    Reviewed,
    /// Direct author decision.
    Authored,
    /// Explicit replacement naming and fingerprinting the prior value; highest precedence.
    Overridden,
}

impl ValueState {
    /// Stable precedence used by overlay synthesis.
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Suggested => 0,
            Self::Derived => 1,
            Self::Imported => 2,
            Self::Reviewed => 3,
            Self::Authored => 4,
            Self::Overridden => 5,
        }
    }
}

/// Bounded confidence vocabulary shared by evidence and projections.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Unknown,
    Low,
    Moderate,
    High,
}

/// Explicit editorial state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    NotRequired,
    Pending,
    Accepted,
    Rejected,
}

/// Author lock state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LockState {
    Unlocked,
    Locked,
}

/// Whether lineage still matches the exact upstream inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Current,
    Stale,
}

/// Date-only birth data with no fabricated components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "precision", deny_unknown_fields)]
pub enum BirthDate {
    /// Known year; month and day remain unknown.
    Year { calendar: Calendar, year: i32 },
    /// Known recurring month and day; year remains unknown.
    MonthDay {
        calendar: Calendar,
        month: u8,
        day: u8,
    },
    /// Complete date.
    Full {
        calendar: Calendar,
        year: i32,
        month: u8,
        day: u8,
    },
}

/// Closed v1 calendar vocabulary. New calendars require a versioned extension.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Calendar {
    ProlepticGregorian,
}

/// Canonical six-factor, 24-facet HEXACO record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HexacoProfile {
    pub honesty_humility: HonestyHumility,
    pub emotionality: Emotionality,
    pub extraversion: Extraversion,
    pub agreeableness: Agreeableness,
    pub conscientiousness: Conscientiousness,
    pub openness: Openness,
}

/// Honesty-Humility factor and its four canonical facets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HonestyHumility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sincerity: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fairness: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greed_avoidance: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modesty: Option<Attributed<TraitMeasurement>>,
}

/// Emotionality factor and its four canonical facets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Emotionality {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fearfulness: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anxiety: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependence: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentimentality: Option<Attributed<TraitMeasurement>>,
}

/// Extraversion factor and its four canonical facets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Extraversion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social_self_esteem: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social_boldness: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sociability: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveliness: Option<Attributed<TraitMeasurement>>,
}

/// Agreeableness factor and its four canonical facets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Agreeableness {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forgivingness: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gentleness: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flexibility: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patience: Option<Attributed<TraitMeasurement>>,
}

/// Conscientiousness factor and its four canonical facets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Conscientiousness {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diligence: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perfectionism: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prudence: Option<Attributed<TraitMeasurement>>,
}

/// Openness factor and its four canonical facets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Openness {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aesthetic_appreciation: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inquisitiveness: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creativity: Option<Attributed<TraitMeasurement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unconventionality: Option<Attributed<TraitMeasurement>>,
}

/// One canonical trait measurement. A record uses a bounded score or one closed band, never both.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "form", deny_unknown_fields)]
pub enum TraitMeasurement {
    /// Inclusive normalized value from 0.0 through 1.0.
    Score { score: f64 },
    /// Coarse representation with a documented v1 numeric anchor for projections.
    Band { band: TraitBand },
}

/// Closed five-band trait vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TraitBand {
    VeryLow,
    Low,
    Middle,
    High,
    VeryHigh,
}

impl TraitBand {
    /// Stable v1 anchor used only for derived compatibility projections.
    #[must_use]
    pub const fn projection_anchor(self) -> f64 {
        match self {
            Self::VeryLow => 0.1,
            Self::Low => 0.3,
            Self::Middle => 0.5,
            Self::High => 0.7,
            Self::VeryHigh => 0.9,
        }
    }
}

/// Stable identifiers for all six factors and 24 facets.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HexacoTrait {
    HonestyHumility,
    Sincerity,
    Fairness,
    GreedAvoidance,
    Modesty,
    Emotionality,
    Fearfulness,
    Anxiety,
    Dependence,
    Sentimentality,
    Extraversion,
    SocialSelfEsteem,
    SocialBoldness,
    Sociability,
    Liveliness,
    Agreeableness,
    Forgivingness,
    Gentleness,
    Flexibility,
    Patience,
    Conscientiousness,
    Organization,
    Diligence,
    Perfectionism,
    Prudence,
    Openness,
    AestheticAppreciation,
    Inquisitiveness,
    Creativity,
    Unconventionality,
}

/// One stable authored inner-life record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthoredNote {
    pub id: String,
    pub category: InnerLifeCategory,
    pub content: Attributed<String>,
}

/// Closed baseline categories; custom taxonomies use a versioned extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InnerLifeCategory {
    Value,
    Fear,
    Desire,
    Contradiction,
    Memory,
    Boundary,
}

/// One stable authored voice direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VoiceDirection {
    pub id: String,
    pub category: VoiceCategory,
    pub content: Attributed<String>,
}

/// Closed baseline voice categories; detailed expression data remains an extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCategory {
    Cadence,
    Tone,
    Vocabulary,
    Constraint,
    Sample,
}

/// Reproducible, non-authoritative views derived from canon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterDerivedViews {
    /// Lossy HEXACO-to-OCEAN compatibility view.
    pub ocean: OceanView,
}

/// Explicitly lossy OCEAN compatibility projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OceanView {
    pub algorithm: String,
    pub algorithm_version: u32,
    pub lossy: bool,
    pub independent_evidence: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openness: Option<DerivedTrait>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conscientiousness: Option<DerivedTrait>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraversion: Option<DerivedTrait>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agreeableness: Option<DerivedTrait>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neuroticism: Option<DerivedTrait>,
    /// Canonical factor intentionally omitted by this five-factor view.
    pub omitted_factor: HexacoTrait,
}

/// One derived dimension with exact input paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DerivedTrait {
    pub score: f64,
    pub confidence: Confidence,
    pub input_paths: Vec<String>,
}

/// A proposal that remains non-canonical until an explicit reviewed operation accepts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterSuggestion {
    pub id: String,
    pub target_path: String,
    pub proposal: Attributed<DomainValue>,
}

/// One versioned optional extension.
// These are persisted authoring records rather than a hot-path queue. Keeping variants direct
// preserves a symmetric public pattern-matching API and a stable serialized contract.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    tag = "kind",
    content = "record",
    deny_unknown_fields
)]
pub enum CharacterExtension {
    IdentityPresentation(VersionedExtension<IdentityPresentation>),
    Expression(VersionedExtension<ExpressionData>),
    BehavioralSignatures(VersionedExtension<BehavioralSignatures>),
    RoleProjections(VersionedExtension<RoleProjections>),
    Relationships(VersionedExtension<RelationshipEdges>),
    AlignmentView(VersionedExtension<AlignmentView>),
    DateContext(VersionedExtension<DateContext>),
    Tabletop(VersionedExtension<OpaqueExtensionData>),
    Opaque(VersionedExtension<OpaqueExtensionData>),
}

/// Shared envelope for optional records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VersionedExtension<T> {
    pub header: ExtensionHeader,
    pub value: T,
}

/// Authority and lifecycle metadata required for every optional extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtensionHeader {
    pub namespace: String,
    pub extension_version: u32,
    pub authority: String,
    pub rationale: String,
    pub state: ValueState,
    pub review: ReviewState,
    pub lock: LockState,
    pub freshness: Freshness,
    pub lineage: Vec<String>,
    /// Always `forbidden` in v1; projections cannot mutate canonical personality evidence.
    pub canonical_personality_write_back: ExtensionWriteBack,
}

/// V1 write-back policy has no permissive state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionWriteBack {
    Forbidden,
}

/// Optional identity and project-relative presentation references.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdentityPresentation {
    pub identity_refs: Vec<String>,
    pub presentation_refs: Vec<String>,
    /// Optional authored language used to refer to the character.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<Attributed<PronounSet>>,
    /// Authored origin or context notes keyed by stable local identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context_notes: BTreeMap<String, IdentityContextNote>,
    /// Appearance descriptions keyed by stable local identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub appearance: BTreeMap<String, AppearanceDescriptor>,
    /// Optional named color slots for portable presentation consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<Attributed<PresentationPalette>>,
    /// Optional sorted style tags. Tags describe presentation only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_tags: Option<Attributed<Vec<String>>>,
    /// Portable, project-relative asset references keyed by stable local identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, Attributed<PresentationAssetReference>>,
    /// Reviewed catalog allocations keyed by presentation slot.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub catalog_assignments: BTreeMap<String, Attributed<PresentationCatalogAssignment>>,
}

/// Explicit grammatical forms without assuming a fixed pronoun vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PronounSet {
    pub subject: String,
    pub object: String,
    pub possessive_determiner: String,
    pub possessive_pronoun: String,
    pub reflexive: String,
}

/// One authored identity-adjacent note that remains outside personality canon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdentityContextNote {
    pub id: String,
    pub kind: IdentityContextKind,
    pub content: Attributed<String>,
}

/// Closed baseline identity-note categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityContextKind {
    Origin,
    Context,
}

/// One authored appearance statement with a namespaced category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppearanceDescriptor {
    pub id: String,
    pub category: String,
    pub content: Attributed<String>,
}

/// Named portable color slots. Values use canonical hexadecimal sRGB notation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationPalette {
    pub colors: BTreeMap<String, String>,
}

/// Closed portable asset roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PresentationAssetKind {
    Avatar,
    Portrait,
    Sprite,
    Model,
    Illustration,
}

/// One project-relative asset coordinate with optional content fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationAssetReference {
    pub id: String,
    pub kind: PresentationAssetKind,
    pub path: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub alt_text: String,
}

/// Exact immutable presentation-catalog coordinate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationCatalogRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

/// Closed values a presentation catalog may propose. None can modify identity or personality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum PresentationCatalogValue {
    Appearance {
        category: String,
        descriptor: String,
    },
    PaletteColor {
        palette_slot: String,
        color: String,
    },
    StyleTag {
        tag: String,
    },
    Asset {
        asset: PresentationAssetReference,
    },
}

/// One reviewed catalog allocation retained with proposal and review fingerprints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationCatalogAssignment {
    pub slot_id: String,
    pub catalog: PresentationCatalogRef,
    pub entry_id: String,
    pub value: PresentationCatalogValue,
    pub proposal_sha256: String,
    pub review_sha256: String,
}

/// Normalized expression and preference records plus behavioral-signature links.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpressionData {
    pub lexicon: BTreeMap<String, NormalizedExpressionTerm>,
    pub preferences: BTreeMap<String, NormalizedPreference>,
    pub behavioral_signature_refs: Vec<String>,
    pub source_pack_refs: Vec<String>,
}

/// One normalized lexicon entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NormalizedExpressionTerm {
    pub id: String,
    pub category: String,
    pub normalized: String,
    pub strength: f64,
}

/// One normalized preference record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NormalizedPreference {
    pub id: String,
    pub category: String,
    pub target: String,
    pub polarity: PreferencePolarity,
    pub strength: f64,
}

/// Closed baseline preference direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreferencePolarity {
    Prefer,
    Avoid,
}

/// Stable behavioral signatures remain projections, not personality evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BehavioralSignatures {
    pub signatures: BTreeMap<String, BehavioralSignature>,
}

/// One normalized behavioral signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BehavioralSignature {
    pub id: String,
    pub cue: String,
    pub strength: f64,
}

/// Accepted role projections keyed by stable identifier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleProjections {
    pub roles: BTreeMap<String, RoleProjection>,
}

/// A reviewed narrative or vocational projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleProjection {
    pub id: String,
    pub taxonomy: String,
    pub role: String,
    pub rationale: String,
    pub input_paths: Vec<String>,
}

/// Character relationship references; richer graph semantics remain in the relationship extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipEdges {
    pub edges: BTreeMap<String, RelationshipEdge>,
}

/// One portable relationship edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelationshipEdge {
    pub id: String,
    pub source_character_id: String,
    pub target_character_id: String,
    pub kind: String,
    pub confidence: Confidence,
}

/// One reviewed, pluggable alignment view with declared authority and rationale in its header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentView {
    pub view_id: String,
    pub pack: AlignmentPackRef,
    /// Approved public values keyed by stable axis identifier. Review traces stay in receipts.
    pub values: BTreeMap<String, ApprovedAlignmentValue>,
    pub input_paths: Vec<String>,
    pub review_sha256: String,
    pub applied_sha256: String,
}

/// Exact alignment-pack coordinate retained by a reviewed public view.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignmentPackRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

/// One approved narrative alignment value. It is shorthand, never diagnostic evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovedAlignmentValue {
    pub id: String,
    pub label_id: String,
    pub label: String,
    pub decision: AlignmentPublicDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_micros: Option<i32>,
    pub coverage_micros: u32,
    pub explanation: String,
    pub input_paths: Vec<String>,
}

/// Review path by which an alignment value entered the approved public view.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentPublicDecision {
    Reviewed,
    Edited,
    Overridden,
}

/// Accepted or proposed date-context references. They are authoring cues, never causal evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DateContext {
    pub context_pack: String,
    pub context_version: String,
    pub context_hash: String,
    /// Additional exact context packs used by one reviewed enrichment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_context_packs: Vec<DateContextPackRef>,
    pub accepted_record_ids: Vec<String>,
    /// Approved public cues keyed by their stable candidate id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accepted_cues: BTreeMap<String, AcceptedDateContextCue>,
}

/// Exact temporal-context pack coordinate retained by an accepted public view.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DateContextPackRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

/// Fictional authoring surface of one temporal cue.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DateContextCueKind {
    Affinity,
    Tension,
    Value,
    Memory,
    Voice,
}

/// Bounded uncertainty retained in the approved public projection.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DateContextUncertainty {
    Exact,
    Bounded,
    Disputed,
}

/// Sensitivity band controlling auto-approval and review policy.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DateContextSensitivity {
    Low,
    Moderate,
    High,
}

/// How an approved temporal cue entered the public view.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DateContextDecision {
    Accepted,
    AutoApproved,
    Edited,
    Overridden,
}

/// One reviewed fictional cue exposed to narrative logic without writing into canon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptedDateContextCue {
    pub id: String,
    pub record_id: String,
    pub pack: DateContextPackRef,
    pub kind: DateContextCueKind,
    pub content: String,
    pub relevance: f64,
    pub uncertainty: DateContextUncertainty,
    pub sensitivity: DateContextSensitivity,
    pub decision: DateContextDecision,
    /// Public lineage for the matched dated fact; never used as personality evidence.
    pub fact_source_ids: Vec<String>,
    /// Public or original lineage for the fictional cue and its declared ranking vector.
    pub cue_source_ids: Vec<String>,
    /// Sorted union of fact and cue lineage retained for compatibility.
    pub source_ids: Vec<String>,
    pub review_sha256: String,
}

/// Forward-compatible payload preserved without interpretation or write-back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpaqueExtensionData {
    pub interpretation: OpaqueInterpretation,
    pub payload: DomainValue,
}

/// Unknown extension versions are retained only in this inactive state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpaqueInterpretation {
    PreservedInactive,
}

/// Immutable versioned template input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterTemplate {
    pub template_format_version: u32,
    pub id: String,
    pub version: String,
    pub profile: CharacterProfile,
}

/// Exact immutable template coordinate retained by an overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterTemplateRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

/// Sparse operations applied over a separately inspectable template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterOverlay {
    pub overlay_format_version: u32,
    pub id: String,
    pub character_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<CharacterTemplateRef>,
    /// Operations must be canonically ordered by target path and then operation id.
    pub operations: Vec<CharacterOperation>,
    pub provenance: Provenance,
}

/// One typed, reviewable profile operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterOperation {
    pub id: String,
    /// Required when replacing or removing a present value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_prior_sha256: Option<String>,
    pub rationale: String,
    pub action: CharacterOperationAction,
}

/// Closed v1 operation vocabulary shared by source tooling, editor actions, CLI, RON, and JSON.
// Operation documents are infrequent review artifacts; direct variants keep the public API and
// serde shape uniform across all actions.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
pub enum CharacterOperationAction {
    SetDisplayName {
        value: Attributed<String>,
    },
    SetAliases {
        value: Attributed<Vec<String>>,
    },
    ClearAliases,
    SetPronouns {
        value: Attributed<PronounSet>,
    },
    ClearPronouns,
    UpsertIdentityContextNote {
        record: IdentityContextNote,
    },
    RemoveIdentityContextNote {
        id: String,
    },
    UpsertAppearanceDescriptor {
        record: AppearanceDescriptor,
    },
    RemoveAppearanceDescriptor {
        id: String,
    },
    SetPresentationPalette {
        value: Attributed<PresentationPalette>,
    },
    ClearPresentationPalette,
    SetPresentationStyleTags {
        value: Attributed<Vec<String>>,
    },
    ClearPresentationStyleTags,
    UpsertPresentationAsset {
        value: Attributed<PresentationAssetReference>,
    },
    RemovePresentationAsset {
        id: String,
    },
    UpsertPresentationAssignment {
        value: Attributed<PresentationCatalogAssignment>,
    },
    RemovePresentationAssignment {
        slot_id: String,
    },
    SetBirthDate {
        value: Attributed<BirthDate>,
    },
    ClearBirthDate,
    SetHexacoTrait {
        trait_id: HexacoTrait,
        value: Attributed<TraitMeasurement>,
    },
    ClearHexacoTrait {
        trait_id: HexacoTrait,
    },
    UpsertInnerLife {
        record: AuthoredNote,
    },
    RemoveInnerLife {
        id: String,
    },
    UpsertVoice {
        record: VoiceDirection,
    },
    RemoveVoice {
        id: String,
    },
    UpsertSuggestion {
        suggestion: CharacterSuggestion,
    },
    RemoveSuggestion {
        id: String,
    },
    UpsertExtension {
        namespace: String,
        extension: CharacterExtension,
    },
    RemoveExtension {
        namespace: String,
    },
}

/// Full deterministic synthesis proof retaining immutable input, sparse overlay, and effective data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterSynthesisResult {
    pub synthesis_format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<CharacterTemplate>,
    pub overlay: CharacterOverlay,
    pub effective_profile: CharacterProfile,
    /// Effective field paths mapped to exact template or operation ownership.
    pub origins: BTreeMap<String, SynthesisOrigin>,
}

/// Exact source of one effective field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SynthesisOrigin {
    Template {
        template_id: String,
        template_version: String,
        source_path: String,
    },
    Overlay {
        overlay_id: String,
        operation_id: String,
    },
}

/// Stable redaction-safe diagnostic codes for the Character contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CharacterDiagnosticCode {
    #[serde(rename = "C100")]
    UnsupportedVersion,
    #[serde(rename = "C101")]
    InvalidIdentifier,
    #[serde(rename = "C102")]
    InvalidValue,
    #[serde(rename = "C103")]
    InvalidLineage,
    #[serde(rename = "C104")]
    InvalidExtension,
    #[serde(rename = "C105")]
    ConflictingOverlay,
    #[serde(rename = "C106")]
    LockedField,
    #[serde(rename = "C107")]
    StaleInput,
    #[serde(rename = "C108")]
    InvalidReference,
    #[serde(rename = "C109")]
    ForbiddenWriteBack,
    #[serde(rename = "C110")]
    InvalidEncoding,
}

/// Typed diagnostic shape shared by every Character surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterDiagnostic {
    pub code: CharacterDiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub path: String,
    /// Static redaction-safe explanation; rejected values are never included.
    pub message: String,
}

/// Diagnostic severity vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}
