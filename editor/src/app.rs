//! GPUI application shell, commands, menus, layout, and startup boundary.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Context, Entity, FocusHandle, KeyBinding, Menu, MenuItem, Window,
    WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb, size,
};

use crate::WELCOME_SOURCE;
use crate::domain::DomainSession;
use crate::graph_view::GraphSurface;
use crate::state::{CenterView, EditorCommand, EditorState};
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
    graph: Entity<GraphSurface>,
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
        let graph = cx.new(|cx| {
            GraphSurface::new(
                domain
                    .document()
                    .expect("the embedded welcome source is valid"),
                cx,
            )
        });
        Self {
            state,
            domain,
            graph,
            focus,
        }
    }

    fn apply(&mut self, command: EditorCommand, cx: &mut Context<Self>) {
        if command == EditorCommand::Compile {
            if let Err(error) = self
                .domain
                .compile_source(self.domain.source().to_owned(), None)
            {
                self.state.report_error(error.to_string());
                cx.notify();
                return;
            }
        }
        self.state.apply(command);
        cx.notify();
    }

    fn panel(title: &str, detail: &str, width: Option<f32>) -> AnyElement {
        let mut panel = div()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(rgb(DARK_THEME.panel))
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(detail.to_owned()),
            );
        if let Some(width) = width {
            panel = panel.w(px(width)).flex_none();
        }
        panel.into_any_element()
    }
}

impl gpui::Render for EditorShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.state.layout.clone();
        let mut workspace = div().flex().flex_1().min_h_0().min_w_0();
        if layout.project_sidebar {
            workspace = workspace.child(Self::panel(
                "Project",
                "Open or create a .weave project",
                Some(layout.left_width),
            ));
        }

        let active_detail = match layout.center {
            CenterView::Graph => "GPU node graph canvas",
            CenterView::Text => "Unicode source editor",
        };
        let center_content = match layout.center {
            CenterView::Graph => self.graph.clone().into_any_element(),
            CenterView::Text => Self::panel("Text", active_detail, None),
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
            workspace = workspace.child(Self::panel(
                "Inspector",
                "Selection details and validation",
                Some(layout.right_width),
            ));
        }

        let mut root = div()
            .id("weave-editor-shell")
            .key_context("WeaveEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &NewProject, _, cx| {
                this.apply(EditorCommand::NewProject, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenProject, _, cx| {
                this.apply(EditorCommand::OpenProject, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveProject, _, cx| {
                this.apply(EditorCommand::Save, cx);
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
            root = root.child(
                div()
                    .h(px(120.0))
                    .p_3()
                    .bg(rgb(DARK_THEME.panel))
                    .border_t_1()
                    .border_color(rgb(DARK_THEME.border))
                    .text_sm()
                    .child("Pattern Browser — Tarot · I-Ching · Elder Futhark · Project"),
            );
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
