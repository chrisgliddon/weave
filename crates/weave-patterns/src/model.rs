use std::collections::BTreeMap;
use std::fmt;

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

/// A serializable semantic field value independent of any host runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum PatternValue {
    /// Null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// Display text.
    String(String),
    /// Meaning-bearing atom.
    Symbol(String),
    /// Ordered values.
    List(Vec<Self>),
    /// Deterministically ordered structured fields.
    Object(BTreeMap<String, Self>),
}

impl fmt::Display for PatternValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => formatter.write_str("null"),
            Self::Bool(value) => value.fmt(formatter),
            Self::Number(value) if value.fract() == 0.0 => write!(formatter, "{value:.0}"),
            Self::Number(value) => value.fmt(formatter),
            Self::String(value) | Self::Symbol(value) => formatter.write_str(value),
            Self::List(values) => {
                formatter.write_str("[")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    value.fmt(formatter)?;
                }
                formatter.write_str("]")
            }
            Self::Object(fields) => {
                formatter.write_str("{")?;
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    write!(formatter, "{name}: {value}")?;
                }
                formatter.write_str("}")
            }
        }
    }
}

/// One immutable item that a pattern system may draw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternElement {
    /// Stable identity unique within the system.
    pub id: String,
    /// Semantic fields visible to narrative expressions.
    pub fields: BTreeMap<String, PatternValue>,
    /// Fields that replace same-named upright fields on a reversed draw.
    pub reversed_fields: BTreeMap<String, PatternValue>,
    /// Whether this element may be reversed.
    pub reversible: bool,
}

impl PatternElement {
    /// Construct one upright-only element.
    #[must_use]
    pub fn new(id: impl Into<String>, fields: BTreeMap<String, PatternValue>) -> Self {
        Self {
            id: id.into(),
            fields,
            reversed_fields: BTreeMap::new(),
            reversible: false,
        }
    }
}

/// Named position sequence applied to one draw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpreadDefinition {
    /// Stable spread name.
    pub name: String,
    /// Ordered semantic position names.
    pub positions: Vec<String>,
}

/// Selection algorithm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DrawMethod {
    /// Every eligible element has equal probability.
    Uniform,
    /// Positive numeric field values define relative probability.
    Weighted {
        /// Element field containing its weight.
        field: String,
    },
    /// Six I-Ching lines generated from three fair coins each.
    ThreeCoin,
    /// Six I-Ching lines generated with traditional yarrow-stalk probabilities.
    YarrowStalks,
}

/// Whether and how generic elements may be reversed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReversalPolicy {
    /// Never reverse draws.
    Never,
    /// Give reversible elements an equal upright/reversed chance.
    Half,
}

/// Complete immutable pattern definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternDefinition {
    /// Schema version for this definition.
    pub version: u32,
    /// Stable system identity.
    pub id: String,
    /// Human-readable system name.
    pub name: String,
    /// Source-ordered elements.
    pub elements: Vec<PatternElement>,
    /// Named spread definitions.
    pub spreads: BTreeMap<String, SpreadDefinition>,
    /// Default selection algorithm.
    pub default_method: DrawMethod,
    /// Whether one draw may contain the same element more than once.
    pub allow_duplicates: bool,
    /// Default reversal behavior.
    pub reversal_policy: ReversalPolicy,
}

/// Serializable mutable state for one pattern instance in one story.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternState {
    /// Number of successful draw requests.
    pub draws: u64,
    /// Stable identities from the most recent successful draw.
    pub last_draw: Vec<String>,
}

/// One requested draw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawRequest {
    /// Named spread, or `None` for one element without a position.
    pub spread: Option<String>,
    /// Per-request method override.
    pub method: Option<DrawMethod>,
    /// Per-request duplicate override.
    pub allow_duplicates: Option<bool>,
    /// Per-request reversal override.
    pub reversal_policy: Option<ReversalPolicy>,
}

/// One structured item in a draw result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawnElement {
    /// Stable element identity.
    pub id: String,
    /// Spread position, absent for an unpositioned single draw.
    pub position: Option<String>,
    /// Whether reversal semantics were applied.
    pub reversed: bool,
    /// Effective semantic fields after reversal overlays.
    pub fields: BTreeMap<String, PatternValue>,
}

impl DrawnElement {
    /// Conventional `name` field when present.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self.fields.get("name") {
            Some(PatternValue::String(value) | PatternValue::Symbol(value)) => Some(value),
            _ => None,
        }
    }

    /// Conventional `meaning` field when present.
    #[must_use]
    pub fn meaning(&self) -> Option<&PatternValue> {
        self.fields.get("meaning")
    }
}

/// Complete serializable draw result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawResult {
    /// Pattern-system identity.
    pub system: String,
    /// Spread identity when used.
    pub spread: Option<String>,
    /// Effective draw method.
    pub method: DrawMethod,
    /// Drawn entries in narrative position order.
    pub entries: Vec<DrawnElement>,
    /// Method-specific structured metadata.
    pub metadata: BTreeMap<String, PatternValue>,
}

/// Structured validation or draw failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PatternError {
    /// Definition schema version is unsupported.
    #[error("P1000: unsupported pattern model version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
    /// A required identity is missing or malformed.
    #[error("P1001: invalid {kind} identifier `{value}`")]
    InvalidIdentifier {
        /// Identity category.
        kind: &'static str,
        /// Invalid value.
        value: String,
    },
    /// A definition has no selectable elements.
    #[error("P1002: pattern `{0}` has no elements")]
    EmptyPattern(String),
    /// Element identity appears more than once.
    #[error("P1003: pattern `{system}` repeats element `{element}`")]
    DuplicateElement { system: String, element: String },
    /// Spread identity appears more than once.
    #[error("P1004: pattern `{system}` repeats spread `{spread}`")]
    DuplicateSpread { system: String, spread: String },
    /// Spread has no usable positions.
    #[error("P1005: spread `{system}.{spread}` has no positions")]
    EmptySpread { system: String, spread: String },
    /// Position appears more than once in a spread.
    #[error("P1006: spread `{system}.{spread}` repeats position `{position}`")]
    DuplicatePosition {
        system: String,
        spread: String,
        position: String,
    },
    /// Requested spread does not exist.
    #[error("P1100: pattern `{system}` has no spread `{spread}`")]
    UnknownSpread { system: String, spread: String },
    /// A without-replacement draw requests more items than exist.
    #[error("P1101: draw requests {requested} unique elements but `{system}` has {available}")]
    InsufficientElements {
        system: String,
        requested: usize,
        available: usize,
    },
    /// Weighted drawing field is absent, non-numeric, non-finite, or non-positive.
    #[error("P1102: element `{element}` has invalid positive weight field `{field}`")]
    InvalidWeight { element: String, field: String },
    /// The selected method is not implemented by this system.
    #[error("P1103: pattern `{system}` does not support draw method {method:?}")]
    UnsupportedMethod { system: String, method: DrawMethod },
    /// Reversal is enabled without any reversible element semantics.
    #[error("P1104: pattern `{0}` enables reversals without reversible elements")]
    InvalidReversalConfiguration(String),
    /// Compiled custom pattern data cannot form a valid definition.
    #[error("P1200: invalid authored pattern `{system}`: {message}")]
    InvalidAuthoredData { system: String, message: String },
}

/// Host-provided deterministic entropy stream.
pub trait RandomSource {
    /// Return the next deterministic 64-bit value.
    fn next_u64(&mut self) -> u64;
}

/// ChaCha8 random source that can resume after a known number of values.
pub struct SeededRandom {
    random: ChaCha8Rng,
    consumed: u64,
}

impl SeededRandom {
    /// Start from `seed`, skipping `offset` prior values.
    #[must_use]
    pub fn new(seed: u64, offset: u64) -> Self {
        let mut random = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..offset {
            let _ = random.next_u64();
        }
        Self {
            random,
            consumed: 0,
        }
    }

    /// Values consumed since construction.
    #[must_use]
    pub const fn consumed(&self) -> u64 {
        self.consumed
    }
}

impl RandomSource for SeededRandom {
    fn next_u64(&mut self) -> u64 {
        self.consumed = self.consumed.saturating_add(1);
        self.random.next_u64()
    }
}

/// Object-safe executable pattern system.
pub trait PatternSystem: Send + Sync + fmt::Debug {
    /// Immutable definition shared by every story instance.
    fn definition(&self) -> &PatternDefinition;

    /// Execute one draw, mutating only caller-owned story state.
    fn draw(
        &self,
        request: &DrawRequest,
        state: &mut PatternState,
        random: &mut dyn RandomSource,
    ) -> Result<DrawResult, PatternError>;
}

pub(crate) fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}
