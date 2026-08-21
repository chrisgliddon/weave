//! Native `gpui-flow` surface for the editor graph workspace.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use gpui::{App, Context, Entity, EventEmitter, KeyDownEvent, Window, div, prelude::*, px, rgb};
use gpui_flow::{BackgroundPattern, Controls, FlowGraph, FlowNode, FlowState, Minimap};
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

/// Complete interactive graph canvas and its shared flow state.
pub struct GraphSurface {
    state: Entity<FlowState>,
    flow: Entity<FlowGraph>,
    minimap: Entity<Minimap>,
    controls: Entity<Controls>,
    presentations: Arc<HashMap<String, NodePresentation>>,
    next_user_node: u64,
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

    fn add_knot(&mut self, cx: &mut Context<Self>) {
        let preferred_name = format!("new_knot_{}", self.next_user_node + 1);
        self.next_user_node += 1;
        cx.emit(GraphSurfaceEvent::Edit(GraphEdit::AddKnot {
            preferred_name,
        }));
    }

    fn add_choice(&mut self, cx: &mut Context<Self>) {
        let knots = self.selected_knot_names(cx);
        let Some(knot) = knots.first() else {
            cx.emit(GraphSurfaceEvent::Unavailable(
                "Select one knot before adding a choice".to_owned(),
            ));
            return;
        };
        let target = self
            .state
            .read(cx)
            .nodes
            .iter()
            .find(|node| {
                node.node_type
                    .as_ref()
                    .is_some_and(|kind| kind.as_ref() == "knot")
                    && node.label.as_ref() != knot
            })
            .map_or_else(|| "END".to_owned(), |node| node.label.to_string());
        cx.emit(GraphSurfaceEvent::Edit(GraphEdit::AddChoice {
            knot: knot.clone(),
            label: "New choice".to_owned(),
            target,
        }));
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
                            .child("+ Knot"),
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
                            .child("+ Choice"),
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
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let direction = match event.keystroke.key.as_ref() {
                    "left" | "arrowleft" => Some(NavigationDirection::Left),
                    "right" | "arrowright" => Some(NavigationDirection::Right),
                    "up" | "arrowup" => Some(NavigationDirection::Up),
                    "down" | "arrowdown" => Some(NavigationDirection::Down),
                    _ => None,
                };
                if let Some(direction) = direction {
                    this.navigate(direction, cx);
                }
            }))
    }
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
