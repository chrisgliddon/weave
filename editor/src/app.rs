//! GPUI application shell, commands, menus, layout, and startup boundary.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Bounds, Context, Entity, FocusHandle, KeyBinding, Menu, MenuItem,
    PathPromptOptions, Window, WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb,
    size,
};

use crate::WELCOME_SOURCE;
use crate::domain::DomainSession;
use crate::graph_view::{GraphSurface, GraphSurfaceEvent};
use crate::node_renderers::{InspectorData, kind_label};
use crate::pattern_browser::{PatternBrowserEvent, PatternBrowserSurface};
use crate::project::{ConflictResolution, ExternalChange, ProjectSession, ProjectWatcher};
use crate::state::{CenterView, EditorCommand, EditorState};
use crate::sync::{CanonicalProjectModel, TextSync};
use crate::text_view::TextSurface;
use crate::theme::DARK_THEME;

actions!(
    weave_editor,
    [
        NewProject,
        OpenProject,
        SaveProject,
        Undo,
        Redo,
        ShowGraph,
        ShowText,
        ToggleProject,
        ToggleInspector,
        TogglePatterns,
        TogglePreview,
        CompileStory,
        RunPreview,
        StopPreview,
        Quit
    ]
);

/// Application launch behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Open the visible editor and run until its windows close.
    Desktop,
    /// Create a hidden window, initialize the complete shell, then exit.
    HeadlessSmoke,
}

/// Failure reported at the process boundary instead of panicking silently.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// GPUI or the native platform panicked during initialization.
    #[error("native application initialization failed: {0}")]
    PlatformPanic(String),
    /// GPUI could not create the main window.
    #[error("main window creation failed: {0}")]
    Window(String),
}

/// Start the editor while converting platform panics into a clear process-level error.
pub fn launch(mode: LaunchMode) -> Result<(), StartupError> {
    match catch_unwind(AssertUnwindSafe(|| launch_unchecked(mode))) {
        Ok(result) => result,
        Err(payload) => Err(StartupError::PlatformPanic(panic_message(payload))),
    }
}

fn launch_unchecked(mode: LaunchMode) -> Result<(), StartupError> {
    let failure = Rc::new(RefCell::new(None::<String>));
    let callback_failure = Rc::clone(&failure);
    let application = match mode {
        LaunchMode::Desktop => gpui_platform::application(),
        LaunchMode::HeadlessSmoke => gpui_platform::headless(),
    };
    application.run(move |cx: &mut App| {
        install_commands_and_menus(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                show: mode == LaunchMode::Desktop,
                window_min_size: Some(size(px(960.0), px(640.0))),
                app_id: Some("dev.weave.editor".to_owned()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| EditorShell::new(window, cx)),
        );
        match opened {
            Ok(_) => {
                cx.activate(true);
                if mode == LaunchMode::HeadlessSmoke {
                    cx.defer(|cx| cx.quit());
                }
            }
            Err(error) => {
                *callback_failure.borrow_mut() = Some(error.to_string());
                cx.quit();
            }
        }
    });
    let message = failure.borrow_mut().take();
    match message {
        Some(message) => Err(StartupError::Window(message)),
        None => Ok(()),
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown native platform failure".to_owned()
    }
}

fn install_commands_and_menus(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-n", NewProject, Some("WeaveEditor")),
        KeyBinding::new("secondary-o", OpenProject, Some("WeaveEditor")),
        KeyBinding::new("secondary-s", SaveProject, Some("WeaveEditor")),
        KeyBinding::new("secondary-z", Undo, Some("WeaveEditor")),
        KeyBinding::new("secondary-shift-z", Redo, Some("WeaveEditor")),
        KeyBinding::new("secondary-1", ShowGraph, Some("WeaveEditor")),
        KeyBinding::new("secondary-2", ShowText, Some("WeaveEditor")),
        KeyBinding::new("secondary-enter", RunPreview, Some("WeaveEditor")),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.set_menus([
        Menu::new("Weave").items([MenuItem::action("Quit Weave", Quit)]),
        Menu::new("File").items([
            MenuItem::action("New Project", NewProject),
            MenuItem::action("Open…", OpenProject),
            MenuItem::separator(),
            MenuItem::action("Save", SaveProject),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Undo", Undo),
            MenuItem::action("Redo", Redo),
        ]),
        Menu::new("View").items([
            MenuItem::action("Graph", ShowGraph),
            MenuItem::action("Text", ShowText),
            MenuItem::separator(),
            MenuItem::action("Project", ToggleProject),
            MenuItem::action("Inspector", ToggleInspector),
            MenuItem::action("Patterns", TogglePatterns),
            MenuItem::action("Preview", TogglePreview),
        ]),
        Menu::new("Run").items([
            MenuItem::action("Compile", CompileStory),
            MenuItem::action("Run Preview", RunPreview),
            MenuItem::action("Stop", StopPreview),
        ]),
    ]);
}

struct EditorShell {
    state: EditorState,
    domain: DomainSession,
    canonical: CanonicalProjectModel,
    canonical_revision_seen: u64,
    text_revision_seen: u64,
    project: ProjectSession,
    watcher: Option<ProjectWatcher>,
    last_recovery_revision: Option<u64>,
    graph: Entity<GraphSurface>,
    text: Entity<TextSurface>,
    patterns: Entity<PatternBrowserSurface>,
    focus: FocusHandle,
}

impl EditorShell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut state = EditorState::default();
        let mut domain = DomainSession::new(0);
        if let Err(error) = domain.compile_source(WELCOME_SOURCE, None) {
            state.report_error(error.to_string());
        }
        let canonical = CanonicalProjectModel::new(WELCOME_SOURCE);
        let graph = cx.new(|cx| {
            GraphSurface::new_with_layout(
                domain
                    .document()
                    .expect("the embedded welcome source is valid"),
                canonical.layout(),
                cx,
            )
        });
        cx.subscribe(&graph, |this, _, event, cx| {
            this.handle_graph_event(event, cx);
        })
        .detach();
        let text = cx.new(|cx| TextSurface::new(WELCOME_SOURCE, cx));
        let patterns = cx.new(|cx| {
            PatternBrowserSurface::new(
                domain.document(),
                domain.last_valid_story(),
                WELCOME_SOURCE,
                domain.diagnostics(),
                cx,
            )
        });
        cx.subscribe(&patterns, |this, _, event, cx| {
            this.handle_pattern_event(event, cx);
        })
        .detach();
        let shell = Self {
            state,
            domain,
            canonical,
            canonical_revision_seen: 0,
            text_revision_seen: 0,
            project: ProjectSession::untitled(WELCOME_SOURCE),
            watcher: None,
            last_recovery_revision: None,
            graph,
            text,
            patterns,
            focus,
        };
        shell.schedule_project_poll(cx);
        shell
    }

    fn schedule_project_poll(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this.update(cx, |this, cx| this.poll_project(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_project(&mut self, cx: &mut Context<Self>) {
        self.synchronize_text_view(cx);
        self.capture_graph_layout(cx);
        self.sync_source_to_project(cx);
        self.persist_recovery_if_needed();
        let change = match self.watcher.as_mut() {
            Some(watcher) => watcher.poll(&mut self.project, Instant::now()),
            None => return,
        };
        match change {
            Ok(Some(ExternalChange::Reloaded)) => {
                if self.load_project_source(cx) {
                    self.state.status = crate::state::StatusMessage::Info(
                        "Reloaded an external project change".to_owned(),
                    );
                }
                cx.notify();
            }
            Ok(Some(ExternalChange::Conflict(_))) => {
                self.state.report_error(
                    "The project changed on disk; choose a conflict action in Project",
                );
                cx.notify();
            }
            Ok(Some(ExternalChange::NoChange | ExternalChange::SelfAuthored)) | Ok(None) => {}
            Err(error) => {
                self.state.report_error(error.to_string());
                cx.notify();
            }
        }
    }

    fn sync_source_to_project(&mut self, cx: &App) {
        let source = self.text.read(cx).source();
        if source != self.project.source() {
            self.project.set_source(source.to_owned());
        }
        self.state.dirty = self.project.is_dirty();
    }

    fn synchronize_text_view(&mut self, cx: &mut Context<Self>) {
        let revision = self.text.read(cx).revision();
        if revision == self.text_revision_seen {
            return;
        }
        let source = self.text.read(cx).source().to_owned();
        match self
            .canonical
            .apply_text(source.clone(), self.canonical_revision_seen)
        {
            Ok(sync) => {
                self.text_revision_seen = revision;
                self.canonical_revision_seen = sync.revision();
                self.project.set_source(source);
                match sync {
                    TextSync::Unchanged { .. } => {}
                    TextSync::Applied { .. } => {
                        if self.compile_current_source(cx) {
                            self.state.status = crate::state::StatusMessage::Info(
                                "Text and graph synchronized".to_owned(),
                            );
                        }
                    }
                    TextSync::Invalid { diagnostics, .. } => {
                        let _ = self.compile_current_source(cx);
                        self.state.report_error(format!(
                            "Graph is showing the last valid source; fix {} syntax diagnostic(s)",
                            diagnostics.len()
                        ));
                    }
                }
            }
            Err(conflict) => {
                self.state
                    .report_error(format!("Synchronization conflict: {conflict}"));
            }
        }
        cx.notify();
    }

    fn capture_graph_layout(&mut self, cx: &App) {
        let layout = self.graph.read(cx).layout(cx);
        let _ = self.canonical.capture_layout(layout);
    }

    fn persist_recovery_if_needed(&mut self) {
        if !self.project.is_dirty() || self.last_recovery_revision == Some(self.project.revision())
        {
            return;
        }
        let Some(path) = self.project.recovery_path() else {
            return;
        };
        match self.project.write_recovery(path) {
            Ok(()) => self.last_recovery_revision = Some(self.project.revision()),
            Err(error) => self.state.report_error(error.to_string()),
        }
    }

    fn compile_current_source(&mut self, cx: &mut Context<Self>) -> bool {
        let source = self.text.read(cx).source().to_owned();
        let source_name = self
            .project
            .path()
            .map(|path| path.to_string_lossy().into_owned());
        let compiled = match self.domain.compile_source(source, source_name) {
            Ok(()) => {
                let document = self
                    .domain
                    .document()
                    .expect("successful compilation retains a syntax tree")
                    .clone();
                self.install_graph(&document, cx);
                true
            }
            Err(error) => {
                self.state.report_error(error.to_string());
                false
            }
        };
        self.refresh_pattern_browser(cx);
        compiled
    }

    fn install_graph(&mut self, document: &weave_core::Document, cx: &mut Context<Self>) {
        let graph =
            cx.new(|cx| GraphSurface::new_with_layout(document, self.canonical.layout(), cx));
        cx.subscribe(&graph, |this, _, event, cx| {
            this.handle_graph_event(event, cx);
        })
        .detach();
        self.graph = graph;
    }

    fn refresh_pattern_browser(&mut self, cx: &mut Context<Self>) {
        let document = self.domain.document().cloned();
        let story = self.domain.last_valid_story().cloned();
        let source = self.domain.source().to_owned();
        let diagnostics = self.domain.diagnostics().to_vec();
        self.patterns.update(cx, |patterns, cx| {
            patterns.refresh(document.as_ref(), story.as_ref(), &source, &diagnostics, cx);
        });
    }

    fn handle_pattern_event(&mut self, event: &PatternBrowserEvent, cx: &mut Context<Self>) {
        match event {
            PatternBrowserEvent::RevealSource(span) => {
                self.text.update(cx, |text, cx| text.reveal_span(*span, cx));
                self.state.layout.center = CenterView::Text;
                self.state.status = crate::state::StatusMessage::Info(format!(
                    "Pattern source at {}:{}",
                    span.line, span.column
                ));
            }
            PatternBrowserEvent::RevealGraph(id) => {
                let graph_id = format!("pattern:{id}");
                let selected = self
                    .graph
                    .update(cx, |graph, cx| graph.select_node(&graph_id, cx));
                if selected {
                    self.state.layout.center = CenterView::Graph;
                    self.state.status =
                        crate::state::StatusMessage::Info(format!("Selected pattern node {id}"));
                } else {
                    self.state
                        .report_error("This built-in has no project graph node");
                }
            }
        }
        cx.notify();
    }

    fn handle_graph_event(&mut self, event: &GraphSurfaceEvent, cx: &mut Context<Self>) {
        let edit = match event {
            GraphSurfaceEvent::Unavailable(message) => {
                self.state.report_error(message.clone());
                cx.notify();
                return;
            }
            GraphSurfaceEvent::Edit(edit) => edit.clone(),
        };
        self.synchronize_text_view(cx);
        match self
            .canonical
            .apply_graph_edit(&edit, self.canonical_revision_seen)
        {
            Ok(sync) => {
                self.canonical_revision_seen = sync.revision;
                self.text
                    .update(cx, |text, cx| text.replace_source(&sync.source, cx));
                self.text_revision_seen = self.text.read(cx).revision();
                self.project.set_source(sync.source);
                let compiled = self.compile_current_source(cx);
                if let Some(node) = sync.focus_node {
                    let _ = self
                        .graph
                        .update(cx, |graph, cx| graph.select_node(&node, cx));
                }
                if compiled {
                    self.state.status = crate::state::StatusMessage::Info(sync.summary);
                }
            }
            Err(conflict) => self
                .state
                .report_error(format!("Graph edit blocked: {conflict}")),
        }
        cx.notify();
    }

    fn undo_active_view(&mut self, cx: &mut Context<Self>) {
        match self.state.layout.center {
            CenterView::Text => {
                if self.text.update(cx, |text, cx| text.undo(cx)) {
                    self.synchronize_text_view(cx);
                    self.state.status =
                        crate::state::StatusMessage::Info("Undid source edit".to_owned());
                }
            }
            CenterView::Graph => {
                if let Some(source) = self.canonical.undo() {
                    self.apply_history_source(source, "Undid synchronized graph edit", cx);
                } else {
                    self.graph.update(cx, |graph, cx| graph.undo(cx));
                }
            }
        }
        cx.notify();
    }

    fn redo_active_view(&mut self, cx: &mut Context<Self>) {
        match self.state.layout.center {
            CenterView::Text => {
                if self.text.update(cx, |text, cx| text.redo(cx)) {
                    self.synchronize_text_view(cx);
                    self.state.status =
                        crate::state::StatusMessage::Info("Redid source edit".to_owned());
                }
            }
            CenterView::Graph => {
                if let Some(source) = self.canonical.redo() {
                    self.apply_history_source(source, "Redid synchronized graph edit", cx);
                } else {
                    self.graph.update(cx, |graph, cx| graph.redo(cx));
                }
            }
        }
        cx.notify();
    }

    fn apply_history_source(&mut self, source: String, message: &str, cx: &mut Context<Self>) {
        self.canonical_revision_seen = self.canonical.revision();
        self.text
            .update(cx, |text, cx| text.replace_source(&source, cx));
        self.text_revision_seen = self.text.read(cx).revision();
        self.project.set_source(source);
        if self.compile_current_source(cx) {
            self.state.status = crate::state::StatusMessage::Info(message.to_owned());
        }
    }

    fn load_project_source(&mut self, cx: &mut Context<Self>) -> bool {
        let source = self.project.source().to_owned();
        self.canonical = CanonicalProjectModel::new(source.clone());
        self.canonical_revision_seen = self.canonical.revision();
        self.text
            .update(cx, |text, cx| text.load_source(&source, cx));
        self.text_revision_seen = self.text.read(cx).revision();
        self.state.project_name = self.project.display_name();
        self.state.dirty = false;
        self.last_recovery_revision = None;
        let graph_document = self.canonical.graph_document().clone();
        self.install_graph(&graph_document, cx);
        self.compile_current_source(cx)
    }

    fn new_project(&mut self, cx: &mut Context<Self>) {
        self.synchronize_text_view(cx);
        self.sync_source_to_project(cx);
        self.persist_recovery_if_needed();
        let mut project = ProjectSession::untitled(WELCOME_SOURCE);
        project.inherit_recent(&self.project);
        self.project = project;
        self.watcher = None;
        let _ = self.load_project_source(cx);
        self.state.status = crate::state::StatusMessage::Info("Created a new project".to_owned());
        cx.notify();
    }

    fn prompt_open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Weave Project".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, _window, cx| this.open_project_path(&path, cx));
        })
        .detach();
    }

    fn open_project_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.synchronize_text_view(cx);
        self.sync_source_to_project(cx);
        self.persist_recovery_if_needed();
        match self.project.open_from(path) {
            Ok(project) => {
                self.project = project;
                let compiled = self.load_project_source(cx);
                match ProjectWatcher::new(path, Duration::from_millis(150)) {
                    Ok(watcher) => {
                        self.watcher = Some(watcher);
                        if compiled {
                            self.state.status = crate::state::StatusMessage::Info(format!(
                                "Opened {}",
                                self.project.display_name()
                            ));
                        }
                    }
                    Err(error) => {
                        self.watcher = None;
                        self.state.report_error(format!(
                            "Opened project, but file watching failed: {error}"
                        ));
                    }
                }
            }
            Err(error) => self.state.report_error(error.to_string()),
        }
        cx.notify();
    }

    fn save_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.synchronize_text_view(cx);
        self.sync_source_to_project(cx);
        if self.project.path().is_none() {
            self.prompt_save_project_as(window, cx);
            return;
        }
        self.save_project_to_current_path(cx);
    }

    fn prompt_save_project_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let directory = self
            .project
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&directory, Some("story.weave"));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let _ = this.update_in(cx, |this, _window, cx| this.save_project_as(&path, cx));
        })
        .detach();
    }

    fn save_project_as(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.synchronize_text_view(cx);
        self.sync_source_to_project(cx);
        match self.project.save_as(path) {
            Ok(saved) => self.finish_save(saved.diagnostics.len(), cx),
            Err(error) => self.state.report_error(error.to_string()),
        }
        cx.notify();
    }

    fn save_project_to_current_path(&mut self, cx: &mut Context<Self>) {
        match self.project.save() {
            Ok(saved) => self.finish_save(saved.diagnostics.len(), cx),
            Err(error) => self.state.report_error(error.to_string()),
        }
        cx.notify();
    }

    fn finish_save(&mut self, diagnostic_count: usize, cx: &mut Context<Self>) {
        self.text.update(cx, |text, _| text.mark_saved());
        self.state.project_name = self.project.display_name();
        self.state.dirty = false;
        self.last_recovery_revision = None;
        let _ = self.compile_current_source(cx);
        if diagnostic_count == 0 {
            self.state.status =
                crate::state::StatusMessage::Info(format!("Saved {}", self.project.display_name()));
        } else {
            self.state.report_error(format!(
                "Saved source with {diagnostic_count} compiler diagnostic(s)"
            ));
        }
        if let Some(path) = self.project.path() {
            match ProjectWatcher::new(path, Duration::from_millis(150)) {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(error) => self
                    .state
                    .report_error(format!("Saved project, but file watching failed: {error}")),
            }
        }
    }

    fn resolve_project_conflict(&mut self, resolution: ConflictResolution, cx: &mut Context<Self>) {
        match self.project.resolve_conflict(resolution) {
            Ok(result) => {
                let mut loaded_cleanly = true;
                if resolution != ConflictResolution::KeepMemory {
                    loaded_cleanly = self.load_project_source(cx);
                } else {
                    self.state.dirty = true;
                }
                let message = match result.memory_copy {
                    Some(path) => format!(
                        "Kept both versions; memory copy saved as {}",
                        path.display()
                    ),
                    None if resolution == ConflictResolution::KeepMemory => {
                        "Kept the in-memory version; save to overwrite the disk version".to_owned()
                    }
                    None => "Loaded the version from disk".to_owned(),
                };
                if loaded_cleanly {
                    self.state.status = crate::state::StatusMessage::Info(message);
                } else {
                    self.state
                        .report_error(format!("{message}; source has compiler diagnostics"));
                }
            }
            Err(error) => self.state.report_error(error.to_string()),
        }
        cx.notify();
    }

    fn apply(&mut self, command: EditorCommand, cx: &mut Context<Self>) {
        match command {
            EditorCommand::Undo => {
                self.undo_active_view(cx);
                return;
            }
            EditorCommand::Redo => {
                self.redo_active_view(cx);
                return;
            }
            _ => {}
        }
        if command == EditorCommand::Compile {
            self.synchronize_text_view(cx);
            self.sync_source_to_project(cx);
            if !self.compile_current_source(cx) {
                cx.notify();
                return;
            }
        }
        self.state.apply(command);
        cx.notify();
    }

    fn project_panel(&self, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let dirty = self.project.is_dirty() || self.text.read(cx).is_dirty();
        let path = self.project.path().map_or_else(
            || "Not saved yet".to_owned(),
            |path| path.display().to_string(),
        );
        let compile_output = self.project.compile_output().map_or_else(
            || "Compile output appears after a valid save".to_owned(),
            |path| format!("Compiled: {}", path.display()),
        );
        let mut panel = div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(rgb(DARK_THEME.panel))
            .border_r_1()
            .border_color(rgb(DARK_THEME.border))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child("Project"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(if dirty {
                        rgb(DARK_THEME.accent)
                    } else {
                        rgb(DARK_THEME.text)
                    })
                    .child(format!(
                        "{}{}",
                        self.project.display_name(),
                        if dirty { " •" } else { "" }
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(path),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(compile_output),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .id("project-new")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(DARK_THEME.chrome))
                            .cursor_pointer()
                            .text_xs()
                            .child("New")
                            .on_click(cx.listener(|this, _, _, cx| this.new_project(cx))),
                    )
                    .child(
                        div()
                            .id("project-open")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(DARK_THEME.chrome))
                            .cursor_pointer()
                            .text_xs()
                            .child("Open…")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.prompt_open_project(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("project-save")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(DARK_THEME.accent))
                            .cursor_pointer()
                            .text_xs()
                            .child("Save")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_project(window, cx);
                            })),
                    ),
            );

        if self.project.conflict().is_some() {
            panel = panel
                .child(
                    div()
                        .mt_2()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(DARK_THEME.error))
                        .text_xs()
                        .child("This file changed outside Weave. Choose which version to keep."),
                )
                .child(
                    div()
                        .id("conflict-keep-memory")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(DARK_THEME.chrome))
                        .cursor_pointer()
                        .text_xs()
                        .child("Keep editor version")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.resolve_project_conflict(ConflictResolution::KeepMemory, cx);
                        })),
                )
                .child(
                    div()
                        .id("conflict-take-disk")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(DARK_THEME.chrome))
                        .cursor_pointer()
                        .text_xs()
                        .child("Load disk version")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.resolve_project_conflict(ConflictResolution::TakeDisk, cx);
                        })),
                )
                .child(
                    div()
                        .id("conflict-save-both")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(DARK_THEME.chrome))
                        .cursor_pointer()
                        .text_xs()
                        .child("Save both versions")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.resolve_project_conflict(ConflictResolution::SaveBoth, cx);
                        })),
                );
        }

        if !self.project.recent().is_empty() {
            panel = panel.child(
                div()
                    .mt_3()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("RECENT"),
            );
            for (index, path) in self.project.recent().iter().take(6).enumerate() {
                let open_path = path.clone();
                let label = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into(),
                );
                panel = panel.child(
                    div()
                        .id(("recent-project", index))
                        .px_1()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.text))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_project_path(&open_path, cx);
                        })),
                );
            }
        }
        panel.into_any_element()
    }

    fn inspector_panel(selection: Option<InspectorData>, width: f32) -> AnyElement {
        let mut panel = div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .bg(rgb(DARK_THEME.panel))
            .border_l_1()
            .border_color(rgb(DARK_THEME.border))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child("Inspector"),
            );
        let Some(selection) = selection else {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child("Select a graph node to inspect its source and validation."),
                )
                .into_any_element();
        };
        panel = panel
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.accent))
                    .child(kind_label(selection.kind).to_uppercase()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child(selection.title),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(selection.preview),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!(
                        "Position  {:.0}, {:.0}",
                        selection.position.x, selection.position.y
                    )),
            );
        if let Some(span) = selection.source_span {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!("Source  {}:{}", span.line, span.column)),
            );
        }
        if let Some(validation) = selection.validation {
            panel = panel.child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.error))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.text))
                    .child(validation),
            );
        }
        panel.into_any_element()
    }
}

impl gpui::Render for EditorShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.state.layout.clone();
        let mut workspace = div().flex().flex_1().min_h_0().min_w_0();
        if layout.project_sidebar {
            workspace = workspace.child(self.project_panel(layout.left_width, cx));
        }

        let active_detail = match layout.center {
            CenterView::Graph => "GPU node graph canvas",
            CenterView::Text => "Unicode source editor",
        };
        let center_content = match layout.center {
            CenterView::Graph => self.graph.clone().into_any_element(),
            CenterView::Text => self.text.clone().into_any_element(),
        };
        let center = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .h(px(38.0))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .bg(rgb(DARK_THEME.chrome))
                    .border_b_1()
                    .border_color(rgb(DARK_THEME.border))
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child(format!("{} View", layout.center.label()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(active_detail),
                    ),
            )
            .child(center_content);
        workspace = workspace.child(center);

        if layout.inspector_sidebar {
            let selection = self.graph.read(cx).inspector_data(cx);
            workspace = workspace.child(Self::inspector_panel(selection, layout.right_width));
        }

        let mut root = div()
            .id("weave-editor-shell")
            .key_context("WeaveEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &NewProject, _, cx| {
                this.new_project(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenProject, window, cx| {
                this.prompt_open_project(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveProject, window, cx| {
                this.save_project(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Undo, _, cx| {
                this.apply(EditorCommand::Undo, cx);
            }))
            .on_action(cx.listener(|this, _: &Redo, _, cx| {
                this.apply(EditorCommand::Redo, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowGraph, _, cx| {
                this.apply(EditorCommand::ShowGraph, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowText, _, cx| {
                this.apply(EditorCommand::ShowText, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleProject, _, cx| {
                this.apply(EditorCommand::ToggleProject, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleInspector, _, cx| {
                this.apply(EditorCommand::ToggleInspector, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePatterns, _, cx| {
                this.apply(EditorCommand::TogglePatterns, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePreview, _, cx| {
                this.apply(EditorCommand::TogglePreview, cx);
            }))
            .on_action(cx.listener(|this, _: &CompileStory, _, cx| {
                this.apply(EditorCommand::Compile, cx);
            }))
            .on_action(cx.listener(|this, _: &RunPreview, _, cx| {
                this.apply(EditorCommand::Run, cx);
            }))
            .on_action(cx.listener(|this, _: &StopPreview, _, cx| {
                this.apply(EditorCommand::Stop, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(DARK_THEME.workspace))
            .text_color(rgb(DARK_THEME.text))
            .child(
                div()
                    .h(px(44.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .bg(rgb(DARK_THEME.chrome))
                    .border_b_1()
                    .border_color(rgb(DARK_THEME.border))
                    .child(
                        div()
                            .text_sm()
                            .child(format!("Weave — {}", self.state.project_name)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child("Graph  Text  Patterns  Preview"),
                    ),
            )
            .child(workspace);

        if layout.pattern_browser {
            root = root.child(div().h(px(280.0)).flex_none().child(self.patterns.clone()));
        }
        if layout.play_preview {
            root = root.child(
                div()
                    .h(px(layout.bottom_height))
                    .p_3()
                    .bg(rgb(DARK_THEME.panel))
                    .border_t_1()
                    .border_color(rgb(DARK_THEME.border))
                    .text_sm()
                    .child("Play Preview — ready"),
            );
        }
        let status_color = if self.state.status.is_error() {
            DARK_THEME.error
        } else {
            DARK_THEME.chrome
        };
        root.child(
            div()
                .h(px(26.0))
                .flex()
                .items_center()
                .px_3()
                .bg(rgb(status_color))
                .border_t_1()
                .border_color(rgb(DARK_THEME.border))
                .text_xs()
                .child(self.state.status.text().to_owned()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_payloads_are_safely_rendered() {
        assert_eq!(panic_message(Box::new("boom")), "boom");
        assert_eq!(
            panic_message(Box::new(String::from("owned boom"))),
            "owned boom"
        );
        assert_eq!(
            panic_message(Box::new(7_u8)),
            "unknown native platform failure"
        );
    }
}
