//! Standalone GPUI application and testable editor-domain services.

pub mod app;
pub mod domain;
pub mod state;
pub mod theme;

pub use app::{LaunchMode, StartupError, launch};
pub use domain::{DomainError, DomainSession, builtin_pattern_definitions};
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
