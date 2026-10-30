//! Ordered manuscript navigation: the chapter tree, the card board, and the
//! chapter table.
//!
//! All three surfaces read one projection of the same ordered child list, so a
//! selection or a filter cannot disagree between them. The projection is plain
//! data and is tested without a window; the surface only draws it.

use std::ops::Range;
use std::path::Path;

use gpui::{
    AnyElement, Context, FocusHandle, KeyDownEvent, Render, ScrollDelta, ScrollWheelEvent, Window,
    div, prelude::*, px, rgb,
};

use crate::manuscript::{
    BodyStore, Chapter, ChapterBody, ChapterId, ChapterKind, ChapterTree, DirectFormat,
    ManuscriptFormat, ManuscriptHistory, ManuscriptSession, ManuscriptStore, Movement, Position,
    Selection, StyleSheet, VisualLine, apply_character_style, apply_paragraph_style, backspace,
    insert, layout, line_layout, move_selection, split_paragraph, toggle_direct,
};
use crate::theme::DARK_THEME;

/// Width of one index card in logical pixels.
pub const CARD_WIDTH: f32 = 260.0;
/// Height of one index card in logical pixels. Together with [`CARD_WIDTH`]
/// this fixes the card aspect ratio.
pub const CARD_HEIGHT: f32 = 176.0;
/// Gap between cards in logical pixels.
pub const CARD_GAP: f32 = 12.0;
/// Columns the board shows before it wraps.
pub const DEFAULT_COLUMNS: usize = 3;
/// Width of the chapter tree panel in logical pixels.
pub const TREE_WIDTH: f32 = 300.0;
/// Height of one table row in logical pixels.
pub const ROW_HEIGHT: f32 = 30.0;
/// Rows rendered beyond the viewport so scrolling does not expose a gap.
const OVERSCAN: usize = 2;
/// Maximum characters of body text a card shows before truncating.
const EXCERPT_CHARS: usize = 220;
/// Text shown when a chapter has no status.
pub const UNSET_STATUS: &str = "Unset status";
/// Text shown when a chapter has no label.
pub const UNSET_LABEL: &str = "Unset label";

/// Display text for a status, never a blank cell.
#[must_use]
pub fn status_display(status: Option<&str>) -> &str {
    status.unwrap_or(UNSET_STATUS)
}

/// Display text for a label, never a blank cell.
#[must_use]
pub fn label_display(label: Option<&str>) -> &str {
    label.unwrap_or(UNSET_LABEL)
}

/// Which dense surface the manuscript view shows beside the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManuscriptMode {
    /// Index cards for the selected container.
    Board,
    /// A table of rows for the selected container.
    Table,
    /// Project status and label definitions.
    Settings,
    /// The chapter as a manuscript page.
    Write,
}

impl ManuscriptMode {
    /// Tab label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Board => "Board",
            Self::Table => "Table",
            Self::Settings => "Settings",
            Self::Write => "Write",
        }
    }
}

/// A positional command available from the toolbar, the menu, and the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveVerb {
    /// One place earlier among siblings.
    Up,
    /// One place later among siblings.
    Down,
    /// One level out, directly after the former parent.
    Promote,
    /// One level in, as the first child of the earlier sibling.
    Demote,
    /// To the start of the sibling list.
    ToFront,
    /// To the end of the sibling list.
    ToBack,
}

impl MoveVerb {
    /// Toolbar label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Promote => "Left",
            Self::Demote => "Right",
            Self::ToFront => "Front",
            Self::ToBack => "Back",
        }
    }

    /// Status text after a successful move.
    #[must_use]
    pub const fn done(self) -> &'static str {
        match self {
            Self::Up => "Moved up",
            Self::Down => "Moved down",
            Self::Promote => "Promoted one level",
            Self::Demote => "Demoted one level",
            Self::ToFront => "Moved to the front",
            Self::ToBack => "Moved to the back",
        }
    }
}

/// A metadata column the table can show after the document title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Column {
    /// Project status.
    Status,
    /// Project label.
    Label,
    /// Structural role.
    Kind,
}

impl Column {
    /// Column heading.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Label => "Label",
            Self::Kind => "Type",
        }
    }
}

/// Every metadata column, in canonical order.
pub const ALL_COLUMNS: [Column; 3] = [Column::Status, Column::Label, Column::Kind];

/// Inputs that shape one projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionOptions {
    /// Current selection.
    pub selected: ChapterId,
    /// Card board column count.
    pub columns: usize,
    /// Case-insensitive title filter.
    pub filter: String,
    /// Metadata columns the table shows.
    pub visible_columns: Vec<Column>,
}

impl Default for ProjectionOptions {
    fn default() -> Self {
        Self {
            selected: ChapterId::ROOT,
            columns: DEFAULT_COLUMNS,
            filter: String::new(),
            visible_columns: ALL_COLUMNS.to_vec(),
        }
    }
}

/// One visible row of the chapter tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// Chapter the row represents.
    pub id: ChapterId,
    /// Nesting depth, where a root child is zero.
    pub depth: usize,
    /// Display title.
    pub title: String,
    /// Structural role label.
    pub kind: &'static str,
    /// Resolved status name, when one is set.
    pub status: Option<String>,
    /// Resolved label name, when one is set.
    pub label: Option<String>,
    /// Whether the chapter holds children.
    pub has_children: bool,
    /// Whether this row is the current selection.
    pub is_selected: bool,
    /// Name a screen reader should announce for the row.
    pub accessible_label: String,
}

/// One index card on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardRecord {
    /// Chapter the card represents.
    pub id: ChapterId,
    /// Display title.
    pub title: String,
    /// Structural role label.
    pub kind: &'static str,
    /// Resolved status name, when one is set.
    pub status: Option<String>,
    /// Resolved label name, when one is set.
    pub label: Option<String>,
    /// Body excerpt, already truncated.
    pub excerpt: String,
    /// Words in the whole body, not just the excerpt.
    pub word_count: usize,
    /// One-based position among the cards.
    pub ordinal: usize,
    /// Number of cards on the board.
    pub total: usize,
    /// Name a screen reader should announce for the card.
    pub accessible_label: String,
}

/// One row of the chapter table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// Chapter the row represents.
    pub id: ChapterId,
    /// Display title.
    pub title: String,
    /// Resolved status name, when one is set.
    pub status: Option<String>,
    /// Resolved label name, when one is set.
    pub label: Option<String>,
    /// Structural role label.
    pub kind: &'static str,
    /// Whether this row is the current selection.
    pub is_selected: bool,
    /// One-based position among the visible rows.
    pub ordinal: usize,
    /// Number of visible rows.
    pub total: usize,
    /// Name a screen reader should announce for the row.
    pub accessible_label: String,
}

/// Everything the surfaces draw, derived from one child list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManuscriptProjection {
    rows: Vec<TreeRow>,
    cards: Vec<CardRecord>,
    table_rows: Vec<TableRow>,
    selected: ChapterId,
    container: ChapterId,
    container_title: String,
    columns: usize,
    filter: String,
    visible_columns: Vec<Column>,
    total_children: usize,
    empty_state: String,
}

impl ManuscriptProjection {
    /// Build the projection for one selection and one filter.
    ///
    /// `bodies` is optional so the projection can be built for a project with
    /// no manuscript on disk. Only the selected container's children are read.
    #[must_use]
    pub fn build(
        tree: &ChapterTree,
        bodies: Option<&BodyStore>,
        options: &ProjectionOptions,
    ) -> Self {
        let columns = options.columns.clamp(1, 6);
        let visible_columns = normalize_columns(&options.visible_columns);
        let selected = tree
            .chapter(options.selected)
            .map_or(ChapterId::ROOT, |chapter| chapter.id);
        let container = tree.chapter(selected).map_or(ChapterId::ROOT, |chapter| {
            if chapter.kind.is_container() {
                chapter.id
            } else {
                chapter.parent_id
            }
        });
        let container_title = tree
            .chapter(container)
            .map_or_else(|| "Manuscript".to_owned(), |chapter| chapter.title.clone());

        let statuses = tree.statuses_by_id();
        let labels = tree.labels_by_id();
        let mut rows = Vec::new();
        collect_rows(
            tree,
            ChapterId::ROOT,
            0,
            selected,
            &statuses,
            &labels,
            &mut rows,
        );

        let mut children: Vec<&Chapter> = tree
            .children(container)
            .into_iter()
            .filter(|chapter| !(container == ChapterId::ROOT && chapter.id == ChapterId::TRASH))
            .collect();
        children.sort_by_key(|chapter| chapter.order);
        let total_children = children.len();

        let needle = options.filter.trim().to_lowercase();
        if !needle.is_empty() {
            children.retain(|chapter| chapter.title.to_lowercase().contains(&needle));
        }

        let total = children.len();

        let mut cards = Vec::with_capacity(total);
        let mut table_rows = Vec::with_capacity(total);
        for (index, chapter) in children.into_iter().enumerate() {
            let ordinal = index + 1;
            let body = bodies.and_then(|store| store.read(chapter.id).ok());
            let paragraph_text = body
                .as_ref()
                .map(crate::manuscript::ChapterBody::text)
                .unwrap_or_default();
            let word_count = body
                .as_ref()
                .map_or(0, crate::manuscript::ChapterBody::word_count);
            let child_count = tree.child_ids(chapter.id).len();
            let excerpt = if paragraph_text.trim().is_empty() && child_count > 0 {
                format!(
                    "{child_count} {}",
                    if child_count == 1 { "scene" } else { "scenes" }
                )
            } else {
                excerpt_of(&paragraph_text)
            };
            let status = chapter
                .status_id
                .and_then(|id| statuses.get(&id))
                .map(|status| status.name.clone());
            let label = chapter
                .label_id
                .and_then(|id| labels.get(&id))
                .map(|label| label.name.clone());
            cards.push(CardRecord {
                id: chapter.id,
                title: chapter.title.clone(),
                kind: chapter.kind.label(),
                status: status.clone(),
                label: label.clone(),
                excerpt,
                word_count,
                ordinal,
                total,
                accessible_label: format!(
                    "{} ({ordinal} of {total} index cards): {}",
                    chapter.kind.label(),
                    chapter.title
                ),
            });

            table_rows.push(TableRow {
                id: chapter.id,
                title: chapter.title.clone(),
                status: status.clone(),
                label: label.clone(),
                kind: chapter.kind.label(),
                is_selected: chapter.id == selected,
                ordinal,
                total,
                accessible_label: format!(
                    "Row {ordinal} of {total}: {}, {}, {}, {}",
                    chapter.title,
                    status.as_deref().unwrap_or("no status"),
                    label.as_deref().unwrap_or("no label"),
                    chapter.kind.label()
                ),
            });
        }

        let empty_state = if !needle.is_empty() {
            format!(
                "No chapters match \u{201c}{}\u{201d}",
                options.filter.trim()
            )
        } else if container == ChapterId::ROOT {
            format!("{container_title} has no chapters yet")
        } else {
            format!("{container_title} has no scenes yet")
        };

        Self {
            rows,
            cards,
            table_rows,
            selected,
            container,
            container_title,
            columns,
            filter: options.filter.trim().to_owned(),
            visible_columns,
            total_children,
            empty_state,
        }
    }

    /// Visible tree rows, in tree order.
    #[must_use]
    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    /// Cards for the selected container, in tree order.
    #[must_use]
    pub fn cards(&self) -> &[CardRecord] {
        &self.cards
    }

    /// Table rows for the selected container, in tree order.
    #[must_use]
    pub fn table_rows(&self) -> &[TableRow] {
        &self.table_rows
    }

    /// Current selection.
    #[must_use]
    pub const fn selected(&self) -> ChapterId {
        self.selected
    }

    /// Container whose children the dense surfaces show.
    #[must_use]
    pub const fn container(&self) -> ChapterId {
        self.container
    }

    /// Title of that container.
    #[must_use]
    pub fn container_title(&self) -> &str {
        &self.container_title
    }

    /// Number of card columns.
    #[must_use]
    pub const fn columns(&self) -> usize {
        self.columns
    }

    /// Active filter.
    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Metadata columns the table shows.
    #[must_use]
    pub fn visible_columns(&self) -> &[Column] {
        &self.visible_columns
    }

    /// Children in the container before filtering.
    #[must_use]
    pub const fn total_children(&self) -> usize {
        self.total_children
    }

    /// Number of rows the filter leaves visible.
    #[must_use]
    pub fn visible_count(&self) -> usize {
        self.table_rows.len()
    }

    /// Text shown when the container has no visible children.
    #[must_use]
    pub fn empty_state(&self) -> &str {
        &self.empty_state
    }

    /// Whether the container has no visible children.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table_rows.is_empty()
    }

    /// Rows intersecting a viewport.
    ///
    /// The range is padded by [`OVERSCAN`] rows on each side so a fast scroll
    /// does not expose a blank gap. It is clamped to the row count, so the
    /// element tree stays proportional to the viewport rather than the
    /// manuscript.
    #[must_use]
    pub fn visible_row_range(&self, scroll_offset: f32, viewport_height: f32) -> Range<usize> {
        let total = self.table_rows.len();
        if total == 0 || viewport_height <= 0.0 {
            return 0..0;
        }
        let first = (scroll_offset.max(0.0) / ROW_HEIGHT).floor() as usize;
        let on_screen = (viewport_height / ROW_HEIGHT).ceil() as usize + 1;
        let start = first.saturating_sub(OVERSCAN).min(total);
        let end = (first.saturating_add(on_screen).saturating_add(OVERSCAN)).min(total);
        start..end.max(start)
    }

    /// Accessible names for the current projection.
    #[must_use]
    pub fn accessibility(&self) -> ManuscriptAccessibility {
        ManuscriptAccessibility {
            tree: self
                .rows
                .iter()
                .map(|row| row.accessible_label.clone())
                .collect(),
            cards: self
                .cards
                .iter()
                .map(|card| card.accessible_label.clone())
                .collect(),
            table: self
                .table_rows
                .iter()
                .map(|row| row.accessible_label.clone())
                .collect(),
            container: self.container_title.clone(),
            empty_state: self.is_empty().then(|| self.empty_state.clone()),
        }
    }
}

/// Accessible names the surface can hand to a platform adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManuscriptAccessibility {
    /// One label per tree row.
    pub tree: Vec<String>,
    /// One label per card.
    pub cards: Vec<String>,
    /// One label per table row.
    pub table: Vec<String>,
    /// Title of the container the surfaces show.
    pub container: String,
    /// Empty-state text, when nothing is visible.
    pub empty_state: Option<String>,
}

/// Deduplicate metadata columns and restore canonical order.
#[must_use]
pub fn normalize_columns(columns: &[Column]) -> Vec<Column> {
    let mut normalized: Vec<Column> = columns.to_vec();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn collect_rows(
    tree: &ChapterTree,
    parent: ChapterId,
    depth: usize,
    selected: ChapterId,
    statuses: &std::collections::BTreeMap<i64, &crate::manuscript::StatusDefinition>,
    labels: &std::collections::BTreeMap<i64, &crate::manuscript::LabelDefinition>,
    rows: &mut Vec<TreeRow>,
) {
    let children = tree.children(parent);
    let total = children.len();
    for (index, chapter) in children.into_iter().enumerate() {
        let has_children = !tree.child_ids(chapter.id).is_empty();
        let status = chapter
            .status_id
            .and_then(|id| statuses.get(&id))
            .map(|status| status.name.clone());
        let label = chapter
            .label_id
            .and_then(|id| labels.get(&id))
            .map(|label| label.name.clone());
        rows.push(TreeRow {
            id: chapter.id,
            depth,
            title: chapter.title.clone(),
            kind: chapter.kind.label(),
            status: status.clone(),
            label: label.clone(),
            has_children,
            is_selected: chapter.id == selected,
            accessible_label: format!(
                "{}, {}, {} of {}, {}, {}",
                chapter.kind.label(),
                chapter.title,
                index + 1,
                total,
                status.as_deref().unwrap_or("no status"),
                label.as_deref().unwrap_or("no label")
            ),
        });
        collect_rows(
            tree,
            chapter.id,
            depth + 1,
            selected,
            statuses,
            labels,
            rows,
        );
    }
}

fn excerpt_of(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= EXCERPT_CHARS {
        return text.to_owned();
    }
    let mut truncated: String = text.chars().take(EXCERPT_CHARS).collect();
    if let Some(index) = truncated.rfind(' ') {
        truncated.truncate(index);
    }
    truncated.push('…');
    truncated
}

fn next_in_cycle<T: PartialEq + Copy>(options: &[T], current: T) -> T {
    let index = options.iter().position(|option| *option == current);
    let next = index.map_or(0, |index| (index + 1) % options.len());
    options[next]
}

/// Advance a chapter's status through the project list and back to unset.
///
/// Returns whether the tree changed.
pub fn cycle_status(tree: &mut ChapterTree, id: ChapterId) -> bool {
    let Some(chapter) = tree.chapter(id) else {
        return false;
    };
    let mut options: Vec<Option<i64>> = vec![None];
    options.extend(tree.statuses.iter().map(|status| Some(status.id)));
    let next = next_in_cycle(&options, chapter.status_id);
    tree.set_status(id, next).is_ok()
}

/// Advance a chapter's label through the project list and back to unset.
///
/// Returns whether the tree changed.
pub fn cycle_label(tree: &mut ChapterTree, id: ChapterId) -> bool {
    let Some(chapter) = tree.chapter(id) else {
        return false;
    };
    let mut options: Vec<Option<i64>> = vec![None];
    options.extend(tree.labels.iter().map(|label| Some(label.id)));
    let next = next_in_cycle(&options, chapter.label_id);
    tree.set_label(id, next).is_ok()
}

/// Advance a chapter's kind through chapter, scene, and note.
///
/// Returns whether the tree changed. A container holding children cannot
/// become a leaf, so that transition is rejected rather than silently lossy.
pub fn cycle_kind(tree: &mut ChapterTree, id: ChapterId) -> bool {
    let Some(chapter) = tree.chapter(id) else {
        return false;
    };
    let options = [ChapterKind::Chapter, ChapterKind::Scene, ChapterKind::Note];
    let next = next_in_cycle(&options, chapter.kind);
    tree.set_kind(id, next).is_ok()
}

/// The insertion gap a pointer resolves to.
///
/// Gaps are indexed `0..=count`, so gap `0` is before the first card and gap
/// `count` is after the last. Rounding the pointer's position by one card
/// stride means the top and bottom halves of a card resolve to the same gap
/// and a pointer near an edge still lands on exactly one insertion point.
#[must_use]
pub fn nearest_gap(pointer_x: f32, count: usize, stride: f32) -> usize {
    if count == 0 || stride <= 0.0 {
        return 0;
    }
    let raw = (pointer_x.max(0.0) / stride).round();
    (raw as usize).min(count)
}

/// The insertion index under a pointer, or `None` when the pointer is past the
/// last card.
///
/// Cards wrap at `columns`, so the row is resolved from the vertical position
/// and the column from the horizontal one.
#[must_use]
pub fn drop_index(pointer: (f32, f32), count: usize, columns: usize) -> Option<usize> {
    if count == 0 {
        return Some(0);
    }
    let columns = columns.max(1);
    let row = (pointer.1.max(0.0) / (CARD_HEIGHT + CARD_GAP)).floor() as usize;
    let column = nearest_gap(pointer.0, columns, CARD_WIDTH + CARD_GAP);
    let index = row * columns + column;
    (index <= count).then_some(index)
}

/// Whether `dragged` may be reparented into `target`.
///
/// A chapter cannot be dropped into itself or into its own descendant, and a
/// target must be able to hold children.
#[must_use]
pub fn can_reparent(tree: &ChapterTree, dragged: ChapterId, target: ChapterId) -> bool {
    if dragged == target {
        return false;
    }
    let Some(target_chapter) = tree.chapter(target) else {
        return false;
    };
    if !target_chapter.kind.is_container() {
        return false;
    }
    if target_chapter.is_descendant_of(dragged, tree) {
        return false;
    }
    tree.chapter(dragged).is_some()
}

fn kind_color(kind: ChapterKind) -> u32 {
    match kind {
        ChapterKind::Chapter => DARK_THEME.accent,
        ChapterKind::Scene => DARK_THEME.muted_text,
        ChapterKind::Note => DARK_THEME.warning,
    }
}

/// What the window status bar shows for the current selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionContext {
    /// Selected chapter.
    pub id: ChapterId,
    /// Display title.
    pub title: String,
    /// Resolved status name, when one is set.
    pub status: Option<String>,
    /// Resolved label name, when one is set.
    pub label: Option<String>,
    /// Whether the selection sits in the trash.
    pub trashed: bool,
}

/// Payload carried while a chapter is dragged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragPayload {
    /// Chapter being dragged.
    pub id: ChapterId,
    /// Container the drag started in.
    pub source: ChapterId,
    /// Display title, for the floating preview.
    pub title: String,
    /// Body excerpt, for the floating preview.
    pub excerpt: String,
}

impl DragPayload {
    /// Whether the payload started in `container`.
    ///
    /// The drop handler consults this to choose a reorder inside one container
    /// over a reparent into another.
    #[must_use]
    pub fn is_local(&self, container: ChapterId) -> bool {
        self.source == container
    }
}

/// Which project list a rename edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefinitionKind {
    Status,
    Label,
}

/// The definition currently being renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DefinitionEdit {
    kind: DefinitionKind,
    id: i64,
}

/// Floating preview drawn under the pointer during a drag.
struct DragPreview {
    title: String,
    excerpt: String,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(CARD_WIDTH))
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(DARK_THEME.accent))
            .bg(rgb(DARK_THEME.panel))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .line_clamp(3)
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(self.excerpt.clone()),
            )
    }
}

/// Interactive chapter tree with the card board and the chapter table.
pub struct ManuscriptSurface {
    tree: ChapterTree,
    store: Option<ManuscriptStore>,
    bodies: Option<BodyStore>,
    options: ProjectionOptions,
    mode: ManuscriptMode,
    scroll: f32,
    history: ManuscriptHistory,
    editing: Option<DefinitionEdit>,
    buffer: String,
    dragging: Option<ChapterId>,
    drag_gap: Option<usize>,
    drag_over_row: Option<ChapterId>,
    body: Option<ChapterBody>,
    body_id: Option<ChapterId>,
    selection: Selection,
    session: ManuscriptSession,
    styles: StyleSheet,
    format: ManuscriptFormat,
    status: String,
    focus: FocusHandle,
    projection: ManuscriptProjection,
}

impl ManuscriptSurface {
    /// Create an empty surface before a project is loaded.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let tree = ChapterTree::new();
        let options = ProjectionOptions::default();
        let projection = ManuscriptProjection::build(&tree, None, &options);
        Self {
            tree,
            store: None,
            bodies: None,
            options,
            mode: ManuscriptMode::Board,
            scroll: 0.0,
            history: ManuscriptHistory::default(),
            editing: None,
            buffer: String::new(),
            dragging: None,
            drag_gap: None,
            drag_over_row: None,
            body: None,
            body_id: None,
            selection: Selection::collapsed(Position::start()),
            session: ManuscriptSession::new(),
            styles: StyleSheet::new(),
            format: ManuscriptFormat::default(),
            status: "Open a project to read its manuscript".to_owned(),
            focus: cx.focus_handle(),
            projection,
        }
    }

    /// Reset to the empty manuscript shown before a project is opened.
    pub fn clear(&mut self) {
        self.tree = ChapterTree::new();
        self.store = None;
        self.bodies = None;
        self.options = ProjectionOptions::default();
        self.scroll = 0.0;
        self.history.clear();
        self.editing = None;
        self.buffer.clear();
        self.body = None;
        self.body_id = None;
        self.selection = Selection::collapsed(Position::start());
        self.session = ManuscriptSession::new();
        self.styles = StyleSheet::new();
        self.status = "Open a project to read its manuscript".to_owned();
        self.refresh();
    }

    /// Read the manuscript beside a project directory, if there is one.
    pub fn load(&mut self, directory: &Path) {
        let store = ManuscriptStore::in_directory(directory.to_path_buf());
        if !store.exists() {
            self.tree = ChapterTree::new();
            self.store = None;
            self.bodies = None;
            self.options.selected = ChapterId::ROOT;
            self.scroll = 0.0;
            self.status = "This project has no manuscript".to_owned();
            self.refresh();
            return;
        }
        match store.read_tree() {
            Ok(tree) => {
                self.options.selected = ChapterId::ROOT;
                self.history.clear();
                self.body = None;
                self.body_id = None;
                self.styles = store.read_styles().unwrap_or_default();
                self.session = store.read_session().unwrap_or_default();
                self.editing = None;
                self.buffer.clear();
                self.bodies = Some(store.bodies().clone());
                self.store = Some(store);
                self.tree = tree;
                self.scroll = 0.0;
                self.status = format!("{} items", self.tree.len().saturating_sub(2));
            }
            Err(error) => {
                self.tree = ChapterTree::new();
                self.store = None;
                self.bodies = None;
                self.status = format!("Manuscript could not be read: {error}");
            }
        }
        self.refresh();
        if self.mode == ManuscriptMode::Write {
            self.open_body();
        }
    }

    /// Select a chapter and rebuild the dense surfaces.
    pub fn select(&mut self, selected: ChapterId) {
        if self.tree.chapter(selected).is_some() {
            self.options.selected = selected;
            self.scroll = 0.0;
            self.refresh();
            if self.mode == ManuscriptMode::Write {
                self.open_body();
            }
        }
    }

    /// Set how many columns the board lays out.
    pub fn set_columns(&mut self, columns: usize) {
        self.options.columns = columns.clamp(1, 6);
        self.refresh();
    }

    /// Set the title filter shared by the board and the table.
    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.options.filter = filter.into();
        self.scroll = 0.0;
        self.refresh();
    }

    /// Switch the dense surface, loading the chapter page when entering Write.
    pub fn set_mode(&mut self, mode: ManuscriptMode) {
        self.mode = mode;
        if mode == ManuscriptMode::Write {
            self.open_body();
        }
    }

    /// Show or hide one metadata column.
    pub fn toggle_column(&mut self, column: Column) {
        let mut columns = self.options.visible_columns.clone();
        if let Some(index) = columns.iter().position(|visible| *visible == column) {
            columns.remove(index);
        } else {
            columns.push(column);
        }
        self.options.visible_columns = normalize_columns(&columns);
        self.refresh();
    }

    /// Cycle a row's status in place and save the tree.
    pub fn cycle_status(&mut self, id: ChapterId) {
        if cycle_status(&mut self.tree, id) {
            self.persist();
            self.refresh();
        }
    }

    /// Cycle a row's label in place and save the tree.
    pub fn cycle_label(&mut self, id: ChapterId) {
        if cycle_label(&mut self.tree, id) {
            self.persist();
            self.refresh();
        }
    }

    /// Cycle a row's kind in place and save the tree.
    pub fn cycle_kind(&mut self, id: ChapterId) {
        if cycle_kind(&mut self.tree, id) {
            self.persist();
            self.refresh();
        } else {
            self.status = "A chapter holding scenes cannot become a leaf".to_owned();
        }
    }

    /// Current projection.
    #[must_use]
    pub const fn projection(&self) -> &ManuscriptProjection {
        &self.projection
    }

    /// Current dense surface.
    #[must_use]
    pub const fn mode(&self) -> ManuscriptMode {
        self.mode
    }

    /// Accessible names for the current projection.
    #[must_use]
    pub fn accessibility(&self) -> ManuscriptAccessibility {
        self.projection.accessibility()
    }

    /// Apply one positional command to the selection.
    ///
    /// This is the single handler behind the toolbar, the menu, and the
    /// keyboard: each entry point calls this method.
    pub fn apply_move(&mut self, verb: MoveVerb) {
        let id = self.options.selected;
        if id.is_reserved() {
            self.status = "The built-in containers cannot be moved".to_owned();
            return;
        }
        if self.tree.chapter(id).is_none() {
            return;
        }
        let before = self.tree.clone();
        let result = match verb {
            MoveVerb::Up => self.tree.move_up(id),
            MoveVerb::Down => self.tree.move_down(id),
            MoveVerb::Promote => self.tree.promote(id),
            MoveVerb::Demote => self.tree.demote(id),
            MoveVerb::ToFront => self.tree.move_to_front(id),
            MoveVerb::ToBack => self.tree.move_to_back(id),
        };
        match result {
            Ok(()) if self.tree != before => {
                self.history.record(&before);
                self.persist();
                self.status = verb.done().to_owned();
                self.refresh();
            }
            Ok(()) => self.status = "Already in place".to_owned(),
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Move the selection to the trash container.
    pub fn trash_selected(&mut self) {
        let id = self.options.selected;
        if id.is_reserved() {
            self.status = "The built-in containers cannot be deleted".to_owned();
            return;
        }
        let before = self.tree.clone();
        match self.tree.trash_chapter(id) {
            Ok(()) => {
                self.history.record(&before);
                self.persist();
                self.options.selected = ChapterId::TRASH;
                self.status = "Moved to Trash".to_owned();
                self.refresh();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Move the selection out of the trash back to the manuscript root.
    pub fn restore_selected(&mut self) {
        let id = self.options.selected;
        if self.tree.chapter(id).is_none() {
            return;
        }
        let before = self.tree.clone();
        match self.tree.restore_chapter(id) {
            Ok(()) => {
                self.history.record(&before);
                self.persist();
                self.status = "Restored from Trash".to_owned();
                self.refresh();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Empty the trash. Deletion is only permanent when this is called.
    pub fn empty_trash(&mut self) {
        let before = self.tree.clone();
        let removed = self.tree.empty_trash();
        if removed.is_empty() {
            self.status = "The trash is already empty".to_owned();
            return;
        }
        self.history.record(&before);
        self.persist();
        self.status = format!("Emptied the trash ({} items)", removed.len());
        self.refresh();
    }

    /// Undo the last structural change.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.history.undo() else {
            self.status = "Nothing to undo".to_owned();
            return false;
        };
        self.tree = previous;
        if self.tree.chapter(self.options.selected).is_none() {
            self.options.selected = ChapterId::ROOT;
        }
        self.persist();
        self.status = "Undid the last manuscript change".to_owned();
        self.refresh();
        true
    }

    /// Whether a structural change can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether the selection sits in the trash.
    #[must_use]
    pub fn selection_is_trashed(&self) -> bool {
        self.tree
            .chapter(self.options.selected)
            .is_some_and(|chapter| chapter.parent_id == ChapterId::TRASH)
    }

    fn clear_drag(&mut self) {
        self.dragging = None;
        self.drag_gap = None;
        self.drag_over_row = None;
    }

    fn drop_on_board(&mut self, payload: &DragPayload) {
        let container = self.projection.container();
        let Some(index) = self.drag_gap else {
            self.clear_drag();
            return;
        };
        let before = self.tree.clone();
        let local = payload.is_local(container);
        let result = if local {
            self.tree.move_chapter(payload.id, index)
        } else {
            self.tree.reparent_to(payload.id, container, Some(index))
        };
        match result {
            Ok(()) => {
                if self.tree != before {
                    self.history.record(&before);
                    self.persist();
                    self.refresh();
                    self.status = if local {
                        "Reordered the chapter".to_owned()
                    } else {
                        "Moved the chapter into this container".to_owned()
                    };
                }
            }
            Err(error) => self.status = error.to_string(),
        }
        self.clear_drag();
    }

    fn drop_on_tree(&mut self, payload: &DragPayload, target: ChapterId) {
        if payload.source == target {
            self.status = "The chapter is already in this container".to_owned();
            self.clear_drag();
            return;
        }
        if !can_reparent(&self.tree, payload.id, target) {
            self.status = "A chapter cannot move into itself or its own descendant".to_owned();
            self.clear_drag();
            return;
        }
        let before = self.tree.clone();
        match self.tree.reparent_to(payload.id, target, None) {
            Ok(()) => {
                self.history.record(&before);
                self.persist();
                self.refresh();
                self.status = "Moved the chapter into that container".to_owned();
            }
            Err(error) => self.status = error.to_string(),
        }
        self.clear_drag();
    }

    fn open_body(&mut self) {
        if self.mode != ManuscriptMode::Write {
            return;
        }
        let id = self.options.selected;
        if self.body_id == Some(id) {
            return;
        }
        if let Some(previous) = self.body_id {
            self.session.set_selection(previous, self.selection);
            if let Some(store) = &self.store {
                let _ = store.write_session(&self.session);
            }
        }
        let body = self
            .bodies
            .as_ref()
            .and_then(|bodies| bodies.read(id).ok())
            .unwrap_or_default();
        self.selection = clamp_selection(
            &body,
            self.session
                .selection(id)
                .unwrap_or_else(|| Selection::collapsed(Position::start())),
        );
        self.body = Some(body);
        self.body_id = Some(id);
    }

    fn persist_edit(&mut self) {
        let Some(id) = self.body_id else {
            return;
        };
        if let Some(store) = &self.store
            && let Some(body) = &self.body
            && let Err(error) = store.bodies().write(id, body)
        {
            self.status = format!("Could not save the chapter: {error}");
        }
        self.session.set_selection(id, self.selection);
        if let Some(store) = &self.store {
            let _ = store.write_session(&self.session);
        }
        self.refresh();
    }

    fn current_paragraph_style(&self) -> Option<String> {
        self.body
            .as_ref()
            .and_then(|body| body.paragraphs.get(self.selection.caret.paragraph))
            .and_then(|paragraph| paragraph.style.clone())
    }

    fn current_character_style(&self) -> Option<String> {
        self.body
            .as_ref()
            .and_then(|body| body.paragraphs.get(self.selection.caret.paragraph))
            .and_then(|paragraph| {
                let mut cursor = 0;
                for run in &paragraph.runs {
                    let length = run.text.chars().count();
                    if self.selection.caret.offset <= cursor + length {
                        return run.character_style.clone();
                    }
                    cursor += length;
                }
                None
            })
    }

    fn cycle_paragraph_style(&mut self) {
        let mut options: Vec<Option<String>> = vec![None];
        options.extend(
            self.styles
                .paragraph_styles
                .iter()
                .map(|style| Some(style.name.clone())),
        );
        let next = next_option(&options, self.current_paragraph_style());
        if let Some(body) = self.body.as_mut() {
            apply_paragraph_style(body, self.selection, next.as_deref());
        }
        self.persist_edit();
    }

    fn cycle_character_style(&mut self) {
        let mut options: Vec<Option<String>> = vec![None];
        options.extend(
            self.styles
                .character_styles
                .iter()
                .map(|style| Some(style.name.clone())),
        );
        let next = next_option(&options, self.current_character_style());
        let applied = self
            .body
            .as_mut()
            .is_some_and(|body| apply_character_style(body, self.selection, next.as_deref()));
        if applied {
            self.persist_edit();
        } else {
            self.status = "Select text to apply a character style".to_owned();
        }
    }

    fn apply_format(&mut self, action: FormatAction) {
        match action {
            FormatAction::Bold | FormatAction::Italic | FormatAction::Underline => {
                let format = match action {
                    FormatAction::Bold => DirectFormat::Bold,
                    FormatAction::Italic => DirectFormat::Italic,
                    _ => DirectFormat::Underline,
                };
                let toggled = self
                    .body
                    .as_mut()
                    .is_some_and(|body| toggle_direct(body, self.selection, format));
                if toggled {
                    self.persist_edit();
                } else {
                    self.status = "Select text to format it".to_owned();
                }
            }
            FormatAction::ParagraphStyle => self.cycle_paragraph_style(),
            FormatAction::CharacterStyle => self.cycle_character_style(),
        }
    }

    fn write_key(&mut self, event: &KeyDownEvent) -> bool {
        if event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return false;
        }
        let extend = event.keystroke.modifiers.shift;
        let key = event.keystroke.key.as_str();
        let Some(body) = self.body.as_mut() else {
            return false;
        };
        let mut edited = false;
        match key {
            "backspace" => {
                self.selection = backspace(body, self.selection);
                edited = true;
            }
            "enter" => {
                self.selection = split_paragraph(body, self.selection);
                edited = true;
            }
            "left" => {
                self.selection = move_selection(body, self.selection, Movement::Left, extend);
            }
            "right" => {
                self.selection = move_selection(body, self.selection, Movement::Right, extend);
            }
            "up" => {
                self.selection = move_selection(body, self.selection, Movement::Up, extend);
            }
            "down" => {
                self.selection = move_selection(body, self.selection, Movement::Down, extend);
            }
            "home" => {
                self.selection = move_selection(body, self.selection, Movement::LineStart, extend);
            }
            "end" => {
                self.selection = move_selection(body, self.selection, Movement::LineEnd, extend);
            }
            _ => {
                let Some(character) = &event.keystroke.key_char else {
                    return false;
                };
                self.selection = insert(body, self.selection, &character.to_string());
                edited = true;
            }
        }
        if edited {
            self.persist_edit();
        } else {
            self.session
                .set_selection(self.options.selected, self.selection);
        }
        true
    }

    fn write_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(body) = self.body.as_ref() else {
            return div()
                .id("manuscript-write")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(DARK_THEME.muted_text))
                .child("Select a chapter to write")
                .into_any_element();
        };
        let has_selection = !self.selection.is_collapsed();
        let mut toolbar = div()
            .id("writer-toolbar")
            .flex()
            .items_center()
            .flex_wrap()
            .gap_1();
        toolbar = toolbar
            .child(self.format_button(FormatAction::Bold, "Bold", has_selection, cx))
            .child(self.format_button(FormatAction::Italic, "Italic", has_selection, cx))
            .child(self.format_button(FormatAction::Underline, "Underline", has_selection, cx))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(16.0))
                    .mx_1()
                    .bg(rgb(DARK_THEME.border)),
            )
            .child(
                self.format_button(
                    FormatAction::ParagraphStyle,
                    self.current_paragraph_style()
                        .unwrap_or_else(|| "Default style".to_owned())
                        .as_str(),
                    true,
                    cx,
                ),
            )
            .child(
                self.format_button(
                    FormatAction::CharacterStyle,
                    self.current_character_style()
                        .unwrap_or_else(|| "No character style".to_owned())
                        .as_str(),
                    has_selection,
                    cx,
                ),
            )
            .child(
                div()
                    .ml_auto()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!("{} words", body.word_count())),
            );

        let page_width =
            self.format.measure as f32 * (self.format.base_size * 0.62) + self.format.margin * 2.0;
        let lines = layout(body, &self.styles, self.format);
        let mut page = div()
            .id("manuscript-page")
            .w(px(page_width))
            .p(px(self.format.margin))
            .rounded_lg()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .flex()
            .flex_col();
        for line in &lines {
            page = page.child(self.render_line(line));
        }

        div()
            .id("manuscript-write")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(toolbar)
            .child(
                div()
                    .id("manuscript-page-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    .child(page),
            )
            .into_any_element()
    }

    fn format_button(
        &self,
        action: FormatAction,
        label: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(action.id())
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .cursor_pointer()
            .text_xs()
            .text_color(rgb(if enabled {
                DARK_THEME.text
            } else {
                DARK_THEME.muted_text
            }))
            .child(label.to_owned())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus.focus(window, cx);
                this.apply_format(action);
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_line(&self, line: &VisualLine) -> AnyElement {
        let split = line_layout(line, self.selection);
        let mut row = div()
            .h(px(self.format.line_height))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(0x202634))
            .text_color(rgb(DARK_THEME.text));
        let mut consumed = 0_usize;
        for piece in &split.pieces {
            let length = piece.text.chars().count();
            let caret_here = split
                .caret_at
                .is_some_and(|at| at >= consumed && at < consumed + length);
            if caret_here {
                let at = split.caret_at.unwrap_or_default() - consumed;
                let before: String = piece.text.chars().take(at).collect();
                let after: String = piece.text.chars().skip(at).collect();
                if !before.is_empty() {
                    row = row.child(span(&before, &piece.style, piece.selected));
                }
                row = row.child(caret_bar(self.format.line_height));
                if !after.is_empty() {
                    row = row.child(span(&after, &piece.style, piece.selected));
                }
            } else {
                row = row.child(span(&piece.text, &piece.style, piece.selected));
            }
            consumed += length;
        }
        if split.caret_at == Some(consumed) {
            row = row.child(caret_bar(self.format.line_height));
        }
        row.into_any_element()
    }

    fn persist(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        if let Err(error) = store.write_tree(&self.tree) {
            self.status = format!("Could not save the manuscript: {error}");
        }
    }

    fn refresh(&mut self) {
        self.projection =
            ManuscriptProjection::build(&self.tree, self.bodies.as_ref(), &self.options);
    }

    fn move_selection(&mut self, forward: bool) {
        let rows = self.projection.rows();
        if rows.is_empty() {
            return;
        }
        let current = rows.iter().position(|row| row.id == self.options.selected);
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(rows.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, _) => 0,
        };
        let id = rows[next].id;
        self.select(id);
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return;
        }
        if self.editing.is_some() {
            match event.keystroke.key.as_ref() {
                "escape" => self.cancel_rename(),
                "enter" => self.commit_rename(),
                "backspace" => {
                    self.buffer.pop();
                }
                _ => {
                    if let Some(character) = &event.keystroke.key_char {
                        self.buffer.push_str(&character.to_string());
                    } else {
                        return;
                    }
                }
            }
            cx.notify();
            return;
        }
        if self.mode == ManuscriptMode::Write && self.write_key(event) {
            cx.notify();
            return;
        }
        match event.keystroke.key.as_ref() {
            "up" => self.move_selection(false),
            "down" => self.move_selection(true),
            "escape" => self.set_filter(String::new()),
            "backspace" => {
                let mut filter = self.options.filter.clone();
                filter.pop();
                self.set_filter(filter);
            }
            _ => {
                if let Some(character) = &event.keystroke.key_char {
                    let mut filter = self.options.filter.clone();
                    filter.push_str(&character.to_string());
                    self.set_filter(filter);
                } else {
                    return;
                }
            }
        }
        cx.notify();
    }

    fn tree_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .id("manuscript-tree")
            .w(px(TREE_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .border_r_1()
            .border_color(rgb(DARK_THEME.border));

        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(rgb(DARK_THEME.muted_text))
                .child("Manuscript"),
        );

        let mut rows = div().id("manuscript-rows").flex().flex_col().gap_1();
        for row in self.projection.rows() {
            let id = row.id;
            let color = self
                .tree
                .chapter(id)
                .map_or(DARK_THEME.muted_text, |chapter| kind_color(chapter.kind));
            rows = rows.child(
                div()
                    .id(("manuscript-row", id.0 as usize))
                    .h(px(24.0))
                    .pl(px(8.0 + row.depth as f32 * 14.0))
                    .pr_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(row.is_selected, |element| {
                        element.bg(rgb(DARK_THEME.accent))
                    })
                    .when(self.drag_over_row == Some(id), |element| {
                        element
                            .border_1()
                            .border_color(rgb(DARK_THEME.accent))
                            .bg(rgb(DARK_THEME.panel))
                    })
                    .on_drag_move::<DragPayload>({
                        let weak = cx.weak_entity();
                        move |_, _, cx| {
                            weak.update(cx, |this, _| {
                                this.drag_gap = None;
                                this.drag_over_row = Some(id);
                            })
                            .ok();
                        }
                    })
                    .on_drop::<DragPayload>({
                        let weak = cx.weak_entity();
                        move |payload: &DragPayload, _, cx| {
                            weak.update(cx, |this, cx| {
                                this.drop_on_tree(payload, id);
                                cx.notify();
                            })
                            .ok();
                        }
                    })
                    .on_hover({
                        let weak = cx.weak_entity();
                        move |hovered: &bool, _, cx| {
                            if !*hovered {
                                weak.update(cx, |this, _| {
                                    if this.drag_over_row == Some(id) {
                                        this.drag_over_row = None;
                                    }
                                })
                                .ok();
                            }
                        }
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.select(id);
                        cx.notify();
                    }))
                    .child(div().size(px(6.0)).rounded_sm().bg(rgb(color)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(rgb(if row.is_selected {
                                DARK_THEME.workspace
                            } else {
                                DARK_THEME.text
                            }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(row.title.clone()),
                    )
                    .child(
                        div()
                            .id(("tree-status", id.0 as usize))
                            .max_w(px(62.0))
                            .px_1()
                            .rounded_sm()
                            .bg(rgb(DARK_THEME.panel))
                            .text_xs()
                            .text_color(rgb(if row.status.is_some() {
                                DARK_THEME.text
                            } else {
                                DARK_THEME.muted_text
                            }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(status_display(row.status.as_deref()).to_owned())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.focus.focus(window, cx);
                                this.cycle_status(id);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id(("tree-label", id.0 as usize))
                            .max_w(px(54.0))
                            .px_1()
                            .rounded_sm()
                            .bg(rgb(DARK_THEME.panel))
                            .text_xs()
                            .text_color(rgb(if row.label.is_some() {
                                DARK_THEME.text
                            } else {
                                DARK_THEME.muted_text
                            }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(label_display(row.label.as_deref()).to_owned())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.focus.focus(window, cx);
                                this.cycle_label(id);
                                cx.notify();
                            })),
                    ),
            );
        }
        panel
            .child(rows)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(self.status.clone()),
            )
            .into_any_element()
    }

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let filter_text = if self.options.filter.is_empty() {
            "Filter chapters…".to_owned()
        } else {
            self.options.filter.clone()
        };
        let mut header = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_2()
            .child(
                div()
                    .text_lg()
                    .text_color(rgb(DARK_THEME.text))
                    .child(self.projection.container_title().to_owned()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(format!(
                        "{} of {}",
                        self.projection.visible_count(),
                        self.projection.total_children()
                    )),
            );

        let mut modes = div().flex().gap_1();
        for mode in [
            ManuscriptMode::Board,
            ManuscriptMode::Table,
            ManuscriptMode::Settings,
            ManuscriptMode::Write,
        ] {
            let active = self.mode == mode;
            modes = modes.child(
                div()
                    .id(match mode {
                        ManuscriptMode::Board => "manuscript-mode-board",
                        ManuscriptMode::Table => "manuscript-mode-table",
                        ManuscriptMode::Settings => "manuscript-mode-settings",
                        ManuscriptMode::Write => "manuscript-mode-write",
                    })
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(if active {
                        DARK_THEME.text
                    } else {
                        DARK_THEME.muted_text
                    }))
                    .when(active, |element| element.bg(rgb(DARK_THEME.accent)))
                    .child(mode.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_mode(mode);
                        cx.notify();
                    })),
            );
        }
        header = header.child(modes);
        if matches!(self.mode, ManuscriptMode::Settings | ManuscriptMode::Write) {
            return header.into_any_element();
        }
        header = header.child(
            div()
                .id("manuscript-filter")
                .h(px(26.0))
                .px_2()
                .flex()
                .items_center()
                .rounded_md()
                .border_1()
                .border_color(rgb(DARK_THEME.border))
                .bg(rgb(DARK_THEME.chrome))
                .text_xs()
                .text_color(rgb(if self.options.filter.is_empty() {
                    DARK_THEME.muted_text
                } else {
                    DARK_THEME.text
                }))
                .cursor_text()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.focus.focus(window, cx);
                }))
                .child(filter_text),
        );

        if self.mode == ManuscriptMode::Table {
            let mut columns = div().flex().gap_1();
            for column in ALL_COLUMNS {
                let active = self.options.visible_columns.contains(&column);
                columns = columns.child(
                    div()
                        .id(match column {
                            Column::Status => "manuscript-column-status",
                            Column::Label => "manuscript-column-label",
                            Column::Kind => "manuscript-column-kind",
                        })
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(if active {
                            DARK_THEME.text
                        } else {
                            DARK_THEME.muted_text
                        }))
                        .when(active, |element| element.bg(rgb(DARK_THEME.panel)))
                        .child(column.label())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_column(column);
                            cx.notify();
                        })),
                );
            }
            header = header.child(columns);
        }

        header.into_any_element()
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.options.selected;
        let movable = !selected.is_reserved() && self.tree.chapter(selected).is_some();
        let trashed = self.selection_is_trashed();
        let mut bar = div()
            .id("manuscript-toolbar")
            .flex()
            .items_center()
            .flex_wrap()
            .gap_1();

        for verb in [
            MoveVerb::Up,
            MoveVerb::Down,
            MoveVerb::Promote,
            MoveVerb::Demote,
            MoveVerb::ToFront,
            MoveVerb::ToBack,
        ] {
            bar = bar.child(self.tool_button(ToolAction::Move(verb), verb.label(), movable, cx));
        }

        bar = bar.child(
            div()
                .w(px(1.0))
                .h(px(16.0))
                .mx_1()
                .bg(rgb(DARK_THEME.border)),
        );
        bar = bar.child(self.tool_button(ToolAction::Trash, "Trash", movable && !trashed, cx));
        bar = bar.child(self.tool_button(ToolAction::Restore, "Restore", trashed, cx));
        bar = bar.child(self.tool_button(
            ToolAction::EmptyTrash,
            "Empty Trash",
            !self.tree.child_ids(ChapterId::TRASH).is_empty(),
            cx,
        ));
        bar = bar.child(
            div()
                .w(px(1.0))
                .h(px(16.0))
                .mx_1()
                .bg(rgb(DARK_THEME.border)),
        );
        bar = bar.child(self.tool_button(ToolAction::Undo, "Undo", self.history.can_undo(), cx));
        bar.into_any_element()
    }

    fn tool_button(
        &self,
        action: ToolAction,
        label: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(action.id())
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(DARK_THEME.border))
            .bg(rgb(DARK_THEME.panel))
            .cursor_pointer()
            .text_xs()
            .text_color(rgb(if enabled {
                DARK_THEME.text
            } else {
                DARK_THEME.muted_text
            }))
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus.focus(window, cx);
                match action {
                    ToolAction::Move(verb) => this.apply_move(verb),
                    ToolAction::Trash => this.trash_selected(),
                    ToolAction::Restore => this.restore_selected(),
                    ToolAction::EmptyTrash => this.empty_trash(),
                    ToolAction::Undo => {
                        this.undo();
                    }
                }
                cx.notify();
            }))
            .into_any_element()
    }

    /// What the window status bar should show for the current selection.
    #[must_use]
    pub fn selection_context(&self) -> Option<SelectionContext> {
        let chapter = self.tree.chapter(self.options.selected)?;
        let statuses = self.tree.statuses_by_id();
        let labels = self.tree.labels_by_id();
        Some(SelectionContext {
            id: chapter.id,
            title: chapter.title.clone(),
            status: chapter
                .status_id
                .and_then(|id| statuses.get(&id))
                .map(|status| status.name.clone()),
            label: chapter
                .label_id
                .and_then(|id| labels.get(&id))
                .map(|label| label.name.clone()),
            trashed: chapter.parent_id == ChapterId::TRASH,
        })
    }

    /// Advance the selected chapter's status, for the window status bar.
    pub fn cycle_status_selected(&mut self) {
        let id = self.options.selected;
        self.cycle_status(id);
    }

    /// Advance the selected chapter's label, for the window status bar.
    pub fn cycle_label_selected(&mut self) {
        let id = self.options.selected;
        self.cycle_label(id);
    }

    fn begin_rename(&mut self, kind: DefinitionKind, id: i64) {
        let name = match kind {
            DefinitionKind::Status => self
                .tree
                .statuses
                .iter()
                .find(|status| status.id == id)
                .map(|status| status.name.clone()),
            DefinitionKind::Label => self
                .tree
                .labels
                .iter()
                .find(|label| label.id == id)
                .map(|label| label.name.clone()),
        };
        if let Some(name) = name {
            self.editing = Some(DefinitionEdit { kind, id });
            self.buffer = name;
        }
    }

    fn commit_rename(&mut self) {
        let Some(edit) = self.editing else {
            return;
        };
        let result = match edit.kind {
            DefinitionKind::Status => self.tree.rename_status(edit.id, &self.buffer),
            DefinitionKind::Label => self.tree.rename_label(edit.id, &self.buffer),
        };
        match result {
            Ok(()) => {
                self.editing = None;
                self.buffer.clear();
                self.persist();
                self.status = "Renamed".to_owned();
                self.refresh();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn cancel_rename(&mut self) {
        if self.editing.take().is_some() {
            self.buffer.clear();
            self.status = "Rename cancelled".to_owned();
        }
    }

    fn add_definition(&mut self, kind: DefinitionKind) {
        let added = match kind {
            DefinitionKind::Status => self.tree.add_status("New status").ok(),
            DefinitionKind::Label => self.tree.add_label("New label", [0.55, 0.36, 0.96]).ok(),
        };
        if let Some(id) = added {
            self.persist();
            self.refresh();
            self.begin_rename(kind, id);
        }
    }

    fn settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("manuscript-settings")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.definition_list("Statuses", DefinitionKind::Status, cx))
            .child(self.definition_list("Labels", DefinitionKind::Label, cx))
            .into_any_element()
    }

    fn definition_list(
        &self,
        title: &'static str,
        kind: DefinitionKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entries: Vec<(i64, String)> = match kind {
            DefinitionKind::Status => self
                .tree
                .statuses
                .iter()
                .map(|status| (status.id, status.name.clone()))
                .collect(),
            DefinitionKind::Label => self
                .tree
                .labels
                .iter()
                .map(|label| (label.id, label.name.clone()))
                .collect(),
        };
        let add_id = match kind {
            DefinitionKind::Status => "settings-add-status",
            DefinitionKind::Label => "settings-add-label",
        };
        let row_prefix = match kind {
            DefinitionKind::Status => "settings-status",
            DefinitionKind::Label => "settings-label",
        };
        let empty = match kind {
            DefinitionKind::Status => "No statuses defined",
            DefinitionKind::Label => "No labels defined",
        };

        let mut list = div().flex().flex_col().gap_1().child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(DARK_THEME.text))
                        .child(title),
                )
                .child(
                    div()
                        .id(add_id)
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(DARK_THEME.border))
                        .bg(rgb(DARK_THEME.panel))
                        .cursor_pointer()
                        .text_xs()
                        .child("Add")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.focus.focus(window, cx);
                            this.add_definition(kind);
                            cx.notify();
                        })),
                ),
        );

        if entries.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(rgb(DARK_THEME.muted_text))
                    .child(empty),
            );
        }

        for (id, name) in entries {
            let editing = self.editing == Some(DefinitionEdit { kind, id });
            let shown = if editing {
                format!("{}▏", self.buffer)
            } else {
                name
            };
            list = list.child(
                div()
                    .id((row_prefix, id as usize))
                    .h(px(26.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if editing {
                        DARK_THEME.accent
                    } else {
                        DARK_THEME.border
                    }))
                    .bg(rgb(DARK_THEME.panel))
                    .cursor_text()
                    .text_sm()
                    .text_color(rgb(DARK_THEME.text))
                    .child(shown)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.begin_rename(kind, id);
                        cx.notify();
                    })),
            );
        }

        list.into_any_element()
    }

    fn board(&self, cx: &mut Context<Self>) -> AnyElement {
        let width = self.projection.columns() as f32 * CARD_WIDTH
            + (self.projection.columns() as f32 - 1.0) * CARD_GAP;
        let weak = cx.weak_entity();
        let board = div()
            .id("manuscript-board")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3();

        if self.projection.is_empty() {
            return board
                .child(
                    div()
                        .id("manuscript-empty")
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child(self.projection.empty_state().to_owned()),
                )
                .into_any_element();
        }

        let gap_weak = weak.clone();
        let drop_weak = weak.clone();
        let mut grid = div()
            .id("manuscript-cards")
            .w(px(width))
            .flex()
            .flex_wrap()
            .gap(px(CARD_GAP))
            .on_drag_move::<DragPayload>(move |event, _, cx| {
                let x = f32::from(event.event.position.x - event.bounds.left());
                let y = f32::from(event.event.position.y - event.bounds.top());
                gap_weak
                    .update(cx, |this, _| {
                        this.drag_over_row = None;
                        let count = this.projection.cards().len();
                        this.drag_gap = drop_index((x, y), count, this.projection.columns());
                    })
                    .ok();
            })
            .on_drop::<DragPayload>(move |payload: &DragPayload, _, cx| {
                drop_weak
                    .update(cx, |this, cx| {
                        this.drop_on_board(payload);
                        cx.notify();
                    })
                    .ok();
            });

        for card in self.projection.cards() {
            let id = card.id;
            let payload = DragPayload {
                id,
                source: self.projection.container(),
                title: card.title.clone(),
                excerpt: card.excerpt.clone(),
            };
            let preview_weak = weak.clone();
            let accent = self
                .tree
                .chapter(id)
                .map_or(DARK_THEME.muted_text, |chapter| kind_color(chapter.kind));
            grid = grid.child(
                div()
                    .id(("manuscript-card", id.0 as usize))
                    .w(px(CARD_WIDTH))
                    .h(px(CARD_HEIGHT))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(DARK_THEME.border))
                    .bg(rgb(DARK_THEME.panel))
                    .cursor_pointer()
                    .when(self.dragging == Some(id), |element| {
                        element.border_color(rgb(DARK_THEME.accent))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.select(id);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(8.0)).rounded_sm().bg(rgb(accent)))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(DARK_THEME.text))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(card.title.clone()),
                            ),
                    )
                    .child(div().h(px(1.0)).w_full().bg(rgb(DARK_THEME.border)))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .line_clamp(5)
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(card.excerpt.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(DARK_THEME.muted_text))
                            .child(format!("{} words", card.word_count)),
                    )
                    .on_drag(payload, move |value: &DragPayload, _, _, cx| {
                        let id = value.id;
                        preview_weak
                            .update(cx, |this, _| this.dragging = Some(id))
                            .ok();
                        let title = value.title.clone();
                        let excerpt = value.excerpt.clone();
                        cx.new(|_| DragPreview { title, excerpt })
                    }),
            );
        }

        let mut surface = div().relative().w(px(width)).child(grid);
        if let (Some(gap), Some(_)) = (self.drag_gap, self.dragging) {
            let columns = self.projection.columns();
            let row = gap / columns;
            let column = gap % columns;
            let x = (column as f32 * (CARD_WIDTH + CARD_GAP) - CARD_GAP / 2.0).max(0.0);
            let y = row as f32 * (CARD_HEIGHT + CARD_GAP);
            surface = surface.child(
                div()
                    .id("manuscript-insertion-bar")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(2.0))
                    .h(px(CARD_HEIGHT))
                    .bg(rgb(DARK_THEME.accent)),
            );
        }
        board.child(surface).into_any_element()
    }

    fn table(&self, cx: &mut Context<Self>) -> AnyElement {
        let visible = self.projection.visible_columns().to_vec();
        let mut header_row = div()
            .flex()
            .items_center()
            .h(px(24.0))
            .border_b_1()
            .border_color(rgb(DARK_THEME.border))
            .text_xs()
            .text_color(rgb(DARK_THEME.muted_text))
            .child(div().flex_1().px_2().child("Document"));
        for column in &visible {
            header_row = header_row.child(
                div()
                    .w(px(column_width(*column)))
                    .px_2()
                    .child(column.label()),
            );
        }

        if self.projection.is_empty() {
            return div()
                .id("manuscript-table")
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_2()
                .child(header_row)
                .child(
                    div()
                        .id("manuscript-empty")
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(rgb(DARK_THEME.muted_text))
                        .child(self.projection.empty_state().to_owned()),
                )
                .into_any_element();
        }

        let range = self
            .projection
            .visible_row_range(self.scroll, self.viewport_rows() * ROW_HEIGHT);
        let before = range.start as f32 * ROW_HEIGHT;
        let after = (self.projection.visible_count() - range.end) as f32 * ROW_HEIGHT;

        let mut body = div().id("manuscript-table-rows").flex().flex_col();
        if before > 0.0 {
            body = body.child(div().h(px(before)));
        }
        for row in &self.projection.table_rows()[range.clone()] {
            let id = row.id;
            let mut line = div()
                .id(("manuscript-table-row", id.0 as usize))
                .flex()
                .items_center()
                .h(px(ROW_HEIGHT))
                .border_b_1()
                .border_color(rgb(DARK_THEME.border))
                .text_sm()
                .when(row.is_selected, |element| element.bg(rgb(DARK_THEME.panel)))
                .child(
                    div()
                        .id(("Document", id.0 as usize))
                        .flex_1()
                        .min_w_0()
                        .px_2()
                        .cursor_pointer()
                        .text_color(rgb(DARK_THEME.text))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(row.title.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.focus.focus(window, cx);
                            this.select(id);
                            cx.notify();
                        })),
                );

            for column in &visible {
                let text = match column {
                    Column::Status => row.status.clone().unwrap_or_else(|| "Unset".to_owned()),
                    Column::Label => row.label.clone().unwrap_or_else(|| "Unset".to_owned()),
                    Column::Kind => row.kind.to_owned(),
                };
                let muted = match column {
                    Column::Status => row.status.is_none(),
                    Column::Label => row.label.is_none(),
                    Column::Kind => false,
                };
                let column = *column;
                line = line.child(
                    div()
                        .id((column.label(), id.0 as usize))
                        .w(px(column_width(column)))
                        .px_2()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(if muted {
                            DARK_THEME.muted_text
                        } else {
                            DARK_THEME.text
                        }))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(text)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.focus.focus(window, cx);
                            match column {
                                Column::Status => this.cycle_status(id),
                                Column::Label => this.cycle_label(id),
                                Column::Kind => this.cycle_kind(id),
                            }
                            cx.notify();
                        })),
                );
            }
            body = body.child(line);
        }
        if after > 0.0 {
            body = body.child(div().h(px(after)));
        }

        div()
            .id("manuscript-table")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(header_row)
            .child(
                div()
                    .id("manuscript-table-viewport")
                    .flex_1()
                    .overflow_y_scroll()
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                        let delta = match event.delta {
                            ScrollDelta::Pixels(point) => f32::from(point.y),
                            ScrollDelta::Lines(point) => point.y * ROW_HEIGHT,
                        };
                        this.scroll = (this.scroll - delta).max(0.0);
                        cx.notify();
                    }))
                    .child(body),
            )
            .into_any_element()
    }

    fn viewport_rows(&self) -> f32 {
        (self.projection.visible_count() as f32).min(12.0)
    }
}

/// One toolbar command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolAction {
    /// A positional move.
    Move(MoveVerb),
    /// Move the selection to the trash.
    Trash,
    /// Restore the selection from the trash.
    Restore,
    /// Permanently remove everything in the trash.
    EmptyTrash,
    /// Undo the last structural change.
    Undo,
}

impl ToolAction {
    const fn id(self) -> &'static str {
        match self {
            Self::Move(MoveVerb::Up) => "manuscript-tool-up",
            Self::Move(MoveVerb::Down) => "manuscript-tool-down",
            Self::Move(MoveVerb::Promote) => "manuscript-tool-promote",
            Self::Move(MoveVerb::Demote) => "manuscript-tool-demote",
            Self::Move(MoveVerb::ToFront) => "manuscript-tool-front",
            Self::Move(MoveVerb::ToBack) => "manuscript-tool-back",
            Self::Trash => "manuscript-tool-trash",
            Self::Restore => "manuscript-tool-restore",
            Self::EmptyTrash => "manuscript-tool-empty-trash",
            Self::Undo => "manuscript-tool-undo",
        }
    }
}

/// One writing toolbar command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormatAction {
    Bold,
    Italic,
    Underline,
    ParagraphStyle,
    CharacterStyle,
}

impl FormatAction {
    const fn id(self) -> &'static str {
        match self {
            Self::Bold => "writer-bold",
            Self::Italic => "writer-italic",
            Self::Underline => "writer-underline",
            Self::ParagraphStyle => "writer-paragraph-style",
            Self::CharacterStyle => "writer-character-style",
        }
    }
}

fn next_option(options: &[Option<String>], current: Option<String>) -> Option<String> {
    let index = options
        .iter()
        .position(|option| *option == current)
        .map_or(0, |index| (index + 1) % options.len());
    options[index].clone()
}

fn clamp_selection(body: &ChapterBody, selection: Selection) -> Selection {
    let clamp = |position: Position| {
        let paragraph = position
            .paragraph
            .min(body.paragraphs.len().saturating_sub(1));
        let length = body
            .paragraphs
            .get(paragraph)
            .map_or(0, |paragraph| paragraph.text().chars().count());
        Position {
            paragraph,
            offset: position.offset.min(length),
        }
    };
    Selection {
        anchor: clamp(selection.anchor),
        caret: clamp(selection.caret),
    }
}

fn span(text: &str, style: &crate::manuscript::ResolvedStyle, selected: bool) -> AnyElement {
    div()
        .text_size(px(style.size * 1.3333))
        .font_family("Menlo")
        .when(style.bold, |element| {
            element.font_weight(gpui::FontWeight::BOLD)
        })
        .when(style.italic, |element| element.italic())
        .when(style.underline, |element| element.underline())
        .when(selected, |element| element.bg(rgb(0x334066)))
        .child(text.to_owned())
        .into_any_element()
}

fn caret_bar(line_height: f32) -> AnyElement {
    div()
        .w(px(2.0))
        .h(px(line_height * 0.75))
        .bg(rgb(DARK_THEME.accent))
        .into_any_element()
}

fn column_width(column: Column) -> f32 {
    match column {
        Column::Status => 110.0,
        Column::Label => 100.0,
        Column::Kind => 90.0,
    }
}

impl Render for ManuscriptSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dense = match self.mode {
            ManuscriptMode::Board => self.board(cx),
            ManuscriptMode::Table => self.table(cx),
            ManuscriptMode::Settings => self.settings_panel(cx),
            ManuscriptMode::Write => self.write_panel(cx),
        };
        div()
            .id("manuscript-surface")
            .key_context("WeaveManuscript")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .bg(rgb(DARK_THEME.workspace))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.key_down(event, cx);
            }))
            .child(self.tree_panel(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(self.header(cx))
                    .child(self.toolbar(cx))
                    .child(dense),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (ChapterTree, ChapterId, ChapterId) {
        let mut tree = ChapterTree::new();
        let part = tree
            .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Part One")
            .expect("part");
        let scene = tree
            .add_chapter(part, ChapterKind::Scene, "Opening")
            .expect("scene");
        (tree, part, scene)
    }

    #[test]
    fn rows_follow_tree_order_and_depth() {
        let (tree, part, scene) = tree();
        let projection = ManuscriptProjection::build(&tree, None, &ProjectionOptions::default());
        let rows = projection.rows();
        assert_eq!(rows[0].id, part);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].id, scene);
        assert_eq!(rows[1].depth, 1);
        assert!(rows[0].accessible_label.contains("Part One"));
        assert!(rows[1].accessible_label.contains("1 of 1"));
    }

    #[test]
    fn selecting_a_leaf_shows_its_parents_children() {
        let (tree, part, scene) = tree();
        let options = ProjectionOptions {
            selected: scene,
            ..ProjectionOptions::default()
        };
        let projection = ManuscriptProjection::build(&tree, None, &options);
        assert_eq!(projection.container(), part);
        assert_eq!(projection.container_title(), "Part One");
        assert_eq!(projection.cards().len(), 1);
        assert_eq!(projection.cards()[0].id, scene);
    }

    #[test]
    fn cards_are_numbered_and_name_their_ordinal() {
        let (mut tree, part, _scene) = tree();
        tree.add_chapter(part, ChapterKind::Scene, "Middle")
            .expect("middle");
        let options = ProjectionOptions {
            selected: part,
            ..ProjectionOptions::default()
        };
        let projection = ManuscriptProjection::build(&tree, None, &options);
        let cards = projection.cards();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].ordinal, 1);
        assert_eq!(cards[1].ordinal, 2);
        assert_eq!(cards[1].total, 2);
        assert_eq!(
            cards[1].accessible_label,
            "Scene (2 of 2 index cards): Middle"
        );
    }

    #[test]
    fn the_root_board_hides_the_trash() {
        let (tree, _part, _scene) = tree();
        let projection = ManuscriptProjection::build(&tree, None, &ProjectionOptions::default());
        assert!(
            projection
                .cards()
                .iter()
                .all(|card| card.id != ChapterId::TRASH)
        );
    }

    #[test]
    fn a_container_with_no_children_names_itself() {
        let (mut tree, _part, _scene) = tree();
        let empty = tree
            .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Empty Part")
            .expect("empty");
        let options = ProjectionOptions {
            selected: empty,
            ..ProjectionOptions::default()
        };
        let projection = ManuscriptProjection::build(&tree, None, &options);
        assert!(projection.is_empty());
        assert_eq!(projection.empty_state(), "Empty Part has no scenes yet");
        assert_eq!(
            projection.accessibility().empty_state.as_deref(),
            Some("Empty Part has no scenes yet")
        );
    }

    #[test]
    fn columns_are_clamped_to_a_usable_range() {
        let (tree, _part, _scene) = tree();
        let low = ProjectionOptions {
            columns: 0,
            ..ProjectionOptions::default()
        };
        assert_eq!(ManuscriptProjection::build(&tree, None, &low).columns(), 1);
        let high = ProjectionOptions {
            columns: 99,
            ..ProjectionOptions::default()
        };
        assert_eq!(ManuscriptProjection::build(&tree, None, &high).columns(), 6);
    }

    #[test]
    fn an_excerpt_is_truncated_on_a_word_boundary() {
        let long = "word ".repeat(200);
        let excerpt = excerpt_of(&long);
        assert!(excerpt.chars().count() <= EXCERPT_CHARS + 1);
        assert!(excerpt.ends_with('…'));
        assert!(!excerpt.contains("wor…"));
    }

    #[test]
    fn the_table_and_the_board_show_the_same_filtered_set() {
        let mut tree = ChapterTree::new();
        let part = tree
            .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Part One")
            .expect("part");
        for title in ["Opening", "The Gate", "Midnight"] {
            tree.add_chapter(part, ChapterKind::Scene, title)
                .expect("scene");
        }
        let options = ProjectionOptions {
            selected: part,
            filter: "the".to_owned(),
            ..ProjectionOptions::default()
        };
        let projection = ManuscriptProjection::build(&tree, None, &options);

        assert_eq!(projection.total_children(), 3);
        assert_eq!(projection.visible_count(), 1);
        assert_eq!(projection.cards().len(), 1);
        assert_eq!(projection.table_rows().len(), 1);
        assert_eq!(projection.cards()[0].id, projection.table_rows()[0].id);
        assert_eq!(projection.cards()[0].title, "The Gate");
    }

    #[test]
    fn a_filter_with_no_matches_reports_it() {
        let (tree, part, _scene) = tree();
        let options = ProjectionOptions {
            selected: part,
            filter: "zzz".to_owned(),
            ..ProjectionOptions::default()
        };
        let projection = ManuscriptProjection::build(&tree, None, &options);
        assert!(projection.is_empty());
        assert!(projection.empty_state().contains("zzz"));
        assert_eq!(projection.visible_count(), 0);
    }

    #[test]
    fn visible_rows_stay_proportional_to_the_viewport() {
        let mut tree = ChapterTree::new();
        let part = tree
            .add_chapter(ChapterId::ROOT, ChapterKind::Chapter, "Part One")
            .expect("part");
        for index in 0..200 {
            tree.add_chapter(part, ChapterKind::Scene, &format!("Scene {index}"))
                .expect("scene");
        }
        let options = ProjectionOptions {
            selected: part,
            ..ProjectionOptions::default()
        };
        let projection = ManuscriptProjection::build(&tree, None, &options);
        assert_eq!(projection.table_rows().len(), 200);

        let range = projection.visible_row_range(0.0, ROW_HEIGHT * 10.0);
        assert_eq!(range.start, 0);
        assert!(
            range.len() <= 16,
            "rendered {} rows for a 10-row viewport",
            range.len()
        );
        assert!(range.end < 200);

        let middle = projection.visible_row_range(ROW_HEIGHT * 100.0, ROW_HEIGHT * 10.0);
        assert!(middle.start >= 100 - OVERSCAN);
        assert!(middle.end - middle.start <= 16);

        let end = projection.visible_row_range(ROW_HEIGHT * 1_000.0, ROW_HEIGHT * 10.0);
        assert_eq!(end.end, 200);
    }

    #[test]
    fn a_column_can_be_hidden_and_restored() {
        let columns = normalize_columns(&[Column::Label, Column::Status, Column::Label]);
        assert_eq!(columns, vec![Column::Status, Column::Label]);
        assert_eq!(normalize_columns(&[]), Vec::<Column>::new());
    }

    #[test]
    fn status_label_and_kind_cycle_and_wrap() {
        let (mut tree, part, _scene) = tree();
        tree.add_status("Draft").expect("status");
        tree.add_label("POV", [0.5, 0.5, 0.5]).expect("label");
        let id = part;

        assert!(tree.chapter(id).expect("chapter").status_id.is_none());
        assert!(cycle_status(&mut tree, id));
        assert!(tree.chapter(id).expect("chapter").status_id.is_some());
        assert!(cycle_status(&mut tree, id));
        assert!(tree.chapter(id).expect("chapter").status_id.is_none());

        assert!(cycle_label(&mut tree, id));
        assert!(tree.chapter(id).expect("chapter").label_id.is_some());
        assert!(cycle_label(&mut tree, id));
        assert!(tree.chapter(id).expect("chapter").label_id.is_none());

        // Kind cycling runs on a leaf: a container holding scenes cannot
        // become a leaf, which the next test covers.
        let leaf = tree.child_ids(part)[0];
        assert_eq!(tree.chapter(leaf).expect("leaf").kind, ChapterKind::Scene);
        assert!(cycle_kind(&mut tree, leaf));
        assert_eq!(tree.chapter(leaf).expect("leaf").kind, ChapterKind::Note);
        assert!(cycle_kind(&mut tree, leaf));
        assert_eq!(tree.chapter(leaf).expect("leaf").kind, ChapterKind::Chapter);
        assert!(cycle_kind(&mut tree, leaf));
        assert_eq!(tree.chapter(leaf).expect("leaf").kind, ChapterKind::Scene);
    }

    #[test]
    fn an_unset_status_and_label_are_named_not_blank() {
        assert_eq!(status_display(None), "Unset status");
        assert_eq!(label_display(None), "Unset label");
        assert_eq!(status_display(Some("Draft")), "Draft");
    }

    #[test]
    fn the_projection_resolves_status_and_label_names() {
        let (mut tree, part, _scene) = tree();
        let status = tree.add_status("Revised").expect("status");
        let label = tree.add_label("POV", [0.5, 0.5, 0.5]).expect("label");
        tree.set_status(part, Some(status)).expect("set status");
        tree.set_label(part, Some(label)).expect("set label");

        let projection = ManuscriptProjection::build(&tree, None, &ProjectionOptions::default());
        let card = projection
            .cards()
            .iter()
            .find(|card| card.id == part)
            .expect("card");
        assert_eq!(card.status.as_deref(), Some("Revised"));
        assert_eq!(card.label.as_deref(), Some("POV"));

        let row = projection
            .rows()
            .iter()
            .find(|row| row.id == part)
            .expect("row");
        assert_eq!(row.status.as_deref(), Some("Revised"));
        assert!(
            row.accessible_label.contains("Revised"),
            "the row should announce its status, got {:?}",
            row.accessible_label
        );
    }

    #[test]
    fn the_nearest_gap_rounds_to_one_insertion_point() {
        let stride = CARD_WIDTH + CARD_GAP;
        assert_eq!(nearest_gap(-20.0, 3, stride), 0);
        assert_eq!(nearest_gap(10.0, 3, stride), 0);
        assert_eq!(nearest_gap(CARD_WIDTH - 4.0, 3, stride), 1);
        assert_eq!(nearest_gap(stride - CARD_GAP / 2.0, 3, stride), 1);
        assert_eq!(nearest_gap(stride * 3.0 + 50.0, 3, stride), 3);
    }

    #[test]
    fn the_drop_index_wraps_and_disappears_past_the_last_card() {
        let stride = CARD_WIDTH + CARD_GAP;
        assert_eq!(drop_index((10.0, 10.0), 5, 3), Some(0));
        assert_eq!(drop_index((stride * 2.0, 10.0), 5, 3), Some(2));
        assert_eq!(
            drop_index((10.0, CARD_HEIGHT + CARD_GAP + 10.0), 5, 3),
            Some(3)
        );
        assert_eq!(
            drop_index((10.0, (CARD_HEIGHT + CARD_GAP) * 4.0), 5, 3),
            None
        );
    }

    #[test]
    fn a_chapter_cannot_be_dropped_into_its_own_descendant() {
        let (tree, part, scene) = tree();
        assert!(!can_reparent(&tree, part, part));
        assert!(!can_reparent(&tree, part, scene));
        assert!(!can_reparent(&tree, scene, scene));
        assert!(can_reparent(&tree, scene, part));
    }

    #[test]
    fn the_payload_reports_whether_it_is_local() {
        let payload = DragPayload {
            id: ChapterId(2),
            source: ChapterId(3),
            title: "Opening".to_owned(),
            excerpt: "The lantern guttered".to_owned(),
        };
        assert!(payload.is_local(ChapterId(3)));
        assert!(!payload.is_local(ChapterId::ROOT));
    }

    #[test]
    fn a_container_holding_scenes_cannot_cycle_into_a_leaf() {
        let (mut tree, part, _scene) = tree();
        assert!(!cycle_kind(&mut tree, part));
        assert_eq!(
            tree.chapter(part).expect("chapter").kind,
            ChapterKind::Chapter
        );
    }
}
