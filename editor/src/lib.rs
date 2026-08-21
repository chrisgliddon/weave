//! Standalone GPUI application and testable editor-domain services.

pub mod app;
pub mod domain;
pub mod graph;
pub mod graph_view;
pub mod node_renderers;
pub mod state;
pub mod theme;

pub use app::{LaunchMode, StartupError, launch};
pub use domain::{DomainError, DomainSession, builtin_pattern_definitions};
pub use graph::{
    GraphConnectionError, GraphDocument, GraphEdge, GraphEdgeKind, GraphNode, GraphNodeKind,
    GraphPoint, GraphRect, GraphViewport, INTERACTIVE_FRAME_BUDGET_MS, NavigationDirection,
};
pub use graph_view::GraphSurface;
pub use node_renderers::{
    EdgeVisualStyle, InspectorData, NodePresentation, NodeShape, NodeVisualState, NodeVisualStyle,
    edge_visual_style, kind_label, node_visual_style,
};
pub use state::{
    CenterView, EditorCommand, EditorState, MenuDefinition, MenuEntry, PanelLayout, StatusMessage,
    menu_definitions,
};
pub use theme::{DARK_THEME, EditorTheme};

/// Source compiled when the editor opens without a project.
pub const WELCOME_SOURCE: &str = r#"=== start ===
Welcome to Weave.
-> END
"#;
