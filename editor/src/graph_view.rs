//! Native `gpui-flow` surface for the editor graph workspace.

use gpui::{
    AnyElement, App, Context, Entity, FontWeight, KeyDownEvent, Window, div, prelude::*, px, rgb,
};
use gpui_flow::{
    BackgroundPattern, Controls, FlowGraph, FlowNode, FlowState, HandleDef, HandlePosition, Minimap,
};
use weave_core::ast::Document;

use crate::graph::{
    GraphDocument, NavigationDirection, directional_flow_node, is_valid_flow_connection,
};
use crate::theme::DARK_THEME;

/// Complete interactive graph canvas and its shared flow state.
pub struct GraphSurface {
    state: Entity<FlowState>,
    flow: Entity<FlowGraph>,
    minimap: Entity<Minimap>,
    controls: Entity<Controls>,
    next_user_node: u64,
}

impl GraphSurface {
    /// Create a graph surface from the canonical parsed document.
    pub fn new(document: &Document, cx: &mut Context<Self>) -> Self {
        let graph = GraphDocument::from_ast(document);
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
        let flow = cx.new(|cx| {
            FlowGraph::new(state.clone(), cx)
                .bg_color(DARK_THEME.workspace)
                .grid_color(DARK_THEME.border)
                .bg_pattern(BackgroundPattern::Dots)
                .node_bg_color(DARK_THEME.panel)
                .node_border_color(DARK_THEME.border)
                .default_renderer(render_basic_node)
                .validate_connection(|connection, state| {
                    is_valid_flow_connection(connection, &state.nodes)
                })
        });
        let minimap = cx.new(|_| Minimap::new(state.clone()).container_bounds(1_000.0, 650.0));
        let controls = cx.new(|_| Controls::new(state.clone()).container_size(1_000.0, 650.0));
        Self {
            state,
            flow,
            minimap,
            controls,
            next_user_node,
        }
    }

    /// Shared `gpui-flow` state used by synchronization and inspector panels.
    #[must_use]
    pub fn state(&self) -> Entity<FlowState> {
        self.state.clone()
    }

    fn add_knot(&mut self, cx: &mut Context<Self>) {
        let id = format!("knot:new_{}", self.next_user_node);
        let label = format!("New knot {}", self.next_user_node + 1);
        self.next_user_node += 1;
        self.state.update(cx, |state, _| {
            state.push_undo();
            let center = state.viewport.screen_to_flow(500.0, 325.0);
            state.nodes.push(
                FlowNode::new(id, center.x, center.y)
                    .label(label)
                    .node_type("knot")
                    .size(220.0, 92.0)
                    .handles(vec![
                        HandleDef::target(HandlePosition::Left).id("in"),
                        HandleDef::source(HandlePosition::Right).id("out"),
                    ]),
            );
            state.rebuild_lookup();
        });
        cx.notify();
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

fn render_basic_node(node: &FlowNode, _window: &mut Window, _cx: &mut App) -> AnyElement {
    let kind = node
        .node_type
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "construct".to_owned());
    div()
        .w(px(188.0))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(DARK_THEME.muted_text))
                .child(kind.to_uppercase()),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(DARK_THEME.text))
                .child(if node.label.is_empty() {
                    node.id.to_string()
                } else {
                    node.label.to_string()
                }),
        )
        .into_any_element()
}
