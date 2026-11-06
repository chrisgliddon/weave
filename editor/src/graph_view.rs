//! Native `gpui-flow` surface for the editor graph workspace.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use gpui::{App, Context, Entity, EventEmitter, KeyDownEvent, Window, div, prelude::*, px, rgb};
use gpui_flow::{BackgroundPattern, Connection, Controls, FlowGraph, FlowNode, FlowState, Minimap};
use weave_core::ast::Document;

use crate::graph::{
    GraphDocument, GraphNodeKind, GraphPoint, NavigationDirection, directional_flow_node,
    is_valid_flow_connection,
};
use crate::node_renderers::{
    InspectorData, NodePresentation, install_weave_renderers, presentations,
};
use crate::sync::GraphEdit;
use crate::theme::DARK_THEME;

/// What the inline editor writes when the author presses Enter.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditTarget {
    /// Replace the first prose line of a knot.
    Prose { knot: String },
    /// Rename a knot.
    Title(String),
    /// Replace the label of an existing choice.
    ChoiceLabel {
        knot: String,
        path: Vec<usize>,
        node: String,
    },
    /// Append a new choice to a knot.
    NewChoice { knot: String },
}

/// Focused, single-line editor shown over the canvas.
#[derive(Debug, Clone)]
struct InlineEdit {
    target: EditTarget,
    text: String,
    cursor: usize,
}

impl InlineEdit {
    fn new(target: EditTarget, initial: String) -> Self {
        let cursor = initial.len();
        Self {
            target,
            text: initial,
            cursor,
        }
    }

    fn insert(&mut self, character: char) {
        self.text.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    fn backspace(&mut self) {
        if let Some((index, _)) = self.text[..self.cursor].char_indices().next_back() {
            self.text.remove(index);
            self.cursor = index;
        }
    }

    fn move_left(&mut self) {
        if let Some((index, _)) = self.text[..self.cursor].char_indices().next_back() {
            self.cursor = index;
        }
    }

    fn move_right(&mut self) {
        if let Some(character) = self.text[self.cursor..].chars().next() {
            self.cursor += character.len_utf8();
        }
    }

    fn rendered(&self) -> String {
        format!(
            "{}▏{}",
            &self.text[..self.cursor],
            &self.text[self.cursor..]
        )
    }

    fn prompt(&self) -> &'static str {
        match self.target {
            EditTarget::Prose { .. } => "Beat prose",
            EditTarget::Title(_) => "Knot name",
            EditTarget::ChoiceLabel { .. } => "Choice label",
            EditTarget::NewChoice { .. } => "New branch label",
        }
    }
}

/// Complete interactive graph canvas and its shared flow state.
pub struct GraphSurface {
    state: Entity<FlowState>,
    flow: Entity<FlowGraph>,
    minimap: Entity<Minimap>,
    controls: Entity<Controls>,
    presentations: Arc<HashMap<String, NodePresentation>>,
    next_user_node: u64,
    pending_connections: Arc<Mutex<Vec<Connection>>>,
    inline: Option<InlineEdit>,
}

/// Semantic edit request or actionable graph feedback for the owning shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphSurfaceEvent {
    Edit(GraphEdit),
    Unavailable(String),
}

impl GraphSurface {
    /// Create a graph surface from the canonical parsed document.
    pub fn new(document: &Document, cx: &mut Context<Self>) -> Self {
        Self::new_with_layout(document, &BTreeMap::new(), cx)
    }

    /// Create a graph while restoring stable editor-owned node positions.
    pub fn new_with_layout(
        document: &Document,
        layout: &BTreeMap<String, GraphPoint>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut graph = GraphDocument::from_ast(document);
        graph.apply_positions(layout);
        let presentations = Arc::new(presentations(&graph));
        let (nodes, edges) = graph.flow_parts();
        let next_user_node = nodes.len() as u64;
        let state = cx.new(|_| {
            let mut state = FlowState::new(nodes, edges);
            state.min_zoom = 0.2;
            state.max_zoom = 4.0;
            state.snap_to_grid = true;
            state.snap_grid = (20.0, 20.0);
            state
        });
        let renderer_presentations = Arc::clone(&presentations);
        let pending_connections: Arc<Mutex<Vec<Connection>>> = Arc::new(Mutex::new(Vec::new()));
        let connection_queue = Arc::clone(&pending_connections);
        let flow = cx.new(|cx| {
            install_weave_renderers(
                FlowGraph::new(state.clone(), cx)
                    .bg_color(DARK_THEME.workspace)
                    .grid_color(DARK_THEME.border)
                    .bg_pattern(BackgroundPattern::Dots)
                    .node_bg_color(DARK_THEME.panel)
                    .node_border_color(DARK_THEME.border)
                    .validate_connection(|connection, state| {
                        is_valid_flow_connection(connection, &state.nodes)
                    })
                    .on_connect(move |connection, _| {
                        if let Ok(mut queue) = connection_queue.lock() {
                            queue.push(connection.clone());
                        }
                    }),
                renderer_presentations,
            )
        });
        let minimap = cx.new(|_| Minimap::new(state.clone()).container_bounds(1_000.0, 650.0));
        let controls = cx.new(|_| Controls::new(state.clone()).container_size(1_000.0, 650.0));
        Self {
            state,
            flow,
            minimap,
            controls,
            presentations,
            next_user_node,
            pending_connections,
            inline: None,
        }
    }

    /// Shared `gpui-flow` state used by synchronization and inspector panels.
    #[must_use]
    pub fn state(&self) -> Entity<FlowState> {
        self.state.clone()
    }

    /// Current stable node positions for canonical project metadata.
    #[must_use]
    pub fn layout(&self, cx: &App) -> BTreeMap<String, GraphPoint> {
        self.state
            .read(cx)
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.to_string(),
                    GraphPoint::new(node.position.x, node.position.y),
                )
            })
            .collect()
    }

    /// Select one graph node requested by another editor panel.
    pub fn select_node(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let mut found = false;
        self.state.update(cx, |state, _| {
            for node in &mut state.nodes {
                node.selected = node.id.as_ref() == id;
                found |= node.selected;
            }
        });
        if found {
            cx.notify();
        }
        found
    }

    /// Undo one canvas-only interaction such as a move or connection gesture.
    pub fn undo(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.state.update(cx, |state, _| state.undo());
        if changed {
            cx.notify();
        }
        changed
    }

    /// Redo one canvas-only interaction.
    pub fn redo(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.state.update(cx, |state, _| state.redo());
        if changed {
            cx.notify();
        }
        changed
    }

    /// Current selected-node details for the inspector panel.
    #[must_use]
    pub fn inspector_data(&self, cx: &App) -> Option<InspectorData> {
        let state = self.state.read(cx);
        let node = state.nodes.iter().find(|node| node.selected)?;
        let id = node.id.to_string();
        if let Some(presentation) = self.presentations.get(&id) {
            return Some(InspectorData {
                id: presentation.id.clone(),
                kind: presentation.kind,
                title: presentation.title.clone(),
                preview: presentation.preview.clone(),
                position: GraphPoint::new(node.position.x, node.position.y),
                source_span: presentation.source_span,
                validation: presentation.validation.clone(),
            });
        }
        Some(InspectorData {
            id: node.id.to_string(),
            kind: kind_from_flow(node),
            title: if node.label.is_empty() {
                node.id.to_string()
            } else {
                node.label.to_string()
            },
            preview: "New graph construct".to_owned(),
            position: GraphPoint::new(node.position.x, node.position.y),
            source_span: None,
            validation: Some("Complete this construct before saving".to_owned()),
        })
    }

    fn selected_node(&self, cx: &App) -> Option<FlowNode> {
        self.state
            .read(cx)
            .nodes
            .iter()
            .find(|node| node.selected)
            .cloned()
    }

    fn add_knot(&mut self, cx: &mut Context<Self>) {
        self.next_user_node += 1;
        let preferred_name = format!("beat_{}", self.next_user_node);
        cx.emit(GraphSurfaceEvent::Edit(GraphEdit::AddKnot {
            preferred_name,
        }));
    }

    fn add_choice(&mut self, cx: &mut Context<Self>) {
        let knots = self.selected_knot_names(cx);
        let Some(knot) = knots.first().cloned() else {
            cx.emit(GraphSurfaceEvent::Unavailable(
                "Select one knot before adding a branch".to_owned(),
            ));
            return;
        };
        self.inline = Some(InlineEdit::new(
            EditTarget::NewChoice { knot },
            String::new(),
        ));
        cx.notify();
    }

    fn edit_selected(&mut self, rename: bool, cx: &mut Context<Self>) {
        let Some(node) = self.selected_node(cx) else {
            cx.emit(GraphSurfaceEvent::Unavailable(
                "Select a card before editing it".to_owned(),
            ));
            return;
        };
        let id = node.id.to_string();
        let Some(presentation) = self.presentations.get(&id) else {
            cx.emit(GraphSurfaceEvent::Unavailable(
                "This card cannot be edited yet".to_owned(),
            ));
            return;
        };
        let target = match presentation.kind {
            GraphNodeKind::Knot if rename => EditTarget::Title(presentation.title.clone()),
            GraphNodeKind::Knot => EditTarget::Prose {
                knot: presentation.title.clone(),
            },
            GraphNodeKind::Choice => {
                let Some((knot, path)) = choice_coordinate(&id) else {
                    cx.emit(GraphSurfaceEvent::Unavailable(
                        "This choice cannot be edited from the graph".to_owned(),
                    ));
                    return;
                };
                EditTarget::ChoiceLabel {
                    knot,
                    path,
                    node: id,
                }
            }
            _ => {
                cx.emit(GraphSurfaceEvent::Unavailable(
                    "Only beat and branch cards are editable".to_owned(),
                ));
                return;
            }
        };
        let initial = match &target {
            EditTarget::Prose { .. } => presentation.prose.clone().unwrap_or_default(),
            EditTarget::Title(_) => presentation.title.clone(),
            EditTarget::ChoiceLabel { .. } | EditTarget::NewChoice { .. } => {
                presentation.title.clone()
            }
        };
        self.inline = Some(InlineEdit::new(target, initial));
        cx.notify();
    }

    fn remove_selected(&mut self, cx: &mut Context<Self>) {
        let Some(node) = self.selected_node(cx) else {
            cx.emit(GraphSurfaceEvent::Unavailable(
                "Select a card before removing it".to_owned(),
            ));
            return;
        };
        let id = node.id.to_string();
        if let Some(knot) = id.strip_prefix("knot:") {
            cx.emit(GraphSurfaceEvent::Edit(GraphEdit::RemoveKnot {
                knot: knot.to_owned(),
            }));
        } else if let Some((knot, path)) = choice_coordinate(&id) {
            cx.emit(GraphSurfaceEvent::Edit(GraphEdit::RemoveChoice {
                knot,
                path,
            }));
        } else {
            cx.emit(GraphSurfaceEvent::Unavailable(
                "This card cannot be removed from the graph".to_owned(),
            ));
        }
    }

    fn edit_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_ref();
        match key {
            "escape" => {
                self.inline = None;
                cx.notify();
            }
            "enter" => self.commit_edit(cx),
            "backspace" => {
                if let Some(edit) = &mut self.inline {
                    edit.backspace();
                }
                cx.notify();
            }
            "left" => {
                if let Some(edit) = &mut self.inline {
                    edit.move_left();
                }
                cx.notify();
            }
            "right" => {
                if let Some(edit) = &mut self.inline {
                    edit.move_right();
                }
                cx.notify();
            }
            _ => {
                if event.keystroke.modifiers.control
                    || event.keystroke.modifiers.platform
                    || event.keystroke.modifiers.alt
                {
                    return;
                }
                if let Some(character) = &event.keystroke.key_char {
                    if let Some(edit) = &mut self.inline {
                        for character in character.chars().filter(|value| !value.is_control()) {
                            edit.insert(character);
                        }
                    }
                    cx.notify();
                }
            }
        }
    }

    fn commit_edit(&mut self, cx: &mut Context<Self>) {
        let Some(current) = self.inline.as_ref() else {
            return;
        };
        let text = current.text.trim().to_owned();
        if text.is_empty() && matches!(current.target, EditTarget::NewChoice { .. }) {
            return;
        }
        let Some(edit) = self.inline.take() else {
            return;
        };
        let graph_edit = match edit.target {
            EditTarget::Prose { knot } => GraphEdit::SetKnotText { knot, text },
            EditTarget::Title(knot) => GraphEdit::RenameKnot { knot, name: text },
            EditTarget::ChoiceLabel { knot, path, .. } => {
                GraphEdit::SetChoiceText { knot, path, text }
            }
            EditTarget::NewChoice { knot } => GraphEdit::AddChoice {
                knot,
                label: text,
                target: "END".to_owned(),
            },
        };
        cx.emit(GraphSurfaceEvent::Edit(graph_edit));
        cx.notify();
    }

    fn drain_connections(&mut self, cx: &mut Context<Self>) {
        let connections = {
            let Ok(mut queue) = self.pending_connections.lock() else {
                return;
            };
            std::mem::take(&mut *queue)
        };
        if connections.is_empty() {
            return;
        }
        let entity = cx.entity();
        for connection in connections {
            let Some(edit) = connection_edit(&connection) else {
                continue;
            };
            let entity = entity.clone();
            cx.defer(move |app| {
                entity.update(app, |_, cx| cx.emit(GraphSurfaceEvent::Edit(edit.clone())));
            });
        }
    }

    fn connect_selected_knots(&mut self, thread: bool, cx: &mut Context<Self>) {
        let knots = self.selected_knot_names(cx);
        if knots.len() != 2 {
            cx.emit(GraphSurfaceEvent::Unavailable(
                "Select exactly two knots to connect them".to_owned(),
            ));
            return;
        }
        cx.emit(GraphSurfaceEvent::Edit(GraphEdit::ConnectKnots {
            source: knots[0].clone(),
            target: knots[1].clone(),
            thread,
        }));
    }

    fn selected_knot_names(&self, cx: &App) -> Vec<String> {
        self.state
            .read(cx)
            .nodes
            .iter()
            .filter(|node| {
                node.selected
                    && node
                        .node_type
                        .as_ref()
                        .is_some_and(|kind| kind.as_ref() == "knot")
            })
            .map(|node| node.label.to_string())
            .collect()
    }

    fn navigate(&mut self, direction: NavigationDirection, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            let origin = state
                .nodes
                .iter()
                .find(|node| node.selected)
                .or_else(|| state.nodes.first())
                .map(|node| (node.id.clone(), node.position));
            let Some((origin_id, origin_position)) = origin else {
                return;
            };
            let next = state
                .nodes
                .iter()
                .filter(|node| node.id != origin_id)
                .filter_map(|node| {
                    directional_flow_node(origin_position, node.position, direction)
                        .map(|score| (score, node.id.clone()))
                })
                .min_by(|left, right| left.0.total_cmp(&right.0))
                .map(|(_, id)| id);
            if let Some(next) = next {
                for node in &mut state.nodes {
                    node.selected = node.id == next;
                }
            }
        });
        cx.notify();
    }
}

impl EventEmitter<GraphSurfaceEvent> for GraphSurface {}

impl gpui::Render for GraphSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drain_connections(cx);
        let (nodes, edges, zoom) = {
            let state = self.state.read(cx);
            (state.nodes.len(), state.edges.len(), state.viewport.zoom)
        };
        div()
            .id("weave-graph-surface")
            .key_context("WeaveGraph")
            .size_full()
            .relative()
            .overflow_hidden()
            .child(self.flow.clone())
            .child(
                div()
                    .absolute()
                    .top(px(14.0))
                    .left(px(14.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("add-knot")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(DARK_THEME.border))
                            .bg(rgb(DARK_THEME.chrome))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(DARK_THEME.accent)))
                            .on_click(cx.listener(|this, _, _, cx| this.add_knot(cx)))
                            .child("+ Beat"),
                    )
                    .child(
                        div()
                            .id("add-choice")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(DARK_THEME.border))
                            .bg(rgb(DARK_THEME.chrome))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(DARK_THEME.accent)))
                            .on_click(cx.listener(|this, _, _, cx| this.add_choice(cx)))
                            .child("+ Branch"),
                    )
                    .child(
                        div()
                            .id("edit-node")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(DARK_THEME.border))
                            .bg(rgb(DARK_THEME.chrome))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(DARK_THEME.accent)))
                            .on_click(cx.listener(|this, _, _, cx| this.edit_selected(false, cx)))
                            .child("Edit"),
                    )
                    .child(
                        div()
                            .id("delete-node")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(DARK_THEME.border))
                            .bg(rgb(DARK_THEME.chrome))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(DARK_THEME.accent)))
                            .on_click(cx.listener(|this, _, _, cx| this.remove_selected(cx)))
                            .child("Delete"),
                    )
                    .child(
                        div()
                            .id("connect-knots")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(DARK_THEME.border))
                            .bg(rgb(DARK_THEME.chrome))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(DARK_THEME.accent)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.connect_selected_knots(false, cx);
                            }))
                            .child("Link knots"),
                    )
                    .child(
                        div()
                            .id("thread-knots")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(DARK_THEME.border))
                            .bg(rgb(DARK_THEME.chrome))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(DARK_THEME.accent)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.connect_selected_knots(true, cx);
                            }))
                            .child("Thread knots"),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(DARK_THEME.chrome))
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child("Drag to pan · ⌘/Ctrl-scroll to zoom · Shift-drag to select"),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(14.0))
                    .right(px(14.0))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.chrome))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!(
                        "{nodes} nodes · {edges} edges · {:.0}%",
                        zoom * 100.0
                    )),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(14.0))
                    .left(px(14.0))
                    .child(self.controls.clone()),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(14.0))
                    .right(px(14.0))
                    .child(self.minimap.clone()),
            )
            .when_some(self.inline.clone(), |this, edit| {
                this.child(
                    div()
                        .absolute()
                        .top(px(58.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .w(px(560.0))
                                .px_4()
                                .py_3()
                                .rounded_md()
                                .bg(rgb(DARK_THEME.panel))
                                .border_1()
                                .border_color(rgb(DARK_THEME.accent))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(DARK_THEME.muted_text))
                                        .child(edit.prompt()),
                                )
                                .child(div().text_sm().child(edit.rendered()))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(DARK_THEME.muted_text))
                                        .child("Enter to save · Esc to cancel"),
                                ),
                        ),
                )
            })
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.inline.is_some() {
                    this.edit_key(event, cx);
                    return;
                }
                if event.keystroke.modifiers.shift && event.keystroke.key == "enter" {
                    this.add_knot(cx);
                    return;
                }
                match event.keystroke.key.as_ref() {
                    "enter" => this.edit_selected(false, cx),
                    "f2" => this.edit_selected(true, cx),
                    "tab" => this.add_choice(cx),
                    "delete" | "backspace" => this.remove_selected(cx),
                    "escape" => {}
                    key => {
                        let direction = match key {
                            "left" | "arrowleft" => Some(NavigationDirection::Left),
                            "right" | "arrowright" => Some(NavigationDirection::Right),
                            "up" | "arrowup" => Some(NavigationDirection::Up),
                            "down" | "arrowdown" => Some(NavigationDirection::Down),
                            _ => None,
                        };
                        if let Some(direction) = direction {
                            this.navigate(direction, cx);
                        }
                    }
                }
            }))
    }
}

/// Split a choice node identifier into its owning knot and statement path.
fn choice_coordinate(node_id: &str) -> Option<(String, Vec<usize>)> {
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

/// Convert a connection drawn on the canvas into the source edit it represents.
fn connection_edit(connection: &Connection) -> Option<GraphEdit> {
    let source = connection.source.to_string();
    let target = connection.target.to_string();
    let target_knot = target.strip_prefix("knot:")?.to_owned();
    if let Some(source_knot) = source.strip_prefix("knot:") {
        return Some(GraphEdit::ConnectKnots {
            source: source_knot.to_owned(),
            target: target_knot,
            thread: false,
        });
    }
    let (knot, path) = choice_coordinate(&source)?;
    Some(GraphEdit::ConnectChoice {
        knot,
        path,
        target: target_knot,
    })
}

fn kind_from_flow(node: &FlowNode) -> GraphNodeKind {
    match node.node_type.as_ref().map(ToString::to_string).as_deref() {
        Some("choice") => GraphNodeKind::Choice,
        Some("grammar") => GraphNodeKind::Grammar,
        Some("pattern") => GraphNodeKind::Pattern,
        Some("variable") => GraphNodeKind::Variable,
        Some("knot") | Some(_) | None => GraphNodeKind::Knot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(source: &str, target: &str) -> Connection {
        Connection {
            source: source.to_owned().into(),
            target: target.to_owned().into(),
            source_handle: None,
            target_handle: None,
        }
    }

    #[test]
    fn choice_coordinates_split_knot_and_path() {
        assert_eq!(
            choice_coordinate("choice:start:0.1"),
            Some(("start".to_owned(), vec![0, 1]))
        );
        assert_eq!(choice_coordinate("choice:start:"), None);
        assert_eq!(choice_coordinate("choice::0"), None);
        assert_eq!(choice_coordinate("knot:start"), None);
    }

    #[test]
    fn drawn_connections_become_source_edits() {
        assert_eq!(
            connection_edit(&connection("knot:a", "knot:b")),
            Some(GraphEdit::ConnectKnots {
                source: "a".to_owned(),
                target: "b".to_owned(),
                thread: false,
            })
        );
        assert_eq!(
            connection_edit(&connection("choice:a:0.2", "knot:b")),
            Some(GraphEdit::ConnectChoice {
                knot: "a".to_owned(),
                path: vec![0, 2],
                target: "b".to_owned(),
            })
        );
        assert_eq!(connection_edit(&connection("grammar:g", "knot:b")), None);
        assert_eq!(connection_edit(&connection("knot:a", "choice:b:0")), None);
    }

    #[test]
    fn inline_editor_handles_unicode_cursor_moves() {
        let mut edit = InlineEdit::new(
            EditTarget::Prose {
                knot: "start".to_owned(),
            },
            "héllo".to_owned(),
        );
        edit.move_left();
        edit.backspace();
        assert_eq!(edit.text, "hélo");
        edit.move_right();
        edit.insert('!');
        assert_eq!(edit.text, "hélo!");
        assert_eq!(edit.rendered(), "hélo!▏");
    }
}
