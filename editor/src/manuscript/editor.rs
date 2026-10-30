//! Caret, selection, and the editing operations behind the chapter editor.
//!
//! Everything here is pure data over [`ChapterBody`], so the editing rules are
//! tested without a window. Offsets are character offsets, never byte offsets,
//! because a manuscript is prose and every offset crosses multi-byte text.

use serde::{Deserialize, Serialize};

use super::body::{ChapterBody, Paragraph, TextRun};

/// A caret position inside the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    /// Paragraph index.
    pub paragraph: usize,
    /// Character offset inside the paragraph.
    pub offset: usize,
}

impl Position {
    /// The start of the body.
    #[must_use]
    pub const fn start() -> Self {
        Self {
            paragraph: 0,
            offset: 0,
        }
    }
}

/// An anchor and a caret. A collapsed selection is a plain caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// Where the selection began.
    pub anchor: Position,
    /// Where the caret currently is.
    pub caret: Position,
}

impl Selection {
    /// A selection with no extent.
    #[must_use]
    pub const fn collapsed(caret: Position) -> Self {
        Self {
            anchor: caret,
            caret,
        }
    }

    /// Whether the selection has no extent.
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.caret
    }

    /// The selection ordered from start to end.
    #[must_use]
    pub fn ordered(&self) -> (Position, Position) {
        if self.anchor <= self.caret {
            (self.anchor, self.caret)
        } else {
            (self.caret, self.anchor)
        }
    }
}

/// A direct character format the toolbar can toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectFormat {
    /// Bold.
    Bold,
    /// Italic.
    Italic,
    /// Underline.
    Underline,
}

impl DirectFormat {
    fn get(self, run: &TextRun) -> bool {
        match self {
            Self::Bold => run.bold,
            Self::Italic => run.italic,
            Self::Underline => run.underline,
        }
    }

    fn set(self, run: &mut TextRun, value: bool) {
        match self {
            Self::Bold => run.bold = value,
            Self::Italic => run.italic = value,
            Self::Underline => run.underline = value,
        }
    }
}

/// Where a caret movement goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movement {
    /// One character left, across paragraph boundaries.
    Left,
    /// One character right, across paragraph boundaries.
    Right,
    /// Start of the paragraph.
    LineStart,
    /// End of the paragraph.
    LineEnd,
    /// Previous paragraph at the same offset.
    Up,
    /// Next paragraph at the same offset.
    Down,
    /// Start of the body.
    DocumentStart,
    /// End of the body.
    DocumentEnd,
}

/// Build a selection from a caret movement.
#[must_use]
pub fn move_selection(
    body: &ChapterBody,
    selection: Selection,
    movement: Movement,
    extend: bool,
) -> Selection {
    let caret = movement_target(body, selection.caret, movement);
    if extend {
        Selection {
            anchor: selection.anchor,
            caret,
        }
    } else {
        Selection::collapsed(caret)
    }
}

fn movement_target(body: &ChapterBody, caret: Position, movement: Movement) -> Position {
    let position = clamp_position(body, caret);
    match movement {
        Movement::Left => {
            if position.offset > 0 {
                Position {
                    offset: position.offset - 1,
                    ..position
                }
            } else if position.paragraph > 0 {
                let previous = position.paragraph - 1;
                Position {
                    paragraph: previous,
                    offset: body.paragraphs.get(previous).map_or(0, length_of),
                }
            } else {
                Position::start()
            }
        }
        Movement::Right => {
            let length = body.paragraphs.get(position.paragraph).map_or(0, length_of);
            if position.offset < length {
                Position {
                    offset: position.offset + 1,
                    ..position
                }
            } else if position.paragraph + 1 < body.paragraphs.len() {
                Position {
                    paragraph: position.paragraph + 1,
                    offset: 0,
                }
            } else {
                position
            }
        }
        Movement::LineStart => Position {
            offset: 0,
            ..position
        },
        Movement::LineEnd => Position {
            offset: body.paragraphs.get(position.paragraph).map_or(0, length_of),
            ..position
        },
        Movement::Up => {
            if position.paragraph == 0 {
                Position::start()
            } else {
                Position {
                    paragraph: position.paragraph - 1,
                    offset: position.offset,
                }
            }
        }
        Movement::Down => {
            if position.paragraph + 1 < body.paragraphs.len() {
                Position {
                    paragraph: position.paragraph + 1,
                    offset: position.offset,
                }
            } else {
                position
            }
        }
        Movement::DocumentStart => Position::start(),
        Movement::DocumentEnd => {
            let last = body.paragraphs.len().saturating_sub(1);
            Position {
                paragraph: last,
                offset: body.paragraphs.get(last).map_or(0, length_of),
            }
        }
    }
}

fn clamp_position(body: &ChapterBody, position: Position) -> Position {
    let paragraph = position
        .paragraph
        .min(body.paragraphs.len().saturating_sub(1));
    let offset = position
        .offset
        .min(body.paragraphs.get(paragraph).map_or(0, length_of));
    Position { paragraph, offset }
}

/// Replace the selection with `text` and return the caret after it.
pub fn insert(body: &mut ChapterBody, selection: Selection, text: &str) -> Selection {
    let (start, end) = ordered_clamped(body, selection);
    remove_range(body, start, end);
    if text.is_empty() {
        return Selection::collapsed(start);
    }
    Selection::collapsed(insert_at(body, start, text))
}

/// Delete the selection, or one character before the caret.
pub fn backspace(body: &mut ChapterBody, selection: Selection) -> Selection {
    if !selection.is_collapsed() {
        let (start, end) = ordered_clamped(body, selection);
        remove_range(body, start, end);
        return Selection::collapsed(start);
    }
    let caret = clamp_position(body, selection.caret);
    if caret.offset > 0 {
        let start = Position {
            offset: caret.offset - 1,
            ..caret
        };
        remove_range(body, start, caret);
        Selection::collapsed(start)
    } else if caret.paragraph > 0 {
        let previous = caret.paragraph - 1;
        let offset = body.paragraphs.get(previous).map_or(0, length_of);
        remove_range(
            body,
            Position {
                paragraph: previous,
                offset,
            },
            caret,
        );
        Selection::collapsed(Position {
            paragraph: previous,
            offset,
        })
    } else {
        Selection::collapsed(caret)
    }
}

/// Split the paragraph at the caret, as Enter does.
pub fn split_paragraph(body: &mut ChapterBody, selection: Selection) -> Selection {
    let (start, end) = ordered_clamped(body, selection);
    remove_range(body, start, end);
    let caret = clamp_position(body, start);
    let Some(paragraph) = body.paragraphs.get(caret.paragraph) else {
        return Selection::collapsed(Position::start());
    };
    let head = slice_runs(&paragraph.runs, 0, caret.offset);
    let tail = slice_runs(&paragraph.runs, caret.offset, usize::MAX);
    let style = paragraph.style.clone();
    body.paragraphs[caret.paragraph].runs = head;
    body.paragraphs
        .insert(caret.paragraph + 1, Paragraph { runs: tail, style });
    Selection::collapsed(Position {
        paragraph: caret.paragraph + 1,
        offset: 0,
    })
}

/// Toggle a direct format over the selection.
///
/// Returns `false` when the selection is collapsed, because direct formatting
/// applies to text rather than to the caret. The toolbar mirrors that rule.
pub fn toggle_direct(body: &mut ChapterBody, selection: Selection, format: DirectFormat) -> bool {
    if selection.is_collapsed() {
        return false;
    }
    let (start, end) = ordered_clamped(body, selection);
    let all_set = all_chars(body, start, end, |run| format.get(run));
    map_runs(body, start, end, |run| format.set(run, !all_set));
    true
}

/// Apply a named character style, or clear it, over the selection.
///
/// Returns `false` when the selection is collapsed.
pub fn apply_character_style(
    body: &mut ChapterBody,
    selection: Selection,
    style: Option<&str>,
) -> bool {
    if selection.is_collapsed() {
        return false;
    }
    let (start, end) = ordered_clamped(body, selection);
    let style = style.map(str::to_owned);
    map_runs(body, start, end, |run| {
        run.character_style.clone_from(&style);
    });
    true
}

/// Apply a named paragraph style to every paragraph the selection touches.
pub fn apply_paragraph_style(body: &mut ChapterBody, selection: Selection, style: Option<&str>) {
    let (start, end) = ordered_clamped(body, selection);
    for index in start.paragraph..=end.paragraph {
        if let Some(paragraph) = body.paragraphs.get_mut(index) {
            paragraph.style = style.map(str::to_owned);
        }
    }
}

/// Whether every character in the selection carries the direct format.
#[must_use]
pub fn all_direct(body: &ChapterBody, selection: Selection, format: DirectFormat) -> bool {
    let (start, end) = ordered_clamped(body, selection);
    all_chars(body, start, end, |run| format.get(run))
}

fn ordered_clamped(body: &ChapterBody, selection: Selection) -> (Position, Position) {
    let (start, end) = selection.ordered();
    (clamp_position(body, start), clamp_position(body, end))
}

fn remove_range(body: &mut ChapterBody, start: Position, end: Position) {
    if start == end {
        return;
    }
    if start.paragraph == end.paragraph {
        if let Some(paragraph) = body.paragraphs.get_mut(start.paragraph) {
            let mut runs = slice_runs(&paragraph.runs, 0, start.offset);
            for run in slice_runs(&paragraph.runs, end.offset, usize::MAX) {
                push_run(&mut runs, run);
            }
            paragraph.runs = runs;
        }
        return;
    }
    let head = body
        .paragraphs
        .get(start.paragraph)
        .map(|paragraph| slice_runs(&paragraph.runs, 0, start.offset))
        .unwrap_or_default();
    let tail = body
        .paragraphs
        .get(end.paragraph)
        .map(|paragraph| slice_runs(&paragraph.runs, end.offset, usize::MAX))
        .unwrap_or_default();
    let mut runs = head;
    for run in tail {
        push_run(&mut runs, run);
    }
    if let Some(paragraph) = body.paragraphs.get_mut(start.paragraph) {
        paragraph.runs = runs;
    }
    let from = start.paragraph + 1;
    let to = (end.paragraph + 1).min(body.paragraphs.len());
    if from < to {
        body.paragraphs.drain(from..to);
    }
}

fn insert_at(body: &mut ChapterBody, position: Position, text: &str) -> Position {
    let position = clamp_position(body, position);
    if body.paragraphs.is_empty() {
        body.paragraphs.push(Paragraph::plain(text));
        return Position {
            paragraph: 0,
            offset: text.chars().count(),
        };
    }
    let style_source = run_style_at(&body.paragraphs[position.paragraph], position.offset);
    let target = &mut body.paragraphs[position.paragraph];
    let mut runs = slice_runs(&target.runs, 0, position.offset);
    push_run(
        &mut runs,
        TextRun {
            text: text.to_owned(),
            ..style_source
        },
    );
    for run in slice_runs(&target.runs, position.offset, usize::MAX) {
        push_run(&mut runs, run);
    }
    target.runs = runs;
    Position {
        paragraph: position.paragraph,
        offset: position.offset + text.chars().count(),
    }
}

fn run_style_at(paragraph: &Paragraph, offset: usize) -> TextRun {
    let mut cursor = 0;
    for run in &paragraph.runs {
        let length = run.text.chars().count();
        if length > 0 && offset <= cursor + length {
            return TextRun {
                text: String::new(),
                ..run.clone()
            };
        }
        cursor += length;
    }
    paragraph.runs.last().map_or_else(
        || TextRun::plain(String::new()),
        |run| TextRun {
            text: String::new(),
            ..run.clone()
        },
    )
}

fn slice_runs(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
    if start >= end {
        return Vec::new();
    }
    let mut sliced = Vec::new();
    let mut cursor = 0;
    for run in runs {
        let length = run.text.chars().count();
        let run_start = cursor;
        let run_end = run_start + length;
        cursor = run_end;
        let from = start.max(run_start);
        let to = end.min(run_end);
        if from >= to {
            continue;
        }
        let text: String = run
            .text
            .chars()
            .skip(from - run_start)
            .take(to - from)
            .collect();
        push_run(
            &mut sliced,
            TextRun {
                text,
                ..run.clone()
            },
        );
    }
    sliced
}

fn slice_one(run: &TextRun, from: usize, to: usize) -> Option<TextRun> {
    if from >= to {
        return None;
    }
    let text: String = run.text.chars().skip(from).take(to - from).collect();
    (!text.is_empty()).then(|| TextRun {
        text,
        ..run.clone()
    })
}

fn push_run(runs: &mut Vec<TextRun>, run: TextRun) {
    if run.text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut()
        && last.character_style == run.character_style
        && last.bold == run.bold
        && last.italic == run.italic
        && last.underline == run.underline
    {
        last.text.push_str(&run.text);
        return;
    }
    runs.push(run);
}

fn length_of(paragraph: &Paragraph) -> usize {
    paragraph
        .runs
        .iter()
        .map(|run| run.text.chars().count())
        .sum()
}

fn all_chars(
    body: &ChapterBody,
    start: Position,
    end: Position,
    predicate: impl Fn(&TextRun) -> bool,
) -> bool {
    if start == end {
        return false;
    }
    let mut seen = false;
    for index in start.paragraph..=end.paragraph {
        let Some(paragraph) = body.paragraphs.get(index) else {
            continue;
        };
        let from = if index == start.paragraph {
            start.offset
        } else {
            0
        };
        let to = if index == end.paragraph {
            end.offset
        } else {
            length_of(paragraph)
        };
        if from >= to {
            continue;
        }
        let mut cursor = 0;
        for run in &paragraph.runs {
            let length = run.text.chars().count();
            let run_end = cursor + length;
            if from.max(cursor) < to.min(run_end) {
                seen = true;
                if !predicate(run) {
                    return false;
                }
            }
            cursor = run_end;
        }
    }
    seen
}

fn map_runs(body: &mut ChapterBody, start: Position, end: Position, apply: impl Fn(&mut TextRun)) {
    for index in start.paragraph..=end.paragraph {
        let Some(paragraph) = body.paragraphs.get_mut(index) else {
            continue;
        };
        let from = if index == start.paragraph {
            start.offset
        } else {
            0
        };
        let to = if index == end.paragraph {
            end.offset
        } else {
            length_of(paragraph)
        };
        if from >= to {
            continue;
        }
        let mut rebuilt = Vec::new();
        let mut cursor = 0;
        for run in &paragraph.runs {
            let length = run.text.chars().count();
            let run_start = cursor;
            let run_end = run_start + length;
            cursor = run_end;
            if run_end <= from || run_start >= to {
                push_run(&mut rebuilt, run.clone());
                continue;
            }
            if let Some(before) = slice_one(run, 0, from.saturating_sub(run_start)) {
                push_run(&mut rebuilt, before);
            }
            let middle_from = from.max(run_start) - run_start;
            let middle_to = to.min(run_end) - run_start;
            if let Some(mut middle) = slice_one(run, middle_from, middle_to) {
                apply(&mut middle);
                push_run(&mut rebuilt, middle);
            }
            if run_end > to
                && let Some(after) = slice_one(run, to - run_start, run_end - run_start)
            {
                push_run(&mut rebuilt, after);
            }
        }
        paragraph.runs = rebuilt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_from(paragraphs: &[&str]) -> ChapterBody {
        ChapterBody {
            version: 1,
            paragraphs: paragraphs
                .iter()
                .map(|text| Paragraph::plain(*text))
                .collect(),
        }
    }

    fn at(paragraph: usize, offset: usize) -> Selection {
        Selection::collapsed(Position { paragraph, offset })
    }

    fn range(from: (usize, usize), to: (usize, usize)) -> Selection {
        Selection {
            anchor: Position {
                paragraph: from.0,
                offset: from.1,
            },
            caret: Position {
                paragraph: to.0,
                offset: to.1,
            },
        }
    }

    fn text(body: &ChapterBody) -> String {
        body.text()
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut body = body_from(&["The lantern"]);
        let selection = insert(&mut body, at(0, 11), " guttered");
        assert_eq!(text(&body), "The lantern guttered");
        assert_eq!(selection.caret.offset, 20);
    }

    #[test]
    fn typing_inside_a_bold_run_stays_bold() {
        let mut body = body_from(&["bold text"]);
        body.paragraphs[0].runs = vec![TextRun {
            text: "bold text".to_owned(),
            character_style: None,
            bold: true,
            italic: false,
            underline: false,
        }];
        insert(&mut body, at(0, 4), "er");
        assert_eq!(body.paragraphs[0].runs.len(), 1);
        assert!(body.paragraphs[0].runs[0].bold);
        assert_eq!(body.paragraphs[0].runs[0].text, "bolder text");
    }

    #[test]
    fn deleting_a_selection_inside_one_paragraph_joins_the_ends() {
        let mut body = body_from(&["abcdef"]);
        backspace(&mut body, range((0, 2), (0, 4)));
        assert_eq!(text(&body), "abef");
    }

    #[test]
    fn deleting_a_selection_across_paragraphs_merges_them() {
        let mut body = body_from(&["first line", "second line"]);
        backspace(&mut body, range((0, 6), (1, 7)));
        assert_eq!(body.paragraphs.len(), 1);
        assert_eq!(text(&body), "first line");
    }

    #[test]
    fn backspace_at_a_paragraph_start_merges_upwards() {
        let mut body = body_from(&["first", "second"]);
        let caret = backspace(&mut body, at(1, 0));
        assert_eq!(body.paragraphs.len(), 1);
        assert_eq!(text(&body), "firstsecond");
        assert_eq!(
            caret.caret,
            Position {
                paragraph: 0,
                offset: 5
            }
        );
    }

    #[test]
    fn enter_splits_a_paragraph_and_keeps_its_style() {
        let mut body = body_from(&["firstsecond"]);
        body.paragraphs[0].style = Some("Body".to_owned());
        let caret = split_paragraph(&mut body, at(0, 5));
        assert_eq!(body.paragraphs.len(), 2);
        assert_eq!(body.paragraphs[0].text(), "first");
        assert_eq!(body.paragraphs[1].text(), "second");
        assert_eq!(body.paragraphs[1].style.as_deref(), Some("Body"));
        assert_eq!(
            caret.caret,
            Position {
                paragraph: 1,
                offset: 0
            }
        );
    }

    #[test]
    fn bold_toggles_on_and_then_off_for_the_same_range() {
        let mut body = body_from(&["plain prose"]);
        let selection = range((0, 0), (0, 5));
        assert!(toggle_direct(&mut body, selection, DirectFormat::Bold));
        assert!(body.paragraphs[0].runs[0].bold);
        assert_eq!(body.paragraphs[0].runs[0].text, "plain");

        assert!(all_direct(&body, selection, DirectFormat::Bold));
        assert!(toggle_direct(&mut body, selection, DirectFormat::Bold));
        assert!(!body.paragraphs[0].runs[0].bold);
    }

    #[test]
    fn direct_formatting_needs_a_selection() {
        let mut body = body_from(&["plain"]);
        assert!(!toggle_direct(&mut body, at(0, 2), DirectFormat::Bold));
        assert!(!apply_character_style(
            &mut body,
            at(0, 2),
            Some("Emphasis")
        ));
    }

    #[test]
    fn a_character_style_applies_only_to_the_selected_run() {
        let mut body = body_from(&["plain prose"]);
        let applied = apply_character_style(&mut body, range((0, 0), (0, 5)), Some("Emphasis"));
        assert!(applied);
        let styled: Vec<&TextRun> = body.paragraphs[0]
            .runs
            .iter()
            .filter(|run| run.character_style.as_deref() == Some("Emphasis"))
            .collect();
        assert_eq!(styled.len(), 1);
        assert_eq!(styled[0].text, "plain");
        assert_eq!(text(&body), "plain prose");
    }

    #[test]
    fn a_paragraph_style_applies_to_every_touched_paragraph() {
        let mut body = body_from(&["one", "two", "three"]);
        apply_paragraph_style(&mut body, range((0, 1), (1, 2)), Some("Body"));
        assert_eq!(body.paragraphs[0].style.as_deref(), Some("Body"));
        assert_eq!(body.paragraphs[1].style.as_deref(), Some("Body"));
        assert_eq!(body.paragraphs[2].style, None);
    }

    #[test]
    fn caret_movement_crosses_paragraph_boundaries() {
        let body = body_from(&["ab", "cd"]);
        let right = move_selection(&body, at(0, 2), Movement::Right, false);
        assert_eq!(
            right.caret,
            Position {
                paragraph: 1,
                offset: 0
            }
        );
        let left = move_selection(&body, right, Movement::Left, false);
        assert_eq!(
            left.caret,
            Position {
                paragraph: 0,
                offset: 2
            }
        );
    }

    #[test]
    fn extending_a_selection_keeps_its_anchor() {
        let body = body_from(&["abcd"]);
        let extended = move_selection(&body, at(0, 1), Movement::Right, true);
        assert_eq!(
            extended.anchor,
            Position {
                paragraph: 0,
                offset: 1
            }
        );
        assert_eq!(
            extended.caret,
            Position {
                paragraph: 0,
                offset: 2
            }
        );
        assert!(!extended.is_collapsed());
    }

    #[test]
    fn offsets_are_characters_not_bytes() {
        let mut body = body_from(&["héllo"]);
        insert(&mut body, at(0, 0), "→ ");
        assert_eq!(text(&body), "→ héllo");
        assert_eq!(body.word_count(), 2);
    }
}
