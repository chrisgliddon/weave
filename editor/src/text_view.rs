//! Virtualized native source editor surface.

use std::ops::Range;

use gpui::{
    AnyElement, ClipboardItem, Context, FocusHandle, KeyDownEvent, ScrollStrategy,
    UniformListScrollHandle, Window, div, prelude::*, px, rgb, rgba, uniform_list,
};
use weave_core::Span;
use weave_domain::DomainCatalog;

use crate::text_editor::{SyntaxKind, SyntaxToken, TextBuffer};
use crate::theme::DARK_THEME;

/// Editable, virtualized `.weave` text surface.
pub struct TextSurface {
    buffer: TextBuffer,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    search_active: bool,
    search_query: String,
    search_matches: Vec<Range<usize>>,
    search_index: usize,
    message: String,
}

impl TextSurface {
    /// Create a text editor over current project source.
    pub fn new(source: impl Into<String>, cx: &mut Context<Self>) -> Self {
        Self {
            buffer: TextBuffer::new(source),
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            search_active: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: 0,
            message: "Source is current".to_owned(),
        }
    }

    /// Current raw source.
    #[must_use]
    pub fn source(&self) -> &str {
        self.buffer.source()
    }

    /// Whether the text buffer differs from its saved revision.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.buffer.is_dirty()
    }

    /// Monotonic edit revision used by the project synchronizer.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.buffer.revision()
    }

    /// Replace the whole source as one undoable edit.
    pub fn replace_source(&mut self, source: &str, cx: &mut Context<Self>) {
        self.buffer.select_all();
        self.buffer.insert(source);
        self.after_edit();
        cx.notify();
    }

    /// Load a project or externally-updated file as a clean history boundary.
    pub fn load_source(&mut self, source: &str, cx: &mut Context<Self>) {
        self.buffer.load(source);
        self.after_edit();
        self.message = "Source loaded from disk".to_owned();
        cx.notify();
    }

    /// Replace the explicit domain catalog used for inline compiler diagnostics.
    pub fn set_domain_catalog(&mut self, domain_catalog: DomainCatalog, cx: &mut Context<Self>) {
        self.buffer.set_domain_catalog(domain_catalog);
        self.after_edit();
        cx.notify();
    }

    /// Mark current source as saved.
    pub const fn mark_saved(&mut self) {
        self.buffer.mark_saved();
    }

    /// Reveal and select a source span requested by another editor panel.
    pub fn reveal_span(&mut self, span: Span, cx: &mut Context<Self>) {
        self.buffer.select_range(span.start..span.end);
        self.scroll_cursor_into_view();
        self.message = format!("Source {}:{}", span.line, span.column);
        cx.notify();
    }

    /// Undo one source edit and refresh derived editor feedback.
    pub fn undo(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.buffer.undo();
        if changed {
            self.after_edit();
            cx.notify();
        }
        changed
    }

    /// Redo one source edit and refresh derived editor feedback.
    pub fn redo(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.buffer.redo();
        if changed {
            self.after_edit();
            cx.notify();
        }
        changed
    }

    fn after_edit(&mut self) {
        self.refresh_search();
        self.message = if self.buffer.diagnostics().is_empty() {
            "Source compiled successfully".to_owned()
        } else {
            format!("{} diagnostic(s)", self.buffer.diagnostics().len())
        };
    }

    fn format_source(&mut self, cx: &mut Context<Self>) {
        match self.buffer.format() {
            Ok(true) => {
                self.message = "Formatted source".to_owned();
                self.refresh_search();
            }
            Ok(false) => self.message = "Source is already formatted".to_owned(),
            Err(diagnostics) => {
                self.message = format!("Cannot format: {} diagnostic(s)", diagnostics.len());
            }
        }
        cx.notify();
    }

    fn activate_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_active = true;
        self.refresh_search();
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn refresh_search(&mut self) {
        self.search_matches = self.buffer.search(&self.search_query, false);
        if self.search_matches.is_empty() {
            self.search_index = 0;
        } else {
            self.search_index = self.search_index.min(self.search_matches.len() - 1);
        }
    }

    fn next_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_index = (self.search_index + 1) % self.search_matches.len();
        let range = self.search_matches[self.search_index].clone();
        self.buffer.select_range(range);
        self.scroll_cursor_into_view();
    }

    fn scroll_cursor_into_view(&self) {
        let (line, _) = self.buffer.line_column(self.buffer.cursor());
        self.scroll.scroll_to_item(line, ScrollStrategy::Nearest);
    }

    fn goto_diagnostic(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.buffer.navigate_to_diagnostic(index) {
            self.scroll_cursor_into_view();
            self.message = format!(
                "Diagnostic {} of {}",
                index + 1,
                self.buffer.diagnostics().len()
            );
            cx.notify();
        }
    }

    fn click_line(&mut self, line: usize, window: &mut Window, cx: &mut Context<Self>) {
        let end = self
            .buffer
            .line_range(line)
            .map_or(self.buffer.source().len(), |range| {
                range.start
                    + self.buffer.source()[range.clone()]
                        .trim_end_matches(['\r', '\n'])
                        .len()
            });
        self.buffer.move_to(end, false);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_ref();
        let platform = event.keystroke.modifiers.platform;
        let shift = event.keystroke.modifiers.shift;

        if platform && key == "f" {
            self.activate_search(window, cx);
            return;
        }
        if self.search_active {
            match key {
                "escape" => self.search_active = false,
                "enter" => self.next_search_match(),
                "backspace" => {
                    self.search_query.pop();
                    self.refresh_search();
                }
                _ if !platform && !event.keystroke.modifiers.control => {
                    if let Some(character) = &event.keystroke.key_char {
                        self.search_query.push_str(&character.to_string());
                        self.refresh_search();
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        if platform {
            match key {
                "z" if shift => {
                    self.buffer.redo();
                    self.after_edit();
                }
                "z" => {
                    self.buffer.undo();
                    self.after_edit();
                }
                "a" => self.buffer.select_all(),
                "c" => {
                    let selection = self.buffer.selection();
                    if !selection.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            self.buffer.source()[selection].to_owned(),
                        ));
                    }
                }
                "x" => {
                    let selection = self.buffer.selection();
                    if !selection.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            self.buffer.source()[selection].to_owned(),
                        ));
                        self.buffer.insert("");
                        self.after_edit();
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.buffer.insert(&text);
                        self.after_edit();
                    }
                }
                "g" => self.next_search_match(),
                _ => return,
            }
            cx.notify();
            return;
        }

        let edited = match key {
            "backspace" => {
                self.buffer.delete_backward();
                true
            }
            "delete" => {
                self.buffer.delete_forward();
                true
            }
            "enter" => {
                self.buffer.insert("\n");
                true
            }
            "tab" => {
                self.buffer.insert("    ");
                true
            }
            "left" | "arrowleft" => {
                self.buffer.move_left(shift);
                false
            }
            "right" | "arrowright" => {
                self.buffer.move_right(shift);
                false
            }
            "up" | "arrowup" => {
                self.buffer.move_vertical(-1, shift);
                false
            }
            "down" | "arrowdown" => {
                self.buffer.move_vertical(1, shift);
                false
            }
            "home" => {
                self.buffer.move_home(shift);
                false
            }
            "end" => {
                self.buffer.move_end(shift);
                false
            }
            _ if !event.keystroke.modifiers.control && !event.keystroke.modifiers.alt => {
                if let Some(character) = &event.keystroke.key_char {
                    self.buffer.insert(&character.to_string());
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if edited {
            self.after_edit();
        }
        self.scroll_cursor_into_view();
        cx.notify();
    }

    fn render_line(&self, line: usize, focused: bool) -> AnyElement {
        let Some(range) = self.buffer.line_range(line) else {
            return div().into_any_element();
        };
        let raw = self.buffer.source()[range.clone()].trim_end_matches(['\r', '\n']);
        let content_end = range.start + raw.len();
        let tokens = self.buffer.tokens_for_line(line);
        let selection = self.buffer.selection();
        let mut segments = Vec::new();
        let mut offset = range.start;
        for token in tokens {
            let start = token.range.start.max(range.start).min(content_end);
            let end = token.range.end.max(start).min(content_end);
            if offset < start {
                segments.push(source_segment(
                    &self.buffer.source()[offset..start],
                    None,
                    selection.start < start && selection.end > offset,
                ));
            }
            if start < end {
                segments.push(source_segment(
                    &self.buffer.source()[start..end],
                    Some(token),
                    selection.start < end && selection.end > start,
                ));
            }
            offset = end;
        }
        if offset < content_end {
            segments.push(source_segment(
                &self.buffer.source()[offset..content_end],
                None,
                selection.start < content_end && selection.end > offset,
            ));
        }
        if segments.is_empty() {
            segments.push(source_segment(" ", None, false));
        }
        let (cursor_line, cursor_column) = self.buffer.line_column(self.buffer.cursor());
        let has_diagnostic = self
            .buffer
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.span.is_some_and(|span| span.line == line + 1));
        let mut row = div()
            .id(("source-line", line))
            .h(px(22.0))
            .w_full()
            .relative()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .when(cursor_line == line, |row| row.bg(rgba(0x7c9cff12)))
            .child(
                div()
                    .w(px(52.0))
                    .pr_3()
                    .text_right()
                    .text_xs()
                    .text_color(if has_diagnostic {
                        rgb(0xff5d73)
                    } else {
                        rgb(0x626d80)
                    })
                    .child((line + 1).to_string()),
            )
            .child(
                div()
                    .flex()
                    .font_family(".ZedMono")
                    .text_sm()
                    .children(segments),
            );
        if focused && cursor_line == line && selection.is_empty() {
            row = row.child(
                div()
                    .absolute()
                    .left(px(55.0 + cursor_column as f32 * 7.8))
                    .top(px(3.0))
                    .w(px(1.0))
                    .h(px(17.0))
                    .bg(rgb(DARK_THEME.accent)),
            );
        }
        if let Some(span) = self
            .buffer
            .diagnostics()
            .iter()
            .find_map(|diagnostic| diagnostic.span.filter(|span| span.line == line + 1))
        {
            row = row.child(
                div()
                    .absolute()
                    .left(px(55.0 + span.column.saturating_sub(1) as f32 * 7.8))
                    .bottom(px(1.0))
                    .w(px((span.end.saturating_sub(span.start).max(1) as f32
                        * 7.8)
                        .min(240.0)))
                    .h(px(1.0))
                    .bg(rgb(0xff5d73)),
            );
        }
        row.into_any_element()
    }
}

impl gpui::Render for TextSurface {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line_count = self.buffer.line_count();
        let focused = self.focus.is_focused(window);
        let dirty = self.buffer.is_dirty();
        let diagnostic_count = self.buffer.diagnostics().len();
        let diagnostics = self
            .buffer
            .diagnostics()
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>();

        div()
            .id("weave-text-surface")
            .key_context("WeaveText")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(DARK_THEME.workspace))
            .on_key_down(cx.listener(Self::key_down))
            .child(
                div()
                    .h(px(38.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .bg(rgb(DARK_THEME.chrome))
                    .border_b_1()
                    .border_color(rgb(DARK_THEME.border))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("format-source")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(DARK_THEME.border))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(DARK_THEME.accent)))
                                    .on_click(cx.listener(|this, _, _, cx| this.format_source(cx)))
                                    .text_xs()
                                    .child("Format"),
                            )
                            .child(
                                div()
                                    .id("search-source")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(DARK_THEME.border))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.activate_search(window, cx);
                                    }))
                                    .text_xs()
                                    .child("Search  ⌘/Ctrl-F"),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if diagnostic_count > 0 {
                                rgb(0xff8a9d)
                            } else {
                                rgb(DARK_THEME.muted_text)
                            })
                            .child(format!(
                                "{} · {line_count} lines · {diagnostic_count} diagnostics",
                                if dirty { "Modified" } else { "Saved" }
                            )),
                    ),
            )
            .when(self.search_active, |root| {
                root.child(
                    div()
                        .h(px(34.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .bg(rgb(DARK_THEME.panel))
                        .border_b_1()
                        .border_color(rgb(DARK_THEME.border))
                        .text_xs()
                        .child("Find")
                        .child(
                            div()
                                .flex_1()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(DARK_THEME.workspace))
                                .child(if self.search_query.is_empty() {
                                    "Type to search…".to_owned()
                                } else {
                                    self.search_query.clone()
                                }),
                        )
                        .child(format!(
                            "{} / {}",
                            if self.search_matches.is_empty() {
                                0
                            } else {
                                self.search_index + 1
                            },
                            self.search_matches.len()
                        )),
                )
            })
            .child(
                uniform_list(
                    "weave-source-lines",
                    line_count,
                    cx.processor(move |this, range: Range<usize>, _window, cx| {
                        range
                            .map(|line| {
                                div()
                                    .id(("source-line-hit", line))
                                    .child(this.render_line(line, focused))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.click_line(line, window, cx);
                                    }))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1(),
            )
            .when(!diagnostics.is_empty(), |root| {
                root.child(
                    div()
                        .id("text-diagnostics-scroll")
                        .flex_none()
                        .max_h(px(116.0))
                        .overflow_y_scroll()
                        .bg(rgb(DARK_THEME.panel))
                        .border_t_1()
                        .border_color(rgb(DARK_THEME.border))
                        .children(diagnostics.into_iter().enumerate().map(
                            |(index, diagnostic)| {
                                div()
                                    .id(("diagnostic", index))
                                    .px_3()
                                    .py_1()
                                    .flex()
                                    .gap_2()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(DARK_THEME.chrome)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.goto_diagnostic(index, cx);
                                    }))
                                    .text_xs()
                                    .child(
                                        div()
                                            .text_color(rgb(0xff8a9d))
                                            .child(diagnostic.code.to_string()),
                                    )
                                    .child(diagnostic.message)
                            },
                        )),
                )
            })
            .child(
                div()
                    .h(px(24.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .bg(rgb(DARK_THEME.chrome))
                    .border_t_1()
                    .border_color(rgb(DARK_THEME.border))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(self.message.clone()),
            )
    }
}

fn source_segment(text: &str, token: Option<SyntaxToken>, selected: bool) -> AnyElement {
    let color = token.map_or(DARK_THEME.text, |token| syntax_color(token.kind));
    div()
        .text_color(rgb(color))
        .when(selected, |segment| segment.bg(rgba(0x7c9cff55)))
        .child(text.to_owned())
        .into_any_element()
}

const fn syntax_color(kind: SyntaxKind) -> u32 {
    match kind {
        SyntaxKind::Comment => 0x6f7b91,
        SyntaxKind::KnotHeader => 0x8fb0ff,
        SyntaxKind::Keyword => 0xff8fbd,
        SyntaxKind::String => 0xa6d189,
        SyntaxKind::Number => 0xf2c57c,
        SyntaxKind::GrammarReference => 0x6fd3c5,
        SyntaxKind::PatternCall => 0xc6a0f6,
        SyntaxKind::Divert => 0x7c9cff,
        SyntaxKind::ChoiceMarker => 0xf4b860,
        SyntaxKind::Punctuation => 0x99a3b5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_palette_keeps_semantic_classes_distinct() {
        let colors = [
            syntax_color(SyntaxKind::Comment),
            syntax_color(SyntaxKind::KnotHeader),
            syntax_color(SyntaxKind::Keyword),
            syntax_color(SyntaxKind::String),
            syntax_color(SyntaxKind::GrammarReference),
            syntax_color(SyntaxKind::PatternCall),
            syntax_color(SyntaxKind::Divert),
            syntax_color(SyntaxKind::ChoiceMarker),
        ];
        let unique = colors.into_iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), colors.len());
    }
}
