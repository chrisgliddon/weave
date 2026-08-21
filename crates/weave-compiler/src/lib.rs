//! Compiler from `.weave` source to the versioned runtime IR.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use weave_core::ast::{
    BinaryOperator, Declaration, Document, Expr, GrammarEntry, Item, ListOperation, Literal,
    PatternDrawMethod, PatternEntry, Spanned, Statement, UnaryOperator, VariableKind,
};
use weave_core::ir::{
    BinaryOperatorIr, BuiltinPatternIr, ChoiceIr, ConditionalBranchIr, ConditionalIr,
    DeclarationIr, Expression, GrammarIr, Instruction, InstructionKind, KnotIr, ListOperationIr,
    PatternCollectionIr, PatternDrawMethodIr, PatternElementIr, PatternSystemIr, SpreadIr, StoryIr,
    Template, TemplatePartIr, UnaryOperatorIr, ValueLiteral, VariableKindIr,
};
use weave_core::{Diagnostic, Severity, TemplatePart, has_errors, parse_document, parse_template};

/// Compiler version corresponding to the current IR.
pub const COMPILER_IR_VERSION: u32 = weave_core::ir::IR_VERSION;

/// Source metadata supplied by an embedding application.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompileOptions {
    /// Optional path or display name embedded in the IR.
    pub source_name: Option<String>,
}

/// Successful compilation and its non-fatal diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledStory {
    /// Immutable runtime representation.
    pub story: StoryIr,
    /// Compiler warnings. Successful compilation never contains errors here.
    pub diagnostics: Vec<Diagnostic>,
}

/// One or more source diagnostics that prevented compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    /// Complete diagnostic list, in source order where possible.
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                formatter.write_str("\n")?;
            }
            diagnostic.fmt(formatter)?;
        }
        Ok(())
    }
}

impl Error for CompileError {}

/// A serialization failure after successful source compilation.
#[derive(Debug, thiserror::Error)]
pub enum SerializeError {
    /// RON serialization failed.
    #[error("could not serialize RON: {0}")]
    Ron(#[from] ron::Error),
    /// JSON serialization failed.
    #[error("could not serialize JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Parse, statically check, and lower source into runtime IR.
pub fn compile(source: &str, options: &CompileOptions) -> Result<CompiledStory, CompileError> {
    let document = parse_document(source).map_err(|diagnostics| CompileError { diagnostics })?;
    let checked = weave_core::type_check(&document);
    if has_errors(&checked.diagnostics) {
        return Err(CompileError {
            diagnostics: checked.diagnostics,
        });
    }

    let mut lowerer = Lowerer::new(options.source_name.clone());
    let story = lowerer.lower_document(&document);
    if has_errors(&lowerer.diagnostics) {
        return Err(CompileError {
            diagnostics: lowerer.diagnostics,
        });
    }

    let mut diagnostics = checked
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .collect::<Vec<_>>();
    diagnostics.append(&mut lowerer.diagnostics);
    Ok(CompiledStory { story, diagnostics })
}

/// Serialize an IR story as canonical, human-readable RON.
pub fn to_ron(story: &StoryIr) -> Result<String, SerializeError> {
    let config = ron::ser::PrettyConfig::new()
        .new_line("\n")
        .indentor("    ")
        .struct_names(true)
        .enumerate_arrays(false)
        .compact_arrays(false)
        .compact_maps(false);
    let mut output = ron::ser::to_string_pretty(story, config)?;
    output.push('\n');
    Ok(output)
}

/// Serialize an IR story as stable, pretty JSON.
pub fn to_json(story: &StoryIr) -> Result<String, SerializeError> {
    let mut output = serde_json::to_string_pretty(story)?;
    output.push('\n');
    Ok(output)
}

/// Compile source directly to canonical RON.
pub fn compile_to_ron(
    source: &str,
    options: &CompileOptions,
) -> Result<(String, Vec<Diagnostic>), CompileOrSerializeError> {
    let compiled = compile(source, options)?;
    let output = to_ron(&compiled.story)?;
    Ok((output, compiled.diagnostics))
}

/// Failure from the convenience compile-and-serialize API.
#[derive(Debug, thiserror::Error)]
pub enum CompileOrSerializeError {
    /// Source compilation failed.
    #[error("{0}")]
    Compile(#[from] CompileError),
    /// Output serialization failed.
    #[error("{0}")]
    Serialize(#[from] SerializeError),
}

struct Lowerer {
    source_name: Option<String>,
    diagnostics: Vec<Diagnostic>,
    current_grammar: Option<String>,
    current_knot: String,
}

impl Lowerer {
    fn new(source_name: Option<String>) -> Self {
        Self {
            source_name,
            diagnostics: Vec::new(),
            current_grammar: None,
            current_knot: String::new(),
        }
    }

    fn lower_document(&mut self, document: &Document) -> StoryIr {
        let entry = document
            .knots()
            .next()
            .map(|knot| knot.node.name.clone())
            .unwrap_or_else(|| "start".to_owned());
        let mut story = StoryIr::new(entry);
        story.source_name = self.source_name.clone();

        for item in &document.items {
            match &item.node {
                Item::Grammar(grammar) => {
                    self.current_grammar = Some(grammar.node.name.clone());
                    let mut rules = BTreeMap::new();
                    for entry in &grammar.node.entries {
                        if let GrammarEntry::Rule(rule) = &entry.node {
                            let alternatives = rule
                                .alternatives
                                .iter()
                                .map(|alternative| self.lower_template(alternative, entry.span))
                                .collect();
                            rules.insert(rule.name.clone(), alternatives);
                        }
                    }
                    story
                        .grammars
                        .insert(grammar.node.name.clone(), GrammarIr { rules });
                    self.current_grammar = None;
                }
                Item::Pattern(pattern) => {
                    let mut builtin = None;
                    let mut collections = BTreeMap::new();
                    let mut spreads = BTreeMap::new();
                    let mut draw_method = None;
                    let mut allow_duplicates = false;
                    let mut reversals = false;
                    for entry in &pattern.node.entries {
                        match &entry.node {
                            PatternEntry::Builtin(name) => {
                                builtin = match name.as_str() {
                                    "tarot" => Some(BuiltinPatternIr::Tarot),
                                    "i_ching" => Some(BuiltinPatternIr::IChing),
                                    "elder_futhark" => Some(BuiltinPatternIr::ElderFuthark),
                                    _ => None,
                                };
                            }
                            PatternEntry::DrawMethod(method) => {
                                draw_method = Some(match method {
                                    PatternDrawMethod::Uniform => PatternDrawMethodIr::Uniform,
                                    PatternDrawMethod::WeightedBy(field) => {
                                        PatternDrawMethodIr::WeightedBy {
                                            field: field.clone(),
                                        }
                                    }
                                    PatternDrawMethod::ThreeCoin => PatternDrawMethodIr::ThreeCoin,
                                    PatternDrawMethod::YarrowStalks => {
                                        PatternDrawMethodIr::YarrowStalks
                                    }
                                });
                            }
                            PatternEntry::Reversals(value) => reversals = *value,
                            PatternEntry::Duplicates(value) => allow_duplicates = *value,
                            PatternEntry::Collection(collection) => {
                                let elements = collection
                                    .elements
                                    .iter()
                                    .map(|element| PatternElementIr {
                                        fields: element
                                            .fields
                                            .iter()
                                            .map(|(name, value)| {
                                                (name.clone(), lower_literal(value))
                                            })
                                            .collect(),
                                    })
                                    .collect();
                                collections.insert(
                                    collection.name.clone(),
                                    PatternCollectionIr { elements },
                                );
                            }
                            PatternEntry::Spread(spread) => {
                                spreads.insert(
                                    spread.name.clone(),
                                    SpreadIr {
                                        positions: spread.positions.clone(),
                                    },
                                );
                            }
                            PatternEntry::Comment(_) | PatternEntry::Blank => {}
                        }
                    }
                    story.patterns.insert(
                        pattern.node.name.clone(),
                        PatternSystemIr {
                            builtin,
                            collections,
                            spreads,
                            draw_method: draw_method.unwrap_or_else(|| {
                                if builtin == Some(BuiltinPatternIr::IChing) {
                                    PatternDrawMethodIr::ThreeCoin
                                } else {
                                    PatternDrawMethodIr::Uniform
                                }
                            }),
                            allow_duplicates,
                            reversals,
                        },
                    );
                }
                Item::Global(statement) => {
                    if let Some(instruction) = self.lower_statement(statement, "global") {
                        story.globals.push(instruction);
                    }
                }
                Item::Knot(knot) => {
                    self.current_knot.clone_from(&knot.node.name);
                    let content = self.lower_block(&knot.node.body, "root");
                    story
                        .knots
                        .insert(knot.node.name.clone(), KnotIr { content });
                }
                Item::Comment(_) | Item::Blank => {}
            }
        }
        story
    }

    fn lower_block(&mut self, statements: &[Spanned<Statement>], path: &str) -> Vec<Instruction> {
        statements
            .iter()
            .enumerate()
            .filter_map(|(index, statement)| {
                self.lower_statement(statement, &format!("{path}.{index}"))
            })
            .collect()
    }

    fn lower_statement(
        &mut self,
        statement: &Spanned<Statement>,
        path: &str,
    ) -> Option<Instruction> {
        let kind = match &statement.node {
            Statement::Text(text) => {
                InstructionKind::Text(self.lower_template(text, statement.span))
            }
            Statement::Declare(declaration) => {
                InstructionKind::Declare(lower_declaration(declaration))
            }
            Statement::Assign { name, value } => InstructionKind::Assign {
                name: name.clone(),
                value: lower_expression(&value.node),
            },
            Statement::MutateList {
                operation,
                name,
                value,
            } => InstructionKind::MutateList {
                operation: match operation {
                    ListOperation::Push => ListOperationIr::Push,
                    ListOperation::Remove => ListOperationIr::Remove,
                },
                name: name.clone(),
                value: lower_expression(&value.node),
            },
            Statement::Choice(choice) => {
                let id = format!("{}:{path}", self.current_knot);
                let body = self.lower_block(&choice.body, &format!("{path}.body"));
                InstructionKind::Choice(ChoiceIr {
                    id,
                    once: choice.once,
                    text: self.lower_template(&choice.text, statement.span),
                    condition: choice
                        .condition
                        .as_ref()
                        .map(|condition| lower_expression(&condition.node)),
                    body,
                    divert: choice.divert.clone(),
                })
            }
            Statement::Conditional(conditional) => {
                let branches = conditional
                    .branches
                    .iter()
                    .enumerate()
                    .map(|(index, branch)| ConditionalBranchIr {
                        condition: lower_expression(&branch.condition.node),
                        body: self.lower_block(&branch.body, &format!("{path}.branch.{index}")),
                    })
                    .collect();
                let fallback = conditional
                    .fallback
                    .as_ref()
                    .map(|fallback| self.lower_block(fallback, &format!("{path}.fallback")));
                InstructionKind::Conditional(ConditionalIr { branches, fallback })
            }
            Statement::Divert(target) => InstructionKind::Divert(target.clone()),
            Statement::Thread(target) => InstructionKind::Thread(target.clone()),
            Statement::End => InstructionKind::End,
            Statement::Comment(_) | Statement::Blank => return None,
        };
        Some(Instruction {
            span: statement.span,
            kind,
        })
    }

    fn lower_template(&mut self, source: &str, span: weave_core::Span) -> Template {
        match parse_template(source, span) {
            Ok(parts) => Template {
                parts: parts
                    .into_iter()
                    .map(|part| match part.node {
                        TemplatePart::Text(text) => TemplatePartIr::Text(text),
                        TemplatePart::GrammarRef { grammar, rule } => TemplatePartIr::GrammarRef {
                            grammar: grammar.or_else(|| self.current_grammar.clone()),
                            rule,
                        },
                        TemplatePart::Expression(expression) => {
                            TemplatePartIr::Expression(lower_expression(&expression.node))
                        }
                    })
                    .collect(),
            },
            Err(mut diagnostics) => {
                self.diagnostics.append(&mut diagnostics);
                Template::default()
            }
        }
    }
}

fn lower_declaration(declaration: &Declaration) -> DeclarationIr {
    DeclarationIr {
        kind: match declaration.kind {
            VariableKind::Variable => VariableKindIr::Variable,
            VariableKind::List => VariableKindIr::List,
            VariableKind::Flag => VariableKindIr::Flag,
            VariableKind::State => VariableKindIr::State,
        },
        name: declaration.name.clone(),
        value: lower_expression(&declaration.value.node),
        allowed_states: declaration.allowed_states.clone(),
    }
}

fn lower_expression(expression: &Expr) -> Expression {
    match expression {
        Expr::Literal(value) => Expression::Literal(lower_literal(value)),
        Expr::List(values) => Expression::List(
            values
                .iter()
                .map(|value| lower_expression(&value.node))
                .collect(),
        ),
        Expr::Path(path) => Expression::Path(path.clone()),
        Expr::GrammarRef { grammar, rule } => Expression::GrammarRef {
            grammar: grammar.clone(),
            rule: rule.clone(),
        },
        Expr::Call { path, arguments } => Expression::Call {
            path: path.clone(),
            arguments: arguments
                .iter()
                .map(|argument| lower_expression(&argument.node))
                .collect(),
        },
        Expr::Unary { operator, operand } => Expression::Unary {
            operator: match operator {
                UnaryOperator::Not => UnaryOperatorIr::Not,
                UnaryOperator::Negate => UnaryOperatorIr::Negate,
            },
            operand: Box::new(lower_expression(&operand.node)),
        },
        Expr::Binary {
            left,
            operator,
            right,
        } => Expression::Binary {
            left: Box::new(lower_expression(&left.node)),
            operator: match operator {
                BinaryOperator::Add => BinaryOperatorIr::Add,
                BinaryOperator::Subtract => BinaryOperatorIr::Subtract,
                BinaryOperator::Multiply => BinaryOperatorIr::Multiply,
                BinaryOperator::Divide => BinaryOperatorIr::Divide,
                BinaryOperator::Remainder => BinaryOperatorIr::Remainder,
                BinaryOperator::Equal => BinaryOperatorIr::Equal,
                BinaryOperator::NotEqual => BinaryOperatorIr::NotEqual,
                BinaryOperator::Less => BinaryOperatorIr::Less,
                BinaryOperator::LessEqual => BinaryOperatorIr::LessEqual,
                BinaryOperator::Greater => BinaryOperatorIr::Greater,
                BinaryOperator::GreaterEqual => BinaryOperatorIr::GreaterEqual,
                BinaryOperator::In => BinaryOperatorIr::In,
                BinaryOperator::And => BinaryOperatorIr::And,
                BinaryOperator::Or => BinaryOperatorIr::Or,
            },
            right: Box::new(lower_expression(&right.node)),
        },
    }
}

fn lower_literal(literal: &Literal) -> ValueLiteral {
    match literal {
        Literal::Null => ValueLiteral::Null,
        Literal::Bool(value) => ValueLiteral::Bool(*value),
        Literal::Number(value) => ValueLiteral::Number(*value),
        Literal::String(value) => ValueLiteral::String(value.clone()),
        Literal::Symbol(value) => ValueLiteral::Symbol(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_source_without_partial_output() {
        let error = compile("=== start ===\n-> missing\n", &CompileOptions::default())
            .expect_err("unknown target should fail");
        assert!(
            error
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.0 == "W2021")
        );
    }

    #[test]
    fn output_is_stable_and_round_trips() {
        let source =
            "grammar words {\nvalue: [\"one\", \"two\"]\n}\n=== start ===\n#words.value#\n-> END\n";
        let first = compile(source, &CompileOptions::default()).expect("source should compile");
        let second = compile(source, &CompileOptions::default()).expect("source should compile");
        let first_ron = to_ron(&first.story).expect("RON should serialize");
        assert_eq!(
            first_ron,
            to_ron(&second.story).expect("RON should serialize")
        );
        let decoded: StoryIr = ron::from_str(&first_ron).expect("RON should deserialize");
        assert_eq!(decoded, first.story);
    }

    #[test]
    fn lowers_builtin_and_authored_patterns_into_executable_ir() {
        let source = r#"
pattern cards {
    builtin: tarot
    reversals: true
}
pattern changes {
    builtin: i_ching
}
pattern omens {
    signs: [(name: "Crow", meaning: warning, severity: 2)]
    draw: weighted_by_severity
    duplicates: true
}
VAR card = cards.draw()
VAR change = changes.draw()
VAR omen = omens.draw()
=== start ===
{card.name} {change.name} {omen.name}
"#;
        let compiled = compile(source, &CompileOptions::default()).expect("source compiles");
        assert_eq!(
            compiled.story.patterns["cards"].builtin,
            Some(BuiltinPatternIr::Tarot)
        );
        assert_eq!(
            compiled.story.patterns["changes"].draw_method,
            PatternDrawMethodIr::ThreeCoin
        );
        assert_eq!(
            compiled.story.patterns["omens"].draw_method,
            PatternDrawMethodIr::WeightedBy {
                field: "severity".to_owned()
            }
        );
        assert!(compiled.story.patterns["omens"].allow_duplicates);

        let ron = to_ron(&compiled.story).expect("patterns serialize");
        let decoded: StoryIr = ron::from_str(&ron).expect("patterns deserialize");
        assert_eq!(decoded, compiled.story);
    }
}
