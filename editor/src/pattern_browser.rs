//! Searchable pattern catalog, isolated preview draws, and native browser panel.

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, EventEmitter, FocusHandle, KeyDownEvent, Window, div, prelude::*, px, rgb,
};
use weave_core::ast::{Document, Span};
use weave_core::ir::{BuiltinPatternIr, PatternDrawMethodIr, PatternSystemIr, StoryIr};
use weave_core::{Diagnostic, has_errors};
use weave_patterns::{
    DrawMethod, DrawRequest, DrawResult, PatternDefinition, PatternState, PatternSystem,
    PatternValue, SeededRandom, system_from_ir,
};

use crate::theme::DARK_THEME;

/// Where a browsable pattern definition comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternOrigin {
    Builtin,
    Project,
}

impl PatternOrigin {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Builtin => "Built-in",
            Self::Project => "Project",
        }
    }
}

/// Catalog origin filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternFilter {
    All,
    Builtin,
    Project,
}

impl PatternFilter {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Builtin => "Built-ins",
            Self::Project => "Project",
        }
    }

    const fn includes(self, origin: PatternOrigin) -> bool {
        match self {
            Self::All => true,
            Self::Builtin => matches!(origin, PatternOrigin::Builtin),
            Self::Project => matches!(origin, PatternOrigin::Project),
        }
    }
}

/// One catalog entry backed directly by the executable pattern boundary.
#[derive(Debug, Clone)]
pub struct PatternRecord {
    pub id: String,
    pub origin: PatternOrigin,
    pub definition: Option<PatternDefinition>,
    pub definition_span: Option<Span>,
    pub usage_spans: Vec<Span>,
    pub validation_errors: Vec<String>,
    system: Option<Arc<dyn PatternSystem>>,
    search_text: String,
}

impl PatternRecord {
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.definition
            .as_ref()
            .map_or(self.id.as_str(), |definition| definition.name.as_str())
    }

    fn matches(&self, query: &str) -> bool {
        query.is_empty() || self.search_text.contains(query)
    }
}

/// Preview outcome kept entirely outside the active story runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternPreview {
    Draw(DrawResult),
    Error(String),
}

/// Renderer-independent pattern browser state.
#[derive(Debug, Clone)]
pub struct PatternBrowserModel {
    records: Vec<PatternRecord>,
    query: String,
    filter: PatternFilter,
    selected: Option<usize>,
    selected_element: Option<usize>,
    seed: u64,
    preview: Option<PatternPreview>,
}

impl PatternBrowserModel {
    /// Build built-in and project entries from canonical parser/compiler output.
    #[must_use]
    pub fn new(
        document: Option<&Document>,
        story: Option<&StoryIr>,
        source: &str,
        diagnostics: &[Diagnostic],
    ) -> Self {
        let records = catalog_records(document, story, source, diagnostics);
        Self {
            selected: (!records.is_empty()).then_some(0),
            records,
            query: String::new(),
            filter: PatternFilter::All,
            selected_element: None,
            seed: 0,
            preview: None,
        }
    }

    /// Rebuild the catalog while preserving the user's query, filter, selection, and seed.
    pub fn rebuild(
        &mut self,
        document: Option<&Document>,
        story: Option<&StoryIr>,
        source: &str,
        diagnostics: &[Diagnostic],
    ) {
        let selected_id = self.selected_record().map(|record| record.id.clone());
        self.records = catalog_records(document, story, source, diagnostics);
        self.selected = selected_id
            .and_then(|id| self.records.iter().position(|record| record.id == id))
            .or_else(|| (!self.records.is_empty()).then_some(0));
        self.selected_element = None;
        self.preview = None;
        self.reconcile_selection();
    }

    #[must_use]
    pub fn records(&self) -> &[PatternRecord] {
        &self.records
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub const fn filter(&self) -> PatternFilter {
        self.filter
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub fn preview(&self) -> Option<&PatternPreview> {
        self.preview.as_ref()
    }

    #[must_use]
    pub fn selected_record(&self) -> Option<&PatternRecord> {
        self.selected.and_then(|index| self.records.get(index))
    }

    #[must_use]
    pub fn selected_element(&self) -> Option<usize> {
        self.selected_element
    }

    #[must_use]
    pub fn visible_record_indices(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        self.records
            .iter()
            .enumerate()
            .filter(|(_, record)| self.filter.includes(record.origin) && record.matches(&query))
            .map(|(index, _)| index)
            .collect()
    }

    #[must_use]
    pub fn visible_element_indices(&self) -> Vec<usize> {
        let Some(definition) = self
            .selected_record()
            .and_then(|record| record.definition.as_ref())
        else {
            return Vec::new();
        };
        let query = self.query.to_lowercase();
        definition
            .elements
            .iter()
            .enumerate()
            .filter(|(_, element)| {
                query.is_empty() || element_search_text(element).contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.reconcile_selection();
    }

    pub fn set_filter(&mut self, filter: PatternFilter) {
        self.filter = filter;
        self.reconcile_selection();
    }

    pub fn select_record(&mut self, index: usize) {
        if self.records.get(index).is_some() {
            self.selected = Some(index);
            self.selected_element = None;
            self.preview = None;
        }
    }

    pub fn select_element(&mut self, index: usize) {
        if self
            .selected_record()
            .and_then(|record| record.definition.as_ref())
            .is_some_and(|definition| definition.elements.get(index).is_some())
        {
            self.selected_element = Some(index);
        }
    }

    pub fn change_seed(&mut self, delta: i64) {
        self.seed = self.seed.saturating_add_signed(delta);
        self.preview = None;
    }

    /// Draw with fresh preview-only state and entropy.
    pub fn draw(&mut self, spread: Option<String>) {
        let Some(system) = self
            .selected_record()
            .and_then(|record| record.system.clone())
        else {
            self.preview = Some(PatternPreview::Error(
                "This pattern is not executable until its validation errors are fixed".to_owned(),
            ));
            return;
        };
        let mut state = PatternState::default();
        let mut random = SeededRandom::new(self.seed, 0);
        let request = DrawRequest {
            spread,
            ..DrawRequest::default()
        };
        self.preview = Some(match system.draw(&request, &mut state, &mut random) {
            Ok(result) => PatternPreview::Draw(result),
            Err(error) => PatternPreview::Error(error.to_string()),
        });
    }

    fn reconcile_selection(&mut self) {
        let visible = self.visible_record_indices();
        if self
            .selected
            .is_none_or(|selected| !visible.contains(&selected))
        {
            self.selected = visible.first().copied();
            self.selected_element = None;
            self.preview = None;
        }
    }
}

/// Navigation emitted by source and graph links in the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternBrowserEvent {
    RevealSource(Span),
    RevealGraph(String),
}

/// Native GPUI pattern browser surface.
pub struct PatternBrowserSurface {
    model: PatternBrowserModel,
    focus: FocusHandle,
}

impl PatternBrowserSurface {
    pub fn new(
        document: Option<&Document>,
        story: Option<&StoryIr>,
        source: &str,
        diagnostics: &[Diagnostic],
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            model: PatternBrowserModel::new(document, story, source, diagnostics),
            focus: cx.focus_handle(),
        }
    }

    pub fn refresh(
        &mut self,
        document: Option<&Document>,
        story: Option<&StoryIr>,
        source: &str,
        diagnostics: &[Diagnostic],
        cx: &mut Context<Self>,
    ) {
        self.model.rebuild(document, story, source, diagnostics);
        cx.notify();
    }

    #[must_use]
    pub const fn model(&self) -> &PatternBrowserModel {
        &self.model
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return;
        }
        match event.keystroke.key.as_ref() {
            "escape" => self.model.set_query(""),
            "backspace" => {
                let mut query = self.model.query().to_owned();
                query.pop();
                self.model.set_query(query);
            }
            "enter" => self.model.draw(None),
            _ => {
                if let Some(character) = &event.keystroke.key_char {
                    let mut query = self.model.query().to_owned();
                    query.push_str(&character.to_string());
                    self.model.set_query(query);
                } else {
                    return;
                }
            }
        }
        cx.notify();
    }

    fn system_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .w(px(220.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .pr_2()
            .border_r_1()
            .border_color(rgb(DARK_THEME.border));
        let query = if self.model.query().is_empty() {
            "Search names and meanings…".to_owned()
        } else {
            self.model.query().to_owned()
        };
        list = list
            .child(
                div()
                    .id("pattern-search")
                    .h(px(28.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(DARK_THEME.border))
                    .bg(rgb(DARK_THEME.chrome))
                    .text_xs()
                    .text_color(if self.model.query().is_empty() {
                        rgb(DARK_THEME.muted_text)
                    } else {
                        rgb(DARK_THEME.text)
                    })
                    .cursor_text()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.focus.focus(window, cx);
                    }))
                    .child(query),
            )
            .child(self.filter_bar(cx));

        let selected = self.model.selected;
        let visible = self.model.visible_record_indices();
        let mut records = div()
            .id("pattern-records")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        if visible.is_empty() {
            records = records.child(
                div()
                    .p_2()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("No patterns match this filter."),
            );
        }
        for index in visible {
            let record = &self.model.records[index];
            let label = if record.display_name() == record.id {
                record.id.clone()
            } else {
                format!("{} · {}", record.id, record.display_name())
            };
            let count = record
                .definition
                .as_ref()
                .map_or(0, |definition| definition.elements.len());
            records = records.child(
                div()
                    .id(("pattern-record", index))
                    .p_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected == Some(index), |row| {
                        row.bg(rgb(DARK_THEME.chrome))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.select_record(index);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.text))
                            .child(label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(format!("{} · {count} elements", record.origin.label())),
                    ),
            );
        }
        list.child(records).into_any_element()
    }

    fn filter_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div().flex().gap_1();
        for (index, filter) in [
            PatternFilter::All,
            PatternFilter::Builtin,
            PatternFilter::Project,
        ]
        .into_iter()
        .enumerate()
        {
            row = row.child(
                div()
                    .id(("pattern-filter", index))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .when(self.model.filter() == filter, |button| {
                        button.bg(rgb(DARK_THEME.accent))
                    })
                    .when(self.model.filter() != filter, |button| {
                        button.bg(rgb(DARK_THEME.chrome))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.set_filter(filter);
                        cx.notify();
                    }))
                    .child(filter.label()),
            );
        }
        row.into_any_element()
    }

    fn definition_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(record) = self.model.selected_record().cloned() else {
            return div()
                .flex_1()
                .p_3()
                .text_sm()
                .child("Select a pattern system")
                .into_any_element();
        };
        let mut panel = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .px_3()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(DARK_THEME.text))
                            .child(record.display_name().to_owned()),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(DARK_THEME.chrome))
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(record.origin.label()),
                    ),
            );

        if let Some(span) = record.definition_span {
            panel = panel.child(
                div()
                    .flex()
                    .gap_1()
                    .child(link_button(
                        "Definition",
                        "pattern-definition",
                        cx,
                        move |cx| {
                            cx.emit(PatternBrowserEvent::RevealSource(span));
                        },
                    ))
                    .child(link_button("Graph node", "pattern-graph", cx, {
                        let id = record.id.clone();
                        move |cx| cx.emit(PatternBrowserEvent::RevealGraph(id.clone()))
                    })),
            );
        }

        for error in &record.validation_errors {
            panel = panel.child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.error))
                    .text_xs()
                    .child(error.clone()),
            );
        }

        let Some(definition) = record.definition else {
            return panel.into_any_element();
        };
        panel = panel.child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(format!(
                    "{} · duplicates {} · reversals {}",
                    draw_method_label(&definition.default_method),
                    if definition.allow_duplicates {
                        "on"
                    } else {
                        "off"
                    },
                    match definition.reversal_policy {
                        weave_patterns::ReversalPolicy::Never => "off",
                        weave_patterns::ReversalPolicy::Half => "50%",
                    }
                )),
        );
        if !definition.spreads.is_empty() {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!(
                        "Spreads: {}",
                        definition
                            .spreads
                            .values()
                            .map(|spread| format!(
                                "{} ({})",
                                spread.name,
                                spread.positions.join(" · ")
                            ))
                            .collect::<Vec<_>>()
                            .join("  ")
                    )),
            );
        }

        if let Some(element) = self
            .model
            .selected_element()
            .and_then(|index| definition.elements.get(index))
        {
            let fields = element
                .fields
                .iter()
                .map(|(name, value)| format!("{name}: {value}"))
                .collect::<Vec<_>>()
                .join(" · ");
            let reversed = if element.reversed_fields.is_empty() {
                String::new()
            } else {
                format!(
                    " · reversed: {}",
                    element
                        .reversed_fields
                        .iter()
                        .map(|(name, value)| format!("{name}: {value}"))
                        .collect::<Vec<_>>()
                        .join(" · ")
                )
            };
            panel = panel.child(
                div()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(DARK_THEME.accent))
                    .bg(rgb(DARK_THEME.chrome))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.text))
                            .child(format!("{} · {fields}{reversed}", element.id)),
                    ),
            );
        }

        let visible_elements = self.model.visible_element_indices();
        let mut elements = div()
            .id("pattern-elements")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for index in visible_elements {
            let element = &definition.elements[index];
            let name = element
                .fields
                .get("name")
                .map_or_else(|| element.id.clone(), ToString::to_string);
            let meaning = element
                .fields
                .get("meaning")
                .map_or_else(|| "No meaning field".to_owned(), ToString::to_string);
            elements = elements.child(
                div()
                    .id(("pattern-element", index))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(self.model.selected_element() == Some(index), |row| {
                        row.bg(rgb(DARK_THEME.chrome))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.select_element(index);
                        cx.notify();
                    }))
                    .child(div().text_xs().text_color(rgb(DARK_THEME.text)).child(name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(meaning),
                    ),
            );
        }
        panel = panel.child(elements);

        if !record.usage_spans.is_empty() {
            let mut uses = div().flex().gap_1();
            for (index, span) in record.usage_spans.into_iter().take(5).enumerate() {
                uses = uses.child(link_button(
                    &format!("Use {}:{}", span.line, span.column),
                    ("pattern-use", index),
                    cx,
                    move |cx| cx.emit(PatternBrowserEvent::RevealSource(span)),
                ));
            }
            panel = panel.child(uses);
        }
        panel.into_any_element()
    }

    fn preview_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .w(px(330.0))
            .h_full()
            .flex_none()
            .pl_3()
            .border_l_1()
            .border_color(rgb(DARK_THEME.border))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Preview draw"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(seed_button("−", "pattern-seed-down", -1, cx))
                            .child(
                                div()
                                    .px_2()
                                    .text_xs()
                                    .text_color(rgb(DARK_THEME.muted_text))
                                    .child(format!("Seed {}", self.model.seed())),
                            )
                            .child(seed_button("+", "pattern-seed-up", 1, cx)),
                    ),
            )
            .child(
                div()
                    .id("pattern-draw-single")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(DARK_THEME.accent))
                    .cursor_pointer()
                    .text_xs()
                    .child("Draw one")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model.draw(None);
                        cx.notify();
                    })),
            );

        if let Some(definition) = self
            .model
            .selected_record()
            .and_then(|record| record.definition.as_ref())
        {
            for (index, spread) in definition.spreads.values().enumerate() {
                let name = spread.name.clone();
                panel = panel.child(
                    div()
                        .id(("pattern-draw-spread", index))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(DARK_THEME.chrome))
                        .cursor_pointer()
                        .text_xs()
                        .child(format!(
                            "Draw {} · {}",
                            spread.name,
                            spread.positions.join(" / ")
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model.draw(Some(name.clone()));
                            cx.notify();
                        })),
                );
            }
        }

        let preview = match self.model.preview() {
            None => div()
                .p_2()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child("Preview state is isolated from Play Preview. Reusing a seed reproduces the same result.")
                .into_any_element(),
            Some(PatternPreview::Error(error)) => div()
                .p_2()
                .rounded_md()
                .bg(rgb(DARK_THEME.error))
                .text_xs()
                .child(error.clone())
                .into_any_element(),
            Some(PatternPreview::Draw(result)) => preview_result(result),
        };
        panel
            .child(
                div()
                    .id("pattern-preview-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(preview),
            )
            .into_any_element()
    }
}

impl EventEmitter<PatternBrowserEvent> for PatternBrowserSurface {}

impl gpui::Render for PatternBrowserSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("pattern-browser")
            .key_context("WeavePatterns")
            .track_focus(&self.focus)
            .size_full()
            .p_3()
            .flex()
            .gap_3()
            .bg(rgb(DARK_THEME.panel))
            .border_t_1()
            .border_color(rgb(DARK_THEME.border))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.key_down(event, cx);
            }))
            .child(self.system_list(cx))
            .child(self.definition_panel(cx))
            .child(self.preview_panel(cx))
    }
}

fn catalog_records(
    document: Option<&Document>,
    story: Option<&StoryIr>,
    source: &str,
    diagnostics: &[Diagnostic],
) -> Vec<PatternRecord> {
    let mut records = Vec::new();
    for (id, builtin, method, reversals) in [
        (
            "tarot",
            BuiltinPatternIr::Tarot,
            PatternDrawMethodIr::Uniform,
            true,
        ),
        (
            "i_ching",
            BuiltinPatternIr::IChing,
            PatternDrawMethodIr::ThreeCoin,
            false,
        ),
        (
            "elder_futhark",
            BuiltinPatternIr::ElderFuthark,
            PatternDrawMethodIr::Uniform,
            true,
        ),
    ] {
        records.push(record_from_system(
            id,
            PatternOrigin::Builtin,
            system_from_ir(
                id,
                &PatternSystemIr {
                    builtin: Some(builtin),
                    collections: BTreeMap::new(),
                    spreads: BTreeMap::new(),
                    draw_method: method,
                    allow_duplicates: false,
                    reversals,
                },
            ),
            None,
            Vec::new(),
            Vec::new(),
        ));
    }

    let Some(document) = document else {
        return records;
    };
    let executable_story = story.filter(|_| !has_errors(diagnostics));
    for declaration in document.patterns() {
        let id = declaration.node.name.clone();
        let span = declaration.span;
        let usage_spans = pattern_usage_spans(source, &id);
        let mut errors = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .span
                    .is_some_and(|error_span| spans_overlap(error_span, span))
            })
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>();
        let system = executable_story
            .and_then(|story| story.patterns.get(&id))
            .map_or_else(
                || {
                    errors.push(
                        "Pattern is unavailable until the current source compiles".to_owned(),
                    );
                    Err(weave_patterns::PatternError::InvalidAuthoredData {
                        system: id.clone(),
                        message: "current source has compiler diagnostics".to_owned(),
                    })
                },
                |ir| system_from_ir(&id, ir),
            );
        records.push(record_from_system(
            &id,
            PatternOrigin::Project,
            system,
            Some(span),
            usage_spans,
            errors,
        ));
    }
    records
}

fn record_from_system(
    id: &str,
    origin: PatternOrigin,
    system: Result<Arc<dyn PatternSystem>, weave_patterns::PatternError>,
    definition_span: Option<Span>,
    usage_spans: Vec<Span>,
    mut validation_errors: Vec<String>,
) -> PatternRecord {
    let (definition, system) = match system {
        Ok(system) => (Some(system.definition().clone()), Some(system)),
        Err(error) => {
            if validation_errors.is_empty() {
                validation_errors.push(error.to_string());
            }
            (None, None)
        }
    };
    let search_text = definition.as_ref().map_or_else(
        || id.to_lowercase(),
        |definition| {
            let mut text = format!("{} {}", id, definition.name).to_lowercase();
            for element in &definition.elements {
                text.push(' ');
                text.push_str(&element_search_text(element));
            }
            text
        },
    );
    PatternRecord {
        id: id.to_owned(),
        origin,
        definition,
        definition_span,
        usage_spans,
        validation_errors,
        system,
        search_text,
    }
}

fn element_search_text(element: &weave_patterns::PatternElement) -> String {
    let mut text = element.id.to_lowercase();
    for (name, value) in &element.fields {
        text.push(' ');
        text.push_str(&name.to_lowercase());
        text.push(' ');
        text.push_str(&value.to_string().to_lowercase());
    }
    text
}

fn pattern_usage_spans(source: &str, id: &str) -> Vec<Span> {
    let needle = format!("{id}.");
    source
        .match_indices(&needle)
        .filter(|(start, _)| {
            source[..*start]
                .chars()
                .next_back()
                .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_')
        })
        .map(|(start, _)| source_span(source, start, start + id.len()))
        .collect()
}

fn source_span(source: &str, start: usize, end: usize) -> Span {
    let before = &source[..start];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..start].chars().count() + 1;
    Span::new(start, end, line, column)
}

fn spans_overlap(left: Span, right: Span) -> bool {
    left.start < right.end && right.start < left.end
}

fn draw_method_label(method: &DrawMethod) -> String {
    match method {
        DrawMethod::Uniform => "Uniform draw".to_owned(),
        DrawMethod::Weighted { field } => format!("Weighted by {field}"),
        DrawMethod::ThreeCoin => "Three-coin draw".to_owned(),
        DrawMethod::YarrowStalks => "Yarrow-stalk draw".to_owned(),
    }
}

fn preview_result(result: &DrawResult) -> AnyElement {
    let mut panel = div().flex().flex_col().gap_1().child(
        div()
            .text_xs()
            .text_color(rgb(DARK_THEME.muted_text))
            .child(format!(
                "{}{} · {}",
                result.system,
                result
                    .spread
                    .as_ref()
                    .map_or_else(String::new, |spread| format!(" / {spread}")),
                draw_method_label(&result.method)
            )),
    );
    for entry in &result.entries {
        let name = entry.name().unwrap_or(&entry.id);
        let meaning = entry
            .meaning()
            .map_or_else(|| "No meaning field".to_owned(), PatternValue::to_string);
        panel = panel.child(
            div()
                .p_2()
                .rounded_md()
                .bg(rgb(DARK_THEME.chrome))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.text))
                        .child(format!(
                            "{}{}{}",
                            entry
                                .position
                                .as_ref()
                                .map_or_else(String::new, |position| format!("{position}: ")),
                            name,
                            if entry.reversed { " · reversed" } else { "" }
                        )),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child(meaning),
                ),
        );
    }
    panel.into_any_element()
}

fn link_button(
    label: &str,
    id: impl Into<gpui::ElementId>,
    cx: &mut Context<PatternBrowserSurface>,
    emit: impl Fn(&mut Context<PatternBrowserSurface>) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(DARK_THEME.chrome))
        .cursor_pointer()
        .text_xs()
        .text_color(rgb(DARK_THEME.accent))
        .child(label.to_owned())
        .on_click(cx.listener(move |_, _, _, cx| emit(cx)))
        .into_any_element()
}

fn seed_button(
    label: &str,
    id: &'static str,
    delta: i64,
    cx: &mut Context<PatternBrowserSurface>,
) -> AnyElement {
    div()
        .id(id)
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(rgb(DARK_THEME.chrome))
        .cursor_pointer()
        .text_xs()
        .child(label.to_owned())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.model.change_seed(delta);
            cx.notify();
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use weave_compiler::{CompileOptions, compile};
    use weave_core::parse_document;
    use weave_runtime::Story;

    use super::*;

    const CUSTOM_SOURCE: &str = r#"pattern omens {
    omens: [
        (name: "Storm Crow", meaning: ill_tidings, severity: 3),
        (name: "Sun Dog", meaning: good_fortune, severity: 1),
        (name: "Frost Wolf", meaning: harsh_winter, severity: 4),
    ]
    draw: weighted_by_severity
    spread day_omen { positions: [dawn, noon, dusk] }
}
=== start ===
VAR omen = omens.spread.day_omen.draw()
-> END
"#;

    fn custom_catalog() -> (PatternBrowserModel, Story) {
        let document = parse_document(CUSTOM_SOURCE).expect("source parses");
        let compiled = compile(CUSTOM_SOURCE, &CompileOptions::default()).expect("source compiles");
        let story = Story::with_seed(compiled.story.clone(), 91).expect("runtime starts");
        (
            PatternBrowserModel::new(
                Some(&document),
                Some(&compiled.story),
                CUSTOM_SOURCE,
                &compiled.diagnostics,
            ),
            story,
        )
    }

    #[test]
    fn catalog_uses_shared_builtins_and_authored_runtime_data() {
        let (model, _) = custom_catalog();
        assert_eq!(model.records().len(), 4);
        assert_eq!(
            model.records()[0]
                .definition
                .as_ref()
                .unwrap()
                .elements
                .len(),
            78
        );
        assert_eq!(
            model.records()[1]
                .definition
                .as_ref()
                .unwrap()
                .elements
                .len(),
            64
        );
        assert_eq!(
            model.records()[2]
                .definition
                .as_ref()
                .unwrap()
                .elements
                .len(),
            24
        );
        let custom = model
            .records()
            .iter()
            .find(|record| record.id == "omens")
            .expect("custom pattern");
        assert_eq!(custom.origin, PatternOrigin::Project);
        assert_eq!(custom.definition.as_ref().unwrap().elements.len(), 3);
        assert!(custom.definition_span.is_some());
        assert_eq!(custom.usage_spans.len(), 1);
    }

    #[test]
    fn search_filters_systems_and_elements_by_semantic_meaning() {
        let (mut model, _) = custom_catalog();
        model.set_query("ill_tidings");
        let visible = model.visible_record_indices();
        assert_eq!(visible.len(), 1);
        assert_eq!(model.records()[visible[0]].id, "omens");
        assert_eq!(model.visible_element_indices().len(), 1);
    }

    #[test]
    fn preview_draws_are_reproducible_and_do_not_mutate_play_state() {
        let (mut model, story) = custom_catalog();
        let runtime_before = story.state().clone();
        let custom = model
            .records()
            .iter()
            .position(|record| record.id == "omens")
            .expect("custom pattern");
        model.select_record(custom);
        model.change_seed(47);
        model.draw(Some("day_omen".to_owned()));
        let first = model.preview().cloned();
        model.draw(Some("day_omen".to_owned()));
        assert_eq!(model.preview(), first.as_ref());
        assert_eq!(story.state(), &runtime_before);
    }

    #[test]
    fn origin_filter_and_validation_errors_remain_visible() {
        let (mut model, _) = custom_catalog();
        model.set_filter(PatternFilter::Project);
        let visible = model.visible_record_indices();
        assert_eq!(visible.len(), 1);
        assert_eq!(model.records()[visible[0]].id, "omens");

        let invalid = r#"pattern broken {
    values: [(name: "Empty Weight", meaning: warning, weight: 0)]
    draw: weighted_by_weight
}
=== start ===
-> END
"#;
        let document = parse_document(invalid).expect("invalid semantics still parse");
        let diagnostics = compile(invalid, &CompileOptions::default())
            .expect_err("weight validation fails")
            .diagnostics;
        let invalid_model = PatternBrowserModel::new(Some(&document), None, invalid, &diagnostics);
        let broken = invalid_model
            .records()
            .iter()
            .find(|record| record.id == "broken")
            .expect("invalid declaration remains browsable");
        assert!(broken.definition.is_none());
        assert!(!broken.validation_errors.is_empty());
    }
}
