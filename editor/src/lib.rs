//! Standalone GPUI application and testable editor-domain services.

pub mod alignment;
pub mod app;
pub mod assistance;
pub mod character;
pub mod character_authoring;
pub mod character_presentation;
pub mod domain;
pub mod expression;
pub mod graph;
pub mod graph_view;
pub mod health;
pub mod node_renderers;
pub mod pattern_browser;
pub mod preview;
pub mod project;
pub mod projections;
pub mod relationships;
pub mod state;
pub mod sync;
pub mod tabletop;
pub mod temporal_context;
pub mod text_editor;
pub mod text_view;
pub mod theme;

pub use alignment::AlignmentSession;
pub use app::{LaunchMode, StartupError, launch};
pub use assistance::{CharacterAssistanceSession, CharacterAssistanceSessionError};
pub use character::CharacterCorpusSession;
pub use character_authoring::{
    CHARACTER_AUTHORING_CONTROLS, CharacterAuthoringAccessibility, CharacterAuthoringControl,
    CharacterAuthoringKey, CharacterAuthoringKeyboardAction, CharacterAuthoringSession,
};
pub use character_presentation::CharacterPresentationSession;
pub use domain::{
    DomainError, DomainSession, ModuleExportInspection, ModuleInspection,
    builtin_pattern_definitions,
};
pub use expression::ExpressionSession;
pub use graph::{
    GraphConnectionError, GraphDocument, GraphEdge, GraphEdgeKind, GraphNode, GraphNodeKind,
    GraphPoint, GraphRect, GraphViewport, INTERACTIVE_FRAME_BUDGET_MS, NavigationDirection,
};
pub use graph_view::GraphSurface;
pub use health::{CharacterHealthNavigationLink, CharacterHealthSession};
pub use node_renderers::{
    EdgeVisualStyle, InspectorData, NodePresentation, NodeShape, NodeVisualState, NodeVisualStyle,
    edge_visual_style, kind_label, node_visual_style,
};
pub use pattern_browser::{
    PatternBrowserEvent, PatternBrowserModel, PatternBrowserSurface, PatternFilter, PatternOrigin,
    PatternPreview, PatternRecord,
};
pub use preview::{
    PreviewCompileDebouncer, PreviewEvent, PreviewIssue, PreviewSession, PreviewStatus,
    PreviewSurface, PreviewTranscript,
};
pub use project::{
    ConflictResolution, ConflictResult, ExternalChange, ExternalConflict, ProjectError,
    ProjectSave, ProjectSession, ProjectWatcher,
};
pub use projections::ProjectionSession;
pub use relationships::RelationshipSession;
pub use state::{
    CenterView, EditorCommand, EditorState, MenuDefinition, MenuEntry, PanelLayout, StatusMessage,
    menu_definitions,
};
pub use sync::{CanonicalProjectModel, GraphEdit, GraphSync, SyncConflict, TextSync};
pub use tabletop::{
    DungeonpunkCreationSession, DungeonpunkSeedLineage, FreehackCreationSession,
    FreehackSeedLineage, PlugAndPlayCreationSession, PlugAndPlaySeedLineage, TabletopEditorCatalog,
    TabletopPanelInspection,
};
pub use temporal_context::TemporalContextSession;
pub use text_editor::{SyntaxKind, SyntaxToken, TextBuffer, highlight_source};
pub use text_view::TextSurface;
pub use theme::{DARK_THEME, EditorTheme};

/// Source compiled when the editor opens without a project.
pub const WELCOME_SOURCE: &str = r#"=== start ===
Welcome to Weave.
-> END
"#;
