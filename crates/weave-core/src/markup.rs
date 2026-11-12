//! Writer metadata inside text: `^tags`, stable line identifiers, and inline markup.
//!
//! Narrative lines, choice tails, and `STRINGS` entries may carry tags. A tag is
//! `^word` or `^key:value` and must follow whitespace or the start of the text.
//! `\^` writes a literal caret. Tags are removed from the displayed text.
//!
//! Narrative lines and `STRINGS` values may also carry inline markup: a pause
//! `<pause 0.6>` and style spans `<name>…</name>`. `\<` writes a literal `<`.
//! Markup is parsed only from authored source; grammar expansions and
//! interpolated values are always plain text, so runtime data cannot inject it.
//!
//! Rendered offsets count Unicode scalar values, not bytes or UTF-16 code units.

use std::collections::BTreeMap;

use crate::Diagnostic;
use crate::ast::{Span, Spanned, Statement};
use crate::ir::{BeatIr, StyleSpanIr};

/// Reserved tag key that sets a line's stable identifier.
pub const ID_TAG: &str = "id";

/// Name of the channel every story declares implicitly.
pub const NARRATION_CHANNEL: &str = "narration";

/// Longest pause, in seconds, that `<pause N>` accepts.
pub const MAX_PAUSE_SECONDS: f64 = 60.0;

/// One `^tag` or `^key:value` written in source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    /// Bare tag word, or the key of a `key:value` tag.
    pub key: String,
    /// Value of a `key:value` tag; `None` for a bare tag.
    pub value: Option<String>,
}

impl Tag {
    /// Render the tag back to its source form, without the leading space.
    #[must_use]
    pub fn to_source(&self) -> String {
        match &self.value {
            Some(value) => format!("^{}:{value}", self.key),
            None => format!("^{}", self.key),
        }
    }
}

/// Text with its tags removed, plus the tags in source order.
#[derive(Debug, Clone, PartialEq)]
pub struct Tagged {
    /// Source text with every tag and the whitespace before it removed.
    /// Escapes such as `\^` and `\<` are preserved for the next parsing stage.
    pub text: String,
    /// Source text with each tag overwritten by spaces, so byte offsets still
    /// match the source. Use it to report diagnostics at exact positions.
    pub blanked: String,
    /// Tags in source order.
    pub tags: Vec<Spanned<Tag>>,
}

/// Tags folded into the serialized shape: bare tags plus sorted metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagSet {
    /// Bare tags in first-seen source order, without duplicates.
    pub tags: Vec<String>,
    /// `key:value` tags, sorted by key. Excludes the reserved `id` key.
    pub meta: BTreeMap<String, String>,
    /// Explicit `^id:` value, when present.
    pub id: Option<String>,
}

/// Split `^tags` out of one source text.
///
/// Characters inside `{…}` interpolations and `#…#` grammar references are
/// never treated as tags. A `^` that follows whitespace (or starts the text)
/// must begin a well-formed tag; otherwise it is `W1050`. A `^` elsewhere, as in
/// `2^8`, is ordinary text.
pub fn extract_tags(source: &str, span: Span) -> Result<Tagged, Vec<Diagnostic>> {
    let mut text = String::with_capacity(source.len());
    let mut blanked = String::with_capacity(source.len());
    let mut tags = Vec::new();
    let mut diagnostics = Vec::new();
    let mut previous_is_space = true;
    let mut index = 0;

    while index < source.len() {
        let Some(character) = source[index..].chars().next() else {
            break;
        };
        let width = character.len_utf8();
        match character {
            '\\' => {
                // Only a recognised escape hides the next character; a lone
                // backslash, as in `a\ ^tag`, is ordinary text.
                let start = index;
                index += width;
                if let Some(next) = source[index..]
                    .chars()
                    .next()
                    .filter(|next| matches!(next, '\\' | '^' | '<' | '>' | '{' | '}' | '#'))
                {
                    index += next.len_utf8();
                }
                text.push_str(&source[start..index]);
                blanked.push_str(&source[start..index]);
                previous_is_space = false;
            }
            '{' | '#' => {
                let closing = if character == '{' { '}' } else { '#' };
                let start = index;
                index += width;
                let end = find_unescaped(&source[index..], closing)
                    .map_or(source.len(), |relative| {
                        index + relative + closing.len_utf8()
                    });
                text.push_str(&source[start..end]);
                blanked.push_str(&source[start..end]);
                index = end;
                previous_is_space = false;
            }
            '^' if previous_is_space => {
                let start = index;
                let end = source[index..]
                    .find(char::is_whitespace)
                    .map_or(source.len(), |relative| index + relative);
                let lexeme = &source[start + 1..end];
                let tag_span = sub_span(span, source, start, end);
                match parse_tag(lexeme) {
                    Ok(tag) => {
                        let kept = text.trim_end().len();
                        text.truncate(kept);
                        blanked.extend(std::iter::repeat_n(' ', end - start));
                        tags.push(Spanned::new(tag, tag_span));
                    }
                    Err(message) => diagnostics.push(
                        Diagnostic::error("W1050", message)
                            .with_span(tag_span)
                            .with_help(
                                "write `^word` or `^key:value`, or escape a literal caret as `\\^`",
                            ),
                    ),
                }
                index = end;
                previous_is_space = false;
            }
            _ => {
                text.push(character);
                blanked.push(character);
                index += width;
                previous_is_space = character.is_whitespace();
            }
        }
    }

    if diagnostics.is_empty() {
        Ok(Tagged {
            text: text.trim().to_owned(),
            blanked,
            tags,
        })
    } else {
        Err(diagnostics)
    }
}

/// Fold source tags into bare tags, sorted metadata, and an optional explicit id.
///
/// A repeated metadata key is `W1051`. An `^id:` value that is not a dotted
/// identifier is `W1052`. When `allow_id` is false, any `^id:` is `W1053`.
pub fn fold_tags(tags: &[Spanned<Tag>], allow_id: bool) -> Result<TagSet, Vec<Diagnostic>> {
    let mut set = TagSet::default();
    let mut diagnostics = Vec::new();
    for tag in tags {
        match &tag.node.value {
            None => {
                if tag.node.key == ID_TAG {
                    diagnostics.push(
                        Diagnostic::error(
                            "W1052",
                            "`^id` needs a value, as in `^id:intro.greeting`",
                        )
                        .with_span(tag.span),
                    );
                } else if !set.tags.contains(&tag.node.key) {
                    set.tags.push(tag.node.key.clone());
                }
            }
            Some(value) if tag.node.key == ID_TAG => {
                if !allow_id {
                    diagnostics.push(
                        Diagnostic::error(
                            "W1053",
                            "`^id:` is only allowed on narrative lines",
                        )
                        .with_span(tag.span)
                        .with_help("choices use compiler-generated ids and STRINGS entries are keyed by name"),
                    );
                } else if !is_line_id(value) {
                    diagnostics.push(
                        Diagnostic::error("W1052", format!("`{value}` is not a valid line id"))
                            .with_span(tag.span)
                            .with_help("use dot-separated segments of letters, digits, and `_`, starting with a letter or `_`"),
                    );
                } else if set.id.is_some() {
                    diagnostics.push(
                        Diagnostic::error("W1051", "a line can set `^id:` only once")
                            .with_span(tag.span),
                    );
                } else {
                    set.id = Some(value.clone());
                }
            }
            Some(value) => {
                if set.meta.contains_key(&tag.node.key) {
                    diagnostics.push(
                        Diagnostic::error(
                            "W1051",
                            format!("tag key `{}` is repeated", tag.node.key),
                        )
                        .with_span(tag.span),
                    );
                } else {
                    set.meta.insert(tag.node.key.clone(), value.clone());
                }
            }
        }
    }
    if diagnostics.is_empty() {
        Ok(set)
    } else {
        Err(diagnostics)
    }
}

fn parse_tag(lexeme: &str) -> Result<Tag, String> {
    let (key, value) = match lexeme.split_once(':') {
        Some((key, value)) => (key, Some(value)),
        None => (lexeme, None),
    };
    if !is_tag_key(key) {
        return Err(if key.is_empty() {
            "a `^` tag needs a name".to_owned()
        } else {
            format!("`^{lexeme}` is not a valid tag")
        });
    }
    if value.is_some_and(str::is_empty) {
        return Err(format!("tag `^{key}:` needs a value"));
    }
    Ok(Tag {
        key: key.to_owned(),
        value: value.map(str::to_owned),
    })
}

/// Whether `key` is a valid tag word: a letter or `_`, then letters, digits, `_`, `-`, or `.`.
#[must_use]
pub fn is_tag_key(key: &str) -> bool {
    let mut characters = key.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

/// Whether `value` is a valid line id: dot-separated segments of ASCII letters,
/// digits, and `_`, where the first segment starts with a letter or `_`.
#[must_use]
pub fn is_line_id(value: &str) -> bool {
    let mut segments = value.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    let first_ok = first
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && first
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    first_ok
        && segments.all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
}

/// Default line id: `<knot>.<n>` with a one-based source-order index.
#[must_use]
pub fn default_line_id(knot: &str, ordinal: usize) -> String {
    format!("{knot}.{ordinal}")
}

/// One narrative line located by [`collect_line_ids`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIdEntry {
    /// Effective id: the explicit `^id:` value or the default.
    pub id: String,
    /// Whether the source sets `^id:` explicitly.
    pub explicit: bool,
    /// Location of the narrative line.
    pub span: Span,
    /// Raw source text of the line, tags included.
    pub raw: String,
}

/// Assign ids to every narrative line in one knot body, in source order.
///
/// Lines inside choice bodies and conditional branches count. A line whose tags
/// cannot be parsed still takes its ordinal, so one bad line never renumbers
/// the rest; it receives its default id.
#[must_use]
pub fn collect_line_ids(knot: &str, body: &[Spanned<Statement>]) -> Vec<LineIdEntry> {
    let mut entries = Vec::new();
    collect_into(knot, body, &mut entries);
    entries
}

fn collect_into(knot: &str, body: &[Spanned<Statement>], entries: &mut Vec<LineIdEntry>) {
    for statement in body {
        match &statement.node {
            Statement::Text(raw) => {
                let ordinal = entries.len() + 1;
                let explicit = extract_tags(raw, statement.span)
                    .ok()
                    .and_then(|tagged| fold_tags(&tagged.tags, true).ok())
                    .and_then(|set| set.id);
                entries.push(LineIdEntry {
                    explicit: explicit.is_some(),
                    id: explicit.unwrap_or_else(|| default_line_id(knot, ordinal)),
                    span: statement.span,
                    raw: raw.clone(),
                });
            }
            Statement::Choice(choice) => collect_into(knot, &choice.body, entries),
            Statement::Conditional(conditional) => {
                for branch in &conditional.branches {
                    collect_into(knot, &branch.body, entries);
                }
                if let Some(fallback) = &conditional.fallback {
                    collect_into(knot, fallback, entries);
                }
            }
            _ => {}
        }
    }
}

/// One element of rendered text before offsets are computed.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    /// Plain text. Never interpreted as markup.
    Text(String),
    /// A pause of the given number of seconds at the current position.
    Pause(f64),
    /// Opening of a named style span.
    Open(String),
    /// Closing of the innermost style span, which has this name.
    Close(String),
}

/// Plain text, Unreal rich text, and offset metadata produced from pieces.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rendered {
    /// Plain text with all markup removed.
    pub text: String,
    /// Unreal `URichTextBlock` form: `<style>text</>`, flattened, escaped.
    pub rich: String,
    /// Style spans in opening order, with offsets in Unicode scalar values.
    pub spans: Vec<StyleSpanIr>,
    /// Pauses in source order, with offsets in Unicode scalar values.
    pub beats: Vec<BeatIr>,
}

/// Render pieces into plain text, rich text, spans, and beats.
///
/// Unbalanced pieces are tolerated here: an unmatched close is ignored and an
/// unclosed span ends at the end of the text. Compile-time validation rejects
/// both before any story reaches this function.
#[must_use]
pub fn render(pieces: &[Piece]) -> Rendered {
    let pieces = trim_pieces(pieces);
    let mut rendered = Rendered::default();
    let mut offset = 0usize;
    let mut open: Vec<(String, usize)> = Vec::new();
    let mut rich_style: Option<String> = None;
    // A pause written between two spaces, as in `a <pause 1> b`, keeps one.
    let mut pause_after_space = false;

    for piece in &pieces {
        match piece {
            Piece::Text(text) => {
                let text = if pause_after_space {
                    text.strip_prefix(' ').unwrap_or(text)
                } else {
                    text
                };
                pause_after_space = false;
                if text.is_empty() {
                    continue;
                }
                let wanted = open.last().map(|(style, _)| style.clone());
                if wanted != rich_style {
                    if rich_style.is_some() {
                        rendered.rich.push_str("</>");
                    }
                    if let Some(style) = &wanted {
                        rendered.rich.push('<');
                        rendered.rich.push_str(style);
                        rendered.rich.push('>');
                    }
                    rich_style = wanted;
                }
                escape_rich_into(text, &mut rendered.rich);
                rendered.text.push_str(text);
                offset += text.chars().count();
            }
            Piece::Pause(seconds) => {
                pause_after_space = rendered.text.ends_with(char::is_whitespace);
                rendered.beats.push(BeatIr {
                    at: offset,
                    pause: *seconds,
                });
            }
            Piece::Open(style) => {
                open.push((style.clone(), rendered.spans.len()));
                rendered.spans.push(StyleSpanIr {
                    style: style.clone(),
                    from: offset,
                    to: offset,
                });
            }
            Piece::Close(_) => {
                if let Some((_, index)) = open.pop() {
                    rendered.spans[index].to = offset;
                }
            }
        }
    }
    for (_, index) in open {
        rendered.spans[index].to = offset;
    }
    if rich_style.is_some() {
        rendered.rich.push_str("</>");
    }
    rendered
}

/// Escape `& < > "` the way Unreal rich text expects.
#[must_use]
pub fn escape_rich(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    escape_rich_into(text, &mut output);
    output
}

fn escape_rich_into(text: &str, output: &mut String) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(character),
        }
    }
}

/// Outcome of reading one markup element that starts at a `<`.
pub(crate) enum MarkupLex {
    /// Not markup: the `<` is literal text.
    Literal,
    /// A markup element and the byte length it occupies, including `<` and `>`.
    Element(Piece, usize),
    /// Malformed markup with a diagnostic message and code.
    Invalid(&'static str, String, usize),
}

/// Read one markup element at the start of `source`, which begins with `<`.
///
/// `<` followed by a letter, `_`, or `/` starts markup that must be well formed.
/// Any other `<`, as in `3 < 4` or `<3`, is literal text.
pub(crate) fn lex_markup(source: &str) -> MarkupLex {
    let rest = &source[1..];
    let starts_markup = rest.chars().next().is_some_and(|character| {
        character.is_ascii_alphabetic() || character == '_' || character == '/'
    });
    if !starts_markup {
        return MarkupLex::Literal;
    }
    let Some(close) = rest
        .find(['>', '\n'])
        .filter(|index| rest.as_bytes()[*index] == b'>')
    else {
        return MarkupLex::Invalid(
            "W1060",
            "unterminated markup; close it with `>` or escape the `<` as `\\<`".to_owned(),
            source.len(),
        );
    };
    let inner = &rest[..close];
    let length = close + 2;
    if let Some(name) = inner.strip_prefix('/') {
        return if is_style_name(name) {
            MarkupLex::Element(Piece::Close(name.to_owned()), length)
        } else {
            MarkupLex::Invalid("W1061", format!("`<{inner}>` is not valid markup"), length)
        };
    }
    if let Some(argument) = inner.strip_prefix("pause")
        && (argument.is_empty() || argument.starts_with(char::is_whitespace))
    {
        let argument = argument.trim();
        return match argument.parse::<f64>() {
            Ok(seconds)
                if seconds.is_finite()
                    && seconds > 0.0
                    && seconds <= MAX_PAUSE_SECONDS
                    && argument
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'.') =>
            {
                MarkupLex::Element(Piece::Pause(seconds), length)
            }
            _ => MarkupLex::Invalid(
                "W1062",
                format!(
                    "`<{inner}>` needs a positive number of seconds no greater than {MAX_PAUSE_SECONDS}, as in `<pause 0.6>`"
                ),
                length,
            ),
        };
    }
    if is_style_name(inner) {
        MarkupLex::Element(Piece::Open(inner.to_owned()), length)
    } else {
        MarkupLex::Invalid("W1061", format!("`<{inner}>` is not valid markup"), length)
    }
}

/// Drop whitespace at either end of the visible text, looking through
/// pauses and style markers, so `Done. <pause 1>` renders as `Done.`.
fn trim_pieces(pieces: &[Piece]) -> Vec<Piece> {
    let mut pieces = pieces.to_vec();
    for piece in &mut pieces {
        if let Piece::Text(text) = piece {
            let trimmed = text.trim_start();
            let done = !trimmed.is_empty();
            *text = trimmed.to_owned();
            if done {
                break;
            }
        }
    }
    for piece in pieces.iter_mut().rev() {
        if let Piece::Text(text) = piece {
            let trimmed = text.trim_end();
            let done = !trimmed.is_empty();
            *text = trimmed.to_owned();
            if done {
                break;
            }
        }
    }
    pieces
}

/// Whether `name` is a valid style name: an identifier other than `pause`.
#[must_use]
pub fn is_style_name(name: &str) -> bool {
    let mut characters = name.chars();
    name != "pause"
        && characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Check that style spans nest and balance with matching names.
///
/// `items` pairs each markup piece with its span. Mismatched or stray closes
/// are `W1063`; spans left open are `W1064`.
pub(crate) fn check_balance(items: &[(Piece, Span)]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<(&str, Span)> = Vec::new();
    for (piece, span) in items {
        match piece {
            Piece::Open(name) => stack.push((name, *span)),
            Piece::Close(name) => match stack.last() {
                Some((open, _)) if open == name => {
                    stack.pop();
                }
                Some((open, _)) => diagnostics.push(
                    Diagnostic::error(
                        "W1063",
                        format!("`</{name}>` closes `<{open}>`; spans must nest"),
                    )
                    .with_span(*span),
                ),
                None => diagnostics.push(
                    Diagnostic::error("W1063", format!("`</{name}>` has no matching `<{name}>`"))
                        .with_span(*span),
                ),
            },
            Piece::Text(_) | Piece::Pause(_) => {}
        }
    }
    for (name, span) in stack {
        diagnostics.push(
            Diagnostic::error("W1064", format!("`<{name}>` is never closed"))
                .with_span(span)
                .with_help(format!("add `</{name}>` later on the same line")),
        );
    }
    diagnostics
}

/// Parse a `STRINGS` value: the raw contents between its quotes.
///
/// String escapes `\\ \" \n \r \t` apply, plus `\<` for a literal `<`. Markup
/// is allowed. `{`, `}`, and `#` are ordinary characters: there is no
/// interpolation.
pub fn parse_string_value(raw: &str, span: Span) -> Result<Vec<Piece>, Vec<Diagnostic>> {
    let mut pieces = Vec::new();
    let mut balance = Vec::new();
    let mut diagnostics = Vec::new();
    let mut literal = String::new();
    let mut index = 0;
    while index < raw.len() {
        let Some(character) = raw[index..].chars().next() else {
            break;
        };
        match character {
            '\\' => {
                let next = raw[index + 1..].chars().next();
                let decoded = match next {
                    Some('\\') => Some('\\'),
                    Some('"') => Some('"'),
                    Some('n') => Some('\n'),
                    Some('r') => Some('\r'),
                    Some('t') => Some('\t'),
                    Some('<') => Some('<'),
                    Some('>') => Some('>'),
                    Some('^') => Some('^'),
                    _ => None,
                };
                if let Some(decoded) = decoded {
                    literal.push(decoded);
                    index += 1 + next.map_or(0, char::len_utf8);
                } else {
                    diagnostics.push(
                        Diagnostic::error("W1032", "unsupported string escape")
                            .with_span(sub_span(span, raw, index, (index + 2).min(raw.len())))
                            .with_help(
                                "use `\\\\`, `\\\"`, `\\n`, `\\r`, `\\t`, `\\<`, `\\>`, or `\\^`",
                            ),
                    );
                    index += 1 + next.map_or(0, char::len_utf8);
                }
            }
            '<' => match lex_markup(&raw[index..]) {
                MarkupLex::Literal => {
                    literal.push('<');
                    index += 1;
                }
                MarkupLex::Element(piece, length) => {
                    if !literal.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut literal)));
                    }
                    balance.push((piece.clone(), sub_span(span, raw, index, index + length)));
                    pieces.push(piece);
                    index += length;
                }
                MarkupLex::Invalid(code, message, length) => {
                    let end = (index + length).min(raw.len());
                    diagnostics.push(
                        Diagnostic::error(code, message).with_span(sub_span(span, raw, index, end)),
                    );
                    index = end;
                }
            },
            _ => {
                literal.push(character);
                index += character.len_utf8();
            }
        }
    }
    if !literal.is_empty() {
        pieces.push(Piece::Text(literal));
    }
    diagnostics.extend(check_balance(&balance));
    if diagnostics.is_empty() {
        Ok(pieces)
    } else {
        Err(diagnostics)
    }
}

pub(crate) fn find_unescaped(source: &str, closing: char) -> Option<usize> {
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

/// Span of `source[start..end]` given the span of `source`, on one line.
pub(crate) fn sub_span(base: Span, source: &str, start: usize, end: usize) -> Span {
    let column = base.column + source[..start].chars().count();
    Span::new(base.start + start, base.start + end, base.line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn tagged(source: &str) -> Tagged {
        extract_tags(source, span()).expect("tags parse")
    }

    #[test]
    fn strips_tags_and_the_whitespace_before_them() {
        let result = tagged("^angry Hello ^beat there. ^id:intro.hello ^mood:wry");
        assert_eq!(result.text, "Hello there.");
        let keys = result
            .tags
            .iter()
            .map(|tag| tag.node.to_source())
            .collect::<Vec<_>>();
        assert_eq!(keys, ["^angry", "^beat", "^id:intro.hello", "^mood:wry"]);
        let set = fold_tags(&result.tags, true).expect("fold");
        assert_eq!(set.tags, ["angry", "beat"]);
        assert_eq!(set.id.as_deref(), Some("intro.hello"));
        assert_eq!(set.meta.get("mood").map(String::as_str), Some("wry"));
    }

    #[test]
    fn carets_inside_words_escapes_and_interpolations_are_text() {
        assert_eq!(tagged("2^8 is 256").text, "2^8 is 256");
        assert_eq!(tagged(r"a \^b").text, r"a \^b");
        assert_eq!(tagged("{x} ^t").text, "{x}");
        assert!(tagged("{ ^nope }").tags.is_empty());
    }

    #[test]
    fn rejects_malformed_tags_and_ids() {
        assert!(extract_tags("bad ^", span()).is_err());
        assert!(extract_tags("bad ^1x", span()).is_err());
        assert!(extract_tags("bad ^k:", span()).is_err());
        let tags = tagged("x ^id:1bad").tags;
        assert!(fold_tags(&tags, true).is_err());
        let tags = tagged("x ^id:ok").tags;
        assert!(fold_tags(&tags, false).is_err());
        let tags = tagged("x ^k:a ^k:b").tags;
        assert!(fold_tags(&tags, true).is_err());
    }

    #[test]
    fn line_ids_accept_numeric_later_segments() {
        assert!(is_line_id("intro.3"));
        assert!(is_line_id("_a.b_c.9"));
        assert!(!is_line_id("3.intro"));
        assert!(!is_line_id("a..b"));
        assert!(!is_line_id("a."));
        assert!(!is_line_id(""));
    }

    #[test]
    fn renders_nested_spans_flattened_and_escaped() {
        let rendered = render(&[
            Piece::Text("A & ".to_owned()),
            Piece::Open("b".to_owned()),
            Piece::Text("bold ".to_owned()),
            Piece::Open("i".to_owned()),
            Piece::Text("<both>".to_owned()),
            Piece::Close("i".to_owned()),
            Piece::Pause(0.5),
            Piece::Text(" \"é\"".to_owned()),
            Piece::Close("b".to_owned()),
            Piece::Text(".".to_owned()),
        ]);
        assert_eq!(rendered.text, "A & bold <both> \"é\".");
        assert_eq!(
            rendered.rich,
            "A &amp; <b>bold </><i>&lt;both&gt;</><b> &quot;é&quot;</>."
        );
        assert_eq!(
            rendered.spans,
            [
                StyleSpanIr {
                    style: "b".to_owned(),
                    from: 4,
                    to: 19
                },
                StyleSpanIr {
                    style: "i".to_owned(),
                    from: 9,
                    to: 15
                },
            ]
        );
        assert_eq!(rendered.beats, [BeatIr { at: 15, pause: 0.5 }]);
    }

    #[test]
    fn a_pause_between_spaces_keeps_one_space() {
        let rendered = render(&[
            Piece::Text("Wait. ".to_owned()),
            Piece::Pause(1.0),
            Piece::Text(" Go.".to_owned()),
        ]);
        assert_eq!(rendered.text, "Wait. Go.");
        assert_eq!(rendered.beats, [BeatIr { at: 6, pause: 1.0 }]);
        let rendered = render(&[
            Piece::Text("Wait.".to_owned()),
            Piece::Pause(1.0),
            Piece::Text(" Go.".to_owned()),
        ]);
        assert_eq!(rendered.text, "Wait. Go.");
    }

    #[test]
    fn lexes_markup_and_leaves_comparisons_literal() {
        assert!(matches!(lex_markup("< 4"), MarkupLex::Literal));
        assert!(matches!(lex_markup("<3"), MarkupLex::Literal));
        assert!(matches!(lex_markup("<=x"), MarkupLex::Literal));
        assert!(matches!(
            lex_markup("<pause 0.6>"),
            MarkupLex::Element(Piece::Pause(seconds), 11) if (seconds - 0.6).abs() < f64::EPSILON
        ));
        assert!(matches!(
            lex_markup("<pause>"),
            MarkupLex::Invalid("W1062", _, _)
        ));
        assert!(matches!(
            lex_markup("<pause -1>"),
            MarkupLex::Invalid("W1062", _, _)
        ));
        assert!(matches!(
            lex_markup("<pause 1e3>"),
            MarkupLex::Invalid("W1062", _, _)
        ));
        assert!(matches!(
            lex_markup("<b class>"),
            MarkupLex::Invalid("W1061", _, _)
        ));
        assert!(matches!(
            lex_markup("<b"),
            MarkupLex::Invalid("W1060", _, _)
        ));
        assert!(matches!(
            lex_markup("<pausex>"),
            MarkupLex::Element(Piece::Open(_), 8)
        ));
    }

    #[test]
    fn string_values_decode_escapes_and_reject_unbalanced_spans() {
        let pieces = parse_string_value(r#"Say \"<b>hi</b>\" \< {x} #y#"#, span()).expect("valid");
        let rendered = render(&pieces);
        assert_eq!(rendered.text, "Say \"hi\" < {x} #y#");
        assert!(parse_string_value("<b>open", span()).is_err());
        assert!(parse_string_value("</b>", span()).is_err());
        assert!(parse_string_value("<b><i></b></i>", span()).is_err());
        assert!(parse_string_value(r"\q", span()).is_err());
    }
}
