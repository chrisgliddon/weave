//! Specialized visual presentations for every Weave graph construct.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{AnyElement, App, FontWeight, Window, div, prelude::*, px, rgb};
use gpui_flow::{FlowGraph, FlowNode};
use weave_core::Span;

use crate::graph::{GraphDocument, GraphEdgeKind, GraphNodeKind, GraphPoint};
use crate::theme::DARK_THEME;

/// Shape language used by source construct cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    Card,
    Diamond,
    RoundedBox,
    Hexagon,
    Pill,
}

/// Interaction and validation state used to derive an accessible visual style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeVisualState {
    pub selected: bool,
    pub focused: bool,
    pub error: bool,
}

/// Resolved renderer-independent node style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeVisualStyle {
    pub shape: NodeShape,
    pub accent: u32,
    pub background: u32,
    pub border: u32,
    pub border_width: u8,
}

/// Resolved renderer-independent edge style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeVisualStyle {
    pub color: u32,
    pub width: f32,
    pub dotted: bool,
    pub label: &'static str,
}

/// Source-backed content shown by a graph node and inspector.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePresentation {
    pub id: String,
    pub kind: GraphNodeKind,
    pub title: String,
    pub preview: String,
    pub position: GraphPoint,
    pub source_span: Option<Span>,
    pub validation: Option<String>,
}

impl NodePresentation {
    /// Accessible description announced for this node.
    #[must_use]
    pub fn accessibility_label(&self) -> String {
        let mut label = format!("{} node: {}", kind_label(self.kind), self.title);
        if !self.preview.is_empty() {
            label.push_str(". ");
            label.push_str(&self.preview);
        }
        if let Some(validation) = &self.validation {
            label.push_str(". Error: ");
            label.push_str(validation);
        }
        label
    }
}

/// Presentation data for an inspector selection.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectorData {
    pub id: String,
    pub kind: GraphNodeKind,
    pub title: String,
    pub preview: String,
    pub position: GraphPoint,
    pub source_span: Option<Span>,
    pub validation: Option<String>,
}

/// Build stable presentation records from the graph model.
#[must_use]
pub fn presentations(graph: &GraphDocument) -> HashMap<String, NodePresentation> {
    graph
        .nodes()
        .iter()
        .map(|node| {
            (
                node.id.clone(),
                NodePresentation {
                    id: node.id.clone(),
                    kind: node.kind,
                    title: node.label.clone(),
                    preview: node.preview.clone(),
                    position: node.position,
                    source_span: node.source_span,
                    validation: node.validation.clone(),
                },
            )
        })
        .collect()
}

/// Resolve node visuals for default, selected, focused, and error states.
#[must_use]
pub const fn node_visual_style(kind: GraphNodeKind, state: NodeVisualState) -> NodeVisualStyle {
    let (shape, accent) = match kind {
        GraphNodeKind::Knot => (NodeShape::Card, 0x5b8cff),
        GraphNodeKind::Choice => (NodeShape::Diamond, 0xf4b860),
        GraphNodeKind::Grammar => (NodeShape::RoundedBox, 0x3ecf8e),
        GraphNodeKind::Pattern => (NodeShape::Hexagon, 0xb481ff),
        GraphNodeKind::Variable => (NodeShape::Pill, 0x4cc9f0),
    };
    let border = if state.error {
        0xff5d73
    } else if state.focused || state.selected {
        DARK_THEME.accent
    } else {
        DARK_THEME.border
    };
    NodeVisualStyle {
        shape,
        accent,
        background: if state.error {
            DARK_THEME.error
        } else {
            DARK_THEME.panel
        },
        border,
        border_width: if state.focused {
            3
        } else if state.selected || state.error {
            2
        } else {
            1
        },
    }
}

/// Resolve visuals for choice, divert, and thread edges.
#[must_use]
pub const fn edge_visual_style(kind: GraphEdgeKind) -> EdgeVisualStyle {
    match kind {
        GraphEdgeKind::Choice => EdgeVisualStyle {
            color: 0x7c9cff,
            width: 2.0,
            dotted: false,
            label: "choice",
        },
        GraphEdgeKind::Divert => EdgeVisualStyle {
            color: 0x99a3b5,
            width: 2.0,
            dotted: false,
            label: "divert",
        },
        GraphEdgeKind::Thread => EdgeVisualStyle {
            color: 0xb481ff,
            width: 1.0,
            dotted: true,
            label: "thread",
        },
    }
}

/// Register all specialized node renderers on a `gpui-flow` graph.
#[must_use]
pub fn install_weave_renderers(
    graph: FlowGraph,
    presentations: Arc<HashMap<String, NodePresentation>>,
) -> FlowGraph {
    let knot = Arc::clone(&presentations);
    let choice = Arc::clone(&presentations);
    let grammar = Arc::clone(&presentations);
    let pattern = Arc::clone(&presentations);
    let variable = presentations;
    graph
        .no_node_chrome()
        .node_renderer("knot", move |node, window, cx| {
            render_weave_node(node, GraphNodeKind::Knot, &knot, window, cx)
        })
        .node_renderer("choice", move |node, window, cx| {
            render_weave_node(node, GraphNodeKind::Choice, &choice, window, cx)
        })
        .node_renderer("grammar", move |node, window, cx| {
            render_weave_node(node, GraphNodeKind::Grammar, &grammar, window, cx)
        })
        .node_renderer("pattern", move |node, window, cx| {
            render_weave_node(node, GraphNodeKind::Pattern, &pattern, window, cx)
        })
        .node_renderer("variable", move |node, window, cx| {
            render_weave_node(node, GraphNodeKind::Variable, &variable, window, cx)
        })
}

fn render_weave_node(
    node: &FlowNode,
    kind: GraphNodeKind,
    presentations: &HashMap<String, NodePresentation>,
    _window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    let id = node.id.to_string();
    let presentation = presentations.get(&id);
    let title = presentation.map_or_else(
        || {
            if node.label.is_empty() {
                id.clone()
            } else {
                node.label.to_string()
            }
        },
        |presentation| presentation.title.clone(),
    );
    let preview = presentation.map_or("", |presentation| presentation.preview.as_str());
    let validation = presentation.and_then(|presentation| presentation.validation.as_deref());
    let visual = node_visual_style(
        kind,
        NodeVisualState {
            selected: node.selected,
            focused: node.dragging,
            error: validation.is_some(),
        },
    );
    let icon = match visual.shape {
        NodeShape::Card => "▤",
        NodeShape::Diamond => "◆",
        NodeShape::RoundedBox => "⌁",
        NodeShape::Hexagon => "⬡",
        NodeShape::Pill => "●",
    };
    let mut card = div()
        .w(px(if kind == GraphNodeKind::Variable {
            176.0
        } else {
            220.0
        }))
        .min_h(px(if kind == GraphNodeKind::Variable {
            54.0
        } else {
            78.0
        }))
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .bg(rgb(visual.background))
        .border_color(rgb(visual.border))
        .when(visual.border_width == 1, |card| card.border_1())
        .when(visual.border_width == 2, |card| card.border_2())
        .when(visual.border_width == 3, |card| card.border_3())
        .when(kind == GraphNodeKind::Variable, |card| card.rounded_full())
        .when(kind != GraphNodeKind::Variable, |card| card.rounded_lg())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_color(rgb(visual.accent))
                        .font_weight(FontWeight::BOLD)
                        .child(icon),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(visual.accent))
                        .child(kind_label(kind).to_uppercase()),
                ),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(DARK_THEME.text))
                .child(title),
        );
    if !preview.is_empty() && kind != GraphNodeKind::Variable {
        card = card.child(
            div()
                .max_w(px(190.0))
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(preview.to_owned()),
        );
    }
    if let Some(validation) = validation {
        card = card.child(
            div()
                .mt_1()
                .text_xs()
                .text_color(rgb(0xffb4c0))
                .child(validation.to_owned()),
        );
    }
    card.into_any_element()
}

/// Human-readable construct kind.
#[must_use]
pub const fn kind_label(kind: GraphNodeKind) -> &'static str {
    match kind {
        GraphNodeKind::Knot => "Knot",
        GraphNodeKind::Choice => "Choice",
        GraphNodeKind::Grammar => "Grammar",
        GraphNodeKind::Pattern => "Pattern",
        GraphNodeKind::Variable => "Variable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(kind: GraphNodeKind, state: NodeVisualState) -> String {
        let style = node_visual_style(kind, state);
        format!(
            "{kind:?}|{:?}|#{:06x}|#{:06x}|{}",
            style.shape, style.accent, style.border, style.border_width
        )
    }

    #[test]
    fn every_node_renderer_has_stable_default_selected_focused_and_error_snapshots() {
        let kinds = [
            GraphNodeKind::Knot,
            GraphNodeKind::Choice,
            GraphNodeKind::Grammar,
            GraphNodeKind::Pattern,
            GraphNodeKind::Variable,
        ];
        let snapshots = kinds
            .into_iter()
            .flat_map(|kind| {
                [
                    NodeVisualState::default(),
                    NodeVisualState {
                        selected: true,
                        ..Default::default()
                    },
                    NodeVisualState {
                        focused: true,
                        ..Default::default()
                    },
                    NodeVisualState {
                        error: true,
                        ..Default::default()
                    },
                ]
                .map(|state| snapshot(kind, state))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            snapshots,
            [
                "Knot|Card|#5b8cff|#2a3242|1",
                "Knot|Card|#5b8cff|#7c9cff|2",
                "Knot|Card|#5b8cff|#7c9cff|3",
                "Knot|Card|#5b8cff|#ff5d73|2",
                "Choice|Diamond|#f4b860|#2a3242|1",
                "Choice|Diamond|#f4b860|#7c9cff|2",
                "Choice|Diamond|#f4b860|#7c9cff|3",
                "Choice|Diamond|#f4b860|#ff5d73|2",
                "Grammar|RoundedBox|#3ecf8e|#2a3242|1",
                "Grammar|RoundedBox|#3ecf8e|#7c9cff|2",
                "Grammar|RoundedBox|#3ecf8e|#7c9cff|3",
                "Grammar|RoundedBox|#3ecf8e|#ff5d73|2",
                "Pattern|Hexagon|#b481ff|#2a3242|1",
                "Pattern|Hexagon|#b481ff|#7c9cff|2",
                "Pattern|Hexagon|#b481ff|#7c9cff|3",
                "Pattern|Hexagon|#b481ff|#ff5d73|2",
                "Variable|Pill|#4cc9f0|#2a3242|1",
                "Variable|Pill|#4cc9f0|#7c9cff|2",
                "Variable|Pill|#4cc9f0|#7c9cff|3",
                "Variable|Pill|#4cc9f0|#ff5d73|2",
            ]
        );
    }

    #[test]
    fn divert_and_thread_edge_snapshots_are_distinct() {
        assert_eq!(
            edge_visual_style(GraphEdgeKind::Divert),
            EdgeVisualStyle {
                color: 0x99a3b5,
                width: 2.0,
                dotted: false,
                label: "divert",
            }
        );
        assert!(edge_visual_style(GraphEdgeKind::Thread).dotted);
        assert_ne!(
            edge_visual_style(GraphEdgeKind::Choice).color,
            edge_visual_style(GraphEdgeKind::Thread).color
        );
    }

    #[test]
    fn presentations_include_accessible_validation_context() {
        let presentation = NodePresentation {
            id: "knot:new".to_owned(),
            kind: GraphNodeKind::Knot,
            title: "New knot".to_owned(),
            preview: "Empty narrative block".to_owned(),
            position: GraphPoint::default(),
            source_span: None,
            validation: Some("Name this knot".to_owned()),
        };
        assert_eq!(
            presentation.accessibility_label(),
            "Knot node: New knot. Empty narrative block. Error: Name this knot"
        );
    }
}
