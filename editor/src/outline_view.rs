//! Keyboard-friendly outline surface over the story graph.

use gpui::{Context, EventEmitter, KeyDownEvent, Window, div, prelude::*, px, rgb};
use weave_core::ast::Document;

use crate::outline::{Outline, OutlineRowKind};
use crate::theme::DARK_THEME;

/// Request produced by the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlineViewEvent {
    /// Select the graph node that owns this row.
    SelectNode(String),
    /// Open the knot owning this row.
    OpenKnot(String),
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
    pub fn handle_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        match key {
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
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let _ = this.outline.select_node(&node);
                        this.open_selected(cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(rgb(DARK_THEME.text))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
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
                match event.keystroke.key.as_ref() {
                    "down" | "arrowdown" => this.move_selection(true, cx),
                    "up" | "arrowup" => this.move_selection(false, cx),
                    "enter" => this.open_selected(cx),
                    _ => {}
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
}
