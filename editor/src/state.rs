//! Testable application state, commands, menus, and panel layout.

/// Primary workspace view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CenterView {
    /// Visual narrative graph.
    Graph,
    /// Raw `.weave` source.
    Text,
}

impl CenterView {
    /// Human-readable tab label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Graph => "Graph",
            Self::Text => "Text",
        }
    }
}

/// Persistent panel arrangement.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelLayout {
    /// Whether the project sidebar is visible.
    pub project_sidebar: bool,
    /// Whether the inspector sidebar is visible.
    pub inspector_sidebar: bool,
    /// Whether the pattern browser is visible.
    pub pattern_browser: bool,
    /// Whether the preview panel is visible.
    pub play_preview: bool,
    /// Active center view.
    pub center: CenterView,
    /// Left sidebar width in logical pixels.
    pub left_width: f32,
    /// Right sidebar width in logical pixels.
    pub right_width: f32,
    /// Bottom panel height in logical pixels.
    pub bottom_height: f32,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            project_sidebar: true,
            inspector_sidebar: true,
            pattern_browser: false,
            play_preview: true,
            center: CenterView::Graph,
            left_width: 240.0,
            right_width: 280.0,
            bottom_height: 220.0,
        }
    }
}

/// Commands understood by the editor state reducer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommand {
    /// Create an empty project.
    NewProject,
    /// Open an existing project.
    OpenProject,
    /// Save the current source file.
    Save,
    /// Undo the active view's last edit.
    Undo,
    /// Redo the active view's last edit.
    Redo,
    /// Select the graph view.
    ShowGraph,
    /// Select the text view.
    ShowText,
    /// Toggle the project sidebar.
    ToggleProject,
    /// Toggle the inspector sidebar.
    ToggleInspector,
    /// Toggle the pattern browser.
    TogglePatterns,
    /// Toggle the live preview.
    TogglePreview,
    /// Compile current source.
    Compile,
    /// Start or continue preview playback.
    Run,
    /// Stop preview playback.
    Stop,
}

/// User-facing state of the last operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusMessage {
    /// Editor is ready for input.
    Ready,
    /// Informational operation result.
    Info(String),
    /// Recoverable operation failure.
    Error(String),
}

impl StatusMessage {
    /// Render concise status text.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Ready => "Ready",
            Self::Info(message) | Self::Error(message) => message,
        }
    }

    /// Whether this status represents a recoverable failure.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// Core application state independent of GPUI entity ownership.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorState {
    /// Display name for the active project.
    pub project_name: String,
    /// Active layout.
    pub layout: PanelLayout,
    /// Whether source differs from its saved form.
    pub dirty: bool,
    /// Last visible operation status.
    pub status: StatusMessage,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            project_name: "Untitled".to_owned(),
            layout: PanelLayout::default(),
            dirty: false,
            status: StatusMessage::Ready,
        }
    }
}

impl EditorState {
    /// Apply one command to shell state.
    pub fn apply(&mut self, command: EditorCommand) {
        match command {
            EditorCommand::NewProject => {
                *self = Self::default();
                self.status = StatusMessage::Info("Created a new project".to_owned());
            }
            EditorCommand::OpenProject => {
                self.status = StatusMessage::Info("Choose a Weave project to open".to_owned());
            }
            EditorCommand::Save => {
                self.dirty = false;
                self.status = StatusMessage::Info("Project saved".to_owned());
            }
            EditorCommand::Undo => {
                self.status = StatusMessage::Info("Undo".to_owned());
            }
            EditorCommand::Redo => {
                self.status = StatusMessage::Info("Redo".to_owned());
            }
            EditorCommand::ShowGraph => self.layout.center = CenterView::Graph,
            EditorCommand::ShowText => self.layout.center = CenterView::Text,
            EditorCommand::ToggleProject => {
                self.layout.project_sidebar = !self.layout.project_sidebar;
            }
            EditorCommand::ToggleInspector => {
                self.layout.inspector_sidebar = !self.layout.inspector_sidebar;
            }
            EditorCommand::TogglePatterns => {
                self.layout.pattern_browser = !self.layout.pattern_browser;
            }
            EditorCommand::TogglePreview => {
                self.layout.play_preview = !self.layout.play_preview;
            }
            EditorCommand::Compile => {
                self.status = StatusMessage::Info("Compiled current source".to_owned());
            }
            EditorCommand::Run => {
                self.status = StatusMessage::Info("Preview running".to_owned());
            }
            EditorCommand::Stop => {
                self.status = StatusMessage::Info("Preview stopped".to_owned());
            }
        }
    }

    /// Report a recoverable failure without unwinding the application.
    pub fn report_error(&mut self, message: impl Into<String>) {
        self.status = StatusMessage::Error(message.into());
    }
}

/// One entry in a platform menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEntry {
    /// Command item.
    Command(&'static str, EditorCommand),
    /// Visual separator.
    Separator,
}

/// Renderer-independent menu definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuDefinition {
    /// Top-level menu title.
    pub title: &'static str,
    /// Ordered menu items.
    pub entries: Vec<MenuEntry>,
}

/// Canonical editor menu structure.
#[must_use]
pub fn menu_definitions() -> Vec<MenuDefinition> {
    vec![
        MenuDefinition {
            title: "File",
            entries: vec![
                MenuEntry::Command("New Project", EditorCommand::NewProject),
                MenuEntry::Command("Open…", EditorCommand::OpenProject),
                MenuEntry::Separator,
                MenuEntry::Command("Save", EditorCommand::Save),
            ],
        },
        MenuDefinition {
            title: "Edit",
            entries: vec![
                MenuEntry::Command("Undo", EditorCommand::Undo),
                MenuEntry::Command("Redo", EditorCommand::Redo),
            ],
        },
        MenuDefinition {
            title: "View",
            entries: vec![
                MenuEntry::Command("Graph", EditorCommand::ShowGraph),
                MenuEntry::Command("Text", EditorCommand::ShowText),
                MenuEntry::Separator,
                MenuEntry::Command("Patterns", EditorCommand::TogglePatterns),
                MenuEntry::Command("Preview", EditorCommand::TogglePreview),
            ],
        },
        MenuDefinition {
            title: "Run",
            entries: vec![
                MenuEntry::Command("Compile", EditorCommand::Compile),
                MenuEntry::Command("Run Preview", EditorCommand::Run),
                MenuEntry::Command("Stop", EditorCommand::Stop),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_exposes_a_complete_project_workspace() {
        let state = EditorState::default();
        assert!(state.layout.project_sidebar);
        assert!(state.layout.inspector_sidebar);
        assert!(state.layout.play_preview);
        assert_eq!(state.layout.center, CenterView::Graph);
    }

    #[test]
    fn commands_update_layout_and_recoverable_status() {
        let mut state = EditorState::default();
        state.apply(EditorCommand::ShowText);
        state.apply(EditorCommand::TogglePatterns);
        state.report_error("source has errors");
        assert_eq!(state.layout.center, CenterView::Text);
        assert!(state.layout.pattern_browser);
        assert!(state.status.is_error());

        state.apply(EditorCommand::Save);
        assert!(!state.status.is_error());
    }

    #[test]
    fn menus_cover_project_editing_views_and_preview() {
        let menus = menu_definitions();
        assert_eq!(
            menus.iter().map(|menu| menu.title).collect::<Vec<_>>(),
            ["File", "Edit", "View", "Run"]
        );
        assert!(menus.iter().any(|menu| {
            menu.entries
                .contains(&MenuEntry::Command("Run Preview", EditorCommand::Run))
        }));
    }
}
