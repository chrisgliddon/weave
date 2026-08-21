//! Versioned, runtime-facing intermediate representation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ast::Span;

/// Current serialized story format version.
pub const IR_VERSION: u32 = 1;

/// Complete immutable compiled story.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoryIr {
    /// Serialized schema version. Must equal [`IR_VERSION`] in Phase 1.
    pub version: u32,
    /// Optional source path or display name.
    pub source_name: Option<String>,
    /// Default entry knot.
    pub entry: String,
    /// Startup instructions evaluated before entering the first knot.
    pub globals: Vec<Instruction>,
    /// Sorted grammar definitions.
    pub grammars: BTreeMap<String, GrammarIr>,
    /// Sorted pattern definitions reserved for Phase 2 execution.
    pub patterns: BTreeMap<String, PatternSystemIr>,
    /// Sorted knot definitions.
    pub knots: BTreeMap<String, KnotIr>,
}

impl StoryIr {
    /// Create an empty version-1 story with the supplied entry point.
    #[must_use]
    pub fn new(entry: impl Into<String>) -> Self {
        Self {
            version: IR_VERSION,
            source_name: None,
            entry: entry.into(),
            globals: Vec::new(),
            grammars: BTreeMap::new(),
            patterns: BTreeMap::new(),
            knots: BTreeMap::new(),
        }
    }
}

/// Compiled generative grammar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrammarIr {
    /// Sorted rules, each with one or more uniform alternatives.
    pub rules: BTreeMap<String, Vec<Template>>,
}

/// Compiled pattern definition. Execution is implemented in Phase 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternSystemIr {
    /// Sorted named collections.
    pub collections: BTreeMap<String, PatternCollectionIr>,
    /// Sorted named spreads.
    pub spreads: BTreeMap<String, SpreadIr>,
}

/// Named pattern collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternCollectionIr {
    /// Source-ordered semantic elements.
    pub elements: Vec<PatternElementIr>,
}

/// Structured pattern element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternElementIr {
    /// Deterministically sorted fields.
    pub fields: BTreeMap<String, ValueLiteral>,
}

/// Named spread definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpreadIr {
    /// Ordered symbolic positions.
    pub positions: Vec<String>,
}

/// Compiled narrative knot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnotIr {
    /// Source-ordered executable instructions.
    pub content: Vec<Instruction>,
}

/// One source-spanned executable instruction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instruction {
    /// Author-facing source location.
    pub span: Span,
    /// Runtime operation.
    pub kind: InstructionKind,
}

/// Runtime operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListOperationIr {
    /// Append one member.
    Push,
    /// Remove the first equal member.
    Remove,
}

/// Compiled choice branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionalIr {
    /// Conditional branches in source order.
    pub branches: Vec<ConditionalBranchIr>,
    /// Optional final fallback.
    pub fallback: Option<Vec<Instruction>>,
}

/// One compiled conditional branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionalBranchIr {
    /// Boolean condition.
    pub condition: Expression,
    /// Instructions executed when true.
    pub body: Vec<Instruction>,
}

/// Pre-parsed text template.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnaryOperatorIr {
    /// Boolean negation.
    Not,
    /// Numeric negation.
    Negate,
}

/// Runtime binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
