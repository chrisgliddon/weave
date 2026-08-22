//! Canonical, semantics-preserving formatting for `.weave` source.

use std::fmt::Write;

use weave_core::Diagnostic;
use weave_core::ast::{
    BinaryOperator, Declaration, Document, Expr, GrammarEntry, Item, ListOperation, Literal,
    ModuleEntry, PatternDrawMethod, PatternEntry, Spanned, Statement, UnaryOperator, VariableKind,
};

/// Current canonical formatting contract.
pub const FORMAT_VERSION: u32 = 1;
const INDENT: &str = "    ";

/// Parse and format one complete source file.
///
/// Invalid source returns diagnostics and never yields replacement text, allowing callers to
/// avoid destructive writes.
pub fn format_source(source: &str) -> Result<String, Vec<Diagnostic>> {
    let document = weave_core::parse_document(source)?;
    Ok(format_document(&document))
}

/// Format an already parsed source document.
#[must_use]
pub fn format_document(document: &Document) -> String {
    let mut formatter = Formatter::default();
    formatter.document(document);
    formatter.finish()
}

#[derive(Default)]
struct Formatter {
    output: String,
}

impl Formatter {
    fn finish(mut self) -> String {
        while self.output.ends_with("\n\n\n") {
            self.output.pop();
        }
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.output
    }

    fn document(&mut self, document: &Document) {
        for item in &document.items {
            match &item.node {
                Item::Module(module) => {
                    self.line(0, &format!("module {} {{", module.node.alias));
                    let mut seen_content = false;
                    for entry in &module.node.entries {
                        match &entry.node {
                            ModuleEntry::Id(value) => {
                                seen_content = true;
                                self.line(1, &format!("id: {}", quote(value)));
                            }
                            ModuleEntry::Version(value) => {
                                seen_content = true;
                                self.line(1, &format!("version: {}", quote(value)));
                            }
                            ModuleEntry::Pack(value) => {
                                seen_content = true;
                                self.line(1, &format!("pack: {}", quote(value)));
                            }
                            ModuleEntry::Override { path, value } => {
                                seen_content = true;
                                self.line(
                                    1,
                                    &format!(
                                        "override {}: {}",
                                        path.join("."),
                                        format_expr(&value.node)
                                    ),
                                );
                            }
                            ModuleEntry::Comment(comment) => {
                                seen_content = true;
                                self.comment(1, comment);
                            }
                            ModuleEntry::Blank if seen_content => self.blank(),
                            ModuleEntry::Blank => {}
                        }
                    }
                    self.line(0, "}");
                }
                Item::Grammar(grammar) => {
                    self.line(0, &format!("grammar {} {{", grammar.node.name));
                    let mut seen_content = false;
                    for entry in &grammar.node.entries {
                        match &entry.node {
                            GrammarEntry::Rule(rule) => {
                                seen_content = true;
                                let value = if rule.alternatives.len() == 1 {
                                    quote(&rule.alternatives[0])
                                } else {
                                    format!(
                                        "[{}]",
                                        rule.alternatives
                                            .iter()
                                            .map(|value| quote(value))
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    )
                                };
                                self.line(1, &format!("{}: {value}", rule.name));
                            }
                            GrammarEntry::Comment(comment) => {
                                seen_content = true;
                                self.comment(1, comment);
                            }
                            GrammarEntry::Blank if seen_content => self.blank(),
                            GrammarEntry::Blank => {}
                        }
                    }
                    self.line(0, "}");
                }
                Item::Pattern(pattern) => {
                    self.line(0, &format!("pattern {} {{", pattern.node.name));
                    let mut seen_content = false;
                    for entry in &pattern.node.entries {
                        match &entry.node {
                            PatternEntry::Builtin(name) => {
                                seen_content = true;
                                self.line(1, &format!("builtin: {name}"));
                            }
                            PatternEntry::DrawMethod(method) => {
                                seen_content = true;
                                let method = match method {
                                    PatternDrawMethod::Uniform => "uniform".to_owned(),
                                    PatternDrawMethod::WeightedBy(field) => {
                                        format!("weighted_by_{field}")
                                    }
                                    PatternDrawMethod::ThreeCoin => "three_coin".to_owned(),
                                    PatternDrawMethod::YarrowStalks => "yarrow_stalks".to_owned(),
                                };
                                self.line(1, &format!("draw: {method}"));
                            }
                            PatternEntry::Reversals(value) => {
                                seen_content = true;
                                self.line(1, &format!("reversals: {value}"));
                            }
                            PatternEntry::Duplicates(value) => {
                                seen_content = true;
                                self.line(1, &format!("duplicates: {value}"));
                            }
                            PatternEntry::Collection(collection) => {
                                seen_content = true;
                                self.line(1, &format!("{}: [", collection.name));
                                for element in &collection.elements {
                                    let fields = element
                                        .fields
                                        .iter()
                                        .map(|(name, value)| {
                                            format!("{name}: {}", format_literal(value))
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    self.line(2, &format!("({fields}),"));
                                }
                                self.line(1, "]");
                            }
                            PatternEntry::Spread(spread) => {
                                seen_content = true;
                                self.line(
                                    1,
                                    &format!(
                                        "spread {} {{ positions: [{}] }}",
                                        spread.name,
                                        spread.positions.join(", ")
                                    ),
                                );
                            }
                            PatternEntry::Comment(comment) => {
                                seen_content = true;
                                self.comment(1, comment);
                            }
                            PatternEntry::Blank if seen_content => self.blank(),
                            PatternEntry::Blank => {}
                        }
                    }
                    self.line(0, "}");
                }
                Item::Global(statement) => self.statement(statement, 0),
                Item::Knot(knot) => {
                    self.line(0, &format!("=== {} ===", knot.node.name));
                    self.block(&knot.node.body, 0);
                }
                Item::Comment(comment) => self.comment(0, comment),
                Item::Blank => self.blank(),
            }
        }
    }

    fn block(&mut self, statements: &[Spanned<Statement>], depth: usize) {
        for statement in statements {
            self.statement(statement, depth);
        }
    }

    fn statement(&mut self, statement: &Spanned<Statement>, depth: usize) {
        match &statement.node {
            Statement::Text(text) => self.line(depth, text),
            Statement::Declare(declaration) => {
                self.line(depth, &format_declaration(declaration));
            }
            Statement::Assign { name, value } => {
                self.line(depth, &format!("SET {name} = {}", format_expr(&value.node)));
            }
            Statement::MutateList {
                operation,
                name,
                value,
            } => {
                let keyword = match operation {
                    ListOperation::Push => "PUSH",
                    ListOperation::Remove => "REMOVE",
                };
                self.line(
                    depth,
                    &format!("{keyword} {name}, {}", format_expr(&value.node)),
                );
            }
            Statement::Choice(choice) => {
                let marker = if choice.once { '*' } else { '+' };
                let mut header = format!("{marker} [{}]", escape_choice(&choice.text));
                if let Some(condition) = &choice.condition {
                    let _ = write!(header, " {{if {}}}", format_expr(&condition.node));
                }
                if let Some(divert) = &choice.divert {
                    let _ = write!(header, " -> {divert}");
                }
                self.line(depth, &header);
                self.block(&choice.body, depth + 1);
            }
            Statement::Conditional(conditional) => {
                if let Some(first) = conditional.branches.first() {
                    self.line(depth, &format!("{{{}:", format_expr(&first.condition.node)));
                    self.block(&first.body, depth + 1);
                    for branch in &conditional.branches[1..] {
                        self.line(
                            depth,
                            &format!("- {}:", format_expr(&branch.condition.node)),
                        );
                        self.block(&branch.body, depth + 1);
                    }
                    if let Some(fallback) = &conditional.fallback {
                        self.line(depth, "- else:");
                        self.block(fallback, depth + 1);
                    }
                    self.line(depth, "}");
                }
            }
            Statement::Divert(target) => self.line(depth, &format!("-> {target}")),
            Statement::Thread(target) => self.line(depth, &format!("<- {target}")),
            Statement::End => self.line(depth, "-> END"),
            Statement::Comment(comment) => self.comment(depth, comment),
            Statement::Blank => self.blank(),
        }
    }

    fn comment(&mut self, depth: usize, comment: &str) {
        if comment.is_empty() {
            self.line(depth, "//");
        } else {
            self.line(depth, &format!("// {comment}"));
        }
    }

    fn line(&mut self, depth: usize, value: &str) {
        for _ in 0..depth {
            self.output.push_str(INDENT);
        }
        self.output.push_str(value.trim_end());
        self.output.push('\n');
    }

    fn blank(&mut self) {
        if !self.output.ends_with("\n\n") && !self.output.is_empty() {
            self.output.push('\n');
        }
    }
}

fn format_declaration(declaration: &Declaration) -> String {
    let keyword = match declaration.kind {
        VariableKind::Variable => "VAR",
        VariableKind::List => "LIST",
        VariableKind::Flag => "FLAG",
        VariableKind::State => "STATE",
    };
    let mut output = format!(
        "{keyword} {} = {}",
        declaration.name,
        format_expr(&declaration.value.node)
    );
    if declaration.kind == VariableKind::State {
        let _ = write!(output, " [{}]", declaration.allowed_states.join(", "));
    }
    output
}

fn format_expr(expression: &Expr) -> String {
    format_expr_at(expression, 0)
}

fn format_expr_at(expression: &Expr, parent_precedence: u8) -> String {
    let precedence = expression_precedence(expression);
    let mut output = match expression {
        Expr::Literal(literal) => format_literal(literal),
        Expr::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|value| format_expr(&value.node))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Path(path) => path.join("."),
        Expr::GrammarRef { grammar, rule } => match grammar {
            Some(grammar) => format!("#{grammar}.{rule}#"),
            None => format!("#{rule}#"),
        },
        Expr::Call { path, arguments } => format!(
            "{}({})",
            path.join("."),
            arguments
                .iter()
                .map(|argument| format_expr(&argument.node))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Unary { operator, operand } => {
            let operator = match operator {
                UnaryOperator::Not => "not ",
                UnaryOperator::Negate => "-",
            };
            format!("{operator}{}", format_expr_at(&operand.node, precedence))
        }
        Expr::Binary {
            left,
            operator,
            right,
        } => {
            let operator_text = binary_operator(*operator);
            format!(
                "{} {operator_text} {}",
                format_expr_at(&left.node, precedence),
                format_expr_at(&right.node, precedence + 1)
            )
        }
    };
    if precedence < parent_precedence {
        output = format!("({output})");
    }
    output
}

fn expression_precedence(expression: &Expr) -> u8 {
    match expression {
        Expr::Binary { operator, .. } => match operator {
            BinaryOperator::Or => 1,
            BinaryOperator::And => 2,
            BinaryOperator::Equal | BinaryOperator::NotEqual => 3,
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::In => 4,
            BinaryOperator::Add | BinaryOperator::Subtract => 5,
            BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Remainder => 6,
        },
        Expr::Unary { .. } => 7,
        _ => 8,
    }
}

fn binary_operator(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::In => "in",
        BinaryOperator::And => "and",
        BinaryOperator::Or => "or",
    }
}

fn format_literal(literal: &Literal) -> String {
    match literal {
        Literal::Null => "null".to_owned(),
        Literal::Bool(value) => value.to_string(),
        Literal::Number(value) if value.fract() == 0.0 => format!("{value:.0}"),
        Literal::Number(value) => value.to_string(),
        Literal::String(value) => quote(value),
        Literal::Symbol(value) => value.clone(),
    }
}

fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
            .replace('"', "\\\"")
    )
}

fn escape_choice(value: &str) -> String {
    value.replace('\\', "\\\\").replace(']', "\\]")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSY: &str = r#"// file
module constellation{
id:"org.weave.synthetic_constellation"
version:"=1.0.0"
pack:"glasswing_sky@=1.0.0"
}
grammar words{
value:["one","two"]
}
pattern cards{
draw:uniform
reversals:false
duplicates:false
deck:[
(name:"One",meaning:hope),
]
spread single{positions:[card]}
}
VAR score=1+2*3
LIST bag=[key,"map"]
FLAG ready=false
STATE phase=dormant[dormant,active]
=== start ===
// body
SET score=score+1
PUSH bag,"coin"
REMOVE bag,key
* [Go]{if score>=2}->next
+ [Stay]
	<- aside
{ready:
	yes
- score>0:
	maybe
- else:
	no
}
=== aside ===
aside
=== next ===
->END
"#;

    const GOLDEN: &str = r#"// file
module constellation {
    id: "org.weave.synthetic_constellation"
    version: "=1.0.0"
    pack: "glasswing_sky@=1.0.0"
}
grammar words {
    value: ["one", "two"]
}
pattern cards {
    draw: uniform
    reversals: false
    duplicates: false
    deck: [
        (name: "One", meaning: hope),
    ]
    spread single { positions: [card] }
}
VAR score = 1 + 2 * 3
LIST bag = [key, "map"]
FLAG ready = false
STATE phase = dormant [dormant, active]
=== start ===
// body
SET score = score + 1
PUSH bag, "coin"
REMOVE bag, key
* [Go] {if score >= 2} -> next
+ [Stay]
    <- aside
{ready:
    yes
- score > 0:
    maybe
- else:
    no
}
=== aside ===
aside
=== next ===
-> END
"#;

    #[test]
    fn formatting_is_idempotent_and_covers_every_construct() {
        let once = format_source(MESSY).expect("fixture should format");
        let twice = format_source(&once).expect("formatted fixture should parse");
        assert_eq!(once, GOLDEN);
        assert_eq!(once, twice);
        assert!(once.contains("    <- aside"));
        assert!(once.contains("VAR score = 1 + 2 * 3"));
        assert!(once.contains("spread single { positions: [card] }"));
        assert!(once.contains("module constellation"));
        assert!(once.contains("// body"));
    }

    #[test]
    fn invalid_input_never_returns_replacement_text() {
        let error = format_source("=== start ===\n{broken:\n")
            .expect_err("invalid input should not format");
        assert!(!error.is_empty());
    }

    #[test]
    fn preserves_parentheses_required_by_associativity() {
        let formatted = format_source("=== start ===\nVAR value = 10 - (3 - 1)\n-> END\n")
            .expect("source should format");
        assert!(formatted.contains("10 - (3 - 1)"));
    }
}
