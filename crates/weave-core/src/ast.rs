//! Source AST definitions.

use serde::{Deserialize, Serialize};

/// Byte and line location in a source file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
}

impl Span {
    /// Construct a span from byte offsets and a one-based line/column pair.
    #[must_use]
    pub const fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// Cover this span and `other` with one range.
    #[must_use]
    pub fn cover(self, other: Self) -> Self {
        if self.start <= other.start {
            Self {
                end: self.end.max(other.end),
                ..self
            }
        } else {
            Self {
                end: self.end.max(other.end),
                ..other
            }
        }
    }
}

/// A syntax node paired with its source location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spanned<T> {
    /// Syntax node.
    pub node: T,
    /// Location of the node.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Pair a node with its source location.
    #[must_use]
    pub const fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }

    /// Transform the syntax node while preserving its span.
    #[must_use]
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned::new(map(self.node), self.span)
    }
}

/// Parsed Weave source document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Top-level declarations in source order.
    pub items: Vec<Spanned<Item>>,
}

impl Document {
    /// Iterate over all knots in source order.
    pub fn knots(&self) -> impl Iterator<Item = &Spanned<Knot>> {
        self.items.iter().filter_map(|item| match &item.node {
            Item::Knot(knot) => Some(knot),
            _ => None,
        })
    }

    /// Iterate over all grammar declarations in source order.
    pub fn grammars(&self) -> impl Iterator<Item = &Spanned<GrammarDecl>> {
        self.items.iter().filter_map(|item| match &item.node {
            Item::Grammar(grammar) => Some(grammar),
            _ => None,
        })
    }

    /// Iterate over all pattern declarations in source order.
    pub fn patterns(&self) -> impl Iterator<Item = &Spanned<PatternDecl>> {
        self.items.iter().filter_map(|item| match &item.node {
            Item::Pattern(pattern) => Some(pattern),
            _ => None,
        })
    }
}

/// Top-level source item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Item {
    /// Scoped generative grammar.
    Grammar(Spanned<GrammarDecl>),
    /// Meaning-bearing pattern definition.
    Pattern(Spanned<PatternDecl>),
    /// Story-scoped declaration executed at startup.
    Global(Spanned<Statement>),
    /// Named narrative block.
    Knot(Spanned<Knot>),
    /// Source comment without its `//` marker.
    Comment(String),
    /// Blank source line.
    Blank,
}

/// Generative grammar declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrammarDecl {
    /// Grammar name.
    pub name: String,
    /// Rules and trivia in source order.
    pub entries: Vec<Spanned<GrammarEntry>>,
}

/// Item within a grammar block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GrammarEntry {
    /// Named grammar rule.
    Rule(GrammarRule),
    /// Source comment without its marker.
    Comment(String),
    /// Blank source line.
    Blank,
}

/// One grammar rule with one or more alternatives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrammarRule {
    /// Rule name.
    pub name: String,
    /// Uniformly selected string alternatives.
    pub alternatives: Vec<String>,
}

/// Pattern-system declaration reserved for Phase 2 execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternDecl {
    /// Pattern-system name.
    pub name: String,
    /// Collections, spreads, and trivia in source order.
    pub entries: Vec<Spanned<PatternEntry>>,
}

/// Item within a pattern block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatternEntry {
    /// Named collection of semantic elements.
    Collection(PatternCollection),
    /// Named spread with ordered positions.
    Spread(SpreadDecl),
    /// Source comment without its marker.
    Comment(String),
    /// Blank source line.
    Blank,
}

/// Named collection of pattern elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternCollection {
    /// Collection name.
    pub name: String,
    /// Ordered elements.
    pub elements: Vec<PatternElement>,
}

/// Structured pattern element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternElement {
    /// Fields in author-defined order.
    pub fields: Vec<(String, Literal)>,
}

/// Named spread definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpreadDecl {
    /// Spread name.
    pub name: String,
    /// Ordered symbolic positions.
    pub positions: Vec<String>,
}

/// Named narrative block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Knot {
    /// Knot name.
    pub name: String,
    /// Source-ordered statements.
    pub body: Vec<Spanned<Statement>>,
}

/// Executable or formatting-preserving story statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    /// Narrative text before template lowering.
    Text(String),
    /// Variable/list/flag/state declaration.
    Declare(Declaration),
    /// Assignment to an existing story variable.
    Assign {
        /// Target name.
        name: String,
        /// New value.
        value: Spanned<Expr>,
    },
    /// Append or remove one list value.
    MutateList {
        /// Operation to perform.
        operation: ListOperation,
        /// Target list name.
        name: String,
        /// Value to append or remove.
        value: Spanned<Expr>,
    },
    /// One selectable branch.
    Choice(Choice),
    /// Ordered conditional branches.
    Conditional(Conditional),
    /// Replace execution with another knot.
    Divert(String),
    /// Call another knot and return on natural completion.
    Thread(String),
    /// End story execution.
    End,
    /// Source comment without its marker.
    Comment(String),
    /// Blank source line.
    Blank,
}

/// Variable declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Declaration {
    /// Declared kind.
    pub kind: VariableKind,
    /// Variable name.
    pub name: String,
    /// Initial value.
    pub value: Spanned<Expr>,
    /// Allowed values for state machines; empty for other kinds.
    pub allowed_states: Vec<String>,
}

/// Explicit declaration kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableKind {
    /// General variable.
    Variable,
    /// Ordered list.
    List,
    /// Boolean flag.
    Flag,
    /// Symbol-constrained state machine.
    State,
}

/// List mutation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListOperation {
    /// Append one value.
    Push,
    /// Remove the first equal value.
    Remove,
}

/// Choice statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    /// `true` for `*`, `false` for sticky `+` choices.
    pub once: bool,
    /// Author-visible label before template lowering.
    pub text: String,
    /// Optional eligibility condition.
    pub condition: Option<Spanned<Expr>>,
    /// Statements executed after selection.
    pub body: Vec<Spanned<Statement>>,
    /// Optional target executed after the body.
    pub divert: Option<String>,
}

/// Ordered conditional statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conditional {
    /// Conditional branches in evaluation order.
    pub branches: Vec<ConditionalBranch>,
    /// Optional final `else` body.
    pub fallback: Option<Vec<Spanned<Statement>>>,
}

/// One conditional branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionalBranch {
    /// Boolean condition.
    pub condition: Spanned<Expr>,
    /// Body executed when the condition is true.
    pub body: Vec<Spanned<Statement>>,
}

/// Source expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// Scalar literal.
    Literal(Literal),
    /// List literal.
    List(Vec<Spanned<Expr>>),
    /// Variable, symbol, or field path.
    Path(Vec<String>),
    /// Inline grammar reference.
    GrammarRef {
        /// Explicit grammar name; absent for same-grammar references.
        grammar: Option<String>,
        /// Rule name.
        rule: String,
    },
    /// Compiler-recognized path call.
    Call {
        /// Dotted function path.
        path: Vec<String>,
        /// Call arguments.
        arguments: Vec<Spanned<Expr>>,
    },
    /// Unary operation.
    Unary {
        /// Operator.
        operator: UnaryOperator,
        /// Operand.
        operand: Box<Spanned<Expr>>,
    },
    /// Binary operation.
    Binary {
        /// Left operand.
        left: Box<Spanned<Expr>>,
        /// Operator.
        operator: BinaryOperator,
        /// Right operand.
        right: Box<Spanned<Expr>>,
    },
}

/// Scalar source literal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    /// Null value.
    Null,
    /// Boolean value.
    Bool(bool),
    /// Finite number.
    Number(f64),
    /// UTF-8 string.
    String(String),
    /// Semantic atom.
    Symbol(String),
}

/// Unary expression operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    /// Boolean negation.
    Not,
    /// Numeric negation.
    Negate,
}

/// Binary expression operator in the source language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    /// Addition or string concatenation.
    Add,
    /// Numeric subtraction.
    Subtract,
    /// Numeric multiplication.
    Multiply,
    /// Numeric division.
    Divide,
    /// Numeric remainder.
    Remainder,
    /// Equality.
    Equal,
    /// Inequality.
    NotEqual,
    /// Less-than ordering.
    Less,
    /// Less-than-or-equal ordering.
    LessEqual,
    /// Greater-than ordering.
    Greater,
    /// Greater-than-or-equal ordering.
    GreaterEqual,
    /// Membership or substring test.
    In,
    /// Short-circuit Boolean conjunction.
    And,
    /// Short-circuit Boolean disjunction.
    Or,
}
