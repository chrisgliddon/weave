//! Versioned, runtime-facing intermediate representation.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ast::Span;

/// Current serialized story format version.
pub const IR_VERSION: u32 = 4;

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
            knots: BTreeMap::new(),
        }
    }
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
    Text(Template),
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
    /// End the story.
    End,
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
