//! Keyboard-friendly outline surface over the story graph.

use gpui::{Context, EventEmitter, KeyDownEvent, Window, div, prelude::*, px, rgb};
use weave_core::ast::{Document, Statement};

use crate::outline::{Outline, OutlineRow, OutlineRowKind};
use crate::sync::GraphEdit;
use crate::theme::DARK_THEME;

/// Request produced by the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlineViewEvent {
    /// Select the graph node that owns this row.
    SelectNode(String),
    /// Open the knot owning this row.
    OpenKnot(String),
    /// Move the selected row earlier or later.
    Move { node_id: String, delta: i32 },
}

/// Structural edit produced by an `Alt+Shift` outline key.
///
/// This is a separate event so the existing [`OutlineViewEvent`] subscription keeps its meaning.
/// The shell subscribes to this event, or calls [`OutlineSurface::structural_edit`] directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlineStructuralEvent {
    /// Apply one canonical graph edit derived from the selected row.
    Edit(GraphEdit),
}

/// Indented story outline.
pub struct OutlineSurface {
    outline: Outline,
    document: Document,
}

impl Default for OutlineSurface {
    fn default() -> Self {
        Self::new(&Document::default())
    }
}

impl OutlineSurface {
    /// Build an outline over a parsed document.
    #[must_use]
    pub fn new(document: &Document) -> Self {
        Self {
            outline: Outline::build(document),
            document: document.clone(),
        }
    }

    /// Rebuild when the document changes, keeping the selection by node id.
    pub fn set_document(&mut self, document: &Document, cx: &mut Context<Self>) {
        if self.document == *document {
            return;
        }
        let selected = self.outline.selected_row().map(|row| row.node_id.clone());
        self.document = document.clone();
        self.outline = Outline::build(document);
        if let Some(node) = selected {
            let _ = self.outline.select_node(&node);
        }
        cx.notify();
    }

    fn move_selection(&mut self, next: bool, cx: &mut Context<Self>) {
        if next {
            self.outline.select_next();
        } else {
            self.outline.select_previous();
        }
        cx.notify();
    }

    /// Apply one navigation key. Returns false when the key is not an outline command.
    ///
    /// `Alt+Up` and `Alt+Down` keep the existing single-step moves: a knot row moves the beat in
    /// story order and a choice row swaps the branch with its neighbour. `Alt+Shift+Up` and
    /// `Alt+Shift+Down` emit a structural edit: a choice row moves the branch to the previous or
    /// next beat, and a knot row moves its first non-choice block earlier or later inside the
    /// knot. Call [`Self::handle_key_with_modifiers`] to receive the shift modifier.
    pub fn handle_key(&mut self, key: &str, alt: bool, cx: &mut Context<Self>) -> bool {
        self.handle_key_with_modifiers(key, alt, false, cx)
    }

    /// Apply one navigation key with explicit shift state.
    ///
    /// This is the entry point the shell must call for the `Alt+Shift` structural moves.
    pub fn handle_key_with_modifiers(
        &mut self,
        key: &str,
        alt: bool,
        shift: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "down" | "arrowdown" if alt && shift => {
                self.move_selected_structural(1, cx);
                true
            }
            "up" | "arrowup" if alt && shift => {
                self.move_selected_structural(-1, cx);
                true
            }
            "down" | "arrowdown" if alt => {
                self.move_selected(1, cx);
                true
            }
            "up" | "arrowup" if alt => {
                self.move_selected(-1, cx);
                true
            }
            "down" | "arrowdown" => {
                self.move_selection(true, cx);
                true
            }
            "up" | "arrowup" => {
                self.move_selection(false, cx);
                true
            }
            "enter" => {
                self.open_selected(cx);
                true
            }
            _ => false,
        }
    }

    fn move_selected(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(row) = self.outline.selected_row() else {
            return;
        };
        let node_id = row.node_id.clone();
        cx.emit(OutlineViewEvent::Move { node_id, delta });
        cx.notify();
    }

    /// Structural edit for the selected row and a `-1` or `1` step.
    ///
    /// A choice row moves its branch to the previous or next beat. A knot row moves its first
    /// non-choice block one place inside that knot. Returns `None` at the ends, with no selected
    /// row, or when the row has no movable block.
    #[must_use]
    pub fn structural_edit(&self, delta: i32) -> Option<GraphEdit> {
        let row = self.outline.selected_row()?;
        structural_edit(&self.document, row, delta)
    }

    fn move_selected_structural(&mut self, delta: i32, cx: &mut Context<Self>) {
        if let Some(edit) = self.structural_edit(delta) {
            cx.emit(OutlineStructuralEvent::Edit(edit));
        }
        cx.notify();
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.outline.selected_row() else {
            return;
        };
        let node = row.node_id.clone();
        let knot = knot_of_row(&node);
        cx.emit(OutlineViewEvent::SelectNode(node));
        if let Some(knot) = knot {
            cx.emit(OutlineViewEvent::OpenKnot(knot));
        }
        cx.notify();
    }
}

impl EventEmitter<OutlineViewEvent> for OutlineSurface {}
impl EventEmitter<OutlineStructuralEvent> for OutlineSurface {}

impl gpui::Render for OutlineSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self
            .outline
            .selected_row()
            .and_then(|row| self.outline.index_of_node(&row.node_id));
        let mut list = div().flex().flex_col().gap_0p5().p_3();
        for (index, row) in self.outline.rows().iter().enumerate() {
            let (title, detail) = match &row.kind {
                OutlineRowKind::Knot { prose, choices } => (
                    title_of(&row.node_id),
                    prose
                        .clone()
                        .unwrap_or_else(|| format!("{choices} branch(es)")),
                ),
                OutlineRowKind::Choice { label, target } => (
                    format!("↳ {label}"),
                    target.clone().unwrap_or_else(|| "END".to_owned()),
                ),
            };
            let node = row.node_id.clone();
            list = list.child(
                div()
                    .id(("outline-row", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl(px(if row.depth == 0 { 8.0 } else { 28.0 }))
                    .pr_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected == Some(index), |row| {
                        row.bg(rgb(DARK_THEME.accent))
                            .text_color(rgb(DARK_THEME.workspace))
                    })
                    .role(gpui::Role::TreeItem)
                    .aria_label(format!("{title} {detail}"))
                    .aria_selected(selected == Some(index))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let _ = this.outline.select_node(&node);
                        this.open_selected(cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(rgb(if selected == Some(index) {
                                DARK_THEME.workspace
                            } else {
                                DARK_THEME.text
                            }))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(if selected == Some(index) {
                                DARK_THEME.workspace
                            } else {
                                DARK_THEME.muted_text
                            }))
                            .child(detail),
                    ),
            );
        }
        div()
            .id("outline-surface")
            .key_context("WeaveOutline")
            .size_full()
            .overflow_hidden()
            .bg(rgb(DARK_THEME.workspace))
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child("Outline"),
            )
            .child(list)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = event.keystroke.key.clone();
                let alt = event.keystroke.modifiers.alt;
                let shift = event.keystroke.modifiers.shift;
                if this.handle_key_with_modifiers(&key, alt, shift, cx) {
                    cx.stop_propagation();
                }
            }))
    }
}

fn title_of(node_id: &str) -> String {
    node_id
        .strip_prefix("knot:")
        .map_or_else(|| node_id.to_owned(), str::to_owned)
}

fn knot_of_row(node_id: &str) -> Option<String> {
    if let Some(knot) = node_id.strip_prefix("knot:") {
        return Some(knot.to_owned());
    }
    let rest = node_id.strip_prefix("choice:")?;
    let (knot, _) = rest.split_once(':')?;
    Some(knot.to_owned())
}

/// Split a choice row identifier into its owning knot and statement path.
fn choice_of_row(node_id: &str) -> Option<(String, Vec<usize>)> {
    let rest = node_id.strip_prefix("choice:")?;
    let (knot, path) = rest.split_once(':')?;
    if knot.is_empty() {
        return None;
    }
    let path = path
        .split('.')
        .map(str::parse::<usize>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some((knot.to_owned(), path))
}

/// Build the structural edit for the selected outline row.
///
/// A choice row moves to the previous or next beat. A knot row moves its first non-choice block
/// one place inside the knot. The edit is `None` at the ends and for rows with nothing to move.
fn structural_edit(document: &Document, row: &OutlineRow, delta: i32) -> Option<GraphEdit> {
    if delta == 0 {
        return None;
    }
    match &row.kind {
        OutlineRowKind::Choice { .. } => {
            let (knot, path) = choice_of_row(&row.node_id)?;
            let target_knot = adjacent_knot(document, &knot, delta)?;
            Some(GraphEdit::MoveChoiceToKnot {
                knot,
                path,
                target_knot,
            })
        }
        OutlineRowKind::Knot { .. } => {
            let knot = row.node_id.strip_prefix("knot:")?.to_owned();
            let path = first_non_choice_block(document, &knot)?;
            Some(GraphEdit::MoveStatement {
                knot,
                path,
                delta,
                confirm_after_end: false,
            })
        }
    }
}

/// Name of the knot one position before or after `knot` in document order.
fn adjacent_knot(document: &Document, knot: &str, delta: i32) -> Option<String> {
    let names = document
        .knots()
        .map(|owner| owner.node.name.as_str())
        .collect::<Vec<_>>();
    let position = names.iter().position(|name| *name == knot)?;
    let target = position as i64 + i64::from(delta.signum());
    let target = usize::try_from(target).ok()?;
    names.get(target).map(|name| (*name).to_owned())
}

/// Path of the first non-choice statement in a knot body.
fn first_non_choice_block(document: &Document, knot: &str) -> Option<Vec<usize>> {
    let owner = document.knots().find(|owner| owner.node.name == knot)?;
    owner
        .node
        .body
        .iter()
        .position(|statement| !matches!(statement.node, Statement::Choice(_)))
        .map(|index| vec![index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_to_knots_and_choices() {
        assert_eq!(knot_of_row("knot:start").as_deref(), Some("start"));
        assert_eq!(knot_of_row("choice:start:0.1").as_deref(), Some("start"));
        assert_eq!(knot_of_row("grammar:names"), None);
        assert_eq!(title_of("knot:start"), "start");
    }

    #[test]
    fn rebuilding_keeps_selection_by_node_id() {
        let document =
            weave_core::parse_document("=== start ===\n* [Go] -> END\n=== next ===\nDone.\n")
                .expect("fixture parses");
        let mut surface = OutlineSurface::new(&document);
        assert!(surface.outline.select_node("knot:start"));
        let selected = surface
            .outline
            .selected_row()
            .map(|row| row.node_id.clone());
        assert_eq!(selected.as_deref(), Some("knot:start"));
    }

    const STRUCTURAL: &str = r#"=== start ===
Opening.
* [Go] -> next
A closing line.
=== next ===
Done.
=== last ===
* [Back] -> start
-> END
"#;

    fn structural_document() -> Document {
        weave_core::parse_document(STRUCTURAL).expect("fixture parses")
    }

    #[test]
    fn choice_rows_emit_cross_beat_moves() {
        let document = structural_document();
        let outline = Outline::build(&document);
        let next = outline.index_of_node("choice:start:1").expect("choice row");
        assert_eq!(
            structural_edit(&document, &outline.rows()[next], 1),
            Some(GraphEdit::MoveChoiceToKnot {
                knot: "start".to_owned(),
                path: vec![1],
                target_knot: "next".to_owned(),
            })
        );
        let back = outline.index_of_node("choice:last:0").expect("choice row");
        assert_eq!(
            structural_edit(&document, &outline.rows()[back], -1),
            Some(GraphEdit::MoveChoiceToKnot {
                knot: "last".to_owned(),
                path: vec![0],
                target_knot: "next".to_owned(),
            })
        );
    }

    #[test]
    fn knot_rows_emit_block_moves_for_their_first_non_choice_block() {
        let document = structural_document();
        let outline = Outline::build(&document);
        let start = outline.index_of_node("knot:start").expect("knot row");
        assert_eq!(
            structural_edit(&document, &outline.rows()[start], 1),
            Some(GraphEdit::MoveStatement {
                knot: "start".to_owned(),
                path: vec![0],
                delta: 1,
                confirm_after_end: false,
            })
        );
        let last = outline.index_of_node("knot:last").expect("knot row");
        assert_eq!(first_non_choice_block(&document, "last"), Some(vec![1]));
        assert_eq!(
            structural_edit(&document, &outline.rows()[last], -1),
            Some(GraphEdit::MoveStatement {
                knot: "last".to_owned(),
                path: vec![1],
                delta: -1,
                confirm_after_end: false,
            })
        );
    }

    #[test]
    fn structural_moves_stop_at_the_ends_and_ignore_zero_delta() {
        let document = structural_document();
        let outline = Outline::build(&document);
        let first_choice = outline.index_of_node("choice:start:1").expect("choice row");
        let last_choice = outline.index_of_node("choice:last:0").expect("choice row");
        assert_eq!(
            structural_edit(&document, &outline.rows()[first_choice], -1),
            None,
            "the first beat has no previous target"
        );
        assert_eq!(
            structural_edit(&document, &outline.rows()[last_choice], 1),
            None,
            "the last beat has no next target"
        );
        assert_eq!(
            structural_edit(&document, &outline.rows()[first_choice], 0),
            None
        );
    }

    #[test]
    fn choice_row_paths_split_into_knot_and_indices() {
        assert_eq!(
            choice_of_row("choice:start:1.2"),
            Some(("start".to_owned(), vec![1, 2]))
        );
        assert_eq!(choice_of_row("choice:start:"), None);
        assert_eq!(choice_of_row("knot:start"), None);
    }
}
