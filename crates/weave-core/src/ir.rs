//! Versioned, runtime-facing intermediate representation.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ast::Span;

/// Current serialized story format version.
pub const IR_VERSION: u32 = 7;

/// Complete immutable compiled story.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StoryIr {
    /// Serialized schema version. Must equal [`IR_VERSION`].
    pub version: u32,
    /// Optional source path or display name.
    pub source_name: Option<String>,
    /// Default entry knot.
    pub entry: String,
    /// Startup instructions evaluated before entering the first knot.
    pub globals: Vec<Instruction>,
    /// Sorted grammar definitions.
    pub grammars: BTreeMap<String, GrammarIr>,
    /// Sorted executable pattern definitions.
    pub patterns: BTreeMap<String, PatternSystemIr>,
    /// Active, validated domain modules keyed by story-local alias.
    pub modules: BTreeMap<String, DomainModuleIr>,
    /// Declared background scenes and character stages.
    #[serde(default, skip_serializing_if = "PresentationIr::is_empty")]
    pub presentation: PresentationIr,
    /// Declared text channels keyed by name. Always contains `narration`.
    pub channels: BTreeMap<String, ChannelIr>,
    /// Interface string tables: table name, then entry key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub strings: BTreeMap<String, BTreeMap<String, StringEntryIr>>,
    /// Sorted knot definitions.
    pub knots: BTreeMap<String, KnotIr>,
}

impl StoryIr {
    /// Create an empty story at the current IR version with the supplied entry point.
    #[must_use]
    pub fn new(entry: impl Into<String>) -> Self {
        Self {
            version: IR_VERSION,
            source_name: None,
            entry: entry.into(),
            globals: Vec::new(),
            grammars: BTreeMap::new(),
            patterns: BTreeMap::new(),
            modules: BTreeMap::new(),
            presentation: PresentationIr::default(),
            channels: BTreeMap::from([(
                crate::markup::NARRATION_CHANNEL.to_owned(),
                ChannelIr::default(),
            )]),
            strings: BTreeMap::new(),
            knots: BTreeMap::new(),
        }
    }
}

const fn animated_default() -> bool {
    true
}

/// Where and how a host places text: a dialogue box, a book page, a sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelIr {
    /// Whether text on this channel types on over time. Defaults to `true`.
    #[serde(default = "animated_default")]
    pub animated: bool,
    /// Characters per second for animated text, when the author sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cps: Option<f64>,
    /// Author-defined settings the host interprets, sorted by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, ValueLiteral>,
}

impl Default for ChannelIr {
    fn default() -> Self {
        Self {
            animated: true,
            cps: None,
            meta: BTreeMap::new(),
        }
    }
}

/// One interface string, rendered at compile time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StringEntryIr {
    /// Plain text with markup removed.
    pub text: String,
    /// Unreal rich-text form of the value.
    pub rich: String,
    /// Style spans, offsets in Unicode scalar values.
    pub spans: Vec<StyleSpanIr>,
    /// Pauses, offsets in Unicode scalar values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beats: Vec<BeatIr>,
    /// Bare tags in source order.
    pub tags: Vec<String>,
    /// `key:value` tags sorted by key.
    pub meta: BTreeMap<String, String>,
}

/// A styled range of rendered text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StyleSpanIr {
    /// Style name from `<name>…</name>`.
    pub style: String,
    /// Inclusive start offset in Unicode scalar values.
    pub from: usize,
    /// Exclusive end offset in Unicode scalar values.
    pub to: usize,
}

/// A pause at one position in rendered text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BeatIr {
    /// Offset in Unicode scalar values at which the pause happens.
    pub at: usize,
    /// Pause length in seconds.
    pub pause: f64,
}

/// One narrative line with its stable id and writer metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LineIr {
    /// Stable line id: `^id:` or the default `<knot>.<n>`.
    pub id: String,
    /// Text template, including markup parts.
    pub template: Template,
    /// Bare tags in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `key:value` tags sorted by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

/// Declared visual assets addressable by presentation cues.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresentationIr {
    /// Sorted background scenes keyed by scene identifier.
    pub scenes: BTreeMap<String, SceneIr>,
    /// Sorted character stages keyed by stage identifier.
    pub stages: BTreeMap<String, StageIr>,
}

impl PresentationIr {
    /// Whether no scenes or stages are declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty() && self.stages.is_empty()
    }
}

/// Compiled background scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneIr {
    /// Optional relative background asset path.
    pub background: Option<String>,
    /// How the background fills the stage frame.
    #[serde(default)]
    pub fit: Fit,
    /// Normalized focal point within the background image.
    #[serde(default)]
    pub anchor: AnchorIr,
}

impl Slot {
    /// Parse one source slot token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "far_left" => Some(Self::FarLeft),
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            "far_right" => Some(Self::FarRight),
            _ => None,
        }
    }
}

/// How a scene background fills the rendered frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Scale to fill the frame, cropping overflow.
    #[default]
    Cover,
    /// Scale to fit inside the frame, preserving the whole image.
    Contain,
}

impl Fit {
    /// Parse one source fit token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "cover" => Some(Self::Cover),
            "contain" => Some(Self::Contain),
            _ => None,
        }
    }
}

/// Normalized background focal point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnchorIr {
    /// Horizontal position in the closed range 0..=1.
    pub x: f64,
    /// Vertical position in the closed range 0..=1.
    pub y: f64,
}

impl Default for AnchorIr {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Named horizontal position for a character stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    /// Far-left position.
    FarLeft,
    /// Left position.
    Left,
    /// Centre position.
    #[default]
    Center,
    /// Right position.
    Right,
    /// Far-right position.
    FarRight,
}

/// Compiled character stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageIr {
    /// Optional relative character image asset path.
    pub image: Option<String>,
    /// Optional author-facing display name.
    pub name: Option<String>,
    /// Named horizontal position.
    #[serde(default)]
    pub slot: Slot,
    /// Paint order in the closed range 0..=9; higher layers paint later.
    #[serde(default)]
    pub layer: u8,
    /// Sorted named variant images keyed by variant identifier.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, String>,
}

/// One exact, validated domain-module activation embedded for runtime hosts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainModuleIr {
    /// Globally stable module identity.
    pub id: String,
    /// Exact selected module semantic version.
    pub version: String,
    /// Exact selected pack identity.
    pub pack_id: String,
    /// Exact selected pack semantic version.
    pub pack_version: String,
    /// Validated exported values in deterministic name order.
    pub exports: BTreeMap<String, DomainExportIr>,
    /// Canonically ordered fictional replacements, separate from immutable pack defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authored_overrides: Vec<DomainOverrideIr>,
}

impl DomainModuleIr {
    /// Read one nested export path without depending on a runtime or editor.
    #[must_use]
    pub fn value(&self, path: &[&str]) -> Option<&DomainValueIr> {
        let (export, fields) = path.split_first()?;
        let mut value = &self.exports.get(*export)?.value;
        for field in fields {
            let DomainValueIr::Object(values) = value else {
                return None;
            };
            value = values.get(*field)?;
        }
        Some(value)
    }
}

/// One source-authored domain replacement retained for provenance-aware consumers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainOverrideIr {
    /// Export path beginning with the export name.
    pub path: Vec<String>,
    /// Compile-time constant fictional value.
    pub value: DomainValueIr,
}

/// One module export plus its runtime ownership boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainExportIr {
    /// Whether the value remains immutable pack data or initializes module-owned state.
    pub source: DomainExportSourceIr,
    /// Validated initial value.
    pub value: DomainValueIr,
}

/// Ownership of one compiled domain export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DomainExportSourceIr {
    /// Immutable value embedded from a selected pack.
    Pack,
    /// Initial value copied into isolated, versioned runtime state.
    State,
}

/// Unambiguous portable domain value embedded in story IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    tag = "kind",
    content = "value",
    deny_unknown_fields
)]
pub enum DomainValueIr {
    /// Explicit null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// UTF-8 string.
    String(String),
    /// Meaning-bearing symbol.
    Symbol(String),
    /// Ordered values.
    List(Vec<DomainValueIr>),
    /// Deterministically ordered fields.
    Object(BTreeMap<String, DomainValueIr>),
}

/// Compiled generative grammar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GrammarIr {
    /// Sorted rules, each with one or more uniform alternatives.
    pub rules: BTreeMap<String, Vec<Template>>,
}

/// Compiled custom or built-in pattern definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PatternSystemIr {
    /// Optional built-in data and algorithm source.
    pub builtin: Option<BuiltinPatternIr>,
    /// Sorted named collections.
    pub collections: BTreeMap<String, PatternCollectionIr>,
    /// Sorted named spreads.
    pub spreads: BTreeMap<String, SpreadIr>,
    /// Default draw algorithm.
    pub draw_method: PatternDrawMethodIr,
    /// Whether one spread draw may repeat an element.
    pub allow_duplicates: bool,
    /// Whether reversible elements may be drawn reversed.
    pub reversals: bool,
}

/// Built-in pattern data set and algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinPatternIr {
    /// Complete 78-card tarot system.
    Tarot,
    /// Complete 64-hexagram I-Ching system.
    IChing,
    /// Complete 24-rune Elder Futhark system.
    ElderFuthark,
}

/// Compiled default pattern draw algorithm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PatternDrawMethodIr {
    /// Equal-probability element selection.
    Uniform,
    /// Positive numeric field values define relative probability.
    WeightedBy {
        /// Weight field name.
        field: String,
    },
    /// I-Ching three-coin generation.
    ThreeCoin,
    /// I-Ching yarrow-stalk generation.
    YarrowStalks,
}

/// Named pattern collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PatternCollectionIr {
    /// Source-ordered semantic elements.
    pub elements: Vec<PatternElementIr>,
}

/// Structured pattern element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PatternElementIr {
    /// Deterministically sorted fields.
    pub fields: BTreeMap<String, ValueLiteral>,
}

/// Named spread definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpreadIr {
    /// Ordered symbolic positions.
    pub positions: Vec<String>,
}

/// Compiled narrative knot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KnotIr {
    /// Source-ordered executable instructions.
    pub content: Vec<Instruction>,
}

/// One source-spanned executable instruction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Instruction {
    /// Author-facing source location.
    pub span: Span,
    /// Runtime operation.
    pub kind: InstructionKind,
}

/// Runtime operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum InstructionKind {
    /// Deliver one rendered narrative line.
    Text(LineIr),
    /// Declare or reset a story value.
    Declare(DeclarationIr),
    /// Assign an existing story value.
    Assign {
        /// Target name.
        name: String,
        /// New value.
        value: Expression,
    },
    /// Append or remove one list member.
    MutateList {
        /// Operation.
        operation: ListOperationIr,
        /// Target list.
        name: String,
        /// Member value.
        value: Expression,
    },
    /// One member of an adjacent runtime choice set.
    Choice(ChoiceIr),
    /// Ordered conditional branch set.
    Conditional(ConditionalIr),
    /// Replace the execution stack with another knot.
    Divert(String),
    /// Call another knot and return on natural completion.
    Thread(String),
    /// Apply one presentation directive without ending the current advance.
    Cue(CueIr),
    /// Send following lines to a declared channel.
    Channel(String),
    /// End the story.
    End,
}

/// Compiled presentation cue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum CueIr {
    /// Switch to a declared background scene.
    Scene(String),
    /// Show a declared character stage.
    Show(String),
    /// Hide a declared character stage.
    Hide(String),
    /// Set the current speaker; `None` is narration.
    Speaker(Option<String>),
    /// Move a declared character stage to a named slot.
    Move {
        /// Stage identifier.
        stage: String,
        /// Destination slot.
        to: Slot,
    },
    /// Replace a declared stage image with one of its named variants.
    Swap {
        /// Stage identifier.
        stage: String,
        /// Variant identifier declared on that stage.
        variant: String,
    },
    /// Request a bounded transition between presentation states.
    Transition {
        /// Transition style.
        kind: TransitionKind,
        /// Nominal duration in milliseconds.
        ms: u16,
    },
}

/// Compiled transition style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// Immediate replacement.
    Cut,
    /// Cross-fade.
    Fade,
    /// Directional slide.
    Slide,
    /// Reveal wipe.
    Wipe,
}

impl TransitionKind {
    /// Parse one source transition-kind token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "cut" => Some(Self::Cut),
            "fade" => Some(Self::Fade),
            "slide" => Some(Self::Slide),
            "wipe" => Some(Self::Wipe),
            _ => None,
        }
    }
}

/// Compiled declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeclarationIr {
    /// Explicit declaration kind.
    pub kind: VariableKindIr,
    /// Story-scoped name.
    pub name: String,
    /// Initial value.
    pub value: Expression,
    /// Allowed symbols for state machines.
    pub allowed_states: Vec<String>,
}

/// Runtime declaration kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariableKindIr {
    /// General variable.
    Variable,
    /// Ordered list.
    List,
    /// Boolean flag.
    Flag,
    /// Symbol-constrained state machine.
    State,
}

/// Runtime list mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListOperationIr {
    /// Append one member.
    Push,
    /// Remove the first equal member.
    Remove,
}

/// Compiled choice branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChoiceIr {
    /// Deterministic identifier used for once-choice state.
    pub id: String,
    /// Whether the choice disappears after selection.
    pub once: bool,
    /// Rendered author-facing label.
    pub text: Template,
    /// Optional eligibility condition.
    pub condition: Option<Expression>,
    /// Instructions executed after selection.
    pub body: Vec<Instruction>,
    /// Optional target executed after the body.
    pub divert: Option<String>,
    /// Bare tags in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `key:value` tags sorted by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

/// Ordered conditional branches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConditionalIr {
    /// Conditional branches in source order.
    pub branches: Vec<ConditionalBranchIr>,
    /// Optional final fallback.
    pub fallback: Option<Vec<Instruction>>,
}

/// One compiled conditional branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConditionalBranchIr {
    /// Boolean condition.
    pub condition: Expression,
    /// Instructions executed when true.
    pub body: Vec<Instruction>,
}

/// Pre-parsed text template.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Template {
    /// Source-ordered parts.
    pub parts: Vec<TemplatePartIr>,
}

impl Template {
    /// Construct a template containing only literal text.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            parts: vec![TemplatePartIr::Text(value.into())],
        }
    }
}

/// One runtime template part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum TemplatePartIr {
    /// Literal text.
    Text(String),
    /// Grammar rule expansion.
    GrammarRef {
        /// Explicit grammar, absent only inside a grammar rule.
        grammar: Option<String>,
        /// Rule name.
        rule: String,
    },
    /// Runtime expression interpolation.
    Expression(Expression),
    /// A pause in seconds at this point of a narrative line.
    Pause(f64),
    /// Opening of a named style span.
    StyleOpen(String),
    /// Closing of the innermost style span, which has this name.
    StyleClose(String),
}

/// Runtime expression, deliberately separate from the source AST.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Expression {
    /// Scalar value.
    Literal(ValueLiteral),
    /// Ordered list value.
    List(Vec<Expression>),
    /// Story variable or field path.
    Path(Vec<String>),
    /// Grammar expansion used as an expression.
    GrammarRef {
        /// Explicit grammar name.
        grammar: Option<String>,
        /// Rule name.
        rule: String,
    },
    /// Reserved compiler-recognized call.
    Call {
        /// Dotted function path.
        path: Vec<String>,
        /// Arguments.
        arguments: Vec<Expression>,
    },
    /// Unary operation.
    Unary {
        /// Operator.
        operator: UnaryOperatorIr,
        /// Operand.
        operand: Box<Expression>,
    },
    /// Binary operation.
    Binary {
        /// Left operand.
        left: Box<Expression>,
        /// Operator.
        operator: BinaryOperatorIr,
        /// Right operand.
        right: Box<Expression>,
    },
}

/// Scalar runtime value literal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum ValueLiteral {
    /// Null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// UTF-8 string.
    String(String),
    /// Semantic symbol.
    Symbol(String),
}

/// Runtime unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnaryOperatorIr {
    /// Boolean negation.
    Not,
    /// Numeric negation.
    Negate,
}

/// Runtime binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOperatorIr {
    /// Addition or string concatenation.
    Add,
    /// Subtraction.
    Subtract,
    /// Multiplication.
    Multiply,
    /// Division.
    Divide,
    /// Remainder.
    Remainder,
    /// Equality.
    Equal,
    /// Inequality.
    NotEqual,
    /// Less than.
    Less,
    /// Less than or equal.
    LessEqual,
    /// Greater than.
    Greater,
    /// Greater than or equal.
    GreaterEqual,
    /// Membership.
    In,
    /// Short-circuit conjunction.
    And,
    /// Short-circuit disjunction.
    Or,
}
