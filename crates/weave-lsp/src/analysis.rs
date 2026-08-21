use std::collections::BTreeMap;

use weave_core::ast::{
    Conditional, Expr, GrammarEntry, Item, PatternEntry, Span, Spanned, Statement,
};
use weave_core::{Diagnostic, TemplatePart, Type, parse_document, parse_template, type_check};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Knot,
    Grammar,
    GrammarRule,
    Pattern,
    Collection,
    Spread,
    Variable,
}

impl SymbolKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Knot => "knot",
            Self::Grammar => "grammar",
            Self::GrammarRule => "grammar rule",
            Self::Pattern => "pattern system",
            Self::Collection => "pattern collection",
            Self::Spread => "pattern spread",
            Self::Variable => "variable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub key: String,
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub selection: Span,
    pub detail: String,
    pub container: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceTarget {
    Exact(String),
    Value(String),
    GrammarRule {
        grammar: Option<String>,
        rule: String,
    },
}

impl ReferenceTarget {
    #[must_use]
    pub fn matches(&self, symbol: &Symbol) -> bool {
        match self {
            Self::Exact(key) => key == &symbol.key,
            Self::Value(name) => {
                name == &symbol.name
                    && matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Pattern)
            }
            Self::GrammarRule { grammar, rule } => {
                symbol.kind == SymbolKind::GrammarRule
                    && &symbol.name == rule
                    && grammar
                        .as_ref()
                        .is_some_and(|grammar| symbol.container.as_ref() == Some(grammar))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub target: ReferenceTarget,
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub selection: Span,
    pub detail: String,
    pub children: Vec<Self>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub outlines: Vec<Outline>,
    pub variable_types: BTreeMap<String, Type>,
}

impl Analysis {
    #[must_use]
    pub fn target_at(&self, byte: usize) -> Option<ReferenceTarget> {
        self.symbols
            .iter()
            .find(|symbol| contains(symbol.selection, byte))
            .map(|symbol| ReferenceTarget::Exact(symbol.key.clone()))
            .or_else(|| {
                self.references
                    .iter()
                    .find(|reference| contains(reference.span, byte))
                    .map(|reference| reference.target.clone())
            })
    }

    #[must_use]
    pub fn selection_at(&self, byte: usize) -> Option<Span> {
        self.symbols
            .iter()
            .find(|symbol| contains(symbol.selection, byte))
            .map(|symbol| symbol.selection)
            .or_else(|| {
                self.references
                    .iter()
                    .find(|reference| contains(reference.span, byte))
                    .map(|reference| reference.span)
            })
    }
}

#[must_use]
pub fn analyze(source: &str) -> Analysis {
    let document = match parse_document(source) {
        Ok(document) => document,
        Err(diagnostics) => {
            return Analysis {
                diagnostics,
                ..Analysis::default()
            };
        }
    };

    let checked = type_check(&document);
    let mut analysis = Analysis {
        diagnostics: checked.diagnostics,
        variable_types: checked.variables,
        ..Analysis::default()
    };

    for item in &document.items {
        match &item.node {
            Item::Knot(knot) => {
                let selection = identifier_span(source, item.span, &knot.node.name);
                analysis.symbols.push(Symbol {
                    key: format!("knot:{}", knot.node.name),
                    name: knot.node.name.clone(),
                    kind: SymbolKind::Knot,
                    span: item.span,
                    selection,
                    detail: "narrative knot".to_owned(),
                    container: None,
                });
                analysis.outlines.push(Outline {
                    name: knot.node.name.clone(),
                    kind: SymbolKind::Knot,
                    span: item.span,
                    selection,
                    detail: "narrative knot".to_owned(),
                    children: Vec::new(),
                });
                collect_statements(source, &knot.node.body, &mut analysis);
            }
            Item::Grammar(grammar) => {
                let selection = identifier_span(source, item.span, &grammar.node.name);
                let mut children = Vec::new();
                analysis.symbols.push(Symbol {
                    key: format!("grammar:{}", grammar.node.name),
                    name: grammar.node.name.clone(),
                    kind: SymbolKind::Grammar,
                    span: item.span,
                    selection,
                    detail: "generative grammar".to_owned(),
                    container: None,
                });
                for entry in &grammar.node.entries {
                    if let GrammarEntry::Rule(rule) = &entry.node {
                        let rule_selection = identifier_span(source, entry.span, &rule.name);
                        analysis.symbols.push(Symbol {
                            key: format!("grammar_rule:{}.{}", grammar.node.name, rule.name),
                            name: rule.name.clone(),
                            kind: SymbolKind::GrammarRule,
                            span: entry.span,
                            selection: rule_selection,
                            detail: format!("rule in grammar {}", grammar.node.name),
                            container: Some(grammar.node.name.clone()),
                        });
                        children.push(Outline {
                            name: rule.name.clone(),
                            kind: SymbolKind::GrammarRule,
                            span: entry.span,
                            selection: rule_selection,
                            detail: format!("{} alternatives", rule.alternatives.len()),
                            children: Vec::new(),
                        });
                    }
                }
                analysis.outlines.push(Outline {
                    name: grammar.node.name.clone(),
                    kind: SymbolKind::Grammar,
                    span: item.span,
                    selection,
                    detail: "generative grammar".to_owned(),
                    children,
                });
            }
            Item::Pattern(pattern) => {
                let selection = identifier_span(source, item.span, &pattern.node.name);
                let mut children = Vec::new();
                analysis.symbols.push(Symbol {
                    key: format!("pattern:{}", pattern.node.name),
                    name: pattern.node.name.clone(),
                    kind: SymbolKind::Pattern,
                    span: item.span,
                    selection,
                    detail: "pattern system".to_owned(),
                    container: None,
                });
                for entry in &pattern.node.entries {
                    let (name, kind, key_prefix, detail) = match &entry.node {
                        PatternEntry::Collection(collection) => (
                            &collection.name,
                            SymbolKind::Collection,
                            "collection",
                            "pattern collection",
                        ),
                        PatternEntry::Spread(spread) => {
                            (&spread.name, SymbolKind::Spread, "spread", "pattern spread")
                        }
                        _ => continue,
                    };
                    let child_selection = identifier_span(source, entry.span, name);
                    analysis.symbols.push(Symbol {
                        key: format!("{key_prefix}:{}.{}", pattern.node.name, name),
                        name: name.clone(),
                        kind,
                        span: entry.span,
                        selection: child_selection,
                        detail: detail.to_owned(),
                        container: Some(pattern.node.name.clone()),
                    });
                    children.push(Outline {
                        name: name.clone(),
                        kind,
                        span: entry.span,
                        selection: child_selection,
                        detail: detail.to_owned(),
                        children: Vec::new(),
                    });
                }
                analysis.outlines.push(Outline {
                    name: pattern.node.name.clone(),
                    kind: SymbolKind::Pattern,
                    span: item.span,
                    selection,
                    detail: "pattern system".to_owned(),
                    children,
                });
            }
            Item::Global(statement) => collect_statement(source, statement, &mut analysis),
            Item::Comment(_) | Item::Blank => {}
        }
    }

    analysis
        .references
        .extend(scan_grammar_references(source, &analysis.symbols));
    analysis.references.sort_by_key(|reference| {
        (
            reference.span.start,
            reference.span.end,
            reference.name.clone(),
        )
    });
    analysis
        .references
        .dedup_by(|left, right| left.span == right.span && left.target == right.target);
    analysis
}

fn collect_statements(source: &str, statements: &[Spanned<Statement>], analysis: &mut Analysis) {
    for statement in statements {
        collect_statement(source, statement, analysis);
    }
}

fn collect_statement(source: &str, statement: &Spanned<Statement>, analysis: &mut Analysis) {
    match &statement.node {
        Statement::Text(text) => collect_template(source, text, statement.span, analysis),
        Statement::Declare(declaration) => {
            let selection = identifier_span(source, statement.span, &declaration.name);
            let value_type = analysis
                .variable_types
                .get(&declaration.name)
                .map_or_else(|| "Any".to_owned(), ToString::to_string);
            analysis.symbols.push(Symbol {
                key: format!("variable:{}", declaration.name),
                name: declaration.name.clone(),
                kind: SymbolKind::Variable,
                span: statement.span,
                selection,
                detail: format!("{}: {value_type}", declaration_kind(declaration.kind)),
                container: None,
            });
            collect_expr(source, &declaration.value, analysis);
        }
        Statement::Assign { name, value } | Statement::MutateList { name, value, .. } => {
            push_reference(
                analysis,
                ReferenceTarget::Exact(format!("variable:{name}")),
                name,
                identifier_span(source, statement.span, name),
            );
            collect_expr(source, value, analysis);
        }
        Statement::Choice(choice) => {
            collect_template(source, &choice.text, statement.span, analysis);
            if let Some(condition) = &choice.condition {
                collect_expr(source, condition, analysis);
            }
            collect_statements(source, &choice.body, analysis);
            if let Some(target) = &choice.divert {
                push_reference(
                    analysis,
                    ReferenceTarget::Exact(format!("knot:{target}")),
                    target,
                    last_identifier_span(source, statement.span, target),
                );
            }
        }
        Statement::Conditional(conditional) => collect_conditional(source, conditional, analysis),
        Statement::Divert(target) | Statement::Thread(target) => push_reference(
            analysis,
            ReferenceTarget::Exact(format!("knot:{target}")),
            target,
            last_identifier_span(source, statement.span, target),
        ),
        Statement::End | Statement::Comment(_) | Statement::Blank => {}
    }
}

fn collect_conditional(source: &str, conditional: &Conditional, analysis: &mut Analysis) {
    for branch in &conditional.branches {
        collect_expr(source, &branch.condition, analysis);
        collect_statements(source, &branch.body, analysis);
    }
    if let Some(fallback) = &conditional.fallback {
        collect_statements(source, fallback, analysis);
    }
}

fn collect_template(source: &str, template: &str, span: Span, analysis: &mut Analysis) {
    if let Ok(parts) = parse_template(template, span) {
        for part in parts {
            if let TemplatePart::Expression(expression) = part.node {
                collect_expr(source, &expression, analysis);
            }
        }
    }
}

fn collect_expr(source: &str, expression: &Spanned<Expr>, analysis: &mut Analysis) {
    match &expression.node {
        Expr::Literal(_) => {}
        Expr::List(values) => {
            for value in values {
                collect_expr(source, value, analysis);
            }
        }
        Expr::Path(path) => {
            if let Some(root) = path.first() {
                let target = if analysis.variable_types.contains_key(root) {
                    ReferenceTarget::Exact(format!("variable:{root}"))
                } else {
                    ReferenceTarget::Value(root.clone())
                };
                push_reference(
                    analysis,
                    target,
                    root,
                    identifier_span(source, expression.span, root),
                );
            }
        }
        Expr::GrammarRef { .. } => {}
        Expr::Call { path, arguments } => {
            if let Some(root) = path.first() {
                push_reference(
                    analysis,
                    ReferenceTarget::Exact(format!("pattern:{root}")),
                    root,
                    identifier_span(source, expression.span, root),
                );
            }
            if path.len() >= 4 && path.get(1).is_some_and(|part| part == "spread") {
                let system = &path[0];
                let spread = &path[2];
                push_reference(
                    analysis,
                    ReferenceTarget::Exact(format!("spread:{system}.{spread}")),
                    spread,
                    identifier_span_after(source, expression.span, spread, path[0].len()),
                );
            }
            for argument in arguments {
                collect_expr(source, argument, analysis);
            }
        }
        Expr::Unary { operand, .. } => collect_expr(source, operand, analysis),
        Expr::Binary { left, right, .. } => {
            collect_expr(source, left, analysis);
            collect_expr(source, right, analysis);
        }
    }
}

fn scan_grammar_references(source: &str, symbols: &[Symbol]) -> Vec<Reference> {
    let bytes = source.as_bytes();
    let mut references = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'#' || is_escaped(bytes, index) || in_line_comment(source, index) {
            index += 1;
            continue;
        }
        let content_start = index + 1;
        let Some(relative_end) = source[content_start..].find('#') else {
            break;
        };
        let content_end = content_start + relative_end;
        let raw = &source[content_start..content_end];
        let leading = raw.len() - raw.trim_start().len();
        let content = raw.trim();
        let (explicit_grammar, rule) = content
            .split_once('.')
            .map_or((None, content), |(grammar, rule)| (Some(grammar), rule));
        if valid_identifier(rule) && explicit_grammar.is_none_or(valid_identifier) {
            let rule_offset = content.rfind(rule).unwrap_or(0);
            let content_offset = content_start + leading;
            let start = content_offset + rule_offset;
            let containing_grammar = symbols.iter().find(|symbol| {
                symbol.kind == SymbolKind::Grammar
                    && symbol.span.start <= index
                    && index < symbol.span.end
            });
            let grammar = explicit_grammar
                .map(str::to_owned)
                .or_else(|| containing_grammar.map(|symbol| symbol.name.clone()));
            if let Some(explicit_grammar) = explicit_grammar {
                references.push(Reference {
                    target: ReferenceTarget::Exact(format!("grammar:{explicit_grammar}")),
                    name: explicit_grammar.to_owned(),
                    span: span_from_offsets(
                        source,
                        content_offset,
                        content_offset + explicit_grammar.len(),
                    ),
                });
            }
            references.push(Reference {
                target: ReferenceTarget::GrammarRule {
                    grammar,
                    rule: rule.to_owned(),
                },
                name: rule.to_owned(),
                span: span_from_offsets(source, start, start + rule.len()),
            });
        }
        index = content_end + 1;
    }
    references
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let preceding = bytes[..index]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    preceding % 2 == 1
}

fn in_line_comment(source: &str, index: usize) -> bool {
    let line_start = source[..index].rfind('\n').map_or(0, |offset| offset + 1);
    source[line_start..index]
        .find("//")
        .is_some_and(|comment| !is_escaped(source.as_bytes(), line_start + comment))
}

fn declaration_kind(kind: weave_core::ast::VariableKind) -> &'static str {
    match kind {
        weave_core::ast::VariableKind::Variable => "variable",
        weave_core::ast::VariableKind::List => "list",
        weave_core::ast::VariableKind::Flag => "flag",
        weave_core::ast::VariableKind::State => "state",
    }
}

fn push_reference(analysis: &mut Analysis, target: ReferenceTarget, name: &str, span: Span) {
    analysis.references.push(Reference {
        target,
        name: name.to_owned(),
        span,
    });
}

fn identifier_span(source: &str, span: Span, name: &str) -> Span {
    identifier_span_after(source, span, name, 0)
}

fn identifier_span_after(source: &str, span: Span, name: &str, skip: usize) -> Span {
    let start = span.start.min(source.len());
    let end = span.end.min(source.len()).max(start);
    let search_start = (start + skip).min(end);
    let haystack = &source[search_start..end];
    haystack
        .match_indices(name)
        .find(|(offset, _)| {
            let absolute = search_start + offset;
            identifier_boundary(source, absolute, absolute + name.len())
        })
        .map_or(span, |(offset, _)| {
            let absolute = search_start + offset;
            span_from_offsets(source, absolute, absolute + name.len())
        })
}

fn last_identifier_span(source: &str, span: Span, name: &str) -> Span {
    let start = span.start.min(source.len());
    let end = span.end.min(source.len()).max(start);
    source[start..end]
        .match_indices(name)
        .filter(|(offset, _)| {
            let absolute = start + offset;
            identifier_boundary(source, absolute, absolute + name.len())
        })
        .last()
        .map_or(span, |(offset, _)| {
            let absolute = start + offset;
            span_from_offsets(source, absolute, absolute + name.len())
        })
}

fn identifier_boundary(source: &str, start: usize, end: usize) -> bool {
    let before = source[..start].chars().next_back();
    let after = source[end..].chars().next();
    before.is_none_or(|character| !identifier_character(character))
        && after.is_none_or(|character| !identifier_character(character))
}

fn identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(identifier_character)
}

fn span_from_offsets(source: &str, start: usize, end: usize) -> Span {
    let start = start.min(source.len());
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
    let column = source[line_start..start].chars().count() + 1;
    Span::new(start, end.min(source.len()).max(start), line, column)
}

fn contains(span: Span, byte: usize) -> bool {
    span.start <= byte && byte <= span.end
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"grammar omen {
    opening: ["The #closing#"]
    closing: ["door"]
}

pattern tarot {
    major: [(name: "Fool", meaning: beginning)]
    spread single { positions: [past] }
}

VAR courage = 2

=== start ===
Hello {courage}. #omen.opening#
SET courage = courage + 1
VAR draw = tarot.spread.single.draw()
-> ending

=== ending ===
-> END
"#;

    #[test]
    fn indexes_language_symbols_and_references() {
        let analysis = analyze(SOURCE);
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.key == "knot:start"),
            "{:?}",
            analysis.diagnostics
        );
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.key == "grammar_rule:omen.opening")
        );
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.key == "spread:tarot.single")
        );
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.key == "variable:courage")
        );
        assert!(analysis.references.iter().any(|reference| {
            reference.target == ReferenceTarget::Exact("knot:ending".to_owned())
        }));
        assert!(analysis.references.iter().any(|reference| {
            reference.target == ReferenceTarget::Exact("spread:tarot.single".to_owned())
        }));
        assert!(analysis.references.iter().any(|reference| matches!(
            &reference.target,
            ReferenceTarget::GrammarRule { grammar: Some(grammar), rule }
                if grammar == "omen" && rule == "opening"
        )));
    }

    #[test]
    fn returns_targets_at_definition_and_use_sites() {
        let analysis = analyze(SOURCE);
        let declaration = SOURCE.find("courage =").expect("declaration");
        assert_eq!(
            analysis.target_at(declaration),
            Some(ReferenceTarget::Exact("variable:courage".to_owned()))
        );
        let use_site = SOURCE.find("{courage}").expect("use") + 1;
        assert_eq!(
            analysis.target_at(use_site),
            Some(ReferenceTarget::Exact("variable:courage".to_owned()))
        );

        let grammar = SOURCE.find("omen.opening").expect("grammar use");
        assert_eq!(
            analysis.target_at(grammar),
            Some(ReferenceTarget::Exact("grammar:omen".to_owned()))
        );
        assert_eq!(
            analysis.target_at(grammar + "omen.".len()),
            Some(ReferenceTarget::GrammarRule {
                grammar: Some("omen".to_owned()),
                rule: "opening".to_owned(),
            })
        );
    }

    #[test]
    fn preserves_unknown_template_diagnostics() {
        let source = "=== start ===\nHello {missing.value}.\n-> END\n";
        let analysis = analyze(source);
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.0 == "W2028"),
            "{:?}",
            analysis.diagnostics
        );
    }

    #[test]
    fn ignores_grammar_markers_in_comments() {
        let analysis = analyze("// #ghost.rule#\n=== start ===\n-> END\n");
        assert!(analysis.references.is_empty());
    }
}
