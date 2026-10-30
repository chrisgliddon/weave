//! Ordered manuscript navigation: the chapter tree and the card board.
//!
//! Both surfaces read one projection of the same ordered child list, so a
//! selection cannot disagree between them and a later table can join them
//! without a second data source. The projection is plain data and is tested
//! without a window; the surface only draws it.

use std::path::Path;

use gpui::{
    AnyElement, Context, FocusHandle, KeyDownEvent, Render, Window, div, prelude::*, px, rgb,
};

use crate::manuscript::{BodyStore, ChapterId, ChapterKind, ChapterTree, ManuscriptStore};
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
/// Maximum characters of body text a card shows before truncating.
const EXCERPT_CHARS: usize = 220;

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

/// Everything the tree and the board draw, derived from one child list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManuscriptProjection {
    rows: Vec<TreeRow>,
    cards: Vec<CardRecord>,
    selected: ChapterId,
    container: ChapterId,
    container_title: String,
    columns: usize,
    empty_state: String,
}

impl ManuscriptProjection {
    /// Build the projection for a selection.
    ///
    /// `bodies` is optional so the projection can be built for a project with
    /// no manuscript on disk. Only the selected container's children are read.
    #[must_use]
    pub fn build(
        tree: &ChapterTree,
        bodies: Option<&BodyStore>,
        selected: ChapterId,
        columns: usize,
    ) -> Self {
        let columns = columns.clamp(1, 6);
        let selected = tree
            .chapter(selected)
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

        let mut rows = Vec::new();
        collect_rows(tree, ChapterId::ROOT, 0, selected, &mut rows);

        let mut children: Vec<&crate::manuscript::Chapter> = tree
            .children(container)
            .into_iter()
            .filter(|chapter| !(container == ChapterId::ROOT && chapter.id == ChapterId::TRASH))
            .collect();

        // The trash is a container like any other, so trashed chapters stay
        // visible and restorable from the board.
        children.sort_by_key(|chapter| chapter.order);

        let total = children.len();
        let mut cards = Vec::with_capacity(total);
        for (index, chapter) in children.into_iter().enumerate() {
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
            let ordinal = index + 1;
            cards.push(CardRecord {
                id: chapter.id,
                title: chapter.title.clone(),
                kind: chapter.kind.label(),
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
        }

        let empty_state = if container == ChapterId::ROOT {
            format!("{container_title} has no chapters yet")
        } else {
            format!("{container_title} has no scenes yet")
        };

        Self {
            rows,
            cards,
            selected,
            container,
            container_title,
            columns,
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

    /// Current selection.
    #[must_use]
    pub const fn selected(&self) -> ChapterId {
        self.selected
    }

    /// Container whose children the board shows.
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

    /// Text shown when the container has no children.
    #[must_use]
    pub fn empty_state(&self) -> &str {
        &self.empty_state
    }

    /// Whether the container has no children.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
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
    /// Title of the container the board shows.
    pub container: String,
    /// Empty-state text, when the container has no children.
    pub empty_state: Option<String>,
}

fn collect_rows(
    tree: &ChapterTree,
    parent: ChapterId,
    depth: usize,
    selected: ChapterId,
    rows: &mut Vec<TreeRow>,
) {
    let children = tree.children(parent);
    let total = children.len();
    for (index, chapter) in children.into_iter().enumerate() {
        let has_children = !tree.child_ids(chapter.id).is_empty();
        rows.push(TreeRow {
            id: chapter.id,
            depth,
            title: chapter.title.clone(),
            kind: chapter.kind.label(),
            has_children,
            is_selected: chapter.id == selected,
            accessible_label: format!(
                "{}, {}, {} of {}",
                chapter.kind.label(),
                chapter.title,
                index + 1,
                total
            ),
        });
        collect_rows(tree, chapter.id, depth + 1, selected, rows);
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

fn kind_color(kind: ChapterKind) -> u32 {
    match kind {
        ChapterKind::Chapter => DARK_THEME.accent,
        ChapterKind::Scene => DARK_THEME.muted_text,
        ChapterKind::Note => DARK_THEME.warning,
    }
}

/// Interactive chapter tree and card board.
pub struct ManuscriptSurface {
    tree: ChapterTree,
    bodies: Option<BodyStore>,
    selected: ChapterId,
    columns: usize,
    projection: ManuscriptProjection,
    status: String,
    focus: FocusHandle,
}

impl ManuscriptSurface {
    /// Create an empty surface before a project is loaded.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let tree = ChapterTree::new();
        let projection = ManuscriptProjection::build(&tree, None, ChapterId::ROOT, DEFAULT_COLUMNS);
        Self {
            tree,
            bodies: None,
            selected: ChapterId::ROOT,
            columns: DEFAULT_COLUMNS,
            projection,
            status: "Open a project to read its manuscript".to_owned(),
            focus: cx.focus_handle(),
        }
    }

    /// Read the manuscript beside a project directory, if there is one.
    pub fn load(&mut self, directory: &Path) {
        let store = ManuscriptStore::in_directory(directory.to_path_buf());
        if !store.exists() {
            self.tree = ChapterTree::new();
            self.bodies = None;
            self.selected = ChapterId::ROOT;
            self.status = "This project has no manuscript".to_owned();
            self.refresh();
            return;
        }
        match store.read_tree() {
            Ok(tree) => {
                self.selected = ChapterId::ROOT;
                self.bodies = Some(store.bodies().clone());
                self.tree = tree;
                self.status = format!("{} items", self.tree.len().saturating_sub(2));
            }
            Err(error) => {
                self.tree = ChapterTree::new();
                self.bodies = None;
                self.status = format!("Manuscript could not be read: {error}");
            }
        }
        self.refresh();
    }

    /// Reset to the empty manuscript shown before a project is opened.
    pub fn clear(&mut self) {
        self.tree = ChapterTree::new();
        self.bodies = None;
        self.selected = ChapterId::ROOT;
        self.status = "Open a project to read its manuscript".to_owned();
        self.refresh();
    }

    /// Select a chapter and rebuild the board.
    pub fn select(&mut self, selected: ChapterId) {
        if self.tree.chapter(selected).is_some() {
            self.selected = selected;
            self.refresh();
        }
    }

    /// Set how many columns the board lays out.
    pub fn set_columns(&mut self, columns: usize) {
        self.columns = columns.clamp(1, 6);
        self.refresh();
    }

    /// Current projection.
    #[must_use]
    pub const fn projection(&self) -> &ManuscriptProjection {
        &self.projection
    }

    /// Accessible names for the current projection.
    #[must_use]
    pub fn accessibility(&self) -> ManuscriptAccessibility {
        self.projection.accessibility()
    }

    fn refresh(&mut self) {
        self.projection = ManuscriptProjection::build(
            &self.tree,
            self.bodies.as_ref(),
            self.selected,
            self.columns,
        );
    }

    fn move_selection(&mut self, forward: bool) {
        let rows = self.projection.rows();
        if rows.is_empty() {
            return;
        }
        let current = rows.iter().position(|row| row.id == self.selected);
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(rows.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, _) => 0,
        };
        self.select(rows[next].id);
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "up" => self.move_selection(false),
            "down" => self.move_selection(true),
            _ => return,
        }
        cx.notify();
    }

    fn tree_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .id("manuscript-tree")
            .w(px(260.0))
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.select(id);
                        cx.notify();
                    }))
                    .child(div().size(px(6.0)).rounded_sm().bg(rgb(color)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(if row.is_selected {
                                DARK_THEME.workspace
                            } else {
                                DARK_THEME.text
                            }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(row.title.clone()),
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

    fn board(&self, cx: &mut Context<Self>) -> AnyElement {
        let width = self.columns as f32 * CARD_WIDTH + (self.columns as f32 - 1.0) * CARD_GAP;
        let mut board = div()
            .id("manuscript-board")
            .flex_1()
            .min_w_0()
            .h_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_3();

        board = board.child(
            div()
                .flex()
                .items_center()
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
                        .child(format!("{} cards", self.projection.cards().len())),
                ),
        );

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

        let mut grid = div()
            .id("manuscript-cards")
            .w(px(width))
            .flex()
            .flex_wrap()
            .gap(px(CARD_GAP));

        for card in self.projection.cards() {
            let id = card.id;
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
                    ),
            );
        }

        board.child(grid).into_any_element()
    }
}

impl Render for ManuscriptSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.board(cx))
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
        let projection = ManuscriptProjection::build(&tree, None, ChapterId::ROOT, DEFAULT_COLUMNS);
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
        let projection = ManuscriptProjection::build(&tree, None, scene, DEFAULT_COLUMNS);
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
        let projection = ManuscriptProjection::build(&tree, None, part, DEFAULT_COLUMNS);
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
        let projection = ManuscriptProjection::build(&tree, None, ChapterId::ROOT, DEFAULT_COLUMNS);
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
        let projection = ManuscriptProjection::build(&tree, None, empty, DEFAULT_COLUMNS);
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
        assert_eq!(
            ManuscriptProjection::build(&tree, None, ChapterId::ROOT, 0).columns(),
            1
        );
        assert_eq!(
            ManuscriptProjection::build(&tree, None, ChapterId::ROOT, 99).columns(),
            6
        );
    }

    #[test]
    fn an_excerpt_is_truncated_on_a_word_boundary() {
        let long = "word ".repeat(200);
        let excerpt = excerpt_of(&long);
        assert!(excerpt.chars().count() <= EXCERPT_CHARS + 1);
        assert!(excerpt.ends_with('…'));
        assert!(!excerpt.contains("wor…"));
    }
}
