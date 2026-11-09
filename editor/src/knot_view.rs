//! Rich and source presentation for one knot, with remembered modes.

use gpui::{App, Context, EventEmitter, KeyDownEvent, Window, div, prelude::*, px, rgb};
use weave_core::ast::Document;

use crate::knot_editor::{
    KnotBlock, KnotBlockKind, KnotViewMode, KnotViewPreferences, blocks, knot_source,
};
use crate::sync::GraphEdit;
use crate::theme::DARK_THEME;

/// Author request produced by the knot editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnotViewEvent {
    /// Apply one canonical source edit.
    Edit(GraphEdit),
    /// Persist the mode chosen for a knot.
    ModeChanged { knot: String, mode: KnotViewMode },
    /// Reveal this knot in the raw text view.
    OpenSource(String),
}

/// Focused single-line field for one block.
#[derive(Debug, Clone)]
struct Draft {
    target: DraftTarget,
    text: String,
    cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DraftTarget {
    Prose(Vec<usize>),
    Choice(Vec<usize>),
    NewProse,
}

/// Multi-line editor over one knot body.
#[derive(Debug, Clone)]
struct SourceDraft {
    knot: String,
    text: String,
    cursor: usize,
}

impl SourceDraft {
    fn new(knot: &str, text: String) -> Self {
        let cursor = text.len();
        Self {
            knot: knot.to_owned(),
            text,
            cursor,
        }
    }

    fn insert(&mut self, value: &str) {
        self.text.insert_str(self.cursor, value);
        self.cursor += value.len();
    }

    fn backspace(&mut self) {
        if let Some((index, _)) = self.text[..self.cursor].char_indices().next_back() {
            self.text.remove(index);
            self.cursor = index;
        }
    }

    fn move_left(&mut self) {
        if let Some((index, _)) = self.text[..self.cursor].char_indices().next_back() {
            self.cursor = index;
        }
    }

    fn move_right(&mut self) {
        if let Some(character) = self.text[self.cursor..].chars().next() {
            self.cursor += character.len_utf8();
        }
    }

    fn move_vertical(&mut self, down: bool) {
        let line_start = self.text[..self.cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let column = self.text[line_start..self.cursor].chars().count();
        let target_start = if down {
            self.text[self.cursor..]
                .find('\n')
                .map(|index| self.cursor + index + 1)
        } else if line_start == 0 {
            None
        } else {
            let previous_end = line_start - 1;
            Some(
                self.text[..previous_end]
                    .rfind('\n')
                    .map_or(0, |index| index + 1),
            )
        };
        let Some(target_start) = target_start else {
            return;
        };
        let target_end = self.text[target_start..]
            .find('\n')
            .map_or(self.text.len(), |index| target_start + index);
        let mut index = target_start;
        for (seen, (offset, character)) in self.text[target_start..target_end]
            .char_indices()
            .enumerate()
        {
            if seen >= column {
                break;
            }
            index = target_start + offset + character.len_utf8();
        }
        self.cursor = index;
    }

    fn rendered(&self) -> String {
        format!(
            "{}▏{}",
            &self.text[..self.cursor],
            &self.text[self.cursor..]
        )
    }
}

/// One knot rendered as labelled blocks or as a link to its source.
pub struct KnotView {
    document: Document,
    knot: Option<String>,
    blocks: Vec<KnotBlock>,
    preferences: KnotViewPreferences,
    mode: KnotViewMode,
    draft: Option<Draft>,
    source: Option<SourceDraft>,
}

impl KnotView {
    /// Create a knot editor over a parsed document.
    pub fn new(document: &Document, preferences: KnotViewPreferences) -> Self {
        Self {
            document: document.clone(),
            knot: None,
            blocks: Vec::new(),
            preferences,
            mode: KnotViewMode::Rich,
            draft: None,
            source: None,
        }
    }

    /// Point the editor at one knot, restoring its remembered mode.
    pub fn set_knot(&mut self, document: &Document, knot: Option<&str>, cx: &mut Context<Self>) {
        let changed = self.knot.as_deref() != knot || self.document != *document;
        if !changed {
            return;
        }
        self.document = document.clone();
        self.knot = knot.map(str::to_owned);
        self.blocks = knot
            .and_then(|name| blocks(document, name))
            .unwrap_or_default();
        self.mode = knot.map_or(KnotViewMode::Rich, |name| self.preferences.mode_for(name));
        self.draft = None;
        self.source = None;
        self.prefill_source();
        cx.notify();
    }

    /// Current preferences, including the remembered per-knot modes.
    #[must_use]
    pub const fn preferences(&self) -> &KnotViewPreferences {
        &self.preferences
    }

    /// Replace the preferences, for example after loading them from disk.
    pub fn set_preferences(&mut self, preferences: KnotViewPreferences, cx: &mut Context<Self>) {
        self.preferences = preferences;
        self.mode = self
            .knot
            .as_deref()
            .map_or(KnotViewMode::Rich, |knot| self.preferences.mode_for(knot));
        cx.notify();
    }

    /// Change the preference default for knots with no remembered mode.
    pub fn set_default_mode(&mut self, mode: KnotViewMode, cx: &mut Context<Self>) {
        self.preferences.set_default_mode(mode);
        if let Some(knot) = self.knot.clone() {
            self.mode = self.preferences.mode_for(&knot);
        }
        cx.notify();
    }

    fn set_mode(&mut self, mode: KnotViewMode, cx: &mut Context<Self>) {
        self.mode = mode;
        if let Some(knot) = self.knot.clone() {
            self.preferences.set_mode_for(&knot, mode);
            cx.emit(KnotViewEvent::ModeChanged { knot, mode });
        }
        self.draft = None;
        self.source = None;
        self.prefill_source();
        cx.notify();
    }

    fn prefill_source(&mut self) {
        if self.mode != KnotViewMode::Source {
            return;
        }
        let Some(knot) = self.knot.clone() else {
            return;
        };
        if let Some(source) = knot_source(&self.document, &knot) {
            self.source = Some(SourceDraft::new(&knot, source));
        }
    }

    fn begin_prose(&mut self, path: Vec<usize>, text: String, cx: &mut Context<Self>) {
        self.draft = Some(Draft {
            target: DraftTarget::Prose(path),
            cursor: text.len(),
            text,
        });
        cx.notify();
    }

    fn begin_choice(&mut self, path: Vec<usize>, text: String, cx: &mut Context<Self>) {
        self.draft = Some(Draft {
            target: DraftTarget::Choice(path),
            cursor: text.len(),
            text,
        });
        cx.notify();
    }

    fn begin_new_prose(&mut self, cx: &mut Context<Self>) {
        self.draft = Some(Draft {
            target: DraftTarget::NewProse,
            text: String::new(),
            cursor: 0,
        });
        cx.notify();
    }

    fn add_choice(&mut self, cx: &mut Context<Self>) {
        let Some(knot) = self.knot.clone() else {
            return;
        };
        cx.emit(KnotViewEvent::Edit(GraphEdit::AddChoice {
            knot,
            label: "New branch".to_owned(),
            target: "END".to_owned(),
        }));
    }

    fn remove(&mut self, path: Vec<usize>, choice: bool, cx: &mut Context<Self>) {
        let Some(knot) = self.knot.clone() else {
            return;
        };
        let edit = if choice {
            GraphEdit::RemoveChoice { knot, path }
        } else {
            GraphEdit::RemoveStatement { knot, path }
        };
        cx.emit(KnotViewEvent::Edit(edit));
    }

    fn move_block(&mut self, path: Vec<usize>, choice: bool, delta: i32, cx: &mut Context<Self>) {
        let Some(knot) = self.knot.clone() else {
            return;
        };
        cx.emit(KnotViewEvent::Edit(block_move_edit(
            &knot, path, choice, delta,
        )));
    }

    fn draft_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_ref();
        match key {
            "escape" => {
                self.draft = None;
                cx.notify();
            }
            "enter" => self.commit_draft(cx),
            "backspace" => {
                if let Some(draft) = &mut self.draft
                    && let Some((index, _)) = draft.text[..draft.cursor].char_indices().next_back()
                {
                    draft.text.remove(index);
                    draft.cursor = index;
                }
                cx.notify();
            }
            "left" => {
                if let Some(draft) = &mut self.draft
                    && let Some((index, _)) = draft.text[..draft.cursor].char_indices().next_back()
                {
                    draft.cursor = index;
                }
                cx.notify();
            }
            "right" => {
                if let Some(draft) = &mut self.draft
                    && let Some(character) = draft.text[draft.cursor..].chars().next()
                {
                    draft.cursor += character.len_utf8();
                }
                cx.notify();
            }
            _ => {
                if event.keystroke.modifiers.control
                    || event.keystroke.modifiers.platform
                    || event.keystroke.modifiers.alt
                {
                    return;
                }
                if let Some(input) = &event.keystroke.key_char {
                    if let Some(draft) = &mut self.draft {
                        for character in input.chars().filter(|value| !value.is_control()) {
                            draft.text.insert(draft.cursor, character);
                            draft.cursor += character.len_utf8();
                        }
                    }
                    cx.notify();
                }
            }
        }
    }

    fn commit_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.as_ref() else {
            return;
        };
        let text = draft.text.trim().to_owned();
        if text.is_empty() && draft.target == DraftTarget::NewProse {
            return;
        }
        let Some(draft) = self.draft.take() else {
            return;
        };
        let Some(knot) = self.knot.clone() else {
            return;
        };
        let edit = match draft.target {
            DraftTarget::Prose(path) => GraphEdit::SetStatementText { knot, path, text },
            DraftTarget::Choice(path) => GraphEdit::SetChoiceText { knot, path, text },
            DraftTarget::NewProse => GraphEdit::AppendProse { knot, text },
        };
        cx.emit(KnotViewEvent::Edit(edit));
        cx.notify();
    }

    fn source_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let key = event.keystroke.key.as_ref();
        let command = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        if command && key == "enter" {
            self.commit_source(cx);
            return;
        }
        match key {
            "escape" => {
                self.source = None;
                self.prefill_source();
            }
            "enter" => {
                if let Some(source) = &mut self.source {
                    source.insert("\n");
                }
            }
            "tab" => {
                if let Some(source) = &mut self.source {
                    source.insert("    ");
                }
            }
            "backspace" => {
                if let Some(source) = &mut self.source {
                    source.backspace();
                }
            }
            "left" => {
                if let Some(source) = &mut self.source {
                    source.move_left();
                }
            }
            "right" => {
                if let Some(source) = &mut self.source {
                    source.move_right();
                }
            }
            "up" => {
                if let Some(source) = &mut self.source {
                    source.move_vertical(false);
                }
            }
            "down" => {
                if let Some(source) = &mut self.source {
                    source.move_vertical(true);
                }
            }
            _ => {
                if command || event.keystroke.modifiers.alt {
                    return;
                }
                if let Some(input) = &event.keystroke.key_char
                    && let Some(source) = &mut self.source
                {
                    for character in input.chars().filter(|value| !value.is_control()) {
                        let mut buffer = [0_u8; 4];
                        source.insert(character.encode_utf8(&mut buffer));
                    }
                }
            }
        }
        cx.notify();
    }

    fn commit_source(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.take() else {
            return;
        };
        cx.emit(KnotViewEvent::Edit(GraphEdit::ReplaceKnotSource {
            knot: source.knot,
            source: source.text,
        }));
        cx.notify();
    }
}

impl EventEmitter<KnotViewEvent> for KnotView {}

impl gpui::Render for KnotView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("knot-editor")
            .key_context("WeaveKnotEditor")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .bg(rgb(DARK_THEME.workspace))
            .text_color(rgb(DARK_THEME.text))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.source.is_some() {
                    this.source_key(event, cx);
                } else if this.draft.is_some() {
                    this.draft_key(event, cx);
                }
            }));

        let Some(knot) = self.knot.clone() else {
            return panel.child(
                div()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("Select a beat in the graph to edit it here."),
            );
        };

        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_base().child(knot.clone()))
                .child(mode_button(
                    "mode-rich",
                    "Rich",
                    self.mode == KnotViewMode::Rich,
                    cx.listener(|this, _, _, cx| this.set_mode(KnotViewMode::Rich, cx)),
                ))
                .child(mode_button(
                    "mode-source",
                    "Source",
                    self.mode == KnotViewMode::Source,
                    cx.listener(|this, _, _, cx| this.set_mode(KnotViewMode::Source, cx)),
                ))
                .child(
                    div()
                        .id("make-default-mode")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .cursor_pointer()
                        .bg(rgb(DARK_THEME.chrome))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let mode = this.mode;
                            this.set_default_mode(mode, cx);
                        }))
                        .child("Use as default"),
                ),
        );

        if self.mode == KnotViewMode::Source {
            let rendered = self
                .source
                .as_ref()
                .map(SourceDraft::rendered)
                .unwrap_or_default();
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child("Raw Weave body, without the knot header. Cmd/Ctrl+Enter saves."),
                )
                .child(
                    div()
                        .id("knot-source-text")
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .p_3()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(DARK_THEME.border))
                        .bg(rgb(DARK_THEME.panel))
                        .text_sm()
                        .child(rendered),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .id("save-knot-source")
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(DARK_THEME.border))
                                .bg(rgb(DARK_THEME.chrome))
                                .text_sm()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| this.commit_source(cx)))
                                .child("Save"),
                        )
                        .child(
                            div()
                                .id("revert-knot-source")
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(DARK_THEME.border))
                                .bg(rgb(DARK_THEME.chrome))
                                .text_sm()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.source = None;
                                    this.prefill_source();
                                    cx.notify();
                                }))
                                .child("Revert"),
                        )
                        .child(
                            div()
                                .id("open-source")
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(DARK_THEME.border))
                                .bg(rgb(DARK_THEME.chrome))
                                .text_sm()
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(knot) = this.knot.clone() {
                                        cx.emit(KnotViewEvent::OpenSource(knot));
                                    }
                                    cx.notify();
                                }))
                                .child("Open in Text view"),
                        ),
                );
        }

        let mut list = div().flex().flex_col().gap_1();
        for block in &self.blocks {
            list = list.child(self.block_row(block, cx));
        }
        panel = panel.child(list);

        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id("add-prose")
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(DARK_THEME.border))
                        .bg(rgb(DARK_THEME.chrome))
                        .text_sm()
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.begin_new_prose(cx)))
                        .child("+ Prose"),
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
                        .on_click(cx.listener(|this, _, _, cx| this.add_choice(cx)))
                        .child("+ Choice"),
                ),
        );

        if let Some(draft) = self.draft.clone() {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(DARK_THEME.accent))
                    .bg(rgb(DARK_THEME.panel))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(match draft.target {
                                DraftTarget::Choice(_) => "Choice label",
                                DraftTarget::Prose(_) => "Prose",
                                DraftTarget::NewProse => "New prose",
                            }),
                    )
                    .child(div().text_sm().child(format!(
                        "{}▏{}",
                        &draft.text[..draft.cursor],
                        &draft.text[draft.cursor..]
                    )))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child("Enter to save · Esc to cancel"),
                    ),
            );
        }

        panel
    }
}

impl KnotView {
    fn block_row(&self, block: &KnotBlock, cx: &mut Context<Self>) -> gpui::AnyElement {
        let path = block.path.clone();
        let block_key = path
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(".");
        let editable_path = path.clone();
        let removable_path = path.clone();
        let move_up_path = path.clone();
        let move_down_path = path;
        let is_choice = matches!(block.kind, KnotBlockKind::Choice { .. });
        let (content, editable) = match &block.kind {
            KnotBlockKind::Prose { text } => (text.clone(), true),
            KnotBlockKind::Choice { label, target, .. } => (
                format!("{label} → {}", target.as_deref().unwrap_or("END")),
                true,
            ),
            KnotBlockKind::Divert { target } => (format!("→ {target}"), false),
            KnotBlockKind::Thread { target } => (format!("↩ {target}"), false),
            KnotBlockKind::Cue { description } => (description.clone(), false),
            KnotBlockKind::Declaration { description }
            | KnotBlockKind::Assignment { description } => (description.clone(), false),
            KnotBlockKind::Conditional { branches } => {
                (format!("{branches} conditional branch(es)"), false)
            }
            KnotBlockKind::Comment { text } => (format!("// {text}"), false),
            KnotBlockKind::End => ("End of knot".to_owned(), false),
            KnotBlockKind::Blank => ("Blank line".to_owned(), false),
        };
        let inline_text = match &block.kind {
            KnotBlockKind::Prose { text } => text.clone(),
            KnotBlockKind::Choice { label, .. } => label.clone(),
            _ => String::new(),
        };
        let editable_kind = matches!(
            block.kind,
            KnotBlockKind::Prose { .. } | KnotBlockKind::Choice { .. }
        );

        let mut row = div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(DARK_THEME.panel))
            .child(
                div()
                    .w(px(88.0))
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(block.kind.title()),
            )
            .child(div().flex_1().text_sm().child(if content.is_empty() {
                "(empty)".to_owned()
            } else {
                content
            }));

        row = row
            .child(
                div()
                    .id(format!("move-block-up:{block_key}"))
                    .role(gpui::Role::Button)
                    .aria_label("Move block earlier")
                    .px_2()
                    .rounded_md()
                    .text_xs()
                    .cursor_pointer()
                    .bg(rgb(DARK_THEME.chrome))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.move_block(move_up_path.clone(), is_choice, -1, cx);
                    }))
                    .child("↑"),
            )
            .child(
                div()
                    .id(format!("move-block-down:{block_key}"))
                    .role(gpui::Role::Button)
                    .aria_label("Move block later")
                    .px_2()
                    .rounded_md()
                    .text_xs()
                    .cursor_pointer()
                    .bg(rgb(DARK_THEME.chrome))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.move_block(move_down_path.clone(), is_choice, 1, cx);
                    }))
                    .child("↓"),
            );

        if editable && editable_kind {
            let inline_text = inline_text.clone();
            row = row.child(
                div()
                    .id(format!("edit-block:{block_key}"))
                    .role(gpui::Role::Button)
                    .aria_label("Edit block")
                    .px_2()
                    .rounded_md()
                    .text_xs()
                    .cursor_pointer()
                    .bg(rgb(DARK_THEME.chrome))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if is_choice {
                            this.begin_choice(editable_path.clone(), inline_text.clone(), cx);
                        } else {
                            this.begin_prose(editable_path.clone(), inline_text.clone(), cx);
                        }
                    }))
                    .child("Edit"),
            );
        }
        if block.kind.removable() {
            row = row.child(
                div()
                    .id(format!("remove-block:{block_key}"))
                    .role(gpui::Role::Button)
                    .aria_label("Remove block")
                    .px_2()
                    .rounded_md()
                    .text_xs()
                    .cursor_pointer()
                    .bg(rgb(DARK_THEME.chrome))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove(removable_path.clone(), is_choice, cx);
                    }))
                    .child("Remove"),
            );
        }
        row.into_any_element()
    }
}

/// Canonical edit for moving one block from the knot panel.
///
/// Choices reorder among choices; every other block moves by one statement position. The panel
/// does not confirm unreachable content, so a move after a terminal `END` is refused.
fn block_move_edit(knot: &str, path: Vec<usize>, choice: bool, delta: i32) -> GraphEdit {
    if choice {
        GraphEdit::MoveChoice {
            knot: knot.to_owned(),
            path,
            delta,
        }
    } else {
        GraphEdit::MoveStatement {
            knot: knot.to_owned(),
            path,
            delta,
            confirm_after_end: false,
        }
    }
}

fn mode_button(
    id: &'static str,
    label: &'static str,
    active: bool,
    listener: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs()
        .cursor_pointer()
        .bg(rgb(if active {
            DARK_THEME.accent
        } else {
            DARK_THEME.chrome
        }))
        .when(active, |button| {
            button.text_color(rgb(DARK_THEME.workspace))
        })
        .role(gpui::Role::Tab)
        .aria_label(label)
        .aria_selected(active)
        .child(label)
        .on_click(listener)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_draft_edits_and_moves_across_lines() {
        let mut draft = SourceDraft::new("start", "one\ntwo".to_owned());
        assert_eq!(draft.cursor, 7);
        draft.move_vertical(false);
        draft.insert("!");
        assert_eq!(draft.text, "one!\ntwo");
        assert_eq!(draft.cursor, 4);
        draft.move_vertical(true);
        assert_eq!(draft.cursor, 8);
        draft.insert("\nthree");
        assert_eq!(draft.text, "one!\ntwo\nthree");
    }

    #[test]
    fn source_draft_handles_unicode_boundaries() {
        let mut draft = SourceDraft::new("start", "héllo".to_owned());
        draft.move_left();
        draft.backspace();
        assert_eq!(draft.text, "hélo");
        draft.move_right();
        assert_eq!(draft.rendered(), "hélo▏");
    }

    #[test]
    fn block_move_edits_route_choices_and_blocks_separately() {
        assert_eq!(
            block_move_edit("start", vec![1], true, 1),
            GraphEdit::MoveChoice {
                knot: "start".to_owned(),
                path: vec![1],
                delta: 1,
            }
        );
        assert_eq!(
            block_move_edit("start", vec![2], false, -1),
            GraphEdit::MoveStatement {
                knot: "start".to_owned(),
                path: vec![2],
                delta: -1,
                confirm_after_end: false,
            }
        );
    }
}
