//! Pure outline model for the story graph.
//!
//! [`Outline::build`] mirrors a parsed [`Document`] as a flat, indented list.
//! Knots appear in document order. Each knot is followed by the choices that
//! belong directly to it, one level deeper. Diverts and threads are not rows:
//! they surface as the target of the choice that reaches them.

use weave_core::ast::{Choice, Document, Spanned, Statement};

/// Maximum number of characters kept in a knot's prose preview.
pub const PREVIEW_CHARS: usize = 80;

/// Depth of a knot row.
pub const KNOT_DEPTH: u8 = 0;

/// Depth of a choice row, one level under its knot.
pub const CHOICE_DEPTH: u8 = 1;

/// Reason the outline refuses to reorder or nest rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReorderBlocked {
    /// Arbitrary nesting changes the story graph, so it needs an explicit graph edit.
    RequiresGraphEdit,
}

impl ReorderBlocked {
    /// Explanation the UI can show to the author.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::RequiresGraphEdit => {
                "Reordering or nesting blocks changes the story graph; apply an explicit graph edit instead."
            }
        }
    }
}

impl std::fmt::Display for ReorderBlocked {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.reason())
    }
}

/// Reason value the outline reports when a reorder or nesting request is refused.
pub const REORDER_BLOCKED: ReorderBlocked = ReorderBlocked::RequiresGraphEdit;

/// Content of one outline row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlineRowKind {
    /// A knot, with its prose preview and the number of choice rows under it.
    Knot {
        /// Short preview of the knot's first prose statement.
        prose: Option<String>,
        /// Number of choice rows listed under this knot.
        choices: usize,
    },
    /// A choice, with its author-visible label and resolved target.
    Choice {
        /// Author-visible choice label.
        label: String,
        /// Divert or thread target reached by this choice, when there is one.
        target: Option<String>,
    },
}

/// One row in the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    /// Stable graph node id: `knot:<name>` or `choice:<knot>:<dotted path>`.
    pub node_id: String,
    /// Row content.
    pub kind: OutlineRowKind,
    /// Indentation level: [`KNOT_DEPTH`] or [`CHOICE_DEPTH`].
    pub depth: u8,
    /// Statement path inside the owning knot; empty for knots.
    pub path: Vec<usize>,
}

/// Indented list view of a story document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    rows: Vec<OutlineRow>,
    selected: Option<usize>,
}

impl Outline {
    /// Build the outline from a document, in document order.
    #[must_use]
    pub fn build(document: &Document) -> Self {
        let mut rows = Vec::new();
        for knot in document.knots() {
            let choices = direct_choices(&knot.node.body);
            rows.push(OutlineRow {
                node_id: knot_node_id(&knot.node.name),
                kind: OutlineRowKind::Knot {
                    prose: preview(&knot.node.body),
                    choices: choices.len(),
                },
                depth: KNOT_DEPTH,
                path: Vec::new(),
            });
            for choice in choices {
                rows.push(OutlineRow {
                    node_id: choice_node_id(&knot.node.name, &choice.path),
                    kind: OutlineRowKind::Choice {
                        label: choice.label,
                        target: choice.target,
                    },
                    depth: CHOICE_DEPTH,
                    path: choice.path,
                });
            }
        }
        Self {
            rows,
            selected: None,
        }
    }

    /// All rows in display order.
    #[must_use]
    pub fn rows(&self) -> &[OutlineRow] {
        &self.rows
    }

    /// Row index of the current selection.
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Currently selected row, if any.
    #[must_use]
    pub fn selected_row(&self) -> Option<&OutlineRow> {
        self.selected.and_then(|index| self.rows.get(index))
    }

    /// Row index for a graph node id, if the outline lists it.
    #[must_use]
    pub fn index_of_node(&self, node_id: &str) -> Option<usize> {
        self.rows.iter().position(|row| row.node_id == node_id)
    }

    /// Select the row with `node_id`, preserving the selection when it is missing.
    pub fn select_node(&mut self, node_id: &str) -> bool {
        match self.index_of_node(node_id) {
            Some(index) => {
                self.selected = Some(index);
                true
            }
            None => false,
        }
    }

    /// Select the knot named `name`, preserving the selection when it is missing.
    pub fn select_knot(&mut self, name: &str) -> bool {
        self.select_node(&knot_node_id(name))
    }

    /// Select the choice at `path` inside the knot named `knot`.
    ///
    /// The selection is preserved when no choice row matches `path`.
    pub fn select_choice(&mut self, knot: &str, path: &[usize]) -> bool {
        self.select_node(&choice_node_id(knot, path))
    }

    /// Select the next row and return its index.
    ///
    /// With no selection this selects the first row. At the last row the
    /// selection stays there. An empty outline keeps no selection.
    pub fn select_next(&mut self) -> Option<usize> {
        let last = self.rows.len().checked_sub(1)?;
        let index = match self.selected {
            Some(index) => (index + 1).min(last),
            None => 0,
        };
        self.selected = Some(index);
        Some(index)
    }

    /// Select the previous row and return its index.
    ///
    /// With no selection this selects the last row. At the first row the
    /// selection stays there. An empty outline keeps no selection.
    pub fn select_previous(&mut self) -> Option<usize> {
        let last = self.rows.len().checked_sub(1)?;
        let index = match self.selected {
            Some(index) => index.saturating_sub(1),
            None => last,
        };
        self.selected = Some(index);
        Some(index)
    }
}

/// Choice data collected while walking one knot.
struct ChoiceSummary {
    label: String,
    path: Vec<usize>,
    target: Option<String>,
}

/// Return the choices written directly in a knot body, in document order.
fn direct_choices(body: &[Spanned<Statement>]) -> Vec<ChoiceSummary> {
    body.iter()
        .enumerate()
        .filter_map(|(index, statement)| match &statement.node {
            Statement::Choice(choice) => Some(ChoiceSummary {
                label: choice.text.trim().to_owned(),
                path: vec![index],
                target: choice_target(choice),
            }),
            _ => None,
        })
        .collect()
}

/// Resolve the target of a choice: an explicit divert, then its first divert or thread.
fn choice_target(choice: &Choice) -> Option<String> {
    if let Some(target) = &choice.divert {
        return Some(target.clone());
    }
    choice
        .body
        .iter()
        .find_map(|statement| match &statement.node {
            Statement::Divert(target) | Statement::Thread(target) => Some(target.clone()),
            _ => None,
        })
}

/// Build the node id for a knot.
fn knot_node_id(name: &str) -> String {
    format!("knot:{name}")
}

/// Build the node id for a choice from its knot and dotted statement path.
fn choice_node_id(knot: &str, path: &[usize]) -> String {
    format!("choice:{knot}:{}", dotted_path(path))
}

/// Render a statement path as dot-separated indices.
fn dotted_path(path: &[usize]) -> String {
    let mut rendered = String::new();
    for (index, part) in path.iter().enumerate() {
        if index > 0 {
            rendered.push('.');
        }
        rendered.push_str(&part.to_string());
    }
    rendered
}

/// Take the first non-empty prose statement as a bounded preview.
fn preview(body: &[Spanned<Statement>]) -> Option<String> {
    body.iter().find_map(|statement| match &statement.node {
        Statement::Text(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(truncate_preview(trimmed))
            }
        }
        _ => None,
    })
}

/// Keep at most [`PREVIEW_CHARS`] characters, marking a longer source.
fn truncate_preview(text: &str) -> String {
    let mut characters = text.chars();
    let mut preview: String = characters.by_ref().take(PREVIEW_CHARS).collect();
    if characters.next().is_some() {
        preview.push('…');
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::{
        CHOICE_DEPTH, KNOT_DEPTH, Outline, OutlineRowKind, REORDER_BLOCKED, ReorderBlocked,
    };
    use weave_core::ast::Document;
    use weave_core::parse_document;

    const STORY: &str = r#"=== start ===
The road forks at the old oak.
* [Take the forest road]
    You follow the ferns.
    -> forest
+ [Wait by the signpost]
    The wind moves.
    <- rest
=== forest ===
The old gate opens.
-> END
=== rest ===
You rest.
-> END
"#;

    fn story() -> Document {
        parse_document(STORY).expect("test story parses")
    }

    #[test]
    fn outline_lists_knots_in_document_order() {
        let outline = Outline::build(&story());
        let knots = outline
            .rows()
            .iter()
            .filter(|row| matches!(row.kind, OutlineRowKind::Knot { .. }))
            .map(|row| row.node_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(knots, ["knot:start", "knot:forest", "knot:rest"]);
    }

    #[test]
    fn outline_nests_choices_under_their_knot_with_paths_and_depth() {
        let outline = Outline::build(&story());
        let rows = outline.rows();

        assert_eq!(rows[0].node_id, "knot:start");
        assert_eq!(rows[0].depth, KNOT_DEPTH);
        assert_eq!(
            rows[0].kind,
            OutlineRowKind::Knot {
                prose: Some("The road forks at the old oak.".to_owned()),
                choices: 2,
            }
        );

        assert_eq!(rows[1].node_id, "choice:start:1");
        assert_eq!(rows[1].depth, CHOICE_DEPTH);
        assert_eq!(rows[1].path, [1]);
        assert_eq!(rows[2].node_id, "choice:start:2");
        assert_eq!(rows[2].depth, CHOICE_DEPTH);
        assert_eq!(rows[2].path, [2]);

        assert_eq!(
            rows[3].kind,
            OutlineRowKind::Knot {
                prose: Some("The old gate opens.".to_owned()),
                choices: 0,
            }
        );
        assert_eq!(rows[4].node_id, "knot:rest");
    }

    #[test]
    fn outline_choice_rows_carry_labels_and_divert_or_thread_targets() {
        let outline = Outline::build(&story());
        assert_eq!(
            outline.rows()[1].kind,
            OutlineRowKind::Choice {
                label: "Take the forest road".to_owned(),
                target: Some("forest".to_owned()),
            }
        );
        assert_eq!(
            outline.rows()[2].kind,
            OutlineRowKind::Choice {
                label: "Wait by the signpost".to_owned(),
                target: Some("rest".to_owned()),
            }
        );
    }

    #[test]
    fn outline_resolves_node_ids_for_knots_and_choices() {
        let mut outline = Outline::build(&story());

        assert_eq!(outline.index_of_node("knot:start"), Some(0));
        assert_eq!(outline.index_of_node("choice:start:2"), Some(2));
        assert_eq!(outline.index_of_node("choice:start:9"), None);
        assert_eq!(outline.index_of_node("knot:missing"), None);

        assert!(outline.select_node("knot:forest"));
        assert_eq!(outline.selected(), Some(3));
        assert_eq!(
            outline.selected_row().map(|row| row.node_id.as_str()),
            Some("knot:forest")
        );

        assert!(outline.select_choice("start", &[2]));
        assert_eq!(outline.selected(), Some(2));
        assert!(outline.select_knot("rest"));
        assert_eq!(outline.selected(), Some(4));
    }

    #[test]
    fn outline_navigation_stops_at_both_ends() {
        let mut outline = Outline::build(&story());

        assert_eq!(outline.selected(), None);
        assert_eq!(outline.select_next(), Some(0));
        assert_eq!(outline.select_previous(), Some(0));

        for _ in 0..20 {
            let _ = outline.select_next();
        }
        assert_eq!(outline.selected(), Some(4));
        assert_eq!(outline.select_next(), Some(4));

        for _ in 0..20 {
            let _ = outline.select_previous();
        }
        assert_eq!(outline.selected(), Some(0));
        assert_eq!(outline.select_previous(), Some(0));

        let mut fresh = Outline::build(&story());
        assert_eq!(fresh.select_previous(), Some(4));
    }

    #[test]
    fn outline_selecting_a_missing_node_returns_false_and_keeps_selection() {
        let mut outline = Outline::build(&story());
        assert!(outline.select_node("knot:rest"));

        assert!(!outline.select_node("knot:missing"));
        assert!(!outline.select_knot("missing"));
        assert!(!outline.select_choice("start", &[9]));
        assert!(!outline.select_choice("missing", &[0]));
        assert_eq!(outline.selected(), Some(4));
    }

    #[test]
    fn outline_empty_document_has_no_rows_and_does_not_panic() {
        let mut outline = Outline::build(&Document::default());

        assert!(outline.rows().is_empty());
        assert_eq!(outline.selected(), None);
        assert_eq!(outline.selected_row(), None);
        assert_eq!(outline.select_next(), None);
        assert_eq!(outline.select_previous(), None);
        assert!(!outline.select_node("knot:start"));
        assert!(!outline.select_knot("start"));
        assert!(!outline.select_choice("start", &[0]));
    }

    #[test]
    fn outline_reorder_blocked_explains_the_graph_edit_requirement() {
        assert_eq!(REORDER_BLOCKED, ReorderBlocked::RequiresGraphEdit);
        assert!(REORDER_BLOCKED.reason().contains("explicit graph edit"));
        assert_eq!(
            REORDER_BLOCKED.to_string(),
            ReorderBlocked::RequiresGraphEdit.reason()
        );
    }
}
