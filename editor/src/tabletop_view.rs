//! GPUI surface for one manifest-gated tabletop panel session.
//!
//! The surface never matches an adapter id. It renders whatever panels the session exposes and
//! shows `TT106` when a capability is absent. Field values live in small text buffers; each edit
//! parses to the declared [`PanelFieldKind`] and reports the exact path and reason on failure.

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, EventEmitter, FocusHandle, KeyDownEvent, Window, div, prelude::*, px,
    rgb,
};
use weave_tabletop::{
    EventVisibility, TabletopCapability, TabletopError, TabletopEvent, has_capability,
};

use crate::tabletop::tabletop_panels::{
    ModulePanelOutcome, ModulePanelView, PanelCreationPreview, PanelDescriptor, PanelField,
    PanelFieldKind, PanelForm, PanelOperation, PanelValue, TabletopPanelAdapter,
    TabletopPanelSession,
};
use crate::theme::DARK_THEME;

/// Documented keyboard shortcuts shown at the bottom of the surface.
pub const TABLETOP_KEYBOARD_HINT: &str =
    "Tab/Shift+Tab move · Enter activate · Esc cancel edit · arrows move focus";

/// Host notification emitted after a visible change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabletopViewEvent {
    /// Anything repaintable changed: selection, focus, field text, notice, or log.
    Changed,
    /// Creation was accepted and the play state is open.
    CreationAccepted,
    /// One declared operation produced a receipt.
    OperationExecuted { operation: String, events: usize },
    /// One retained transition replayed with an identical receipt.
    Replayed { index: usize },
}

/// One parse or validation failure tied to a declared field path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabletopFieldError {
    pub path: String,
    pub reason: String,
}

impl std::fmt::Display for TabletopFieldError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

/// Raw buffers and parsed values for one panel form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelDraftState {
    values: BTreeMap<String, PanelValue>,
    buffers: BTreeMap<String, String>,
    errors: BTreeMap<String, String>,
}

impl PanelDraftState {
    /// Start from an existing form and the panel fields it belongs to.
    #[must_use]
    pub fn new(form: &PanelForm, fields: &[PanelField]) -> Self {
        let mut state = Self::default();
        for field in fields {
            let Some(value) = form.get(field.path) else {
                continue;
            };
            state
                .buffers
                .insert(field.path.to_owned(), value_text(value));
            state.values.insert(field.path.to_owned(), value.clone());
        }
        state
    }

    /// Current raw text for one path, or the empty string.
    #[must_use]
    pub fn buffer(&self, path: &str) -> &str {
        self.buffers.get(path).map_or("", String::as_str)
    }

    /// Parsed value for one path, if the last edit was valid.
    #[must_use]
    pub fn value(&self, path: &str) -> Option<&PanelValue> {
        self.values.get(path)
    }

    /// Current failure reason for one path, if any.
    #[must_use]
    pub fn error(&self, path: &str) -> Option<&str> {
        self.errors.get(path).map(String::as_str)
    }

    /// First remaining field failure, ordered by path.
    #[must_use]
    pub fn first_error(&self) -> Option<(&str, &str)> {
        self.errors
            .iter()
            .next()
            .map(|(path, reason)| (path.as_str(), reason.as_str()))
    }

    /// Parse and store one edited buffer.
    ///
    /// Invalid input keeps the previous parsed value and reports the exact path and reason. An
    /// empty optional field removes its value; an empty required field is reported as missing.
    pub fn edit(&mut self, field: &PanelField, raw: &str) -> Result<(), TabletopFieldError> {
        self.buffers.insert(field.path.to_owned(), raw.to_owned());
        match parse_field(field, raw) {
            Ok(Some(value)) => {
                self.values.insert(field.path.to_owned(), value);
                self.errors.remove(field.path);
                Ok(())
            }
            Ok(None) => {
                self.values.remove(field.path);
                self.errors.remove(field.path);
                Ok(())
            }
            Err(reason) => {
                self.errors.insert(field.path.to_owned(), reason.clone());
                Err(TabletopFieldError {
                    path: field.path.to_owned(),
                    reason,
                })
            }
        }
    }

    /// Build the validated-shape form the session API accepts.
    #[must_use]
    pub fn form(&self) -> PanelForm {
        let mut form = PanelForm::new();
        for (path, value) in &self.values {
            form = match value {
                PanelValue::Text(value) => form.with_text(path, value),
                PanelValue::Integer(value) => form.with_integer(path, *value),
                PanelValue::Boolean(value) => form.with_boolean(path, *value),
                PanelValue::Choice(value) => form.with_choice(path, value),
            };
        }
        form
    }
}

/// Parse one raw field buffer into the declared value shape.
///
/// Returns `Ok(None)` for an empty optional field. Errors carry a plain reason; the caller adds
/// the field path to the report.
pub fn parse_field(field: &PanelField, raw: &str) -> Result<Option<PanelValue>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        if field.required {
            return Err("required field is missing".to_owned());
        }
        return Ok(None);
    }
    match field.kind {
        PanelFieldKind::Text => Ok(Some(PanelValue::Text(raw.to_owned()))),
        PanelFieldKind::Unsigned => match raw.parse::<i64>() {
            Ok(value) if value >= 0 => Ok(Some(PanelValue::Integer(value))),
            Ok(_) | Err(_) => Err("expected a non-negative whole number".to_owned()),
        },
        PanelFieldKind::Signed => raw
            .parse::<i64>()
            .map(|value| Some(PanelValue::Integer(value)))
            .map_err(|_| "expected a whole number".to_owned()),
        PanelFieldKind::Boolean => match raw.to_ascii_lowercase().as_str() {
            "true" => Ok(Some(PanelValue::Boolean(true))),
            "false" => Ok(Some(PanelValue::Boolean(false))),
            _ => Err("expected true or false".to_owned()),
        },
        PanelFieldKind::Choice(options) => {
            if options.contains(&raw) {
                Ok(Some(PanelValue::Choice(raw.to_owned())))
            } else {
                Err(format!("expected one of: {}", options.join(", ")))
            }
        }
    }
}

/// Panels the session exposes, with only operations whose capabilities its manifest declares.
#[must_use]
pub fn visible_panels(
    adapter: &dyn TabletopPanelAdapter,
    session: &TabletopPanelSession,
) -> Vec<(PanelDescriptor, Vec<PanelOperation>)> {
    let declared = session.panels();
    adapter
        .panels()
        .into_iter()
        .filter(|panel| {
            declared
                .iter()
                .any(|summary| summary.capability == panel.capability)
        })
        .map(|panel| {
            let operations = panel
                .operations
                .iter()
                .filter(|operation| has_capability(session.manifest(), operation.capability))
                .cloned()
                .collect();
            (panel, operations)
        })
        .collect()
}

/// One line in the event log.
#[must_use]
pub fn event_line(event: &TabletopEvent) -> String {
    format!(
        "#{} · {} · {} · {}",
        event.sequence,
        event.kind,
        capability_label(event.capability),
        visibility_label(event.visibility)
    )
}

/// Author-facing name for one capability.
#[must_use]
pub const fn capability_label(capability: TabletopCapability) -> &'static str {
    match capability {
        TabletopCapability::CharacterCreation => "Character creation",
        TabletopCapability::DerivedValues => "Derived values",
        TabletopCapability::ChecksAndConflicts => "Checks and conflicts",
        TabletopCapability::ResourcesAndConditions => "Resources and conditions",
        TabletopCapability::EquipmentAndAbilities => "Equipment and abilities",
        TabletopCapability::Advancement => "Advancement",
        TabletopCapability::Encounters => "Encounters",
        TabletopCapability::Clocks => "Clocks",
        TabletopCapability::Scenes => "Scenes",
        TabletopCapability::Factions => "Factions",
        TabletopCapability::WorldState => "World state",
        TabletopCapability::CampaignState => "Campaign state",
    }
}

fn visibility_label(visibility: EventVisibility) -> &'static str {
    match visibility {
        EventVisibility::Public => "public",
        EventVisibility::Authoring => "authoring",
        EventVisibility::HostOnly => "host only",
    }
}

fn value_text(value: &PanelValue) -> String {
    match value {
        PanelValue::Text(value) | PanelValue::Choice(value) => value.clone(),
        PanelValue::Integer(value) => value.to_string(),
        PanelValue::Boolean(value) => value.to_string(),
    }
}

fn kind_label(kind: PanelFieldKind) -> String {
    match kind {
        PanelFieldKind::Text => "text".to_owned(),
        PanelFieldKind::Unsigned => "whole number".to_owned(),
        PanelFieldKind::Signed => "number".to_owned(),
        PanelFieldKind::Boolean => "true or false".to_owned(),
        PanelFieldKind::Choice(options) => format!("one of {}", options.join(", ")),
    }
}

fn focus_border(focused: bool) -> u32 {
    if focused {
        DARK_THEME.accent
    } else {
        DARK_THEME.border
    }
}

const fn control_background(focused: bool) -> u32 {
    if focused {
        DARK_THEME.accent
    } else {
        DARK_THEME.chrome
    }
}

/// One control in the surface's single logical focus order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabletopFocus {
    Panel(usize),
    CreationField(usize),
    PreviewCreation,
    AcceptCreation,
    Operation(usize),
    OperationField(usize),
    ExecuteOperation,
    ModuleField(usize),
    ApplyModules,
    Replay(usize),
}

impl TabletopFocus {
    /// Author-facing label announced with the focus.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Panel(index) => format!("Panel {}", index + 1),
            Self::CreationField(index) => format!("Creation field {}", index + 1),
            Self::PreviewCreation => "Preview creation".to_owned(),
            Self::AcceptCreation => "Accept creation".to_owned(),
            Self::Operation(index) => format!("Operation {}", index + 1),
            Self::OperationField(index) => format!("Operation field {}", index + 1),
            Self::ExecuteOperation => "Execute operation".to_owned(),
            Self::ModuleField(index) => format!("Module field {}", index + 1),
            Self::ApplyModules => "Apply modules".to_owned(),
            Self::Replay(index) => format!("Replay record {}", index + 1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldTarget {
    Creation(PanelField),
    Operation {
        operation: &'static str,
        field: PanelField,
    },
    Module(PanelField),
}

impl FieldTarget {
    fn label(self) -> String {
        match self {
            Self::Creation(field) => format!("{} ({})", field.label, field.path),
            Self::Operation { operation, field } => {
                format!("{operation} · {} ({})", field.label, field.path)
            }
            Self::Module(field) => format!("{} ({})", field.label, field.path),
        }
    }
}

#[derive(Debug, Clone)]
struct FieldDraft {
    target: FieldTarget,
    text: String,
    cursor: usize,
}

impl FieldDraft {
    fn new(target: FieldTarget, text: String) -> Self {
        let cursor = text.len();
        Self {
            target,
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

    fn rendered(&self) -> String {
        format!(
            "{}▏{}",
            &self.text[..self.cursor],
            &self.text[self.cursor..]
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Notice {
    Success(String),
    Warning(String),
    Error(String),
}

fn adapter_matches_session(
    adapter: &dyn TabletopPanelAdapter,
    session: &TabletopPanelSession,
) -> bool {
    adapter.adapter_id() == session.manifest().id
        && adapter.adapter_version() == session.manifest().version
}

/// Explain why accepting the current creation would use stale data, if it would.
fn acceptance_blocker(
    creation_dirty: bool,
    preview_present: bool,
    first_error: Option<(&str, &str)>,
) -> Option<String> {
    if let Some((path, reason)) = first_error {
        return Some(format!(
            "{path}: {reason}. Fix the field before accepting the preview."
        ));
    }
    if creation_dirty {
        return Some(
            "The creation form has unapplied edits. Build a fresh preview before accepting."
                .to_owned(),
        );
    }
    if !preview_present {
        return Some("editor.acceptance: build a preview before accepting".to_owned());
    }
    None
}

/// Manifest-gated tabletop authoring and play surface.
pub struct TabletopView {
    adapter: Option<Arc<dyn TabletopPanelAdapter>>,
    session: Option<TabletopPanelSession>,
    focus: FocusHandle,
    focus_target: TabletopFocus,
    creation: PanelDraftState,
    creation_dirty: bool,
    preview: Option<PanelCreationPreview>,
    selected_panel: Option<TabletopCapability>,
    selected_operation: Option<&'static str>,
    operation_drafts: BTreeMap<&'static str, PanelDraftState>,
    module: PanelDraftState,
    module_outcome: Option<ModulePanelOutcome>,
    notice: Option<Notice>,
    draft: Option<FieldDraft>,
}

impl TabletopView {
    /// Create an empty surface before an adapter is selected.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            adapter: None,
            session: None,
            focus: cx.focus_handle(),
            focus_target: TabletopFocus::Panel(0),
            creation: PanelDraftState::default(),
            creation_dirty: false,
            preview: None,
            selected_panel: None,
            selected_operation: None,
            operation_drafts: BTreeMap::new(),
            module: PanelDraftState::default(),
            module_outcome: None,
            notice: None,
            draft: None,
        }
    }

    /// Install one adapter and resolver registry, then show its declared panels.
    pub fn set_adapter(
        &mut self,
        adapter: Arc<dyn TabletopPanelAdapter>,
        registry: Arc<weave_tabletop::ResolverRegistry>,
        cx: &mut Context<Self>,
    ) -> Result<(), TabletopError> {
        let session = TabletopPanelSession::new(adapter.clone(), registry)?;
        self.install(adapter, session, cx);
        Ok(())
    }

    /// Install an already constructed session together with the adapter that supplies its forms.
    ///
    /// The adapter is required because [`TabletopPanelSession`] exposes operation ids but not the
    /// declared operation fields.
    pub fn set_session(
        &mut self,
        adapter: Arc<dyn TabletopPanelAdapter>,
        session: TabletopPanelSession,
        cx: &mut Context<Self>,
    ) -> Result<(), TabletopError> {
        if !adapter_matches_session(adapter.as_ref(), &session) {
            return Err(TabletopError::ResolverMismatch);
        }
        self.install(adapter, session, cx);
        Ok(())
    }

    /// Current session, if one is installed.
    #[must_use]
    pub fn session(&self) -> Option<&TabletopPanelSession> {
        self.session.as_ref()
    }

    /// Currently focused control.
    #[must_use]
    pub const fn focus_target(&self) -> &TabletopFocus {
        &self.focus_target
    }

    /// Move window focus onto the surface so its keyboard order is reachable.
    pub fn focus_surface(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
    }

    fn install(
        &mut self,
        adapter: Arc<dyn TabletopPanelAdapter>,
        session: TabletopPanelSession,
        cx: &mut Context<Self>,
    ) {
        self.adapter = Some(adapter);
        self.session = Some(session);
        self.reset_state();
        self.focus_target = TabletopFocus::Panel(0);
        cx.notify();
        cx.emit(TabletopViewEvent::Changed);
    }

    fn reset_state(&mut self) {
        self.creation = PanelDraftState::default();
        self.creation_dirty = false;
        self.preview = None;
        self.selected_panel = None;
        self.selected_operation = None;
        self.operation_drafts.clear();
        self.module = PanelDraftState::default();
        self.module_outcome = None;
        self.notice = None;
        self.draft = None;

        let Some(session) = self.session.as_ref() else {
            return;
        };
        let creation_fields = session
            .creation_panel()
            .map(|panel| panel.fields)
            .unwrap_or_default();
        self.creation = PanelDraftState::new(session.creation_form(), &creation_fields);

        if let Some(adapter) = self.adapter.as_ref() {
            let rows = visible_panels(adapter.as_ref(), session);
            self.selected_panel = rows.first().map(|(panel, _)| panel.capability);
            self.selected_operation = rows
                .first()
                .and_then(|(_, operations)| operations.first())
                .map(|operation| operation.id);
            for (_, operations) in rows {
                for operation in operations {
                    self.operation_drafts.insert(
                        operation.id,
                        PanelDraftState::new(&PanelForm::new(), &operation.fields),
                    );
                }
            }
        }

        if let Ok(panel) = session.module_panel() {
            self.module = PanelDraftState::new(&PanelForm::new(), &panel.fields);
        }
    }

    fn panel_rows(&self) -> Vec<(PanelDescriptor, Vec<PanelOperation>)> {
        match (&self.adapter, &self.session) {
            (Some(adapter), Some(session)) => visible_panels(adapter.as_ref(), session),
            _ => Vec::new(),
        }
    }

    fn selected_operation(&self) -> Option<PanelOperation> {
        let operation_id = self.selected_operation?;
        self.panel_rows()
            .into_iter()
            .find_map(|(panel, operations)| {
                if Some(panel.capability) != self.selected_panel {
                    return None;
                }
                operations
                    .into_iter()
                    .find(|operation| operation.id == operation_id)
            })
    }

    fn focus_order(&self) -> Vec<TabletopFocus> {
        let mut order = Vec::new();
        let rows = self.panel_rows();
        for index in 0..rows.len() {
            order.push(TabletopFocus::Panel(index));
        }

        if let Some(panel) = self
            .session
            .as_ref()
            .and_then(|session| session.creation_panel().ok())
        {
            for index in 0..panel.fields.len() {
                order.push(TabletopFocus::CreationField(index));
            }
            order.push(TabletopFocus::PreviewCreation);
            order.push(TabletopFocus::AcceptCreation);
        }

        if let Some((_, operations)) = rows
            .iter()
            .find(|(panel, _)| Some(panel.capability) == self.selected_panel)
        {
            for index in 0..operations.len() {
                order.push(TabletopFocus::Operation(index));
            }
            if self.selected_operation().is_some() {
                let field_count = self
                    .selected_operation()
                    .map_or(0, |operation| operation.fields.len());
                for index in 0..field_count {
                    order.push(TabletopFocus::OperationField(index));
                }
                order.push(TabletopFocus::ExecuteOperation);
            }
        }

        if self
            .session
            .as_ref()
            .is_some_and(|session| session.module_panel().is_ok())
        {
            let field_count = self.module_field_count();
            for index in 0..field_count {
                order.push(TabletopFocus::ModuleField(index));
            }
            order.push(TabletopFocus::ApplyModules);
        }

        if let Some(session) = self.session.as_ref() {
            for index in 0..session.records().len() {
                order.push(TabletopFocus::Replay(index));
            }
        }
        order
    }

    fn module_field_count(&self) -> usize {
        self.session
            .as_ref()
            .and_then(|session| session.module_panel().ok())
            .map_or(0, |panel| panel.fields.len())
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.draft.is_some() {
            self.draft_key(event, cx);
            return;
        }
        let key = event.keystroke.key.as_ref();
        let command = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        if command || event.keystroke.modifiers.alt {
            return;
        }
        let order = self.focus_order();
        if order.is_empty() {
            return;
        }
        match key {
            "tab" => self.step_focus(&order, !event.keystroke.modifiers.shift, cx),
            "down" | "right" => self.step_focus(&order, true, cx),
            "up" | "left" => self.step_focus(&order, false, cx),
            "home" => {
                self.focus_target = order[0].clone();
                self.changed(cx);
            }
            "end" => {
                if let Some(last) = order.last() {
                    self.focus_target = last.clone();
                }
                self.changed(cx);
            }
            "enter" | "space" => {
                cx.stop_propagation();
                self.activate(cx);
            }
            _ => {}
        }
    }

    fn step_focus(&mut self, order: &[TabletopFocus], forward: bool, cx: &mut Context<Self>) {
        let index = order
            .iter()
            .position(|target| *target == self.focus_target)
            .map_or(0, |index| {
                if forward {
                    (index + 1) % order.len()
                } else {
                    (index + order.len() - 1) % order.len()
                }
            });
        self.focus_target = order[index].clone();
        self.changed(cx);
    }

    fn activate(&mut self, cx: &mut Context<Self>) {
        match self.focus_target.clone() {
            TabletopFocus::Panel(index) => self.select_panel(index, cx),
            TabletopFocus::CreationField(index) => self.begin_creation_field(index, cx),
            TabletopFocus::PreviewCreation => self.preview(cx),
            TabletopFocus::AcceptCreation => self.accept(cx),
            TabletopFocus::Operation(index) => self.select_operation(index, cx),
            TabletopFocus::OperationField(index) => self.begin_operation_field(index, cx),
            TabletopFocus::ExecuteOperation => self.execute(cx),
            TabletopFocus::ModuleField(index) => self.begin_module_field(index, cx),
            TabletopFocus::ApplyModules => self.apply_modules(cx),
            TabletopFocus::Replay(index) => self.replay(index, cx),
        }
    }

    fn select_panel(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((panel, operations)) = self.panel_rows().into_iter().nth(index) else {
            return;
        };
        self.selected_panel = Some(panel.capability);
        self.selected_operation = operations.first().map(|operation| operation.id);
        self.notice = None;
        self.changed(cx);
    }

    fn select_operation(&mut self, index: usize, cx: &mut Context<Self>) {
        let operation = self
            .panel_rows()
            .into_iter()
            .find(|(panel, _)| Some(panel.capability) == self.selected_panel)
            .and_then(|(_, operations)| operations.into_iter().nth(index));
        let Some(operation) = operation else {
            return;
        };
        self.operation_drafts
            .entry(operation.id)
            .or_insert_with(|| PanelDraftState::new(&PanelForm::new(), &operation.fields));
        self.selected_operation = Some(operation.id);
        self.notice = None;
        self.changed(cx);
    }

    fn begin_creation_field(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(field) = self
            .session
            .as_ref()
            .and_then(|session| session.creation_panel().ok())
            .and_then(|panel| panel.fields.get(index).copied())
        else {
            return;
        };
        let text = self.creation.buffer(field.path).to_owned();
        self.draft = Some(FieldDraft::new(FieldTarget::Creation(field), text));
        self.changed(cx);
    }

    fn begin_operation_field(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(operation) = self.selected_operation() else {
            return;
        };
        let Some(field) = operation.fields.get(index).copied() else {
            return;
        };
        let text = self
            .operation_drafts
            .get(operation.id)
            .map_or("", |state| state.buffer(field.path))
            .to_owned();
        self.draft = Some(FieldDraft::new(
            FieldTarget::Operation {
                operation: operation.id,
                field,
            },
            text,
        ));
        self.changed(cx);
    }

    fn begin_module_field(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(field) = self
            .session
            .as_ref()
            .and_then(|session| session.module_panel().ok())
            .and_then(|panel| panel.fields.get(index).copied())
        else {
            return;
        };
        let text = self.module.buffer(field.path).to_owned();
        self.draft = Some(FieldDraft::new(FieldTarget::Module(field), text));
        self.changed(cx);
    }

    fn draft_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let key = event.keystroke.key.as_ref();
        match key {
            "escape" => {
                self.draft = None;
            }
            "enter" => self.commit_draft(),
            "backspace" => {
                if let Some(draft) = &mut self.draft {
                    draft.backspace();
                }
            }
            "left" => {
                if let Some(draft) = &mut self.draft {
                    draft.move_left();
                }
            }
            "right" => {
                if let Some(draft) = &mut self.draft {
                    draft.move_right();
                }
            }
            _ => {
                if event.keystroke.modifiers.control
                    || event.keystroke.modifiers.platform
                    || event.keystroke.modifiers.alt
                {
                    return;
                }
                if let Some(input) = &event.keystroke.key_char
                    && let Some(draft) = &mut self.draft
                {
                    for character in input.chars().filter(|value| !value.is_control()) {
                        let mut buffer = [0_u8; 4];
                        draft.insert(character.encode_utf8(&mut buffer));
                    }
                }
            }
        }
        self.changed(cx);
    }

    fn commit_draft(&mut self) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        match draft.target {
            FieldTarget::Creation(field) => {
                self.creation_dirty = true;
                if let Err(error) = self.creation.edit(&field, &draft.text) {
                    self.preview = None;
                    self.notice = Some(Notice::Error(error.to_string()));
                } else {
                    self.preview = None;
                    self.notice = None;
                    self.sync_creation_form();
                }
            }
            FieldTarget::Operation { operation, field } => {
                match self.operation_drafts.get_mut(operation) {
                    Some(state) => match state.edit(&field, &draft.text) {
                        Ok(()) => self.notice = None,
                        Err(error) => self.notice = Some(Notice::Error(error.to_string())),
                    },
                    None => {
                        self.notice = Some(Notice::Error(format!(
                            "{}: the operation form is not loaded",
                            field.path
                        )));
                    }
                }
            }
            FieldTarget::Module(field) => {
                if let Err(error) = self.module.edit(&field, &draft.text) {
                    self.notice = Some(Notice::Error(error.to_string()));
                } else {
                    self.module_outcome = None;
                    self.notice = None;
                }
            }
        }
    }

    fn sync_creation_form(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Ok(panel) = session.creation_panel() else {
            return;
        };
        let form = self.creation.form();
        if form.validate(&panel.fields).is_ok() && session.update_creation_form(form).is_ok() {
            self.creation_dirty = false;
            self.preview = None;
        }
    }

    fn preview(&mut self, cx: &mut Context<Self>) {
        if let Some((path, reason)) = self.creation.first_error() {
            self.notice = Some(Notice::Error(format!("{path}: {reason}")));
            self.changed(cx);
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let form = self.creation.form();
        let result = session
            .update_creation_form(form)
            .and_then(|()| session.preview_creation().cloned());
        match result {
            Ok(preview) => {
                self.creation_dirty = false;
                self.preview = Some(preview);
                self.notice = Some(Notice::Success(
                    "Creation preview built. Review it before accepting.".to_owned(),
                ));
            }
            Err(error) => {
                self.preview = None;
                self.notice = Some(Notice::Error(error.to_string()));
            }
        }
        self.changed(cx);
    }

    fn accept(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if let Some(message) = acceptance_blocker(
            self.creation_dirty,
            self.preview.is_some(),
            self.creation.first_error(),
        ) {
            self.notice = Some(Notice::Error(message));
            self.changed(cx);
            return;
        }
        match session.accept_creation() {
            Ok(()) => {
                self.notice = Some(Notice::Success(
                    "Creation accepted. Play operations are enabled.".to_owned(),
                ));
                cx.emit(TabletopViewEvent::CreationAccepted);
            }
            Err(error) => self.notice = Some(Notice::Error(error.to_string())),
        }
        self.changed(cx);
    }

    fn execute(&mut self, cx: &mut Context<Self>) {
        let Some(operation) = self.selected_operation() else {
            return;
        };
        if self.creation_dirty {
            self.notice = Some(Notice::Error(
                "editor.creation: apply the creation form before running operations".to_owned(),
            ));
            self.changed(cx);
            return;
        }
        let Some(state) = self.operation_drafts.get(operation.id) else {
            return;
        };
        if let Some((path, reason)) = state.first_error() {
            self.notice = Some(Notice::Error(format!("{path}: {reason}")));
            self.changed(cx);
            return;
        }
        let form = state.form();
        if let Err(error) = form.validate(&operation.fields) {
            self.notice = Some(Notice::Error(error.to_string()));
            self.changed(cx);
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match session.execute_operation(operation.id, &form) {
            Ok(outcome) => {
                let events = outcome.events.len();
                self.notice = Some(Notice::Success(format!(
                    "{} produced {events} event(s).",
                    operation.title
                )));
                cx.emit(TabletopViewEvent::OperationExecuted {
                    operation: operation.id.to_owned(),
                    events,
                });
            }
            Err(error) => self.notice = Some(Notice::Error(error.to_string())),
        }
        self.changed(cx);
    }

    fn apply_modules(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let panel = match session.module_panel() {
            Ok(panel) => panel,
            Err(error) => {
                self.notice = Some(Notice::Error(error.to_string()));
                self.changed(cx);
                return;
            }
        };
        if let Some((path, reason)) = self.module.first_error() {
            self.notice = Some(Notice::Error(format!("{path}: {reason}")));
            self.changed(cx);
            return;
        }
        let form = self.module.form();
        if let Err(error) = form.validate(&panel.fields) {
            self.notice = Some(Notice::Error(error.to_string()));
            self.changed(cx);
            return;
        }
        match session.apply_modules(&form) {
            Ok(outcome) => {
                let enabled = outcome.enabled_modules.len();
                let diagnostics = outcome.diagnostics.len();
                self.module_outcome = Some(outcome);
                self.notice = Some(if diagnostics == 0 {
                    Notice::Success(format!(
                        "Module review complete: {enabled} module(s) enabled."
                    ))
                } else {
                    Notice::Warning(format!(
                        "Module review found {diagnostics} diagnostic(s); no module was enabled."
                    ))
                });
            }
            Err(error) => self.notice = Some(Notice::Error(error.to_string())),
        }
        self.changed(cx);
    }

    fn replay(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        match session.replay_record(index) {
            Ok(()) => {
                self.notice = Some(Notice::Success(format!(
                    "Record {} replayed with an identical receipt.",
                    index + 1
                )));
                cx.emit(TabletopViewEvent::Replayed { index });
            }
            Err(error) => self.notice = Some(Notice::Error(error.to_string())),
        }
        self.changed(cx);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        cx.emit(TabletopViewEvent::Changed);
    }

    fn panel_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.panel_rows();
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(div().text_base().child("Panels"));
        if rows.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("This adapter declares no play or progression panels."),
            );
        }
        let mut list = div().flex().flex_col().gap_1();
        for (index, (panel, operations)) in rows.iter().enumerate() {
            let focused = self.focus_target == TabletopFocus::Panel(index);
            let selected = self.selected_panel == Some(panel.capability);
            let operation_titles = if operations.is_empty() {
                "No declared operations".to_owned()
            } else {
                format!(
                    "Operations: {}",
                    operations
                        .iter()
                        .map(|operation| operation.title)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            list = list.child(
                div()
                    .id(("tabletop-panel", index))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(focus_border(focused)))
                    .bg(rgb(if selected {
                        DARK_THEME.accent
                    } else {
                        DARK_THEME.chrome
                    }))
                    .role(gpui::Role::Tab)
                    .aria_label(panel.title.to_string())
                    .aria_selected(selected)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.select_panel(index, cx)))
                    .child(div().text_sm().child(panel.title.to_owned()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(format!(
                                "{} · {}",
                                capability_label(panel.capability),
                                panel.description
                            )),
                    )
                    .child(div().text_xs().child(operation_titles)),
            );
        }
        section.child(list).into_any_element()
    }

    fn creation_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session.as_ref() else {
            return div().into_any_element();
        };
        let panel = match session.creation_panel() {
            Ok(panel) => panel,
            Err(error) => return banner("Creation unavailable", &error.to_string(), true),
        };
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(div().text_base().child(panel.title.to_owned()))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(panel.description.to_owned()),
            );
        let steps = panel
            .steps
            .iter()
            .map(|step| step.title.clone())
            .collect::<Vec<_>>()
            .join(" · ");
        if !steps.is_empty() {
            section = section.child(div().text_xs().child(format!("Steps: {steps}")));
        }

        let mut fields = div().flex().flex_col().gap_1();
        for (index, field) in panel.fields.iter().enumerate() {
            let focused = self.focus_target == TabletopFocus::CreationField(index);
            fields = fields.child(field_row(
                ("tabletop-creation-field", index),
                field,
                self.creation.buffer(field.path),
                self.creation.error(field.path),
                focused,
                cx.listener(move |this, _, _, cx| this.begin_creation_field(index, cx)),
            ));
        }
        section = section
            .child(fields)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(action_button(
                        ("tabletop-preview", 0),
                        "Preview creation",
                        self.focus_target == TabletopFocus::PreviewCreation,
                        cx.listener(|this, _, _, cx| this.preview(cx)),
                    ))
                    .child(action_button(
                        ("tabletop-accept", 0),
                        "Accept creation",
                        self.focus_target == TabletopFocus::AcceptCreation,
                        cx.listener(|this, _, _, cx| this.accept(cx)),
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(if session.is_creation_accepted() {
                        "Creation accepted. Play operations are enabled.".to_owned()
                    } else {
                        "Preview the exact visible form before accepting it.".to_owned()
                    }),
            );

        if let Some(preview) = &self.preview {
            section = section.child(preview_panel(preview));
        }
        section.into_any_element()
    }

    fn operation_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.panel_rows();
        let Some((panel, operations)) = rows
            .into_iter()
            .find(|(panel, _)| Some(panel.capability) == self.selected_panel)
        else {
            return div().into_any_element();
        };
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(
                div()
                    .text_base()
                    .child(format!("{} operations", capability_label(panel.capability))),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(panel.description),
            );

        let mut operation_list = div().flex().flex_col().gap_1();
        for (index, operation) in operations.iter().enumerate() {
            let focused = self.focus_target == TabletopFocus::Operation(index);
            let selected = self.selected_operation == Some(operation.id);
            operation_list = operation_list.child(
                div()
                    .id(("tabletop-operation", index))
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(focus_border(focused)))
                    .bg(rgb(if selected {
                        DARK_THEME.accent
                    } else {
                        DARK_THEME.chrome
                    }))
                    .role(gpui::Role::Button)
                    .aria_label(operation.title.to_string())
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.select_operation(index, cx)))
                    .child(div().text_sm().child(operation.title.to_owned()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(format!(
                                "{} · {}",
                                operation.id,
                                capability_label(operation.capability)
                            )),
                    ),
            );
        }
        section = section.child(operation_list);

        let Some(operation) = self.selected_operation() else {
            return section
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child("Select an operation to open its form."),
                )
                .into_any_element();
        };
        let mut fields = div().flex().flex_col().gap_1();
        for (index, field) in operation.fields.iter().enumerate() {
            let focused = self.focus_target == TabletopFocus::OperationField(index);
            let state = self.operation_drafts.get(operation.id);
            let buffer = state.map_or("", |state| state.buffer(field.path));
            let error = state.and_then(|state| state.error(field.path));
            fields = fields.child(field_row(
                ("tabletop-operation-field", index),
                field,
                buffer,
                error,
                focused,
                cx.listener(move |this, _, _, cx| this.begin_operation_field(index, cx)),
            ));
        }
        section
            .child(fields)
            .child(action_button(
                ("tabletop-execute", 0),
                "Execute operation",
                self.focus_target == TabletopFocus::ExecuteOperation,
                cx.listener(|this, _, _, cx| this.execute(cx)),
            ))
            .into_any_element()
    }

    fn event_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session.as_ref() else {
            return div().into_any_element();
        };
        let events = session.events();
        let records = session.records();
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(div().text_base().child("Event log"));

        if events.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("No operations recorded."),
            );
        } else {
            let mut log = div()
                .id("tabletop-event-log")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(180.0))
                .overflow_y_scroll();
            for (index, event) in events.iter().enumerate() {
                log = log.child(
                    div()
                        .id(("tabletop-event", index))
                        .p_1()
                        .rounded_md()
                        .bg(rgb(DARK_THEME.chrome))
                        .text_xs()
                        .child(event_line(event)),
                );
            }
            section = section.child(log);
        }

        if !records.is_empty() {
            let mut replays = div().flex().flex_col().gap_1();
            for (index, record) in records.iter().enumerate() {
                let focused = self.focus_target == TabletopFocus::Replay(index);
                replays = replays.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_1()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(focus_border(focused)))
                        .bg(rgb(DARK_THEME.chrome))
                        .child(div().flex_1().text_xs().child(format!(
                            "#{} · {}",
                            index + 1,
                            record.request.operation
                        )))
                        .child(action_button(
                            ("tabletop-replay", index),
                            "Replay",
                            focused,
                            cx.listener(move |this, _, _, cx| this.replay(index, cx)),
                        )),
                );
            }
            section = section.child(replays);
        }
        section.into_any_element()
    }

    fn module_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session.as_ref() else {
            return div().into_any_element();
        };
        let panel = match session.module_panel() {
            Ok(panel) => panel,
            Err(TabletopError::UndeclaredCapability { .. }) => return div().into_any_element(),
            Err(error) => return banner("Module review unavailable", &error.to_string(), true),
        };
        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .child(div().text_base().child(panel.title.to_owned()))
            .child(module_list(&panel));

        let mut fields = div().flex().flex_col().gap_1();
        for (index, field) in panel.fields.iter().enumerate() {
            let focused = self.focus_target == TabletopFocus::ModuleField(index);
            fields = fields.child(field_row(
                ("tabletop-module-field", index),
                field,
                self.module.buffer(field.path),
                self.module.error(field.path),
                focused,
                cx.listener(move |this, _, _, cx| this.begin_module_field(index, cx)),
            ));
        }
        section = section.child(fields).child(action_button(
            ("tabletop-apply-modules", 0),
            "Apply modules",
            self.focus_target == TabletopFocus::ApplyModules,
            cx.listener(|this, _, _, cx| this.apply_modules(cx)),
        ));

        if let Some(outcome) = &self.module_outcome {
            section = section.child(module_outcome(outcome));
        }
        section.into_any_element()
    }
}

impl EventEmitter<TabletopViewEvent> for TabletopView {}

impl gpui::Render for TabletopView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("tabletop-view")
            .key_context("WeaveTabletop")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .bg(rgb(DARK_THEME.workspace))
            .text_color(rgb(DARK_THEME.text))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.key_down(event, cx);
            }));

        let order = self.focus_order();
        if !order.is_empty() && !order.contains(&self.focus_target) {
            self.focus_target = order[0].clone();
        }

        let Some(session) = self.session.as_ref() else {
            return root.child(
                div()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child("Select a tabletop adapter to open its panels."),
            );
        };
        let manifest = session.manifest();
        root = root.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_base()
                        .child(format!("{} · {}", manifest.id, manifest.version)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child(format!(
                            "Focus: {} · {}",
                            self.focus_target.label(),
                            if session.is_creation_accepted() {
                                "creation accepted"
                            } else {
                                "creation pending"
                            }
                        )),
                ),
        );
        if let Some(notice) = &self.notice {
            root = root.child(notice_banner(notice));
        }
        root = root
            .child(self.panel_section(cx))
            .child(self.creation_section(cx))
            .child(self.operation_section(cx))
            .child(self.event_section(cx))
            .child(self.module_section(cx))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(TABLETOP_KEYBOARD_HINT),
            );

        if let Some(draft) = &self.draft {
            root = root.child(
                div()
                    .id("tabletop-field-draft")
                    .role(gpui::Role::TextInput)
                    .aria_label(draft.target.label())
                    .aria_value(draft.rendered())
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
                            .child(draft.target.label()),
                    )
                    .child(div().text_sm().child(draft.rendered()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child("Enter to apply · Esc to cancel"),
                    ),
            );
        }
        root
    }
}

fn field_row(
    id: (&'static str, usize),
    field: &PanelField,
    buffer: &str,
    error: Option<&str>,
    focused: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut row = div()
        .id(id)
        .role(gpui::Role::TextInput)
        .aria_label(field.label.to_owned())
        .aria_value(buffer.to_owned())
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(focus_border(focused)))
        .bg(rgb(DARK_THEME.panel))
        .cursor_text()
        .on_click(on_click)
        .child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(format!(
                    "{} · {} · {}{}",
                    field.label,
                    field.path,
                    kind_label(field.kind),
                    if field.required { " · required" } else { "" }
                )),
        )
        .child(div().text_sm().child(if buffer.is_empty() {
            "(empty)".to_owned()
        } else {
            buffer.to_owned()
        }));
    if let Some(error) = error {
        row = row.child(
            div()
                .p_1()
                .rounded_md()
                .bg(rgb(DARK_THEME.error))
                .text_xs()
                .child(format!("{}: {}", field.path, error)),
        );
    }
    row.into_any_element()
}

fn action_button(
    id: (&'static str, usize),
    label: &'static str,
    focused: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .px_3()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(rgb(focus_border(focused)))
        .bg(rgb(control_background(focused)))
        .text_sm()
        .cursor_pointer()
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

fn banner(title: &str, message: &str, error: bool) -> AnyElement {
    div()
        .p_2()
        .rounded_md()
        .bg(rgb(if error {
            DARK_THEME.error
        } else {
            DARK_THEME.warning
        }))
        .child(div().text_sm().child(title.to_owned()))
        .child(div().text_xs().child(message.to_owned()))
        .into_any_element()
}

fn notice_banner(notice: &Notice) -> AnyElement {
    match notice {
        Notice::Success(message) => banner("Ready", message, false),
        Notice::Warning(message) => banner("Module diagnostics", message, false),
        Notice::Error(message) => banner("Tabletop error", message, true),
    }
}

fn preview_panel(preview: &PanelCreationPreview) -> AnyElement {
    let mut panel = div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(DARK_THEME.success))
        .bg(rgb(DARK_THEME.chrome))
        .child(div().text_sm().child("Creation preview"))
        .child(
            div()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child(format!(
                    "Definition {} · JSON {} characters · RON {} characters",
                    preview.definition.definition_sha256,
                    preview.canonical_json.chars().count(),
                    preview.canonical_ron.chars().count()
                )),
        );
    for explanation in &preview.explanations {
        panel = panel.child(div().text_xs().child(format!("· {explanation}")));
    }
    if !preview.diagnostics.is_empty() {
        for diagnostic in &preview.diagnostics {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!(
                        "{} at {}: {}",
                        diagnostic.code, diagnostic.path, diagnostic.message
                    )),
            );
        }
    }
    panel.into_any_element()
}

fn module_list(panel: &ModulePanelView) -> AnyElement {
    let mut list = div().flex().flex_col().gap_1();
    for module in &panel.modules {
        list = list.child(
            div()
                .p_1()
                .rounded_md()
                .bg(rgb(DARK_THEME.chrome))
                .child(div().text_xs().child(format!(
                    "{} · {} {} · {}",
                    module.title,
                    module.id,
                    module.version,
                    if module.verified {
                        "verified"
                    } else {
                        "unverified"
                    }
                )))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child(module.reason.clone()),
                ),
        );
    }
    list.into_any_element()
}

fn module_outcome(outcome: &ModulePanelOutcome) -> AnyElement {
    let capabilities = outcome
        .effective_capabilities
        .iter()
        .map(|capability| capability_label(*capability))
        .collect::<Vec<_>>()
        .join(", ");
    let rejected = !outcome.diagnostics.is_empty();
    let mut panel = div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(if rejected {
            DARK_THEME.warning
        } else {
            DARK_THEME.success
        }))
        .bg(rgb(DARK_THEME.chrome))
        .child(div().text_sm().child(if rejected {
            "Module review rejected"
        } else {
            "Module review"
        }))
        .child(div().text_xs().child(format!(
            "Enabled: {} · Effective capabilities: {}",
            if outcome.enabled_modules.is_empty() {
                "none".to_owned()
            } else {
                outcome.enabled_modules.join(", ")
            },
            if capabilities.is_empty() {
                "none".to_owned()
            } else {
                capabilities
            }
        )));
    for diagnostic in &outcome.diagnostics {
        panel = panel.child(
            div()
                .text_xs()
                .bg(rgb(DARK_THEME.error))
                .p_1()
                .child(format!(
                    "{} at {}: {}",
                    diagnostic.code, diagnostic.path, diagnostic.message
                )),
        );
    }
    for module in &outcome.inactive_modules {
        panel = panel.child(
            div()
                .text_xs()
                .child(format!("Inactive: {} ({})", module.title, module.reason)),
        );
    }
    panel.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabletop::tabletop_panels::{open_adventure, open_tale, spark, wicked_ones};
    use weave_tabletop::{
        AdapterCharacterDefinition, AdapterManifest, OpenAdventureResolver, OpenTaleResolver,
        ResolutionRequest, ResolverRegistry, SparkResolver, TabletopResolver, TabletopState,
        WickedOnesResolver,
    };

    fn registry(
        manifest: &AdapterManifest,
        resolver: impl TabletopResolver + 'static,
    ) -> Arc<ResolverRegistry> {
        let mut registry = ResolverRegistry::new();
        registry.register(manifest, resolver).unwrap();
        Arc::new(registry)
    }

    fn spark_session() -> (Arc<dyn TabletopPanelAdapter>, TabletopPanelSession) {
        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(spark::SparkPanels);
        let manifest = adapter.manifest();
        let session =
            TabletopPanelSession::new(adapter.clone(), registry(&manifest, SparkResolver)).unwrap();
        (adapter, session)
    }

    fn builtin_sessions() -> Vec<(
        &'static str,
        Arc<dyn TabletopPanelAdapter>,
        TabletopPanelSession,
    )> {
        let mut sessions = Vec::new();

        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(spark::SparkPanels);
        let manifest = adapter.manifest();
        sessions.push((
            "Spark",
            adapter.clone(),
            TabletopPanelSession::new(adapter, registry(&manifest, SparkResolver)).unwrap(),
        ));

        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(wicked_ones::WickedOnesPanels);
        let manifest = adapter.manifest();
        sessions.push((
            "Wicked Ones",
            adapter.clone(),
            TabletopPanelSession::new(adapter, registry(&manifest, WickedOnesResolver)).unwrap(),
        ));

        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(open_tale::OpenTalePanels);
        let manifest = adapter.manifest();
        sessions.push((
            "Open Tale",
            adapter.clone(),
            TabletopPanelSession::new(adapter, registry(&manifest, OpenTaleResolver)).unwrap(),
        ));

        let adapter: Arc<dyn TabletopPanelAdapter> = Arc::new(open_adventure::OpenAdventurePanels);
        let manifest = adapter.manifest();
        sessions.push((
            "Open Adventure",
            adapter.clone(),
            TabletopPanelSession::new(adapter, registry(&manifest, OpenAdventureResolver)).unwrap(),
        ));

        sessions
    }

    fn spark_form() -> PanelForm {
        PanelForm::new()
            .with_text("character_id", "org.weave.character.view_lumen")
            .with_text("name", "View Lumen")
            .with_text("concept", "A lamp warden testing editor panels.")
            .with_integer("starting_influence", 10)
            .with_text("beliefs[0].id", "steady_light")
            .with_text(
                "beliefs[0].statement",
                "A steady light protects the path behind it.",
            )
            .with_text("beliefs[1].id", "honest_warning")
            .with_text(
                "beliefs[1].statement",
                "A warning is only useful when it is honest.",
            )
            .with_text("beliefs[2].id", "shared_route")
            .with_text(
                "beliefs[2].statement",
                "Every safe route belongs to its travelers.",
            )
            .with_choice("attributes.body", "d4")
            .with_choice("attributes.heart", "d6")
            .with_choice("attributes.mind", "d10")
            .with_choice("attributes.spark", "d8")
    }

    fn open_conflict_form() -> PanelForm {
        PanelForm::new()
            .with_text("conflict_id", "view_gate_conflict")
            .with_text("stakes", "Who holds the editor gate")
            .with_integer("max_escalation", 3)
            .with_choice("victory_cost_kind", "influence")
            .with_integer("victory_cost_amount", 2)
    }

    #[test]
    fn panel_rows_reflect_declared_capabilities() {
        for (name, adapter, session) in builtin_sessions() {
            let declared = session
                .panels()
                .iter()
                .map(|panel| panel.capability)
                .collect::<Vec<_>>();
            let rows = visible_panels(adapter.as_ref(), &session);
            assert_eq!(
                rows.iter()
                    .map(|(panel, _)| panel.capability)
                    .collect::<Vec<_>>(),
                declared,
                "{name} panel rows must match the declared capabilities"
            );
            for (panel, operations) in &rows {
                assert!(
                    has_capability(session.manifest(), panel.capability),
                    "{name} exposed an undeclared panel"
                );
                assert_eq!(
                    operations.len(),
                    panel
                        .operations
                        .iter()
                        .filter(|operation| {
                            has_capability(session.manifest(), operation.capability)
                        })
                        .count(),
                    "{name} exposed an undeclared operation"
                );
            }
            if name == "Spark" {
                assert_eq!(
                    declared,
                    [
                        TabletopCapability::ChecksAndConflicts,
                        TabletopCapability::Scenes,
                        TabletopCapability::Advancement,
                    ]
                );
            }
        }
    }

    #[test]
    fn field_edit_validates_and_reports_path_and_reason() {
        let unsigned = PanelField {
            path: "max_escalation",
            label: "Maximum escalation",
            kind: PanelFieldKind::Unsigned,
            required: true,
        };
        let mut state = PanelDraftState::default();
        let error = state.edit(&unsigned, "many").unwrap_err();
        assert_eq!(error.path, "max_escalation");
        assert!(error.reason.contains("non-negative whole number"));
        assert_eq!(
            state.error("max_escalation"),
            Some("expected a non-negative whole number")
        );
        assert_eq!(state.value("max_escalation"), None);

        state.edit(&unsigned, "3").unwrap();
        assert_eq!(state.value("max_escalation"), Some(&PanelValue::Integer(3)));
        assert_eq!(state.form().integer("max_escalation").unwrap(), 3);
        assert!(state.error("max_escalation").is_none());

        let choice = PanelField {
            path: "attributes.body",
            label: "Body step",
            kind: PanelFieldKind::Choice(&["d4", "d6"]),
            required: true,
        };
        let error = state.edit(&choice, "d20").unwrap_err();
        assert_eq!(error.path, "attributes.body");
        assert!(error.reason.contains("expected one of"));
        assert_eq!(state.value("attributes.body"), None);
    }

    struct GatedOperationAdapter(spark::SparkPanels);

    impl TabletopPanelAdapter for GatedOperationAdapter {
        fn adapter_id(&self) -> &'static str {
            self.0.adapter_id()
        }

        fn adapter_version(&self) -> &'static str {
            self.0.adapter_version()
        }

        fn manifest(&self) -> AdapterManifest {
            self.0.manifest()
        }

        fn creation_panel(&self) -> crate::tabletop::tabletop_panels::CreationPanel {
            self.0.creation_panel()
        }

        fn panels(&self) -> Vec<PanelDescriptor> {
            let mut panels = self.0.panels();
            if let Some(panel) = panels
                .iter_mut()
                .find(|panel| panel.capability == TabletopCapability::ChecksAndConflicts)
            {
                panel.operations.push(PanelOperation {
                    id: "dormant_operation",
                    title: "Dormant operation",
                    capability: TabletopCapability::Encounters,
                    fields: Vec::new(),
                    build_request: dormant_request,
                });
            }
            panels
        }
    }

    fn dormant_request(
        _form: &PanelForm,
        _definition: &AdapterCharacterDefinition,
        _state: &TabletopState,
    ) -> Result<ResolutionRequest, TabletopError> {
        Err(TabletopError::UndeclaredCapability {
            capability: TabletopCapability::Encounters,
        })
    }

    #[test]
    fn capability_gated_operation_disappears_when_absent() {
        let adapter: Arc<dyn TabletopPanelAdapter> =
            Arc::new(GatedOperationAdapter(spark::SparkPanels));
        let manifest = adapter.manifest();
        let session =
            TabletopPanelSession::new(adapter.clone(), registry(&manifest, SparkResolver)).unwrap();
        let rows = visible_panels(adapter.as_ref(), &session);
        let (_, operations) = rows
            .iter()
            .find(|(panel, _)| panel.capability == TabletopCapability::ChecksAndConflicts)
            .unwrap();
        assert!(
            !operations
                .iter()
                .any(|operation| operation.id == "dormant_operation")
        );
        assert!(
            operations
                .iter()
                .any(|operation| operation.id == "open_conflict")
        );
        let error = session.panel(TabletopCapability::Encounters).unwrap_err();
        assert!(error.to_string().starts_with("TT106"));
    }

    #[test]
    fn accept_refuses_dirty_or_invalid_creation() {
        assert_eq!(acceptance_blocker(false, true, None), None);
        let dirty = acceptance_blocker(true, true, None).unwrap();
        assert!(dirty.contains("fresh preview"));
        let invalid = acceptance_blocker(
            true,
            true,
            Some(("attributes.body", "expected one of: d4, d6")),
        )
        .unwrap();
        assert!(invalid.contains("attributes.body"));
        assert!(invalid.contains("expected one of"));
        let no_preview = acceptance_blocker(false, false, None).unwrap();
        assert!(no_preview.contains("build a preview"));
    }

    #[test]
    fn session_requires_the_matching_adapter_coordinate() {
        let (spark_adapter, session) = spark_session();
        assert!(adapter_matches_session(spark_adapter.as_ref(), &session));

        let other: Arc<dyn TabletopPanelAdapter> = Arc::new(open_adventure::OpenAdventurePanels);
        assert!(!adapter_matches_session(other.as_ref(), &session));
    }

    #[test]
    fn event_log_renders_recorded_operation() {
        let (_, mut session) = spark_session();
        session.update_creation_form(spark_form()).unwrap();
        session.preview_creation().unwrap();
        session.accept_creation().unwrap();
        session
            .execute_operation("open_conflict", &open_conflict_form())
            .unwrap();

        let lines = session.events().iter().map(event_line).collect::<Vec<_>>();
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|line| {
            line.contains("conflict_opened") && line.contains("Checks and conflicts")
        }));
        assert!(lines.iter().all(|line| line.starts_with('#')));
        assert_eq!(session.records().len(), 1);
    }
}
