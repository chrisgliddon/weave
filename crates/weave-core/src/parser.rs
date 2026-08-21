//! Pest-backed parser for Weave source and expressions.

use std::collections::VecDeque;

use pest::Parser;
use pest::error::{InputLocation, LineColLocation};
use pest::iterators::Pair;
use pest_derive::Parser;

use crate::Diagnostic;
use crate::ast::{
    BinaryOperator, Choice, Conditional, ConditionalBranch, Declaration, Document, Expr,
    GrammarDecl, GrammarEntry, GrammarRule, Item, Knot, ListOperation, Literal, PatternCollection,
    PatternDecl, PatternElement, PatternEntry, Span, Spanned, SpreadDecl, Statement, UnaryOperator,
    VariableKind,
};

#[derive(Parser)]
#[grammar = "weave.pest"]
struct WeaveParser;

/// Parse a complete Weave document.
pub fn parse_document(source: &str) -> Result<Document, Vec<Diagnostic>> {
    let mut parsed =
        WeaveParser::parse(Rule::document, source).map_err(|error| vec![pest_diagnostic(error)])?;
    let Some(document_pair) = parsed.next() else {
        return Err(vec![internal_parser_error("parser returned no document")]);
    };

    let mut document = Document::default();
    let mut diagnostics = Vec::new();

    for pair in document_pair.into_inner() {
        let built = match pair.as_rule() {
            Rule::blank_line => Ok(Spanned::new(Item::Blank, span_for(&pair))),
            Rule::comment_line => Ok(Spanned::new(
                Item::Comment(comment_from(pair.clone())),
                span_for(&pair),
            )),
            Rule::grammar_decl => build_grammar(pair),
            Rule::pattern_decl => build_pattern(pair),
            Rule::global_decl => build_global(pair),
            Rule::knot => build_knot(pair),
            Rule::EOI => continue,
            _ => Err(vec![internal_parser_error(format!(
                "unexpected top-level parser rule {:?}",
                pair.as_rule()
            ))]),
        };

        match built {
            Ok(item) => document.items.push(item),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if diagnostics.is_empty() {
        Ok(document)
    } else {
        Err(diagnostics)
    }
}

/// Parse an expression independently for editor, compiler, and runtime tooling.
pub fn parse_expression(source: &str) -> Result<Spanned<Expr>, Diagnostic> {
    parse_expression_at(source, 0, 1, 1)
}

fn build_grammar(pair: Pair<'_, Rule>) -> Result<Spanned<Item>, Vec<Diagnostic>> {
    let outer_span = span_for(&pair);
    let mut name = None;
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier if name.is_none() => name = Some(inner.as_str().to_owned()),
            Rule::blank_line => entries.push(Spanned::new(GrammarEntry::Blank, span_for(&inner))),
            Rule::comment_line => {
                let span = span_for(&inner);
                entries.push(Spanned::new(
                    GrammarEntry::Comment(comment_from(inner)),
                    span,
                ));
            }
            Rule::grammar_rule => match build_grammar_rule(inner) {
                Ok(rule) => entries.push(rule),
                Err(error) => diagnostics.push(error),
            },
            _ => {}
        }
    }

    let Some(name) = name else {
        diagnostics.push(internal_parser_error("grammar has no name"));
        return Err(diagnostics);
    };

    if diagnostics.is_empty() {
        let grammar = Spanned::new(GrammarDecl { name, entries }, outer_span);
        Ok(Spanned::new(Item::Grammar(grammar), outer_span))
    } else {
        Err(diagnostics)
    }
}

fn build_grammar_rule(pair: Pair<'_, Rule>) -> Result<Spanned<GrammarEntry>, Diagnostic> {
    let span = span_for(&pair);
    let mut inner = pair.into_inner();
    let Some(name) = inner.next() else {
        return Err(internal_parser_error("grammar rule has no name"));
    };
    let Some(value) = inner.next() else {
        return Err(internal_parser_error("grammar rule has no value"));
    };

    let alternatives = match value.as_rule() {
        Rule::string_literal => vec![decode_string(value.as_str(), span)?],
        Rule::string_list => value
            .into_inner()
            .filter(|item| item.as_rule() == Rule::string_literal)
            .map(|item| decode_string(item.as_str(), span_for(&item)))
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(internal_parser_error(format!(
                "unexpected grammar value {:?}",
                value.as_rule()
            )));
        }
    };

    Ok(Spanned::new(
        GrammarEntry::Rule(GrammarRule {
            name: name.as_str().to_owned(),
            alternatives,
        }),
        span,
    ))
}

fn build_pattern(pair: Pair<'_, Rule>) -> Result<Spanned<Item>, Vec<Diagnostic>> {
    let outer_span = span_for(&pair);
    let mut name = None;
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier if name.is_none() => name = Some(inner.as_str().to_owned()),
            Rule::blank_line => entries.push(Spanned::new(PatternEntry::Blank, span_for(&inner))),
            Rule::comment_line => {
                let span = span_for(&inner);
                entries.push(Spanned::new(
                    PatternEntry::Comment(comment_from(inner)),
                    span,
                ));
            }
            Rule::pattern_collection => match build_pattern_collection(inner) {
                Ok(collection) => entries.push(collection),
                Err(error) => diagnostics.push(error),
            },
            Rule::spread_decl => match build_spread(inner) {
                Ok(spread) => entries.push(spread),
                Err(error) => diagnostics.push(error),
            },
            _ => {}
        }
    }

    let Some(name) = name else {
        diagnostics.push(internal_parser_error("pattern has no name"));
        return Err(diagnostics);
    };

    if diagnostics.is_empty() {
        let pattern = Spanned::new(PatternDecl { name, entries }, outer_span);
        Ok(Spanned::new(Item::Pattern(pattern), outer_span))
    } else {
        Err(diagnostics)
    }
}

fn build_pattern_collection(pair: Pair<'_, Rule>) -> Result<Spanned<PatternEntry>, Diagnostic> {
    let span = span_for(&pair);
    let mut inner = pair.into_inner();
    let Some(name) = inner.next() else {
        return Err(internal_parser_error("pattern collection has no name"));
    };
    let mut elements = Vec::new();
    for item in inner {
        if item.as_rule() == Rule::pattern_element_line {
            let Some(element) = item
                .into_inner()
                .find(|candidate| candidate.as_rule() == Rule::pattern_element)
            else {
                return Err(internal_parser_error("pattern element line has no element"));
            };
            elements.push(build_pattern_element(element)?);
        }
    }
    Ok(Spanned::new(
        PatternEntry::Collection(PatternCollection {
            name: name.as_str().to_owned(),
            elements,
        }),
        span,
    ))
}

fn build_pattern_element(pair: Pair<'_, Rule>) -> Result<PatternElement, Diagnostic> {
    let mut fields = Vec::new();
    for field in pair.into_inner() {
        if field.as_rule() != Rule::pattern_field {
            continue;
        }
        let field_span = span_for(&field);
        let mut parts = field.into_inner();
        let Some(name) = parts.next() else {
            return Err(internal_parser_error("pattern field has no name"));
        };
        let Some(value) = parts.next() else {
            return Err(internal_parser_error("pattern field has no value"));
        };
        fields.push((
            name.as_str().to_owned(),
            build_pattern_literal(value, field_span)?,
        ));
    }
    Ok(PatternElement { fields })
}

fn build_pattern_literal(pair: Pair<'_, Rule>, span: Span) -> Result<Literal, Diagnostic> {
    match pair.as_rule() {
        Rule::string_literal => decode_string(pair.as_str(), span).map(Literal::String),
        Rule::number_literal => parse_number(pair.as_str(), span).map(Literal::Number),
        Rule::boolean_literal => Ok(Literal::Bool(pair.as_str() == "true")),
        Rule::null_literal => Ok(Literal::Null),
        Rule::identifier => Ok(Literal::Symbol(pair.as_str().to_owned())),
        _ => Err(internal_parser_error(format!(
            "unexpected pattern literal {:?}",
            pair.as_rule()
        ))),
    }
}

fn build_spread(pair: Pair<'_, Rule>) -> Result<Spanned<PatternEntry>, Diagnostic> {
    let span = span_for(&pair);
    let mut inner = pair.into_inner();
    let Some(name) = inner.next() else {
        return Err(internal_parser_error("spread has no name"));
    };
    let Some(position_list) = inner.next() else {
        return Err(internal_parser_error("spread has no positions"));
    };
    let positions = position_list
        .into_inner()
        .filter(|item| item.as_rule() == Rule::identifier)
        .map(|item| item.as_str().to_owned())
        .collect();
    Ok(Spanned::new(
        PatternEntry::Spread(SpreadDecl {
            name: name.as_str().to_owned(),
            positions,
        }),
        span,
    ))
}

fn build_global(pair: Pair<'_, Rule>) -> Result<Spanned<Item>, Vec<Diagnostic>> {
    let span = span_for(&pair);
    let text = pair.as_str().trim_end_matches(['\r', '\n']);
    let line = BodyLine::new(text, span.start, span.line);
    match parse_simple_statement(&line) {
        Ok(Statement::Declare(declaration)) => {
            let statement = Spanned::new(Statement::Declare(declaration), span);
            Ok(Spanned::new(Item::Global(statement), span))
        }
        Ok(_) => Err(vec![internal_parser_error(
            "top-level parser accepted a non-declaration",
        )]),
        Err(error) => Err(vec![error]),
    }
}

fn build_knot(pair: Pair<'_, Rule>) -> Result<Spanned<Item>, Vec<Diagnostic>> {
    let outer_span = span_for(&pair);
    let mut inner = pair.into_inner();
    let Some(header) = inner.next() else {
        return Err(vec![internal_parser_error("knot has no header")]);
    };
    let Some(name) = header
        .into_inner()
        .find(|item| item.as_rule() == Rule::identifier)
        .map(|item| item.as_str().to_owned())
    else {
        return Err(vec![internal_parser_error("knot header has no name")]);
    };
    let Some(body_pair) = inner.next() else {
        return Err(vec![internal_parser_error("knot has no body pair")]);
    };

    let body_span = span_for(&body_pair);
    let mut parser = BodyParser::new(body_pair.as_str(), body_span.start, body_span.line);
    let body = parser.parse();
    if parser.diagnostics.is_empty() {
        let knot = Spanned::new(Knot { name, body }, outer_span);
        Ok(Spanned::new(Item::Knot(knot), outer_span))
    } else {
        Err(parser.diagnostics)
    }
}

#[derive(Debug, Clone)]
struct BodyLine {
    content: String,
    trimmed: String,
    indent: usize,
    leading_bytes: usize,
    start: usize,
    end: usize,
    line: usize,
}

impl BodyLine {
    fn new(content: &str, start: usize, line: usize) -> Self {
        let mut indent = 0;
        let mut leading_bytes = 0;
        for character in content.chars() {
            match character {
                ' ' => {
                    indent += 1;
                    leading_bytes += 1;
                }
                '\t' => {
                    indent += 4;
                    leading_bytes += 1;
                }
                _ => break,
            }
        }
        let trimmed = content[leading_bytes..].trim_end().to_owned();
        Self {
            content: content.to_owned(),
            trimmed,
            indent,
            leading_bytes,
            start,
            end: start + content.len(),
            line,
        }
    }

    fn span(&self) -> Span {
        Span::new(
            self.start + self.leading_bytes,
            self.end,
            self.line,
            self.indent + 1,
        )
    }

    fn expression_location(&self, expression: &str) -> (usize, usize, usize) {
        let relative = self.content.find(expression).unwrap_or(self.leading_bytes);
        (self.start + relative, self.line, relative + 1)
    }
}

struct BodyParser {
    lines: Vec<BodyLine>,
    index: usize,
    diagnostics: Vec<Diagnostic>,
}

impl BodyParser {
    fn new(source: &str, base_offset: usize, base_line: usize) -> Self {
        let mut lines = Vec::new();
        let mut offset = base_offset;
        let mut line_number = base_line;
        for segment in source.split_inclusive('\n') {
            let content = segment
                .strip_suffix('\n')
                .unwrap_or(segment)
                .strip_suffix('\r')
                .unwrap_or_else(|| segment.strip_suffix('\n').unwrap_or(segment));
            lines.push(BodyLine::new(content, offset, line_number));
            offset += segment.len();
            line_number += 1;
        }
        if !source.is_empty() && !source.ends_with('\n') && lines.is_empty() {
            lines.push(BodyLine::new(source, base_offset, base_line));
        }
        Self {
            lines,
            index: 0,
            diagnostics: Vec::new(),
        }
    }

    fn parse(&mut self) -> Vec<Spanned<Statement>> {
        self.parse_block(0)
    }

    fn parse_block(&mut self, expected_indent: usize) -> Vec<Spanned<Statement>> {
        let mut statements = Vec::new();
        while self.index < self.lines.len() {
            let line = self.lines[self.index].clone();

            if line.trimmed.is_empty() {
                statements.push(Spanned::new(Statement::Blank, line.span()));
                self.index += 1;
                continue;
            }
            if line.indent < expected_indent {
                break;
            }
            if line.indent == expected_indent
                && (line.trimmed == "}"
                    || (line.trimmed.starts_with('-') && line.trimmed.ends_with(':')))
            {
                break;
            }
            if line.indent > expected_indent {
                self.diagnostics.push(
                    Diagnostic::error("W1002", "unexpected indentation")
                        .with_span(line.span())
                        .with_help(format!(
                            "expected {expected_indent} spaces at this location"
                        )),
                );
            }

            if is_choice_line(&line.trimmed) {
                statements.push(self.parse_choice(line));
                continue;
            }
            if is_conditional_header(&line.trimmed) {
                statements.push(self.parse_conditional(line));
                continue;
            }

            match parse_simple_statement(&line) {
                Ok(statement) => statements.push(Spanned::new(statement, line.span())),
                Err(error) => self.diagnostics.push(error),
            }
            self.index += 1;
        }
        statements
    }

    fn parse_choice(&mut self, line: BodyLine) -> Spanned<Statement> {
        let span = line.span();
        let (once, text, condition, divert) = match parse_choice_header(&line) {
            Ok(choice) => choice,
            Err(error) => {
                self.diagnostics.push(error);
                self.index += 1;
                return Spanned::new(Statement::Blank, span);
            }
        };

        self.index += 1;
        let body = match self.next_nonblank_indent() {
            Some(indent) if indent > line.indent => self.parse_block(indent),
            _ => Vec::new(),
        };

        Spanned::new(
            Statement::Choice(Choice {
                once,
                text,
                condition,
                body,
                divert,
            }),
            span,
        )
    }

    fn parse_conditional(&mut self, line: BodyLine) -> Spanned<Statement> {
        let span = line.span();
        let first_source = line
            .trimmed
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix(':'))
            .map(str::trim);
        let Some(first_source) = first_source else {
            self.diagnostics
                .push(Diagnostic::error("W1003", "invalid conditional header").with_span(span));
            self.index += 1;
            return Spanned::new(Statement::Blank, span);
        };
        let first_condition = self.parse_line_expression(&line, first_source);
        self.index += 1;
        let first_body = self.parse_conditional_body(line.indent);

        let mut branches = Vec::new();
        if let Some(condition) = first_condition {
            branches.push(ConditionalBranch {
                condition,
                body: first_body,
            });
        }
        let mut fallback = None;

        while self.index < self.lines.len() {
            let marker = self.lines[self.index].clone();
            if marker.trimmed == "}" && marker.indent == line.indent {
                self.index += 1;
                return Spanned::new(
                    Statement::Conditional(Conditional { branches, fallback }),
                    span.cover(marker.span()),
                );
            }
            if marker.indent != line.indent
                || !marker.trimmed.starts_with('-')
                || !marker.trimmed.ends_with(':')
            {
                self.diagnostics.push(
                    Diagnostic::error("W1004", "expected a conditional branch or `}`")
                        .with_span(marker.span()),
                );
                self.index += 1;
                continue;
            }

            let branch_source = marker
                .trimmed
                .strip_prefix('-')
                .and_then(|value| value.strip_suffix(':'))
                .map(str::trim)
                .unwrap_or_default();
            self.index += 1;
            let body = self.parse_conditional_body(line.indent);
            if branch_source == "else" {
                if fallback.is_some() {
                    self.diagnostics.push(
                        Diagnostic::error("W1005", "a conditional can have only one else branch")
                            .with_span(marker.span()),
                    );
                }
                fallback = Some(body);
            } else if fallback.is_some() {
                self.diagnostics.push(
                    Diagnostic::error("W1006", "the else branch must be last")
                        .with_span(marker.span()),
                );
            } else if let Some(condition) = self.parse_line_expression(&marker, branch_source) {
                branches.push(ConditionalBranch { condition, body });
            }
        }

        self.diagnostics.push(
            Diagnostic::error("W1007", "unterminated conditional block")
                .with_span(span)
                .with_help("add a closing `}` at the same indentation as the opening branch"),
        );
        Spanned::new(
            Statement::Conditional(Conditional { branches, fallback }),
            span,
        )
    }

    fn parse_conditional_body(&mut self, parent_indent: usize) -> Vec<Spanned<Statement>> {
        match self.next_nonblank_indent() {
            Some(indent) if indent > parent_indent => self.parse_block(indent),
            _ => Vec::new(),
        }
    }

    fn next_nonblank_indent(&self) -> Option<usize> {
        self.lines[self.index..]
            .iter()
            .find(|line| !line.trimmed.is_empty())
            .map(|line| line.indent)
    }

    fn parse_line_expression(&mut self, line: &BodyLine, source: &str) -> Option<Spanned<Expr>> {
        let (offset, line_number, column) = line.expression_location(source);
        match parse_expression_at(source, offset, line_number, column) {
            Ok(expression) => Some(expression),
            Err(error) => {
                self.diagnostics.push(error);
                None
            }
        }
    }
}

fn is_choice_line(source: &str) -> bool {
    source
        .strip_prefix('*')
        .or_else(|| source.strip_prefix('+'))
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn is_conditional_header(source: &str) -> bool {
    source.starts_with('{') && source.ends_with(':')
}

type ParsedChoice = (bool, String, Option<Spanned<Expr>>, Option<String>);

fn parse_choice_header(line: &BodyLine) -> Result<ParsedChoice, Diagnostic> {
    let once = line.trimmed.starts_with('*');
    let rest = line.trimmed[1..].trim_start();
    if !rest.starts_with('[') {
        return Err(
            Diagnostic::error("W1010", "a choice label must start with `[").with_span(line.span()),
        );
    }
    let Some(close) = find_unescaped(rest, ']', 1) else {
        return Err(Diagnostic::error("W1011", "unterminated choice label").with_span(line.span()));
    };
    let text = rest[1..close].replace("\\]", "]").replace("\\\\", "\\");
    let mut tail = rest[close + 1..].trim();
    let mut condition = None;

    if let Some(after_open) = tail.strip_prefix("{if") {
        let Some(close_condition) = find_unescaped(after_open, '}', 0) else {
            return Err(
                Diagnostic::error("W1012", "unterminated choice condition").with_span(line.span())
            );
        };
        let source = after_open[..close_condition].trim();
        let (offset, line_number, column) = line.expression_location(source);
        condition = Some(parse_expression_at(source, offset, line_number, column)?);
        tail = after_open[close_condition + 1..].trim();
    }

    let divert = if tail.is_empty() {
        None
    } else if let Some(target) = tail.strip_prefix("->") {
        let target = target.trim();
        if target.is_empty() {
            return Err(
                Diagnostic::error("W1013", "choice divert target cannot be empty")
                    .with_span(line.span()),
            );
        }
        Some(target.to_owned())
    } else {
        return Err(
            Diagnostic::error("W1014", "unexpected text after choice label")
                .with_span(line.span())
                .with_help("use `{if condition}` and/or `-> target` after the label"),
        );
    };

    Ok((once, text, condition, divert))
}

fn find_unescaped(source: &str, needle: char, start: usize) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in source.char_indices().filter(|(index, _)| *index >= start) {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
        } else if character == needle {
            return Some(index);
        }
    }
    None
}

fn parse_simple_statement(line: &BodyLine) -> Result<Statement, Diagnostic> {
    let source = line.trimmed.as_str();
    if let Some(comment) = source.strip_prefix("//") {
        return Ok(Statement::Comment(comment.trim_start().to_owned()));
    }
    if source.is_empty() {
        return Ok(Statement::Blank);
    }
    if let Some(target) = source.strip_prefix("->") {
        let target = target.trim();
        if target.is_empty() {
            return Err(
                Diagnostic::error("W1020", "divert target cannot be empty").with_span(line.span())
            );
        }
        return if target == "END" {
            Ok(Statement::End)
        } else {
            Ok(Statement::Divert(target.to_owned()))
        };
    }
    if let Some(target) = source
        .strip_prefix("<-")
        .or_else(|| source.strip_prefix("THREAD "))
    {
        let target = target.trim();
        if target.is_empty() {
            return Err(
                Diagnostic::error("W1021", "thread target cannot be empty").with_span(line.span())
            );
        }
        return Ok(Statement::Thread(target.to_owned()));
    }

    for (keyword, kind) in [
        ("VAR", VariableKind::Variable),
        ("LIST", VariableKind::List),
        ("FLAG", VariableKind::Flag),
        ("STATE", VariableKind::State),
    ] {
        if let Some(rest) = source.strip_prefix(keyword).and_then(strip_required_space) {
            return parse_declaration(line, kind, rest).map(Statement::Declare);
        }
    }

    if let Some(rest) = source.strip_prefix("SET").and_then(strip_required_space) {
        let (name, value_source) = split_assignment(rest, line.span())?;
        let value = parse_line_fragment(line, value_source)?;
        return Ok(Statement::Assign {
            name: name.to_owned(),
            value,
        });
    }
    for (keyword, operation) in [
        ("PUSH", ListOperation::Push),
        ("REMOVE", ListOperation::Remove),
    ] {
        if let Some(rest) = source.strip_prefix(keyword).and_then(strip_required_space) {
            let Some((name, value_source)) = rest.split_once(',') else {
                return Err(Diagnostic::error(
                    "W1022",
                    format!("{keyword} requires `name, value`"),
                )
                .with_span(line.span()));
            };
            let name = name.trim();
            validate_identifier(name, line.span())?;
            let value = parse_line_fragment(line, value_source.trim())?;
            return Ok(Statement::MutateList {
                operation,
                name: name.to_owned(),
                value,
            });
        }
    }

    Ok(Statement::Text(source.to_owned()))
}

fn strip_required_space(source: &str) -> Option<&str> {
    source
        .strip_prefix(char::is_whitespace)
        .map(str::trim_start)
}

fn parse_declaration(
    line: &BodyLine,
    kind: VariableKind,
    source: &str,
) -> Result<Declaration, Diagnostic> {
    let (name, remainder) = split_assignment(source, line.span())?;
    validate_identifier(name, line.span())?;

    if kind == VariableKind::State {
        let Some(open) = remainder.rfind('[') else {
            return Err(
                Diagnostic::error("W1023", "STATE requires an allowed-state list")
                    .with_span(line.span()),
            );
        };
        let (value_source, states_source) = remainder.split_at(open);
        let states_source = states_source.trim();
        if !states_source.ends_with(']') {
            return Err(
                Diagnostic::error("W1024", "unterminated allowed-state list")
                    .with_span(line.span()),
            );
        }
        let allowed_states = states_source[1..states_source.len() - 1]
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                validate_identifier(value, line.span())?;
                Ok(value.to_owned())
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        let value = parse_line_fragment(line, value_source.trim())?;
        return Ok(Declaration {
            kind,
            name: name.to_owned(),
            value,
            allowed_states,
        });
    }

    let value = parse_line_fragment(line, remainder)?;
    Ok(Declaration {
        kind,
        name: name.to_owned(),
        value,
        allowed_states: Vec::new(),
    })
}

fn split_assignment(source: &str, span: Span) -> Result<(&str, &str), Diagnostic> {
    let Some((name, value)) = source.split_once('=') else {
        return Err(
            Diagnostic::error("W1025", "assignment requires `name = expression`").with_span(span),
        );
    };
    let name = name.trim();
    let value = value.trim();
    if value.is_empty() {
        return Err(Diagnostic::error("W1026", "expression cannot be empty").with_span(span));
    }
    Ok((name, value))
}

fn validate_identifier(source: &str, span: Span) -> Result<(), Diagnostic> {
    let mut characters = source.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    let valid_rest =
        characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
    if valid_start && valid_rest {
        Ok(())
    } else {
        Err(
            Diagnostic::error("W1027", format!("`{source}` is not a valid identifier"))
                .with_span(span),
        )
    }
}

fn parse_line_fragment(line: &BodyLine, source: &str) -> Result<Spanned<Expr>, Diagnostic> {
    let (offset, line_number, column) = line.expression_location(source);
    parse_expression_at(source, offset, line_number, column)
}

pub(crate) fn parse_expression_at(
    source: &str,
    base_offset: usize,
    base_line: usize,
    base_column: usize,
) -> Result<Spanned<Expr>, Diagnostic> {
    let mut parsed = WeaveParser::parse(Rule::expression_input, source).map_err(|error| {
        let mut diagnostic = pest_diagnostic(error);
        if let Some(span) = diagnostic.span.as_mut() {
            span.start += base_offset;
            span.end += base_offset;
            span.line += base_line.saturating_sub(1);
            if span.line == base_line {
                span.column += base_column.saturating_sub(1);
            }
        }
        diagnostic
    })?;
    let Some(pair) = parsed.next() else {
        return Err(internal_parser_error("expression parser returned no pair"));
    };
    build_expression(pair, base_offset, base_line, base_column)
}

fn build_expression(
    pair: Pair<'_, Rule>,
    base_offset: usize,
    base_line: usize,
    base_column: usize,
) -> Result<Spanned<Expr>, Diagnostic> {
    let span = span_for_with_base(&pair, base_offset, base_line, base_column);
    match pair.as_rule() {
        Rule::expression_input | Rule::expression => {
            let Some(inner) = pair.into_inner().next() else {
                return Err(internal_parser_error("empty expression"));
            };
            build_expression(inner, base_offset, base_line, base_column)
        }
        Rule::logical_or
        | Rule::logical_and
        | Rule::equality
        | Rule::comparison
        | Rule::additive
        | Rule::multiplicative => {
            build_binary_level(pair, span, base_offset, base_line, base_column)
        }
        Rule::unary => {
            let mut parts: VecDeque<_> = pair.into_inner().collect();
            let Some(operand_pair) = parts.pop_back() else {
                return Err(internal_parser_error("unary expression has no operand"));
            };
            let mut operand = build_expression(operand_pair, base_offset, base_line, base_column)?;
            while let Some(operator) = parts.pop_back() {
                let operator_span =
                    span_for_with_base(&operator, base_offset, base_line, base_column);
                let operator = match operator.as_str().trim() {
                    "-" => UnaryOperator::Negate,
                    "!" | "not" => UnaryOperator::Not,
                    value => {
                        return Err(internal_parser_error(format!(
                            "unknown unary operator {value}"
                        )));
                    }
                };
                let expression_span = operator_span.cover(operand.span);
                operand = Spanned::new(
                    Expr::Unary {
                        operator,
                        operand: Box::new(operand),
                    },
                    expression_span,
                );
            }
            Ok(Spanned::new(operand.node, span))
        }
        Rule::list_literal => {
            let values = pair
                .into_inner()
                .filter(|item| item.as_rule() == Rule::expression)
                .map(|item| build_expression(item, base_offset, base_line, base_column))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Spanned::new(Expr::List(values), span))
        }
        Rule::grammar_reference => {
            let identifiers = pair
                .into_inner()
                .filter(|item| item.as_rule() == Rule::identifier)
                .map(|item| item.as_str().to_owned())
                .collect::<Vec<_>>();
            match identifiers.as_slice() {
                [rule] => Ok(Spanned::new(
                    Expr::GrammarRef {
                        grammar: None,
                        rule: rule.clone(),
                    },
                    span,
                )),
                [grammar, rule] => Ok(Spanned::new(
                    Expr::GrammarRef {
                        grammar: Some(grammar.clone()),
                        rule: rule.clone(),
                    },
                    span,
                )),
                _ => Err(internal_parser_error("invalid grammar reference")),
            }
        }
        Rule::call_or_path => {
            let mut inner = pair.into_inner();
            let Some(path_pair) = inner.next() else {
                return Err(internal_parser_error("path expression has no path"));
            };
            let path = path_pair
                .into_inner()
                .filter(|item| item.as_rule() == Rule::identifier)
                .map(|item| item.as_str().to_owned())
                .collect::<Vec<_>>();
            if let Some(arguments) = inner.next() {
                let arguments = arguments
                    .into_inner()
                    .filter(|item| item.as_rule() == Rule::expression)
                    .map(|item| build_expression(item, base_offset, base_line, base_column))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Spanned::new(Expr::Call { path, arguments }, span))
            } else {
                Ok(Spanned::new(Expr::Path(path), span))
            }
        }
        Rule::string_literal => Ok(Spanned::new(
            Expr::Literal(Literal::String(decode_string(pair.as_str(), span)?)),
            span,
        )),
        Rule::number_literal => Ok(Spanned::new(
            Expr::Literal(Literal::Number(parse_number(pair.as_str(), span)?)),
            span,
        )),
        Rule::boolean_literal => Ok(Spanned::new(
            Expr::Literal(Literal::Bool(pair.as_str() == "true")),
            span,
        )),
        Rule::null_literal => Ok(Spanned::new(Expr::Literal(Literal::Null), span)),
        rule => Err(internal_parser_error(format!(
            "unexpected expression rule {rule:?}"
        ))),
    }
}

fn build_binary_level(
    pair: Pair<'_, Rule>,
    span: Span,
    base_offset: usize,
    base_line: usize,
    base_column: usize,
) -> Result<Spanned<Expr>, Diagnostic> {
    let mut inner = pair.into_inner();
    let Some(first) = inner.next() else {
        return Err(internal_parser_error(
            "binary expression has no left operand",
        ));
    };
    let mut expression = build_expression(first, base_offset, base_line, base_column)?;
    while let Some(operator_pair) = inner.next() {
        let Some(right_pair) = inner.next() else {
            return Err(internal_parser_error(
                "binary expression has no right operand",
            ));
        };
        let operator = binary_operator(operator_pair.as_str().trim())?;
        let right = build_expression(right_pair, base_offset, base_line, base_column)?;
        let expression_span = expression.span.cover(right.span);
        expression = Spanned::new(
            Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            },
            expression_span,
        );
    }
    Ok(Spanned::new(expression.node, span))
}

fn binary_operator(source: &str) -> Result<BinaryOperator, Diagnostic> {
    match source {
        "+" => Ok(BinaryOperator::Add),
        "-" => Ok(BinaryOperator::Subtract),
        "*" => Ok(BinaryOperator::Multiply),
        "/" => Ok(BinaryOperator::Divide),
        "%" => Ok(BinaryOperator::Remainder),
        "==" => Ok(BinaryOperator::Equal),
        "!=" => Ok(BinaryOperator::NotEqual),
        "<" => Ok(BinaryOperator::Less),
        "<=" => Ok(BinaryOperator::LessEqual),
        ">" => Ok(BinaryOperator::Greater),
        ">=" => Ok(BinaryOperator::GreaterEqual),
        "in" => Ok(BinaryOperator::In),
        "and" | "&&" => Ok(BinaryOperator::And),
        "or" | "||" => Ok(BinaryOperator::Or),
        value => Err(internal_parser_error(format!(
            "unknown binary operator {value}"
        ))),
    }
}

fn parse_number(source: &str, span: Span) -> Result<f64, Diagnostic> {
    match source.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(
            Diagnostic::error("W1030", "number must be finite and representable").with_span(span),
        ),
    }
}

fn decode_string(source: &str, span: Span) -> Result<String, Diagnostic> {
    let Some(contents) = source
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(internal_parser_error("string literal is missing quotes"));
    };
    let mut output = String::new();
    let mut characters = contents.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let Some(escaped) = characters.next() else {
            return Err(Diagnostic::error("W1031", "unterminated string escape").with_span(span));
        };
        output.push(match escaped {
            '\\' => '\\',
            '"' => '"',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            _ => {
                return Err(Diagnostic::error(
                    "W1032",
                    format!("unknown string escape `\\{escaped}`"),
                )
                .with_span(span));
            }
        });
    }
    Ok(output)
}

fn comment_from(pair: Pair<'_, Rule>) -> String {
    pair.into_inner()
        .find(|item| item.as_rule() == Rule::comment_text)
        .map(|item| item.as_str().trim_start().to_owned())
        .unwrap_or_default()
}

fn span_for(pair: &Pair<'_, Rule>) -> Span {
    let pest_span = pair.as_span();
    let (line, column) = pest_span.start_pos().line_col();
    Span::new(pest_span.start(), pest_span.end(), line, column)
}

fn span_for_with_base(
    pair: &Pair<'_, Rule>,
    base_offset: usize,
    base_line: usize,
    base_column: usize,
) -> Span {
    let relative = span_for(pair);
    Span::new(
        base_offset + relative.start,
        base_offset + relative.end,
        base_line + relative.line - 1,
        if relative.line == 1 {
            base_column + relative.column - 1
        } else {
            relative.column
        },
    )
}

fn pest_diagnostic(error: pest::error::Error<Rule>) -> Diagnostic {
    let (line, column) = match error.line_col {
        LineColLocation::Pos(position) => position,
        LineColLocation::Span(start, _) => start,
    };
    let (start, end) = match error.location {
        InputLocation::Pos(position) => (position, position.saturating_add(1)),
        InputLocation::Span(range) => range,
    };
    Diagnostic::error("W1000", error.to_string()).with_span(Span::new(start, end, line, column))
}

fn internal_parser_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(
        "W1099",
        format!("internal parser invariant: {}", message.into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expression_precedence() {
        let expression = parse_expression("1 + 2 * 3 == 7 and true");
        assert!(expression.is_ok(), "{expression:?}");
    }

    #[test]
    fn parses_nested_choices_and_conditions() {
        let source = r#"grammar names {
    first: ["Mira", "Aldric"]
}

VAR visits = 0

=== start ===
* [Enter] {if visits == 0}
    SET visits = visits + 1
    * [Continue] -> ending
+ [Leave] -> END

=== ending ===
{visits > 0:
    Hello, #names.first#.
- else:
    No one is here.
}
-> END
"#;
        let document = parse_document(source);
        assert!(document.is_ok(), "{document:?}");
        let document = document.ok().unwrap_or_default();
        assert_eq!(document.knots().count(), 2);
    }

    #[test]
    fn rejects_malformed_input_without_panicking() {
        let result = parse_document("grammar broken { value: [\"x\"\n");
        assert!(result.is_err());
    }
}
