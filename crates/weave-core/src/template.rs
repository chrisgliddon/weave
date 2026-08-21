//! Parsing for grammar expansion and expression interpolation inside text.

use crate::Diagnostic;
use crate::ast::{Expr, Span, Spanned};
use crate::parser::parse_expression_at;

/// One lowered piece of source text.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    /// Literal text with source escapes removed.
    Text(String),
    /// Grammar rule expansion.
    GrammarRef {
        /// Explicit grammar, absent for same-grammar references.
        grammar: Option<String>,
        /// Referenced rule.
        rule: String,
    },
    /// Runtime expression interpolation.
    Expression(Spanned<Expr>),
}

/// Split one line or grammar alternative into literal, grammar, and expression parts.
pub fn parse_template(
    source: &str,
    source_span: Span,
) -> Result<Vec<Spanned<TemplatePart>>, Vec<Diagnostic>> {
    let mut parts = Vec::new();
    let mut diagnostics = Vec::new();
    let mut literal = String::new();
    let mut literal_start = 0;
    let mut index = 0;

    while index < source.len() {
        let Some(character) = source[index..].chars().next() else {
            break;
        };
        let width = character.len_utf8();
        if character == '\\' {
            let next_index = index + width;
            if let Some(next) = source[next_index..].chars().next()
                && matches!(next, '{' | '}' | '#' | '\\')
            {
                literal.push(next);
                index = next_index + next.len_utf8();
                continue;
            }
            literal.push(character);
            index += width;
            continue;
        }

        if character != '#' && character != '{' {
            literal.push(character);
            index += width;
            continue;
        }

        flush_literal(&mut parts, &mut literal, literal_start, index, source_span);
        let closing = if character == '#' { '#' } else { '}' };
        let content_start = index + width;
        let Some(relative_end) = find_unescaped(&source[content_start..], closing) else {
            diagnostics.push(
                Diagnostic::error(
                    "W1100",
                    if character == '#' {
                        "unterminated grammar reference"
                    } else {
                        "unterminated expression interpolation"
                    },
                )
                .with_span(relative_span(source_span, source, index, source.len())),
            );
            break;
        };
        let content_end = content_start + relative_end;
        let content = source[content_start..content_end].trim();
        let part_end = content_end + closing.len_utf8();

        if content.is_empty() {
            diagnostics.push(
                Diagnostic::error("W1101", "template marker cannot be empty")
                    .with_span(relative_span(source_span, source, index, part_end)),
            );
        } else if character == '#' {
            match parse_grammar_path(content) {
                Ok((grammar, rule)) => parts.push(Spanned::new(
                    TemplatePart::GrammarRef { grammar, rule },
                    relative_span(source_span, source, index, part_end),
                )),
                Err(message) => {
                    diagnostics.push(Diagnostic::error("W1102", message).with_span(relative_span(
                        source_span,
                        source,
                        index,
                        part_end,
                    )))
                }
            }
        } else {
            let content_relative = source[content_start..content_end]
                .find(content)
                .unwrap_or_default();
            let expression_start = content_start + content_relative;
            let expression_span = relative_span(
                source_span,
                source,
                expression_start,
                expression_start + content.len(),
            );
            match parse_expression_at(
                content,
                expression_span.start,
                expression_span.line,
                expression_span.column,
            ) {
                Ok(expression) => parts.push(Spanned::new(
                    TemplatePart::Expression(expression),
                    relative_span(source_span, source, index, part_end),
                )),
                Err(error) => diagnostics.push(error),
            }
        }

        index = part_end;
        literal_start = index;
    }

    if index < source.len() {
        literal.push_str(&source[index..]);
    }
    flush_literal(
        &mut parts,
        &mut literal,
        literal_start,
        source.len(),
        source_span,
    );

    if diagnostics.is_empty() {
        Ok(parts)
    } else {
        Err(diagnostics)
    }
}

fn flush_literal(
    parts: &mut Vec<Spanned<TemplatePart>>,
    literal: &mut String,
    start: usize,
    end: usize,
    source_span: Span,
) {
    if literal.is_empty() {
        return;
    }
    parts.push(Spanned::new(
        TemplatePart::Text(std::mem::take(literal)),
        Span::new(
            source_span.start + start,
            source_span.start + end,
            source_span.line,
            source_span.column + start,
        ),
    ));
}

fn parse_grammar_path(source: &str) -> Result<(Option<String>, String), String> {
    let parts = source.split('.').collect::<Vec<_>>();
    if !parts.iter().all(|part| valid_identifier(part)) {
        return Err(format!("`{source}` is not a valid grammar reference"));
    }
    match parts.as_slice() {
        [rule] => Ok((None, (*rule).to_owned())),
        [grammar, rule] => Ok((Some((*grammar).to_owned()), (*rule).to_owned())),
        _ => Err("grammar references must be `#rule#` or `#grammar.rule#`".to_owned()),
    }
}

fn valid_identifier(source: &str) -> bool {
    let mut characters = source.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn find_unescaped(source: &str, closing: char) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in source.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == closing {
            return Some(index);
        }
    }
    None
}

fn relative_span(base: Span, source: &str, start: usize, end: usize) -> Span {
    let before = &source[..start];
    let relative_lines = before.bytes().filter(|byte| *byte == b'\n').count();
    let (line, column) = if relative_lines == 0 {
        (base.line, base.column + before.chars().count())
    } else {
        let last_line = before.rsplit('\n').next().unwrap_or_default();
        (base.line + relative_lines, last_line.chars().count() + 1)
    };
    Span::new(base.start + start, base.start + end, line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_grammar_and_expression_parts() {
        let parts = parse_template(
            "Hello #names.full#, visit {visits + 1}.",
            Span::new(0, 45, 1, 1),
        );
        assert!(parts.is_ok(), "{parts:?}");
        assert_eq!(parts.unwrap_or_default().len(), 5);
    }

    #[test]
    fn preserves_escaped_markers_as_text() {
        let parts = parse_template(r"\#literal\# \{value\}", Span::new(0, 21, 1, 1));
        assert!(parts.is_ok(), "{parts:?}");
        let parts = parts.unwrap_or_default();
        assert_eq!(parts.len(), 1);
        assert_eq!(
            parts[0].node,
            TemplatePart::Text("#literal# {value}".to_owned())
        );
    }
}
