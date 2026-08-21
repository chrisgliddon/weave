//! Unicode-safe source buffer, syntax classification, diagnostics, search, and history.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use weave_compiler::{CompileOptions, compile};
use weave_core::Diagnostic;

/// Syntax category used by the native text renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxKind {
    Comment,
    KnotHeader,
    Keyword,
    String,
    Number,
    GrammarReference,
    PatternCall,
    Divert,
    ChoiceMarker,
    Punctuation,
}

/// One byte-range token. Every boundary is valid UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxToken {
    pub range: Range<usize>,
    pub kind: SyntaxKind,
}

#[derive(Debug, Clone)]
struct TextEdit {
    range: Range<usize>,
    deleted: String,
    inserted: String,
    before_cursor: usize,
    before_anchor: Option<usize>,
    after_cursor: usize,
    after_anchor: Option<usize>,
}

/// Editable `.weave` buffer with coherent history and compiler feedback.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    source: String,
    cursor: usize,
    anchor: Option<usize>,
    tokens: Vec<SyntaxToken>,
    diagnostics: Vec<Diagnostic>,
    undo: Vec<TextEdit>,
    redo: Vec<TextEdit>,
    revision: u64,
    saved_revision: u64,
}

impl TextBuffer {
    /// Create and analyze a source buffer.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        let source = source.into();
        let cursor = source.len();
        let mut buffer = Self {
            tokens: highlight_source(&source),
            source,
            cursor,
            anchor: None,
            diagnostics: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            saved_revision: 0,
        };
        buffer.refresh_diagnostics();
        buffer
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        match self.anchor {
            Some(anchor) if anchor < self.cursor => anchor..self.cursor,
            Some(anchor) => self.cursor..anchor,
            None => self.cursor..self.cursor,
        }
    }

    #[must_use]
    pub fn tokens(&self) -> &[SyntaxToken] {
        &self.tokens
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Mark current content as durably saved.
    pub const fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// Replace the complete document as a clean load boundary.
    pub fn load(&mut self, source: impl Into<String>) {
        self.source = source.into();
        self.cursor = self.source.len();
        self.anchor = None;
        self.undo.clear();
        self.redo.clear();
        self.revision = self.revision.saturating_add(1);
        self.saved_revision = self.revision;
        self.refresh_analysis();
    }

    /// Replace current selection or insert at the caret.
    pub fn insert(&mut self, text: &str) {
        let selection = self.selection();
        self.replace(selection, text);
    }

    /// Delete the grapheme cluster before the selection or caret.
    pub fn delete_backward(&mut self) {
        let selection = self.selection();
        if !selection.is_empty() {
            self.replace(selection, "");
            return;
        }
        let previous = self.previous_grapheme(self.cursor);
        if previous != self.cursor {
            self.replace(previous..self.cursor, "");
        }
    }

    /// Delete the grapheme cluster after the selection or caret.
    pub fn delete_forward(&mut self) {
        let selection = self.selection();
        if !selection.is_empty() {
            self.replace(selection, "");
            return;
        }
        let next = self.next_grapheme(self.cursor);
        if next != self.cursor {
            self.replace(self.cursor..next, "");
        }
    }

    /// Move one grapheme left, optionally extending selection.
    pub fn move_left(&mut self, extend: bool) {
        let destination = if !extend && !self.selection().is_empty() {
            self.selection().start
        } else {
            self.previous_grapheme(self.cursor)
        };
        self.move_to(destination, extend);
    }

    /// Move one grapheme right, optionally extending selection.
    pub fn move_right(&mut self, extend: bool) {
        let destination = if !extend && !self.selection().is_empty() {
            self.selection().end
        } else {
            self.next_grapheme(self.cursor)
        };
        self.move_to(destination, extend);
    }

    /// Move vertically while preserving the Unicode scalar column when possible.
    pub fn move_vertical(&mut self, delta: isize, extend: bool) {
        let (line, column) = self.line_column(self.cursor);
        let target = line.saturating_add_signed(delta).min(self.line_count() - 1);
        let range = self
            .line_range(target)
            .unwrap_or(self.source.len()..self.source.len());
        let line_text = self.source[range.clone()].trim_end_matches(['\r', '\n']);
        let byte_column = line_text
            .char_indices()
            .nth(column)
            .map_or(line_text.len(), |(offset, _)| offset);
        self.move_to(range.start + byte_column, extend);
    }

    /// Move to the beginning of the current line.
    pub fn move_home(&mut self, extend: bool) {
        let (line, _) = self.line_column(self.cursor);
        let start = self.line_range(line).map_or(0, |range| range.start);
        self.move_to(start, extend);
    }

    /// Move to the end of the current line, before its newline.
    pub fn move_end(&mut self, extend: bool) {
        let (line, _) = self.line_column(self.cursor);
        let end = self.line_range(line).map_or(self.source.len(), |range| {
            range.start
                + self.source[range.clone()]
                    .trim_end_matches(['\r', '\n'])
                    .len()
        });
        self.move_to(end, extend);
    }

    /// Place the caret at a byte offset, clamped to a UTF-8 boundary.
    pub fn move_to(&mut self, offset: usize, extend: bool) {
        let offset = floor_char_boundary(&self.source, offset.min(self.source.len()));
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = offset;
    }

    /// Select the complete source.
    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.source.len();
    }

    /// Select an explicit byte range after clamping both ends to UTF-8 boundaries.
    pub fn select_range(&mut self, range: Range<usize>) {
        let start = floor_char_boundary(&self.source, range.start.min(self.source.len()));
        let end = floor_char_boundary(&self.source, range.end.min(self.source.len())).max(start);
        self.anchor = Some(start);
        self.cursor = end;
    }

    /// Undo one edit.
    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.undo.pop() else {
            return false;
        };
        let inserted_range = edit.range.start..edit.range.start + edit.inserted.len();
        self.source.replace_range(inserted_range, &edit.deleted);
        self.cursor = edit.before_cursor;
        self.anchor = edit.before_anchor;
        self.revision = self.revision.saturating_sub(1);
        self.redo.push(edit);
        self.refresh_analysis();
        true
    }

    /// Redo one edit.
    pub fn redo(&mut self) -> bool {
        let Some(edit) = self.redo.pop() else {
            return false;
        };
        let deleted_range = edit.range.start..edit.range.start + edit.deleted.len();
        self.source.replace_range(deleted_range, &edit.inserted);
        self.cursor = edit.after_cursor;
        self.anchor = edit.after_anchor;
        self.revision = self.revision.saturating_add(1);
        self.undo.push(edit);
        self.refresh_analysis();
        true
    }

    /// Canonically format valid source as one undoable edit.
    pub fn format(&mut self) -> Result<bool, Vec<Diagnostic>> {
        let formatted = weave_fmt::format_source(&self.source)?;
        if formatted == self.source {
            return Ok(false);
        }
        let length = self.source.len();
        self.replace(0..length, &formatted);
        Ok(true)
    }

    /// Find non-overlapping literal matches in deterministic byte order.
    #[must_use]
    pub fn search(&self, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
        if query.is_empty() {
            return Vec::new();
        }
        if case_sensitive {
            return self
                .source
                .match_indices(query)
                .map(|(start, value)| start..start + value.len())
                .collect();
        }
        unicode_case_insensitive_matches(&self.source, query)
    }

    /// Move to a compiler diagnostic by index.
    pub fn navigate_to_diagnostic(&mut self, index: usize) -> bool {
        let Some(span) = self
            .diagnostics
            .get(index)
            .and_then(|diagnostic| diagnostic.span)
        else {
            return false;
        };
        self.move_to(span.start, false);
        true
    }

    /// Zero-based line and Unicode scalar column for one byte offset.
    #[must_use]
    pub fn line_column(&self, offset: usize) -> (usize, usize) {
        let offset = floor_char_boundary(&self.source, offset.min(self.source.len()));
        let before = &self.source[..offset];
        let line = before.bytes().filter(|byte| *byte == b'\n').count();
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let column = self.source[line_start..offset].chars().count();
        (line, column)
    }

    /// Number of logical lines; an empty buffer has one line.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.source.bytes().filter(|byte| *byte == b'\n').count() + 1
    }

    /// Byte range for one zero-based line, including its newline when present.
    #[must_use]
    pub fn line_range(&self, target: usize) -> Option<Range<usize>> {
        let mut start = 0;
        for (line, chunk) in self.source.split_inclusive('\n').enumerate() {
            let end = start + chunk.len();
            if line == target {
                return Some(start..end);
            }
            start = end;
        }
        if target + 1 == self.line_count() {
            Some(start..self.source.len())
        } else {
            None
        }
    }

    /// Highlight tokens intersecting one line range.
    #[must_use]
    pub fn tokens_for_line(&self, line: usize) -> Vec<SyntaxToken> {
        let Some(range) = self.line_range(line) else {
            return Vec::new();
        };
        self.tokens
            .iter()
            .filter(|token| token.range.start < range.end && token.range.end > range.start)
            .cloned()
            .collect()
    }

    fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = floor_char_boundary(&self.source, range.start.min(self.source.len()));
        let end = floor_char_boundary(&self.source, range.end.min(self.source.len())).max(start);
        let range = start..end;
        let before_cursor = self.cursor;
        let before_anchor = self.anchor;
        let deleted = self.source[range.clone()].to_owned();
        self.source.replace_range(range.clone(), text);
        self.cursor = range.start + text.len();
        self.anchor = None;
        self.undo.push(TextEdit {
            range,
            deleted,
            inserted: text.to_owned(),
            before_cursor,
            before_anchor,
            after_cursor: self.cursor,
            after_anchor: self.anchor,
        });
        if self.undo.len() > 1_000 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision = self.revision.saturating_add(1);
        self.refresh_analysis();
    }

    fn refresh_analysis(&mut self) {
        self.tokens = highlight_source(&self.source);
        self.refresh_diagnostics();
    }

    fn refresh_diagnostics(&mut self) {
        self.diagnostics = match compile(&self.source, &CompileOptions::default()) {
            Ok(compiled) => compiled.diagnostics,
            Err(error) => error.diagnostics,
        };
    }

    fn previous_grapheme(&self, offset: usize) -> usize {
        self.source
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_grapheme(&self, offset: usize) -> usize {
        self.source
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.source.len())
    }
}

/// Classify source constructs without allocating editor-specific syntax trees.
#[must_use]
pub fn highlight_source(source: &str) -> Vec<SyntaxToken> {
    let mut tokens = Vec::new();
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        highlight_line(line, line_start, &mut tokens);
        line_start += line.len();
    }
    if (source.is_empty() || !source.ends_with('\n')) && line_start < source.len() {
        highlight_line(&source[line_start..], line_start, &mut tokens);
    }
    tokens
}

fn highlight_line(line: &str, base: usize, tokens: &mut Vec<SyntaxToken>) {
    let body = line.trim_end_matches(['\r', '\n']);
    let leading = body.len() - body.trim_start().len();
    let trimmed = &body[leading..];
    if trimmed.starts_with("//") {
        push_token(
            tokens,
            base + leading..base + body.len(),
            SyntaxKind::Comment,
        );
        return;
    }
    if trimmed.starts_with("===") && trimmed.ends_with("===") {
        push_token(
            tokens,
            base + leading..base + body.len(),
            SyntaxKind::KnotHeader,
        );
        return;
    }

    let bytes = body.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'/' {
            push_token(tokens, base + index..base + body.len(), SyntaxKind::Comment);
            break;
        }
        if bytes[index] == b'"' {
            let mut end = index + 1;
            let mut escaped = false;
            while end < bytes.len() {
                if bytes[end] == b'"' && !escaped {
                    end += 1;
                    break;
                }
                escaped = bytes[end] == b'\\' && !escaped;
                if bytes[end] != b'\\' {
                    escaped = false;
                }
                end += 1;
            }
            push_token(tokens, base + index..base + end, SyntaxKind::String);
            index = end;
            continue;
        }
        if bytes[index] == b'#'
            && let Some(relative) = body[index + 1..].find('#')
        {
            let end = index + relative + 2;
            push_token(
                tokens,
                base + index..base + end,
                SyntaxKind::GrammarReference,
            );
            index = end;
            continue;
        }
        if index + 1 < bytes.len() && matches!(&bytes[index..index + 2], b"->" | b"<-") {
            push_token(tokens, base + index..base + index + 2, SyntaxKind::Divert);
            index += 2;
            continue;
        }
        if index == leading && matches!(bytes[index], b'*' | b'+') {
            push_token(
                tokens,
                base + index..base + index + 1,
                SyntaxKind::ChoiceMarker,
            );
            index += 1;
            continue;
        }
        if bytes[index].is_ascii_digit() {
            let end = take_while(bytes, index + 1, |byte| {
                byte.is_ascii_digit() || byte == b'.'
            });
            push_token(tokens, base + index..base + end, SyntaxKind::Number);
            index = end;
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let end = take_while(bytes, index + 1, |byte| {
                byte.is_ascii_alphanumeric() || byte == b'_'
            });
            let word = &body[index..end];
            let kind = if is_keyword(word) {
                Some(SyntaxKind::Keyword)
            } else if body[end..].starts_with(".spread")
                || body[end..].starts_with(".draw")
                || word == "draw"
            {
                Some(SyntaxKind::PatternCall)
            } else {
                None
            };
            if let Some(kind) = kind {
                push_token(tokens, base + index..base + end, kind);
            }
            index = end;
            continue;
        }
        if matches!(
            bytes[index],
            b'{' | b'}' | b'[' | b']' | b'(' | b')' | b':' | b','
        ) {
            push_token(
                tokens,
                base + index..base + index + 1,
                SyntaxKind::Punctuation,
            );
        }
        index += utf8_character_width(bytes[index]);
    }
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "grammar"
            | "pattern"
            | "spread"
            | "builtin"
            | "draw"
            | "reversals"
            | "duplicates"
            | "VAR"
            | "LIST"
            | "FLAG"
            | "STATE"
            | "SET"
            | "PUSH"
            | "REMOVE"
            | "END"
            | "if"
            | "else"
            | "and"
            | "or"
            | "not"
            | "in"
    )
}

fn push_token(tokens: &mut Vec<SyntaxToken>, range: Range<usize>, kind: SyntaxKind) {
    if range.start < range.end {
        tokens.push(SyntaxToken { range, kind });
    }
}

fn take_while(bytes: &[u8], mut index: usize, predicate: impl Fn(u8) -> bool) -> usize {
    while index < bytes.len() && predicate(bytes[index]) {
        index += 1;
    }
    index
}

fn utf8_character_width(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first < 0xe0 {
        2
    } else if first < 0xf0 {
        3
    } else {
        4
    }
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn unicode_case_insensitive_matches(source: &str, query: &str) -> Vec<Range<usize>> {
    let folded_query = query.to_lowercase();
    let mut matches = Vec::new();
    let mut start = 0;
    while start < source.len() {
        let mut folded = String::new();
        let mut matched_end = None;
        for (relative, character) in source[start..].char_indices() {
            folded.extend(character.to_lowercase());
            let end = start + relative + character.len_utf8();
            if folded == folded_query {
                matched_end = Some(end);
                break;
            }
            if !folded_query.starts_with(&folded) {
                break;
            }
        }
        if let Some(end) = matched_end {
            matches.push(start..end);
            start = end;
        } else {
            start += source[start..].chars().next().map_or(0, char::len_utf8);
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CONSTRUCTS: &str = r#"// comment
grammar names {
    value: ["Mira", "Aldric"]
}
pattern cards {
    builtin: tarot
    draw: uniform
}
VAR score = 12
=== start ===
#names.value#
* [Draw] -> reading
VAR card = cards.spread.single.draw()
{score > 1:
    Ready.
}
=== reading ===
-> END
"#;

    #[test]
    fn highlighting_covers_every_language_surface() {
        let kinds = highlight_source(ALL_CONSTRUCTS)
            .into_iter()
            .map(|token| token.kind)
            .collect::<std::collections::HashSet<_>>();
        for expected in [
            SyntaxKind::Comment,
            SyntaxKind::KnotHeader,
            SyntaxKind::Keyword,
            SyntaxKind::String,
            SyntaxKind::Number,
            SyntaxKind::GrammarReference,
            SyntaxKind::PatternCall,
            SyntaxKind::Divert,
            SyntaxKind::ChoiceMarker,
            SyntaxKind::Punctuation,
        ] {
            assert!(kinds.contains(&expected), "missing {expected:?}");
        }
    }

    #[test]
    fn unicode_editing_uses_grapheme_boundaries_and_round_trips_history() {
        let mut buffer = TextBuffer::new("=== start ===\né👩‍🚀\n-> END\n");
        let emoji_end = buffer.source().find('\n').expect("header newline") + 1 + "é👩‍🚀".len();
        buffer.move_to(emoji_end, false);
        buffer.delete_backward();
        assert!(buffer.source().contains("é\n"));
        assert!(buffer.undo());
        assert!(buffer.source().contains("é👩‍🚀\n"));
        assert!(buffer.redo());
        assert!(buffer.source().is_char_boundary(buffer.cursor()));
    }

    #[test]
    fn loading_a_project_resets_history_and_dirty_state() {
        let mut buffer = TextBuffer::new("=== start ===\nFirst.\n-> END\n");
        buffer.insert("changed");
        assert!(buffer.is_dirty());
        buffer.load("=== start ===\nSecond.\n-> END\n");
        assert!(!buffer.is_dirty());
        assert!(!buffer.undo());
        assert!(buffer.source().contains("Second."));
    }

    #[test]
    fn formatter_search_selection_and_diagnostics_stay_current() {
        let mut buffer = TextBuffer::new("=== start ===\nHello, 世界.\n->END\n");
        assert!(buffer.format().expect("source formats"));
        assert!(buffer.source().contains("-> END"));
        assert_eq!(buffer.search("世界", true).len(), 1);
        assert_eq!(
            TextBuffer::new("İstanbul ISTANBUL")
                .search("istanbul", false)
                .len(),
            1
        );
        buffer.insert("{");
        assert!(!buffer.diagnostics().is_empty());
        assert!(buffer.diagnostics().iter().all(|diagnostic| {
            diagnostic
                .span
                .is_none_or(|span| span.start <= buffer.source().len())
        }));
        assert!(buffer.navigate_to_diagnostic(0));
        assert!(buffer.source().is_char_boundary(buffer.cursor()));
    }

    #[test]
    fn large_unicode_documents_keep_line_and_token_ranges_valid() {
        let mut source = String::from("=== start ===\n");
        for index in 0..5_000 {
            source.push_str(&format!("Line {index}: 星と風.\n"));
        }
        source.push_str("-> END\n");
        let mut buffer = TextBuffer::new(source);
        assert_eq!(buffer.line_count(), 5_003);
        buffer.move_to(buffer.source().len(), false);
        buffer.insert("// fin\n");
        assert!(buffer.tokens().iter().all(|token| {
            buffer.source().is_char_boundary(token.range.start)
                && buffer.source().is_char_boundary(token.range.end)
        }));
        assert!(buffer.line_range(4_999).is_some());
    }
}
