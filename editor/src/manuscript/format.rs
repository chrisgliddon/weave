//! The manuscript page: line guides, margins, and styled visual lines.
//!
//! The target manuscript format is double-spaced monospaced prose inside a
//! one-inch margin, measured in characters rather than pixels. Laying the body
//! out into visual lines here keeps wrapping, style resolution, and caret
//! mapping testable without a window.

use super::body::{ChapterBody, StyleSheet};
use super::editor::Selection;

/// The page a chapter is written on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ManuscriptFormat {
    /// Characters per visual line.
    pub measure: usize,
    /// Page margin in logical pixels.
    pub margin: f32,
    /// Baseline distance between visual lines.
    pub line_height: f32,
    /// Base type size in points.
    pub base_size: f32,
}

impl Default for ManuscriptFormat {
    fn default() -> Self {
        Self {
            measure: 64,
            margin: 72.0,
            line_height: 26.0,
            base_size: 12.0,
        }
    }
}

/// Style attributes resolved from the style sheet and direct formatting.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStyle {
    /// Type size in points.
    pub size: f32,
    /// Whether the run is bold.
    pub bold: bool,
    /// Whether the run is italic.
    pub italic: bool,
    /// Whether the run is underlined.
    pub underline: bool,
}

impl Default for ResolvedStyle {
    fn default() -> Self {
        Self {
            size: 12.0,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

/// A run of text with one resolved style.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSegment {
    /// The text.
    pub text: String,
    /// The resolved style.
    pub style: ResolvedStyle,
}

/// One wrapped line of the page.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualLine {
    /// Paragraph the line belongs to.
    pub paragraph: usize,
    /// First character offset of the line inside the paragraph.
    pub start: usize,
    /// One past the last character offset of the line.
    pub end: usize,
    /// Styled runs, in order.
    pub segments: Vec<LineSegment>,
}

/// One rendered piece of a line, with selection state.
#[derive(Debug, Clone, PartialEq)]
pub struct LinePiece {
    /// The text.
    pub text: String,
    /// The resolved style.
    pub style: ResolvedStyle,
    /// Whether the piece is inside the selection.
    pub selected: bool,
}

/// A line split for rendering, with the caret offset when the caret is on it.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLayout {
    /// Pieces in order.
    pub pieces: Vec<LinePiece>,
    /// Caret offset inside the line, in characters.
    pub caret_at: Option<usize>,
}

/// Lay a body out into visual lines.
#[must_use]
pub fn layout(
    body: &ChapterBody,
    styles: &StyleSheet,
    format: ManuscriptFormat,
) -> Vec<VisualLine> {
    let measure = format.measure.max(1);
    let mut lines = Vec::new();
    for (index, paragraph) in body.paragraphs.iter().enumerate() {
        let chars = styled_chars(paragraph, styles, format.base_size);
        if chars.is_empty() {
            lines.push(VisualLine {
                paragraph: index,
                start: 0,
                end: 0,
                segments: Vec::new(),
            });
            continue;
        }
        let mut start = 0;
        while start < chars.len() {
            let end = (start + measure).min(chars.len());
            lines.push(VisualLine {
                paragraph: index,
                start,
                end,
                segments: segments(&chars[start..end]),
            });
            start = end;
        }
    }
    if lines.is_empty() {
        lines.push(VisualLine {
            paragraph: 0,
            start: 0,
            end: 0,
            segments: Vec::new(),
        });
    }
    lines
}

/// Split one line around the caret and the selection.
#[must_use]
pub fn line_layout(line: &VisualLine, selection: Selection) -> LineLayout {
    let (from, to) = selection.ordered();
    let caret = selection.caret;
    let mut pieces: Vec<LinePiece> = Vec::new();
    let mut offset = line.start;
    for segment in &line.segments {
        for character in segment.text.chars() {
            let position = (line.paragraph, offset);
            let selected = !selection.is_collapsed()
                && position >= (from.paragraph, from.offset)
                && position < (to.paragraph, to.offset);
            if let Some(last) = pieces.last_mut()
                && last.style == segment.style
                && last.selected == selected
            {
                last.text.push(character);
            } else {
                pieces.push(LinePiece {
                    text: character.to_string(),
                    style: segment.style.clone(),
                    selected,
                });
            }
            offset += 1;
        }
    }
    let caret_at = (caret.paragraph == line.paragraph
        && caret.offset >= line.start
        && caret.offset <= line.end)
        .then_some(caret.offset - line.start);
    LineLayout { pieces, caret_at }
}

fn styled_chars(
    paragraph: &super::body::Paragraph,
    styles: &StyleSheet,
    base_size: f32,
) -> Vec<(char, ResolvedStyle)> {
    let paragraph_style = paragraph.style.as_deref().and_then(|name| {
        styles
            .paragraph_styles
            .iter()
            .find(|style| style.name == name)
    });
    let mut chars = Vec::new();
    for run in &paragraph.runs {
        let character_style = run.character_style.as_deref().and_then(|name| {
            styles
                .character_styles
                .iter()
                .find(|style| style.name == name)
        });
        let resolved = ResolvedStyle {
            size: character_style
                .map(|style| style.size)
                .or_else(|| paragraph_style.map(|style| style.size))
                .unwrap_or(base_size),
            bold: run.bold
                || character_style.is_some_and(|style| style.bold)
                || paragraph_style.is_some_and(|style| style.bold),
            italic: run.italic
                || character_style.is_some_and(|style| style.italic)
                || paragraph_style.is_some_and(|style| style.italic),
            underline: run.underline,
        };
        chars.extend(
            run.text
                .chars()
                .map(|character| (character, resolved.clone())),
        );
    }
    chars
}

fn segments(chars: &[(char, ResolvedStyle)]) -> Vec<LineSegment> {
    let mut segments: Vec<LineSegment> = Vec::new();
    for (character, style) in chars {
        if let Some(last) = segments.last_mut()
            && last.style == *style
        {
            last.text.push(*character);
        } else {
            segments.push(LineSegment {
                text: character.to_string(),
                style: style.clone(),
            });
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manuscript::body::{ChapterBody, NamedStyle, Paragraph, TextRun};
    use crate::manuscript::editor::Position;

    fn body(text: &str) -> ChapterBody {
        ChapterBody {
            version: 1,
            paragraphs: vec![Paragraph::plain(text)],
        }
    }

    fn styles() -> StyleSheet {
        StyleSheet {
            version: 1,
            paragraph_styles: vec![NamedStyle {
                name: "Body".to_owned(),
                size: 13.0,
                bold: false,
                italic: false,
            }],
            character_styles: vec![NamedStyle {
                name: "Emphasis".to_owned(),
                size: 12.0,
                bold: false,
                italic: true,
            }],
        }
    }

    #[test]
    fn a_long_paragraph_wraps_at_the_measure() {
        let format = ManuscriptFormat {
            measure: 10,
            ..ManuscriptFormat::default()
        };
        let lines = layout(&body("abcdefghijklmnopqrstuvwxyz"), &styles(), format);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].start, 0);
        assert_eq!(lines[0].end, 10);
        assert_eq!(lines[2].end, 26);
    }

    #[test]
    fn an_empty_paragraph_still_occupies_a_line() {
        let lines = layout(&body(""), &styles(), ManuscriptFormat::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].segments.is_empty());
    }

    #[test]
    fn a_paragraph_style_changes_the_whole_line() {
        let mut chapter = body("styled");
        chapter.paragraphs[0].style = Some("Body".to_owned());
        let lines = layout(&chapter, &styles(), ManuscriptFormat::default());
        assert_eq!(lines[0].segments[0].style.size, 13.0);
    }

    #[test]
    fn a_character_style_changes_only_its_run() {
        let mut chapter = body("plain emphasis");
        chapter.paragraphs[0].runs = vec![
            TextRun::plain("plain "),
            TextRun {
                text: "emphasis".to_owned(),
                character_style: Some("Emphasis".to_owned()),
                bold: false,
                italic: false,
                underline: false,
            },
        ];
        let lines = layout(&chapter, &styles(), ManuscriptFormat::default());
        assert_eq!(lines[0].segments.len(), 2);
        assert!(!lines[0].segments[0].style.italic);
        assert!(lines[0].segments[1].style.italic);
    }

    #[test]
    fn the_caret_maps_to_its_line_and_offset() {
        let format = ManuscriptFormat {
            measure: 10,
            ..ManuscriptFormat::default()
        };
        let chapter = body("abcdefghijklmnopqrstuvwxyz");
        let lines = layout(&chapter, &styles(), format);
        let selection = Selection::collapsed(Position {
            paragraph: 0,
            offset: 12,
        });
        assert_eq!(line_layout(&lines[0], selection).caret_at, None);
        assert_eq!(line_layout(&lines[1], selection).caret_at, Some(2));
    }

    #[test]
    fn the_selection_splits_pieces_at_its_boundaries() {
        let chapter = body("abcdef");
        let lines = layout(&chapter, &styles(), ManuscriptFormat::default());
        let selection = Selection {
            anchor: Position {
                paragraph: 0,
                offset: 2,
            },
            caret: Position {
                paragraph: 0,
                offset: 4,
            },
        };
        let split = line_layout(&lines[0], selection);
        let selected: String = split
            .pieces
            .iter()
            .filter(|piece| piece.selected)
            .map(|piece| piece.text.as_str())
            .collect();
        assert_eq!(selected, "cd");
        assert_eq!(split.caret_at, Some(4));
    }
}
